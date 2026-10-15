use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::rc::Rc;

use super::geometry::{Cut, Piece, Split, cut_candidates, split_at, splits};
use super::*;

#[derive(Clone)]
struct State {
    ready: Vec<Rc<Piece>>,
    active_setup: Option<SawSetup2D>,
    used_accessories: u8,
    counts: SawCounts2D,
    trace: Option<Rc<Trace>>,
}

// Ancestry operation IDs are trace-local. They do not change any future move;
// the retained representative supplies its own valid creating-cut dependencies.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct ReadyKey {
    id: WorkpieceId2D,
    stock_index: usize,
    parts: Vec<usize>,
    orientation: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct FutureKey {
    ready: Vec<ReadyKey>,
    next_sheet: Option<usize>,
    active_setup: Option<(SawSetupKind2D, u32)>,
    used_accessories: u8,
}

fn future_key(
    state: &State,
    index: usize,
    split: &Split,
    edge: ReferenceEdge2D,
    job: &SawJob2D,
    options: &SawPlanOptions2D,
) -> FutureKey {
    let change = transition(state, index, split.cut, edge, options);
    let mut ready: Vec<_> = state
        .ready
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != index)
        .map(|(_, p)| ReadyKey {
            id: p.id,
            stock_index: job.stock_indices[p.id.sheet_index],
            parts: p.parts.clone(),
            orientation: p.orientation,
        })
        .collect();
    ready.extend(split.children.iter().filter(|p| !p.terminal(job)).map(|p| ReadyKey {
        id: p.id,
        stock_index: job.stock_indices[p.id.sheet_index],
        parts: p.parts.clone(),
        orientation: change.orientation,
    }));
    ready.sort();
    FutureKey {
        next_sheet: (!options.batch_across_sheets)
            .then(|| ready.first().map(|p| p.id.sheet_index))
            .flatten(),
        ready,
        active_setup: Some((change.setup.kind, change.setup.distance)),
        used_accessories: change.used_accessories,
    }
}

fn dominates(a: SawCounts2D, b: SawCounts2D) -> bool {
    a.cuts <= b.cuts
        && a.initial_setups <= b.initial_setups
        && a.setup_changes <= b.setup_changes
        && a.rotations <= b.rotations
        && a.handling <= b.handling
}

#[derive(Default)]
struct DominanceTable {
    entries: BTreeMap<FutureKey, Vec<SawCounts2D>>,
    units: usize,
    peak_units: usize,
    saturated: bool,
}

impl DominanceTable {
    // For R ready pieces and D total descriptor/part-index units, key construction
    // copies O(D) units and sorts in O(R log R * C), where C bounds one ready-piece
    // descriptor comparison. Tree lookup/insertion costs O(D log K + F) for K keys
    // and an F-entry Pareto front; inserting a new key also copies O(D) units.
    // After admission, the caller filters S retained successors in O(S * D) key
    // comparison work. These costs include variable-sized keys, not just counts.
    // Keys and Pareto fronts consume at most 65,536 scalar storage units, excluding
    // allocator overhead, traces, beam storage and the separate geometry cache.
    // Once full, uncached moves remain eligible: memory pressure never discards a state.
    fn admit(&mut self, key: &FutureKey, counts: SawCounts2D) -> bool {
        if let Some(front) = self.entries.get_mut(key) {
            if front.iter().any(|&old| dominates(old, counts)) {
                return false;
            }
            let removed = front.iter().filter(|&&old| dominates(counts, old)).count();
            let new_units = self.units.saturating_sub(5 * removed).saturating_add(5);
            if new_units > 65536 {
                self.saturated = true;
                return true;
            }
            front.retain(|&old| !dominates(counts, old));
            front.push(counts);
            self.units = new_units;
        } else {
            let units = 16 + key.ready.iter().map(|p| 8 + p.parts.len()).sum::<usize>() + 5;
            if self.entries.len() == 4096 || units > 65536usize.saturating_sub(self.units) {
                self.saturated = true;
                return true;
            }
            self.entries.insert(key.clone(), vec![counts]);
            self.units += units;
        }
        self.peak_units = self.peak_units.max(self.units);
        true
    }
}

struct Trace {
    previous: Option<Rc<Trace>>,
    operations: Vec<SawOperation2D>,
    pieces: Vec<SawWorkpiece2D>,
}

