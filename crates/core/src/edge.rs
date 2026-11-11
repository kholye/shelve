use crate::config::Edge;

/// Screen rectangle in physical pixels; `right`/`bottom` are exclusive, as in Win32 `RECT`.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ScreenRect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl ScreenRect {
    fn contains(&self, (x, y): (i32, i32)) -> bool {
        x >= self.left && x < self.right && y >= self.top && y < self.bottom
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeEvent {
    Show,
    Hide,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Idle,
    Dwelling { since: u64 },
    Visible,
    Hiding { since: u64 },
}

/// Decides when the shelf appears at the screen edge during a drag and when it hides again.
#[derive(Debug)]
pub struct EdgeTrigger {
    edge: Edge,
    screen: ScreenRect,
    zone_px: u32,
    dwell_ms: u32,
    hide_delay_ms: u32,
    shelf_rect: Option<ScreenRect>,
    state: State,
}

impl EdgeTrigger {
    pub fn new(
        edge: Edge,
        screen: ScreenRect,
        zone_px: u32,
        dwell_ms: u32,
        hide_delay_ms: u32,
    ) -> Self {
        Self {
            edge,
            screen,
            zone_px,
            dwell_ms,
            hide_delay_ms,
            shelf_rect: None,
            state: State::Idle,
        }
    }

    /// Area of the shelf window itself; the cursor over it keeps the shelf visible.
    pub fn set_shelf_rect(&mut self, rect: Option<ScreenRect>) {
        self.shelf_rect = rect;
    }

    /// `button_down`: the left or the right button is held.
    pub fn on_mouse(
        &mut self,
        pos: (i32, i32),
        button_down: bool,
        now_ms: u64,
    ) -> Option<EdgeEvent> {
        let elapsed = |since: u64| now_ms.saturating_sub(since);
        let keep_visible = button_down || self.shelf_rect.is_some_and(|r| r.contains(pos));
        let (next, event) = match self.state {
            State::Idle | State::Dwelling { .. } if !(button_down && self.in_zone(pos)) => {
                (State::Idle, None)
            }
            State::Idle => (State::Dwelling { since: now_ms }, None),
            State::Dwelling { since } if elapsed(since) >= self.dwell_ms as u64 => {
                (State::Visible, Some(EdgeEvent::Show))
            }
            s @ State::Dwelling { .. } => (s, None),
            State::Visible | State::Hiding { .. } if keep_visible => (State::Visible, None),
            State::Visible => (State::Hiding { since: now_ms }, None),
            State::Hiding { since } if elapsed(since) >= self.hide_delay_ms as u64 => {
                (State::Idle, Some(EdgeEvent::Hide))
            }
            s @ State::Hiding { .. } => (s, None),
        };
        self.state = next;
        event
    }

    /// Shows at once (a file drag reached the edge), then hides by the usual rules.
    pub fn show_now(&mut self) -> Option<EdgeEvent> {
        let was_shown = matches!(self.state, State::Visible | State::Hiding { .. });
        self.state = State::Visible; // also cancels a pending hide
        (!was_shown).then_some(EdgeEvent::Show)
    }

    fn in_zone(&self, (x, y): (i32, i32)) -> bool {
        let zone = self.zone_px as i32;
        let near_edge = match self.edge {
            Edge::Left => x < self.screen.left + zone,
            Edge::Right => x >= self.screen.right - zone,
        };
        near_edge && self.screen.contains((x, y))
    }
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
    const ZONE: u32 = 4;
    const DWELL: u32 = 250;
    const HIDE: u32 = 600;
    const RIGHT_EDGE: (i32, i32) = (1919, 500);
    const AWAY: (i32, i32) = (900, 500);

    fn trigger(edge: Edge) -> EdgeTrigger {
        EdgeTrigger::new(edge, SCREEN, ZONE, DWELL, HIDE)
    }

    fn shown_right() -> EdgeTrigger {
        let mut t = trigger(Edge::Right);
        assert_eq!(t.on_mouse(RIGHT_EDGE, true, 0), None);
        assert_eq!(
            t.on_mouse(RIGHT_EDGE, true, DWELL as u64),
            Some(EdgeEvent::Show)
        );
        t
    }

    #[test]
    fn no_show_without_button() {
        let mut t = trigger(Edge::Right);
        assert_eq!(t.on_mouse(RIGHT_EDGE, false, 0), None);
        assert_eq!(t.on_mouse(RIGHT_EDGE, false, 1000), None);
    }

    #[test]
    fn show_after_dwell() {
        let mut t = shown_right();
        assert_eq!(t.on_mouse(RIGHT_EDGE, true, DWELL as u64 + 10), None);
    }

    #[test]
    fn no_show_before_dwell() {
        let mut t = trigger(Edge::Right);
        assert_eq!(t.on_mouse(RIGHT_EDGE, true, 0), None);
        assert_eq!(t.on_mouse(RIGHT_EDGE, true, DWELL as u64 - 1), None);
    }

    #[test]
    fn no_show_if_leaves_zone_before_dwell() {
        let mut t = trigger(Edge::Right);
        assert_eq!(t.on_mouse(RIGHT_EDGE, true, 0), None);
        assert_eq!(t.on_mouse(AWAY, true, 100), None);
        assert_eq!(t.on_mouse(RIGHT_EDGE, true, DWELL as u64), None);
        // Dwell restarts from the re-entry at DWELL.
        assert_eq!(
            t.on_mouse(RIGHT_EDGE, true, 2 * DWELL as u64),
            Some(EdgeEvent::Show)
        );
    }

    #[test]
    fn hide_after_release_and_delay() {
        let mut t = shown_right();
        let released = 1000;
        assert_eq!(t.on_mouse(AWAY, false, released), None);
        assert_eq!(t.on_mouse(AWAY, false, released + HIDE as u64 - 1), None);
        assert_eq!(
            t.on_mouse(AWAY, false, released + HIDE as u64),
            Some(EdgeEvent::Hide)
        );
        assert_eq!(t.on_mouse(AWAY, false, released + 2 * HIDE as u64), None);
    }

    #[test]
    fn stays_visible_while_button_held() {
        let mut t = shown_right();
        assert_eq!(t.on_mouse(AWAY, true, 5000), None);
        assert_eq!(t.on_mouse(AWAY, true, 9000), None);
    }

    #[test]
    fn stays_visible_over_shelf_rect() {
        let mut t = shown_right();
        t.set_shelf_rect(Some(ScreenRect {
            left: 1700,
            top: 400,
            right: 1920,
            bottom: 700,
        }));
        assert_eq!(t.on_mouse((1800, 500), false, 1000), None);
        assert_eq!(
            t.on_mouse((1800, 500), false, 1000 + 10 * HIDE as u64),
            None
        );
        // Leaving the shelf starts the hide delay.
        assert_eq!(t.on_mouse(AWAY, false, 20_000), None);
        assert_eq!(
            t.on_mouse(AWAY, false, 20_000 + HIDE as u64),
            Some(EdgeEvent::Hide)
        );
    }

    #[test]
    fn returning_to_shelf_cancels_hide() {
        let mut t = shown_right();
        t.set_shelf_rect(Some(ScreenRect {
            left: 1700,
            top: 400,
            right: 1920,
            bottom: 700,
        }));
        assert_eq!(t.on_mouse(AWAY, false, 1000), None);
        assert_eq!(t.on_mouse((1800, 500), false, 1100), None);
        assert_eq!(t.on_mouse((1800, 500), false, 1000 + HIDE as u64), None);
    }

    #[test]
    fn left_edge_mirrors_right() {
        let mut t = trigger(Edge::Left);
        assert_eq!(t.on_mouse(RIGHT_EDGE, true, 0), None);
        assert_eq!(t.on_mouse(RIGHT_EDGE, true, 1000), None);
        let left_edge = (0, 500);
        assert_eq!(t.on_mouse(left_edge, true, 2000), None);
        assert_eq!(
            t.on_mouse(left_edge, true, 2000 + DWELL as u64),
            Some(EdgeEvent::Show)
        );
    }

    #[test]
    fn zone_width_is_respected() {
        let mut t = trigger(Edge::Right);
        let just_outside = (1920 - ZONE as i32 - 1, 500);
        assert_eq!(t.on_mouse(just_outside, true, 0), None);
        assert_eq!(t.on_mouse(just_outside, true, 1000), None);
        let inner_edge_of_zone = (1920 - ZONE as i32, 500);
        assert_eq!(t.on_mouse(inner_edge_of_zone, true, 2000), None);
        assert_eq!(
            t.on_mouse(inner_edge_of_zone, true, 2000 + DWELL as u64),
            Some(EdgeEvent::Show)
        );
    }

    #[test]
    fn show_now_shows_once_then_hides_after_release() {
        let mut t = trigger(Edge::Right);
        assert_eq!(t.show_now(), Some(EdgeEvent::Show));
        assert_eq!(t.show_now(), None);
        assert_eq!(t.on_mouse(AWAY, true, 100), None);
        assert_eq!(t.on_mouse(AWAY, false, 200), None);
        assert_eq!(
            t.on_mouse(AWAY, false, 200 + HIDE as u64),
            Some(EdgeEvent::Hide)
        );
        assert_eq!(t.show_now(), Some(EdgeEvent::Show));
    }
}
