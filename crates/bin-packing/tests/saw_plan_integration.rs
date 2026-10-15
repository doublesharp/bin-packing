#![cfg(feature = "two-d")]

use bin_packing::two_d::saw_plan::*;
use bin_packing::two_d::{Placement2D, RectDemand2D, Sheet2D, TwoDProblem};

fn strips(kerf: u32, sheets: usize) -> SawJob2D {
    let width = 1200 + 3 * kerf;
    SawJob2D {
        problem: TwoDProblem {
            sheets: vec![Sheet2D {
                name: "stock".into(),
                width,
                height: 600,
                cost: 1.0,
                quantity: None,
                kerf,
                edge_kerf_relief: false,
            }],
            demands: vec![RectDemand2D {
                name: "panel".into(),
                width: 300,
                height: 600,
                quantity: 4 * sheets,
                can_rotate: false,
            }],
        },
        layouts: (0..sheets)
            .map(|_| SawLayout2D {
                width,
                height: 600,
                placements: (0..4)
                    .map(|i| Placement2D {
                        name: "panel".into(),
                        x: i * (300 + kerf),
                        y: 0,
                        width: 300,
                        height: 600,
                        rotated: false,
                    })
                    .collect(),
            })
            .collect(),
        stock_indices: vec![0; sheets],
        demand_indices: vec![vec![0; 4]; sheets],
    }
}

#[test]
fn repeated_strips_use_one_real_setting() {
    for kerf in [0, 3] {
        let job = strips(kerf, 1);
        let plan = plan_saw_cuts(&job, &SawPlanOptions2D::default()).expect("physical strips");
        assert_eq!(plan.counts.cuts, 3);
        assert_eq!(plan.counts.initial_setups, 1);
        assert_eq!(plan.counts.setup_changes, 0);
        replay(&job, &plan);
    }
}

// Replay derives exact partitions and ancestry from operations, not planner helpers.
fn replay(job: &SawJob2D, plan: &SawPlan2D) {
    replay_with_minimum(job, plan, 0);
}

