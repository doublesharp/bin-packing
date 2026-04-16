//! Integration tests covering the edge-gap invariant for every 2D algorithm
//! when `Sheet2D.kerf > 0`.

use bin_packing::two_d::{
    Placement2D, RectDemand2D, Sheet2D, TwoDAlgorithm, TwoDOptions, TwoDProblem, solve_2d,
};

fn simple_problem(algorithm: TwoDAlgorithm, kerf: u32) -> (TwoDProblem, TwoDOptions) {
    let problem = TwoDProblem {
        sheets: vec![Sheet2D {
            name: "sheet".to_string(),
            width: 20,
            height: 20,
            cost: 1.0,
            quantity: None,
            kerf,
            edge_kerf_relief: false,
        }],
        demands: vec![RectDemand2D {
            name: "panel".to_string(),
            width: 8,
            height: 8,
            quantity: 4,
            can_rotate: true,
        }],
    };
    let options = TwoDOptions { algorithm, seed: Some(7), ..Default::default() };
    (problem, options)
}

fn assert_edge_gap_respected(placements: &[Placement2D], kerf: u32) {
    for (i, a) in placements.iter().enumerate() {
        let a_right = a.x + a.width;
        let a_bottom = a.y + a.height;
        for b in &placements[i + 1..] {
            let b_right = b.x + b.width;
            let b_bottom = b.y + b.height;

            // Along x: if intervals overlap on y, they must not overlap on
            // x AND must be at least `kerf` apart when not overlapping.
            let y_overlap = a.y.max(b.y) < a_bottom.min(b_bottom);
            if y_overlap {
                let x_overlap = a.x.max(b.x) < a_right.min(b_right);
                assert!(
                    !x_overlap,
                    "placements overlap on x while overlapping on y: {a:?} vs {b:?}"
                );
                let gap = if a_right <= b.x {
                    b.x - a_right
                } else if b_right <= a.x {
                    a.x - b_right
                } else {
                    unreachable!("already checked non-overlap")
                };
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
                let gap = if a_bottom <= b.y {
                    b.y - a_bottom
                } else if b_bottom <= a.y {
                    a.y - b_bottom
                } else {
                    unreachable!("already checked non-overlap")
                };
                assert!(
                    gap >= kerf,
                    "y-adjacent placements must be at least kerf={kerf} apart, got {gap}: {a:?} vs {b:?}"
                );
            }
        }
    }
}

#[test]
fn nfdh_respects_kerf_gap() {
    let (problem, options) = simple_problem(TwoDAlgorithm::NextFitDecreasingHeight, 2);
    let solution = solve_2d(problem, options).expect("solve");
    for layout in &solution.layouts {
        assert_edge_gap_respected(&layout.placements, 2);
    }
}

#[test]
fn ffdh_respects_kerf_gap() {
    let (problem, options) = simple_problem(TwoDAlgorithm::FirstFitDecreasingHeight, 2);
    let solution = solve_2d(problem, options).expect("solve");
    for layout in &solution.layouts {
        assert_edge_gap_respected(&layout.placements, 2);
    }
}

#[test]
fn bfdh_respects_kerf_gap() {
    let (problem, options) = simple_problem(TwoDAlgorithm::BestFitDecreasingHeight, 2);
    let solution = solve_2d(problem, options).expect("solve");
    for layout in &solution.layouts {
        assert_edge_gap_respected(&layout.placements, 2);
    }
}

#[test]
fn guillotine_respects_kerf_gap() {
    let (problem, options) = simple_problem(TwoDAlgorithm::Guillotine, 2);
    let solution = solve_2d(problem, options).expect("solve");
    for layout in &solution.layouts {
        assert_edge_gap_respected(&layout.placements, 2);
    }
}

#[test]
fn guillotine_bssf_respects_kerf_gap() {
    let (problem, options) = simple_problem(TwoDAlgorithm::GuillotineBestShortSideFit, 2);
    let solution = solve_2d(problem, options).expect("solve");
    for layout in &solution.layouts {
        assert_edge_gap_respected(&layout.placements, 2);
    }
}

