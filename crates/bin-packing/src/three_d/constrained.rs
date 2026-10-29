//! Payload-aware, indexed 3D packing with an explicit material objective.

use std::cmp::Ordering;

use serde::{Deserialize, Serialize};

use super::common::{FreeCuboid3D, volume_u64};
use super::model::ItemInstance3D;
use super::{BinLayout3D, Placement3D, Rotation3D, SolverMetrics3D, ThreeDProblem, ThreeDSolution};
use crate::{BinPackingError, Result};

const MAX_ITEMS: usize = 256;
const MAX_DEMANDS: usize = 64;
const MAX_BIN_TYPES: usize = 16;
const MAX_BEAM_WIDTH: usize = 32;
const MAX_SEARCH_STATES: usize = 100_000;
const MAX_FREE_SPACES: usize = 128;
const MAX_MOVES: usize = 16;

/// A 3D packing problem with per-demand weights and per-bin payload limits.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConstrainedProblem3D {
    /// Existing dimensions, rotation permissions, costs and inventory.
    pub problem: ThreeDProblem,
    /// Weight per item of each source demand; empty means all zero.
    #[serde(default)]
    pub item_weights: Vec<u64>,
    /// Maximum payload for each source bin; empty means unlimited.
    #[serde(default)]
    pub bin_max_weights: Vec<Option<u64>>,
}

/// Objective applied after minimizing the number of unplaced items.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PackingObjective3D {
    /// Minimize bin count, then empty volume, then material cost.
    #[default]
    BinCount,
    /// Minimize material cost, then bin count, then empty volume.
    TotalCost,
}

/// Deterministic construction and bounded-search controls.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ConstrainedOptions3D {
    /// Material objective used during construction and final comparison.
    pub objective: PackingObjective3D,
    /// Maximum number of partial candidates retained at each item.
    pub beam_width: usize,
    /// Maximum candidate state expansions after greedy construction.
    pub max_states: usize,
}

impl Default for ConstrainedOptions3D {
    fn default() -> Self {
        Self { objective: PackingObjective3D::BinCount, beam_width: 8, max_states: 4_096 }
    }
}

/// Packing output with source-index provenance and actual payloads.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConstrainedSolution3D {
    /// Geometry and aggregate material usage.
    pub solution: ThreeDSolution,
    /// Source bin index for every layout, in layout order.
    pub bin_indices: Vec<usize>,
    /// Source demand index for every placement, grouped in layout order.
    pub demand_indices: Vec<Vec<usize>>,
    /// Total payload for every layout, in layout order.
    pub bin_weights: Vec<u64>,
    /// Number of candidate state expansions after greedy construction.
    pub explored_states: usize,
    /// Whether a search budget, beam, or geometric shortlist omitted alternatives.
    /// False concerns only the fixed-order free-cuboid candidate model, not a
    /// complete enumeration of all possible physical packings.
    pub search_truncated: bool,
}

/// Solve payload-aware 3D packing without changing the ordinary solver API.
///
/// The better of tight-space and low-corner greedy packings is retained while a
/// bounded fixed-item-order beam explores alternative bin types, rotations and
/// free-cuboid corners. Every
/// candidate checks payload before placement; no unconstrained answer is merely
/// filtered after solving. Missing stock or overweight items produce a partial
/// packing. Unplaced entries are emitted in source-demand order with quantity one.
/// The solver makes no global optimality or physical stacking/support claim.
///
/// Inputs are limited to 256 items, 64 demand types and 16 bin types. Beam width
/// is 1..=32, search states 0..=100,000. `max_states = 0` returns the greedy
/// incumbent. The budget counts cloned candidate successors, not construction,
/// free-space probes, validation, serialization or wall time. Each open bin keeps
/// at most 128 maximal free cuboids and each expansion keeps at most 16 moves.
/// Weight totals must fit `u64`; two items whose combined weight overflows cannot
/// share a bin, even if its payload is unlimited.
///
/// # Errors
/// Returns input or structured constraint errors for malformed arrays, unsupported
/// work limits or aggregate cost overflow, and invalid-solution errors if the
/// independently verified result violates its source contract.
pub fn solve_3d_constrained(
    problem: ConstrainedProblem3D,
    options: ConstrainedOptions3D,
) -> Result<ConstrainedSolution3D> {
    validate_problem(&problem)?;
    if !(1..=MAX_BEAM_WIDTH).contains(&options.beam_width) {
        return Err(constraint("beam_width", "must be in 1..=32"));
    }
    if options.max_states > MAX_SEARCH_STATES {
        return Err(constraint("max_states", "must be at most 100000"));
    }
    // The shared instance orientation helper deduplicates equal extents. Names
    // are cloned once per demand here and only again when emitting final output.
    let demands: Vec<_> = problem
        .problem
        .demands
        .iter()
        .enumerate()
        .map(|(demand_index, demand)| ItemInstance3D {
            demand_index,
            name: demand.name.clone(),
            width: demand.width,
            height: demand.height,
            depth: demand.depth,
            allowed_rotations: demand.allowed_rotations,
        })
        .collect();
    let mut order: Vec<_> = problem
        .problem
        .demands
        .iter()
        .enumerate()
        .flat_map(|(index, demand)| std::iter::repeat_n(index, demand.quantity))
        .collect();
    order.sort_by(|&a, &b| {
        let first = &demands[a];
        let second = &demands[b];
        volume_u64(second.width, second.height, second.depth)
            .cmp(&volume_u64(first.width, first.height, first.depth))
            .then_with(|| problem.weight(b).cmp(&problem.weight(a)))
            .then_with(|| a.cmp(&b))
    });

    let mut best =
        construct(&problem, &demands, &order, options.objective, CandidateRule::TightSpace);
    let alternative =
        construct(&problem, &demands, &order, options.objective, CandidateRule::LowCorner);
    if compare_states(&alternative, &best, options.objective).is_lt() {
        best = alternative;
    }

    let mut explored_states = 0;
    let mut truncated = best.truncated || options.max_states == 0;
    // With one stock type, complete one-bin packing attains the minimum count.
    // Its material cost and waste are also fixed. Both supported full objectives
    // are therefore already attained; running a beam cannot improve them.
    let single_bin_bound_met =
        problem.problem.bins.len() == 1 && best.bins.len() == 1 && best.unplaced.is_empty();
    if options.max_states > 0 && !single_bin_bound_met {
        let mut beam = vec![State::new()];
        'search: for (position, &demand) in order.iter().enumerate() {
            let mut next = Vec::new();
            let complete = position + 1 == order.len();
            for state in &beam {
                let (moves, omitted) = candidate_moves(
                    &problem,
                    &demands[demand],
                    state,
                    options.objective,
                    remaining_copies(&order, position, demand),
                    options.beam_width.saturating_mul(2).min(MAX_MOVES),
                    CandidateRule::TightSpace,
                );
                truncated |= omitted;
                for movement in moves {
                    if explored_states == options.max_states {
                        truncated = true;
                        break 'search;
                    }
                    explored_states += 1;
                    let mut candidate = state.clone();
                    candidate.truncated |= omitted;
                    candidate.place(&problem, demand, movement);
                    if complete {
                        truncated |= candidate.truncated;
                        if compare_states(&candidate, &best, options.objective).is_lt() {
                            best = candidate;
                        }
                    } else {
                        next.push(candidate);
                    }
                }
                // Skipping a feasible item can preserve scarce stock for more
                // later items. It is an explicit alternative, not lost demand.
                if explored_states == options.max_states {
                    truncated = true;
                    break 'search;
                }
                explored_states += 1;
                let mut skipped = state.clone();
                skipped.unplaced.push(demand);
                if complete {
                    truncated |= skipped.truncated;
                    if compare_states(&skipped, &best, options.objective).is_lt() {
                        best = skipped;
                    }
                } else {
                    next.push(skipped);
                }
            }
            if complete {
                break;
            }
            next.sort_by(|a, b| compare_states(a, b, options.objective));
            if next.len() > options.beam_width {
                truncated = true;
                next.truncate(options.beam_width);
            }
            beam = next;
        }
    }
    let mut result = assemble(&problem, best, explored_states, truncated);
    if single_bin_bound_met && options.max_states > 0 {
        result
            .solution
            .metrics
            .notes
            .push("complete single-bin material bound met; beam search skipped".into());
    }
    validate_3d_constrained(&problem, &result)?;
    Ok(result)
}

