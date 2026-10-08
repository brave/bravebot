//! Everyday workflows run under the sandbox default ([SANDBOX-21]).
//!
//! A workflow is a short list of programs a person asks a session to run, each started the way a
//! `run` stage is: under the confinement the session would give it, with a home directory that is
//! not the account's. A workflow passes when every stage works. A workflow that is expected to be
//! refused passes when its last stage is refused and the same workflow works without the sandbox,
//! so a refusal is never a file that was not there.
//!
//! The report names workflows and stages. It never carries what a program printed: the output of a
//! program is content, and it goes to a log file whose path the report gives.
//!
//! [SANDBOX-21]: ../../../docs/specs/sandboxing.md

use crate::confine::Confinement;
use bravebot_core::command::Step;
use std::ffi::OsStr;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// How long one stage may run before it is stopped and counted as failed. A cold `cargo build` is
/// the longest of them.
const LIMIT: Duration = Duration::from_secs(300);

/// Names a host environment may set that would move what a workflow reads away from its scratch
/// home, or hand it an agent socket the machine's owner left open.
const HOST_VARIABLES: &[&str] = &[
    "CARGO_HOME",
    "GH_CONFIG_DIR",
    "GIT_ASKPASS",
    "GIT_CONFIG_GLOBAL",
    "GIT_CONFIG_SYSTEM",
    "GIT_DIR",
    "GIT_WORK_TREE",
    "SSH_ASKPASS",
    "SSH_AUTH_SOCK",
    "XDG_CACHE_HOME",
    "XDG_CONFIG_HOME",
    "XDG_DATA_HOME",
];

/// What a workflow is expected to do under the sandbox.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Expect {
    /// Every stage works.
    Works,
    /// The last stage is refused: it reaches something the sandbox exists to keep from a program.
    Refused,
}

/// One program started by a workflow.
#[derive(Debug, Clone)]
pub struct Stage {
    program: String,
    args: Vec<String>,
    environment: Vec<(String, String)>,
}

impl Stage {
    /// A program and its arguments. `{session}`, `{home}` and `{beside}` in either stand for the
    /// directory the session was opened on, the scratch home and a directory the session was not
    /// opened on. A program with a `/` in it is a path from the session directory.
    pub fn new(program: &str, args: &[&str]) -> Self {
        Self {
            program: program.to_string(),
            args: args.iter().map(ToString::to_string).collect(),
            environment: Vec::new(),
        }
    }

    /// `NAME=value` written in front of the program, which is how a line removes the credential
    /// scope from the stage ([SANDBOX-16]).
    ///
    /// [SANDBOX-16]: ../../../docs/specs/sandboxing.md
    #[must_use]
    pub fn with_env(mut self, name: &str, value: &str) -> Self {
        self.environment.push((name.to_string(), value.to_string()));
        self
    }
}

/// A named list of stages with the files it starts from.
#[derive(Debug, Clone)]
pub struct Workflow {
    pub group: &'static str,
    pub name: &'static str,
    pub expect: Expect,
    files: Vec<(String, String)>,
    setup: Vec<Stage>,
    stages: Vec<Stage>,
    needs: Vec<String>,
}

impl Workflow {
    pub fn new(group: &'static str, name: &'static str) -> Self {
        Self {
            group,
            name,
            expect: Expect::Works,
            files: Vec::new(),
            setup: Vec::new(),
            stages: Vec::new(),
            needs: Vec::new(),
        }
    }

    /// A file in the session directory before anything runs. One that starts with `#!` is made
    /// executable.
    #[must_use]
    pub fn file(mut self, path: &str, contents: &str) -> Self {
        self.files.push((path.to_string(), contents.to_string()));
        self
    }

    /// A stage run before the workflow, outside the sandbox, that has to work.
    #[must_use]
    pub fn setup(mut self, stage: Stage) -> Self {
        self.setup.push(stage);
        self
    }

