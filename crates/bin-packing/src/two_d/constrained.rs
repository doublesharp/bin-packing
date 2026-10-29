//! Native rectangular packing with physical sheet exclusions and explicit objectives.

use std::cmp::Ordering;
use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::model::{ItemInstance2D, Rect};
use super::{Placement2D, SheetLayout2D, SolverMetrics2D, TwoDProblem, TwoDSolution};
use crate::{BinPackingError, Result};

/// An axis-aligned area of defective material in original sheet coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Defect2D {
    /// Left coordinate.
    pub x: u32,
    /// Top coordinate.
    pub y: u32,
    /// Defect width.
    pub width: u32,
    /// Defect height.
    pub height: u32,
}

/// Excluded borders and defects applying to every copy of a sheet type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct SheetConstraints2D {
    /// Unusable left border.
    pub trim_left: u32,
    /// Unusable right border.
    pub trim_right: u32,
    /// Unusable top border.
    pub trim_top: u32,
    /// Unusable bottom border.
    pub trim_bottom: u32,
    /// Defective rectangles, which may overlap one another or the trimmed borders.
    pub defects: Vec<Defect2D>,
}

/// Existing rectangular demands and indexed physical sheet constraints.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConstrainedProblem2D {
    /// Source stock and demand records; their labels need not be unique.
    pub problem: TwoDProblem,
    /// One entry per stock type, or empty to use no trims or defects.
    #[serde(default)]
    pub constraints: Vec<SheetConstraints2D>,
}

/// Ranking applied during construction as well as final candidate selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PackingObjective2D {
    /// Minimize unplaced items, sheet count, waste, then cost.
    #[default]
    BinCount,
    /// Minimize unplaced items, material cost, sheet count, then waste.
    TotalCost,
}

/// Deterministic search controls for the constrained rectangular solver.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ConstrainedOptions2D {
    /// Requested material objective.
    pub objective: PackingObjective2D,
    /// Maximum retained beam states, from 1 through 64.
    pub beam_width: usize,
    /// Maximum charged beam, allocation and repair successors; zero returns the baseline.
    pub max_states: usize,
}

impl Default for ConstrainedOptions2D {
    fn default() -> Self {
        Self { objective: PackingObjective2D::BinCount, beam_width: 8, max_states: 4_000 }
    }
}

/// Constrained layouts with source indices and bounded-search diagnostics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConstrainedSolution2D {
    /// Layouts and totals; no guillotine or global optimality claim is made.
    pub solution: TwoDSolution,
    /// Source sheet index for each layout, in layout order.
    pub stock_indices: Vec<usize>,
    /// Source demand index for each placement, in layout and placement order.
    pub demand_indices: Vec<Vec<usize>>,
    /// Charged search successors and allocation profiles, excluding baseline construction.
    pub explored_states: usize,
    /// Whether a beam-width or effort limit prevented complete candidate search.
    pub search_truncated: bool,
}

/// Pack rectangles while excluding damaged material and trimmed borders.
///
/// A fixed-order beam retains distinct parent families; a completed greedy
/// packing remains the incumbent. Bounded demand-order repair explores maximal
/// free rectangles. Two-sheet repair also tries source-quantity allocations,
/// repeated-item grids and canonical cell search on sheets with zero kerf and
/// no exclusions. That lane accepts at most 4096 quantity combinations and
/// 4096 cells after GCD normalization. Allocation-prefix checks are capped at
/// `16 * max_states` and reported in metrics; retained profiles, CP roots, seed
/// occurrences, placements and explicit void successors share `max_states`.
/// Forced unusable regions propagate within each CP node as geometry updates.
/// This is a heuristic, including when no search budget is exhausted. At most
/// 512 parts, 128 demand types, 16 stocks and 256 defects are accepted overall.
///
/// # Errors
/// Returns input, constraint, or internal candidate validation errors.
pub fn solve_2d_constrained(
    problem: ConstrainedProblem2D,
    options: ConstrainedOptions2D,
) -> Result<ConstrainedSolution2D> {
    validate_problem(&problem)?;
    if !(1..=64).contains(&options.beam_width) || options.max_states > 100_000 {
        return Err(invalid_constraint(
            "search",
            "beam_width must be 1..=64 and max_states at most 100000",
        ));
    }
    let initial_free = initial_free_rectangles(&problem);
    let mut items = Vec::new();
    for (index, demand) in problem.problem.demands.iter().enumerate() {
        items.extend(std::iter::repeat_n(index, demand.quantity));
    }
    items.sort_by(|&left, &right| {
        let left = &problem.problem.demands[left];
        let right = &problem.problem.demands[right];
        (u64::from(right.width) * u64::from(right.height))
            .cmp(&(u64::from(left.width) * u64::from(left.height)))
            .then_with(|| right.width.max(right.height).cmp(&left.width.max(left.height)))
    });

    let mut best = State::new(&problem);
    for &demand in &items {
        let candidates = moves(&problem, &initial_free, &best, demand, options.objective);
        match candidates.first() {
            Some(&candidate) => apply_move(&problem, &initial_free, &mut best, demand, candidate),
            None => best.skip(demand),
        }
    }

    let mut explored_states = 0usize;
    let mut repair_diagnostics = RepairDiagnostics::default();
    let mut search_truncated = options.max_states == 0;
    let repair_enabled = options.objective == PackingObjective2D::BinCount
        && items.len() <= 64
        && problem.problem.demands.len() <= 16
        && best.unplaced.is_empty()
        && best.bins.len() > 1;
    let beam_budget = if repair_enabled { options.max_states / 4 } else { options.max_states };
    if options.max_states > 0 {
        let mut beam = vec![State::new(&problem)];
        'search: for (depth, &demand) in items.iter().enumerate() {
            let mut next = Vec::new();
            let complete = depth + 1 == items.len();
            // Reserve enough effort to carry a retained frontier toward a
            // completed packing, instead of spending the entire budget on early
            // alternatives that can never replace the completed incumbent.
            let per_state = ((beam_budget - explored_states)
                / beam.len().saturating_mul(items.len() - depth).max(1))
            .max(1)
            .min(options.beam_width);
            for (parent, state) in beam.iter().enumerate() {
                let candidates = moves(&problem, &initial_free, state, demand, options.objective);
                search_truncated |= candidates.len() > per_state;
                let mut choices = if candidates.is_empty() {
                    vec![None]
                } else {
                    candidates.into_iter().take(per_state).map(Some).collect()
                };
                if choices.first().is_some_and(Option::is_some) {
                    choices.push(None);
                }
                for choice in choices {
                    if explored_states == beam_budget {
                        search_truncated = true;
                        break 'search;
                    }
                    explored_states += 1;
                    let mut child = state.clone();
                    if let Some(movement) = choice {
                        apply_move(&problem, &initial_free, &mut child, demand, movement);
                    } else {
                        child.skip(demand);
                    }
                    if complete {
                        if compare_states(&child, &best, options.objective).is_lt() {
                            best = child;
                        }
                    } else {
                        next.push((parent, child));
                    }
                }
            }
            if complete {
                break;
            }
            next.sort_by(|left, right| compare_states(&left.1, &right.1, options.objective));
            // Inserting the same set of rectangles in a different order does
            // not merit another beam slot. The key includes source identity and
            // skipped demand, so equal labels never merge different inputs.
            let mut seen = BTreeSet::new();
            next.retain(|(_, candidate)| seen.insert(state_key(candidate)));
            if next.len() > options.beam_width {
                search_truncated = true;
            }
            let mut parents = BTreeSet::new();
            let mut retained = Vec::new();
            let mut alternatives = Vec::new();
            for (parent, candidate) in next {
                if retained.len() < options.beam_width && parents.insert(parent) {
                    retained.push(candidate);
                } else {
                    alternatives.push(candidate);
                }
            }
            retained.extend(alternatives.into_iter().take(options.beam_width - retained.len()));
            beam = retained;
        }
    }
    if repair_enabled && best.bins.len() > 1 && explored_states < options.max_states {
        search_truncated |= repair_bins(
            &problem,
            &initial_free,
            &options,
            &mut best,
            &mut explored_states,
            &mut repair_diagnostics,
        );
    }
    search_truncated |= repair_diagnostics.truncated;
    let mut result = finish(&problem, best, explored_states, search_truncated);
    if repair_diagnostics.allocation_checks > 0 {
        result.solution.metrics.notes.push(format!(
            "two-sheet allocation repair: {} bounded allocation-prefix checks, {} charged profiles, {} canonical calls, {} forced-void geometry propagations; zero kerf/no exclusions, <=4096 GCD-normalized cells, <=4096 quantity combinations; CP roots, seed occurrences, placements and explicit void children are charged to max_states",
            repair_diagnostics.allocation_checks, repair_diagnostics.profiles,
            repair_diagnostics.canonical_calls, repair_diagnostics.forced_void_propagations,
        ));
    }
    validate_2d_constrained(&problem, &result)?;
    Ok(result)
}

/// Independently check geometry, provenance, constraints and exact accounting.
///
/// # Errors
/// Returns an input/constraint error or an invalid-solution error.
pub fn validate_2d_constrained(
    problem: &ConstrainedProblem2D,
    solution: &ConstrainedSolution2D,
) -> Result<()> {
    validate_source(problem)?;
    let result = &solution.solution;
    if result.layouts.len() != solution.stock_indices.len()
        || result.layouts.len() != solution.demand_indices.len()
        || result.sheet_count != result.layouts.len()
        || result.guillotine
    {
        return Err(invalid_solution("layout count, provenance, or guillotine claim"));
    }
    let mut used = vec![0usize; problem.problem.sheets.len()];
    let mut placed = vec![0usize; problem.problem.demands.len()];
    let mut total_waste = 0u64;
    let mut total_kerf = 0u64;
    let mut total_cost = 0.0;
    let mut max_drop = 0u64;
    let mut total_drop_squares = 0u128;
    for ((layout, &stock_index), indices) in
        result.layouts.iter().zip(&solution.stock_indices).zip(&solution.demand_indices)
    {
        let stock = problem
            .problem
            .sheets
            .get(stock_index)
            .ok_or_else(|| invalid_solution("unknown source stock index"))?;
        let constraints = constraints_for(problem, stock_index);
        if layout.placements.is_empty()
            || layout.placements.len() != indices.len()
            || layout.sheet_name != stock.name
            || (layout.width, layout.height) != (stock.width, stock.height)
            || layout.cost != stock.cost
        {
            return Err(invalid_solution("layout metadata or placement provenance"));
        }
        used[stock_index] += 1;
        if stock.quantity.is_some_and(|limit| used[stock_index] > limit) {
            return Err(invalid_solution("source stock inventory exceeded"));
        }
        let mut area = 0u64;
        for (position, (placement, &demand_index)) in
            layout.placements.iter().zip(indices).enumerate()
        {
            let demand = problem
                .problem
                .demands
                .get(demand_index)
                .ok_or_else(|| invalid_solution("unknown source demand index"))?;
            let expected = if placement.rotated {
                (demand.height, demand.width)
            } else {
                (demand.width, demand.height)
            };
            if placement.name != demand.name
                || (placement.width, placement.height) != expected
                || (placement.rotated && !demand.can_rotate)
                || placement.x < constraints.trim_left
                || placement.y < constraints.trim_top
                || u64::from(placement.x) + u64::from(placement.width)
                    > u64::from(stock.width) - u64::from(constraints.trim_right)
                || u64::from(placement.y) + u64::from(placement.height)
                    > u64::from(stock.height) - u64::from(constraints.trim_bottom)
            {
                return Err(invalid_solution("placement dimensions, rotation or trimmed bounds"));
            }
            let rectangle = placement_rect(placement);
            for earlier in &layout.placements[..position] {
                if !separated(rectangle, placement_rect(earlier), stock.kerf) {
                    return Err(invalid_solution("overlapping placements or insufficient kerf"));
                }
            }
            for defect in &constraints.defects {
                if !separated(rectangle, defect_rect(*defect), stock.kerf) {
                    return Err(invalid_solution("placement intersects a defect or its clearance"));
                }
            }
            placed[demand_index] += 1;
            if placed[demand_index] > demand.quantity {
                return Err(invalid_solution("source demand quantity exceeded"));
            }
            area += u64::from(placement.width) * u64::from(placement.height);
        }
        let expected_kerf = super::kerf::kerf_area_for_layout(stock, &layout.placements);
        let (expected_drop, expected_squares) =
            drop_metrics(stock, &constraints, &layout.placements);
        let expected_waste = u64::from(stock.width) * u64::from(stock.height) - area;
        if layout.used_area != area
            || layout.waste_area != expected_waste
            || layout.kerf_area != expected_kerf
            || layout.largest_usable_drop_area != expected_drop
            || layout.sum_sq_usable_drop_areas != expected_squares
        {
            return Err(invalid_solution("incorrect per-layout material totals"));
        }
        total_waste += expected_waste;
        total_kerf += expected_kerf;
        total_cost += stock.cost;
        max_drop = max_drop.max(expected_drop);
        total_drop_squares = total_drop_squares.saturating_add(expected_squares);
    }
    // Repeated identical source labels are intentionally permitted. An unplaced
    // record can consume only the residual quantity of matching source records;
    // placed provenance remains exact even for identical labels and geometry.
    for unplaced in &result.unplaced {
        if unplaced.quantity == 0 {
            return Err(invalid_solution("zero unplaced quantity"));
        }
        let mut remaining = unplaced.quantity;
        for (index, source) in problem.problem.demands.iter().enumerate() {
            if source.name == unplaced.name
                && source.width == unplaced.width
                && source.height == unplaced.height
                && source.can_rotate == unplaced.can_rotate
            {
                let take = remaining.min(source.quantity - placed[index]);
                placed[index] += take;
                remaining -= take;
            }
        }
        if remaining > 0 {
            return Err(invalid_solution("unplaced record lacks matching residual demand"));
        }
    }
    if placed.iter().zip(&problem.problem.demands).any(|(&n, demand)| n != demand.quantity)
        || result.total_waste_area != total_waste
        || result.total_kerf_area != total_kerf
        || !total_cost.is_finite()
        || result.total_cost != total_cost
        || result.max_usable_drop_area != max_drop
        || result.total_sum_sq_usable_drop_areas != total_drop_squares
        || result.metrics.explored_states != solution.explored_states
    {
        return Err(invalid_solution("incomplete demand accounting or incorrect solution totals"));
    }
    Ok(())
}

