use std::{
    collections::{HashMap, HashSet},
    fmt,
};

use crate::{BinPackingError, Result};

use super::{
    heuristics,
    model::{
        CutDemand1D, OneDOptions, OneDProblem, OneDSolution, PackedBin, PieceInstance,
        SolverMetrics1D, Stock1D,
    },
};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Pattern {
    counts: Vec<usize>,
}

struct BuildContext<'a> {
    stock: &'a Stock1D,
    demands: &'a [&'a CutDemand1D],
    lengths: &'a [u32],
    exact: bool,
    lower_bound: f64,
    generated_patterns: usize,
    enumerated_patterns: usize,
    explored_states: usize,
}

impl Pattern {
    fn used_length(&self, stock: &Stock1D, lengths: &[u32]) -> u32 {
        let mut pieces = 0_u32;
        let mut total = 0_u32;

        for (count, length) in self.counts.iter().zip(lengths) {
            pieces = pieces.saturating_add(*count as u32);
            total = total.saturating_add(length.saturating_mul(*count as u32));
        }

        if pieces > 1 { total.saturating_add(stock.kerf.saturating_mul(pieces - 1)) } else { total }
    }

    fn total_pieces(&self) -> usize {
        self.counts.iter().sum()
    }
}

pub(super) fn solve_exact(problem: &OneDProblem, options: &OneDOptions) -> Result<OneDSolution> {
    if problem.stock.len() != 1 {
        return Err(BinPackingError::Unsupported(
            "column generation currently supports exactly one stock type".to_string(),
        ));
    }

    let stock = &problem.stock[0];
    if stock.available.is_some() {
        return Err(BinPackingError::Unsupported(
            "column generation currently assumes unlimited stock availability".to_string(),
        ));
    }

    let demands = problem.demands.iter().filter(|demand| demand.quantity > 0).collect::<Vec<_>>();

    let lengths = demands.iter().map(|demand| demand.length).collect::<Vec<_>>();
    let quantities = demands.iter().map(|demand| demand.quantity).collect::<Vec<_>>();
    let weights =
        lengths.iter().map(|length| length.saturating_add(stock.kerf)).collect::<Vec<_>>();
    let capacity = stock.adjusted_capacity();

    if demands.is_empty() {
        return Ok(OneDSolution::from_bins(
            "column_generation",
            true,
            Some(0.0),
            std::slice::from_ref(stock),
            &[],
            &[],
            SolverMetrics1D {
                iterations: 0,
                generated_patterns: 0,
                enumerated_patterns: 0,
                explored_states: 0,
                notes: vec!["empty demand set".to_string()],
            },
        ));
    }

    let mut heuristic = heuristics::solve_local_search(problem, options)?;

    let mut patterns = initial_patterns(&quantities, &weights, capacity);
    let mut seen = patterns.iter().map(|pattern| pattern.counts.clone()).collect::<HashSet<_>>();
    let mut generated_patterns = 0;
    let mut lp_lower_bound = 0.0_f64;

    for _ in 0..options.column_generation_rounds {
        let Ok(dual) = solve_dual_lp(&patterns, &quantities) else {
            break;
        };
        lp_lower_bound = dual.objective;

        let priced = best_pricing_pattern(&weights, &quantities, &dual.values, capacity);
        if priced.value <= 1.0 + 1e-6 {
            break;
        }

        if seen.insert(priced.pattern.counts.clone()) {
            patterns.push(priced.pattern);
            generated_patterns += 1;
        } else {
            break;
        }
    }

    let mut enumerated_all = true;
    let enumerated_patterns =
        enumerate_patterns(&weights, &quantities, capacity, options.exact_pattern_limit)
            .unwrap_or_else(|| {
                enumerated_all = false;
                Vec::new()
            });

    for pattern in enumerated_patterns {
        if seen.insert(pattern.counts.clone()) {
            patterns.push(pattern);
        }
    }

    patterns.sort_by(|left, right| {
        right.total_pieces().cmp(&left.total_pieces()).then_with(|| {
            right.used_length(stock, &lengths).cmp(&left.used_length(stock, &lengths))
        })
    });

    let state_limit = heuristic.stock_count.max(1);
    let demand_state = quantities.clone();
    let mut cache = HashMap::new();
    let mut explored_states = 0;

    let Some(best_count) = exact_search(
        &demand_state,
        &patterns,
        &lengths,
        capacity,
        state_limit,
        &mut cache,
        &mut explored_states,
    ) else {
        heuristic.algorithm = "column_generation".to_string();
        heuristic.lower_bound = (lp_lower_bound > 0.0).then_some(lp_lower_bound);
        heuristic.metrics.generated_patterns = generated_patterns;
        heuristic.metrics.enumerated_patterns = if enumerated_all { patterns.len() } else { 0 };
        heuristic.metrics.explored_states = explored_states;
        heuristic.metrics.notes.push("exact search fell back to heuristic incumbent".to_string());
        return Ok(heuristic);
    };

    if best_count > heuristic.stock_count {
        heuristic.algorithm = "column_generation".to_string();
        heuristic.lower_bound = (lp_lower_bound > 0.0).then_some(lp_lower_bound);
        heuristic.metrics.generated_patterns = generated_patterns;
        heuristic.metrics.enumerated_patterns = if enumerated_all { patterns.len() } else { 0 };
        heuristic.metrics.explored_states = explored_states;
        heuristic.metrics.notes.push("heuristic incumbent outperformed pattern search".to_string());
        return Ok(heuristic);
    }

    let mut remaining = demand_state.clone();
    let mut selected_patterns = Vec::new();
    while remaining.iter().any(|count| *count > 0) {
        let choice = cache.get(&remaining).and_then(|entry| entry.choice).ok_or_else(|| {
            BinPackingError::Unsupported(
                "pattern-search reconstruction failed to recover the incumbent".to_string(),
            )
        })?;
        let pattern = patterns[choice].clone();
        subtract_pattern(&mut remaining, &pattern).ok_or_else(|| {
            BinPackingError::Unsupported(
                "pattern-search reconstruction produced an infeasible subtraction".to_string(),
            )
        })?;
        selected_patterns.push(pattern);
    }

    let build_context = BuildContext {
        stock,
        demands: &demands,
        lengths: &lengths,
        exact: enumerated_all,
        lower_bound: if lp_lower_bound > 0.0 { lp_lower_bound } else { best_count as f64 },
        generated_patterns,
        enumerated_patterns: patterns.len(),
        explored_states,
    };
    let solution = build_solution_from_patterns(&build_context, &selected_patterns);

    Ok(solution)
}

