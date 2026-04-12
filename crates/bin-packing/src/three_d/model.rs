//! Data model types for 3D rectangular bin packing problems and solutions.

use serde::{Deserialize, Serialize};

use crate::{BinPackingError, Result};

/// Maximum supported per-axis dimension. Chosen so that
/// `MAX_DIMENSION_3D^3 = 2^45` leaves 2^19 of headroom in `u64` for
/// per-bin / per-multistart accumulation. Smaller than the 1D/2D
/// `1 << 30` cap by design — the cube of `1 << 30` overflows `u64`.
pub const MAX_DIMENSION_3D: u32 = 1 << 15;

/// Maximum number of bins a 3D solution may consume. Solvers that would
/// produce more bins than this should abort with `InvalidInput`.
pub const MAX_BIN_COUNT_3D: usize = 1 << 15;

/// Algorithm selector for [`solve_3d`](super::solve_3d).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ThreeDAlgorithm {
    /// Try multiple strategies and return the best.
    #[default]
    Auto,
    /// Extreme Points construction with volume-fit residual scoring (default EP).
    ExtremePoints,
    /// Extreme Points construction with residual-space scoring (Crainic-Perboli-Tadei "RS").
    ExtremePointsResidualSpace,
    /// Extreme Points construction with free-volume scoring (Crainic-Perboli-Tadei "FV").
    ExtremePointsFreeVolume,
    /// Extreme Points construction with bottom-left-back tiebreaking.
    ExtremePointsBottomLeftBack,
    /// Extreme Points construction with contact-point scoring.
    ExtremePointsContactPoint,
    /// Extreme Points construction with Euclidean-distance scoring (Crainic-Perboli-Tadei "EU").
    ExtremePointsEuclidean,
    // Note: serde's `rename_all = "snake_case"` would convert `Guillotine3D`
    // to `guillotine3_d`, which contradicts the wire format. Each
    // digit-containing variant uses an explicit rename to match the spec.
    /// Guillotine 3D beam search with best-volume-fit ranking.
    #[serde(rename = "guillotine_3d")]
    Guillotine3D,
    /// Guillotine 3D beam search ranked by shortest leftover edge.
    #[serde(rename = "guillotine_3d_best_short_side_fit")]
    Guillotine3DBestShortSideFit,
    /// Guillotine 3D beam search ranked by longest leftover edge.
    #[serde(rename = "guillotine_3d_best_long_side_fit")]
    Guillotine3DBestLongSideFit,
    /// Guillotine 3D beam search splitting along the shortest leftover axis.
    #[serde(rename = "guillotine_3d_shorter_leftover_axis")]
    Guillotine3DShorterLeftoverAxis,
    /// Guillotine 3D beam search splitting along the longest leftover axis.
    #[serde(rename = "guillotine_3d_longer_leftover_axis")]
    Guillotine3DLongerLeftoverAxis,
    /// Guillotine 3D beam search minimising the new sub-cuboid volume on split.
    #[serde(rename = "guillotine_3d_min_volume_split")]
    Guillotine3DMinVolumeSplit,
    /// Guillotine 3D beam search maximising the new sub-cuboid volume on split.
    #[serde(rename = "guillotine_3d_max_volume_split")]
    Guillotine3DMaxVolumeSplit,
    /// Layer-building (horizontal layers) with `auto` 2D inner backend.
    LayerBuilding,
    /// Layer-building with the `max_rects` 2D inner backend.
    LayerBuildingMaxRects,
    /// Layer-building with the `skyline` 2D inner backend.
    LayerBuildingSkyline,
    /// Layer-building with the `guillotine` 2D inner backend.
    LayerBuildingGuillotine,
    /// Layer-building with the `best_fit_decreasing_height` shelf inner backend.
    LayerBuildingShelf,
    /// Bischoff & Marriott vertical wall-building.
    WallBuilding,
    /// Column / vertical-stack building with 2D footprint packing.
    ColumnBuilding,
    /// Deepest-Bottom-Left placement (Karabulut & İnceoğlu).
    DeepestBottomLeft,
    /// Deepest-Bottom-Left-Fill placement.
    DeepestBottomLeftFill,
    /// First-fit decreasing by volume.
    FirstFitDecreasingVolume,
    /// Best-fit decreasing by volume.
    BestFitDecreasingVolume,
    /// Multi-start randomized EP meta-strategy.
    MultiStart,
    /// GRASP construction + local search.
    Grasp,
    /// Standalone local search seeded from FFD.
    LocalSearch,
    /// Restricted Martello-Pisinger-Vigo branch-and-bound exact backend.
    BranchAndBound,
}

