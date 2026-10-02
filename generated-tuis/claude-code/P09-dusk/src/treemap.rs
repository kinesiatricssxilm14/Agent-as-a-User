//! Squarified treemap layout.
//!
//! Rectangle *area* is proportional to the item's size, which is what makes the
//! view readable: a file taking half the disk takes half the canvas. The
//! algorithm is the classic squarified one (Bruls, Huizing & van Wijk): lay
//! items out biggest-first in rows/columns, extending the current strip while
//! doing so improves its worst aspect ratio.

use ratatui::layout::Rect;

/// One input item: a label plus its weight in bytes.
#[derive(Debug, Clone)]
pub struct Item {
    pub label: String,
    pub weight: u64,
    /// Index back into the caller's own list.
    pub index: usize,
}

/// One laid out tile.
#[derive(Debug, Clone)]
pub struct Tile {
    pub rect: Rect,
    pub label: String,
    pub weight: u64,
    pub index: usize,
}

/// Lay `items` out inside `area`. Items must be sorted descending by weight for
/// the nicest result; the function tolerates any order.
///
/// Items whose area rounds away to nothing are dropped, so the returned vector
/// may be shorter than the input. Tiles are returned in input order.
pub fn squarify(items: &[Item], area: Rect) -> Vec<Tile> {
    let mut out = Vec::new();
    if area.width == 0 || area.height == 0 || items.is_empty() {
        return out;
    }

    let total: u64 = items.iter().map(|i| i.weight).sum();
    if total == 0 {
        return grid(items, area);
    }

    // Work in f64 rectangle space and round only when emitting tiles.
    let canvas = FRect {
        x: area.x as f64,
        y: area.y as f64,
        w: area.width as f64,
        h: area.height as f64,
    };
    let scale = (canvas.w * canvas.h) / total as f64;

    let mut remaining: Vec<(&Item, f64)> = items
        .iter()
        .filter(|i| i.weight > 0)
        .map(|i| (i, i.weight as f64 * scale))
        .collect();
    // Squarified layout assumes descending order.
    remaining.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    let mut rect = canvas;
    let mut cursor = 0;
    while cursor < remaining.len() {
        let short = rect.w.min(rect.h);
        if short <= 0.0 {
            break;
        }

        // Grow the strip while the worst aspect ratio keeps improving.
        let mut strip_area = remaining[cursor].1;
        let mut end = cursor + 1;
        let mut worst = aspect(strip_area, remaining[cursor].1, remaining[cursor].1, short);
        while end < remaining.len() {
            let next_area = strip_area + remaining[end].1;
            let min = remaining[end].1;
            let max = remaining[cursor].1;
            let candidate = aspect(next_area, min, max, short);
            if candidate > worst {
                break;
            }
            worst = candidate;
            strip_area = next_area;
            end += 1;
        }

        let strip = &remaining[cursor..end];
        rect = place_strip(strip, strip_area, rect, &mut out);
        cursor = end;
    }

    out.sort_by_key(|t| t.index);
    out
}

