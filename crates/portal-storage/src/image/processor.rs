//! Image validation and processing.

use super::config::ImageConfig;
use super::error::ImageError;
use bytes::Bytes;
use image::imageops::FilterType;
use image::{DynamicImage, GenericImageView, ImageReader};
use std::io::Cursor;
use tracing::instrument;

/// Result of image processing.
#[derive(Debug)]
pub struct ProcessedImage {
    /// Main processed image data.
    pub main: Bytes,
    /// Optional thumbnail data.
    pub thumbnail: Option<Bytes>,
    /// Output MIME content type.
    pub content_type: String,
    /// Final dimensions (width, height).
    pub dimensions: (u32, u32),
}

/// Image processor for validation and transformation.
pub struct ImageProcessor;

impl ImageProcessor {
    /// Detect the MIME type of image data using magic bytes.
    #[must_use]
    pub fn detect_mime_type(data: &[u8]) -> Option<&'static str> {
        infer::get(data).and_then(|kind| {
            match kind.mime_type() {
                "image/png" => Some("image/png"),
                "image/jpeg" => Some("image/jpeg"),
                "image/webp" => Some("image/webp"),
                "image/gif" => Some("image/gif"),
                _ => None,
            }
        })
    }

    /// Validate and process an image according to the given configuration.
    ///
    /// This performs:
    /// 1. Size validation
    /// 2. Format detection via magic bytes
    /// 3. Format validation
    /// 4. Dimension validation
    /// 5. Aspect ratio validation (if configured)
    /// 6. Resize (if configured)
    /// 7. Re-encode to output format
    /// 8. Generate thumbnail (if configured)
    #[instrument(skip(data, config))]
    pub fn process(data: &[u8], config: &ImageConfig) -> Result<ProcessedImage, ImageError> {
        // 1. Check file size
        if data.len() > config.max_size_bytes {
            return Err(ImageError::TooLarge {
                size: data.len(),
                max: config.max_size_bytes,
            });
        }

        // 2. Detect format via magic bytes
        let detected_mime = Self::detect_mime_type(data).ok_or(ImageError::UnknownFormat)?;

        // 3. Validate format
        if !config.allowed_types.contains(&detected_mime) {
            return Err(ImageError::unsupported_format(detected_mime));
        }

        // 4. Decode image
        let img = ImageReader::new(Cursor::new(data))
            .with_guessed_format()
            .map_err(|e| ImageError::decoding_failed(e.to_string()))?
            .decode()
            .map_err(|e| ImageError::decoding_failed(e.to_string()))?;

        let (width, height) = img.dimensions();

        // 5. Validate dimensions
        Self::validate_dimensions(width, height, config)?;

        // 6. Validate aspect ratio
        if let Some((min_ratio, max_ratio)) = config.aspect_ratio_range {
            let ratio = width as f32 / height as f32;
            if ratio < min_ratio || ratio > max_ratio {
                return Err(ImageError::InvalidAspectRatio {
                    ratio,
                    min: min_ratio,
                    max: max_ratio,
                });
            }
        }

        // 7. Resize if configured
        let (processed_img, final_dims) = if let Some((target_w, target_h)) = config.resize_to {
            let resized = img.resize_exact(target_w, target_h, FilterType::Lanczos3);
            (resized, (target_w, target_h))
        } else {
            (img.clone(), (width, height))
        };

        // 8. Encode to output format
        let main = Self::encode_image(&processed_img, config)?;

        // 9. Generate thumbnail if configured
        let thumbnail = if let Some((thumb_w, thumb_h)) = config.thumbnail_size {
            let thumb = img.resize_exact(thumb_w, thumb_h, FilterType::Lanczos3);
            Some(Self::encode_image(&thumb, config)?)
        } else {
            None
        };

        let content_type = match config.output_format {
            image::ImageFormat::WebP => "image/webp",
            image::ImageFormat::Png => "image/png",
            image::ImageFormat::Jpeg => "image/jpeg",
            _ => "application/octet-stream",
        };

        tracing::debug!(
            original_dims = ?( width, height),
            final_dims = ?final_dims,
            output_size = main.len(),
            "Image processed successfully"
        );

        Ok(ProcessedImage {
            main,
            thumbnail,
            content_type: content_type.to_string(),
            dimensions: final_dims,
        })
    }

    fn validate_dimensions(width: u32, height: u32, config: &ImageConfig) -> Result<(), ImageError> {
        let (min_w, min_h) = config.min_dimensions;
        let (max_w, max_h) = config.max_dimensions;

        if width < min_w || height < min_h {
            return Err(ImageError::TooSmall {
                width,
                height,
                min_width: min_w,
                min_height: min_h,
            });
        }

        if width > max_w || height > max_h {
            return Err(ImageError::DimensionsTooLarge {
                width,
                height,
                max_width: max_w,
                max_height: max_h,
            });
        }

        Ok(())
    }

    fn encode_image(img: &DynamicImage, config: &ImageConfig) -> Result<Bytes, ImageError> {
        let mut buffer = Cursor::new(Vec::new());

        match config.output_format {
            image::ImageFormat::WebP => {
                // WebP encoding via image crate
                img.write_to(&mut buffer, config.output_format)
                    .map_err(|e| ImageError::encoding_failed(e.to_string()))?;
            }
            image::ImageFormat::Png => {
                img.write_to(&mut buffer, image::ImageFormat::Png)
                    .map_err(|e| ImageError::encoding_failed(e.to_string()))?;
            }
            image::ImageFormat::Jpeg => {
                let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(
                    &mut buffer,
                    config.quality,
                );
                img.write_with_encoder(encoder)
                    .map_err(|e| ImageError::encoding_failed(e.to_string()))?;
            }
            _ => {
                return Err(ImageError::encoding_failed(format!(
                    "Unsupported output format: {:?}",
                    config.output_format
                )));
            }
        }

        Ok(Bytes::from(buffer.into_inner()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image::config::ImageConfig;

    // Create a simple 100x100 PNG in memory for testing
    fn create_test_png(width: u32, height: u32) -> Vec<u8> {
        use image::{ImageBuffer, Rgba};

        let img: ImageBuffer<Rgba<u8>, Vec<u8>> =
            ImageBuffer::from_fn(width, height, |_x, _y| Rgba([255, 0, 0, 255]));

        let mut buffer = Cursor::new(Vec::new());
        img.write_to(&mut buffer, image::ImageFormat::Png).unwrap();
        buffer.into_inner()
    }

    #[test]
    fn test_detect_png() {
        let png_data = create_test_png(100, 100);
        assert_eq!(ImageProcessor::detect_mime_type(&png_data), Some("image/png"));
    }

    #[test]
    fn test_process_valid_avatar() {
        let png_data = create_test_png(256, 256);
        let config = ImageConfig::player_avatar();

        let result = ImageProcessor::process(&png_data, &config).unwrap();

        assert_eq!(result.dimensions, (256, 256));
        assert_eq!(result.content_type, "image/webp");
        assert!(result.thumbnail.is_some());
    }

    #[test]
    fn test_process_resize() {
        let png_data = create_test_png(512, 512);
        let config = ImageConfig::player_avatar();

        let result = ImageProcessor::process(&png_data, &config).unwrap();

        // Should be resized to 256x256
        assert_eq!(result.dimensions, (256, 256));
    }

    #[test]
    fn test_too_small() {
        let png_data = create_test_png(16, 16);
        let config = ImageConfig::player_avatar();

        let result = ImageProcessor::process(&png_data, &config);

        assert!(matches!(result, Err(ImageError::TooSmall { .. })));
    }

    #[test]
    fn test_invalid_aspect_ratio() {
        // Create a 200x100 image (2:1 ratio) for an avatar that expects 1:1
        let png_data = create_test_png(200, 100);
        let config = ImageConfig::player_avatar();

        let result = ImageProcessor::process(&png_data, &config);

        assert!(matches!(result, Err(ImageError::InvalidAspectRatio { .. })));
    }
}