    #[must_use]
    pub fn stage(mut self, stage: Stage) -> Self {
        self.stages.push(stage);
        self
    }

    /// Programs a stage starts from inside a shell line. The workflow is skipped where one is not
    /// installed.
    #[must_use]
    pub fn needing(mut self, programs: &[&str]) -> Self {
        self.needs.extend(programs.iter().map(ToString::to_string));
        self
    }

    /// The last stage is expected to be refused.
    #[must_use]
    pub fn refused(mut self) -> Self {
        self.expect = Expect::Refused;
        self
    }

    fn programs(&self) -> impl Iterator<Item = &str> {
        self.setup
            .iter()
            .chain(&self.stages)
            .map(|stage| stage.program.as_str())
            .chain(self.needs.iter().map(String::as_str))
    }
}

/// Why a workflow failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    /// A stage that had to work was refused, or exited with an error under the sandbox that it does
    /// not exit with outside it.
    Refused,
    /// The last stage of a workflow expected to be refused worked.
    Allowed,
    /// The workflow fails outside the sandbox as well, so it says nothing about the sandbox.
    WithoutTheSandbox,
    /// A stage that sets the workflow up failed.
    Setup,
    /// The platform would not confine the stage.
    NotConfined(String),
}

/// The stage a workflow failed at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    pub kind: Kind,
    /// The position among the workflow's stages, from one. A setup stage counts from one as well.
    pub stage: usize,
    pub program: String,
    pub code: Option<i32>,
    /// Where the stage's output went. Nothing in the report reads it.
    pub log: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Passed,
    /// Not run, because a program it starts is not on this machine.
    Skipped(Vec<String>),
    Failed(Failure),
}

#[derive(Debug, Clone)]
pub struct Row {
    pub group: &'static str,
    pub name: &'static str,
    pub expect: Expect,
    pub outcome: Outcome,
}

#[derive(Debug, Clone, Default)]
pub struct Report {
    pub rows: Vec<Row>,
}

impl Report {
    pub fn failed(&self) -> Vec<&Row> {
        self.rows
            .iter()
            .filter(|row| matches!(row.outcome, Outcome::Failed(_)))
            .collect()
    }

    pub fn skipped(&self) -> Vec<&Row> {
        self.rows
            .iter()
            .filter(|row| matches!(row.outcome, Outcome::Skipped(_)))
            .collect()
    }

    pub fn passed(&self) -> usize {
        self.rows
            .iter()
            .filter(|row| row.outcome == Outcome::Passed)
            .count()
    }

    /// Whether no workflow failed. A workflow that was skipped did not fail.
    pub fn is_green(&self) -> bool {
        self.failed().is_empty()
    }
}

/// Why the suite could not run at all.
#[derive(Debug)]
pub enum Unavailable {
    /// This platform has no base to confine against.
    NoBase,
    /// The directory the workflows are made in is under the temporary directory, which the
    /// sandbox lets every program write, so a refused write there could not be told from a
    /// permitted one.
    Root(PathBuf),
    Io(std::io::Error),
}

