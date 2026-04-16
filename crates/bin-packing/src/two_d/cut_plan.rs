//! 2D cut sequencing: guillotine cut trees for single-blade machines,
//! tool paths for CNC routers.
//!
//! See `docs/superpowers/specs/2026-04-15-cut-sequencer-design.md` for
//! the design rationale.

use serde::{Deserialize, Serialize};

use crate::cut_plan::{CutPlanError, Result};

use super::model::{Placement2D, SheetLayout2D, TwoDSolution};

/// Preset cost model for a 2D cutting operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CutPlanPreset2D {
    /// Table saw. Guillotine-compatible layouts only. Costs: cut 1.0,
    /// rotate 2.0, fence_reset 0.5.
    #[default]
    TableSaw,
    /// Panel saw. Guillotine-compatible layouts only. Costs: cut 1.0,
    /// rotate 5.0 (rotating a panel is expensive), fence_reset 0.3.
    PanelSaw,
    /// CNC router. Accepts both guillotine and non-guillotine layouts.
    /// Costs: cut 1.0, tool_up_down 0.2, travel 0.01 per unit.
    CncRouter,
}

/// Resolved cost values after applying the preset and user overrides.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EffectiveCosts2D {
    /// Cost of a single cut step.
    pub cut_cost: f64,
    /// Cost of rotating the workpiece (table/panel saw only).
    pub rotate_cost: f64,
    /// Cost of moving the fence to a new position (table/panel saw only).
    pub fence_reset_cost: f64,
    /// Cost of a tool-up or tool-down transition (CNC only).
    pub tool_up_down_cost: f64,
    /// Cost per unit of tool travel (CNC only).
    pub travel_cost: f64,
}

/// Options controlling how `plan_cuts` scores a 2D plan.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct CutPlanOptions2D {
    /// Cost preset. Defaults to [`CutPlanPreset2D::TableSaw`].
    #[serde(default)]
    pub preset: CutPlanPreset2D,
    /// Override the preset's `cut_cost`.
    #[serde(default)]
    pub cut_cost: Option<f64>,
    /// Override the preset's `rotate_cost` (no effect on CNC router).
    #[serde(default)]
    pub rotate_cost: Option<f64>,
    /// Override the preset's `fence_reset_cost` (no effect on CNC router).
    #[serde(default)]
    pub fence_reset_cost: Option<f64>,
    /// Override the preset's `tool_up_down_cost` (CNC only).
    #[serde(default)]
    pub tool_up_down_cost: Option<f64>,
    /// Override the preset's `travel_cost` (CNC only).
    #[serde(default)]
    pub travel_cost: Option<f64>,
}

/// Axis along which a cut is made.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CutAxis {
    /// Vertical cut: the blade runs along the Y axis, separating regions by X.
    Vertical,
    /// Horizontal cut: the blade runs along the X axis, separating regions by Y.
    Horizontal,
}

/// A single step in a 2D cut plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CutStep2D {
    /// Make a cut along `axis` at `position`. For a vertical cut,
    /// `position` is the x-coordinate; for horizontal, the y-coordinate.
    Cut {
        /// Direction the cut runs.
        axis: CutAxis,
        /// Coordinate where the cut is placed.
        position: u32,
    },
    /// Rotate the workpiece 90° (table / panel saw only).
    Rotate,
    /// Reset the fence to a new position (table / panel saw only).
    FenceReset {
        /// New fence setting.
        new_position: u32,
    },
    /// Lift the tool off the workpiece (CNC router only).
    ToolUp,
    /// Lower the tool onto the workpiece (CNC router only).
    ToolDown,
    /// Travel to a new coordinate with the tool up (CNC router only).
    /// The start of the travel is the previous step's endpoint.
    Travel {
        /// Destination x-coordinate.
        to_x: u32,
        /// Destination y-coordinate.
        to_y: u32,
    },
}

