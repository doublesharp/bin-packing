//! Emit actual serialized defaults for the generated documentation reference.

use bin_packing::{irregular, one_d, optimization, three_d, two_d};
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let linear: one_d::OneDProblem = serde_json::from_value(json!({
        "stock": [{"name": "stock", "length": 2}],
        "demands": [{"name": "part", "length": 1, "quantity": 1}]
    }))?;
    let linear_solution = one_d::solve_1d(linear.clone(), one_d::OneDOptions::default())?;
    let chop = one_d::cut_plan::plan_cuts(&linear, &linear_solution, &Default::default())?;
    let sheet = serde_json::from_value(json!({
        "sheets": [{"name": "stock", "width": 2, "height": 1}],
        "demands": [{"name": "part", "width": 1, "height": 1, "quantity": 1}]
    }))?;
    let sheet_solution = two_d::solve_2d(sheet, two_d::TwoDOptions::default())?;
    let table = two_d::cut_plan::plan_cuts(&sheet_solution, &Default::default())?;
    let panel = two_d::cut_plan::plan_cuts(
        &sheet_solution,
        &two_d::cut_plan::CutPlanOptions2D {
            preset: two_d::cut_plan::CutPlanPreset2D::PanelSaw,
            ..Default::default()
        },
    )?;
    let cnc = two_d::cut_plan::plan_cuts(
        &sheet_solution,
        &two_d::cut_plan::CutPlanOptions2D {
            preset: two_d::cut_plan::CutPlanPreset2D::CncRouter,
            ..Default::default()
        },
    )?;
    let defaults = json!({
        "OneDOptions": one_d::OneDOptions::default(),
        "TwoDOptions": two_d::TwoDOptions::default(),
        "ThreeDOptions": three_d::ThreeDOptions::default(),
        "OptimizationOptions": optimization::OptimizationOptions::default(),
        "ConstrainedOptions2D": two_d::constrained::ConstrainedOptions2D::default(),
        "ConstrainedOptions3D": three_d::constrained::ConstrainedOptions3D::default(),
        "AdaptiveOptions2D": two_d::adaptive::AdaptiveOptions2D::default(),
        "StagedOptions2D": two_d::staged::StagedOptions2D::default(),
        "StagedRules2D": two_d::staged::StagedRules2D::default(),
        "LoadingOptions3D": three_d::loading::LoadingOptions3D::default(),
        "IrregularOptions": irregular::IrregularOptions::default(),
        "SawPlanOptions2D": two_d::saw_plan::SawPlanOptions2D::default(),
        "SawSearchLimits2D": two_d::saw_plan::SawSearchLimits2D::default(),
        "SawWeights2D": two_d::saw_plan::SawWeights2D::default(),
        "ReuseOptions2D": two_d::reuse::ReuseOptions2D::default(),
        "CutPlanOptions1D": one_d::cut_plan::CutPlanOptions1D::default(),
        "CutPlanOptions2D": two_d::cut_plan::CutPlanOptions2D::default(),
        "CutPlanPresetCosts": {
            "chop_saw": chop.effective_costs,
            "table_saw": table.effective_costs,
            "panel_saw": panel.effective_costs,
            "cnc_router": cnc.effective_costs,
        },
    });
    println!("{}", serde_json::to_string(&defaults)?);
    Ok(())
}
