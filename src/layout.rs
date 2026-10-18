//! Window layout in device-independent pixels (DIPs, 96 per inch).
//!
//! Rendering and hit-testing both use `compute`, so what is drawn and what is
//! clickable can never drift apart. Mouse coordinates arrive in physical pixels
//! and must be converted with `px_to_dip` first.

use windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F as Rect;

pub const MARGIN: f32 = 24.0;
pub const STATUS_BAR_HEIGHT: f32 = 24.0;

const TITLE_HEIGHT: f32 = 32.0;
const TITLE_ICON_SIZE: f32 = 24.0;
/// The header button that opens the menu; the window has no menu bar.
const MENU_BUTTON_SIZE: f32 = 28.0;
const HEADER_HEIGHT: f32 = 48.0;
const ROW_PITCH: f32 = 72.0;
const LABEL_HEIGHT: f32 = 16.0;
/// Row icons are as tall as the label next to them.
const ROW_ICON_SIZE: f32 = LABEL_HEIGHT;
const ICON_GAP: f32 = 8.0;
/// How far a card sticks out past the row it holds; the theme decides whether it's drawn.
const CARD_PAD: f32 = 10.0;
pub const CARD_RADIUS: f32 = 10.0;
const VALUE_WIDTH: f32 = 60.0;
const TRACK_OFFSET: f32 = 32.0;
const TRACK_HEIGHT: f32 = 4.0;
const TRACK_HIT_PADDING: f32 = 12.0;
const TOOLTIP_WIDTH: f32 = 300.0;
const TOOLTIP_HEIGHT: f32 = 100.0;
const TOOLTIP_GAP: f32 = 8.0;

