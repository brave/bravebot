//! Firing the hooks a person attached to a moment, and the sentences for those that went wrong.

use bravebot_i18n::t;

use crate::report::Reporter;
use crate::workspace::Workspace;

/// Where [`compact`] finds the hooks a person declared and the directory it runs them in.
#[derive(Clone, Copy)]
pub struct Hooked<'a> {
    /// The user's own directory, which holds `hooks.json` (HOOK-1). `None` where the platform names
    /// none, which leaves no hooks.
    pub home: Option<&'a std::path::Path>,
    /// The workspace the session is working in: a hook runs in its root (HOOK-4) and the session's
    /// rewind coverage records that a program ran beside it.
    pub workspace: &'a Workspace,
}

/// Run the hooks a person attached to `moment`, and answer with what went wrong where something
/// did.
///
/// A hook that ended well is said nothing about: it did what it was asked, and a line per round
/// saying so would bury the turn. A hook that could not run is worth a sentence, because the
/// person is otherwise waiting for a formatter that has not run since they mistyped its path.
///
/// Said to a live display as it happens and handed back as well, because a caller with nowhere to
/// draw reads the turn's notices off the outcome, and a one-shot run whose hook never fired is
/// exactly the case that needs telling.
///
/// Nothing here reads what a hook produced or changes what the turn does next. See
/// [`crate::hooks`], which is where that argument lives.
pub(super) fn fire_hooks<R: Reporter + ?Sized>(
    hooks: &bravebot_config::hooks::Hooks,
    moment: bravebot_config::hooks::Moment,
    tool: Option<&str>,
    workspace: &Workspace,
    reporter: &mut R,
) -> Vec<String> {
    firing_in(hooks, moment, tool, workspace, reporter, |watch| {
        crate::hooks::fire_watched(
            hooks,
            moment,
            tool,
            workspace.root(),
            crate::hooks::default_limit(moment),
            watch,
        )
    })
}

/// Run the hooks for the moment a checkout was written for a delegate, in that checkout, and
/// answer with what went wrong where something did (HOOK-2).
///
/// Nothing is fired for a workspace that is not a checkout. The hook is handed two directories
/// (HOOK-5): the checkout's, which the driver made, and the working directory it was made from,
/// which the person's session works in. Neither is read from anything a model or a file wrote.
pub(super) fn fire_checkout_made<R: Reporter + ?Sized>(
    hooks: &bravebot_config::hooks::Hooks,
    workspace: &Workspace,
    reporter: &mut R,
) -> Vec<String> {
    let Some(checkout) = workspace.checkout() else {
        return Vec::new();
    };
    let moment = bravebot_config::hooks::Moment::CheckoutMade;
    firing_in(hooks, moment, None, workspace, reporter, |watch| {
        crate::hooks::fire_checkout_made(
            hooks,
            crate::hooks::Made {
                checkout: checkout.path(),
                working_directory: checkout.working_directory(),
            },
            crate::hooks::default_limit(moment),
            watch,
        )
    })
}

/// What [`fire_hooks`] and [`fire_checkout_made`] share: the record that a program ran in the
/// workspace, the display told as each hook starts and ends, and a sentence for each that went
/// wrong. `fire` runs the hooks, telling the watcher it is given.
fn firing_in<R: Reporter + ?Sized>(
    hooks: &bravebot_config::hooks::Hooks,
    moment: bravebot_config::hooks::Moment,
    tool: Option<&str>,
    workspace: &Workspace,
    reporter: &mut R,
    fire: impl FnOnce(&mut dyn FnMut(crate::hooks::Watch<'_>)) -> Vec<crate::hooks::Fired>,
) -> Vec<String> {
    // Hook programs can write files outside the backup journal, regardless of their outcome.
    if hooks.firing(moment, tool).next().is_some() {
        workspace.mark_rewind_gap(crate::rewind::CoverageGap::Hook);
        if let Some(checkout) = workspace.checkout() {
            checkout.mark_worked_in();
        }
    }
    let fired_all = fire(&mut |edge| match edge {
        crate::hooks::Watch::Starting { moment, program } => {
            reporter.hook_started(moment, program.to_string())
        }
        crate::hooks::Watch::Over => reporter.hook_finished(),
    });
    let said = trouble_sentences(fired_all);
    for one in &said {
        reporter.notice(one.clone());
    }
    said
}

/// The confirmer fires `prompt-shown` with no reporter to hand and leaves each hook's trouble in
/// `waiting`, so it is drained here, where the turn has one.
pub(super) fn say_prompt_notices<R: Reporter + ?Sized>(
    waiting: &std::sync::Mutex<Vec<String>>,
    reporter: &mut R,
) -> Vec<String> {
    let said = waiting
        .lock()
        .map(|mut held| std::mem::take(&mut *held))
        .unwrap_or_default();
    for one in &said {
        reporter.notice(one.clone());
    }
    said
}

/// What went wrong, in a sentence each, for the hooks that did not run and end well.
pub(crate) fn trouble_sentences(fired_all: Vec<crate::hooks::Fired>) -> Vec<String> {
    let mut said = Vec::new();
    for fired in fired_all {
        let Some(trouble) = fired.trouble else {
            continue;
        };
        said.push(match trouble {
            crate::hooks::Trouble::NotStarted(detail) => t!(
                hook_not_started,
                moment = fired.moment,
                program = fired.program,
                detail = detail
            ),
            crate::hooks::Trouble::Ended(status) => t!(
                hook_failed,
                moment = fired.moment,
                program = fired.program,
                status = status
            ),
            crate::hooks::Trouble::Stopped(limit) => t!(
                hook_stopped,
                moment = fired.moment,
                program = fired.program,
                seconds = limit.as_secs()
            ),
        });
    }
    said
}
