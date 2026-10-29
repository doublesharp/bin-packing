//! Public wire model for rigid irregular shapes and their nesting results.

use serde::{Deserialize, Serialize};

/// A finite planar coordinate, in the same units as all other geometry.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Point2D {
    /// Horizontal coordinate.
    pub x: f64,
    /// Vertical coordinate.
    pub y: f64,
}

/// A segment following the previous endpoint in a closed curved contour.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CurveSegment2D {
    /// A straight segment ending at `end`.
    Line {
        /// Final endpoint.
        end: Point2D,
    },
    /// A circular arc; its start and end must have the same radius about `center`.
    Arc {
        /// Final endpoint; an endpoint equal to the start denotes a full circle.
        end: Point2D,
        /// Center of the circle.
        center: Point2D,
        /// Traverse clockwise when true, counterclockwise otherwise.
        clockwise: bool,
    },
}

/// A polygonal, circular, or piecewise circular closed boundary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Contour2D {
    /// Straight polygon vertices; the final closing vertex is optional.
    Polygon {
        /// At least three distinct ordered vertices.
        vertices: Vec<Point2D>,
    },
    /// An exact source circle, conservatively enveloped during packing.
    Circle {
        /// Circle center in the shape's original local coordinate frame.
        center: Point2D,
        /// Strictly positive radius.
        radius: f64,
    },
    /// A closed sequence of straight segments and circular arcs.
    Curve {
        /// First endpoint; the last segment must return here.
        start: Point2D,
        /// Ordered segments.
        segments: Vec<CurveSegment2D>,
    },
}

/// One connected material polygon, including any holes cut out of it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Polygon2D {
    /// Outer contour.
    pub exterior: Contour2D,
    /// Interior holes; each must be strictly inside the exterior.
    #[serde(default)]
    pub holes: Vec<Contour2D>,
}

/// A rigid shape comprising one or more non-overlapping material polygons.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Shape2D {
    /// Components share one local coordinate frame and are transformed together.
    pub components: Vec<Polygon2D>,
}

/// An inclusive interval of permitted counterclockwise rotation angles.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RotationRange2D {
    /// First angle in degrees, between zero and 360.
    pub start: f64,
    /// Last angle, at least `start` and at most 360. Equal endpoints are discrete.
    pub end: f64,
}

fn default_rotations() -> Vec<RotationRange2D> {
    vec![RotationRange2D { start: 0.0, end: 0.0 }]
}

fn default_unit_value() -> f64 {
    1.0
}

/// One available bin type with its original material outline and defects.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IrregularBin {
    /// Display label; source indices, rather than names, identify stocks.
    pub name: String,
    /// Available material, including source holes.
    pub shape: Shape2D,
    /// Per-copy material cost.
    #[serde(default = "default_unit_value")]
    pub cost: f64,
    /// Available copies, or unlimited when absent.
    #[serde(default)]
    pub quantity: Option<usize>,
    /// Unusable shapes in the same original coordinate frame as the bin.
    #[serde(default)]
    pub defects: Vec<Shape2D>,
    /// Additional minimum distance to this bin's material boundary.
    #[serde(default)]
    pub spacing: f64,
}

/// One rigid source shape and its required copies and legal transformations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IrregularDemand {
    /// Display label; duplicate labels do not merge demand rows.
    pub name: String,
    /// Original local material geometry.
    pub shape: Shape2D,
    /// Required number of copies.
    pub quantity: usize,
    /// Inclusive legal angle intervals. Missing values permit only zero degrees.
    #[serde(default = "default_rotations")]
    pub allowed_rotations: Vec<RotationRange2D>,
    /// Permit reflection of the original local X coordinate before rotation.
    #[serde(default)]
    pub can_mirror: bool,
    /// Value of each placed copy for the knapsack objective.
    #[serde(default = "default_unit_value")]
    pub profit: f64,
}

/// A native irregular nesting problem.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IrregularProblem {
    /// Available bin types.
    pub bins: Vec<IrregularBin>,
    /// Rigid source shapes to nest.
    pub demands: Vec<IrregularDemand>,
}

