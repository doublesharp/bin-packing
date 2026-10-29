#![cfg(feature = "one-d")]

use std::collections::BTreeMap;

use bin_packing::one_d::{
    CutDemand1D, OneDAlgorithm, OneDOptions, OneDProblem, OneDSolution, Stock1D, solve_1d,
    solve_1d_arc_flow, solve_1d_min_cost,
};

fn stock(name: &str, length: u32, available: Option<usize>) -> Stock1D {
    Stock1D { name: name.into(), length, kerf: 0, trim: 0, cost: 1.0, available }
}

fn demand(name: &str, length: u32, quantity: usize) -> CutDemand1D {
    CutDemand1D { name: name.into(), length, quantity }
}

#[test]
fn min_cost_can_choose_more_bars_to_reduce_material_cost() {
    let mut expensive = stock("large", 10, None);
    expensive.cost = 10.0;
    let problem = OneDProblem {
        stock: vec![expensive, stock("small", 5, None)],
        demands: vec![demand("part", 5, 2)],
    };
    let count_first =
        solve_1d_arc_flow(problem.clone(), OneDOptions::default()).expect("count-first search");
    assert_eq!((count_first.stock_count, count_first.total_cost), (1, 10.0));
    let cost_first =
        solve_1d_min_cost(problem.clone(), OneDOptions::default()).expect("cost-first search");
    check(&problem, &cost_first);
    assert_eq!((cost_first.stock_count, cost_first.total_cost), (2, 2.0));
    assert!(cost_first.exact);
}

fn check(problem: &OneDProblem, solution: &OneDSolution) {
    let mut counts = BTreeMap::new();
    for cut in solution.layouts.iter().flat_map(|layout| &layout.cuts).chain(&solution.unplaced) {
        *counts.entry((cut.name.as_str(), cut.length)).or_insert(0_usize) += 1;
    }
    let mut expected = BTreeMap::new();
    for cut in &problem.demands {
        *expected.entry((cut.name.as_str(), cut.length)).or_insert(0_usize) += cut.quantity;
    }
    assert_eq!(counts, expected);
    let mut waste = 0_u64;
    let mut cost = 0.0;
    for source in &problem.stock {
        let layouts: Vec<_> =
            solution.layouts.iter().filter(|l| l.stock_name == source.name).collect();
        assert!(source.available.is_none_or(|cap| layouts.len() <= cap));
        for layout in layouts {
            assert!(!layout.cuts.is_empty());
            let used = layout.cuts.iter().map(|cut| u64::from(cut.length)).sum::<u64>()
                + u64::from(source.kerf) * (layout.cuts.len() - 1) as u64;
            assert!(used + u64::from(source.trim) <= u64::from(source.length));
            assert_eq!(u64::from(layout.used_length), used);
            assert_eq!(
                u64::from(layout.remaining_length),
                u64::from(source.length - source.trim) - used
            );
            assert_eq!(layout.waste, layout.remaining_length);
            waste += u64::from(layout.waste);
            cost += source.cost;
        }
    }
    assert_eq!(solution.stock_count, solution.layouts.len());
    assert_eq!(solution.total_waste, waste);
    assert!((solution.total_cost - cost).abs() <= f64::EPSILON * cost.max(1.0) * 4.0);
}

#[test]
fn arc_flow_completes_multi_stock_integer_search() {
    let problem = OneDProblem {
        stock: vec![
            stock("eight", 8, Some(2)),
            stock("nine", 9, Some(1)),
            stock("seven", 7, Some(1)),
        ],
        demands: vec![demand("a", 2, 3), demand("b", 3, 2), demand("c", 4, 1)],
    };
    let result = solve_1d_arc_flow(problem.clone(), OneDOptions::default()).expect("solve");
    check(&problem, &result);
    assert_eq!((result.unplaced.len(), result.stock_count, result.total_waste), (0, 2, 0));
    assert!(result.exact, "complete small multi-stock search must certify its optimum");
}

