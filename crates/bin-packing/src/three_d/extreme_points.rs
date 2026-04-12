//! Extreme Points placement engine with six scoring variants.
//!
//! Implements Crainic, Perboli, and Tadei (2008) "Extreme Point-Based
//! Heuristics for Three-Dimensional Bin Packing" (INFORMS J. Comput. 20(3))
//! generalised to a multi-bin loop. A single shared engine processes the
//! expanded item list, parameterised by an [`ExtremePointsScoring`] enum
//! that selects which of the six candidate rankings to use.
//!
//! The module also exports three helpers ([`BinState`],
//! [`try_place_into_bin`], [`solve_with_scoring_on_items`]) that later
//! tasks (sorted FFD/BFD by volume, multi-start, GRASP, local search)
//! consume to drive the same placement logic from different outer loops.

use std::collections::BTreeSet;

use super::common::{placement_feasible, surface_area_u64, volume_u64};
use super::model::{
    Bin3D, BinLayout3D, BoxDemand3D, ItemInstance3D, MAX_BIN_COUNT_3D, Placement3D, Rotation3D,
    SolverMetrics3D, ThreeDOptions, ThreeDProblem, ThreeDSolution,
};
use crate::{BinPackingError, Result};

/// Scoring rules that drive the Extreme Points candidate ranking. All rules
/// are normalised to "lower is better" at storage time — variants whose
/// natural form is "higher is better" (e.g. [`Self::ContactPoint`]) are
/// stored as `u64::MAX - raw` so the comparator stays uniform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ExtremePointsScoring {
    /// Volume-fit residual: `bin_volume - placed_volume - item_volume`.
    /// The default EP scoring rule.
    VolumeFitResidual,
    /// Residual-space: `min(gap_x, gap_y, gap_z)` at the candidate anchor.
    ResidualSpace,
    /// Free-volume: `gap_x * gap_y * gap_z`, the anchored residual cuboid
    /// approximation from CPT08 "FV".
    FreeVolume,
    /// Bottom-left-back: lexicographic `(ep.y, ep.x, ep.z)` tuple.
    BottomLeftBack,
    /// Contact-point: negated sum of wall and neighbour contact area, with
    /// the neighbour contribution computed as the *union* of the face
    /// overlaps (not the per-neighbour sum).
    ContactPoint,
    /// Euclidean: `ep.x^2 + ep.y^2 + ep.z^2`, widened to `u64` before
    /// multiplication.
    Euclidean,
}

/// Snapshot of one open bin during Extreme Points placement.
///
/// Extreme points are stored as `(z, y, x)` tuples inside a [`BTreeSet`] so
/// that the natural sorted order iterates "deepest, bottom, leftmost" for
/// free — which is exactly what the `BottomLeftBack` scoring rule wants as
/// its ordering key.
pub(super) struct BinState {
    /// Index into `problem.bins` of the bin type this state occupies.
    pub(super) bin_index: usize,
    /// Boxes placed into the bin so far, in placement order.
    pub(super) placements: Vec<Placement3D>,
    /// Current set of anchor points, stored as `(z, y, x)` tuples.
    pub(super) eps: BTreeSet<(u32, u32, u32)>,
    /// Total number of extreme points ever generated inside this bin. Used
    /// by the multi-bin loop to accumulate `metrics.extreme_points_generated`.
    pub(super) extreme_points_generated: usize,
}

impl BinState {
    /// Construct a fresh bin state seeded with the `(0, 0, 0)` anchor.
    pub(super) fn new(bin_index: usize) -> Self {
        let mut eps = BTreeSet::new();
        eps.insert((0, 0, 0));
        Self { bin_index, placements: Vec::new(), eps, extreme_points_generated: 1 }
    }
}

