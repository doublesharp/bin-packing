//! Stage-constrained physical guillotine cut trees and native packing search.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use super::constrained::{ConstrainedProblem2D, PackingObjective2D, SheetConstraints2D};
use super::model::ItemInstance2D;
use super::{Placement2D, Sheet2D, SheetLayout2D, SolverMetrics2D, TwoDSolution};
use crate::{BinPackingError, Result};

/// Axis of a full-span physical cut.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StagedAxis2D {
    /// A vertical cut divides material width.
    Vertical,
    /// A horizontal cut divides material height.
    Horizontal,
}

/// Permitted nominal first-stage direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum FirstStageAxis2D {
    /// Choose independently for each sheet.
    Any,
    /// Nominal stage one divides width.
    #[default]
    Vertical,
    /// Nominal stage one divides height.
    Horizontal,
}

/// Terminal cutting policy, with every finishing cut retained in the tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum StagedCutType2D {
    /// Every physical axis level counts toward the stage limit.
    Exact,
    /// One additional terminal axis level may isolate one part and one waste.
    #[default]
    NonExact,
    /// Exact stages with one source demand type in each final-stage strip.
    Homogeneous,
    /// A terminal extra level may isolate up to two parts and one waste.
    Roadef2018,
}

/// Physical machine and cut-pattern restrictions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct StagedRules2D {
    /// Maximum effective axis levels; `None` permits unlimited stages.
    pub max_stages: Option<usize>,
    /// First nominal cut axis, allowing an empty first stage when necessary.
    pub first_axis: FirstStageAxis2D,
    /// Exact, homogeneous, or explicitly discounted terminal finishing cuts.
    pub cut_type: StagedCutType2D,
    /// Minimum material width of a productive stage-one strip.
    pub minimum_distance_1_cuts: u32,
    /// Maximum material width of a productive stage-one strip.
    pub maximum_distance_1_cuts: Option<u32>,
    /// Minimum material width of a productive stage-two strip.
    pub minimum_distance_2_cuts: u32,
    /// Maximum material width of a productive stage-two strip.
    pub maximum_distance_2_cuts: Option<u32>,
    /// Maximum physical stage-one cuts per sheet.
    pub maximum_number_1_cuts: Option<usize>,
    /// Maximum physical stage-two cuts in each first-level subplate.
    pub maximum_number_2_cuts: Option<usize>,
    /// Minimum width and height of every nonempty terminal waste piece.
    pub minimum_waste_length: u32,
    /// Whether a physical blade band or zero-kerf line may cross a defect.
    pub cut_through_defects: bool,
}

impl Default for StagedRules2D {
    fn default() -> Self {
        Self {
            max_stages: Some(3),
            first_axis: FirstStageAxis2D::Vertical,
            cut_type: StagedCutType2D::NonExact,
            minimum_distance_1_cuts: 0,
            maximum_distance_1_cuts: None,
            minimum_distance_2_cuts: 0,
            maximum_distance_2_cuts: None,
            maximum_number_1_cuts: None,
            maximum_number_2_cuts: None,
            minimum_waste_length: 0,
            cut_through_defects: false,
        }
    }
}

/// Prepared rectangular stock, demands, and indexed cutting rules.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StagedProblem2D {
    /// Original stock plus trim and defect exclusions.
    pub packing: ConstrainedProblem2D,
    /// Stage, tool, and cut restrictions.
    #[serde(default)]
    pub rules: StagedRules2D,
    /// Cutting-sequence identifiers per source demand, or empty for independent
    /// demands. Earlier source types in a group must finish before later ones.
    #[serde(default)]
    pub groups: Vec<Option<u32>>,
}

/// Deterministic construction and improvement controls.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct StagedOptions2D {
    /// Count-first or material-cost-first ranking after fulfillment.
    pub objective: PackingObjective2D,
    /// Retained frontier size; one through 32.
    pub beam_width: usize,
    /// Maximum successor states after greedy construction; zero keeps baseline.
    pub max_states: usize,
}

impl Default for StagedOptions2D {
    fn default() -> Self {
        Self { objective: PackingObjective2D::BinCount, beam_width: 8, max_states: 4_000 }
    }
}

/// Positive rectangular material region in original sheet coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct StagedRegion2D {
    /// Left coordinate.
    pub x: u32,
    /// Top coordinate.
    pub y: u32,
    /// Material width.
    pub width: u32,
    /// Material height.
    pub height: u32,
}

/// A cut partitions a region into its nonempty material children plus blade loss.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StagedAction2D {
    /// A full-span cut, whose thickness is the source sheet's kerf.
    Cut {
        /// Physical blade direction.
        axis: StagedAxis2D,
        /// Distance from the region's low edge to the near blade edge.
        offset: u32,
        /// Nonempty children, in low-coordinate then high-coordinate order.
        children: Vec<usize>,
    },
    /// A finished part occupies the complete region.
    Part {
        /// Authoritative source-demand index.
        demand_index: usize,
        /// Whether width/height were interchanged.
        rotated: bool,
    },
    /// Unused material, possibly containing excluded defects.
    Waste,
}

/// Indexed node of an authoritative physical cut tree.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct StagedNode2D {
    /// Material region before this operation.
    pub region: StagedRegion2D,
    /// Full-span cut, exact finished part, or terminal waste.
    pub action: StagedAction2D,
}

/// Prepared usable stock starts at node zero. Trims were removed beforehand.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct StagedTree2D {
    /// Nominal first-stage direction chosen for this sheet.
    pub first_axis: StagedAxis2D,
    /// Indexed nodes; every node must be reachable exactly once from zero.
    pub nodes: Vec<StagedNode2D>,
}

/// One finished part in the canonical low-before-high production sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct StagedPart2D {
    /// Index of the source-aligned layout/tree.
    pub layout_index: usize,
    /// Index of the part leaf in that tree.
    pub node_index: usize,
}

/// Packing with physically reconstructable cut trees and source provenance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StagedSolution2D {
    /// Geometry and material totals, with guillotine compatibility true.
    pub solution: TwoDSolution,
    /// Source sheet for each layout/tree.
    pub stock_indices: Vec<usize>,
    /// Source demand for each layout placement, in canonical tree order.
    pub demand_indices: Vec<Vec<usize>>,
    /// Exact unplaced count for every source demand, including repeated labels.
    pub unplaced_quantities: Vec<usize>,
    /// One authoritative physical tree per layout.
    pub trees: Vec<StagedTree2D>,
    /// Low-before-high traversal of all finished parts, in sheet order.
    pub production_order: Vec<StagedPart2D>,
    /// Successor states evaluated after baseline construction.
    pub explored_states: usize,
    /// Whether a budget or frontier restriction omitted alternatives.
    pub search_truncated: bool,
}

/// Construct cut trees while enforcing staged machine restrictions.
///
/// A deterministic corner-cut construction supplies an incumbent before bounded
/// mixed-demand beam search. At most 128 parts, 64 demand types, 16 stock types,
/// and 128 defects are supported. This is a heuristic even when search finishes;
/// `search_truncated == false` never proves global optimality.
///
/// # Errors
/// Returns structured malformed-input, unsupported-control, or invalid-result errors.
pub fn solve_2d_staged(
    problem: StagedProblem2D,
    options: StagedOptions2D,
) -> Result<StagedSolution2D> {
    validate_problem(&problem)?;
    if !(1..=32).contains(&options.beam_width) || options.max_states > 100_000 {
        return Err(constraint(
            "search",
            "beam_width must be 1..=32 and max_states at most 100000",
        ));
    }
    let quantity = problem.packing.problem.demands.iter().map(|d| d.quantity).sum::<usize>();
    let mut best = State::new(&problem);
    let mut baseline = best.clone();
    let mut truncated = options.max_states == 0;
    for _ in 0..quantity {
        let (choices, omitted) = movements(&problem, &baseline, options.objective);
        truncated |= omitted;
        let mut accepted = None;
        for movement in choices {
            let mut child = baseline.clone();
            apply(&problem, &mut child, movement)?;
            if state_valid(&problem, &child, false) {
                accepted = Some(child);
                break;
            }
        }
        if let Some(child) = accepted {
            baseline = child;
            if compare_states(&baseline, &best, options.objective).is_lt()
                && state_valid(&problem, &baseline, true)
            {
                best = baseline.clone();
            }
        } else if let Some(demand) = baseline.remaining.iter().position(|&q| q != 0) {
            baseline.remaining[demand] -= 1;
        }
    }

    let mut explored = 0usize;
    let mut beam = vec![State::new(&problem)];
    'search: for depth in 0..quantity {
        if options.max_states == 0 {
            break;
        }
        let mut next = Vec::new();
        // Reserve effort for complete paths rather than spending the entire
        // budget generating alternatives at the first demand.
        let per_state = ((options.max_states - explored)
            / beam.len().saturating_mul(quantity - depth).max(1))
        .clamp(1, 64);
        for state in &beam {
            let (choices, omitted) = movements(&problem, state, options.objective);
            truncated |= omitted || choices.len() > per_state;
            for movement in choices.into_iter().take(per_state) {
                if explored == options.max_states {
                    truncated = true;
                    break 'search;
                }
                explored += 1;
                let mut child = state.clone();
                apply(&problem, &mut child, movement)?;
                if !state_valid(&problem, &child, false) {
                    continue;
                }
                if compare_states(&child, &best, options.objective).is_lt()
                    && state_valid(&problem, &child, true)
                {
                    best = child.clone();
                }
                next.push(child);
            }
            if let Some(demand) = state.remaining.iter().position(|&q| q != 0) {
                if explored == options.max_states {
                    truncated = true;
                    break 'search;
                }
                explored += 1;
                let mut child = state.clone();
                child.remaining[demand] -= 1;
                next.push(child);
            }
        }
        next.sort_by(|a, b| compare_states(a, b, options.objective));
        let mut seen = BTreeSet::new();
        next.retain(|state| seen.insert(state_key(state)));
        if next.len() > options.beam_width {
            truncated = true;
            next.truncate(options.beam_width);
        }
        if next.is_empty() {
            break;
        }
        beam = next;
    }
    let result = finish(&problem, &best, explored, truncated)?;
    validate_2d_staged(&problem, &result)?;
    Ok(result)
}