impl From<std::io::Error> for Unavailable {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

/// Whether this machine can apply the confinement the workflows run under. A suite that runs
/// inside a profile of its own cannot, since the one it would apply nests inside it.
pub fn available() -> bool {
    bravebot_sandbox::base::Prelude::current().is_some()
        && bravebot_sandbox::confinement_works_here()
}

/// Run `workflows`, each in directories of its own under `root`, and say what happened to each.
///
/// `root` must not be under the temporary directory.
pub fn run(root: &Path, workflows: &[Workflow]) -> Result<Report, Unavailable> {
    std::fs::create_dir_all(root)?;
    let root = root.canonicalize()?;
    // The directory is compared against, never written to.
    // nosemgrep: rust.lang.security.temp-dir.temp-dir
    let temporary = std::env::temp_dir();
    let temporary = temporary.canonicalize().unwrap_or(temporary);
    if root.starts_with(&temporary) {
        return Err(Unavailable::Root(root));
    }
    let mut report = Report::default();
    for (index, workflow) in workflows.iter().enumerate() {
        let outcome = judge(&root, index, workflow)?;
        report.rows.push(Row {
            group: workflow.group,
            name: workflow.name,
            expect: workflow.expect,
            outcome,
        });
    }
    Ok(report)
}

fn judge(root: &Path, index: usize, workflow: &Workflow) -> Result<Outcome, Unavailable> {
    let path = std::env::var_os("PATH").unwrap_or_default();
    let missing: Vec<String> = workflow
        .programs()
        .filter(|program| !program.contains('/') && find(program, &path).is_none())
        .map(ToString::to_string)
        .collect();
    if !missing.is_empty() {
        return Ok(Outcome::Skipped(missing));
    }

    let confined = attempt(root, index, "confined", workflow, true)?;
    let last = workflow.stages.len();
    let verdict = match (workflow.expect, &confined.ended) {
        (_, Ended::NotConfined(detail)) => Some(Kind::NotConfined(detail.clone())),
        (_, Ended::Setup(_)) => Some(Kind::Setup),
        (Expect::Works, Ended::Completed) => None,
        (Expect::Works, Ended::Stopped(_)) => Some(Kind::Refused),
        (Expect::Refused, Ended::Completed) => Some(Kind::Allowed),
        (Expect::Refused, Ended::Stopped(at)) if *at == last => None,
        (Expect::Refused, Ended::Stopped(_)) => Some(Kind::Refused),
    };
    let Some(mut kind) = verdict else {
        return Ok(match workflow.expect {
            Expect::Works => Outcome::Passed,
            Expect::Refused => {
                let control = attempt(root, index, "control", workflow, false)?;
                if control.ended == Ended::Completed {
                    Outcome::Passed
                } else {
                    Outcome::Failed(control.failure(workflow, Kind::WithoutTheSandbox))
                }
            }
        });
    };
    if kind == Kind::Refused {
        let control = attempt(root, index, "control", workflow, false)?;
        if control.ended != Ended::Completed {
            kind = Kind::WithoutTheSandbox;
        }
    }
    Ok(Outcome::Failed(confined.failure(workflow, kind)))
}

#[derive(Debug, PartialEq, Eq)]
enum Ended {
    Completed,
    /// The stage, from one, that exited with an error.
    Stopped(usize),
    /// The setup stage that failed.
    Setup(usize),
    NotConfined(String),
}

struct Attempt {
    ended: Ended,
    program: String,
    stage: usize,
    code: Option<i32>,
    log: Option<PathBuf>,
}

impl Attempt {
    fn failure(&self, _workflow: &Workflow, kind: Kind) -> Failure {
        Failure {
            kind,
            stage: self.stage,
            program: self.program.clone(),
            code: self.code,
            log: self.log.clone(),
        }
    }
}

/// The directories a workflow runs in: the session, an account home that holds a credential of
/// each kind, and a directory beside the session.
struct Places {
    session: PathBuf,
    home: PathBuf,
    beside: PathBuf,
    logs: PathBuf,
}

impl Places {
    fn new(root: &Path, index: usize, label: &str) -> std::io::Result<Self> {
        let top = root.join(format!("{index:02}-{label}"));
        let _ = std::fs::remove_dir_all(&top);
        let session = top.join("session");
        let home = top.join("home");
        let beside = top.join("beside");
        let logs = top.join("logs");
        for directory in [&session, &home, &beside, &logs] {
            std::fs::create_dir_all(directory)?;
        }
        for (path, contents) in [
            (
                ".gitconfig",
                "[user]\n\tname = Usability\n\temail = usability@example.invalid\n\
                 [init]\n\tdefaultBranch = main\n[commit]\n\tgpgsign = false\n",
            ),
            (".aws/credentials", "aws secret\n"),
            (".ssh/id_ed25519", "ssh secret\n"),
            (".ssh/id_ed25519.pub", "ssh public\n"),
            (".ssh/config", "# ssh config\n"),
            (".config/gh/config.yml", "version: 1\n"),
            (
                ".config/gh/hosts.yml",
                "github.com:\n    users:\n        usability:\n            oauth_token: not-a-token\n    git_protocol: https\n    user: usability\n    oauth_token: not-a-token\n",
            ),
        ] {
            let file = home.join(path);
            if let Some(parent) = file.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(file, contents)?;
        }
        // An account that has used each ecosystem: a cache is granted where it is, and the first
        // build on an account that never ran the tool is refused it ([SANDBOX-15]).
        for directory in [".cache", ".cargo", ".npm", "Library/Caches", "go/pkg"] {
            std::fs::create_dir_all(home.join(directory))?;
        }
        Ok(Self {
            session: session.canonicalize()?,
            home: home.canonicalize()?,
            beside: beside.canonicalize()?,
            logs: logs.canonicalize()?,
        })
    }

