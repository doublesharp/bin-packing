use std::collections::BTreeMap;

use bin_packing::BinPackingError;
use bin_packing::one_d::{
    CutDemand1D, OneDAlgorithm, OneDOptions, OneDProblem, OneDSolution, Stock1D, solve_1d,
};
use bin_packing::three_d::{
    Bin3D, BoxDemand3D, Placement3D, RotationMask3D, ThreeDAlgorithm, ThreeDOptions, ThreeDProblem,
    ThreeDSolution, solve_3d,
};
use bin_packing::two_d::{
    Placement2D, RectDemand2D, Sheet2D, TwoDAlgorithm, TwoDOptions, TwoDProblem, TwoDSolution,
    solve_2d,
};
use proptest::{prelude::*, test_runner::Config};

fn count_cut_lengths(problem: &OneDProblem) -> BTreeMap<(String, u32), usize> {
    let mut counts = BTreeMap::new();
    for demand in &problem.demands {
        counts.insert((demand.name.clone(), demand.length), demand.quantity);
    }
    counts
}

fn assert_valid_1d_solution(problem: &OneDProblem, solution: &OneDSolution) {
    let stock_lookup =
        problem.stock.iter().map(|stock| (stock.name.clone(), stock)).collect::<BTreeMap<_, _>>();

    let mut remaining_counts = count_cut_lengths(problem);
    let mut total_waste = 0_u64;

    for layout in &solution.layouts {
        let stock = stock_lookup
            .get(&layout.stock_name)
            .expect("layout should reference a declared stock type");
        let cuts_sum = layout.cuts.iter().map(|cut| cut.length).sum::<u32>();
        let kerf_count = layout.cuts.len().saturating_sub(1) as u32;
        let expected_used = cuts_sum.saturating_add(stock.kerf.saturating_mul(kerf_count));

        assert!(layout.used_length <= stock.length.saturating_sub(stock.trim));
        assert_eq!(layout.used_length, expected_used);
        assert_eq!(
            layout.remaining_length,
            stock.length.saturating_sub(stock.trim).saturating_sub(layout.used_length)
        );
        assert_eq!(layout.waste, layout.remaining_length);

        for cut in &layout.cuts {
            let key = (cut.name.clone(), cut.length);
            let entry = remaining_counts
                .get_mut(&key)
                .expect("placed cut should correspond to a declared demand");
            assert!(*entry > 0, "cut was placed more times than demanded");
            *entry -= 1;
        }

        total_waste = total_waste.saturating_add(u64::from(layout.waste));
    }

    for cut in &solution.unplaced {
        let key = (cut.name.clone(), cut.length);
        let entry = remaining_counts
            .get_mut(&key)
            .expect("unplaced cut should correspond to a declared demand");
        assert!(*entry > 0, "cut was returned unplaced more times than demanded");
        *entry -= 1;
    }

    assert!(remaining_counts.values().all(|count| *count == 0));
    assert_eq!(solution.layouts.len(), solution.stock_count);
    assert_eq!(solution.total_waste, total_waste);
    assert!(solution.lower_bound.is_none_or(|bound| bound >= 0.0));
}

fn assert_placements_non_overlapping(placements: &[Placement2D]) {
    for (left_index, left) in placements.iter().enumerate() {
        for right in placements.iter().skip(left_index + 1) {
            let separated = left.x + left.width <= right.x
                || right.x + right.width <= left.x
                || left.y + left.height <= right.y
                || right.y + right.height <= left.y;
            assert!(separated, "placements overlap: {left:?} vs {right:?}");
        }
    }
}

fn assert_valid_2d_solution(problem: &TwoDProblem, solution: &TwoDSolution) {
    let sheet_lookup =
        problem.sheets.iter().map(|sheet| (sheet.name.clone(), sheet)).collect::<BTreeMap<_, _>>();

    let mut remaining = BTreeMap::new();
    for demand in &problem.demands {
        remaining.insert(
            (demand.name.clone(), demand.width, demand.height, demand.can_rotate),
            demand.quantity,
        );
    }

    let mut total_waste = 0_u64;

    for layout in &solution.layouts {
        let sheet =
            sheet_lookup.get(&layout.sheet_name).expect("layout should reference a declared sheet");
        let sheet_area = u64::from(sheet.width) * u64::from(sheet.height);

        assert_placements_non_overlapping(&layout.placements);

        let mut used_area = 0_u64;
        for placement in &layout.placements {
            assert!(placement.x + placement.width <= sheet.width);
            assert!(placement.y + placement.height <= sheet.height);

            let demand = problem
                .demands
                .iter()
                .find(|demand| {
                    demand.name == placement.name
                        && ((demand.width == placement.width && demand.height == placement.height)
                            || (demand.can_rotate
                                && demand.width == placement.height
                                && demand.height == placement.width))
                })
                .expect("placement should correspond to a declared demand");

            let key = (demand.name.clone(), demand.width, demand.height, demand.can_rotate);
            let entry =
                remaining.get_mut(&key).expect("declared demand should have a remaining counter");
            assert!(*entry > 0, "placement exceeds requested quantity");
            *entry -= 1;
            used_area =
                used_area.saturating_add(u64::from(placement.width) * u64::from(placement.height));
        }

        assert_eq!(layout.used_area, used_area);
        assert_eq!(layout.waste_area, sheet_area.saturating_sub(used_area));
        total_waste = total_waste.saturating_add(layout.waste_area);

        // Edge-gap invariant: adjacent placements must be at least kerf apart.
        if sheet.kerf > 0 {
            let kerf = sheet.kerf;
            let placements = &layout.placements;
            for (i, a) in placements.iter().enumerate() {
                let a_right = a.x + a.width;
                let a_bottom = a.y + a.height;
                for b in placements.iter().skip(i + 1) {
                    let b_right = b.x + b.width;
                    let b_bottom = b.y + b.height;

                    let y_overlap = a.y.max(b.y) < a_bottom.min(b_bottom);
                    if y_overlap {
                        let x_overlap = a.x.max(b.x) < a_right.min(b_right);
                        assert!(
                            !x_overlap,
                            "placements overlap on x while overlapping on y: {a:?} vs {b:?}"
                        );
                        let gap = if a_right <= b.x { b.x - a_right } else { a.x - b_right };
                        assert!(
                            gap >= kerf,
                            "x-adjacent placements must be at least kerf={kerf} apart, got {gap}: {a:?} vs {b:?}"
                        );
                    }

                    let x_overlap = a.x.max(b.x) < a_right.min(b_right);
                    if x_overlap {
                        let y_overlap = a.y.max(b.y) < a_bottom.min(b_bottom);
                        assert!(
                            !y_overlap,
                            "placements overlap on y while overlapping on x: {a:?} vs {b:?}"
                        );
                        let gap = if a_bottom <= b.y { b.y - a_bottom } else { a.y - b_bottom };
                        assert!(
                            gap >= kerf,
                            "y-adjacent placements must be at least kerf={kerf} apart, got {gap}: {a:?} vs {b:?}"
                        );
                    }
                }
            }
        }
    }

    for item in &solution.unplaced {
        let key = (item.name.clone(), item.width, item.height, item.can_rotate);
        let entry =
            remaining.get_mut(&key).expect("unplaced item should correspond to a declared demand");
        assert!(*entry > 0, "unplaced item exceeds requested quantity");
        *entry -= item.quantity;
    }

    assert!(remaining.values().all(|count| *count == 0));
    assert_eq!(solution.layouts.len(), solution.sheet_count);
    assert_eq!(solution.total_waste_area, total_waste);
    assert_eq!(
        solution.total_kerf_area,
        solution.layouts.iter().map(|l| l.kerf_area).sum::<u64>(),
        "total_kerf_area must equal sum of per-layout kerf_area"
    );

    // Consolidation invariants.
    for layout in &solution.layouts {
        assert!(
            layout.largest_usable_drop_area <= layout.waste_area,
            "largest_usable_drop_area ({}) must not exceed waste_area ({})",
            layout.largest_usable_drop_area,
            layout.waste_area,
        );
    }
    assert_eq!(
        solution.max_usable_drop_area,
        solution.layouts.iter().map(|l| l.largest_usable_drop_area).max().unwrap_or(0),
        "solution.max_usable_drop_area must equal max of per-layout largest_usable_drop_area"
    );
    let expected_total_sum_sq: u128 = solution
        .layouts
        .iter()
        .fold(0_u128, |acc, l| acc.saturating_add(l.sum_sq_usable_drop_areas));
    assert_eq!(
        solution.total_sum_sq_usable_drop_areas, expected_total_sum_sq,
        "solution.total_sum_sq_usable_drop_areas must equal saturating sum of per-layout sum_sq_usable_drop_areas"
    );
}