/// Cut plan for a single sheet.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SheetCutPlan2D {
    /// Name of the sheet type consumed by this layout.
    pub sheet_name: String,
    /// Index of the originating `SheetLayout2D` in `solution.layouts`.
    pub sheet_index_in_solution: usize,
    /// Sum of step costs on this sheet.
    pub total_cost: f64,
    /// Number of cut steps emitted.
    pub num_cuts: usize,
    /// Number of rotation steps emitted.
    pub num_rotations: usize,
    /// Number of fence-reset steps emitted.
    pub num_fence_resets: usize,
    /// Number of tool-up steps emitted (tool-down count equals tool-up).
    pub num_tool_ups: usize,
    /// Total distance traveled with the tool up.
    pub travel_distance: u64,
    /// Ordered steps.
    pub steps: Vec<CutStep2D>,
}

/// Cut plan for an entire 2D solution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CutPlanSolution2D {
    /// Preset used to score this plan.
    pub preset: CutPlanPreset2D,
    /// Resolved cost values.
    pub effective_costs: EffectiveCosts2D,
    /// Per-sheet plans, in `solution.layouts` order.
    pub sheet_plans: Vec<SheetCutPlan2D>,
    /// Sum of per-sheet costs.
    pub total_cost: f64,
}

/// Generate a cut plan for a finished 2D solution.
///
/// # Errors
///
/// Returns [`CutPlanError::InvalidOptions`] if any cost override is
/// negative, NaN, or infinite. Returns
/// [`CutPlanError::NonGuillotineNotCuttable`] if the preset requires a
/// single-blade machine (table saw or panel saw) and any sheet layout
/// is not guillotine-compatible.
pub fn plan_cuts(solution: &TwoDSolution, options: &CutPlanOptions2D) -> Result<CutPlanSolution2D> {
    let effective_costs = resolve_costs(options)?;

    let mut sheet_plans = Vec::with_capacity(solution.layouts.len());
    let mut total_cost = 0.0_f64;

    for (index, layout) in solution.layouts.iter().enumerate() {
        let plan = plan_sheet(index, layout, options.preset, &effective_costs)?;
        total_cost += plan.total_cost;
        sheet_plans.push(plan);
    }

    Ok(CutPlanSolution2D { preset: options.preset, effective_costs, sheet_plans, total_cost })
}

fn plan_sheet(
    index: usize,
    layout: &SheetLayout2D,
    preset: CutPlanPreset2D,
    costs: &EffectiveCosts2D,
) -> Result<SheetCutPlan2D> {
    // CNC router always uses per-placement outlines regardless of whether
    // the layout is guillotine-compatible.  Single-blade machines (table
    // saw / panel saw) require a guillotine layout.
    if preset == CutPlanPreset2D::CncRouter {
        return Ok(emit_router_plan(index, layout, costs));
    }

    // If any placement extends past the layout's declared width/height
    // (because the source sheet had `edge_kerf_relief` enabled), expand the
    // root region to enclose them; otherwise the cut-tree reconstructor
    // would reject those placements as outside the region.
    let region_w = layout
        .placements
        .iter()
        .map(|p| p.x.saturating_add(p.width))
        .max()
        .unwrap_or(0)
        .max(layout.width);
    let region_h = layout
        .placements
        .iter()
        .map(|p| p.y.saturating_add(p.height))
        .max()
        .unwrap_or(0)
        .max(layout.height);
    let tree = reconstruct_cut_tree(&layout.placements, 0, 0, region_w, region_h);
    match tree {
        Some(tree) => {
            let mut steps = Vec::new();
            let mut counters = Counters::default();
            let mut prev_axis: Option<CutAxis> = None;
            let mut prev_position: Option<u32> = None;
            emit_guillotine_steps(
                &tree,
                costs,
                &mut steps,
                &mut counters,
                &mut prev_axis,
                &mut prev_position,
            );
            let total_cost = (counters.cuts as f64) * costs.cut_cost
                + (counters.rotations as f64) * costs.rotate_cost
                + (counters.fence_resets as f64) * costs.fence_reset_cost;
            Ok(SheetCutPlan2D {
                sheet_name: layout.sheet_name.clone(),
                sheet_index_in_solution: index,
                total_cost,
                num_cuts: counters.cuts,
                num_rotations: counters.rotations,
                num_fence_resets: counters.fence_resets,
                num_tool_ups: 0,
                travel_distance: 0,
                steps,
            })
        }
        None => {
            Err(CutPlanError::NonGuillotineNotCuttable { sheet_name: layout.sheet_name.clone() })
        }
    }
}

