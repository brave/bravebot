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
        let path =
            std::env::temp_dir().join(format!("bravebot-hooks-{}-{name}", std::process::id()));
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

/// Fire the hooks for a new checkout, under the gate [`STARTING`] describes.
fn fire_checkout_made(hooks: &Hooks, made: hooks::Made<'_>) -> Vec<Fired> {
    let _held = starting();
    hooks::fire_checkout_made(hooks, made, Duration::from_secs(30), &mut |_| {})
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
/// environment is this process's own, less what is scrubbed and with `PATH` reduced, with no name
/// added and no value carrying the tool or the moment.
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

/// HOOK-4, HOOK-5: the hook for a checkout that was just written runs in it, is given the checkout
/// and the working directory it was made from by name in its environment, and its standard input
/// is still the moment alone. A hook that is not told which is which would copy from the wrong one.
#[test]
fn a_checkout_hook_is_handed_the_checkout_and_the_working_directory_it_was_made_from() {
    let scratch = Scratch::new("checkout-handed");
    let program = script(
        &scratch.path,
        "keep-both",
        "#!/bin/sh\nprintf '%s' \"$BRAVEBOT_CHECKOUT\" > checkout.txt\n\
         printf '%s' \"$BRAVEBOT_WORKING_DIRECTORY\" > working.txt\ncat > told.json\n",
    );
    let hooks = declaring("checkout-made", &[program.to_str().expect("a path")]);

    let fired = fire_checkout_made(
        &hooks,
        hooks::Made {
            checkout: &scratch.path,
            working_directory: Path::new("/people/their/project"),
        },
    );

    assert_eq!(fired.len(), 1);
    assert_eq!(fired[0].trouble, None);
    assert_eq!(fired[0].moment, "checkout-made");
    assert_eq!(
        std::fs::read_to_string(scratch.path.join("checkout.txt"))
            .expect("the hook did not run in the checkout"),
        scratch.path.to_str().expect("a path"),
        "the hook was not told which checkout it was for"
    );
    assert_eq!(
        std::fs::read_to_string(scratch.path.join("working.txt")).expect("the hook ran"),
        "/people/their/project"
    );
    let told = std::fs::read_to_string(scratch.path.join("told.json")).expect("the hook was told");
    assert_eq!(
        told.trim(),
        r#"{"event":"checkout-made"}"#,
        "a directory reached the hook on standard input"
    );
}

/// HOOK-5, PATH-003: a directory reaches the hook as the bytes the file system holds. A working
/// directory whose name is not text is passed on as it is, and the name its replacement-character
/// rendering spells is passed on as that, so a lossy spelling would hand the hook the second for
/// the first.
#[cfg(unix)]
#[test]
fn a_checkout_hook_is_handed_a_directory_whose_name_is_not_text_as_it_is() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;
    let scratch = Scratch::new("checkout-bytes");
    let program = script(
        &scratch.path,
        "keep-bytes",
        "#!/bin/sh\nprintf '%s' \"$BRAVEBOT_WORKING_DIRECTORY\" > working.bin\n",
    );
    let hooks = declaring("checkout-made", &[program.to_str().expect("a path")]);
    let not_text = Path::new(OsStr::from_bytes(b"/people/caf\xe9/project"));
    let lookalike = Path::new("/people/caf\u{FFFD}/project");

    for working_directory in [not_text, lookalike] {
        let _ = std::fs::remove_file(scratch.path.join("working.bin"));

        fire_checkout_made(
            &hooks,
            hooks::Made {
                checkout: &scratch.path,
                working_directory,
            },
        );

        assert_eq!(
            std::fs::read(scratch.path.join("working.bin")).expect("the hook ran"),
            working_directory.as_os_str().as_bytes(),
            "the hook was told another directory than {working_directory:?}"
        );
    }
}

