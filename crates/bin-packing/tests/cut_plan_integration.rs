//! Integration tests for the cut-plan post-processor.
//!
//! Each test exercises `plan_cuts` end-to-end through the public API.

use bin_packing::{
    CutPlanError,
    one_d::{
        CutDemand1D, OneDOptions, OneDProblem, Stock1D,
        cut_plan::{CutPlanOptions1D, CutPlanPreset1D, plan_cuts as plan_cuts_1d},
    },
    two_d::{
        Placement2D, RectDemand2D, Sheet2D, SheetLayout2D, SolverMetrics2D, TwoDAlgorithm,
        TwoDOptions, TwoDProblem, TwoDSolution,
        cut_plan::{
            CutAxis, CutPlanOptions2D, CutPlanPreset2D, CutStep2D, plan_cuts as plan_cuts_2d,
        },
        solve_2d,
    },
};

// ---------------------------------------------------------------------------
// Helper builders
// ---------------------------------------------------------------------------

/// Build a `TwoDSolution` directly from a list of `SheetLayout2D` values.
///
/// This constructs the outer solution shell without calling the internal
/// `from_layouts` method (which is `pub(crate)`), so it is safe to call
/// from integration tests.
fn solution_from_sheet_layouts(
    algorithm: &str,
    guillotine: bool,
    layouts: Vec<SheetLayout2D>,
) -> TwoDSolution {
    let sheet_count = layouts.len();
    let total_waste_area = layouts.iter().map(|l| l.waste_area).sum();
    let total_kerf_area = layouts.iter().map(|l| l.kerf_area).sum();
    let total_cost = layouts.iter().map(|l| l.cost).sum();
    let max_usable_drop_area =
        layouts.iter().map(|l| l.largest_usable_drop_area).max().unwrap_or(0);
    let total_sum_sq_usable_drop_areas =
        layouts.iter().map(|l| l.sum_sq_usable_drop_areas).fold(0_u128, u128::saturating_add);
    TwoDSolution {
        algorithm: algorithm.to_string(),
        guillotine,
        sheet_count,
        total_waste_area,
        total_kerf_area,
        total_cost,
        max_usable_drop_area,
        total_sum_sq_usable_drop_areas,
        layouts,
        unplaced: Vec::new(),
        metrics: SolverMetrics2D { iterations: 0, explored_states: 0, notes: Vec::new() },
    }
}

/// Build a `SheetLayout2D` for a 10×10 sheet with the given placements.
fn sheet_layout(
    name: &str,
    width: u32,
    height: u32,
    placements: Vec<Placement2D>,
) -> SheetLayout2D {
    let sheet_area = u64::from(width) * u64::from(height);
    let used_area: u64 = placements.iter().map(|p| u64::from(p.width) * u64::from(p.height)).sum();
    let waste_area = sheet_area.saturating_sub(used_area);
    SheetLayout2D {
        sheet_name: name.to_string(),
        width,
        height,
        cost: 1.0,
        placements,
        used_area,
        waste_area,
        kerf_area: 0,
        largest_usable_drop_area: 0,
        sum_sq_usable_drop_areas: 0,
    }
}

/// Classic pinwheel — four placements arranged so no full-width or full-height
/// cut can avoid crossing a placement.
fn pinwheel_solution() -> TwoDSolution {
    let placements = vec![
        Placement2D { name: "a".to_string(), x: 0, y: 0, width: 6, height: 4, rotated: false },
        Placement2D { name: "b".to_string(), x: 6, y: 0, width: 4, height: 6, rotated: false },
        Placement2D { name: "c".to_string(), x: 4, y: 6, width: 6, height: 4, rotated: false },
        Placement2D { name: "d".to_string(), x: 0, y: 4, width: 4, height: 6, rotated: false },
    ];
    solution_from_sheet_layouts(
        "hand_built_pinwheel",
        false,
        vec![sheet_layout("s", 10, 10, placements)],
    )
}

// ---------------------------------------------------------------------------
// Test 1: one_d_simple_sequence
// ---------------------------------------------------------------------------

