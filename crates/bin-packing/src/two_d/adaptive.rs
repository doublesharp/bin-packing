//! Sequential, count-bound-aware selection of existing rectangular solvers.
//!
//! This opt-in API can stop as soon as the minimum sheet count is certified.
//! Such a certificate covers sheet count only: it does not prove that reusable
//! offcuts, waste across different stock types, or material cost are optimal.

use serde::{Deserialize, Serialize};

use super::{TwoDAlgorithm, TwoDOptions, TwoDProblem, TwoDSolution, lower_bound_2d, solve_2d};
use crate::{BinPackingError, Result};

/// Controls the adaptive ensemble without changing the ordinary Auto solver.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AdaptiveOptions2D {
    /// Existing per-algorithm controls. `algorithm` must be `auto`.
    pub solver: TwoDOptions,
    /// Maximum number of complete algorithm calls; from 1 through 11.
    pub max_candidates: usize,
    /// Stop when all demands are placed using the certified minimum sheet count.
    ///
    /// Set to false to keep searching for better secondary metrics such as
    /// reusable offcuts. A count certificate never covers those metrics.
    pub stop_on_count_bound: bool,
}

impl Default for AdaptiveOptions2D {
    fn default() -> Self {
        Self {
            solver: TwoDOptions { seed: Some(0), ..TwoDOptions::default() },
            max_candidates: 11,
            stop_on_count_bound: true,
        }
    }
}

/// Packing and evidence from the bounded adaptive ensemble.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdaptiveSolution2D {
    /// Best candidate under the ordinary 2D ranking among the algorithms run.
    pub solution: TwoDSolution,
    /// Independently computed lower bound on sheets required for all demands.
    pub lower_bound: usize,
    /// All demands are placed and the returned sheet count equals the bound.
    /// This certifies only count, not secondary objective values.
    pub count_optimal: bool,
    /// Number of algorithm calls, including candidates that returned an error.
    pub candidates_run: usize,
    /// A count certificate caused the ensemble to stop before another algorithm.
    pub stopped_on_bound: bool,
    /// The candidate limit prevented another algorithm call.
    pub candidate_limit_reached: bool,
}

/// Run existing solvers sequentially, retaining the incumbent between calls.
///
/// At most 4,096 pieces, 128 demand types and 32 stock types are accepted. The
/// algorithm-call budget is deterministic; it is not a wall-clock deadline or a
/// bound on the work inside an algorithm. Ordinary Auto and its complete ranking
/// are unchanged. Unspecified seeds are fixed to zero for this explicit API.
///
/// # Errors
/// Returns ordinary problem/feasibility errors, malformed-control errors, or the
/// last candidate error when no algorithm succeeds.
pub fn solve_2d_adaptive(
    problem: TwoDProblem,
    options: AdaptiveOptions2D,
) -> Result<AdaptiveSolution2D> {
    problem.validate()?;
    let total_quantity =
        problem.demands.iter().fold(0usize, |sum, d| sum.saturating_add(d.quantity));
    if total_quantity > 4096 || problem.demands.len() > 128 || problem.sheets.len() > 32 {
        return Err(invalid("problem", "supports at most 4096 pieces, 128 demands and 32 stocks"));
    }
    if problem.sheets.iter().any(|stock| {
        let (width, height) = super::model::effective_bounds(stock);
        u128::from(width) * u128::from(height) * total_quantity as u128 > u128::from(u64::MAX)
            || !(stock.cost * total_quantity as f64).is_finite()
    }) {
        return Err(invalid("totals", "aggregate area or material cost would overflow"));
    }
    if !(1..=11).contains(&options.max_candidates) {
        return Err(invalid("max_candidates", "must be between 1 and 11"));
    }
    if options.solver.algorithm != TwoDAlgorithm::Auto {
        return Err(invalid("solver.algorithm", "must be auto for adaptive selection"));
    }
    if !(1..=32).contains(&options.solver.beam_width)
        || !(1..=64).contains(&options.solver.multistart_runs)
        || options.solver.auto_rotation_search_max_types > 16
    {
        return Err(invalid(
            "solver",
            "requires beam width 1..=32, multistart runs 1..=64 and rotation search threshold at most 16",
        ));
    }
    let lower_bound = lower_bound_2d(&problem)?;
    let algorithms: &[TwoDAlgorithm] = if options.solver.guillotine_required {
        &[
            TwoDAlgorithm::GuillotineBestShortSideFit,
            TwoDAlgorithm::GuillotineShorterLeftoverAxis,
            TwoDAlgorithm::Guillotine,
            TwoDAlgorithm::GuillotineBestLongSideFit,
            TwoDAlgorithm::GuillotineLongerLeftoverAxis,
            TwoDAlgorithm::GuillotineMinAreaSplit,
            TwoDAlgorithm::GuillotineMaxAreaSplit,
        ]
    } else {
        &[
            TwoDAlgorithm::MaxRectsBestShortSideFit,
            TwoDAlgorithm::MaxRects,
            TwoDAlgorithm::MaxRectsBestLongSideFit,
            TwoDAlgorithm::MaxRectsContactPoint,
            TwoDAlgorithm::Skyline,
            TwoDAlgorithm::SkylineMinWaste,
            TwoDAlgorithm::BestFitDecreasingHeight,
            TwoDAlgorithm::GuillotineBestShortSideFit,
            TwoDAlgorithm::GuillotineShorterLeftoverAxis,
            TwoDAlgorithm::MultiStart,
            TwoDAlgorithm::RotationSearch,
        ]
    };
    let mut best: Option<TwoDSolution> = None;
    let mut last_error = None;
    let mut candidates_run = 0;
    let mut explored_states = 0usize;
    let mut iterations = 0usize;
    let mut stopped_on_bound = false;
    for &algorithm in algorithms.iter().take(options.max_candidates) {
        let solver_options = TwoDOptions {
            algorithm,
            seed: Some(options.solver.seed.unwrap_or(0)),
            ..options.solver.clone()
        };
        candidates_run += 1;
        match solve_2d(problem.clone(), solver_options) {
            Ok(solution) => {
                explored_states = explored_states.saturating_add(solution.metrics.explored_states);
                iterations = iterations.saturating_add(solution.metrics.iterations);
                if best.as_ref().is_none_or(|b| solution.is_better_than(b)) {
                    best = Some(solution);
                }
            }
            Err(error) => last_error = Some(error),
        }
        let count_optimal = best.as_ref().is_some_and(|solution| {
            solution.unplaced.is_empty() && solution.sheet_count == lower_bound
        });
        if options.stop_on_count_bound && count_optimal {
            stopped_on_bound = candidates_run < algorithms.len();
            break;
        }
    }
    let mut solution = best
        .ok_or_else(|| last_error.unwrap_or_else(|| invalid("solver", "no candidate succeeded")))?;
    let count_optimal = solution.unplaced.is_empty() && solution.sheet_count == lower_bound;
    solution.metrics.iterations = iterations;
    solution.metrics.explored_states = explored_states;
    solution.metrics.notes.push(format!(
        "adaptive: {candidates_run} algorithms; count lower bound {lower_bound}; count optimal {count_optimal}; secondary metrics not certified"
    ));
    Ok(AdaptiveSolution2D {
        solution,
        lower_bound,
        count_optimal,
        candidates_run,
        stopped_on_bound,
        candidate_limit_reached: !stopped_on_bound
            && candidates_run == options.max_candidates
            && candidates_run < algorithms.len(),
    })
}