/// HOOK-4, HOOK-5, PATH-003: a hook runs in a checkout whose name is not text and is told that
/// name as the bytes the file system holds. The directory its replacement-character rendering
/// spells exists beside it, so a lossy spelling would run the hook in the wrong one or tell it the
/// wrong one. Linux only: other hosts' file systems cannot hold the name.
#[cfg(target_os = "linux")]
#[test]
fn a_checkout_hook_runs_in_a_checkout_whose_name_is_not_text_and_is_told_it_as_it_is() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;
    let scratch = Scratch::new("checkout-named-bytes");
    let program = script(
        &scratch.path,
        "keep-checkout",
        "#!/bin/sh\nprintf '%s' \"$BRAVEBOT_CHECKOUT\" > checkout.bin\n",
    );
    let hooks = declaring("checkout-made", &[program.to_str().expect("a path")]);
    let not_text = scratch.path.join(OsStr::from_bytes(b"checkout-\xe9"));
    let lookalike = scratch.path.join("checkout-\u{FFFD}");
    std::fs::create_dir(&not_text).expect("a checkout whose name is not text");
    std::fs::create_dir(&lookalike).expect("its lossy lookalike");

    for checkout in [&not_text, &lookalike] {
        let fired = fire_checkout_made(
            &hooks,
            hooks::Made {
                checkout,
                working_directory: Path::new("/people/their/project"),
            },
        );

        assert_eq!(
            fired[0].trouble, None,
            "the hook did not run in {checkout:?}"
        );
        assert_eq!(
            std::fs::read(checkout.join("checkout.bin")).expect("the hook ran in the checkout"),
            checkout.as_os_str().as_bytes(),
            "the hook was told another directory than the one it ran in"
        );
    }
}

/// HOOK-5: only the hook for a checkout is given the directories. The hook for any other moment is
/// told the moment and given neither, whichever moment it is.
#[test]
fn a_hook_for_any_other_moment_is_given_neither_directory() {
    let scratch = Scratch::new("checkout-not-handed");
    let program = script(
        &scratch.path,
        "dump-env",
        "#!/bin/sh\nexec env > seen.env\n",
    );
    for (word, moment, tool) in [
        ("turn-started", Moment::TurnStarted, None),
        ("tool-finished", Moment::ToolFinished, Some("write_file")),
        ("turn-finished", Moment::TurnFinished, None),
        ("prompt-shown", Moment::PromptShown, None),
    ] {
        let _ = std::fs::remove_file(scratch.path.join("seen.env"));
        let hooks = declaring(word, &[program.to_str().expect("a path")]);

        fire(&hooks, moment, tool, &scratch.path);

        let seen = std::fs::read_to_string(scratch.path.join("seen.env")).expect("the hook ran");
        assert!(
            !seen.is_empty(),
            "the {word} hook saw no environment at all"
        );
        assert!(
            !seen.contains("BRAVEBOT_CHECKOUT=") && !seen.contains("BRAVEBOT_WORKING_DIRECTORY="),
            "the {word} hook was given a directory of a checkout"
        );
    }
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

/// HOOK-7: the bound a hook that states no `timeout` is held to is the thirty seconds the clause
/// gives, a number the other tests do not state because they pass a short one in its place.
#[test]
fn the_default_bound_is_thirty_seconds() {
    assert_eq!(hooks::LIMIT, Duration::from_secs(30));
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

/// What a watcher saw, in order: the edge and whether the hook's own marker file existed then.
#[derive(Debug, PartialEq, Eq)]
enum Seen {
    Starting(String, bool),
    Over(bool),
}

/// Fire watched, recording each edge beside whether `marker` was there at that moment.
fn watching(
    hooks: &Hooks,
    moment: Moment,
    at: &Path,
    marker: &Path,
    limit: Duration,
) -> (Vec<Fired>, Vec<Seen>, Vec<Instant>) {
    let mut seen = Vec::new();
    let mut when = Vec::new();
    let held = starting();
    let fired = hooks::fire_watched(hooks, moment, None, at, limit, &mut |edge| {
        when.push(Instant::now());
        seen.push(match edge {
            hooks::Watch::Starting { moment, program } => {
                Seen::Starting(format!("{moment} {program}"), marker.exists())
            }
            hooks::Watch::Over => Seen::Over(marker.exists()),
        })
    });
    drop(held);
    (fired, seen, when)
}

use std::time::Instant;

/// HOOK-8: the person is told a hook is running before the process exists and that it is over
/// after it has ended, so what is drawn spans exactly the time the turn is held.
#[test]
fn a_hook_is_announced_before_it_starts_and_over_after_it_exits() {
    let scratch = Scratch::new("watched");
    let marker = scratch.path.join("ran");
    let program = script(
        &scratch.path,
        "touch",
        &format!("#!/bin/sh\nsleep 0.2\ntouch {marker:?}\n"),
    );
    let hooks = declaring("tool-finished", &[program.to_str().expect("a path")]);

    let (fired, seen, _) = watching(
        &hooks,
        Moment::ToolFinished,
        &scratch.path,
        &marker,
        Duration::from_secs(30),
    );

    assert_eq!(fired[0].trouble, None);
    assert_eq!(
        seen,
        vec![
            Seen::Starting(format!("tool-finished {}", program.display()), false),
            Seen::Over(true),
        ],
        "begin must precede the process and end must follow its exit"
    );
}

/// HOOK-8: a hook stopped at its bound is over once it has been stopped, not before.
#[test]
fn a_stopped_hook_is_over_only_after_it_was_stopped() {
    let scratch = Scratch::new("watched-stopped");
    let marker = scratch.path.join("ran");
    let program = script(&scratch.path, "linger", "#!/bin/sh\nsleep 30\n");
    let hooks = declaring("turn-finished", &[program.to_str().expect("a path")]);

    let (fired, seen, when) = watching(
        &hooks,
        Moment::TurnFinished,
        &scratch.path,
        &marker,
        Duration::from_millis(300),
    );

    assert!(matches!(fired[0].trouble, Some(Trouble::Stopped(_))));
    assert_eq!(seen.len(), 2, "{seen:?}");
    assert!(
        when[1] - when[0] >= Duration::from_millis(300),
        "over was said before the bound ran out"
    );
}

/// HOOK-8: a program that could not be started still closes the pair, or a display would say a
/// hook was running that never was.
#[test]
fn a_hook_that_never_started_is_still_closed() {
    let scratch = Scratch::new("watched-missing");
    let marker = scratch.path.join("ran");
    let hooks = declaring("turn-started", &["/no/such/program"]);

    let (_, seen, _) = watching(
        &hooks,
        Moment::TurnStarted,
        &scratch.path,
        &marker,
        Duration::from_secs(30),
    );

    assert_eq!(
        seen,
        vec![
            Seen::Starting("turn-started /no/such/program".into(), false),
            Seen::Over(false)
        ]
    );
}

/// HOOK-8: two hooks on one moment are two pairs, one after the other, never nested or merged.
#[test]
fn two_hooks_are_two_pairs_in_order() {
    let scratch = Scratch::new("watched-two");
    let marker = scratch.path.join("ran");
    let hooks = Hooks::parse(
        r#"{"hooks": [
            {"on": "turn-started", "run": ["/no/such/first"]},
            {"on": "turn-started", "run": ["/no/such/second"]}
        ]}"#,
    );

    let (_, seen, _) = watching(
        &hooks,
        Moment::TurnStarted,
        &scratch.path,
        &marker,
        Duration::from_secs(30),
    );

    assert_eq!(
        seen,
        vec![
            Seen::Starting("turn-started /no/such/first".into(), false),
            Seen::Over(false),
            Seen::Starting("turn-started /no/such/second".into(), false),
            Seen::Over(false),
        ]
    );
}

