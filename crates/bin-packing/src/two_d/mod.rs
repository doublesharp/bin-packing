//! Two-dimensional rectangular bin packing solvers.
//!
//! Provides MaxRects (several heuristics), Skyline, Guillotine beam search, and shelf-based
//! NFDH/FFDH/BFDH algorithms plus a multistart meta-strategy.

mod guillotine;
mod maxrects;
mod model;
mod shelf;
mod skyline;

pub use model::{
    Placement2D, RectDemand2D, Sheet2D, SheetLayout2D, SolverMetrics2D, TwoDAlgorithm, TwoDOptions,
    TwoDProblem, TwoDSolution,
};

use crate::Result;

/// Solve a 2D rectangular bin packing problem using the requested algorithm.
///
/// Validates the problem and dispatches to the algorithm selected in `options`. Use
/// [`TwoDAlgorithm::Auto`] to let the solver try several strategies and return the best
/// result.
///
/// # Errors
///
/// Returns [`BinPackingError::InvalidInput`](crate::BinPackingError::InvalidInput) for
/// malformed problems and
/// [`BinPackingError::Infeasible2D`](crate::BinPackingError::Infeasible2D) when at least
/// one demand cannot fit any declared sheet (even with rotation).
pub fn solve_2d(problem: TwoDProblem, options: TwoDOptions) -> Result<TwoDSolution> {
    problem.validate()?;
    problem.ensure_feasible_demands()?;

    match options.algorithm {
        TwoDAlgorithm::MaxRects => maxrects::solve_maxrects(&problem, &options),
        TwoDAlgorithm::MaxRectsBestShortSideFit => {
            maxrects::solve_maxrects_bssf(&problem, &options)
        }
        TwoDAlgorithm::MaxRectsBestLongSideFit => maxrects::solve_maxrects_blsf(&problem, &options),
        TwoDAlgorithm::MaxRectsBottomLeft => {
            maxrects::solve_maxrects_bottom_left(&problem, &options)
        }
        TwoDAlgorithm::MaxRectsContactPoint => {
            maxrects::solve_maxrects_contact_point(&problem, &options)
        }
        TwoDAlgorithm::Skyline => skyline::solve_skyline(&problem, &options),
        TwoDAlgorithm::SkylineMinWaste => skyline::solve_skyline_min_waste(&problem, &options),
        TwoDAlgorithm::Guillotine => guillotine::solve_guillotine(&problem, &options),
        TwoDAlgorithm::GuillotineBestShortSideFit => {
            guillotine::solve_guillotine_bssf(&problem, &options)
        }
        TwoDAlgorithm::GuillotineBestLongSideFit => {
            guillotine::solve_guillotine_blsf(&problem, &options)
        }
        TwoDAlgorithm::GuillotineShorterLeftoverAxis => {
            guillotine::solve_guillotine_slas(&problem, &options)
        }
        TwoDAlgorithm::GuillotineLongerLeftoverAxis => {
            guillotine::solve_guillotine_llas(&problem, &options)
        }
        TwoDAlgorithm::GuillotineMinAreaSplit => {
            guillotine::solve_guillotine_min_area_split(&problem, &options)
        }
        TwoDAlgorithm::GuillotineMaxAreaSplit => {
            guillotine::solve_guillotine_max_area_split(&problem, &options)
        }
        TwoDAlgorithm::NextFitDecreasingHeight => shelf::solve_nfdh(&problem, &options),
        TwoDAlgorithm::FirstFitDecreasingHeight => shelf::solve_ffdh(&problem, &options),
        TwoDAlgorithm::BestFitDecreasingHeight => shelf::solve_bfdh(&problem, &options),
        TwoDAlgorithm::MultiStart => maxrects::solve_multistart(&problem, &options),
        TwoDAlgorithm::Auto => solve_auto(problem, options),
    }
}