    fn fill(&self, text: &str) -> String {
        text.replace("{session}", &self.session.to_string_lossy())
            .replace("{home}", &self.home.to_string_lossy())
            .replace("{beside}", &self.beside.to_string_lossy())
    }
}

fn attempt(
    root: &Path,
    index: usize,
    label: &str,
    workflow: &Workflow,
    confined: bool,
) -> Result<Attempt, Unavailable> {
    let places = Places::new(root, index, label)?;
    for (path, contents) in &workflow.files {
        let file = places.session.join(path);
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&file, places.fill(contents))?;
        if contents.starts_with("#!") {
            std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755))?;
        }
    }
    let confinement = if confined {
        Some(
            Confinement::here(vec![places.session.clone()], None, Some(&places.home))
                .ok_or(Unavailable::NoBase)?,
        )
    } else {
        None
    };
    let mut done = Attempt {
        ended: Ended::Completed,
        program: String::new(),
        stage: 0,
        code: None,
        log: None,
    };
    for (number, stage) in workflow.setup.iter().enumerate() {
        let log = places.logs.join(format!("setup-{}.log", number + 1));
        let ran = start(stage, &places, None, &log);
        if !matches!(ran, Ok(Some(0))) {
            done.ended = Ended::Setup(number + 1);
            done.code = ran.ok().flatten();
            done.program = stage.program.clone();
            done.stage = number + 1;
            done.log = Some(log);
            return Ok(done);
        }
    }
    for (number, stage) in workflow.stages.iter().enumerate() {
        let log = places.logs.join(format!("stage-{}.log", number + 1));
        done.program = stage.program.clone();
        done.stage = number + 1;
        done.log = Some(log.clone());
        done.code = None;
        match start(stage, &places, confinement.as_ref(), &log) {
            Ok(Some(0)) => done.code = Some(0),
            Ok(code) => {
                done.ended = Ended::Stopped(number + 1);
                done.code = code;
                return Ok(done);
            }
            Err(Started::NotConfined(detail)) => {
                done.ended = Ended::NotConfined(detail);
                return Ok(done);
            }
            Err(Started::Other) => {
                done.ended = Ended::Stopped(number + 1);
                return Ok(done);
            }
        }
    }
    Ok(done)
}

enum Started {
    NotConfined(String),
    Other,
}

