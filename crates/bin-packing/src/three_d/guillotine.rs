//! Guillotine 3D beam-search placement engine with seven variants.
//!
//! Implements the unrestricted three-slab guillotine partition (Hifi 2004,
//! generalised to 3D): placing a box of extents `(w, h, d)` into a free
//! cuboid of extents `(W, H, D)` produces three sub-cuboids — the *right*
//! slab, the *top* slab, and the *front* slab. Slabs whose split-axis
//! extent is zero are discarded. The slab partition is unconditional; the
//! split-rule variants below act as *secondary scoring keys* during
//! candidate selection, not as alternative partitions.
//!
//! All variants share a single beam-search engine parameterised by
//! ([`GuillotineRanking`], [`GuillotineSplit`]). At each expansion step
//! the engine tries to place the *first* remaining item (items pre-sorted
//! by volume desc) into every feasible
//! `(open bin, free cuboid, allowed rotation)` triple, ranks the children
//! under the variant's `(primary_score, secondary_score)` comparator
//! (lower is better throughout), and keeps the top `options.beam_width`
//! live nodes. A leaf is a node whose `remaining` list is empty or that
//! cannot place its next item in any open bin *or* a freshly opened bin;
//! the best leaf under [`ThreeDSolution::is_better_than`] is returned.

use std::collections::VecDeque;

use super::common::{FreeCuboid3D, build_solution, volume_u64};
use super::model::{
    Bin3D, ItemInstance3D, MAX_BIN_COUNT_3D, Placement3D, Rotation3D, SolverMetrics3D,
    ThreeDOptions, ThreeDProblem, ThreeDSolution,
};
use crate::{BinPackingError, Result};

/// Primary ranking rules for beam children. Lower is better.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GuillotineRanking {
    /// `chosen_free_cuboid.volume() - item_volume` (tight volume fit).
    VolumeFit,
    /// Minimum leftover edge of the chosen cuboid after placing the box.
    ShortSide,
    /// Maximum leftover edge of the chosen cuboid after placing the box.
    LongSide,
}

/// Secondary split-scoring rules. Lower is better after normalisation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GuillotineSplit {
    /// Secondary key is `0`; ranking decides everything.
    Default,
    /// Secondary key is `min(W-w, H-h, D-d)` of the chosen free cuboid.
    ShorterLeftoverAxis,
    /// Secondary key is `max(W-w, H-h, D-d)` of the chosen free cuboid.
    LongerLeftoverAxis,
    /// Secondary key is the minimum volume of the three new slabs.
    MinVolumeSplit,
    /// Secondary key is `u64::MAX - max(slab volumes)` — keep one big slab.
    MaxVolumeSplit,
}

/// One live beam-search state.
#[derive(Debug, Clone)]
struct BeamNode {
    /// Free cuboids per currently open bin.
    free_per_bin: Vec<Vec<FreeCuboid3D>>,
    /// Placements per currently open bin.
    placements_per_bin: Vec<Vec<Placement3D>>,
    /// Bin-type index (into `problem.bins`) for each currently open bin.
    bin_indices: Vec<usize>,
    /// Per-bin-type usage counts, used to honour `Bin3D.quantity` caps.
    bin_quantity_used: Vec<usize>,
    /// Items still waiting to be placed, in volume-desc order.
    remaining: VecDeque<ItemInstance3D>,
    /// Items this branch has given up on.
    unplaced: Vec<ItemInstance3D>,
    /// Primary ranking score accumulated across the branch (lower better).
    score: u64,
    /// Secondary split-scoring key accumulated across the branch.
    secondary_score: u64,
}

impl BeamNode {
    fn new(num_bin_types: usize, remaining: Vec<ItemInstance3D>) -> Self {
        Self {
            free_per_bin: Vec::new(),
            placements_per_bin: Vec::new(),
            bin_indices: Vec::new(),
            bin_quantity_used: vec![0; num_bin_types],
            remaining: remaining.into(),
            unplaced: Vec::new(),
            score: 0,
            secondary_score: 0,
        }
    }
}

