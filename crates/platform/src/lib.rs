#[cfg(windows)]
mod windows;

mod icon;
pub use icon::{icon_rgba, IconStyle};

#[cfg(windows)]
pub use crate::windows::{
    acquire_single_instance, animations_enabled, file_icon, hide_shelf_window, is_autostart,
    monitor_at_cursor, prepare_shelf_window, set_autostart, show_shelf_window, start,
    start_drag_out, start_mouse_hook, system_look, trim_memory, try_hotkey, MouseHook,
    PlatformHandle, SingleInstance, SystemLook,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlatformEvent {
    Hotkey,
    TrayToggle,
    TrayClear,
    TraySettings,
    TrayQuit,
    ShowExisting,
    EdgeShow,
    EdgeHide,
    /// Windows theme, taskbar colour or accent colour changed.
    ThemeChanged,
}

/// Result of registering the global hotkey, shown in the tray menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HotkeyStatus {
    Registered,
    Failed(String),
}

/// How a drag-out ended. The shelf only offers copy, so there is no "moved".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragOutcome {
    Copied,
    Cancelled,
}

/// One mouse event from the low-level hook, in physical screen pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MouseSample {
    pub pos: (i32, i32),
    pub left_down: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookEvent {
    Mouse(MouseSample),
    /// A drag carrying files (not text or links) reached the screen edge.
    FileDragAtEdge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyError {
    /// global-hotkey has no code for this key.
    Unsupported,
    /// Another program holds it (Windows does not say which).
    Taken,
}
