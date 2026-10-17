//! Command-line entry point.

#![forbid(unsafe_code)]

mod auth;
mod background;
mod bug_report;
mod completion;
mod continued;
mod doctor;
mod exit;
mod finish;
mod flags;
mod host;
mod import;
mod json;
mod mcp;
mod model;
mod one_shot;
mod permissions_check;
mod plain;
mod progress;
mod push_identity;
mod sandbox_check;
mod session_import;
mod session_search;
mod sessions_list;
mod shell_init;
#[cfg(unix)]
mod supervisor;
#[cfg(test)]
mod test_support;
mod update;
mod usage;
mod workspace;
use bravebot_agent::servers;

use crate::exit::{Ending, fail};
use crate::finish::{
    Finished, ending_of_a_turn, failure_of_a_turn, print_trace, refusals, refused_with_the_usage,
    report, say_notices, say_the_result, say_what_a_stopped_manifest_produced,
    stopped_before_the_turn, what_ran,
};
use crate::flags::{
    INCOGNITO, SAFE, SKIP_PERMISSIONS, VET, starts_neither_a_session_nor_a_task, take_flag,
    take_valued,
};
use crate::model::{
    model_asked_for, model_for_this_run, model_not_served, nothing_serves, resolve_advisor,
};
use crate::one_shot::OneShot;
use crate::usage::print_help;
use crate::workspace::{
    confinement_name, current_workspace, definition_for_a_run, open_directories,
    rules_for_a_one_shot_run, scratch_for_this_run, unattended_trust,
};
use bravebot_agent::Mode;
use bravebot_agent::turn::{self, SystemPrompts, Task};
use bravebot_config::{Config, Managed};
use bravebot_core::cancel::Cancel;
use bravebot_i18n::t;
use bravebot_sandbox::SandboxMode;
use bravebot_session::sessions::Resumable;
use std::io::{IsTerminal, Read};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// What the command line settled, for the dispatch that follows.
struct Settled {
    as_json: bool,
    skip_permissions: bool,
    carried: Option<String>,
    agent: Option<String>,
    links: bravebot_tui::app::Links,
    prompts: SystemPrompts,
}

/// Takes every flag that belongs to the process rather than to a command out of `args`, and engages
/// what each one switches on, in the order the comments below give. A line that is refused stops
/// here, ahead of any dispatch, with the status `stopped_before_the_turn` chose.
fn settle_flags(args: &mut Vec<String>) -> Result<Settled, ExitCode> {
    // A bare `--` ends bravebot's own flags: what follows is another program's argv, as after
    // `mcp add <alias> --`, where a server's own `--settings` must stay the server's.
    let foreign = match args.iter().position(|arg| arg == "--") {
        Some(at) => args.split_off(at),
        None => Vec::new(),
    };

    // Noted before the flags below are taken out, because they are what it looks for: a session
    // that starts in another process reads none of them, and one that ran without a flag it was
    // given would be running under rules its author did not choose.
    let carried: Option<String> = args
        .iter()
        .find(|arg| {
            matches!(
                arg.as_str(),
                "--incognito"
                    | "--safe"
                    | "--locked"
                    | "--tools"
                    | "--no-shell"
                    | "--vet"
                    | "--settings"
                    | "--run-network"
                    | "--sandbox"
                    | "--sandbox-allow-read"
                    | "--sandbox-deny-read"
                    | "--sandbox-allow-write"
                    | "--sandbox-deny-write"
                    | "--log-level"
            )
        })
        .cloned();
    if let Some(flag) = &carried {
        bravebot_tui::app::background_cannot_carry(flag);
    }

    // Engaged here rather than deeper in because it must be true before the first thing that could
    // write is reached, and this is the last moment that is certain to be before all of them.
    //
    // Removed before anything dispatches on what leads the list, so the mode composes with every
    // other way of starting rather than being a fourth flag that may lead: `--incognito -p "task"`,
    // `--incognito --resume` and `--incognito` on its own all mean what they look like. Repeats are
    // one flag rather than an error: `--incognito --incognito` asks for a mode that is already on,
    // and refusing it would be a rule about privacy's typing rather than about privacy.
    if take_flag(args, INCOGNITO) {
        bravebot_core::incognito::engage();
    }

    // Beside the mode above and for the same reason: every loader it turns off is reached while a
    // session is assembled, and this is before the first of them.
    //
    // It composes with every way of starting: a session, a resumed session, a session in lines and
    // a one-shot run all load the same five things, so reading it per subcommand would be a chance
    // to forget it in each.
    if take_flag(args, SAFE) {
        bravebot_core::safe::engage();
    }

    // Beside the mode above, which it includes, and before the first setting is read: a settings
    // layer read once from a checkout cannot be unread. The bypass flag is refused here and not
    // after the arguments are parsed, so that nothing else the line says is acted on first.
    let locked = take_flag(args, "--locked");
    if locked {
        bravebot_core::safe::engage();
        bravebot_config::lock();
    }

    // Beside the mode above and for the same reason: the switch has to be settled before a session
    // is assembled, and this is the last moment certain to be before every way of assembling one.
    // What it turns on is read once per session, where the three routes into it are resolved.
    //
    // Turns auto-vetting on for this run, and outranks both standing answers because it is the
    // narrowest in time: somebody typing it has said what they want of the run in front of them.
    // That includes a recorded `off`, on the footing `--dangerously-skip-permissions` sits on,
    // which is a strictly larger thing anything able to pass this flag could pass instead. There is
    // no flag the other way. It would matter only to somebody who had turned the mode on standing
    // and wanted one run without it, and for them the answer is the file the standing answer is
    // kept in. A second flag whose absence and presence both mean something is worth adding when
    // there is a case for it, not before.
    if take_flag(args, VET) {
        bravebot_core::vetting::ask_for_it();
    }

    // Taken out before anything dispatches on the first argument, because this one belongs to every
    // way of starting: a session, a resumed session, and a one-shot run all put the same four
    // questions to the same trait. Reading it per subcommand would be four chances to read it in
    // three of them, and the flag would then be silently ignored wherever it was forgotten.
    //
    // See [`bravebot_agent::PermissionMode`] for what giving it up costs. Deny rules from the
    // settings file are not part of it: they refuse before there is anything to prompt about, so
    // the flag means "stop asking me" rather than "forget what I wrote down".
    let skip_permissions = take_flag(args, SKIP_PERMISSIONS);
    if locked && skip_permissions {
        return Err(stopped_before_the_turn(
            wants_json(args),
            Ending::Argument,
            t!(cli_locked_refuses_bypass).to_string(),
        ));
    }

    // Read off the arguments as typed, before the flag below takes anything out of them: a result
    // object was asked for by the command line that failed, whichever token the failure consumed.
    let as_json = wants_json(args);

    // Taken out before dispatch for the reason the two above are, and acted on here because this is
    // before the first read of a setting: a layer registered after the interface had loaded the
    // others would configure half of one process.
    match take_settings(args) {
        Ok(None) => {}
        Ok(Some(path)) if path.is_file() => bravebot_config::name_a_settings_file(path),
        // Refused rather than ignored, and for the audience CLI-11 refuses a directory for: a run
        // told to configure itself from a file is a run whose configuration is the file, so falling
        // back to whatever was found would be the wrong configuration used in silence. A mistyped
        // path and a variable that expanded to nothing look the same here, and both are common.
        Ok(Some(path)) => {
            return Err(stopped_before_the_turn(
                as_json,
                Ending::Argument,
                t!(cli_settings_not_a_file, path = path.display().to_string()),
            ));
        }
        Err(complaint) => {
            return Err(stopped_before_the_turn(
                as_json,
                Ending::Argument,
                complaint,
            ));
        }
    }

    // After the settings file is registered, which the settings it reads include, and before a
    // session is assembled: a stage started under one answer and the next under another would
    // make the network a property of the line rather than of the session.
    match take_run_network(args) {
        Ok(flag) => {
            bravebot_config::settle_run_network(flag);
        }
        Err(complaint) => {
            return Err(stopped_before_the_turn(
                as_json,
                Ending::Argument,
                complaint,
            ));
        }
    }

    // Settled with the network and for the same reason: the lists a stage is held to are a property
    // of the session, and a line started under one answer and the next under another would make
    // them a property of the line. The flags are the person's own act for this run, so they add to
    // what their settings say.
    match take_sandbox_filesystem(args) {
        Ok(lists) => {
            bravebot_config::settle_sandbox_filesystem(&lists);
        }
        Err(complaint) => {
            return Err(stopped_before_the_turn(
                as_json,
                Ending::Argument,
                complaint,
            ));
        }
    }

    // The host list a stage is held to and the key each owner's repositories are pushed with,
    // settled with the lists above for the same reason.
    let layers = bravebot_config::Settings::load();
    bravebot_config::sandbox_network::settle(&layers);
    bravebot_config::git_identities::settle(&layers);

    // After the mode above is engaged, because an incognito session is given no directory to log
    // into, and before the first thing that could fail in a way worth recording.
    match take_log_level(args) {
        Ok(level) => bravebot_diag::configure(
            level.unwrap_or(bravebot_diag::Level::Error),
            bravebot_agent::home::writable(),
        ),
        Err(complaint) => {
            return Err(stopped_before_the_turn(
                as_json,
                Ending::Argument,
                complaint,
            ));
        }
    }

    // Taken out here, like the settings file, because a session, a session in lines and a one-shot
    // run all use the name the same way. It is matched later, by whichever of them starts, because
    // the set it is matched against depends on the directory's trust (CLI-17).
    let agent = match take_agent(args) {
        Ok(agent) => agent,
        Err(complaint) => {
            return Err(stopped_before_the_turn(
                as_json,
                Ending::Argument,
                complaint,
            ));
        }
    };
    if let Some(refused) = agent
        .as_ref()
        .and_then(|_| without_a_definition(args.first().map(String::as_str)))
    {
        return Err(stopped_before_the_turn(as_json, Ending::Argument, refused));
    }

    // Taken out beside `--agent`, for its reason: it names what the session it opens is about.
    let links = match take_links(args) {
        Ok(links) => links,
        Err(complaint) => {
            return Err(stopped_before_the_turn(
                as_json,
                Ending::Argument,
                complaint,
            ));
        }
    };
    if links != bravebot_tui::app::Links::default() && !opens_a_session(args) {
        return Err(stopped_before_the_turn(
            as_json,
            Ending::Argument,
            t!(cli_links_need_a_session).to_string(),
        ));
    }

    // Taken out here for the same reason: a session, a session in lines, a resumed session and a
    // one-shot run all carry the words the same way (CLI-19).
    let prompts = match take_system_prompts(args) {
        Ok(prompts) => prompts,
        Err(complaint) => {
            return Err(stopped_before_the_turn(
                as_json,
                Ending::Argument,
                complaint,
            ));
        }
    };
    if let Some(flag) = flag_named(&prompts)
        && let Some(refused) = without_a_prompt_to_give(flag, args.first().map(String::as_str))
    {
        return Err(stopped_before_the_turn(as_json, Ending::Argument, refused));
    }

    // After the flag above and after the layer named beside it, because a file `--settings` names is
    // one of the layers that may say this. MODE-5: the flag opens the bypass mode unless a layer in
    // force made it unreachable, and then the flag is refused with the file named rather than
    // ignored. Refused rather than downgraded to asking: a run that was told to stop asking and
    // carried on with a notice is a run whose author believes it is unattended.
    if skip_permissions
        && let Some(path) =
            bypass_made_unreachable(&bravebot_config::Settings::load(), &Managed::load())
    {
        return Err(stopped_before_the_turn(
            as_json,
            Ending::Argument,
            t!(cli_bypass_unreachable, path = path.display().to_string()),
        ));
    }

    // After the settings file is named and before anything dispatches, for the reason the settings
    // are: a turn started by any of the ways below holds its programs to the one answer settled
    // here, and a managed floor that the flag or a file goes under stops the session before it
    // starts rather than being met on the first `run`.
    let sandbox_flag = match take_sandbox(args) {
        Ok(flag) => flag,
        Err(complaint) => {
            return Err(stopped_before_the_turn(
                as_json,
                Ending::Argument,
                complaint,
            ));
        }
    };
    match bravebot_config::sandbox::resolve(
        sandbox_flag,
        &bravebot_config::Settings::load(),
        &Managed::load(),
    ) {
        Ok(choice) => bravebot_config::sandbox::engage(choice),
        // The commands that start no program are let through, `doctor` among them: the person
        // holding a setting the managed file refuses is the one who needs the report that names it,
        // and a diagnosis that is itself refused cannot be read.
        Err(refused) if starts_no_program(args.first().map(String::as_str)) => {
            bravebot_config::sandbox::engage(bravebot_config::sandbox::Choice {
                mode: refused.pinned,
                source: bravebot_config::sandbox::Source::Managed(refused.pinned_in),
            });
        }
        Err(refused) => {
            return Err(stopped_before_the_turn(
                as_json,
                Ending::Argument,
                sandbox_refusal(&refused),
            ));
        }
    }

    // Taken out after every flag that takes a value, so the first argument left is the command, and
    // settled before a session is assembled so that the list a turn is offered is written once under
    // one answer. A refused list stops here, ahead of any turn.
    let tool_limit = match take_tool_limit(args) {
        Ok(limit) => limit,
        Err(complaint) => {
            return Err(stopped_before_the_turn(
                wants_json(args),
                Ending::Argument,
                complaint,
            ));
        }
    };
    if !tool_limit.is_none() {
        if let Some(refused) = args
            .first()
            .and_then(|first| without_a_tool_set_to_limit(first))
        {
            return Err(stopped_before_the_turn(
                wants_json(args),
                Ending::Argument,
                refused,
            ));
        }
        bravebot_core::tool_set::settle(tool_limit);
    }

    // Before the words after a bare `--` rejoin the line, since those are another program's.
    if let Some(refused) = interactive_out_of_place(args) {
        return Err(stopped_before_the_turn(as_json, Ending::Argument, refused));
    }

    args.extend(foreign);
    Ok(Settled {
        as_json,
        skip_permissions,
        carried,
        agent,
        links,
        prompts,
    })
}

fn main() -> ExitCode {
    // First of all, before the command line is read: what this process was started with, its
    // environment included, is readable from `/proc/<pid>/environ` by any process of the same
    // user until the kernel is told otherwise, and a program a stage starts is one. The mark is
    // cleared by `execve`, so the programs this process starts are not affected. A refusal is
    // said and the run goes on, since the remedy is outside the program. See SANDBOX-8.
    if let Some(notice) = bravebot_sandbox::inspection::refuse_same_user_inspection().notice() {
        eprintln!("bravebot: {notice}");
    }

    // Next, because it has to be true before the first credential is in memory and the
    // signing key is unmasked while the command line is still being read. A crash after this
    // writes no image of this process, which is where a key and a live session credential would
    // otherwise be recovered from. Nothing is done about a platform that refuses: the remedy is
    // outside the program, and quitting over it would protect the credential by making the tool
    // unusable. See CRED-23.
    let _ = bravebot_sandbox::crash::disable_core_dumps();

    // Before anything is printed, and exactly once: every later lookup reads what this settled
    // on. Nothing else in the tree consults the environment about a language.
    bravebot_i18n::init_from_environment();

    // The process's own argv, parsed below into the documented flags. There is no other
    // way for a command line tool to learn what it was asked to do.
    // nosemgrep: rust.lang.security.args.args
    let mut args: Vec<String> = std::env::args().skip(1).collect();

    let Settled {
        as_json,
        skip_permissions,
        carried,
        agent,
        links,
        prompts,
    } = match settle_flags(&mut args) {
        Ok(settled) => settled,
        Err(code) => return code,
    };

    match args.first().map(String::as_str) {
        Some("--version" | "-V") => {
            // The same words a session record writes down, so the two can be compared without
            // anyone having to work out what "the current build" means.
            println!("bravebot {}", bravebot_stamp::BUILD);
            ExitCode::SUCCESS
        }
        Some("--help" | "-h") => {
            print_help();
            ExitCode::SUCCESS
        }
        // With no arguments the interactive session is the natural default.
        None => interactive(
            bravebot_tui::app::Start::Fresh,
            agent,
            links,
            skip_permissions,
            prompts,
        ),
        // The same session, with a prompt from the command line sent as its first turn (CLI-31). A
        // flag of its own because `bravebot "<task>"` is the one-shot run scripts already call.
        Some("-i" | "--interactive") => {
            open_on(&args[1..], as_json, agent, links, skip_permissions, prompts)
        }
        // Picking up where a session left off, chosen from a list or named outright.
        Some("--resume" | "-r" | "--continue" | "-c") if continues_with_a_task(&args) => {
            run_task(&args, skip_permissions, agent, prompts)
        }
        Some("--resume" | "-r") => match args.get(1) {
            Some(id) => resume_named(id, agent, links, skip_permissions, prompts),
            None => interactive(
                bravebot_tui::app::Start::Choose,
                agent,
                links,
                skip_permissions,
                prompts,
            ),
        },
        // The same list, opened for the sessions linked to one pull request. A missing value is
        // refused rather than read as a plain `--resume`, since a script whose variable expanded to
        // nothing asked about one pull request and would be shown every session.
        Some("--from-pr") => match args.get(1).map(|wanted| wanted.trim()) {
            Some(wanted) if !wanted.is_empty() && !wanted.starts_with('-') => interactive(
                bravebot_tui::app::Start::ChooseFromPr(wanted.to_string()),
                agent,
                links,
                skip_permissions,
                prompts,
            ),
            _ => fail(Ending::Argument, t!(cli_from_pr_needs_a_value)),
        },
        // The same, for the session somebody was in a moment ago, which is the one they mean
        // often enough that asking them to find its id is asking for nothing.
        Some("--continue" | "-c") if !continues_with_a_task(&args) => {
            continue_here(agent, links, skip_permissions, prompts)
        }
        // Fork a session, creating a new session record that starts with the same transcript.
        Some("--fork" | "-f") => match args.get(1) {
            Some(id) => fork_named(id, agent, links, skip_permissions, prompts),
            // No result object here, and none is owed: reaching this arm means the arguments held
            // `--fork` and nothing after it, so the command line cannot also have carried `--json`.
            // `bravebot --fork --json` reads the flag as the session id and is refused by name in
            // `fork_named` instead.
            None => fail(Ending::Argument, t!(cli_fork_needs_a_name)),
        },
        // A session in lines, which takes nothing from the terminal (CLI-14). On its own, because
        // it starts a session rather than describing one: the flags that compose with every way of
        // starting are the ones taken out above, and everything else on this list is another way of
        // starting.
        Some("--plain") => match args.len() {
            1 => plain::session(skip_permissions, agent, prompts),
            _ => refused_with_the_usage(as_json, t!(cli_plain_takes_nothing_else)),
        },
        // The task flags may lead: `bravebot -p "task"` and `bravebot --mode manifest "task"`
        // would otherwise be caught below as unknown options.
        Some(
            "-p" | "--print" | "--mode" | "--model" | "--advisor" | "--effort" | "--file"
            | "--session-ref" | "--add-dir" | "--trust-workspace" | "--trace" | "--json"
            | "--json-stream" | "--output-schema",
        ) => run_task(&args, skip_permissions, agent, prompts),
        Some("doctor") if args.get(1).map(String::as_str) == Some("--sandbox-check") => {
            match args.len() {
                2 => sandbox_check::run(),
                _ => refused_with_the_usage(as_json, t!(cli_doctor_sandbox_takes_nothing_else)),
            }
        }
        Some("doctor") if args.get(1).map(String::as_str) == Some("--push-identity") => {
            match args.len() {
                2 | 3 => push_identity::run_command(args.get(2).map(String::as_str)),
                _ => refused_with_the_usage(as_json, t!(cli_doctor_push_identity_argument)),
            }
        }
        Some("doctor") => doctor::doctor(),
        Some("update") => update::command(&args[1..]),
        Some("bug-report") => bug_report::command(&args[1..]),
        Some("auth") => auth::command(&args[1..]),
        Some("sessions") => background::sessions(&args[1..]),
        Some("--bg") => match carried {
            Some(flag) => stopped_before_the_turn(
                as_json,
                Ending::Argument,
                t!(bg_takes_nothing_else, flag = flag.as_str()),
            ),
            None => background_start(&args[1..], skip_permissions, agent, &prompts),
        },
        Some("attach") => background::attach(&args[1..]),
        Some("reply") => background::reply(&args[1..]),
        Some("__bg-host") => {
            use bravebot_session::jobs::Mode as HostMode;
            match (args.get(1), args.get(2).map(|word| HostMode::named(word))) {
                (Some(id), Some(Some(mode))) => background::host(id, mode),
                (Some(id), None) => background::host(id, HostMode::Ask),
                _ => ExitCode::FAILURE,
            }
        }
        Some("__bg-supervisor") => background::supervise(),
        Some("mcp") => mcp::command(&args[1..]),
        Some("permissions") => permissions_check::command(&args[1..]),
        Some("completion") => match completion::command(&args[1..]) {
            Some(()) => ExitCode::SUCCESS,
            None => {
                stopped_before_the_turn(as_json, Ending::Argument, t!(cli_completion_needs_a_shell))
            }
        },
        Some("shell-init") => match shell_init::command(&args[1..]) {
            Some(()) => ExitCode::SUCCESS,
            None => {
                stopped_before_the_turn(as_json, Ending::Argument, t!(cli_shell_init_needs_a_shell))
            }
        },
        Some("import-leo-creds") => auth::import_leo_creds(&args[1..]),
        Some("import-providers") => import::providers(&args[1..]),
        Some(flag) if flag.starts_with('-') => {
            refused_with_the_usage(as_json, t!(cli_unknown_option, flag = flag))
        }
        // Anything else is treated as the task prompt.
        Some(_) => run_task(&args, skip_permissions, agent, prompts),
    }
}

/// `-i <prompt>`: the interface that draws, sending the prompt as its first turn (CLI-31).
///
/// Refused where stdin or stdout is not a terminal rather than run as a one-shot. Whoever wrote the
/// line asked for a person to watch the task and be able to stop it, and a run with nobody watching
/// is a different thing to do in their name, with `--dangerously-skip-permissions` most of all.
fn open_on(
    words: &[String],
    as_json: bool,
    agent: Option<String>,
    links: bravebot_tui::app::Links,
    skip_permissions: bool,
    prompts: SystemPrompts,
) -> ExitCode {
    let (prompt, named) = match prompt_to_open_on(words) {
        Ok(opening) => opening,
        Err(refused) => return stopped_before_the_turn(as_json, Ending::Argument, refused),
    };
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        return fail(Ending::Argument, t!(cli_interactive_needs_a_terminal));
    }
    interactive(
        bravebot_tui::app::Start::Sending(prompt, named),
        agent,
        links,
        skip_permissions,
        prompts,
    )
}

/// The prompt `-i` opens a session on and the model and level named after it, or why the words after
/// it give none.
///
/// One argument, taken as written: a prompt a script assembled holds newlines, and joining words as
/// `--bg` does would flatten them. A missing or blank prompt is refused, for the reason a blank
/// `--model` is: a script whose variable expanded to nothing asked for a task. So is one opening
/// with `-` and holding no whitespace, which is the next flag. `--model` and `--effort` may follow it,
/// read as a one-shot run reads them. Anything else after the prompt is refused by name rather than
/// ignored, because the other options that describe a task are a one-shot run's, and a session that
/// dropped `--file` would run without a file the line asked for.
fn prompt_to_open_on(words: &[String]) -> Result<(String, bravebot_tui::app::AskedFor), String> {
    let prompt = match words.first() {
        Some(prompt) if is_a_value(prompt) => prompt.clone(),
        _ => return Err(t!(cli_interactive_needs_a_prompt).to_string()),
    };
    let mut named = bravebot_tui::app::AskedFor::default();
    let mut index = 1;
    while let Some(word) = words.get(index) {
        match word.as_str() {
            "--model" => named.model = Some(model_name(words.get(index + 1))?),
            "--effort" => named.effort = Some(effort_level(words.get(index + 1))?),
            after => {
                return Err(t!(cli_interactive_takes_nothing_else, argument = after));
            }
        }
        index += 2;
    }
    Ok((prompt, named))
}

