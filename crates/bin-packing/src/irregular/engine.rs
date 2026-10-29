//! Indexed contact-anchor construction and bounded polygon-aware beam search.

use std::cmp::Ordering;

use geo::{BoundingRect, CoordsIter, Validation};

use super::geometry::{
    PreparedShape, Role, actual_bounds, anchor_vertices, constraint, covered_by, overlaps,
    prepare_shape, rectangle_center, transform,
};
use super::{
    IrregularLayout, IrregularObjective, IrregularOptions, IrregularPlacement, IrregularProblem,
    IrregularSolution, Point2D,
};
use crate::{BinPackingError, Result};

const MAX_PARTS: usize = 64;
const MAX_CONSTRUCTION_PROBES: usize = 32_768;
const MAX_TOTAL_VERTICES: usize = 16_384;
const MAX_ANCHOR_PROPOSALS: usize = 16_384;
const ALGORITHM: &str = "native_irregular_contact";

struct Oriented {
    geometry: PreparedShape,
    angle: f64,
    mirrored: bool,
}

struct PreparedBin {
    geometry: PreparedShape,
    defects: Vec<PreparedShape>,
}

struct PreparedProblem {
    bins: Vec<PreparedBin>,
    demands: Vec<PreparedShape>,
    orientations: Vec<Vec<Oriented>>,
    total_quantity: usize,
    angular_sampling: bool,
}

fn charge_vertices(total: &mut usize, shape: &PreparedShape) -> Result<()> {
    *total = total.saturating_add(shape.geometry.coords_count());
    if *total > MAX_TOTAL_VERTICES {
        return Err(constraint("geometry", "prepared source shapes/defects exceed 16384 vertices"));
    }
    Ok(())
}

fn validate_options(options: &IrregularOptions) -> Result<()> {
    if options.max_states > 200_000
        || !(1..=32).contains(&options.beam_width)
        || !(1..=512).contains(&options.max_candidates_per_item)
        || !(1..=128).contains(&options.max_orientations)
    {
        return Err(constraint(
            "search",
            "requires states 0..=200000, beam 1..=32, candidates 1..=512 and orientations 1..=128",
        ));
    }
    if !options.rotation_step_degrees.is_finite()
        || !(0.1..=180.0).contains(&options.rotation_step_degrees)
        || !options.curve_tolerance.is_finite()
        || !(1e-8..=10_000.0).contains(&options.curve_tolerance)
    {
        return Err(constraint(
            "geometry",
            "angular step must be 0.1..=180 degrees and curve tolerance 1e-8..=10000",
        ));
    }
    for value in [options.item_spacing, options.bin_spacing, options.defect_spacing] {
        if !value.is_finite() || !(0.0..=1_000_000.0).contains(&value) {
            return Err(constraint("spacing", "must be finite, nonnegative and at most 1000000"));
        }
    }
    Ok(())
}

fn open_objective(objective: IrregularObjective) -> bool {
    matches!(
        objective,
        IrregularObjective::OpenDimensionX
            | IrregularObjective::OpenDimensionY
            | IrregularObjective::OpenDimensionXY
    )
}

