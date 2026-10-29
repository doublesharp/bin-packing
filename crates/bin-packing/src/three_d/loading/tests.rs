use super::*;
use crate::BinPackingError;
use crate::three_d::{Bin3D, BoxDemand3D, RotationMask3D, ThreeDProblem};

fn demand(width: u32, height: u32, depth: u32, quantity: usize) -> BoxDemand3D {
    BoxDemand3D {
        name: "item".into(),
        width,
        height,
        depth,
        quantity,
        allowed_rotations: RotationMask3D::UPRIGHT,
    }
}

fn problem(width: u32, height: u32, depth: u32, demands: Vec<BoxDemand3D>) -> LoadingProblem3D {
    LoadingProblem3D {
        packing: ConstrainedProblem3D {
            problem: ThreeDProblem {
                bins: vec![Bin3D {
                    name: "bin".into(),
                    width,
                    height,
                    depth,
                    cost: 1.0,
                    quantity: None,
                }],
                demands,
            },
            item_weights: Vec::new(),
            bin_max_weights: Vec::new(),
        },
        item_rules: Vec::new(),
        bin_rules: Vec::new(),
        unloading: UnloadingConstraint3D::None,
    }
}

fn nesting_problem() -> LoadingProblem3D {
    let mut problem = problem(5, 10, 5, vec![demand(5, 6, 5, 2)]);
    problem.packing.item_weights = vec![3];
    problem.packing.bin_max_weights = vec![Some(10)];
    problem.item_rules = vec![StackItemRules3D { nesting_height: 2, ..Default::default() }];
    problem
}

fn solve(problem: LoadingProblem3D) -> LoadingSolution3D {
    solve_loading_3d(problem, LoadingOptions3D::default()).expect("valid loading problem")
}

fn invalid(problem: &LoadingProblem3D, result: &LoadingSolution3D) {
    assert!(matches!(
        validate_loading_3d(problem, result),
        Err(BinPackingError::InvalidSolution { .. })
    ));
}

fn axles(axis: FloorAxis3D, extent: u32, maximum: u64) -> AxleLimits3D {
    AxleLimits3D {
        axis,
        front_position: 0,
        rear_position: extent,
        front_maximum: maximum,
        rear_maximum: maximum,
        front_tare: 0,
        rear_tare: 0,
    }
}

#[test]
fn loading_nesting_height_saves_a_bin() {
    let problem = nesting_problem();
    let result = solve(problem.clone());
    assert_eq!(result.bin_count, 1);
    assert!(result.unplaced.is_empty());
    assert_eq!(result.layouts[0].stacks[0].height, 10);
    assert_eq!(result.layouts[0].stacks[0].items[1].y, 4);
    assert!(validate_loading_3d(&problem, &result).is_ok());
    let mut ordinary = problem;
    ordinary.item_rules.clear();
    assert_eq!(solve(ordinary).bin_count, 2);
}

#[test]
fn loading_weight_above_reorders_fragile_items_and_checks_every_level() {
    let mut problem = problem(1, 3, 1, vec![demand(1, 1, 1, 1); 3]);
    problem.packing.item_weights = vec![1, 4, 2];
    problem.item_rules = vec![
        StackItemRules3D { maximum_weight_above: Some(0), ..Default::default() },
        StackItemRules3D::default(),
        StackItemRules3D { maximum_weight_above: Some(1), ..Default::default() },
    ];
    let result = solve(problem.clone());
    assert_eq!(result.bin_count, 1);
    let items = &result.layouts[0].stacks[0].items;
    assert_eq!(items.iter().map(|item| item.demand_index).collect::<Vec<_>>(), [1, 2, 0]);
    let mut tampered = result;
    tampered.layouts[0].stacks[0].items[1].demand_index = 0;
    tampered.layouts[0].stacks[0].items[2].demand_index = 2;
    invalid(&problem, &tampered);
}

#[test]
fn loading_stackability_identifier_and_every_item_count_limit_apply() {
    let mut problem = problem(1, 3, 1, vec![demand(1, 1, 1, 1); 3]);
    problem.item_rules = vec![
        StackItemRules3D::default(),
        StackItemRules3D { stackability_id: 1, ..Default::default() },
        StackItemRules3D { maximum_stackability: Some(1), ..Default::default() },
    ];
    assert_eq!(solve(problem.clone()).bin_count, 3);
    problem.item_rules[1].stackability_id = 0;
    problem.item_rules[2].maximum_stackability = Some(2);
    let result = solve(problem.clone());
    assert_eq!(result.bin_count, 2);
    assert!(result.unplaced.is_empty());
    let contained = result
        .layouts
        .iter()
        .flat_map(|layout| &layout.stacks)
        .find(|stack| stack.items.len() > 1)
        .expect("a stacked pair")
        .items
        .last()
        .expect("upper item")
        .demand_index;
    problem.item_rules[contained].maximum_stackability = Some(1);
    invalid(&problem, &result);
    problem.item_rules[contained].maximum_stackability = None;
    problem.item_rules[contained].stackability_id = 99;
    invalid(&problem, &result);
}

