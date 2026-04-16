//! One-dimensional cutting stock solvers.
//!
//! Provides heuristics (first-fit decreasing, best-fit decreasing, local search) and an
//! exact backend (column generation with pattern-search refinement).

pub mod cut_plan;
mod exact;
mod heuristics;
mod model;

pub use model::{
    CutAssignment1D, CutDemand1D, OneDAlgorithm, OneDOptions, OneDProblem, OneDSolution,
    SolverMetrics1D, Stock1D, StockLayout1D, StockRequirement1D,
};

use crate::Result;

/// Solve a 1D cutting stock problem using the requested algorithm.
///
/// Validates the problem and dispatches to the algorithm selected in `options`. Use
/// [`OneDAlgorithm::Auto`] to let the solver choose a heuristic and optionally escalate
/// to the exact backend when the instance is small enough.
///
/// # Errors
///
/// Returns [`BinPackingError::InvalidInput`](crate::BinPackingError::InvalidInput) for
/// malformed problems, [`BinPackingError::Infeasible1D`](crate::BinPackingError::Infeasible1D)
/// when at least one demand cannot fit any declared stock, and
/// [`BinPackingError::Unsupported`](crate::BinPackingError::Unsupported) when the exact
/// backend encounters a configuration it does not handle.
pub fn solve_1d(problem: OneDProblem, options: OneDOptions) -> Result<OneDSolution> {
    problem.validate()?;
    problem.ensure_feasible_demands()?;

    let mut solution = solve_1d_core(&problem, &options)?;

    if problem.stock.iter().any(|stock| stock.available.is_some()) {
        let required_counts = estimate_required_stock_counts(&problem, &options)?;
        solution.set_required_stock_counts(&problem.stock, &required_counts);
        solution
            .metrics
            .notes
            .push("stock requirements estimated from a relaxed-inventory auto solve".to_string());
    }

    Ok(solution)
}

fn solve_1d_core(problem: &OneDProblem, options: &OneDOptions) -> Result<OneDSolution> {
    match options.algorithm {
        OneDAlgorithm::FirstFitDecreasing => heuristics::solve_ffd(problem, options),
        OneDAlgorithm::BestFitDecreasing => heuristics::solve_bfd(problem, options),
        OneDAlgorithm::LocalSearch => heuristics::solve_local_search(problem, options),
        OneDAlgorithm::ColumnGeneration => exact::solve_exact(problem, options),
        OneDAlgorithm::Auto => solve_auto(problem, options),
    }
}

fn estimate_required_stock_counts(
    problem: &OneDProblem,
    options: &OneDOptions,
) -> Result<Vec<usize>> {
    let mut relaxed_problem = problem.clone();
    for stock in &mut relaxed_problem.stock {
        stock.available = None;
    }

    let relaxed_options = OneDOptions { algorithm: OneDAlgorithm::Auto, ..options.clone() };
    let relaxed_solution = solve_1d_core(&relaxed_problem, &relaxed_options)?;

    Ok(relaxed_solution
        .stock_requirements
        .into_iter()
        .map(|requirement| requirement.used_quantity)
        .collect())
}

type SolverFn1D = fn(&OneDProblem, &OneDOptions) -> Result<OneDSolution>;

fn run_candidates_1d(
    candidates: &[SolverFn1D],
    problem: &OneDProblem,
    options: &OneDOptions,
) -> Vec<Result<OneDSolution>> {
    #[cfg(feature = "parallel")]
    if crate::parallel::use_parallel() {
        use rayon::prelude::*;
        return candidates.par_iter().map(|solver| solver(problem, options)).collect();
    }

    candidates.iter().map(|solver| solver(problem, options)).collect()
}

