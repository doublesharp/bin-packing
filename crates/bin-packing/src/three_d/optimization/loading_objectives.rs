use super::{OptimizationOptions, PackingObjective};
use crate::optimization::{Candidate, Model};
use crate::three_d::constrained::PackingObjective3D;
use crate::three_d::loading::{
    FloorAxis3D, LoadingOptions3D, LoadingProblem3D, LoadingSolution3D, UnplacedLoadingItem3D,
    solve_loading_3d, validate_loading_3d,
};
use crate::{BinPackingError, Result};

/// Stack-loading input with source-indexed profits and mandatory quantities.
pub type OptimizationProblemLoading3D = crate::optimization::OptimizationProblem<LoadingProblem3D>;
/// Constraint-preserving nested columns and explicit objective evidence.
pub type OptimizedSolutionLoading3D =
    crate::optimization::OptimizedSolution<LoadingSolution3D, LoadingProblem3D>;

/// Optimize loading profit, feasibility, material or one-container dimensions.
///
/// Common 128-copy/16-type search bounds apply. Open crops retain original axle
/// positions, nesting, density, payload and unloading constraints. The volume
/// bound discounts every allowed nesting height, so it cannot overstate the
/// material occupied by overlapping nested items.
///
/// # Errors
/// Returns malformed source/value/control errors or native loading errors.
pub fn solve_loading_3d_optimized(
    problem: OptimizationProblemLoading3D,
    options: OptimizationOptions,
) -> Result<OptimizedSolutionLoading3D> {
    crate::optimization::solve(problem, options)
}

fn empty(problem: &LoadingProblem3D) -> LoadingSolution3D {
    LoadingSolution3D {
        layouts: Vec::new(),
        unplaced: problem
            .packing
            .problem
            .demands
            .iter()
            .enumerate()
            .map(|(demand_index, d)| UnplacedLoadingItem3D { demand_index, quantity: d.quantity })
            .collect(),
        bin_count: 0,
        total_cost: 0.0,
        explored_states: 0,
        search_truncated: false,
    }
}

