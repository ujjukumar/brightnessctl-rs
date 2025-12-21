use crate::state::AppState;
use windows::core::{Interface, Result, w};
use windows::Win32::Foundation::{HWND, RECT};
use windows_numerics::Vector2;
use windows::Win32::Graphics::Direct2D::{
    ID2D1Factory,
    ID2D1HwndRenderTarget,
    ID2D1RenderTarget,
    ID2D1SolidColorBrush,
    D2D1CreateFactory,
    D2D1_FACTORY_TYPE_SINGLE_THREADED,
    D2D1_HWND_RENDER_TARGET_PROPERTIES,
    D2D1_RENDER_TARGET_PROPERTIES,
    D2D1_PRESENT_OPTIONS_NONE,
};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D1_COLOR_F,
    D2D_SIZE_U,
    D2D_RECT_F,
};
use windows::Win32::Graphics::Direct2D::D2D1_ELLIPSE;
use windows::Win32::Graphics::DirectWrite::{
    IDWriteFactory,
    IDWriteTextFormat,
    DWriteCreateFactory,
    DWRITE_FACTORY_TYPE_SHARED,
    DWRITE_FONT_WEIGHT_NORMAL,
    DWRITE_FONT_STYLE_NORMAL,
    DWRITE_FONT_STRETCH_NORMAL,
    DWRITE_MEASURING_MODE_NATURAL,
};
use windows::Win32::UI::WindowsAndMessaging::GetClientRect;
use windows::Win32::Graphics::Direct2D::D2D1_DRAW_TEXT_OPTIONS_NONE;

pub struct Renderer {
    d2d_factory: ID2D1Factory,
    dw_factory: IDWriteFactory,
    render_target: Option<ID2D1HwndRenderTarget>,
    text_format: Option<IDWriteTextFormat>,
    brush_bg: Option<ID2D1SolidColorBrush>,
    brush_fg: Option<ID2D1SolidColorBrush>,
    brush_accent: Option<ID2D1SolidColorBrush>,
    brush_text: Option<ID2D1SolidColorBrush>,
}

impl Renderer {
    pub fn new() -> Result<Self> {
        unsafe {
            Ok(Self {
                d2d_factory: D2D1CreateFactory(
                    D2D1_FACTORY_TYPE_SINGLE_THREADED,
                    None,
                )?,
                dw_factory: DWriteCreateFactory(
                    DWRITE_FACTORY_TYPE_SHARED,
                )?,
                render_target: None,
                text_format: None,
                brush_bg: None,
                brush_fg: None,
                brush_accent: None,
                brush_text: None,
            })
        }
    }

    unsafe fn create_resources(&mut self, hwnd: HWND) -> Result<()> {
        if self.render_target.is_some() {
            return Ok(());
        }

        let mut rect = RECT::default();
        GetClientRect(hwnd, &mut rect)?;

        let size = D2D_SIZE_U {
            width: (rect.right - rect.left) as u32,
            height: (rect.bottom - rect.top) as u32,
        };

        let hwnd_rt = self.d2d_factory.CreateHwndRenderTarget(
            &D2D1_RENDER_TARGET_PROPERTIES::default(),
            &D2D1_HWND_RENDER_TARGET_PROPERTIES {
                hwnd,
                pixelSize: size,
                presentOptions: D2D1_PRESENT_OPTIONS_NONE,
            },
        )?;

        let rt: ID2D1RenderTarget = hwnd_rt.cast()?;

        let brush_bg = rt.CreateSolidColorBrush(
            &D2D1_COLOR_F { r: 0.10, g: 0.10, b: 0.10, a: 1.0 },
            None,
        )?;
        let brush_fg = rt.CreateSolidColorBrush(
            &D2D1_COLOR_F { r: 0.90, g: 0.90, b: 0.90, a: 1.0 },
            None,
        )?;
        let brush_accent = rt.CreateSolidColorBrush(
            &D2D1_COLOR_F { r: 0.00, g: 0.47, b: 0.83, a: 1.0 },
            None,
        )?;
        let brush_text = rt.CreateSolidColorBrush(
            &D2D1_COLOR_F { r: 0.80, g: 0.80, b: 0.80, a: 1.0 },
            None,
        )?;

        let text_format = self.dw_factory.CreateTextFormat(
            w!("Segoe UI"),
            None,
            DWRITE_FONT_WEIGHT_NORMAL,
            DWRITE_FONT_STYLE_NORMAL,
            DWRITE_FONT_STRETCH_NORMAL,
            14.0,
            w!("en-us"),
        )?;

        self.render_target = Some(hwnd_rt);
        self.brush_bg = Some(brush_bg);
        self.brush_fg = Some(brush_fg);
        self.brush_accent = Some(brush_accent);
        self.brush_text = Some(brush_text);
        self.text_format = Some(text_format);

        Ok(())
    }

