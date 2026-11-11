//! Low-level mouse hook plus an invisible "sensor" strip along the screen edge.
//! The strip is shown only while the left button is held and is an OLE drop
//! target, so Windows tells us what is being dragged: files (`CF_HDROP`) fire
//! `FileDragAtEdge`, text, links and windows do not.

use std::cell::{Cell, RefCell};
use std::io;
use std::sync::mpsc::{self, Sender};
use std::thread::{self, JoinHandle};

use shelve_core::Edge;
use windows::core::{implement, w, Ref};
use windows::Win32::Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, POINTL, WPARAM};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromPoint, HMONITOR, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::System::Com::{IDataObject, DVASPECT_CONTENT, FORMATETC, TYMED_HGLOBAL};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Ole::{
    IDropTarget, IDropTarget_Impl, OleInitialize, OleUninitialize, RegisterDragDrop,
    RevokeDragDrop, CF_HDROP, DROPEFFECT, DROPEFFECT_NONE,
};
use windows::Win32::System::SystemServices::MODIFIERKEYS_FLAGS;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW,
    PostMessageW, PostThreadMessageW, RegisterClassW, SetLayeredWindowAttributes, SetWindowPos,
    SetWindowsHookExW, ShowWindow, UnhookWindowsHookEx, HWND_TOPMOST, LWA_ALPHA, MSG,
    MSLLHOOKSTRUCT, SWP_NOACTIVATE, SWP_SHOWWINDOW, SW_HIDE, WH_MOUSE_LL, WM_APP, WM_LBUTTONDOWN,
    WM_LBUTTONUP, WM_MOUSEMOVE, WM_QUIT, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};

use crate::{HookEvent, MouseSample};

/// wParam: HMONITOR to place the strip on.
const WM_PLACE_SENSOR: u32 = WM_APP + 1;
const WM_HIDE_SENSOR: u32 = WM_APP + 2;

thread_local! {
    static TX: RefCell<Option<Sender<HookEvent>>> = const { RefCell::new(None) };
    static LEFT_DOWN: Cell<bool> = const { Cell::new(false) };
    static SENSOR: Cell<isize> = const { Cell::new(0) };
    /// Monitor the strip currently sits on; 0 while hidden.
    static SENSOR_MON: Cell<isize> = const { Cell::new(0) };
    static EDGE: Cell<Edge> = const { Cell::new(Edge::Right) };
    static ZONE: Cell<i32> = const { Cell::new(4) };
}

fn send(e: HookEvent) {
    TX.with(|t| {
        if let Some(tx) = t.borrow().as_ref() {
            let _ = tx.send(e);
        }
    });
}

/// Hook and sensor live on their own thread (no admin rights needed). Drop removes both.
pub struct MouseHook {
    thread_id: u32,
    thread: Option<JoinHandle<()>>,
}