fn build_solution_from_patterns(context: &BuildContext<'_>, patterns: &[Pattern]) -> OneDSolution {
    let mut bins = patterns
        .iter()
        .map(|pattern| {
            let mut bin = PackedBin::new(0);
            for (index, count) in pattern.counts.iter().enumerate() {
                for _ in 0..*count {
                    let inserted = bin.add_piece(
                        PieceInstance {
                            demand_index: index,
                            name: context.demands[index].name.clone(),
                            length: context.lengths[index],
                        },
                        context.stock,
                    );
                    debug_assert!(inserted, "generated pattern exceeded stock capacity");
                }
            }
            bin
        })
        .collect::<Vec<_>>();

    bins.sort_by_key(|bin| std::cmp::Reverse(bin.used_length()));

    OneDSolution::from_bins(
        "column_generation",
        context.exact,
        Some(context.lower_bound),
        std::slice::from_ref(context.stock),
        &bins,
        &[],
        SolverMetrics1D {
            iterations: context.generated_patterns + 1,
            generated_patterns: context.generated_patterns,
            enumerated_patterns: context.enumerated_patterns,
            explored_states: context.explored_states,
            notes: vec![
                "column generation supplies the LP lower bound".to_string(),
                if context.exact {
                    "pattern dynamic programming proved optimality".to_string()
                } else {
                    "pattern enumeration hit the configured cap; result is best-known".to_string()
                },
            ],
        },
    )
}

