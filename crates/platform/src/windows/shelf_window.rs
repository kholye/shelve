//! Shows and hides the shelf window without ever activating it. winit's own
//! `set_visible(true)` uses `SW_SHOW` after the first time, which steals focus.

use shelve_core::ScreenRect;
use windows::core::BOOL;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{
    DwmSetWindowAttribute, DWMNCRENDERINGPOLICY, DWMNCRP_DISABLED, DWMWA_NCRENDERING_POLICY,
    DWMWA_TRANSITIONS_FORCEDISABLED,
};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::System::Threading::{GetCurrentProcess, SetProcessWorkingSetSize};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{
    GetCursorPos, GetWindowLongPtrW, SetWindowLongPtrW, SetWindowPos, ShowWindow, GWL_EXSTYLE,
    HWND_TOPMOST, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER,
    SWP_SHOWWINDOW, SW_HIDE, WM_NCACTIVATE, WM_NCCALCSIZE, WM_NCPAINT, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW,
};

fn hwnd(raw: isize) -> HWND {
    HWND(raw as *mut _)
}

/// Keeps Windows from treating any part of the shelf as a title bar.
unsafe extern "system" fn no_frame_proc(
    h: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    // The whole window is client area. egui-winit turns on winit's "undecorated shadow",
    // which leaves a 1 px non-client row on top that Windows hit-tested as a title bar:
    // the shelf could be dragged around by it.
    if msg == WM_NCCALCSIZE && wparam.0 != 0 {
        return LRESULT(0);
    }
    // DefWindowProc paints a classic title bar over the shelf on activation (a click), even
    // without caption styles, and the idle shelf never repaints over it. TRUE lets the
    // activation go on; winit still tracks focus through WM_SETFOCUS / WM_KILLFOCUS.
    if msg == WM_NCACTIVATE {
        return LRESULT(1);
    }
    // Same for the whole classic frame on the first show: it showed through the transparent
    // part of the window while the shelf slid in. There is no frame to paint.
    if msg == WM_NCPAINT {
        return LRESULT(0);
    }
    unsafe { DefSubclassProc(h, msg, wparam, lparam) }
}

/// Clicks on the window no longer activate it, and it stays out of Alt+Tab. Turns off DWM's
/// frame rendering: winit keeps caption styles on undecorated windows, so DWM drew a shadow
/// outline around the transparent shelf and its hidden caption buttons. Also turns off the
/// system fade on show/hide: the shelf slides itself. Call before every show: winit
/// rewrites the extended style.
pub fn prepare_shelf_window(raw_hwnd: isize) {
    unsafe {
        // Same id again only replaces the data, so repeated calls install it once.
        let _ = SetWindowSubclass(hwnd(raw_hwnd), Some(no_frame_proc), 1, 0);
        let _ = SetWindowPos(
            hwnd(raw_hwnd),
            None,
            0,
            0,
            0,
            0,
            SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
        );
        let ex = GetWindowLongPtrW(hwnd(raw_hwnd), GWL_EXSTYLE);
        SetWindowLongPtrW(
            hwnd(raw_hwnd),
            GWL_EXSTYLE,
            ex | (WS_EX_NOACTIVATE.0 | WS_EX_TOOLWINDOW.0) as isize,
        );
        let off = BOOL(1);
        let _ = DwmSetWindowAttribute(
            hwnd(raw_hwnd),
            DWMWA_TRANSITIONS_FORCEDISABLED,
            &off as *const BOOL as *const _,
            size_of::<BOOL>() as u32,
        );
        let policy = DWMNCRP_DISABLED;
        let _ = DwmSetWindowAttribute(
            hwnd(raw_hwnd),
            DWMWA_NCRENDERING_POLICY,
            &policy as *const DWMNCRENDERINGPOLICY as *const _,
            size_of::<DWMNCRENDERINGPOLICY>() as u32,
        );
    }
}

/// `rect` is in physical pixels.
pub fn show_shelf_window(raw_hwnd: isize, rect: ScreenRect) {
    unsafe {
        let _ = SetWindowPos(
            hwnd(raw_hwnd),
            Some(HWND_TOPMOST),
            rect.left,
            rect.top,
            rect.right - rect.left,
            rect.bottom - rect.top,
            SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
    }
}

pub fn hide_shelf_window(raw_hwnd: isize) {
    unsafe {
        let _ = ShowWindow(hwnd(raw_hwnd), SW_HIDE);
    }
}

/// Hands the pages the process is not touching back to Windows (same as EmptyWorkingSet).
/// Nothing is lost: a page that is needed again is simply brought back. Most of the idle
/// working set belongs to the OpenGL driver and is never touched while the shelf is hidden.
pub fn trim_memory() {
    unsafe {
        let _ = SetProcessWorkingSetSize(GetCurrentProcess(), usize::MAX, usize::MAX);
    }
}

/// Work area (without the taskbar) and DPI scale of the monitor under the cursor.
pub fn monitor_at_cursor() -> (ScreenRect, f32) {
    unsafe {
        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
        let mon = MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        let _ = GetMonitorInfoW(mon, &mut info);
        let (mut dpi, mut dpi_y) = (96, 96);
        let _ = GetDpiForMonitor(mon, MDT_EFFECTIVE_DPI, &mut dpi, &mut dpi_y);
        let RECT {
            left,
            top,
            right,
            bottom,
        } = info.rcWork;
        (
            ScreenRect {
                left,
                top,
                right,
                bottom,
            },
            dpi as f32 / 96.0,
        )
    }
}
