//! Draws the tab at the screen edge: a small piece of the shelf with an arrow pointing
//! into the screen. Rounded on the side away from the edge, a border on that side, top
//! and bottom. Painted here, in software, so it is smooth at any scale and testable.

use shelve_core::Edge;

/// Tab size in points.
pub const TAB_SIZE: (f32, f32) = (12.0, 48.0);
/// Corner radius on the side away from the edge, points.
const RADIUS: f32 = 6.0;
const BORDER: f32 = 1.0;
/// The arrow for a tab on the right edge (it points left), points: from, tip, to.
const ARROW: [(f32, f32); 3] = [(7.5, 20.5), (4.5, 24.0), (7.5, 27.5)];
const ARROW_HALF_WIDTH: f32 = 0.7;
/// Samples per pixel side; 4 × 4 smooths the curve and the arrow.
const SAMPLES: u32 = 4;

/// Colours of the shelf itself, so the tab looks like a piece of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TabColors {
    pub bg: [u8; 3],
    pub border: [u8; 3],
    /// The arrow; the whole tab while the mouse is on it.
    pub accent: [u8; 3],
}

/// `w` × `h` pixels as premultiplied BGRA, top row first (for `UpdateLayeredWindow`).
/// Under the mouse the tab fills with the accent and the arrow takes the background.
pub fn tab_pixels(
    w: u32,
    h: u32,
    scale: f32,
    edge: Edge,
    colors: TabColors,
    hovered: bool,
) -> Vec<u8> {
    let (fill, border, arrow) = if hovered {
        (colors.accent, colors.accent, colors.bg)
    } else {
        (colors.bg, colors.border, colors.accent)
    };
    let mut out = Vec::with_capacity((w * h * 4) as usize);
    let n = (SAMPLES * SAMPLES) as f32;
    for py in 0..h {
        for px in 0..w {
            let mut sum = [0.0f32; 3];
            let mut covered = 0.0f32;
            for sy in 0..SAMPLES {
                for sx in 0..SAMPLES {
                    let x = (px as f32 + (sx as f32 + 0.5) / SAMPLES as f32) / scale;
                    let y = (py as f32 + (sy as f32 + 0.5) / SAMPLES as f32) / scale;
                    // Drawn for the right edge; the left one is its mirror image.
                    let x = match edge {
                        Edge::Right => x,
                        Edge::Left => TAB_SIZE.0 - x,
                    };
                    let Some(c) = colour_at(x, y, fill, border, arrow) else {
                        continue;
                    };
                    for (s, v) in sum.iter_mut().zip(c) {
                        *s += v as f32;
                    }
                    covered += 1.0;
                }
            }
            // Premultiplied: each channel is already scaled by the coverage.
            let ch = |v: f32| (v / n).round() as u8;
            out.extend_from_slice(&[ch(sum[2]), ch(sum[1]), ch(sum[0]), ch(covered * 255.0)]);
        }
    }
    out
}

/// What is at point (x, y) of a right-edge tab; None outside it.
fn colour_at(x: f32, y: f32, fill: [u8; 3], border: [u8; 3], arrow: [u8; 3]) -> Option<[u8; 3]> {
    if !in_shape(x, y, 0.0) {
        return None;
    }
    let on_arrow = ARROW
        .windows(2)
        .any(|s| distance_to_segment((x, y), s[0], s[1]) <= ARROW_HALF_WIDTH);
    Some(if on_arrow {
        arrow
    } else if in_shape(x, y, BORDER) {
        fill
    } else {
        border
    })
}

/// Inside the tab shrunk by `inset` on every side but the screen edge (the right one).
fn in_shape(x: f32, y: f32, inset: f32) -> bool {
    let (w, h) = TAB_SIZE;
    if x < inset || x >= w || y < inset || y >= h - inset {
        return false;
    }
    let r = RADIUS - inset;
    let cx = RADIUS;
    let cy = y.clamp(RADIUS, h - RADIUS);
    x >= cx || (x - cx).powi(2) + (y - cy).powi(2) <= r * r
}