fn initial_patterns(quantities: &[usize], weights: &[u32], capacity: u32) -> Vec<Pattern> {
    quantities
        .iter()
        .enumerate()
        .filter_map(|(index, quantity)| {
            let max_count = usize::try_from(capacity / weights[index]).ok()?.min(*quantity);
            (max_count > 0).then(|| {
                let mut counts = vec![0; quantities.len()];
                counts[index] = max_count;
                Pattern { counts }
            })
        })
        .collect()
}

fn best_pricing_pattern(
    weights: &[u32],
    quantities: &[usize],
    dual_values: &[f64],
    capacity: u32,
) -> PricingResult {
    let mut cache = HashMap::new();
    let mut current = vec![0; quantities.len()];

    let (_, counts) =
        price_recursively(0, capacity, weights, quantities, dual_values, &mut current, &mut cache);

    let value =
        counts.iter().enumerate().map(|(index, count)| dual_values[index] * (*count as f64)).sum();

    PricingResult { value, pattern: Pattern { counts } }
}

fn price_recursively(
    index: usize,
    remaining_capacity: u32,
    weights: &[u32],
    quantities: &[usize],
    dual_values: &[f64],
    current: &mut [usize],
    cache: &mut HashMap<(usize, u32), (f64, Vec<usize>)>,
) -> (f64, Vec<usize>) {
    if let Some(result) = cache.get(&(index, remaining_capacity)) {
        return result.clone();
    }

    if index == weights.len() {
        return (0.0, current.to_vec());
    }

    let mut best_value = f64::MIN;
    let mut best_counts = current.to_vec();
    let limit =
        usize::try_from(remaining_capacity / weights[index]).unwrap_or(0).min(quantities[index]);

    for count in 0..=limit {
        current[index] = count;
        let residual_capacity =
            remaining_capacity.saturating_sub(weights[index].saturating_mul(count as u32));
        let (tail_value, tail_counts) = price_recursively(
            index + 1,
            residual_capacity,
            weights,
            quantities,
            dual_values,
            current,
            cache,
        );

        let value = tail_value + dual_values[index] * count as f64;
        if value > best_value {
            best_value = value;
            best_counts = tail_counts;
        }
    }

    current[index] = 0;
    let result = (best_value.max(0.0), best_counts);
    cache.insert((index, remaining_capacity), result.clone());
    result
}

fn enumerate_patterns(
    weights: &[u32],
    quantities: &[usize],
    capacity: u32,
    pattern_limit: usize,
) -> Option<Vec<Pattern>> {
    let mut patterns = Vec::new();
    let mut current = vec![0; quantities.len()];
    enumerate_patterns_recursively(
        0,
        capacity,
        weights,
        quantities,
        &mut current,
        &mut patterns,
        pattern_limit,
    )
    .then_some(patterns)
}

fn enumerate_patterns_recursively(
    index: usize,
    remaining_capacity: u32,
    weights: &[u32],
    quantities: &[usize],
    current: &mut [usize],
    patterns: &mut Vec<Pattern>,
    pattern_limit: usize,
) -> bool {
    if patterns.len() > pattern_limit {
        return false;
    }

    if index == weights.len() {
        if current.iter().any(|count| *count > 0) {
            patterns.push(Pattern { counts: current.to_vec() });
        }
        return true;
    }

    let max_count =
        usize::try_from(remaining_capacity / weights[index]).unwrap_or(0).min(quantities[index]);

    for count in 0..=max_count {
        current[index] = count;
        let residual_capacity =
            remaining_capacity.saturating_sub(weights[index].saturating_mul(count as u32));
        if !enumerate_patterns_recursively(
            index + 1,
            residual_capacity,
            weights,
            quantities,
            current,
            patterns,
            pattern_limit,
        ) {
            return false;
        }
    }

    current[index] = 0;
    true
}

#[derive(Debug, Clone)]
struct PricingResult {
    value: f64,
    pattern: Pattern,
}

#[derive(Debug, Clone)]
struct DualSolution {
    objective: f64,
    values: Vec<f64>,
}

