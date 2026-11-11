//! The tab: a small accent-coloured strip at the screen edge while a shelf with files is
//! hidden. Resting the mouse on it (about 150 ms, no button held) or clicking it sends
//! `TabHovered`. A plain window on the platform thread, so it needs no mouse hook: a
//! mouse that stands still on it still gets its hover message.

use std::cell::RefCell;
use std::sync::mpsc::Sender;

use shelve_core::ScreenRect;
use windows::core::w;
use windows::Win32::Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateSolidBrush, DeleteObject, EndPaint, FillRect, InvalidateRect, PAINTSTRUCT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::SystemServices::{MK_LBUTTON, MK_MBUTTON, MK_RBUTTON};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    TrackMouseEvent, TME_HOVER, TME_LEAVE, TRACKMOUSEEVENT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, LoadCursorW, RegisterClassW, SetWindowPos,
    ShowWindow, HWND_TOPMOST, IDC_ARROW, MA_NOACTIVATE, SWP_NOACTIVATE, SWP_SHOWWINDOW, SW_HIDE,
    WM_LBUTTONUP, WM_MOUSEACTIVATE, WM_MOUSEMOVE, WM_PAINT, WNDCLASSW, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};

use crate::PlatformEvent;

/// How long the mouse rests on the tab before the shelf comes out; a mouse passing by
/// does not stay that long.
const HOVER_MS: u32 = 150;

/// Sent after `TrackMouseEvent` with `TME_HOVER`. The windows crate keeps it under
/// Win32_UI_Controls, a whole feature for one number.
const WM_MOUSEHOVER: u32 = 0x02A1;

thread_local! {
    static TX: RefCell<Option<Sender<PlatformEvent>>> = const { RefCell::new(None) };
}

fn send(e: PlatformEvent) {
    TX.with(|t| {
        if let Some(tx) = t.borrow().as_ref() {
            let _ = tx.send(e);
        }
    });
}

/// Must be created and used on the thread that pumps its messages. Drop destroys it.
pub struct Tab {
    hwnd: HWND,
}

impl Tab {
    pub fn create(tx: Sender<PlatformEvent>) -> windows::core::Result<Tab> {
        TX.with(|t| *t.borrow_mut() = Some(tx));
        unsafe {
            let instance = HINSTANCE(GetModuleHandleW(None)?.0);
            let class = w!("ShelveTab");
            RegisterClassW(&WNDCLASSW {
                lpfnWndProc: Some(tab_proc),
                hInstance: instance,
                hCursor: LoadCursorW(None, IDC_ARROW)?,
                lpszClassName: class,
                ..Default::default()
            });
            // Out of Alt+Tab and the taskbar, never takes the focus.
            let hwnd = CreateWindowExW(
                WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                class,
                w!(""),
                WS_POPUP,
                0,
                0,
                0,
                0,
                None,
                None,
                Some(instance),
                None,
            )?;
            Ok(Tab { hwnd })
        }
    }

    /// Shows the tab at `rect`, or hides it.
    pub fn place(&self, rect: Option<ScreenRect>) {
        unsafe {
            match rect {
                Some(r) => {
                    let _ = SetWindowPos(
                        self.hwnd,
                        Some(HWND_TOPMOST),
                        r.left,
                        r.top,
                        r.right - r.left,
                        r.bottom - r.top,
                        SWP_NOACTIVATE | SWP_SHOWWINDOW,
                    );
                }
                None => {
                    let _ = ShowWindow(self.hwnd, SW_HIDE);
                }
            }
        }
    }

    /// Paints it again in the current accent colour.
    pub fn repaint(&self) {
        unsafe {
            let _ = InvalidateRect(Some(self.hwnd), None, true);
        }
    }
}

impl Drop for Tab {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyWindow(self.hwnd);
        }
        TX.with(|t| *t.borrow_mut() = None);
    }
}

unsafe extern "system" fn tab_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_MOUSEMOVE => {
            // Hover is reported once per tracking; every move arms it again.
            let mut track = TRACKMOUSEEVENT {
                cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                dwFlags: TME_HOVER | TME_LEAVE,
                hwndTrack: hwnd,
                dwHoverTime: HOVER_MS,
            };
            let _ = unsafe { TrackMouseEvent(&mut track) };
            LRESULT(0)
        }
        WM_MOUSEHOVER => {
            // A held button: something else is being dragged past, a scroll bar thumb say.
            let held = (MK_LBUTTON | MK_RBUTTON | MK_MBUTTON).0 as usize;
            if wparam.0 & held == 0 {
                send(PlatformEvent::TabHovered);
            }
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            send(PlatformEvent::TabHovered);
            LRESULT(0)
        }
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_PAINT => {
            unsafe {
                let mut ps = PAINTSTRUCT::default();
                let dc = BeginPaint(hwnd, &mut ps);
                let [r, g, b] = crate::windows::system_look().accent;
                let brush =
                    CreateSolidBrush(COLORREF(r as u32 | (g as u32) << 8 | (b as u32) << 16));
                FillRect(dc, &ps.rcPaint, brush);
                let _ = DeleteObject(brush.into());
                let _ = EndPaint(hwnd, &ps);
            }
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}