// ---------------------------------------------------------------------------
// Cut-tree reconstruction
// ---------------------------------------------------------------------------

/// Internal node of a reconstructed guillotine cut tree.
#[derive(Debug)]
enum CutTreeNode {
    /// A region occupied by a single placement or a single piece of waste that
    /// needs no further cutting.
    Leaf,
    /// A region with no placements (waste / drop).
    Empty,
    /// A region split by a guillotine cut into two sub-regions.
    Split {
        /// Axis of the cut.
        axis: CutAxis,
        /// Coordinate of the cut along the axis.
        position: u32,
        /// The first (left / top) sub-region.
        first: Box<CutTreeNode>,
        /// The second (right / bottom) sub-region.
        second: Box<CutTreeNode>,
    },
}

/// Try to reconstruct a guillotine cut tree for the placements inside the
/// given region.  Returns `None` when no guillotine decomposition exists.
fn reconstruct_cut_tree(
    placements: &[Placement2D],
    region_x: u32,
    region_y: u32,
    region_width: u32,
    region_height: u32,
) -> Option<CutTreeNode> {
    // Collect placements that are fully contained within this region.
    let inside: Vec<&Placement2D> = placements
        .iter()
        .filter(|p| {
            p.x >= region_x
                && p.y >= region_y
                && p.x + p.width <= region_x + region_width
                && p.y + p.height <= region_y + region_height
        })
        .collect();

    if inside.is_empty() {
        return Some(CutTreeNode::Empty);
    }

    if inside.len() == 1 {
        let only = inside[0];
        // Exactly fills the region → leaf.
        if only.x == region_x
            && only.y == region_y
            && only.width == region_width
            && only.height == region_height
        {
            return Some(CutTreeNode::Leaf);
        }
        // Single placement doesn't fill the whole region; fall through to
        // the general cut-finder which will find a cut separating it from
        // the waste.
    }

    // Try vertical cuts at every candidate x coordinate.
    let v_candidates = collect_vertical_candidates(&inside);
    for cut_x in v_candidates {
        if cut_x <= region_x || cut_x >= region_x + region_width {
            continue;
        }
        // A placement touching the cut line (ending exactly at cut_x) is
        // on the left; a placement starting at cut_x is on the right.
        if inside.iter().all(|p| p.x + p.width <= cut_x || p.x >= cut_x) {
            let first = reconstruct_cut_tree(
                placements,
                region_x,
                region_y,
                cut_x - region_x,
                region_height,
            )?;
            let second = reconstruct_cut_tree(
                placements,
                cut_x,
                region_y,
                region_x + region_width - cut_x,
                region_height,
            )?;
            return Some(CutTreeNode::Split {
                axis: CutAxis::Vertical,
                position: cut_x,
                first: Box::new(first),
                second: Box::new(second),
            });
        }
    }

    // Try horizontal cuts at every candidate y coordinate.
    let h_candidates = collect_horizontal_candidates(&inside);
    for cut_y in h_candidates {
        if cut_y <= region_y || cut_y >= region_y + region_height {
            continue;
        }
        if inside.iter().all(|p| p.y + p.height <= cut_y || p.y >= cut_y) {
            let first = reconstruct_cut_tree(
                placements,
                region_x,
                region_y,
                region_width,
                cut_y - region_y,
            )?;
            let second = reconstruct_cut_tree(
                placements,
                region_x,
                cut_y,
                region_width,
                region_y + region_height - cut_y,
            )?;
            return Some(CutTreeNode::Split {
                axis: CutAxis::Horizontal,
                position: cut_y,
                first: Box::new(first),
                second: Box::new(second),
            });
        }
    }

    // No valid guillotine cut found.
    None
}

fn collect_vertical_candidates(placements: &[&Placement2D]) -> Vec<u32> {
    let mut out: Vec<u32> = placements.iter().flat_map(|p| [p.x, p.x + p.width]).collect();
    out.sort_unstable();
    out.dedup();
    out
}

