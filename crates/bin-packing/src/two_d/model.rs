//! Data model types for 2D rectangular bin packing problems and solutions.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::{BinPackingError, Result};

/// Algorithm selector for [`solve_2d`](super::solve_2d).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TwoDAlgorithm {
    /// Try multiple strategies and return the best.
    #[default]
    Auto,
    /// Classic MaxRects best-area-fit construction.
    MaxRects,
    /// MaxRects with best-short-side-fit placement scoring.
    MaxRectsBestShortSideFit,
    /// MaxRects with best-long-side-fit placement scoring.
    MaxRectsBestLongSideFit,
    /// MaxRects with bottom-left placement scoring.
    MaxRectsBottomLeft,
    /// MaxRects with contact-point placement scoring.
    MaxRectsContactPoint,
    /// Skyline-based construction.
    Skyline,
    /// Skyline construction ranked by minimum waste.
    SkylineMinWaste,
    /// Guillotine beam search.
    Guillotine,
    /// Guillotine beam search with best-short-side-fit candidate ranking.
    GuillotineBestShortSideFit,
    /// Guillotine beam search with best-long-side-fit candidate ranking.
    GuillotineBestLongSideFit,
    /// Guillotine beam search with shorter-leftover-axis split selection.
    GuillotineShorterLeftoverAxis,
    /// Guillotine beam search with longer-leftover-axis split selection.
    GuillotineLongerLeftoverAxis,
    /// Guillotine beam search with minimum-area split selection.
    GuillotineMinAreaSplit,
    /// Guillotine beam search with maximum-area split selection.
    GuillotineMaxAreaSplit,
    /// Next-fit decreasing height shelf heuristic.
    NextFitDecreasingHeight,
    /// First-fit decreasing height shelf heuristic.
    FirstFitDecreasingHeight,
    /// Best-fit decreasing height shelf heuristic.
    BestFitDecreasingHeight,
    /// Multistart MaxRects meta-strategy.
    MultiStart,
    /// Exhaustive rotation search: enumerates all 2^k rotation assignments
    /// for k rotatable demand types (or samples `multistart_runs` random
    /// assignments when k exceeds `auto_rotation_search_max_types`). Uses
    /// MaxRects best-area-fit as the inner packer.
    RotationSearch,
}

/// A sheet stock entry that demands can be placed on.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sheet2D {
    /// Human-readable identifier for this sheet type.
    pub name: String,
    /// Sheet width.
    pub width: u32,
    /// Sheet height.
    pub height: u32,
    /// Per-unit cost of consuming a sheet of this type.
    #[serde(default = "default_sheet_cost")]
    pub cost: f64,
    /// Optional cap on the number of sheets of this type that may be used.
    #[serde(default)]
    pub quantity: Option<usize>,
    /// Material removed by the cutting tool on each cut of this sheet
    /// (e.g., table-saw blade thickness, CNC router bit diameter). Kerf is
    /// enforced as a minimum gap between edge-adjacent placements on the
    /// same sheet and is applied symmetrically across all 2D algorithms.
    /// Defaults to `0`, preserving pre-kerf-aware solver behavior.
    #[serde(default)]
    pub kerf: u32,
    /// When `true`, the trailing placement on this sheet may extend up to
    /// one `kerf` past the sheet's right and bottom boundaries. This models
    /// a cut that runs off the stock — the blade exits the material with
    /// only part of the kerf consuming material, and the rest is air. Does
    /// not relax individual part feasibility: every part must still satisfy
    /// `width <= sheet.width && height <= sheet.height`. Defaults to `false`
    /// (pre-edge-relief behavior).
    #[serde(default)]
    pub edge_kerf_relief: bool,
}

/// Returns `(effective_width, effective_height)` for `sheet`, accounting
/// for edge kerf relief. When `sheet.edge_kerf_relief` is `true`, both
/// dimensions are padded by `sheet.kerf` (saturating), allowing a trailing
/// placement to extend up to one kerf past the sheet boundary. When `false`,
/// returns the sheet's declared dimensions unchanged.
pub(crate) fn effective_bounds(sheet: &Sheet2D) -> (u32, u32) {
    if sheet.edge_kerf_relief {
        (sheet.width.saturating_add(sheet.kerf), sheet.height.saturating_add(sheet.kerf))
    } else {
        (sheet.width, sheet.height)
    }
}

/// A demand for a set of identical rectangular pieces.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RectDemand2D {
    /// Human-readable identifier for the demand.
    pub name: String,
    /// Required width of each rectangle.
    pub width: u32,
    /// Required height of each rectangle.
    pub height: u32,
    /// Number of identical rectangles required.
    pub quantity: usize,
    /// Whether the solver may rotate this rectangle 90 degrees.
    #[serde(default = "default_can_rotate")]
    pub can_rotate: bool,
}

/// A single rectangle placed on a packed sheet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Placement2D {
    /// Name of the originating demand.
    pub name: String,
    /// X offset of the rectangle's top-left corner on the sheet.
    pub x: u32,
    /// Y offset of the rectangle's top-left corner on the sheet.
    pub y: u32,
    /// Width of the placed rectangle after any rotation.
    pub width: u32,
    /// Height of the placed rectangle after any rotation.
    pub height: u32,
    /// Whether the rectangle was rotated 90 degrees from its declared orientation.
    pub rotated: bool,
}

