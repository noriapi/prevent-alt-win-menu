use std::{thread, time::Duration};

fn main() {
    #[cfg(feature = "log")]
    colog::init();

    prevent_alt_win_menu::start(Default::default()).unwrap();

    // your main code ...
    loop {
        thread::sleep(Duration::from_secs(10))
    }
}
