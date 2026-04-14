use std::cmp::Ordering;

use rand::{RngCore, SeedableRng, rngs::SmallRng};

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

#[derive(Debug, Clone, Copy)]
struct PlacementCandidate {
    sheet_index: usize,
    free_index: usize,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    rotated: bool,
    area_waste: u64,
    short_side_fit: u32,
    long_side_fit: u32,
    bottom: u32,
    left: u32,
    contact_score: u32,
}

#[derive(Debug, Clone, Copy)]
struct NewSheetCandidate {
    stock_index: usize,
    width: u32,
    height: u32,
    rotated: bool,
    cost: f64,
    area_waste: u64,
    short_side_fit: u32,
    long_side_fit: u32,
    bottom: u32,
    left: u32,
    contact_score: u32,
}

#[derive(Debug, Clone, Copy)]
enum MaxRectsStrategy {
    BestAreaFit,
    BestShortSideFit,
    BestLongSideFit,
    BottomLeft,
    ContactPoint,
}

pub(super) fn solve_maxrects(
    problem: &TwoDProblem,
    _options: &TwoDOptions,
) -> Result<TwoDSolution> {
    solve_with_strategy(problem, MaxRectsStrategy::BestAreaFit, "max_rects")
}

pub(super) fn solve_maxrects_bssf(
    problem: &TwoDProblem,
    _options: &TwoDOptions,
) -> Result<TwoDSolution> {
    solve_with_strategy(
        problem,
        MaxRectsStrategy::BestShortSideFit,
        "max_rects_best_short_side_fit",
    )
}

pub(super) fn solve_maxrects_blsf(
    problem: &TwoDProblem,
    _options: &TwoDOptions,
) -> Result<TwoDSolution> {
    solve_with_strategy(problem, MaxRectsStrategy::BestLongSideFit, "max_rects_best_long_side_fit")
}

pub(super) fn solve_maxrects_bottom_left(
    problem: &TwoDProblem,
    _options: &TwoDOptions,
) -> Result<TwoDSolution> {
    solve_with_strategy(problem, MaxRectsStrategy::BottomLeft, "max_rects_bottom_left")
}

pub(super) fn solve_maxrects_contact_point(
    problem: &TwoDProblem,
    _options: &TwoDOptions,
) -> Result<TwoDSolution> {
    solve_with_strategy(problem, MaxRectsStrategy::ContactPoint, "max_rects_contact_point")
}

pub(super) fn solve_multistart(
    problem: &TwoDProblem,
    options: &TwoDOptions,
) -> Result<TwoDSolution> {
    let mut items = problem.expanded_items();
    sort_items_descending(&mut items);

    let mut best =
        pack_with_order(problem, &items, "multi_start", 1, MaxRectsStrategy::BestAreaFit)?;
    let base_seed = options.seed.unwrap_or(0x4D41_5852_4543_5453);

    let runs = options.multistart_runs.max(1);
    let candidates = crate::parallel::par_map_indexed(runs, |run| {
        let mut rng = SmallRng::seed_from_u64(crate::parallel::iteration_seed(base_seed, run));
        let trial_items = multistart_ordered_items(&items, &mut rng);
        pack_with_order(
            problem,
            &trial_items,
            "multi_start",
            run + 2,
            MaxRectsStrategy::BestAreaFit,
        )
    });

    for candidate in candidates.into_iter().flatten() {
        if candidate.is_better_than(&best) {
            best = candidate;
        }
    }

    best.metrics
        .notes
        .push("multistart randomizes tie ordering among similarly ranked items".to_string());
    best.metrics.notes.push("multistart kept the best candidate".to_string());
    Ok(best)
}

fn solve_with_strategy(
    problem: &TwoDProblem,
    strategy: MaxRectsStrategy,
    algorithm: &str,
) -> Result<TwoDSolution> {
    let mut items = problem.expanded_items();
    sort_items_descending(&mut items);
    pack_with_order(problem, &items, algorithm, 1, strategy)
}