fn score(c: SawCounts2D, w: SawWeights2D) -> f64 {
    c.cuts as f64 * w.cut
        + c.initial_setups as f64 * w.initial_setup
        + c.setup_changes as f64 * w.setup_change
        + c.rotations as f64 * w.rotation
        + c.handling as f64 * w.handling
}

fn compare(a: &State, b: &State, o: &SawPlanOptions2D) -> Ordering {
    compare_counts(a.counts, b.counts, o)
}

fn compare_counts(a: SawCounts2D, b: SawCounts2D, o: &SawPlanOptions2D) -> Ordering {
    let primary = match o.objective {
        SawObjective2D::FewestSetupChanges => {
            (a.initial_setups + a.setup_changes).cmp(&(b.initial_setups + b.setup_changes))
        }
        SawObjective2D::FewestCuts => a.cuts.cmp(&b.cuts),
        SawObjective2D::WeightedEffort => Ordering::Equal,
    };
    primary
        .then_with(|| score(a, o.weights).total_cmp(&score(b, o.weights)))
        .then_with(|| a.cuts.cmp(&b.cuts))
        .then_with(|| {
            (a.initial_setups + a.setup_changes).cmp(&(b.initial_setups + b.setup_changes))
        })
        .then_with(|| a.rotations.cmp(&b.rotations))
        .then_with(|| a.handling.cmp(&b.handling))
}

fn edges(piece: &Piece, cut: Cut) -> impl Iterator<Item = ReferenceEdge2D> {
    let b = piece.id.bounds;
    let k = cut.band;
    let candidates = match cut.axis {
        SawAxis2D::Vertical => [
            (ReferenceEdge2D::Left, k.x > b.x),
            (ReferenceEdge2D::Right, k.x.saturating_add(k.width) < b.x.saturating_add(b.width)),
        ],
        SawAxis2D::Horizontal => [
            (ReferenceEdge2D::Top, k.y > b.y),
            (ReferenceEdge2D::Bottom, k.y.saturating_add(k.height) < b.y.saturating_add(b.height)),
        ],
    };
    candidates.into_iter().filter_map(|(edge, retained)| retained.then_some(edge))
}

#[derive(Default)]
struct SplitCache {
    entries: BTreeMap<WorkpieceId2D, Rc<Vec<Rc<Split>>>>,
    units: usize,
}

// Shared geometry is independent of machine state. Capping the cache at 4096
// workpieces and 65,536 storage units bounds memory; larger misses are not cached.
fn cached_splits(job: &SawJob2D, piece: &Piece, cache: &mut SplitCache) -> Rc<Vec<Rc<Split>>> {
    if let Some(found) = cache.entries.get(&piece.id) {
        return Rc::clone(found);
    }
    let result = Rc::new(splits(job, piece).into_iter().map(Rc::new).collect::<Vec<_>>());
    let units = result
        .iter()
        .map(|s| 16 + s.children.iter().map(|p| 8 + p.parts.len()).sum::<usize>())
        .sum::<usize>();
    if cache.entries.len() < 4096 && units <= 65536usize.saturating_sub(cache.units) {
        cache.units += units;
        cache.entries.insert(piece.id, Rc::clone(&result));
    }
    result
}

struct BaselineFrame {
    state: State,
    choices: Vec<(Cut, ReferenceEdge2D, SawCounts2D)>,
    next: usize,
}

// Iterative DFS attempts one split at a time. The opposite reference edge changes
// effort but never geometric feasibility, so only the cheaper edge is needed here.
fn baseline(
    start: &State,
    job: &SawJob2D,
    o: &SawPlanOptions2D,
) -> Result<(State, usize), SawPlanError2D> {
    let mut state = start.clone();
    let mut stack: Vec<BaselineFrame> = Vec::new();
    let mut evaluated = 0;
    let mut current_sheet = state.ready.first().map_or(0, |p| p.id.sheet_index);
    loop {
        let Some(piece) = state.ready.first() else {
            return Ok((state, evaluated));
        };
        if piece.id.sheet_index != current_sheet {
            stack.clear();
            current_sheet = piece.id.sheet_index;
        }
        let mut choices = Vec::new();
        for cut in cut_candidates(job, piece) {
            if let Some(best) = edges(piece, cut)
                .map(|edge| (cut, edge, transition(&state, 0, cut, edge, o).counts))
                .min_by(|a, b| compare_counts(a.2, b.2, o))
            {
                choices.push(best);
            }
        }
        choices.sort_by(|a, b| compare_counts(a.2, b.2, o));
        stack.push(BaselineFrame { state, choices, next: 0 });
        loop {
            let Some(frame) = stack.last_mut() else {
                return Err(SawPlanError2D::NonGuillotine { sheet_index: current_sheet });
            };
            if let Some(&(cut, edge, _)) = frame.choices.get(frame.next) {
                if evaluated == o.search.baseline_max_states {
                    return Err(SawPlanError2D::BaselineLimit {
                        limit: o.search.baseline_max_states,
                    });
                }
                evaluated += 1;
                frame.next += 1;
                if let Some(split) = split_at(job, &frame.state.ready[0], cut) {
                    state = apply(&frame.state, 0, &split, edge, job, o);
                    break;
                }
            } else {
                stack.pop();
            }
        }
    }
}