#[test]
fn loading_exact_fractional_density_limits_whole_stack() {
    let mut problem = problem(2, 2, 2, vec![demand(2, 1, 2, 2)]);
    problem.packing.item_weights = vec![2];
    assert_eq!(solve(problem.clone()).bin_count, 1);
    problem.bin_rules = vec![LoadingBinRules3D {
        maximum_stack_density: Some(WeightDensity3D { numerator: 3, denominator: 5 }),
        ..Default::default()
    }];
    assert_eq!(solve(problem).bin_count, 2);
}

#[test]
fn loading_axles_find_balanced_and_asymmetric_tare_positions() {
    let mut problem = problem(10, 1, 2, vec![demand(2, 1, 2, 1)]);
    problem.packing.item_weights = vec![100];
    problem.bin_rules = vec![LoadingBinRules3D {
        axle_limits: Some(axles(FloorAxis3D::X, 10, 50)),
        ..Default::default()
    }];
    let result = solve(problem.clone());
    assert_eq!(result.layouts[0].stacks[0].x, 4);
    assert_eq!(result.layouts[0].axle_loads, Some(AxleLoads3D { front: 50, rear: 50 }));
    let mut unbalanced = result;
    unbalanced.layouts[0].stacks[0].x = 0;
    unbalanced.layouts[0].axle_loads = Some(AxleLoads3D { front: 90, rear: 10 });
    invalid(&problem, &unbalanced);
    problem.bin_rules[0].axle_limits = Some(AxleLimits3D {
        front_maximum: 70,
        rear_maximum: 40,
        front_tare: 10,
        ..axles(FloorAxis3D::X, 10, 50)
    });
    let result = solve(problem);
    assert_eq!(result.layouts[0].stacks[0].x, 3);
    assert_eq!(result.layouts[0].axle_loads, Some(AxleLoads3D { front: 70, rear: 40 }));
}

#[test]
fn loading_axle_reactions_use_conservative_integer_ceilings() {
    let mut problem = problem(1, 1, 3, vec![demand(1, 1, 1, 1)]);
    problem.packing.item_weights = vec![1];
    problem.bin_rules = vec![LoadingBinRules3D {
        axle_limits: Some(axles(FloorAxis3D::Z, 3, 1)),
        ..Default::default()
    }];
    let result = solve(problem.clone());
    assert_eq!(result.layouts[0].axle_loads, Some(AxleLoads3D { front: 1, rear: 1 }));
    let mut rounded_down = result;
    rounded_down.layouts[0].axle_loads = Some(AxleLoads3D { front: 0, rear: 0 });
    invalid(&problem, &rounded_down);
}

#[test]
fn loading_unloading_groups_respect_lanes_and_vertical_removal() {
    for mode in [UnloadingConstraint3D::OnlyXMovements, UnloadingConstraint3D::OnlyZMovements] {
        let (width, depth) =
            if mode == UnloadingConstraint3D::OnlyXMovements { (2, 1) } else { (1, 2) };
        let mut problem = problem(width, 1, depth, vec![demand(1, 1, 1, 1); 2]);
        problem.item_rules = vec![
            StackItemRules3D::default(),
            StackItemRules3D { group_id: 1, ..Default::default() },
        ];
        problem.unloading = mode;
        let result = solve(problem.clone());
        assert_eq!(result.bin_count, 1);
        let mut blocked = result;
        for stack in &mut blocked.layouts[0].stacks {
            if mode == UnloadingConstraint3D::OnlyXMovements {
                stack.x = 1 - stack.x;
            } else {
                stack.z = 1 - stack.z;
            }
        }
        invalid(&problem, &blocked);
        problem.packing.problem.bins[0].width = 1;
        problem.packing.problem.bins[0].depth = 1;
        problem.packing.problem.bins[0].height = 2;
        let mut vertical = solve(problem.clone());
        assert_eq!(vertical.bin_count, 1);
        assert_eq!(vertical.layouts[0].stacks[0].items[0].demand_index, 1);
        vertical.layouts[0].stacks[0].items[0].demand_index = 0;
        vertical.layouts[0].stacks[0].items[1].demand_index = 1;
        invalid(&problem, &vertical);
    }
}

