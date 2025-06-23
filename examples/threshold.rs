use std::{thread, time::Duration};

use prevent_alt_win_menu::event_handler::{Config, KeyboardAndMouse::VK__none_};

fn main() {
    #[cfg(feature = "log")]
    colog::init();

    // start to prevent alt/win menus...
    prevent_alt_win_menu::start(Config::default().set_on_released(|hold| {
        if hold.duration() > Duration::from_millis(300) {
            Some(VK__none_)
        } else {
            None
        }
    }))
    .unwrap();

    // your main code ...
    loop {
        thread::sleep(Duration::from_secs(10))
    }
}
