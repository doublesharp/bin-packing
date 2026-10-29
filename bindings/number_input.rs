//! Exact integer checks shared by the Node and WASM loading boundaries.

use bin_packing::three_d::constrained::ConstrainedProblem3D;
use bin_packing::three_d::loading::{LoadingProblem3D, LoadingSolution3D};

pub(crate) const MAX_SAFE_JS_INTEGER: u64 = (1_u64 << 53) - 1;

fn safe(values: impl IntoIterator<Item = u64>) -> Result<(), &'static str> {
    if values.into_iter().any(|value| value > MAX_SAFE_JS_INTEGER) {
        return Err("weights and loading limits must be nonnegative JavaScript safe integers");
    }
    Ok(())
}

pub(crate) fn weights(problem: &ConstrainedProblem3D) -> Result<(), &'static str> {
    safe(
        problem
            .item_weights
            .iter()
            .copied()
            .chain(problem.bin_max_weights.iter().flatten().copied()),
    )
}

pub(crate) fn loading(problem: &LoadingProblem3D) -> Result<(), &'static str> {
    weights(&problem.packing)?;
    safe(problem.item_rules.iter().filter_map(|rule| rule.maximum_weight_above))?;
    for rule in &problem.bin_rules {
        if let Some(density) = rule.maximum_stack_density {
            safe([density.numerator, density.denominator])?;
        }
        if let Some(axles) = rule.axle_limits {
            safe([axles.front_maximum, axles.rear_maximum, axles.front_tare, axles.rear_tare])?;
        }
    }
    Ok(())
}

pub(crate) fn payloads(values: &[u64]) -> Result<(), &'static str> {
    safe(values.iter().copied())
}

pub(crate) fn indexed_payloads(
    weights: &[u64],
    indices: &[Vec<usize>],
) -> Result<(), &'static str> {
    for layout in indices {
        let mut payload = 0u64;
        for &index in layout {
            let weight = if weights.is_empty() {
                0
            } else {
                *weights.get(index).ok_or("invalid indexed loading source")?
            };
            payload = payload
                .checked_add(weight)
                .ok_or("bin payload exceeds JavaScript safe integers")?;
        }
        safe([payload])?;
    }
    Ok(())
}

pub(crate) fn loaded(solution: &LoadingSolution3D) -> Result<(), &'static str> {
    for layout in &solution.layouts {
        safe([layout.payload])?;
        safe(layout.stacks.iter().map(|stack| stack.weight))?;
        if let Some(axles) = layout.axle_loads {
            safe([axles.front, axles.rear])?;
        }
    }
    Ok(())
}