/// Independently validate a constrained solution against its exact source input.
///
/// # Errors
/// Returns input/constraint validation errors or an invalid-solution error for
/// geometry, source identity, inventory, payload, totals or demand conservation.
/// Unplaced entries must follow source order with quantity one. Complexity is
/// O(P² + P + D + B) for P placements, D demand types and B bin types.
pub fn validate_3d_constrained(
    problem: &ConstrainedProblem3D,
    result: &ConstrainedSolution3D,
) -> Result<()> {
    validate_problem(problem)?;
    let solution = &result.solution;
    let count = solution.layouts.len();
    let item_count: usize = problem.problem.demands.iter().map(|demand| demand.quantity).sum();
    if count > item_count
        || result.explored_states > MAX_SEARCH_STATES
        || result.explored_states != solution.metrics.explored_states
    {
        return Err(invalid("layout count or search diagnostics exceed the source contract"));
    }
    if solution.bin_count != count
        || result.bin_indices.len() != count
        || result.demand_indices.len() != count
        || result.bin_weights.len() != count
    {
        return Err(invalid("layout metadata lengths do not match"));
    }
    if solution.exact
        || solution.lower_bound.is_some()
        || solution.guillotine
        || !solution.bin_requirements.is_empty()
    {
        return Err(invalid("unsupported optimality, guillotine or procurement metadata"));
    }
    let mut counts = vec![0usize; problem.problem.demands.len()];
    let mut stocks = vec![0usize; problem.problem.bins.len()];
    let mut total_waste = 0u64;
    let mut total_cost = 0.0;
    for (layout_index, layout) in solution.layouts.iter().enumerate() {
        let source = result.bin_indices[layout_index];
        let stock =
            problem.problem.bins.get(source).ok_or_else(|| invalid("unknown source bin index"))?;
        if layout.bin_name != stock.name
            || (layout.width, layout.height, layout.depth)
                != (stock.width, stock.height, stock.depth)
            || layout.cost != stock.cost
            || layout.placements.is_empty()
        {
            return Err(invalid("layout does not match its source bin"));
        }
        stocks[source] += 1;
        if stock.quantity.is_some_and(|cap| stocks[source] > cap) {
            return Err(invalid("bin inventory exceeded"));
        }
        let provenance = &result.demand_indices[layout_index];
        if provenance.len() != layout.placements.len() {
            return Err(invalid("placement metadata lengths do not match"));
        }
        let mut weight = 0u64;
        let mut used = 0u64;
        for (placement_index, placement) in layout.placements.iter().enumerate() {
            let demand_index = provenance[placement_index];
            let demand = problem
                .problem
                .demands
                .get(demand_index)
                .ok_or_else(|| invalid("unknown source demand index"))?;
            if placement.name != demand.name
                || !demand.allowed_rotations.contains(placement.rotation)
                || (placement.width, placement.height, placement.depth)
                    != placement.rotation.apply(demand.width, demand.height, demand.depth)
            {
                return Err(invalid("placement does not match its source demand or rotation"));
            }
            if u64::from(placement.x) + u64::from(placement.width) > u64::from(stock.width)
                || u64::from(placement.y) + u64::from(placement.height) > u64::from(stock.height)
                || u64::from(placement.z) + u64::from(placement.depth) > u64::from(stock.depth)
            {
                return Err(invalid("placement extends outside its bin"));
            }
            for previous in &layout.placements[..placement_index] {
                if independent_overlap(placement, previous) {
                    return Err(invalid("placements overlap"));
                }
            }
            counts[demand_index] += 1;
            if counts[demand_index] > demand.quantity {
                return Err(invalid("too many copies of a demand were placed"));
            }
            weight = weight
                .checked_add(problem.weight(demand_index))
                .ok_or_else(|| invalid("bin payload overflows u64"))?;
            used = used
                .checked_add(volume_u64(placement.width, placement.height, placement.depth))
                .ok_or_else(|| invalid("used volume overflows u64"))?;
        }
        if weight != result.bin_weights[layout_index]
            || problem.max_weight(source).is_some_and(|cap| weight > cap)
        {
            return Err(invalid("reported payload or payload limit violated"));
        }
        let volume = volume_u64(stock.width, stock.height, stock.depth);
        if used > volume || layout.used_volume != used || layout.waste_volume != volume - used {
            return Err(invalid("layout volume totals are inconsistent"));
        }
        total_waste += layout.waste_volume;
        total_cost += stock.cost;
    }
    if solution.total_waste_volume != total_waste || solution.total_cost != total_cost {
        return Err(invalid("aggregate waste or material cost is inconsistent"));
    }
    let mut unplaced = solution.unplaced.iter();
    for (demand_index, demand) in problem.problem.demands.iter().enumerate() {
        for _ in counts[demand_index]..demand.quantity {
            let item = unplaced.next().ok_or_else(|| invalid("unplaced demand is missing"))?;
            if item.name != demand.name
                || item.width != demand.width
                || item.height != demand.height
                || item.depth != demand.depth
                || item.quantity != 1
                || item.allowed_rotations != demand.allowed_rotations
            {
                return Err(invalid("unplaced demand does not match source order and quantities"));
            }
        }
    }
    if unplaced.next().is_some() {
        return Err(invalid("unplaced demand contains extra items"));
    }
    Ok(())
}

impl ConstrainedProblem3D {
    fn weight(&self, demand: usize) -> u64 {
        self.item_weights.get(demand).copied().unwrap_or(0)
    }

    fn max_weight(&self, bin: usize) -> Option<u64> {
        self.bin_max_weights.get(bin).copied().flatten()
    }
}

