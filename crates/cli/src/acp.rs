//! `bravebot acp`: the Agent Client Protocol on stdin and stdout, for an editor that hosts a session
//! (ACP-1, CLI-39).
//!
//! The loop is the one `bravebot-acp` runs, [`bravebot_ui_bridge::acp_loop::serve`], so what each
//! message means is decided in one place. What is left here is what a command of this binary does
//! before it: the process-wide answers were settled by the entry point, the retention sweep is
//! started, and the settings file the command line named is handed to the sessions the editor opens.

use std::io::BufReader;

/// `bravebot acp`: serve the editor until its input ends.
///
/// `None` is a command line this does not take: the command takes no words, since the settings file
/// was taken out of the arguments before dispatch, and a word left over is a flag this command does
/// not read. It is refused before anything is read from the editor, because stdout is the protocol
/// from the first byte.
pub(crate) fn command(args: &[String]) -> Option<()> {
    if !args.is_empty() {
        return None;
    }
    let settings = bravebot_config::named_settings_file().map(std::path::Path::to_path_buf);
    // The same layers `bravebot-acp` reads: this directory's project files, the person's and the
    // named one. The answers every turn reads were settled before dispatch.
    let layers = bravebot_ui_bridge::settings::layers(
        std::env::current_dir().ok().as_deref(),
        settings.as_deref(),
    );
    bravebot_ui_bridge::settings::start_the_sweep_for_process(&layers);

    let finished = bravebot_ui_bridge::acp_loop::serve(
        BufReader::new(std::io::stdin()),
        std::io::stdout(),
        settings,
        || eprintln!("{}", bravebot_i18n::t!(cli_acp_unreadable_line)),
    );
    if !finished {
        eprintln!("{}", bravebot_i18n::t!(cli_acp_sessions_still_working));
    }
    Some(())
}
