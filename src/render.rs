use crate::state::AppState;
use windows::core::{Interface, Result, w};
use windows::Win32::Foundation::{D2DERR_RECREATE_TARGET, HWND, RECT};
use windows_numerics::Vector2;
use windows::Win32::Graphics::Direct2D::{
    ID2D1Bitmap,
    ID2D1Factory,
    ID2D1GradientStopCollection,
    ID2D1HwndRenderTarget,
    ID2D1LinearGradientBrush,
    ID2D1RenderTarget,
    ID2D1SolidColorBrush,
    D2D1CreateFactory,
    D2D1_BITMAP_INTERPOLATION_MODE_LINEAR,
    D2D1_BITMAP_PROPERTIES,
    D2D1_FACTORY_TYPE_SINGLE_THREADED,
    D2D1_HWND_RENDER_TARGET_PROPERTIES,
    D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES,
    D2D1_RENDER_TARGET_PROPERTIES,
    D2D1_PRESENT_OPTIONS_NONE,
};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D1_ALPHA_MODE_PREMULTIPLIED,
    D2D1_COLOR_F,
    D2D1_GRADIENT_STOP,
    D2D1_PIXEL_FORMAT,
    D2D_SIZE_U,
    D2D_RECT_F,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Direct2D::{
    D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
    D2D1_ELLIPSE,
    D2D1_EXTEND_MODE_CLAMP,
    D2D1_GAMMA_2_2,
    D2D1_ROUNDED_RECT,
};
use windows::Win32::Graphics::DirectWrite::{
    IDWriteFactory,
    IDWriteTextFormat,
    DWriteCreateFactory,
    DWRITE_FACTORY_TYPE_SHARED,
    DWRITE_FONT_WEIGHT_NORMAL,
    DWRITE_FONT_STYLE_NORMAL,
    DWRITE_FONT_STRETCH_NORMAL,
    DWRITE_MEASURING_MODE_NATURAL,
    DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
};
use windows::Win32::UI::WindowsAndMessaging::GetClientRect;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::Graphics::Gdi::InvalidateRect;
use windows::Win32::Graphics::Direct2D::D2D1_DRAW_TEXT_OPTIONS_NONE;
use crate::layout;
use crate::state::Backend;
use crate::background::{self, Picture};
use crate::settings::ThemeMode;
use crate::window::FrameColors;
use windows::Win32::Foundation::COLORREF;

/// Which look to draw. `Auto` picks `Dark` or `Light`, while `Glass` is a look of its own
/// with a gradient and translucent panels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ThemeStyle {
    Dark,
    Light,
    Glass,
}

impl ThemeStyle {
    fn of(state: &AppState) -> Self {
        if state.settings.theme == ThemeMode::Glass {
            ThemeStyle::Glass
        } else if state.is_dark() {
            ThemeStyle::Dark
        } else {
            ThemeStyle::Light
        }
    }

    /// Whether panels and the gradient are drawn. Their colors exist for every style, so
    /// the rest of the code doesn't need to special-case the theme.
    fn has_cards(self) -> bool {
        self == ThemeStyle::Glass
    }

    /// Colors for the title bar and border, when the theme wants them tinted. The flat
    /// themes return `None` and keep the native frame.
    fn frame_colors(self) -> Option<FrameColors> {
        if !self.has_cards() {
            return None;
        }
        let palette = ColorPalette::glass();
        Some(FrameColors {
            // Matches the top of the gradient, so the title bar looks like part of it.
            background: colorref(palette.bg),
            text: colorref(palette.fg),
            // A little darker than the background, so the window keeps a visible edge.
            border: colorref(D2D1_COLOR_F {
                r: palette.bg.r * 0.88,
                g: palette.bg.g * 0.86,
                b: palette.bg.b * 0.84,
                a: 1.0,
            }),
        })
    }
}

/// The window frame colors for the theme in `state`, or `None` to leave it to Windows.
pub fn frame_colors(state: &AppState) -> Option<FrameColors> {
    ThemeStyle::of(state).frame_colors()
}

/// Converts a drawing color to the `0x00BBGGRR` layout Win32 color APIs use.
fn colorref(color: D2D1_COLOR_F) -> COLORREF {
    let channel = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u32;
    COLORREF(channel(color.r) | (channel(color.g) << 8) | (channel(color.b) << 16))
}

