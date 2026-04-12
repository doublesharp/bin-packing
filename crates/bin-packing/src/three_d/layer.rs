//! Horizontal layer-building 3D packing (George & Robinson 1980).
//!
//! The bin is decomposed into a stack of horizontal layers whose heights are
//! chosen from the items themselves: the first item assigned to a layer
//! fixes the layer's thickness at its oriented `y_extent`, and every
//! subsequent item must fit under that thickness. Within a layer, the items
//! lie on the layer floor and are packed by delegating to the shared 2D
//! placement engine (`crate::two_d::place_into_sheet`) on a synthetic
//! sheet whose extents are `(bin.width, bin.depth)`.
//!
//! Five variants share one engine, differing only in the inner 2D backend:
//! `auto`, `max_rects`, `skyline`, `guillotine`, and
//! `best_fit_decreasing_height`. The guillotine variant additionally sets
//! `solution.guillotine = true` on the returned solution.
//!
//! # Known limitation (v1)
//!
//! Layer height is fixed by the *first* (thickest) item assigned to the
//! layer. Items shorter than that thickness sit on the layer floor and the
//! slab above them — up to `layer_height - item.y_extent` per item — is
//! unused. There is no cross-layer infill in v1. Instances with highly
//! mixed y-extents will waste vertical space; prefer `extreme_points*`,
//! `deepest_bottom_left_fill`, or the volume-sorted heuristics when
//! y-axis utilisation matters.

use super::common::{BinPlacements, build_solution, surface_area_u64, volume_u64};
use super::model::{
    Bin3D, ItemInstance3D, Placement3D, Rotation3D, SolverMetrics3D, ThreeDOptions, ThreeDProblem,
    ThreeDSolution,
};
use crate::Result;
use crate::two_d::{ItemInstance2D, Placement2D, TwoDAlgorithm, TwoDOptions, place_into_sheet};

/// Solve with layer-building and the `auto` 2D inner backend.
///
/// See the module-level documentation for the known v1 limitation on
/// per-layer y-slab waste.
///
/// # Errors
///
/// Propagates [`crate::BinPackingError::Unsupported`] when the multi-bin
/// loop would exceed [`super::model::MAX_BIN_COUNT_3D`] and any error
/// bubbled up from the inner 2D solver.
pub(super) fn solve_layer_building(
    problem: &ThreeDProblem,
    options: &ThreeDOptions,
) -> Result<ThreeDSolution> {
    solve_with_inner(problem, options, TwoDAlgorithm::Auto, "layer_building")
}

/// Solve with layer-building and the `max_rects` 2D inner backend.
///
/// See the module-level documentation for the known v1 limitation on
/// per-layer y-slab waste.
///
/// # Errors
///
/// See [`solve_layer_building`].
pub(super) fn solve_layer_building_max_rects(
    problem: &ThreeDProblem,
    options: &ThreeDOptions,
) -> Result<ThreeDSolution> {
    solve_with_inner(problem, options, TwoDAlgorithm::MaxRects, "layer_building_max_rects")
}

/// Solve with layer-building and the `skyline` 2D inner backend.
///
/// See the module-level documentation for the known v1 limitation on
/// per-layer y-slab waste.
///
/// # Errors
///
/// See [`solve_layer_building`].
pub(super) fn solve_layer_building_skyline(
    problem: &ThreeDProblem,
    options: &ThreeDOptions,
) -> Result<ThreeDSolution> {
    solve_with_inner(problem, options, TwoDAlgorithm::Skyline, "layer_building_skyline")
}

/// Solve with layer-building and the `guillotine` 2D inner backend.
///
/// Sets `solution.guillotine = true` on the returned solution. See the
/// module-level documentation for the known v1 limitation on per-layer
/// y-slab waste.
///
/// # Errors
///
/// See [`solve_layer_building`].
pub(super) fn solve_layer_building_guillotine(
    problem: &ThreeDProblem,
    options: &ThreeDOptions,
) -> Result<ThreeDSolution> {
    solve_with_inner(problem, options, TwoDAlgorithm::Guillotine, "layer_building_guillotine")
}

/// Solve with layer-building and the `best_fit_decreasing_height` 2D shelf
/// inner backend.
///
/// See the module-level documentation for the known v1 limitation on
/// per-layer y-slab waste.
///
/// # Errors
///
/// See [`solve_layer_building`].
pub(super) fn solve_layer_building_shelf(
    problem: &ThreeDProblem,
    options: &ThreeDOptions,
) -> Result<ThreeDSolution> {
    solve_with_inner(
        problem,
        options,
        TwoDAlgorithm::BestFitDecreasingHeight,
        "layer_building_shelf",
    )
}