fn multistart_ordered_items(items: &[ItemInstance2D], rng: &mut SmallRng) -> Vec<ItemInstance2D> {
    let mut keyed_items = items
        .iter()
        .cloned()
        .enumerate()
        .map(|(index, item)| (item, rng.next_u64(), index))
        .collect::<Vec<_>>();

    keyed_items.sort_by(|left, right| {
        // Widen to u64 — u32 * u32 can overflow at MAX_DIMENSION = 1 << 30.
        let left_area = u64::from(left.0.width) * u64::from(left.0.height);
        let right_area = u64::from(right.0.width) * u64::from(right.0.height);

        right_area
            .cmp(&left_area)
            .then_with(|| right.0.width.max(right.0.height).cmp(&left.0.width.max(left.0.height)))
            .then_with(|| left.1.cmp(&right.1))
            .then_with(|| left.2.cmp(&right.2))
    });

    keyed_items.into_iter().map(|(item, _, _)| item).collect()
}

fn pack_with_order(
    problem: &TwoDProblem,
    items: &[ItemInstance2D],
    algorithm: &str,
    iterations: usize,
    strategy: MaxRectsStrategy,
) -> Result<TwoDSolution> {
    let mut sheets = Vec::<SheetState>::new();
    let mut usage_counts = vec![0_usize; problem.sheets.len()];
    let mut unplaced = Vec::new();

    for item in items {
        if let Some(candidate) = choose_existing_candidate(problem, &sheets, item, strategy) {
            place_candidate(&mut sheets[candidate.sheet_index], item, candidate);
            continue;
        }

        if let Some(candidate) = choose_new_sheet(problem, item, &usage_counts, strategy) {
            let stock = &problem.sheets[candidate.stock_index];
            let mut state = SheetState {
                stock_index: candidate.stock_index,
                free_rects: vec![Rect { x: 0, y: 0, width: stock.width, height: stock.height }],
                placements: Vec::new(),
            };

            place_candidate(
                &mut state,
                item,
                PlacementCandidate {
                    sheet_index: 0,
                    free_index: 0,
                    x: 0,
                    y: 0,
                    width: candidate.width,
                    height: candidate.height,
                    rotated: candidate.rotated,
                    area_waste: candidate.area_waste,
                    short_side_fit: candidate.short_side_fit,
                    long_side_fit: candidate.long_side_fit,
                    bottom: candidate.bottom,
                    left: candidate.left,
                    contact_score: candidate.contact_score,
                },
            );

            sheets.push(state);
            usage_counts[candidate.stock_index] =
                usage_counts[candidate.stock_index].saturating_add(1);
        } else {
            unplaced.push(item.clone());
        }
    }

    let layouts =
        sheets.into_iter().map(|sheet| (sheet.stock_index, sheet.placements)).collect::<Vec<_>>();

    Ok(TwoDSolution::from_layouts(
        algorithm.to_string(),
        false,
        &problem.sheets,
        layouts,
        unplaced,
        SolverMetrics2D {
            iterations,
            explored_states: 0,
            notes: vec![strategy.note().to_string()],
        },
    ))
}

fn choose_existing_candidate(
    problem: &TwoDProblem,
    sheets: &[SheetState],
    item: &ItemInstance2D,
    strategy: MaxRectsStrategy,
) -> Option<PlacementCandidate> {
    sheets
        .iter()
        .enumerate()
        .flat_map(|(sheet_index, sheet)| {
            sheet.free_rects.iter().enumerate().flat_map(move |(free_index, free)| {
                item.orientations()
                    .filter(move |(width, height, _)| free.fits(*width, *height))
                    .map(move |(width, height, rotated)| {
                        build_candidate(
                            problem,
                            sheet,
                            sheet_index,
                            free_index,
                            Oriented { width, height, rotated },
                        )
                    })
            })
        })
        .min_by(|left, right| compare_candidates(strategy, left, right))
}

fn choose_new_sheet(
    problem: &TwoDProblem,
    item: &ItemInstance2D,
    usage_counts: &[usize],
    strategy: MaxRectsStrategy,
) -> Option<NewSheetCandidate> {
    problem
        .sheets
        .iter()
        .enumerate()
        .filter(|(index, sheet)| {
            sheet.quantity.map(|quantity| usage_counts[*index] < quantity).unwrap_or(true)
        })
        .flat_map(|(stock_index, sheet)| {
            item.orientations()
                .filter(move |(width, height, _)| sheet.width >= *width && sheet.height >= *height)
                .map(move |(width, height, rotated)| NewSheetCandidate {
                    stock_index,
                    width,
                    height,
                    rotated,
                    cost: sheet.cost,
                    area_waste: u64::from(sheet.width) * u64::from(sheet.height)
                        - u64::from(width) * u64::from(height),
                    short_side_fit: sheet
                        .width
                        .saturating_sub(width)
                        .min(sheet.height.saturating_sub(height)),
                    long_side_fit: sheet
                        .width
                        .saturating_sub(width)
                        .max(sheet.height.saturating_sub(height)),
                    bottom: height,
                    left: 0,
                    contact_score: width.saturating_add(height),
                })
        })
        .min_by(|left, right| compare_new_sheet_candidates(strategy, left, right))
}

