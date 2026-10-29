//! Independent finite knapsack oracles shared by three physical models.

#![cfg(all(feature = "one-d", feature = "two-d", feature = "three-d"))]

use bin_packing::BinPackingError;
use bin_packing::one_d::optimization::{OptimizationProblem1D, solve_1d_optimized};
use bin_packing::one_d::{CutDemand1D, OneDProblem, Stock1D};
use bin_packing::optimization::{OptimizationOptions, PackingObjective};
use bin_packing::three_d::constrained::ConstrainedProblem3D;
use bin_packing::three_d::optimization::{OptimizationProblem3D, solve_3d_optimized};
use bin_packing::three_d::{Bin3D, BoxDemand3D, RotationMask3D, ThreeDProblem};
use bin_packing::two_d::constrained::ConstrainedProblem2D;
use bin_packing::two_d::optimization::{OptimizationProblem2D, solve_2d_optimized};
use bin_packing::two_d::{RectDemand2D, Sheet2D, TwoDProblem};

fn linear_problem() -> OptimizationProblem1D {
    OptimizationProblem1D {
        packing: OneDProblem {
            stock: vec![Stock1D {
                name: "stock".into(),
                length: 10,
                kerf: 0,
                trim: 0,
                cost: 1.0,
                available: Some(1),
            }],
            demands: vec![CutDemand1D { name: "source".into(), length: 5, quantity: 2 }],
        },
        profits: Vec::new(),
        minimum_quantities: Vec::new(),
    }
}

fn rectangular_problem(one: &OptimizationProblem1D) -> OptimizationProblem2D {
    OptimizationProblem2D {
        packing: ConstrainedProblem2D {
            problem: TwoDProblem {
                sheets: one
                    .packing
                    .stock
                    .iter()
                    .map(|stock| Sheet2D {
                        name: stock.name.clone(),
                        width: stock.length,
                        height: 2,
                        kerf: stock.kerf,
                        edge_kerf_relief: false,
                        cost: stock.cost,
                        quantity: stock.available,
                    })
                    .collect(),
                demands: one
                    .packing
                    .demands
                    .iter()
                    .map(|demand| RectDemand2D {
                        name: demand.name.clone(),
                        width: demand.length,
                        height: 2,
                        quantity: demand.quantity,
                        can_rotate: false,
                    })
                    .collect(),
            },
            constraints: Vec::new(),
        },
        profits: one.profits.clone(),
        minimum_quantities: one.minimum_quantities.clone(),
    }
}

fn spatial_problem(one: &OptimizationProblem1D) -> OptimizationProblem3D {
    OptimizationProblem3D {
        packing: ConstrainedProblem3D {
            problem: ThreeDProblem {
                bins: one
                    .packing
                    .stock
                    .iter()
                    .map(|stock| Bin3D {
                        name: stock.name.clone(),
                        width: stock.length,
                        height: 2,
                        depth: 3,
                        cost: stock.cost,
                        quantity: stock.available,
                    })
                    .collect(),
                demands: one
                    .packing
                    .demands
                    .iter()
                    .map(|demand| BoxDemand3D {
                        name: demand.name.clone(),
                        width: demand.length,
                        height: 2,
                        depth: 3,
                        quantity: demand.quantity,
                        allowed_rotations: RotationMask3D::XYZ,
                    })
                    .collect(),
            },
            item_weights: Vec::new(),
            bin_max_weights: Vec::new(),
        },
        profits: one.profits.clone(),
        minimum_quantities: one.minimum_quantities.clone(),
    }
}

fn assert_constraint<T>(result: Result<T, BinPackingError>, expected: &str) {
    assert!(
        matches!(result, Err(BinPackingError::InvalidConstraint { field, .. }) if field == expected),
        "expected the {expected} constraint error"
    );
}

fn material_choice_problem() -> OptimizationProblem1D {
    let mut one = linear_problem();
    one.packing.stock = vec![
        Stock1D {
            name: "short".into(),
            length: 5,
            kerf: 0,
            trim: 0,
            cost: 0.4,
            available: Some(2),
        },
        Stock1D {
            name: "long".into(),
            length: 10,
            kerf: 0,
            trim: 0,
            cost: 1.0,
            available: Some(1),
        },
    ];
    one
}

