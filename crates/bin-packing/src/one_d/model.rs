//! Data model types for 1D cutting stock problems and solutions.

use serde::{Deserialize, Serialize};

use crate::{BinPackingError, Result};

/// Maximum allowed value for stock/demand lengths and kerf/trim. Chosen so that
/// `length * quantity` in the exact backend (where `quantity` is a `u32`-bound
/// `usize`) cannot overflow a `u64`. Matches the 2D model's `MAX_DIMENSION`.
const MAX_DIMENSION: u32 = 1 << 30;

/// Algorithm selector for [`solve_1d`](super::solve_1d).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum OneDAlgorithm {
    /// Try multiple strategies and return the best.
    #[default]
    Auto,
    /// Deterministic first-fit-decreasing construction.
    FirstFitDecreasing,
    /// Deterministic best-fit-decreasing construction.
    BestFitDecreasing,
    /// Multistart local search with bin-elimination repair.
    LocalSearch,
    /// Exact column generation with pattern-search refinement.
    ColumnGeneration,
}

/// A linear stock entry that demands can be cut from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stock1D {
    /// Human-readable identifier for this stock type.
    pub name: String,
    /// Raw length of the stock before any trim is removed.
    pub length: u32,
    /// Material lost to the saw between adjacent cuts.
    #[serde(default)]
    pub kerf: u32,
    /// Unusable material removed from the stock length before packing.
    #[serde(default)]
    pub trim: u32,
    /// Per-unit cost of consuming a piece of this stock type.
    #[serde(default = "default_stock_cost")]
    pub cost: f64,
    /// Optional cap on the number of pieces of this stock type that may be used.
    #[serde(default)]
    pub available: Option<usize>,
}

/// A demand for a set of identical 1D cuts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CutDemand1D {
    /// Human-readable identifier for the demand.
    pub name: String,
    /// Required length of each cut.
    pub length: u32,
    /// Number of identical cuts required.
    pub quantity: usize,
}

/// A single cut assigned to a packed stock layout.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CutAssignment1D {
    /// Name of the originating demand.
    pub name: String,
    /// Length of this individual cut.
    pub length: u32,
}

/// A single packed stock layout produced by the solver.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StockLayout1D {
    /// Name of the stock type consumed by this layout.
    pub stock_name: String,
    /// Raw length of the stock piece.
    pub stock_length: u32,
    /// Length occupied by placed cuts (including kerf contributions).
    pub used_length: u32,
    /// Remaining length after the placed cuts.
    pub remaining_length: u32,
    /// Length wasted on this layout.
    pub waste: u32,
    /// Cost of consuming this stock piece.
    pub cost: f64,
    /// Cuts assigned to this layout.
    pub cuts: Vec<CutAssignment1D>,
}

/// Per-stock procurement summary for a 1D cut list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StockRequirement1D {
    /// Name of the stock type.
    pub stock_name: String,
    /// Raw length of the stock piece.
    pub stock_length: u32,
    /// Usable length after trim is removed.
    pub usable_length: u32,
    /// Cost of consuming one piece of this stock type.
    pub cost: f64,
    /// Declared inventory available for this stock type, if capped.
    pub available_quantity: Option<usize>,
    /// Number of pieces used in the returned solution.
    pub used_quantity: usize,
    /// Number of pieces required to satisfy the full cut list with the chosen stock mix.
    pub required_quantity: usize,
    /// Additional pieces needed beyond `available_quantity`.
    pub additional_quantity_needed: usize,
}

/// Metrics captured while running a 1D solver.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SolverMetrics1D {
    /// Number of top-level solver iterations performed.
    pub iterations: usize,
    /// Number of candidate patterns generated during column generation.
    pub generated_patterns: usize,
    /// Number of patterns enumerated in the exact backend.
    pub enumerated_patterns: usize,
    /// Number of states explored during local search or branching.
    pub explored_states: usize,
    /// Free-form notes emitted by the solver for diagnostics.
    pub notes: Vec<String>,
}