pub struct ColorPalette {
    pub bg: D2D1_COLOR_F,
    /// Second stop of the background gradient; the same as `bg` for the flat themes.
    pub bg_alt: D2D1_COLOR_F,
    pub fg: D2D1_COLOR_F,
    pub accent: D2D1_COLOR_F,
    pub subdued: D2D1_COLOR_F,
    /// Fill and outline of the panels: the option buttons everywhere, and the row cards
    /// and status bar on the glass theme.
    pub card: D2D1_COLOR_F,
    pub border: D2D1_COLOR_F,
}

impl ColorPalette {
    pub fn dark() -> Self {
        let bg = D2D1_COLOR_F { r: 0.1, g: 0.1, b: 0.1, a: 1.0 };
        Self {
            bg,
            bg_alt: bg,
            fg: D2D1_COLOR_F { r: 0.95, g: 0.95, b: 0.95, a: 1.0 },
            accent: D2D1_COLOR_F { r: 0.0, g: 0.47, b: 0.83, a: 1.0 },
            subdued: D2D1_COLOR_F { r: 0.6, g: 0.6, b: 0.6, a: 1.0 },
            // The option buttons: lighter than the window, so their edges read.
            card: D2D1_COLOR_F { r: 0.17, g: 0.17, b: 0.17, a: 1.0 },
            border: D2D1_COLOR_F { r: 0.32, g: 0.32, b: 0.32, a: 1.0 },
        }
    }

    pub fn light() -> Self {
        let bg = D2D1_COLOR_F { r: 0.95, g: 0.95, b: 0.95, a: 1.0 };
        Self {
            bg,
            bg_alt: bg,
            fg: D2D1_COLOR_F { r: 0.1, g: 0.1, b: 0.1, a: 1.0 },
            accent: D2D1_COLOR_F { r: 0.0, g: 0.47, b: 0.83, a: 1.0 },
            subdued: D2D1_COLOR_F { r: 0.4, g: 0.4, b: 0.4, a: 1.0 },
            // The option buttons: white on the grey window, so their edges read.
            card: D2D1_COLOR_F { r: 1.0, g: 1.0, b: 1.0, a: 1.0 },
            border: D2D1_COLOR_F { r: 0.82, g: 0.82, b: 0.82, a: 1.0 },
        }
    }

    /// Cream fading into warm peach, with translucent white panels over it. The text
    /// colors stay dark enough to read on the lightest part of the gradient.
    pub fn glass() -> Self {
        Self {
            bg: D2D1_COLOR_F { r: 0.99, g: 0.97, b: 0.93, a: 1.0 },
            bg_alt: D2D1_COLOR_F { r: 0.98, g: 0.91, b: 0.85, a: 1.0 },
            fg: D2D1_COLOR_F { r: 0.20, g: 0.16, b: 0.13, a: 1.0 },
            accent: D2D1_COLOR_F { r: 0.80, g: 0.40, b: 0.12, a: 1.0 },
            subdued: D2D1_COLOR_F { r: 0.42, g: 0.36, b: 0.31, a: 1.0 },
            card: D2D1_COLOR_F { r: 1.0, g: 1.0, b: 1.0, a: 0.45 },
            border: D2D1_COLOR_F { r: 1.0, g: 1.0, b: 1.0, a: 0.75 },
        }
    }

    fn of(style: ThemeStyle) -> Self {
        match style {
            ThemeStyle::Dark => Self::dark(),
            ThemeStyle::Light => Self::light(),
            ThemeStyle::Glass => Self::glass(),
        }
    }
}