fn solve_auto(problem: TwoDProblem, options: TwoDOptions) -> Result<TwoDSolution> {
    if options.guillotine_required {
        return solve_auto_guillotine(problem, options);
    }

    let mut best = maxrects::solve_maxrects(&problem, &options)?;
    let bssf = maxrects::solve_maxrects_bssf(&problem, &options)?;
    if bssf.is_better_than(&best) {
        best = bssf;
    }

    let blsf = maxrects::solve_maxrects_blsf(&problem, &options)?;
    if blsf.is_better_than(&best) {
        best = blsf;
    }

    let contact = maxrects::solve_maxrects_contact_point(&problem, &options)?;
    if contact.is_better_than(&best) {
        best = contact;
    }

    let skyline = skyline::solve_skyline(&problem, &options)?;
    if skyline.is_better_than(&best) {
        best = skyline;
    }

    let skyline_min_waste = skyline::solve_skyline_min_waste(&problem, &options)?;
    if skyline_min_waste.is_better_than(&best) {
        best = skyline_min_waste;
    }

    let bfdh = shelf::solve_bfdh(&problem, &options)?;
    if bfdh.is_better_than(&best) {
        best = bfdh;
    }

    let guillotine_bssf = guillotine::solve_guillotine_bssf(&problem, &options)?;
    if guillotine_bssf.is_better_than(&best) {
        best = guillotine_bssf;
    }

    let guillotine_slas = guillotine::solve_guillotine_slas(&problem, &options)?;
    if guillotine_slas.is_better_than(&best) {
        best = guillotine_slas;
    }

    let multistart = maxrects::solve_multistart(&problem, &options)?;
    if multistart.is_better_than(&best) {
        best = multistart;
    }

    Ok(best)
}

fn solve_auto_guillotine(problem: TwoDProblem, options: TwoDOptions) -> Result<TwoDSolution> {
    let mut best = guillotine::solve_guillotine(&problem, &options)?;

    let bssf = guillotine::solve_guillotine_bssf(&problem, &options)?;
    if bssf.is_better_than(&best) {
        best = bssf;
    }

    let blsf = guillotine::solve_guillotine_blsf(&problem, &options)?;
    if blsf.is_better_than(&best) {
        best = blsf;
    }

    let slas = guillotine::solve_guillotine_slas(&problem, &options)?;
    if slas.is_better_than(&best) {
        best = slas;
    }

    let llas = guillotine::solve_guillotine_llas(&problem, &options)?;
    if llas.is_better_than(&best) {
        best = llas;
    }

    let min_area = guillotine::solve_guillotine_min_area_split(&problem, &options)?;
    if min_area.is_better_than(&best) {
        best = min_area;
    }

    let max_area = guillotine::solve_guillotine_max_area_split(&problem, &options)?;
    if max_area.is_better_than(&best) {
        best = max_area;
    }

    Ok(best)
}

pub(crate) use model::ItemInstance2D;

