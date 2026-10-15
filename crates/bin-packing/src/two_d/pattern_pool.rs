//! Bounded recombination of complete physical sheet patterns.

use std::collections::HashMap;

use super::{Placement2D, SolverMetrics2D, TwoDAlgorithm, TwoDOptions, TwoDProblem, TwoDSolution};
use crate::{BinPackingError, Result};

const MAX_TYPES: usize = 16;
const MAX_ITEMS: usize = 96;
const MAX_PATTERNS: usize = 256;
const MAX_STATES: usize = 25_000;

/// Recombine physical sheet patterns from the ordinary Auto portfolio.
///
/// This opt-in search preserves the best portfolio incumbent and selects whole
/// sheet layouts with exact demand counts and hard stock caps. Demand names are
/// internally indexed so repeated names remain unambiguous. `options.algorithm`
/// is replaced by the portfolio's algorithms; all other options are retained.
///
/// Recombination is limited to 16 demand types, 16 stock types, 96 pieces,
/// 256 distinct patterns, and at most 25,000 search states. The state budget is
/// `min(max(beam_width, 1) * 512, 25_000)`. Larger inputs use ordinary Auto.
/// A restricted pattern pool does not prove global optimality.
///
/// # Errors
///
/// Returns the same input-validation and infeasible-demand errors as
/// [`super::solve_2d`], or the last portfolio error if no solver succeeds.
pub fn solve_2d_recombined(problem: TwoDProblem, options: TwoDOptions) -> Result<TwoDSolution> {
    problem.validate()?;
    problem.ensure_feasible_demands()?;
    let total = problem.demands.iter().map(|d| d.quantity).sum::<usize>();
    if problem.demands.len() > MAX_TYPES || problem.sheets.len() > MAX_TYPES || total > MAX_ITEMS {
        let mut solution =
            super::solve_2d(problem, TwoDOptions { algorithm: TwoDAlgorithm::Auto, ..options })?;
        solution.metrics.notes.push("pattern recombination skipped: input cap".into());
        return Ok(solution);
    }

    let mut indexed = problem.clone();
    for (index, demand) in indexed.demands.iter_mut().enumerate() {
        demand.name = index.to_string();
    }
    let algorithms = if options.guillotine_required {
        vec![
            TwoDAlgorithm::Guillotine,
            TwoDAlgorithm::GuillotineBestShortSideFit,
            TwoDAlgorithm::GuillotineBestLongSideFit,
            TwoDAlgorithm::GuillotineShorterLeftoverAxis,
            TwoDAlgorithm::GuillotineLongerLeftoverAxis,
            TwoDAlgorithm::GuillotineMinAreaSplit,
            TwoDAlgorithm::GuillotineMaxAreaSplit,
        ]
    } else {
        vec![
            TwoDAlgorithm::MaxRects,
            TwoDAlgorithm::MaxRectsBestShortSideFit,
            TwoDAlgorithm::MaxRectsBestLongSideFit,
            TwoDAlgorithm::MaxRectsContactPoint,
            TwoDAlgorithm::Skyline,
            TwoDAlgorithm::SkylineMinWaste,
            TwoDAlgorithm::BestFitDecreasingHeight,
            TwoDAlgorithm::GuillotineBestShortSideFit,
            TwoDAlgorithm::GuillotineShorterLeftoverAxis,
            TwoDAlgorithm::MultiStart,
            TwoDAlgorithm::RotationSearch,
        ]
    };
    let outcomes = crate::parallel::par_map_indexed(algorithms.len(), |index| {
        super::solve_2d(
            indexed.clone(),
            TwoDOptions { algorithm: algorithms[index], ..options.clone() },
        )
    });
    let mut candidates = Vec::new();
    let mut last_error = None;
    for outcome in outcomes {
        match outcome {
            Ok(candidate) => candidates.push(candidate),
            Err(error) => last_error = Some(error),
        }
    }
    if candidates.is_empty() {
        return Err(last_error.unwrap_or_else(|| {
            BinPackingError::Unsupported("pattern recombination: no candidate succeeded".into())
        }));
    }
    let Some(mut solution) = select_patterns(&indexed, &candidates, &options) else {
        return Err(BinPackingError::Unsupported("pattern recombination: empty portfolio".into()));
    };
    for placement in solution.layouts.iter_mut().flat_map(|layout| &mut layout.placements) {
        restore_name(&mut placement.name, &problem)?;
    }
    for demand in &mut solution.unplaced {
        restore_name(&mut demand.name, &problem)?;
    }
    Ok(solution)
}

