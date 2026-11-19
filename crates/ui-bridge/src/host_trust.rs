// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! The question the local host puts on its own terminal about each workspace, before it starts any
//! client (HOST-1).
//!
//! It is the startup question about a working directory as a session in lines puts it (CLI-14), with
//! the same lines, kept in the same record on the same terms (TRUST-23), so a yes given here is
//! honoured by every session opened in that workspace later, the host's included. A workspace that
//! already has a kept yes, or where no yes can be kept, is not asked about, and the host says which.
//! The person typed the command line and types the answer, so the answer is theirs.

use bravebot_agent::confirm::shown_path;
use bravebot_agent::trusted;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

/// What the person did at the questions.
#[derive(Debug, PartialEq, Eq)]
pub enum Asked {
    /// Every workspace was answered or needed no answer, so the clients may start.
    Answered,
    /// The person typed `quit`, or the input ended or could not be read, so the host stops
    /// without starting a client.
    Stop,
}

/// Put the question about each of `workspaces`, in order, reading the answers from `input` and
/// writing the question and what came of it to `output`.
///
/// `home` and `profile` are where the record is, as [`trusted::record_to_keep`] takes them. Each
/// workspace is read under the name its links resolve to, which is the name a session opened there
/// reads the record under. A yes is kept naming no session, since none has started.
pub fn ask_about_each<R: BufRead, W: Write>(
    workspaces: &[PathBuf],
    home: Option<&Path>,
    profile: Option<&Path>,
    input: &mut R,
    output: &mut W,
) -> Asked {
    for workspace in workspaces {
        if ask_about(workspace, home, profile, input, output) == Asked::Stop {
            return Asked::Stop;
        }
    }
    Asked::Answered
}

fn ask_about<R: BufRead, W: Write>(
    workspace: &Path,
    home: Option<&Path>,
    profile: Option<&Path>,
    input: &mut R,
    output: &mut W,
) -> Asked {
    let shown = shown_path(workspace);
    let mut say = |line: &str| show(output, &format!("{line}\n"));
    let Ok(directory) = workspace.canonicalize() else {
        say(&format!(
            "bravebot-host: {shown} cannot be resolved, so each session there asks whether to trust it"
        ));
        return Asked::Answered;
    };

    if let Some(trusted::Honoured { store, kept }) = trusted::honoured(home, profile, &directory) {
        let when = bravebot_session::sessions::how_long_ago(kept.at);
        let path = shown_path(store.path());
        say(&match kept.root == directory {
            true => format!(
                "bravebot-host: sessions in {shown} start trusted, because you trusted it {when}. To be asked again, run /forget-trust in bravebot there, or delete the lines naming it from {path}"
            ),
            false => format!(
                "bravebot-host: sessions in {shown} start trusted, because you trusted {}, which holds it, {when}. To be asked again, run /forget-trust in bravebot there, or delete the lines naming {} from {path}",
                shown_path(&kept.root),
                shown_path(&kept.root),
            ),
        });
        return Asked::Answered;
    }

    let Some(keeping) = trusted::record_to_keep(home, profile, &directory) else {
        say(&format!(
            "bravebot-host: a yes about {shown} cannot be kept, so it is not asked about here and each session there asks whether to trust it"
        ));
        return Asked::Answered;
    };

    let lines = bravebot_approval::trust_directory_lines(
        &directory,
        Some(bravebot_approval::Keeping {
            record: keeping.0.path(),
            take_back: bravebot_i18n::t!(local_host_trust_remember_where),
        }),
    );
    for line in &lines {
        say(line);
    }
    show(
        output,
        &bravebot_approval::invitation(bravebot_i18n::t!(trust_directory_title)),
    );
    let mut typed = String::new();
    let read = input.read_line(&mut typed);
    // The person's own Enter ended the line on a terminal. Where the input is not one, nothing
    // did, and what follows would be written after the question.
    show(output, "\n");
    let mut say = |line: &str| show(output, &format!("{line}\n"));
    let yes = match read {
        Ok(0) => {
            say(&format!(
                "bravebot-host: its input ended at the question about {shown}; nothing was kept, and the host is stopping"
            ));
            return Asked::Stop;
        }
        Ok(_) if typed.trim() == "quit" => {
            say(&format!(
                "bravebot-host: nothing was kept about {shown}, and the host is stopping"
            ));
            return Asked::Stop;
        }
        Ok(_) => bravebot_approval::says_yes(&typed),
        // A line that is not text is not yes.
        Err(error) if error.kind() == std::io::ErrorKind::InvalidData => false,
        Err(error) => {
            say(&format!(
                "bravebot-host: cannot read its input ({error}); nothing was kept about {shown}, and the host is stopping"
            ));
            return Asked::Stop;
        }
    };

    let path = shown_path(keeping.0.path());
    say(&match trusted::keep_a_yes(Some(&keeping), yes, "", true) {
        Some(true) => format!(
            "bravebot-host: sessions in {shown} start trusted, now and later. To take it back, run /forget-trust in bravebot there, or delete the lines naming it from {path}"
        ),
        Some(false) => format!(
            "bravebot-host: the answer about {shown} could not be written to {path}, so each session there asks whether to trust it"
        ),
        None => format!(
            "bravebot-host: {shown} is not trusted and nothing was kept, so each session there asks whether to trust it"
        ),
    });
    Asked::Answered
}

