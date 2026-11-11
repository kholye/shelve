use std::ffi::c_void;

use windows::core::{w, BOOL, PCWSTR};
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::Registry::{
    RegCloseKey, RegGetValueW, RegNotifyChangeKeyValue, RegOpenKeyExW, HKEY, HKEY_CURRENT_USER,
    KEY_NOTIFY, REG_NOTIFY_CHANGE_LAST_SET, RRF_RT_REG_DWORD,
};
use windows::Win32::System::Threading::CreateEventW;
use windows::Win32::UI::WindowsAndMessaging::{
    SystemParametersInfoW, SPI_GETCLIENTAREAANIMATION, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
};

const PERSONALIZE_KEY: PCWSTR = w!(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize");
const DWM_KEY: PCWSTR = w!(r"Software\Microsoft\Windows\DWM");

/// The registry keys whose changes mean "the look may have changed".
pub const WATCHED_KEYS: [PCWSTR; 2] = [PERSONALIZE_KEY, DWM_KEY];

/// Windows 11 default accent, used when the registry has none.
const DEFAULT_ACCENT: [u8; 3] = [0x0F, 0x6C, 0xBD];

/// What Shelve copies from Windows: app theme, taskbar theme, accent colour (RGB).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SystemLook {
    pub apps_dark: bool,
    pub taskbar_dark: bool,
    pub accent: [u8; 3],
}

pub fn system_look() -> SystemLook {
    look_from(
        read_dword(PERSONALIZE_KEY, w!("AppsUseLightTheme")),
        read_dword(PERSONALIZE_KEY, w!("SystemUsesLightTheme")),
        read_dword(DWM_KEY, w!("AccentColor")),
    )
}

/// False when "Animation effects" is off in Windows. On error: animate.
pub fn animations_enabled() -> bool {
    let mut on = BOOL(1);
    let res = unsafe {
        SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            Some((&raw mut on).cast::<c_void>()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    };
    res.is_err() || on.as_bool()
}

/// Old Windows 10 has neither theme value: apps light, taskbar dark.
fn look_from(
    apps_light: Option<u32>,
    system_light: Option<u32>,
    accent_abgr: Option<u32>,
) -> SystemLook {
    SystemLook {
        apps_dark: apps_light == Some(0),
        taskbar_dark: system_light != Some(1),
        accent: accent_abgr.map_or(DEFAULT_ACCENT, accent_from_abgr),
    }
}

fn accent_from_abgr(abgr: u32) -> [u8; 3] {
    let [r, g, b, _a] = abgr.to_le_bytes();
    [r, g, b]
}

fn read_dword(key: PCWSTR, value: PCWSTR) -> Option<u32> {
    let mut data = 0u32;
    let mut size = size_of::<u32>() as u32;
    let err = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key,
            value,
            RRF_RT_REG_DWORD,
            None,
            Some((&raw mut data).cast::<c_void>()),
            Some(&mut size),
        )
    };
    err.is_ok().then_some(data)
}

/// Signals `event` when a value under the key changes. No polling: the thread
/// that owns it sleeps on the event. Must stay on the thread that created it,
/// Windows drops the subscription when that thread exits.
pub struct KeyWatch {
    key: HKEY,
    event: HANDLE,
}

impl KeyWatch {
    pub fn new(path: PCWSTR) -> Option<Self> {
        let mut key = HKEY::default();
        unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, path, None, KEY_NOTIFY, &mut key) }
            .ok()
            .ok()?;
        let Ok(event) = (unsafe { CreateEventW(None, false, false, None) }) else {
            unsafe {
                let _ = RegCloseKey(key);
            }
            return None;
        };
        let watch = Self { key, event };
        watch.arm().then_some(watch)
    }

    pub fn event(&self) -> HANDLE {
        self.event
    }

    /// One subscription reports one change; call again after every signal.
    pub fn arm(&self) -> bool {
        unsafe {
            RegNotifyChangeKeyValue(
                self.key,
                false,
                REG_NOTIFY_CHANGE_LAST_SET,
                Some(self.event),
                true,
            )
        }
        .is_ok()
    }
}

impl Drop for KeyWatch {
    fn drop(&mut self) {
        unsafe {
            let _ = RegCloseKey(self.key);
            let _ = CloseHandle(self.event);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accent_is_stored_as_abgr() {
        assert_eq!(accent_from_abgr(0xFFD4_7800), [0x00, 0x78, 0xD4]);
    }

    #[test]
    fn registry_values_map_to_look() {
        assert_eq!(
            look_from(Some(0), Some(1), Some(0xFFD4_7800)),
            SystemLook {
                apps_dark: true,
                taskbar_dark: false,
                accent: [0x00, 0x78, 0xD4]
            }
        );
    }

    #[test]
    fn missing_registry_values_fall_back_to_old_windows_10_look() {
        assert_eq!(
            look_from(None, None, None),
            SystemLook {
                apps_dark: false,
                taskbar_dark: true,
                accent: [0x0F, 0x6C, 0xBD]
            }
        );
    }
}
