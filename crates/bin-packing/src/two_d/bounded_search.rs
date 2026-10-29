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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::two_d::{Placement2D, RectDemand2D, Sheet2D, SolverMetrics2D};

    fn fixture() -> (TwoDProblem, TwoDSolution) {
        let problem = TwoDProblem {
            sheets: vec![Sheet2D {
                name: "stock".into(),
                width: 12,
                height: 10,
                cost: 1.0,
                quantity: Some(1),
                kerf: 1,
                edge_kerf_relief: false,
            }],
            demands: vec![RectDemand2D {
                name: "part".into(),
                width: 3,
                height: 2,
                quantity: 2,
                can_rotate: true,
            }],
        };
        let placements = vec![
            Placement2D { name: "part".into(), x: 0, y: 0, width: 3, height: 2, rotated: false },
            Placement2D { name: "part".into(), x: 4, y: 0, width: 3, height: 2, rotated: false },
        ];
        let solution = TwoDSolution::from_layouts(
            "fixture",
            false,
            &problem.sheets,
            vec![(0, placements)],
            Vec::new(),
            SolverMetrics2D { iterations: 0, explored_states: 0, notes: Vec::new() },
            0,
        );
        (problem, solution)
    }

    fn rejected(problem: &TwoDProblem, solution: &TwoDSolution) {
        assert!(
            matches!(validate_solution(problem, solution), Err(BinPackingError::Unsupported(reason)) if reason == "experimental candidate failed independent layout validation")
        );
    }

    #[test]
    fn validator_rejects_forged_layouts_and_preserves_valid_rotation_and_gap() {
        let (problem, valid) = fixture();
        assert!(validate_solution(&problem, &valid).is_ok());
        let mut rotated = valid.clone();
        rotated.layouts[0].placements[0].rotated = true;
        rotated.layouts[0].placements[0].width = 2;
        rotated.layouts[0].placements[0].height = 3;
        assert!(validate_solution(&problem, &rotated).is_ok());
        let mut cases = Vec::new();
        let mut bad = valid.clone();
        bad.layouts[0].sheet_name = "unknown".into();
        cases.push(bad);
        let mut bad = valid.clone();
        bad.layouts[0].width += 1;
        cases.push(bad);
        let mut bad = valid.clone();
        bad.layouts.push(bad.layouts[0].clone());
        bad.sheet_count = 2;
        cases.push(bad);
        let mut bad = valid.clone();
        bad.layouts[0].placements[0].name = "unknown".into();
        cases.push(bad);
        let mut bad = valid.clone();
        bad.layouts[0].placements[0].width += 1;
        cases.push(bad);
        let mut bad = valid.clone();
        bad.layouts[0].placements[0].x = 10;
        cases.push(bad);
        let mut bad = valid.clone();
        bad.layouts[0].placements[0].y = 9;
        cases.push(bad);
        let mut bad = valid.clone();
        bad.layouts[0].placements[1].x = 3;
        cases.push(bad);
        let mut bad = valid.clone();
        bad.sheet_count = 0;
        cases.push(bad);
        let mut bad = valid.clone();
        assert!(bad.layouts[0].placements.pop().is_some());
        cases.push(bad);
        for bad in cases {
            rejected(&problem, &bad);
        }
        let mut no_rotation = problem.clone();
        no_rotation.demands[0].can_rotate = false;
        rejected(&no_rotation, &rotated);
    }

    #[test]
    fn validator_checks_unplaced_source_identity_and_conservation() {
        let (problem, mut partial) = fixture();
        assert!(partial.layouts[0].placements.pop().is_some());
        let mut missing = problem.demands[0].clone();
        missing.quantity = 1;
        partial.unplaced.push(missing);
        assert!(validate_solution(&problem, &partial).is_ok());
        let mut cases = Vec::new();
        let mut bad = partial.clone();
        bad.unplaced[0].name = "unknown".into();
        cases.push(bad);
        let mut bad = partial.clone();
        bad.unplaced[0].width += 1;
        cases.push(bad);
        let mut bad = partial.clone();
        bad.unplaced[0].height += 1;
        cases.push(bad);
        let mut bad = partial.clone();
        bad.unplaced[0].can_rotate = false;
        cases.push(bad);
        let mut bad = partial.clone();
        bad.unplaced[0].quantity = 2;
        cases.push(bad);
        for bad in cases {
            rejected(&problem, &bad);
        }
    }

    #[test]
    fn prepare_enforces_independent_resource_limits_and_inclusive_controls() {
        let (base, _) = fixture();
        let options = TwoDOptions::default();
        for axis in 0..4 {
            let mut problem = base.clone();
            match axis {
                0 => problem.demands[0].quantity = 129,
                1 => {
                    problem.demands = (0..65)
                        .map(|i| RectDemand2D {
                            name: format!("d{i}"),
                            quantity: 1,
                            can_rotate: false,
                            ..base.demands[0].clone()
                        })
                        .collect()
                }
                2 => {
                    problem.sheets = (0..9)
                        .map(|i| Sheet2D { name: format!("s{i}"), ..base.sheets[0].clone() })
                        .collect()
                }
                _ => {
                    problem.demands = (0..9)
                        .map(|i| RectDemand2D { name: format!("d{i}"), ..base.demands[0].clone() })
                        .collect()
                }
            }
            assert!(
                matches!(prepare(&problem, &options), Err(BinPackingError::Unsupported(reason)) if reason == "bounded 2D search supports at most 128 parts, 64 demands, 8 stocks and 8 rotatable types")
            );
        }
        for (beam_width, multistart_runs) in [(0, 1), (33, 1), (1, 0), (1, 17)] {
            let options = TwoDOptions { beam_width, multistart_runs, ..Default::default() };
            assert!(
                matches!(prepare(&base, &options), Err(BinPackingError::Unsupported(reason)) if reason == "bounded 2D search requires beam width 1..=32 and multistart runs 1..=16")
            );
        }
        for (beam_width, multistart_runs) in [(1, 1), (32, 16)] {
            let options = TwoDOptions { beam_width, multistart_runs, ..Default::default() };
            assert!(prepare(&base, &options).is_ok());
        }
    }
}