#[derive(Debug, Clone, Copy)]
struct Oriented {
    width: u32,
    height: u32,
    rotated: bool,
}

fn build_candidate(
    problem: &TwoDProblem,
    sheet: &SheetState,
    sheet_index: usize,
    free_index: usize,
    oriented: Oriented,
) -> PlacementCandidate {
    let free = sheet.free_rects[free_index];
    let Oriented { width, height, rotated } = oriented;
    let x = free.x;
    let y = free.y;
    PlacementCandidate {
        sheet_index,
        free_index,
        x,
        y,
        width,
        height,
        rotated,
        area_waste: free.area().saturating_sub(u64::from(width) * u64::from(height)),
        short_side_fit: free.width.saturating_sub(width).min(free.height.saturating_sub(height)),
        long_side_fit: free.width.saturating_sub(width).max(free.height.saturating_sub(height)),
        bottom: y.saturating_add(height),
        left: x,
        contact_score: contact_score(problem, sheet, x, y, width, height),
    }
}

fn place_candidate(sheet: &mut SheetState, item: &ItemInstance2D, candidate: PlacementCandidate) {
    // `candidate.x`/`candidate.y` are always equal to the chosen free rect's top-left
    // corner by construction — both `build_candidate` and the new-sheet path pin
    // placements to `(free.x, free.y)`.
    let placed =
        Rect { x: candidate.x, y: candidate.y, width: candidate.width, height: candidate.height };

    sheet.placements.push(Placement2D {
        name: item.name.clone(),
        x: placed.x,
        y: placed.y,
        width: placed.width,
        height: placed.height,
        rotated: candidate.rotated,
    });

    let old_free_rects = std::mem::take(&mut sheet.free_rects);
    for free_rect in old_free_rects {
        if !free_rect.intersects(placed) {
            sheet.free_rects.push(free_rect);
            continue;
        }

        split_free_rect(free_rect, placed, &mut sheet.free_rects);
    }

    prune_contained_rects(&mut sheet.free_rects);
}

fn split_free_rect(free: Rect, used: Rect, target: &mut Vec<Rect>) {
    let free_right = free.x + free.width;
    let free_bottom = free.y + free.height;
    let used_right = used.x + used.width;
    let used_bottom = used.y + used.height;
    let overlap_left = free.x.max(used.x);
    let overlap_top = free.y.max(used.y);
    let overlap_right = free_right.min(used_right);
    let overlap_bottom = free_bottom.min(used_bottom);

    if overlap_left > free.x {
        target.push(Rect {
            x: free.x,
            y: free.y,
            width: overlap_left - free.x,
            height: free.height,
        });
    }

    if overlap_right < free_right {
        target.push(Rect {
            x: overlap_right,
            y: free.y,
            width: free_right - overlap_right,
            height: free.height,
        });
    }

    if overlap_top > free.y {
        target.push(Rect { x: free.x, y: free.y, width: free.width, height: overlap_top - free.y });
    }

    if overlap_bottom < free_bottom {
        target.push(Rect {
            x: free.x,
            y: overlap_bottom,
            width: free.width,
            height: free_bottom - overlap_bottom,
        });
    }
}

fn prune_contained_rects(rects: &mut Vec<Rect>) {
    // Drop a rect if some *other* rect strictly contains it, OR if an earlier
    // rect in the list is equal to it (deduplication). The earlier-index
    // tie-break is load-bearing: without it, two identical rects would each
    // see the other as a container and both would be dropped, silently losing
    // that region of free space.
    let mut filtered = Vec::new();

    for (index, rect) in rects.iter().enumerate() {
        if rect.width == 0 || rect.height == 0 {
            continue;
        }
        let contained = rects.iter().enumerate().any(|(other_index, other)| {
            if index == other_index || !other.contains(*rect) {
                return false;
            }
            // `other` contains `rect`. If they are equal, only the
            // earlier-indexed copy survives; otherwise `other` strictly
            // contains `rect` and `rect` is dropped.
            *other != *rect || other_index < index
        });
        if !contained {
            filtered.push(*rect);
        }
    }

    *rects = filtered;
}