fn prepare(problem: &IrregularProblem, options: &IrregularOptions) -> Result<PreparedProblem> {
    validate_options(options)?;
    if problem.bins.is_empty()
        || problem.bins.len() > 8
        || problem.demands.is_empty()
        || problem.demands.len() > 32
        || problem.demands.iter().any(|d| d.quantity == 0 || d.quantity > MAX_PARTS)
    {
        return Err(constraint(
            "problem",
            "requires 1..=8 stocks, 1..=32 demands and 1..=64 copies per row",
        ));
    }
    let total_quantity = problem.demands.iter().fold(0usize, |n, d| n.saturating_add(d.quantity));
    if total_quantity > MAX_PARTS {
        return Err(constraint("demands.quantity", "at most 64 total parts are supported"));
    }
    if open_objective(options.objective) && problem.bins.len() != 1 {
        return Err(constraint("objective", "open dimensions require one maximum bin type"));
    }
    for bin in &problem.bins {
        if bin.name.trim().is_empty()
            || !bin.cost.is_finite()
            || bin.cost < 0.0
            || !(bin.cost * (total_quantity + 1) as f64).is_finite()
            || !bin.spacing.is_finite()
            || !(0.0..=1_000_000.0).contains(&bin.spacing)
            || bin.defects.len() > 32
        {
            return Err(constraint(
                "bins",
                "invalid label, cost, spacing, defect count or aggregate cost",
            ));
        }
    }
    let mut vertices = 0usize;
    let mut bins = Vec::new();
    for bin in &problem.bins {
        let geometry = prepare_shape(&bin.shape, options, Role::Bin)?;
        charge_vertices(&mut vertices, &geometry)?;
        let mut defects = Vec::new();
        for defect in &bin.defects {
            let prepared = prepare_shape(defect, options, Role::Item)?;
            if !covered_by(&prepared, &geometry, 0.0) {
                return Err(constraint(
                    "bins.defects",
                    "defects must lie inside the usable bin at the selected tolerance",
                ));
            }
            charge_vertices(&mut vertices, &prepared)?;
            defects.push(prepared);
        }
        bins.push(PreparedBin { geometry, defects });
    }
    let mut demands = Vec::new();
    let mut orientations = Vec::new();
    let mut angular_sampling = false;
    for demand in &problem.demands {
        if demand.name.trim().is_empty()
            || !demand.profit.is_finite()
            || demand.profit < 0.0
            || !(demand.profit * (total_quantity + 1) as f64).is_finite()
            || demand.allowed_rotations.is_empty()
            || demand.allowed_rotations.len() > 128
        {
            return Err(constraint("demands", "invalid label, profit or rotation range count"));
        }
        let shape = prepare_shape(&demand.shape, options, Role::Item)?;
        charge_vertices(&mut vertices, &shape)?;
        let mut angles = Vec::new();
        for range in &demand.allowed_rotations {
            if !range.start.is_finite()
                || !range.end.is_finite()
                || !(0.0..=360.0).contains(&range.start)
                || !(range.start..=360.0).contains(&range.end)
            {
                return Err(constraint(
                    "allowed_rotations",
                    "requires inclusive finite intervals in 0..=360 degrees",
                ));
            }
            let span = range.end - range.start;
            angular_sampling |= span > 0.0 && shape.circle.is_none();
            if shape.circle.is_some() {
                if angles.is_empty() {
                    angles.push(range.start);
                }
                continue;
            }
            let steps = (span / options.rotation_step_degrees).ceil() as usize;
            if steps > 3_600 {
                return Err(constraint("allowed_rotations", "too many angular samples"));
            }
            if steps == 0 {
                angles.push(range.start);
            } else {
                for step in 0..=steps {
                    angles.push(if step == steps {
                        range.end
                    } else {
                        range.start + span * step as f64 / steps as f64
                    });
                }
                angles.push(range.start + span / 2.0);
            }
        }
        angles.sort_by(f64::total_cmp);
        angles.dedup();
        // Zero and 360 produce the same rigid geometry. Keep a legal witness.
        if angles.first() == Some(&0.0) && angles.last() == Some(&360.0) {
            angles.pop();
        }
        let mirrors = if demand.can_mirror && shape.circle.is_none() { 2 } else { 1 };
        if angles.len().saturating_mul(mirrors) > options.max_orientations {
            return Err(constraint(
                "max_orientations",
                "increase the orientation cap or angular step to cover the declared intervals",
            ));
        }
        let mut variants = Vec::new();
        for mirrored in [false, true].into_iter().take(mirrors) {
            for &angle in &angles {
                variants.push(Oriented {
                    geometry: transform(&shape, angle, mirrored, 0.0, 0.0),
                    angle,
                    mirrored,
                });
            }
        }
        demands.push(shape);
        orientations.push(variants);
    }
    Ok(PreparedProblem { bins, demands, orientations, total_quantity, angular_sampling })
}