fn invalid_constraint(field: &str, reason: &str) -> BinPackingError {
    BinPackingError::InvalidConstraint { field: field.into(), reason: reason.into() }
}

fn invalid_solution(reason: &str) -> BinPackingError {
    BinPackingError::InvalidSolution { reason: reason.into() }
}

pub(super) fn validate_problem(problem: &ConstrainedProblem2D) -> Result<()> {
    validate_source(problem)?;
    problem.problem.ensure_feasible_demands()
}

pub(super) fn validate_source(problem: &ConstrainedProblem2D) -> Result<()> {
    problem.problem.validate_source(true)?;
    let quantity = problem.problem.demands.iter().map(|demand| demand.quantity).sum::<usize>();
    let defects = problem.constraints.iter().map(|entry| entry.defects.len()).sum::<usize>();
    if quantity > 512
        || problem.problem.demands.len() > 128
        || problem.problem.sheets.len() > 16
        || defects > 256
    {
        return Err(invalid_constraint(
            "problem",
            "supports at most 512 parts, 128 demand types, 16 stocks and 256 defects",
        ));
    }
    if !problem.constraints.is_empty() && problem.constraints.len() != problem.problem.sheets.len()
    {
        return Err(invalid_constraint("constraints", "must be empty or match stock count"));
    }
    for (index, sheet) in problem.problem.sheets.iter().enumerate() {
        if sheet.edge_kerf_relief {
            return Err(invalid_constraint(
                "edge_kerf_relief",
                "physical constrained packing requires false",
            ));
        }
        if u128::from(sheet.width) * u128::from(sheet.height) * quantity as u128
            > u128::from(u64::MAX)
            || !(sheet.cost * quantity as f64).is_finite()
        {
            return Err(invalid_constraint("totals", "aggregate area or cost would overflow"));
        }
        let constraints = constraints_for(problem, index);
        if u64::from(constraints.trim_left) + u64::from(constraints.trim_right)
            >= u64::from(sheet.width)
            || u64::from(constraints.trim_top) + u64::from(constraints.trim_bottom)
                >= u64::from(sheet.height)
        {
            return Err(invalid_constraint("trims", "must leave positive width and height"));
        }
        for defect in constraints.defects {
            if defect.width == 0
                || defect.height == 0
                || u64::from(defect.x) + u64::from(defect.width) > u64::from(sheet.width)
                || u64::from(defect.y) + u64::from(defect.height) > u64::from(sheet.height)
            {
                return Err(invalid_constraint("defects", "must be positive and inside stock"));
            }
        }
    }
    Ok(())
}

fn constraints_for(problem: &ConstrainedProblem2D, index: usize) -> SheetConstraints2D {
    problem.constraints.get(index).cloned().unwrap_or_default()
}

fn defect_rect(defect: Defect2D) -> Rect {
    Rect { x: defect.x, y: defect.y, width: defect.width, height: defect.height }
}

fn placement_rect(placement: &Placement2D) -> Rect {
    Rect { x: placement.x, y: placement.y, width: placement.width, height: placement.height }
}

/// Widened independent disjointness check, including the required blade gap.
fn separated(left: Rect, right: Rect, kerf: u32) -> bool {
    let kerf = u64::from(kerf);
    u64::from(left.x) + u64::from(left.width) + kerf <= u64::from(right.x)
        || u64::from(right.x) + u64::from(right.width) + kerf <= u64::from(left.x)
        || u64::from(left.y) + u64::from(left.height) + kerf <= u64::from(right.y)
        || u64::from(right.y) + u64::from(right.height) + kerf <= u64::from(left.y)
}

fn padded(rect: Rect, kerf: u32) -> Rect {
    let x = rect.x.saturating_sub(kerf);
    let y = rect.y.saturating_sub(kerf);
    Rect {
        x,
        y,
        width: rect.x.saturating_add(rect.width).saturating_add(kerf).saturating_sub(x),
        height: rect.y.saturating_add(rect.height).saturating_add(kerf).saturating_sub(y),
    }
}

/// Split maximal free rectangles around a forbidden region, then remove contained
/// pieces. O(F²) containment pruning; candidate rectangle count is O(P² + D²)
/// for P placed parts and D defects. No free-space areas are summed as disjoint.
fn subtract(free: &mut Vec<Rect>, blocked: Rect) {
    let mut next = Vec::new();
    for &rect in free.iter() {
        if !rect.intersects(blocked) {
            next.push(rect);
            continue;
        }
        let right = rect.x.saturating_add(rect.width);
        let bottom = rect.y.saturating_add(rect.height);
        let blocked_right = blocked.x.saturating_add(blocked.width);
        let blocked_bottom = blocked.y.saturating_add(blocked.height);
        if blocked.x > rect.x {
            next.push(Rect { width: blocked.x - rect.x, ..rect });
        }
        if blocked_right < right {
            next.push(Rect { x: blocked_right, width: right - blocked_right, ..rect });
        }
        if blocked.y > rect.y {
            next.push(Rect { height: blocked.y - rect.y, ..rect });
        }
        if blocked_bottom < bottom {
            next.push(Rect { y: blocked_bottom, height: bottom - blocked_bottom, ..rect });
        }
    }
    let retained: Vec<_> = next
        .iter()
        .enumerate()
        .filter(|(index, rectangle)| {
            !next.iter().enumerate().any(|(other_index, other)| {
                other_index != *index
                    && other.contains(**rectangle)
                    && (other != *rectangle || other_index < *index)
            })
        })
        .map(|(_, &rectangle)| rectangle)
        .collect();
    *free = retained;
}

fn initial_free_rectangles(problem: &ConstrainedProblem2D) -> Vec<Vec<Rect>> {
    problem
        .problem
        .sheets
        .iter()
        .enumerate()
        .map(|(index, stock)| {
            let constraints = constraints_for(problem, index);
            let mut free = vec![Rect {
                x: constraints.trim_left,
                y: constraints.trim_top,
                width: stock.width - constraints.trim_left - constraints.trim_right,
                height: stock.height - constraints.trim_top - constraints.trim_bottom,
            }];
            for defect in constraints.defects {
                subtract(&mut free, padded(defect_rect(defect), stock.kerf));
            }
            free
        })
        .collect()
}

#[derive(Debug, Clone, Copy)]
struct Packed {
    demand: usize,
    rect: Rect,
    rotated: bool,
}

#[derive(Debug, Clone)]
struct Bin {
    stock: usize,
    free: Vec<Rect>,
    placed: Vec<Packed>,
}

#[derive(Debug, Clone)]
struct State {
    bins: Vec<Bin>,
    used: Vec<usize>,
    unplaced: Vec<usize>,
    cost: f64,
    stock_area: u64,
    placed_area: u64,
    remaining: Vec<usize>,
}

impl State {
    fn new(problem: &ConstrainedProblem2D) -> Self {
        Self {
            bins: Vec::new(),
            used: vec![0; problem.problem.sheets.len()],
            unplaced: Vec::new(),
            cost: 0.0,
            stock_area: 0,
            placed_area: 0,
            remaining: problem.problem.demands.iter().map(|demand| demand.quantity).collect(),
        }
    }

    fn skip(&mut self, demand: usize) {
        self.unplaced.push(demand);
        self.remaining[demand] -= 1;
    }
}

#[derive(Debug, Clone, Copy)]
struct Move {
    bin: usize,
    stock: usize,
    rect: Rect,
    rotated: bool,
    new_bin: bool,
    cost: f64,
    stock_area: u64,
    residual: u64,
}

fn moves(
    problem: &ConstrainedProblem2D,
    initial_free: &[Vec<Rect>],
    state: &State,
    demand_index: usize,
    objective: PackingObjective2D,
) -> Vec<Move> {
    let demand = &problem.problem.demands[demand_index];
    let mut candidates = Vec::new();
    let mut add = |bin: usize, stock: usize, free: &[Rect], new_bin: bool| {
        let sheet = &problem.problem.sheets[stock];
        let item = ItemInstance2D {
            name: String::new(),
            width: demand.width,
            height: demand.height,
            can_rotate: demand.can_rotate,
            fixed_rotation: None,
        };
        for (width, height, rotated) in item.orientations() {
            for &available in free {
                if available.fits(width, height) {
                    candidates.push(Move {
                        bin,
                        stock,
                        rect: Rect { width, height, ..available },
                        rotated,
                        new_bin,
                        cost: if new_bin { sheet.cost } else { 0.0 },
                        stock_area: if new_bin {
                            u64::from(sheet.width) * u64::from(sheet.height)
                        } else {
                            0
                        },
                        residual: available.area() - u64::from(width) * u64::from(height),
                    });
                }
            }
        }
    };
    for (index, bin) in state.bins.iter().enumerate() {
        add(index, bin.stock, &bin.free, false);
    }
    for (stock, (sheet, free)) in problem.problem.sheets.iter().zip(initial_free).enumerate() {
        if sheet.quantity.is_none_or(|limit| state.used[stock] < limit) {
            add(state.bins.len(), stock, free, true);
        }
    }
    candidates.sort_by(|left, right| {
        let primary = match objective {
            PackingObjective2D::BinCount => left
                .new_bin
                .cmp(&right.new_bin)
                .then_with(|| left.stock_area.cmp(&right.stock_area))
                .then_with(|| left.cost.total_cmp(&right.cost)),
            PackingObjective2D::TotalCost => left
                .cost
                .total_cmp(&right.cost)
                .then_with(|| left.new_bin.cmp(&right.new_bin))
                .then_with(|| left.stock_area.cmp(&right.stock_area)),
        };
        primary
            .then_with(|| left.residual.cmp(&right.residual))
            .then_with(|| left.bin.cmp(&right.bin))
            .then_with(|| left.stock.cmp(&right.stock))
            .then_with(|| left.rect.y.cmp(&right.rect.y))
            .then_with(|| left.rect.x.cmp(&right.rect.x))
            .then_with(|| left.rotated.cmp(&right.rotated))
    });
    let mut seen = BTreeSet::new();
    candidates.retain(|candidate| {
        seen.insert((
            candidate.bin,
            candidate.stock,
            candidate.rect.x,
            candidate.rect.y,
            candidate.rect.width,
            candidate.rect.height,
            candidate.rotated,
        ))
    });
    candidates
}