fn replay_with_minimum(job: &SawJob2D, plan: &SawPlan2D, min_usable_side: u32) {
    use std::collections::{BTreeMap, BTreeSet};
    let mut available = BTreeMap::new();
    let mut ancestry = BTreeMap::new();
    for (sheet_index, layout) in job.layouts.iter().enumerate() {
        let id = WorkpieceId2D {
            sheet_index,
            bounds: SawRect2D { x: 0, y: 0, width: layout.width, height: layout.height },
        };
        available.insert(id, 0u8);
        ancestry.insert(id, (None, None));
    }
    let mut active = None;
    let mut setup = None;
    let mut seen_kinds = BTreeSet::new();
    let mut counts = SawCounts2D::default();
    let mut last_sheet = 0;
    for (i, operation) in plan.operations.iter().enumerate() {
        assert_eq!(operation.id, i);
        let id = operation.workpiece;
        if !plan.batch_across_sheets {
            assert!(id.sheet_index >= last_sheet, "non-batched sheet order");
        }
        last_sheet = id.sheet_index;
        assert!(available.contains_key(&id));
        assert_eq!(operation.depends_on, ancestry[&id].1);
        if !matches!(operation.action, SawAction2D::Handle) {
            assert_eq!(active, Some(id));
        }
        match &operation.action {
            SawAction2D::Handle => {
                assert!(active.is_none());
                active = Some(id);
                counts.handling += 1;
            }
            SawAction2D::Rotate { quarter_turns } => {
                let old = available.get_mut(&id).expect("available workpiece");
                assert!(*quarter_turns < 4);
                assert_ne!(*old, *quarter_turns);
                *old = *quarter_turns;
                counts.rotations += 1;
            }
            SawAction2D::Setup { setup: new, initial } => {
                assert_ne!(setup, Some(*new));
                assert_eq!(*initial, seen_kinds.insert(new.kind));
                if *initial {
                    counts.initial_setups += 1;
                } else {
                    counts.setup_changes += 1;
                }
                setup = Some(*new);
            }
            SawAction2D::Cut {
                axis,
                reference_edge,
                setting,
                kerf,
                kerf_region,
                segment,
                children,
            } => {
                let b = id.bounds;
                let stock = &job.problem.sheets[job.stock_indices[id.sheet_index]];
                assert_eq!(*kerf, stock.kerf);
                let orientation = available.remove(&id).expect("live parent");
                let vertical = *axis == SawAxis2D::Vertical;
                let accessory = match (plan.machine, vertical) {
                    (SawMachine2D::PanelSaw, _) => SawSetupKind2D::PanelStop,
                    (_, true) => SawSetupKind2D::RipFence,
                    (_, false) => SawSetupKind2D::CrosscutStop,
                };
                assert_eq!(setup, Some(SawSetup2D { kind: accessory, distance: *setting }));
                let (lo, hi, start, end) = if vertical {
                    assert_eq!((kerf_region.y, kerf_region.height), (b.y, b.height));
                    assert_eq!(
                        *segment,
                        SawSegment2D {
                            from_x: kerf_region.x,
                            to_x: kerf_region.x,
                            from_y: b.y,
                            to_y: b.y + b.height
                        }
                    );
                    assert!(matches!(
                        reference_edge,
                        ReferenceEdge2D::Left | ReferenceEdge2D::Right
                    ));
                    (b.x, b.x + b.width, kerf_region.x, kerf_region.x + kerf_region.width)
                } else {
                    assert_eq!((kerf_region.x, kerf_region.width), (b.x, b.width));
                    assert_eq!(
                        *segment,
                        SawSegment2D {
                            from_x: b.x,
                            to_x: b.x + b.width,
                            from_y: kerf_region.y,
                            to_y: kerf_region.y
                        }
                    );
                    assert!(matches!(
                        reference_edge,
                        ReferenceEdge2D::Top | ReferenceEdge2D::Bottom
                    ));
                    (b.y, b.y + b.height, kerf_region.y, kerf_region.y + kerf_region.height)
                };
                assert!(start >= lo && end <= hi && start <= end && start < hi && end > lo);
                assert!(end - start <= stock.kerf);
                if end - start < stock.kerf {
                    assert!(stock.edge_kerf_relief && (start == lo || end == hi));
                }
                assert!(stock.kerf == 0 || end > start);
                let from_low =
                    matches!(reference_edge, ReferenceEdge2D::Left | ReferenceEdge2D::Top);
                assert_eq!(*setting, if from_low { start - lo } else { hi - end });
                assert!(*setting > 0);
                // Rotate the outward reference-edge normal in the X-right/Y-down frame.
                let mut normal = match reference_edge {
                    ReferenceEdge2D::Left => (-1, 0),
                    ReferenceEdge2D::Top => (0, -1),
                    ReferenceEdge2D::Right => (1, 0),
                    ReferenceEdge2D::Bottom => (0, 1),
                };
                for _ in 0..orientation {
                    normal = (-normal.1, normal.0);
                }
                assert_eq!(normal, (-1, 0), "reference edge must face the left guide");
                for p in &job.layouts[id.sheet_index].placements {
                    if p.x >= b.x
                        && p.y >= b.y
                        && p.x + p.width <= b.x + b.width
                        && p.y + p.height <= b.y + b.height
                    {
                        if vertical {
                            assert!(p.x + p.width <= start || p.x >= end, "blade crosses a part");
                        } else {
                            assert!(p.y + p.height <= start || p.y >= end, "blade crosses a part");
                        }
                    }
                }
                // Exact children are stronger than summed area: area alone admits overlaps.
                let mut expected = Vec::new();
                for (a, z) in [(lo, start), (end, hi)] {
                    if z > a {
                        expected.push(WorkpieceId2D {
                            sheet_index: id.sheet_index,
                            bounds: if vertical {
                                SawRect2D { x: a, width: z - a, ..b }
                            } else {
                                SawRect2D { y: a, height: z - a, ..b }
                            },
                        });
                    }
                }
                assert_eq!(*children, expected);
                for child in children {
                    assert!(available.insert(*child, orientation).is_none());
                    assert!(ancestry.insert(*child, (Some(id), Some(i))).is_none());
                }
                active = None;
                counts.cuts += 1;
            }
        }
    }
    assert!(active.is_none());
    assert_eq!(counts, plan.counts);
    assert_eq!(effort(counts, plan.effective_weights), plan.effort_score);
    assert_eq!(plan.workpieces.len(), ancestry.len());
    let mut recorded = BTreeSet::new();
    for piece in &plan.workpieces {
        assert!(recorded.insert(piece.id), "duplicate workpiece record");
        assert_eq!((piece.parent, piece.created_by), ancestry[&piece.id]);
        let b = piece.id.bounds;
        let placements = &job.layouts[piece.id.sheet_index].placements;
        let finished = placements
            .iter()
            .position(|p| (p.x, p.y, p.width, p.height) == (b.x, b.y, b.width, b.height))
            .map(|placement_index| PartId2D { sheet_index: piece.id.sheet_index, placement_index });
        assert_eq!(piece.finished_part, finished);
        let waste = available.contains_key(&piece.id)
            && !placements.iter().any(|p| {
                p.x < b.x + b.width
                    && b.x < p.x + p.width
                    && p.y < b.y + b.height
                    && b.y < p.y + p.height
            });
        assert_eq!(
            piece.usable_offcut,
            waste && b.width >= min_usable_side && b.height >= min_usable_side
        );
    }
    for (sheet_index, layout) in job.layouts.iter().enumerate() {
        for p in &layout.placements {
            assert!(
                available.contains_key(&WorkpieceId2D {
                    sheet_index,
                    bounds: SawRect2D { x: p.x, y: p.y, width: p.width, height: p.height }
                }),
                "required finished part missing"
            );
        }
    }
    let expected_views: Vec<_> = job
        .stock_indices
        .iter()
        .enumerate()
        .map(|(sheet_index, &stock_index)| SawSheetView2D {
            sheet_index_in_solution: sheet_index,
            stock_index,
            operation_ids: plan
                .operations
                .iter()
                .filter(|op| op.workpiece.sheet_index == sheet_index)
                .map(|op| op.id)
                .collect(),
        })
        .collect();
    assert_eq!(plan.sheet_views, expected_views);
}

fn effort(c: SawCounts2D, w: SawWeights2D) -> f64 {
    [
        (c.cuts, w.cut),
        (c.initial_setups, w.initial_setup),
        (c.setup_changes, w.setup_change),
        (c.rotations, w.rotation),
        (c.handling, w.handling),
    ]
    .into_iter()
    .map(|(count, weight)| count as f64 * weight)
    .sum()
}

#[test]
fn provenance_rotation_and_geometry_fail_explicitly() {
    let mut job = strips(3, 1);
    job.layouts[0].placements[3].x += 1;
    assert!(matches!(
        plan_saw_cuts(&job, &SawPlanOptions2D::default()),
        Err(SawPlanError2D::InvalidGeometry { .. })
    ));
    job.problem.sheets[0].edge_kerf_relief = true;
    assert!(matches!(
        plan_saw_cuts(&job, &SawPlanOptions2D::default()),
        Err(SawPlanError2D::InvalidGeometry { .. })
    ));
    let mut job = strips(0, 1);
    job.layouts[0].placements[0].rotated = true;
    assert!(matches!(
        plan_saw_cuts(&job, &SawPlanOptions2D::default()),
        Err(SawPlanError2D::InvalidMapping { .. })
    ));
    let mut job = strips(0, 1);
    job.layouts[0].placements[1].x = 299;
    assert!(matches!(
        plan_saw_cuts(&job, &SawPlanOptions2D::default()),
        Err(SawPlanError2D::InvalidGeometry { .. })
    ));
    let mut job = strips(0, 1);
    job.stock_indices[0] = 1;
    assert!(matches!(
        plan_saw_cuts(&job, &SawPlanOptions2D::default()),
        Err(SawPlanError2D::InvalidMapping { .. })
    ));
}

