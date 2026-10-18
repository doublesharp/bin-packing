use serde::{Deserialize, Serialize};

use crate::two_d::{Placement2D, SheetLayout2D, TwoDProblem};

/// Explicit provenance for the layouts to cut. Labels are never identity keys.
/// Partial solutions are allowed; only supplied placements are scheduled.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SawJob2D {
    /// Original dimensions, quantities, blade kerfs and rotation constraints.
    pub problem: TwoDProblem,
    /// Layouts in their original solution order. Aggregate packing metrics are ignored.
    pub layouts: Vec<SawLayout2D>,
    /// Source stock index for each layout, supplied by the caller without name inference.
    pub stock_indices: Vec<usize>,
    /// Source demand index for every placement, grouped by layout.
    pub demand_indices: Vec<Vec<usize>>,
}

/// Geometry-only layout input. Unknown JSON fields such as packing metrics are ignored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SawLayout2D {
    /// Physical source stock width.
    pub width: u32,
    /// Physical source stock height.
    pub height: u32,
    /// Required finished placements, preserving their rotation metadata.
    pub placements: Vec<Placement2D>,
}

impl From<&SheetLayout2D> for SawLayout2D {
    fn from(layout: &SheetLayout2D) -> Self {
        Self { width: layout.width, height: layout.height, placements: layout.placements.clone() }
    }
}

/// Axis of a full-span cut in original sheet coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SawAxis2D {
    /// Blade travels parallel to the sheet Y axis.
    Vertical,
    /// Blade travels parallel to the sheet X axis.
    Horizontal,
}

/// Original-sheet edge of the current workpiece placed against the guide.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReferenceEdge2D {
    /// Low X edge; canonical orientation, zero quarter turns.
    Left,
    /// Low Y edge; three clockwise quarter turns.
    Top,
    /// High X edge; two clockwise quarter turns.
    Right,
    /// High Y edge; one clockwise quarter turn.
    Bottom,
}

/// Supported machinery. No stacked passes or CNC paths are modeled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SawMachine2D {
    /// Rip fence plus independent crosscut sled stop. Both are required.
    #[default]
    TableSaw,
    /// Panel saw with a reconfigurable stop and movable workpieces.
    PanelSaw,
}

/// Machine accessory used for a physical setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SawSetupKind2D {
    /// Table-saw rip fence for original-sheet vertical cuts.
    RipFence,
    /// Independent crosscut sled stop for original-sheet horizontal cuts.
    CrosscutStop,
    /// Panel-saw stop, shared between cut axes after workpiece rotation.
    PanelStop,
}

/// Distance from reference edge to the nearest blade edge, in input units.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SawSetup2D {
    /// Active accessory.
    pub kind: SawSetupKind2D,
    /// Material width retained on the reference side, excluding blade kerf.
    pub distance: u32,
}

/// Primary objective. Ties use effort, then cuts, setups, rotations, handling,
/// then deterministic candidate enumeration order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SawObjective2D {
    /// Minimize initial setups plus subsequent setting/accessory changes.
    FewestSetupChanges,
    /// Minimize physical blade passes.
    FewestCuts,
    /// Minimize the weighted sum of explicit operations.
    #[default]
    WeightedEffort,
}

/// Nonnegative finite effort units per operation, independent of material price.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SawWeights2D {
    /// One physical saw pass.
    pub cut: f64,
    /// First use of each accessory.
    pub initial_setup: f64,
    /// Change of distance or reactivation of a previously used accessory.
    pub setup_change: f64,
    /// One explicit orientation change, including a half turn.
    pub rotation: f64,
    /// Select and load one workpiece, including the initial sheet.
    pub handling: f64,
}
impl Default for SawWeights2D {
    fn default() -> Self {
        Self { cut: 1.0, initial_setup: 5.0, setup_change: 5.0, rotation: 2.0, handling: 1.0 }
    }
}

/// Deterministic search controls. Zero optimization states returns the baseline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SawSearchLimits2D {
    /// Maximum generated optimization successors, at most 100,000.
    pub max_states: usize,
    /// Maximum retained states at a cut depth, 1 through 256.
    pub beam_width: usize,
    /// Maximum attempted baseline geometric splits, 1 through 100,000.
    /// Includes candidates rejected for intersecting a required part.
    pub baseline_max_states: usize,
}
impl Default for SawSearchLimits2D {
    fn default() -> Self {
        Self { max_states: 4000, beam_width: 32, baseline_max_states: 20000 }
    }
}

