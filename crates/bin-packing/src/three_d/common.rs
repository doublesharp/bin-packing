//! Shared placement primitives and geometry helpers for the 3D bin packing
//! solvers.

use super::model::{
    Bin3D, BinLayout3D, BoxDemand3D, ItemInstance3D, MAX_BIN_COUNT_3D, MAX_DIMENSION_3D,
    Placement3D, Rotation3D, SolverMetrics3D, ThreeDSolution,
};

/// An axis-aligned free cuboid inside a bin.
#[allow(dead_code)] // Consumed by Task 6+ placement engines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FreeCuboid3D {
    pub(crate) x: u32,
    pub(crate) y: u32,
    pub(crate) z: u32,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) depth: u32,
}

#[allow(dead_code)] // Consumed by Task 6+ placement engines.
impl FreeCuboid3D {
    pub(crate) fn volume(self) -> u64 {
        volume_u64(self.width, self.height, self.depth)
    }

    pub(crate) fn fits(self, w: u32, h: u32, d: u32) -> bool {
        w <= self.width && h <= self.height && d <= self.depth
    }

    pub(crate) fn intersects(self, other: Self) -> bool {
        let self_x_end = self.x.saturating_add(self.width);
        let self_y_end = self.y.saturating_add(self.height);
        let self_z_end = self.z.saturating_add(self.depth);
        let other_x_end = other.x.saturating_add(other.width);
        let other_y_end = other.y.saturating_add(other.height);
        let other_z_end = other.z.saturating_add(other.depth);

        self.x < other_x_end
            && self_x_end > other.x
            && self.y < other_y_end
            && self_y_end > other.y
            && self.z < other_z_end
            && self_z_end > other.z
    }

    pub(crate) fn contains(self, other: Self) -> bool {
        let self_x_end = self.x.saturating_add(self.width);
        let self_y_end = self.y.saturating_add(self.height);
        let self_z_end = self.z.saturating_add(self.depth);
        let other_x_end = other.x.saturating_add(other.width);
        let other_y_end = other.y.saturating_add(other.height);
        let other_z_end = other.z.saturating_add(other.depth);

        self.x <= other.x
            && self.y <= other.y
            && self.z <= other.z
            && self_x_end >= other_x_end
            && self_y_end >= other_y_end
            && self_z_end >= other_z_end
    }
}

/// Anchor position for the EP family.
#[allow(dead_code)] // Consumed by Task 6 EP placement engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct ExtremePoint3D {
    pub(crate) x: u32,
    pub(crate) y: u32,
    pub(crate) z: u32,
}

/// Whether two placements (in the same bin) overlap.
///
/// Edge-touching (`==` boundary) is allowed; the check uses strict `<`
/// like `crate::two_d::Rect::intersects`.
#[allow(dead_code)] // Consumed by Task 6+ placement engines.
pub(crate) fn placements_overlap(a: &Placement3D, b: &Placement3D) -> bool {
    let a_x_end = a.x.saturating_add(a.width);
    let a_y_end = a.y.saturating_add(a.height);
    let a_z_end = a.z.saturating_add(a.depth);
    let b_x_end = b.x.saturating_add(b.width);
    let b_y_end = b.y.saturating_add(b.height);
    let b_z_end = b.z.saturating_add(b.depth);

    a.x < b_x_end
        && a_x_end > b.x
        && a.y < b_y_end
        && a_y_end > b.y
        && a.z < b_z_end
        && a_z_end > b.z
}

/// Whether a candidate placement at `(x, y, z)` with the given extents fits
/// inside the bin and does not overlap any element of `placed`.
#[allow(dead_code)] // Consumed by Task 6+ placement engines.
#[allow(clippy::too_many_arguments)] // Spec API: caller passes bin extents and candidate extents explicitly.
pub(crate) fn placement_feasible(
    x: u32,
    y: u32,
    z: u32,
    w: u32,
    h: u32,
    d: u32,
    bin_w: u32,
    bin_h: u32,
    bin_d: u32,
    placed: &[Placement3D],
) -> bool {
    if x.saturating_add(w) > bin_w || y.saturating_add(h) > bin_h || z.saturating_add(d) > bin_d {
        return false;
    }
    let candidate = Placement3D {
        name: String::new(),
        x,
        y,
        z,
        width: w,
        height: h,
        depth: d,
        rotation: Rotation3D::Xyz,
    };
    placed.iter().all(|other| !placements_overlap(&candidate, other))
}

/// Returns `Err(BinPackingError::Unsupported(...))` if `current` is at or
/// above [`MAX_BIN_COUNT_3D`]. Solvers must call this *before* opening a
/// new bin so that pathological inputs surface as a structured error
/// instead of running until they exhaust memory.
///
/// The error message has the stable shape
/// `"3D bin count cap exceeded: opened {N} bins, MAX_BIN_COUNT_3D = {cap}"`
/// so callers can substring-match without parsing English.
#[allow(dead_code)] // Consumed by Task 6+ placement engines.
pub(crate) fn check_bin_count_cap(current: usize) -> crate::Result<()> {
    if current >= MAX_BIN_COUNT_3D {
        return Err(crate::BinPackingError::Unsupported(format!(
            "3D bin count cap exceeded: opened {current} bins, MAX_BIN_COUNT_3D = {MAX_BIN_COUNT_3D}"
        )));
    }
    Ok(())
}

