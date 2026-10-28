//! 从 exe / lnk / dll 里取图标，缓存成 PNG 给速达磁贴用。
//!
//! 首选 `PrivateExtractIconsW`：一次调用就能指定 48×48，在 150% 缩放的机器上磁贴不糊。
//! 拿不到再退回 `ExtractAssociatedIconW`（固定 32×32，但一定有）。
//! 两个 API 都吃定长路径缓冲（260 / 128 WCHAR），超长路径直接报错而不是静默截断。

use image::ImageFormat;
use windows::Win32::Foundation::{HINSTANCE, HWND};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, ReleaseDC, SelectObject,
    BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HGDIOBJ,
};
use windows::Win32::UI::Shell::ExtractAssociatedIconW;
use windows::Win32::UI::WindowsAndMessaging::{
    DestroyIcon, DrawIconEx, PrivateExtractIconsW, DI_NORMAL, HICON, LR_DEFAULTSIZE,
};

use crate::error::{Error, Result};

/// 磁贴图标尺寸（Windows「超大图标」档）。
pub const ICON_PX: i32 = 48;

fn wide_fixed<const N: usize>(s: &str) -> std::result::Result<[u16; N], Error> {
    if s.encode_utf16().count() + 1 > N {
        return Err(Error::msg(
            "路径太长（Windows 传统 API 只吃 260 个字符），请把目标放到更短的目录",
        ));
    }
    let mut buf = [0u16; N];
    for (i, unit) in s.encode_utf16().enumerate() {
        buf[i] = unit;
    }
    Ok(buf)
}

/// 取图标并编成 PNG 字节。失败由调用方降级处理（磁贴回退到首字母），不弹错。
pub fn extract_png(path: &str) -> Result<Vec<u8>> {
    let size = ICON_PX;
    let mut icon = HICON::default();

    // 两个 API 都是定长缓冲。超长要第一个说：这种路径我们根本递不进 API，
    // 先查存在性的话会把「路径太长」误报成「文件不存在」，指错方向。
    if wide_fixed::<260>(path).is_err() && wide_fixed::<128>(path).is_err() {
        return Err(wide_fixed::<260>(path).err().unwrap());
    }

    // 文件不存在要单独说：ExtractAssociatedIconW 对不存在的路径会返回一个通用文档图标，
    // 不先拦下来的话，磁贴就会带着一个看起来正常的假图标进库。
    if !std::path::Path::new(path).exists() {
        return Err(Error::msg("文件不存在，可能被移动或删除了"));
    }

    if let Ok(buf) = wide_fixed::<260>(path) {
        let mut out_icons = [HICON::default(); 1];
        let mut ids = [0u32; 1];
        let got = unsafe {
            PrivateExtractIconsW(
                &buf,
                0,
                size,
                size,
                Some(&mut out_icons),
                Some(&mut ids as *mut u32),
                LR_DEFAULTSIZE.0,
            )
        };
        if got > 0 {
            icon = out_icons[0];
        }
    }

    if icon.is_invalid() {
        // 退路：关联图标。它会把路径就地改写，所以要一个可写的缓冲。
        if let Ok(mut buf) = wide_fixed::<128>(path) {
            let mut index: u16 = 0;
            let fallback =
                unsafe { ExtractAssociatedIconW(Some(HINSTANCE::default()), &mut buf, &mut index) };
            if !fallback.is_invalid() {
                icon = fallback;
            }
        }
    }

    if icon.is_invalid() {
        return Err(Error::msg("这个文件里没有可提取的图标"));
    }

    let rgba = unsafe { icon_to_rgba(icon, size as u32) };
    unsafe {
        let _ = DestroyIcon(icon);
    }
    encode_png(rgba.0, rgba.1, rgba.2)
}

