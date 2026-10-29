use std::collections::BTreeMap;

use super::geometry::{self, bin_rule, invalid, item_rule, weight};
use super::{
    AxleLimits3D, AxleLoads3D, FloorAxis3D, LoadingBinLayout3D, LoadingProblem3D,
    LoadingSolution3D, Rotation3D, StackColumn3D, UnloadingConstraint3D,
};
use crate::Result;

pub(super) fn validate(problem: &LoadingProblem3D, result: &LoadingSolution3D) -> Result<()> {
    geometry::validate_problem(problem)?;
    let source = &problem.packing.problem;
    let quantity: usize = source.demands.iter().map(|demand| demand.quantity).sum();
    if result.layouts.len() != result.bin_count
        || result.layouts.len() > quantity
        || result.explored_states > geometry::MAX_STATES
    {
        return Err(invalid("invalid bin count or search accounting"));
    }
    let mut usage = vec![0usize; source.bins.len()];
    let mut placed = vec![0usize; source.demands.len()];
    let mut cost = 0.0;
    for layout in &result.layouts {
        let stock =
            source.bins.get(layout.bin_index).ok_or_else(|| invalid("unknown source bin"))?;
        usage[layout.bin_index] += 1;
        if stock.quantity.is_some_and(|limit| usage[layout.bin_index] > limit) {
            return Err(invalid("source bin inventory exceeded"));
        }
        cost += stock.cost;
        validate_layout(problem, layout, &mut placed)?;
    }
    if !result.total_cost.is_finite() || result.total_cost != cost {
        return Err(invalid("material cost differs from consumed source bins"));
    }
    let mut previous = None;
    for missing in &result.unplaced {
        if missing.quantity == 0 || previous.is_some_and(|index| index >= missing.demand_index) {
            return Err(invalid("unplaced records must be positive, unique and source-ordered"));
        }
        let count = placed
            .get_mut(missing.demand_index)
            .ok_or_else(|| invalid("unknown unplaced demand"))?;
        *count = count
            .checked_add(missing.quantity)
            .ok_or_else(|| invalid("demand accounting overflow"))?;
        previous = Some(missing.demand_index);
    }
    if placed.iter().zip(&source.demands).any(|(&count, demand)| count != demand.quantity) {
        return Err(invalid("placed and unplaced quantities do not match source demands"));
    }
    Ok(())
}

fn validate_layout(
    problem: &LoadingProblem3D,
    layout: &LoadingBinLayout3D,
    placed: &mut [usize],
) -> Result<()> {
    let bin = &problem.packing.problem.bins[layout.bin_index];
    let rules = bin_rule(problem, layout.bin_index);
    if layout.stacks.is_empty() || layout.stacks.len() > geometry::MAX_ITEMS {
        return Err(invalid("loaded bins must contain a bounded nonempty set of stacks"));
    }
    let mut payload = 0u64;
    for (index, stack) in layout.stacks.iter().enumerate() {
        if stack.width == 0
            || stack.depth == 0
            || stack.height == 0
            || u64::from(stack.x) + u64::from(stack.width) > u64::from(bin.width)
            || u64::from(stack.z) + u64::from(stack.depth) > u64::from(bin.depth)
            || stack.height > bin.height
        {
            return Err(invalid("stack extends outside source bin"));
        }
        let rect = geometry::stack_rect(stack);
        if rules.defects.iter().any(|defect| {
            geometry::overlaps(
                rect,
                geometry::FloorRect {
                    x: defect.x,
                    z: defect.z,
                    width: defect.width,
                    depth: defect.depth,
                },
            )
        }) || layout.stacks[..index]
            .iter()
            .any(|other| geometry::overlaps(rect, geometry::stack_rect(other)))
        {
            return Err(invalid("columns overlap a floor defect or one another"));
        }
        let mass = validate_stack(problem, stack, placed)?;
        if rules.maximum_stack_density.is_some_and(|density| {
            u128::from(mass) * u128::from(density.denominator)
                > u128::from(density.numerator) * u128::from(stack.width) * u128::from(stack.depth)
        }) {
            return Err(invalid("stack exceeds maximum footprint density"));
        }
        payload = payload.checked_add(mass).ok_or_else(|| invalid("bin payload overflows u64"))?;
        for other in &layout.stacks[..index] {
            for a in &stack.items {
                for b in &other.items {
                    if !delivery_pair(
                        problem.unloading,
                        stack,
                        item_rule(problem, a.demand_index).group_id,
                        other,
                        item_rule(problem, b.demand_index).group_id,
                    ) {
                        return Err(invalid("later delivery group blocks earlier group"));
                    }
                }
            }
        }
    }
    if payload != layout.payload
        || geometry::payload_limit(problem, layout.bin_index).is_some_and(|limit| payload > limit)
    {
        return Err(invalid("bin payload differs from source weights or exceeds capacity"));
    }
    let expected = match rules.axle_limits {
        Some(axles) => Some(validate_axles(problem, layout, axles)?),
        None => None,
    };
    if layout.axle_loads != expected {
        return Err(invalid("reported axle reactions differ from exact load"));
    }
    Ok(())
}