// Ranking lives on `ThreeDSolution::is_better_than(&self, &Self) -> bool`
// declared in `model.rs`. This matches the 1D and 2D pattern exactly: the
// comparator takes no `guillotine_required` parameter. The
// `guillotine_required` filter is enforced one tier up by
// `auto.rs::solve_auto_guillotine`, which restricts its candidate set to
// guillotine variants — exactly how `solve_auto_guillotine` works in 2D.

/// Widening volume helper. `debug_assert!` enforces the cap; release builds
/// trust the public validator.
#[allow(dead_code)] // Consumed by Task 6+ placement engines.
pub(crate) fn volume_u64(width: u32, height: u32, depth: u32) -> u64 {
    debug_assert!(width <= MAX_DIMENSION_3D, "width {width} exceeds MAX_DIMENSION_3D");
    debug_assert!(height <= MAX_DIMENSION_3D, "height {height} exceeds MAX_DIMENSION_3D");
    debug_assert!(depth <= MAX_DIMENSION_3D, "depth {depth} exceeds MAX_DIMENSION_3D");
    u64::from(width) * u64::from(height) * u64::from(depth)
}

/// Widening face-area helper used by contact-point scoring.
#[allow(dead_code)] // Consumed by Task 6 EP contact-point variant.
pub(crate) fn surface_area_u64(a: u32, b: u32) -> u64 {
    debug_assert!(a <= MAX_DIMENSION_3D, "a {a} exceeds MAX_DIMENSION_3D");
    debug_assert!(b <= MAX_DIMENSION_3D, "b {b} exceeds MAX_DIMENSION_3D");
    u64::from(a) * u64::from(b)
}

/// Compute used and waste volume for a single layout.
#[allow(dead_code)] // Consumed by Task 6+ solvers when assembling solutions.
pub(crate) fn layout_volume_breakdown(layout: &BinLayout3D) -> (u64, u64) {
    let bin_volume = volume_u64(layout.width, layout.height, layout.depth);
    let used = layout
        .placements
        .iter()
        .map(|placement| volume_u64(placement.width, placement.height, placement.depth))
        .sum::<u64>();
    debug_assert!(used <= bin_volume, "layout used volume exceeds bin volume");
    (used, bin_volume.saturating_sub(used))
}

/// `(bin_index_in_problem.bins, placements)` tuple for [`build_solution`].
#[allow(dead_code)] // Consumed by Task 6+ solvers.
pub(crate) type BinPlacements = (usize, Vec<Placement3D>);

