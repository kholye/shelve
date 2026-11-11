use std::io;
use std::mem::ManuallyDrop;
use std::os::windows::ffi::OsStrExt;
use std::path::PathBuf;

use windows::core::{implement, w, PCWSTR};
use windows::Win32::Foundation::{
    GlobalFree, DRAGDROP_S_CANCEL, DRAGDROP_S_DROP, DRAGDROP_S_USEDEFAULTCURSORS, LPARAM, S_OK,
    WPARAM,
};
use windows::Win32::System::Com::{
    IDataObject, DVASPECT_CONTENT, FORMATETC, STGMEDIUM, STGMEDIUM_0, TYMED_HGLOBAL,
};
use windows::Win32::System::DataExchange::RegisterClipboardFormatW;
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::System::Ole::{
    DoDragDrop, IDropSource, IDropSource_Impl, DROPEFFECT, DROPEFFECT_COPY, DROPEFFECT_MOVE,
};
use windows::Win32::System::SystemServices::{MK_LBUTTON, MK_RBUTTON, MODIFIERKEYS_FLAGS};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Shell::Common::ITEMIDLIST;
use windows::Win32::UI::Shell::{
    BHID_DataObject, ILCreateFromPathW, ILFree, IShellItemArray, SHCreateShellItemArrayFromIDLists,
};
use windows::Win32::UI::WindowsAndMessaging::{PostThreadMessageW, WM_NULL};

use crate::{DragButton, DragOutcome};

/// OLE drag of the files themselves, held by `button`. Copy and move are both offered;
/// the left button asks Explorer to move, the right one puts Copy first in its menu, Ctrl
/// makes a copy. The shelf never deletes anything: Explorer moves the file itself. Blocks
/// inside `DoDragDrop` until the user drops or cancels. Must run on the UI thread: winit
/// has initialised OLE there for its drop target.
pub fn start_drag_out(paths: &[PathBuf], button: DragButton) -> io::Result<DragOutcome> {
    let data = data_object(paths)?;
    let preferred = match button {
        DragButton::Left => DROPEFFECT_MOVE,
        DragButton::Right => DROPEFFECT_COPY,
    };
    let allowed = match set_preferred_effect(&data, preferred) {
        Ok(()) => DROPEFFECT_COPY | DROPEFFECT_MOVE,
        Err(e) => {
            // Without it Explorer picks copy or move by its own rules; copy only is safe.
            println!("preferred drop effect not set, copy only: {e}");
            DROPEFFECT_COPY
        }
    };
    let source: IDropSource = Source { button }.into();
    let mut effect = DROPEFFECT::default();
    let res = unsafe { DoDragDrop(&data, &source, allowed, &mut effect) };
    // After DoDragDrop the event loop slept through every repaint request, its own timer
    // included, until some message reached this thread (the mouse over the shelf). The
    // row of a moved file stayed, the hotkey went unanswered. An empty message wakes it.
    unsafe {
        let _ = PostThreadMessageW(GetCurrentThreadId(), WM_NULL, WPARAM(0), LPARAM(0));
    }
    Ok(outcome(res, effect))
}

/// Any drop counts, whatever effect comes back. After a move Explorer reports no effect
/// ("optimized move": it moved the file itself, the source must not delete it), so the
/// effect cannot tell a move from a refused drop. The presence watch tells them apart.
fn outcome(res: windows::core::HRESULT, _effect: DROPEFFECT) -> DragOutcome {
    if res == DRAGDROP_S_DROP {
        DragOutcome::Dropped
    } else {
        DragOutcome::Cancelled
    }
}

fn preferred_drop_effect_format() -> u16 {
    // CFSTR_PREFERREDDROPEFFECT. Registered formats are 0xC000..=0xFFFF.
    unsafe { RegisterClipboardFormatW(w!("Preferred DropEffect")) as u16 }
}

/// Tells the drop target which action to take by default ("Preferred DropEffect"). The
/// target may ignore it. The data object takes the memory and frees it itself.
fn set_preferred_effect(data: &IDataObject, effect: DROPEFFECT) -> io::Result<()> {
    let format = FORMATETC {
        cfFormat: preferred_drop_effect_format(),
        ptd: std::ptr::null_mut(),
        dwAspect: DVASPECT_CONTENT.0,
        lindex: -1,
        tymed: TYMED_HGLOBAL.0 as u32,
    };
    unsafe {
        let h = GlobalAlloc(GMEM_MOVEABLE, 4).map_err(io::Error::other)?;
        let p = GlobalLock(h) as *mut u32;
        if p.is_null() {
            let _ = GlobalFree(Some(h));
            return Err(io::Error::other("GlobalLock failed"));
        }
        *p = effect.0;
        let _ = GlobalUnlock(h);
        let medium = STGMEDIUM {
            tymed: TYMED_HGLOBAL.0 as u32,
            u: STGMEDIUM_0 { hGlobal: h },
            pUnkForRelease: ManuallyDrop::new(None),
        };
        data.SetData(&format, &medium, true).map_err(|e| {
            let _ = GlobalFree(Some(h));
            io::Error::other(e)
        })
    }
}