/// One monitor's row: card, icon, name label, percentage value, slider track and its
/// click area.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Row {
    pub card: Rect,
    pub icon: Rect,
    pub label: Rect,
    pub value: Rect,
    pub track: Rect,
    pub hit: Rect,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Layout {
    pub title_icon: Rect,
    pub title: Rect,
    /// The menu button at the right end of the header.
    pub menu_button: Rect,
    pub rows: Vec<Row>,
    /// Card behind the status bar, drawn by themes that have one.
    pub status_card: Rect,
    pub status_bar: Rect,
    pub tooltip: Rect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hit {
    /// The menu button in the header.
    Menu,
    /// The percentage label of row `n`.
    Value(usize),
    /// The slider of row `n`.
    Track(usize),
}

impl Hit {
    /// The row that was hit, or `None` for the menu button.
    pub fn index(self) -> Option<usize> {
        match self {
            Hit::Menu => None,
            Hit::Value(i) | Hit::Track(i) => Some(i),
        }
    }
}

/// Lays out a client area of `width` x `height` DIPs holding `row_count` monitors.
pub fn compute(width: f32, height: f32, row_count: usize) -> Layout {
    let content_right = MARGIN + (width - 2.0 * MARGIN).max(1.0);
    let icon_top = MARGIN + (TITLE_HEIGHT - TITLE_ICON_SIZE) / 2.0;
    let title_icon = rect(MARGIN, icon_top, MARGIN + TITLE_ICON_SIZE, icon_top + TITLE_ICON_SIZE);
    let title_left = title_icon.right + ICON_GAP;
    let button_top = MARGIN + (TITLE_HEIGHT - MENU_BUTTON_SIZE) / 2.0;
    let menu_button = rect(
        content_right - MENU_BUTTON_SIZE,
        button_top,
        content_right,
        button_top + MENU_BUTTON_SIZE,
    );
    let title = rect(
        title_left,
        MARGIN,
        (menu_button.left - ICON_GAP).max(title_left),
        MARGIN + TITLE_HEIGHT,
    );

    let first_row_top = MARGIN + HEADER_HEIGHT;
    let rows = (0..row_count)
        .map(|i| {
            let top = first_row_top + i as f32 * ROW_PITCH;
            let icon = rect(MARGIN, top, MARGIN + ROW_ICON_SIZE, top + ROW_ICON_SIZE);
            let value = rect(content_right - VALUE_WIDTH, top, content_right, top + LABEL_HEIGHT);
            let label_left = icon.right + ICON_GAP;
            let label = rect(label_left, top, value.left.max(label_left), top + LABEL_HEIGHT);
            let track_top = top + TRACK_OFFSET;
            let track = rect(MARGIN, track_top, content_right, track_top + TRACK_HEIGHT);
            let card = rect(
                MARGIN - CARD_PAD,
                top - CARD_PAD,
                content_right + CARD_PAD,
                track.bottom + CARD_PAD,
            );
            // The click area spans the full window width for an easy target.
            let hit = rect(
                0.0,
                track.top - TRACK_HIT_PADDING,
                width.max(track.right),
                track.bottom + TRACK_HIT_PADDING,
            );
            Row { card, icon, label, value, track, hit }
        })
        .collect();

    let status_bar = rect(0.0, height - STATUS_BAR_HEIGHT, width, height);
    let status_card = rect(
        MARGIN - CARD_PAD,
        status_bar.top + 2.0,
        content_right + CARD_PAD,
        status_bar.bottom - 2.0,
    );
    let tooltip_bottom = status_bar.top - TOOLTIP_GAP;
    let tooltip = rect(
        MARGIN,
        tooltip_bottom - TOOLTIP_HEIGHT,
        MARGIN + TOOLTIP_WIDTH,
        tooltip_bottom,
    );

    Layout { title_icon, title, menu_button, rows, status_card, status_bar, tooltip }
}

impl Layout {
    /// Returns what lies under the point (`x`, `y`), in DIPs.
    pub fn hit_test(&self, x: f32, y: f32) -> Option<Hit> {
        if contains(&self.menu_button, x, y) {
            return Some(Hit::Menu);
        }
        for (i, row) in self.rows.iter().enumerate() {
            if contains(&row.value, x, y) {
                return Some(Hit::Value(i));
            }
            if contains(&row.hit, x, y) {
                return Some(Hit::Track(i));
            }
        }
        None
    }
}

/// Slider value (0.0-1.0) for a horizontal position on `track`, clamped to the ends.
pub fn value_at(track: &Rect, x: f32) -> f32 {
    let width = (track.right - track.left).max(1.0);
    ((x - track.left) / width).clamp(0.0, 1.0)
}

/// Horizontal position of the thumb for `value` (0.0-1.0) on `track`.
pub fn thumb_x(track: &Rect, value: f32) -> f32 {
    track.left + (track.right - track.left) * value.clamp(0.0, 1.0)
}

/// Converts physical pixels to DIPs for a window at `dpi`.
pub fn px_to_dip(px: i32, dpi: u32) -> f32 {
    if dpi == 0 {
        return px as f32;
    }
    px as f32 * 96.0 / dpi as f32
}

/// Converts DIPs to physical pixels for a window at `dpi`.
pub fn dip_to_px(dip: f32, dpi: u32) -> i32 {
    (dip * dpi as f32 / 96.0).round() as i32
}

fn rect(left: f32, top: f32, right: f32, bottom: f32) -> Rect {
    Rect { left, top, right, bottom }
}

fn contains(r: &Rect, x: f32, y: f32) -> bool {
    x >= r.left && x <= r.right && y >= r.top && y <= r.bottom
}

#[cfg(test)]
mod tests {
    use super::*;

    fn center(r: &Rect) -> (f32, f32) {
        ((r.left + r.right) / 2.0, (r.top + r.bottom) / 2.0)
    }

    #[test]
    fn test_menu_button_sits_at_the_top_right() {
        let layout = compute(400.0, 600.0, 1);
        assert_eq!(layout.menu_button.right, 400.0 - MARGIN);
        assert!(layout.menu_button.bottom <= layout.rows[0].icon.top);
        assert!(layout.title.right <= layout.menu_button.left);
        let (x, y) = center(&layout.menu_button);
        assert_eq!(layout.hit_test(x, y), Some(Hit::Menu));
    }

    #[test]
    fn test_rows_do_not_overlap() {
        let layout = compute(400.0, 600.0, 4);
        assert_eq!(layout.rows.len(), 4);
        for row in &layout.rows {
            assert!(row.icon.bottom <= row.hit.top);
            assert!(row.icon.right <= row.label.left);
            assert!(row.label.bottom <= row.hit.top);
            assert!(row.value.bottom <= row.hit.top);
            // The card wraps everything the row draws.
            assert!(row.card.left < row.icon.left && row.card.right > row.value.right);
            assert!(row.card.top <= row.label.top && row.card.bottom >= row.track.bottom);
        }
        for pair in layout.rows.windows(2) {
            assert!(pair[0].hit.bottom <= pair[1].label.top);
            assert!(pair[0].card.bottom <= pair[1].card.top);
        }
        assert!(layout.title_icon.right <= layout.title.left);
        assert!(layout.title_icon.bottom <= layout.rows[0].icon.top);
        assert!(layout.title.bottom <= layout.rows[0].label.top);
    }

    #[test]
    fn test_hit_area_contains_track() {
        for width in [10.0, 400.0, 1200.0] {
            for row in compute(width, 600.0, 2).rows {
                assert!(row.hit.left <= row.track.left);
                assert!(row.hit.right >= row.track.right);
                assert!(row.hit.top <= row.track.top);
                assert!(row.hit.bottom >= row.track.bottom);
            }
        }
    }

    #[test]
    fn test_value_is_flush_right() {
        let layout = compute(400.0, 600.0, 1);
        let row = layout.rows[0];
        assert_eq!(row.value.right, 400.0 - MARGIN);
        assert_eq!(row.track.right, 400.0 - MARGIN);
        assert_eq!(row.label.right, row.value.left);
        assert_eq!(row.icon.left, MARGIN);
        assert_eq!(row.label.left, row.icon.right + ICON_GAP);
    }

    #[test]
    fn test_narrow_window_keeps_track_width_positive() {
        let layout = compute(10.0, 50.0, 1);
        let track = layout.rows[0].track;
        assert!(track.right - track.left >= 1.0);
        assert!(layout.rows[0].label.right >= layout.rows[0].label.left);
        assert_eq!(value_at(&track, 0.0), 0.0);
    }

    #[test]
    fn test_status_bar_and_tooltip_anchor_to_bottom() {
        let layout = compute(400.0, 600.0, 1);
        assert_eq!(layout.status_bar.bottom, 600.0);
        assert_eq!(layout.status_bar.top, 600.0 - STATUS_BAR_HEIGHT);
        assert!(layout.tooltip.bottom < layout.status_bar.top);
        assert!(layout.status_card.top >= layout.status_bar.top);
        assert_eq!(layout.status_card.bottom, layout.status_bar.bottom - 2.0);
        assert_eq!(layout.status_card.left, MARGIN - CARD_PAD);
        assert_eq!(layout.status_card.right, 400.0 - MARGIN + CARD_PAD);
    }

    #[test]
    fn test_hit_test() {
        let layout = compute(400.0, 600.0, 2);
        let (vx, vy) = center(&layout.rows[1].value);
        assert_eq!(layout.hit_test(vx, vy), Some(Hit::Value(1)));

        let (tx, ty) = center(&layout.rows[0].track);
        assert_eq!(layout.hit_test(tx, ty), Some(Hit::Track(0)));
        // Left of the track still grabs the slider.
        assert_eq!(layout.hit_test(2.0, ty), Some(Hit::Track(0)));

        let (lx, ly) = center(&layout.rows[0].label);
        assert_eq!(layout.hit_test(lx, ly), None);
        let (ix, iy) = center(&layout.rows[0].icon);
        assert_eq!(layout.hit_test(ix, iy), None);
        let (hx, hy) = center(&layout.title);
        assert_eq!(layout.hit_test(hx, hy), None);
    }

    #[test]
    fn test_value_at_and_thumb_x_round_trip() {
        let track = compute(400.0, 600.0, 1).rows[0].track;
        assert_eq!(value_at(&track, track.left), 0.0);
        assert_eq!(value_at(&track, track.right), 1.0);
        assert_eq!(value_at(&track, track.left - 50.0), 0.0);
        assert_eq!(value_at(&track, track.right + 50.0), 1.0);
        for v in [0.0, 0.25, 0.5, 0.8, 1.0] {
            assert!((value_at(&track, thumb_x(&track, v)) - v).abs() < 1e-5);
        }
    }

    #[test]
    fn test_px_to_dip() {
        assert_eq!(px_to_dip(150, 144), 100.0);
        assert_eq!(px_to_dip(120, 120), 96.0);
        assert_eq!(px_to_dip(100, 96), 100.0);
        assert_eq!(px_to_dip(100, 0), 100.0);
    }
}
