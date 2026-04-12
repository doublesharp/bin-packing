//! Bischoff & Marriott (1990) vertical wall-building 3D bin packer.
//!
//! The bin is sliced into vertical "walls" perpendicular to the depth axis.
//! Each wall is packed face-on as a 2D problem — its width is the bin
//! width, its height is the bin height, and its depth is chosen to match
//! the deepest item in the wall. Items are sorted in descending order of
//! their maximum per-rotation z-extent, then greedily assigned to the
//! current wall (provided their chosen-rotation z-extent fits within the
//! wall's depth). The 2D packing step is delegated to
//! [`crate::two_d::place_into_sheet`].
//!
//! v1 packs each item at the back of its wall (`z = wall_z_offset`) and
//! leaves any front gap empty. A future revision could attempt to fill
//! front gaps with shallower items to tighten utilisation.

use super::common::{BinPlacements, build_solution, check_bin_count_cap, volume_u64};
use super::model::{
    Bin3D, ItemInstance3D, Placement3D, Rotation3D, SolverMetrics3D, ThreeDOptions, ThreeDProblem,
    ThreeDSolution,
};
use crate::Result;
use crate::two_d::{ItemInstance2D, Placement2D, TwoDAlgorithm, TwoDOptions, place_into_sheet};

/// A pre-rotated item paired with the rotation that yields its maximum
/// z-extent. The wall-building engine sorts the working set by
/// `z_extent` descending and drains walls using these extents directly.
#[derive(Debug, Clone)]
struct WallItem {
    item: ItemInstance3D,
    rotation: Rotation3D,
    x_extent: u32,
    y_extent: u32,
    z_extent: u32,
}

/// Solve a 3D wall-building problem.
///
/// Slices each bin into vertical walls perpendicular to the depth axis,
/// packs each wall as a 2D sub-problem via
/// [`crate::two_d::place_into_sheet`], and loops across multiple bins
/// when a single bin cannot absorb every demand. Honours
/// [`Bin3D::quantity`] caps and tiebreaks bin selection by declaration
/// order among bin types of equal volume.
///
/// # Errors
///
/// Propagates any error returned by the inner 2D solver or by
/// [`build_solution`] when the solution would exceed
/// [`crate::three_d::MAX_BIN_COUNT_3D`].
pub(super) fn solve_wall_building(
    problem: &ThreeDProblem,
    _options: &ThreeDOptions,
) -> Result<ThreeDSolution> {
    let raw_items = problem.expanded_items();

    // Pre-compute the maximum z-extent and the rotation that achieves it
    // for every item, filtering rotations by the item's allowed mask.
    // Tiebreak on `Rotation3D` declaration order (the iterator walks the
    // mask in declaration order, so the first match wins ties).
    let mut working: Vec<WallItem> = raw_items
        .into_iter()
        .filter_map(|item| {
            best_rotation_for_wall(&item, &problem.bins).map(|chosen| chosen.into_wall_item(item))
        })
        .collect();

    // Descending by z-extent, with stable tiebreaks on volume (larger
    // first) then demand index for determinism across rotations with
    // identical z-extents.
    working.sort_by(|a, b| {
        let by_z = b.z_extent.cmp(&a.z_extent);
        if by_z.is_ne() {
            return by_z;
        }
        let vol_a = volume_u64(a.x_extent, a.y_extent, a.z_extent);
        let vol_b = volume_u64(b.x_extent, b.y_extent, b.z_extent);
        let by_vol = vol_b.cmp(&vol_a);
        if by_vol.is_ne() {
            return by_vol;
        }
        a.item.demand_index.cmp(&b.item.demand_index)
    });

    let mut bin_placements: Vec<BinPlacements> = Vec::new();
    let mut bin_quantity_used: Vec<usize> = vec![0; problem.bins.len()];
    let mut unplaced: Vec<ItemInstance3D> = Vec::new();

    // Drain the working list one bin at a time. Every iteration opens a
    // bin (if needed) and walks it wall-by-wall until the bin's depth is
    // exhausted or no item fits the remaining depth. Items that still
    // refuse to place after the bin is full carry over to the next bin.
    while !working.is_empty() {
        check_bin_count_cap(bin_placements.len())?;
        let Some(bin_index) = select_bin_for(&problem.bins, &bin_quantity_used, &working[0]) else {
            // No declared bin can hold the deepest remaining item under
            // its chosen rotation — every remaining item spills into
            // `unplaced`. Because the working list is sorted by z-extent
            // descending, subsequent items may still fit a different
            // bin, so we only drop the ones that have no home.
            let (placed_elsewhere, dropped) =
                split_items_with_no_home(&problem.bins, &bin_quantity_used, working);
            working = placed_elsewhere;
            for wall_item in dropped {
                unplaced.push(wall_item.item);
            }
            continue;
        };

        let bin = &problem.bins[bin_index];
        let (bin_placements_vec, remaining) = pack_single_bin(bin, working)?;
        working = remaining;

        if bin_placements_vec.is_empty() {
            // Defensive: `select_bin_for` guaranteed the deepest item
            // fits, so a fresh bin cannot refuse everything. If it
            // somehow does, move every item at or above this bin's
            // depth limit into `unplaced` to guarantee forward progress.
            debug_assert!(false, "wall-building opened a bin but placed nothing");
            for wall_item in working {
                unplaced.push(wall_item.item);
            }
            break;
        }

        bin_quantity_used[bin_index] = bin_quantity_used[bin_index].saturating_add(1);
        bin_placements.push((bin_index, bin_placements_vec));
    }

    let unplaced_count = unplaced.len();
    let bin_count = bin_placements.len();
    let metrics = SolverMetrics3D {
        iterations: 1,
        explored_states: 0,
        extreme_points_generated: 0,
        branch_and_bound_nodes: 0,
        notes: vec![format!("wall_building: {bin_count} bin(s), {unplaced_count} unplaced")],
    };

    build_solution("wall_building", &problem.bins, bin_placements, unplaced, metrics, false)
}

