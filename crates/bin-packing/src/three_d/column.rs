//! Column / stack-building solver.
//!
//! Builds vertical stacks of items whose horizontal footprint
//! `(x_extent, z_extent)` matches exactly (under some allowed rotation),
//! then 2D-packs the stack footprints onto the bin floor via
//! [`crate::two_d::place_into_sheet`]. Every item in a stack shares the
//! same `(x, z)` coordinates and the stack grows monotonically along the
//! y-axis up to `bin.height`.
//!
//! # Known limitation
//!
//! The "match exactly under some allowed rotation" constraint means the
//! solver is only useful for catalogues with a small set of canonical
//! footprints (uniform pallet stacks, identical-base cartons). On
//! heterogeneous instances every item becomes its own single-item stack,
//! at which point the algorithm degrades to a plain 2D packing of the
//! bin floor with no z-axis benefit. For that reason the `Auto` runner
//! does **not** include column building in its tier 1 sweep — callers
//! must opt in explicitly via
//! [`ThreeDAlgorithm::ColumnBuilding`](super::model::ThreeDAlgorithm::ColumnBuilding).

use super::common::{
    BinPlacements, build_solution, check_bin_count_cap, surface_area_u64, volume_u64,
};
use super::model::{
    Bin3D, ItemInstance3D, Placement3D, Rotation3D, SolverMetrics3D, ThreeDOptions, ThreeDProblem,
    ThreeDSolution,
};
use crate::Result;
use crate::two_d::{self, ItemInstance2D, TwoDAlgorithm, TwoDOptions};

/// A vertical stack of items with a shared footprint.
#[derive(Debug, Clone)]
struct Stack {
    /// Shared footprint x-extent.
    footprint_x: u32,
    /// Shared footprint z-extent.
    footprint_z: u32,
    /// Running sum of the `y_extent` of the items currently stacked. Kept
    /// in sync with `items` so the stacking loop can test
    /// `accumulated_height + item_y_extent <= bin.height` in O(1).
    accumulated_height: u32,
    /// `(item, chosen rotation, y_extent under that rotation)` triples in
    /// stack order (bottom first).
    items: Vec<StackEntry>,
}

#[derive(Debug, Clone)]
struct StackEntry {
    item: ItemInstance3D,
    rotation: Rotation3D,
    y_extent: u32,
    x_extent: u32,
    z_extent: u32,
}

/// Footprint + flat orientation chosen for one item at the start of the
/// algorithm. Items are ranked by footprint area descending before
/// stacking.
#[derive(Debug, Clone)]
struct FlatItem {
    item: ItemInstance3D,
    flat_rotation: Rotation3D,
    flat_x_extent: u32,
    flat_y_extent: u32,
    flat_z_extent: u32,
    flat_area: u64,
}

