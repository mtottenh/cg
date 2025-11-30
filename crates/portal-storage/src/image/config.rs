//! Image configuration for different image types.

use image::ImageFormat;

/// Configuration for image validation and processing.
#[derive(Debug, Clone)]
pub struct ImageConfig {
    /// Maximum file size in bytes.
    pub max_size_bytes: usize,
    /// Minimum dimensions (width, height).
    pub min_dimensions: (u32, u32),
    /// Maximum dimensions (width, height).
    pub max_dimensions: (u32, u32),
    /// Allowed aspect ratio range (min, max). None means any aspect ratio.
    pub aspect_ratio_range: Option<(f32, f32)>,
    /// Target dimensions to resize to. None means no resizing.
    pub resize_to: Option<(u32, u32)>,
    /// Thumbnail dimensions. None means no thumbnail.
    pub thumbnail_size: Option<(u32, u32)>,
    /// Allowed MIME types.
    pub allowed_types: Vec<&'static str>,
    /// Output format for processed images.
    pub output_format: ImageFormat,
    /// Quality for output encoding (0-100).
    pub quality: u8,
}

impl ImageConfig {
    /// Configuration for team logos.
    ///
    /// - Square aspect ratio (1:1)
    /// - Maximum 5MB
    /// - Resized to 512x512
    /// - WebP output
    #[must_use]
    pub fn team_logo() -> Self {
        Self {
            max_size_bytes: 5 * 1024 * 1024, // 5MB
            min_dimensions: (64, 64),
            max_dimensions: (4096, 4096),
            aspect_ratio_range: Some((0.9, 1.1)), // Nearly square
            resize_to: Some((512, 512)),
            thumbnail_size: Some((128, 128)),
            allowed_types: vec!["image/png", "image/jpeg", "image/webp", "image/gif"],
            output_format: ImageFormat::WebP,
            quality: 85,
        }
    }

    /// Configuration for team banners.
    ///
    /// - Wide aspect ratio (4:1)
    /// - Maximum 10MB
    /// - Resized to 1920x480
    /// - WebP output
    #[must_use]
    pub fn team_banner() -> Self {
        Self {
            max_size_bytes: 10 * 1024 * 1024, // 10MB
            min_dimensions: (480, 120),
            max_dimensions: (8192, 2048),
            aspect_ratio_range: Some((3.5, 4.5)), // Approximately 4:1
            resize_to: Some((1920, 480)),
            thumbnail_size: Some((480, 120)),
            allowed_types: vec!["image/png", "image/jpeg", "image/webp"],
            output_format: ImageFormat::WebP,
            quality: 85,
        }
    }

    /// Configuration for player avatars.
    ///
    /// - Square aspect ratio (1:1)
    /// - Maximum 2MB
    /// - Resized to 256x256
    /// - WebP output
    #[must_use]
    pub fn player_avatar() -> Self {
        Self {
            max_size_bytes: 2 * 1024 * 1024, // 2MB
            min_dimensions: (32, 32),
            max_dimensions: (2048, 2048),
            aspect_ratio_range: Some((0.9, 1.1)), // Nearly square
            resize_to: Some((256, 256)),
            thumbnail_size: Some((64, 64)),
            allowed_types: vec!["image/png", "image/jpeg", "image/webp", "image/gif"],
            output_format: ImageFormat::WebP,
            quality: 85,
        }
    }

    /// Configuration for player banners.
    ///
    /// - Wide aspect ratio (4:1)
    /// - Maximum 5MB
    /// - Resized to 1200x300
    /// - WebP output
    #[must_use]
    pub fn player_banner() -> Self {
        Self {
            max_size_bytes: 5 * 1024 * 1024, // 5MB
            min_dimensions: (400, 100),
            max_dimensions: (4800, 1200),
            aspect_ratio_range: Some((3.5, 4.5)), // Approximately 4:1
            resize_to: Some((1200, 300)),
            thumbnail_size: Some((400, 100)),
            allowed_types: vec!["image/png", "image/jpeg", "image/webp"],
            output_format: ImageFormat::WebP,
            quality: 85,
        }
    }
}

/// Image type enum for selecting configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageType {
    /// Team logo (square).
    TeamLogo,
    /// Team banner (wide).
    TeamBanner,
    /// Player avatar (square).
    PlayerAvatar,
    /// Player banner (wide).
    PlayerBanner,
}

impl ImageType {
    /// Get the configuration for this image type.
    #[must_use]
    pub fn config(&self) -> ImageConfig {
        match self {
            Self::TeamLogo => ImageConfig::team_logo(),
            Self::TeamBanner => ImageConfig::team_banner(),
            Self::PlayerAvatar => ImageConfig::player_avatar(),
            Self::PlayerBanner => ImageConfig::player_banner(),
        }
    }

    /// Get the storage path prefix for this image type.
    #[must_use]
    pub const fn prefix(&self) -> &'static str {
        match self {
            Self::TeamLogo => "teams/logos",
            Self::TeamBanner => "teams/banners",
            Self::PlayerAvatar => "players/avatars",
            Self::PlayerBanner => "players/banners",
        }
    }

    /// Get the MIME type for the output format.
    #[must_use]
    pub const fn output_content_type(&self) -> &'static str {
        "image/webp"
    }
}
