use crate::ImageData;

/// Expand an image to the given dimensions, padding with transparent black (0,0,0,0).
/// Returns `None` if the image is already the correct size.
#[cfg(test)]
pub fn expand_image(img: &ImageData, width: u32, height: u32) -> Option<ImageData> {
    if img.width == width && img.height == height {
        return None;
    }

    let mut data = vec![0u8; (width * height * 4) as usize];

    for y in 0..img.height {
        for x in 0..img.width {
            let src = ((y * img.width + x) * 4) as usize;
            let dst = ((y * width + x) * 4) as usize;
            data[dst..dst + 4].copy_from_slice(&img.data[src..src + 4]);
        }
    }

    Some(ImageData { width, height, data })
}

/// Expand an image into a bump-allocated buffer, padding with transparent black (0,0,0,0).
/// Returns `None` if the image is already the correct size.
pub(crate) fn expand_image_in_bump<'a>(
    bump: &'a bumpalo::Bump,
    img: &ImageData,
    width: u32,
    height: u32,
) -> Option<bumpalo::collections::Vec<'a, u8>> {
    if img.width == width && img.height == height {
        return None;
    }

    let len = (width * height * 4) as usize;
    let mut data = bumpalo::collections::Vec::with_capacity_in(len, bump);
    data.resize(len, 0u8);

    for y in 0..img.height {
        for x in 0..img.width {
            let src = ((y * img.width + x) * 4) as usize;
            let dst = ((y * width + x) * 4) as usize;
            data[dst..dst + 4].copy_from_slice(&img.data[src..src + 4]);
        }
    }

    Some(data)
}

/// Draw a diff pixel using the configured diff colors.
/// When `delta < 0.0` (img1 brighter), uses `diff_color_alt` (falls back to `diff_color`).
/// When `delta >= 0.0` (img2 brighter), uses `diff_color`.
#[inline]
pub fn draw_pixel_diff(
    output: &mut [u8],
    pos: usize,
    delta: f64,
    diff_color: &[u8; 3],
    diff_color_alt: Option<&[u8; 3]>,
) {
    let color = if delta < 0.0 {
        diff_color_alt.unwrap_or(diff_color)
    } else {
        diff_color
    };
    let out = &mut output[pos..pos + 4];
    out[0] = color[0];
    out[1] = color[1];
    out[2] = color[2];
    out[3] = 255;
}

/// Draw an antialiased pixel using the configured AA color.
#[inline]
pub fn draw_pixel_aa(output: &mut [u8], pos: usize, aa_color: &[u8; 3]) {
    let out = &mut output[pos..pos + 4];
    out[0] = aa_color[0];
    out[1] = aa_color[1];
    out[2] = aa_color[2];
    out[3] = 255;
}

/// Draw a matching pixel (greyscale blended with white at given alpha).
#[inline]
pub fn draw_pixel_same(output: &mut [u8], pos: usize, img: &[u8], alpha: f64) {
    let px = &img[pos..pos + 4];
    let r = f64::from(px[0]);
    let g = f64::from(px[1]);
    let b = f64::from(px[2]);
    let a = f64::from(px[3]) / 255.0;

    let luminance = r * 0.29889531 + g * 0.58662247 + b * 0.11448223;
    let val = (255.0 + (luminance - 255.0) * alpha * a) as u8;

    let out = &mut output[pos..pos + 4];
    out[0] = val;
    out[1] = val;
    out[2] = val;
    out[3] = 255;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expand_image_no_change() {
        let img = ImageData { width: 2, height: 2, data: vec![0u8; 16] };
        assert!(expand_image(&img, 2, 2).is_none());
    }

    #[test]
    fn test_expand_image_larger() {
        let img = ImageData {
            width: 1,
            height: 1,
            data: vec![255, 0, 0, 255],
        };
        let expanded = expand_image(&img, 2, 2).unwrap();
        assert_eq!(expanded.width, 2);
        assert_eq!(expanded.height, 2);
        // First pixel should be preserved
        assert_eq!(&expanded.data[0..4], &[255, 0, 0, 255]);
        // Padded pixels should be transparent black
        assert_eq!(&expanded.data[4..8], &[0, 0, 0, 0]);
    }

    #[test]
    fn test_draw_pixel_diff_positive_delta() {
        let mut output = vec![0u8; 4];
        let diff_color = [255, 119, 119];
        draw_pixel_diff(&mut output, 0, 1.0, &diff_color, None);
        // positive delta (img2 brighter) → diff_color [255, 119, 119]
        assert_eq!(output, [255, 119, 119, 255]);
    }

    #[test]
    fn test_draw_pixel_diff_negative_delta() {
        let mut output = vec![0u8; 4];
        let diff_color = [255, 119, 119];
        draw_pixel_diff(&mut output, 0, -1.0, &diff_color, None);
        // negative delta (img1 brighter), no alt → falls back to diff_color
        assert_eq!(output, [255, 119, 119, 255]);
    }

    #[test]
    fn test_draw_pixel_diff_negative_delta_with_alt() {
        let mut output = vec![0u8; 4];
        let diff_color = [255, 119, 119];
        let diff_color_alt = [255, 0, 0];
        draw_pixel_diff(&mut output, 0, -1.0, &diff_color, Some(&diff_color_alt));
        // negative delta (img1 brighter) with alt → uses diff_color_alt
        assert_eq!(output, [255, 0, 0, 255]);
    }

    #[test]
    fn test_draw_pixel_diff_positive_delta_with_alt() {
        let mut output = vec![0u8; 4];
        let diff_color = [255, 119, 119];
        let diff_color_alt = [255, 0, 0];
        draw_pixel_diff(&mut output, 0, 1.0, &diff_color, Some(&diff_color_alt));
        // positive delta (img2 brighter) with alt → still uses diff_color
        assert_eq!(output, [255, 119, 119, 255]);
    }

    #[test]
    fn test_draw_pixel_same_opaque_white() {
        let mut output = vec![0u8; 4];
        let img = vec![255u8, 255, 255, 255];
        draw_pixel_same(&mut output, 0, &img, 0.1);
        // White pixel blended with white at 0.1 alpha → near 255
        assert_eq!(output[3], 255);
        // All channels should be equal (greyscale)
        assert_eq!(output[0], output[1]);
        assert_eq!(output[1], output[2]);
    }

    #[test]
    fn test_draw_pixel_aa() {
        let mut output = vec![0u8; 4];
        let aa_color = [255, 255, 0];
        draw_pixel_aa(&mut output, 0, &aa_color);
        assert_eq!(output, [255, 255, 0, 255]); // Yellow
    }

    #[test]
    fn test_draw_pixel_aa_custom_color() {
        let mut output = vec![0u8; 4];
        let aa_color = [0, 128, 255];
        draw_pixel_aa(&mut output, 0, &aa_color);
        assert_eq!(output, [0, 128, 255, 255]);
    }
}