/// Shared engine backing every `solve_layer_building*` entry point.
///
/// `inner` selects the 2D placement backend used to pack each layer's
/// footprint; `name` is the algorithm string stamped on the returned
/// solution. Items are oriented "as flat as possible" (minimum y-extent),
/// sorted by min y-extent descending, and then greedily swept into layers
/// whose height is fixed by the first item assigned to each layer. A new
/// bin is opened whenever the next layer would overflow the current bin's
/// height or when the 2D backend cannot place any remaining item in the
/// current layer.
fn solve_with_inner(
    problem: &ThreeDProblem,
    _options: &ThreeDOptions,
    inner: TwoDAlgorithm,
    name: &'static str,
) -> Result<ThreeDSolution> {
    // 1. Orient every item "as flat as possible" — chose the rotation that
    //    minimises the y-extent among rotations that fit at least one
    //    declared bin, tiebreaking by `Rotation3D` declaration order. Build
    //    a `Vec<OrientedItem>` so we can re-sort and drain.
    let mut oriented: Vec<OrientedItem> = problem
        .expanded_items()
        .into_iter()
        .map(|item| {
            let flat = flattest_orientation(&item, &problem.bins);
            OrientedItem {
                item,
                rotation: flat.rotation,
                x_extent: flat.x,
                y_extent: flat.y,
                z_extent: flat.z,
            }
        })
        .collect();

    // 2. Sort by min y-extent desc, with a stable tiebreak that orders the
    //    remaining axes by decreasing area so similar-thickness items cluster.
    oriented.sort_by(|a, b| {
        b.y_extent.cmp(&a.y_extent).then_with(|| {
            let area_a = surface_area_u64(a.x_extent, a.z_extent);
            let area_b = surface_area_u64(b.x_extent, b.z_extent);
            area_b.cmp(&area_a)
        })
    });

    // 3. Multi-bin loop. `pending` is the queue of items yet to be placed.
    let mut pending: Vec<OrientedItem> = oriented;
    let mut bin_placements: Vec<BinPlacements> = Vec::new();
    let mut bin_quantity_used: Vec<usize> = vec![0; problem.bins.len()];
    let mut unplaced: Vec<ItemInstance3D> = Vec::new();

    let two_d_options = TwoDOptions::default();

    while let Some(next) = pending.first() {
        // Pick the smallest bin type (by volume) that can fit the first
        // pending item in its chosen orientation. Tiebreak on `Bin3D`
        // declaration order. Honour `Bin3D.quantity` caps.
        let Some(bin_index) = select_bin_for_item(problem, next, &bin_quantity_used) else {
            // No feasible bin: this item and all remaining items become
            // unplaced.
            for item in pending.drain(..) {
                unplaced.push(item.item);
            }
            break;
        };

        super::common::check_bin_count_cap(bin_placements.len())?;

        bin_quantity_used[bin_index] = bin_quantity_used[bin_index].saturating_add(1);
        let bin = &problem.bins[bin_index];

        let mut bin_ok_placements: Vec<Placement3D> = Vec::new();
        let mut layer_y_offset: u32 = 0;

        // Keep stuffing layers into this bin until something forces us to
        // open a new bin.
        loop {
            // Drop any item that cannot fit this bin in its flat
            // orientation — push it onto the unplaced list. This can
            // happen when `select_bin_for_item` chose a bin large enough
            // for the *first* item but later items are larger along some
            // axis.
            pending.retain(|oi| {
                if oi.x_extent <= bin.width && oi.y_extent <= bin.height && oi.z_extent <= bin.depth
                {
                    true
                } else {
                    unplaced.push(oi.item.clone());
                    false
                }
            });

            let Some(first) = pending.first() else {
                break;
            };

            let layer_height = first.y_extent;
            if layer_y_offset.saturating_add(layer_height) > bin.height {
                // No room for even the first pending item as a fresh
                // layer in this bin. Close the bin and open another.
                break;
            }

            // Collect every pending item whose min y-extent fits under
            // this layer's thickness into a candidate list, preserving
            // the sort order.
            let mut layer_item_indices: Vec<usize> = Vec::new();
            for (idx, oi) in pending.iter().enumerate() {
                if oi.y_extent <= layer_height {
                    layer_item_indices.push(idx);
                }
            }

            // Build the synthetic 2D input. The 2D `height` axis maps
            // onto the 3D depth axis.
            let items_2d: Vec<ItemInstance2D> = layer_item_indices
                .iter()
                .map(|&idx| {
                    let oi = &pending[idx];
                    ItemInstance2D {
                        name: format!("__layer_item_{idx}__"),
                        width: oi.x_extent,
                        height: oi.z_extent,
                        can_rotate: false,
                    }
                })
                .collect();

            let (placements_2d, _leftover_2d) =
                place_into_sheet(&items_2d, bin.width, bin.depth, inner, &two_d_options)?;

            if placements_2d.is_empty() {
                // The 2D backend could not place anything from this
                // candidate set into the layer — which should not
                // normally happen because the candidates already fit the
                // bin's footprint. Guard against an infinite loop: if
                // nothing was placed, the leftover set equals the input,
                // so open a new bin.
                break;
            }

            // Map each synthetic 2D placement name back to the exact
            // pending item index. Demand names are not required to be
            // unique, so name-based occurrence matching is not safe here.
            let mut consumed_indices: Vec<usize> = Vec::with_capacity(placements_2d.len());
            for p2d in &placements_2d {
                let pending_idx = match parse_layer_item_index(&p2d.name) {
                    Some(idx) => idx,
                    None => {
                        // Defensive: the 2D backend returned a name we
                        // did not submit. Skip rather than panic. The
                        // debug_assert catches this in test builds.
                        debug_assert!(
                            false,
                            "place_into_sheet returned an unknown name `{}`",
                            p2d.name
                        );
                        continue;
                    }
                };
                if pending_idx >= pending.len() {
                    debug_assert!(
                        false,
                        "place_into_sheet returned an out-of-range layer item `{}`",
                        p2d.name
                    );
                    continue;
                }
                consumed_indices.push(pending_idx);

                let oi = &pending[pending_idx];
                bin_ok_placements.push(build_placement(p2d, oi, layer_y_offset));
            }

            // Build a "consumed" bitmap over the current `pending` slice
            // so we can drop the placed items in a single `retain` pass
            // rather than O(n) shifts per removal.
            consumed_indices.sort_unstable();
            consumed_indices.dedup();
            let mut consumed_mask: Vec<bool> = vec![false; pending.len()];
            for idx in consumed_indices {
                if idx < consumed_mask.len() {
                    consumed_mask[idx] = true;
                }
            }
            let mut retain_cursor: usize = 0;
            pending.retain(|_| {
                let keep = !consumed_mask[retain_cursor];
                retain_cursor += 1;
                keep
            });

            layer_y_offset = layer_y_offset.saturating_add(layer_height);

            // If the bin is now full (no room for the next item's layer
            // thickness) fall through and the outer loop will open a new
            // bin.
            if let Some(next_pending) = pending.first()
                && layer_y_offset.saturating_add(next_pending.y_extent) > bin.height
            {
                break;
            }
        }

        if bin_ok_placements.is_empty() {
            // We opened a bin but couldn't place anything. Roll back the
            // quantity-used counter so we don't consume a bin slot for
            // nothing, and push the offending first item to `unplaced`
            // to guarantee forward progress. Swap-remove the front item
            // by draining the first element — the remainder's order does
            // not matter for unplacement bookkeeping.
            bin_quantity_used[bin_index] = bin_quantity_used[bin_index].saturating_sub(1);
            if !pending.is_empty() {
                let stuck = pending.swap_remove(0);
                unplaced.push(stuck.item);
                // The swap perturbs ordering, so re-sort by min y-extent
                // desc to preserve the layer-building invariant.
                pending.sort_by(|a, b| {
                    b.y_extent.cmp(&a.y_extent).then_with(|| {
                        let area_a = surface_area_u64(a.x_extent, a.z_extent);
                        let area_b = surface_area_u64(b.x_extent, b.z_extent);
                        area_b.cmp(&area_a)
                    })
                });
            }
            continue;
        }

        bin_placements.push((bin_index, bin_ok_placements));
    }

    let notes = vec![format!(
        "{name}: {} bin(s), {} unplaced; per-layer y-slab above short items is not infilled (v1)",
        bin_placements.len(),
        unplaced.len(),
    )];

    let metrics = SolverMetrics3D {
        iterations: 1,
        explored_states: 0,
        extreme_points_generated: 0,
        branch_and_bound_nodes: 0,
        notes,
    };

    let guillotine = matches!(inner, TwoDAlgorithm::Guillotine);
    build_solution(name, &problem.bins, bin_placements, unplaced, metrics, guillotine)
}