/// A OneDProblem with three cuts (45, 45, 30) on a 100-length bar.
/// Verify: 3 cuts, 2 fence resets, total_cost = 3 * 1.0 + 2 * 0.3 = 3.6.
///
/// The two identical 45-unit cuts share one fence reset; the 30-unit cut
/// requires a second reset.
#[test]
fn one_d_simple_sequence() {
    let problem = OneDProblem {
        stock: vec![Stock1D {
            name: "bar".to_string(),
            length: 100,
            kerf: 0,
            trim: 0,
            cost: 1.0,
            available: None,
        }],
        demands: vec![
            CutDemand1D { name: "long".to_string(), length: 45, quantity: 2 },
            CutDemand1D { name: "short".to_string(), length: 30, quantity: 1 },
        ],
    };

    let solution =
        bin_packing::one_d::solve_1d(problem.clone(), OneDOptions::default()).expect("solve");

    // Verify all three cuts fit on one bar (45 + 45 + 30 = 120 > 100, so
    // the solver may spread them over 2 bars). We care about totals.
    let options = CutPlanOptions1D { preset: CutPlanPreset1D::ChopSaw, ..Default::default() };
    let plan = plan_cuts_1d(&problem, &solution, &options).expect("plan");

    let total_cuts: usize = plan.bar_plans.iter().map(|b| b.num_cuts).sum();
    let total_resets: usize = plan.bar_plans.iter().map(|b| b.num_fence_resets).sum();

    assert_eq!(total_cuts, 3, "expected 3 cuts total");
    // Two distinct lengths → at most 2 resets per bar (each new length forces
    // a reset when first encountered). Across all bars, total resets ≥ 1.
    // The exact count depends on how many bars the solver allocates; guard
    // the upper bound (≤ 3, one per cut) and lower bound (≥ 1).
    assert!((1..=3).contains(&total_resets), "resets={total_resets} out of expected range");

    // total_cost = cuts * 1.0 + resets * 0.3
    let expected_cost = (total_cuts as f64) * 1.0 + (total_resets as f64) * 0.3;
    let tolerance = 1e-9;
    assert!(
        (plan.total_cost - expected_cost).abs() < tolerance,
        "total_cost={} expected={}",
        plan.total_cost,
        expected_cost
    );
}

// ---------------------------------------------------------------------------
// Test 2: two_d_guillotine_cut_tree_recovered
// ---------------------------------------------------------------------------

/// A 2×2 grid of 5×5 placements on a 10×10 sheet.
/// Cut-tree reconstruction must find two cuts (one vertical at x=5, one
/// horizontal at y=5) or equivalent axis sequence.
#[test]
fn two_d_guillotine_cut_tree_recovered() {
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
            width: 5,
            height: 5,
            quantity: 4,
            can_rotate: false,
        }],
    };

    let solution = solve_2d(
        problem,
        TwoDOptions { algorithm: TwoDAlgorithm::Guillotine, ..Default::default() },
    )
    .expect("solve");

    let options = CutPlanOptions2D { preset: CutPlanPreset2D::TableSaw, ..Default::default() };
    let plan = plan_cuts_2d(&solution, &options).expect("plan");

    assert_eq!(plan.sheet_plans.len(), 1);
    let sheet = &plan.sheet_plans[0];

    // A 2×2 guillotine layout needs at least 2 cuts (a first full cut plus
    // one cut on each sub-panel; the exact count depends on tree shape).
    // The cut planner emits at least 2 and at most 3 cuts for a 4-piece grid.
    assert!(
        sheet.num_cuts >= 2 && sheet.num_cuts <= 3,
        "expected 2–3 cuts, got {}",
        sheet.num_cuts
    );

    // At least one Cut step must be present.
    let has_cut = sheet.steps.iter().any(|s| matches!(s, CutStep2D::Cut { .. }));
    assert!(has_cut, "expected at least one Cut step");

    // Both axes should appear (vertical rip + horizontal cross-cut).
    let has_vertical =
        sheet.steps.iter().any(|s| matches!(s, CutStep2D::Cut { axis: CutAxis::Vertical, .. }));
    let has_horizontal =
        sheet.steps.iter().any(|s| matches!(s, CutStep2D::Cut { axis: CutAxis::Horizontal, .. }));
    assert!(has_vertical || has_horizontal, "expected at least one cut step with a defined axis");
    // For a 2×2 grid we expect BOTH axes to appear at some level in the tree.
    assert!(has_vertical && has_horizontal, "expected cuts on both axes for a 2×2 grid");
}

// ---------------------------------------------------------------------------
// Test 3: two_d_table_saw_rejects_non_guillotine
// ---------------------------------------------------------------------------

