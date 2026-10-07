//! Tests for running a pipeline, against real processes in a real directory.
//!
//! Everything here is about the plumbing rather than the gates: whether stages are chained the way
//! a shell would chain them, whether an argument survives intact, and whether a run that goes
//! wrong comes back with what it produced instead of nothing.

use bravebot_agent::exec::{self, ExecError};
use bravebot_core::cancel::{Cancel, Handoff, JobStop};
use bravebot_core::{Pipeline, Stage};
use std::path::PathBuf;
use std::time::Duration;

/// A scratch directory that removes itself, so tests do not leave state behind.
struct Scratch {
    path: PathBuf,
}

impl Scratch {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("bravebot-exec-{name}"));
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

/// Serialises the tests that set variables, and restores what was there.
///
/// The environment belongs to the process, so two of these running at once would each see the
/// other's values and both would be testing something nobody wrote. The lock is held by the guard,
/// which is why every caller binds it: `let _ = with_env(..)` drops it immediately and takes the
/// variables with it.
static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

struct EnvGuard {
    restore: Vec<(String, Option<std::ffi::OsString>)>,
    _lock: std::sync::MutexGuard<'static, ()>,
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (name, previous) in &self.restore {
            // SAFETY: single-threaded within the lock this guard holds.
            match previous {
                Some(value) => unsafe { std::env::set_var(name, value) },
                None => unsafe { std::env::remove_var(name) },
            }
        }
    }
}

fn with_env(vars: &[(&str, Option<&str>)]) -> EnvGuard {
    let lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut restore = Vec::new();
    for (name, value) in vars {
        restore.push(((*name).to_string(), std::env::var_os(name)));
        // SAFETY: single-threaded within the lock, and restored when the guard drops.
        match value {
            Some(value) => unsafe { std::env::set_var(name, value) },
            None => unsafe { std::env::remove_var(name) },
        }
    }
    EnvGuard {
        restore,
        _lock: lock,
    }
}

/// Resolve every stage the way the tool does, then run it.
fn run(pipeline: Pipeline, at: &std::path::Path) -> Result<exec::Ran, ExecError> {
    let resolved = resolve_all(&pipeline, at)?;
    run_resolved(&pipeline, &resolved, at)
}

fn resolve_all(
    pipeline: &Pipeline,
    at: &std::path::Path,
) -> Result<Vec<std::path::PathBuf>, ExecError> {
    pipeline
        .stages
        .iter()
        .map(|stage| {
            bravebot_agent::programs::resolve(&stage.program, at).ok_or_else(|| {
                ExecError::NotStarted {
                    program: stage.program.clone(),
                    detail: "not found".to_string(),
                }
            })
        })
        .collect()
}

/// Make `name` an executable script in `at`, and return the path it resolved to.
fn script(at: &std::path::Path, name: &str, body: &str) -> PathBuf {
    let path = at.join(name);
    std::fs::write(&path, body).expect("write the script");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
            .expect("make it executable");
    }
    path.canonicalize().expect("canonicalize")
}

/// Runs `attempt`, retrying while it fails because the program is held open for writing.
///
/// A test that writes a program and then runs it races every other test in this binary. Between a
/// sibling thread's fork and its exec the child holds a copy of every descriptor this process had
/// open, and the descriptor [`script`] wrote through is close-on-exec, so it lives until that
/// exec: for that window this process is itself a writer holding the new program open, and
/// `execve` answers `ETXTBSY` for precisely that. The window belongs to whichever thread forked,
/// so there is nothing to close here and nothing to synchronise on, only to wait out. Left
/// unhandled it is an occasional failure in a test that has nothing to do with what it reports.
fn past_text_file_busy<T>(
    mut attempt: impl FnMut() -> Result<T, ExecError>,
) -> Result<T, ExecError> {
    // `ETXTBSY`, 26 on both Linux and macOS, compared as the message the operating system gives
    // it because that string is all `NotStarted` carries.
    let busy = std::io::Error::from_raw_os_error(26).to_string();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let outcome = attempt();
        let held_open =
            matches!(&outcome, Err(ExecError::NotStarted { detail, .. }) if *detail == busy);
        if !held_open || std::time::Instant::now() >= deadline {
            return outcome;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

/// [`exec::run`] for a pipeline already resolved, waiting out a busy program.
fn run_resolved(
    pipeline: &Pipeline,
    resolved: &[PathBuf],
    at: &std::path::Path,
) -> Result<exec::Ran, ExecError> {
    past_text_file_busy(|| exec::run(pipeline, resolved, at, &Cancel::new(), None))
}

/// [`exec::run_within`], waiting out a busy program.
fn run_within(
    pipeline: &Pipeline,
    resolved: &[PathBuf],
    at: &std::path::Path,
    limit: std::time::Duration,
) -> Result<exec::Ran, ExecError> {
    past_text_file_busy(|| exec::run_within(pipeline, resolved, at, &Cancel::new(), limit, None))
}

/// [`exec::start`], waiting out a busy program.
fn start(
    pipeline: &Pipeline,
    resolved: &[PathBuf],
    at: &std::path::Path,
) -> Result<exec::Background, ExecError> {
    past_text_file_busy(|| exec::start(pipeline, resolved, at, None))
}

#[test]
fn a_single_stage_returns_what_it_printed() {
    let scratch = Scratch::new("single");
    let ran = run(
        Pipeline::new(vec![Stage::new("echo", vec!["hello".into()])]),
        &scratch.path,
    )
    .expect("echo runs");
    assert_eq!(ran.stdout.trim(), "hello");
    assert!(ran.succeeded());
}

/// The reason `run` takes a pipeline rather than a single program: narrowing output has to be a
/// stage, because a pipe character would be a destination nobody approved.
#[test]
fn stages_are_chained_so_one_feeds_the_next() {
    let scratch = Scratch::new("chain");
    let ran = run(
        Pipeline::new(vec![
            Stage::new("printf", vec!["a\nb\nc\n".into()]),
            Stage::new("wc", vec!["-l".into()]),
        ]),
        &scratch.path,
    )
    .expect("the pipeline runs");
    assert_eq!(ran.stdout.trim(), "3");
    assert!(ran.succeeded());
}

/// The property the whole tool rests on. A metacharacter in an argument is one argument, because
/// nothing ever builds a command line for anything to re-parse.
#[test]
fn a_metacharacter_in_an_argument_stays_one_argument() {
    let scratch = Scratch::new("metachar");
    std::fs::write(scratch.path.join("keep.txt"), "still here").unwrap();

    let ran = run(
        Pipeline::new(vec![Stage::new(
            "echo",
            vec!["; rm -rf .".into(), "&& whoami".into(), "$(id)".into()],
        )]),
        &scratch.path,
    )
    .expect("echo runs");

    assert_eq!(ran.stdout.trim(), "; rm -rf . && whoami $(id)");
    assert!(
        scratch.path.join("keep.txt").exists(),
        "a metacharacter was interpreted rather than carried"
    );
}

/// An argument that looks like a redirection is text, not a destination.
#[test]
fn a_redirection_in_an_argument_writes_no_file() {
    let scratch = Scratch::new("redirect");
    let ran = run(
        Pipeline::new(vec![Stage::new(
            "echo",
            vec![">".into(), "written.txt".into()],
        )]),
        &scratch.path,
    )
    .expect("echo runs");
    assert_eq!(ran.stdout.trim(), "> written.txt");
    assert!(
        !scratch.path.join("written.txt").exists(),
        "an argument was treated as a redirection"
    );
}

#[test]
fn a_stage_runs_in_the_directory_it_was_given() {
    let scratch = Scratch::new("cwd");
    let ran = run(
        Pipeline::new(vec![Stage::new("pwd", Vec::new())]),
        &scratch.path,
    )
    .expect("pwd runs");
    // The scratch path may be reached through a symlink, so the tail is what is compared.
    assert!(
        ran.stdout.trim().ends_with("bravebot-exec-cwd"),
        "ran somewhere else: {}",
        ran.stdout.trim()
    );
}

/// A failing stage is reported as failing, and its explanation comes back rather than being
/// dropped. A run that produced something must not come back empty.
#[test]
fn a_failing_stage_reports_its_code_and_its_message() {
    let scratch = Scratch::new("failing");
    let ran = run(
        Pipeline::new(vec![Stage::new("ls", vec!["no-such-file".into()])]),
        &scratch.path,
    )
    .expect("ls runs even when it fails");
    assert!(!ran.succeeded());
    assert_eq!(ran.failures().len(), 1);
    assert!(
        !ran.stderr.is_empty(),
        "a stage explained itself and the explanation was dropped"
    );
}

/// A run that failed put its explanation on standard error, and a reader who cannot tell that
/// explanation from the result concludes that the command worked. Run together, `ls: nosuch: No such
/// file or directory` reads as a line the listing printed.
#[test]
fn standard_error_comes_back_labelled_beside_standard_output() {
    assert_eq!(exec::both_streams("a.txt\n", ""), "a.txt\n");
    assert_eq!(
        exec::both_streams("a.txt\n", "ls: nosuch: No such file or directory\n"),
        "a.txt\nstandard error:\nls: nosuch: No such file or directory\n"
    );
    // A command that printed nothing else is still told which stream it is reading.
    assert_eq!(exec::both_streams("", "boom\n"), "standard error:\nboom\n");
    // A last line with no newline of its own must not run into the label.
    assert_eq!(
        exec::both_streams("a.txt", "boom\n"),
        "a.txt\nstandard error:\nboom\n"
    );
}

/// Backgrounding changes when the planner is told, never what it is told, so the label a waited-for
/// run puts on standard error is on a background run's output too.
#[test]
fn a_background_run_labels_standard_error_as_a_waited_for_one_does() {
    let scratch = Scratch::new("background-stderr");
    let resolved = script(
        &scratch.path,
        "both",
        "#!/bin/sh\necho listing\necho boom >&2\n",
    );

    let pipeline = Pipeline::new(vec![Stage::new("both", Vec::new())]);
    let mut job = start(&pipeline, &[resolved], &scratch.path).expect("it starts");
    for _ in 0..100 {
        if job.ended() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }

    assert_eq!(job.printed(), "listing\nstandard error:\nboom\n");
}

/// The incremental read is the one production takes, for `job_output` and for the wake-up between
/// rounds, so the label has to be on what `since` hands over, and on each delivery that carries
/// standard error: a later one does not repeat the earlier, so a reader of it has no label to look
/// back to.
#[test]
fn what_a_background_run_hands_over_incrementally_labels_each_delivery_of_standard_error() {
    let scratch = Scratch::new("background-since-stderr");
    let looked = scratch.path.join("looked");
    let resolved = script(
        &scratch.path,
        "both",
        &format!(
            "#!/bin/sh\necho out1\necho err1 >&2\nwhile [ ! -e {} ]; do sleep 0.05; done\n\
             echo err2 >&2\n",
            looked.display()
        ),
    );

    let pipeline = Pipeline::new(vec![Stage::new("both", Vec::new())]);
    let mut job = start(&pipeline, &[resolved], &scratch.path).expect("it starts");
    let mut seen = exec::Seen::default();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while !(job.printed().contains("out1") && job.printed().contains("err1"))
        && std::time::Instant::now() < deadline
    {
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert_eq!(job.since(&mut seen), "out1\nstandard error:\nerr1\n");

    std::fs::write(&looked, "").expect("release the second line");
    until_ended(&mut job);
    assert_eq!(job.since(&mut seen), "standard error:\nerr2\n");
}

/// A shell reports only the last stage, which hides the case that matters: an early stage failing
/// while a later one cheerfully processes the nothing it was handed.
#[test]
fn an_early_stage_failing_makes_the_whole_pipeline_fail() {
    let scratch = Scratch::new("early");
    let ran = run(
        Pipeline::new(vec![
            Stage::new("ls", vec!["no-such-file".into()]),
            Stage::new("wc", vec!["-l".into()]),
        ]),
        &scratch.path,
    )
    .expect("the pipeline runs");
    assert!(
        !ran.succeeded(),
        "an early failure was hidden by a later success"
    );
    assert_eq!(ran.failures().first().map(|(at, _)| *at), Some(1));
}

/// A program that is not installed is a refusal to report, not a panic. The name is safe to say
/// back: argv was endorsed by a person, so it is not something an attacker chose.
#[test]
fn a_program_that_does_not_exist_is_reported_by_name() {
    let scratch = Scratch::new("missing");
    let error = run(
        Pipeline::new(vec![Stage::new(
            "bravebot-no-such-program-anywhere",
            Vec::new(),
        )]),
        &scratch.path,
    )
    .expect_err("a missing program cannot run");
    match error {
        ExecError::NotStarted { program, .. } => {
            assert_eq!(program, "bravebot-no-such-program-anywhere");
        }
        other => panic!("expected NotStarted, got {other:?}"),
    }
}

/// Nothing is typed at a program bravebot started, so a stage that reads stdin gets an empty one.
/// Inheriting the terminal's would hang the turn on input nobody is going to provide.
#[test]
fn a_stage_that_reads_stdin_is_given_nothing_rather_than_the_terminal() {
    let scratch = Scratch::new("stdin");
    let ran = run(
        Pipeline::new(vec![Stage::new("cat", Vec::new())]),
        &scratch.path,
    )
    .expect("cat runs and ends");
    assert_eq!(ran.stdout, "", "something was fed in that nobody approved");
    assert!(ran.succeeded());
}

/// The weakness this closes: a person approving a run reads the binary, the argv and the
/// directory, so a credential travelling in the environment is granted without having been seen.
/// The signing key has no use in a subprocess, which is doing what somebody approved rather than
/// authenticating as this agent.
#[cfg(unix)]
#[test]
fn this_agents_own_credentials_do_not_reach_a_program_it_runs() {
    let scratch = Scratch::new("scrub");
    let _guard = with_env(&[
        ("SERVICES_KEY_AICHAT", Some("a-live-signing-key")),
        ("BRAVE_SERVICES_KEY_ID", Some("a-live-key-id")),
    ]);

    // `env` rather than a shell expansion: the point is what the process was handed, and a stage
    // that expanded a variable itself would be testing this crate's argv handling instead.
    let ran = run(
        Pipeline::new(vec![Stage::new("env", Vec::new())]),
        &scratch.path,
    )
    .expect("env runs");

    assert!(ran.succeeded());
    assert!(
        !ran.stdout.contains("a-live-signing-key"),
        "the signing key reached a program the planner chose"
    );
    assert!(
        !ran.stdout.contains("a-live-key-id"),
        "the key id reached a program the planner chose"
    );
}

/// A home directory whose settings declare a gateway that reads its token from a variable no
/// built-in list names, with the variable set to a live-looking value.
///
/// `HOME` is pointed at the scratch directory so the block is the one the program under test loads,
/// and the name is deliberately not `OPENROUTER_API_KEY`: a withheld set that happened to list the
/// well-known gateways would pass with that one.
#[cfg(unix)]
fn with_a_gateway_token(scratch: &Scratch) -> EnvGuard {
    let home = scratch.path.join("home");
    std::fs::create_dir_all(home.join(".bravebot")).expect("create the home");
    std::fs::write(
        home.join(".bravebot").join("settings.json"),
        r#"{"provider": {"acme": {
            "options": {"baseURL": "https://gateway.acme.invalid/v1"},
            "env": ["ACME_GATEWAY_TOKEN", "ACME_GATEWAY_FALLBACK"]
        }}}"#,
    )
    .expect("write the settings");
    with_env(&[
        ("HOME", Some(home.to_str().expect("a path"))),
        ("ACME_GATEWAY_TOKEN", Some("a-live-gateway-token")),
        ("ACME_GATEWAY_FALLBACK", Some("a-live-fallback-token")),
        ("BRAVEBOT_SUBPROCESS_ENV_SCRUB", None),
    ])
}

