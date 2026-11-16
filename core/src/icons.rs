//! Extracts an executable's icon with the Windows shell and returns it as a PNG data URI for the UI.

use std::mem::{size_of, zeroed};

use windows_sys::Win32::Graphics::Gdi::{DeleteObject, GetDC, GetDIBits, GetObjectW, ReleaseDC, BITMAP, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS};
use windows_sys::Win32::UI::Shell::{SHGetFileInfoW, SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON};
use windows_sys::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, ICONINFO};

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if c.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    out
}

/// 32x32 icon of `path` as `data:image/png;base64,...`, or None if it has no usable icon.
pub fn exe_icon_data_uri(path: &str) -> Option<String> {
    unsafe {
        let mut fi: SHFILEINFOW = zeroed();
        let r = SHGetFileInfoW(wide(path).as_ptr(), 0, &mut fi, size_of::<SHFILEINFOW>() as u32, SHGFI_ICON | SHGFI_LARGEICON);
        if r == 0 || fi.hIcon == 0 {
            return None;
        }
        let hicon = fi.hIcon;
        let mut ii: ICONINFO = zeroed();
        if GetIconInfo(hicon, &mut ii) == 0 {
            DestroyIcon(hicon);
            return None;
        }
        let rgba = icon_rgba(&ii);
        if ii.hbmColor != 0 {
            DeleteObject(ii.hbmColor);
        }
        if ii.hbmMask != 0 {
            DeleteObject(ii.hbmMask);
        }
        DestroyIcon(hicon);
        let (w, h, px) = rgba?;
        let mut png_bytes = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut png_bytes, w, h);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            let mut wr = enc.write_header().ok()?;
            wr.write_image_data(&px).ok()?;
        }
        Some(format!("data:image/png;base64,{}", base64(&png_bytes)))
    }
}

unsafe fn icon_rgba(ii: &ICONINFO) -> Option<(u32, u32, Vec<u8>)> {
    if ii.hbmColor == 0 {
        return None; // monochrome icon: not worth rendering
    }
    let mut bm: BITMAP = zeroed();
    if GetObjectW(ii.hbmColor, size_of::<BITMAP>() as i32, &mut bm as *mut _ as *mut _) == 0 {
        return None;
    }
    let (w, h) = (bm.bmWidth, bm.bmHeight);
    if w <= 0 || h <= 0 || w > 256 || h > 256 {
        return None;
    }
    let hdc = GetDC(0);
    let mut bi: BITMAPINFO = zeroed();
    bi.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
    bi.bmiHeader.biWidth = w;
    bi.bmiHeader.biHeight = -h; // top-down
    bi.bmiHeader.biPlanes = 1;
    bi.bmiHeader.biBitCount = 32;
    bi.bmiHeader.biCompression = BI_RGB;
    let n = (w * h * 4) as usize;
    let mut color = vec![0u8; n];
    let ok = GetDIBits(hdc, ii.hbmColor, 0, h as u32, color.as_mut_ptr() as *mut _, &mut bi, DIB_RGB_COLORS);
    let mut mask = vec![0u8; n];
    let mut bi2 = bi;
    let okm = if ii.hbmMask != 0 { GetDIBits(hdc, ii.hbmMask, 0, h as u32, mask.as_mut_ptr() as *mut _, &mut bi2, DIB_RGB_COLORS) } else { 0 };
    ReleaseDC(0, hdc);
    if ok == 0 {
        return None;
    }
    // Modern icons carry alpha in the colour bitmap; older ones rely on the AND mask (white = transparent).
    let has_alpha = color.chunks_exact(4).any(|p| p[3] != 0);
    let mut px = Vec::with_capacity(n);
    for i in 0..(w * h) as usize {
        let (b, g, r, a) = (color[i * 4], color[i * 4 + 1], color[i * 4 + 2], color[i * 4 + 3]);
        let alpha = if has_alpha { a } else if okm != 0 && mask[i * 4] > 127 { 0 } else { 255 };
        px.extend_from_slice(&[r, g, b, alpha]);
    }
    Some((w as u32, h as u32, px))
}