#[derive(Clone)]
struct Placed {
    placement: IrregularPlacement,
    geometry: PreparedShape,
}

#[derive(Clone)]
struct OpenBin {
    source: usize,
    placements: Vec<Placed>,
}

#[derive(Clone)]
struct State {
    layouts: Vec<OpenBin>,
    usage: Vec<usize>,
    counts: Vec<usize>,
}

impl State {
    fn new(problem: &IrregularProblem) -> Self {
        Self {
            layouts: Vec::new(),
            usage: vec![0; problem.bins.len()],
            counts: vec![0; problem.demands.len()],
        }
    }

    fn placed_count(&self) -> usize {
        self.counts.iter().sum()
    }

    fn cost(&self, problem: &IrregularProblem) -> f64 {
        self.usage
            .iter()
            .zip(&problem.bins)
            .map(|(&quantity, bin)| quantity as f64 * bin.cost)
            .sum()
    }

    fn profit(&self, problem: &IrregularProblem) -> f64 {
        self.counts
            .iter()
            .zip(&problem.demands)
            .map(|(&quantity, demand)| quantity as f64 * demand.profit)
            .sum()
    }

    fn extents(&self, prepared: &PreparedProblem) -> (f64, f64) {
        let mut width = 0.0f64;
        let mut height = 0.0f64;
        for layout in &self.layouts {
            let origin = prepared.bins[layout.source].geometry.original_min;
            for placed in &layout.placements {
                let bounds = actual_bounds(&placed.geometry);
                width = width.max(bounds.max().x - origin.x);
                height = height.max(bounds.max().y - origin.y);
            }
        }
        (width, height)
    }
}

fn compare_states(
    first: &State,
    second: &State,
    problem: &IrregularProblem,
    prepared: &PreparedProblem,
    objective: IrregularObjective,
) -> Ordering {
    let count = || second.placed_count().cmp(&first.placed_count());
    let cost = || first.cost(problem).total_cmp(&second.cost(problem));
    let bins = || first.layouts.len().cmp(&second.layouts.len());
    let extent = || {
        let (fw, fh) = first.extents(prepared);
        let (sw, sh) = second.extents(prepared);
        match objective {
            IrregularObjective::OpenDimensionX => fw.total_cmp(&sw).then_with(|| fh.total_cmp(&sh)),
            IrregularObjective::OpenDimensionY => fh.total_cmp(&sh).then_with(|| fw.total_cmp(&sw)),
            _ => (fw * fh).total_cmp(&(sw * sh)).then_with(|| (fw + fh).total_cmp(&(sw + sh))),
        }
    };
    match objective {
        IrregularObjective::Knapsack => second
            .profit(problem)
            .total_cmp(&first.profit(problem))
            .then_with(cost)
            .then_with(bins)
            .then_with(count),
        IrregularObjective::TotalCost => count().then_with(cost).then_with(bins).then_with(extent),
        IrregularObjective::OpenDimensionX
        | IrregularObjective::OpenDimensionY
        | IrregularObjective::OpenDimensionXY => {
            count().then_with(bins).then_with(extent).then_with(cost)
        }
        _ => count().then_with(bins).then_with(cost).then_with(extent),
    }
}

#[derive(Clone)]
struct Candidate {
    target: usize,
    source: usize,
    placed: Placed,
    capacity: f64,
}

fn feasible(
    candidate: &PreparedShape,
    bin: &PreparedBin,
    placed: &[Placed],
    options: &IrregularOptions,
    boundary_spacing: f64,
) -> bool {
    // Reject exact circle conflicts cheaply before validating the sampled
    // polygon. Invalid transformed polygons never reach topology predicates.
    if candidate.circle.is_some()
        && ((bin.geometry.circle.is_some()
            && !covered_by(candidate, &bin.geometry, boundary_spacing))
            || bin
                .defects
                .iter()
                .filter(|d| d.circle.is_some())
                .any(|d| overlaps(candidate, d, options.defect_spacing))
            || placed
                .iter()
                .filter(|p| p.geometry.circle.is_some())
                .any(|p| overlaps(candidate, &p.geometry, options.item_spacing)))
    {
        return false;
    }
    candidate.geometry.is_valid()
        && covered_by(candidate, &bin.geometry, boundary_spacing)
        && !bin.defects.iter().any(|defect| overlaps(candidate, defect, options.defect_spacing))
        && !placed.iter().any(|other| overlaps(candidate, &other.geometry, options.item_spacing))
}

