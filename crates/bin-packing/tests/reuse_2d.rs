#![cfg(feature = "two-d")]

use bin_packing::two_d::reuse::*;
use bin_packing::two_d::saw_plan::{SawJob2D, SawLayout2D};
use bin_packing::two_d::{
    Placement2D, RectDemand2D, Sheet2D, TwoDAlgorithm, TwoDOptions, TwoDProblem,
};

fn job(width: u32, height: u32, part_width: u32, part_height: u32, kerf: u32) -> SawJob2D {
    SawJob2D {
        problem: TwoDProblem {
            sheets: vec![Sheet2D {
                name: "stock".into(),
                width,
                height,
                cost: 4.0,
                quantity: Some(1),
                kerf,
                edge_kerf_relief: false,
            }],
            demands: vec![RectDemand2D {
                name: "current".into(),
                width: part_width,
                height: part_height,
                quantity: 1,
                can_rotate: false,
            }],
        },
        layouts: vec![SawLayout2D {
            width,
            height,
            placements: vec![Placement2D {
                name: "current".into(),
                x: 0,
                y: 0,
                width: part_width,
                height: part_height,
                rotated: false,
            }],
        }],
        stock_indices: vec![0],
        demand_indices: vec![vec![0]],
    }
}

fn forecast(width: u32, height: u32, quantity: usize, weight: f64) -> ReuseForecast2D {
    ReuseForecast2D {
        demands: vec![ForecastPart2D { width, height, quantity, can_rotate: false, weight }],
    }
}

#[test]
fn smaller_shaped_physical_offcut_beats_larger_unusable_strip() {
    let options = ReuseOptions2D::default();
    let shaped =
        evaluate_reuse(&job(10, 10, 2, 10, 0), &forecast(8, 8, 1, 3.0), &options).expect("shaped");
    let bigger = evaluate_reuse(&job(30, 4, 1, 4, 0), &forecast(8, 8, 1, 3.0), &options)
        .expect("larger strip");
    assert_eq!(shaped.weighted_fulfillment, 3.0);
    assert_eq!(bigger.weighted_fulfillment, 0.0);
    assert_eq!(shaped.fulfilled_counts, vec![1]);
}

#[test]
fn forecast_rotation_and_kerf_change_real_fulfillment() {
    let options = ReuseOptions2D::default();
    let current = job(12, 8, 2, 8, 1); // Physical remnant is 9x8.
    let mut f = forecast(8, 9, 1, 1.0);
    assert_eq!(evaluate_reuse(&current, &f, &options).expect("fixed").fulfilled_counts, vec![0]);
    f.demands[0].can_rotate = true;
    assert_eq!(evaluate_reuse(&current, &f, &options).expect("rotated").fulfilled_counts, vec![1]);
    let f = forecast(4, 8, 2, 1.0);
    assert_eq!(
        evaluate_reuse(&current, &f, &options).expect("kerf fits").fulfilled_counts,
        vec![2]
    );
    let current = job(12, 8, 2, 8, 2); // 8x8, two 4-wide parts need another kerf.
    assert_eq!(
        evaluate_reuse(&current, &f, &options).expect("kerf prevents reuse").fulfilled_counts,
        vec![1]
    );
}

#[test]
fn zero_invalid_and_bounded_forecasts_are_explicit() {
    let current = job(12, 8, 2, 8, 0);
    let options = ReuseOptions2D::default();
    assert_eq!(
        evaluate_reuse(&current, &forecast(4, 4, 3, 0.0), &options)
            .expect("zero weight")
            .weighted_fulfillment,
        0.0
    );
    assert_eq!(
        evaluate_reuse(&current, &ReuseForecast2D::default(), &options)
            .expect("empty forecast")
            .weighted_fulfillment,
        0.0
    );
    for weight in [-1.0, f64::NAN, f64::INFINITY] {
        assert!(matches!(
            evaluate_reuse(&current, &forecast(4, 4, 1, weight), &options),
            Err(ReuseError2D::InvalidForecast { .. })
        ));
    }
    assert!(matches!(
        evaluate_reuse(&current, &forecast(0, 4, 1, 1.0), &options),
        Err(ReuseError2D::InvalidForecast { .. })
    ));
    let bounded = ReuseOptions2D { max_future_parts: 1, ..options };
    let evaluation = evaluate_reuse(&current, &forecast(2, 2, 20, 1.0), &bounded).expect("bounded");
    assert_eq!(evaluation.placements.len(), 1);
    assert!(evaluation.evaluation_truncated);
}