/// Objective used during irregular construction and bounded improvement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum IrregularObjective {
    /// Minimize unplaced copies, then bin count, material cost, and empty area.
    #[default]
    BinCount,
    /// Minimize unplaced copies, then material cost and bin count.
    TotalCost,
    /// Maximize placed profit, then minimize material cost and bin count.
    Knapsack,
    /// Fulfill demands within the declared inventory; no infeasibility proof is implied.
    Feasibility,
    /// Minimize the rightmost material extent in a single declared maximum bin.
    OpenDimensionX,
    /// Minimize the topmost material extent in a single declared maximum bin.
    OpenDimensionY,
    /// Minimize the enclosing extent area in a single declared maximum bin.
    #[serde(rename = "open_dimension_xy")]
    OpenDimensionXY,
}

/// Validated geometry approximation, transformation and effort controls.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct IrregularOptions {
    /// Requested material or profit objective.
    pub objective: IrregularObjective,
    /// Maximum charged geometry probes in bounded improvement, excluding baseline.
    pub max_states: usize,
    /// Maximum retained complete or partial construction candidates.
    pub beam_width: usize,
    /// Maximum anchor translations considered per demand/orientation/bin.
    pub max_candidates_per_item: usize,
    /// Largest angular sample gap for non-discrete permitted intervals.
    pub rotation_step_degrees: f64,
    /// Maximum orientations per demand, including reflected variants.
    pub max_orientations: usize,
    /// Maximum chord error for approximating source circular boundaries.
    pub curve_tolerance: f64,
    /// Minimum distance between material of different placed items.
    pub item_spacing: f64,
    /// Minimum distance to a bin's outer boundary and holes.
    pub bin_spacing: f64,
    /// Minimum distance between placed material and defects.
    pub defect_spacing: f64,
}

impl Default for IrregularOptions {
    fn default() -> Self {
        Self {
            objective: IrregularObjective::BinCount,
            max_states: 4_096,
            beam_width: 8,
            max_candidates_per_item: 128,
            rotation_step_degrees: 15.0,
            max_orientations: 64,
            curve_tolerance: 0.01,
            item_spacing: 0.0,
            bin_spacing: 0.0,
            defect_spacing: 0.0,
        }
    }
}

/// A legal rigid transform of one original demand copy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IrregularPlacement {
    /// Original demand-row index.
    pub demand_index: usize,
    /// Translation after optional reflection and counterclockwise rotation.
    pub x: f64,
    /// Vertical translation in the original bin coordinate frame.
    pub y: f64,
    /// Legal counterclockwise source rotation in degrees.
    pub rotation_degrees: f64,
    /// Whether source local X was reflected before rotation.
    pub mirrored: bool,
}

/// All nested rigid objects on one source bin copy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IrregularLayout {
    /// Original bin-type index.
    pub bin_index: usize,
    /// Legal source transforms, in placement order.
    pub placements: Vec<IrregularPlacement>,
    /// Sum of original material areas, including analytic circular-arc integration.
    pub used_area: f64,
    /// Width from the bin's left bound to the rightmost placed conservative material.
    pub used_width: f64,
    /// Height from the bin's bottom bound to the topmost placed conservative material.
    pub used_height: f64,
}

/// An irregular nesting result and the controls needed for independent readback.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IrregularSolution {
    /// Algorithm identifying native contact-anchor polygon nesting.
    pub algorithm: String,
    /// Options used for approximation, transformations, spacing and ranking.
    pub options: IrregularOptions,
    /// Source-indexed layouts.
    pub layouts: Vec<IrregularLayout>,
    /// Remaining copies for each original demand row.
    pub unplaced_quantities: Vec<usize>,
    /// Total consumed bin material cost.
    pub total_cost: f64,
    /// Total profit of placed source copies.
    pub total_profit: f64,
    /// Whether all demands were placed.
    pub feasible: bool,
    /// Number of charged geometry probes after baseline construction.
    pub explored_states: usize,
    /// Number of geometric probes in greedy baseline construction.
    pub construction_probes: usize,
    /// Whether effort, beam, anchor or angular limits omitted alternatives.
    pub search_truncated: bool,
}