fn restore_name(name: &mut String, problem: &TwoDProblem) -> Result<()> {
    let original = name.parse::<usize>().ok().and_then(|i| problem.demands.get(i));
    let Some(original) = original else {
        return Err(BinPackingError::Unsupported(
            "pattern recombination: lost demand identity".into(),
        ));
    };
    name.clone_from(&original.name);
    Ok(())
}

#[derive(Clone)]
struct Pattern {
    stock: usize,
    counts: Vec<usize>,
    placements: Vec<Placement2D>,
    guillotine: bool,
    prefix: Prefix,
}

#[derive(Clone, Copy, Default)]
struct Prefix {
    waste: u64,
    max_drop: u64,
    sum_sq: u128,
}

impl Prefix {
    fn dominates(self, other: Self) -> bool {
        self.waste <= other.waste && self.max_drop >= other.max_drop && self.sum_sq >= other.sum_sq
    }

    fn add(self, other: Self) -> Self {
        Self {
            waste: self.waste.saturating_add(other.waste),
            max_drop: self.max_drop.max(other.max_drop),
            sum_sq: self.sum_sq.saturating_add(other.sum_sq),
        }
    }
}

#[derive(Hash, PartialEq, Eq)]
struct Key {
    remaining: Vec<usize>,
    usage: Vec<usize>,
}

struct Search<'a> {
    problem: &'a TwoDProblem,
    options: &'a TwoDOptions,
    patterns: Vec<Pattern>,
    best: TwoDSolution,
    seen: HashMap<Key, Vec<Prefix>>,
    states: usize,
    limit: usize,
    exhausted: bool,
}

fn select_patterns(
    problem: &TwoDProblem,
    candidates: &[TwoDSolution],
    options: &TwoDOptions,
) -> Option<TwoDSolution> {
    let mut best = candidates.first()?.clone();
    for candidate in candidates.iter().skip(1) {
        if candidate.is_better_than(&best) {
            best = candidate.clone();
        }
    }
    let patterns = collect_patterns(problem, candidates);
    let pattern_count = patterns.len();
    let mut search = Search {
        problem,
        options,
        patterns,
        best,
        seen: HashMap::new(),
        states: 0,
        limit: options.beam_width.max(1).saturating_mul(512).min(MAX_STATES),
        exhausted: false,
    };
    let mut remaining = problem.demands.iter().map(|d| d.quantity).collect::<Vec<_>>();
    search.visit(
        &mut remaining,
        &mut vec![0; problem.sheets.len()],
        &mut Vec::new(),
        Prefix::default(),
    );
    search.best.metrics.explored_states =
        search.best.metrics.explored_states.saturating_add(search.states);
    search.best.metrics.notes.push(format!(
        "pattern recombination: {pattern_count} patterns, {} states, budget_exhausted={}",
        search.states, search.exhausted,
    ));
    Some(search.best)
}

fn collect_patterns(problem: &TwoDProblem, candidates: &[TwoDSolution]) -> Vec<Pattern> {
    let mut patterns: Vec<Pattern> = Vec::new();
    let mut keys = HashMap::<(usize, Vec<usize>), usize>::new();
    for candidate in candidates {
        for layout in &candidate.layouts {
            let Some(stock) = problem.sheets.iter().position(|s| s.name == layout.sheet_name)
            else {
                continue;
            };
            let Some(counts) = checked_counts(problem, stock, &layout.placements) else {
                continue;
            };
            let prefix = Prefix {
                waste: layout.waste_area,
                max_drop: layout.largest_usable_drop_area,
                sum_sq: layout.sum_sq_usable_drop_areas,
            };
            let key = (stock, counts.clone());
            if let Some(&index) = keys.get(&key) {
                let old = &patterns[index];
                if prefix.dominates(old.prefix) && (!old.guillotine || candidate.guillotine) {
                    patterns[index] = Pattern {
                        stock,
                        counts,
                        placements: layout.placements.clone(),
                        guillotine: candidate.guillotine,
                        prefix,
                    };
                }
            } else if patterns.len() < MAX_PATTERNS {
                let previous = keys.insert(key, patterns.len());
                debug_assert!(previous.is_none());
                patterns.push(Pattern {
                    stock,
                    counts,
                    placements: layout.placements.clone(),
                    guillotine: candidate.guillotine,
                    prefix,
                });
            }
        }
    }
    patterns.sort_by_key(|pattern| std::cmp::Reverse(pattern.counts.iter().sum::<usize>()));
    patterns
}

