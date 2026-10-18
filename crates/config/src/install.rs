//! How this copy of bravebot was installed, and the command that updates it.
//!
//! Two ways of installing have an update command this program can name: the npm package, whose
//! launcher says which it is, and the install script, which writes down where it put the binary.
//! Everything else, a build from source above all, has no command here. Naming one for a copy
//! nobody can account for would send somebody a line that replaces a binary that is not theirs.
//!
//! Here rather than beside the startup notice that reads it, because two surfaces ask the same
//! question: the interface says at startup that a newer version is out, and `bravebot update` says
//! how to update whether or not one is. The notice is presentation and the command line is another
//! presentation crate, so neither may reach the other ([LAYER-1](../../../docs/specs/layering.md)),
//! and what they share is the configuration surface both already depend on.
//!
//! Nothing here asks a registry anything. The command is a literal per installation, and which
//! installation this is comes from the launcher's word and a path on disk.

use std::path::{Path, PathBuf};

/// What the npm launcher sets [`crate::env_var::INSTALLED_VIA`] to.
const NPM: &str = "npm";

/// What updates an npm install.
const NPM_COMMAND: &str = "npm install -g @brave/bravebot@latest";

/// What updates a script install: the same line that installed it, which takes the newest release
/// and puts it where this one already is.
const SCRIPT_COMMAND: &str =
    "curl -fsSL https://raw.githubusercontent.com/brave/bravebot/main/install.sh | sh";

/// The file the install script writes the installed path into.
const INSTALLED_BY_FILE: &str = "installed-by";

/// How this copy was installed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Install {
    /// The npm package. Its launcher runs this binary and says so.
    Npm,
    /// The install script, which recorded where it put the binary.
    Script,
}

impl Install {
    /// The command that updates a copy installed this way.
    ///
    /// A literal per installation, never composed: this is a line somebody pastes into a shell,
    /// so no part of it comes from a file, an environment variable or a response.
    pub fn update_command(self) -> &'static str {
        match self {
            Self::Npm => NPM_COMMAND,
            Self::Script => SCRIPT_COMMAND,
        }
    }
}

/// How this copy was installed, or `None` for one nothing here can offer a command for.
pub fn installed_how() -> Option<Install> {
    method(
        std::env::var(crate::env_var::INSTALLED_VIA).ok().as_deref(),
        // Compared against the known install locations to report how bravebot was installed.
        // A wrong answer downgrades to "unknown"; nothing is granted on the strength of it.
        // nosemgrep: rust.lang.security.current-exe.current-exe
        std::env::current_exe().ok().as_deref(),
        recorded_install().as_deref(),
    )
}

/// Decide from the launcher's word and the recorded path, so neither the environment nor the
/// filesystem is needed to test it.
///
/// The recorded path has to be the binary that is actually running. A checkout built from source,
/// on a machine where the script installed a copy as well, is not a script install: it is a build
/// nothing here knows how to update, and telling its user to curl over the top of it would replace
/// somebody else's copy rather than theirs.
fn method(via: Option<&str>, running: Option<&Path>, recorded: Option<&Path>) -> Option<Install> {
    if via == Some(NPM) {
        return Some(Install::Npm);
    }
    match (running, recorded) {
        (Some(running), Some(recorded)) if same_file(running, recorded) => Some(Install::Script),
        _ => None,
    }
}

/// Whether two paths name the same binary, following symlinks where they resolve.
///
/// A directory on `PATH` is often a link, so the path a process was started by and the path the
/// script wrote down can spell the same file differently.
fn same_file(left: &Path, right: &Path) -> bool {
    let resolved = |path: &Path| std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    left == right || resolved(left) == resolved(right)
}

/// Where the install script says it put the binary.
fn recorded_install() -> Option<PathBuf> {
    let path = crate::settings::home()?.join(INSTALLED_BY_FILE);
    let contents = std::fs::read_to_string(path).ok()?;
    let recorded = contents.lines().next()?.trim();
    (!recorded.is_empty()).then(|| PathBuf::from(recorded))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The launcher is the one thing that knows it is the npm package, since the binary it starts
    /// is an ordinary file wherever npm happened to unpack it.
    #[test]
    fn the_launcher_saying_npm_is_what_makes_it_an_npm_install() {
        assert_eq!(
            method(Some("npm"), None, None),
            Some(Install::Npm),
            "the launcher was not believed"
        );
        assert_eq!(
            method(Some("something else"), None, None),
            None,
            "a word nothing sets was taken for an installation"
        );
    }

    #[test]
    fn the_binary_the_script_recorded_is_a_script_install() {
        assert_eq!(
            method(
                None,
                Some(Path::new("/usr/local/bin/bravebot")),
                Some(Path::new("/usr/local/bin/bravebot"))
            ),
            Some(Install::Script)
        );
    }

    /// A checkout built from source on a machine that also has a script install is a build nothing
    /// here can update, and a command that replaced the other copy would be worse than silence.
    #[test]
    fn a_binary_other_than_the_recorded_one_is_not_a_script_install() {
        assert_eq!(
            method(
                None,
                Some(Path::new("/home/someone/bravebot/target/debug/bravebot")),
                Some(Path::new("/usr/local/bin/bravebot"))
            ),
            None
        );
    }

    /// A build from source, which is neither installation, has no command to be given.
    #[test]
    fn an_installation_nothing_recorded_is_left_alone() {
        assert_eq!(method(None, Some(Path::new("/tmp/bravebot")), None), None);
    }

    /// Each installation is told to update the way it was installed. An npm copy sent the script
    /// would gain a second binary and update the one that is not running, and a script copy sent
    /// the npm line would be told to install a package manager's copy instead of replacing itself.
    #[test]
    fn each_installation_is_updated_the_way_it_was_installed() {
        assert_eq!(
            Install::Npm.update_command(),
            "npm install -g @brave/bravebot@latest"
        );
        assert!(
            Install::Script.update_command().contains("install.sh"),
            "{}",
            Install::Script.update_command()
        );
        assert!(
            !Install::Script.update_command().contains("npm install"),
            "{}",
            Install::Script.update_command()
        );
    }
}
