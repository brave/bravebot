//! The text of a bug report, which `bravebot bug-report` writes and the desktop app saves (DIAG-8).
//!
//! One format for both front ends, so a maintainer reads a report the same way whichever wrote it.
//! The caller hands in what each section holds: the terminal what `doctor` printed, the desktop app
//! its settings report. This reads no log and names the newest one by the path it was given.

use std::path::Path;

/// The heading of the section naming the log, which the person attaches as a file of its own.
pub const LOG_HEADING: &str = "newest diagnostic log, to attach separately";

/// The report's text: the header lines, each section under its heading, and the log's path.
///
/// Fixed headings in English: a maintainer reads it, not the person who made it. A section's body
/// is written as it was given, so it ends in a newline where the caller's text did.
pub fn compose(header: &[&str], sections: &[(&str, &str)], log: Option<&Path>) -> String {
    let mut text = String::new();
    for line in header {
        text.push_str(line);
        text.push('\n');
    }
    for (heading, body) in sections {
        text.push_str(&format!("\n== {heading} ==\n"));
        text.push_str(body);
    }
    text.push_str(&format!("\n== {LOG_HEADING} ==\n"));
    match log {
        Some(path) => text.push_str(&format!("{}\n", path.display())),
        None => text.push_str("none\n"),
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The layout both front ends write: the header, a blank line, each section under its heading
    /// and the log's path last, with `none` where there is no log.
    #[test]
    fn a_report_is_its_header_its_sections_and_the_logs_path() {
        let text = compose(
            &["bravebot 1.2.3", "target linux-x86_64"],
            &[
                ("doctor", "out line\n"),
                ("doctor, standard error", "err\n"),
            ],
            Some(Path::new("/h/logs/a.log")),
        );
        assert_eq!(
            text,
            "bravebot 1.2.3\ntarget linux-x86_64\n\n== doctor ==\nout line\n\n\
             == doctor, standard error ==\nerr\n\n\
             == newest diagnostic log, to attach separately ==\n/h/logs/a.log\n"
        );
        let text = compose(&["one"], &[], None);
        assert_eq!(
            text,
            "one\n\n== newest diagnostic log, to attach separately ==\nnone\n"
        );
    }
}