/// Place a list of pre-instanced items onto a single sheet of the given
/// dimensions using the requested algorithm. Returns the produced
/// placements (in placement order) and any items that did not fit.
///
/// This is a placement-only entrypoint used by the 3D layer/wall/column
/// solvers. It does **not** call `TwoDProblem::validate` or
/// `ensure_feasible_demands` — items that cannot fit the sheet in any
/// allowed rotation are silently moved to the returned `unplaced` list
/// rather than producing an error. The caller is responsible for the
/// outer-level validation.
// Consumed by the Task 8/9/10 layer/wall/column 3D solvers; kept live
// here via the regression tests in this module.
#[allow(dead_code)]
pub(crate) fn place_into_sheet(
    items: &[ItemInstance2D],
    sheet_width: u32,
    sheet_height: u32,
    algorithm: TwoDAlgorithm,
    options: &TwoDOptions,
) -> Result<(Vec<Placement2D>, Vec<ItemInstance2D>)> {
    if items.is_empty() {
        return Ok((Vec::new(), Vec::new()));
    }

    // Pre-filter items that cannot possibly fit the sheet in any allowed
    // rotation. Sending them through `solve_2d` would trigger
    // `ensure_feasible_demands` and bubble up `Infeasible2D` — but we owe
    // the caller a partial answer, so route them straight into `leftover`.
    let mut leftover: Vec<ItemInstance2D> = Vec::new();
    let mut feasible_items: Vec<(usize, &ItemInstance2D)> = Vec::new();
    for (index, item) in items.iter().enumerate() {
        // Defensive check: zero dimensions cannot have come from an
        // expanded demand (validation rejects them upstream), but we still
        // refuse to forward them to `solve_2d`.
        if item.width == 0 || item.height == 0 {
            debug_assert!(false, "place_into_sheet: zero-dimension item `{}`", item.name);
            leftover.push(item.clone());
            continue;
        }

        // Synthetic name collision guard. Real callers should not pass
        // names matching the placeholder pattern.
        debug_assert!(
            !item.name.starts_with("__item_"),
            "place_into_sheet: item name `{}` collides with synthetic placeholder",
            item.name,
        );

        let fits_default = item.width <= sheet_width && item.height <= sheet_height;
        let fits_rotated =
            item.can_rotate && item.height <= sheet_width && item.width <= sheet_height;
        if fits_default || fits_rotated {
            feasible_items.push((index, item));
        } else {
            leftover.push(item.clone());
        }
    }

    if feasible_items.is_empty() {
        return Ok((Vec::new(), leftover));
    }

    // Build a synthetic single-sheet problem and reuse the existing solver
    // dispatch. The synthetic name `__item_<feasible_index>__` lets us
    // recover the original `ItemInstance2D` after `solve_2d` returns.
    let problem = TwoDProblem {
        sheets: vec![Sheet2D {
            name: "__placement_sheet__".to_string(),
            width: sheet_width,
            height: sheet_height,
            cost: 0.0,
            quantity: Some(1),
        }],
        demands: feasible_items
            .iter()
            .enumerate()
            .map(|(feasible_idx, (_, item))| RectDemand2D {
                name: format!("__item_{feasible_idx}__"),
                width: item.width,
                height: item.height,
                quantity: 1,
                can_rotate: item.can_rotate,
            })
            .collect(),
    };

    let solution = solve_2d(problem, TwoDOptions { algorithm, ..options.clone() })?;

    let mut placements: Vec<Placement2D> =
        solution.layouts.into_iter().flat_map(|layout| layout.placements).collect();

    // Restore the original item names by parsing the placeholder index
    // back into the `feasible_items` array.
    for placement in placements.iter_mut() {
        if let Some(idx_str) =
            placement.name.strip_prefix("__item_").and_then(|rest| rest.strip_suffix("__"))
            && let Ok(feasible_idx) = idx_str.parse::<usize>()
            && let Some((_, item)) = feasible_items.get(feasible_idx)
        {
            placement.name = item.name.clone();
        }
    }

    // Items that `solve_2d` failed to place go back into `leftover`.
    for demand in solution.unplaced {
        if let Some(idx_str) =
            demand.name.strip_prefix("__item_").and_then(|rest| rest.strip_suffix("__"))
            && let Ok(feasible_idx) = idx_str.parse::<usize>()
            && let Some((_, item)) = feasible_items.get(feasible_idx)
        {
            leftover.push((*item).clone());
        }
    }

    Ok((placements, leftover))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_respects_guillotine_requirement_and_can_take_multistart_candidate() {
        let guillotine_problem = TwoDProblem {
            sheets: vec![Sheet2D {
                name: "sheet".to_string(),
                width: 10,
                height: 10,
                cost: 1.0,
                quantity: Some(1),
            }],
            demands: vec![
                RectDemand2D {
                    name: "A".to_string(),
                    width: 6,
                    height: 6,
                    quantity: 1,
                    can_rotate: false,
                },
                RectDemand2D {
                    name: "B".to_string(),
                    width: 6,
                    height: 6,
                    quantity: 1,
                    can_rotate: false,
                },
            ],
        };
        let auto = solve_2d(
            guillotine_problem.clone(),
            TwoDOptions { guillotine_required: true, ..TwoDOptions::default() },
        )
        .expect("auto guillotine");
        let guillotine = solve_2d(
            guillotine_problem,
            TwoDOptions { algorithm: TwoDAlgorithm::Guillotine, ..TwoDOptions::default() },
        )
        .expect("guillotine");
        assert_eq!(auto.guillotine, guillotine.guillotine);
        assert_eq!(auto.unplaced.len(), guillotine.unplaced.len());
        assert_eq!(auto.total_waste_area, guillotine.total_waste_area);

        let multistart_problem = TwoDProblem {
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
        let maxrects = solve_2d(
            multistart_problem.clone(),
            TwoDOptions {
                algorithm: TwoDAlgorithm::MaxRects,
                seed: Some(11),
                ..TwoDOptions::default()
            },
        )
        .expect("maxrects");
        let bssf = solve_2d(
            multistart_problem.clone(),
            TwoDOptions {
                algorithm: TwoDAlgorithm::MaxRectsBestShortSideFit,
                seed: Some(11),
                ..TwoDOptions::default()
            },
        )
        .expect("bssf");
        let contact = solve_2d(
            multistart_problem.clone(),
            TwoDOptions {
                algorithm: TwoDAlgorithm::MaxRectsContactPoint,
                seed: Some(11),
                ..TwoDOptions::default()
            },
        )
        .expect("contact");
        let skyline = solve_2d(
            multistart_problem.clone(),
            TwoDOptions {
                algorithm: TwoDAlgorithm::Skyline,
                seed: Some(11),
                ..TwoDOptions::default()
            },
        )
        .expect("skyline");
        let skyline_min_waste = solve_2d(
            multistart_problem.clone(),
            TwoDOptions {
                algorithm: TwoDAlgorithm::SkylineMinWaste,
                seed: Some(11),
                ..TwoDOptions::default()
            },
        )
        .expect("skyline_min_waste");
        let multistart = solve_2d(
            multistart_problem.clone(),
            TwoDOptions {
                algorithm: TwoDAlgorithm::MultiStart,
                seed: Some(11),
                ..TwoDOptions::default()
            },
        )
        .expect("multistart");
        let auto =
            solve_2d(multistart_problem, TwoDOptions { seed: Some(11), ..TwoDOptions::default() })
                .expect("auto");

        assert!(multistart.is_better_than(&maxrects));
        assert!(multistart.is_better_than(&bssf));
        assert!(multistart.is_better_than(&contact));
        assert!(multistart.is_better_than(&skyline));
        assert!(multistart.is_better_than(&skyline_min_waste));
        assert_eq!(auto.sheet_count, multistart.sheet_count);
        assert_eq!(auto.total_waste_area, multistart.total_waste_area);
    }

    #[test]
    fn public_dispatch_exposes_shelf_strategies() {
        let problem = TwoDProblem {
            sheets: vec![Sheet2D {
                name: "sheet".to_string(),
                width: 10,
                height: 8,
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

        let ffdh = solve_2d(
            problem.clone(),
            TwoDOptions {
                algorithm: TwoDAlgorithm::FirstFitDecreasingHeight,
                ..TwoDOptions::default()
            },
        )
        .expect("ffdh");
        let bfdh = solve_2d(
            problem,
            TwoDOptions {
                algorithm: TwoDAlgorithm::BestFitDecreasingHeight,
                ..TwoDOptions::default()
            },
        )
        .expect("bfdh");

        assert_eq!(ffdh.algorithm, "first_fit_decreasing_height");
        assert_eq!(bfdh.algorithm, "best_fit_decreasing_height");
        assert_eq!(
            ffdh.layouts[0]
                .placements
                .iter()
                .find(|placement| placement.name == "C")
                .map(|placement| placement.y),
            Some(0),
        );
        assert_eq!(
            bfdh.layouts[0]
                .placements
                .iter()
                .find(|placement| placement.name == "C")
                .map(|placement| placement.y),
            Some(4),
        );
    }

    #[test]
    fn public_dispatch_exposes_new_strategy_variants() {
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

        let nfdh = solve_2d(
            problem.clone(),
            TwoDOptions {
                algorithm: TwoDAlgorithm::NextFitDecreasingHeight,
                ..TwoDOptions::default()
            },
        )
        .expect("nfdh");
        let bssf = solve_2d(
            problem.clone(),
            TwoDOptions {
                algorithm: TwoDAlgorithm::MaxRectsBestShortSideFit,
                ..TwoDOptions::default()
            },
        )
        .expect("bssf");
        let blsf = solve_2d(
            problem.clone(),
            TwoDOptions {
                algorithm: TwoDAlgorithm::MaxRectsBestLongSideFit,
                ..TwoDOptions::default()
            },
        )
        .expect("blsf");
        let bottom_left = solve_2d(
            problem.clone(),
            TwoDOptions { algorithm: TwoDAlgorithm::MaxRectsBottomLeft, ..TwoDOptions::default() },
        )
        .expect("bottom_left");
        let contact = solve_2d(
            problem.clone(),
            TwoDOptions {
                algorithm: TwoDAlgorithm::MaxRectsContactPoint,
                ..TwoDOptions::default()
            },
        )
        .expect("contact");
        let skyline_min_waste = solve_2d(
            problem.clone(),
            TwoDOptions { algorithm: TwoDAlgorithm::SkylineMinWaste, ..TwoDOptions::default() },
        )
        .expect("skyline_min_waste");
        let guillotine_bssf = solve_2d(
            problem.clone(),
            TwoDOptions {
                algorithm: TwoDAlgorithm::GuillotineBestShortSideFit,
                ..TwoDOptions::default()
            },
        )
        .expect("guillotine_bssf");
        let guillotine_slas = solve_2d(
            problem.clone(),
            TwoDOptions {
                algorithm: TwoDAlgorithm::GuillotineShorterLeftoverAxis,
                ..TwoDOptions::default()
            },
        )
        .expect("guillotine_slas");
        let guillotine_llas = solve_2d(
            problem.clone(),
            TwoDOptions {
                algorithm: TwoDAlgorithm::GuillotineLongerLeftoverAxis,
                ..TwoDOptions::default()
            },
        )
        .expect("guillotine_llas");
        let guillotine_min_area = solve_2d(
            problem.clone(),
            TwoDOptions {
                algorithm: TwoDAlgorithm::GuillotineMinAreaSplit,
                ..TwoDOptions::default()
            },
        )
        .expect("guillotine_min_area");
        let guillotine_max_area = solve_2d(
            problem,
            TwoDOptions {
                algorithm: TwoDAlgorithm::GuillotineMaxAreaSplit,
                ..TwoDOptions::default()
            },
        )
        .expect("guillotine_max_area");

        assert_eq!(nfdh.algorithm, "next_fit_decreasing_height");
        assert_eq!(bssf.algorithm, "max_rects_best_short_side_fit");
        assert_eq!(blsf.algorithm, "max_rects_best_long_side_fit");
        assert_eq!(bottom_left.algorithm, "max_rects_bottom_left");
        assert_eq!(contact.algorithm, "max_rects_contact_point");
        assert_eq!(skyline_min_waste.algorithm, "skyline_min_waste");
        assert_eq!(guillotine_bssf.algorithm, "guillotine_best_short_side_fit");
        assert_eq!(guillotine_slas.algorithm, "guillotine_shorter_leftover_axis");
        assert_eq!(guillotine_llas.algorithm, "guillotine_longer_leftover_axis");
        assert_eq!(guillotine_min_area.algorithm, "guillotine_min_area_split");
        assert_eq!(guillotine_max_area.algorithm, "guillotine_max_area_split");
    }

    /// Regression test: `TwoDAlgorithm::MultiStart` must dispatch to the
    /// standalone multistart solver, not to `solve_auto`. Earlier code
    /// collapsed the match arm into `MultiStart | Auto => solve_auto(...)`,
    /// which silently promoted `MultiStart` into a full meta-search.
    #[test]
    fn multistart_variant_dispatches_to_multistart_solver() {
        let problem = TwoDProblem {
            sheets: vec![Sheet2D {
                name: "sheet".to_string(),
                width: 10,
                height: 10,
                cost: 1.0,
                quantity: None,
            }],
            demands: vec![RectDemand2D {
                name: "A".to_string(),
                width: 4,
                height: 4,
                quantity: 4,
                can_rotate: true,
            }],
        };

        let multistart = solve_2d(
            problem,
            TwoDOptions {
                algorithm: TwoDAlgorithm::MultiStart,
                seed: Some(1),
                ..TwoDOptions::default()
            },
        )
        .expect("multistart");

        assert_eq!(multistart.algorithm, "multi_start");
    }

    #[test]
    fn place_into_sheet_packs_items_with_each_algorithm() {
        use super::model::ItemInstance2D;
        use super::place_into_sheet;

        let items = vec![
            ItemInstance2D { name: "a".into(), width: 4, height: 4, can_rotate: false },
            ItemInstance2D { name: "b".into(), width: 4, height: 4, can_rotate: false },
            ItemInstance2D { name: "c".into(), width: 4, height: 4, can_rotate: false },
            ItemInstance2D { name: "d".into(), width: 4, height: 4, can_rotate: false },
        ];
        let options = TwoDOptions::default();

        for algorithm in [
            TwoDAlgorithm::MaxRects,
            TwoDAlgorithm::MaxRectsBestShortSideFit,
            TwoDAlgorithm::MaxRectsBottomLeft,
            TwoDAlgorithm::Skyline,
            TwoDAlgorithm::SkylineMinWaste,
            TwoDAlgorithm::Guillotine,
            TwoDAlgorithm::FirstFitDecreasingHeight,
            TwoDAlgorithm::BestFitDecreasingHeight,
            TwoDAlgorithm::Auto,
        ] {
            let (placements, leftover) =
                place_into_sheet(&items, 10, 10, algorithm, &options).expect("place");
            assert_eq!(placements.len() + leftover.len(), 4, "{:?}", algorithm);
            assert!(placements.len() >= 2, "{:?} should fit at least two", algorithm);
            for placement in &placements {
                assert!(placement.x + placement.width <= 10);
                assert!(placement.y + placement.height <= 10);
            }
        }
    }

    #[test]
    fn place_into_sheet_returns_unplaced_when_oversized() {
        use super::model::ItemInstance2D;
        use super::place_into_sheet;

        let items =
            vec![ItemInstance2D { name: "huge".into(), width: 20, height: 20, can_rotate: false }];
        let (placements, leftover) =
            place_into_sheet(&items, 10, 10, TwoDAlgorithm::MaxRects, &TwoDOptions::default())
                .expect("place");
        assert!(placements.is_empty());
        assert_eq!(leftover.len(), 1);
    }
}