fn push_anchor(anchors: &mut Vec<Point2D>, point: Point2D) {
    if point.x.is_finite()
        && point.y.is_finite()
        && point.x.abs() <= 4_000_000.0
        && point.y.abs() <= 4_000_000.0
    {
        anchors.push(point);
    }
}

// Contact generation is O((P + D) * V²) for sampled vertex sets V<=68,
// followed by sorting. Every acceptance predicate uses the full polygons.
fn anchors(
    bin: &PreparedBin,
    existing: &[Placed],
    oriented: &PreparedShape,
    options: &IrregularOptions,
    boundary_spacing: f64,
) -> (Vec<Point2D>, bool) {
    let mut priority = Vec::new();
    let bounds = actual_bounds(oriented);
    let container = actual_bounds(&bin.geometry);
    let width = bounds.width();
    let height = bounds.height();
    for x in [container.min().x + boundary_spacing, container.max().x - boundary_spacing - width] {
        for y in
            [container.min().y + boundary_spacing, container.max().y - boundary_spacing - height]
        {
            push_anchor(&mut priority, Point2D { x: x - bounds.min().x, y: y - bounds.min().y });
        }
    }
    let center = rectangle_center(container);
    let item_center = rectangle_center(bounds);
    push_anchor(
        &mut priority,
        Point2D { x: center.x - item_center.x, y: center.y - item_center.y },
    );
    // Circle centers have analytic contact loci. These witnesses avoid losing
    // useful tangencies among thousands of sampled polygon-vertex alignments.
    if let Some(item) = oriented.circle {
        if let Some(container) = bin.geometry.circle {
            let radius = container.radius - item.radius - boundary_spacing;
            if radius >= 0.0 {
                for index in 0..8 {
                    let (sin, cos) = (f64::from(index) * std::f64::consts::FRAC_PI_4).sin_cos();
                    push_anchor(
                        &mut priority,
                        Point2D {
                            x: container.center.x + radius * cos - item.center.x,
                            y: container.center.y + radius * sin - item.center.y,
                        },
                    );
                }
            }
        }
        for (other, gap) in existing
            .iter()
            .map(|p| (&p.geometry, options.item_spacing))
            .chain(bin.defects.iter().map(|d| (d, options.defect_spacing)))
        {
            if let Some(other) = other.circle {
                for index in 0..8 {
                    let (sin, cos) = (f64::from(index) * std::f64::consts::FRAC_PI_4).sin_cos();
                    let radius = item.radius + other.radius + gap;
                    push_anchor(
                        &mut priority,
                        Point2D {
                            x: other.center.x + radius * cos - item.center.x,
                            y: other.center.y + radius * sin - item.center.y,
                        },
                    );
                }
            }
        }
    }
    let (item_vertices, mut omitted) = anchor_vertices(oriented);
    let (bin_vertices, bin_sampled) = anchor_vertices(&bin.geometry);
    omitted |= bin_sampled;
    let mut contacts: Vec<_> = bin_vertices.into_iter().map(|p| (p, boundary_spacing)).collect();
    for other in existing {
        let (vertices, sampled) = anchor_vertices(&other.geometry);
        omitted |= sampled;
        contacts.extend(vertices.into_iter().map(|p| (p, options.item_spacing)));
        // Hole lower-left anchors make enclosed material voids available to
        // unrelated parts instead of treating a holed item as its bounding box.
        for polygon in &other.geometry.geometry.0 {
            for hole in polygon.interiors() {
                if let Some(hole) = hole.bounding_rect() {
                    push_anchor(
                        &mut priority,
                        Point2D {
                            x: hole.min().x + options.item_spacing - bounds.min().x,
                            y: hole.min().y + options.item_spacing - bounds.min().y,
                        },
                    );
                }
            }
        }
        let other_bounds = actual_bounds(&other.geometry);
        for (x, y) in [
            (other_bounds.max().x + options.item_spacing, other_bounds.min().y),
            (other_bounds.min().x, other_bounds.max().y + options.item_spacing),
            (other_bounds.min().x - width - options.item_spacing, other_bounds.min().y),
            (other_bounds.min().x, other_bounds.min().y - height - options.item_spacing),
        ] {
            push_anchor(&mut priority, Point2D { x: x - bounds.min().x, y: y - bounds.min().y });
        }
    }
    for defect in &bin.defects {
        let (vertices, sampled) = anchor_vertices(defect);
        omitted |= sampled;
        contacts.extend(vertices.into_iter().map(|p| (p, options.defect_spacing)));
    }
    let mut points = Vec::new();
    'contacts: for (contact, gap) in contacts {
        for vertex in &item_vertices {
            for (dx, dy) in [(0.0, 0.0), (gap, 0.0), (-gap, 0.0), (0.0, gap), (0.0, -gap)]
                .into_iter()
                .take(if gap == 0.0 { 1 } else { 5 })
            {
                if points.len() == MAX_ANCHOR_PROPOSALS {
                    omitted = true;
                    break 'contacts;
                }
                push_anchor(
                    &mut points,
                    Point2D { x: contact.x - vertex.x + dx, y: contact.y - vertex.y + dy },
                );
            }
        }
    }
    points.sort_by(|a, b| a.y.total_cmp(&b.y).then_with(|| a.x.total_cmp(&b.x)));
    points.dedup();
    priority.extend(points);
    // Remove translations whose true bounds cannot possibly fit. This cheap
    // filter does not decide polygon feasibility or discard hole interlocking.
    priority.retain(|p| {
        p.x + bounds.min().x >= container.min().x
            && p.y + bounds.min().y >= container.min().y
            && p.x + bounds.max().x <= container.max().x
            && p.y + bounds.max().y <= container.max().y
    });
    (priority, omitted)
}

