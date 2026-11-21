// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! Whether a desktop chat may be handed to a background session, and the program that runs it
//! there (BG-2, CMD-34).
//!
//! The refusals are the terminal's `/bg` refusals (`handed_to_the_background` in the TUI), made
//! from what the bridge holds about an open chat. The terminal's refusal for a command-line option
//! the background session would lose has no counterpart, because a window has no command line, and
//! a chat has no loop or goal, so its watches stand for all three. A question beside the work (an
//! enhancement or a handoff brief) is refused with a turn, since the chat would close under it.

use bravebot_agent::PermissionMode;
use bravebot_sandbox::SandboxMode;
use bravebot_session::jobs::Mode;
use std::path::{Path, PathBuf};

/// What [`refusal`] reads about an open chat.
#[derive(Debug, Clone, Copy)]
pub struct Chat {
    /// The mode the chat's next turn runs in.
    pub permission_mode: PermissionMode,
    /// The sandbox mode the chat's next turn runs its programs under.
    pub sandbox: SandboxMode,
    /// The sandbox mode the settings choose for the project, which the background session runs in
    /// (SANDBOX-22).
    pub settled: SandboxMode,
    /// Whether a turn, a compaction, or a question beside the work is running in the chat.
    pub busy: bool,
    /// Whether a watch is set in the chat.
    pub watching: bool,
    /// Whether the chat has a record on disk a background session can resume.
    pub recorded: bool,
    /// Whether this process leaves nothing behind (BG-1).
    pub incognito: bool,
}

/// The mode the chat goes on in once handed over, or the sentence saying why it may not be.
///
/// The checks run in the terminal's order, so a chat that fails several is told the same first
/// reason `/bg` would give.
pub fn refusal(chat: &Chat) -> Result<Mode, String> {
    if !cfg!(unix) {
        return Err("Background sessions are not available on this system.".to_string());
    }
    let Some(mode) = Mode::of(chat.permission_mode) else {
        return Err(match chat.permission_mode {
            PermissionMode::Auto => {
                "A chat in auto cannot continue in the background, because nobody would be there \
                 to see what auto decided. Choose another mode first."
            }
            _ => {
                "A chat that bypasses permissions cannot continue in the background. Choose another \
                 mode first."
            }
        }
        .to_string());
    };
    if chat.sandbox != chat.settled {
        return Err(format!(
            "This chat's sandbox mode is {}, and the background session would run in {}, which the \
             settings choose. Set the sandbox mode back to {} first.",
            chat.sandbox.name(),
            chat.settled.name(),
            chat.settled.name()
        ));
    }
    if chat.busy {
        return Err("Wait for what is running in this chat to finish first.".to_string());
    }
    if chat.watching {
        return Err(
            "Stop this chat's watches first, because the background session would not run them."
                .to_string(),
        );
    }
    if chat.incognito {
        return Err(
            "An incognito chat leaves no record, so it cannot continue in the background."
                .to_string(),
        );
    }
    if !chat.recorded {
        return Err(
            "There is nothing to continue in the background until a turn of this chat has ended."
                .to_string(),
        );
    }
    Ok(mode)
}

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

    fn idle() -> Chat {
        Chat {
            permission_mode: PermissionMode::Ask,
            sandbox: SandboxMode::WorkspaceWrite,
            settled: SandboxMode::WorkspaceWrite,
            busy: false,
            watching: false,
            recorded: true,
            incognito: false,
        }
    }

    /// BG-2, CMD-34: each of `/bg`'s refusals a chat can be in is made from what the bridge holds,
    /// and an idle recorded chat in a mode that carries over is handed over in that mode.
    #[test]
    fn each_refusal_of_bg_is_made_from_what_the_bridge_holds() {
        if !cfg!(unix) {
            assert!(refusal(&idle()).is_err());
            return;
        }
        assert_eq!(refusal(&idle()), Ok(Mode::Ask));
        for (mode, carried) in [
            (PermissionMode::AcceptEdits, Mode::AcceptEdits),
            (PermissionMode::Plan, Mode::Plan),
        ] {
            let chat = Chat {
                permission_mode: mode,
                ..idle()
            };
            assert_eq!(refusal(&chat), Ok(carried));
        }

        let refused = [
            Chat {
                permission_mode: PermissionMode::Auto,
                ..idle()
            },
            Chat {
                permission_mode: PermissionMode::Bypass,
                ..idle()
            },
            Chat {
                sandbox: SandboxMode::Strict,
                ..idle()
            },
            Chat {
                busy: true,
                ..idle()
            },
            Chat {
                watching: true,
                ..idle()
            },
            Chat {
                incognito: true,
                ..idle()
            },
            Chat {
                recorded: false,
                ..idle()
            },
        ];
        let sentences: Vec<String> = refused
            .iter()
            .map(|chat| refusal(chat).expect_err("refused"))
            .collect();
        for (index, sentence) in sentences.iter().enumerate() {
            assert!(!sentence.is_empty());
            assert!(
                !sentences[index + 1..].contains(sentence),
                "two refusals share a sentence: {sentence}"
            );
        }
        assert!(sentences[2].contains("strict") && sentences[2].contains("workspace-write"));
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
