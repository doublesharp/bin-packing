//! Drop extraction and usable-drop scoring for finished 2D layouts.
//!
//! Given a sheet and its placements, this module computes two metrics
//! over the unused region:
//!
//! - `largest_usable_drop_area`: area of the biggest axis-aligned
//!   rectangle that fits entirely in the unused region and whose sides
//!   both satisfy `>= min_usable_side`. Computed from the maximal free
//!   rectangle (MFR) set; the "largest rectangle that fits."
//! - `sum_sq_usable_drop_areas`: sum of `area²` over a canonical
//!   disjoint rectilinear partition of the unused region, filtered by
//!   `min_usable_side`. Rewards consolidation without a tunable knob:
//!   merging two adjacent drops into one strictly increases the sum.
//!
//! See `docs/superpowers/specs/2026-04-14-waste-consolidation-2d-design.md`
//! for the formal definitions.

use super::model::{Placement2D, Rect, Sheet2D};

/// Compute `(largest_usable_drop_area, sum_sq_usable_drop_areas)` for a
/// finished layout. Filters by `min_usable_side`: any rectangle with
/// either side below the threshold contributes zero.
///
/// `largest_usable_drop_area` is the area of the single largest maximal
/// free rectangle whose width and height both satisfy `>= min_usable_side`.
/// Uses a slab-pair sweep: for every pair of y-band boundaries, find the
/// widest free x-interval among placements that intersect the slab.
///
/// `sum_sq_usable_drop_areas` sums `area²` over a canonical
/// horizontal-strip disjoint partition of the free region, filtering out
/// rectangles with either side `< min_usable_side`. The sum-of-squares
/// metric rewards consolidation: merging two drops into one strictly
/// increases the sum.
///
/// Both computations share the same y-band boundary set derived from
/// placement tops/bottoms plus `0` and `sheet.height`. Complexity is
/// O(p³) in the number of placements p; for typical pack counts (<100
/// placements per sheet) this is sub-millisecond.
pub(crate) fn usable_drop_metrics(
    sheet: &Sheet2D,
    placements: &[Placement2D],
    min_usable_side: u32,
) -> (u64, u128) {
    let clipped = clipped_placements(sheet, placements);
    let ys = y_boundaries(sheet, &clipped);
    let largest = largest_usable_free_rectangle_area(sheet, &clipped, &ys, min_usable_side);

    let partition = disjoint_partition(sheet, &clipped, &ys);
    let sum_sq = partition
        .iter()
        .filter(|r| r.width >= min_usable_side && r.height >= min_usable_side)
        .map(|r| {
            let area = u128::from(r.width) * u128::from(r.height);
            area * area
        })
        .fold(0_u128, u128::saturating_add);

    (largest, sum_sq)
}

#[derive(Debug, Clone, Copy)]
struct ClippedPlacement {
    x_lo: u32,
    x_hi: u32,
    y_lo: u32,
    y_hi: u32,
}

fn clipped_placements(sheet: &Sheet2D, placements: &[Placement2D]) -> Vec<ClippedPlacement> {
    placements
        .iter()
        .filter_map(|placement| {
            let x_lo = placement.x.min(sheet.width);
            let x_hi = placement.x.saturating_add(placement.width).min(sheet.width);
            let y_lo = placement.y.min(sheet.height);
            let y_hi = placement.y.saturating_add(placement.height).min(sheet.height);
            (x_lo < x_hi && y_lo < y_hi).then_some(ClippedPlacement { x_lo, x_hi, y_lo, y_hi })
        })
        .collect()
}

fn y_boundaries(sheet: &Sheet2D, placements: &[ClippedPlacement]) -> Vec<u32> {
    let mut ys: Vec<u32> = Vec::with_capacity(placements.len() * 2 + 2);
    ys.push(0);
    ys.push(sheet.height);
    for placement in placements {
        ys.push(placement.y_lo);
        ys.push(placement.y_hi);
    }
    ys.sort_unstable();
    ys.dedup();
    ys
}