#[test]
fn loading_increasing_modes_compare_origins_even_across_separate_lanes() {
    let mut problem = problem(2, 1, 2, vec![demand(1, 1, 1, 1); 2]);
    problem.item_rules =
        vec![StackItemRules3D::default(), StackItemRules3D { group_id: 1, ..Default::default() }];
    let mut result = solve(problem.clone());
    for stack in &mut result.layouts[0].stacks {
        let later = u32::from(stack.items[0].demand_index == 1);
        stack.x = later;
        stack.z = later;
    }
    problem.unloading = UnloadingConstraint3D::OnlyXMovements;
    assert!(validate_loading_3d(&problem, &result).is_ok());
    problem.unloading = UnloadingConstraint3D::IncreasingX;
    invalid(&problem, &result);
    problem.unloading = UnloadingConstraint3D::IncreasingZ;
    invalid(&problem, &result);
    for stack in &mut result.layouts[0].stacks {
        stack.z = 0;
    }
    assert!(validate_loading_3d(&problem, &result).is_ok());

    // Straight removal permits a neighboring lane in either source/order
    // direction. In a shared lane the early delivery must be nearest the exit.
    for mode in [UnloadingConstraint3D::OnlyXMovements, UnloadingConstraint3D::OnlyZMovements] {
        problem.unloading = mode;
        for (early_along, late_along, early_across, late_across, expected) in
            [(0, 1, 0, 1, true), (0, 1, 1, 0, true), (1, 0, 0, 0, true), (0, 1, 0, 0, false)]
        {
            for reverse in [false, true] {
                let mut stacks: Vec<_> =
                    [(0, early_along, early_across), (1, late_along, late_across)]
                        .into_iter()
                        .map(|(demand_index, along, across)| StackColumn3D {
                            x: if mode == UnloadingConstraint3D::OnlyXMovements {
                                along
                            } else {
                                across
                            },
                            z: if mode == UnloadingConstraint3D::OnlyXMovements {
                                across
                            } else {
                                along
                            },
                            width: 1,
                            depth: 1,
                            height: 1,
                            weight: 0,
                            items: vec![StackedItem3D {
                                demand_index,
                                y: 0,
                                rotation: Rotation3D::Xyz,
                            }],
                        })
                        .collect();
                if reverse {
                    stacks.reverse();
                }
                let a = &stacks[0];
                let b = &stacks[1];
                assert_eq!(
                    geometry::unloading_pair(
                        mode,
                        geometry::stack_rect(a),
                        a.items[0].demand_index as u32,
                        geometry::stack_rect(b),
                        b.items[0].demand_index as u32,
                    ),
                    expected
                );
                let readback = validate_loading_3d(&problem, &certificate(stacks, 0));
                if expected {
                    readback.expect("separate lane or unblocked shared lane");
                } else {
                    assert!(matches!(readback, Err(BinPackingError::InvalidSolution { reason })
                        if reason == "later delivery group blocks earlier group"));
                }
            }
        }
    }
}

#[test]
fn loading_axles_are_checked_after_each_delivery_group() {
    let mut problem = problem(10, 1, 1, vec![demand(2, 1, 1, 1); 2]);
    problem.packing.item_weights = vec![10, 10];
    problem.item_rules =
        vec![StackItemRules3D::default(), StackItemRules3D { group_id: 1, ..Default::default() }];
    problem.bin_rules = vec![LoadingBinRules3D {
        axle_limits: Some(AxleLimits3D {
            front_position: 3,
            rear_position: 7,
            ..axles(FloorAxis3D::X, 10, 10)
        }),
        ..Default::default()
    }];
    let result = LoadingSolution3D {
        layouts: vec![LoadingBinLayout3D {
            bin_index: 0,
            payload: 20,
            axle_loads: Some(AxleLoads3D { front: 10, rear: 10 }),
            stacks: [8, 0]
                .into_iter()
                .enumerate()
                .map(|(demand_index, x)| StackColumn3D {
                    x,
                    z: 0,
                    width: 2,
                    depth: 1,
                    height: 1,
                    weight: 10,
                    items: vec![StackedItem3D { demand_index, y: 0, rotation: Rotation3D::Xyz }],
                })
                .collect(),
        }],
        unplaced: Vec::new(),
        bin_count: 1,
        total_cost: 1.0,
        explored_states: 0,
        search_truncated: true,
    };
    assert!(validate_loading_3d(&problem, &result).is_ok());
    problem.unloading = UnloadingConstraint3D::OnlyXMovements;
    invalid(&problem, &result);
    let actual = solve(problem.clone());
    assert!(actual.unplaced.is_empty());
    assert!(validate_loading_3d(&problem, &actual).is_ok());
}

#[test]
fn loading_inventory_payload_and_duplicate_names_keep_source_quantities() {
    let mut problem = nesting_problem();
    problem.packing.problem.bins[0].quantity = Some(1);
    problem.packing.problem.demands[0].quantity = 3;
    let result = solve(problem.clone());
    assert_eq!(result.bin_count, 1);
    assert_eq!(result.unplaced, [UnplacedLoadingItem3D { demand_index: 0, quantity: 1 }]);
    problem.packing.problem.demands = vec![demand(5, 6, 5, 1); 2];
    problem.packing.item_weights = vec![11, 8];
    problem.item_rules.clear();
    let result = solve(problem.clone());
    assert_eq!(result.unplaced, [UnplacedLoadingItem3D { demand_index: 0, quantity: 1 }]);
    assert_eq!(result.layouts[0].stacks[0].items[0].demand_index, 1);
    assert_eq!(result.layouts[0].payload, 8);
    assert!(validate_loading_3d(&problem, &result).is_ok());
}

