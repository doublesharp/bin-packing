//! Opt-in, forecast-dependent reuse of physically separated offcuts.
//!
//! Evaluation first constructs a replayable guillotine saw decomposition. Only its
//! disjoint terminal waste pieces enter a deterministic greedy future-part packer.
//! The score is a feasible weighted fulfillment, not a bound or an optimal value.
//! Current and future cuts use the source stock's kerf and edge-relief permission.

use serde::{Deserialize, Serialize};

use super::saw_plan::{
    SawJob2D, SawLayout2D, SawPlanError2D, SawPlanOptions2D, SawRect2D, SawSearchLimits2D,
    WorkpieceId2D, plan_saw_cuts,
};
use super::{TwoDAlgorithm, TwoDOptions, TwoDProblem, TwoDSolution, solve_2d};

/// One indexed future demand. Names are unnecessary because indices preserve identity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ForecastPart2D {
    /// Finished width, 1 through 2^30 input units.
    pub width: u32,
    /// Finished height, 1 through 2^30 input units.
    pub height: u32,
    /// Maximum future pieces of this shape to value, zero through 4096.
    pub quantity: usize,
    /// Whether a quarter-turn is permitted.
    pub can_rotate: bool,
    /// Finite nonnegative value per fulfilled piece, at most 1e100.
    pub weight: f64,
}

/// Explicit user-supplied forecast. An empty forecast has zero value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ReuseForecast2D {
    /// At most 64 shapes and 4096 total pieces. Zero weights consume no offcuts.
    pub demands: Vec<ForecastPart2D>,
}

/// Bounded controls and optional material ceilings for the opt-in API.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ReuseOptions2D {
    /// Top-level solves including the requested baseline, 1 through 8.
    /// Auto and multistart may internally run several bounded strategies.
    pub max_candidate_solves: usize,
    /// Maximum future-piece placement attempts, 1 through 512.
    pub max_future_parts: usize,
    /// Explicit sheet ceiling. Defaults to baseline sheet count in the solve API.
    pub max_sheet_count: Option<usize>,
    /// Finite nonnegative material ceiling. Defaults to baseline cost in the solve API.
    pub max_material_cost: Option<f64>,
}

impl Default for ReuseOptions2D {
    fn default() -> Self {
        Self {
            max_candidate_solves: 6,
            max_future_parts: 256,
            max_sheet_count: None,
            max_material_cost: None,
        }
    }
}

/// One physically separated terminal waste piece, in original sheet coordinates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecoverableOffcut2D {
    /// Sheet instance and exact disjoint physical bounds.
    pub workpiece: WorkpieceId2D,
    /// Original source stock index, independent of repeated labels.
    pub stock_index: usize,
    /// Blade kerf used for subsequent subdivision of this offcut.
    pub kerf: u32,
}

/// One feasible future part allocated to a disjoint physical offcut.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReusePlacement2D {
    /// Index in the caller's forecast demands.
    pub forecast_index: usize,
    /// Index in the evaluation's physical offcuts.
    pub offcut_index: usize,
    /// Original-sheet bounds of the finished future part.
    pub bounds: SawRect2D,
    /// Whether the forecast dimensions were rotated.
    pub rotated: bool,
}

/// Forecast value of a physical decomposition, without altering the current layout.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReuseEvaluation2D {
    /// Sum of forecast weight times fulfilled quantity. Depends on the forecast.
    pub weighted_fulfillment: f64,
    /// Fulfilled quantity per input forecast index.
    pub fulfilled_counts: Vec<usize>,
    /// Disjoint, extractable terminal pieces from the current job's saw plan.
    pub offcuts: Vec<RecoverableOffcut2D>,
    /// Greedy feasible future placements, each consuming space only once.
    pub placements: Vec<ReusePlacement2D>,
    /// The configured future-part attempt cap stopped evaluation early.
    pub evaluation_truncated: bool,
}

