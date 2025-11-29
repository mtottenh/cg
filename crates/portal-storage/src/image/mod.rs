//! Image validation and processing module.
//!
//! Provides configuration-based image validation and processing for
//! different image types (avatars, logos, banners).

pub mod config;
pub mod error;
pub mod processor;

pub use config::{ImageConfig, ImageType};
pub use error::ImageError;
pub use processor::{ImageProcessor, ProcessedImage};