/// Convert a 2D placement on the bin floor into a 3D placement at the
/// layer's `y` offset. Uses the item's pre-computed flat orientation for
/// the y-extent and rotation fields.
fn build_placement(p2d: &Placement2D, oi: &OrientedItem, layer_y_offset: u32) -> Placement3D {
    Placement3D {
        name: oi.item.name.clone(),
        x: p2d.x,
        y: layer_y_offset,
        z: p2d.y,
        width: p2d.width,
        height: oi.y_extent,
        depth: p2d.height,
        rotation: oi.rotation,
    }
}

/// Pick the smallest-volume bin type that can physically contain `item` in
/// its flat orientation and still has remaining quantity. Ties break on
/// `Bin3D` declaration order.
fn select_bin_for_item(
    problem: &ThreeDProblem,
    item: &OrientedItem,
    bin_quantity_used: &[usize],
) -> Option<usize> {
    let mut best: Option<(usize, u64)> = None;
    for (bin_index, bin) in problem.bins.iter().enumerate() {
        if let Some(cap) = bin.quantity
            && bin_quantity_used[bin_index] >= cap
        {
            continue;
        }
        if !bin_contains_item(bin, item) {
            continue;
        }
        let volume = volume_u64(bin.width, bin.height, bin.depth);
        match best {
            None => best = Some((bin_index, volume)),
            Some((_, current_volume)) if volume < current_volume => {
                best = Some((bin_index, volume));
            }
            Some(_) => {}
        }
    }
    best.map(|(index, _)| index)
}

