use std::cmp::Ordering;

use crate::Result;

use super::model::{
    ItemInstance2D, Placement2D, SolverMetrics2D, TwoDOptions, TwoDProblem, TwoDSolution,
};

#[derive(Debug, Clone)]
struct SheetState {
    stock_index: usize,
    shelves: Vec<Shelf>,
    placements: Vec<Placement2D>,
}

#[derive(Debug, Clone, Copy)]
struct Shelf {
    y: u32,
    height: u32,
    used_width: u32,
}

#[derive(Debug, Clone, Copy)]
struct ExistingShelfCandidate {
    sheet_index: usize,
    shelf_index: usize,
    width: u32,
    height: u32,
    rotated: bool,
    remaining_width: u32,
}

#[derive(Debug, Clone, Copy)]
struct NewShelfCandidate {
    sheet_index: usize,
    width: u32,
    height: u32,
    rotated: bool,
    remaining_width: u32,
    y: u32,
}

#[derive(Debug, Clone, Copy)]
struct NewSheetCandidate {
    stock_index: usize,
    width: u32,
    height: u32,
    rotated: bool,
    waste: u64,
    cost: f64,
}

// Variant names match the canonical shelf-packing strategy names (Next-Fit, First-Fit,
// Best-Fit decreasing height). Renaming them to `Next`, `First`, `Best` would lose the
// link to the algorithm literature, so we silence the nitpick lint.
#[allow(clippy::enum_variant_names)]
#[derive(Debug, Clone, Copy)]
enum ShelfStrategy {
    NextFit,
    FirstFit,
    BestFit,
}

pub(super) fn solve_nfdh(problem: &TwoDProblem, options: &TwoDOptions) -> Result<TwoDSolution> {
    solve_shelf(problem, options, ShelfStrategy::NextFit, "next_fit_decreasing_height")
}

pub(super) fn solve_ffdh(problem: &TwoDProblem, options: &TwoDOptions) -> Result<TwoDSolution> {
    solve_shelf(problem, options, ShelfStrategy::FirstFit, "first_fit_decreasing_height")
}

pub(super) fn solve_bfdh(problem: &TwoDProblem, options: &TwoDOptions) -> Result<TwoDSolution> {
    solve_shelf(problem, options, ShelfStrategy::BestFit, "best_fit_decreasing_height")
}

fn solve_shelf(
    problem: &TwoDProblem,
    options: &TwoDOptions,
    strategy: ShelfStrategy,
    algorithm: &str,
) -> Result<TwoDSolution> {
    let mut items = problem.expanded_items();
    sort_items_descending(&mut items);

    let mut sheets = Vec::<SheetState>::new();
    let mut usage_counts = vec![0_usize; problem.sheets.len()];
    let mut unplaced = Vec::new();
    let mut explored_states = 0_usize;

    for item in items {
        explored_states = explored_states.saturating_add(1);

        if let Some(candidate) = choose_existing_shelf(problem, &sheets, &item, strategy) {
            let sheet_kerf = problem.sheets[sheets[candidate.sheet_index].stock_index].kerf;
            place_on_existing_shelf(
                &mut sheets[candidate.sheet_index],
                sheet_kerf,
                &item,
                candidate,
            );
            continue;
        }

        if let Some(candidate) = choose_new_shelf(problem, &sheets, &item, strategy) {
            place_on_new_shelf(&mut sheets[candidate.sheet_index], &item, candidate);
            continue;
        }

        if let Some(candidate) = choose_new_sheet(problem, &item, &usage_counts) {
            open_new_sheet(&mut sheets, &item, candidate);
            usage_counts[candidate.stock_index] =
                usage_counts[candidate.stock_index].saturating_add(1);
        } else {
            unplaced.push(item);
        }
    }

    let layouts =
        sheets.into_iter().map(|sheet| (sheet.stock_index, sheet.placements)).collect::<Vec<_>>();

    Ok(TwoDSolution::from_layouts(
        algorithm,
        false,
        &problem.sheets,
        layouts,
        unplaced,
        SolverMetrics2D {
            iterations: 1,
            explored_states,
            notes: vec!["decreasing-height shelf packing heuristic".to_string()],
        },
        options.min_usable_side,
    ))
}

