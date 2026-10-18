//! Optional picture drawn behind the sliders.
//!
//! The file is decoded with WIC on a short-lived thread, so a large photo never
//! holds up the window, and shrunk to the smallest size that still covers the
//! window. The result comes back to the window with `WM_APP_BACKGROUND_READY`.

use std::ffi::{OsString, c_void};
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use std::thread;
use windows::core::{HSTRING, Interface, PCWSTR, PWSTR, Result, w};
use windows::Win32::Foundation::{GENERIC_READ, HWND, LPARAM, WPARAM};
use windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F;
use windows::Win32::Graphics::Imaging::{
    CLSID_WICImagingFactory, GUID_WICPixelFormat32bppPBGRA, IWICBitmapSource,
    IWICImagingFactory, WICBitmapDitherTypeNone, WICBitmapInterpolationModeFant,
    WICBitmapPaletteTypeMedianCut, WICDecodeMetadataCacheOnDemand,
};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize,
};
use windows::Win32::UI::Controls::Dialogs::{
    GetOpenFileNameW, OFN_EXPLORER, OFN_FILEMUSTEXIST, OFN_NOCHANGEDIR, OFN_PATHMUSTEXIST,
    OPENFILENAMEW,
};
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_APP};
use crate::log::debug_log;

/// wParam: the generation passed to `load`; lParam: a `Loaded`, see `take_loaded`.
pub const WM_APP_BACKGROUND_READY: u32 = WM_APP + 3;

/// What a `load` thread sends back: the picture, or why it couldn't be read.
pub type Loaded = std::result::Result<Picture, String>;

/// A decoded picture in premultiplied BGRA, the layout Direct2D bitmaps use.
pub struct Picture {
    pub width: u32,
    pub height: u32,
    /// `height` rows of `width * 4` bytes.
    pub pixels: Vec<u8>,
    /// The same picture, blurred, for the frosted glass panels. Same size as `pixels`.
    pub blurred: Vec<u8>,
    /// Decoded at the file's own size: a bigger window can't get a sharper copy.
    full_size: bool,
}

impl Picture {
    /// Whether this copy is big enough to fill a `width` x `height` pixel window.
    pub fn covers(&self, width: u32, height: u32) -> bool {
        self.full_size || (self.width >= width && self.height >= height)
    }
}

/// Smallest size of a `src_w` x `src_h` picture that still covers `width` x `height`,
/// keeping its aspect ratio and never enlarging it.
fn cover_size(src_w: u32, src_h: u32, width: u32, height: u32) -> (u32, u32) {
    let scale = (width as f64 / src_w as f64)
        .max(height as f64 / src_h as f64)
        .min(1.0);
    let fit = |n: u32| ((n as f64 * scale).ceil() as u32).min(n).max(1);
    (fit(src_w), fit(src_h))
}

/// The centered part of a `bitmap_w` x `bitmap_h` picture that has the aspect ratio of
/// a `width` x `height` area, so the picture fills the area without being stretched.
pub fn crop(bitmap_w: f32, bitmap_h: f32, width: f32, height: f32) -> D2D_RECT_F {
    let whole = D2D_RECT_F { left: 0.0, top: 0.0, right: bitmap_w, bottom: bitmap_h };
    if width <= 0.0 || height <= 0.0 || bitmap_w <= 0.0 || bitmap_h <= 0.0 {
        return whole;
    }
    let scale = (bitmap_w / width).min(bitmap_h / height);
    let left = (bitmap_w - width * scale) / 2.0;
    let top = (bitmap_h - height * scale) / 2.0;
    D2D_RECT_F { left, top, right: bitmap_w - left, bottom: bitmap_h - top }
}

