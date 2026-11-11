mod autostart;
mod drag_out;
mod file_icon;
mod hotkey;
mod mouse_hook;
mod shelf_window;
mod single_instance;
mod system_look;
mod tab;
mod tray;
pub use hotkey::try_hotkey;

use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use global_hotkey::{GlobalHotKeyEvent, HotKeyState};
use shelve_core::{Config, Lang, ScreenRect};
use tray_icon::menu::MenuEvent;
use tray_icon::{MouseButton, MouseButtonState, TrayIconEvent};
use windows::Win32::Foundation::{CloseHandle, LPARAM, WAIT_OBJECT_0, WPARAM};
use windows::Win32::System::Threading::{GetCurrentThreadId, INFINITE};
use windows::Win32::UI::Input::KeyboardAndMouse::GetDoubleClickTime;
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, MsgWaitForMultipleObjects, PeekMessageW, PostThreadMessageW,
    TranslateMessage, MSG, PM_REMOVE, QS_ALLINPUT, WM_APP, WM_QUIT,
};

use crate::{HotkeyStatus, PlatformEvent};
use system_look::KeyWatch;

/// Thread message: show or hide the tab where `PlatformHandle::set_tab` put it.
const WM_PLACE_TAB: u32 = WM_APP + 1;

/// Where the tab should be; None hides it.
type TabPlace = Arc<Mutex<Option<ScreenRect>>>;

pub use autostart::{is_autostart, set_autostart};
pub use drag_out::start_drag_out;
pub use file_icon::file_icon;
pub use mouse_hook::{start_mouse_hook, MouseHook};
pub use shelf_window::{
    cursor_state, hide_shelf_window, monitor_at_cursor, prepare_shelf_window, show_shelf_window,
    trim_memory,
};
pub use single_instance::{acquire_single_instance, SingleInstance};
pub use system_look::{animations_enabled, system_look, SystemLook};

/// Tray and hotkey live on their own thread with its own message loop, so the
/// UI (eframe/winit) never has to pump their messages. Drop stops the thread
/// and removes the tray icon.
pub struct PlatformHandle {
    thread_id: u32,
    thread: Option<JoinHandle<()>>,
    hotkey_status: HotkeyStatus,
    tab: TabPlace,
}

impl PlatformHandle {
    pub fn hotkey_status(&self) -> &HotkeyStatus {
        &self.hotkey_status
    }

    /// Shows the tab at `rect` (physical pixels), or hides it with None.
    pub fn set_tab(&self, rect: Option<ScreenRect>) {
        *self.tab.lock().unwrap() = rect;
        unsafe {
            let _ = PostThreadMessageW(self.thread_id, WM_PLACE_TAB, WPARAM(0), LPARAM(0));
        }
    }
}

impl Drop for PlatformHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
        }
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

pub fn start(cfg: &Config, tx: Sender<PlatformEvent>) -> anyhow::Result<PlatformHandle> {
    let (init_tx, init_rx) = mpsc::sync_channel(1);
    let (hotkey, lang) = (cfg.hotkey.clone(), cfg.lang);
    let tab = TabPlace::default();
    let tab_place = tab.clone();
    let thread = thread::Builder::new()
        .name("platform".into())
        .spawn(move || run(&hotkey, lang, tx, tab_place, init_tx))?;
    let (thread_id, hotkey_status) = init_rx.recv()??;
    Ok(PlatformHandle {
        thread_id,
        thread: Some(thread),
        hotkey_status,
        tab,
    })
}

type Init = anyhow::Result<(u32, HotkeyStatus)>;

/// Why `MsgWaitForMultipleObjects` over `handles` handles returned.
#[derive(Debug, PartialEq)]
enum Woke {
    Handle(usize),
    Messages,
    Failed,
}

fn woke(wait: u32, handles: usize) -> Woke {
    match wait.wrapping_sub(WAIT_OBJECT_0.0) as usize {
        i if i < handles => Woke::Handle(i),
        i if i == handles => Woke::Messages,
        _ => Woke::Failed,
    }
}