fn choose_existing_shelf(
    problem: &TwoDProblem,
    sheets: &[SheetState],
    item: &ItemInstance2D,
    strategy: ShelfStrategy,
) -> Option<ExistingShelfCandidate> {
    match strategy {
        ShelfStrategy::NextFit => sheets.last().and_then(|sheet| {
            let sheet_index = sheets.len().saturating_sub(1);
            let sheet_def = &problem.sheets[sheet.stock_index];
            let (eff_w, _eff_h) = crate::two_d::model::effective_bounds(sheet_def);
            sheet.shelves.last().and_then(|shelf| {
                let shelf_index = sheet.shelves.len().saturating_sub(1);
                existing_shelf_candidate(
                    sheet_index,
                    shelf_index,
                    eff_w,
                    sheet_def.kerf,
                    shelf,
                    item,
                )
            })
        }),
        ShelfStrategy::FirstFit => {
            for (sheet_index, sheet) in sheets.iter().enumerate() {
                let sheet_def = &problem.sheets[sheet.stock_index];
                let (eff_w, _eff_h) = crate::two_d::model::effective_bounds(sheet_def);
                for (shelf_index, shelf) in sheet.shelves.iter().enumerate() {
                    if let Some(candidate) = existing_shelf_candidate(
                        sheet_index,
                        shelf_index,
                        eff_w,
                        sheet_def.kerf,
                        shelf,
                        item,
                    ) {
                        return Some(candidate);
                    }
                }
            }
            None
        }
        ShelfStrategy::BestFit => sheets
            .iter()
            .enumerate()
            .flat_map(|(sheet_index, sheet)| {
                let sheet_def = &problem.sheets[sheet.stock_index];
                let (eff_w, _eff_h) = crate::two_d::model::effective_bounds(sheet_def);
                sheet.shelves.iter().enumerate().filter_map(move |(shelf_index, shelf)| {
                    existing_shelf_candidate(
                        sheet_index,
                        shelf_index,
                        eff_w,
                        sheet_def.kerf,
                        shelf,
                        item,
                    )
                })
            })
            .min_by(compare_existing_candidates),
    }
}

fn existing_shelf_candidate(
    sheet_index: usize,
    shelf_index: usize,
    sheet_width: u32,
    sheet_kerf: u32,
    shelf: &Shelf,
    item: &ItemInstance2D,
) -> Option<ExistingShelfCandidate> {
    // A non-empty shelf already has at least one placement, so the next
    // placement needs a kerf gap. An empty shelf (used_width == 0) is
    // flush against the sheet's left edge — no kerf there per D3.
    let gap = if shelf.used_width == 0 { 0 } else { sheet_kerf };
    item.orientations()
        .filter(|(width, height, _)| {
            *height <= shelf.height
                && shelf.used_width.saturating_add(gap).saturating_add(*width) <= sheet_width
        })
        .map(|(width, height, rotated)| ExistingShelfCandidate {
            sheet_index,
            shelf_index,
            width,
            height,
            rotated,
            remaining_width: sheet_width
                .saturating_sub(shelf.used_width.saturating_add(gap).saturating_add(width)),
        })
        .min_by(compare_existing_candidates)
}

fn choose_new_shelf(
    problem: &TwoDProblem,
    sheets: &[SheetState],
    item: &ItemInstance2D,
    strategy: ShelfStrategy,
) -> Option<NewShelfCandidate> {
    match strategy {
        ShelfStrategy::NextFit => sheets.last().and_then(|sheet| {
            new_shelf_candidate(problem, sheets.len().saturating_sub(1), sheet, item)
        }),
        ShelfStrategy::FirstFit => {
            for (sheet_index, sheet) in sheets.iter().enumerate() {
                if let Some(candidate) = new_shelf_candidate(problem, sheet_index, sheet, item) {
                    return Some(candidate);
                }
            }
            None
        }
        ShelfStrategy::BestFit => sheets
            .iter()
            .enumerate()
            .filter_map(|(sheet_index, sheet)| {
                new_shelf_candidate(problem, sheet_index, sheet, item)
            })
            .min_by(compare_new_shelf_candidates),
    }
}

fn new_shelf_candidate(
    problem: &TwoDProblem,
    sheet_index: usize,
    sheet: &SheetState,
    item: &ItemInstance2D,
) -> Option<NewShelfCandidate> {
    let sheet_def = &problem.sheets[sheet.stock_index];
    let base_y = sheet_height(sheet);
    // First shelf sits flush against the sheet's top edge; every
    // subsequent shelf needs a kerf gap above it.
    let gap = if base_y == 0 { 0 } else { sheet_def.kerf };
    let y = base_y.saturating_add(gap);

    let (eff_w, eff_h) = crate::two_d::model::effective_bounds(sheet_def);
    item.orientations()
        .filter(|(width, height, _)| *width <= eff_w && y.saturating_add(*height) <= eff_h)
        .map(|(width, height, rotated)| NewShelfCandidate {
            sheet_index,
            width,
            height,
            rotated,
            remaining_width: eff_w.saturating_sub(width),
            y,
        })
        .min_by(compare_new_shelf_candidates)
}