/// Assemble a [`ThreeDSolution`] from a list of per-bin placements and a list
/// of unplaced items.
///
/// Computes `used_volume` / `waste_volume` per layout, sorts layouts by
/// descending utilisation with bin-name tiebreak, and aggregates totals.
/// Sets `exact = false`, `lower_bound = None`, `bin_requirements = empty`,
/// and `guillotine` to the supplied flag. The exact backend and `auto`
/// runner override `exact` / `lower_bound` / `bin_requirements` after the
/// fact.
///
/// # Errors
///
/// Returns [`crate::BinPackingError::Unsupported`] if `bin_placements.len()`
/// exceeds [`MAX_BIN_COUNT_3D`]. The cap is enforced here so every algorithm
/// benefits from a single safety check.
#[allow(dead_code)] // Consumed by Task 6+ solvers.
pub(crate) fn build_solution(
    algorithm: impl Into<String>,
    bins: &[Bin3D],
    bin_placements: Vec<BinPlacements>,
    unplaced: Vec<ItemInstance3D>,
    metrics: SolverMetrics3D,
    guillotine: bool,
) -> crate::Result<ThreeDSolution> {
    if bin_placements.len() > MAX_BIN_COUNT_3D {
        return Err(crate::BinPackingError::Unsupported(format!(
            "3D bin count cap exceeded: solution would consume {} bins, MAX_BIN_COUNT_3D = {MAX_BIN_COUNT_3D}",
            bin_placements.len(),
        )));
    }

    let mut layouts = bin_placements
        .into_iter()
        .map(|(bin_index, placements)| {
            let bin = &bins[bin_index];
            let used_volume = placements
                .iter()
                .map(|placement| volume_u64(placement.width, placement.height, placement.depth))
                .sum::<u64>();
            let bin_volume = volume_u64(bin.width, bin.height, bin.depth);
            debug_assert!(used_volume <= bin_volume, "used > bin volume in `{}`", bin.name);
            BinLayout3D {
                bin_name: bin.name.clone(),
                width: bin.width,
                height: bin.height,
                depth: bin.depth,
                cost: bin.cost,
                placements,
                used_volume,
                waste_volume: bin_volume.saturating_sub(used_volume),
            }
        })
        .collect::<Vec<_>>();

    layouts.sort_by(|a, b| {
        b.used_volume.cmp(&a.used_volume).then_with(|| a.bin_name.cmp(&b.bin_name))
    });

    let bin_count = layouts.len();
    let total_waste_volume = layouts.iter().map(|layout| layout.waste_volume).sum();
    let total_cost = layouts.iter().map(|layout| layout.cost).sum();

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

    Ok(ThreeDSolution {
        algorithm: algorithm.into(),
        exact: false,
        lower_bound: None,
        guillotine,
        bin_count,
        total_waste_volume,
        total_cost,
        layouts,
        bin_requirements: Vec::new(),
        unplaced: unplaced_demands,
        metrics,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::three_d::model::Rotation3D;

    fn placement(x: u32, y: u32, z: u32, w: u32, h: u32, d: u32) -> Placement3D {
        Placement3D {
            name: "x".into(),
            x,
            y,
            z,
            width: w,
            height: h,
            depth: d,
            rotation: Rotation3D::Xyz,
        }
    }

    #[test]
    fn placements_overlap_detects_corner_intersection() {
        let a = placement(0, 0, 0, 2, 2, 2);
        let b = placement(1, 1, 1, 2, 2, 2);
        assert!(placements_overlap(&a, &b));
    }

    #[test]
    fn placements_overlap_allows_edge_touch() {
        let a = placement(0, 0, 0, 2, 2, 2);
        let b = placement(2, 0, 0, 2, 2, 2);
        assert!(!placements_overlap(&a, &b));
    }

    #[test]
    fn placement_feasible_rejects_off_bin() {
        let placed: Vec<Placement3D> = vec![];
        assert!(!placement_feasible(8, 0, 0, 4, 4, 4, 10, 10, 10, &placed));
    }

    #[test]
    fn placement_feasible_rejects_overlap() {
        let placed = vec![placement(0, 0, 0, 5, 5, 5)];
        assert!(!placement_feasible(2, 2, 2, 4, 4, 4, 10, 10, 10, &placed));
    }

    #[test]
    fn placement_feasible_accepts_clear_space() {
        let placed = vec![placement(0, 0, 0, 5, 5, 5)];
        assert!(placement_feasible(5, 0, 0, 4, 4, 4, 10, 10, 10, &placed));
    }

    #[test]
    fn free_cuboid_intersects_and_contains() {
        let a = FreeCuboid3D { x: 0, y: 0, z: 0, width: 10, height: 10, depth: 10 };
        let b = FreeCuboid3D { x: 5, y: 5, z: 5, width: 4, height: 4, depth: 4 };
        assert!(a.intersects(b));
        assert!(a.contains(b));
        assert!(!b.contains(a));
    }

    #[test]
    fn free_cuboid_volume_and_fits_helpers() {
        let cuboid = FreeCuboid3D { x: 0, y: 0, z: 0, width: 3, height: 4, depth: 5 };
        assert_eq!(cuboid.volume(), 60);
        assert!(cuboid.fits(3, 4, 5));
        assert!(cuboid.fits(1, 1, 1));
        assert!(!cuboid.fits(4, 4, 5));
    }

    #[test]
    fn layout_volume_breakdown_reports_used_and_waste() {
        let layout = BinLayout3D {
            bin_name: "b".into(),
            width: 10,
            height: 10,
            depth: 10,
            cost: 1.0,
            placements: vec![placement(0, 0, 0, 5, 5, 5), placement(5, 0, 0, 5, 5, 5)],
            used_volume: 0,
            waste_volume: 0,
        };
        let (used, waste) = layout_volume_breakdown(&layout);
        assert_eq!(used, 250);
        assert_eq!(waste, 750);
    }

    #[test]
    fn surface_area_u64_widens_before_multiplying() {
        assert_eq!(surface_area_u64(1 << 15, 1 << 15), 1u64 << 30);
    }

    #[test]
    fn check_bin_count_cap_accepts_one_below_limit() {
        assert!(check_bin_count_cap(MAX_BIN_COUNT_3D - 1).is_ok());
    }

    #[test]
    fn check_bin_count_cap_rejects_at_limit_with_unsupported() {
        let err = check_bin_count_cap(MAX_BIN_COUNT_3D).expect_err("at-limit");
        let crate::BinPackingError::Unsupported(message) = err else {
            panic!("expected Unsupported, got something else");
        };
        assert!(
            message.contains("3D bin count cap exceeded"),
            "stable error message contract: {message}"
        );
    }

    #[test]
    fn extreme_point_3d_is_constructible() {
        // Smoke test that exercises the anchor type so dead-code lints
        // don't fire in release builds of the module.
        let ep = ExtremePoint3D { x: 1, y: 2, z: 3 };
        assert_eq!((ep.x, ep.y, ep.z), (1, 2, 3));
    }
}
