// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! The command line `bravebot-rpc` and `bravebot-acp` take: nothing, `--settings <path>`, or
//! `--version`.
//!
//! What to print is returned rather than printed, because only the transport binaries may write
//! to the standard streams or end the process (docs/specs/layering.md).

use std::path::PathBuf;

/// What a transport binary was started to do.
#[derive(Debug, PartialEq, Eq)]
pub enum Launch {
    /// Speak the protocol, with the settings file the command line named, if it named one.
    Serve { settings: Option<PathBuf> },
    /// Print this line on standard output and end.
    Version(String),
    /// Print these lines on standard error and end with status 2.
    Refused(Vec<String>),
}

/// Read the arguments after the program's own name. `purpose` is what the usage line says the
/// program does.
pub fn launch(program: &str, purpose: &str, mut args: impl Iterator<Item = String>) -> Launch {
    let Some(flag) = args.next() else {
        return Launch::Serve { settings: None };
    };
    match flag.as_str() {
        "--settings" => {
            let Some(path) = args.next() else {
                return Launch::Refused(vec!["--settings requires a file path".into()]);
            };
            if args.next().is_some() {
                return Launch::Refused(vec!["unexpected arguments after settings file".into()]);
            }
            Launch::Serve {
                settings: Some(PathBuf::from(path)),
            }
        }
        "--version" | "-V" => Launch::Version(version_line(program)),
        other => Launch::Refused(vec![
            format!("unknown option: {other}"),
            format!("usage: {program}            {purpose}"),
            format!("       {program} --version"),
        ]),
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
        assert_eq!(read(&[]), Launch::Serve { settings: None });
    }

    #[test]
    fn a_named_settings_file_is_served_with() {
        assert_eq!(
            read(&["--settings", "/some/where/settings.json"]),
            Launch::Serve {
                settings: Some(PathBuf::from("/some/where/settings.json"))
            }
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
    fn anything_after_the_settings_file_is_refused() {
        assert_eq!(
            read(&["--settings", "/some/where/settings.json", "--settings"]),
            refused(&["unexpected arguments after settings file"])
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
                "usage: bravebot-test            speak a test protocol",
                "       bravebot-test --version",
            ])
        );
    }
}