/// The request the withheld variable is for still carries it: it is withheld from the programs this
/// agent starts, and the agent reads it from its own environment.
#[cfg(unix)]
fn the_gateway_still_authenticates() {
    use bravebot_config::provider::Credential;
    let settings = bravebot_config::Settings::load();
    let acme = settings
        .providers()
        .iter()
        .find(|provider| provider.id == "acme")
        .expect("the block was read");
    match acme.credential(|name| std::env::var(name).ok()) {
        Credential::Token(token) => assert_eq!(token.expose(), "a-live-gateway-token"),
        _ => panic!("the gateway lost its token, so withholding it broke the request"),
    }
}

/// RUN-12: the variables a provider block names are withheld from a program the planner chose,
/// every one the block lists, and the gateway still authenticates.
#[cfg(unix)]
#[test]
fn a_gateways_environment_token_does_not_reach_a_program_it_runs() {
    let scratch = Scratch::new("scrub-gateway");
    let _guard = with_a_gateway_token(&scratch);

    let ran = run(
        Pipeline::new(vec![Stage::new("env", Vec::new())]),
        &scratch.path,
    )
    .expect("env runs");

    assert!(ran.succeeded());
    assert!(
        !ran.stdout.contains("a-live-gateway-token"),
        "the gateway's token reached a program the planner chose"
    );
    assert!(
        !ran.stdout.contains("a-live-fallback-token"),
        "only the first variable the block lists was withheld"
    );
    the_gateway_still_authenticates();
}

/// A later stage is as reachable as the first.
#[cfg(unix)]
#[test]
fn no_stage_of_a_pipeline_sees_a_gateways_environment_token() {
    let scratch = Scratch::new("scrub-gateway-stages");
    let _guard = with_a_gateway_token(&scratch);

    let ran = run(
        Pipeline::new(vec![
            Stage::new("true", Vec::new()),
            Stage::new("env", Vec::new()),
        ]),
        &scratch.path,
    )
    .expect("the pipeline runs");

    assert!(
        !ran.stdout.contains("a-live-gateway-token"),
        "a later stage was handed the gateway's token"
    );
}

/// The background path starts its own command, so the foreground test says nothing about it.
#[cfg(unix)]
#[test]
fn a_background_job_is_not_handed_a_gateways_environment_token() {
    let scratch = Scratch::new("scrub-gateway-background");
    let _guard = with_a_gateway_token(&scratch);
    let resolved = script(
        &scratch.path,
        "show-env",
        "#!/bin/sh
env
",
    );

    let pipeline = Pipeline::new(vec![Stage::new("show-env", Vec::new())]);
    let mut job = start(&pipeline, &[resolved], &scratch.path).expect("it starts");
    for _ in 0..100 {
        if job.ended() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }

    let printed = job.printed();
    assert!(job.ended(), "the job never ended");
    assert!(
        printed.contains("PATH="),
        "the job printed no environment at all: {printed}"
    );
    assert!(
        !printed.contains("a-live-gateway-token") && !printed.contains("a-live-fallback-token"),
        "a background job was handed a gateway's token"
    );
    the_gateway_still_authenticates();
}

/// A variable no provider block names is the person's own and still reaches the program, so the
/// set is read from the configuration rather than widened to anything token-shaped.
#[cfg(unix)]
#[test]
fn a_variable_no_provider_block_names_still_reaches_a_program() {
    let scratch = Scratch::new("scrub-gateway-unnamed");
    let _guard = with_a_gateway_token(&scratch);
    // SAFETY: the guard above holds ENV_LOCK, and the variable is removed again below.
    unsafe { std::env::set_var("OPENROUTER_API_KEY", "a-key-no-block-reads") };

    let ran = run(
        Pipeline::new(vec![Stage::new("env", Vec::new())]),
        &scratch.path,
    )
    .expect("env runs");
    // SAFETY: as above, before the assertion so a failure cannot leave it set.
    unsafe { std::env::remove_var("OPENROUTER_API_KEY") };

    assert!(
        ran.stdout.contains("a-key-no-block-reads"),
        "a variable no block names was withheld"
    );
}

/// The escape hatch restores a gateway's variable along with the signing key.
#[cfg(unix)]
#[test]
fn switching_the_filtering_off_restores_a_gateways_environment_token() {
    let scratch = Scratch::new("scrub-gateway-off");
    let _guard = with_a_gateway_token(&scratch);
    // SAFETY: the guard above holds ENV_LOCK, and the previous value is restored below.
    unsafe { std::env::set_var("BRAVEBOT_SUBPROCESS_ENV_SCRUB", "0") };

    let ran = run(
        Pipeline::new(vec![Stage::new("env", Vec::new())]),
        &scratch.path,
    )
    .expect("env runs");

    // SAFETY: as above; the guard recorded the variable as absent, so it removes it again, but
    // it is removed here first so a failed assertion cannot leave it set for the next test.
    unsafe { std::env::remove_var("BRAVEBOT_SUBPROCESS_ENV_SCRUB") };
    assert!(
        ran.stdout.contains("a-live-gateway-token"),
        "`0` did not restore a gateway's variable"
    );
}

/// Every stage, not only the first. A credential is as reachable from the middle of a pipeline as
/// from the front, so one stage spared would be the whole of the hole.
#[cfg(unix)]
#[test]
fn no_stage_of_a_pipeline_sees_this_agents_credentials() {
    let scratch = Scratch::new("scrub-stages");
    let _guard = with_env(&[("SERVICES_KEY_AICHAT", Some("a-live-signing-key"))]);

    // The first stage prints nothing of interest; the second is the one being asked. `env` in a
    // later position is the case a middle stage represents.
    let ran = run(
        Pipeline::new(vec![
            Stage::new("true", Vec::new()),
            Stage::new("env", Vec::new()),
        ]),
        &scratch.path,
    )
    .expect("the pipeline runs");

    assert!(
        !ran.stdout.contains("a-live-signing-key"),
        "a later stage was handed the signing key"
    );
}

