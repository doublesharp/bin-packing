//! 1D cut sequencing: ordered cut list per bar, fence-reset-aware cost.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::cut_plan::{CutPlanError, Result};

use super::model::{OneDProblem, OneDSolution, StockLayout1D};

/// Preset cost model for a 1D cutting operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CutPlanPreset1D {
    /// Chop saw / miter saw with a sliding stop block.
    #[default]
    ChopSaw,
}

/// Resolved cost values after applying the preset and user overrides.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EffectiveCosts1D {
    /// Cost of a single cut.
    pub cut_cost: f64,
    /// Cost of moving the fence / stop block to a new position.
    pub fence_reset_cost: f64,
}

/// Options controlling how `plan_cuts` scores a 1D plan.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct CutPlanOptions1D {
    /// Cost preset. Defaults to [`CutPlanPreset1D::ChopSaw`].
    #[serde(default)]
    pub preset: CutPlanPreset1D,
    /// Override the preset's `cut_cost`.
    #[serde(default)]
    pub cut_cost: Option<f64>,
    /// Override the preset's `fence_reset_cost`.
    #[serde(default)]
    pub fence_reset_cost: Option<f64>,
}

/// A single step in a 1D cut plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CutStep1D {
    /// Make a cut at `position` units from the start of the bar,
    /// producing a piece named `piece_name`.
    Cut {
        /// Absolute position of the cut in the bar's coordinate system
        /// (not the cut length — the distance from the bar origin where
        /// the blade enters).
        position: u32,
        /// Name of the piece the cut produces.
        piece_name: String,
    },
    /// Reset the fence / stop block to `new_position` so the next cut
    /// produces a piece of that length.
    FenceReset {
        /// New fence setting (length of the next piece).
        new_position: u32,
    },
}

/// Cut plan for a single bar.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BarCutPlan1D {
    /// Name of the stock type consumed by this bar.
    pub stock_name: String,
    /// Index of the originating `StockLayout1D` in `solution.layouts`.
    pub bar_index_in_solution: usize,
    /// Sum of step costs on this bar.
    pub total_cost: f64,
    /// Number of cut steps emitted.
    pub num_cuts: usize,
    /// Number of fence-reset steps emitted.
    pub num_fence_resets: usize,
    /// Ordered steps.
    pub steps: Vec<CutStep1D>,
}

/// Cut plan for an entire 1D solution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CutPlanSolution1D {
    /// Preset used to score this plan.
    pub preset: CutPlanPreset1D,
    /// Resolved cost values.
    pub effective_costs: EffectiveCosts1D,
    /// Per-bar plans, in `solution.layouts` order.
    pub bar_plans: Vec<BarCutPlan1D>,
    /// Sum of per-bar costs.
    pub total_cost: f64,
}

/// Generate a cut plan for a finished 1D solution.
///
/// The `problem` is required (in addition to the `solution`) because cut
/// positions include cumulative kerf, and kerf is authoritative in the
/// problem's stock definitions rather than in the solution's derived metrics.
///
/// # Errors
///
/// Returns [`CutPlanError::InvalidOptions`] if any cost override is
/// negative, NaN, or infinite.
pub fn plan_cuts(
    problem: &OneDProblem,
    solution: &OneDSolution,
    options: &CutPlanOptions1D,
) -> Result<CutPlanSolution1D> {
    let effective_costs = resolve_costs(options)?;

    // Lookup kerf per stock name. Use the problem's authoritative
    // definitions rather than trying to recover kerf from the
    // solution's derived metrics.
    let kerf_by_stock: HashMap<&str, u32> =
        problem.stock.iter().map(|s| (s.name.as_str(), s.kerf)).collect();

    let mut bar_plans = Vec::with_capacity(solution.layouts.len());
    let mut total_cost = 0.0_f64;

    for (bar_index, layout) in solution.layouts.iter().enumerate() {
        let kerf = kerf_by_stock.get(layout.stock_name.as_str()).copied().unwrap_or(0);
        let plan = plan_bar(bar_index, layout, kerf, &effective_costs);
        total_cost += plan.total_cost;
        bar_plans.push(plan);
    }

    Ok(CutPlanSolution1D { preset: options.preset, effective_costs, bar_plans, total_cost })
}

fn resolve_costs(options: &CutPlanOptions1D) -> Result<EffectiveCosts1D> {
    let (default_cut, default_reset) = match options.preset {
        CutPlanPreset1D::ChopSaw => (1.0_f64, 0.3_f64),
    };

    let cut_cost = validate_cost("cut_cost", options.cut_cost.unwrap_or(default_cut))?;
    let fence_reset_cost =
        validate_cost("fence_reset_cost", options.fence_reset_cost.unwrap_or(default_reset))?;

    Ok(EffectiveCosts1D { cut_cost, fence_reset_cost })
}

fn validate_cost(name: &str, value: f64) -> Result<f64> {
    if !value.is_finite() || value < 0.0 {
        return Err(CutPlanError::InvalidOptions(format!(
            "{name} must be a non-negative finite number, got {value}"
        )));
    }
    Ok(value)
}

