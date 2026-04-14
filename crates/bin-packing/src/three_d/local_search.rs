//! Standalone local-search meta-strategy for 3D bin packing.
//!
//! Seeds from the FFD volume heuristic and explores a
//! move/rotate/swap neighbourhood with bin-elimination repair after
//! each pass. Runs `options.multistart_runs` outer restarts, each
//! calling [`improve`] for `options.improvement_rounds` inner passes.
//!
//! The [`improve`] helper is kept with a stable signature so Task 14
//! (GRASP) can reuse it as its improvement phase.
//!
//! Neighbourhood moves considered per pass:
//!
//! - **Move**: for each placed item, try every `(extreme-point,
//!   rotation)` in every *other* currently open bin. A candidate is
//!   feasible iff it fits inside the destination bin and does not
//!   overlap any remaining placement in that bin.
//! - **Rotate**: for each placed item, try every other allowed
//!   rotation in its *current* bin.
//! - **Swap**: for each pair of placed items in *different* bins,
//!   swap their positions. Both destinations must remain overlap-free.
//!
//! Acceptance is **first improving**: any move that strictly beats the
//! current best under [`ThreeDSolution::is_better_than`] is taken and
//! the pass restarts. If no improving move is found, bin-elimination
//! repair runs: the bin with the smallest `used_volume` is targeted;
//! if every one of its items can be relocated into other bins, the
//! bin is dropped and the pass repeats.

use std::collections::BTreeSet;

use rand::{SeedableRng, rngs::SmallRng};

use super::common::{build_solution, placement_feasible, placements_overlap, volume_u64};
use super::model::{
    Bin3D, BoxDemand3D, Placement3D, Rotation3D, RotationMask3D, SolverMetrics3D, ThreeDOptions,
    ThreeDProblem, ThreeDSolution,
};
use super::sorted::solve_first_fit_decreasing_volume;
use crate::Result;

/// Default PRNG seed when `options.seed` is `None`.
const DEFAULT_SEED: u64 = 0;

/// Solve a 3D packing problem with the standalone local-search
/// meta-strategy.
///
/// Seeds an initial solution via the FFD heuristic, then runs
/// `options.multistart_runs` outer restarts, each calling [`improve`]
/// for `options.improvement_rounds` inner passes. Returns the best
/// solution seen across restarts under
/// [`ThreeDSolution::is_better_than`].
///
/// The returned solution's `algorithm` field is overwritten to
/// `"local_search"` regardless of what the FFD seed reported.
///
/// # Errors
///
/// Propagates any error from the initial FFD seed.
pub(super) fn solve_local_search(
    problem: &ThreeDProblem,
    options: &ThreeDOptions,
) -> Result<ThreeDSolution> {
    let seed_solution = solve_first_fit_decreasing_volume(problem, options)?;
    let mut rng = SmallRng::seed_from_u64(options.seed.unwrap_or(DEFAULT_SEED));
    let restarts = options.multistart_runs.max(1);

    let mut best = seed_solution.clone();
    best.algorithm = "local_search".to_string();

    // Restarts are sequential: each builds on the current best, so later
    // restarts benefit from earlier improvements. This is inherently
    // iterative and not suitable for parallelisation.
    let mut total_passes: usize = 0;
    for _ in 0..restarts {
        let candidate = improve(best.clone(), problem, options, &mut rng);
        total_passes = total_passes.saturating_add(options.improvement_rounds);
        if candidate.is_better_than(&best) {
            best = candidate;
        }
    }

    best.algorithm = "local_search".to_string();
    best.metrics.iterations = total_passes;
    best.metrics
        .notes
        .push(format!("local_search: {restarts} restart(s), {total_passes} improvement pass(es)"));
    Ok(best)
}

