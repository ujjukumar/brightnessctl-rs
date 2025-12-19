use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};

pub fn init_com() {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }
}