fn solve_dual_lp(patterns: &[Pattern], quantities: &[usize]) -> Result<DualSolution> {
    let variable_count = quantities.len();
    let constraint_count = patterns.len();
    let width = variable_count + constraint_count + 1;
    let height = constraint_count + 1;

    let mut tableau = vec![vec![0.0; width]; height];
    let mut basis = vec![0_usize; constraint_count];

    for (row, pattern) in patterns.iter().enumerate() {
        for (column, coefficient) in pattern.counts.iter().enumerate() {
            tableau[row][column] = *coefficient as f64;
        }
        tableau[row][variable_count + row] = 1.0;
        tableau[row][width - 1] = 1.0;
        basis[row] = variable_count + row;
    }

    for (column, quantity) in quantities.iter().enumerate() {
        tableau[height - 1][column] = -(*quantity as f64);
    }

    for _ in 0..10_000 {
        let entering =
            (0..(width - 1)).filter(|column| tableau[height - 1][*column] < -1e-9).min_by(
                |left, right| tableau[height - 1][*left].total_cmp(&tableau[height - 1][*right]),
            );

        let Some(entering) = entering else {
            let mut values = vec![0.0; variable_count];
            for (row, basis_variable) in basis.iter().enumerate() {
                if *basis_variable < variable_count {
                    values[*basis_variable] = tableau[row][width - 1];
                }
            }

            return Ok(DualSolution { objective: tableau[height - 1][width - 1], values });
        };

        let leaving = (0..constraint_count)
            .filter_map(|row| {
                let coefficient = tableau[row][entering];
                (coefficient > 1e-9).then(|| (row, tableau[row][width - 1] / coefficient))
            })
            .min_by(|left, right| left.1.total_cmp(&right.1))
            .map(|(row, _)| row)
            .ok_or_else(|| {
                BinPackingError::Unsupported("dual simplex became unbounded".to_string())
            })?;

        pivot(&mut tableau, leaving, entering);
        basis[leaving] = entering;
    }

    Err(BinPackingError::Unsupported("dual simplex exceeded the pivot iteration limit".to_string()))
}

fn pivot(tableau: &mut [Vec<f64>], pivot_row: usize, pivot_column: usize) {
    let width = tableau[0].len();
    let pivot_value = tableau[pivot_row][pivot_column];

    for value in &mut tableau[pivot_row] {
        *value /= pivot_value;
    }

    let pivot_snapshot = tableau[pivot_row].clone();
    for (row, current_row) in tableau.iter_mut().enumerate() {
        if row == pivot_row {
            continue;
        }

        let factor = current_row[pivot_column];
        if factor.abs() <= 1e-12 {
            continue;
        }

        for (column, value) in current_row.iter_mut().enumerate().take(width) {
            *value -= factor * pivot_snapshot[column];
        }
    }
}

#[derive(Debug, Clone)]
struct SearchEntry {
    cost: Option<usize>,
    choice: Option<usize>,
}

fn exact_search(
    remaining: &[usize],
    patterns: &[Pattern],
    lengths: &[u32],
    capacity: u32,
    incumbent: usize,
    cache: &mut HashMap<Vec<usize>, SearchEntry>,
    explored_states: &mut usize,
) -> Option<usize> {
    if remaining.iter().all(|count| *count == 0) {
        return Some(0);
    }

    if let Some(entry) = cache.get(remaining) {
        return entry.cost;
    }

    *explored_states = explored_states.saturating_add(1);
    let lower_bound = lower_bound(remaining, lengths, capacity);
    if lower_bound > incumbent {
        cache.insert(remaining.to_vec(), SearchEntry { cost: None, choice: None });
        return None;
    }

    let anchor = remaining
        .iter()
        .enumerate()
        .filter(|(_, count)| **count > 0)
        .max_by_key(|(index, _)| lengths[*index])
        .map(|(index, _)| index)
        .unwrap_or(0);

    let mut best_cost = incumbent;
    let mut best_choice = None;

    for (index, pattern) in patterns.iter().enumerate() {
        if pattern.counts[anchor] == 0 || !pattern_fits(remaining, pattern) {
            continue;
        }

        let mut next = remaining.to_vec();
        if subtract_pattern(&mut next, pattern).is_none() {
            continue;
        }

        let residual = exact_search(
            &next,
            patterns,
            lengths,
            capacity,
            best_cost.saturating_sub(1),
            cache,
            explored_states,
        );

        if let Some(residual) = residual {
            let cost = residual.saturating_add(1);
            if cost <= best_cost {
                best_cost = cost;
                best_choice = Some(index);
            }
        }
    }

    let entry = if let Some(choice) = best_choice {
        SearchEntry { cost: Some(best_cost), choice: Some(choice) }
    } else {
        SearchEntry { cost: None, choice: None }
    };

    cache.insert(remaining.to_vec(), entry.clone());
    entry.cost
}

