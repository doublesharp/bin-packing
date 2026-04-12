//! Deepest-Bottom-Left (DBL) and Deepest-Bottom-Left-Fill (DBLF) placement.
//!
//! Implements the event-point family of Karabulut & İnceoğlu (2004)
//! "A hybrid genetic algorithm for packing in 3D with deepest bottom left
//! with fill method". Both variants maintain a per-bin event-point set
//! seeded at the origin and inserted-into after every placement, but they
//! differ in how they pick which event point an item lands on:
//!
//! - **DBL** (`solve_deepest_bottom_left`) tries only the lex-smallest
//!   event point. If the item does not fit under any allowed rotation at
//!   that point, the item is abandoned for this bin and the solver moves
//!   on to the next bin (opening a new one if necessary).
//! - **DBLF** (`solve_deepest_bottom_left_fill`) scans *every* event point
//!   in lex order and places the item at the first one that fits under
//!   some allowed rotation. This is the "fill" behaviour — earlier gaps
//!   get a chance to be filled before a fresh corner is used.
//!
//! Event points are stored as `(z, y, x)` tuples inside a [`BTreeSet`] so
//! the natural sorted order iterates "deepest, then bottom, then left" for
//! free. After a placement of extents `(w, h, d)` at `(x, y, z)`, three
//! new event points are inserted: `(x + w, y, z)`, `(x, y + h, z)`, and
//! `(x, y, z + d)`. v1 does **not** project these corners backwards onto
//! existing placements (CPT08-style projection). The unprojected form may
//! leave more dead space than projected variants would; this trade-off is
//! recorded in `metrics.notes`.

use std::collections::BTreeSet;

use super::common::{build_solution, placement_feasible, volume_u64};
use super::model::{
    Bin3D, ItemInstance3D, MAX_BIN_COUNT_3D, Placement3D, Rotation3D, SolverMetrics3D,
    ThreeDOptions, ThreeDProblem, ThreeDSolution,
};
use crate::{BinPackingError, Result};

/// Which scanning strategy to use over the event-point set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScanStrategy {
    /// DBL: only the lex-smallest event point is tried.
    FirstOnly,
    /// DBLF: every event point is tried in lex order until one fits.
    Fill,
}

/// Snapshot of one open bin during DBL / DBLF placement.
struct BinState {
    /// Index into `problem.bins` of the bin type this state occupies.
    bin_index: usize,
    /// Boxes placed into the bin so far, in placement order.
    placements: Vec<Placement3D>,
    /// Event points anchored inside the bin, stored as `(z, y, x)` so the
    /// natural [`BTreeSet`] ordering iterates "deepest, bottom, leftmost".
    event_points: BTreeSet<(u32, u32, u32)>,
}

impl BinState {
    fn new(bin_index: usize) -> Self {
        let mut event_points = BTreeSet::new();
        event_points.insert((0, 0, 0));
        Self { bin_index, placements: Vec::new(), event_points }
    }
}

/// Solve the problem using the Deepest-Bottom-Left (DBL) heuristic.
///
/// Items are processed in decreasing volume order. For each item, the
/// solver walks through currently open bins in open-order and tries only
/// the lex-smallest event point in each bin. If no open bin accepts the
/// item at its lex-first event point, a new bin is opened (smallest-volume
/// compatible bin type, honouring `Bin3D.quantity` caps, ties broken by
/// declaration order). Items that cannot fit any bin type land in
/// `unplaced`.
///
/// # Errors
///
/// Returns [`BinPackingError::Unsupported`] when the multi-bin loop would
/// exceed [`MAX_BIN_COUNT_3D`].
pub(super) fn solve_deepest_bottom_left(
    problem: &ThreeDProblem,
    options: &ThreeDOptions,
) -> Result<ThreeDSolution> {
    solve_with_strategy(problem, options, ScanStrategy::FirstOnly, "deepest_bottom_left")
}

/// Solve the problem using the Deepest-Bottom-Left-Fill (DBLF) heuristic.
///
/// Identical to [`solve_deepest_bottom_left`] except that, within each
/// open bin, every event point is tried in lex order (deepest, bottom,
/// left) and the first one that fits the item under any allowed rotation
/// is used. This gives earlier gaps a chance to be filled before a fresh
/// corner is considered.
///
/// # Errors
///
/// Returns [`BinPackingError::Unsupported`] when the multi-bin loop would
/// exceed [`MAX_BIN_COUNT_3D`].
pub(super) fn solve_deepest_bottom_left_fill(
    problem: &ThreeDProblem,
    options: &ThreeDOptions,
) -> Result<ThreeDSolution> {
    solve_with_strategy(problem, options, ScanStrategy::Fill, "deepest_bottom_left_fill")
}

