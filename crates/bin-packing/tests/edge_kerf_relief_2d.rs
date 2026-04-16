//! Integration tests for `Sheet2D::edge_kerf_relief`. Covers every public
//! algorithm dispatch and verifies the spec invariants.

use bin_packing::two_d::{
    RectDemand2D, Sheet2D, TwoDAlgorithm, TwoDOptions, TwoDProblem, solve_2d,
};

fn base_sheet(edge_kerf_relief: bool) -> Sheet2D {
    Sheet2D {
        name: "s".into(),
        width: 48,
        height: 10,
        cost: 1.0,
        quantity: None,
        kerf: 1,
        edge_kerf_relief,
    }
}

fn two_24s() -> Vec<RectDemand2D> {
    vec![RectDemand2D { name: "p".into(), width: 24, height: 10, quantity: 2, can_rotate: false }]
}

#[test]
fn every_algorithm_packs_two_piece_overrun_with_relief() {
    // Each algorithm dispatch should produce a single sheet with both
    // parts placed back-to-back, the second extending one kerf past the
    // sheet edge.
    for algorithm in [
        TwoDAlgorithm::Auto,
        TwoDAlgorithm::Guillotine,
        TwoDAlgorithm::MaxRects,
        TwoDAlgorithm::Skyline,
        TwoDAlgorithm::NextFitDecreasingHeight,
    ] {
        let problem = TwoDProblem { sheets: vec![base_sheet(true)], demands: two_24s() };
        let opts = TwoDOptions { algorithm, ..TwoDOptions::default() };

        let sol = solve_2d(problem, opts)
            .unwrap_or_else(|e| panic!("solve_2d failed for {algorithm:?}: {e:?}"));

        assert_eq!(
            sol.sheet_count, 1,
            "algorithm {algorithm:?} should pack both parts on one sheet under edge relief"
        );
        let max_right = sol.layouts[0]
            .placements
            .iter()
            .map(|p| p.x + p.width)
            .max()
            .expect("placements nonempty");
        assert_eq!(
            max_right, 49,
            "algorithm {algorithm:?} should produce a placement extending to sheet.width + kerf"
        );
    }
}

#[test]
fn no_part_is_larger_than_sheet_even_with_relief() {
    // An item wider than the sheet is infeasible regardless of the relief
    // flag. Parts are still bounded by sheet dims (rule 1 in the spec).
    let problem = TwoDProblem {
        sheets: vec![Sheet2D {
            name: "s".into(),
            width: 48,
            height: 10,
            cost: 1.0,
            quantity: None,
            kerf: 1,
            edge_kerf_relief: true,
        }],
        demands: vec![RectDemand2D {
            name: "huge".into(),
            width: 49,
            height: 5,
            quantity: 1,
            can_rotate: false,
        }],
    };

    let opts = TwoDOptions::default();
    let result = solve_2d(problem, opts);
    assert!(result.is_err(), "oversize part must be infeasible even with edge_kerf_relief");
}

#[test]
fn relief_default_is_backward_compatible() {
    // A problem that's solvable on a 50-wide sheet without relief stays
    // identical with the relief flag at its default (false). Both parts
    // must stay strictly within the sheet.
    let problem = TwoDProblem {
        sheets: vec![Sheet2D {
            name: "s".into(),
            width: 50,
            height: 10,
            cost: 1.0,
            quantity: None,
            kerf: 1,
            edge_kerf_relief: false,
        }],
        demands: two_24s(),
    };

    let opts = TwoDOptions::default();
    let sol = solve_2d(problem, opts).expect("should solve");
    assert_eq!(sol.sheet_count, 1);
    for p in &sol.layouts[0].placements {
        assert!(p.x + p.width <= 50);
        assert!(p.y + p.height <= 10);
    }
}

#[test]
fn relief_preserves_part_fits_sheet_invariant() {
    // With relief on, every part's own dimensions are still <= sheet dims
    // (placement positions may overrun, but the parts themselves don't grow).
    let problem = TwoDProblem { sheets: vec![base_sheet(true)], demands: two_24s() };

    let opts = TwoDOptions::default();
    let sol = solve_2d(problem, opts).expect("should solve");
    for layout in &sol.layouts {
        for p in &layout.placements {
            assert!(p.width <= 48 && p.height <= 10, "placement {p:?} has dims exceeding sheet");
        }
    }
}

#[test]
fn used_area_never_exceeds_sheet_area_under_relief() {
    // The from_layouts.used_area clipping (Task 5b) must hold for every
    // packer producing an overrun layout.
    for algorithm in [
        TwoDAlgorithm::Auto,
        TwoDAlgorithm::Guillotine,
        TwoDAlgorithm::MaxRects,
        TwoDAlgorithm::Skyline,
        TwoDAlgorithm::NextFitDecreasingHeight,
    ] {
        let problem = TwoDProblem { sheets: vec![base_sheet(true)], demands: two_24s() };
        let opts = TwoDOptions { algorithm, ..TwoDOptions::default() };
        let sol = solve_2d(problem, opts).expect("should solve");

        for layout in &sol.layouts {
            let sheet_area = u64::from(layout.width) * u64::from(layout.height);
            assert!(
                layout.used_area <= sheet_area,
                "algorithm {algorithm:?}: layout.used_area {} exceeds sheet area {}",
                layout.used_area,
                sheet_area
            );
        }
    }
}

#[test]
fn kerf_area_unchanged_for_single_piece_under_relief() {
    // A single piece on a sheet with relief has no neighbor to be
    // separated from, so kerf area is 0.
    let problem = TwoDProblem {
        sheets: vec![base_sheet(true)],
        demands: vec![RectDemand2D {
            name: "p".into(),
            width: 24,
            height: 10,
            quantity: 1,
            can_rotate: false,
        }],
    };

    let opts = TwoDOptions::default();
    let sol = solve_2d(problem, opts).expect("should solve");
    assert_eq!(sol.layouts[0].kerf_area, 0);
}
