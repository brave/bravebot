//! Tests for running a hook, against real processes in a real directory.
//!
//! What a declaration means is `bravebot-config`'s. This is about what happens once one fires:
//! whether the program runs, what it is handed, where it runs, and what a turn is told when it
//! goes wrong.

#![cfg(unix)]

use bravebot_agent::hooks::{self, Fired, Trouble};
use bravebot_config::hooks::{Hooks, Moment};
use std::path::{Path, PathBuf};
use std::time::Duration;

struct Scratch {
    path: PathBuf,
}

impl Scratch {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("bravebot-hooks-{name}"));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("create scratch");
        Self { path }
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// Held while a program is written, and again while one is started.
///
/// A test that writes a program races every other test in this binary: between a sibling thread's
/// fork and its exec the child holds a copy of every descriptor this process had open, including
/// the one the script was written through, and `execve` answers `ETXTBSY` for a file anyone holds
/// open for writing. Writing under this and forking under this means no fork ever happens while a
/// write descriptor is open, so no child ever inherits one. Waiting the race out instead would mean
/// running a moment's hooks a second time, which a test counting what they did cannot survive.
static STARTING: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// A test that panicked while holding it left nothing behind to protect: what this guards is a
/// descriptor's lifetime, not state.
fn starting() -> std::sync::MutexGuard<'static, ()> {
    STARTING.lock().unwrap_or_else(|held| held.into_inner())
}

/// Write an executable script and answer with its path.
fn script(at: &Path, name: &str, body: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = at.join(name);
    let held = starting();
    std::fs::write(&path, body).expect("write the script");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
        .expect("make it executable");
    drop(held);
    path.canonicalize().expect("canonicalize")
}

/// A hooks file declaring one entry that runs `run`.
fn declaring(moment: &str, run: &[&str]) -> Hooks {
    let arguments: Vec<String> = run.iter().map(|word| format!("{word:?}")).collect();
    Hooks::parse(&format!(
        r#"{{"hooks": [{{"on": "{moment}", "run": [{}]}}]}}"#,
        arguments.join(", ")
    ))
}

/// Fire the hooks, under the gate [`STARTING`] describes.
fn fire(hooks: &Hooks, moment: Moment, tool: Option<&str>, at: &Path) -> Vec<Fired> {
    fire_within(hooks, moment, tool, at, Duration::from_secs(30))
}

fn fire_within(
    hooks: &Hooks,
    moment: Moment,
    tool: Option<&str>,
    at: &Path,
    limit: Duration,
) -> Vec<Fired> {
    let _held = starting();
    hooks::fire_within(hooks, moment, tool, at, limit)
}

/// HOOK-4: a hook attached to a moment runs when the moment comes.
#[test]
fn a_hook_runs_the_program_the_file_named() {
    let scratch = Scratch::new("runs");
    let program = script(&scratch.path, "touch-it", "#!/bin/sh\ntouch it-ran\n");
    let hooks = declaring("turn-finished", &[program.to_str().expect("a path")]);

    let fired = fire(&hooks, Moment::TurnFinished, None, &scratch.path);

    assert_eq!(fired.len(), 1);
    assert_eq!(
        fired[0].trouble, None,
        "a hook that ended well has no trouble"
    );
    assert!(scratch.path.join("it-ran").exists(), "the hook did not run");
}