/// Apply the local-search improvement phase to `initial`.
///
/// Runs up to `options.improvement_rounds` passes over the
/// move/rotate/swap neighbourhood with bin-elimination repair after
/// each pass. Returns the best solution found (no worse than
/// `initial` under [`ThreeDSolution::is_better_than`]).
///
/// This helper is exposed so Task 14 (GRASP) can consume it as its
/// improvement phase; its signature must remain stable.
pub(super) fn improve(
    initial: ThreeDSolution,
    problem: &ThreeDProblem,
    options: &ThreeDOptions,
    _rng: &mut SmallRng,
) -> ThreeDSolution {
    // Decompose the incoming solution into a mutable working
    // representation. If decomposition fails (e.g. a placement name
    // cannot be matched to a demand), return the initial solution
    // untouched — local search never regresses.
    let Some(mut working) = WorkingState::from_solution(&initial, problem) else {
        return initial;
    };

    let mut best = initial;
    let rounds = options.improvement_rounds;
    for _ in 0..rounds {
        // Each `try_*_once` call commits the first strictly improving
        // move it finds into `working` and returns the snapshot of
        // the new solution. If none of the move/rotate/swap
        // neighbourhoods produce an improvement, fall back to
        // bin-elimination repair. When every neighbourhood reports
        // `None`, the current configuration is a local optimum and
        // the loop terminates early.
        let improved_snapshot = try_move_once(&mut working, problem, &best)
            .or_else(|| try_rotate_once(&mut working, problem, &best))
            .or_else(|| try_swap_once(&mut working, problem, &best))
            .or_else(|| try_bin_elimination(&mut working, problem, &best));
        match improved_snapshot {
            Some(snapshot) => best = snapshot,
            None => break,
        }
    }

    // If the best snapshot doesn't match the working state (e.g. the
    // last tentative move didn't strictly improve), we already
    // retained `best` via the `is_better_than` gate above, so just
    // return it. Preserve the algorithm field as `local_search`.
    let mut out = best;
    out.algorithm = "local_search".to_string();
    out
}

/// A placed item inside a working bin. Carries the original
/// (un-rotated) demand dimensions and rotation mask so the
/// neighbourhood can try alternate rotations.
#[derive(Debug, Clone)]
struct WorkingItem {
    name: String,
    /// Declared width of the originating demand (not yet rotated).
    original_w: u32,
    /// Declared height of the originating demand.
    original_h: u32,
    /// Declared depth of the originating demand.
    original_d: u32,
    allowed_rotations: RotationMask3D,
    placement: Placement3D,
}

impl WorkingItem {
    /// Yield `(rotation, width, height, depth)` for every allowed
    /// rotation, deduplicating extent tuples so cubes and square
    /// cross-sections don't repeat themselves.
    fn orientations(&self) -> Vec<(Rotation3D, u32, u32, u32)> {
        let mut seen: Vec<(u32, u32, u32)> = Vec::with_capacity(6);
        let mut out: Vec<(Rotation3D, u32, u32, u32)> = Vec::with_capacity(6);
        for rotation in self.allowed_rotations.iter() {
            let extents = rotation.apply(self.original_w, self.original_h, self.original_d);
            if seen.contains(&extents) {
                continue;
            }
            seen.push(extents);
            out.push((rotation, extents.0, extents.1, extents.2));
        }
        out
    }
}

/// A working bin: the bin-type index into `problem.bins` and a list
/// of the items currently placed in this particular bin instance.
#[derive(Debug, Clone)]
struct WorkingBin {
    bin_index: usize,
    items: Vec<WorkingItem>,
}

/// Mutable working representation of a partial solution during the
/// local-search improvement phase. `unplaced` is carried through
/// unchanged — local search never picks up or drops items, it only
/// shuffles already-placed ones.
#[derive(Debug, Clone)]
struct WorkingState {
    bins: Vec<WorkingBin>,
    unplaced: Vec<BoxDemand3D>,
}

impl WorkingState {
    /// Decompose a [`ThreeDSolution`] into a [`WorkingState`]. Returns
    /// `None` if any layout bin-name or placement name cannot be
    /// resolved against `problem`.
    fn from_solution(solution: &ThreeDSolution, problem: &ThreeDProblem) -> Option<Self> {
        let mut bins: Vec<WorkingBin> = Vec::with_capacity(solution.layouts.len());
        let mut remaining_by_demand: Vec<usize> =
            problem.demands.iter().map(|demand| demand.quantity).collect();
        for layout in &solution.layouts {
            let bin_index = problem.bins.iter().position(|bin| bin.name == layout.bin_name)?;
            let mut items: Vec<WorkingItem> = Vec::with_capacity(layout.placements.len());
            for placement in &layout.placements {
                let demand_index =
                    problem.demands.iter().enumerate().position(|(index, demand)| {
                        remaining_by_demand[index] > 0
                            && placement_matches_demand(placement, demand)
                    })?;
                remaining_by_demand[demand_index] =
                    remaining_by_demand[demand_index].saturating_sub(1);
                let demand = &problem.demands[demand_index];
                items.push(WorkingItem {
                    name: placement.name.clone(),
                    original_w: demand.width,
                    original_h: demand.height,
                    original_d: demand.depth,
                    allowed_rotations: demand.allowed_rotations,
                    placement: placement.clone(),
                });
            }
            bins.push(WorkingBin { bin_index, items });
        }
        Some(Self { bins, unplaced: solution.unplaced.clone() })
    }