/// Independently checks physical geometry and indexed quantities in O(p² + d).
fn checked_counts(
    problem: &TwoDProblem,
    stock: usize,
    placements: &[Placement2D],
) -> Option<Vec<usize>> {
    if placements.is_empty() {
        return None;
    }
    let sheet = &problem.sheets[stock];
    let (width, height) = super::model::effective_bounds(sheet);
    let mut counts = vec![0_usize; problem.demands.len()];
    for (position, p) in placements.iter().enumerate() {
        let index = p.name.parse::<usize>().ok()?;
        let demand = problem.demands.get(index)?;
        let oriented = if p.rotated {
            if !demand.can_rotate {
                return None;
            }
            (demand.height, demand.width)
        } else {
            (demand.width, demand.height)
        };
        if (p.width, p.height) != oriented
            || p.width > sheet.width
            || p.height > sheet.height
            || u64::from(p.x) + u64::from(p.width) > u64::from(width)
            || u64::from(p.y) + u64::from(p.height) > u64::from(height)
        {
            return None;
        }
        for q in placements.iter().take(position) {
            let gap = u64::from(sheet.kerf);
            if !(u64::from(p.x) + u64::from(p.width) + gap <= u64::from(q.x)
                || u64::from(q.x) + u64::from(q.width) + gap <= u64::from(p.x)
                || u64::from(p.y) + u64::from(p.height) + gap <= u64::from(q.y)
                || u64::from(q.y) + u64::from(q.height) + gap <= u64::from(p.y))
            {
                return None;
            }
        }
        counts[index] = counts[index].saturating_add(1);
        if counts[index] > demand.quantity {
            return None;
        }
    }
    Some(counts)
}