fn apply_move(
    problem: &ConstrainedProblem2D,
    initial_free: &[Vec<Rect>],
    state: &mut State,
    demand: usize,
    candidate: Move,
) {
    if candidate.new_bin {
        state.bins.push(Bin {
            stock: candidate.stock,
            free: initial_free[candidate.stock].clone(),
            placed: Vec::new(),
        });
        state.used[candidate.stock] += 1;
        state.cost += candidate.cost;
        state.stock_area += candidate.stock_area;
    }
    let bin = &mut state.bins[candidate.bin];
    bin.placed.push(Packed { demand, rect: candidate.rect, rotated: candidate.rotated });
    state.placed_area += candidate.rect.area();
    state.remaining[demand] -= 1;
    subtract(&mut bin.free, padded(candidate.rect, problem.problem.sheets[candidate.stock].kerf));
}

fn compare_states(left: &State, right: &State, objective: PackingObjective2D) -> Ordering {
    left.unplaced.len().cmp(&right.unplaced.len()).then_with(|| match objective {
        PackingObjective2D::BinCount => left
            .bins
            .len()
            .cmp(&right.bins.len())
            .then_with(|| {
                (left.stock_area - left.placed_area).cmp(&(right.stock_area - right.placed_area))
            })
            .then_with(|| left.cost.total_cmp(&right.cost)),
        PackingObjective2D::TotalCost => left
            .cost
            .total_cmp(&right.cost)
            .then_with(|| left.bins.len().cmp(&right.bins.len()))
            .then_with(|| {
                (left.stock_area - left.placed_area).cmp(&(right.stock_area - right.placed_area))
            }),
    })
}

#[derive(Debug, Clone, Copy)]
struct RepairChoice {
    demand: usize,
    movement: Move,
    fills_strip: bool,
    bounding_waste: u64,
}

struct RepairFrame {
    state: State,
    choices: Vec<RepairChoice>,
    next: usize,
}

/// Try eliminating one bin with a bounded, iterative demand-order tree search.
/// Only complete all-demand layouts replace the incumbent. At most 64 parts and
/// 16 demand types enter this lane, so the depth and per-frame geometry are bounded.
fn repair_bins(
    problem: &ConstrainedProblem2D,
    initial_free: &[Vec<Rect>],
    options: &ConstrainedOptions2D,
    best: &mut State,
    explored: &mut usize,
    diagnostics: &mut RepairDiagnostics,
) -> bool {
    let target = best.bins.len() - 1;
    if target == 2
        && repair_allocations(problem, initial_free, options, best, explored, diagnostics)
    {
        return true;
    }
    let initial = State::new(problem);
    let (choices, mut truncated) = repair_choices(problem, initial_free, &initial, target, options);
    let mut visited = BTreeSet::new();
    let mut stack = vec![RepairFrame { state: initial, choices, next: 0 }];
    while let Some(frame) = stack.last_mut() {
        let Some(&choice) = frame.choices.get(frame.next) else {
            if stack.pop().is_none() {
                break;
            }
            continue;
        };
        if *explored == options.max_states {
            return true;
        }
        frame.next += 1;
        *explored += 1;
        let mut child = frame.state.clone();
        apply_move(problem, initial_free, &mut child, choice.demand, choice.movement);
        if child.remaining.iter().all(|&count| count == 0) {
            if compare_states(&child, best, options.objective).is_lt() {
                *best = child;
                // The first improved complete packing ends this bounded repair;
                // other material-cost or waste alternatives remain unexplored.
                return true;
            }
            continue;
        }
        if repair_capacity_insufficient(problem, &child, target) {
            continue;
        }
        let key = state_key(&child);
        if visited.contains(&key) {
            continue;
        }
        if visited.len() < 4_096 && !visited.insert(key) {
            continue;
        }
        let (choices, omitted) = repair_choices(problem, initial_free, &child, target, options);
        truncated |= omitted;
        if !choices.is_empty() {
            stack.push(RepairFrame { state: child, choices, next: 0 });
        }
    }
    truncated
}

#[derive(Debug, Default)]
struct RepairDiagnostics {
    allocation_checks: usize,
    profiles: usize,
    canonical_calls: usize,
    forced_void_propagations: usize,
    truncated: bool,
}

#[derive(Debug)]
struct AllocationProfile {
    quantities: Vec<usize>,
    area: u64,
    largest_group: u64,
    count: usize,
}

struct AllocationCollector<'a> {
    quantities: &'a [usize],
    areas: &'a [u64],
    suffix: Vec<u64>,
    range: (u64, u64),
    limit: usize,
    check_limit: usize,
    checks: usize,
    truncated: bool,
    profiles: Vec<AllocationProfile>,
}

impl AllocationCollector<'_> {
    fn visit(&mut self, index: usize, quantities: &mut Vec<usize>, area: u64) {
        if self.truncated {
            return;
        }
        if self.checks == self.check_limit {
            self.truncated = true;
            return;
        }
        self.checks += 1;
        if area > self.range.1 || area + self.suffix[index] < self.range.0 {
            return;
        }
        if index == self.quantities.len() {
            if area == 0 {
                return;
            }
            if self.profiles.len() == self.limit {
                self.truncated = true;
                return;
            }
            self.profiles.push(AllocationProfile {
                quantities: quantities.clone(),
                area,
                largest_group: quantities
                    .iter()
                    .zip(self.areas)
                    .map(|(&count, &size)| count as u64 * size)
                    .max()
                    .unwrap_or(0),
                count: quantities.iter().sum(),
            });
            return;
        }
        for count in 0..=self.quantities[index] {
            quantities.push(count);
            self.visit(index + 1, quantities, area + count as u64 * self.areas[index]);
            quantities.truncate(index);
            if self.truncated {
                return;
            }
        }
    }
}

fn gcd(mut left: u32, mut right: u32) -> u32 {
    while right != 0 {
        (left, right) = (right, left % right);
    }
    left
}

fn canonical_quantum(problem: &ConstrainedProblem2D, stock: usize) -> u32 {
    let source = &problem.problem.sheets[stock];
    problem.problem.demands.iter().fold(gcd(source.width, source.height), |step, demand| {
        gcd(gcd(step, demand.width), demand.height)
    })
}

fn canonical_eligible(problem: &ConstrainedProblem2D, stock: usize) -> bool {
    let source = &problem.problem.sheets[stock];
    let constraints = constraints_for(problem, stock);
    let step = canonical_quantum(problem, stock);
    source.kerf == 0
        && constraints == SheetConstraints2D::default()
        && u64::from(source.width / step) * u64::from(source.height / step) <= 4_096
}

// Enumerating complete source-quantity profiles is bounded independently of
// geometry: at most 4096 combinations, 1024 retained feasible profiles, and
// 16*max_states prefix checks. Every retained profile is charged before CP runs.
fn repair_allocations(
    problem: &ConstrainedProblem2D,
    initial_free: &[Vec<Rect>],
    options: &ConstrainedOptions2D,
    best: &mut State,
    explored: &mut usize,
    diagnostics: &mut RepairDiagnostics,
) -> bool {
    let quantities: Vec<_> = problem.problem.demands.iter().map(|d| d.quantity).collect();
    let combinations =
        quantities.iter().fold(1usize, |count, &quantity| count.saturating_mul(quantity + 1));
    if combinations > 4_096 {
        diagnostics.truncated = true;
        return false;
    }
    let areas: Vec<_> =
        problem.problem.demands.iter().map(|d| u64::from(d.width) * u64::from(d.height)).collect();
    let mut suffix = vec![0; quantities.len() + 1];
    for (index, (&quantity, &area)) in quantities.iter().zip(&areas).enumerate().rev() {
        suffix[index] = suffix[index + 1] + quantity as u64 * area;
    }
    let total = suffix[0];
    let mut pairs = Vec::new();
    for (left, a) in problem.problem.sheets.iter().enumerate() {
        if !canonical_eligible(problem, left) {
            continue;
        }
        for (right, b) in problem.problem.sheets.iter().enumerate().skip(left) {
            if canonical_eligible(problem, right) {
                let capacity = u64::from(a.width) * u64::from(a.height)
                    + u64::from(b.width) * u64::from(b.height);
                if total <= capacity {
                    pairs.push((capacity, left, right));
                }
            }
        }
    }
    pairs.sort_unstable();
    if pairs.is_empty() {
        return false;
    }
    diagnostics.truncated = true;
    for (_, left, right) in pairs {
        let remaining = options.max_states.saturating_sub(*explored);
        let profile_limit = (options.max_states / 4).min(1_024).min(remaining);
        let check_limit =
            options.max_states.saturating_mul(16).saturating_sub(diagnostics.allocation_checks);
        if profile_limit == 0 || check_limit == 0 {
            return false;
        }
        let left_source = &problem.problem.sheets[left];
        let right_source = &problem.problem.sheets[right];
        let mut collector = AllocationCollector {
            quantities: &quantities,
            areas: &areas,
            suffix: suffix.clone(),
            range: (
                total
                    .saturating_sub(u64::from(right_source.width) * u64::from(right_source.height)),
                u64::from(left_source.width) * u64::from(left_source.height),
            ),
            limit: profile_limit,
            check_limit,
            checks: 0,
            truncated: false,
            profiles: Vec::new(),
        };
        collector.visit(0, &mut Vec::new(), 0);
        diagnostics.allocation_checks += collector.checks;
        diagnostics.profiles += collector.profiles.len();
        *explored += collector.profiles.len();
        collector.profiles.sort_by(|a, b| {
            b.largest_group
                .cmp(&a.largest_group)
                .then_with(|| b.count.cmp(&a.count))
                .then_with(|| b.area.cmp(&a.area))
        });
        for profile in collector.profiles {
            let complement: Vec<_> = quantities
                .iter()
                .zip(&profile.quantities)
                .map(|(&full, &used)| full - used)
                .collect();
            if left == right && complement.iter().any(|&q| q > 0) && left_source.quantity == Some(1)
            {
                continue;
            }
            let mut first = None;
            for seed in canonical_seeds(problem, left, &profile.quantities) {
                let mut budget =
                    CanonicalBudget { explored, limit: options.max_states, nodes: 8, diagnostics };
                if let Some(bin) = canonical_pattern(
                    problem,
                    initial_free,
                    &profile.quantities,
                    left,
                    &seed,
                    &mut budget,
                ) {
                    first = Some(bin);
                    break;
                }
                if *explored == options.max_states {
                    return false;
                }
            }
            let Some(first) = first else {
                continue;
            };
            let second = if complement.iter().all(|&q| q == 0) {
                None
            } else {
                let mut budget = CanonicalBudget {
                    explored,
                    limit: options.max_states,
                    nodes: 128,
                    diagnostics,
                };
                let Some(bin) =
                    canonical_pattern(problem, initial_free, &complement, right, &[], &mut budget)
                else {
                    continue;
                };
                Some(bin)
            };
            let mut candidate = State::new(problem);
            for bin in std::iter::once(first).chain(second) {
                let source = &problem.problem.sheets[bin.stock];
                candidate.used[bin.stock] += 1;
                candidate.cost += source.cost;
                candidate.stock_area += u64::from(source.width) * u64::from(source.height);
                candidate.placed_area +=
                    bin.placed.iter().map(|part| part.rect.area()).sum::<u64>();
                candidate.bins.push(bin);
            }
            if problem
                .problem
                .sheets
                .iter()
                .zip(&candidate.used)
                .any(|(source, &count)| source.quantity.is_some_and(|limit| count > limit))
            {
                continue;
            }
            candidate.remaining.fill(0);
            if compare_states(&candidate, best, options.objective).is_lt() {
                *best = candidate;
                return true;
            }
        }
    }
    false
}

