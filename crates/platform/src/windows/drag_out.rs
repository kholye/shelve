use std::io;
use std::os::windows::ffi::OsStrExt;
use std::path::PathBuf;

use windows::core::{implement, PCWSTR};
use windows::Win32::Foundation::{
    DRAGDROP_S_CANCEL, DRAGDROP_S_DROP, DRAGDROP_S_USEDEFAULTCURSORS, S_OK,
};
use windows::Win32::System::Com::IDataObject;
use windows::Win32::System::Ole::{
    DoDragDrop, IDropSource, IDropSource_Impl, DROPEFFECT, DROPEFFECT_COPY,
};
use windows::Win32::System::SystemServices::{MK_LBUTTON, MODIFIERKEYS_FLAGS};
use windows::Win32::UI::Shell::Common::ITEMIDLIST;
use windows::Win32::UI::Shell::{
    BHID_DataObject, ILCreateFromPathW, ILFree, IShellItemArray, SHCreateShellItemArrayFromIDLists,
};

use crate::DragOutcome;

/// OLE drag of the files themselves, copy only (the shelf never moves files). Blocks inside
/// `DoDragDrop` until the user drops or cancels. Must run on the UI thread: winit has
/// initialised OLE there for its drop target.
pub fn start_drag_out(paths: &[PathBuf]) -> io::Result<DragOutcome> {
    let data = data_object(paths)?;
    let source: IDropSource = Source.into();
    let mut effect = DROPEFFECT::default();
    let res = unsafe { DoDragDrop(&data, &source, DROPEFFECT_COPY, &mut effect) };
    Ok(if res == DRAGDROP_S_DROP {
        DragOutcome::Copied
    } else {
        DragOutcome::Cancelled
    })
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
struct Source;

impl IDropSource_Impl for Source_Impl {
    fn QueryContinueDrag(
        &self,
        escape: windows::core::BOOL,
        keys: MODIFIERKEYS_FLAGS,
    ) -> windows::core::HRESULT {
        if escape.as_bool() {
            DRAGDROP_S_CANCEL
        } else if keys & MK_LBUTTON == MODIFIERKEYS_FLAGS(0) {
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
    use windows::Win32::System::Ole::OleInitialize;

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
