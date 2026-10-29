//! Carrying on in a task the Sessions screen sent the person to (SCREEN-8).
//!
//! Called once the interface has handed the terminal back, from the process the person ran, so
//! what starts is still a thing a terminal did. A background process is joined as `bravebot attach`
//! joins it. A record of another directory is resumed or copied, and a session begun there, by this
//! same program started again in that directory with the option that does it, since a session is
//! found from the directory it ran in.

use crate::exit::{Ending, fail};
use bravebot_i18n::t;
use bravebot_tui::app::{Elsewhere, How};
use std::path::Path;
use std::process::ExitCode;

/// The arguments the program is started again with for `how`, or none for a session begun there.
///
/// An id that begins with a dash would be read as an option by the program it is handed to, so it is
/// refused here whatever shape a session's name may have.
fn arguments(how: &How) -> Option<Vec<String>> {
    match how {
        How::Attach(_) | How::Fresh => Some(Vec::new()),
        How::Resume(id) | How::Fork(id) if id.starts_with('-') => None,
        How::Resume(id) => Some(vec!["--resume".to_string(), id.clone()]),
        How::Fork(id) => Some(vec!["--fork".to_string(), id.clone()]),
    }
}

/// Do what `opening` asks, and end with its status.
pub(crate) fn open(opening: &Elsewhere) -> ExitCode {
    if let How::Attach(id) = &opening.how {
        return crate::background::attach(std::slice::from_ref(id));
    }
    let Some(arguments) = arguments(&opening.how) else {
        return fail(Ending::Argument, t!(sessions_screen_open_not_a_session));
    };
    start_again(Path::new(&opening.directory), &arguments)
}

/// The command that starts this program again in `directory` with `arguments`.
///
/// The program is this process's own executable, by the absolute path the operating system gives
/// it. It is started with the environment and the `PATH` this process has, as the person's shell
/// would give them, because it is the person's own session continuing: the credentials this agent
/// holds stay in that environment, and no program a turn runs and no prompt receives them (CRED-14
/// in credential-protection.md). What the new process starts by name it starts under the rule of
/// SANDBOX-30 in sandboxing.md itself, so no name is resolved here. The spec records this under the
/// Known costs of sessions-screen.md.
fn command_for(directory: &Path, arguments: &[String]) -> std::io::Result<std::process::Command> {
    let me = bravebot_sandbox::programs::Resolved::own_executable()?;
    let mut command = bravebot_sandbox::programs::start(
        &me,
        bravebot_sandbox::programs::Inherits::All,
        bravebot_sandbox::programs::SearchPath::Inherited,
    );
    command.args(arguments).current_dir(directory);
    Ok(command)
}

/// This program, started again in `directory` with `arguments`, taking the terminal.
///
/// Where the platform can, the process is replaced, so the terminal has one `bravebot` on it and a
/// signal reaches it directly. The environment is the one this process has, and nothing from the
/// task is passed on but the id, which `--resume` and `--fork` read as a session's name.
fn start_again(directory: &Path, arguments: &[String]) -> ExitCode {
    let Ok(mut command) = command_for(directory, arguments) else {
        return fail(Ending::Failed, t!(sessions_screen_open_not_started));
    };
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let problem = command.exec();
        fail(
            Ending::Failed,
            t!(sessions_screen_open_failed, problem = problem.to_string()),
        )
    }
    #[cfg(not(unix))]
    match command.status() {
        Ok(status) => status
            .code()
            .and_then(|code| u8::try_from(code).ok())
            .map_or(ExitCode::FAILURE, ExitCode::from),
        Err(problem) => fail(
            Ending::Failed,
            t!(sessions_screen_open_failed, problem = problem.to_string()),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// SCREEN-8. Each way of carrying on is the option that does it and the id the Sessions screen holds,
    /// and a session begun in a directory is the program started there with nothing.
    #[test]
    fn each_way_of_carrying_on_is_the_option_that_does_it() {
        assert_eq!(
            arguments(&How::Resume("abc".to_string())),
            Some(vec!["--resume".to_string(), "abc".to_string()])
        );
        assert_eq!(
            arguments(&How::Fork("abc".to_string())),
            Some(vec!["--fork".to_string(), "abc".to_string()])
        );
        assert_eq!(arguments(&How::Fresh), Some(Vec::new()));
        assert_eq!(arguments(&How::Attach("abc".to_string())), Some(Vec::new()));
    }

    /// SCREEN-8. An id that begins with a dash is never handed to the program as an argument, where
    /// it would be read as an option.
    #[test]
    fn an_id_that_begins_with_a_dash_is_not_passed_on() {
        for id in ["-x", "--fork", "--resume"] {
            assert_eq!(arguments(&How::Resume(id.to_string())), None, "{id}");
            assert_eq!(arguments(&How::Fork(id.to_string())), None, "{id}");
        }
    }

    /// SCREEN-8. The program started again is this process's own executable by its absolute path,
    /// and not a name looked up on the `PATH`, with the directory and the arguments it was given and
    /// the environment of this process.
    #[test]
    fn the_program_started_again_is_this_one_by_its_absolute_path() {
        let directory = std::env::temp_dir();
        let arguments = vec!["--resume".to_string(), "abc".to_string()];

        let command = command_for(&directory, &arguments).expect("a command");

        let me = std::env::current_exe().expect("this program");
        assert!(Path::new(command.get_program()).is_absolute());
        assert_eq!(Path::new(command.get_program()), me.as_path());
        assert_eq!(
            command
                .get_args()
                .map(|a| a.to_string_lossy().into_owned())
                .collect::<Vec<_>>(),
            arguments
        );
        assert_eq!(command.get_current_dir(), Some(directory.as_path()));
        assert_eq!(
            command.get_envs().count(),
            0,
            "the environment was changed, or a credential removed, in the program's own start"
        );
    }
}