fn run(
    hotkey: &str,
    lang: Lang,
    tx: Sender<PlatformEvent>,
    tab_place: TabPlace,
    init: mpsc::SyncSender<Init>,
) {
    // Everything below must be created on this thread: its windows get their
    // messages only from this thread's loop.
    let (_manager, hotkey_id, status) = hotkey::register(hotkey);
    let mut taskbar_dark = system_look::system_look().taskbar_dark;
    let tray = match tray::build(hotkey, &status, taskbar_dark, lang) {
        Ok(t) => t,
        Err(e) => return drop(init.send(Err(e))),
    };
    // Without it the shelf still comes back by the hotkey and the tray.
    let tab = tab::Tab::create(tx.clone())
        .inspect_err(|e| println!("tab window failed: {e}"))
        .ok();
    let show_event = match single_instance::show_event() {
        Ok(h) => h,
        Err(e) => return drop(init.send(Err(e.into()))),
    };
    let _ = init.send(Ok((unsafe { GetCurrentThreadId() }, status)));

    // Theme, taskbar colour and accent live in the registry; Windows signals these
    // events on a change, so nothing is polled. A key that cannot be watched is skipped.
    let watches: Vec<KeyWatch> = system_look::WATCHED_KEYS
        .into_iter()
        .filter_map(KeyWatch::new)
        .collect();
    let mut handles = vec![show_event];
    handles.extend(watches.iter().map(KeyWatch::event));

    let mut tray_clicks = tray::ClickFilter::default();
    let started = std::time::Instant::now();
    let mut msg = MSG::default();
    'outer: loop {
        let wait =
            unsafe { MsgWaitForMultipleObjects(Some(&handles), false, INFINITE, QS_ALLINPUT) };
        let event = match woke(wait.0, handles.len()) {
            Woke::Handle(0) => Some(PlatformEvent::ShowExisting),
            Woke::Handle(i) => {
                watches[i - 1].arm();
                let dark = system_look::system_look().taskbar_dark;
                if dark != taskbar_dark {
                    taskbar_dark = dark;
                    let _ = tray.set_icon(tray::icon(dark).ok());
                }
                if let Some(tab) = &tab {
                    tab.repaint(); // the accent colour may have changed
                }
                Some(PlatformEvent::ThemeChanged)
            }
            Woke::Messages => None,
            Woke::Failed => {
                // A failed wait returns at once. Tray and hotkey still work through the
                // messages pumped below; the pause keeps the loop from eating a core.
                thread::sleep(std::time::Duration::from_millis(100));
                None
            }
        };
        if event.is_some_and(|e| tx.send(e).is_err()) {
            break;
        }
        while unsafe { PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE) }.as_bool() {
            if msg.message == WM_QUIT {
                break 'outer;
            }
            if msg.message == WM_PLACE_TAB {
                if let Some(tab) = &tab {
                    tab.place(*tab_place.lock().unwrap());
                }
                continue;
            }
            unsafe {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        // The crates queue their events during DispatchMessage above.
        let mut out = Vec::new();
        while let Ok(e) = GlobalHotKeyEvent::receiver().try_recv() {
            if Some(e.id) == hotkey_id && e.state == HotKeyState::Pressed {
                out.push(PlatformEvent::Hotkey);
            }
        }
        while let Ok(e) = MenuEvent::receiver().try_recv() {
            out.extend(tray::event_for(&e.id));
        }
        // Every tray event is drained, so the unbounded channel does not grow. DoubleClick
        // comes on its own and is skipped; the filter drops its second Up.
        while let Ok(e) = TrayIconEvent::receiver().try_recv() {
            let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = e
            else {
                continue;
            };
            let now = started.elapsed().as_millis() as u64;
            if tray_clicks.accept(now, unsafe { GetDoubleClickTime() } as u64) {
                out.push(PlatformEvent::TrayToggle);
            }
        }
        if out.into_iter().any(|e| tx.send(e).is_err()) {
            break;
        }
    }
    unsafe {
        let _ = CloseHandle(show_event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::Foundation::WAIT_FAILED;

    #[test]
    fn a_wait_result_names_the_signalled_handle_or_messages() {
        assert_eq!(woke(WAIT_OBJECT_0.0, 3), Woke::Handle(0));
        assert_eq!(woke(WAIT_OBJECT_0.0 + 2, 3), Woke::Handle(2));
        assert_eq!(woke(WAIT_OBJECT_0.0 + 3, 3), Woke::Messages);
    }

    #[test]
    fn a_failed_wait_is_told_apart_from_messages() {
        // Bug: WAIT_FAILED returns at once, and the loop took it for "messages": it spun.
        assert_eq!(woke(WAIT_FAILED.0, 3), Woke::Failed);
    }
}