/// Write `text` to the person's terminal and flush it.
fn show<W: Write>(output: &mut W, text: &str) {
    #[expect(
        clippy::let_underscore_must_use,
        reason = "where the terminal cannot be written to, there is nowhere else to tell the person, and the answer read decides what happens"
    )]
    let _ = output
        .write_all(text.as_bytes())
        .and_then(|()| output.flush());
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A home to keep answers in, a profile apart from it, and a workspace in neither.
    struct Scratch {
        home: tempfile::TempDir,
        profile: tempfile::TempDir,
        workspace: tempfile::TempDir,
    }

    impl Scratch {
        fn new() -> Self {
            let made = || tempfile::tempdir().expect("make a scratch directory");
            Scratch {
                home: made(),
                profile: made(),
                workspace: made(),
            }
        }

        /// Ask about the workspace with `input` as what the person typed, and return what the host
        /// did and wrote.
        fn ask(&self, input: &mut impl BufRead) -> (Asked, String) {
            let mut output = Vec::new();
            let asked = ask_about_each(
                &[self.workspace.path().to_path_buf()],
                Some(self.home.path()),
                Some(self.profile.path()),
                input,
                &mut output,
            );
            (
                asked,
                String::from_utf8(output).expect("the host writes text"),
            )
        }

        fn kept(&self) -> bool {
            let directory = self
                .workspace
                .path()
                .canonicalize()
                .expect("resolve the workspace");
            trusted::honoured(
                Some(self.home.path()),
                Some(self.profile.path()),
                &directory,
            )
            .is_some()
        }
    }

    /// Input whose every read fails.
    struct Unreadable;

    impl std::io::Read for Unreadable {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("the terminal went away"))
        }
    }

    impl BufRead for Unreadable {
        fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
            Err(std::io::Error::other("the terminal went away"))
        }

        fn consume(&mut self, _: usize) {}
    }

    #[test]
    fn a_yes_is_kept_and_the_clients_may_start() {
        let scratch = Scratch::new();
        let (asked, _) = scratch.ask(&mut &b"y\n"[..]);
        assert_eq!(asked, Asked::Answered);
        assert!(scratch.kept(), "a yes is kept about the workspace");
    }

    #[test]
    fn a_line_that_is_not_text_is_a_no_and_the_clients_may_start() {
        let scratch = Scratch::new();
        let (asked, written) = scratch.ask(&mut &b"\xff\n"[..]);
        assert_eq!(asked, Asked::Answered);
        assert!(
            !scratch.kept(),
            "nothing is kept for a line that is not text"
        );
        assert!(
            written.contains("is not trusted and nothing was kept"),
            "the host says nothing was kept: {written}"
        );
    }

    #[test]
    fn input_that_cannot_be_read_stops_the_host_and_keeps_nothing() {
        let scratch = Scratch::new();
        let (asked, written) = scratch.ask(&mut Unreadable);
        assert_eq!(asked, Asked::Stop);
        assert!(
            !scratch.kept(),
            "nothing is kept when the input cannot be read"
        );
        assert!(
            written.contains("cannot read its input (the terminal went away)"),
            "the host says why it stops: {written}"
        );
    }
}