/// Independently validate physical cuts, stage rules, provenance and accounting.
///
/// # Errors
/// Returns structured input/constraint or invalid-solution errors.
pub fn validate_2d_staged(problem: &StagedProblem2D, solution: &StagedSolution2D) -> Result<()> {
    validate_problem(problem)?;
    let result = &solution.solution;
    if !result.guillotine
        || result.sheet_count != result.layouts.len()
        || result.layouts.len() != solution.stock_indices.len()
        || result.layouts.len() != solution.demand_indices.len()
        || result.layouts.len() != solution.trees.len()
        || solution.unplaced_quantities.len() != problem.packing.problem.demands.len()
        || solution.explored_states > 100_000
        || result.metrics.explored_states != solution.explored_states
    {
        return Err(invalid("layout counts, provenance or search accounting"));
    }
    let mut used = vec![0usize; problem.packing.problem.sheets.len()];
    let mut placed = vec![0usize; problem.packing.problem.demands.len()];
    let mut order = Vec::new();
    let mut group_order = Vec::new();
    let mut total_waste = 0u64;
    let mut total_kerf = 0u64;
    let mut total_cost = 0.0;
    let mut max_drop = 0u64;
    let mut sum_squares = 0u128;
    for (layout_index, (((layout, &stock_index), indices), tree)) in result
        .layouts
        .iter()
        .zip(&solution.stock_indices)
        .zip(&solution.demand_indices)
        .zip(&solution.trees)
        .enumerate()
    {
        let stock = problem
            .packing
            .problem
            .sheets
            .get(stock_index)
            .ok_or_else(|| invalid("unknown source stock"))?;
        used[stock_index] += 1;
        if stock.quantity.is_some_and(|limit| used[stock_index] > limit) {
            return Err(invalid("stock inventory exceeded"));
        }
        let facts = inspect_tree(problem, stock_index, tree, true)?;
        if facts.parts.is_empty()
            || layout.placements.len() != facts.parts.len()
            || indices.len() != facts.parts.len()
            || layout.sheet_name != stock.name
            || (layout.width, layout.height) != (stock.width, stock.height)
            || layout.cost != stock.cost
        {
            return Err(invalid("layout metadata or placement count"));
        }
        let mut used_area = 0u64;
        for (((&demand_index, placement), part), part_position) in
            indices.iter().zip(&layout.placements).zip(&facts.parts).zip(0..)
        {
            let demand = &problem.packing.problem.demands[part.demand];
            if demand_index != part.demand
                || placement.name != demand.name
                || (placement.x, placement.y, placement.width, placement.height, placement.rotated)
                    != (
                        part.region.x,
                        part.region.y,
                        part.region.width,
                        part.region.height,
                        part.rotated,
                    )
            {
                return Err(invalid("part provenance or physical leaf geometry"));
            }
            for earlier in facts.parts.iter().take(part_position) {
                if !separated(part.region, earlier.region, stock.kerf) {
                    return Err(invalid("parts overlap or lack the physical kerf gap"));
                }
            }
            placed[demand_index] += 1;
            if placed[demand_index] > demand.quantity {
                return Err(invalid("demand quantity exceeded"));
            }
            used_area += area(part.region);
            order.push(StagedPart2D { layout_index, node_index: part.node });
            group_order.push(demand_index);
        }
        let (largest, squares) = drop_metrics(problem, stock_index, &facts.waste);
        let waste = u64::from(stock.width) * u64::from(stock.height) - used_area;
        if layout.used_area != used_area
            || layout.waste_area != waste
            || layout.kerf_area != facts.kerf_area
            || layout.largest_usable_drop_area != largest
            || layout.sum_sq_usable_drop_areas != squares
        {
            return Err(invalid("layout material, physical kerf or drop accounting"));
        }
        total_waste += waste;
        total_kerf += facts.kerf_area;
        total_cost += stock.cost;
        max_drop = max_drop.max(largest);
        sum_squares = sum_squares.saturating_add(squares);
    }
    check_groups(problem, &group_order)?;
    let unplaced: Vec<_> = problem
        .packing
        .problem
        .demands
        .iter()
        .enumerate()
        .filter_map(|(i, d)| {
            let missing = d.quantity - placed[i];
            (missing != 0).then(|| {
                let mut d = d.clone();
                d.quantity = missing;
                d
            })
        })
        .collect();
    if order != solution.production_order
        || result.unplaced != unplaced
        || solution
            .unplaced_quantities
            .iter()
            .enumerate()
            .any(|(i, &q)| q != problem.packing.problem.demands[i].quantity - placed[i])
        || result.total_waste_area != total_waste
        || result.total_kerf_area != total_kerf
        || result.total_cost != total_cost
        || result.max_usable_drop_area != max_drop
        || result.total_sum_sq_usable_drop_areas != sum_squares
    {
        return Err(invalid("production sequence, unplaced source counts or aggregate totals"));
    }
    Ok(())
}

fn constraint(field: &str, reason: &str) -> BinPackingError {
    BinPackingError::InvalidConstraint { field: field.into(), reason: reason.into() }
}

fn invalid(reason: &str) -> BinPackingError {
    BinPackingError::InvalidSolution { reason: reason.into() }
}

fn validate_problem(problem: &StagedProblem2D) -> Result<()> {
    super::constrained::validate_source(&problem.packing)?;
    let quantity = problem.packing.problem.demands.iter().map(|d| d.quantity).sum::<usize>();
    let defects = problem.packing.constraints.iter().map(|c| c.defects.len()).sum::<usize>();
    if quantity > 128 || problem.packing.problem.demands.len() > 64 || defects > 128 {
        return Err(constraint(
            "problem",
            "supports at most 128 parts, 64 demand types and 128 defects",
        ));
    }
    if !problem.groups.is_empty() && problem.groups.len() != problem.packing.problem.demands.len() {
        return Err(constraint("groups", "must be empty or match source demand count"));
    }
    let rules = &problem.rules;
    if rules.max_stages.is_some_and(|n| !(1..=16).contains(&n)) {
        return Err(constraint("max_stages", "must be 1..=16 or null for unlimited stages"));
    }
    if rules.maximum_distance_1_cuts.is_some_and(|max| max < rules.minimum_distance_1_cuts)
        || rules.maximum_distance_2_cuts.is_some_and(|max| max < rules.minimum_distance_2_cuts)
    {
        return Err(constraint("cut_distances", "maximum distance must be at least its minimum"));
    }
    Ok(())
}

static NO_RESTRICTIONS: SheetConstraints2D = SheetConstraints2D {
    trim_left: 0,
    trim_right: 0,
    trim_top: 0,
    trim_bottom: 0,
    defects: Vec::new(),
};

fn restrictions(problem: &StagedProblem2D, index: usize) -> &SheetConstraints2D {
    problem.packing.constraints.get(index).unwrap_or(&NO_RESTRICTIONS)
}

fn prepared(stock: &Sheet2D, constraints: &SheetConstraints2D) -> StagedRegion2D {
    StagedRegion2D {
        x: constraints.trim_left,
        y: constraints.trim_top,
        width: stock.width - constraints.trim_left - constraints.trim_right,
        height: stock.height - constraints.trim_top - constraints.trim_bottom,
    }
}

fn area(region: StagedRegion2D) -> u64 {
    u64::from(region.width) * u64::from(region.height)
}

fn extent(region: StagedRegion2D, axis: StagedAxis2D) -> u32 {
    match axis {
        StagedAxis2D::Vertical => region.width,
        StagedAxis2D::Horizontal => region.height,
    }
}

fn separated(a: StagedRegion2D, b: StagedRegion2D, kerf: u32) -> bool {
    let k = u64::from(kerf);
    u64::from(a.x) + u64::from(a.width) + k <= u64::from(b.x)
        || u64::from(b.x) + u64::from(b.width) + k <= u64::from(a.x)
        || u64::from(a.y) + u64::from(a.height) + k <= u64::from(b.y)
        || u64::from(b.y) + u64::from(b.height) + k <= u64::from(a.y)
}

/// Reconstruct a full-span partition independently from stored child regions.
fn partition(
    region: StagedRegion2D,
    axis: StagedAxis2D,
    offset: u32,
    kerf: u32,
) -> Result<Vec<StagedRegion2D>> {
    let length = extent(region, axis);
    if u64::from(offset) + u64::from(kerf) > u64::from(length)
        || (kerf == 0 && (offset == 0 || offset == length))
        || (offset == 0 && kerf == length)
    {
        return Err(invalid("cut blade must fit inside material and leave a nonempty child"));
    }
    let high = length - offset - kerf;
    let mut children = Vec::with_capacity(2);
    if offset != 0 {
        children.push(match axis {
            StagedAxis2D::Vertical => StagedRegion2D { width: offset, ..region },
            StagedAxis2D::Horizontal => StagedRegion2D { height: offset, ..region },
        });
    }
    if high != 0 {
        children.push(match axis {
            StagedAxis2D::Vertical => {
                StagedRegion2D { x: region.x + offset + kerf, width: high, ..region }
            }
            StagedAxis2D::Horizontal => {
                StagedRegion2D { y: region.y + offset + kerf, height: high, ..region }
            }
        });
    }
    Ok(children)
}

fn cut_crosses_defect(
    region: StagedRegion2D,
    axis: StagedAxis2D,
    offset: u32,
    kerf: u32,
    constraints: &SheetConstraints2D,
) -> bool {
    constraints.defects.iter().any(|d| {
        let (start, end, across_start, across_end, d_start, d_end, d_across_start, d_across_end) =
            match axis {
                StagedAxis2D::Vertical => (
                    u64::from(region.x) + u64::from(offset),
                    u64::from(region.x) + u64::from(offset) + u64::from(kerf),
                    u64::from(region.y),
                    u64::from(region.y) + u64::from(region.height),
                    u64::from(d.x),
                    u64::from(d.x) + u64::from(d.width),
                    u64::from(d.y),
                    u64::from(d.y) + u64::from(d.height),
                ),
                StagedAxis2D::Horizontal => (
                    u64::from(region.y) + u64::from(offset),
                    u64::from(region.y) + u64::from(offset) + u64::from(kerf),
                    u64::from(region.x),
                    u64::from(region.x) + u64::from(region.width),
                    u64::from(d.y),
                    u64::from(d.y) + u64::from(d.height),
                    u64::from(d.x),
                    u64::from(d.x) + u64::from(d.width),
                ),
            };
        across_start < d_across_end
            && d_across_start < across_end
            && if kerf == 0 {
                d_start < start && start < d_end
            } else {
                start < d_end && d_start < end
            }
    })
}

#[derive(Debug, Clone, Copy, Default)]
struct Context {
    axis: Option<StagedAxis2D>,
    stage: usize,
    first_subplate: usize,
}