fn distance_to_segment((px, py): (f32, f32), (ax, ay): (f32, f32), (bx, by): (f32, f32)) -> f32 {
    let (dx, dy) = (bx - ax, by - ay);
    let t = (((px - ax) * dx + (py - ay) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
    ((px - ax - t * dx).powi(2) + (py - ay - t * dy).powi(2)).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    const COLORS: TabColors = TabColors {
        bg: [0x2B, 0x2B, 0x2B],
        border: [0x45, 0x45, 0x45],
        accent: [0x8A, 0x97, 0xA8],
    };

    /// Size in pixels at `scale`.
    fn size(scale: f32) -> (u32, u32) {
        (
            (TAB_SIZE.0 * scale).round() as u32,
            (TAB_SIZE.1 * scale).round() as u32,
        )
    }

    /// (r, g, b, a) of a pixel, premultiplied as painted.
    fn px(pixels: &[u8], w: u32, x: u32, y: u32) -> (u8, u8, u8, u8) {
        let i = ((y * w + x) * 4) as usize;
        (pixels[i + 2], pixels[i + 1], pixels[i], pixels[i + 3])
    }

    fn rgb(c: [u8; 3]) -> (u8, u8, u8, u8) {
        (c[0], c[1], c[2], 255)
    }

    fn paint(edge: Edge, hovered: bool) -> (Vec<u8>, u32) {
        let (w, h) = size(2.0);
        (tab_pixels(w, h, 2.0, edge, COLORS, hovered), w)
    }

    #[test]
    fn one_bgra_pixel_per_point_at_any_scale() {
        for scale in [1.0, 1.25, 1.5, 2.0] {
            let (w, h) = size(scale);
            let p = tab_pixels(w, h, scale, Edge::Right, COLORS, false);
            assert_eq!(p.len(), (w * h * 4) as usize);
        }
    }

    #[test]
    fn rounded_on_the_screen_side_away_from_the_edge() {
        let (p, w) = paint(Edge::Right, false);
        let h = size(2.0).1;
        // The inner corners are cut away, the ones on the screen edge are square.
        assert_eq!(px(&p, w, 0, 0).3, 0);
        assert_eq!(px(&p, w, 0, h - 1).3, 0);
        assert_eq!(px(&p, w, w - 1, 0).3, 255);
        assert_eq!(px(&p, w, w - 1, h - 1).3, 255);
    }

    #[test]
    fn the_left_edge_tab_is_the_mirror_image() {
        let (right, w) = paint(Edge::Right, false);
        let (left, _) = paint(Edge::Left, false);
        let h = size(2.0).1;
        for y in 0..h {
            for x in 0..w {
                assert_eq!(
                    px(&left, w, x, y),
                    px(&right, w, w - 1 - x, y),
                    "at {x},{y}"
                );
            }
        }
    }

    #[test]
    fn a_border_on_the_inner_side_none_on_the_edge() {
        let (p, w) = paint(Edge::Right, false);
        let mid = size(2.0).1 / 2;
        assert_eq!(px(&p, w, 0, mid), rgb(COLORS.border));
        // The column at the screen edge, between top and bottom border: just the fill.
        assert_eq!(px(&p, w, w - 1, 10), rgb(COLORS.bg));
    }

    #[test]
    fn the_arrow_points_into_the_screen_in_the_accent() {
        let (p, w) = paint(Edge::Right, false);
        // The tip of the arrow, 4.5 pt from the inner side, at mid height.
        assert_eq!(px(&p, w, 9, 48), rgb(COLORS.accent));
        // Further right, at mid height, is the open side of the arrow: fill.
        assert_eq!(px(&p, w, 14, 48), rgb(COLORS.bg));
        assert_eq!(px(&p, w, 16, 10), rgb(COLORS.bg));
    }

    #[test]
    fn under_the_mouse_it_fills_with_the_accent() {
        let (p, w) = paint(Edge::Right, true);
        assert_eq!(px(&p, w, 16, 10), rgb(COLORS.accent));
        assert_eq!(px(&p, w, 0, size(2.0).1 / 2), rgb(COLORS.accent));
        assert_eq!(px(&p, w, 9, 48), rgb(COLORS.bg));
    }
}