/// The user's own environment is left alone. `run aws s3 ls` and `run gh pr list` are ordinary
/// requests, and a filter matching names cannot tell one of those from an exfiltration, so what
/// holds here is narrow and exact rather than broad and approximate.
#[cfg(unix)]
#[test]
fn the_users_own_environment_still_reaches_a_program() {
    let scratch = Scratch::new("scrub-keeps");
    let _guard = with_env(&[
        ("AWS_PROFILE", Some("some-profile")),
        ("GITHUB_TOKEN", Some("a-github-token")),
    ]);

    let ran = run(
        Pipeline::new(vec![Stage::new("env", Vec::new())]),
        &scratch.path,
    )
    .expect("env runs");

    assert!(
        ran.stdout.contains("some-profile"),
        "AWS_PROFILE was withheld, so `run aws s3 ls` would stop working"
    );
    assert!(
        ran.stdout.contains("a-github-token"),
        "GITHUB_TOKEN was withheld, so `run gh pr list` would stop working"
    );
}

/// A program still needs the plumbing its caller had. Clearing the environment and allowing a set
/// back in would break the promise `run` makes about `git push`, which needs `HOME` to find
/// `~/.ssh`, so the environment is inherited less the credentials rather than rebuilt.
#[cfg(unix)]
#[test]
fn the_plumbing_a_program_needs_is_still_inherited() {
    let scratch = Scratch::new("scrub-plumbing");
    let ran = run(
        Pipeline::new(vec![Stage::new("env", Vec::new())]),
        &scratch.path,
    )
    .expect("env runs");

    for name in ["PATH=", "HOME="] {
        assert!(
            ran.stdout.contains(name),
            "{name} was withheld, so a program that needs it would fail"
        );
    }
}

/// The escape hatch, for a setup that turns out to need one of the names. Only the documented
/// spelling works: a credential reaching every subprocess is not a thing to switch off by a
/// near-miss like `false` or `off`.
#[cfg(unix)]
#[test]
fn the_filtering_can_be_switched_off_by_its_documented_spelling_only() {
    let scratch = Scratch::new("scrub-off");

    {
        let _guard = with_env(&[
            ("SERVICES_KEY_AICHAT", Some("a-live-signing-key")),
            ("BRAVEBOT_SUBPROCESS_ENV_SCRUB", Some("0")),
        ]);
        let ran = run(
            Pipeline::new(vec![Stage::new("env", Vec::new())]),
            &scratch.path,
        )
        .expect("env runs");
        assert!(
            ran.stdout.contains("a-live-signing-key"),
            "`0` did not restore inheritance, so the escape hatch does not work"
        );
    }

    for spelling in ["false", "off", "no", ""] {
        let _guard = with_env(&[
            ("SERVICES_KEY_AICHAT", Some("a-live-signing-key")),
            ("BRAVEBOT_SUBPROCESS_ENV_SCRUB", Some(spelling)),
        ]);
        let ran = run(
            Pipeline::new(vec![Stage::new("env", Vec::new())]),
            &scratch.path,
        )
        .expect("env runs");
        assert!(
            !ran.stdout.contains("a-live-signing-key"),
            "{spelling:?} switched the filtering off, and only `0` may"
        );
    }
}

/// A user who changes their mind does not wait out a slow program, and the program does not
/// survive the decision.
#[test]
fn cancelling_stops_a_running_pipeline() {
    let scratch = Scratch::new("cancel");
    let cancel = Cancel::new();
    let flag = cancel.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(150));
        flag.cancel();
    });

    let pipeline = Pipeline::new(vec![Stage::new("sleep", vec!["30".into()])]);
    let resolved = resolve_all(&pipeline, &scratch.path).expect("sleep is installed");
    let started = std::time::Instant::now();
    let error =
        exec::run(&pipeline, &resolved, &scratch.path, &cancel, None).expect_err("cancelled");
    assert!(matches!(error, ExecError::Cancelled));
    assert!(
        started.elapsed() < std::time::Duration::from_secs(5),
        "cancelling did not stop the pipeline promptly"
    );
}

/// More output than a pipe buffer holds must not deadlock. The stages are chained by descriptor
/// and stderr is drained on its own thread precisely so this case works.
#[test]
fn a_large_result_does_not_deadlock() {
    let scratch = Scratch::new("large");
    let ran = run(
        Pipeline::new(vec![
            Stage::new("yes", vec!["padding-line".into()]),
            Stage::new("head", vec!["-n".into(), "200000".into()]),
            Stage::new("wc", vec!["-l".into()]),
        ]),
        &scratch.path,
    )
    .expect("the pipeline runs");
    assert_eq!(ran.stdout.trim(), "200000");
}

/// What executes is the path that was resolved, not the name. Resolving again at spawn time would
/// leave a window in which `$PATH` changed and something other than what was approved ran.
#[test]
fn a_stage_runs_the_binary_it_was_resolved_to() {
    let scratch = Scratch::new("resolved");
    let shadow = [script(&scratch.path, "echo", "#!/bin/sh\necho shadowed\n")];

    // The pipeline says `echo`, but the resolution handed over the shadow. What runs is the
    // resolution.
    let pipeline = Pipeline::new(vec![Stage::new("echo", vec!["ignored".into()])]);
    let ran = run_resolved(&pipeline, &shadow, &scratch.path).expect("the resolved program runs");
    assert_eq!(ran.stdout.trim(), "shadowed");
}

/// A pipeline whose stages were not all resolved does not run. Spawning by name for the remainder
/// would be running something nobody resolved and nobody approved.
#[test]
fn a_pipeline_with_missing_resolutions_does_not_run() {
    let scratch = Scratch::new("unresolved");
    let pipeline = Pipeline::new(vec![
        Stage::new("echo", vec!["a".into()]),
        Stage::new("wc", vec!["-l".into()]),
    ]);
    let error = exec::run(&pipeline, &[], &scratch.path, &Cancel::new(), None)
        .expect_err("nothing runs without a resolution per stage");
    assert!(matches!(error, ExecError::Io(_)));
}

/// The case a server is: a program that prints, then keeps running. It is stopped at the limit,
/// but what it printed first is the whole account of what happened, and returning nothing left a
/// caller unable to tell a program that hung from one that was doing exactly what was asked.
#[test]
fn a_pipeline_stopped_at_the_limit_still_returns_what_it_printed() {
    let scratch = Scratch::new("stopped");
    // Prints a line, then outlasts the limit, which is the shape of `http.server` and every other
    // thing asked to serve something.
    let resolved = [script(
        &scratch.path,
        "serve",
        "#!/bin/sh\necho listening\nsleep 30\n",
    )];

    let pipeline = Pipeline::new(vec![Stage::new("serve", Vec::new())]);

    // The limit is found rather than named. What is under test is what a stop keeps, and that is
    // a different question from how long a loaded machine takes to start a process and print a
    // line: a fixed three seconds passed on an idle machine and failed during a compile, which
    // is a test reporting the load rather than the behaviour.
    //
    // A deadline that arrived before the first line comes back with nothing on stdout, and that
    // is the only outcome given more room. Every assertion below is the same whichever limit
    // produced the run, so a stop that genuinely threw the output away fails them at every limit
    // and this only spends longer arriving at the same answer.
    let mut limit = std::time::Duration::from_secs(2);
    let (ran, took) = loop {
        let started = std::time::Instant::now();
        let ran = run_within(&pipeline, &resolved, &scratch.path, limit)
            .expect("a pipeline that outstays the limit is stopped, not an error");
        let took = started.elapsed();

        if !ran.stdout.trim().is_empty() || limit >= std::time::Duration::from_secs(16) {
            break (ran, took);
        }
        limit *= 2;
    };

    assert_eq!(
        ran.stdout.trim(),
        "listening",
        "what it printed before it was killed was thrown away"
    );
    assert!(ran.stopped.is_some(), "the stop was not reported");
    assert!(
        !ran.succeeded(),
        "a pipeline that had to be killed did not succeed"
    );
    // Against the limit that ran rather than a fixed ten seconds, which was that same limit plus
    // the drain grace and room to spare. The drain is what this is watching: a run that comes
    // back long after its deadline is one that waited on a pipe somebody was still holding.
    assert!(
        took < limit + std::time::Duration::from_secs(7),
        "the drain outlived the pipeline it was draining"
    );
}

/// A pipeline that ends by itself is not marked stopped, so a caller can tell the two apart
/// without reading a byte of what was printed.
#[test]
fn a_pipeline_that_ends_by_itself_is_not_marked_stopped() {
    let scratch = Scratch::new("unstopped");
    let ran = run(
        Pipeline::new(vec![Stage::new("echo", vec!["done".into()])]),
        &scratch.path,
    )
    .expect("echo runs");
    assert!(ran.stopped.is_none());
    assert!(ran.succeeded());
}

/// A backgrounded grandchild holds the write end of the pipe after its parent is killed, so the
/// drain cannot be joined: it would wait on a process nobody is waiting for. The run must come
/// back within the grace regardless.
#[test]
fn a_grandchild_holding_the_pipe_does_not_hang_the_run() {
    let scratch = Scratch::new("grandchild");
    let resolved = [script(
        &scratch.path,
        "detach",
        "#!/bin/sh\nsleep 30 &\necho started\nsleep 30\n",
    )];

    let pipeline = Pipeline::new(vec![Stage::new("detach", Vec::new())]);
    let started = std::time::Instant::now();
    let ran = run_within(
        &pipeline,
        &resolved,
        &scratch.path,
        std::time::Duration::from_millis(400),
    )
    .expect("stopped rather than failed");
    assert!(ran.stopped.is_some());
    assert!(
        started.elapsed() < std::time::Duration::from_secs(10),
        "the run hung on a pipe a grandchild was holding open"
    );
}

// A compiled command line, end to end: whether the plan that was endorsed is the plan that runs.

/// Compile a line for `at` and run it, the way the tool will.
fn line(text: &str, at: &std::path::Path) -> exec::Ran {
    ran_and_opened(text, at).0
}

/// The same, with bytes the policy layer supplied for the first step's standard input.
///
/// `within` bounds the wait rather than leaving it at the five minutes a real run gets: a step
/// deadlocked against a pipe nobody is draining would otherwise hold the suite open for the whole
/// of it instead of failing.
fn line_fed(text: &str, at: &std::path::Path, stdin: &str, within: Duration) -> exec::Ran {
    let plan = fed_plan(text, at);
    exec::run_plan(&plan, &Cancel::new(), within, None, Some(stdin))
        .unwrap_or_else(|e| panic!("`{text}` should run: {e}"))
}

/// A compiled line whose standard input the policy layer supplies, labelled as a fetched page is.
///
/// The label is what the plan carries and the bytes are what exec is given, which is the split the
/// route rests on: the plan a person endorses says where the input came from and never what it is.
fn fed_plan(text: &str, at: &std::path::Path) -> bravebot_core::command::Plan {
    let mut plan = bravebot_agent::cmdline::compile(text, at, None, &mut |_, _| Ok(()))
        .unwrap_or_else(|e| panic!("`{text}` should compile: {e}"));
    plan.stdin = Some(bravebot_core::label::Label::untrusted_public());
    plan
}

