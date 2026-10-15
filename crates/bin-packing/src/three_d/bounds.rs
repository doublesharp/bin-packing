//! Integer conservative-scale bounds for fixed-orientation, single-stock packing.
//!
//! For an axis of length C and integer q >= 1, replace each item length x
//! by floor(x/q), and the bin length by floor(C/q). Any disjoint interval
//! chain still fits, since sum(floor(x/q)) <= floor(sum(x)/q). Compact each
//! axis along its original interval precedence graph using longest paths.
//! Each path fits the transformed axis, and every pair of boxes retains
//! at least one separating axis. The transformed boxes therefore still
//! pack. Their total transformed volume divided by transformed bin volume
//! is a valid lower bound. Zero transformed lengths contribute zero.
//!
//! We combine the identity scale with eight threshold scales per axis.
//! The threshold q = floor(C/(k+1)) + 1 detects k-slot restrictions, such
//! as 40-cubes in a 100-cube having at most 2 slots on each axis. This is a
//! bounded family, not the strongest possible conservative-scale bound.

use super::model::ThreeDProblem;

/// Maximum of at most 9^3 transformed-volume bounds. O(9^3 * demand types)
/// work, independent of demand quantity. Caller must have checked the exact
/// backend's single-stock, uncapped, XYZ-only orientation restrictions.
pub(super) fn geometric_lower_bound(problem: &ThreeDProblem) -> u64 {
    debug_assert_eq!(problem.bins.len(), 1);
    debug_assert!(problem.bins[0].quantity.is_none());
    debug_assert!(
        problem.demands.iter().all(|d| d.allowed_rotations == super::model::RotationMask3D::XYZ)
    );
    let bin = &problem.bins[0];
    let mut best = 0;
    for x_scale in axis_scales(bin.width) {
        for y_scale in axis_scales(bin.height) {
            for z_scale in axis_scales(bin.depth) {
                let capacity = u128::from(bin.width / x_scale)
                    * u128::from(bin.height / y_scale)
                    * u128::from(bin.depth / z_scale);
                let load: u128 = problem
                    .demands
                    .iter()
                    .map(|item| {
                        u128::from(item.width / x_scale)
                            * u128::from(item.height / y_scale)
                            * u128::from(item.depth / z_scale)
                            * item.quantity as u128
                    })
                    .sum();
                // Validated quantities sum to at most u32::MAX, so this
                // quotient cannot exceed u32::MAX. u128 also holds the
                // full quantity-weighted volume for maximum dimensions.
                best = best.max(load.div_ceil(capacity) as u64);
            }
        }
    }
    best
}

fn axis_scales(extent: u32) -> Vec<u32> {
    let mut scales = vec![1];
    for slots in 1..=8 {
        let scale = extent / (slots + 1) + 1;
        if !scales.contains(&scale) {
            scales.push(scale);
        }
    }
    scales
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::three_d::{Bin3D, BoxDemand3D, RotationMask3D};

    fn problem(shapes: &[(u32, u32, u32)]) -> ThreeDProblem {
        ThreeDProblem {
            bins: vec![Bin3D {
                name: "B".into(),
                width: 3,
                height: 3,
                depth: 2,
                cost: 1.0,
                quantity: None,
            }],
            demands: shapes
                .iter()
                .enumerate()
                .map(|(i, &(width, height, depth))| BoxDemand3D {
                    name: i.to_string(),
                    width,
                    height,
                    depth,
                    quantity: 1,
                    allowed_rotations: RotationMask3D::XYZ,
                })
                .collect(),
        }
    }

    /// Independent unit-cell occupancy oracle. Enumerates every integer
    /// placement and bin assignment. Integral box sizes admit integral
    /// compaction along each original separating-axis precedence graph.
    fn brute_force_count(shapes: &[(u32, u32, u32)]) -> usize {
        fn visit(shapes: &[(u32, u32, u32)], occupied: &mut [u32]) -> bool {
            let Some((&(w, h, d), rest)) = shapes.split_first() else {
                return true;
            };
            let snapshots: Vec<_> = occupied.iter().copied().enumerate().collect();
            for (index, old) in snapshots {
                for x in 0..=3 - w {
                    for y in 0..=3 - h {
                        for z in 0..=2 - d {
                            let mut mask = 0u32;
                            for dx in 0..w {
                                for dy in 0..h {
                                    for dz in 0..d {
                                        mask |= 1 << ((x + dx) + 3 * (y + dy) + 9 * (z + dz));
                                    }
                                }
                            }
                            if old & mask == 0 {
                                occupied[index] = old | mask;
                                if visit(rest, occupied) {
                                    return true;
                                }
                                occupied[index] = old;
                            }
                        }
                    }
                }
                if old == 0 {
                    break;
                }
            }
            false
        }
        (1..=shapes.len())
            .find(|&count| visit(shapes, &mut vec![0; count]))
            .expect("one bin per individually fitting item is feasible")
    }

    #[test]
    fn scales_obey_every_small_one_dimensional_partition() {
        fn check_partitions(capacity: u32, remaining: u32, loads: &mut Vec<u32>) {
            for scale in axis_scales(capacity) {
                assert!(loads.iter().map(|x| x / scale).sum::<u32>() <= capacity / scale);
            }
            for next in 1..=remaining {
                loads.push(next);
                check_partitions(capacity, remaining - next, loads);
                assert_eq!(loads.pop(), Some(next));
            }
        }
        for capacity in 1..=12 {
            check_partitions(capacity, capacity, &mut Vec::new());
        }
    }

    #[test]
    fn bounds_never_exceed_brute_force_optima_for_all_tiny_triples() {
        let shapes: Vec<_> = (1..=3)
            .flat_map(|w| (1..=3).flat_map(move |h| (1..=2).map(move |d| (w, h, d))))
            .collect();
        for (i, &a) in shapes.iter().enumerate() {
            for (j, &b) in shapes.iter().enumerate().skip(i) {
                for &c in shapes.iter().skip(j) {
                    let instance = [a, b, c];
                    let bound = geometric_lower_bound(&problem(&instance));
                    assert!(bound <= brute_force_count(&instance) as u64, "{instance:?}: {bound}");
                }
            }
        }
    }

    #[test]
    fn maximum_quantity_volume_uses_wide_accumulation() {
        let mut input = problem(&[(1, 1, 1)]);
        input.bins[0].width = 32768;
        input.bins[0].height = 32768;
        input.bins[0].depth = 32768;
        input.demands[0].width = 32768;
        input.demands[0].height = 32768;
        input.demands[0].depth = 32768;
        input.demands[0].quantity = u32::MAX as usize;
        assert_eq!(geometric_lower_bound(&input), u64::from(u32::MAX));
    }
}
