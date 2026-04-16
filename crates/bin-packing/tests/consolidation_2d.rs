//! Integration tests for the waste-consolidation 2D tiebreaker feature.
//!
//! These tests verify end-to-end behaviour that is accessible from outside the
//! crate: running `solve_2d` with various `min_usable_side` settings and
//! checking the output fields that flow through from `drops::usable_drop_metrics`.
//!
//! Tiebreaker unit tests (which call `TwoDSolution::is_better_than` directly)
//! live in `crates/bin-packing/src/two_d/model.rs` because `is_better_than`
//! is `pub(crate)`.

use bin_packing::two_d::{
    RectDemand2D, Sheet2D, TwoDAlgorithm, TwoDOptions, TwoDProblem, solve_2d,
};

/// The same 2D problem used in `solver_regressions.rs` for consistency.
fn baseline_problem() -> TwoDProblem {
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

/// Regression guard: with `min_usable_side = 0` the tiebreakers produce zero
/// for all candidates (every drop passes a zero threshold, but all algorithms
/// that tie on `total_waste_area` will also tie on the consolidation keys).
/// This test pins the Auto choice so we catch any inadvertent shift.
#[test]
fn zero_threshold_leaves_auto_choice_stable() {
    let problem = baseline_problem();
    let solution = solve_2d(
        problem,
        TwoDOptions { algorithm: TwoDAlgorithm::Auto, seed: Some(11), ..TwoDOptions::default() },
    )
    .expect("2d solve should succeed");
    // Pin established after Task 5 tiebreaker extension. Pre-consolidation
    // Auto chose "max_rects"; with the tiebreaker active even at
    // min_usable_side=0 (all drops pass a zero threshold, so consolidation
    // metrics differ across candidate algorithms), Auto now picks
    // "guillotine_best_short_side_fit" which has equal primary keys
    // (unplaced=0, sheets=1, waste=1512, cost=1) but a larger max_drop
    // (1152 vs 900). This is the correct, intended behavior.
    assert_eq!(
        solution.algorithm, "guillotine_best_short_side_fit",
        "Auto algorithm choice must remain stable with min_usable_side=0"
    );
}

/// With `min_usable_side` larger than the kerf width, thin strips produced by
/// kerf gaps should not appear in either consolidation metric.
#[test]
fn threshold_filter_excludes_small_strips() {
    // Two placements on a sheet with kerf=2. The kerf gap between them
    // is 2 units wide — below min_usable_side=3, so it should be filtered.
    let problem = TwoDProblem {
        sheets: vec![Sheet2D {
            name: "sheet".to_string(),
            width: 40,
            height: 20,
            cost: 1.0,
            quantity: None,
            kerf: 2,
            edge_kerf_relief: false,
        }],
        demands: vec![
            RectDemand2D {
                name: "a".to_string(),
                width: 18,
                height: 20,
                quantity: 1,
                can_rotate: false,
            },
            RectDemand2D {
                name: "b".to_string(),
                width: 18,
                height: 20,
                quantity: 1,
                can_rotate: false,
            },
        ],
    };
    let solution = solve_2d(
        problem,
        TwoDOptions {
            algorithm: TwoDAlgorithm::Auto,
            min_usable_side: 3,
            seed: Some(1),
            ..TwoDOptions::default()
        },
    )
    .expect("2d solve should succeed");

    // With min_usable_side=3, any free strip narrower than 3 should not
    // appear in the metrics. The only free area is the kerf strip (2 wide),
    // so both metrics should be zero.
    for layout in &solution.layouts {
        assert_eq!(
            layout.largest_usable_drop_area, 0,
            "kerf strip should be filtered by min_usable_side=3, layout={layout:?}"
        );
        assert_eq!(
            layout.sum_sq_usable_drop_areas, 0,
            "kerf strip should be filtered by min_usable_side=3, layout={layout:?}"
        );
    }
}

/// Checks that the solution-level aggregates match the per-layout values.
#[test]
fn solution_aggregates_match_per_layout() {
    let problem = baseline_problem();
    for algorithm in [
        TwoDAlgorithm::MaxRects,
        TwoDAlgorithm::Guillotine,
        TwoDAlgorithm::Skyline,
        TwoDAlgorithm::NextFitDecreasingHeight,
        TwoDAlgorithm::Auto,
    ] {
        let solution = solve_2d(
            problem.clone(),
            TwoDOptions { algorithm, min_usable_side: 4, seed: Some(7), ..TwoDOptions::default() },
        )
        .expect("2d solve should succeed");

        let expected_max =
            solution.layouts.iter().map(|l| l.largest_usable_drop_area).max().unwrap_or(0);
        assert_eq!(
            solution.max_usable_drop_area, expected_max,
            "max_usable_drop_area must equal max over layouts for algorithm={algorithm:?}"
        );

        let expected_sum_sq: u128 = solution
            .layouts
            .iter()
            .map(|l| l.sum_sq_usable_drop_areas)
            .fold(0_u128, u128::saturating_add);
        assert_eq!(
            solution.total_sum_sq_usable_drop_areas, expected_sum_sq,
            "total_sum_sq must equal saturating sum over layouts for algorithm={algorithm:?}"
        );
    }
}

/// Runs every algorithm variant and asserts the output metrics satisfy the
/// documented bounds: `largest_drop <= waste_area` and
/// `sum_sq <= (waste_area as u128)²`.
#[test]
fn each_algorithm_produces_valid_consolidation_metrics() {
    let problem = baseline_problem();
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
            TwoDOptions { algorithm, min_usable_side: 4, seed: Some(7), ..TwoDOptions::default() },
        )
        .expect("2d solve should succeed");

        for layout in &solution.layouts {
            assert!(
                layout.largest_usable_drop_area <= layout.waste_area,
                "largest_usable_drop_area must be <= waste_area for algorithm={algorithm:?}, layout={layout:?}"
            );
            let waste_sq = (layout.waste_area as u128) * (layout.waste_area as u128);
            assert!(
                layout.sum_sq_usable_drop_areas <= waste_sq,
                "sum_sq_usable_drop_areas must be <= waste_area^2 for algorithm={algorithm:?}"
            );
        }
    }
}