impl Model for LoadingProblem3D {
    type Solution = LoadingSolution3D;
    fn validate(&self) -> Result<()> {
        validate_loading_3d(self, &empty(self))
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
        let capacity = self
            .packing
            .problem
            .bins
            .iter()
            .map(|b| u128::from(b.width) * u128::from(b.height) * u128::from(b.depth))
            .max()
            .unwrap_or(1);
        nested_volume(self).div_ceil(capacity).min(usize::MAX as u128) as usize
    }
    fn open_bound(&self, axes: &[usize]) -> u128 {
        let fixed: u128 = self
            .dimensions()
            .iter()
            .enumerate()
            .filter(|(axis, _)| !axes.contains(axis))
            .map(|(_, v)| u128::from(*v))
            .product();
        nested_volume(self).div_ceil(fixed)
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
        effective.packing = super::cropped(&self.packing, extents, one_bin);
        if one_bin {
            for rules in &mut effective.bin_rules {
                if rules.axle_limits.is_some_and(|a| {
                    a.rear_position
                        > match a.axis {
                            FloorAxis3D::X => extents[0],
                            FloorAxis3D::Z => extents[2],
                        }
                }) {
                    return Err(BinPackingError::InvalidConstraint {
                        field: "open_extent".into(),
                        reason: "crop excludes fixed axle support".into(),
                    });
                }
                rules.defects.retain_mut(|d| {
                    d.width = d.width.min(extents[0].saturating_sub(d.x));
                    d.depth = d.depth.min(extents[2].saturating_sub(d.z));
                    d.width > 0 && d.depth > 0
                });
            }
        }
        let sources: Vec<_> =
            quantities.iter().enumerate().filter(|&(_, q)| *q > 0).map(|(i, _)| i).collect();
        let mut indexed = effective.clone();
        indexed.packing.problem.demands = sources
            .iter()
            .map(|&i| {
                let mut d = self.packing.problem.demands[i].clone();
                d.quantity = quantities[i];
                d
            })
            .collect();
        indexed.packing.item_weights = if self.packing.item_weights.is_empty() {
            Vec::new()
        } else {
            sources.iter().map(|&i| self.packing.item_weights[i]).collect()
        };
        indexed.item_rules = if self.item_rules.is_empty() {
            Vec::new()
        } else {
            sources.iter().map(|&i| self.item_rules[i].clone()).collect()
        };
        let mut result = if sources.is_empty() {
            empty(&effective)
        } else {
            solve_loading_3d(
                indexed,
                LoadingOptions3D {
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
        let mut demands = Vec::new();
        for bin in &mut result.layouts {
            let mut indices = Vec::new();
            for stack in &mut bin.stacks {
                for item in &mut stack.items {
                    item.demand_index = sources[item.demand_index];
                    selected[item.demand_index] += 1;
                    indices.push(item.demand_index);
                }
            }
            demands.push(indices);
        }
        result.unplaced = self
            .packing
            .problem
            .demands
            .iter()
            .enumerate()
            .filter(|(i, d)| selected[*i] < d.quantity)
            .map(|(i, d)| UnplacedLoadingItem3D {
                demand_index: i,
                quantity: d.quantity - selected[i],
            })
            .collect();
        validate_loading_3d(&effective, &result)?;
        let waste = result
            .layouts
            .iter()
            .map(|layout| {
                let b = &effective.packing.problem.bins[layout.bin_index];
                let occupied: u128 = layout
                    .stacks
                    .iter()
                    .map(|s| u128::from(s.width) * u128::from(s.height) * u128::from(s.depth))
                    .sum();
                (u128::from(b.width) * u128::from(b.height) * u128::from(b.depth))
                    .saturating_sub(occupied)
            })
            .sum();
        Ok(Candidate {
            containers: result.bin_count,
            cost: result.total_cost,
            waste,
            extents: if one_bin { extents.to_vec() } else { self.dimensions() },
            states: result.explored_states,
            truncated: result.search_truncated,
            stocks: result.layouts.iter().map(|layout| layout.bin_index).collect(),
            problem: effective,
            demands,
            solution: result,
            selected,
        })
    }
}

fn nested_volume(problem: &LoadingProblem3D) -> u128 {
    problem
        .packing
        .problem
        .demands
        .iter()
        .enumerate()
        .map(|(i, d)| {
            let nesting = problem.item_rules.get(i).map_or(0, |r| r.nesting_height);
            u128::from(d.width)
                * u128::from(d.height.saturating_sub(nesting))
                * u128::from(d.depth)
                * d.quantity as u128
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::three_d::constrained::ConstrainedProblem3D;
    use crate::three_d::loading::StackItemRules3D;
    use crate::three_d::{Bin3D, BoxDemand3D, RotationMask3D, ThreeDProblem};
    fn problem() -> OptimizationProblemLoading3D {
        OptimizationProblemLoading3D {
            packing: LoadingProblem3D {
                packing: ConstrainedProblem3D {
                    problem: ThreeDProblem {
                        bins: vec![Bin3D {
                            name: "crate".into(),
                            width: 2,
                            height: 6,
                            depth: 2,
                            cost: 1.0,
                            quantity: Some(1),
                        }],
                        demands: vec![
                            BoxDemand3D {
                                name: "expensive".into(),
                                width: 2,
                                height: 6,
                                depth: 2,
                                quantity: 1,
                                allowed_rotations: RotationMask3D::XYZ,
                            },
                            BoxDemand3D {
                                name: "nested".into(),
                                width: 2,
                                height: 4,
                                depth: 2,
                                quantity: 2,
                                allowed_rotations: RotationMask3D::XYZ,
                            },
                        ],
                    },
                    item_weights: vec![10, 4],
                    bin_max_weights: vec![Some(10)],
                },
                item_rules: vec![
                    StackItemRules3D::default(),
                    StackItemRules3D { nesting_height: 2, ..Default::default() },
                ],
                bin_rules: Vec::new(),
                unloading: Default::default(),
            },
            profits: vec![1.0, 10.0],
            minimum_quantities: Vec::new(),
        }
    }
    #[test]
    fn loading_profit_retains_nesting_payload_and_indexed_rules() {
        let r = solve_loading_3d_optimized(
            problem(),
            OptimizationOptions { objective: PackingObjective::Knapsack, ..Default::default() },
        )
        .expect("loading profit");
        assert_eq!(r.report.selected_quantities, vec![0, 2]);
        assert_eq!(r.report.total_profit, 20.0);
        assert_eq!(r.solution.layouts[0].stacks[0].height, 6);
        validate_loading_3d(&r.effective_problem, &r.solution).expect("native loading witness");
    }
    #[test]
    fn loading_open_height_bound_accounts_for_nesting_overlap() {
        let mut p = problem();
        p.packing.packing.problem.demands.remove(0);
        p.packing.packing.item_weights.remove(0);
        p.packing.item_rules.remove(0);
        p.profits.clear();
        p.packing.packing.problem.bins[0].height = 100;
        let r = solve_loading_3d_optimized(
            p,
            OptimizationOptions {
                objective: PackingObjective::OpenDimensionY,
                ..Default::default()
            },
        )
        .expect("nested open height");
        assert!(r.report.complete);
        assert_eq!(r.report.open_extents, vec![2, 6, 2]);
        assert_eq!(r.report.primary_bound, Some(4.0));
        assert!(!r.report.primary_optimal);
    }
}
