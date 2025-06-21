use std::{thread, time::Duration};

use uiautomation::{
    UIAutomation, UIMatcher,
    errors::{ERR_NOTFOUND, ERR_TIMEOUT},
};

#[test]
fn show_start_menu() {
    let automation = UIAutomation::new().unwrap();
    let matcher = start_menu_matcher(&automation);

    assert_no_match(&matcher);

    send_win(&automation);

    assert_match(&matcher);

    send_win(&automation);

    assert_no_match(&matcher);
}

#[test]
fn prevent_to_show_start_menu() {
    prevent_alt_win_menu::start(Default::default()).unwrap();

    let automation = UIAutomation::new().unwrap();
    let matcher = start_menu_matcher(&automation);

    assert_no_match(&matcher);

    send_win(&automation);

    assert_no_match(&matcher);
}

fn send_win(automation: &UIAutomation) {
    let root = automation.get_root_element().unwrap();
    root.send_keys("{Win}", 0).unwrap();
}

fn assert_no_match(matcher: &UIMatcher) {
    thread::sleep(Duration::from_millis(500));

    assert!(matches!(
        matcher.find_first().err().map(|e| e.code()),
        Some(ERR_NOTFOUND | ERR_TIMEOUT)
    ));
}

fn assert_match(matcher: &UIMatcher) {
    assert!(matcher.find_first().is_ok());
}

fn start_menu_matcher(automation: &UIAutomation) -> UIMatcher {
    let root = automation.get_root_element().unwrap();

    automation
        .create_matcher()
        .from_ref(&root)
        .timeout(500)
        .depth(2)
        .name("Start")
        .classname("Windows.UI.Core.CoreWindow")
}