/// Whether the words after a flag that takes text are that text, rather than nothing or the next
/// flag.
///
/// Blank is nothing: a script whose variable expanded to nothing asked for words. Opening with `-`
/// and holding no whitespace is the next flag, and taken as the text it would be removed from the
/// arguments. A sentence opening with `-` holds a space and is text.
fn is_a_value(text: &str) -> bool {
    let text = text.trim();
    !text.is_empty() && !(text.starts_with('-') && !text.contains(char::is_whitespace))
}

/// Why `-i` cannot go where the line put it, or `None` where it can.
///
/// It opens a new session, so it cannot go with another way of starting one. After `--fork` or
/// `--from-pr` the session would open and the prompt be dropped, after `--resume` or `--continue`
/// the same or a one-shot run, and after `--bg` the words would start a session nobody watches on a
/// prompt opening `-i`. Only those decide: a command's own words, as in `permissions check Bash grep
/// -i`, may hold one.
fn interactive_out_of_place(args: &[String]) -> Option<String> {
    let first = args.first()?;
    let another_start = matches!(
        first.as_str(),
        "--resume" | "-r" | "--continue" | "-c" | "--fork" | "-f" | "--from-pr" | "--bg"
    );
    let after_it = args[1..]
        .iter()
        .any(|arg| arg == "-i" || arg == "--interactive");
    if !(another_start && after_it) {
        return None;
    }
    Some(t!(
        cli_interactive_not_with_another_start,
        flag = first.as_str()
    ))
}

/// `--bg <prompt>`, which starts a session in another process and so cannot carry what the other
/// flags carry into this one.
fn background_start(
    words: &[String],
    skip_permissions: bool,
    agent: Option<String>,
    prompts: &SystemPrompts,
) -> ExitCode {
    if skip_permissions {
        return fail(Ending::Argument, t!(bg_bypass_refused));
    }
    if agent.is_some() {
        return fail(
            Ending::Argument,
            t!(bg_takes_nothing_else, flag = "--agent"),
        );
    }
    if let Some(flag) = flag_named(prompts) {
        return fail(Ending::Argument, t!(bg_takes_nothing_else, flag = flag));
    }
    background::start(words)
}

/// Take `--agent <name>` out of the arguments, answering with the definition it named.
///
/// Removed before dispatch, like `--settings`. If it is given twice the last name is used, as with
/// `--settings`, because every turn can work under only one definition. A blank name is refused, as
/// `--model` refuses one, because a script whose variable expanded to nothing asked for a
/// definition and would otherwise run under the planner. A name opening with `-` is refused too. It
/// is the next flag, taken as a name, and no definition's name may open with one.
///
/// Only the name is read here. The way of starting that follows matches it, because the set depends
/// on the directory's trust, which has not been asked about yet.
fn take_agent(args: &mut Vec<String>) -> Result<Option<String>, String> {
    let named = take_valued(args, &["--agent"], |_, word| match word.map(str::trim) {
        Some(name) if !name.is_empty() && !name.starts_with('-') => Ok(name.to_string()),
        _ => Err(t!(cli_agent_needs_a_name).to_string()),
    })?;
    Ok(named.into_iter().last())
}

/// Take `--issue <url>` and `--pr <url>` out of the arguments, answering with the links they gave.
///
/// Removed before dispatch, like `--agent`, and the last of two of a kind is used. A value that is
/// not one `http` or `https` address with a host in printable ASCII is refused with a sentence that
/// does not repeat it, as `/issue` and `/pr` refuse one (PANEL-12): it may hold the escape that
/// made it not one. A missing value is refused too, so a script whose variable expanded to nothing
/// is not run with no link, untold.
fn take_links(args: &mut Vec<String>) -> Result<bravebot_tui::app::Links, String> {
    use bravebot_session::sessions::{Link, Url};
    let taken = take_valued(args, &["--issue", "--pr"], |flag, word| {
        let kind = match flag {
            "--issue" => Link::Issue,
            _ => Link::PullRequest,
        };
        match word.and_then(Url::read) {
            Some(url) => Ok((kind, url)),
            None => Err(match kind {
                Link::Issue => t!(cli_issue_needs_a_link).to_string(),
                Link::PullRequest => t!(cli_pr_needs_a_link).to_string(),
            }),
        }
    })?;
    let mut links = bravebot_tui::app::Links::default();
    for (kind, url) in taken {
        match kind {
            Link::Issue => links.issue = Some(url),
            Link::PullRequest => links.pull_request = Some(url),
        }
    }
    Ok(links)
}

/// Whether the arguments left after the flags are taken open an interactive session, which is the
/// only thing a link from `--issue` or `--pr` can be given to: a one-shot run, a session in lines
/// and a background one would drop it, and so do the commands that start no session.
fn opens_a_session(args: &[String]) -> bool {
    match args.first().map(String::as_str) {
        None | Some("-i" | "--interactive" | "--fork" | "-f" | "--from-pr") => true,
        Some("--resume" | "-r" | "--continue" | "-c") => !continues_with_a_task(args),
        Some(_) => false,
    }
}

/// Whether a leading `--resume` or `--continue` carries a task, which makes it a one-shot run
/// carrying on an earlier session (CLI-25) rather than a request to open one.
///
/// A task is `-p` or `--print`, or a word that is not a flag. A flag alone, which opened the
/// session before and still does, is not one. `--resume` followed by a flag names no session, and
/// the task run refuses it by name rather than looking for a session called `-p`.
fn continues_with_a_task(args: &[String]) -> bool {
    let carries = |rest: &[String]| {
        rest.iter()
            .any(|arg| matches!(arg.as_str(), "-p" | "--print") || !arg.starts_with('-'))
    };
    match args.first().map(String::as_str) {
        Some("--resume" | "-r") => match args.get(1) {
            Some(id) => id.starts_with('-') || carries(&args[2..]),
            None => false,
        },
        Some("--continue" | "-c") => carries(&args[1..]),
        _ => false,
    }
}

/// Why `--agent` cannot go with the command line's first argument, or `None` where it can.
///
/// The commands other than a session or a task start neither, so a name they would not use is
/// refused instead of ignored, for the reason CLI-13 gives about a settings file.
fn without_a_definition(first: Option<&str>) -> Option<String> {
    starts_neither_a_session_nor_a_task(first?)
        .map(|command| t!(cli_agent_not_for_a_command, command = command).to_string())
}

/// Take `--system-prompt <prompt>` and `--append-system-prompt <prompt>` out of the arguments.
///
/// Removed before dispatch, like `--agent`, and the last of two is used (CLI-19). A blank value is
/// refused, as `--model` refuses one, because a script whose variable expanded to nothing asked for
/// words and would otherwise run without them. A value that opens with `-` and holds no whitespace
/// is refused too: it is the next flag, and taken as the text it would be removed from the
/// arguments and the run would answer in another format. A sentence opening with `-` holds a space
/// and is text.
fn take_system_prompts(args: &mut Vec<String>) -> Result<SystemPrompts, String> {
    let given = take_valued(
        args,
        &["--system-prompt", "--append-system-prompt"],
        |flag, word| match word.map(str::trim) {
            Some(text) if is_a_value(text) => Ok((flag == "--system-prompt", text.to_string())),
            _ => Err(t!(cli_system_prompt_needs_text, flag = flag).to_string()),
        },
    )?;
    let mut prompts = SystemPrompts::default();
    for (replacing, text) in given {
        if replacing {
            prompts.replacing = Some(text);
        } else {
            prompts.appending = Some(text);
        }
    }
    Ok(prompts)
}

/// The flag a run was given, where it was given either, for the message that refuses it.
fn flag_named(prompts: &SystemPrompts) -> Option<&'static str> {
    match (&prompts.replacing, &prompts.appending) {
        (Some(_), _) => Some("--system-prompt"),
        (None, Some(_)) => Some("--append-system-prompt"),
        (None, None) => None,
    }
}

/// Why the words a flag gave cannot go with the command line's first argument, or `None` where
/// they can.
///
/// A resumed session takes them: the record stores no system prompt (INSTR-5), so the words apply to
/// the turns this process sends. The commands that start neither a session nor a task are refused
/// rather than ignored, for the reason CLI-13 gives about a settings file.
fn without_a_prompt_to_give(flag: &str, first: Option<&str>) -> Option<String> {
    starts_neither_a_session_nor_a_task(first?).map(|command| {
        t!(
            cli_system_prompt_not_for_a_command,
            flag = flag,
            command = command
        )
        .to_string()
    })
}

/// The file that made the bypass mode unreachable, where a layer in force asked for it (PERM-17).
///
/// The strictest thing any layer said, the managed file among them: both keys only refuse, so there
/// is nothing for the strongest layer to lift and no ordering to resolve (PERM-18). The file is what
/// comes back rather than a boolean, because a flag refused without naming where the refusal was
/// written is a person reading the help for a flag that is documented and does not work.
fn bypass_made_unreachable(
    settings: &bravebot_config::Settings,
    managed: &Managed,
) -> Option<PathBuf> {
    if !settings
        .narrowing()
        .strictest(managed.narrowing())
        .makes_bypass_unreachable()
    {
        return None;
    }
    // The settings layers first, weakest of them first, and the managed file where none of them
    // said it: what is wanted is the file somebody can open, and a person told about their own
    // checkout's line has somewhere to go that a person told about `/etc` does not.
    settings
        .narrowed_by(bravebot_config::Narrowing::BYPASS_UNREACHABLE)
        .map(Path::to_path_buf)
        .or_else(|| managed.path().map(Path::to_path_buf))
}

/// Take `--settings <path>` out of the arguments, answering with the file it named.
///
/// Removed before dispatch for the reason `--incognito` and `--dangerously-skip-permissions` are:
/// the settings a run reads are a property of the run rather than of a task, so a session, a
/// resumed session and a one-shot all mean the same thing by it, and a flag read per subcommand
/// would be a flag silently ignored by whichever of them forgot it.
///
/// Given twice the last one is the file, which is how `--mode` and `--model` already resolve a
/// repeat. Refusing instead would be a rule about typing, and there is nothing to add: a second
/// file cannot be a fifth layer, since the flag names the layer above every other and two of those
/// is not an order anybody could read off the command line.
///
/// The arguments are rewritten only once the whole scan has succeeded, so a refusal leaves the list
/// that was typed rather than one this had half consumed.
fn take_settings(args: &mut Vec<String>) -> Result<Option<PathBuf>, String> {
    // A blank path is refused rather than read as no flag, on `--model`'s argument: a script
    // whose variable expanded to nothing asked for a settings file and would otherwise be run
    // under whatever the directory happened to carry, without being told.
    let named = take_valued(args, &["--settings"], |_, word| match word.map(str::trim) {
        Some(path) if !path.is_empty() => Ok(PathBuf::from(path)),
        _ => Err(t!(cli_settings_needs_a_path).to_string()),
    })?;
    Ok(named.into_iter().last())
}

/// Take `--run-network <open|closed>` out of the arguments, answering with the setting it named.
///
/// Removed before dispatch for the reason `--settings` is. A word that is neither is refused with
/// the two that are named, since a typo read as `open` would leave the network a person meant to
/// close open and say nothing. Given twice the last one wins, as `--settings` does. The arguments
/// are rewritten only once the whole scan has succeeded.
fn take_run_network(
    args: &mut Vec<String>,
) -> Result<Option<bravebot_sandbox::network::Network>, String> {
    let named = take_valued(args, &["--run-network"], |_, word| {
        let Some(word) = word else {
            return Err(t!(cli_run_network_needs_a_word).to_string());
        };
        bravebot_sandbox::network::Network::parse(word)
            .ok_or_else(|| t!(cli_run_network_unknown, word = word).to_string())
    })?;
    Ok(named.into_iter().last())
}

/// Take `--log-level <error|info|debug>` out of the arguments, answering with the level it named.
///
/// Removed before dispatch for the reason `--run-network` is, and refused on a missing or unknown
/// word for the same one: a typo read as the default would leave a person without the log they
/// asked for, discovered only when the failure they wanted it for had already happened.
fn take_log_level(args: &mut Vec<String>) -> Result<Option<bravebot_diag::Level>, String> {
    let named = take_valued(args, &["--log-level"], |_, word| {
        let Some(word) = word else {
            return Err(t!(cli_log_level_needs_a_word).to_string());
        };
        bravebot_diag::Level::parse(word)
            .ok_or_else(|| t!(cli_log_level_unknown, word = word).to_string())
    })?;
    Ok(named.into_iter().last())
}

/// Take `--sandbox <mode>` out of the arguments, answering with the mode it named.
///
/// Removed before dispatch for the reason `--settings` is: how far a program may reach is a
/// property of the run, and a flag read per subcommand would be one silently ignored by whichever
/// forgot it. Given twice the last one wins, as `--settings` does. A blank or unknown word is
/// refused rather than read as the default, because the modes differ in what a program may reach
/// and a script whose variable expanded to nothing asked for one of them.
fn take_sandbox(args: &mut Vec<String>) -> Result<Option<SandboxMode>, String> {
    let named = take_valued(args, &["--sandbox"], |_, word| {
        word.and_then(SandboxMode::parse).ok_or_else(|| {
            t!(
                cli_sandbox_needs_a_mode,
                names = SandboxMode::ALL.map(SandboxMode::name).join(", ")
            )
            .to_string()
        })
    })?;
    Ok(named.into_iter().last())
}

/// Whether the command line's first argument names a command that starts no program `run` would
/// confine, so that a refused mode does not stop it.
fn starts_no_program(first: Option<&str>) -> bool {
    matches!(
        first,
        Some(
            "doctor"
                | "bug-report"
                | "update"
                | "--help"
                | "-h"
                | "--version"
                | "-V"
                | "auth"
                | "permissions"
                | "sessions"
                | "attach"
                | "completion"
                | "import-leo-creds"
                | "import-providers"
        )
    )
}

/// What a refused mode says: the file that asked, when it was a file, and the file that holds the
/// floor.
fn sandbox_refusal(refused: &bravebot_config::sandbox::Refused) -> String {
    use bravebot_config::sandbox::Floor;
    let pinned_in = refused.pinned_in.display().to_string();
    match (&refused.asked_in, refused.because) {
        (Some(file), Floor::Mode) => t!(
            cli_sandbox_refused_file,
            asked = refused.asked.name(),
            asked_in = file.display().to_string(),
            pinned = refused.pinned.name(),
            pinned_in = pinned_in,
            looser = bravebot_agent::sandbox_reason::looser(refused)
        )
        .to_string(),
        (None, Floor::Mode) => t!(
            cli_sandbox_refused_flag,
            asked = refused.asked.name(),
            pinned = refused.pinned.name(),
            pinned_in = pinned_in,
            looser = bravebot_agent::sandbox_reason::looser(refused)
        )
        .to_string(),
        (Some(file), Floor::Network) => t!(
            cli_sandbox_refused_network_file,
            asked = refused.asked.name(),
            asked_in = file.display().to_string(),
            pinned_in = pinned_in
        )
        .to_string(),
        (None, Floor::Network) => t!(
            cli_sandbox_refused_network_flag,
            asked = refused.asked.name(),
            pinned_in = pinned_in
        )
        .to_string(),
    }
}

/// Take `--sandbox-allow-read`, `--sandbox-deny-read`, `--sandbox-allow-write` and
/// `--sandbox-deny-write` out of the arguments, each with the path that follows it, answering with
/// the lists they came to. Each may be given any number of times.
///
/// Removed before dispatch for the reason `--settings` is. A flag with nothing after it is refused,
/// since a path read as the next flag would be an entry nobody wrote. The arguments are rewritten
/// only once the whole scan has succeeded.
fn take_sandbox_filesystem(
    args: &mut Vec<String>,
) -> Result<bravebot_sandbox::rules::Lists, String> {
    use bravebot_sandbox::rules::{Entry, List};
    const FLAGS: [(&str, List); 4] = [
        ("--sandbox-allow-read", List::AllowRead),
        ("--sandbox-deny-read", List::DenyRead),
        ("--sandbox-allow-write", List::AllowWrite),
        ("--sandbox-deny-write", List::DenyWrite),
    ];
    let names = FLAGS.map(|(flag, _)| flag);
    let taken = take_valued(args, &names, |flag, word| {
        let Some(path) = word.filter(|path| !path.trim().is_empty()) else {
            return Err(t!(cli_sandbox_flag_needs_a_path, flag = flag).to_string());
        };
        let list = FLAGS
            .iter()
            .find(|(name, _)| *name == flag)
            .map(|(_, list)| *list);
        let entry = Entry {
            path: path.to_string(),
            by: None,
            pinned: false,
        };
        Ok((list, entry))
    })?;
    let mut lists = bravebot_sandbox::rules::Lists::default();
    for (list, entry) in taken {
        match list {
            Some(List::AllowRead) => lists.allow_read.push(entry),
            Some(List::DenyRead) => lists.deny_read.push(entry),
            Some(List::AllowWrite) => lists.allow_write.push(entry),
            Some(List::DenyWrite) => lists.deny_write.push(entry),
            None => {}
        }
    }
    Ok(lists)
}

/// Every tool name `--tools` may name: the ones a turn is offered, wherever a surface offers them,
/// and the `advisor` a session may add.
fn nameable_tools() -> Vec<String> {
    use bravebot_agent::tools::{Running, Scheduling, available};
    let mut names: Vec<String> = [Scheduling::ArrangingALook, Scheduling::PacingALoop]
        .into_iter()
        .flat_map(|scheduling| {
            available(
                scheduling,
                bravebot_agent::watch::Arming::Allowed { free: 1 },
                bravebot_agent::exec::Deadlines::BUILT_IN,
                Running::Offered,
            )
        })
        .map(|tool| tool.function.name)
        .collect();
    names.push("advisor".to_string());
    names.sort();
    names.dedup();
    names
}

/// Take `--tools <a,b,c>` and `--no-shell` out of the arguments, answering with what they limit.
///
/// Removed before dispatch for the reason `--incognito` is: every way of starting builds the same
/// list of tools. A name that is no tool is refused with the ones that are, since a typo read as
/// "offer nothing of that" would leave a run holding a smaller set than its author thought and say
/// nothing, and a list that names nothing is refused for the reason a blank `--model` is. Given
/// twice, `--tools` takes the last list. The arguments are rewritten only once the scan has
/// succeeded.
fn take_tool_limit(args: &mut Vec<String>) -> Result<bravebot_core::tool_set::Limit, String> {
    let lists = take_valued(args, &["--tools"], |_, word| {
        let Some(list) = word.filter(|list| !list.trim().is_empty()) else {
            return Err(t!(cli_tools_needs_a_list).to_string());
        };
        let known = nameable_tools();
        let mut named = Vec::new();
        for name in list.split(',').map(str::trim) {
            if !known.iter().any(|tool| tool == name) {
                return Err(
                    t!(cli_tools_unknown, name = name, known = known.join(", ")).to_string()
                );
            }
            named.push(name.to_string());
        }
        Ok(named)
    })?;
    // After the list, so that `--tools --no-shell` still names `--no-shell` as the unknown tool it
    // was taken for, and not as a list that is missing.
    let no_shell = take_flag(args, "--no-shell");
    Ok(bravebot_core::tool_set::Limit {
        no_shell,
        only: lists.into_iter().last(),
    })
}

/// Why the tool limit cannot go with the command line's first argument, or `None` where it can.
///
/// The commands that start no session and no task offer no tools, so the flags would be taken and do
/// nothing. Refused rather than ignored, for the reason CLI-13 gives about a settings file.
fn without_a_tool_set_to_limit(first: &str) -> Option<String> {
    // CLI-26 lists the commands without `bug-report`, where CLI-19 names it, so it is left out here.
    starts_neither_a_session_nor_a_task(first)
        .filter(|command| *command != "bug-report")
        .map(|command| t!(cli_tools_not_for_a_command, command = command).to_string())
}

/// What a one-shot invocation asked for, before anything runs.
#[derive(Debug)]
struct Invocation {
    prompt: String,
    files: Vec<String>,
    /// The past sessions of this directory named with `--session-ref`, by id (NAME-10).
    session_refs: Vec<String>,
    mode: Mode,
    /// The model the command line named. `None` leaves the configured one in force rather than
    /// standing for a model of its own.
    model: Option<String>,
    /// The model the command line named as the planner's advisor. `None` leaves it to the
    /// `advisorModel` setting.
    advisor: Option<String>,
    /// The level the command line named, which outranks the saved pick and every settings file for
    /// this run alone. `None` leaves those to answer.
    effort: Option<bravebot_session::store::Effort>,
    /// Directories outside the working one that this run may reach into.
    directories: Vec<String>,
    /// Whether the person typed `--trust-workspace`, which gives this run the map a yes to the
    /// startup question writes (TRUST-26).
    trust_workspace: bool,
    trace: bool,
    print: bool,
    /// Whether stdout carries the result object rather than the prose reply.
    json: bool,
    /// Whether stdout also carries an event line for each call, refusal and request as it
    /// finishes, ahead of the result object (CLI-24).
    stream: bool,
    /// The earlier session this run carries on (CLI-25).
    resume: Option<continued::Resume>,
    /// The file holding the JSON Schema the reply must match (CLI-28).
    output_schema: Option<String>,
}

/// The name after `--model`, for a one-shot run and for the session `-i` opens.
///
/// A blank name is refused rather than read as no choice. A stored one is allowed to be blank, where
/// it means the file holds nothing and the configured model answers, but a script that computed an
/// empty variable asked for a model and would otherwise be given whatever was configured without
/// being told.
fn model_name(next: Option<&String>) -> Result<String, String> {
    match next.map(|name| name.trim()) {
        Some(name) if !name.is_empty() => Ok(name.to_string()),
        _ => Err(t!(cli_model_needs_a_name).to_string()),
    }
}

/// The level after `--effort`, for a one-shot run and for the session `-i` opens.
///
/// Refused unless it is a level, for the reason a blank `--model` is, and for a stronger one: a model
/// name the service does not know is substituted and reported, where a word that is no level would be
/// dropped without a word and the run sent at another level.
fn effort_level(next: Option<&String>) -> Result<bravebot_session::store::Effort, String> {
    match next.and_then(|word| bravebot_session::store::Effort::named(word)) {
        Some(level) => Ok(level),
        None => {
            let levels: Vec<&str> = bravebot_session::store::Effort::ALL
                .iter()
                .map(|level| level.as_str())
                .collect();
            Err(t!(cli_effort_needs_a_level, levels = levels.join(", ")))
        }
    }
}