fn validate_stack(
    problem: &LoadingProblem3D,
    stack: &StackColumn3D,
    placed: &mut [usize],
) -> Result<u64> {
    if stack.items.is_empty() || stack.items.len() > geometry::MAX_ITEMS {
        return Err(invalid("stacks must contain a bounded nonempty set of items"));
    }
    let mut top = 0u32;
    let mut compatible = None;
    let mut lower_group = None;
    for (index, item) in stack.items.iter().enumerate() {
        let demand = problem
            .packing
            .problem
            .demands
            .get(item.demand_index)
            .ok_or_else(|| invalid("unknown source demand"))?;
        let rule = item_rule(problem, item.demand_index);
        if !matches!(item.rotation, Rotation3D::Xyz | Rotation3D::Zyx)
            || !demand.allowed_rotations.contains(item.rotation)
        {
            return Err(invalid("item orientation is not permitted upright rotation"));
        }
        let (width, height, depth) = item.rotation.apply(demand.width, demand.height, demand.depth);
        if (width, depth) != (stack.width, stack.depth)
            || compatible.is_some_and(|identifier| identifier != rule.stackability_id)
        {
            return Err(invalid("stack items differ in footprint or stackability identifier"));
        }
        compatible = Some(rule.stackability_id);
        if rule.maximum_stackability.is_some_and(|limit| stack.items.len() > limit) {
            return Err(invalid("stack count exceeds a contained item's maximum stackability"));
        }
        let expected_y = if index == 0 {
            0
        } else {
            top.checked_sub(rule.nesting_height)
                .ok_or_else(|| invalid("nested item extends below floor"))?
        };
        if item.y != expected_y {
            return Err(invalid("item bottom differs from declared nesting rule"));
        }
        top = expected_y.checked_add(height).ok_or_else(|| invalid("stack height overflow"))?;
        if problem.unloading != UnloadingConstraint3D::None
            && lower_group.is_some_and(|group| rule.group_id > group)
        {
            return Err(invalid("later delivery group sits above earlier delivery group"));
        }
        lower_group = Some(rule.group_id);
        let count = &mut placed[item.demand_index];
        *count += 1;
        if *count > demand.quantity {
            return Err(invalid("placed demand exceeds source quantity"));
        }
    }
    if top != stack.height {
        return Err(invalid("reported stack height omits or changes a physical item"));
    }
    let mut above = 0u64;
    for item in stack.items.iter().rev() {
        if item_rule(problem, item.demand_index)
            .maximum_weight_above
            .is_some_and(|limit| above > limit)
        {
            return Err(invalid("weight above a contained item exceeds its limit"));
        }
        above = above
            .checked_add(weight(problem, item.demand_index))
            .ok_or_else(|| invalid("stack weight overflows u64"))?;
    }
    if above != stack.weight {
        return Err(invalid("stack weight differs from source weights"));
    }
    Ok(above)
}

// Check unloading independently of candidate generation. Increasing-coordinate
// modes compare origins, while straight movements also require a shared lane.
fn delivery_pair(
    mode: UnloadingConstraint3D,
    a: &StackColumn3D,
    group_a: u32,
    b: &StackColumn3D,
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
            let separated_lanes = u64::from(a.z) + u64::from(a.depth) <= u64::from(b.z)
                || u64::from(b.z) + u64::from(b.depth) <= u64::from(a.z);
            separated_lanes || u64::from(early.x) >= u64::from(late.x) + u64::from(late.width)
        }
        UnloadingConstraint3D::OnlyZMovements => {
            let separated_lanes = u64::from(a.x) + u64::from(a.width) <= u64::from(b.x)
                || u64::from(b.x) + u64::from(b.width) <= u64::from(a.x);
            separated_lanes || u64::from(early.z) >= u64::from(late.z) + u64::from(late.depth)
        }
    }
}

// The validator reconstructs moments from source item weights. It does not use
// reported stack weights, reactions, anchors, or construction feasibility.
fn validate_axles(
    problem: &LoadingProblem3D,
    layout: &LoadingBinLayout3D,
    axles: AxleLimits3D,
) -> Result<AxleLoads3D> {
    let mut groups: BTreeMap<u32, (u128, u128)> = BTreeMap::new();
    for stack in &layout.stacks {
        let center_twice = match axles.axis {
            FloorAxis3D::X => 2 * u128::from(stack.x) + u128::from(stack.width),
            FloorAxis3D::Z => 2 * u128::from(stack.z) + u128::from(stack.depth),
        };
        for item in &stack.items {
            let group = groups.entry(item_rule(problem, item.demand_index).group_id).or_default();
            let mass = u128::from(weight(problem, item.demand_index));
            group.0 += mass;
            group.1 += mass * center_twice;
        }
    }
    let mut mass = 0u128;
    let mut moment = 0u128;
    for &(group_mass, group_moment) in groups.values().rev() {
        mass += group_mass;
        moment += group_moment;
        if problem.unloading != UnloadingConstraint3D::None {
            validate_reactions(axles, mass, moment)?;
        }
    }
    validate_reactions(axles, mass, moment)
}

fn validate_reactions(axles: AxleLimits3D, mass: u128, moment: u128) -> Result<AxleLoads3D> {
    // The input dimension/quantity limits make all these values fit i128.
    let mass = i128::try_from(mass).map_err(|_| invalid("axle mass overflow"))?;
    let moment = i128::try_from(moment).map_err(|_| invalid("axle moment overflow"))?;
    let span = 2 * i128::from(axles.rear_position - axles.front_position);
    let rear =
        moment - 2 * i128::from(axles.front_position) * mass + i128::from(axles.rear_tare) * span;
    let front =
        2 * i128::from(axles.rear_position) * mass - moment + i128::from(axles.front_tare) * span;
    if front < 0
        || rear < 0
        || front > i128::from(axles.front_maximum) * span
        || rear > i128::from(axles.rear_maximum) * span
    {
        return Err(invalid("axle reaction is negative or exceeds its allowed weight"));
    }
    Ok(AxleLoads3D {
        front: u64::try_from((front + span - 1) / span)
            .map_err(|_| invalid("front reaction overflows u64"))?,
        rear: u64::try_from((rear + span - 1) / span)
            .map_err(|_| invalid("rear reaction overflows u64"))?,
    })
}
