//! `bug-report.compose`: the text of a bug report for the desktop app to show and save (DIAG-9).
//!
//! The same format `bravebot bug-report` writes ([`bravebot_diag::report`]), with the settings
//! report the window's Run Diagnostics shows in place of what `doctor` prints, since the app bundle
//! has no command-line binary to run. The newest log is named and never read (DIAG-6), and nothing
//! here writes: the main process saves the text it is handed back, where the person chose.

use serde_json::{Value, json};
use std::path::Path;

/// The longest interface version taken into the report's header.
const VERSION_LIMIT: usize = 64;

/// The report, or `available: false` where this process may write nothing to the state
/// directory: a machine with no home, or a session that keeps nothing (DIAG-4).
///
/// `interface` is the desktop app's own version, which the main process states. It goes into the
/// header only where it is a version's shape, so no other text reaches the file through it.
pub fn compose(interface: Option<&str>, selected: Option<&Path>) -> Value {
    let Some(home) = bravebot_agent::home::writable() else {
        return json!({ "available": false, "text": "", "log": null });
    };
    // Named before anything below runs, as the terminal's report names it before `doctor`, so the
    // log is the one a failure left and not one this request began.
    let log = bravebot_diag::newest(&home.join(bravebot_diag::DIRECTORY));
    let settings = serde_json::to_string_pretty(&crate::settings::report(None, selected))
        .unwrap_or_default()
        + "\n";
    let text = text(
        interface.filter(|version| is_version(version)),
        crate::agent_build(),
        &format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
        &settings,
        log.as_deref(),
    );
    json!({
        "available": true,
        "text": text,
        "log": log.map(|path| path.display().to_string()),
    })
}

/// The report's text from its parts, so the layout is testable without a home.
fn text(
    interface: Option<&str>,
    build: &str,
    target: &str,
    settings: &str,
    log: Option<&Path>,
) -> String {
    let interface = format!("interface {}", interface.unwrap_or("unknown"));
    let agent = format!("agent {build}");
    let target = format!("target {target}");
    bravebot_diag::report::compose(
        &["bravebot desktop", &interface, &agent, &target],
        &[("settings report", settings)],
        log,
    )
}

/// Whether `text` is a version's shape: letters, digits, dots, dashes and pluses, and short.
fn is_version(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= VERSION_LIMIT
        && text
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'+'))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The header names the window and the agent behind it, then the settings report and the
    /// log's path follow in the terminal report's layout.
    #[test]
    fn the_desktop_report_names_both_versions_the_settings_report_and_the_log() {
        let report = text(
            Some("0.19.0"),
            "0.19.0 (abc1234)",
            "macos-aarch64",
            "{\n  \"configured\": true\n}\n",
            Some(Path::new("/h/.bravebot/logs/a.log")),
        );
        assert_eq!(
            report,
            "bravebot desktop\ninterface 0.19.0\nagent 0.19.0 (abc1234)\ntarget macos-aarch64\n\
             \n== settings report ==\n{\n  \"configured\": true\n}\n\
             \n== newest diagnostic log, to attach separately ==\n/h/.bravebot/logs/a.log\n"
        );
    }

    /// An interface version that is not a version's shape is not written, so a line break or a
    /// sentence cannot reach the file through it.
    #[test]
    fn an_interface_version_of_another_shape_is_not_written() {
        assert!(is_version("0.19.0"));
        assert!(is_version("1.0.0-beta.2+build.7"));
        for odd in [
            "",
            "0.19.0\nanything",
            "a b",
            &"9".repeat(VERSION_LIMIT + 1),
        ] {
            assert!(!is_version(odd), "{odd:?}");
        }
    }
}