#[test]
fn profit_search_matches_independent_exhaustive_selection_oracles() {
    // Full-height strips and full-cross-section boxes reduce physical packing
    // to scalar capacity. Enumerate every quantity vector independently of the
    // optimization engine; arbitrary values, duplicates and minima still apply.
    for seed in 0..48u32 {
        let lengths = [1 + seed % 6, 1 + (seed * 3 + 1) % 6, 1 + (seed * 5 + 2) % 6];
        let values = [1 + (seed * 7) % 19, 1 + (seed * 11 + 3) % 19, 1 + (seed * 13 + 5) % 19];
        let minima = vec![usize::from(seed % 3 == 0), 0, 0];
        let mut optimum = 0u32;
        for a in minima[0]..=2 {
            for b in 0..=2 {
                for c in 0..=2 {
                    let counts = [a as u32, b, c];
                    let load: u32 = counts.iter().zip(lengths).map(|(&q, l)| q * l).sum();
                    if load <= 7 {
                        optimum = optimum.max(counts.iter().zip(values).map(|(&q, p)| q * p).sum());
                    }
                }
            }
        }
        let names = ["same", "same", "third"];
        let options = OptimizationOptions {
            objective: PackingObjective::Knapsack,
            max_evaluations: 512,
            beam_width: 32,
            packing_max_states: 128,
            ..Default::default()
        };
        let profits: Vec<_> = values.iter().map(|&p| f64::from(p)).collect();
        let one = solve_1d_optimized(
            OptimizationProblem1D {
                packing: OneDProblem {
                    stock: vec![Stock1D {
                        name: "bar".into(),
                        length: 7,
                        kerf: 0,
                        trim: 0,
                        cost: 1.0,
                        available: Some(1),
                    }],
                    demands: lengths
                        .iter()
                        .enumerate()
                        .map(|(i, &length)| CutDemand1D {
                            name: names[i].into(),
                            length,
                            quantity: 2,
                        })
                        .collect(),
                },
                profits: profits.clone(),
                minimum_quantities: minima.clone(),
            },
            options.clone(),
        )
        .expect("linear oracle case");
        let two = solve_2d_optimized(
            OptimizationProblem2D {
                packing: ConstrainedProblem2D {
                    problem: TwoDProblem {
                        sheets: vec![Sheet2D {
                            name: "sheet".into(),
                            width: 7,
                            height: 2,
                            cost: 1.0,
                            quantity: Some(1),
                            kerf: 0,
                            edge_kerf_relief: false,
                        }],
                        demands: lengths
                            .iter()
                            .enumerate()
                            .map(|(i, &width)| RectDemand2D {
                                name: names[i].into(),
                                width,
                                height: 2,
                                quantity: 2,
                                can_rotate: false,
                            })
                            .collect(),
                    },
                    constraints: Vec::new(),
                },
                profits: profits.clone(),
                minimum_quantities: minima.clone(),
            },
            options.clone(),
        )
        .expect("rectangular oracle case");
        let three = solve_3d_optimized(
            OptimizationProblem3D {
                packing: ConstrainedProblem3D {
                    problem: ThreeDProblem {
                        bins: vec![Bin3D {
                            name: "box".into(),
                            width: 7,
                            height: 2,
                            depth: 3,
                            cost: 1.0,
                            quantity: Some(1),
                        }],
                        demands: lengths
                            .iter()
                            .enumerate()
                            .map(|(i, &width)| BoxDemand3D {
                                name: names[i].into(),
                                width,
                                height: 2,
                                depth: 3,
                                quantity: 2,
                                allowed_rotations: RotationMask3D::XYZ,
                            })
                            .collect(),
                    },
                    item_weights: Vec::new(),
                    bin_max_weights: Vec::new(),
                },
                profits,
                minimum_quantities: minima,
            },
            options,
        )
        .expect("spatial oracle case");
        for report in [one.report, two.report, three.report] {
            assert_eq!(report.total_profit, f64::from(optimum), "oracle seed {seed}");
            assert!(report.minimums_satisfied);
            assert_eq!(report.selected_quantities.len(), 3);
            assert!(report.evaluations <= 512);
        }
    }
}

