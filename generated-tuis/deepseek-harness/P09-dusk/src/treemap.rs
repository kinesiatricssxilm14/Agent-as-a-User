//! Squarified treemap layout.
//!
//! The layout assigns each item a rectangle whose area is proportional to its
//! weight (here: byte size). The classic "squarified" algorithm (Bruls,
//! Huizing & van Wijk) is used so rectangles stay as square as possible, which
//! makes relative sizes easy to compare visually.

/// A floating point rectangle `(x, y, w, h)`.
type FRect = (f64, f64, f64, f64);

/// Compute the worst (largest) aspect ratio of a candidate row laid out as a
/// strip along the shorter side of a `w` x `h` rectangle.
fn worst(row: &[f64], w: f64, h: f64) -> f64 {
    let sum: f64 = row.iter().sum();
    if sum <= 0.0 || w <= 0.0 || h <= 0.0 {
        return f64::INFINITY;
    }
    let short = w.min(h);
    let s2 = short * short;
    let sum2 = sum * sum;
    let mut worst = 0.0_f64;
    for &area in row {
        if area <= 0.0 {
            return f64::INFINITY;
        }
        let num = area * s2;
        let ratio = (num / sum2).max(sum2 / num);
        if ratio > worst {
            worst = ratio;
        }
    }
    worst
}

/// Lay out one `row` of items inside the current free rectangle and return the
/// remaining free rectangle.
fn layout_row(row: &[f64], x: f64, y: f64, w: f64, h: f64, out: &mut Vec<FRect>) -> FRect {
    let sum: f64 = row.iter().sum();
    if w <= h {
        // Horizontal band along the top: thickness runs along the height.
        let thickness = sum / w;
        let mut cx = x;
        for &area in row {
            let item_w = area / thickness;
            out.push((cx, y, item_w, thickness));
            cx += item_w;
        }
        (x, y + thickness, w, h - thickness)
    } else {
        // Vertical band along the left: thickness runs along the width.
        let thickness = sum / h;
        let mut cy = y;
        for &area in row {
            let item_h = area / thickness;
            out.push((x, cy, thickness, item_h));
            cy += item_h;
        }
        (x + thickness, y, w - thickness, h)
    }
}

/// Squarified treemap: return one rectangle per input weight, in input order.
fn squarify(weights: &[f64], x: f64, y: f64, w: f64, h: f64) -> Vec<FRect> {
    let mut out = Vec::new();
    let (mut x, mut y, mut w, mut h) = (x, y, w, h);
    let n = weights.len();
    let mut i = 0;

    while i < n {
        // Build the next row greedily while adding an item does not worsen the
        // worst aspect ratio.
        let mut row = vec![weights[i]];
        let mut end = i + 1;
        while end < n {
            let mut candidate = row.clone();
            candidate.push(weights[end]);
            if worst(&candidate, w, h) <= worst(&row, w, h) {
                row = candidate;
                end += 1;
            } else {
                break;
            }
        }

        let (nx, ny, nw, nh) = layout_row(&row, x, y, w, h, &mut out);
        x = nx;
        y = ny;
        w = nw;
        h = nh;
        i = end;

        if w <= 0.0 || h <= 0.0 {
            break;
        }
    }

    out
}

/// Public entry point: convert byte weights into integer cell rectangles
/// `(x, y, width, height)` inside a `width` x `height` terminal area.
///
/// Zero-weight items are ignored (they get no area).
pub fn layout_treemap(weights: &[f64], width: u16, height: u16) -> Vec<(u16, u16, u16, u16)> {
    if width == 0 || height == 0 || weights.is_empty() {
        return Vec::new();
    }

    let total: f64 = weights.iter().sum();
    if total <= 0.0 {
        return Vec::new();
    }
    // Normalize so the sum of the areas equals the whole rectangle area; the
    // squarified algorithm operates on absolute areas, not proportions.
    let cell_area = f64::from(width) * f64::from(height);
    let areas: Vec<f64> = weights.iter().map(|w| w / total * cell_area).collect();

    let rects = squarify(&areas, 0.0, 0.0, f64::from(width), f64::from(height));

    rects
        .into_iter()
        .map(|(x, y, w, h)| {
            let x0 = x.round().clamp(0.0, f64::from(width)) as u16;
            let y0 = y.round().clamp(0.0, f64::from(height)) as u16;
            let x1 = (x + w).round().clamp(0.0, f64::from(width)) as u16;
            let y1 = (y + h).round().clamp(0.0, f64::from(height)) as u16;
            let rw = x1.saturating_sub(x0).max(1);
            let rh = y1.saturating_sub(y0).max(1);
            (x0, y0, rw, rh)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equal_weights_produce_four_squares() {
        let rects = layout_treemap(&[1.0, 1.0, 1.0, 1.0], 4, 4);
        assert_eq!(rects.len(), 4);
        for (_, _, w, h) in rects {
            assert_eq!(w, 2);
            assert_eq!(h, 2);
        }
    }

    #[test]
    fn area_preserves_order_and_counts() {
        let rects = layout_treemap(&[9.0, 1.0], 10, 10);
        assert_eq!(rects.len(), 2);
        // The larger item must cover more cells than the smaller one.
        let big = rects[0].2 * rects[0].3;
        let small = rects[1].2 * rects[1].3;
        assert!(big > small);
    }
}