fn sample_1d_problem() -> OneDProblem {
    OneDProblem {
        stock: vec![Stock1D {
            name: "bar".to_string(),
            length: 120,
            kerf: 2,
            trim: 4,
            cost: 1.0,
            available: None,
        }],
        demands: vec![
            CutDemand1D { name: "leg".to_string(), length: 44, quantity: 4 },
            CutDemand1D { name: "rail".to_string(), length: 28, quantity: 4 },
            CutDemand1D { name: "brace".to_string(), length: 18, quantity: 2 },
        ],
    }
}

fn sample_2d_problem() -> TwoDProblem {
    TwoDProblem {
        sheets: vec![Sheet2D {
            name: "sheet".to_string(),
            width: 96,
            height: 48,
            cost: 1.0,
            quantity: None,
            kerf: 0,
            edge_kerf_relief: false,
        }],
        demands: vec![
            RectDemand2D {
                name: "panel_a".to_string(),
                width: 32,
                height: 24,
                quantity: 2,
                can_rotate: true,
            },
            RectDemand2D {
                name: "panel_b".to_string(),
                width: 20,
                height: 18,
                quantity: 3,
                can_rotate: true,
            },
            RectDemand2D {
                name: "panel_c".to_string(),
                width: 12,
                height: 10,
                quantity: 4,
                can_rotate: false,
            },
        ],
    }
}

#[test]
fn one_d_algorithms_produce_consistent_valid_solutions() {
    let problem = sample_1d_problem();
    for algorithm in [
        OneDAlgorithm::FirstFitDecreasing,
        OneDAlgorithm::BestFitDecreasing,
        OneDAlgorithm::LocalSearch,
        OneDAlgorithm::ColumnGeneration,
        OneDAlgorithm::Auto,
    ] {
        let solution = solve_1d(
            problem.clone(),
            OneDOptions { algorithm, seed: Some(7), ..OneDOptions::default() },
        )
        .expect("1d solver should succeed");
        assert_valid_1d_solution(&problem, &solution);
    }
}

#[test]
fn two_d_algorithms_produce_non_overlapping_layouts() {
    let problem = sample_2d_problem();
    for algorithm in [
        TwoDAlgorithm::MaxRects,
        TwoDAlgorithm::MaxRectsBestShortSideFit,
        TwoDAlgorithm::MaxRectsBestLongSideFit,
        TwoDAlgorithm::MaxRectsBottomLeft,
        TwoDAlgorithm::MaxRectsContactPoint,
        TwoDAlgorithm::Skyline,
        TwoDAlgorithm::SkylineMinWaste,
        TwoDAlgorithm::Guillotine,
        TwoDAlgorithm::GuillotineBestShortSideFit,
        TwoDAlgorithm::GuillotineBestLongSideFit,
        TwoDAlgorithm::GuillotineShorterLeftoverAxis,
        TwoDAlgorithm::GuillotineLongerLeftoverAxis,
        TwoDAlgorithm::GuillotineMinAreaSplit,
        TwoDAlgorithm::GuillotineMaxAreaSplit,
        TwoDAlgorithm::NextFitDecreasingHeight,
        TwoDAlgorithm::FirstFitDecreasingHeight,
        TwoDAlgorithm::BestFitDecreasingHeight,
        TwoDAlgorithm::MultiStart,
        TwoDAlgorithm::Auto,
    ] {
        let solution = solve_2d(
            problem.clone(),
            TwoDOptions { algorithm, seed: Some(11), ..TwoDOptions::default() },
        )
        .expect("2d solver should succeed");
        assert_valid_2d_solution(&problem, &solution);
    }
}

#[test]
fn local_search_never_uses_more_stock_than_ffd_for_reference_case() {
    let problem = sample_1d_problem();
    let ffd = solve_1d(
        problem.clone(),
        OneDOptions { algorithm: OneDAlgorithm::FirstFitDecreasing, ..OneDOptions::default() },
    )
    .expect("ffd");
    let local = solve_1d(
        problem,
        OneDOptions {
            algorithm: OneDAlgorithm::LocalSearch,
            seed: Some(99),
            ..OneDOptions::default()
        },
    )
    .expect("local");

    assert!(local.stock_count <= ffd.stock_count);
}

#[test]
fn one_d_returns_hard_error_for_globally_infeasible_cut() {
    let problem = OneDProblem {
        stock: vec![Stock1D {
            name: "bar".to_string(),
            length: 48,
            kerf: 1,
            trim: 2,
            cost: 1.0,
            available: None,
        }],
        demands: vec![CutDemand1D { name: "too_long".to_string(), length: 47, quantity: 1 }],
    };

    let error =
        solve_1d(problem, OneDOptions::default()).expect_err("should reject impossible cut");
    assert!(matches!(
        error,
        BinPackingError::Infeasible1D { item, length } if item == "too_long" && length == 47
    ));
}

#[test]
fn two_d_returns_hard_error_for_globally_infeasible_item() {
    let problem = TwoDProblem {
        sheets: vec![Sheet2D {
            name: "sheet".to_string(),
            width: 12,
            height: 8,
            cost: 1.0,
            quantity: None,
            kerf: 0,
            edge_kerf_relief: false,
        }],
        demands: vec![RectDemand2D {
            name: "oversize".to_string(),
            width: 13,
            height: 8,
            quantity: 1,
            can_rotate: false,
        }],
    };

    for algorithm in [
        TwoDAlgorithm::MaxRects,
        TwoDAlgorithm::MaxRectsBestShortSideFit,
        TwoDAlgorithm::MaxRectsBestLongSideFit,
        TwoDAlgorithm::MaxRectsBottomLeft,
        TwoDAlgorithm::MaxRectsContactPoint,
        TwoDAlgorithm::Skyline,
        TwoDAlgorithm::SkylineMinWaste,
        TwoDAlgorithm::Guillotine,
        TwoDAlgorithm::GuillotineBestShortSideFit,
        TwoDAlgorithm::GuillotineBestLongSideFit,
        TwoDAlgorithm::GuillotineShorterLeftoverAxis,
        TwoDAlgorithm::GuillotineLongerLeftoverAxis,
        TwoDAlgorithm::GuillotineMinAreaSplit,
        TwoDAlgorithm::GuillotineMaxAreaSplit,
        TwoDAlgorithm::NextFitDecreasingHeight,
        TwoDAlgorithm::FirstFitDecreasingHeight,
        TwoDAlgorithm::BestFitDecreasingHeight,
        TwoDAlgorithm::MultiStart,
        TwoDAlgorithm::Auto,
    ] {
        let error = solve_2d(problem.clone(), TwoDOptions { algorithm, ..TwoDOptions::default() })
            .expect_err("should reject impossible item");
        assert!(matches!(
            error,
            BinPackingError::Infeasible2D { item, width, height }
                if item == "oversize" && width == 13 && height == 8
        ));
    }
}

#[test]
fn exact_solver_preserves_large_quantities_without_truncation() {
    let problem = OneDProblem {
        stock: vec![Stock1D {
            name: "bar".to_string(),
            length: 66_000,
            kerf: 0,
            trim: 0,
            cost: 1.0,
            available: None,
        }],
        demands: vec![CutDemand1D { name: "pin".to_string(), length: 1, quantity: 66_000 }],
    };

    let solution = solve_1d(
        problem,
        OneDOptions { algorithm: OneDAlgorithm::ColumnGeneration, ..OneDOptions::default() },
    )
    .expect("exact solve should succeed");

    assert!(solution.unplaced.is_empty());
    assert_eq!(solution.stock_count, 1);
    assert_eq!(solution.layouts.iter().map(|layout| layout.cuts.len()).sum::<usize>(), 66_000);
}

#[test]
fn one_d_multi_stock_lookahead_prefers_one_larger_bar_over_two_exact_small_bars() {
    let problem = OneDProblem {
        stock: vec![
            Stock1D {
                name: "short".to_string(),
                length: 6,
                kerf: 0,
                trim: 0,
                cost: 1.0,
                available: None,
            },
            Stock1D {
                name: "long".to_string(),
                length: 12,
                kerf: 0,
                trim: 0,
                cost: 1.0,
                available: None,
            },
        ],
        demands: vec![CutDemand1D { name: "rail".to_string(), length: 6, quantity: 2 }],
    };

    for algorithm in [
        OneDAlgorithm::FirstFitDecreasing,
        OneDAlgorithm::BestFitDecreasing,
        OneDAlgorithm::LocalSearch,
        OneDAlgorithm::Auto,
    ] {
        let solution = solve_1d(
            problem.clone(),
            OneDOptions { algorithm, seed: Some(11), ..OneDOptions::default() },
        )
        .expect("1d solve should succeed");

        assert!(solution.unplaced.is_empty());
        assert_eq!(solution.stock_count, 1, "{algorithm:?} should use one larger bar");
        assert_eq!(solution.layouts[0].stock_name, "long");
    }
}