// Only the largest allocated repeated source is seeded. Full rectangular grids
// and grids with one vacant cell retain the vacancy as usable physical space.
fn canonical_seeds(
    problem: &ConstrainedProblem2D,
    stock: usize,
    quantities: &[usize],
) -> Vec<Vec<Packed>> {
    let best = quantities.iter().copied().enumerate().filter(|(_, count)| *count >= 2).min_by_key(
        |&(index, count)| {
            (
                std::cmp::Reverse(
                    count as u64
                        * u64::from(problem.problem.demands[index].width)
                        * u64::from(problem.problem.demands[index].height),
                ),
                index,
            )
        },
    );
    let Some((demand, count)) = best else {
        return vec![Vec::new()];
    };
    let d = &problem.problem.demands[demand];
    let source = &problem.problem.sheets[stock];
    let item = ItemInstance2D {
        name: String::new(),
        width: d.width,
        height: d.height,
        can_rotate: d.can_rotate,
        fixed_rotation: None,
    };
    let mut seeds = Vec::new();
    let mut seen = BTreeSet::new();
    for columns in 1..=count {
        let rows = count.div_ceil(columns);
        let cells = columns * rows;
        if cells > count + 1 {
            continue;
        }
        for (width, height, rotated) in item.orientations() {
            let bw = u64::from(width) * columns as u64;
            let bh = u64::from(height) * rows as u64;
            if bw > u64::from(source.width) || bh > u64::from(source.height) {
                continue;
            }
            let (Ok(bw), Ok(bh)) = (u32::try_from(bw), u32::try_from(bh)) else {
                continue;
            };
            for vacant in 0..if cells == count { 1 } else { cells } {
                for x in [0, source.width - bw] {
                    for y in [0, source.height - bh] {
                        let mut seed = Vec::new();
                        for cell in 0..cells {
                            if cells > count && cell == vacant {
                                continue;
                            }
                            let px = u64::from(x) + (cell % columns) as u64 * u64::from(width);
                            let py = u64::from(y) + (cell / columns) as u64 * u64::from(height);
                            let (Ok(px), Ok(py)) = (u32::try_from(px), u32::try_from(py)) else {
                                continue;
                            };
                            seed.push(Packed {
                                demand,
                                rect: Rect { x: px, y: py, width, height },
                                rotated,
                            });
                        }
                        let key: Vec<_> = seed
                            .iter()
                            .map(|p| (p.rect.x, p.rect.y, p.rect.width, p.rect.height))
                            .collect();
                        if seed.len() == count && seen.insert(key) {
                            seeds.push(seed);
                        }
                        if seeds.len() == 64 {
                            return seeds;
                        }
                    }
                }
            }
        }
    }
    seeds
}

struct CanonicalBudget<'a> {
    explored: &'a mut usize,
    limit: usize,
    nodes: usize,
    diagnostics: &'a mut RepairDiagnostics,
}

impl CanonicalBudget<'_> {
    fn charge(&mut self) -> bool {
        if *self.explored == self.limit {
            return false;
        }
        *self.explored += 1;
        true
    }
}

#[derive(Debug, Clone, Copy)]
enum CanonicalChoice {
    Place(usize, Move),
    Void(Rect),
}

struct CanonicalFrame {
    state: State,
    choices: Vec<CanonicalChoice>,
    next: usize,
}

type CanonicalKey = (Vec<usize>, Vec<u64>);

fn canonical_frame(
    problem: &ConstrainedProblem2D,
    mut state: State,
    quantum: u32,
    seen: &mut BTreeSet<CanonicalKey>,
    diagnostics: &mut RepairDiagnostics,
) -> Option<CanonicalFrame> {
    let required: u64 = problem
        .problem
        .demands
        .iter()
        .zip(&state.remaining)
        .map(|(d, &count)| u64::from(d.width) * u64::from(d.height) * count as u64)
        .sum();
    let bin = &mut state.bins[0];
    let before = union_area(&bin.free);
    bin.free.retain(|space| {
        problem.problem.demands.iter().zip(&state.remaining).any(|(d, &count)| {
            count > 0
                && (space.fits(d.width, d.height)
                    || (d.can_rotate && space.fits(d.height, d.width)))
        })
    });
    let available = union_area(&bin.free);
    if available < before {
        diagnostics.forced_void_propagations += 1;
    }
    if required > available {
        return None;
    }
    let source = &problem.problem.sheets[bin.stock];
    let grid_width = source.width / quantum;
    let cells = u64::from(grid_width) * u64::from(source.height / quantum);
    let mut mask = vec![0u64; (cells as usize).div_ceil(64)];
    for space in &bin.free {
        for y in space.y / quantum..(space.y + space.height) / quantum {
            for x in space.x / quantum..(space.x + space.width) / quantum {
                let cell = (u64::from(y) * u64::from(grid_width) + u64::from(x)) as usize;
                mask[cell / 64] |= 1u64 << (cell % 64);
            }
        }
    }
    if !seen.insert((state.remaining.clone(), mask)) {
        return None;
    }
    let point = bin.free.iter().min_by_key(|space| (space.y, space.x)).copied()?;
    let mut placements = Vec::new();
    for (demand, &count) in state.remaining.iter().enumerate() {
        if count == 0 {
            continue;
        }
        let d = &problem.problem.demands[demand];
        let item = ItemInstance2D {
            name: String::new(),
            width: d.width,
            height: d.height,
            can_rotate: d.can_rotate,
            fixed_rotation: None,
        };
        for (width, height, rotated) in item.orientations() {
            let rect = Rect { x: point.x, y: point.y, width, height };
            if bin.free.iter().any(|space| space.contains(rect)) {
                placements.push((
                    demand,
                    Move {
                        bin: 0,
                        stock: bin.stock,
                        rect,
                        rotated,
                        new_bin: false,
                        cost: 0.0,
                        stock_area: 0,
                        residual: 0,
                    },
                ));
            }
        }
    }
    placements.sort_by(|a, b| {
        b.1.rect
            .area()
            .cmp(&a.1.rect.area())
            .then_with(|| a.0.cmp(&b.0))
            .then_with(|| b.1.rect.width.cmp(&a.1.rect.width))
    });
    let mut choices: Vec<_> = placements
        .into_iter()
        .map(|(demand, movement)| CanonicalChoice::Place(demand, movement))
        .collect();
    if available - required >= u64::from(quantum) * u64::from(quantum) {
        choices.push(CanonicalChoice::Void(Rect {
            x: point.x,
            y: point.y,
            width: quantum,
            height: quantum,
        }));
    }
    Some(CanonicalFrame { state, choices, next: 0 })
}

// Iterative canonical-cell search avoids recursion proportional to the grid.
// Forced-unusable regions are propagated within each charged node, as in ordinary
// maximal-space subtraction. Explicit void alternatives are charged successors.
fn canonical_pattern(
    problem: &ConstrainedProblem2D,
    initial_free: &[Vec<Rect>],
    quantities: &[usize],
    stock: usize,
    seed: &[Packed],
    budget: &mut CanonicalBudget<'_>,
) -> Option<Bin> {
    budget.diagnostics.canonical_calls += 1;
    let source = &problem.problem.sheets[stock];
    let quantum = canonical_quantum(problem, stock);
    let mut state = State::new(problem);
    state.remaining = quantities.to_vec();
    state.used[stock] = 1;
    state.cost = source.cost;
    state.stock_area = u64::from(source.width) * u64::from(source.height);
    state.bins.push(Bin { stock, free: initial_free[stock].clone(), placed: Vec::new() });
    for part in seed {
        if !budget.charge() {
            return None;
        }
        apply_move(
            problem,
            initial_free,
            &mut state,
            part.demand,
            Move {
                bin: 0,
                stock,
                rect: part.rect,
                rotated: part.rotated,
                new_bin: false,
                cost: 0.0,
                stock_area: 0,
                residual: 0,
            },
        );
    }
    if !budget.charge() {
        return None;
    }
    let mut nodes = 1;
    if state.remaining.iter().all(|&q| q == 0) {
        return state.bins.into_iter().next();
    }
    let mut seen = BTreeSet::new();
    let initial = canonical_frame(problem, state, quantum, &mut seen, budget.diagnostics)?;
    let mut stack = vec![initial];
    while let Some(frame) = stack.last_mut() {
        let Some(&choice) = frame.choices.get(frame.next) else {
            if stack.pop().is_none() {
                break;
            }
            continue;
        };
        if nodes == budget.nodes || !budget.charge() {
            return None;
        }
        nodes += 1;
        frame.next += 1;
        let mut child = frame.state.clone();
        match choice {
            CanonicalChoice::Place(demand, movement) => {
                apply_move(problem, initial_free, &mut child, demand, movement)
            }
            CanonicalChoice::Void(rect) => subtract(&mut child.bins[0].free, rect),
        }
        if child.remaining.iter().all(|&q| q == 0) {
            return child.bins.into_iter().next();
        }
        if stack.len() == 256 {
            budget.diagnostics.truncated = true;
            continue;
        }
        if let Some(frame) = canonical_frame(problem, child, quantum, &mut seen, budget.diagnostics)
        {
            stack.push(frame);
        }
    }
    None
}

fn repair_choices(
    problem: &ConstrainedProblem2D,
    initial_free: &[Vec<Rect>],
    state: &State,
    target: usize,
    options: &ConstrainedOptions2D,
) -> (Vec<RepairChoice>, bool) {
    let mut choices = Vec::new();
    for (demand, &remaining) in state.remaining.iter().enumerate() {
        if remaining == 0 {
            continue;
        }
        for movement in moves(problem, initial_free, state, demand, options.objective) {
            if movement.new_bin && state.bins.len() == target {
                continue;
            }
            let free = if movement.new_bin {
                &initial_free[movement.stock]
            } else {
                &state.bins[movement.bin].free
            };
            let fills_strip = free.iter().any(|available| {
                let kerf = u64::from(problem.problem.sheets[movement.stock].kerf);
                let width = u64::from(movement.rect.width) + kerf;
                let height = u64::from(movement.rect.height) + kerf;
                let available_width = u64::from(available.width) + kerf;
                let available_height = u64::from(available.height) + kerf;
                available.x == movement.rect.x
                    && available.y == movement.rect.y
                    && ((available_height % height == 0
                        && available_height / height <= remaining as u64)
                        || (available_width % width == 0
                            && available_width / width <= remaining as u64))
            });
            choices.push(RepairChoice {
                demand,
                fills_strip,
                bounding_waste: projected_bounding_waste(state, movement),
                movement,
            });
        }
    }
    choices.sort_by(|left, right| {
        left.movement
            .new_bin
            .cmp(&right.movement.new_bin)
            .then_with(|| right.fills_strip.cmp(&left.fills_strip))
            .then_with(|| left.bounding_waste.cmp(&right.bounding_waste))
            .then_with(|| left.movement.residual.cmp(&right.movement.residual))
            .then_with(|| left.demand.cmp(&right.demand))
    });
    let limit = options.beam_width;
    let truncated = choices.len() > limit;
    choices.truncate(limit);
    (choices, truncated)
}

/// Compactness is only an exploration order, not a lower bound. O(P + B) per
/// probe, with no label or full-state clones. A new singleton bin adds no gap.
fn projected_bounding_waste(state: &State, movement: Move) -> u64 {
    state
        .bins
        .iter()
        .enumerate()
        .map(|(index, bin)| {
            let mut min_x = u32::MAX;
            let mut min_y = u32::MAX;
            let mut max_x = 0u32;
            let mut max_y = 0u32;
            let mut area = 0u64;
            let rectangles = bin
                .placed
                .iter()
                .map(|packed| packed.rect)
                .chain((movement.bin == index && !movement.new_bin).then_some(movement.rect));
            for rect in rectangles {
                min_x = min_x.min(rect.x);
                min_y = min_y.min(rect.y);
                max_x = max_x.max(rect.x + rect.width);
                max_y = max_y.max(rect.y + rect.height);
                area += rect.area();
            }
            u64::from(max_x - min_x) * u64::from(max_y - min_y) - area
        })
        .sum()
}