/// Captures the rotation chosen for an item during wall assignment.
struct ChosenRotation {
    rotation: Rotation3D,
    x_extent: u32,
    y_extent: u32,
    z_extent: u32,
}

impl ChosenRotation {
    fn into_wall_item(self, item: ItemInstance3D) -> WallItem {
        WallItem {
            item,
            rotation: self.rotation,
            x_extent: self.x_extent,
            y_extent: self.y_extent,
            z_extent: self.z_extent,
        }
    }
}

/// Pick the rotation that maximises the item's z-extent, breaking ties
/// on [`Rotation3D`] declaration order (the first encountered rotation
/// wins).
fn best_rotation_for_wall(item: &ItemInstance3D, bins: &[Bin3D]) -> Option<ChosenRotation> {
    let mut best: Option<ChosenRotation> = None;
    let mut fallback: Option<ChosenRotation> = None;
    for (rotation, x, y, z) in item.orientations() {
        match &fallback {
            Some(current) if current.z_extent >= z => {}
            _ => {
                fallback = Some(ChosenRotation { rotation, x_extent: x, y_extent: y, z_extent: z });
            }
        }
        if !bins.iter().any(|bin| x <= bin.width && y <= bin.height && z <= bin.depth) {
            continue;
        }
        match &best {
            Some(current) if current.z_extent >= z => {}
            _ => {
                best = Some(ChosenRotation { rotation, x_extent: x, y_extent: y, z_extent: z });
            }
        }
    }
    best.or(fallback)
}

