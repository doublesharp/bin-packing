use super::{OptimizationOptions, PackingObjective};
use crate::Result;
use crate::optimization::{Candidate, Model};
use crate::two_d::constrained::PackingObjective2D;
use crate::two_d::staged::{
    StagedAction2D, StagedOptions2D, StagedProblem2D, StagedSolution2D, solve_2d_staged,
    validate_2d_staged,
};
use crate::two_d::{SolverMetrics2D, TwoDSolution};

/// Staged machine problem with source-indexed profits and mandatory quantities.
pub type OptimizationProblemStaged2D = crate::optimization::OptimizationProblem<StagedProblem2D>;
/// Physical cut trees and explicit objective evidence.
pub type OptimizedSolutionStaged2D =
    crate::optimization::OptimizedSolution<StagedSolution2D, StagedProblem2D>;

/// Optimize staged cutting without relaxing any physical machine restrictions.
///
/// The common 128-copy/16-type search bounds apply. A selected source type in a
/// cutting group requires all copies of earlier source types in that group.
/// Open X/Y/XY uses the original stock dimensions as upper limits.
///
/// # Errors
/// Returns malformed source/value/control errors or native staged packing errors.
pub fn solve_2d_staged_optimized(
    problem: OptimizationProblemStaged2D,
    options: OptimizationOptions,
) -> Result<OptimizedSolutionStaged2D> {
    crate::optimization::solve(problem, options)
}

fn empty(problem: &StagedProblem2D) -> StagedSolution2D {
    StagedSolution2D {
        solution: TwoDSolution {
            algorithm: "staged_objective_empty".into(),
            guillotine: true,
            sheet_count: 0,
            total_waste_area: 0,
            total_kerf_area: 0,
            total_cost: 0.0,
            max_usable_drop_area: 0,
            total_sum_sq_usable_drop_areas: 0,
            layouts: Vec::new(),
            unplaced: problem.packing.problem.demands.clone(),
            metrics: SolverMetrics2D { iterations: 0, explored_states: 0, notes: Vec::new() },
        },
        stock_indices: Vec::new(),
        demand_indices: Vec::new(),
        unplaced_quantities: problem.packing.quantities(),
        trees: Vec::new(),
        production_order: Vec::new(),
        explored_states: 0,
        search_truncated: false,
    }
}

