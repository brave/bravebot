//! `bravebot update`: the command that updates this copy (CLI-28).
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
    match bravebot_config::install::installed_how() {
        Some(install) => {
            println!("{}", t!(update_how, command = install.update_command()));
            ExitCode::SUCCESS
        }
        // A build from source is the common case here, and it is not a failure: this copy is
        // working, and there is no command this program can honestly name for it.
        None => {
            println!("{}", t!(update_not_installed_by_us));
            ExitCode::SUCCESS
        }
    }
}
