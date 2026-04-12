//! Restricted Martello-Pisinger-Vigo branch-and-bound exact backend for 3D
//! bin packing.
//!
//! This solver is the only 3D algorithm that sets
//! [`ThreeDSolution::exact`] = `true` and populates
//! [`ThreeDSolution::lower_bound`] from integer L0/L1 bounds. It is
//! restricted to a narrow class of inputs (see
//! [`solve_branch_and_bound`]): a single uncapped bin type and demands
//! limited to the identity rotation ([`RotationMask3D::XYZ`]).
//!
//! Implements the MPV (2000) three-dimensional bin-packing B&B with the
//! branching rule "at each node, place the largest unplaced item into
//! each currently open bin (guided by the Extreme-Points placement
//! engine with volume-fit-residual scoring) or open a new bin". Search
//! is depth-first, bounded by
//! [`ThreeDOptions::branch_and_bound_node_limit`] expanded nodes. If the
//! incumbent matches the L2 lower bound, the search terminates early
//! with `exact = true`; otherwise it returns the best incumbent found
//! with `exact = false` and the lower bound still populated.

use std::collections::BTreeSet;

use super::common::{build_solution, volume_u64};
use super::extreme_points::{BinState, ExtremePointsScoring, try_place_into_bin};
use super::model::{
    ItemInstance3D, Placement3D, RotationMask3D, SolverMetrics3D, ThreeDOptions, ThreeDProblem,
    ThreeDSolution,
};
use crate::{BinPackingError, Result};

/// Solve a 3D bin packing problem with a restricted branch-and-bound exact
/// backend.
///
/// # Restrictions
///
/// Returns [`BinPackingError::Unsupported`] unless all of the following
/// hold:
///
/// * `problem.bins.len() == 1`
/// * `problem.bins[0].quantity.is_none()` (the single bin type is uncapped)
/// * every demand has `allowed_rotations == RotationMask3D::XYZ`
///
/// The error message lists the violated condition(s) via a structured
/// format so callers can match on substrings without parsing English.
///
/// # Algorithm
///
/// 1. Compute the volume lower bound `L0 = ceil(total_item_volume /
///    bin_volume)` using `u64` integer division — no floating point.
/// 2. Compute the fat-item lower bound `L1 = |{ items with all three
///    extents strictly greater than half the bin's matching extent }|`.
///    Such items cannot pair pairwise on any axis, so each one needs its
///    own bin.
/// 3. Report `lower_bound = Some(L2 as f64)` with `L2 = max(L0, L1)`.
/// 4. Depth-first search sorted-by-decreasing-volume item list. Each
///    node branches on "place next item into existing bin k" for every
///    open bin k plus "open a new bin for the item". Each branch uses
///    [`try_place_into_bin`] with
///    [`ExtremePointsScoring::VolumeFitResidual`] to pick the actual
///    anchor inside the selected bin.
/// 5. Track the incumbent by minimum bin count. When the incumbent
///    matches `L2` the search short-circuits with `exact = true`.
/// 6. When `branch_and_bound_node_limit` expanded nodes is reached the
///    search stops and returns the incumbent with `exact = false`.
///    `metrics.branch_and_bound_nodes` records the number of nodes
///    expanded.
///
/// # Errors
///
/// * [`BinPackingError::Unsupported`] for unsupported input shapes as
///   described above.
/// * Any error surfaced by [`build_solution`] (e.g. bin-count cap).
pub(super) fn solve_branch_and_bound(
    problem: &ThreeDProblem,
    options: &ThreeDOptions,
) -> Result<ThreeDSolution> {
    let rotated = problem.demands.iter().any(|d| d.allowed_rotations != RotationMask3D::XYZ);
    let capped = !problem.bins.is_empty() && problem.bins[0].quantity.is_some();
    let single_bin_type = problem.bins.len() == 1;

    if !single_bin_type || capped || rotated {
        return Err(BinPackingError::Unsupported(format!(
            "branch_and_bound only supports a single uncapped bin type with no rotation; \
             bins.len()={}, capped={}, rotated={}",
            problem.bins.len(),
            capped,
            rotated,
        )));
    }

    let bin = &problem.bins[0];
    let bin_volume = volume_u64(bin.width, bin.height, bin.depth);
    debug_assert!(bin_volume > 0, "validated bin must have positive volume");

    // Expanded item list, sorted by decreasing volume to drive the
    // branch order "largest unplaced first" used by MPV.
    let mut items = problem.expanded_items();
    items.sort_by(|a, b| {
        let lhs = volume_u64(a.width, a.height, a.depth);
        let rhs = volume_u64(b.width, b.height, b.depth);
        rhs.cmp(&lhs)
    });

    // L0: volume lower bound via integer ceiling division.
    let total_item_volume: u64 =
        items.iter().map(|item| volume_u64(item.width, item.height, item.depth)).sum();
    let l0: u64 = if total_item_volume == 0 {
        0
    } else {
        // ceil(total_item_volume / bin_volume) via widened integer math.
        total_item_volume.div_ceil(bin_volume)
    };

    // L1: fat-item lower bound. Each item whose width, height, and
    // depth all strictly exceed half of the bin's matching extent
    // cannot pair pairwise on any axis, so it forces its own bin.
    //
    // Using `2 * extent > bin_extent` is equivalent to
    // `extent > bin_extent / 2` but avoids the floor from integer
    // division. Widening to `u64` keeps the comparison exact for
    // extents up to `MAX_DIMENSION_3D = 1 << 15`.
    let half_w_gated = |w: u32| u64::from(w).saturating_mul(2) > u64::from(bin.width);
    let half_h_gated = |h: u32| u64::from(h).saturating_mul(2) > u64::from(bin.height);
    let half_d_gated = |d: u32| u64::from(d).saturating_mul(2) > u64::from(bin.depth);
    let l1: u64 = items
        .iter()
        .filter(|item| {
            half_w_gated(item.width) && half_h_gated(item.height) && half_d_gated(item.depth)
        })
        .count() as u64;

    let l2: u64 = l0.max(l1);

    // Depth-first branch-and-bound search.
    let node_limit = options.branch_and_bound_node_limit;
    let mut search = SearchContext {
        problem,
        items: &items,
        node_limit,
        nodes_expanded: 0,
        incumbent: None,
        l2,
        reached_limit: false,
    };
    search.dfs(0, Vec::new());

    let SearchContext { nodes_expanded, incumbent, reached_limit, .. } = search;

    // If no feasible incumbent was found (e.g. the node budget was
    // exhausted before *any* complete leaf), fall back to a greedy
    // sequential placement with the same branching rule so we can at
    // least return a valid solution with `exact = false`.
    let final_states = match incumbent {
        Some(states) => states,
        None => greedy_fallback(problem, &items)?,
    };

    // Assemble the solution via the shared builder.
    let metrics = SolverMetrics3D {
        iterations: 1,
        explored_states: nodes_expanded,
        extreme_points_generated: 0,
        branch_and_bound_nodes: nodes_expanded,
        notes: vec![format!(
            "branch_and_bound: L0={l0}, L1={l1}, L2={l2}, nodes={nodes_expanded}, \
             limit_reached={reached_limit}"
        )],
    };

    let bin_placements: Vec<(usize, Vec<Placement3D>)> =
        final_states.into_iter().map(|state| (state.bin_index, state.placements)).collect();
    let bin_count = bin_placements.len() as u64;

    let mut solution = build_solution(
        "branch_and_bound",
        &problem.bins,
        bin_placements,
        Vec::new(),
        metrics,
        false,
    )?;
    // Override the two fields the exact backend owns end-to-end.
    solution.exact = !reached_limit && bin_count == l2;
    solution.lower_bound = Some(l2 as f64);
    Ok(solution)
}