#[test]
fn arc_flow_budget_exhaustion_keeps_incumbent_and_procurement() {
    let problem = OneDProblem {
        stock: vec![stock("limited", 10, Some(1))],
        demands: vec![demand("cut", 5, 3)],
    };
    let result = solve_1d_arc_flow(
        problem.clone(),
        OneDOptions { exact_pattern_limit: 0, ..OneDOptions::default() },
    )
    .expect("solve");
    check(&problem, &result);
    assert!(!result.exact);
    assert_eq!(result.stock_count, 1);
    assert_eq!(result.unplaced.len(), 1);
    assert_eq!(result.stock_requirements[0].required_quantity, 2);
    assert_eq!(result.stock_requirements[0].additional_quantity_needed, 1);
    assert!(result.metrics.notes.iter().any(|note| note.contains("budget")));
}

#[test]
fn arc_flow_preserves_duplicate_names_kerf_trim_and_cost() {
    let mut cheap = stock("cheap", 15, Some(2));
    cheap.kerf = 1;
    cheap.trim = 2;
    let mut costly = cheap.clone();
    costly.name = "costly".into();
    costly.cost = 3.0;
    let problem = OneDProblem {
        stock: vec![costly, cheap],
        demands: vec![demand("same", 6, 2), demand("same", 3, 1), demand("same", 2, 2)],
    };
    let result = solve_1d_arc_flow(problem.clone(), OneDOptions::default()).expect("solve");
    check(&problem, &result);
    assert!(result.exact);
    assert!(result.unplaced.is_empty());
    assert_eq!(result.stock_count, 2);
    assert_eq!(result.total_cost, 2.0);
}

// Independent oracle assigns individual pieces to bars, without pattern enumeration or a DAG.
fn oracle(problem: &OneDProblem) -> (usize, usize, u64, f64) {
    oracle_with_objective(problem, false)
}

fn oracle_with_objective(problem: &OneDProblem, cost_first: bool) -> (usize, usize, u64, f64) {
    fn visit(
        problem: &OneDProblem,
        pieces: &[u32],
        bars: &mut Vec<(usize, u32)>,
        usage: &mut [usize],
        missing: usize,
        best: &mut (usize, usize, u64, f64),
        cost_first: bool,
    ) {
        let Some((&piece, tail)) = pieces.split_first() else {
            let waste: u64 = bars
                .iter()
                .map(|&(s, used)| u64::from(problem.stock[s].length - problem.stock[s].trim - used))
                .sum();
            let mut ordered = bars.clone();
            ordered.sort_by(|&(a, used_a), &(b, used_b)| {
                used_b.cmp(&used_a).then_with(|| problem.stock[a].name.cmp(&problem.stock[b].name))
            });
            let cost: f64 = ordered.iter().map(|&(s, _)| problem.stock[s].cost).sum();
            let prefix = (missing, bars.len(), waste);
            let best_prefix = (best.0, best.1, best.2);
            let improves = if cost_first {
                missing
                    .cmp(&best.0)
                    .then_with(|| cost.total_cmp(&best.3))
                    .then_with(|| bars.len().cmp(&best.1))
                    .then_with(|| waste.cmp(&best.2))
                    .is_lt()
            } else {
                prefix < best_prefix || (prefix == best_prefix && cost.total_cmp(&best.3).is_lt())
            };
            if improves {
                *best = (missing, bars.len(), waste, cost);
            }
            return;
        };
        for (index, &(s, used)) in bars.iter().enumerate() {
            let source = &problem.stock[s];
            let next = used + source.kerf + piece;
            if next <= source.length - source.trim {
                let mut next_bars = bars.clone();
                next_bars[index].1 = next;
                visit(problem, tail, &mut next_bars, usage, missing, best, cost_first);
            }
        }
        for (s, source) in problem.stock.iter().enumerate() {
            if piece <= source.length - source.trim
                && source.available.is_none_or(|cap| usage[s] < cap)
            {
                bars.push((s, piece));
                usage[s] += 1;
                visit(problem, tail, bars, usage, missing, best, cost_first);
                usage[s] -= 1;
                assert!(bars.pop().is_some());
            }
        }
        visit(problem, tail, bars, usage, missing + 1, best, cost_first);
    }
    let pieces: Vec<_> =
        problem.demands.iter().flat_map(|d| std::iter::repeat_n(d.length, d.quantity)).collect();
    let mut best = (usize::MAX, usize::MAX, u64::MAX, f64::INFINITY);
    visit(
        problem,
        &pieces,
        &mut Vec::new(),
        &mut vec![0; problem.stock.len()],
        0,
        &mut best,
        cost_first,
    );
    best
}

