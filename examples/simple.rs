fn main() {
    #[cfg(feature = "log")]
    colog::init();

    let handles = prevent_alt_win_menu::start(Default::default()).unwrap();
    handles.join().unwrap();
}
