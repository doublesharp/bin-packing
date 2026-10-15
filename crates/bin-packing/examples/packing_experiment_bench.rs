//! Targeted paired solver measurements. Run through scripts/run-packing-experiments.py.

use std::{error::Error, time::Instant};

use bin_packing::two_d::saw_plan::{
    SawJob2D, SawLayout2D, SawPlanOptions2D, SawSearchLimits2D, plan_saw_cuts,
};
use bin_packing::{
    one_d::{OneDOptions, OneDProblem, solve_1d},
    three_d::{
        Bin3D, BoxDemand3D, RotationMask3D, ThreeDAlgorithm, ThreeDOptions, ThreeDProblem, solve_3d,
    },
    two_d::{
        Placement2D, RectDemand2D, Sheet2D, TwoDAlgorithm, TwoDOptions, TwoDProblem, solve_2d,
    },
};
use serde::Serialize;
use serde_json::{Value, json};

fn measure<T: Serialize, E: Error + 'static>(
    case: &str,
    variant: &str,
    input: Value,
    samples: usize,
    mut solve: impl FnMut() -> Result<T, E>,
) -> Result<Value, Box<dyn Error>> {
    let mut result = solve()?;
    let mut times = Vec::with_capacity(samples);
    for _ in 0..samples {
        let started = Instant::now();
        result = solve()?;
        times.push(started.elapsed().as_nanos());
        std::hint::black_box(&result);
    }
    times.sort_unstable();
    let median = times.get(samples / 2).copied().ok_or("empty timing sample")?;
    let p95 = times.get(samples.saturating_mul(95) / 100).copied().ok_or("missing percentile")?;
    Ok(json!({"case": case, "variant": variant, "input": input, "samples": samples,
        "median_us": median as f64 / 1000.0, "p95_us": p95 as f64 / 1000.0,
        "output": result}))
}

fn sheet(width: u32, height: u32) -> Sheet2D {
    Sheet2D {
        name: "sheet".into(),
        width,
        height,
        cost: 1.0,
        quantity: None,
        kerf: 0,
        edge_kerf_relief: false,
    }
}

fn cube_problem(quantity: usize, mixed: bool) -> ThreeDProblem {
    let mut demands = vec![BoxDemand3D {
        name: "cube".into(),
        width: 40,
        height: 40,
        depth: 40,
        quantity,
        allowed_rotations: RotationMask3D::XYZ,
    }];
    if mixed {
        demands.push(BoxDemand3D {
            name: "small".into(),
            width: 10,
            height: 20,
            depth: 30,
            quantity: 3,
            allowed_rotations: RotationMask3D::XYZ,
        });
    }
    ThreeDProblem {
        bins: vec![Bin3D {
            name: "bin".into(),
            width: 100,
            height: 100,
            depth: 100,
            cost: 1.0,
            quantity: None,
        }],
        demands,
    }
}

