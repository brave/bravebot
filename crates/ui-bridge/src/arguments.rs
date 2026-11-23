// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! The command line `bravebot-rpc` and `bravebot-acp` take: `--settings <path>` and
//! `--log-level <error|info|debug>`, each optional and at most once and in either order, or
//! `--version` (`-V`) alone.
//!
//! What to print is returned rather than printed, because only the transport binaries may write
//! to the standard streams or end the process (docs/specs/layering.md).

use std::path::PathBuf;

/// What a transport binary was started to do.
#[derive(Debug, PartialEq, Eq)]
pub enum Launch {
    /// Speak the protocol, with the settings file and the diagnostic log level the command line
    /// named, if it named them.
    Serve {
        settings: Option<PathBuf>,
        log_level: Option<bravebot_diag::Level>,
    },
    /// Print this line on standard output and end.
    Version(String),
    /// Print these lines on standard error and end with status 2.
    Refused(Vec<String>),
}

/// Read the arguments after the program's own name. `purpose` is what the usage line says the
/// program does.
pub fn launch(program: &str, purpose: &str, mut args: impl Iterator<Item = String>) -> Launch {
    let mut settings = None;
    let mut log_level = None;
    let mut first = true;
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--settings" if settings.is_none() => {
                let Some(path) = args.next() else {
                    return Launch::Refused(vec!["--settings requires a file path".into()]);
                };
                settings = Some(PathBuf::from(path));
            }
            // A missing or unknown word is refused rather than read as the default, as the
            // terminal's flag is: a typo would leave a person without the log they asked for, and
            // the process does not serve (DIAG-3).
            "--log-level" if log_level.is_none() => {
                let Some(word) = args.next() else {
                    return Launch::Refused(vec![
                        "--log-level requires error, info or debug".into(),
                    ]);
                };
                let Some(level) = bravebot_diag::Level::parse(&word) else {
                    return Launch::Refused(vec![format!("unknown log level: {word}")]);
                };
                log_level = Some(level);
            }
            "--version" | "-V" if first => return Launch::Version(version_line(program)),
            "--settings" | "--log-level" => {
                return Launch::Refused(vec![format!("{flag} given more than once")]);
            }
            other if !other.starts_with('-') => {
                return Launch::Refused(vec![format!("unexpected argument: {other}")]);
            }
            other => {
                return Launch::Refused(vec![
                    format!("unknown option: {other}"),
                    format!(
                        "usage: {program} [--settings <path>] [--log-level <error|info|debug>]"
                    ),
                    format!("           {purpose}"),
                    format!("       {program} --version"),
                ]);
            }
        }
        first = false;
    }
    Launch::Serve {
        settings,
        log_level,
    }
}

/// The line `--version` prints: the program, its version and the agent build it links.
pub fn version_line(program: &str) -> String {
    format!(
        "{program} {} (agent {})",
        env!("CARGO_PKG_VERSION"),
        crate::agent_build()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(args: &[&str]) -> Launch {
        launch(
            "bravebot-test",
            "speak a test protocol",
            args.iter().map(|arg| arg.to_string()),
        )
    }

    fn refused(lines: &[&str]) -> Launch {
        Launch::Refused(lines.iter().map(|line| line.to_string()).collect())
    }

    #[test]
    fn no_arguments_serve_with_no_named_settings_file() {
        assert_eq!(
            read(&[]),
            Launch::Serve {
                settings: None,
                log_level: None
            }
        );
    }

    #[test]
    fn a_named_settings_file_is_served_with() {
        assert_eq!(
            read(&["--settings", "/some/where/settings.json"]),
            Launch::Serve {
                settings: Some(PathBuf::from("/some/where/settings.json")),
                log_level: None
            }
        );
    }

    #[test]
    fn a_named_log_level_is_served_with() {
        assert_eq!(
            read(&["--log-level", "debug"]),
            Launch::Serve {
                settings: None,
                log_level: Some(bravebot_diag::Level::Debug)
            }
        );
        let both = Launch::Serve {
            settings: Some(PathBuf::from("/some/where/settings.json")),
            log_level: Some(bravebot_diag::Level::Info),
        };
        assert_eq!(
            read(&[
                "--settings",
                "/some/where/settings.json",
                "--log-level",
                "info"
            ]),
            both
        );
        assert_eq!(
            read(&[
                "--log-level",
                "info",
                "--settings",
                "/some/where/settings.json"
            ]),
            both
        );
    }

    /// A flag with no word is refused rather than served at the default level, which would leave
    /// the person without the log they asked for (DIAG-3).
    #[test]
    fn a_log_level_with_no_word_is_refused() {
        assert_eq!(
            read(&["--log-level"]),
            refused(&["--log-level requires error, info or debug"])
        );
        assert_eq!(
            read(&["--settings", "/some/where/settings.json", "--log-level"]),
            refused(&["--log-level requires error, info or debug"])
        );
    }

    #[test]
    fn an_unknown_log_level_is_refused() {
        assert_eq!(
            read(&["--log-level", "verbose"]),
            refused(&["unknown log level: verbose"])
        );
    }

    #[test]
    fn a_word_that_follows_no_flag_is_refused() {
        assert_eq!(
            read(&["--settings", "first.json", "second.json"]),
            refused(&["unexpected argument: second.json"])
        );
    }

    #[test]
    fn a_log_level_given_twice_is_refused() {
        assert_eq!(
            read(&["--log-level", "info", "--log-level", "debug"]),
            refused(&["--log-level given more than once"])
        );
    }

    /// A flag with no path is refused rather than served with no file, which would quietly drop
    /// the settings the person meant to start on.
    #[test]
    fn settings_with_no_path_is_refused() {
        assert_eq!(
            read(&["--settings"]),
            refused(&["--settings requires a file path"])
        );
    }

    #[test]
    fn a_settings_file_given_twice_is_refused() {
        assert_eq!(
            read(&["--settings", "/some/where/settings.json", "--settings"]),
            refused(&["--settings given more than once"])
        );
    }

    #[test]
    fn the_long_and_the_short_version_flag_both_print_the_version_line() {
        let line = format!(
            "bravebot-test {} (agent {})",
            env!("CARGO_PKG_VERSION"),
            bravebot_stamp::BUILD
        );
        assert_eq!(read(&["--version"]), Launch::Version(line.clone()));
        assert_eq!(read(&["-V"]), Launch::Version(line));
    }

    #[test]
    fn an_unknown_option_is_refused_with_the_usage_of_the_program_named() {
        assert_eq!(
            read(&["--sessions-root"]),
            refused(&[
                "unknown option: --sessions-root",
                "usage: bravebot-test [--settings <path>] [--log-level <error|info|debug>]",
                "           speak a test protocol",
                "       bravebot-test --version",
            ])
        );
    }
}
