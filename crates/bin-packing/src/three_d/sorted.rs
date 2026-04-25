//! Volume-sorted First-Fit-Decreasing (FFD) and Best-Fit-Decreasing (BFD)
//! heuristics built on top of the Task 6 Extreme Points placement engine.
//!
//! Both variants:
//!
//! 1. Expand demands into per-quantity [`ItemInstance3D`] entries.
//! 2. Sort that list by decreasing item volume.
//! 3. Walk the sorted list and drive the shared EP placement engine in
//!    [`super::extreme_points`] via [`try_place_into_bin`], reusing the
//!    same multi-bin bookkeeping (declaration-order open bins, best-fit
//!    bin-type opening by volume, [`Bin3D::quantity`] caps) as the EP
//!    entry points.
//!
//! The only difference between the two variants is the per-item bin
//! selection policy:
//!
//! - **FFD** commits the first open bin that accepts the item under the
//!   EP engine's `VolumeFitResidual` scoring. This is the fastest variant
//!   and matches the textbook "first fit" semantics: open bins are never
//!   reconsidered once skipped.
//! - **BFD** evaluates *every* open bin. For each candidate, a snapshot
//!   of the bin's mutable state is taken, `try_place_into_bin` is run on
//!   the snapshot, and the resulting `used_volume` is compared. The bin
//!   that would leave the *smallest* remaining free volume wins; its
//!   snapshot is committed back onto the live state.
//!
//! `BinState` does not implement `Clone` by design, so the snapshot/
//! restore pattern lives in this module via the private
//! [`snapshot_state`] / [`restore_state`] helpers that pluck
//! `pub(super)` fields off of `BinState`. This keeps the extreme-points
//! module free of policy-specific APIs.

use std::collections::BTreeSet;

use super::common::{build_solution, volume_u64};
use super::extreme_points::{BinState, ExtremePointsScoring, try_place_into_bin};
use super::model::{
    Bin3D, ItemInstance3D, MAX_BIN_COUNT_3D, Placement3D, SolverMetrics3D, ThreeDOptions,
    ThreeDProblem, ThreeDSolution,
};
use crate::{BinPackingError, Result};

/// Solve the problem using First-Fit-Decreasing on item volume.
///
/// Items are sorted by decreasing volume. For each item, the solver
/// walks through currently-open bins in declaration order and commits
/// the first one that [`try_place_into_bin`] accepts under the
/// [`ExtremePointsScoring::VolumeFitResidual`] rule. If no open bin
/// accepts the item, the solver opens a new bin using the same
/// smallest-volume compatible bin-type policy as the EP engine,
/// honouring [`Bin3D`] quantity caps.
///
/// # Errors
///
/// Returns [`BinPackingError::Unsupported`] when the multi-bin loop
/// would exceed [`MAX_BIN_COUNT_3D`].
pub(super) fn solve_first_fit_decreasing_volume(
    problem: &ThreeDProblem,
    options: &ThreeDOptions,
) -> Result<ThreeDSolution> {
    solve_with_policy(problem, options, Policy::FirstFit, "first_fit_decreasing_volume")
}

/// Solve the problem using Best-Fit-Decreasing on item volume.
///
/// Items are sorted by decreasing volume. For each item, the solver
/// evaluates *every* currently-open bin by snapshotting its state and
/// running [`try_place_into_bin`] on the snapshot; the snapshot whose
/// post-placement used volume is largest (i.e. smallest leftover) wins
/// and is committed back onto the live state. Ties are broken by open
/// order. If no open bin accepts the item, the solver opens a new bin
/// using the same smallest-volume compatible bin-type policy as the EP
/// engine.
///
/// # Errors
///
/// Returns [`BinPackingError::Unsupported`] when the multi-bin loop
/// would exceed [`MAX_BIN_COUNT_3D`].
pub(super) fn solve_best_fit_decreasing_volume(
    problem: &ThreeDProblem,
    options: &ThreeDOptions,
) -> Result<ThreeDSolution> {
    solve_with_policy(problem, options, Policy::BestFit, "best_fit_decreasing_volume")
}

/// Per-item selection policy applied across the currently-open bins.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Policy {
    /// Commit the first open bin whose EP engine accepts the item.
    FirstFit,
    /// Commit the open bin whose snapshot yields the largest
    /// post-placement used volume (equivalently, the smallest leftover).
    BestFit,
}