/// A single packed sheet layout produced by the solver.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SheetLayout2D {
    /// Name of the sheet type consumed by this layout.
    pub sheet_name: String,
    /// Sheet width.
    pub width: u32,
    /// Sheet height.
    pub height: u32,
    /// Cost of consuming this sheet.
    pub cost: f64,
    /// Rectangles placed on this sheet.
    pub placements: Vec<Placement2D>,
    /// Total area occupied by the placements.
    pub used_area: u64,
    /// Total wasted area on this sheet.
    pub waste_area: u64,
    /// Area lost to kerf on this sheet. Included in `waste_area`; reported
    /// separately so callers can distinguish kerf loss from unused area.
    pub kerf_area: u64,
    /// Area of the single largest axis-aligned rectangle of unused space on
    /// this sheet whose width and height both satisfy
    /// `>= options.min_usable_side`. Zero if no such rectangle exists.
    pub largest_usable_drop_area: u64,
    /// Sum of `area²` over a canonical disjoint partition of this sheet's
    /// free region, restricted to rectangles passing `min_usable_side`.
    /// Rewards consolidation: for positive `a, b`, `a² + b² < (a+b)²`, so
    /// merging two adjacent drops into one strictly increases the sum.
    pub sum_sq_usable_drop_areas: u128,
}

/// Metrics captured while running a 2D solver.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SolverMetrics2D {
    /// Number of top-level solver iterations performed.
    pub iterations: usize,
    /// Number of states explored during search.
    pub explored_states: usize,
    /// Free-form notes emitted by the solver for diagnostics.
    pub notes: Vec<String>,
}

/// A complete solution returned by [`solve_2d`](super::solve_2d).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TwoDSolution {
    /// Name of the algorithm that produced this solution.
    pub algorithm: String,
    /// Whether the layouts are guillotine-compatible.
    pub guillotine: bool,
    /// Number of sheets consumed.
    pub sheet_count: usize,
    /// Total wasted area across all sheets.
    pub total_waste_area: u64,
    /// Total area lost to kerf across all layouts.
    pub total_kerf_area: u64,
    /// Total material cost across all sheets.
    pub total_cost: f64,
    /// Maximum of `SheetLayout2D.largest_usable_drop_area` across all layouts.
    /// Answers "what is the single biggest usable offcut this job yields?"
    pub max_usable_drop_area: u64,
    /// Sum (saturating) of `SheetLayout2D.sum_sq_usable_drop_areas` across
    /// all layouts. Summation is meaningful because the sum-of-squares is
    /// already additive over each sheet's disjoint free-region partition.
    pub total_sum_sq_usable_drop_areas: u128,
    /// Per-sheet layouts in descending order of utilization.
    pub layouts: Vec<SheetLayout2D>,
    /// Rectangles the solver was unable to place.
    pub unplaced: Vec<RectDemand2D>,
    /// Metrics captured while solving.
    pub metrics: SolverMetrics2D,
}

/// Input problem passed to [`solve_2d`](super::solve_2d).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TwoDProblem {
    /// Available sheet types.
    pub sheets: Vec<Sheet2D>,
    /// Rectangular demands to be placed on the sheets.
    pub demands: Vec<RectDemand2D>,
}

/// Options controlling how [`solve_2d`](super::solve_2d) runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TwoDOptions {
    /// Algorithm to dispatch to.
    #[serde(default)]
    pub algorithm: TwoDAlgorithm,
    /// Number of multistart restarts used by randomized strategies.
    #[serde(default = "default_multistart_runs")]
    pub multistart_runs: usize,
    /// Beam width for the guillotine beam search backend.
    #[serde(default = "default_beam_width")]
    pub beam_width: usize,
    /// Whether layouts must be guillotine-compatible.
    #[serde(default)]
    pub guillotine_required: bool,
    /// Optional seed for reproducible randomized algorithms.
    #[serde(default)]
    pub seed: Option<u64>,
    /// Minimum side length (both width and height) that a free-space rectangle
    /// must satisfy to be counted as a "usable drop." Rectangles with either
    /// side smaller than this threshold are treated as scrap and contribute
    /// zero to the consolidation metrics. Default `0` admits every drop.
    ///
    /// Affects the tiebreakers `max_usable_drop_area` and
    /// `total_sum_sq_usable_drop_areas` on `TwoDSolution`. Does not change
    /// the primary ranking objectives (unplaced, sheet_count, waste_area,
    /// cost).
    #[serde(default)]
    pub min_usable_side: u32,
    /// Maximum number of rotatable demand types for which rotation search
    /// uses exhaustive enumeration. When the number of rotatable types
    /// exceeds this threshold, rotation search switches to sampling
    /// `multistart_runs` random assignments instead. Also controls whether
    /// Auto mode includes rotation search as a candidate.
    #[serde(default = "default_auto_rotation_search_max_types")]
    pub auto_rotation_search_max_types: usize,
}

