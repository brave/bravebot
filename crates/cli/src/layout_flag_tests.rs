//! BOT-17.

use super::*;
use bravebot_tui::layout::Layout;

fn args(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|part| (*part).to_string()).collect()
}

#[test]
fn a_name_no_layout_answers_to_is_refused_naming_the_layouts() {
    for name in ["nowhere", "Bots", "bots ", " bots"] {
        let mut typed = args(&["--layout", name, "--agent", "x"]);
        let complaint = take_layout(&mut typed).expect_err(name);
        assert!(
            complaint.contains("default, bots") && complaint.contains(name),
            "{name:?}: {complaint}"
        );
        assert_eq!(typed, args(&["--layout", name, "--agent", "x"]));
    }
}

#[test]
fn an_empty_name_is_refused_as_a_missing_one_is() {
    let mut typed = args(&["--layout", ""]);
    let complaint = take_layout(&mut typed).expect_err("a refusal");
    assert!(
        complaint.contains("needs the name of a layout") && complaint.contains("default, bots"),
        "{complaint}"
    );
    assert!(!complaint.contains("no layout named"), "{complaint}");
}

#[test]
fn a_parsed_layout_opens_that_layout_and_default_is_a_fresh_session() {
    use bravebot_tui::app::Start;
    let mut bots = args(&["--layout", "bots"]);
    let bots = take_layout(&mut bots).expect("a layout");
    assert!(matches!(start_in(bots), Start::Layout(Layout::Bots)));

    let mut default = args(&["--layout", "default"]);
    let default = take_layout(&mut default).expect("a layout");
    assert!(matches!(start_in(default), Start::Fresh));

    assert!(matches!(start_in(None), Start::Fresh));
}

#[test]
fn a_missing_name_is_refused() {
    let mut typed = args(&["--layout"]);
    let complaint = take_layout(&mut typed).expect_err("a refusal");
    assert!(complaint.contains("default, bots"), "{complaint}");
}

#[test]
fn default_is_accepted_and_the_last_name_wins() {
    let mut typed = args(&["--layout", "default"]);
    assert_eq!(take_layout(&mut typed), Ok(Some(Layout::Default)));
    assert!(typed.is_empty());

    let mut twice = args(&["--layout", "bots", "--layout", "default"]);
    assert_eq!(take_layout(&mut twice), Ok(Some(Layout::Default)));

    let mut absent = args(&["--agent", "x"]);
    assert_eq!(take_layout(&mut absent), Ok(None));
    assert_eq!(absent, args(&["--agent", "x"]));
}
