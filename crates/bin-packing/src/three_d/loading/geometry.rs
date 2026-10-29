use std::collections::BTreeMap;

use super::{
    AxleLimits3D, AxleLoads3D, FloorAxis3D, LoadingBinLayout3D, LoadingBinRules3D,
    LoadingProblem3D, StackColumn3D, StackItemRules3D, UnloadingConstraint3D,
};
use crate::{BinPackingError, Result};

pub(super) const MAX_ITEMS: usize = 128;
pub(super) const MAX_TYPES: usize = 32;
pub(super) const MAX_BINS: usize = 16;
pub(super) const MAX_STATES: usize = 100_000;
pub(super) const MAX_BEAM: usize = 32;
const MAX_SPACES: usize = 128;

static DEFAULT_ITEM_RULE: StackItemRules3D = StackItemRules3D {
    stackability_id: 0,
    nesting_height: 0,
    maximum_stackability: None,
    maximum_weight_above: None,
    group_id: 0,
};
static DEFAULT_BIN_RULE: LoadingBinRules3D =
    LoadingBinRules3D { maximum_stack_density: None, axle_limits: None, defects: Vec::new() };

pub(super) fn constraint(field: &str, reason: &str) -> BinPackingError {
    BinPackingError::InvalidConstraint { field: field.into(), reason: reason.into() }
}

pub(super) fn invalid(reason: &str) -> BinPackingError {
    BinPackingError::InvalidSolution { reason: reason.into() }
}

pub(super) fn item_rule(problem: &LoadingProblem3D, index: usize) -> &StackItemRules3D {
    problem.item_rules.get(index).unwrap_or(&DEFAULT_ITEM_RULE)
}

pub(super) fn bin_rule(problem: &LoadingProblem3D, index: usize) -> &LoadingBinRules3D {
    problem.bin_rules.get(index).unwrap_or(&DEFAULT_BIN_RULE)
}

pub(super) fn weight(problem: &LoadingProblem3D, index: usize) -> u64 {
    problem.packing.item_weights.get(index).copied().unwrap_or(0)
}

pub(super) fn payload_limit(problem: &LoadingProblem3D, index: usize) -> Option<u64> {
    problem.packing.bin_max_weights.get(index).copied().flatten()
}