fn choose_new_sheet(
    problem: &TwoDProblem,
    item: &ItemInstance2D,
    usage_counts: &[usize],
) -> Option<NewSheetCandidate> {
    problem
        .sheets
        .iter()
        .enumerate()
        .filter(|(stock_index, sheet)| {
            sheet.quantity.map(|quantity| usage_counts[*stock_index] < quantity).unwrap_or(true)
        })
        .flat_map(|(stock_index, sheet)| {
            let (eff_w, eff_h) = crate::two_d::model::effective_bounds(sheet);
            item.orientations()
                .filter(move |(width, height, _)| *width <= eff_w && *height <= eff_h)
                .map(move |(width, height, rotated)| NewSheetCandidate {
                    stock_index,
                    width,
                    height,
                    rotated,
                    waste: u64::from(eff_w) * u64::from(eff_h)
                        - u64::from(width) * u64::from(height),
                    cost: sheet.cost,
                })
        })
        .min_by(compare_new_sheet_candidates)
}

fn place_on_existing_shelf(
    sheet: &mut SheetState,
    sheet_kerf: u32,
    item: &ItemInstance2D,
    candidate: ExistingShelfCandidate,
) {
    let shelf = &mut sheet.shelves[candidate.shelf_index];
    let gap = if shelf.used_width == 0 { 0 } else { sheet_kerf };
    let x = shelf.used_width.saturating_add(gap);
    shelf.used_width = x.saturating_add(candidate.width);

    sheet.placements.push(Placement2D {
        name: item.name.clone(),
        x,
        y: shelf.y,
        width: candidate.width,
        height: candidate.height,
        rotated: candidate.rotated,
    });
}

fn place_on_new_shelf(sheet: &mut SheetState, item: &ItemInstance2D, candidate: NewShelfCandidate) {
    sheet.shelves.push(Shelf {
        y: candidate.y,
        height: candidate.height,
        used_width: candidate.width,
    });
    sheet.placements.push(Placement2D {
        name: item.name.clone(),
        x: 0,
        y: candidate.y,
        width: candidate.width,
        height: candidate.height,
        rotated: candidate.rotated,
    });
}

fn open_new_sheet(
    sheets: &mut Vec<SheetState>,
    item: &ItemInstance2D,
    candidate: NewSheetCandidate,
) {
    sheets.push(SheetState {
        stock_index: candidate.stock_index,
        shelves: vec![Shelf { y: 0, height: candidate.height, used_width: candidate.width }],
        placements: vec![Placement2D {
            name: item.name.clone(),
            x: 0,
            y: 0,
            width: candidate.width,
            height: candidate.height,
            rotated: candidate.rotated,
        }],
    });
}

fn compare_existing_candidates(
    left: &ExistingShelfCandidate,
    right: &ExistingShelfCandidate,
) -> Ordering {
    left.remaining_width
        .cmp(&right.remaining_width)
        .then_with(|| left.height.cmp(&right.height))
        .then_with(|| left.sheet_index.cmp(&right.sheet_index))
        .then_with(|| left.shelf_index.cmp(&right.shelf_index))
}

fn compare_new_shelf_candidates(left: &NewShelfCandidate, right: &NewShelfCandidate) -> Ordering {
    left.remaining_width
        .cmp(&right.remaining_width)
        .then_with(|| left.height.cmp(&right.height))
        .then_with(|| left.y.cmp(&right.y))
        .then_with(|| left.sheet_index.cmp(&right.sheet_index))
}

fn compare_new_sheet_candidates(left: &NewSheetCandidate, right: &NewSheetCandidate) -> Ordering {
    left.waste
        .cmp(&right.waste)
        .then_with(|| left.cost.total_cmp(&right.cost))
        .then_with(|| left.stock_index.cmp(&right.stock_index))
}

fn sheet_height(sheet: &SheetState) -> u32 {
    sheet.shelves.last().map(|shelf| shelf.y.saturating_add(shelf.height)).unwrap_or(0)
}