#[test]
fn open_area_search_checks_both_extent_orders_and_retains_a_valid_incumbent() {
    let problem = OptimizationProblem2D {
        packing: ConstrainedProblem2D {
            problem: TwoDProblem {
                sheets: vec![Sheet2D {
                    name: "upper bound".into(),
                    width: 100,
                    height: 100,
                    cost: 1.0,
                    quantity: None,
                    kerf: 0,
                    edge_kerf_relief: false,
                }],
                demands: vec![RectDemand2D {
                    name: "square".into(),
                    width: 5,
                    height: 5,
                    quantity: 2,
                    can_rotate: false,
                }],
            },
            constraints: Vec::new(),
        },
        profits: Vec::new(),
        minimum_quantities: Vec::new(),
    };
    let options =
        OptimizationOptions { objective: PackingObjective::OpenDimensionXy, ..Default::default() };
    let solution = solve_2d_optimized(problem.clone(), options.clone()).expect("open area");
    assert_eq!(solution.report.primary_value, 50.0);
    assert!(solution.report.primary_optimal);
    assert!(solution.report.complete);
    let limited =
        solve_2d_optimized(problem, OptimizationOptions { max_evaluations: 1, ..options })
            .expect("limited open area");
    assert!(limited.report.complete);
    assert!(limited.report.search_truncated);
    assert_eq!(limited.report.open_extents, vec![100, 100]);
    assert!(!limited.report.primary_optimal);
}

#[test]
fn new_objective_and_machine_inputs_reject_misspelled_constraints() {
    use bin_packing::two_d::constrained::SheetConstraints2D;
    use bin_packing::two_d::staged::StagedRules2D;
    use serde_json::json;
    assert!(
        serde_json::from_value::<OptimizationOptions>(json!({"objecitve":"knapsack"})).is_err()
    );
    assert!(serde_json::from_value::<StagedRules2D>(json!({"max_stage":1})).is_err());
    assert!(serde_json::from_value::<SheetConstraints2D>(json!({"trim_lefft":2})).is_err());
    let valid = json!({"packing":{"stock":[{"name":"bar","length":10}],"demands":[{"name":"part","length":5,"quantity":1}]}});
    assert!(serde_json::from_value::<OptimizationProblem1D>(valid.clone()).is_ok());
    let mut wrong = valid;
    wrong["minimum_quantity"] = json!([1]);
    assert!(serde_json::from_value::<OptimizationProblem1D>(wrong).is_err());
}

#[test]
fn every_regular_model_rejects_invalid_objective_work_limits() {
    for (evaluations, beam, states) in
        [(0, 8, 256), (4097, 8, 256), (128, 0, 256), (128, 33, 256), (128, 8, 100_001)]
    {
        let options = OptimizationOptions {
            max_evaluations: evaluations,
            beam_width: beam,
            packing_max_states: states,
            ..Default::default()
        };
        let one = linear_problem();
        assert_constraint(solve_1d_optimized(one.clone(), options.clone()), "search");
        assert_constraint(solve_2d_optimized(rectangular_problem(&one), options.clone()), "search");
        assert_constraint(solve_3d_optimized(spatial_problem(&one), options), "search");
    }
}

#[test]
fn every_regular_model_rejects_misaligned_nonfinite_and_overflowing_values() {
    for (profits, minimum_quantities) in [
        (vec![1.0, 2.0], Vec::new()),
        (Vec::new(), vec![0, 0]),
        (vec![-1.0], Vec::new()),
        (vec![f64::NAN], Vec::new()),
        (vec![f64::INFINITY], Vec::new()),
        (vec![f64::MAX], Vec::new()),
        (Vec::new(), vec![3]),
    ] {
        let one = OptimizationProblem1D { profits, minimum_quantities, ..linear_problem() };
        assert_constraint(
            solve_1d_optimized(one.clone(), OptimizationOptions::default()),
            "values",
        );
        assert_constraint(
            solve_2d_optimized(rectangular_problem(&one), OptimizationOptions::default()),
            "values",
        );
        assert_constraint(
            solve_3d_optimized(spatial_problem(&one), OptimizationOptions::default()),
            "values",
        );
    }
}

