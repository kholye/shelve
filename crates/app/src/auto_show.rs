//! The one place that decides whether the shelf is shown. Hook events (a file drag
//! reached the edge, the mouse moved) and commands (hotkey, tray, tab, a second copy
//! started) all come here; the thread answers EdgeShow / EdgeHide and the app obeys.
//! Off the UI thread, so plain mouse movement never wakes egui.

use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use shelve_core::{Command, EdgeEvent, EdgeTrigger};
use shelve_platform::{HookEvent, MouseSample, PlatformEvent};

use crate::layout::SharedRect;

/// Ticks while the shelf is up, so the hide delay fires even if the mouse stands still.
const TICK: Duration = Duration::from_millis(50);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoShowInput {
    Hook(HookEvent),
    Command(Command),
}

/// Lives as long as the program. `cursor` reads the mouse as it is now
/// (`shelve_platform::cursor_state`). Each tick asks it rather than trusting the hook's
/// last event: the hook misses `SetCursorPos` moves, goes silent if Windows removes it,
/// and is not running at all with "show while dragging" off. Hook events still react at
/// once. While the shelf is hidden the thread sleeps until something arrives.
pub fn run(
    events: Receiver<AutoShowInput>,
    hide_delay_ms: u32,
    tx: Sender<PlatformEvent>,
    shared: SharedRect,
    cursor: impl Fn() -> MouseSample,
) {
    let start = Instant::now();
    let mut trigger = EdgeTrigger::new(hide_delay_ms);
    let mut shown = false;
    loop {
        let input = if shown {
            match events.recv_timeout(TICK) {
                Ok(e) => e,
                Err(RecvTimeoutError::Timeout) => AutoShowInput::Hook(HookEvent::Mouse(cursor())),
                Err(RecvTimeoutError::Disconnected) => return,
            }
        } else {
            match events.recv() {
                Ok(e) => e,
                Err(_) => return,
            }
        };
        let now = start.elapsed().as_millis() as u64;
        if shown {
            // The UI thread owns the panel: it grows with the items and may sit on any
            // monitor. Briefly None right after Show; the hide delay covers that.
            trigger.set_shelf_rect(*shared.lock().unwrap());
        }
        let out = match input {
            AutoShowInput::Hook(HookEvent::FileDragAtEdge) => trigger.show_now(),
            AutoShowInput::Hook(HookEvent::Mouse(s)) => trigger.on_mouse(s.pos, s.button_down, now),
            AutoShowInput::Command(c) => trigger.command(c),
        };
        let out = match out {
            Some(EdgeEvent::Show) => {
                shown = true;
                PlatformEvent::EdgeShow
            }
            Some(EdgeEvent::Hide) => {
                trigger.set_shelf_rect(None);
                shown = false;
                PlatformEvent::EdgeHide
            }
            None => continue,
        };
        if tx.send(out).is_err() {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shelve_core::{Command, ScreenRect};
    use std::sync::{mpsc, Arc, Mutex};

    const SHELF: ScreenRect = ScreenRect {
        left: 1600,
        top: 300,
        right: 1920,
        bottom: 700,
    };
    const AWAY: MouseSample = MouseSample {
        pos: (100, 100),
        button_down: false,
    };
    const ON_SHELF: MouseSample = MouseSample {
        pos: (1700, 500),
        button_down: false,
    };

    /// Starts the thread with a 100 ms hide delay and a cursor the test moves.
    fn start() -> (
        Sender<AutoShowInput>,
        Receiver<PlatformEvent>,
        Arc<Mutex<MouseSample>>,
    ) {
        let (input, events) = mpsc::channel();
        let (tx, rx) = mpsc::channel();
        let shared = SharedRect::default();
        *shared.lock().unwrap() = Some(SHELF);
        let cursor = Arc::new(Mutex::new(AWAY));
        let read = cursor.clone();
        std::thread::spawn(move || run(events, 100, tx, shared, move || *read.lock().unwrap()));
        (input, rx, cursor)
    }

    const WAIT: Duration = Duration::from_secs(3);

    #[test]
    fn hides_when_the_cursor_left_without_the_hook_seeing_it() {
        // Bug: a script moved the cursor with SetCursorPos, which the hook never sees.
        // The last hook event still had the cursor on the shelf, and it stayed open.
        let (input, rx, _cursor) = start();
        input
            .send(AutoShowInput::Hook(HookEvent::FileDragAtEdge))
            .unwrap();
        assert_eq!(rx.recv_timeout(WAIT), Ok(PlatformEvent::EdgeShow));
        input
            .send(AutoShowInput::Hook(HookEvent::Mouse(ON_SHELF)))
            .unwrap();
        assert_eq!(rx.recv_timeout(WAIT), Ok(PlatformEvent::EdgeHide));
    }

    #[test]
    fn the_hotkey_shows_and_hides_through_the_thread() {
        let (input, rx, _cursor) = start();
        input.send(AutoShowInput::Command(Command::Toggle)).unwrap();
        assert_eq!(rx.recv_timeout(WAIT), Ok(PlatformEvent::EdgeShow));
        input.send(AutoShowInput::Command(Command::Toggle)).unwrap();
        assert_eq!(rx.recv_timeout(WAIT), Ok(PlatformEvent::EdgeHide));
    }

    #[test]
    fn opened_by_the_hotkey_it_hides_after_a_visit_with_no_hook_at_all() {
        // Bug: the hotkey-shown shelf was the app's own business; a file brought to the
        // edge then hid it. Now the thread knows, and hides it by the one rule.
        let (input, rx, cursor) = start();
        input.send(AutoShowInput::Command(Command::Toggle)).unwrap();
        assert_eq!(rx.recv_timeout(WAIT), Ok(PlatformEvent::EdgeShow));
        // Far past the hide delay, the cursor never on the shelf: it stays.
        assert!(rx.recv_timeout(Duration::from_millis(400)).is_err());
        *cursor.lock().unwrap() = ON_SHELF;
        std::thread::sleep(Duration::from_millis(200));
        *cursor.lock().unwrap() = AWAY;
        assert_eq!(rx.recv_timeout(WAIT), Ok(PlatformEvent::EdgeHide));
    }
}