/// The same, for a session that has a directory of its own.
fn line_given(text: &str, at: &std::path::Path, given: &std::path::Path) -> exec::Ran {
    let plan = bravebot_agent::cmdline::compile(text, at, None, &mut |_, _| Ok(()))
        .unwrap_or_else(|e| panic!("`{text}` should compile: {e}"));
    exec::run_plan(&plan, &Cancel::new(), exec::LIMIT, Some(given), None)
        .unwrap_or_else(|e| panic!("`{text}` should run: {e}"))
}

/// What a program's own `env` printed for `name`, and nothing where it printed no such line.
///
/// The whole line rather than a substring of the output: every path here shares a prefix with the
/// temporary directory the tests run in, so an assertion that a value merely appears somewhere
/// passes for a value that is one of the others with something added to it.
fn value_of(printed: &str, name: &str) -> Option<String> {
    printed
        .lines()
        .find_map(|line| line.strip_prefix(&format!("{name}=")))
        .map(str::to_string)
}

/// A directory to stand in for the one a session is given, under `at`.
fn given_directory(at: &std::path::Path) -> PathBuf {
    let path = at.join("given");
    std::fs::create_dir(&path).expect("the session's own directory");
    path
}

/// The same, keeping the destinations the run entered an effect on.
///
/// Through the notification the tool uses, so what is collected is what the trust map is decided
/// from: a list of files handed back once the line had finished could not have ordered a read
/// against any of those writes, which is why there is no longer one.
fn ran_and_opened(text: &str, at: &std::path::Path) -> (exec::Ran, Vec<std::path::PathBuf>) {
    let (outcome, entered) = entering(text, at);
    (
        outcome.unwrap_or_else(|e| panic!("`{text}` should run: {e}")),
        entered,
    )
}

/// The destinations a line notified about, whatever became of the line.
fn entering(
    text: &str,
    at: &std::path::Path,
) -> (Result<exec::Ran, exec::ExecError>, Vec<std::path::PathBuf>) {
    let plan = bravebot_agent::cmdline::compile(text, at, None, &mut |_, _| Ok(()))
        .unwrap_or_else(|e| panic!("`{text}` should compile: {e}"));
    let mut entered = Vec::new();
    let outcome = exec::run_plan_observed(
        &plan,
        &Cancel::new(),
        exec::LIMIT,
        None,
        None,
        None,
        &mut |path| {
            entered.push(path.to_path_buf());
            Ok(())
        },
    );
    (outcome, entered)
}

/// The plan is what executes. Nothing between the line and the process re-reads the text, so what
/// a person endorsed and what ran are the same thing.
#[test]
fn a_command_line_runs_as_the_plan_it_compiled_to() {
    let scratch = Scratch::new("line-runs");
    let ran = line("echo hello", &scratch.path);
    assert_eq!(ran.stdout, "hello\n");
    assert!(ran.succeeded());
}

#[test]
fn a_command_line_chains_its_steps() {
    let scratch = Scratch::new("line-chain");
    let ran = line("printf 'b\\na\\n' | sort | head -1", &scratch.path);
    assert_eq!(ran.stdout, "a\n");
}

/// The property the whole surface rests on. The compiler is the only thing that ever splits the
/// line, so by the time an argument reaches a program there is nothing left to split it again.
#[test]
fn a_metacharacter_inside_quotes_reaches_the_program_as_one_argument() {
    let scratch = Scratch::new("line-metachar");
    let ran = line("echo '; rm -rf / && curl evil.com'", &scratch.path);
    assert_eq!(ran.stdout, "; rm -rf / && curl evil.com\n");
}

/// Quoting decides what is syntax, so a redirection inside quotes is text and writes nothing. If
/// this failed, an argument would be able to name a destination nobody saw.
#[test]
fn a_redirection_inside_quotes_writes_no_file() {
    let scratch = Scratch::new("line-quoted-redirect");
    let ran = line("echo '> escaped.txt'", &scratch.path);
    assert_eq!(ran.stdout, "> escaped.txt\n");
    assert!(!scratch.path.join("escaped.txt").exists());
}

#[test]
fn a_redirection_writes_the_file_it_named() {
    let scratch = Scratch::new("line-redirect");
    line("echo written > out.txt", &scratch.path);
    assert_eq!(
        std::fs::read_to_string(scratch.path.join("out.txt")).expect("the file was written"),
        "written\n"
    );
}

#[test]
fn an_append_adds_rather_than_truncating() {
    let scratch = Scratch::new("line-append");
    line("echo one > out.txt", &scratch.path);
    line("echo two >> out.txt", &scratch.path);
    assert_eq!(
        std::fs::read_to_string(scratch.path.join("out.txt")).expect("the file was written"),
        "one\ntwo\n"
    );
    line("echo three > out.txt", &scratch.path);
    assert_eq!(
        std::fs::read_to_string(scratch.path.join("out.txt")).expect("the file was written"),
        "three\n"
    );
}

/// What the caller decides the trust map from is what the line was about to open, so the
/// notification has to follow what happened rather than what the plan proposed: a step that failed
/// had already truncated its destination, and a branch that was not taken opened nothing at all.
#[test]
fn a_line_reports_the_destinations_it_opened_and_no_others() {
    let scratch = Scratch::new("line-opened");
    std::fs::write(scratch.path.join("first.txt"), "stale\n").expect("write");
    let (ran, opened) = ran_and_opened(
        "cat no-such-file > first.txt && echo second > second.txt",
        &scratch.path,
    );

    assert!(!ran.succeeded(), "the first step was supposed to fail");
    assert_eq!(opened, [scratch.path.join("first.txt")]);
    assert_eq!(
        std::fs::read_to_string(scratch.path.join("first.txt")).expect("the file"),
        "",
        "the destination of the failing step was not truncated"
    );
    assert!(
        !scratch.path.join("second.txt").exists(),
        "the right side of an && ran after the left side failed"
    );
}

/// A destination the notification arrived for and the open then failed. Opening for writing
/// truncates, so the notification cannot wait for the open to succeed: the caller is told first and
/// is conservative about what it was told, which is what CMDLINE-7 says a failed attempt costs.
///
/// The failure is the assertion's other half. A notification after a successful open would report
/// nothing here, and the caller would go on trusting a path it had been about to write.
#[test]
fn a_destination_that_cannot_be_opened_is_still_reported() {
    let scratch = Scratch::new("line-open-fails");
    std::fs::create_dir(scratch.path.join("a-directory")).expect("mkdir");
    let (outcome, entered) = entering("echo x > a-directory", &scratch.path);

    assert!(outcome.is_err(), "a directory was opened for writing");
    assert_eq!(entered, [scratch.path.join("a-directory")]);
}

/// RUN-3's first sentence, at the layer that carries it: the planner named a reference, the policy
/// layer resolved it into bytes, and the program reads them as if they had been typed at it. This
/// is what makes `sed` over a page nobody vouched for something that can be asked for at all.
///
/// The filter is what makes the assertion mean something: an implementation that fed the whole
/// input and one that fed nothing both come back with output, and only one of them comes back with
/// the second line.
#[test]
fn bytes_supplied_for_standard_input_reach_the_first_stage() {
    let scratch = Scratch::new("fed-stdin");
    let ran = line_fed(
        "sed -n 2p",
        &scratch.path,
        "alpha\nbeta\ngamma\n",
        Duration::from_secs(30),
    );
    assert_eq!(
        ran.stdout, "beta\n",
        "the supplied bytes did not reach the program that was to filter them"
    );
    assert!(ran.succeeded());
}

/// Fed once, to the step at the head of the line, and never again. The bytes are a release that
/// happens once, so a joined line handing every part its own copy would be releasing them as many
/// times as the line has parts, and a reader of `cat && cat` expects the second `cat` to be reading
/// what it is given by the line rather than the reference over again.
#[test]
fn bytes_supplied_for_standard_input_reach_one_stage_and_no_other() {
    let scratch = Scratch::new("fed-stdin-once");
    let ran = line_fed(
        "cat && cat",
        &scratch.path,
        "once\n",
        Duration::from_secs(30),
    );
    assert_eq!(
        ran.stdout, "once\n",
        "the second part of the line was fed the reference as well as the first"
    );
    assert!(ran.succeeded());
}

/// More than a pipe holds, which is the case a synchronous write cannot serve: the reader is a
/// child this same loop has not spawned yet, so writing before the spawn fills the pipe and blocks
/// forever with nothing on the other end.
///
/// Bounded by this test rather than by the run's own deadline, because the deadline is on the
/// children and the block is in front of the first spawn: a run that never starts a process is
/// never in the loop that watches the clock. So the wait is taken here, where a deadlock is a
/// failed assertion instead of a suite that does not finish. The thread is left where it is if it
/// comes to that; nothing in the process is waiting on it.
#[test]
fn more_bytes_than_a_pipe_holds_are_fed_without_deadlocking() {
    // Well past the 64 KiB a pipe buffers on Linux, and past macOS's smaller one.
    let large = "x".repeat(1024 * 1024);
    let (finished, done) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let scratch = Scratch::new("fed-stdin-large");
        let ran = line_fed("wc -c", &scratch.path, &large, Duration::from_secs(30));
        let _ = finished.send((ran.stdout.trim().to_string(), ran.succeeded()));
    });

    // Generous against a loaded machine and far short of a deadlock, which does not end.
    let (counted, ended_well) = done
        .recv_timeout(Duration::from_secs(60))
        .expect("the run did not finish: more than a pipe holds was written before it was read");
    assert_eq!(
        counted, "1048576",
        "the program did not receive the whole of what was supplied"
    );
    assert!(ended_well);
}

/// A program that reads less than it was given is doing what it was asked to do. `head -c 2` closes
/// its end after two bytes and the write comes back `EPIPE`, which is not a failure of the run: a
/// run that reported it would report one for every line that filters.
#[test]
fn a_program_that_reads_none_of_what_it_was_fed_is_not_a_failed_run() {
    let scratch = Scratch::new("fed-stdin-epipe");
    let large = "x".repeat(1024 * 1024);
    let ran = line_fed("head -c 2", &scratch.path, &large, Duration::from_secs(30));
    assert_eq!(ran.stdout, "xx");
    assert!(
        ran.succeeded(),
        "a program that stopped reading was reported as a failed run"
    );
    assert_eq!(
        ran.stderr, "",
        "the write's own error was reported as output"
    );
}

/// The two routes RUN-4 names reach one descriptor, and honouring both is not something a run can
/// do. Refused rather than resolved, because whichever route lost would have been dropped without
/// anybody being told: a line that filtered a file while its reference went nowhere would look
/// exactly like one that had worked.
#[test]
fn a_line_naming_a_file_for_standard_input_cannot_also_be_fed_bytes() {
    let scratch = Scratch::new("fed-stdin-and-file");
    std::fs::write(scratch.path.join("from.txt"), "from the file\n").expect("write");
    let plan = fed_plan("cat < from.txt", &scratch.path);
    let outcome = exec::run_plan(
        &plan,
        &Cancel::new(),
        Duration::from_secs(30),
        None,
        Some("from the reference\n"),
    );
    assert!(
        outcome.is_err(),
        "a line was run with two sources for one descriptor: {outcome:?}"
    );
}