#[test]
fn objective_search_rejects_unsupported_axes_and_multiple_open_stock_types() {
    for objective in [
        PackingObjective::OpenDimensionY,
        PackingObjective::OpenDimensionZ,
        PackingObjective::OpenDimensionXy,
    ] {
        assert_constraint(
            solve_1d_optimized(
                linear_problem(),
                OptimizationOptions { objective, ..Default::default() },
            ),
            "objective",
        );
    }
    let mut one = linear_problem();
    let mut second = one.packing.stock[0].clone();
    second.name = "second stock".into();
    one.packing.stock.push(second);
    let options =
        OptimizationOptions { objective: PackingObjective::OpenDimensionX, ..Default::default() };
    assert_constraint(solve_1d_optimized(one.clone(), options.clone()), "objective");
    assert_constraint(solve_2d_optimized(rectangular_problem(&one), options.clone()), "objective");
    assert_constraint(solve_3d_optimized(spatial_problem(&one), options), "objective");
}

#[test]
fn objective_copy_and_type_limits_fail_before_search() {
    let mut excessive_copies = linear_problem();
    excessive_copies.packing.demands[0].quantity = 129;
    let mut excessive_types = linear_problem();
    excessive_types.packing.demands =
        (0..17).map(|_| CutDemand1D { name: "duplicate".into(), length: 1, quantity: 1 }).collect();
    for one in [excessive_copies, excessive_types] {
        assert_constraint(
            solve_1d_optimized(one.clone(), OptimizationOptions::default()),
            "problem",
        );
        assert_constraint(
            solve_2d_optimized(rectangular_problem(&one), OptimizationOptions::default()),
            "problem",
        );
        assert_constraint(
            solve_3d_optimized(spatial_problem(&one), OptimizationOptions::default()),
            "problem",
        );
    }
    // The regular stock validators accept this inventory; the objective
    // wrapper has its own lower, explicit type limit before any search.
    let mut stocks = linear_problem();
    stocks.packing.stock = (0..17)
        .map(|index| Stock1D { name: format!("stock {index}"), ..stocks.packing.stock[0].clone() })
        .collect();
    assert_constraint(
        solve_1d_optimized(stocks.clone(), OptimizationOptions::default()),
        "problem",
    );
    stocks.packing.stock.pop().expect("seventeenth stock");
    let boundary = solve_1d_optimized(stocks, OptimizationOptions::default())
        .expect("sixteen stock types are supported");
    assert!(boundary.report.complete);
    assert_eq!(boundary.report.selected_quantities, vec![2]);
}

#[test]
fn mandatory_oversized_copies_remain_explicit_and_prevent_optimality_claims() {
    let mut one = linear_problem();
    one.packing.demands.insert(0, CutDemand1D { name: "source".into(), length: 11, quantity: 1 });
    one.profits = vec![100.0, 10.0];
    one.minimum_quantities = vec![1, 0];
    let options =
        OptimizationOptions { objective: PackingObjective::Knapsack, ..Default::default() };
    let linear = solve_1d_optimized(one.clone(), options.clone()).expect("partial mandatory cuts");
    let rectangles = solve_2d_optimized(rectangular_problem(&one), options.clone())
        .expect("partial mandatory rectangles");
    let boxes =
        solve_3d_optimized(spatial_problem(&one), options).expect("partial mandatory boxes");
    for report in [linear.report, rectangles.report, boxes.report] {
        assert_eq!(report.selected_quantities, vec![0, 2]);
        assert_eq!(report.total_profit, 20.0);
        assert!(!report.complete);
        assert!(!report.minimums_satisfied);
        assert!(!report.primary_optimal);
        assert!(report.bound_is_upper);
        assert_eq!(report.primary_bound, Some(120.0));
    }
    assert_eq!(linear.demand_indices, vec![vec![1, 1]]);
    assert_eq!(rectangles.demand_indices, vec![vec![1, 1]]);
    assert_eq!(boxes.demand_indices, vec![vec![1, 1]]);
    assert_eq!(linear.solution.unplaced.len(), 1);
    assert_eq!(rectangles.solution.unplaced[0].quantity, 1);
    assert_eq!(boxes.solution.unplaced[0].quantity, 1);
}