fn lower_bound(remaining: &[usize], lengths: &[u32], capacity: u32) -> usize {
    let total_weight = remaining
        .iter()
        .enumerate()
        .map(|(index, count)| (*count as u64) * u64::from(lengths[index]))
        .sum::<u64>();

    usize::try_from(total_weight.div_ceil(u64::from(capacity))).unwrap_or(0)
}

fn pattern_fits(remaining: &[usize], pattern: &Pattern) -> bool {
    remaining
        .iter()
        .zip(&pattern.counts)
        .all(|(remaining, pattern_count)| *remaining >= *pattern_count)
}

fn subtract_pattern(remaining: &mut [usize], pattern: &Pattern) -> Option<()> {
    if !pattern_fits(remaining, pattern) {
        return None;
    }

    for (remaining, count) in remaining.iter_mut().zip(&pattern.counts) {
        *remaining = remaining.saturating_sub(*count);
    }

    Some(())
}

impl fmt::Display for Pattern {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("[")?;
        for (index, count) in self.counts.iter().enumerate() {
            if index > 0 {
                formatter.write_str(", ")?;
            }
            write!(formatter, "{count}")?;
        }
        formatter.write_str("]")
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::{
        BinPackingError,
        one_d::{CutDemand1D, OneDOptions, OneDProblem, Stock1D},
    };

    use super::{
        Pattern, SearchEntry, enumerate_patterns, exact_search, initial_patterns, pattern_fits,
        solve_dual_lp, solve_exact, subtract_pattern,
    };

    #[test]
    fn exact_solver_proves_three_bar_optimum() {
        let problem = OneDProblem {
            stock: vec![Stock1D {
                name: "stock".to_string(),
                length: 120,
                kerf: 0,
                trim: 0,
                cost: 1.0,
                available: None,
            }],
            demands: vec![
                CutDemand1D { name: "A".to_string(), length: 70, quantity: 2 },
                CutDemand1D { name: "B".to_string(), length: 50, quantity: 2 },
                CutDemand1D { name: "C".to_string(), length: 20, quantity: 2 },
            ],
        };

        let solution = solve_exact(&problem, &OneDOptions::default()).expect("exact solution");
        assert_eq!(solution.stock_count, 3);
        assert!(solution.exact);
        assert!(solution.lower_bound.expect("lower bound") >= 2.0);
    }