#[test]
fn min_cost_matches_tiny_per_piece_oracle() {
    for seed in 0..48_u32 {
        let mut first = stock("first", 7 + seed % 3, Some((seed % 3) as usize));
        first.kerf = seed % 2;
        first.trim = (seed / 2) % 2;
        first.cost = if seed % 7 == 0 {
            0.0
        } else if seed % 2 == 0 {
            3.0
        } else {
            0.1
        };
        let mut second = stock("second", 5 + seed % 4, Some(((seed / 3) % 3) as usize));
        second.kerf = (seed / 4) % 2;
        second.cost = if seed % 2 == 0 { 1.0 } else { 0.2 };
        let problem = OneDProblem {
            stock: vec![first, second],
            demands: vec![demand("same", 1 + seed % 4, 2), demand("same", 3, 2)],
        };
        let result = solve_1d_min_cost(
            problem.clone(),
            OneDOptions { multistart_runs: 1, ..OneDOptions::default() },
        )
        .expect("cost-first solve");
        check(&problem, &result);
        assert!(result.exact, "seed {seed}: {:?}", result.metrics);
        assert_eq!(
            (result.unplaced.len(), result.stock_count, result.total_waste, result.total_cost),
            oracle_with_objective(&problem, true),
            "seed {seed}"
        );
    }
}

#[test]
fn min_cost_zero_budget_keeps_incumbent_without_exactness_claim() {
    let mut costly = stock("large", 10, None);
    costly.cost = 10.0;
    let problem = OneDProblem {
        stock: vec![costly, stock("small", 5, None)],
        demands: vec![demand("part", 5, 2)],
    };
    let result = solve_1d_min_cost(
        problem.clone(),
        OneDOptions { exact_pattern_limit: 0, ..OneDOptions::default() },
    )
    .expect("retained incumbent");
    check(&problem, &result);
    assert!(!result.exact);
    assert_eq!((result.stock_count, result.total_cost), (1, 10.0));
    assert_eq!(result.algorithm, "arc_flow_min_cost");
}

#[test]
fn min_cost_rejects_unrepresentable_aggregate_material_cost() {
    let mut expensive = stock("expensive", 10, None);
    expensive.cost = f64::MAX;
    let problem = OneDProblem { stock: vec![expensive], demands: vec![demand("cut", 6, 2)] };
    assert!(matches!(
        solve_1d_min_cost(problem, OneDOptions::default()),
        Err(bin_packing::BinPackingError::InvalidConstraint { .. })
    ));
}

#[test]
fn arc_flow_matches_tiny_per_piece_oracle() {
    for seed in 0..96_u32 {
        let mut a = stock("a", 7 + seed % 4, Some((seed % 3) as usize));
        a.kerf = seed % 2;
        a.trim = (seed / 2) % 2;
        a.cost = if seed < 48 { 3.0 } else { 0.1 };
        let mut b = stock("b", 8 + seed % 3, Some(((seed / 3) % 3) as usize));
        b.kerf = (seed / 4) % 2;
        b.trim = (seed / 5) % 2;
        b.cost = if seed < 48 { 1.0 } else { 0.2 };
        let problem = OneDProblem {
            stock: vec![a, b],
            demands: vec![
                demand("same", 1 + seed % 5, 2),
                demand("same", 2 + (seed / 3) % 4, 2),
                demand("last", 3, 1),
            ],
        };
        let result = solve_1d_arc_flow(
            problem.clone(),
            OneDOptions { multistart_runs: 1, ..OneDOptions::default() },
        )
        .expect("solve");
        check(&problem, &result);
        assert!(result.exact, "seed {seed}: {:?}", result.metrics);
        assert_eq!(
            (result.unplaced.len(), result.stock_count, result.total_waste, result.total_cost),
            oracle(&problem),
            "seed {seed}"
        );
    }
}