/// Solve a 3D problem using column / stack building.
///
/// # Errors
///
/// Propagates [`crate::BinPackingError::Unsupported`] when the multi-bin
/// loop would exceed
/// [`MAX_BIN_COUNT_3D`](super::model::MAX_BIN_COUNT_3D), or when the
/// inner 2D packing call surfaces an error.
pub(super) fn solve_column_building(
    problem: &ThreeDProblem,
    _options: &ThreeDOptions,
) -> Result<ThreeDSolution> {
    // 1. Orient each item flat (minimise y-extent, tiebreak by rotation
    //    declaration order) and compute its floor footprint area.
    let mut flat_items: Vec<FlatItem> = problem
        .expanded_items()
        .into_iter()
        .map(|item| pick_flat_orientation(item, &problem.bins))
        .collect();

    // 2. Sort by footprint area descending (stable) so stacks with large
    //    bases open first.
    flat_items.sort_by(|left, right| right.flat_area.cmp(&left.flat_area));

    // 3. Build the stacks. We don't know which bin each stack will land in
    //    yet, so the per-bin height cap uses the *maximum* available bin
    //    height: a stack that fits the tallest bin type may still have to
    //    be rejected later if every tall bin is exhausted, at which point
    //    its items spill to `unplaced`.
    let max_bin_height = problem.bins.iter().map(|bin| bin.height).max().unwrap_or(0);
    let mut stacks: Vec<Stack> = Vec::new();
    let mut leftover_items: Vec<ItemInstance3D> = Vec::new();
    for flat in flat_items {
        if let Some(target) = pick_stack_for(&stacks, &flat, max_bin_height) {
            push_onto_stack(&mut stacks[target.index], &flat, target.rotation);
        } else if flat.flat_y_extent <= max_bin_height {
            stacks.push(Stack {
                footprint_x: flat.flat_x_extent,
                footprint_z: flat.flat_z_extent,
                accumulated_height: flat.flat_y_extent,
                items: vec![StackEntry {
                    item: flat.item,
                    rotation: flat.flat_rotation,
                    y_extent: flat.flat_y_extent,
                    x_extent: flat.flat_x_extent,
                    z_extent: flat.flat_z_extent,
                }],
            });
        } else {
            // No declared bin is tall enough to receive this item in its
            // flat orientation. Send it straight to `unplaced`.
            leftover_items.push(flat.item);
        }
    }

    // 4. Multi-bin 2D packing of stack footprints.
    let mut bin_placements: Vec<BinPlacements> = Vec::new();
    let mut bin_quantity_used: Vec<usize> = vec![0; problem.bins.len()];
    let mut pending: Vec<Stack> = stacks;

    while !pending.is_empty() {
        check_bin_count_cap(bin_placements.len())?;
        let bin_index = match select_next_bin(problem, &bin_quantity_used, &pending) {
            Some(index) => index,
            None => break,
        };
        let bin = &problem.bins[bin_index];
        bin_quantity_used[bin_index] = bin_quantity_used[bin_index].saturating_add(1);

        // Filter to stacks that physically fit this bin (footprint AND
        // height). Stacks that don't fit this bin type stay in `pending`
        // for the next attempt.
        let mut stage_indices: Vec<usize> = Vec::new();
        let mut carry_indices: Vec<usize> = Vec::new();
        for (index, stack) in pending.iter().enumerate() {
            if stack_fits_bin(stack, bin) {
                stage_indices.push(index);
            } else {
                carry_indices.push(index);
            }
        }

        if stage_indices.is_empty() {
            // Nothing fits this bin type even on an empty floor. Give up
            // on the remaining stacks — `select_next_bin` already vetted
            // that at least one stack could fit, so this branch is a
            // defensive fallback.
            debug_assert!(false, "select_next_bin returned a bin that fits no remaining stack");
            break;
        }

        // Build the 2D placement request. The synthetic name lets us
        // recover the stack by index after `place_into_sheet` returns.
        let items_2d: Vec<ItemInstance2D> = stage_indices
            .iter()
            .map(|&index| {
                let stack = &pending[index];
                ItemInstance2D {
                    name: format!("__stack_{index}__"),
                    width: stack.footprint_x,
                    height: stack.footprint_z,
                    can_rotate: false,
                }
            })
            .collect();

        let (placements_2d, unplaced_2d) = two_d::place_into_sheet(
            &items_2d,
            bin.width,
            bin.depth,
            TwoDAlgorithm::Auto,
            &TwoDOptions::default(),
        )?;

        // Convert 2D placements into vertical columns of `Placement3D`.
        let mut layout: Vec<Placement3D> = Vec::new();
        for p2d in placements_2d {
            let Some(stack_index) = parse_stack_index(&p2d.name) else {
                debug_assert!(false, "place_into_sheet returned unexpected name {}", p2d.name);
                continue;
            };
            let stack = &pending[stack_index];
            let mut cursor_y: u32 = 0;
            for entry in &stack.items {
                layout.push(Placement3D {
                    name: entry.item.name.clone(),
                    x: p2d.x,
                    y: cursor_y,
                    z: p2d.y,
                    width: entry.x_extent,
                    height: entry.y_extent,
                    depth: entry.z_extent,
                    rotation: entry.rotation,
                });
                cursor_y = cursor_y.saturating_add(entry.y_extent);
            }
        }

        // Anything `place_into_sheet` couldn't fit on this bin floor goes
        // back into the next-bin queue. We identify them by parsing the
        // leftover `ItemInstance2D.name` fields.
        let mut carried_back: Vec<usize> = Vec::new();
        for item_2d in unplaced_2d {
            if let Some(stack_index) = parse_stack_index(&item_2d.name) {
                carried_back.push(stack_index);
            } else {
                debug_assert!(false, "place_into_sheet returned unexpected leftover name");
            }
        }

        // Rebuild `pending` from: (a) stacks that never entered this bin
        // attempt, and (b) staged stacks that didn't fit on the sheet.
        // `placed_indices` are dropped (consumed by `layout`).
        let mut next_pending: Vec<Stack> =
            Vec::with_capacity(carry_indices.len() + carried_back.len());
        for index in &carry_indices {
            next_pending.push(pending[*index].clone());
        }
        for index in &carried_back {
            next_pending.push(pending[*index].clone());
        }

        if layout.is_empty() {
            // `place_into_sheet` refused every staged stack even though
            // `stack_fits_bin` said they should fit. Bail out defensively
            // — the remaining stacks will become `unplaced` below.
            debug_assert!(false, "place_into_sheet placed nothing despite pre-filtering");
            pending = next_pending;
            break;
        }

        bin_placements.push((bin_index, layout));
        pending = next_pending;
    }

    // 5. Any stack still pending becomes unplaced items.
    for stack in pending {
        for entry in stack.items {
            leftover_items.push(entry.item);
        }
    }

    let bin_count = bin_placements.len();
    let metrics = SolverMetrics3D {
        iterations: 1,
        explored_states: 0,
        extreme_points_generated: 0,
        branch_and_bound_nodes: 0,
        notes: vec![format!(
            "column_building: {bin_count} bin(s), {} unplaced item(s)",
            leftover_items.len()
        )],
    };

    build_solution("column_building", &problem.bins, bin_placements, leftover_items, metrics, false)
}

