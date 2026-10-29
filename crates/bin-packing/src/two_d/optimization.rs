//! Explicit profit, feasibility and open-sheet objectives with physical exclusions.

use super::constrained::{
    ConstrainedOptions2D, ConstrainedProblem2D, ConstrainedSolution2D, PackingObjective2D,
    solve_2d_constrained, validate_2d_constrained,
};
use super::{SolverMetrics2D, TwoDSolution};
use crate::optimization::{Candidate, Model};
pub use crate::optimization::{OptimizationOptions, OptimizationReport, PackingObjective};
use crate::{BinPackingError, Result};

mod staged_objectives;
pub use staged_objectives::{
    OptimizationProblemStaged2D, OptimizedSolutionStaged2D, solve_2d_staged_optimized,
};

/// Physical sheet problem with source-indexed profits and mandatory quantities.
pub type OptimizationProblem2D = crate::optimization::OptimizationProblem<ConstrainedProblem2D>;
/// Indexed rectangular geometry and explicit objective evidence.
pub type OptimizedSolution2D =
    crate::optimization::OptimizedSolution<TwoDSolution, ConstrainedProblem2D>;

/// Optimize material, selected profit, feasibility or one-sheet open dimensions.
///
/// Supports 128 copies, 16 demand types and 16 stocks. Open X/Y/XY requires one
/// stock type, whose dimensions are upper limits. Cropping preserves left/top
/// coordinates and edge trim widths; defects are intersected with the crop.
/// Failed bounded searches are not proofs of infeasibility or optimality.
///
/// # Errors
/// Returns malformed source/value/control errors or native packing errors.
pub fn solve_2d_optimized(
    problem: OptimizationProblem2D,
    options: OptimizationOptions,
) -> Result<OptimizedSolution2D> {
    crate::optimization::solve(problem, options)
}

pub(super) fn cropped(
    problem: &ConstrainedProblem2D,
    extents: &[u32],
    one_bin: bool,
) -> Result<ConstrainedProblem2D> {
    let mut result = problem.clone();
    if !one_bin {
        return Ok(result);
    }
    let stock = &mut result.problem.sheets[0];
    stock.width = extents[0];
    stock.height = extents[1];
    stock.quantity = Some(stock.quantity.unwrap_or(1).min(1));
    if let Some(c) = result.constraints.first_mut() {
        if u64::from(c.trim_left) + u64::from(c.trim_right) >= u64::from(stock.width)
            || u64::from(c.trim_top) + u64::from(c.trim_bottom) >= u64::from(stock.height)
        {
            return Err(BinPackingError::InvalidConstraint {
                field: "open_extent".into(),
                reason: "crop leaves no usable sheet".into(),
            });
        }
        c.defects.retain_mut(|d| {
            d.width = d.width.min(stock.width.saturating_sub(d.x));
            d.height = d.height.min(stock.height.saturating_sub(d.y));
            d.width > 0 && d.height > 0
        });
    }
    Ok(result)
}

pub(super) fn fits_stock(problem: &ConstrainedProblem2D, demand: usize) -> bool {
    let d = &problem.problem.demands[demand];
    problem.problem.sheets.iter().any(|s| {
        (d.width <= s.width && d.height <= s.height)
            || (d.can_rotate && d.height <= s.width && d.width <= s.height)
    })
}

