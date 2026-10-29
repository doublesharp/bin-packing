//! Profit, feasibility and open-length search for linear stock.

use super::{
    CutAssignment1D, OneDAlgorithm, OneDOptions, OneDProblem, OneDSolution, SolverMetrics1D,
    solve_1d_arc_flow, solve_1d_min_cost,
};
use crate::optimization::{Candidate, Model};
pub use crate::optimization::{OptimizationOptions, OptimizationReport, PackingObjective};
use crate::{BinPackingError, Result};

/// Linear stock plus per-source profits and mandatory copy quantities.
pub type OptimizationProblem1D = crate::optimization::OptimizationProblem<OneDProblem>;
/// Indexed linear packing with its selected objective and evidence.
pub type OptimizedSolution1D = crate::optimization::OptimizedSolution<OneDSolution, OneDProblem>;

/// Optimize selected profit, feasibility, material or one-container open length.
///
/// Subset search supports at most 128 cuts, 16 demand types and 16 stocks. Open
/// length requires one stock type, whose declared length is an upper limit.
/// Search budgets are deterministic work limits, not wall-clock deadlines.
/// Legacy procurement estimates are omitted because each evaluation may select
/// a different subset. Use the ordinary 1D entry point for full-job procurement.
///
/// # Errors
/// Returns malformed source/value/control errors or native packing errors.
pub fn solve_1d_optimized(
    problem: OptimizationProblem1D,
    options: OptimizationOptions,
) -> Result<OptimizedSolution1D> {
    crate::optimization::solve(problem, options)
}

fn invalid(reason: &str) -> BinPackingError {
    BinPackingError::InvalidSolution { reason: reason.into() }
}

