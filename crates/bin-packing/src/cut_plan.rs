//! Cut sequencing and step generation for finished layouts.
//!
//! Cut planning is a post-processor on solver output: given an
//! [`OneDSolution`](crate::one_d::OneDSolution) or
//! [`TwoDSolution`](crate::two_d::TwoDSolution), produce an ordered
//! sequence of cuts that a shop operator can execute, scored against a
//! preset cost model.
//!
//! Entry points:
//! - [`crate::one_d::cut_plan::plan_cuts`]
//! - [`crate::two_d::cut_plan::plan_cuts`]

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Errors reported by the cut planner.
///
/// Distinct from [`BinPackingError`](crate::BinPackingError) so that
/// callers using the solver but not the cut planner are unaffected.
#[derive(Debug, Clone, PartialEq, Eq, Error, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum CutPlanError {
    /// The caller selected a table-saw or panel-saw preset but the
    /// layout is not guillotine-compatible (a single-blade machine
    /// cannot produce it).
    #[error(
        "layout on sheet `{sheet_name}` is not guillotine-compatible and cannot be cut on a single-blade machine"
    )]
    NonGuillotineNotCuttable {
        /// Name of the offending sheet layout.
        sheet_name: String,
    },
    /// The caller supplied invalid cost overrides (negative value, NaN,
    /// or infinity) or a mismatched configuration.
    #[error("invalid cut-plan options: {0}")]
    InvalidOptions(String),
}

/// `Result` alias for cut-plan operations.
pub type Result<T> = core::result::Result<T, CutPlanError>;