#[test]
fn entirely_unfit_optional_demands_return_an_accounted_empty_witness() {
    let mut one = linear_problem();
    one.packing.demands[0].length = 11;
    let options =
        OptimizationOptions { objective: PackingObjective::Knapsack, ..Default::default() };
    let linear = solve_1d_optimized(one.clone(), options.clone()).expect("empty linear witness");
    let rectangles = solve_2d_optimized(rectangular_problem(&one), options.clone())
        .expect("empty rectangular witness");
    let boxes = solve_3d_optimized(spatial_problem(&one), options).expect("empty spatial witness");
    for report in [linear.report, rectangles.report, boxes.report] {
        assert_eq!(report.selected_quantities, vec![0]);
        assert_eq!(report.total_profit, 0.0);
        assert!(report.minimums_satisfied);
        assert!(!report.complete);
        assert!(!report.primary_optimal);
    }
    assert!(linear.solution.layouts.is_empty());
    assert!(rectangles.solution.layouts.is_empty());
    assert!(boxes.solution.layouts.is_empty());
    assert_eq!(linear.solution.unplaced.len(), 2);
    assert_eq!(rectangles.solution.unplaced[0].quantity, 2);
    assert_eq!(boxes.solution.unplaced.len(), 2);
    assert!(boxes.solution.unplaced.iter().all(|demand| demand.quantity == 1));
}

#[test]
fn zero_inventory_is_a_partial_witness_and_never_a_feasibility_certificate() {
    let mut one = linear_problem();
    one.packing.stock[0].available = Some(0);
    let options = OptimizationOptions {
        objective: PackingObjective::Feasibility,
        packing_max_states: 0,
        ..Default::default()
    };
    let reports = [
        solve_1d_optimized(one.clone(), options.clone()).expect("zero linear inventory").report,
        solve_2d_optimized(rectangular_problem(&one), options.clone())
            .expect("zero rectangular inventory")
            .report,
    ];
    for report in reports {
        assert_eq!(report.selected_quantities, vec![0]);
        assert_eq!(report.primary_value, 2.0);
        assert_eq!(report.primary_bound, Some(0.0));
        assert!(!report.bound_is_upper);
        assert!(!report.complete);
        assert!(!report.primary_optimal);
    }
    // Ordinary 3D source validation requires a positive declared stock cap.
    assert!(matches!(
        solve_3d_optimized(spatial_problem(&one), options),
        Err(BinPackingError::InvalidInput(_))
    ));
}

#[test]
fn finite_inventory_shortfalls_remain_partial_in_every_regular_model() {
    let mut one = linear_problem();
    one.packing.stock[0].length = 5;
    one.packing.demands[0].quantity = 3;
    let options =
        OptimizationOptions { objective: PackingObjective::Feasibility, ..Default::default() };
    let reports = [
        solve_1d_optimized(one.clone(), options.clone()).expect("finite linear inventory").report,
        solve_2d_optimized(rectangular_problem(&one), options.clone())
            .expect("finite rectangular inventory")
            .report,
        solve_3d_optimized(spatial_problem(&one), options)
            .expect("finite spatial inventory")
            .report,
    ];
    for report in reports {
        assert_eq!(report.selected_quantities, vec![1]);
        assert_eq!(report.primary_value, 2.0);
        assert_eq!(report.primary_bound, Some(0.0));
        assert!(!report.bound_is_upper);
        assert!(!report.complete);
        assert!(!report.primary_optimal);
    }
}

#[test]
fn material_price_changes_stock_choice_without_losing_source_accounting() {
    let one = material_choice_problem();
    for objective in [PackingObjective::TotalCost, PackingObjective::BinCount] {
        let options = OptimizationOptions { objective, ..Default::default() };
        let linear =
            solve_1d_optimized(one.clone(), options.clone()).expect("material-priced cuts");
        let rectangles = solve_2d_optimized(rectangular_problem(&one), options.clone())
            .expect("material-priced rectangles");
        let boxes =
            solve_3d_optimized(spatial_problem(&one), options).expect("material-priced boxes");
        let (stocks, value, bound) = if objective == PackingObjective::TotalCost {
            (vec![0, 0], 0.8, 0.4)
        } else {
            (vec![1], 1.0, 1.0)
        };
        assert_eq!(linear.stock_indices, stocks);
        assert_eq!(rectangles.stock_indices, stocks);
        assert_eq!(boxes.stock_indices, stocks);
        for report in [linear.report, rectangles.report, boxes.report] {
            assert!(report.complete);
            assert_eq!(report.selected_quantities, vec![2]);
            assert_eq!(report.primary_value, value);
            assert_eq!(report.primary_bound, Some(bound));
            assert_eq!(report.primary_optimal, objective == PackingObjective::BinCount);
        }
    }
}

