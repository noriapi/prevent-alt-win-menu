use std::time::Duration;

use prevent_alt_win_menu::event_handler::Config;
use windows::Win32::UI::Input::KeyboardAndMouse::VK__none_;

fn main() {
    #[cfg(feature = "log")]
    colog::init();

    let handles = prevent_alt_win_menu::start(Config::default().set_on_released(|hold| {
        if hold.duration() > Duration::from_millis(300) {
            Some(VK__none_)
        } else {
            None
        }
    }))
    .unwrap();
    handles.join().unwrap();
}