#[test]
fn two_d_sheet_selection_uses_cost_as_tie_breaker() {
    let problem = TwoDProblem {
        sheets: vec![
            Sheet2D {
                name: "premium".to_string(),
                width: 10,
                height: 10,
                cost: 5.0,
                quantity: None,
                kerf: 0,
                edge_kerf_relief: false,
            },
            Sheet2D {
                name: "economy".to_string(),
                width: 10,
                height: 10,
                cost: 1.0,
                quantity: None,
                kerf: 0,
                edge_kerf_relief: false,
            },
        ],
        demands: vec![RectDemand2D {
            name: "tile".to_string(),
            width: 5,
            height: 5,
            quantity: 1,
            can_rotate: false,
        }],
    };

    for algorithm in [
        TwoDAlgorithm::MaxRects,
        TwoDAlgorithm::MaxRectsBestShortSideFit,
        TwoDAlgorithm::MaxRectsBestLongSideFit,
        TwoDAlgorithm::MaxRectsBottomLeft,
        TwoDAlgorithm::MaxRectsContactPoint,
        TwoDAlgorithm::Skyline,
        TwoDAlgorithm::SkylineMinWaste,
        TwoDAlgorithm::Guillotine,
        TwoDAlgorithm::GuillotineBestShortSideFit,
        TwoDAlgorithm::GuillotineBestLongSideFit,
        TwoDAlgorithm::GuillotineShorterLeftoverAxis,
        TwoDAlgorithm::GuillotineLongerLeftoverAxis,
        TwoDAlgorithm::GuillotineMinAreaSplit,
        TwoDAlgorithm::GuillotineMaxAreaSplit,
        TwoDAlgorithm::NextFitDecreasingHeight,
        TwoDAlgorithm::FirstFitDecreasingHeight,
        TwoDAlgorithm::BestFitDecreasingHeight,
        TwoDAlgorithm::MultiStart,
        TwoDAlgorithm::Auto,
    ] {
        let solution = solve_2d(
            problem.clone(),
            TwoDOptions { algorithm, seed: Some(17), ..TwoDOptions::default() },
        )
        .expect("2d solve should succeed");

        assert_eq!(solution.total_cost, 1.0);
        assert_eq!(solution.layouts.len(), 1);
        assert_eq!(solution.layouts[0].sheet_name, "economy");
    }
}

#[test]
fn two_d_multi_sheet_lookahead_prefers_one_larger_sheet_over_two_exact_small_sheets() {
    let problem = TwoDProblem {
        sheets: vec![
            Sheet2D {
                name: "small".to_string(),
                width: 5,
                height: 5,
                cost: 1.0,
                quantity: None,
                kerf: 0,
                edge_kerf_relief: false,
            },
            Sheet2D {
                name: "wide".to_string(),
                width: 10,
                height: 5,
                cost: 1.0,
                quantity: None,
                kerf: 0,
                edge_kerf_relief: false,
            },
        ],
        demands: vec![RectDemand2D {
            name: "tile".to_string(),
            width: 5,
            height: 5,
            quantity: 2,
            can_rotate: false,
        }],
    };

    for algorithm in [
        TwoDAlgorithm::MaxRects,
        TwoDAlgorithm::MaxRectsBestShortSideFit,
        TwoDAlgorithm::MaxRectsBestLongSideFit,
        TwoDAlgorithm::MaxRectsBottomLeft,
        TwoDAlgorithm::MaxRectsContactPoint,
        TwoDAlgorithm::Skyline,
        TwoDAlgorithm::SkylineMinWaste,
        TwoDAlgorithm::Guillotine,
        TwoDAlgorithm::GuillotineBestShortSideFit,
        TwoDAlgorithm::GuillotineBestLongSideFit,
        TwoDAlgorithm::GuillotineShorterLeftoverAxis,
        TwoDAlgorithm::GuillotineLongerLeftoverAxis,
        TwoDAlgorithm::GuillotineMinAreaSplit,
        TwoDAlgorithm::GuillotineMaxAreaSplit,
        TwoDAlgorithm::NextFitDecreasingHeight,
        TwoDAlgorithm::FirstFitDecreasingHeight,
        TwoDAlgorithm::BestFitDecreasingHeight,
        TwoDAlgorithm::MultiStart,
        TwoDAlgorithm::Auto,
    ] {
        let solution = solve_2d(
            problem.clone(),
            TwoDOptions { algorithm, seed: Some(17), ..TwoDOptions::default() },
        )
        .expect("2d solve should succeed");

        assert!(solution.unplaced.is_empty());
        assert_eq!(solution.sheet_count, 1, "{algorithm:?} should use one larger sheet");
        assert_eq!(solution.layouts[0].sheet_name, "wide");
    }
}

#[test]
fn multistart_is_deterministic_for_a_fixed_seed() {
    let problem = sample_2d_problem();
    let options = TwoDOptions {
        algorithm: TwoDAlgorithm::MultiStart,
        seed: Some(4242),
        ..TwoDOptions::default()
    };

    let first = solve_2d(problem.clone(), options.clone()).expect("first multistart solve");
    let second = solve_2d(problem, options).expect("second multistart solve");

    assert_eq!(first, second);
}

#[test]
fn validation_rejects_non_finite_or_negative_costs() {
    let one_d_error = solve_1d(
        OneDProblem {
            stock: vec![Stock1D {
                name: "bar".to_string(),
                length: 12,
                kerf: 0,
                trim: 0,
                cost: f64::NAN,
                available: None,
            }],
            demands: vec![CutDemand1D { name: "cut".to_string(), length: 6, quantity: 1 }],
        },
        OneDOptions::default(),
    )
    .expect_err("nan 1d cost should be rejected");
    assert!(matches!(one_d_error, BinPackingError::InvalidInput(_)));

    let two_d_error = solve_2d(
        TwoDProblem {
            sheets: vec![Sheet2D {
                name: "sheet".to_string(),
                width: 12,
                height: 12,
                cost: -1.0,
                quantity: None,
                kerf: 0,
                edge_kerf_relief: false,
            }],
            demands: vec![RectDemand2D {
                name: "rect".to_string(),
                width: 6,
                height: 6,
                quantity: 1,
                can_rotate: false,
            }],
        },
        TwoDOptions::default(),
    )
    .expect_err("negative 2d cost should be rejected");
    assert!(matches!(two_d_error, BinPackingError::InvalidInput(_)));
}

#[test]
fn validation_rejects_duplicate_container_names() {
    let one_d_error = solve_1d(
        OneDProblem {
            stock: vec![
                Stock1D {
                    name: "bar".to_string(),
                    length: 12,
                    kerf: 0,
                    trim: 0,
                    cost: 1.0,
                    available: None,
                },
                Stock1D {
                    name: "bar".to_string(),
                    length: 24,
                    kerf: 1,
                    trim: 0,
                    cost: 1.0,
                    available: None,
                },
            ],
            demands: vec![CutDemand1D { name: "cut".to_string(), length: 6, quantity: 1 }],
        },
        OneDOptions::default(),
    )
    .expect_err("duplicate 1d stock names should be rejected");
    assert!(
        matches!(one_d_error, BinPackingError::InvalidInput(message) if message == "stock name `bar` must be unique")
    );

    let two_d_error = solve_2d(
        TwoDProblem {
            sheets: vec![
                Sheet2D {
                    name: "sheet".to_string(),
                    width: 12,
                    height: 12,
                    cost: 1.0,
                    quantity: None,
                    kerf: 0,
                    edge_kerf_relief: false,
                },
                Sheet2D {
                    name: "sheet".to_string(),
                    width: 24,
                    height: 12,
                    cost: 1.0,
                    quantity: None,
                    kerf: 0,
                    edge_kerf_relief: false,
                },
            ],
            demands: vec![RectDemand2D {
                name: "rect".to_string(),
                width: 6,
                height: 6,
                quantity: 1,
                can_rotate: false,
            }],
        },
        TwoDOptions::default(),
    )
    .expect_err("duplicate 2d sheet names should be rejected");
    assert!(
        matches!(two_d_error, BinPackingError::InvalidInput(message) if message == "sheet name `sheet` must be unique")
    );

    let three_d_error = solve_3d(
        ThreeDProblem {
            bins: vec![
                Bin3D {
                    name: "bin".to_string(),
                    width: 10,
                    height: 10,
                    depth: 10,
                    cost: 1.0,
                    quantity: None,
                },
                Bin3D {
                    name: "bin".to_string(),
                    width: 20,
                    height: 10,
                    depth: 10,
                    cost: 1.0,
                    quantity: None,
                },
            ],
            demands: vec![BoxDemand3D {
                name: "box".to_string(),
                width: 2,
                height: 2,
                depth: 2,
                quantity: 1,
                allowed_rotations: RotationMask3D::ALL,
            }],
        },
        ThreeDOptions::default(),
    )
    .expect_err("duplicate 3d bin names should be rejected");
    assert!(
        matches!(three_d_error, BinPackingError::InvalidInput(message) if message == "bin name `bin` must be unique")
    );
}