    /// Build a [`ThreeDSolution`] snapshot of the current working
    /// state via [`build_solution`]. Returns `None` if
    /// `build_solution` fails.
    fn to_solution(&self, problem: &ThreeDProblem) -> Option<ThreeDSolution> {
        let bin_placements: Vec<(usize, Vec<Placement3D>)> = self
            .bins
            .iter()
            .map(|bin| (bin.bin_index, bin.items.iter().map(|it| it.placement.clone()).collect()))
            .collect();
        let unplaced_items = self.unplaced.iter().map(box_demand_to_instance).collect();
        let metrics = SolverMetrics3D::default();
        build_solution(
            "local_search",
            &problem.bins,
            bin_placements,
            unplaced_items,
            metrics,
            false,
        )
        .ok()
    }

    /// Collect all placements in `bin_index` other than `skip_item`.
    fn placements_in_bin_excluding(&self, bin_index: usize, skip_item: usize) -> Vec<Placement3D> {
        self.bins[bin_index]
            .items
            .iter()
            .enumerate()
            .filter_map(|(i, it)| if i == skip_item { None } else { Some(it.placement.clone()) })
            .collect()
    }
}

fn placement_matches_demand(placement: &Placement3D, demand: &BoxDemand3D) -> bool {
    if placement.name != demand.name || !demand.allowed_rotations.contains(placement.rotation) {
        return false;
    }
    let (width, height, depth) =
        placement.rotation.apply(demand.width, demand.height, demand.depth);
    (placement.width, placement.height, placement.depth) == (width, height, depth)
}

/// Translate a [`BoxDemand3D`] back into an
/// [`ItemInstance3D`](super::model::ItemInstance3D) for
/// `build_solution`'s unplaced-list input.
fn box_demand_to_instance(demand: &BoxDemand3D) -> super::model::ItemInstance3D {
    super::model::ItemInstance3D {
        demand_index: 0,
        name: demand.name.clone(),
        width: demand.width,
        height: demand.height,
        depth: demand.depth,
        allowed_rotations: demand.allowed_rotations,
    }
}

/// Collect the extreme points of a bin given its current placements.
///
/// This is a simplified version of the EP projector: it returns
/// `(0, 0, 0)` plus, for every placed box, its three "far-face"
/// corners `(x + w, y, z)`, `(x, y + h, z)`, and `(x, y, z + d)`.
/// That is not a full EP lattice but it is a superset sufficient for
/// local search: the original placement was feasible, so any
/// improvement move either reaches the destination via one of these
/// anchor points or is at least as good under another anchor the FFD
/// seed already explored.
fn candidate_extreme_points(placements: &[Placement3D], bin: &Bin3D) -> Vec<(u32, u32, u32)> {
    let mut eps: BTreeSet<(u32, u32, u32)> = BTreeSet::new();
    eps.insert((0, 0, 0));
    for placement in placements {
        let x_end = placement.x.saturating_add(placement.width);
        let y_end = placement.y.saturating_add(placement.height);
        let z_end = placement.z.saturating_add(placement.depth);
        if x_end <= bin.width {
            eps.insert((x_end, placement.y, placement.z));
        }
        if y_end <= bin.height {
            eps.insert((placement.x, y_end, placement.z));
        }
        if z_end <= bin.depth {
            eps.insert((placement.x, placement.y, z_end));
        }
    }
    eps.into_iter().collect()
}

