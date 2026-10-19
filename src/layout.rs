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
/// How tall the two option lines under the sliders are.
const OPTION_HEIGHT: f32 = 28.0;
/// Gap between two controls on a line, and between the two lines.
const OPTION_GAP: f32 = 8.0;
/// Width of the "Theme" and "Display" labels at the left of each line.
const OPTION_LABEL_WIDTH: f32 = 52.0;
/// Space between the last slider and the first option line.
const OPTION_TOP_GAP: f32 = 16.0;
const OPTION_SYNC_WIDTH: f32 = 88.0;
const OPTION_PICTURE_WIDTH: f32 = 96.0;
const OPTION_REMOVE_WIDTH: f32 = 76.0;
/// Corner radius of an option control: half its height, so it is a pill.
pub const OPTION_RADIUS: f32 = OPTION_HEIGHT / 2.0;
/// How far the lit theme button sits inside the panel that holds the four of them.
pub const SEGMENT_INSET: f32 = 3.0;

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
    /// The theme buttons, the sync toggle and the picture controls.
    pub options: Options,
    pub rows: Vec<Row>,
    /// Card behind the status bar, drawn by themes that have one.
    pub status_card: Rect,
    pub status_bar: Rect,
    pub tooltip: Rect,
}

/// The two lines of controls under the sliders: the themes on the first, sync and the
/// picture on the second.
#[derive(Debug, Clone, PartialEq)]
pub struct Options {
    /// The "Theme" and "Display" labels, one per line.
    pub labels: [Rect; 2],
    /// The panel that holds the four theme buttons, side by side.
    pub theme_panel: Rect,
    /// One button per theme, in `ThemeMode` order: Auto, Light, Dark, Glass.
    pub themes: Vec<Rect>,
    /// Turns sync mode on and off.
    pub sync: Rect,
    /// Opens the picture picker.
    pub picture: Rect,
    /// Clears the picture; only there while one is set.
    pub remove: Option<Rect>,
}

