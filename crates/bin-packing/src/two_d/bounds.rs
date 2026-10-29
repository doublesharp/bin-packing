//! Conservative lower bounds on the sheets needed to place every demanded part.
//!
//! The single-stock geometric bounds use original integer implementations of
//! conservative scales. PackingSolver's description of dual feasible functions
//! is a useful reference: <https://fontanf.github.io/packingsolver/internals/rectangle/dual_feasible_functions.html>.

use super::{ItemInstance2D, TwoDProblem, model::effective_bounds};
use crate::Result;

/// Return a lower bound on the number of sheets needed to place every part.
///
/// Uses quantity-weighted area divided by the largest effective sheet area.
/// The bound relaxes inventory caps and blade gaps; it does not certify that
/// the requested stock is sufficient. Invalid input or individually oversized
/// demands return the same errors as [`super::solve_2d`]. Quantities remain
/// compressed and aggregate area uses `u128`.
///
/// For one stock type without edge relief, also evaluates at most 17 conservative
/// scales on each axis. Every demand contributes the minimum transformed area
/// among its allowed, individually fitting orientations. This remains valid
/// when different copies use different orientations. Work is O(289 * D), where
/// D is the number of demand types, and is independent of expanded quantity.
/// The bounded scale family can miss stronger geometric lower bounds.
///
/// # Errors
/// Returns public problem validation or individual-fit errors.
pub fn lower_bound_2d(problem: &TwoDProblem) -> Result<usize> {
    problem.validate()?;
    problem.ensure_feasible_demands()?;
    let total_area: u128 = problem
        .demands
        .iter()
        .map(|demand| {
            u128::from(demand.width) * u128::from(demand.height) * demand.quantity as u128
        })
        .sum();
    let capacity = problem.sheets.iter().fold(1u128, |largest, sheet| {
        let (width, height) = effective_bounds(sheet);
        largest.max(u128::from(width) * u128::from(height))
    });
    // Validation bounds total quantity by u32::MAX. An individually fitting
    // item occupies at most the largest sheet, so this quotient fits usize on
    // both native and wasm32 targets. u128 covers the full weighted area.
    let area_bound = total_area.div_ceil(capacity) as usize;
    if problem.sheets.len() != 1 || problem.sheets[0].edge_kerf_relief {
        return Ok(area_bound);
    }
    Ok(area_bound.max(geometric_bound(problem)))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AxisScale {
    Identity,
    Floor(u32),
    Threshold(u32),
}

impl AxisScale {
    fn apply(self, length: u32, capacity: u32) -> u32 {
        match self {
            Self::Identity => length,
            Self::Floor(divisor) => length / divisor,
            Self::Threshold(threshold) => {
                if length < threshold {
                    0
                } else if length > capacity - threshold {
                    capacity
                } else {
                    length
                }
            }
        }
    }
}

// At most 17 scales regardless of dimensions or quantity. Floor scales satisfy
// sum(floor(x/q)) <= floor(C/q) whenever sum(x) <= C. Threshold scale f0 is
// identity except x<k becomes 0 and x>C-k becomes C, for 1<=k<=C/2. If a long
// interval exists, all other intervals total less than k and map to 0;
// otherwise no interval is enlarged. Thus both preserve all feasible chains.
fn axis_scales(capacity: u32) -> Vec<AxisScale> {
    let mut scales = vec![AxisScale::Identity];
    for slots in 1..=8 {
        let divisor = capacity / (slots + 1) + 1;
        let floor = AxisScale::Floor(divisor);
        if divisor > 1 && !scales.contains(&floor) {
            scales.push(floor);
        }
        let threshold = capacity / (slots + 1);
        let scale = AxisScale::Threshold(threshold);
        if threshold > 0 && !scales.contains(&scale) {
            scales.push(scale);
        }
    }
    scales
}

struct DemandOrientations {
    dimensions: [(u32, u32); 2],
    count: usize,
    quantity: usize,
}

// O(289 * D) time and O(D) memory, with no per-copy expansion. Applying a
// chain-preserving scale on each axis preserves a packing: compact each axis
// along the original separating-interval precedence graph, whose paths still
// fit in the transformed capacity. Every pair retains a separating axis.
// Therefore transformed total area / transformed capacity is a lower bound.
fn geometric_bound(problem: &TwoDProblem) -> usize {
    let sheet = &problem.sheets[0];
    let demands: Vec<_> = problem
        .demands
        .iter()
        .map(|demand| {
            let item = ItemInstance2D {
                name: String::new(),
                width: demand.width,
                height: demand.height,
                can_rotate: demand.can_rotate,
                fixed_rotation: None,
            };
            let mut orientations =
                DemandOrientations { dimensions: [(0, 0); 2], count: 0, quantity: demand.quantity };
            for (width, height, _) in item.orientations() {
                if width <= sheet.width && height <= sheet.height {
                    orientations.dimensions[orientations.count] = (width, height);
                    orientations.count += 1;
                }
            }
            debug_assert!(orientations.count > 0);
            orientations
        })
        .collect();
    let x_scales = axis_scales(sheet.width);
    let y_scales = axis_scales(sheet.height);
    let mut best = 0;
    for &x_scale in &x_scales {
        for &y_scale in &y_scales {
            let capacity = u128::from(x_scale.apply(sheet.width, sheet.width))
                * u128::from(y_scale.apply(sheet.height, sheet.height));
            let load: u128 = demands
                .iter()
                .map(|demand| {
                    demand.dimensions[..demand.count]
                        .iter()
                        .map(|&(width, height)| {
                            u128::from(x_scale.apply(width, sheet.width))
                                * u128::from(y_scale.apply(height, sheet.height))
                        })
                        .min()
                        .unwrap_or(0)
                        * demand.quantity as u128
                })
                .sum();
            // Each transformed item fits, and total quantity is validated.
            best = best.max(load.div_ceil(capacity) as usize);
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::two_d::{RectDemand2D, Sheet2D};

    fn problem(width: u32, height: u32, shapes: &[(u32, u32, bool)]) -> TwoDProblem {
        TwoDProblem {
            sheets: vec![Sheet2D {
                name: "sheet".into(),
                width,
                height,
                cost: 1.0,
                quantity: None,
                kerf: 0,
                edge_kerf_relief: false,
            }],
            demands: shapes
                .iter()
                .enumerate()
                .map(|(index, &(width, height, can_rotate))| RectDemand2D {
                    name: index.to_string(),
                    width,
                    height,
                    quantity: 1,
                    can_rotate,
                })
                .collect(),
        }
    }

    // Independent unit-cell oracle. Each recursive step enumerates every
    // integer position, allowed rotation and bin assignment for the next item.
    // Integer sizes admit integer compaction of a feasible rectangular packing.
    fn brute_force_count(width: u32, height: u32, shapes: &[(u32, u32, bool)]) -> usize {
        fn visit(
            width: u32,
            height: u32,
            shapes: &[(u32, u32, bool)],
            occupied: &mut [u32],
        ) -> bool {
            let Some((&(item_width, item_height, rotate), rest)) = shapes.split_first() else {
                return true;
            };
            let orientations = [(item_width, item_height), (item_height, item_width)];
            let snapshots: Vec<_> = occupied.iter().copied().enumerate().collect();
            for (index, old) in snapshots {
                for &(w, h) in &orientations[..if rotate { 2 } else { 1 }] {
                    if w > width || h > height {
                        continue;
                    }
                    for x in 0..=width - w {
                        for y in 0..=height - h {
                            let mut mask = 0u32;
                            for dx in 0..w {
                                for dy in 0..h {
                                    let cell =
                                        u64::from(x + dx) + u64::from(width) * u64::from(y + dy);
                                    mask |= 1 << cell;
                                }
                            }
                            if old & mask == 0 {
                                occupied[index] = old | mask;
                                if visit(width, height, rest, occupied) {
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
            .find(|&count| visit(width, height, shapes, &mut vec![0; count]))
            .expect("one bin per individually fitting item is feasible")
    }

    #[test]
    fn geometric_bound_improves_naive_area_bound() {
        let shapes = [(6, 4, false), (4, 6, false), (4, 3, false)];
        let mut input = problem(10, 6, &shapes);
        for demand in &mut input.demands {
            demand.quantity = 2;
        }
        assert_eq!(lower_bound_2d(&input).expect("valid input"), 3);
    }

    #[test]
    fn scales_preserve_all_tiny_feasible_interval_chains() {
        fn check(capacity: u32, remaining: u32, chain: &mut Vec<u32>) {
            for scale in axis_scales(capacity) {
                let load: u32 = chain.iter().map(|&length| scale.apply(length, capacity)).sum();
                assert!(load <= scale.apply(capacity, capacity), "{capacity}: {scale:?} {chain:?}");
            }
            for length in 1..=remaining {
                chain.push(length);
                check(capacity, remaining - length, chain);
                assert_eq!(chain.pop(), Some(length));
            }
        }

        for capacity in 1..=12 {
            assert!(axis_scales(capacity).len() <= 17);
            check(capacity, capacity, &mut Vec::new());
        }
    }

    #[test]
    fn all_tiny_triples_stay_below_independent_optimum() {
        for (width, height) in [(3, 3), (4, 2), (2, 3)] {
            let shapes: Vec<_> = (1..=width.max(height))
                .flat_map(|w| {
                    (1..=width.max(height)).flat_map(move |h| {
                        [false, true]
                            .into_iter()
                            .filter(move |&rotate| {
                                (w <= width && h <= height) || (rotate && h <= width && w <= height)
                            })
                            .map(move |rotate| (w, h, rotate))
                    })
                })
                .collect();
            for (i, &a) in shapes.iter().enumerate() {
                for (j, &b) in shapes.iter().enumerate().skip(i) {
                    for &c in shapes.iter().skip(j) {
                        let parts = [a, b, c];
                        let bound = lower_bound_2d(&problem(width, height, &parts))
                            .expect("individually fitting parts");
                        assert!(
                            bound <= brute_force_count(width, height, &parts),
                            "{width}x{height} {parts:?}: {bound}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn rotated_fit_is_used_even_when_declared_orientation_does_not_fit() {
        let input = problem(2, 5, &[(5, 2, true)]);
        assert_eq!(lower_bound_2d(&input).expect("rotated fit"), 1);
    }

    #[test]
    fn mixed_stock_and_exhausted_inventory_remain_relaxations() {
        let mut input = problem(3, 3, &[(2, 2, false), (2, 2, false)]);
        input.sheets[0].quantity = Some(0);
        assert_eq!(lower_bound_2d(&input).expect("inventory is relaxed"), 2);
        let mut larger = input.sheets[0].clone();
        larger.name = "larger".into();
        larger.width = 4;
        larger.height = 4;
        input.sheets.push(larger);
        assert_eq!(lower_bound_2d(&input).expect("mixed stock area bound"), 1);
    }

    #[test]
    fn aggregate_area_stays_wide_at_maximum_dimensions_and_quantity() {
        let mut input = problem(1 << 30, 1 << 30, &[(1 << 30, 1 << 30, false)]);
        input.demands[0].quantity = u32::MAX as usize;
        assert_eq!(lower_bound_2d(&input).expect("wide aggregate"), u32::MAX as usize);
    }

    #[test]
    fn invalid_and_individually_infeasible_inputs_fail_at_boundary() {
        let mut input = problem(2, 2, &[(1, 1, false)]);
        input.demands[0].quantity = 0;
        assert!(matches!(lower_bound_2d(&input), Err(crate::BinPackingError::InvalidInput(_))));
        input.demands.clear();
        assert!(matches!(lower_bound_2d(&input), Err(crate::BinPackingError::InvalidInput(_))));
        input.demands.push(RectDemand2D {
            name: "too large".into(),
            width: 3,
            height: 3,
            quantity: 1,
            can_rotate: false,
        });
        assert!(matches!(lower_bound_2d(&input), Err(crate::BinPackingError::Infeasible2D { .. })));
    }

    #[test]
    fn edge_relief_uses_effective_area_instead_of_unsupported_physical_area() {
        let mut input = problem(10, 10, &[(4, 4, false)]);
        input.sheets[0].kerf = 2;
        input.sheets[0].edge_kerf_relief = true;
        input.demands[0].quantity = 9;
        assert_eq!(lower_bound_2d(&input).expect("relaxed effective bounds"), 1);
    }
}
