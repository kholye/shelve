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

/// Asked for by the user rather than seen in the mouse.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    /// The hotkey or a left click on the tray icon: shows a hidden shelf, hides a shown one.
    Toggle,
    /// A second copy of the program was started: show the shelf, never hide it.
    ShowExisting,
    /// The mouse rested on the tab at the screen edge, or clicked it.
    TabHovered,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Idle,
    /// Shown by a command; the cursor has not been on the shelf yet, so it does not hide.
    Waiting,
    Visible,
    Hiding {
        since: u64,
    },
}

/// Decides when the shelf shows and when it hides again. One rule for hiding: buttons
/// up, cursor off the shelf, `hide_delay_ms` passed. A shelf opened by the hotkey or the
/// tray starts that rule only once the cursor has been on it.
#[derive(Debug)]
pub struct EdgeTrigger {
    hide_delay_ms: u32,
    shelf_rect: Option<ScreenRect>,
    state: State,
}

impl EdgeTrigger {
    pub fn new(hide_delay_ms: u32) -> Self {
        Self {
            hide_delay_ms,
            shelf_rect: None,
            state: State::Idle,
        }
    }

    /// Area of the shelf window itself; the cursor over it keeps the shelf visible.
    pub fn set_shelf_rect(&mut self, rect: Option<ScreenRect>) {
        self.shelf_rect = rect;
    }

    /// `button_down`: the left or the right button is held. The mouse alone never shows
    /// the shelf: that is the edge sensor's job (`show_now`) or a command's.
    pub fn on_mouse(
        &mut self,
        pos: (i32, i32),
        button_down: bool,
        now_ms: u64,
    ) -> Option<EdgeEvent> {
        let on_shelf = self.shelf_rect.is_some_and(|r| r.contains(pos));
        let keep_visible = button_down || on_shelf;
        let (next, event) = match self.state {
            State::Idle => (State::Idle, None),
            State::Waiting if on_shelf => (State::Visible, None),
            State::Waiting => (State::Waiting, None),
            State::Visible | State::Hiding { .. } if keep_visible => (State::Visible, None),
            State::Visible => (State::Hiding { since: now_ms }, None),
            State::Hiding { since }
                if now_ms.saturating_sub(since) >= self.hide_delay_ms as u64 =>
            {
                (State::Idle, Some(EdgeEvent::Hide))
            }
            s @ State::Hiding { .. } => (s, None),
        };
        self.state = next;
        event
    }

    /// Shows at once (a file drag reached the edge), then hides by the usual rules.
    pub fn show_now(&mut self) -> Option<EdgeEvent> {
        let was_shown = self.state != State::Idle;
        self.state = State::Visible; // also cancels a pending hide
        (!was_shown).then_some(EdgeEvent::Show)
    }

