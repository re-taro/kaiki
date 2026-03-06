use serde::Serialize;

/// A bounding box around a region of differing pixels.
#[derive(Debug, Clone, Serialize)]
pub struct BoundingBox {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// Detect connected components of differing pixels and return their bounding boxes.
///
/// Uses Union-Find with 8-connectivity. Components with fewer than `min_area`
/// pixels are filtered out (noise removal).
pub fn detect_diff_regions(
    img_width: u32,
    img_height: u32,
    diff_mask: &[bool],
    min_area: u32,
) -> Vec<BoundingBox> {
    let total = (img_width as usize) * (img_height as usize);
    assert_eq!(diff_mask.len(), total);

    if total == 0 {
        return Vec::new();
    }

    let mut parent: Vec<usize> = (0..total).collect();
    let mut rank: Vec<u8> = vec![0; total];

    fn find(parent: &mut [usize], mut x: usize) -> usize {
        while parent[x] != x {
            parent[x] = parent[parent[x]]; // path halving
            x = parent[x];
        }
        x
    }

    fn union(parent: &mut [usize], rank: &mut [u8], a: usize, b: usize) {
        let ra = find(parent, a);
        let rb = find(parent, b);
        if ra == rb {
            return;
        }
        if rank[ra] < rank[rb] {
            parent[ra] = rb;
        } else if rank[ra] > rank[rb] {
            parent[rb] = ra;
        } else {
            parent[rb] = ra;
            rank[ra] += 1;
        }
    }

    let w = img_width as usize;

    // Merge adjacent diff pixels (8-connectivity)
    for y in 0..img_height as usize {
        for x in 0..w {
            let idx = y * w + x;
            if !diff_mask[idx] {
                continue;
            }

            // Check right neighbor
            if x + 1 < w && diff_mask[idx + 1] {
                union(&mut parent, &mut rank, idx, idx + 1);
            }
            // Check bottom neighbor
            if y + 1 < img_height as usize {
                let below = idx + w;
                if diff_mask[below] {
                    union(&mut parent, &mut rank, idx, below);
                }
                // Check bottom-left
                if x > 0 && diff_mask[below - 1] {
                    union(&mut parent, &mut rank, idx, below - 1);
                }
                // Check bottom-right
                if x + 1 < w && diff_mask[below + 1] {
                    union(&mut parent, &mut rank, idx, below + 1);
                }
            }
        }
    }

    // Collect bounding boxes per component
    use std::collections::HashMap;

    struct ComponentBounds {
        min_x: u32,
        min_y: u32,
        max_x: u32,
        max_y: u32,
        area: u32,
    }

    let mut components: HashMap<usize, ComponentBounds> = HashMap::new();

    for y in 0..img_height {
        for x in 0..img_width {
            let idx = (y as usize) * w + (x as usize);
            if !diff_mask[idx] {
                continue;
            }
            let root = find(&mut parent, idx);
            let entry = components.entry(root).or_insert(ComponentBounds {
                min_x: x,
                min_y: y,
                max_x: x,
                max_y: y,
                area: 0,
            });
            entry.min_x = entry.min_x.min(x);
            entry.min_y = entry.min_y.min(y);
            entry.max_x = entry.max_x.max(x);
            entry.max_y = entry.max_y.max(y);
            entry.area += 1;
        }
    }

    let mut boxes: Vec<BoundingBox> = components
        .into_values()
        .filter(|c| c.area >= min_area)
        .map(|c| BoundingBox {
            x: c.min_x,
            y: c.min_y,
            width: c.max_x - c.min_x + 1,
            height: c.max_y - c.min_y + 1,
        })
        .collect();

    // Sort by position for deterministic output
    boxes.sort_by_key(|b| (b.y, b.x));
    boxes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_diff() {
        let mask = vec![false; 100];
        let result = detect_diff_regions(10, 10, &mask, 1);
        assert!(result.is_empty());
    }

    #[test]
    fn test_all_diff() {
        let mask = vec![true; 100];
        let result = detect_diff_regions(10, 10, &mask, 1);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].x, 0);
        assert_eq!(result[0].y, 0);
        assert_eq!(result[0].width, 10);
        assert_eq!(result[0].height, 10);
    }

    #[test]
    fn test_two_separate_clusters() {
        // 10x10 grid with two 2x2 clusters far apart
        let mut mask = vec![false; 100];
        // Cluster at (0,0)
        mask[0] = true;
        mask[1] = true;
        mask[10] = true;
        mask[11] = true;
        // Cluster at (8,8)
        mask[88] = true;
        mask[89] = true;
        mask[98] = true;
        mask[99] = true;

        let result = detect_diff_regions(10, 10, &mask, 1);
        assert_eq!(result.len(), 2);

        assert_eq!(result[0].x, 0);
        assert_eq!(result[0].y, 0);
        assert_eq!(result[0].width, 2);
        assert_eq!(result[0].height, 2);

        assert_eq!(result[1].x, 8);
        assert_eq!(result[1].y, 8);
        assert_eq!(result[1].width, 2);
        assert_eq!(result[1].height, 2);
    }

    #[test]
    fn test_min_area_filter() {
        let mut mask = vec![false; 100];
        // Single pixel at (5,5) - area = 1
        mask[55] = true;
        // 3x3 block at (0,0) - area = 9
        for y in 0..3 {
            for x in 0..3 {
                mask[y * 10 + x] = true;
            }
        }

        // min_area = 5 should filter out the single pixel
        let result = detect_diff_regions(10, 10, &mask, 5);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].x, 0);
        assert_eq!(result[0].y, 0);
        assert_eq!(result[0].width, 3);
        assert_eq!(result[0].height, 3);
    }

    #[test]
    fn test_diagonal_connectivity() {
        // Two pixels connected diagonally should be in the same component (8-connectivity)
        let mut mask = vec![false; 25];
        mask[0] = true; // (0,0)
        mask[6] = true; // (1,1)

        let result = detect_diff_regions(5, 5, &mask, 1);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].width, 2);
        assert_eq!(result[0].height, 2);
    }

    #[test]
    fn test_empty_image() {
        let mask: Vec<bool> = Vec::new();
        let result = detect_diff_regions(0, 0, &mask, 1);
        assert!(result.is_empty());
    }
}