impl Model for StagedProblem2D {
    type Solution = StagedSolution2D;
    fn validate(&self) -> Result<()> {
        validate_2d_staged(self, &empty(self))
    }
    fn quantities(&self) -> Vec<usize> {
        self.packing.quantities()
    }
    fn stock_count(&self) -> usize {
        self.packing.stock_count()
    }
    fn dimensions(&self) -> Vec<u32> {
        self.packing.dimensions()
    }
    fn count_bound(&self) -> usize {
        self.packing.count_bound()
    }
    fn open_bound(&self, axes: &[usize]) -> u128 {
        self.packing.open_bound(axes)
    }
    fn minimum_stock_cost(&self) -> f64 {
        self.packing.minimum_stock_cost()
    }
    fn evaluate(
        &self,
        quantities: &[usize],
        extents: &[u32],
        one_bin: bool,
        options: &OptimizationOptions,
    ) -> Result<Candidate<Self, Self::Solution>> {
        let mut effective = self.clone();
        effective.packing = super::cropped(&self.packing, extents, one_bin)?;
        // Preserve full source-group predecessors, including omitted rows that
        // a compact positive-demand subproblem would otherwise forget.
        let mut permitted: Vec<_> = quantities
            .iter()
            .enumerate()
            .map(|(i, &q)| if super::fits_stock(&effective.packing, i) { q } else { 0 })
            .collect();
        for (index, group) in self.groups.iter().enumerate() {
            if group.is_some()
                && self.groups.iter().take(index).enumerate().any(|(i, earlier)| {
                    earlier == group && permitted[i] < self.packing.problem.demands[i].quantity
                })
            {
                permitted[index] = 0;
            }
        }
        let sources: Vec<_> =
            permitted.iter().enumerate().filter(|&(_, q)| *q > 0).map(|(i, _)| i).collect();
        let mut indexed = effective.clone();
        indexed.packing.problem.demands = sources
            .iter()
            .map(|&i| {
                let mut d = self.packing.problem.demands[i].clone();
                d.quantity = permitted[i];
                d
            })
            .collect();
        indexed.groups = if self.groups.is_empty() {
            Vec::new()
        } else {
            sources.iter().map(|&i| self.groups[i]).collect()
        };
        let mut result = if sources.is_empty() {
            empty(&effective)
        } else {
            solve_2d_staged(
                indexed,
                StagedOptions2D {
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
        for tree in &mut result.trees {
            for node in &mut tree.nodes {
                if let StagedAction2D::Part { demand_index, .. } = &mut node.action {
                    *demand_index = sources[*demand_index];
                }
            }
        }
        result.unplaced_quantities =
            self.quantities().iter().zip(&selected).map(|(&q, &s)| q - s).collect();
        result.solution.unplaced = self
            .packing
            .problem
            .demands
            .iter()
            .enumerate()
            .filter(|(i, _)| result.unplaced_quantities[*i] > 0)
            .map(|(i, d)| {
                let mut d = d.clone();
                d.quantity = result.unplaced_quantities[i];
                d
            })
            .collect();
        validate_2d_staged(&effective, &result)?;
        Ok(Candidate {
            containers: result.solution.sheet_count,
            cost: result.solution.total_cost,
            waste: u128::from(result.solution.total_waste_area),
            extents: if one_bin { extents.to_vec() } else { self.dimensions() },
            states: result.explored_states,
            truncated: result.search_truncated,
            problem: effective,
            stocks: result.stock_indices.clone(),
            demands: result.demand_indices.clone(),
            solution: result,
            selected,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::two_d::constrained::ConstrainedProblem2D;
    use crate::two_d::{RectDemand2D, Sheet2D, TwoDProblem};
    fn problem() -> OptimizationProblemStaged2D {
        OptimizationProblemStaged2D {
            packing: StagedProblem2D {
                packing: ConstrainedProblem2D {
                    problem: TwoDProblem {
                        sheets: vec![Sheet2D {
                            name: "sheet".into(),
                            width: 10,
                            height: 5,
                            cost: 1.0,
                            quantity: Some(1),
                            kerf: 0,
                            edge_kerf_relief: false,
                        }],
                        demands: vec![
                            RectDemand2D {
                                name: "full".into(),
                                width: 10,
                                height: 5,
                                quantity: 1,
                                can_rotate: false,
                            },
                            RectDemand2D {
                                name: "half".into(),
                                width: 5,
                                height: 5,
                                quantity: 2,
                                can_rotate: false,
                            },
                        ],
                    },
                    constraints: Vec::new(),
                },
                rules: Default::default(),
                groups: Vec::new(),
            },
            profits: vec![1.0, 10.0],
            minimum_quantities: Vec::new(),
        }
    }
    #[test]
    fn staged_profit_preserves_physical_trees_and_group_predecessors() {
        let r = solve_2d_staged_optimized(
            problem(),
            OptimizationOptions { objective: PackingObjective::Knapsack, ..Default::default() },
        )
        .expect("staged profit");
        assert_eq!(r.report.selected_quantities, vec![0, 2]);
        assert_eq!(r.report.total_profit, 20.0);
        assert!(!r.solution.trees.is_empty());
        validate_2d_staged(&r.effective_problem, &r.solution).expect("physical profit tree");
        let mut p = problem();
        p.packing.groups = vec![Some(7), Some(7)];
        let r = solve_2d_staged_optimized(
            p,
            OptimizationOptions { objective: PackingObjective::Knapsack, ..Default::default() },
        )
        .expect("grouped staged profit");
        assert_eq!(r.report.selected_quantities, vec![1, 0]);
    }
    #[test]
    fn staged_open_dimension_returns_cropped_physical_tree() {
        let mut p = problem();
        p.packing.packing.problem.demands.remove(0);
        p.profits.clear();
        p.packing.packing.problem.sheets[0].width = 100;
        let r = solve_2d_staged_optimized(
            p,
            OptimizationOptions {
                objective: PackingObjective::OpenDimensionX,
                ..Default::default()
            },
        )
        .expect("staged open dimension");
        assert_eq!(r.report.open_extents, vec![10, 5]);
        assert!(r.report.complete);
        assert!(r.report.primary_optimal);
    }

    #[test]
    fn staged_unfit_predecessor_keeps_later_group_types_unplaced() {
        let mut p = problem();
        p.packing.packing.problem.demands[0].width = 11;
        p.packing.groups = vec![Some(7), Some(7)];
        let result = solve_2d_staged_optimized(
            p,
            OptimizationOptions { objective: PackingObjective::Knapsack, ..Default::default() },
        )
        .expect("unfit predecessor is a partial witness");
        assert_eq!(result.report.selected_quantities, vec![0, 0]);
        assert_eq!(result.report.total_profit, 0.0);
        assert!(!result.report.complete);
        validate_2d_staged(&result.effective_problem, &result.solution)
            .expect("group order remains enforced");
    }
}