/// A bin (container) entry that demands can be placed inside.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bin3D {
    /// Human-readable identifier for this bin type.
    pub name: String,
    /// Bin width (x axis).
    pub width: u32,
    /// Bin height (y axis, vertical).
    pub height: u32,
    /// Bin depth (z axis).
    pub depth: u32,
    /// Per-unit cost of consuming a bin of this type.
    #[serde(default = "default_bin_cost")]
    pub cost: f64,
    /// Optional cap on the number of bins of this type that may be used.
    #[serde(default)]
    pub quantity: Option<usize>,
}

fn default_bin_cost() -> f64 {
    1.0
}

fn default_rotation_mask() -> RotationMask3D {
    RotationMask3D::ALL
}

/// A demand for a set of identical rectangular boxes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoxDemand3D {
    /// Human-readable identifier for the demand.
    pub name: String,
    /// Required width (declared x-extent).
    pub width: u32,
    /// Required height (declared y-extent).
    pub height: u32,
    /// Required depth (declared z-extent).
    pub depth: u32,
    /// Number of identical boxes required.
    pub quantity: usize,
    /// Bitmask of axis-permutation rotations the solver may apply to this box.
    /// Defaults to [`RotationMask3D::ALL`].
    #[serde(default = "default_rotation_mask")]
    pub allowed_rotations: RotationMask3D,
}

/// One of the six axis-permutation rotations of a rectangular box.
///
/// Each variant denotes the permutation `(input_a, input_b, input_c)` where
/// each letter selects which declared extent maps onto the bin's x, y, and z
/// axis respectively. For example, [`Rotation3D::Zxy`] applied to a box
/// declared as `(width=3, height=5, depth=7)` produces placement extents
/// `(x_extent=7, y_extent=3, z_extent=5)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Rotation3D {
    /// Identity: `(w, h, d)` → `(w, h, d)`.
    Xyz,
    /// `(w, h, d)` → `(w, d, h)`.
    Xzy,
    /// `(w, h, d)` → `(h, w, d)`.
    Yxz,
    /// `(w, h, d)` → `(h, d, w)`.
    Yzx,
    /// `(w, h, d)` → `(d, w, h)`.
    Zxy,
    /// `(w, h, d)` → `(d, h, w)`.
    Zyx,
}

impl Rotation3D {
    /// Apply this rotation to a declared `(w, h, d)` box, returning the
    /// `(x_extent, y_extent, z_extent)` of the placed box.
    pub fn apply(self, width: u32, height: u32, depth: u32) -> (u32, u32, u32) {
        match self {
            Self::Xyz => (width, height, depth),
            Self::Xzy => (width, depth, height),
            Self::Yxz => (height, width, depth),
            Self::Yzx => (height, depth, width),
            Self::Zxy => (depth, width, height),
            Self::Zyx => (depth, height, width),
        }
    }

    /// Bit position of this rotation inside [`RotationMask3D`].
    pub(crate) fn bit(self) -> u8 {
        match self {
            Self::Xyz => 0,
            Self::Xzy => 1,
            Self::Yxz => 2,
            Self::Yzx => 3,
            Self::Zxy => 4,
            Self::Zyx => 5,
        }
    }
}

/// Bitmask over the six [`Rotation3D`] axis permutations.
///
/// A box has 24 orientation-preserving rotations in 3D, but only the six
/// distinct *axis permutations* of `(w, h, d)` produce different placements
/// (the other 18 collapse onto these because the box is unlabelled).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RotationMask3D(u8);

impl RotationMask3D {
    /// Identity (`Rotation3D::Xyz`) only.
    pub const XYZ: Self = Self(1 << 0);
    /// `Rotation3D::Xzy` only.
    pub const XZY: Self = Self(1 << 1);
    /// `Rotation3D::Yxz` only.
    pub const YXZ: Self = Self(1 << 2);
    /// `Rotation3D::Yzx` only.
    pub const YZX: Self = Self(1 << 3);
    /// `Rotation3D::Zxy` only.
    pub const ZXY: Self = Self(1 << 4);
    /// `Rotation3D::Zyx` only.
    pub const ZYX: Self = Self(1 << 5);
    /// All six axis permutations.
    pub const ALL: Self = Self(0b00111111);
    /// Only the two permutations that preserve the y-axis as vertical
    /// (`Xyz` and `Zyx`).
    pub const UPRIGHT: Self = Self(0b00100001);
    /// Empty mask. Rejected at validation time.
    pub const NONE: Self = Self(0);