#[test]
fn an_input_redirection_feeds_the_first_step() {
    let scratch = Scratch::new("line-input");
    std::fs::write(scratch.path.join("in.txt"), "one\ntwo\nthree\n").expect("write");
    let ran = line("wc -l < in.txt", &scratch.path);
    assert_eq!(ran.stdout.trim(), "3");
}

/// A failing step explains itself on standard error, and a line that sends it somewhere has to
/// send it there rather than back in the result.
#[test]
fn standard_error_can_be_sent_to_its_own_file() {
    let scratch = Scratch::new("line-stderr");
    let ran = line("ls no-such-file 2> err.txt", &scratch.path);
    assert!(ran.stderr.is_empty(), "stderr went to the file, not back");
    let written = std::fs::read_to_string(scratch.path.join("err.txt")).expect("the file");
    assert!(!written.is_empty(), "the explanation reached the file");
}

#[test]
fn joining_the_streams_puts_both_in_one_place() {
    let scratch = Scratch::new("line-join");
    let ran = line("ls no-such-file 2>&1", &scratch.path);
    assert!(
        ran.stdout.contains("no-such-file"),
        "standard error joined standard output: {:?}",
        ran.stdout
    );
    assert!(ran.stderr.is_empty());
}

#[test]
fn both_streams_can_go_to_one_file() {
    let scratch = Scratch::new("line-both");
    line("ls no-such-file &> all.txt", &scratch.path);
    let written = std::fs::read_to_string(scratch.path.join("all.txt")).expect("the file");
    assert!(written.contains("no-such-file"));
}

/// A stream sent to `/dev/null` is dropped and is no file effect, so nothing is reported for the
/// trust map to reserve or mark. The stream the line did not discard still comes back, which is
/// what tells a discard from a line that lost all of its output.
#[test]
fn a_discarded_stream_is_dropped_and_opens_nothing() {
    let scratch = Scratch::new("line-discard");
    std::fs::write(scratch.path.join("kept.txt"), "").expect("write");

    let (ran, opened) = ran_and_opened("ls kept.txt no-such-file > /dev/null", &scratch.path);
    assert!(opened.is_empty(), "a discard was reported: {opened:?}");
    assert_eq!(ran.stdout, "", "standard output was not discarded");
    assert!(
        ran.stderr.contains("no-such-file"),
        "standard error went with it: {:?}",
        ran.stderr
    );

    let (ran, opened) = ran_and_opened("ls kept.txt no-such-file 2> /dev/null", &scratch.path);
    assert!(opened.is_empty(), "a discard was reported: {opened:?}");
    assert_eq!(ran.stdout, "kept.txt\n", "standard output went with it");
    assert_eq!(ran.stderr, "", "standard error was not discarded");

    for both in [
        "ls kept.txt no-such-file > /dev/null 2>&1",
        "ls kept.txt no-such-file &> /dev/null",
    ] {
        let (ran, opened) = ran_and_opened(both, &scratch.path);
        assert!(opened.is_empty(), "`{both}` reported {opened:?}");
        assert_eq!(
            (ran.stdout.as_str(), ran.stderr.as_str()),
            ("", ""),
            "`{both}`"
        );
    }
}

/// A branch is an effect a person answered for up front, and it has to run when the line says it
/// does and not otherwise.
#[test]
fn the_right_side_of_and_runs_only_when_the_left_succeeded() {
    let scratch = Scratch::new("line-and");
    assert_eq!(
        line("true && echo reached", &scratch.path).stdout,
        "reached\n"
    );
    assert_eq!(line("false && echo reached", &scratch.path).stdout, "");
}

#[test]
fn the_right_side_of_or_runs_only_when_the_left_failed() {
    let scratch = Scratch::new("line-or");
    assert_eq!(
        line("false || echo reached", &scratch.path).stdout,
        "reached\n"
    );
    assert_eq!(line("true || echo reached", &scratch.path).stdout, "");
}

#[test]
fn a_semicolon_runs_both_sides_whatever_the_first_did() {
    let scratch = Scratch::new("line-semi");
    let ran = line("false ; echo reached", &scratch.path);
    assert_eq!(ran.stdout, "reached\n");
}

/// A group sequences and starts nothing of its own, so what it holds runs in the same directory
/// with the same environment as everything else in the line.
#[test]
fn a_group_sequences_the_steps_it_holds() {
    let scratch = Scratch::new("line-group");
    let ran = line("(echo one ; echo two) && echo three", &scratch.path);
    assert_eq!(ran.stdout, "one\ntwo\nthree\n");
}

/// A line whose branches did what they were told ended well even where a step failed, and saying
/// otherwise would report a working line as a broken one.
#[test]
fn a_line_that_branched_past_a_failure_still_ended_well() {
    let scratch = Scratch::new("line-outcome");
    let ran = line("false || echo recovered", &scratch.path);
    assert!(ran.ended_well, "the line did what it was told");
    assert!(!ran.succeeded(), "a step in it still failed");
}

/// An environment written in front of one step reaches that step and no other, because there is
/// no shell between them to hold it.
#[test]
fn an_assignment_reaches_the_step_it_was_written_in_front_of() {
    let scratch = Scratch::new("line-env");
    let ran = line("BRAVEBOT_LINE_MARK=here env", &scratch.path);
    assert!(ran.stdout.contains("BRAVEBOT_LINE_MARK=here"));
    let after = line("env", &scratch.path);
    assert!(
        !after.stdout.contains("BRAVEBOT_LINE_MARK"),
        "it did not carry over to the next line"
    );
}

/// A program is told where the session's own directory is, so one that wants somewhere to write
/// finds it without a tool having been called to ask for one.
#[test]
fn a_stage_is_told_where_the_sessions_own_directory_is() {
    let scratch = Scratch::new("line-given");
    let given = given_directory(&scratch.path);

    // `env` rather than a shell expansion: the line expands nothing, and what is being asked is
    // what the process was handed.
    let ran = line_given("env", &scratch.path, &given);

    assert_eq!(
        value_of(&ran.stdout, "BRAVEBOT_SCRATCH_DIR"),
        Some(given.display().to_string()),
        "{}",
        ran.stdout
    );
}

/// A session that has no directory of its own names none. A variable holding a path that is not
/// there is worse than an absent one, which a program can test for.
#[test]
fn a_session_with_no_directory_of_its_own_names_none() {
    // Set to something rather than removed: what this process was started with is another session's
    // directory or none, so passing it on would name a directory that is not there.
    let _guard = with_env(&[("BRAVEBOT_SCRATCH_DIR", Some("/nowhere/an-outer-session"))]);
    let scratch = Scratch::new("line-not-given");

    let ran = line("env", &scratch.path);

    assert_eq!(
        value_of(&ran.stdout, "BRAVEBOT_SCRATCH_DIR"),
        None,
        "{}",
        ran.stdout
    );
}

/// An assignment on the line wins, the way the same assignment in front of a program wins in a
/// shell. What a step's own environment says is what that step is handed.
#[test]
fn a_steps_own_assignment_wins_over_the_directory_it_was_given() {
    let scratch = Scratch::new("line-given-set");
    let given = given_directory(&scratch.path);

    let ran = line_given("BRAVEBOT_SCRATCH_DIR=elsewhere env", &scratch.path, &given);

    assert_eq!(
        value_of(&ran.stdout, "BRAVEBOT_SCRATCH_DIR"),
        Some("elsewhere".to_string()),
        "{}",
        ran.stdout
    );
}

/// Where a program puts a temporary file of its own is left alone. Such a file is named in no plan
/// and read back by nothing, so a directory the map answers for is the wrong place for one.
#[test]
fn a_stage_keeps_the_temporary_directory_this_process_has() {
    let scratch = Scratch::new("line-tmpdir");
    let given = given_directory(&scratch.path);

    // Whatever this process holds, rather than a value set here: the variable decides where every
    // other test's own directory goes, so a test that rewrote it would take them with it.
    let ran = line_given("env", &scratch.path, &given);

    assert_eq!(
        value_of(&ran.stdout, "TMPDIR"),
        std::env::var("TMPDIR").ok(),
        "{}",
        ran.stdout
    );
}