/// HOOK-8: nothing is announced for a moment that fires no hook.
#[test]
fn a_moment_with_no_hook_announces_nothing() {
    let scratch = Scratch::new("watched-none");
    let marker = scratch.path.join("ran");
    let hooks = declaring("turn-finished", &["/no/such/program"]);

    let (_, seen, _) = watching(
        &hooks,
        Moment::TurnStarted,
        &scratch.path,
        &marker,
        Duration::from_secs(30),
    );

    assert!(seen.is_empty());
}

/// A state directory under `at` holding a hooks file that declares `entries`, as a person writes one.
fn a_home_declaring(at: &Path, entries: &str) -> PathBuf {
    let home = at.join("state");
    std::fs::create_dir_all(&home).expect("a state directory");
    std::fs::write(
        home.join("hooks.json"),
        format!(r#"{{"hooks": [{entries}]}}"#),
    )
    .expect("a hooks file");
    home
}

/// `session-started` for `moments`, under the gate [`STARTING`] describes.
fn opened(moments: &mut hooks::SessionMoments) -> Vec<String> {
    let _held = starting();
    moments.started()
}

/// `session-ended` for `moments`, under the same gate.
fn closed(moments: &mut hooks::SessionMoments) -> Vec<String> {
    let _held = starting();
    moments.ended()
}

/// HOOK-10, HOOK-5: `session-started` fires once for a session however often the interface asks, and
/// the hook is told the moment and nothing else. The regression it rejects is firing at each turn or
/// each conversation, which runs a command meant to run once as often as the person types.
#[test]
fn a_session_start_hook_fires_once_however_often_it_is_asked() {
    let scratch = Scratch::new("session-started-once");
    let keep = script(&scratch.path, "keep-it", "#!/bin/sh\ncat >> told.json\n");
    let home = a_home_declaring(
        &scratch.path,
        &format!(
            r#"{{"on": "session-started", "run": [{:?}]}}"#,
            keep.to_str().expect("a path")
        ),
    );
    let mut moments = hooks::SessionMoments::read(Some(&home), &scratch.path);

    for _ in 0..3 {
        assert!(opened(&mut moments).is_empty());
    }

    let told = std::fs::read_to_string(scratch.path.join("told.json")).expect("the hook was told");
    assert_eq!(told, "{\"event\":\"session-started\"}\n");
}

/// HOOK-9, HOOK-10: an interface that is told as the start hook runs hears one pair of edges, from
/// before the process exists to after it has ended, and none from a later call, which fires
/// nothing. The regressions it rejects are edges that are not told at all, which leaves a window
/// with a first turn that looks slow, and a repeat call announcing a hook that never ran.
#[test]
fn a_session_start_hook_is_announced_once_however_often_it_is_asked() {
    let scratch = Scratch::new("session-started-watched");
    let keep = script(&scratch.path, "keep-it", "#!/bin/sh\ncat >> told.json\n");
    let program = keep.to_str().expect("a path").to_string();
    let home = a_home_declaring(
        &scratch.path,
        &format!(r#"{{"on": "session-started", "run": [{program:?}]}}"#),
    );
    let mut moments = hooks::SessionMoments::read(Some(&home), &scratch.path);
    let mut edges = Vec::new();
    let _held = starting();

    for _ in 0..3 {
        let said = moments.started_watched(&mut |edge| {
            edges.push(match edge {
                hooks::Watch::Starting { moment, program } => format!("{moment} {program}"),
                hooks::Watch::Over => "over".to_string(),
            })
        });
        assert!(said.is_empty());
    }

    assert_eq!(edges, [format!("session-started {program}"), "over".into()]);
}

/// HOOK-10: `session-ended` fires once however the interface is left, and never for a session that
/// did not begin. The regressions it rejects are an end fired for a session nobody opened, which
/// stops a service nothing started, and an end fired at every exit path the interface has.
#[test]
fn a_session_end_hook_fires_once_and_only_for_a_session_that_began() {
    let scratch = Scratch::new("session-ended-once");
    let keep = script(&scratch.path, "keep-it", "#!/bin/sh\ncat >> told.json\n");
    let home = a_home_declaring(
        &scratch.path,
        &format!(
            r#"{{"on": "session-ended", "run": [{:?}]}}"#,
            keep.to_str().expect("a path")
        ),
    );
    let mut never_began = hooks::SessionMoments::read(Some(&home), &scratch.path);
    assert!(closed(&mut never_began).is_empty());
    assert!(
        !scratch.path.join("told.json").exists(),
        "an end fired for a session that never began"
    );

    let mut moments = hooks::SessionMoments::read(Some(&home), &scratch.path);
    opened(&mut moments);
    assert!(closed(&mut moments).is_empty());
    assert!(closed(&mut moments).is_empty());

    let told = std::fs::read_to_string(scratch.path.join("told.json")).expect("the hook was told");
    assert_eq!(told, "{\"event\":\"session-ended\"}\n");
}

/// HOOK-2, HOOK-10: each end fires the hooks written for it, in the order the session has them, and
/// a hook written for a turn moment is not one of them.
#[test]
fn each_end_of_a_session_fires_the_hooks_written_for_it() {
    let scratch = Scratch::new("session-each-end");
    let note = script(
        &scratch.path,
        "note",
        "#!/bin/sh\necho \"$1\" >> fired.txt\n",
    );
    let note = note.to_str().expect("a path");
    let home = a_home_declaring(
        &scratch.path,
        &format!(
            r#"{{"on": "session-ended", "run": [{note:?}, "closed"]}},
               {{"on": "turn-started", "run": [{note:?}, "turn"]}},
               {{"on": "session-started", "run": [{note:?}, "opened"]}}"#
        ),
    );
    let mut moments = hooks::SessionMoments::read(Some(&home), &scratch.path);

    opened(&mut moments);
    closed(&mut moments);

    let fired = std::fs::read_to_string(scratch.path.join("fired.txt")).expect("both hooks ran");
    assert_eq!(fired, "opened\nclosed\n");
}

/// HOOK-10: both run in the directory the session opened in, so a relative path in the hook that
/// ends a session is the one its twin started from. The regression it rejects is the end running
/// wherever the process happens to be, which is not where the person's files are.
#[test]
fn a_session_hook_runs_in_the_directory_the_session_opened_in() {
    let scratch = Scratch::new("session-directory");
    let say = script(&scratch.path, "say-where", "#!/bin/sh\npwd >> where.txt\n");
    let say = say.to_str().expect("a path");
    let home = a_home_declaring(
        &scratch.path,
        &format!(
            r#"{{"on": "session-started", "run": [{say:?}]}},
               {{"on": "session-ended", "run": [{say:?}]}}"#
        ),
    );
    let mut moments = hooks::SessionMoments::read(Some(&home), &scratch.path);

    opened(&mut moments);
    closed(&mut moments);

    let places = std::fs::read_to_string(scratch.path.join("where.txt")).expect("a directory");
    let opened_in = scratch.path.canonicalize().expect("canonicalize");
    assert_eq!(places.lines().count(), 2, "{places}");
    for place in places.lines() {
        assert_eq!(
            PathBuf::from(place).canonicalize().expect("canonicalize"),
            opened_in
        );
    }
}

/// HOOK-10: the hooks are read when the session begins and the same ones fire at its end, so a file
/// edited halfway through does not give a session an end it did not begin with.
#[test]
fn the_hooks_a_session_began_with_are_the_ones_that_fire_at_its_end() {
    let scratch = Scratch::new("session-read-once");
    let note = script(
        &scratch.path,
        "note",
        "#!/bin/sh\necho \"$1\" >> fired.txt\n",
    );
    let note = note.to_str().expect("a path");
    let home = a_home_declaring(
        &scratch.path,
        &format!(r#"{{"on": "session-ended", "run": [{note:?}, "the-first"]}}"#),
    );
    let mut moments = hooks::SessionMoments::read(Some(&home), &scratch.path);
    opened(&mut moments);

    a_home_declaring(
        &scratch.path,
        &format!(r#"{{"on": "session-ended", "run": [{note:?}, "the-second"]}}"#),
    );
    closed(&mut moments);

    let fired = std::fs::read_to_string(scratch.path.join("fired.txt")).expect("the hook ran");
    assert_eq!(fired, "the-first\n");
}

/// HOOK-7, HOOK-10: a hook that goes wrong at either end is said, naming the moment and the program,
/// and neither end is held up by it.
#[test]
fn a_session_hook_that_goes_wrong_is_said_naming_the_moment_and_program() {
    let scratch = Scratch::new("session-trouble");
    let missing = scratch.path.join("not-installed");
    let missing = missing.to_str().expect("a path");
    let home = a_home_declaring(
        &scratch.path,
        &format!(
            r#"{{"on": "session-started", "run": [{missing:?}]}},
               {{"on": "session-ended", "run": [{missing:?}]}}"#
        ),
    );
    let mut moments = hooks::SessionMoments::read(Some(&home), &scratch.path);

    let at_start = opened(&mut moments);
    let at_end = closed(&mut moments);

    assert_eq!(at_start.len(), 1, "{at_start:?}");
    assert!(
        at_start[0].contains("session-started") && at_start[0].contains(missing),
        "{at_start:?}"
    );
    assert_eq!(at_end.len(), 1, "{at_end:?}");
    assert!(
        at_end[0].contains("session-ended") && at_end[0].contains(missing),
        "{at_end:?}"
    );
}

/// HOOK-7, HOOK-10: the bound of a `session-ended` hook that states no `timeout` is five seconds, a
/// number the tests below do not state because they pass a short one in its place.
#[test]
fn the_default_bound_of_a_session_end_is_five_seconds() {
    assert_eq!(hooks::SESSION_END_LIMIT, Duration::from_secs(5));
    assert!(hooks::SESSION_END_LIMIT < hooks::LIMIT);
}

/// HOOK-7: the bound a hook that states no `timeout` is held to is chosen per moment in one place,
/// and only a session end has a shorter one. The regression it rejects is a moment given the
/// session end's bound or the turn's, which a front end that asks this function would then report as
/// what is applied.
#[test]
fn only_a_session_end_has_a_shorter_default_bound_than_thirty_seconds() {
    for moment in Moment::ALL {
        let expected = Duration::from_secs(match moment {
            Moment::SessionEnded => 5,
            Moment::TurnStarted
            | Moment::ToolFinished
            | Moment::TurnFinished
            | Moment::PromptShown
            | Moment::CheckoutMade
            | Moment::Compacted
            | Moment::SessionStarted
            | Moment::FileChanged => 30,
        });
        assert_eq!(
            hooks::default_limit(moment),
            expected,
            "{}",
            moment.as_str()
        );
    }
}

/// HOOK-7, HOOK-10: a `session-ended` hook is stopped at the bound the end has, and not at the
/// thirty seconds every other moment's default is, and a `session-started` hook is not held to the
/// end's. The regressions it rejects are the end using the turn's default, which holds a person
/// who is leaving for half a minute, and the start using the end's, which cuts a service short while
/// it is still starting.
#[test]
fn a_session_end_hook_is_stopped_at_the_bound_the_end_has() {
    let scratch = Scratch::new("session-end-bound");
    let linger = script(&scratch.path, "linger", "#!/bin/sh\nsleep 30\n");
    let pause = script(
        &scratch.path,
        "pause",
        "#!/bin/sh\nsleep 1\ntouch started-fully\n",
    );
    let home = a_home_declaring(
        &scratch.path,
        &format!(
            r#"{{"on": "session-ended", "run": [{:?}]}},
               {{"on": "session-started", "run": [{:?}]}}"#,
            linger.to_str().expect("a path"),
            pause.to_str().expect("a path")
        ),
    );
    let mut moments = hooks::SessionMoments::read(Some(&home), &scratch.path)
        .ending_within(Duration::from_millis(200));

    assert!(opened(&mut moments).is_empty());
    assert!(
        scratch.path.join("started-fully").exists(),
        "the start was held to the end's bound"
    );

    let began = Instant::now();
    let at_end = closed(&mut moments);

    assert_eq!(at_end.len(), 1, "{at_end:?}");
    assert!(at_end[0].contains("session-ended"), "{at_end:?}");
    assert!(
        began.elapsed() < Duration::from_secs(20),
        "the end was held to the turn's bound"
    );
}

/// HOOK-7, HOOK-10: a `session-ended` hook that states a `timeout` keeps it, longer than the end's
/// default included. The regression it rejects is the end's bound overriding what the person wrote.
#[test]
fn a_session_end_hook_that_states_a_timeout_keeps_it() {
    let scratch = Scratch::new("session-end-timeout");
    let pause = script(
        &scratch.path,
        "pause",
        "#!/bin/sh\nsleep 1\ntouch finished\n",
    );
    let home = a_home_declaring(
        &scratch.path,
        &format!(
            r#"{{"on": "session-ended", "run": [{:?}], "timeout": 30}}"#,
            pause.to_str().expect("a path")
        ),
    );
    let mut moments = hooks::SessionMoments::read(Some(&home), &scratch.path)
        .ending_within(Duration::from_millis(200));
    opened(&mut moments);

    assert!(closed(&mut moments).is_empty());

    assert!(scratch.path.join("finished").exists());
}

/// HOOK-10: an interface is told whether a `session-started` hook will run, so it can draw what the
/// person is waiting on before the hook holds the first prompt back. Not for a session with no such
/// hook, which would be a frame drawn for nothing, and not once it has fired.
#[test]
fn an_interface_is_told_whether_a_start_hook_will_run() {
    let scratch = Scratch::new("session-will-start");
    let note = script(&scratch.path, "note", "#!/bin/sh\ntrue\n");
    let note = note.to_str().expect("a path");
    let with = a_home_declaring(
        &scratch.path.join("with"),
        &format!(r#"{{"on": "session-started", "run": [{note:?}]}}"#),
    );
    let without = a_home_declaring(
        &scratch.path.join("without"),
        &format!(r#"{{"on": "session-ended", "run": [{note:?}]}}"#),
    );

    let mut declared = hooks::SessionMoments::read(Some(&with), &scratch.path);
    assert!(declared.will_start_a_hook());
    opened(&mut declared);
    assert!(
        !declared.will_start_a_hook(),
        "it will not fire a second time"
    );

    let undeclared = hooks::SessionMoments::read(Some(&without), &scratch.path);
    assert!(!undeclared.will_start_a_hook());
}

/// A state directory declaring `file-changed` entries that each append a line to `ran-<name>` when
/// they fire, and a watched file for each, written as `a`. Answers the state directory and the
/// files.
fn watching_files(at: &Path, names: &[&str]) -> (PathBuf, Vec<PathBuf>) {
    let entries: Vec<String> = names
        .iter()
        .map(|name| {
            let run = script(
                at,
                &format!("run-{name}"),
                &format!("#!/bin/sh\ncat >> told-{name}.json\necho x >> ran-{name}\n"),
            );
            std::fs::write(at.join(name), "a").expect("a watched file");
            format!(
                r#"{{"on": "file-changed", "path": {name:?}, "run": [{:?}]}}"#,
                run.to_str().expect("a path")
            )
        })
        .collect();
    let home = a_home_declaring(at, &entries.join(", "));
    (home, names.iter().map(|name| at.join(name)).collect())
}

/// How many times the entry for `name` ran.
fn runs(at: &Path, name: &str) -> usize {
    std::fs::read_to_string(at.join(format!("ran-{name}"))).map_or(0, |text| text.lines().count())
}

fn polled(watched: &mut hooks::FileChanged, now: std::time::Instant) -> Vec<String> {
    let _held = starting();
    watched.poll(now)
}

/// HOOK-2: several changes seen in one window run the entry once, and a file that did not change
/// runs nothing. The regressions it rejects are a run for each change, which a save that writes
/// twice turns into two builds, and a run for each look.
#[test]
fn a_changed_file_runs_the_hook_once_for_the_changes_one_window_held() {
    let scratch = Scratch::new("file-changed-once");
    let (home, files) = watching_files(&scratch.path, &["moved", "still"]);
    let began = std::time::Instant::now();
    let mut watched = hooks::FileChanged::read(Some(&home), &scratch.path, began);
    let look = hooks::BETWEEN_LOOKS;

    for step in 1..=3 {
        std::fs::write(&files[0], "b".repeat(step + 1)).expect("change it");
    }
    assert!(polled(&mut watched, began + look).is_empty());
    assert_eq!(runs(&scratch.path, "moved"), 1);
    assert_eq!(runs(&scratch.path, "still"), 0, "nothing changed");

    polled(&mut watched, began + look * 2);
    assert_eq!(
        runs(&scratch.path, "moved"),
        1,
        "the next look saw no change"
    );
}

/// HOOK-2: a file is looked at no oftener than every five seconds, so a change made before the
/// first look is due runs nothing yet. The regression it rejects is a look at every poll.
#[test]
fn a_file_is_looked_at_no_oftener_than_the_interval() {
    let scratch = Scratch::new("file-changed-interval");
    let (home, files) = watching_files(&scratch.path, &["moved"]);
    let began = std::time::Instant::now();
    let mut watched = hooks::FileChanged::read(Some(&home), &scratch.path, began);

    std::fs::write(&files[0], "changed").expect("change it");
    polled(
        &mut watched,
        began + hooks::BETWEEN_LOOKS - Duration::from_millis(1),
    );
    assert_eq!(runs(&scratch.path, "moved"), 0);

    polled(&mut watched, began + hooks::BETWEEN_LOOKS);
    assert_eq!(runs(&scratch.path, "moved"), 1);
}

/// HOOK-2: a file that appears or is deleted is a change, as one that is written to is.
#[test]
fn a_file_that_appears_or_goes_is_a_change() {
    let scratch = Scratch::new("file-changed-appears");
    let (home, files) = watching_files(&scratch.path, &["moved"]);
    let began = std::time::Instant::now();
    let mut watched = hooks::FileChanged::read(Some(&home), &scratch.path, began);
    let look = hooks::BETWEEN_LOOKS;

    std::fs::remove_file(&files[0]).expect("delete it");
    polled(&mut watched, began + look);
    assert_eq!(runs(&scratch.path, "moved"), 1, "deleted");

    std::fs::write(&files[0], "back").expect("create it");
    polled(&mut watched, began + look * 3);
    assert_eq!(runs(&scratch.path, "moved"), 2, "created");
}

/// HOOK-5: the hook is told the moment and nothing else, though the entry names a path. The
/// regression it rejects is the path, or anything the filesystem produced, on its standard input.
#[test]
fn a_file_changed_hook_is_told_the_moment_and_not_the_path() {
    let scratch = Scratch::new("file-changed-told");
    let (home, files) = watching_files(&scratch.path, &["moved"]);
    let began = std::time::Instant::now();
    let mut watched = hooks::FileChanged::read(Some(&home), &scratch.path, began);

    std::fs::write(&files[0], "changed").expect("change it");
    polled(&mut watched, began + hooks::BETWEEN_LOOKS);

    let told = std::fs::read_to_string(scratch.path.join("told-moved.json")).expect("told");
    assert_eq!(told, "{\"event\":\"file-changed\"}\n");
}

/// HOOK-2: an entry runs at most once in the floor after its last run, and the change seen in the
/// meantime runs it when the floor is over. The regression it rejects is no floor, where a command
/// that writes the file it watches runs at every look.
#[test]
fn a_hook_that_ran_lately_waits_out_the_floor_before_it_runs_again() {
    let scratch = Scratch::new("file-changed-floor");
    let (home, files) = watching_files(&scratch.path, &["moved"]);
    let began = std::time::Instant::now();
    let mut watched = hooks::FileChanged::read(Some(&home), &scratch.path, began);
    let look = hooks::BETWEEN_LOOKS;

    std::fs::write(&files[0], "one").expect("change it");
    polled(&mut watched, began + look);
    std::fs::write(&files[0], "second").expect("change it again");
    polled(&mut watched, began + look * 2);
    assert_eq!(runs(&scratch.path, "moved"), 1, "within the floor");

    polled(&mut watched, began + look * 2 + hooks::BETWEEN_RUNS);
    assert_eq!(
        runs(&scratch.path, "moved"),
        2,
        "the change was kept, not lost"
    );
}

/// HOOK-2: a session keeps eight entries and no more. The regression it rejects is every entry
/// kept, which is a file of paths looked at forever.
#[test]
fn a_session_keeps_no_more_than_eight_file_entries() {
    let scratch = Scratch::new("file-changed-bound");
    let names: Vec<String> = (0..9).map(|n| format!("f{n}")).collect();
    let names: Vec<&str> = names.iter().map(String::as_str).collect();
    let (home, files) = watching_files(&scratch.path, &names);
    let began = std::time::Instant::now();
    let mut watched = hooks::FileChanged::read(Some(&home), &scratch.path, began);

    for file in &files {
        std::fs::write(file, "changed").expect("change it");
    }
    polled(&mut watched, began + hooks::BETWEEN_LOOKS);

    let ran: usize = names.iter().map(|name| runs(&scratch.path, name)).sum();
    assert_eq!(ran, hooks::MAX_WATCHED);
    assert_eq!(runs(&scratch.path, "f8"), 0, "the ninth is not kept");
}

/// HOOK-2: entries for the other moments are not looked at.
#[test]
fn an_entry_for_another_moment_watches_no_file() {
    let scratch = Scratch::new("file-changed-other");
    let home = a_home_declaring(
        &scratch.path,
        r#"{"on": "turn-finished", "path": "x", "run": ["true"]}"#,
    );

    let watched = hooks::FileChanged::read(Some(&home), &scratch.path, std::time::Instant::now());

    assert!(watched.is_empty());
}

/// HOOK-2: a tool named on a `file-changed` entry fires it for nothing, so the watch keeps no file
/// for it. The regression it rejects is a filter on the moment alone, which watches and runs an
/// entry the editor reports as never firing.
#[test]
fn a_tool_named_on_a_file_changed_entry_watches_no_file() {
    let scratch = Scratch::new("file-changed-tool");
    std::fs::write(scratch.path.join("moved"), "a").expect("a watched file");
    let home = a_home_declaring(
        &scratch.path,
        r#"{"on": "file-changed", "path": "moved", "tool": "edit", "run": ["true"]}"#,
    );

    let watched = hooks::FileChanged::read(Some(&home), &scratch.path, std::time::Instant::now());

    assert!(watched.is_empty());
}

/// HOOK-7: a hook that could not start is said, naming the moment and the program, and the watch
/// goes on.
#[test]
fn a_file_changed_hook_that_could_not_start_is_said() {
    let scratch = Scratch::new("file-changed-trouble");
    std::fs::write(scratch.path.join("moved"), "a").expect("a watched file");
    let home = a_home_declaring(
        &scratch.path,
        r#"{"on": "file-changed", "path": "moved", "run": ["/nonexistent/notify"]}"#,
    );
    let began = std::time::Instant::now();
    let mut watched = hooks::FileChanged::read(Some(&home), &scratch.path, began);

    std::fs::write(scratch.path.join("moved"), "changed").expect("change it");
    let said = polled(&mut watched, began + hooks::BETWEEN_LOOKS);

    assert_eq!(said.len(), 1, "{said:?}");
    assert!(said[0].contains("file-changed"), "{said:?}");
    assert!(said[0].contains("/nonexistent/notify"), "{said:?}");
}

/// HOOK-7, HOOK-9: an interface looking on its own thread is told what a hook said, without a turn
/// to carry it. The regression it rejects is a thread that runs the hook and drops what it said.
#[test]
fn a_thread_looking_for_an_interface_hands_back_what_a_hook_said() {
    let scratch = Scratch::new("file-changed-thread");
    std::fs::write(scratch.path.join("moved"), "a").expect("a watched file");
    let home = a_home_declaring(
        &scratch.path,
        r#"{"on": "file-changed", "path": "moved", "run": ["/nonexistent/notify"]}"#,
    );
    let watched = hooks::FileChanged::read(Some(&home), &scratch.path, std::time::Instant::now())
        .looking_every(Duration::from_millis(20));
    let watching = hooks::Watching::begin(watched).expect("one entry is kept");

    std::fs::write(scratch.path.join("moved"), "changed").expect("change it");
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    let mut said = Vec::new();
    while said.is_empty() && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
        said = watching.said();
    }

    assert_eq!(said.len(), 1, "{said:?}");
    assert!(said[0].contains("/nonexistent/notify"), "{said:?}");
}

/// HOOK-2: nothing kept is nothing to look on a thread.
#[test]
fn no_thread_looks_where_no_entry_is_kept() {
    let scratch = Scratch::new("file-changed-no-thread");
    let watched = hooks::FileChanged::read(None, &scratch.path, std::time::Instant::now());

    assert!(hooks::Watching::begin(watched).is_none());
}