/// Attempt a single improving *move* across all placed items. Returns
/// `true` if a move was committed.
///
/// Iterates items in each bin and, for every other open bin, tries
/// every `(extreme-point, rotation)` feasible placement. Accepts the
/// first move that strictly improves the current best.
fn try_move_once(
    working: &mut WorkingState,
    problem: &ThreeDProblem,
    current_best: &ThreeDSolution,
) -> Option<ThreeDSolution> {
    let bin_count = working.bins.len();
    for src_bin in 0..bin_count {
        let item_count = working.bins[src_bin].items.len();
        for src_item in 0..item_count {
            let item_snapshot = working.bins[src_bin].items[src_item].clone();
            for dst_bin in 0..bin_count {
                if dst_bin == src_bin {
                    continue;
                }
                let dst_placements: Vec<Placement3D> =
                    working.bins[dst_bin].items.iter().map(|it| it.placement.clone()).collect();
                let dst_bin_type = &problem.bins[working.bins[dst_bin].bin_index];
                let eps = candidate_extreme_points(&dst_placements, dst_bin_type);
                for (rotation, w, h, d) in item_snapshot.orientations() {
                    for &(ex, ey, ez) in &eps {
                        if !placement_feasible(
                            ex,
                            ey,
                            ez,
                            w,
                            h,
                            d,
                            dst_bin_type.width,
                            dst_bin_type.height,
                            dst_bin_type.depth,
                            &dst_placements,
                        ) {
                            continue;
                        }
                        // Tentatively apply the move.
                        let mut trial = working.clone();
                        let moved_item = trial.bins[src_bin].items.swap_remove(src_item);
                        let new_placement = Placement3D {
                            name: moved_item.name.clone(),
                            x: ex,
                            y: ey,
                            z: ez,
                            width: w,
                            height: h,
                            depth: d,
                            rotation,
                        };
                        trial.bins[dst_bin]
                            .items
                            .push(WorkingItem { placement: new_placement, ..moved_item });
                        // Drop any bin that became empty.
                        trial.bins.retain(|bin| !bin.items.is_empty());
                        if let Some(snapshot) = trial.to_solution(problem)
                            && snapshot.is_better_than(current_best)
                        {
                            *working = trial;
                            return Some(snapshot);
                        }
                    }
                }
            }
        }
    }
    None
}

/// Attempt a single improving *rotate* across all placed items.
fn try_rotate_once(
    working: &mut WorkingState,
    problem: &ThreeDProblem,
    current_best: &ThreeDSolution,
) -> Option<ThreeDSolution> {
    let bin_count = working.bins.len();
    for bin_idx in 0..bin_count {
        let item_count = working.bins[bin_idx].items.len();
        for item_idx in 0..item_count {
            let item_snapshot = working.bins[bin_idx].items[item_idx].clone();
            let others: Vec<Placement3D> = working.placements_in_bin_excluding(bin_idx, item_idx);
            let bin_type = &problem.bins[working.bins[bin_idx].bin_index];
            let current_rotation = item_snapshot.placement.rotation;
            for (rotation, w, h, d) in item_snapshot.orientations() {
                if rotation == current_rotation {
                    continue;
                }
                let anchor_x = item_snapshot.placement.x;
                let anchor_y = item_snapshot.placement.y;
                let anchor_z = item_snapshot.placement.z;
                if !placement_feasible(
                    anchor_x,
                    anchor_y,
                    anchor_z,
                    w,
                    h,
                    d,
                    bin_type.width,
                    bin_type.height,
                    bin_type.depth,
                    &others,
                ) {
                    continue;
                }
                let mut trial = working.clone();
                let new_placement = Placement3D {
                    name: item_snapshot.name.clone(),
                    x: anchor_x,
                    y: anchor_y,
                    z: anchor_z,
                    width: w,
                    height: h,
                    depth: d,
                    rotation,
                };
                trial.bins[bin_idx].items[item_idx].placement = new_placement;
                if let Some(snapshot) = trial.to_solution(problem)
                    && snapshot.is_better_than(current_best)
                {
                    *working = trial;
                    return Some(snapshot);
                }
            }
        }
    }
    None
}

/// Attempt a single improving *swap* across pairs of items in
/// different bins.
fn try_swap_once(
    working: &mut WorkingState,
    problem: &ThreeDProblem,
    current_best: &ThreeDSolution,
) -> Option<ThreeDSolution> {
    let bin_count = working.bins.len();
    for bin_a in 0..bin_count {
        for bin_b in (bin_a + 1)..bin_count {
            let items_a = working.bins[bin_a].items.len();
            let items_b = working.bins[bin_b].items.len();
            for ia in 0..items_a {
                for ib in 0..items_b {
                    if let Some(snapshot) =
                        try_apply_swap(working, problem, current_best, bin_a, ia, bin_b, ib)
                    {
                        return Some(snapshot);
                    }
                }
            }
        }
    }
    None
}