fn collect_horizontal_candidates(placements: &[&Placement2D]) -> Vec<u32> {
    let mut out: Vec<u32> = placements.iter().flat_map(|p| [p.y, p.y + p.height]).collect();
    out.sort_unstable();
    out.dedup();
    out
}

// ---------------------------------------------------------------------------
// Step emission
// ---------------------------------------------------------------------------

/// Running counters for steps and cost components.
#[derive(Debug, Default)]
struct Counters {
    /// Number of `Cut` steps emitted.
    cuts: usize,
    /// Number of `Rotate` steps emitted.
    rotations: usize,
    /// Number of `FenceReset` steps emitted.
    fence_resets: usize,
}

/// Depth-first traversal of a cut tree, emitting `CutStep2D` values.
///
/// Emits `Rotate` before a cut whose axis differs from the previous emitted
/// cut.  Emits `FenceReset` before a cut whose position differs from the
/// previous emitted cut **on the same axis** (a `Rotate` resets fence
/// tracking, so no `FenceReset` is emitted immediately after a `Rotate`).
fn emit_guillotine_steps(
    node: &CutTreeNode,
    costs: &EffectiveCosts2D,
    steps: &mut Vec<CutStep2D>,
    counters: &mut Counters,
    prev_axis: &mut Option<CutAxis>,
    prev_position: &mut Option<u32>,
) {
    match node {
        CutTreeNode::Leaf | CutTreeNode::Empty => {
            // No cuts inside a leaf or empty region.
        }
        CutTreeNode::Split { axis, position, first, second } => {
            let rotated = if let Some(prev) = *prev_axis { prev != *axis } else { false };

            if rotated {
                steps.push(CutStep2D::Rotate);
                counters.rotations += 1;
                // After a Rotate the fence position context resets; do NOT
                // emit a FenceReset for the first cut on the new axis.
                *prev_position = None;
            } else if let Some(prev) = *prev_position
                && prev != *position
            {
                steps.push(CutStep2D::FenceReset { new_position: *position });
                counters.fence_resets += 1;
            }

            steps.push(CutStep2D::Cut { axis: *axis, position: *position });
            counters.cuts += 1;
            *prev_axis = Some(*axis);
            *prev_position = Some(*position);

            emit_guillotine_steps(first, costs, steps, counters, prev_axis, prev_position);
            emit_guillotine_steps(second, costs, steps, counters, prev_axis, prev_position);
        }
    }
    let _ = costs; // cost already counted via counters
}

// ---------------------------------------------------------------------------
// CNC router path
// ---------------------------------------------------------------------------