/// Select the smallest-volume bin type that can accept `wall_item`'s
/// chosen rotation and still has remaining quantity. Tiebreaks on
/// declaration order among bins of equal volume.
fn select_bin_for(
    bins: &[Bin3D],
    bin_quantity_used: &[usize],
    wall_item: &WallItem,
) -> Option<usize> {
    let mut best: Option<(usize, u64)> = None;
    for (bin_index, bin) in bins.iter().enumerate() {
        if let Some(cap) = bin.quantity
            && bin_quantity_used[bin_index] >= cap
        {
            continue;
        }
        if !bin_accepts(bin, wall_item) {
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

/// Whether `bin` can contain `wall_item` in its chosen rotation.
fn bin_accepts(bin: &Bin3D, wall_item: &WallItem) -> bool {
    wall_item.x_extent <= bin.width
        && wall_item.y_extent <= bin.height
        && wall_item.z_extent <= bin.depth
}

/// Partition `items` into `(still_placeable, doomed)` where every item
/// in `doomed` has no remaining declared bin type that could hold it.
/// Called when the current bin-selection step returns `None` for the
/// head of the list, to peel off items that cannot ever fit rather than
/// stalling the outer loop.
fn split_items_with_no_home(
    bins: &[Bin3D],
    bin_quantity_used: &[usize],
    items: Vec<WallItem>,
) -> (Vec<WallItem>, Vec<WallItem>) {
    let mut still = Vec::with_capacity(items.len());
    let mut doomed = Vec::new();
    for wall_item in items {
        if select_bin_for(bins, bin_quantity_used, &wall_item).is_some() {
            still.push(wall_item);
        } else {
            doomed.push(wall_item);
        }
    }
    (still, doomed)
}

/// Pack `items` into a single fresh bin using the wall-building rule,
/// returning the placements emitted and any items that did not fit.
fn pack_single_bin(bin: &Bin3D, items: Vec<WallItem>) -> Result<(Vec<Placement3D>, Vec<WallItem>)> {
    let mut remaining: Vec<WallItem> = items;
    let mut bin_placements: Vec<Placement3D> = Vec::new();
    let mut wall_z_offset: u32 = 0;

    while !remaining.is_empty() {
        // Filter to the items that (a) fit this bin in their chosen
        // rotation and (b) still fit the remaining depth of the bin.
        let depth_remaining = bin.depth.saturating_sub(wall_z_offset);
        if depth_remaining == 0 {
            break;
        }

        // The first item whose chosen rotation fits this bin determines
        // the wall depth. Items are sorted by z-extent descending, so
        // the first fit is the deepest feasible item.
        let wall_depth = match remaining.iter().find(|wall_item| {
            wall_item.x_extent <= bin.width
                && wall_item.y_extent <= bin.height
                && wall_item.z_extent <= depth_remaining
        }) {
            Some(wall_item) => wall_item.z_extent,
            None => break,
        };

        // Drain the items that can fit inside this wall into a candidate
        // list. We keep the residual items (those whose z-extent exceeds
        // `wall_depth`, or whose footprint exceeds the bin face) in
        // `residual` and pass the candidates to the 2D solver.
        let mut candidates: Vec<WallItem> = Vec::new();
        let mut residual: Vec<WallItem> = Vec::new();
        for wall_item in remaining.drain(..) {
            let fits_face = wall_item.x_extent <= bin.width && wall_item.y_extent <= bin.height;
            if fits_face && wall_item.z_extent <= wall_depth {
                candidates.push(wall_item);
            } else {
                residual.push(wall_item);
            }
        }

        // Build the 2D item list. Names are preserved so placements can
        // be mapped back to the originating 3D items; within a wall,
        // every declared demand can only appear once per `ItemInstance3D`
        // so name collisions reflect a single originating box.
        let items_2d: Vec<ItemInstance2D> = candidates
            .iter()
            .map(|wall_item| ItemInstance2D {
                name: wall_item.item.name.clone(),
                width: wall_item.x_extent,
                height: wall_item.y_extent,
                // Wall-building fixes the rotation at bin-selection
                // time; the 2D step must not rotate the face itself.
                can_rotate: false,
            })
            .collect();

        let (placements_2d, unplaced_2d) = place_into_sheet(
            &items_2d,
            bin.width,
            bin.height,
            TwoDAlgorithm::Auto,
            &TwoDOptions::default(),
        )?;

        // Convert 2D placements back into 3D. The wall is anchored at
        // `z = wall_z_offset` and its depth is `wall_depth`. The 2D step
        // guarantees no rotation (we passed `can_rotate = false`), so
        // `Placement2D.width == wall_item.x_extent` and likewise for
        // height.
        let (placed_names, unplaced_names) = partition_by_placement(&candidates, &placements_2d);

        for (wall_item, placement_2d) in placed_names {
            bin_placements.push(Placement3D {
                name: wall_item.item.name.clone(),
                x: placement_2d.x,
                y: placement_2d.y,
                z: wall_z_offset,
                width: placement_2d.width,
                height: placement_2d.height,
                depth: wall_item.z_extent,
                rotation: wall_item.rotation,
            });
        }

        // Items that the 2D solver failed to place go back into the
        // working list for the next wall / next bin, together with the
        // residual carried over from this wall.
        let mut next_remaining: Vec<WallItem> = unplaced_names;
        next_remaining.extend(residual);
        // Re-sort so the descending-by-z invariant holds for the next
        // wall even after residuals are reintroduced.
        next_remaining.sort_by(|a, b| {
            let by_z = b.z_extent.cmp(&a.z_extent);
            if by_z.is_ne() {
                return by_z;
            }
            a.item.demand_index.cmp(&b.item.demand_index)
        });

        // Silence: `unplaced_2d` is strictly informational — every entry
        // appears in the unplaced name list we computed above because
        // `place_into_sheet` preserves the original item names when the
        // solver returns without placing them. Debug-assert the two are
        // consistent to catch drift if `place_into_sheet` ever changes.
        debug_assert_eq!(
            unplaced_2d.len() + placements_2d.len(),
            candidates.len(),
            "place_into_sheet accounting mismatch",
        );

        remaining = next_remaining;
        wall_z_offset = wall_z_offset.saturating_add(wall_depth);

        // If the 2D solver placed nothing from this wall, the wall
        // itself contributed no boxes; advancing `wall_z_offset` would
        // waste the remainder of the bin. Instead, break so the caller
        // can open a fresh bin for the remaining items.
        if !bin_placements
            .iter()
            .any(|placement| placement.z >= wall_z_offset.saturating_sub(wall_depth))
        {
            // This branch is unreachable in practice because
            // `select_bin_for` guarantees at least one item fits the
            // fresh bin, but leaving the explicit break here keeps the
            // invariant obvious for future maintainers.
            debug_assert!(false, "wall-building selected a bin that placed nothing");
            break;
        }
    }

    Ok((bin_placements, remaining))
}

/// Pair each placed 2D placement back to its originating [`WallItem`]
/// and return the list of items the 2D solver dropped.
///
/// The pairing is name-driven: `place_into_sheet` preserves the
/// `ItemInstance2D.name` on both the returned placements and the
/// returned leftover list. Because we built the 2D input from a single
/// candidate vector, each placement maps one-to-one to an unused
/// candidate with a matching name.
fn partition_by_placement(
    candidates: &[WallItem],
    placements_2d: &[Placement2D],
) -> (Vec<(WallItem, Placement2D)>, Vec<WallItem>) {
    let mut taken: Vec<bool> = vec![false; candidates.len()];
    let mut placed: Vec<(WallItem, Placement2D)> = Vec::with_capacity(placements_2d.len());

    for placement in placements_2d {
        if let Some((idx, wall_item)) = candidates.iter().enumerate().find(|(idx, wall_item)| {
            !taken[*idx]
                && wall_item.item.name == placement.name
                && wall_item.x_extent == placement.width
                && wall_item.y_extent == placement.height
        }) {
            taken[idx] = true;
            placed.push((wall_item.clone(), placement.clone()));
        } else if let Some((idx, wall_item)) = candidates
            .iter()
            .enumerate()
            .find(|(idx, wall_item)| !taken[*idx] && wall_item.item.name == placement.name)
        {
            // Fallback: dimensions didn't line up (should not happen
            // because we passed `can_rotate = false`). Take the first
            // name match anyway so we never silently drop a placement.
            debug_assert!(
                false,
                "wall-building placement dimensions diverged from candidate: {}",
                placement.name,
            );
            taken[idx] = true;
            placed.push((wall_item.clone(), placement.clone()));
        }
    }

    let leftover: Vec<WallItem> = candidates
        .iter()
        .enumerate()
        .filter_map(|(idx, wall_item)| if taken[idx] { None } else { Some(wall_item.clone()) })
        .collect();

    (placed, leftover)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::three_d::model::{Bin3D, BoxDemand3D, RotationMask3D, ThreeDProblem};

    fn sample_bin(name: &str, w: u32, h: u32, d: u32) -> Bin3D {
        Bin3D { name: name.into(), width: w, height: h, depth: d, cost: 1.0, quantity: None }
    }

    fn sample_demand(name: &str, w: u32, h: u32, d: u32, qty: usize) -> BoxDemand3D {
        BoxDemand3D {
            name: name.into(),
            width: w,
            height: h,
            depth: d,
            quantity: qty,
            allowed_rotations: RotationMask3D::ALL,
        }
    }

    #[test]
    fn wall_building_trivial_fit_single_box() {
        let problem = ThreeDProblem {
            bins: vec![sample_bin("b", 10, 10, 10)],
            demands: vec![sample_demand("a", 4, 4, 4, 1)],
        };
        let solution =
            solve_wall_building(&problem, &ThreeDOptions::default()).expect("wall solve");
        assert_eq!(solution.algorithm, "wall_building");
        assert_eq!(solution.bin_count, 1);
        assert!(solution.unplaced.is_empty(), "unplaced: {:?}", solution.unplaced);
        assert!(!solution.guillotine);
        assert_eq!(solution.layouts[0].placements.len(), 1);
    }

    #[test]
    fn wall_building_opens_second_bin_when_full() {
        // Two 5x5x5 cubes with rotation locked to XYZ and a 5x5x5 bin:
        // only one fits per bin, so the solver must open a second bin.
        let problem = ThreeDProblem {
            bins: vec![sample_bin("b", 5, 5, 5)],
            demands: vec![BoxDemand3D {
                name: "a".into(),
                width: 5,
                height: 5,
                depth: 5,
                quantity: 2,
                allowed_rotations: RotationMask3D::XYZ,
            }],
        };
        let solution =
            solve_wall_building(&problem, &ThreeDOptions::default()).expect("wall solve");
        assert_eq!(solution.bin_count, 2);
        assert!(solution.unplaced.is_empty());
        let total_placements: usize =
            solution.layouts.iter().map(|layout| layout.placements.len()).sum();
        assert_eq!(total_placements, 2);
    }

    #[test]
    fn wall_building_respects_rotation_mask() {
        // Declared as 2x3x5, allowed rotations = ALL. Wall-building
        // picks the rotation that maximises z-extent (5), so the
        // placement's depth must be 5 regardless of the declared
        // depth of 5 lining up with the identity rotation or not.
        // Verify the chosen extents are a valid axis-permutation of
        // the declared extents.
        let problem = ThreeDProblem {
            bins: vec![sample_bin("b", 10, 10, 10)],
            demands: vec![sample_demand("a", 2, 3, 5, 1)],
        };
        let solution =
            solve_wall_building(&problem, &ThreeDOptions::default()).expect("wall solve");
        assert!(solution.unplaced.is_empty());
        assert_eq!(solution.bin_count, 1);
        let placement = &solution.layouts[0].placements[0];
        assert_eq!(placement.depth, 5, "max z-extent rotation should be chosen");
        let mut extents = [placement.width, placement.height, placement.depth];
        extents.sort_unstable();
        assert_eq!(extents, [2, 3, 5], "placement extents must be a permutation of the demand");
    }

    #[test]
    fn wall_building_honours_restricted_rotation_mask() {
        // Restrict rotations to XYZ only. With 3x4x2 declared, the
        // only legal rotation is the identity, so the placement must
        // match the declared extents exactly.
        let problem = ThreeDProblem {
            bins: vec![sample_bin("b", 10, 10, 10)],
            demands: vec![BoxDemand3D {
                name: "a".into(),
                width: 3,
                height: 4,
                depth: 2,
                quantity: 1,
                allowed_rotations: RotationMask3D::XYZ,
            }],
        };
        let solution =
            solve_wall_building(&problem, &ThreeDOptions::default()).expect("wall solve");
        assert!(solution.unplaced.is_empty());
        let placement = &solution.layouts[0].placements[0];
        assert_eq!(placement.rotation, Rotation3D::Xyz);
        assert_eq!(placement.width, 3);
        assert_eq!(placement.height, 4);
        assert_eq!(placement.depth, 2);
    }

    #[test]
    fn wall_building_chooses_deepest_orientation_that_fits_a_bin() {
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
        let solution =
            solve_wall_building(&problem, &ThreeDOptions::default()).expect("wall solve");
        assert!(solution.unplaced.is_empty());
        let placement = &solution.layouts[0].placements[0];
        assert_eq!((placement.width, placement.height, placement.depth), (1, 6, 1));
    }

    #[test]
    fn wall_building_algorithm_name_is_snake_case() {
        let problem = ThreeDProblem {
            bins: vec![sample_bin("b", 10, 10, 10)],
            demands: vec![sample_demand("a", 2, 2, 2, 1)],
        };
        let solution =
            solve_wall_building(&problem, &ThreeDOptions::default()).expect("wall solve");
        assert_eq!(solution.algorithm, "wall_building");
    }

    #[test]
    fn wall_building_honours_bin_quantity_cap() {
        // Two cubes, locked rotation, bin capped at 1: the second cube
        // must spill into `unplaced`.
        let problem = ThreeDProblem {
            bins: vec![Bin3D {
                name: "b".into(),
                width: 5,
                height: 5,
                depth: 5,
                cost: 1.0,
                quantity: Some(1),
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
        let solution =
            solve_wall_building(&problem, &ThreeDOptions::default()).expect("wall solve");
        assert_eq!(solution.bin_count, 1);
        assert_eq!(solution.unplaced.len(), 1);
    }
}
