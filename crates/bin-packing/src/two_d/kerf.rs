//! Kerf-area accounting for finished 2D layouts.
//!
//! Given a sheet and its placements, computes the total area consumed by
//! kerf lines. Algorithms produce placements that satisfy the edge-gap
//! constraint (placements are separated by at least `kerf` along every
//! shared edge), so the accounting scans gaps between adjacent placements
//! and sums `kerf * covered_length` for each distinct kerf band, clipped
//! to the sheet.

use super::model::{Placement2D, Sheet2D};

/// Total area lost to kerf lines on a single sheet.
///
/// The accounting rule (D7, "cut-line model"): for each distinct kerf band
/// — a strip of width `kerf` between two groups of placements separated
/// by exactly `kerf` along one axis — the covered length along the
/// perpendicular axis is the intersection of the convex hulls of the two
/// groups' perpendicular spans. Summing `kerf * covered_length` across all
/// bands gives the kerf area. Boundary edges against the sheet itself do
/// not consume kerf (the factory edge is not a cut).
///
/// Returns `0` when `sheet.kerf == 0` or when there are fewer than two
/// placements on the sheet. Complexity: O(n²) in the placement count,
/// which is fine for realistic sheet occupancies (tens to low hundreds).
pub(crate) fn kerf_area_for_layout(sheet: &Sheet2D, placements: &[Placement2D]) -> u64 {
    let kerf = sheet.kerf;
    if kerf == 0 || placements.len() < 2 {
        return 0;
    }

    let kerf_u64 = u64::from(kerf);

    // For each distinct kerf band we track the convex hull of perpendicular
    // spans for the pieces on each side of the gap.  The key is the gap's
    // start coordinate; the value is (lo_a, hi_a, lo_b, hi_b) where
    // [lo_a, hi_a) is the convex hull of spans for the "earlier" side and
    // [lo_b, hi_b) for the "later" side.
    //
    // "Earlier" = the piece(s) whose edge is at `gap_start`; "later" = the
    // piece(s) whose edge is at `gap_start + kerf`.

    // (gap_start) → (a_lo, a_hi, b_lo, b_hi)
    let mut h_bands: std::collections::BTreeMap<u32, (u32, u32, u32, u32)> =
        std::collections::BTreeMap::new();
    let mut v_bands: std::collections::BTreeMap<u32, (u32, u32, u32, u32)> =
        std::collections::BTreeMap::new();

    for (i, a) in placements.iter().enumerate() {
        let a_right = a.x.saturating_add(a.width);
        let a_bottom = a.y.saturating_add(a.height);
        for b in &placements[i + 1..] {
            let b_right = b.x.saturating_add(b.width);
            let b_bottom = b.y.saturating_add(b.height);

            // Vertical kerf band: a is left of b or vice versa.
            if let Some((gap_x, a_is_left)) = gap_start(a.x, a_right, b.x, b_right, kerf) {
                // Perpendicular spans are in y.
                let (left_y_lo, left_y_hi, right_y_lo, right_y_hi) = if a_is_left {
                    (a.y, a_bottom, b.y, b_bottom)
                } else {
                    (b.y, b_bottom, a.y, a_bottom)
                };
                v_bands
                    .entry(gap_x)
                    .and_modify(|(alo, ahi, blo, bhi)| {
                        *alo = (*alo).min(left_y_lo);
                        *ahi = (*ahi).max(left_y_hi);
                        *blo = (*blo).min(right_y_lo);
                        *bhi = (*bhi).max(right_y_hi);
                    })
                    .or_insert((left_y_lo, left_y_hi, right_y_lo, right_y_hi));
            }

            // Horizontal kerf band: a is above b or vice versa.
            if let Some((gap_y, a_is_top)) = gap_start(a.y, a_bottom, b.y, b_bottom, kerf) {
                // Perpendicular spans are in x.
                let (top_x_lo, top_x_hi, bot_x_lo, bot_x_hi) = if a_is_top {
                    (a.x, a_right, b.x, b_right)
                } else {
                    (b.x, b_right, a.x, a_right)
                };
                h_bands
                    .entry(gap_y)
                    .and_modify(|(alo, ahi, blo, bhi)| {
                        *alo = (*alo).min(top_x_lo);
                        *ahi = (*ahi).max(top_x_hi);
                        *blo = (*blo).min(bot_x_lo);
                        *bhi = (*bhi).max(bot_x_hi);
                    })
                    .or_insert((top_x_lo, top_x_hi, bot_x_lo, bot_x_hi));
            }
        }
    }

    let mut total: u64 = 0;
    for (a_lo, a_hi, b_lo, b_hi) in h_bands.values().chain(v_bands.values()) {
        let covered = hull_intersection(*a_lo, *a_hi, *b_lo, *b_hi);
        total = total.saturating_add(kerf_u64.saturating_mul(covered));
    }

    // Clip to sheet area as a defensive cap.
    let sheet_area = u64::from(sheet.width).saturating_mul(u64::from(sheet.height));
    total.min(sheet_area)
}