/// Start one stage, wait for it, and give its exit code, or `None` when a signal ended it or it ran
/// past [`LIMIT`].
fn start(
    stage: &Stage,
    places: &Places,
    confinement: Option<&Confinement>,
    log: &Path,
) -> Result<Option<i32>, Started> {
    let path = std::env::var_os("PATH").unwrap_or_default();
    let program = places.fill(&stage.program);
    let started_as = find_from(&program, &places.session, &path).ok_or(Started::Other)?;
    let args: Vec<String> = stage.args.iter().map(|arg| places.fill(arg)).collect();
    let environment: Vec<(String, String)> = stage
        .environment
        .iter()
        .map(|(name, value)| (name.clone(), places.fill(value)))
        .collect();
    let step = Step {
        program: stage.program.clone(),
        resolved: started_as.canonicalize().map_err(|_| Started::Other)?,
        started_as: started_as.clone(),
        args: args.clone(),
        environment: environment.clone(),
        routes: Vec::new(),
    };

    let mut command = Command::new(&started_as);
    command.args(&args).current_dir(&places.session);
    for name in HOST_VARIABLES {
        command.env_remove(name);
    }
    command
        .env("HOME", &places.home)
        .env("GOTOOLCHAIN", "local")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_EDITOR", "true")
        .env("GIT_PAGER", "cat")
        .env("PAGER", "cat")
        .env("NO_COLOR", "1");
    // The rustup proxy finds its toolchains from the account's home, which is not this one.
    if std::env::var_os("RUSTUP_HOME").is_none()
        && let Some(real) = std::env::var_os("HOME")
    {
        command.env("RUSTUP_HOME", Path::new(&real).join(".rustup"));
    }
    for (name, value) in &environment {
        command.env(name, value);
    }
    crate::scrub::apply(&mut command);

    let mut command = match confinement {
        Some(confinement) => confinement
            .wrap(command, &step, &places.session)
            .map_err(|error| Started::NotConfined(error.to_string()))?,
        None => command,
    };
    let output = std::fs::File::create(log).map_err(|_| Started::Other)?;
    let errors = output.try_clone().map_err(|_| Started::Other)?;
    command.stdin(Stdio::null()).stdout(output).stderr(errors);
    let mut child = command.spawn().map_err(|_| Started::Other)?;
    let deadline = Instant::now() + LIMIT;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status.code()),
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(20));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Ok(None);
            }
        }
    }
}

fn find_from(program: &str, session: &Path, path: &OsStr) -> Option<PathBuf> {
    if program.contains('/') {
        let candidate = Path::new(program);
        let candidate = if candidate.is_absolute() {
            candidate.to_path_buf()
        } else {
            session.join(candidate)
        };
        return executable(&candidate).then_some(candidate);
    }
    find(program, path)
}

fn find(program: &str, path: &OsStr) -> Option<PathBuf> {
    std::env::split_paths(path)
        .map(|directory| directory.join(program))
        .find(|candidate| executable(candidate))
}

fn executable(path: &Path) -> bool {
    path.metadata()
        .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
}

fn git(args: &[&str]) -> Stage {
    Stage::new("git", args)
}

fn shell(script: &str) -> Stage {
    Stage::new("sh", &["-c", script])
}

/// A repository `work` with one commit, made outside the sandbox.
fn with_a_repository(workflow: Workflow) -> Workflow {
    workflow
        .file("work/a.txt", "one\n")
        .setup(git(&["init", "-q", "work"]))
        .setup(git(&["-C", "work", "add", "a.txt"]))
        .setup(git(&["-C", "work", "commit", "-q", "-m", "first"]))
}

/// Every workflow the suite runs.
pub fn workflows() -> Vec<Workflow> {
    let mut all = Vec::new();
    all.extend(git_workflows());
    all.extend(script_workflows());
    all.extend(make_workflows());
    all.extend(rust_workflows());
    all.extend(gh_workflows());
    all.extend(node_workflows());
    all.extend(python_workflows());
    all.extend(go_workflows());
    all.extend(search_and_edit_workflows());
    all.extend(refused_workflows());
    all
}