/// Build a per-placement outline tool-path for a CNC router.
///
/// Visits placements in nearest-neighbor order from (0, 0).  For each
/// placement emits `ToolDown`, four `Cut` steps tracing the rectangle
/// outline, and `ToolUp`.  A `Travel` step is emitted (with `ToolUp`
/// before it) whenever the router must move between placements.
fn emit_router_plan(
    index: usize,
    layout: &SheetLayout2D,
    costs: &EffectiveCosts2D,
) -> SheetCutPlan2D {
    let mut steps = Vec::new();
    let mut num_cuts = 0_usize;
    let mut num_tool_ups = 0_usize;
    let mut travel_distance = 0_u64;

    let visit_order = nearest_neighbor_order(&layout.placements, 0, 0);

    let mut current_x = 0_u32;
    let mut current_y = 0_u32;
    let mut tool_down = false;

    for &idx in &visit_order {
        let p = &layout.placements[idx];

        // Move to the placement's top-left corner if not already there.
        if current_x != p.x || current_y != p.y {
            if tool_down {
                steps.push(CutStep2D::ToolUp);
                num_tool_ups += 1;
                tool_down = false;
            }
            travel_distance =
                travel_distance.saturating_add(manhattan(current_x, current_y, p.x, p.y));
            steps.push(CutStep2D::Travel { to_x: p.x, to_y: p.y });
        }

        if !tool_down {
            steps.push(CutStep2D::ToolDown);
            tool_down = true;
        }

        // Trace the four edges of the rectangle as Cut steps.
        // The tool ends back at (p.x, p.y) after the full outline.

        // Top edge → right: horizontal cut, position = right edge x.
        steps.push(CutStep2D::Cut { axis: CutAxis::Horizontal, position: p.x + p.width });
        num_cuts += 1;

        // Right edge → bottom: vertical cut, position = bottom edge y.
        steps.push(CutStep2D::Cut { axis: CutAxis::Vertical, position: p.y + p.height });
        num_cuts += 1;

        // Bottom edge → left: horizontal cut, position = left edge x.
        steps.push(CutStep2D::Cut { axis: CutAxis::Horizontal, position: p.x });
        num_cuts += 1;

        // Left edge → top: vertical cut, position = top edge y.
        steps.push(CutStep2D::Cut { axis: CutAxis::Vertical, position: p.y });
        num_cuts += 1;

        // Tool is now back at (p.x, p.y); record this for next iteration.
        current_x = p.x;
        current_y = p.y;
    }

    if tool_down {
        steps.push(CutStep2D::ToolUp);
        num_tool_ups += 1;
    }

    let total_cost = (num_cuts as f64) * costs.cut_cost
        + (num_tool_ups as f64) * costs.tool_up_down_cost
        + (travel_distance as f64) * costs.travel_cost;

    SheetCutPlan2D {
        sheet_name: layout.sheet_name.clone(),
        sheet_index_in_solution: index,
        total_cost,
        num_cuts,
        num_rotations: 0,
        num_fence_resets: 0,
        num_tool_ups,
        travel_distance,
        steps,
    }
}

/// Returns placement indices ordered by nearest-neighbor greedy traversal
/// starting from (`start_x`, `start_y`).
fn nearest_neighbor_order(placements: &[Placement2D], start_x: u32, start_y: u32) -> Vec<usize> {
    let mut visited = vec![false; placements.len()];
    let mut order = Vec::with_capacity(placements.len());
    let mut cx = start_x;
    let mut cy = start_y;

    for _ in 0..placements.len() {
        let mut best: Option<(usize, u64)> = None;
        for (i, p) in placements.iter().enumerate() {
            if visited[i] {
                continue;
            }
            let d = manhattan(cx, cy, p.x, p.y);
            if best.is_none_or(|(_, bd)| d < bd) {
                best = Some((i, d));
            }
        }
        if let Some((i, _)) = best {
            visited[i] = true;
            order.push(i);
            cx = placements[i].x;
            cy = placements[i].y;
        }
    }

    order
}

/// Manhattan distance between two integer coordinates, widened to `u64`.
fn manhattan(ax: u32, ay: u32, bx: u32, by: u32) -> u64 {
    u64::from(ax.abs_diff(bx)) + u64::from(ay.abs_diff(by))
}

fn resolve_costs(options: &CutPlanOptions2D) -> Result<EffectiveCosts2D> {
    let (cut, rotate, fence, tool_up_down, travel) = match options.preset {
        CutPlanPreset2D::TableSaw => (1.0, 2.0, 0.5, 0.0, 0.0),
        CutPlanPreset2D::PanelSaw => (1.0, 5.0, 0.3, 0.0, 0.0),
        CutPlanPreset2D::CncRouter => (1.0, 0.0, 0.0, 0.2, 0.01),
    };

    let cut_cost = validate_cost("cut_cost", options.cut_cost.unwrap_or(cut))?;
    let rotate_cost = validate_cost("rotate_cost", options.rotate_cost.unwrap_or(rotate))?;
    let fence_reset_cost =
        validate_cost("fence_reset_cost", options.fence_reset_cost.unwrap_or(fence))?;
    let tool_up_down_cost =
        validate_cost("tool_up_down_cost", options.tool_up_down_cost.unwrap_or(tool_up_down))?;
    let travel_cost = validate_cost("travel_cost", options.travel_cost.unwrap_or(travel))?;

    Ok(EffectiveCosts2D { cut_cost, rotate_cost, fence_reset_cost, tool_up_down_cost, travel_cost })
}

