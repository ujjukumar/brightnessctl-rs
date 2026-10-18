//! Window layout in device-independent pixels (DIPs, 96 per inch).
//!
//! Rendering and hit-testing both use `compute`, so what is drawn and what is
//! clickable can never drift apart. Mouse coordinates arrive in physical pixels
//! and must be converted with `px_to_dip` first.

use windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F as Rect;

pub const MARGIN: f32 = 24.0;
pub const STATUS_BAR_HEIGHT: f32 = 24.0;

const TITLE_HEIGHT: f32 = 32.0;
const HEADER_HEIGHT: f32 = 48.0;
const ROW_PITCH: f32 = 72.0;
const LABEL_HEIGHT: f32 = 16.0;
const VALUE_WIDTH: f32 = 60.0;
const TRACK_OFFSET: f32 = 32.0;
const TRACK_HEIGHT: f32 = 4.0;
const TRACK_HIT_PADDING: f32 = 12.0;
const TOOLTIP_WIDTH: f32 = 300.0;
const TOOLTIP_HEIGHT: f32 = 100.0;
const TOOLTIP_GAP: f32 = 8.0;

/// One monitor's row: name label, percentage value, slider track and its click area.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Row {
    pub label: Rect,
    pub value: Rect,
    pub track: Rect,
    pub hit: Rect,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Layout {
    pub title: Rect,
    pub rows: Vec<Row>,
    pub status_bar: Rect,
    pub tooltip: Rect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hit {
    /// The percentage label of row `n`.
    Value(usize),
    /// The slider of row `n`.
    Track(usize),
}

impl Hit {
    /// The row that was hit.
    pub fn index(self) -> usize {
        match self {
            Hit::Value(i) | Hit::Track(i) => i,
        }
    }
}

/// Lays out a client area of `width` x `height` DIPs holding `row_count` monitors.
pub fn compute(width: f32, height: f32, row_count: usize) -> Layout {
    let content_right = MARGIN + (width - 2.0 * MARGIN).max(1.0);
    let title = rect(MARGIN, MARGIN, content_right, MARGIN + TITLE_HEIGHT);

    let first_row_top = MARGIN + HEADER_HEIGHT;
    let rows = (0..row_count)
        .map(|i| {
            let top = first_row_top + i as f32 * ROW_PITCH;
            let value = rect(content_right - VALUE_WIDTH, top, content_right, top + LABEL_HEIGHT);
            let label = rect(MARGIN, top, value.left.max(MARGIN), top + LABEL_HEIGHT);
            let track_top = top + TRACK_OFFSET;
            let track = rect(MARGIN, track_top, content_right, track_top + TRACK_HEIGHT);
            // The click area spans the full window width for an easy target.
            let hit = rect(
                0.0,
                track.top - TRACK_HIT_PADDING,
                width.max(track.right),
                track.bottom + TRACK_HIT_PADDING,
            );
            Row { label, value, track, hit }
        })
        .collect();

    let status_bar = rect(0.0, height - STATUS_BAR_HEIGHT, width, height);
    let tooltip_bottom = status_bar.top - TOOLTIP_GAP;
    let tooltip = rect(
        MARGIN,
        tooltip_bottom - TOOLTIP_HEIGHT,
        MARGIN + TOOLTIP_WIDTH,
        tooltip_bottom,
    );

    Layout { title, rows, status_bar, tooltip }
}

impl Layout {
    /// Returns what lies under the point (`x`, `y`), in DIPs.
    pub fn hit_test(&self, x: f32, y: f32) -> Option<Hit> {
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
    fn test_rows_do_not_overlap() {
        let layout = compute(400.0, 600.0, 4);
        assert_eq!(layout.rows.len(), 4);
        for row in &layout.rows {
            assert!(row.label.bottom <= row.hit.top);
            assert!(row.value.bottom <= row.hit.top);
        }
        for pair in layout.rows.windows(2) {
            assert!(pair[0].hit.bottom <= pair[1].label.top);
        }
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
        assert_eq!(row.label.left, MARGIN);
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