#[test]
fn guillotine_blsf_respects_kerf_gap() {
    let (problem, options) = simple_problem(TwoDAlgorithm::GuillotineBestLongSideFit, 2);
    let solution = solve_2d(problem, options).expect("solve");
    for layout in &solution.layouts {
        assert_edge_gap_respected(&layout.placements, 2);
    }
}

#[test]
fn guillotine_slas_respects_kerf_gap() {
    let (problem, options) = simple_problem(TwoDAlgorithm::GuillotineShorterLeftoverAxis, 2);
    let solution = solve_2d(problem, options).expect("solve");
    for layout in &solution.layouts {
        assert_edge_gap_respected(&layout.placements, 2);
    }
}

#[test]
fn guillotine_llas_respects_kerf_gap() {
    let (problem, options) = simple_problem(TwoDAlgorithm::GuillotineLongerLeftoverAxis, 2);
    let solution = solve_2d(problem, options).expect("solve");
    for layout in &solution.layouts {
        assert_edge_gap_respected(&layout.placements, 2);
    }
}

#[test]
fn guillotine_min_split_respects_kerf_gap() {
    let (problem, options) = simple_problem(TwoDAlgorithm::GuillotineMinAreaSplit, 2);
    let solution = solve_2d(problem, options).expect("solve");
    for layout in &solution.layouts {
        assert_edge_gap_respected(&layout.placements, 2);
    }
}

#[test]
fn guillotine_max_split_respects_kerf_gap() {
    let (problem, options) = simple_problem(TwoDAlgorithm::GuillotineMaxAreaSplit, 2);
    let solution = solve_2d(problem, options).expect("solve");
    for layout in &solution.layouts {
        assert_edge_gap_respected(&layout.placements, 2);
    }
}

#[test]
fn guillotine_required_with_kerf_still_sets_flag() {
    let problem = TwoDProblem {
        sheets: vec![Sheet2D {
            name: "sheet".to_string(),
            width: 20,
            height: 20,
            cost: 1.0,
            quantity: None,
            kerf: 1,
            edge_kerf_relief: false,
        }],
        demands: vec![RectDemand2D {
            name: "panel".to_string(),
            width: 6,
            height: 6,
            quantity: 4,
            can_rotate: true,
        }],
    };
    let options = TwoDOptions {
        algorithm: TwoDAlgorithm::Auto,
        guillotine_required: true,
        seed: Some(42),
        ..Default::default()
    };
    let solution = solve_2d(problem, options).expect("solve");
    assert!(solution.guillotine);
    for layout in &solution.layouts {
        assert_edge_gap_respected(&layout.placements, 1);
    }
}

#[test]
fn max_rects_respects_kerf_gap() {
    let (problem, options) = simple_problem(TwoDAlgorithm::MaxRects, 2);
    let solution = solve_2d(problem, options).expect("solve");
    for layout in &solution.layouts {
        assert_edge_gap_respected(&layout.placements, 2);
    }
}

#[test]
fn max_rects_bssf_respects_kerf_gap() {
    let (problem, options) = simple_problem(TwoDAlgorithm::MaxRectsBestShortSideFit, 2);
    let solution = solve_2d(problem, options).expect("solve");
    for layout in &solution.layouts {
        assert_edge_gap_respected(&layout.placements, 2);
    }
}

#[test]
fn max_rects_blsf_respects_kerf_gap() {
    let (problem, options) = simple_problem(TwoDAlgorithm::MaxRectsBestLongSideFit, 2);
    let solution = solve_2d(problem, options).expect("solve");
    for layout in &solution.layouts {
        assert_edge_gap_respected(&layout.placements, 2);
    }
}

#[test]
fn max_rects_bottom_left_respects_kerf_gap() {
    let (problem, options) = simple_problem(TwoDAlgorithm::MaxRectsBottomLeft, 2);
    let solution = solve_2d(problem, options).expect("solve");
    for layout in &solution.layouts {
        assert_edge_gap_respected(&layout.placements, 2);
    }
}