impl Search<'_> {
    fn visit(
        &mut self,
        remaining: &mut [usize],
        usage: &mut [usize],
        path: &mut Vec<usize>,
        prefix: Prefix,
    ) {
        if self.states == self.limit {
            self.exhausted = true;
            return;
        }
        self.states += 1;
        let Some(target) = remaining.iter().position(|&count| count > 0) else {
            let layouts = path
                .iter()
                .map(|&i| (self.patterns[i].stock, self.patterns[i].placements.clone()))
                .collect();
            let candidate = TwoDSolution::from_layouts(
                "pattern_recombination",
                path.iter().all(|&i| self.patterns[i].guillotine),
                &self.problem.sheets,
                layouts,
                Vec::new(),
                SolverMetrics2D { iterations: 1, explored_states: 0, notes: Vec::new() },
                self.options.min_usable_side,
            );
            if candidate.is_better_than(&self.best) {
                self.best = candidate;
            }
            return;
        };
        if self.best.unplaced.is_empty() && path.len() >= self.best.sheet_count {
            return;
        }
        let key = Key { remaining: remaining.to_vec(), usage: usage.to_vec() };
        let prefixes = self.seen.entry(key).or_default();
        if prefixes.iter().any(|&p| p.dominates(prefix)) {
            return;
        }
        prefixes.retain(|&p| !prefix.dominates(p));
        prefixes.push(prefix);
        // This loop indexes immutable patterns; recursive updates affect only the
        // separate demand/usage vectors and the bounded search ledger.
        for index in 0..self.patterns.len() {
            if self.exhausted {
                break;
            }
            let pattern = &self.patterns[index];
            let stock = pattern.stock;
            if pattern.counts[target] == 0
                || pattern.counts.iter().zip(remaining.iter()).any(|(&a, &b)| a > b)
                || self.problem.sheets[stock].quantity.is_some_and(|cap| usage[stock] >= cap)
            {
                continue;
            }
            let counts = pattern.counts.clone();
            let next_prefix = prefix.add(pattern.prefix);
            for (left, &count) in remaining.iter_mut().zip(&counts) {
                *left -= count;
            }
            usage[stock] += 1;
            path.push(index);
            self.visit(remaining, usage, path, next_prefix);
            let removed = path.pop();
            debug_assert_eq!(removed, Some(index));
            usage[stock] -= 1;
            for (left, &count) in remaining.iter_mut().zip(&counts) {
                *left += count;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Placement2D, RectDemand2D, Sheet2D, SolverMetrics2D};
    use super::*;

    fn problem() -> TwoDProblem {
        TwoDProblem {
            sheets: vec![Sheet2D {
                name: "stock".into(),
                width: 10,
                height: 10,
                cost: 1.0,
                quantity: None,
                kerf: 0,
                edge_kerf_relief: false,
            }],
            demands: vec![
                RectDemand2D {
                    name: "0".into(),
                    width: 5,
                    height: 10,
                    quantity: 2,
                    can_rotate: false,
                },
                RectDemand2D {
                    name: "1".into(),
                    width: 10,
                    height: 5,
                    quantity: 2,
                    can_rotate: false,
                },
            ],
        }
    }

    fn placement(name: &str, x: u32, y: u32, width: u32, height: u32) -> Placement2D {
        Placement2D { name: name.into(), x, y, width, height, rotated: false }
    }

    fn candidate(problem: &TwoDProblem, layouts: Vec<Vec<Placement2D>>) -> TwoDSolution {
        TwoDSolution::from_layouts(
            "fixture",
            false,
            &problem.sheets,
            layouts.into_iter().map(|p| (0, p)).collect(),
            Vec::new(),
            SolverMetrics2D { iterations: 0, explored_states: 0, notes: Vec::new() },
            0,
        )
    }

    #[test]
    fn combines_patterns_from_different_candidates() {
        let problem = problem();
        let first = candidate(
            &problem,
            vec![
                vec![placement("0", 0, 0, 5, 10), placement("0", 5, 0, 5, 10)],
                vec![placement("1", 0, 0, 10, 5)],
                vec![placement("1", 0, 0, 10, 5)],
            ],
        );
        let second = candidate(
            &problem,
            vec![
                vec![placement("0", 0, 0, 5, 10)],
                vec![placement("0", 0, 0, 5, 10)],
                vec![placement("1", 0, 0, 10, 5), placement("1", 0, 5, 10, 5)],
            ],
        );
        let result = select_patterns(&problem, &[first, second], &TwoDOptions::default())
            .expect("nonempty candidates");
        assert_eq!(result.sheet_count, 2);
        assert!(result.unplaced.is_empty());
        assert_eq!(result.layouts.iter().map(|l| l.placements.len()).sum::<usize>(), 4);
    }
    #[test]
    fn duplicate_names_and_stock_caps_preserve_every_demand() {
        let mut input = problem();
        input.sheets[0].quantity = Some(1);
        for demand in &mut input.demands {
            demand.name = "same".into();
        }
        let result = solve_2d_recombined(
            input.clone(),
            TwoDOptions { multistart_runs: 2, ..TwoDOptions::default() },
        )
        .expect("valid input");
        assert_eq!(result.sheet_count, 1);
        assert_eq!(
            result.layouts.iter().map(|l| l.placements.len()).sum::<usize>()
                + result.unplaced.iter().map(|d| d.quantity).sum::<usize>(),
            4
        );
        for dimensions in [(5, 10), (10, 5)] {
            let placed = result
                .layouts
                .iter()
                .flat_map(|l| &l.placements)
                .filter(|p| (p.width, p.height) == dimensions)
                .count();
            let unplaced = result
                .unplaced
                .iter()
                .filter(|d| (d.width, d.height) == dimensions)
                .map(|d| d.quantity)
                .sum::<usize>();
            assert_eq!(placed + unplaced, 2);
        }
        assert!(result.layouts.iter().flat_map(|l| &l.placements).all(|p| p.name == "same"));
        assert!(result.unplaced.iter().all(|d| d.name == "same"));
    }

    #[test]
    fn rejects_overlapping_or_misidentified_patterns() {
        let input = problem();
        assert!(
            checked_counts(&input, 0, &[placement("0", 0, 0, 5, 10), placement("0", 0, 0, 5, 10)])
                .is_none()
        );
        assert!(checked_counts(&input, 0, &[placement("1", 0, 0, 5, 10)]).is_none());
        assert!(checked_counts(&input, 0, &[placement("missing", 0, 0, 5, 10)]).is_none());
    }

    #[test]
    fn guillotine_pool_keeps_cuttable_layouts() {
        let result = solve_2d_recombined(
            problem(),
            TwoDOptions { guillotine_required: true, multistart_runs: 2, ..TwoDOptions::default() },
        )
        .expect("valid solve");
        assert!(result.guillotine);
        assert_eq!(result.sheet_count, 2);
        assert!(
            super::super::cut_plan::plan_cuts(
                &result,
                &super::super::cut_plan::CutPlanOptions2D::default()
            )
            .is_ok()
        );
    }

    #[test]
    fn public_boundary_rejects_invalid_dimensions() {
        let mut input = problem();
        input.demands[0].width = 0;
        assert!(matches!(
            solve_2d_recombined(input, TwoDOptions::default()),
            Err(BinPackingError::InvalidInput(_))
        ));
    }

    #[test]
    fn recombination_improves_real_portfolio_offcuts() {
        let input: TwoDProblem =
            serde_json::from_str(include_str!("../../tests/fixtures/pattern-offcuts.json"))
                .expect("fixture");
        let options = TwoDOptions {
            multistart_runs: 2,
            auto_rotation_search_max_types: 2,
            beam_width: 4,
            seed: Some(17),
            ..TwoDOptions::default()
        };
        let baseline = super::super::solve_2d(input.clone(), options.clone()).expect("baseline");
        let result = solve_2d_recombined(input.clone(), options).expect("recombined");
        assert!(result.is_better_than(&baseline));
        assert_eq!(result.sheet_count, baseline.sheet_count);
        assert_eq!(result.total_waste_area, baseline.total_waste_area);
        for layout in &result.layouts {
            assert!(checked_counts(&input, 0, &layout.placements).is_some());
        }
        println!(
            "baseline_count={} recombined_count={} baseline_drop={} recombined_drop={} baseline_sum_sq={} recombined_sum_sq={}",
            baseline.sheet_count,
            result.sheet_count,
            baseline.max_usable_drop_area,
            result.max_usable_drop_area,
            baseline.total_sum_sq_usable_drop_areas,
            result.total_sum_sq_usable_drop_areas
        );
    }

    #[test]
    fn recombination_reduces_sheet_count_on_repeated_demands() {
        let input: TwoDProblem =
            serde_json::from_str(include_str!("../../tests/fixtures/pattern-count.json"))
                .expect("fixture");
        let options = TwoDOptions {
            multistart_runs: 2,
            auto_rotation_search_max_types: 2,
            beam_width: 4,
            seed: Some(17),
            ..TwoDOptions::default()
        };
        let baseline = super::super::solve_2d(input.clone(), options.clone()).expect("baseline");
        let result = solve_2d_recombined(input.clone(), options.clone()).expect("recombined");
        assert_eq!(baseline.sheet_count, 6);
        assert_eq!(result.sheet_count, 5);
        assert_eq!(result.total_waste_area, 370);
        assert!(result.unplaced.is_empty());
        assert_eq!(result, solve_2d_recombined(input.clone(), options).expect("repeat"));
        let mut counts = vec![0_usize; input.demands.len()];
        for layout in &result.layouts {
            let found = checked_counts(&input, 0, &layout.placements).expect("physical layout");
            for (total, n) in counts.iter_mut().zip(found) {
                *total += n;
            }
        }
        assert_eq!(counts, input.demands.iter().map(|d| d.quantity).collect::<Vec<_>>());
    }

    #[test]
    fn recombination_keeps_kerf_and_edge_relief_geometry() {
        for kerf in [1, 2] {
            for relief in [false, true] {
                let mut input = problem();
                input.sheets[0].kerf = kerf;
                input.sheets[0].edge_kerf_relief = relief;
                input.demands = vec![RectDemand2D {
                    name: "0".into(),
                    width: 4,
                    height: 4,
                    quantity: 5,
                    can_rotate: true,
                }];
                let options = TwoDOptions { multistart_runs: 2, ..TwoDOptions::default() };
                let baseline =
                    super::super::solve_2d(input.clone(), options.clone()).expect("baseline");
                let result = solve_2d_recombined(input.clone(), options).expect("recombined");
                assert!(!baseline.is_better_than(&result));
                let mut placed = 0;
                for layout in &result.layouts {
                    let counts =
                        checked_counts(&input, 0, &layout.placements).expect("kerf geometry");
                    placed += counts[0];
                }
                assert_eq!(placed, 5);
            }
        }
    }
}