fn contact_score(
    problem: &TwoDProblem,
    sheet: &SheetState,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
) -> u32 {
    let sheet_def = &problem.sheets[sheet.stock_index];
    let mut score = 0_u32;

    if x == 0 || x.saturating_add(width) == sheet_def.width {
        score = score.saturating_add(height);
    }
    if y == 0 || y.saturating_add(height) == sheet_def.height {
        score = score.saturating_add(width);
    }

    for placement in &sheet.placements {
        if placement.x == x.saturating_add(width)
            || placement.x.saturating_add(placement.width) == x
        {
            score = score.saturating_add(overlap_len(
                y,
                y.saturating_add(height),
                placement.y,
                placement.y.saturating_add(placement.height),
            ));
        }
        if placement.y == y.saturating_add(height)
            || placement.y.saturating_add(placement.height) == y
        {
            score = score.saturating_add(overlap_len(
                x,
                x.saturating_add(width),
                placement.x,
                placement.x.saturating_add(placement.width),
            ));
        }
    }

    score
}

fn overlap_len(start_a: u32, end_a: u32, start_b: u32, end_b: u32) -> u32 {
    end_a.min(end_b).saturating_sub(start_a.max(start_b))
}

#[derive(Debug, Clone, Copy)]
struct CandidateMetrics {
    area_waste: u64,
    short_side_fit: u32,
    long_side_fit: u32,
    bottom: u32,
    left: u32,
    contact_score: u32,
}

impl PlacementCandidate {
    fn metrics(&self) -> CandidateMetrics {
        CandidateMetrics {
            area_waste: self.area_waste,
            short_side_fit: self.short_side_fit,
            long_side_fit: self.long_side_fit,
            bottom: self.bottom,
            left: self.left,
            contact_score: self.contact_score,
        }
    }
}

impl NewSheetCandidate {
    fn metrics(&self) -> CandidateMetrics {
        CandidateMetrics {
            area_waste: self.area_waste,
            short_side_fit: self.short_side_fit,
            long_side_fit: self.long_side_fit,
            bottom: self.bottom,
            left: self.left,
            contact_score: self.contact_score,
        }
    }
}

fn compare_candidates(
    strategy: MaxRectsStrategy,
    left: &PlacementCandidate,
    right: &PlacementCandidate,
) -> Ordering {
    strategy
        .compare(left.metrics(), right.metrics())
        .then_with(|| left.sheet_index.cmp(&right.sheet_index))
        .then_with(|| left.free_index.cmp(&right.free_index))
}

fn compare_new_sheet_candidates(
    strategy: MaxRectsStrategy,
    left: &NewSheetCandidate,
    right: &NewSheetCandidate,
) -> Ordering {
    strategy
        .compare(left.metrics(), right.metrics())
        .then_with(|| left.cost.total_cmp(&right.cost))
        .then_with(|| left.stock_index.cmp(&right.stock_index))
}

fn sort_items_descending(items: &mut [ItemInstance2D]) {
    items.sort_by(|left, right| {
        // Widen to u64 — u32 * u32 can overflow at MAX_DIMENSION = 1 << 30.
        let left_area = u64::from(left.width) * u64::from(left.height);
        let right_area = u64::from(right.width) * u64::from(right.height);
        right_area
            .cmp(&left_area)
            .then_with(|| right.width.max(right.height).cmp(&left.width.max(left.height)))
    });
}

