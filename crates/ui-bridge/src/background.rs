// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! Whether a desktop chat may be handed to a background session, and the program that runs it
//! there (BG-2, CMD-34).
//!
//! The decision is [`bravebot_session::jobs::refusal`], the one the terminal's `/bg` makes, read
//! from what the bridge holds about an open chat; only the sentences are the bridge's. The
//! terminal's refusal for a command-line option the background session would lose has no
//! counterpart, because a window has no command line, and a chat has no loop or goal, so its
//! watches stand for all three. A question beside the work (an enhancement or a handoff brief) is
//! refused as something running, since the chat would close under it.

use bravebot_session::jobs::{Handing, Mode, Refusal};
use std::path::{Path, PathBuf};

/// The mode the chat goes on in once handed over, or the sentence saying why it may not be.
///
/// A chat that fails several checks is told the same first reason `/bg` would give, because the
/// order is the shared decision's.
pub fn refusal(chat: &Handing) -> Result<Mode, String> {
    bravebot_session::jobs::refusal(chat).map_err(said)
}

/// The sentence the window is told for a refusal.
fn said(refusal: Refusal) -> String {
    match refusal {
        Refusal::Unsupported => "Background sessions are not available on this system.".to_string(),
        Refusal::Auto => "A chat in auto cannot continue in the background, because nobody would \
                          be there to see what auto decided. Choose another mode first."
            .to_string(),
        Refusal::Bypass => "A chat that bypasses permissions cannot continue in the background. \
                            Choose another mode first."
            .to_string(),
        Refusal::SandboxMoved { mode, settled } => format!(
            "This chat's sandbox mode is {}, and the background session would run in {}, which the \
             settings choose. Set the sandbox mode back to {} first.",
            mode.name(),
            settled.name(),
            settled.name()
        ),
        Refusal::Busy => "Wait for what is running in this chat to finish first.".to_string(),
        Refusal::KeepsRunning => {
            "Stop this chat's watches first, because the background session would not run them."
                .to_string()
        }
        Refusal::Incognito => {
            "An incognito chat leaves no record, so it cannot continue in the background."
                .to_string()
        }
        Refusal::NothingRecorded => NOTHING_RECORDED.to_string(),
    }
}

/// The sentence for a chat with no record a background session could resume.
pub const NOTHING_RECORDED: &str =
    "There is nothing to continue in the background until a turn of this chat has ended.";

/// The sentence for a chat whose hand-over is refused because the `bravebot` program is not
/// installed beside the bridge.
pub const NO_PROGRAM: &str = "Continue in Background needs the bravebot command-line program \
                              installed beside the desktop app, and it is not there.";

/// The `bravebot` program installed beside the bridge's own executable, or `None` where there is
/// none. The command line and the bridge are built from one workspace and ship together, so the
/// one beside it is the same version.
pub fn program() -> Option<PathBuf> {
    // The bridge's own location, read from the operating system; nothing a chat holds names it.
    // nosemgrep: rust.lang.security.current-exe.current-exe
    let me = std::env::current_exe().ok()?;
    program_beside(&me)
}

/// The `bravebot` program in the directory `executable` is in, where it is a file.
pub fn program_beside(executable: &Path) -> Option<PathBuf> {
    let program = executable
        .parent()?
        .join(format!("bravebot{}", std::env::consts::EXE_SUFFIX));
    program.is_file().then_some(program)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// BG-2, CMD-34: each reason `/bg` is refused for that a chat can be in has a sentence of its
    /// own, and the one for a moved sandbox names both modes. Which reason a chat is given is
    /// `bravebot_session::jobs::refusal`'s, and is tested there.
    #[test]
    fn each_refusal_of_bg_is_made_from_what_the_bridge_holds() {
        use bravebot_sandbox::SandboxMode;
        let refusals = [
            Refusal::Unsupported,
            Refusal::Auto,
            Refusal::Bypass,
            Refusal::SandboxMoved {
                mode: SandboxMode::Strict,
                settled: SandboxMode::WorkspaceWrite,
            },
            Refusal::Busy,
            Refusal::KeepsRunning,
            Refusal::Incognito,
            Refusal::NothingRecorded,
        ];
        let sentences: Vec<String> = refusals.into_iter().map(said).collect();
        for (index, sentence) in sentences.iter().enumerate() {
            assert!(sentence.ends_with('.'), "{sentence}");
            assert!(
                !sentences[index + 1..].contains(sentence),
                "two refusals share a sentence: {sentence}"
            );
        }
        assert!(sentences[3].contains("strict") && sentences[3].contains("workspace-write"));
    }

    #[test]
    fn the_program_is_the_one_beside_the_executable() {
        let directory =
            std::env::temp_dir().join(format!("bravebot-beside-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let me = directory.join("bravebot-rpc");
        assert_eq!(program_beside(&me), None);
        let program = directory.join(format!("bravebot{}", std::env::consts::EXE_SUFFIX));
        std::fs::write(&program, b"").unwrap();
        assert_eq!(program_beside(&me), Some(program));
        let _ = std::fs::remove_dir_all(&directory);
    }
}
