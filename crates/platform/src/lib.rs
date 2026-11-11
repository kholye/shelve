#[cfg(windows)]
mod windows;

mod icon;
pub use icon::{icon_rgba, IconStyle};
pub use shelve_core::HotkeyError;

#[cfg(windows)]
pub use crate::windows::{
    acquire_single_instance, animations_enabled, cursor_state, file_icon, hide_shelf_window,
    is_autostart, monitor_at_cursor, prepare_shelf_window, set_autostart, show_shelf_window, start,
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
    /// Kept as a kind, not a library message, so it is shown in the interface language.
    Failed(HotkeyError),
}

/// How a drag-out ended. Whether the file was copied or moved is not known here: Explorer
/// often moves it after the drop, and its answer about that is unreliable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragOutcome {
    /// The button was let go over a window. Copied, moved, or refused: Explorer reports
    /// no effect after a move too, so whether the file left is found out by watching it.
    Dropped,
    Cancelled,
}

/// The mouse button a row is dragged off the shelf with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragButton {
    /// Moves; Ctrl makes a copy.
    Left,
    /// Explorer shows its Copy / Move menu on the drop.
    Right,
}

/// Where the mouse is, in physical screen pixels: from the hook or read directly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MouseSample {
    pub pos: (i32, i32),
    /// The left or the right button is held.
    pub button_down: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookEvent {
    Mouse(MouseSample),
    /// A drag carrying files (not text or links) reached the screen edge.
    FileDragAtEdge,
}