fn layout_job(width: u32, height: u32, kerf: u32, parts: &[(u32, u32, u32, u32)]) -> SawJob2D {
    SawJob2D {
        problem: TwoDProblem {
            sheets: vec![Sheet2D {
                name: "duplicate".into(),
                width,
                height,
                cost: 1.0,
                quantity: None,
                kerf,
                edge_kerf_relief: false,
            }],
            demands: parts
                .iter()
                .map(|&(_, _, w, h)| RectDemand2D {
                    name: "duplicate".into(),
                    width: w,
                    height: h,
                    quantity: 1,
                    can_rotate: false,
                })
                .collect(),
        },
        layouts: vec![SawLayout2D {
            width,
            height,
            placements: parts
                .iter()
                .map(|&(x, y, w, h)| Placement2D {
                    name: "duplicate".into(),
                    x,
                    y,
                    width: w,
                    height: h,
                    rotated: false,
                })
                .collect(),
        }],
        stock_indices: vec![0],
        demand_indices: vec![(0..parts.len()).collect()],
    }
}

#[test]
fn trims_waste_factory_edges_and_partial_blade_relief_replay() {
    let mut job = layout_job(53, 41, 3, &[(2, 0, 21, 41), (26, 0, 27, 41)]);
    assert!(plan_saw_cuts(&job, &SawPlanOptions2D::default()).is_err());
    job.problem.sheets[0].edge_kerf_relief = true;
    let plan = plan_saw_cuts(&job, &SawPlanOptions2D::default()).expect("clipped blade in air");
    assert_eq!(plan.counts.cuts, 2);
    replay(&job, &plan);
    let job = layout_job(87, 61, 3, &[(4, 7, 23, 19), (40, 7, 47, 19), (4, 29, 23, 32)]);
    let plan = plan_saw_cuts(&job, &SawPlanOptions2D { min_usable_side: 4, ..Default::default() })
        .expect("asymmetric trims");
    replay_with_minimum(&job, &plan, 4);
    assert!(plan.workpieces.iter().any(|p| p.usable_offcut));
}

#[test]
fn non_slicing_pinwheel_is_rejected() {
    let job = layout_job(
        10,
        10,
        0,
        &[(0, 0, 6, 4), (6, 0, 4, 6), (4, 6, 6, 4), (0, 4, 4, 6), (4, 4, 2, 2)],
    );
    assert!(matches!(
        plan_saw_cuts(&job, &SawPlanOptions2D::default()),
        Err(SawPlanError2D::NonGuillotine { .. })
    ));
}

#[test]
fn exhausted_budget_is_deterministic_and_keeps_incumbent() {
    let job = layout_job(87, 61, 3, &[(4, 7, 23, 19), (40, 7, 47, 19), (4, 29, 23, 32)]);
    for max_states in [0, 1, 17, 4000] {
        let o = SawPlanOptions2D {
            search: SawSearchLimits2D { max_states, ..Default::default() },
            ..Default::default()
        };
        let plan = plan_saw_cuts(&job, &o).expect("incumbent");
        assert_eq!(plan, plan_saw_cuts(&job, &o).expect("deterministic"));
        assert!(plan.effort_score <= plan.search.baseline_effort_score);
        assert!(plan.search.explored_states <= max_states);
        replay(&job, &plan);
    }
}

#[test]
fn duplicate_labels_never_merge_stock_or_part_instances() {
    let mut job = strips(0, 2);
    job.problem.sheets.push(job.problem.sheets[0].clone());
    job.problem.sheets[0].quantity = Some(1);
    job.problem.sheets[1].quantity = Some(1);
    job.stock_indices = vec![0, 1];
    let plan =
        plan_saw_cuts(&job, &SawPlanOptions2D { batch_across_sheets: true, ..Default::default() })
            .expect("indices are identities");
    assert_eq!(plan.counts.cuts, 6);
    assert_eq!(plan.counts.initial_setups, 1);
    assert_eq!(plan.counts.setup_changes, 0);
    assert_eq!(plan.sheet_views[1].stock_index, 1);
    replay(&job, &plan);
}

#[test]
fn geometry_input_ignores_large_unused_packing_metrics() {
    let job = strips(0, 1);
    let mut value = serde_json::to_value(&job).expect("json");
    value["layouts"][0]["sum_sq_usable_drop_areas"] = serde_json::json!(1e30);
    let parsed: SawJob2D = serde_json::from_value(value).expect("ignore unused u128 metric");
    assert_eq!(parsed, job);
}

fn tradeoff_options() -> SawPlanOptions2D {
    SawPlanOptions2D {
        machine: SawMachine2D::PanelSaw,
        search: SawSearchLimits2D { max_states: 30000, beam_width: 64, ..Default::default() },
        ..Default::default()
    }
}

#[test]
fn weights_change_actual_schedules_and_independently_replayed_scores() {
    // This tiny trimmed panel has a proven tradeoff: using the opposite edge can
    // reuse setting 3, but requires an additional orientation change.
    let job = layout_job(4, 5, 0, &[(0, 0, 1, 2)]);
    let a_options = SawPlanOptions2D {
        weights: SawWeights2D { setup_change: 0.0, rotation: 100.0, ..Default::default() },
        ..tradeoff_options()
    };
    let b_options = SawPlanOptions2D {
        weights: SawWeights2D { setup_change: 100.0, rotation: 0.0, ..Default::default() },
        ..tradeoff_options()
    };
    let a = plan_saw_cuts(&job, &a_options).expect("rotation priority");
    let b = plan_saw_cuts(&job, &b_options).expect("setting priority");
    assert_eq!(a.counts, oracle(&job, &a_options).expect("rotation optimum"));
    assert_eq!(b.counts, oracle(&job, &b_options).expect("setup optimum"));
    assert_eq!(a.effective_weights, a_options.weights);
    assert_eq!(b.effective_weights, b_options.weights);
    assert_ne!(a.operations, b.operations);
    assert!(effort(a.counts, a_options.weights) < effort(b.counts, a_options.weights));
    assert!(effort(b.counts, b_options.weights) < effort(a.counts, b_options.weights));
    replay(&job, &a);
    replay(&job, &b);
}