    /// Whether this mask contains the given rotation.
    pub fn contains(self, rotation: Rotation3D) -> bool {
        (self.0 & (1 << rotation.bit())) != 0
    }

    /// Whether the mask is empty.
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Iterator over the rotations contained in this mask, in `Rotation3D`
    /// declaration order (`Xyz`, `Xzy`, `Yxz`, `Yzx`, `Zxy`, `Zyx`).
    pub fn iter(self) -> impl Iterator<Item = Rotation3D> {
        const ALL: [Rotation3D; 6] = [
            Rotation3D::Xyz,
            Rotation3D::Xzy,
            Rotation3D::Yxz,
            Rotation3D::Yzx,
            Rotation3D::Zxy,
            Rotation3D::Zyx,
        ];
        ALL.into_iter().filter(move |rot| self.contains(*rot))
    }
}

/// A single placed box on a packed bin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Placement3D {
    /// Name of the originating demand.
    pub name: String,
    /// X offset of the box's near-bottom-left corner.
    pub x: u32,
    /// Y offset of the box's near-bottom-left corner.
    pub y: u32,
    /// Z offset of the box's near-bottom-left corner.
    pub z: u32,
    /// Width of the placed box after rotation.
    pub width: u32,
    /// Height of the placed box after rotation.
    pub height: u32,
    /// Depth of the placed box after rotation.
    pub depth: u32,
    /// Rotation applied relative to the demand's declared `(w, h, d)`.
    pub rotation: Rotation3D,
}

/// A single packed bin layout produced by the solver.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BinLayout3D {
    /// Name of the bin type consumed by this layout.
    pub bin_name: String,
    /// Bin width.
    pub width: u32,
    /// Bin height.
    pub height: u32,
    /// Bin depth.
    pub depth: u32,
    /// Cost of consuming this bin.
    pub cost: f64,
    /// Boxes placed in this bin.
    pub placements: Vec<Placement3D>,
    /// Total volume occupied by the placements.
    pub used_volume: u64,
    /// Total wasted volume in this bin.
    pub waste_volume: u64,
}

/// Per-bin procurement summary for a 3D solution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BinRequirement3D {
    /// Name of the bin type.
    pub bin_name: String,
    /// Bin width.
    pub bin_width: u32,
    /// Bin height.
    pub bin_height: u32,
    /// Bin depth.
    pub bin_depth: u32,
    /// Cost of consuming one bin of this type.
    pub cost: f64,
    /// Declared inventory available for this bin type, if capped.
    pub available_quantity: Option<usize>,
    /// Number of bins used in the returned solution.
    pub used_quantity: usize,
    /// Number of bins required to satisfy the full demand under the chosen mix.
    pub required_quantity: usize,
    /// Additional bins needed beyond `available_quantity`.
    pub additional_quantity_needed: usize,
}

/// Metrics captured while running a 3D solver.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SolverMetrics3D {
    /// Number of top-level solver iterations performed. The exact meaning
    /// depends on the algorithm: deterministic constructive heuristics
    /// (`extreme_points*`, `deepest_bottom_left*`, `wall_building`,
    /// `column_building`, `layer_building*`,
    /// `first_fit_decreasing_volume`, `best_fit_decreasing_volume`)
    /// report 1; `multi_start` reports the number of restart loops it
    /// executed; `local_search` reports the total number of improvement
    /// passes; `grasp` reports the number of restart loops; `guillotine_3d*`
    /// reports the number of beam-expansion steps; `branch_and_bound`
    /// reports the number of outer search depths visited.
    pub iterations: usize,
    /// Number of states explored during search.
    pub explored_states: usize,
    /// Number of extreme points generated by EP-family solvers. Set to 0
    /// by every other algorithm.
    pub extreme_points_generated: usize,
    /// Number of branch-and-bound nodes expanded by the exact backend.
    /// Set to 0 by every other algorithm.
    pub branch_and_bound_nodes: usize,
    /// Free-form notes emitted by the solver for diagnostics.
    pub notes: Vec<String>,
}