#[test]
fn arc_flow_compresses_repeated_demands_without_materializing_paths() {
    let problem = OneDProblem {
        stock: vec![stock("bar", 20, None)],
        demands: (0..16).map(|index| demand(&format!("d{index}"), 1, 20)).collect(),
    };
    let options = OneDOptions {
        exact_pattern_limit: 10_000,
        column_generation_rounds: 1,
        multistart_runs: 1,
        improvement_rounds: 0,
        ..OneDOptions::default()
    };
    let result = solve_1d_arc_flow(problem.clone(), options).expect("solve");
    check(&problem, &result);
    assert_eq!(result.stock_count, 16);
    assert!(!result.exact);
    assert_eq!(metric(&result, "represented_patterns"), 7_307_872_109);
    assert_eq!(metric(&result, "graph_nodes"), 317);
    assert_eq!(metric(&result, "graph_arcs"), 3486);
    assert_eq!(metric(&result, "search_work"), 10_000);
    println!("compression fixture {:?}", result.metrics);
    assert!(result.metrics.notes.iter().any(|note| note.contains("represented_patterns=")));
    assert!(result.metrics.notes.iter().any(|note| note.contains("graph_nodes=")));
}

#[test]
fn arc_flow_adversarial_projected_greedy_fixture() {
    let problem = OneDProblem {
        stock: vec![
            stock("ten", 10, Some(2)),
            stock("eight", 8, Some(1)),
            stock("seven", 7, Some(1)),
        ],
        demands: vec![
            demand("six", 6, 1),
            demand("five", 5, 1),
            demand("three", 3, 1),
            demand("two", 2, 3),
        ],
    };
    let options = OneDOptions {
        algorithm: OneDAlgorithm::BestFitDecreasing,
        seed: Some(7),
        ..OneDOptions::default()
    };
    let greedy = solve_1d(problem.clone(), options.clone()).expect("greedy");
    let result = solve_1d_arc_flow(problem.clone(), options).expect("solve");
    check(&problem, &result);
    assert_eq!(greedy.stock_count, 3);
    assert_eq!(result.stock_count, 2);
    assert_eq!(result.total_waste, 0);
    assert!(result.exact);
    println!(
        "quality fixture greedy={:?}; arc={:?}; metrics={:?}",
        (greedy.stock_count, greedy.total_waste, greedy.total_cost),
        (result.stock_count, result.total_waste, result.total_cost),
        result.metrics
    );
}

fn auto_counterexample() -> OneDProblem {
    let mut a = stock("s0", 17, Some(3));
    a.kerf = 1;
    let mut b = stock("s1", 21, Some(3));
    b.kerf = 1;
    let mut c = stock("s2", 16, Some(1));
    c.cost = 2.0;
    OneDProblem {
        stock: vec![a, b, c],
        demands: vec![
            demand("d0", 11, 3),
            demand("d1", 7, 2),
            demand("d2", 9, 2),
            demand("d3", 9, 1),
        ],
    }
}

#[test]
fn arc_flow_improves_auto_incumbent() {
    let problem = auto_counterexample();
    let options = OneDOptions { seed: Some(7), ..OneDOptions::default() };
    let baseline = solve_1d(problem.clone(), options.clone()).expect("auto");
    let result = solve_1d_arc_flow(problem.clone(), options).expect("arc-flow");
    check(&problem, &result);
    assert_eq!((baseline.stock_count, baseline.total_waste, baseline.total_cost), (5, 11, 6.0));
    assert_eq!((result.stock_count, result.total_waste, result.total_cost), (4, 2, 4.0));
    assert!(result.exact);
    println!("Auto counterexample {:?}", result.metrics);
}

