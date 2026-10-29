//! Profit, feasibility and open-container objectives with payload constraints.

use super::constrained::{
    ConstrainedOptions3D, ConstrainedProblem3D, ConstrainedSolution3D, PackingObjective3D,
    solve_3d_constrained, validate_3d_constrained,
};
use super::{SolverMetrics3D, ThreeDSolution};
use crate::Result;
use crate::optimization::{Candidate, Model};
pub use crate::optimization::{OptimizationOptions, OptimizationReport, PackingObjective};

mod loading_objectives;
pub use loading_objectives::{
    OptimizationProblemLoading3D, OptimizedSolutionLoading3D, solve_loading_3d_optimized,
};

/// Payload-aware container input plus source-indexed profits and minima.
pub type OptimizationProblem3D = crate::optimization::OptimizationProblem<ConstrainedProblem3D>;
/// Indexed spatial geometry and explicit objective evidence.
pub type OptimizedSolution3D =
    crate::optimization::OptimizedSolution<ThreeDSolution, ConstrainedProblem3D>;

/// Optimize material, selected profit, feasibility or one-container dimensions.
///
/// Supports 128 copies, 16 demand types and 16 stocks. Open X/Y/Z/XY requires
/// one stock type, whose extents are upper limits. Payloads remain enforced
/// while selecting subsets and trying smaller dimensions.
///
/// # Errors
/// Returns malformed source/value/control errors or native packing errors.
pub fn solve_3d_optimized(
    problem: OptimizationProblem3D,
    options: OptimizationOptions,
) -> Result<OptimizedSolution3D> {
    crate::optimization::solve(problem, options)
}

pub(super) fn cropped(
    problem: &ConstrainedProblem3D,
    extents: &[u32],
    one_bin: bool,
) -> ConstrainedProblem3D {
    let mut result = problem.clone();
    if one_bin {
        let bin = &mut result.problem.bins[0];
        bin.width = extents[0];
        bin.height = extents[1];
        bin.depth = extents[2];
        bin.quantity = Some(bin.quantity.unwrap_or(1).min(1));
    }
    result
}