#[test]
fn objectives_meet_independently_feasible_quality_bounds() {
    let job = layout_job(23, 19, 0, &[(0, 0, 10, 7), (10, 0, 7, 8), (10, 8, 7, 5), (10, 13, 4, 6)]);
    let a = plan_saw_cuts(
        &job,
        &SawPlanOptions2D { objective: SawObjective2D::FewestCuts, ..tradeoff_options() },
    )
    .expect("cuts");
    let b = plan_saw_cuts(
        &job,
        &SawPlanOptions2D { objective: SawObjective2D::FewestSetupChanges, ..tradeoff_options() },
    )
    .expect("setups");
    // Independently replayed feasible witnesses from this fixture have six cuts
    // and four total setups. Better search may dominate either witness.
    assert!(a.counts.cuts <= 6);
    assert!(b.counts.initial_setups + b.counts.setup_changes <= 4);
    replay(&job, &a);
    replay(&job, &b);
}

#[test]
fn cross_sheet_batching_saves_actual_changes_in_authoritative_order() {
    let mut job =
        layout_job(23, 19, 0, &[(0, 0, 5, 9), (5, 0, 8, 8), (13, 0, 2, 7), (13, 7, 2, 2)]);
    job.layouts.push(job.layouts[0].clone());
    job.stock_indices.push(0);
    job.demand_indices.push(job.demand_indices[0].clone());
    for d in &mut job.problem.demands {
        d.quantity *= 2;
    }
    let a = plan_saw_cuts(&job, &tradeoff_options()).expect("independent sheets");
    let b =
        plan_saw_cuts(&job, &SawPlanOptions2D { batch_across_sheets: true, ..tradeoff_options() })
            .expect("batch");
    assert!(b.counts.setup_changes < a.counts.setup_changes);
    assert!(b.effort_score < a.effort_score);
    assert_eq!(b.counts.handling, b.counts.cuts);
    let sheets: Vec<_> = b
        .operations
        .iter()
        .filter(|op| matches!(op.action, SawAction2D::Cut { .. }))
        .map(|op| op.workpiece.sheet_index)
        .collect();
    assert!(sheets.windows(2).filter(|w| w[0] != w[1]).count() > 1);
    for view in &b.sheet_views {
        for &id in &view.operation_ids {
            assert_eq!(b.operations[id].workpiece.sheet_index, view.sheet_index_in_solution);
        }
    }
    replay(&job, &a);
    replay(&job, &b);
    let mut mislabeled = b.clone();
    mislabeled.batch_across_sheets = false;
    assert!(std::panic::catch_unwind(|| replay(&job, &mislabeled)).is_err());
}

#[test]
fn coordinate_values_are_not_machine_settings() {
    let job = layout_job(8, 4, 0, &[(0, 0, 4, 2), (4, 0, 4, 2), (0, 2, 4, 2), (4, 2, 4, 2)]);
    let plan = plan_saw_cuts(&job, &SawPlanOptions2D::default()).expect("grid");
    replay(&job, &plan);
    let setups: Vec<_> = plan
        .operations
        .iter()
        .filter_map(|op| match op.action {
            SawAction2D::Setup { setup, .. } => Some(setup),
            _ => None,
        })
        .collect();
    assert!(setups.iter().any(|s| s.kind == SawSetupKind2D::CrosscutStop));
    assert!(setups.iter().any(|s| s.kind == SawSetupKind2D::RipFence));
    let job = strips(0, 1);
    let plan = plan_saw_cuts(&job, &SawPlanOptions2D::default()).expect("strips");
    let cuts: Vec<_> = plan
        .operations
        .iter()
        .filter_map(|op| match op.action {
            SawAction2D::Cut { setting, segment, .. } => Some((segment.from_x, setting)),
            _ => None,
        })
        .collect();
    assert_eq!(cuts, vec![(300, 300), (600, 300), (900, 300)]);
    // At the same sheet coordinate X=4, a root needs setting 4 while the
    // remainder of an earlier cut needs setting 2. No weight-change side effect
    // is needed to manufacture the distinction.
    let fixtures = [
        layout_job(8, 6, 0, &[(0, 0, 4, 6), (4, 0, 4, 6)]),
        layout_job(8, 6, 0, &[(0, 0, 2, 6), (4, 0, 4, 6)]),
    ];
    let mut observed = Vec::new();
    for job in &fixtures {
        let plan = plan_saw_cuts(
            job,
            &SawPlanOptions2D {
                search: SawSearchLimits2D { max_states: 0, ..Default::default() },
                ..Default::default()
            },
        )
        .expect("coordinate fixture");
        replay(job, &plan);
        for op in &plan.operations {
            if let SawAction2D::Cut { axis: SawAxis2D::Vertical, segment, setting, .. } = op.action
                && segment.from_x == 4
            {
                observed.push((op.workpiece.bounds.x, setting));
            }
        }
    }
    assert_eq!(observed, vec![(0, 4), (2, 2)]);
}

