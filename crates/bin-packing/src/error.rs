use thiserror::Error;

/// Errors returned by the bin-packing solvers.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum BinPackingError {
    /// The problem failed input validation (e.g. empty demand list, zero dimension, invalid cost).
    #[error("invalid input: {0}")]
    InvalidInput(String),
    /// The requested solver configuration is not currently supported.
    #[error("unsupported configuration: {0}")]
    Unsupported(String),
    /// A 1D demand cannot fit any declared stock entry.
    #[error("no feasible stock can fit demand `{item}` with length {length}")]
    Infeasible1D {
        /// Name of the demand that could not be placed.
        item: String,
        /// Length of the demand that triggered the infeasibility.
        length: u32,
    },
    /// A 2D demand cannot fit any declared sheet entry, even with rotation.
    #[error("no feasible sheet can fit item `{item}` with size {width}x{height}")]
    Infeasible2D {
        /// Name of the demand that could not be placed.
        item: String,
        /// Width of the demand that triggered the infeasibility.
        width: u32,
        /// Height of the demand that triggered the infeasibility.
        height: u32,
    },
    /// A 3D demand cannot fit any declared bin entry, even with rotation.
    #[error("no feasible bin can fit item `{item}` with size {width}x{height}x{depth}")]
    Infeasible3D {
        /// Name of the demand that could not be placed.
        item: String,
        /// Width of the demand that triggered the infeasibility.
        width: u32,
        /// Height of the demand that triggered the infeasibility.
        height: u32,
        /// Depth of the demand that triggered the infeasibility.
        depth: u32,
    },
}

/// Convenient `Result` alias that uses [`BinPackingError`] as the error type.
pub type Result<T> = std::result::Result<T, BinPackingError>;