/// Necessary free-material tests once every allowed bin is open. Individual
/// orientation-feasible maximal rectangles may overlap: compare their union,
/// never their sum. Reject only if the remaining physical area cannot fit.
fn repair_capacity_insufficient(
    problem: &ConstrainedProblem2D,
    state: &State,
    target: usize,
) -> bool {
    if state.bins.len() < target {
        return false;
    }
    let required: u64 = problem
        .problem
        .demands
        .iter()
        .zip(&state.remaining)
        .map(|(demand, &count)| u64::from(demand.width) * u64::from(demand.height) * count as u64)
        .sum();
    let available: u64 = state.bins.iter().map(|bin| union_area(&bin.free)).sum();
    if required > available {
        return true;
    }
    for (demand, &count) in problem.problem.demands.iter().zip(&state.remaining) {
        if count == 0 {
            continue;
        }
        let item = ItemInstance2D {
            name: String::new(),
            width: demand.width,
            height: demand.height,
            can_rotate: demand.can_rotate,
            fixed_rotation: None,
        };
        let available: u64 = state
            .bins
            .iter()
            .map(|bin| {
                let suitable: Vec<_> = bin
                    .free
                    .iter()
                    .copied()
                    .filter(|free| item.orientations().any(|(w, h, _)| free.fits(w, h)))
                    .collect();
                union_area(&suitable)
            })
            .sum();
        if u64::from(demand.width) * u64::from(demand.height) * count as u64 > available {
            return true;
        }
    }
    false
}

/// Exact union area by an x-slab sweep. O(F² log F) time, O(F) temporary memory.
fn union_area(rectangles: &[Rect]) -> u64 {
    let mut boundaries: Vec<_> =
        rectangles.iter().flat_map(|rect| [rect.x, rect.x + rect.width]).collect();
    boundaries.sort_unstable();
    boundaries.dedup();
    let mut total = 0u64;
    for edge in boundaries.windows(2) {
        let mut intervals: Vec<_> = rectangles
            .iter()
            .filter(|rect| rect.x <= edge[0] && rect.x + rect.width >= edge[1])
            .map(|rect| (rect.y, rect.y + rect.height))
            .collect();
        intervals.sort_unstable();
        let mut height = 0u64;
        let mut cursor = 0;
        for (start, end) in intervals {
            height += u64::from(end.saturating_sub(cursor.max(start)));
            cursor = cursor.max(end);
        }
        total += u64::from(edge[1] - edge[0]) * height;
    }
    total
}

type PlacementKey = (usize, u32, u32, u32, u32, bool);
type GeometryKey = Vec<(usize, Vec<PlacementKey>)>;

fn state_key(state: &State) -> (GeometryKey, Vec<usize>) {
    let mut geometry: GeometryKey = state
        .bins
        .iter()
        .map(|bin| {
            let mut placements: Vec<_> = bin
                .placed
                .iter()
                .map(|packed| {
                    (
                        packed.demand,
                        packed.rect.x,
                        packed.rect.y,
                        packed.rect.width,
                        packed.rect.height,
                        packed.rotated,
                    )
                })
                .collect();
            placements.sort_unstable();
            (bin.stock, placements)
        })
        .collect();
    geometry.sort_unstable();
    let mut skipped = state.unplaced.clone();
    skipped.sort_unstable();
    (geometry, skipped)
}

fn drop_metrics(
    stock: &super::Sheet2D,
    constraints: &SheetConstraints2D,
    placements: &[Placement2D],
) -> (u64, u128) {
    let mut excluded = placements.to_vec();
    let mut add = |rect: Rect| {
        if rect.width > 0 && rect.height > 0 {
            excluded.push(Placement2D {
                name: String::new(),
                x: rect.x,
                y: rect.y,
                width: rect.width,
                height: rect.height,
                rotated: false,
            });
        }
    };
    add(Rect { x: 0, y: 0, width: constraints.trim_left, height: stock.height });
    add(Rect {
        x: stock.width - constraints.trim_right,
        y: 0,
        width: constraints.trim_right,
        height: stock.height,
    });
    add(Rect { x: 0, y: 0, width: stock.width, height: constraints.trim_top });
    add(Rect {
        x: 0,
        y: stock.height - constraints.trim_bottom,
        width: stock.width,
        height: constraints.trim_bottom,
    });
    for &defect in &constraints.defects {
        add(defect_rect(defect));
    }
    super::drops::usable_drop_metrics(stock, &excluded, 0)
}