/// HOOK-5: the whole of what a hook is told is which moment fired.
#[test]
fn a_hook_is_told_the_moment_and_nothing_else() {
    let scratch = Scratch::new("told");
    let program = script(&scratch.path, "keep-it", "#!/bin/sh\ncat > told.json\n");
    let hooks = declaring("tool-finished", &[program.to_str().expect("a path")]);

    fire(
        &hooks,
        Moment::ToolFinished,
        Some("write_file"),
        &scratch.path,
    );

    let told = std::fs::read_to_string(scratch.path.join("told.json")).expect("the hook was told");
    assert_eq!(told.trim(), r#"{"event":"tool-finished"}"#);
}

/// HOOK-5: the moment and the tool an entry names reach the hook on its standard input only. Its
/// environment is this process's own, less what is scrubbed, with no name added and no value
/// carrying the tool or the moment.
#[test]
fn a_hook_is_given_no_environment_of_its_own() {
    let scratch = Scratch::new("environment");
    // `exec env` so the shell that reads the script is replaced before it can export anything.
    let program = script(
        &scratch.path,
        "dump-env",
        "#!/bin/sh\nexec env > seen.env\n",
    );
    let hooks = declaring("tool-finished", &[program.to_str().expect("a path")]);

    fire(
        &hooks,
        Moment::ToolFinished,
        Some("write_file"),
        &scratch.path,
    );

    let seen = std::fs::read_to_string(scratch.path.join("seen.env")).expect("the hook ran");
    let ours: std::collections::HashSet<String> = std::env::vars_os()
        .map(|(name, _)| name.to_string_lossy().into_owned())
        .collect();
    // Set by the shell itself before `exec` replaces it, so not something the agent added.
    let by_the_shell = ["PWD", "OLDPWD", "SHLVL", "_"];
    for line in seen.lines() {
        let Some((name, value)) = line.split_once('=') else {
            continue;
        };
        assert!(
            ours.contains(name) || by_the_shell.contains(&name),
            "the hook was given {name}, which this process did not have"
        );
        assert!(
            !value.contains("write_file") && !value.contains("tool-finished"),
            "{name} carries what the hook should only have been told on standard input"
        );
    }
    assert!(!seen.is_empty(), "the hook saw no environment at all");
}

/// HOOK-3: the words in the file are an argument vector. One holding a space and a semicolon is
/// one argument, because there is no shell between the file and the process.
#[test]
fn an_argument_holding_shell_syntax_arrives_as_one_argument() {
    let scratch = Scratch::new("argv");
    let program = script(
        &scratch.path,
        "keep-arg",
        "#!/bin/sh\nprintf '%s' \"$1\" > arg.txt\n",
    );
    let hooks = declaring(
        "turn-started",
        &[program.to_str().expect("a path"), "one two; touch pwned"],
    );

    fire(&hooks, Moment::TurnStarted, None, &scratch.path);

    let argument = std::fs::read_to_string(scratch.path.join("arg.txt")).expect("the argument");
    assert_eq!(argument, "one two; touch pwned");
    assert!(
        !scratch.path.join("pwned").exists(),
        "the argument was parsed by a shell"
    );
}

/// HOOK-4: a hook runs where the work is, so a relative path in one means what it means to the
/// person who wrote it.
#[test]
fn a_hook_runs_in_the_directory_the_turn_is_working_in() {
    let scratch = Scratch::new("directory");
    let program = script(&scratch.path, "say-where", "#!/bin/sh\npwd > where.txt\n");
    let hooks = declaring("turn-started", &[program.to_str().expect("a path")]);

    fire(&hooks, Moment::TurnStarted, None, &scratch.path);

    let where_it_ran =
        std::fs::read_to_string(scratch.path.join("where.txt")).expect("a directory");
    assert_eq!(
        PathBuf::from(where_it_ran.trim())
            .canonicalize()
            .expect("canonicalize"),
        scratch.path.canonicalize().expect("canonicalize")
    );
}

/// HOOK-7: a hook that ends badly is reported, and that is the whole of what its status is used
/// for.
#[test]
fn a_hook_that_ends_badly_is_reported() {
    let scratch = Scratch::new("badly");
    let program = script(&scratch.path, "fail", "#!/bin/sh\nexit 3\n");
    let hooks = declaring("turn-finished", &[program.to_str().expect("a path")]);

    let fired = fire(&hooks, Moment::TurnFinished, None, &scratch.path);

    assert!(
        matches!(&fired[0].trouble, Some(Trouble::Ended(_))),
        "a non-zero exit is trouble worth saying: {:?}",
        fired[0].trouble
    );
    assert_eq!(fired[0].moment, "turn-finished");
}

/// HOOK-7: a hook whose program is not there is a sentence to whoever wrote the file, not an
/// error that ends anything.
#[test]
fn a_hook_whose_program_is_not_there_is_reported() {
    let scratch = Scratch::new("absent");
    let hooks = declaring(
        "turn-started",
        &[scratch.path.join("not-installed").to_str().expect("a path")],
    );

    let fired = fire(&hooks, Moment::TurnStarted, None, &scratch.path);

    assert!(
        matches!(&fired[0].trouble, Some(Trouble::NotStarted(_))),
        "a program that is not there should say so: {:?}",
        fired[0].trouble
    );
}

/// HOOK-7: a hook holds the turn open while it runs, so one that does not finish is stopped
/// rather than waited on.
#[test]
fn a_hook_that_outstays_the_bound_is_stopped() {
    let scratch = Scratch::new("slow");
    let program = script(&scratch.path, "linger", "#!/bin/sh\nsleep 30\n");
    let hooks = declaring("turn-finished", &[program.to_str().expect("a path")]);

    let began = std::time::Instant::now();
    let fired = fire_within(
        &hooks,
        Moment::TurnFinished,
        None,
        &scratch.path,
        Duration::from_millis(200),
    );

    assert_eq!(
        fired[0].trouble,
        Some(Trouble::Stopped(Duration::from_millis(200)))
    );
    assert!(
        began.elapsed() < Duration::from_secs(20),
        "the bound was not applied"
    );
}

/// A hooks file declaring one entry that runs `run` with a `timeout`.
fn declaring_with_timeout(moment: &str, run: &[&str], timeout: u64) -> Hooks {
    let arguments: Vec<String> = run.iter().map(|word| format!("{word:?}")).collect();
    Hooks::parse(&format!(
        r#"{{"hooks": [{{"on": "{moment}", "run": [{}], "timeout": {timeout}}}]}}"#,
        arguments.join(", ")
    ))
}

/// HOOK-7: a hook that states a `timeout` is stopped at that bound and not at the default, and the
/// stop says the bound it was held to.
#[test]
fn a_hook_is_stopped_at_the_timeout_it_declared() {
    let scratch = Scratch::new("own-timeout-short");
    let program = script(&scratch.path, "linger", "#!/bin/sh\nsleep 30\n");
    let hooks = declaring_with_timeout("turn-finished", &[program.to_str().expect("a path")], 1);

    let began = std::time::Instant::now();
    let fired = fire(&hooks, Moment::TurnFinished, None, &scratch.path);

    assert_eq!(
        fired[0].trouble,
        Some(Trouble::Stopped(Duration::from_secs(1)))
    );
    assert!(
        began.elapsed() < Duration::from_secs(15),
        "the default bound was applied in place of the declared one"
    );
}

/// HOOK-7: a hook that states a longer `timeout` than the default is not stopped at the default.
#[test]
fn a_hook_may_outlive_the_default_bound_by_declaring_more() {
    let scratch = Scratch::new("own-timeout-long");
    let program = script(
        &scratch.path,
        "pause",
        "#!/bin/sh\nsleep 1\ntouch finished\n",
    );
    let hooks = declaring_with_timeout("turn-finished", &[program.to_str().expect("a path")], 30);

    // The default is shorter than the hook runs; only the declared bound lets it finish.
    let fired = fire_within(
        &hooks,
        Moment::TurnFinished,
        None,
        &scratch.path,
        Duration::from_millis(200),
    );

    assert_eq!(fired[0].trouble, None);
    assert!(scratch.path.join("finished").exists());
}

/// HOOK-4: two entries on one moment are two commands, run in the order the file listed them.
#[test]
fn two_hooks_on_one_moment_run_in_the_order_the_file_listed_them() {
    let scratch = Scratch::new("order");
    let first = script(
        &scratch.path,
        "first",
        "#!/bin/sh\necho first >> order.txt\n",
    );
    let second = script(
        &scratch.path,
        "second",
        "#!/bin/sh\necho second >> order.txt\n",
    );
    let hooks = Hooks::parse(&format!(
        r#"{{"hooks": [
            {{"on": "turn-started", "run": [{:?}]}},
            {{"on": "turn-started", "run": [{:?}]}}
        ]}}"#,
        first.to_str().expect("a path"),
        second.to_str().expect("a path")
    ));

    fire(&hooks, Moment::TurnStarted, None, &scratch.path);

    let order = std::fs::read_to_string(scratch.path.join("order.txt")).expect("both ran");
    assert_eq!(order, "first\nsecond\n");
}

