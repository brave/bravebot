//! The line that says crash reports are sent, through the same call the startup screen makes.
//!
//! In a process of its own because whether reports are on is the process's, set once by `begin`.

#![allow(
    clippy::unwrap_used,
    reason = "helpers in a test target are not covered by allow-unwrap-in-tests"
)]

use bravebot_backtrace::{Process, begin};

/// CRASH-9: a launch that is not reporting says nothing. The person who switched reports off, a
/// build with nowhere to send them and an incognito session are all this launch, and a line about
/// something that is not happening would be a false alarm.
///
/// Then the first launch that does report says it, and names both ways of turning it off, and no
/// launch after it says it again. The second launch is the same state directory with a new
/// process, which is what the file left behind stands for.
#[test]
fn the_first_launch_that_reports_says_so_once_and_a_launch_that_does_not_says_nothing() {
    assert_eq!(
        bravebot_tui::crash_notice::at_startup(),
        None,
        "a launch that is not reporting said something about it"
    );

    let state = tempfile::tempdir().unwrap();
    begin(
        Process::Cli,
        state.path().to_path_buf(),
        // Nothing is waiting to be sent, so nothing connects to this.
        "http://127.0.0.1:9/never".to_string(),
    );

    let line = bravebot_tui::crash_notice::at_startup().expect("the first launch said nothing");
    assert!(line.contains("crashReports"), "{line}");
    assert!(line.contains("BRAVEBOT_CRASH_REPORTS=0"), "{line}");
    assert!(line.contains("Brave"), "{line}");
    assert!(line.contains("prompts"), "{line}");

    assert_eq!(
        bravebot_tui::crash_notice::at_startup(),
        None,
        "the line was said twice in one launch"
    );
    assert!(
        state.path().join("crash-reports").join("notice").exists(),
        "nothing was left to stop the next launch saying it"
    );
    assert!(!bravebot_backtrace::notice_due());
}