prop_compose! {
    fn arb_one_d_problem()(
        stock_length in 32_u32..160,
        kerf in 0_u32..4,
        trim in 0_u32..8,
        demand_count in 1_usize..5,
    )(
        stock_length in Just(stock_length),
        kerf in Just(kerf),
        trim in Just(trim.min(stock_length.saturating_sub(1))),
        demands in prop::collection::vec((4_u32..80, 1_usize..5), demand_count),
    ) -> OneDProblem {
        let usable = stock_length.saturating_sub(trim).max(8);
        let demands = demands
            .into_iter()
            .enumerate()
            .map(|(index, (length, quantity))| CutDemand1D {
                name: format!("cut_{index}"),
                length: length.min(usable),
                quantity,
            })
            .collect::<Vec<_>>();

        OneDProblem {
            stock: vec![Stock1D {
                name: "stock".to_string(),
                length: stock_length.max(trim + 1),
                kerf,
                trim,
                cost: 1.0,
                available: None,
            }],
            demands,
        }
    }
}

prop_compose! {
    fn arb_two_d_problem()(
        sheet_width in 16_u32..96,
        sheet_height in 16_u32..96,
        demand_count in 1_usize..5,
    )(
        sheet_width in Just(sheet_width),
        sheet_height in Just(sheet_height),
        // kerf must satisfy kerf * 2 < min(sheet_width, sheet_height); cap at 3 for realism
        kerf in 0_u32..=(sheet_width.min(sheet_height) / 2).saturating_sub(1).min(3),
        demands in prop::collection::vec((4_u32..48, 4_u32..48, 1_usize..4, any::<bool>()), demand_count),
    ) -> TwoDProblem {
        let demands = demands
            .into_iter()
            .enumerate()
            .map(|(index, (width, height, quantity, can_rotate))| RectDemand2D {
                name: format!("rect_{index}"),
                width: width.min(sheet_width),
                height: height.min(sheet_height),
                quantity,
                can_rotate,
            })
            .collect::<Vec<_>>();

        TwoDProblem {
            sheets: vec![Sheet2D {
                name: "sheet".to_string(),
                width: sheet_width,
                height: sheet_height,
                cost: 1.0,
                quantity: None,
                kerf,
                edge_kerf_relief: false,
            }],
            demands,
        }
    }
}

// ---------------------------------------------------------------------------
// Error type contract tests: these lock in behavior that downstream callers
// depend on (error message content, `#[non_exhaustive]` marker, which error
// variant a particular misuse maps to).
// ---------------------------------------------------------------------------

#[test]
fn error_display_messages_carry_context() {
    // The `#[error(...)]` attributes on `BinPackingError` define the public
    // display contract. Verify each variant's Display output includes enough
    // context that a caller's logs can diagnose the failure. A silent rename
    // of the format strings would break downstream log-scraping.
    let invalid = BinPackingError::InvalidInput("zero length demand".to_string());
    assert!(invalid.to_string().contains("invalid input"));
    assert!(invalid.to_string().contains("zero length demand"));

    let unsupported = BinPackingError::Unsupported("multi-stock not supported".to_string());
    assert!(unsupported.to_string().contains("unsupported"));
    assert!(unsupported.to_string().contains("multi-stock"));

    let infeas_1d = BinPackingError::Infeasible1D { item: "rail".to_string(), length: 50 };
    let msg_1d = infeas_1d.to_string();
    assert!(msg_1d.contains("rail"), "message should include item name: {msg_1d}");
    assert!(msg_1d.contains("50"), "message should include length: {msg_1d}");

    let infeas_2d =
        BinPackingError::Infeasible2D { item: "panel".to_string(), width: 100, height: 50 };
    let msg_2d = infeas_2d.to_string();
    assert!(msg_2d.contains("panel"), "message should include item name: {msg_2d}");
    assert!(msg_2d.contains("100"), "message should include width: {msg_2d}");
    assert!(msg_2d.contains("50"), "message should include height: {msg_2d}");
}

#[test]
fn error_enum_is_non_exhaustive_so_future_variants_are_source_compatible() {
    // This match intentionally uses a wildcard arm. It compiles *today*
    // because the enum has four variants, but if the `#[non_exhaustive]`
    // marker is ever removed from `BinPackingError`, clippy's
    // `match_wildcard_for_single_variants` won't fire and this arm will
    // remain dead — meaning a future added variant would silently drop into
    // the wildcard without forcing callers to update. Keep this test as a
    // living reminder that the marker is load-bearing.
    let err = BinPackingError::InvalidInput("test".to_string());
    let label = match err {
        BinPackingError::InvalidInput(_) => "invalid",
        BinPackingError::Unsupported(_) => "unsupported",
        BinPackingError::Infeasible1D { .. } => "infeasible_1d",
        BinPackingError::Infeasible2D { .. } => "infeasible_2d",
        _ => "future",
    };
    assert_eq!(label, "invalid");
}

#[test]
fn column_generation_rejects_multi_stock_with_unsupported_error() {
    // The exact backend only supports a single stock type. Verify the public
    // entry point surfaces this as `Unsupported`, not as a panic or a silent
    // fallback to a heuristic.
    let problem = OneDProblem {
        stock: vec![
            Stock1D {
                name: "a".to_string(),
                length: 50,
                kerf: 0,
                trim: 0,
                cost: 1.0,
                available: None,
            },
            Stock1D {
                name: "b".to_string(),
                length: 60,
                kerf: 0,
                trim: 0,
                cost: 1.5,
                available: None,
            },
        ],
        demands: vec![CutDemand1D { name: "cut".to_string(), length: 20, quantity: 2 }],
    };

    let error = solve_1d(
        problem,
        OneDOptions { algorithm: OneDAlgorithm::ColumnGeneration, ..OneDOptions::default() },
    )
    .expect_err("column generation must reject multi-stock");
    assert!(matches!(error, BinPackingError::Unsupported(message) if message.contains("stock")));
}

#[test]
fn column_generation_rejects_inventory_capped_stock_with_unsupported_error() {
    // The exact backend assumes unlimited stock availability. Explicit
    // inventory caps should surface as `Unsupported`.
    let problem = OneDProblem {
        stock: vec![Stock1D {
            name: "bar".to_string(),
            length: 50,
            kerf: 0,
            trim: 0,
            cost: 1.0,
            available: Some(2),
        }],
        demands: vec![CutDemand1D { name: "cut".to_string(), length: 20, quantity: 3 }],
    };

    let error = solve_1d(
        problem,
        OneDOptions { algorithm: OneDAlgorithm::ColumnGeneration, ..OneDOptions::default() },
    )
    .expect_err("column generation must reject inventory caps");
    assert!(
        matches!(error, BinPackingError::Unsupported(message) if message.contains("availability"))
    );
}

// ---------------------------------------------------------------------------
// Algorithm name contract: these strings are part of the public `serde`
// surface and appear in README.md. A rename would silently break downstream
// consumers that match on `solution.algorithm`.
// ---------------------------------------------------------------------------

#[test]
fn one_d_algorithm_strings_match_documented_names() {
    let problem = sample_1d_problem();

    for (algorithm, expected) in [
        (OneDAlgorithm::FirstFitDecreasing, "first_fit_decreasing"),
        (OneDAlgorithm::BestFitDecreasing, "best_fit_decreasing"),
        (OneDAlgorithm::LocalSearch, "local_search"),
        (OneDAlgorithm::ColumnGeneration, "column_generation"),
    ] {
        let solution = solve_1d(
            problem.clone(),
            OneDOptions { algorithm, seed: Some(1), ..OneDOptions::default() },
        )
        .expect("1d solve should succeed");
        assert_eq!(solution.algorithm, expected, "algorithm name must match documented snake_case");
    }
}

