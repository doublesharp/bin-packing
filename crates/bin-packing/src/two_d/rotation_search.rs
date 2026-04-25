//! Exhaustive (or sampled) rotation assignment search for 2D bin packing.
//!
//! Enumerates all 2^k rotation assignments for k rotatable demand types, or
//! samples `multistart_runs` random assignments when k exceeds the configured
//! threshold. Each assignment fixes rotations and packs via MaxRects
//! best-area-fit.

use rand::{RngCore, SeedableRng, rngs::SmallRng};

use crate::Result;

use super::maxrects::{self, MaxRectsStrategy};
use super::model::{ItemInstance2D, TwoDOptions, TwoDProblem, TwoDSolution};

/// Identifies the distinct rotatable demand types. A demand type is rotatable
/// when `can_rotate == true` AND `width != height` (square items gain nothing
/// from rotation). Returns the indices into `problem.demands` that are
/// rotatable.
fn rotatable_demand_indices(problem: &TwoDProblem) -> Vec<usize> {
    problem
        .demands
        .iter()
        .enumerate()
        .filter(|(_, d)| d.can_rotate && d.width != d.height)
        .map(|(i, _)| i)
        .collect()
}

/// Build an expanded items list with a specific rotation assignment applied.
/// `mask` is a bitmask where bit `i` (of `rotatable_indices`) indicates that
/// the corresponding demand type should have its width/height swapped and
/// `can_rotate` set to `false`.
fn apply_rotation_mask(
    problem: &TwoDProblem,
    rotatable_indices: &[usize],
    mask: u64,
) -> Vec<ItemInstance2D> {
    let mut items = Vec::new();
    for (demand_idx, demand) in problem.demands.iter().enumerate() {
        // Check if this demand is in the rotatable set, and if so, which bit.
        let bit_position = rotatable_indices.iter().position(|&ri| ri == demand_idx);
        let rotate = if let Some(pos) = bit_position { mask & (1u64 << pos) != 0 } else { false };

        for _ in 0..demand.quantity {
            if rotate {
                items.push(ItemInstance2D {
                    name: demand.name.clone(),
                    width: demand.height,
                    height: demand.width,
                    can_rotate: false,
                });
            } else if bit_position.is_some() {
                // Rotatable type but not rotated in this assignment — fix orientation.
                items.push(ItemInstance2D {
                    name: demand.name.clone(),
                    width: demand.width,
                    height: demand.height,
                    can_rotate: false,
                });
            } else {
                // Not a rotatable type — preserve original can_rotate.
                items.push(ItemInstance2D {
                    name: demand.name.clone(),
                    width: demand.width,
                    height: demand.height,
                    can_rotate: demand.can_rotate,
                });
            }
        }
    }
    items
}

/// Solve 2D bin packing by exhaustively (or via sampling) searching over
/// rotation assignments for rotatable demand types.
pub(super) fn solve_rotation_search(
    problem: &TwoDProblem,
    options: &TwoDOptions,
) -> Result<TwoDSolution> {
    let rotatable = rotatable_demand_indices(problem);
    let k = rotatable.len();
    let max_types = options.auto_rotation_search_max_types;

    // Determine whether to enumerate exhaustively or sample.
    let exhaustive = k <= max_types && k <= 63;
    let total_assignments: u64 = if exhaustive { 1u64 << k } else { 0 };

    let base_seed = options.seed.unwrap_or(0x524F_5441_5445_5253);

    if exhaustive {
        // Enumerate all 2^k assignments.
        let n = total_assignments as usize;

        let results = crate::parallel::par_map_indexed(n, |i| {
            let mask = i as u64;
            let mut items = apply_rotation_mask(problem, &rotatable, mask);
            maxrects::sort_items_descending(&mut items);
            maxrects::pack_with_order(
                problem,
                options,
                &items,
                "rotation_search",
                i + 1,
                MaxRectsStrategy::BestAreaFit,
            )
        });

        let mut best: Option<TwoDSolution> = None;
        let mut last_error: Option<crate::BinPackingError> = None;
        for result in results {
            match result {
                Ok(sol) => {
                    if best.as_ref().is_none_or(|b| sol.is_better_than(b)) {
                        best = Some(sol);
                    }
                }
                Err(e) => last_error = Some(e),
            }
        }

        match best {
            Some(mut sol) => {
                sol.algorithm = "rotation_search".to_string();
                sol.metrics.notes.push(format!(
                    "exhaustive rotation search over {k} rotatable types ({n} assignments)"
                ));
                Ok(sol)
            }
            None => Err(last_error.unwrap_or_else(|| {
                crate::BinPackingError::Unsupported(
                    "rotation_search: no assignment succeeded".to_string(),
                )
            })),
        }
    } else {
        // Sample multistart_runs random assignments.
        let runs = options.multistart_runs.max(1);

        let results = crate::parallel::par_map_indexed(runs, |run| {
            let mut rng = SmallRng::seed_from_u64(crate::parallel::iteration_seed(base_seed, run));
            let mask = rng.next_u64();
            let mut items = apply_rotation_mask(problem, &rotatable, mask);
            maxrects::sort_items_descending(&mut items);
            maxrects::pack_with_order(
                problem,
                options,
                &items,
                "rotation_search",
                run + 1,
                MaxRectsStrategy::BestAreaFit,
            )
        });

        let mut best: Option<TwoDSolution> = None;
        let mut last_error: Option<crate::BinPackingError> = None;
        for result in results {
            match result {
                Ok(sol) => {
                    if best.as_ref().is_none_or(|b| sol.is_better_than(b)) {
                        best = Some(sol);
                    }
                }
                Err(e) => last_error = Some(e),
            }
        }

        match best {
            Some(mut sol) => {
                sol.algorithm = "rotation_search".to_string();
                sol.metrics
                    .notes
                    .push(format!("sampled rotation search: {runs} of 2^{k} assignments"));
                Ok(sol)
            }
            None => Err(last_error.unwrap_or_else(|| {
                crate::BinPackingError::Unsupported(
                    "rotation_search: no sampled assignment succeeded".to_string(),
                )
            })),
        }
    }
}
