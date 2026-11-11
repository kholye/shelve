use std::io;

use windows::core::{w, HSTRING};
use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, WIN32_ERROR};
use windows::Win32::System::Registry::{
    RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ,
};

const VALUE_NAME: &str = "Shelve";
const RUN_KEY: windows::core::PCWSTR = w!(r"Software\Microsoft\Windows\CurrentVersion\Run");

pub fn set_autostart(enabled: bool) -> io::Result<()> {
    set_named(VALUE_NAME, enabled)
}

pub fn is_autostart() -> bool {
    is_named(VALUE_NAME)
}

fn set_named(name: &str, enabled: bool) -> io::Result<()> {
    let name = HSTRING::from(name);
    let err = if enabled {
        // Quoted so a path with spaces is not split by the shell at logon.
        let cmd = format!("\"{}\"", std::env::current_exe()?.display());
        let data: Vec<u16> = cmd.encode_utf16().chain([0]).collect();
        unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                RUN_KEY,
                &name,
                REG_SZ.0,
                Some(data.as_ptr().cast()),
                (data.len() * 2) as u32,
            )
        }
    } else {
        match unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN_KEY, &name) } {
            ERROR_FILE_NOT_FOUND => WIN32_ERROR(0),
            e => e,
        }
    };
    win32(err)
}

fn is_named(name: &str) -> bool {
    let name = HSTRING::from(name);
    let err = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            RUN_KEY,
            &name,
            RRF_RT_REG_SZ,
            None,
            None,
            None,
        )
    };
    err.is_ok()
}

fn win32(err: WIN32_ERROR) -> io::Result<()> {
    if err.is_ok() {
        Ok(())
    } else {
        Err(io::Error::from_raw_os_error(err.0 as i32))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_NAME: &str = "ShelveTest";

    /// Removes the test value even if an assertion fails midway.
    struct Cleanup;
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = set_named(TEST_NAME, false);
        }
    }

    #[test]
    fn autostart_roundtrip() {
        let _cleanup = Cleanup;
        set_named(TEST_NAME, true).unwrap();
        assert!(is_named(TEST_NAME));
        set_named(TEST_NAME, false).unwrap();
        assert!(!is_named(TEST_NAME));
    }
}