#[test]
fn reuse_allocations_and_physical_offcuts_are_disjoint() {
    let current = job(20, 20, 4, 4, 1);
    let evaluated = evaluate_reuse(&current, &forecast(3, 3, 20, 1.0), &Default::default())
        .expect("physical reuse");
    for (i, a) in evaluated.offcuts.iter().enumerate() {
        for b in &evaluated.offcuts[i + 1..] {
            let a = a.workpiece.bounds;
            let b = b.workpiece.bounds;
            assert!(
                a.x + a.width <= b.x
                    || b.x + b.width <= a.x
                    || a.y + a.height <= b.y
                    || b.y + b.height <= a.y
            );
        }
    }
    for (i, a) in evaluated.placements.iter().enumerate() {
        let remnant = evaluated.offcuts[a.offcut_index].workpiece.bounds;
        assert!(a.bounds.x >= remnant.x && a.bounds.y >= remnant.y);
        assert!(a.bounds.x + a.bounds.width <= remnant.x + remnant.width);
        assert!(a.bounds.y + a.bounds.height <= remnant.y + remnant.height);
        for b in &evaluated.placements[i + 1..] {
            let (a, b) = (a.bounds, b.bounds);
            assert!(
                a.x + a.width <= b.x
                    || b.x + b.width <= a.x
                    || a.y + a.height <= b.y
                    || b.y + b.height <= a.y
            );
        }
    }
}

#[test]
fn material_budget_rejection_and_source_identity_survive_duplicate_names() {
    let mut current = job(12, 8, 2, 8, 1);
    let strict = ReuseOptions2D { max_material_cost: Some(3.0), ..Default::default() };
    assert!(matches!(
        evaluate_reuse(&current, &forecast(4, 4, 1, 1.0), &strict),
        Err(ReuseError2D::MaterialBudget { .. })
    ));
    current.problem.demands[0].width = 12;
    current.problem.sheets.push(current.problem.sheets[0].clone());
    current.problem.demands.push(current.problem.demands[0].clone());
    let result = solve_2d_with_reuse(
        current.problem.clone(),
        TwoDOptions { algorithm: TwoDAlgorithm::Guillotine, seed: Some(7), ..Default::default() },
        &forecast(4, 4, 2, 1.0),
        &Default::default(),
    )
    .expect("indexed solve");
    assert!(result.solution.sheet_count <= result.baseline_sheet_count);
    assert!(result.solution.total_cost <= result.baseline_material_cost);
    assert_eq!(
        result
            .job
            .demand_indices
            .iter()
            .flatten()
            .copied()
            .collect::<std::collections::BTreeSet<_>>(),
        [0, 1].into()
    );
    assert_eq!(
        result.job.stock_indices.iter().copied().collect::<std::collections::BTreeSet<_>>(),
        [0, 1].into()
    );
    assert_eq!(result.job.problem, current.problem);
    assert!(result.candidate_solves <= 8);
}

#[test]
fn forecast_reranking_improves_fulfillment_within_baseline_material_ceilings() {
    let mut current = job(24, 20, 6, 4, 0);
    current.problem.sheets[0].quantity = Some(3);
    current.problem.demands = [(6, 4), (14, 10), (10, 6), (6, 12)]
        .into_iter()
        .enumerate()
        .map(|(i, (width, height))| RectDemand2D {
            name: format!("part-{i}"),
            width,
            height,
            quantity: 1,
            can_rotate: true,
        })
        .collect();
    let future = forecast(8, 7, 4, 1.0);
    let options = TwoDOptions {
        algorithm: TwoDAlgorithm::MaxRectsBottomLeft,
        seed: Some(7),
        multistart_runs: 1,
        ..Default::default()
    };
    let reuse = ReuseOptions2D { max_candidate_solves: 8, ..Default::default() };
    let started = std::time::Instant::now();
    let result = solve_2d_with_reuse(current.problem.clone(), options.clone(), &future, &reuse)
        .expect("reranked physical solve");
    eprintln!(
        "reuse fixture elapsed={:?} baseline={} selected={} sheets={} cost={} solves={} material_rejections={} physical_rejections={} algorithm={}",
        started.elapsed(),
        result.baseline_evaluation.weighted_fulfillment,
        result.evaluation.weighted_fulfillment,
        result.solution.sheet_count,
        result.solution.total_cost,
        result.candidate_solves,
        result.material_rejections,
        result.physical_rejections,
        result.solution.algorithm
    );
    assert_eq!(result.baseline_evaluation.fulfilled_counts, vec![0]);
    assert!(result.evaluation.fulfilled_counts[0] >= 1);
    assert_eq!(result.solution.sheet_count, 1);
    assert_eq!(result.solution.total_cost, 4.0);
    assert!(result.solution.unplaced.is_empty());
    assert_eq!(
        result,
        solve_2d_with_reuse(current.problem, options, &future, &reuse)
            .expect("deterministic solve")
    );
}