fn metric(solution: &OneDSolution, key: &str) -> u128 {
    solution
        .metrics
        .notes
        .iter()
        .flat_map(|note| note.split_whitespace())
        .find_map(|word| word.strip_prefix(&format!("{key}=")).and_then(|value| value.parse().ok()))
        .expect("documented metric")
}

#[test]
fn arc_flow_graph_budget_stops_before_search() {
    let problem = auto_counterexample();
    let options = OneDOptions { seed: Some(7), exact_pattern_limit: 1, ..OneDOptions::default() };
    let baseline = solve_1d(problem.clone(), options.clone()).expect("auto");
    let result = solve_1d_arc_flow(problem.clone(), options).expect("solve");
    check(&problem, &result);
    assert_eq!(result.layouts, baseline.layouts);
    assert_eq!(result.stock_requirements, baseline.stock_requirements);
    assert!(!result.exact);
    assert_eq!(result.metrics.explored_states, 0);
    assert!(result.metrics.notes.iter().any(|note| note.contains("graph budget")));
}

#[test]
fn arc_flow_depth_guard_returns_valid_incumbent() {
    let problem = OneDProblem {
        stock: vec![stock("bar", 1, Some(129))],
        demands: vec![demand("cut", 1, 129)],
    };
    let options =
        OneDOptions { multistart_runs: 1, improvement_rounds: 0, ..OneDOptions::default() };
    let result = solve_1d_arc_flow(problem.clone(), options).expect("solve");
    check(&problem, &result);
    assert_eq!(result.stock_count, 129);
    assert!(!result.exact);
    assert!(result.metrics.notes.iter().any(|note| note.contains("depth limit")));
}

#[test]
fn arc_flow_rejects_cardinality_before_incumbent_expansion() {
    let problems = [
        OneDProblem {
            stock: vec![stock("bar", 1, None)],
            demands: vec![demand("cut", 1, u32::MAX as usize)],
        },
        OneDProblem {
            stock: vec![stock("bar", 4097, None)],
            demands: vec![demand("cut", 1, 4097)],
        },
        OneDProblem {
            stock: vec![stock("bar", 129, None)],
            demands: (0..129).map(|i| demand(&format!("d{i}"), 1, 1)).collect(),
        },
        OneDProblem {
            stock: (0..129).map(|i| stock(&format!("s{i}"), 1, None)).collect(),
            demands: vec![demand("cut", 1, 1)],
        },
    ];
    for problem in problems {
        assert!(matches!(
            solve_1d_arc_flow(
                problem,
                OneDOptions {
                    exact_pattern_limit: 0,
                    multistart_runs: 1,
                    improvement_rounds: 0,
                    ..OneDOptions::default()
                }
            ),
            Err(bin_packing::BinPackingError::Unsupported(_))
        ));
    }
}

#[test]
fn arc_flow_nonintegral_costs_do_not_merge_numerically_distinct_prefixes() {
    let mut a = stock("a", 6, Some(2));
    a.cost = 0.1;
    let mut b = stock("b", 6, Some(2));
    b.cost = 0.2;
    let problem = OneDProblem { stock: vec![b, a], demands: vec![demand("cut", 3, 3)] };
    let result = solve_1d_arc_flow(problem.clone(), OneDOptions::default()).expect("solve");
    check(&problem, &result);
    assert_eq!(result.total_cost, 0.2);
    assert!(result.exact);
    assert_eq!(
        metric(&result, "memo_states"),
        0,
        "fractional cost summation may depend on layout order"
    );
}