/// Opt-in saw planning options. Defaults use table-saw weighted effort.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct SawPlanOptions2D {
    /// Supported machine and accessory model.
    pub machine: SawMachine2D,
    /// Schedule objective.
    pub objective: SawObjective2D,
    /// Effective effort per operation, including initial setups.
    pub weights: SawWeights2D,
    /// Interleave ready workpieces across sheets. Otherwise complete sheets in input order.
    pub batch_across_sheets: bool,
    /// Bounded search effort.
    pub search: SawSearchLimits2D,
    /// Minimum width and height for a terminal waste piece to be a usable offcut.
    pub min_usable_side: u32,
}

/// Rectangle in original sheet coordinates. Finished parts never move in this frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SawRect2D {
    /// Minimum X coordinate.
    pub x: u32,
    /// Minimum Y coordinate.
    pub y: u32,
    /// Extent in X.
    pub width: u32,
    /// Extent in Y.
    pub height: u32,
}

/// Stable identity derived from sheet instance and physical bounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct WorkpieceId2D {
    /// Index in the request's layouts.
    pub sheet_index: usize,
    /// Bounds uniquely identifying this workpiece within the sheet.
    pub bounds: SawRect2D,
}

/// Stable finished-part instance identity, independent of labels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PartId2D {
    /// Index in the request's layouts.
    pub sheet_index: usize,
    /// Index in that layout's placements.
    pub placement_index: usize,
}

/// Workpiece ancestry and terminal classification. Roots have no parent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SawWorkpiece2D {
    /// Workpiece identity and original bounds.
    pub id: WorkpieceId2D,
    /// Workpiece consumed to create this one.
    pub parent: Option<WorkpieceId2D>,
    /// Creating cut operation, or null for stock roots.
    pub created_by: Option<usize>,
    /// Placement identity when exactly finished, otherwise null.
    pub finished_part: Option<PartId2D>,
    /// True only for terminal waste satisfying the minimum usable side threshold.
    pub usable_offcut: bool,
}

/// Diagram line along the low-coordinate edge of the blade band.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SawSegment2D {
    /// Starting X.
    pub from_x: u32,
    /// Starting Y.
    pub from_y: u32,
    /// Ending X.
    pub to_x: u32,
    /// Ending Y.
    pub to_y: u32,
}

/// Physical instruction. Execute in the authoritative job order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SawAction2D {
    /// Select and load this workpiece in its inherited orientation.
    Handle,
    /// Reorient the loaded workpiece. Does not modify placement/grain metadata.
    Rotate {
        /// Absolute clockwise quarter turns from original sheet orientation, 0 through 3.
        quarter_turns: u8,
    },
    /// Configure the indicated accessory. Never crosscut against a rip fence.
    Setup {
        /// Required accessory and retained material distance.
        setup: SawSetup2D,
        /// True for the first use of this accessory in the job.
        initial: bool,
    },
    /// Consume the entire parent and produce its listed children in one pass.
    Cut {
        /// Cut axis in original sheet space.
        axis: SawAxis2D,
        /// Edge of this workpiece against the guide.
        reference_edge: ReferenceEdge2D,
        /// Distance from the reference edge to the nearest blade edge.
        setting: u32,
        /// Full blade width from source stock, even for a clipped edge-relief pass.
        kerf: u32,
        /// Actual material removed; blade outside stock is excluded. Zero width/height at zero kerf.
        kerf_region: SawRect2D,
        /// Diagram-ready original-sheet blade edge segment.
        segment: SawSegment2D,
        /// Nonempty resulting workpieces in low-to-high coordinate order.
        children: Vec<WorkpieceId2D>,
    },
}

/// One authoritative operation. IDs are stable for identical requests/options.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SawOperation2D {
    /// Zero-based operation index in the job schedule.
    pub id: usize,
    /// Workpiece being handled or cut, including its sheet instance.
    pub workpiece: WorkpieceId2D,
    /// Creating cut for this workpiece. Machine state additionally requires job order.
    pub depends_on: Option<usize>,
    /// Executable instruction.
    pub action: SawAction2D,
}

/// Counts obtained from the returned physical instructions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SawCounts2D {
    /// Actual blade passes.
    pub cuts: usize,
    /// First setup for each used accessory.
    pub initial_setups: usize,
    /// Subsequent accessory activations or distance changes.
    pub setup_changes: usize,
    /// Explicit reorientation operations.
    pub rotations: usize,
    /// Workpiece selections/loads.
    pub handling: usize,
}