fn git_workflows() -> Vec<Workflow> {
    vec![
        Workflow::new("git", "init, add and commit")
            .file("work/a.txt", "one\n")
            .stage(git(&["init", "-q", "work"]))
            .stage(git(&["-C", "work", "add", "a.txt"]))
            .stage(git(&["-C", "work", "commit", "-q", "-m", "first"]))
            .stage(git(&["-C", "work", "log", "--oneline"])),
        with_a_repository(Workflow::new("git", "branch, edit, diff and merge"))
            .stage(git(&["-C", "work", "checkout", "-q", "-b", "topic"]))
            .stage(shell("echo two >> work/a.txt"))
            .stage(git(&["-C", "work", "diff", "--stat"]))
            .stage(git(&["-C", "work", "status", "--short"]))
            .stage(git(&["-C", "work", "commit", "-q", "-a", "-m", "topic"]))
            .stage(git(&["-C", "work", "checkout", "-q", "main"]))
            .stage(git(&["-C", "work", "merge", "-q", "--no-edit", "topic"]))
            .stage(git(&["-C", "work", "branch", "-q", "-d", "topic"])),
        with_a_repository(Workflow::new("git", "stash, tag and rebase"))
            .stage(shell("echo two >> work/a.txt"))
            .stage(git(&["-C", "work", "stash", "-q"]))
            .stage(git(&["-C", "work", "stash", "pop", "-q"]))
            .stage(git(&["-C", "work", "tag", "v1"]))
            .stage(git(&["-C", "work", "checkout", "-q", "-b", "topic"]))
            .stage(shell("echo t > work/t.txt"))
            .stage(git(&["-C", "work", "add", "t.txt"]))
            .stage(git(&["-C", "work", "commit", "-q", "-a", "-m", "topic"]))
            .stage(git(&["-C", "work", "checkout", "-q", "main"]))
            .stage(shell("echo m > work/m.txt"))
            .stage(git(&["-C", "work", "add", "m.txt"]))
            .stage(git(&["-C", "work", "commit", "-q", "-m", "main"]))
            .stage(git(&["-C", "work", "rebase", "-q", "main", "topic"])),
        with_a_repository(Workflow::new("git", "worktree"))
            .stage(git(&[
                "-C", "work", "worktree", "add", "-q", "../tree", "-b", "wt",
            ]))
            .stage(git(&["-C", "tree", "log", "--oneline"]))
            .stage(git(&["-C", "work", "worktree", "remove", "../tree"])),
        with_a_repository(Workflow::new("git", "commit with a hook"))
            .setup(shell(
                "printf '#!/bin/sh\\nexec git diff --cached --check\\n' \
                 > work/.git/hooks/pre-commit && chmod +x work/.git/hooks/pre-commit",
            ))
            .stage(shell("echo two >> work/a.txt"))
            .stage(git(&["-C", "work", "commit", "-q", "-a", "-m", "second"])),
        with_a_repository(Workflow::new("git", "push, clone, fetch and pull"))
            .setup(git(&["init", "-q", "--bare", "remote.git"]))
            .setup(git(&[
                "-C",
                "work",
                "remote",
                "add",
                "origin",
                "{session}/remote.git",
            ]))
            .stage(git(&["-C", "work", "push", "-q", "-u", "origin", "main"]))
            .stage(git(&["clone", "-q", "remote.git", "other"]))
            .stage(shell("echo two >> other/a.txt"))
            .stage(git(&["-C", "other", "commit", "-q", "-a", "-m", "second"]))
            .stage(git(&["-C", "other", "push", "-q"]))
            .stage(git(&["-C", "work", "fetch", "-q"]))
            .stage(git(&["-C", "work", "pull", "-q"]))
            .stage(git(&["-C", "work", "ls-remote", "origin"])),
    ]
}

fn script_workflows() -> Vec<Workflow> {
    vec![with_a_repository(Workflow::new(
        "script",
        "a wrapper script that runs git and make",
    ))
    .file(
        "tools/build.sh",
        "#!/bin/sh\nset -e\ngit -C work rev-parse --short HEAD > build.id\nmake -s all\ncat out/a.txt\n",
    )
    .file(
        "Makefile",
        "all:\n\tmkdir -p out\n\techo a > out/a.txt\n",
    )
    .needing(&["git", "make"])
    .stage(Stage::new("./tools/build.sh", &[]))]
}