#[allow(clippy::too_many_arguments)] // Internal helper: explicit indices keep the caller readable.
fn try_apply_swap(
    working: &mut WorkingState,
    problem: &ThreeDProblem,
    current_best: &ThreeDSolution,
    bin_a: usize,
    ia: usize,
    bin_b: usize,
    ib: usize,
) -> Option<ThreeDSolution> {
    let item_a = working.bins[bin_a].items[ia].clone();
    let item_b = working.bins[bin_b].items[ib].clone();
    let bin_a_type = &problem.bins[working.bins[bin_a].bin_index];
    let bin_b_type = &problem.bins[working.bins[bin_b].bin_index];

    // Placements around the swap target (without the outgoing item).
    let others_a: Vec<Placement3D> = working.placements_in_bin_excluding(bin_a, ia);
    let others_b: Vec<Placement3D> = working.placements_in_bin_excluding(bin_b, ib);

    // Anchor point = upper-left-near corner of the item being evicted.
    let anchor_a = (item_a.placement.x, item_a.placement.y, item_a.placement.z);
    let anchor_b = (item_b.placement.x, item_b.placement.y, item_b.placement.z);

    for (rot_b_into_a, w_b, h_b, d_b) in item_b.orientations() {
        if !placement_feasible(
            anchor_a.0,
            anchor_a.1,
            anchor_a.2,
            w_b,
            h_b,
            d_b,
            bin_a_type.width,
            bin_a_type.height,
            bin_a_type.depth,
            &others_a,
        ) {
            continue;
        }
        for (rot_a_into_b, w_a, h_a, d_a) in item_a.orientations() {
            if !placement_feasible(
                anchor_b.0,
                anchor_b.1,
                anchor_b.2,
                w_a,
                h_a,
                d_a,
                bin_b_type.width,
                bin_b_type.height,
                bin_b_type.depth,
                &others_b,
            ) {
                continue;
            }
            // Double-check the two new placements don't collide with
            // each other in their respective bins (the
            // placements_in_bin_excluding snapshot already excluded
            // the outgoing item, so this is just overlap with the
            // rest). Redundantly guard with placements_overlap to
            // future-proof against any `placement_feasible` tweak.
            let new_a = Placement3D {
                name: item_b.name.clone(),
                x: anchor_a.0,
                y: anchor_a.1,
                z: anchor_a.2,
                width: w_b,
                height: h_b,
                depth: d_b,
                rotation: rot_b_into_a,
            };
            let new_b = Placement3D {
                name: item_a.name.clone(),
                x: anchor_b.0,
                y: anchor_b.1,
                z: anchor_b.2,
                width: w_a,
                height: h_a,
                depth: d_a,
                rotation: rot_a_into_b,
            };
            if others_a.iter().any(|other| placements_overlap(&new_a, other)) {
                continue;
            }
            if others_b.iter().any(|other| placements_overlap(&new_b, other)) {
                continue;
            }
            let mut trial = working.clone();
            trial.bins[bin_a].items[ia] = WorkingItem {
                name: item_b.name.clone(),
                original_w: item_b.original_w,
                original_h: item_b.original_h,
                original_d: item_b.original_d,
                allowed_rotations: item_b.allowed_rotations,
                placement: new_a,
            };
            trial.bins[bin_b].items[ib] = WorkingItem {
                name: item_a.name.clone(),
                original_w: item_a.original_w,
                original_h: item_a.original_h,
                original_d: item_a.original_d,
                allowed_rotations: item_a.allowed_rotations,
                placement: new_b,
            };
            if let Some(snapshot) = trial.to_solution(problem)
                && snapshot.is_better_than(current_best)
            {
                *working = trial;
                return Some(snapshot);
            }
        }
    }
    None
}

