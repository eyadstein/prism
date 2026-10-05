//! Error type shared by every Prism crate.

use thiserror::Error;

/// Everything that can go wrong inside the engine.
#[derive(Debug, Error, PartialEq)]
pub enum PrismError {
    /// A wavelength fell outside the supported visible range.
    #[error("wavelength {0} nm is outside the supported range")]
    WavelengthOutOfRange(f64),

    /// A caller supplied a value that makes no physical or numerical sense.
    #[error("invalid parameter `{name}`: {reason}")]
    InvalidParameter {
        /// Name of the offending parameter.
        name: &'static str,
        /// Human readable explanation.
        reason: String,
    },
}

/// Convenience alias used across the workspace.
pub type Result<T> = core::result::Result<T, PrismError>;
