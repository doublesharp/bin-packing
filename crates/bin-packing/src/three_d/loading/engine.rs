use std::cmp::Ordering;
use std::collections::BTreeSet;

use super::geometry::{self, Addition, FloorRect, item_rule, weight};
use super::{
    LoadingBinLayout3D, LoadingOptions3D, LoadingProblem3D, LoadingSolution3D, PackingObjective3D,
    Rotation3D, StackColumn3D, StackedItem3D, UnloadingConstraint3D, UnplacedLoadingItem3D,
};
use crate::Result;

const MAX_MOVES: usize = 64;

#[derive(Debug, Clone)]
struct State {
    layouts: Vec<LoadingBinLayout3D>,
    remaining: Vec<usize>,
    unplaced: Vec<usize>,
    cost: f64,
}

impl State {
    fn new(problem: &LoadingProblem3D) -> Self {
        Self {
            layouts: Vec::new(),
            remaining: problem.packing.problem.demands.iter().map(|d| d.quantity).collect(),
            unplaced: vec![0; problem.packing.problem.demands.len()],
            cost: 0.0,
        }
    }

    fn skip(&mut self, demand: usize) {
        self.remaining[demand] -= 1;
        self.unplaced[demand] += 1;
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Move {
    demand: usize,
    target: usize,
    new_source: Option<usize>,
    stack: Option<usize>,
    rect: FloorRect,
    rotation: Rotation3D,
    priority: usize,
    capacity: u64,
    residual: u64,
    cost: f64,
}

pub(super) fn solve(
    problem: LoadingProblem3D,
    options: LoadingOptions3D,
) -> Result<LoadingSolution3D> {
    geometry::validate_problem(&problem)?;
    if !(1..=geometry::MAX_BEAM).contains(&options.beam_width)
        || options.max_states > geometry::MAX_STATES
    {
        return Err(geometry::constraint(
            "search",
            "beam_width must be 1..=32 and max_states at most 100000",
        ));
    }
    let total: usize = problem.packing.problem.demands.iter().map(|d| d.quantity).sum();
    let priorities = demand_priorities(&problem);
    let mut best = State::new(&problem);
    let mut truncated = options.max_states == 0;
    while best.remaining.iter().any(|&count| count > 0) {
        let (moves, omitted) = candidate_moves(&problem, &best, &priorities, options.objective);
        truncated |= omitted;
        if let Some(&movement) = moves.first() {
            apply(&problem, &mut best, movement)?;
        } else {
            for (missing, remaining) in best.unplaced.iter_mut().zip(&mut best.remaining) {
                *missing += *remaining;
                *remaining = 0;
            }
        }
    }
    let mut explored = 0usize;
    if options.max_states > 0 {
        let mut beam = vec![State::new(&problem)];
        'search: for depth in 0..total {
            let complete = depth + 1 == total;
            let allowance = ((options.max_states - explored)
                / beam.len().saturating_mul(total - depth).max(1))
            .max(1)
            .min(options.beam_width);
            let mut next = Vec::new();
            for state in &beam {
                let (moves, omitted) =
                    candidate_moves(&problem, state, &priorities, options.objective);
                truncated |= omitted || moves.len() > allowance;
                if moves.is_empty() {
                    let Some(demand) = state.remaining.iter().position(|&count| count > 0) else {
                        continue;
                    };
                    if explored == options.max_states {
                        truncated = true;
                        break 'search;
                    }
                    explored += 1;
                    let mut child = state.clone();
                    child.skip(demand);
                    if complete {
                        if compare(&problem, &child, &best, options.objective).is_lt() {
                            best = child;
                        }
                    } else {
                        next.push(child);
                    }
                } else {
                    for movement in moves.into_iter().take(allowance) {
                        if explored == options.max_states {
                            truncated = true;
                            break 'search;
                        }
                        explored += 1;
                        let mut child = state.clone();
                        apply(&problem, &mut child, movement)?;
                        if complete {
                            if compare(&problem, &child, &best, options.objective).is_lt() {
                                best = child;
                            }
                        } else {
                            next.push(child);
                        }
                    }
                }
            }
            if complete {
                break;
            }
            next.sort_by(|a, b| compare(&problem, a, b, options.objective));
            let mut seen = BTreeSet::new();
            next.retain(|state| seen.insert(state_key(state)));
            if next.len() > options.beam_width {
                truncated = true;
                next.truncate(options.beam_width);
            }
            beam = next;
        }
    }
    let unplaced = best
        .unplaced
        .iter()
        .copied()
        .enumerate()
        .filter(|(_, quantity)| *quantity > 0)
        .map(|(demand_index, quantity)| UnplacedLoadingItem3D { demand_index, quantity })
        .collect();
    let result = LoadingSolution3D {
        bin_count: best.layouts.len(),
        total_cost: best.cost,
        layouts: best.layouts,
        unplaced,
        explored_states: explored,
        search_truncated: truncated,
    };
    super::validate_loading_3d(&problem, &result)?;
    Ok(result)
}

fn demand_priorities(problem: &LoadingProblem3D) -> Vec<usize> {
    let mut indices: Vec<_> = (0..problem.packing.problem.demands.len()).collect();
    indices.sort_by(|&a, &b| {
        let ra = item_rule(problem, a);
        let rb = item_rule(problem, b);
        let group = if problem.unloading == UnloadingConstraint3D::None {
            Ordering::Equal
        } else {
            rb.group_id.cmp(&ra.group_id)
        };
        let da = &problem.packing.problem.demands[a];
        let db = &problem.packing.problem.demands[b];
        group
            .then_with(|| {
                rb.maximum_weight_above
                    .unwrap_or(u64::MAX)
                    .cmp(&ra.maximum_weight_above.unwrap_or(u64::MAX))
            })
            .then_with(|| {
                volume(db.width, db.height, db.depth).cmp(&volume(da.width, da.height, da.depth))
            })
            .then_with(|| weight(problem, b).cmp(&weight(problem, a)))
            .then_with(|| a.cmp(&b))
    });
    let mut priorities = vec![0; indices.len()];
    for (rank, demand) in indices.into_iter().enumerate() {
        priorities[demand] = rank;
    }
    priorities
}

fn compare_moves(a: &Move, b: &Move, objective: PackingObjective3D) -> Ordering {
    let rank = match objective {
        PackingObjective3D::BinCount => a
            .new_source
            .is_some()
            .cmp(&b.new_source.is_some())
            .then_with(|| a.stack.is_none().cmp(&b.stack.is_none()))
            .then_with(|| a.priority.cmp(&b.priority))
            .then_with(|| b.capacity.cmp(&a.capacity))
            .then_with(|| a.cost.total_cmp(&b.cost)),
        PackingObjective3D::TotalCost => (a.cost / a.capacity.max(1) as f64)
            .total_cmp(&(b.cost / b.capacity.max(1) as f64))
            .then_with(|| a.new_source.is_some().cmp(&b.new_source.is_some()))
            .then_with(|| a.stack.is_none().cmp(&b.stack.is_none()))
            .then_with(|| a.priority.cmp(&b.priority)),
    };
    rank.then_with(|| a.residual.cmp(&b.residual)).then_with(|| {
        (a.target, a.new_source, a.stack, a.rect.z, a.rect.x, a.rotation.bit()).cmp(&(
            b.target,
            b.new_source,
            b.stack,
            b.rect.z,
            b.rect.x,
            b.rotation.bit(),
        ))
    })
}

// Keep at most 64 numeric moves. Prefer one best move per remaining demand
// before extra geometric alternatives, so a beam can change stack order.
fn candidate_moves(
    problem: &LoadingProblem3D,
    state: &State,
    priorities: &[usize],
    objective: PackingObjective3D,
) -> (Vec<Move>, bool) {
    let mut moves = Vec::new();
    let mut best_by_demand: Vec<Option<Move>> = vec![None; state.remaining.len()];
    let mut shortlist_omitted = false;
    let mut geometry_omitted = false;
    let mut usage = vec![0usize; problem.packing.problem.bins.len()];
    for layout in &state.layouts {
        usage[layout.bin_index] += 1;
    }
    let mut retain = |movement: Move| {
        let best = &mut best_by_demand[movement.demand];
        if best.as_ref().is_none_or(|old| compare_moves(&movement, old, objective).is_lt()) {
            *best = Some(movement);
        }
        if moves.contains(&movement) {
            return;
        }
        let insertion =
            moves.partition_point(|old| !compare_moves(old, &movement, objective).is_gt());
        if moves.len() < MAX_MOVES {
            moves.insert(insertion, movement);
        } else {
            shortlist_omitted = true;
            if insertion < MAX_MOVES {
                moves.insert(insertion, movement);
                moves.truncate(MAX_MOVES);
            }
        }
    };
    for target in 0..state.layouts.len() + problem.packing.problem.bins.len() {
        let layout = state.layouts.get(target);
        let source = match layout {
            Some(layout) => layout.bin_index,
            None => target - state.layouts.len(),
        };
        let stock = &problem.packing.problem.bins[source];
        if layout.is_none() && stock.quantity.is_some_and(|limit| usage[source] >= limit) {
            continue;
        }
        let stacks = layout.map_or(&[][..], |layout| layout.stacks.as_slice());
        let (free, truncated) = geometry::floor_spaces(problem, source, stacks);
        geometry_omitted |= truncated;
        for (demand, &remaining) in
            state.remaining.iter().enumerate().filter(|(_, remaining)| **remaining > 0)
        {
            let item = &problem.packing.problem.demands[demand];
            let Some(payload) =
                layout.map_or(0, |layout| layout.payload).checked_add(weight(problem, demand))
            else {
                continue;
            };
            if geometry::payload_limit(problem, source).is_some_and(|limit| payload > limit) {
                continue;
            }
            for rotation in [Rotation3D::Xyz, Rotation3D::Zyx] {
                if !item.allowed_rotations.contains(rotation) {
                    continue;
                }
                if rotation == Rotation3D::Zyx
                    && item.width == item.depth
                    && item.allowed_rotations.contains(Rotation3D::Xyz)
                {
                    continue;
                }
                let (width, height, depth) = rotation.apply(item.width, item.height, item.depth);
                if height > stock.height || width > stock.width || depth > stock.depth {
                    continue;
                }
                for (stack_index, stack) in stacks.iter().enumerate() {
                    if (stack.width, stack.depth) != (width, depth)
                        || !can_append(problem, source, stack, demand)
                    {
                        continue;
                    }
                    let rect = geometry::stack_rect(stack);
                    if !unloading_fits(problem, stacks, Some(stack_index), rect, demand)
                        || geometry::axle_loads(
                            problem,
                            source,
                            layout,
                            Some(Addition { demand, rect }),
                        )
                        .is_none()
                    {
                        continue;
                    }
                    retain(Move {
                        demand,
                        target,
                        new_source: None,
                        stack: Some(stack_index),
                        rect,
                        rotation,
                        priority: priorities[demand],
                        capacity: 1,
                        residual: u64::from(stock.height - stack.height),
                        cost: 0.0,
                    });
                }
                for &space in
                    free.iter().filter(|space| width <= space.width && depth <= space.depth)
                {
                    for (x, z) in
                        geometry::anchors(problem, source, layout, demand, space, width, depth)
                    {
                        let rect = FloorRect { x, z, width, depth };
                        if !geometry::density_fits(problem, source, rect, weight(problem, demand))
                            || !unloading_fits(problem, stacks, None, rect, demand)
                            || geometry::axle_loads(
                                problem,
                                source,
                                layout,
                                Some(Addition { demand, rect }),
                            )
                            .is_none()
                        {
                            continue;
                        }
                        let nesting = item_rule(problem, demand).nesting_height;
                        let per_column =
                            1 + u64::from(stock.height - height) / u64::from(height - nesting);
                        let capacity = (u64::from(stock.width) / u64::from(width))
                            * (u64::from(stock.depth) / u64::from(depth))
                            * per_column;
                        retain(Move {
                            demand,
                            target: target.min(state.layouts.len()),
                            new_source: layout.is_none().then_some(source),
                            stack: None,
                            rect,
                            rotation,
                            priority: priorities[demand],
                            capacity: capacity.min(remaining as u64).max(1),
                            residual: space.area() - rect.area(),
                            cost: if layout.is_none() { stock.cost } else { 0.0 },
                        });
                    }
                }
            }
        }
    }
    let mut omitted = shortlist_omitted || geometry_omitted;
    let mut diverse: Vec<_> = best_by_demand.into_iter().flatten().collect();
    diverse.sort_by(|a, b| compare_moves(a, b, objective));
    for movement in moves {
        if !diverse.contains(&movement) {
            if diverse.len() == MAX_MOVES {
                omitted = true;
                break;
            }
            diverse.push(movement);
        }
    }
    (diverse, omitted)
}

fn can_append(
    problem: &LoadingProblem3D,
    source: usize,
    stack: &StackColumn3D,
    demand: usize,
) -> bool {
    let rule = item_rule(problem, demand);
    let Some(first) = stack.items.first() else {
        return false;
    };
    if item_rule(problem, first.demand_index).stackability_id != rule.stackability_id {
        return false;
    }
    let count = stack.items.len() + 1;
    if rule.maximum_stackability.is_some_and(|limit| count > limit)
        || stack.items.iter().any(|item| {
            item_rule(problem, item.demand_index)
                .maximum_stackability
                .is_some_and(|limit| count > limit)
        })
    {
        return false;
    }
    let item = &problem.packing.problem.demands[demand];
    let Some(y) = stack.height.checked_sub(rule.nesting_height) else {
        return false;
    };
    if u64::from(y) + u64::from(item.height)
        > u64::from(problem.packing.problem.bins[source].height)
    {
        return false;
    }
    let Some(mass) = stack.weight.checked_add(weight(problem, demand)) else {
        return false;
    };
    if !geometry::density_fits(problem, source, geometry::stack_rect(stack), mass) {
        return false;
    }
    let mut above = weight(problem, demand);
    for below in stack.items.iter().rev() {
        if item_rule(problem, below.demand_index)
            .maximum_weight_above
            .is_some_and(|limit| above > limit)
        {
            return false;
        }
        let Some(next) = above.checked_add(weight(problem, below.demand_index)) else {
            return false;
        };
        above = next;
    }
    problem.unloading == UnloadingConstraint3D::None
        || stack
            .items
            .last()
            .is_none_or(|below| rule.group_id <= item_rule(problem, below.demand_index).group_id)
}

fn unloading_fits(
    problem: &LoadingProblem3D,
    stacks: &[StackColumn3D],
    own_stack: Option<usize>,
    rect: FloorRect,
    demand: usize,
) -> bool {
    let group = item_rule(problem, demand).group_id;
    stacks.iter().enumerate().filter(|(index, _)| Some(*index) != own_stack).all(|(_, stack)| {
        stack.items.iter().all(|item| {
            geometry::unloading_pair(
                problem.unloading,
                rect,
                group,
                geometry::stack_rect(stack),
                item_rule(problem, item.demand_index).group_id,
            )
        })
    })
}

fn apply(problem: &LoadingProblem3D, state: &mut State, movement: Move) -> Result<()> {
    if let Some(source) = movement.new_source {
        state.cost += problem.packing.problem.bins[source].cost;
        state.layouts.push(LoadingBinLayout3D {
            bin_index: source,
            payload: 0,
            stacks: Vec::new(),
            axle_loads: None,
        });
    }
    let layout = state
        .layouts
        .get_mut(movement.target)
        .ok_or_else(|| geometry::invalid("unknown construction bin"))?;
    layout.payload = layout
        .payload
        .checked_add(weight(problem, movement.demand))
        .ok_or_else(|| geometry::invalid("construction payload overflow"))?;
    if let Some(index) = movement.stack {
        let stack = layout
            .stacks
            .get_mut(index)
            .ok_or_else(|| geometry::invalid("unknown construction stack"))?;
        let y = stack
            .height
            .checked_sub(item_rule(problem, movement.demand).nesting_height)
            .ok_or_else(|| geometry::invalid("nested item extends below floor"))?;
        stack.height = y.saturating_add(problem.packing.problem.demands[movement.demand].height);
        stack.weight = stack
            .weight
            .checked_add(weight(problem, movement.demand))
            .ok_or_else(|| geometry::invalid("construction stack weight overflow"))?;
        stack.items.push(StackedItem3D {
            demand_index: movement.demand,
            y,
            rotation: movement.rotation,
        });
    } else {
        layout.stacks.push(StackColumn3D {
            x: movement.rect.x,
            z: movement.rect.z,
            width: movement.rect.width,
            depth: movement.rect.depth,
            height: problem.packing.problem.demands[movement.demand].height,
            weight: weight(problem, movement.demand),
            items: vec![StackedItem3D {
                demand_index: movement.demand,
                y: 0,
                rotation: movement.rotation,
            }],
        });
    }
    layout.axle_loads = geometry::axle_loads(problem, layout.bin_index, Some(layout), None)
        .ok_or_else(|| geometry::invalid("construction exceeded axle or delivery-stage limits"))?;
    state.remaining[movement.demand] -= 1;
    Ok(())
}

fn volume(width: u32, height: u32, depth: u32) -> u64 {
    u64::from(width) * u64::from(height) * u64::from(depth)
}

fn waste(problem: &LoadingProblem3D, state: &State) -> u64 {
    state
        .layouts
        .iter()
        .map(|layout| {
            let bin = &problem.packing.problem.bins[layout.bin_index];
            volume(bin.width, bin.height, bin.depth).saturating_sub(
                layout
                    .stacks
                    .iter()
                    .map(|stack| volume(stack.width, stack.height, stack.depth))
                    .sum(),
            )
        })
        .sum()
}

fn compare(
    problem: &LoadingProblem3D,
    a: &State,
    b: &State,
    objective: PackingObjective3D,
) -> Ordering {
    a.unplaced.iter().sum::<usize>().cmp(&b.unplaced.iter().sum()).then_with(|| match objective {
        PackingObjective3D::BinCount => a
            .layouts
            .len()
            .cmp(&b.layouts.len())
            .then_with(|| waste(problem, a).cmp(&waste(problem, b)))
            .then_with(|| a.cost.total_cmp(&b.cost)),
        PackingObjective3D::TotalCost => a
            .cost
            .total_cmp(&b.cost)
            .then_with(|| a.layouts.len().cmp(&b.layouts.len()))
            .then_with(|| waste(problem, a).cmp(&waste(problem, b))),
    })
}

type ItemKey = (usize, u32, u8);
type StackKey = (u32, u32, u32, u32, Vec<ItemKey>);
type BinKey = (usize, Vec<StackKey>);
type StateKey = (Vec<BinKey>, Vec<usize>, Vec<usize>);

fn state_key(state: &State) -> StateKey {
    let mut bins: Vec<_> = state
        .layouts
        .iter()
        .map(|layout| {
            let mut stacks: Vec<_> = layout
                .stacks
                .iter()
                .map(|stack| {
                    (
                        stack.x,
                        stack.z,
                        stack.width,
                        stack.depth,
                        stack
                            .items
                            .iter()
                            .map(|item| (item.demand_index, item.y, item.rotation.bit()))
                            .collect(),
                    )
                })
                .collect();
            stacks.sort_unstable();
            (layout.bin_index, stacks)
        })
        .collect();
    bins.sort_unstable();
    (bins, state.remaining.clone(), state.unplaced.clone())
}
