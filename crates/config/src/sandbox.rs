//! Which sandbox mode a session runs its programs under, and who said so (SANDBOX-22).
//!
//! Three places can name one: the command line, the person's own settings file and the managed
//! file. The checkout's two files may only ask for `strict`. A mode is a bundle of what a program
//! `run` starts may reach, so a line in a checkout that could pick `standard` or `off` would let
//! whoever wrote the checkout loosen what the person who cloned it is held to. Asking for `strict`
//! takes nothing from that person, on the footing of the `permissions` keys that only refuse
//! ([`crate::Narrowing`]).
//!
//! The managed file is a floor and not a default: nothing the command line or a settings file says
//! may be looser than it, and a setting that is refuses the session before it starts, naming the
//! file, rather than being read as the pin.

use bravebot_sandbox::SandboxMode;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// The block that holds the key.
pub(crate) const SANDBOX_BLOCK: &str = "sandbox";

/// The key inside it that names the mode.
pub const MODE_KEY: &str = "mode";

/// What one file said about the mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Stated {
    /// The file named no mode.
    Absent,
    /// The file named one.
    Mode(SandboxMode),
    /// The file named something that is not a mode: a word the three do not include, or a value
    /// that is not a word. Read as absence and reported, since a mistyped `strict` leaves the
    /// session less confined than whoever wrote it believes.
    Unreadable,
}

/// What a settings root says about the mode.
pub(crate) fn stated(root: &serde_json::Map<String, serde_json::Value>) -> Stated {
    match root.get(SANDBOX_BLOCK) {
        None => Stated::Absent,
        Some(serde_json::Value::Object(block)) => match block.get(MODE_KEY) {
            None => Stated::Absent,
            Some(serde_json::Value::String(word)) => {
                SandboxMode::parse(word).map_or(Stated::Unreadable, Stated::Mode)
            }
            Some(_) => Stated::Unreadable,
        },
        Some(_) => Stated::Unreadable,
    }
}

/// Where the mode in force came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// No layer said, and the command line did not: the default.
    Default,
    /// `--sandbox`.
    Flag,
    /// A settings file.
    File(PathBuf),
    /// The managed file, because nothing stricter was asked for.
    Managed(PathBuf),
}

/// The mode a session runs under and where it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    pub mode: SandboxMode,
    pub source: Source,
}

impl Default for Choice {
    fn default() -> Self {
        Self {
            mode: SandboxMode::Standard,
            source: Source::Default,
        }
    }
}

/// Why the managed file does not allow a mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Floor {
    /// It pins `sandbox.mode`.
    Mode,
    /// It pins the network closed and names no mode that already holds a program to a profile.
    /// `off` starts a program with no profile, so the closed network would not bind it.
    Network,
}

/// A mode asked for that the managed file does not allow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refused {
    /// What was asked for.
    pub asked: SandboxMode,
    /// Where it was asked, or `None` for the command line.
    pub asked_in: Option<PathBuf>,
    /// The loosest mode the managed file allows.
    pub pinned: SandboxMode,
    /// The managed file.
    pub pinned_in: PathBuf,
    /// Which of its keys makes it a floor.
    pub because: Floor,
}

/// The mode in force: the flag, else the settings files, else the managed pin, else standard.
///
/// A mode looser than the managed pin is refused whoever asked for it. One at least as strict is
/// kept, so a person may be stricter than the machine requires. A managed file that pins the
/// network closed is a floor of `standard` as well, since `off` would otherwise open it.
pub fn resolve(
    flag: Option<SandboxMode>,
    settings: &crate::Settings,
    managed: &crate::Managed,
) -> Result<Choice, Refused> {
    let asked = match flag {
        Some(mode) => Some(Choice {
            mode,
            source: Source::Flag,
        }),
        None => settings.sandbox().map(|(mode, file)| Choice {
            mode,
            source: Source::File(file.to_path_buf()),
        }),
    };
    let Some(floor) = floor(managed) else {
        return Ok(asked.unwrap_or_default());
    };
    match asked {
        Some(asked) if asked.mode.is_looser_than(floor.mode) => Err(Refused {
            asked: asked.mode,
            asked_in: match asked.source {
                Source::File(file) => Some(file),
                _ => None,
            },
            pinned: floor.mode,
            pinned_in: floor.file,
            because: floor.because,
        }),
        Some(asked) => Ok(asked),
        None if floor.because == Floor::Network => Ok(Choice::default()),
        None => Ok(Choice {
            mode: floor.mode,
            source: Source::Managed(floor.file),
        }),
    }
}