/// The shell's own data object for `paths` (CF_HDROP and the shell formats). The paths go in
/// as they are: the `drag` crate canonicalised them first, and `\\?\UNC\...` from a network
/// path made `ILCreateFromPathW` fail and the crate panic.
fn data_object(paths: &[PathBuf]) -> io::Result<IDataObject> {
    let pidls: Vec<*mut ITEMIDLIST> = paths
        .iter()
        .map(|p| {
            let wide: Vec<u16> = p.as_os_str().encode_wide().chain([0]).collect();
            unsafe { ILCreateFromPathW(PCWSTR(wide.as_ptr())) }
        })
        .collect();
    let result = if pidls.iter().any(|p| p.is_null()) {
        Err(io::Error::other("the shell does not know one of the paths"))
    } else {
        let list: Vec<*const ITEMIDLIST> = pidls.iter().map(|p| p.cast_const()).collect();
        unsafe {
            SHCreateShellItemArrayFromIDLists(&list)
                .and_then(|items: IShellItemArray| items.BindToHandler(None, &BHID_DataObject))
        }
        .map_err(io::Error::other)
    };
    for p in pidls.into_iter().filter(|p| !p.is_null()) {
        unsafe { ILFree(Some(p.cast_const())) };
    }
    result
}

#[implement(IDropSource)]
struct Source {
    /// The file drops when this button is let go.
    button: DragButton,
}

impl IDropSource_Impl for Source_Impl {
    fn QueryContinueDrag(
        &self,
        escape: windows::core::BOOL,
        keys: MODIFIERKEYS_FLAGS,
    ) -> windows::core::HRESULT {
        let held = match self.button {
            DragButton::Left => MK_LBUTTON,
            DragButton::Right => MK_RBUTTON,
        };
        if escape.as_bool() {
            DRAGDROP_S_CANCEL
        } else if keys & held == MODIFIERKEYS_FLAGS(0) {
            DRAGDROP_S_DROP
        } else {
            S_OK
        }
    }

    fn GiveFeedback(&self, _: DROPEFFECT) -> windows::core::HRESULT {
        DRAGDROP_S_USEDEFAULTCURSORS
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::System::Ole::{OleInitialize, ReleaseStgMedium, DROPEFFECT_NONE};

    fn temp_file(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("shelve-{}-drag", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join(name);
        std::fs::write(&p, b"x").unwrap();
        p
    }

    #[test]
    fn data_object_for_local_files() {
        let _ = unsafe { OleInitialize(None) };
        let a = temp_file("а файл.txt");
        assert!(data_object(&[a.clone(), a.with_file_name("а файл.txt")]).is_ok());
    }

    /// Reads "Preferred DropEffect" back the way Explorer does.
    fn preferred_effect(data: &IDataObject) -> Option<u32> {
        let format = FORMATETC {
            cfFormat: preferred_drop_effect_format(),
            ptd: std::ptr::null_mut(),
            dwAspect: DVASPECT_CONTENT.0,
            lindex: -1,
            tymed: TYMED_HGLOBAL.0 as u32,
        };
        let mut medium = unsafe { data.GetData(&format) }.ok()?;
        let effect = unsafe {
            let h = medium.u.hGlobal;
            let p = GlobalLock(h) as *const u32;
            let effect = (!p.is_null()).then(|| *p);
            let _ = GlobalUnlock(h);
            effect
        };
        unsafe { ReleaseStgMedium(&mut medium) };
        effect
    }

    #[test]
    fn data_object_with_preferred_effect() {
        // Explorer reads it to pick move over copy for a plain left-button drop.
        let _ = unsafe { OleInitialize(None) };
        let a = temp_file("перенос.txt");
        let data = data_object(&[a]).unwrap();
        set_preferred_effect(&data, DROPEFFECT_MOVE).unwrap();
        assert_eq!(preferred_effect(&data), Some(DROPEFFECT_MOVE.0));
    }

    #[test]
    fn a_drop_with_no_effect_still_counts_as_dropped() {
        // Explorer moves the file itself ("optimized move") and then reports no effect,
        // so that the source deletes nothing. The row must still be watched.
        assert_eq!(
            outcome(DRAGDROP_S_DROP, DROPEFFECT_NONE),
            DragOutcome::Dropped
        );
        assert_eq!(
            outcome(DRAGDROP_S_DROP, DROPEFFECT_COPY),
            DragOutcome::Dropped
        );
        assert_eq!(
            outcome(DRAGDROP_S_CANCEL, DROPEFFECT_NONE),
            DragOutcome::Cancelled
        );
    }

    #[test]
    fn data_object_for_network_path() {
        // Bug: dragging a file from \\server\share crashed the app inside the `drag` crate.
        let _ = unsafe { OleInitialize(None) };
        let local = temp_file("сетевой файл.txt");
        let s = local.to_string_lossy();
        let Some(rest) = s.strip_prefix("C:\\").or_else(|| s.strip_prefix("D:\\")) else {
            return;
        };
        let drive = &s[..1];
        let unc = PathBuf::from(format!(r"\\localhost\{drive}$\{rest}"));
        if !unc.exists() {
            println!("skipped: admin share {drive}$ not reachable");
            return;
        }
        assert!(data_object(&[unc]).is_ok());
    }

    #[test]
    fn data_object_for_unknown_path_is_an_error_not_a_panic() {
        let _ = unsafe { OleInitialize(None) };
        assert!(data_object(&[PathBuf::from(r"\\?\UNC\localhost\нет\такого")]).is_err());
    }
}