fn record(
    piece: &Piece,
    parent: Option<WorkpieceId2D>,
    job: &SawJob2D,
    min: u32,
) -> SawWorkpiece2D {
    SawWorkpiece2D {
        id: piece.id,
        parent,
        created_by: piece.created_by,
        finished_part: piece.finished(job),
        usable_offcut: piece.parts.is_empty()
            && piece.id.bounds.width >= min
            && piece.id.bounds.height >= min,
    }
}

fn initial(job: &SawJob2D, o: &SawPlanOptions2D) -> (State, Vec<SawWorkpiece2D>) {
    let mut ready = Vec::new();
    let mut roots = Vec::new();
    for (sheet_index, layout) in job.layouts.iter().enumerate() {
        let piece = Piece {
            id: WorkpieceId2D {
                sheet_index,
                bounds: SawRect2D { x: 0, y: 0, width: layout.width, height: layout.height },
            },
            parts: (0..layout.placements.len()).collect(),
            orientation: 0,
            created_by: None,
        };
        roots.push(record(&piece, None, job, o.min_usable_side));
        if !piece.terminal(job) {
            ready.push(Rc::new(piece));
        }
    }
    (
        State {
            ready,
            active_setup: None,
            used_accessories: 0,
            counts: SawCounts2D::default(),
            trace: None,
        },
        roots,
    )
}

struct Transition {
    orientation: u8,
    setup: SawSetup2D,
    setup_change: Option<bool>,
    used_accessories: u8,
    counts: SawCounts2D,
}

// Score a move without cloning ready workpieces, geometry, ancestry or traces.
fn transition(
    state: &State,
    index: usize,
    cut: Cut,
    edge: ReferenceEdge2D,
    o: &SawPlanOptions2D,
) -> Transition {
    let piece = &state.ready[index];
    let b = piece.id.bounds;
    let band = cut.band;
    let distance = match edge {
        ReferenceEdge2D::Left => band.x - b.x,
        ReferenceEdge2D::Right => b.x.saturating_add(b.width) - band.x.saturating_add(band.width),
        ReferenceEdge2D::Top => band.y - b.y,
        ReferenceEdge2D::Bottom => {
            b.y.saturating_add(b.height) - band.y.saturating_add(band.height)
        }
    };
    let orientation = match edge {
        ReferenceEdge2D::Left => 0,
        ReferenceEdge2D::Top => 3,
        ReferenceEdge2D::Right => 2,
        ReferenceEdge2D::Bottom => 1,
    };
    let kind = match (o.machine, cut.axis) {
        (SawMachine2D::PanelSaw, _) => SawSetupKind2D::PanelStop,
        (_, SawAxis2D::Vertical) => SawSetupKind2D::RipFence,
        (_, SawAxis2D::Horizontal) => SawSetupKind2D::CrosscutStop,
    };
    let setup = SawSetup2D { kind, distance };
    let mut counts = state.counts;
    counts.cuts += 1;
    counts.handling += 1;
    if orientation != piece.orientation {
        counts.rotations += 1;
    }
    let bit = match kind {
        SawSetupKind2D::RipFence => 1,
        SawSetupKind2D::CrosscutStop => 2,
        SawSetupKind2D::PanelStop => 4,
    };
    let setup_change =
        (state.active_setup != Some(setup)).then_some(state.used_accessories & bit == 0);
    match setup_change {
        Some(true) => counts.initial_setups += 1,
        Some(false) => counts.setup_changes += 1,
        None => {}
    }
    Transition {
        orientation,
        setup,
        setup_change,
        used_accessories: state.used_accessories | bit,
        counts,
    }
}

