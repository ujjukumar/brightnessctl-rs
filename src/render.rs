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
    text_format_title: Option<IDWriteTextFormat>,
    text_format_body: Option<IDWriteTextFormat>,
    text_format_label: Option<IDWriteTextFormat>,
    brush_bg: Option<ID2D1SolidColorBrush>,
    brush_fg: Option<ID2D1SolidColorBrush>,
    brush_accent: Option<ID2D1SolidColorBrush>,
    brush_subdued: Option<ID2D1SolidColorBrush>,
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
                text_format_title: None,
                text_format_body: None,
                text_format_label: None,
                brush_bg: None,
                brush_fg: None,
                brush_accent: None,
                brush_subdued: None,
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

        // Background: near-black neutral
        let brush_bg = rt.CreateSolidColorBrush(
            &D2D1_COLOR_F { r: 0.1, g: 0.1, b: 0.1, a: 1.0 },
            None,
        )?;
        // Foreground: off-white
        let brush_fg = rt.CreateSolidColorBrush(
            &D2D1_COLOR_F { r: 0.95, g: 0.95, b: 0.95, a: 1.0 },
            None,
        )?;
        // Accent: muted blue
        let brush_accent = rt.CreateSolidColorBrush(
            &D2D1_COLOR_F { r: 0.0, g: 0.47, b: 0.83, a: 1.0 },
            None,
        )?;
        // Subdued: gray for labels
        let brush_subdued = rt.CreateSolidColorBrush(
            &D2D1_COLOR_F { r: 0.6, g: 0.6, b: 0.6, a: 1.0 },
            None,
        )?;

        let text_format_title = self.dw_factory.CreateTextFormat(
            w!("Segoe UI"),
            None,
            DWRITE_FONT_WEIGHT_NORMAL,
            DWRITE_FONT_STYLE_NORMAL,
            DWRITE_FONT_STRETCH_NORMAL,
            22.0,
            w!("en-us"),
        )?;

        let text_format_body = self.dw_factory.CreateTextFormat(
            w!("Segoe UI"),
            None,
            DWRITE_FONT_WEIGHT_NORMAL,
            DWRITE_FONT_STYLE_NORMAL,
            DWRITE_FONT_STRETCH_NORMAL,
            14.0,
            w!("en-us"),
        )?;

        let text_format_label = self.dw_factory.CreateTextFormat(
            w!("Segoe UI"),
            None,
            DWRITE_FONT_WEIGHT_NORMAL,
            DWRITE_FONT_STYLE_NORMAL,
            DWRITE_FONT_STRETCH_NORMAL,
            12.0,
            w!("en-us"),
        )?;

        self.render_target = Some(hwnd_rt);
        self.brush_bg = Some(brush_bg);
        self.brush_fg = Some(brush_fg);
        self.brush_accent = Some(brush_accent);
        self.brush_subdued = Some(brush_subdued);
        self.text_format_title = Some(text_format_title);
        self.text_format_body = Some(text_format_body);
        self.text_format_label = Some(text_format_label);

        Ok(())
    }

    unsafe fn discard_resources(&mut self) {
        self.render_target = None;
        self.brush_bg = None;
        self.brush_fg = None;
        self.brush_accent = None;
        self.brush_subdued = None;
        self.text_format_title = None;
        self.text_format_body = None;
        self.text_format_label = None;
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
            rt.Clear(Some(&D2D1_COLOR_F { r: 0.1, g: 0.1, b: 0.1, a: 1.0 }));

            let margin_x = 24.0;
            let mut y = 24.0;

            // App Title
            let title = w!("Brightness Control");
            rt.DrawText(
                title.as_wide(),
                self.text_format_title.as_ref().unwrap(),
                &D2D_RECT_F {
                    left: margin_x,
                    top: y,
                    right: rect.right as f32 - margin_x,
                    bottom: y + 32.0,
                },
                self.brush_fg.as_ref().unwrap(),
                D2D1_DRAW_TEXT_OPTIONS_NONE,
                DWRITE_MEASURING_MODE_NATURAL,
            );
            y += 48.0;

            for (i, monitor) in state.monitors.iter().enumerate() {
                // Monitor Name (Label)
                let name: Vec<u16> = monitor.name.encode_utf16().collect();
                rt.DrawText(
                    &name,
                    self.text_format_label.as_ref().unwrap(),
                    &D2D_RECT_F {
                        left: margin_x,
                        top: y,
                        right: rect.right as f32 - margin_x,
                        bottom: y + 16.0,
                    },
                    self.brush_subdued.as_ref().unwrap(),
                    D2D1_DRAW_TEXT_OPTIONS_NONE,
                    DWRITE_MEASURING_MODE_NATURAL,
                );
                
                // Brightness percentage (right aligned)
                let brightness = state.brightness[i];
                let pct_text: Vec<u16> = format!("{}%", brightness).encode_utf16().collect();
                rt.DrawText(
                    &pct_text,
                    self.text_format_label.as_ref().unwrap(),
                    &D2D_RECT_F {
                        left: margin_x,
                        top: y,
                        right: rect.right as f32 - margin_x,
                        bottom: y + 16.0,
                    },
                    self.brush_subdued.as_ref().unwrap(),
                    D2D1_DRAW_TEXT_OPTIONS_NONE, // We need right alignment for text format
                    DWRITE_MEASURING_MODE_NATURAL,
                );
                // Note: Standard DrawText doesn't right align easily without changing format.
                // For now, let's keep it simple or use fixed spacing.

                y += 24.0;

                // Slider Track
                let slider_height = 4.0;
                let slider_width = rect.right as f32 - 2.0 * margin_x;
                let track_rect = D2D_RECT_F {
                    left: margin_x,
                    top: y + 8.0,
                    right: margin_x + slider_width,
                    bottom: y + 8.0 + slider_height,
                };
                rt.FillRectangle(&track_rect, self.brush_subdued.as_ref().unwrap());

                // Slider Fill
                let fill_width = slider_width * (brightness as f32 / 100.0);
                let fill_rect = D2D_RECT_F {
                    left: margin_x,
                    top: track_rect.top,
                    right: margin_x + fill_width,
                    bottom: track_rect.bottom,
                };
                rt.FillRectangle(&fill_rect, self.brush_accent.as_ref().unwrap());

                // Thumb
                let thumb_radius = 8.0;
                rt.FillEllipse(
                    &D2D1_ELLIPSE {
                        point: Vector2 {
                            X: margin_x + fill_width,
                            Y: track_rect.top + slider_height / 2.0,
                        },
                        radiusX: thumb_radius,
                        radiusY: thumb_radius,
                    },
                    self.brush_fg.as_ref().unwrap(),
                );

                y += 48.0; // Spacing between monitors
            }

            let _ = rt.EndDraw(None, None);
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_renderer_new() {
        let result = Renderer::new();
        assert!(result.is_ok(), "Renderer creation failed: {:?}", result.err());
    }
}