fn compare_candidates(
    first: &Candidate,
    second: &Candidate,
    state: &State,
    problem: &IrregularProblem,
    objective: IrregularObjective,
) -> Ordering {
    let first_new = first.target == state.layouts.len();
    let second_new = second.target == state.layouts.len();
    let cost = |candidate: &Candidate, fresh: bool| {
        if fresh { problem.bins[candidate.source].cost } else { 0.0 }
    };
    let bounds_first = actual_bounds(&first.placed.geometry);
    let bounds_second = actual_bounds(&second.placed.geometry);
    let position = match objective {
        IrregularObjective::OpenDimensionX => bounds_first
            .max()
            .x
            .total_cmp(&bounds_second.max().x)
            .then_with(|| bounds_first.max().y.total_cmp(&bounds_second.max().y)),
        IrregularObjective::OpenDimensionY => bounds_first
            .max()
            .y
            .total_cmp(&bounds_second.max().y)
            .then_with(|| bounds_first.max().x.total_cmp(&bounds_second.max().x)),
        _ => bounds_first
            .min()
            .y
            .total_cmp(&bounds_second.min().y)
            .then_with(|| bounds_first.min().x.total_cmp(&bounds_second.min().x)),
    };
    let material = match objective {
        IrregularObjective::TotalCost | IrregularObjective::Knapsack => {
            let first_cost = cost(first, first_new) / first.capacity.max(1.0);
            let second_cost = cost(second, second_new) / second.capacity.max(1.0);
            first_cost.total_cmp(&second_cost).then_with(|| first_new.cmp(&second_new))
        }
        _ => first_new.cmp(&second_new).then_with(|| second.capacity.total_cmp(&first.capacity)),
    };
    material
        .then(position)
        .then_with(|| first.source.cmp(&second.source))
        .then_with(|| {
            first
                .placed
                .placement
                .rotation_degrees
                .total_cmp(&second.placed.placement.rotation_degrees)
        })
        .then_with(|| first.placed.placement.mirrored.cmp(&second.placed.placement.mirrored))
}

