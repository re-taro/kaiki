mod antialias;
mod pixel;
pub mod regions;
mod render;

pub use regions::BoundingBox;

use thiserror::Error;
use xxhash_rust::xxh3::xxh3_64;

/// RGBA image with pixel data stored as a flat `Vec<u8>` (4 bytes per pixel).
#[derive(Debug, Clone)]
pub struct ImageData {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

/// Configuration for pixel-by-pixel image comparison.
#[derive(Debug, Clone)]
pub struct CompareOptions {
    /// YIQ delta threshold (0.0 = exact match, 1.0 = any difference accepted).
    pub matching_threshold: f64,
    /// When `false`, anti-aliased pixels are detected and shown separately.
    pub enable_antialias: bool,
    /// RGB color for differing pixels.
    pub diff_color: [u8; 3],
    /// Optional alternate RGB color when img1 is brighter than img2.
    pub diff_color_alt: Option<[u8; 3]>,
    /// RGB color for anti-aliased pixels.
    pub aa_color: [u8; 3],
    /// Blending factor for matching pixels in the diff output (0.0..=1.0).
    pub alpha: f64,
}

impl Default for CompareOptions {
    fn default() -> Self {
        Self {
            matching_threshold: 0.0,
            enable_antialias: false,
            diff_color: [255, 119, 119],
            diff_color_alt: None,
            aa_color: [255, 255, 0],
            alpha: 0.1,
        }
    }
}

/// Result of an image comparison.
#[derive(Debug, Clone)]
pub struct DiffResult {
    /// Number of pixels that differ beyond the threshold.
    pub diff_count: u64,
    /// Total pixel count (based on the larger of the two images).
    pub total_pixels: u64,
    pub width: u32,
    pub height: u32,
    /// `true` when the input bytes were identical (hash fast-path).
    pub images_are_same: bool,
    /// Rendered diff image. `None` when `images_are_same` is `true`.
    pub diff_image: Option<ImageData>,
    /// Per-pixel boolean mask of differing pixels. `None` when `images_are_same` is `true`.
    pub diff_mask: Option<Vec<bool>>,
}

/// Errors that can occur during image comparison.
#[derive(Debug, Error)]
pub enum DiffError {
    #[error("failed to decode image: {0}")]
    ImageDecode(String),
    #[error("unsupported image format")]
    UnsupportedFormat,
}

/// Compare two image files from raw bytes with xxh3 hash fast-path.
pub fn compare_image_files(
    actual: &[u8],
    expected: &[u8],
    options: &CompareOptions,
) -> Result<DiffResult, DiffError> {
    let actual_hash = xxh3_64(actual);
    let expected_hash = xxh3_64(expected);

    if actual_hash == expected_hash {
        let img =
            image::load_from_memory(actual).map_err(|e| DiffError::ImageDecode(e.to_string()))?;
        let rgba = img.to_rgba8();
        let (w, h) = rgba.dimensions();
        return Ok(DiffResult {
            diff_count: 0,
            total_pixels: u64::from(w) * u64::from(h),
            width: w,
            height: h,
            images_are_same: true,
            diff_image: None,
            diff_mask: None,
        });
    }

    let actual_img =
        image::load_from_memory(actual).map_err(|e| DiffError::ImageDecode(e.to_string()))?;
    let expected_img =
        image::load_from_memory(expected).map_err(|e| DiffError::ImageDecode(e.to_string()))?;

    let actual_rgba = actual_img.to_rgba8();
    let expected_rgba = expected_img.to_rgba8();

    let actual_data = ImageData {
        width: actual_rgba.width(),
        height: actual_rgba.height(),
        data: actual_rgba.into_raw(),
    };
    let expected_data = ImageData {
        width: expected_rgba.width(),
        height: expected_rgba.height(),
        data: expected_rgba.into_raw(),
    };

    Ok(compare_images(&actual_data, &expected_data, options))
}

/// Compare two decoded images pixel by pixel using the pixelmatch algorithm.
pub fn compare_images(
    actual: &ImageData,
    expected: &ImageData,
    options: &CompareOptions,
) -> DiffResult {
    let width = actual.width.max(expected.width);
    let height = actual.height.max(expected.height);
    let total_pixels = u64::from(width) * u64::from(height);

    let bump = bumpalo::Bump::new();
    let actual_expanded = render::expand_image_in_bump(&bump, actual, width, height);
    let expected_expanded = render::expand_image_in_bump(&bump, expected, width, height);

    let actual_data = actual_expanded.as_deref().unwrap_or(&actual.data);
    let expected_data = expected_expanded.as_deref().unwrap_or(&expected.data);

    let max_delta = pixel::MAX_YIQ_DELTA;
    let threshold = options.matching_threshold * options.matching_threshold * max_delta;

    let diff_color = &options.diff_color;
    let diff_color_alt = options.diff_color_alt.as_ref();
    let aa_color = &options.aa_color;
    let alpha = options.alpha;

    let mut diff_count: u64 = 0;
    let mut diff_pixels = vec![0u8; (width * height * 4) as usize];
    let mut diff_mask = vec![false; (width * height) as usize];

    for y in 0..height {
        for x in 0..width {
            let pos = ((y * width + x) * 4) as usize;

            if actual_data[pos..pos + 4] == expected_data[pos..pos + 4] {
                render::draw_pixel_same(&mut diff_pixels, pos, actual_data, alpha);
                continue;
            }

            let delta = pixel::color_delta(actual_data, expected_data, pos, pos, false);

            if delta.abs() > threshold {
                if !options.enable_antialias
                    && (antialias::is_antialiased(actual_data, expected_data, x, y, width, height)
                        || antialias::is_antialiased(
                            expected_data,
                            actual_data,
                            x,
                            y,
                            width,
                            height,
                        ))
                {
                    render::draw_pixel_aa(&mut diff_pixels, pos, aa_color);
                } else {
                    diff_count += 1;
                    diff_mask[(y * width + x) as usize] = true;
                    render::draw_pixel_diff(
                        &mut diff_pixels,
                        pos,
                        delta,
                        diff_color,
                        diff_color_alt,
                    );
                }
            } else {
                render::draw_pixel_same(&mut diff_pixels, pos, actual_data, alpha);
            }
        }
    }

    DiffResult {
        diff_count,
        total_pixels,
        width,
        height,
        images_are_same: false,
        diff_image: Some(ImageData { width, height, data: diff_pixels }),
        diff_mask: Some(diff_mask),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_solid_image(w: u32, h: u32, r: u8, g: u8, b: u8, a: u8) -> ImageData {
        let pixel = [r, g, b, a];
        ImageData { width: w, height: h, data: pixel.repeat((w * h) as usize) }
    }

    #[test]
    fn test_compare_identical_images() {
        let img = make_solid_image(4, 4, 128, 128, 128, 255);
        let options = CompareOptions::default();
        let result = compare_images(&img, &img, &options);
        assert_eq!(result.diff_count, 0);
        assert_eq!(result.total_pixels, 16);
    }

    #[test]
    fn test_compare_different_images() {
        let img1 = make_solid_image(4, 4, 0, 0, 0, 255);
        let img2 = make_solid_image(4, 4, 255, 255, 255, 255);
        let options = CompareOptions::default();
        let result = compare_images(&img1, &img2, &options);
        assert!(result.diff_count > 0);
    }

    #[test]
    fn test_compare_single_pixel() {
        let img1 = make_solid_image(1, 1, 0, 0, 0, 255);
        let img2 = make_solid_image(1, 1, 255, 0, 0, 255);
        let options = CompareOptions::default();
        let result = compare_images(&img1, &img2, &options);
        assert_eq!(result.diff_count, 1);
        assert_eq!(result.total_pixels, 1);
    }

    #[test]
    fn test_compare_size_mismatch() {
        let img1 = make_solid_image(2, 2, 128, 128, 128, 255);
        let img2 = make_solid_image(4, 4, 128, 128, 128, 255);
        let options = CompareOptions::default();
        let result = compare_images(&img1, &img2, &options);
        // Images are expanded to max size (4x4)
        assert_eq!(result.width, 4);
        assert_eq!(result.height, 4);
        assert_eq!(result.total_pixels, 16);
    }

    #[test]
    fn test_compare_image_files_md5_fastpath() {
        // Create a minimal valid PNG in memory
        let img = make_solid_image(2, 2, 100, 100, 100, 255);
        let mut png_buf = Vec::new();
        {
            use image::ImageEncoder;
            let encoder = image::codecs::png::PngEncoder::new(&mut png_buf);
            encoder
                .write_image(&img.data, img.width, img.height, image::ExtendedColorType::Rgba8)
                .unwrap();
        }
        let options = CompareOptions::default();
        // Same bytes → MD5 match → fast path
        let result = compare_image_files(&png_buf, &png_buf, &options).unwrap();
        assert!(result.images_are_same);
        assert_eq!(result.diff_count, 0);
    }

    #[test]
    fn test_identical_pixel_fastpath() {
        // When pixels are identical byte-for-byte, they should skip color_delta
        let img = make_solid_image(4, 4, 50, 100, 150, 255);
        let options = CompareOptions::default();
        let result = compare_images(&img, &img, &options);
        assert_eq!(result.diff_count, 0);
        // diff_image should exist and be greyscale
        assert!(result.diff_image.is_some());
    }
}