fn constraint(field: &str, reason: &str) -> BinPackingError {
    BinPackingError::InvalidConstraint { field: field.into(), reason: reason.into() }
}

fn invalid(reason: &str) -> BinPackingError {
    BinPackingError::InvalidSolution { reason: reason.into() }
}

pub(super) fn validate_problem(problem: &ConstrainedProblem3D) -> Result<()> {
    problem.problem.validate()?;
    if !problem.item_weights.is_empty()
        && problem.item_weights.len() != problem.problem.demands.len()
    {
        return Err(constraint("item_weights", "must be empty or match the source demand count"));
    }
    if !problem.bin_max_weights.is_empty()
        && problem.bin_max_weights.len() != problem.problem.bins.len()
    {
        return Err(constraint("bin_max_weights", "must be empty or match the source bin count"));
    }
    let count: usize = problem.problem.demands.iter().map(|d| d.quantity).sum();
    if count > MAX_ITEMS
        || problem.problem.demands.len() > MAX_DEMANDS
        || problem.problem.bins.len() > MAX_BIN_TYPES
    {
        return Err(constraint(
            "problem",
            "supports at most 256 items, 64 demands and 16 bin types",
        ));
    }
    // Every nonempty bin uses at least one item. This conservative guard ensures
    // any material-cost sum produced by this bounded solver stays finite.
    if problem.problem.bins.iter().any(|bin| !(((count + 1) as f64) * bin.cost).is_finite()) {
        return Err(constraint("bins.cost", "aggregate material cost could overflow f64"));
    }
    Ok(())
}

fn independent_overlap(first: &Placement3D, second: &Placement3D) -> bool {
    [
        (first.x, first.width, second.x, second.width),
        (first.y, first.height, second.y, second.height),
        (first.z, first.depth, second.z, second.depth),
    ]
    .iter()
    .all(|&(a, length_a, b, length_b)| {
        u64::from(a) < u64::from(b) + u64::from(length_b)
            && u64::from(b) < u64::from(a) + u64::from(length_a)
    })
}

#[derive(Debug, Clone, Copy)]
struct IndexedPlacement {
    demand: usize,
    space: FreeCuboid3D,
    rotation: Rotation3D,
}

#[derive(Debug, Clone)]
struct OpenBin {
    source: usize,
    placements: Vec<IndexedPlacement>,
    spaces: Vec<FreeCuboid3D>,
    weight: u64,
    used: u64,
}

#[derive(Debug, Clone)]
struct State {
    bins: Vec<OpenBin>,
    unplaced: Vec<usize>,
    cost: f64,
    waste: u64,
    truncated: bool,
}

impl State {
    fn new() -> Self {
        Self { bins: Vec::new(), unplaced: Vec::new(), cost: 0.0, waste: 0, truncated: false }
    }

    fn place(&mut self, problem: &ConstrainedProblem3D, demand: usize, movement: Movement) {
        if movement.target == self.bins.len() {
            let source = &problem.problem.bins[movement.source];
            let space = FreeCuboid3D {
                x: 0,
                y: 0,
                z: 0,
                width: source.width,
                height: source.height,
                depth: source.depth,
            };
            self.cost += source.cost;
            self.waste += space.volume();
            self.bins.push(OpenBin {
                source: movement.source,
                placements: Vec::new(),
                spaces: vec![space],
                weight: 0,
                used: 0,
            });
        }
        let bin = &mut self.bins[movement.target];
        // candidate_moves checked this exact addition before creating the move.
        bin.weight = bin.weight.saturating_add(problem.weight(demand));
        let volume = movement.space.volume();
        bin.used += volume;
        self.waste -= volume;
        bin.placements.push(IndexedPlacement {
            demand,
            space: movement.space,
            rotation: movement.rotation,
        });
        self.truncated |= subtract_space(&mut bin.spaces, movement.space);
    }
}

fn compare_states(first: &State, second: &State, objective: PackingObjective3D) -> Ordering {
    first.unplaced.len().cmp(&second.unplaced.len()).then_with(|| match objective {
        PackingObjective3D::BinCount => first
            .bins
            .len()
            .cmp(&second.bins.len())
            .then_with(|| first.waste.cmp(&second.waste))
            .then_with(|| first.cost.total_cmp(&second.cost)),
        PackingObjective3D::TotalCost => first
            .cost
            .total_cmp(&second.cost)
            .then_with(|| first.bins.len().cmp(&second.bins.len()))
            .then_with(|| first.waste.cmp(&second.waste)),
    })
}

#[derive(Debug, Clone, Copy)]
struct Movement {
    target: usize,
    source: usize,
    space: FreeCuboid3D,
    rotation: Rotation3D,
    new_bin: bool,
    capacity: u64,
    residual: u64,
    position_rank: (u32, u32, u32),
    cost: f64,
}

fn compare_moves(first: &Movement, second: &Movement, objective: PackingObjective3D) -> Ordering {
    let rank = match objective {
        PackingObjective3D::BinCount => first
            .new_bin
            .cmp(&second.new_bin)
            .then_with(|| second.capacity.cmp(&first.capacity))
            .then_with(|| first.residual.cmp(&second.residual))
            .then_with(|| first.cost.total_cmp(&second.cost)),
        PackingObjective3D::TotalCost => first
            .cost
            .total_cmp(&second.cost)
            .then_with(|| first.new_bin.cmp(&second.new_bin))
            .then_with(|| second.capacity.cmp(&first.capacity))
            .then_with(|| first.residual.cmp(&second.residual)),
    };
    rank.then_with(|| {
        (first.target, first.source, first.position_rank).cmp(&(
            second.target,
            second.source,
            second.position_rank,
        ))
    })
}

#[derive(Debug, Clone, Copy)]
enum CandidateRule {
    TightSpace,
    LowCorner,
}

// Two fixed deterministic constructions diversify local placements without
// cloning labels or invoking the full ordinary ensemble. Both obey the same
// material objective, source indices, payload limits and inventory accounting.
fn construct(
    problem: &ConstrainedProblem3D,
    demands: &[ItemInstance3D],
    order: &[usize],
    objective: PackingObjective3D,
    rule: CandidateRule,
) -> State {
    let mut result = State::new();
    for (position, &demand) in order.iter().enumerate() {
        let (moves, omitted) = candidate_moves(
            problem,
            &demands[demand],
            &result,
            objective,
            remaining_copies(order, position, demand),
            1,
            rule,
        );
        result.truncated |= omitted;
        if let Some(movement) = moves.first() {
            result.place(problem, demand, *movement);
        } else {
            result.unplaced.push(demand);
        }
    }
    result
}

fn remaining_copies(order: &[usize], start: usize, demand: usize) -> usize {
    order[start..].iter().filter(|&&index| index == demand).count()
}