#[test]
fn loading_floor_defects_are_applied_during_construction() {
    let mut problem = problem(4, 1, 1, vec![demand(1, 1, 1, 3)]);
    problem.packing.problem.bins[0].quantity = Some(1);
    problem.bin_rules = vec![LoadingBinRules3D {
        defects: vec![FloorDefect3D { x: 1, z: 0, width: 2, depth: 1 }],
        ..Default::default()
    }];
    let result = solve(problem.clone());
    assert_eq!(result.unplaced, [UnplacedLoadingItem3D { demand_index: 0, quantity: 1 }]);
    assert_eq!(result.layouts[0].stacks.iter().map(|stack| stack.x).collect::<Vec<_>>(), [0, 3]);
    let mut overlap = result;
    overlap.layouts[0].stacks[0].x = 1;
    invalid(&problem, &overlap);
}

#[test]
fn loading_count_and_cost_choose_different_stock() {
    let mut problem = problem(2, 1, 1, vec![demand(1, 1, 1, 2)]);
    problem.packing.problem.bins[0].cost = 10.0;
    problem.packing.problem.bins.push(Bin3D {
        name: "small".into(),
        width: 1,
        height: 1,
        depth: 1,
        cost: 1.0,
        quantity: None,
    });
    let count = solve(problem.clone());
    assert_eq!((count.bin_count, count.total_cost), (1, 10.0));
    let cost = solve_loading_3d(
        problem,
        LoadingOptions3D { objective: PackingObjective3D::TotalCost, ..Default::default() },
    )
    .expect("cost objective");
    assert_eq!((cost.bin_count, cost.total_cost), (2, 2.0));
}

#[test]
fn loading_zero_and_tiny_budgets_keep_complete_deterministic_incumbents() {
    let problem = nesting_problem();
    for max_states in [0, 1, 2, 3] {
        let options = LoadingOptions3D { max_states, ..Default::default() };
        let result = solve_loading_3d(problem.clone(), options.clone()).expect("budgeted packing");
        assert!(result.unplaced.is_empty());
        assert_eq!(result.bin_count, 1);
        assert!(result.explored_states <= max_states);
        if max_states < 3 {
            assert!(result.search_truncated);
        }
        assert_eq!(result, solve_loading_3d(problem.clone(), options).expect("repeat packing"));
        assert!(validate_loading_3d(&problem, &result).is_ok());
    }
}

#[test]
fn loading_u64_extremes_are_split_without_wrapping_weights_or_moments() {
    let mut problem = problem(1, 2, 1, vec![demand(1, 1, 1, 2)]);
    problem.packing.item_weights = vec![u64::MAX];
    problem.bin_rules = vec![LoadingBinRules3D {
        axle_limits: Some(axles(FloorAxis3D::X, 1, u64::MAX / 2 + 1)),
        ..Default::default()
    }];
    let result = solve(problem.clone());
    assert_eq!(result.bin_count, 2);
    assert!(result.unplaced.is_empty());
    assert!(result.layouts.iter().all(|layout| layout.payload == u64::MAX));
    assert!(validate_loading_3d(&problem, &result).is_ok());
}

#[test]
fn loading_tiny_homogeneous_cases_match_an_independent_capacity_oracle() {
    for quantity in 1..=6 {
        for nesting in 0..=1 {
            for maximum in 1..=3 {
                for payload in [3u64, 6, 9] {
                    let mut problem = problem(1, 4, 1, vec![demand(1, 2, 1, quantity)]);
                    problem.packing.item_weights = vec![3];
                    problem.packing.bin_max_weights = vec![Some(payload)];
                    problem.item_rules = vec![StackItemRules3D {
                        nesting_height: nesting,
                        maximum_stackability: Some(maximum),
                        ..Default::default()
                    }];
                    let height_capacity = if nesting == 0 { 2 } else { 3 };
                    let capacity = height_capacity.min(maximum).min((payload / 3) as usize);
                    let expected = quantity.div_ceil(capacity);
                    let result = solve(problem);
                    assert!(result.unplaced.is_empty());
                    assert_eq!(
                        result.bin_count, expected,
                        "q={quantity} n={nesting} cap={maximum} payload={payload}"
                    );
                }
            }
        }
    }
}

#[test]
fn loading_rejects_invalid_rules_and_controls_with_structured_errors() {
    let base = nesting_problem();
    let mut malformed = Vec::new();
    let mut p = base.clone();
    p.item_rules[0].nesting_height = 6;
    malformed.push(p);
    let mut p = base.clone();
    p.item_rules[0].maximum_stackability = Some(0);
    malformed.push(p);
    let mut p = base.clone();
    p.item_rules.push(StackItemRules3D::default());
    malformed.push(p);
    let mut p = base.clone();
    p.packing.item_weights.push(0);
    malformed.push(p);
    let mut p = base.clone();
    p.packing.bin_max_weights.push(None);
    malformed.push(p);
    let mut p = base.clone();
    p.packing.problem.demands[0].quantity = 129;
    malformed.push(p);
    let mut p = base.clone();
    p.bin_rules = vec![LoadingBinRules3D {
        maximum_stack_density: Some(WeightDensity3D { numerator: 1, denominator: 0 }),
        ..Default::default()
    }];
    malformed.push(p);
    let mut p = base.clone();
    p.bin_rules = vec![LoadingBinRules3D {
        axle_limits: Some(axles(FloorAxis3D::X, 6, 1)),
        ..Default::default()
    }];
    malformed.push(p);
    let mut p = base.clone();
    p.bin_rules = vec![LoadingBinRules3D {
        defects: vec![FloorDefect3D { x: u32::MAX, z: 0, width: 1, depth: 1 }],
        ..Default::default()
    }];
    malformed.push(p);
    for p in malformed {
        assert!(matches!(
            solve_loading_3d(p, LoadingOptions3D::default()),
            Err(BinPackingError::InvalidConstraint { .. })
        ));
    }
    for options in [
        LoadingOptions3D { beam_width: 0, ..Default::default() },
        LoadingOptions3D { max_states: 100_001, ..Default::default() },
    ] {
        assert!(matches!(
            solve_loading_3d(base.clone(), options),
            Err(BinPackingError::InvalidConstraint { .. })
        ));
    }
}