impl MaxRectsStrategy {
    fn compare(self, left: CandidateMetrics, right: CandidateMetrics) -> Ordering {
        match self {
            Self::BestAreaFit => left
                .area_waste
                .cmp(&right.area_waste)
                .then_with(|| left.short_side_fit.cmp(&right.short_side_fit))
                .then_with(|| left.long_side_fit.cmp(&right.long_side_fit)),
            Self::BestShortSideFit => left
                .short_side_fit
                .cmp(&right.short_side_fit)
                .then_with(|| left.long_side_fit.cmp(&right.long_side_fit))
                .then_with(|| left.area_waste.cmp(&right.area_waste)),
            Self::BestLongSideFit => left
                .long_side_fit
                .cmp(&right.long_side_fit)
                .then_with(|| left.short_side_fit.cmp(&right.short_side_fit))
                .then_with(|| left.area_waste.cmp(&right.area_waste)),
            Self::BottomLeft => left
                .bottom
                .cmp(&right.bottom)
                .then_with(|| left.left.cmp(&right.left))
                .then_with(|| left.area_waste.cmp(&right.area_waste)),
            Self::ContactPoint => right
                .contact_score
                .cmp(&left.contact_score)
                .then_with(|| left.area_waste.cmp(&right.area_waste))
                .then_with(|| left.short_side_fit.cmp(&right.short_side_fit)),
        }
    }

    fn note(self) -> &'static str {
        match self {
            Self::BestAreaFit => "best-area-fit with free-rectangle splitting",
            Self::BestShortSideFit => "best-short-side-fit MaxRects with free-rectangle splitting",
            Self::BestLongSideFit => "best-long-side-fit MaxRects with free-rectangle splitting",
            Self::BottomLeft => "bottom-left MaxRects with free-rectangle splitting",
            Self::ContactPoint => "contact-point MaxRects with free-rectangle splitting",
        }
    }
}

#[cfg(test)]
mod tests {
    use rand::{SeedableRng, rngs::SmallRng};

    use crate::two_d::{RectDemand2D, Sheet2D, TwoDOptions, TwoDProblem};

    use super::{
        CandidateMetrics, ItemInstance2D, MaxRectsStrategy, Rect, multistart_ordered_items,
        overlap_len, prune_contained_rects, solve_maxrects, solve_maxrects_blsf,
        solve_maxrects_bottom_left, solve_maxrects_bssf, solve_maxrects_contact_point,
        solve_multistart,
    };

