#![cfg(feature = "two-d")]
use bin_packing::two_d::{
    RectDemand2D, Sheet2D, TwoDAlgorithm, TwoDOptions, TwoDProblem, TwoDSolution, solve_2d,
    solve_2d_demand_aware, solve_2d_mixed_rotations,
};

fn problem(width: u32, height: u32, demands: &[(u32, u32, usize, bool)]) -> TwoDProblem {
    TwoDProblem {
        sheets: vec![Sheet2D {
            name: "sheet".into(),
            width,
            height,
            cost: 1.0,
            quantity: None,
            kerf: 0,
            edge_kerf_relief: false,
        }],
        demands: demands
            .iter()
            .enumerate()
            .map(|(i, &(width, height, quantity, can_rotate))| RectDemand2D {
                name: format!("part-{i}"),
                width,
                height,
                quantity,
                can_rotate,
            })
            .collect(),
    }
}

fn validate(problem: &TwoDProblem, solution: &TwoDSolution) {
    let mut counts = vec![0; problem.demands.len()];
    let mut stock_counts = vec![0; problem.sheets.len()];
    for layout in &solution.layouts {
        let (stock_index, stock) = problem
            .sheets
            .iter()
            .enumerate()
            .find(|(_, s)| s.name == layout.sheet_name)
            .expect("stock identity");
        stock_counts[stock_index] += 1;
        for (i, p) in layout.placements.iter().enumerate() {
            let (index, demand) = problem
                .demands
                .iter()
                .enumerate()
                .find(|(_, d)| d.name == p.name)
                .expect("demand identity");
            counts[index] += 1;
            assert!(!p.rotated || demand.can_rotate);
            assert_eq!(
                (p.width, p.height),
                if p.rotated {
                    (demand.height, demand.width)
                } else {
                    (demand.width, demand.height)
                }
            );
            let pad = if stock.edge_kerf_relief { stock.kerf } else { 0 };
            assert!(u64::from(p.x) + u64::from(p.width) <= u64::from(stock.width) + u64::from(pad));
            assert!(
                u64::from(p.y) + u64::from(p.height) <= u64::from(stock.height) + u64::from(pad)
            );
            for q in &layout.placements[..i] {
                let k = u64::from(stock.kerf);
                assert!(
                    u64::from(p.x) + u64::from(p.width) + k <= u64::from(q.x)
                        || u64::from(q.x) + u64::from(q.width) + k <= u64::from(p.x)
                        || u64::from(p.y) + u64::from(p.height) + k <= u64::from(q.y)
                        || u64::from(q.y) + u64::from(q.height) + k <= u64::from(p.y)
                );
            }
        }
    }
    for demand in &solution.unplaced {
        let index =
            problem.demands.iter().position(|d| d.name == demand.name).expect("unplaced identity");
        counts[index] += demand.quantity;
    }
    assert_eq!(counts, problem.demands.iter().map(|d| d.quantity).collect::<Vec<_>>());
    for (index, stock) in problem.sheets.iter().enumerate() {
        assert!(stock.quantity.is_none_or(|q| stock_counts[index] <= q));
    }
}

#[test]
fn preserving_last_viable_cavity_beats_geometric_tie() {
    let problem = problem(10, 10, &[(8, 6, 1, true), (4, 10, 1, false)]);
    let options =
        TwoDOptions { algorithm: TwoDAlgorithm::MaxRects, seed: Some(7), ..Default::default() };
    let baseline = solve_2d(problem.clone(), options.clone()).expect("geometric baseline");
    let aware = solve_2d_demand_aware(problem.clone(), options.clone()).expect("demand-aware");
    let auto = solve_2d(
        problem.clone(),
        TwoDOptions { algorithm: TwoDAlgorithm::Auto, ..options.clone() },
    )
    .expect("Auto comparison");
    eprintln!(
        "last cavity baseline={} aware={} Auto={} metrics={:?}",
        baseline.sheet_count, aware.sheet_count, auto.sheet_count, aware.metrics
    );
    assert_eq!(baseline.sheet_count, 2);
    assert_eq!(aware.sheet_count, 1);
    assert!(aware.unplaced.is_empty());
    validate(&problem, &aware);
    assert_eq!(aware, solve_2d_demand_aware(problem, options).expect("deterministic"));
}

#[test]
fn mixed_rotation_counts_beat_uniform_type_assignments() {
    let problem = problem(5, 5, &[(3, 2, 4, true)]);
    let options = TwoDOptions {
        algorithm: TwoDAlgorithm::RotationSearch,
        seed: Some(11),
        multistart_runs: 8,
        ..Default::default()
    };
    let uniform = solve_2d(problem.clone(), options.clone()).expect("uniform rotations");
    let mixed =
        solve_2d_mixed_rotations(problem.clone(), options.clone()).expect("mixed rotations");
    let auto = solve_2d(
        problem.clone(),
        TwoDOptions { algorithm: TwoDAlgorithm::Auto, ..options.clone() },
    )
    .expect("Auto comparison");
    eprintln!(
        "pinwheel uniform={} mixed={} Auto={} metrics={:?}",
        uniform.sheet_count, mixed.sheet_count, auto.sheet_count, mixed.metrics
    );
    assert_eq!(uniform.sheet_count, 2);
    assert_eq!(mixed.sheet_count, 1);
    let rotated = mixed.layouts.iter().flat_map(|s| &s.placements).filter(|p| p.rotated).count();
    assert_eq!(rotated, 2);
    validate(&problem, &mixed);
    assert_eq!(mixed, solve_2d_mixed_rotations(problem, options).expect("deterministic"));
}