#[test]
fn max_rects_contact_point_respects_kerf_gap() {
    let (problem, options) = simple_problem(TwoDAlgorithm::MaxRectsContactPoint, 2);
    let solution = solve_2d(problem, options).expect("solve");
    for layout in &solution.layouts {
        assert_edge_gap_respected(&layout.placements, 2);
    }
}

#[test]
fn multi_start_respects_kerf_gap() {
    let (problem, options) = simple_problem(TwoDAlgorithm::MultiStart, 2);
    let solution = solve_2d(problem, options).expect("solve");
    for layout in &solution.layouts {
        assert_edge_gap_respected(&layout.placements, 2);
    }
}

#[test]
fn skyline_respects_kerf_gap() {
    let (problem, options) = simple_problem(TwoDAlgorithm::Skyline, 2);
    let solution = solve_2d(problem, options).expect("solve");
    for layout in &solution.layouts {
        assert_edge_gap_respected(&layout.placements, 2);
    }
}

#[test]
fn skyline_min_waste_respects_kerf_gap() {
    let (problem, options) = simple_problem(TwoDAlgorithm::SkylineMinWaste, 2);
    let solution = solve_2d(problem, options).expect("solve");
    for layout in &solution.layouts {
        assert_edge_gap_respected(&layout.placements, 2);
    }
}

#[test]
fn total_kerf_area_nonzero_when_kerf_applied_and_multiple_placements() {
    let (problem, options) = simple_problem(TwoDAlgorithm::Guillotine, 2);
    let solution = solve_2d(problem, options).expect("solve");
    assert!(
        solution.total_kerf_area > 0,
        "expected non-zero kerf area, got {}",
        solution.total_kerf_area
    );
    for layout in &solution.layouts {
        assert!(
            layout.kerf_area <= layout.waste_area,
            "kerf_area {} should be <= waste_area {}",
            layout.kerf_area,
            layout.waste_area,
        );
    }
}

#[test]
fn sheet_area_equals_used_plus_waste_for_every_algorithm() {
    for algorithm in [
        TwoDAlgorithm::MaxRects,
        TwoDAlgorithm::MaxRectsBestShortSideFit,
        TwoDAlgorithm::Skyline,
        TwoDAlgorithm::Guillotine,
        TwoDAlgorithm::NextFitDecreasingHeight,
    ] {
        let (problem, options) = simple_problem(algorithm, 2);
        let solution = solve_2d(problem, options).expect("solve");
        for layout in &solution.layouts {
            let sheet_area = u64::from(layout.width) * u64::from(layout.height);
            assert_eq!(
                layout.used_area + layout.waste_area,
                sheet_area,
                "used+waste should equal sheet area for {algorithm:?}",
            );
        }
    }
}

#[test]
fn zero_kerf_reproduces_today_behavior() {
    let problem = TwoDProblem {
        sheets: vec![Sheet2D {
            name: "sheet".to_string(),
            width: 96,
            height: 48,
            cost: 1.0,
            quantity: None,
            kerf: 0,
            edge_kerf_relief: false,
        }],
        demands: vec![RectDemand2D {
            name: "panel".to_string(),
            width: 24,
            height: 18,
            quantity: 4,
            can_rotate: true,
        }],
    };
    let options =
        TwoDOptions { algorithm: TwoDAlgorithm::Auto, seed: Some(42), ..Default::default() };
    let solution = solve_2d(problem, options).expect("solve");
    assert_eq!(solution.total_kerf_area, 0);
    for layout in &solution.layouts {
        assert_eq!(layout.kerf_area, 0);
    }
}