// Independent exhaustive oracle for tiny integer sheets. It scans every blade
// position, including clipped kerf, rather than using planner cut descriptors.
fn oracle(job: &SawJob2D, options: &SawPlanOptions2D) -> Option<SawCounts2D> {
    use std::collections::BTreeMap;
    #[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
    struct Region {
        x: u32,
        y: u32,
        w: u32,
        h: u32,
        orientation: u8,
    }
    #[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
    struct OracleState {
        regions: Vec<Region>,
        setup: Option<(u8, u32)>,
        seen: u8,
    }
    fn compare(a: SawCounts2D, b: SawCounts2D, o: &SawPlanOptions2D) -> std::cmp::Ordering {
        let primary = match o.objective {
            SawObjective2D::FewestCuts => a.cuts.cmp(&b.cuts),
            SawObjective2D::FewestSetupChanges => {
                (a.initial_setups + a.setup_changes).cmp(&(b.initial_setups + b.setup_changes))
            }
            SawObjective2D::WeightedEffort => std::cmp::Ordering::Equal,
        };
        primary
            .then_with(|| effort(a, o.weights).total_cmp(&effort(b, o.weights)))
            .then_with(|| a.cuts.cmp(&b.cuts))
            .then_with(|| {
                (a.initial_setups + a.setup_changes).cmp(&(b.initial_setups + b.setup_changes))
            })
            .then_with(|| a.rotations.cmp(&b.rotations))
            .then_with(|| a.handling.cmp(&b.handling))
    }
    fn visit(
        job: &SawJob2D,
        o: &SawPlanOptions2D,
        mut state: OracleState,
        memo: &mut BTreeMap<OracleState, Option<SawCounts2D>>,
    ) -> Option<SawCounts2D> {
        let parts = &job.layouts[0].placements;
        let stock = &job.problem.sheets[job.stock_indices[0]];
        state.regions.retain(|r| {
            let contained: Vec<_> = parts
                .iter()
                .filter(|p| {
                    p.x >= r.x
                        && p.y >= r.y
                        && p.x + p.width <= r.x + r.w
                        && p.y + p.height <= r.y + r.h
                })
                .collect();
            !(contained.is_empty()
                || (contained.len() == 1
                    && (contained[0].x, contained[0].y, contained[0].width, contained[0].height)
                        == (r.x, r.y, r.w, r.h)))
        });
        if state.regions.is_empty() {
            return Some(SawCounts2D::default());
        }
        state.regions.sort();
        if let Some(found) = memo.get(&state) {
            return *found;
        }
        let mut best: Option<SawCounts2D> = None;
        for (index, r) in state.regions.iter().enumerate() {
            for vertical in [true, false] {
                let (lo, length) = if vertical { (r.x, r.w) } else { (r.y, r.h) };
                let hi = lo + length;
                for blade in (i64::from(lo) - i64::from(stock.kerf))..=i64::from(hi) {
                    let far = blade + i64::from(stock.kerf);
                    if !stock.edge_kerf_relief && (blade < i64::from(lo) || far > i64::from(hi)) {
                        continue;
                    }
                    let start = blade.max(i64::from(lo));
                    let end = far.min(i64::from(hi));
                    if start > end || start >= i64::from(hi) || end <= i64::from(lo) {
                        continue;
                    }
                    let start = start as u32;
                    let end = end as u32;
                    let intrusive = parts
                        .iter()
                        .filter(|p| {
                            p.x >= r.x
                                && p.y >= r.y
                                && p.x + p.width <= r.x + r.w
                                && p.y + p.height <= r.y + r.h
                        })
                        .any(|p| {
                            if vertical {
                                p.x + p.width > start && p.x < end
                            } else {
                                p.y + p.height > start && p.y < end
                            }
                        });
                    if intrusive {
                        continue;
                    }
                    for opposite in [false, true] {
                        let distance = if opposite { hi - end } else { start - lo };
                        if distance == 0 {
                            continue;
                        }
                        let orientation = match (vertical, opposite) {
                            (true, false) => 0,
                            (true, true) => 2,
                            (false, false) => 3,
                            (false, true) => 1,
                        };
                        let accessory = match (o.machine, vertical) {
                            (SawMachine2D::PanelSaw, _) => 2,
                            (_, true) => 0,
                            (_, false) => 1,
                        };
                        let mut next = state.clone();
                        next.regions = state
                            .regions
                            .iter()
                            .enumerate()
                            .filter(|(i, _)| *i != index)
                            .map(|(_, r)| r.clone())
                            .collect();
                        for (a, z) in [(lo, start), (end, hi)] {
                            if z > a {
                                next.regions.push(if vertical {
                                    Region { x: a, w: z - a, orientation, ..r.clone() }
                                } else {
                                    Region { y: a, h: z - a, orientation, ..r.clone() }
                                });
                            }
                        }
                        let changed = state.setup != Some((accessory, distance));
                        let initial = state.seen & (1 << accessory) == 0;
                        next.setup = Some((accessory, distance));
                        next.seen |= 1 << accessory;
                        if let Some(mut counts) = visit(job, o, next, memo) {
                            counts.cuts += 1;
                            counts.handling += 1;
                            counts.rotations += usize::from(orientation != r.orientation);
                            counts.initial_setups += usize::from(changed && initial);
                            counts.setup_changes += usize::from(changed && !initial);
                            if best.is_none_or(|old| compare(counts, old, o).is_lt()) {
                                best = Some(counts);
                            }
                        }
                    }
                }
            }
        }
        memo.insert(state, best);
        best
    }
    assert_eq!(job.layouts.len(), 1, "oracle is intentionally one-sheet");
    let l = &job.layouts[0];
    visit(
        job,
        options,
        OracleState {
            regions: vec![Region { x: 0, y: 0, w: l.width, h: l.height, orientation: 0 }],
            setup: None,
            seen: 0,
        },
        &mut BTreeMap::new(),
    )
}

#[test]
fn tiny_schedules_match_independent_exhaustive_oracle() {
    for parts in [
        vec![(0, 0, 1, 2), (1, 0, 1, 2), (2, 0, 1, 2)],
        vec![(0, 0, 1, 1), (1, 0, 2, 1), (0, 1, 3, 1)],
    ] {
        let job = layout_job(3, 2, 0, &parts);
        for weights in [
            SawWeights2D::default(),
            SawWeights2D { setup_change: 0.0, rotation: 20.0, ..Default::default() },
            SawWeights2D { setup_change: 30.0, rotation: 0.0, ..Default::default() },
        ] {
            let options = SawPlanOptions2D {
                machine: SawMachine2D::PanelSaw,
                weights,
                search: SawSearchLimits2D {
                    max_states: 100000,
                    beam_width: 256,
                    ..Default::default()
                },
                ..Default::default()
            };
            let plan = plan_saw_cuts(&job, &options).expect("tiny exhaustive");
            assert_eq!(plan.counts, oracle(&job, &options).expect("oracle schedule"));
            assert_eq!(plan.search.optimality, SawOptimality2D::OptimalWithinCandidateModel);
            replay(&job, &plan);
        }
    }
}