/// Selected solution and explicit provenance for the reuse experiment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReuseSolution2D {
    /// Ordinary packing result with original names restored.
    pub solution: TwoDSolution,
    /// Indexed source identity for every selected sheet and placement.
    pub job: SawJob2D,
    /// Forecast evaluation of the requested algorithm before reranking.
    pub baseline_evaluation: ReuseEvaluation2D,
    /// Forecast evaluation of the selected candidate.
    pub evaluation: ReuseEvaluation2D,
    /// Default sheet ceiling, before any explicitly supplied override.
    pub baseline_sheet_count: usize,
    /// Default cost ceiling, before any explicitly supplied override.
    pub baseline_material_cost: f64,
    /// Top-level solves attempted, including the baseline.
    pub candidate_solves: usize,
    /// Candidates rejected for exceeding the baseline or explicit material ceilings.
    pub material_rejections: usize,
    /// Candidates rejected because a physical saw decomposition could not be found.
    pub physical_rejections: usize,
}

/// Structured validation and feasibility failures of the opt-in reuse API.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum ReuseError2D {
    /// Invalid forecast dimension, weight, quantity, or total forecast size.
    #[error("invalid reuse forecast at index {index}: {reason}")]
    InvalidForecast {
        /// Shape index, or demands.len() for an aggregate limit.
        index: usize,
        /// Specific rejected constraint.
        reason: &'static str,
    },
    /// Invalid or unbounded solve/evaluation configuration.
    #[error("invalid reuse option: {field}")]
    InvalidOption {
        /// Rejected field or input-size bound.
        field: &'static str,
    },
    /// Current problem or solver failure.
    #[error("invalid reuse problem: {0}")]
    InvalidProblem(String),
    /// Explicit material budget was exceeded.
    #[error("reuse material budget exceeded: {sheet_count} sheets, cost {cost}")]
    MaterialBudget {
        /// Actual sheet count.
        sheet_count: usize,
        /// Actual cost recomputed from indexed stock.
        cost: f64,
    },
    /// Input geometry or physical decomposition was rejected.
    #[error(transparent)]
    Physical(#[from] SawPlanError2D),
}

fn validate(forecast: &ReuseForecast2D, options: &ReuseOptions2D) -> Result<(), ReuseError2D> {
    if !(1..=8).contains(&options.max_candidate_solves)
        || !(1..=512).contains(&options.max_future_parts)
    {
        return Err(ReuseError2D::InvalidOption { field: "search limits" });
    }
    if options.max_material_cost.is_some_and(|c| !c.is_finite() || c < 0.0) {
        return Err(ReuseError2D::InvalidOption { field: "max_material_cost" });
    }
    let mut quantity = 0usize;
    for (index, part) in forecast.demands.iter().enumerate() {
        let reason =
            if part.width == 0 || part.height == 0 || part.width > 1 << 30 || part.height > 1 << 30
            {
                Some("dimensions must be 1 through 2^30")
            } else if !part.weight.is_finite() || !(0.0..=1e100).contains(&part.weight) {
                Some("weight must be finite and between zero and 1e100")
            } else if part.quantity > 4096 {
                Some("quantity exceeds 4096")
            } else {
                None
            };
        if let Some(reason) = reason {
            return Err(ReuseError2D::InvalidForecast { index, reason });
        }
        quantity = quantity.saturating_add(part.quantity);
    }
    if forecast.demands.len() > 64 || quantity > 4096 {
        return Err(ReuseError2D::InvalidForecast {
            index: forecast.demands.len(),
            reason: "forecast exceeds 64 shapes or 4096 pieces",
        });
    }
    Ok(())
}

fn check_material(job: &SawJob2D, options: &ReuseOptions2D) -> Result<(), ReuseError2D> {
    let mut cost = 0.0;
    for &index in &job.stock_indices {
        let stock = job
            .problem
            .sheets
            .get(index)
            .ok_or(ReuseError2D::InvalidOption { field: "source stock index out of range" })?;
        cost += stock.cost;
    }
    if options.max_sheet_count.is_some_and(|limit| job.layouts.len() > limit)
        || options.max_material_cost.is_some_and(|limit| cost > limit)
    {
        return Err(ReuseError2D::MaterialBudget { sheet_count: job.layouts.len(), cost });
    }
    Ok(())
}