fn plan_bar(
    bar_index: usize,
    layout: &StockLayout1D,
    kerf: u32,
    costs: &EffectiveCosts1D,
) -> BarCutPlan1D {
    let mut steps = Vec::with_capacity(layout.cuts.len() * 2);
    let mut num_cuts = 0_usize;
    let mut num_fence_resets = 0_usize;
    let mut last_reset: Option<u32> = None;
    let mut cursor: u64 = 0;

    for cut in &layout.cuts {
        if last_reset != Some(cut.length) {
            steps.push(CutStep1D::FenceReset { new_position: cut.length });
            num_fence_resets += 1;
            last_reset = Some(cut.length);
        }
        cursor = cursor.saturating_add(u64::from(cut.length));
        #[allow(clippy::cast_possible_truncation)]
        let position = cursor.min(u64::from(u32::MAX)) as u32;
        steps.push(CutStep1D::Cut { position, piece_name: cut.name.clone() });
        num_cuts += 1;
        // Kerf consumes material after every cut except the final one on
        // this bar (the final cut produces the last piece; nothing is cut
        // afterward). The cursor advance happens before the next cut's
        // position is computed, so adding kerf here is always correct —
        // the cursor won't be read again after the last iteration.
        cursor = cursor.saturating_add(u64::from(kerf));
    }

    let total_cost =
        (num_cuts as f64) * costs.cut_cost + (num_fence_resets as f64) * costs.fence_reset_cost;

    BarCutPlan1D {
        stock_name: layout.stock_name.clone(),
        bar_index_in_solution: bar_index,
        total_cost,
        num_cuts,
        num_fence_resets,
        steps,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::one_d::{
        CutDemand1D, OneDOptions, OneDProblem, OneDSolution, SolverMetrics1D, Stock1D,
    };

    fn empty_problem() -> OneDProblem {
        OneDProblem {
            stock: vec![Stock1D {
                name: "bar".to_string(),
                length: 100,
                kerf: 2,
                trim: 0,
                cost: 1.0,
                available: None,
            }],
            demands: vec![CutDemand1D { name: "cut".to_string(), length: 10, quantity: 1 }],
        }
    }

    fn empty_solution() -> OneDSolution {
        OneDSolution {
            algorithm: "test".to_string(),
            exact: false,
            lower_bound: None,
            stock_count: 0,
            total_waste: 0,
            total_cost: 0.0,
            layouts: Vec::new(),
            stock_requirements: Vec::new(),
            unplaced: Vec::new(),
            metrics: SolverMetrics1D {
                iterations: 0,
                generated_patterns: 0,
                enumerated_patterns: 0,
                explored_states: 0,
                notes: Vec::new(),
            },
        }
    }

    #[test]
    fn chop_saw_preset_yields_1_cut_cost_and_point_three_fence_reset() {
        let options = CutPlanOptions1D::default();
        let plan = plan_cuts(&empty_problem(), &empty_solution(), &options)
            .expect("empty solution should plan cleanly");
        assert_eq!(plan.preset, CutPlanPreset1D::ChopSaw);
        assert_eq!(plan.effective_costs.cut_cost, 1.0);
        assert_eq!(plan.effective_costs.fence_reset_cost, 0.3);
        assert!(plan.bar_plans.is_empty());
        assert_eq!(plan.total_cost, 0.0);
    }

    #[test]
    fn invalid_override_is_rejected() {
        let options = CutPlanOptions1D {
            preset: CutPlanPreset1D::ChopSaw,
            cut_cost: Some(-1.0),
            fence_reset_cost: None,
        };
        let result = plan_cuts(&empty_problem(), &empty_solution(), &options);
        assert!(matches!(result, Err(CutPlanError::InvalidOptions(_))));
    }

    #[test]
    fn kerf_is_included_in_cut_positions() {
        let problem = OneDProblem {
            stock: vec![Stock1D {
                name: "bar".to_string(),
                length: 100,
                kerf: 2,
                trim: 0,
                cost: 1.0,
                available: None,
            }],
            demands: vec![CutDemand1D { name: "rail".to_string(), length: 30, quantity: 3 }],
        };
        let solution =
            crate::one_d::solve_1d(problem.clone(), OneDOptions::default()).expect("solve");
        let plan = plan_cuts(&problem, &solution, &CutPlanOptions1D::default()).expect("plan");

        assert_eq!(plan.bar_plans.len(), 1);
        let bar = &plan.bar_plans[0];
        assert_eq!(bar.num_cuts, 3);
        // Three identical cuts: 1 fence reset (before the first), not 3.
        assert_eq!(bar.num_fence_resets, 1);

        let positions: Vec<u32> = bar
            .steps
            .iter()
            .filter_map(|step| match step {
                CutStep1D::Cut { position, .. } => Some(*position),
                _ => None,
            })
            .collect();
        assert_eq!(positions, vec![30, 62, 94]);
    }
}