fn solve_auto(problem: &OneDProblem, options: &OneDOptions) -> Result<OneDSolution> {
    let heuristic_candidates: &[SolverFn1D] =
        &[heuristics::solve_ffd, heuristics::solve_bfd, heuristics::solve_local_search];

    let results = run_candidates_1d(heuristic_candidates, problem, options);
    let mut best: Option<OneDSolution> = None;
    for sol in results.into_iter().flatten() {
        if best.as_ref().is_none_or(|b| sol.is_better_than(b)) {
            best = Some(sol);
        }
    }
    let mut best = best.ok_or_else(|| {
        crate::BinPackingError::Unsupported("auto: all heuristic candidates failed".to_string())
    })?;

    // Exact solver runs after heuristics — its result takes precedence when
    // it reports `exact = true` even if the heuristic score is equal.
    let should_attempt_exact = problem.stock.len() == 1
        && problem.stock[0].available.is_none()
        && problem.demands.len() <= options.auto_exact_max_types
        && problem.total_quantity() <= options.auto_exact_max_quantity;

    if should_attempt_exact
        && let Ok(exact) = exact::solve_exact(problem, options)
        && (exact.is_better_than(&best) || exact.exact)
    {
        best = exact;
    }

    Ok(best)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_upgrades_from_ffd_when_other_heuristics_are_better() {
        let bfd_problem = OneDProblem {
            stock: vec![
                Stock1D {
                    name: "s0".to_string(),
                    length: 18,
                    kerf: 1,
                    trim: 2,
                    cost: 2.0,
                    available: None,
                },
                Stock1D {
                    name: "s1".to_string(),
                    length: 29,
                    kerf: 0,
                    trim: 0,
                    cost: 1.0,
                    available: None,
                },
            ],
            demands: vec![
                CutDemand1D { name: "d0".to_string(), length: 12, quantity: 2 },
                CutDemand1D { name: "d1".to_string(), length: 1, quantity: 1 },
                CutDemand1D { name: "d2".to_string(), length: 25, quantity: 2 },
                CutDemand1D { name: "d3".to_string(), length: 14, quantity: 3 },
                CutDemand1D { name: "d4".to_string(), length: 1, quantity: 4 },
            ],
        };
        let auto = solve_1d(bfd_problem.clone(), OneDOptions::default()).expect("auto solve");
        let ffd = solve_1d(
            bfd_problem.clone(),
            OneDOptions { algorithm: OneDAlgorithm::FirstFitDecreasing, ..OneDOptions::default() },
        )
        .expect("ffd");
        let bfd = solve_1d(
            bfd_problem,
            OneDOptions { algorithm: OneDAlgorithm::BestFitDecreasing, ..OneDOptions::default() },
        )
        .expect("bfd");
        assert!(bfd.is_better_than(&ffd));
        assert_eq!(auto.total_waste, bfd.total_waste);

        let local_problem = OneDProblem {
            stock: vec![Stock1D {
                name: "s".to_string(),
                length: 8,
                kerf: 0,
                trim: 0,
                cost: 1.0,
                available: None,
            }],
            demands: vec![
                CutDemand1D { name: "a".to_string(), length: 2, quantity: 3 },
                CutDemand1D { name: "b".to_string(), length: 3, quantity: 2 },
                CutDemand1D { name: "c".to_string(), length: 4, quantity: 1 },
            ],
        };
        let local = solve_1d(
            local_problem.clone(),
            OneDOptions {
                algorithm: OneDAlgorithm::LocalSearch,
                seed: Some(7),
                ..OneDOptions::default()
            },
        )
        .expect("local");
        let bfd = solve_1d(
            local_problem.clone(),
            OneDOptions { algorithm: OneDAlgorithm::BestFitDecreasing, ..OneDOptions::default() },
        )
        .expect("bfd");
        let ffd = solve_1d(
            local_problem.clone(),
            OneDOptions { algorithm: OneDAlgorithm::FirstFitDecreasing, ..OneDOptions::default() },
        )
        .expect("ffd");
        let auto = solve_1d(local_problem, OneDOptions { seed: Some(7), ..OneDOptions::default() })
            .expect("auto solve");
        assert!(local.is_better_than(&bfd));
        assert!(local.is_better_than(&ffd));
        assert_eq!(auto.stock_count, local.stock_count);
        assert_eq!(auto.algorithm, "column_generation");
    }

    #[test]
    fn auto_respects_exact_limit_gates() {
        let problem = OneDProblem {
            stock: vec![Stock1D {
                name: "s".to_string(),
                length: 8,
                kerf: 0,
                trim: 0,
                cost: 1.0,
                available: None,
            }],
            demands: vec![
                CutDemand1D { name: "a".to_string(), length: 2, quantity: 3 },
                CutDemand1D { name: "b".to_string(), length: 3, quantity: 2 },
                CutDemand1D { name: "c".to_string(), length: 4, quantity: 1 },
            ],
        };

        let local = solve_1d(
            problem.clone(),
            OneDOptions {
                algorithm: OneDAlgorithm::LocalSearch,
                seed: Some(7),
                ..OneDOptions::default()
            },
        )
        .expect("local");
        let auto = solve_1d(
            problem,
            OneDOptions { seed: Some(7), auto_exact_max_types: 2, ..OneDOptions::default() },
        )
        .expect("auto");

        assert_eq!(auto.algorithm, local.algorithm);
        assert_eq!(auto.stock_count, local.stock_count);
        assert_eq!(auto.total_waste, local.total_waste);
        assert!(!auto.exact);
    }

    #[test]
    fn solve_reports_inventory_shortfalls_by_stock_type() {
        let problem = OneDProblem {
            stock: vec![Stock1D {
                name: "bar".to_string(),
                length: 10,
                kerf: 0,
                trim: 0,
                cost: 1.0,
                available: Some(1),
            }],
            demands: vec![CutDemand1D { name: "cut".to_string(), length: 5, quantity: 3 }],
        };

        let solution = solve_1d(problem, OneDOptions::default()).expect("solve");

        assert_eq!(solution.stock_count, 1);
        assert_eq!(solution.unplaced.len(), 1);
        assert_eq!(solution.stock_requirements.len(), 1);
        assert_eq!(solution.stock_requirements[0].used_quantity, 1);
        assert_eq!(solution.stock_requirements[0].required_quantity, 2);
        assert_eq!(solution.stock_requirements[0].additional_quantity_needed, 1);
        assert!(
            solution.metrics.notes.iter().any(|note| note.contains("relaxed-inventory auto solve"))
        );
    }
}