/// Evaluate a forecast using only terminal offcuts from a physical saw plan.
///
/// Work is bounded to 32 layouts, 128 current placements, 64 forecast shapes and
/// 512 future placement attempts. Each future placement makes a vertical cut,
/// then a horizontal cut as needed, retaining disjoint right/bottom rectangles.
/// Complexity is O(F A^2) for F forecast shapes and A bounded future attempts.
/// Weights determine greedy order; equal weights use larger area then input index.
///
/// # Errors
/// Returns structured errors for invalid forecasts/options, explicit material
/// ceilings, invalid indexed layouts, or unavailable physical decompositions.
/// Mixed blade kerfs require separate jobs, as in the underlying saw planner.
pub fn evaluate_reuse(
    job: &SawJob2D,
    forecast: &ReuseForecast2D,
    options: &ReuseOptions2D,
) -> Result<ReuseEvaluation2D, ReuseError2D> {
    validate(forecast, options)?;
    if job.layouts.len() > 32 || job.layouts.iter().map(|l| l.placements.len()).sum::<usize>() > 128
    {
        return Err(ReuseError2D::InvalidOption {
            field: "job exceeds 32 sheets or 128 placements",
        });
    }
    let saw = plan_saw_cuts(
        job,
        &SawPlanOptions2D {
            search: SawSearchLimits2D {
                max_states: 0,
                baseline_max_states: 10000,
                ..Default::default()
            },
            ..Default::default()
        },
    )?;
    check_material(job, options)?;
    let offcuts: Vec<_> = saw
        .workpieces
        .iter()
        .filter(|p| p.usable_offcut)
        .map(|p| {
            let stock_index = job.stock_indices[p.id.sheet_index];
            RecoverableOffcut2D {
                workpiece: p.id,
                stock_index,
                kerf: job.problem.sheets[stock_index].kerf,
            }
        })
        .collect();
    let mut free: Vec<_> =
        offcuts.iter().enumerate().map(|(i, p)| (i, p.workpiece.bounds)).collect();
    let mut order: Vec<_> = (0..forecast.demands.len()).collect();
    order.sort_by(|&a, &b| {
        let a = &forecast.demands[a];
        let b = &forecast.demands[b];
        b.weight.total_cmp(&a.weight).then_with(|| {
            (u64::from(b.width) * u64::from(b.height))
                .cmp(&(u64::from(a.width) * u64::from(a.height)))
        })
    });
    let mut result = ReuseEvaluation2D {
        weighted_fulfillment: 0.0,
        fulfilled_counts: vec![0; forecast.demands.len()],
        offcuts,
        placements: Vec::new(),
        evaluation_truncated: false,
    };
    let mut attempts = 0;
    for index in order {
        let part = &forecast.demands[index];
        if part.weight == 0.0 {
            continue;
        }
        for _ in 0..part.quantity {
            if attempts == options.max_future_parts {
                result.evaluation_truncated = true;
                return Ok(result);
            }
            attempts += 1;
            let mut choice = None;
            'find: for (free_index, &(offcut_index, bounds)) in free.iter().enumerate() {
                let offcut = &result.offcuts[offcut_index];
                let relief = job.problem.sheets[offcut.stock_index].edge_kerf_relief;
                for rotated in [false, true] {
                    if rotated && (!part.can_rotate || part.width == part.height) {
                        continue;
                    }
                    let (width, height) =
                        if rotated { (part.height, part.width) } else { (part.width, part.height) };
                    if let (Some(right), Some(bottom)) = (
                        remaining(bounds.width, width, offcut.kerf, relief),
                        remaining(bounds.height, height, offcut.kerf, relief),
                    ) {
                        choice = Some((
                            free_index,
                            offcut_index,
                            bounds,
                            width,
                            height,
                            right,
                            bottom,
                            rotated,
                        ));
                        break 'find;
                    }
                }
            }
            let Some((free_index, offcut_index, bounds, width, height, right, bottom, rotated)) =
                choice
            else {
                break;
            };
            let removed = free.swap_remove(free_index);
            debug_assert_eq!(removed, (offcut_index, bounds));
            let kerf = result.offcuts[offcut_index].kerf;
            if right > 0 {
                free.push((
                    offcut_index,
                    SawRect2D {
                        x: bounds.x.saturating_add(width).saturating_add(kerf),
                        y: bounds.y,
                        width: right,
                        height: bounds.height,
                    },
                ));
            }
            if bottom > 0 {
                free.push((
                    offcut_index,
                    SawRect2D {
                        x: bounds.x,
                        y: bounds.y.saturating_add(height).saturating_add(kerf),
                        width,
                        height: bottom,
                    },
                ));
            }
            result.placements.push(ReusePlacement2D {
                forecast_index: index,
                offcut_index,
                bounds: SawRect2D { width, height, ..bounds },
                rotated,
            });
            result.fulfilled_counts[index] += 1;
            result.weighted_fulfillment += part.weight;
        }
    }
    Ok(result)
}