impl Model for ConstrainedProblem3D {
    type Solution = ThreeDSolution;
    fn validate(&self) -> Result<()> {
        super::constrained::validate_problem(self)
    }
    fn quantities(&self) -> Vec<usize> {
        self.problem.demands.iter().map(|d| d.quantity).collect()
    }
    fn stock_count(&self) -> usize {
        self.problem.bins.len()
    }
    fn dimensions(&self) -> Vec<u32> {
        let b = &self.problem.bins[0];
        vec![b.width, b.height, b.depth]
    }
    fn count_bound(&self) -> usize {
        let load: u128 = self
            .problem
            .demands
            .iter()
            .map(|d| {
                u128::from(d.width)
                    * u128::from(d.height)
                    * u128::from(d.depth)
                    * d.quantity as u128
            })
            .sum();
        let capacity = self
            .problem
            .bins
            .iter()
            .map(|s| u128::from(s.width) * u128::from(s.height) * u128::from(s.depth))
            .max()
            .unwrap_or(1);
        load.div_ceil(capacity) as usize
    }
    fn open_bound(&self, axes: &[usize]) -> u128 {
        let load: u128 = self
            .problem
            .demands
            .iter()
            .map(|d| {
                u128::from(d.width)
                    * u128::from(d.height)
                    * u128::from(d.depth)
                    * d.quantity as u128
            })
            .sum();
        let fixed: u128 = self
            .dimensions()
            .iter()
            .enumerate()
            .filter(|(axis, _)| !axes.contains(axis))
            .map(|(_, v)| u128::from(*v))
            .product();
        load.div_ceil(fixed)
    }
    fn minimum_stock_cost(&self) -> f64 {
        self.problem.bins.iter().map(|s| s.cost).fold(f64::INFINITY, f64::min)
    }
    fn evaluate(
        &self,
        quantities: &[usize],
        extents: &[u32],
        one_bin: bool,
        options: &OptimizationOptions,
    ) -> Result<Candidate<Self, Self::Solution>> {
        let effective = cropped(self, extents, one_bin);
        let sources: Vec<_> =
            quantities.iter().enumerate().filter(|&(_, q)| *q > 0).map(|(i, _)| i).collect();
        let mut indexed = effective.clone();
        indexed.problem.demands = sources
            .iter()
            .map(|&i| {
                let mut d = self.problem.demands[i].clone();
                d.quantity = quantities[i];
                d
            })
            .collect();
        indexed.item_weights = if self.item_weights.is_empty() {
            Vec::new()
        } else {
            sources.iter().map(|&i| self.item_weights[i]).collect()
        };
        let mut result = if sources.is_empty() {
            ConstrainedSolution3D {
                solution: ThreeDSolution {
                    algorithm: "objective_empty".into(),
                    exact: false,
                    lower_bound: None,
                    guillotine: false,
                    bin_count: 0,
                    total_waste_volume: 0,
                    total_cost: 0.0,
                    layouts: Vec::new(),
                    bin_requirements: Vec::new(),
                    unplaced: Vec::new(),
                    metrics: SolverMetrics3D::default(),
                },
                bin_indices: Vec::new(),
                demand_indices: Vec::new(),
                bin_weights: Vec::new(),
                explored_states: 0,
                search_truncated: false,
            }
        } else {
            solve_3d_constrained(
                indexed,
                ConstrainedOptions3D {
                    objective: if options.objective == PackingObjective::TotalCost {
                        PackingObjective3D::TotalCost
                    } else {
                        PackingObjective3D::BinCount
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
            .flat_map(|(i, d)| {
                let mut d = d.clone();
                d.quantity = 1;
                std::iter::repeat_n(d, self.problem.demands[i].quantity - selected[i])
            })
            .collect();
        validate_3d_constrained(&effective, &result)?;
        Ok(Candidate {
            containers: result.solution.bin_count,
            cost: result.solution.total_cost,
            waste: u128::from(result.solution.total_waste_volume),
            extents: if one_bin { extents.to_vec() } else { self.dimensions() },
            states: result.explored_states,
            truncated: result.search_truncated,
            problem: effective,
            solution: result.solution,
            stocks: result.bin_indices,
            demands: result.demand_indices,
            selected,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::three_d::{Bin3D, BoxDemand3D, RotationMask3D, ThreeDProblem};
    fn problem() -> OptimizationProblem3D {
        OptimizationProblem3D {
            packing: ConstrainedProblem3D {
                problem: ThreeDProblem {
                    bins: vec![Bin3D {
                        name: "stock".into(),
                        width: 10,
                        height: 5,
                        depth: 5,
                        cost: 1.0,
                        quantity: Some(1),
                    }],
                    demands: vec![
                        BoxDemand3D {
                            name: "same".into(),
                            width: 10,
                            height: 5,
                            depth: 5,
                            quantity: 1,
                            allowed_rotations: RotationMask3D::XYZ,
                        },
                        BoxDemand3D {
                            name: "same".into(),
                            width: 5,
                            height: 5,
                            depth: 5,
                            quantity: 2,
                            allowed_rotations: RotationMask3D::XYZ,
                        },
                    ],
                },
                item_weights: vec![20, 5],
                bin_max_weights: vec![Some(20)],
            },
            profits: vec![1.0, 10.0],
            minimum_quantities: Vec::new(),
        }
    }
    #[test]
    fn optimized_spatial_profit_preserves_payload_and_source_values() {
        let r = solve_3d_optimized(
            problem(),
            OptimizationOptions { objective: PackingObjective::Knapsack, ..Default::default() },
        )
        .expect("profit packing");
        assert_eq!(r.report.selected_quantities, vec![0, 2]);
        assert_eq!(r.report.total_profit, 20.0);
        assert_eq!(r.demand_indices, vec![vec![1, 1]]);
    }
    #[test]
    fn optimized_open_container_keeps_vertical_and_floor_axes_distinct() {
        let mut p = problem();
        p.packing.problem.demands.remove(0);
        p.packing.item_weights.remove(0);
        p.profits.clear();
        p.packing.problem.bins[0].width = 100;
        let r = solve_3d_optimized(
            p,
            OptimizationOptions {
                objective: PackingObjective::OpenDimensionX,
                ..Default::default()
            },
        )
        .expect("open container");
        assert!(r.report.complete);
        assert_eq!(r.report.open_extents, vec![10, 5, 5]);
        assert!(r.report.primary_optimal);
    }
    #[test]
    fn optimized_spatial_knapsack_can_select_nothing_when_payload_is_zero() {
        let mut p = problem();
        p.packing.bin_max_weights = vec![Some(0)];
        let r = solve_3d_optimized(
            p,
            OptimizationOptions { objective: PackingObjective::Knapsack, ..Default::default() },
        )
        .expect("empty packing");
        assert_eq!(r.report.selected_quantities, vec![0, 0]);
        assert_eq!(r.report.total_profit, 0.0);
        assert!(!r.report.complete);
    }
}