impl Model for OneDProblem {
    type Solution = OneDSolution;
    fn validate(&self) -> Result<()> {
        OneDProblem::validate(self)?;
        if self
            .stock
            .iter()
            .any(|s| !(s.cost * (self.total_quantity().saturating_add(1)) as f64).is_finite())
        {
            return Err(BinPackingError::InvalidConstraint {
                field: "stock.cost".into(),
                reason: "aggregate material cost would overflow".into(),
            });
        }
        Ok(())
    }
    fn quantities(&self) -> Vec<usize> {
        self.demands.iter().map(|d| d.quantity).collect()
    }
    fn stock_count(&self) -> usize {
        self.stock.len()
    }
    fn dimensions(&self) -> Vec<u32> {
        vec![self.stock[0].length]
    }
    fn count_bound(&self) -> usize {
        let load: u128 =
            self.demands.iter().map(|d| u128::from(d.length) * d.quantity as u128).sum();
        let capacity = self
            .stock
            .iter()
            .map(|s| u128::from(s.length.saturating_sub(s.trim)))
            .max()
            .unwrap_or(1);
        load.div_ceil(capacity) as usize
    }
    fn open_bound(&self, _axes: &[usize]) -> u128 {
        let count = self.total_quantity();
        self.demands.iter().map(|d| u128::from(d.length) * d.quantity as u128).sum::<u128>()
            + u128::from(self.stock[0].trim)
            + u128::from(self.stock[0].kerf) * count.saturating_sub(1) as u128
    }
    fn minimum_stock_cost(&self) -> f64 {
        self.stock.iter().map(|s| s.cost).fold(f64::INFINITY, f64::min)
    }
    fn evaluate(
        &self,
        quantities: &[usize],
        extents: &[u32],
        one_bin: bool,
        options: &OptimizationOptions,
    ) -> Result<Candidate<Self, Self::Solution>> {
        let mut effective = self.clone();
        if one_bin {
            if extents[0] <= effective.stock[0].trim {
                return Err(BinPackingError::InvalidConstraint {
                    field: "open_extent".into(),
                    reason: "crop leaves no usable stock".into(),
                });
            }
            effective.stock[0].length = extents[0];
            effective.stock[0].available = Some(effective.stock[0].available.unwrap_or(1).min(1));
        }
        let mut indexed = effective.clone();
        indexed.demands = self
            .demands
            .iter()
            .enumerate()
            .filter(|(i, d)| {
                quantities[*i] > 0
                    && effective.stock.iter().any(|s| d.length <= s.length.saturating_sub(s.trim))
            })
            .map(|(index, d)| {
                let mut d = d.clone();
                d.name = format!("objective_{index}");
                d.quantity = quantities[index];
                d
            })
            .collect();
        let native_options = OneDOptions {
            algorithm: OneDAlgorithm::Auto,
            seed: Some(options.seed),
            multistart_runs: options.beam_width.min(4),
            improvement_rounds: 4,
            exact_pattern_limit: options.packing_max_states,
            column_generation_rounds: 1,
            auto_exact_max_types: 0,
            auto_exact_max_quantity: 0,
        };
        let mut solution = if indexed.demands.is_empty() {
            OneDSolution::from_bins(
                "objective_empty",
                false,
                None,
                &effective.stock,
                &[],
                &[],
                SolverMetrics1D {
                    iterations: 0,
                    generated_patterns: 0,
                    enumerated_patterns: 0,
                    explored_states: 0,
                    notes: Vec::new(),
                },
            )
        } else if options.objective == PackingObjective::TotalCost {
            solve_1d_min_cost(indexed, native_options)?
        } else {
            solve_1d_arc_flow(indexed, native_options)?
        };
        let mut selected = vec![0; self.demands.len()];
        let mut stocks = Vec::new();
        let mut demands = Vec::new();
        for layout in &mut solution.layouts {
            let stock = self
                .stock
                .iter()
                .position(|s| s.name == layout.stock_name)
                .ok_or_else(|| invalid("unknown optimized stock"))?;
            stocks.push(stock);
            let mut indices = Vec::new();
            for cut in &mut layout.cuts {
                let index = cut
                    .name
                    .strip_prefix("objective_")
                    .and_then(|s| s.parse::<usize>().ok())
                    .filter(|&i| i < self.demands.len())
                    .ok_or_else(|| invalid("unknown optimized demand"))?;
                selected[index] += 1;
                if selected[index] > quantities[index] || cut.length != self.demands[index].length {
                    return Err(invalid("optimized source accounting"));
                }
                cut.name = self.demands[index].name.clone();
                indices.push(index);
            }
            demands.push(indices);
        }
        solution.unplaced = self
            .demands
            .iter()
            .enumerate()
            .flat_map(|(i, d)| {
                std::iter::repeat_n(
                    CutAssignment1D { name: d.name.clone(), length: d.length },
                    d.quantity - selected[i],
                )
            })
            .collect();
        // The native estimate belongs to the compact selected subproblem, not
        // to the original demands that this result conserves as placed/unplaced.
        solution.stock_requirements.clear();
        if !matches!(options.objective, PackingObjective::BinCount | PackingObjective::TotalCost)
            || selected.iter().zip(&self.demands).any(|(&n, d)| n < d.quantity)
        {
            solution.exact = false;
            solution.lower_bound = None;
        }
        let states = solution.metrics.explored_states;
        Ok(Candidate {
            containers: solution.stock_count,
            cost: solution.total_cost,
            waste: u128::from(solution.total_waste),
            extents: if one_bin { extents.to_vec() } else { self.dimensions() },
            states,
            truncated: !solution.exact,
            problem: effective,
            solution,
            stocks,
            demands,
            selected,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::one_d::{CutDemand1D, Stock1D};
    fn problem() -> OptimizationProblem1D {
        OptimizationProblem1D {
            packing: OneDProblem {
                stock: vec![Stock1D {
                    name: "bar".into(),
                    length: 10,
                    kerf: 0,
                    trim: 0,
                    cost: 1.0,
                    available: Some(1),
                }],
                demands: vec![
                    CutDemand1D { name: "same".into(), length: 10, quantity: 1 },
                    CutDemand1D { name: "same".into(), length: 5, quantity: 2 },
                ],
            },
            profits: vec![1.0, 10.0],
            minimum_quantities: Vec::new(),
        }
    }
    #[test]
    fn optimized_knapsack_uses_value_and_preserves_duplicate_source_labels() {
        let result = solve_1d_optimized(
            problem(),
            OptimizationOptions { objective: PackingObjective::Knapsack, ..Default::default() },
        )
        .expect("profit search");
        assert_eq!(result.report.selected_quantities, vec![0, 2]);
        assert_eq!(result.report.total_profit, 20.0);
        assert_eq!(result.demand_indices, vec![vec![1, 1]]);
        assert_eq!(result.solution.unplaced.len(), 1);
        assert!(
            result.solution.stock_requirements.is_empty(),
            "a selected subset cannot report full-job procurement"
        );
    }
    #[test]
    fn optimized_open_length_includes_blade_and_trim() {
        let mut p = problem();
        p.packing.demands.truncate(1);
        p.packing.demands[0].length = 3;
        p.packing.demands[0].quantity = 2;
        p.profits.clear();
        p.packing.stock[0].length = 100;
        p.packing.stock[0].kerf = 2;
        p.packing.stock[0].trim = 1;
        let result = solve_1d_optimized(
            p,
            OptimizationOptions {
                objective: PackingObjective::OpenDimensionX,
                ..Default::default()
            },
        )
        .expect("open length");
        assert!(result.report.complete);
        assert_eq!(result.report.open_extents, vec![9]);
        assert!(result.report.primary_optimal);
    }
    #[test]
    fn optimized_knapsack_honors_mandatory_copies_before_profit() {
        let mut p = problem();
        p.minimum_quantities = vec![1, 0];
        let result = solve_1d_optimized(
            p,
            OptimizationOptions { objective: PackingObjective::Knapsack, ..Default::default() },
        )
        .expect("mandatory profit search");
        assert_eq!(result.report.selected_quantities, vec![1, 0]);
        assert!(result.report.minimums_satisfied);
    }
    #[test]
    fn optimized_feasibility_does_not_call_failed_search_infeasible_proof() {
        let result = solve_1d_optimized(
            problem(),
            OptimizationOptions { objective: PackingObjective::Feasibility, ..Default::default() },
        )
        .expect("partial witness");
        assert!(!result.report.complete);
        assert!(!result.report.primary_optimal);
    }

    #[test]
    fn optimized_knapsack_can_omit_an_individually_oversized_optional_cut() {
        let mut p = problem();
        p.packing.demands[0].length = 11;
        let result = solve_1d_optimized(
            p,
            OptimizationOptions { objective: PackingObjective::Knapsack, ..Default::default() },
        )
        .expect("oversized optional demand");
        assert_eq!(result.report.selected_quantities, vec![0, 2]);
        assert_eq!(result.report.total_profit, 20.0);
    }
}