fn make_workflows() -> Vec<Workflow> {
    vec![
        Workflow::new("make", "targets, a recursive make and clean")
            .file(
                "Makefile",
                "all: out/a.txt out/b.txt\n\
                 out/%.txt:\n\t@mkdir -p out\n\t@echo $* > $@\n\
                 sub:\n\t$(MAKE) -s -C inner hello\n\
                 clean:\n\trm -rf out\n",
            )
            .file("inner/Makefile", "hello:\n\t@echo hello > hello.txt\n")
            .stage(Stage::new("make", &["-s", "all"]))
            .stage(Stage::new("make", &["-s", "-j2", "sub"]))
            .stage(Stage::new("make", &["-s", "clean"])),
    ]
}

fn rust_workflows() -> Vec<Workflow> {
    // Its own workspace, since the directory the suite runs in may be inside one.
    let package =
        "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2021\"\n[workspace]\n";
    let manifest = "--manifest-path=demo/Cargo.toml";
    vec![
        Workflow::new("rust", "check, build, run and test")
            .file("demo/Cargo.toml", package)
            .file(
                "demo/src/main.rs",
                "fn main() { println!(\"{}\", 1 + 1); }\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn adds() { assert_eq!(1 + 1, 2); }\n}\n",
            )
            .stage(Stage::new("cargo", &["check", "-q", "--offline", manifest]))
            .stage(Stage::new("cargo", &["build", "-q", "--offline", manifest]))
            .stage(Stage::new("cargo", &["run", "-q", "--offline", manifest]))
            .stage(Stage::new("cargo", &["test", "-q", "--offline", manifest])),
        Workflow::new("rust", "a build script")
            .file("demo/Cargo.toml", package)
            .file(
                "demo/build.rs",
                "fn main() {\n    let out = std::env::var(\"OUT_DIR\").unwrap();\n    \
                 std::fs::write(std::path::Path::new(&out).join(\"made.rs\"), \"pub const N: u32 = 1;\").unwrap();\n}\n",
            )
            .file(
                "demo/src/main.rs",
                "include!(concat!(env!(\"OUT_DIR\"), \"/made.rs\"));\nfn main() { println!(\"{}\", N); }\n",
            )
            .stage(Stage::new("cargo", &["build", "-q", "--offline", manifest])),
    ]
}

fn gh_workflows() -> Vec<Workflow> {
    vec![
        // No network, and nothing that prints a login: the host keychain answers `gh auth token`
        // ahead of the file in the scratch home.
        Workflow::new("gh", "version and configuration")
            .stage(Stage::new("gh", &["--version"]))
            .stage(Stage::new("gh", &["config", "get", "git_protocol"]))
            .stage(Stage::new("gh", &["alias", "list"])),
    ]
}

fn node_workflows() -> Vec<Workflow> {
    vec![
        Workflow::new("node", "init, run a script and test")
            .file("app/index.js", "console.log(1 + 1);\n")
            .stage(Stage::new("node", &["--version"]))
            .stage(shell("cd app && npm init -y > /dev/null"))
            .stage(shell(
                "cd app && npm pkg set scripts.start='node index.js' scripts.test='node index.js'",
            ))
            .stage(shell("cd app && npm run -s start"))
            .stage(shell("cd app && npm test --silent"))
            .needing(&["node", "npm"]),
        Workflow::new("node", "install a package from a tarball")
            .file(
                "lib/package.json",
                "{\"name\":\"lib\",\"version\":\"1.0.0\",\"main\":\"index.js\"}\n",
            )
            .file("lib/index.js", "module.exports = 2;\n")
            .file(
                "app/package.json",
                "{\"name\":\"app\",\"version\":\"1.0.0\",\"private\":true}\n",
            )
            .stage(shell("cd lib && npm pack --silent > /dev/null"))
            .stage(shell(
                "cd app && npm install --silent --no-audit --no-fund ../lib/lib-1.0.0.tgz",
            ))
            .stage(shell(
                "cd app && node -e 'if (require(\"lib\") !== 2) process.exit(1)'",
            ))
            .needing(&["node", "npm"]),
    ]
}