#[test]
fn linear_open_crop_respects_large_trim_and_cannot_consume_a_second_bar() {
    let mut one = linear_problem();
    one.packing.stock[0].length = 100;
    one.packing.stock[0].trim = 90;
    one.packing.stock[0].kerf = 2;
    one.packing.stock[0].available = Some(10);
    one.packing.demands[0].length = 3;
    let result = solve_1d_optimized(
        one,
        OptimizationOptions { objective: PackingObjective::OpenDimensionX, ..Default::default() },
    )
    .expect("trimmed open bar");
    // 90 units trimmed, two 3-unit cuts, one 2-unit separation.
    assert_eq!(result.report.open_extents, vec![98]);
    assert_eq!(result.report.primary_bound, Some(98.0));
    assert!(result.report.primary_optimal);
    assert_eq!(result.effective_problem.stock[0].available, Some(1));
    assert_eq!(result.solution.stock_count, 1);
    assert_eq!(result.solution.layouts[0].used_length, 8);
}

#[test]
fn linear_aggregate_material_overflow_is_rejected_before_search() {
    let mut one = linear_problem();
    one.packing.stock[0].cost = f64::MAX;
    assert_constraint(solve_1d_optimized(one, OptimizationOptions::default()), "stock.cost");
}

#[test]
fn open_sheet_crop_preserves_fixed_trims_and_intersects_source_defects() {
    use bin_packing::two_d::constrained::{Defect2D, SheetConstraints2D};
    let mut problem = rectangular_problem(&linear_problem());
    let sheet = &mut problem.packing.problem.sheets[0];
    sheet.width = 100;
    sheet.height = 40;
    sheet.quantity = None;
    let demand = &mut problem.packing.problem.demands[0];
    demand.width = 5;
    demand.height = 10;
    demand.quantity = 1;
    problem.packing.constraints = vec![SheetConstraints2D {
        trim_left: 30,
        trim_right: 20,
        trim_top: 5,
        trim_bottom: 5,
        defects: vec![
            Defect2D { x: 90, y: 0, width: 10, height: 3 },
            Defect2D { x: 50, y: 0, width: 20, height: 3 },
        ],
    }];
    let result = solve_2d_optimized(
        problem,
        OptimizationOptions { objective: PackingObjective::OpenDimensionX, ..Default::default() },
    )
    .expect("open sheet retains border preparation");
    assert!(result.report.complete);
    assert_eq!(result.report.open_extents, vec![55, 40]);
    assert!(!result.report.primary_optimal);
    let rules = &result.effective_problem.constraints[0];
    assert_eq!(
        (rules.trim_left, rules.trim_right, rules.trim_top, rules.trim_bottom),
        (30, 20, 5, 5)
    );
    assert_eq!(rules.defects, vec![Defect2D { x: 50, y: 0, width: 5, height: 3 }]);
    let layout = &result.solution.layouts[0];
    assert_eq!(layout.width, 55);
    assert_eq!(layout.placements.len(), 1);
    let part = &layout.placements[0];
    assert_eq!(part.x, 30);
    assert!(part.y >= 5 && part.y + part.height <= 35);
}

#[test]
fn spatial_open_axes_preserve_orientation_and_check_geometry_independently() {
    let mut problem = spatial_problem(&linear_problem());
    let bin = &mut problem.packing.problem.bins[0];
    bin.width = 20;
    bin.height = 20;
    bin.depth = 20;
    let demand = &mut problem.packing.problem.demands[0];
    demand.width = 2;
    demand.height = 3;
    demand.depth = 4;
    for (objective, expected) in [
        (PackingObjective::OpenDimensionX, vec![2, 20, 20]),
        (PackingObjective::OpenDimensionY, vec![20, 3, 20]),
        (PackingObjective::OpenDimensionZ, vec![20, 20, 4]),
        (PackingObjective::OpenDimensionXy, vec![2, 3, 20]),
    ] {
        let result = solve_3d_optimized(
            problem.clone(),
            OptimizationOptions { objective, ..Default::default() },
        )
        .expect("open spatial packing");
        assert!(result.report.complete);
        assert_eq!(result.report.open_extents, expected);
        assert_eq!(result.report.selected_quantities, vec![2]);
        assert_eq!(result.effective_problem.problem.bins[0].quantity, Some(1));
        assert_eq!(result.solution.layouts.len(), 1);
        let layout = &result.solution.layouts[0];
        assert_eq!(layout.placements.len(), 2);
        for part in &layout.placements {
            assert_eq!((part.width, part.height, part.depth), (2, 3, 4));
            assert!(part.x + part.width <= layout.width);
            assert!(part.y + part.height <= layout.height);
            assert!(part.z + part.depth <= layout.depth);
        }
        let a = &layout.placements[0];
        let b = &layout.placements[1];
        assert!(
            a.x + a.width <= b.x
                || b.x + b.width <= a.x
                || a.y + a.height <= b.y
                || b.y + b.height <= a.y
                || a.z + a.depth <= b.z
                || b.z + b.depth <= a.z
        );
    }
}