/// Canonical horizontal-strip decomposition of the free region.
///
/// y-band boundaries are derived from `0`, `sheet.height`, and each
/// placement's top and bottom edges. Within each band, placements that
/// fully span the band (i.e., `p.y <= y_lo AND p.y + p.height >= y_hi`)
/// are subtracted to yield disjoint free rectangles. The result is a
/// partition: every point in the free region belongs to exactly one
/// returned rectangle.
fn disjoint_partition(sheet: &Sheet2D, placements: &[ClippedPlacement], ys: &[u32]) -> Vec<Rect> {
    let mut rects = Vec::new();
    for window in ys.windows(2) {
        let y_lo = window[0];
        let y_hi = window[1];
        if y_hi == y_lo {
            continue;
        }
        let band_height = y_hi - y_lo;

        // Collect x-intervals occupied by placements that fully span this band.
        let mut occupied: Vec<(u32, u32)> = placements
            .iter()
            .filter(|p| p.y_lo <= y_lo && p.y_hi >= y_hi)
            .map(|p| (p.x_lo, p.x_hi))
            .collect();
        occupied.sort_unstable();

        // Complement within [0, sheet.width) to get free intervals.
        let mut cursor = 0_u32;
        for (start, end) in occupied {
            if start > cursor {
                rects.push(Rect { x: cursor, y: y_lo, width: start - cursor, height: band_height });
            }
            cursor = cursor.max(end);
        }
        if cursor < sheet.width {
            rects.push(Rect {
                x: cursor,
                y: y_lo,
                width: sheet.width - cursor,
                height: band_height,
            });
        }
    }

    rects
}

/// Area of the largest axis-aligned free rectangle with both sides
/// `>= min_usable_side`.
///
/// Uses a slab-pair sweep over all pairs of y-band boundaries. For each
/// slab `[y_lo, y_hi)`, placements that *intersect* the slab (i.e.,
/// `p.y < y_hi AND p.y + p.height > y_lo`) block their x-interval for any
/// rectangle that spans the full slab height. For a fixed `y_lo`, blockers
/// are added incrementally as `y_hi` advances, avoiding a fresh scan and
/// sort for every slab. Worst-case complexity remains O(p³), but with less
/// repeated allocation and sorting than the direct slab-pair implementation.
fn largest_usable_free_rectangle_area(
    sheet: &Sheet2D,
    placements: &[ClippedPlacement],
    ys: &[u32],
    min_usable_side: u32,
) -> u64 {
    let mut best: u64 = 0;
    for i in 0..ys.len() {
        let y_lo = ys[i];
        let mut candidates: Vec<ClippedPlacement> =
            placements.iter().copied().filter(|p| p.y_hi > y_lo).collect();
        candidates.sort_unstable_by_key(|p| p.y_lo);

        let mut next_candidate = 0_usize;
        let mut occupied: Vec<(u32, u32)> = Vec::new();

        for &y_hi in ys.iter().skip(i + 1) {
            let slab_height = y_hi - y_lo;
            while next_candidate < candidates.len() && candidates[next_candidate].y_lo < y_hi {
                let placement = candidates[next_candidate];
                insert_merged_interval(&mut occupied, placement.x_lo, placement.x_hi);
                next_candidate += 1;
            }
            if slab_height < min_usable_side {
                continue;
            }

            let free_width = widest_free_interval(sheet.width, &occupied);
            if free_width >= min_usable_side {
                let candidate = u64::from(free_width) * u64::from(slab_height);
                if candidate > best {
                    best = candidate;
                }
            }
        }
    }
    best
}