/// Pick the rotation that minimises the y-extent (i.e. orients the item
/// flat). Tiebreak by rotation declaration order. `item.allowed_rotations`
/// is non-empty (enforced by `ThreeDProblem::validate`), so
/// `orientations()` always yields at least one entry; the
/// [`debug_assert`] below is a defensive guard that triggers in tests if
/// the invariant ever breaks.
fn pick_flat_orientation(item: ItemInstance3D, bins: &[Bin3D]) -> FlatItem {
    // Seed with a sentinel chosen from the declared extents. The first
    // iteration of the loop unconditionally replaces it, so these values
    // are only ever observed if `orientations()` is unexpectedly empty.
    let mut chosen: Option<(Rotation3D, u32, u32, u32)> = None;
    let mut fallback: Option<(Rotation3D, u32, u32, u32)> = None;
    for (rotation, x_extent, y_extent, z_extent) in item.orientations() {
        let replace_fallback = match fallback {
            None => true,
            Some((_, _, current_y, _)) => y_extent < current_y,
        };
        if replace_fallback {
            fallback = Some((rotation, x_extent, y_extent, z_extent));
        }
        if !bins
            .iter()
            .any(|bin| x_extent <= bin.width && y_extent <= bin.height && z_extent <= bin.depth)
        {
            continue;
        }
        let replace = match chosen {
            None => true,
            Some((_, _, current_y, _)) => y_extent < current_y,
        };
        if replace {
            chosen = Some((rotation, x_extent, y_extent, z_extent));
        }
    }

    debug_assert!(chosen.is_some(), "validated item `{}` has no allowed rotation", item.name);
    let (flat_rotation, flat_x_extent, flat_y_extent, flat_z_extent) =
        chosen.or(fallback).unwrap_or((Rotation3D::Xyz, item.width, item.height, item.depth));
    let flat_area = surface_area_u64(flat_x_extent, flat_z_extent);
    FlatItem { item, flat_rotation, flat_x_extent, flat_y_extent, flat_z_extent, flat_area }
}

/// Selected stack + the rotation under which `flat.item`'s extents match
/// the stack's footprint.
struct StackMatch {
    index: usize,
    rotation: ChosenRotation,
}

/// `(rotation, x_extent, y_extent, z_extent)` for an item that fits an
/// existing stack.
#[derive(Debug, Clone, Copy)]
struct ChosenRotation {
    rotation: Rotation3D,
    x_extent: u32,
    y_extent: u32,
    z_extent: u32,
}

/// Find the best existing stack for `flat`. A stack accepts the item iff
///
/// 1. *Some* allowed rotation of the item produces `(x_extent, z_extent)`
///    matching the stack's footprint exactly, and
/// 2. `stack.accumulated_height + item_y_extent <= bin_height_cap`.
///
/// Among all accepting stacks, the one with the smallest
/// `accumulated_height` wins. Tiebreaks fall back to stack creation order.
fn pick_stack_for(stacks: &[Stack], flat: &FlatItem, bin_height_cap: u32) -> Option<StackMatch> {
    let mut best: Option<(StackMatch, u32)> = None;
    for (index, stack) in stacks.iter().enumerate() {
        let Some(chosen) = matching_rotation(&flat.item, stack.footprint_x, stack.footprint_z)
        else {
            continue;
        };
        if stack.accumulated_height.saturating_add(chosen.y_extent) > bin_height_cap {
            continue;
        }
        let accept = match &best {
            None => true,
            Some((_, best_height)) => stack.accumulated_height < *best_height,
        };
        if accept {
            best = Some((StackMatch { index, rotation: chosen }, stack.accumulated_height));
        }
    }
    best.map(|(m, _)| m)
}