/// Attempt to empty the bin with the smallest `used_volume` by
/// relocating every item in it to some other bin. Returns the rebuilt
/// snapshot iff the bin was successfully removed from `working` *and*
/// the new configuration strictly improves `current_best`.
fn try_bin_elimination(
    working: &mut WorkingState,
    problem: &ThreeDProblem,
    current_best: &ThreeDSolution,
) -> Option<ThreeDSolution> {
    if working.bins.len() < 2 {
        return None;
    }

    // Pick the bin with the smallest total placement volume.
    let target = working
        .bins
        .iter()
        .enumerate()
        .min_by_key(|(_, bin)| {
            bin.items
                .iter()
                .map(|it| volume_u64(it.placement.width, it.placement.height, it.placement.depth))
                .sum::<u64>()
        })
        .map(|(idx, _)| idx);
    let target_bin = target?;

    // Tentatively work on a clone: if all items from the target bin
    // can be relocated elsewhere, commit the clone.
    let mut trial = working.clone();
    let items_to_move: Vec<WorkingItem> = std::mem::take(&mut trial.bins[target_bin].items);

    for item in items_to_move {
        let placed = relocate_item_into_any_other_bin(&mut trial, problem, target_bin, &item);
        if !placed {
            // Relocation failed: abandon the trial.
            return None;
        }
    }

    // All items moved — remove the now-empty target bin.
    trial.bins.swap_remove(target_bin);
    let snapshot = trial.to_solution(problem)?;
    if snapshot.is_better_than(current_best) {
        *working = trial;
        Some(snapshot)
    } else {
        None
    }
}