impl Model for ConstrainedProblem2D {
    type Solution = TwoDSolution;
    fn validate(&self) -> Result<()> {
        super::constrained::validate_source(self)
    }
    fn quantities(&self) -> Vec<usize> {
        self.problem.demands.iter().map(|d| d.quantity).collect()
    }
    fn stock_count(&self) -> usize {
        self.problem.sheets.len()
    }
    fn dimensions(&self) -> Vec<u32> {
        vec![self.problem.sheets[0].width, self.problem.sheets[0].height]
    }
    fn count_bound(&self) -> usize {
        let load: u128 = self
            .problem
            .demands
            .iter()
            .map(|d| u128::from(d.width) * u128::from(d.height) * d.quantity as u128)
            .sum();
        let capacity = self
            .problem
            .sheets
            .iter()
            .map(|s| u128::from(s.width) * u128::from(s.height))
            .max()
            .unwrap_or(1);
        load.div_ceil(capacity) as usize
    }
    fn open_bound(&self, axes: &[usize]) -> u128 {
        let area: u128 = self
            .problem
            .demands
            .iter()
            .map(|d| u128::from(d.width) * u128::from(d.height) * d.quantity as u128)
            .sum();
        if axes.len() == 2 {
            area
        } else {
            area.div_ceil(u128::from(self.dimensions()[1 - axes[0]]))
        }
    }
    fn minimum_stock_cost(&self) -> f64 {
        self.problem.sheets.iter().map(|s| s.cost).fold(f64::INFINITY, f64::min)
    }
    fn evaluate(
        &self,
        quantities: &[usize],
        extents: &[u32],
        one_bin: bool,
        options: &OptimizationOptions,
    ) -> Result<Candidate<Self, Self::Solution>> {
        let effective = cropped(self, extents, one_bin)?;
        let mut indexed = effective.clone();
        let sources: Vec<_> = quantities
            .iter()
            .enumerate()
            .filter(|&(i, q)| *q > 0 && fits_stock(&effective, i))
            .map(|(i, _)| i)
            .collect();
        indexed.problem.demands = sources
            .iter()
            .map(|&i| {
                let mut d = self.problem.demands[i].clone();
                d.quantity = quantities[i];
                d
            })
            .collect();
        let mut result = if sources.is_empty() {
            ConstrainedSolution2D {
                solution: TwoDSolution {
                    algorithm: "objective_empty".into(),
                    guillotine: false,
                    sheet_count: 0,
                    total_waste_area: 0,
                    total_kerf_area: 0,
                    total_cost: 0.0,
                    max_usable_drop_area: 0,
                    total_sum_sq_usable_drop_areas: 0,
                    layouts: Vec::new(),
                    unplaced: Vec::new(),
                    metrics: SolverMetrics2D {
                        iterations: 0,
                        explored_states: 0,
                        notes: Vec::new(),
                    },
                },
                stock_indices: Vec::new(),
                demand_indices: Vec::new(),
                explored_states: 0,
                search_truncated: false,
            }
        } else {
            solve_2d_constrained(
                indexed,
                ConstrainedOptions2D {
                    objective: if options.objective == PackingObjective::TotalCost {
                        PackingObjective2D::TotalCost
                    } else {
                        PackingObjective2D::BinCount
                    },
                    beam_width: options.beam_width,
                    max_states: options.packing_max_states,
                },
            )?
        };
        let mut selected = vec![0; quantities.len()];
        for indices in &mut result.demand_indices {
            for index in indices {
                *index = sources[*index];
                selected[*index] += 1;
            }
        }
        result.solution.unplaced = self
            .problem
            .demands
            .iter()
            .enumerate()
            .filter(|(i, d)| selected[*i] < d.quantity)
            .map(|(i, d)| {
                let mut d = d.clone();
                d.quantity -= selected[i];
                d
            })
            .collect();
        validate_2d_constrained(&effective, &result)?;
        Ok(Candidate {
            containers: result.solution.sheet_count,
            cost: result.solution.total_cost,
            waste: u128::from(result.solution.total_waste_area),
            extents: if one_bin { extents.to_vec() } else { self.dimensions() },
            states: result.explored_states,
            truncated: result.search_truncated,
            problem: effective,
            solution: result.solution,
            stocks: result.stock_indices,
            demands: result.demand_indices,
            selected,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::two_d::{RectDemand2D, Sheet2D, TwoDProblem};
    fn problem() -> OptimizationProblem2D {
        OptimizationProblem2D {
            packing: ConstrainedProblem2D {
                problem: TwoDProblem {
                    sheets: vec![Sheet2D {
                        name: "stock".into(),
                        width: 10,
                        height: 5,
                        cost: 1.0,
                        quantity: Some(1),
                        kerf: 0,
                        edge_kerf_relief: false,
                    }],
                    demands: vec![
                        RectDemand2D {
                            name: "same".into(),
                            width: 10,
                            height: 5,
                            quantity: 1,
                            can_rotate: false,
                        },
                        RectDemand2D {
                            name: "same".into(),
                            width: 5,
                            height: 5,
                            quantity: 2,
                            can_rotate: false,
                        },
                    ],
                },
                constraints: Vec::new(),
            },
            profits: vec![1.0, 10.0],
            minimum_quantities: Vec::new(),
        }
    }
    #[test]
    fn optimized_rectangular_profit_preserves_source_values() {
        let r = solve_2d_optimized(
            problem(),
            OptimizationOptions { objective: PackingObjective::Knapsack, ..Default::default() },
        )
        .expect("profit packing");
        assert_eq!(r.report.selected_quantities, vec![0, 2]);
        assert_eq!(r.report.total_profit, 20.0);
        assert_eq!(r.demand_indices, vec![vec![1, 1]]);
    }
    #[test]
    fn optimized_open_sheet_crops_dimensions_without_scaling_parts() {
        let mut p = problem();
        p.packing.problem.demands.remove(0);
        p.profits.clear();
        p.packing.problem.sheets[0].width = 100;
        let r = solve_2d_optimized(
            p,
            OptimizationOptions {
                objective: PackingObjective::OpenDimensionX,
                ..Default::default()
            },
        )
        .expect("open sheet");
        assert!(r.report.complete);
        assert_eq!(r.report.open_extents, vec![10, 5]);
        assert!(r.report.primary_optimal);
    }
    #[test]
    fn optimized_rectangular_rejects_unsupported_axis_and_bad_values() {
        assert!(
            matches!(solve_2d_optimized(problem(),OptimizationOptions {objective:PackingObjective::OpenDimensionZ,..Default::default()}),Err(BinPackingError::InvalidConstraint {field,..}) if field=="objective")
        );
        let mut p = problem();
        p.profits[0] = f64::INFINITY;
        assert!(
            matches!(solve_2d_optimized(p,OptimizationOptions::default()),Err(BinPackingError::InvalidConstraint {field,..}) if field=="values")
        );
    }

    #[test]
    fn optimized_knapsack_can_omit_an_individually_oversized_optional_rectangle() {
        let mut p = problem();
        p.packing.problem.demands[0].width = 11;
        let result = solve_2d_optimized(
            p,
            OptimizationOptions { objective: PackingObjective::Knapsack, ..Default::default() },
        )
        .expect("oversized optional rectangle");
        assert_eq!(result.report.selected_quantities, vec![0, 2]);
        assert_eq!(result.report.total_profit, 20.0);
    }
}