/// Return the first allowed rotation (in declaration order) whose
/// `(x_extent, z_extent)` matches `(target_x, target_z)` exactly, or
/// `None` if no rotation matches.
fn matching_rotation(
    item: &ItemInstance3D,
    target_x: u32,
    target_z: u32,
) -> Option<ChosenRotation> {
    for (rotation, x_extent, y_extent, z_extent) in item.orientations() {
        if x_extent == target_x && z_extent == target_z {
            return Some(ChosenRotation { rotation, x_extent, y_extent, z_extent });
        }
    }
    None
}

/// Append `flat.item` to `stack` under the rotation identified by
/// [`pick_stack_for`].
fn push_onto_stack(stack: &mut Stack, flat: &FlatItem, rotation: ChosenRotation) {
    stack.accumulated_height = stack.accumulated_height.saturating_add(rotation.y_extent);
    stack.items.push(StackEntry {
        item: flat.item.clone(),
        rotation: rotation.rotation,
        y_extent: rotation.y_extent,
        x_extent: rotation.x_extent,
        z_extent: rotation.z_extent,
    });
}

/// Whether the stack fits inside `bin` in isolation — footprint against
/// the floor and accumulated height against `bin.height`.
fn stack_fits_bin(stack: &Stack, bin: &Bin3D) -> bool {
    stack.footprint_x <= bin.width
        && stack.footprint_z <= bin.depth
        && stack.accumulated_height <= bin.height
}

/// Pick the next bin type to open. Chooses the smallest-volume bin type
/// (among those with remaining quantity) that can hold at least one of
/// the pending stacks in isolation. Tiebreaks by declaration order.
fn select_next_bin(
    problem: &ThreeDProblem,
    bin_quantity_used: &[usize],
    pending: &[Stack],
) -> Option<usize> {
    let mut best: Option<(usize, u64)> = None;
    for (bin_index, bin) in problem.bins.iter().enumerate() {
        if let Some(cap) = bin.quantity
            && bin_quantity_used[bin_index] >= cap
        {
            continue;
        }
        if !pending.iter().any(|stack| stack_fits_bin(stack, bin)) {
            continue;
        }
        let volume = volume_u64(bin.width, bin.height, bin.depth);
        best = match best {
            None => Some((bin_index, volume)),
            Some((current_index, current_volume)) => {
                if volume < current_volume {
                    Some((bin_index, volume))
                } else {
                    Some((current_index, current_volume))
                }
            }
        };
    }
    best.map(|(index, _)| index)
}