/// Returns `Some((gap_lo, a_is_earlier))` when one interval ends exactly
/// `kerf` units before the other begins. `a_is_earlier` is `true` when `a`
/// is the "earlier" side (its hi edge is at `gap_lo`). Returns `None`
/// otherwise.
fn gap_start(a_lo: u32, a_hi: u32, b_lo: u32, b_hi: u32, kerf: u32) -> Option<(u32, bool)> {
    if a_hi <= b_lo && b_lo.saturating_sub(a_hi) == kerf {
        return Some((a_hi, true));
    }
    if b_hi <= a_lo && a_lo.saturating_sub(b_hi) == kerf {
        return Some((b_hi, false));
    }
    None
}

/// Length of the intersection of the convex hulls `[a_lo, a_hi)` and
/// `[b_lo, b_hi)`. Zero if the hulls do not overlap.
fn hull_intersection(a_lo: u32, a_hi: u32, b_lo: u32, b_hi: u32) -> u64 {
    let lo = a_lo.max(b_lo);
    let hi = a_hi.min(b_hi);
    if hi > lo { u64::from(hi - lo) } else { 0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sheet(width: u32, height: u32, kerf: u32) -> Sheet2D {
        Sheet2D {
            name: "s".to_string(),
            width,
            height,
            cost: 1.0,
            quantity: None,
            kerf,
            edge_kerf_relief: false,
        }
    }

    fn place(x: u32, y: u32, width: u32, height: u32) -> Placement2D {
        Placement2D { name: "p".to_string(), x, y, width, height, rotated: false }
    }

    #[test]
    fn zero_kerf_has_zero_area() {
        let s = sheet(10, 10, 0);
        let placements = vec![place(0, 0, 5, 10), place(5, 0, 5, 10)];
        assert_eq!(kerf_area_for_layout(&s, &placements), 0);
    }

    #[test]
    fn single_placement_has_zero_kerf() {
        let s = sheet(10, 10, 2);
        let placements = vec![place(0, 0, 10, 10)];
        assert_eq!(kerf_area_for_layout(&s, &placements), 0);
    }

    #[test]
    fn horizontally_adjacent_pair_consumes_one_kerf_line() {
        // Two 4x10 placements with a kerf=2 gap between them in a 10x10
        // sheet. Left placement at x=0..4, right placement at x=6..10.
        // Kerf rectangle spans x=4..6, y=0..10 → area 2 * 10 = 20.
        let s = sheet(10, 10, 2);
        let placements = vec![place(0, 0, 4, 10), place(6, 0, 4, 10)];
        assert_eq!(kerf_area_for_layout(&s, &placements), 20);
    }

    #[test]
    fn vertically_adjacent_pair_consumes_one_kerf_line() {
        // Top 10x4, bottom 10x4 with a kerf=1 gap in a 10x10 sheet.
        let s = sheet(10, 10, 1);
        let placements = vec![place(0, 0, 10, 4), place(0, 5, 10, 4)];
        assert_eq!(kerf_area_for_layout(&s, &placements), 10);
    }

    #[test]
    fn partial_overlap_uses_overlap_length_not_full_edge() {
        // Left placement 4x10 at (0,0). Right placement 4x6 at (6, 2).
        // Shared x-gap is at x=4..6. Overlap along y: max(0,2)..min(10,8) = 2..8 → 6.
        // Kerf area = 2 * 6 = 12.
        let s = sheet(10, 10, 2);
        let placements = vec![place(0, 0, 4, 10), place(6, 2, 4, 6)];
        assert_eq!(kerf_area_for_layout(&s, &placements), 12);
    }

    #[test]
    fn non_adjacent_pair_contributes_nothing() {
        // Two placements with a larger-than-kerf gap. No shared edge, so
        // no kerf line.
        let s = sheet(20, 10, 2);
        let placements = vec![place(0, 0, 4, 10), place(10, 0, 4, 10)];
        assert_eq!(kerf_area_for_layout(&s, &placements), 0);
    }

    #[test]
    fn boundary_flush_placement_does_not_count_against_sheet_edge() {
        // A single placement flush against the left edge contributes no
        // kerf (D3: factory edge is not a cut).
        let s = sheet(10, 10, 2);
        let placements = vec![place(0, 0, 4, 10)];
        assert_eq!(kerf_area_for_layout(&s, &placements), 0);
    }

    #[test]
    fn three_placements_share_two_kerf_lines() {
        // Three 2x10 placements at x=0, x=4, x=8 with kerf=2 in a 10x10
        // sheet. Two kerf lines (x=2..4 and x=6..8), each 2 * 10 = 20,
        // total 40.
        let s = sheet(10, 10, 2);
        let placements = vec![place(0, 0, 2, 10), place(4, 0, 2, 10), place(8, 0, 2, 10)];
        assert_eq!(kerf_area_for_layout(&s, &placements), 40);
    }

    #[test]
    fn t_intersection_counts_each_shared_edge_once() {
        // T-intersection: one full-width placement on top, two half-width
        // placements below it. Kerf=1 in a 10x10 sheet.
        //
        //   top: x=0..10, y=0..4
        //   bottom_left:  x=0..4, y=5..10
        //   bottom_right: x=5..10, y=5..10
        //
        // Horizontal kerf line at y=4..5: spans full width 10 → area 10.
        // Vertical kerf line at x=4..5, y=5..10: height 5 → area 5.
        // Total = 15.
        let s = sheet(10, 10, 1);
        let placements = vec![place(0, 0, 10, 4), place(0, 5, 4, 5), place(5, 5, 5, 5)];
        assert_eq!(kerf_area_for_layout(&s, &placements), 15);
    }
}