/// Snapshot of the mutable fields of a [`BinState`], captured before a
/// tentative `try_place_into_bin` call and restored after if the
/// placement was either rejected or not the winning candidate.
struct BinStateSnapshot {
    placements: Vec<Placement3D>,
    eps: BTreeSet<(u32, u32, u32)>,
    extreme_points_generated: usize,
}

/// Capture a snapshot of the mutable fields of `state`. The `bin_index`
/// is immutable for the lifetime of a `BinState`, so it is not
/// captured.
fn snapshot_state(state: &BinState) -> BinStateSnapshot {
    BinStateSnapshot {
        placements: state.placements.clone(),
        eps: state.eps.clone(),
        extreme_points_generated: state.extreme_points_generated,
    }
}

/// Restore `state` to the values captured in `snapshot`. Consumes the
/// snapshot so a single snapshot cannot be reused accidentally.
fn restore_state(state: &mut BinState, snapshot: BinStateSnapshot) {
    state.placements = snapshot.placements;
    state.eps = snapshot.eps;
    state.extreme_points_generated = snapshot.extreme_points_generated;
}

/// Sum of the volumes of every placement in `state`, widened to `u64`.
fn sum_placed_volume(state: &BinState) -> u64 {
    state
        .placements
        .iter()
        .map(|placement| volume_u64(placement.width, placement.height, placement.depth))
        .sum()
}

fn solve_with_policy(
    problem: &ThreeDProblem,
    _options: &ThreeDOptions,
    policy: Policy,
    name: &'static str,
) -> Result<ThreeDSolution> {
    let mut items = problem.expanded_items();
    items.sort_by(|a, b| {
        let lhs = volume_u64(a.width, a.height, a.depth);
        let rhs = volume_u64(b.width, b.height, b.depth);
        rhs.cmp(&lhs)
    });

    let mut states: Vec<BinState> = Vec::new();
    let mut bin_quantity_used: Vec<usize> = vec![0; problem.bins.len()];
    let mut unplaced: Vec<ItemInstance3D> = Vec::new();
    let mut total_extreme_points_generated: usize = 0;

    for item in items {
        if place_item_multi_bin(
            problem,
            &item,
            &mut states,
            &mut bin_quantity_used,
            policy,
            &mut total_extreme_points_generated,
        )? {
            continue;
        }
        unplaced.push(item);
    }

    let bin_count = states.len();
    let bin_placements: Vec<(usize, Vec<Placement3D>)> =
        states.into_iter().map(|state| (state.bin_index, state.placements)).collect();

    let metrics = SolverMetrics3D {
        iterations: 1,
        explored_states: 0,
        extreme_points_generated: total_extreme_points_generated,
        branch_and_bound_nodes: 0,
        notes: vec![format!("{name}: {bin_count} bin(s), {} unplaced", unplaced.len())],
    };

    build_solution(name, &problem.bins, bin_placements, unplaced, metrics, false)
}