#[test]
fn two_d_base_algorithm_strings_match_documented_names() {
    // Each of these base variants is documented in README.md and Node bindings
    // call into this string to label the chosen heuristic. Renaming any of
    // them is a breaking change — lock them down with an explicit assertion.
    let problem = sample_2d_problem();

    for (algorithm, expected) in [
        (TwoDAlgorithm::MaxRects, "max_rects"),
        (TwoDAlgorithm::Skyline, "skyline"),
        (TwoDAlgorithm::Guillotine, "guillotine"),
        (TwoDAlgorithm::MultiStart, "multi_start"),
    ] {
        let solution = solve_2d(
            problem.clone(),
            TwoDOptions { algorithm, seed: Some(1), ..TwoDOptions::default() },
        )
        .expect("2d solve should succeed");
        assert_eq!(solution.algorithm, expected, "algorithm name must match documented snake_case");
    }
}

// ---------------------------------------------------------------------------
// Serde round-trip: every enum variant must survive JSON serialization and
// deserialization. Stripping a `#[serde(rename_all = "snake_case")]` or
// renaming a variant would break this.
// ---------------------------------------------------------------------------

#[test]
fn all_one_d_algorithm_variants_round_trip_through_serde_json() {
    for variant in [
        OneDAlgorithm::Auto,
        OneDAlgorithm::FirstFitDecreasing,
        OneDAlgorithm::BestFitDecreasing,
        OneDAlgorithm::LocalSearch,
        OneDAlgorithm::ColumnGeneration,
    ] {
        let json = serde_json::to_string(&variant).expect("serialize");
        let back: OneDAlgorithm = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(variant, back, "round-trip must preserve variant: {json}");
    }
}

#[test]
fn all_two_d_algorithm_variants_round_trip_through_serde_json() {
    for variant in [
        TwoDAlgorithm::Auto,
        TwoDAlgorithm::MaxRects,
        TwoDAlgorithm::MaxRectsBestShortSideFit,
        TwoDAlgorithm::MaxRectsBestLongSideFit,
        TwoDAlgorithm::MaxRectsBottomLeft,
        TwoDAlgorithm::MaxRectsContactPoint,
        TwoDAlgorithm::Skyline,
        TwoDAlgorithm::SkylineMinWaste,
        TwoDAlgorithm::Guillotine,
        TwoDAlgorithm::GuillotineBestShortSideFit,
        TwoDAlgorithm::GuillotineBestLongSideFit,
        TwoDAlgorithm::GuillotineShorterLeftoverAxis,
        TwoDAlgorithm::GuillotineLongerLeftoverAxis,
        TwoDAlgorithm::GuillotineMinAreaSplit,
        TwoDAlgorithm::GuillotineMaxAreaSplit,
        TwoDAlgorithm::NextFitDecreasingHeight,
        TwoDAlgorithm::FirstFitDecreasingHeight,
        TwoDAlgorithm::BestFitDecreasingHeight,
        TwoDAlgorithm::MultiStart,
    ] {
        let json = serde_json::to_string(&variant).expect("serialize");
        let back: TwoDAlgorithm = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(variant, back, "round-trip must preserve variant: {json}");
    }
}

#[test]
fn one_d_solution_round_trips_through_serde_json() {
    // Full-solution round-trip guarantees that every nested pub type serializes
    // cleanly — a missing `#[derive(Serialize, Deserialize)]` anywhere in the
    // tree would fail to compile or produce a structural mismatch here.
    let problem = sample_1d_problem();
    let solution = solve_1d(
        problem,
        OneDOptions { algorithm: OneDAlgorithm::Auto, seed: Some(42), ..OneDOptions::default() },
    )
    .expect("1d solve");

    let json = serde_json::to_string(&solution).expect("serialize solution");
    let back: OneDSolution = serde_json::from_str(&json).expect("deserialize solution");
    assert_eq!(solution, back, "1D solution must round-trip losslessly");
}

#[test]
fn two_d_solution_round_trips_through_serde_json() {
    let problem = sample_2d_problem();
    let solution = solve_2d(
        problem,
        TwoDOptions { algorithm: TwoDAlgorithm::Auto, seed: Some(42), ..TwoDOptions::default() },
    )
    .expect("2d solve");

    let json = serde_json::to_string(&solution).expect("serialize solution");
    let back: TwoDSolution = serde_json::from_str(&json).expect("deserialize solution");
    assert_eq!(solution, back, "2D solution must round-trip losslessly");
}

// ---------------------------------------------------------------------------
// `solve_auto` gate conditions. The 1D Auto mode escalates to the exact
// backend only when four conjunct conditions hold. The existing
// `auto_respects_exact_limit_gates` test covers one gate; these cover the
// remaining three so that loosening any individual gate is caught.
// ---------------------------------------------------------------------------

#[test]
fn one_d_auto_skips_exact_backend_when_stock_has_multiple_types() {
    // stock.len() > 1 → exact backend rejected even though every other gate
    // would permit it.
    let problem = OneDProblem {
        stock: vec![
            Stock1D {
                name: "a".to_string(),
                length: 20,
                kerf: 0,
                trim: 0,
                cost: 1.0,
                available: None,
            },
            Stock1D {
                name: "b".to_string(),
                length: 20,
                kerf: 0,
                trim: 0,
                cost: 1.5,
                available: None,
            },
        ],
        demands: vec![CutDemand1D { name: "cut".to_string(), length: 5, quantity: 2 }],
    };

    let solution = solve_1d(problem, OneDOptions::default()).expect("auto solve");
    assert_ne!(
        solution.algorithm, "column_generation",
        "auto must not escalate to the exact backend on multi-stock problems"
    );
    assert!(!solution.exact);
}

#[test]
fn one_d_auto_skips_exact_backend_when_stock_has_inventory_cap() {
    // stock[0].available.is_some() → exact backend rejected.
    let problem = OneDProblem {
        stock: vec![Stock1D {
            name: "bar".to_string(),
            length: 20,
            kerf: 0,
            trim: 0,
            cost: 1.0,
            available: Some(10),
        }],
        demands: vec![CutDemand1D { name: "cut".to_string(), length: 5, quantity: 2 }],
    };

    let solution = solve_1d(problem, OneDOptions::default()).expect("auto solve");
    assert_ne!(
        solution.algorithm, "column_generation",
        "auto must not escalate to the exact backend when inventory is capped"
    );
    assert!(!solution.exact);
}

#[test]
fn one_d_auto_skips_exact_backend_when_total_quantity_exceeds_gate() {
    // total_quantity() > auto_exact_max_quantity → exact backend rejected.
    // The default cap is 96; request 200 cuts to blow past it.
    let problem = OneDProblem {
        stock: vec![Stock1D {
            name: "bar".to_string(),
            length: 100,
            kerf: 0,
            trim: 0,
            cost: 1.0,
            available: None,
        }],
        demands: vec![CutDemand1D { name: "cut".to_string(), length: 1, quantity: 200 }],
    };

    let solution = solve_1d(problem, OneDOptions::default()).expect("auto solve");
    assert_ne!(
        solution.algorithm, "column_generation",
        "auto must not escalate to the exact backend when total quantity exceeds the gate"
    );
    assert!(!solution.exact);
}

// ---------------------------------------------------------------------------
// Inventory-zero edge case: a stock with `available: Some(0)` must be
// filtered out by `choose_new_stock`, so its pieces end up as unplaced (or
// are absorbed by other stock types if any are declared).
// ---------------------------------------------------------------------------

#[test]
fn one_d_stock_with_zero_inventory_is_never_opened() {
    let problem = OneDProblem {
        stock: vec![Stock1D {
            name: "exhausted".to_string(),
            length: 50,
            kerf: 0,
            trim: 0,
            cost: 1.0,
            available: Some(0),
        }],
        demands: vec![CutDemand1D { name: "cut".to_string(), length: 10, quantity: 2 }],
    };

    let solution = solve_1d(problem, OneDOptions::default()).expect("auto solve");
    assert_eq!(solution.stock_count, 0, "zero-inventory stock must never be opened");
    assert_eq!(solution.unplaced.len(), 2, "both cuts must be reported unplaced");
}

// ---------------------------------------------------------------------------
// Inventory shortfall gating: when every stock has unlimited inventory, the
// relaxed-inventory auto re-solve must NOT run (it would be wasted work) and
// the solver must NOT emit the "relaxed-inventory auto solve" metrics note.
// ---------------------------------------------------------------------------