fn solve_with_strategy(
    problem: &ThreeDProblem,
    _options: &ThreeDOptions,
    strategy: ScanStrategy,
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

    for item in items {
        if place_item_multi_bin(problem, &item, &mut states, &mut bin_quantity_used, strategy)? {
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
        extreme_points_generated: 0,
        branch_and_bound_nodes: 0,
        notes: vec![
            "dblf: extreme points are unprojected (v1)".to_string(),
            format!("{name}: {bin_count} bin(s), {} unplaced", unplaced.len()),
        ],
    };

    build_solution(name, &problem.bins, bin_placements, unplaced, metrics, false)
}

/// Attempt to place `item` into any currently open bin; failing that,
/// open a new bin using best-volume-fit over the bin types. Returns
/// `Ok(true)` on success, `Ok(false)` when every candidate bin type has
/// been exhausted for this item.
fn place_item_multi_bin(
    problem: &ThreeDProblem,
    item: &ItemInstance3D,
    states: &mut Vec<BinState>,
    bin_quantity_used: &mut [usize],
    strategy: ScanStrategy,
) -> Result<bool> {
    // First try every currently-open bin, in the order they were opened.
    for state in states.iter_mut() {
        let bin = &problem.bins[state.bin_index];
        if try_place_into_bin(state, bin, item, strategy).is_some() {
            return Ok(true);
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
        let bin = &problem.bins[bin_index];
        if try_place_into_bin(&mut state, bin, item, strategy).is_some() {
            bin_quantity_used[bin_index] = bin_quantity_used[bin_index].saturating_add(1);
            states.push(state);
            return Ok(true);
        }
        // Defensive: `bin_contains_some_orientation` already vetted that
        // *some* rotation fits this bin in isolation, so a fresh bin with
        // an empty placement list must accept the item. If it somehow
        // doesn't (e.g. a pathological rotation filter), skip this bin
        // type on subsequent iterations so we make progress toward
        // `unplaced`.
        skip_bin_type[bin_index] = true;
    }
}

/// Try to place `item` into `state` under the chosen scan strategy.
///
/// For each candidate event point in lex order, select the smallest
/// rotation by placed volume that fits (ties broken by [`Rotation3D`]
/// declaration order). DBL stops after the lex-smallest event point;
/// DBLF keeps scanning until a fit is found.
fn try_place_into_bin(
    state: &mut BinState,
    bin: &Bin3D,
    item: &ItemInstance3D,
    strategy: ScanStrategy,
) -> Option<Placement3D> {
    // Snapshot the set so the scan is stable even when we later mutate
    // `state.event_points`.
    let ep_snapshot: Vec<(u32, u32, u32)> = state.event_points.iter().copied().collect();

    for &(ep_z, ep_y, ep_x) in &ep_snapshot {
        if let Some((rotation, w, h, d)) = pick_rotation(item, state, bin, ep_x, ep_y, ep_z) {
            let placement = Placement3D {
                name: item.name.clone(),
                x: ep_x,
                y: ep_y,
                z: ep_z,
                width: w,
                height: h,
                depth: d,
                rotation,
            };
            state.event_points.remove(&(ep_z, ep_y, ep_x));
            insert_new_event_points(&placement, bin, &mut state.event_points);
            state.placements.push(placement.clone());
            return Some(placement);
        }
        if strategy == ScanStrategy::FirstOnly {
            return None;
        }
    }
    None
}

/// Pick the best rotation of `item` that fits at `(ep_x, ep_y, ep_z)`
/// inside `bin` without overlapping any previously placed box.
///
/// Selection rule: the smallest enclosing rotation by placed volume,
/// with ties broken by the rotation's declaration order (via the
/// natural [`Rotation3D`] enum iterator on [`super::model::RotationMask3D`]).
/// All six rotations of a given item have the same volume, so in practice
/// the "smallest" criterion is a no-op and the tiebreaker picks the
/// first fitting rotation in declaration order — which is exactly what
/// the plan mandates.
fn pick_rotation(
    item: &ItemInstance3D,
    state: &BinState,
    bin: &Bin3D,
    ep_x: u32,
    ep_y: u32,
    ep_z: u32,
) -> Option<(Rotation3D, u32, u32, u32)> {
    let mut best: Option<(Rotation3D, u32, u32, u32)> = None;
    let mut best_volume: u64 = u64::MAX;
    for (rotation, w, h, d) in item.orientations() {
        if !placement_feasible(
            ep_x,
            ep_y,
            ep_z,
            w,
            h,
            d,
            bin.width,
            bin.height,
            bin.depth,
            &state.placements,
        ) {
            continue;
        }
        let volume = volume_u64(w, h, d);
        if best.is_none() || volume < best_volume {
            best = Some((rotation, w, h, d));
            best_volume = volume;
        }
    }
    best
}

/// Append the three unprojected corner event points generated by placing
/// `placement`. Corners that fall outside the bin extents are silently
/// dropped — they correspond to items flush against a bin wall.
fn insert_new_event_points(
    placement: &Placement3D,
    bin: &Bin3D,
    event_points: &mut BTreeSet<(u32, u32, u32)>,
) {
    let corner_x = placement.x.saturating_add(placement.width);
    if corner_x < bin.width {
        event_points.insert((placement.z, placement.y, corner_x));
    }
    let corner_y = placement.y.saturating_add(placement.height);
    if corner_y < bin.height {
        event_points.insert((placement.z, corner_y, placement.x));
    }
    let corner_z = placement.z.saturating_add(placement.depth);
    if corner_z < bin.depth {
        event_points.insert((corner_z, placement.y, placement.x));
    }
}

/// Whether `bin` is large enough to hold `item` in at least one allowed rotation.
fn bin_contains_some_orientation(bin: &Bin3D, item: &ItemInstance3D) -> bool {
    item.orientations().any(|(_, w, h, d)| w <= bin.width && h <= bin.height && d <= bin.depth)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::three_d::model::{Bin3D, BoxDemand3D, RotationMask3D, ThreeDOptions, ThreeDProblem};

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
    fn dbl_places_single_box_in_trivial_bin() {
        let solution = solve_deepest_bottom_left(&problem_one_box(), &ThreeDOptions::default())
            .expect("solve dbl");
        assert!(solution.unplaced.is_empty());
        assert_eq!(solution.bin_count, 1);
        assert_eq!(solution.layouts[0].placements.len(), 1);
    }

    #[test]
    fn dblf_opens_second_bin_when_first_is_full() {
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
        let solution = solve_deepest_bottom_left_fill(&problem, &ThreeDOptions::default())
            .expect("solve dblf");
        assert!(solution.unplaced.is_empty());
        assert_eq!(solution.bin_count, 2);
    }

    #[test]
    fn dblf_respects_rotation_mask() {
        // 6x1x1 box in a 1x6x1 bin: only rotations that put 6 on the
        // y-axis fit. With `ALL` allowed, the solver must rotate to fit.
        let problem = ThreeDProblem {
            bins: vec![Bin3D {
                name: "b".into(),
                width: 1,
                height: 6,
                depth: 1,
                cost: 1.0,
                quantity: None,
            }],
            demands: vec![BoxDemand3D {
                name: "a".into(),
                width: 6,
                height: 1,
                depth: 1,
                quantity: 1,
                allowed_rotations: RotationMask3D::ALL,
            }],
        };
        let solution = solve_deepest_bottom_left_fill(&problem, &ThreeDOptions::default())
            .expect("solve dblf");
        assert!(solution.unplaced.is_empty());
        assert_eq!(solution.bin_count, 1);
        let placement = &solution.layouts[0].placements[0];
        assert_eq!(placement.height, 6);
    }

    #[test]
    fn algorithm_names_match_wire_format() {
        let problem = problem_one_box();
        let dbl = solve_deepest_bottom_left(&problem, &ThreeDOptions::default()).expect("dbl");
        let dblf =
            solve_deepest_bottom_left_fill(&problem, &ThreeDOptions::default()).expect("dblf");
        assert_eq!(dbl.algorithm, "deepest_bottom_left");
        assert_eq!(dblf.algorithm, "deepest_bottom_left_fill");
        assert!(!dbl.guillotine);
        assert!(!dblf.guillotine);
    }

    #[test]
    fn dblf_fills_gap_that_dbl_abandons() {
        // Hand-constructed contrast: a 10x10x10 bin with rotations
        // disabled, two items sorted largest-first by volume:
        //
        //   A = 10x5x5 (vol 500) placed at the origin.
        //   B = 10x6x3 (vol 180) needs to follow.
        //
        // After placing A at (0,0,0), the event-point set holds two
        // unprojected corners: (0,5,0) and (0,0,5). Lex order
        // ((z, y, x) tuple) is (0,5,0) < (5,0,0), i.e. (0,5,0) first.
        //
        // At (0,5,0), only 5 units of height remain, so B (height 6)
        // does not fit. DBL gives up on the bin and opens a second
        // bin for B. DBLF continues the scan and finds that B fits
        // at (0,0,5) (width 10, height 10, depth 5 are all available),
        // so it packs both items into a single bin.
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
                    name: "a".into(),
                    width: 10,
                    height: 5,
                    depth: 5,
                    quantity: 1,
                    allowed_rotations: RotationMask3D::XYZ,
                },
                BoxDemand3D {
                    name: "b_item".into(),
                    width: 10,
                    height: 6,
                    depth: 3,
                    quantity: 1,
                    allowed_rotations: RotationMask3D::XYZ,
                },
            ],
        };

        let dbl = solve_deepest_bottom_left(&problem, &ThreeDOptions::default()).expect("dbl");
        let dblf =
            solve_deepest_bottom_left_fill(&problem, &ThreeDOptions::default()).expect("dblf");

        assert!(dbl.unplaced.is_empty(), "DBL should still place both items, just in two bins");
        assert!(dblf.unplaced.is_empty());
        assert_eq!(dbl.bin_count, 2, "DBL abandons bin 1 after the lex-first EP fails");
        assert_eq!(dblf.bin_count, 1, "DBLF scans the whole EP set and fills the gap");
        assert!(dblf.bin_count <= dbl.bin_count);

        // Confirm the fill: DBLF's single bin must contain B_ITEM at
        // (0, 0, 5), which is the later EP that DBL never tried.
        let layout = &dblf.layouts[0];
        let b_placement = layout
            .placements
            .iter()
            .find(|placement| placement.name == "b_item")
            .expect("b_item present in dblf layout");
        assert_eq!((b_placement.x, b_placement.y, b_placement.z), (0, 0, 5));
    }
}