#[test]
fn invalid_weights_limits_and_baseline_exhaustion_are_explicit() {
    let job = strips(0, 1);
    for value in [f64::NAN, f64::INFINITY, -1.0, 1e308] {
        assert!(matches!(
            plan_saw_cuts(
                &job,
                &SawPlanOptions2D {
                    weights: SawWeights2D { cut: value, ..Default::default() },
                    ..Default::default()
                }
            ),
            Err(SawPlanError2D::InvalidOption { .. })
        ));
    }
    assert!(matches!(
        plan_saw_cuts(
            &job,
            &SawPlanOptions2D {
                search: SawSearchLimits2D { beam_width: 0, ..Default::default() },
                ..Default::default()
            }
        ),
        Err(SawPlanError2D::InvalidOption { .. })
    ));
    assert!(matches!(
        plan_saw_cuts(
            &job,
            &SawPlanOptions2D {
                search: SawSearchLimits2D { baseline_max_states: 1, ..Default::default() },
                ..Default::default()
            }
        ),
        Err(SawPlanError2D::BaselineLimit { limit: 1 })
    ));
}

#[test]
fn baseline_backtracks_when_first_nonintrusive_cut_strands_a_narrow_margin() {
    let job = layout_job(13, 12, 2, &[(0, 0, 4, 4), (7, 6, 4, 4)]);
    let plan = plan_saw_cuts(
        &job,
        &SawPlanOptions2D {
            search: SawSearchLimits2D { max_states: 0, ..Default::default() },
            ..Default::default()
        },
    )
    .expect("valid horizontal-first decomposition");
    replay(&job, &plan);
}

#[test]
fn blade_changes_are_rejected_and_reference_side_must_contain_material() {
    let mut job = strips(3, 2);
    job.problem.sheets.push(job.problem.sheets[0].clone());
    job.problem.sheets[1].kerf = 2;
    job.stock_indices[1] = 1;
    assert!(matches!(
        plan_saw_cuts(&job, &SawPlanOptions2D::default()),
        Err(SawPlanError2D::InvalidMapping { .. })
    ));
    let mut job = layout_job(53, 41, 3, &[(2, 0, 21, 41), (26, 0, 27, 41)]);
    job.problem.sheets[0].edge_kerf_relief = true;
    let plan = plan_saw_cuts(&job, &SawPlanOptions2D::default()).expect("relief");
    for op in &plan.operations {
        if let SawAction2D::Cut { setting, .. } = op.action {
            assert!(setting > 0);
        }
    }
}

#[test]
fn placement_orientation_survives_workpiece_turns_and_stock_mapping() {
    let mut job = layout_job(19, 13, 1, &[(2, 2, 7, 9)]);
    job.problem.demands[0].width = 9;
    job.problem.demands[0].height = 7;
    job.problem.demands[0].can_rotate = true;
    job.layouts[0].placements[0].rotated = true;
    let before = job.clone();
    let plan =
        plan_saw_cuts(&job, &SawPlanOptions2D::default()).expect("allowed placement rotation");
    replay(&job, &plan);
    assert_eq!(job, before);
    assert!(plan.counts.rotations > 0);
    let mut second = strips(1, 1);
    second.problem.sheets[0].name = job.problem.sheets[0].name.clone();
    job.problem.sheets.push(second.problem.sheets[0].clone());
    job.problem.demands.push(second.problem.demands[0].clone());
    job.layouts.push(second.layouts[0].clone());
    job.stock_indices.push(1);
    job.demand_indices.push(vec![1; 4]);
    let plan = plan_saw_cuts(&job, &SawPlanOptions2D::default())
        .expect("different stock sizes, same labels");
    replay(&job, &plan);
    job.problem.sheets[1].quantity = Some(0);
    assert!(matches!(
        plan_saw_cuts(&job, &SawPlanOptions2D::default()),
        Err(SawPlanError2D::InvalidMapping { sheet_index: 1, .. })
    ));
}

#[test]
fn malformed_large_coordinates_do_not_wrap_and_too_narrow_gaps_fail() {
    let mut job = strips(3, 1);
    job.layouts[0].placements[0].x = u32::MAX;
    assert!(matches!(
        plan_saw_cuts(&job, &SawPlanOptions2D::default()),
        Err(SawPlanError2D::InvalidGeometry { .. })
    ));
    let job = layout_job(11, 10, 2, &[(0, 0, 5, 10), (6, 0, 5, 10)]);
    assert!(matches!(
        plan_saw_cuts(&job, &SawPlanOptions2D::default()),
        Err(SawPlanError2D::NonGuillotine { .. })
    ));
}

#[test]
fn default_budget_meets_fixed_large_grid_quality_bounds() {
    let parts: Vec<_> = (0..64).map(|i| ((i % 8) * 148, (i / 8) * 100, 145, 97)).collect();
    let job = layout_job(1181, 797, 3, &parts);
    let baseline = plan_saw_cuts(
        &job,
        &SawPlanOptions2D {
            search: SawSearchLimits2D { max_states: 0, ..Default::default() },
            ..Default::default()
        },
    )
    .expect("baseline grid");
    let optimized = plan_saw_cuts(&job, &SawPlanOptions2D::default()).expect("scheduled grid");
    assert!(optimized.counts.setup_changes <= 2);
    assert!(optimized.effort_score <= 168.0);
    assert!(optimized.effort_score <= baseline.effort_score);
    replay(&job, &optimized);
}

