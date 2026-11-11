//! Sizes of the floating shelf panel and of the transparent window around it.
//! Points are logical (DPI-independent); `ScreenRect`s are physical pixels.

use shelve_core::{Align, Edge, ScreenRect};

pub const PANEL_W: f32 = 272.0;
pub const ROW_H: f32 = 44.0;
pub const HEADER_H: f32 = 36.0;
/// Height of the empty shelf's "drop files here" plate.
pub const EMPTY_H: f32 = 132.0;
pub const PAD_BOTTOM: f32 = 4.0;
/// From the panel to the screen edge.
pub const GAP: f32 = 8.0;
/// Transparent room around the panel for its shadow.
pub const SHADOW: f32 = 12.0;
pub const RADIUS: f32 = 8.0;
/// The panel never takes more than this share of the work area's height; then it scrolls.
const MAX_SCREEN_SHARE: f32 = 0.7;

/// Where the panel (with its gap to the screen edge) currently is on screen, for the
/// thread that hides the shelf when the cursor leaves it. None while the shelf is hidden.
pub type SharedRect = std::sync::Arc<std::sync::Mutex<Option<ScreenRect>>>;

/// Panel height in points. `rows` is fractional so a row can grow in gradually.
pub fn panel_height(rows: f32, work_h: f32) -> f32 {
    if rows <= 0.0 {
        return EMPTY_H;
    }
    (HEADER_H + rows * ROW_H + PAD_BOTTOM).min((MAX_SCREEN_SHARE * work_h).floor())
}

/// Window size in points: the panel, the gap on the screen-edge side, shadow room elsewhere.
pub fn window_size(panel_h: f32) -> (f32, f32) {
    (PANEL_W + GAP + SHADOW, panel_h + 2.0 * SHADOW)
}

/// The panel inside its window, in physical pixels.
pub fn panel_rect(window: ScreenRect, edge: Edge, scale: f32) -> ScreenRect {
    let gap = (GAP * scale).round() as i32;
    let shadow = (SHADOW * scale).round() as i32;
    let (left, right) = match edge {
        Edge::Left => (window.left + gap, window.right - shadow),
        Edge::Right => (window.left + shadow, window.right - gap),
    };
    ScreenRect {
        left,
        top: window.top + shadow,
        right,
        bottom: window.bottom - shadow,
    }
}

/// How far below its usual place (shadow room from the window's top) the panel starts
/// when the window has room for a taller panel: it stays anchored like the shelf.
pub fn panel_top_offset(align: Align, window_panel_h: f32, panel_h: f32) -> f32 {
    let spare = (window_panel_h - panel_h).max(0.0);
    match align {
        Align::Top => 0.0,
        Align::Center => spare / 2.0,
        Align::Bottom => spare,
    }
}

/// The panel in physical pixels when the window has room for a `window_panel_h` tall
/// panel but the panel is `panel_h` tall right now (its height is gliding).
pub fn panel_rect_in(
    window: ScreenRect,
    edge: Edge,
    align: Align,
    window_panel_h: f32,
    panel_h: f32,
    scale: f32,
) -> ScreenRect {
    let full = panel_rect(window, edge, scale);
    let top = full.top + (panel_top_offset(align, window_panel_h, panel_h) * scale).round() as i32;
    ScreenRect {
        top,
        bottom: top + (panel_h * scale).round() as i32,
        ..full
    }
}