// ---------------------------------------------------------------------------
// Search context
// ---------------------------------------------------------------------------

/// Shared state threaded through the depth-first branch-and-bound search.
struct SearchContext<'a> {
    problem: &'a ThreeDProblem,
    items: &'a [ItemInstance3D],
    node_limit: usize,
    nodes_expanded: usize,
    incumbent: Option<Vec<CloneableBinState>>,
    l2: u64,
    reached_limit: bool,
}

impl SearchContext<'_> {
    /// Whether the search has already found a provably optimal solution.
    fn is_proven_optimal(&self) -> bool {
        matches!(&self.incumbent, Some(states) if (states.len() as u64) == self.l2)
    }

    /// Whether the node budget is exhausted.
    fn budget_exhausted(&self) -> bool {
        self.nodes_expanded >= self.node_limit
    }

    /// Record a new incumbent if it improves the best known bin count.
    fn record_incumbent(&mut self, states: &[CloneableBinState]) {
        let bin_count = states.len();
        let better = match &self.incumbent {
            None => true,
            Some(current) => bin_count < current.len(),
        };
        if better {
            self.incumbent = Some(states.to_vec());
        }
    }

    /// Depth-first exploration starting at item index `item_index` with
    /// the bin states accumulated so far.
    fn dfs(&mut self, item_index: usize, states: Vec<CloneableBinState>) {
        if self.is_proven_optimal() {
            return;
        }
        if self.budget_exhausted() {
            self.reached_limit = true;
            return;
        }

        // Prune: if we already have an incumbent at least as good as the
        // current bin count, no extension of this branch can beat it.
        if let Some(current) = &self.incumbent
            && states.len() >= current.len()
        {
            return;
        }

        // Leaf: all items placed. Record the incumbent.
        if item_index >= self.items.len() {
            self.record_incumbent(&states);
            return;
        }

        // Count this as an expanded node.
        self.nodes_expanded = self.nodes_expanded.saturating_add(1);

        let item = &self.items[item_index];
        let bin = &self.problem.bins[0];

        // Branch 1..N: place the item into each existing open bin, if
        // the EP engine admits a placement.
        for bin_position in 0..states.len() {
            if self.is_proven_optimal() || self.budget_exhausted() {
                break;
            }
            let mut child_states = clone_states(&states);
            let child = &mut child_states[bin_position];
            let mut working: BinState = child.to_bin_state();
            if try_place_into_bin(&mut working, bin, item, ExtremePointsScoring::VolumeFitResidual)
                .is_some()
            {
                *child = CloneableBinState::from_bin_state(&working);
                self.dfs(item_index + 1, child_states);
            }
        }

        if self.is_proven_optimal() || self.budget_exhausted() {
            if self.budget_exhausted() {
                self.reached_limit = true;
            }
            return;
        }

        // Branch N+1: open a new bin and place the item there. Since we
        // already vetted `ensure_feasible_demands` upstream and the
        // single-rotation restriction, a fresh bin must accept the item
        // when it fits — otherwise the problem would have been rejected
        // as infeasible. Guard defensively anyway.
        let new_bin_count = states.len() + 1;
        // Prune: a new bin can only improve things if the resulting
        // count is still strictly better than the incumbent.
        let prune_new_bin = matches!(
            &self.incumbent,
            Some(current) if new_bin_count >= current.len()
        );
        if prune_new_bin {
            return;
        }

        let mut fresh = BinState::new(0);
        if try_place_into_bin(&mut fresh, bin, item, ExtremePointsScoring::VolumeFitResidual)
            .is_some()
        {
            let mut child_states = clone_states(&states);
            child_states.push(CloneableBinState::from_bin_state(&fresh));
            self.dfs(item_index + 1, child_states);
        }
        // If a fresh bin cannot hold the item (which would indicate an
        // upstream validation gap), we simply drop this branch. The
        // overall search still terminates with whatever incumbent is
        // currently recorded.
    }
}