#[test]
fn generated_physical_slicing_layouts_replay_with_nonzero_kerf() {
    for seed in 0u32..48 {
        let mut regions = vec![(0u32, 0u32, 71u32, 53u32)];
        for step in 0u32..5 {
            let index = step as usize % regions.len();
            let (x, y, w, h) = regions.swap_remove(index);
            let vertical = (seed + step) % 2 == 0;
            let length = if vertical { w } else { h };
            if length < 7 {
                regions.push((x, y, w, h));
                continue;
            }
            let first = 2 + (seed + step * 7) % (length - 5);
            let second = length - first - 2;
            if vertical {
                regions.extend([(x, y, first, h), (x + first + 2, y, second, h)]);
            } else {
                regions.extend([(x, y, w, first), (x, y + first + 2, w, second)]);
            }
        }
        let job = layout_job(71, 53, 2, &regions);
        let plan = plan_saw_cuts(
            &job,
            &SawPlanOptions2D {
                search: SawSearchLimits2D { max_states: 1000, ..Default::default() },
                ..Default::default()
            },
        )
        .expect("physically constructed layout");
        replay(&job, &plan);
    }
}

#[test]
fn straight_strip_baseline_scales_to_the_supported_part_limit() {
    let count = 512u32;
    let parts: Vec<_> = (0..count).map(|i| (i.saturating_mul(13), 0, 10, 31)).collect();
    let job = layout_job(count.saturating_mul(13) - 3, 31, 3, &parts);
    let options = SawPlanOptions2D {
        search: SawSearchLimits2D { max_states: 0, baseline_max_states: 512, ..Default::default() },
        ..Default::default()
    };
    let plan = plan_saw_cuts(&job, &options)
        .expect("simple strip construction must fit a linear attempt budget");
    assert_eq!(plan.counts.cuts, 511);
    assert_eq!(plan.counts.initial_setups, 1);
    assert_eq!(plan.counts.setup_changes, 0);
    replay(&job, &plan);
}

#[test]
fn replay_rejects_wrong_machine_accessory() {
    let job = strips(3, 1);
    let mut plan = plan_saw_cuts(&job, &Default::default()).expect("plan");
    for op in &mut plan.operations {
        if let SawAction2D::Setup { setup, .. } = &mut op.action {
            setup.kind = SawSetupKind2D::PanelStop;
        }
    }
    assert!(std::panic::catch_unwind(|| replay(&job, &plan)).is_err());
}
#[test]
fn replay_rejects_wrong_blade_width() {
    let job = strips(3, 1);
    let mut plan = plan_saw_cuts(&job, &Default::default()).expect("plan");
    for op in &mut plan.operations {
        if let SawAction2D::Cut { kerf, .. } = &mut op.action {
            *kerf = 100;
        }
    }
    assert!(std::panic::catch_unwind(|| replay(&job, &plan)).is_err());
}
#[test]
fn replay_rejects_shifted_diagram_segments() {
    let job = strips(3, 1);
    let mut plan = plan_saw_cuts(&job, &Default::default()).expect("plan");
    for op in &mut plan.operations {
        if let SawAction2D::Cut { segment, .. } = &mut op.action {
            segment.from_x = 0;
            segment.to_x = 0;
        }
    }
    assert!(std::panic::catch_unwind(|| replay(&job, &plan)).is_err());
}
#[test]
fn replay_rejects_destroyed_ancestry_and_sheet_views() {
    let job = strips(3, 1);
    let mut plan = plan_saw_cuts(&job, &Default::default()).expect("plan");
    plan.sheet_views.clear();
    plan.workpieces.retain(|p| p.finished_part.is_some());
    for p in &mut plan.workpieces {
        p.parent = None;
        p.created_by = None;
        p.usable_offcut = true;
    }
    assert!(std::panic::catch_unwind(|| replay(&job, &plan)).is_err());
}
#[test]
fn replay_rejects_overlapping_children_with_equal_area() {
    let job = layout_job(10, 10, 0, &[(0, 0, 2, 10)]);
    let mut plan = plan_saw_cuts(&job, &Default::default()).expect("plan");
    for op in &mut plan.operations {
        if let SawAction2D::Cut { children, .. } = &mut op.action {
            children.retain(|p| p.bounds.width == 2);
            for x in [2, 4] {
                children.push(WorkpieceId2D {
                    sheet_index: 0,
                    bounds: SawRect2D { x, y: 0, width: 4, height: 10 },
                });
            }
        }
    }
    assert!(std::panic::catch_unwind(|| replay(&job, &plan)).is_err());
}
#[test]
fn replay_rejects_partial_blade_without_permission() {
    let mut job = layout_job(53, 41, 3, &[(2, 0, 21, 41), (26, 0, 27, 41)]);
    job.problem.sheets[0].edge_kerf_relief = true;
    let plan = plan_saw_cuts(&job, &Default::default()).expect("plan");
    job.problem.sheets[0].edge_kerf_relief = false;
    assert!(std::panic::catch_unwind(|| replay(&job, &plan)).is_err());
}

#[test]
fn every_effort_coefficient_is_applied_to_its_own_operation() {
    let job = layout_job(23, 19, 0, &[(0, 0, 5, 9), (5, 0, 8, 8), (13, 0, 2, 7), (13, 7, 2, 2)]);
    let weights = SawWeights2D {
        cut: 3.0,
        initial_setup: 7.0,
        setup_change: 11.0,
        rotation: 13.0,
        handling: 17.0,
    };
    let options = SawPlanOptions2D { weights, ..Default::default() };
    let plan = plan_saw_cuts(&job, &options).expect("distinct operation weights");
    assert_eq!(plan.effective_weights, weights);
    let c = plan.counts;
    assert!(
        c.cuts > 0
            && c.initial_setups > 0
            && c.setup_changes > 0
            && c.rotations > 0
            && c.handling > 0
    );
    assert_eq!(
        plan.effort_score,
        (3 * c.cuts
            + 7 * c.initial_setups
            + 11 * c.setup_changes
            + 13 * c.rotations
            + 17 * c.handling) as f64
    );
    replay(&job, &plan);
}

