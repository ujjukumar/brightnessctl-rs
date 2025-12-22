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

pub struct ColorPalette {
    pub bg: D2D1_COLOR_F,
    pub fg: D2D1_COLOR_F,
    pub accent: D2D1_COLOR_F,
    pub subdued: D2D1_COLOR_F,
}

impl ColorPalette {
    pub fn dark() -> Self {
        Self {
            bg: D2D1_COLOR_F { r: 0.1, g: 0.1, b: 0.1, a: 1.0 },
            fg: D2D1_COLOR_F { r: 0.95, g: 0.95, b: 0.95, a: 1.0 },
            accent: D2D1_COLOR_F { r: 0.0, g: 0.47, b: 0.83, a: 1.0 },
            subdued: D2D1_COLOR_F { r: 0.6, g: 0.6, b: 0.6, a: 1.0 },
        }
    }

    pub fn light() -> Self {
        Self {
            bg: D2D1_COLOR_F { r: 0.95, g: 0.95, b: 0.95, a: 1.0 },
            fg: D2D1_COLOR_F { r: 0.1, g: 0.1, b: 0.1, a: 1.0 },
            accent: D2D1_COLOR_F { r: 0.0, g: 0.47, b: 0.83, a: 1.0 },
            subdued: D2D1_COLOR_F { r: 0.4, g: 0.4, b: 0.4, a: 1.0 },
        }
    }
}

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
    current_theme_is_dark: Option<bool>,
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
                current_theme_is_dark: None,
            })
        }
    }

    unsafe fn create_resources(&mut self, hwnd: HWND, is_dark: bool) -> Result<()> {
        if self.render_target.is_some() && self.current_theme_is_dark == Some(is_dark) {
            return Ok(());
        }

        if self.current_theme_is_dark != Some(is_dark) {
             self.brush_bg = None;
             self.brush_fg = None;
             self.brush_accent = None;
             self.brush_subdued = None;
        }

        let palette = if is_dark { ColorPalette::dark() } else { ColorPalette::light() };

        if self.render_target.is_none() {
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
            self.render_target = Some(hwnd_rt);
        }

        let rt: ID2D1RenderTarget = self.render_target.as_ref().unwrap().cast()?;

        if self.brush_bg.is_none() {
            self.brush_bg = Some(rt.CreateSolidColorBrush(&palette.bg, None)?);
        }
        if self.brush_fg.is_none() {
            self.brush_fg = Some(rt.CreateSolidColorBrush(&palette.fg, None)?);
        }
        if self.brush_accent.is_none() {
            self.brush_accent = Some(rt.CreateSolidColorBrush(&palette.accent, None)?);
        }
        if self.brush_subdued.is_none() {
            self.brush_subdued = Some(rt.CreateSolidColorBrush(&palette.subdued, None)?);
        }

        if self.text_format_title.is_none() {
            self.text_format_title = Some(self.dw_factory.CreateTextFormat(
                w!("Segoe UI"),
                None,
                DWRITE_FONT_WEIGHT_NORMAL,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                22.0,
                w!("en-us"),
            )?);
        }

        if self.text_format_body.is_none() {
            self.text_format_body = Some(self.dw_factory.CreateTextFormat(
                w!("Segoe UI"),
                None,
                DWRITE_FONT_WEIGHT_NORMAL,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                14.0,
                w!("en-us"),
            )?);
        }

        if self.text_format_label.is_none() {
            self.text_format_label = Some(self.dw_factory.CreateTextFormat(
                w!("Segoe UI"),
                None,
                DWRITE_FONT_WEIGHT_NORMAL,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                12.0,
                w!("en-us"),
            )?);
        }

        self.current_theme_is_dark = Some(is_dark);
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
        self.current_theme_is_dark = None;
    }

    pub fn render(&mut self, hwnd: HWND, state: &AppState) -> Result<()> {
        unsafe {
            let is_dark = state.settings.is_dark_mode();
            if self.create_resources(hwnd, is_dark).is_err() {
                self.discard_resources();
                self.create_resources(hwnd, is_dark)?;
            }

            let rt_hwnd = self.render_target.as_ref().unwrap();
            let rt: ID2D1RenderTarget = rt_hwnd.cast()?;

            let mut rect = RECT::default();
            GetClientRect(hwnd, &mut rect)?;
            let _ = rt_hwnd.Resize(&D2D_SIZE_U {
                width: (rect.right - rect.left) as u32,
                height: (rect.bottom - rect.top) as u32,
            });

            let palette = if is_dark { ColorPalette::dark() } else { ColorPalette::light() };

            rt.BeginDraw();
            rt.Clear(Some(&palette.bg));

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
                let display_name = if let Some(ident) = &monitor.identity {
                    format!("{} {}", ident.manufacturer_id, ident.serial)
                } else {
                    monitor.name.clone()
                };

                let ddc_status = if monitor.is_disabled {
                    "Disabled"
                } else if monitor.write_confirmed {
                    "DDC Active"
                } else {
                    "High-level"
                };

                let label_text = format!("{} â€¢ {}", display_name, ddc_status);
                let name: Vec<u16> = label_text.encode_utf16().collect();
                
                rt.DrawText(
                    &name,
                    self.text_format_label.as_ref().unwrap(),
                    &D2D_RECT_F {
                        left: margin_x,
                        top: y,
                        right: rect.right as f32 - margin_x - 60.0,
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
                        left: rect.right as f32 - margin_x - 60.0,
                        top: y,
                        right: rect.right as f32 - margin_x,
                        bottom: y + 16.0,
                    },
                    self.brush_subdued.as_ref().unwrap(),
                    D2D1_DRAW_TEXT_OPTIONS_NONE,
                    DWRITE_MEASURING_MODE_NATURAL,
                );

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
                
                let is_hovered = state.hover_monitor_idx == Some(i);
                let is_active = state.active_monitor_idx == Some(i);

                let track_color = if is_hovered || is_active {
                    D2D1_COLOR_F { r: palette.subdued.r + 0.1, g: palette.subdued.g + 0.1, b: palette.subdued.b + 0.1, a: 1.0 }
                } else {
                    palette.subdued
                };
                let track_brush = rt.CreateSolidColorBrush(&track_color, None)?;
                rt.FillRectangle(&track_rect, &track_brush);

                // Slider Fill
                let fill_width = slider_width * (brightness as f32 / 100.0);
                let fill_rect = D2D_RECT_F {
                    left: margin_x,
                    top: track_rect.top,
                    right: margin_x + fill_width,
                    bottom: track_rect.bottom,
                };
                
                let accent_color = if is_active {
                     D2D1_COLOR_F { r: palette.accent.r + 0.1, g: palette.accent.g + 0.1, b: palette.accent.b + 0.1, a: 1.0 }
                } else {
                     palette.accent
                };
                let accent_brush = rt.CreateSolidColorBrush(&accent_color, None)?;
                rt.FillRectangle(&fill_rect, &accent_brush);

                // Thumb
                let thumb_radius = if is_active { 10.0 } else if is_hovered { 9.0 } else { 8.0 };
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

            // Status Bar at the bottom
            let status_bar_height = 24.0;
            let status_rect = D2D_RECT_F {
                left: 0.0,
                top: rect.bottom as f32 - status_bar_height,
                right: rect.right as f32,
                bottom: rect.bottom as f32,
            };
            
            // Subtle separator line
            rt.DrawLine(
                Vector2 { X: 0.0, Y: status_rect.top },
                Vector2 { X: rect.right as f32, Y: status_rect.top },
                self.brush_subdued.as_ref().unwrap(),
                1.0,
                None,
            );

            let mut status_msg = state.status_message.clone();
            if state.lock_mode {
                if !status_msg.is_empty() {
                    status_msg.push_str(" | ");
                }
                status_msg.push_str("SYNC ACTIVE");
            }

            if !status_msg.is_empty() {
                let status_text: Vec<u16> = status_msg.encode_utf16().collect();
                rt.DrawText(
                    &status_text,
                    self.text_format_label.as_ref().unwrap(),
                    &D2D_RECT_F {
                        left: 8.0,
                        top: status_rect.top + 4.0,
                        right: rect.right as f32 - 8.0,
                        bottom: status_rect.bottom,
                    },
                    self.brush_subdued.as_ref().unwrap(),
                    D2D1_DRAW_TEXT_OPTIONS_NONE,
                    DWRITE_MEASURING_MODE_NATURAL,
                );
            }

            // Overlay Tooltip
            if let Some(idx) = state.hover_monitor_idx {
                if let Some(m) = state.monitors.get(idx) {
                    let mut tooltip_text = format!("Name: {}\nRange: {} - {}\nPath: {}", m.name, m.hardware_min, m.hardware_max, m.device_path);
                    if let Some(ident) = &m.identity {
                        tooltip_text.push_str(&format!("\nMFG: {}\nCode: {}\nSerial: {}", ident.manufacturer_id, ident.product_code, ident.serial));
                    }
                    let text_wide: Vec<u16> = tooltip_text.encode_utf16().collect();

                    let tt_width = 300.0;
                    let tt_height = 100.0;
                    let tt_x = margin_x;
                    let tt_y = rect.bottom as f32 - status_bar_height - tt_height - 8.0;

                    let tt_rect = D2D_RECT_F {
                        left: tt_x,
                        top: tt_y,
                        right: tt_x + tt_width,
                        bottom: tt_y + tt_height,
                    };

                    rt.FillRectangle(&tt_rect, self.brush_bg.as_ref().unwrap());
                    rt.DrawRectangle(&tt_rect, self.brush_subdued.as_ref().unwrap(), 1.0, None);

                    rt.DrawText(
                        &text_wide,
                        self.text_format_label.as_ref().unwrap(),
                        &D2D_RECT_F {
                            left: tt_x + 8.0,
                            top: tt_y + 8.0,
                            right: tt_x + tt_width - 8.0,
                            bottom: tt_y + tt_height - 8.0,
                        },
                        self.brush_fg.as_ref().unwrap(),
                        D2D1_DRAW_TEXT_OPTIONS_NONE,
                        DWRITE_MEASURING_MODE_NATURAL,
                    );
                }
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
