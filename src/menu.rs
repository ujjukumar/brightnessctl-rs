use windows::Win32::Foundation::{HWND, LPARAM, POINT, WPARAM};
use windows::Win32::Graphics::Gdi::ClientToScreen;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::*;

pub const IDM_EXIT: u16 = 101;
pub const IDM_REFRESH: u16 = 102;
pub const IDM_THEME_AUTO: u16 = 103;
pub const IDM_THEME_LIGHT: u16 = 104;
pub const IDM_THEME_DARK: u16 = 105;
pub const IDM_ABOUT: u16 = 106;
pub const IDM_LOCK_MODE: u16 = 107;
pub const IDM_BACKGROUND_PICK: u16 = 108;
pub const IDM_BACKGROUND_REMOVE: u16 = 109;
pub const IDM_THEME_GLASS: u16 = 110;

/// The menus the header button opens. The window has no menu bar, so there is no bar for
/// Windows to draw and tint; the handles are kept so the check marks can still be updated.
pub struct Menus {
    /// The top-level pop-up holding File, View, Theme and Help.
    pub root: HMENU,
    /// The theme sub-menu, whose check mark follows the current theme.
    pub theme: HMENU,
}

pub unsafe fn create_menus() -> Result<Menus> {
    let root = CreatePopupMenu()?;

    // File Menu
    let hfile = CreatePopupMenu()?;
    AppendMenuW(hfile, MF_STRING, IDM_EXIT as usize, w!("Exit"))?;
    AppendMenuW(root, MF_POPUP, hfile.0 as usize, w!("File"))?;

    // View Menu
    let hview = CreatePopupMenu()?;
    AppendMenuW(hview, MF_STRING, IDM_REFRESH as usize, w!("Refresh Monitors"))?;
    AppendMenuW(hview, MF_STRING, IDM_LOCK_MODE as usize, w!("Sync All Monitors (Lock)"))?;
    AppendMenuW(hview, MF_SEPARATOR, 0, None)?;
    AppendMenuW(hview, MF_STRING, IDM_BACKGROUND_PICK as usize, w!("Background Image..."))?;
    AppendMenuW(hview, MF_STRING, IDM_BACKGROUND_REMOVE as usize, w!("Remove Background"))?;
    AppendMenuW(root, MF_POPUP, hview.0 as usize, w!("View"))?;

    // Theme Menu
    let theme = CreatePopupMenu()?;
    AppendMenuW(theme, MF_STRING, IDM_THEME_AUTO as usize, w!("Auto"))?;
    AppendMenuW(theme, MF_STRING, IDM_THEME_LIGHT as usize, w!("Light"))?;
    AppendMenuW(theme, MF_STRING, IDM_THEME_DARK as usize, w!("Dark"))?;
    AppendMenuW(theme, MF_SEPARATOR, 0, None)?;
    AppendMenuW(theme, MF_STRING, IDM_THEME_GLASS as usize, w!("Glass"))?;
    AppendMenuW(root, MF_POPUP, theme.0 as usize, w!("Theme"))?;

    // Help Menu
    let hhelp = CreatePopupMenu()?;
    AppendMenuW(hhelp, MF_STRING, IDM_ABOUT as usize, w!("About"))?;
    AppendMenuW(root, MF_POPUP, hhelp.0 as usize, w!("Help"))?;

    Ok(Menus { root, theme })
}

/// Opens `hmenu` as a pop-up at the window-relative point (`x`, `y`). The chosen item is
/// delivered to the window as a `WM_COMMAND`, exactly like the old menu bar did.
pub unsafe fn show_popup(window: HWND, hmenu: HMENU, x: i32, y: i32) {
    let mut point = POINT { x, y };
    let _ = ClientToScreen(window, &mut point);
    // Without this the pop-up would not close when the user clicks elsewhere.
    let _ = SetForegroundWindow(window);
    let _ = TrackPopupMenu(
        hmenu,
        TPM_LEFTALIGN | TPM_TOPALIGN,
        point.x,
        point.y,
        None,
        window,
        None,
    );
    // The pop-up has to see a null message afterwards, or it can stay stuck open.
    let _ = PostMessageW(Some(window), WM_NULL, WPARAM(0), LPARAM(0));
}