    pub fn command(&mut self, command: Command) -> Option<EdgeEvent> {
        let (next, event) = match (self.state, command) {
            (State::Idle, Command::Toggle | Command::ShowExisting) => {
                (State::Waiting, Some(EdgeEvent::Show))
            }
            // As if the cursor had been on the shelf: brushed by accident, it goes again.
            (State::Idle, Command::TabHovered) => (State::Visible, Some(EdgeEvent::Show)),
            (_, Command::Toggle) => (State::Idle, Some(EdgeEvent::Hide)),
            (s, Command::ShowExisting | Command::TabHovered) => (s, None),
        };
        self.state = next;
        event
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HIDE: u32 = 600;
    const AWAY: (i32, i32) = (900, 500);
    const ON_SHELF: (i32, i32) = (1800, 500);
    const SHELF: ScreenRect = ScreenRect {
        left: 1700,
        top: 400,
        right: 1920,
        bottom: 700,
    };

    fn trigger() -> EdgeTrigger {
        let mut t = EdgeTrigger::new(HIDE);
        t.set_shelf_rect(Some(SHELF));
        t
    }

    /// Shown by a file dragged to the edge.
    fn shown_by_drag() -> EdgeTrigger {
        let mut t = trigger();
        assert_eq!(t.show_now(), Some(EdgeEvent::Show));
        t
    }

    /// Shown by the hotkey or the tray.
    fn shown_by_command() -> EdgeTrigger {
        let mut t = trigger();
        assert_eq!(t.command(Command::Toggle), Some(EdgeEvent::Show));
        t
    }

    /// Runs the hide delay from `at` with the cursor away; true if the shelf hid.
    fn hides_after_delay(t: &mut EdgeTrigger, at: u64) -> bool {
        assert_eq!(t.on_mouse(AWAY, false, at), None);
        assert_eq!(t.on_mouse(AWAY, false, at + HIDE as u64 - 1), None);
        t.on_mouse(AWAY, false, at + HIDE as u64) == Some(EdgeEvent::Hide)
    }

    #[test]
    fn mouse_alone_never_shows_the_shelf() {
        let mut t = trigger();
        assert_eq!(t.on_mouse((1919, 500), true, 0), None);
        assert_eq!(t.on_mouse((1919, 500), true, 10_000), None);
        assert_eq!(t.on_mouse(ON_SHELF, false, 20_000), None);
    }

    #[test]
    fn hide_after_release_and_delay() {
        let mut t = shown_by_drag();
        assert!(hides_after_delay(&mut t, 1000));
        assert_eq!(t.on_mouse(AWAY, false, 1000 + 2 * HIDE as u64), None);
    }

    #[test]
    fn stays_visible_while_button_held() {
        let mut t = shown_by_drag();
        assert_eq!(t.on_mouse(AWAY, true, 5000), None);
        assert_eq!(t.on_mouse(AWAY, true, 9000), None);
    }

    #[test]
    fn stays_visible_over_shelf_rect() {
        let mut t = shown_by_drag();
        assert_eq!(t.on_mouse(ON_SHELF, false, 1000), None);
        assert_eq!(t.on_mouse(ON_SHELF, false, 1000 + 10 * HIDE as u64), None);
        assert!(hides_after_delay(&mut t, 20_000));
    }

    #[test]
    fn returning_to_shelf_cancels_hide() {
        let mut t = shown_by_drag();
        assert_eq!(t.on_mouse(AWAY, false, 1000), None);
        assert_eq!(t.on_mouse(ON_SHELF, false, 1100), None);
        assert_eq!(t.on_mouse(ON_SHELF, false, 1000 + HIDE as u64), None);
    }

    #[test]
    fn show_now_shows_once_then_hides_after_release() {
        let mut t = shown_by_drag();
        assert_eq!(t.show_now(), None);
        assert_eq!(t.on_mouse(AWAY, true, 100), None);
        assert!(hides_after_delay(&mut t, 200));
        assert_eq!(t.show_now(), Some(EdgeEvent::Show));
    }

    #[test]
    fn opened_by_command_waits_for_the_cursor_however_long() {
        // Otherwise it would hide while the mouse is still on its way across the screen.
        let mut t = shown_by_command();
        assert_eq!(t.on_mouse(AWAY, false, 0), None);
        assert_eq!(t.on_mouse(AWAY, false, 10 * HIDE as u64), None);
        assert_eq!(t.on_mouse(AWAY, false, 1_000_000), None);
    }

    #[test]
    fn opened_by_command_hides_once_the_cursor_came_and_left() {
        let mut t = shown_by_command();
        assert_eq!(t.on_mouse(AWAY, false, 0), None);
        assert_eq!(t.on_mouse(ON_SHELF, false, 5000), None);
        assert!(hides_after_delay(&mut t, 6000));
    }

    #[test]
    fn show_existing_also_waits_for_the_cursor() {
        let mut t = trigger();
        assert_eq!(t.command(Command::ShowExisting), Some(EdgeEvent::Show));
        assert_eq!(t.on_mouse(AWAY, false, 10 * HIDE as u64), None);
    }

    #[test]
    fn toggle_hides_from_every_shown_state() {
        let mut waiting = shown_by_command();
        assert_eq!(waiting.command(Command::Toggle), Some(EdgeEvent::Hide));

        let mut visible = shown_by_drag();
        assert_eq!(visible.command(Command::Toggle), Some(EdgeEvent::Hide));

        let mut hiding = shown_by_drag();
        assert_eq!(hiding.on_mouse(AWAY, false, 100), None);
        assert_eq!(hiding.command(Command::Toggle), Some(EdgeEvent::Hide));

        // Hidden for good: no late Hide from the delay that was running.
        assert_eq!(hiding.on_mouse(AWAY, false, 100 + HIDE as u64), None);
        // And the next Toggle shows it again.
        assert_eq!(hiding.command(Command::Toggle), Some(EdgeEvent::Show));
    }

    #[test]
    fn show_commands_do_not_change_a_shown_shelf() {
        for cmd in [Command::ShowExisting, Command::TabHovered] {
            let mut waiting = shown_by_command();
            assert_eq!(waiting.command(cmd), None);
            // Still waiting: the cursor has not been on the shelf.
            assert_eq!(waiting.on_mouse(AWAY, false, 10 * HIDE as u64), None);

            let mut hiding = shown_by_drag();
            assert_eq!(hiding.on_mouse(AWAY, false, 100), None);
            assert_eq!(hiding.command(cmd), None);
            // The delay started at 100 keeps running.
            assert_eq!(
                hiding.on_mouse(AWAY, false, 100 + HIDE as u64),
                Some(EdgeEvent::Hide)
            );
        }
    }

    #[test]
    fn a_file_at_the_edge_turns_a_command_shelf_into_a_drag_shelf() {
        let mut t = shown_by_command();
        assert_eq!(t.show_now(), None);
        assert!(hides_after_delay(&mut t, 1000));
    }

    #[test]
    fn opened_by_the_tab_hides_after_the_cursor_leaves() {
        // The tab counts as the cursor having been on the shelf: brushed by accident, gone.
        let mut t = trigger();
        assert_eq!(t.command(Command::TabHovered), Some(EdgeEvent::Show));
        assert!(hides_after_delay(&mut t, 0));
    }

    #[test]
    fn a_held_right_button_keeps_a_command_shelf_open_too() {
        let mut t = shown_by_command();
        assert_eq!(t.on_mouse(ON_SHELF, false, 0), None);
        assert_eq!(t.on_mouse(AWAY, true, 100), None);
        assert_eq!(t.on_mouse(AWAY, true, 100 + 10 * HIDE as u64), None);
    }
}