/// The panel plus the gap between it and the screen edge: where the cursor counts as
/// "on the shelf". A cursor pushed against the edge rests in that gap.
pub fn hover_rect(panel: ScreenRect, window: ScreenRect, edge: Edge) -> ScreenRect {
    match edge {
        Edge::Left => ScreenRect {
            left: window.left,
            ..panel
        },
        Edge::Right => ScreenRect {
            right: window.right,
            ..panel
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::window::shelf_rect;

    const SCREEN: ScreenRect = ScreenRect {
        left: 0,
        top: 0,
        right: 1920,
        bottom: 1080,
    };

    #[test]
    fn a_shorter_panel_in_a_taller_window_stays_anchored_like_the_shelf() {
        // Window with room for a 172 pt panel, panel currently 84 pt tall.
        let w = shelf_rect(Edge::Right, Align::Center, SCREEN, (292.0, 196.0), 1.0);
        let at = |align| {
            let r = panel_rect_in(w, Edge::Right, align, 172.0, 84.0, 1.0);
            (r.left, r.top, r.right, r.bottom)
        };
        assert_eq!(at(Align::Top), (1640, 454, 1912, 538));
        assert_eq!(at(Align::Center), (1640, 498, 1912, 582));
        assert_eq!(at(Align::Bottom), (1640, 542, 1912, 626));
    }

    #[test]
    fn the_gap_at_the_screen_edge_counts_as_the_shelf_for_auto_hide() {
        // Bug: a cursor resting at the very edge of the screen was "off the shelf".
        let w = shelf_rect(Edge::Right, Align::Center, SCREEN, (292.0, 196.0), 1.0);
        let r = hover_rect(panel_rect(w, Edge::Right, 1.0), w, Edge::Right);
        assert_eq!((r.left, r.top, r.right, r.bottom), (1640, 454, 1920, 626));
        let w = shelf_rect(Edge::Left, Align::Top, SCREEN, (292.0, 196.0), 1.0);
        let r = hover_rect(panel_rect(w, Edge::Left, 1.0), w, Edge::Left);
        assert_eq!((r.left, r.top, r.right, r.bottom), (0, 12, 280, 184));
    }

    #[test]
    fn a_panel_that_fills_its_window_is_the_plain_panel_rect() {
        let w = shelf_rect(Edge::Left, Align::Bottom, SCREEN, (292.0, 196.0), 1.0);
        assert_eq!(
            panel_rect_in(w, Edge::Left, Align::Bottom, 172.0, 172.0, 1.0),
            panel_rect(w, Edge::Left, 1.0)
        );
    }

    #[test]
    fn empty_shelf_is_a_small_plate() {
        assert_eq!(panel_height(0.0, 1040.0), 132.0);
    }

    #[test]
    fn height_grows_with_rows() {
        assert_eq!(panel_height(1.0, 1040.0), 84.0);
        assert_eq!(panel_height(3.0, 1040.0), 172.0);
    }

    #[test]
    fn height_stops_at_70_percent_of_the_work_area() {
        assert_eq!(panel_height(1000.0, 1040.0), 728.0);
        assert_eq!(panel_height(300.0, 1040.0), 728.0);
    }

    #[test]
    fn window_is_the_panel_plus_gap_and_shadow_room() {
        assert_eq!(window_size(172.0), (292.0, 196.0));
    }

    #[test]
    fn panel_sits_a_gap_away_from_the_right_edge() {
        let w = shelf_rect(Edge::Right, Align::Center, SCREEN, (292.0, 196.0), 1.0);
        assert_eq!((w.left, w.top, w.right, w.bottom), (1628, 442, 1920, 638));
        assert_eq!(
            panel_rect(w, Edge::Right, 1.0),
            ScreenRect {
                left: 1640,
                top: 454,
                right: 1912,
                bottom: 626
            }
        );
    }

    #[test]
    fn panel_sits_a_gap_away_from_the_left_edge() {
        let wl = shelf_rect(Edge::Left, Align::Top, SCREEN, (292.0, 196.0), 1.0);
        assert_eq!(
            panel_rect(wl, Edge::Left, 1.0),
            ScreenRect {
                left: 8,
                top: 12,
                right: 280,
                bottom: 184
            }
        );
    }

    #[test]
    fn panel_rect_scales_with_dpi() {
        let w = shelf_rect(Edge::Right, Align::Top, SCREEN, (292.0, 196.0), 1.5);
        assert_eq!(
            panel_rect(w, Edge::Right, 1.5),
            ScreenRect {
                left: 1920 - 12 - 408,
                top: 18,
                right: 1920 - 12,
                bottom: 294 - 18
            }
        );
    }
}
