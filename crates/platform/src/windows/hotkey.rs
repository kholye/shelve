use global_hotkey::hotkey::HotKey;
use global_hotkey::GlobalHotKeyManager;

use crate::{HotkeyError, HotkeyStatus};

/// Never fails: a bad string or a hotkey taken by another app ends up in `Failed`,
/// sorted the same way as in [`try_hotkey`].
/// The manager must live (on this thread) as long as the hotkey should work.
pub fn register(spec: &str) -> (Option<GlobalHotKeyManager>, Option<u32>, HotkeyStatus) {
    let Ok(manager) = GlobalHotKeyManager::new() else {
        return (None, None, HotkeyStatus::Failed(HotkeyError::Taken));
    };
    let Ok(hotkey) = spec.parse::<HotKey>() else {
        let status = HotkeyStatus::Failed(HotkeyError::Unsupported);
        return (Some(manager), None, status);
    };
    match manager.register(hotkey) {
        Ok(()) => (Some(manager), Some(hotkey.id()), HotkeyStatus::Registered),
        Err(_) => (
            Some(manager),
            None,
            HotkeyStatus::Failed(HotkeyError::Taken),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unknown_key_is_unsupported() {
        let (_, id, status) = register("Ctrl+Alt+NoSuchKey");
        assert_eq!(id, None);
        assert_eq!(status, HotkeyStatus::Failed(HotkeyError::Unsupported));
    }

    #[test]
    fn a_hotkey_held_elsewhere_is_taken() {
        let taken: HotKey = "Ctrl+Alt+Shift+F21".parse().unwrap();
        let holder = GlobalHotKeyManager::new().unwrap();
        holder.register(taken).unwrap();
        let (_, id, status) = register("Ctrl+Alt+Shift+F21");
        assert_eq!(id, None);
        assert_eq!(status, HotkeyStatus::Failed(HotkeyError::Taken));
    }
}
