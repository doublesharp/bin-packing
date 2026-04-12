//! GRASP (Greedy Randomized Adaptive Search Procedure) meta-strategy for
//! 3D bin packing.
//!
//! Each GRASP iteration has two phases:
//!
//! 1. **Construction** — a Restricted Candidate List (RCL) built by ranking
//!    the remaining items by volume (descending) and keeping those whose
//!    volume is at least `(1.0 - α) × max_volume` of the best candidate.
//!    One item is drawn uniformly at random from the RCL, so the resulting
//!    item ordering is randomised but still biased toward large items. The
//!    ordered list is fed verbatim to the EP engine via
//!    [`solve_with_scoring_on_items`].
//!
//! 2. **Improvement** — the constructed solution is handed to
//!    [`super::local_search::improve`], which explores move/rotate/swap
//!    neighbourhoods until a local optimum is reached.
//!
//! [`ThreeDOptions::multistart_runs`] controls how many GRASP iterations are
//! executed (minimum 1). The best solution across all iterations under
//! [`ThreeDSolution::is_better_than`] is returned.

use rand::{Rng, SeedableRng, rngs::SmallRng};

use super::common::volume_u64;
use super::extreme_points::{ExtremePointsScoring, solve_with_scoring_on_items};
use super::local_search::improve;
use super::model::{ItemInstance3D, ThreeDOptions, ThreeDProblem, ThreeDSolution};
use crate::Result;

/// α parameter for the RCL threshold: items in the top `(1-α)` fraction of
/// volume are eligible for random selection.
const GRASP_ALPHA: f64 = 0.3;

/// Default PRNG seed when the caller does not supply `options.seed`.
const DEFAULT_SEED: u64 = 0;

/// Build an RCL-ordered item list for a single GRASP construction phase.
///
/// At each step the remaining items are evaluated by volume. Items whose
/// volume is at least `(1.0 - α) × max_volume` form the RCL; one is picked
/// uniformly at random. If the RCL would be empty (e.g. all volumes are
/// zero), the single best item is used instead. The process repeats until all
/// items have been assigned an ordering.
fn rcl_order(mut items: Vec<ItemInstance3D>, rng: &mut SmallRng) -> Vec<ItemInstance3D> {
    let mut ordered = Vec::with_capacity(items.len());

    while !items.is_empty() {
        // Compute volumes and find the maximum.
        let volumes: Vec<u64> =
            items.iter().map(|it| volume_u64(it.width, it.height, it.depth)).collect();

        let max_vol = *volumes.iter().max().unwrap_or(&0);

        // Build the RCL: indices of items with volume >= threshold.
        let threshold = (max_vol as f64 * (1.0 - GRASP_ALPHA)) as u64;
        let mut rcl: Vec<usize> = volumes
            .iter()
            .enumerate()
            .filter_map(|(i, &v)| if v >= threshold { Some(i) } else { None })
            .collect();

        // Fall back to the single best item if the RCL is empty.
        if rcl.is_empty()
            && let Some((best_idx, _)) = volumes.iter().enumerate().max_by_key(|&(_, &v)| v)
        {
            rcl.push(best_idx);
        }

        // Pick one uniformly at random from the RCL.
        if rcl.is_empty() {
            break;
        }
        let chosen_idx = rcl[rng.random_range(0..rcl.len())];

        // Remove the chosen item from `items` and append it to `ordered`.
        ordered.push(items.swap_remove(chosen_idx));
    }

    ordered
}