/// HICON → 直 alpha 的 RGBA 缓冲。
///
/// DrawIconEx 画进 32bpp DIB 得到的是预乘 alpha，PNG 要直通 alpha，所以最后除回去；
/// alpha 为 0 的像素直接给全透明，避免除零。
unsafe fn icon_to_rgba(icon: HICON, size: u32) -> (Vec<u8>, u32, u32) {
    let screen = GetDC(Some(HWND::default()));
    let mem_dc = CreateCompatibleDC(Some(screen));

    let bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: size as i32,
            // 负高度 = top-down，行序与 PNG 一致，省掉一次翻转
            biHeight: -(size as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };

    let mut bits_ptr: *mut core::ffi::c_void = std::ptr::null_mut();
    let dib = CreateDIBSection(Some(mem_dc), &bmi, DIB_RGB_COLORS, &mut bits_ptr, None, 0);
    let dib_obj = match dib {
        Ok(handle) => HGDIOBJ(handle.0),
        Err(_) => HGDIOBJ::default(),
    };

    let mut out: Vec<u8> = Vec::new();
    let len = (size * size * 4) as usize;
    if !bits_ptr.is_null() {
        let old = SelectObject(mem_dc, dib_obj);
        std::slice::from_raw_parts_mut(bits_ptr as *mut u8, len).fill(0);
        let _ = DrawIconEx(
            mem_dc,
            0,
            0,
            icon,
            size as i32,
            size as i32,
            0,
            None,
            DI_NORMAL,
        );

        let premul = std::slice::from_raw_parts(bits_ptr as *const u8, len);
        out = Vec::with_capacity(len);
        for px in premul.as_chunks::<4>().0 {
            let (b, g, r, a) = (px[0], px[1], px[2], px[3]);
            if a == 0 {
                out.extend_from_slice(&[0, 0, 0, 0]);
            } else if a == 255 {
                out.extend_from_slice(&[r, g, b, 255]);
            } else {
                let f = |c: u8| ((c as u32 * 255) / a as u32).min(255) as u8;
                out.extend_from_slice(&[f(r), f(g), f(b), a]);
            }
        }
        let _ = SelectObject(mem_dc, old);
    }

    let _ = DeleteObject(dib_obj);
    let _ = DeleteDC(mem_dc);
    ReleaseDC(Some(HWND::default()), screen);
    (out, size, size)
}

fn encode_png(pixels: Vec<u8>, w: u32, h: u32) -> Result<Vec<u8>> {
    let img = image::RgbaImage::from_raw(w, h, pixels)
        .ok_or_else(|| Error::msg("图标像素缓冲尺寸对不上"))?;
    let mut cursor = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(img)
        .write_to(&mut cursor, ImageFormat::Png)
        .map_err(|e| Error::msg(format!("图标编码 PNG 失败：{e}")))?;
    Ok(cursor.into_inner())
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    /// 拿一个一定存在的系统 exe 验证整条链路：提取 → 解预乘 → 编 PNG。
    /// 断言只看结构（PNG magic + 有非透明像素），不依赖图标具体长什么样。
    #[test]
    fn extracts_a_real_icon_into_a_png() {
        let Ok(root) = std::env::var("SystemRoot") else {
            eprintln!("跳过：没有 SystemRoot");
            return;
        };
        let path = format!(r"{root}\System32\notepad.exe");
        if !std::path::Path::new(&path).exists() {
            eprintln!("跳过：找不到 {path}");
            return;
        }

        let png = extract_png(&path).expect("提取图标");
        assert!(png.len() > 200, "PNG 太小，不像真图标：{}", png.len());
        assert_eq!(
            &png[..8],
            &[0x89, b'P', b'N', b'G', 13, 10, 26, 10],
            "PNG magic 不对"
        );

        let decoded = image::load_from_memory(&png).expect("PNG 可解回");
        assert_eq!(decoded.width(), ICON_PX as u32);
        assert_eq!(decoded.height(), ICON_PX as u32);
        let opaque = decoded
            .into_rgba8()
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|px| px[3] > 200)
            .count();
        assert!(opaque > 100, "几乎没有实心像素，多半是透明空图：{opaque}");
    }

    #[test]
    fn missing_file_is_reported_rather_than_answered_with_a_generic_glyph() {
        let err = extract_png(r"Z:\nope\nothing-here.exe").unwrap_err();
        assert!(
            err.to_string().contains("不存在"),
            "缺文件要说成缺文件，不能悄悄给一张通用图标：{err}"
        );
    }

    #[test]
    fn overlong_paths_are_refused_not_silently_truncated() {
        let long = format!(r"C:\{}x", "a".repeat(300));
        assert!(wide_fixed::<260>(&long).is_err());
        // 超长路径要单独说清楚，不能混进「没有图标」里让人去找文件问题
        let err = extract_png(&long).unwrap_err();
        assert!(err.to_string().contains("路径太长"), "{err}");
    }
}
