//! A plain HTTPS GET through WinHTTP, which ships with Windows: no TLS library of our
//! own, and the system proxy settings apply.

use std::ffi::c_void;

use anyhow::{bail, Context, Result};
use windows::core::{w, HSTRING, PCWSTR};
use windows::Win32::Networking::WinHttp::{
    WinHttpCloseHandle, WinHttpConnect, WinHttpOpen, WinHttpOpenRequest, WinHttpQueryHeaders,
    WinHttpReadData, WinHttpReceiveResponse, WinHttpSendRequest, WinHttpSetTimeouts,
    INTERNET_DEFAULT_HTTPS_PORT, WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, WINHTTP_FLAG_SECURE,
    WINHTTP_QUERY_FLAG_NUMBER, WINHTTP_QUERY_STATUS_CODE,
};

/// Answers bigger than this are not what was asked for.
const MAX_BODY: usize = 1 << 20;
const TIMEOUT_MS: i32 = 15_000;

/// Closes a WinHTTP handle when dropped.
struct Handle(*mut c_void);

impl Handle {
    fn new(raw: *mut c_void, what: &str) -> Result<Self> {
        if raw.is_null() {
            bail!("{what}: {}", windows::core::Error::from_thread());
        }
        Ok(Self(raw))
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: a handle WinHTTP gave out, closed once.
        unsafe {
            let _ = WinHttpCloseHandle(self.0);
        }
    }
}

/// The body of `https://{host}{path}` if the server answers 200.
pub fn https_get(host: &str, path: &str, headers: &str) -> Result<String> {
    // SAFETY: every handle is checked before use and outlives the calls made with it.
    unsafe {
        let session = Handle::new(
            WinHttpOpen(
                // Also the User-Agent, which GitHub's API requires.
                w!("Shelve"),
                WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
                PCWSTR::null(),
                PCWSTR::null(),
                0,
            ),
            "open",
        )?;
        WinHttpSetTimeouts(session.0, TIMEOUT_MS, TIMEOUT_MS, TIMEOUT_MS, TIMEOUT_MS)?;
        let connect = Handle::new(
            WinHttpConnect(
                session.0,
                &HSTRING::from(host),
                INTERNET_DEFAULT_HTTPS_PORT,
                0,
            ),
            "connect",
        )?;
        let request = Handle::new(
            WinHttpOpenRequest(
                connect.0,
                w!("GET"),
                &HSTRING::from(path),
                PCWSTR::null(),
                PCWSTR::null(),
                std::ptr::null(),
                WINHTTP_FLAG_SECURE,
            ),
            "request",
        )?;
        let headers: Vec<u16> = headers.encode_utf16().collect();
        // An empty slice still has a pointer, which WinHTTP would read as text.
        let headers = (!headers.is_empty()).then_some(headers.as_slice());
        WinHttpSendRequest(request.0, headers, None, 0, 0, 0).context("send")?;
        WinHttpReceiveResponse(request.0, std::ptr::null_mut()).context("receive")?;

        let mut status = 0u32;
        let mut len = size_of::<u32>() as u32;
        WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            PCWSTR::null(),
            Some(&mut status as *mut u32 as *mut c_void),
            &mut len,
            std::ptr::null_mut(),
        )
        .context("status")?;
        if status != 200 {
            bail!("HTTP {status}");
        }

        let mut body = Vec::new();
        let mut chunk = [0u8; 8192];
        loop {
            let mut read = 0u32;
            WinHttpReadData(
                request.0,
                chunk.as_mut_ptr() as *mut c_void,
                chunk.len() as u32,
                &mut read,
            )
            .context("read")?;
            if read == 0 {
                break;
            }
            body.extend_from_slice(&chunk[..read as usize]);
            if body.len() > MAX_BODY {
                bail!("answer too long");
            }
        }
        String::from_utf8(body).context("not UTF-8")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Goes to the internet: `cargo test -p shelve-platform -- --ignored`.
    #[test]
    #[ignore]
    fn reads_the_latest_release_from_github() {
        let body = https_get(
            "api.github.com",
            "/repos/kholye/shelve/releases/latest",
            "Accept: application/vnd.github+json\r\n",
        )
        .unwrap();
        assert!(body.contains("\"tag_name\""), "{body}");
    }

    #[test]
    #[ignore]
    fn a_missing_page_is_an_error() {
        let e = https_get("api.github.com", "/repos/kholye/no-such-repo-here", "").unwrap_err();
        assert!(e.to_string().contains("404"), "{e}");
    }
}