pub(super) fn validate_problem(problem: &LoadingProblem3D) -> Result<()> {
    problem.packing.problem.validate()?;
    let source = &problem.packing.problem;
    let quantity: usize = source.demands.iter().map(|demand| demand.quantity).sum();
    if quantity > MAX_ITEMS || source.demands.len() > MAX_TYPES || source.bins.len() > MAX_BINS {
        return Err(constraint("problem", "loading supports 128 items, 32 demands and 16 bins"));
    }
    if !problem.packing.item_weights.is_empty()
        && problem.packing.item_weights.len() != source.demands.len()
    {
        return Err(constraint("item_weights", "must be empty or match demand count"));
    }
    if !problem.packing.bin_max_weights.is_empty()
        && problem.packing.bin_max_weights.len() != source.bins.len()
    {
        return Err(constraint("bin_max_weights", "must be empty or match bin count"));
    }
    if !problem.item_rules.is_empty() && problem.item_rules.len() != source.demands.len() {
        return Err(constraint("item_rules", "must be empty or match demand count"));
    }
    if !problem.bin_rules.is_empty() && problem.bin_rules.len() != source.bins.len() {
        return Err(constraint("bin_rules", "must be empty or match bin count"));
    }
    if problem.bin_rules.iter().map(|rules| rules.defects.len()).sum::<usize>() > 256 {
        return Err(constraint("defects", "at most 256 floor defects are supported"));
    }
    for (index, demand) in source.demands.iter().enumerate() {
        let rule = item_rule(problem, index);
        if rule.nesting_height >= demand.height {
            return Err(constraint("nesting_height", "must be smaller than full item height"));
        }
        if rule.maximum_stackability == Some(0) {
            return Err(constraint("maximum_stackability", "must be positive when specified"));
        }
        if !demand.allowed_rotations.contains(super::Rotation3D::Xyz)
            && !demand.allowed_rotations.contains(super::Rotation3D::Zyx)
        {
            return Err(constraint("allowed_rotations", "loading requires an upright orientation"));
        }
    }
    for (index, bin) in source.bins.iter().enumerate() {
        if !(bin.cost * (quantity + 1) as f64).is_finite() {
            return Err(constraint("bins.cost", "aggregate material cost could overflow"));
        }
        let rule = bin_rule(problem, index);
        if rule.maximum_stack_density.is_some_and(|density| density.denominator == 0) {
            return Err(constraint("maximum_stack_density", "denominator must be positive"));
        }
        for defect in &rule.defects {
            if defect.width == 0
                || defect.depth == 0
                || u64::from(defect.x) + u64::from(defect.width) > u64::from(bin.width)
                || u64::from(defect.z) + u64::from(defect.depth) > u64::from(bin.depth)
            {
                return Err(constraint("defects", "must be positive rectangles inside the floor"));
            }
        }
        if let Some(axles) = rule.axle_limits {
            let extent = match axles.axis {
                FloorAxis3D::X => bin.width,
                FloorAxis3D::Z => bin.depth,
            };
            if axles.front_position >= axles.rear_position || axles.rear_position > extent {
                return Err(constraint("axle_limits", "positions must increase within the bin"));
            }
            if axles.front_tare > axles.front_maximum || axles.rear_tare > axles.rear_maximum {
                return Err(constraint(
                    "axle_limits",
                    "tare reactions exceed allowed axle weights",
                ));
            }
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct FloorRect {
    pub x: u32,
    pub z: u32,
    pub width: u32,
    pub depth: u32,
}

impl FloorRect {
    pub fn area(self) -> u64 {
        u64::from(self.width) * u64::from(self.depth)
    }
    pub fn right(self) -> u32 {
        self.x.saturating_add(self.width)
    }
    pub fn far(self) -> u32 {
        self.z.saturating_add(self.depth)
    }
    fn contains(self, other: Self) -> bool {
        self.x <= other.x
            && self.z <= other.z
            && u64::from(self.x) + u64::from(self.width)
                >= u64::from(other.x) + u64::from(other.width)
            && u64::from(self.z) + u64::from(self.depth)
                >= u64::from(other.z) + u64::from(other.depth)
    }
}

pub(super) fn stack_rect(stack: &StackColumn3D) -> FloorRect {
    FloorRect { x: stack.x, z: stack.z, width: stack.width, depth: stack.depth }
}

pub(super) fn overlaps(a: FloorRect, b: FloorRect) -> bool {
    u64::from(a.x) < u64::from(b.x) + u64::from(b.width)
        && u64::from(b.x) < u64::from(a.x) + u64::from(a.width)
        && u64::from(a.z) < u64::from(b.z) + u64::from(b.depth)
        && u64::from(b.z) < u64::from(a.z) + u64::from(a.depth)
}

// Each subtraction emits at most four rectangles per free region. Quadratic
// containment pruning keeps maximal regions; the explicit cap can omit choices
// but never introduces occupied material into retained regions.
fn subtract(spaces: &mut Vec<FloorRect>, excluded: FloorRect) -> bool {
    let mut next = Vec::new();
    for region in std::mem::take(spaces) {
        if !overlaps(region, excluded) {
            next.push(region);
            continue;
        }
        if excluded.x > region.x {
            next.push(FloorRect { width: excluded.x - region.x, ..region });
        }
        if excluded.right() < region.right() {
            next.push(FloorRect {
                x: excluded.right(),
                width: region.right() - excluded.right(),
                ..region
            });
        }
        if excluded.z > region.z {
            next.push(FloorRect { depth: excluded.z - region.z, ..region });
        }
        if excluded.far() < region.far() {
            next.push(FloorRect {
                z: excluded.far(),
                depth: region.far() - excluded.far(),
                ..region
            });
        }
    }
    next.sort_unstable();
    next.dedup();
    let mut retained: Vec<_> = next
        .iter()
        .enumerate()
        .filter(|(index, region)| {
            !next
                .iter()
                .enumerate()
                .any(|(other, container)| other != *index && container.contains(**region))
        })
        .map(|(_, &region)| region)
        .collect();
    retained.sort_by(|a, b| b.area().cmp(&a.area()).then_with(|| a.cmp(b)));
    let truncated = retained.len() > MAX_SPACES;
    retained.truncate(MAX_SPACES);
    *spaces = retained;
    truncated
}

pub(super) fn floor_spaces(
    problem: &LoadingProblem3D,
    bin_index: usize,
    stacks: &[StackColumn3D],
) -> (Vec<FloorRect>, bool) {
    let bin = &problem.packing.problem.bins[bin_index];
    let mut spaces = vec![FloorRect { x: 0, z: 0, width: bin.width, depth: bin.depth }];
    let mut truncated = false;
    for defect in &bin_rule(problem, bin_index).defects {
        truncated |= subtract(
            &mut spaces,
            FloorRect { x: defect.x, z: defect.z, width: defect.width, depth: defect.depth },
        );
    }
    for stack in stacks {
        truncated |= subtract(&mut spaces, stack_rect(stack));
    }
    (spaces, truncated)
}

pub(super) fn density_fits(
    problem: &LoadingProblem3D,
    bin: usize,
    rect: FloorRect,
    mass: u64,
) -> bool {
    bin_rule(problem, bin).maximum_stack_density.is_none_or(|density| {
        u128::from(mass) * u128::from(density.denominator)
            <= u128::from(density.numerator) * u128::from(rect.area())
    })
}

pub(super) fn unloading_pair(
    mode: UnloadingConstraint3D,
    a: FloorRect,
    group_a: u32,
    b: FloorRect,
    group_b: u32,
) -> bool {
    if group_a == group_b || mode == UnloadingConstraint3D::None {
        return true;
    }
    let (early, late) = if group_a < group_b { (a, b) } else { (b, a) };
    match mode {
        UnloadingConstraint3D::None => true,
        UnloadingConstraint3D::IncreasingX => early.x >= late.x,
        UnloadingConstraint3D::IncreasingZ => early.z >= late.z,
        UnloadingConstraint3D::OnlyXMovements => {
            let share_lane = u64::from(a.z) < u64::from(b.z) + u64::from(b.depth)
                && u64::from(b.z) < u64::from(a.z) + u64::from(a.depth);
            !share_lane || u64::from(early.x) >= u64::from(late.x) + u64::from(late.width)
        }
        UnloadingConstraint3D::OnlyZMovements => {
            let share_lane = u64::from(a.x) < u64::from(b.x) + u64::from(b.width)
                && u64::from(b.x) < u64::from(a.x) + u64::from(a.width);
            !share_lane || u64::from(early.z) >= u64::from(late.z) + u64::from(late.depth)
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Addition {
    pub demand: usize,
    pub rect: FloorRect,
}

// Cargo weights are checked to fit u64; coordinates are bounded by 32768.
// Twice-moments and tare terms therefore fit i128 with substantial headroom.
fn reactions(axles: AxleLimits3D, mass: i128, twice_moment: i128) -> Option<AxleLoads3D> {
    let denominator = 2 * i128::from(axles.rear_position - axles.front_position);
    let front = 2 * i128::from(axles.rear_position) * mass - twice_moment
        + i128::from(axles.front_tare) * denominator;
    let rear = twice_moment - 2 * i128::from(axles.front_position) * mass
        + i128::from(axles.rear_tare) * denominator;
    if front < 0
        || rear < 0
        || front > i128::from(axles.front_maximum) * denominator
        || rear > i128::from(axles.rear_maximum) * denominator
    {
        return None;
    }
    Some(AxleLoads3D {
        front: u64::try_from((front + denominator - 1) / denominator).ok()?,
        rear: u64::try_from((rear + denominator - 1) / denominator).ok()?,
    })
}

pub(super) fn axle_loads(
    problem: &LoadingProblem3D,
    source: usize,
    layout: Option<&LoadingBinLayout3D>,
    addition: Option<Addition>,
) -> Option<Option<AxleLoads3D>> {
    let Some(axles) = bin_rule(problem, source).axle_limits else {
        return Some(None);
    };
    let mut groups: BTreeMap<u32, (i128, i128)> = BTreeMap::new();
    let mut add_item = |demand, rect: FloorRect| {
        let (position, extent) = match axles.axis {
            FloorAxis3D::X => (rect.x, rect.width),
            FloorAxis3D::Z => (rect.z, rect.depth),
        };
        let mass = i128::from(weight(problem, demand));
        let group = groups.entry(item_rule(problem, demand).group_id).or_default();
        group.0 += mass;
        group.1 += mass * (2 * i128::from(position) + i128::from(extent));
    };
    if let Some(layout) = layout {
        for stack in &layout.stacks {
            for item in &stack.items {
                add_item(item.demand_index, stack_rect(stack));
            }
        }
    }
    if let Some(addition) = addition {
        add_item(addition.demand, addition.rect);
    }
    let mut mass = 0i128;
    let mut moment = 0i128;
    let mut full = reactions(axles, 0, 0)?;
    for (group_mass, group_moment) in groups.values().rev() {
        mass += group_mass;
        moment += group_moment;
        if problem.unloading != UnloadingConstraint3D::None {
            reactions(axles, mass, moment)?;
        }
    }
    if !groups.is_empty() {
        full = reactions(axles, mass, moment)?;
    }
    Some(Some(full))
}

// Correct signed floor/ceiling division for positive denominators.
fn floor_div(a: i128, b: i128) -> i128 {
    a.div_euclid(b)
}
fn ceil_div(a: i128, b: i128) -> i128 {
    -(-a).div_euclid(b)
}

pub(super) fn anchors(
    problem: &LoadingProblem3D,
    source: usize,
    layout: Option<&LoadingBinLayout3D>,
    demand: usize,
    free: FloorRect,
    width: u32,
    depth: u32,
) -> Vec<(u32, u32)> {
    let xmax = free.right() - width;
    let zmax = free.far() - depth;
    let mut xs = vec![free.x, xmax, free.x + (xmax - free.x) / 2];
    let mut zs = vec![free.z, zmax, free.z + (zmax - free.z) / 2];
    if let Some(axles) = bin_rule(problem, source).axle_limits {
        let added = i128::from(weight(problem, demand));
        if added > 0 {
            let mut mass = 0i128;
            let mut moment = 0i128;
            if let Some(layout) = layout {
                for stack in &layout.stacks {
                    let (position, extent) = match axles.axis {
                        FloorAxis3D::X => (stack.x, stack.width),
                        FloorAxis3D::Z => (stack.z, stack.depth),
                    };
                    mass += i128::from(stack.weight);
                    moment +=
                        i128::from(stack.weight) * (2 * i128::from(position) + i128::from(extent));
                }
            }
            let denominator = 2 * i128::from(axles.rear_position - axles.front_position);
            let old_front = 2 * i128::from(axles.rear_position) * mass - moment
                + i128::from(axles.front_tare) * denominator;
            let old_rear = moment - 2 * i128::from(axles.front_position) * mass
                + i128::from(axles.rear_tare) * denominator;
            let extent = i128::from(match axles.axis {
                FloorAxis3D::X => width,
                FloorAxis3D::Z => depth,
            });
            let front_constant = old_front + added * (2 * i128::from(axles.rear_position) - extent);
            let rear_constant = old_rear + added * (extent - 2 * i128::from(axles.front_position));
            let lower = ceil_div(-rear_constant, 2 * added).max(ceil_div(
                front_constant - i128::from(axles.front_maximum) * denominator,
                2 * added,
            ));
            let upper =
                floor_div(i128::from(axles.rear_maximum) * denominator - rear_constant, 2 * added)
                    .min(floor_div(front_constant, 2 * added));
            let (coordinates, start, end) = match axles.axis {
                FloorAxis3D::X => (&mut xs, free.x, xmax),
                FloorAxis3D::Z => (&mut zs, free.z, zmax),
            };
            let lower = lower.max(i128::from(start));
            let upper = upper.min(i128::from(end));
            if lower <= upper {
                if let Ok(value) = u32::try_from(lower) {
                    coordinates.push(value);
                }
                if let Ok(value) = u32::try_from(upper) {
                    coordinates.push(value);
                }
            }
        }
    }
    xs.sort_unstable();
    xs.dedup();
    zs.sort_unstable();
    zs.dedup();
    xs.into_iter().flat_map(|x| zs.iter().copied().map(move |z| (x, z))).collect()
}