/// Attempt to place `item` into any currently open bin under `policy`;
/// failing that, open a new bin using best-volume-fit over the bin
/// types. Returns `Ok(true)` on success, `Ok(false)` when every
/// candidate bin type has been exhausted for this item.
fn place_item_multi_bin(
    problem: &ThreeDProblem,
    item: &ItemInstance3D,
    states: &mut Vec<BinState>,
    bin_quantity_used: &mut [usize],
    policy: Policy,
    total_extreme_points_generated: &mut usize,
) -> Result<bool> {
    match policy {
        Policy::FirstFit => {
            for state in states.iter_mut() {
                let bin = &problem.bins[state.bin_index];
                if try_place_into_bin(state, bin, item, ExtremePointsScoring::VolumeFitResidual)
                    .is_some()
                {
                    *total_extreme_points_generated =
                        total_extreme_points_generated.saturating_add(state.eps.len());
                    return Ok(true);
                }
            }
        }
        Policy::BestFit => {
            // Track the best (highest post-placement used volume,
            // i.e. smallest leftover) candidate seen so far.
            let mut best: Option<BestFitCandidate> = None;
            for (index, state) in states.iter_mut().enumerate() {
                let bin = &problem.bins[state.bin_index];
                let pre_snapshot = snapshot_state(state);
                if try_place_into_bin(state, bin, item, ExtremePointsScoring::VolumeFitResidual)
                    .is_some()
                {
                    let post_used = sum_placed_volume(state);
                    let post_snapshot = snapshot_state(state);
                    restore_state(state, pre_snapshot);
                    match best {
                        None => {
                            best = Some(BestFitCandidate {
                                index,
                                used_volume: post_used,
                                snapshot: post_snapshot,
                            });
                        }
                        Some(ref current) => {
                            // "Best" = smallest leftover = largest used
                            // volume after placement. Ties break on the
                            // earliest open-order index, which is the
                            // already-stored `current` (we never prefer
                            // a later-indexed equal candidate).
                            if post_used > current.used_volume {
                                best = Some(BestFitCandidate {
                                    index,
                                    used_volume: post_used,
                                    snapshot: post_snapshot,
                                });
                            }
                        }
                    }
                } else {
                    // `try_place_into_bin` returns `None` without
                    // mutating `state` on failure, but restore
                    // defensively so any future change to that contract
                    // does not silently corrupt the live state.
                    restore_state(state, pre_snapshot);
                }
            }
            if let Some(BestFitCandidate { index, snapshot, .. }) = best {
                let state = &mut states[index];
                restore_state(state, snapshot);
                *total_extreme_points_generated =
                    total_extreme_points_generated.saturating_add(state.eps.len());
                return Ok(true);
            }
        }
    }

    // No open bin accepted the item; open a new one. Best-fit by bin
    // volume among the bin types that still have remaining quantity and
    // can physically contain at least one orientation of the item. Ties
    // break on `Bin3D` declaration order.
    let mut skip_bin_type: Vec<bool> = vec![false; problem.bins.len()];
    loop {
        let mut best: Option<(usize, u64)> = None;
        for (bin_index, bin) in problem.bins.iter().enumerate() {
            if skip_bin_type[bin_index] {
                continue;
            }
            if let Some(cap) = bin.quantity
                && bin_quantity_used[bin_index] >= cap
            {
                continue;
            }
            if !bin_contains_some_orientation(bin, item) {
                continue;
            }
            let volume = volume_u64(bin.width, bin.height, bin.depth);
            best = match best {
                None => Some((bin_index, volume)),
                Some((current_index, current_volume)) => {
                    if volume < current_volume {
                        Some((bin_index, volume))
                    } else {
                        Some((current_index, current_volume))
                    }
                }
            };
        }
        let Some((bin_index, _)) = best else {
            return Ok(false);
        };

        if states.len() >= MAX_BIN_COUNT_3D {
            return Err(BinPackingError::Unsupported(format!(
                "3D bin count cap exceeded: opened {} bins, MAX_BIN_COUNT_3D = {MAX_BIN_COUNT_3D}",
                states.len()
            )));
        }

        let mut state = BinState::new(bin_index);
        *total_extreme_points_generated = total_extreme_points_generated.saturating_add(1);
        let bin = &problem.bins[bin_index];
        if try_place_into_bin(&mut state, bin, item, ExtremePointsScoring::VolumeFitResidual)
            .is_some()
        {
            bin_quantity_used[bin_index] = bin_quantity_used[bin_index].saturating_add(1);
            *total_extreme_points_generated =
                total_extreme_points_generated.saturating_add(state.eps.len());
            states.push(state);
            return Ok(true);
        }
        // Defensive: a fresh bin with an empty placement list should
        // always accept an item whose dimensions physically fit. If it
        // somehow doesn't (pathological rotation filter, future scoring
        // change, etc.), skip this bin type on subsequent iterations so
        // we still make progress toward `unplaced`.
        skip_bin_type[bin_index] = true;
    }
}

/// Best-fit candidate captured during the open-bin scan in
/// [`Policy::BestFit`]. `snapshot` is the *post-placement* snapshot,
/// ready to be committed onto the live state once the best candidate
/// is known.
struct BestFitCandidate {
    index: usize,
    used_volume: u64,
    snapshot: BinStateSnapshot,
}

