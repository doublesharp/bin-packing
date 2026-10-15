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

/// O(d + p) assignment for d demand types and p expanded pieces. A vector of
/// words supports sampled assignments of any validated demand count.
fn apply_rotation_mask(
    problem: &TwoDProblem,
    rotatable_indices: &[usize],
    mask: &[u64],
) -> Vec<ItemInstance2D> {
    let mut rotatable = rotatable_indices.iter().copied().enumerate().peekable();
    let mut items = Vec::new();
    for (demand_idx, demand) in problem.demands.iter().enumerate() {
        let fixed_rotation = if let Some(&(bit, index)) = rotatable.peek()
            && index == demand_idx
        {
            rotatable.next();
            Some(mask[bit / 64] & (1u64 << (bit % 64)) != 0)
        } else {
            None
        };
        for _ in 0..demand.quantity {
            items.push(ItemInstance2D {
                name: demand.name.clone(),
                width: demand.width,
                height: demand.height,
                can_rotate: demand.can_rotate,
                fixed_rotation,
            });
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
    // At most 65,536 exhaustive assignments, even if callers raise the threshold.
    // Larger searches use the caller's sampled run budget.
    let exhaustive = k <= options.auto_rotation_search_max_types.min(16);
    let runs = if exhaustive { 1usize << k } else { options.multistart_runs.max(1) };
    let base_seed = options.seed.unwrap_or(0x524F_5441_5445_5253);
    let mut best: Option<TwoDSolution> = None;
    let mut last_error = None;

    // Retain at most 32 completed candidates, instead of every assignment's
    // layouts. Ordered batches preserve deterministic tie and error selection.
    for start in (0..runs).step_by(32) {
        let count = (runs - start).min(32);
        let results = crate::parallel::par_map_indexed(count, |offset| {
            let run = start + offset;
            let mask = if exhaustive {
                vec![run as u64]
            } else {
                let mut rng =
                    SmallRng::seed_from_u64(crate::parallel::iteration_seed(base_seed, run));
                (0..k.div_ceil(64)).map(|_| rng.next_u64()).collect()
            };
            let mut items = apply_rotation_mask(problem, &rotatable, &mask);
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
        for result in results {
            match result {
                Ok(solution) => {
                    if best.as_ref().is_none_or(|b| solution.is_better_than(b)) {
                        best = Some(solution);
                    }
                }
                Err(error) => last_error = Some(error),
            }
        }
    }
    match best {
        Some(mut solution) => {
            solution.metrics.notes.push(if exhaustive {
                format!("exhaustive rotation search over {k} rotatable types ({runs} assignments)")
            } else {
                format!("sampled rotation search: {runs} of 2^{k} assignments")
            });
            Ok(solution)
        }
        None => Err(last_error.unwrap_or_else(|| {
            crate::BinPackingError::Unsupported(
                "rotation_search: no assignment succeeded".to_string(),
            )
        })),
    }
}

/// Bounded mixed counts and orders. Count assignments constrain each individual
/// instance while preserving its original dimensions and grain permission.
/// Candidate MaxRects packing is heuristic, even if every count vector is tried.
pub(super) fn solve_mixed_rotations(
    problem: TwoDProblem,
    mut options: TwoDOptions,
) -> Result<TwoDSolution> {
    use super::bounded_search;
    use rand::seq::SliceRandom;
    let indexed = bounded_search::prepare(&problem, &options)?;
    options.seed = Some(options.seed.unwrap_or(0));
    let mut best = bounded_search::baseline(&indexed, &options)?;
    if options.guillotine_required {
        best.metrics
            .notes
            .push("mixed-rotation MaxRects search skipped for required guillotine layouts".into());
        return bounded_search::restore(&problem, &indexed, best);
    }
    let baseline_states = best.metrics.explored_states;
    let baseline_iterations = best.metrics.iterations;
    let rotatable = rotatable_demand_indices(&indexed);
    let combinations = rotatable
        .iter()
        .fold(1usize, |n, &i| n.saturating_mul(indexed.demands[i].quantity.saturating_add(1)));
    let max_trials = options.multistart_runs.clamp(1, 16) * 8;
    let assignment_limit = max_trials.div_ceil(3);
    let assignment_count = combinations.min(assignment_limit);
    let exhaustive_counts = combinations <= assignment_limit;
    let mut trials = 0usize;
    let mut rejected = 0usize;
    for assignment in 0..assignment_count {
        let mut rng = SmallRng::seed_from_u64(crate::parallel::iteration_seed(
            options.seed.unwrap_or(0),
            assignment,
        ));
        let mut residue = assignment;
        let counts: Vec<_> = rotatable
            .iter()
            .map(|&i| {
                let quantity = indexed.demands[i].quantity;
                if exhaustive_counts {
                    let count = residue % (quantity + 1);
                    residue /= quantity + 1;
                    count
                } else {
                    match assignment {
                        0 => quantity / 2,
                        1 => 0,
                        2 => quantity,
                        _ => (rng.next_u64() % (quantity as u64 + 1)) as usize,
                    }
                }
            })
            .collect();
        for order in 0..3 {
            if trials == max_trials {
                break;
            }
            let mut items = Vec::new();
            for (demand_index, demand) in indexed.demands.iter().enumerate() {
                let count = rotatable.iter().position(|&i| i == demand_index).map(|i| counts[i]);
                for occurrence in 0..demand.quantity {
                    let fixed_rotation = count.map(|count| {
                        if order == 1 {
                            occurrence < count.div_ceil(2)
                                || occurrence >= demand.quantity - count / 2
                        } else {
                            occurrence < count
                        }
                    });
                    items.push(ItemInstance2D {
                        name: demand.name.clone(),
                        width: demand.width,
                        height: demand.height,
                        can_rotate: demand.can_rotate,
                        fixed_rotation,
                    });
                }
            }
            if order == 2 {
                items.shuffle(&mut rng);
            } else {
                maxrects::sort_items_descending(&mut items);
            }
            trials += 1;
            let candidate = maxrects::pack_with_order(
                &indexed,
                &options,
                &items,
                "mixed_rotations",
                trials,
                MaxRectsStrategy::BestAreaFit,
            )?;
            if bounded_search::validate_solution(&indexed, &candidate).is_err() {
                rejected += 1;
                continue;
            }
            if candidate.is_better_than(&best) {
                best = candidate;
            }
        }
    }
    best.metrics.iterations = baseline_iterations.saturating_add(trials);
    best.metrics.explored_states = baseline_states.saturating_add(trials);
    best.metrics.notes.push(format!("opt-in mixed rotations: {assignment_count} count assignments, {trials} packing/order trials, {rejected} rejected layouts; exhaustive count vectors={exhaustive_counts}; placement/order coverage is heuristic; baseline retained unless improved"));
    bounded_search::restore(&problem, &indexed, best)
}