    unsafe fn discard_resources(&mut self) {
        self.render_target = None;
        self.brush_bg = None;
        self.brush_fg = None;
        self.brush_accent = None;
        self.brush_text = None;
        self.text_format = None;
    }

    pub fn render(&mut self, hwnd: HWND, state: &AppState) -> Result<()> {
        unsafe {
            if self.create_resources(hwnd).is_err() {
                self.discard_resources();
                self.create_resources(hwnd)?;
            }

            let rt_hwnd = self.render_target.as_ref().unwrap();
            let rt: ID2D1RenderTarget = rt_hwnd.cast()?;

            let mut rect = RECT::default();
            GetClientRect(hwnd, &mut rect)?;
            rt_hwnd.Resize(&D2D_SIZE_U {
                width: (rect.right - rect.left) as u32,
                height: (rect.bottom - rect.top) as u32,
            })?;

            rt.BeginDraw();
            rt.Clear(Some(&D2D1_COLOR_F { r: 0.10, g: 0.10, b: 0.10, a: 1.0 }));

            let padding = 16.0;
            let slider_height = 4.0;
            let thumb_radius = 8.0;
            let mut y = 24.0;

            for (i, monitor) in state.monitors.iter().enumerate() {
                let name: Vec<u16> = monitor.name.encode_utf16().collect();

                rt.DrawText(
                    &name,
                    self.text_format.as_ref().unwrap(),
                    &D2D_RECT_F {
                        left: padding,
                        top: y,
                        right: rect.right as f32 - padding,
                        bottom: y + 20.0,
                    },
                    self.brush_text.as_ref().unwrap(),
                    D2D1_DRAW_TEXT_OPTIONS_NONE,
                    DWRITE_MEASURING_MODE_NATURAL,
                );

                y += 24.0;

                let brightness = state.brightness[i] as f32;
                let slider_left = padding;
                let slider_right = rect.right as f32 - padding - 48.0;

                let track = D2D_RECT_F {
                    left: slider_left,
                    top: y + 8.0,
                    right: slider_right,
                    bottom: y + 8.0 + slider_height,
                };

                rt.FillRectangle(&track, self.brush_text.as_ref().unwrap());

                let fill_width = (slider_right - slider_left) * (brightness / 100.0);
                rt.FillRectangle(
                    &D2D_RECT_F {
                        left: slider_left,
                        top: track.top,
                        right: slider_left + fill_width,
                        bottom: track.bottom,
                    },
                    self.brush_accent.as_ref().unwrap(),
                );

                rt.FillEllipse(
                    &D2D1_ELLIPSE {
                        point: Vector2 {
                            X: slider_left + fill_width,
                            Y: track.top + slider_height / 2.0,
                        },
                        radiusX: thumb_radius,
                        radiusY: thumb_radius,
                    },
                    self.brush_fg.as_ref().unwrap(),
                );

                let pct: Vec<u16> =
                    format!("{}%", brightness as u32).encode_utf16().collect();

                rt.DrawText(
                    &pct,
                    self.text_format.as_ref().unwrap(),
                    &D2D_RECT_F {
                        left: slider_right + 8.0,
                        top: y,
                        right: rect.right as f32 - padding,
                        bottom: y + 20.0,
                    },
                    self.brush_fg.as_ref().unwrap(),
                    D2D1_DRAW_TEXT_OPTIONS_NONE,
                    DWRITE_MEASURING_MODE_NATURAL,
                );

                y += 40.0;
            }

            rt.EndDraw(None, None)?;
            Ok(())
        }
    }
}
