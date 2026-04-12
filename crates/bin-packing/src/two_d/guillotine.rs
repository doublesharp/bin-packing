use std::cmp::Ordering;

use crate::Result;

use super::model::{
    ItemInstance2D, Placement2D, Rect, SolverMetrics2D, TwoDOptions, TwoDProblem, TwoDSolution,
};

#[derive(Debug, Clone)]
struct SheetState {
    stock_index: usize,
    free_rects: Vec<Rect>,
    placements: Vec<Placement2D>,
}

#[derive(Debug, Clone)]
struct BeamState {
    sheets: Vec<SheetState>,
    usage_counts: Vec<usize>,
    unplaced: Vec<ItemInstance2D>,
    total_waste_area: u64,
    total_cost: f64,
    fragmentation: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SplitAxis {
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, Copy)]
struct Candidate {
    sheet_index: usize,
    stock_or_free_index: usize,
    width: u32,
    height: u32,
    rotated: bool,
    split_axis: SplitAxis,
    waste: u64,
    short_side_fit: u32,
    long_side_fit: u32,
    incremental_cost: f64,
}

#[derive(Debug, Clone, Copy)]
struct PlacementDelta {
    used_area: u64,
    fragmentation_delta: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SplitHeuristic {
    BeamBoth,
    ShorterLeftoverAxis,
    LongerLeftoverAxis,
    MinAreaSplit,
    MaxAreaSplit,
}

// Variant names match the canonical MaxRects-style scoring names from the literature
// (see "A Thousand Ways to Pack the Bin" by Jukka Jylänki). Renaming them to `Area`,
// `ShortSide`, `LongSide` would lose that terminology, so we silence the nitpick lint.
#[allow(clippy::enum_variant_names)]
#[derive(Debug, Clone, Copy)]
enum GuillotineStrategy {
    BestAreaFit,
    BestShortSideFit,
    BestLongSideFit,
}

pub(super) fn solve_guillotine(
    problem: &TwoDProblem,
    options: &TwoDOptions,
) -> Result<TwoDSolution> {
    solve_with_strategy(
        problem,
        options,
        GuillotineStrategy::BestAreaFit,
        SplitHeuristic::BeamBoth,
        "guillotine",
    )
}

pub(super) fn solve_guillotine_bssf(
    problem: &TwoDProblem,
    options: &TwoDOptions,
) -> Result<TwoDSolution> {
    solve_with_strategy(
        problem,
        options,
        GuillotineStrategy::BestShortSideFit,
        SplitHeuristic::BeamBoth,
        "guillotine_best_short_side_fit",
    )
}

pub(super) fn solve_guillotine_blsf(
    problem: &TwoDProblem,
    options: &TwoDOptions,
) -> Result<TwoDSolution> {
    solve_with_strategy(
        problem,
        options,
        GuillotineStrategy::BestLongSideFit,
        SplitHeuristic::BeamBoth,
        "guillotine_best_long_side_fit",
    )
}

pub(super) fn solve_guillotine_slas(
    problem: &TwoDProblem,
    options: &TwoDOptions,
) -> Result<TwoDSolution> {
    solve_with_strategy(
        problem,
        options,
        GuillotineStrategy::BestAreaFit,
        SplitHeuristic::ShorterLeftoverAxis,
        "guillotine_shorter_leftover_axis",
    )
}

pub(super) fn solve_guillotine_llas(
    problem: &TwoDProblem,
    options: &TwoDOptions,
) -> Result<TwoDSolution> {
    solve_with_strategy(
        problem,
        options,
        GuillotineStrategy::BestAreaFit,
        SplitHeuristic::LongerLeftoverAxis,
        "guillotine_longer_leftover_axis",
    )
}

pub(super) fn solve_guillotine_min_area_split(
    problem: &TwoDProblem,
    options: &TwoDOptions,
) -> Result<TwoDSolution> {
    solve_with_strategy(
        problem,
        options,
        GuillotineStrategy::BestAreaFit,
        SplitHeuristic::MinAreaSplit,
        "guillotine_min_area_split",
    )
}

pub(super) fn solve_guillotine_max_area_split(
    problem: &TwoDProblem,
    options: &TwoDOptions,
) -> Result<TwoDSolution> {
    solve_with_strategy(
        problem,
        options,
        GuillotineStrategy::BestAreaFit,
        SplitHeuristic::MaxAreaSplit,
        "guillotine_max_area_split",
    )
}

fn solve_with_strategy(
    problem: &TwoDProblem,
    options: &TwoDOptions,
    strategy: GuillotineStrategy,
    split_heuristic: SplitHeuristic,
    algorithm: &str,
) -> Result<TwoDSolution> {
    let mut items = problem.expanded_items();
    items.sort_by(|left, right| {
        let left_side = left.width.max(left.height);
        let right_side = right.width.max(right.height);
        right_side.cmp(&left_side).then_with(|| {
            // Widen to u64 — u32 * u32 can overflow at MAX_DIMENSION = 1 << 30.
            let left_area = u64::from(left.width) * u64::from(left.height);
            let right_area = u64::from(right.width) * u64::from(right.height);
            right_area.cmp(&left_area)
        })
    });

    let mut beam = vec![BeamState {
        sheets: Vec::new(),
        usage_counts: vec![0; problem.sheets.len()],
        unplaced: Vec::new(),
        total_waste_area: 0,
        total_cost: 0.0,
        fragmentation: 0,
    }];
    let mut explored_states = 0_usize;
    let iterations = items.len();

    for item in items {
        let mut next_beam = Vec::new();
        for state in &beam {
            explored_states = explored_states.saturating_add(1);
            let candidates = enumerate_candidates(problem, state, &item, strategy, split_heuristic);

            if candidates.is_empty() {
                let mut child = state.clone();
                child.unplaced.push(item.clone());
                next_beam.push(child);
                continue;
            }

            for candidate in candidates {
                let mut child = state.clone();
                let mut candidate = candidate;
                if candidate.sheet_index == child.sheets.len() {
                    let stock = &problem.sheets[candidate.stock_or_free_index];
                    child.sheets.push(SheetState {
                        stock_index: candidate.stock_or_free_index,
                        free_rects: vec![Rect {
                            x: 0,
                            y: 0,
                            width: stock.width,
                            height: stock.height,
                        }],
                        placements: Vec::new(),
                    });
                    child.usage_counts[candidate.stock_or_free_index] =
                        child.usage_counts[candidate.stock_or_free_index].saturating_add(1);
                    let stock_area = u64::from(stock.width) * u64::from(stock.height);
                    child.total_waste_area = child.total_waste_area.saturating_add(stock_area);
                    child.total_cost += stock.cost;
                    child.fragmentation = child.fragmentation.saturating_add(1);
                    candidate.stock_or_free_index = 0;
                }

                let delta =
                    place_candidate(&mut child.sheets[candidate.sheet_index], &item, candidate);
                child.total_waste_area = child.total_waste_area.saturating_sub(delta.used_area);
                if delta.fragmentation_delta >= 0 {
                    child.fragmentation =
                        child.fragmentation.saturating_add(delta.fragmentation_delta as u64);
                } else {
                    child.fragmentation =
                        child.fragmentation.saturating_sub((-delta.fragmentation_delta) as u64);
                }
                next_beam.push(child);
            }
        }

        next_beam.sort_by(compare_states);
        next_beam.truncate(options.beam_width.max(1));
        beam = next_beam;
    }

    let best = match beam.into_iter().min_by(compare_states) {
        Some(best) => best,
        None => {
            // Beam is seeded non-empty and every iteration pushes at least one
            // child per state, so this branch is unreachable in practice.
            debug_assert!(false, "beam should not be empty");
            return Ok(TwoDSolution::from_layouts(
                algorithm,
                true,
                &problem.sheets,
                Vec::new(),
                Vec::new(),
                SolverMetrics2D {
                    iterations,
                    explored_states,
                    notes: vec!["beam search produced no states".to_string()],
                },
            ));
        }
    };

    let layouts = best
        .sheets
        .into_iter()
        .map(|sheet| (sheet.stock_index, sheet.placements))
        .collect::<Vec<_>>();

    Ok(TwoDSolution::from_layouts(
        algorithm,
        true,
        &problem.sheets,
        layouts,
        best.unplaced,
        SolverMetrics2D {
            iterations,
            explored_states,
            notes: vec![strategy.note(split_heuristic).to_string()],
        },
    ))
}

fn enumerate_candidates(
    problem: &TwoDProblem,
    state: &BeamState,
    item: &ItemInstance2D,
    strategy: GuillotineStrategy,
    split_heuristic: SplitHeuristic,
) -> Vec<Candidate> {
    let mut candidates = Vec::new();

    for (sheet_index, sheet) in state.sheets.iter().enumerate() {
        for (free_index, free_rect) in sheet.free_rects.iter().enumerate() {
            for (width, height, rotated) in item.orientations() {
                if !free_rect.fits(width, height) {
                    continue;
                }

                let waste = free_rect.area() - u64::from(width) * u64::from(height);
                let short_side_fit = free_rect
                    .width
                    .saturating_sub(width)
                    .min(free_rect.height.saturating_sub(height));
                let long_side_fit = free_rect
                    .width
                    .saturating_sub(width)
                    .max(free_rect.height.saturating_sub(height));
                push_split_candidates(
                    &mut candidates,
                    free_rect.width,
                    free_rect.height,
                    width,
                    height,
                    split_heuristic,
                    Candidate {
                        sheet_index,
                        stock_or_free_index: free_index,
                        width,
                        height,
                        rotated,
                        split_axis: SplitAxis::Horizontal,
                        waste,
                        short_side_fit,
                        long_side_fit,
                        incremental_cost: 0.0,
                    },
                );
            }
        }
    }

    for (stock_index, stock) in problem.sheets.iter().enumerate() {
        if stock.quantity.map(|quantity| state.usage_counts[stock_index] < quantity).unwrap_or(true)
        {
            for (width, height, rotated) in item.orientations() {
                if stock.width >= width && stock.height >= height {
                    let waste = u64::from(stock.width) * u64::from(stock.height)
                        - u64::from(width) * u64::from(height);
                    let short_side_fit =
                        stock.width.saturating_sub(width).min(stock.height.saturating_sub(height));
                    let long_side_fit =
                        stock.width.saturating_sub(width).max(stock.height.saturating_sub(height));
                    push_split_candidates(
                        &mut candidates,
                        stock.width,
                        stock.height,
                        width,
                        height,
                        split_heuristic,
                        Candidate {
                            sheet_index: state.sheets.len(),
                            stock_or_free_index: stock_index,
                            width,
                            height,
                            rotated,
                            split_axis: SplitAxis::Horizontal,
                            waste,
                            short_side_fit,
                            long_side_fit,
                            incremental_cost: stock.cost,
                        },
                    );
                }
            }
        }
    }

    candidates.sort_by(|left, right| compare_candidates(strategy, left, right));
    candidates.truncate(6);
    candidates
}

fn push_split_candidates(
    candidates: &mut Vec<Candidate>,
    free_width: u32,
    free_height: u32,
    used_width: u32,
    used_height: u32,
    split_heuristic: SplitHeuristic,
    base: Candidate,
) {
    match split_heuristic {
        SplitHeuristic::BeamBoth => {
            let mut horizontal = base;
            horizontal.split_axis = SplitAxis::Horizontal;
            candidates.push(horizontal);

            let mut vertical = base;
            vertical.split_axis = SplitAxis::Vertical;
            candidates.push(vertical);
        }
        _ => {
            let mut candidate = base;
            candidate.split_axis = preferred_split_axis(
                free_width,
                free_height,
                used_width,
                used_height,
                split_heuristic,
            );
            candidates.push(candidate);
        }
    }
}

fn preferred_split_axis(
    free_width: u32,
    free_height: u32,
    used_width: u32,
    used_height: u32,
    split_heuristic: SplitHeuristic,
) -> SplitAxis {
    let remaining_width = free_width.saturating_sub(used_width);
    let remaining_height = free_height.saturating_sub(used_height);
    match split_heuristic {
        SplitHeuristic::BeamBoth => SplitAxis::Horizontal,
        SplitHeuristic::ShorterLeftoverAxis => {
            if remaining_width <= remaining_height {
                SplitAxis::Vertical
            } else {
                SplitAxis::Horizontal
            }
        }
        SplitHeuristic::LongerLeftoverAxis => {
            if remaining_width <= remaining_height {
                SplitAxis::Horizontal
            } else {
                SplitAxis::Vertical
            }
        }
        SplitHeuristic::MinAreaSplit => {
            let horizontal_max = child_area_max(
                free_width,
                free_height,
                used_width,
                used_height,
                SplitAxis::Horizontal,
            );
            let vertical_max = child_area_max(
                free_width,
                free_height,
                used_width,
                used_height,
                SplitAxis::Vertical,
            );
            if horizontal_max <= vertical_max { SplitAxis::Horizontal } else { SplitAxis::Vertical }
        }
        SplitHeuristic::MaxAreaSplit => {
            let horizontal_max = child_area_max(
                free_width,
                free_height,
                used_width,
                used_height,
                SplitAxis::Horizontal,
            );
            let vertical_max = child_area_max(
                free_width,
                free_height,
                used_width,
                used_height,
                SplitAxis::Vertical,
            );
            if horizontal_max >= vertical_max { SplitAxis::Horizontal } else { SplitAxis::Vertical }
        }
    }
}

fn child_area_max(
    free_width: u32,
    free_height: u32,
    used_width: u32,
    used_height: u32,
    split_axis: SplitAxis,
) -> u64 {
    let (first, second) = match split_axis {
        SplitAxis::Horizontal => (
            u64::from(free_width) * u64::from(free_height.saturating_sub(used_height)),
            u64::from(free_width.saturating_sub(used_width)) * u64::from(used_height),
        ),
        SplitAxis::Vertical => (
            u64::from(free_width.saturating_sub(used_width)) * u64::from(free_height),
            u64::from(used_width) * u64::from(free_height.saturating_sub(used_height)),
        ),
    };
    first.max(second)
}

fn place_candidate(
    sheet: &mut SheetState,
    item: &ItemInstance2D,
    candidate: Candidate,
) -> PlacementDelta {
    let previous_fragmentation = sheet.free_rects.len();
    let free_rect = sheet.free_rects.remove(candidate.stock_or_free_index);
    let placed =
        Rect { x: free_rect.x, y: free_rect.y, width: candidate.width, height: candidate.height };

    sheet.placements.push(Placement2D {
        name: item.name.clone(),
        x: placed.x,
        y: placed.y,
        width: placed.width,
        height: placed.height,
        rotated: candidate.rotated,
    });

    match candidate.split_axis {
        SplitAxis::Horizontal => {
            push_rect(
                &mut sheet.free_rects,
                Rect {
                    x: free_rect.x,
                    y: free_rect.y + candidate.height,
                    width: free_rect.width,
                    height: free_rect.height.saturating_sub(candidate.height),
                },
            );
            push_rect(
                &mut sheet.free_rects,
                Rect {
                    x: free_rect.x + candidate.width,
                    y: free_rect.y,
                    width: free_rect.width.saturating_sub(candidate.width),
                    height: candidate.height,
                },
            );
        }
        SplitAxis::Vertical => {
            push_rect(
                &mut sheet.free_rects,
                Rect {
                    x: free_rect.x + candidate.width,
                    y: free_rect.y,
                    width: free_rect.width.saturating_sub(candidate.width),
                    height: free_rect.height,
                },
            );
            push_rect(
                &mut sheet.free_rects,
                Rect {
                    x: free_rect.x,
                    y: free_rect.y + candidate.height,
                    width: candidate.width,
                    height: free_rect.height.saturating_sub(candidate.height),
                },
            );
        }
    }

    PlacementDelta {
        used_area: placed.area(),
        fragmentation_delta: sheet.free_rects.len() as i64 - previous_fragmentation as i64,
    }
}

fn push_rect(rects: &mut Vec<Rect>, rect: Rect) {
    if rect.width > 0 && rect.height > 0 {
        rects.push(rect);
    }
}

fn compare_states(left: &BeamState, right: &BeamState) -> Ordering {
    left.unplaced
        .len()
        .cmp(&right.unplaced.len())
        .then_with(|| left.sheets.len().cmp(&right.sheets.len()))
        .then_with(|| left.total_waste_area.cmp(&right.total_waste_area))
        .then_with(|| left.total_cost.total_cmp(&right.total_cost))
        .then_with(|| left.fragmentation.cmp(&right.fragmentation))
}

fn compare_candidates(
    strategy: GuillotineStrategy,
    left: &Candidate,
    right: &Candidate,
) -> Ordering {
    strategy
        .compare(
            left.waste,
            left.short_side_fit,
            left.long_side_fit,
            right.waste,
            right.short_side_fit,
            right.long_side_fit,
        )
        .then_with(|| left.incremental_cost.total_cmp(&right.incremental_cost))
        .then_with(|| left.width.max(left.height).cmp(&right.width.max(right.height)))
        .then_with(|| left.sheet_index.cmp(&right.sheet_index))
        .then_with(|| left.stock_or_free_index.cmp(&right.stock_or_free_index))
}

impl GuillotineStrategy {
    fn compare(
        self,
        left_waste: u64,
        left_short_side_fit: u32,
        left_long_side_fit: u32,
        right_waste: u64,
        right_short_side_fit: u32,
        right_long_side_fit: u32,
    ) -> Ordering {
        match self {
            Self::BestAreaFit => left_waste
                .cmp(&right_waste)
                .then_with(|| left_short_side_fit.cmp(&right_short_side_fit))
                .then_with(|| left_long_side_fit.cmp(&right_long_side_fit)),
            Self::BestShortSideFit => left_short_side_fit
                .cmp(&right_short_side_fit)
                .then_with(|| left_long_side_fit.cmp(&right_long_side_fit))
                .then_with(|| left_waste.cmp(&right_waste)),
            Self::BestLongSideFit => left_long_side_fit
                .cmp(&right_long_side_fit)
                .then_with(|| left_short_side_fit.cmp(&right_short_side_fit))
                .then_with(|| left_waste.cmp(&right_waste)),
        }
    }

