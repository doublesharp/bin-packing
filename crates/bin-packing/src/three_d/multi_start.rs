//! Multi-start meta-strategy for 3D bin packing.
//!
//! Seeds a deterministic PRNG from [`ThreeDOptions::seed`] and runs the
//! Extreme Points volume-fit-residual engine on
//! [`ThreeDOptions::multistart_runs`] randomly shuffled permutations of the
//! expanded item list. The best candidate (ranked by
//! [`ThreeDSolution::is_better_than`]) is returned.
//!
//! Transient per-restart failures are captured in the returned solution's
//! [`SolverMetrics3D::notes`] rather than aborting the entire sweep — the
//! call only fails if *every* restart (including the first) errors out.

use rand::{SeedableRng, prelude::SliceRandom, rngs::SmallRng};

use crate::Result;
use crate::three_d::extreme_points::{ExtremePointsScoring, solve_with_scoring_on_items};
use crate::three_d::model::{ThreeDOptions, ThreeDProblem, ThreeDSolution};

/// Default PRNG seed when the caller does not supply `options.seed`.
///
/// Using a fixed default keeps the solver deterministic out of the box: two
/// calls with the same `(problem, options)` produce byte-identical solutions
/// regardless of whether the caller passed a seed.
const DEFAULT_SEED: u64 = 0;

/// Solve a 3D packing problem with the multi-start meta-strategy.
///
/// Shuffles the expanded item list `options.multistart_runs` times (minimum 1)
/// using a [`SmallRng`] seeded from `options.seed.unwrap_or(DEFAULT_SEED)`,
/// runs the Task 6 Extreme Points volume-fit-residual engine on each
/// permutation, and returns whichever candidate ranks highest under
/// [`ThreeDSolution::is_better_than`]. Ties are broken in favour of the
/// already-selected candidate (i.e. the earlier permutation wins).
///
/// # Errors
///
/// Returns an error only if *every* restart (including the first) fails. A
/// mix of failures and successes is captured as notes on the returned
/// solution so callers can observe transient placement issues without the
/// whole meta-strategy aborting.
pub(super) fn solve_multi_start(
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
        let mut trial_items = base_items.clone();
        trial_items.shuffle(&mut rng);

        match solve_with_scoring_on_items(
            problem,
            trial_items,
            options,
            ExtremePointsScoring::VolumeFitResidual,
            "multi_start",
        ) {
            Ok(candidate) => {
                best = Some(match best.take() {
                    Some(current) if current.is_better_than(&candidate) => current,
                    _ => candidate,
                });
            }
            Err(err) => {
                pending_notes.push(format!("multi_start: run {run} failed: {err}"));
                if first_error.is_none() {
                    first_error = Some(err);
                }
            }
        }
    }

    match best {
        Some(mut solution) => {
            solution.metrics.iterations = runs;
            solution.metrics.notes.extend(pending_notes);
            solution.metrics.notes.push(format!("multi_start: completed {runs} restart(s)"));
            Ok(solution)
        }
        None => Err(first_error.unwrap_or_else(|| {
            crate::BinPackingError::Unsupported("multi_start: zero restarts executed".to_string())
        })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::three_d::extreme_points::solve_extreme_points;
    use crate::three_d::model::{Bin3D, BoxDemand3D, RotationMask3D, ThreeDAlgorithm};

    fn options_with_seed(seed: u64, runs: usize) -> ThreeDOptions {
        ThreeDOptions {
            algorithm: ThreeDAlgorithm::MultiStart,
            multistart_runs: runs,
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

    #[test]
    fn multi_start_trivial_fit() {
        let problem = trivial_problem();
        let options = options_with_seed(1, 4);
        let solution = solve_multi_start(&problem, &options).expect("solve");
        assert!(solution.unplaced.is_empty());
        assert_eq!(solution.bin_count, 1);
    }

    #[test]
    fn multi_start_opens_multiple_bins() {
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
                name: "cube".into(),
                width: 5,
                height: 5,
                depth: 5,
                quantity: 3,
                allowed_rotations: RotationMask3D::XYZ,
            }],
        };
        let options = options_with_seed(42, 6);
        let solution = solve_multi_start(&problem, &options).expect("solve");
        assert!(solution.unplaced.is_empty());
        assert_eq!(solution.bin_count, 3);
    }

    #[test]
    fn multi_start_algorithm_name_is_multi_start() {
        let problem = trivial_problem();
        let options = options_with_seed(7, 3);
        let solution = solve_multi_start(&problem, &options).expect("solve");
        assert_eq!(solution.algorithm, "multi_start");
    }

    #[test]
    fn multi_start_is_deterministic_under_seed() {
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
                quantity: 8,
                allowed_rotations: RotationMask3D::ALL,
            }],
        };
        let options = options_with_seed(0xDEAD_BEEF, 5);
        let first = solve_multi_start(&problem, &options).expect("first run");
        let second = solve_multi_start(&problem, &options).expect("second run");
        let first_json = serde_json::to_string(&first).expect("serialize first");
        let second_json = serde_json::to_string(&second).expect("serialize second");
        assert_eq!(first_json, second_json);
    }

    #[test]
    fn multi_start_matches_or_beats_single_extreme_points_pass() {
        // A bin-packing instance where multi-start has a chance to improve on
        // the volume-descending EP ordering. Even when the improvement is a
        // tie, multi-start must never regress.
        let problem = ThreeDProblem {
            bins: vec![Bin3D {
                name: "b".into(),
                width: 8,
                height: 8,
                depth: 8,
                cost: 1.0,
                quantity: None,
            }],
            demands: vec![
                BoxDemand3D {
                    name: "big".into(),
                    width: 5,
                    height: 5,
                    depth: 5,
                    quantity: 2,
                    allowed_rotations: RotationMask3D::ALL,
                },
                BoxDemand3D {
                    name: "mid".into(),
                    width: 3,
                    height: 3,
                    depth: 3,
                    quantity: 4,
                    allowed_rotations: RotationMask3D::ALL,
                },
                BoxDemand3D {
                    name: "thin".into(),
                    width: 1,
                    height: 8,
                    depth: 2,
                    quantity: 3,
                    allowed_rotations: RotationMask3D::ALL,
                },
            ],
        };
        let options = options_with_seed(11, 16);

        let ep = solve_extreme_points(&problem, &ThreeDOptions::default()).expect("ep solve");
        let multi = solve_multi_start(&problem, &options).expect("multi solve");

        assert!(
            multi.is_better_than(&ep) || !ep.is_better_than(&multi),
            "multi_start regressed versus a single extreme_points pass",
        );
    }
}