/// Try to place a single item into `state` under `scoring`.
///
/// Iterates every feasible `(rotation, extreme_point)` pair, ranks them
/// under `scoring`, and installs the best placement in
/// `state.placements` / `state.eps` (regenerating the extreme points via
/// the projection rule). Returns the installed [`Placement3D`] on success
/// and `None` if no `(rotation, extreme_point)` triple is feasible.
pub(super) fn try_place_into_bin(
    state: &mut BinState,
    bin: &Bin3D,
    item: &ItemInstance3D,
    scoring: ExtremePointsScoring,
) -> Option<Placement3D> {
    let placed_volume: u64 = state
        .placements
        .iter()
        .map(|placement| volume_u64(placement.width, placement.height, placement.depth))
        .sum();
    let bin_volume = volume_u64(bin.width, bin.height, bin.depth);

    // Snapshot the eps set into a Vec so we can iterate while we defer the
    // mutation to after selection.
    let ep_snapshot: Vec<(u32, u32, u32)> = state.eps.iter().copied().collect();

    // (score, tiebreak, rotation, ep_x, ep_y, ep_z, w, h, d)
    let mut best: Option<Candidate> = None;
    for (rotation, w, h, d) in item.orientations() {
        for &(ep_z, ep_y, ep_x) in &ep_snapshot {
            if !placement_feasible(
                ep_x,
                ep_y,
                ep_z,
                w,
                h,
                d,
                bin.width,
                bin.height,
                bin.depth,
                &state.placements,
            ) {
                continue;
            }
            let score = score_candidate(
                scoring,
                bin,
                &state.placements,
                placed_volume,
                bin_volume,
                ep_x,
                ep_y,
                ep_z,
                w,
                h,
                d,
            );
            let tiebreak = (ep_y, ep_x, ep_z, rotation_ord(rotation), item.demand_index);
            let candidate = Candidate { score, tiebreak, rotation, ep_x, ep_y, ep_z, w, h, d };
            best = match best {
                None => Some(candidate),
                Some(current) => {
                    if candidate.is_better_than(&current) {
                        Some(candidate)
                    } else {
                        Some(current)
                    }
                }
            };
        }
    }

    let chosen = best?;
    let placement = Placement3D {
        name: item.name.clone(),
        x: chosen.ep_x,
        y: chosen.ep_y,
        z: chosen.ep_z,
        width: chosen.w,
        height: chosen.h,
        depth: chosen.d,
        rotation: chosen.rotation,
    };
    state.eps.remove(&(chosen.ep_z, chosen.ep_y, chosen.ep_x));
    let generated = project_extreme_points(&placement, &state.placements, bin, &mut state.eps);
    state.extreme_points_generated = state.extreme_points_generated.saturating_add(generated);
    state.placements.push(placement.clone());
    Some(placement)
}

/// Run the Extreme Points placement engine against a *pre-ordered* item
/// list. No internal sort is applied, so callers (multi-start, GRASP,
/// local search) can seed the order externally.
///
/// `name` is written verbatim into `ThreeDSolution::algorithm`.
///
/// # Errors
///
/// Returns [`BinPackingError::Unsupported`] when the multi-bin loop would
/// need to open more than [`MAX_BIN_COUNT_3D`] bins before placing the
/// next item.
pub(super) fn solve_with_scoring_on_items(
    problem: &ThreeDProblem,
    items: Vec<ItemInstance3D>,
    _options: &ThreeDOptions,
    scoring: ExtremePointsScoring,
    name: &'static str,
) -> Result<ThreeDSolution> {
    let mut states: Vec<BinState> = Vec::new();
    let mut bin_quantity_used: Vec<usize> = vec![0; problem.bins.len()];
    let mut unplaced: Vec<ItemInstance3D> = Vec::new();
    let mut total_extreme_points_generated: usize = 0;

    for item in items {
        if place_item_multi_bin(
            problem,
            &item,
            &mut states,
            &mut bin_quantity_used,
            scoring,
            &mut total_extreme_points_generated,
        )? {
            continue;
        }
        unplaced.push(item);
    }

    // Assemble the solution inline. `build_solution` will land in a later
    // Phase 2 prerequisite commit; until then each algorithm builds its
    // own layouts and totals.
    let bin_count = states.len();
    if bin_count > MAX_BIN_COUNT_3D {
        return Err(BinPackingError::InvalidInput(format!(
            "solution would consume {bin_count} bins, exceeding the supported maximum of {MAX_BIN_COUNT_3D}"
        )));
    }

    let mut layouts: Vec<BinLayout3D> = states
        .into_iter()
        .map(|state| {
            let bin = &problem.bins[state.bin_index];
            let used_volume: u64 = state
                .placements
                .iter()
                .map(|placement| volume_u64(placement.width, placement.height, placement.depth))
                .sum();
            let bin_volume = volume_u64(bin.width, bin.height, bin.depth);
            debug_assert!(used_volume <= bin_volume, "used > bin volume in `{}`", bin.name);
            BinLayout3D {
                bin_name: bin.name.clone(),
                width: bin.width,
                height: bin.height,
                depth: bin.depth,
                cost: bin.cost,
                placements: state.placements,
                used_volume,
                waste_volume: bin_volume.saturating_sub(used_volume),
            }
        })
        .collect();
    layouts.sort_by(|a, b| {
        b.used_volume.cmp(&a.used_volume).then_with(|| a.bin_name.cmp(&b.bin_name))
    });
    let total_waste_volume: u64 = layouts.iter().map(|layout| layout.waste_volume).sum();
    let total_cost: f64 = layouts.iter().map(|layout| layout.cost).sum();

    let mut unplaced_demands: Vec<BoxDemand3D> = unplaced
        .into_iter()
        .map(|item| BoxDemand3D {
            name: item.name,
            width: item.width,
            height: item.height,
            depth: item.depth,
            quantity: 1,
            allowed_rotations: item.allowed_rotations,
        })
        .collect();
    unplaced_demands.sort_by(|left, right| {
        let left_volume = volume_u64(left.width, left.height, left.depth);
        let right_volume = volume_u64(right.width, right.height, right.depth);
        right_volume.cmp(&left_volume)
    });

    let metrics = SolverMetrics3D {
        iterations: 1,
        explored_states: 0,
        extreme_points_generated: total_extreme_points_generated,
        branch_and_bound_nodes: 0,
        notes: vec![format!("{name}: {bin_count} bin(s), {} unplaced", unplaced_demands.len())],
    };

    Ok(ThreeDSolution {
        algorithm: name.to_string(),
        exact: false,
        lower_bound: None,
        guillotine: false,
        bin_count: layouts.len(),
        total_waste_volume,
        total_cost,
        layouts,
        bin_requirements: Vec::new(),
        unplaced: unplaced_demands,
        metrics,
    })
}

