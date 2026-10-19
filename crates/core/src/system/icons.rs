//! Program icons for the UI.
//!
//! OWNER: system agent.

use std::ffi::c_void;
use std::path::Path;

use base64::Engine as _;
use windows::core::PCWSTR;
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits, GetObjectW, BITMAP, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
    DIB_RGB_COLORS, HBITMAP, HDC,
};
use windows::Win32::Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES;
use windows::Win32::UI::Shell::{ExtractIconExW, SHGetFileInfoW, SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON};
use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, PrivateExtractIconsW, HICON, ICONINFO};

use crate::system::registry::parse_icon_location;
use crate::system::{wide, ComGuard};

/// Extracts the icon designated by `hint` (a `DisplayIcon` value such as
/// `C:\app\app.exe,0`, or a plain .exe/.ico path, possibly quoted) and returns
/// it as a `data:image/png;base64,...` URL of `size`×`size` pixels (32 or 48),
/// with alpha. `None` when nothing usable is found.
pub fn icon_data_url(hint: &str, size: u32) -> Option<String> {
    let size = size.clamp(16, 256);
    let (path, index) = parse_icon_location(hint)?;
    if !path.exists() {
        return None;
    }
    let icon = extract_icon(&path, index, size)?;
    let (width, height, rgba) = icon_to_rgba(icon.0)?;
    let rgba = if (width, height) == (size, size) {
        rgba
    } else {
        resize(&rgba, width, height, size)
    };
    let png = encode_png(&rgba, size, size)?;
    Some(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(png)
    ))
}

/// An icon handle destroyed on drop.
struct OwnedIcon(HICON);

impl Drop for OwnedIcon {
    fn drop(&mut self) {
        // SAFETY: the icon was created by an extraction API for us alone.
        unsafe {
            let _ = DestroyIcon(self.0);
        }
    }
}

/// A GDI bitmap deleted on drop (null allowed).
struct OwnedBitmap(HBITMAP);

impl Drop for OwnedBitmap {
    fn drop(&mut self) {
        if !self.0.is_invalid() {
            // SAFETY: GetIconInfo hands us copies of the bitmaps; we own them.
            unsafe {
                let _ = DeleteObject(self.0.into());
            }
        }
    }
}

/// A memory device context deleted on drop.
struct MemoryDc(HDC);

impl MemoryDc {
    fn new() -> Option<MemoryDc> {
        // SAFETY: plain call; the DC is deleted in Drop.
        let dc = unsafe { CreateCompatibleDC(None) };
        (!dc.is_invalid()).then_some(MemoryDc(dc))
    }
}

impl Drop for MemoryDc {
    fn drop(&mut self) {
        // SAFETY: the DC was created by CreateCompatibleDC.
        unsafe {
            let _ = DeleteDC(self.0);
        }
    }
}

/// Icon at `index` (negative = resource id) of `path`, as close to `size` as
/// the file allows: exact size with `PrivateExtractIconsW`, then the large
/// shell icon with `ExtractIconExW`, then the file type's icon with
/// `SHGetFileInfoW`.
fn extract_icon(path: &Path, index: i32, size: u32) -> Option<OwnedIcon> {
    let wpath = wide(path);
    if wpath.len() <= 260 {
        let mut name = [0u16; 260];
        name[..wpath.len()].copy_from_slice(&wpath);
        let mut icons = [HICON::default()];
        // SAFETY: `name` is a NUL-terminated MAX_PATH buffer and `icons` has
        // room for the one icon requested.
        let count = unsafe { PrivateExtractIconsW(&name, index, size as i32, size as i32, Some(&mut icons), None, 0) };
        if count != 0 && count != u32::MAX && !icons[0].is_invalid() {
            return Some(OwnedIcon(icons[0]));
        }
    }

    let mut large = HICON::default();
    // SAFETY: `wpath` is NUL-terminated; `large` receives at most one icon.
    let count = unsafe { ExtractIconExW(PCWSTR(wpath.as_ptr()), index, Some(&mut large), None, 1) };
    if count != 0 && count != u32::MAX && !large.is_invalid() {
        return Some(OwnedIcon(large));
    }

    let _com = ComGuard::new();
    let mut info = SHFILEINFOW::default();
    // SAFETY: `info` is a properly sized SHFILEINFOW; `wpath` is NUL-terminated.
    let ok = unsafe {
        SHGetFileInfoW(
            PCWSTR(wpath.as_ptr()),
            FILE_FLAGS_AND_ATTRIBUTES(0),
            Some(&mut info),
            std::mem::size_of::<SHFILEINFOW>() as u32,
            SHGFI_ICON | SHGFI_LARGEICON,
        )
    };
    (ok != 0 && !info.hIcon.is_invalid()).then_some(OwnedIcon(info.hIcon))
}

