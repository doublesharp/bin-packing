//! Shared validation and demand identity for explicitly bounded search experiments.
use super::maxrects::{self, MaxRectsStrategy};
use super::{TwoDOptions, TwoDProblem, TwoDSolution};
use crate::{BinPackingError, Result};

pub(super) fn prepare(problem: &TwoDProblem, options: &TwoDOptions) -> Result<TwoDProblem> {
    problem.validate()?;
    problem.ensure_feasible_demands()?;
    let quantity = problem.demands.iter().fold(0usize, |n, d| n.saturating_add(d.quantity));
    let rotations = problem.demands.iter().filter(|d| d.can_rotate && d.width != d.height).count();
    if quantity > 128 || problem.demands.len() > 64 || problem.sheets.len() > 8 || rotations > 8 {
        return Err(BinPackingError::Unsupported("bounded 2D search supports at most 128 parts, 64 demands, 8 stocks and 8 rotatable types".into()));
    }
    if !(1..=32).contains(&options.beam_width) || !(1..=16).contains(&options.multistart_runs) {
        return Err(BinPackingError::Unsupported(
            "bounded 2D search requires beam width 1..=32 and multistart runs 1..=16".into(),
        ));
    }
    if problem.sheets.iter().any(|sheet| {
        u128::from(sheet.width) * u128::from(sheet.height) * quantity as u128 > u128::from(u64::MAX)
    }) {
        return Err(BinPackingError::Unsupported(
            "bounded 2D search aggregate stock area exceeds u64::MAX".into(),
        ));
    }
    // Stock names are already unique. Preserve them because layout sorting
    // uses these names before summing floating-point costs.
    let mut indexed = problem.clone();
    for (i, d) in indexed.demands.iter_mut().enumerate() {
        d.name = format!("search_demand_{i}");
    }
    Ok(indexed)
}

// O(P^2 + P D + S T) independent readback of source identity, stock limits,
// orientation, effective bounds, pairwise blade gaps and exact demand conservation.
pub(super) fn validate_solution(problem: &TwoDProblem, solution: &TwoDSolution) -> Result<()> {
    let bad = || {
        BinPackingError::Unsupported(
            "experimental candidate failed independent layout validation".into(),
        )
    };
    let mut counts = vec![0usize; problem.demands.len()];
    let mut stocks = vec![0usize; problem.sheets.len()];
    for layout in &solution.layouts {
        let (index, stock) = problem
            .sheets
            .iter()
            .enumerate()
            .find(|(_, s)| s.name == layout.sheet_name)
            .ok_or_else(bad)?;
        if (layout.width, layout.height) != (stock.width, stock.height) {
            return Err(bad());
        }
        stocks[index] += 1;
        if stock.quantity.is_some_and(|q| stocks[index] > q) {
            return Err(bad());
        }
        let (bound_width, bound_height) = super::model::effective_bounds(stock);
        for (i, p) in layout.placements.iter().enumerate() {
            let (index, demand) = problem
                .demands
                .iter()
                .enumerate()
                .find(|(_, d)| d.name == p.name)
                .ok_or_else(bad)?;
            let dimensions = if p.rotated {
                (demand.height, demand.width)
            } else {
                (demand.width, demand.height)
            };
            if (p.width, p.height) != dimensions
                || (p.rotated && !demand.can_rotate)
                || p.width > stock.width
                || p.height > stock.height
                || u64::from(p.x) + u64::from(p.width) > u64::from(bound_width)
                || u64::from(p.y) + u64::from(p.height) > u64::from(bound_height)
            {
                return Err(bad());
            }
            counts[index] += 1;
            for q in &layout.placements[..i] {
                let k = u64::from(stock.kerf);
                if !(u64::from(p.x) + u64::from(p.width) + k <= u64::from(q.x)
                    || u64::from(q.x) + u64::from(q.width) + k <= u64::from(p.x)
                    || u64::from(p.y) + u64::from(p.height) + k <= u64::from(q.y)
                    || u64::from(q.y) + u64::from(q.height) + k <= u64::from(p.y))
                {
                    return Err(bad());
                }
            }
        }
    }
    for d in &solution.unplaced {
        let (index, source) = problem
            .demands
            .iter()
            .enumerate()
            .find(|(_, source)| source.name == d.name)
            .ok_or_else(bad)?;
        if (d.width, d.height, d.can_rotate) != (source.width, source.height, source.can_rotate) {
            return Err(bad());
        }
        counts[index] = counts[index].saturating_add(d.quantity);
    }
    if counts.iter().zip(&problem.demands).any(|(&n, d)| n != d.quantity)
        || solution.sheet_count != solution.layouts.len()
    {
        return Err(bad());
    }
    Ok(())
}

pub(super) fn baseline(problem: &TwoDProblem, options: &TwoDOptions) -> Result<TwoDSolution> {
    let solution = super::solve_2d(problem.clone(), options.clone())?;
    validate_solution(problem, &solution)?;
    Ok(solution)
}

pub(super) fn restore(
    original: &TwoDProblem,
    indexed: &TwoDProblem,
    mut solution: TwoDSolution,
) -> Result<TwoDSolution> {
    validate_solution(indexed, &solution)?;
    for layout in &mut solution.layouts {
        for p in &mut layout.placements {
            let demand =
                indexed.demands.iter().position(|d| d.name == p.name).ok_or_else(|| {
                    BinPackingError::Unsupported("unknown experimental demand identity".into())
                })?;
            p.name = original.demands[demand].name.clone();
        }
    }
    for d in &mut solution.unplaced {
        let index =
            indexed.demands.iter().position(|source| source.name == d.name).ok_or_else(|| {
                BinPackingError::Unsupported("unknown experimental unplaced identity".into())
            })?;
        d.name = original.demands[index].name.clone();
    }
    Ok(solution)
}

pub(super) fn demand_aware(problem: TwoDProblem, mut options: TwoDOptions) -> Result<TwoDSolution> {
    let indexed = prepare(&problem, &options)?;
    options.seed = Some(options.seed.unwrap_or(0));
    let mut best = baseline(&indexed, &options)?;
    let baseline_states = best.metrics.explored_states;
    let baseline_iterations = best.metrics.iterations;
    if !options.guillotine_required {
        let mut items = indexed.expanded_items();
        maxrects::sort_items_descending(&mut items);
        let candidate = maxrects::pack_with_order(
            &indexed,
            &options,
            &items,
            "demand_aware",
            1,
            MaxRectsStrategy::DemandAware,
        )?;
        let effort = candidate.metrics.explored_states;
        if validate_solution(&indexed, &candidate).is_ok() && candidate.is_better_than(&best) {
            best = candidate;
        }
        best.metrics.explored_states = baseline_states.saturating_add(effort);
        best.metrics.iterations = baseline_iterations.saturating_add(1);
        best.metrics.notes.push(format!("opt-in demand-aware search scored {effort} shortlisted candidates; baseline retained unless improved; no optimality claim"));
    } else {
        best.metrics
            .notes
            .push("demand-aware MaxRects search skipped for required guillotine layouts".into());
    }
    restore(&problem, &indexed, best)
}