/// The case the whole thing exists for. A server prints that it is listening and then keeps
/// running, so a caller that had to wait for it would wait out the limit and be handed a corpse.
#[test]
fn a_background_pipeline_reports_what_it_printed_while_it_is_still_running() {
    let scratch = Scratch::new("background-serving");
    let resolved = script(
        &scratch.path,
        "serve",
        "#!/bin/sh\necho listening\nsleep 30\n",
    );

    let pipeline = Pipeline::new(vec![Stage::new("serve", Vec::new())]);
    let started = std::time::Instant::now();
    let mut job = start(&pipeline, &[resolved], &scratch.path).expect("it starts");
    assert!(
        started.elapsed() < std::time::Duration::from_secs(5),
        "starting a background pipeline waited for it"
    );

    // What it printed is readable while it is still running, which is the point.
    let mut printed = String::new();
    for _ in 0..100 {
        printed = job.printed();
        if printed.contains("listening") {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert!(
        printed.contains("listening"),
        "nothing came back from a program that had printed: {printed:?}"
    );
    assert!(
        !job.ended(),
        "a program sleeping for 30s was reported ended"
    );
}

#[test]
fn a_background_pipeline_that_finishes_says_so_and_reports_its_code() {
    let scratch = Scratch::new("background-ends");
    let resolved = script(&scratch.path, "quick", "#!/bin/sh\necho done\nexit 3\n");

    let pipeline = Pipeline::new(vec![Stage::new("quick", Vec::new())]);
    let mut job = start(&pipeline, &[resolved], &scratch.path).expect("it starts");

    let mut ended = false;
    for _ in 0..100 {
        if job.ended() {
            ended = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert!(ended, "a program that exited was never reported as ended");
    assert_eq!(job.codes(), [Some(3)]);
    assert!(job.printed().contains("done"));
}

/// A background job outliving its turn would be an effect nobody is watching and nobody can stop.
/// The turn owns it, so dropping the handle has to end the process rather than orphan it.
#[test]
fn dropping_a_background_pipeline_kills_it() {
    let scratch = Scratch::new("background-dropped");
    // Writes a file a second after starting, so the test can tell whether it was still alive.
    let resolved = script(
        &scratch.path,
        "later",
        "#!/bin/sh\nsleep 1\ntouch survived\n",
    );

    let pipeline = Pipeline::new(vec![Stage::new("later", Vec::new())]);
    let job = start(&pipeline, &[resolved], &scratch.path).expect("it starts");
    drop(job);

    std::thread::sleep(std::time::Duration::from_millis(2500));
    assert!(
        !scratch.path.join("survived").exists(),
        "a dropped background pipeline went on running"
    );
}

#[test]
fn a_killed_background_pipeline_keeps_what_it_printed() {
    let scratch = Scratch::new("background-killed");
    let resolved = script(
        &scratch.path,
        "serve",
        "#!/bin/sh\necho listening\nsleep 30\n",
    );

    let pipeline = Pipeline::new(vec![Stage::new("serve", Vec::new())]);
    let mut job = start(&pipeline, &[resolved], &scratch.path).expect("it starts");
    for _ in 0..100 {
        if job.printed().contains("listening") {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }

    job.kill();
    assert!(
        job.printed().contains("listening"),
        "killing it threw away what it had printed"
    );
    assert!(job.ended(), "a killed pipeline was not reported as ended");
}

/// Stages are chained in the background exactly as they are in the foreground.
#[test]
fn background_stages_are_chained_so_one_feeds_the_next() {
    let scratch = Scratch::new("background-chained");
    let pipeline = Pipeline::new(vec![
        Stage::new("printf", vec!["a\nb\nc\n".into()]),
        Stage::new("wc", vec!["-l".into()]),
    ]);
    let resolved = resolve_all(&pipeline, &scratch.path).expect("both resolve");
    let mut job = start(&pipeline, &resolved, &scratch.path).expect("it starts");

    for _ in 0..100 {
        if job.ended() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert_eq!(job.printed().trim(), "3");
}

/// Compile a line for `at` and wait for it the way the tool waits for one a person may move.
fn movable(
    text: &str,
    at: &std::path::Path,
    cancel: &Cancel,
    handoff: &Handoff,
) -> Result<exec::Waited, ExecError> {
    let plan = bravebot_agent::cmdline::compile(text, at, None, &mut |_, _| Ok(()))
        .unwrap_or_else(|e| panic!("`{text}` should compile: {e}"));
    past_text_file_busy(|| exec::run_plan_movable(&plan, cancel, handoff, exec::LIMIT, None, None))
}

/// Waits for a moved job to end, so a test can read the whole of what it printed.
fn until_ended(job: &mut exec::Background) {
    let until = std::time::Instant::now() + Duration::from_secs(20);
    while !job.ended() {
        assert!(
            std::time::Instant::now() < until,
            "the moved job never ended"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// The press arrives while the wait is under way, from another thread, as it does from the
/// interface. The token is read on every pass of the wait, so the line comes back as moved long
/// before the program ends, and the same process goes on to print the rest and exit with its own
/// code: what it printed before the move is not lost to the handover.
#[test]
fn a_line_moved_part_way_keeps_running_and_keeps_all_it_printed() {
    let scratch = Scratch::new("moved-part-way");
    script(
        &scratch.path,
        "build",
        "#!/bin/sh\necho before\nsleep 10\necho after\nexit 3\n",
    );
    let handoff = Handoff::new();
    let pressed = handoff.clone();
    let press = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(300));
        pressed.request();
    });

    let waited = movable("./build", &scratch.path, &Cancel::new(), &handoff).expect("it runs");
    press.join().unwrap();

    let exec::Waited::Moved(mut moved) = waited else {
        panic!("a line asked to move was waited for to its end: {waited:?}");
    };
    assert!(
        moved.after < Duration::from_secs(8),
        "the move waited {:?}, so the press was not read while the line ran",
        moved.after
    );
    assert!(
        !moved.running.ended(),
        "a program ten seconds from its end was reported ended at the move"
    );
    until_ended(&mut moved.running);
    let printed = moved.running.printed();
    assert!(
        printed.contains("before") && printed.contains("after"),
        "the job lost part of what it printed: {printed:?}"
    );
    assert_eq!(moved.running.codes(), [Some(3)]);
}

/// A person who asked the turn to stop asked for everything in it to stop, including a command
/// they had also asked to keep. A pass that finds both tokens set kills the line.
#[test]
fn a_cancellation_wins_over_a_move_asked_for_at_the_same_time() {
    let scratch = Scratch::new("moved-and-cancelled");
    script(&scratch.path, "slow", "#!/bin/sh\nsleep 30\n");
    let cancel = Cancel::new();
    let handoff = Handoff::new();
    cancel.cancel();
    handoff.request();

    let waited = movable("./slow", &scratch.path, &cancel, &handoff);

    assert!(
        matches!(waited, Err(ExecError::Cancelled)),
        "a cancelled line was not killed: {waited:?}"
    );
}

/// A job is one pipeline whose output a reader drains. A join decides its next part by waiting
/// on the one before, and a redirection sends a stream where no job reads it, so neither shape
/// is moved however the token is set: each is waited for to its end with its output intact.
#[test]
fn a_line_with_a_join_or_a_route_is_waited_for_even_when_asked_to_move() {
    let scratch = Scratch::new("not-movable");
    script(&scratch.path, "slow", "#!/bin/sh\nsleep 1\necho slow\n");
    for (text, printed) in [
        ("./slow && echo joined", "joined"),
        ("./slow 2>&1", "slow"),
        ("./slow 2>/dev/null", "slow"),
    ] {
        let handoff = Handoff::new();
        handoff.request();

        let waited = movable(text, &scratch.path, &Cancel::new(), &handoff)
            .unwrap_or_else(|e| panic!("`{text}` should run: {e}"));

        let exec::Waited::Ran(ran) = waited else {
            panic!("`{text}` was moved: {waited:?}");
        };
        assert!(ran.succeeded(), "`{text}`: {ran:?}");
        assert!(ran.stdout.contains(printed), "`{text}`: {ran:?}");
    }
}

/// Offering the key changes nothing until it is pressed. A line nobody moved is waited for to its
/// end and handed back exactly as a line that was never offered the key.
#[test]
fn a_token_nobody_pressed_leaves_the_line_waited_for() {
    let scratch = Scratch::new("not-moved");
    script(&scratch.path, "slow", "#!/bin/sh\nsleep 1\necho done\n");

    let waited =
        movable("./slow", &scratch.path, &Cancel::new(), &Handoff::new()).expect("it runs");

    let exec::Waited::Ran(ran) = waited else {
        panic!("a line nobody moved was moved: {waited:?}");
    };
    assert!(ran.succeeded(), "{ran:?}");
    assert_eq!(ran.stdout.trim(), "done");
}

/// The credentials rule is not relaxed by moving to the background. A long-lived program is a
/// better place to read one from than a short one, so this must hold here too.
#[test]
fn a_background_pipeline_does_not_see_this_agents_credentials() {
    let _guard = with_env(&[
        ("SERVICES_KEY_AICHAT", Some("a-live-signing-key")),
        ("BRAVE_SERVICES_KEY_ID", Some("a-live-key-id")),
        // The user's own, which is deliberately kept: a name-matching filter cannot tell
        // `run aws s3 ls` from an exfiltration, so only this agent's secrets are withheld.
        ("AWS_SECRET_ACCESS_KEY", Some("the-users-own-key")),
    ]);
    let scratch = Scratch::new("background-scrubbed");

    let pipeline = Pipeline::new(vec![Stage::new("env", Vec::new())]);
    let resolved = resolve_all(&pipeline, &scratch.path).expect("env resolves");
    let mut job = start(&pipeline, &resolved, &scratch.path).expect("it starts");
    for _ in 0..100 {
        if job.ended() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }

    let printed = job.printed();
    assert!(
        !printed.contains("a-live-signing-key"),
        "the signing key reached a background program"
    );
    assert!(
        !printed.contains("a-live-key-id"),
        "the key id reached a background program"
    );
    assert!(
        printed.contains("the-users-own-key"),
        "the user's own environment did not reach a background program"
    );
}

/// A program left running is told the same directory. One that goes on printing for minutes has
/// more reason to want somewhere of its own to write than one that finishes, not less.
#[test]
fn a_background_stage_is_told_where_the_sessions_own_directory_is() {
    let scratch = Scratch::new("background-given");
    let given = given_directory(&scratch.path);

    let pipeline = Pipeline::new(vec![Stage::new("env", Vec::new())]);
    let resolved = resolve_all(&pipeline, &scratch.path).expect("env resolves");
    let mut job =
        past_text_file_busy(|| exec::start(&pipeline, &resolved, &scratch.path, Some(&given)))
            .expect("it starts");
    for _ in 0..100 {
        if job.ended() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }

    let printed = job.printed();
    assert_eq!(
        value_of(&printed, "BRAVEBOT_SCRATCH_DIR"),
        Some(given.display().to_string()),
        "{printed}"
    );
}

/// And a session with none leaves a program running with the name absent, not with what an outer
/// environment set it to.
#[test]
fn a_background_stage_of_a_session_with_no_directory_names_none() {
    let _guard = with_env(&[("BRAVEBOT_SCRATCH_DIR", Some("/nowhere/an-outer-session"))]);
    let scratch = Scratch::new("background-not-given");

    let pipeline = Pipeline::new(vec![Stage::new("env", Vec::new())]);
    let resolved = resolve_all(&pipeline, &scratch.path).expect("env resolves");
    let mut job =
        past_text_file_busy(|| start(&pipeline, &resolved, &scratch.path)).expect("it starts");
    for _ in 0..100 {
        if job.ended() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }

    let printed = job.printed();
    assert_eq!(
        value_of(&printed, "BRAVEBOT_SCRATCH_DIR"),
        None,
        "{printed}"
    );
}

#[test]
fn a_background_pipeline_with_missing_resolutions_does_not_start() {
    let scratch = Scratch::new("background-unresolved");
    let pipeline = Pipeline::new(vec![Stage::new("echo", vec!["a".into()])]);
    let error = start(&pipeline, &[], &scratch.path)
        .expect_err("nothing starts without a resolution per stage");
    assert!(matches!(error, ExecError::Io(_)));
}

/// A job reported as ended is one whose account is complete, so whoever is told it ended is told
/// everything it printed. Nothing here synchronises with the steps: a step can print and exit with
/// its output still in the pipe, unread because the thread reading it has not run yet.
///
/// The pipe held open by a step's own child is the case that makes this observable without racing
/// the scheduler. It also pins the bound: a pipe that never reaches its end does not leave a
/// pipeline whose steps have all exited reported as running forever.
#[test]
fn a_background_pipeline_reported_as_ended_has_all_of_its_output() {
    let scratch = Scratch::new("background-ended-output");
    // The step exits at once, leaving a child that prints a second later and then holds the write
    // end of the pipe, so what it printed arrives after every step has been reaped.
    let resolved = script(
        &scratch.path,
        "chatty",
        "#!/bin/sh\n(sleep 1; echo late; sleep 5) &\n",
    );

    let pipeline = Pipeline::new(vec![Stage::new("chatty", Vec::new())]);
    let mut job = start(&pipeline, &[resolved], &scratch.path).expect("it starts");

    let mut ended = false;
    for _ in 0..100 {
        if job.ended() {
            ended = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert!(
        ended,
        "a pipeline whose every step had exited was never reported as ended"
    );

    let printed = job.printed();
    assert!(
        printed.contains("late"),
        "a job reported as ended was missing what it printed: {printed:?}"
    );
}

/// Waits for the job's output to arrive, so the caller can watch a running program.
///
/// A caller with only a snapshot has to ask again to learn anything, and every ask is another whole
/// round trip. Returning as soon as something arrives is what makes one call able to answer a
/// question about a program that has not printed yet.
#[test]
fn waiting_for_more_returns_when_the_job_prints_rather_than_at_the_bound() {
    let scratch = Scratch::new("wait-for-output");
    let resolved = script(
        &scratch.path,
        "chatty",
        "#!/bin/sh\necho first\nsleep 1\necho second\nsleep 30\n",
    );

    let pipeline = Pipeline::new(vec![Stage::new("chatty", Vec::new())]);
    let mut job = start(&pipeline, &[resolved], &scratch.path).expect("it starts");
    for _ in 0..100 {
        if job.printed().contains("first") {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    // Asserted rather than assumed: where the first line has not arrived, the wait below returns on
    // it and every assertion after this reports a fault in the wait instead of a slow warm-up.
    assert!(
        job.printed().contains("first"),
        "the job had not printed its first line within five seconds, so nothing below is a test \
         of the wait"
    );

    let mut seen = exec::Seen::default();
    assert!(
        job.since(&mut seen).contains("first"),
        "the first look was not handed the first line, so nothing below is a test of the wait"
    );

    let began = std::time::Instant::now();
    job.wait_for_more(
        &seen,
        std::time::Duration::from_secs(30),
        &Cancel::new(),
        &[],
    );
    let waited = began.elapsed();

    assert!(
        waited < std::time::Duration::from_secs(15),
        "a wait sat out its bound though the job printed after a second: {waited:?}"
    );
    let printed = job.printed();
    assert!(
        printed.contains("second"),
        "the wait returned without the output it was waiting for: {printed:?}"
    );
}

/// Output that lands after the caller's last look and before the wait starts is unseen, so the wait
/// returns on it. A wait that counted what had arrived when it was entered as already seen would
/// sit out the whole bound on a job that has nothing further to print.
#[test]
fn waiting_for_more_returns_at_once_on_output_that_landed_since_the_last_look() {
    let scratch = Scratch::new(&format!("wait-landed-since-look-{}", std::process::id()));
    let looked = scratch.path.join("looked");
    let resolved = script(
        &scratch.path,
        "late",
        &format!(
            "#!/bin/sh\necho first\nwhile [ ! -f '{}' ]; do sleep 0.05; done\necho second\n\
             sleep 60\n",
            looked.display()
        ),
    );

    let pipeline = Pipeline::new(vec![Stage::new("late", Vec::new())]);
    let mut job = start(&pipeline, &[resolved], &scratch.path).expect("it starts");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while !job.printed().contains("first") && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let mut seen = exec::Seen::default();
    assert!(
        job.since(&mut seen).contains("first"),
        "the first look was not handed the first line, so nothing below is a test of the wait"
    );

    // The second line is released after the look and waited for before the wait is entered, so it
    // has arrived and has not been handed over.
    std::fs::write(&looked, "").expect("release the second line");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while !job.printed().contains("second") && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert!(
        job.printed().contains("second"),
        "the second line never arrived, so nothing below is a test of the wait"
    );

    let began = std::time::Instant::now();
    job.wait_for_more(
        &seen,
        std::time::Duration::from_secs(20),
        &Cancel::new(),
        &[],
    );
    let waited = began.elapsed();

    assert!(
        waited < std::time::Duration::from_secs(10),
        "a wait sat out its bound though output the caller had not been handed was already there: \
         {waited:?}"
    );
    assert!(
        job.since(&mut seen).contains("second"),
        "the output that landed since the last look was not handed over"
    );
}

/// Output the caller has already been handed is not something new, so it does not end a wait. A
/// wait that returned on it would report the same lines twice and answer a question about the
/// window nobody watched.
#[test]
fn waiting_for_more_lasts_its_bound_where_a_job_that_has_printed_says_nothing_further() {
    let scratch = Scratch::new("wait-out-the-bound");
    let resolved = script(
        &scratch.path,
        "quiet",
        "#!/bin/sh\necho listening\nsleep 30\n",
    );

    let pipeline = Pipeline::new(vec![Stage::new("quiet", Vec::new())]);
    let mut job = start(&pipeline, &[resolved], &scratch.path).expect("it starts");
    for _ in 0..100 {
        if job.printed().contains("listening") {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    // Asserted rather than assumed: where the line has not arrived, the wait below returns on it and
    // the assertion that it lasted its bound reports a fault in the wait instead of a slow warm-up.
    assert!(
        job.printed().contains("listening"),
        "the job had not printed within five seconds, so nothing below is a test of the wait"
    );

    let mut seen = exec::Seen::default();
    assert!(
        job.since(&mut seen).contains("listening"),
        "the first look was not handed the line, so nothing below is a test of the wait"
    );

    let bound = std::time::Duration::from_secs(2);
    let began = std::time::Instant::now();
    job.wait_for_more(&seen, bound, &Cancel::new(), &[]);
    let waited = began.elapsed();

    assert!(
        waited >= bound,
        "a wait came back early on output the caller already had: {waited:?}"
    );
    assert!(
        !job.ended(),
        "a program sleeping for 30s was reported ended"
    );
}

/// A job that has exited will never print again, so waiting on for the rest of the bound would buy
/// nothing and spend the turn the caller has.
#[test]
fn waiting_for_more_returns_when_the_job_ends_without_printing() {
    let scratch = Scratch::new("wait-until-ended");
    let resolved = script(&scratch.path, "silent", "#!/bin/sh\nsleep 1\nexit 0\n");

    let pipeline = Pipeline::new(vec![Stage::new("silent", Vec::new())]);
    let mut job = start(&pipeline, &[resolved], &scratch.path).expect("it starts");

    let began = std::time::Instant::now();
    job.wait_for_more(
        &exec::Seen::default(),
        std::time::Duration::from_secs(60),
        &Cancel::new(),
        &[],
    );
    let waited = began.elapsed();

    assert!(
        waited < std::time::Duration::from_secs(30),
        "a wait on a job that had exited ran to its bound: {waited:?}"
    );
    assert!(job.ended(), "the wait returned before the job had ended");
}

/// What arrived on one pipe is never reported as what arrived on the other.
///
/// The composed text puts standard output first, so a line arriving on it after standard error has
/// printed moves the whole of the error text further along. A single offset into that composition
/// then names a place inside text the caller was already shown: it is handed the error line a second
/// time, the line it was actually waiting for sits before the offset and is skipped, and the offset
/// moves past it for good.
#[test]
fn what_arrived_on_one_pipe_is_not_reported_as_what_arrived_on_the_other() {
    // Named for this process as well as for the test, because the handshake below is a file and a
    // scratch name is otherwise a fixed path in the temporary directory. Two runs of this binary at
    // once on one machine would share that file, and the second job would be released by the first
    // test's look rather than by its own.
    let scratch = Scratch::new(&format!("since-interleaved-{}", std::process::id()));
    let looked = scratch.path.join("looked");
    // The second line waits for the test to say it has taken its first look, rather than for a
    // second on the clock. A second is how long the first look has before the line it is supposed
    // to be too early for turns up inside it, and on a machine loaded enough to deliver the two
    // pipes a second apart that is the same machine that made the look late.
    let resolved = script(
        &scratch.path,
        "both",
        &format!(
            "#!/bin/sh\necho out1\necho err1 >&2\nwhile [ ! -f '{}' ]; do sleep 0.05; done\n\
             echo out2\nsleep 30\n",
            looked.display()
        ),
    );

    let pipeline = Pipeline::new(vec![Stage::new("both", Vec::new())]);
    let job = start(&pipeline, &[resolved], &scratch.path).expect("it starts");
    // Both lines, because both are what the first look has to be handed to be a first look at all.
    // They arrive on two pipes read by two threads, so err1 having been drained says nothing about
    // out1 having been. Bounded by the thirty seconds the wait below is given rather than by five:
    // this is the same job starting up, and what a shorter allowance measures on a machine running
    // every other test binary beside this one is the load.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while std::time::Instant::now() < deadline {
        let printed = job.printed();
        if printed.contains("out1") && printed.contains("err1") {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }

    let mut seen = exec::Seen::default();
    let first = job.since(&mut seen);
    assert!(
        first.contains("out1") && first.contains("err1"),
        "the first look was handed neither stream in full, so nothing below is a test of the \
         second: {first:?}"
    );

    std::fs::write(&looked, "").expect("release the second line");
    // Polled rather than waited for, so the line is known to be there before the look below.
    // `has_more` is that question whenever it is asked.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while !job.has_more(&seen) && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let second = job.since(&mut seen);
    assert!(
        second.contains("out2"),
        "the line that arrived on standard output was never handed over: {second:?}"
    );
    assert!(
        !second.contains("err1"),
        "a line the caller had already been shown was handed over a second time: {second:?}"
    );
}

/// A character the pipe has not finished delivering is held back, not taken lossily.
///
/// Taking it now would hand over one replacement character and move the offset past the bytes that
/// produced it, so the character on its way would never reach anybody at all. Holding it back costs
/// one more look and delivers it.
#[test]
fn a_character_split_across_two_pipe_reads_is_handed_over_whole() {
    let scratch = Scratch::new("since-split-character");
    let resolved = script(
        &scratch.path,
        "split",
        "#!/bin/sh\nprintf '\\303'\nsleep 1\nprintf '\\251 done\\n'\nsleep 30\n",
    );

    let pipeline = Pipeline::new(vec![Stage::new("split", Vec::new())]);
    let mut job = start(&pipeline, &[resolved], &scratch.path).expect("it starts");
    let mut seen = exec::Seen::default();
    for _ in 0..100 {
        if job.has_more(&seen) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert!(
        job.has_more(&seen),
        "the first byte of the character never arrived, so nothing below is a test of the hold-back"
    );

    let held = job.since(&mut seen);
    assert!(
        held.is_empty(),
        "half a character was handed over as a replacement character: {held:?}"
    );

    job.wait_for_more(
        &seen,
        std::time::Duration::from_secs(30),
        &Cancel::new(),
        &[],
    );
    let whole = job.since(&mut seen);
    assert!(
        whole.contains("\u{e9} done"),
        "the character was never handed over whole: {whole:?}"
    );
    assert!(
        !whole.contains('\u{fffd}'),
        "the character arrived as a replacement character: {whole:?}"
    );
}

/// The bound runs to ten minutes, and somebody who has changed their mind should not have to sit
/// through the rest of a wait they asked to stop. The token is checked every pass for that reason
/// rather than once at the end, and nothing inside a pass blocks.
#[test]
fn a_cancelled_wait_for_more_comes_back_without_waiting_out_its_bound() {
    let scratch = Scratch::new("wait-cancelled");
    let resolved = script(&scratch.path, "quiet", "#!/bin/sh\nsleep 30\n");

    let pipeline = Pipeline::new(vec![Stage::new("quiet", Vec::new())]);
    let mut job = start(&pipeline, &[resolved], &scratch.path).expect("it starts");

    let cancel = Cancel::new();
    cancel.cancel();
    let began = std::time::Instant::now();
    job.wait_for_more(
        &exec::Seen::default(),
        std::time::Duration::from_secs(600),
        &cancel,
        &[],
    );
    let waited = began.elapsed();

    assert!(
        waited < std::time::Duration::from_secs(5),
        "a cancelled wait went on waiting: {waited:?}"
    );
}

/// A person who asks to stop a job the turn holds is answered by the look ending, not by the rest
/// of a wait the planner asked for, whichever job the look is waiting on: the stop is carried out
/// at the turn's next step, which the wait holds back. The stop arrives part way through, from
/// another thread, the way it does from the interface.
#[test]
fn a_wait_for_more_comes_back_when_the_person_asks_to_stop_any_job_of_the_turn() {
    let scratch = Scratch::new("wait-job-stopped");
    let resolved = script(&scratch.path, "quiet", "#!/bin/sh\nsleep 30\n");

    let pipeline = Pipeline::new(vec![Stage::new("quiet", Vec::new())]);
    let mut job = start(&pipeline, &[resolved], &scratch.path).expect("it starts");

    let stop = JobStop::new();
    let asking = stop.clone();
    let asker = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(300));
        asking.request();
    });
    let began = std::time::Instant::now();
    job.wait_for_more(
        &exec::Seen::default(),
        std::time::Duration::from_secs(600),
        &Cancel::new(),
        &[JobStop::new(), stop],
    );
    let waited = began.elapsed();
    asker.join().expect("the request was made");

    assert!(
        waited < std::time::Duration::from_secs(5),
        "a wait went on waiting after the person asked to stop a job: {waited:?}"
    );
}

/// RUN-23: figures nobody named are the ones compiled in, so a checkout with no settings file runs
/// exactly as it did before either key existed.
#[test]
fn run_deadlines_nobody_named_are_the_built_in_ones() {
    assert_eq!(
        exec::Deadlines::resolve(bravebot_config::RunDeadlines {
            default: None,
            ceiling: None
        }),
        exec::Deadlines::BUILT_IN
    );
    assert_eq!(exec::Deadlines::BUILT_IN.default, exec::LIMIT);
    assert_eq!(exec::Deadlines::BUILT_IN.ceiling, exec::CEILING);
}

/// RUN-23: a default past the built-in ceiling raises it, rather than being held down to a figure
/// nobody wrote.
///
/// This is the whole motivating case: somebody whose build takes eleven minutes writes one key, and
/// a resolution that clamped the default to the built-in ceiling would hand them ten and kill the
/// build with nothing saying why.
#[test]
fn a_default_past_the_built_in_ceiling_raises_it() {
    let resolved = exec::Deadlines::resolve(bravebot_config::RunDeadlines {
        default: Some(Duration::from_secs(1200)),
        ceiling: None,
    });
    assert_eq!(resolved.default, Duration::from_secs(1200));
    assert_eq!(
        resolved.ceiling,
        Duration::from_secs(1200),
        "a default a call could not have named for itself"
    );
}

/// RUN-23: a ceiling under the built-in default lowers it, since a default no call could name is not
/// a default.
///
/// The other direction of the same rule, and the one that matters for somebody holding every run to
/// a minute: a default left at 300 there would be a figure the ceiling forbids, and every command
/// would run under a number neither the person nor the program chose.
#[test]
fn a_ceiling_under_the_built_in_default_lowers_it() {
    let resolved = exec::Deadlines::resolve(bravebot_config::RunDeadlines {
        default: None,
        ceiling: Some(Duration::from_secs(60)),
    });
    assert_eq!(resolved.ceiling, Duration::from_secs(60));
    assert_eq!(resolved.default, Duration::from_secs(60));
}

/// RUN-23: where a file names both, the ceiling it names is the bound and the default is held to it.
///
/// Both figures are that person's own words, and of the two the ceiling is the one that says what the
/// most a run may take is. A resolution that took the larger of the two instead would make the
/// ceiling unwritable in any file that also raised the default.
#[test]
fn a_file_naming_both_holds_the_default_to_the_ceiling_it_named() {
    let resolved = exec::Deadlines::resolve(bravebot_config::RunDeadlines {
        default: Some(Duration::from_secs(900)),
        ceiling: Some(Duration::from_secs(300)),
    });
    assert_eq!(resolved.ceiling, Duration::from_secs(300));
    assert_eq!(resolved.default, Duration::from_secs(300));
}

/// RUN-23: a call's own deadline is held to the ceiling in force rather than to the built-in one.
///
/// Both ends of it: a raised ceiling is a deadline a call may actually reach, and a lowered one is a
/// bound a call cannot talk its way past.
#[test]
fn a_call_is_held_to_the_ceiling_in_force() {
    let raised = exec::Deadlines::resolve(bravebot_config::RunDeadlines {
        default: None,
        ceiling: Some(Duration::from_secs(1800)),
    });
    assert_eq!(raised.held_to(1200), Duration::from_secs(1200));
    assert_eq!(raised.held_to(3600), Duration::from_secs(1800));

    let lowered = exec::Deadlines::resolve(bravebot_config::RunDeadlines {
        default: None,
        ceiling: Some(Duration::from_secs(60)),
    });
    assert_eq!(lowered.held_to(600), Duration::from_secs(60));
    assert_eq!(
        lowered.held_to(-10),
        exec::FLOOR,
        "the floor is not a figure a settings file moves"
    );
}

/// RUN-23: a ceiling too large to be a number of seconds still bounds nothing, rather than turning
/// into a bound below the floor.
///
/// `maxSeconds` is read as a `u64`, so a file may name a figure past `i64::MAX`. Converted with a
/// cast, that ceiling wraps negative, the clamp's bounds come out the wrong way round, and every
/// deadline a call names collapses to one second: every command in the session stopped before it
/// prints, from a key the person wrote meaning the opposite. Nothing warns, and the figure they
/// asked for is the one place they would not think to look.
#[test]
fn a_ceiling_too_large_to_count_in_seconds_does_not_collapse_every_deadline() {
    let absurd = exec::Deadlines::resolve(bravebot_config::RunDeadlines {
        default: None,
        ceiling: Some(Duration::from_secs(u64::MAX)),
    });
    assert_eq!(
        absurd.held_to(1200),
        Duration::from_secs(1200),
        "a deadline the call named was not granted under an unbounded ceiling"
    );
    assert_eq!(
        absurd.held_to(-10),
        exec::FLOOR,
        "the floor still holds at the bottom"
    );
}

// A program named through a link starts by the link, while the link leads to the approved file.

/// Point `name` in `at` at `target`, replacing whatever it pointed at before.
#[cfg(unix)]
fn link(at: &std::path::Path, name: &str, target: &std::path::Path) {
    let path = at.join(name);
    let _ = std::fs::remove_file(&path);
    std::os::unix::fs::symlink(target, &path).expect("make the link");
}

/// RUN-2: a program named through a link is started by the link.
///
/// A script's `$0` is the path it was started by, so what it prints is that path.
#[cfg(unix)]
#[test]
fn a_program_named_through_a_link_is_started_by_the_link() {
    let scratch = Scratch::new("started-as-link");
    let file = script(&scratch.path, "tool.sh", "#!/bin/sh\necho \"$0\"\n");
    link(&scratch.path, "link", &file);

    let plan = bravebot_agent::cmdline::compile("./link", &scratch.path, None, &mut |_, _| Ok(()))
        .expect("the line compiles");
    let ran =
        past_text_file_busy(|| exec::run_plan(&plan, &Cancel::new(), exec::LIMIT, None, None))
            .expect("the link runs");

    assert_eq!(
        ran.stdout.trim(),
        std::path::absolute(scratch.path.join("link"))
            .unwrap()
            .to_string_lossy()
    );
    assert_ne!(ran.stdout.trim(), file.to_string_lossy());
}

/// RUN-2: a link repointed after the line was compiled is refused rather than run, and the refusal
/// comes before the line opens the file it writes to.
#[cfg(unix)]
#[test]
fn a_link_pointed_elsewhere_after_approval_is_refused() {
    let scratch = Scratch::new("started-as-repointed");
    let approved = script(&scratch.path, "approved.sh", "#!/bin/sh\necho approved\n");
    let other = script(&scratch.path, "other.sh", "#!/bin/sh\ntouch other-ran\n");
    link(&scratch.path, "tool", &approved);

    let plan =
        bravebot_agent::cmdline::compile("./tool > out.txt", &scratch.path, None, &mut |_, _| {
            Ok(())
        })
        .expect("the line compiles");
    link(&scratch.path, "tool", &other);

    let refused =
        past_text_file_busy(|| exec::run_plan(&plan, &Cancel::new(), exec::LIMIT, None, None));
    assert!(
        matches!(&refused, Err(ExecError::NotStarted { program, .. }) if program == "./tool"),
        "a repointed link was not refused: {refused:?}"
    );
    assert!(
        !scratch.path.join("other-ran").exists(),
        "the file the link now leads to ran"
    );
    assert!(
        !scratch.path.join("out.txt").exists(),
        "a refused line opened its destination"
    );

    link(&scratch.path, "tool", &approved);
    past_text_file_busy(|| exec::run_plan(&plan, &Cancel::new(), exec::LIMIT, None, None))
        .expect("pointed back, the link runs");
    assert_eq!(
        std::fs::read_to_string(scratch.path.join("out.txt")).unwrap(),
        "approved\n"
    );
}

/// RUN-2: the same holds of a line left running, and pointed back it is started by the link.
#[cfg(unix)]
#[test]
fn a_background_link_pointed_elsewhere_after_approval_is_refused() {
    let scratch = Scratch::new("started-as-repointed-background");
    let approved = script(&scratch.path, "approved.sh", "#!/bin/sh\necho \"$0\"\n");
    let other = script(&scratch.path, "other.sh", "#!/bin/sh\ntouch other-ran\n");
    link(&scratch.path, "tool", &approved);

    let plan = bravebot_agent::cmdline::compile("./tool", &scratch.path, None, &mut |_, _| Ok(()))
        .expect("the line compiles");
    let bravebot_core::command::Steps::Pipeline(steps) = &plan.steps else {
        panic!("one program is one pipeline");
    };
    link(&scratch.path, "tool", &other);

    match past_text_file_busy(|| exec::start_steps(steps, &scratch.path, None, None)) {
        Err(ExecError::NotStarted { program, .. }) => assert_eq!(program, "./tool"),
        Err(other) => panic!("refused for the wrong reason: {other}"),
        Ok(_) => panic!("a repointed link was started in the background"),
    }
    assert!(
        !scratch.path.join("other-ran").exists(),
        "the file the link now leads to ran"
    );

    link(&scratch.path, "tool", &approved);
    let mut job = past_text_file_busy(|| exec::start_steps(steps, &scratch.path, None, None))
        .expect("pointed back, the line starts");
    for _ in 0..100 {
        if job.ended() {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(
        job.printed().trim(),
        std::path::absolute(scratch.path.join("tool"))
            .unwrap()
            .to_string_lossy()
    );
}
