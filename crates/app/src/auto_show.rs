//! Turns hook events into Show/Hide off the UI thread, so plain mouse movement
//! never wakes egui. Show comes from the edge sensor (a file drag reached the
//! edge); hiding follows `EdgeTrigger`'s rules (button released, cursor off the shelf).

use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use shelve_core::{Config, EdgeEvent, EdgeTrigger};
use shelve_platform::{HookEvent, MouseSample, PlatformEvent};

use crate::layout::SharedRect;

/// Ticks while the shelf is up, so the hide delay fires even if the mouse stands still.
const TICK: Duration = Duration::from_millis(50);

/// `cursor` reads the mouse as it is now (`shelve_platform::cursor_state`). Each tick
/// asks it rather than trusting the hook's last event: the hook misses `SetCursorPos`
/// moves and goes silent if Windows removes it. Hook events still react at once.
pub fn run(
    events: Receiver<HookEvent>,
    cfg: Config,
    tx: Sender<PlatformEvent>,
    shared: SharedRect,
    cursor: impl Fn() -> MouseSample,
) {
    let start = Instant::now();
    let mut trigger = EdgeTrigger::new(cfg.hide_delay_ms);
    let mut shown = false;
    loop {
        let event = if shown {
            match events.recv_timeout(TICK) {
                Ok(e) => e,
                Err(RecvTimeoutError::Timeout) => HookEvent::Mouse(cursor()),
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
        let out = match event {
            HookEvent::FileDragAtEdge => trigger.show_now(),
            HookEvent::Mouse(s) => trigger.on_mouse(s.pos, s.button_down, now),
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
    use shelve_core::ScreenRect;
    use std::sync::mpsc;

    const SHELF: ScreenRect = ScreenRect {
        left: 1600,
        top: 300,
        right: 1920,
        bottom: 700,
    };

    #[test]
    fn hides_when_the_cursor_left_without_the_hook_seeing_it() {
        // Bug: a script moved the cursor with SetCursorPos, which the hook never sees.
        // The last hook event still had the cursor on the shelf, and it stayed open.
        let (hook_tx, hook_rx) = mpsc::channel();
        let (tx, rx) = mpsc::channel();
        let shared = SharedRect::default();
        *shared.lock().unwrap() = Some(SHELF);
        let cfg = Config {
            hide_delay_ms: 100,
            ..Config::default()
        };
        let away = || MouseSample {
            pos: (100, 100),
            button_down: false,
        };
        std::thread::spawn(move || run(hook_rx, cfg, tx, shared, away));
        hook_tx.send(HookEvent::FileDragAtEdge).unwrap();
        assert_eq!(rx.recv().unwrap(), PlatformEvent::EdgeShow);
        let on_shelf = MouseSample {
            pos: (1700, 500),
            button_down: false,
        };
        hook_tx.send(HookEvent::Mouse(on_shelf)).unwrap();
        let out = rx.recv_timeout(Duration::from_secs(3));
        assert_eq!(out, Ok(PlatformEvent::EdgeHide));
    }
}