/// Find any feasible `(destination bin, extreme point, rotation)` for
/// `item` excluding `skip_bin`, and install it there. Returns `true`
/// on success.
fn relocate_item_into_any_other_bin(
    working: &mut WorkingState,
    problem: &ThreeDProblem,
    skip_bin: usize,
    item: &WorkingItem,
) -> bool {
    let bin_count = working.bins.len();
    for dst_bin in 0..bin_count {
        if dst_bin == skip_bin {
            continue;
        }
        let dst_placements: Vec<Placement3D> =
            working.bins[dst_bin].items.iter().map(|it| it.placement.clone()).collect();
        let dst_bin_type = &problem.bins[working.bins[dst_bin].bin_index];
        let eps = candidate_extreme_points(&dst_placements, dst_bin_type);
        for (rotation, w, h, d) in item.orientations() {
            for &(ex, ey, ez) in &eps {
                if placement_feasible(
                    ex,
                    ey,
                    ez,
                    w,
                    h,
                    d,
                    dst_bin_type.width,
                    dst_bin_type.height,
                    dst_bin_type.depth,
                    &dst_placements,
                ) {
                    let new_placement = Placement3D {
                        name: item.name.clone(),
                        x: ex,
                        y: ey,
                        z: ez,
                        width: w,
                        height: h,
                        depth: d,
                        rotation,
                    };
                    working.bins[dst_bin].items.push(WorkingItem {
                        name: item.name.clone(),
                        original_w: item.original_w,
                        original_h: item.original_h,
                        original_d: item.original_d,
                        allowed_rotations: item.allowed_rotations,
                        placement: new_placement,
                    });
                    return true;
                }
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::three_d::model::{Bin3D, BoxDemand3D, RotationMask3D, ThreeDAlgorithm};

    fn options_with_seed(seed: u64) -> ThreeDOptions {
        ThreeDOptions {
            algorithm: ThreeDAlgorithm::LocalSearch,
            multistart_runs: 2,
            improvement_rounds: 4,
            seed: Some(seed),
            ..ThreeDOptions::default()
        }
    }

    fn trivial_problem() -> ThreeDProblem {
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

    fn multi_bin_problem() -> ThreeDProblem {
        ThreeDProblem {
            bins: vec![Bin3D {
                name: "b".into(),
                width: 5,
                height: 5,
                depth: 5,
                cost: 1.0,
                quantity: None,
            }],
            demands: vec![BoxDemand3D {
                name: "cube".into(),
                width: 5,
                height: 5,
                depth: 5,
                quantity: 3,
                allowed_rotations: RotationMask3D::XYZ,
            }],
        }
    }

    #[test]
    fn local_search_trivial_fit() {
        let problem = trivial_problem();
        let options = options_with_seed(1);
        let solution = solve_local_search(&problem, &options).expect("solve");
        assert!(solution.unplaced.is_empty());
        assert_eq!(solution.bin_count, 1);
    }

    #[test]
    fn local_search_opens_multiple_bins_when_required() {
        let problem = multi_bin_problem();
        let options = options_with_seed(42);
        let solution = solve_local_search(&problem, &options).expect("solve");
        assert!(solution.unplaced.is_empty());
        assert_eq!(solution.bin_count, 3);
    }

    #[test]
    fn local_search_algorithm_name_is_local_search() {
        let problem = trivial_problem();
        let options = options_with_seed(7);
        let solution = solve_local_search(&problem, &options).expect("solve");
        assert_eq!(solution.algorithm, "local_search");
    }

    #[test]
    fn local_search_never_regresses_versus_ffd_seed() {
        // Hand-crafted workload with mixed box shapes so the initial
        // FFD seed is unlikely to be an immediate local optimum.
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
                    width: 6,
                    height: 6,
                    depth: 6,
                    quantity: 2,
                    allowed_rotations: RotationMask3D::ALL,
                },
                BoxDemand3D {
                    name: "mid".into(),
                    width: 3,
                    height: 4,
                    depth: 5,
                    quantity: 4,
                    allowed_rotations: RotationMask3D::ALL,
                },
                BoxDemand3D {
                    name: "tile".into(),
                    width: 2,
                    height: 2,
                    depth: 2,
                    quantity: 6,
                    allowed_rotations: RotationMask3D::ALL,
                },
            ],
        };
        let ffd_options = ThreeDOptions {
            algorithm: ThreeDAlgorithm::FirstFitDecreasingVolume,
            ..ThreeDOptions::default()
        };
        let ffd = solve_first_fit_decreasing_volume(&problem, &ffd_options).expect("ffd");
        let ls_options = options_with_seed(123);
        let ls = solve_local_search(&problem, &ls_options).expect("ls");
        assert!(!ffd.is_better_than(&ls), "local search must be at least as good as its FFD seed");
    }

    #[test]
    fn local_search_is_deterministic_under_seed() {
        let problem = ThreeDProblem {
            bins: vec![Bin3D {
                name: "b".into(),
                width: 10,
                height: 10,
                depth: 10,
                cost: 1.0,
                quantity: None,
            }],
            demands: vec![BoxDemand3D {
                name: "mixed".into(),
                width: 3,
                height: 4,
                depth: 5,
                quantity: 6,
                allowed_rotations: RotationMask3D::ALL,
            }],
        };
        let options = options_with_seed(0xC0FFEE);
        let first = solve_local_search(&problem, &options).expect("first");
        let second = solve_local_search(&problem, &options).expect("second");
        let first_json = serde_json::to_string(&first).expect("serialize first");
        let second_json = serde_json::to_string(&second).expect("serialize second");
        assert_eq!(first_json, second_json);
    }

    #[test]
    fn working_state_matches_duplicate_names_by_dimensions_and_rotation() {
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
                    name: "dup".into(),
                    width: 2,
                    height: 2,
                    depth: 2,
                    quantity: 1,
                    allowed_rotations: RotationMask3D::XYZ,
                },
                BoxDemand3D {
                    name: "dup".into(),
                    width: 1,
                    height: 3,
                    depth: 1,
                    quantity: 1,
                    allowed_rotations: RotationMask3D::XYZ,
                },
            ],
        };
        let solution = ThreeDSolution {
            algorithm: "seed".into(),
            exact: false,
            lower_bound: None,
            guillotine: false,
            bin_count: 1,
            total_waste_volume: 997,
            total_cost: 1.0,
            layouts: vec![super::super::model::BinLayout3D {
                bin_name: "b".into(),
                width: 10,
                height: 10,
                depth: 10,
                cost: 1.0,
                placements: vec![Placement3D {
                    name: "dup".into(),
                    x: 0,
                    y: 0,
                    z: 0,
                    width: 1,
                    height: 3,
                    depth: 1,
                    rotation: Rotation3D::Xyz,
                }],
                used_volume: 3,
                waste_volume: 997,
            }],
            bin_requirements: Vec::new(),
            unplaced: vec![BoxDemand3D {
                name: "dup".into(),
                width: 2,
                height: 2,
                depth: 2,
                quantity: 1,
                allowed_rotations: RotationMask3D::XYZ,
            }],
            metrics: SolverMetrics3D::default(),
        };

        let working =
            WorkingState::from_solution(&solution, &problem).expect("working state should build");
        let item = &working.bins[0].items[0];
        assert_eq!((item.original_w, item.original_h, item.original_d), (1, 3, 1));
    }
}