/// Parse `<prompt> [--file path]... [--add-dir path]... [--mode name] [--model name]
/// [--effort level] [--trace] [--json] [-p]`.
fn parse_invocation(args: &[String]) -> Result<Invocation, String> {
    let mut prompt = String::new();
    let mut files = Vec::new();
    let mut session_refs: Vec<String> = Vec::new();
    let mut mode = Mode::default();
    let mut model = None;
    let mut advisor = None;
    let mut effort = None;
    let mut directories = Vec::new();
    let mut trust_workspace = false;
    let mut trace = false;
    let mut print = false;
    let mut json = false;
    let mut stream = false;
    let mut resume = None;
    let mut output_schema = None;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--mode" => match args.get(index + 1).map(|name| name.parse::<Mode>()) {
                Some(Ok(chosen)) => {
                    mode = chosen;
                    index += 2;
                }
                Some(Err(complaint)) => return Err(complaint),
                None => {
                    return Err(t!(cli_mode_needs_a_name, names = Mode::NAMES.join(", ")));
                }
            },
            "--model" => {
                model = Some(model_name(args.get(index + 1))?);
                index += 2;
            }
            // Refused when blank for the reason `--model` is: a script that computed an empty
            // variable asked for an advisor and would otherwise run without one, untold.
            "--advisor" => match args.get(index + 1).map(|name| name.trim()) {
                Some(name) if !name.is_empty() => {
                    advisor = Some(name.to_string());
                    index += 2;
                }
                _ => return Err(t!(cli_advisor_needs_a_name).to_string()),
            },
            "--effort" => {
                effort = Some(effort_level(args.get(index + 1))?);
                index += 2;
            }
            "--file" => match args.get(index + 1) {
                Some(path) => {
                    files.push(path.clone());
                    index += 2;
                }
                None => return Err(t!(cli_file_needs_a_path).to_string()),
            },
            "--session-ref" => match args.get(index + 1).map(|id| id.trim()) {
                Some(id) if !id.is_empty() => {
                    // Naming one twice is naming it once, as on a line with the mention twice.
                    if !session_refs.iter().any(|seen| seen == id) {
                        session_refs.push(id.to_string());
                    }
                    index += 2;
                }
                _ => return Err(t!(cli_session_ref_needs_an_id).to_string()),
            },
            // Repeatable, since reaching one sibling checkout is no more natural than reaching
            // two, and a flag that could only be given once would be a rule about typing.
            "--add-dir" => match args.get(index + 1).map(|path| path.trim()) {
                Some(path) if !path.is_empty() => {
                    directories.push(path.to_string());
                    index += 2;
                }
                _ => return Err(t!(cli_add_dir_needs_a_path).to_string()),
            },
            // Repeats are one flag, as with `--incognito`: asking twice for what is already so is
            // no mistake worth refusing a run over.
            "--trust-workspace" => {
                trust_workspace = true;
                index += 1;
            }
            "--trace" => {
                trace = true;
                index += 1;
            }
            // A task carrying on an earlier session. The id is refused when it is missing or is
            // the next flag, for the reason a blank `--model` is: the run would otherwise look for
            // a session called `-p`, or carry on none.
            "--resume" | "-r" => match args.get(index + 1) {
                Some(id) if !id.trim().is_empty() && !id.starts_with('-') => {
                    resume = Some(continued::Resume::Id(id.clone()));
                    index += 2;
                }
                _ => return Err(t!(cli_resume_needs_an_id).to_string()),
            },
            "--continue" | "-c" => {
                resume = Some(continued::Resume::Latest);
                index += 1;
            }
            // Refused when blank for the reason `--model` is: a script that computed an empty
            // variable asked for a shape and would otherwise get free prose, untold.
            "--output-schema" => match args.get(index + 1).map(|path| path.trim()) {
                Some(path) if !path.is_empty() => {
                    output_schema = Some(path.to_string());
                    index += 2;
                }
                _ => return Err(t!(cli_output_schema_needs_a_path).to_string()),
            },
            "--json" => {
                json = true;
                index += 1;
            }
            // The result object is still the last line, so the stream is a form of `--json`.
            "--json-stream" => {
                json = true;
                stream = true;
                index += 1;
            }
            "-p" | "--print" => {
                print = true;
                index += 1;
            }
            // Reached only behind a task flag, since a leading one opens a session instead. Taken as
            // the prompt, `bravebot --model opus -i` would run the task `-i` with nobody watching.
            "-i" | "--interactive" => return Err(t!(cli_interactive_not_with_a_task).to_string()),
            other if prompt.is_empty() => {
                prompt = other.to_string();
                index += 1;
            }
            other => return Err(t!(cli_unexpected_argument, argument = other)),
        }
    }

    Ok(Invocation {
        prompt,
        files,
        session_refs,
        mode,
        model,
        advisor,
        effort,
        directories,
        trust_workspace,
        trace,
        print,
        json,
        stream,
        resume,
        output_schema,
    })
}

/// The first of the flags a manifest run cannot take that this invocation gave, as the sentence that
/// refuses it, in the order the checks run: agent, advisor, output schema, tools, system prompts,
/// resume. `None` for any other mode.
fn manifest_conflict(
    mode: Mode,
    has_agent: bool,
    has_advisor: bool,
    has_output_schema: bool,
    prompts: &SystemPrompts,
    resume: Option<&continued::Resume>,
) -> Option<String> {
    if mode != Mode::Manifest {
        return None;
    }
    // A manifest run's plan is written by the planner before any step runs, and an addressed
    // definition is matched a turn at a time by the turn's own kernel, so the name would be taken
    // and nothing would be addressed to it.
    if has_agent {
        return Some(t!(cli_agent_not_with_a_manifest).to_string());
    }
    // A manifest run's steps are run from the plan, not chosen by a planner that could ask, so an
    // advisor would be named and never consulted.
    if has_advisor {
        return Some(t!(cli_advisor_not_with_a_manifest).to_string());
    }
    // A manifest run has a reply per step and none for the run, so there is no one reply to hold to
    // a schema.
    if has_output_schema {
        return Some(t!(cli_output_schema_not_with_a_manifest).to_string());
    }
    // A manifest run's steps are planned and run from the plan, not chosen from a list of tools, so
    // the flags would be taken and limit nothing.
    if !bravebot_core::tool_set::settled().is_none() {
        return Some(t!(cli_tools_not_with_a_manifest).to_string());
    }
    // Neither reaches the planner of a manifest run, which is given no standing instructions from
    // the command line, and words a run would drop are refused instead (CLI-19).
    if let Some(flag) = flag_named(prompts) {
        return Some(t!(cli_system_prompt_not_with_a_manifest, flag = flag));
    }
    // A manifest run has no conversation to carry a recorded one into.
    resume.map(|asked| {
        let flag = match asked {
            continued::Resume::Id(_) => "--resume",
            continued::Resume::Latest => "--continue",
        };
        t!(cli_resume_not_with_a_manifest, flag = flag)
    })
}

/// Whether a one-shot puts its plan to somebody: only where both ends of the conversation are a
/// terminal, since either one piped or redirected is nobody (CLI-8).
fn a_plan_is_put(stdin_is_terminal: bool, stderr_is_terminal: bool) -> bool {
    stdin_is_terminal && stderr_is_terminal
}

fn run_task(
    args: &[String],
    skip_permissions: bool,
    agent: Option<String>,
    prompts: SystemPrompts,
) -> ExitCode {
    match prepare_the_run(args, skip_permissions, agent, prompts) {
        Ok(prepared) => run_and_report(prepared),
        Err(stopped) => stopped,
    }
}

/// What the second half of a one-shot run reads, and the two guards that only have to live as long
/// as the run does. The fields are declared in the order the run used to declare the locals they
/// replace, and `run_and_report` binds them in that order, so they are dropped in the order they
/// always were.
struct Prepared {
    workspace: bravebot_agent::Workspace,
    continued: Option<bravebot_session::sessions::Record>,
    agent: Option<String>,
    /// Held because dropping it removes the run's scratch directory.
    _scratch: Option<bravebot_agent::SessionScratch>,
    egress: bravebot_net::Egress,
    task: Task,
    /// Held because dropping it stops the servers the task reaches.
    _reached: servers::Reached,
    config: Config,
    under: Option<String>,
    model: String,
    mode: Mode,
    trace: bool,
    as_json: bool,
    as_stream: bool,
    trust_workspace: bool,
    named_on_the_command_line: bool,
    permission_mode: bravebot_agent::PermissionMode,
}

/// Everything a one-shot run settles before the turn: the arguments, the configuration, the
/// workspace and the session it continues, the definition and the model, and the task. Each refusal
/// is printed here, so the code it returns is the one the run exits with.
fn prepare_the_run(
    args: &[String],
    skip_permissions: bool,
    agent: Option<String>,
    prompts: SystemPrompts,
) -> Result<Prepared, ExitCode> {
    let invocation = match parse_invocation(args) {
        Ok(invocation) => invocation,
        // Whether a result object was asked for is read off the raw arguments here, because the
        // parse that would otherwise have said is the thing that just failed, and a caller that
        // asked for one asked for this one too.
        Err(err) => {
            return Err(stopped_before_the_turn(
                wants_json(args),
                Ending::Argument,
                err,
            ));
        }
    };
    let Invocation {
        prompt,
        files,
        session_refs,
        mode,
        model,
        advisor,
        effort,
        directories,
        trust_workspace,
        trace,
        print,
        json: as_json,
        stream: as_stream,
        resume,
        output_schema,
    } = invocation;

    // Read before the emptiness check below, since `cat notes.md | bravebot -p` is a complete
    // invocation: the pipe is the input and the prompt may be left off.
    let piped = if print {
        let stdin = std::io::stdin();
        let is_tty = stdin.is_terminal();
        match piped_input(stdin.lock(), is_tty) {
            Ok(text) => text,
            Err(err) => return Err(stopped_before_the_turn(as_json, Ending::Argument, err)),
        }
    } else {
        None
    };

    if prompt.is_empty() && piped.is_none() {
        return Err(stopped_before_the_turn(
            as_json,
            Ending::Argument,
            t!(cli_task_required),
        ));
    }

    if let Some(conflict) = manifest_conflict(
        mode,
        agent.is_some(),
        advisor.is_some(),
        output_schema.is_some(),
        &prompts,
        resume.as_ref(),
    ) {
        return Err(stopped_before_the_turn(as_json, Ending::Argument, conflict));
    }
    // Read and vetted before anything is sent, so a schema outside what is checked is a refused
    // argument and not a constraint the run quietly did not hold the reply to.
    let output_schema = match output_schema.as_deref().map(load_output_schema).transpose() {
        Ok(schema) => schema,
        Err(complaint) => {
            return Err(stopped_before_the_turn(
                as_json,
                Ending::Argument,
                complaint,
            ));
        }
    };

    let mut config = match Config::from_env() {
        Ok(c) => c,
        Err(err) => {
            return Err(stopped_before_the_turn(
                as_json,
                Ending::Configuration,
                t!(cli_configuration_problem, problem = err),
            ));
        }
    };

    // Before anything runs, because nothing here is configured to serve a turn yet: what a person
    // would conclude from whatever came back is that the agent is poor rather than that nothing has
    // been set up.
    //
    // Asked of the model this run will actually request, which is what decides where the request
    // goes. The flag it may have been named by is resolved below; what is read here is the same
    // answer without it, since a `--model` naming a configured service's model is exactly the case
    // this must not refuse.
    //
    // A run under a definition and no `--model` is asked below instead, once the definition is
    // matched, because the model it asks for is the definition's where the definition names one.
    // A continued session may be recorded as having worked under one, which is not known until its
    // record is read.
    let settings = bravebot_config::Settings::load();
    // The `agent` setting stands in for `--agent` where there is none, except in a manifest run,
    // which no definition addresses (ADDRESS-13).
    let configured = settings.agent().filter(|_| mode != Mode::Manifest);
    let asked_below =
        (agent.is_some() || configured.is_some() || resume.is_some()) && model.is_none();
    if !asked_below
        && let Some(how) = nothing_serves(&config, &model_for_this_run(model.as_deref(), &config))
    {
        return Err(stopped_before_the_turn(as_json, Ending::Configuration, how));
    }

    // Resolved like `--model`, and refused like it where the machine-level layer refuses the
    // name, so a run never starts with an advisor its first question could not reach (BACKEND-48).
    let advisor = match advisor.map(|name| resolve_advisor(&config, &name)) {
        Some(Ok(advisor)) => Some(advisor),
        Some(Err(how)) => return Err(stopped_before_the_turn(as_json, Ending::Configuration, how)),
        None => None,
    };

    let mut workspace = match current_workspace(&settings, &Managed::load()) {
        Ok(w) => w,
        Err(err) => {
            return Err(stopped_before_the_turn(
                as_json,
                Ending::Failed,
                t!(cli_workspace_problem, problem = err),
            ));
        }
    };

    // Fatal rather than said and carried on with. A session leaves the person to retype it; a
    // script that asked to reach a directory and did not gets a turn that fails somewhere further
    // in, over a file it was told it could open.
    match open_directories(&mut workspace, &directories) {
        Ok(notices) => {
            for notice in notices {
                eprintln!("{}", t!(cli_notice, notice = notice));
            }
        }
        Err(problem) => return Err(stopped_before_the_turn(as_json, Ending::Argument, problem)),
    }

    // Found before anything is sent, so a session that is not there exits with the status for an
    // argument and costs nothing (CLI-25).
    let continued = match resume
        .as_ref()
        .map(|asked| continued::find(workspace.root(), asked))
        .transpose()
    {
        Ok(record) => record,
        Err(why) => return Err(stopped_before_the_turn(as_json, Ending::Argument, why)),
    };

    // The rules the settings file carried, read before the definition is matched so the match
    // leaves out a definition they deny reading, as the turn does. Anything unreadable is named on
    // stderr below, beside the rest of what this run has to say about itself.
    let (permissions, rejected) = rules_for_a_one_shot_run(
        &settings,
        bravebot_agent::home::profile().as_deref(),
        workspace.root(),
        skip_permissions,
    );

    // Matched here, before any server is reached, so a name matching nothing sends nothing and the
    // refusal lists the names that exist (ADDRESS-5). The turn's kernel matches it again and makes
    // the decision. This match is what makes a miss exit with the argument status.
    //
    // A continued session works under the definition its record names unless this command line
    // named one. The recorded name is the person's own earlier `--agent`, so a name that matches
    // nothing now is said on stderr and the run goes on without it, where a name typed here would
    // end the run (ADDRESS-3).
    let recorded = continued
        .as_ref()
        .and_then(|record| record.agent.as_deref());
    let (agent, under) =
        match bravebot_tui::app::settle_definition(agent.as_deref(), recorded, configured, |name| {
            definition_for_a_run(&config, &workspace, &permissions, name, model.is_some())
        }) {
            Ok(bravebot_tui::app::Settled::Under {
                name,
                found,
                by_setting,
            }) => {
                if by_setting {
                    eprintln!("{}", t!(session_working_under_by_setting));
                }
                (Some(name), found)
            }
            Ok(bravebot_tui::app::Settled::SettingGone { name, why }) => {
                eprintln!("{}", t!(cli_agent_setting_gone, definition = name.as_str()));
                eprintln!("{why}");
                (None, None)
            }
            Ok(bravebot_tui::app::Settled::Gone { name, why }) => {
                eprintln!(
                    "{}",
                    t!(session_recorded_definition_gone, definition = name.as_str())
                );
                eprintln!("{why}");
                (None, None)
            }
            Ok(bravebot_tui::app::Settled::Nothing) => (None, None),
            Err(refused) => {
                return Err(stopped_before_the_turn(as_json, Ending::Argument, refused));
            }
        };
    if asked_below
        && let Some(how) = nothing_serves(
            &config,
            &under
                .clone()
                .unwrap_or_else(|| model_for_this_run(None, &config)),
        )
    {
        return Err(stopped_before_the_turn(as_json, Ending::Configuration, how));
    }

    // A run is a session for this: it is given somewhere of its own to write what is not part of
    // the project, and the directory goes when the run does. Held in a binding for exactly that
    // reason, since dropping it is what removes it.
    let _scratch = scratch_for_this_run(&mut workspace);

    let egress = bravebot_net::Egress::new();

    for problem in &rejected {
        eprintln!(
            "{}",
            t!(
                session_permission_rule_ignored,
                problem = bravebot_agent::permissions::describe(problem)
            )
        );
    }

    // Bypassing where the flag was given, and asking otherwise. There is no mode key here: a
    // one-shot run has no session to hold a mode and nobody to press anything, so the command line
    // is the whole of what can say.
    let permission_mode = match skip_permissions {
        true => bravebot_agent::PermissionMode::Bypass,
        false => bravebot_agent::PermissionMode::Ask,
    };
    // Resolved against the configuration rather than at parse, since a tier word names a model
    // only the configuration knows: the AWS account's ARN for that tier where it named one, and
    // Brave's name for it otherwise. The settings key this flag outranks accepts those words, so a
    // flag that did not would refuse a spelling the file it overrides takes.
    let named = model.map(|name| config.model_named(&name));
    // Held onto because it is what separates a substitution worth reporting from one worth failing
    // the run over: below the flag the model is whatever was recorded or configured, and a script
    // that never named one did not ask for what it did not get.
    let named_on_the_command_line = named.is_some();
    let pick = match named_on_the_command_line {
        true => bravebot_agent::backend::Pick::Absent,
        false => bravebot_agent::backend::pick(
            &config,
            bravebot_session::store::model(bravebot_session::store::load_model(), &settings),
        ),
    };
    match &pick {
        bravebot_agent::backend::Pick::SetAside(model) => eprintln!(
            "{}",
            t!(session_model_pick_set_aside, model = model.as_str())
        ),
        bravebot_agent::backend::Pick::Refused { recorded, reason } => eprintln!(
            "{}",
            t!(
                session_model_pick_refused,
                model = recorded.as_str(),
                reason = reason.as_str()
            )
        ),
        bravebot_agent::backend::Pick::Absent | bravebot_agent::backend::Pick::InForce(_) => {}
    }
    // Read before anything is sent, so an id that names nothing quotable is a refused argument and
    // not a run that quietly went without the session it was told to carry (NAME-10).
    let mut excerpts = Vec::new();
    for id in &session_refs {
        match bravebot_session::excerpt::of(workspace.root(), id) {
            Ok(excerpt) => excerpts.push(excerpt),
            Err(refused) => {
                return Err(stopped_before_the_turn(
                    as_json,
                    Ending::Argument,
                    refused.said(id),
                ));
            }
        }
    }
    let mut task = Task::new(prompt)
        .with_home(bravebot_agent::home::directory())
        .with_profile(bravebot_agent::home::profile())
        .with_cache(bravebot_agent::home::cache())
        .with_model(model_asked_for(named, pick.into_model()))
        .with_advisor(advisor)
        .with_output_schema(output_schema)
        .with_permissions(permissions)
        .with_permission_mode(permission_mode)
        // The words the command line put in the planner's system prompt (CLI-19). A manifest run
        // was refused above, so this task is a turn's.
        .with_system_prompts(prompts)
        .addressing(agent.clone())
        .model_outranks_a_definition(named_on_the_command_line);
    task = task_from_the_settings(task, &settings, effort);
    for file in files {
        task = task.with_file(file);
    }
    for excerpt in excerpts {
        task = task.with_session_excerpt(excerpt.id, excerpt.text);
    }
    if let Some(text) = piped {
        task = task.with_piped_input(text);
    }

    // Nobody is asked here, so a server no answer of the person's covers is absent and the reason
    // goes to stderr beside the rest of what this run says about itself (SERVERS-4). Held until the
    // run ends, since dropping one stops its server.
    let reached = servers::for_this_session(
        &settings,
        workspace.root(),
        match skip_permissions {
            true => servers::Asking::Bypass,
            false => servers::Asking::OneShot,
        },
        &mut servers::nobody(),
        bravebot_sandbox::Stream::Inherited,
    );
    for note in &reached.notes {
        eprintln!("{}", t!(cli_notice, notice = note));
    }
    task = task.with_mcp(reached.session());

    // On stderr, beside the progress lines, so a pipe of the reply is unaffected. Said even here,
    // where nobody may be reading: a run that wrote to the tree without asking should leave a record
    // of having been told not to ask.
    if skip_permissions {
        eprintln!(
            "{}",
            t!(cli_notice, notice = t!(session_permissions_skipped))
        );
    }

    // Before the turn, so the sign-in's own output is not interleaved with progress lines and a
    // browser opening is accounted for. Nothing happens where no sign-in is wanted, which includes
    // every run whose model is served by Brave.
    //
    // Not fatal: the turn goes ahead and fails with the backend's own account of what is wrong,
    // which says more than this could guess.
    //
    let model =
        check_the_model_can_serve_the_run(&mut config, &mut task, under.as_deref(), as_json)?;

    Ok(Prepared {
        workspace,
        continued,
        agent,
        _scratch,
        egress,
        task,
        _reached: reached,
        config,
        under,
        model,
        mode,
        trace,
        as_json,
        as_stream,
        trust_workspace,
        named_on_the_command_line,
        permission_mode,
    })
}

/// Which model the run asks for, and whether the endpoint can serve it as asked: its window, the
/// output-schema check (CLI-28), the effort level (BACKEND-22) and the sign-in. Returns the model
/// in force, and the refusal's status where there is one.
fn check_the_model_can_serve_the_run(
    config: &mut Config,
    task: &mut Task,
    under: Option<&str>,
    as_json: bool,
) -> Result<String, ExitCode> {
    // The definition's model where it names one the command line did not outrank, since that is
    // the one the turn will ask for, and every question below is about what is asked for.
    let model = under
        .map(str::to_string)
        .or_else(|| task.model.clone())
        .unwrap_or_else(|| config.default_model.clone());

    // The window the endpoint advertises for that model, which is what compaction measures a
    // conversation against. A run opens no picker and holds no session, so this is the only place
    // it can be looked up, and the model is in force here however it got there: named on the
    // command line, read back off disk, or pinned in the settings file. Without it a run compacts
    // against a default that a narrow window never reaches, so compaction never fires, while a wide
    // one reaches it with three quarters of the conversation still to spare.
    //
    // The same listing says whether that model reads an effort level, which is the other thing a
    // run cannot learn anywhere else.
    let reads_effort = bravebot_tui::app::adopt_listing_for_model(config, &model);

    // A model that cannot be asked for a reply of a given shape refuses the flag before anything
    // is sent, rather than answering in prose that the check afterwards fails (CLI-28). Bedrock has
    // no field for it here, and a roster row that states its parameters without one says the
    // service would ignore it.
    if task.output_schema.is_some()
        && (config.bedrock_for(&model).is_some()
            || !bravebot_tui::app::reads_structured_output(config, &model))
    {
        return Err(stopped_before_the_turn(
            as_json,
            Ending::Argument,
            t!(cli_output_schema_not_served, model = model.as_str()),
        ));
    }

    // A level goes out only where the listing describing the model in force says it is read
    // (BACKEND-22). What is recorded stays recorded: the choice applies again the moment a model
    // that reads one is in force, so what a request carries does not depend on the order two
    // commands were typed in.
    //
    // Said, because a level that is charged for and discarded at the far end is indistinguishable
    // from one that was honoured, so silence here would leave somebody believing every turn of the
    // run thought harder than it did. Said only where one was chosen: a run that asked for no level
    // has had nothing withheld from it.
    if !reads_effort && task.effort.is_some() {
        eprintln!("{}", t!(cli_notice, notice = t!(session_effort_not_read)));
        task.effort = None;
    }

    // The sign-in's own lines go to stderr as they arrive, beside every other progress line, which
    // keeps stdout the reply and nothing else. A URL and a code are no use after the fact, so they
    // are printed while the command that wrote them is still waiting.
    let signed_in = bravebot_agent::backend::Backend::sign_in_if_needed(config, &model, |line| {
        eprintln!("{line}");
    });
    if let Err(failure) = signed_in {
        eprintln!("{}", t!(cli_notice, notice = failure.to_string()));
    }

    Ok(model)
}