#[test]
fn optimized_loading_crops_preserve_axles_and_clip_floor_defects() {
    use bin_packing::three_d::loading::{
        AxleLimits3D, FloorAxis3D, FloorDefect3D, LoadingBinRules3D, LoadingProblem3D,
        UnloadingConstraint3D, validate_loading_3d,
    };
    use bin_packing::three_d::optimization::{
        OptimizationProblemLoading3D, solve_loading_3d_optimized,
    };
    for axis in [FloorAxis3D::X, FloorAxis3D::Z] {
        let mut packing = spatial_problem(&linear_problem()).packing;
        packing.problem.bins[0].width = 20;
        packing.problem.bins[0].depth = 20;
        packing.problem.bins[0].height = 4;
        packing.problem.demands[0].width = 2;
        packing.problem.demands[0].height = 4;
        packing.problem.demands[0].depth = 2;
        packing.problem.demands[0].quantity = 1;
        packing.item_weights = vec![8];
        let axles = AxleLimits3D {
            axis,
            front_position: 0,
            rear_position: 12,
            front_maximum: 8,
            rear_maximum: 8,
            front_tare: 0,
            rear_tare: 0,
        };
        let defects = vec![
            FloorDefect3D { x: 10, z: 10, width: 5, depth: 5 },
            FloorDefect3D { x: 18, z: 18, width: 2, depth: 2 },
        ];
        let problem = OptimizationProblemLoading3D {
            packing: LoadingProblem3D {
                packing,
                item_rules: Vec::new(),
                bin_rules: vec![LoadingBinRules3D {
                    axle_limits: Some(axles),
                    defects,
                    ..Default::default()
                }],
                unloading: UnloadingConstraint3D::None,
            },
            profits: Vec::new(),
            minimum_quantities: Vec::new(),
        };
        let result = solve_loading_3d_optimized(
            problem,
            OptimizationOptions {
                objective: if axis == FloorAxis3D::X {
                    PackingObjective::OpenDimensionX
                } else {
                    PackingObjective::OpenDimensionZ
                },
                ..Default::default()
            },
        )
        .expect("fixed support open crop");
        assert!(result.report.complete);
        assert!(!result.report.primary_optimal);
        assert_eq!(
            result.report.open_extents,
            if axis == FloorAxis3D::X { vec![12, 4, 20] } else { vec![20, 4, 12] }
        );
        let rules = &result.effective_problem.bin_rules[0];
        assert_eq!(rules.axle_limits, Some(axles));
        assert_eq!(
            rules.defects,
            vec![FloorDefect3D {
                x: 10,
                z: 10,
                width: if axis == FloorAxis3D::X { 2 } else { 5 },
                depth: if axis == FloorAxis3D::Z { 2 } else { 5 },
            }]
        );
        let layout = &result.solution.layouts[0];
        assert_eq!(layout.payload, 8);
        let stack = &layout.stacks[0];
        let center_twice = match axis {
            FloorAxis3D::X => 2 * u64::from(stack.x) + u64::from(stack.width),
            FloorAxis3D::Z => 2 * u64::from(stack.z) + u64::from(stack.depth),
        };
        let reaction = layout.axle_loads.expect("source retains support reactions");
        // Balance eight units at the actual centre against supports 0 and 12.
        // Doubled coordinates retain half-unit centres without floating point.
        assert!(center_twice <= 24);
        assert_eq!(reaction.front, (8 * (24 - center_twice)).div_ceil(24));
        assert_eq!(reaction.rear, (8 * center_twice).div_ceil(24));
        validate_loading_3d(&result.effective_problem, &result.solution)
            .expect("cropped loading readback");
    }
}

