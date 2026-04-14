use rand::{Rng, SeedableRng, prelude::SliceRandom, rngs::SmallRng};

use crate::{BinPackingError, Result};

use super::model::{
    OneDOptions, OneDProblem, OneDSolution, PackedBin, PieceInstance, SolverMetrics1D,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PlacementStrategy {
    FirstFit,
    BestFit,
}

pub(super) fn solve_ffd(problem: &OneDProblem, _options: &OneDOptions) -> Result<OneDSolution> {
    let mut pieces = problem.expanded_pieces();
    sort_pieces_descending(&mut pieces);

    let (bins, unplaced) = pack_ordered(problem, pieces, PlacementStrategy::FirstFit)?;

    Ok(OneDSolution::from_bins(
        "first_fit_decreasing",
        false,
        None,
        &problem.stock,
        &bins,
        &unplaced,
        SolverMetrics1D {
            iterations: 1,
            generated_patterns: 0,
            enumerated_patterns: 0,
            explored_states: 0,
            notes: vec!["deterministic descending construction".to_string()],
        },
    ))
}

pub(super) fn solve_bfd(problem: &OneDProblem, _options: &OneDOptions) -> Result<OneDSolution> {
    let mut pieces = problem.expanded_pieces();
    sort_pieces_descending(&mut pieces);

    let (bins, unplaced) = pack_ordered(problem, pieces, PlacementStrategy::BestFit)?;

    Ok(OneDSolution::from_bins(
        "best_fit_decreasing",
        false,
        None,
        &problem.stock,
        &bins,
        &unplaced,
        SolverMetrics1D {
            iterations: 1,
            generated_patterns: 0,
            enumerated_patterns: 0,
            explored_states: 0,
            notes: vec!["deterministic best-fit-decreasing construction".to_string()],
        },
    ))
}

pub(super) fn solve_local_search(
    problem: &OneDProblem,
    options: &OneDOptions,
) -> Result<OneDSolution> {
    let mut baseline_pieces = problem.expanded_pieces();
    sort_pieces_descending(&mut baseline_pieces);

    let (initial_ffd, initial_ffd_unplaced) =
        pack_ordered(problem, baseline_pieces.clone(), PlacementStrategy::FirstFit)?;
    let (initial_bfd, initial_bfd_unplaced) =
        pack_ordered(problem, baseline_pieces.clone(), PlacementStrategy::BestFit)?;

    let mut best_solution = OneDSolution::from_bins(
        "local_search",
        false,
        None,
        &problem.stock,
        &initial_ffd,
        &initial_ffd_unplaced,
        SolverMetrics1D {
            iterations: 1,
            generated_patterns: 0,
            enumerated_patterns: 0,
            explored_states: 0,
            notes: Vec::new(),
        },
    );
    // Track the origin of the current best separately from the per-solution
    // notes so that later `best_solution = candidate` assignments cannot clobber
    // the "started from ..." trail.
    let mut best_start_note = "started from first_fit_decreasing".to_string();

    let candidate = OneDSolution::from_bins(
        "local_search",
        false,
        None,
        &problem.stock,
        &initial_bfd,
        &initial_bfd_unplaced,
        SolverMetrics1D {
            iterations: 1,
            generated_patterns: 0,
            enumerated_patterns: 0,
            explored_states: 0,
            notes: Vec::new(),
        },
    );

    if candidate.is_better_than(&best_solution) {
        best_solution = candidate;
        best_start_note = "started from best_fit_decreasing".to_string();
    }

    let base_seed = options.seed.unwrap_or(DEFAULT_SEED);
    let mut iterations = 2;
    let mut best_notes = vec![
        best_start_note,
        "combines multistart reorderings with a bin-elimination repair pass".to_string(),
    ];

    let runs = options.multistart_runs.max(1);
    let run_results = crate::parallel::par_map_indexed(runs, |run| {
        let mut rng = SmallRng::seed_from_u64(crate::parallel::iteration_seed(base_seed, run));
        let mut pieces = baseline_pieces.clone();
        perturb_piece_order(&mut pieces, &mut rng);

        let strategy =
            if run % 2 == 0 { PlacementStrategy::BestFit } else { PlacementStrategy::FirstFit };

        let pack_result = pack_ordered(problem, pieces, strategy);
        match pack_result {
            Ok((mut bins, unplaced)) => {
                if unplaced.is_empty() {
                    eliminate_bins(problem, &mut bins, options.improvement_rounds);
                }
                Ok(OneDSolution::from_bins(
                    "local_search",
                    false,
                    None,
                    &problem.stock,
                    &bins,
                    &unplaced,
                    SolverMetrics1D {
                        iterations: 0,
                        generated_patterns: 0,
                        enumerated_patterns: 0,
                        explored_states: 0,
                        notes: vec![format!(
                            "run {} used {}",
                            run + 1,
                            match strategy {
                                PlacementStrategy::FirstFit => "first_fit",
                                PlacementStrategy::BestFit => "best_fit",
                            }
                        )],
                    },
                ))
            }
            Err(err) => Err(err),
        }
    });

    for (run, result) in run_results.into_iter().enumerate() {
        if let Ok(solution) = result
            && solution.is_better_than(&best_solution)
        {
            best_notes.push(format!("accepted improved run {}", run + 1));
            best_solution = solution;
        }
        iterations += 1;
    }

    best_solution.metrics.iterations = iterations;
    best_solution.metrics.notes.extend(best_notes);
    Ok(best_solution)
}

// Default deterministic seed for reproducible runs when the caller doesn't
// supply one. The literal encodes "BINPACK0" in ASCII so `grep` can find it.
const DEFAULT_SEED: u64 = 0x4249_4E50_4143_4B30;

#[cfg(test)]
fn seeded_rng(seed: Option<u64>) -> SmallRng {
    match seed {
        Some(seed) => SmallRng::seed_from_u64(seed),
        None => SmallRng::seed_from_u64(DEFAULT_SEED),
    }
}

fn pack_ordered(
    problem: &OneDProblem,
    ordered_pieces: Vec<PieceInstance>,
    strategy: PlacementStrategy,
) -> Result<(Vec<PackedBin>, Vec<PieceInstance>)> {
    let mut bins: Vec<PackedBin> = Vec::new();
    let mut usage_counts = vec![0_usize; problem.stock.len()];
    let mut unplaced = Vec::new();

    for piece in ordered_pieces {
        if let Some(bin_index) = choose_existing_bin(problem, &bins, &piece, strategy) {
            let stock_index = bins[bin_index].stock_index;
            let stock = &problem.stock[stock_index];
            let inserted = bins[bin_index].add_piece(piece, stock);
            debug_assert!(inserted, "existing feasibility check failed");
            continue;
        }

        if let Some(stock_index) = choose_new_stock(problem, &piece, &usage_counts) {
            let stock = &problem.stock[stock_index];
            let mut bin = PackedBin::new(stock_index);
            let piece_name = piece.name.clone();
            let piece_length = piece.length;
            let inserted = bin.add_piece(piece, stock);

            // `choose_new_stock` already filtered by `stock.usable_length() >=
            // piece.length`, so a fresh empty bin must accept the piece. If it
            // doesn't we have an internal invariant violation — report it as
            // `Unsupported` so callers can distinguish it from a legitimate
            // infeasible-demand error.
            debug_assert!(inserted, "fresh bin rejected a feasibility-checked piece");
            if !inserted {
                return Err(BinPackingError::Unsupported(format!(
                    "internal invariant violation: fresh bin rejected piece \
                    `{piece_name}` of length {piece_length}"
                )));
            }

            bins.push(bin);
            usage_counts[stock_index] = usage_counts[stock_index].saturating_add(1);
        } else {
            unplaced.push(piece);
        }
    }

    Ok((bins, unplaced))
}

fn choose_existing_bin(
    problem: &OneDProblem,
    bins: &[PackedBin],
    piece: &PieceInstance,
    strategy: PlacementStrategy,
) -> Option<usize> {
    match strategy {
        PlacementStrategy::FirstFit => bins.iter().enumerate().find_map(|(index, bin)| {
            let stock = &problem.stock[bin.stock_index];
            bin.can_fit_piece(piece, stock).then_some(index)
        }),
        PlacementStrategy::BestFit => bins
            .iter()
            .enumerate()
            .filter_map(|(index, bin)| {
                let stock = &problem.stock[bin.stock_index];
                let remaining = bin.remaining_length(stock);
                let delta = bin.delta_for_piece(piece, stock)?;
                Some((index, remaining.saturating_sub(delta), stock.cost))
            })
            .min_by(|left, right| left.1.cmp(&right.1).then_with(|| left.2.total_cmp(&right.2)))
            .map(|(index, _, _)| index),
    }
}

fn choose_new_stock(
    problem: &OneDProblem,
    piece: &PieceInstance,
    usage_counts: &[usize],
) -> Option<usize> {
    problem
        .stock
        .iter()
        .enumerate()
        .filter(|(index, stock)| {
            stock.usable_length() >= piece.length
                && stock.available.map(|available| usage_counts[*index] < available).unwrap_or(true)
        })
        .min_by(|left, right| {
            let left_waste = left.1.usable_length().saturating_sub(piece.length);
            let right_waste = right.1.usable_length().saturating_sub(piece.length);

            left_waste
                .cmp(&right_waste)
                .then_with(|| left.1.cost.total_cmp(&right.1.cost))
                .then_with(|| left.1.length.cmp(&right.1.length))
        })
        .map(|(index, _)| index)
}

fn eliminate_bins(problem: &OneDProblem, bins: &mut Vec<PackedBin>, rounds: usize) {
    for _ in 0..rounds {
        let mut removed_any = false;
        let mut indices = (0..bins.len()).collect::<Vec<_>>();
        indices.sort_by(|left, right| {
            let left_fill = bins[*left].pieces.len();
            let right_fill = bins[*right].pieces.len();
            left_fill.cmp(&right_fill)
        });

        for index in indices {
            // We break out of this loop on the first successful elimination, so
            // `index` always refers to the correct bin within a single round.
            let candidate_pieces = bins[index].pieces.clone();
            let mut rebuilt = bins
                .iter()
                .enumerate()
                .filter_map(|(current, bin)| (current != index).then_some(bin.clone()))
                .collect::<Vec<_>>();

            if try_insert_without_opening(problem, &mut rebuilt, &candidate_pieces) {
                *bins = rebuilt;
                removed_any = true;
                break;
            }
        }

        if !removed_any {
            break;
        }
    }
}

fn try_insert_without_opening(
    problem: &OneDProblem,
    bins: &mut [PackedBin],
    pieces: &[PieceInstance],
) -> bool {
    let mut pieces = pieces.to_vec();
    sort_pieces_descending(&mut pieces);

    for piece in pieces {
        let candidate = bins
            .iter()
            .enumerate()
            .filter_map(|(index, bin)| {
                let stock = &problem.stock[bin.stock_index];
                let delta = bin.delta_for_piece(&piece, stock)?;
                Some((index, bin.remaining_length(stock).saturating_sub(delta)))
            })
            .min_by_key(|(_, remaining_after)| *remaining_after)
            .map(|(index, _)| index);

        let Some(index) = candidate else {
            return false;
        };

        let stock_index = bins[index].stock_index;
        let inserted = bins[index].add_piece(piece, &problem.stock[stock_index]);
        debug_assert!(inserted, "best-fit reinsertion became infeasible");
    }

    true
}

fn perturb_piece_order(pieces: &mut [PieceInstance], rng: &mut SmallRng) {
    pieces.shuffle(rng);
    sort_pieces_descending(pieces);

    let swap_count = (pieces.len() / 4).max(1);
    for _ in 0..swap_count {
        let left = rng.random_range(0..pieces.len());
        let right = rng.random_range(0..pieces.len());
        if pieces[left].length.abs_diff(pieces[right].length) <= 5 {
            pieces.swap(left, right);
        }
    }
}

fn sort_pieces_descending(pieces: &mut [PieceInstance]) {
    pieces.sort_by(|left, right| {
        right.length.cmp(&left.length).then_with(|| left.demand_index.cmp(&right.demand_index))
    });
}

#[cfg(test)]
mod tests {
    use rand::RngCore;

    use super::*;
    use crate::one_d::model::{CutDemand1D, OneDAlgorithm, OneDOptions, OneDProblem, Stock1D};

    fn problem() -> OneDProblem {
        OneDProblem {
            stock: vec![Stock1D {
                name: "bar".to_string(),
                length: 100,
                kerf: 0,
                trim: 0,
                cost: 1.0,
                available: None,
            }],
            demands: vec![
                CutDemand1D { name: "A".to_string(), length: 45, quantity: 2 },
                CutDemand1D { name: "B".to_string(), length: 30, quantity: 2 },
            ],
        }
    }

    #[test]
    fn best_fit_decreasing_finds_two_stock_solution() {
        let solution = solve_bfd(&problem(), &OneDOptions::default()).expect("solver should work");
        assert_eq!(solution.stock_count, 2);
        assert!(solution.unplaced.is_empty());
    }

    #[test]
    fn local_search_never_worsens_baseline() {
        let options =
            OneDOptions { algorithm: OneDAlgorithm::LocalSearch, ..OneDOptions::default() };

        let ffd = solve_ffd(&problem(), &options).expect("ffd");
        let local = solve_local_search(&problem(), &options).expect("local");
        assert!(local.stock_count <= ffd.stock_count);
    }

    /// Regression test: local search must preserve the "started from ..." note
    /// even when a multistart-loop iteration swaps in a better candidate. A
    /// prior version assigned `best_solution = solution` inside the loop,
    /// which clobbered the initial note before it could be appended.
    #[test]
    fn local_search_preserves_started_from_note_through_swaps() {
        let options = OneDOptions {
            algorithm: OneDAlgorithm::LocalSearch,
            seed: Some(7),
            ..OneDOptions::default()
        };
        let solution = solve_local_search(&problem(), &options).expect("local");
        assert!(
            solution.metrics.notes.iter().any(|note| note.starts_with("started from ")),
            "expected a `started from ...` note, got {:?}",
            solution.metrics.notes
        );
    }

    #[test]
    fn seeded_rng_uses_a_stable_default_seed() {
        let mut first = seeded_rng(None);
        let mut second = seeded_rng(None);
        assert_eq!(first.next_u64(), second.next_u64());
    }

    #[test]
    fn pack_ordered_marks_pieces_unplaced_when_inventory_is_exhausted() {
        let problem = OneDProblem {
            stock: vec![Stock1D {
                name: "limited".to_string(),
                length: 5,
                kerf: 0,
                trim: 0,
                cost: 1.0,
                available: Some(1),
            }],
            demands: vec![CutDemand1D { name: "cut".to_string(), length: 3, quantity: 2 }],
        };

        let pieces = problem.expanded_pieces();
        let (bins, unplaced) =
            pack_ordered(&problem, pieces, PlacementStrategy::FirstFit).expect("packing");

        assert_eq!(bins.len(), 1);
        assert_eq!(unplaced.len(), 1);
        assert_eq!(unplaced[0].name, "cut");
    }

    #[test]
    fn choose_new_stock_prefers_lower_waste_then_cost_then_length() {
        let piece = PieceInstance { demand_index: 0, name: "piece".to_string(), length: 8 };

        let lower_waste = OneDProblem {
            stock: vec![
                Stock1D {
                    name: "wide".to_string(),
                    length: 12,
                    kerf: 0,
                    trim: 0,
                    cost: 1.0,
                    available: None,
                },
                Stock1D {
                    name: "snug".to_string(),
                    length: 9,
                    kerf: 0,
                    trim: 0,
                    cost: 2.0,
                    available: None,
                },
            ],
            demands: vec![CutDemand1D { name: "piece".to_string(), length: 8, quantity: 1 }],
        };
        assert_eq!(choose_new_stock(&lower_waste, &piece, &[0, 0]), Some(1));

        let lower_cost = OneDProblem {
            stock: vec![
                Stock1D {
                    name: "costly".to_string(),
                    length: 10,
                    kerf: 0,
                    trim: 0,
                    cost: 2.0,
                    available: None,
                },
                Stock1D {
                    name: "cheap".to_string(),
                    length: 10,
                    kerf: 0,
                    trim: 0,
                    cost: 1.0,
                    available: None,
                },
            ],
            demands: vec![CutDemand1D { name: "piece".to_string(), length: 8, quantity: 1 }],
        };
        assert_eq!(choose_new_stock(&lower_cost, &piece, &[0, 0]), Some(1));

        let shorter_stock = OneDProblem {
            stock: vec![
                Stock1D {
                    name: "long".to_string(),
                    length: 11,
                    kerf: 0,
                    trim: 0,
                    cost: 1.0,
                    available: None,
                },
                Stock1D {
                    name: "short".to_string(),
                    length: 10,
                    kerf: 0,
                    trim: 0,
                    cost: 1.0,
                    available: None,
                },
            ],
            demands: vec![CutDemand1D { name: "piece".to_string(), length: 8, quantity: 1 }],
        };
        assert_eq!(choose_new_stock(&shorter_stock, &piece, &[0, 0]), Some(1));
    }

    #[test]
    fn elimination_and_reinsertion_cover_success_and_failure_paths() {
        let problem = OneDProblem {
            stock: vec![Stock1D {
                name: "bar".to_string(),
                length: 10,
                kerf: 0,
                trim: 0,
                cost: 1.0,
                available: None,
            }],
            demands: vec![CutDemand1D { name: "piece".to_string(), length: 1, quantity: 1 }],
        };

        let removable_piece =
            PieceInstance { demand_index: 0, name: "small".to_string(), length: 4 };
        let anchor_piece = PieceInstance { demand_index: 1, name: "large".to_string(), length: 6 };
        let stock = &problem.stock[0];
        let mut first = PackedBin::new(0);
        assert!(first.add_piece(anchor_piece.clone(), stock));
        let mut second = PackedBin::new(0);
        assert!(second.add_piece(removable_piece.clone(), stock));
        let mut removable_bins = vec![first, second];

        eliminate_bins(&problem, &mut removable_bins, 1);
        assert_eq!(removable_bins.len(), 1);
        assert_eq!(removable_bins[0].pieces.len(), 2);

        let mut full_first = PackedBin::new(0);
        assert!(full_first.add_piece(anchor_piece.clone(), stock));
        let mut full_second = PackedBin::new(0);
        assert!(full_second.add_piece(anchor_piece.clone(), stock));
        let mut full_bins = vec![full_first, full_second];
        eliminate_bins(&problem, &mut full_bins, 1);
        assert_eq!(full_bins.len(), 2);

        let mut reinsertion = PackedBin::new(0);
        assert!(reinsertion.add_piece(anchor_piece, stock));
        let mut reinsertion_target = [reinsertion];
        assert!(try_insert_without_opening(&problem, &mut reinsertion_target, &[removable_piece]));
        assert_eq!(reinsertion_target[0].pieces.len(), 2);

        let oversized = PieceInstance { demand_index: 2, name: "too_big".to_string(), length: 8 };
        assert!(!try_insert_without_opening(&problem, &mut reinsertion_target, &[oversized]));
    }
}