/// Decodes the first frame of `path`, shrunk to cover `width` x `height` pixels.
/// COM must be initialized on the calling thread.
pub fn decode(path: &Path, width: u32, height: u32) -> Result<Picture> {
    unsafe {
        let factory: IWICImagingFactory =
            CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)?;
        let decoder = factory.CreateDecoderFromFilename(
            &HSTRING::from(path),
            None,
            GENERIC_READ,
            WICDecodeMetadataCacheOnDemand,
        )?;
        let frame = decoder.GetFrame(0)?;
        let converter = factory.CreateFormatConverter()?;
        converter.Initialize(
            &frame,
            &GUID_WICPixelFormat32bppPBGRA,
            WICBitmapDitherTypeNone,
            None,
            0.0,
            WICBitmapPaletteTypeMedianCut,
        )?;

        let (mut src_w, mut src_h) = (0, 0);
        converter.GetSize(&mut src_w, &mut src_h)?;
        let (w, h) = cover_size(src_w, src_h, width, height);
        let full_size = (w, h) == (src_w, src_h);
        let source: IWICBitmapSource = if full_size {
            converter.cast()?
        } else {
            let scaler = factory.CreateBitmapScaler()?;
            scaler.Initialize(&converter, w, h, WICBitmapInterpolationModeFant)?;
            scaler.cast()?
        };

        let stride = w * 4;
        let mut pixels = vec![0; stride as usize * h as usize];
        source.CopyPixels(std::ptr::null(), stride, &mut pixels)?;
        // Blurred here rather than at paint time, so the window thread never waits.
        let blurred = blur(&pixels, w, h, blur_radius(w, h));
        Ok(Picture { width: w, height: h, pixels, blurred, full_size })
    }
}

/// Radius of the frosted-glass blur, relative to the picture's size so it looks the
/// same whether the window is small or large.
fn blur_radius(width: u32, height: u32) -> u32 {
    (width.min(height) / 24).clamp(4, 32)
}

/// Two box-blur passes, which is smooth enough behind a translucent panel and needs no
/// shader. Blurring premultiplied pixels directly is correct, so all four channels are
/// averaged the same way.
fn blur(pixels: &[u8], width: u32, height: u32, radius: u32) -> Vec<u8> {
    let (w, h, r) = (width as usize, height as usize, radius as usize);
    let mut sharp = pixels.to_vec();
    let mut scratch = vec![0u8; pixels.len()];
    for _ in 0..2 {
        for y in 0..h {
            blur_run(&sharp, &mut scratch, y * w * 4, y * w * 4, 1, w, r);
        }
        for x in 0..w {
            blur_run(&scratch, &mut sharp, x * 4, x * 4, w, h, r);
        }
    }
    sharp
}

/// Averages one run of `len` pixels that start at `from` and sit `step` pixels apart,
/// writing the result to the same places in `dst`. Prefix sums keep the cost the same
/// whatever the radius is, and a window that runs off an edge just uses the pixels that
/// are there, so no halo appears.
fn blur_run(src: &[u8], dst: &mut [u8], from: usize, to: usize, step: usize, len: usize, radius: usize) {
    let mut sums = vec![[0u32; 4]; len + 1];
    for i in 0..len {
        let (before, at) = (sums[i], from + i * step * 4);
        sums[i + 1] = [
            before[0] + src[at] as u32,
            before[1] + src[at + 1] as u32,
            before[2] + src[at + 2] as u32,
            before[3] + src[at + 3] as u32,
        ];
    }
    for i in 0..len {
        let (low, high) = (i.saturating_sub(radius), (i + radius).min(len - 1));
        let count = (high - low + 1) as u32;
        let (before, after) = (sums[low], sums[high + 1]);
        let at = to + i * step * 4;
        for channel in 0..4 {
            dst[at + channel] = ((after[channel] - before[channel]) / count) as u8;
        }
    }
}

/// Decodes `path` on a new thread, then posts `WM_APP_BACKGROUND_READY` to `hwnd`
/// with `generation` so the window can ignore results it no longer wants.
pub fn load(hwnd: HWND, generation: usize, path: PathBuf, width: u32, height: u32) {
    // HWND isn't Send, but posting to a window is allowed from any thread.
    let hwnd = hwnd.0 as isize;
    let spawned = thread::Builder::new().name("background".into()).spawn(move || {
        let hwnd = HWND(hwnd as *mut c_void);
        let initialized = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).is_ok() };
        // The message text is read while COM is still initialized.
        let loaded: Loaded = decode(&path, width, height).map_err(|err| err.message());
        if initialized {
            unsafe { CoUninitialize() };
        }
        let loaded = Box::into_raw(Box::new(loaded));
        unsafe {
            let posted = PostMessageW(
                Some(hwnd),
                WM_APP_BACKGROUND_READY,
                WPARAM(generation),
                LPARAM(loaded as isize),
            );
            if posted.is_err() {
                // Never delivered, so nobody else will free it.
                drop(Box::from_raw(loaded));
            }
        }
    });
    if let Err(err) = spawned {
        debug_log!("could not start the background thread: {err}");
    }
}