#[test]
fn experimental_entrypoints_preserve_kerf_rotation_inventory_and_baseline() {
    for relief in [false, true] {
        for kerf in [0, 1] {
            let mut problem = problem(13, 11, &[(6, 4, 3, true), (3, 3, 2, false)]);
            problem.sheets[0].kerf = kerf;
            problem.sheets[0].edge_kerf_relief = relief;
            problem.sheets[0].quantity = Some(2);
            let options = TwoDOptions {
                algorithm: TwoDAlgorithm::MaxRects,
                seed: Some(17),
                multistart_runs: 4,
                ..Default::default()
            };
            let baseline = solve_2d(problem.clone(), options.clone()).expect("baseline");
            for solver in [solve_2d_demand_aware, solve_2d_mixed_rotations] {
                let selected = solver(problem.clone(), options.clone()).expect("experiment");
                assert!(
                    (
                        selected.unplaced.len(),
                        selected.sheet_count,
                        selected.total_waste_area,
                        selected.total_cost
                    ) <= (
                        baseline.unplaced.len(),
                        baseline.sheet_count,
                        baseline.total_waste_area,
                        baseline.total_cost
                    )
                );
                validate(&problem, &selected);
            }
        }
    }
}

#[test]
fn seeded_count_and_order_search_improves_sampled_auto_but_not_full_auto() {
    let p = problem(
        24,
        20,
        &[
            (12, 10, 1, true),
            (9, 11, 1, true),
            (6, 12, 1, true),
            (3, 13, 1, true),
            (13, 3, 1, true),
            (10, 4, 1, true),
        ],
    );
    let options = TwoDOptions {
        algorithm: TwoDAlgorithm::Auto,
        seed: Some(17),
        multistart_runs: 2,
        auto_rotation_search_max_types: 2,
        ..Default::default()
    };
    let before = std::time::Instant::now();
    let baseline = solve_2d(p.clone(), options.clone()).expect("sampled Auto");
    let baseline_elapsed = before.elapsed();
    let before = std::time::Instant::now();
    let selected =
        solve_2d_mixed_rotations(p.clone(), options.clone()).expect("seeded mixed trial");
    let mixed_elapsed = before.elapsed();
    let full = solve_2d(
        p.clone(),
        TwoDOptions { multistart_runs: 12, auto_rotation_search_max_types: 16, ..options.clone() },
    )
    .expect("full Auto");
    eprintln!(
        "broader seeded fixture sampled_Auto={} mixed={} full_Auto={} baseline_elapsed={baseline_elapsed:?} mixed_elapsed={mixed_elapsed:?} metrics={:?}",
        baseline.sheet_count, selected.sheet_count, full.sheet_count, selected.metrics
    );
    assert_eq!(baseline.sheet_count, 2);
    assert_eq!(selected.sheet_count, 1);
    assert_eq!(full.sheet_count, 1);
    validate(&p, &selected);
    assert_eq!(
        selected,
        solve_2d_mixed_rotations(p, options).expect("deterministic sampled counts")
    );
}

#[test]
fn bounds_duplicate_source_names_and_guillotine_constraints_are_explicit() {
    for solver in [solve_2d_demand_aware, solve_2d_mixed_rotations] {
        let mut p = problem(8, 10, &[(8, 10, 1, false), (8, 10, 1, false)]);
        p.sheets[0].quantity = Some(1);
        p.sheets.push(p.sheets[0].clone());
        p.demands[1].name = p.demands[0].name.clone();
        let options =
            TwoDOptions { algorithm: TwoDAlgorithm::MaxRects, seed: Some(3), ..Default::default() };
        let selected =
            solver(p.clone(), options.clone()).expect("source indices survive duplicate labels");
        assert_eq!(selected.sheet_count, 2);
        assert!(selected.unplaced.is_empty());
        assert_eq!(selected.layouts.iter().map(|s| s.placements.len()).sum::<usize>(), 2);
        p.demands[0].quantity = 129;
        assert!(matches!(
            solver(p, options.clone()),
            Err(bin_packing::BinPackingError::Unsupported(_))
        ));
        let p = problem(10, 10, &[(4, 3, 4, true)]);
        let selected = solver(
            p.clone(),
            TwoDOptions {
                algorithm: TwoDAlgorithm::Auto,
                guillotine_required: true,
                ..options.clone()
            },
        )
        .expect("guillotine baseline");
        assert!(selected.guillotine);
        validate(&p, &selected);
        let mut invalid = p;
        invalid.demands[0].width = 0;
        assert!(matches!(
            solver(invalid, options),
            Err(bin_packing::BinPackingError::InvalidInput(_))
        ));
    }
}