impl Drop for MouseHook {
    fn drop(&mut self) {
        unsafe {
            let _ = PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
        }
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

pub fn start_mouse_hook(edge: Edge, zone_px: u32, tx: Sender<HookEvent>) -> io::Result<MouseHook> {
    let (init_tx, init_rx) = mpsc::sync_channel(1);
    let thread = thread::Builder::new()
        .name("mouse-hook".into())
        .spawn(move || run(edge, zone_px, tx, init_tx))?;
    let thread_id = init_rx
        .recv()
        .map_err(io::Error::other)?
        .map_err(io::Error::other)?;
    Ok(MouseHook {
        thread_id,
        thread: Some(thread),
    })
}

fn run(
    edge: Edge,
    zone_px: u32,
    tx: Sender<HookEvent>,
    init: mpsc::SyncSender<windows::core::Result<u32>>,
) {
    TX.with(|t| *t.borrow_mut() = Some(tx));
    EDGE.set(edge);
    ZONE.set(zone_px.max(1) as i32);
    let setup = unsafe {
        (|| {
            OleInitialize(None)?;
            let module = GetModuleHandleW(None)?;
            let instance = HINSTANCE(module.0);
            let class = w!("ShelveEdgeSensor");
            RegisterClassW(&WNDCLASSW {
                lpfnWndProc: Some(sensor_proc),
                hInstance: instance,
                lpszClassName: class,
                ..Default::default()
            });
            let sensor = CreateWindowExW(
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
            // Alpha 1, not 0: a fully transparent window would not be hit-tested by OLE.
            SetLayeredWindowAttributes(sensor, COLORREF(0), 1, LWA_ALPHA)?;
            let target: IDropTarget = Sensor.into();
            RegisterDragDrop(sensor, &target)?;
            SENSOR.set(sensor.0 as isize);
            // The hook proc runs on this thread, inside GetMessageW below.
            let hook = SetWindowsHookExW(WH_MOUSE_LL, Some(hook_proc), Some(instance), 0)?;
            Ok((sensor, hook))
        })()
    };
    let (sensor, hook) = match setup {
        Ok(v) => v,
        Err(e) => return drop(init.send(Err(e))),
    };
    let _ = init.send(Ok(unsafe { GetCurrentThreadId() }));
    let mut msg = MSG::default();
    while unsafe { GetMessageW(&mut msg, None, 0, 0) }.as_bool() {
        unsafe {
            DispatchMessageW(&msg);
        }
    }
    unsafe {
        let _ = UnhookWindowsHookEx(hook);
        let _ = RevokeDragDrop(sensor);
        let _ = DestroyWindow(sensor);
        OleUninitialize();
    }
}

// Must return fast: Windows silently drops slow low-level hooks. Window work is posted.
unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let info = unsafe { &*(lparam.0 as *const MSLLHOOKSTRUCT) };
        let sensor = HWND(SENSOR.get() as *mut _);
        match wparam.0 as u32 {
            WM_LBUTTONDOWN => LEFT_DOWN.set(true),
            WM_LBUTTONUP => {
                LEFT_DOWN.set(false);
                if SENSOR_MON.replace(0) != 0 {
                    let _ =
                        unsafe { PostMessageW(Some(sensor), WM_HIDE_SENSOR, WPARAM(0), LPARAM(0)) };
                }
            }
            // Only once the button is held and moving: the press itself has already been
            // hit-tested, so the strip never steals a click (e.g. on an edge scrollbar).
            WM_MOUSEMOVE if LEFT_DOWN.get() => {
                let mon = unsafe { MonitorFromPoint(info.pt, MONITOR_DEFAULTTONEAREST) };
                if SENSOR_MON.replace(mon.0 as isize) != mon.0 as isize {
                    let _ = unsafe {
                        PostMessageW(
                            Some(sensor),
                            WM_PLACE_SENSOR,
                            WPARAM(mon.0 as usize),
                            LPARAM(0),
                        )
                    };
                }
            }
            _ => {}
        }
        send(HookEvent::Mouse(MouseSample {
            pos: (info.pt.x, info.pt.y),
            left_down: LEFT_DOWN.get(),
        }));
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

unsafe extern "system" fn sensor_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_PLACE_SENSOR => {
            let mut info = MONITORINFO {
                cbSize: size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            let mon = HMONITOR(wparam.0 as *mut _);
            if unsafe { GetMonitorInfoW(mon, &mut info) }.as_bool() {
                // Whole monitor, not the work area: the edge is where the cursor stops.
                let r = info.rcMonitor;
                let zone = ZONE.get();
                let x = match EDGE.get() {
                    Edge::Left => r.left,
                    Edge::Right => r.right - zone,
                };
                let _ = unsafe {
                    SetWindowPos(
                        hwnd,
                        Some(HWND_TOPMOST),
                        x,
                        r.top,
                        zone,
                        r.bottom - r.top,
                        SWP_NOACTIVATE | SWP_SHOWWINDOW,
                    )
                };
            }
            LRESULT(0)
        }
        WM_HIDE_SENSOR => {
            let _ = unsafe { ShowWindow(hwnd, SW_HIDE) };
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

#[implement(IDropTarget)]
struct Sensor;

fn has_files(data: Ref<IDataObject>) -> bool {
    let format = FORMATETC {
        cfFormat: CF_HDROP.0,
        ptd: std::ptr::null_mut(),
        dwAspect: DVASPECT_CONTENT.0,
        lindex: -1,
        tymed: TYMED_HGLOBAL.0 as u32,
    };
    data.as_ref()
        .is_some_and(|d| unsafe { d.QueryGetData(&format) }.is_ok())
}

impl IDropTarget_Impl for Sensor_Impl {
    fn DragEnter(
        &self,
        data: Ref<IDataObject>,
        _keys: MODIFIERKEYS_FLAGS,
        _pt: &POINTL,
        effect: *mut DROPEFFECT,
    ) -> windows::core::Result<()> {
        if has_files(data) {
            send(HookEvent::FileDragAtEdge);
        }
        // The shelf opens right above the strip; the strip itself never takes the drop.
        unsafe { *effect = DROPEFFECT_NONE };
        Ok(())
    }

    fn DragOver(
        &self,
        _keys: MODIFIERKEYS_FLAGS,
        _pt: &POINTL,
        effect: *mut DROPEFFECT,
    ) -> windows::core::Result<()> {
        unsafe { *effect = DROPEFFECT_NONE };
        Ok(())
    }

    fn DragLeave(&self) -> windows::core::Result<()> {
        Ok(())
    }

    fn Drop(
        &self,
        _data: Ref<IDataObject>,
        _keys: MODIFIERKEYS_FLAGS,
        _pt: &POINTL,
        effect: *mut DROPEFFECT,
    ) -> windows::core::Result<()> {
        unsafe { *effect = DROPEFFECT_NONE };
        Ok(())
    }
}
