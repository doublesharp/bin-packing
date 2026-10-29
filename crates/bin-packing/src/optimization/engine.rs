//! Bounded subset and open-size search, independent of a geometry backend.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use super::{
    OptimizationOptions, OptimizationProblem, OptimizationReport, OptimizedSolution,
    PackingObjective,
};
use crate::{BinPackingError, Result};

pub(crate) struct Candidate<P, S> {
    pub problem: P,
    pub solution: S,
    pub stocks: Vec<usize>,
    pub demands: Vec<Vec<usize>>,
    pub selected: Vec<usize>,
    pub containers: usize,
    pub cost: f64,
    pub waste: u128,
    pub extents: Vec<u32>,
    pub states: usize,
    pub truncated: bool,
}

pub(crate) trait Model: Sized + Clone {
    type Solution;
    fn validate(&self) -> Result<()>;
    fn quantities(&self) -> Vec<usize>;
    fn stock_count(&self) -> usize;
    fn dimensions(&self) -> Vec<u32>;
    fn count_bound(&self) -> usize;
    fn open_bound(&self, axes: &[usize]) -> u128;
    fn minimum_stock_cost(&self) -> f64;
    fn evaluate(
        &self,
        quantities: &[usize],
        extents: &[u32],
        one_bin: bool,
        options: &OptimizationOptions,
    ) -> Result<Candidate<Self, Self::Solution>>;
}

fn invalid(field: &str, reason: &str) -> BinPackingError {
    BinPackingError::InvalidConstraint { field: field.into(), reason: reason.into() }
}

fn profit(quantities: &[usize], profits: &[f64]) -> f64 {
    quantities.iter().zip(profits).map(|(&q, &p)| q as f64 * p).sum()
}

fn missing(selected: &[usize], requested: &[usize]) -> usize {
    selected.iter().zip(requested).map(|(&s, &q)| q.saturating_sub(s)).sum()
}

fn extent_value(extents: &[u32], axes: &[usize]) -> u128 {
    axes.iter().map(|&axis| u128::from(extents[axis])).product()
}

fn compare<P, S>(
    a: &Candidate<P, S>,
    b: &Candidate<P, S>,
    objective: PackingObjective,
    requested: &[usize],
    minima: &[usize],
    profits: &[f64],
) -> Ordering {
    let priority = if objective == PackingObjective::Knapsack {
        missing(&a.selected, minima)
            .cmp(&missing(&b.selected, minima))
            .then_with(|| profit(&b.selected, profits).total_cmp(&profit(&a.selected, profits)))
    } else {
        missing(&a.selected, requested).cmp(&missing(&b.selected, requested)).then_with(|| {
            match objective {
                PackingObjective::TotalCost => a.cost.total_cmp(&b.cost),
                PackingObjective::BinCount | PackingObjective::Feasibility => {
                    a.containers.cmp(&b.containers)
                }
                _ => extent_value(&a.extents, objective.axes())
                    .cmp(&extent_value(&b.extents, objective.axes())),
            }
        })
    };
    priority
        .then_with(|| a.containers.cmp(&b.containers))
        .then_with(|| a.cost.total_cmp(&b.cost))
        .then_with(|| a.waste.cmp(&b.waste))
}

struct Evaluator<'a, P: Model> {
    model: &'a P,
    options: &'a OptimizationOptions,
    evaluations: usize,
    states: usize,
    truncated: bool,
}

impl<P: Model> Evaluator<'_, P> {
    fn run(
        &mut self,
        quantities: &[usize],
        extents: &[u32],
        one_bin: bool,
    ) -> Result<Option<Candidate<P, P::Solution>>> {
        if self.evaluations == self.options.max_evaluations {
            self.truncated = true;
            return Ok(None);
        }
        self.evaluations += 1;
        let candidate = self.model.evaluate(quantities, extents, one_bin, self.options)?;
        self.states = self.states.saturating_add(candidate.states);
        self.truncated |= candidate.truncated;
        Ok(Some(candidate))
    }
}