fn apply(
    state: &State,
    index: usize,
    split: &Split,
    edge: ReferenceEdge2D,
    job: &SawJob2D,
    o: &SawPlanOptions2D,
) -> State {
    let piece = &state.ready[index];
    let b = piece.id.bounds;
    let band = split.cut.band;
    let change = transition(state, index, split.cut, edge, o);
    let orientation = change.orientation;
    let distance = change.setup.distance;
    let mut next = State {
        ready: Vec::new(),
        active_setup: Some(change.setup),
        used_accessories: change.used_accessories,
        counts: change.counts,
        trace: None,
    };
    let c = state.counts;
    let offset = c.cuts + c.initial_setups + c.setup_changes + c.rotations + c.handling;
    let mut operations = Vec::with_capacity(4);
    let mut emit = |action| {
        operations.push(SawOperation2D {
            id: offset + operations.len(),
            workpiece: piece.id,
            depends_on: piece.created_by,
            action,
        })
    };
    emit(SawAction2D::Handle);
    if orientation != piece.orientation {
        emit(SawAction2D::Rotate { quarter_turns: orientation });
    }
    if let Some(initial) = change.setup_change {
        emit(SawAction2D::Setup { setup: change.setup, initial });
    }
    let cut_id = offset + operations.len();
    let mut records = Vec::with_capacity(2);
    next.ready = state
        .ready
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != index)
        .map(|(_, p)| Rc::clone(p))
        .collect();
    for child in &split.children {
        let mut child = child.clone();
        child.orientation = orientation;
        child.created_by = Some(cut_id);
        records.push(record(&child, Some(piece.id), job, o.min_usable_side));
        if !child.terminal(job) {
            next.ready.push(Rc::new(child));
        }
    }
    // Sorting references is cheap and makes enumeration independent of insertion order.
    next.ready.sort_by_key(|p| p.id);
    let segment = match split.cut.axis {
        SawAxis2D::Vertical => SawSegment2D {
            from_x: band.x,
            from_y: b.y,
            to_x: band.x,
            to_y: b.y.saturating_add(b.height),
        },
        SawAxis2D::Horizontal => SawSegment2D {
            from_x: b.x,
            from_y: band.y,
            to_x: b.x.saturating_add(b.width),
            to_y: band.y,
        },
    };
    operations.push(SawOperation2D {
        id: cut_id,
        workpiece: piece.id,
        depends_on: piece.created_by,
        action: SawAction2D::Cut {
            axis: split.cut.axis,
            reference_edge: edge,
            setting: distance,
            kerf: job.problem.sheets[job.stock_indices[piece.id.sheet_index]].kerf,
            kerf_region: band,
            segment,
            children: split.children.iter().map(|p| p.id).collect(),
        },
    });
    next.trace =
        Some(Rc::new(Trace { previous: state.trace.clone(), operations, pieces: records }));
    next
}

struct ScheduledMove {
    index: usize,
    split: Rc<Split>,
    edge: ReferenceEdge2D,
    counts: SawCounts2D,
}

// Schedule the validated baseline tree without enumerating alternate decompositions.
// Candidate scoring shares the optimization budget; only selected states allocate traces.
fn schedule_baseline_tree(
    start: &State,
    baseline: &State,
    job: &SawJob2D,
    o: &SawPlanOptions2D,
    explored: &mut usize,
) -> Option<State> {
    if *explored == o.search.max_states {
        return None;
    }
    let mut tree = BTreeMap::new();
    let mut cursor = baseline.trace.as_deref();
    while let Some(trace) = cursor {
        for op in &trace.operations {
            if let SawAction2D::Cut { axis, kerf_region, .. } = op.action {
                tree.insert(op.workpiece, Cut { axis, band: kerf_region });
            }
        }
        cursor = trace.previous.as_deref();
    }
    let mut tree_splits: BTreeMap<WorkpieceId2D, Rc<Split>> = BTreeMap::new();
    let mut state = start.clone();
    while !state.ready.is_empty() {
        let mut best: Option<ScheduledMove> = None;
        for (index, piece) in state.ready.iter().enumerate() {
            if !o.batch_across_sheets && piece.id.sheet_index != state.ready[0].id.sheet_index {
                break;
            }
            if *explored == o.search.max_states {
                return None;
            }
            let split = if let Some(split) = tree_splits.get(&piece.id) {
                Rc::clone(split)
            } else {
                let split = Rc::new(split_at(job, piece, *tree.get(&piece.id)?)?);
                tree_splits.insert(piece.id, Rc::clone(&split));
                split
            };
            for edge in edges(piece, split.cut) {
                if *explored == o.search.max_states {
                    return None;
                }
                *explored += 1;
                let counts = transition(&state, index, split.cut, edge, o).counts;
                if best.as_ref().is_none_or(|b| compare_counts(counts, b.counts, o).is_lt()) {
                    best = Some(ScheduledMove { index, split: Rc::clone(&split), edge, counts });
                }
            }
        }
        let best = best?;
        state = apply(&state, best.index, &best.split, best.edge, job, o);
    }
    Some(state)
}