fn remaining(extent: u32, size: u32, kerf: u32, relief: bool) -> Option<u32> {
    if size > extent {
        return None;
    }
    if size == extent {
        return Some(0);
    }
    let leftover = extent - size;
    if leftover >= kerf {
        Some(leftover - kerf)
    } else if relief {
        Some(0)
    } else {
        None
    }
}

fn indexed_problem(problem: &TwoDProblem) -> TwoDProblem {
    let mut indexed = problem.clone();
    for (i, sheet) in indexed.sheets.iter_mut().enumerate() {
        sheet.name = format!("reuse_stock_{i}");
    }
    for (i, demand) in indexed.demands.iter_mut().enumerate() {
        demand.name = format!("reuse_demand_{i}");
    }
    indexed
}

fn source_job(
    problem: &TwoDProblem,
    indexed: &TwoDProblem,
    solution: &mut TwoDSolution,
) -> Result<SawJob2D, ReuseError2D> {
    let mut stock_indices = Vec::new();
    let mut demand_indices = Vec::new();
    for layout in &mut solution.layouts {
        let stock = indexed
            .sheets
            .iter()
            .position(|s| s.name == layout.sheet_name)
            .ok_or(ReuseError2D::InvalidOption { field: "solver returned unknown stock" })?;
        stock_indices.push(stock);
        layout.sheet_name = problem.sheets[stock].name.clone();
        let mut indices = Vec::new();
        for p in &mut layout.placements {
            let demand =
                indexed.demands.iter().position(|d| d.name == p.name).ok_or(
                    ReuseError2D::InvalidOption { field: "solver returned unknown demand" },
                )?;
            indices.push(demand);
            p.name = problem.demands[demand].name.clone();
        }
        demand_indices.push(indices);
    }
    for demand in &mut solution.unplaced {
        let index = indexed.demands.iter().position(|d| d.name == demand.name).ok_or(
            ReuseError2D::InvalidOption { field: "solver returned unknown unplaced demand" },
        )?;
        demand.name = problem.demands[index].name.clone();
    }
    Ok(SawJob2D {
        problem: problem.clone(),
        layouts: solution.layouts.iter().map(SawLayout2D::from).collect(),
        stock_indices,
        demand_indices,
    })
}