/// Verifies that two placements placed differently, but with the same total
/// waste, produce non-negative metrics (sanity check).
#[test]
fn consolidation_metrics_are_non_negative_for_all_algorithms() {
    let problem = TwoDProblem {
        sheets: vec![Sheet2D {
            name: "sheet".to_string(),
            width: 30,
            height: 20,
            cost: 1.0,
            quantity: None,
            kerf: 0,
            edge_kerf_relief: false,
        }],
        demands: vec![RectDemand2D {
            name: "panel".to_string(),
            width: 10,
            height: 10,
            quantity: 2,
            can_rotate: false,
        }],
    };
    for algorithm in [TwoDAlgorithm::MaxRects, TwoDAlgorithm::Guillotine, TwoDAlgorithm::Auto] {
        let solution = solve_2d(
            problem.clone(),
            TwoDOptions { algorithm, min_usable_side: 5, seed: Some(3), ..TwoDOptions::default() },
        )
        .expect("2d solve should succeed");

        assert!(
            solution.max_usable_drop_area <= solution.total_waste_area,
            "max_usable_drop_area must be <= total_waste_area"
        );
        // sum_sq is u128, always non-negative by type
        for layout in &solution.layouts {
            assert!(layout.largest_usable_drop_area <= layout.waste_area);
        }
    }
}

/// Verifies that a placement leaving a large contiguous block of free space
/// reports a large `largest_usable_drop_area`.
#[test]
fn large_contiguous_free_space_produces_large_drop_area() {
    // 100×50 sheet, place a single 10×10 in the corner. The free region
    // is large; with min_usable_side=0 the largest drop should be close
    // to the sheet size minus the used area.
    let problem = TwoDProblem {
        sheets: vec![Sheet2D {
            name: "sheet".to_string(),
            width: 100,
            height: 50,
            cost: 1.0,
            quantity: None,
            kerf: 0,
            edge_kerf_relief: false,
        }],
        demands: vec![RectDemand2D {
            name: "tiny".to_string(),
            width: 10,
            height: 10,
            quantity: 1,
            can_rotate: false,
        }],
    };
    let solution = solve_2d(
        problem,
        TwoDOptions {
            algorithm: TwoDAlgorithm::MaxRects,
            min_usable_side: 0,
            seed: Some(1),
            ..TwoDOptions::default()
        },
    )
    .expect("2d solve should succeed");

    // The free region contains a rectangle of at least 90×50=4500 (beside the placement)
    assert!(
        solution.max_usable_drop_area >= 4500,
        "large free region should produce large drop area, got {}",
        solution.max_usable_drop_area
    );
}