/// A complete solution returned by [`solve_3d`](super::solve_3d).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThreeDSolution {
    /// Name of the algorithm that produced this solution. When the user
    /// passes `ThreeDAlgorithm::Auto`, this is the *leaf* algorithm name
    /// (e.g. `"extreme_points_residual_space"`), never the literal string
    /// `"auto"`. Mirrors the 1D / 2D convention.
    pub algorithm: String,
    /// Whether the solution is proven optimal. Set to `true` only by
    /// `branch_and_bound` when the search exhausts the tree below
    /// `branch_and_bound_node_limit`. Every other algorithm leaves this
    /// as `false`.
    pub exact: bool,
    /// Optional lower bound on the optimal objective (number of bins).
    /// Populated only by `branch_and_bound` from the L0/L1/L2 bounds it
    /// computes; `None` for every other algorithm in v1.
    pub lower_bound: Option<f64>,
    /// Whether the layouts are guillotine-compatible. Set to `true` by
    /// `guillotine_3d*` and by `layer_building_guillotine`; `false` by
    /// every other algorithm.
    pub guillotine: bool,
    /// Number of bins consumed.
    pub bin_count: usize,
    /// Total wasted volume across all bins.
    pub total_waste_volume: u64,
    /// Total material cost across all bins.
    pub total_cost: f64,
    /// Per-bin layouts in descending order of utilization.
    pub layouts: Vec<BinLayout3D>,
    /// Per-bin requirement summary. Populated by `solve_3d` (not by the
    /// individual algorithm) when at least one `Bin3D.quantity` cap is
    /// set; otherwise an empty `Vec`. See "Inventory-aware re-solve" in
    /// the spec for the relaxed-pass mechanic.
    #[serde(default)]
    pub bin_requirements: Vec<BinRequirement3D>,
    /// Boxes the solver was unable to place. Each entry is a
    /// `BoxDemand3D` with `quantity = 1` (one entry per unplaced
    /// instance), matching how 2D returns `unplaced: Vec<RectDemand2D>`.
    pub unplaced: Vec<BoxDemand3D>,
    /// Metrics captured while solving.
    pub metrics: SolverMetrics3D,
}

/// Input problem passed to [`solve_3d`](super::solve_3d).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThreeDProblem {
    /// Available bin types.
    pub bins: Vec<Bin3D>,
    /// Box demands to be placed in the bins.
    pub demands: Vec<BoxDemand3D>,
}

/// Options controlling how [`solve_3d`](super::solve_3d) runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreeDOptions {
    /// Algorithm to dispatch to.
    #[serde(default)]
    pub algorithm: ThreeDAlgorithm,
    /// Number of multistart restarts used by randomized strategies.
    #[serde(default = "default_multistart_runs")]
    pub multistart_runs: usize,
    /// Number of improvement rounds per local search start.
    #[serde(default = "default_improvement_rounds")]
    pub improvement_rounds: usize,
    /// Beam width for the guillotine beam search backend.
    #[serde(default = "default_beam_width")]
    pub beam_width: usize,
    /// Maximum number of demand types for the Auto mode to attempt the exact backend.
    #[serde(default = "default_auto_exact_max_types")]
    pub auto_exact_max_types: usize,
    /// Maximum total quantity for the Auto mode to attempt the exact backend.
    #[serde(default = "default_auto_exact_max_quantity")]
    pub auto_exact_max_quantity: usize,
    /// Maximum number of branch-and-bound nodes the exact backend may expand.
    #[serde(default = "default_branch_and_bound_node_limit")]
    pub branch_and_bound_node_limit: usize,
    /// Whether layouts must be guillotine-compatible.
    #[serde(default)]
    pub guillotine_required: bool,
    /// Optional seed for reproducible randomized algorithms.
    #[serde(default)]
    pub seed: Option<u64>,
}

impl Default for ThreeDOptions {
    fn default() -> Self {
        Self {
            algorithm: ThreeDAlgorithm::Auto,
            multistart_runs: default_multistart_runs(),
            improvement_rounds: default_improvement_rounds(),
            beam_width: default_beam_width(),
            auto_exact_max_types: default_auto_exact_max_types(),
            auto_exact_max_quantity: default_auto_exact_max_quantity(),
            branch_and_bound_node_limit: default_branch_and_bound_node_limit(),
            guillotine_required: false,
            seed: None,
        }
    }
}

