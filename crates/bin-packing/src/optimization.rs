//! Explicit material, profit, feasibility and open-dimension objectives.
//!
//! Dimension-specific entry points live in each dimension's `optimization`
//! module. Geometry remains native to the selected packing model.

use serde::{Deserialize, Serialize};

/// Primary objective for the additive optimization entry points.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum PackingObjective {
    /// Fulfill demands, then minimize container count.
    #[default]
    BinCount,
    /// Fulfill demands, then minimize material cost.
    TotalCost,
    /// Maximize selected profit while respecting mandatory demand minima.
    Knapsack,
    /// Find a packing fulfilling all demands within declared inventory.
    Feasibility,
    /// Fulfill demands in one container, minimizing its X extent.
    OpenDimensionX,
    /// Fulfill demands in one container, minimizing its Y extent.
    OpenDimensionY,
    /// Fulfill demands in one container, minimizing its Z extent.
    OpenDimensionZ,
    /// Fulfill demands in one container, minimizing its X/Y bounding area.
    OpenDimensionXy,
}

impl PackingObjective {
    #[cfg(any(feature = "one-d", feature = "two-d", feature = "three-d"))]
    pub(crate) fn axes(self) -> &'static [usize] {
        match self {
            Self::OpenDimensionX => &[0],
            Self::OpenDimensionY => &[1],
            Self::OpenDimensionZ => &[2],
            Self::OpenDimensionXy => &[0, 1],
            _ => &[],
        }
    }
}

/// Original packing input plus source-indexed value and minimum-copy data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OptimizationProblem<P> {
    /// Original dimensional packing problem, including physical constraints.
    pub packing: P,
    /// Profit per source demand copy; empty means one for every type.
    #[serde(default)]
    pub profits: Vec<f64>,
    /// Mandatory source quantities for knapsack; empty means all zero.
    #[serde(default)]
    pub minimum_quantities: Vec<usize>,
}

/// Deterministic outer search and per-packing effort controls.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct OptimizationOptions {
    /// Requested primary objective.
    pub objective: PackingObjective,
    /// Maximum complete subset/container-size evaluations, from 1 through 4096.
    pub max_evaluations: usize,
    /// Native effort limit per evaluation, from 0 to 100000. In 1D this caps
    /// graph records and integer-search work separately; in 2D/3D it caps
    /// successor states. Construction and independent validation are separate.
    pub packing_max_states: usize,
    /// Maximum retained outer candidates and native beam width, from 1 to 32.
    pub beam_width: usize,
    /// Seed forwarded to native strategies that use randomized construction.
    pub seed: u64,
}

impl Default for OptimizationOptions {
    fn default() -> Self {
        Self {
            objective: PackingObjective::BinCount,
            max_evaluations: 128,
            packing_max_states: 256,
            beam_width: 8,
            seed: 0,
        }
    }
}

/// Objective evidence. A failed heuristic packing is never infeasibility proof.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OptimizationReport {
    /// Objective used during candidate search and selection.
    pub objective: PackingObjective,
    /// Whether every original requested copy was placed.
    pub complete: bool,
    /// Whether all declared mandatory quantities were placed.
    pub minimums_satisfied: bool,
    /// Actual placed quantity per original source demand.
    pub selected_quantities: Vec<usize>,
    /// Profit of the actual placed source copies.
    pub total_profit: f64,
    /// Number of complete subset/container-size evaluations performed.
    pub evaluations: usize,
    /// Native successor states reported across evaluations; excludes construction.
    pub packing_states: usize,
    /// An effort/beam/geometry limit omitted alternatives.
    pub search_truncated: bool,
    /// Actual returned effective container extents, in X/Y/Z order.
    pub open_extents: Vec<u32>,
    /// Value of the selected primary objective.
    pub primary_value: f64,
    /// A conservative bound when available; large integer measures round in JSON.
    pub primary_bound: Option<f64>,
    /// True for a profit upper bound; false for a minimization lower bound.
    pub bound_is_upper: bool,
    /// A witness meets a valid primary bound; secondary metrics are not certified.
    pub primary_optimal: bool,
}

/// Native geometry and authoritative source provenance for an objective search.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OptimizedSolution<S, P> {
    /// Original input with the returned open container sizes, if applicable.
    pub effective_problem: P,
    /// Dimensional layout, including every omitted original copy as unplaced.
    pub solution: S,
    /// Original stock index for each returned layout.
    pub stock_indices: Vec<usize>,
    /// Original source demand index for each placement, grouped by layout.
    pub demand_indices: Vec<Vec<usize>>,
    /// Explicit objective, work and proof evidence.
    pub report: OptimizationReport,
}

#[cfg(any(feature = "one-d", feature = "two-d", feature = "three-d"))]
mod engine;
#[cfg(any(feature = "one-d", feature = "two-d", feature = "three-d"))]
pub(crate) use engine::{Candidate, Model, solve};