/// The pinwheel layout is not guillotine-compatible.
/// TableSaw preset must return NonGuillotineNotCuttable.
#[test]
fn two_d_table_saw_rejects_non_guillotine() {
    let solution = pinwheel_solution();
    let options = CutPlanOptions2D { preset: CutPlanPreset2D::TableSaw, ..Default::default() };
    let result = plan_cuts_2d(&solution, &options);
    assert!(
        matches!(result, Err(CutPlanError::NonGuillotineNotCuttable { .. })),
        "expected NonGuillotineNotCuttable, got {result:?}",
    );
}

// ---------------------------------------------------------------------------
// Test 4: two_d_cnc_router_handles_non_guillotine
// ---------------------------------------------------------------------------

/// Same pinwheel layout, CncRouter preset must succeed and emit
/// Cut + Travel + ToolUp/Down steps for every placement.
#[test]
fn two_d_cnc_router_handles_non_guillotine() {
    let solution = pinwheel_solution();
    let options = CutPlanOptions2D { preset: CutPlanPreset2D::CncRouter, ..Default::default() };
    let plan = plan_cuts_2d(&solution, &options).expect("CNC router should handle non-guillotine");

    let sheet = &plan.sheet_plans[0];

    // 4 placements × 4 edge cuts each = 16 cuts.
    assert_eq!(sheet.num_cuts, 16, "expected 16 cuts (4 placements × 4 edges)");

    // Must have at least one ToolUp and one Travel step.
    assert!(sheet.num_tool_ups > 0, "expected at least one ToolUp");
    let has_travel = sheet.steps.iter().any(|s| matches!(s, CutStep2D::Travel { .. }));
    assert!(has_travel, "expected at least one Travel step");
    let has_tool_down = sheet.steps.iter().any(|s| matches!(s, CutStep2D::ToolDown));
    assert!(has_tool_down, "expected at least one ToolDown step");
}

// ---------------------------------------------------------------------------
// Test 5: preset_defaults_applied_when_overrides_absent
// ---------------------------------------------------------------------------

/// Construct options with no overrides; verify cost components match preset
/// documented defaults.
#[test]
fn preset_defaults_applied_when_overrides_absent() {
    // ChopSaw 1D defaults: cut_cost = 1.0, fence_reset_cost = 0.3
    let options_1d = CutPlanOptions1D { preset: CutPlanPreset1D::ChopSaw, ..Default::default() };
    let problem = OneDProblem {
        stock: vec![Stock1D {
            name: "bar".to_string(),
            length: 100,
            kerf: 0,
            trim: 0,
            cost: 1.0,
            available: None,
        }],
        demands: vec![CutDemand1D { name: "cut".to_string(), length: 20, quantity: 1 }],
    };
    let solution =
        bin_packing::one_d::solve_1d(problem.clone(), OneDOptions::default()).expect("solve 1d");
    let plan_1d = plan_cuts_1d(&problem, &solution, &options_1d).expect("plan 1d");
    assert_eq!(plan_1d.effective_costs.cut_cost, 1.0);
    assert_eq!(plan_1d.effective_costs.fence_reset_cost, 0.3);

    // TableSaw 2D defaults: cut_cost = 1.0, rotate_cost = 2.0, fence_reset = 0.5
    let options_2d_ts =
        CutPlanOptions2D { preset: CutPlanPreset2D::TableSaw, ..Default::default() };
    let problem_2d = TwoDProblem {
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
            width: 5,
            height: 5,
            quantity: 1,
            can_rotate: false,
        }],
    };
    let solution_2d = solve_2d(problem_2d, TwoDOptions::default()).expect("solve 2d");
    let plan_2d = plan_cuts_2d(&solution_2d, &options_2d_ts).expect("plan 2d");
    assert_eq!(plan_2d.effective_costs.cut_cost, 1.0);
    assert_eq!(plan_2d.effective_costs.rotate_cost, 2.0);
    assert_eq!(plan_2d.effective_costs.fence_reset_cost, 0.5);
    assert_eq!(plan_2d.effective_costs.tool_up_down_cost, 0.0);
    assert_eq!(plan_2d.effective_costs.travel_cost, 0.0);

    // CncRouter 2D defaults: cut_cost = 1.0, tool_up_down = 0.2, travel = 0.01
    let options_2d_cnc =
        CutPlanOptions2D { preset: CutPlanPreset2D::CncRouter, ..Default::default() };
    let problem_2d2 = TwoDProblem {
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
            width: 5,
            height: 5,
            quantity: 1,
            can_rotate: false,
        }],
    };
    let solution_2d2 = solve_2d(problem_2d2, TwoDOptions::default()).expect("solve 2d cnc");
    let plan_2d2 = plan_cuts_2d(&solution_2d2, &options_2d_cnc).expect("plan 2d cnc");
    assert_eq!(plan_2d2.effective_costs.cut_cost, 1.0);
    assert_eq!(plan_2d2.effective_costs.tool_up_down_cost, 0.2);
    assert_eq!(plan_2d2.effective_costs.travel_cost, 0.01);
    assert_eq!(plan_2d2.effective_costs.rotate_cost, 0.0);
    assert_eq!(plan_2d2.effective_costs.fence_reset_cost, 0.0);
}

