//! The tab: a small piece of the shelf with an arrow, at the screen edge while a shelf
//! with files is hidden. Resting the mouse on it (about 150 ms, no button held) or
//! clicking it sends `TabHovered`. A plain window on the platform thread, so it needs
//! no mouse hook: a mouse that stands still on it still gets its hover message.
//! Layered, so its rounded corners are see-through and smooth.

use std::cell::Cell;
use std::cell::RefCell;
use std::sync::mpsc::Sender;

use windows::core::w;
use windows::Win32::Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, POINT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, ReleaseDC, SelectObject,
    AC_SRC_ALPHA, AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION, DIB_RGB_COLORS,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::SystemServices::{MK_LBUTTON, MK_MBUTTON, MK_RBUTTON};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    TrackMouseEvent, TME_HOVER, TME_LEAVE, TRACKMOUSEEVENT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetCursorPos, LoadCursorW, RegisterClassW,
    SetWindowPos, ShowWindow, UpdateLayeredWindow, HWND_TOPMOST, IDC_ARROW, MA_NOACTIVATE,
    SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW, SW_HIDE, ULW_ALPHA, WM_LBUTTONUP,
    WM_MOUSEACTIVATE, WM_MOUSEMOVE, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    WS_EX_TOPMOST, WS_POPUP,
};

use crate::{tab_pixels, PlatformEvent, TabSpec};

/// How long the mouse rests on the tab before the shelf comes out; a mouse passing by
/// does not stay that long.
const HOVER_MS: u32 = 150;

/// Sent after `TrackMouseEvent`. The windows crate keeps these under Win32_UI_Controls,
/// a whole feature for two numbers.
const WM_MOUSEHOVER: u32 = 0x02A1;
const WM_MOUSELEAVE: u32 = 0x02A3;

thread_local! {
    static TX: RefCell<Option<Sender<PlatformEvent>>> = const { RefCell::new(None) };
    /// What the tab shows now; None while hidden.
    static SPEC: Cell<Option<TabSpec>> = const { Cell::new(None) };
    static HOVERED: Cell<bool> = const { Cell::new(false) };
    static MOVES: Cell<MoveFilter> = const { Cell::new(MoveFilter { still_at: None }) };
}

/// Windows reports a mouse move to a window that appears under a cursor standing still.
/// That is not the user pointing at the tab, so it is dropped until the cursor moves.
#[derive(Debug, Clone, Copy, Default)]
struct MoveFilter {
    /// Where the cursor was when the tab appeared; None once it has moved.
    still_at: Option<(i32, i32)>,
}

impl MoveFilter {
    fn shown(&mut self, cursor: (i32, i32)) {
        self.still_at = Some(cursor);
    }

    /// True for a move the user made.
    fn moved(&mut self, at: (i32, i32)) -> bool {
        if self.still_at == Some(at) {
            return false;
        }
        self.still_at = None;
        true
    }
}

fn cursor() -> (i32, i32) {
    let mut p = POINT::default();
    let _ = unsafe { GetCursorPos(&mut p) };
    (p.x, p.y)
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
                WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
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

    /// Shows the tab as `spec` says, or hides it.
    pub fn place(&self, spec: Option<TabSpec>) {
        SPEC.set(spec);
        HOVERED.set(false);
        let mut moves = MoveFilter::default();
        moves.shown(cursor());
        MOVES.set(moves);
        match spec {
            Some(spec) => {
                if let Err(e) = draw(self.hwnd, spec, false) {
                    println!("tab draw failed: {e}");
                    return;
                }
                unsafe {
                    let _ = SetWindowPos(
                        self.hwnd,
                        Some(HWND_TOPMOST),
                        0,
                        0,
                        0,
                        0,
                        SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
                    );
                }
            }
            None => unsafe {
                let _ = ShowWindow(self.hwnd, SW_HIDE);
            },
        }
    }
}

impl Drop for Tab {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyWindow(self.hwnd);
        }
        SPEC.set(None);
        TX.with(|t| *t.borrow_mut() = None);
    }
}

/// Paints the tab and puts the window at `spec.rect` in one go.
fn draw(hwnd: HWND, spec: TabSpec, hovered: bool) -> windows::core::Result<()> {
    let r = spec.rect;
    let (w, h) = (r.right - r.left, r.bottom - r.top);
    if w <= 0 || h <= 0 {
        return Ok(());
    }
    let pixels = tab_pixels(
        w as u32,
        h as u32,
        spec.scale,
        spec.edge,
        spec.colors,
        hovered,
    );
    unsafe {
        let screen = GetDC(None);
        let mem = CreateCompatibleDC(Some(screen));
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h, // top row first
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits = std::ptr::null_mut();
        let result = CreateDIBSection(Some(mem), &info, DIB_RGB_COLORS, &mut bits, None, 0)
            .and_then(|bitmap| {
                std::ptr::copy_nonoverlapping(pixels.as_ptr(), bits as *mut u8, pixels.len());
                let old = SelectObject(mem, bitmap.into());
                let blend = BLENDFUNCTION {
                    BlendOp: AC_SRC_OVER as u8,
                    BlendFlags: 0,
                    SourceConstantAlpha: 255,
                    AlphaFormat: AC_SRC_ALPHA as u8,
                };
                let done = UpdateLayeredWindow(
                    hwnd,
                    Some(screen),
                    Some(&POINT {
                        x: r.left,
                        y: r.top,
                    }),
                    Some(&SIZE { cx: w, cy: h }),
                    Some(mem),
                    Some(&POINT::default()),
                    COLORREF(0),
                    Some(&blend),
                    ULW_ALPHA,
                );
                SelectObject(mem, old);
                let _ = DeleteObject(bitmap.into());
                done
            });
        let _ = DeleteDC(mem);
        ReleaseDC(None, screen);
        result
    }
}

fn set_hovered(hwnd: HWND, hovered: bool) {
    if HOVERED.replace(hovered) == hovered {
        return;
    }
    if let Some(spec) = SPEC.get() {
        let _ = draw(hwnd, spec, hovered);
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
            let mut moves = MOVES.get();
            let real = moves.moved(cursor());
            MOVES.set(moves);
            if !real {
                return LRESULT(0);
            }
            set_hovered(hwnd, true);
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
        WM_MOUSELEAVE => {
            set_hovered(hwnd, false);
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
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tab_appearing_under_a_still_cursor_is_not_a_move() {
        // Bug: the hotkey hid the shelf with the cursor where the tab appears. Windows
        // reported a move to the new window, the hover fired and the shelf came back.
        let mut f = MoveFilter::default();
        f.shown((1914, 520));
        assert!(!f.moved((1914, 520)));
        assert!(!f.moved((1914, 520)));
        assert!(f.moved((1913, 520)));
    }

    #[test]
    fn after_a_real_move_the_same_spot_counts_again() {
        let mut f = MoveFilter::default();
        f.shown((1914, 520));
        assert!(f.moved((1915, 521)));
        assert!(f.moved((1914, 520)));
    }

    #[test]
    fn a_cursor_away_from_the_tab_counts_on_its_first_move_onto_it() {
        let mut f = MoveFilter::default();
        f.shown((900, 500));
        assert!(f.moved((1914, 520)));
    }
}
