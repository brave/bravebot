//! `bravebot update`: the command that updates this copy (CLI-29).
//!
//! It says what to run and runs nothing. Replacing the running binary is a line somebody reads
//! before they paste it, and a command that did it would be this program choosing to overwrite
//! itself from the network on the strength of one word typed at a shell.
//!
//! Which installation this is, and the literal that updates it, are
//! [`bravebot_config::install`]'s. Nothing here asks a registry anything, so the command is the
//! same whether or not a newer version exists: it is how to update, not whether to.

use crate::exit::{Ending, fail};
use bravebot_i18n::t;
use std::process::ExitCode;

/// `bravebot update`: print the command that updates this copy.
///
/// Takes no arguments. A word after it is refused rather than ignored, for the reason `bug-report`
/// refuses one: a person who typed something meant it to do something.
pub(crate) fn command(args: &[String]) -> ExitCode {
    if !args.is_empty() {
        return fail(Ending::Argument, t!(cli_update_takes_nothing_else));
    }
    println!("{}", said(bravebot_config::install::installed_how()));
    ExitCode::SUCCESS
}

/// What `bravebot update` prints for an installation.
///
/// Apart from [`command`] so a test can give it an installation recognised from a layout a test
/// process cannot stand in: the apt and dnf packages are recognised at `/usr/bin/bravebot`, which
/// the binary under test is not at.
fn said(install: Option<bravebot_config::install::Install>) -> String {
    match install {
        Some(install) => t!(update_how, command = install.update_command()).to_string(),
        // A build from source is the common case here, and it is not a failure: this copy is
        // working, and there is no command this program can honestly name for it.
        None => t!(update_not_installed_by_us).to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bravebot_config::install::Install;

    /// CLI-29. A copy recognised as the apt package is told the apt line and a dnf copy the dnf
    /// line, each whole. The regression this rejects is `bravebot update` printing the
    /// no-command text for them, or one manager's line for the other's copy.
    #[test]
    fn a_copy_apt_or_dnf_installed_is_told_that_managers_upgrade_line() {
        let apt = said(Some(Install::Apt));
        assert!(
            apt.contains("sudo apt update && sudo apt upgrade bravebot"),
            "{apt}"
        );
        assert!(!apt.contains("dnf"), "an apt copy was sent dnf: {apt}");
        let dnf = said(Some(Install::Dnf));
        assert!(dnf.contains("sudo dnf upgrade bravebot"), "{dnf}");
        assert!(!dnf.contains("apt"), "a dnf copy was sent apt: {dnf}");
    }

    /// CLI-29. A copy nothing recognised is told there is no command and is not handed a
    /// package manager's line.
    #[test]
    fn a_copy_nothing_recognised_is_told_there_is_no_command() {
        let line = said(None);
        assert!(line.contains("no update command"), "{line}");
        assert!(
            !line.contains("apt upgrade") && !line.contains("dnf upgrade"),
            "{line}"
        );
    }
}