/// The loosest mode the managed file allows, and the file and key that say so.
struct Held {
    mode: SandboxMode,
    file: PathBuf,
    because: Floor,
}

/// What the managed file holds the mode to, or `None` where it holds it to nothing.
fn floor(managed: &crate::Managed) -> Option<Held> {
    let file = managed.path().map(Path::to_path_buf)?;
    let closed = managed.network() == Some(bravebot_sandbox::network::Network::Closed);
    let (mode, because) = match managed.sandbox() {
        Some(pinned) if pinned <= SandboxMode::Standard || !closed => (pinned, Floor::Mode),
        _ if closed => (SandboxMode::Standard, Floor::Network),
        _ => return None,
    };
    Some(Held {
        mode,
        file,
        because,
    })
}

/// Whether a session already running may move to `mode`, which is the question [`resolve`] puts to a
/// flag at start-up and is answered by the same floor.
///
/// What `/sandbox` asks before it changes anything. Nothing but the managed file is consulted: the
/// command is a person's word and ranks as `--sandbox` does, so a checkout's `strict` does not
/// outrank it.
pub fn allowed_in_session(mode: SandboxMode, managed: &crate::Managed) -> Result<(), Refused> {
    match floor(managed) {
        Some(held) if mode.is_looser_than(held.mode) => Err(Refused {
            asked: mode,
            asked_in: None,
            pinned: held.mode,
            pinned_in: held.file,
            because: held.because,
        }),
        _ => Ok(()),
    }
}

/// The mode a window's turn runs under, which has no flag to carry and no way to say it was refused.
///
/// What the settings say, the managed pin being the floor where they went under it. `off` is
/// read as `standard`: the window has no way to show that nothing confines a program, and a
/// setting that silently removed the only thing between a model and the machine is not one a
/// person looking at a window has agreed to. The command line says so on its opening line.
pub fn for_a_window(settings: &crate::Settings, managed: &crate::Managed) -> SandboxMode {
    let mode = match resolve(None, settings, managed) {
        Ok(choice) => choice.mode,
        Err(refused) => refused.pinned,
    };
    mode.min(SandboxMode::Standard)
}

/// The mode this process settled on at start-up.
///
/// Process-wide for the reason [`crate::name_a_settings_file`] is: `--sandbox` configures this run,
/// and the interface, the one-shot run and the plain session each start a turn that has to hold
/// its programs to the same answer, none of them reached from the entry point that parsed the flag.
static IN_FORCE: OnceLock<Choice> = OnceLock::new();

/// Settle the mode for the rest of this process. The first call wins.
pub fn engage(choice: Choice) {
    let _ = IN_FORCE.set(choice);
}

/// The mode the entry point settled on, or the default where it settled none (a test, or a front
/// end started by something other than the command line).
pub fn in_force() -> Choice {
    IN_FORCE.get().cloned().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn root(value: serde_json::Value) -> serde_json::Map<String, serde_json::Value> {
        value.as_object().cloned().expect("an object")
    }

    #[test]
    fn a_mode_is_read_from_the_block_and_nothing_near_it_is() {
        assert_eq!(
            stated(&root(json!({"sandbox": {"mode": "strict"}}))),
            Stated::Mode(SandboxMode::Strict)
        );
        assert_eq!(stated(&root(json!({}))), Stated::Absent);
        assert_eq!(stated(&root(json!({"sandbox": {}}))), Stated::Absent);
        assert_eq!(
            stated(&root(json!({"sandbox": {"enabled": true}}))),
            Stated::Absent
        );
        for bad in [
            json!({"sandbox": {"mode": "Strict"}}),
            json!({"sandbox": {"mode": true}}),
            json!({"sandbox": {"mode": null}}),
            json!({"sandbox": "strict"}),
            json!({"sandbox": true}),
        ] {
            assert_eq!(stated(&root(bad.clone())), Stated::Unreadable, "{bad}");
        }
    }
}