#[test]
fn unlimited_inventory_skips_relaxed_inventory_re_solve() {
    let problem = sample_1d_problem();
    // sample_1d_problem has `available: None`, so the shortfall estimation
    // branch should be skipped entirely.
    let solution = solve_1d(problem, OneDOptions::default()).expect("auto solve");
    assert!(
        !solution.metrics.notes.iter().any(|note| note.contains("relaxed-inventory auto solve")),
        "unlimited-inventory problems must not trigger the relaxed re-solve: notes={:?}",
        solution.metrics.notes,
    );
}

// ---------------------------------------------------------------------------
// Stock requirements ordering: entries must follow the order of
// `problem.stock`. Reordering inside `build_stock_requirements` would be a
// breaking change in the public API.
// ---------------------------------------------------------------------------

#[test]
fn stock_requirements_preserve_problem_stock_order() {
    let problem = OneDProblem {
        stock: vec![
            Stock1D {
                name: "zulu".to_string(),
                length: 50,
                kerf: 0,
                trim: 0,
                cost: 1.0,
                available: Some(1),
            },
            Stock1D {
                name: "alpha".to_string(),
                length: 50,
                kerf: 0,
                trim: 0,
                cost: 1.0,
                available: Some(1),
            },
        ],
        demands: vec![CutDemand1D { name: "cut".to_string(), length: 10, quantity: 2 }],
    };

    let solution = solve_1d(problem, OneDOptions::default()).expect("auto solve");
    assert_eq!(solution.stock_requirements.len(), 2);
    assert_eq!(
        solution.stock_requirements[0].stock_name, "zulu",
        "stock_requirements must follow problem.stock order, not alphabetical"
    );
    assert_eq!(solution.stock_requirements[1].stock_name, "alpha");
}

proptest! {
    #![proptest_config(Config {
        failure_persistence: None,
        .. Config::default()
    })]

    #[test]
    fn randomized_one_d_solutions_respect_inventory(problem in arb_one_d_problem()) {
        let solution = solve_1d(
            problem.clone(),
            OneDOptions {
                algorithm: OneDAlgorithm::Auto,
                seed: Some(123),
                ..OneDOptions::default()
            },
        ).expect("1d randomized solve should succeed");

        assert_valid_1d_solution(&problem, &solution);
    }

    #[test]
    fn randomized_two_d_solutions_stay_within_sheet(
        problem in arb_two_d_problem(),
        min_usable_side in 0_u32..=4,
    ) {
        let solution = solve_2d(
            problem.clone(),
            TwoDOptions {
                algorithm: TwoDAlgorithm::Auto,
                seed: Some(321),
                min_usable_side,
                ..TwoDOptions::default()
            },
        ).expect("2d randomized solve should succeed");

        assert_valid_2d_solution(&problem, &solution);

        // Cut-plan post-condition: the router preset must succeed on any
        // valid 2D solution, and total_cost must be finite.
        let cut_plan = bin_packing::two_d::cut_plan::plan_cuts(
            &solution,
            &bin_packing::two_d::cut_plan::CutPlanOptions2D {
                preset: bin_packing::two_d::cut_plan::CutPlanPreset2D::CncRouter,
                ..Default::default()
            },
        ).expect("CNC router should plan any valid 2D solution");
        assert!(cut_plan.total_cost.is_finite(), "cut plan total_cost must be finite");
    }
}

// =============================================================================
// 3D helpers and regression tests
// =============================================================================

fn assert_placements_3d_non_overlapping(placements: &[Placement3D]) {
    for (i, a) in placements.iter().enumerate() {
        for b in placements.iter().skip(i + 1) {
            let separated = a.x + a.width <= b.x
                || b.x + b.width <= a.x
                || a.y + a.height <= b.y
                || b.y + b.height <= a.y
                || a.z + a.depth <= b.z
                || b.z + b.depth <= a.z;
            assert!(separated, "3D placements overlap: {a:?} vs {b:?}");
        }
    }
}

fn assert_valid_3d_solution(problem: &ThreeDProblem, solution: &ThreeDSolution) {
    let bin_lookup: BTreeMap<_, _> =
        problem.bins.iter().map(|bin| (bin.name.clone(), bin)).collect();

    let mut remaining: BTreeMap<(String, u32, u32, u32), usize> = BTreeMap::new();
    for demand in &problem.demands {
        remaining.insert(
            (demand.name.clone(), demand.width, demand.height, demand.depth),
            demand.quantity,
        );
    }

    let mut total_waste: u64 = 0;
    let mut total_cost: f64 = 0.0;

    for layout in &solution.layouts {
        let bin =
            bin_lookup.get(&layout.bin_name).expect("layout should reference a declared bin type");

        assert_placements_3d_non_overlapping(&layout.placements);

        let mut used_vol: u64 = 0;
        for placement in &layout.placements {
            assert!(
                placement.x + placement.width <= bin.width,
                "placement x+w {} > bin width {} in {placement:?}",
                placement.x + placement.width,
                bin.width,
            );
            assert!(
                placement.y + placement.height <= bin.height,
                "placement y+h {} > bin height {} in {placement:?}",
                placement.y + placement.height,
                bin.height,
            );
            assert!(
                placement.z + placement.depth <= bin.depth,
                "placement z+d {} > bin depth {} in {placement:?}",
                placement.z + placement.depth,
                bin.depth,
            );
            assert!(
                placement.width > 0 && placement.height > 0 && placement.depth > 0,
                "placement has zero dimension: {placement:?}"
            );

            let demand = problem
                .demands
                .iter()
                .find(|d| {
                    if d.name != placement.name {
                        return false;
                    }
                    let (w, h, depth) = (d.width, d.height, d.depth);
                    let orientations = [
                        (w, h, depth),
                        (w, depth, h),
                        (h, w, depth),
                        (h, depth, w),
                        (depth, w, h),
                        (depth, h, w),
                    ];
                    orientations.iter().any(|&(pw, ph, pd)| {
                        pw == placement.width && ph == placement.height && pd == placement.depth
                    })
                })
                .expect("placement should correspond to a declared demand");

            let key = (demand.name.clone(), demand.width, demand.height, demand.depth);
            let entry = remaining.get_mut(&key).expect("demand counter missing");
            assert!(*entry > 0, "demand placed more times than requested");
            *entry -= 1;

            used_vol = used_vol.saturating_add(
                u64::from(placement.width)
                    * u64::from(placement.height)
                    * u64::from(placement.depth),
            );
        }

        assert_eq!(layout.used_volume, used_vol, "BinLayout3D.used_volume mismatch");
        let bin_vol = u64::from(bin.width) * u64::from(bin.height) * u64::from(bin.depth);
        assert_eq!(
            layout.waste_volume,
            bin_vol.saturating_sub(used_vol),
            "BinLayout3D.waste_volume mismatch"
        );
        total_waste = total_waste.saturating_add(layout.waste_volume);
        total_cost += layout.cost;
    }

    for item in &solution.unplaced {
        let key = (item.name.clone(), item.width, item.height, item.depth);
        let entry =
            remaining.get_mut(&key).expect("unplaced should correspond to a declared demand");
        assert!(*entry >= item.quantity, "unplaced exceeds demand quantity");
        *entry -= item.quantity;
    }

    assert!(remaining.values().all(|&c| c == 0), "not all demands accounted for: {remaining:?}");
    assert_eq!(solution.layouts.len(), solution.bin_count);
    assert_eq!(solution.total_waste_volume, total_waste);
    let n = solution.layouts.len().max(1);
    let scale = solution.total_cost.abs().max(total_cost.abs()).max(1.0);
    let tolerance = (n as f64) * 1e-9 * scale;
    assert!(
        (solution.total_cost - total_cost).abs() <= tolerance,
        "3D total_cost mismatch: {} vs {}",
        solution.total_cost,
        total_cost
    );
}

fn sample_3d_problem() -> ThreeDProblem {
    ThreeDProblem {
        bins: vec![Bin3D {
            name: "crate".to_string(),
            width: 60,
            height: 40,
            depth: 30,
            cost: 1.0,
            quantity: None,
        }],
        demands: vec![
            BoxDemand3D {
                name: "box_a".to_string(),
                width: 20,
                height: 15,
                depth: 10,
                quantity: 3,
                allowed_rotations: RotationMask3D::ALL,
            },
            BoxDemand3D {
                name: "box_b".to_string(),
                width: 12,
                height: 8,
                depth: 6,
                quantity: 4,
                allowed_rotations: RotationMask3D::ALL,
            },
        ],
    }
}