/// Solve with best-volume-fit ranking and the default split (identity).
///
/// # Errors
///
/// Returns [`BinPackingError::Unsupported`] when the beam search would
/// need to open more than [`MAX_BIN_COUNT_3D`] bins in any branch, and
/// propagates the same from [`build_solution`] on assembly.
pub(super) fn solve_guillotine_3d(
    problem: &ThreeDProblem,
    options: &ThreeDOptions,
) -> Result<ThreeDSolution> {
    run_variant(
        problem,
        options,
        GuillotineRanking::VolumeFit,
        GuillotineSplit::Default,
        "guillotine_3d",
    )
}

/// Solve with best-short-side-fit ranking and the default split.
///
/// # Errors
///
/// See [`solve_guillotine_3d`].
pub(super) fn solve_guillotine_3d_best_short_side_fit(
    problem: &ThreeDProblem,
    options: &ThreeDOptions,
) -> Result<ThreeDSolution> {
    run_variant(
        problem,
        options,
        GuillotineRanking::ShortSide,
        GuillotineSplit::Default,
        "guillotine_3d_best_short_side_fit",
    )
}

/// Solve with best-long-side-fit ranking and the default split.
///
/// # Errors
///
/// See [`solve_guillotine_3d`].
pub(super) fn solve_guillotine_3d_best_long_side_fit(
    problem: &ThreeDProblem,
    options: &ThreeDOptions,
) -> Result<ThreeDSolution> {
    run_variant(
        problem,
        options,
        GuillotineRanking::LongSide,
        GuillotineSplit::Default,
        "guillotine_3d_best_long_side_fit",
    )
}

/// Solve with best-volume-fit ranking and the shorter-leftover-axis split.
///
/// # Errors
///
/// See [`solve_guillotine_3d`].
pub(super) fn solve_guillotine_3d_shorter_leftover_axis(
    problem: &ThreeDProblem,
    options: &ThreeDOptions,
) -> Result<ThreeDSolution> {
    run_variant(
        problem,
        options,
        GuillotineRanking::VolumeFit,
        GuillotineSplit::ShorterLeftoverAxis,
        "guillotine_3d_shorter_leftover_axis",
    )
}

/// Solve with best-volume-fit ranking and the longer-leftover-axis split.
///
/// # Errors
///
/// See [`solve_guillotine_3d`].
pub(super) fn solve_guillotine_3d_longer_leftover_axis(
    problem: &ThreeDProblem,
    options: &ThreeDOptions,
) -> Result<ThreeDSolution> {
    run_variant(
        problem,
        options,
        GuillotineRanking::VolumeFit,
        GuillotineSplit::LongerLeftoverAxis,
        "guillotine_3d_longer_leftover_axis",
    )
}

/// Solve with best-volume-fit ranking and the min-volume-split rule.
///
/// # Errors
///
/// See [`solve_guillotine_3d`].
pub(super) fn solve_guillotine_3d_min_volume_split(
    problem: &ThreeDProblem,
    options: &ThreeDOptions,
) -> Result<ThreeDSolution> {
    run_variant(
        problem,
        options,
        GuillotineRanking::VolumeFit,
        GuillotineSplit::MinVolumeSplit,
        "guillotine_3d_min_volume_split",
    )
}

/// Solve with best-volume-fit ranking and the max-volume-split rule.
///
/// # Errors
///
/// See [`solve_guillotine_3d`].
pub(super) fn solve_guillotine_3d_max_volume_split(
    problem: &ThreeDProblem,
    options: &ThreeDOptions,
) -> Result<ThreeDSolution> {
    run_variant(
        problem,
        options,
        GuillotineRanking::VolumeFit,
        GuillotineSplit::MaxVolumeSplit,
        "guillotine_3d_max_volume_split",
    )
}