    fn note(self, split_heuristic: SplitHeuristic) -> &'static str {
        match (self, split_heuristic) {
            (Self::BestAreaFit, SplitHeuristic::BeamBoth) => {
                "beam search over guillotine split decisions"
            }
            (Self::BestShortSideFit, SplitHeuristic::BeamBoth) => {
                "guillotine beam search with best-short-side-fit candidate ranking"
            }
            (Self::BestLongSideFit, SplitHeuristic::BeamBoth) => {
                "guillotine beam search with best-long-side-fit candidate ranking"
            }
            (Self::BestAreaFit, SplitHeuristic::ShorterLeftoverAxis) => {
                "guillotine beam search with shorter-leftover-axis splitting"
            }
            (Self::BestAreaFit, SplitHeuristic::LongerLeftoverAxis) => {
                "guillotine beam search with longer-leftover-axis splitting"
            }
            (Self::BestAreaFit, SplitHeuristic::MinAreaSplit) => {
                "guillotine beam search with minimum-area split selection"
            }
            (Self::BestAreaFit, SplitHeuristic::MaxAreaSplit) => {
                "guillotine beam search with maximum-area split selection"
            }
            (Self::BestShortSideFit, _) => {
                "guillotine beam search with best-short-side-fit candidate ranking"
            }
            (Self::BestLongSideFit, _) => {
                "guillotine beam search with best-long-side-fit candidate ranking"
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::two_d::{RectDemand2D, Sheet2D, TwoDOptions, TwoDProblem};

    use super::{
        SplitAxis, SplitHeuristic, child_area_max, preferred_split_axis, solve_guillotine,
        solve_guillotine_blsf, solve_guillotine_bssf, solve_guillotine_llas,
        solve_guillotine_max_area_split, solve_guillotine_min_area_split, solve_guillotine_slas,
    };

    #[test]
    fn guillotine_beam_search_finds_feasible_layout() {
        let problem = TwoDProblem {
            sheets: vec![Sheet2D {
                name: "sheet".to_string(),
                width: 10,
                height: 10,
                cost: 1.0,
                quantity: None,
            }],
            demands: vec![
                RectDemand2D {
                    name: "A".to_string(),
                    width: 6,
                    height: 4,
                    quantity: 1,
                    can_rotate: false,
                },
                RectDemand2D {
                    name: "B".to_string(),
                    width: 4,
                    height: 6,
                    quantity: 1,
                    can_rotate: false,
                },
            ],
        };

        let solution = solve_guillotine(&problem, &TwoDOptions::default()).expect("pack");
        assert_eq!(solution.sheet_count, 1);
        assert!(solution.unplaced.is_empty());
    }

    #[test]
    fn guillotine_marks_items_unplaced_when_no_candidates_are_available() {
        let problem = TwoDProblem {
            sheets: vec![Sheet2D {
                name: "sheet".to_string(),
                width: 6,
                height: 6,
                cost: 1.0,
                quantity: Some(1),
            }],
            demands: vec![RectDemand2D {
                name: "panel".to_string(),
                width: 6,
                height: 6,
                quantity: 2,
                can_rotate: false,
            }],
        };

        let solution =
            solve_guillotine(&problem, &TwoDOptions { beam_width: 1, ..TwoDOptions::default() })
                .expect("pack");
        assert_eq!(solution.sheet_count, 1);
        assert_eq!(solution.unplaced.len(), 1);
    }

    #[test]
    fn guillotine_variants_are_available() {
        let problem = TwoDProblem {
            sheets: vec![Sheet2D {
                name: "sheet".to_string(),
                width: 10,
                height: 10,
                cost: 1.0,
                quantity: None,
            }],
            demands: vec![
                RectDemand2D {
                    name: "A".to_string(),
                    width: 6,
                    height: 4,
                    quantity: 1,
                    can_rotate: false,
                },
                RectDemand2D {
                    name: "B".to_string(),
                    width: 4,
                    height: 6,
                    quantity: 1,
                    can_rotate: false,
                },
            ],
        };

        let bssf = solve_guillotine_bssf(&problem, &TwoDOptions::default()).expect("bssf");
        let blsf = solve_guillotine_blsf(&problem, &TwoDOptions::default()).expect("blsf");
        let slas = solve_guillotine_slas(&problem, &TwoDOptions::default()).expect("slas");
        let llas = solve_guillotine_llas(&problem, &TwoDOptions::default()).expect("llas");
        let min_area =
            solve_guillotine_min_area_split(&problem, &TwoDOptions::default()).expect("min_area");
        let max_area =
            solve_guillotine_max_area_split(&problem, &TwoDOptions::default()).expect("max_area");
        assert_eq!(bssf.algorithm, "guillotine_best_short_side_fit");
        assert_eq!(blsf.algorithm, "guillotine_best_long_side_fit");
        assert_eq!(slas.algorithm, "guillotine_shorter_leftover_axis");
        assert_eq!(llas.algorithm, "guillotine_longer_leftover_axis");
        assert_eq!(min_area.algorithm, "guillotine_min_area_split");
        assert_eq!(max_area.algorithm, "guillotine_max_area_split");
        assert!(bssf.unplaced.is_empty());
        assert!(blsf.unplaced.is_empty());
        assert!(slas.unplaced.is_empty());
        assert!(llas.unplaced.is_empty());
        assert!(min_area.unplaced.is_empty());
        assert!(max_area.unplaced.is_empty());
    }

    /// Place a 4x5 item into a 10x20 free rect and verify each split heuristic
    /// picks the documented axis. With these dimensions every heuristic produces a
    /// distinct decision, so this test catches sign-flip regressions in the rule
    /// table.
    #[test]
    fn preferred_split_axis_matches_jylanki_split_rules() {
        // free 10x20, used 4x5 -> remaining 6 wide, 15 tall
        let (fw, fh, uw, uh) = (10_u32, 20_u32, 4_u32, 5_u32);

        // BeamBoth never asks for a preferred axis (the caller emits both candidates),
        // but the function still has to return *something*; we lock in Horizontal as
        // the documented placeholder so a future change has to update the test.
        assert_eq!(
            preferred_split_axis(fw, fh, uw, uh, SplitHeuristic::BeamBoth),
            SplitAxis::Horizontal,
        );

        // SAS preserves the *shorter* leftover side as a solid slab. The 6-wide
        // residual column is shorter than the 15-tall residual row, so SAS splits
        // vertically (cut runs top-to-bottom, leaving the right slab full-height).
        assert_eq!(
            preferred_split_axis(fw, fh, uw, uh, SplitHeuristic::ShorterLeftoverAxis),
            SplitAxis::Vertical,
        );

        // LAS preserves the *longer* leftover side. Same residuals, opposite choice:
        // splits horizontally so the bottom strip stays full-width (10 wide x 15 tall).
        assert_eq!(
            preferred_split_axis(fw, fh, uw, uh, SplitHeuristic::LongerLeftoverAxis),
            SplitAxis::Horizontal,
        );

        // child_area_max for these dimensions:
        //   Horizontal: max(10*15, 6*5) = max(150, 30) = 150
        //   Vertical:   max(6*20, 4*15) = max(120, 60) = 120
        // MIN picks the axis with the smaller worst-case child -> Vertical (120).
        // MAX picks the axis with the larger  worst-case child -> Horizontal (150).
        assert_eq!(child_area_max(fw, fh, uw, uh, SplitAxis::Horizontal), 150);
        assert_eq!(child_area_max(fw, fh, uw, uh, SplitAxis::Vertical), 120);
        assert_eq!(
            preferred_split_axis(fw, fh, uw, uh, SplitHeuristic::MinAreaSplit),
            SplitAxis::Vertical,
        );
        assert_eq!(
            preferred_split_axis(fw, fh, uw, uh, SplitHeuristic::MaxAreaSplit),
            SplitAxis::Horizontal,
        );
    }

    /// Symmetric case: when the leftover sides are equal, every rule must still
    /// be deterministic. The current convention picks Vertical for SAS (the
    /// `<=` branch) and Horizontal for LAS, and Horizontal for both area rules
    /// because the children are identical and the `<=` / `>=` tie-break selects
    /// the first arm. Locking these in keeps refactors honest.
    #[test]
    fn preferred_split_axis_is_deterministic_on_symmetric_remainders() {
        // free 10x10, used 4x4 -> remaining 6x6, perfectly symmetric.
        let (fw, fh, uw, uh) = (10_u32, 10_u32, 4_u32, 4_u32);

        assert_eq!(
            preferred_split_axis(fw, fh, uw, uh, SplitHeuristic::ShorterLeftoverAxis),
            SplitAxis::Vertical,
        );
        assert_eq!(
            preferred_split_axis(fw, fh, uw, uh, SplitHeuristic::LongerLeftoverAxis),
            SplitAxis::Horizontal,
        );
        assert_eq!(
            preferred_split_axis(fw, fh, uw, uh, SplitHeuristic::MinAreaSplit),
            SplitAxis::Horizontal,
        );
        assert_eq!(
            preferred_split_axis(fw, fh, uw, uh, SplitHeuristic::MaxAreaSplit),
            SplitAxis::Horizontal,
        );
    }
}