fn saw_job() -> SawJob2D {
    SawJob2D {
        problem: TwoDProblem {
            sheets: vec![Sheet2D {
                name: "stock".into(),
                width: 1209,
                height: 600,
                cost: 1.0,
                quantity: None,
                kerf: 3,
                edge_kerf_relief: false,
            }],
            demands: vec![RectDemand2D {
                name: "panel".into(),
                width: 300,
                height: 600,
                quantity: 12,
                can_rotate: false,
            }],
        },
        layouts: (0..3)
            .map(|_| SawLayout2D {
                width: 1209,
                height: 600,
                placements: (0..4)
                    .map(|i| Placement2D {
                        name: "panel".into(),
                        x: i * 303,
                        y: 0,
                        width: 300,
                        height: 600,
                        rotated: false,
                    })
                    .collect(),
            })
            .collect(),
        stock_indices: vec![0; 3],
        demand_indices: vec![vec![0; 4]; 3],
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut rows = Vec::new();
    for (name, quantity, mixed) in
        [("bounds-nine", 9, false), ("bounds-seventeen", 17, false), ("bounds-mixed", 9, true)]
    {
        let problem = cube_problem(quantity, mixed);
        let options = ThreeDOptions {
            algorithm: ThreeDAlgorithm::BranchAndBound,
            seed: Some(7),
            ..ThreeDOptions::default()
        };
        rows.push(measure(
            name,
            "ordinary",
            json!({"problem": &problem, "options": &options}),
            21,
            || solve_3d(problem.clone(), options.clone()),
        )?);
    }
    let job = saw_job();
    for batching in [false, true] {
        let options = SawPlanOptions2D {
            batch_across_sheets: batching,
            search: SawSearchLimits2D {
                max_states: 100_000,
                beam_width: 256,
                ..SawSearchLimits2D::default()
            },
            ..SawPlanOptions2D::default()
        };
        rows.push(measure(
            if batching { "saw-batched" } else { "saw-sequential" },
            "ordinary",
            json!({"job": &job, "options": &options}),
            11,
            || plan_saw_cuts(&job, &options),
        )?);
    }
    let mut single_job = job.clone();
    single_job.layouts.truncate(1);
    single_job.stock_indices.truncate(1);
    single_job.demand_indices.truncate(1);
    single_job.problem.demands[0].quantity = 4;
    let single_options = SawPlanOptions2D {
        search: SawSearchLimits2D {
            max_states: 100_000,
            beam_width: 256,
            ..SawSearchLimits2D::default()
        },
        ..SawPlanOptions2D::default()
    };
    rows.push(measure(
        "saw-single",
        "ordinary",
        json!({"job": &single_job, "options": &single_options}),
        31,
        || plan_saw_cuts(&single_job, &single_options),
    )?);
    let options = TwoDOptions {
        multistart_runs: 2,
        auto_rotation_search_max_types: 2,
        beam_width: 4,
        seed: Some(17),
        ..TwoDOptions::default()
    };
    let pattern_count: TwoDProblem =
        serde_json::from_str(include_str!("../tests/fixtures/pattern-count.json"))?;
    let pattern_offcuts: TwoDProblem =
        serde_json::from_str(include_str!("../tests/fixtures/pattern-offcuts.json"))?;
    for (name, problem) in
        [("pattern-count", &pattern_count), ("pattern-offcuts", &pattern_offcuts)]
    {
        rows.push(measure(
            name,
            "ordinary",
            json!({"problem": problem, "options": &options}),
            21,
            || solve_2d(problem.clone(), options.clone()),
        )?);
    }
    let demand = TwoDProblem {
        sheets: vec![sheet(10, 10)],
        demands: vec![
            RectDemand2D {
                name: "current".into(),
                width: 8,
                height: 6,
                quantity: 1,
                can_rotate: true,
            },
            RectDemand2D {
                name: "remaining".into(),
                width: 4,
                height: 10,
                quantity: 1,
                can_rotate: false,
            },
        ],
    };
    let demand_options = TwoDOptions {
        algorithm: TwoDAlgorithm::MaxRects,
        seed: Some(7),
        multistart_runs: 2,
        ..TwoDOptions::default()
    };
    rows.push(measure(
        "demand-space",
        "ordinary",
        json!({"problem": &demand, "options": &demand_options}),
        31,
        || solve_2d(demand.clone(), demand_options.clone()),
    )?);
    let mixed = TwoDProblem {
        sheets: vec![sheet(5, 5)],
        demands: vec![RectDemand2D {
            name: "tile".into(),
            width: 3,
            height: 2,
            quantity: 4,
            can_rotate: true,
        }],
    };
    let mixed_options = TwoDOptions {
        algorithm: TwoDAlgorithm::RotationSearch,
        seed: Some(7),
        multistart_runs: 8,
        ..TwoDOptions::default()
    };
    rows.push(measure(
        "mixed-rotations",
        "ordinary",
        json!({"problem": &mixed, "options": &mixed_options}),
        31,
        || solve_2d(mixed.clone(), mixed_options.clone()),
    )?);
    let full_auto = TwoDOptions {
        algorithm: TwoDAlgorithm::Auto,
        multistart_runs: 12,
        auto_rotation_search_max_types: 16,
        seed: Some(7),
        ..TwoDOptions::default()
    };
    for (name, problem) in [("demand-space-auto", &demand), ("mixed-rotations-auto", &mixed)] {
        rows.push(measure(
            name,
            "ordinary",
            json!({"problem": problem, "options": &full_auto}),
            21,
            || solve_2d(problem.clone(), full_auto.clone()),
        )?);
    }
    let repair = ThreeDProblem {
        bins: vec![Bin3D {
            name: "bar".into(),
            width: 10,
            height: 1,
            depth: 1,
            cost: 1.0,
            quantity: None,
        }],
        demands: [6, 5, 3, 2, 2, 2]
            .into_iter()
            .map(|width| BoxDemand3D {
                name: "same".into(),
                width,
                height: 1,
                depth: 1,
                quantity: 1,
                allowed_rotations: RotationMask3D::XYZ,
            })
            .collect(),
    };
    let repair_options = ThreeDOptions {
        algorithm: ThreeDAlgorithm::LocalSearch,
        seed: Some(7),
        multistart_runs: 2,
        improvement_rounds: 4,
        ..ThreeDOptions::default()
    };
    rows.push(measure(
        "repair-strip",
        "ordinary",
        json!({"problem": &repair, "options": &repair_options}),
        31,
        || solve_3d(repair.clone(), repair_options.clone()),
    )?);
    let arc_fixture: Value = serde_json::from_str(include_str!("../tests/fixtures/arc-auto.json"))?;
    let arc: OneDProblem = serde_json::from_value(arc_fixture["problem"].clone())?;
    let arc_options: OneDOptions = serde_json::from_value(arc_fixture["options"].clone())?;
    rows.push(measure(
        "arc-mixed",
        "ordinary",
        json!({"problem": &arc, "options": &arc_options}),
        21,
        || solve_1d(arc.clone(), arc_options.clone()),
    )?);
    let repeated: OneDProblem = serde_json::from_value(json!({
        "stock": [{"name": "stock", "length": 20, "kerf": 0, "trim": 0,
                   "cost": 1.0, "available": null}],
        "demands": (0..16).map(|i| json!({"name": format!("row-{i}"), "length": 1,
                                        "quantity": 20})).collect::<Vec<_>>()
    }))?;
    let repeated_options = OneDOptions {
        seed: Some(7),
        multistart_runs: 1,
        improvement_rounds: 0,
        exact_pattern_limit: 10_000,
        column_generation_rounds: 1,
        ..OneDOptions::default()
    };
    rows.push(measure(
        "arc-repeated",
        "ordinary",
        json!({"problem": &repeated, "options": &repeated_options}),
        21,
        || solve_1d(repeated.clone(), repeated_options.clone()),
    )?);
    let reuse = TwoDProblem {
        sheets: vec![Sheet2D {
            name: "sheet".into(),
            width: 24,
            height: 20,
            cost: 4.0,
            quantity: Some(3),
            kerf: 0,
            edge_kerf_relief: false,
        }],
        demands: [(6, 4), (14, 10), (10, 6), (6, 12)]
            .into_iter()
            .enumerate()
            .map(|(i, (width, height))| RectDemand2D {
                name: i.to_string(),
                width,
                height,
                quantity: 1,
                can_rotate: true,
            })
            .collect(),
    };
    let reuse_options = TwoDOptions {
        algorithm: TwoDAlgorithm::MaxRectsBottomLeft,
        seed: Some(7),
        multistart_runs: 1,
        ..TwoDOptions::default()
    };
    rows.push(measure(
        "forecast-reuse",
        "ordinary",
        json!({"problem": &reuse, "options": &reuse_options}),
        31,
        || solve_2d(reuse.clone(), reuse_options.clone()),
    )?);

    // EXPERIMENT_ONLY_START
    for (name, problem) in
        [("pattern-count", &pattern_count), ("pattern-offcuts", &pattern_offcuts)]
    {
        rows.push(measure(
            name,
            "recombined",
            json!({"problem": problem, "options": &options}),
            21,
            || bin_packing::two_d::solve_2d_recombined(problem.clone(), options.clone()),
        )?);
    }
    rows.push(measure(
        "demand-space",
        "demand-aware",
        json!({"problem": &demand, "options": &demand_options}),
        31,
        || bin_packing::two_d::solve_2d_demand_aware(demand.clone(), demand_options.clone()),
    )?);
    rows.push(measure(
        "mixed-rotations",
        "mixed-count",
        json!({"problem": &mixed, "options": &mixed_options}),
        31,
        || bin_packing::two_d::solve_2d_mixed_rotations(mixed.clone(), mixed_options.clone()),
    )?);
    rows.push(measure(
        "repair-strip",
        "joint-repair",
        json!({"problem": &repair, "options": &repair_options}),
        31,
        || bin_packing::three_d::solve_3d_repaired(repair.clone(), repair_options.clone()),
    )?);
    rows.push(measure(
        "arc-mixed",
        "arc-flow",
        json!({"problem": &arc, "options": &arc_options}),
        21,
        || bin_packing::one_d::solve_1d_arc_flow(arc.clone(), arc_options.clone()),
    )?);
    rows.push(measure(
        "arc-repeated",
        "arc-flow",
        json!({"problem": &repeated, "options": &repeated_options}),
        21,
        || bin_packing::one_d::solve_1d_arc_flow(repeated.clone(), repeated_options.clone()),
    )?);
    let forecast = bin_packing::two_d::reuse::ReuseForecast2D {
        demands: vec![bin_packing::two_d::reuse::ForecastPart2D {
            width: 8,
            height: 7,
            quantity: 4,
            can_rotate: false,
            weight: 1.0,
        }],
    };
    let reuse_search =
        bin_packing::two_d::reuse::ReuseOptions2D { max_candidate_solves: 8, ..Default::default() };
    rows.push(measure("forecast-reuse", "forecast", json!({"problem": &reuse, "options": &reuse_options, "forecast": &forecast, "reuse_options": &reuse_search}), 21,
        || bin_packing::two_d::reuse::solve_2d_with_reuse(reuse.clone(), reuse_options.clone(), &forecast, &reuse_search))?);
    // EXPERIMENT_ONLY_END

    println!("{}", serde_json::to_string(&rows)?);
    Ok(())
}