// O((B + S) * R * F * K), where B open bins, S stock types, R <= 6 legal
// orientations, F <= 128 maximal free cuboids and K <= 16 retained moves.
// Candidate probes borrow labels and copy geometry only; no state is cloned here.
fn candidate_moves(
    problem: &ConstrainedProblem3D,
    item: &ItemInstance3D,
    state: &State,
    objective: PackingObjective3D,
    remaining: usize,
    limit: usize,
    rule: CandidateRule,
) -> (Vec<Movement>, bool) {
    let mut moves: Vec<Movement> = Vec::new();
    let mut omitted = false;
    let weight = problem.weight(item.demand_index);
    let mut inventory = vec![0usize; problem.problem.bins.len()];
    for bin in &state.bins {
        inventory[bin.source] += 1;
    }
    for target in 0..state.bins.len() + problem.problem.bins.len() {
        let existing = state.bins.get(target);
        let source = match existing {
            Some(bin) => bin.source,
            None => target - state.bins.len(),
        };
        let stock = &problem.problem.bins[source];
        if existing.is_none() && stock.quantity.is_some_and(|cap| inventory[source] >= cap) {
            continue;
        }
        let Some(new_weight) = existing.map_or(0, |bin| bin.weight).checked_add(weight) else {
            continue;
        };
        if problem.max_weight(source).is_some_and(|cap| new_weight > cap) {
            continue;
        }
        let initial = [FreeCuboid3D {
            x: 0,
            y: 0,
            z: 0,
            width: stock.width,
            height: stock.height,
            depth: stock.depth,
        }];
        let spaces = existing.map_or(initial.as_slice(), |bin| bin.spaces.as_slice());
        for (rotation, width, height, depth) in item.orientations() {
            let geometry_capacity = (u64::from(stock.width) / u64::from(width))
                * (u64::from(stock.height) / u64::from(height))
                * (u64::from(stock.depth) / u64::from(depth));
            let weight_capacity = problem
                .max_weight(source)
                .and_then(|cap| cap.checked_div(weight))
                .unwrap_or(u64::MAX);
            for space in spaces {
                if !space.fits(width, height, depth) {
                    continue;
                }
                let new_bin = existing.is_none();
                let movement = Movement {
                    target: if new_bin { state.bins.len() } else { target },
                    source,
                    space: FreeCuboid3D {
                        x: space.x,
                        y: space.y,
                        z: space.z,
                        width,
                        height,
                        depth,
                    },
                    rotation,
                    new_bin,
                    capacity: if new_bin {
                        geometry_capacity.min(weight_capacity).min(remaining as u64)
                    } else {
                        0
                    },
                    residual: match rule {
                        CandidateRule::TightSpace => {
                            space.volume() - volume_u64(width, height, depth)
                        }
                        CandidateRule::LowCorner => {
                            initial[0].volume()
                                - existing.map_or(0, |bin| bin.used)
                                - volume_u64(width, height, depth)
                        }
                    },
                    position_rank: match rule {
                        CandidateRule::TightSpace => (space.y, space.z, space.x),
                        CandidateRule::LowCorner => (space.y, space.x, space.z),
                    },
                    cost: if new_bin { stock.cost } else { 0.0 },
                };
                let insertion = moves.partition_point(|candidate| {
                    !compare_moves(candidate, &movement, objective).is_gt()
                });
                if moves.len() < limit {
                    moves.insert(insertion, movement);
                } else {
                    omitted = true;
                    if insertion < limit {
                        moves.insert(insertion, movement);
                        moves.truncate(limit);
                    }
                }
            }
        }
    }
    (moves, omitted)
}

fn spaces_overlap(first: FreeCuboid3D, second: FreeCuboid3D) -> bool {
    first.x < second.x + second.width
        && second.x < first.x + first.width
        && first.y < second.y + second.height
        && second.y < first.y + first.height
        && first.z < second.z + second.depth
        && second.z < first.z + first.depth
}

fn contains(first: FreeCuboid3D, second: FreeCuboid3D) -> bool {
    first.x <= second.x
        && first.y <= second.y
        && first.z <= second.z
        && first.x + first.width >= second.x + second.width
        && first.y + first.height >= second.y + second.height
        && first.z + first.depth >= second.z + second.depth
}

// Maximal-space subtraction emits at most six cuboids per intersecting space,
// then O(F²) containment pruning. Free spaces may overlap each other; they never
// overlap placed items and their volumes are never summed as independent capacity.
// A largest-space shortlist bounds memory and is reported as heuristic pruning.
fn subtract_space(spaces: &mut Vec<FreeCuboid3D>, used: FreeCuboid3D) -> bool {
    let mut next = Vec::with_capacity(spaces.len().saturating_mul(3));
    for free in spaces.drain(..) {
        if !spaces_overlap(free, used) {
            next.push(free);
            continue;
        }
        if used.x > free.x {
            next.push(FreeCuboid3D { width: used.x - free.x, ..free });
        }
        if used.x + used.width < free.x + free.width {
            next.push(FreeCuboid3D {
                x: used.x + used.width,
                width: free.x + free.width - used.x - used.width,
                ..free
            });
        }
        if used.y > free.y {
            next.push(FreeCuboid3D { height: used.y - free.y, ..free });
        }
        if used.y + used.height < free.y + free.height {
            next.push(FreeCuboid3D {
                y: used.y + used.height,
                height: free.y + free.height - used.y - used.height,
                ..free
            });
        }
        if used.z > free.z {
            next.push(FreeCuboid3D { depth: used.z - free.z, ..free });
        }
        if used.z + used.depth < free.z + free.depth {
            next.push(FreeCuboid3D {
                z: used.z + used.depth,
                depth: free.z + free.depth - used.z - used.depth,
                ..free
            });
        }
    }
    next.sort_unstable_by_key(|space| {
        (space.x, space.y, space.z, space.width, space.height, space.depth)
    });
    next.dedup();
    let mut retained = Vec::new();
    for (index, &space) in next.iter().enumerate() {
        if !next
            .iter()
            .enumerate()
            .any(|(other, &container)| other != index && contains(container, space))
        {
            retained.push(space);
        }
    }
    let truncated = retained.len() > MAX_FREE_SPACES;
    if truncated {
        retained.sort_by_key(|space| std::cmp::Reverse(space.volume()));
        retained.truncate(MAX_FREE_SPACES);
    }
    *spaces = retained;
    truncated
}