#[test]
fn tight_problem_under_kerf_returns_unplaced_not_error() {
    // A problem that exactly fits without kerf cannot all fit with kerf.
    // It must degrade gracefully to unplaced entries, not an error.
    let problem = TwoDProblem {
        sheets: vec![Sheet2D {
            name: "sheet".to_string(),
            width: 10,
            height: 10,
            cost: 1.0,
            quantity: Some(1),
            kerf: 1,
            edge_kerf_relief: false,
        }],
        demands: vec![RectDemand2D {
            name: "panel".to_string(),
            width: 5,
            height: 10,
            quantity: 2,
            can_rotate: false,
        }],
    };
    // Without kerf, 2 x 5 == 10 fits exactly. With kerf=1, only one panel fits.
    let options =
        TwoDOptions { algorithm: TwoDAlgorithm::Guillotine, seed: Some(1), ..Default::default() };
    let solution = solve_2d(problem, options).expect("solve");
    assert!(!solution.unplaced.is_empty(), "expected at least one unplaced panel under kerf");
}

#[test]
fn kerf_adjacent_to_sheet_edge_ok() {
    // A single placement flush against every sheet edge must be accepted
    // when kerf > 0 (D3: the factory edge is not a cut, no kerf charged
    // against the sheet boundary).
    let problem = TwoDProblem {
        sheets: vec![Sheet2D {
            name: "sheet".to_string(),
            width: 20,
            height: 20,
            cost: 1.0,
            quantity: None,
            kerf: 3,
            edge_kerf_relief: false,
        }],
        demands: vec![RectDemand2D {
            name: "panel".to_string(),
            width: 20,
            height: 20,
            quantity: 1,
            can_rotate: false,
        }],
    };
    for algorithm in [
        TwoDAlgorithm::MaxRects,
        TwoDAlgorithm::Skyline,
        TwoDAlgorithm::Guillotine,
        TwoDAlgorithm::BestFitDecreasingHeight,
    ] {
        let options = TwoDOptions { algorithm, seed: Some(7), ..Default::default() };
        let solution = solve_2d(problem.clone(), options).expect("solve");
        assert!(
            solution.unplaced.is_empty(),
            "{algorithm:?}: a full-sheet placement should not be rejected by kerf",
        );
        assert_eq!(solution.layouts.len(), 1);
        let layout = &solution.layouts[0];
        assert_eq!(layout.placements.len(), 1);
        let p = &layout.placements[0];
        assert_eq!((p.x, p.y, p.width, p.height), (0, 0, 20, 20));
    }
}

#[test]
fn kerf_area_accounting_matches_hand_computed_values() {
    // Hand-constructed layout: two 8×20 panels on a 20×20 sheet with kerf=2.
    // After placing two 8-wide panels, the kerf line between them runs the
    // full 20-unit height at width 2. Expected kerf_area = 2 * 20 = 40.
    // Remaining waste = 20*20 - 2*(8*20) - 0 = 400 - 320 = 80. Of that 80,
    // 40 is kerf and 40 is the 2-wide strip to the right of the second panel.
    let problem = TwoDProblem {
        sheets: vec![Sheet2D {
            name: "sheet".to_string(),
            width: 20,
            height: 20,
            cost: 1.0,
            quantity: None,
            kerf: 2,
            edge_kerf_relief: false,
        }],
        demands: vec![RectDemand2D {
            name: "panel".to_string(),
            width: 8,
            height: 20,
            quantity: 2,
            can_rotate: false,
        }],
    };
    let options =
        TwoDOptions { algorithm: TwoDAlgorithm::Guillotine, seed: Some(7), ..Default::default() };
    let solution = solve_2d(problem, options).expect("solve");
    assert!(solution.unplaced.is_empty());
    assert_eq!(solution.layouts.len(), 1);
    let layout = &solution.layouts[0];
    assert_eq!(layout.placements.len(), 2);
    // Verify the placements are side-by-side with kerf gap.
    let mut xs: Vec<u32> = layout.placements.iter().map(|p| p.x).collect();
    xs.sort();
    assert_eq!(xs[0], 0);
    assert_eq!(xs[1], 10); // 8 (panel) + 2 (kerf) = 10
    // Area accounting.
    assert_eq!(layout.used_area, 320); // 2 * (8 * 20)
    assert_eq!(layout.waste_area, 80); // 400 - 320
    assert_eq!(layout.kerf_area, 40); // 2 * 20 (one vertical kerf line)
    assert!(layout.kerf_area <= layout.waste_area);
}