fn finish(
    problem: &ConstrainedProblem2D,
    state: State,
    explored_states: usize,
    search_truncated: bool,
) -> ConstrainedSolution2D {
    let mut layouts = Vec::new();
    let mut stock_indices = Vec::new();
    let mut demand_indices = Vec::new();
    for bin in state.bins {
        let stock = &problem.problem.sheets[bin.stock];
        let mut indices = Vec::new();
        let placements: Vec<_> = bin
            .placed
            .into_iter()
            .map(|packed| {
                indices.push(packed.demand);
                Placement2D {
                    name: problem.problem.demands[packed.demand].name.clone(),
                    x: packed.rect.x,
                    y: packed.rect.y,
                    width: packed.rect.width,
                    height: packed.rect.height,
                    rotated: packed.rotated,
                }
            })
            .collect();
        let used_area = placements.iter().map(|p| u64::from(p.width) * u64::from(p.height)).sum();
        let kerf_area = super::kerf::kerf_area_for_layout(stock, &placements);
        let (largest_usable_drop_area, sum_sq_usable_drop_areas) =
            drop_metrics(stock, &constraints_for(problem, bin.stock), &placements);
        stock_indices.push(bin.stock);
        demand_indices.push(indices);
        layouts.push(SheetLayout2D {
            sheet_name: stock.name.clone(),
            width: stock.width,
            height: stock.height,
            cost: stock.cost,
            used_area,
            waste_area: u64::from(stock.width) * u64::from(stock.height) - used_area,
            kerf_area,
            largest_usable_drop_area,
            sum_sq_usable_drop_areas,
            placements,
        });
    }
    let mut missing = vec![0usize; problem.problem.demands.len()];
    for index in state.unplaced {
        missing[index] += 1;
    }
    let unplaced = missing
        .into_iter()
        .enumerate()
        .filter(|(_, quantity)| *quantity > 0)
        .map(|(index, quantity)| {
            let mut demand = problem.problem.demands[index].clone();
            demand.quantity = quantity;
            demand
        })
        .collect();
    ConstrainedSolution2D {
        solution: TwoDSolution {
            algorithm: "constrained_max_rects".into(),
            guillotine: false,
            sheet_count: layouts.len(),
            total_waste_area: layouts.iter().map(|layout| layout.waste_area).sum(),
            total_kerf_area: layouts.iter().map(|layout| layout.kerf_area).sum(),
            total_cost: layouts.iter().map(|layout| layout.cost).sum(),
            max_usable_drop_area: layouts
                .iter()
                .map(|layout| layout.largest_usable_drop_area)
                .max()
                .unwrap_or(0),
            total_sum_sq_usable_drop_areas: layouts
                .iter()
                .map(|layout| layout.sum_sq_usable_drop_areas)
                .fold(0u128, u128::saturating_add),
            layouts,
            unplaced,
            metrics: SolverMetrics2D {
                iterations: 1,
                explored_states,
                notes: vec![
                    "deterministic constrained construction; greedy incumbent retained; no global optimality claim".into(),
                ],
            },
        },
        stock_indices,
        demand_indices,
        explored_states,
        search_truncated,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::two_d::{RectDemand2D, Sheet2D};

    fn sheet(name: &str, width: u32, height: u32, cost: f64) -> Sheet2D {
        Sheet2D {
            name: name.into(),
            width,
            height,
            cost,
            quantity: None,
            kerf: 0,
            edge_kerf_relief: false,
        }
    }

    fn demand(name: &str, width: u32, height: u32, quantity: usize) -> RectDemand2D {
        RectDemand2D { name: name.into(), width, height, quantity, can_rotate: false }
    }

    fn simple_problem() -> ConstrainedProblem2D {
        ConstrainedProblem2D {
            problem: TwoDProblem {
                sheets: vec![sheet("stock", 10, 10, 1.0)],
                demands: vec![demand("part", 5, 5, 4)],
            },
            constraints: Vec::new(),
        }
    }

    #[test]
    fn validator_rejects_independent_metadata_and_total_forgeries() {
        let problem = simple_problem();
        let seed = solve_2d_constrained(
            problem.clone(),
            ConstrainedOptions2D { max_states: 0, ..Default::default() },
        )
        .expect("valid seed");
        validate_2d_constrained(&problem, &seed).expect("seed certificate");
        type Mutation = (&'static str, fn(&mut ConstrainedSolution2D), &'static str);
        let cases: &[Mutation] = &[
            (
                "stock vector",
                |s| s.stock_indices.clear(),
                "layout count, provenance, or guillotine claim",
            ),
            (
                "demand vector",
                |s| s.demand_indices.clear(),
                "layout count, provenance, or guillotine claim",
            ),
            (
                "sheet count",
                |s| s.solution.sheet_count += 1,
                "layout count, provenance, or guillotine claim",
            ),
            (
                "guillotine",
                |s| s.solution.guillotine = true,
                "layout count, provenance, or guillotine claim",
            ),
            (
                "empty layout",
                |s| s.solution.layouts[0].placements.clear(),
                "layout metadata or placement provenance",
            ),
            (
                "sheet name",
                |s| s.solution.layouts[0].sheet_name.push('x'),
                "layout metadata or placement provenance",
            ),
            (
                "sheet width",
                |s| s.solution.layouts[0].width += 1,
                "layout metadata or placement provenance",
            ),
            (
                "sheet height",
                |s| s.solution.layouts[0].height += 1,
                "layout metadata or placement provenance",
            ),
            (
                "sheet cost",
                |s| s.solution.layouts[0].cost += 1.0,
                "layout metadata or placement provenance",
            ),
            (
                "part name",
                |s| s.solution.layouts[0].placements[0].name.push('x'),
                "placement dimensions, rotation or trimmed bounds",
            ),
            (
                "part width",
                |s| s.solution.layouts[0].placements[0].width += 1,
                "placement dimensions, rotation or trimmed bounds",
            ),
            (
                "part height",
                |s| s.solution.layouts[0].placements[0].height += 1,
                "placement dimensions, rotation or trimmed bounds",
            ),
            (
                "used area",
                |s| s.solution.layouts[0].used_area += 1,
                "incorrect per-layout material totals",
            ),
            (
                "kerf area",
                |s| s.solution.layouts[0].kerf_area += 1,
                "incorrect per-layout material totals",
            ),
            (
                "largest drop",
                |s| s.solution.layouts[0].largest_usable_drop_area += 1,
                "incorrect per-layout material totals",
            ),
            (
                "squared drops",
                |s| s.solution.layouts[0].sum_sq_usable_drop_areas += 1,
                "incorrect per-layout material totals",
            ),
            (
                "total waste",
                |s| s.solution.total_waste_area += 1,
                "incomplete demand accounting or incorrect solution totals",
            ),
            (
                "total kerf",
                |s| s.solution.total_kerf_area += 1,
                "incomplete demand accounting or incorrect solution totals",
            ),
            (
                "total cost nan",
                |s| s.solution.total_cost = f64::NAN,
                "incomplete demand accounting or incorrect solution totals",
            ),
            (
                "max drop",
                |s| s.solution.max_usable_drop_area += 1,
                "incomplete demand accounting or incorrect solution totals",
            ),
            (
                "total squared drops",
                |s| s.solution.total_sum_sq_usable_drop_areas += 1,
                "incomplete demand accounting or incorrect solution totals",
            ),
            (
                "explored states",
                |s| s.solution.metrics.explored_states += 1,
                "incomplete demand accounting or incorrect solution totals",
            ),
            (
                "zero unplaced",
                |s| s.solution.unplaced.push(demand("part", 5, 5, 0)),
                "zero unplaced quantity",
            ),
            (
                "unknown unplaced",
                |s| s.solution.unplaced.push(demand("other", 5, 5, 1)),
                "unplaced record lacks matching residual demand",
            ),
        ];
        for (label, corrupt, expected) in cases {
            let mut forged = seed.clone();
            corrupt(&mut forged);
            assert!(
                matches!(validate_2d_constrained(&problem, &forged),
                Err(BinPackingError::InvalidSolution { reason }) if reason == *expected),
                "{label}"
            );
        }
        let mut missing = problem.clone();
        missing.problem.demands[0].quantity += 1;
        assert!(matches!(validate_2d_constrained(&missing, &seed),
            Err(BinPackingError::InvalidSolution { reason }) if reason == "incomplete demand accounting or incorrect solution totals"));
        let mut valid_partial = seed.clone();
        valid_partial.solution.unplaced.push(demand("part", 5, 5, 1));
        validate_2d_constrained(&missing, &valid_partial).expect("matching residual demand");
        for altered in [
            demand("part", 6, 5, 1),
            demand("part", 5, 6, 1),
            RectDemand2D { can_rotate: true, ..demand("part", 5, 5, 1) },
        ] {
            let mut forged = valid_partial.clone();
            forged.solution.unplaced[0] = altered;
            assert!(matches!(validate_2d_constrained(&missing, &forged),
                Err(BinPackingError::InvalidSolution { reason }) if reason == "unplaced record lacks matching residual demand"));
        }
    }

    #[test]
    fn validator_enforces_defect_clearance_at_exact_boundary() {
        let mut stock = sheet("stock", 12, 10, 1.0);
        stock.kerf = 1;
        let problem = ConstrainedProblem2D {
            problem: TwoDProblem { sheets: vec![stock], demands: vec![demand("part", 4, 10, 1)] },
            constraints: vec![SheetConstraints2D {
                defects: vec![Defect2D { x: 5, y: 0, width: 2, height: 10 }],
                ..Default::default()
            }],
        };
        let seed = solve_2d_constrained(
            problem.clone(),
            ConstrainedOptions2D { max_states: 0, ..Default::default() },
        )
        .expect("clear placement");
        assert_eq!(seed.solution.layouts[0].placements[0].x, 0);
        validate_2d_constrained(&problem, &seed).expect("one-unit clearance");
        let mut forged = seed;
        forged.solution.layouts[0].placements[0].x = 1;
        assert!(matches!(validate_2d_constrained(&problem, &forged),
            Err(BinPackingError::InvalidSolution { reason }) if reason == "placement intersects a defect or its clearance"));
    }

    #[test]
    fn source_limits_and_constraint_boundaries_are_structured() {
        let mut cases = Vec::new();
        let mut p = simple_problem();
        p.problem.demands[0].quantity = 513;
        cases.push((p, "problem"));
        let mut p = simple_problem();
        p.problem.demands = vec![demand("part", 1, 1, 1); 129];
        cases.push((p, "problem"));
        let mut p = simple_problem();
        p.problem.sheets =
            (0..17).map(|index| sheet(&format!("stock {index}"), 10, 10, 1.0)).collect();
        cases.push((p, "problem"));
        let mut p = simple_problem();
        p.constraints = vec![SheetConstraints2D {
            defects: vec![Defect2D { x: 0, y: 0, width: 1, height: 1 }; 257],
            ..Default::default()
        }];
        cases.push((p, "problem"));
        let mut p = simple_problem();
        p.problem.sheets[0].cost = f64::MAX;
        cases.push((p, "totals"));
        let mut p = simple_problem();
        p.constraints =
            vec![SheetConstraints2D { trim_top: 5, trim_bottom: 5, ..Default::default() }];
        cases.push((p, "trims"));
        for defect in [
            Defect2D { x: 0, y: 0, width: 1, height: 0 },
            Defect2D { x: 0, y: 10, width: 1, height: 1 },
        ] {
            let mut p = simple_problem();
            p.constraints =
                vec![SheetConstraints2D { defects: vec![defect], ..Default::default() }];
            cases.push((p, "defects"));
        }
        for (problem, expected) in cases {
            assert!(
                matches!(validate_source(&problem), Err(BinPackingError::InvalidConstraint { field, .. }) if field == expected),
                "{expected}"
            );
        }
        let mut boundary = simple_problem();
        boundary.problem.demands[0].quantity = 512;
        boundary.constraints = vec![SheetConstraints2D {
            trim_top: 9,
            defects: vec![Defect2D { x: 9, y: 9, width: 1, height: 1 }; 256],
            ..Default::default()
        }];
        validate_source(&boundary)
            .expect("inclusive quantity and defect limits, positive trim and edge defect");
    }

    fn dense_mixed_problem() -> ConstrainedProblem2D {
        ConstrainedProblem2D {
            problem: TwoDProblem {
                sheets: vec![sheet("stock", 24, 18, 1.0)],
                demands: [
                    (12, 6, 2, true),
                    (9, 6, 2, true),
                    (9, 8, 3, false),
                    (8, 10, 1, false),
                    (13, 2, 2, true),
                    (9, 4, 2, true),
                    (8, 9, 2, false),
                    (12, 2, 1, false),
                ]
                .into_iter()
                .enumerate()
                .map(|(index, (width, height, quantity, can_rotate))| super::super::RectDemand2D {
                    name: index.to_string(),
                    width,
                    height,
                    quantity,
                    can_rotate,
                })
                .collect(),
            },
            constraints: Vec::new(),
        }
    }

    #[test]
    fn constrained_dense_mixed_rectangles_fit_two_sheets() {
        let problem = dense_mixed_problem();
        let result = solve_2d_constrained(problem.clone(), ConstrainedOptions2D::default())
            .expect("bounded dense packing");
        validate_2d_constrained(&problem, &result).expect("independent dense layout validation");
        assert!(result.solution.unplaced.is_empty());
        assert_eq!(result.solution.sheet_count, 2);
        assert!(result.explored_states <= ConstrainedOptions2D::default().max_states);
    }

    #[test]
    fn constrained_dense_packing_preserves_integral_unit_changes() {
        let mut problem = dense_mixed_problem();
        for stock in &mut problem.problem.sheets {
            stock.width = stock.width.saturating_mul(10);
            stock.height = stock.height.saturating_mul(10);
        }
        for demand in &mut problem.problem.demands {
            demand.width = demand.width.saturating_mul(10);
            demand.height = demand.height.saturating_mul(10);
        }
        let result = solve_2d_constrained(problem.clone(), ConstrainedOptions2D::default())
            .expect("same physical job in smaller units");
        assert_eq!(result.solution.sheet_count, 2);
        assert!(result.solution.unplaced.is_empty());
        assert!(result.explored_states <= ConstrainedOptions2D::default().max_states);
        validate_2d_constrained(&problem, &result)
            .expect("scaled geometry remains independently valid");
    }

    #[test]
    fn constrained_skips_a_fitting_part_to_fulfill_more_finite_stock_demand() {
        let mut problem = ConstrainedProblem2D {
            problem: TwoDProblem {
                sheets: vec![sheet("stock", 10, 1, 1.0)],
                demands: vec![demand("large", 6, 1, 1), demand("small", 5, 1, 2)],
            },
            constraints: Vec::new(),
        };
        problem.problem.sheets[0].quantity = Some(1);
        let result = solve_2d_constrained(
            problem.clone(),
            ConstrainedOptions2D { max_states: 64, ..Default::default() },
        )
        .expect("finite-stock packing");
        assert_eq!(result.solution.layouts[0].placements.len(), 2);
        assert_eq!(result.demand_indices, [vec![1, 1]]);
        assert_eq!(result.solution.unplaced, [problem.problem.demands[0].clone()]);
        assert!(result.explored_states <= 64);
        validate_2d_constrained(&problem, &result).expect("independent partial validation");
    }

    #[test]
    fn constrained_validator_accepts_explicitly_unplaced_oversized_originals() {
        let mut problem = simple_problem();
        let mut result = solve_2d_constrained(problem.clone(), ConstrainedOptions2D::default())
            .expect("base layout");
        let oversized = demand("oversized", 11, 11, 1);
        problem.problem.demands.push(oversized.clone());
        result.solution.unplaced.push(oversized);
        validate_2d_constrained(&problem, &result).expect("unfit omitted source witness");
        assert!(matches!(
            solve_2d_constrained(problem, ConstrainedOptions2D::default()),
            Err(BinPackingError::Infeasible2D { .. })
        ));
    }

    fn elongated_problem() -> ConstrainedProblem2D {
        let mut first = demand("a", 21, 6, 3);
        first.can_rotate = true;
        let mut third = demand("c", 9, 8, 2);
        third.can_rotate = true;
        ConstrainedProblem2D {
            problem: TwoDProblem {
                sheets: vec![sheet("stock", 60, 20, 1.0)],
                demands: vec![first, demand("b", 17, 5, 4), third, demand("d", 33, 4, 2)],
            },
            constraints: Vec::new(),
        }
    }

    #[test]
    fn constrained_trims_and_defects_exclude_physical_material() {
        let problem = ConstrainedProblem2D {
            problem: TwoDProblem {
                sheets: vec![sheet("stock", 12, 12, 1.0)],
                demands: vec![demand("part", 4, 4, 2)],
            },
            constraints: vec![SheetConstraints2D {
                trim_left: 2,
                trim_top: 2,
                defects: vec![Defect2D { x: 2, y: 2, width: 4, height: 4 }],
                ..Default::default()
            }],
        };
        let result = solve_2d_constrained(problem.clone(), ConstrainedOptions2D::default())
            .expect("constrained packing");
        assert!(result.solution.unplaced.is_empty());
        for p in &result.solution.layouts[0].placements {
            assert!(p.x >= 2 && p.y >= 2);
            assert!(p.x >= 6 || p.y >= 6);
        }
        validate_2d_constrained(&problem, &result).expect("independent validation");
    }

    #[test]
    fn constrained_cost_objective_changes_stock_selection() {
        let problem = ConstrainedProblem2D {
            problem: TwoDProblem {
                sheets: vec![sheet("large", 10, 10, 10.0), sheet("small", 5, 10, 1.0)],
                demands: vec![demand("part", 5, 10, 2)],
            },
            constraints: Vec::new(),
        };
        let result = solve_2d_constrained(
            problem,
            ConstrainedOptions2D { objective: PackingObjective2D::TotalCost, ..Default::default() },
        )
        .expect("cost packing");
        assert_eq!(result.solution.total_cost, 2.0);
        assert_eq!(result.stock_indices, vec![1, 1]);
    }

    #[test]
    fn constrained_order_search_recovers_elongated_single_sheet_layout() {
        let problem = elongated_problem();
        let baseline = solve_2d_constrained(
            problem.clone(),
            ConstrainedOptions2D { max_states: 0, ..Default::default() },
        )
        .expect("greedy baseline");
        assert_eq!(baseline.solution.sheet_count, 2);
        let limited = solve_2d_constrained(
            problem.clone(),
            ConstrainedOptions2D { max_states: 1, ..Default::default() },
        )
        .expect("one-state repair budget");
        assert_eq!(limited.explored_states, 1);
        assert_eq!(limited.solution.layouts, baseline.solution.layouts);
        validate_2d_constrained(&problem, &limited).expect("retained all-demand incumbent");
        let result = solve_2d_constrained(
            problem.clone(),
            ConstrainedOptions2D { beam_width: 8, max_states: 512, ..Default::default() },
        )
        .expect("bounded demand-order search");
        assert_eq!(result.solution.sheet_count, 1);
        assert!(result.solution.unplaced.is_empty());
        assert!(result.explored_states <= 512);
        validate_2d_constrained(&problem, &result).expect("independent recovered layout");
    }

    #[test]
    fn constrained_order_repair_preserves_translated_physical_exclusions() {
        let mut problem = elongated_problem();
        problem.problem.sheets[0].width = 66;
        problem.problem.sheets[0].height = 26;
        problem.constraints = vec![SheetConstraints2D {
            trim_left: 3,
            trim_right: 3,
            trim_top: 3,
            trim_bottom: 3,
            defects: vec![Defect2D { x: 0, y: 0, width: 3, height: 26 }],
        }];
        let result = solve_2d_constrained(
            problem.clone(),
            ConstrainedOptions2D { beam_width: 8, max_states: 512, ..Default::default() },
        )
        .expect("translated strip repair");
        assert_eq!(result.solution.sheet_count, 1);
        assert_eq!(result.solution.layouts[0].used_area, 1_126);
        assert_eq!(result.solution.total_waste_area, 590);
        assert!(result.explored_states <= 512);
        validate_2d_constrained(&problem, &result).expect("physical exclusions retained");
    }

    #[test]
    fn constrained_count_objective_explores_larger_stock() {
        let problem = ConstrainedProblem2D {
            problem: TwoDProblem {
                sheets: vec![sheet("large", 10, 10, 10.0), sheet("small", 5, 10, 1.0)],
                demands: vec![demand("part", 5, 10, 2)],
            },
            constraints: Vec::new(),
        };
        let result =
            solve_2d_constrained(problem, ConstrainedOptions2D::default()).expect("count packing");
        assert_eq!(result.solution.sheet_count, 1);
        assert_eq!(result.stock_indices, vec![0]);
        assert_eq!(result.solution.total_cost, 10.0);
    }

    #[test]
    fn constrained_duplicate_labels_preserve_indices_and_partial_inventory() {
        let mut stock = sheet("same", 10, 10, 1.0);
        stock.quantity = Some(1);
        let problem = ConstrainedProblem2D {
            problem: TwoDProblem {
                sheets: vec![stock.clone(), stock],
                demands: vec![demand("same", 10, 5, 1), demand("same", 5, 10, 3)],
            },
            constraints: Vec::new(),
        };
        let result = solve_2d_constrained(problem.clone(), ConstrainedOptions2D::default())
            .expect("partial indexed packing");
        assert_eq!(result.solution.sheet_count, 2);
        assert_eq!(result.solution.unplaced.iter().map(|d| d.quantity).sum::<usize>(), 1);
        assert_ne!(result.stock_indices[0], result.stock_indices[1]);
        for (layout, indices) in result.solution.layouts.iter().zip(&result.demand_indices) {
            for (placement, &index) in layout.placements.iter().zip(indices) {
                assert_eq!(placement.width, problem.problem.demands[index].width);
            }
        }
        validate_2d_constrained(&problem, &result).expect("partial accounting");
    }

    #[test]
    fn constrained_defect_clearance_and_blade_gap_are_enforced() {
        let mut stock = sheet("stock", 12, 10, 1.0);
        stock.kerf = 1;
        let problem = ConstrainedProblem2D {
            problem: TwoDProblem { sheets: vec![stock], demands: vec![demand("part", 4, 10, 2)] },
            constraints: vec![SheetConstraints2D {
                defects: vec![Defect2D { x: 5, y: 0, width: 2, height: 10 }],
                ..Default::default()
            }],
        };
        let result = solve_2d_constrained(problem.clone(), ConstrainedOptions2D::default())
            .expect("clearance packing");
        assert_eq!(result.solution.sheet_count, 1);
        let placements = &result.solution.layouts[0].placements;
        assert_eq!(placements.iter().map(|p| p.x).collect::<Vec<_>>(), vec![0, 8]);
        validate_2d_constrained(&problem, &result).expect("clearance validation");
    }

    #[test]
    fn constrained_rotation_respects_trimmed_bounds() {
        let mut item = demand("part", 8, 10, 1);
        item.can_rotate = true;
        let problem = ConstrainedProblem2D {
            problem: TwoDProblem { sheets: vec![sheet("stock", 12, 8, 1.0)], demands: vec![item] },
            constraints: vec![SheetConstraints2D { trim_left: 2, ..Default::default() }],
        };
        let result = solve_2d_constrained(problem.clone(), ConstrainedOptions2D::default())
            .expect("rotated packing");
        let part = &result.solution.layouts[0].placements[0];
        assert!(part.rotated);
        assert_eq!((part.x, part.y, part.width, part.height), (2, 0, 10, 8));
        validate_2d_constrained(&problem, &result).expect("rotation validation");
    }

    #[test]
    fn constrained_drop_metrics_exclude_overlapping_defects_and_trims() {
        let problem = ConstrainedProblem2D {
            problem: TwoDProblem {
                sheets: vec![sheet("stock", 12, 10, 1.0)],
                demands: vec![demand("part", 5, 5, 1)],
            },
            constraints: vec![SheetConstraints2D {
                trim_left: 2,
                defects: vec![
                    Defect2D { x: 0, y: 0, width: 7, height: 10 },
                    Defect2D { x: 2, y: 0, width: 5, height: 10 },
                ],
                ..Default::default()
            }],
        };
        let result = solve_2d_constrained(problem.clone(), ConstrainedOptions2D::default())
            .expect("overlapping exclusion packing");
        assert_eq!(result.solution.max_usable_drop_area, 25);
        assert_eq!(result.solution.total_sum_sq_usable_drop_areas, 625);
        assert_eq!(result.solution.total_waste_area, 95);
        validate_2d_constrained(&problem, &result).expect("excluded drop validation");
    }

    #[test]
    fn constrained_zero_and_tiny_budgets_retain_complete_incumbent() {
        let problem = simple_problem();
        let baseline = solve_2d_constrained(
            problem.clone(),
            ConstrainedOptions2D { max_states: 0, ..Default::default() },
        )
        .expect("zero budget baseline");
        let limited = solve_2d_constrained(
            problem.clone(),
            ConstrainedOptions2D { max_states: 1, ..Default::default() },
        )
        .expect("limited beam");
        assert_eq!(baseline.solution.layouts, limited.solution.layouts);
        assert_eq!(baseline.solution.unplaced, limited.solution.unplaced);
        assert_eq!(baseline.explored_states, 0);
        assert_eq!(limited.explored_states, 1);
        assert!(baseline.search_truncated && limited.search_truncated);
        validate_2d_constrained(&problem, &limited).expect("retained complete incumbent");

        for (label, problem) in
            [("single sheet", simple_problem()), ("repair eligible", elongated_problem())]
        {
            let baseline = solve_2d_constrained(
                problem.clone(),
                ConstrainedOptions2D { max_states: 0, ..Default::default() },
            )
            .expect("baseline for budget table");
            for budget in [0, 1, 2, 3, 4, 8] {
                let result = solve_2d_constrained(
                    problem.clone(),
                    ConstrainedOptions2D { max_states: budget, ..Default::default() },
                )
                .expect("budgeted solve");
                assert!(result.explored_states <= budget, "{label}, budget {budget}: state limit");
                assert!(
                    result.solution.unplaced.is_empty(),
                    "{label}, budget {budget}: demand retained"
                );
                let count: usize =
                    result.solution.layouts.iter().map(|layout| layout.placements.len()).sum();
                let requested: usize =
                    problem.problem.demands.iter().map(|demand| demand.quantity).sum();
                assert_eq!(count, requested, "{label}, budget {budget}: demand conservation");
                assert!(
                    result.solution.sheet_count <= baseline.solution.sheet_count,
                    "{label}, budget {budget}: incumbent retained"
                );
                assert!(
                    validate_2d_constrained(&problem, &result).is_ok(),
                    "{label}, budget {budget}: certificate"
                );
            }
        }
    }

    #[test]
    fn constrained_invalid_physical_constraints_are_structured_errors() {
        let mut cases = Vec::new();
        let mut mismatch = simple_problem();
        mismatch.constraints = vec![SheetConstraints2D::default(); 2];
        cases.push(mismatch);
        let mut trimmed_away = simple_problem();
        trimmed_away.constraints =
            vec![SheetConstraints2D { trim_left: u32::MAX, ..Default::default() }];
        cases.push(trimmed_away);
        let mut outside = simple_problem();
        outside.constraints = vec![SheetConstraints2D {
            defects: vec![Defect2D { x: u32::MAX, y: 0, width: 1, height: 1 }],
            ..Default::default()
        }];
        cases.push(outside);
        let mut zero_defect = simple_problem();
        zero_defect.constraints = vec![SheetConstraints2D {
            defects: vec![Defect2D { x: 0, y: 0, width: 0, height: 1 }],
            ..Default::default()
        }];
        cases.push(zero_defect);
        let mut edge_relief = simple_problem();
        edge_relief.problem.sheets[0].edge_kerf_relief = true;
        cases.push(edge_relief);
        for problem in cases {
            assert!(matches!(
                solve_2d_constrained(problem, ConstrainedOptions2D::default()),
                Err(BinPackingError::InvalidConstraint { .. })
            ));
        }
        for options in [
            ConstrainedOptions2D { beam_width: 0, ..Default::default() },
            ConstrainedOptions2D { max_states: 100_001, ..Default::default() },
        ] {
            assert!(matches!(
                solve_2d_constrained(simple_problem(), options),
                Err(BinPackingError::InvalidConstraint { .. })
            ));
        }
    }

    #[test]
    fn constrained_validator_rejects_geometry_provenance_and_accounting_tampering() {
        let problem = simple_problem();
        let result = solve_2d_constrained(problem.clone(), ConstrainedOptions2D::default())
            .expect("valid seed");
        let mut corruptions = Vec::new();
        let mut overlap = result.clone();
        overlap.solution.layouts[0].placements[1].x = overlap.solution.layouts[0].placements[0].x;
        overlap.solution.layouts[0].placements[1].y = overlap.solution.layouts[0].placements[0].y;
        corruptions.push(overlap);
        let mut bad_stock = result.clone();
        bad_stock.stock_indices[0] = usize::MAX;
        corruptions.push(bad_stock);
        let mut bad_demand = result.clone();
        bad_demand.demand_indices[0][0] = usize::MAX;
        corruptions.push(bad_demand);
        let mut bad_cost = result.clone();
        bad_cost.solution.total_cost = f64::NAN;
        corruptions.push(bad_cost);
        let mut bad_area = result.clone();
        bad_area.solution.layouts[0].waste_area += 1;
        corruptions.push(bad_area);
        let mut extra_unplaced = result.clone();
        extra_unplaced.solution.unplaced.push(demand("part", 5, 5, 1));
        corruptions.push(extra_unplaced);
        let mut wrong_orientation = result.clone();
        wrong_orientation.solution.layouts[0].placements[0].rotated = true;
        corruptions.push(wrong_orientation);
        let mut missing_placement = result;
        assert!(missing_placement.demand_indices[0].pop().is_some());
        corruptions.push(missing_placement);
        for corruption in corruptions {
            assert!(matches!(
                validate_2d_constrained(&problem, &corruption),
                Err(BinPackingError::InvalidSolution { .. })
            ));
        }

        for (label, width, height, constraints, invalid_position) in [
            ("left", 2, 10, SheetConstraints2D { trim_left: 2, ..Default::default() }, (0, 0)),
            ("right", 2, 10, SheetConstraints2D { trim_right: 2, ..Default::default() }, (8, 0)),
            ("top", 10, 2, SheetConstraints2D { trim_top: 2, ..Default::default() }, (0, 0)),
            ("bottom", 10, 2, SheetConstraints2D { trim_bottom: 2, ..Default::default() }, (0, 8)),
        ] {
            let mut border_problem = ConstrainedProblem2D {
                problem: TwoDProblem {
                    sheets: vec![sheet("stock", 10, 10, 1.0)],
                    demands: vec![demand("strip", width, height, 1)],
                },
                constraints: Vec::new(),
            };
            let mut seed =
                solve_2d_constrained(border_problem.clone(), ConstrainedOptions2D::default())
                    .expect("untrimmed strip seed");
            assert!(
                validate_2d_constrained(&border_problem, &seed).is_ok(),
                "{label}: untrimmed seed"
            );
            assert_eq!(
                seed.solution.layouts[0].largest_usable_drop_area, 80,
                "{label}: seed offcut"
            );
            border_problem.constraints = vec![constraints];
            let boundary =
                solve_2d_constrained(border_problem.clone(), ConstrainedOptions2D::default())
                    .expect("trimmed strip boundary");
            assert!(
                validate_2d_constrained(&border_problem, &boundary).is_ok(),
                "{label}: valid trimmed placement"
            );
            seed.solution.layouts[0].placements[0].x = invalid_position.0;
            seed.solution.layouts[0].placements[0].y = invalid_position.1;
            assert!(
                matches!(
                    validate_2d_constrained(&border_problem, &seed),
                    Err(BinPackingError::InvalidSolution { .. })
                ),
                "{label}: strip in removed border"
            );
        }
    }

    #[test]
    fn constrained_large_dimensions_use_widened_area_and_reject_aggregate_overflow() {
        let large = 1 << 30;
        let problem = ConstrainedProblem2D {
            problem: TwoDProblem {
                sheets: vec![sheet("stock", large, large, 1.0)],
                demands: vec![demand("part", large, large, 1)],
            },
            constraints: Vec::new(),
        };
        let result = solve_2d_constrained(problem.clone(), ConstrainedOptions2D::default())
            .expect("widened area");
        assert_eq!(result.solution.layouts[0].used_area, 1u64 << 60);
        let mut overflowing = problem;
        overflowing.problem.demands[0].quantity = 16;
        assert!(matches!(
            solve_2d_constrained(overflowing, ConstrainedOptions2D::default()),
            Err(BinPackingError::InvalidConstraint { .. })
        ));
    }

    #[test]
    fn constrained_serialization_preserves_indices_and_controls() {
        let problem = simple_problem();
        let result = solve_2d_constrained(problem.clone(), ConstrainedOptions2D::default())
            .expect("serializable packing");
        let wire = serde_json::to_string(&result).expect("serialize result");
        let decoded: ConstrainedSolution2D = serde_json::from_str(&wire).expect("decode result");
        assert_eq!(decoded, result);
        validate_2d_constrained(&problem, &decoded).expect("decoded validation");
        let options: ConstrainedOptions2D =
            serde_json::from_str(r#"{"objective":"total_cost","max_states":0}"#)
                .expect("default beam width");
        assert_eq!(options.objective, PackingObjective2D::TotalCost);
        assert_eq!(options.beam_width, 8);
    }

    #[test]
    fn terra_batch1_forged_matching_demand_provenance_exceeds_quantity() {
        let mut problem = simple_problem();
        problem.problem.demands = vec![demand("part", 5, 5, 2), demand("part", 5, 5, 2)];
        let mut result = solve_2d_constrained(problem.clone(), ConstrainedOptions2D::default())
            .expect("valid identical demands");
        validate_2d_constrained(&problem, &result).expect("valid provenance");
        result.demand_indices[0].fill(0);
        assert!(matches!(
            validate_2d_constrained(&problem, &result),
            Err(BinPackingError::InvalidSolution { .. })
        ));
    }

    #[test]
    fn terra_batch1_stock_inventory_cannot_be_forged_with_identical_sheets() {
        let mut problem = simple_problem();
        problem.problem.demands[0].quantity = 2;
        problem.problem.demands[0].width = 10;
        problem.problem.demands[0].height = 10;
        problem.problem.sheets[0].quantity = Some(1);
        problem.problem.sheets.push(problem.problem.sheets[0].clone());
        let mut result = solve_2d_constrained(problem.clone(), ConstrainedOptions2D::default())
            .expect("two stock copies");
        validate_2d_constrained(&problem, &result).expect("valid stock inventory");
        assert_eq!(result.stock_indices.len(), 2);
        result.stock_indices.fill(0);
        assert!(matches!(
            validate_2d_constrained(&problem, &result),
            Err(BinPackingError::InvalidSolution { .. })
        ));
    }

    #[test]
    fn terra_batch1_rejects_forged_solution_diagnostics_and_totals() {
        let problem = simple_problem();
        let seed = solve_2d_constrained(problem.clone(), ConstrainedOptions2D::default())
            .expect("valid seed");
        let mut cases = Vec::new();
        let mut value = seed.clone();
        value.solution.metrics.explored_states = value.explored_states.saturating_add(1);
        cases.push(value);
        let mut value = seed.clone();
        value.solution.total_kerf_area += 1;
        cases.push(value);
        let mut value = seed.clone();
        value.solution.max_usable_drop_area += 1;
        cases.push(value);
        let mut value = seed.clone();
        value.solution.total_sum_sq_usable_drop_areas += 1;
        cases.push(value);
        let mut value = seed;
        value.solution.guillotine = true;
        cases.push(value);
        for value in cases {
            assert!(matches!(
                validate_2d_constrained(&problem, &value),
                Err(BinPackingError::InvalidSolution { .. })
            ));
        }
    }

    #[test]
    fn terra_batch1_exact_trim_and_defect_source_boundaries() {
        let mut problem = simple_problem();
        problem.constraints = vec![SheetConstraints2D {
            trim_right: 5,
            defects: vec![Defect2D { x: 9, y: 9, width: 1, height: 1 }],
            ..Default::default()
        }];
        let result = solve_2d_constrained(problem.clone(), ConstrainedOptions2D::default())
            .expect("boundary constraints are valid");
        validate_2d_constrained(&problem, &result).expect("valid boundary certificate");
        problem.constraints[0].defects[0].height = 2;
        assert!(matches!(validate_2d_constrained(&problem, &result),
            Err(BinPackingError::InvalidConstraint { ref field, .. }) if field == "defects"));
        problem.constraints[0].defects.clear();
        problem.constraints[0].trim_right = 10;
        assert!(matches!(validate_2d_constrained(&problem, &result),
            Err(BinPackingError::InvalidConstraint { ref field, .. }) if field == "trims"));
    }

    #[test]
    fn allocation_collector_enumerates_and_obeys_both_limits() {
        let quantities = [2, 1];
        let areas = [2, 3];
        let mut collector = AllocationCollector {
            quantities: &quantities,
            areas: &areas,
            suffix: vec![7, 3, 0],
            range: (3, 5),
            limit: 10,
            check_limit: 100,
            checks: 0,
            truncated: false,
            profiles: Vec::new(),
        };
        collector.visit(0, &mut Vec::new(), 0);
        let profiles: Vec<_> = collector
            .profiles
            .iter()
            .map(|p| (p.quantities.clone(), p.area, p.largest_group, p.count))
            .collect();
        assert_eq!(
            profiles,
            vec![(vec![0, 1], 3, 3, 1), (vec![1, 1], 5, 3, 2), (vec![2, 0], 4, 4, 2)]
        );
        assert!(!collector.truncated);
        let mut limited = AllocationCollector {
            quantities: &quantities,
            areas: &areas,
            suffix: vec![7, 3, 0],
            range: (0, 7),
            limit: 1,
            check_limit: 100,
            checks: 0,
            truncated: false,
            profiles: Vec::new(),
        };
        limited.visit(0, &mut Vec::new(), 0);
        assert!(limited.truncated);
        assert_eq!(limited.profiles.len(), 1);
        assert_eq!(limited.profiles[0].quantities, vec![0, 1]);
        let checks = limited.checks;
        limited.visit(0, &mut Vec::new(), 0);
        assert_eq!(limited.checks, checks);
        let mut checked = AllocationCollector {
            quantities: &quantities,
            areas: &areas,
            suffix: vec![7, 3, 0],
            range: (0, 7),
            limit: 10,
            check_limit: 2,
            checks: 0,
            truncated: false,
            profiles: Vec::new(),
        };
        checked.visit(0, &mut Vec::new(), 0);
        assert!(checked.truncated);
        assert_eq!(checked.checks, 2);
        assert!(checked.profiles.is_empty());
    }

    #[test]
    fn canonical_eligibility_respects_geometry_and_exclusions() {
        let mut problem = simple_problem();
        assert!(canonical_eligible(&problem, 0));
        problem.problem.sheets[0].kerf = 1;
        assert!(!canonical_eligible(&problem, 0));
        problem.problem.sheets[0].kerf = 0;
        problem.constraints = vec![SheetConstraints2D { trim_left: 1, ..Default::default() }];
        assert!(!canonical_eligible(&problem, 0));
        problem.constraints.clear();
        problem.problem.sheets[0].width = 64;
        problem.problem.sheets[0].height = 64;
        problem.problem.demands[0].width = 1;
        problem.problem.demands[0].height = 1;
        assert!(canonical_eligible(&problem, 0));
        problem.problem.sheets[0].width = 65;
        assert!(!canonical_eligible(&problem, 0));
    }

    #[test]
    fn repair_allocation_rejects_oversized_enumeration_and_unusable_pairs() {
        let mut problem = simple_problem();
        problem.problem.demands = vec![demand("a", 1, 1, 64), demand("b", 1, 1, 64)];
        let options = ConstrainedOptions2D { max_states: 32, ..Default::default() };
        let mut best = State::new(&problem);
        let mut explored = 0;
        let mut diagnostics = RepairDiagnostics::default();
        assert!(!repair_allocations(
            &problem,
            &initial_free_rectangles(&problem),
            &options,
            &mut best,
            &mut explored,
            &mut diagnostics
        ));
        assert!(diagnostics.truncated);
        assert_eq!((explored, diagnostics.allocation_checks, diagnostics.profiles), (0, 0, 0));
        problem.problem.demands = vec![demand("a", 10, 10, 3)];
        best = State::new(&problem);
        diagnostics = RepairDiagnostics::default();
        assert!(!repair_allocations(
            &problem,
            &initial_free_rectangles(&problem),
            &options,
            &mut best,
            &mut explored,
            &mut diagnostics
        ));
        assert!(!diagnostics.truncated);
        assert_eq!((explored, diagnostics.allocation_checks), (0, 0));
        problem.problem.demands[0].quantity = 1;
        problem.problem.sheets[0].kerf = 1;
        diagnostics = RepairDiagnostics::default();
        assert!(!repair_allocations(
            &problem,
            &initial_free_rectangles(&problem),
            &options,
            &mut best,
            &mut explored,
            &mut diagnostics
        ));
        assert!(!diagnostics.truncated);
        assert_eq!(explored, 0);
    }

    #[test]
    fn repair_allocation_charges_profiles_and_respects_stock_and_budget() {
        let mut problem = simple_problem();
        problem.problem.demands[0].quantity = 2;
        problem.problem.sheets[0].width = 5;
        problem.problem.sheets[0].height = 5;
        problem.problem.sheets[0].quantity = Some(1);
        let free = initial_free_rectangles(&problem);
        let mut best = State::new(&problem);
        let mut explored = 0;
        let mut diagnostics = RepairDiagnostics::default();
        let options = ConstrainedOptions2D { max_states: 16, ..Default::default() };
        assert!(!repair_allocations(
            &problem,
            &free,
            &options,
            &mut best,
            &mut explored,
            &mut diagnostics
        ));
        assert_eq!(diagnostics.profiles, 1);
        assert_eq!(explored, 1);
        assert_eq!(diagnostics.canonical_calls, 0);
        problem.problem.sheets[0].quantity = Some(2);
        let mut best = State::new(&problem);
        // Repair compares completed incumbents. Explicitly account for every
        // unplaced item so an empty in-progress state cannot outrank a packing.
        for _ in 0..problem.problem.demands[0].quantity {
            best.skip(0);
        }
        let mut explored = 0;
        let mut diagnostics = RepairDiagnostics::default();
        assert!(repair_allocations(
            &problem,
            &free,
            &options,
            &mut best,
            &mut explored,
            &mut diagnostics
        ));
        assert_eq!(best.remaining, vec![0]);
        assert_eq!(best.used, vec![2]);
        assert_eq!(best.bins.iter().map(|b| b.placed.len()).sum::<usize>(), 2);
        assert!(explored <= options.max_states);
        assert_eq!(diagnostics.profiles, 1);
        assert_eq!(diagnostics.canonical_calls, 2);
        let mut best = State::new(&problem);
        let mut explored = 16;
        let mut diagnostics = RepairDiagnostics::default();
        assert!(!repair_allocations(
            &problem,
            &free,
            &options,
            &mut best,
            &mut explored,
            &mut diagnostics
        ));
        assert_eq!((explored, diagnostics.profiles, diagnostics.allocation_checks), (16, 0, 0));
    }

    #[test]
    fn canonical_seeds_preserve_repeated_group_and_single_vacancy() {
        let mut problem = simple_problem();
        problem.problem.sheets[0].width = 4;
        problem.problem.sheets[0].height = 4;
        problem.problem.demands = vec![demand("small", 1, 1, 3), demand("large", 2, 2, 1)];
        assert_eq!(canonical_seeds(&problem, 0, &[1, 1]).len(), 1);
        assert!(canonical_seeds(&problem, 0, &[1, 1])[0].is_empty());
        let seeds = canonical_seeds(&problem, 0, &[3, 1]);
        assert!(!seeds.is_empty());
        assert!(seeds.iter().all(
            |seed| seed.len() == 3 && seed.iter().all(|p| p.demand == 0 && p.rect.area() == 1)
        ));
        assert!(seeds.iter().any(|seed| {
            let max_x = seed.iter().map(|p| p.rect.x).max().expect("three cells");
            let min_x = seed.iter().map(|p| p.rect.x).min().expect("three cells");
            let max_y = seed.iter().map(|p| p.rect.y).max().expect("three cells");
            let min_y = seed.iter().map(|p| p.rect.y).min().expect("three cells");
            max_x - min_x == 1 && max_y - min_y == 1
        }));
    }
}