/// The turn or the manifest, and what is reported of how it ended.
fn run_and_report(prepared: Prepared) -> ExitCode {
    let Prepared {
        workspace,
        continued,
        agent,
        _scratch,
        egress,
        task,
        _reached,
        config,
        under,
        model,
        mode,
        trace,
        as_json,
        as_stream,
        trust_workspace,
        named_on_the_command_line,
        permission_mode,
    } = prepared;
    let mut sink = json::Streaming::new(as_stream.then(|| json::Stream::new(std::io::stdout())));

    // A one-shot run has nobody to ask about a write, so writes are refused rather than silently
    // applied. The one exception is a plan, which is put before the first step rather than in the
    // middle of a run, and only where both ends of the conversation are a terminal: a plan written
    // into a redirected stderr is a plan nobody read, and a pipe means the answer would be read
    // from whatever fed it.
    //
    // Unless the flag was given, which is the one way a run nobody is watching may write: the
    // person accepted that when they typed it, and this is the only path where the refusal above
    // is what stands between the flag and an effect.
    let attended = a_plan_is_put(
        std::io::stdin().is_terminal(),
        std::io::stderr().is_terminal(),
    );
    let mut one_shot = OneShot::new(std::io::stdin(), std::io::stderr(), attended);
    // Screening off the task, so the answer the tools fill a verdict in under and the answer that
    // decides whether a verdict which objects can refuse are the one value resolved above.
    let mut confirmer =
        bravebot_agent::Confining::new(&mut one_shot, permission_mode, task.auto_vetting);

    // Progress goes to stderr so stdout stays the reply and nothing else, which is what makes
    // the command pipeable. Without it a long turn prints nothing until it is over.
    let mut reporter = progress::Progress::new(std::io::stderr())
        .streaming(as_stream.then(|| json::Stream::new(std::io::stdout())));

    // The id of the record this run wrote, which the result object carries (CLI-25).
    let mut session: Option<String> = None;

    // Both modes take the same arguments and return the same outcome. The whole of the
    // difference is inside: one asks the model what to do next after every result, the other
    // asked once, before there were any.
    let outcome = match mode {
        Mode::Turn => {
            let (outcome, kept) = run_turn(
                TurnRun {
                    config: &config,
                    egress: &egress,
                    workspace: &workspace,
                    task: &task,
                    trust_workspace,
                    agent: agent.as_deref(),
                },
                continued,
                &mut confirmer,
                &mut reporter,
                &mut sink,
            );
            session = kept;
            outcome
        }
        Mode::Manifest => bravebot_agent::manifest::run(
            &config,
            &egress,
            &workspace,
            &task,
            &mut confirmer,
            &mut reporter,
            &mut sink,
            unattended_trust(workspace.root(), trust_workspace),
            &Cancel::new(),
        ),
    };

    // A manifest run is written down like any other session. It cannot be resumed, and the
    // picker says so, but "cannot be continued" is a different thing from "leaves no trace":
    // the run somebody needs to read is the one that stopped, and until now it left nothing.
    if mode == Mode::Manifest {
        bravebot_session::sessions::record_manifest_run(
            workspace.root(),
            &task.prompt,
            &outcome,
            Some(reporter.spent()),
            // The same surface the full-screen interface records: one binary, one terminal, and a
            // run started from a session written down the same way as one started from here.
            bravebot_session::sessions::Front::Terminal,
            bravebot_stamp::BUILD,
        );
    }

    match outcome {
        Ok(outcome) => {
            // The reply is untrusted model output. Printing it is safe, since the
            // terminal is not a decision, so it is released explicitly for display.
            // A traced manifest run puts the plan on stderr with the trail, never on
            // stdout: stdout stays the reply so a pipe is still a pipe.
            let attempt = if trace {
                outcome
                    .attempt
                    .as_ref()
                    .map(bravebot_agent::manifest::Attempt::describe)
            } else {
                None
            };
            // What was asked for against what answered. A model this run cannot be served is
            // substituted rather than refused: one that needs a subscription comes back answered
            // by a weaker model, with a 200 and an ordinary reply, so the name the server reports
            // is the only trace of it. Which name the service was actually asked for, and whether
            // it reports that name at all, are the backend's questions and are put to it here,
            // where the configuration is in hand.
            //
            // Against the model in force rather than the flag alone. A model pinned in the
            // settings file is the one a repository commits beside its scripts, so a substitution
            // of that is the case the reporting is for, and a session is told about it whatever
            // named the model.
            let asked = bravebot_agent::backend::Backend::name_as_asked(&config, &model);
            let comparable = bravebot_agent::backend::Backend::reports_the_model_it_was_asked_for(
                &config, &model,
            );
            // Skipped where the definition's model was asked for, because the turn compared that
            // model itself and reported the substitution in the definition's words. The same for a
            // turn a skill moved onto its own model (SKILL-15): the command line did not name that
            // model, so it does not fail the run.
            let not_served = (under.is_none() && outcome.ran_on_the_sessions_model())
                .then(|| model_not_served(&asked, comparable, &outcome.model))
                .flatten();

            let ending = ending_of_a_turn(
                outcome.clean,
                named_on_the_command_line,
                not_served.is_some(),
            );

            // Held to the schema only once the turn is otherwise a success: a run that failed for
            // another reason keeps that reason, and a reply that was not asked for in this shape
            // has nothing to be held to (CLI-28).
            let (ending, structured, off_schema) = match (&task.output_schema, ending) {
                (Some(schema), Ending::Done) => match schema.check(outcome.reply_for_display()) {
                    Ok(value) => (ending, Some(value), None),
                    Err(broke) => (Ending::Schema, None, Some(off_schema_message(&broke))),
                },
                _ => (ending, None, None),
            };

            // Built before the reply is chosen, because with `--json` the result object is what
            // goes on stdout in the reply's place and it has to hold the reply itself.
            let rendered = as_json.then(|| {
                json::render(&json::Report {
                    ending,
                    message: off_schema
                        .as_deref()
                        .or_else(|| failure_of_a_turn(ending, not_served.as_deref())),
                    reply: outcome.reply_for_display(),
                    model: &outcome.model,
                    agent: outcome.addressed.as_ref().map(|addressed| addressed.name()),
                    session: session.as_deref(),
                    steps: outcome.steps,
                    tokens: json::Tokens {
                        total: outcome.tokens,
                        output: outcome.output_tokens,
                        context: outcome.context_tokens,
                        cache_read: outcome.cached.read_tokens,
                        cache_written: outcome.cached.written_tokens,
                    },
                    calls: reporter.calls(),
                    refusals: &refusals(sink.recorded()),
                    notices: &outcome.notices,
                    structured: structured.as_deref(),
                })
            });

            let finished = Finished {
                reply: rendered.as_deref().unwrap_or(outcome.reply_for_display()),
                notices: &outcome.notices,
                attempt: attempt.as_deref(),
                trail: trace.then_some((sink.recorded(), outcome.model.as_str())),
                clean: outcome.clean,
                ending,
                not_served: not_served.as_deref(),
            };
            // Nothing of a reply that is off its schema goes where a pipe would take it for the
            // shape it asked for. With `--json` the object holds it, beside the status that says so.
            match (&off_schema, as_json) {
                (Some(_), false) => report(
                    &mut std::io::sink(),
                    &mut std::io::stderr().lock(),
                    &finished,
                ),
                _ => report(
                    &mut std::io::stdout().lock(),
                    &mut std::io::stderr().lock(),
                    &finished,
                ),
            }
            if let Some(complaint) = &off_schema {
                eprintln!("{}", ending.told(complaint));
            }
            ending.code()
        }
        // A run that stopped is the one worth looking at, so what it produced is printed
        // whether or not --trace was asked for. Without it a failed plan is a one-line
        // complaint about a document nobody can see.
        Err(bravebot_agent::TurnError::Manifest { attempt, cause }) => {
            report_a_failed_turn(&cause, Some(&attempt), trace, as_json, &reporter, &sink)
        }
        Err(err) => report_a_failed_turn(&err, None, trace, as_json, &reporter, &sink),
    }
}

/// What `run_turn` reads of the run around it, and none of what it changes.
struct TurnRun<'a> {
    config: &'a Config,
    egress: &'a bravebot_net::Egress,
    workspace: &'a bravebot_agent::Workspace,
    task: &'a Task,
    trust_workspace: bool,
    agent: Option<&'a str>,
}

/// The `Mode::Turn` arm of a one-shot run: the turn, continued from `continued` where there is one,
/// and the id of the record it wrote (CLI-25). `continued` is taken by value so the record can be
/// `take()`n and handed to `continued::keep`.
fn run_turn<C: bravebot_agent::Confirmer + Send>(
    run: TurnRun<'_>,
    mut continued: Option<bravebot_session::sessions::Record>,
    confirmer: &mut C,
    reporter: &mut progress::Progress<std::io::Stderr>,
    sink: &mut json::Streaming,
) -> (
    Result<turn::Outcome, bravebot_agent::TurnError>,
    Option<String>,
) {
    let TurnRun {
        config,
        egress,
        workspace,
        task,
        trust_workspace,
        agent,
    } = run;
    let mut session: Option<String> = None;
    // What a continued session vouched for comes back with it, as it does in a session
    // (SESSION-1). The programs it vouched for do not (CLI-1): the run is handed none, and
    // the record keeps the ones it holds.
    let trust = match continued
        .as_ref()
        .and_then(|record| record.trust_map(workspace.root()))
    {
        Some(mut carried) => {
            if trust_workspace {
                carried.trust(".");
            }
            carried
        }
        None => unattended_trust(workspace.root(), trust_workspace),
    };
    let mut conversation = continued
        .as_ref()
        .map(|record| bravebot_agent::Conversation::restored(record.conversation.clone()))
        .unwrap_or_default();
    let begins = conversation.recounted().len();
    let mut rewind = continued.as_ref().map(|record| {
        continued::rewind_points(record, workspace, &conversation, &trust, &task.prompt)
    });
    let completed = turn::resume(
        config,
        egress,
        workspace,
        task,
        &mut conversation,
        confirmer,
        reporter,
        sink,
        trust,
        bravebot_core::programs::TrustedPrograms::new(),
        None,
        &Cancel::new(),
    );
    if let Some(points) = rewind.as_mut() {
        points.keep_backups(workspace.take_backups());
    }
    let turn::Decisions {
        trust,
        programs: _,
        asked_about: _,
        exposed: _,
    } = completed.decisions;
    if let Ok(finished) = &completed.outcome {
        session = continued::keep(
            workspace.root(),
            continued.take().zip(rewind),
            continued::Finished {
                prompt: &task.prompt,
                conversation: &conversation,
                outcome: finished,
                trust: &trust,
                begins,
                prompt_at: reporter.prompt_at(),
                agent,
            },
        );
    }
    (completed.outcome, session)
}

/// What a turn that stopped says: the notices, then what it produced and the trail, then the result
/// object where one was asked for. A manifest run passes the cause and the attempt, and prints the
/// attempt whether or not `--trace` was given, after the notices and before the trail (MANIFEST-3).
fn report_a_failed_turn(
    error: &bravebot_agent::TurnError,
    attempt: Option<&bravebot_agent::manifest::Attempt>,
    trace: bool,
    as_json: bool,
    reporter: &progress::Progress<std::io::Stderr>,
    sink: &json::Streaming,
) -> ExitCode {
    let ending = exit::ending_of(error);
    let stopped = fail(ending, error);
    say_notices(&mut std::io::stderr().lock(), reporter.notices());
    match attempt {
        Some(attempt) => say_what_a_stopped_manifest_produced(
            &mut std::io::stderr().lock(),
            attempt,
            trace.then(|| sink.recorded()),
        ),
        None => {
            if trace {
                eprintln!();
                print_trace(&mut std::io::stderr().lock(), sink.recorded());
            }
        }
    }
    if as_json {
        say_the_result(&what_ran(
            ending,
            &error.to_string(),
            reporter.spent(),
            reporter.calls(),
            &refusals(sink.recorded()),
            reporter.notices(),
        ));
    }
    stopped
}

/// The calls that build a one-shot run's task off the settings, and the sandbox and vetting state
/// beside them, apart from the ones that name the run itself.
fn task_from_the_settings(
    task: Task,
    settings: &bravebot_config::Settings,
    effort: Option<bravebot_session::store::Effort>,
) -> Task {
    task
        // The flag, then the settings layers and the saved pick ranked as BACKEND-43 ranks them.
        // The layers are the only route a machine where nobody ever opens the interface has to a
        // level that outlives one run.
        .with_effort(effort.or_else(|| {
            bravebot_session::store::effort(bravebot_session::store::load_effort(), settings)
        }))
        // What the settings say this run may add to a commit message or a pull request it writes
        // (BACKEND-30). Read off the same resolved settings the permission rules came from.
        .with_attribution(settings.attribution().clone())
        // And what they say a command's output may spend of this run's context (RUN-21), off the
        // same resolved settings.
        .with_output_cap(settings.run_output_cap())
        // And what they say one download may write to a file (DOWNLOAD-4).
        .with_download_cap(settings.download_max_bytes())
        // And how long one may take, resolved here because this is where the settings were read
        // (RUN-23).
        .with_deadlines(bravebot_agent::exec::Deadlines::resolve(
            settings.run_deadlines(),
        ))
        .with_confined_runs(true)
        .with_sandbox_mode(bravebot_config::sandbox::in_force().mode)
        // Whether a check that finds nothing answers in a person's place. Resolved here, once, out
        // of the three routes: `bravebot_core::vetting::auto` is the rule and nothing below reads
        // any of the three again. A run nobody is watching has no prompt to fall back to, so
        // without this every check on this path ends in a refusal.
        .with_auto_vetting(bravebot_core::vetting::auto(
            bravebot_core::vetting::asked_for(),
            bravebot_session::store::load_vetting(),
            settings.auto_vetting(),
        ))
}

/// The schema a path names, read and vetted, or the sentence that says why it cannot be used.
fn load_output_schema(path: &str) -> Result<bravebot_agent::output_schema::OutputSchema, String> {
    use bravebot_agent::output_schema::{OutputSchema, SchemaError};
    let text = std::fs::read_to_string(path).map_err(|err| {
        t!(
            cli_output_schema_unreadable,
            path = path,
            problem = err.to_string()
        )
        .to_string()
    })?;
    OutputSchema::parse(&text).map_err(|err| {
        match err {
            SchemaError::NotJson => t!(cli_output_schema_not_json, path = path),
            SchemaError::NotAnObject { at } => {
                t!(cli_output_schema_not_an_object, path = path, at = at)
            }
            SchemaError::Unsupported { at, keyword } => t!(
                cli_output_schema_unsupported,
                path = path,
                at = at,
                keyword = keyword
            ),
            SchemaError::Malformed { at, keyword } => t!(
                cli_output_schema_malformed,
                path = path,
                at = at,
                keyword = keyword
            ),
        }
        .to_string()
    })
}

/// Why a finished reply is not the shape it was asked for, naming where in it and which rule.
///
/// Built from a position and a rule alone, so nothing the reply spelt reaches the sentence.
fn off_schema_message(broke: &bravebot_agent::output_schema::Mismatch) -> String {
    use bravebot_agent::output_schema::Rule;
    let problem = match broke.rule {
        Rule::NotJson => t!(cli_output_schema_rule_not_json),
        Rule::Type => t!(cli_output_schema_rule_type),
        Rule::Enum => t!(cli_output_schema_rule_enum),
        Rule::Const => t!(cli_output_schema_rule_const),
        Rule::Required => t!(cli_output_schema_rule_required),
        Rule::Extra => t!(cli_output_schema_rule_extra),
        Rule::Length => t!(cli_output_schema_rule_length),
        Rule::Count => t!(cli_output_schema_rule_count),
        Rule::Range => t!(cli_output_schema_rule_range),
    };
    t!(
        cli_output_schema_mismatch,
        at = broke.at.as_str(),
        problem = problem
    )
    .to_string()
}

/// Whether a result object was asked for, read straight off the command line.
///
/// Only for the case where the parse failed, which is the one moment the parsed invocation cannot
/// answer.
fn wants_json(args: &[String]) -> bool {
    args.iter()
        .any(|arg| arg == "--json" || arg == "--json-stream")
}

/// What a pipe may carry before it is refused.
///
/// Matches the cap other agents document, and exists because the alternative is a `bravebot -p` that a
/// stray `cat` of a disk image holds open while it fills memory.
const PIPE_CAP: usize = 10 * 1024 * 1024;

/// Read input piped into the process, if any.
///
/// Generic over the source, as [`progress::Progress`] is over its sink, so a test can pass bytes
/// and a terminal answer without a real pipe.
///
/// A terminal means nothing was piped, and a read error means nobody is feeding us: neither is a
/// reason to stop, so the run continues on the argument prompt. Exceeding the cap is different,
/// since silently truncating would hand the model a fragment of what the user piped and say
/// nothing about it.
fn piped_input(source: impl Read, is_tty: bool) -> Result<Option<String>, String> {
    if is_tty {
        return Ok(None);
    }

    // One byte past the cap, so the buffer that proves the input was too large is not itself the
    // problem the cap exists to avoid.
    let mut buffer = Vec::new();
    if let Err(err) = source.take(PIPE_CAP as u64 + 1).read_to_end(&mut buffer) {
        eprintln!("{}", t!(cli_piped_input_unreadable, problem = err));
        return Ok(None);
    }

    if buffer.len() > PIPE_CAP {
        return Err(t!(
            cli_piped_input_too_large,
            limit = PIPE_CAP / (1024 * 1024)
        ));
    }

    if buffer.is_empty() {
        return Ok(None);
    }

    // Lossy because the bytes are never decided from: they go into a slot and the planner is shown
    // a reference, so a replacement character changes nothing that matters.
    Ok(Some(String::from_utf8_lossy(&buffer).into_owned()))
}

fn resume_named(
    id: &str,
    agent: Option<String>,
    links: bravebot_tui::app::Links,
    skip_permissions: bool,
    prompts: SystemPrompts,
) -> ExitCode {
    let Ok(directory) = std::env::current_dir() else {
        return fail(Ending::Failed, t!(cli_directory_unknown));
    };
    match bravebot_session::sessions::load(&directory, id) {
        // Printing what a run produced is what naming one here is for (SESSION-10), and a session
        // that started a run says to name it. So this answers on stdout and succeeds: the id was
        // real, the record was read, and the person got the thing they asked for. Reporting it as a
        // failure would make the line the session printed read as advice that does not work.
        Some(record) if record.manifest.is_some() => {
            println!("{}", t!(cli_manifest_run, id = id));
            if let Some(stored) = &record.manifest {
                let report = stored.describe();
                if !report.is_empty() {
                    println!();
                    print!("{report}");
                }
            }
            ExitCode::SUCCESS
        }
        // Not resumed by a second process: the one running it writes the record after every turn,
        // and two writers would be one conversation (BG-9).
        Some(record) if background::is_running(&record.id) => {
            background::refuse_to_resume(&record.id)
        }
        Some(record) => interactive(
            bravebot_tui::app::Start::Resuming(Box::new(record)),
            agent,
            links,
            skip_permissions,
            prompts,
        ),
        None => fail(Ending::Argument, t!(cli_no_such_session, id = id)),
    }
}

fn fork_named(
    id: &str,
    agent: Option<String>,
    links: bravebot_tui::app::Links,
    skip_permissions: bool,
    prompts: SystemPrompts,
) -> ExitCode {
    let Ok(directory) = std::env::current_dir() else {
        return fail(Ending::Failed, t!(cli_directory_unknown));
    };
    match bravebot_session::sessions::load(&directory, id) {
        Some(record) if record.manifest.is_some() => {
            let refused = fail(Ending::Failed, bravebot_tui::resume::manifest_note());
            if let Some(stored) = &record.manifest {
                let report = stored.describe();
                if !report.is_empty() {
                    eprintln!();
                    eprint!("{report}");
                }
            }
            refused
        }
        Some(_) => match bravebot_session::sessions::fork(&directory, id) {
            Some(record) => interactive(
                bravebot_tui::app::Start::Resuming(Box::new(record)),
                agent,
                links,
                skip_permissions,
                prompts,
            ),
            None => fail(Ending::Argument, t!(cli_no_such_session, id = id)),
        },
        None => fail(Ending::Argument, t!(cli_no_such_session, id = id)),
    }
}

/// Pick up the most recent session under this directory, without anybody naming one.
///
/// Where there is none, this says so and fails. Starting a fresh session instead would answer a
/// different question than the one asked, and it would answer it by throwing away the request:
/// somebody who meant to carry on and got an empty transcript has lost the thing they asked for.
fn continue_here(
    agent: Option<String>,
    links: bravebot_tui::app::Links,
    skip_permissions: bool,
    prompts: SystemPrompts,
) -> ExitCode {
    let Ok(directory) = std::env::current_dir() else {
        return fail(Ending::Failed, t!(cli_directory_unknown));
    };
    match bravebot_session::sessions::most_recent(&directory) {
        // By the id, so this arrives at the interface the way a named resume does, down to a
        // record that went away between the list and the read.
        Some(session) => resume_named(&session.id, agent, links, skip_permissions, prompts),
        None => fail(Ending::Failed, t!(cli_nothing_to_continue)),
    }
}

fn interactive(
    start: bravebot_tui::app::Start,
    agent: Option<String>,
    links: bravebot_tui::app::Links,
    skip_permissions: bool,
    prompts: SystemPrompts,
) -> ExitCode {
    if agent.is_some() {
        bravebot_tui::app::background_cannot_carry("--agent");
    }
    if let Some(flag) = flag_named(&prompts) {
        bravebot_tui::app::background_cannot_carry(flag);
    }
    let mut config = match Config::from_env() {
        Ok(c) => c,
        Err(err) => {
            return fail(
                Ending::Configuration,
                t!(cli_configuration_problem, problem = err),
            );
        }
    };

    // Before the session opens, for the reason a one-shot run is stopped before the turn: a
    // transcript that began with nothing configured would read as the agent rather than as the
    // configuration, and this is the one moment somebody is looking for what to do next.
    if let Some(ended) = import::before_the_session(&mut config) {
        return ended;
    }
    // The model `-i` was given is asked about as a one-shot run's is, so one nothing serves or the
    // machine-level layer refuses ends the start here, naming the file, rather than at the first turn.
    if let bravebot_tui::app::Start::Sending(
        _,
        bravebot_tui::app::AskedFor {
            model: Some(name), ..
        },
    ) = &start
        && let Some(how) = nothing_serves(&config, &config.model_named(name))
    {
        return fail(Ending::Configuration, how);
    }

    let settings = bravebot_config::Settings::load();
    let workspace = match current_workspace(&settings, &Managed::load()) {
        Ok(w) => w,
        Err(err) => return fail(Ending::Failed, t!(cli_workspace_problem, problem = err)),
    };

    // What the platform offers, which is what the session reports: this confines a process running
    // code we did not write, which is an MCP server this session starts and nothing else it runs.
    // Read once here because the answer cannot change while the session runs. With no backend no
    // stage starts, so nothing is said about the sockets one would reach.
    let (confinement, sockets_with_a_path_reachable) =
        match bravebot_sandbox::for_current_platform() {
            Ok(sandbox) => {
                let capabilities = sandbox.capabilities();
                (
                    confinement_name(capabilities.level),
                    !capabilities.governs_sockets_with_a_path,
                )
            }
            Err(_) => (
                confinement_name(bravebot_sandbox::policy::ConfinementLevel::None),
                false,
            ),
        };

    // Settled on the plain terminal before the screen takes it, so a question about a server this
    // checkout requests is a line somebody answers rather than a dialog drawn over a session that
    // has not begun. Held until the session ends, since dropping one stops its server.
    let mut reached = servers::for_this_session(
        &settings,
        workspace.root(),
        match skip_permissions {
            true => servers::Asking::Bypass,
            false => servers::Asking::Person,
        },
        &mut servers::at_the_terminal(),
        // Nowhere, since the screen would draw over whatever a server writes to its stderr.
        bravebot_sandbox::Stream::Null,
    );
    let started = bravebot_tui::Servers {
        started: reached.aliases(),
        confined: reached.confined(),
        notes: std::mem::take(&mut reached.notes),
        session: reached.session(),
    };

    match bravebot_tui::app::run(
        &mut config,
        &workspace,
        confinement,
        sockets_with_a_path_reachable,
        started,
        start,
        agent,
        links,
        skip_permissions,
        prompts,
    ) {
        // Printed after the terminal is handed back, so it survives on the screen the person is
        // left looking at rather than going onto the alternate screen with everything else. A
        // session is worth resuming far more often than anybody thinks to write its name down
        // beforehand, and the picker is no help to someone who has already closed the window.
        Ok(bravebot_tui::app::Ended::Left(Some(left))) => {
            println!("{}", resume_hint(&left, workspace.root()));
            ExitCode::SUCCESS
        }
        Ok(bravebot_tui::app::Ended::Left(None)) => ExitCode::SUCCESS,
        // After the terminal is handed back, so what it says survives on the screen. The process
        // that starts the background session is this one, run by the person (BG-2).
        Ok(bravebot_tui::app::Ended::Background { left, mode }) => {
            match background::hand_over(&left, mode) {
                Ok(()) => ExitCode::SUCCESS,
                Err(code) => {
                    eprintln!("{}", resume_hint(&left, workspace.root()));
                    code
                }
            }
        }
        // Printed after the terminal is handed back, like the hint above, with the argument status:
        // the session never took a turn, and a one-shot run exits with the same status for the
        // same name (CLI-17).
        Ok(bravebot_tui::app::Ended::Refused(refused)) => fail(Ending::Argument, refused),
        Err(err) => fail(Ending::Failed, t!(cli_interface_problem, problem = err)),
    }
}

