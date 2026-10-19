//! How this copy of bravebot was installed, and the command that updates it.
//!
//! Four ways of installing have an update command this program can name: the npm package, whose
//! launcher says which it is, the install script, which writes down where it put the binary, and
//! the Homebrew cask and the WinGet package, each of which keeps the binary under a directory only
//! that package manager creates.
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

/// What updates a Homebrew cask install.
const HOMEBREW_COMMAND: &str = "brew upgrade --cask bravebot";

/// What updates a WinGet install.
const WINGET_COMMAND: &str = "winget upgrade Brave.Bravebot";

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
    /// The Homebrew cask, which keeps the binary under `Caskroom/bravebot/`.
    Homebrew,
    /// The WinGet package, which keeps the binary under `WinGet/Packages/Brave.Bravebot_*/`.
    Winget,
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
            Self::Homebrew => HOMEBREW_COMMAND,
            Self::Winget => WINGET_COMMAND,
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
    if let Some(manager) = running.and_then(package_manager) {
        return Some(manager);
    }
    match (running, recorded) {
        (Some(running), Some(recorded)) if same_file(running, recorded) => Some(Install::Script),
        _ => None,
    }
}

/// The package manager whose directory holds this binary, once links are followed.
///
/// A cask or a portable WinGet package is one binary the manager unpacked into a directory of its
/// own and linked onto the `PATH`, so the link is what runs and the directory it resolves into is
/// what says which manager put it there. A copy built from source, or one the script wrote, is
/// under neither and is not told to ask a package manager for an update it did not come from.
fn package_manager(running: &Path) -> Option<Install> {
    let resolved = std::fs::canonicalize(running).unwrap_or_else(|_| running.to_path_buf());
    let names: Vec<String> = resolved
        .components()
        .map(|part| part.as_os_str().to_string_lossy().into_owned())
        .collect();
    let below = |parent: &str, name: &dyn Fn(&str) -> bool| {
        names
            .windows(2)
            .any(|pair| pair[0].eq_ignore_ascii_case(parent) && name(&pair[1]))
    };
    // Homebrew's own directory for casks, then the package's name: another cask's binary under it
    // is not this one's.
    if below("Caskroom", &|name| name == "bravebot") {
        return Some(Install::Homebrew);
    }
    // WinGet names a portable package's directory `<identifier>_<source>`.
    if below("Packages", &|name| {
        name.get(..15)
            .is_some_and(|head| head.eq_ignore_ascii_case("Brave.Bravebot_"))
    }) && names.iter().any(|part| part.eq_ignore_ascii_case("WinGet"))
    {
        return Some(Install::Winget);
    }
    None
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

    /// A package manager's directory is what says it installed the binary, and only that package's
    /// directory: another cask's binary, or a copy beside them, is nobody's to upgrade from here.
    #[test]
    fn a_binary_under_a_package_managers_directory_is_that_managers_install() {
        let under = |path: &str| method(None, Some(Path::new(path)), None);
        assert_eq!(
            under("/opt/homebrew/Caskroom/bravebot/0.9.0/bravebot-darwin-arm64"),
            Some(Install::Homebrew)
        );
        assert_eq!(
            under("/opt/homebrew/Caskroom/other/1.0/bravebot"),
            None,
            "another cask's binary was taken for this one's"
        );
        assert_eq!(
            under("/home/a/Caskroom-copy/bravebot"),
            None,
            "a directory that only resembles Homebrew's was believed"
        );
        assert_eq!(
            under(
                "/home/a/WinGet/Packages/Brave.Bravebot_Microsoft.Winget.Source_8wekyb3d8bbwe/bravebot"
            ),
            Some(Install::Winget)
        );
        assert_eq!(
            under("/home/a/Packages/Brave.Bravebot_x/bravebot"),
            None,
            "a Packages directory with no WinGet above it was believed"
        );
    }

    /// A binary the manager unpacked is its install even when the script's record names the same
    /// path, since the record can only be stale there.
    #[test]
    fn a_package_manager_directory_wins_over_a_script_record() {
        let path = Path::new("/opt/homebrew/Caskroom/bravebot/0.9.0/bravebot-darwin-arm64");
        assert_eq!(
            method(None, Some(path), Some(path)),
            Some(Install::Homebrew)
        );
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
        assert_eq!(
            Install::Homebrew.update_command(),
            "brew upgrade --cask bravebot"
        );
        assert_eq!(
            Install::Winget.update_command(),
            "winget upgrade Brave.Bravebot"
        );
        assert!(
            !Install::Script.update_command().contains("npm install"),
            "{}",
            Install::Script.update_command()
        );
    }
}