impl Options {
    /// What the controls have under the point, if anything.
    pub fn hit_test(&self, x: f32, y: f32) -> Option<Hit> {
        for (i, theme) in self.themes.iter().enumerate() {
            if contains(theme, x, y) {
                return Some(Hit::Theme(i));
            }
        }
        if contains(&self.sync, x, y) {
            return Some(Hit::Sync);
        }
        if contains(&self.picture, x, y) {
            return Some(Hit::Picture);
        }
        if self.remove.as_ref().is_some_and(|area| contains(area, x, y)) {
            return Some(Hit::RemovePicture);
        }
        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hit {
    /// A theme button, in `ThemeMode` order: Auto, Light, Dark, Glass.
    Theme(usize),
    /// The sync toggle.
    Sync,
    /// The picture picker.
    Picture,
    /// Clears the background picture.
    RemovePicture,
    /// The percentage label of row `n`.
    Value(usize),
    /// The slider of row `n`.
    Track(usize),
}

impl Hit {
    /// The monitor row that was hit, or `None` for one of the window's own controls.
    pub fn index(self) -> Option<usize> {
        match self {
            Hit::Value(i) | Hit::Track(i) => Some(i),
            _ => None,
        }
    }
}

/// Lays out a client area of `width` x `height` DIPs holding `row_count` monitors.
pub fn compute(width: f32, height: f32, row_count: usize, has_picture: bool) -> Layout {
    let content_right = MARGIN + (width - 2.0 * MARGIN).max(1.0);
    let icon_top = MARGIN + (TITLE_HEIGHT - TITLE_ICON_SIZE) / 2.0;
    let title_icon = rect(MARGIN, icon_top, MARGIN + TITLE_ICON_SIZE, icon_top + TITLE_ICON_SIZE);
    let title = rect(title_icon.right + ICON_GAP, MARGIN, content_right, MARGIN + TITLE_HEIGHT);

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
    // The options sit in the space under the sliders, just above the status bar.
    let options = options(content_right, status_bar.top, has_picture);
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

    Layout { title_icon, title, options, rows, status_card, status_bar, tooltip }
}

/// Places the two option lines so they end just above `status_top`.
fn options(content_right: f32, status_top: f32, has_picture: bool) -> Options {
    let first_top = status_top - OPTION_TOP_GAP - OPTION_HEIGHT * 2.0 - OPTION_GAP;
    let second_top = first_top + OPTION_HEIGHT + OPTION_GAP;
    let line_left = (MARGIN + OPTION_LABEL_WIDTH + OPTION_GAP).min(content_right);
    let line_width = (content_right - line_left).max(1.0);

    // A label per line, centred against the buttons next to it.
    let label = |top: f32| {
        let text_top = top + (OPTION_HEIGHT - LABEL_HEIGHT) / 2.0;
        rect(MARGIN, text_top, line_left - OPTION_GAP, text_top + LABEL_HEIGHT)
    };

    // The four theme buttons share one panel, segment by segment with no gaps.
    let theme_panel = rect(line_left, first_top, content_right, first_top + OPTION_HEIGHT);
    let theme_width = (line_width / 4.0).max(1.0);
    let themes = (0..4)
        .map(|i| {
            let left = line_left + i as f32 * theme_width;
            rect(left, first_top, (left + theme_width).min(content_right), first_top + OPTION_HEIGHT)
        })
        .collect();

    // Sync and the picture controls fill the second line from the left.
    let mut left = line_left;
    let mut next = move |width: f32, top: f32| {
        let area = rect(left, top, (left + width).min(content_right), top + OPTION_HEIGHT);
        left += width + OPTION_GAP;
        area
    };
    let sync = next(OPTION_SYNC_WIDTH, second_top);
    let picture = next(OPTION_PICTURE_WIDTH, second_top);
    let remove = has_picture.then(|| next(OPTION_REMOVE_WIDTH, second_top));

    Options {
        labels: [label(first_top), label(second_top)],
        theme_panel,
        themes,
        sync,
        picture,
        remove,
    }
}

impl Layout {
    /// Returns what lies under the point (`x`, `y`), in DIPs.
    pub fn hit_test(&self, x: f32, y: f32) -> Option<Hit> {
        // The option controls are drawn over the sliders, so they take the pointer first.
        if let Some(hit) = self.options.hit_test(x, y) {
            return Some(hit);
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
    fn test_options_sit_above_the_status_bar() {
        // Both lines start after their label and end at the right margin.
        let layout = compute(384.0, 600.0, 2, true);
        let options = &layout.options;

        assert_eq!(options.labels.len(), 2);
        assert_eq!(options.themes.len(), 4);
        assert_eq!(options.themes[0].left, options.labels[0].right + OPTION_GAP);
        assert_eq!(options.themes[3].right, 384.0 - MARGIN);
        // One panel spans the four theme buttons, which sit side by side inside it.
        assert_eq!(options.theme_panel.left, options.themes[0].left);
        assert_eq!(options.theme_panel.right, options.themes[3].right);
        assert_eq!(
            options.theme_panel.right - options.theme_panel.left,
            options.themes.iter().map(|t| t.right - t.left).sum::<f32>()
        );
        assert_eq!(options.theme_panel.bottom - options.theme_panel.top, OPTION_HEIGHT);
        let remove = options.remove.expect("a picture is set");
        assert_eq!(remove.right, 384.0 - MARGIN);

        // The first line is above the second, which is above the status bar.
        assert!(options.labels[0].top < options.labels[1].top);
        assert!(options.themes[0].bottom < options.sync.top);
        assert!(options.sync.bottom < layout.status_bar.top);
        for pair in options.themes.windows(2) {
            assert_eq!(pair[0].right, pair[1].left);
        }
        assert!(options.sync.right < options.picture.left);
        assert!(options.picture.right < remove.left);

        // Every control is clickable; the labels and the gaps are not.
        let (x, y) = center(&options.themes[2]);
        assert_eq!(layout.hit_test(x, y), Some(Hit::Theme(2)));
        let (x, y) = center(&options.sync);
        assert_eq!(layout.hit_test(x, y), Some(Hit::Sync));
        let (x, y) = center(&options.picture);
        assert_eq!(layout.hit_test(x, y), Some(Hit::Picture));
        let (x, y) = center(&remove);
        assert_eq!(layout.hit_test(x, y), Some(Hit::RemovePicture));
        let (x, y) = center(&options.labels[0]);
        assert_eq!(layout.hit_test(x, y), None);
        assert_eq!(layout.hit_test(0.0, 0.0), None);
    }

    #[test]
    fn test_remove_button_only_while_a_picture_is_set() {
        assert!(compute(400.0, 600.0, 1, false).options.remove.is_none());
        assert!(compute(400.0, 600.0, 1, true).options.remove.is_some());
    }

    #[test]
    fn test_rows_do_not_overlap() {
        let layout = compute(400.0, 600.0, 4, false);
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
            for row in compute(width, 600.0, 2, false).rows {
                assert!(row.hit.left <= row.track.left);
                assert!(row.hit.right >= row.track.right);
                assert!(row.hit.top <= row.track.top);
                assert!(row.hit.bottom >= row.track.bottom);
            }
        }
    }

    #[test]
    fn test_value_is_flush_right() {
        let layout = compute(400.0, 600.0, 1, false);
        let row = layout.rows[0];
        assert_eq!(row.value.right, 400.0 - MARGIN);
        assert_eq!(row.track.right, 400.0 - MARGIN);
        assert_eq!(row.label.right, row.value.left);
        assert_eq!(row.icon.left, MARGIN);
        assert_eq!(row.label.left, row.icon.right + ICON_GAP);
    }

    #[test]
    fn test_narrow_window_keeps_track_width_positive() {
        let layout = compute(10.0, 50.0, 1, false);
        let track = layout.rows[0].track;
        assert!(track.right - track.left >= 1.0);
        assert!(layout.rows[0].label.right >= layout.rows[0].label.left);
        assert_eq!(value_at(&track, 0.0), 0.0);
    }

    #[test]
    fn test_status_bar_and_tooltip_anchor_to_bottom() {
        let layout = compute(400.0, 600.0, 1, false);
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
        let layout = compute(400.0, 600.0, 2, false);
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
        let track = compute(400.0, 600.0, 1, false).rows[0].track;
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