// A ranked move borrows its parent and shares cached geometry. Only retained
// moves allocate operation traces and ready-workpiece vectors.
struct BeamMove<'a> {
    key: Option<FutureKey>,
    state: &'a State,
    index: usize,
    split: Rc<Split>,
    edge: ReferenceEdge2D,
    counts: SawCounts2D,
}

pub(super) fn plan(job: &SawJob2D, o: &SawPlanOptions2D) -> Result<SawPlan2D, SawPlanError2D> {
    let (start, roots) = initial(job, o);
    let mut cache = SplitCache::default();
    let (baseline, baseline_states) = baseline(&start, job, o)?;
    let baseline_counts = baseline.counts;
    let baseline_effort_score = score(baseline_counts, o.weights);
    let mut explored_states = 0;
    let scheduled = schedule_baseline_tree(&start, &baseline, job, o, &mut explored_states);
    let mut incumbent = baseline;
    if let Some(scheduled) = scheduled
        && compare(&scheduled, &incumbent, o).is_lt()
    {
        incumbent = scheduled;
    }
    let mut beam = if start.ready.is_empty() { Vec::new() } else { vec![start] };
    let mut budget_exhausted = false;
    let mut beam_pruned = false;
    let mut dominated_states = 0;
    let mut peak_dominance_units = 0;
    let mut dominance_table_saturated = false;
    // Search memory is bounded by beam_width, plus at most one successor batch.
    // Trace prefixes are shared through Rc; layouts and labels are never cloned per state.
    'search: while !beam.is_empty() {
        // A repeated-sheet release fixture was about 8x slower with batched
        // canonical pruning, with no effort improvement. Sequential search
        // benefited, so batched beams bypass key construction and table work.
        let mut dominance = (!o.batch_across_sheets).then(DominanceTable::default);
        let mut successors: Vec<BeamMove<'_>> = Vec::new();
        for state in &beam {
            for (index, piece) in state.ready.iter().enumerate() {
                if !o.batch_across_sheets && piece.id.sheet_index != state.ready[0].id.sheet_index {
                    break;
                }
                if explored_states == o.search.max_states {
                    budget_exhausted = true;
                    break 'search;
                }
                let splits = cached_splits(job, piece, &mut cache);
                for split in splits.iter() {
                    for edge in edges(piece, split.cut) {
                        if explored_states == o.search.max_states {
                            budget_exhausted = true;
                            break 'search;
                        }
                        explored_states += 1;
                        let counts = transition(state, index, split.cut, edge, o).counts;
                        if state.ready.len() == 1 && split.children.iter().all(|p| p.terminal(job))
                        {
                            if compare_counts(counts, incumbent.counts, o).is_lt() {
                                incumbent = apply(state, index, split, edge, job, o);
                            }
                        } else {
                            let key = if let Some(dominance) = dominance.as_mut() {
                                let key = future_key(state, index, split, edge, job, o);
                                let admitted = dominance.admit(&key, counts);
                                dominated_states += usize::from(!admitted);
                                peak_dominance_units =
                                    peak_dominance_units.max(dominance.peak_units);
                                dominance_table_saturated |= dominance.saturated;
                                if !admitted {
                                    continue;
                                }
                                successors.retain(|m| {
                                    m.key.as_ref() != Some(&key) || !dominates(counts, m.counts)
                                });
                                Some(key)
                            } else {
                                None
                            };
                            successors.push(BeamMove {
                                key,
                                state,
                                index,
                                split: Rc::clone(split),
                                edge,
                                counts,
                            });
                        }
                        // Bound retained memory even when one workpiece has many splits.
                        if successors.len() >= o.search.beam_width.saturating_mul(2) {
                            successors.sort_by(|a, b| compare_counts(a.counts, b.counts, o));
                            successors.truncate(o.search.beam_width);
                            beam_pruned = true;
                        }
                    }
                }
            }
        }
        successors.sort_by(|a, b| compare_counts(a.counts, b.counts, o));
        if successors.len() > o.search.beam_width {
            beam_pruned = true;
            successors.truncate(o.search.beam_width);
        }
        beam = successors
            .into_iter()
            .map(|m| apply(m.state, m.index, &m.split, m.edge, job, o))
            .collect();
    }
    let mut traces = Vec::new();
    let mut cursor = incumbent.trace.as_deref();
    while let Some(trace) = cursor {
        traces.push(trace);
        cursor = trace.previous.as_deref();
    }
    let mut operations = Vec::new();
    let mut workpieces = roots;
    for trace in traces.into_iter().rev() {
        operations.extend_from_slice(&trace.operations);
        workpieces.extend_from_slice(&trace.pieces);
    }
    let sheet_views = job
        .stock_indices
        .iter()
        .enumerate()
        .map(|(sheet_index, &stock_index)| SawSheetView2D {
            sheet_index_in_solution: sheet_index,
            stock_index,
            operation_ids: operations
                .iter()
                .filter(|op| op.workpiece.sheet_index == sheet_index)
                .map(|op| op.id)
                .collect(),
        })
        .collect();
    Ok(SawPlan2D {
        schema_version: 1,
        machine: o.machine,
        objective: o.objective,
        batch_across_sheets: o.batch_across_sheets,
        operations,
        workpieces,
        sheet_views,
        counts: incumbent.counts,
        effective_weights: o.weights,
        effort_score: score(incumbent.counts, o.weights),
        search: SawSearchReport2D {
            limits: o.search,
            baseline_states,
            explored_states,
            budget_exhausted,
            beam_pruned,
            dominated_states,
            peak_dominance_units,
            dominance_table_saturated,
            optimality: if budget_exhausted || beam_pruned {
                SawOptimality2D::Heuristic
            } else {
                SawOptimality2D::OptimalWithinCandidateModel
            },
            baseline_effort_score,
            baseline_counts,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key() -> FutureKey {
        FutureKey {
            ready: vec![ReadyKey {
                id: WorkpieceId2D {
                    sheet_index: 0,
                    bounds: SawRect2D { x: 0, y: 0, width: 20, height: 20 },
                },
                stock_index: 0,
                parts: vec![0, 1],
                orientation: 0,
            }],
            next_sheet: Some(0),
            active_setup: Some((SawSetupKind2D::RipFence, 10)),
            used_accessories: 1,
        }
    }

    #[test]
    fn key_distinguishes_every_future_constraint_even_for_equal_dimensions() {
        let original = key();
        for mutation in 0..8 {
            let mut changed = original.clone();
            match mutation {
                0 => changed.ready[0].id.sheet_index = 1,
                1 => changed.ready[0].stock_index = 1,
                2 => changed.ready[0].parts = vec![2, 3],
                3 => changed.ready[0].orientation = 2,
                4 => changed.next_sheet = None,
                5 => changed.active_setup = Some((SawSetupKind2D::CrosscutStop, 10)),
                6 => changed.active_setup = Some((SawSetupKind2D::RipFence, 11)),
                _ => changed.used_accessories = 3,
            }
            assert_ne!(original, changed);
        }
    }

    #[test]
    fn dominance_keeps_tradeoffs_and_bypasses_full_tables() {
        let key = key();
        let mut table = DominanceTable::default();
        let cuts = SawCounts2D { cuts: 2, rotations: 0, ..Default::default() };
        let turns = SawCounts2D { cuts: 1, rotations: 1, ..Default::default() };
        assert!(table.admit(&key, cuts));
        assert!(table.admit(&key, turns));
        assert!(!table.admit(&key, cuts));
        assert_eq!(table.entries[&key].len(), 2);
        let mut oversized = key.clone();
        oversized.ready[0].parts = vec![0; 65536];
        assert!(table.admit(&oversized, cuts));
        assert!(table.saturated);
        assert!(table.peak_units <= 65536);
        assert!(!table.entries.contains_key(&oversized));
    }
}