#[test]
fn edge_relief_zero_quantity_and_explicit_sheet_ceiling_are_respected() {
    let mut current = job(12, 8, 2, 8, 2);
    let future = forecast(7, 8, 1, 2.0);
    assert_eq!(
        evaluate_reuse(&current, &future, &Default::default()).expect("no relief").fulfilled_counts,
        vec![0]
    );
    current.problem.sheets[0].edge_kerf_relief = true;
    assert_eq!(
        evaluate_reuse(&current, &future, &Default::default())
            .expect("blade can overhang")
            .fulfilled_counts,
        vec![1]
    );
    assert_eq!(
        evaluate_reuse(&current, &forecast(7, 8, 0, 2.0), &Default::default())
            .expect("zero quantity")
            .fulfilled_counts,
        vec![0]
    );
    assert!(matches!(
        evaluate_reuse(
            &current,
            &future,
            &ReuseOptions2D { max_sheet_count: Some(0), ..Default::default() }
        ),
        Err(ReuseError2D::MaterialBudget { .. })
    ));
}

#[test]
fn input_rotation_geometry_inventory_and_serde_are_validated() {
    let mut current = job(12, 8, 2, 8, 1);
    let f = forecast(4, 4, 1, 1.0);
    current.layouts[0].placements[0].rotated = true;
    assert!(matches!(
        evaluate_reuse(&current, &f, &Default::default()),
        Err(ReuseError2D::Physical(_))
    ));
    current.layouts[0].placements[0].rotated = false;
    current.layouts[0].placements[0].x = 11;
    assert!(matches!(
        evaluate_reuse(&current, &f, &Default::default()),
        Err(ReuseError2D::Physical(_))
    ));
    current.layouts[0].placements[0].x = 0;
    current.problem.sheets[0].quantity = Some(0);
    assert!(matches!(
        evaluate_reuse(&current, &f, &Default::default()),
        Err(ReuseError2D::Physical(_))
    ));
    current.problem.sheets[0].quantity = Some(1);
    let value = evaluate_reuse(&current, &f, &Default::default()).expect("valid");
    assert_eq!(
        value,
        serde_json::from_value::<ReuseEvaluation2D>(
            serde_json::to_value(&value).expect("wire output")
        )
        .expect("wire input")
    );
    assert_eq!(
        f,
        serde_json::from_value::<ReuseForecast2D>(
            serde_json::to_value(&f).expect("forecast output")
        )
        .expect("forecast input")
    );
}

#[test]
fn infeasible_solver_error_retains_type_and_original_demand_name() {
    let mut problem = job(12, 8, 13, 8, 0).problem;
    problem.demands[0].name = "door".into();
    let error = solve_2d_with_reuse(
        problem,
        TwoDOptions::default(),
        &ReuseForecast2D::default(),
        &ReuseOptions2D::default(),
    )
    .expect_err("oversized door must fail");
    let source = std::error::Error::source(&error)
        .and_then(|source| source.downcast_ref::<bin_packing::BinPackingError>());
    assert!(
        matches!(
            source,
            Some(bin_packing::BinPackingError::Infeasible2D { item, width: 13, height: 8 })
                if item == "door"
        ),
        "expected original typed infeasibility, got {error:?}"
    );
}