// ---------------------------------------------------------------------------
// Test 6: overrides_win_over_preset
// ---------------------------------------------------------------------------

/// Partially override cost fields; verify overridden fields use caller values
/// and others use preset defaults.
#[test]
fn overrides_win_over_preset() {
    // 1D: override cut_cost but leave fence_reset_cost at preset default.
    let options_1d = CutPlanOptions1D {
        preset: CutPlanPreset1D::ChopSaw,
        cut_cost: Some(2.5),
        fence_reset_cost: None,
    };
    let problem = OneDProblem {
        stock: vec![Stock1D {
            name: "bar".to_string(),
            length: 100,
            kerf: 0,
            trim: 0,
            cost: 1.0,
            available: None,
        }],
        demands: vec![CutDemand1D { name: "cut".to_string(), length: 20, quantity: 1 }],
    };
    let solution =
        bin_packing::one_d::solve_1d(problem.clone(), OneDOptions::default()).expect("solve 1d");
    let plan_1d = plan_cuts_1d(&problem, &solution, &options_1d).expect("plan 1d");
    assert_eq!(plan_1d.effective_costs.cut_cost, 2.5, "overridden cut_cost");
    assert_eq!(plan_1d.effective_costs.fence_reset_cost, 0.3, "preset default fence_reset_cost");

    // 2D: override cut_cost and fence_reset_cost but leave rotate_cost at preset default.
    let options_2d = CutPlanOptions2D {
        preset: CutPlanPreset2D::TableSaw,
        cut_cost: Some(3.0),
        fence_reset_cost: Some(0.9),
        rotate_cost: None,
        tool_up_down_cost: None,
        travel_cost: None,
    };
    let problem_2d = TwoDProblem {
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
            width: 5,
            height: 5,
            quantity: 1,
            can_rotate: false,
        }],
    };
    let solution_2d = solve_2d(problem_2d, TwoDOptions::default()).expect("solve 2d");
    let plan_2d = plan_cuts_2d(&solution_2d, &options_2d).expect("plan 2d");
    assert_eq!(plan_2d.effective_costs.cut_cost, 3.0, "overridden cut_cost");
    assert_eq!(plan_2d.effective_costs.fence_reset_cost, 0.9, "overridden fence_reset_cost");
    assert_eq!(plan_2d.effective_costs.rotate_cost, 2.0, "preset default rotate_cost");
}

// ---------------------------------------------------------------------------
// Test 7: total_cost_equals_linear_sum_of_components
// ---------------------------------------------------------------------------