/// Solve the problem using Extreme Points with volume-fit residual scoring.
///
/// This is the default EP entry point and is used by `ThreeDAlgorithm::ExtremePoints`.
///
/// # Errors
///
/// Propagates [`BinPackingError::Unsupported`] when the multi-bin loop
/// would exceed [`MAX_BIN_COUNT_3D`].
pub(super) fn solve_extreme_points(
    problem: &ThreeDProblem,
    options: &ThreeDOptions,
) -> Result<ThreeDSolution> {
    run_variant(problem, options, ExtremePointsScoring::VolumeFitResidual, "extreme_points")
}

/// Solve with the residual-space EP scoring variant.
///
/// # Errors
///
/// See [`solve_extreme_points`].
pub(super) fn solve_extreme_points_residual_space(
    problem: &ThreeDProblem,
    options: &ThreeDOptions,
) -> Result<ThreeDSolution> {
    run_variant(
        problem,
        options,
        ExtremePointsScoring::ResidualSpace,
        "extreme_points_residual_space",
    )
}

/// Solve with the free-volume EP scoring variant (CPT08 "FV").
///
/// # Errors
///
/// See [`solve_extreme_points`].
pub(super) fn solve_extreme_points_free_volume(
    problem: &ThreeDProblem,
    options: &ThreeDOptions,
) -> Result<ThreeDSolution> {
    run_variant(problem, options, ExtremePointsScoring::FreeVolume, "extreme_points_free_volume")
}

/// Solve with the bottom-left-back EP scoring variant.
///
/// # Errors
///
/// See [`solve_extreme_points`].
pub(super) fn solve_extreme_points_bottom_left_back(
    problem: &ThreeDProblem,
    options: &ThreeDOptions,
) -> Result<ThreeDSolution> {
    run_variant(
        problem,
        options,
        ExtremePointsScoring::BottomLeftBack,
        "extreme_points_bottom_left_back",
    )
}

/// Solve with the contact-point EP scoring variant.
///
/// # Errors
///
/// See [`solve_extreme_points`].
pub(super) fn solve_extreme_points_contact_point(
    problem: &ThreeDProblem,
    options: &ThreeDOptions,
) -> Result<ThreeDSolution> {
    run_variant(
        problem,
        options,
        ExtremePointsScoring::ContactPoint,
        "extreme_points_contact_point",
    )
}