/// Scope of the optimizer's guarantee.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SawOptimality2D {
    /// A bounded search result, no optimality claim.
    Heuristic,
    /// Exhausted the finite boundary-cut/reference-edge model with no pruning.
    OptimalWithinCandidateModel,
}

/// Search accounting excludes validation and includes separate baseline work.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SawSearchReport2D {
    /// Effective limits.
    pub limits: SawSearchLimits2D,
    /// Attempted baseline geometric splits, including rejected candidates.
    /// Reference-edge scoring and boundary enumeration are not counted.
    pub baseline_states: usize,
    /// Generated optimization successors.
    pub explored_states: usize,
    /// Whether the successor budget stopped exploration.
    pub budget_exhausted: bool,
    /// Whether the beam discarded states.
    pub beam_pruned: bool,
    /// Equivalent future states rejected by componentwise operation-count dominance.
    /// Zero when cross-sheet batching bypasses dominance pruning.
    #[serde(default)]
    pub dominated_states: usize,
    /// Peak per-depth dominance storage in scalar units, at most 65,536.
    /// Excludes the beam, trace prefixes, allocator overhead and shared split cache.
    /// Zero when cross-sheet batching bypasses dominance pruning.
    #[serde(default)]
    pub peak_dominance_units: usize,
    /// Some moves bypassed the full dominance table without being discarded.
    /// False when cross-sheet batching bypasses dominance pruning.
    #[serde(default)]
    pub dominance_table_saturated: bool,
    /// Honest optimality scope.
    pub optimality: SawOptimality2D,
    /// Valid incumbent effort before optimization, using the same weights.
    pub baseline_effort_score: f64,
    /// Baseline counts for objective comparisons.
    pub baseline_counts: SawCounts2D,
}

/// Per-sheet diagram view. These indices are not an independently executable plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SawSheetView2D {
    /// Index in the original layouts.
    pub sheet_index_in_solution: usize,
    /// Explicit source stock index.
    pub stock_index: usize,
    /// References into the authoritative schedule, in job order.
    pub operation_ids: Vec<usize>,
}

/// Versioned physical cutting schedule. Material metrics remain on the packing result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SawPlan2D {
    /// Output schema version, currently 1.
    pub schema_version: u32,
    /// Effective machine model.
    pub machine: SawMachine2D,
    /// Primary search objective.
    pub objective: SawObjective2D,
    /// Whether cross-sheet interleaving was allowed.
    pub batch_across_sheets: bool,
    /// One ordered, authoritative instruction list.
    pub operations: Vec<SawOperation2D>,
    /// All root, intermediate, finished and waste workpieces.
    pub workpieces: Vec<SawWorkpiece2D>,
    /// Diagram views referencing the job, never independent execution instructions.
    pub sheet_views: Vec<SawSheetView2D>,
    /// Physical operation totals.
    pub counts: SawCounts2D,
    /// Weights actually used in search and scoring.
    pub effective_weights: SawWeights2D,
    /// Weighted effort, not elapsed time or material cost.
    pub effort_score: f64,
    /// Limits, baseline and optimization metrics.
    pub search: SawSearchReport2D,
}

/// Structured failures of the opt-in physical planner.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum SawPlanError2D {
    /// Invalid original stock/demand boundary input.
    #[error("invalid saw problem: {0}")]
    InvalidProblem(String),
    /// Invalid search or cost control.
    #[error("invalid saw option: {field}")]
    InvalidOption {
        /// Rejected option name.
        field: &'static str,
    },
    /// Provenance does not match the supplied instances.
    #[error("invalid saw mapping on sheet {sheet_index}: {reason}")]
    InvalidMapping {
        /// Layout index or layouts.len() for a top-level mismatch.
        sheet_index: usize,
        /// Specific mapping failure.
        reason: &'static str,
    },
    /// Physically malformed layout, including finished material outside stock.
    #[error("invalid saw geometry on sheet {sheet_index}: {reason}")]
    InvalidGeometry {
        /// Layout index.
        sheet_index: usize,
        /// Specific geometry failure.
        reason: &'static str,
    },
    /// No nonintrusive full-span cut exists for an unfinished region.
    #[error("sheet {sheet_index} has no physical guillotine decomposition")]
    NonGuillotine {
        /// Layout index.
        sheet_index: usize,
    },
    /// Cannot obtain a baseline within the requested construction budget.
    #[error("baseline construction exceeded its {limit} state limit")]
    BaselineLimit {
        /// Effective baseline evaluation limit.
        limit: usize,
    },
}
