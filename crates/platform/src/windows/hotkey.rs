use global_hotkey::hotkey::HotKey;
use global_hotkey::GlobalHotKeyManager;

use crate::{HotkeyError, HotkeyStatus};

/// Never fails: a bad string or a hotkey taken by another app ends up in `Failed`.
/// The manager must live (on this thread) as long as the hotkey should work.
pub fn register(spec: &str) -> (Option<GlobalHotKeyManager>, Option<u32>, HotkeyStatus) {
    let manager = match GlobalHotKeyManager::new() {
        Ok(m) => m,
        Err(e) => return (None, None, HotkeyStatus::Failed(e.to_string())),
    };
    let hotkey: HotKey = match spec.parse() {
        Ok(h) => h,
        Err(e) => {
            return (
                Some(manager),
                None,
                HotkeyStatus::Failed(format!("{spec}: {e}")),
            )
        }
    };
    match manager.register(hotkey) {
        Ok(()) => (Some(manager), Some(hotkey.id()), HotkeyStatus::Registered),
        Err(e) => (
            Some(manager),
            None,
            HotkeyStatus::Failed(format!("{spec}: {e}")),
        ),
    }
}

/// Can `spec` be registered right now? Registers it for a moment on the calling thread and
/// releases it. Windows does not say which program holds a taken hotkey.
pub fn try_hotkey(spec: &str) -> Result<(), HotkeyError> {
    let hotkey: HotKey = spec.parse().map_err(|_| HotkeyError::Unsupported)?;
    let manager = GlobalHotKeyManager::new().map_err(|_| HotkeyError::Taken)?;
    manager.register(hotkey).map_err(|_| HotkeyError::Taken)?;
    let _ = manager.unregister(hotkey);
    Ok(())
}