/// HOOK-4: this agent's own credentials do not travel into a hook, any more than they travel into
/// a program the planner asked for.
///
/// The variable is set here rather than assumed, because a machine that never exported one would
/// make this pass without the scrubbing being in force at all.
#[test]
fn a_hook_is_not_handed_this_agent_s_own_credentials() {
    let scratch = Scratch::new("scrubbed");
    let program = script(
        &scratch.path,
        "keep-env",
        "#!/bin/sh\nprintf '%s' \"$SERVICES_KEY_AICHAT\" > key.txt\n",
    );
    let hooks = declaring("turn-started", &[program.to_str().expect("a path")]);

    // SAFETY: nothing else in this binary reads or writes this name, and the value is put back
    // before the test returns.
    let restore = std::env::var_os("SERVICES_KEY_AICHAT");
    unsafe { std::env::set_var("SERVICES_KEY_AICHAT", "a-secret") };

    fire(&hooks, Moment::TurnStarted, None, &scratch.path);

    // SAFETY: as above.
    match restore {
        Some(value) => unsafe { std::env::set_var("SERVICES_KEY_AICHAT", value) },
        None => unsafe { std::env::remove_var("SERVICES_KEY_AICHAT") },
    }

    let key = std::fs::read_to_string(scratch.path.join("key.txt")).expect("what it saw");
    assert!(key.is_empty(), "a credential reached a hook: {key}");
}

