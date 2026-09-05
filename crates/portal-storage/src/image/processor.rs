//! Image validation and processing.

use super::config::ImageConfig;
use super::error::ImageError;
use bytes::Bytes;
use image::imageops::FilterType;
use image::{DynamicImage, GenericImageView, ImageReader, Limits};
use std::io::Cursor;
use tracing::instrument;

/// Cap on bytes the `image` crate may allocate while decoding a single image.
///
/// A small zlib-compressed PNG can decode to a multi-gigapixel buffer (a
/// "decompression bomb"). Without an explicit limit, the dimension check
/// downstream fires *after* the allocation, which is too late. 256 MiB
/// comfortably covers any legitimate avatar/banner upload (2048×2048 RGBA =
/// 16 MiB) while keeping a single hostile request from blowing through the
/// host's RAM.
const MAX_DECODE_ALLOC_BYTES: u64 = 256 * 1024 * 1024;

/// Hard cap on input image dimensions before decode. We re-validate against
/// the per-image-type config after decode; this is a coarse safety net that
/// fires before the pixel buffer is materialized.
const MAX_DECODE_WIDTH: u32 = 16_384;
const MAX_DECODE_HEIGHT: u32 = 16_384;

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
        infer::get(data).and_then(|kind| match kind.mime_type() {
            "image/png" => Some("image/png"),
            "image/jpeg" => Some("image/jpeg"),
            "image/webp" => Some("image/webp"),
            "image/gif" => Some("image/gif"),
            _ => None,
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

        // 4. Decode image, with explicit allocation/dimension limits so a
        //    decompression bomb is rejected before its pixel buffer is
        //    materialized.
        let mut limits = Limits::default();
        limits.max_alloc = Some(MAX_DECODE_ALLOC_BYTES);
        limits.max_image_width = Some(MAX_DECODE_WIDTH);
        limits.max_image_height = Some(MAX_DECODE_HEIGHT);

        let mut reader = ImageReader::new(Cursor::new(data))
            .with_guessed_format()
            .map_err(|e| ImageError::decoding_failed(e.to_string()))?;
        reader.limits(limits);
        let img = reader
            .decode()
            .map_err(|e| ImageError::decoding_failed(e.to_string()))?;

        let (width, height) = img.dimensions();

        // 5. Validate dimensions
        Self::validate_dimensions(width, height, config)?;

        // 6. Fit the image to the target shape. With a resize target the
        //    image is centre-cropped to that shape — a 16:9 photo becomes a
        //    4:1 banner by losing its top and bottom, never by being refused
        //    or squashed. Only a configuration with a ratio band but no
        //    target still rejects, because there is nothing to crop to.
        let img = if let Some((target_w, target_h)) = config.resize_to {
            Self::center_crop_to_aspect(&img, target_w, target_h)
        } else if let Some((min_ratio, max_ratio)) = config.aspect_ratio_range {
            let ratio = width as f32 / height as f32;
            if ratio < min_ratio || ratio > max_ratio {
                return Err(ImageError::InvalidAspectRatio {
                    ratio,
                    min: min_ratio,
                    max: max_ratio,
                });
            }
            img
        } else {
            img
        };
        let (width, height) = img.dimensions();

        // 7. Resize if configured (the crop above made the shape match, so
        //    this is a pure scale)
        let (processed_img, final_dims) = if let Some((target_w, target_h)) = config.resize_to {
            let resized = img.resize_exact(target_w, target_h, FilterType::Lanczos3);
            (resized, (target_w, target_h))
        } else {
            (img.clone(), (width, height))
        };

        // 8. Encode to output format
        let main = Self::encode_image(&processed_img, config)?;

        // 9. Generate thumbnail if configured, from the cropped image so it
        //    shows the same framing as the main image
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

    const fn validate_dimensions(
        width: u32,
        height: u32,
        config: &ImageConfig,
    ) -> Result<(), ImageError> {
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

    /// The largest centred sub-rectangle of `img` with the aspect ratio
    /// `target_w:target_h`. An image already at that ratio comes back whole.
    pub(crate) fn center_crop_to_aspect(
        img: &DynamicImage,
        target_w: u32,
        target_h: u32,
    ) -> DynamicImage {
        let (w, h) = img.dimensions();
        if w == 0 || h == 0 || target_w == 0 || target_h == 0 {
            return img.clone();
        }
        // Compare w/h against target_w/target_h without floats:
        // w * target_h > h * target_w  <=>  image is wider than the target.
        let (crop_w, crop_h) =
            if u64::from(w) * u64::from(target_h) > u64::from(h) * u64::from(target_w) {
                // Too wide: keep full height, narrow the width.
                let cw = u64::from(h) * u64::from(target_w) / u64::from(target_h);
                (u32::try_from(cw).unwrap_or(w).clamp(1, w), h)
            } else {
                // Too tall (or exact): keep full width, shorten the height.
                let ch = u64::from(w) * u64::from(target_h) / u64::from(target_w);
                (w, u32::try_from(ch).unwrap_or(h).clamp(1, h))
            };
        if (crop_w, crop_h) == (w, h) {
            return img.clone();
        }
        let x = (w - crop_w) / 2;
        let y = (h - crop_h) / 2;
        img.crop_imm(x, y, crop_w, crop_h)
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
                let encoder =
                    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buffer, config.quality);
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
        assert_eq!(
            ImageProcessor::detect_mime_type(&png_data),
            Some("image/png")
        );
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
    fn off_ratio_images_are_cropped_to_the_target_not_refused() {
        // A 2:1 image for a square avatar: cropped to the middle square, then
        // scaled to the avatar target. Before, this was refused outright —
        // which is why no player banner (a 4:1 target fed 16:9 photos) ever
        // saved.
        let png_data = create_test_png(400, 200);
        let config = ImageConfig::player_avatar();
        let processed = ImageProcessor::process(&png_data, &config).expect("cropped, not refused");
        assert_eq!(processed.dimensions, config.resize_to.unwrap());

        // A 16:9 photo for a 4:1 player banner.
        let png_data = create_test_png(1920, 1080);
        let config = ImageConfig::player_banner();
        let processed = ImageProcessor::process(&png_data, &config).expect("cropped, not refused");
        assert_eq!(processed.dimensions, (1200, 300));
    }

    #[test]
    fn center_crop_keeps_the_middle() {
        use image::{DynamicImage, GenericImageView, ImageBuffer, Rgba};
        // Left half red, right half blue; cropping 400x100 to 1:1 keeps the
        // middle 100x100, which straddles the seam: 50 red, 50 blue columns.
        let img: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::from_fn(400, 100, |x, _| {
            if x < 200 {
                Rgba([255, 0, 0, 255])
            } else {
                Rgba([0, 0, 255, 255])
            }
        });
        let cropped = ImageProcessor::center_crop_to_aspect(&DynamicImage::ImageRgba8(img), 1, 1);
        assert_eq!(cropped.dimensions(), (100, 100));
        assert_eq!(cropped.get_pixel(0, 0), Rgba([255, 0, 0, 255]));
        assert_eq!(cropped.get_pixel(99, 0), Rgba([0, 0, 255, 255]));
    }

    #[test]
    fn a_ratio_band_without_a_target_still_rejects() {
        let mut config = ImageConfig::player_avatar();
        config.resize_to = None;
        let png_data = create_test_png(200, 100);
        let result = ImageProcessor::process(&png_data, &config);
        assert!(matches!(result, Err(ImageError::InvalidAspectRatio { .. })));
    }
}