/// A complete solution returned by [`solve_1d`](super::solve_1d).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OneDSolution {
    /// Name of the algorithm that produced this solution.
    pub algorithm: String,
    /// Whether the solution is proven optimal.
    pub exact: bool,
    /// Optional lower bound on the optimal objective.
    pub lower_bound: Option<f64>,
    /// Number of stock pieces consumed.
    pub stock_count: usize,
    /// Total wasted length across all layouts.
    pub total_waste: u64,
    /// Total material cost across all layouts.
    pub total_cost: f64,
    /// Per-stock layouts in descending order of utilization.
    pub layouts: Vec<StockLayout1D>,
    /// Per-stock requirement summary, including any shortage against declared availability.
    #[serde(default)]
    pub stock_requirements: Vec<StockRequirement1D>,
    /// Cuts the solver was unable to place.
    pub unplaced: Vec<CutAssignment1D>,
    /// Metrics captured while solving.
    pub metrics: SolverMetrics1D,
}

/// Input problem passed to [`solve_1d`](super::solve_1d).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OneDProblem {
    /// Available stock types.
    pub stock: Vec<Stock1D>,
    /// Demands to be cut from the stock.
    pub demands: Vec<CutDemand1D>,
}

/// Options controlling how [`solve_1d`](super::solve_1d) runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OneDOptions {
    /// Algorithm to dispatch to.
    #[serde(default)]
    pub algorithm: OneDAlgorithm,
    /// Number of multistart restarts used by local search.
    #[serde(default = "default_multistart_runs")]
    pub multistart_runs: usize,
    /// Number of improvement rounds per local search start.
    #[serde(default = "default_improvement_rounds")]
    pub improvement_rounds: usize,
    /// Number of column-generation rounds in the exact backend.
    #[serde(default = "default_column_generation_rounds")]
    pub column_generation_rounds: usize,
    /// Maximum number of patterns enumerated by the exact backend.
    #[serde(default = "default_exact_pattern_limit")]
    pub exact_pattern_limit: usize,
    /// Maximum number of demand types for the Auto mode to attempt exact solving.
    #[serde(default = "default_auto_exact_max_types")]
    pub auto_exact_max_types: usize,
    /// Maximum total quantity for the Auto mode to attempt exact solving.
    #[serde(default = "default_auto_exact_max_quantity")]
    pub auto_exact_max_quantity: usize,
    /// Optional seed for reproducible randomized algorithms.
    #[serde(default)]
    pub seed: Option<u64>,
}

