//! Cut list optimization and bin packing for 1D (linear stock), 2D (sheet
//! stock), and 3D (box) problems.
//!
//! The crate exposes three shipping entry points by default:
//!
//! - [`one_d::solve_1d`] — cutting stock / linear bin packing. Supports first-fit
//!   decreasing, best-fit decreasing, multistart local search, and an exact
//!   column-generation backend that reports an LP lower bound.
//! - [`two_d::solve_2d`] — rectangular sheet packing. Supports the MaxRects family
//!   (best-area, best-short-side-fit, best-long-side-fit, bottom-left, contact-point),
//!   the Skyline family (default and minimum-waste), Guillotine beam search with
//!   configurable ranking and split rules (best-short-side-fit, best-long-side-fit,
//!   shorter- and longer-leftover-axis, min- and max-area splits), shelf heuristics
//!   (NFDH, FFDH, BFDH), and a multistart meta-strategy.
//! - [`three_d::solve_3d`] — rectangular box packing. Supports Extreme Points,
//!   Guillotine 3D, layer/wall/column builders, DBLF, volume-sorted baselines,
//!   randomized meta-strategies, and a restricted exact backend.
//!
//! The `three-d` feature (enabled by default) provides the 3D rectangular box
//! packing module with a 29-algorithm catalog. The legacy `three-d-preview`
//! alias remains available as a compatibility shim.
//!
//! The 1D and 2D entry points ship an `Auto` mode that runs multiple strategies and
//! returns the best candidate, ranked lexicographically by unplaced count, stock /
//! sheet count, waste, and cost (with `exact` as a secondary tiebreaker for 1D).
//!
//! Additional capabilities:
//!
//! - Multiple stock / sheet types per problem, each with its own cost and optional
//!   inventory cap. When 1D inventory caps are present, [`one_d::solve_1d`] also
//!   reports a relaxed-inventory procurement estimate per stock type.
//! - Per-item rotation control for 2D demands, plus a `guillotine_required` flag
//!   that restricts the solver to guillotine-compatible layouts.
//! - Kerf and trim modeling for 1D cuts so layouts match physical cut lists.
//! - Reproducible randomized search via an optional `seed`.
//! - Automatic multi-core parallelism via [`rayon`] when the `parallel` feature
//!   is enabled (on by default). Auto-mode solvers dispatch algorithms in
//!   parallel and multi-start / GRASP / local-search meta-strategies run their
//!   iterations concurrently. Falls back to sequential execution on single-core
//!   hosts or when the feature is disabled.
//! - Structured [`BinPackingError`] variants for input validation, infeasible
//!   demands (1D, 2D, and 3D), and unsupported solver configurations.
//! - `metrics` blocks with iteration counts, explored states, and diagnostic notes.
//!
//! All problem, option, solution, and metrics types derive [`serde::Serialize`] and
//! [`serde::Deserialize`] so they can flow through JSON APIs or other wire formats
//! without wrapping.
//!
//! # Example: 1D cutting stock
//!
//! ```no_run
//! # #[cfg(feature = "one-d")]
//! # {
//! use bin_packing::one_d::{CutDemand1D, OneDOptions, OneDProblem, Stock1D, solve_1d};
//!
//! let problem = OneDProblem {
//!     stock: vec![Stock1D {
//!         name: "bar".into(),
//!         length: 96,
//!         kerf: 1,
//!         trim: 0,
//!         cost: 1.0,
//!         available: None,
//!     }],
//!     demands: vec![
//!         CutDemand1D { name: "rail".into(), length: 45, quantity: 2 },
//!         CutDemand1D { name: "brace".into(), length: 30, quantity: 2 },
//!     ],
//! };
//! let solution = solve_1d(problem, OneDOptions::default())?;
//! println!("stock used: {}", solution.stock_count);
//! # Ok::<(), bin_packing::BinPackingError>(())
//! # }
//! # #[cfg(not(feature = "one-d"))]
//! # Ok::<(), bin_packing::BinPackingError>(())
//! ```
//!
//! # Example: 2D rectangular packing
//!
//! ```no_run
//! # #[cfg(feature = "two-d")]
//! # {
//! use bin_packing::two_d::{RectDemand2D, Sheet2D, TwoDOptions, TwoDProblem, solve_2d};
//!
//! let problem = TwoDProblem {
//!     sheets: vec![Sheet2D {
//!         name: "plywood".into(),
//!         width: 96,
//!         height: 48,
//!         cost: 1.0,
//!         quantity: None,
//!         kerf: 0,
//!         edge_kerf_relief: false,
//!     }],
//!     demands: vec![RectDemand2D {
//!         name: "panel".into(),
//!         width: 24,
//!         height: 18,
//!         quantity: 4,
//!         can_rotate: true,
//!     }],
//! };
//! let solution = solve_2d(problem, TwoDOptions::default())?;
//! println!("sheets used: {}", solution.sheet_count);
//! # Ok::<(), bin_packing::BinPackingError>(())
//! # }
//! # #[cfg(not(feature = "two-d"))]
//! # Ok::<(), bin_packing::BinPackingError>(())
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]

pub mod cut_plan;
mod error;
#[cfg(feature = "one-d")]
pub mod one_d;
#[cfg_attr(not(feature = "parallel"), allow(dead_code))]
mod parallel;
#[cfg(feature = "two-d")]
pub mod two_d;

#[cfg(feature = "three-d")]
pub mod three_d;

pub use cut_plan::CutPlanError;
pub use error::{BinPackingError, Result};