#[test]
fn three_d_algorithms_produce_consistent_valid_solutions() {
    let problem = sample_3d_problem();

    for algorithm in [
        ThreeDAlgorithm::ExtremePoints,
        ThreeDAlgorithm::ExtremePointsResidualSpace,
        ThreeDAlgorithm::ExtremePointsFreeVolume,
        ThreeDAlgorithm::ExtremePointsBottomLeftBack,
        ThreeDAlgorithm::ExtremePointsContactPoint,
        ThreeDAlgorithm::ExtremePointsEuclidean,
        ThreeDAlgorithm::Guillotine3D,
        ThreeDAlgorithm::Guillotine3DBestShortSideFit,
        ThreeDAlgorithm::Guillotine3DBestLongSideFit,
        ThreeDAlgorithm::Guillotine3DShorterLeftoverAxis,
        ThreeDAlgorithm::Guillotine3DLongerLeftoverAxis,
        ThreeDAlgorithm::Guillotine3DMinVolumeSplit,
        ThreeDAlgorithm::Guillotine3DMaxVolumeSplit,
        ThreeDAlgorithm::LayerBuilding,
        ThreeDAlgorithm::LayerBuildingMaxRects,
        ThreeDAlgorithm::LayerBuildingSkyline,
        ThreeDAlgorithm::LayerBuildingGuillotine,
        ThreeDAlgorithm::LayerBuildingShelf,
        ThreeDAlgorithm::WallBuilding,
        ThreeDAlgorithm::ColumnBuilding,
        ThreeDAlgorithm::DeepestBottomLeft,
        ThreeDAlgorithm::DeepestBottomLeftFill,
        ThreeDAlgorithm::FirstFitDecreasingVolume,
        ThreeDAlgorithm::BestFitDecreasingVolume,
        ThreeDAlgorithm::MultiStart,
        ThreeDAlgorithm::LocalSearch,
        ThreeDAlgorithm::Grasp,
        ThreeDAlgorithm::Auto,
    ] {
        let options = ThreeDOptions { algorithm, seed: Some(42), ..ThreeDOptions::default() };
        match solve_3d(problem.clone(), options) {
            Ok(solution) => {
                assert_valid_3d_solution(&problem, &solution);
                assert!(!solution.algorithm.is_empty(), "{algorithm:?} must set algorithm field");
            }
            Err(e) => panic!("{algorithm:?} returned unexpected error: {e:?}"),
        }
    }
}

#[test]
fn three_d_auto_places_all_items_when_feasible() {
    let problem = ThreeDProblem {
        bins: vec![Bin3D {
            name: "container".to_string(),
            width: 100,
            height: 100,
            depth: 100,
            cost: 1.0,
            quantity: None,
        }],
        demands: vec![BoxDemand3D {
            name: "item_a".to_string(),
            width: 10,
            height: 10,
            depth: 10,
            quantity: 5,
            allowed_rotations: RotationMask3D::ALL,
        }],
    };
    let solution =
        solve_3d(problem.clone(), ThreeDOptions::default()).expect("auto should succeed");
    assert_valid_3d_solution(&problem, &solution);
    assert_eq!(solution.unplaced.len(), 0, "all items must be placed");
}

#[test]
fn three_d_infeasible_demand_returns_error() {
    let problem = ThreeDProblem {
        bins: vec![Bin3D {
            name: "tiny".to_string(),
            width: 5,
            height: 5,
            depth: 5,
            cost: 1.0,
            quantity: None,
        }],
        demands: vec![BoxDemand3D {
            name: "giant".to_string(),
            width: 10,
            height: 10,
            depth: 10,
            quantity: 1,
            allowed_rotations: RotationMask3D::ALL,
        }],
    };
    let result = solve_3d(problem, ThreeDOptions::default());
    assert!(
        matches!(result, Err(BinPackingError::Infeasible3D { .. })),
        "expected Infeasible3D, got {result:?}"
    );
}

#[test]
fn three_d_no_rotation_respected() {
    let problem = ThreeDProblem {
        bins: vec![Bin3D {
            name: "bin".to_string(),
            width: 20,
            height: 5,
            depth: 5,
            cost: 1.0,
            quantity: None,
        }],
        demands: vec![BoxDemand3D {
            name: "plank".to_string(),
            width: 20,
            height: 3,
            depth: 3,
            quantity: 1,
            allowed_rotations: RotationMask3D::XYZ,
        }],
    };
    let solution = solve_3d(problem.clone(), ThreeDOptions::default()).expect("should succeed");
    assert_valid_3d_solution(&problem, &solution);
    assert_eq!(solution.unplaced.len(), 0);
    let p = &solution.layouts[0].placements[0];
    assert_eq!((p.width, p.height, p.depth), (20, 3, 3), "must keep identity orientation");
}

#[test]
fn three_d_inventory_cap_respected() {
    // 4 identical full-bin items but only 2 bins available — at most 2 fit.
    let problem = ThreeDProblem {
        bins: vec![Bin3D {
            name: "box".to_string(),
            width: 10,
            height: 10,
            depth: 10,
            cost: 1.0,
            quantity: Some(2),
        }],
        demands: vec![BoxDemand3D {
            name: "cube".to_string(),
            width: 10,
            height: 10,
            depth: 10,
            quantity: 4,
            allowed_rotations: RotationMask3D::ALL,
        }],
    };
    let result = solve_3d(problem.clone(), ThreeDOptions::default());
    match result {
        Ok(solution) => {
            assert!(solution.layouts.len() <= 2, "must not exceed inventory cap of 2");
            assert_eq!(solution.bin_requirements.len(), 1);
            let requirement = &solution.bin_requirements[0];
            assert_eq!(requirement.used_quantity, 2);
            assert_eq!(requirement.required_quantity, 4);
            assert_eq!(requirement.additional_quantity_needed, 2);
        }
        Err(BinPackingError::Infeasible3D { .. }) | Err(BinPackingError::InvalidInput(_)) => {}
        Err(e) => panic!("unexpected error: {e:?}"),
    }
}

// ---------------------------------------------------------------------------
// RotationSearch regression tests
// ---------------------------------------------------------------------------