impl ThreeDSolution {
    /// Lexicographic ranking comparator. Mirrors `OneDSolution::is_better_than`
    /// and `TwoDSolution::is_better_than`. The tuple is
    /// `(unplaced.len(), bin_count, total_waste_volume, OrderedFloat(total_cost))`.
    /// `guillotine_required` is **not** part of the comparator — that filter
    /// is enforced by `auto.rs::solve_auto_guillotine` narrowing its
    /// candidate set, exactly the same way 2D does it.
    // Consumed by `three_d::auto` and per-algorithm solvers (Task 6+); kept
    // live here via the tie-break regression test in this module.
    #[allow(dead_code)]
    pub(crate) fn is_better_than(&self, other: &Self) -> bool {
        (
            self.unplaced.len(),
            self.bin_count,
            self.total_waste_volume,
            OrderedFloat3D(self.total_cost),
        ) < (
            other.unplaced.len(),
            other.bin_count,
            other.total_waste_volume,
            OrderedFloat3D(other.total_cost),
        )
    }
}

// Used by `ThreeDSolution::is_better_than` (see above) which is exercised
// by the tie-break regression test; the struct itself is only constructed
// inside that comparator.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq)]
struct OrderedFloat3D(f64);
impl Eq for OrderedFloat3D {}
impl PartialOrd for OrderedFloat3D {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for OrderedFloat3D {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.total_cmp(&other.0)
    }
}

fn default_multistart_runs() -> usize {
    12
}
fn default_improvement_rounds() -> usize {
    24
}
fn default_beam_width() -> usize {
    8
}
fn default_auto_exact_max_types() -> usize {
    8
}
fn default_auto_exact_max_quantity() -> usize {
    32
}
fn default_branch_and_bound_node_limit() -> usize {
    1_000_000
}

/// A pre-instanced item ready for placement (one entry per `quantity`).
// Consumed by the Task 6+ EP / guillotine / layer-building solvers via
// `ThreeDProblem::expanded_items`.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ItemInstance3D {
    pub(crate) demand_index: usize,
    pub(crate) name: String,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) depth: u32,
    pub(crate) allowed_rotations: RotationMask3D,
}

impl ItemInstance3D {
    /// Yields `(rotation, x_extent, y_extent, z_extent)` for every rotation
    /// allowed by `allowed_rotations`, deduplicating rotations that produce
    /// identical extents (e.g. a cube collapses all six rotations into one).
    // Consumed by Task 6+ placement engines.
    #[allow(dead_code)]
    pub(crate) fn orientations(&self) -> impl Iterator<Item = (Rotation3D, u32, u32, u32)> + '_ {
        let mut seen: Vec<(u32, u32, u32)> = Vec::with_capacity(6);
        self.allowed_rotations.iter().filter_map(move |rotation| {
            let extents = rotation.apply(self.width, self.height, self.depth);
            if seen.contains(&extents) {
                None
            } else {
                seen.push(extents);
                Some((rotation, extents.0, extents.1, extents.2))
            }
        })
    }
}

impl ThreeDProblem {
    /// Boundary-validate the problem. Called by `solve_3d` before any algorithm runs.
    pub(crate) fn validate(&self) -> Result<()> {
        if self.bins.is_empty() {
            return Err(BinPackingError::InvalidInput(
                "at least one bin entry is required".to_string(),
            ));
        }
        if self.demands.is_empty() {
            return Err(BinPackingError::InvalidInput(
                "at least one box demand entry is required".to_string(),
            ));
        }

        for bin in &self.bins {
            if bin.width == 0 || bin.height == 0 || bin.depth == 0 {
                return Err(BinPackingError::InvalidInput(format!(
                    "bin `{}` must have positive width, height, and depth",
                    bin.name
                )));
            }
            if bin.width > MAX_DIMENSION_3D
                || bin.height > MAX_DIMENSION_3D
                || bin.depth > MAX_DIMENSION_3D
            {
                return Err(BinPackingError::InvalidInput(format!(
                    "bin `{}` dimensions exceed the supported maximum of {}",
                    bin.name, MAX_DIMENSION_3D
                )));
            }
            if !bin.cost.is_finite() || bin.cost < 0.0 {
                return Err(BinPackingError::InvalidInput(format!(
                    "bin `{}` must have a finite non-negative cost",
                    bin.name
                )));
            }
            if let Some(quantity) = bin.quantity
                && quantity == 0
            {
                return Err(BinPackingError::InvalidInput(format!(
                    "bin `{}` quantity, if set, must be positive",
                    bin.name
                )));
            }
        }

        for demand in &self.demands {
            if demand.width == 0 || demand.height == 0 || demand.depth == 0 {
                return Err(BinPackingError::InvalidInput(format!(
                    "demand `{}` must have positive width, height, and depth",
                    demand.name
                )));
            }
            if demand.width > MAX_DIMENSION_3D
                || demand.height > MAX_DIMENSION_3D
                || demand.depth > MAX_DIMENSION_3D
            {
                return Err(BinPackingError::InvalidInput(format!(
                    "demand `{}` dimensions exceed the supported maximum of {}",
                    demand.name, MAX_DIMENSION_3D
                )));
            }
            if demand.quantity == 0 {
                return Err(BinPackingError::InvalidInput(format!(
                    "demand `{}` must have positive quantity",
                    demand.name
                )));
            }
            if demand.allowed_rotations.is_empty() {
                return Err(BinPackingError::InvalidInput(format!(
                    "demand `{}` must allow at least one rotation",
                    demand.name
                )));
            }
        }

        Ok(())
    }