#[test]
fn loading_validator_rejects_tampered_geometry_identity_and_accounting() {
    let problem = nesting_problem();
    let result = solve(problem.clone());
    let mut mutations = Vec::new();
    let mut r = result.clone();
    r.layouts[0].payload = 0;
    mutations.push(r);
    let mut r = result.clone();
    r.layouts[0].stacks[0].weight = 0;
    mutations.push(r);
    let mut r = result.clone();
    r.layouts[0].stacks[0].height = 9;
    mutations.push(r);
    let mut r = result.clone();
    r.layouts[0].stacks[0].items[1].y = 3;
    mutations.push(r);
    let mut r = result.clone();
    r.layouts[0].stacks[0].items[0].rotation = Rotation3D::Xzy;
    mutations.push(r);
    let mut r = result.clone();
    r.layouts[0].stacks[0].items[0].demand_index = 8;
    mutations.push(r);
    let mut r = result.clone();
    r.layouts[0].bin_index = 9;
    mutations.push(r);
    let mut r = result.clone();
    r.layouts[0].stacks[0].x = u32::MAX;
    mutations.push(r);
    let mut r = result.clone();
    r.bin_count = 8;
    mutations.push(r);
    let mut r = result.clone();
    r.total_cost = f64::NAN;
    mutations.push(r);
    let mut r = result.clone();
    r.layouts[0].stacks[0].items.pop().expect("top item");
    mutations.push(r);
    let mut r = result.clone();
    r.unplaced.push(UnplacedLoadingItem3D { demand_index: 0, quantity: 1 });
    mutations.push(r);
    let mut r = result.clone();
    r.layouts[0].axle_loads = Some(AxleLoads3D { front: 0, rear: 0 });
    mutations.push(r);
    for tampered in mutations {
        invalid(&problem, &tampered);
    }
    validate_loading_3d(&problem, &result).expect("untampered nesting certificate");
    type Mutation = (&'static str, &'static str, fn(&mut LoadingSolution3D));
    let cases: &[Mutation] = &[
        ("search count", "invalid bin count or search accounting", |s| s.explored_states = 100_001),
        ("layout count", "invalid bin count or search accounting", |s| {
            s.layouts = vec![s.layouts[0].clone(); 3];
            s.bin_count = 3;
        }),
        ("finite wrong cost", "material cost differs from consumed source bins", |s| {
            s.total_cost += 1.0
        }),
        ("no columns", "loaded bins must contain a bounded nonempty set of stacks", |s| {
            s.layouts[0].stacks.clear()
        }),
        ("too many columns", "loaded bins must contain a bounded nonempty set of stacks", |s| {
            s.layouts[0].stacks = vec![s.layouts[0].stacks[0].clone(); 129]
        }),
        ("zero width", "stack extends outside source bin", |s| s.layouts[0].stacks[0].width = 0),
        ("zero depth", "stack extends outside source bin", |s| s.layouts[0].stacks[0].depth = 0),
        ("zero height", "stack extends outside source bin", |s| s.layouts[0].stacks[0].height = 0),
        ("depth outside", "stack extends outside source bin", |s| {
            s.layouts[0].stacks[0].z = u32::MAX
        }),
        ("height outside", "stack extends outside source bin", |s| {
            s.layouts[0].stacks[0].height += 1
        }),
        ("too many items", "stacks must contain a bounded nonempty set of items", |s| {
            s.layouts[0].stacks[0].items = vec![s.layouts[0].stacks[0].items[0].clone(); 129]
        }),
    ];
    for &(label, expected, mutate) in cases {
        let mut forged = result.clone();
        mutate(&mut forged);
        assert!(
            matches!(validate_loading_3d(&problem, &forged),
            Err(BinPackingError::InvalidSolution { reason }) if reason == expected),
            "{label}"
        );
    }
    let mut fewer = problem.clone();
    fewer.packing.problem.demands[0].quantity = 1;
    assert!(matches!(validate_loading_3d(&fewer, &result),
        Err(BinPackingError::InvalidSolution { reason }) if reason == "placed demand exceeds source quantity"));
    let mut forbidden = problem;
    forbidden.packing.problem.demands[0].allowed_rotations = RotationMask3D::ZYX;
    assert!(matches!(validate_loading_3d(&forbidden, &result),
        Err(BinPackingError::InvalidSolution { reason }) if reason == "item orientation is not permitted upright rotation"));
}

