use tray_icon::menu::{Menu, MenuId, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};
use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSMICON};

use crate::icon::{icon_rgba, IconStyle};
use crate::{HotkeyStatus, PlatformEvent};
use shelve_core::Lang;

const TOGGLE: &str = "toggle";
const CLEAR: &str = "clear";
const SETTINGS: &str = "settings";
const QUIT: &str = "quit";

pub fn build(
    hotkey: &str,
    status: &HotkeyStatus,
    taskbar_dark: bool,
    lang: Lang,
) -> anyhow::Result<TrayIcon> {
    let t = lang.text();
    let status_text = match status {
        HotkeyStatus::Registered => lang.tray_hotkey(hotkey),
        HotkeyStatus::Failed(e) => lang.tray_hotkey_failed(&lang.hotkey_status_error(hotkey, *e)),
    };
    let menu = Menu::with_items(&[
        &MenuItem::with_id(TOGGLE, t.tray_toggle, true, None),
        &MenuItem::with_id(CLEAR, t.clear, true, None),
        &MenuItem::with_id(SETTINGS, t.settings, true, None),
        &PredefinedMenuItem::separator(),
        &MenuItem::new(status_text, false, None),
        &PredefinedMenuItem::separator(),
        &MenuItem::with_id(QUIT, t.quit, true, None),
    ])?;
    Ok(TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        // Left click toggles the shelf (`ClickFilter`), right click opens the menu.
        .with_menu_on_left_click(false)
        .with_tooltip("Shelve")
        .with_icon(icon(taskbar_dark)?)
        .build()?)
}

pub fn event_for(id: &MenuId) -> Option<PlatformEvent> {
    Some(match id.as_ref() {
        TOGGLE => PlatformEvent::TrayToggle,
        CLEAR => PlatformEvent::TrayClear,
        SETTINGS => PlatformEvent::TraySettings,
        QUIT => PlatformEvent::TrayQuit,
        _ => return None,
    })
}

/// Left clicks on the tray icon that toggle the shelf. The second click of a double click
/// is dropped, so a double click does not show the shelf and hide it again at once.
#[derive(Debug, Default)]
pub struct ClickFilter {
    /// When the last click that counted was let go, ms.
    last: Option<u64>,
}

impl ClickFilter {
    /// `double_ms`: the Windows double-click time.
    pub fn accept(&mut self, now_ms: u64, double_ms: u64) -> bool {
        if self
            .last
            .is_some_and(|t| now_ms.saturating_sub(t) < double_ms)
        {
            return false;
        }
        self.last = Some(now_ms);
        true
    }
}

/// One-colour glyph that contrasts with the taskbar.
pub fn icon(taskbar_dark: bool) -> anyhow::Result<Icon> {
    let style = if taskbar_dark {
        IconStyle::MonoWhite
    } else {
        IconStyle::MonoDark
    };
    // The tray slot's own size (16 px at 100%), so Windows has nothing to rescale.
    let size = unsafe { GetSystemMetrics(SM_CXSMICON) }.max(16) as u32;
    Ok(Icon::from_rgba(icon_rgba(style, size), size, size)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOUBLE: u64 = 500;

    #[test]
    fn the_second_click_of_a_double_click_is_dropped() {
        let mut f = ClickFilter::default();
        assert!(f.accept(1000, DOUBLE));
        assert!(!f.accept(1000 + DOUBLE - 1, DOUBLE));
    }

    #[test]
    fn clicks_a_double_click_time_apart_both_count() {
        let mut f = ClickFilter::default();
        assert!(f.accept(1000, DOUBLE));
        assert!(f.accept(1000 + DOUBLE, DOUBLE));
        assert!(f.accept(5000, DOUBLE));
    }

    #[test]
    fn a_dropped_click_does_not_push_the_next_one_back() {
        let mut f = ClickFilter::default();
        assert!(f.accept(1000, DOUBLE));
        assert!(!f.accept(1300, DOUBLE));
        // Measured from the click that counted, not from the dropped one.
        assert!(f.accept(1000 + DOUBLE, DOUBLE));
    }
}