/// Width, height and RGBA pixels (top-down, straight alpha) of an icon.
fn icon_to_rgba(icon: HICON) -> Option<(u32, u32, Vec<u8>)> {
    let mut info = ICONINFO::default();
    // SAFETY: `info` receives two bitmaps that we delete (OwnedBitmap).
    unsafe { GetIconInfo(icon, &mut info) }.ok()?;
    let color = OwnedBitmap(info.hbmColor);
    let mask = OwnedBitmap(info.hbmMask);
    let dc = MemoryDc::new()?;

    if !color.0.is_invalid() {
        let (width, height) = bitmap_size(color.0)?;
        let mut pixels = dib_bits(&dc, color.0, width, height)?;
        // Old-style icons have no alpha channel: transparency is in the mask.
        if pixels.as_chunks::<4>().0.iter().all(|px| px[3] == 0) {
            let mask_bits = if mask.0.is_invalid() {
                None
            } else {
                dib_bits(&dc, mask.0, width, height)
            };
            for (i, px) in pixels.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                let transparent = mask_bits.as_ref().is_some_and(|m| m[i * 4] != 0);
                px[3] = if transparent { 0 } else { 255 };
            }
        }
        for px in pixels.as_chunks_mut::<4>().0 {
            px.swap(0, 2); // BGRA → RGBA
        }
        return Some((width, height, pixels));
    }

    // Monochrome icon: the mask bitmap holds the AND mask above the XOR mask.
    if mask.0.is_invalid() {
        return None;
    }
    let (width, double_height) = bitmap_size(mask.0)?;
    let height = double_height / 2;
    if height == 0 {
        return None;
    }
    let bits = dib_bits(&dc, mask.0, width, double_height)?;
    let pixel_count = (width * height) as usize;
    let mut rgba = vec![0u8; pixel_count * 4];
    for i in 0..pixel_count {
        let and = bits[i * 4] != 0;
        let xor = bits[(pixel_count + i) * 4] != 0;
        let value = match (and, xor) {
            (true, false) => None,     // transparent
            (true, true) => Some(0u8), // inverted screen: draw black
            (false, white) => Some(if white { 255 } else { 0 }),
        };
        if let Some(v) = value {
            rgba[i * 4..i * 4 + 4].copy_from_slice(&[v, v, v, 255]);
        }
    }
    Some((width, height, rgba))
}

fn bitmap_size(bitmap: HBITMAP) -> Option<(u32, u32)> {
    let mut bm = BITMAP::default();
    // SAFETY: `bm` is a BITMAP and its size is passed.
    let written = unsafe {
        GetObjectW(
            bitmap.into(),
            std::mem::size_of::<BITMAP>() as i32,
            Some((&mut bm as *mut BITMAP).cast::<c_void>()),
        )
    };
    let (width, height) = (bm.bmWidth.unsigned_abs(), bm.bmHeight.unsigned_abs());
    (written != 0 && (1..=1024).contains(&width) && (1..=2048).contains(&height)).then_some((width, height))
}