impl Default for TwoDOptions {
    fn default() -> Self {
        Self {
            algorithm: TwoDAlgorithm::Auto,
            multistart_runs: default_multistart_runs(),
            beam_width: default_beam_width(),
            guillotine_required: false,
            seed: None,
            min_usable_side: 0,
            auto_rotation_search_max_types: default_auto_rotation_search_max_types(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ItemInstance2D {
    pub(crate) name: String,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) can_rotate: bool,
}

impl ItemInstance2D {
    pub(crate) fn orientations(&self) -> impl Iterator<Item = (u32, u32, bool)> + '_ {
        let primary = std::iter::once((self.width, self.height, false));
        let rotated = self
            .can_rotate
            .then_some((self.height, self.width, true))
            .filter(|(width, height, _)| *width != self.width || *height != self.height)
            .into_iter();
        primary.chain(rotated)
    }
}

pub(crate) fn projected_fresh_sheet_fit_count(
    sheet: &Sheet2D,
    first_width: u32,
    first_height: u32,
    remaining_items: &[ItemInstance2D],
) -> usize {
    let (eff_w, eff_h) = effective_bounds(sheet);
    if first_width > eff_w || first_height > eff_h {
        return 0;
    }

    #[derive(Debug, Clone, Copy)]
    struct ShelfProjection {
        y: u32,
        height: u32,
        used_width: u32,
    }

    let mut shelves = vec![ShelfProjection { y: 0, height: first_height, used_width: first_width }];
    let mut count = 1_usize;

    for item in remaining_items {
        let existing = shelves
            .iter()
            .enumerate()
            .flat_map(|(shelf_index, shelf)| {
                item.orientations().filter_map(move |(width, height, _)| {
                    let gap = if shelf.used_width == 0 { 0 } else { sheet.kerf };
                    (height <= shelf.height
                        && shelf.used_width.saturating_add(gap).saturating_add(width) <= eff_w)
                        .then(|| {
                            let remaining_width = eff_w.saturating_sub(
                                shelf.used_width.saturating_add(gap).saturating_add(width),
                            );
                            (shelf_index, width, remaining_width, height)
                        })
                })
            })
            .min_by(|left, right| {
                left.2
                    .cmp(&right.2)
                    .then_with(|| left.3.cmp(&right.3))
                    .then_with(|| left.0.cmp(&right.0))
            });

        if let Some((shelf_index, width, _, _)) = existing {
            let shelf = &mut shelves[shelf_index];
            let gap = if shelf.used_width == 0 { 0 } else { sheet.kerf };
            shelf.used_width = shelf.used_width.saturating_add(gap).saturating_add(width);
            count = count.saturating_add(1);
            continue;
        }

        let base_y = shelves.last().map(|shelf| shelf.y.saturating_add(shelf.height)).unwrap_or(0);
        let gap = if base_y == 0 { 0 } else { sheet.kerf };
        let y = base_y.saturating_add(gap);
        let new_shelf = item
            .orientations()
            .filter(|(width, height, _)| *width <= eff_w && y.saturating_add(*height) <= eff_h)
            .map(|(width, height, _)| {
                (width, height, eff_w.saturating_sub(width), y.saturating_add(height))
            })
            .min_by(|left, right| {
                left.2
                    .cmp(&right.2)
                    .then_with(|| left.1.cmp(&right.1))
                    .then_with(|| left.3.cmp(&right.3))
            });

        if let Some((width, height, _, _)) = new_shelf {
            shelves.push(ShelfProjection { y, height, used_width: width });
            count = count.saturating_add(1);
        }
    }

    count
}

const MAX_DIMENSION: u32 = 1 << 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Rect {
    pub(crate) x: u32,
    pub(crate) y: u32,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

impl TwoDProblem {
    pub(crate) fn validate(&self) -> Result<()> {
        if self.sheets.is_empty() {
            return Err(BinPackingError::InvalidInput(
                "at least one sheet stock entry is required".to_string(),
            ));
        }

        if self.demands.is_empty() {
            return Err(BinPackingError::InvalidInput(
                "at least one rectangular demand entry is required".to_string(),
            ));
        }

        let mut sheet_names = HashSet::new();
        for sheet in &self.sheets {
            if !sheet_names.insert(sheet.name.as_str()) {
                return Err(BinPackingError::InvalidInput(format!(
                    "sheet name `{}` must be unique",
                    sheet.name
                )));
            }

            if sheet.width == 0 || sheet.height == 0 {
                return Err(BinPackingError::InvalidInput(format!(
                    "sheet `{}` must have positive width and height",
                    sheet.name
                )));
            }

            if sheet.width > MAX_DIMENSION || sheet.height > MAX_DIMENSION {
                return Err(BinPackingError::InvalidInput(format!(
                    "sheet `{}` dimensions exceed the supported maximum of {}",
                    sheet.name, MAX_DIMENSION
                )));
            }

            if !sheet.cost.is_finite() || sheet.cost < 0.0 {
                return Err(BinPackingError::InvalidInput(format!(
                    "sheet `{}` must have a finite non-negative cost",
                    sheet.name
                )));
            }

            let shortest_side = sheet.width.min(sheet.height);
            if u64::from(sheet.kerf) * 2 >= u64::from(shortest_side) {
                return Err(BinPackingError::InvalidInput(format!(
                    "sheet `{}` kerf {} is too large for shortest side {}",
                    sheet.name, sheet.kerf, shortest_side
                )));
            }
        }

        for demand in &self.demands {
            if demand.width == 0 || demand.height == 0 {
                return Err(BinPackingError::InvalidInput(format!(
                    "demand `{}` must have positive width and height",
                    demand.name
                )));
            }

            if demand.width > MAX_DIMENSION || demand.height > MAX_DIMENSION {
                return Err(BinPackingError::InvalidInput(format!(
                    "demand `{}` dimensions exceed the supported maximum of {}",
                    demand.name, MAX_DIMENSION
                )));
            }

            if demand.quantity == 0 {
                return Err(BinPackingError::InvalidInput(format!(
                    "demand `{}` must have positive quantity",
                    demand.name
                )));
            }
        }

        Ok(())
    }

    pub(crate) fn ensure_feasible_demands(&self) -> Result<()> {
        for demand in &self.demands {
            let feasible = self.sheets.iter().any(|sheet| {
                (sheet.width >= demand.width && sheet.height >= demand.height)
                    || (demand.can_rotate
                        && sheet.width >= demand.height
                        && sheet.height >= demand.width)
            });

            if !feasible {
                return Err(BinPackingError::Infeasible2D {
                    item: demand.name.clone(),
                    width: demand.width,
                    height: demand.height,
                });
            }
        }

        Ok(())
    }

    pub(crate) fn expanded_items(&self) -> Vec<ItemInstance2D> {
        let mut items = Vec::new();
        for demand in &self.demands {
            for _ in 0..demand.quantity {
                items.push(ItemInstance2D {
                    name: demand.name.clone(),
                    width: demand.width,
                    height: demand.height,
                    can_rotate: demand.can_rotate,
                });
            }
        }

        items
    }
}

impl Rect {
    pub(crate) fn area(self) -> u64 {
        u64::from(self.width) * u64::from(self.height)
    }

    pub(crate) fn fits(self, width: u32, height: u32) -> bool {
        width <= self.width && height <= self.height
    }

    pub(crate) fn intersects(self, other: Self) -> bool {
        let self_right = self.x.saturating_add(self.width);
        let self_bottom = self.y.saturating_add(self.height);
        let other_right = other.x.saturating_add(other.width);
        let other_bottom = other.y.saturating_add(other.height);

        self.x < other_right
            && self_right > other.x
            && self.y < other_bottom
            && self_bottom > other.y
    }

    pub(crate) fn contains(self, other: Self) -> bool {
        self.x <= other.x
            && self.y <= other.y
            && self.x.saturating_add(self.width) >= other.x.saturating_add(other.width)
            && self.y.saturating_add(self.height) >= other.y.saturating_add(other.height)
    }
}

impl TwoDSolution {
    pub(crate) fn from_layouts(
        algorithm: impl Into<String>,
        guillotine: bool,
        sheets: &[Sheet2D],
        layouts: Vec<(usize, Vec<Placement2D>)>,
        unplaced_items: Vec<ItemInstance2D>,
        metrics: SolverMetrics2D,
        min_usable_side: u32,
    ) -> Self {
        let mut computed_layouts = layouts
            .into_iter()
            .map(|(sheet_index, placements)| {
                let sheet = &sheets[sheet_index];
                let sheet_area = u64::from(sheet.width) * u64::from(sheet.height);
                let used_area = placements
                    .iter()
                    .map(|placement| {
                        let on_sheet_w = placement
                            .x
                            .saturating_add(placement.width)
                            .min(sheet.width)
                            .saturating_sub(placement.x);
                        let on_sheet_h = placement
                            .y
                            .saturating_add(placement.height)
                            .min(sheet.height)
                            .saturating_sub(placement.y);
                        u64::from(on_sheet_w) * u64::from(on_sheet_h)
                    })
                    .sum::<u64>();
                // In debug builds, fail loudly if a solver produced more used
                // area than the sheet provides — that indicates overlapping or
                // off-sheet placements upstream. Release builds fall back to
                // saturating subtraction so the bug surfaces as "0 waste"
                // instead of a u64 underflow panic.
                debug_assert!(
                    used_area <= sheet_area,
                    "sheet `{}` placements use {used_area} area but sheet capacity is {sheet_area}",
                    sheet.name,
                );
                let waste_area = sheet_area.saturating_sub(used_area);

                let kerf_area = super::kerf::kerf_area_for_layout(sheet, &placements);
                let (largest_usable_drop_area, sum_sq_usable_drop_areas) =
                    super::drops::usable_drop_metrics(sheet, &placements, min_usable_side);
                SheetLayout2D {
                    sheet_name: sheet.name.clone(),
                    width: sheet.width,
                    height: sheet.height,
                    cost: sheet.cost,
                    placements,
                    used_area,
                    waste_area,
                    kerf_area,
                    largest_usable_drop_area,
                    sum_sq_usable_drop_areas,
                }
            })
            .collect::<Vec<_>>();

        computed_layouts.sort_by(|left, right| {
            right
                .used_area
                .cmp(&left.used_area)
                .then_with(|| left.sheet_name.cmp(&right.sheet_name))
        });

        let total_waste_area = computed_layouts.iter().map(|layout| layout.waste_area).sum();
        let total_kerf_area = computed_layouts.iter().map(|layout| layout.kerf_area).sum();
        let total_cost = computed_layouts.iter().map(|layout| layout.cost).sum();
        let max_usable_drop_area = computed_layouts
            .iter()
            .map(|layout| layout.largest_usable_drop_area)
            .max()
            .unwrap_or(0);
        let total_sum_sq_usable_drop_areas = computed_layouts
            .iter()
            .map(|layout| layout.sum_sq_usable_drop_areas)
            .fold(0_u128, u128::saturating_add);
        let mut unplaced = unplaced_items
            .into_iter()
            .map(|item| RectDemand2D {
                name: item.name,
                width: item.width,
                height: item.height,
                quantity: 1,
                can_rotate: item.can_rotate,
            })
            .collect::<Vec<_>>();
        unplaced.sort_by(|left, right| {
            let left_area = u64::from(left.width) * u64::from(left.height);
            let right_area = u64::from(right.width) * u64::from(right.height);
            right_area.cmp(&left_area)
        });

        Self {
            algorithm: algorithm.into(),
            guillotine,
            sheet_count: computed_layouts.len(),
            total_waste_area,
            total_kerf_area,
            total_cost,
            max_usable_drop_area,
            total_sum_sq_usable_drop_areas,
            layouts: computed_layouts,
            unplaced,
            metrics,
        }
    }

    pub(crate) fn is_better_than(&self, other: &Self) -> bool {
        // Lexicographic ranking key:
        //   (unplaced_count, sheet_count, total_waste_area, total_cost,
        //    -max_usable_drop_area, -total_sum_sq_usable_drop_areas)
        //
        // Consolidation is a tiebreaker AFTER the primary objectives
        // (unplaced, sheet_count, waste_area, cost): a layout with even
        // 1 sq unit less waste always beats a layout with a bigger drop.
        // The consolidation terms are negated via `Reverse` so that *more*
        // drop area / *higher* sum-of-squares sorts earlier (is "better")
        // among candidates that tie on all four primary keys.
        use std::cmp::Reverse;
        (
            self.unplaced.len(),
            self.sheet_count,
            self.total_waste_area,
            OrderedFloat(self.total_cost),
            Reverse(self.max_usable_drop_area),
            Reverse(self.total_sum_sq_usable_drop_areas),
        ) < (
            other.unplaced.len(),
            other.sheet_count,
            other.total_waste_area,
            OrderedFloat(other.total_cost),
            Reverse(other.max_usable_drop_area),
            Reverse(other.total_sum_sq_usable_drop_areas),
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct OrderedFloat(pub f64);

impl Eq for OrderedFloat {}

impl PartialOrd for OrderedFloat {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for OrderedFloat {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.total_cmp(&other.0)
    }
}

fn default_sheet_cost() -> f64 {
    1.0
}

fn default_can_rotate() -> bool {
    true
}

fn default_multistart_runs() -> usize {
    12
}

fn default_beam_width() -> usize {
    8
}

fn default_auto_rotation_search_max_types() -> usize {
    16
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn sample_problem() -> TwoDProblem {
        TwoDProblem {
            sheets: vec![Sheet2D {
                name: "sheet".to_string(),
                width: 10,
                height: 8,
                cost: 1.0,
                quantity: None,
                kerf: 0,
                edge_kerf_relief: false,
            }],
            demands: vec![
                RectDemand2D {
                    name: "panel".to_string(),
                    width: 4,
                    height: 3,
                    quantity: 2,
                    can_rotate: true,
                },
                RectDemand2D {
                    name: "brace".to_string(),
                    width: 2,
                    height: 2,
                    quantity: 1,
                    can_rotate: false,
                },
            ],
        }
    }

    #[test]
    fn serde_defaults_fill_in_optional_sheet_and_option_fields() {
        let sheet: Sheet2D =
            serde_json::from_value(json!({ "name": "sheet", "width": 12, "height": 8 }))
                .expect("sheet");
        assert_eq!(sheet.cost, 1.0);
        assert_eq!(sheet.quantity, None);

        let demand: RectDemand2D = serde_json::from_value(
            json!({ "name": "panel", "width": 5, "height": 4, "quantity": 1 }),
        )
        .expect("demand");
        assert!(demand.can_rotate);

        let options: TwoDOptions = serde_json::from_value(json!({})).expect("options");
        assert_eq!(options, TwoDOptions::default());
    }

    #[test]
    fn validation_rejects_missing_or_invalid_two_d_inputs() {
        let missing_sheets = TwoDProblem { sheets: Vec::new(), demands: sample_problem().demands };
        assert!(matches!(
            missing_sheets.validate(),
            Err(BinPackingError::InvalidInput(message))
                if message == "at least one sheet stock entry is required"
        ));

        let missing_demands = TwoDProblem { sheets: sample_problem().sheets, demands: Vec::new() };
        assert!(matches!(
            missing_demands.validate(),
            Err(BinPackingError::InvalidInput(message))
                if message == "at least one rectangular demand entry is required"
        ));

        let zero_sheet = TwoDProblem {
            sheets: vec![Sheet2D {
                name: "sheet".to_string(),
                width: 0,
                height: 8,
                cost: 1.0,
                quantity: None,
                kerf: 0,
                edge_kerf_relief: false,
            }],
            demands: vec![RectDemand2D {
                name: "panel".to_string(),
                width: 1,
                height: 1,
                quantity: 1,
                can_rotate: false,
            }],
        };
        assert!(matches!(
            zero_sheet.validate(),
            Err(BinPackingError::InvalidInput(message))
                if message == "sheet `sheet` must have positive width and height"
        ));

        let zero_demand = TwoDProblem {
            sheets: vec![Sheet2D {
                name: "sheet".to_string(),
                width: 8,
                height: 8,
                cost: 1.0,
                quantity: None,
                kerf: 0,
                edge_kerf_relief: false,
            }],
            demands: vec![RectDemand2D {
                name: "panel".to_string(),
                width: 0,
                height: 2,
                quantity: 1,
                can_rotate: false,
            }],
        };
        assert!(matches!(
            zero_demand.validate(),
            Err(BinPackingError::InvalidInput(message))
                if message == "demand `panel` must have positive width and height"
        ));

        let zero_quantity = TwoDProblem {
            sheets: vec![Sheet2D {
                name: "sheet".to_string(),
                width: 8,
                height: 8,
                cost: 1.0,
                quantity: None,
                kerf: 0,
                edge_kerf_relief: false,
            }],
            demands: vec![RectDemand2D {
                name: "panel".to_string(),
                width: 2,
                height: 2,
                quantity: 0,
                can_rotate: false,
            }],
        };
        assert!(matches!(
            zero_quantity.validate(),
            Err(BinPackingError::InvalidInput(message))
                if message == "demand `panel` must have positive quantity"
        ));
    }

    #[test]
    fn feasibility_expansion_and_geometry_helpers_cover_rotation_paths() {
        let feasible = sample_problem();
        feasible.validate().expect("sample input should validate");
        feasible.ensure_feasible_demands().expect("sample input should be feasible");

        let rotated_only = TwoDProblem {
            sheets: vec![Sheet2D {
                name: "sheet".to_string(),
                width: 4,
                height: 6,
                cost: 1.0,
                quantity: None,
                kerf: 0,
                edge_kerf_relief: false,
            }],
            demands: vec![RectDemand2D {
                name: "rotated".to_string(),
                width: 6,
                height: 4,
                quantity: 1,
                can_rotate: true,
            }],
        };
        rotated_only.ensure_feasible_demands().expect("rotation should make item feasible");

        let infeasible = TwoDProblem {
            sheets: vec![Sheet2D {
                name: "sheet".to_string(),
                width: 4,
                height: 6,
                cost: 1.0,
                quantity: None,
                kerf: 0,
                edge_kerf_relief: false,
            }],
            demands: vec![RectDemand2D {
                name: "oversized".to_string(),
                width: 7,
                height: 4,
                quantity: 1,
                can_rotate: false,
            }],
        };
        assert!(matches!(
            infeasible.ensure_feasible_demands(),
            Err(BinPackingError::Infeasible2D { item, width, height })
                if item == "oversized" && width == 7 && height == 4
        ));

        let items = feasible.expanded_items();
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].name, "panel");
        assert_eq!(items[2].name, "brace");

        let outer = Rect { x: 0, y: 0, width: 10, height: 8 };
        let inner = Rect { x: 2, y: 2, width: 3, height: 4 };
        let disjoint = Rect { x: 10, y: 0, width: 2, height: 2 };
        assert_eq!(outer.area(), 80);
        assert!(outer.fits(5, 4));
        assert!(outer.contains(inner));
        assert!(outer.intersects(inner));
        assert!(!outer.intersects(disjoint));
    }

    #[test]
    fn from_layouts_sorts_outputs_and_better_than_prefers_fewer_sheets() {
        let sheets = vec![
            Sheet2D {
                name: "alpha".to_string(),
                width: 10,
                height: 10,
                cost: 2.0,
                quantity: None,
                kerf: 0,
                edge_kerf_relief: false,
            },
            Sheet2D {
                name: "beta".to_string(),
                width: 8,
                height: 8,
                cost: 1.0,
                quantity: None,
                kerf: 0,
                edge_kerf_relief: false,
            },
        ];

        let solution = TwoDSolution::from_layouts(
            "maxrects",
            false,
            &sheets,
            vec![
                (
                    1,
                    vec![Placement2D {
                        name: "small".to_string(),
                        x: 0,
                        y: 0,
                        width: 4,
                        height: 4,
                        rotated: false,
                    }],
                ),
                (
                    0,
                    vec![
                        Placement2D {
                            name: "large".to_string(),
                            x: 0,
                            y: 0,
                            width: 5,
                            height: 5,
                            rotated: false,
                        },
                        Placement2D {
                            name: "medium".to_string(),
                            x: 5,
                            y: 0,
                            width: 2,
                            height: 2,
                            rotated: false,
                        },
                    ],
                ),
            ],
            vec![
                ItemInstance2D { name: "wide".to_string(), width: 6, height: 2, can_rotate: true },
                ItemInstance2D { name: "tiny".to_string(), width: 1, height: 1, can_rotate: false },
            ],
            SolverMetrics2D { iterations: 3, explored_states: 2, notes: vec!["test".to_string()] },
            0,
        );

        assert_eq!(solution.layouts[0].sheet_name, "alpha");
        assert_eq!(solution.layouts[1].sheet_name, "beta");
        assert_eq!(solution.unplaced[0].name, "wide");
        assert_eq!(solution.total_cost, 3.0);
        assert_eq!(solution.total_waste_area, 119);

        let worse = TwoDSolution { sheet_count: solution.sheet_count + 1, ..solution.clone() };
        assert!(solution.is_better_than(&worse));
    }

    /// Edge cases for `Rect` geometry helpers that the placement code relies
    /// on. Each assertion pins down behavior that a plausible refactor could
    /// silently flip (for example, changing `intersects` to use `<=` instead
    /// of `<` would turn edge-touching rectangles into overlapping ones).
    #[test]
    fn rect_helpers_handle_boundary_cases() {
        // Touching along a single edge does NOT count as intersecting.
        // `left_right == right.x` means the strict `<` in `intersects` fails.
        let left = Rect { x: 0, y: 0, width: 5, height: 5 };
        let right = Rect { x: 5, y: 0, width: 5, height: 5 };
        assert!(!left.intersects(right), "edge-touching rects are not intersecting");
        assert!(!right.intersects(left), "intersects is symmetric for edge-touching rects");

        // Touching at a single corner is also not intersecting.
        let corner = Rect { x: 5, y: 5, width: 5, height: 5 };
        assert!(!left.intersects(corner));

        // Overlap by even one unit IS intersecting.
        let overlap_by_one = Rect { x: 4, y: 0, width: 5, height: 5 };
        assert!(left.intersects(overlap_by_one));

        // Self-containment holds (contains uses `<=` and `>=`).
        assert!(left.contains(left), "a rect should contain itself");

        // Exact fit — `fits` uses `<=` so identical dimensions are accepted.
        assert!(left.fits(5, 5));
        // One unit larger in either dimension does not fit.
        assert!(!left.fits(6, 5));
        assert!(!left.fits(5, 6));

        // Area widens to u64 so extreme dimensions do not overflow.
        let huge = Rect { x: 0, y: 0, width: u32::MAX, height: u32::MAX };
        assert_eq!(huge.area(), u64::from(u32::MAX) * u64::from(u32::MAX));
    }

    /// `TwoDSolution::is_better_than` compares on a 4-key tuple:
    /// (unplaced count, sheet_count, total_waste_area, total_cost).
    /// Verify each key is consulted in order as a tiebreaker.
    #[test]
    fn two_d_is_better_than_tie_breaks_on_each_key() {
        let sheets = vec![Sheet2D {
            name: "s".to_string(),
            width: 10,
            height: 10,
            cost: 1.0,
            quantity: None,
            kerf: 0,
            edge_kerf_relief: false,
        }];
        let base = TwoDSolution::from_layouts(
            "test",
            false,
            &sheets,
            vec![(
                0,
                vec![Placement2D {
                    name: "x".to_string(),
                    x: 0,
                    y: 0,
                    width: 5,
                    height: 5,
                    rotated: false,
                }],
            )],
            Vec::new(),
            SolverMetrics2D { iterations: 0, explored_states: 0, notes: Vec::new() },
            0,
        );

        // Fewer unplaced beats more unplaced (primary key).
        let more_unplaced = TwoDSolution {
            unplaced: vec![RectDemand2D {
                name: "u".to_string(),
                width: 1,
                height: 1,
                quantity: 1,
                can_rotate: false,
            }],
            ..base.clone()
        };
        assert!(base.is_better_than(&more_unplaced));
        assert!(!more_unplaced.is_better_than(&base));

        // Fewer sheets wins when unplaced ties.
        let more_sheets = TwoDSolution { sheet_count: base.sheet_count + 1, ..base.clone() };
        assert!(base.is_better_than(&more_sheets));

        // Less waste wins when unplaced and sheets tie.
        let more_waste =
            TwoDSolution { total_waste_area: base.total_waste_area + 100, ..base.clone() };
        assert!(base.is_better_than(&more_waste));

        // Lower cost wins when every preceding key ties.
        let more_cost = TwoDSolution { total_cost: base.total_cost + 1.0, ..base.clone() };
        assert!(base.is_better_than(&more_cost));

        // Strictly equal solutions are not "better than" each other.
        assert!(!base.is_better_than(&base));
    }

    /// `ItemInstance2D::orientations` should collapse the rotated orientation
    /// to a no-op when the item is square, even if `can_rotate` is true.
    /// That avoids the solver double-evaluating an identical placement.
    #[test]
    fn item_orientations_collapse_squares_to_one_arm() {
        let square =
            ItemInstance2D { name: "square".to_string(), width: 5, height: 5, can_rotate: true };
        let orientations = square.orientations().collect::<Vec<_>>();
        assert_eq!(
            orientations.len(),
            1,
            "square with can_rotate=true should emit exactly one orientation"
        );
        assert_eq!(orientations[0], (5, 5, false));

        let non_square =
            ItemInstance2D { name: "rect".to_string(), width: 3, height: 7, can_rotate: true };
        let orientations = non_square.orientations().collect::<Vec<_>>();
        assert_eq!(orientations.len(), 2);
        assert_eq!(orientations[0], (3, 7, false));
        assert_eq!(orientations[1], (7, 3, true));

        let non_rotatable =
            ItemInstance2D { name: "rect".to_string(), width: 3, height: 7, can_rotate: false };
        let orientations = non_rotatable.orientations().collect::<Vec<_>>();
        assert_eq!(orientations.len(), 1);
        assert_eq!(orientations[0], (3, 7, false));
    }

    #[test]
    fn sheet_kerf_defaults_to_zero_when_absent_from_json() {
        let sheet: Sheet2D =
            serde_json::from_value(json!({ "name": "s", "width": 10, "height": 10 }))
                .expect("sheet");
        assert_eq!(sheet.kerf, 0);

        let sheet_with_kerf: Sheet2D =
            serde_json::from_value(json!({ "name": "s", "width": 10, "height": 10, "kerf": 3 }))
                .expect("sheet");
        assert_eq!(sheet_with_kerf.kerf, 3);
    }

    #[test]
    fn validation_rejects_kerf_that_consumes_entire_sheet() {
        let bad = TwoDProblem {
            sheets: vec![Sheet2D {
                name: "thin".to_string(),
                width: 4,
                height: 20,
                cost: 1.0,
                quantity: None,
                kerf: 2,
                edge_kerf_relief: false,
            }],
            demands: vec![RectDemand2D {
                name: "x".to_string(),
                width: 1,
                height: 1,
                quantity: 1,
                can_rotate: false,
            }],
        };
        assert!(matches!(
            bad.validate(),
            Err(BinPackingError::InvalidInput(message))
                if message == "sheet `thin` kerf 2 is too large for shortest side 4"
        ));
    }

    #[test]
    fn from_layouts_populates_zero_kerf_area_when_sheet_kerf_is_zero() {
        let sheets = vec![Sheet2D {
            name: "s".to_string(),
            width: 10,
            height: 10,
            cost: 1.0,
            quantity: None,
            kerf: 0,
            edge_kerf_relief: false,
        }];
        let solution = TwoDSolution::from_layouts(
            "test",
            false,
            &sheets,
            vec![(
                0,
                vec![Placement2D {
                    name: "a".to_string(),
                    x: 0,
                    y: 0,
                    width: 5,
                    height: 5,
                    rotated: false,
                }],
            )],
            Vec::new(),
            SolverMetrics2D { iterations: 0, explored_states: 0, notes: Vec::new() },
            0,
        );
        assert_eq!(solution.layouts[0].kerf_area, 0);
        assert_eq!(solution.total_kerf_area, 0);
    }

    #[test]
    fn two_d_options_min_usable_side_defaults_to_zero_when_absent_from_json() {
        let options: TwoDOptions = serde_json::from_value(json!({})).expect("options");
        assert_eq!(options.min_usable_side, 0);

        let options_with_threshold: TwoDOptions =
            serde_json::from_value(json!({ "min_usable_side": 12 })).expect("options");
        assert_eq!(options_with_threshold.min_usable_side, 12);
    }

    #[test]
    fn from_layouts_populates_zero_consolidation_metrics_when_threshold_is_zero_and_kerf_is_zero() {
        // With min_usable_side=0 and a single full-sheet placement leaving zero
        // waste, the consolidation metrics on the layout must all be zero.
        let sheets = vec![Sheet2D {
            name: "s".to_string(),
            width: 10,
            height: 10,
            cost: 1.0,
            quantity: None,
            kerf: 0,
            edge_kerf_relief: false,
        }];
        let solution = TwoDSolution::from_layouts(
            "test",
            false,
            &sheets,
            vec![(
                0,
                vec![Placement2D {
                    name: "a".to_string(),
                    x: 0,
                    y: 0,
                    width: 10,
                    height: 10,
                    rotated: false,
                }],
            )],
            Vec::new(),
            SolverMetrics2D { iterations: 0, explored_states: 0, notes: Vec::new() },
            /* min_usable_side */ 0,
        );
        assert_eq!(solution.layouts[0].largest_usable_drop_area, 0);
        assert_eq!(solution.layouts[0].sum_sq_usable_drop_areas, 0);
        assert_eq!(solution.max_usable_drop_area, 0);
        assert_eq!(solution.total_sum_sq_usable_drop_areas, 0);
    }

    /// Helper: build a minimal `TwoDSolution` by overriding the consolidation
    /// fields on a base solution. `from_layouts` does not expose those fields
    /// directly, so we build via `from_layouts` (which populates them from
    /// `drops::usable_drop_metrics`) and then reconstruct with struct-update
    /// syntax to test specific metric values.
    fn make_solution_with_metrics(
        max_drop: u64,
        sum_sq: u128,
        waste_area: u64,
        cost: f64,
    ) -> TwoDSolution {
        // A 100×100 sheet; place nothing so waste = 10000.  We then
        // override the consolidation and waste fields to the desired values.
        let sheets = vec![Sheet2D {
            name: "s".to_string(),
            width: 100,
            height: 100,
            cost,
            quantity: None,
            kerf: 0,
            edge_kerf_relief: false,
        }];
        let base = TwoDSolution::from_layouts(
            "test",
            false,
            &sheets,
            vec![(0, Vec::new())],
            Vec::new(),
            SolverMetrics2D { iterations: 0, explored_states: 0, notes: Vec::new() },
            0,
        );
        TwoDSolution {
            max_usable_drop_area: max_drop,
            total_sum_sq_usable_drop_areas: sum_sq,
            total_waste_area: waste_area,
            total_cost: cost,
            ..base
        }
    }

    /// `is_better_than` must prefer the solution with the larger
    /// `max_usable_drop_area` when all primary keys are equal.
    #[test]
    fn consolidation_tiebreak_picks_larger_drop() {
        // Both solutions: 0 unplaced, 1 sheet, waste=100, cost=1.0.
        // A has max_drop=80; B has max_drop=40.  A should be preferred.
        let a = make_solution_with_metrics(80, 6_400, 100, 1.0);
        let b = make_solution_with_metrics(40, 1_600, 100, 1.0);

        assert!(a.is_better_than(&b), "larger max_usable_drop_area should win the tiebreak");
        assert!(!b.is_better_than(&a), "smaller max_usable_drop_area should lose the tiebreak");
        // Strictly equal: neither is better.
        assert!(!a.is_better_than(&a));
    }

    /// When `max_usable_drop_area` is also tied, `total_sum_sq_usable_drop_areas`
    /// is the next tiebreaker: higher sum-of-squares wins.
    #[test]
    fn consolidation_tiebreak_picks_more_concentrated_sum_sq() {
        // Both solutions: same unplaced, sheets, max_drop=50, waste=100, cost=1.0.
        // A has sum_sq=2500 (consolidated); B has sum_sq=500 (fragmented).
        let a = make_solution_with_metrics(50, 2_500, 100, 1.0);
        let b = make_solution_with_metrics(50, 500, 100, 1.0);

        assert!(
            a.is_better_than(&b),
            "higher sum_sq should win the tiebreak when max_drop is tied"
        );
        assert!(
            !b.is_better_than(&a),
            "lower sum_sq should lose the tiebreak when max_drop is tied"
        );
    }

    /// `total_waste_area` must still beat the consolidation tiebreaker:
    /// a solution with lower waste must win even if it has a tiny drop.
    #[test]
    fn waste_area_beats_consolidation() {
        // A: waste=100, max_drop=5 (tiny drop)
        // B: waste=101, max_drop=90 (huge drop)
        // A must win because waste is checked before consolidation.
        let a = make_solution_with_metrics(5, 25, 100, 1.0);
        let b = make_solution_with_metrics(90, 8_100, 101, 1.0);

        assert!(a.is_better_than(&b), "lower waste_area must beat better consolidation");
        assert!(!b.is_better_than(&a), "better consolidation must not override lower waste_area");
    }

    #[test]
    fn sheet_edge_kerf_relief_defaults_to_false_when_absent() {
        let sheet: Sheet2D =
            serde_json::from_value(json!({ "name": "s", "width": 10, "height": 10 }))
                .expect("sheet");
        assert!(!sheet.edge_kerf_relief);

        let sheet_with_relief: Sheet2D = serde_json::from_value(json!({
            "name": "s",
            "width": 10,
            "height": 10,
            "edge_kerf_relief": true
        }))
        .expect("sheet");
        assert!(sheet_with_relief.edge_kerf_relief);
    }

    #[test]
    fn effective_bounds_returns_sheet_dims_when_relief_off() {
        let s = Sheet2D {
            name: "s".into(),
            width: 100,
            height: 50,
            cost: 1.0,
            quantity: None,
            kerf: 3,
            edge_kerf_relief: false,
        };
        assert_eq!(effective_bounds(&s), (100, 50));
    }

    #[test]
    fn effective_bounds_pads_by_kerf_when_relief_on() {
        let s = Sheet2D {
            name: "s".into(),
            width: 100,
            height: 50,
            cost: 1.0,
            quantity: None,
            kerf: 3,
            edge_kerf_relief: true,
        };
        assert_eq!(effective_bounds(&s), (103, 53));
    }

    #[test]
    fn effective_bounds_saturates_at_u32_max() {
        let s = Sheet2D {
            name: "s".into(),
            width: u32::MAX,
            height: u32::MAX,
            cost: 1.0,
            quantity: None,
            kerf: 10,
            edge_kerf_relief: true,
        };
        assert_eq!(effective_bounds(&s), (u32::MAX, u32::MAX));
    }

    #[test]
    fn from_layouts_clips_used_area_for_overrun_placements() {
        // Two 24-wide placements on a 48-wide sheet with kerf=1 and edge
        // relief enabled. Second placement spans x=25..49, overrunning by 1.
        let sheets = vec![Sheet2D {
            name: "s".into(),
            width: 48,
            height: 10,
            cost: 1.0,
            quantity: None,
            kerf: 1,
            edge_kerf_relief: true,
        }];
        let placements = vec![
            Placement2D { name: "a".into(), x: 0, y: 0, width: 24, height: 10, rotated: false },
            Placement2D { name: "b".into(), x: 25, y: 0, width: 24, height: 10, rotated: false },
        ];

        let solution = TwoDSolution::from_layouts(
            "test",
            true,
            &sheets,
            vec![(0, placements)],
            Vec::new(),
            SolverMetrics2D { iterations: 0, explored_states: 0, notes: Vec::new() },
            0,
        );

        let layout = &solution.layouts[0];
        let sheet_area = u64::from(48_u32) * u64::from(10_u32);
        assert!(
            layout.used_area <= sheet_area,
            "used_area {} must not exceed sheet area {}",
            layout.used_area,
            sheet_area
        );
        // Part A contributes 24*10 = 240. Part B's on-sheet portion is
        // 23*10 = 230 (x=25..48 clipped from x=25..49). Sum = 470.
        assert_eq!(layout.used_area, 470);
    }
}