impl Default for OneDOptions {
    fn default() -> Self {
        Self {
            algorithm: OneDAlgorithm::Auto,
            multistart_runs: default_multistart_runs(),
            improvement_rounds: default_improvement_rounds(),
            column_generation_rounds: default_column_generation_rounds(),
            exact_pattern_limit: default_exact_pattern_limit(),
            auto_exact_max_types: default_auto_exact_max_types(),
            auto_exact_max_quantity: default_auto_exact_max_quantity(),
            seed: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PieceInstance {
    pub(crate) demand_index: usize,
    pub(crate) name: String,
    pub(crate) length: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PackedBin {
    pub(crate) stock_index: usize,
    pub(crate) pieces: Vec<PieceInstance>,
    occupied_length: u32,
}

impl Stock1D {
    pub(crate) fn usable_length(&self) -> u32 {
        self.length.saturating_sub(self.trim)
    }

    pub(crate) fn adjusted_capacity(&self) -> u32 {
        self.usable_length().saturating_add(self.kerf)
    }
}

impl OneDProblem {
    pub(crate) fn validate(&self) -> Result<()> {
        if self.stock.is_empty() {
            return Err(BinPackingError::InvalidInput(
                "at least one stock entry is required".to_string(),
            ));
        }

        if self.demands.is_empty() {
            return Err(BinPackingError::InvalidInput(
                "at least one demand entry is required".to_string(),
            ));
        }

        for stock in &self.stock {
            if stock.length == 0 {
                return Err(BinPackingError::InvalidInput(format!(
                    "stock `{}` must have a positive length",
                    stock.name
                )));
            }

            if stock.length > MAX_DIMENSION {
                return Err(BinPackingError::InvalidInput(format!(
                    "stock `{}` length {} exceeds the supported maximum of {MAX_DIMENSION}",
                    stock.name, stock.length
                )));
            }

            if stock.kerf > MAX_DIMENSION {
                return Err(BinPackingError::InvalidInput(format!(
                    "stock `{}` kerf {} exceeds the supported maximum of {MAX_DIMENSION}",
                    stock.name, stock.kerf
                )));
            }

            if stock.trim > MAX_DIMENSION {
                return Err(BinPackingError::InvalidInput(format!(
                    "stock `{}` trim {} exceeds the supported maximum of {MAX_DIMENSION}",
                    stock.name, stock.trim
                )));
            }

            if stock.usable_length() == 0 {
                return Err(BinPackingError::InvalidInput(format!(
                    "stock `{}` has no usable length after trim",
                    stock.name
                )));
            }

            if !stock.cost.is_finite() || stock.cost < 0.0 {
                return Err(BinPackingError::InvalidInput(format!(
                    "stock `{}` must have a finite non-negative cost",
                    stock.name
                )));
            }
        }

        for demand in &self.demands {
            if demand.length == 0 {
                return Err(BinPackingError::InvalidInput(format!(
                    "demand `{}` must have a positive length",
                    demand.name
                )));
            }

            if demand.length > MAX_DIMENSION {
                return Err(BinPackingError::InvalidInput(format!(
                    "demand `{}` length {} exceeds the supported maximum of {MAX_DIMENSION}",
                    demand.name, demand.length
                )));
            }

            if demand.quantity == 0 {
                return Err(BinPackingError::InvalidInput(format!(
                    "demand `{}` must have a positive quantity",
                    demand.name
                )));
            }
        }

        Ok(())
    }

    pub(crate) fn ensure_feasible_demands(&self) -> Result<()> {
        for demand in &self.demands {
            let feasible = self.stock.iter().any(|stock| stock.usable_length() >= demand.length);
            if !feasible {
                return Err(BinPackingError::Infeasible1D {
                    item: demand.name.clone(),
                    length: demand.length,
                });
            }
        }

        Ok(())
    }

    pub(crate) fn total_quantity(&self) -> usize {
        self.demands.iter().map(|demand| demand.quantity).sum()
    }

    pub(crate) fn expanded_pieces(&self) -> Vec<PieceInstance> {
        let mut pieces = Vec::with_capacity(self.total_quantity());

        for (index, demand) in self.demands.iter().enumerate() {
            for _ in 0..demand.quantity {
                pieces.push(PieceInstance {
                    demand_index: index,
                    name: demand.name.clone(),
                    length: demand.length,
                });
            }
        }

        pieces
    }
}

impl PackedBin {
    pub(crate) fn new(stock_index: usize) -> Self {
        Self { stock_index, pieces: Vec::new(), occupied_length: 0 }
    }

    pub(crate) fn delta_for_piece(&self, piece: &PieceInstance, stock: &Stock1D) -> Option<u32> {
        let delta = if self.pieces.is_empty() {
            piece.length
        } else {
            piece.length.saturating_add(stock.kerf)
        };

        (self.occupied_length.saturating_add(delta) <= stock.usable_length()).then_some(delta)
    }

    pub(crate) fn can_fit_piece(&self, piece: &PieceInstance, stock: &Stock1D) -> bool {
        self.delta_for_piece(piece, stock).is_some()
    }

    pub(crate) fn add_piece(&mut self, piece: PieceInstance, stock: &Stock1D) -> bool {
        if let Some(delta) = self.delta_for_piece(&piece, stock) {
            self.occupied_length = self.occupied_length.saturating_add(delta);
            self.pieces.push(piece);
            true
        } else {
            false
        }
    }

    pub(crate) fn used_length(&self) -> u32 {
        self.occupied_length
    }

    pub(crate) fn remaining_length(&self, stock: &Stock1D) -> u32 {
        stock.usable_length().saturating_sub(self.occupied_length)
    }
}

impl OneDSolution {
    pub(crate) fn from_bins(
        algorithm: impl Into<String>,
        exact: bool,
        lower_bound: Option<f64>,
        stock: &[Stock1D],
        bins: &[PackedBin],
        unplaced: &[PieceInstance],
        metrics: SolverMetrics1D,
    ) -> Self {
        let used_counts = count_stock_usage(stock.len(), bins);
        let mut layouts = bins
            .iter()
            .map(|bin| {
                let stock_entry = &stock[bin.stock_index];
                let used_length = bin.used_length();
                let remaining_length = bin.remaining_length(stock_entry);

                StockLayout1D {
                    stock_name: stock_entry.name.clone(),
                    stock_length: stock_entry.length,
                    used_length,
                    remaining_length,
                    waste: remaining_length,
                    cost: stock_entry.cost,
                    cuts: bin
                        .pieces
                        .iter()
                        .map(|piece| CutAssignment1D {
                            name: piece.name.clone(),
                            length: piece.length,
                        })
                        .collect(),
                }
            })
            .collect::<Vec<_>>();

        layouts.sort_by(|left, right| {
            right
                .used_length
                .cmp(&left.used_length)
                .then_with(|| left.stock_name.cmp(&right.stock_name))
        });

        let total_waste = layouts.iter().map(|layout| u64::from(layout.waste)).sum();
        let total_cost = layouts.iter().map(|layout| layout.cost).sum();

        let mut unplaced_cuts = unplaced
            .iter()
            .map(|piece| CutAssignment1D { name: piece.name.clone(), length: piece.length })
            .collect::<Vec<_>>();
        unplaced_cuts.sort_by(|left, right| right.length.cmp(&left.length));

        Self {
            algorithm: algorithm.into(),
            exact,
            lower_bound,
            stock_count: layouts.len(),
            total_waste,
            total_cost,
            layouts,
            stock_requirements: build_stock_requirements(stock, &used_counts, &used_counts),
            unplaced: unplaced_cuts,
            metrics,
        }
    }

    pub(crate) fn set_required_stock_counts(
        &mut self,
        stock: &[Stock1D],
        required_counts: &[usize],
    ) {
        let used_counts = self
            .stock_requirements
            .iter()
            .map(|requirement| requirement.used_quantity)
            .collect::<Vec<_>>();
        self.stock_requirements = build_stock_requirements(stock, &used_counts, required_counts);
    }

    pub(crate) fn is_better_than(&self, other: &Self) -> bool {
        (
            self.unplaced.len(),
            self.stock_count,
            self.total_waste,
            OrderedFloat(self.total_cost),
            !self.exact,
        ) < (
            other.unplaced.len(),
            other.stock_count,
            other.total_waste,
            OrderedFloat(other.total_cost),
            !other.exact,
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

fn default_stock_cost() -> f64 {
    1.0
}

fn count_stock_usage(stock_count: usize, bins: &[PackedBin]) -> Vec<usize> {
    let mut counts = vec![0_usize; stock_count];
    for bin in bins {
        counts[bin.stock_index] = counts[bin.stock_index].saturating_add(1);
    }
    counts
}

fn build_stock_requirements(
    stock: &[Stock1D],
    used_counts: &[usize],
    required_counts: &[usize],
) -> Vec<StockRequirement1D> {
    stock
        .iter()
        .enumerate()
        .map(|(index, stock_entry)| {
            let used_quantity = *used_counts.get(index).unwrap_or(&0);
            let required_quantity = *required_counts.get(index).unwrap_or(&used_quantity);
            let additional_quantity_needed = stock_entry
                .available
                .map(|available| required_quantity.saturating_sub(available))
                .unwrap_or(0);

            StockRequirement1D {
                stock_name: stock_entry.name.clone(),
                stock_length: stock_entry.length,
                usable_length: stock_entry.usable_length(),
                cost: stock_entry.cost,
                available_quantity: stock_entry.available,
                used_quantity,
                required_quantity,
                additional_quantity_needed,
            }
        })
        .collect()
}

fn default_multistart_runs() -> usize {
    16
}

fn default_improvement_rounds() -> usize {
    24
}

fn default_column_generation_rounds() -> usize {
    32
}

fn default_exact_pattern_limit() -> usize {
    25_000
}

fn default_auto_exact_max_types() -> usize {
    14
}

fn default_auto_exact_max_quantity() -> usize {
    96
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn sample_problem() -> OneDProblem {
        OneDProblem {
            stock: vec![Stock1D {
                name: "bar".to_string(),
                length: 10,
                kerf: 1,
                trim: 2,
                cost: 1.0,
                available: None,
            }],
            demands: vec![
                CutDemand1D { name: "leg".to_string(), length: 4, quantity: 2 },
                CutDemand1D { name: "brace".to_string(), length: 3, quantity: 1 },
            ],
        }
    }

    #[test]
    fn serde_defaults_fill_in_optional_stock_and_option_fields() {
        let stock: Stock1D =
            serde_json::from_value(json!({ "name": "bar", "length": 12 })).expect("stock");
        assert_eq!(stock.cost, 1.0);
        assert_eq!(stock.kerf, 0);
        assert_eq!(stock.trim, 0);
        assert_eq!(stock.available, None);

        let options: OneDOptions = serde_json::from_value(json!({})).expect("options");
        assert_eq!(options, OneDOptions::default());
    }

    #[test]
    fn validation_rejects_missing_or_invalid_one_d_inputs() {
        let missing_stock = OneDProblem { stock: Vec::new(), demands: sample_problem().demands };
        assert!(matches!(
            missing_stock.validate(),
            Err(BinPackingError::InvalidInput(message))
                if message == "at least one stock entry is required"
        ));

        let missing_demands = OneDProblem { stock: sample_problem().stock, demands: Vec::new() };
        assert!(matches!(
            missing_demands.validate(),
            Err(BinPackingError::InvalidInput(message))
                if message == "at least one demand entry is required"
        ));

        let zero_length_stock = OneDProblem {
            stock: vec![Stock1D {
                name: "bar".to_string(),
                length: 0,
                kerf: 0,
                trim: 0,
                cost: 1.0,
                available: None,
            }],
            demands: vec![CutDemand1D { name: "cut".to_string(), length: 1, quantity: 1 }],
        };
        assert!(matches!(
            zero_length_stock.validate(),
            Err(BinPackingError::InvalidInput(message))
                if message == "stock `bar` must have a positive length"
        ));

        let trimmed_away_stock = OneDProblem {
            stock: vec![Stock1D {
                name: "bar".to_string(),
                length: 5,
                kerf: 0,
                trim: 5,
                cost: 1.0,
                available: None,
            }],
            demands: vec![CutDemand1D { name: "cut".to_string(), length: 1, quantity: 1 }],
        };
        assert!(matches!(
            trimmed_away_stock.validate(),
            Err(BinPackingError::InvalidInput(message))
                if message == "stock `bar` has no usable length after trim"
        ));

        let zero_length_demand = OneDProblem {
            stock: vec![Stock1D {
                name: "bar".to_string(),
                length: 5,
                kerf: 0,
                trim: 0,
                cost: 1.0,
                available: None,
            }],
            demands: vec![CutDemand1D { name: "cut".to_string(), length: 0, quantity: 1 }],
        };
        assert!(matches!(
            zero_length_demand.validate(),
            Err(BinPackingError::InvalidInput(message))
                if message == "demand `cut` must have a positive length"
        ));

        let zero_quantity_demand = OneDProblem {
            stock: vec![Stock1D {
                name: "bar".to_string(),
                length: 5,
                kerf: 0,
                trim: 0,
                cost: 1.0,
                available: None,
            }],
            demands: vec![CutDemand1D { name: "cut".to_string(), length: 1, quantity: 0 }],
        };
        assert!(matches!(
            zero_quantity_demand.validate(),
            Err(BinPackingError::InvalidInput(message))
                if message == "demand `cut` must have a positive quantity"
        ));
    }

    /// Regression test for the `MAX_DIMENSION` validation added to prevent the
    /// exact backend from silently saturating `length * quantity` computations.
    #[test]
    fn validation_rejects_lengths_above_max_dimension() {
        let oversized_stock = OneDProblem {
            stock: vec![Stock1D {
                name: "huge".to_string(),
                length: MAX_DIMENSION + 1,
                kerf: 0,
                trim: 0,
                cost: 1.0,
                available: None,
            }],
            demands: vec![CutDemand1D { name: "cut".to_string(), length: 1, quantity: 1 }],
        };
        assert!(matches!(
            oversized_stock.validate(),
            Err(BinPackingError::InvalidInput(message))
                if message.starts_with("stock `huge` length")
        ));

        let oversized_kerf = OneDProblem {
            stock: vec![Stock1D {
                name: "bar".to_string(),
                length: 100,
                kerf: MAX_DIMENSION + 1,
                trim: 0,
                cost: 1.0,
                available: None,
            }],
            demands: vec![CutDemand1D { name: "cut".to_string(), length: 1, quantity: 1 }],
        };
        assert!(matches!(
            oversized_kerf.validate(),
            Err(BinPackingError::InvalidInput(message))
                if message.starts_with("stock `bar` kerf")
        ));

        let oversized_trim = OneDProblem {
            stock: vec![Stock1D {
                name: "bar".to_string(),
                length: 100,
                kerf: 0,
                trim: MAX_DIMENSION + 1,
                cost: 1.0,
                available: None,
            }],
            demands: vec![CutDemand1D { name: "cut".to_string(), length: 1, quantity: 1 }],
        };
        assert!(matches!(
            oversized_trim.validate(),
            Err(BinPackingError::InvalidInput(message))
                if message.starts_with("stock `bar` trim")
        ));

        let oversized_demand = OneDProblem {
            stock: vec![Stock1D {
                name: "bar".to_string(),
                length: 100,
                kerf: 0,
                trim: 0,
                cost: 1.0,
                available: None,
            }],
            demands: vec![CutDemand1D {
                name: "cut".to_string(),
                length: MAX_DIMENSION + 1,
                quantity: 1,
            }],
        };
        assert!(matches!(
            oversized_demand.validate(),
            Err(BinPackingError::InvalidInput(message))
                if message.starts_with("demand `cut` length")
        ));
    }

    #[test]
    fn feasibility_and_piece_expansion_follow_declared_demands() {
        let feasible = sample_problem();
        feasible.validate().expect("sample input should validate");
        feasible.ensure_feasible_demands().expect("sample input should be feasible");
        assert_eq!(feasible.total_quantity(), 3);

        let pieces = feasible.expanded_pieces();
        assert_eq!(pieces.len(), 3);
        assert_eq!(pieces[0].demand_index, 0);
        assert_eq!(pieces[0].name, "leg");
        assert_eq!(pieces[2].demand_index, 1);
        assert_eq!(pieces[2].name, "brace");

        let infeasible = OneDProblem {
            stock: feasible.stock.clone(),
            demands: vec![CutDemand1D { name: "oversized".to_string(), length: 9, quantity: 1 }],
        };
        assert!(matches!(
            infeasible.ensure_feasible_demands(),
            Err(BinPackingError::Infeasible1D { item, length })
                if item == "oversized" && length == 9
        ));
    }

    #[test]
    fn packed_bin_accounts_for_kerf_and_rejects_overflow() {
        let stock = Stock1D {
            name: "bar".to_string(),
            length: 12,
            kerf: 1,
            trim: 2,
            cost: 1.0,
            available: None,
        };
        assert_eq!(stock.usable_length(), 10);
        assert_eq!(stock.adjusted_capacity(), 11);

        let first = PieceInstance { demand_index: 0, name: "A".to_string(), length: 6 };
        let second = PieceInstance { demand_index: 1, name: "B".to_string(), length: 3 };
        let oversized = PieceInstance { demand_index: 2, name: "C".to_string(), length: 4 };

        let mut bin = PackedBin::new(0);
        assert!(bin.can_fit_piece(&first, &stock));
        assert_eq!(bin.delta_for_piece(&first, &stock), Some(6));
        assert!(bin.add_piece(first.clone(), &stock));
        assert_eq!(bin.used_length(), 6);
        assert_eq!(bin.remaining_length(&stock), 4);

        assert_eq!(bin.delta_for_piece(&second, &stock), Some(4));
        assert!(bin.add_piece(second, &stock));
        assert_eq!(bin.used_length(), 10);
        assert_eq!(bin.remaining_length(&stock), 0);

        assert!(!bin.can_fit_piece(&oversized, &stock));
        assert_eq!(bin.delta_for_piece(&oversized, &stock), None);
        assert!(!bin.add_piece(oversized, &stock));
    }

    #[test]
    fn solution_helpers_sort_layouts_and_prefer_exact_ties() {
        let stock = vec![
            Stock1D {
                name: "slow".to_string(),
                length: 10,
                kerf: 0,
                trim: 0,
                cost: 2.0,
                available: None,
            },
            Stock1D {
                name: "fast".to_string(),
                length: 10,
                kerf: 0,
                trim: 0,
                cost: 1.0,
                available: None,
            },
        ];

        let bins = vec![
            PackedBin {
                stock_index: 0,
                pieces: vec![PieceInstance {
                    demand_index: 0,
                    name: "small".to_string(),
                    length: 3,
                }],
                occupied_length: 3,
            },
            PackedBin {
                stock_index: 1,
                pieces: vec![
                    PieceInstance { demand_index: 1, name: "large".to_string(), length: 6 },
                    PieceInstance { demand_index: 2, name: "medium".to_string(), length: 2 },
                ],
                occupied_length: 8,
            },
        ];
        let unplaced = vec![
            PieceInstance { demand_index: 0, name: "tiny".to_string(), length: 1 },
            PieceInstance { demand_index: 1, name: "big".to_string(), length: 7 },
        ];
        let metrics = SolverMetrics1D {
            iterations: 2,
            generated_patterns: 0,
            enumerated_patterns: 0,
            explored_states: 0,
            notes: vec!["test".to_string()],
        };

        let exact = OneDSolution::from_bins(
            "column_generation",
            true,
            Some(2.0),
            &stock,
            &bins,
            &unplaced,
            metrics.clone(),
        );
        assert_eq!(exact.layouts[0].stock_name, "fast");
        assert_eq!(exact.layouts[1].stock_name, "slow");
        assert_eq!(exact.unplaced[0].name, "big");
        assert_eq!(exact.total_cost, 3.0);
        assert_eq!(exact.total_waste, 9);
        assert_eq!(exact.stock_requirements.len(), 2);
        assert_eq!(exact.stock_requirements[0].stock_name, "slow");
        assert_eq!(exact.stock_requirements[0].used_quantity, 1);
        assert_eq!(exact.stock_requirements[0].required_quantity, 1);
        assert_eq!(exact.stock_requirements[1].stock_name, "fast");
        assert_eq!(exact.stock_requirements[1].used_quantity, 1);
        assert_eq!(exact.stock_requirements[1].additional_quantity_needed, 0);

        let heuristic = OneDSolution { exact: false, ..exact.clone() };
        assert!(exact.is_better_than(&heuristic));
    }

    #[test]
    fn stock_requirements_can_record_shortfalls_against_inventory() {
        let stock = vec![Stock1D {
            name: "bar".to_string(),
            length: 10,
            kerf: 0,
            trim: 0,
            cost: 1.0,
            available: Some(1),
        }];
        let bins = vec![PackedBin {
            stock_index: 0,
            pieces: vec![
                PieceInstance { demand_index: 0, name: "A".to_string(), length: 5 },
                PieceInstance { demand_index: 1, name: "B".to_string(), length: 5 },
            ],
            occupied_length: 10,
        }];
        let mut solution = OneDSolution::from_bins(
            "first_fit_decreasing",
            false,
            None,
            &stock,
            &bins,
            &[],
            SolverMetrics1D {
                iterations: 1,
                generated_patterns: 0,
                enumerated_patterns: 0,
                explored_states: 0,
                notes: Vec::new(),
            },
        );

        solution.set_required_stock_counts(&stock, &[2]);

        assert_eq!(solution.stock_requirements.len(), 1);
        assert_eq!(solution.stock_requirements[0].available_quantity, Some(1));
        assert_eq!(solution.stock_requirements[0].used_quantity, 1);
        assert_eq!(solution.stock_requirements[0].required_quantity, 2);
        assert_eq!(solution.stock_requirements[0].additional_quantity_needed, 1);
    }

    /// `OneDSolution::is_better_than` compares on a 5-key tuple:
    /// (unplaced.len, stock_count, total_waste, total_cost, !exact).
    /// The existing `solution_helpers_sort_layouts_and_prefer_exact_ties`
    /// test covers the last key; this one pins down the other four.
    #[test]
    fn one_d_is_better_than_tie_breaks_on_each_key() {
        let stock = vec![Stock1D {
            name: "bar".to_string(),
            length: 10,
            kerf: 0,
            trim: 0,
            cost: 1.0,
            available: None,
        }];
        let bins = vec![PackedBin {
            stock_index: 0,
            pieces: vec![PieceInstance { demand_index: 0, name: "cut".to_string(), length: 5 }],
            occupied_length: 5,
        }];
        let base = OneDSolution::from_bins(
            "test",
            false,
            None,
            &stock,
            &bins,
            &[],
            SolverMetrics1D {
                iterations: 0,
                generated_patterns: 0,
                enumerated_patterns: 0,
                explored_states: 0,
                notes: Vec::new(),
            },
        );

        // Fewer unplaced wins (primary key).
        let more_unplaced = OneDSolution {
            unplaced: vec![CutAssignment1D { name: "u".to_string(), length: 1 }],
            ..base.clone()
        };
        assert!(base.is_better_than(&more_unplaced));
        assert!(!more_unplaced.is_better_than(&base));

        // Fewer stock wins when unplaced ties.
        let more_stock = OneDSolution { stock_count: base.stock_count + 1, ..base.clone() };
        assert!(base.is_better_than(&more_stock));

        // Less waste wins when unplaced and stock_count tie.
        let more_waste = OneDSolution { total_waste: base.total_waste + 10, ..base.clone() };
        assert!(base.is_better_than(&more_waste));

        // Lower cost wins when every preceding key ties.
        let more_cost = OneDSolution { total_cost: base.total_cost + 1.0, ..base.clone() };
        assert!(base.is_better_than(&more_cost));

        // Identical solutions are not "better than" each other.
        assert!(!base.is_better_than(&base));
    }
}