/// What to say on the way out about picking this session up again.
///
/// `--resume` looks an id up under the working directory it is run in, so the id on its own is a
/// whole answer only while the session ended where it started. `/cd` moves the record, and the
/// shell this is printed into did not move with it: a bare id there names a session the shell
/// cannot find, or worse, finds an earlier state of the same one and resumes that. Naming the
/// directory is what keeps the line true.
///
/// Said only when the two differ. Telling somebody the directory they are already standing in
/// reads as though something had happened to it.
fn resume_hint(left: &Resumable, started_in: &Path) -> String {
    let heading = match left.directory == started_in {
        true => t!(cli_resume_heading).to_string(),
        false => t!(
            cli_resume_moved,
            directory = left.directory.display().to_string()
        ),
    };
    format!("{heading}\nbravebot --resume {}", left.id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::*;
    use crate::workspace::unattended_trust_with;
    use bravebot_agent::Workspace;
    use bravebot_agent::confirm::{
        Confirmer, Decision, ManifestRequest, VouchRequest, WriteRequest,
    };
    use bravebot_core::capability::Capability;
    use bravebot_core::event::Sink;
    use bravebot_core::event::{Event, RecordingSink, Role};
    use bravebot_core::label::Label;
    use bravebot_core::slot::SlotId;
    use bravebot_session::store::Effort;
    use bravebot_tui::app::AskedFor;
    use std::io::Write;
    use std::path::PathBuf;

    /// These commands start no session, so a flag that shapes one has nothing to shape. Taking it
    /// and ignoring it would read as honoured, which is the reason CLI-13 refuses it by name.
    #[test]
    fn the_commands_that_start_no_session_refuse_every_flag_that_shapes_one() {
        let thirteen = [
            "doctor",
            "update",
            "bug-report",
            "auth",
            "mcp",
            "permissions",
            "sessions",
            "attach",
            "reply",
            "import-leo-creds",
            "import-providers",
            "completion",
            "shell-init",
        ];
        for command in thirteen {
            assert!(without_a_definition(Some(command)).is_some(), "{command}");
            assert!(
                without_a_prompt_to_give("--system-prompt", Some(command)).is_some(),
                "{command}"
            );
            // CLI-26 does not name `bug-report`, so the tool limit is not refused beside it.
            assert_eq!(
                without_a_tool_set_to_limit(command).is_some(),
                command != "bug-report",
                "{command}"
            );
        }
        assert!(without_a_definition(Some("-p")).is_none());
        assert!(without_a_definition(None).is_none());
        assert!(without_a_tool_set_to_limit("-p").is_none());
    }

    /// A session that stayed where it started needs no directory: the shell reading this line is
    /// already standing in the one the id will be looked up under.
    #[test]
    fn a_session_that_stayed_put_is_named_by_its_id_alone() {
        let left = Resumable {
            id: "abc".to_string(),
            directory: PathBuf::from("/work"),
        };
        let hint = resume_hint(&left, Path::new("/work"));

        assert!(hint.contains("bravebot --resume abc"), "{hint}");
        assert!(
            !hint.contains("/work"),
            "the directory somebody is already in was named at them: {hint}"
        );
    }

    /// A session that moved with `/cd` left its record where it moved to, and `--resume` looks an
    /// id up under the directory it is run in. Printing the id alone would name a session this
    /// shell cannot find, or find an earlier state of the same one and resume that.
    #[test]
    fn a_session_that_moved_says_where_to_resume_it() {
        let left = Resumable {
            id: "abc".to_string(),
            directory: PathBuf::from("/other"),
        };
        let hint = resume_hint(&left, Path::new("/work"));

        assert!(hint.contains("bravebot --resume abc"), "{hint}");
        assert!(
            hint.contains("/other"),
            "the line does not say where the session went: {hint}"
        );
    }

    /// The flag is taken out wherever it was typed, with its word, and a word that is not a level
    /// is refused rather than read as the default.
    #[test]
    fn the_log_level_flag_is_taken_out_with_its_word_and_refuses_any_other() {
        use bravebot_diag::Level;
        for typed in [
            &["--log-level", "debug", "-p", "do a thing"][..],
            &["-p", "--log-level", "debug", "do a thing"][..],
            &["-p", "do a thing", "--log-level", "debug"][..],
        ] {
            let mut arguments = args(typed);
            assert_eq!(take_log_level(&mut arguments), Ok(Some(Level::Debug)));
            assert_eq!(arguments, args(&["-p", "do a thing"]), "{typed:?}");
        }
        let mut none = args(&["-p", "do a thing"]);
        assert_eq!(take_log_level(&mut none), Ok(None));
        for typed in [
            &["-p", "x", "--log-level"][..],
            &["--log-level", "Debug", "-p", "x"][..],
            &["--log-level", "trace", "-p", "x"][..],
        ] {
            let mut arguments = args(typed);
            assert!(take_log_level(&mut arguments).is_err(), "{typed:?}");
            assert_eq!(arguments, args(typed), "a refusal rewrote the list");
        }
    }

    /// An interactive `bravebot -p "task"` must not block waiting for a pipe that is not coming.
    #[test]
    fn a_terminal_stdin_is_not_read() {
        let source: &[u8] = b"this would be read from a pipe";
        assert_eq!(piped_input(source, true), Ok(None));
    }

    #[test]
    fn piped_bytes_are_read_when_stdin_is_not_a_terminal() {
        let source: &[u8] = b"a build log\n";
        assert_eq!(piped_input(source, false), Ok(Some("a build log\n".into())));
    }

    /// Truncating would hand the planner a fragment of what the user piped without saying so, and
    /// a quarantined fragment is one nobody can notice is short.
    #[test]
    fn input_over_the_cap_is_refused() {
        let oversized = vec![b'x'; PIPE_CAP + 1];
        assert!(
            piped_input(oversized.as_slice(), false).is_err(),
            "an oversized pipe must be refused, not truncated"
        );

        let at_the_cap = vec![b'x'; PIPE_CAP];
        assert!(
            piped_input(at_the_cap.as_slice(), false).is_ok(),
            "the cap itself is allowed"
        );
    }

    /// A pipe is not something a person can shorten: what they have is a program writing bytes at
    /// a command, and a refusal that only says the bytes were too many leaves them with nothing to
    /// try. The way to hand over something this large is a different gesture, so the refusal names
    /// it.
    #[test]
    fn a_refused_pipe_says_what_to_do_instead() {
        let oversized = vec![b'x'; PIPE_CAP + 1];

        let refusal = piped_input(oversized.as_slice(), false)
            .expect_err("an oversized pipe must be refused, not truncated");

        assert!(
            refusal.contains("Write it to a file and name that instead"),
            "the refusal said the input was too large and nothing about what to do about it: \
             {refusal}"
        );
    }

    fn trail_of(events: Vec<Event>) -> RecordingSink {
        let mut sink = RecordingSink::new();
        for event in events {
            sink.emit(event);
        }
        sink
    }

    /// The reply is what gets piped onward, so anything else sharing its stream corrupts the
    /// file at the other end. This is the whole of what makes a one-shot run pipeable.
    #[test]
    fn stdout_carries_the_reply_and_nothing_else() {
        let sink = trail_of(vec![Event::GatePassed {
            gate: "display",
            detail: "assistant reply shown to the user".into(),
        }]);
        let (reply, beside) = written(&Finished {
            reply: "ok",
            notices: &["a skill was loaded".to_string()],
            attempt: None,
            trail: Some((&sink, "qwen-3-235b")),
            clean: false,
            ending: Ending::Refused,
            not_served: None,
        });

        assert_eq!(reply, "ok\n");
        assert!(beside.contains("audit trail"), "got: {beside}");
        assert!(beside.contains("note: a skill was loaded"), "got: {beside}");
        assert!(beside.contains("model: qwen-3-235b"), "got: {beside}");
        assert!(beside.contains("a policy gate refused"), "got: {beside}");
        // Status 4 says BB1004 where it is read off a log (CLI-6).
        assert!(beside.contains("BB1004: "), "got: {beside}");
    }

    /// A caller reading the result object rather than stderr is a caller with nowhere to draw, and a
    /// run that failed is the one whose sentences have no reply to arrive on (HOOK-7). Left out,
    /// there is no surface at all on which that caller learns a hook of theirs is broken.
    #[test]
    fn a_result_object_for_a_failed_run_lists_what_the_turn_said() {
        let object = what_ran(
            Ending::Failed,
            "BB1001: nothing answered",
            Default::default(),
            &[],
            &[],
            &["hook turn-finished: /usr/bin/fmt could not be started".to_string()],
        );

        assert!(
            object
                .contains(r#""notices":["hook turn-finished: /usr/bin/fmt could not be started"]"#),
            "got: {object}"
        );
    }

    /// Without `--trace` the trail is not written at all, rather than written somewhere quieter.
    #[test]
    fn an_untraced_run_writes_no_trail() {
        let (reply, beside) = written(&Finished {
            reply: "ok",
            notices: &[],
            attempt: None,
            trail: None,
            clean: true,
            ending: Ending::Done,
            not_served: None,
        });

        assert_eq!(reply, "ok\n");
        assert!(beside.is_empty(), "got: {beside}");
    }

    /// Every arm of the trail is a line somebody reads to see what the turn was allowed to do,
    /// so a refusal has to be as legible as a pass.
    #[test]
    fn the_trail_renders_a_line_for_every_event() {
        let sink = trail_of(vec![
            Event::GatePassed {
                gate: "capability",
                detail: "file_read granted".into(),
            },
            Event::GateBlocked {
                gate: "network",
                detail: "egress".into(),
                reason: "host not allowed".into(),
                principle: bravebot_core::event::Principle::Confinement,
            },
            Event::Observed {
                capability: Capability::FileRead,
                label: Label::untrusted_private(),
            },
            Event::SlotWritten {
                slot: SlotId::new("file:a.rs"),
                label: Label::untrusted_private(),
            },
            Event::SlotDeferred {
                slot: SlotId::new("file:b.rs"),
                label: Label::untrusted_private(),
                origin: "b.rs".into(),
            },
            Event::Declassified {
                slot: SlotId::new("reply"),
                from: Label::untrusted_public(),
                to: Label::trusted_public(),
                reason: "present",
            },
            Event::ActionField {
                tool: "write".into(),
                field: "path".into(),
                role: Role::Routing,
                label: Label::untrusted_public(),
                allowed: false,
            },
        ]);

        let mut output = Vec::new();
        print_trace(&mut output, &sink);
        let trail = String::from_utf8(output).expect("utf-8");
        let lines: Vec<&str> = trail.lines().collect();

        assert_eq!(lines[0], "audit trail");
        assert_eq!(lines.len(), 8, "a line each, plus the heading: {trail}");
        assert!(lines[1].contains("ok      capability: file_read granted"));
        assert!(lines[2].contains("BLOCK   network: host not allowed"));
        assert!(lines[3].starts_with("  observe "));
        assert!(lines[4].starts_with("  slot    file:a.rs"));
        assert!(lines[5].contains("holds b.rs, unread"));
        assert!(lines[6].starts_with("  release reply "));
        assert!(lines[7].contains("BLOCK") && lines[7].contains("write.path [routing]"));
    }

    /// The trail is written after the reply is already on stdout, so a stderr nobody is reading
    /// must not take the run down with it: the exit code still has a turn to report on.
    #[test]
    fn a_closed_stream_does_not_stop_the_trail() {
        struct Closed;
        impl Write for Closed {
            fn write(&mut self, _buf: &[u8]) -> std::io::Result<usize> {
                Err(std::io::Error::from(std::io::ErrorKind::BrokenPipe))
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Err(std::io::Error::from(std::io::ErrorKind::BrokenPipe))
            }
        }

        let mut sink = RecordingSink::new();
        sink.emit(Event::GatePassed {
            gate: "display",
            detail: "assistant reply shown to the user".into(),
        });
        print_trace(&mut Closed, &sink);
    }

    fn args(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|part| (*part).to_string()).collect()
    }

    /// TRUST-26. The flag gives the run the map a yes writes: the working directory and what is
    /// under it. A directory `--add-dir` opened sits beside it and is not covered, and neither is
    /// anything above it.
    #[test]
    fn a_flagged_run_trusts_its_directory_and_nothing_outside_it() {
        let scratch = Scratch::new("trust-flag-covers");
        let project = scratch.directory("project");
        let beside = scratch.directory("beside");

        let (trust, notice) = unattended_trust_with(None, None, &project, true);

        assert!(trust.is_trusted("."), "the working directory");
        assert!(trust.is_trusted("src/lib.rs"), "a file under it");
        assert!(
            !trust.is_trusted(&beside.join("notes.md").display().to_string()),
            "a directory opened beside the working one"
        );
        assert!(!trust.is_trusted("/etc/passwd"), "a path outside it");
        assert_eq!(
            notice, None,
            "the person typed the flag, so nothing is said"
        );
    }

    /// TRUST-26. The flag answers for this run alone, and the run without it keeps the empty map.
    #[test]
    fn a_flagged_run_writes_no_record_and_says_nothing() {
        let scratch = Scratch::new("trust-flag-writes-nothing");
        let project = scratch.directory("project");
        let state = scratch.directory("state");

        let (_, notice) = unattended_trust_with(Some(&state), None, &project, true);
        assert_eq!(notice, None);
        assert!(
            !state.join("trusted").exists(),
            "the flag kept an answer for later sessions"
        );

        let (trust, notice) = unattended_trust_with(Some(&state), None, &project, false);
        assert!(!trust.is_trusted("."), "nothing was said and nothing kept");
        assert_eq!(notice, None);
    }

    /// RUN-5. A one-shot run starts from a kept answer, and what that answer says about confined
    /// lines is what the entry said: an answer kept before the question said so does not allow
    /// them, one that says so does, and neither changes what the run trusts.
    #[test]
    fn a_kept_answer_allows_confined_lines_only_where_its_entry_says_so() {
        let scratch = Scratch::new("trust-kept-confined-lines");
        let project = scratch.directory("project");
        let state = scratch.directory("state");
        let Some(identity) = bravebot_agent::trusted::Identity::of(&project) else {
            return;
        };
        let store = bravebot_agent::trusted::Store::new(&state, &project);
        assert!(store.keep(&identity, "1-2", 7, false), "not kept");
        let (trust, _) = unattended_trust_with(Some(&state), None, &project, false);
        assert!(trust.is_trusted("."));
        assert!(
            !trust.allows_confined_lines(),
            "an answer kept before the question said so allowed confined lines"
        );

        assert!(store.keep(&identity, "3-4", 8, true), "not kept");
        let (trust, _) = unattended_trust_with(Some(&state), None, &project, false);
        assert!(trust.allows_confined_lines(), "the entry said so");

        let (flagged, _) = unattended_trust_with(Some(&state), None, &project, true);
        assert!(
            !flagged.allows_confined_lines(),
            "--trust-workspace allowed confined lines"
        );
    }

    /// TRUST-23. A kept answer trusts a one-shot run in that directory and the run says where it
    /// came from, since nobody is watching it open.
    #[test]
    fn a_kept_answer_trusts_a_one_shot_run_there_and_says_so() {
        let scratch = Scratch::new("trust-kept-one-shot");
        let project = scratch.directory("project");
        let state = scratch.directory("state");
        let Some(identity) = bravebot_agent::trusted::Identity::of(&project) else {
            // A filesystem that cannot say when a directory was made keeps no answer to read.
            return;
        };
        let store = bravebot_agent::trusted::Store::new(&state, &project);
        assert!(
            store.keep(&identity, "1-2", 7, false),
            "the answer was not kept"
        );

        let (trust, notice) = unattended_trust_with(Some(&state), None, &project, false);

        assert!(trust.is_trusted("."), "the kept answer did not apply");
        assert!(trust.is_trusted("src/lib.rs"), "it did not cover the tree");
        assert!(!trust.is_trusted("/etc/passwd"), "it covered too much");
        let notice = notice.expect("a kept answer was used without saying so");
        assert!(
            notice.contains(&project.display().to_string())
                && notice.contains(&store.path().display().to_string()),
            "the notice names neither the directory nor the file: {notice}"
        );
    }

    /// TRUST-23. A one-shot run started in a package of a repository whose root has a kept answer is
    /// settled by it, as a session there is, trusts its own directory and says which one answered.
    #[test]
    fn a_kept_answer_about_the_repository_root_trusts_a_one_shot_run_in_a_package() {
        let scratch = Scratch::new("trust-kept-root-one-shot");
        let repository = scratch.directory("repository");
        let package = scratch.directory("repository/packages/app");
        let state = scratch.directory("state");
        std::fs::create_dir_all(repository.join(".git")).expect("make the repository");
        let Some(identity) = bravebot_agent::trusted::Identity::of(&repository) else {
            return;
        };
        let store = bravebot_agent::trusted::Store::new(&state, &repository);
        assert!(
            store.keep(&identity, "1-2", 7, false),
            "the answer was not kept"
        );

        let (trust, notice) = unattended_trust_with(Some(&state), None, &package, false);

        assert!(trust.is_trusted("."), "the root's answer did not apply");
        assert!(trust.is_trusted("src/lib.rs"), "it did not cover the tree");
        let notice = notice.expect("a kept answer was used without saying so");
        assert!(
            notice.contains(&repository.display().to_string())
                && !notice.contains(&package.display().to_string()),
            "the notice does not name the root that answered, and only it: {notice}"
        );
    }

    /// TRUST-23. The answer is about one directory: another at a different path, and a new
    /// directory made at the same path, are not covered, and a run there trusts nothing.
    #[test]
    fn a_kept_answer_about_another_directory_trusts_a_one_shot_run_nowhere() {
        let scratch = Scratch::new("trust-kept-elsewhere");
        let project = scratch.directory("project");
        let other = scratch.directory("other");
        let state = scratch.directory("state");
        let Some(identity) = bravebot_agent::trusted::Identity::of(&project) else {
            return;
        };
        let store = bravebot_agent::trusted::Store::new(&state, &project);
        assert!(
            store.keep(&identity, "1-2", 7, false),
            "the answer was not kept"
        );

        let (trust, notice) = unattended_trust_with(Some(&state), None, &other, false);
        assert!(!trust.is_trusted("."), "another directory was trusted");
        assert_eq!(notice, None);

        std::fs::remove_dir_all(&project).expect("remove the directory answered about");
        std::fs::create_dir_all(&project).expect("make another at the same path");
        let (trust, notice) = unattended_trust_with(Some(&state), None, &project, false);
        assert!(!trust.is_trusted("."), "a remade directory was trusted");
        assert_eq!(notice, None);
    }

    /// TRUST-26. The flag is a switch on the command line that may be repeated, and its absence
    /// leaves the run without it.
    #[test]
    fn the_workspace_flag_is_read_from_the_command_line_and_may_repeat() {
        let typed = |arguments: &[&str]| {
            parse_invocation(&args(arguments))
                .expect("parses")
                .trust_workspace
        };
        assert!(typed(&["--trust-workspace", "-p", "do a thing"]));
        assert!(typed(&["-p", "do a thing", "--trust-workspace"]));
        assert!(typed(&[
            "--trust-workspace",
            "--trust-workspace",
            "do a thing"
        ]));
        assert!(!typed(&["-p", "do a thing"]));
    }

    /// The mode composes rather than leads: what is left after taking it out is the invocation the
    /// person would have typed without it, so every dispatch below sees what it always saw.
    #[test]
    fn the_incognito_flag_is_taken_out_wherever_it_appears() {
        for typed in [
            &["--incognito", "-p", "do a thing"][..],
            &["-p", "--incognito", "do a thing"][..],
            &["-p", "do a thing", "--incognito"][..],
        ] {
            let mut arguments = args(typed);
            assert!(
                take_flag(&mut arguments, INCOGNITO),
                "{typed:?} did not engage it"
            );
            assert_eq!(
                arguments,
                args(&["-p", "do a thing"]),
                "left over: {typed:?}"
            );
        }
    }

    /// Asking twice for a mode that is already on is not an error to report.
    #[test]
    fn asking_for_incognito_twice_is_asking_once() {
        let mut arguments = args(&["--incognito", "--incognito", "do a thing"]);
        assert!(take_flag(&mut arguments, INCOGNITO));
        assert_eq!(arguments, args(&["do a thing"]));
    }

    /// The flag is the whole invocation often enough to be worth pinning: what is left is nothing,
    /// which the dispatch reads as the interactive session, and that is the intent.
    #[test]
    fn incognito_alone_leaves_the_interactive_session() {
        let mut arguments = args(&["--incognito"]);
        assert!(take_flag(&mut arguments, INCOGNITO));
        assert!(arguments.is_empty());
    }

    /// An ordinary invocation is untouched, and stays not incognito. A mode that turned itself on
    /// for something that merely mentioned it would be worse than one that never worked.
    #[test]
    fn an_invocation_without_the_flag_is_left_alone() {
        let mut arguments = args(&["-p", "write about incognito mode"]);
        assert!(!take_flag(&mut arguments, INCOGNITO));
        assert_eq!(arguments, args(&["-p", "write about incognito mode"]));
    }

    /// `--safe` belongs to every way of starting, so it is taken out wherever it appears, once or
    /// twice, and a task that merely mentions safe mode does not turn it on.
    #[test]
    fn the_safe_flag_is_taken_out_wherever_it_appears() {
        for typed in [
            &["--safe", "-p", "do a thing"][..],
            &["-p", "--safe", "do a thing"][..],
            &["-p", "do a thing", "--safe", "--safe"][..],
        ] {
            let mut arguments = args(typed);
            assert!(take_flag(&mut arguments, SAFE), "{typed:?}");
            assert!(!arguments.iter().any(|arg| arg == "--safe"), "{typed:?}");
        }
        let mut arguments = args(&["-p", "explain safe mode"]);
        assert!(!take_flag(&mut arguments, SAFE));
        assert_eq!(arguments, args(&["-p", "explain safe mode"]));
    }

    /// `--tools` and `--no-shell` belong to every way of starting, so they are taken out wherever they
    /// stand. A task that merely mentions them is left alone, and a list naming no tool is refused
    /// rather than read as no limit.
    #[test]
    fn the_tool_limit_is_taken_out_wherever_it_appears() {
        let mut arguments = args(&[
            "-p",
            "--tools",
            "read_file,search",
            "do a thing",
            "--no-shell",
        ]);
        let limit = take_tool_limit(&mut arguments).expect("a list of tools");
        assert_eq!(arguments, args(&["-p", "do a thing"]));
        assert!(limit.no_shell);
        assert_eq!(
            limit.only,
            Some(vec!["read_file".to_string(), "search".to_string()])
        );

        let mut arguments = args(&["-p", "explain --no-shell and --tools"]);
        let limit = take_tool_limit(&mut arguments).expect("no flag");
        assert!(limit.is_none());
        assert_eq!(arguments, args(&["-p", "explain --no-shell and --tools"]));

        let mut arguments = args(&["-p", "x", "--tools", "read_file,nothing"]);
        let err = take_tool_limit(&mut arguments).expect_err("an unknown tool");
        assert!(
            err.contains("nothing") && err.contains("read_file"),
            "{err}"
        );
    }

    /// A list that is followed by `--no-shell` has taken it as a tool name, so the refusal names it
    /// and the arguments are left as typed.
    #[test]
    fn a_tool_list_that_is_the_no_shell_flag_names_it_as_the_unknown_tool() {
        let mut arguments = args(&["-p", "x", "--tools", "--no-shell"]);
        let err = take_tool_limit(&mut arguments).expect_err("a flag is no tool");
        assert!(err.contains("--no-shell"), "{err}");
        assert_eq!(arguments, args(&["-p", "x", "--tools", "--no-shell"]));
    }

    /// `--vet` belongs to every way of starting, so it is taken out wherever it appears and the
    /// dispatch below goes on matching a list it is no longer in. A flag left in the arguments
    /// would be refused as an unknown option by whichever subcommand read them next.
    #[test]
    fn the_vet_flag_is_taken_out_wherever_it_appears() {
        for typed in [
            &["--vet", "-p", "do a thing"][..],
            &["-p", "--vet", "do a thing"][..],
            &["-p", "do a thing", "--vet"][..],
        ] {
            let mut arguments = args(typed);
            assert!(
                take_flag(&mut arguments, VET),
                "{typed:?} did not ask for it"
            );
            assert_eq!(
                arguments,
                args(&["-p", "do a thing"]),
                "left over: {typed:?}"
            );
        }
    }

    /// Asking twice for something that is already on is not an error to report.
    #[test]
    fn asking_to_vet_twice_is_asking_once() {
        let mut arguments = args(&["--vet", "--vet", "do a thing"]);
        assert!(take_flag(&mut arguments, VET));
        assert_eq!(arguments, args(&["do a thing"]));
    }

    /// An ordinary invocation is untouched and stays as it was. A run that turned the asking off
    /// because the prompt mentioned vetting would be worse than a flag that never worked.
    #[test]
    fn an_invocation_that_only_mentions_vetting_does_not_ask_for_it() {
        let mut arguments = args(&["-p", "explain --vetting to me"]);
        assert!(!take_flag(&mut arguments, VET));
        assert_eq!(arguments, args(&["-p", "explain --vetting to me"]));
    }

    /// It composes with the other two flags that belong to every way of starting, which is the
    /// whole reason all three are taken out before anything dispatches.
    #[test]
    fn vetting_composes_with_the_other_flags_that_lead() {
        for typed in [
            &["--vet", "--incognito", "-p", "x"][..],
            &["--incognito", "--vet", "-p", "x"][..],
        ] {
            let mut arguments = args(typed);
            assert!(take_flag(&mut arguments, VET), "{typed:?}");
            assert!(take_flag(&mut arguments, INCOGNITO), "{typed:?}");
            assert_eq!(arguments, args(&["-p", "x"]), "{typed:?}");
        }
    }

    fn a_plan() -> ManifestRequest {
        ManifestRequest {
            task: "summarise the notes".to_string(),
            steps: vec![
                "1. [fetch] read notes.md into notes".to_string(),
                "2. [act] write summary to summary.md".to_string(),
            ],
        }
    }

    /// The question reaches the person and their answer reaches the run. It names the task in their
    /// own words and how many steps they are answering for, and says what a yes does not cover.
    #[test]
    fn a_plan_is_answered_by_whoever_typed_the_command() {
        let plan = a_plan();
        let mut shown = Vec::new();
        let decision = OneShot::new(&b"y\n"[..], &mut shown, true).confirm_manifest(&plan);

        assert_eq!(decision, Decision::Approve);
        let shown = String::from_utf8(shown).expect("the question is text");
        assert!(shown.contains(&plan.task), "the task is missing: {shown}");
        assert!(shown.contains("2 steps"), "the count is missing: {shown}");
        assert!(
            shown.contains("not approving its writes"),
            "what a yes leaves open is missing: {shown}"
        );
    }

    /// The steps are the lines the run narrated a moment earlier, from the same renderer, so the
    /// question does not print them again: the same list twice under two headings reads as two
    /// plans, and the person would be answering about the second one.
    #[test]
    fn the_question_does_not_reprint_the_narrated_plan() {
        let plan = a_plan();
        let mut shown = Vec::new();
        OneShot::new(&b"y\n"[..], &mut shown, true).confirm_manifest(&plan);

        let shown = String::from_utf8(shown).expect("the question is text");
        for step in &plan.steps {
            assert!(!shown.contains(step), "step printed twice: {shown}");
        }
    }

    /// A pipe or a redirected stderr is nobody, and then a plan is refused like every other
    /// question. The answer is not read either: bytes arriving on a pipe are whatever fed it rather
    /// than somebody agreeing, and nothing was written for them to be agreeing to.
    #[test]
    fn a_plan_is_refused_where_nobody_can_be_asked() {
        let mut shown = Vec::new();
        let decision = OneShot::new(&b"y\n"[..], &mut shown, false).confirm_manifest(&a_plan());

        assert_eq!(decision, Decision::Reject);
        assert!(shown.is_empty(), "a plan was shown to nobody: {shown:?}");
    }

    /// Both ends have to be a terminal: a plan put to a redirected stderr is a plan nobody read, and
    /// a piped stdin would be read for an answer from whatever fed it.
    #[test]
    fn a_plan_is_put_only_where_stdin_and_stderr_are_both_a_terminal() {
        assert!(a_plan_is_put(true, true));
        assert!(!a_plan_is_put(true, false), "a redirected stderr is nobody");
        assert!(!a_plan_is_put(false, true), "a piped stdin is nobody");
        assert!(!a_plan_is_put(false, false));
    }

    /// The affirmative is the only line that runs a program. End of input is not one, and neither is
    /// a longer sentence that happens to start with it.
    #[test]
    fn anything_but_yes_declines_a_plan() {
        for typed in ["n\n", "\n", "", "yes please\n", "y or n\n"] {
            let mut shown = Vec::new();
            let decision =
                OneShot::new(typed.as_bytes(), &mut shown, true).confirm_manifest(&a_plan());

            assert_eq!(decision, Decision::Reject, "{typed:?} was taken for a yes");
        }
    }

    /// Approving a plan is the whole of what this confirmer can approve. A write, a file nobody
    /// vouched for and the rest are refused exactly as they were before there was a plan question,
    /// and nothing is put on screen about them.
    #[test]
    fn a_one_shot_answers_the_plan_and_nothing_else() {
        let mut shown = Vec::new();
        let mut one_shot = OneShot::new(&b"y\ny\n"[..], &mut shown, true);

        let write = WriteRequest {
            written_since_checkout: false,
            path: "notes.md".to_string(),
            contents: "text".to_string(),
            existing: None,
            diff: bravebot_agent::diff::Diff::compute("", "text"),
            intent: bravebot_agent::Intent::Create,
            untrusted: false,
            remark: None,
            credentials: Vec::new(),
            may_always: false,
            record: None,
        };
        let vouch = VouchRequest {
            path: "notes.md".to_string(),
            preview: "text".to_string(),
            truncated: false,
            verdict: bravebot_core::vetting::Verdict::Safe,
            reason: None,
            label: String::new(),
            reason_label: None,
        };

        assert_eq!(
            one_shot.confirm_write(&write),
            bravebot_agent::WriteDecision::reject()
        );
        assert_eq!(one_shot.confirm_vouch(&vouch), Decision::Reject);
        assert!(one_shot.interjection().is_none());
        drop(one_shot);

        assert!(shown.is_empty(), "something was asked about: {shown:?}");
    }

    /// ASK-7: a question in a one-shot run is declined by the confirmer a one-shot run uses, even
    /// where there is a terminal and a "y" waiting to be read. The planner is told a reply came from
    /// a person, so an answer this confirmer invented would be taken for theirs.
    #[test]
    fn a_one_shot_declines_every_question_even_where_it_may_ask_about_a_plan() {
        let series = bravebot_core::ask::asking(&bravebot_core::ask::Series::new(vec![
            bravebot_core::ask::Question::new(
                "Cache",
                "Which cache layer?",
                vec![bravebot_core::ask::Choice::new("HTTP", None)],
                false,
            ),
            bravebot_core::ask::Question::new("Branch", "Which branch?", Vec::new(), false),
        ]));
        let mut shown = Vec::new();
        let mut one_shot = OneShot::new(&b"y\ny\n"[..], &mut shown, true);

        let answers = one_shot.ask_user(&series);
        assert!(
            answers
                .iter()
                .all(|answer| *answer == bravebot_core::ask::Answer::Declined),
            "a question was answered for the person: {answers:?}"
        );
        drop(one_shot);

        assert!(shown.is_empty(), "a question was put to nobody: {shown:?}");
    }

    /// A home directory and a checkout, each with the settings file a test writes into it.
    ///
    /// `Settings::layered` takes both rather than reading the process's own, so the layers under test
    /// are the ones this wrote and not whatever the machine running the tests has configured.
    fn layers(
        scratch: &Scratch,
        home: Option<&str>,
        project: Option<&str>,
    ) -> bravebot_config::Settings {
        let state = scratch.directory("home");
        let cwd = scratch.directory("cwd");
        if let Some(text) = home {
            std::fs::write(state.join("settings.json"), text).expect("a home layer");
        }
        if let Some(text) = project {
            std::fs::create_dir_all(cwd.join(".bravebot")).expect("a project directory");
            std::fs::write(cwd.join(".bravebot/settings.json"), text).expect("a project layer");
        }
        bravebot_config::Settings::layered(Some(state), Some(&cwd), None)
    }

    /// MODE-5 and PERM-17: the flag opens the bypass mode unless a layer in force made it
    /// unreachable, and then it is refused with the file that asked for it named. Every layer may
    /// ask, the managed one among them, and no layer lifts what another asked for (PERM-18).
    ///
    /// The failure this rejects is reading the key from the person's own file alone, which is where
    /// `vetting.auto` is read from: a machine an administrator pinned, and a checkout a team wants
    /// confined, would both be honoured nowhere.
    #[test]
    fn the_bypass_flag_is_refused_where_a_layer_made_the_mode_unreachable() {
        let asked = r#"{"permissions": {"bypassUnreachable": true}}"#;
        let lifted = r#"{"permissions": {"bypassUnreachable": false}}"#;

        let home = Scratch::new("cli-bypass-home");
        let settings = layers(&home, Some(asked), None);
        assert_eq!(
            bypass_made_unreachable(&settings, &Managed::default()).as_deref(),
            Some(home.path.join("home/settings.json").as_path()),
            "the person's own file could not ask for it"
        );

        let checkout = Scratch::new("cli-bypass-project");
        let settings = layers(&checkout, None, Some(asked));
        assert_eq!(
            bypass_made_unreachable(&settings, &Managed::default()).as_deref(),
            Some(checkout.path.join("cwd/.bravebot/settings.json").as_path()),
            "a checkout could not ask for it"
        );

        let machine = Scratch::new("cli-bypass-managed");
        let (managed, file) = pinned(&machine, asked);
        let settings = layers(&machine, None, None);
        assert_eq!(
            bypass_made_unreachable(&settings, &managed).as_deref(),
            Some(file.as_path()),
            "an administrator could not pin it"
        );

        // The strictest value any layer named: a file saying no does not lift a pin, and the refusal
        // names the file that did ask rather than the one that declined.
        let contested = Scratch::new("cli-bypass-contested");
        let (managed, file) = pinned(&contested, asked);
        let settings = layers(&contested, Some(lifted), None);
        assert_eq!(
            bypass_made_unreachable(&settings, &managed).as_deref(),
            Some(file.as_path()),
            "a settings file lifted a pin"
        );

        // And the state every run is in unless somebody wrote the key.
        let quiet = Scratch::new("cli-bypass-unasked");
        for settings in [
            layers(&quiet, None, None),
            layers(&quiet, Some(lifted), Some(lifted)),
            layers(
                &quiet,
                Some(r#"{"permissions": {"deny": ["Read(./.env)"]}}"#),
                None,
            ),
        ] {
            assert_eq!(
                bypass_made_unreachable(&settings, &Managed::default()),
                None,
                "the flag was refused where no layer asked for it"
            );
        }
    }

    /// Absent is the state every run is in unless somebody typed the flag, and it is the only state
    /// in which a write is put to a person first.
    #[test]
    fn permissions_are_enforced_unless_the_flag_is_given() {
        let mut arguments = args(&["do a thing"]);
        assert!(!take_flag(&mut arguments, SKIP_PERMISSIONS));
        assert_eq!(arguments, args(&["do a thing"]));
    }

    /// Taken out wherever it appears, so the parser downstream sees only what it already understood.
    /// Left in, it would come back as "unexpected argument" from whichever parser met it.
    #[test]
    fn the_skip_permissions_flag_is_taken_out_wherever_it_appears() {
        for typed in [
            &["--dangerously-skip-permissions", "-p", "do a thing"][..],
            &["-p", "--dangerously-skip-permissions", "do a thing"][..],
            &["-p", "do a thing", "--dangerously-skip-permissions"][..],
        ] {
            let mut arguments = args(typed);
            assert!(
                take_flag(&mut arguments, SKIP_PERMISSIONS),
                "{typed:?} did not engage it"
            );
            assert_eq!(
                arguments,
                args(&["-p", "do a thing"]),
                "left over: {typed:?}"
            );
        }
    }

    /// It belongs to every way of starting, not only to a one-shot run, so stripping it must leave a
    /// resume or a bare interactive invocation still recognisable to the dispatch below.
    #[test]
    fn the_flag_leaves_every_other_way_of_starting_intact() {
        let mut arguments = args(&[
            "--resume",
            "1787860306-65099",
            "--dangerously-skip-permissions",
        ]);
        assert!(take_flag(&mut arguments, SKIP_PERMISSIONS));
        assert_eq!(arguments, args(&["--resume", "1787860306-65099"]));

        // Nothing but the flag is an interactive session, not an unknown option.
        let mut alone = args(&["--dangerously-skip-permissions"]);
        assert!(take_flag(&mut alone, SKIP_PERMISSIONS));
        assert!(alone.is_empty());
    }

    /// It composes with the other flag that is taken out before dispatch, in either order: both are
    /// about the whole run rather than about a task, and somebody may well want both.
    #[test]
    fn it_composes_with_incognito() {
        for typed in [
            &["--incognito", "--dangerously-skip-permissions", "-p", "x"][..],
            &["--dangerously-skip-permissions", "--incognito", "-p", "x"][..],
        ] {
            let mut arguments = args(typed);
            assert!(take_flag(&mut arguments, INCOGNITO), "{typed:?}");
            assert!(take_flag(&mut arguments, SKIP_PERMISSIONS), "{typed:?}");
            assert_eq!(arguments, args(&["-p", "x"]), "left over: {typed:?}");
        }
    }

    /// The setting is taken out wherever it was typed, with its word, and a word that is neither
    /// is refused rather than read as `open`.
    #[test]
    fn the_run_network_flag_is_taken_out_with_its_word_and_refuses_any_other() {
        use bravebot_sandbox::network::Network;
        for typed in [
            &["--run-network", "closed", "-p", "do a thing"][..],
            &["-p", "--run-network", "closed", "do a thing"][..],
            &["-p", "do a thing", "--run-network", "closed"][..],
        ] {
            let mut arguments = args(typed);
            assert_eq!(
                take_run_network(&mut arguments),
                Ok(Some(Network::Closed)),
                "{typed:?}"
            );
            assert_eq!(arguments, args(&["-p", "do a thing"]), "{typed:?}");
        }
        let mut twice = args(&["--run-network", "closed", "--run-network", "open"]);
        assert_eq!(take_run_network(&mut twice), Ok(Some(Network::Open)));
        let mut none = args(&["-p", "do a thing"]);
        assert_eq!(take_run_network(&mut none), Ok(None));
        for typed in [
            &["-p", "x", "--run-network"][..],
            &["--run-network", "Closed", "-p", "x"][..],
            &["--run-network", "off", "-p", "x"][..],
        ] {
            let mut arguments = args(typed);
            assert!(take_run_network(&mut arguments).is_err(), "{typed:?}");
            assert_eq!(arguments, args(typed), "a refusal rewrote the list");
        }
    }

    /// Each of the four flags is taken out wherever it was typed, with its path, into the list it
    /// names, any number of times, and what is left is the invocation without them. A flag with no
    /// path after it, or a blank one, is refused and the arguments are left as they were: read as the
    /// next flag, it would be an entry nobody wrote.
    #[test]
    fn the_sandbox_flags_are_taken_out_with_their_paths_into_their_lists() {
        let mut arguments = args(&[
            "--sandbox-deny-read",
            "~/.config/gh",
            "-p",
            "--sandbox-allow-write",
            "/data/out",
            "do a thing",
            "--sandbox-deny-read",
            "**/*.env",
            "--sandbox-allow-read",
            "~/.aws",
            "--sandbox-deny-write",
            ".env",
        ]);
        let lists = take_sandbox_filesystem(&mut arguments).expect("well formed");
        let paths = |entries: &[bravebot_sandbox::rules::Entry]| {
            entries
                .iter()
                .map(|entry| entry.path.clone())
                .collect::<Vec<_>>()
        };

        assert_eq!(arguments, args(&["-p", "do a thing"]));
        assert_eq!(paths(&lists.deny_read), ["~/.config/gh", "**/*.env"]);
        assert_eq!(paths(&lists.allow_read), ["~/.aws"]);
        assert_eq!(paths(&lists.allow_write), ["/data/out"]);
        assert_eq!(paths(&lists.deny_write), [".env"]);
        assert!(lists.deny_read.iter().all(|entry| entry.by.is_none()));

        for typed in [
            &["-p", "x", "--sandbox-deny-read"][..],
            &["--sandbox-allow-write", "  ", "-p", "x"][..],
        ] {
            let mut arguments = args(typed);
            assert!(
                take_sandbox_filesystem(&mut arguments).is_err(),
                "{typed:?}"
            );
            assert_eq!(arguments, args(typed), "a refusal rewrote the list");
        }
    }

    /// The file is taken out wherever it was typed, with the argument that belongs to it, and what
    /// is left is the invocation somebody would have typed without it. Left in, the flag would come
    /// back as an unknown option from whichever parser met it, and its path as a second prompt.
    #[test]
    fn the_settings_flag_is_taken_out_with_the_file_it_named() {
        for typed in [
            &["--settings", "/etc/ci.json", "-p", "do a thing"][..],
            &["-p", "--settings", "/etc/ci.json", "do a thing"][..],
            &["-p", "do a thing", "--settings", "/etc/ci.json"][..],
        ] {
            let mut arguments = args(typed);
            assert_eq!(
                take_settings(&mut arguments).expect("names a file"),
                Some(PathBuf::from("/etc/ci.json")),
                "{typed:?} named no file"
            );
            assert_eq!(
                arguments,
                args(&["-p", "do a thing"]),
                "left over: {typed:?}"
            );
        }
    }

    /// It belongs to every way of starting rather than to a one-shot run, so taking it out has to
    /// leave a resume and a bare interactive invocation recognisable to the dispatch.
    #[test]
    fn a_named_settings_file_leaves_every_other_way_of_starting_intact() {
        let mut arguments = args(&["--resume", "1787860306-65099", "--settings", "/etc/ci.json"]);
        assert!(take_settings(&mut arguments).is_ok());
        assert_eq!(arguments, args(&["--resume", "1787860306-65099"]));

        // Nothing but the flag and its file is an interactive session, not an unknown option.
        let mut alone = args(&["--settings", "/etc/ci.json"]);
        assert!(take_settings(&mut alone).is_ok());
        assert!(alone.is_empty());
    }

    /// A repeat resolves the way `--mode` and `--model` already resolve one, and both are taken
    /// out: one left behind would reach a parser that has never heard of it.
    #[test]
    fn the_last_settings_file_named_is_the_one_read() {
        let mut arguments = args(&[
            "--settings",
            "/etc/first.json",
            "--settings",
            "/etc/second.json",
            "-p",
            "do a thing",
        ]);
        assert_eq!(
            take_settings(&mut arguments).expect("names a file"),
            Some(PathBuf::from("/etc/second.json"))
        );
        assert_eq!(arguments, args(&["-p", "do a thing"]));
    }

    /// The flag with nothing after it, and the flag with a path that expanded to nothing, are both
    /// refused rather than read as no flag at all, and the arguments are left as they were typed
    /// rather than half consumed.
    #[test]
    fn a_settings_flag_with_no_path_is_refused() {
        for typed in [
            &["-p", "do a thing", "--settings"][..],
            &["--settings", "   ", "-p", "do a thing"][..],
        ] {
            let mut arguments = args(typed);
            assert!(
                take_settings(&mut arguments).is_err(),
                "{typed:?} was accepted"
            );
            assert_eq!(arguments, args(typed), "the arguments changed: {typed:?}");
        }
    }

    /// It composes with the other two flags taken out before dispatch, in any order: all three are
    /// about the whole run rather than about a task, and a job that wants one may well want another.
    /// What is left is the way of starting, whichever one it is, so a mode that refuses every
    /// argument beside itself still reaches dispatch alone.
    #[test]
    fn a_named_settings_file_composes_with_the_other_flags_before_dispatch() {
        for (typed, left) in [
            (
                &[
                    "--incognito",
                    "--settings",
                    "/etc/ci.json",
                    "--dangerously-skip-permissions",
                    "-p",
                    "x",
                ][..],
                &["-p", "x"][..],
            ),
            (
                &[
                    "--settings",
                    "/etc/ci.json",
                    "--dangerously-skip-permissions",
                    "--incognito",
                    "-p",
                    "x",
                ][..],
                &["-p", "x"][..],
            ),
            (
                &[
                    "--settings",
                    "/etc/ci.json",
                    "--incognito",
                    "--dangerously-skip-permissions",
                    "--plain",
                ][..],
                &["--plain"][..],
            ),
        ] {
            let mut arguments = args(typed);
            assert!(take_flag(&mut arguments, INCOGNITO), "{typed:?}");
            assert!(take_flag(&mut arguments, SKIP_PERMISSIONS), "{typed:?}");
            assert_eq!(
                take_settings(&mut arguments).expect("names a file"),
                Some(PathBuf::from("/etc/ci.json")),
                "{typed:?}"
            );
            assert_eq!(arguments, args(left), "left over: {typed:?}");
        }
    }

    /// The name is taken out wherever it was typed, with the argument that belongs to it, and
    /// beside every other flag taken out before dispatch. Left in, `--plain` would refuse it as
    /// another argument and `-p` would read the name as the start of its prompt.
    #[test]
    fn the_agent_flag_is_taken_out_with_the_name_it_gave() {
        for (typed, left) in [
            (
                &["--agent", "reviewer", "-p", "do a thing"][..],
                &["-p", "do a thing"][..],
            ),
            (
                &["-p", "do a thing", "--agent", "reviewer"][..],
                &["-p", "do a thing"][..],
            ),
            (&["--plain", "--agent", "reviewer"][..], &["--plain"][..]),
            (&["--agent", "reviewer"][..], &[][..]),
            (
                &[
                    "--incognito",
                    "--agent",
                    "reviewer",
                    "--settings",
                    "/etc/ci.json",
                    "--plain",
                ][..],
                &["--plain"][..],
            ),
        ] {
            let mut arguments = args(typed);
            take_flag(&mut arguments, INCOGNITO);
            take_settings(&mut arguments).expect("names a file or none");
            assert_eq!(
                take_agent(&mut arguments).expect("names a definition"),
                Some("reviewer".to_string()),
                "{typed:?} named no definition"
            );
            assert_eq!(arguments, args(left), "left over: {typed:?}");
        }
    }

    /// The last name is used, as with `--settings`, because every turn can work under only one
    /// definition. A second flag left in the arguments would reach a parser that does not know it.
    #[test]
    fn the_last_definition_named_is_the_one_worked_under() {
        let mut arguments = args(&["--agent", "first", "--agent", "second", "-p", "x"]);
        assert_eq!(
            take_agent(&mut arguments).expect("names a definition"),
            Some("second".to_string())
        );
        assert_eq!(arguments, args(&["-p", "x"]));
    }

    /// A missing name and a name that expanded to nothing are refused. Treating either as no flag
    /// would run the planner without telling anyone. So is a flag where the name should be: taken
    /// as the name, `--json` would be removed and the run would answer in the other format. The
    /// arguments are left as typed.
    #[test]
    fn an_agent_flag_with_no_name_is_refused() {
        for typed in [
            &["-p", "do a thing", "--agent"][..],
            &["--agent", "  ", "-p", "do a thing"][..],
            &["--agent", "--json", "-p", "do a thing"][..],
            &["--agent", "-p", "do a thing"][..],
        ] {
            let mut arguments = args(typed);
            assert!(
                take_agent(&mut arguments).is_err(),
                "{typed:?} was accepted"
            );
            assert_eq!(arguments, args(typed), "the arguments changed: {typed:?}");
        }
    }

    /// A command that starts no session has nothing to address, so the name is refused rather than
    /// silently dropped. A way of picking up a recorded session has something to address: the
    /// record names the definition, and a name given with the flag replaces it (CLI-17).
    #[test]
    fn a_definition_is_refused_where_nothing_would_work_under_it() {
        for first in [
            "doctor",
            "update",
            "bug-report",
            "auth",
            "mcp",
            "permissions",
            "sessions",
            "attach",
            "reply",
            "import-leo-creds",
            "import-providers",
            "completion",
            "shell-init",
        ] {
            assert!(
                without_a_definition(Some(first)).is_some(),
                "{first} took a definition"
            );
        }
        for first in [
            None,
            Some("-p"),
            Some("--plain"),
            Some("--model"),
            Some("a task"),
            Some("--resume"),
            Some("-r"),
            Some("--continue"),
            Some("-c"),
            Some("--fork"),
            Some("-f"),
            Some("--from-pr"),
        ] {
            assert_eq!(without_a_definition(first), None, "{first:?} was refused");
        }
    }

    /// Each flag is removed wherever it is typed and the words it gave reach the right field, so
    /// what remains is the task alone and `--system-prompt` does not become `--append-system-prompt`.
    #[test]
    fn the_system_prompt_flags_are_taken_out_with_the_words_they_gave() {
        let mut arguments = args(&[
            "--append-system-prompt",
            "answer in French",
            "-p",
            "do a thing",
            "--system-prompt",
            "You are a reviewer.",
        ]);
        let prompts = take_system_prompts(&mut arguments).expect("both flags parse");
        assert_eq!(prompts.replacing.as_deref(), Some("You are a reviewer."));
        assert_eq!(prompts.appending.as_deref(), Some("answer in French"));
        assert_eq!(arguments, args(&["-p", "do a thing"]));

        let mut arguments = args(&["-p", "do a thing"]);
        assert_eq!(
            take_system_prompts(&mut arguments),
            Ok(SystemPrompts::default())
        );
        assert_eq!(arguments, args(&["-p", "do a thing"]));
    }

    /// Of two of the same flag the last is used, for each flag independently.
    #[test]
    fn the_last_system_prompt_named_is_the_one_used() {
        let mut arguments = args(&[
            "--system-prompt",
            "first opening",
            "--append-system-prompt",
            "first addition",
            "--system-prompt",
            "second opening",
            "--append-system-prompt",
            "second addition",
            "-p",
            "x",
        ]);
        let prompts = take_system_prompts(&mut arguments).expect("parses");
        assert_eq!(prompts.replacing.as_deref(), Some("second opening"));
        assert_eq!(prompts.appending.as_deref(), Some("second addition"));
        assert_eq!(arguments, args(&["-p", "x"]));
    }

    /// A missing value, a blank one and a flag where the words should be are refused with the
    /// arguments as typed. Taken as the words, `--json` would be removed and the run would answer
    /// in the other format.
    #[test]
    fn a_system_prompt_flag_with_no_words_is_refused() {
        for typed in [
            &["-p", "do a thing", "--system-prompt"][..],
            &["-p", "do a thing", "--append-system-prompt"][..],
            &["--system-prompt", "  ", "-p", "do a thing"][..],
            &["--append-system-prompt", "", "-p", "do a thing"][..],
            &["--system-prompt", "--json", "-p", "do a thing"][..],
            &["--append-system-prompt", "-p", "do a thing"][..],
        ] {
            let mut arguments = args(typed);
            let refused =
                take_system_prompts(&mut arguments).expect_err(&format!("{typed:?} was accepted"));
            let typed_flag = typed
                .iter()
                .find(|part| part.ends_with("system-prompt"))
                .expect("every case types a flag");
            assert!(
                refused.contains(typed_flag),
                "the refusal does not name {typed_flag}: {refused}"
            );
            assert_eq!(arguments, args(typed), "the arguments changed: {typed:?}");
        }
    }

    /// A sentence that opens with a dash holds a space and is words, which is what tells it from
    /// the next flag.
    #[test]
    fn words_that_open_with_a_dash_are_taken_when_they_are_a_sentence() {
        let mut arguments = args(&["--append-system-prompt", "- always use rust", "-p", "x"]);
        let prompts = take_system_prompts(&mut arguments).expect("a sentence is words");
        assert_eq!(prompts.appending.as_deref(), Some("- always use rust"));
        assert_eq!(arguments, args(&["-p", "x"]));
    }

    /// The commands that start neither a session nor a task are refused, so the words are never
    /// silently dropped. A session, a resumed one and a one-shot run take them.
    #[test]
    fn the_system_prompt_flags_are_refused_where_nothing_would_use_them() {
        for first in [
            "doctor",
            "update",
            "bug-report",
            "auth",
            "mcp",
            "permissions",
            "sessions",
            "attach",
            "reply",
            "import-leo-creds",
            "import-providers",
            "completion",
            "shell-init",
        ] {
            let refused = without_a_prompt_to_give("--system-prompt", Some(first))
                .unwrap_or_else(|| panic!("{first} took the words"));
            assert!(
                refused.contains("--system-prompt") && refused.contains(first),
                "the refusal names neither the flag nor the command: {refused}"
            );
        }
        for first in [
            None,
            Some("-p"),
            Some("--plain"),
            Some("--resume"),
            Some("--continue"),
            Some("--fork"),
            Some("a task"),
        ] {
            assert_eq!(
                without_a_prompt_to_give("--system-prompt", first),
                None,
                "{first:?} was refused"
            );
        }
    }

    #[test]
    fn a_model_flag_names_the_model_a_run_asks_for() {
        let invocation =
            parse_invocation(&args(&["--model", "some-model", "do a thing"])).expect("parses");
        assert_eq!(invocation.model.as_deref(), Some("some-model"));
        assert_eq!(invocation.prompt, "do a thing");
    }

    /// A run that named no model leaves the configured one in force, which is the whole of how
    /// `--model` ranks above configuration without standing in for it.
    #[test]
    fn a_run_that_named_no_model_names_nothing() {
        let invocation = parse_invocation(&args(&["do a thing"])).expect("parses");
        assert_eq!(invocation.model, None);
    }

    /// The name is carried as typed, since what a tier word and an older spelling of the routing
    /// entry name is a question for the configuration and there is none at parse.
    #[test]
    fn a_model_name_is_carried_as_it_was_typed() {
        let invocation =
            parse_invocation(&args(&["--model", "automatic", "do a thing"])).expect("parses");
        assert_eq!(invocation.model.as_deref(), Some("automatic"));
    }

    /// A tier word in the settings key this flag outranks resolves to a model that exists, so a
    /// flag that sent the word as written would refuse a spelling the file it overrides takes and
    /// be answered by whatever the service substitutes for a name it has never heard of.
    #[test]
    fn a_tier_word_on_the_command_line_names_the_model_the_settings_key_would() {
        let config = bravebot_config::Config::from_lookup(|key| match key {
            "BRAVE_AI_CHAT_ENDPOINT" => Some("https://example.invalid".into()),
            "BRAVE_SERVICES_KEY_ID" => Some("test-key-id".into()),
            "SERVICES_KEY_AICHAT" => Some("test-signing-key".into()),
            _ => None,
        })
        .expect("a configuration");

        let named = parse_invocation(&args(&["--model", "opus", "do a thing"]))
            .expect("parses")
            .model
            .map(|name| config.model_named(&name));

        assert_eq!(
            named.as_deref(),
            Some(bravebot_config::bedrock::Tier::Opus.brave_model())
        );
    }

    #[test]
    fn a_model_flag_with_no_name_is_refused() {
        let err = parse_invocation(&args(&["--model"])).expect_err("must refuse");
        assert!(err.contains("--model"), "{err}");
    }

    /// A script that computed an empty variable asked for a model. Reading the blank as no choice
    /// would answer it with whatever was configured and say nothing, which is the substitution
    /// this flag exists to make impossible.
    #[test]
    fn a_blank_model_is_refused_rather_than_read_as_no_choice() {
        for typed in [
            args(&["--model", "", "do a thing"]),
            args(&["--model", "   ", "do a thing"]),
        ] {
            let err = parse_invocation(&typed).expect_err("must refuse");
            assert!(err.contains("--model"), "{typed:?}: {err}");
        }
    }

    /// `--output-schema` names the file holding the schema, and a run that gave none has none.
    #[test]
    fn an_output_schema_flag_names_the_file_holding_the_schema() {
        let parsed = parse_invocation(&args(&["--output-schema", "verdict.json", "do a thing"]))
            .expect("parses");
        assert_eq!(parsed.output_schema.as_deref(), Some("verdict.json"));
        assert_eq!(parsed.prompt, "do a thing");
        let none = parse_invocation(&args(&["do a thing"])).expect("parses");
        assert_eq!(none.output_schema, None);
    }

    #[test]
    fn an_output_schema_flag_naming_no_file_is_refused() {
        for typed in [
            args(&["--output-schema"]),
            args(&["--output-schema", "", "do a thing"]),
            args(&["--output-schema", "   ", "do a thing"]),
        ] {
            let err = parse_invocation(&typed).expect_err("refused");
            assert!(err.contains("--output-schema"), "{typed:?}: {err}");
        }
    }

    /// The advisor is named on the command line, separately from the model, and a run that did not
    /// name one has none.
    #[test]
    fn an_advisor_flag_names_the_model_the_planner_may_consult() {
        let invocation =
            parse_invocation(&args(&["--advisor", "big-model", "do a thing"])).expect("parses");
        assert_eq!(invocation.advisor.as_deref(), Some("big-model"));
        assert_eq!(
            invocation.model, None,
            "the advisor replaced the run's model"
        );
        assert_eq!(invocation.prompt, "do a thing");

        let invocation = parse_invocation(&args(&["do a thing"])).expect("parses");
        assert_eq!(invocation.advisor, None);
    }

    /// PANEL-12. `--issue` and `--pr` are taken out of the line wherever they stand, give each its
    /// own link, and let the last of two of a kind win.
    #[test]
    fn issue_and_pr_flags_are_taken_out_and_set_their_own_link() {
        use bravebot_session::sessions::Url;
        const ISSUE: &str = "https://github.com/brave/bravebot/issues/1267";
        const OTHER: &str = "https://github.com/brave/bravebot/issues/9";
        const PULL: &str = "https://github.com/brave/bravebot/pull/1270";
        let mut line = args(&[
            "--issue", OTHER, "-i", "fix it", "--pr", PULL, "--issue", ISSUE,
        ]);
        let links = take_links(&mut line).expect("links");
        assert_eq!(links.issue, Url::read(ISSUE));
        assert_eq!(links.pull_request, Url::read(PULL));
        assert_eq!(line, args(&["-i", "fix it"]));

        let mut line = args(&["--pr", PULL]);
        let links = take_links(&mut line).expect("links");
        assert_eq!(links.issue, None, "--pr set the issue");
        assert!(line.is_empty());
    }

    /// PANEL-12. A value that `/issue` and `/pr` would refuse, or none, stops the line, and the
    /// sentence does not repeat what was typed, which may hold the escape that made it no link.
    #[test]
    fn a_value_that_is_not_one_web_address_is_refused_and_not_repeated() {
        for flag in ["--issue", "--pr"] {
            for value in [
                "https://example.com/a\nhttps://example.com/b",
                "https://example.com/\u{1b}]0;owned\u{7}",
                "ftp://example.com/pull/1",
                "not a link",
                "https:///pull/1",
                "",
            ] {
                let complaint = take_links(&mut args(&[flag, value])).expect_err("refused");
                assert!(complaint.contains(flag), "{flag} {value:?}: {complaint}");
                assert!(
                    value.is_empty() || !complaint.contains(value),
                    "{flag} {value:?}: {complaint}"
                );
            }
            let complaint = take_links(&mut args(&[flag])).expect_err("refused");
            assert!(complaint.contains(flag), "{complaint}");
        }
    }

    /// The links go to a session that opens, and a line that opens none refuses them rather than
    /// dropping them.
    #[test]
    fn only_a_line_that_opens_a_session_takes_links() {
        for opens in [
            &[][..],
            &["-i", "fix it"],
            &["--resume", "abc"],
            &["--resume"],
            &["-c"],
            &["--fork", "abc"],
            &["--from-pr", "12"],
        ] {
            assert!(opens_a_session(&args(opens)), "{opens:?}");
        }
        for opens_none in [
            &["fix it"][..],
            &["-p", "fix it"],
            &["--resume", "abc", "-p", "go"],
            &["--bg", "fix it"],
            &["--plain"],
            &["doctor"],
            &["--mode", "manifest", "go"],
        ] {
            assert!(!opens_a_session(&args(opens_none)), "{opens_none:?}");
        }
    }

    /// A script that computed an empty variable asked for an advisor, and running without one would
    /// not say so.
    #[test]
    fn a_blank_advisor_is_refused_rather_than_read_as_no_choice() {
        for typed in [
            args(&["--advisor"]),
            args(&["--advisor", "", "do a thing"]),
            args(&["--advisor", "   ", "do a thing"]),
        ] {
            let err = parse_invocation(&typed).expect_err("must refuse");
            assert!(err.contains("--advisor"), "{typed:?}: {err}");
        }
    }

    /// CLI-21. A machine whose managed settings refuse a model does not request it for an advisor
    /// either, and the run stops before the first round naming the file that refused it, rather than
    /// starting with an advisor whose first question could not be sent.
    ///
    /// Not a binary run, because the managed file is read from a fixed system path that a test
    /// cannot point elsewhere. The configuration is built from a managed file the test writes.
    #[test]
    fn an_advisor_the_managed_settings_refuse_is_refused_before_the_run() {
        let scratch = Scratch::new("cli-advisor-managed");
        let (managed, file) = pinned(&scratch, r#"{"models": {"deny": ["stub/advisor-model"]}}"#);
        let settings = layers(
            &scratch,
            Some(
                r#"{"provider": {"stub": {"options": {"baseURL": "http://127.0.0.1:1/v1"},
                    "models": {"planner": {}, "advisor-model": {}}}}}"#,
            ),
            None,
        );
        let config = Config::from_env_and_settings(&settings, &managed).expect("a configuration");

        let how = resolve_advisor(&config, "stub/advisor-model").expect_err("must be refused");
        assert!(
            how.contains("stub/advisor-model") && how.contains(&file.display().to_string()),
            "the refusal named neither the model nor the file that refused it: {how}"
        );
        assert_eq!(
            resolve_advisor(&config, "stub/planner").as_deref(),
            Ok("stub/planner"),
            "a model the file does not name was refused"
        );
    }

    /// The level a run asks for, in any case, and nothing where the flag was not given, which
    /// leaves the settings and the saved pick to answer.
    #[test]
    fn an_effort_flag_names_the_level_a_run_asks_for() {
        let invocation =
            parse_invocation(&args(&["--effort", "XHigh", "do a thing"])).expect("parses");
        assert_eq!(
            invocation.effort,
            Some(bravebot_session::store::Effort::Xhigh)
        );
        assert_eq!(invocation.prompt, "do a thing");

        let invocation = parse_invocation(&args(&["do a thing"])).expect("parses");
        assert_eq!(invocation.effort, None);
    }

    /// A word that is no level must not reach a request field, and read as no choice it would
    /// hand a script that asked for one whatever the settings said without telling it. The refusal
    /// names the levels, since the word typed is the one thing the person got wrong.
    #[test]
    fn an_effort_flag_naming_no_level_is_refused() {
        for typed in [
            args(&["--effort"]),
            args(&["--effort", "", "do a thing"]),
            args(&["--effort", "  ", "do a thing"]),
            args(&["--effort", "highest", "do a thing"]),
        ] {
            let err = parse_invocation(&typed).expect_err("must refuse");
            assert!(err.contains("--effort"), "{typed:?}: {err}");
            assert!(err.contains("xhigh"), "{typed:?}: {err}");
        }
    }

    /// The flag names a model for one run, which is the only thing a script can pin a model
    /// against a choice recorded elsewhere with.
    #[test]
    fn the_command_line_outranks_the_record_a_session_would_read() {
        assert_eq!(
            model_asked_for(
                Some("named-on-the-command-line".into()),
                Some("chosen".into())
            )
            .as_deref(),
            Some("named-on-the-command-line")
        );
    }

    /// A run that named no model asks for what a session opening in the same directory would, so
    /// reaching a model somebody already chose needs no interactive step and no flag.
    #[test]
    fn a_run_that_named_no_model_reads_the_record_a_session_would() {
        assert_eq!(
            model_asked_for(None, Some("chosen".into())).as_deref(),
            Some("chosen")
        );
    }

    /// Nothing recorded and nothing named leaves the configured model in force, which is what a
    /// person who has never picked one gets in either surface.
    #[test]
    fn a_run_with_nothing_to_go_on_leaves_the_configured_model_in_force() {
        assert_eq!(model_asked_for(None, None), None);
    }

    /// A run asked for a model and was answered by another. Nothing on stdout says so, and a
    /// model is pinned for a reason whichever route pinned it.
    #[test]
    fn a_model_asked_for_and_not_served_is_reported() {
        let complaint = model_not_served("a-premium-model", true, "a-free-one")
            .expect("a complaint about the substitution");
        assert!(complaint.contains("a-premium-model"), "{complaint}");
        assert!(complaint.contains("a-free-one"), "{complaint}");
    }

    /// The routing entry asks for whichever model the server picks, so a concrete name coming back
    /// is that name working rather than a model standing in for another.
    #[test]
    fn a_routing_entry_answered_by_a_model_is_not_a_substitution() {
        assert_eq!(
            model_not_served(bravebot_config::DEFAULT_MODEL, true, "the-model-picked"),
            None
        );
    }

    /// A backend asked by an opaque handle answers with a name that never matched what went in, so
    /// comparing them would fail every run ever made against one.
    #[test]
    fn a_backend_that_does_not_report_what_it_was_asked_is_not_compared() {
        assert_eq!(
            model_not_served("an-opaque-handle", false, "some-model"),
            None
        );
    }

    #[test]
    fn a_model_that_answered_as_asked_is_no_complaint() {
        assert_eq!(model_not_served("same-model", true, "same-model"), None);
    }

    /// The complaint is about the run rather than part of what the run produced, so a pipe of
    /// stdout carries the reply and nothing else whichever way the run went.
    #[test]
    fn a_substituted_model_is_reported_beside_the_reply_never_in_it() {
        let (reply, beside) = written(&Finished {
            reply: "ok",
            notices: &[],
            attempt: None,
            trail: None,
            clean: true,
            ending: Ending::Failed,
            not_served: Some("a-premium-model was not served"),
        });

        assert_eq!(reply, "ok\n");
        assert!(
            beside.contains("BB1001: a-premium-model was not served"),
            "{beside}"
        );
    }

    /// The status is the only part of a finished run a script is certain to read, so a model the
    /// command line named and did not get has to reach it. A turn nothing refused is not enough on
    /// its own.
    #[test]
    fn a_run_answered_by_a_model_other_than_the_one_it_named_does_not_succeed() {
        let substituted = ending_of_a_turn(true, true, true);
        assert!(!substituted.ok());
        assert_eq!(substituted.status(), 1);

        assert!(ending_of_a_turn(true, true, false).ok());
    }

    /// A turn that was refused something did not do what it was asked, however much of a reply it
    /// produced on the way. The refusal is a line on stderr and the reply still goes to stdout, so
    /// a script piping one command into the next has only the status to tell it that the work it
    /// asked for did not all happen.
    ///
    /// Its own status rather than the catch-all, because a policy refusal is the one failure that
    /// needs a person rather than another attempt.
    #[test]
    fn a_turn_something_was_refused_in_does_not_succeed() {
        let refused = ending_of_a_turn(false, false, false);
        assert!(!refused.ok());
        assert_eq!(refused, Ending::Refused);
        assert_eq!(refused.status(), 4);

        assert!(ending_of_a_turn(true, false, false).ok());
    }

    /// Below the flag the model is whatever was recorded or configured, so failing here would have
    /// a script that names no model exit non-zero over a choice made in a terminal. It is still
    /// reported, which is the whole of what a person needs to see it.
    #[test]
    fn a_substitution_the_command_line_did_not_ask_for_is_reported_and_not_failed() {
        let substituted = Finished {
            reply: "ok",
            notices: &[],
            attempt: None,
            trail: None,
            clean: true,
            ending: Ending::Done,
            not_served: Some("a-premium-model was not served"),
        };

        let (reply, beside) = written(&substituted);
        assert_eq!(reply, "ok\n");
        assert!(beside.contains("was not served"), "{beside}");
        // Not a failure here, so nothing claims the identifier of one.
        assert!(!beside.contains("BB1"), "{beside}");
        assert!(ending_of_a_turn(true, false, true).ok());
    }

    /// NAME-10: `--session-ref` takes an id, once however often it is written, and refuses a
    /// missing or blank one.
    #[test]
    fn a_session_ref_flag_names_a_session_once() {
        let invocation = parse_invocation(&args(&[
            "--session-ref",
            "abc-123",
            "do a thing",
            "--session-ref",
            " abc-123 ",
            "--session-ref",
            "def-456",
        ]))
        .expect("parses");
        assert_eq!(invocation.session_refs, ["abc-123", "def-456"]);
        for typed in [
            args(&["--session-ref"]),
            args(&["--session-ref", "  ", "x"]),
        ] {
            let err = parse_invocation(&typed).expect_err("refused");
            assert!(err.contains("--session-ref"), "{typed:?}: {err}");
        }
    }

    #[test]
    fn a_directory_flag_names_a_directory_the_run_may_reach() {
        let invocation = parse_invocation(&args(&["--add-dir", "/somewhere/else", "do a thing"]))
            .expect("parses");
        assert_eq!(invocation.directories, vec!["/somewhere/else".to_string()]);
        assert_eq!(invocation.prompt, "do a thing");
    }

    /// Reaching one sibling checkout is no more natural than reaching two, and a flag that could
    /// only be given once would be a rule about typing.
    #[test]
    fn the_directory_flag_is_repeatable() {
        let invocation = parse_invocation(&args(&[
            "--add-dir",
            "/one",
            "--add-dir",
            "/two",
            "do a thing",
        ]))
        .expect("parses");
        assert_eq!(
            invocation.directories,
            vec!["/one".to_string(), "/two".to_string()]
        );
    }

    #[test]
    fn a_directory_flag_with_no_path_is_refused() {
        for typed in [
            args(&["--add-dir"]),
            args(&["--add-dir", "  ", "do a thing"]),
        ] {
            let err = parse_invocation(&typed).expect_err("must refuse");
            assert!(err.contains("--add-dir"), "{typed:?}: {err}");
        }
    }

    /// The point of the flag: a file outside the working directory is unreachable until one is
    /// opened, and reachable afterwards.
    #[test]
    fn a_directory_the_command_line_named_is_reachable() {
        let scratch = Scratch::new("add-dir-reachable");
        let project = scratch.directory("project");
        let beside = scratch.directory("beside");
        let file = beside.join("notes.md");
        std::fs::write(&file, "notes").expect("write");

        let mut workspace = Workspace::new(project).expect("a workspace");
        assert!(
            workspace.confines(&file).is_err(),
            "reachable before it was opened"
        );
        open_directories(&mut workspace, &[beside.display().to_string()]).expect("opens");
        assert!(workspace.confines(&file).is_ok(), "not reachable after");
    }

    /// CHECKOUT-7. `--add-dir` of a directory holding the working directory says so, since a run
    /// is where nobody is watching to find out from a refused spawn; one beside it says nothing.
    #[test]
    fn a_directory_holding_the_working_directory_is_said_to_end_checkouts() {
        let scratch = Scratch::new("add-dir-ends-checkouts");
        let project = scratch.directory("project");
        let beside = scratch.directory("beside");
        let mut workspace = Workspace::new(project).expect("a workspace");

        let notices =
            open_directories(&mut workspace, &[beside.display().to_string()]).expect("opens");
        assert!(notices.is_empty(), "{notices:?}");
        let notices =
            open_directories(&mut workspace, &[scratch.path.display().to_string()]).expect("opens");
        assert_eq!(notices.len(), 1, "{notices:?}");
        assert!(
            notices[0].contains("no delegate is given a checkout while it is open"),
            "{notices:?}"
        );
    }

    /// A script that asked to reach a directory and did not would otherwise fail somewhere further
    /// in, over a file it was told it could open.
    #[test]
    fn a_directory_that_cannot_be_opened_stops_the_run() {
        let scratch = Scratch::new("add-dir-missing");
        let mut workspace = Workspace::new(scratch.directory("project")).expect("a workspace");
        let absent = scratch.path.join("not-here");

        let err = open_directories(&mut workspace, &[absent.display().to_string()])
            .expect_err("must refuse");
        assert!(err.contains("not-here"), "{err}");
    }

    /// CLI-31. The prompt reaches the session as written. A prompt a script assembled holds line
    /// breaks and indentation that are part of it, and joining the words after the flag, as `--bg`
    /// does, would send a different prompt from the one the script built.
    #[test]
    fn the_prompt_a_session_opens_on_is_the_argument_as_written() {
        let written = "Fix the failing test.\n\n  - run it alone first\n  - then the suite";
        assert_eq!(
            prompt_to_open_on(&args(&[written])),
            Ok((written.to_string(), AskedFor::default()))
        );
        // A sentence opening with `-` holds a space, so it is a prompt and not the next flag.
        assert_eq!(
            prompt_to_open_on(&args(&["- one item"])),
            Ok(("- one item".to_string(), AskedFor::default()))
        );
    }

    /// CLI-31. A line giving `-i` no prompt is refused rather than opening an empty session. A script
    /// whose variable expanded to nothing asked for a task, and a session waiting for somebody to
    /// type one leaves it undone with nothing said. A flag after `-i` is the next flag, and taken as
    /// the prompt it would send `--json` to the planner as the person's words.
    #[test]
    fn a_session_given_no_prompt_is_refused() {
        let refused = t!(cli_interactive_needs_a_prompt).to_string();
        for words in [
            &[][..],
            &["  \n"][..],
            &["--json"][..],
            &["-p"][..],
            &[" --json "][..],
        ] {
            assert_eq!(
                prompt_to_open_on(&args(words)),
                Err(refused.clone()),
                "{words:?} was taken as a prompt"
            );
        }
    }

    /// CLI-31. `--model` and `--effort` after the prompt reach the session, in either order and
    /// whichever is given. A session that read them and kept neither would open on the model a file
    /// or an earlier pick chose, and the line that named one would not know.
    #[test]
    fn a_model_and_a_level_after_the_prompt_reach_the_session() {
        let asked = |model: Option<&str>, effort| AskedFor {
            model: model.map(str::to_string),
            effort,
        };
        for (words, expected) in [
            (
                &["fix it", "--model", "opus"][..],
                asked(Some("opus"), None),
            ),
            (
                &["fix it", "--effort", "high"][..],
                asked(None, Some(Effort::High)),
            ),
            (
                &["fix it", "--effort", "low", "--model", "brave/x"][..],
                asked(Some("brave/x"), Some(Effort::Low)),
            ),
            (
                &["fix it", "--model", "a", "--model", "b"][..],
                asked(Some("b"), None),
            ),
        ] {
            assert_eq!(
                prompt_to_open_on(&args(words)),
                Ok(("fix it".to_string(), expected)),
                "{words:?}"
            );
        }
    }

    /// CLI-31. A `--model` with no name and an `--effort` that is no level are refused, as they are
    /// for a one-shot run. A name the service does not know is substituted and reported, but a word
    /// that is no level would be dropped and the session opened at another level without a word.
    #[test]
    fn a_blank_model_or_an_unknown_level_after_the_prompt_is_refused() {
        for (words, refused) in [
            (
                &["fix it", "--model"][..],
                t!(cli_model_needs_a_name).to_string(),
            ),
            (
                &["fix it", "--model", "  "][..],
                t!(cli_model_needs_a_name).to_string(),
            ),
            (
                &["fix it", "--effort"][..],
                effort_level(None).expect_err("no level"),
            ),
            (
                &["fix it", "--effort", "turbo"][..],
                effort_level(Some(&"turbo".to_string())).expect_err("no level"),
            ),
        ] {
            assert_eq!(prompt_to_open_on(&args(words)), Err(refused), "{words:?}");
        }
    }

    /// CLI-31. Anything else after the prompt is refused, and named. What the line could add is a
    /// one-shot run's: a session that let `--file` pass unread would run without a file the line
    /// asked for, and one that let an unquoted second word pass would send part of the prompt.
    #[test]
    fn anything_else_after_the_prompt_is_refused_by_name() {
        for (words, named) in [
            (&["fix it", "--file", "a.txt"][..], "--file"),
            (&["fix it", "--mode", "plan"][..], "--mode"),
            (&["fix it", "--model", "opus", "--json"][..], "--json"),
            (&["fix", "the", "tests"][..], "the"),
        ] {
            let refused = prompt_to_open_on(&args(words)).expect_err("must refuse");
            assert!(
                refused.contains(&format!("'{named}'")),
                "{words:?}: {refused}"
            );
        }
    }

    /// CLI-31. `-i` after another way of starting a session is refused rather than dropped. `--fork`
    /// and `--from-pr` read no further than their value, so the session would open and the prompt
    /// never be sent, and `--bg` joins its words, so the session nobody watches would be given a
    /// prompt opening `-i`. A command's own words are not read: `permissions check` takes a
    /// program's arguments, and `grep -i` is one.
    #[test]
    fn a_session_flag_beside_another_way_of_starting_is_refused() {
        for (typed, start) in [
            (&["--fork", "abc", "-i", "try the other way"][..], "--fork"),
            (&["-f", "abc", "--interactive", "try it"][..], "-f"),
            (&["--from-pr", "12", "-i", "pick it up"][..], "--from-pr"),
            (&["--resume", "abc", "-i"][..], "--resume"),
            (&["-r", "abc", "-i", "carry on"][..], "-r"),
            (&["--continue", "-i"][..], "--continue"),
            (&["-c", "-i", "carry on"][..], "-c"),
            (&["--bg", "-i", "fix the tests"][..], "--bg"),
        ] {
            assert_eq!(
                interactive_out_of_place(&args(typed)),
                Some(t!(cli_interactive_not_with_another_start, flag = start)),
                "{typed:?}"
            );
        }
        for typed in [
            &["-i", "fix it"][..],
            &["--resume", "abc"][..],
            &["permissions", "check", "Bash", "grep", "-i", "needle"][..],
            &["--bg", "explain what -i does"][..],
        ] {
            assert_eq!(interactive_out_of_place(&args(typed)), None, "{typed:?}");
        }
    }

    /// CLI-31. `-i` behind a task's options is refused rather than read as the task. A one-shot run
    /// takes its first word that is no option as the prompt, so `bravebot --model opus -i` would run
    /// the task `-i` with nobody watching, and `bravebot --model opus -i "fix it"` would be refused
    /// over `fix it` with nothing said about where `-i` goes.
    #[test]
    fn a_session_flag_behind_a_tasks_options_is_refused_rather_than_run_as_the_task() {
        let refused = t!(cli_interactive_not_with_a_task).to_string();
        for typed in [
            &["--model", "opus", "-i"][..],
            &["--model", "opus", "--interactive", "fix it"][..],
            &["-p", "fix it", "-i"][..],
        ] {
            assert_eq!(
                parse_invocation(&args(typed)).err(),
                Some(refused.clone()),
                "{typed:?}"
            );
        }
    }

    /// Turn is what an unqualified run has always been, so an omitted `--mode` has to stay that.
    #[test]
    fn the_default_mode_is_the_turn_loop() {
        let invocation = parse_invocation(&args(&["do a thing"])).expect("parses");
        assert_eq!(invocation.mode, Mode::Turn);
        assert_eq!(invocation.prompt, "do a thing");
    }

    #[test]
    fn a_leading_mode_flag_is_a_task_not_an_unknown_option() {
        let invocation =
            parse_invocation(&args(&["--mode", "manifest", "do a thing"])).expect("parses");
        assert_eq!(invocation.mode, Mode::Manifest);
        assert_eq!(invocation.prompt, "do a thing");
    }

    #[test]
    fn an_unknown_mode_is_refused_rather_than_guessed() {
        let err =
            parse_invocation(&args(&["--mode", "safe", "do a thing"])).expect_err("must refuse");
        assert!(err.contains("safe"), "{err}");
        assert!(err.contains("turn") && err.contains("manifest"), "{err}");
    }

    /// A failed plan is the document nobody would otherwise see. It belongs beside the reply,
    /// never in it: a pipe of stdout would otherwise pick up the model's own words mixed into
    /// whatever the run produced.
    #[test]
    fn a_failed_plan_is_printed_beside_the_reply() {
        let (reply, beside) = written(&Finished {
            reply: "ok",
            notices: &[],
            attempt: Some("manifest proposed, which was not usable\n  not JSON\n"),
            trail: None,
            clean: true,
            ending: Ending::Done,
            not_served: None,
        });
        assert_eq!(reply, "ok\n");
        assert!(beside.contains("not usable"), "got: {beside}");
        assert!(!reply.contains("not usable"));
    }

    /// A manifest run that stopped is the one worth looking at, so what it produced is printed
    /// whether or not `--trace` was asked for. Only the audit trail is the flag's to add.
    #[test]
    fn a_stopped_manifest_prints_what_it_produced_without_the_trace_flag() {
        let attempt = bravebot_agent::manifest::Attempt {
            proposed: Some("not JSON at all".to_string()),
            steps: vec!["1. read notes.md: done".to_string()],
            ..Default::default()
        };
        let sink = trail_of(vec![Event::GatePassed {
            gate: "display",
            detail: "assistant reply shown to the user".into(),
        }]);

        let mut untraced = Vec::new();
        say_what_a_stopped_manifest_produced(&mut untraced, &attempt, None);
        let untraced = String::from_utf8(untraced).expect("utf-8");
        assert!(untraced.contains("not JSON at all"), "got: {untraced}");
        assert!(untraced.contains("read notes.md"), "got: {untraced}");
        assert!(!untraced.contains("audit trail"), "got: {untraced}");

        let mut traced = Vec::new();
        say_what_a_stopped_manifest_produced(&mut traced, &attempt, Some(&sink));
        let traced = String::from_utf8(traced).expect("utf-8");
        assert!(traced.contains("not JSON at all"), "got: {traced}");
        assert!(traced.contains("audit trail"), "got: {traced}");
    }

    /// Nobody is watching a one-shot run, so the list that answers a prompt in advance answers
    /// nothing: there is no prompt for it to reach. Without the flag an allow rule decides no
    /// more than a line nobody wrote, while the two lists that refuse and that force an ask carry
    /// over, because those say something a run with nobody at it can still act on.
    #[test]
    fn an_allow_rule_decides_nothing_for_a_run_nobody_is_watching() {
        use bravebot_core::permissions::{Decision, Ruling, Subject};

        let settings = bravebot_config::Settings::parse(
            r#"{
              "permissions": {
                "allow": ["Edit(**)"],
                "ask": ["Bash(git push *)"],
                "deny": ["Read(./.env)"]
              }
            }"#,
        );

        let (permissions, rejected) =
            rules_for_a_one_shot_run(&settings, None, Path::new("."), false);
        assert!(rejected.is_empty());
        assert_eq!(
            permissions.for_path(Subject::Edit, "notes.md"),
            Decision::Unmatched,
            "an allow rule answered a write prompt in a run with nobody to prompt"
        );
        assert_eq!(
            permissions.for_path(Subject::Read, ".env"),
            Decision::Ruled(Ruling::Deny)
        );
        assert_eq!(
            permissions.for_command(&["git", "push", "origin", "main"]),
            Decision::Ruled(Ruling::Ask)
        );
    }

    /// With the flag the person answered every prompt themselves, so their file is read whole: the
    /// list is theirs, and the run may act because they said so on the command line.
    #[test]
    fn the_flag_is_what_lets_an_allow_rule_decide_again() {
        use bravebot_core::permissions::{Decision, Ruling, Subject};

        let settings =
            bravebot_config::Settings::parse(r#"{"permissions": {"allow": ["Edit(**)"]}}"#);

        let (permissions, _) = rules_for_a_one_shot_run(&settings, None, Path::new("."), true);
        assert_eq!(
            permissions.for_path(Subject::Edit, "notes.md"),
            Decision::Ruled(Ruling::Allow)
        );
    }

    /// A leading `--resume` or `--continue` with something after the session it names is a task
    /// run (CLI-25), and with nothing after it is still the request to open that session. Reading
    /// every `--resume` as a task would turn the interactive resume into a refusal for the missing
    /// task, and reading none would drop the task a script sent.
    #[test]
    fn a_resume_with_a_task_after_it_runs_the_task_and_one_without_opens_the_session() {
        for (typed, task) in [
            (&["--resume", "abc", "-p", "next"][..], true),
            (&["--resume", "abc", "next"][..], true),
            (&["-r", "abc", "--json", "-p", "next"][..], true),
            (&["--resume", "abc"][..], false),
            (&["--resume"][..], false),
            (&["--continue", "-p", "next"][..], true),
            (&["-c", "next"][..], true),
            (&["--continue"][..], false),
            (&["--resume", "abc", "--plain"][..], false),
            (&["--continue", "--json"][..], false),
            (&["-p", "next", "--resume", "abc"][..], false),
            (&["doctor"][..], false),
        ] {
            assert_eq!(continues_with_a_task(&args(typed)), task, "{typed:?}");
        }
        // A flag where the id should be is a task run, which refuses it by name.
        assert!(continues_with_a_task(&args(&["--resume", "-p", "next"])));
    }

    /// The task may lead and the session may follow, as Claude Code's `-p "task" --resume id`
    /// does, so a script written for either order carries on the session it names.
    #[test]
    fn a_task_may_name_the_session_it_carries_on_before_or_after_it() {
        use crate::continued::Resume;

        for typed in [
            &["-p", "next", "--resume", "abc"][..],
            &["--resume", "abc", "-p", "next"][..],
            &["-r", "abc", "next"][..],
        ] {
            let invocation = parse_invocation(&args(typed)).expect("parses");
            assert_eq!(invocation.resume, Some(Resume::Id("abc".to_string())));
            assert_eq!(invocation.prompt, "next");
        }
        let latest = parse_invocation(&args(&["-p", "next", "-c"])).expect("parses");
        assert_eq!(latest.resume, Some(Resume::Latest));
        let fresh = parse_invocation(&args(&["-p", "next"])).expect("parses");
        assert_eq!(fresh.resume, None);
    }

    /// A flag or nothing where the id should be is refused rather than read as a session called
    /// `-p`, or as no session at all, which would run the follow-up with nothing before it.
    #[test]
    fn a_resume_naming_no_session_is_refused() {
        for typed in [
            &["next", "--resume"][..],
            &["--resume", "-p", "next"][..],
            &["next", "--resume", " "][..],
        ] {
            parse_invocation(&args(typed)).expect_err("a missing id must be refused");
        }
    }

    /// SANDBOX-22, COMPAT-001: `--sandbox` takes one of four exact words, `standard` still naming
    /// `workspace-write`, the last of a repeat winning, and is taken out of the arguments wherever
    /// it stood. The regression it rejects is a flag that reads a near miss as a mode, a script
    /// written for `standard` that stops working, or a flag that leaves its word behind to be read
    /// as the task.
    #[test]
    fn the_sandbox_flag_takes_one_of_four_words_and_the_last_wins() {
        for (typed, expected) in [
            (
                &["--sandbox", "strict", "-p", "do a thing"][..],
                SandboxMode::Strict,
            ),
            (
                &["-p", "--sandbox", "off", "do a thing"][..],
                SandboxMode::Off,
            ),
            (
                &["-p", "do a thing", "--sandbox", "workspace-write"][..],
                SandboxMode::WorkspaceWrite,
            ),
            (
                &["--sandbox", "read-only", "-p", "do a thing"][..],
                SandboxMode::ReadOnly,
            ),
            (
                &["-p", "--sandbox", "standard", "do a thing"][..],
                SandboxMode::WorkspaceWrite,
            ),
            (
                &["--sandbox", "off", "--sandbox", "strict"][..],
                SandboxMode::Strict,
            ),
        ] {
            let mut arguments = args(typed);
            assert_eq!(
                take_sandbox(&mut arguments),
                Ok(Some(expected)),
                "{typed:?}"
            );
            assert!(
                !arguments.iter().any(|argument| argument == "--sandbox"),
                "{typed:?} left the flag behind: {arguments:?}"
            );
            assert!(
                !arguments
                    .iter()
                    .any(|argument| SandboxMode::parse(argument).is_some()),
                "{typed:?} left its word behind: {arguments:?}"
            );
        }
        let mut none = args(&["-p", "do a thing"]);
        assert_eq!(take_sandbox(&mut none), Ok(None));
        assert_eq!(none, args(&["-p", "do a thing"]));
    }

    /// SANDBOX-22: a missing, blank or unknown word is refused naming the four, and the arguments
    /// are left as typed. The regression it rejects is a script whose variable expanded to nothing
    /// being run under the default.
    #[test]
    fn the_sandbox_flag_refuses_a_word_that_is_not_a_mode() {
        for typed in [
            &["-p", "do a thing", "--sandbox"][..],
            &["--sandbox", "", "-p", "do a thing"][..],
            &["--sandbox", "Strict", "-p", "do a thing"][..],
            &["--sandbox", "none", "-p", "do a thing"][..],
            &["--sandbox", "-p", "do a thing"][..],
        ] {
            let mut arguments = args(typed);
            let refused = take_sandbox(&mut arguments).expect_err(&format!("{typed:?}"));
            assert!(
                refused.contains("strict, read-only, workspace-write, off"),
                "{refused}"
            );
            assert_eq!(arguments, args(typed), "a refusal rewrote the arguments");
        }
    }

    /// SANDBOX-22: a refused mode stops what runs programs and lets through what only reads or
    /// writes the person's own files. The regression it rejects is `bravebot completion` or
    /// `bravebot auth` failing on a machine whose settings went under the managed floor, or a
    /// session or a task starting past one.
    #[test]
    fn a_refused_mode_stops_what_runs_programs_and_not_what_does_not() {
        for first in [
            "doctor",
            "update",
            "--help",
            "--version",
            "auth",
            "permissions",
            "sessions",
            "attach",
            "completion",
            "import-providers",
        ] {
            assert!(starts_no_program(Some(first)), "{first}");
        }
        for first in [
            None,
            Some("-p"),
            Some("--bg"),
            Some("--resume"),
            Some("reply"),
            Some("__bg-host"),
            Some("__bg-supervisor"),
        ] {
            assert!(!starts_no_program(first), "{first:?}");
        }
    }

    /// SANDBOX-22: a refusal under a pin says which of reading and writing the asked mode does more
    /// of, and suggests the pin, which the pin always keeps. The fault it rejects is a sentence that
    /// tells a person under a `read-only` pin to choose a stricter mode, of which there is none, or
    /// tells one under a `strict` pin that `read-only` is looser without saying how.
    #[test]
    fn a_refused_mode_says_what_it_does_more_of_and_suggests_a_mode_the_pin_keeps() {
        let settings = bravebot_config::Settings::default();
        for (pin, asked, said) in [
            (
                "read-only",
                SandboxMode::Strict,
                "strict writes more than read-only.",
            ),
            (
                "strict",
                SandboxMode::ReadOnly,
                "read-only reads more of the machine than strict.",
            ),
            (
                "strict",
                SandboxMode::WorkspaceWrite,
                "workspace-write reads more of the machine and writes more than strict.",
            ),
        ] {
            let managed = pin_file(
                &format!("looser-{pin}-{asked}"),
                &format!(r#"{{"sandbox": {{"mode": "{pin}"}}}}"#),
            );
            let refused = bravebot_config::sandbox::resolve(Some(asked), &settings, &managed)
                .expect_err("looser than the pin");
            let sentence = sandbox_refusal(&refused);
            assert!(sentence.contains(said), "{sentence}");
            assert!(
                sentence.contains(&format!("Run with --sandbox {pin}, or without the flag.")),
                "{sentence}"
            );
            assert!(!sentence.contains("stricter"), "{sentence}");
            let suggested = SandboxMode::parse(pin).expect("a mode");
            assert!(
                bravebot_config::sandbox::resolve(Some(suggested), &settings, &managed).is_ok(),
                "{pin} keeps {pin}"
            );
        }

        let home = scratch("looser-file");
        let file = home.join("settings.json");
        std::fs::write(&file, r#"{"sandbox": {"mode": "strict"}}"#).expect("home layer");
        let settings = bravebot_config::Settings::layered(Some(home), None, None);
        let managed = pin_file("looser-file-pin", r#"{"sandbox": {"mode": "read-only"}}"#);
        let refused = bravebot_config::sandbox::resolve(None, &settings, &managed)
            .expect_err("looser than the pin");
        let sentence = sandbox_refusal(&refused);
        assert!(
            sentence.contains("strict writes more than read-only. Change it to read-only there"),
            "{sentence}"
        );
    }

    /// The refusals a manifest run gives for flags it cannot take: none outside a manifest run,
    /// and the first in the order the checks run where several are given.
    #[test]
    fn a_manifest_run_is_refused_the_flags_it_cannot_take_in_order() {
        let words = SystemPrompts {
            replacing: Some("words".to_string()),
            ..SystemPrompts::default()
        };
        let none = SystemPrompts::default();
        let latest = continued::Resume::Latest;

        assert_eq!(
            manifest_conflict(Mode::Turn, true, true, true, &words, Some(&latest)),
            None,
            "a turn is refused nothing here"
        );
        assert_eq!(
            manifest_conflict(Mode::Manifest, false, false, false, &none, None),
            None
        );
        let first = manifest_conflict(Mode::Manifest, true, true, true, &words, Some(&latest));
        assert_eq!(first.as_deref(), Some(t!(cli_agent_not_with_a_manifest)));
        let first = manifest_conflict(Mode::Manifest, false, true, true, &words, Some(&latest));
        assert_eq!(first.as_deref(), Some(t!(cli_advisor_not_with_a_manifest)));
        let first = manifest_conflict(Mode::Manifest, false, false, true, &words, Some(&latest));
        assert_eq!(
            first.as_deref(),
            Some(t!(cli_output_schema_not_with_a_manifest))
        );
        let first = manifest_conflict(Mode::Manifest, false, false, false, &words, Some(&latest));
        assert_eq!(
            first,
            Some(t!(
                cli_system_prompt_not_with_a_manifest,
                flag = "--system-prompt"
            ))
        );
        let first = manifest_conflict(Mode::Manifest, false, false, false, &none, Some(&latest));
        assert_eq!(
            first,
            Some(t!(cli_resume_not_with_a_manifest, flag = "--continue"))
        );
        let id = continued::Resume::Id("abc".to_string());
        let named = manifest_conflict(Mode::Manifest, false, false, false, &none, Some(&id));
        assert_eq!(
            named,
            Some(t!(cli_resume_not_with_a_manifest, flag = "--resume"))
        );
    }
}