pub struct Renderer {
    d2d_factory: ID2D1Factory,
    dw_factory: IDWriteFactory,
    render_target: Option<ID2D1HwndRenderTarget>,
    text_format_title: Option<IDWriteTextFormat>,
    /// Option buttons: 14pt, centred between the top and bottom of the button.
    text_format_button: Option<IDWriteTextFormat>,
    text_format_label: Option<IDWriteTextFormat>,
    brush_bg: Option<ID2D1SolidColorBrush>,
    brush_fg: Option<ID2D1SolidColorBrush>,
    brush_accent: Option<ID2D1SolidColorBrush>,
    brush_subdued: Option<ID2D1SolidColorBrush>,
    /// Slider track and fill; recolored per row with `SetColor` for hover and active.
    brush_track: Option<ID2D1SolidColorBrush>,
    brush_fill: Option<ID2D1SolidColorBrush>,
    /// Panel fill and outline, used by the glass theme.
    brush_card: Option<ID2D1SolidColorBrush>,
    brush_border: Option<ID2D1SolidColorBrush>,
    /// Background gradient, built only for the glass theme.
    gradient_stops: Option<ID2D1GradientStopCollection>,
    brush_gradient: Option<ID2D1LinearGradientBrush>,
    current_style: Option<ThemeStyle>,
    /// DPI the render target currently draws at; 0 when there is no render target.
    dpi: u32,
    /// The decoded background picture, kept so the bitmaps can be rebuilt after a device loss.
    background: Option<Picture>,
    background_bitmap: Option<ID2D1Bitmap>,
    /// Blurred copy of the picture, shown through the glass panels.
    background_blur_bitmap: Option<ID2D1Bitmap>,
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
                text_format_button: None,
                text_format_label: None,
                brush_bg: None,
                brush_fg: None,
                brush_accent: None,
                brush_subdued: None,
                brush_track: None,
                brush_fill: None,
                brush_card: None,
                brush_border: None,
                gradient_stops: None,
                brush_gradient: None,
                current_style: None,
                dpi: 0,
                background: None,
                background_bitmap: None,
                background_blur_bitmap: None,
            })
        }
    }

    unsafe fn create_resources(&mut self, hwnd: HWND, style: ThemeStyle) -> Result<()> {
        if self.render_target.is_some() && self.current_style == Some(style) {
            return Ok(());
        }

        if self.current_style != Some(style) {
            self.drop_brushes();
        }

        let palette = ColorPalette::of(style);

        if self.render_target.is_none() {
            let mut rect = RECT::default();
            GetClientRect(hwnd, &mut rect)?;

            let size = D2D_SIZE_U {
                width: (rect.right - rect.left) as u32,
                height: (rect.bottom - rect.top) as u32,
            };

            // Draw at the window's DPI so all coordinates are DIPs.
            let dpi = GetDpiForWindow(hwnd);
            let hwnd_rt = self.d2d_factory.CreateHwndRenderTarget(
                &D2D1_RENDER_TARGET_PROPERTIES {
                    dpiX: dpi as f32,
                    dpiY: dpi as f32,
                    ..Default::default()
                },
                &D2D1_HWND_RENDER_TARGET_PROPERTIES {
                    hwnd,
                    pixelSize: size,
                    presentOptions: D2D1_PRESENT_OPTIONS_NONE,
                },
            )?;
            self.render_target = Some(hwnd_rt);
            self.dpi = dpi;
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
        if self.brush_track.is_none() {
            self.brush_track = Some(rt.CreateSolidColorBrush(&palette.subdued, None)?);
        }
        if self.brush_fill.is_none() {
            self.brush_fill = Some(rt.CreateSolidColorBrush(&palette.accent, None)?);
        }
        if self.brush_card.is_none() {
            self.brush_card = Some(rt.CreateSolidColorBrush(&palette.card, None)?);
        }
        if self.brush_border.is_none() {
            self.brush_border = Some(rt.CreateSolidColorBrush(&palette.border, None)?);
        }
        if style.has_cards() && self.gradient_stops.is_none() {
            let stops = [
                D2D1_GRADIENT_STOP { position: 0.0, color: palette.bg },
                D2D1_GRADIENT_STOP { position: 1.0, color: palette.bg_alt },
            ];
            self.gradient_stops = Some(rt.CreateGradientStopCollection(
                &stops,
                D2D1_GAMMA_2_2,
                D2D1_EXTEND_MODE_CLAMP,
            )?);
        }
        if self.brush_gradient.is_none()
            && let Some(stops) = &self.gradient_stops
        {
            // The points are set from the client size before every paint.
            let properties = D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES {
                startPoint: Vector2 { X: 0.0, Y: 0.0 },
                endPoint: Vector2 { X: 1.0, Y: 1.0 },
            };
            let brush = rt.CreateLinearGradientBrush(&properties, None, stops)?;
            self.brush_gradient = Some(brush);
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

        if self.text_format_button.is_none() {
            let format = self.dw_factory.CreateTextFormat(
                w!("Segoe UI"),
                None,
                DWRITE_FONT_WEIGHT_NORMAL,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                14.0,
                w!("en-us"),
            )?;
            format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
            self.text_format_button = Some(format);
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

        self.current_style = Some(style);
        Ok(())
    }

    fn drop_brushes(&mut self) {
        self.brush_bg = None;
        self.brush_fg = None;
        self.brush_accent = None;
        self.brush_subdued = None;
        self.brush_track = None;
        self.brush_fill = None;
        self.brush_card = None;
        self.brush_border = None;
        self.brush_gradient = None;
        self.gradient_stops = None;
    }

    /// Drops everything tied to the GPU device. Text formats are device-independent and kept.
    unsafe fn discard_device_resources(&mut self) {
        self.render_target = None;
        self.drop_brushes();
        self.background_bitmap = None;
        self.background_blur_bitmap = None;
        self.dpi = 0;
    }

    /// Replaces the background picture; `None` goes back to the plain theme color.
    pub fn set_background(&mut self, picture: Option<Picture>) {
        self.background = picture;
        self.background_bitmap = None;
        self.background_blur_bitmap = None;
    }

    /// Whether the background (if any) is big enough for a client of `width` x `height` pixels.
    pub fn background_covers(&self, width: u32, height: u32) -> bool {
        self.background.as_ref().is_none_or(|picture| picture.covers(width, height))
    }

    /// Resizes the drawing surface to the new client size in pixels (from `WM_SIZE`).
    pub fn resize(&mut self, width: u32, height: u32) {
        if let Some(rt) = &self.render_target {
            unsafe {
                let _ = rt.Resize(&D2D_SIZE_U { width, height });
            }
        }
    }

    pub fn render(&mut self, hwnd: HWND, state: &AppState) -> Result<()> {
        unsafe {
            let style = ThemeStyle::of(state);
            if self.create_resources(hwnd, style).is_err() {
                self.discard_device_resources();
                self.create_resources(hwnd, style)?;
            }

            let rt_hwnd = self.render_target.as_ref().unwrap();
            let rt: ID2D1RenderTarget = rt_hwnd.cast()?;

            // The window may have moved to a monitor with a different scale factor.
            let dpi = GetDpiForWindow(hwnd);
            if dpi != 0 && dpi != self.dpi {
                rt.SetDpi(dpi as f32, dpi as f32);
                self.dpi = dpi;
            }

            // GetSize reports DIPs, matching what input handling computes from pixels.
            let size = rt.GetSize();
            let layout = layout::compute(
                size.width,
                size.height,
                state.monitors.len(),
                state.settings.background_image.is_some(),
            );
            let window_rect = D2D_RECT_F {
                left: 0.0,
                top: 0.0,
                right: size.width,
                bottom: size.height,
            };
            let dim = 1.0 - state.settings.background_dim;

            let palette = ColorPalette::of(style);

            if self.background_bitmap.is_none()
                && let Some(picture) = &self.background
            {
                let properties = D2D1_BITMAP_PROPERTIES {
                    pixelFormat: D2D1_PIXEL_FORMAT {
                        format: DXGI_FORMAT_B8G8R8A8_UNORM,
                        alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                    },
                    // One bitmap DIP per pixel; `crop` works in these units.
                    dpiX: 96.0,
                    dpiY: 96.0,
                };
                let bitmap_size = D2D_SIZE_U { width: picture.width, height: picture.height };
                let pitch = picture.width * 4;
                self.background_bitmap = rt
                    .CreateBitmap(bitmap_size, Some(picture.pixels.as_ptr().cast()), pitch, &properties)
                    .ok();
                self.background_blur_bitmap = rt
                    .CreateBitmap(bitmap_size, Some(picture.blurred.as_ptr().cast()), pitch, &properties)
                    .ok();
            }

            rt.BeginDraw();
            if style.has_cards() {
                // Cream at the top left fading to warm peach at the bottom right.
                let gradient = self.brush_gradient.as_ref().unwrap();
                gradient.SetStartPoint(Vector2 { X: window_rect.left, Y: window_rect.top });
                gradient.SetEndPoint(Vector2 { X: window_rect.right, Y: window_rect.bottom });
                rt.FillRectangle(&window_rect, gradient);
            } else {
                rt.Clear(Some(&palette.bg));
            }

            // Drawn faded over the theme color, which works as the dim layer.
            if let Some(bitmap) = &self.background_bitmap {
                let picture = bitmap.GetSize();
                let source = background::crop(picture.width, picture.height, size.width, size.height);
                rt.DrawBitmap(
                    bitmap,
                    Some(&window_rect),
                    dim,
                    D2D1_BITMAP_INTERPOLATION_MODE_LINEAR,
                    Some(&source),
                );
            }

            draw_sun(&rt, &layout.title_icon, self.brush_accent.as_ref().unwrap());

            // App Title
            let title = w!("Brightness Control");
            rt.DrawText(
                title.as_wide(),
                self.text_format_title.as_ref().unwrap(),
                &layout.title,
                self.brush_fg.as_ref().unwrap(),
                D2D1_DRAW_TEXT_OPTIONS_NONE,
                DWRITE_MEASURING_MODE_NATURAL,
            );

            for (i, (monitor, row)) in state.monitors.iter().zip(&layout.rows).enumerate() {
                if style.has_cards() {
                    // The picture shows through the panel, blurred, like frosted glass.
                    self.draw_frosted(&rt, &row.card, &window_rect, dim);
                    let card = card_rect(&row.card);
                    rt.FillRoundedRectangle(&card, self.brush_card.as_ref().unwrap());
                    rt.DrawRoundedRectangle(&card, self.brush_border.as_ref().unwrap(), 1.0, None);
                }

                // Laptop panels are the ones driven through WMI.
                let icon_brush = self.brush_subdued.as_ref().unwrap();
                if monitor.backend == Backend::Wmi {
                    draw_laptop(&rt, &row.icon, icon_brush);
                } else {
                    draw_monitor(&rt, &row.icon, icon_brush);
                }

                // Monitor Name (Label)
                let display_name = if let Some(ident) = &monitor.identity {
                    format!("{} {}", ident.manufacturer_id, ident.serial)
                } else {
                    monitor.name.clone()
                };

                let label_text = format!("{} \u{2022} {}", display_name, monitor.status_label());
                let name: Vec<u16> = label_text.encode_utf16().collect();
                
                rt.DrawText(
                    &name,
                    self.text_format_label.as_ref().unwrap(),
                    &row.label,
                    self.brush_subdued.as_ref().unwrap(),
                    D2D1_DRAW_TEXT_OPTIONS_NONE,
                    DWRITE_MEASURING_MODE_NATURAL,
                );
                
                // Brightness percentage (right aligned)
                let brightness_normalized = monitor.ui_value;
                let is_editing = state.editing_monitor == Some(i);
                let (pct_text, brush) = if is_editing {
                    (format!("{}|", state.edit_buffer), self.brush_accent.as_ref().unwrap())
                } else {
                    (format!("{}%", (brightness_normalized * 100.0).round() as u32), self.brush_subdued.as_ref().unwrap())
                };

                let pct_text_wide: Vec<u16> = pct_text.encode_utf16().collect();

                if is_editing {
                    rt.DrawRectangle(&row.value, self.brush_accent.as_ref().unwrap(), 1.0, None);
                }

                // No value is known until the worker has probed the monitor.
                if monitor.ready {
                    rt.DrawText(
                        &pct_text_wide,
                        self.text_format_label.as_ref().unwrap(),
                        &row.value,
                        brush,
                        D2D1_DRAW_TEXT_OPTIONS_NONE,
                        DWRITE_MEASURING_MODE_NATURAL,
                    );
                }

                // Slider Track
                let is_hovered = state.hover_monitor_idx == Some(i);
                let is_active = state.active_monitor_idx == Some(i);

                let track_color = if is_hovered || is_active {
                    D2D1_COLOR_F { r: palette.subdued.r + 0.1, g: palette.subdued.g + 0.1, b: palette.subdued.b + 0.1, a: 1.0 }
                } else {
                    palette.subdued
                };
                let track_brush = self.brush_track.as_ref().unwrap();
                track_brush.SetColor(&track_color);
                rt.FillRectangle(&row.track, track_brush);

                if !monitor.ready {
                    continue;
                }

                // Slider Fill
                let thumb_x = layout::thumb_x(&row.track, brightness_normalized);
                let fill_rect = D2D_RECT_F { right: thumb_x, ..row.track };
                
                let accent_color = if is_active {
                     D2D1_COLOR_F { r: palette.accent.r + 0.1, g: palette.accent.g + 0.1, b: palette.accent.b + 0.1, a: 1.0 }
                } else {
                     palette.accent
                };
                let fill_brush = self.brush_fill.as_ref().unwrap();
                fill_brush.SetColor(&accent_color);
                rt.FillRectangle(&fill_rect, fill_brush);

                // Thumb
                let thumb_radius = if is_active { 10.0 } else if is_hovered { 9.0 } else { 8.0 };
                rt.FillEllipse(
                    &D2D1_ELLIPSE {
                        point: Vector2 {
                            X: thumb_x,
                            Y: (row.track.top + row.track.bottom) / 2.0,
                        },
                        radiusX: thumb_radius,
                        radiusY: thumb_radius,
                    },
                    self.brush_fg.as_ref().unwrap(),
                );
            }

            // The option controls under the sliders, over anything the rows reach.
            self.draw_options(&rt, &layout.options, state, &palette);

            // Status Bar at the bottom
            let status_rect = layout.status_bar;
            
            if style.has_cards() {
                // The status bar sits on a panel of its own instead of a plain line.
                self.draw_frosted(&rt, &layout.status_card, &window_rect, dim);
                let card = card_rect(&layout.status_card);
                rt.FillRoundedRectangle(&card, self.brush_card.as_ref().unwrap());
                rt.DrawRoundedRectangle(&card, self.brush_border.as_ref().unwrap(), 1.0, None);
            } else {
                // Subtle separator line
                rt.DrawLine(
                    Vector2 { X: status_rect.left, Y: status_rect.top },
                    Vector2 { X: status_rect.right, Y: status_rect.top },
                    self.brush_subdued.as_ref().unwrap(),
                    1.0,
                    None,
                );
            }

            let mut status_msg = state.status_message.clone();
            if state.lock_mode {
                if !status_msg.is_empty() {
                    status_msg.push_str(" | ");
                }
                status_msg.push_str("SYNC ACTIVE");
            }

            // Show hotkey failure warning if any
            let failed_count = state.hotkey_status.values().filter(|&&v| !v).count();
            if failed_count > 0 {
                if !status_msg.is_empty() {
                    status_msg.push_str(" | ");
                }
                status_msg.push_str(&format!("HOTKEY ERROR ({})", failed_count));
            }

            if !status_msg.is_empty() {
                let status_text: Vec<u16> = status_msg.encode_utf16().collect();
                rt.DrawText(
                    &status_text,
                    self.text_format_label.as_ref().unwrap(),
                    &D2D_RECT_F {
                        left: status_rect.left + 8.0,
                        top: status_rect.top + 4.0,
                        right: status_rect.right - 8.0,
                        bottom: status_rect.bottom,
                    },
                    self.brush_subdued.as_ref().unwrap(),
                    D2D1_DRAW_TEXT_OPTIONS_NONE,
                    DWRITE_MEASURING_MODE_NATURAL,
                );
            }

            // Overlay Tooltip
            if let Some(m) = state.hover_monitor_idx.and_then(|idx| state.monitors.get(idx)) {
                let mut tooltip_text = format!("Name: {}\nRange: {} - {}\nPath: {}", m.name, m.hardware_min, m.hardware_max, m.device_path);
                if let Some(ident) = &m.identity {
                    tooltip_text.push_str(&format!("\nMFG: {}\nCode: {}\nSerial: {}", ident.manufacturer_id, ident.product_code, ident.serial));
                }
                let text_wide: Vec<u16> = tooltip_text.encode_utf16().collect();

                let tt_rect = layout.tooltip;
                rt.FillRectangle(&tt_rect, self.brush_bg.as_ref().unwrap());
                rt.DrawRectangle(&tt_rect, self.brush_subdued.as_ref().unwrap(), 1.0, None);

                rt.DrawText(
                    &text_wide,
                    self.text_format_label.as_ref().unwrap(),
                    &D2D_RECT_F {
                        left: tt_rect.left + 8.0,
                        top: tt_rect.top + 8.0,
                        right: tt_rect.right - 8.0,
                        bottom: tt_rect.bottom - 8.0,
                    },
                    self.brush_fg.as_ref().unwrap(),
                    D2D1_DRAW_TEXT_OPTIONS_NONE,
                    DWRITE_MEASURING_MODE_NATURAL,
                );
            }

            // The device was lost (driver reset, GPU switch, some remote sessions):
            // rebuild the device resources and paint again.
            if let Err(err) = rt.EndDraw(None, None)
                && err.code() == D2DERR_RECREATE_TARGET
            {
                self.discard_device_resources();
                let _ = InvalidateRect(Some(hwnd), None, false);
            }
            Ok(())
        }
    }

    /// Draws the option controls under the sliders: the themes, sync and the picture.
    unsafe fn draw_options(
        &self,
        rt: &ID2D1RenderTarget,
        options: &layout::Options,
        state: &AppState,
        palette: &ColorPalette,
    ) {
        // The two line labels, in the spare space left of the buttons.
        for (label, area) in ["Theme", "Display"].iter().zip(&options.labels) {
            let text: Vec<u16> = label.encode_utf16().collect();
            rt.DrawText(
                &text,
                self.text_format_label.as_ref().unwrap(),
                area,
                self.brush_subdued.as_ref().unwrap(),
                D2D1_DRAW_TEXT_OPTIONS_NONE,
                DWRITE_MEASURING_MODE_NATURAL,
            );
        }

        // One button per theme, with the current one lit.
        for (i, area) in options.themes.iter().enumerate() {
            let Some(theme) = ThemeMode::ALL.get(i).copied() else {
                break;
            };
            self.draw_control(rt, area, theme.label(), state.settings.theme == theme, palette);
        }

        // Sync is a toggle, so it is lit the same way while it is on.
        self.draw_control(rt, &options.sync, "Sync", state.lock_mode, palette);
        self.draw_control(rt, &options.picture, "Picture\u{2026}", false, palette);
        if let Some(remove) = &options.remove {
            self.draw_control(rt, remove, "Remove", false, palette);
        }
    }

    /// Draws one option button: an outline, tinted while it is the current choice.
    unsafe fn draw_control(
        &self,
        rt: &ID2D1RenderTarget,
        area: &D2D_RECT_F,
        label: &str,
        active: bool,
        palette: &ColorPalette,
    ) {
        let shape = D2D1_ROUNDED_RECT {
            rect: *area,
            radiusX: layout::OPTION_RADIUS,
            radiusY: layout::OPTION_RADIUS,
        };
        // A tint of the accent keeps the label legible on every theme, where a solid
        // accent fill would leave white text on the pale end of the gradient.
        let (fill, border, text_brush) = if active {
            let fill = self.brush_fill.as_ref().unwrap();
            fill.SetColor(&D2D1_COLOR_F { a: 0.22, ..palette.accent });
            (fill, self.brush_accent.as_ref().unwrap(), self.brush_fg.as_ref().unwrap())
        } else {
            (
                self.brush_card.as_ref().unwrap(),
                self.brush_border.as_ref().unwrap(),
                self.brush_subdued.as_ref().unwrap(),
            )
        };
        rt.FillRoundedRectangle(&shape, fill);
        rt.DrawRoundedRectangle(&shape, border, 1.0, None);

        let text: Vec<u16> = label.encode_utf16().collect();
        rt.DrawText(
            &text,
            self.text_format_button.as_ref().unwrap(),
            area,
            text_brush,
            D2D1_DRAW_TEXT_OPTIONS_NONE,
            DWRITE_MEASURING_MODE_NATURAL,
        );
    }

    /// Draws the blurred copy of the picture, cropped exactly like the sharp one, clipped to
    /// `area`, so a panel shows whatever is behind it as frosted glass.
    unsafe fn draw_frosted(
        &self,
        rt: &ID2D1RenderTarget,
        area: &D2D_RECT_F,
        window: &D2D_RECT_F,
        opacity: f32,
    ) {
        let Some(bitmap) = &self.background_blur_bitmap else {
            return;
        };
        let picture = bitmap.GetSize();
        let source = background::crop(picture.width, picture.height, window.right, window.bottom);
        rt.PushAxisAlignedClip(area, D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
        rt.DrawBitmap(
            bitmap,
            Some(window),
            opacity,
            D2D1_BITMAP_INTERPOLATION_MODE_LINEAR,
            Some(&source),
        );
        rt.PopAxisAlignedClip();
    }
}

/// A panel outline: the rectangle with the theme's corner radius.
fn card_rect(area: &D2D_RECT_F) -> D2D1_ROUNDED_RECT {
    D2D1_ROUNDED_RECT {
        rect: *area,
        radiusX: layout::CARD_RADIUS,
        radiusY: layout::CARD_RADIUS,
    }
}

/// Maps a square grid of `units` x `units` icon units onto a layout rectangle,
/// so the icons are drawn as shapes and stay sharp at every size and DPI.
struct IconGrid {
    left: f32,
    top: f32,
    unit: f32,
}

impl IconGrid {
    fn new(area: &D2D_RECT_F, units: f32) -> Self {
        Self { left: area.left, top: area.top, unit: (area.right - area.left) / units }
    }

    fn len(&self, units: f32) -> f32 {
        units * self.unit
    }

    fn point(&self, x: f32, y: f32) -> Vector2 {
        Vector2 { X: self.left + self.len(x), Y: self.top + self.len(y) }
    }

    fn rounded(&self, left: f32, top: f32, right: f32, bottom: f32, radius: f32) -> D2D1_ROUNDED_RECT {
        let (a, b) = (self.point(left, top), self.point(right, bottom));
        D2D1_ROUNDED_RECT {
            rect: D2D_RECT_F { left: a.X, top: a.Y, right: b.X, bottom: b.Y },
            radiusX: self.len(radius),
            radiusY: self.len(radius),
        }
    }
}

/// Header icon: a sun with eight rays (24-unit grid).
unsafe fn draw_sun(rt: &ID2D1RenderTarget, area: &D2D_RECT_F, brush: &ID2D1SolidColorBrush) {
    let g = IconGrid::new(area, 24.0);
    let center = g.point(12.0, 12.0);
    rt.FillEllipse(&D2D1_ELLIPSE { point: center, radiusX: g.len(4.5), radiusY: g.len(4.5) }, brush);
    for i in 0..8 {
        let (sin, cos) = (i as f32 * std::f32::consts::FRAC_PI_4).sin_cos();
        let ray = |r: f32| g.point(12.0 + cos * r, 12.0 + sin * r);
        rt.DrawLine(ray(7.5), ray(10.5), brush, g.len(2.0), None);
    }
}

/// Row icon for an external display: a screen on a stand (16-unit grid).
unsafe fn draw_monitor(rt: &ID2D1RenderTarget, area: &D2D_RECT_F, brush: &ID2D1SolidColorBrush) {
    let g = IconGrid::new(area, 16.0);
    let stroke = g.len(1.5);
    rt.DrawRoundedRectangle(&g.rounded(1.0, 1.5, 15.0, 11.0, 1.5), brush, stroke, None);
    rt.DrawLine(g.point(8.0, 11.0), g.point(8.0, 14.0), brush, stroke, None);
    rt.DrawLine(g.point(4.0, 14.25), g.point(12.0, 14.25), brush, stroke, None);
}

/// Row icon for a laptop panel: a screen above a wider base (16-unit grid).
unsafe fn draw_laptop(rt: &ID2D1RenderTarget, area: &D2D_RECT_F, brush: &ID2D1SolidColorBrush) {
    let g = IconGrid::new(area, 16.0);
    rt.DrawRoundedRectangle(&g.rounded(2.5, 3.0, 13.5, 11.0, 1.0), brush, g.len(1.5), None);
    rt.DrawLine(g.point(0.75, 13.0), g.point(15.25, 13.0), brush, g.len(2.0), None);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_icon_grid_maps_units_onto_the_area() {
        let area = D2D_RECT_F { left: 10.0, top: 20.0, right: 42.0, bottom: 52.0 };
        let g = IconGrid::new(&area, 16.0);
        assert_eq!(g.len(1.0), 2.0);
        assert_eq!(g.point(0.0, 0.0), Vector2 { X: 10.0, Y: 20.0 });
        assert_eq!(g.point(16.0, 16.0), Vector2 { X: 42.0, Y: 52.0 });
        let r = g.rounded(1.0, 2.0, 15.0, 11.0, 1.5);
        assert_eq!((r.rect.left, r.rect.top, r.rect.right, r.rect.bottom), (12.0, 24.0, 40.0, 42.0));
        assert_eq!((r.radiusX, r.radiusY), (3.0, 3.0));
    }

    #[test]
    fn test_renderer_new() {
        let result = Renderer::new();
        assert!(result.is_ok(), "Renderer creation failed: {:?}", result.err());
    }
}