fn insert_merged_interval(intervals: &mut Vec<(u32, u32)>, start: u32, end: u32) {
    if start >= end {
        return;
    }

    let old_intervals = std::mem::take(intervals);
    let mut pending = Some((start, end));

    for (lo, hi) in old_intervals {
        let Some((new_lo, new_hi)) = pending else {
            intervals.push((lo, hi));
            continue;
        };

        if hi < new_lo {
            intervals.push((lo, hi));
        } else if new_hi < lo {
            intervals.push((new_lo, new_hi));
            intervals.push((lo, hi));
            pending = None;
        } else {
            pending = Some((new_lo.min(lo), new_hi.max(hi)));
        }
    }

    if let Some(interval) = pending {
        intervals.push(interval);
    }
}

fn widest_free_interval(sheet_width: u32, occupied: &[(u32, u32)]) -> u32 {
    let mut best = 0_u32;
    let mut cursor = 0_u32;
    for &(start, end) in occupied {
        if start > cursor {
            best = best.max(start - cursor);
        }
        cursor = cursor.max(end);
    }
    if sheet_width > cursor {
        best = best.max(sheet_width - cursor);
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sheet(width: u32, height: u32) -> Sheet2D {
        Sheet2D {
            name: "s".to_string(),
            width,
            height,
            cost: 1.0,
            quantity: None,
            kerf: 0,
            edge_kerf_relief: false,
        }
    }

    fn place(x: u32, y: u32, width: u32, height: u32) -> Placement2D {
        Placement2D { name: "p".to_string(), x, y, width, height, rotated: false }
    }

    #[test]
    fn empty_placements_make_the_whole_sheet_one_drop() {
        // 10×10 sheet, no placements -> single 10×10 drop.
        //   largest = 100
        //   sum_sq  = 10000
        let s = sheet(10, 10);
        let (largest, sum_sq) = usable_drop_metrics(&s, &[], 0);
        assert_eq!(largest, 100);
        assert_eq!(sum_sq, 10_000);
    }

    #[test]
    fn full_sheet_placement_yields_zero_metrics() {
        // A single placement covering the full sheet leaves no drop.
        let s = sheet(10, 10);
        let (largest, sum_sq) = usable_drop_metrics(&s, &[place(0, 0, 10, 10)], 0);
        assert_eq!(largest, 0);
        assert_eq!(sum_sq, 0);
    }

    #[test]
    fn single_corner_placement_yields_l_shape_largest_and_partition_sum() {
        // 10×10 sheet with a 4×4 placement in the top-left corner.
        //
        // Free region is an L-shape. Maximal free rectangles:
        //   top-right strip:   x=4..10, y=0..10  -> 6×10 = 60
        //   bottom-left strip: x=0..10, y=4..10  -> 10×6 = 60
        // Largest MFR area = 60.
        //
        // Horizontal-strip disjoint partition (bands at y=0, 4, 10):
        //   band y=0..4: x free in [4..10] -> 6×4 = 24
        //   band y=4..10: x free in [0..10] -> 10×6 = 60
        // sum_sq = 24² + 60² = 576 + 3600 = 4176.
        let s = sheet(10, 10);
        let (largest, sum_sq) = usable_drop_metrics(&s, &[place(0, 0, 4, 4)], 0);
        assert_eq!(largest, 60);
        assert_eq!(sum_sq, 4_176);
    }

    #[test]
    fn two_placements_produce_multiple_drops() {
        // 10×10 sheet with two 3×3 placements at opposite corners
        // (top-left and bottom-right). Several MFRs; hand-compute the
        // horizontal-strip partition.
        //
        // Placements:
        //   a = (0,0) 3×3
        //   b = (7,7) 3×3
        //
        // y-band boundaries: 0, 3, 7, 10.
        //   band y=0..3: x free in [3..10] -> 7×3 = 21
        //   band y=3..7: x free in [0..10] -> 10×4 = 40
        //   band y=7..10: x free in [0..7]  -> 7×3 = 21
        // sum_sq = 21² + 40² + 21² = 441 + 1600 + 441 = 2482.
        //
        // Largest MFR: the middle band is 10×4 = 40 or there's a 7×7
        // rectangle spanning y=0..7, x=3..10. Check: that rectangle is
        // free (doesn't intersect either placement). 7×7 = 49 > 40.
        // Similarly x=0..7, y=3..10 is 7×7 = 49. Largest = 49.
        let s = sheet(10, 10);
        let placements = vec![place(0, 0, 3, 3), place(7, 7, 3, 3)];
        let (largest, sum_sq) = usable_drop_metrics(&s, &placements, 0);
        assert_eq!(largest, 49);
        assert_eq!(sum_sq, 2_482);
    }

    #[test]
    fn threshold_filters_strips_below_minimum_side() {
        // 20×10 sheet with two 9×10 placements separated by a 2-unit gap.
        //   a = (0,0)  9×10
        //   b = (11,0) 9×10
        // Free region: a 2×10 strip at x=9..11. That strip is the only
        // drop. Width 2 is below min_usable_side=3, so it contributes
        // zero.
        let s = sheet(20, 10);
        let placements = vec![place(0, 0, 9, 10), place(11, 0, 9, 10)];

        let (largest_unfiltered, sum_sq_unfiltered) = usable_drop_metrics(&s, &placements, 0);
        assert_eq!(largest_unfiltered, 20); // 2×10 strip
        assert_eq!(sum_sq_unfiltered, 400); // 20²

        let (largest_filtered, sum_sq_filtered) = usable_drop_metrics(&s, &placements, 3);
        assert_eq!(largest_filtered, 0);
        assert_eq!(sum_sq_filtered, 0);
    }

    #[test]
    fn sum_sq_strictly_increases_when_drops_merge() {
        // Two fragmented layouts covering the same total waste.
        //
        // Use a minimal setup:
        //   Sheet 6×2. Waste scenarios:
        //     A: placement (2,0) 2×2 -> two free strips: 2×2 at x=0..2 (area 4)
        //        and 2×2 at x=4..6 (area 4). sum_sq = 4² + 4² = 32.
        //     B: placement (0,0) 2×2 -> one free 4×2 strip. sum_sq = 8² = 64.
        //   Same waste area (8), merged wins on sum_sq.
        let s = sheet(6, 2);
        let (_, sum_sq_fragmented) = usable_drop_metrics(&s, &[place(2, 0, 2, 2)], 0);
        let (_, sum_sq_merged) = usable_drop_metrics(&s, &[place(0, 0, 2, 2)], 0);
        assert!(
            sum_sq_merged > sum_sq_fragmented,
            "merged layout should have higher sum_sq: merged={sum_sq_merged} fragmented={sum_sq_fragmented}"
        );
    }

    #[test]
    fn largest_drop_bounded_by_waste_area() {
        // Defensive invariant: largest drop must not exceed waste area.
        let s = sheet(10, 10);
        let placements = vec![place(0, 0, 3, 3), place(7, 7, 3, 3)];
        let (largest, _) = usable_drop_metrics(&s, &placements, 0);
        let used_area =
            placements.iter().map(|p| u64::from(p.width) * u64::from(p.height)).sum::<u64>();
        let sheet_area = u64::from(s.width) * u64::from(s.height);
        assert!(largest <= sheet_area - used_area);
    }

    #[test]
    fn sum_sq_bounded_by_waste_area_squared() {
        // Defensive invariant: sum_sq must not exceed waste_area squared.
        let s = sheet(10, 10);
        let placements = vec![place(2, 2, 6, 6)];
        let (_, sum_sq) = usable_drop_metrics(&s, &placements, 0);
        let used_area =
            placements.iter().map(|p| u128::from(p.width) * u128::from(p.height)).sum::<u128>();
        let sheet_area = u128::from(s.width) * u128::from(s.height);
        let waste_area = sheet_area - used_area;
        assert!(sum_sq <= waste_area * waste_area);
    }
}