#[test]
fn loading_serde_roundtrips_and_rejects_unknown_rule_fields() {
    let problem = nesting_problem();
    let json = serde_json::to_string(&problem).expect("serialize loading input");
    assert_eq!(serde_json::from_str::<LoadingProblem3D>(&json).expect("input roundtrip"), problem);
    let result = solve(problem);
    let json = serde_json::to_string(&result).expect("serialize loading output");
    assert_eq!(serde_json::from_str::<LoadingSolution3D>(&json).expect("output roundtrip"), result);
    assert!(serde_json::from_str::<StackItemRules3D>(r#"{"maximum_stackablity":2}"#).is_err());
    let defaults = serde_json::from_str::<LoadingOptions3D>("{}").expect("default controls");
    assert_eq!(defaults, LoadingOptions3D::default());
}

fn certificate(stacks: Vec<StackColumn3D>, payload: u64) -> LoadingSolution3D {
    LoadingSolution3D {
        layouts: vec![LoadingBinLayout3D { bin_index: 0, payload, stacks, axle_loads: None }],
        unplaced: Vec::new(),
        bin_count: 1,
        total_cost: 1.0,
        explored_states: 0,
        search_truncated: true,
    }
}

#[test]
fn loading_manual_certificate_checks_exact_density_payload_and_fragility_boundaries() {
    // Two three-unit loads occupy a six-unit footprint. The lower item carries
    // exactly three units above it; all three limits are tight independently.
    let mut problem = problem(2, 2, 3, vec![demand(2, 1, 3, 2)]);
    problem.packing.item_weights = vec![3];
    problem.packing.bin_max_weights = vec![Some(6)];
    problem.item_rules = vec![StackItemRules3D {
        maximum_stackability: Some(2),
        maximum_weight_above: Some(3),
        ..Default::default()
    }];
    problem.bin_rules = vec![LoadingBinRules3D {
        maximum_stack_density: Some(WeightDensity3D { numerator: 1, denominator: 1 }),
        ..Default::default()
    }];
    let result = certificate(
        vec![StackColumn3D {
            x: 0,
            z: 0,
            width: 2,
            depth: 3,
            height: 2,
            weight: 6,
            items: vec![
                StackedItem3D { demand_index: 0, y: 0, rotation: Rotation3D::Xyz },
                StackedItem3D { demand_index: 0, y: 1, rotation: Rotation3D::Xyz },
            ],
        }],
        6,
    );
    validate_loading_3d(&problem, &result).expect("all exact boundaries are permitted");
    let mut stricter = problem.clone();
    stricter.bin_rules[0].maximum_stack_density =
        Some(WeightDensity3D { numerator: 5, denominator: 6 });
    invalid(&stricter, &result);
    let mut stricter = problem.clone();
    stricter.packing.bin_max_weights[0] = Some(5);
    invalid(&stricter, &result);
    let mut stricter = problem.clone();
    stricter.item_rules[0].maximum_weight_above = Some(2);
    invalid(&stricter, &result);
    problem.item_rules[0].maximum_stackability = Some(1);
    invalid(&problem, &result);
}

#[test]
fn loading_axle_positions_match_a_hand_calculated_reaction_table_on_both_axes() {
    // An eight-unit load between supports at 1 and 5 transfers two units from
    // front to rear per unit translation. Fixed tare adds two and three units.
    let expected = [(10, 3), (8, 5), (6, 7), (4, 9), (2, 11)];
    for axis in [FloorAxis3D::X, FloorAxis3D::Z] {
        let (width, depth) = if axis == FloorAxis3D::X { (6, 2) } else { (2, 6) };
        let mut problem = problem(width, 1, depth, vec![demand(2, 1, 2, 1)]);
        problem.packing.item_weights = vec![8];
        problem.bin_rules = vec![LoadingBinRules3D {
            axle_limits: Some(AxleLimits3D {
                axis,
                front_position: 1,
                rear_position: 5,
                front_maximum: 8,
                rear_maximum: 9,
                front_tare: 2,
                rear_tare: 3,
            }),
            ..Default::default()
        }];
        for (position, &(front, rear)) in (0u32..=4).zip(&expected) {
            let mut result = certificate(
                vec![StackColumn3D {
                    x: if axis == FloorAxis3D::X { position } else { 0 },
                    z: if axis == FloorAxis3D::Z { position } else { 0 },
                    width: 2,
                    depth: 2,
                    height: 1,
                    weight: 8,
                    items: vec![StackedItem3D { demand_index: 0, y: 0, rotation: Rotation3D::Xyz }],
                }],
                8,
            );
            result.layouts[0].axle_loads = Some(AxleLoads3D { front, rear });
            assert_eq!(
                validate_loading_3d(&problem, &result).is_ok(),
                (1..=3).contains(&position),
                "axis={axis:?}, position={position}"
            );
        }
        let result = solve(problem.clone());
        let column = &result.layouts[0].stacks[0];
        let position = if axis == FloorAxis3D::X { column.x } else { column.z };
        assert!((1..=3).contains(&position));
        let (front, rear) = expected[position as usize];
        assert_eq!(result.layouts[0].axle_loads, Some(AxleLoads3D { front, rear }));
    }
}

#[test]
fn loading_overhanging_centers_cannot_have_negative_support_reactions() {
    let mut problem = problem(4, 1, 1, vec![demand(1, 1, 1, 1)]);
    problem.packing.item_weights = vec![10];
    problem.bin_rules = vec![LoadingBinRules3D {
        axle_limits: Some(AxleLimits3D {
            front_position: 1,
            rear_position: 3,
            ..axles(FloorAxis3D::X, 4, u64::MAX)
        }),
        ..Default::default()
    }];
    // At x=1 and x=2 the reactions are 7.5/2.5 and 2.5/7.5. Centers outside
    // both supports require a negative reaction, regardless of generous limits.
    for (x, front, rear, valid) in
        [(0, 13, 0, false), (1, 8, 3, true), (2, 3, 8, true), (3, 0, 13, false)]
    {
        let mut result = certificate(
            vec![StackColumn3D {
                x,
                z: 0,
                width: 1,
                depth: 1,
                height: 1,
                weight: 10,
                items: vec![StackedItem3D { demand_index: 0, y: 0, rotation: Rotation3D::Xyz }],
            }],
            10,
        );
        result.layouts[0].axle_loads = Some(AxleLoads3D { front, rear });
        assert_eq!(validate_loading_3d(&problem, &result).is_ok(), valid, "x={x}");
    }
}

#[test]
fn loading_nesting_cannot_extend_an_upper_item_below_the_floor() {
    let mut problem = problem(1, 5, 1, vec![demand(1, 2, 1, 1), demand(1, 4, 1, 1)]);
    problem.item_rules = vec![
        StackItemRules3D::default(),
        StackItemRules3D { nesting_height: 3, ..Default::default() },
    ];
    let result = solve(problem.clone());
    assert_eq!(result.bin_count, 2);
    assert!(result.unplaced.is_empty());
    let forged = certificate(
        vec![StackColumn3D {
            x: 0,
            z: 0,
            width: 1,
            depth: 1,
            height: 3,
            weight: 0,
            items: vec![
                StackedItem3D { demand_index: 0, y: 0, rotation: Rotation3D::Xyz },
                StackedItem3D { demand_index: 1, y: 0, rotation: Rotation3D::Xyz },
            ],
        }],
        0,
    );
    invalid(&problem, &forged);
}

#[test]
fn loading_unplaced_certificates_require_canonical_source_accounting() {
    let problem = problem(1, 1, 1, vec![demand(1, 1, 1, 1); 3]);
    let result = LoadingSolution3D {
        layouts: Vec::new(),
        unplaced: (0..3)
            .map(|demand_index| UnplacedLoadingItem3D { demand_index, quantity: 1 })
            .collect(),
        bin_count: 0,
        total_cost: 0.0,
        explored_states: 0,
        search_truncated: true,
    };
    validate_loading_3d(&problem, &result).expect("an explicit all-unplaced incumbent is valid");
    for unplaced in [
        vec![UnplacedLoadingItem3D { demand_index: 0, quantity: 0 }],
        vec![UnplacedLoadingItem3D { demand_index: 0, quantity: 1 }; 3],
        result.unplaced.iter().cloned().rev().collect(),
        vec![UnplacedLoadingItem3D { demand_index: 3, quantity: 1 }],
    ] {
        let mut forged = result.clone();
        forged.unplaced = unplaced;
        invalid(&problem, &forged);
    }
    let mut forged = result;
    forged.layouts.push(LoadingBinLayout3D {
        bin_index: 0,
        payload: 0,
        stacks: vec![StackColumn3D {
            x: 0,
            z: 0,
            width: 1,
            depth: 1,
            height: 1,
            weight: 0,
            items: vec![StackedItem3D { demand_index: 0, y: 0, rotation: Rotation3D::Xyz }],
        }],
        axle_loads: None,
    });
    forged.bin_count = 1;
    forged.total_cost = 1.0;
    forged.unplaced[0].quantity = usize::MAX;
    invalid(&problem, &forged);
}

#[test]
fn loading_validates_floor_column_separation_and_inventory_independently() {
    let mut problem = problem(2, 1, 1, vec![demand(1, 1, 1, 2)]);
    let result = certificate(
        (0..2)
            .map(|x| StackColumn3D {
                x,
                z: 0,
                width: 1,
                depth: 1,
                height: 1,
                weight: 0,
                items: vec![StackedItem3D { demand_index: 0, y: 0, rotation: Rotation3D::Xyz }],
            })
            .collect(),
        0,
    );
    validate_loading_3d(&problem, &result).expect("adjacent columns have disjoint interiors");
    let mut forged = result.clone();
    forged.layouts[0].stacks[1].x = 0;
    invalid(&problem, &forged);
    let mut forged = result.clone();
    forged.layouts[0].stacks[0].items.clear();
    invalid(&problem, &forged);
    let mut forged = result.clone();
    forged.layouts[0].stacks[0].width = 2;
    forged.layouts[0].stacks.pop().expect("other column");
    invalid(&problem, &forged);
    let mut forged = result;
    let second = forged.layouts[0].stacks.pop().expect("second column");
    forged.layouts.push(LoadingBinLayout3D {
        bin_index: 0,
        payload: 0,
        stacks: vec![second],
        axle_loads: None,
    });
    forged.bin_count = 2;
    forged.total_cost = 2.0;
    problem.packing.problem.bins[0].quantity = Some(2);
    validate_loading_3d(&problem, &forged).expect("two consumed copies are available");
    problem.packing.problem.bins[0].quantity = Some(1);
    invalid(&problem, &forged);
}

#[test]
fn loading_zero_density_accepts_weightless_items_and_rejects_positive_mass() {
    let mut problem = problem(1, 1, 1, vec![demand(1, 1, 1, 1)]);
    problem.bin_rules = vec![LoadingBinRules3D {
        maximum_stack_density: Some(WeightDensity3D { numerator: 0, denominator: u64::MAX }),
        ..Default::default()
    }];
    let weightless = solve(problem.clone());
    assert!(weightless.unplaced.is_empty());
    assert_eq!(weightless.layouts[0].payload, 0);
    problem.packing.item_weights = vec![1];
    let positive = solve(problem);
    assert_eq!(positive.bin_count, 0);
    assert_eq!(positive.unplaced, [UnplacedLoadingItem3D { demand_index: 0, quantity: 1 }]);
}

#[test]
fn loading_rejects_unsupported_or_misaligned_physical_constraints() {
    let base = problem(2, 2, 2, vec![demand(1, 1, 1, 1)]);
    let mut malformed = Vec::new();
    let mut p = base.clone();
    p.packing.problem.demands[0].allowed_rotations = RotationMask3D::XZY;
    malformed.push(p);
    let mut p = base.clone();
    p.packing.problem.demands = vec![demand(1, 1, 1, 1); 33];
    malformed.push(p);
    let mut p = base.clone();
    p.packing.problem.bins = vec![p.packing.problem.bins[0].clone(); 17];
    for (index, bin) in p.packing.problem.bins.iter_mut().enumerate() {
        bin.name = format!("stock {index}");
    }
    malformed.push(p);
    let mut p = base.clone();
    p.bin_rules = vec![LoadingBinRules3D::default(); 2];
    malformed.push(p);
    let mut p = base.clone();
    p.bin_rules = vec![LoadingBinRules3D {
        defects: vec![FloorDefect3D { x: 0, z: 0, width: 1, depth: 1 }; 257],
        ..Default::default()
    }];
    malformed.push(p);
    let mut p = base.clone();
    p.packing.problem.bins[0].cost = f64::MAX;
    malformed.push(p);
    for axis in [FloorAxis3D::X, FloorAxis3D::Z] {
        let mut p = base.clone();
        p.bin_rules = vec![LoadingBinRules3D {
            axle_limits: Some(AxleLimits3D { front_tare: 2, ..axles(axis, 2, 1) }),
            ..Default::default()
        }];
        malformed.push(p);
    }
    for (case, p) in malformed.into_iter().enumerate() {
        let result = solve_loading_3d(p, LoadingOptions3D::default());
        assert!(
            matches!(result, Err(BinPackingError::InvalidConstraint { .. })),
            "physical constraint case {case}: {result:?}"
        );
    }

    for defect in [
        FloorDefect3D { x: 0, z: 0, width: 0, depth: 1 },
        FloorDefect3D { x: 0, z: 0, width: 1, depth: 0 },
        FloorDefect3D { x: 0, z: 2, width: 1, depth: 1 },
    ] {
        let mut p = base.clone();
        p.bin_rules = vec![LoadingBinRules3D { defects: vec![defect], ..Default::default() }];
        assert!(matches!(solve_loading_3d(p, LoadingOptions3D::default()),
            Err(BinPackingError::InvalidConstraint { field, .. }) if field == "defects"));
    }
    for limits in [
        AxleLimits3D { front_position: 1, rear_position: 1, ..axles(FloorAxis3D::X, 2, 1) },
        AxleLimits3D { rear_tare: 2, ..axles(FloorAxis3D::Z, 2, 1) },
    ] {
        let mut p = base.clone();
        p.bin_rules = vec![LoadingBinRules3D { axle_limits: Some(limits), ..Default::default() }];
        assert!(matches!(solve_loading_3d(p, LoadingOptions3D::default()),
            Err(BinPackingError::InvalidConstraint { field, .. }) if field == "axle_limits"));
    }
    // A ZYX-only source is upright and legal even when XYZ is forbidden.
    let mut zyx = base.clone();
    zyx.packing.problem.demands[0].allowed_rotations = RotationMask3D::ZYX;
    let result = solve(zyx.clone());
    assert_eq!(result.layouts[0].stacks[0].items[0].rotation, Rotation3D::Zyx);
    validate_loading_3d(&zyx, &result).expect("permitted alternate upright orientation");
}