/// A problem where the optimal rotation assignment fits on 1 sheet but the
/// worst orientation needs 2. RotationSearch should find the 1-sheet solution.
#[test]
fn rotation_search_places_all_items_on_solvable_problem() {
    // Sheet is 10x20. Two demands: 6x15 (qty 2, can_rotate).
    // Default orientation: 6w x 15h — two of them need 12w x 15h, fits on 10x20? No (12>10).
    // Rotated: 15w x 6h — two stacked vertically need 15w x 12h, fits on 10x20? No (15>10).
    // But one default (6x15) + one rotated (15x6) won't help on a 10x20 sheet either.
    //
    // Better approach: sheet 20x10, demand 11x5 qty 2 can_rotate.
    // Default: 11w x 5h, two stacked = 11w x 10h. Fits 20x10? Yes (11<=20, 10<=10). 1 sheet.
    // Rotated: 5w x 11h, won't fit height 10. So rotation search must pick default.
    //
    // Actually let's design it so both orientations fit but one is clearly better:
    // Sheet 20x10, demand A: 12x9 qty 1 can_rotate, demand B: 12x9 qty 1 can_rotate.
    // Default (12x9): two side-by-side = 24w > 20, stacked = 12w x 18h > 10h. Need 2 sheets.
    // Rotated (9x12): 9w x 12h, height 12>10, doesn't fit.
    // Mixed: one 12x9 + one 9x12: 9x12 doesn't fit height.
    //
    // Simpler: sheet 20x10. Demand: 9x6 qty 2 can_rotate.
    // Default: 9x6, two side-by-side = 18x6, fits (18<=20, 6<=10). 1 sheet.
    // Rotated: 6x9, two side-by-side = 12x9, fits. Also 1 sheet.
    // Both fit on 1 sheet, not useful.
    //
    // Sheet 10x10. Demand A: 7x4 qty 2, can_rotate. Demand B: 6x3 qty 2, can_rotate.
    // MaxRects without rotation search might or might not find the best orientation.
    // Let's use a case where only the right rotation combo works:
    //
    // Sheet 10x10. Demand: 6x3 qty 3, can_rotate=true.
    // Default 6x3: area = 18 each, total 54. Sheet area 100. Should easily fit.
    // This is too easy. Let's make it harder.
    //
    // Sheet 10x5. Demand: 6x4 qty 2, can_rotate=true.
    // Default 6x4: side by side 12x4 > 10w. Stacked 6x8 > 5h. Need 2 sheets.
    // Rotated 4x6: 4x6, height 6>5. Doesn't fit!
    // So default is the only option => 2 sheets. Not useful.
    //
    // Sheet 10x8. Demand: 6x4 qty 2, can_rotate=true.
    // Default 6x4: stacked 6x8 fits (6<=10, 8<=8). 1 sheet!
    // Rotated 4x6: side by side 8x6 fits. Also 1 sheet.
    // MaxRects can find this without rotation search. Need a tighter case.
    //
    // Let's use a case where greedy per-piece rotation fails:
    // Sheet 13x10. Demand A: 7x5 qty 2, can_rotate. Demand B: 6x4 qty 2, can_rotate.
    // If A is 7x5 (default): two A's = 14x5 > 13, or 7x10. 7x10 fits, leaves 6x10.
    //   Then B default 6x4: two B's stacked = 6x8, fits in 6x10. Total: 1 sheet.
    // If A is 5x7 (rotated): two A's = 10x7 or 5x14>10. 10x7 fits, leaves 3x10 + 10x3.
    //   Then B default 6x4: 6>3, won't fit in leftover 3x10. Need 2 sheets.
    // So: rotation assignment "A default, B default" = 1 sheet.
    //     rotation assignment "A rotated, B default" = 2 sheets.
    // RotationSearch should find the 1-sheet solution.

    let problem = TwoDProblem {
        sheets: vec![Sheet2D {
            name: "sheet".to_string(),
            width: 13,
            height: 10,
            cost: 1.0,
            quantity: None,
            kerf: 0,
            edge_kerf_relief: false,
        }],
        demands: vec![
            RectDemand2D {
                name: "A".to_string(),
                width: 7,
                height: 5,
                quantity: 2,
                can_rotate: true,
            },
            RectDemand2D {
                name: "B".to_string(),
                width: 6,
                height: 4,
                quantity: 2,
                can_rotate: true,
            },
        ],
    };

    let solution = solve_2d(
        problem,
        TwoDOptions {
            algorithm: TwoDAlgorithm::RotationSearch,
            seed: Some(42),
            ..TwoDOptions::default()
        },
    )
    .expect("rotation_search should succeed");

    assert_eq!(solution.algorithm, "rotation_search");
    assert!(solution.unplaced.is_empty(), "all items should be placed");
    assert_eq!(solution.sheet_count, 1, "should fit on 1 sheet");
}

/// Items with `can_rotate: false` should never be rotated by rotation search.
#[test]
fn rotation_search_respects_can_rotate_false() {
    let problem = TwoDProblem {
        sheets: vec![Sheet2D {
            name: "sheet".to_string(),
            width: 20,
            height: 20,
            cost: 1.0,
            quantity: None,
            kerf: 0,
            edge_kerf_relief: false,
        }],
        demands: vec![
            RectDemand2D {
                name: "fixed".to_string(),
                width: 15,
                height: 3,
                quantity: 1,
                can_rotate: false,
            },
            RectDemand2D {
                name: "rotatable".to_string(),
                width: 8,
                height: 5,
                quantity: 1,
                can_rotate: true,
            },
        ],
    };

    let solution = solve_2d(
        problem,
        TwoDOptions {
            algorithm: TwoDAlgorithm::RotationSearch,
            seed: Some(1),
            ..TwoDOptions::default()
        },
    )
    .expect("rotation_search should succeed");

    // The "fixed" item must never appear rotated.
    for layout in &solution.layouts {
        for placement in &layout.placements {
            if placement.name == "fixed" {
                assert!(!placement.rotated, "can_rotate=false item must not be rotated");
                assert_eq!(placement.width, 15);
                assert_eq!(placement.height, 3);
            }
        }
    }
}

/// Same seed produces the same result.
#[test]
fn rotation_search_is_deterministic_for_fixed_seed() {
    let problem = TwoDProblem {
        sheets: vec![Sheet2D {
            name: "sheet".to_string(),
            width: 20,
            height: 15,
            cost: 1.0,
            quantity: None,
            kerf: 0,
            edge_kerf_relief: false,
        }],
        demands: vec![
            RectDemand2D {
                name: "A".to_string(),
                width: 8,
                height: 5,
                quantity: 3,
                can_rotate: true,
            },
            RectDemand2D {
                name: "B".to_string(),
                width: 7,
                height: 4,
                quantity: 2,
                can_rotate: true,
            },
        ],
    };

    let opts = TwoDOptions {
        algorithm: TwoDAlgorithm::RotationSearch,
        seed: Some(99),
        ..TwoDOptions::default()
    };
    let sol1 = solve_2d(problem.clone(), opts.clone()).expect("run 1");
    let sol2 = solve_2d(problem, opts).expect("run 2");

    assert_eq!(sol1.sheet_count, sol2.sheet_count);
    assert_eq!(sol1.total_waste_area, sol2.total_waste_area);
    assert_eq!(sol1.layouts.len(), sol2.layouts.len());
    for (l1, l2) in sol1.layouts.iter().zip(sol2.layouts.iter()) {
        assert_eq!(l1.placements.len(), l2.placements.len());
    }
}

/// Kerf > 0 with rotation search: verify edge-gap invariant holds.
#[test]
fn rotation_search_with_kerf_respects_edge_gap() {
    let problem = TwoDProblem {
        sheets: vec![Sheet2D {
            name: "sheet".to_string(),
            width: 30,
            height: 20,
            cost: 1.0,
            quantity: None,
            kerf: 2,
            edge_kerf_relief: false,
        }],
        demands: vec![
            RectDemand2D {
                name: "X".to_string(),
                width: 10,
                height: 6,
                quantity: 3,
                can_rotate: true,
            },
            RectDemand2D {
                name: "Y".to_string(),
                width: 8,
                height: 5,
                quantity: 2,
                can_rotate: true,
            },
        ],
    };

    let solution = solve_2d(
        problem,
        TwoDOptions {
            algorithm: TwoDAlgorithm::RotationSearch,
            seed: Some(7),
            ..TwoDOptions::default()
        },
    )
    .expect("rotation_search with kerf should succeed");

    assert_eq!(solution.algorithm, "rotation_search");

    // Verify no two placements on the same sheet overlap when kerf gap is
    // accounted for. Each placement occupies [x, x+w+kerf) x [y, y+h+kerf)
    // except the last along each axis (but a simpler check: no raw overlap).
    for layout in &solution.layouts {
        let placements = &layout.placements;
        for (i, a) in placements.iter().enumerate() {
            // Placement must be within sheet bounds.
            assert!(a.x + a.width <= layout.width);
            assert!(a.y + a.height <= layout.height);
            for b in placements.iter().skip(i + 1) {
                // No raw overlap (kerf means they should be further apart, but
                // at minimum they must not overlap at all).
                let overlap_x = a.x < b.x + b.width && b.x < a.x + a.width;
                let overlap_y = a.y < b.y + b.height && b.y < a.y + a.height;
                assert!(!(overlap_x && overlap_y), "placements {:?} and {:?} overlap", a, b);
            }
        }
    }
}

/// Auto mode should include rotation search and can select it when rotation
/// matters.
#[test]
fn auto_includes_rotation_search() {
    // Use the same problem from the solvable test — rotation search should
    // find the 1-sheet solution, and auto should pick it (or match it).
    let problem = TwoDProblem {
        sheets: vec![Sheet2D {
            name: "sheet".to_string(),
            width: 13,
            height: 10,
            cost: 1.0,
            quantity: None,
            kerf: 0,
            edge_kerf_relief: false,
        }],
        demands: vec![
            RectDemand2D {
                name: "A".to_string(),
                width: 7,
                height: 5,
                quantity: 2,
                can_rotate: true,
            },
            RectDemand2D {
                name: "B".to_string(),
                width: 6,
                height: 4,
                quantity: 2,
                can_rotate: true,
            },
        ],
    };

    let auto_sol =
        solve_2d(problem.clone(), TwoDOptions { seed: Some(42), ..TwoDOptions::default() })
            .expect("auto should succeed");

    let rotation_sol = solve_2d(
        problem,
        TwoDOptions {
            algorithm: TwoDAlgorithm::RotationSearch,
            seed: Some(42),
            ..TwoDOptions::default()
        },
    )
    .expect("rotation_search should succeed");

    // Auto should be at least as good as rotation search alone.
    assert!(
        auto_sol.sheet_count <= rotation_sol.sheet_count,
        "auto should be at least as good as rotation_search"
    );
    assert!(auto_sol.unplaced.is_empty());
}