/// The bitmap as 32-bit top-down BGRA.
fn dib_bits(dc: &MemoryDc, bitmap: HBITMAP, width: u32, height: u32) -> Option<Vec<u8>> {
    let mut bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width as i32,
            biHeight: -(height as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut pixels = vec![0u8; (width * height * 4) as usize];
    // SAFETY: `pixels` holds height rows of width 32-bit pixels, as described
    // by `bmi`; 32-bit BI_RGB needs no color table beyond bmiColors.
    let lines = unsafe {
        GetDIBits(
            dc.0,
            bitmap,
            0,
            height,
            Some(pixels.as_mut_ptr().cast()),
            &mut bmi,
            DIB_RGB_COLORS,
        )
    };
    (lines == height as i32).then_some(pixels)
}

/// Resamples straight-alpha RGBA to `size`×`size`: box filter when
/// shrinking, bilinear when enlarging, on premultiplied values.
fn resize(src: &[u8], width: u32, height: u32, size: u32) -> Vec<u8> {
    let (w, h) = (width as usize, height as usize);
    let premultiplied: Vec<[f32; 4]> = src
        .as_chunks::<4>()
        .0
        .iter()
        .map(|px| {
            let a = px[3] as f32 / 255.0;
            [px[0] as f32 * a, px[1] as f32 * a, px[2] as f32 * a, px[3] as f32]
        })
        .collect();
    let at = |x: usize, y: usize| premultiplied[y.min(h - 1) * w + x.min(w - 1)];
    let (scale_x, scale_y) = (width as f32 / size as f32, height as f32 / size as f32);
    let mut out = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size as usize {
        for x in 0..size as usize {
            let mut acc = [0f32; 4];
            if scale_x >= 1.0 && scale_y >= 1.0 {
                let x0 = (x as f32 * scale_x) as usize;
                let x1 = (((x + 1) as f32 * scale_x).ceil() as usize).clamp(x0 + 1, w);
                let y0 = (y as f32 * scale_y) as usize;
                let y1 = (((y + 1) as f32 * scale_y).ceil() as usize).clamp(y0 + 1, h);
                for sy in y0..y1 {
                    for sx in x0..x1 {
                        let px = at(sx, sy);
                        (0..4).for_each(|c| acc[c] += px[c]);
                    }
                }
                let n = ((x1 - x0) * (y1 - y0)) as f32;
                (0..4).for_each(|c| acc[c] /= n);
            } else {
                let fx = ((x as f32 + 0.5) * scale_x - 0.5).clamp(0.0, (w - 1) as f32);
                let fy = ((y as f32 + 0.5) * scale_y - 0.5).clamp(0.0, (h - 1) as f32);
                let (x0, y0) = (fx as usize, fy as usize);
                let (tx, ty) = (fx - x0 as f32, fy - y0 as f32);
                let (a, b, c, d) = (at(x0, y0), at(x0 + 1, y0), at(x0, y0 + 1), at(x0 + 1, y0 + 1));
                for i in 0..4 {
                    let top = a[i] + (b[i] - a[i]) * tx;
                    let bottom = c[i] + (d[i] - c[i]) * tx;
                    acc[i] = top + (bottom - top) * ty;
                }
            }
            let alpha = acc[3];
            let unpremultiply = |v: f32| {
                if alpha > 0.0 {
                    (v * 255.0 / alpha).round().clamp(0.0, 255.0) as u8
                } else {
                    0
                }
            };
            out.extend_from_slice(&[
                unpremultiply(acc[0]),
                unpremultiply(acc[1]),
                unpremultiply(acc[2]),
                alpha.round().clamp(0.0, 255.0) as u8,
            ]);
        }
    }
    out
}

fn encode_png(rgba: &[u8], width: u32, height: u32) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().ok()?;
    writer.write_image_data(rgba).ok()?;
    writer.finish().ok()?;
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Decodes a data URL produced by `icon_data_url`: (width, height, RGBA).
    fn decode(url: &str) -> (u32, u32, Vec<u8>) {
        let b64 = url.strip_prefix("data:image/png;base64,").expect("PNG data URL");
        let bytes = base64::engine::general_purpose::STANDARD.decode(b64).expect("base64");
        let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
        let mut reader = decoder.read_info().expect("PNG header");
        let mut buf = vec![0u8; reader.output_buffer_size().expect("buffer size")];
        let info = reader.next_frame(&mut buf).expect("PNG frame");
        assert_eq!(info.color_type, png::ColorType::Rgba);
        assert_eq!(info.bit_depth, png::BitDepth::Eight);
        buf.truncate(info.buffer_size());
        (info.width, info.height, buf)
    }

    fn assert_icon(url: Option<String>, size: u32) {
        let url = url.expect("an icon");
        let (w, h, rgba) = decode(&url);
        assert_eq!((w, h), (size, size));
        let alphas: Vec<u8> = rgba.as_chunks::<4>().0.iter().map(|px| px[3]).collect();
        assert!(alphas.contains(&0), "expected transparent pixels");
        assert!(alphas.iter().any(|&a| a > 200), "expected opaque pixels");
    }

    fn windir() -> String {
        std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into())
    }

    #[test]
    fn explorer_icon() {
        assert_icon(icon_data_url(&format!(r"{}\explorer.exe", windir()), 32), 32);
        assert_icon(icon_data_url(&format!(r#""{}\explorer.exe",0"#, windir()), 48), 48);
    }

    #[test]
    fn notepad_icon() {
        assert_icon(icon_data_url(&format!(r"{}\System32\notepad.exe,0", windir()), 48), 48);
        assert_icon(icon_data_url(r"%SystemRoot%\System32\notepad.exe", 32), 32);
    }

    #[test]
    fn resource_id_and_fallbacks() {
        // Negative index = resource id (shell32 icon #4: open folder), which
        // is not the first icon of the file.
        let by_id = icon_data_url(&format!(r"{}\System32\shell32.dll,-4", windir()), 32).expect("resource id 4");
        let first = icon_data_url(&format!(r"{}\System32\shell32.dll,0", windir()), 32).expect("first icon");
        assert_ne!(by_id, first);
        // Not an icon container: the file type's icon.
        let toml = concat!(env!("CARGO_MANIFEST_DIR"), r"\Cargo.toml");
        let (w, h, _) = decode(&icon_data_url(toml, 48).expect("file type icon"));
        assert_eq!((w, h), (48, 48));
        assert_eq!(icon_data_url(r"C:\does\not\exist.exe,0", 32), None);
        assert_eq!(icon_data_url("", 32), None);
    }

    #[test]
    fn resizing_keeps_alpha() {
        // 2×2: opaque red, transparent, opaque blue, transparent.
        let src = [255, 0, 0, 255, 0, 0, 0, 0, 0, 0, 255, 255, 0, 0, 0, 0];
        let up = resize(&src, 2, 2, 4);
        assert_eq!(up.len(), 4 * 4 * 4);
        assert_eq!(&up[0..4], &[255, 0, 0, 255]);
        let down = resize(&up, 4, 4, 1);
        assert_eq!(down.len(), 4);
        assert!((100..=155).contains(&down[3]), "{down:?}");
        // Color is not darkened by transparent neighbours (premultiplied).
        assert_eq!(down[1], 0);
        assert!(down[0] > 100 && down[2] > 100, "{down:?}");
    }
}