fn invalid(field: &str, reason: &str) -> BinPackingError {
    BinPackingError::InvalidConstraint { field: field.into(), reason: reason.into() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::two_d::{RectDemand2D, Sheet2D};

    fn squares() -> TwoDProblem {
        TwoDProblem {
            sheets: vec![Sheet2D {
                name: "sheet".into(),
                width: 10,
                height: 10,
                cost: 1.0,
                quantity: None,
                kerf: 0,
                edge_kerf_relief: false,
            }],
            demands: vec![RectDemand2D {
                name: "square".into(),
                width: 5,
                height: 5,
                quantity: 4,
                can_rotate: true,
            }],
        }
    }

    #[test]
    fn adaptive_stops_after_first_count_certificate() {
        let result =
            solve_2d_adaptive(squares(), AdaptiveOptions2D::default()).expect("adaptive packing");
        assert!(result.count_optimal);
        assert_eq!(result.solution.sheet_count, 1);
        assert_eq!(result.lower_bound, 1);
        assert_eq!(result.candidates_run, 1);
        assert!(result.stopped_on_bound);
        assert!(!result.candidate_limit_reached);
    }

    #[test]
    fn adaptive_candidate_budget_is_respected_without_count_stopping() {
        let result = solve_2d_adaptive(
            squares(),
            AdaptiveOptions2D {
                max_candidates: 1,
                stop_on_count_bound: false,
                ..AdaptiveOptions2D::default()
            },
        )
        .expect("bounded packing");
        assert_eq!(result.candidates_run, 1);
        assert!(result.candidate_limit_reached);
        assert!(!result.stopped_on_bound);
    }

    #[test]
    fn adaptive_rejects_zero_candidate_budget() {
        let result = solve_2d_adaptive(
            squares(),
            AdaptiveOptions2D { max_candidates: 0, ..AdaptiveOptions2D::default() },
        );
        assert!(matches!(result, Err(BinPackingError::InvalidConstraint { .. })));
    }

    #[test]
    fn adaptive_reports_control_and_problem_limits() {
        let seed = AdaptiveOptions2D { max_candidates: 1, ..AdaptiveOptions2D::default() };
        assert!(solve_2d_adaptive(squares(), seed.clone()).is_ok());
        let mut options = Vec::new();
        let mut bad = seed.clone();
        bad.max_candidates = 12;
        options.push((bad, "max_candidates", "must be between 1 and 11"));
        let mut bad = seed.clone();
        bad.solver.algorithm = TwoDAlgorithm::Skyline;
        options.push((bad, "solver.algorithm", "must be auto for adaptive selection"));
        for value in [0, 33] {
            let mut bad = seed.clone();
            bad.solver.beam_width = value;
            options.push((bad, "solver", "requires beam width 1..=32, multistart runs 1..=64 and rotation search threshold at most 16"));
        }
        for value in [0, 65] {
            let mut bad = seed.clone();
            bad.solver.multistart_runs = value;
            options.push((bad, "solver", "requires beam width 1..=32, multistart runs 1..=64 and rotation search threshold at most 16"));
        }
        let mut bad = seed.clone();
        bad.solver.auto_rotation_search_max_types = 17;
        options.push((bad, "solver", "requires beam width 1..=32, multistart runs 1..=64 and rotation search threshold at most 16"));
        for (options, expected_field, expected_reason) in options {
            assert!(
                matches!(solve_2d_adaptive(squares(), options), Err(BinPackingError::InvalidConstraint { field, reason }) if field == expected_field && reason == expected_reason)
            );
        }
        let mut too_many_pieces = squares();
        too_many_pieces.demands[0].quantity = 4097;
        let mut too_many_demands = squares();
        too_many_demands.demands = vec![too_many_demands.demands[0].clone(); 129];
        let mut too_many_stocks = squares();
        too_many_stocks.sheets = (0..33)
            .map(|i| {
                let mut sheet = squares().sheets[0].clone();
                sheet.name = format!("sheet{i}");
                sheet
            })
            .collect();
        for problem in [too_many_pieces, too_many_demands, too_many_stocks] {
            assert!(
                matches!(solve_2d_adaptive(problem, seed.clone()), Err(BinPackingError::InvalidConstraint { field, reason }) if field == "problem" && reason == "supports at most 4096 pieces, 128 demands and 32 stocks")
            );
        }
    }

    #[test]
    fn adaptive_rejects_unrepresentable_aggregate_totals() {
        let mut large = squares();
        large.sheets[0].width = 1 << 30;
        large.sheets[0].height = 1 << 30;
        large.demands[0].width = 3 << 28;
        large.demands[0].height = 3 << 28;
        large.demands[0].quantity = 64;
        let mut expensive = squares();
        expensive.sheets[0].cost = f64::MAX;
        expensive.demands[0].quantity = 8;
        for problem in [large, expensive] {
            let result = solve_2d_adaptive(
                problem,
                AdaptiveOptions2D { max_candidates: 1, ..AdaptiveOptions2D::default() },
            );
            assert!(matches!(result, Err(BinPackingError::InvalidConstraint { .. })));
        }
    }

    #[test]
    fn adaptive_inventory_shortage_never_certifies_count() {
        let mut problem = squares();
        problem.sheets[0].quantity = Some(1);
        problem.demands[0].quantity = 5;
        let result = solve_2d_adaptive(
            problem,
            AdaptiveOptions2D { max_candidates: 1, ..AdaptiveOptions2D::default() },
        )
        .expect("partial packing");
        assert_eq!(result.solution.sheet_count, 1);
        assert!(!result.solution.unplaced.is_empty());
        assert!(!result.count_optimal);
        assert!(!result.stopped_on_bound);
    }

    #[test]
    fn adaptive_can_keep_searching_secondary_metrics() {
        let options =
            AdaptiveOptions2D { stop_on_count_bound: false, ..AdaptiveOptions2D::default() };
        let problem = squares();
        let ordinary = solve_2d(problem.clone(), options.solver.clone()).expect("ordinary Auto");
        let result = solve_2d_adaptive(problem, options).expect("adaptive full ensemble");
        assert_eq!(result.candidates_run, 11);
        assert!(!ordinary.is_better_than(&result.solution));
        assert!(!result.stopped_on_bound);
        assert!(!result.candidate_limit_reached);
    }

    #[test]
    fn adaptive_guillotine_selection_preserves_required_geometry() {
        let options = AdaptiveOptions2D {
            solver: TwoDOptions { guillotine_required: true, ..TwoDOptions::default() },
            ..AdaptiveOptions2D::default()
        };
        let result = solve_2d_adaptive(squares(), options).expect("guillotine packing");
        assert!(result.solution.guillotine);
        assert!(result.count_optimal);
    }

    #[test]
    fn adaptive_results_and_options_round_trip() {
        let options: AdaptiveOptions2D = serde_json::from_str("{}").expect("defaults");
        let result = solve_2d_adaptive(squares(), options).expect("packing");
        let decoded: AdaptiveSolution2D =
            serde_json::from_str(&serde_json::to_string(&result).expect("encode")).expect("decode");
        assert_eq!(decoded, result);
    }
}