/// Whether `bin` is large enough to hold `item` in at least one allowed
/// rotation.
fn bin_contains_some_orientation(bin: &Bin3D, item: &ItemInstance3D) -> bool {
    item.orientations().any(|(_, w, h, d)| w <= bin.width && h <= bin.height && d <= bin.depth)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::three_d::model::{BoxDemand3D, RotationMask3D};

    fn problem_one_box() -> ThreeDProblem {
        ThreeDProblem {
            bins: vec![Bin3D {
                name: "b".into(),
                width: 10,
                height: 10,
                depth: 10,
                cost: 1.0,
                quantity: None,
            }],
            demands: vec![BoxDemand3D {
                name: "a".into(),
                width: 4,
                height: 4,
                depth: 4,
                quantity: 1,
                allowed_rotations: RotationMask3D::ALL,
            }],
        }
    }

    #[test]
    fn ffd_places_single_box_in_trivial_bin() {
        let solution =
            solve_first_fit_decreasing_volume(&problem_one_box(), &ThreeDOptions::default())
                .expect("solve ffd");
        assert!(solution.unplaced.is_empty());
        assert_eq!(solution.bin_count, 1);
        assert_eq!(solution.layouts[0].placements.len(), 1);
    }

    #[test]
    fn bfd_places_single_box_in_trivial_bin() {
        let solution =
            solve_best_fit_decreasing_volume(&problem_one_box(), &ThreeDOptions::default())
                .expect("solve bfd");
        assert!(solution.unplaced.is_empty());
        assert_eq!(solution.bin_count, 1);
        assert_eq!(solution.layouts[0].placements.len(), 1);
    }

    #[test]
    fn ffd_opens_multiple_bins_when_capacity_is_exceeded() {
        // Two 5x5x5 boxes cannot share a 5x5x5 bin — FFD must open two.
        let problem = ThreeDProblem {
            bins: vec![Bin3D {
                name: "b".into(),
                width: 5,
                height: 5,
                depth: 5,
                cost: 1.0,
                quantity: None,
            }],
            demands: vec![BoxDemand3D {
                name: "a".into(),
                width: 5,
                height: 5,
                depth: 5,
                quantity: 2,
                allowed_rotations: RotationMask3D::XYZ,
            }],
        };
        let solution = solve_first_fit_decreasing_volume(&problem, &ThreeDOptions::default())
            .expect("solve ffd");
        assert!(solution.unplaced.is_empty());
        assert_eq!(solution.bin_count, 2);
    }

    #[test]
    fn algorithm_names_match_wire_format() {
        let problem = problem_one_box();
        let ffd =
            solve_first_fit_decreasing_volume(&problem, &ThreeDOptions::default()).expect("ffd");
        let bfd =
            solve_best_fit_decreasing_volume(&problem, &ThreeDOptions::default()).expect("bfd");
        assert_eq!(ffd.algorithm, "first_fit_decreasing_volume");
        assert_eq!(bfd.algorithm, "best_fit_decreasing_volume");
        assert!(!ffd.guillotine);
        assert!(!bfd.guillotine);
    }

    #[test]
    fn bfd_is_at_least_as_good_as_ffd_on_hand_constructed_input() {
        // Hand-constructed workload: the FFD and BFD policies may differ
        // on which open bin absorbs each subsequent item. For a small
        // instance we can't guarantee a strict bin-count or waste gap,
        // so we fall back to the `is_better_than` comparator with a
        // tie allowance per the plan's contract.
        let problem = ThreeDProblem {
            bins: vec![Bin3D {
                name: "b".into(),
                width: 10,
                height: 10,
                depth: 10,
                cost: 1.0,
                quantity: None,
            }],
            demands: vec![
                BoxDemand3D {
                    name: "big".into(),
                    width: 10,
                    height: 10,
                    depth: 5,
                    quantity: 1,
                    allowed_rotations: RotationMask3D::XYZ,
                },
                BoxDemand3D {
                    name: "med".into(),
                    width: 10,
                    height: 5,
                    depth: 5,
                    quantity: 1,
                    allowed_rotations: RotationMask3D::XYZ,
                },
                BoxDemand3D {
                    name: "small".into(),
                    width: 10,
                    height: 5,
                    depth: 5,
                    quantity: 1,
                    allowed_rotations: RotationMask3D::XYZ,
                },
            ],
        };
        let ffd =
            solve_first_fit_decreasing_volume(&problem, &ThreeDOptions::default()).expect("ffd");
        let bfd =
            solve_best_fit_decreasing_volume(&problem, &ThreeDOptions::default()).expect("bfd");
        assert!(ffd.unplaced.is_empty());
        assert!(bfd.unplaced.is_empty());
        assert!(
            bfd.is_better_than(&ffd) || !ffd.is_better_than(&bfd),
            "BFD must be at least as good as FFD under the lex comparator"
        );
    }
}