/// O(E * packing-work) evaluations. The visited set stores at most O(E * D)
/// quantity vectors of length D; the live frontier retains beam_width vectors.
/// Geometry work inside an evaluation has its separate declared native budget.
pub(crate) fn solve<P: Model>(
    problem: OptimizationProblem<P>,
    options: OptimizationOptions,
) -> Result<OptimizedSolution<P::Solution, P>> {
    problem.packing.validate()?;
    if !(1..=4096).contains(&options.max_evaluations)
        || !(1..=32).contains(&options.beam_width)
        || options.packing_max_states > 100_000
    {
        return Err(invalid(
            "search",
            "requires evaluations 1..=4096, beam 1..=32, packing states 0..=100000",
        ));
    }
    let requested = problem.packing.quantities();
    if requested.len() > 16
        || requested.iter().sum::<usize>() > 128
        || problem.packing.stock_count() > 16
    {
        return Err(invalid(
            "problem",
            "objective search supports 128 items, 16 demand types and 16 stocks",
        ));
    }
    let profits = if problem.profits.is_empty() {
        vec![1.0; requested.len()]
    } else {
        problem.profits.clone()
    };
    let minima = if problem.minimum_quantities.is_empty() {
        vec![0; requested.len()]
    } else {
        problem.minimum_quantities.clone()
    };
    if profits.len() != requested.len()
        || minima.len() != requested.len()
        || profits.iter().any(|&p| !p.is_finite() || p < 0.0)
        || minima.iter().zip(&requested).any(|(&m, &q)| m > q)
        || !profit(&requested, &profits).is_finite()
    {
        return Err(invalid(
            "values",
            "profits/minima must match source rows and remain finite, nonnegative and within demand",
        ));
    }
    let dimensions = problem.packing.dimensions();
    let axes = options.objective.axes();
    if axes.iter().any(|&axis| axis >= dimensions.len())
        || (!axes.is_empty() && problem.packing.stock_count() != 1)
    {
        return Err(invalid(
            "objective",
            "open dimensions require a supported axis and exactly one stock type",
        ));
    }
    let mut evaluator = Evaluator {
        model: &problem.packing,
        options: &options,
        evaluations: 0,
        states: 0,
        truncated: false,
    };
    let mut best = evaluator
        .run(&requested, &dimensions, !axes.is_empty())?
        .ok_or_else(|| invalid("search", "no initial evaluation"))?;
    if options.objective == PackingObjective::Knapsack {
        let mut frontier = vec![requested.clone()];
        let mut seen = BTreeSet::from([requested.clone()]);
        while !frontier.is_empty() && evaluator.evaluations < options.max_evaluations {
            let mut next = Vec::new();
            for quantities in std::mem::take(&mut frontier) {
                for source in 0..quantities.len() {
                    if quantities[source] == minima[source] {
                        continue;
                    }
                    let mut child = quantities.clone();
                    child[source] -= 1;
                    if !seen.insert(child.clone()) {
                        continue;
                    }
                    if missing(&best.selected, &minima) == 0
                        && profit(&child, &profits) < profit(&best.selected, &profits)
                    {
                        continue;
                    }
                    let Some(candidate) = evaluator.run(&child, &dimensions, false)? else {
                        break;
                    };
                    let value = profit(&candidate.selected, &profits);
                    if compare(&candidate, &best, options.objective, &requested, &minima, &profits)
                        .is_lt()
                    {
                        best = candidate;
                    }
                    next.push((value, profit(&child, &profits), child));
                }
                if evaluator.evaluations == options.max_evaluations {
                    break;
                }
            }
            next.sort_by(|a, b| {
                b.0.total_cmp(&a.0).then_with(|| b.1.total_cmp(&a.1)).then_with(|| a.2.cmp(&b.2))
            });
            if next.len() > options.beam_width {
                evaluator.truncated = true;
                next.truncate(options.beam_width);
            }
            frontier = next.into_iter().map(|(_, _, q)| q).collect();
        }
        if !frontier.is_empty() {
            evaluator.truncated = true;
        }
    } else if !axes.is_empty() {
        // Heuristic failure is not infeasibility proof. Binary search is an
        // exploration policy only; the complete incumbent is always retained.
        let orders: Vec<Vec<usize>> = if axes.len() == 2 {
            vec![axes.to_vec(), axes.iter().rev().copied().collect()]
        } else {
            vec![axes.to_vec()]
        };
        let mut visited =
            BTreeMap::from([(dimensions.clone(), missing(&best.selected, &requested) == 0)]);
        for order in orders {
            let mut current = dimensions.clone();
            for axis in order {
                let mut low = 1u32;
                let mut high = current[axis];
                while low < high && evaluator.evaluations < options.max_evaluations {
                    let mid = low + (high - low) / 2;
                    let mut trial = current.clone();
                    trial[axis] = mid;
                    if let Some(&complete) = visited.get(&trial) {
                        if complete {
                            high = mid;
                            current = trial;
                        } else {
                            low = mid.saturating_add(1);
                        }
                        continue;
                    }
                    let evaluated = evaluator.run(&requested, &trial, true);
                    let candidate = match evaluated {
                        Ok(Some(candidate)) => candidate,
                        Ok(None) => break,
                        Err(
                            BinPackingError::Infeasible1D { .. }
                            | BinPackingError::Infeasible2D { .. }
                            | BinPackingError::Infeasible3D { .. },
                        ) => {
                            visited.insert(trial, false);
                            low = mid.saturating_add(1);
                            continue;
                        }
                        // A crop that leaves no positive trimmed material is
                        // another invalid trial, not a malformed original input.
                        Err(BinPackingError::InvalidConstraint { field, .. })
                            if field == "open_extent" =>
                        {
                            visited.insert(trial, false);
                            low = mid.saturating_add(1);
                            continue;
                        }
                        Err(error) => return Err(error),
                    };
                    let complete = missing(&candidate.selected, &requested) == 0;
                    visited.insert(trial.clone(), complete);
                    if complete {
                        high = mid;
                        current = trial;
                        if compare(
                            &candidate,
                            &best,
                            options.objective,
                            &requested,
                            &minima,
                            &profits,
                        )
                        .is_lt()
                        {
                            best = candidate;
                        }
                    } else {
                        low = mid.saturating_add(1);
                    }
                }
            }
        }
        evaluator.truncated = true; // Restricted packing and size paths are heuristic.
    }
    let complete = missing(&best.selected, &requested) == 0;
    let minimums_satisfied = missing(&best.selected, &minima) == 0;
    let total_profit = profit(&best.selected, &profits);
    let (primary_value, primary_bound, bound_is_upper, primary_optimal) = match options.objective {
        PackingObjective::Knapsack => (
            total_profit,
            Some(profit(&requested, &profits)),
            true,
            minimums_satisfied
                && best
                    .selected
                    .iter()
                    .zip(&requested)
                    .zip(&profits)
                    .all(|((&s, &q), &p)| s == q || p == 0.0),
        ),
        PackingObjective::Feasibility => {
            (missing(&best.selected, &requested) as f64, Some(0.0), false, complete)
        }
        PackingObjective::BinCount => {
            let bound = problem.packing.count_bound();
            (
                best.containers as f64,
                Some(bound as f64),
                false,
                complete && best.containers == bound,
            )
        }
        PackingObjective::TotalCost => {
            let bound = problem.packing.minimum_stock_cost();
            (best.cost, Some(bound), false, complete && best.cost == bound)
        }
        _ => {
            let value = extent_value(&best.extents, axes);
            let bound = problem.packing.open_bound(axes);
            (value as f64, Some(bound as f64), false, complete && value == bound)
        }
    };
    let report = OptimizationReport {
        objective: options.objective,
        complete,
        minimums_satisfied,
        selected_quantities: best.selected,
        total_profit,
        evaluations: evaluator.evaluations,
        packing_states: evaluator.states,
        search_truncated: evaluator.truncated,
        open_extents: if axes.is_empty() { Vec::new() } else { best.extents },
        primary_value,
        primary_bound,
        bound_is_upper,
        primary_optimal,
    };
    Ok(OptimizedSolution {
        effective_problem: best.problem,
        solution: best.solution,
        stock_indices: best.stocks,
        demand_indices: best.demands,
        report,
    })
}