// ---------------------------------------------------------------------------
// Cloneable snapshot of `BinState`
// ---------------------------------------------------------------------------

/// Deep-clonable snapshot of an [`BinState`]. `BinState` itself is not
/// `Clone` (by design — it is owned by the EP engine's mutable loop),
/// so the branch-and-bound search uses this intermediate representation
/// to branch without aliasing mutable state.
#[derive(Clone)]
struct CloneableBinState {
    bin_index: usize,
    placements: Vec<Placement3D>,
    eps: BTreeSet<(u32, u32, u32)>,
    extreme_points_generated: usize,
}

impl CloneableBinState {
    fn from_bin_state(state: &BinState) -> Self {
        Self {
            bin_index: state.bin_index,
            placements: state.placements.clone(),
            eps: state.eps.clone(),
            extreme_points_generated: state.extreme_points_generated,
        }
    }

    fn to_bin_state(&self) -> BinState {
        BinState {
            bin_index: self.bin_index,
            placements: self.placements.clone(),
            eps: self.eps.clone(),
            extreme_points_generated: self.extreme_points_generated,
        }
    }
}

fn clone_states(states: &[CloneableBinState]) -> Vec<CloneableBinState> {
    states.to_vec()
}

// ---------------------------------------------------------------------------
// Greedy fallback
// ---------------------------------------------------------------------------