/// Takes ownership of the `Loaded` sent with `WM_APP_BACKGROUND_READY`.
///
/// # Safety
/// `lparam` must come from that message and be taken exactly once.
pub unsafe fn take_loaded(lparam: LPARAM) -> Box<Loaded> {
    unsafe { Box::from_raw(lparam.0 as *mut Loaded) }
}

/// Asks for a picture with the standard Open dialog, starting at `current`.
///
/// The dialog runs its own message loop, so the caller must not hold the `App` borrow.
pub fn pick_file(owner: HWND, current: Option<&Path>) -> Option<PathBuf> {
    let filter: Vec<u16> = "Pictures (*.png;*.jpg;*.jpeg;*.bmp;*.gif)\0*.png;*.jpg;*.jpeg;*.bmp;*.gif\0\
                            All files (*.*)\0*.*\0\0"
        .encode_utf16()
        .collect();
    let mut file = vec![0u16; 4096];
    if let Some(current) = current {
        let wide: Vec<u16> = current.as_os_str().encode_wide().collect();
        // Leave room for the terminating zero; a longer path just starts empty.
        if wide.len() < file.len() {
            file[..wide.len()].copy_from_slice(&wide);
        }
    }

    let mut dialog = OPENFILENAMEW {
        lStructSize: size_of::<OPENFILENAMEW>() as u32,
        hwndOwner: owner,
        lpstrFilter: PCWSTR(filter.as_ptr()),
        lpstrFile: PWSTR(file.as_mut_ptr()),
        nMaxFile: file.len() as u32,
        lpstrTitle: w!("Background Image"),
        Flags: OFN_EXPLORER | OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR,
        ..Default::default()
    };
    if !unsafe { GetOpenFileNameW(&mut dialog) }.as_bool() {
        return None;
    }
    let len = file.iter().position(|&c| c == 0).unwrap_or(file.len());
    Some(PathBuf::from(OsString::from_wide(&file[..len])))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Writes an uncompressed 24-bit BMP filled with one color.
    fn write_bmp(path: &Path, width: u32, height: u32, bgr: [u8; 3]) {
        let row = (width * 3).div_ceil(4) * 4;
        let mut data = Vec::new();
        data.extend_from_slice(b"BM");
        data.extend_from_slice(&(54 + row * height).to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&54u32.to_le_bytes());
        data.extend_from_slice(&40u32.to_le_bytes());
        data.extend_from_slice(&(width as i32).to_le_bytes());
        data.extend_from_slice(&(height as i32).to_le_bytes());
        data.extend_from_slice(&1u16.to_le_bytes());
        data.extend_from_slice(&24u16.to_le_bytes());
        // No compression, image size, resolution and palette counts.
        data.extend_from_slice(&[0; 24]);
        for _ in 0..height {
            let start = data.len();
            for _ in 0..width {
                data.extend_from_slice(&bgr);
            }
            data.resize(start + row as usize, 0);
        }
        fs::write(path, data).unwrap();
    }

    fn test_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join(format!("brightnessctl-test-{}", std::process::id()))
            .join(name);
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn with_com<T>(f: impl FnOnce() -> T) -> T {
        let initialized = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).is_ok() };
        let result = f();
        if initialized {
            unsafe { CoUninitialize() };
        }
        result
    }

    #[test]
    fn test_cover_size_shrinks_but_never_enlarges() {
        assert_eq!(cover_size(4000, 3000, 400, 600), (800, 600));
        assert_eq!(cover_size(3000, 4000, 600, 400), (600, 800));
        assert_eq!(cover_size(100, 50, 400, 600), (100, 50));
        assert_eq!(cover_size(800, 600, 800, 600), (800, 600));
        assert_eq!(cover_size(800, 600, 0, 0), (1, 1));
        assert_eq!(cover_size(0, 10, 5, 5), (1, 10));
    }

    #[test]
    fn test_crop_keeps_the_area_aspect_ratio_centered() {
        let tall = crop(800.0, 600.0, 400.0, 600.0);
        assert_eq!((tall.left, tall.top, tall.right, tall.bottom), (200.0, 0.0, 600.0, 600.0));
        let wide = crop(800.0, 600.0, 800.0, 300.0);
        assert_eq!((wide.left, wide.top, wide.right, wide.bottom), (0.0, 150.0, 800.0, 450.0));
        let empty = crop(800.0, 600.0, 0.0, 300.0);
        assert_eq!((empty.left, empty.top, empty.right, empty.bottom), (0.0, 0.0, 800.0, 600.0));
    }

    #[test]
    fn test_covers() {
        let picture = |width, height, full_size| Picture {
            width,
            height,
            pixels: Vec::new(),
            blurred: Vec::new(),
            full_size,
        };
        assert!(picture(800, 600, false).covers(800, 600));
        assert!(picture(800, 600, false).covers(400, 300));
        assert!(!picture(800, 600, false).covers(801, 600));
        assert!(!picture(800, 600, false).covers(800, 601));
        assert!(picture(100, 50, true).covers(4000, 4000));
    }

    #[test]
    fn test_decode_shrinks_to_cover_and_premultiplies() {
        let path = test_dir("decode").join("solid.bmp");
        write_bmp(&path, 4, 2, [10, 120, 250]);
        with_com(|| {
            let small = decode(&path, 2, 1).unwrap();
            assert_eq!((small.width, small.height), (2, 1));
            assert!(!small.covers(3, 1));
            assert_eq!(small.pixels.len(), 2 * 4);
            for pixel in small.pixels.chunks(4) {
                for (got, want) in pixel.iter().zip([10u8, 120, 250, 255]) {
                    assert!(got.abs_diff(want) <= 1, "{pixel:?}");
                }
            }

            let full = decode(&path, 8, 8).unwrap();
            assert_eq!((full.width, full.height), (4, 2));
            assert!(full.covers(8, 8));
            assert_eq!(full.pixels[..4], [10, 120, 250, 255]);
        });
    }

    #[test]
    fn test_decode_reports_missing_files() {
        let path = test_dir("missing").join("nothing-here.png");
        with_com(|| assert!(decode(&path, 10, 10).is_err()));
    }

    #[test]
    fn test_blur_spreads_an_edge_without_touching_far_pixels() {
        // One row: sixteen black pixels, then sixteen white ones.
        let mut pixels = Vec::new();
        for x in 0..32 {
            let value = if x < 16 { 0 } else { 255 };
            pixels.extend_from_slice(&[value, value, value, 255]);
        }
        let out = blur(&pixels, 32, 1, 2);
        assert_eq!(out.len(), pixels.len());
        // The ends never see the other color, so they stay put.
        assert_eq!(out[0], 0);
        assert_eq!(out[124], 255);
        // The edge becomes a ramp.
        let luma = |x: usize| out[x * 4] as u32;
        assert!(luma(14) < luma(16), "{out:?}");
        assert!((0..31).all(|x| luma(x) <= luma(x + 1)), "{out:?}");
        // Alpha is opaque everywhere and must stay that way.
        assert!(out.chunks(4).all(|pixel| pixel[3] == 255), "{out:?}");
    }

    #[test]
    fn test_blur_keeps_a_flat_picture_flat() {
        let pixels = [40u8, 80, 120, 255].repeat(6 * 4);
        assert_eq!(blur(&pixels, 6, 4, 3), pixels);
    }

    #[test]
    fn test_blur_radius_scales_with_the_picture() {
        assert_eq!(blur_radius(800, 600), 25);
        assert_eq!(blur_radius(100, 100), 4);
        assert_eq!(blur_radius(4000, 4000), 32);
    }
}
