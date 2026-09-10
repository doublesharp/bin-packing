#![cfg(feature = "two-d")]

use bin_packing::two_d::saw_plan::{SawJob2D, SawLayout2D, plan_saw_cuts};
use bin_packing::two_d::{
    RectDemand2D, Sheet2D, TwoDAlgorithm, TwoDOptions, TwoDProblem, solve_2d,
};

fn problem(types: usize, width: u32, height: u32) -> TwoDProblem {
    TwoDProblem {
        sheets: vec![Sheet2D {
            name: "stock".into(),
            width,
            height,
            cost: 1.0,
            quantity: Some(types),
            kerf: 0,
            edge_kerf_relief: false,
        }],
        demands: (0..types)
            .map(|i| RectDemand2D {
                name: format!("part_{i}"),
                width: 1,
                height: 2,
                quantity: 1,
                can_rotate: true,
            })
            .collect(),
    }
}

#[test]
fn assigned_rotations_keep_source_dimensions_and_feed_saw_planner() {
    let input = problem(1, 2, 1);
    for threshold in [0, 8] {
        let solution = solve_2d(
            input.clone(),
            TwoDOptions {
                algorithm: TwoDAlgorithm::RotationSearch,
                seed: Some(42),
                multistart_runs: 16,
                auto_rotation_search_max_types: threshold,
                ..Default::default()
            },
        )
        .expect("rotation search");
        assert!(solution.unplaced.is_empty());
        let part = &solution.layouts[0].placements[0];
        assert_eq!((part.width, part.height, part.rotated), (2, 1, true));
        let job = SawJob2D {
            problem: input.clone(),
            layouts: solution.layouts.iter().map(SawLayout2D::from).collect(),
            stock_indices: vec![0],
            demand_indices: vec![vec![0]],
        };
        assert!(plan_saw_cuts(&job, &Default::default()).is_ok());
    }
}

#[test]
fn sampled_rotation_supports_more_than_one_mask_word() {
    for types in [63, 64, 65, 129] {
        let input = problem(types, 100, 100);
        let options = TwoDOptions {
            algorithm: TwoDAlgorithm::RotationSearch,
            multistart_runs: 1,
            seed: Some(42),
            ..Default::default()
        };
        let solution = solve_2d(input.clone(), options.clone()).expect("sampled search");
        assert!(solution.unplaced.is_empty());
        assert_eq!(solution.layouts.iter().map(|s| s.placements.len()).sum::<usize>(), types);
        for part in solution.layouts.iter().flat_map(|s| &s.placements) {
            assert_eq!((part.width, part.height), if part.rotated { (2, 1) } else { (1, 2) });
        }
        assert_eq!(solution, solve_2d(input, options).expect("deterministic repeat"));
    }
}

#[test]
fn oversized_exhaustive_threshold_uses_bounded_sampling() {
    let solution = solve_2d(
        problem(63, 100, 100),
        TwoDOptions {
            algorithm: TwoDAlgorithm::RotationSearch,
            auto_rotation_search_max_types: 63,
            multistart_runs: 1,
            ..Default::default()
        },
    )
    .expect("bounded search");
    assert!(solution.metrics.notes.iter().any(|note| note.contains("sampled")));
}

#[test]
fn unplaced_rotation_assignments_preserve_original_demand() {
    let mut input = problem(1, 1, 2);
    input.sheets[0].quantity = Some(0);
    let solution = solve_2d(
        input.clone(),
        TwoDOptions { algorithm: TwoDAlgorithm::RotationSearch, ..Default::default() },
    )
    .expect("capped stock");
    assert_eq!(solution.unplaced, input.demands);
}