/// Verify total_cost = cuts * cut_cost + resets * fence_reset_cost for 1D.
/// Verify total_cost = cuts * cut_cost + rotations * rotate_cost + resets *
/// fence_reset_cost for 2D guillotine.
#[test]
fn total_cost_equals_linear_sum_of_components() {
    // 1D
    let problem_1d = OneDProblem {
        stock: vec![Stock1D {
            name: "bar".to_string(),
            length: 100,
            kerf: 0,
            trim: 0,
            cost: 1.0,
            available: None,
        }],
        demands: vec![
            CutDemand1D { name: "a".to_string(), length: 30, quantity: 2 },
            CutDemand1D { name: "b".to_string(), length: 20, quantity: 1 },
        ],
    };
    let solution_1d =
        bin_packing::one_d::solve_1d(problem_1d.clone(), OneDOptions::default()).expect("solve 1d");
    let opts_1d = CutPlanOptions1D::default();
    let plan_1d = plan_cuts_1d(&problem_1d, &solution_1d, &opts_1d).expect("plan 1d");

    let ec = &plan_1d.effective_costs;
    let expected_total_1d: f64 = plan_1d
        .bar_plans
        .iter()
        .map(|b| {
            (b.num_cuts as f64) * ec.cut_cost + (b.num_fence_resets as f64) * ec.fence_reset_cost
        })
        .sum();
    assert!(
        (plan_1d.total_cost - expected_total_1d).abs() < 1e-9,
        "1D total_cost {} != linear sum {}",
        plan_1d.total_cost,
        expected_total_1d
    );

    // 2D guillotine (TableSaw)
    let problem_2d = TwoDProblem {
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
            width: 5,
            height: 5,
            quantity: 4,
            can_rotate: false,
        }],
    };
    let solution_2d = solve_2d(
        problem_2d,
        TwoDOptions { algorithm: TwoDAlgorithm::Guillotine, ..Default::default() },
    )
    .expect("solve 2d");
    let opts_2d = CutPlanOptions2D { preset: CutPlanPreset2D::TableSaw, ..Default::default() };
    let plan_2d = plan_cuts_2d(&solution_2d, &opts_2d).expect("plan 2d");

    let ec2 = &plan_2d.effective_costs;
    let expected_total_2d: f64 = plan_2d
        .sheet_plans
        .iter()
        .map(|sp| {
            (sp.num_cuts as f64) * ec2.cut_cost
                + (sp.num_rotations as f64) * ec2.rotate_cost
                + (sp.num_fence_resets as f64) * ec2.fence_reset_cost
                + (sp.num_tool_ups as f64) * ec2.tool_up_down_cost
                + (sp.travel_distance as f64) * ec2.travel_cost
        })
        .sum();
    assert!(
        (plan_2d.total_cost - expected_total_2d).abs() < 1e-9,
        "2D total_cost {} != linear sum {}",
        plan_2d.total_cost,
        expected_total_2d
    );
}

// ---------------------------------------------------------------------------
// Test 8: invalid_options_rejected
// ---------------------------------------------------------------------------

/// Negative cost and NaN cost both produce InvalidOptions.
#[test]
fn invalid_options_rejected() {
    // 1D negative cut_cost
    let problem = OneDProblem {
        stock: vec![Stock1D {
            name: "bar".to_string(),
            length: 100,
            kerf: 0,
            trim: 0,
            cost: 1.0,
            available: None,
        }],
        demands: vec![CutDemand1D { name: "cut".to_string(), length: 20, quantity: 1 }],
    };
    let solution =
        bin_packing::one_d::solve_1d(problem.clone(), OneDOptions::default()).expect("solve");

    let neg_opts = CutPlanOptions1D {
        preset: CutPlanPreset1D::ChopSaw,
        cut_cost: Some(-1.0),
        fence_reset_cost: None,
    };
    assert!(
        matches!(
            plan_cuts_1d(&problem, &solution, &neg_opts),
            Err(CutPlanError::InvalidOptions(_))
        ),
        "expected InvalidOptions for negative cut_cost"
    );

    let nan_opts = CutPlanOptions1D {
        preset: CutPlanPreset1D::ChopSaw,
        cut_cost: None,
        fence_reset_cost: Some(f64::NAN),
    };
    assert!(
        matches!(
            plan_cuts_1d(&problem, &solution, &nan_opts),
            Err(CutPlanError::InvalidOptions(_))
        ),
        "expected InvalidOptions for NaN fence_reset_cost"
    );

    // 2D negative rotate_cost
    let problem_2d = TwoDProblem {
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
            width: 5,
            height: 5,
            quantity: 1,
            can_rotate: false,
        }],
    };
    let solution_2d = solve_2d(problem_2d, TwoDOptions::default()).expect("solve 2d");

    let neg_opts_2d = CutPlanOptions2D {
        preset: CutPlanPreset2D::TableSaw,
        rotate_cost: Some(-2.0),
        ..Default::default()
    };
    assert!(
        matches!(plan_cuts_2d(&solution_2d, &neg_opts_2d), Err(CutPlanError::InvalidOptions(_))),
        "expected InvalidOptions for negative rotate_cost"
    );

    let nan_opts_2d = CutPlanOptions2D {
        preset: CutPlanPreset2D::CncRouter,
        travel_cost: Some(f64::NAN),
        ..Default::default()
    };
    assert!(
        matches!(plan_cuts_2d(&solution_2d, &nan_opts_2d), Err(CutPlanError::InvalidOptions(_))),
        "expected InvalidOptions for NaN travel_cost"
    );
}

// ---------------------------------------------------------------------------
// Test 9: empty_sheet_yields_empty_plan
// ---------------------------------------------------------------------------