#[test]
fn malformed_sources_retain_validation_error_and_original_names() {
    for malformed_stock in [false, true] {
        let mut problem = job(12, 8, 2, 8, 0).problem;
        let original_name = if malformed_stock {
            problem.sheets[0].name = "oak panel".into();
            problem.sheets[0].width = 0;
            "oak panel"
        } else {
            problem.demands[0].name = "cabinet door".into();
            problem.demands[0].width = 0;
            "cabinet door"
        };
        let error = solve_2d_with_reuse(
            problem,
            TwoDOptions::default(),
            &ReuseForecast2D::default(),
            &ReuseOptions2D::default(),
        )
        .expect_err("zero dimension must fail");
        let source = std::error::Error::source(&error)
            .and_then(|source| source.downcast_ref::<bin_packing::BinPackingError>());
        assert!(
            matches!(
                source,
                Some(bin_packing::BinPackingError::InvalidInput(message))
                    if message.contains(original_name)
            ),
            "expected original typed validation error, got {error:?}"
        );
    }
}

#[test]
fn restored_source_order_preserves_ordinary_baseline_cost_bits() {
    let mut problem = job(10, 10, 10, 10, 0).problem;
    problem.demands[0].quantity = 3;
    problem.sheets = [("z", 0.1), ("y", 0.2), ("x", 0.3)]
        .into_iter()
        .map(|(name, cost)| Sheet2D { name: name.into(), cost, ..problem.sheets[0].clone() })
        .collect();
    let options = TwoDOptions { algorithm: TwoDAlgorithm::Guillotine, ..Default::default() };
    let ordinary =
        bin_packing::two_d::solve_2d(problem.clone(), options.clone()).expect("ordinary");
    let reused = solve_2d_with_reuse(
        problem,
        options,
        &ReuseForecast2D::default(),
        &ReuseOptions2D { max_candidate_solves: 1, ..Default::default() },
    )
    .expect("reuse baseline");
    assert_eq!(reused.solution.total_cost.to_bits(), ordinary.total_cost.to_bits());
    assert_eq!(reused.baseline_material_cost.to_bits(), ordinary.total_cost.to_bits());
    assert_eq!(reused.solution.layouts, ordinary.layouts);
    assert_eq!(reused.job.stock_indices, vec![2, 1, 0]);
    assert_eq!(reused.job.demand_indices, vec![vec![0]; 3]);
}

#[test]
fn bounded_reuse_rejects_aggregate_area_overflow_before_solving() {
    let mut problem = job(1_000_000_000, 1_000_000_000, 600_000_000, 600_000_000, 0).problem;
    problem.sheets[0].quantity = None;
    problem.demands[0].quantity = 96;
    assert!(matches!(
        solve_2d_with_reuse(
            problem,
            TwoDOptions { algorithm: TwoDAlgorithm::Guillotine, ..Default::default() },
            &ReuseForecast2D::default(),
            &ReuseOptions2D { max_candidate_solves: 1, ..Default::default() },
        ),
        Err(ReuseError2D::InvalidOption { field: "aggregate sheet area exceeds u64" })
    ));
}

#[test]
fn persisted_reuse_solution_keeps_replayable_forecast_provenance() {
    let current = job(12, 8, 2, 8, 1);
    let future = forecast(4, 8, 2, 1.0);
    let limits = ReuseOptions2D { max_candidate_solves: 1, ..Default::default() };
    let result = solve_2d_with_reuse(
        current.problem,
        TwoDOptions {
            algorithm: TwoDAlgorithm::Guillotine,
            multistart_runs: 1,
            seed: Some(7),
            ..Default::default()
        },
        &future,
        &limits,
    )
    .expect("current solve and forecast");
    let wire = serde_json::to_string(&result).expect("serialize indexed reuse result");
    let restored: ReuseSolution2D =
        serde_json::from_str(&wire).expect("restore indexed reuse result");
    assert_eq!(restored, result);
    let reevaluated = evaluate_reuse(&restored.job, &future, &limits)
        .expect("persisted job independently validates and decomposes");
    assert_eq!(reevaluated.fulfilled_counts, vec![2]);
    assert_eq!(reevaluated, restored.evaluation);
    assert_eq!(restored.job.stock_indices, vec![0]);
    assert_eq!(restored.job.demand_indices, vec![vec![0]]);
}