/// Fallback construction used when the DFS exhausts its node budget
/// without producing any complete leaf. Uses the same branching rule
/// (first try existing bins, then open a new one) but evaluates only
/// the greedy branch at each step so it is guaranteed to place every
/// item in `O(items * bins)` calls to [`try_place_into_bin`].
fn greedy_fallback(
    problem: &ThreeDProblem,
    items: &[ItemInstance3D],
) -> Result<Vec<CloneableBinState>> {
    let bin = &problem.bins[0];
    let mut states: Vec<BinState> = Vec::new();
    for item in items {
        let mut placed = false;
        for state in states.iter_mut() {
            if try_place_into_bin(state, bin, item, ExtremePointsScoring::VolumeFitResidual)
                .is_some()
            {
                placed = true;
                break;
            }
        }
        if placed {
            continue;
        }
        let mut fresh = BinState::new(0);
        if try_place_into_bin(&mut fresh, bin, item, ExtremePointsScoring::VolumeFitResidual)
            .is_some()
        {
            states.push(fresh);
            continue;
        }
        // Item cannot fit — this should have been caught by
        // `ensure_feasible_demands`. Surface as an unsupported error.
        return Err(BinPackingError::Unsupported(format!(
            "branch_and_bound greedy fallback: item `{}` cannot fit a fresh bin",
            item.name
        )));
    }
    Ok(states.iter().map(CloneableBinState::from_bin_state).collect())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::three_d::model::{Bin3D, BoxDemand3D, ThreeDAlgorithm};

    fn bin(name: &str, w: u32, h: u32, d: u32) -> Bin3D {
        Bin3D { name: name.into(), width: w, height: h, depth: d, cost: 1.0, quantity: None }
    }

    fn demand(name: &str, w: u32, h: u32, d: u32, quantity: usize) -> BoxDemand3D {
        BoxDemand3D {
            name: name.into(),
            width: w,
            height: h,
            depth: d,
            quantity,
            allowed_rotations: RotationMask3D::XYZ,
        }
    }

    fn default_options() -> ThreeDOptions {
        ThreeDOptions { algorithm: ThreeDAlgorithm::BranchAndBound, ..ThreeDOptions::default() }
    }

    #[test]
    fn optimal_trivial_fit_single_bin_proves_exact() {
        let problem = ThreeDProblem {
            bins: vec![bin("B", 4, 4, 4)],
            demands: vec![demand("cube", 2, 2, 2, 8)],
        };
        let options = default_options();
        let solution = solve_branch_and_bound(&problem, &options)
            .expect("eight 2-cubes fit exactly in a 4-cube");
        assert!(solution.unplaced.is_empty(), "no demand should be left over");
        assert_eq!(solution.bin_count, 1, "one bin suffices");
        assert!(solution.exact, "optimal solution must be marked exact");
        assert_eq!(solution.lower_bound, Some(1.0), "L2 = max(L0=1, L1=0) = 1");
    }

    #[test]
    fn rejects_multiple_bin_types_with_structured_message() {
        let problem = ThreeDProblem {
            bins: vec![bin("A", 10, 10, 10), bin("B", 20, 20, 20)],
            demands: vec![demand("x", 1, 1, 1, 1)],
        };
        let err = solve_branch_and_bound(&problem, &default_options())
            .expect_err("multi-bin should be unsupported");
        let BinPackingError::Unsupported(msg) = err else {
            panic!("expected Unsupported, got {err:?}");
        };
        assert!(msg.contains("bins.len()=2"), "structured message includes bin count: {msg}");
    }

    #[test]
    fn rejects_capped_bin_with_structured_message() {
        let mut capped_bin = bin("B", 10, 10, 10);
        capped_bin.quantity = Some(5);
        let problem =
            ThreeDProblem { bins: vec![capped_bin], demands: vec![demand("x", 1, 1, 1, 1)] };
        let err = solve_branch_and_bound(&problem, &default_options())
            .expect_err("capped bin should be unsupported");
        let BinPackingError::Unsupported(msg) = err else {
            panic!("expected Unsupported, got {err:?}");
        };
        assert!(msg.contains("capped=true"), "structured message flags capped: {msg}");
    }

    #[test]
    fn rejects_rotated_demand_with_structured_message() {
        let rotated_demand = BoxDemand3D {
            name: "r".into(),
            width: 2,
            height: 3,
            depth: 5,
            quantity: 1,
            allowed_rotations: RotationMask3D::ALL,
        };
        let problem =
            ThreeDProblem { bins: vec![bin("B", 10, 10, 10)], demands: vec![rotated_demand] };
        let err = solve_branch_and_bound(&problem, &default_options())
            .expect_err("rotated demand should be unsupported");
        let BinPackingError::Unsupported(msg) = err else {
            panic!("expected Unsupported, got {err:?}");
        };
        assert!(msg.contains("rotated=true"), "structured message flags rotation: {msg}");
    }

    #[test]
    fn node_limit_caps_expanded_nodes() {
        // Five non-trivial items force branching; a node limit of 1
        // should return a solution (via the greedy fallback or the
        // very first recorded incumbent) with `exact = false` and at
        // most one expanded node recorded in the metrics.
        let problem = ThreeDProblem {
            bins: vec![bin("B", 4, 4, 4)],
            demands: vec![demand("cube", 2, 2, 2, 5)],
        };
        let options = ThreeDOptions {
            algorithm: ThreeDAlgorithm::BranchAndBound,
            branch_and_bound_node_limit: 1,
            ..ThreeDOptions::default()
        };
        let solution = solve_branch_and_bound(&problem, &options)
            .expect("fallback must return a valid solution when the budget is tight");
        assert!(!solution.exact, "a single-node budget cannot prove optimality here");
        assert!(
            solution.metrics.branch_and_bound_nodes <= 1,
            "node count budgeted to 1, got {}",
            solution.metrics.branch_and_bound_nodes
        );
        assert_eq!(solution.lower_bound, Some(1.0));
    }
}
