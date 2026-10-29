//! Same-footprint stack loading with nesting, payload, axle and unloading rules.

use serde::{Deserialize, Serialize};

use super::Rotation3D;
use super::constrained::{ConstrainedProblem3D, PackingObjective3D};
use crate::Result;

mod engine;
mod geometry;
mod validate;

/// Horizontal axis used for axle positions and unloading.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum FloorAxis3D {
    /// The bin width axis.
    X,
    /// The bin depth axis.
    #[default]
    Z,
}

/// Delivery-order rules; group zero unloads first, toward the positive axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum UnloadingConstraint3D {
    /// No delivery-order restriction.
    #[default]
    None,
    /// Later groups cannot block earlier groups along positive X.
    OnlyXMovements,
    /// Later groups cannot block earlier groups along positive Z.
    OnlyZMovements,
    /// Earlier groups cannot have a smaller X origin than later groups.
    IncreasingX,
    /// Earlier groups cannot have a smaller Z origin than later groups.
    IncreasingZ,
}

/// Rules attached to one source demand type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct StackItemRules3D {
    /// Only items with equal identifiers and equal footprints may share a stack.
    pub stackability_id: u32,
    /// Height saved when this item nests into a lower item; zero on the floor.
    pub nesting_height: u32,
    /// Maximum total item count in any stack containing this type.
    pub maximum_stackability: Option<usize>,
    /// Maximum total weight of all items above an item of this type.
    pub maximum_weight_above: Option<u64>,
    /// Delivery group, with lower values unloaded first.
    pub group_id: u32,
}

/// Exact maximum stack weight per unit footprint area.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WeightDensity3D {
    /// Nonnegative numerator of the density limit.
    pub numerator: u64,
    /// Positive denominator of the density limit.
    pub denominator: u64,
}

/// Rectangular floor region where no stack may be placed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FloorDefect3D {
    /// Left floor coordinate.
    pub x: u32,
    /// Near floor coordinate.
    pub z: u32,
    /// Width of the excluded floor region.
    pub width: u32,
    /// Depth of the excluded floor region.
    pub depth: u32,
}

/// Two support/axle locations and their maximum total reactions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AxleLimits3D {
    /// Horizontal coordinate axis for both support positions.
    pub axis: FloorAxis3D,
    /// Position of the first support; must be less than `rear_position`.
    pub front_position: u32,
    /// Position of the second support, within the declared bin.
    pub rear_position: u32,
    /// Maximum cargo-plus-tare reaction at the first support.
    pub front_maximum: u64,
    /// Maximum cargo-plus-tare reaction at the second support.
    pub rear_maximum: u64,
    /// Fixed tare reaction at the first support.
    #[serde(default)]
    pub front_tare: u64,
    /// Fixed tare reaction at the second support.
    #[serde(default)]
    pub rear_tare: u64,
}

/// Additional constraints applying to each copy of one source bin type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct LoadingBinRules3D {
    /// Maximum complete-stack weight per unit footprint area.
    pub maximum_stack_density: Option<WeightDensity3D>,
    /// Optional exact static two-support reaction limits.
    pub axle_limits: Option<AxleLimits3D>,
    /// Floor exclusions; rectangles may overlap one another.
    pub defects: Vec<FloorDefect3D>,
}

/// Indexed stack loading problem. The vertical coordinate is Y.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoadingProblem3D {
    /// Source geometry, upright rotation permissions, costs, inventory and weights.
    pub packing: ConstrainedProblem3D,
    /// One record per source demand, or empty for default rules.
    #[serde(default)]
    pub item_rules: Vec<StackItemRules3D>,
    /// One record per source bin, or empty for default rules.
    #[serde(default)]
    pub bin_rules: Vec<LoadingBinRules3D>,
    /// Floor-based delivery-order restriction.
    #[serde(default)]
    pub unloading: UnloadingConstraint3D,
}

/// Deterministic bounded stack construction controls.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LoadingOptions3D {
    /// Objective used after minimizing unplaced item count.
    pub objective: PackingObjective3D,
    /// Maximum retained candidates; from one through 32.
    pub beam_width: usize,
    /// Maximum cloned search successors; zero returns greedy construction.
    pub max_states: usize,
}

impl Default for LoadingOptions3D {
    fn default() -> Self {
        Self { objective: PackingObjective3D::BinCount, beam_width: 8, max_states: 4_096 }
    }
}

/// One physical item in a stack, with authoritative source identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StackedItem3D {
    /// Index into the original demand vector.
    pub demand_index: usize,
    /// Physical bottom coordinate; authorized nesting may overlap the lower item.
    pub y: u32,
    /// Legal upright orientation of the source item.
    pub rotation: Rotation3D,
}

/// A floor-supported column of compatible, equal-footprint items.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StackColumn3D {
    /// Left floor position.
    pub x: u32,
    /// Near floor position.
    pub z: u32,
    /// Common rotated item width.
    pub width: u32,
    /// Common rotated item depth.
    pub depth: u32,
    /// Physical top of the entire column, including its full upper item.
    pub height: u32,
    /// Checked total weight of the column.
    pub weight: u64,
    /// Bottom-to-top source items.
    pub items: Vec<StackedItem3D>,
}

/// Conservative integer ceilings of the exact two-support reactions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AxleLoads3D {
    /// Ceiling of the cargo-plus-tare reaction at the first support.
    pub front: u64,
    /// Ceiling of the cargo-plus-tare reaction at the second support.
    pub rear: u64,
}

/// One loaded copy of an indexed bin type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoadingBinLayout3D {
    /// Index into the source bin vector.
    pub bin_index: usize,
    /// Actual cargo weight; tare is reported only in axle reactions.
    pub payload: u64,
    /// Nonoverlapping floor columns.
    pub stacks: Vec<StackColumn3D>,
    /// Present exactly when this source bin has axle constraints.
    pub axle_loads: Option<AxleLoads3D>,
}

/// Indexed demand quantity not loaded by this bounded search.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnplacedLoadingItem3D {
    /// Original demand vector index.
    pub demand_index: usize,
    /// Unplaced copies of that source demand.
    pub quantity: usize,
}

/// Validated stack loading and bounded-search evidence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoadingSolution3D {
    /// Packed source-bin copies, in construction order.
    pub layouts: Vec<LoadingBinLayout3D>,
    /// Missing source quantities in increasing demand-index order.
    pub unplaced: Vec<UnplacedLoadingItem3D>,
    /// Number of loaded bins.
    pub bin_count: usize,
    /// Sum of consumed source-bin material costs.
    pub total_cost: f64,
    /// Evaluated cloned successors, excluding baseline construction.
    pub explored_states: usize,
    /// A bounded search or shortlist omitted alternatives; no optimality claim.
    pub search_truncated: bool,
}

/// Construct compatible nested stacks while respecting loading constraints.
///
/// # Errors
/// Returns structured input, constraint or independent output-validation errors.
pub fn solve_loading_3d(
    problem: LoadingProblem3D,
    options: LoadingOptions3D,
) -> Result<LoadingSolution3D> {
    engine::solve(problem, options)
}

/// Independently check stack identity, nesting, loading rules and exact demand accounting.
///
/// # Errors
/// Returns malformed-input errors or [`crate::BinPackingError::InvalidSolution`].
pub fn validate_loading_3d(problem: &LoadingProblem3D, result: &LoadingSolution3D) -> Result<()> {
    validate::validate(problem, result)
}

#[cfg(test)]
mod tests;