fn cut_stage(context: Context, first: StagedAxis2D, axis: StagedAxis2D) -> usize {
    match context.axis {
        None => {
            if axis == first {
                1
            } else {
                2
            }
        }
        Some(previous) => context.stage + usize::from(previous != axis),
    }
}

#[derive(Debug, Clone, Copy)]
struct Part {
    node: usize,
    demand: usize,
    rotated: bool,
    region: StagedRegion2D,
}

#[derive(Debug, Default)]
struct Facts {
    parts: Vec<Part>,
    waste: Vec<StagedRegion2D>,
    kerf_area: u64,
}

#[derive(Debug, Clone, Copy, Default)]
struct Summary {
    parts: usize,
    waste: usize,
    demand: Option<usize>,
    mixed: bool,
}

/// O(N + P² + N * D) independent traversal, for nodes N, parts P and defects D.
/// No cut-generation state or cached region accounting is trusted.
fn inspect_tree(
    problem: &StagedProblem2D,
    stock_index: usize,
    tree: &StagedTree2D,
    final_waste: bool,
) -> Result<Facts> {
    let stock = problem
        .packing
        .problem
        .sheets
        .get(stock_index)
        .ok_or_else(|| invalid("unknown source stock"))?;
    let constraints = restrictions(problem, stock_index);
    let max_nodes = problem
        .packing
        .problem
        .demands
        .iter()
        .map(|d| d.quantity)
        .sum::<usize>()
        .saturating_mul(4)
        .saturating_add(1);
    if tree.nodes.is_empty()
        || tree.nodes.len() > max_nodes
        || tree.nodes[0].region != prepared(stock, constraints)
        || matches!(problem.rules.first_axis, FirstStageAxis2D::Vertical)
            && tree.first_axis != StagedAxis2D::Vertical
        || matches!(problem.rules.first_axis, FirstStageAxis2D::Horizontal)
            && tree.first_axis != StagedAxis2D::Horizontal
    {
        return Err(invalid("tree root, size or first-stage direction"));
    }
    let mut seen = vec![false; tree.nodes.len()];
    let mut contexts = vec![Context::default(); tree.nodes.len()];
    let mut cut_stages = vec![0usize; tree.nodes.len()];
    let mut visits = Vec::with_capacity(tree.nodes.len());
    let mut stack = vec![(0usize, Context::default())];
    let mut facts = Facts::default();
    let mut deepest = 0usize;
    let mut count_one = 0usize;
    let mut count_two = BTreeMap::<usize, usize>::new();
    while let Some((index, context)) = stack.pop() {
        let node = tree.nodes.get(index).ok_or_else(|| invalid("cut child index out of bounds"))?;
        if seen[index] || node.region.width == 0 || node.region.height == 0 {
            return Err(invalid("cycle, shared child or empty material region"));
        }
        seen[index] = true;
        contexts[index] = context;
        visits.push(index);
        match &node.action {
            StagedAction2D::Cut { axis, offset, children } => {
                let stage = cut_stage(context, tree.first_axis, *axis);
                cut_stages[index] = stage;
                deepest = deepest.max(stage);
                if stage == 1 {
                    count_one += 1;
                }
                if stage == 2 {
                    *count_two.entry(context.first_subplate).or_default() += 1;
                }
                if !problem.rules.cut_through_defects
                    && cut_crosses_defect(node.region, *axis, *offset, stock.kerf, constraints)
                {
                    return Err(invalid("physical cut crosses excluded defect"));
                }
                let expected = partition(node.region, *axis, *offset, stock.kerf)?;
                if children.len() != expected.len() {
                    return Err(invalid("cut material child count"));
                }
                let blade_area = u64::from(stock.kerf)
                    * u64::from(match axis {
                        StagedAxis2D::Vertical => node.region.height,
                        StagedAxis2D::Horizontal => node.region.width,
                    });
                if expected.iter().map(|&r| area(r)).sum::<u64>() + blade_area != area(node.region)
                {
                    return Err(invalid("cut material conservation"));
                }
                facts.kerf_area += blade_area;
                for (&child, region) in children.iter().zip(expected).rev() {
                    if tree.nodes.get(child).is_none_or(|node| node.region != region) {
                        return Err(invalid(
                            "child is not the reconstructed full-span material region",
                        ));
                    }
                    stack.push((
                        child,
                        Context {
                            axis: Some(*axis),
                            stage,
                            first_subplate: if stage == 1 { child } else { context.first_subplate },
                        },
                    ));
                }
            }
            StagedAction2D::Part { demand_index, rotated } => {
                let demand = problem
                    .packing
                    .problem
                    .demands
                    .get(*demand_index)
                    .ok_or_else(|| invalid("unknown source demand"))?;
                let dimensions = if *rotated {
                    (demand.height, demand.width)
                } else {
                    (demand.width, demand.height)
                };
                if (*rotated && !demand.can_rotate)
                    || dimensions != (node.region.width, node.region.height)
                {
                    return Err(invalid(
                        "part does not exactly occupy its leaf or rotation is forbidden",
                    ));
                }
                if constraints.defects.iter().any(|d| {
                    !separated(
                        node.region,
                        StagedRegion2D { x: d.x, y: d.y, width: d.width, height: d.height },
                        stock.kerf,
                    )
                }) {
                    return Err(invalid("part intersects defect or its kerf clearance"));
                }
                facts.parts.push(Part {
                    node: index,
                    demand: *demand_index,
                    rotated: *rotated,
                    region: node.region,
                });
            }
            StagedAction2D::Waste => {
                if final_waste
                    && (node.region.width < problem.rules.minimum_waste_length
                        || node.region.height < problem.rules.minimum_waste_length)
                {
                    return Err(invalid("terminal waste is below minimum waste length"));
                }
                facts.waste.push(node.region);
            }
        }
    }
    if seen.iter().any(|&seen| !seen) {
        return Err(invalid("unreachable cut-tree nodes"));
    }
    if problem.rules.maximum_number_1_cuts.is_some_and(|limit| count_one > limit)
        || problem
            .rules
            .maximum_number_2_cuts
            .is_some_and(|limit| count_two.values().any(|&n| n > limit))
    {
        return Err(invalid("physical stage cut-count limit exceeded"));
    }
    let mut summaries = vec![Summary::default(); tree.nodes.len()];
    for &index in visits.iter().rev() {
        summaries[index] = match &tree.nodes[index].action {
            StagedAction2D::Part { demand_index, .. } => {
                Summary { parts: 1, demand: Some(*demand_index), ..Summary::default() }
            }
            StagedAction2D::Waste => Summary { waste: 1, ..Summary::default() },
            StagedAction2D::Cut { children, .. } => {
                let mut summary = Summary::default();
                for &child in children {
                    let child = summaries[child];
                    summary.parts += child.parts;
                    summary.waste += child.waste;
                    summary.mixed |= child.mixed
                        || (summary.demand.is_some()
                            && child.demand.is_some()
                            && summary.demand != child.demand);
                    summary.demand = summary.demand.or(child.demand);
                }
                summary
            }
        };
    }
    for &index in &visits {
        let context = contexts[index];
        let node = &tree.nodes[index];
        let continuation = matches!(&node.action, StagedAction2D::Cut { axis, .. }
            if Some(*axis) == context.axis);
        if summaries[index].parts != 0 && !continuation && (1..=2).contains(&context.stage) {
            let Some(axis) = context.axis else {
                return Err(invalid("missing strip axis"));
            };
            check_distance(&problem.rules, context.stage, extent(node.region, axis))?;
        }
    }
    // An empty nominal first stage still has one full-width productive strip.
    if deepest == 0 || cut_stages[0] == 2 {
        check_distance(&problem.rules, 1, extent(tree.nodes[0].region, tree.first_axis))?;
    }
    let terminal = problem.rules.max_stages.unwrap_or(deepest);
    let mut finishing = deepest != 0;
    for &index in &visits {
        let stage = cut_stages[index];
        if stage == 0 || contexts[index].stage == stage {
            continue;
        }
        if stage == deepest {
            let summary = summaries[index];
            let max_parts = match problem.rules.cut_type {
                StagedCutType2D::NonExact => 1,
                StagedCutType2D::Roadef2018 => 2,
                StagedCutType2D::Exact | StagedCutType2D::Homogeneous => 0,
            };
            finishing &= summary.parts > 0 && summary.parts <= max_parts && summary.waste <= 1;
        }
        if problem.rules.cut_type == StagedCutType2D::Homogeneous
            && stage == terminal
            && summaries[index].mixed
        {
            return Err(invalid("homogeneous final-stage strip mixes source demand types"));
        }
    }
    if problem.rules.max_stages.is_some_and(|limit| deepest - usize::from(finishing) > limit) {
        return Err(invalid("effective cutting-stage limit exceeded"));
    }
    Ok(facts)
}

fn check_distance(rules: &StagedRules2D, stage: usize, distance: u32) -> Result<()> {
    let (minimum, maximum) = if stage == 1 {
        (rules.minimum_distance_1_cuts, rules.maximum_distance_1_cuts)
    } else {
        (rules.minimum_distance_2_cuts, rules.maximum_distance_2_cuts)
    };
    if distance < minimum || maximum.is_some_and(|max| distance > max) {
        return Err(invalid("productive stage strip violates cutting distance"));
    }
    Ok(())
}

fn check_groups(problem: &StagedProblem2D, order: &[usize]) -> Result<()> {
    let mut produced = vec![0usize; problem.packing.problem.demands.len()];
    for &demand in order {
        if let Some(Some(group)) = problem.groups.get(demand)
            && problem.groups.iter().take(demand).enumerate().any(|(earlier, earlier_group)| {
                earlier_group == &Some(*group)
                    && produced[earlier] != problem.packing.problem.demands[earlier].quantity
            })
        {
            return Err(invalid("source demand sequence within a cutting group is violated"));
        }
        produced[demand] += 1;
    }
    Ok(())
}

/// Cut-disconnected waste pieces are never merged, including with zero kerf.
fn drop_metrics(
    problem: &StagedProblem2D,
    stock_index: usize,
    waste: &[StagedRegion2D],
) -> (u64, u128) {
    let constraints = restrictions(problem, stock_index);
    let mut largest = 0;
    let mut squares = 0u128;
    for region in waste {
        let sheet = Sheet2D {
            name: String::new(),
            width: region.width,
            height: region.height,
            cost: 0.0,
            quantity: None,
            kerf: 0,
            edge_kerf_relief: false,
        };
        let blocked: Vec<_> = constraints
            .defects
            .iter()
            .filter_map(|d| {
                let x = d.x.max(region.x);
                let y = d.y.max(region.y);
                let right = d.x.saturating_add(d.width).min(region.x.saturating_add(region.width));
                let bottom =
                    d.y.saturating_add(d.height).min(region.y.saturating_add(region.height));
                (x < right && y < bottom).then(|| Placement2D {
                    name: String::new(),
                    x: x - region.x,
                    y: y - region.y,
                    width: right - x,
                    height: bottom - y,
                    rotated: false,
                })
            })
            .collect();
        let (area, sum) = super::drops::usable_drop_metrics(&sheet, &blocked, 0);
        largest = largest.max(area);
        squares = squares.saturating_add(sum);
    }
    (largest, squares)
}