struct Work {
    spent: usize,
    limit: usize,
    omitted: bool,
}

fn candidates(
    problem: &IrregularProblem,
    prepared: &PreparedProblem,
    options: &IrregularOptions,
    state: &State,
    demand: usize,
    work: &mut Work,
    per_call: usize,
) -> Vec<Candidate> {
    let mut retained = Vec::new();
    let mut call_probes = 0usize;
    for target in 0..state.layouts.len() + problem.bins.len() {
        let existing = state.layouts.get(target);
        let source = match existing {
            Some(bin) => bin.source,
            None => target - state.layouts.len(),
        };
        if existing.is_none()
            && (problem.bins[source].quantity.is_some_and(|cap| state.usage[source] >= cap)
                || (open_objective(options.objective) && !state.layouts.is_empty()))
        {
            continue;
        }
        let placed = existing.map_or(&[][..], |bin| bin.placements.as_slice());
        let boundary_spacing = options.bin_spacing.max(problem.bins[source].spacing);
        for oriented in &prepared.orientations[demand] {
            if work.spent == work.limit || call_probes == per_call {
                work.omitted = true;
                return retained;
            }
            let (translations, omitted) = anchors(
                &prepared.bins[source],
                placed,
                &oriented.geometry,
                options,
                boundary_spacing,
            );
            work.omitted |= omitted || translations.len() > options.max_candidates_per_item;
            for translation in translations.into_iter().take(options.max_candidates_per_item) {
                if work.spent == work.limit || call_probes == per_call {
                    work.omitted = true;
                    return retained;
                }
                work.spent += 1;
                call_probes += 1;
                let geometry =
                    transform(&oriented.geometry, 0.0, false, translation.x, translation.y);
                if !feasible(&geometry, &prepared.bins[source], placed, options, boundary_spacing) {
                    continue;
                }
                let candidate = Candidate {
                    target: if existing.is_none() { state.layouts.len() } else { target },
                    source,
                    capacity: (prepared.bins[source].geometry.material_area
                        / prepared.demands[demand].material_area)
                        .floor()
                        .min(problem.demands[demand].quantity as f64)
                        .max(1.0),
                    placed: Placed {
                        placement: IrregularPlacement {
                            demand_index: demand,
                            x: translation.x,
                            y: translation.y,
                            rotation_degrees: oriented.angle,
                            mirrored: oriented.mirrored,
                        },
                        geometry,
                    },
                };
                let insertion = retained.partition_point(|other| {
                    !compare_candidates(other, &candidate, state, problem, options.objective)
                        .is_gt()
                });
                retained.insert(insertion, candidate);
                if retained.len() > options.beam_width {
                    retained.truncate(options.beam_width);
                    work.omitted = true;
                }
            }
        }
    }
    retained
}

fn place(state: &mut State, candidate: Candidate) {
    if candidate.target == state.layouts.len() {
        state.usage[candidate.source] += 1;
        state.layouts.push(OpenBin { source: candidate.source, placements: Vec::new() });
    }
    state.counts[candidate.placed.placement.demand_index] += 1;
    state.layouts[candidate.target].placements.push(candidate.placed);
}