// ---------------------------------------------------------------------------
// Engine
// ---------------------------------------------------------------------------

fn run_variant(
    problem: &ThreeDProblem,
    options: &ThreeDOptions,
    ranking: GuillotineRanking,
    split: GuillotineSplit,
    name: &'static str,
) -> Result<ThreeDSolution> {
    let beam_width = options.beam_width.max(1);

    let mut items = problem.expanded_items();
    items.sort_by(|a, b| {
        let lhs = volume_u64(a.width, a.height, a.depth);
        let rhs = volume_u64(b.width, b.height, b.depth);
        rhs.cmp(&lhs)
    });

    let initial = BeamNode::new(problem.bins.len(), items);
    let mut frontier: Vec<BeamNode> = vec![initial];
    let mut leaves: Vec<BeamNode> = Vec::new();
    let mut iterations: usize = 0;
    let mut explored_states: usize = 0;

    while !frontier.is_empty() {
        iterations = iterations.saturating_add(1);
        let mut next: Vec<BeamNode> = Vec::new();
        for node in frontier.drain(..) {
            explored_states = explored_states.saturating_add(1);
            if node.remaining.is_empty() {
                leaves.push(node);
                continue;
            }
            let children = expand_node(&node, problem, ranking, split)?;
            if children.is_empty() {
                // No feasible placement for the next item in any open bin
                // or a freshly opened one; surrender the item to `unplaced`
                // and continue with the rest.
                let mut terminal = node;
                if let Some(head) = terminal.remaining.pop_front() {
                    terminal.unplaced.push(head);
                }
                next.push(terminal);
            } else {
                next.extend(children);
            }
        }

        // Prune the frontier to the top `beam_width` by (score, secondary).
        next.sort_by(|a, b| {
            a.score.cmp(&b.score).then_with(|| a.secondary_score.cmp(&b.secondary_score))
        });
        if next.len() > beam_width {
            next.truncate(beam_width);
        }
        frontier = next;
    }

    if leaves.is_empty() {
        return Err(BinPackingError::Unsupported(
            "guillotine_3d beam search produced no leaf nodes".to_string(),
        ));
    }

    // Build each candidate solution and pick the best under `is_better_than`.
    let mut best: Option<ThreeDSolution> = None;
    for leaf in leaves {
        let candidate = assemble_solution(problem, leaf, name, iterations, explored_states)?;
        best = match best {
            None => Some(candidate),
            Some(current) => {
                if candidate.is_better_than(&current) {
                    Some(candidate)
                } else {
                    Some(current)
                }
            }
        };
    }

    best.ok_or_else(|| {
        BinPackingError::Unsupported(
            "guillotine_3d beam search produced no assembled solution".to_string(),
        )
    })
}

