use std::os::windows::ffi::OsStrExt;
use std::path::Path;

use windows::core::PCWSTR;
use windows::Win32::Graphics::Gdi::{
    DeleteObject, GetDC, GetDIBits, GetObjectW, ReleaseDC, BITMAP, BITMAPINFO, BITMAPINFOHEADER,
    BI_RGB, DIB_RGB_COLORS,
};
use windows::Win32::Storage::FileSystem::{FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_NORMAL};
use windows::Win32::UI::Shell::{
    SHGetFileInfoW, SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON, SHGFI_USEFILEATTRIBUTES,
};
use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, ICONINFO};

/// Large (32 px at 100% scale) shell icon as (width, height, RGBA). A missing path gets the generic icon for its type.
pub fn file_icon(path: &Path, is_dir: bool) -> Option<(u32, u32, Vec<u8>)> {
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
    let mut flags = SHGFI_ICON | SHGFI_LARGEICON;
    if !path.exists() {
        flags |= SHGFI_USEFILEATTRIBUTES;
    }
    let attrs = if is_dir {
        FILE_ATTRIBUTE_DIRECTORY
    } else {
        FILE_ATTRIBUTE_NORMAL
    };
    let mut info = SHFILEINFOW::default();
    unsafe {
        let ok = SHGetFileInfoW(
            PCWSTR(wide.as_ptr()),
            attrs,
            Some(&mut info),
            size_of::<SHFILEINFOW>() as u32,
            flags,
        );
        if ok == 0 || info.hIcon.is_invalid() {
            return None;
        }
        let rgba = icon_rgba(&info);
        let _ = DestroyIcon(info.hIcon);
        rgba
    }
}

unsafe fn icon_rgba(info: &SHFILEINFOW) -> Option<(u32, u32, Vec<u8>)> {
    let mut ii = ICONINFO::default();
    unsafe { GetIconInfo(info.hIcon, &mut ii) }.ok()?;
    let result = (|| {
        let mut bm = BITMAP::default();
        let got = unsafe {
            GetObjectW(
                ii.hbmColor.into(),
                size_of::<BITMAP>() as i32,
                Some(&mut bm as *mut _ as *mut _),
            )
        };
        if got == 0 || bm.bmWidth <= 0 || bm.bmHeight <= 0 {
            return None;
        }
        let (w, h) = (bm.bmWidth, bm.bmHeight);
        let mut bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h, // top-down rows
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut px = vec![0u8; (w * h * 4) as usize];
        let dc = unsafe { GetDC(None) };
        let lines = unsafe {
            GetDIBits(
                dc,
                ii.hbmColor,
                0,
                h as u32,
                Some(px.as_mut_ptr().cast()),
                &mut bmi,
                DIB_RGB_COLORS,
            )
        };
        unsafe { ReleaseDC(None, dc) };
        if lines == 0 {
            return None;
        }
        // Note: icons without an alpha channel are shown fully opaque (mask ignored); rare today.
        let has_alpha = px.as_chunks::<4>().0.iter().any(|p| p[3] != 0);
        for p in px.as_chunks_mut::<4>().0 {
            p.swap(0, 2); // BGRA -> RGBA
            if !has_alpha {
                p[3] = 255;
            }
        }
        Some((w as u32, h as u32, px))
    })();
    unsafe {
        let _ = DeleteObject(ii.hbmColor.into());
        let _ = DeleteObject(ii.hbmMask.into());
    }
    result
}
