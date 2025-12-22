use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::*;

pub const IDM_EXIT: u16 = 101;
pub const IDM_REFRESH: u16 = 102;
pub const IDM_THEME_AUTO: u16 = 103;
pub const IDM_THEME_LIGHT: u16 = 104;
pub const IDM_THEME_DARK: u16 = 105;
pub const IDM_ABOUT: u16 = 106;
pub const IDM_LOCK_MODE: u16 = 107;

pub unsafe fn create_menu_bar() -> Result<HMENU> {
    let hmenu = CreateMenu()?;

    // File Menu
    let hfile = CreatePopupMenu()?;
    AppendMenuW(hfile, MF_STRING, IDM_EXIT as usize, w!("Exit"))?;
    AppendMenuW(hmenu, MF_POPUP, hfile.0 as usize, w!("&File"))?;

    // View Menu
    let hview = CreatePopupMenu()?;
    AppendMenuW(hview, MF_STRING, IDM_REFRESH as usize, w!("Refresh Monitors"))?;
    AppendMenuW(hview, MF_STRING, IDM_LOCK_MODE as usize, w!("Sync All Monitors (Lock)"))?;
    AppendMenuW(hmenu, MF_POPUP, hview.0 as usize, w!("&View"))?;

    // Theme Menu
    let htheme = CreatePopupMenu()?;
    AppendMenuW(htheme, MF_STRING, IDM_THEME_AUTO as usize, w!("Auto"))?;
    AppendMenuW(htheme, MF_STRING, IDM_THEME_LIGHT as usize, w!("Light"))?;
    AppendMenuW(htheme, MF_STRING, IDM_THEME_DARK as usize, w!("Dark"))?;
    AppendMenuW(hmenu, MF_POPUP, htheme.0 as usize, w!("&Theme"))?;

    // Help Menu
    let hhelp = CreatePopupMenu()?;
    AppendMenuW(hhelp, MF_STRING, IDM_ABOUT as usize, w!("About"))?;
    AppendMenuW(hmenu, MF_POPUP, hhelp.0 as usize, w!("&Help"))?;

    Ok(hmenu)
}
