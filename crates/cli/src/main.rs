//! Command-line entry point.

#![forbid(unsafe_code)]

mod exit;
mod json;
mod progress;

use crate::exit::{Ending, fail};
use bravebot_agent::confirm::{
    Confirmer, Decision, FetchRequest, ManifestRequest, OutputRequest, RunDecision, RunRequest,
    ServerRequest, VetRequest, VouchRequest, WriteRequest,
};
use bravebot_agent::turn::{self, Task};
use bravebot_agent::{Mode, Workspace};
use bravebot_config::{Config, Managed};
use bravebot_core::ask::{Answer, Asking};
use bravebot_core::cancel::Cancel;
use bravebot_core::event::{Event, RecordingSink, Role};
use bravebot_core::trust::TrustStore;
use bravebot_i18n::t;
use bravebot_net::Transport;
use bravebot_net::transport::{
    CERTIFICATE_DIRECTORY, CERTIFICATE_FILE, PROXY_VARIABLES, TrustRoots,
};
use bravebot_sandbox::SandboxError;
use bravebot_sandbox::policy::Capabilities;
use bravebot_tui::sessions::Resumable;
use std::io::{BufRead, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() -> ExitCode {
    // Before anything is printed, and exactly once: every later lookup reads what this settled
    // on. Nothing else in the tree consults the environment about a language.
    bravebot_i18n::init_from_environment();

    // The process's own argv, parsed below into the documented flags. There is no other
    // way for a command line tool to learn what it was asked to do.
    // nosemgrep: rust.lang.security.args.args
    let mut args: Vec<String> = std::env::args().skip(1).collect();

    // Engaged here rather than deeper in because it must be true before the first thing that could
    // write is reached, and this is the last moment that is certain to be before all of them.
    if take_incognito(&mut args) {
        bravebot_core::incognito::engage();
    }

    // Taken out before anything dispatches on the first argument, because this one belongs to every
    // way of starting: a session, a resumed session, and a one-shot run all put the same four
    // questions to the same trait. Reading it per subcommand would be four chances to read it in
    // three of them, and the flag would then be silently ignored wherever it was forgotten.
    let skip_permissions = take_skip_permissions(&mut args);

    // Read off the arguments as typed, before the flag below takes anything out of them: a result
    // object was asked for by the command line that failed, whichever token the failure consumed.
    let as_json = wants_json(&args);

    // Taken out before dispatch for the reason the two above are, and acted on here because this is
    // before the first read of a setting: a layer registered after the interface had loaded the
    // others would configure half of one process.
    match take_settings(&mut args) {
        Ok(None) => {}
        Ok(Some(path)) if path.is_file() => bravebot_config::name_a_settings_file(path),
        // Refused rather than ignored, and for the audience CLI-11 refuses a directory for: a run
        // told to configure itself from a file is a run whose configuration is the file, so falling
        // back to whatever was found would be the wrong configuration used in silence. A mistyped
        // path and a variable that expanded to nothing look the same here, and both are common.
        Ok(Some(path)) => {
            return stopped_before_the_turn(
                as_json,
                Ending::Argument,
                t!(cli_settings_not_a_file, path = path.display().to_string()),
            );
        }
        Err(complaint) => {
            return stopped_before_the_turn(as_json, Ending::Argument, complaint);
        }
    }

    match args.first().map(String::as_str) {
        Some("--version" | "-V") => {
            // The same words a session record writes down, so the two can be compared without
            // anyone having to work out what "the current build" means.
            println!("bravebot {}", bravebot_tui::BUILD);
            ExitCode::SUCCESS
        }
        Some("--help" | "-h") => {
            print_help();
            ExitCode::SUCCESS
        }
        // With no arguments the interactive session is the natural default.
        None => interactive(bravebot_tui::app::Start::Fresh, skip_permissions),
        // Picking up where a session left off, chosen from a list or named outright.
        Some("--resume" | "-r") => match args.get(1) {
            Some(id) => resume_named(id, skip_permissions),
            None => interactive(bravebot_tui::app::Start::Choose, skip_permissions),
        },
        // The same, for the session somebody was in a moment ago, which is the one they mean
        // often enough that asking them to find its id is asking for nothing.
        Some("--continue" | "-c") => continue_here(skip_permissions),
        // Fork a session, creating a new session record that starts with the same transcript.
        Some("--fork" | "-f") => match args.get(1) {
            Some(id) => fork_named(id, skip_permissions),
            None => fail(Ending::Argument, t!(cli_fork_needs_a_name)),
        },
        // The task flags may lead: `bravebot -p "task"` and `bravebot --mode manifest "task"`
        // would otherwise be caught below as unknown options.
        Some(
            "-p" | "--print" | "--mode" | "--model" | "--file" | "--add-dir" | "--trace" | "--json",
        ) => run_task(&args, skip_permissions),
        Some("doctor") => doctor(),
        Some("import-leo-creds") => import_leo_creds(&args[1..]),
        Some(flag) if flag.starts_with('-') => {
            let refused = fail(Ending::Argument, t!(cli_unknown_option, flag = flag));
            print_help();
            refused
        }
        // Anything else is treated as the task prompt.
        Some(_) => run_task(&args, skip_permissions),
    }
}

/// Take `--dangerously-skip-permissions` out of the arguments, reporting whether it was there.
///
/// Removed before dispatch for the reason `--incognito` is, and it composes with it: the flag belongs
/// to every way of starting rather than to a task, since a session, a resumed session and a one-shot
/// run all put the same questions to the same trait. Read per subcommand it would be three chances to
/// read it in two of them.
///
/// See [`bravebot_agent::PermissionMode`] for what giving it up costs. Deny rules from the settings
/// file are not part of it: they refuse before there is anything to prompt about, so the flag means
/// "stop asking me" rather than "forget what I wrote down".
///
/// Repeats are one flag rather than an error, as with `--incognito`.
fn take_skip_permissions(args: &mut Vec<String>) -> bool {
    let asked = args.len();
    args.retain(|arg| arg != "--dangerously-skip-permissions");
    args.len() != asked
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
    let mut named = None;
    let mut kept = Vec::with_capacity(args.len());
    let mut index = 0;
    while index < args.len() {
        if args[index] != "--settings" {
            kept.push(args[index].clone());
            index += 1;
            continue;
        }
        // A blank path is refused rather than read as no flag, on `--model`'s argument: a script
        // whose variable expanded to nothing asked for a settings file and would otherwise be run
        // under whatever the directory happened to carry, without being told.
        match args.get(index + 1).map(|path| path.trim()) {
            Some(path) if !path.is_empty() => {
                named = Some(PathBuf::from(path));
                index += 2;
            }
            _ => return Err(t!(cli_settings_needs_a_path).to_string()),
        }
    }
    *args = kept;
    Ok(named)
}

fn print_help() {
    /// Wide enough for the longest invocation below, so a translated description starts in the
    /// same column as every other one rather than wherever hand-counted spaces left it.
    const FORM: usize = 39;
    /// The same, for the key column and the option column.
    const KEY: usize = 22;
    /// Wide enough for `--dangerously-skip-permissions` and a gap. A narrower column would leave
    /// that one flag touching its own description, since the padding below cannot go negative.
    const OPTION: usize = 32;

    println!("{}", t!(cli_tagline, version = VERSION));
    println!();
    println!("{}", t!(cli_usage_heading));
    for (form, description) in [
        ("bravebot", t!(cli_usage_interactive)),
        ("bravebot \"<task>\" [--file <path>]...", t!(cli_usage_task)),
        ("cat file | bravebot -p \"<task>\"", t!(cli_usage_piped)),
        ("bravebot --resume [id]", t!(cli_usage_resume)),
        ("bravebot --continue", t!(cli_usage_continue)),
        ("bravebot --fork <id>", t!(cli_usage_fork)),
        ("bravebot doctor", t!(cli_usage_doctor)),
        ("bravebot import-leo-creds [channel]", t!(cli_usage_import)),
    ] {
        println!("  {form:<FORM$}{description}");
    }
    println!();

    println!("{}", t!(cli_keys_heading));
    for (keys, description) in [
        ("Enter", t!(cli_key_send)),
        ("Ctrl-T", t!(cli_key_audit)),
        ("Up/Down", t!(cli_key_history)),
        ("Ctrl-R", t!(cli_key_history_search)),
        ("Wheel, PageUp/Down", t!(cli_key_scroll)),
        ("Home/End", t!(cli_key_jump)),
        ("Esc", t!(cli_key_cancel)),
        ("Ctrl-C", t!(cli_key_leave)),
    ] {
        println!("  {keys:<KEY$}{description}");
    }
    println!();

    // Listed from the commands themselves, so one renamed or added cannot leave this advertising a
    // word that no longer works. The interface offers the same list when a slash is typed.
    println!("{}", t!(cli_commands_heading));
    for command in bravebot_tui::app::commands() {
        let word = if command.argument.is_empty() {
            command.name.to_string()
        } else {
            format!("{} {}", command.name, command.argument)
        };
        println!("  {word:<20}  {}", command.description);
    }
    // Not a command, but typed in the same place and worth finding here.
    println!("  {:<20}  {}", "@<path>", t!(cli_name_a_file));
    println!();

    println!("{}", t!(cli_options_heading));
    for (flags, description) in [
        ("--file <path>", t!(cli_option_file)),
        ("--add-dir <path>", t!(cli_option_add_dir)),
        ("--settings <path>", t!(cli_option_settings)),
        ("--mode <mode>", t!(cli_option_mode)),
        ("--model <name>", t!(cli_option_model)),
        ("-p, --print", t!(cli_option_print)),
        ("--trace", t!(cli_option_trace)),
        ("--json", t!(cli_option_json)),
        ("--incognito", t!(cli_option_incognito)),
        (
            "--dangerously-skip-permissions",
            t!(cli_option_dangerously_skip_permissions),
        ),
        ("-h, --help", t!(cli_option_help)),
        ("-V, --version", t!(cli_option_version)),
    ] {
        println!("  {flags:<OPTION$}{description}");
    }
}

/// What to say to somebody whose configuration names no service that can serve a turn.
///
/// Three routes, each a line, in the order they are worth taking today. The two that reach a
/// service directly lead; importing a Leo Premium subscription is last and says why, since those
/// models are reached through Brave's AI gateway and it has open problems of its own. It is still
/// the shortest route for somebody who already subscribes, which is why it is offered at all.
///
/// `a_service_is_configured` replaces all three with one line. That person set a service up and
/// left the model this build baked in, which a settings block copied out of another tool does by
/// design, so what they need is the key that names one of their own models.
///
/// `refused` is what was wrong with a stored subscription, where something was. It leads, because
/// it changes what the person should do: a batch that could not be read is one import away.
///
/// Built as a string rather than printed, so the ending that carries it leads the block with its
/// identifier the way it does every other failure, and so `doctor` can say the same thing. It says it too: a report reading "configuration OK" about a machine where a
/// session refuses to start is the first thing somebody in that position goes and looks at.
///
/// Nothing here names `doctor` itself, for that reason. A line telling somebody to run the
/// command they are reading the output of is a line that has to be edited out of one of the two
/// places it appears, which is how the two come to say different things.
fn how_to_configure_a_model(refused: Option<&str>, a_service_is_configured: bool) -> String {
    let mut lines = vec![t!(onboarding_no_model).to_string()];
    if let Some(problem) = refused {
        lines.push(t!(onboarding_subscription_unusable, problem = problem));
    }
    lines.push(String::new());

    // One line or three routes, never both. Somebody who has a service set up and only the wrong
    // model in force is one settings key away, and three ways to set up a service is three things
    // to read past on the way to the one that applies.
    if a_service_is_configured {
        lines.push(t!(onboarding_name_a_configured_model).to_string());
    } else {
        lines.push(t!(onboarding_pick_one).to_string());
        for route in [
            t!(onboarding_bedrock),
            t!(onboarding_openrouter),
            t!(onboarding_leo),
        ] {
            lines.push(format!("  - {route}"));
        }
    }

    lines.push(String::new());
    lines.push(t!(onboarding_where_to_read).to_string());
    lines.join("\n")
}

/// Take `--incognito` out of the arguments, reporting whether it was there.
///
/// Removed before anything dispatches on what leads the list, so the mode composes with every
/// other way of starting rather than being a fourth flag that may lead: `--incognito -p "task"`,
/// `--incognito --resume` and `--incognito` on its own all mean what they look like, and the
/// dispatch below goes on matching against a list the mode is no longer in.
///
/// Repeats are one flag rather than an error. `--incognito --incognito` asks for a mode that is
/// already on, and refusing it would be a rule about typing rather than about privacy.
fn take_incognito(args: &mut Vec<String>) -> bool {
    let asked = args.len();
    args.retain(|arg| arg != "--incognito");
    args.len() != asked
}

/// What a one-shot invocation asked for, before anything runs.
#[derive(Debug)]
struct Invocation {
    prompt: String,
    files: Vec<String>,
    mode: Mode,
    /// The model the command line named. `None` leaves the configured one in force rather than
    /// standing for a model of its own.
    model: Option<String>,
    /// Directories outside the working one that this run may reach into.
    directories: Vec<String>,
    trace: bool,
    print: bool,
    /// Whether stdout carries the result object rather than the prose reply.
    json: bool,
}

/// Parse `<prompt> [--file path]... [--add-dir path]... [--mode name] [--model name]
/// [--trace] [--json] [-p]`.
fn parse_invocation(args: &[String]) -> Result<Invocation, String> {
    let mut prompt = String::new();
    let mut files = Vec::new();
    let mut mode = Mode::default();
    let mut model = None;
    let mut directories = Vec::new();
    let mut trace = false;
    let mut print = false;
    let mut json = false;
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
            // A blank name is refused rather than read as no choice. A stored one is allowed to
            // be blank, where it means the file holds nothing and the configured model answers,
            // but a script that computed an empty variable asked for a model and would otherwise
            // be given whatever was configured without being told.
            "--model" => match args.get(index + 1).map(|name| name.trim()) {
                Some(name) if !name.is_empty() => {
                    model = Some(name.to_string());
                    index += 2;
                }
                _ => return Err(t!(cli_model_needs_a_name).to_string()),
            },
            "--file" => match args.get(index + 1) {
                Some(path) => {
                    files.push(path.clone());
                    index += 2;
                }
                None => return Err(t!(cli_file_needs_a_path).to_string()),
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
            "--trace" => {
                trace = true;
                index += 1;
            }
            "--json" => {
                json = true;
                index += 1;
            }
            "-p" | "--print" => {
                print = true;
                index += 1;
            }
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
        mode,
        model,
        directories,
        trace,
        print,
        json,
    })
}

