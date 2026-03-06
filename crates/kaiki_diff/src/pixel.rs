/// Maximum possible YIQ delta value.
pub const MAX_YIQ_DELTA: f64 = 35215.0;

const Y_R: f64 = 0.29889531;
const Y_G: f64 = 0.58662247;
const Y_B: f64 = 0.11448223;

const I_R: f64 = 0.59597799;
const I_G: f64 = -0.27417610;
const I_B: f64 = -0.32180189;

const Q_R: f64 = 0.21147017;
const Q_G: f64 = -0.52261711;
const Q_B: f64 = 0.31114694;

const DELTA_Y: f64 = 0.5053;
const DELTA_I: f64 = 0.299;
const DELTA_Q: f64 = 0.1957;

const PHI: f64 = 1.618033988749895;

/// Compute the YIQ color delta between two pixels.
/// `k` is the byte offset into img1's RGBA data, `m` is the byte offset into img2's RGBA data.
/// If `y_only` is true, returns only the Y (brightness) component.
///
/// This is a direct port of pixelmatch's `colorDelta` function.
#[inline]
pub fn color_delta(img1: &[u8], img2: &[u8], k: usize, m: usize, y_only: bool) -> f64 {
    let px1 = &img1[k..k + 4];
    let px2 = &img2[m..m + 4];

    let r1 = f64::from(px1[0]);
    let g1 = f64::from(px1[1]);
    let b1 = f64::from(px1[2]);
    let a1 = f64::from(px1[3]);
    let r2 = f64::from(px2[0]);
    let g2 = f64::from(px2[1]);
    let b2 = f64::from(px2[2]);
    let a2 = f64::from(px2[3]);

    let mut dr = r1 - r2;
    let mut dg = g1 - g2;
    let mut db = b1 - b2;
    let da = a1 - a2;

    // Fast path: all components are equal
    if dr == 0.0 && dg == 0.0 && db == 0.0 && da == 0.0 {
        return 0.0;
    }

    // If either pixel has alpha < 255, blend with background pattern
    if a1 < 255.0 || a2 < 255.0 {
        // Background pattern (pixelmatch uses byte offset k for the pattern)
        let rb = 48.0 + 159.0 * f64::from((k % 2) as u8);
        let gb = 48.0 + 159.0 * f64::from(((k as f64 / PHI).floor() as usize % 2) as u8);
        let bb = 48.0 + 159.0 * f64::from(((k as f64 / (1.0 + PHI)).floor() as usize % 2) as u8);

        dr = (r1 * a1 - r2 * a2 - rb * da) / 255.0;
        dg = (g1 * a1 - g2 * a2 - gb * da) / 255.0;
        db = (b1 * a1 - b2 * a2 - bb * da) / 255.0;
    }

    let y = dr * Y_R + dg * Y_G + db * Y_B;

    if y_only {
        return y;
    }

    let i = dr * I_R + dg * I_G + db * I_B;
    let q = dr * Q_R + dg * Q_G + db * Q_B;

    let delta = DELTA_Y * y * y + DELTA_I * i * i + DELTA_Q * q * q;

    // Encode whether the pixel lightens or darkens in the sign
    if y > 0.0 { -delta } else { delta }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper: make RGBA pixel at offset 0
    fn px(r: u8, g: u8, b: u8, a: u8) -> Vec<u8> {
        vec![r, g, b, a]
    }

    #[test]
    fn test_color_delta_identical_opaque() {
        let img = px(100, 150, 200, 255);
        assert_eq!(color_delta(&img, &img, 0, 0, false), 0.0);
    }

    #[test]
    fn test_color_delta_different_opaque() {
        let img1 = px(255, 0, 0, 255);
        let img2 = px(0, 0, 255, 255);
        let delta = color_delta(&img1, &img2, 0, 0, false);
        assert!(delta.abs() > 0.0, "different colors should have non-zero delta");
    }

    #[test]
    fn test_color_delta_with_alpha() {
        let img1 = px(255, 0, 0, 128);
        let img2 = px(0, 0, 255, 128);
        let delta = color_delta(&img1, &img2, 0, 0, false);
        assert!(delta.abs() > 0.0, "different semi-transparent colors should have non-zero delta");
    }

    #[test]
    fn test_color_delta_y_only() {
        let img1 = px(255, 255, 255, 255);
        let img2 = px(0, 0, 0, 255);
        let y = color_delta(&img1, &img2, 0, 0, true);
        // White is brighter, so y should be positive (r1-r2 = 255 for all channels)
        assert!(y > 0.0);
    }

    #[test]
    fn test_color_delta_dual_offset() {
        // Two pixels in a single image, at different offsets
        let img = vec![255, 0, 0, 255, 0, 255, 0, 255];
        let delta = color_delta(&img, &img, 0, 4, false);
        assert!(delta.abs() > 0.0, "different pixels at different offsets");
    }

    #[test]
    fn test_color_delta_fully_transparent() {
        let img1 = px(255, 0, 0, 0);
        let img2 = px(0, 255, 0, 0);
        // Both fully transparent: blended result depends on background but da=0
        let delta = color_delta(&img1, &img2, 0, 0, false);
        // With alpha=0, dr = (r1*0 - r2*0 - rb*0)/255 = 0
        assert_eq!(delta, 0.0);
    }

    #[test]
    fn test_color_delta_sign_encoding() {
        // When img2 is brighter (y < 0), delta should be positive
        let dark = px(0, 0, 0, 255);
        let bright = px(255, 255, 255, 255);
        let delta = color_delta(&dark, &bright, 0, 0, false);
        assert!(delta > 0.0, "delta should be positive when img2 is brighter");
    }

    #[test]
    fn test_color_delta_max_delta_bound() {
        let black = px(0, 0, 0, 255);
        let white = px(255, 255, 255, 255);
        let delta = color_delta(&black, &white, 0, 0, false);
        assert!(delta.abs() <= MAX_YIQ_DELTA, "delta should not exceed MAX_YIQ_DELTA");
    }
}
