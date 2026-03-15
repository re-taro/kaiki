use crate::pixel;

/// Check if a pixel at (x, y) is antialiased by examining its 3x3 neighborhood.
#[inline]
pub fn is_antialiased(img1: &[u8], img2: &[u8], x: u32, y: u32, width: u32, height: u32) -> bool {
    let x0 = x.saturating_sub(1);
    let y0 = y.saturating_sub(1);
    let x1 = (x + 1).min(width - 1);
    let y1 = (y + 1).min(height - 1);

    let is_edge = x == x0 || x == x1 || y == y0 || y == y1;
    let mut zeroes: u32 = if is_edge { 1 } else { 0 };
    let mut min_delta: f64 = 0.0;
    let mut max_delta: f64 = 0.0;
    let mut min_x: u32 = 0;
    let mut min_y: u32 = 0;
    let mut max_x: u32 = 0;
    let mut max_y: u32 = 0;

    let center_k = ((y * width + x) * 4) as usize;

    for ny in y0..=y1 {
        for nx in x0..=x1 {
            if nx == x && ny == y {
                continue;
            }

            let neighbor_k = ((ny * width + nx) * 4) as usize;

            let delta = pixel::color_delta(img1, img1, center_k, neighbor_k, true);

            if delta == 0.0 {
                zeroes += 1;
                if zeroes > 2 {
                    return false;
                }
            } else if delta < min_delta {
                min_delta = delta;
                min_x = nx;
                min_y = ny;
            } else if delta > max_delta {
                max_delta = delta;
                max_x = nx;
                max_y = ny;
            }
        }
    }

    if min_delta == 0.0 || max_delta == 0.0 {
        return false;
    }

    (has_many_siblings(img1, min_x, min_y, width, height)
        && has_many_siblings(img2, min_x, min_y, width, height))
        || (has_many_siblings(img1, max_x, max_y, width, height)
            && has_many_siblings(img2, max_x, max_y, width, height))
}

/// Check if a pixel has 3+ adjacent pixels of the same color (by 32-bit RGBA value).
#[inline]
fn has_many_siblings(img: &[u8], x: u32, y: u32, width: u32, height: u32) -> bool {
    let x0 = x.saturating_sub(1);
    let y0 = y.saturating_sub(1);
    let x1 = (x + 1).min(width - 1);
    let y1 = (y + 1).min(height - 1);

    let k = ((y * width + x) * 4) as usize;
    let center = &img[k..k + 4];

    let is_edge = x == x0 || x == x1 || y == y0 || y == y1;
    let mut zeroes: u32 = if is_edge { 1 } else { 0 };

    for ny in y0..=y1 {
        for nx in x0..=x1 {
            if nx == x && ny == y {
                continue;
            }

            let nk = ((ny * width + nx) * 4) as usize;
            if img[nk..nk + 4] == *center {
                zeroes += 1;
                if zeroes > 2 {
                    return true;
                }
            }
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Create a flat 3x3 image (all same color).
    fn flat_3x3(r: u8, g: u8, b: u8) -> Vec<u8> {
        let pixel = [r, g, b, 255];
        pixel.repeat(9)
    }

    #[test]
    fn test_not_antialiased_flat_area() {
        // All pixels the same color: definitely not antialiased
        let img = flat_3x3(128, 128, 128);
        assert!(!is_antialiased(&img, &img, 1, 1, 3, 3));
    }

    #[test]
    fn test_not_antialiased_edge_pixel() {
        // Edge pixel (0,0) in a uniform image
        let img = flat_3x3(100, 100, 100);
        assert!(!is_antialiased(&img, &img, 0, 0, 3, 3));
    }

    #[test]
    fn test_has_many_siblings_uniform() {
        // All pixels identical → should have many siblings
        let img = flat_3x3(50, 50, 50);
        assert!(has_many_siblings(&img, 1, 1, 3, 3));
    }

    #[test]
    fn test_has_many_siblings_diverse() {
        // All pixels different → no siblings
        let mut img = vec![0u8; 9 * 4];
        for i in 0..9 {
            img[i * 4] = (i * 28) as u8;
            img[i * 4 + 1] = (i * 17) as u8;
            img[i * 4 + 2] = (i * 37) as u8;
            img[i * 4 + 3] = 255;
        }
        assert!(!has_many_siblings(&img, 1, 1, 3, 3));
    }

    #[test]
    fn test_edge_pixel_zeroes_initialization() {
        // Corner pixel (0,0) with all identical neighbors should return false
        // because zeroes starts at 1 (edge) + all neighbors equal = zeroes > 2 quickly
        let img = flat_3x3(100, 100, 100);
        // At (0,0) with 3x3 image, only 3 neighbors visible; zeroes starts at 1
        // All 3 neighbors are identical to center: zeroes = 1 + 3 = 4 > 2 → not AA
        assert!(!is_antialiased(&img, &img, 0, 0, 3, 3));
    }
}