    #[test]
    fn maxrects_packs_simple_case_into_one_sheet() {
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
                    width: 5,
                    height: 5,
                    quantity: 2,
                    can_rotate: true,
                },
                RectDemand2D {
                    name: "B".to_string(),
                    width: 5,
                    height: 5,
                    quantity: 2,
                    can_rotate: true,
                },
            ],
        };

        let solution = solve_maxrects(&problem, &TwoDOptions::default()).expect("pack");
        assert_eq!(solution.sheet_count, 1);
        assert!(solution.unplaced.is_empty());
    }

    #[test]
    fn multistart_ordering_is_seeded_and_explores_tie_groups() {
        let items = vec![
            ItemInstance2D { name: "A".to_string(), width: 10, height: 10, can_rotate: true },
            ItemInstance2D { name: "B".to_string(), width: 10, height: 10, can_rotate: true },
            ItemInstance2D { name: "C".to_string(), width: 10, height: 10, can_rotate: true },
            ItemInstance2D { name: "D".to_string(), width: 10, height: 10, can_rotate: true },
        ];

        let mut first_rng = SmallRng::seed_from_u64(7);
        let mut repeat_rng = SmallRng::seed_from_u64(7);
        let mut different_rng = SmallRng::seed_from_u64(8);

        let first = multistart_ordered_items(&items, &mut first_rng);
        let repeated = multistart_ordered_items(&items, &mut repeat_rng);
        let different = multistart_ordered_items(&items, &mut different_rng);

        assert_eq!(first, repeated);
        assert_ne!(first, different);
    }

    #[test]
    fn maxrects_marks_items_unplaced_when_sheet_inventory_runs_out() {
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

        let solution = solve_maxrects(&problem, &TwoDOptions::default()).expect("pack");
        assert_eq!(solution.sheet_count, 1);
        assert_eq!(solution.unplaced.len(), 1);
    }

    #[test]
    fn multistart_can_improve_on_the_initial_maxrects_layout() {
        let problem = TwoDProblem {
            sheets: vec![
                Sheet2D {
                    name: "s0".to_string(),
                    width: 25,
                    height: 21,
                    cost: 1.0,
                    quantity: None,
                },
                Sheet2D { name: "s1".to_string(), width: 9, height: 11, cost: 2.0, quantity: None },
                Sheet2D {
                    name: "s2".to_string(),
                    width: 23,
                    height: 15,
                    cost: 2.0,
                    quantity: None,
                },
            ],
            demands: vec![
                RectDemand2D {
                    name: "d0".to_string(),
                    width: 12,
                    height: 9,
                    quantity: 2,
                    can_rotate: true,
                },
                RectDemand2D {
                    name: "d1".to_string(),
                    width: 13,
                    height: 20,
                    quantity: 2,
                    can_rotate: false,
                },
                RectDemand2D {
                    name: "d2".to_string(),
                    width: 20,
                    height: 11,
                    quantity: 1,
                    can_rotate: true,
                },
                RectDemand2D {
                    name: "d3".to_string(),
                    width: 12,
                    height: 9,
                    quantity: 2,
                    can_rotate: false,
                },
            ],
        };

        let options = TwoDOptions { seed: Some(11), ..TwoDOptions::default() };
        let baseline = solve_maxrects(&problem, &options).expect("baseline");
        let multistart = solve_multistart(&problem, &options).expect("multistart");

        assert!(multistart.is_better_than(&baseline));
    }

    #[test]
    fn multistart_uses_a_stable_default_seed() {
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
                    width: 5,
                    height: 5,
                    quantity: 2,
                    can_rotate: true,
                },
                RectDemand2D {
                    name: "B".to_string(),
                    width: 5,
                    height: 5,
                    quantity: 2,
                    can_rotate: true,
                },
            ],
        };

        let first = solve_multistart(&problem, &TwoDOptions::default()).expect("first");
        let second = solve_multistart(&problem, &TwoDOptions::default()).expect("second");

        assert_eq!(first.layouts, second.layouts);
        assert_eq!(first.total_waste_area, second.total_waste_area);
        assert_eq!(first.metrics.notes, second.metrics.notes);
    }

    #[test]
    fn explicit_maxrects_variants_are_available() {
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

        let bssf = solve_maxrects_bssf(&problem, &TwoDOptions::default()).expect("bssf");
        let blsf = solve_maxrects_blsf(&problem, &TwoDOptions::default()).expect("blsf");
        let bottom_left =
            solve_maxrects_bottom_left(&problem, &TwoDOptions::default()).expect("bottom-left");
        let contact =
            solve_maxrects_contact_point(&problem, &TwoDOptions::default()).expect("contact");

        assert_eq!(bssf.algorithm, "max_rects_best_short_side_fit");
        assert_eq!(blsf.algorithm, "max_rects_best_long_side_fit");
        assert_eq!(bottom_left.algorithm, "max_rects_bottom_left");
        assert_eq!(contact.algorithm, "max_rects_contact_point");
        assert!(bssf.unplaced.is_empty());
        assert!(blsf.unplaced.is_empty());
        assert!(bottom_left.unplaced.is_empty());
        assert!(contact.unplaced.is_empty());
    }

    /// Differential ranking test for `MaxRectsStrategy::compare`. With two
    /// crafted metric sets that differ on each axis, every strategy should
    /// pick a different winner consistent with its documented priority.
    #[test]
    fn maxrects_strategy_compare_picks_per_strategy_primary_key() {
        use std::cmp::Ordering;

        // `left` is better on area_waste but worse on every side-fit and
        // contact metric. `right` is the opposite.
        let left = CandidateMetrics {
            area_waste: 10,
            short_side_fit: 50,
            long_side_fit: 50,
            bottom: 100,
            left: 100,
            contact_score: 1,
        };
        let right = CandidateMetrics {
            area_waste: 100,
            short_side_fit: 5,
            long_side_fit: 5,
            bottom: 10,
            left: 10,
            contact_score: 99,
        };

        // BestAreaFit ranks by area_waste first → left wins (smaller waste).
        assert_eq!(MaxRectsStrategy::BestAreaFit.compare(left, right), Ordering::Less);

        // BestShortSideFit ranks by short_side_fit first → right wins.
        assert_eq!(MaxRectsStrategy::BestShortSideFit.compare(left, right), Ordering::Greater);

        // BestLongSideFit ranks by long_side_fit first → right wins.
        assert_eq!(MaxRectsStrategy::BestLongSideFit.compare(left, right), Ordering::Greater);

        // BottomLeft ranks by bottom (then left) → right wins (lower y).
        assert_eq!(MaxRectsStrategy::BottomLeft.compare(left, right), Ordering::Greater);

        // ContactPoint inverts the comparison: higher contact_score wins, so
        // right (score 99) should beat left (score 1).
        assert_eq!(MaxRectsStrategy::ContactPoint.compare(left, right), Ordering::Greater);
    }

    /// `overlap_len` is the half-open interval overlap helper used by
    /// `contact_score`. Pin down every branch so a sign flip or a swap of
    /// `min`/`max` cannot slip through silently.
    #[test]
    fn overlap_len_half_open_interval_semantics() {
        // Fully overlapping intervals.
        assert_eq!(overlap_len(0, 10, 0, 10), 10);

        // A is fully inside B.
        assert_eq!(overlap_len(3, 7, 0, 10), 4);

        // Partial overlap on the right side of A.
        assert_eq!(overlap_len(0, 5, 3, 10), 2);

        // Partial overlap on the left side of A.
        assert_eq!(overlap_len(3, 10, 0, 5), 2);

        // Touching at a single point: [0, 5) and [5, 10) do not overlap.
        assert_eq!(overlap_len(0, 5, 5, 10), 0);
        assert_eq!(overlap_len(5, 10, 0, 5), 0);

        // Disjoint intervals with a gap.
        assert_eq!(overlap_len(0, 3, 7, 10), 0);
        assert_eq!(overlap_len(7, 10, 0, 3), 0);

        // Degenerate intervals (start == end) contribute no overlap even when
        // they sit inside another interval.
        assert_eq!(overlap_len(5, 5, 0, 10), 0);
        assert_eq!(overlap_len(0, 10, 5, 5), 0);
    }

    /// Regression test for a bug where `prune_contained_rects` would drop
    /// every copy of a duplicated free rectangle. `Rect::contains` uses `<=`
    /// and `>=` so two identical rects each "contain" the other; without the
    /// earlier-index tie-break, both were marked contained and removed,
    /// silently losing that region of free space.
    #[test]
    fn prune_contained_rects_keeps_first_of_each_duplicate() {
        // Two identical rects — the fix must keep exactly one.
        let mut identical = vec![
            Rect { x: 0, y: 0, width: 10, height: 10 },
            Rect { x: 0, y: 0, width: 10, height: 10 },
        ];
        prune_contained_rects(&mut identical);
        assert_eq!(identical.len(), 1, "identical duplicates must collapse to one entry");
        assert_eq!(identical[0], Rect { x: 0, y: 0, width: 10, height: 10 });

        // Three identical rects — still collapse to one.
        let mut triple = vec![
            Rect { x: 4, y: 4, width: 6, height: 6 },
            Rect { x: 4, y: 4, width: 6, height: 6 },
            Rect { x: 4, y: 4, width: 6, height: 6 },
        ];
        prune_contained_rects(&mut triple);
        assert_eq!(triple.len(), 1);

        // Strict containment is still handled: the inner rect is dropped.
        let mut containment = vec![
            Rect { x: 0, y: 0, width: 10, height: 10 },
            Rect { x: 2, y: 2, width: 3, height: 3 },
        ];
        prune_contained_rects(&mut containment);
        assert_eq!(containment.len(), 1);
        assert_eq!(containment[0].width, 10);

        // Disjoint rects are both kept.
        let mut disjoint = vec![
            Rect { x: 0, y: 0, width: 5, height: 5 },
            Rect { x: 10, y: 0, width: 5, height: 5 },
        ];
        prune_contained_rects(&mut disjoint);
        assert_eq!(disjoint.len(), 2);

        // A mix of strict containment and a duplicate: the duplicate is kept
        // once, the strictly-contained rect is dropped.
        let mut mixed = vec![
            Rect { x: 0, y: 0, width: 10, height: 10 },
            Rect { x: 0, y: 0, width: 10, height: 10 },
            Rect { x: 3, y: 3, width: 2, height: 2 },
        ];
        prune_contained_rects(&mut mixed);
        assert_eq!(mixed.len(), 1);
        assert_eq!(mixed[0].width, 10);

        // Zero-dimension rects are always pruned.
        let mut zero_dim = vec![
            Rect { x: 0, y: 0, width: 10, height: 10 },
            Rect { x: 2, y: 2, width: 0, height: 5 },
            Rect { x: 2, y: 2, width: 5, height: 0 },
        ];
        prune_contained_rects(&mut zero_dim);
        assert_eq!(zero_dim.len(), 1);
        assert_eq!(zero_dim[0].width, 10);
    }
}
