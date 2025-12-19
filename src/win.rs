use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};

pub fn init_com() {
    unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok();
    }
}