#[test]
fn machine_objectives_rank_material_cost_without_relaxing_the_physical_model() {
    use bin_packing::three_d::loading::{
        LoadingProblem3D, UnloadingConstraint3D, validate_loading_3d,
    };
    use bin_packing::three_d::optimization::{
        OptimizationProblemLoading3D, solve_loading_3d_optimized,
    };
    use bin_packing::two_d::optimization::{
        OptimizationProblemStaged2D, solve_2d_staged_optimized,
    };
    use bin_packing::two_d::staged::{StagedProblem2D, StagedRules2D, validate_2d_staged};
    let one = material_choice_problem();
    for objective in [PackingObjective::BinCount, PackingObjective::TotalCost] {
        let options = OptimizationOptions { objective, ..Default::default() };
        let staged = solve_2d_staged_optimized(
            OptimizationProblemStaged2D {
                packing: StagedProblem2D {
                    packing: rectangular_problem(&one).packing,
                    rules: StagedRules2D::default(),
                    groups: Vec::new(),
                },
                profits: Vec::new(),
                minimum_quantities: Vec::new(),
            },
            options.clone(),
        )
        .expect("staged material selection");
        let loading = solve_loading_3d_optimized(
            OptimizationProblemLoading3D {
                packing: LoadingProblem3D {
                    packing: spatial_problem(&one).packing,
                    item_rules: Vec::new(),
                    bin_rules: Vec::new(),
                    unloading: UnloadingConstraint3D::None,
                },
                profits: Vec::new(),
                minimum_quantities: Vec::new(),
            },
            options,
        )
        .expect("loading material selection");
        let (stocks, value) = if objective == PackingObjective::TotalCost {
            (vec![0, 0], 0.8)
        } else {
            (vec![1], 1.0)
        };
        assert_eq!(staged.stock_indices, stocks);
        assert_eq!(loading.stock_indices, stocks);
        for report in [staged.report, loading.report] {
            assert!(report.complete);
            assert_eq!(report.selected_quantities, vec![2]);
            assert_eq!(report.primary_value, value);
            assert_eq!(report.primary_optimal, objective == PackingObjective::BinCount);
        }
        validate_2d_staged(&staged.effective_problem, &staged.solution)
            .expect("material-ranked physical trees");
        validate_loading_3d(&loading.effective_problem, &loading.solution)
            .expect("material-ranked upright columns");
    }
}

#[test]
fn optimized_loading_accounts_for_every_omitted_copy_after_an_empty_selection() {
    use bin_packing::three_d::loading::{
        LoadingProblem3D, StackItemRules3D, UnloadingConstraint3D, validate_loading_3d,
    };
    use bin_packing::three_d::optimization::{
        OptimizationProblemLoading3D, solve_loading_3d_optimized,
    };
    let mut packing = spatial_problem(&linear_problem()).packing;
    packing.problem.demands[0].width = 11;
    packing.item_weights = vec![7];
    let result = solve_loading_3d_optimized(
        OptimizationProblemLoading3D {
            packing: LoadingProblem3D {
                packing,
                item_rules: vec![StackItemRules3D {
                    maximum_weight_above: Some(0),
                    ..Default::default()
                }],
                bin_rules: Vec::new(),
                unloading: UnloadingConstraint3D::None,
            },
            profits: Vec::new(),
            minimum_quantities: Vec::new(),
        },
        OptimizationOptions { objective: PackingObjective::Knapsack, ..Default::default() },
    )
    .expect("unfit optional loading demand");
    assert!(result.solution.layouts.is_empty());
    assert_eq!(result.solution.unplaced.len(), 1);
    assert_eq!(result.solution.unplaced[0].demand_index, 0);
    assert_eq!(result.solution.unplaced[0].quantity, 2);
    assert_eq!(result.report.selected_quantities, vec![0]);
    assert!(result.report.minimums_satisfied);
    assert!(!result.report.complete);
    assert!(!result.report.primary_optimal);
    assert_eq!(result.report.total_profit, 0.0);
    validate_loading_3d(&result.effective_problem, &result.solution)
        .expect("empty loading conservation");
}