fn parse_layer_item_index(name: &str) -> Option<usize> {
    name.strip_prefix("__layer_item_")
        .and_then(|rest| rest.strip_suffix("__"))
        .and_then(|index| index.parse::<usize>().ok())
}

/// Whether the bin can hold the item under its flat orientation.
fn bin_contains_item(bin: &Bin3D, item: &OrientedItem) -> bool {
    item.x_extent <= bin.width && item.y_extent <= bin.height && item.z_extent <= bin.depth
}

/// The "flattest" orientation of an item — the rotation that minimises the
/// `y` extent. Ties break on [`Rotation3D`] declaration order.
struct FlatOrientation {
    rotation: Rotation3D,
    x: u32,
    y: u32,
    z: u32,
}

fn flattest_orientation(item: &ItemInstance3D, bins: &[Bin3D]) -> FlatOrientation {
    let mut best: Option<FlatOrientation> = None;
    let mut fallback: Option<FlatOrientation> = None;
    for (rotation, x, y, z) in item.orientations() {
        let candidate = FlatOrientation { rotation, x, y, z };
        let replace_fallback = match &fallback {
            None => true,
            Some(current) => y < current.y,
        };
        if replace_fallback {
            fallback = Some(FlatOrientation { rotation, x, y, z });
        }
        if !bins.iter().any(|bin| x <= bin.width && y <= bin.height && z <= bin.depth) {
            continue;
        }
        let take = match &best {
            None => true,
            Some(current) => y < current.y,
        };
        if take {
            best = Some(candidate);
        }
    }
    // `orientations()` always yields at least one rotation because the
    // demand-level validator rejects empty rotation masks.
    match best.or(fallback) {
        Some(flat) => flat,
        None => FlatOrientation {
            rotation: Rotation3D::Xyz,
            x: item.width,
            y: item.height,
            z: item.depth,
        },
    }
}

/// An item paired with its flat orientation.
#[derive(Clone)]
struct OrientedItem {
    item: ItemInstance3D,
    rotation: Rotation3D,
    x_extent: u32,
    y_extent: u32,
    z_extent: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::three_d::model::{
        Bin3D, BoxDemand3D, RotationMask3D, ThreeDAlgorithm, ThreeDOptions, ThreeDProblem,
    };

