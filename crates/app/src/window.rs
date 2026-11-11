use eframe::egui;
use shelve_core::{Align, Edge, ScreenRect};
use shelve_platform::{icon_rgba, IconStyle};

const ICON_SIZE: u32 = 32;

/// Set on every window explicitly: eframe only icons the first window that is active, and
/// the shelf never is, so a reopened settings window would get the bare exe icon.
pub fn app_icon() -> std::sync::Arc<egui::IconData> {
    std::sync::Arc::new(egui::IconData {
        rgba: icon_rgba(IconStyle::Tile, ICON_SIZE),
        width: ICON_SIZE,
        height: ICON_SIZE,
    })
}

/// Shelf rectangle in physical pixels: `size` is in logical points, `scale` is the monitor DPI scale.
pub fn shelf_rect(
    edge: Edge,
    align: Align,
    screen: ScreenRect,
    size: (f32, f32),
    scale: f32,
) -> ScreenRect {
    let w = (size.0 * scale).round() as i32;
    let h = (size.1 * scale).round() as i32;
    let left = match edge {
        Edge::Left => screen.left,
        Edge::Right => screen.right - w,
    };
    let top = match align {
        Align::Top => screen.top,
        Align::Center => screen.top + (screen.bottom - screen.top - h) / 2,
        Align::Bottom => screen.bottom - h,
    };
    ScreenRect {
        left,
        top,
        right: left + w,
        bottom: top + h,
    }
}

/// Tab size in points: the strip left at the screen edge while a shelf with files is hidden.
const TAB_SIZE: (f32, f32) = (6.0, 48.0);

/// Where the tab goes: on the shelf's edge, at the shelf's height, physical pixels.
pub fn tab_rect(edge: Edge, align: Align, screen: ScreenRect, scale: f32) -> ScreenRect {
    shelf_rect(edge, align, screen, TAB_SIZE, scale)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: ScreenRect = ScreenRect {
        left: 0,
        top: 0,
        right: 1920,
        bottom: 1080,
    };
    const SIZE: (f32, f32) = (220.0, 400.0);

    #[test]
    fn right_center() {
        let r = shelf_rect(Edge::Right, Align::Center, SCREEN, SIZE, 1.0);
        assert_eq!(
            r,
            ScreenRect {
                left: 1700,
                top: 340,
                right: 1920,
                bottom: 740
            }
        );
    }

    #[test]
    fn left_top() {
        let r = shelf_rect(Edge::Left, Align::Top, SCREEN, SIZE, 1.0);
        assert_eq!((r.left, r.top), (0, 0));
    }

    #[test]
    fn right_bottom() {
        let r = shelf_rect(Edge::Right, Align::Bottom, SCREEN, SIZE, 1.0);
        assert_eq!((r.right, r.bottom), (1920, 1080));
    }

    #[test]
    fn scale_2x_doubles_size() {
        let r = shelf_rect(Edge::Right, Align::Center, SCREEN, SIZE, 2.0);
        assert_eq!((r.right - r.left, r.bottom - r.top), (440, 800));
    }

    #[test]
    fn follows_monitor_offset() {
        let second = ScreenRect {
            left: -1280,
            top: 100,
            right: 0,
            bottom: 1124,
        };
        let r = shelf_rect(Edge::Left, Align::Bottom, second, SIZE, 1.0);
        assert_eq!((r.left, r.bottom), (-1280, 1124));
    }

    #[test]
    fn the_tab_sits_on_the_shelf_edge_at_the_shelf_height() {
        let tab = |edge, align| tab_rect(edge, align, SCREEN, 1.0);
        let r = |left, top| ScreenRect {
            left,
            top,
            right: left + 6,
            bottom: top + 48,
        };
        assert_eq!(tab(Edge::Right, Align::Top), r(1914, 0));
        assert_eq!(tab(Edge::Right, Align::Center), r(1914, 516));
        assert_eq!(tab(Edge::Right, Align::Bottom), r(1914, 1032));
        assert_eq!(tab(Edge::Left, Align::Top), r(0, 0));
        assert_eq!(tab(Edge::Left, Align::Center), r(0, 516));
        assert_eq!(tab(Edge::Left, Align::Bottom), r(0, 1032));
    }

    #[test]
    fn the_tab_grows_with_the_monitor_scale() {
        let r = tab_rect(Edge::Right, Align::Center, SCREEN, 1.5);
        assert_eq!(
            r,
            ScreenRect {
                left: 1911,
                top: 504,
                right: 1920,
                bottom: 576
            }
        );
    }
}