    /// Verify that every demand can fit *some* bin in *some* allowed rotation.
    pub(crate) fn ensure_feasible_demands(&self) -> Result<()> {
        for demand in &self.demands {
            let feasible = self.bins.iter().any(|bin| {
                demand.allowed_rotations.iter().any(|rotation| {
                    let (x, y, z) = rotation.apply(demand.width, demand.height, demand.depth);
                    bin.width >= x && bin.height >= y && bin.depth >= z
                })
            });
            if !feasible {
                return Err(BinPackingError::Infeasible3D {
                    item: demand.name.clone(),
                    width: demand.width,
                    height: demand.height,
                    depth: demand.depth,
                });
            }
        }
        Ok(())
    }

    /// Expand each demand into one [`ItemInstance3D`] per `quantity`.
    // Consumed by Task 6+ placement engines.
    #[allow(dead_code)]
    pub(crate) fn expanded_items(&self) -> Vec<ItemInstance3D> {
        let mut items = Vec::new();
        for (index, demand) in self.demands.iter().enumerate() {
            for _ in 0..demand.quantity {
                items.push(ItemInstance3D {
                    demand_index: index,
                    name: demand.name.clone(),
                    width: demand.width,
                    height: demand.height,
                    depth: demand.depth,
                    allowed_rotations: demand.allowed_rotations,
                });
            }
        }
        items
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn rotation3d_serializes_as_snake_case() {
        let value = serde_json::to_value(Rotation3D::Zxy).expect("serialize");
        assert_eq!(value, json!("zxy"));
    }

    #[test]
    fn algorithm_serializes_with_explicit_renames_for_digit_variants() {
        // Catches the regression where serde's snake_case converter would
        // produce `guillotine3_d` instead of the spec's `guillotine_3d`.
        let cases = [
            (ThreeDAlgorithm::Auto, "auto"),
            (ThreeDAlgorithm::ExtremePoints, "extreme_points"),
            (ThreeDAlgorithm::Guillotine3D, "guillotine_3d"),
            (ThreeDAlgorithm::Guillotine3DBestShortSideFit, "guillotine_3d_best_short_side_fit"),
            (ThreeDAlgorithm::LayerBuildingMaxRects, "layer_building_max_rects"),
            (ThreeDAlgorithm::DeepestBottomLeftFill, "deepest_bottom_left_fill"),
            (ThreeDAlgorithm::BranchAndBound, "branch_and_bound"),
        ];
        for (variant, expected) in cases {
            let value = serde_json::to_value(variant).expect("serialize");
            assert_eq!(value, json!(expected), "{:?}", variant);
            let parsed: ThreeDAlgorithm =
                serde_json::from_value(json!(expected)).expect("deserialize");
            assert_eq!(parsed, variant);
        }
    }

    #[test]
    fn rotation_mask_all_contains_every_rotation() {
        let mask = RotationMask3D::ALL;
        for rot in [
            Rotation3D::Xyz,
            Rotation3D::Xzy,
            Rotation3D::Yxz,
            Rotation3D::Yzx,
            Rotation3D::Zxy,
            Rotation3D::Zyx,
        ] {
            assert!(mask.contains(rot), "ALL should contain {:?}", rot);
        }
    }

    #[test]
    fn rotation_mask_upright_only_keeps_y_axis() {
        let upright = RotationMask3D::UPRIGHT;
        assert!(upright.contains(Rotation3D::Xyz));
        assert!(upright.contains(Rotation3D::Zyx));
        assert!(!upright.contains(Rotation3D::Xzy));
        assert!(!upright.contains(Rotation3D::Yxz));
        assert!(!upright.contains(Rotation3D::Yzx));
        assert!(!upright.contains(Rotation3D::Zxy));
    }

    #[test]
    fn rotation_mask_none_is_empty() {
        let none = RotationMask3D::NONE;
        for rot in [
            Rotation3D::Xyz,
            Rotation3D::Xzy,
            Rotation3D::Yxz,
            Rotation3D::Yzx,
            Rotation3D::Zxy,
            Rotation3D::Zyx,
        ] {
            assert!(!none.contains(rot));
        }
    }

    #[test]
    fn rotation3d_apply_zxy_maps_input_dims_correctly() {
        // Per spec: Rotation3D::Zxy applied to (w=3, h=5, d=7) yields
        // (x_extent=7, y_extent=3, z_extent=5).
        let (x, y, z) = Rotation3D::Zxy.apply(3, 5, 7);
        assert_eq!((x, y, z), (7, 3, 5));
    }

    #[test]
    fn rotation3d_apply_xyz_is_identity() {
        let (x, y, z) = Rotation3D::Xyz.apply(3, 5, 7);
        assert_eq!((x, y, z), (3, 5, 7));
    }

    #[test]
    fn rotation3d_apply_zyx_swaps_x_and_z() {
        let (x, y, z) = Rotation3D::Zyx.apply(3, 5, 7);
        assert_eq!((x, y, z), (7, 5, 3));
    }

    #[test]
    fn validate_rejects_empty_bins() {
        let problem = ThreeDProblem { bins: vec![], demands: vec![sample_demand("a", 1, 1, 1, 1)] };
        let err = problem.validate().expect_err("should reject");
        assert!(
            matches!(&err, crate::BinPackingError::InvalidInput(msg) if msg.contains("bin")),
            "unexpected error: {err:?}",
        );
    }

    #[test]
    fn validate_rejects_empty_demands() {
        let problem = ThreeDProblem { bins: vec![sample_bin("b", 10, 10, 10)], demands: vec![] };
        let err = problem.validate().expect_err("should reject");
        assert!(matches!(err, crate::BinPackingError::InvalidInput(_)));
    }

    #[test]
    fn validate_rejects_zero_dimension_bin() {
        let problem = ThreeDProblem {
            bins: vec![sample_bin("b", 0, 10, 10)],
            demands: vec![sample_demand("a", 1, 1, 1, 1)],
        };
        assert!(matches!(problem.validate(), Err(crate::BinPackingError::InvalidInput(_))));
    }

    #[test]
    fn validate_rejects_oversized_dimension() {
        let oversize = MAX_DIMENSION_3D + 1;
        let problem = ThreeDProblem {
            bins: vec![sample_bin("b", oversize, 10, 10)],
            demands: vec![sample_demand("a", 1, 1, 1, 1)],
        };
        assert!(matches!(problem.validate(), Err(crate::BinPackingError::InvalidInput(_))));
    }

    #[test]
    fn validate_rejects_non_finite_cost() {
        let mut bin = sample_bin("b", 10, 10, 10);
        bin.cost = f64::NAN;
        let problem =
            ThreeDProblem { bins: vec![bin], demands: vec![sample_demand("a", 1, 1, 1, 1)] };
        assert!(matches!(problem.validate(), Err(crate::BinPackingError::InvalidInput(_))));
    }

    #[test]
    fn validate_rejects_negative_cost() {
        let mut bin = sample_bin("b", 10, 10, 10);
        bin.cost = -1.0;
        let problem =
            ThreeDProblem { bins: vec![bin], demands: vec![sample_demand("a", 1, 1, 1, 1)] };
        assert!(matches!(problem.validate(), Err(crate::BinPackingError::InvalidInput(_))));
    }

    #[test]
    fn validate_rejects_zero_quantity_demand() {
        let problem = ThreeDProblem {
            bins: vec![sample_bin("b", 10, 10, 10)],
            demands: vec![sample_demand("a", 1, 1, 1, 0)],
        };
        assert!(matches!(problem.validate(), Err(crate::BinPackingError::InvalidInput(_))));
    }

    #[test]
    fn validate_rejects_empty_rotation_mask() {
        let mut demand = sample_demand("a", 1, 1, 1, 1);
        demand.allowed_rotations = RotationMask3D::NONE;
        let problem =
            ThreeDProblem { bins: vec![sample_bin("b", 10, 10, 10)], demands: vec![demand] };
        assert!(matches!(problem.validate(), Err(crate::BinPackingError::InvalidInput(_))));
    }

    #[test]
    fn validate_accepts_well_formed_problem() {
        let problem = ThreeDProblem {
            bins: vec![sample_bin("b", 10, 10, 10)],
            demands: vec![sample_demand("a", 2, 3, 4, 1)],
        };
        problem.validate().expect("should accept");
    }

    #[test]
    fn ensure_feasible_demands_rejects_oversize_item() {
        let problem = ThreeDProblem {
            bins: vec![sample_bin("b", 5, 5, 5)],
            demands: vec![sample_demand("a", 6, 6, 6, 1)],
        };
        problem.validate().expect("validate ok");
        let err = problem.ensure_feasible_demands().expect_err("should reject");
        assert!(
            matches!(&err, crate::BinPackingError::Infeasible3D { item, .. } if item == "a"),
            "unexpected error: {err:?}",
        );
    }

    #[test]
    fn ensure_feasible_demands_uses_rotation() {
        // 6x1x1 in a 1x6x1 bin: only the (Yxz) rotation works.
        let problem = ThreeDProblem {
            bins: vec![sample_bin("b", 1, 6, 1)],
            demands: vec![sample_demand("a", 6, 1, 1, 1)],
        };
        problem.validate().expect("validate ok");
        problem.ensure_feasible_demands().expect("should accept via rotation");
    }

    #[test]
    fn expanded_items_yields_one_per_quantity() {
        let problem = ThreeDProblem {
            bins: vec![sample_bin("b", 10, 10, 10)],
            demands: vec![sample_demand("a", 2, 3, 4, 5), sample_demand("c", 1, 1, 1, 2)],
        };
        let items = problem.expanded_items();
        assert_eq!(items.len(), 7);
        assert_eq!(items.iter().filter(|item| item.name == "a").count(), 5);
        assert_eq!(items.iter().filter(|item| item.name == "c").count(), 2);
    }

    /// `ThreeDSolution::is_better_than` ranks on the 4-tuple
    /// `(unplaced.len(), bin_count, total_waste_volume, total_cost)`.
    /// Verify each key dominates in turn — mirrors the 1D / 2D
    /// `is_better_than_tie_breaks_on_each_key` regression tests.
    #[test]
    fn three_d_is_better_than_tie_breaks_on_each_key() {
        let base = sample_solution(0, 1, 0, 1.0);

        // Fewer unplaced wins (primary key).
        let more_unplaced =
            ThreeDSolution { unplaced: vec![sample_demand("u", 1, 1, 1, 1)], ..base.clone() };
        assert!(base.is_better_than(&more_unplaced));
        assert!(!more_unplaced.is_better_than(&base));

        // Fewer bins wins when unplaced ties.
        let more_bins = ThreeDSolution { bin_count: 2, ..base.clone() };
        assert!(base.is_better_than(&more_bins));

        // Less waste wins when unplaced and bin_count tie.
        let more_waste = ThreeDSolution { total_waste_volume: 100, ..base.clone() };
        assert!(base.is_better_than(&more_waste));

        // Lower cost wins when every preceding key ties.
        let more_cost = ThreeDSolution { total_cost: base.total_cost + 1.0, ..base.clone() };
        assert!(base.is_better_than(&more_cost));

        // Identical solutions are not "better than" each other.
        assert!(!base.is_better_than(&base));
    }

    fn sample_solution(
        unplaced_count: usize,
        bin_count: usize,
        waste: u64,
        cost: f64,
    ) -> ThreeDSolution {
        ThreeDSolution {
            algorithm: "test".to_string(),
            exact: false,
            lower_bound: None,
            guillotine: false,
            bin_count,
            total_waste_volume: waste,
            total_cost: cost,
            layouts: Vec::new(),
            bin_requirements: Vec::new(),
            unplaced: (0..unplaced_count)
                .map(|i| sample_demand(&format!("u{i}"), 1, 1, 1, 1))
                .collect(),
            metrics: SolverMetrics3D::default(),
        }
    }

    fn sample_bin(name: &str, w: u32, h: u32, d: u32) -> Bin3D {
        Bin3D { name: name.to_string(), width: w, height: h, depth: d, cost: 1.0, quantity: None }
    }

    fn sample_demand(name: &str, w: u32, h: u32, d: u32, qty: usize) -> BoxDemand3D {
        BoxDemand3D {
            name: name.to_string(),
            width: w,
            height: h,
            depth: d,
            quantity: qty,
            allowed_rotations: RotationMask3D::ALL,
        }
    }
}