#[test]
fn arc_flow_rejects_invalid_input_and_handles_large_dimensions() {
    let empty = OneDProblem { stock: Vec::new(), demands: vec![demand("x", 1, 1)] };
    assert!(matches!(
        solve_1d_arc_flow(empty, OneDOptions::default()),
        Err(bin_packing::BinPackingError::InvalidInput(_))
    ));
    let oversized =
        OneDProblem { stock: vec![stock("bar", 10, None)], demands: vec![demand("x", 11, 1)] };
    assert!(matches!(
        solve_1d_arc_flow(oversized, OneDOptions::default()),
        Err(bin_packing::BinPackingError::Infeasible1D { .. })
    ));
    let mut large = stock("bar", 1 << 30, None);
    large.kerf = 1 << 30;
    let problem = OneDProblem { stock: vec![large], demands: vec![demand("x", 1 << 30, 2)] };
    let result = solve_1d_arc_flow(problem.clone(), OneDOptions::default()).expect("solve");
    check(&problem, &result);
    assert!(result.exact);
    assert_eq!(result.stock_count, 2);
}

#[test]
#[ignore = "run explicitly for uncontended benchmark measurements"]
fn arc_flow_benchmark_fixtures() {
    let fixtures = [
        ("mixed_inventory", auto_counterexample()),
        (
            "repeated_demands",
            OneDProblem {
                stock: vec![stock("bar", 20, None)],
                demands: (0..16).map(|index| demand(&format!("d{index}"), 1, 20)).collect(),
            },
        ),
    ];
    for (name, problem) in fixtures {
        let options = OneDOptions {
            seed: Some(7),
            exact_pattern_limit: 10_000,
            column_generation_rounds: 4,
            ..OneDOptions::default()
        };
        let start = std::time::Instant::now();
        let baseline = solve_1d(problem.clone(), options.clone()).expect("auto");
        let baseline_time = start.elapsed();
        let start = std::time::Instant::now();
        let result = solve_1d_arc_flow(problem.clone(), options).expect("arc-flow");
        let elapsed = start.elapsed();
        check(&problem, &result);
        println!(
            "fixture={name} auto_us={} arc_flow_us={} auto_bars={} arc_bars={} auto_waste={} arc_waste={} auto_cost={} arc_cost={} exact={} metrics={:?}",
            baseline_time.as_micros(),
            elapsed.as_micros(),
            baseline.stock_count,
            result.stock_count,
            baseline.total_waste,
            result.total_waste,
            baseline.total_cost,
            result.total_cost,
            result.exact,
            result.metrics
        );
    }
}

#[test]
fn zero_graph_budget_preserves_complete_auto_incumbent_across_seeds() {
    let mut first = stock("first", 10, Some(2));
    first.kerf = 1;
    first.trim = 1;
    first.cost = 0.1;
    let mut second = stock("second", 8, Some(1));
    second.kerf = 1;
    second.cost = 0.2;
    let problem = OneDProblem {
        stock: vec![first, second],
        demands: vec![demand("a", 4, 3), demand("b", 3, 2), demand("c", 1, 8)],
    };
    for seed in [0, 7, 17] {
        let options = OneDOptions {
            algorithm: OneDAlgorithm::Auto,
            seed: Some(seed),
            exact_pattern_limit: 0,
            multistart_runs: 2,
            ..Default::default()
        };
        let baseline = solve_1d(problem.clone(), options.clone()).expect("Auto incumbent");
        let bounded = solve_1d_arc_flow(problem.clone(), options).expect("bounded fallback");
        check(&problem, &bounded);
        assert!(!bounded.exact);
        assert_eq!(bounded.layouts, baseline.layouts);
        assert_eq!(bounded.unplaced, baseline.unplaced);
        assert_eq!(bounded.stock_requirements, baseline.stock_requirements);
        assert_eq!(bounded.stock_count, baseline.stock_count);
        assert_eq!(bounded.total_waste, baseline.total_waste);
        assert_eq!(bounded.total_cost.to_bits(), baseline.total_cost.to_bits());
    }
}