    #[test]
    fn exact_solver_handles_two_type_optimum_without_hanging() {
        let problem = OneDProblem {
            stock: vec![Stock1D {
                name: "stock".to_string(),
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
        };

        let solution = solve_exact(&problem, &OneDOptions::default()).expect("exact solution");
        assert_eq!(solution.stock_count, 2);
        assert!(solution.exact);
    }

    #[test]
    fn exact_solver_rejects_unsupported_stock_configurations() {
        let multiple_stock_types = OneDProblem {
            stock: vec![
                Stock1D {
                    name: "A".to_string(),
                    length: 10,
                    kerf: 0,
                    trim: 0,
                    cost: 1.0,
                    available: None,
                },
                Stock1D {
                    name: "B".to_string(),
                    length: 12,
                    kerf: 0,
                    trim: 0,
                    cost: 1.0,
                    available: None,
                },
            ],
            demands: vec![CutDemand1D { name: "cut".to_string(), length: 5, quantity: 1 }],
        };
        assert!(matches!(
            solve_exact(&multiple_stock_types, &OneDOptions::default()),
            Err(BinPackingError::Unsupported(message))
                if message == "column generation currently supports exactly one stock type"
        ));

        let limited_stock = OneDProblem {
            stock: vec![Stock1D {
                name: "A".to_string(),
                length: 10,
                kerf: 0,
                trim: 0,
                cost: 1.0,
                available: Some(1),
            }],
            demands: vec![CutDemand1D { name: "cut".to_string(), length: 5, quantity: 1 }],
        };
        assert!(matches!(
            solve_exact(&limited_stock, &OneDOptions::default()),
            Err(BinPackingError::Unsupported(message))
                if message == "column generation currently assumes unlimited stock availability"
        ));
    }

    #[test]
    fn exact_solver_handles_empty_demands_without_running_search() {
        let problem = OneDProblem {
            stock: vec![Stock1D {
                name: "stock".to_string(),
                length: 10,
                kerf: 0,
                trim: 0,
                cost: 1.0,
                available: None,
            }],
            demands: Vec::new(),
        };

        let solution = solve_exact(&problem, &OneDOptions::default()).expect("empty solve");
        assert_eq!(solution.algorithm, "column_generation");
        assert!(solution.exact);
        assert_eq!(solution.lower_bound, Some(0.0));
        assert_eq!(solution.stock_count, 0);
    }

    #[test]
    fn pattern_helpers_cover_limits_dual_errors_and_display() {
        let initial = initial_patterns(&[3, 2], &[4, 3], 10);
        assert_eq!(initial.len(), 2);
        assert_eq!(initial[0].counts, vec![2, 0]);
        assert_eq!(initial[1].counts, vec![0, 2]);

        assert!(enumerate_patterns(&[3, 4], &[2, 2], 8, 0).is_none());

        let dual_error =
            solve_dual_lp(&[Pattern { counts: vec![0] }], &[1]).expect_err("unbounded");
        assert!(matches!(
            dual_error,
            BinPackingError::Unsupported(message) if message == "dual simplex became unbounded"
        ));

        let mut remaining = vec![1, 0];
        let infeasible = Pattern { counts: vec![2, 0] };
        assert!(pattern_fits(&remaining, &Pattern { counts: vec![1, 0] }));
        assert!(!pattern_fits(&remaining, &infeasible));
        assert!(subtract_pattern(&mut remaining, &infeasible).is_none());
        assert_eq!(format!("{}", Pattern { counts: vec![1, 2, 3] }), "[1, 2, 3]");
    }

    #[test]
    fn exact_solver_marks_capped_pattern_enumeration_as_best_known() {
        let problem = OneDProblem {
            stock: vec![Stock1D {
                name: "stock".to_string(),
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
        };

        let solution = solve_exact(
            &problem,
            &OneDOptions { exact_pattern_limit: 0, ..OneDOptions::default() },
        )
        .expect("capped exact solve");

        assert_eq!(solution.stock_count, 2);
        assert!(!solution.exact);
        assert_eq!(solution.algorithm, "column_generation");
        assert!(
            solution
                .metrics
                .notes
                .iter()
                .any(|note| note
                    == "pattern enumeration hit the configured cap; result is best-known")
        );
    }

    #[test]
    fn exact_search_uses_cached_results_and_prunes_impossible_incumbents() {
        let patterns = vec![Pattern { counts: vec![1] }];
        let mut cache = HashMap::from([(vec![1], SearchEntry { cost: Some(1), choice: Some(0) })]);
        let mut explored_states = 0;

        let cached = exact_search(&[1], &patterns, &[5], 5, 3, &mut cache, &mut explored_states);
        assert_eq!(cached, Some(1));
        assert_eq!(explored_states, 0);

        let mut pruned_cache = HashMap::new();
        let mut pruned_states = 0;
        let pruned =
            exact_search(&[2], &patterns, &[5], 5, 1, &mut pruned_cache, &mut pruned_states);
        assert_eq!(pruned, None);
        assert_eq!(pruned_states, 1);
        assert_eq!(pruned_cache.get(&vec![2]).and_then(|entry| entry.cost), None,);
    }
}