fn run_task(args: &[String], skip_permissions: bool) -> ExitCode {
    let invocation = match parse_invocation(args) {
        Ok(invocation) => invocation,
        // Whether a result object was asked for is read off the raw arguments here, because the
        // parse that would otherwise have said is the thing that just failed, and a caller that
        // asked for one asked for this one too.
        Err(err) => return stopped_before_the_turn(wants_json(args), Ending::Argument, err),
    };
    let Invocation {
        prompt,
        files,
        mode,
        model,
        directories,
        trace,
        print,
        json: as_json,
    } = invocation;

    // Read before the emptiness check below, since `cat notes.md | bravebot -p` is a complete
    // invocation: the pipe is the input and the prompt may be left off.
    let piped = if print {
        let stdin = std::io::stdin();
        let is_tty = stdin.is_terminal();
        match piped_input(stdin.lock(), is_tty) {
            Ok(text) => text,
            Err(err) => return stopped_before_the_turn(as_json, Ending::Argument, err),
        }
    } else {
        None
    };

    if prompt.is_empty() && piped.is_none() {
        return stopped_before_the_turn(as_json, Ending::Argument, t!(cli_task_required));
    }

    let mut config = match Config::from_env() {
        Ok(c) => c,
        Err(err) => {
            return stopped_before_the_turn(
                as_json,
                Ending::Configuration,
                t!(cli_configuration_problem, problem = err),
            );
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
    if let bravebot_agent::backend::Serving::NothingConfigured {
        subscription,
        a_service_is_configured,
    } = bravebot_agent::backend::serving(
        &config,
        &bravebot_net::Egress::new(),
        &model_for_this_run(model.as_deref(), &config),
    ) {
        return stopped_before_the_turn(
            as_json,
            Ending::Configuration,
            how_to_configure_a_model(subscription.as_deref(), a_service_is_configured),
        );
    }

    let settings = bravebot_config::Settings::load();

    let mut workspace = match current_workspace(&settings) {
        Ok(w) => w,
        Err(err) => {
            return stopped_before_the_turn(
                as_json,
                Ending::Failed,
                t!(cli_workspace_problem, problem = err),
            );
        }
    };

    // Fatal rather than said and carried on with. A session leaves the person to retype it; a
    // script that asked to reach a directory and did not gets a turn that fails somewhere further
    // in, over a file it was told it could open.
    if let Err(problem) = open_directories(&mut workspace, &directories) {
        return stopped_before_the_turn(as_json, Ending::Argument, problem);
    }

    // A run is a session for this: it is given somewhere of its own to write what is not part of
    // the project, and the directory goes when the run does. Held in a binding for exactly that
    // reason, since dropping it is what removes it.
    let _scratch = scratch_for_this_run(&mut workspace);

    let egress = bravebot_net::Egress::new();
    let mut sink = RecordingSink::new();

    // The rules the settings file carried. Anything unreadable is named on stderr, beside the rest
    // of what this run has to say about itself.
    let (permissions, rejected) = rules_for_a_one_shot_run(
        &settings,
        bravebot_agent::home::directory().as_deref(),
        skip_permissions,
    );
    for problem in &rejected {
        eprintln!(
            "{}",
            t!(
                session_permission_rule_ignored,
                problem = problem.to_string()
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
    let mut task = Task::new(prompt)
        .with_home(bravebot_agent::home::directory())
        .with_model(model_asked_for(named, bravebot_tui::store::load_model()))
        .with_effort(bravebot_tui::store::load_effort())
        .with_permissions(permissions)
        .with_permission_mode(permission_mode);
    for file in files {
        task = task.with_file(file);
    }
    if let Some(text) = piped {
        task = task.with_piped_input(text);
    }

    // A one-shot run has nobody to ask about a write, so writes are refused rather than silently
    // applied. The one exception is a plan, which is put before the first step rather than in the
    // middle of a run, and only where both ends of the conversation are a terminal: a plan written
    // into a redirected stderr is a plan nobody read, and a pipe means the answer would be read
    // from whatever fed it.
    //
    // Unless the flag was given, which is the one way a run nobody is watching may write: the
    // person accepted that when they typed it, and this is the only path where the refusal above
    // is what stands between the flag and an effect.
    let attended = std::io::stdin().is_terminal() && std::io::stderr().is_terminal();
    let mut one_shot = OneShot::new(std::io::stdin(), std::io::stderr(), attended);
    let mut confirmer = bravebot_agent::Confining::new(&mut one_shot, permission_mode);
    // On stderr, beside the progress lines, so a pipe of the reply is unaffected. Said even here,
    // where nobody may be reading: a run that wrote to the tree without asking should leave a record
    // of having been told not to ask.
    if skip_permissions {
        eprintln!(
            "{}",
            t!(cli_notice, notice = t!(session_permissions_skipped))
        );
    }

    // Progress goes to stderr so stdout stays the reply and nothing else, which is what makes
    // the command pipeable. Without it a long turn prints nothing until it is over.
    let mut reporter = progress::Progress::new(std::io::stderr());

    // Before the turn, so the sign-in's own output is not interleaved with progress lines and a
    // browser opening is accounted for. Nothing happens where no sign-in is wanted, which includes
    // every run whose model is served by Brave.
    //
    // Not fatal: the turn goes ahead and fails with the backend's own account of what is wrong,
    // which says more than this could guess.
    let model = task
        .model
        .as_deref()
        .unwrap_or(&config.default_model)
        .to_string();

    // The window the endpoint advertises for that model, which is what compaction measures a
    // conversation against. A run opens no picker and holds no session, so this is the only place
    // it can be looked up, and the model is in force here however it got there: named on the
    // command line, read back off disk, or pinned in the settings file. Without it a run compacts
    // against a default that a narrow window never reaches, so compaction never fires, while a wide
    // one reaches it with three quarters of the conversation still to spare.
    bravebot_tui::app::adopt_budget_for_model(&mut config, &model);

    // The sign-in's own lines go to stderr as they arrive, beside every other progress line, which
    // keeps stdout the reply and nothing else. A URL and a code are no use after the fact, so they
    // are printed while the command that wrote them is still waiting.
    let signed_in = bravebot_agent::backend::Backend::sign_in_if_needed(&config, &model, |line| {
        eprintln!("{line}");
    });
    if let Err(failure) = signed_in {
        eprintln!("{}", t!(cli_notice, notice = failure.to_string()));
    }

    // Both modes take the same arguments and return the same outcome. The whole of the
    // difference is inside: one asks the model what to do next after every result, the other
    // asked once, before there were any.
    let outcome = match mode {
        Mode::Turn => turn::run_cancellable(
            &config,
            &egress,
            &workspace,
            &task,
            &mut confirmer,
            &mut reporter,
            &mut sink,
            TrustStore::new(workspace.root()),
            &Cancel::new(),
        ),
        Mode::Manifest => bravebot_agent::manifest::run(
            &config,
            &egress,
            &workspace,
            &task,
            &mut confirmer,
            &mut reporter,
            &mut sink,
            TrustStore::new(workspace.root()),
            &Cancel::new(),
        ),
    };

    // A manifest run is written down like any other session. It cannot be resumed, and the
    // picker says so, but "cannot be continued" is a different thing from "leaves no trace":
    // the run somebody needs to read is the one that stopped, and until now it left nothing.
    if mode == Mode::Manifest {
        bravebot_tui::sessions::record_manifest_run(workspace.root(), &task.prompt, &outcome);
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
            let not_served = model_not_served(&asked, comparable, &outcome.model);

            let ending = ending_of_a_turn(
                outcome.clean,
                named_on_the_command_line,
                not_served.is_some(),
            );

            // Built before the reply is chosen, because with `--json` the result object is what
            // goes on stdout in the reply's place and it has to hold the reply itself.
            let rendered = as_json.then(|| {
                json::render(&json::Report {
                    ending,
                    message: failure_of_a_turn(ending, not_served.as_deref()),
                    reply: outcome.reply_for_display(),
                    model: &outcome.model,
                    steps: outcome.steps,
                    tokens: json::Tokens {
                        total: outcome.tokens,
                        output: outcome.output_tokens,
                        context: outcome.context_tokens,
                        cache_read: outcome.cached.read_tokens,
                        cache_written: outcome.cached.written_tokens,
                    },
                    calls: reporter.calls(),
                    refusals: &refusals(&sink),
                    notices: &outcome.notices,
                })
            });

            let finished = Finished {
                reply: rendered.as_deref().unwrap_or(outcome.reply_for_display()),
                notices: &outcome.notices,
                attempt: attempt.as_deref(),
                trail: trace.then_some((&sink, outcome.model.as_str())),
                clean: outcome.clean,
                not_served: not_served.as_deref(),
            };
            report(
                &mut std::io::stdout().lock(),
                &mut std::io::stderr().lock(),
                &finished,
            );
            ending.code()
        }
        // A run that stopped is the one worth looking at, so what it produced is printed
        // whether or not --trace was asked for. Without it a failed plan is a one-line
        // complaint about a document nobody can see.
        Err(bravebot_agent::TurnError::Manifest { attempt, cause }) => {
            let ending = exit::ending_of(&cause);
            let stopped = fail(ending, &cause);
            let report = attempt.describe();
            if !report.is_empty() {
                eprintln!();
                eprint!("{report}");
            }
            if trace {
                eprintln!();
                print_trace(&mut std::io::stderr().lock(), &sink);
            }
            if as_json {
                say_the_result(&what_ran(
                    ending,
                    &cause.to_string(),
                    reporter.calls(),
                    &refusals(&sink),
                ));
            }
            stopped
        }
        Err(err) => {
            let ending = exit::ending_of(&err);
            let stopped = fail(ending, &err);
            if as_json {
                say_the_result(&what_ran(
                    ending,
                    &err.to_string(),
                    reporter.calls(),
                    &refusals(&sink),
                ));
            }
            stopped
        }
    }
}

/// Whether a result object was asked for, read straight off the command line.
///
/// Only for the case where the parse failed, which is the one moment the parsed invocation cannot
/// answer.
fn wants_json(args: &[String]) -> bool {
    args.iter().any(|arg| arg == "--json")
}

/// Stop before the turn: the identifier and the message on stderr, and a result object on stdout
/// where one was asked for.
fn stopped_before_the_turn(
    as_json: bool,
    ending: Ending,
    message: impl std::fmt::Display,
) -> ExitCode {
    let message = message.to_string();
    let stopped = fail(ending, &message);
    if as_json {
        say_the_result(&what_ran(ending, &message, &[], &[]));
    }
    stopped
}

/// Put a result object on stdout.
///
/// A failed write is dropped, as it is for the reply: a closed stdout is a caller that stopped
/// reading, and a run that has already finished should not die reporting what it did.
fn say_the_result(result: &str) {
    let _ = writeln!(std::io::stdout().lock(), "{result}");
}

/// How a turn that ran ended.
///
/// A turn something was refused in did not do what it was asked, and neither did one answered by
/// a model other than the one the command line named. Both are invisible to a script reading the
/// reply, which is what makes the status the only thing that can carry them.
fn ending_of_a_turn(clean: bool, named_on_the_command_line: bool, not_served: bool) -> Ending {
    if !clean {
        return Ending::Refused;
    }
    if named_on_the_command_line && not_served {
        return Ending::Failed;
    }
    Ending::Done
}

/// The sentence a result object explains a finished turn's failure with, if it has one.
///
/// The same words the run said on stderr, so the two surfaces do not disagree about why a turn
/// that produced a reply is still a failure.
fn failure_of_a_turn(ending: Ending, not_served: Option<&str>) -> Option<&str> {
    match ending {
        Ending::Done => None,
        Ending::Refused => Some(t!(cli_something_was_refused)),
        _ => not_served,
    }
}

/// Every refusal the trail holds, as a result object lists them.
fn refusals(sink: &RecordingSink) -> Vec<json::Refusal> {
    sink.blocked()
        .filter_map(|event| match event {
            Event::GateBlocked {
                gate,
                reason,
                principle,
                ..
            } => Some(json::Refusal {
                gate,
                principle: principle.name(),
                reason: reason.clone(),
            }),
            _ => None,
        })
        .collect()
}

/// The result object for a run with no reply to report.
///
/// Written even where nothing ran at all, because the alternative is a caller having to tell an
/// empty stdout from a result, which is the prose surface again with an extra step. A run that
/// failed part way through still says what it had done by then: the calls are what a caller needs
/// to know which of them to undo.
fn what_ran(
    ending: Ending,
    message: &str,
    calls: &[json::Call],
    refusals: &[json::Refusal],
) -> String {
    json::render(&json::Report {
        ending,
        message: Some(message),
        reply: "",
        model: "",
        steps: 0,
        tokens: json::Tokens::default(),
        calls,
        refusals,
        notices: &[],
    })
}

/// The rules a one-shot run works under.
///
/// Nobody is watching one, so the allow list is left out: an allow rule answers a prompt in
/// advance, and where there is nobody to ask it would instead be a line in a settings file letting
/// the run write, execute or fetch unwatched, beside the flag that is meant to be the only way
/// that happens. The deny and ask lists carry over, and both still decide something here.
///
/// With the flag, the person answered every one of those prompts themselves when they typed it, so
/// their file is read whole.
fn rules_for_a_one_shot_run(
    settings: &bravebot_config::Settings,
    home: Option<&Path>,
    skip_permissions: bool,
) -> (
    bravebot_core::permissions::Permissions,
    Vec<bravebot_core::permissions::Rejected>,
) {
    match skip_permissions {
        true => bravebot_agent::permissions::from_settings(settings, home),
        false => bravebot_agent::permissions::for_an_unattended_run(settings, home),
    }
}

/// Open every directory the command line named, or say which one could not be opened.
///
/// Reachability and nothing else. `/add-dir` grants a second thing, recording that the person
/// vouched for the directory, and this deliberately does not: a run nobody is watching holds an
/// empty trust map, the directory it was started in included, so a rule trusting a sibling
/// checkout would leave the tree the run was pointed at more trusted than the one it works in.
/// Reads there are on the same footing as reads of the project's own files.
///
/// `~` is left to the shell, which expands it before this ever sees the path. A path that is not
/// absolute, does not exist, is not a directory, or lies inside the working one is refused by the
/// workspace, and the refusal names which.
fn open_directories(workspace: &mut Workspace, directories: &[String]) -> Result<(), String> {
    for directory in directories {
        workspace.add_directory(directory).map_err(|problem| {
            t!(
                session_directory_not_added,
                directory = directory,
                problem = problem.to_string()
            )
        })?;
    }
    Ok(())
}

/// The model a run asks for: the one the command line named, else the one a session would read.
///
/// A run started from a script resolves a model the way a session opening in the same directory
/// does, so a script reaches the model somebody already chose without an interactive step, and
/// neither surface has a model the other cannot ask for. Below both is the configured model,
/// which is what an absent record leaves in force.
///
/// The command line outranks the record because it names a model for one run and nothing else,
/// which is the only way a script can pin one against a choice made elsewhere.
fn model_asked_for(named: Option<String>, stored: Option<String>) -> Option<String> {
    named.or(stored)
}

/// The model this run or session will ask a service for.
///
/// The same three sources the task below is built from, in the same order: a name given on the
/// command line, the one a session recorded, and the configured default. `named` is the raw
/// argument, resolved against the configuration here for the reason the task resolves it, since a
/// tier word names a model only the configuration knows.
///
/// A function because the question is asked before the task exists, and because an answer that
/// differed from the task's would refuse a run over a model it was never going to request.
fn model_for_this_run(named: Option<&str>, config: &Config) -> String {
    model_asked_for(
        named.map(|name| config.model_named(name)),
        bravebot_tui::store::load_model(),
    )
    .unwrap_or_else(|| config.default_model.clone())
}

/// The complaint a run has when one model was asked for and another answered, if it has one.
///
/// The endpoint substitutes rather than refusing, so the reported name is the only trace there is.
/// About the model in force whatever named it: the flag, a remembered choice and the settings key
/// all pin a model somebody expects to be answered by.
///
/// Nothing where the name asks for whichever model the server picks rather than for a particular
/// one, because resolving to a model is what that name is for. Nothing where the backend does not
/// report the name it was asked for either, where a reply that says something else is the
/// indirection working rather than a different model answering.
fn model_not_served(asked: &str, comparable: bool, served: &str) -> Option<String> {
    if !comparable || asked == bravebot_config::DEFAULT_MODEL || asked == served {
        return None;
    }
    Some(t!(
        session_model_substituted,
        asked = asked,
        served = served
    ))
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

/// What a one-shot run answers its questions with: [`bravebot_agent::Unattended`], except that a
/// plan is put to whoever typed the command.
///
/// Every other question is refused, as CLI-1 has it. Those are due in the middle of a run, over a
/// path or a program nobody undertook to watch for, and there is nothing about typing a command that
/// says somebody will still be there. A plan is asked at one known moment instead: once, before the
/// first step, while the command that raised it has printed nothing but its own progress. The person
/// who typed it is the person reading that, so there is somebody to ask.
///
/// Generic over both ends, as [`piped_input`] is over its source, so a test answers without a
/// terminal.
struct OneShot<R: Read, W: Write> {
    /// Where the answer is read from, and only ever after the question was written.
    ///
    /// Buffered here rather than taken buffered, because the whole confirmer travels to the thread
    /// the turn runs on and a held stdin lock cannot.
    input: std::io::BufReader<R>,
    /// Where the plan and the question go. stderr in a real run, which is what keeps stdout the
    /// reply alone (CLI-5).
    output: W,
    /// Whether there is anybody to ask.
    ///
    /// Both ends being a terminal, not stdin alone. A plan written into a redirected stderr is a
    /// plan nobody read, and running a program nobody was shown is the one thing this question
    /// cannot mean.
    attended: bool,
    /// Every question but the plan. Delegated rather than restated so there is one account of what
    /// a one-shot refuses, and adding a question here cannot quietly start approving it.
    refusing: bravebot_agent::Unattended,
}

impl<R: Read, W: Write> OneShot<R, W> {
    fn new(input: R, output: W, attended: bool) -> Self {
        Self {
            input: std::io::BufReader::new(input),
            output,
            attended,
            refusing: bravebot_agent::Unattended,
        }
    }

    /// Write the question out, then read the answer back.
    ///
    /// The steps are not printed here. The run narrates the frozen plan a line per step immediately
    /// before asking, from the same renderer this request was built with, so on a terminal they are
    /// the lines directly above the question and are still on screen while it is answered. Printing
    /// them again would put the same list twice under two different headings, which reads as two
    /// plans rather than one. The panel a session draws does repeat them, and has to: it covers the
    /// transcript that showed them.
    fn put_the_plan(&mut self, request: &ManifestRequest) -> std::io::Result<Decision> {
        writeln!(self.output, "{}", t!(plan_title))?;
        writeln!(
            self.output,
            "{} {}  {}",
            t!(plan_verb),
            t!(plan_steps, count = request.steps.len()),
            t!(plan_goal, task = &request.task)
        )?;
        writeln!(self.output)?;
        for sentence in [
            t!(plan_explained),
            t!(plan_not_its_writes),
            t!(plan_nothing_yet),
        ] {
            writeln!(self.output, "{sentence}")?;
        }
        write!(self.output, "{} ", t!(plan_answer))?;
        self.output.flush()?;

        // A closed stdin reads nothing, which is nobody answering, which is a no. So is any other
        // line: the affirmative is the only answer that runs a program, and a person who typed
        // something else did not type that.
        let mut answer = String::new();
        self.input.read_line(&mut answer)?;
        Ok(match answer.trim().to_lowercase() == t!(plan_answer_yes) {
            true => Decision::Approve,
            false => Decision::Reject,
        })
    }
}

impl<R: Read, W: Write> Confirmer for OneShot<R, W> {
    /// The one question this confirmer may answer yes to, and only where somebody is there.
    fn confirm_manifest(&mut self, request: &ManifestRequest) -> Decision {
        if !self.attended {
            return self.refusing.confirm_manifest(request);
        }
        // A question that could not be written is a plan nobody saw, so it is declined rather than
        // taken as unanswered and waved through.
        self.put_the_plan(request).unwrap_or(Decision::Reject)
    }

    fn confirm_write(&mut self, request: &WriteRequest) -> Decision {
        self.refusing.confirm_write(request)
    }

    fn confirm_run(&mut self, request: &RunRequest) -> RunDecision {
        self.refusing.confirm_run(request)
    }

    fn confirm_read_output(&mut self, request: &OutputRequest) -> Decision {
        self.refusing.confirm_read_output(request)
    }

    fn confirm_vetted_read(&mut self, request: &VetRequest) -> Decision {
        self.refusing.confirm_vetted_read(request)
    }

    fn confirm_fetch(&mut self, request: &FetchRequest) -> Decision {
        self.refusing.confirm_fetch(request)
    }

    fn confirm_server(&mut self, request: &ServerRequest) -> Decision {
        self.refusing.confirm_server(request)
    }

    fn confirm_vouch(&mut self, request: &VouchRequest) -> Decision {
        self.refusing.confirm_vouch(request)
    }

    /// Declined rather than answered, as everywhere nobody can be asked: a reply invented here would
    /// be reported to the planner as the person's own words.
    fn ask_user(&mut self, asking: &Asking) -> Vec<Answer> {
        self.refusing.ask_user(asking)
    }

    /// Nothing. Nobody is typing between steps: the only line this reads is an answer to a question
    /// it just asked.
    fn interjection(&mut self) -> Option<String> {
        self.refusing.interjection()
    }
}

/// What a finished turn has to say, before anything decides where it goes.
struct Finished<'a> {
    /// What goes on stdout: the released reply, or the result object `--json` asked for in its
    /// place.
    reply: &'a str,
    /// The driver's own words about what loaded and what did not, never anything read out of a
    /// file.
    notices: &'a [String],
    /// What a traced manifest run planned, when there was one. On stderr with the trail,
    /// never on stdout: a pipe of the reply must not pick up the plan.
    attempt: Option<&'a str>,
    /// The trail and the model behind it, when `--trace` asked for them.
    trail: Option<(&'a RecordingSink, &'a str)>,
    /// Whether no gate refused anything during the turn.
    clean: bool,
    /// What to say where one model was asked for and another one answered.
    not_served: Option<&'a str>,
}

/// Write a finished turn: the reply to `reply`, every other word to `beside`.
///
/// Which stream each part lands on is the whole of what this decides, so it takes both rather
/// than reaching for stdout and stderr itself: a run's output is then something a test can read
/// back. A notice or an audit trail sharing the reply's stream would corrupt whatever the reply
/// was piped into.
fn report(reply: &mut impl Write, beside: &mut impl Write, run: &Finished<'_>) {
    for notice in run.notices {
        let _ = writeln!(beside, "{}", t!(cli_notice, notice = notice));
    }
    if let Some(complaint) = run.not_served {
        let _ = writeln!(beside, "{complaint}");
    }
    let _ = writeln!(reply, "{}", run.reply);
    if let Some(attempt) = run.attempt {
        let _ = writeln!(beside);
        let _ = write!(beside, "{attempt}");
    }
    if let Some((sink, model)) = run.trail {
        let _ = writeln!(beside);
        print_trace(beside, sink);
        let _ = writeln!(beside, "{}", t!(cli_model_used, model = model));
    }
    if !run.clean {
        let _ = writeln!(beside);
        let _ = writeln!(beside, "{}", t!(cli_something_was_refused));
    }
}

/// Print the audit trail: what was checked, allowed, and refused.
///
/// A failed write is dropped, as it is for progress. The trail describes a turn that has already
/// happened, so a closed stderr means nobody is reading it, not that the run should die holding a
/// reply it has already produced.
fn print_trace(output: &mut impl Write, sink: &RecordingSink) {
    macro_rules! trace {
        ($($arg:tt)*) => {
            { let _ = writeln!(output, $($arg)*); }
        };
    }

    trace!("audit trail");
    for (delegate, event) in sink.recorded() {
        // Which run took the decision, in front of what it decided. A turn and the delegates it
        // spawned record into this one trail, and two delegates of the same kind decide alike.
        let run = match delegate {
            Some(delegate) => format!("{delegate} "),
            None => String::new(),
        };
        match event {
            Event::GatePassed { gate, detail } => trace!("  ok      {run}{gate}: {detail}"),
            Event::GateBlocked { gate, reason, .. } => trace!("  BLOCK   {run}{gate}: {reason}"),
            Event::Observed { capability, label } => {
                trace!("  observe {run}{capability} produced {label}")
            }
            Event::SlotWritten { slot, label } => trace!("  slot    {run}{slot} at {label}"),
            Event::SlotDeferred {
                slot,
                label,
                origin,
            } => trace!("  defer   {run}{slot} holds {origin}, unread, at {label}"),
            Event::Declassified { slot, from, to, .. } => {
                trace!("  release {run}{slot} {from} -> {to}")
            }
            Event::ActionField {
                tool,
                field,
                role,
                label,
                allowed,
            } => {
                let mark = if *allowed { "ok     " } else { "BLOCK  " };
                let role = match role {
                    Role::Routing => "routing",
                    Role::Content => "content",
                };
                trace!("  {mark} {run}{tool}.{field} [{role}] {label}");
            }
        }
    }
}

fn resume_named(id: &str, skip_permissions: bool) -> ExitCode {
    let Ok(directory) = std::env::current_dir() else {
        return fail(Ending::Failed, t!(cli_directory_unknown));
    };
    match bravebot_tui::sessions::load(&directory, id) {
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
        Some(record) => interactive(
            bravebot_tui::app::Start::Resuming(Box::new(record)),
            skip_permissions,
        ),
        None => fail(Ending::Argument, t!(cli_no_such_session, id = id)),
    }
}

fn fork_named(id: &str, skip_permissions: bool) -> ExitCode {
    let Ok(directory) = std::env::current_dir() else {
        return fail(Ending::Failed, t!(cli_directory_unknown));
    };
    match bravebot_tui::sessions::load(&directory, id) {
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
        Some(_) => match bravebot_tui::sessions::fork(&directory, id) {
            Some(record) => interactive(
                bravebot_tui::app::Start::Resuming(Box::new(record)),
                skip_permissions,
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
fn continue_here(skip_permissions: bool) -> ExitCode {
    let Ok(directory) = std::env::current_dir() else {
        return fail(Ending::Failed, t!(cli_directory_unknown));
    };
    match bravebot_tui::sessions::most_recent(&directory) {
        // By the id, so this arrives at the interface the way a named resume does, down to a
        // record that went away between the list and the read.
        Some(session) => resume_named(&session.id, skip_permissions),
        None => fail(Ending::Failed, t!(cli_nothing_to_continue)),
    }
}

fn interactive(start: bravebot_tui::app::Start, skip_permissions: bool) -> ExitCode {
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
    if let bravebot_agent::backend::Serving::NothingConfigured {
        subscription,
        a_service_is_configured,
    } = bravebot_agent::backend::serving(
        &config,
        &bravebot_net::Egress::new(),
        &model_for_this_run(None, &config),
    ) {
        return fail(
            Ending::Configuration,
            how_to_configure_a_model(subscription.as_deref(), a_service_is_configured),
        );
    }

    let workspace = match current_workspace(&bravebot_config::Settings::load()) {
        Ok(w) => w,
        Err(err) => return fail(Ending::Failed, t!(cli_workspace_problem, problem = err)),
    };

    // Reported in the status bar so the guarantee in force is visible for the whole
    // session rather than assumed.
    let confinement = match bravebot_sandbox::for_current_platform() {
        Ok(sandbox) => named(sandbox.capabilities().level),
        Err(_) => named(bravebot_sandbox::policy::ConfinementLevel::None),
    };

    match bravebot_tui::app::run(
        &mut config,
        &workspace,
        confinement,
        start,
        skip_permissions,
    ) {
        // Printed after the terminal is handed back, so it survives on the screen the person is
        // left looking at rather than going onto the alternate screen with everything else. A
        // session is worth resuming far more often than anybody thinks to write its name down
        // beforehand, and the picker is no help to someone who has already closed the window.
        Ok(Some(left)) => {
            println!("{}", resume_hint(&left, workspace.root()));
            ExitCode::SUCCESS
        }
        Ok(None) => ExitCode::SUCCESS,
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

/// What to call the confinement that was achieved.
///
/// Named here rather than by `bravebot_sandbox`, whose Display is a diagnostic and whose whole
/// point is to hold no words meant for a person: it depends on nothing, and a catalog is a
/// dependency. So it reports which of the three it got and this says it in the reader's language.
fn named(level: bravebot_sandbox::policy::ConfinementLevel) -> String {
    use bravebot_sandbox::policy::ConfinementLevel;
    match level {
        ConfinementLevel::Kernel => t!(confinement_kernel),
        ConfinementLevel::Partial => t!(confinement_partial),
        ConfinementLevel::None => t!(confinement_none),
    }
    .to_string()
}

/// The directory this run writes what is not part of the project into, made and reachable.
///
/// Nothing on a machine that cannot give it one, which is not a reason to refuse to run: a full or
/// read-only temporary directory leaves a turn with nowhere to put an intermediate file and nothing
/// else. On stderr, beside every other line this run has to say about itself, because a turn told
/// there is nowhere to write leaves nothing else to read it off.
fn scratch_for_this_run(workspace: &mut Workspace) -> Option<bravebot_agent::SessionScratch> {
    let scratch = match bravebot_agent::SessionScratch::create() {
        Ok(scratch) => Some(scratch),
        Err(problem) => {
            eprintln!(
                "{}",
                t!(
                    cli_notice,
                    notice = t!(session_scratch_unavailable, problem = problem.to_string())
                )
            );
            None
        }
    };
    workspace.open_scratch(scratch.as_ref().map(|held| held.path().to_path_buf()));
    scratch
}

/// The workspace is the current directory: file arguments resolve relative to it, and
/// confinement keeps reads inside it.
///
/// The search caps come in from the settings here rather than being read inside the workspace,
/// because a workspace is built by every test in the tree and one that read the settings would
/// answer differently on a machine whose owner had configured them.
fn current_workspace(settings: &bravebot_config::Settings) -> Result<Workspace, String> {
    let caps = settings.search();
    std::env::current_dir()
        .map_err(|e| e.to_string())
        .and_then(|dir| Workspace::new(dir).map_err(|e| e.to_string()))
        .map(|workspace| workspace.with_search_caps(caps.files, caps.time))
}

/// Import a Leo Premium subscription from a local Brave install.
///
/// This registers as an *additional* device rather than taking the browser's credentials, so the
/// browser keeps its own and nothing it holds is spent. Only the order id is read from the
/// profile; the credentials themselves are minted here and signed by Brave's service.
fn import_leo_creds(args: &[String]) -> ExitCode {
    let mut channel = None;
    let mut forget = false;

    for arg in args {
        match arg.as_str() {
            "--forget" => forget = true,
            other if other.starts_with('-') => {
                return fail(Ending::Argument, t!(cli_unknown_option, flag = other));
            }
            other => match bravebot_skus::Channel::parse(other) {
                Some(parsed) => channel = Some(parsed),
                None => {
                    let refused = fail(Ending::Argument, t!(leo_unknown_channel, channel = other));
                    eprintln!("{}", t!(leo_expected_channel));
                    return refused;
                }
            },
        }
    }

    // Stable is what someone importing without saying which install means.
    let channel = channel.unwrap_or(bravebot_skus::Channel::Stable);

    // There is one stored batch, so forgetting takes no channel: naming one would suggest
    // `--forget nightly` leaves a stable import in place, and it does not.
    if forget {
        return match bravebot_skus::store::clear() {
            Ok(()) => {
                println!("{}", t!(leo_forgotten));
                ExitCode::SUCCESS
            }
            Err(err) => fail(Ending::Failed, err),
        };
    }

    // Refused rather than silently skipped, and refused before the device is registered so a
    // batch is not minted that nothing will ever be able to spend. An import is a write by
    // definition: a credential that did not outlive the session would not be an import. It is the
    // one command an incognito session cannot carry out rather than merely decline to record.
    // Forgetting above is allowed: removing a stored secret leaves less behind, not more.
    if bravebot_core::incognito::engaged() {
        return fail(Ending::Failed, t!(leo_not_while_incognito));
    }

    // Warned about early: the import would otherwise succeed and then never be used, since a
    // credential is only ever sent to the premium host.
    match Config::from_env() {
        Ok(config) if config.premium_endpoint.is_none() => {
            eprintln!("{}", t!(leo_no_premium_endpoint));
            eprintln!(
                "         {}",
                t!(
                    leo_set_and_rebuild,
                    variable = bravebot_config::env_var::PREMIUM_ENDPOINT
                )
            );
        }
        _ => {}
    }

    println!("{}", t!(leo_looking, channel = channel.as_str()));

    let order = match bravebot_skus::find_leo_order(channel) {
        Ok(order) => order,
        Err(err) => return fail(Ending::Failed, err),
    };

    println!(
        "{}",
        t!(
            leo_found,
            environment = order.environment.as_str(),
            order = &order.order_id
        )
    );
    println!("{}", t!(leo_registering));

    // A fresh request id is what makes this a new device rather than a claim on an existing
    // device's batch.
    let request_id = bravebot_skus::new_request_id();

    let registration = match bravebot_skus::device::register(
        order.environment,
        &order.order_id,
        &request_id,
        Transport::shared().agent_config_builder(),
    ) {
        Ok(registration) => registration,
        Err(err) => return fail(Ending::Failed, err),
    };

    let credentials: bravebot_skus::StoredCredentials = registration.into();
    let count = credentials.credentials.len();
    let last = credentials
        .credentials
        .iter()
        .map(|c| c.valid_to.as_str())
        .max()
        .unwrap_or("unknown")
        .to_string();

    if let Err(err) = bravebot_skus::store::save(&credentials) {
        return fail(Ending::Failed, err);
    }

    // Named so the user knows where the secret went: it is an ordinary file now, and one they may
    // want to inspect, exclude from a backup, or delete by hand.
    let where_stored = match bravebot_skus::store::path() {
        Ok(path) => path.display().to_string(),
        Err(err) => return fail(Ending::Failed, err),
    };

    println!(
        "{}",
        t!(
            leo_stored,
            count = count,
            path = where_stored,
            expiry = last
        )
    );
    println!("{}", t!(leo_browser_untouched));
    ExitCode::SUCCESS
}

/// Report whether configuration is usable, without revealing the signing key.
fn doctor() -> ExitCode {
    let mut ok = true;

    // Read before the configuration so the file can be reported even when it is what made the
    // configuration wrong.
    let settings = bravebot_config::Settings::load();
    let managed = Managed::load();

    // Resolved once for the two sections that need it, since two answers to where the state
    // directory is would be two answers to which rules a run reads.
    let resolved = bravebot_agent::home::resolved();
    let home = resolved.as_ref().map(|(_, path)| path.as_path());

    match Config::from_env_and_settings(&settings, &managed) {
        Ok(config) => {
            println!("{}", t!(doctor_configuration_ok));

            // Names only. On some machines a value here is a credential, and a diagnostic that
            // prints one is a diagnostic people paste into issues.
            //
            // The variables the file names, not everything it configured: a gateway is a block rather
            // than a variable and gets a section of its own below. A file that only configures one
            // therefore names nothing here, which is absence rather than an empty list.
            let named: Vec<&str> = settings.names().collect();
            match (named.is_empty(), settings.is_empty()) {
                (false, _) => fact(
                    t!(doctor_settings),
                    t!(doctor_settings_names, names = named.join(", ")),
                ),
                (true, false) => fact(t!(doctor_settings), t!(doctor_settings_no_variables)),
                (true, true) => fact(t!(doctor_settings), t!(doctor_settings_absent)),
            }

            // Which files are in force, weakest first, and then only the names where that order
            // decided something. A person reading a value they did not expect has three places it
            // could have come from, and the paths are the whole of what tells them which.
            for layer in settings.layers() {
                fact(t!(doctor_settings_layer), layer.display().to_string());
            }
            for (name, path) in settings.overridden() {
                fact(
                    t!(doctor_settings_override),
                    t!(
                        doctor_settings_overridden,
                        name = name,
                        path = path.display().to_string()
                    ),
                );
            }

            // After the layers a person owns, because it is what answers for a name none of them
            // explains: a value they set and cannot see taking effect is pinned above all of them.
            for line in managed_layer(&managed) {
                println!("{line}");
            }

            // Counted rather than listed: a rule is the user's own text and printing it back says
            // nothing they cannot read in the file. What is worth saying is which of them this
            // build could not act on, because those are the ones that look like protection and
            // are not.
            let (permissions, rejected) =
                bravebot_agent::permissions::from_settings(&settings, home);
            fact(
                t!(doctor_permissions),
                match permissions.is_empty() {
                    true => t!(doctor_permissions_absent).to_string(),
                    false => t!(doctor_permissions_count, count = permissions.len() as i64),
                },
            );
            for problem in &rejected {
                ok = false;
                fact(t!(doctor_permissions_unreadable), problem.to_string());
            }

            // Both, where both are reachable, because both are offered to a person choosing and a
            // report naming one of them explains only the half of the picker they happened to use.
            if config.serves_aichat() {
                report_aichat(&config);
            }
            if let Some(bedrock) = config.bedrock.as_ref() {
                report_bedrock(bedrock);
            }
            for provider in &config.providers {
                report_gateway(provider);
            }

            // What a run would actually request, since a choice made with `/model` overrides the
            // configured default and reporting only the default would explain the wrong thing.
            match bravebot_tui::store::load_model() {
                Some(chosen) => fact(t!(doctor_model), t!(doctor_model_chosen, model = chosen)),
                None => fact(
                    t!(doctor_model),
                    t!(doctor_model_default, model = &config.default_model),
                ),
            }

            // Only where a subscription means something. A Leo credential is what the premium half of
            // the Brave roster needs, and it means nothing to Bedrock.
            if config.serves_aichat() {
                report_subscription();
            }

            // Last of the configuration section, and a failure, because a report that said
            // "configuration OK" about a machine where a session refuses to start is the one
            // thing a person in that position is certain to read first.
            if let bravebot_agent::backend::Serving::NothingConfigured {
                subscription,
                a_service_is_configured,
            } = bravebot_agent::backend::serving(
                &config,
                &bravebot_net::Egress::new(),
                &model_for_this_run(None, &config),
            ) {
                ok = false;
                println!();
                println!(
                    "{}",
                    how_to_configure_a_model(subscription.as_deref(), a_service_is_configured)
                );
            }
        }
        Err(err) => {
            eprintln!("{}", t!(cli_configuration_problem, problem = err));
            ok = false;
            // The one line from the section above that still has to be printed. A pin is the case
            // where the person reading this can do nothing about the error, so the file that holds
            // it is the only actionable thing in the report.
            for line in managed_layer(&managed) {
                println!("{line}");
            }
        }
    }

    println!();
    // Outside the block above, which a configuration error stops before it prints anything: where
    // the state is kept is a fact about the machine either way, and a machine with nowhere to keep
    // it is one of the reasons the configuration above it can be wrong.
    for line in state_directory(
        resolved
            .as_ref()
            .map(|(variable, path)| (*variable, path.as_path())),
        bravebot_agent::home::PROFILE_VARIABLES,
        RESTRICTED,
    ) {
        println!("{line}");
    }

    println!();
    // Outside the configuration block for the same reason the state directory is: what a handshake
    // is validated against and what a request is routed through are facts about the machine, and
    // they are most often what is wanted when the configuration above them looks right and nothing
    // connects.
    let transport = Transport::shared();
    for line in network(transport) {
        println!("{line}");
    }
    // Any of the three is a statement about this machine that the program is not honouring,
    // which is what a report exits non-zero over: nothing is trusted, a named path holds nothing, or
    // a proxy was named that requests are not taking.
    if transport.trust_problem().is_some() || transport.unusable_proxy().is_some() {
        ok = false;
    }

    println!();
    // Not a warning: without confinement, untrusted work will be refused rather than run, so
    // this is a hard problem for the user to solve.
    if !report_confinement(
        bravebot_sandbox::for_current_platform().map(|sandbox| sandbox.capabilities()),
    ) {
        ok = false;
    }

    // Development setup is advisory: released binaries need neither facility.
    if let Ok(cwd) = std::env::current_dir() {
        let lines = development(&cwd, std::env::var_os("PATH").as_deref(), cfg!(windows));
        if !lines.is_empty() {
            println!();
        }
        for line in lines {
            println!("{line}");
        }
    }

    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// Recognise the source tree by its checked-in layout, including from a subdirectory.
/// No Git command or setup script is run by this read-only check.
fn development(cwd: &Path, path: Option<&std::ffi::OsStr>, windows: bool) -> Vec<String> {
    let mut boundary = false;
    let mut ancestors = cwd.ancestors().take_while(|root| {
        let include = !boundary;
        boundary = root.join(".git").exists();
        include
    });
    let Some(root) = ancestors.find(|root| {
        [
            "Cargo.toml",
            "crates/cli/Cargo.toml",
            "agents/setup.py",
            "agents/AGENTS.md",
            "docs/development/agent-configuration.md",
        ]
        .iter()
        .all(|marker| root.join(marker).is_file())
    }) else {
        return Vec::new();
    };
    vec![
        t!(doctor_development, path = root.display().to_string()),
        aligned("AGENTS.md", agent_discovery(root, windows), FACT),
        aligned(
            "direnv",
            if direnv_available(path) {
                t!(doctor_direnv_ok)
            } else {
                t!(doctor_direnv_missing)
            },
            FACT,
        ),
    ]
}

fn agent_discovery(root: &Path, windows: bool) -> String {
    let source = root.join("agents/AGENTS.md");
    let destination = root.join("AGENTS.md");
    match std::fs::symlink_metadata(&destination) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            match (destination.canonicalize(), source.canonicalize()) {
                (Ok(actual), Ok(expected)) if actual == expected => {
                    t!(doctor_agents_ok).to_string()
                }
                (Ok(_), _) => t!(doctor_agents_wrong).to_string(),
                _ => t!(doctor_agents_broken).to_string(),
            }
        }
        Ok(metadata) if windows && metadata.is_file() => {
            match (std::fs::read(&destination), std::fs::read(&source)) {
                (Ok(actual), Ok(expected)) if actual == expected => {
                    t!(doctor_agents_copy_ok).to_string()
                }
                _ => t!(doctor_agents_copy_stale).to_string(),
            }
        }
        Ok(_) => t!(doctor_agents_conflict).to_string(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            t!(doctor_agents_missing).to_string()
        }
        Err(_) => t!(doctor_agents_unreadable).to_string(),
    }
}

/// Inspect PATH without executing a tool or changing the process environment.
fn direnv_available(path: Option<&std::ffi::OsStr>) -> bool {
    path.is_some_and(|path| {
        std::env::split_paths(path).any(|directory| {
            let executable = directory.join(if cfg!(windows) {
                "direnv.exe"
            } else {
                "direnv"
            });
            std::fs::metadata(executable).is_ok_and(|metadata| {
                if !metadata.is_file() {
                    return false;
                }
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    metadata.permissions().mode() & 0o111 != 0
                }
                #[cfg(not(unix))]
                {
                    true
                }
            })
        })
    })
}

/// What `doctor` says about the Bedrock half of the roster.
///
/// No key and no endpoint: there is no key, and the host is derived from the region rather than
/// configured. The credentials are the AWS CLI's to hold, which is why the profile is the useful
/// thing to report and there is nothing here to redact.
fn report_bedrock(bedrock: &bravebot_config::bedrock::Bedrock) {
    fact(t!(doctor_backend), t!(doctor_backend_bedrock));
    fact(t!(doctor_region), &bedrock.region);
    match bedrock.profile.as_deref() {
        Some(profile) => fact(t!(doctor_profile), profile),
        None => fact(t!(doctor_profile), t!(doctor_profile_absent)),
    }

    // The tiers a person may choose, by name. An ARN is unreadable and looks identical between
    // tiers, so the names are what tells someone whether the block did what they meant.
    match bedrock.models().is_empty() {
        false => fact(
            t!(doctor_tiers),
            bedrock
                .models()
                .iter()
                .map(bravebot_config::bedrock::Entry::display_name)
                .collect::<Vec<_>>()
                .join(", "),
        ),
        true => fact(t!(doctor_tiers), t!(doctor_tiers_absent)),
    }
}

/// What `doctor` says about one configured gateway.
///
/// Whether a credential can be found rather than what it is, because on this path the value is a
/// bearer token and a diagnostic that printed one is a diagnostic people paste into issues. Which
/// models, because a gateway's own roster is far larger than the block names and the listed slugs are
/// what tells someone whether the block did what they meant.
fn report_gateway(provider: &bravebot_config::provider::Provider) {
    fact(
        t!(doctor_backend),
        t!(doctor_backend_gateway, gateway = provider.display_name()),
    );
    fact(t!(doctor_endpoint), provider.chat_completions_url());
    fact(
        t!(doctor_key_name),
        gateway_credential(provider, |name| std::env::var(name).ok()),
    );
    match provider.models.is_empty() {
        false => fact(
            t!(doctor_tiers),
            provider
                .models
                .iter()
                .map(|model| model.id.as_str())
                .collect::<Vec<_>>()
                .join(", "),
        ),
        true => fact(t!(doctor_tiers), t!(doctor_gateway_models_absent)),
    }
}

/// What `doctor` says about a gateway's credential: that one was found, never what it is.
///
/// Separate from the printing so the withholding is testable. The value here is a long-lived bearer
/// token, so a diagnostic that echoed one would put a live credential in every issue somebody pastes
/// this into.
///
/// Three answers, because a block that named nowhere for a credential to live needs none and there is
/// nothing for anybody to go and set. Reported as absent, it reads as the thing to fix on a gateway
/// that is working.
fn gateway_credential(
    provider: &bravebot_config::provider::Provider,
    lookup: impl Fn(&str) -> Option<String>,
) -> &'static str {
    match provider.credential(lookup) {
        bravebot_config::provider::Credential::Token(_) => t!(doctor_gateway_token),
        bravebot_config::provider::Credential::Absent => t!(doctor_gateway_token_absent),
        bravebot_config::provider::Credential::NotNeeded => t!(doctor_gateway_token_not_needed),
    }
}

/// What `doctor` says about a build pointed at the Brave backend.
fn report_aichat(config: &Config) {
    fact(t!(doctor_backend), t!(doctor_backend_aichat));
    fact(t!(doctor_endpoint), config.chat_completions_url());
    match config.premium_chat_completions_url() {
        Some(url) => fact(t!(doctor_premium), &url),
        None => fact(t!(doctor_premium), t!(doctor_premium_absent)),
    }
    fact(t!(doctor_key_id), &config.key_id);
    // Redacting `Display`, so what is reported is the placeholder rather than the key.
    fact(
        t!(doctor_key_name),
        t!(doctor_key, key = &config.signing_key),
    );
}

/// Where the values line up in what `doctor` reports, and in its confinement section.
const FACT: usize = 10;
const DETAIL: usize = 17;

/// One line of what `doctor` found: a name, then what it is, in two columns.
///
/// The gap is computed rather than typed into each line, since a translated name is not the
/// length the English one was and a column of hand-counted spaces stops being a column. Counted
/// in characters, which is not the width a terminal draws for every script, but is much closer
/// to it than a count of bytes and needs nothing to work it out.
fn aligned(name: impl AsRef<str>, value: impl AsRef<str>, column: usize) -> String {
    let name = name.as_ref();
    let gap = column.saturating_sub(name.chars().count()).max(1);
    format!("  {name}{}{}", " ".repeat(gap), value.as_ref())
}

fn fact(name: impl AsRef<str>, value: impl AsRef<str>) {
    println!("{}", aligned(name, value, FACT));
}

/// What `doctor` says about the machine-level layer, which is nothing where there is no such file.
///
/// The names it pinned rather than the values, on the same footing as the settings above: a value
/// here is a host. Naming them is the whole point of the line, since a name in this list is the
/// answer to why a variable somebody exported is changing nothing.
///
/// A file that was read is named even where nothing in it could be pinned, because the alternative
/// leaves whoever wrote it with no way to tell a file this program never found from one holding
/// names it may not honour.
fn managed_layer(managed: &Managed) -> Vec<String> {
    let Some(path) = managed.path() else {
        return Vec::new();
    };
    let path = path.display().to_string();
    let pinned: Vec<&str> = managed.pinned().collect();
    vec![aligned(
        t!(doctor_managed),
        match pinned.is_empty() {
            true => t!(doctor_managed_nothing, path = &path).to_string(),
            false => t!(
                doctor_managed_pinned,
                names = pinned.join(", "),
                path = &path
            )
            .to_string(),
        },
        FACT,
    )]
}

/// Report the imported subscription, and how much of it is left.
///
/// Counts only: a credential is a bearer secret, so none of it is printed. The environment rather
/// than the channel it came from, because that is what decides whether the batch can be spent
/// against the endpoint this build talks to, and it is what the file records.
fn report_subscription() {
    if let Ok(stored) = bravebot_skus::store::load() {
        fact(
            t!(doctor_leo),
            t!(
                doctor_subscription,
                environment = stored.environment.as_str(),
                unspent = stored.remaining(),
                total = stored.credentials.len()
            ),
        );
    }
}

/// Whether a file created under the state directory is reachable only by the account that owns it.
///
/// [`bravebot_agent::home::create_directory`] and [`bravebot_agent::home::write_file`] ask for that
/// mode as they create, and on a platform with no mode to ask for they cannot: the files carry
/// whatever the profile directory grants them instead. Read here and passed in, so a test on either
/// platform holds what the section says in both cases.
const RESTRICTED: bool = cfg!(unix);

/// The state directory section of `doctor`: where it is, or that there is none and what that costs.
///
/// Built rather than printed, so what the section says is a value a test can hold.
///
/// An absent directory is reported rather than failed on: STATE-2 makes no `HOME` a state this
/// program supports, and a container that has none of it wanted none of it. Saying so is still
/// owed, because every subsystem treats the absence as absence and none of them says a word. The
/// `settings` line above says at most that no file was found, which reads as a file nobody has
/// written rather than a directory there is nowhere to put.
///
/// Both halves are named because the absence is partial. A checkout's `.bravebot/settings.json`,
/// its skills and its `AGENTS.md` are read with no home at all, so a report that said only
/// "settings are not kept" would have somebody looking for why the file in front of them is being
/// ignored when it is in force.
///
/// A directory that is there is reported with the variable that named it, since more than one can
/// and the one that answered is what somebody has to change to put the directory elsewhere. It is
/// also where `restricted` is spent: a person keeping a shared or synced profile is owed the
/// difference between files this program narrowed and files carrying whatever they inherited, and
/// the state directory is the only section that could tell them.
///
/// `variables` is passed in for the same reason `restricted` is: the list is the one thing here that
/// differs by platform, and a host that states a profile directory in one variable could otherwise
/// not hold what the report says on a host that states it in two.
fn state_directory(
    found: Option<(&str, &Path)>,
    variables: &[&str],
    restricted: bool,
) -> Vec<String> {
    let Some((variable, path)) = found else {
        let variables = variables.join(" or ");
        return vec![
            t!(doctor_state_directory_absent, variables = &variables).to_string(),
            aligned(
                t!(doctor_state_directory_not_kept),
                t!(doctor_state_directory_forgotten),
                DETAIL,
            ),
            aligned(
                t!(doctor_state_directory_not_read),
                t!(doctor_state_directory_your_own),
                DETAIL,
            ),
            aligned(
                t!(doctor_state_directory_remedy),
                t!(doctor_state_directory_set_profile, variables = &variables),
                DETAIL,
            ),
        ];
    };
    let mut lines = vec![
        t!(
            doctor_state_directory,
            path = path.display().to_string(),
            variable = variable
        )
        .to_string(),
    ];
    if !restricted {
        lines.push(aligned(
            t!(doctor_state_directory_unprotected),
            t!(doctor_state_directory_permissions),
            DETAIL,
        ));
    }
    lines
}

/// The network section of `doctor`: the certificate authorities a handshake is validated against,
/// and the proxy a request goes through.
///
/// Built rather than printed, so what the section says is a value a test can hold.
///
/// Both exist to answer the failure that has nothing to say for itself. A machine behind a
/// TLS-inspecting proxy refuses every connection with a certificate error naming an authority the
/// user has already installed, and a machine with a proxy variable set sends every request
/// somewhere the rest of the report does not mention. Neither is visible anywhere else, and the
/// second is the one nobody thinks to check.
///
/// The proxy is named by protocol, host and port. Its uri is not printed, because a proxy that
/// requires a credential carries it there and a diagnostic that echoed one would put a live
/// password in every issue somebody pastes this into. That a credential is in use is still said:
/// a proxy rejecting an unauthenticated request is one of the failures this is run to explain.
/// A list of variable names as a sentence reads one: commas, and `or` before the last.
///
/// Every list here is a set of variables somebody has to go and set one of, so joining them all
/// with `or` would have the reader parsing a report rather than reading a remedy.
fn listed(names: &[&str]) -> String {
    match names.split_last() {
        None => String::new(),
        Some((last, [])) => (*last).to_string(),
        Some((last, rest)) => format!("{} or {last}", rest.join(", ")),
    }
}

fn network(transport: &Transport) -> Vec<String> {
    let roots = match (transport.roots(), transport.trusts_nothing()) {
        (TrustRoots::Bundled, _) => t!(
            doctor_trust_roots_bundled,
            variables = listed(&[CERTIFICATE_FILE, CERTIFICATE_DIRECTORY])
        )
        .to_string(),
        (_, true) => t!(doctor_trust_roots_none).to_string(),
        (named, false) => t!(
            doctor_trust_roots_named,
            paths = named
                .paths()
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )
        .to_string(),
    };

    let mut lines = vec![
        t!(doctor_network).to_string(),
        aligned(t!(doctor_trust_roots), roots, DETAIL),
    ];

    // A path that yielded nothing is named even where another one did, because the set in force is
    // then not the set that was asked for, and nothing else would say so.
    if let Some(problem) = transport.trust_problem() {
        lines.push(aligned(
            t!(doctor_trust_roots_unusable),
            problem.to_string(),
            DETAIL,
        ));
    }

    let proxy = match (transport.proxy_summary(), transport.unusable_proxy()) {
        (_, Some(protocol)) => t!(doctor_proxy_unsupported, protocol = protocol).to_string(),
        (None, None) => t!(doctor_proxy_absent, variables = listed(PROXY_VARIABLES)).to_string(),
        (Some(proxy), None) if transport.proxy_is_authenticated() => {
            t!(doctor_proxy_authenticated, proxy = proxy).to_string()
        }
        (Some(proxy), None) => t!(doctor_proxy_in_force, proxy = proxy).to_string(),
    };
    lines.push(aligned(t!(doctor_proxy), proxy, DETAIL));

    // Which hosts the proxy is not used for, since that is what decides whether a proxy in force
    // applies to the host that is failing, and `NO_PROXY=*` leaves one configured and used for
    // nothing.
    if let Some(excluded) = transport.no_proxy() {
        lines.push(aligned(t!(doctor_no_proxy), excluded, DETAIL));
    }

    lines
}

/// What `doctor` says about confinement: the lines, and whether there was any.
struct Confinement {
    lines: Vec<String>,
    established: bool,
}

/// Report the confinement actually achieved here, and say whether there was any.
///
/// Printed rather than assumed: the guarantee differs by platform and kernel, and a
/// user is entitled to know which one they have before trusting the sandbox.
///
/// Takes what the lookup found rather than calling it: a machine has only its own backend to
/// look up, so what is said about the other two levels is otherwise unreachable.
fn report_confinement(found: Result<Capabilities, SandboxError>) -> bool {
    let report = confinement(found);
    for line in &report.lines {
        // A refusal is not a finding among the others: it goes where a problem goes.
        match report.established {
            true => println!("{line}"),
            false => eprintln!("{line}"),
        }
    }
    report.established
}

/// The confinement section of `doctor`: the level in force, and what enforces it.
///
/// Built rather than printed, so what the section says about a level is a value a test can hold.
fn confinement(found: Result<Capabilities, SandboxError>) -> Confinement {
    match found {
        Ok(caps) => Confinement {
            lines: vec![
                t!(doctor_confinement, level = named(caps.level)).to_string(),
                aligned(t!(doctor_mechanisms), caps.mechanisms.join(", "), DETAIL),
                aligned(
                    t!(doctor_network_denial),
                    if caps.network_denial_enforced {
                        t!(doctor_kernel_enforced)
                    } else {
                        t!(doctor_not_enforced)
                    },
                    DETAIL,
                ),
            ],
            established: true,
        },
        Err(err) => Confinement {
            lines: vec![
                t!(doctor_confinement_unavailable).to_string(),
                format!("  {err}"),
            ],
            established: false,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bravebot_core::capability::Capability;
    use bravebot_core::event::Sink;
    use bravebot_core::label::Label;
    use bravebot_core::slot::SlotId;
    use bravebot_sandbox::policy::ConfinementLevel;
    use std::path::PathBuf;

    /// The two facts about the network nothing else in the report carries. A machine behind a
    /// TLS-inspecting proxy refuses every connection over an authority its user already installed,
    /// and a machine with a proxy variable set sends every request through somebody else's
    /// machine; neither is visible in any other line.
    #[test]
    fn the_network_section_names_the_roots_in_force_and_the_proxy() {
        let transport = Transport::stated(
            TrustRoots::Named {
                file: Some(PathBuf::from("/etc/corp/ca.pem")),
                directory: None,
            },
            Some("http://proxy.corp.example:8080"),
            None,
        );

        let report = network(&transport).join("\n");

        assert!(report.contains("/etc/corp/ca.pem"), "{report}");
        assert!(
            report.contains("http://proxy.corp.example:8080"),
            "{report}"
        );
    }

    /// A proxy that requires a credential carries it in the uri. A diagnostic that echoed one would
    /// put a live password in every issue somebody pastes this into, so the report says a credential
    /// is in use and never what it is.
    #[test]
    fn the_network_section_never_prints_a_proxy_credential() {
        let transport = Transport::stated(
            TrustRoots::Bundled,
            Some("http://alice:s3cret@proxy.corp.example:8080"),
            None,
        );

        let report = network(&transport).join("\n");

        assert!(!report.contains("s3cret"), "{report}");
        assert!(!report.contains("alice"), "{report}");
        assert!(
            report.contains("http://proxy.corp.example:8080"),
            "{report}"
        );
    }

    /// Nothing named means the built-in set, and the variables that would name another are the whole
    /// of the remedy: which variables a machine states an authority in is not something the reader
    /// is expected to know.
    #[test]
    fn the_network_section_points_at_the_variables_when_nothing_names_a_root_or_a_proxy() {
        let report = network(&Transport::stated(TrustRoots::Bundled, None, None)).join("\n");

        assert!(report.contains(CERTIFICATE_FILE), "{report}");
        assert!(report.contains(CERTIFICATE_DIRECTORY), "{report}");
        for variable in PROXY_VARIABLES {
            assert!(report.contains(variable), "{report}");
        }
    }

    /// A named path that yields no certificate leaves nothing trusted, so every connection this
    /// program makes is about to fail. Reported as a path in force it would read as working
    /// configuration, and the one command run to explain the failure would explain nothing.
    #[test]
    fn a_trust_root_that_cannot_be_read_is_reported_as_the_reason_connections_will_fail() {
        let transport = Transport::stated(
            TrustRoots::Named {
                file: Some(PathBuf::from("/etc/corp/absent.pem")),
                directory: None,
            },
            None,
            None,
        );

        let report = network(&transport).join("\n");

        assert!(transport.trusts_nothing());
        assert!(report.contains("/etc/corp/absent.pem"), "{report}");
        assert!(report.contains("fail"), "{report}");
    }

    /// A protocol this build cannot connect through is not the route requests take, so naming it as
    /// the proxy in force would send whoever ran this looking at a proxy that is seeing nothing.
    #[test]
    fn a_proxy_this_build_cannot_connect_through_is_reported_as_not_the_route() {
        let transport =
            Transport::stated(TrustRoots::Bundled, Some("socks5://proxy.corp:1080"), None);

        let report = network(&transport).join("\n");

        assert!(report.contains("socks5"), "{report}");
        assert!(report.contains("direct"), "{report}");
    }

    /// A remedy naming three variables is read, not parsed, so the list is punctuated the way a
    /// sentence is.
    #[test]
    fn a_list_of_variables_is_punctuated_as_a_sentence() {
        assert_eq!(listed(&[]), "");
        assert_eq!(listed(&["ONE"]), "ONE");
        assert_eq!(listed(&["ONE", "TWO"]), "ONE or TWO");
        assert_eq!(listed(&["ONE", "TWO", "THREE"]), "ONE, TWO or THREE");
    }

    /// Which hosts a proxy is not used for decides whether the one in force applies to the host
    /// that is failing, and `NO_PROXY=*` leaves a proxy configured and used for nothing.
    #[test]
    fn the_network_section_names_the_hosts_a_proxy_is_not_used_for() {
        let transport = Transport::stated(
            TrustRoots::Bundled,
            Some("http://proxy.corp.example:8080"),
            Some("localhost,.internal.example"),
        );

        let report = network(&transport).join("\n");

        assert!(report.contains("localhost,.internal.example"), "{report}");
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

    /// The column the English names were hand-spaced into, so extracting them changed nothing
    /// about what `doctor` prints.
    #[test]
    fn a_name_shorter_than_its_column_is_padded_out_to_it() {
        assert_eq!(
            aligned("endpoint", "https://example", FACT),
            "  endpoint  https://example"
        );
        assert_eq!(aligned("key id", "abc", FACT), "  key id    abc");
        assert_eq!(
            aligned("network denial", "kernel-enforced", DETAIL),
            "  network denial   kernel-enforced"
        );
    }

    /// A translation is not the length the English was, and a name that fills the column has to
    /// stay a name rather than running into what it is naming.
    #[test]
    fn a_name_longer_than_its_column_still_leaves_a_gap() {
        let line = aligned("point de terminaison", "https://example", FACT);
        assert_eq!(line, "  point de terminaison https://example");
    }

    /// The lines `doctor` prints for a set of capabilities, for the two tests below.
    fn confinement_report(level: ConfinementLevel, network_denial_enforced: bool) -> Vec<String> {
        confinement(Ok(Capabilities {
            level,
            mechanisms: vec!["a mechanism"],
            network_denial_enforced,
        }))
        .lines
    }

    /// The guarantee genuinely differs by platform and kernel, so which of the three is in force
    /// is what somebody runs `doctor` to learn before trusting the sandbox with untrusted work. A
    /// level it does not name is one they have to assume.
    #[test]
    fn doctor_names_the_confinement_level_in_force() {
        // The opening line rather than the report, because "kernel-enforced" is also what the
        // network denial line says, and a level reported into the wrong field is not reported.
        let opening = |level| {
            confinement_report(level, false)
                .first()
                .expect("the report opens with the level")
                .clone()
        };

        for level in [
            ConfinementLevel::Kernel,
            ConfinementLevel::Partial,
            ConfinementLevel::None,
        ] {
            let line = opening(level);
            assert!(
                line.contains(&named(level)),
                "the level in force is not in the line that reports it: {line}"
            );
        }

        // Three openings that read the same would satisfy the loop above while telling a reader
        // nothing about which of the three they have.
        assert_ne!(
            opening(ConfinementLevel::Kernel),
            opening(ConfinementLevel::Partial)
        );
        assert_ne!(
            opening(ConfinementLevel::Partial),
            opening(ConfinementLevel::None)
        );
        assert_ne!(
            opening(ConfinementLevel::Kernel),
            opening(ConfinementLevel::None)
        );
    }

    /// An overstated capability is the failure this report exists to prevent. A backend that
    /// leaves network denial to convention rather than to the kernel is the case somebody most
    /// needs told, since it is the one where the guarantee they assume is not the one they have.
    #[test]
    fn doctor_says_whether_the_kernel_enforces_network_denial() {
        // One level, so the difference can only be the answer to that question.
        assert_ne!(
            confinement_report(ConfinementLevel::Kernel, true).last(),
            confinement_report(ConfinementLevel::Kernel, false).last(),
            "the report reads the same whether or not the kernel enforces network denial"
        );
    }

    /// Without a backend, untrusted work is refused rather than run unconfined, so this is a thing
    /// the user has to go and fix: a `doctor` that reported it and still exited successfully would
    /// present it as one finding among the others, and one that did not say what was missing would
    /// leave them nothing to act on.
    #[test]
    fn confinement_that_could_not_be_established_fails_the_run() {
        let missing = || SandboxError::Unavailable {
            platform: "test",
            detail: "no backend is implemented here".into(),
        };

        assert!(
            !report_confinement(Err(missing())),
            "doctor reported no confinement and still passed"
        );

        let lines = confinement(Err(missing())).lines;
        assert!(
            lines
                .iter()
                .any(|line| line.contains(&missing().to_string())),
            "the refusal does not say what was missing: {lines:?}"
        );
    }

    /// A managed layer somebody cannot change has to say so somewhere, or "why is the variable I
    /// exported doing nothing" has no answer anywhere on the machine. The names and the file, since
    /// the file is what whoever can lift the pin has to be pointed at.
    #[test]
    fn doctor_names_what_the_managed_layer_pinned_and_the_file_it_came_from() {
        let scratch = Scratch::new("doctor-managed-pinned");
        let path = scratch.path.join("managed.json");
        std::fs::write(
            &path,
            r#"{"env": {"BRAVE_AI_CHAT_ENDPOINT": "https://approved.example"}}"#,
        )
        .expect("a managed file");

        let lines = managed_layer(&Managed::at(&path));

        assert_eq!(lines.len(), 1, "one line, or the report grew: {lines:?}");
        assert!(
            lines[0].contains("BRAVE_AI_CHAT_ENDPOINT"),
            "the pinned name is not reported: {lines:?}"
        );
        assert!(
            lines[0].contains(&path.display().to_string()),
            "the file that pinned it is not named: {lines:?}"
        );
        assert!(
            !lines[0].contains("https://approved.example"),
            "the value is printed, and on some machines that is a credential: {lines:?}"
        );
    }

    /// The case every machine without an administrator is in. A report listing every place a file
    /// could have been is a report where the lines that matter are the hard ones to find.
    #[test]
    fn doctor_says_nothing_about_a_managed_layer_that_is_not_there() {
        let scratch = Scratch::new("doctor-managed-absent");
        let absent = scratch.path.join("managed.json");
        assert!(managed_layer(&Managed::at(&absent)).is_empty());
    }

    /// A file holding only names this layer may not pin did nothing, and saying nothing about it
    /// would leave whoever wrote it unable to tell that from a file this program never found.
    #[test]
    fn doctor_names_a_managed_file_that_pinned_nothing() {
        let scratch = Scratch::new("doctor-managed-nothing");
        let path = scratch.path.join("managed.json");
        std::fs::write(&path, r#"{"env": {"BRAVEBOT_CONTEXT_BUDGET": "4096"}}"#)
            .expect("a managed file");

        let lines = managed_layer(&Managed::at(&path));

        assert_eq!(lines.len(), 1, "the file is not reported: {lines:?}");
        assert!(
            lines[0].contains(
                &t!(doctor_managed_nothing, path = path.display().to_string()).to_string()
            ),
            "the line does not say the file pinned nothing: {lines:?}"
        );
    }

    /// Which directory this is depends on the environment of whoever started the process, so a
    /// person under `sudo`, or running the same binary from a service manager, has a different one
    /// from the session they are looking for. Naming it is what settles which of them is in force.
    ///
    /// The variable it came from is named with it, because more than one can name a profile
    /// directory and only the one that answered is worth changing: told the path alone, somebody
    /// moving the directory on Windows sets `USERPROFILE` and watches a `HOME` they forgot they had
    /// go on winning.
    #[test]
    fn doctor_names_the_state_directory_it_resolved() {
        let lines = state_directory(
            Some(("HOME", Path::new("/home/someone/.bravebot"))),
            &["HOME"],
            true,
        );

        assert!(
            lines
                .iter()
                .any(|line| line.contains("/home/someone/.bravebot")),
            "the state directory in use is not in the section that reports it: {lines:?}"
        );
        assert!(
            lines.iter().any(|line| line.contains("HOME")),
            "the section does not say which variable named the directory: {lines:?}"
        );
        assert!(
            !lines
                .iter()
                .any(|line| line.contains(&t!(doctor_state_directory_forgotten).to_string())),
            "a machine with a state directory was told what it is not keeping: {lines:?}"
        );
    }

    /// The mode `home` asks for as it creates is what keeps a prompt history out of another
    /// account's reach, and on a platform that is given no mode the same files are written carrying
    /// whatever the profile directory grants. Somebody typing a token into a prompt on a shared or
    /// synced profile is entitled to know which of the two they have, and no other line in the
    /// report distinguishes them: the section reads identically either way.
    #[test]
    fn doctor_says_when_the_files_are_left_unrestricted() {
        let path = Path::new("C:\\Users\\someone\\.bravebot");
        let named = ["HOME", "USERPROFILE"];
        let unrestricted = state_directory(Some(("USERPROFILE", path)), &named, false).join("\n");

        assert!(
            unrestricted.contains(&t!(doctor_state_directory_permissions).to_string()),
            "a platform that narrows nothing said nothing about it: {unrestricted}"
        );
        assert!(
            !state_directory(Some(("USERPROFILE", path)), &named, true)
                .join("\n")
                .contains(&t!(doctor_state_directory_permissions).to_string()),
            "a platform that narrows every file reported them as unrestricted"
        );
    }

    /// The loss this reports is silent: the session records behind `--resume`, the prompt history
    /// and the recorded model are neither read nor written, every subsystem treats that as absence
    /// by design, and somebody whose `/model` choice does not survive the session has nothing else
    /// in the report to explain it. A stripped environment, a service manager and a container all
    /// reach it.
    ///
    /// What is still read has to be said in the same breath, because the absence is partial: a
    /// checkout's own settings, skills and `AGENTS.md` load with no home at all, and a report that
    /// left that out would send somebody looking for why a file that is in force is ignored.
    #[test]
    fn a_missing_state_directory_is_reported_with_what_it_costs() {
        let variables = bravebot_agent::home::PROFILE_VARIABLES;
        let lines = state_directory(None, variables, RESTRICTED);
        let section = lines.join("\n");

        for lost in ["sessions", "--resume", "prompt history", "model"] {
            assert!(
                section.contains(lost),
                "the section does not say that {lost} is not kept: {section}"
            );
        }
        assert!(
            section.contains("checkout"),
            "the section does not say that a checkout's own files are still read: {section}"
        );
        assert_ne!(
            lines,
            state_directory(
                Some(("HOME", Path::new("/home/someone/.bravebot"))),
                variables,
                RESTRICTED
            ),
            "a machine with no state directory reads the same as one with a state directory"
        );
    }

    /// Which variables a platform states a profile directory in is not something the reader knows,
    /// so the report names every one that was looked at: told only that `HOME` names nothing,
    /// somebody on a platform that answers with another variable is being sent to set the one that
    /// was never going to be consulted. The remedy carries the same list as the line above it, since
    /// the remedy is the half somebody acts on.
    ///
    /// Held against two variables rather than this host's own, which states one. Against a list of
    /// one, an assertion that the report names `HOME` is satisfied by the remedy's own wording
    /// whatever the report does with the list, which is a test that passes having pinned nothing.
    #[test]
    fn a_missing_state_directory_names_every_variable_it_looked_at() {
        let lines = state_directory(None, &["HOME", "USERPROFILE"], RESTRICTED);
        let absent = lines.first().expect("the absence is reported");
        let remedy = lines
            .iter()
            .find(|line| line.contains(&t!(doctor_state_directory_remedy).to_string()))
            .expect("the report says how to keep them");

        for line in [absent, remedy] {
            assert!(
                line.contains("HOME or USERPROFILE"),
                "a line of the report names fewer than the two variables looked at: {line}"
            );
        }
    }

    /// The gateway a settings file configured, for the `doctor` tests below.
    fn configured_gateway(text: &str) -> bravebot_config::provider::Provider {
        bravebot_config::Settings::parse(text)
            .providers()
            .first()
            .expect("one provider")
            .clone()
    }

    /// A gateway's credential is a long-lived bearer token, so a diagnostic that echoed one would put
    /// a live credential in every issue somebody pastes this into.
    #[test]
    fn a_gateway_credential_is_reported_as_found_and_never_printed() {
        let provider = configured_gateway(
            r#"{"provider": {"gw": {
                "env": ["A_TOKEN_VARIABLE"],
                "options": {"baseURL": "https://example.invalid/v1", "apiKey": "in-the-file"}
            }}}"#,
        );

        let from_variable = gateway_credential(&provider, |name| match name {
            "A_TOKEN_VARIABLE" => Some("secret-from-the-environment".to_string()),
            _ => None,
        });
        assert!(!from_variable.contains("secret-from-the-environment"));

        // A token written into the file is found too, and is withheld on the same footing.
        let from_file = gateway_credential(&provider, |_| None);
        assert!(!from_file.contains("in-the-file"));
        assert_eq!(from_variable, from_file);
    }

    /// A gateway nothing holds a credential for says so. Reporting one as found would answer the
    /// question `doctor` exists to answer wrongly, and the request then fails somewhere further away.
    #[test]
    fn a_gateway_with_no_credential_is_reported_as_having_none() {
        let provider = configured_gateway(
            r#"{"provider": {"gw": {
                "env": ["ABSENT_ONE"],
                "options": {"baseURL": "https://example.invalid/v1"}
            }}}"#,
        );

        assert_ne!(
            gateway_credential(&provider, |_| None),
            gateway_credential(&provider, |_| Some("anything".to_string()))
        );
    }

    /// A gateway whose block names no credential needs none, and saying "none found" of it reads as
    /// something to go and set on a gateway that is working. Distinct from the case above, which names
    /// a variable and really does want a token in it.
    #[test]
    fn a_gateway_needing_no_credential_is_reported_as_needing_none() {
        let needs_none = configured_gateway(
            r#"{"provider": {"ollama": {
                "options": {"baseURL": "http://localhost:11434/v1"}
            }}}"#,
        );
        let names_one = configured_gateway(
            r#"{"provider": {"gw": {
                "env": ["ABSENT_ONE"],
                "options": {"baseURL": "https://example.invalid/v1"}
            }}}"#,
        );

        assert_eq!(
            gateway_credential(&needs_none, |_| None),
            t!(doctor_gateway_token_not_needed)
        );
        assert_eq!(
            gateway_credential(&names_one, |_| None),
            t!(doctor_gateway_token_absent)
        );
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

    /// Reads back what a run would have written, as the two streams it writes to.
    fn written(run: &Finished<'_>) -> (String, String) {
        let mut reply = Vec::new();
        let mut beside = Vec::new();
        report(&mut reply, &mut beside, run);
        (
            String::from_utf8(reply).expect("utf-8"),
            String::from_utf8(beside).expect("utf-8"),
        )
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
            not_served: None,
        });

        assert_eq!(reply, "ok\n");
        assert!(beside.contains("audit trail"), "got: {beside}");
        assert!(beside.contains("note: a skill was loaded"), "got: {beside}");
        assert!(beside.contains("model: qwen-3-235b"), "got: {beside}");
        assert!(beside.contains("a policy gate refused"), "got: {beside}");
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

    fn development_fixture(name: &str) -> Scratch {
        let scratch = Scratch::new(name);
        for marker in [
            "Cargo.toml",
            "crates/cli/Cargo.toml",
            "agents/setup.py",
            "agents/AGENTS.md",
            "docs/development/agent-configuration.md",
        ] {
            let file = scratch.path.join(marker);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(file, "source instructions").unwrap();
        }
        scratch
    }

    /// A user's workspace needs neither the repository's links nor its build tools.
    #[test]
    fn doctor_development_checks_only_apply_to_the_source_tree() {
        let scratch = Scratch::new("doctor-ordinary-workspace");
        scratch.directory(".git");
        std::fs::write(scratch.path.join("AGENTS.md"), "user instructions").unwrap();
        assert!(development(&scratch.path, None, false).is_empty());
        std::fs::remove_dir(scratch.path.join(".git")).unwrap();
        std::fs::write(scratch.path.join(".git"), "gitdir: elsewhere").unwrap();
        assert!(development(&scratch.path, None, false).is_empty());
        let source = development_fixture("doctor-source-tree");
        std::fs::write(source.path.join(".git"), "gitdir: elsewhere").unwrap();
        for windows in [false, true] {
            assert!(agent_discovery(&source.path, windows).starts_with("missing;"));
        }
        let nested = source.directory("crates/cli/src");
        let report = development(&nested, None, false).join("\n");
        assert!(report.contains("development environment"));
        assert!(report.contains("AGENTS.md"));
        assert!(report.contains("python3 agents/setup.py link"));
        assert!(report.contains("direnv"));
        assert!(report.contains("https://direnv.net/"));
        assert!(report.contains("brew install direnv"));
        assert!(!source.path.join("AGENTS.md").exists());
    }

    /// A hand-written path must survive a diagnostic and must not look healthy.
    #[test]
    fn doctor_reports_agent_discovery_conflicts_without_changing_them() {
        let scratch = development_fixture("doctor-conflicts");
        let destination = scratch.path.join("AGENTS.md");
        std::fs::write(&destination, "personal instructions").unwrap();
        let report = agent_discovery(&scratch.path, false);
        assert!(report.contains("conflict"));
        assert!(report.contains("resolve the existing file or directory first"));
        assert_eq!(
            std::fs::read_to_string(&destination).unwrap(),
            "personal instructions"
        );
        std::fs::remove_file(&destination).unwrap();
        std::fs::create_dir(&destination).unwrap();
        for windows in [false, true] {
            assert!(agent_discovery(&scratch.path, windows).contains("conflict"));
            assert!(destination.is_dir());
        }
    }

    /// Windows setup copies the source, so a current copy is healthy and a stale one is repairable.
    #[test]
    fn doctor_accepts_current_windows_copies_and_reports_stale_ones() {
        let scratch = development_fixture("doctor-windows-copy");
        let destination = scratch.path.join("AGENTS.md");
        std::fs::copy(scratch.path.join("agents/AGENTS.md"), &destination).unwrap();
        assert_eq!(
            agent_discovery(&scratch.path, true),
            "OK (Windows copy of agents/AGENTS.md)"
        );
        std::fs::write(&destination, "old instructions").unwrap();
        let report = agent_discovery(&scratch.path, true);
        assert!(report.contains("stale"));
        assert!(report.contains("python3 agents/setup.py link"));
        assert_eq!(
            std::fs::read_to_string(destination).unwrap(),
            "old instructions"
        );
    }

    /// Existence alone does not mean an agent will read this repository's instructions.
    #[cfg(unix)]
    #[test]
    fn doctor_checks_resolved_agent_link_targets() {
        use std::os::unix::fs::symlink;
        let scratch = development_fixture("doctor-links");
        let destination = scratch.path.join("AGENTS.md");
        for target in [
            std::path::PathBuf::from("agents/AGENTS.md"),
            std::path::PathBuf::from("agents/../agents/AGENTS.md"),
            scratch.path.join("agents/AGENTS.md"),
        ] {
            symlink(target, &destination).unwrap();
            assert_eq!(
                agent_discovery(&scratch.path, false),
                "OK (link to agents/AGENTS.md)"
            );
            std::fs::remove_file(&destination).unwrap();
        }
        std::fs::write(scratch.path.join("other.md"), "source instructions").unwrap();
        symlink("other.md", &destination).unwrap();
        let report = agent_discovery(&scratch.path, false);
        assert!(report.contains("wrong target"), "{report}");
        assert!(report.contains("python3 agents/setup.py link"));
        assert_eq!(
            std::fs::read_link(&destination).unwrap(),
            Path::new("other.md")
        );
        std::fs::remove_file(scratch.path.join("other.md")).unwrap();
        let report = agent_discovery(&scratch.path, false);
        assert!(report.contains("broken"));
        assert!(report.contains("python3 agents/setup.py link"));
        assert!(destination.is_symlink());
    }

    /// PATH is supplied by the fixture, so an installed host tool cannot hide a missing-tool bug.
    #[test]
    fn doctor_finds_direnv_only_when_path_contains_an_executable() {
        let scratch = development_fixture("doctor-direnv");
        let bin = scratch.directory("bin");
        let path = std::env::join_paths([&bin]).unwrap();
        assert!(!direnv_available(None));
        assert!(!direnv_available(Some(&path)));
        let executable = bin.join(if cfg!(windows) {
            "direnv.exe"
        } else {
            "direnv"
        });
        std::fs::create_dir(&executable).unwrap();
        assert!(!direnv_available(Some(&path)));
        std::fs::remove_dir(&executable).unwrap();
        std::fs::write(&executable, "not executed").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o644)).unwrap();
            assert!(!direnv_available(Some(&path)));
            std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        assert!(direnv_available(Some(&path)));
        let report = development(&scratch.path, Some(&path), false).join("\n");
        assert!(report.contains("available on PATH"));
        assert!(!report.contains("brew install"));
    }

    /// A scratch directory that removes itself, so tests do not leave state behind.
    ///
    /// Under this crate's own build directory rather than the system temporary one, which is
    /// shared between users and where a name this predictable is somebody else's to create first.
    struct Scratch {
        path: PathBuf,
    }

    impl Scratch {
        fn new(name: &str) -> Self {
            let path = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../target/test-scratch")
                .join(name);
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).expect("create scratch");
            Self {
                path: path.canonicalize().expect("canonical scratch"),
            }
        }

        /// A directory inside the scratch, made ready to be used.
        fn directory(&self, name: &str) -> PathBuf {
            let path = self.path.join(name);
            std::fs::create_dir_all(&path).expect("create directory");
            path
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
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
                take_incognito(&mut arguments),
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
        assert!(take_incognito(&mut arguments));
        assert_eq!(arguments, args(&["do a thing"]));
    }

    /// The flag is the whole invocation often enough to be worth pinning: what is left is nothing,
    /// which the dispatch reads as the interactive session, and that is the intent.
    #[test]
    fn incognito_alone_leaves_the_interactive_session() {
        let mut arguments = args(&["--incognito"]);
        assert!(take_incognito(&mut arguments));
        assert!(arguments.is_empty());
    }

    /// An ordinary invocation is untouched, and stays not incognito. A mode that turned itself on
    /// for something that merely mentioned it would be worse than one that never worked.
    #[test]
    fn an_invocation_without_the_flag_is_left_alone() {
        let mut arguments = args(&["-p", "write about incognito mode"]);
        assert!(!take_incognito(&mut arguments));
        assert_eq!(arguments, args(&["-p", "write about incognito mode"]));
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
            path: "notes.md".to_string(),
            contents: "text".to_string(),
            existing: None,
            intent: bravebot_agent::Intent::Create,
            untrusted: false,
            remark: None,
        };
        let vouch = VouchRequest {
            path: "notes.md".to_string(),
            preview: "text".to_string(),
            truncated: false,
            verdict: bravebot_core::vetting::Verdict::Safe,
            reason: None,
        };

        assert_eq!(one_shot.confirm_write(&write), Decision::Reject);
        assert_eq!(one_shot.confirm_vouch(&vouch), Decision::Reject);
        assert!(one_shot.interjection().is_none());
        drop(one_shot);

        assert!(shown.is_empty(), "something was asked about: {shown:?}");
    }

    /// Absent is the state every run is in unless somebody typed the flag, and it is the only state
    /// in which a write is put to a person first.
    #[test]
    fn permissions_are_enforced_unless_the_flag_is_given() {
        let mut arguments = args(&["do a thing"]);
        assert!(!take_skip_permissions(&mut arguments));
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
                take_skip_permissions(&mut arguments),
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
        assert!(take_skip_permissions(&mut arguments));
        assert_eq!(arguments, args(&["--resume", "1787860306-65099"]));

        // Nothing but the flag is an interactive session, not an unknown option.
        let mut alone = args(&["--dangerously-skip-permissions"]);
        assert!(take_skip_permissions(&mut alone));
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
            assert!(take_incognito(&mut arguments), "{typed:?}");
            assert!(take_skip_permissions(&mut arguments), "{typed:?}");
            assert_eq!(arguments, args(&["-p", "x"]), "left over: {typed:?}");
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
    #[test]
    fn a_named_settings_file_composes_with_the_other_flags_before_dispatch() {
        for typed in [
            &[
                "--incognito",
                "--settings",
                "/etc/ci.json",
                "--dangerously-skip-permissions",
                "-p",
                "x",
            ][..],
            &[
                "--settings",
                "/etc/ci.json",
                "--dangerously-skip-permissions",
                "--incognito",
                "-p",
                "x",
            ][..],
        ] {
            let mut arguments = args(typed);
            assert!(take_incognito(&mut arguments), "{typed:?}");
            assert!(take_skip_permissions(&mut arguments), "{typed:?}");
            assert_eq!(
                take_settings(&mut arguments).expect("names a file"),
                Some(PathBuf::from("/etc/ci.json")),
                "{typed:?}"
            );
            assert_eq!(arguments, args(&["-p", "x"]), "left over: {typed:?}");
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
            not_served: Some("a-premium-model was not served"),
        });

        assert_eq!(reply, "ok\n");
        assert!(
            beside.contains("a-premium-model was not served"),
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
            not_served: Some("a-premium-model was not served"),
        };

        let (reply, beside) = written(&substituted);
        assert_eq!(reply, "ok\n");
        assert!(beside.contains("was not served"), "{beside}");
        assert!(ending_of_a_turn(true, false, true).ok());
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
            not_served: None,
        });
        assert_eq!(reply, "ok\n");
        assert!(beside.contains("not usable"), "got: {beside}");
        assert!(!reply.contains("not usable"));
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

        let (permissions, rejected) = rules_for_a_one_shot_run(&settings, None, false);
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
            permissions.for_command("git push origin main"),
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

        let (permissions, _) = rules_for_a_one_shot_run(&settings, None, true);
        assert_eq!(
            permissions.for_path(Subject::Edit, "notes.md"),
            Decision::Ruled(Ruling::Allow)
        );
    }
}