#[derive(Debug, Clone)]
struct Bin {
    stock: usize,
    tree: StagedTree2D,
}

#[derive(Debug, Clone)]
struct State {
    bins: Vec<Bin>,
    remaining: Vec<usize>,
    placed: Vec<usize>,
    cost: f64,
    stock_area: u64,
    placed_area: u64,
}

impl State {
    fn new(problem: &StagedProblem2D) -> Self {
        Self {
            bins: Vec::new(),
            remaining: problem.packing.problem.demands.iter().map(|d| d.quantity).collect(),
            placed: vec![0; problem.packing.problem.demands.len()],
            cost: 0.0,
            stock_area: 0,
            placed_area: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Step {
    axis: StagedAxis2D,
    offset: u32,
    low: bool,
}

#[derive(Debug, Clone, Copy)]
struct Movement {
    bin: usize,
    stock: usize,
    leaf: usize,
    first: StagedAxis2D,
    demand: usize,
    rotated: bool,
    steps: [Option<Step>; 2],
    region: StagedRegion2D,
    new_bin: bool,
    cost: f64,
    residual: u64,
    part_area: u64,
    capacity: u64,
}

fn move_compare(a: &Movement, b: &Movement, objective: PackingObjective2D) -> Ordering {
    let price = a.cost.total_cmp(&b.cost);
    let count = a.new_bin.cmp(&b.new_bin);
    let primary = match objective {
        PackingObjective2D::BinCount => count
            .then_with(|| {
                if a.new_bin && b.new_bin { b.capacity.cmp(&a.capacity) } else { Ordering::Equal }
            })
            .then(price),
        PackingObjective2D::TotalCost => price.then(count),
    };
    primary.then_with(|| b.part_area.cmp(&a.part_area)).then(a.residual.cmp(&b.residual)).then_with(
        || {
            (a.bin, a.stock, a.leaf, a.first, a.demand, a.rotated, a.steps, a.region)
                .cmp(&(b.bin, b.stock, b.leaf, b.first, b.demand, b.rotated, b.steps, b.region))
        },
    )
}

fn eligible(problem: &StagedProblem2D, state: &State, demand: usize) -> bool {
    state.remaining[demand] != 0
        && problem.groups.get(demand).is_none_or(|group| {
            group.is_none()
                || !problem.groups.iter().take(demand).enumerate().any(|(i, earlier)| {
                    earlier == group
                        && state.placed[i] != problem.packing.problem.demands[i].quantity
                })
        })
}

/// O(PDN) geometric probes with a bounded 128-entry deterministic shortlist.
/// The shortlist bound is reported as truncation; it is not an exact search.
fn movements(
    problem: &StagedProblem2D,
    state: &State,
    objective: PackingObjective2D,
) -> (Vec<Movement>, bool) {
    let mut choices = Vec::<Movement>::new();
    let mut omitted = false;
    for (demand_index, demand) in problem.packing.problem.demands.iter().enumerate() {
        if !eligible(problem, state, demand_index) {
            continue;
        }
        let item = ItemInstance2D {
            name: String::new(),
            width: demand.width,
            height: demand.height,
            can_rotate: demand.can_rotate,
            fixed_rotation: None,
        };
        for stock_index in 0..problem.packing.problem.sheets.len() {
            let stock = &problem.packing.problem.sheets[stock_index];
            let constraints = restrictions(problem, stock_index);
            let used = state.bins.iter().filter(|bin| bin.stock == stock_index).count();
            let mut targets = Vec::new();
            for (bin_index, bin) in
                state.bins.iter().enumerate().filter(|(_, b)| b.stock == stock_index)
            {
                for (leaf, node) in bin.tree.nodes.iter().enumerate() {
                    if node.action == StagedAction2D::Waste {
                        targets.push((bin_index, leaf, bin.tree.first_axis, node.region, false));
                    }
                }
            }
            if stock.quantity.is_none_or(|limit| used < limit) {
                let axes: &[StagedAxis2D] = match problem.rules.first_axis {
                    FirstStageAxis2D::Any => &[StagedAxis2D::Vertical, StagedAxis2D::Horizontal],
                    FirstStageAxis2D::Vertical => &[StagedAxis2D::Vertical],
                    FirstStageAxis2D::Horizontal => &[StagedAxis2D::Horizontal],
                };
                for &axis in axes {
                    targets.push((state.bins.len(), 0, axis, prepared(stock, constraints), true));
                }
            }
            for (bin, leaf, first, region, new_bin) in targets {
                for (width, height, rotated) in item.orientations() {
                    if width > region.width || height > region.height {
                        continue;
                    }
                    for order in [
                        [StagedAxis2D::Vertical, StagedAxis2D::Horizontal],
                        [StagedAxis2D::Horizontal, StagedAxis2D::Vertical],
                    ] {
                        for corner in 0..4 {
                            let Some((steps, part)) = isolation(
                                region,
                                (width, height),
                                order,
                                corner,
                                stock.kerf,
                                constraints,
                                problem.rules.cut_through_defects,
                            ) else {
                                continue;
                            };
                            if constraints.defects.iter().any(|d| {
                                !separated(
                                    part,
                                    StagedRegion2D {
                                        x: d.x,
                                        y: d.y,
                                        width: d.width,
                                        height: d.height,
                                    },
                                    stock.kerf,
                                )
                            }) {
                                continue;
                            }
                            let movement = Movement {
                                bin,
                                stock: stock_index,
                                leaf,
                                first,
                                demand: demand_index,
                                rotated,
                                steps,
                                region: part,
                                new_bin,
                                cost: if new_bin { stock.cost } else { 0.0 },
                                residual: area(region) - u64::from(width) * u64::from(height),
                                part_area: u64::from(width) * u64::from(height),
                                // A same-orientation grid is a stock-selection
                                // heuristic, not a feasibility or stage proof.
                                capacity: (((u64::from(region.width) + u64::from(stock.kerf))
                                    / (u64::from(width) + u64::from(stock.kerf)))
                                    * ((u64::from(region.height) + u64::from(stock.kerf))
                                        / (u64::from(height) + u64::from(stock.kerf))))
                                .min(state.remaining[demand_index] as u64),
                            };
                            match choices
                                .binary_search_by(|other| move_compare(other, &movement, objective))
                            {
                                Ok(_) => {}
                                Err(index) if index < 128 => {
                                    choices.insert(index, movement);
                                    if choices.len() > 128 {
                                        choices.pop();
                                        omitted = true;
                                    }
                                }
                                Err(_) => omitted = true,
                            }
                        }
                    }
                }
            }
        }
    }
    (choices, omitted)
}

fn isolation(
    mut region: StagedRegion2D,
    size: (u32, u32),
    order: [StagedAxis2D; 2],
    corner: usize,
    kerf: u32,
    constraints: &SheetConstraints2D,
    cut_through: bool,
) -> Option<([Option<Step>; 2], StagedRegion2D)> {
    let (width, height) = size;
    let mut steps = [None, None];
    for (index, axis) in order.into_iter().enumerate() {
        let wanted = match axis {
            StagedAxis2D::Vertical => width,
            StagedAxis2D::Horizontal => height,
        };
        let length = extent(region, axis);
        if length == wanted {
            continue;
        }
        if length - wanted < kerf {
            return None;
        }
        let low = match axis {
            StagedAxis2D::Vertical => corner & 1 == 0,
            StagedAxis2D::Horizontal => corner & 2 == 0,
        };
        let offset = if low { wanted } else { length - wanted - kerf };
        if !cut_through && cut_crosses_defect(region, axis, offset, kerf, constraints) {
            return None;
        }
        let children = partition(region, axis, offset, kerf).ok()?;
        region = if low { *children.first()? } else { *children.last()? };
        steps[index] = Some(Step { axis, offset, low });
    }
    Some((steps, region))
}

fn apply(problem: &StagedProblem2D, state: &mut State, movement: Movement) -> Result<()> {
    if movement.new_bin {
        let stock = &problem.packing.problem.sheets[movement.stock];
        state.bins.push(Bin {
            stock: movement.stock,
            tree: StagedTree2D {
                first_axis: movement.first,
                nodes: vec![StagedNode2D {
                    region: prepared(stock, restrictions(problem, movement.stock)),
                    action: StagedAction2D::Waste,
                }],
            },
        });
        state.cost += stock.cost;
        state.stock_area += u64::from(stock.width) * u64::from(stock.height);
    }
    let tree = &mut state.bins[movement.bin].tree;
    let mut leaf = movement.leaf;
    for step in movement.steps.into_iter().flatten() {
        // Geometric probes already established this partition. Keep failure
        // handling explicit so a future construction change cannot panic.
        let regions = partition(
            tree.nodes[leaf].region,
            step.axis,
            step.offset,
            problem.packing.problem.sheets[movement.stock].kerf,
        )?;
        let mut children = Vec::with_capacity(regions.len());
        for region in regions {
            children.push(tree.nodes.len());
            tree.nodes.push(StagedNode2D { region, action: StagedAction2D::Waste });
        }
        let retained = if step.low { children.first() } else { children.last() };
        let &retained =
            retained.ok_or_else(|| invalid("construction produced an empty partition"))?;
        tree.nodes[leaf].action =
            StagedAction2D::Cut { axis: step.axis, offset: step.offset, children };
        leaf = retained;
    }
    tree.nodes[leaf].action =
        StagedAction2D::Part { demand_index: movement.demand, rotated: movement.rotated };
    state.remaining[movement.demand] -= 1;
    state.placed[movement.demand] += 1;
    state.placed_area += movement.part_area;
    Ok(())
}

fn state_valid(problem: &StagedProblem2D, state: &State, final_waste: bool) -> bool {
    let mut order = Vec::new();
    for bin in &state.bins {
        let Ok(facts) = inspect_tree(problem, bin.stock, &bin.tree, final_waste) else {
            return false;
        };
        order.extend(facts.parts.iter().map(|p| p.demand));
    }
    check_groups(problem, &order).is_ok()
}

fn compare_states(a: &State, b: &State, objective: PackingObjective2D) -> Ordering {
    let fulfilled = b.placed.iter().sum::<usize>().cmp(&a.placed.iter().sum::<usize>());
    let count = a.bins.len().cmp(&b.bins.len());
    let price = a.cost.total_cmp(&b.cost);
    let waste = (a.stock_area - a.placed_area).cmp(&(b.stock_area - b.placed_area));
    fulfilled.then(match objective {
        PackingObjective2D::BinCount => count.then(waste).then(price),
        PackingObjective2D::TotalCost => price.then(count).then(waste),
    })
}

fn state_key(state: &State) -> (Vec<(usize, StagedTree2D)>, Vec<usize>) {
    (state.bins.iter().map(|bin| (bin.stock, bin.tree.clone())).collect(), state.remaining.clone())
}

fn finish(
    problem: &StagedProblem2D,
    state: &State,
    explored: usize,
    truncated: bool,
) -> Result<StagedSolution2D> {
    let mut layouts = Vec::new();
    let mut indices = Vec::new();
    let mut production_order = Vec::new();
    for (layout_index, bin) in state.bins.iter().enumerate() {
        let stock = &problem.packing.problem.sheets[bin.stock];
        let facts = inspect_tree(problem, bin.stock, &bin.tree, true)?;
        let placements = facts
            .parts
            .iter()
            .map(|part| Placement2D {
                name: problem.packing.problem.demands[part.demand].name.clone(),
                x: part.region.x,
                y: part.region.y,
                width: part.region.width,
                height: part.region.height,
                rotated: part.rotated,
            })
            .collect();
        let used = facts.parts.iter().map(|part| area(part.region)).sum::<u64>();
        let (largest, squares) = drop_metrics(problem, bin.stock, &facts.waste);
        layouts.push(SheetLayout2D {
            sheet_name: stock.name.clone(),
            width: stock.width,
            height: stock.height,
            cost: stock.cost,
            placements,
            used_area: used,
            waste_area: u64::from(stock.width) * u64::from(stock.height) - used,
            kerf_area: facts.kerf_area,
            largest_usable_drop_area: largest,
            sum_sq_usable_drop_areas: squares,
        });
        indices.push(facts.parts.iter().map(|part| part.demand).collect());
        production_order.extend(
            facts.parts.iter().map(|part| StagedPart2D { layout_index, node_index: part.node }),
        );
    }
    let unplaced_quantities: Vec<_> = problem
        .packing
        .problem
        .demands
        .iter()
        .enumerate()
        .map(|(i, d)| d.quantity - state.placed[i])
        .collect();
    let unplaced = problem
        .packing
        .problem
        .demands
        .iter()
        .zip(&unplaced_quantities)
        .filter(|&(_, &q)| q != 0)
        .map(|(d, &q)| {
            let mut d = d.clone();
            d.quantity = q;
            d
        })
        .collect();
    Ok(StagedSolution2D {
        solution: TwoDSolution {
            algorithm: "staged_guillotine".into(), guillotine: true, sheet_count: layouts.len(),
            total_waste_area: layouts.iter().map(|l| l.waste_area).sum(),
            total_kerf_area: layouts.iter().map(|l| l.kerf_area).sum(),
            total_cost: layouts.iter().map(|l| l.cost).sum(),
            max_usable_drop_area: layouts.iter().map(|l| l.largest_usable_drop_area).max().unwrap_or(0),
            total_sum_sq_usable_drop_areas: layouts.iter().map(|l| l.sum_sq_usable_drop_areas)
                .fold(0u128, u128::saturating_add), layouts, unplaced,
            metrics: SolverMetrics2D { iterations: 1, explored_states: explored,
                notes: vec!["Native corner-cut heuristic; physical tree independently validated; no optimality guarantee".into()] },
        },
        stock_indices: state.bins.iter().map(|bin| bin.stock).collect(), demand_indices: indices,
        unplaced_quantities, trees: state.bins.iter().map(|bin| bin.tree.clone()).collect(),
        production_order, explored_states: explored, search_truncated: truncated,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::two_d::{RectDemand2D, Sheet2D, TwoDProblem};

    #[test]
    fn terra_batch2_machine_rule_boundaries_report_fields() {
        let mut input = problem(&[(10, 10, 1)]);
        for stages in [0, 17] {
            input.rules.max_stages = Some(stages);
            assert!(matches!(solve_2d_staged(input.clone(), StagedOptions2D::default()),
                Err(BinPackingError::InvalidConstraint { field, reason })
                if field == "max_stages" && reason == "must be 1..=16 or null for unlimited stages"));
        }
        input.rules.max_stages = Some(16);
        input.rules.minimum_distance_1_cuts = 5;
        input.rules.maximum_distance_1_cuts = Some(4);
        assert!(matches!(solve_2d_staged(input.clone(), StagedOptions2D::default()),
            Err(BinPackingError::InvalidConstraint { field, reason })
            if field == "cut_distances" && reason == "maximum distance must be at least its minimum"));
        input.rules.maximum_distance_1_cuts = Some(5);
        let result =
            solve_2d_staged(input.clone(), StagedOptions2D::default()).expect("equal limits");
        validate_2d_staged(&input, &result).expect("valid boundary rules");
    }

    #[test]
    fn terra_batch2_search_controls_and_group_shape() {
        let mut input = problem(&[(10, 10, 1)]);
        input.groups = vec![None, None];
        assert!(matches!(solve_2d_staged(input.clone(), StagedOptions2D::default()),
            Err(BinPackingError::InvalidConstraint { field, reason })
            if field == "groups" && reason == "must be empty or match source demand count"));
        input.groups = vec![None];
        for options in [
            StagedOptions2D { beam_width: 33, ..Default::default() },
            StagedOptions2D { max_states: 100_001, ..Default::default() },
        ] {
            assert!(matches!(solve_2d_staged(input.clone(), options),
                Err(BinPackingError::InvalidConstraint { field, reason })
                if field == "search" && reason == "beam_width must be 1..=32 and max_states at most 100000"));
        }
        let result = solve_2d_staged(
            input.clone(),
            StagedOptions2D { beam_width: 32, max_states: 100_000, ..Default::default() },
        )
        .expect("maximum search controls");
        assert_eq!(result.unplaced_quantities, vec![0]);
        validate_2d_staged(&input, &result).expect("boundary result");
    }

    #[test]
    fn terra_batch2_finite_stock_partial_result_accounts_for_each_source() {
        let mut input = problem(&[(10, 10, 3)]);
        input.packing.problem.sheets[0].quantity = Some(1);
        let result =
            solve_2d_staged(input.clone(), StagedOptions2D::default()).expect("limited stock");
        assert_eq!(result.stock_indices, vec![0]);
        assert_eq!(result.demand_indices, vec![vec![0]]);
        assert_eq!(result.unplaced_quantities, vec![2]);
        assert_eq!(result.solution.unplaced[0].quantity, 2);
        assert_eq!(result.solution.layouts[0].used_area, 100);
        assert_eq!(result.solution.total_waste_area, 0);
        validate_2d_staged(&input, &result).expect("partial accounting");
        let mut forged = result;
        forged.solution.unplaced[0].quantity = 1;
        assert!(matches!(validate_2d_staged(&input, &forged),
            Err(BinPackingError::InvalidSolution { reason })
            if reason == "production sequence, unplaced source counts or aggregate totals"));
    }

    #[test]
    fn terra_batch2_first_axis_provenance_is_checked_even_for_uncut_stock() {
        let mut input = problem(&[(10, 10, 1)]);
        input.rules.first_axis = FirstStageAxis2D::Horizontal;
        let result =
            solve_2d_staged(input.clone(), StagedOptions2D::default()).expect("uncut sheet");
        assert_eq!(result.trees[0].first_axis, StagedAxis2D::Horizontal);
        validate_2d_staged(&input, &result).expect("matching first axis");
        input.rules.first_axis = FirstStageAxis2D::Vertical;
        assert!(matches!(validate_2d_staged(&input, &result),
            Err(BinPackingError::InvalidSolution { reason })
            if reason == "tree root, size or first-stage direction"));
        input.rules.first_axis = FirstStageAxis2D::Any;
        validate_2d_staged(&input, &result).expect("either first axis");
    }

    fn problem(parts: &[(u32, u32, usize)]) -> StagedProblem2D {
        StagedProblem2D {
            packing: ConstrainedProblem2D {
                problem: TwoDProblem {
                    sheets: vec![Sheet2D {
                        name: "sheet".into(),
                        width: 10,
                        height: 10,
                        cost: 1.0,
                        quantity: None,
                        kerf: 0,
                        edge_kerf_relief: false,
                    }],
                    demands: parts
                        .iter()
                        .enumerate()
                        .map(|(index, &(width, height, quantity))| RectDemand2D {
                            name: format!("p{index}"),
                            width,
                            height,
                            quantity,
                            can_rotate: false,
                        })
                        .collect(),
                },
                constraints: Vec::new(),
            },
            rules: StagedRules2D {
                max_stages: Some(2),
                cut_type: StagedCutType2D::Exact,
                ..Default::default()
            },
            groups: Vec::new(),
        }
    }

    #[test]
    fn staged_construction_emits_real_two_stage_column_cuts() {
        let input = problem(&[(4, 3, 1), (4, 7, 1), (6, 5, 2)]);
        let result =
            solve_2d_staged(input.clone(), StagedOptions2D::default()).expect("staged solve");
        assert_eq!(result.solution.sheet_count, 1);
        assert!(result.solution.unplaced.is_empty());
        assert!(!result.trees[0].nodes.is_empty());
        validate_2d_staged(&input, &result).expect("independent staged validation");
    }

    #[test]
    fn non_exact_terminal_cut_is_distinct_from_exact_stage_limit() {
        let mut input = problem(&[(4, 3, 1)]);
        input.rules.max_stages = Some(1);
        let exact =
            solve_2d_staged(input.clone(), StagedOptions2D::default()).expect("exact solve");
        assert_eq!(exact.unplaced_quantities, vec![1]);
        input.rules.cut_type = StagedCutType2D::NonExact;
        let non_exact =
            solve_2d_staged(input.clone(), StagedOptions2D::default()).expect("non-exact solve");
        assert_eq!(non_exact.solution.sheet_count, 1);
        assert!(non_exact.solution.unplaced.is_empty());
    }

    #[test]
    fn three_stages_pack_a_pattern_that_two_cannot_cut() {
        // The full-height 4-wide part forces the first vertical split. The
        // remaining 6-wide plate needs a horizontal split, then another
        // vertical split for the two 3x4 parts. Total part area is exactly 100.
        let mut input = problem(&[(4, 10, 1), (6, 6, 1), (3, 4, 2)]);
        let two = solve_2d_staged(input.clone(), StagedOptions2D::default()).expect("two stages");
        assert!(two.solution.sheet_count > 1);
        input.rules.max_stages = Some(3);
        let three =
            solve_2d_staged(input.clone(), StagedOptions2D::default()).expect("three stages");
        assert_eq!(three.solution.sheet_count, 1);
        assert!(three.solution.unplaced.is_empty());
        let mut prohibited = input.clone();
        prohibited.rules.max_stages = Some(2);
        assert!(matches!(
            validate_2d_staged(&prohibited, &three),
            Err(BinPackingError::InvalidSolution { .. })
        ));
    }

    #[test]
    fn fixed_first_axis_and_empty_nominal_first_stage_are_enforced() {
        let mut input = problem(&[(4, 10, 1), (6, 5, 2)]);
        let vertical =
            solve_2d_staged(input.clone(), StagedOptions2D::default()).expect("vertical first");
        assert_eq!(vertical.solution.sheet_count, 1);
        input.rules.first_axis = FirstStageAxis2D::Horizontal;
        let horizontal =
            solve_2d_staged(input.clone(), StagedOptions2D::default()).expect("horizontal first");
        assert!(horizontal.solution.sheet_count > 1);
        assert!(horizontal.solution.unplaced.is_empty());
        assert!(validate_2d_staged(&input, &vertical).is_err());
        input.rules.first_axis = FirstStageAxis2D::Any;
        let either = solve_2d_staged(input, StagedOptions2D::default()).expect("either first axis");
        assert_eq!(either.solution.sheet_count, 1);
    }

    #[test]
    fn homogeneous_final_strip_cannot_mix_source_types() {
        let mut input = problem(&[(10, 3, 1), (10, 7, 1)]);
        let exact = solve_2d_staged(input.clone(), StagedOptions2D::default()).expect("exact");
        assert_eq!(exact.solution.sheet_count, 1);
        input.rules.cut_type = StagedCutType2D::Homogeneous;
        assert!(validate_2d_staged(&input, &exact).is_err());
        let homogeneous = solve_2d_staged(input, StagedOptions2D::default()).expect("homogeneous");
        assert_eq!(homogeneous.solution.sheet_count, 2);
        assert!(homogeneous.solution.unplaced.is_empty());
    }

    #[test]
    fn roadef_finishing_level_can_contain_two_parts() {
        // Width six prevents a non-exact solution using two side-by-side
        // single-item terminal strips on the ten-wide stock.
        let mut input = problem(&[(6, 3, 1), (6, 7, 1)]);
        input.rules.max_stages = Some(1);
        input.rules.cut_type = StagedCutType2D::NonExact;
        let non_exact =
            solve_2d_staged(input.clone(), StagedOptions2D::default()).expect("non-exact");
        assert_eq!(non_exact.solution.sheet_count, 2);
        input.rules.cut_type = StagedCutType2D::Roadef2018;
        let roadef = solve_2d_staged(input.clone(), StagedOptions2D::default()).expect("Roadef");
        assert_eq!(roadef.solution.sheet_count, 1);
        assert!(roadef.solution.unplaced.is_empty());
        input.rules.cut_type = StagedCutType2D::NonExact;
        assert!(validate_2d_staged(&input, &roadef).is_err());
    }

    #[test]
    fn physical_kerf_and_cut_through_defect_switch_are_independent_of_part_fit() {
        let mut input = problem(&[(5, 5, 2)]);
        input.packing.problem.sheets[0].width = 11;
        input.packing.problem.sheets[0].height = 11;
        input.packing.problem.sheets[0].kerf = 1;
        input.packing.problem.sheets[0].quantity = Some(1);
        input.packing.constraints = vec![SheetConstraints2D {
            defects: vec![super::super::constrained::Defect2D { x: 5, y: 8, width: 1, height: 2 }],
            ..Default::default()
        }];
        let blocked =
            solve_2d_staged(input.clone(), StagedOptions2D::default()).expect("avoid defect cuts");
        assert_eq!(blocked.unplaced_quantities, vec![2]);
        input.rules.cut_through_defects = true;
        let allowed = solve_2d_staged(input.clone(), StagedOptions2D::default())
            .expect("blade crosses defect");
        assert_eq!(allowed.solution.sheet_count, 1);
        assert!(allowed.solution.unplaced.is_empty());
        assert_eq!(allowed.solution.total_kerf_area, 21);
        assert_eq!(allowed.solution.total_waste_area, 71);
        assert_eq!(allowed.solution.max_usable_drop_area, 25);
        input.rules.cut_through_defects = false;
        assert!(validate_2d_staged(&input, &allowed).is_err());
    }

    #[test]
    fn all_four_prepared_trims_define_the_root_and_material_totals() {
        let mut input = problem(&[(10, 10, 1)]);
        input.packing.problem.sheets[0].width = 14;
        input.packing.problem.sheets[0].height = 16;
        input.packing.constraints = vec![SheetConstraints2D {
            trim_left: 1,
            trim_right: 3,
            trim_top: 2,
            trim_bottom: 4,
            defects: Vec::new(),
        }];
        let result =
            solve_2d_staged(input.clone(), StagedOptions2D::default()).expect("trimmed stock");
        assert_eq!(
            result.trees[0].nodes[0].region,
            StagedRegion2D { x: 1, y: 2, width: 10, height: 10 }
        );
        assert_eq!(result.solution.total_waste_area, 124);
        assert_eq!(result.solution.total_kerf_area, 0);
        assert_eq!(result.solution.max_usable_drop_area, 0);
        validate_2d_staged(&input, &result).expect("trim material accounting");
    }

    #[test]
    fn productive_strip_distances_and_stage_cut_counts_change_feasibility() {
        let mut input = problem(&[(4, 10, 1), (6, 10, 1)]);
        let unrestricted =
            solve_2d_staged(input.clone(), StagedOptions2D::default()).expect("unrestricted");
        assert_eq!(unrestricted.solution.sheet_count, 1);
        input.rules.minimum_distance_1_cuts = 5;
        let minimum = solve_2d_staged(input.clone(), StagedOptions2D::default())
            .expect("minimum strip width");
        assert_eq!(minimum.unplaced_quantities, vec![1, 0]);
        assert!(validate_2d_staged(&input, &unrestricted).is_err());
        input.rules.minimum_distance_1_cuts = 0;
        input.rules.maximum_distance_1_cuts = Some(5);
        let maximum = solve_2d_staged(input.clone(), StagedOptions2D::default())
            .expect("maximum strip width");
        assert_eq!(maximum.unplaced_quantities, vec![0, 1]);
        input.rules.maximum_distance_1_cuts = None;
        input.rules.maximum_number_1_cuts = Some(0);
        let count =
            solve_2d_staged(input, StagedOptions2D::default()).expect("zero stage-one cuts");
        assert_eq!(count.unplaced_quantities, vec![1, 1]);

        let mut second = problem(&[(4, 3, 1), (4, 7, 1), (6, 5, 2)]);
        second.rules.maximum_number_2_cuts = Some(0);
        let restricted = solve_2d_staged(second.clone(), StagedOptions2D::default())
            .expect("stage-two cut count");
        assert!(restricted.unplaced_quantities.iter().sum::<usize>() > 0);
        second.rules.maximum_number_2_cuts = None;
        second.rules.maximum_distance_2_cuts = Some(6);
        let limited_height =
            solve_2d_staged(second, StagedOptions2D::default()).expect("stage-two distance");
        assert_eq!(limited_height.unplaced_quantities[1], 1);
    }

    #[test]
    fn cutting_groups_enforce_full_source_type_predecessors() {
        let mut input = problem(&[(5, 6, 1), (5, 7, 1), (5, 3, 1), (5, 4, 1)]);
        let independent =
            solve_2d_staged(input.clone(), StagedOptions2D::default()).expect("independent types");
        assert_eq!(independent.solution.sheet_count, 1);
        input.groups = vec![Some(0), Some(0), Some(1), Some(1)];
        assert!(validate_2d_staged(&input, &independent).is_err());
        let grouped = solve_2d_staged(input.clone(), StagedOptions2D::default())
            .expect("ordered cutting groups");
        assert_eq!(grouped.solution.sheet_count, 2);
        assert!(grouped.solution.unplaced.is_empty());
        validate_2d_staged(&input, &grouped).expect("group production order");

        input.packing.problem.sheets[0].quantity = Some(1);
        let limited = solve_2d_staged(input.clone(), StagedOptions2D::default())
            .expect("grouped partial supply");
        assert!(limited.unplaced_quantities.iter().sum::<usize>() > 0);
        validate_2d_staged(&input, &limited).expect("partial grouped result");
    }

    #[test]
    fn duplicate_labels_keep_indexed_identity_and_bounded_search_keeps_baseline() {
        let mut input = problem(&[(4, 3, 1), (4, 7, 1), (6, 5, 2)]);
        for demand in &mut input.packing.problem.demands {
            demand.name = "duplicate".into();
        }
        let baseline =
            solve_2d_staged(input.clone(), StagedOptions2D { max_states: 0, ..Default::default() })
                .expect("greedy incumbent");
        assert_eq!(baseline.explored_states, 0);
        assert!(baseline.search_truncated);
        let tiny =
            solve_2d_staged(input.clone(), StagedOptions2D { max_states: 1, ..Default::default() })
                .expect("tiny search");
        assert!(tiny.explored_states <= 1);
        assert!(tiny.search_truncated);
        assert_eq!(tiny.unplaced_quantities, baseline.unplaced_quantities);
        assert_eq!(tiny.solution.sheet_count, baseline.solution.sheet_count);
        let encoded = serde_json::to_string(&tiny).expect("serialize indexed trees");
        let decoded =
            serde_json::from_str::<StagedSolution2D>(&encoded).expect("deserialize indexed trees");
        assert_eq!(tiny, decoded);
        validate_2d_staged(&input, &decoded).expect("indexed duplicate labels");
        let mut corrupt = tiny;
        corrupt.demand_indices[0][0] = (corrupt.demand_indices[0][0] + 1) % 3;
        assert!(validate_2d_staged(&input, &corrupt).is_err());
    }

    #[test]
    fn cost_objective_changes_stock_selection_during_construction() {
        let mut input = problem(&[(5, 5, 4)]);
        let small = Sheet2D {
            name: "small".into(),
            width: 5,
            height: 5,
            cost: 1.0,
            quantity: None,
            kerf: 0,
            edge_kerf_relief: false,
        };
        input.packing.problem.sheets[0].cost = 10.0;
        input.packing.problem.sheets.push(small);
        let count =
            solve_2d_staged(input.clone(), StagedOptions2D { max_states: 0, ..Default::default() })
                .expect("count objective during baseline construction");
        assert_eq!(count.solution.sheet_count, 1);
        let cheap = solve_2d_staged(
            input,
            StagedOptions2D {
                objective: PackingObjective2D::TotalCost,
                max_states: 0,
                ..Default::default()
            },
        )
        .expect("cost objective");
        assert_eq!(cheap.solution.total_cost, 4.0);
        assert_eq!(cheap.solution.sheet_count, 4);
    }

    #[test]
    fn independent_validator_rejects_corrupted_physical_tree_and_totals() {
        let input = problem(&[(4, 3, 1), (4, 7, 1), (6, 5, 2)]);
        let valid = solve_2d_staged(input.clone(), StagedOptions2D::default()).expect("valid tree");
        let mut corrupt = valid.clone();
        corrupt.trees[0].nodes[1].region.width = 99;
        assert!(validate_2d_staged(&input, &corrupt).is_err());
        let mut corrupt = valid.clone();
        if let StagedAction2D::Cut { children, .. } = &mut corrupt.trees[0].nodes[0].action {
            children[0] = 0;
        }
        assert!(validate_2d_staged(&input, &corrupt).is_err());
        let mut corrupt = valid.clone();
        if let StagedAction2D::Cut { offset, .. } = &mut corrupt.trees[0].nodes[0].action {
            *offset += 1;
        }
        assert!(validate_2d_staged(&input, &corrupt).is_err());
        let mut corrupt = valid.clone();
        corrupt.trees[0].nodes.push(StagedNode2D {
            region: StagedRegion2D { x: 0, y: 0, width: 1, height: 1 },
            action: StagedAction2D::Waste,
        });
        assert!(validate_2d_staged(&input, &corrupt).is_err());
        let mut corrupt = valid.clone();
        corrupt.unplaced_quantities[0] = 1;
        assert!(validate_2d_staged(&input, &corrupt).is_err());
        let mut corrupt = valid.clone();
        corrupt.solution.total_kerf_area += 1;
        assert!(validate_2d_staged(&input, &corrupt).is_err());
        let mut corrupt = valid;
        corrupt.production_order.reverse();
        assert!(validate_2d_staged(&input, &corrupt).is_err());
    }

    #[test]
    fn malformed_rules_fail_with_structured_errors() {
        let mut input = problem(&[(4, 3, 1)]);
        input.rules.max_stages = Some(0);
        assert!(matches!(
            solve_2d_staged(input.clone(), StagedOptions2D::default()),
            Err(BinPackingError::InvalidConstraint { .. })
        ));
        input.rules.max_stages = Some(2);
        input.groups = vec![Some(1), None];
        assert!(matches!(
            solve_2d_staged(input.clone(), StagedOptions2D::default()),
            Err(BinPackingError::InvalidConstraint { .. })
        ));
        input.groups.clear();
        input.rules.minimum_distance_2_cuts = 10;
        input.rules.maximum_distance_2_cuts = Some(9);
        assert!(matches!(
            solve_2d_staged(input.clone(), StagedOptions2D::default()),
            Err(BinPackingError::InvalidConstraint { .. })
        ));
        input.rules.maximum_distance_2_cuts = Some(10);
        assert!(matches!(
            solve_2d_staged(input, StagedOptions2D { beam_width: 0, ..Default::default() }),
            Err(BinPackingError::InvalidConstraint { .. })
        ));
    }

    #[test]
    fn minimum_second_strip_and_terminal_waste_lengths_are_enforced() {
        let mut input = problem(&[(4, 3, 1), (4, 7, 1), (6, 5, 2)]);
        input.rules.minimum_distance_2_cuts = 4;
        let bounded =
            solve_2d_staged(input, StagedOptions2D::default()).expect("minimum second-stage strip");
        assert_eq!(bounded.unplaced_quantities[0], 1);
        let mut input = problem(&[(7, 10, 1)]);
        input.rules.minimum_waste_length = 4;
        let waste = solve_2d_staged(input.clone(), StagedOptions2D::default())
            .expect("minimum waste length");
        assert_eq!(waste.unplaced_quantities, vec![1]);
        input.packing.problem.demands[0].width = 6;
        let enough =
            solve_2d_staged(input.clone(), StagedOptions2D::default()).expect("large enough waste");
        assert_eq!(enough.unplaced_quantities, vec![0]);
        validate_2d_staged(&input, &enough).expect("minimum waste validation");
    }

    #[test]
    fn physical_rotation_and_large_dimensions_are_checked_without_overflow() {
        let mut input = problem(&[(4, 10, 1)]);
        input.packing.problem.sheets[0].height = 4;
        input.packing.problem.demands[0].can_rotate = true;
        let rotated =
            solve_2d_staged(input.clone(), StagedOptions2D::default()).expect("rotated part");
        assert!(rotated.solution.layouts[0].placements[0].rotated);
        input.packing.problem.demands[0].can_rotate = false;
        assert!(validate_2d_staged(&input, &rotated).is_err());

        let large_dimension = 1 << 30;
        let mut wide = problem(&[(large_dimension, 1, 2)]);
        wide.packing.problem.sheets[0].width = large_dimension;
        wide.packing.problem.sheets[0].height = 1;
        let result =
            solve_2d_staged(wide.clone(), StagedOptions2D::default()).expect("wide dimensions");
        assert_eq!(result.solution.sheet_count, 2);
        assert_eq!(result.solution.total_waste_area, 0);
        validate_2d_staged(&wide, &result).expect("wide dimensions validation");
        wide.packing.problem.sheets[0].height = large_dimension;
        wide.packing.problem.demands[0].quantity = 17;
        assert!(matches!(
            solve_2d_staged(wide, StagedOptions2D::default()),
            Err(BinPackingError::InvalidConstraint { .. })
        ));
    }

    #[test]
    fn staged_complete_strip_tilings_match_an_independent_kerf_oracle() {
        // These 72 exact tilings have n equal parts and n-1 blade bands. No
        // remainder or heuristic choice is needed to know the physical answer.
        for horizontal in [false, true] {
            for part_length in 5..=7u32 {
                for kerf in 0..=2u32 {
                    for quantity in 1..=4u32 {
                        let stock_length = quantity * part_length + (quantity - 1) * kerf;
                        let (width, height) =
                            if horizontal { (5, part_length) } else { (part_length, 5) };
                        let mut input = problem(&[(width, height, quantity as usize)]);
                        let stock = &mut input.packing.problem.sheets[0];
                        stock.width = if horizontal { 5 } else { stock_length };
                        stock.height = if horizontal { stock_length } else { 5 };
                        stock.kerf = kerf;
                        input.rules.max_stages = Some(1);
                        input.rules.first_axis = if horizontal {
                            FirstStageAxis2D::Horizontal
                        } else {
                            FirstStageAxis2D::Vertical
                        };
                        input.rules.maximum_number_1_cuts = Some((quantity - 1) as usize);
                        let result = solve_2d_staged(
                            input.clone(),
                            StagedOptions2D { max_states: 0, ..Default::default() },
                        )
                        .expect("complete straight-strip tiling");
                        assert_eq!(result.solution.sheet_count, 1);
                        assert!(result.solution.unplaced.is_empty());
                        let layout = &result.solution.layouts[0];
                        assert_eq!(layout.placements.len(), quantity as usize);
                        assert_eq!(
                            layout.used_area,
                            u64::from(quantity) * u64::from(part_length) * 5
                        );
                        let blade_area = u64::from(quantity - 1) * u64::from(kerf) * 5;
                        assert_eq!(layout.kerf_area, blade_area);
                        assert_eq!(layout.waste_area, blade_area);
                        assert_eq!(layout.largest_usable_drop_area, 0);
                        for (position, part) in (0u32..).zip(&layout.placements) {
                            let origin = position * (part_length + kerf);
                            assert_eq!(
                                (part.x, part.y),
                                if horizontal { (0, origin) } else { (origin, 0) }
                            );
                            assert_eq!((part.width, part.height), (width, height));
                        }
                        validate_2d_staged(&input, &result)
                            .expect("independent physical validation");
                        if quantity > 1 {
                            input.rules.maximum_number_1_cuts = Some((quantity - 2) as usize);
                            assert!(matches!(
                                validate_2d_staged(&input, &result),
                                Err(BinPackingError::InvalidSolution { .. })
                            ));
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn staged_zero_kerf_cut_can_touch_a_defect_but_cannot_cross_its_interior() {
        let mut input = problem(&[(5, 4, 2)]);
        input.packing.problem.sheets[0].quantity = Some(1);
        input.packing.constraints = vec![SheetConstraints2D {
            defects: vec![super::super::constrained::Defect2D { x: 4, y: 8, width: 2, height: 1 }],
            ..Default::default()
        }];
        let options = StagedOptions2D { max_states: 0, ..Default::default() };
        let prohibited = solve_2d_staged(input.clone(), options.clone()).expect("protected defect");
        assert_eq!(prohibited.unplaced_quantities, [2]);
        input.rules.cut_through_defects = true;
        let allowed = solve_2d_staged(input.clone(), options.clone()).expect("cut through defect");
        assert!(allowed.solution.unplaced.is_empty());
        assert_eq!(allowed.solution.total_kerf_area, 0);
        input.rules.cut_through_defects = false;
        assert!(matches!(
            validate_2d_staged(&input, &allowed),
            Err(BinPackingError::InvalidSolution { .. })
        ));
        // Moving the defect's left edge onto the vertical x=5 cut leaves its
        // interior in waste. A zero-width blade may touch that boundary.
        input.packing.constraints[0].defects[0].x = 5;
        input.packing.constraints[0].defects[0].width = 1;
        let touching = solve_2d_staged(input.clone(), options).expect("cut on defect boundary");
        assert!(touching.solution.unplaced.is_empty());
        validate_2d_staged(&input, &touching).expect("boundary contact preserves material");
    }

    #[test]
    fn staged_validator_rejects_forged_blades_children_and_source_leaves() {
        let input = problem(&[(4, 10, 1), (6, 10, 1)]);
        let valid = solve_2d_staged(input.clone(), StagedOptions2D::default()).expect("one cut");
        assert_eq!(valid.solution.sheet_count, 1);
        assert!(matches!(valid.trees[0].nodes[0].action, StagedAction2D::Cut { .. }));
        let mut forgeries = Vec::new();
        let mut forged = valid.clone();
        forged.trees[0].nodes.clear();
        forgeries.push(forged);
        let mut forged = valid.clone();
        if let StagedAction2D::Cut { children, .. } = &mut forged.trees[0].nodes[0].action {
            children.pop().expect("second child");
        }
        forgeries.push(forged);
        for offset in [0, 10, u32::MAX] {
            let mut forged = valid.clone();
            if let StagedAction2D::Cut { offset: actual, .. } = &mut forged.trees[0].nodes[0].action
            {
                *actual = offset;
            }
            forgeries.push(forged);
        }
        for action in [
            StagedAction2D::Part { demand_index: usize::MAX, rotated: false },
            StagedAction2D::Part { demand_index: 0, rotated: true },
            StagedAction2D::Waste,
        ] {
            let mut forged = valid.clone();
            let part = forged.trees[0]
                .nodes
                .iter_mut()
                .find(|node| matches!(node.action, StagedAction2D::Part { demand_index: 0, .. }))
                .expect("source zero leaf");
            part.action = action;
            forgeries.push(forged);
        }
        for forged in forgeries {
            assert!(matches!(
                validate_2d_staged(&input, &forged),
                Err(BinPackingError::InvalidSolution { .. })
            ));
        }
    }

    #[test]
    fn staged_validator_reconstructs_source_inventory_and_metadata() {
        let mut input = problem(&[(10, 10, 2)]);
        input.packing.problem.sheets[0].quantity = Some(2);
        let valid = solve_2d_staged(input.clone(), StagedOptions2D::default()).expect("two sheets");
        assert_eq!(valid.solution.sheet_count, 2);
        input.packing.problem.sheets[0].quantity = Some(1);
        assert!(matches!(
            validate_2d_staged(&input, &valid),
            Err(BinPackingError::InvalidSolution { .. })
        ));
        input.packing.problem.sheets[0].quantity = Some(2);
        let mut forgeries = Vec::new();
        let mut forged = valid.clone();
        forged.stock_indices[0] = usize::MAX;
        forgeries.push(forged);
        let mut forged = valid.clone();
        forged.solution.layouts[0].sheet_name = "different source".into();
        forgeries.push(forged);
        let mut forged = valid.clone();
        forged.solution.layouts[0].placements[0].name = "different demand".into();
        forgeries.push(forged);
        let mut forged = valid.clone();
        forged.solution.layouts[0].cost = f64::NAN;
        forgeries.push(forged);
        let mut forged = valid.clone();
        forged.solution.guillotine = false;
        forgeries.push(forged);
        let mut forged = valid.clone();
        forged.solution.metrics.explored_states = forged.explored_states + 1;
        forgeries.push(forged);
        let mut forged = valid;
        forged.explored_states = 100_001;
        forged.solution.metrics.explored_states = forged.explored_states;
        forgeries.push(forged);
        for forged in forgeries {
            assert!(matches!(
                validate_2d_staged(&input, &forged),
                Err(BinPackingError::InvalidSolution { .. })
            ));
        }
    }

    #[test]
    fn certificate_guard_matrix_isolates_layout_and_aggregate_fields() {
        let input = problem(&[(4, 10, 1), (6, 10, 1)]);
        let valid =
            solve_2d_staged(input.clone(), StagedOptions2D::default()).expect("valid certificate");
        validate_2d_staged(&input, &valid).expect("untampered certificate");
        let mut cases = Vec::new();
        let mut x = valid.clone();
        x.solution.sheet_count += 1;
        cases.push((x, "layout counts, provenance or search accounting"));
        let mut x = valid.clone();
        x.stock_indices.clear();
        cases.push((x, "layout counts, provenance or search accounting"));
        let mut x = valid.clone();
        x.demand_indices.clear();
        cases.push((x, "layout counts, provenance or search accounting"));
        let mut x = valid.clone();
        x.trees.clear();
        cases.push((x, "layout counts, provenance or search accounting"));
        let mut x = valid.clone();
        x.unplaced_quantities.clear();
        cases.push((x, "layout counts, provenance or search accounting"));
        let mut x = valid.clone();
        x.solution.layouts[0].placements.pop();
        cases.push((x, "layout metadata or placement count"));
        let mut x = valid.clone();
        x.demand_indices[0].pop();
        cases.push((x, "layout metadata or placement count"));
        let mut x = valid.clone();
        x.solution.layouts[0].width += 1;
        cases.push((x, "layout metadata or placement count"));
        let mut x = valid.clone();
        x.demand_indices[0][0] = 1;
        cases.push((x, "part provenance or physical leaf geometry"));
        let mut x = valid.clone();
        x.solution.layouts[0].placements[0].x += 1;
        cases.push((x, "part provenance or physical leaf geometry"));
        let mut x = valid.clone();
        x.solution.layouts[0].used_area += 1;
        cases.push((x, "layout material, physical kerf or drop accounting"));
        let mut x = valid.clone();
        x.solution.layouts[0].waste_area += 1;
        cases.push((x, "layout material, physical kerf or drop accounting"));
        let mut x = valid.clone();
        x.solution.layouts[0].kerf_area += 1;
        cases.push((x, "layout material, physical kerf or drop accounting"));
        let mut x = valid.clone();
        x.solution.layouts[0].largest_usable_drop_area += 1;
        cases.push((x, "layout material, physical kerf or drop accounting"));
        let mut x = valid.clone();
        x.solution.layouts[0].sum_sq_usable_drop_areas += 1;
        cases.push((x, "layout material, physical kerf or drop accounting"));
        let mut x = valid.clone();
        x.production_order.reverse();
        cases.push((x, "production sequence, unplaced source counts or aggregate totals"));
        let mut x = valid.clone();
        x.unplaced_quantities[0] += 1;
        cases.push((x, "production sequence, unplaced source counts or aggregate totals"));
        let mut x = valid.clone();
        x.solution.total_waste_area += 1;
        cases.push((x, "production sequence, unplaced source counts or aggregate totals"));
        let mut x = valid.clone();
        x.solution.total_kerf_area += 1;
        cases.push((x, "production sequence, unplaced source counts or aggregate totals"));
        let mut x = valid.clone();
        x.solution.total_cost += 1.0;
        cases.push((x, "production sequence, unplaced source counts or aggregate totals"));
        let mut x = valid.clone();
        x.solution.max_usable_drop_area += 1;
        cases.push((x, "production sequence, unplaced source counts or aggregate totals"));
        let mut x = valid.clone();
        x.solution.total_sum_sq_usable_drop_areas += 1;
        cases.push((x, "production sequence, unplaced source counts or aggregate totals"));
        for (forged, expected) in cases {
            assert!(
                matches!(validate_2d_staged(&input, &forged),
                Err(BinPackingError::InvalidSolution { reason }) if reason == expected),
                "expected {expected}"
            );
        }
    }

    #[test]
    fn tree_reachability_and_part_quantity_are_independently_checked() {
        let input = problem(&[(5, 10, 2)]);
        let valid =
            solve_2d_staged(input.clone(), StagedOptions2D::default()).expect("two equal leaves");
        validate_2d_staged(&input, &valid).expect("valid tree");
        let mut shared = valid.clone();
        if let StagedAction2D::Cut { children, .. } = &mut shared.trees[0].nodes[0].action {
            children[1] = children[0];
        }
        assert!(
            matches!(validate_2d_staged(&input, &shared), Err(BinPackingError::InvalidSolution { reason }) if reason == "child is not the reconstructed full-span material region")
        );
        let mut unreachable = valid.clone();
        unreachable.trees[0].nodes.push(StagedNode2D {
            region: StagedRegion2D { x: 0, y: 0, width: 1, height: 1 },
            action: StagedAction2D::Waste,
        });
        assert!(
            matches!(validate_2d_staged(&input, &unreachable), Err(BinPackingError::InvalidSolution { reason }) if reason == "unreachable cut-tree nodes")
        );
        let mut less = input.clone();
        less.packing.problem.demands[0].quantity = 1;
        assert!(
            matches!(validate_2d_staged(&less, &valid), Err(BinPackingError::InvalidSolution { reason }) if reason == "demand quantity exceeded")
        );
    }

    #[test]
    fn blade_partition_edges_preserve_nonempty_material_and_area() {
        let region = StagedRegion2D { x: 2, y: 3, width: 10, height: 8 };
        for axis in [StagedAxis2D::Vertical, StagedAxis2D::Horizontal] {
            let length = extent(region, axis);
            for (offset, kerf) in [(0, 0), (length, 0), (0, length), (length, 1), (u32::MAX, 1)] {
                assert!(
                    matches!(partition(region, axis, offset, kerf), Err(BinPackingError::InvalidSolution { reason }) if reason == "cut blade must fit inside material and leave a nonempty child")
                );
            }
            for (offset, kerf, count) in [(0, 1, 1), (length - 1, 1, 1), (1, 0, 2), (1, 1, 2)] {
                let pieces = partition(region, axis, offset, kerf).expect("valid blade");
                assert_eq!(pieces.len(), count);
                let span = match axis {
                    StagedAxis2D::Vertical => region.height,
                    StagedAxis2D::Horizontal => region.width,
                };
                assert_eq!(
                    pieces.iter().map(|&piece| area(piece)).sum::<u64>()
                        + u64::from(kerf) * u64::from(span),
                    area(region)
                );
            }
        }
    }

    #[test]
    fn defect_intersection_respects_blade_and_cross_span_boundaries() {
        let region = StagedRegion2D { x: 0, y: 0, width: 10, height: 10 };
        for axis in [StagedAxis2D::Vertical, StagedAxis2D::Horizontal] {
            let defect = |along, across| match axis {
                StagedAxis2D::Vertical => {
                    super::super::constrained::Defect2D { x: along, y: across, width: 2, height: 2 }
                }
                StagedAxis2D::Horizontal => {
                    super::super::constrained::Defect2D { x: across, y: along, width: 2, height: 2 }
                }
            };
            for (along, across, offset, kerf, expected) in [
                (4, 0, 5, 0, true),
                (5, 0, 5, 0, false),
                (3, 0, 5, 0, false),
                (5, 0, 4, 1, false),
                (4, 0, 5, 1, true),
                (4, 10, 5, 1, false),
            ] {
                let constraints = SheetConstraints2D {
                    defects: vec![defect(along, across)],
                    ..Default::default()
                };
                assert_eq!(cut_crosses_defect(region, axis, offset, kerf, &constraints), expected);
            }
        }
    }
}