fn python_workflows() -> Vec<Workflow> {
    vec![
        Workflow::new("python", "run a script and its tests")
            .file("app/tool.py", "def double(n):\n    return n * 2\n")
            .file(
                "app/test_tool.py",
                "import unittest\nimport tool\n\nclass T(unittest.TestCase):\n    def test_double(self):\n        self.assertEqual(tool.double(2), 4)\n\nif __name__ == '__main__':\n    unittest.main()\n",
            )
            .stage(Stage::new("python3", &["-c", "print(1 + 1)"]))
            .stage(shell("cd app && python3 -m unittest -q")),
        Workflow::new("python", "a virtual environment")
            .stage(Stage::new("python3", &["-m", "venv", ".venv"]))
            .stage(Stage::new(".venv/bin/python", &["-c", "import sys; sys.exit(0)"])),
    ]
}

fn go_workflows() -> Vec<Workflow> {
    vec![Workflow::new("go", "build, vet, run and test")
        .file("app/go.mod", "module example.invalid/app\n\ngo 1.18\n")
        .file(
            "app/main.go",
            "package main\n\nimport \"fmt\"\n\nfunc double(n int) int { return n * 2 }\n\nfunc main() { fmt.Println(double(2)) }\n",
        )
        .file(
            "app/main_test.go",
            "package main\n\nimport \"testing\"\n\nfunc TestDouble(t *testing.T) {\n\tif double(2) != 4 {\n\t\tt.Fatal(\"no\")\n\t}\n}\n",
        )
        .stage(shell("cd app && go build ./..."))
        .stage(shell("cd app && go vet ./..."))
        .stage(shell("cd app && go run ."))
        .stage(shell("cd app && go test ./..."))
        .needing(&["go"])]
}

fn search_and_edit_workflows() -> Vec<Workflow> {
    vec![
        Workflow::new("search", "find, grep, sed, awk, diff and patch")
            .file("tree/a.txt", "one\ntwo\nthree\n")
            .file("tree/sub/b.txt", "two\n")
            .stage(Stage::new("find", &["tree", "-name", "*.txt"]))
            .stage(Stage::new("grep", &["-rn", "two", "tree"]))
            .stage(shell("sed -e 's/two/2/' tree/a.txt > tree/a.new"))
            .stage(shell("awk '{ print NR, $0 }' tree/a.new > tree/a.numbered"))
            .stage(shell(
                "diff -u tree/a.txt tree/a.new > change.patch; test $? -eq 1",
            ))
            .stage(shell("cd tree && patch -s a.txt < ../change.patch"))
            .needing(&["sed", "awk", "diff", "patch"]),
        Workflow::new("editor", "vim edits a file and less shows it")
            .file("note.txt", "one\n")
            .stage(Stage::new(
                "vim",
                &[
                    "-es",
                    "-u",
                    "NONE",
                    "-n",
                    "-c",
                    "%s/one/two/",
                    "-c",
                    "wq",
                    "note.txt",
                ],
            ))
            .stage(shell("grep -q two note.txt"))
            .stage(shell(
                "less -F note.txt > shown.txt && grep -q two shown.txt",
            ))
            .needing(&["grep", "less"]),
    ]
}

fn refused_workflows() -> Vec<Workflow> {
    vec![
        Workflow::new("refused", "read a private key")
            .stage(Stage::new("cat", &["{home}/.ssh/id_ed25519"]))
            .refused(),
        Workflow::new("refused", "read a cloud credential")
            .stage(Stage::new("cat", &["{home}/.aws/credentials"]))
            .refused(),
        Workflow::new("refused", "write outside the session")
            .stage(Stage::new("touch", &["{beside}/planted.txt"]))
            .refused(),
        Workflow::new("refused", "write to the home directory")
            .stage(Stage::new("touch", &["{home}/.bashrc"]))
            .refused(),
    ]
}