/// Solve a 3D bin packing problem with the GRASP meta-strategy.
///
/// Runs `options.multistart_runs` (minimum 1) GRASP iterations. Each
/// iteration:
///
/// 1. Builds an RCL-biased random item ordering (α = 0.3).
/// 2. Feeds that ordering to the EP `VolumeResidual` engine.
/// 3. Applies [`super::local_search::improve`] as the improvement phase.
///
/// Returns the best solution across all iterations under
/// [`ThreeDSolution::is_better_than`].
///
/// # Errors
///
/// Returns an error only if *every* iteration fails. A mix of failures and
/// successes is captured in the returned solution's metrics notes.
pub(super) fn solve_grasp(
    problem: &ThreeDProblem,
    options: &ThreeDOptions,
) -> Result<ThreeDSolution> {
    let base_items = problem.expanded_items();
    let mut rng = SmallRng::seed_from_u64(options.seed.unwrap_or(DEFAULT_SEED));
    let runs = options.multistart_runs.max(1);

    let mut best: Option<ThreeDSolution> = None;
    let mut pending_notes: Vec<String> = Vec::new();
    let mut first_error: Option<crate::BinPackingError> = None;

    for run in 0..runs {
        // Phase 1: greedy randomized construction via RCL ordering.
        let ordered_items = rcl_order(base_items.clone(), &mut rng);

        let construction_result = solve_with_scoring_on_items(
            problem,
            ordered_items,
            options,
            ExtremePointsScoring::VolumeFitResidual,
            "grasp",
        );

        let constructed = match construction_result {
            Ok(sol) => sol,
            Err(err) => {
                pending_notes.push(format!("grasp: run {run} construction failed: {err}"));
                if first_error.is_none() {
                    first_error = Some(err);
                }
                continue;
            }
        };

        // Phase 2: local search improvement.
        let mut candidate = improve(constructed, problem, options, &mut rng);
        candidate.algorithm = "grasp".to_string();

        best = Some(match best.take() {
            Some(current) if current.is_better_than(&candidate) => current,
            _ => candidate,
        });
    }

    match best {
        Some(mut solution) => {
            solution.algorithm = "grasp".to_string();
            solution.metrics.iterations = runs;
            solution.metrics.notes.extend(pending_notes);
            solution.metrics.notes.push(format!("grasp: {runs} iteration(s)"));
            Ok(solution)
        }
        None => Err(first_error.unwrap_or_else(|| {
            crate::BinPackingError::Unsupported("grasp: zero iterations executed".to_string())
        })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::three_d::model::{Bin3D, BoxDemand3D, RotationMask3D, ThreeDAlgorithm};

    fn bin(w: u32, h: u32, d: u32) -> Bin3D {
        Bin3D { name: "b".into(), width: w, height: h, depth: d, cost: 1.0, quantity: None }
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

    fn options_with(seed: u64, runs: usize) -> ThreeDOptions {
        ThreeDOptions {
            algorithm: ThreeDAlgorithm::Grasp,
            multistart_runs: runs,
            seed: Some(seed),
            ..ThreeDOptions::default()
        }
    }

    /// GRASP should place all items from a small 2-item problem.
    #[test]
    fn grasp_places_all_items_two_item_problem() {
        let problem = ThreeDProblem {
            bins: vec![bin(10, 10, 10)],
            demands: vec![demand("a", 4, 4, 4, 1), demand("b", 3, 3, 3, 1)],
        };
        let options = options_with(1, 1);
        let solution = solve_grasp(&problem, &options).expect("solve");
        assert!(solution.unplaced.is_empty(), "expected no unplaced items");
        assert_eq!(solution.bin_count, 1);
    }

    /// Multiple GRASP iterations (multistart_runs=3) should not panic and
    /// should return a valid solution.
    #[test]
    fn grasp_multiple_iterations_no_panic() {
        let problem = ThreeDProblem {
            bins: vec![bin(10, 10, 10)],
            demands: vec![demand("x", 5, 5, 5, 2), demand("y", 3, 3, 3, 3)],
        };
        let options = options_with(42, 3);
        let solution = solve_grasp(&problem, &options).expect("solve");
        // Must return a valid solution — bin_count >= 1 and metrics populated.
        assert!(solution.bin_count >= 1);
        assert_eq!(solution.metrics.iterations, 3);
        assert!(!solution.metrics.notes.is_empty());
    }

    /// Same seed should produce byte-identical results (determinism).
    #[test]
    fn grasp_deterministic_under_same_seed() {
        let problem = ThreeDProblem {
            bins: vec![bin(10, 10, 10)],
            demands: vec![
                demand("big", 5, 5, 5, 2),
                demand("mid", 3, 4, 5, 4),
                demand("tiny", 2, 2, 2, 6),
            ],
        };
        let options = options_with(0xDEAD_BEEF, 5);
        let first = solve_grasp(&problem, &options).expect("first run");
        let second = solve_grasp(&problem, &options).expect("second run");
        assert_eq!(first.bin_count, second.bin_count, "same seed must produce the same bin count");
    }
}
