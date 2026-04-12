//! Auto-strategy for 3D bin packing.
//!
//! Runs a tiered multi-strategy sweep and returns the best solution under
//! [`ThreeDSolution::is_better_than`].
//!
//! **Tier 1 — fast sweep** (always): six deterministic construction
//! heuristics are run in sequence and the best result is kept.
//!
//! **Tier 2 — meta sweep** (when `options.multistart_runs > 0`): the
//! `MultiStart` and `LocalSearch` meta-strategies are added to the
//! candidate pool.
//!
//! If every candidate call errors, the last error is propagated. Otherwise
//! the best non-erroring result is returned with the winning leaf
//! algorithm preserved and a diagnostic note added to
//! `solution.metrics.notes`.

use super::model::{ThreeDOptions, ThreeDProblem, ThreeDSolution};
use crate::Result;

/// Solve a 3D bin-packing problem by trying several algorithms and returning
/// the best result under [`ThreeDSolution::is_better_than`].
///
/// # Errors
///
/// Propagates an error only if *every* candidate algorithm call fails.
pub(super) fn solve_auto(
    problem: &ThreeDProblem,
    options: &ThreeDOptions,
) -> Result<ThreeDSolution> {
    if options.guillotine_required {
        return solve_auto_guillotine(problem, options);
    }

    // Tier 1: fast deterministic construction heuristics.
    let candidates: &[SolverFn] = &[
        super::extreme_points::solve_extreme_points,
        super::extreme_points::solve_extreme_points_residual_space,
        super::extreme_points::solve_extreme_points_contact_point,
        super::guillotine::solve_guillotine_3d,
        super::layer::solve_layer_building,
        super::sorted::solve_first_fit_decreasing_volume,
    ];
    let mut results = Vec::with_capacity(candidates.len().saturating_add(2));
    for solver in candidates {
        results.push(solver(problem, options));
    }

    // Tier 2: meta-strategies (only when multistart_runs > 0).
    if options.multistart_runs > 0 {
        results.push(super::multi_start::solve_multi_start(problem, options));
        results.push(super::local_search::solve_local_search(problem, options));
    }

    let total = results.len();

    // Collect successful results, tracking last error for fallback.
    let mut best: Option<ThreeDSolution> = None;
    let mut last_error: Option<crate::BinPackingError> = None;

    for result in results {
        match result {
            Ok(sol) => {
                let is_better = best.as_ref().is_none_or(|b| sol.is_better_than(b));
                if is_better {
                    best = Some(sol);
                }
            }
            Err(e) => {
                last_error = Some(e);
            }
        }
    }

    match best {
        Some(mut sol) => {
            let winning_alg = sol.algorithm.clone();
            sol.metrics
                .notes
                .push(format!("auto: tried {total} algorithms, best was {winning_alg}"));
            Ok(sol)
        }
        None => Err(last_error.unwrap_or_else(|| {
            crate::BinPackingError::Unsupported(
                "auto: no candidate algorithms were run".to_string(),
            )
        })),
    }
}

type SolverFn = fn(&ThreeDProblem, &ThreeDOptions) -> Result<ThreeDSolution>;

fn solve_auto_guillotine(
    problem: &ThreeDProblem,
    options: &ThreeDOptions,
) -> Result<ThreeDSolution> {
    let candidates: &[SolverFn] = &[
        super::guillotine::solve_guillotine_3d,
        super::guillotine::solve_guillotine_3d_best_short_side_fit,
        super::guillotine::solve_guillotine_3d_best_long_side_fit,
        super::guillotine::solve_guillotine_3d_shorter_leftover_axis,
        super::guillotine::solve_guillotine_3d_longer_leftover_axis,
        super::guillotine::solve_guillotine_3d_min_volume_split,
        super::guillotine::solve_guillotine_3d_max_volume_split,
        super::layer::solve_layer_building_guillotine,
    ];

    let mut best: Option<ThreeDSolution> = None;
    let mut last_error: Option<crate::BinPackingError> = None;

    for solver in candidates {
        match solver(problem, options) {
            Ok(sol) if sol.guillotine => {
                if best.as_ref().is_none_or(|current| sol.is_better_than(current)) {
                    best = Some(sol);
                }
            }
            Ok(_sol) => {}
            Err(err) => last_error = Some(err),
        }
    }

    match best {
        Some(mut sol) => {
            let winning_alg = sol.algorithm.clone();
            sol.metrics.notes.push(format!(
                "auto: tried {} guillotine algorithms, best was {winning_alg}",
                candidates.len()
            ));
            Ok(sol)
        }
        None => Err(last_error.unwrap_or_else(|| {
            crate::BinPackingError::Unsupported(
                "auto: no guillotine-compatible candidate succeeded".to_string(),
            )
        })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::three_d::model::{Bin3D, BoxDemand3D, RotationMask3D, ThreeDProblem};

    fn simple_problem() -> ThreeDProblem {
        ThreeDProblem {
            bins: vec![Bin3D {
                name: "bin".to_string(),
                width: 10,
                height: 10,
                depth: 10,
                cost: 1.0,
                quantity: None,
            }],
            demands: vec![BoxDemand3D {
                name: "box".to_string(),
                width: 5,
                height: 5,
                depth: 5,
                quantity: 1,
                allowed_rotations: RotationMask3D::ALL,
            }],
        }
    }

    #[test]
    fn auto_does_not_error_on_simple_problem() {
        let problem = simple_problem();
        let options = ThreeDOptions::default();
        let result = solve_auto(&problem, &options);
        assert!(result.is_ok(), "expected Ok, got {result:?}");
    }

    #[test]
    fn auto_preserves_winning_leaf_algorithm() {
        let problem = simple_problem();
        let options = ThreeDOptions::default();
        let solution = solve_auto(&problem, &options).expect("solve_auto should succeed");
        assert_ne!(solution.algorithm, "auto");
        assert!(!solution.algorithm.is_empty());
    }

    #[test]
    fn auto_adds_diagnostic_note() {
        let problem = simple_problem();
        let options = ThreeDOptions::default();
        let solution = solve_auto(&problem, &options).expect("solve_auto should succeed");
        let has_note = solution.metrics.notes.iter().any(|n| n.starts_with("auto: tried "));
        assert!(has_note, "expected diagnostic note, got {:?}", solution.metrics.notes);
    }

    #[test]
    fn auto_guillotine_required_returns_guillotine_solution() {
        let problem = simple_problem();
        let options = ThreeDOptions { guillotine_required: true, ..ThreeDOptions::default() };
        let solution = solve_auto(&problem, &options).expect("solve_auto should succeed");
        assert!(solution.guillotine);
        assert_ne!(solution.algorithm, "auto");
    }
}