/// Enumerate all feasible child nodes produced by placing the first
/// remaining item of `node`. If no open bin accepts the item, attempts to
/// open a new bin (smallest-volume first, honouring `Bin3D.quantity`
/// caps). Returns an empty vector if no placement is feasible at all.
fn expand_node(
    node: &BeamNode,
    problem: &ThreeDProblem,
    ranking: GuillotineRanking,
    split: GuillotineSplit,
) -> Result<Vec<BeamNode>> {
    let Some(item) = node.remaining.front() else {
        return Ok(Vec::new());
    };

    let mut children: Vec<BeamNode> = Vec::new();

    // Try placing into every currently open bin.
    for (open_idx, free_list) in node.free_per_bin.iter().enumerate() {
        for (free_idx, free) in free_list.iter().enumerate() {
            for (rotation, w, h, d) in item.orientations() {
                if !free.fits(w, h, d) {
                    continue;
                }
                let child = apply_placement(
                    node, open_idx, free_idx, *free, item, rotation, w, h, d, ranking, split,
                );
                children.push(child);
            }
        }
    }

    if !children.is_empty() {
        return Ok(children);
    }

    // No open bin accepted the item. Open a new bin (smallest-volume-first
    // among bin types that still have remaining quantity and contain *some*
    // orientation of the item). Ties break on declaration order.
    let mut best_bin: Option<(usize, u64)> = None;
    for (bin_index, bin) in problem.bins.iter().enumerate() {
        if let Some(cap) = bin.quantity
            && node.bin_quantity_used[bin_index] >= cap
        {
            continue;
        }
        if !bin_contains_some_orientation(bin, item) {
            continue;
        }
        let volume = volume_u64(bin.width, bin.height, bin.depth);
        best_bin = match best_bin {
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

    let Some((bin_index, _)) = best_bin else {
        return Ok(Vec::new());
    };

    if node.bin_indices.len() >= MAX_BIN_COUNT_3D {
        return Err(BinPackingError::Unsupported(format!(
            "3D bin count cap exceeded: opened {} bins, MAX_BIN_COUNT_3D = {MAX_BIN_COUNT_3D}",
            node.bin_indices.len()
        )));
    }

    // Spawn the new bin and try to place the item in it.
    let bin = &problem.bins[bin_index];
    let root_free =
        FreeCuboid3D { x: 0, y: 0, z: 0, width: bin.width, height: bin.height, depth: bin.depth };
    for (rotation, w, h, d) in item.orientations() {
        if !root_free.fits(w, h, d) {
            continue;
        }
        let mut fresh = node.clone();
        fresh.bin_indices.push(bin_index);
        fresh.free_per_bin.push(vec![root_free]);
        fresh.placements_per_bin.push(Vec::new());
        fresh.bin_quantity_used[bin_index] = fresh.bin_quantity_used[bin_index].saturating_add(1);
        let new_open_idx = fresh.bin_indices.len() - 1;
        let child = apply_placement(
            &fresh,
            new_open_idx,
            0,
            root_free,
            item,
            rotation,
            w,
            h,
            d,
            ranking,
            split,
        );
        children.push(child);
    }

    Ok(children)
}

#[allow(clippy::too_many_arguments)] // Engine-internal helper: all of these are load-bearing state.
fn apply_placement(
    node: &BeamNode,
    open_idx: usize,
    free_idx: usize,
    free: FreeCuboid3D,
    item: &ItemInstance3D,
    rotation: Rotation3D,
    w: u32,
    h: u32,
    d: u32,
    ranking: GuillotineRanking,
    split: GuillotineSplit,
) -> BeamNode {
    let mut child = node.clone();
    let Some(head) = child.remaining.pop_front() else {
        debug_assert!(false, "beam engine tried to place with an empty remaining queue");
        return child;
    };
    debug_assert_eq!(head.name, item.name, "beam engine desynchronised from remaining head");

    // Compute the three slabs (right / top / front). Slabs whose split-axis
    // extent is zero are skipped.
    let leftover_w = free.width - w;
    let leftover_h = free.height - h;
    let leftover_d = free.depth - d;
    let right_slab = FreeCuboid3D {
        x: free.x + w,
        y: free.y,
        z: free.z,
        width: leftover_w,
        height: free.height,
        depth: free.depth,
    };
    let top_slab = FreeCuboid3D {
        x: free.x,
        y: free.y + h,
        z: free.z,
        width: w,
        height: leftover_h,
        depth: free.depth,
    };
    let front_slab = FreeCuboid3D {
        x: free.x,
        y: free.y,
        z: free.z + d,
        width: w,
        height: h,
        depth: leftover_d,
    };

    let right_volume = if leftover_w > 0 { right_slab.volume() } else { 0 };
    let top_volume = if leftover_h > 0 { top_slab.volume() } else { 0 };
    let front_volume = if leftover_d > 0 { front_slab.volume() } else { 0 };

    // Replace the chosen free cuboid with the non-empty slabs in-place.
    let free_list = &mut child.free_per_bin[open_idx];
    free_list.swap_remove(free_idx);
    if leftover_w > 0 {
        free_list.push(right_slab);
    }
    if leftover_h > 0 {
        free_list.push(top_slab);
    }
    if leftover_d > 0 {
        free_list.push(front_slab);
    }

    let placement = Placement3D {
        name: head.name,
        x: free.x,
        y: free.y,
        z: free.z,
        width: w,
        height: h,
        depth: d,
        rotation,
    };
    child.placements_per_bin[open_idx].push(placement);

    let primary = primary_score(ranking, free, w, h, d);
    let secondary = secondary_score(
        split,
        leftover_w,
        leftover_h,
        leftover_d,
        right_volume,
        top_volume,
        front_volume,
    );
    child.score = child.score.saturating_add(primary);
    child.secondary_score = child.secondary_score.saturating_add(secondary);
    child
}

fn primary_score(ranking: GuillotineRanking, free: FreeCuboid3D, w: u32, h: u32, d: u32) -> u64 {
    match ranking {
        GuillotineRanking::VolumeFit => {
            let item_volume = volume_u64(w, h, d);
            free.volume().saturating_sub(item_volume)
        }
        GuillotineRanking::ShortSide => {
            let leftover_w = free.width - w;
            let leftover_h = free.height - h;
            let leftover_d = free.depth - d;
            u64::from(leftover_w.min(leftover_h).min(leftover_d))
        }
        GuillotineRanking::LongSide => {
            let leftover_w = free.width - w;
            let leftover_h = free.height - h;
            let leftover_d = free.depth - d;
            u64::from(leftover_w.max(leftover_h).max(leftover_d))
        }
    }
}

fn secondary_score(
    split: GuillotineSplit,
    leftover_w: u32,
    leftover_h: u32,
    leftover_d: u32,
    right_volume: u64,
    top_volume: u64,
    front_volume: u64,
) -> u64 {
    match split {
        GuillotineSplit::Default => 0,
        GuillotineSplit::ShorterLeftoverAxis => {
            u64::from(leftover_w.min(leftover_h).min(leftover_d))
        }
        GuillotineSplit::LongerLeftoverAxis => {
            u64::from(leftover_w.max(leftover_h).max(leftover_d))
        }
        GuillotineSplit::MinVolumeSplit => right_volume.min(top_volume).min(front_volume),
        GuillotineSplit::MaxVolumeSplit => {
            let max = right_volume.max(top_volume).max(front_volume);
            u64::MAX - max
        }
    }
}

fn bin_contains_some_orientation(bin: &Bin3D, item: &ItemInstance3D) -> bool {
    item.orientations().any(|(_, w, h, d)| w <= bin.width && h <= bin.height && d <= bin.depth)
}

fn assemble_solution(
    problem: &ThreeDProblem,
    node: BeamNode,
    name: &'static str,
    iterations: usize,
    explored_states: usize,
) -> Result<ThreeDSolution> {
    let BeamNode { bin_indices, placements_per_bin, unplaced, remaining, .. } = node;
    // Anything still in `remaining` at a leaf (shouldn't happen in normal
    // flow but be defensive) also counts as unplaced.
    let mut all_unplaced = unplaced;
    all_unplaced.extend(remaining);

    let bin_count = bin_indices.len();
    let bin_placements: Vec<(usize, Vec<Placement3D>)> =
        bin_indices.into_iter().zip(placements_per_bin).collect();

    let metrics = SolverMetrics3D {
        iterations,
        explored_states,
        extreme_points_generated: 0,
        branch_and_bound_nodes: 0,
        notes: vec![format!("{name}: {bin_count} bin(s), {} unplaced", all_unplaced.len())],
    };

    build_solution(name, &problem.bins, bin_placements, all_unplaced, metrics, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::three_d::model::{Bin3D, BoxDemand3D, RotationMask3D, ThreeDOptions, ThreeDProblem};

    fn bin(name: &str, w: u32, h: u32, d: u32) -> Bin3D {
        Bin3D { name: name.into(), width: w, height: h, depth: d, cost: 1.0, quantity: None }
    }

    fn demand(name: &str, w: u32, h: u32, d: u32, qty: usize) -> BoxDemand3D {
        BoxDemand3D {
            name: name.into(),
            width: w,
            height: h,
            depth: d,
            quantity: qty,
            allowed_rotations: RotationMask3D::ALL,
        }
    }

    fn trivial_problem() -> ThreeDProblem {
        ThreeDProblem { bins: vec![bin("b", 10, 10, 10)], demands: vec![demand("a", 3, 3, 3, 1)] }
    }

    #[test]
    fn guillotine_3d_places_single_box() {
        let solution =
            solve_guillotine_3d(&trivial_problem(), &ThreeDOptions::default()).expect("solve");
        assert!(solution.unplaced.is_empty());
        assert_eq!(solution.bin_count, 1);
        assert!(solution.guillotine);
        assert_eq!(solution.algorithm, "guillotine_3d");
    }

    #[test]
    fn guillotine_3d_opens_second_bin_when_needed() {
        // Four 5x5x5 boxes into a 5x5x5 bin forces >= 2 bins even if
        // the first bin somehow fit more than one (it can't).
        let problem = ThreeDProblem {
            bins: vec![bin("b", 5, 5, 5)],
            demands: vec![BoxDemand3D {
                name: "a".into(),
                width: 5,
                height: 5,
                depth: 5,
                quantity: 4,
                allowed_rotations: RotationMask3D::XYZ,
            }],
        };
        let solution =
            solve_guillotine_3d(&problem, &ThreeDOptions::default()).expect("solve multi-bin");
        assert!(solution.unplaced.is_empty(), "expected all placed");
        assert!(solution.bin_count >= 2, "expected >= 2 bins, got {}", solution.bin_count);
    }

    #[test]
    fn guillotine_3d_respects_rotation() {
        // A 6x1x1 box in a 1x6x1 bin: only rotations that put 6 on the
        // y-axis fit. With ALL rotations allowed, the solver must find one.
        let problem =
            ThreeDProblem { bins: vec![bin("b", 1, 6, 1)], demands: vec![demand("a", 6, 1, 1, 1)] };
        let solution =
            solve_guillotine_3d(&problem, &ThreeDOptions::default()).expect("solve rotation");
        assert!(solution.unplaced.is_empty(), "rotation-only fit should succeed");
        assert_eq!(solution.bin_count, 1);
    }

    #[test]
    fn all_guillotine_variants_expose_the_expected_algorithm_name() {
        let problem = ThreeDProblem {
            bins: vec![bin("b", 10, 10, 10)],
            demands: vec![demand("a", 3, 3, 3, 4)],
        };
        type Solver = fn(&ThreeDProblem, &ThreeDOptions) -> Result<ThreeDSolution>;
        let cases: [(&str, Solver); 7] = [
            ("guillotine_3d", solve_guillotine_3d),
            ("guillotine_3d_best_short_side_fit", solve_guillotine_3d_best_short_side_fit),
            ("guillotine_3d_best_long_side_fit", solve_guillotine_3d_best_long_side_fit),
            ("guillotine_3d_shorter_leftover_axis", solve_guillotine_3d_shorter_leftover_axis),
            ("guillotine_3d_longer_leftover_axis", solve_guillotine_3d_longer_leftover_axis),
            ("guillotine_3d_min_volume_split", solve_guillotine_3d_min_volume_split),
            ("guillotine_3d_max_volume_split", solve_guillotine_3d_max_volume_split),
        ];
        for (expected, solver) in cases {
            let solution = solver(&problem, &ThreeDOptions::default()).expect("solve variant");
            assert_eq!(solution.algorithm, expected);
            assert!(solution.guillotine, "{expected} must set guillotine=true");
            assert!(solution.unplaced.is_empty(), "{expected}");
        }
    }
}
