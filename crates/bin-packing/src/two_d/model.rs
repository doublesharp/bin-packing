//! Data model types for 2D rectangular bin packing problems and solutions.

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
    /// Total material cost across all sheets.
    pub total_cost: f64,
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
}

impl Default for TwoDOptions {
    fn default() -> Self {
        Self {
            algorithm: TwoDAlgorithm::Auto,
            multistart_runs: default_multistart_runs(),
            beam_width: default_beam_width(),
            guillotine_required: false,
            seed: None,
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

        for sheet in &self.sheets {
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
    ) -> Self {
        let mut computed_layouts = layouts
            .into_iter()
            .map(|(sheet_index, placements)| {
                let sheet = &sheets[sheet_index];
                let sheet_area = u64::from(sheet.width) * u64::from(sheet.height);
                let used_area = placements
                    .iter()
                    .map(|placement| u64::from(placement.width) * u64::from(placement.height))
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

                SheetLayout2D {
                    sheet_name: sheet.name.clone(),
                    width: sheet.width,
                    height: sheet.height,
                    cost: sheet.cost,
                    placements,
                    used_area,
                    waste_area,
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
        let total_cost = computed_layouts.iter().map(|layout| layout.cost).sum();
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
            total_cost,
            layouts: computed_layouts,
            unplaced,
            metrics,
        }
    }

    pub(crate) fn is_better_than(&self, other: &Self) -> bool {
        (
            self.unplaced.len(),
            self.sheet_count,
            self.total_waste_area,
            OrderedFloat(self.total_cost),
        ) < (
            other.unplaced.len(),
            other.sheet_count,
            other.total_waste_area,
            OrderedFloat(other.total_cost),
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
            Sheet2D { name: "alpha".to_string(), width: 10, height: 10, cost: 2.0, quantity: None },
            Sheet2D { name: "beta".to_string(), width: 8, height: 8, cost: 1.0, quantity: None },
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
}