/// Recover the stack index from the synthetic `__stack_<index>__` name.
fn parse_stack_index(name: &str) -> Option<usize> {
    name.strip_prefix("__stack_")
        .and_then(|rest| rest.strip_suffix("__"))
        .and_then(|s| s.parse::<usize>().ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::three_d::model::{Bin3D, BoxDemand3D, RotationMask3D, ThreeDProblem};

    fn single_box_problem() -> ThreeDProblem {
        ThreeDProblem {
            bins: vec![Bin3D {
                name: "b".into(),
                width: 10,
                height: 10,
                depth: 10,
                cost: 1.0,
                quantity: None,
            }],
            demands: vec![BoxDemand3D {
                name: "a".into(),
                width: 4,
                height: 4,
                depth: 4,
                quantity: 1,
                allowed_rotations: RotationMask3D::ALL,
            }],
        }
    }

    #[test]
    fn column_building_places_single_box() {
        let problem = single_box_problem();
        let solution = solve_column_building(&problem, &ThreeDOptions::default()).expect("solve");
        assert!(solution.unplaced.is_empty());
        assert_eq!(solution.bin_count, 1);
        assert_eq!(solution.layouts[0].placements.len(), 1);
    }

    #[test]
    fn column_building_algorithm_name_is_column_building() {
        let problem = single_box_problem();
        let solution = solve_column_building(&problem, &ThreeDOptions::default()).expect("solve");
        assert_eq!(solution.algorithm, "column_building");
        assert!(!solution.guillotine);
    }

    #[test]
    fn column_building_opens_second_bin_when_full() {
        // Two items, each filling a full bin floor, so two stacks but only
        // one stack fits per bin footprint.
        let problem = ThreeDProblem {
            bins: vec![Bin3D {
                name: "b".into(),
                width: 5,
                height: 5,
                depth: 5,
                cost: 1.0,
                quantity: None,
            }],
            demands: vec![BoxDemand3D {
                name: "a".into(),
                width: 5,
                height: 5,
                depth: 5,
                quantity: 2,
                allowed_rotations: RotationMask3D::XYZ,
            }],
        };
        let solution = solve_column_building(&problem, &ThreeDOptions::default()).expect("solve");
        assert!(solution.unplaced.is_empty());
        assert_eq!(solution.bin_count, 2);
    }

    #[test]
    fn column_building_chooses_a_bin_feasible_flat_orientation() {
        // 6x1x1 box in a 1x6x1 bin: only rotations that put `6` on the y
        // axis fit. The flat-orientation picker defaults to the rotation
        // with the smallest y-extent (1), so the first attempt would
        // place it flat — but the 1x6x1 bin can't hold a 6x1 footprint,
        // so the placement must fall back to a rotation that orients
        // the 6 along y.
        //
        let problem = ThreeDProblem {
            bins: vec![Bin3D {
                name: "b".into(),
                width: 1,
                height: 6,
                depth: 1,
                cost: 1.0,
                quantity: None,
            }],
            demands: vec![BoxDemand3D {
                name: "a".into(),
                width: 6,
                height: 1,
                depth: 1,
                quantity: 1,
                allowed_rotations: RotationMask3D::ALL,
            }],
        };
        let solution = solve_column_building(&problem, &ThreeDOptions::default()).expect("solve");
        assert!(solution.unplaced.is_empty());
        assert_eq!(solution.layouts.len(), 1);
        let placement = &solution.layouts[0].placements[0];
        assert_eq!((placement.width, placement.height, placement.depth), (1, 6, 1));
    }

    #[test]
    fn column_building_stacks_share_xz_and_grow_monotonically_in_y() {
        // Four uniform boxes with the same footprint should stack into a
        // single column, each on top of the last.
        let problem = ThreeDProblem {
            bins: vec![Bin3D {
                name: "tall".into(),
                width: 4,
                height: 20,
                depth: 4,
                cost: 1.0,
                quantity: None,
            }],
            demands: vec![BoxDemand3D {
                name: "unit".into(),
                width: 4,
                height: 3,
                depth: 4,
                quantity: 4,
                allowed_rotations: RotationMask3D::XYZ,
            }],
        };
        let solution = solve_column_building(&problem, &ThreeDOptions::default()).expect("solve");
        assert!(solution.unplaced.is_empty());
        assert_eq!(solution.bin_count, 1);
        let placements = &solution.layouts[0].placements;
        assert_eq!(placements.len(), 4);

        // Group by `(x, z)` — column building guarantees every stack has
        // a unique footprint anchor.
        let mut by_column: std::collections::BTreeMap<(u32, u32), Vec<&Placement3D>> =
            std::collections::BTreeMap::new();
        for placement in placements {
            by_column.entry((placement.x, placement.z)).or_default().push(placement);
        }
        assert_eq!(by_column.len(), 1, "all four items should share one column");

        for column in by_column.values() {
            // Monotonically increasing y with no overlaps.
            let mut sorted = column.clone();
            sorted.sort_by_key(|placement| placement.y);
            let mut cursor: u32 = 0;
            for placement in sorted {
                assert_eq!(placement.y, cursor, "stack must be contiguous in y");
                cursor = cursor.saturating_add(placement.height);
            }
            assert!(cursor <= 20, "column must fit the bin height");
        }
    }

    #[test]
    fn column_building_heterogeneous_footprints_degenerate_to_one_stack_per_item() {
        // Two items with different footprints share a bin. Each becomes
        // its own stack, so the solver reduces to 2D-packing the two
        // footprints on the bin floor.
        let problem = ThreeDProblem {
            bins: vec![Bin3D {
                name: "b".into(),
                width: 10,
                height: 10,
                depth: 10,
                cost: 1.0,
                quantity: None,
            }],
            demands: vec![
                BoxDemand3D {
                    name: "a".into(),
                    width: 4,
                    height: 3,
                    depth: 5,
                    quantity: 1,
                    allowed_rotations: RotationMask3D::XYZ,
                },
                BoxDemand3D {
                    name: "b".into(),
                    width: 3,
                    height: 3,
                    depth: 4,
                    quantity: 1,
                    allowed_rotations: RotationMask3D::XYZ,
                },
            ],
        };
        let solution = solve_column_building(&problem, &ThreeDOptions::default()).expect("solve");
        assert!(solution.unplaced.is_empty());
        assert_eq!(solution.bin_count, 1);
        assert_eq!(solution.layouts[0].placements.len(), 2);

        // Neither placement sits on top of the other — both are at y = 0.
        for placement in &solution.layouts[0].placements {
            assert_eq!(placement.y, 0, "heterogeneous footprints should not stack");
        }
    }
}