fn finish(
    problem: &IrregularProblem,
    prepared: &PreparedProblem,
    options: IrregularOptions,
    state: State,
    work: Work,
    construction_probes: usize,
) -> IrregularSolution {
    let cost = state.cost(problem);
    let profit = state.profit(problem);
    let unplaced_quantities: Vec<_> =
        problem.demands.iter().zip(&state.counts).map(|(d, &count)| d.quantity - count).collect();
    let layouts = state
        .layouts
        .into_iter()
        .map(|bin| {
            let origin = prepared.bins[bin.source].geometry.original_min;
            let used_width = bin
                .placements
                .iter()
                .map(|p| actual_bounds(&p.geometry).max().x - origin.x)
                .fold(0.0f64, f64::max);
            let used_height = bin
                .placements
                .iter()
                .map(|p| actual_bounds(&p.geometry).max().y - origin.y)
                .fold(0.0f64, f64::max);
            IrregularLayout {
                bin_index: bin.source,
                used_area: bin
                    .placements
                    .iter()
                    .map(|p| prepared.demands[p.placement.demand_index].material_area)
                    .sum(),
                used_width,
                used_height,
                placements: bin.placements.into_iter().map(|p| p.placement).collect(),
            }
        })
        .collect();
    IrregularSolution {
        algorithm: ALGORITHM.into(),
        options,
        layouts,
        feasible: unplaced_quantities.iter().all(|&count| count == 0),
        unplaced_quantities,
        total_cost: cost,
        total_profit: profit,
        explored_states: work.spent,
        construction_probes,
        search_truncated: work.omitted || prepared.angular_sampling,
    }
}

pub(super) fn solve(
    problem: IrregularProblem,
    options: IrregularOptions,
) -> Result<IrregularSolution> {
    let prepared = prepare(&problem, &options)?;
    let mut order = Vec::new();
    for (index, demand) in problem.demands.iter().enumerate() {
        order.extend(std::iter::repeat_n(index, demand.quantity));
    }
    order.sort_by(|&first, &second| {
        let area = prepared.demands[second]
            .material_area
            .total_cmp(&prepared.demands[first].material_area);
        if options.objective == IrregularObjective::Knapsack {
            let a = problem.demands[first].profit / prepared.demands[first].material_area;
            let b = problem.demands[second].profit / prepared.demands[second].material_area;
            b.total_cmp(&a).then(area).then_with(|| first.cmp(&second))
        } else {
            area.then_with(|| first.cmp(&second))
        }
    });
    let mut construction = Work { spent: 0, limit: MAX_CONSTRUCTION_PROBES, omitted: false };
    let mut best = State::new(&problem);
    for &demand in &order {
        if options.objective == IrregularObjective::Knapsack
            && problem.demands[demand].profit == 0.0
        {
            continue;
        }
        let mut moves = candidates(
            &problem,
            &prepared,
            &options,
            &best,
            demand,
            &mut construction,
            MAX_CONSTRUCTION_PROBES,
        );
        if !moves.is_empty() {
            place(&mut best, moves.swap_remove(0));
        }
    }
    let mut work = Work {
        spent: 0,
        limit: options.max_states,
        omitted: construction.omitted || options.max_states == 0,
    };
    let mut beam = vec![State::new(&problem)];
    for (depth, &demand) in order.iter().enumerate() {
        if work.spent == work.limit {
            work.omitted = true;
            break;
        }
        let mut next = Vec::new();
        let allowance = ((work.limit - work.spent)
            / beam.len().saturating_mul(order.len() - depth).max(1))
        .max(1);
        for state in &beam {
            let moves =
                candidates(&problem, &prepared, &options, state, demand, &mut work, allowance);
            for movement in moves {
                let mut child = state.clone();
                place(&mut child, movement);
                if depth + 1 == order.len() {
                    if compare_states(&child, &best, &problem, &prepared, options.objective).is_lt()
                    {
                        best = child;
                    }
                } else {
                    next.push(child);
                }
            }
            // Every skipped copy remains visible in final source accounting.
            let skipped = state.clone();
            if depth + 1 == order.len() {
                if compare_states(&skipped, &best, &problem, &prepared, options.objective).is_lt() {
                    best = skipped;
                }
            } else {
                next.push(skipped);
            }
        }
        if depth + 1 == order.len() {
            break;
        }
        next.sort_by(|a, b| compare_states(a, b, &problem, &prepared, options.objective));
        if next.len() > options.beam_width {
            next.truncate(options.beam_width);
            work.omitted = true;
        }
        beam = next;
    }
    let result = finish(&problem, &prepared, options, best, work, construction.spent);
    validate(&problem, &result)?;
    Ok(result)
}