/// Solve with explicit forecast reranking of a bounded candidate family.
///
/// Ordinary [`solve_2d`] and [`TwoDOptions`] retain their existing behavior. The
/// requested algorithm supplies the baseline. Candidates may improve weighted
/// reuse only without worsening current unplaced quantity. Equal forecast scores
/// retain the ordinary comparator. Sheet count and cost default to baseline
/// ceilings; callers may explicitly override either. All inventory caps remain hard.
/// Current jobs are capped at 8 stock types, 64 demand types and 128 total parts.
/// Options must use beam width 1..=32, multistart runs 1..=16, and the rotation enumeration threshold is capped at 8.
/// A fixed fallback seed of zero makes unspecified seeds reproducible here.
/// The baseline itself must admit a physical saw decomposition.
///
/// # Errors
/// Returns [`ReuseError2D`] for validation, work limits, material ceilings, solver
/// failure or an unavailable baseline physical decomposition. No optimality claim
/// is made about the candidate family or the supplied forecast.
pub fn solve_2d_with_reuse(
    problem: TwoDProblem,
    mut options: TwoDOptions,
    forecast: &ReuseForecast2D,
    reuse: &ReuseOptions2D,
) -> Result<ReuseSolution2D, ReuseError2D> {
    validate(forecast, reuse)?;
    if problem.sheets.len() > 8
        || problem.demands.len() > 64
        || problem.demands.iter().fold(0usize, |n, d| n.saturating_add(d.quantity)) > 128
    {
        return Err(ReuseError2D::InvalidOption { field: "problem exceeds bounded reuse size" });
    }
    if !(1..=32).contains(&options.beam_width) || !(1..=16).contains(&options.multistart_runs) {
        return Err(ReuseError2D::InvalidOption { field: "unbounded solver controls" });
    }
    options.auto_rotation_search_max_types = options.auto_rotation_search_max_types.min(8);
    options.seed = Some(options.seed.unwrap_or(0));
    let indexed = indexed_problem(&problem);
    indexed.validate().map_err(|e| ReuseError2D::InvalidProblem(e.to_string()))?;
    let mut solution = solve_2d(indexed.clone(), options.clone())
        .map_err(|e| ReuseError2D::InvalidProblem(e.to_string()))?;
    let mut job = source_job(&problem, &indexed, &mut solution)?;
    let baseline_sheet_count = solution.sheet_count;
    let baseline_material_cost = solution.total_cost;
    let effective = ReuseOptions2D {
        max_sheet_count: Some(reuse.max_sheet_count.unwrap_or(baseline_sheet_count)),
        max_material_cost: Some(reuse.max_material_cost.unwrap_or(baseline_material_cost)),
        ..reuse.clone()
    };
    let baseline_evaluation = evaluate_reuse(&job, forecast, &effective)?;
    let mut evaluation = baseline_evaluation.clone();
    let mut candidate_solves = 1;
    let mut material_rejections = 0;
    let mut physical_rejections = 0;
    for algorithm in [
        TwoDAlgorithm::Guillotine,
        TwoDAlgorithm::GuillotineBestShortSideFit,
        TwoDAlgorithm::GuillotineShorterLeftoverAxis,
        TwoDAlgorithm::GuillotineLongerLeftoverAxis,
        TwoDAlgorithm::MaxRectsBottomLeft,
        TwoDAlgorithm::MaxRectsContactPoint,
        TwoDAlgorithm::Skyline,
    ] {
        if candidate_solves == reuse.max_candidate_solves {
            break;
        }
        if algorithm == options.algorithm {
            continue;
        }
        candidate_solves += 1;
        let mut candidate = solve_2d(indexed.clone(), TwoDOptions { algorithm, ..options.clone() })
            .map_err(|e| ReuseError2D::InvalidProblem(e.to_string()))?;
        let candidate_job = source_job(&problem, &indexed, &mut candidate)?;
        if let Err(error) = check_material(&candidate_job, &effective) {
            match error {
                ReuseError2D::MaterialBudget { .. } => {
                    material_rejections += 1;
                    continue;
                }
                other => return Err(other),
            }
        }
        let unplaced = |s: &TwoDSolution| s.unplaced.iter().map(|d| d.quantity).sum::<usize>();
        if unplaced(&candidate) > unplaced(&solution) {
            continue;
        }
        let candidate_evaluation = match evaluate_reuse(&candidate_job, forecast, &effective) {
            Ok(value) => value,
            Err(ReuseError2D::Physical(_)) => {
                physical_rejections += 1;
                continue;
            }
            Err(other) => return Err(other),
        };
        if unplaced(&candidate) < unplaced(&solution)
            || candidate_evaluation.weighted_fulfillment > evaluation.weighted_fulfillment
            || (candidate_evaluation.weighted_fulfillment == evaluation.weighted_fulfillment
                && candidate.is_better_than(&solution))
        {
            solution = candidate;
            job = candidate_job;
            evaluation = candidate_evaluation;
        }
    }
    Ok(ReuseSolution2D {
        solution,
        job,
        baseline_evaluation,
        evaluation,
        baseline_sheet_count,
        baseline_material_cost,
        candidate_solves,
        material_rejections,
        physical_rejections,
    })
}
