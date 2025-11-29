//! Image validation and processing errors.

use thiserror::Error;

/// Errors that can occur during image validation and processing.
#[derive(Error, Debug)]
pub enum ImageError {
    /// Image exceeds maximum allowed size.
    #[error("Image too large: {size} bytes exceeds maximum of {max} bytes")]
    TooLarge {
        /// Actual size in bytes.
        size: usize,
        /// Maximum allowed size.
        max: usize,
    },

    /// Image dimensions are too small.
    #[error("Image too small: {width}x{height} is smaller than minimum {min_width}x{min_height}")]
    TooSmall {
        /// Actual width.
        width: u32,
        /// Actual height.
        height: u32,
        /// Minimum width.
        min_width: u32,
        /// Minimum height.
        min_height: u32,
    },

    /// Image dimensions are too large.
    #[error("Image too large: {width}x{height} exceeds maximum {max_width}x{max_height}")]
    DimensionsTooLarge {
        /// Actual width.
        width: u32,
        /// Actual height.
        height: u32,
        /// Maximum width.
        max_width: u32,
        /// Maximum height.
        max_height: u32,
    },

    /// Image aspect ratio is outside allowed range.
    #[error("Invalid aspect ratio: {ratio:.2} is outside allowed range {min:.2} to {max:.2}")]
    InvalidAspectRatio {
        /// Actual aspect ratio.
        ratio: f32,
        /// Minimum allowed aspect ratio.
        min: f32,
        /// Maximum allowed aspect ratio.
        max: f32,
    },

    /// Unsupported image format.
    #[error("Unsupported image format: {format}")]
    UnsupportedFormat {
        /// Detected format.
        format: String,
    },

    /// Could not detect image format from content.
    #[error("Could not detect image format")]
    UnknownFormat,

    /// Image decoding failed.
    #[error("Failed to decode image: {0}")]
    DecodingFailed(String),

    /// Image encoding failed.
    #[error("Failed to encode image: {0}")]
    EncodingFailed(String),
}

impl ImageError {
    /// Create an unsupported format error.
    #[must_use]
    pub fn unsupported_format(format: impl Into<String>) -> Self {
        Self::UnsupportedFormat {
            format: format.into(),
        }
    }

    /// Create a decoding failed error.
    #[must_use]
    pub fn decoding_failed(message: impl Into<String>) -> Self {
        Self::DecodingFailed(message.into())
    }

    /// Create an encoding failed error.
    #[must_use]
    pub fn encoding_failed(message: impl Into<String>) -> Self {
        Self::EncodingFailed(message.into())
    }
}