#[test]
fn whole_stock_parts_and_empty_layouts_have_no_machine_operations() {
    let mut job = layout_job(17, 13, 2, &[(0, 0, 17, 13)]);
    job.layouts.push(SawLayout2D { width: 17, height: 13, placements: vec![] });
    job.stock_indices.push(0);
    job.demand_indices.push(vec![]);
    let plan = plan_saw_cuts(&job, &Default::default()).expect("no cutting required");
    assert!(plan.operations.is_empty());
    assert_eq!(plan.counts, SawCounts2D::default());
    assert_eq!(plan.effort_score, 0.0);
    replay(&job, &plan);
}

#[test]
fn replay_rejects_each_independent_metadata_corruption() {
    let job = strips(3, 1);
    let plan = plan_saw_cuts(&job, &Default::default()).expect("plan");
    for mutation in 0..6 {
        let mut broken = plan.clone();
        match mutation {
            0 => broken.sheet_views.clear(),
            1 => broken.workpieces[1].parent = None,
            2 => broken.workpieces[1].created_by = None,
            3 => broken.workpieces[1].usable_offcut = true,
            4 => broken.workpieces.push(broken.workpieces[0].clone()),
            _ => broken.operations.iter_mut().for_each(|op| op.depends_on = None),
        }
        assert!(
            std::panic::catch_unwind(|| replay(&job, &broken)).is_err(),
            "mutation {mutation} escaped replay"
        );
    }
}

#[test]
fn exhaustive_oracle_covers_kerf_waste_accessories_and_all_objectives() {
    let mut clipped = layout_job(5, 5, 2, &[(1, 0, 4, 5)]);
    clipped.problem.sheets[0].edge_kerf_relief = true;
    let fixtures = [
        layout_job(3, 3, 1, &[(1, 1, 1, 1)]),
        layout_job(3, 3, 1, &[(0, 0, 1, 3), (2, 0, 1, 3)]),
        layout_job(3, 3, 0, &[(0, 0, 2, 1), (0, 1, 1, 2)]),
        clipped,
    ];
    for job in &fixtures {
        for machine in [SawMachine2D::TableSaw, SawMachine2D::PanelSaw] {
            for objective in [
                SawObjective2D::FewestCuts,
                SawObjective2D::FewestSetupChanges,
                SawObjective2D::WeightedEffort,
            ] {
                let options = SawPlanOptions2D {
                    machine,
                    objective,
                    weights: SawWeights2D {
                        cut: 3.0,
                        initial_setup: 7.0,
                        setup_change: 11.0,
                        rotation: 13.0,
                        handling: 17.0,
                    },
                    search: SawSearchLimits2D {
                        max_states: 100000,
                        beam_width: 256,
                        ..Default::default()
                    },
                    ..Default::default()
                };
                let plan = plan_saw_cuts(job, &options).expect("tiny physical schedule");
                assert_eq!(
                    plan.counts,
                    oracle(job, &options).expect("independent optimum"),
                    "{job:?} {options:?}"
                );
                assert_eq!(plan.effective_weights, options.weights);
                replay(job, &plan);
            }
        }
    }
}

#[test]
fn grain_lock_is_checked_even_when_rotated_dimensions_are_correct() {
    let mut job = layout_job(19, 13, 1, &[(2, 2, 9, 7)]);
    job.problem.demands[0].width = 7;
    job.problem.demands[0].height = 9;
    job.layouts[0].placements[0].rotated = true;
    assert!(matches!(
        plan_saw_cuts(&job, &Default::default()),
        Err(SawPlanError2D::InvalidMapping { .. })
    ));
    job.problem.demands[0].can_rotate = true;
    replay(&job, &plan_saw_cuts(&job, &Default::default()).expect("only grain permission changed"));
}

#[test]
fn commuting_sheet_cuts_prune_only_sequential_search_and_preserve_physical_identity() {
    for batch_across_sheets in [false, true] {
        let job = strips(3, 3);
        let options = SawPlanOptions2D {
            batch_across_sheets,
            search: SawSearchLimits2D { max_states: 100000, beam_width: 256, ..Default::default() },
            ..Default::default()
        };
        let started = std::time::Instant::now();
        let plan = plan_saw_cuts(&job, &options).expect("physical commuting cuts");
        eprintln!(
            "batch={batch_across_sheets} elapsed={:?} effort={} search={:?}",
            started.elapsed(),
            plan.effort_score,
            plan.search
        );
        replay(&job, &plan);
        assert!(plan.effort_score <= plan.search.baseline_effort_score);
        let report = serde_json::to_value(&plan.search).expect("report JSON");
        if batch_across_sheets {
            assert_eq!(report["dominated_states"], 0);
            assert_eq!(report["peak_dominance_units"], 0);
            assert_eq!(report["dominance_table_saturated"], false);
        } else {
            assert!(plan.search.dominated_states > 0);
            assert!(plan.search.peak_dominance_units > 0);
            assert!(plan.search.peak_dominance_units <= 65536);
        }
        assert_eq!(plan, plan_saw_cuts(&job, &options).expect("deterministic replay"));
    }
}

#[test]
fn legacy_saw_report_json_defaults_new_pruning_diagnostics() {
    let plan = plan_saw_cuts(&strips(3, 2), &Default::default()).expect("physical saw plan");
    let mut legacy = serde_json::to_value(&plan).expect("serialize current plan");
    let report = legacy["search"].as_object_mut().expect("search report object");
    for field in ["dominated_states", "peak_dominance_units", "dominance_table_saturated"] {
        assert!(report.remove(field).is_some(), "current report must emit {field}");
    }
    let decoded: SawPlan2D =
        serde_json::from_value(legacy).expect("legacy report remains readable");
    assert_eq!(decoded.search.dominated_states, 0);
    assert_eq!(decoded.search.peak_dominance_units, 0);
    assert!(!decoded.search.dominance_table_saturated);
    replay(&strips(3, 2), &decoded);
    assert_eq!(decoded.operations, plan.operations);
    assert_eq!(decoded.counts, plan.counts);
    assert_eq!(decoded.effort_score, plan.effort_score);
}