fn assemble(
    problem: &ConstrainedProblem3D,
    state: State,
    explored_states: usize,
    search_truncated: bool,
) -> ConstrainedSolution3D {
    let mut layouts = Vec::new();
    let mut bin_indices = Vec::new();
    let mut demand_indices = Vec::new();
    let mut bin_weights = Vec::new();
    let mut counts = vec![0usize; problem.problem.demands.len()];
    let mut bins = state.bins;
    bins.sort_by(|a, b| b.used.cmp(&a.used).then_with(|| a.source.cmp(&b.source)));
    for bin in bins {
        let stock = &problem.problem.bins[bin.source];
        bin_indices.push(bin.source);
        bin_weights.push(bin.weight);
        demand_indices.push(bin.placements.iter().map(|p| p.demand).collect());
        let placements = bin
            .placements
            .into_iter()
            .map(|p| {
                counts[p.demand] += 1;
                Placement3D {
                    name: problem.problem.demands[p.demand].name.clone(),
                    x: p.space.x,
                    y: p.space.y,
                    z: p.space.z,
                    width: p.space.width,
                    height: p.space.height,
                    depth: p.space.depth,
                    rotation: p.rotation,
                }
            })
            .collect();
        layouts.push(BinLayout3D {
            bin_name: stock.name.clone(),
            width: stock.width,
            height: stock.height,
            depth: stock.depth,
            cost: stock.cost,
            placements,
            used_volume: bin.used,
            waste_volume: volume_u64(stock.width, stock.height, stock.depth) - bin.used,
        });
    }
    let mut unplaced = Vec::new();
    for (index, demand) in problem.problem.demands.iter().enumerate() {
        for _ in counts[index]..demand.quantity {
            let mut item = demand.clone();
            item.quantity = 1;
            unplaced.push(item);
        }
    }
    let solution = ThreeDSolution {
        algorithm: "constrained_maximal_spaces".into(),
        exact: false,
        lower_bound: None,
        guillotine: false,
        bin_count: layouts.len(),
        total_waste_volume: layouts.iter().map(|l| l.waste_volume).sum(),
        total_cost: layouts.iter().map(|l| l.cost).sum(),
        layouts,
        bin_requirements: Vec::new(),
        unplaced,
        metrics: SolverMetrics3D {
            iterations: 1,
            explored_states,
            notes: vec![
                "payload-aware indexed tight-space/low-corner construction and beam search; no physical support claim"
                    .into(),
            ],
            ..SolverMetrics3D::default()
        },
    };
    ConstrainedSolution3D {
        solution,
        bin_indices,
        demand_indices,
        bin_weights,
        explored_states,
        search_truncated,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::three_d::{Bin3D, BoxDemand3D, RotationMask3D};

    fn weighted_problem() -> ConstrainedProblem3D {
        ConstrainedProblem3D {
            problem: ThreeDProblem {
                bins: vec![Bin3D {
                    name: "carton".into(),
                    width: 10,
                    height: 10,
                    depth: 10,
                    cost: 1.0,
                    quantity: None,
                }],
                demands: vec![BoxDemand3D {
                    name: "item".into(),
                    width: 5,
                    height: 5,
                    depth: 5,
                    quantity: 2,
                    allowed_rotations: RotationMask3D::ALL,
                }],
            },
            item_weights: vec![6],
            bin_max_weights: vec![Some(10)],
        }
    }

    #[test]
    fn terra_batch1_rejects_excess_inventory_in_certificate() {
        let mut problem = weighted_problem();
        let result = solve_3d_constrained(problem.clone(), ConstrainedOptions3D::default())
            .expect("two payload-limited bins");
        assert_eq!(result.solution.bin_count, 2);
        problem.problem.bins[0].quantity = Some(1);
        assert!(matches!(validate_3d_constrained(&problem, &result),
            Err(BinPackingError::InvalidSolution { reason }) if reason == "bin inventory exceeded"));
    }

    #[test]
    fn terra_batch1_rejects_malformed_proof_and_diagnostics() {
        let problem = weighted_problem();
        let valid = solve_3d_constrained(problem.clone(), ConstrainedOptions3D::default())
            .expect("reference certificate");
        let mut cases = Vec::new();
        let mut changed = valid.clone();
        changed.bin_indices.pop();
        cases.push(changed);
        let mut changed = valid.clone();
        changed.demand_indices[0].clear();
        cases.push(changed);
        let mut changed = valid.clone();
        changed.solution.exact = true;
        cases.push(changed);
        let mut changed = valid;
        changed.explored_states = MAX_SEARCH_STATES + 1;
        changed.solution.metrics.explored_states = changed.explored_states;
        cases.push(changed);
        for changed in cases {
            assert!(matches!(
                validate_3d_constrained(&problem, &changed),
                Err(BinPackingError::InvalidSolution { .. })
            ));
        }
    }

    #[test]
    fn terra_batch1_rejects_forbidden_rotation_even_for_equal_extents() {
        let mut problem = weighted_problem();
        problem.problem.demands[0].allowed_rotations = RotationMask3D::XYZ;
        let mut result = solve_3d_constrained(problem.clone(), ConstrainedOptions3D::default())
            .expect("fixed orientation certificate");
        result.solution.layouts[0].placements[0].rotation = Rotation3D::Zxy;
        assert!(matches!(validate_3d_constrained(&problem, &result),
            Err(BinPackingError::InvalidSolution { reason })
                if reason == "placement does not match its source demand or rotation"));
    }

    #[test]
    fn terra_batch1_rejects_unplaced_source_order_changes() {
        let mut problem = weighted_problem();
        problem.bin_max_weights[0] = Some(0);
        problem.problem.demands[0].quantity = 1;
        let mut second = problem.problem.demands[0].clone();
        second.name = "second".into();
        problem.problem.demands.push(second);
        problem.item_weights.push(6);
        let mut result = solve_3d_constrained(problem.clone(), ConstrainedOptions3D::default())
            .expect("unplaced certificate");
        assert_eq!(result.solution.unplaced.len(), 2);
        result.solution.unplaced.swap(0, 1);
        assert!(matches!(validate_3d_constrained(&problem, &result),
            Err(BinPackingError::InvalidSolution { reason })
                if reason == "unplaced demand does not match source order and quantities"));
    }

    #[test]
    fn terra_batch1_checks_coordinate_addition_without_wrapping() {
        let mut problem = weighted_problem();
        problem.problem.bins[0].width = 32_768;
        problem.problem.bins[0].height = 1;
        problem.problem.bins[0].depth = 1;
        problem.problem.demands[0].width = 2;
        problem.problem.demands[0].height = 1;
        problem.problem.demands[0].depth = 1;
        problem.problem.demands[0].quantity = 1;
        let mut result = solve_3d_constrained(problem.clone(), ConstrainedOptions3D::default())
            .expect("boundary certificate");
        result.solution.layouts[0].placements[0].x = 32_766;
        validate_3d_constrained(&problem, &result).expect("exact edge fits");
        result.solution.layouts[0].placements[0].x = u32::MAX - 1;
        assert!(matches!(validate_3d_constrained(&problem, &result),
            Err(BinPackingError::InvalidSolution { reason })
                if reason == "placement extends outside its bin"));
    }

    #[test]
    fn payload_limit_splits_geometrically_fitting_items() {
        let problem = weighted_problem();
        let result = solve_3d_constrained(problem.clone(), ConstrainedOptions3D::default())
            .expect("weighted packing succeeds");
        assert_eq!(result.solution.bin_count, 2, "payload must constrain construction");
        assert_eq!(result.bin_weights, vec![6, 6]);
        validate_3d_constrained(&problem, &result).expect("independent validation");
    }

    #[test]
    fn cost_objective_changes_bin_selection_and_count() {
        let mut problem = weighted_problem();
        problem.item_weights.clear();
        problem.bin_max_weights.clear();
        problem.problem.bins = vec![
            Bin3D {
                name: "small".into(),
                width: 5,
                height: 5,
                depth: 5,
                cost: 1.0,
                quantity: None,
            },
            Bin3D {
                name: "large".into(),
                width: 10,
                height: 5,
                depth: 5,
                cost: 3.0,
                quantity: None,
            },
        ];
        let count_first = solve_3d_constrained(problem.clone(), ConstrainedOptions3D::default())
            .expect("count objective");
        assert_eq!(count_first.solution.bin_count, 1);
        assert_eq!(count_first.solution.total_cost, 3.0);
        let cost_first = solve_3d_constrained(
            problem.clone(),
            ConstrainedOptions3D {
                objective: PackingObjective3D::TotalCost,
                ..ConstrainedOptions3D::default()
            },
        )
        .expect("cost objective");
        assert_eq!(cost_first.solution.bin_count, 2);
        assert_eq!(cost_first.solution.total_cost, 2.0);
        validate_3d_constrained(&problem, &cost_first).expect("independent validation");
    }

    #[test]
    fn absent_and_nonbinding_weights_preserve_simple_geometry() {
        let mut problem = weighted_problem();
        problem.item_weights.clear();
        problem.bin_max_weights.clear();
        let empty = solve_3d_constrained(problem.clone(), ConstrainedOptions3D::default())
            .expect("absent weights");
        assert_eq!(empty.solution.bin_count, 1);
        assert_eq!(empty.bin_weights, vec![0]);
        problem.item_weights = vec![6];
        problem.bin_max_weights = vec![Some(12)];
        let nonbinding = solve_3d_constrained(problem.clone(), ConstrainedOptions3D::default())
            .expect("nonbinding weights");
        assert_eq!(nonbinding.solution.layouts, empty.solution.layouts);
        assert_eq!(nonbinding.bin_weights, vec![12]);
        validate_3d_constrained(&problem, &nonbinding).expect("independent validation");
    }

    #[test]
    fn allowed_rotation_is_used_and_forbidden_rotation_is_not() {
        let mut problem = weighted_problem();
        problem.item_weights.clear();
        problem.bin_max_weights.clear();
        problem.problem.bins[0].width = 4;
        problem.problem.bins[0].height = 2;
        problem.problem.bins[0].depth = 3;
        problem.problem.demands[0] = BoxDemand3D {
            name: "rotated".into(),
            width: 2,
            height: 3,
            depth: 4,
            quantity: 1,
            allowed_rotations: RotationMask3D::ZXY,
        };
        let allowed = solve_3d_constrained(problem.clone(), ConstrainedOptions3D::default())
            .expect("rotation fits");
        assert!(allowed.solution.unplaced.is_empty());
        assert_eq!(allowed.solution.layouts[0].placements[0].rotation, Rotation3D::Zxy);
        problem.problem.demands[0].allowed_rotations = RotationMask3D::XYZ;
        let forbidden = solve_3d_constrained(problem.clone(), ConstrainedOptions3D::default())
            .expect("partial result for impossible orientation");
        assert_eq!(forbidden.solution.bin_count, 0);
        assert_eq!(forbidden.solution.unplaced.len(), 1);
        validate_3d_constrained(&problem, &forbidden).expect("independent validation");
    }

    #[test]
    fn weight_overflow_cannot_wrap_into_a_shared_bin() {
        let mut problem = weighted_problem();
        problem.item_weights[0] = u64::MAX;
        problem.bin_max_weights.clear();
        let result = solve_3d_constrained(problem.clone(), ConstrainedOptions3D::default())
            .expect("separate bins avoid payload overflow");
        assert_eq!(result.solution.bin_count, 2);
        assert_eq!(result.bin_weights, vec![u64::MAX, u64::MAX]);
        validate_3d_constrained(&problem, &result).expect("independent validation");
    }

    #[test]
    fn repeated_labels_retain_independent_weights_and_demand_indices() {
        let mut problem = weighted_problem();
        problem.problem.demands[0].quantity = 1;
        problem.problem.demands.push(BoxDemand3D {
            name: "item".into(),
            width: 2,
            height: 2,
            depth: 2,
            quantity: 1,
            allowed_rotations: RotationMask3D::XYZ,
        });
        problem.item_weights.push(1);
        problem.bin_max_weights[0] = Some(6);
        let result = solve_3d_constrained(problem.clone(), ConstrainedOptions3D::default())
            .expect("duplicate labels supported by source indices");
        assert_eq!(result.solution.bin_count, 2);
        let mut sources: Vec<_> = result.demand_indices.iter().flatten().copied().collect();
        sources.sort_unstable();
        assert_eq!(sources, vec![0, 1]);
        validate_3d_constrained(&problem, &result).expect("independent validation");
        let mut corrupted = result;
        corrupted.demand_indices[1][0] = 0;
        assert!(matches!(
            validate_3d_constrained(&problem, &corrupted),
            Err(BinPackingError::InvalidSolution { .. })
        ));
    }

    #[test]
    fn stock_shortage_and_overweight_items_return_conserved_partials() {
        let mut problem = weighted_problem();
        problem.problem.bins[0].quantity = Some(1);
        let partial = solve_3d_constrained(problem.clone(), ConstrainedOptions3D::default())
            .expect("stock-limited partial");
        assert_eq!(partial.solution.bin_count, 1);
        assert_eq!(partial.solution.unplaced.len(), 1);
        validate_3d_constrained(&problem, &partial).expect("independent validation");
        problem.bin_max_weights[0] = Some(5);
        let overweight = solve_3d_constrained(problem.clone(), ConstrainedOptions3D::default())
            .expect("overweight partial");
        assert_eq!(overweight.solution.bin_count, 0);
        assert_eq!(overweight.solution.unplaced.len(), 2);
        validate_3d_constrained(&problem, &overweight).expect("independent validation");
    }

    #[test]
    fn budget_exhaustion_returns_greedy_incumbent_with_diagnostics() {
        let problem = weighted_problem();
        let baseline = solve_3d_constrained(
            problem.clone(),
            ConstrainedOptions3D { max_states: 0, ..ConstrainedOptions3D::default() },
        )
        .expect("zero-budget baseline");
        assert_eq!(baseline.explored_states, 0);
        assert!(baseline.search_truncated);
        let limited = solve_3d_constrained(
            problem.clone(),
            ConstrainedOptions3D { max_states: 1, ..ConstrainedOptions3D::default() },
        )
        .expect("one-state incumbent");
        assert_eq!(limited.solution.layouts, baseline.solution.layouts);
        assert_eq!(limited.explored_states, 1);
        assert!(limited.search_truncated);
        validate_3d_constrained(&problem, &limited).expect("independent validation");
    }

    #[test]
    fn final_frontier_improvement_survives_mid_frontier_budget_exhaustion() {
        let mut problem = weighted_problem();
        problem.item_weights.clear();
        problem.bin_max_weights.clear();
        problem.problem.bins = vec![
            Bin3D {
                name: "small".into(),
                width: 5,
                height: 5,
                depth: 5,
                cost: 1.0,
                quantity: None,
            },
            Bin3D {
                name: "large".into(),
                width: 10,
                height: 5,
                depth: 5,
                cost: 3.0,
                quantity: None,
            },
        ];
        problem.problem.demands[0].quantity = 1;
        problem.problem.demands.push(BoxDemand3D {
            name: "second".into(),
            width: 4,
            height: 5,
            depth: 5,
            quantity: 1,
            allowed_rotations: RotationMask3D::ALL,
        });
        let baseline = solve_3d_constrained(
            problem.clone(),
            ConstrainedOptions3D { max_states: 0, ..ConstrainedOptions3D::default() },
        )
        .expect("greedy baseline");
        assert_eq!(baseline.solution.bin_count, 2);
        let improved = solve_3d_constrained(
            problem.clone(),
            ConstrainedOptions3D { max_states: 11, ..ConstrainedOptions3D::default() },
        )
        .expect("last-frontier improvement");
        assert_eq!(improved.solution.bin_count, 1);
        assert_eq!(improved.explored_states, 11);
        assert!(improved.search_truncated);
        validate_3d_constrained(&problem, &improved).expect("independent validation");
    }

    #[test]
    fn malformed_arrays_controls_and_aggregate_cost_are_structured_errors() {
        let problem = weighted_problem();
        let mut malformed = problem.clone();
        malformed.item_weights.push(0);
        assert!(matches!(solve_3d_constrained(malformed, ConstrainedOptions3D::default()),
            Err(BinPackingError::InvalidConstraint { field, .. }) if field == "item_weights"));
        let mut malformed = problem.clone();
        malformed.bin_max_weights.push(None);
        assert!(matches!(solve_3d_constrained(malformed, ConstrainedOptions3D::default()),
            Err(BinPackingError::InvalidConstraint { field, .. }) if field == "bin_max_weights"));
        for options in [
            ConstrainedOptions3D { beam_width: 0, ..ConstrainedOptions3D::default() },
            ConstrainedOptions3D {
                max_states: MAX_SEARCH_STATES + 1,
                ..ConstrainedOptions3D::default()
            },
        ] {
            assert!(matches!(
                solve_3d_constrained(problem.clone(), options),
                Err(BinPackingError::InvalidConstraint { .. })
            ));
        }
        let mut malformed = problem.clone();
        malformed.problem.bins[0].cost = f64::MAX;
        assert!(matches!(solve_3d_constrained(malformed, ConstrainedOptions3D::default()),
            Err(BinPackingError::InvalidConstraint { field, .. }) if field == "bins.cost"));
        let mut oversized = problem;
        oversized.problem.demands[0].quantity = MAX_ITEMS + 1;
        assert!(matches!(solve_3d_constrained(oversized, ConstrainedOptions3D::default()),
            Err(BinPackingError::InvalidConstraint { field, .. }) if field == "problem"));
        for (demands, bins) in [(MAX_DEMANDS + 1, 1), (1, MAX_BIN_TYPES + 1)] {
            let mut oversized = weighted_problem();
            oversized.item_weights.clear();
            oversized.bin_max_weights.clear();
            oversized.problem.demands = vec![oversized.problem.demands[0].clone(); demands];
            for demand in &mut oversized.problem.demands {
                demand.quantity = 1;
            }
            oversized.problem.bins = (0..bins)
                .map(|index| Bin3D {
                    name: format!("stock {index}"),
                    ..oversized.problem.bins[0].clone()
                })
                .collect();
            assert!(matches!(solve_3d_constrained(oversized, ConstrainedOptions3D::default()),
                Err(BinPackingError::InvalidConstraint { field, .. }) if field == "problem"));
        }
    }

    #[test]
    fn validator_rejects_forged_payload_geometry_totals_and_accounting() {
        let mut problem = weighted_problem();
        problem.bin_max_weights[0] = Some(12);
        let valid = solve_3d_constrained(problem.clone(), ConstrainedOptions3D::default())
            .expect("valid reference");
        let mut cases = Vec::new();
        let mut changed = valid.clone();
        changed.bin_weights[0] = 0;
        cases.push(changed);
        let mut changed = valid.clone();
        changed.solution.layouts[0].placements[0].x = u32::MAX;
        cases.push(changed);
        let mut changed = valid.clone();
        changed.solution.layouts[0].placements[1] =
            changed.solution.layouts[0].placements[0].clone();
        cases.push(changed);
        let mut changed = valid.clone();
        changed.solution.total_cost = 0.0;
        cases.push(changed);
        let mut changed = valid.clone();
        changed.solution.total_waste_volume += 1;
        cases.push(changed);
        let mut changed = valid.clone();
        changed.solution.layouts[0].used_volume += 1;
        cases.push(changed);
        let mut changed = valid.clone();
        changed.solution.unplaced.push(problem.problem.demands[0].clone());
        cases.push(changed);
        let mut changed = valid.clone();
        changed.bin_indices[0] = 1;
        cases.push(changed);
        let mut changed = valid.clone();
        changed.demand_indices[0][0] = 1;
        cases.push(changed);
        let mut changed = valid.clone();
        changed.explored_states += 1;
        cases.push(changed);
        for changed in cases {
            assert!(matches!(
                validate_3d_constrained(&problem, &changed),
                Err(BinPackingError::InvalidSolution { .. })
            ));
        }

        validate_3d_constrained(&problem, &valid).expect("untampered reference");
        type Mutation = (&'static str, &'static str, fn(&mut ConstrainedSolution3D));
        let mutations: &[Mutation] = &[
            (
                "more loaded bins than requested copies",
                "layout count or search diagnostics exceed the source contract",
                |s| {
                    s.solution.layouts = vec![s.solution.layouts[0].clone(); 3];
                    s.solution.bin_count = 3;
                    s.bin_indices = vec![0; 3];
                    s.demand_indices = vec![vec![0]; 3];
                    s.bin_weights = vec![6; 3];
                },
            ),
            ("bin count", "layout metadata lengths do not match", |s| s.solution.bin_count += 1),
            ("demand vector", "layout metadata lengths do not match", |s| s.demand_indices.clear()),
            ("payload vector", "layout metadata lengths do not match", |s| s.bin_weights.clear()),
            ("lower bound", "unsupported optimality, guillotine or procurement metadata", |s| {
                s.solution.lower_bound = Some(0.0)
            }),
            ("guillotine", "unsupported optimality, guillotine or procurement metadata", |s| {
                s.solution.guillotine = true
            }),
            (
                "unsupported procurement claim",
                "unsupported optimality, guillotine or procurement metadata",
                |s| {
                    s.solution.bin_requirements.push(crate::three_d::BinRequirement3D {
                        bin_name: "carton".into(),
                        bin_width: 10,
                        bin_height: 10,
                        bin_depth: 10,
                        cost: 1.0,
                        available_quantity: None,
                        used_quantity: 1,
                        required_quantity: 1,
                        additional_quantity_needed: 0,
                    })
                },
            ),
            ("stock name", "layout does not match its source bin", |s| {
                s.solution.layouts[0].bin_name.push('!')
            }),
            ("stock height", "layout does not match its source bin", |s| {
                s.solution.layouts[0].height += 1
            }),
            ("stock cost", "layout does not match its source bin", |s| {
                s.solution.layouts[0].cost += 1.0
            }),
            ("empty layout", "layout does not match its source bin", |s| {
                s.solution.layouts[0].placements.clear()
            }),
            ("item name", "placement does not match its source demand or rotation", |s| {
                s.solution.layouts[0].placements[0].name.push('!')
            }),
            ("y bound", "placement extends outside its bin", |s| {
                s.solution.layouts[0].placements[0].y = u32::MAX
            }),
            ("z bound", "placement extends outside its bin", |s| {
                s.solution.layouts[0].placements[0].z = u32::MAX
            }),
            ("waste volume", "layout volume totals are inconsistent", |s| {
                s.solution.layouts[0].waste_volume += 1
            }),
        ];
        for &(label, expected, mutate) in mutations {
            let mut forged = valid.clone();
            mutate(&mut forged);
            assert!(
                matches!(validate_3d_constrained(&problem, &forged),
                Err(BinPackingError::InvalidSolution { reason }) if reason == expected),
                "{label}"
            );
        }
        let mut fewer = problem.clone();
        fewer.problem.demands[0].quantity = 1;
        assert!(matches!(validate_3d_constrained(&fewer, &valid),
            Err(BinPackingError::InvalidSolution { reason }) if reason == "too many copies of a demand were placed"));
        let mut lighter = problem.clone();
        lighter.bin_max_weights[0] = Some(11);
        assert!(matches!(validate_3d_constrained(&lighter, &valid),
            Err(BinPackingError::InvalidSolution { reason }) if reason == "reported payload or payload limit violated"));

        let mut blocked = problem;
        blocked.bin_max_weights[0] = Some(5);
        let partial = solve_3d_constrained(blocked.clone(), ConstrainedOptions3D::default())
            .expect("all-unplaced reference");
        validate_3d_constrained(&blocked, &partial).expect("valid conserved partial");
        assert_eq!(partial.solution.unplaced.len(), 2);
        for axis in 0..5 {
            let mut forged = partial.clone();
            let item = &mut forged.solution.unplaced[0];
            match axis {
                0 => item.width += 1,
                1 => item.height += 1,
                2 => item.depth += 1,
                3 => item.quantity += 1,
                _ => item.allowed_rotations = RotationMask3D::XYZ,
            }
            assert!(
                matches!(validate_3d_constrained(&blocked, &forged),
                Err(BinPackingError::InvalidSolution { reason }) if reason == "unplaced demand does not match source order and quantities"),
                "unplaced field {axis}"
            );
        }
    }

    #[test]
    fn unit_items_match_independent_geometry_and_payload_capacity() {
        for weight in 0..=4u64 {
            for payload in 0..=7u64 {
                let mut problem = weighted_problem();
                problem.problem.bins[0].width = 3;
                problem.problem.bins[0].height = 2;
                problem.problem.bins[0].depth = 2;
                problem.problem.bins[0].quantity = Some(2);
                problem.problem.demands[0].width = 1;
                problem.problem.demands[0].height = 1;
                problem.problem.demands[0].depth = 1;
                problem.problem.demands[0].quantity = 15;
                problem.item_weights[0] = weight;
                problem.bin_max_weights[0] = Some(payload);
                let per_bin =
                    payload.checked_div(weight).map_or(12, |capacity| capacity.min(12)) as usize;
                let expected_placed = 15.min(2 * per_bin);
                let result = solve_3d_constrained(
                    problem.clone(),
                    ConstrainedOptions3D { max_states: 0, ..ConstrainedOptions3D::default() },
                )
                .expect("bounded unit packing");
                assert_eq!(15 - result.solution.unplaced.len(), expected_placed);
                validate_3d_constrained(&problem, &result).expect("independent validation");
            }
        }
    }

    #[test]
    fn serde_defaults_and_roundtrip_keep_payloads_and_source_maps() {
        let problem = weighted_problem();
        let defaults: ConstrainedOptions3D = serde_json::from_str("{}").expect("default options");
        assert_eq!(defaults, ConstrainedOptions3D::default());
        let result = solve_3d_constrained(problem.clone(), defaults).expect("weighted result");
        let encoded = serde_json::to_string(&result).expect("serialize result");
        let decoded: ConstrainedSolution3D = serde_json::from_str(&encoded).expect("decode result");
        assert_eq!(decoded, result);
        validate_3d_constrained(&problem, &decoded).expect("decoded output validates");
        assert_eq!(
            serde_json::to_value(PackingObjective3D::TotalCost).expect("objective"),
            serde_json::json!("total_cost")
        );
    }

    #[test]
    fn mixed_fixed_and_rotatable_boxes_fit_one_bin_at_small_search_budget() {
        let dimensions = [
            (6, 2, 4, 2, RotationMask3D::XYZ),
            (6, 4, 5, 2, RotationMask3D::XYZ),
            (2, 2, 8, 2, RotationMask3D::ALL),
            (3, 8, 4, 1, RotationMask3D::ALL),
            (5, 4, 6, 2, RotationMask3D::ALL),
            (7, 3, 5, 1, RotationMask3D::XYZ),
            (5, 8, 5, 1, RotationMask3D::ALL),
            (7, 5, 4, 2, RotationMask3D::ALL),
        ];
        let problem = ConstrainedProblem3D {
            problem: ThreeDProblem {
                bins: vec![Bin3D {
                    name: "stock".into(),
                    width: 16,
                    height: 12,
                    depth: 10,
                    cost: 1.0,
                    quantity: None,
                }],
                demands: dimensions
                    .into_iter()
                    .enumerate()
                    .map(|(index, (width, height, depth, quantity, allowed_rotations))| {
                        BoxDemand3D {
                            name: format!("part_{index}"),
                            width,
                            height,
                            depth,
                            quantity,
                            allowed_rotations,
                        }
                    })
                    .collect(),
            },
            item_weights: Vec::new(),
            bin_max_weights: Vec::new(),
        };
        let result = solve_3d_constrained(
            problem.clone(),
            ConstrainedOptions3D {
                beam_width: 8,
                max_states: 512,
                ..ConstrainedOptions3D::default()
            },
        )
        .expect("mixed-orientation packing");
        assert!(result.solution.unplaced.is_empty());
        assert_eq!(result.solution.bin_count, 1, "avoid fragmenting room for fixed-axis boxes");
        assert!(result.explored_states <= 512);
        assert_eq!(
            result.explored_states, 0,
            "a complete one-bin incumbent needs no beam improvement"
        );
        validate_3d_constrained(&problem, &result).expect("independently valid one-bin packing");
    }
}