fn sort_items_descending(items: &mut [ItemInstance2D]) {
    items.sort_by(|left, right| {
        right
            .height
            .cmp(&left.height)
            .then_with(|| {
                // Widen to u64 — u32 * u32 can overflow at MAX_DIMENSION.
                let left_area = u64::from(left.width) * u64::from(left.height);
                let right_area = u64::from(right.width) * u64::from(right.height);
                right_area.cmp(&left_area)
            })
            .then_with(|| right.width.cmp(&left.width))
    });
}

#[cfg(test)]
mod tests {
    use crate::two_d::{RectDemand2D, Sheet2D, TwoDOptions, TwoDProblem};

    use super::{solve_bfdh, solve_ffdh, solve_nfdh};

    #[test]
    fn ffdh_and_bfdh_choose_different_shelves() {
        let problem = TwoDProblem {
            sheets: vec![Sheet2D {
                name: "sheet".to_string(),
                width: 10,
                height: 8,
                cost: 1.0,
                quantity: None,
                kerf: 0,
                edge_kerf_relief: false,
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
                    width: 8,
                    height: 3,
                    quantity: 1,
                    can_rotate: false,
                },
                RectDemand2D {
                    name: "C".to_string(),
                    width: 2,
                    height: 3,
                    quantity: 1,
                    can_rotate: false,
                },
            ],
        };

        let ffdh = solve_ffdh(&problem, &TwoDOptions::default()).expect("ffdh");
        let bfdh = solve_bfdh(&problem, &TwoDOptions::default()).expect("bfdh");

        let ffdh_c = ffdh.layouts[0]
            .placements
            .iter()
            .find(|placement| placement.name == "C")
            .expect("ffdh placement");
        let bfdh_c = bfdh.layouts[0]
            .placements
            .iter()
            .find(|placement| placement.name == "C")
            .expect("bfdh placement");

        assert_eq!(ffdh.sheet_count, 1);
        assert_eq!(bfdh.sheet_count, 1);
        assert_eq!(ffdh_c.y, 0);
        assert_eq!(bfdh_c.y, 4);
    }

    #[test]
    fn shelf_algorithms_mark_items_unplaced_when_sheet_inventory_runs_out() {
        let problem = TwoDProblem {
            sheets: vec![Sheet2D {
                name: "sheet".to_string(),
                width: 6,
                height: 4,
                cost: 1.0,
                quantity: Some(1),
                kerf: 0,
                edge_kerf_relief: false,
            }],
            demands: vec![
                RectDemand2D {
                    name: "A".to_string(),
                    width: 6,
                    height: 2,
                    quantity: 2,
                    can_rotate: false,
                },
                RectDemand2D {
                    name: "B".to_string(),
                    width: 6,
                    height: 2,
                    quantity: 1,
                    can_rotate: false,
                },
            ],
        };

        let nfdh = solve_nfdh(&problem, &TwoDOptions::default()).expect("nfdh");
        let ffdh = solve_ffdh(&problem, &TwoDOptions::default()).expect("ffdh");
        let bfdh = solve_bfdh(&problem, &TwoDOptions::default()).expect("bfdh");

        assert_eq!(nfdh.sheet_count, 1);
        assert_eq!(ffdh.sheet_count, 1);
        assert_eq!(bfdh.sheet_count, 1);
        assert_eq!(nfdh.unplaced.len(), 1);
        assert_eq!(ffdh.unplaced.len(), 1);
        assert_eq!(bfdh.unplaced.len(), 1);
    }

    #[test]
    fn shelf_edge_relief_packs_two_pieces_on_one_sheet_with_overrun() {
        let problem = TwoDProblem {
            sheets: vec![Sheet2D {
                name: "s".into(),
                width: 48,
                height: 10,
                cost: 1.0,
                quantity: None,
                kerf: 1,
                edge_kerf_relief: true,
            }],
            demands: vec![RectDemand2D {
                name: "p".into(),
                width: 24,
                height: 10,
                quantity: 2,
                can_rotate: false,
            }],
        };

        let solution = solve_nfdh(&problem, &TwoDOptions::default()).expect("shelf should solve");

        assert_eq!(solution.sheet_count, 1);
        let sheet = &solution.layouts[0];
        assert_eq!(sheet.placements.len(), 2);
        let max_right =
            sheet.placements.iter().map(|p| p.x + p.width).max().expect("placements nonempty");
        assert_eq!(max_right, 49);
    }
}
