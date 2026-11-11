//! Turns hook events into Show/Hide off the UI thread, so plain mouse movement
//! never wakes egui. Show comes from the edge sensor (a file drag reached the
//! edge); hiding follows `EdgeTrigger`'s rules (button released, cursor off the shelf).

use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use shelve_core::{Config, EdgeEvent, EdgeTrigger, ScreenRect};
use shelve_platform::{HookEvent, MouseSample, PlatformEvent};

use crate::layout::SharedRect;

/// Ticks while the shelf is up, so the hide delay fires even if the mouse stands still.
const TICK: Duration = Duration::from_millis(50);

pub fn run(
    events: Receiver<HookEvent>,
    cfg: Config,
    tx: Sender<PlatformEvent>,
    shared: SharedRect,
) {
    let start = Instant::now();
    let unused = ScreenRect {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    // Zone 0: the dwell path never fires; showing is the sensor's job (show_now).
    let mut trigger = EdgeTrigger::new(cfg.edge, unused, 0, 0, cfg.hide_delay_ms);
    let mut last = MouseSample {
        pos: (0, 0),
        left_down: false,
    };
    let mut shown = false;
    loop {
        let event = if shown {
            match events.recv_timeout(TICK) {
                Ok(e) => e,
                Err(RecvTimeoutError::Timeout) => HookEvent::Mouse(last),
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
            HookEvent::Mouse(s) => {
                last = s;
                trigger.on_mouse(s.pos, s.left_down, now)
            }
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