#[derive(Debug, Clone, Copy)]
struct FRect {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

/// Worst aspect ratio of a strip of the given total area laid along `short`.
fn aspect(strip_area: f64, min: f64, max: f64, short: f64) -> f64 {
    if strip_area <= 0.0 || short <= 0.0 {
        return f64::INFINITY;
    }
    let side = strip_area / short; // thickness of the strip
    let a = (short * short * max) / (strip_area * strip_area);
    let b = (strip_area * strip_area) / (short * short * min.max(f64::MIN_POSITIVE));
    let _ = side;
    a.max(b)
}

/// Emit one strip and return the rectangle left over for the next strip.
fn place_strip(strip: &[(&Item, f64)], strip_area: f64, rect: FRect, out: &mut Vec<Tile>) -> FRect {
    let horizontal = rect.w >= rect.h;
    // Thickness of the strip across the long side.
    let thickness = if horizontal {
        (strip_area / rect.h).min(rect.w)
    } else {
        (strip_area / rect.w).min(rect.h)
    };

    let mut offset = 0.0;
    let span = if horizontal { rect.h } else { rect.w };
    for (item, area) in strip {
        let extent = if strip_area > 0.0 {
            span * (area / strip_area)
        } else {
            span / strip.len() as f64
        };
        let tile = if horizontal {
            FRect {
                x: rect.x,
                y: rect.y + offset,
                w: thickness,
                h: extent,
            }
        } else {
            FRect {
                x: rect.x + offset,
                y: rect.y,
                w: extent,
                h: thickness,
            }
        };
        offset += extent;
        if let Some(r) = snap(tile) {
            out.push(Tile {
                rect: r,
                label: item.label.clone(),
                weight: item.weight,
                index: item.index,
            });
        }
    }

    if horizontal {
        FRect {
            x: rect.x + thickness,
            y: rect.y,
            w: (rect.w - thickness).max(0.0),
            h: rect.h,
        }
    } else {
        FRect {
            x: rect.x,
            y: rect.y + thickness,
            w: rect.w,
            h: (rect.h - thickness).max(0.0),
        }
    }
}

/// Round a float rectangle to cells, keeping edges aligned so tiles neither
/// overlap nor leave gaps.
fn snap(r: FRect) -> Option<Rect> {
    let x0 = r.x.round();
    let y0 = r.y.round();
    let x1 = (r.x + r.w).round();
    let y1 = (r.y + r.h).round();
    let w = (x1 - x0) as i64;
    let h = (y1 - y0) as i64;
    if w <= 0 || h <= 0 {
        return None;
    }
    Some(Rect {
        x: x0 as u16,
        y: y0 as u16,
        width: w as u16,
        height: h as u16,
    })
}

/// Fallback used when every item has weight zero: equal-size grid cells so the
/// view still shows something instead of collapsing to nothing.
fn grid(items: &[Item], area: Rect) -> Vec<Tile> {
    let n = items.len() as u16;
    let cols = (n as f64).sqrt().ceil().max(1.0) as u16;
    let rows = n.div_ceil(cols).max(1);
    let cw = (area.width / cols).max(1);
    let ch = (area.height / rows).max(1);
    let mut out = Vec::new();
    for (i, item) in items.iter().enumerate() {
        let col = i as u16 % cols;
        let row = i as u16 / cols;
        let x = area.x + col * cw;
        let y = area.y + row * ch;
        if x >= area.right() || y >= area.bottom() {
            continue;
        }
        out.push(Tile {
            rect: Rect {
                x,
                y,
                width: cw.min(area.right() - x),
                height: ch.min(area.bottom() - y),
            },
            label: item.label.clone(),
            weight: item.weight,
            index: item.index,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items(weights: &[u64]) -> Vec<Item> {
        weights
            .iter()
            .enumerate()
            .map(|(i, &w)| Item {
                label: format!("item{}", i),
                weight: w,
                index: i,
            })
            .collect()
    }

    #[test]
    fn tiles_stay_inside_the_canvas() {
        let area = Rect::new(2, 3, 60, 20);
        let tiles = squarify(&items(&[500, 300, 120, 60, 12, 5, 3]), area);
        assert!(!tiles.is_empty());
        for t in &tiles {
            assert!(t.rect.x >= area.x, "{:?}", t.rect);
            assert!(t.rect.y >= area.y, "{:?}", t.rect);
            assert!(t.rect.right() <= area.right(), "{:?}", t.rect);
            assert!(t.rect.bottom() <= area.bottom(), "{:?}", t.rect);
        }
    }

    #[test]
    fn tiles_do_not_overlap() {
        let area = Rect::new(0, 0, 80, 24);
        let tiles = squarify(&items(&[900, 700, 400, 350, 200, 90, 40, 20]), area);
        let mut seen = vec![0u8; (area.width as usize) * (area.height as usize)];
        for t in &tiles {
            for y in t.rect.y..t.rect.bottom() {
                for x in t.rect.x..t.rect.right() {
                    let idx = y as usize * area.width as usize + x as usize;
                    seen[idx] += 1;
                    assert_eq!(seen[idx], 1, "cell {},{} covered twice", x, y);
                }
            }
        }
    }

    #[test]
    fn bigger_weight_gets_bigger_area() {
        let area = Rect::new(0, 0, 100, 40);
        let tiles = squarify(&items(&[800, 100, 50, 50]), area);
        let biggest = tiles.iter().find(|t| t.index == 0).unwrap();
        let second = tiles.iter().find(|t| t.index == 1).unwrap();
        let a = biggest.rect.width as u32 * biggest.rect.height as u32;
        let b = second.rect.width as u32 * second.rect.height as u32;
        assert!(a > b, "expected {} > {}", a, b);
        // 800/1000 of a 4000 cell canvas is roughly 3200 cells.
        assert!((a as i64 - 3200).abs() < 700, "area was {}", a);
    }

    #[test]
    fn degenerate_inputs_do_not_panic() {
        assert!(squarify(&[], Rect::new(0, 0, 10, 10)).is_empty());
        assert!(squarify(&items(&[1, 2]), Rect::new(0, 0, 0, 10)).is_empty());
        let zeros = squarify(&items(&[0, 0, 0]), Rect::new(0, 0, 10, 10));
        assert_eq!(zeros.len(), 3);
    }
}
