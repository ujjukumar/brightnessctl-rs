//! The menu behind the header button.
//!
//! There is no menu bar and no native pop-up: Windows cannot colour either one to match
//! the theme (a themed menu bar ignores `MIM_BACKGROUND`, and a pop-up is drawn by the
//! system), so the rows are painted by the renderer into the client area. This module
//! only says what the menu offers; `layout` places it and `render` draws it.

use crate::settings::ThemeMode;
use crate::state::AppState;

pub const IDM_EXIT: u16 = 101;
pub const IDM_THEME_AUTO: u16 = 103;
pub const IDM_THEME_LIGHT: u16 = 104;
pub const IDM_THEME_DARK: u16 = 105;
pub const IDM_LOCK_MODE: u16 = 107;
pub const IDM_BACKGROUND_PICK: u16 = 108;
pub const IDM_BACKGROUND_REMOVE: u16 = 109;
pub const IDM_THEME_GLASS: u16 = 110;

/// One line of the menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Row {
    /// A hairline between groups of commands.
    Separator,
    /// A clickable command.
    Command {
        /// The id it sends the window.
        id: u16,
        label: &'static str,
        /// Draws a tick: the current theme, and the sync mode.
        checked: bool,
    },
}

/// Whether the menu is open, and which row the pointer is over.
#[derive(Debug, Default)]
pub struct State {
    pub open: bool,
    /// An index into `rows`; only ever a command.
    pub hover: Option<usize>,
}

/// The rows to show for the current state, top to bottom. A row that would do nothing is
/// left out, so the menu stays as short as the app allows.
pub fn rows(state: &AppState) -> Vec<Row> {
    let theme = state.settings.theme;
    let command = |id: u16, label: &'static str| Row::Command { id, label, checked: false };
    let choice = |id: u16, label: &'static str, mode: ThemeMode| Row::Command {
        id,
        label,
        checked: theme == mode,
    };

    let mut rows = vec![
        choice(IDM_THEME_AUTO, "Auto", ThemeMode::Auto),
        choice(IDM_THEME_LIGHT, "Light", ThemeMode::Light),
        choice(IDM_THEME_DARK, "Dark", ThemeMode::Dark),
        choice(IDM_THEME_GLASS, "Glass", ThemeMode::Glass),
        Row::Separator,
        command(IDM_BACKGROUND_PICK, "Background Image..."),
    ];
    // There is nothing to remove while no picture is set.
    if state.settings.background_image.is_some() {
        rows.push(command(IDM_BACKGROUND_REMOVE, "Remove Background"));
    }
    rows.push(Row::Separator);
    rows.push(Row::Command {
        id: IDM_LOCK_MODE,
        label: "Sync All Monitors (Lock)",
        checked: state.lock_mode,
    });
    rows.push(Row::Separator);
    rows.push(command(IDM_EXIT, "Exit"));
    rows
}