fn validate_cost(name: &str, value: f64) -> Result<f64> {
    if !value.is_finite() || value < 0.0 {
        return Err(CutPlanError::InvalidOptions(format!(
            "{name} must be a non-negative finite number, got {value}"
        )));
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::two_d::{
        Placement2D, RectDemand2D, Sheet2D, SolverMetrics2D, TwoDAlgorithm, TwoDOptions,
        TwoDProblem, TwoDSolution, solve_2d,
    };

    #[test]
    fn default_options_use_table_saw_preset() {
        let options = CutPlanOptions2D::default();
        assert_eq!(options.preset, CutPlanPreset2D::TableSaw);
    }

    #[test]
    fn table_saw_preset_defaults() {
        let options = CutPlanOptions2D::default();
        let costs = resolve_costs(&options).expect("valid defaults");
        assert_eq!(costs.cut_cost, 1.0);
        assert_eq!(costs.rotate_cost, 2.0);
        assert_eq!(costs.fence_reset_cost, 0.5);
        assert_eq!(costs.tool_up_down_cost, 0.0);
        assert_eq!(costs.travel_cost, 0.0);
    }

    #[test]
    fn panel_saw_preset_defaults() {
        let options =
            CutPlanOptions2D { preset: CutPlanPreset2D::PanelSaw, ..CutPlanOptions2D::default() };
        let costs = resolve_costs(&options).expect("valid defaults");
        assert_eq!(costs.rotate_cost, 5.0);
        assert_eq!(costs.fence_reset_cost, 0.3);
    }

    #[test]
    fn cnc_router_preset_defaults() {
        let options =
            CutPlanOptions2D { preset: CutPlanPreset2D::CncRouter, ..CutPlanOptions2D::default() };
        let costs = resolve_costs(&options).expect("valid defaults");
        assert_eq!(costs.tool_up_down_cost, 0.2);
        assert_eq!(costs.travel_cost, 0.01);
        assert_eq!(costs.rotate_cost, 0.0);
        assert_eq!(costs.fence_reset_cost, 0.0);
    }

    #[test]
    fn override_wins_over_preset_default() {
        let options = CutPlanOptions2D {
            preset: CutPlanPreset2D::TableSaw,
            cut_cost: Some(2.5),
            rotate_cost: None,
            fence_reset_cost: Some(0.9),
            tool_up_down_cost: None,
            travel_cost: None,
        };
        let costs = resolve_costs(&options).expect("valid overrides");
        assert_eq!(costs.cut_cost, 2.5);
        assert_eq!(costs.rotate_cost, 2.0); // preset default
        assert_eq!(costs.fence_reset_cost, 0.9);
    }

    #[test]
    fn negative_cost_rejected() {
        let options = CutPlanOptions2D {
            preset: CutPlanPreset2D::TableSaw,
            cut_cost: Some(-1.0),
            ..CutPlanOptions2D::default()
        };
        let result = resolve_costs(&options);
        assert!(matches!(result, Err(CutPlanError::InvalidOptions(_))));
    }

    #[test]
    fn nan_cost_rejected() {
        let options = CutPlanOptions2D {
            preset: CutPlanPreset2D::TableSaw,
            rotate_cost: Some(f64::NAN),
            ..CutPlanOptions2D::default()
        };
        let result = resolve_costs(&options);
        assert!(matches!(result, Err(CutPlanError::InvalidOptions(_))));
    }

    #[test]
    fn infinite_cost_rejected() {
        let options = CutPlanOptions2D {
            preset: CutPlanPreset2D::TableSaw,
            travel_cost: Some(f64::INFINITY),
            ..CutPlanOptions2D::default()
        };
        let result = resolve_costs(&options);
        assert!(matches!(result, Err(CutPlanError::InvalidOptions(_))));
    }

    // -----------------------------------------------------------------------
    // Task 4 tests
    // -----------------------------------------------------------------------

    #[test]
    fn single_placement_full_sheet_needs_no_cuts() {
        let problem = TwoDProblem {
            sheets: vec![Sheet2D {
                name: "s".to_string(),
                width: 10,
                height: 10,
                cost: 1.0,
                quantity: None,
                kerf: 0,
                edge_kerf_relief: false,
            }],
            demands: vec![RectDemand2D {
                name: "p".to_string(),
                width: 10,
                height: 10,
                quantity: 1,
                can_rotate: false,
            }],
        };
        let solution = solve_2d(problem, TwoDOptions::default()).expect("solve");
        let plan = plan_cuts(&solution, &CutPlanOptions2D::default()).expect("plan");
        assert_eq!(plan.sheet_plans.len(), 1);
        assert!(plan.sheet_plans[0].steps.is_empty());
        assert_eq!(plan.sheet_plans[0].total_cost, 0.0);
    }

    #[test]
    fn two_side_by_side_placements_yield_one_vertical_cut() {
        let problem = TwoDProblem {
            sheets: vec![Sheet2D {
                name: "s".to_string(),
                width: 10,
                height: 5,
                cost: 1.0,
                quantity: None,
                kerf: 0,
                edge_kerf_relief: false,
            }],
            demands: vec![RectDemand2D {
                name: "p".to_string(),
                width: 5,
                height: 5,
                quantity: 2,
                can_rotate: false,
            }],
        };
        let solution = solve_2d(
            problem,
            TwoDOptions { algorithm: TwoDAlgorithm::Guillotine, ..Default::default() },
        )
        .expect("solve");
        let plan = plan_cuts(&solution, &CutPlanOptions2D::default()).expect("plan");
        let sheet_plan = &plan.sheet_plans[0];
        assert_eq!(sheet_plan.num_cuts, 1);
        assert_eq!(sheet_plan.num_rotations, 0);
        match &sheet_plan.steps[0] {
            CutStep2D::Cut { axis: CutAxis::Vertical, position } => {
                assert_eq!(*position, 5);
            }
            other => panic!("expected vertical cut at 5, got {other:?}"),
        }
    }

    #[test]
    fn table_saw_rejects_non_guillotine_layout() {
        let sheets = vec![Sheet2D {
            name: "s".to_string(),
            width: 10,
            height: 10,
            cost: 1.0,
            quantity: None,
            kerf: 0,
            edge_kerf_relief: false,
        }];
        // Classic pinwheel: four placements arranged so no full-width or
        // full-height cut can avoid crossing a placement.
        let pinwheel_placements = vec![
            Placement2D { name: "a".to_string(), x: 0, y: 0, width: 6, height: 4, rotated: false },
            Placement2D { name: "b".to_string(), x: 6, y: 0, width: 4, height: 6, rotated: false },
            Placement2D { name: "c".to_string(), x: 4, y: 6, width: 6, height: 4, rotated: false },
            Placement2D { name: "d".to_string(), x: 0, y: 4, width: 4, height: 6, rotated: false },
        ];
        let pinwheel = TwoDSolution::from_layouts(
            "hand_built_pinwheel",
            false,
            &sheets,
            vec![(0, pinwheel_placements)],
            Vec::new(),
            SolverMetrics2D { iterations: 0, explored_states: 0, notes: Vec::new() },
            0,
        );
        let options =
            CutPlanOptions2D { preset: CutPlanPreset2D::TableSaw, ..CutPlanOptions2D::default() };
        let result = plan_cuts(&pinwheel, &options);
        assert!(
            matches!(
                result,
                Err(CutPlanError::NonGuillotineNotCuttable { ref sheet_name }) if sheet_name == "s"
            ),
            "expected NonGuillotineNotCuttable, got {result:?}",
        );
    }

    // -----------------------------------------------------------------------
    // Task 5 tests
    // -----------------------------------------------------------------------

    #[test]
    fn cnc_router_handles_non_guillotine_layout() {
        let sheets = vec![Sheet2D {
            name: "s".to_string(),
            width: 10,
            height: 10,
            cost: 1.0,
            quantity: None,
            kerf: 0,
            edge_kerf_relief: false,
        }];
        // Classic pinwheel: four placements that cannot be guillotine-cut.
        let placements = vec![
            Placement2D { name: "a".to_string(), x: 0, y: 0, width: 6, height: 4, rotated: false },
            Placement2D { name: "b".to_string(), x: 6, y: 0, width: 4, height: 6, rotated: false },
            Placement2D { name: "c".to_string(), x: 4, y: 6, width: 6, height: 4, rotated: false },
            Placement2D { name: "d".to_string(), x: 0, y: 4, width: 4, height: 6, rotated: false },
        ];
        let solution = TwoDSolution::from_layouts(
            "hand_built_pinwheel",
            false,
            &sheets,
            vec![(0, placements)],
            Vec::new(),
            SolverMetrics2D { iterations: 0, explored_states: 0, notes: Vec::new() },
            0,
        );
        let options =
            CutPlanOptions2D { preset: CutPlanPreset2D::CncRouter, ..CutPlanOptions2D::default() };
        let plan = plan_cuts(&solution, &options).expect("CNC router should handle non-guillotine");
        let sheet_plan = &plan.sheet_plans[0];
        assert_eq!(sheet_plan.num_cuts, 16, "4 placements × 4 edges each");
        assert!(sheet_plan.num_tool_ups > 0);
        assert!(sheet_plan.travel_distance > 0);
    }

    #[test]
    fn cnc_router_on_guillotine_layout_also_uses_router_path() {
        // Two side-by-side 5×5 placements: a guillotine-compatible layout.
        // With CncRouter preset the router path must be used (not guillotine),
        // so we expect 8 cuts (2 placements × 4 edges) and at least 1 tool-up.
        let problem = TwoDProblem {
            sheets: vec![Sheet2D {
                name: "s".to_string(),
                width: 10,
                height: 5,
                cost: 1.0,
                quantity: None,
                kerf: 0,
                edge_kerf_relief: false,
            }],
            demands: vec![RectDemand2D {
                name: "p".to_string(),
                width: 5,
                height: 5,
                quantity: 2,
                can_rotate: false,
            }],
        };
        let solution = solve_2d(
            problem,
            TwoDOptions { algorithm: TwoDAlgorithm::Guillotine, ..Default::default() },
        )
        .expect("solve");
        let options =
            CutPlanOptions2D { preset: CutPlanPreset2D::CncRouter, ..CutPlanOptions2D::default() };
        let plan = plan_cuts(&solution, &options).expect("plan");
        let sheet_plan = &plan.sheet_plans[0];
        assert_eq!(sheet_plan.num_cuts, 8, "2 placements × 4 edges each");
        assert!(sheet_plan.num_tool_ups > 0);
    }

    #[test]
    fn cut_plan_reconstructs_layout_with_edge_kerf_relief_overrun() {
        use crate::two_d::model::SheetLayout2D;

        // Hand-built layout where part B trails at x=25..49 — 1 unit past
        // the layout's declared width of 48.
        let layout = SheetLayout2D {
            sheet_name: "s".into(),
            width: 48,
            height: 10,
            cost: 1.0,
            placements: vec![
                Placement2D { name: "a".into(), x: 0, y: 0, width: 24, height: 10, rotated: false },
                Placement2D {
                    name: "b".into(),
                    x: 25,
                    y: 0,
                    width: 24,
                    height: 10,
                    rotated: false,
                },
            ],
            used_area: 470,
            waste_area: 10,
            kerf_area: 10,
            largest_usable_drop_area: 0,
            sum_sq_usable_drop_areas: 0,
        };
        let solution = TwoDSolution {
            algorithm: "test".into(),
            guillotine: true,
            sheet_count: 1,
            total_waste_area: 10,
            total_kerf_area: 10,
            total_cost: 1.0,
            max_usable_drop_area: 0,
            total_sum_sq_usable_drop_areas: 0,
            layouts: vec![layout],
            unplaced: Vec::new(),
            metrics: SolverMetrics2D { iterations: 0, explored_states: 0, notes: Vec::new() },
        };

        let opts =
            CutPlanOptions2D { preset: CutPlanPreset2D::TableSaw, ..CutPlanOptions2D::default() };
        let plan = plan_cuts(&solution, &opts).expect("cut plan should reconstruct");
        assert_eq!(plan.sheet_plans.len(), 1);
        assert!(
            plan.sheet_plans[0].num_cuts >= 1,
            "two-piece layout must produce at least one cut"
        );
    }
}