fn invalid(reason: &str) -> BinPackingError {
    BinPackingError::InvalidSolution { reason: reason.into() }
}

pub(super) fn validate(problem: &IrregularProblem, solution: &IrregularSolution) -> Result<()> {
    let prepared = prepare(problem, &solution.options)?;
    if solution.algorithm != ALGORITHM
        || solution.unplaced_quantities.len() != problem.demands.len()
        || solution.layouts.len() > prepared.total_quantity
        || solution.explored_states > solution.options.max_states
        || solution.construction_probes > MAX_CONSTRUCTION_PROBES
    {
        return Err(invalid("invalid irregular result metadata or work counts"));
    }
    if open_objective(solution.options.objective) && solution.layouts.len() > 1 {
        return Err(invalid("open-dimension output must use at most one bin"));
    }
    let mut state = State::new(problem);
    for layout in &solution.layouts {
        let bin =
            prepared.bins.get(layout.bin_index).ok_or_else(|| invalid("unknown bin index"))?;
        if layout.placements.is_empty() || layout.placements.len() > prepared.total_quantity {
            return Err(invalid("empty or excessive placements"));
        }
        state.usage[layout.bin_index] += 1;
        if problem.bins[layout.bin_index]
            .quantity
            .is_some_and(|limit| state.usage[layout.bin_index] > limit)
        {
            return Err(invalid("bin inventory exceeded"));
        }
        let mut placed = Vec::new();
        let mut area = 0.0;
        let mut width = 0.0f64;
        let mut height = 0.0f64;
        let origin = bin.geometry.original_min;
        for placement in &layout.placements {
            let source = problem
                .demands
                .get(placement.demand_index)
                .ok_or_else(|| invalid("unknown demand index"))?;
            if !placement.x.is_finite()
                || !placement.y.is_finite()
                || placement.x.abs() > 4_000_000.0
                || placement.y.abs() > 4_000_000.0
                || !placement.rotation_degrees.is_finite()
                || !source
                    .allowed_rotations
                    .iter()
                    .any(|range| (range.start..=range.end).contains(&placement.rotation_degrees))
                || (placement.mirrored && !source.can_mirror)
            {
                return Err(invalid("illegal source translation, rotation or reflection"));
            }
            let geometry = transform(
                &prepared.demands[placement.demand_index],
                placement.rotation_degrees,
                placement.mirrored,
                placement.x,
                placement.y,
            );
            if !feasible(
                &geometry,
                bin,
                &placed,
                &solution.options,
                solution.options.bin_spacing.max(problem.bins[layout.bin_index].spacing),
            ) {
                return Err(invalid(
                    "placement violates containment, material non-overlap or spacing",
                ));
            }
            state.counts[placement.demand_index] += 1;
            if state.counts[placement.demand_index] > source.quantity {
                return Err(invalid("source demand was placed too many times"));
            }
            let bounds = actual_bounds(&geometry);
            width = width.max(bounds.max().x - origin.x);
            height = height.max(bounds.max().y - origin.y);
            area += prepared.demands[placement.demand_index].material_area;
            placed.push(Placed { placement: placement.clone(), geometry });
        }
        if layout.used_area != area || layout.used_width != width || layout.used_height != height {
            return Err(invalid("layout area or extents disagree with source transforms"));
        }
        state.layouts.push(OpenBin { source: layout.bin_index, placements: placed });
    }
    for ((&placed, &unplaced), demand) in
        state.counts.iter().zip(&solution.unplaced_quantities).zip(&problem.demands)
    {
        if placed.checked_add(unplaced) != Some(demand.quantity) {
            return Err(invalid("placed and unplaced quantities do not conserve source demand"));
        }
    }
    if solution.total_cost != state.cost(problem)
        || solution.total_profit != state.profit(problem)
        || solution.feasible != solution.unplaced_quantities.iter().all(|&quantity| quantity == 0)
    {
        return Err(invalid("incorrect irregular totals or feasibility flag"));
    }
    Ok(())
}