/// Solve with the Euclidean EP scoring variant (CPT08 "EU").
///
/// # Errors
///
/// See [`solve_extreme_points`].
pub(super) fn solve_extreme_points_euclidean(
    problem: &ThreeDProblem,
    options: &ThreeDOptions,
) -> Result<ThreeDSolution> {
    run_variant(problem, options, ExtremePointsScoring::Euclidean, "extreme_points_euclidean")
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Sort items by decreasing volume (stable) and delegate to
/// [`solve_with_scoring_on_items`].
fn run_variant(
    problem: &ThreeDProblem,
    options: &ThreeDOptions,
    scoring: ExtremePointsScoring,
    name: &'static str,
) -> Result<ThreeDSolution> {
    let mut items = problem.expanded_items();
    items.sort_by(|a, b| {
        let lhs = volume_u64(a.width, a.height, a.depth);
        let rhs = volume_u64(b.width, b.height, b.depth);
        rhs.cmp(&lhs)
    });
    solve_with_scoring_on_items(problem, items, options, scoring, name)
}

/// Candidate `(rotation, ep)` triple evaluated during placement selection.
#[derive(Debug, Clone, Copy)]
struct Candidate {
    score: u64,
    tiebreak: (u32, u32, u32, u8, usize),
    rotation: Rotation3D,
    ep_x: u32,
    ep_y: u32,
    ep_z: u32,
    w: u32,
    h: u32,
    d: u32,
}

impl Candidate {
    fn is_better_than(&self, other: &Self) -> bool {
        match self.score.cmp(&other.score) {
            std::cmp::Ordering::Less => true,
            std::cmp::Ordering::Greater => false,
            std::cmp::Ordering::Equal => self.tiebreak < other.tiebreak,
        }
    }
}

/// Try to place `item` in any open bin; if none accept it, open a new bin
/// (best-fit by bin volume, honouring `Bin3D.quantity` caps). Returns
/// `Ok(true)` if placement succeeded and `Ok(false)` if every feasible bin
/// type is exhausted for this item.
fn place_item_multi_bin(
    problem: &ThreeDProblem,
    item: &ItemInstance3D,
    states: &mut Vec<BinState>,
    bin_quantity_used: &mut [usize],
    scoring: ExtremePointsScoring,
    total_extreme_points_generated: &mut usize,
) -> Result<bool> {
    // First try every currently-open bin, in the order they were opened.
    for state in states.iter_mut() {
        let bin = &problem.bins[state.bin_index];
        if try_place_into_bin(state, bin, item, scoring).is_some() {
            *total_extreme_points_generated =
                total_extreme_points_generated.saturating_add(state.eps.len());
            return Ok(true);
        }
    }

    // No open bin accepted the item; open a new one. Best-fit by bin
    // volume among the bin types that still have remaining quantity and
    // can physically contain at least one orientation of the item. Ties
    // break on `Bin3D` declaration order.
    //
    // `skip_bin_type` ensures forward progress even if a freshly opened
    // bin (somehow) refuses the item on the empty placement list — we
    // exclude that bin type from the local search on the next iteration.
    let mut skip_bin_type: Vec<bool> = vec![false; problem.bins.len()];
    loop {
        let mut best: Option<(usize, u64)> = None;
        for (bin_index, bin) in problem.bins.iter().enumerate() {
            if skip_bin_type[bin_index] {
                continue;
            }
            if let Some(cap) = bin.quantity
                && bin_quantity_used[bin_index] >= cap
            {
                continue;
            }
            if !bin_contains_some_orientation(bin, item) {
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
        let Some((bin_index, _)) = best else {
            return Ok(false);
        };

        if states.len() >= MAX_BIN_COUNT_3D {
            return Err(BinPackingError::Unsupported(format!(
                "3D bin count cap exceeded: opened {} bins, MAX_BIN_COUNT_3D = {MAX_BIN_COUNT_3D}",
                states.len()
            )));
        }

        let mut state = BinState::new(bin_index);
        *total_extreme_points_generated = total_extreme_points_generated.saturating_add(1);
        let bin = &problem.bins[bin_index];
        if try_place_into_bin(&mut state, bin, item, scoring).is_some() {
            bin_quantity_used[bin_index] = bin_quantity_used[bin_index].saturating_add(1);
            *total_extreme_points_generated =
                total_extreme_points_generated.saturating_add(state.eps.len());
            states.push(state);
            return Ok(true);
        }
        // Defensive: `bin_contains_some_orientation` already vetted that
        // *some* rotation fits this bin in isolation, so a fresh bin with
        // an empty placement list must accept the item. If it somehow
        // doesn't (e.g. a disallowed rotation), skip this bin type on
        // subsequent iterations so we make progress towards `unplaced`.
        skip_bin_type[bin_index] = true;
    }
}

/// Whether `bin` is large enough to hold `item` in at least one allowed rotation.
fn bin_contains_some_orientation(bin: &Bin3D, item: &ItemInstance3D) -> bool {
    item.orientations().any(|(_, w, h, d)| w <= bin.width && h <= bin.height && d <= bin.depth)
}

fn rotation_ord(rotation: Rotation3D) -> u8 {
    match rotation {
        Rotation3D::Xyz => 0,
        Rotation3D::Xzy => 1,
        Rotation3D::Yxz => 2,
        Rotation3D::Yzx => 3,
        Rotation3D::Zxy => 4,
        Rotation3D::Zyx => 5,
    }
}

/// Score a candidate placement under `scoring`. All results are normalised
/// to "lower is better".
#[allow(clippy::too_many_arguments)]
fn score_candidate(
    scoring: ExtremePointsScoring,
    bin: &Bin3D,
    placements: &[Placement3D],
    placed_volume: u64,
    bin_volume: u64,
    ep_x: u32,
    ep_y: u32,
    ep_z: u32,
    w: u32,
    h: u32,
    d: u32,
) -> u64 {
    match scoring {
        ExtremePointsScoring::VolumeFitResidual => {
            let item_volume = volume_u64(w, h, d);
            bin_volume.saturating_sub(placed_volume).saturating_sub(item_volume)
        }
        ExtremePointsScoring::ResidualSpace => {
            let gap_x = bin.width - ep_x - w;
            let gap_y = bin.height - ep_y - h;
            let gap_z = bin.depth - ep_z - d;
            u64::from(gap_x.min(gap_y).min(gap_z))
        }
        ExtremePointsScoring::FreeVolume => {
            let gap_x = bin.width - ep_x - w;
            let gap_y = bin.height - ep_y - h;
            let gap_z = bin.depth - ep_z - d;
            volume_u64(gap_x, gap_y, gap_z)
        }
        ExtremePointsScoring::BottomLeftBack => {
            // Encode `(ep_y, ep_x, ep_z)` into a single `u64`. Each axis
            // is bounded by `MAX_DIMENSION_3D = 1 << 15`, so 16 bits per
            // axis suffices.
            (u64::from(ep_y) << 32) | (u64::from(ep_x) << 16) | u64::from(ep_z)
        }
        ExtremePointsScoring::ContactPoint => {
            let contact = contact_point_score(bin, placements, ep_x, ep_y, ep_z, w, h, d);
            u64::MAX - contact
        }
        ExtremePointsScoring::Euclidean => {
            let dx = u64::from(ep_x);
            let dy = u64::from(ep_y);
            let dz = u64::from(ep_z);
            dx.saturating_mul(dx)
                .saturating_add(dy.saturating_mul(dy))
                .saturating_add(dz.saturating_mul(dz))
        }
    }
}

/// Contact-point scoring: wall contact plus the *union* of neighbour-face
/// overlap areas, summed across all six faces of the candidate box.
///
/// The neighbour contribution deliberately uses the union of overlap
/// rectangles (via coordinate compression) rather than a per-neighbour
/// sum, so that a single face touching several neighbours does not
/// double-count the shared regions.
#[allow(clippy::too_many_arguments)]
fn contact_point_score(
    bin: &Bin3D,
    placements: &[Placement3D],
    ep_x: u32,
    ep_y: u32,
    ep_z: u32,
    w: u32,
    h: u32,
    d: u32,
) -> u64 {
    let x_lo = ep_x;
    let x_hi = ep_x + w;
    let y_lo = ep_y;
    let y_hi = ep_y + h;
    let z_lo = ep_z;
    let z_hi = ep_z + d;

    let mut total: u64 = 0;

    // Wall contact (x = 0 and x = bin.width).
    if x_lo == 0 {
        total = total.saturating_add(surface_area_u64(h, d));
    }
    if x_hi == bin.width {
        total = total.saturating_add(surface_area_u64(h, d));
    }
    if y_lo == 0 {
        total = total.saturating_add(surface_area_u64(w, d));
    }
    if y_hi == bin.height {
        total = total.saturating_add(surface_area_u64(w, d));
    }
    if z_lo == 0 {
        total = total.saturating_add(surface_area_u64(w, h));
    }
    if z_hi == bin.depth {
        total = total.saturating_add(surface_area_u64(w, h));
    }

    // Neighbour contact on the six faces. For each face, collect the
    // axis-aligned rectangles where a neighbour's matching face lies flush
    // against the candidate face, then compute the union area.

    // Face at x = x_lo: neighbours with `other.x + other.width == x_lo`.
    let mut rects_x_lo: Vec<(u32, u32, u32, u32)> = Vec::new();
    // Face at x = x_hi: neighbours with `other.x == x_hi`.
    let mut rects_x_hi: Vec<(u32, u32, u32, u32)> = Vec::new();
    // Face at y = y_lo: neighbours with `other.y + other.height == y_lo`.
    let mut rects_y_lo: Vec<(u32, u32, u32, u32)> = Vec::new();
    // Face at y = y_hi: neighbours with `other.y == y_hi`.
    let mut rects_y_hi: Vec<(u32, u32, u32, u32)> = Vec::new();
    // Face at z = z_lo: neighbours with `other.z + other.depth == z_lo`.
    let mut rects_z_lo: Vec<(u32, u32, u32, u32)> = Vec::new();
    // Face at z = z_hi: neighbours with `other.z == z_hi`.
    let mut rects_z_hi: Vec<(u32, u32, u32, u32)> = Vec::new();

    for placement in placements {
        let p_x_lo = placement.x;
        let p_x_hi = placement.x + placement.width;
        let p_y_lo = placement.y;
        let p_y_hi = placement.y + placement.height;
        let p_z_lo = placement.z;
        let p_z_hi = placement.z + placement.depth;

        let yz_cand = ((y_lo, y_hi), (z_lo, z_hi));
        let xz_cand = ((x_lo, x_hi), (z_lo, z_hi));
        let xy_cand = ((x_lo, x_hi), (y_lo, y_hi));
        let yz_other = ((p_y_lo, p_y_hi), (p_z_lo, p_z_hi));
        let xz_other = ((p_x_lo, p_x_hi), (p_z_lo, p_z_hi));
        let xy_other = ((p_x_lo, p_x_hi), (p_y_lo, p_y_hi));

        if p_x_hi == x_lo
            && let Some(rect) = intersect_2d(yz_cand.0, yz_cand.1, yz_other.0, yz_other.1)
        {
            rects_x_lo.push(rect);
        }
        if p_x_lo == x_hi
            && let Some(rect) = intersect_2d(yz_cand.0, yz_cand.1, yz_other.0, yz_other.1)
        {
            rects_x_hi.push(rect);
        }
        if p_y_hi == y_lo
            && let Some(rect) = intersect_2d(xz_cand.0, xz_cand.1, xz_other.0, xz_other.1)
        {
            rects_y_lo.push(rect);
        }
        if p_y_lo == y_hi
            && let Some(rect) = intersect_2d(xz_cand.0, xz_cand.1, xz_other.0, xz_other.1)
        {
            rects_y_hi.push(rect);
        }
        if p_z_hi == z_lo
            && let Some(rect) = intersect_2d(xy_cand.0, xy_cand.1, xy_other.0, xy_other.1)
        {
            rects_z_lo.push(rect);
        }
        if p_z_lo == z_hi
            && let Some(rect) = intersect_2d(xy_cand.0, xy_cand.1, xy_other.0, xy_other.1)
        {
            rects_z_hi.push(rect);
        }
    }

    total = total.saturating_add(union_area(&rects_x_lo));
    total = total.saturating_add(union_area(&rects_x_hi));
    total = total.saturating_add(union_area(&rects_y_lo));
    total = total.saturating_add(union_area(&rects_y_hi));
    total = total.saturating_add(union_area(&rects_z_lo));
    total = total.saturating_add(union_area(&rects_z_hi));

    total
}

/// Intersect two 2D half-open intervals on the face plane. Each interval
/// is `[lo, hi)` on its axis. Returns `(a_lo, a_hi, b_lo, b_hi)` on a
/// non-empty overlap and `None` otherwise.
fn intersect_2d(
    axis_a: (u32, u32),
    axis_b: (u32, u32),
    other_axis_a: (u32, u32),
    other_axis_b: (u32, u32),
) -> Option<(u32, u32, u32, u32)> {
    let la = axis_a.0.max(other_axis_a.0);
    let ha = axis_a.1.min(other_axis_a.1);
    let lb = axis_b.0.max(other_axis_b.0);
    let hb = axis_b.1.min(other_axis_b.1);
    if la < ha && lb < hb { Some((la, ha, lb, hb)) } else { None }
}

/// Compute the union area of a set of axis-aligned rectangles, each
/// specified as `(a_lo, a_hi, b_lo, b_hi)` on a 2D face plane. Uses
/// coordinate compression — `O(n^2)` for `n` input rectangles.
fn union_area(rects: &[(u32, u32, u32, u32)]) -> u64 {
    if rects.is_empty() {
        return 0;
    }
    let mut xs: Vec<u32> = Vec::with_capacity(rects.len() * 2);
    let mut ys: Vec<u32> = Vec::with_capacity(rects.len() * 2);
    for &(lo_a, hi_a, lo_b, hi_b) in rects {
        xs.push(lo_a);
        xs.push(hi_a);
        ys.push(lo_b);
        ys.push(hi_b);
    }
    xs.sort_unstable();
    xs.dedup();
    ys.sort_unstable();
    ys.dedup();

    let mut area: u64 = 0;
    for xi in 0..xs.len().saturating_sub(1) {
        let x0 = xs[xi];
        let x1 = xs[xi + 1];
        let dx = u64::from(x1 - x0);
        for yi in 0..ys.len().saturating_sub(1) {
            let y0 = ys[yi];
            let y1 = ys[yi + 1];
            let covered = rects.iter().any(|&(lo_a, hi_a, lo_b, hi_b)| {
                lo_a <= x0 && x1 <= hi_a && lo_b <= y0 && y1 <= hi_b
            });
            if covered {
                let dy = u64::from(y1 - y0);
                area = area.saturating_add(dx.saturating_mul(dy));
            }
        }
    }
    area
}

/// Project the three raw new corners generated by placing `placement` and
/// insert them into `eps`. Returns the number of extreme points generated
/// (i.e., actually inserted — duplicates are dropped by the `BTreeSet`).
///
/// **Projection rule.** Given the placement's far corner
/// `(px+w, py+h, pz+d)`, the three "raw" corners are
/// `(px+w, py, pz)`, `(px, py+h, pz)`, and `(px, py, pz+d)`. For each
/// corner, the two axes that did *not* advance are projected back by
/// walking over existing placements and taking the maximum of the
/// "far face" coordinate on each projected axis, filtered to those
/// placements whose other two axes bracket the unchanged corner
/// coordinates. `{0}` is always included in each max, handling the case
/// where no placement blocks the projection.
fn project_extreme_points(
    placement: &Placement3D,
    placements: &[Placement3D],
    bin: &Bin3D,
    eps: &mut BTreeSet<(u32, u32, u32)>,
) -> usize {
    let px = placement.x;
    let py = placement.y;
    let pz = placement.z;
    let w = placement.width;
    let h = placement.height;
    let d = placement.depth;

    let mut generated = 0;
    let mut insert_if_in_bin = |eps: &mut BTreeSet<(u32, u32, u32)>, x: u32, y: u32, z: u32| {
        if x < bin.width && y < bin.height && z < bin.depth && eps.insert((z, y, x)) {
            generated += 1;
        }
    };

    // Corner (px + w, py, pz): project down in y, back in z.
    let corner_x = px.saturating_add(w);
    if corner_x < bin.width {
        let mut proj_y: u32 = 0;
        let mut proj_z: u32 = 0;
        for other in placements {
            let o_x_end = other.x.saturating_add(other.width);
            let o_y_end = other.y.saturating_add(other.height);
            let o_z_end = other.z.saturating_add(other.depth);
            // proj_y: max { o_y_end : o_y_end <= py, o.x..o_x_end covers corner_x, o.z..o_z_end covers pz }
            if o_y_end <= py
                && other.x <= corner_x
                && corner_x < o_x_end
                && other.z <= pz
                && pz < o_z_end
                && o_y_end > proj_y
            {
                proj_y = o_y_end;
            }
            if o_z_end <= pz
                && other.x <= corner_x
                && corner_x < o_x_end
                && other.y <= py
                && py < o_y_end
                && o_z_end > proj_z
            {
                proj_z = o_z_end;
            }
        }
        insert_if_in_bin(eps, corner_x, proj_y, proj_z);
    }

    // Corner (px, py + h, pz): project back in x, back in z.
    let corner_y = py.saturating_add(h);
    if corner_y < bin.height {
        let mut proj_x: u32 = 0;
        let mut proj_z: u32 = 0;
        for other in placements {
            let o_x_end = other.x.saturating_add(other.width);
            let o_y_end = other.y.saturating_add(other.height);
            let o_z_end = other.z.saturating_add(other.depth);
            if o_x_end <= px
                && other.y <= corner_y
                && corner_y < o_y_end
                && other.z <= pz
                && pz < o_z_end
                && o_x_end > proj_x
            {
                proj_x = o_x_end;
            }
            if o_z_end <= pz
                && other.y <= corner_y
                && corner_y < o_y_end
                && other.x <= px
                && px < o_x_end
                && o_z_end > proj_z
            {
                proj_z = o_z_end;
            }
        }
        insert_if_in_bin(eps, proj_x, corner_y, proj_z);
    }

    // Corner (px, py, pz + d): project back in x, down in y.
    let corner_z = pz.saturating_add(d);
    if corner_z < bin.depth {
        let mut proj_x: u32 = 0;
        let mut proj_y: u32 = 0;
        for other in placements {
            let o_x_end = other.x.saturating_add(other.width);
            let o_y_end = other.y.saturating_add(other.height);
            let o_z_end = other.z.saturating_add(other.depth);
            if o_x_end <= px
                && other.y <= py
                && py < o_y_end
                && other.z <= corner_z
                && corner_z < o_z_end
                && o_x_end > proj_x
            {
                proj_x = o_x_end;
            }
            if o_y_end <= py
                && other.x <= px
                && px < o_x_end
                && other.z <= corner_z
                && corner_z < o_z_end
                && o_y_end > proj_y
            {
                proj_y = o_y_end;
            }
        }
        insert_if_in_bin(eps, proj_x, proj_y, corner_z);
    }

    generated
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::three_d::model::{Bin3D, BoxDemand3D, RotationMask3D, ThreeDOptions, ThreeDProblem};

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
    fn extreme_points_places_single_box() {
        let solution =
            solve_extreme_points(&problem_one_box(), &ThreeDOptions::default()).expect("solve");
        assert!(solution.unplaced.is_empty());
        assert_eq!(solution.bin_count, 1);
        assert_eq!(solution.algorithm, "extreme_points");
    }

    #[test]
    fn extreme_points_opens_second_bin_when_full() {
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
        let solution = solve_extreme_points(&problem, &ThreeDOptions::default()).expect("solve");
        assert!(solution.unplaced.is_empty());
        assert_eq!(solution.bin_count, 2);
    }

    #[test]
    fn extreme_points_respects_rotation_mask() {
        // 6x1x1 box in a 1x6x1 bin: only rotations that put 6 on the y-axis fit.
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
        let solution = solve_extreme_points(&problem, &ThreeDOptions::default()).expect("solve");
        assert!(solution.unplaced.is_empty());
    }

    #[test]
    fn each_scoring_variant_produces_a_valid_solution() {
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
                quantity: 5,
                allowed_rotations: RotationMask3D::ALL,
            }],
        };
        type Solver = fn(&ThreeDProblem, &ThreeDOptions) -> Result<ThreeDSolution>;
        let cases: [(&str, Solver); 6] = [
            ("extreme_points", solve_extreme_points),
            ("extreme_points_residual_space", solve_extreme_points_residual_space),
            ("extreme_points_free_volume", solve_extreme_points_free_volume),
            ("extreme_points_bottom_left_back", solve_extreme_points_bottom_left_back),
            ("extreme_points_contact_point", solve_extreme_points_contact_point),
            ("extreme_points_euclidean", solve_extreme_points_euclidean),
        ];
        for (name, solver) in cases {
            let solution = solver(&problem, &ThreeDOptions::default()).expect("solve");
            assert_eq!(solution.algorithm, name);
            assert!(solution.unplaced.is_empty(), "{name}");
            assert!(solution.bin_count >= 1, "{name}");
        }
    }

    #[test]
    fn extreme_points_honours_bin_quantity_cap() {
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
        let solution = solve_extreme_points(&problem, &ThreeDOptions::default()).expect("solve");
        assert_eq!(solution.bin_count, 1);
        assert_eq!(solution.unplaced.len(), 1);
    }
}