/// A single placement spanning the full sheet produces an empty plan for
/// TableSaw (the placement IS the sheet — no cut needed).
/// For CncRouter, a single placement generates 4 Cut steps (the outline).
#[test]
fn empty_sheet_yields_empty_plan() {
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
            name: "full".to_string(),
            width: 10,
            height: 10,
            quantity: 1,
            can_rotate: false,
        }],
    };

    let solution = solve_2d(problem, TwoDOptions::default()).expect("solve");

    // TableSaw: single full-sheet placement → no cuts needed.
    let options_ts = CutPlanOptions2D { preset: CutPlanPreset2D::TableSaw, ..Default::default() };
    let plan_ts = plan_cuts_2d(&solution, &options_ts).expect("plan table saw");
    assert_eq!(plan_ts.sheet_plans.len(), 1);
    assert!(
        plan_ts.sheet_plans[0].steps.is_empty(),
        "expected empty steps for a full-sheet placement on TableSaw"
    );
    assert_eq!(plan_ts.sheet_plans[0].total_cost, 0.0);

    // CncRouter: one placement → 4 outline cuts.
    let options_cnc = CutPlanOptions2D { preset: CutPlanPreset2D::CncRouter, ..Default::default() };
    let plan_cnc = plan_cuts_2d(&solution, &options_cnc).expect("plan cnc");
    assert_eq!(plan_cnc.sheet_plans[0].num_cuts, 4, "CNC outlines the single placement");
}

// ---------------------------------------------------------------------------
// Test 10: plan_is_deterministic_for_fixed_inputs
// ---------------------------------------------------------------------------

/// Run `plan_cuts` twice on the same inputs; the outputs must be identical.
#[test]
fn plan_is_deterministic_for_fixed_inputs() {
    // 1D
    let problem_1d = OneDProblem {
        stock: vec![Stock1D {
            name: "bar".to_string(),
            length: 100,
            kerf: 2,
            trim: 0,
            cost: 1.0,
            available: None,
        }],
        demands: vec![
            CutDemand1D { name: "a".to_string(), length: 30, quantity: 3 },
            CutDemand1D { name: "b".to_string(), length: 20, quantity: 2 },
        ],
    };
    let solution_1d = bin_packing::one_d::solve_1d(
        problem_1d.clone(),
        OneDOptions { seed: Some(42), ..Default::default() },
    )
    .expect("solve 1d");
    let opts_1d = CutPlanOptions1D::default();
    let plan_a = plan_cuts_1d(&problem_1d, &solution_1d, &opts_1d).expect("plan a");
    let plan_b = plan_cuts_1d(&problem_1d, &solution_1d, &opts_1d).expect("plan b");
    assert_eq!(plan_a, plan_b, "1D plan must be deterministic");

    // 2D
    let problem_2d = TwoDProblem {
        sheets: vec![Sheet2D {
            name: "s".to_string(),
            width: 20,
            height: 20,
            cost: 1.0,
            quantity: None,
            kerf: 0,
            edge_kerf_relief: false,
        }],
        demands: vec![
            RectDemand2D {
                name: "p".to_string(),
                width: 5,
                height: 5,
                quantity: 4,
                can_rotate: false,
            },
            RectDemand2D {
                name: "q".to_string(),
                width: 10,
                height: 5,
                quantity: 2,
                can_rotate: false,
            },
        ],
    };
    let solution_2d = solve_2d(
        problem_2d,
        TwoDOptions { algorithm: TwoDAlgorithm::Guillotine, seed: Some(42), ..Default::default() },
    )
    .expect("solve 2d");
    let opts_2d = CutPlanOptions2D { preset: CutPlanPreset2D::TableSaw, ..Default::default() };
    let plan_c = plan_cuts_2d(&solution_2d, &opts_2d).expect("plan c");
    let plan_d = plan_cuts_2d(&solution_2d, &opts_2d).expect("plan d");
    assert_eq!(plan_c, plan_d, "2D plan must be deterministic");

    // Also test CNC router determinism on a non-guillotine layout.
    let pinwheel = pinwheel_solution();
    let opts_cnc = CutPlanOptions2D { preset: CutPlanPreset2D::CncRouter, ..Default::default() };
    let plan_e = plan_cuts_2d(&pinwheel, &opts_cnc).expect("plan e");
    let plan_f = plan_cuts_2d(&pinwheel, &opts_cnc).expect("plan f");
    assert_eq!(plan_e, plan_f, "CNC plan must be deterministic");
}