/// RUN-12: the variables a provider block names are withheld from a hook as well, and the hook
/// still runs in the environment it had otherwise.
///
/// The block names a variable no built-in list holds. `HOME` is pointed at a settings file that
/// declares it, which is where the program under test reads providers from.
#[test]
fn a_hook_is_not_handed_a_gateways_environment_token() {
    let scratch = Scratch::new("scrubbed-gateway");
    let home = scratch.path.join("home");
    std::fs::create_dir_all(home.join(".bravebot")).expect("create the home");
    std::fs::write(
        home.join(".bravebot").join("settings.json"),
        r#"{"provider": {"acme": {
            "options": {"baseURL": "https://gateway.acme.invalid/v1"},
            "env": ["ACME_GATEWAY_TOKEN"]
        }}}"#,
    )
    .expect("write the settings");
    let program = script(
        &scratch.path,
        "keep-env",
        "#!/bin/sh\nenv > environment.txt\n",
    );
    let hooks = declaring("turn-started", &[program.to_str().expect("a path")]);

    static ENVIRONMENT: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _held = ENVIRONMENT.lock().unwrap_or_else(|held| held.into_inner());
    let previous_home = std::env::var_os("HOME");
    // SAFETY: ENVIRONMENT is held, nothing else in this binary reads these names, and both
    // variables are put back before any assertion.
    unsafe {
        std::env::set_var("HOME", &home);
        std::env::set_var("ACME_GATEWAY_TOKEN", "a-live-gateway-token");
        std::env::remove_var("BRAVEBOT_SUBPROCESS_ENV_SCRUB");
    }

    fire(&hooks, Moment::TurnStarted, None, &scratch.path);

    // SAFETY: as above.
    unsafe {
        std::env::remove_var("ACME_GATEWAY_TOKEN");
        match previous_home {
            Some(value) => std::env::set_var("HOME", value),
            None => std::env::remove_var("HOME"),
        }
    }

    let environment =
        std::fs::read_to_string(scratch.path.join("environment.txt")).expect("what it saw");
    assert!(
        environment.contains("PATH="),
        "the hook saw no environment at all: {environment}"
    );
    assert!(
        !environment.contains("a-live-gateway-token"),
        "a gateway's token reached a hook"
    );
}