    fn problem_one_box() -> ThreeDProblem {
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
    fn layer_building_places_single_box() {
        let solution = solve_layer_building(&problem_one_box(), &ThreeDOptions::default())
            .expect("solve layer_building");
        assert!(solution.unplaced.is_empty(), "unexpected unplaced: {:?}", solution.unplaced);
        assert_eq!(solution.bin_count, 1);
        assert_eq!(solution.algorithm, "layer_building");
        assert!(!solution.guillotine);
        assert_eq!(solution.layouts.len(), 1);
        let layout = &solution.layouts[0];
        assert_eq!(layout.placements.len(), 1);
        let placement = &layout.placements[0];
        assert_eq!(placement.y, 0);
        assert_eq!(placement.width, 4);
        assert_eq!(placement.height, 4);
        assert_eq!(placement.depth, 4);
    }

    #[test]
    fn layer_building_opens_second_bin_when_full() {
        // A 5x5x5 bin with only a single 5x5x5 cube worth of floor space
        // plus a cap on one layer. Asking for two cubes forces a second bin.
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
        let solution =
            solve_layer_building(&problem, &ThreeDOptions::default()).expect("solve multi bin");
        assert!(solution.unplaced.is_empty());
        assert_eq!(solution.bin_count, 2);
    }

    #[test]
    fn layer_building_respects_non_xyz_rotation() {
        // Declared (w=10, h=10, d=3) in a (10x3x10) bin. The XYZ rotation
        // has y_extent = 10 > bin.height = 3 and cannot fit; the flattest
        // orientation is `Xzy` which produces extents (10, 3, 10) — a
        // rotation different from identity — and does fit. The test
        // exercises the rotation-lookup path that maps 2D placements
        // back to 3D rotations.
        let problem = ThreeDProblem {
            bins: vec![Bin3D {
                name: "b".into(),
                width: 10,
                height: 3,
                depth: 10,
                cost: 1.0,
                quantity: None,
            }],
            demands: vec![BoxDemand3D {
                name: "a".into(),
                width: 10,
                height: 10,
                depth: 3,
                quantity: 1,
                allowed_rotations: RotationMask3D::ALL,
            }],
        };
        let solution = solve_layer_building(&problem, &ThreeDOptions::default())
            .expect("solve non-xyz rotation");
        assert!(solution.unplaced.is_empty());
        assert_eq!(solution.bin_count, 1);
        let placement = &solution.layouts[0].placements[0];
        assert_ne!(placement.rotation, Rotation3D::Xyz);
        assert_eq!(placement.height, 3, "item should lie flat along the y-axis");
    }

    #[test]
    fn layer_building_chooses_flattest_orientation_that_fits_a_bin() {
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
        let solution = solve_layer_building(&problem, &ThreeDOptions::default())
            .expect("solve bin-feasible rotation");
        assert!(solution.unplaced.is_empty());
        let placement = &solution.layouts[0].placements[0];
        assert_eq!((placement.width, placement.height, placement.depth), (1, 6, 1));
    }

    #[test]
    fn layer_building_variants_emit_expected_algorithm_names() {
        let problem = ThreeDProblem {
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
                width: 3,
                height: 3,
                depth: 3,
                quantity: 4,
                allowed_rotations: RotationMask3D::ALL,
            }],
        };
        type Solver = fn(&ThreeDProblem, &ThreeDOptions) -> Result<ThreeDSolution>;
        let cases: [(&str, Solver); 5] = [
            ("layer_building", solve_layer_building),
            ("layer_building_max_rects", solve_layer_building_max_rects),
            ("layer_building_skyline", solve_layer_building_skyline),
            ("layer_building_guillotine", solve_layer_building_guillotine),
            ("layer_building_shelf", solve_layer_building_shelf),
        ];
        for (expected_name, solver) in cases {
            let solution =
                solver(&problem, &ThreeDOptions::default()).expect("solve layer variant");
            assert_eq!(solution.algorithm, expected_name);
            assert!(solution.unplaced.is_empty(), "{expected_name}");
            assert!(solution.bin_count >= 1, "{expected_name}");
        }
        // Keep the dispatch-aware enum honest by touching every variant.
        let _ = ThreeDAlgorithm::LayerBuilding;
        let _ = ThreeDAlgorithm::LayerBuildingMaxRects;
        let _ = ThreeDAlgorithm::LayerBuildingSkyline;
        let _ = ThreeDAlgorithm::LayerBuildingGuillotine;
        let _ = ThreeDAlgorithm::LayerBuildingShelf;
    }

    #[test]
    fn layer_building_guillotine_sets_guillotine_flag() {
        let problem = ThreeDProblem {
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
                width: 3,
                height: 3,
                depth: 3,
                quantity: 2,
                allowed_rotations: RotationMask3D::ALL,
            }],
        };
        let solution = solve_layer_building_guillotine(&problem, &ThreeDOptions::default())
            .expect("solve guillotine");
        assert!(solution.guillotine);
        assert_eq!(solution.algorithm, "layer_building_guillotine");

        let not_guillotine = solve_layer_building(&problem, &ThreeDOptions::default())
            .expect("solve non-guillotine");
        assert!(!not_guillotine.guillotine);
    }
}
