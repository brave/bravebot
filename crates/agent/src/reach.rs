//! Reach a person gave a command, remembered so the next plan for that command asks with it.
//!
//! A grant is a step shape and a reach. The shape is the file the program resolved to and its
//! operation word; the reach is one credential scope of the closed table, or a directory the person
//! named, read by default. It is made by a person typing `/reach`, and by nothing else: no program
//! output, no planner argument and no settings file in a checkout is an input.
//!
//! A grant is for the checkout it was typed in, except a credential scope for a program the scope
//! table already gives that scope to ([`Grant::workspace`]).
//!
//! `docs/specs/sandboxing.md` (SANDBOX-23) decides what a grant means. This module is the record: how
//! a grant is spelled on disk, which are read back, and what the command that makes and removes
//! them does.
//!
//! # One line per change, appended
//!
//! `reach.jsonl` in the state directory is JSON, one object per line, appended for the reasons
//! [`crate::remembered`] gives. A line either allows a grant or revokes one, and the file is read in
//! order, so removing a grant is a line rather than a rewrite. An unreadable line, or one naming a
//! scope this build does not know, is skipped: it grants nothing. So is one that is bound to no
//! checkout where it should be.
//!
//! # Everything degrades to asking
//!
//! No state directory, an unreadable file or a failed write each mean no grant, and a plan with no
//! grant is the plan a session had before this existed. Nothing here fails a run.

use crate::granted::WrittenPath;
use crate::trusted::Identity;
use bravebot_core::command::Step;
use bravebot_i18n::t;
use bravebot_sandbox::scope::{Scope, judged_directory};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};

const FILE: &str = "reach.jsonl";

/// What a grant adds to a step's profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reached {
    /// The rows of a credential scope, as the scope brings them to a step that names it.
    Scope(Scope),
    /// A directory a person named: read, and written where the grant says so.
    Directory(PathBuf),
}

impl Reached {
    /// The reach as a person reads it: the scope's name, or the directory.
    pub fn display(&self) -> String {
        match self {
            Self::Scope(scope) => scope.name().to_string(),
            Self::Directory(path) => path.display().to_string(),
        }
    }
}

/// How long a grant applies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Lifetime {
    /// The session whose id this is, and a `--resume` of it, which keeps the id.
    Session(String),
    /// Every session, until a person removes it.
    Always,
}

/// The checkout a grant was typed in: where it is, and which directory was there.
///
/// The identity is what tells the checkout apart from another one made at the same path, as it
/// does for a kept answer to the startup question ([`crate::trusted`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Workspace {
    /// The workspace root, as the session names it.
    pub root: PathBuf,
    /// What the filesystem says about the directory at `root`.
    pub identity: Identity,
}

impl Workspace {
    /// The workspace at `root` as it is now, or `None` where the filesystem cannot tell it from
    /// another directory made at the same path.
    pub fn of(root: &Path) -> Option<Self> {
        Some(Self {
            root: root.to_path_buf(),
            identity: Identity::of(root)?,
        })
    }

    /// Whether this is the directory at `root` now.
    fn is_at(&self, root: &Path) -> bool {
        self.root == root && Identity::of(root) == Some(self.identity)
    }
}

/// Reach attached to every step of one shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grant {
    /// The file the program resolved to, never the name a line used.
    pub binary: PathBuf,
    /// The first argument when it is not an option, as [`operation_of`] reads it.
    pub operation: Option<String>,
    /// What is added.
    pub reached: Reached,
    /// Whether a directory is written as well as read. Never set for a scope.
    pub write: bool,
    /// The day a person allowed it, `YYYY-MM-DD`, for the plan to say.
    pub allowed: String,
    /// How long it applies.
    pub lifetime: Lifetime,
    /// The checkout it applies in, or `None` where it applies in every one.
    ///
    /// `None` only for a credential scope of a program the scope table gives that scope to
    /// ([`Grant::is_global`]), such as `gh` for the remote scope, whose configuration directory is
    /// the same in every project. Every other grant, a directory above all, is for the program a
    /// checkout's own files decide the behaviour of (`make`, `npm run`, `sh script.sh`), so an
    /// answer about one checkout's `make check` is not one about the next checkout's.
    pub workspace: Option<Workspace>,
}

/// The operation word of an argument vector: its first argument, unless that is an option.
///
/// An option in front of the operation (`git -C dir push`) leaves none, so a grant made for
/// `git push` does not follow it. A step with no operation is covered only when it has no arguments
/// at all ([`Grant::covers`]), and `/reach` makes no grant for one that starts with an option.
pub fn operation_of(args: &[String]) -> Option<String> {
    args.first()
        .filter(|first| !first.starts_with('-'))
        .cloned()
}

/// The parts of a stage a grant is keyed on.
struct Key<'a> {
    binary: PathBuf,
    args: &'a [String],
    assigned: bool,
}

fn key_of(step: &Step) -> Key<'_> {
    let Step {
        program: _,
        resolved,
        started_as: _,
        args,
        environment,
        routes: _,
    } = step;
    Key {
        binary: resolved.clone(),
        args,
        assigned: !environment.is_empty(),
    }
}

impl Grant {
    /// Whether a grant of `reached` to `binary` and `operation` is for every checkout.
    ///
    /// Asked of the scope table, which is the one list of the programs a scope belongs to.
    pub fn is_global(binary: &Path, operation: Option<&str>, reached: &Reached) -> bool {
        let Reached::Scope(scope) = reached else {
            return false;
        };
        let args: Vec<String> = operation.map(str::to_string).into_iter().collect();
        Scope::of(binary, &args, &[]) == Some(*scope)
    }

    /// Whether this grant is in force in the workspace at `root`.
    fn applies_in(&self, root: &Path) -> bool {
        self.workspace
            .as_ref()
            .is_none_or(|workspace| workspace.is_at(root))
    }

    /// Whether this grant attaches to `step`.
    ///
    /// A step with an assignment in front of it is covered by none, as it carries no scope: every
    /// program a scope names reads a variable that moves the directory or names a program to run.
    pub fn covers(&self, step: &Step) -> bool {
        let Step {
            program: _,
            resolved,
            started_as: _,
            args,
            environment,
            routes: _,
        } = step;
        environment.is_empty()
            && *resolved == self.binary
            && match &self.operation {
                Some(operation) => operation_of(args).as_ref() == Some(operation),
                None => args.is_empty(),
            }
    }

    /// The program and operation, as the grant is listed.
    pub fn command(&self) -> String {
        let name = self
            .binary
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.binary.display().to_string());
        match &self.operation {
            Some(operation) => format!("{name} {operation}"),
            None => name,
        }
    }

    /// Whether `other` is the same grant, whatever day it was allowed.
    fn same_as(&self, other: &Self) -> bool {
        self.binary == other.binary
            && self.operation == other.operation
            && self.reached == other.reached
            && self.write == other.write
            && self.lifetime == other.lifetime
            && self.workspace == other.workspace
    }

    /// The directory to open, judged again against `home` now. A link that has since been pointed
    /// at `~/.ssh` is refused here whatever it was when it was allowed.
    pub fn directory(&self, home: &Path) -> Option<PathBuf> {
        match &self.reached {
            Reached::Directory(path) => judged_directory(path, home),
            Reached::Scope(_) => None,
        }
    }

    fn access(&self) -> String {
        match self.write {
            true => t!(reach_access_writes).to_string(),
            false => t!(reach_access_reads).to_string(),
        }
    }

    fn lasting(&self) -> String {
        match (&self.lifetime, &self.workspace) {
            (Lifetime::Session(_), None) => t!(reach_lifetime_session).to_string(),
            (Lifetime::Always, None) => t!(reach_lifetime_always).to_string(),
            (Lifetime::Session(_), Some(workspace)) => t!(
                reach_lifetime_session_in,
                workspace = workspace.root.display().to_string()
            )
            .to_string(),
            (Lifetime::Always, Some(workspace)) => t!(
                reach_lifetime_always_in,
                workspace = workspace.root.display().to_string()
            )
            .to_string(),
        }
    }
}

/// Whether this session may add to the record, which is whether it may add to the state directory.
pub fn may_be_added_to() -> bool {
    crate::remembered::may_be_added_to()
}

/// Today's date, UTC, as the grant records it.
pub fn today() -> String {
    crate::preamble::today()
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum Action {
    Allow,
    Revoke,
}

/// One line of the record, as it is spelled on disk.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Written {
    action: Action,
    binary: String,
    operation: Option<String>,
    scope: Option<String>,
    directory: Option<String>,
    write: bool,
    allowed: String,
    session: Option<String>,
    workspace: Option<WrittenWorkspace>,
}

/// The checkout a line is bound to, as it is spelled on disk.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WrittenWorkspace {
    root: WrittenPath,
    identity: Identity,
}

impl Written {
    fn of(action: Action, grant: &Grant) -> Option<Self> {
        let (scope, directory) = match &grant.reached {
            Reached::Scope(scope) => (Some(scope.name().to_string()), None),
            Reached::Directory(path) => (None, Some(path.to_str()?.to_string())),
        };
        Some(Self {
            action,
            binary: grant.binary.to_str()?.to_string(),
            operation: grant.operation.clone(),
            scope,
            directory,
            write: grant.write,
            allowed: grant.allowed.clone(),
            session: match &grant.lifetime {
                Lifetime::Session(id) => Some(id.clone()),
                Lifetime::Always => None,
            },
            workspace: grant.workspace.as_ref().map(|workspace| WrittenWorkspace {
                root: WrittenPath::of(&workspace.root),
                identity: workspace.identity,
            }),
        })
    }

    fn into_grant(self) -> Option<(Action, Grant)> {
        let reached = match (self.scope, self.directory) {
            (Some(word), None) if !self.write => Reached::Scope(Scope::named(&word)?),
            (None, Some(path)) => {
                let path = PathBuf::from(path);
                path.is_absolute().then_some(())?;
                Reached::Directory(path)
            }
            _ => return None,
        };
        let binary = PathBuf::from(self.binary);
        let workspace = match self.workspace {
            Some(bound) => {
                let root = bound.root.to_path()?;
                root.is_absolute().then_some(())?;
                Some(Workspace {
                    root,
                    identity: bound.identity,
                })
            }
            None => {
                // A line with no checkout is for every checkout, which only the scope table may
                // say. Anything else, including a line from before grants were bound, grants
                // nothing.
                Grant::is_global(&binary, self.operation.as_deref(), &reached).then_some(())?;
                None
            }
        };
        Some((
            self.action,
            Grant {
                binary,
                operation: self.operation,
                reached,
                write: self.write,
                allowed: self.allowed,
                lifetime: match self.session {
                    Some(id) => Lifetime::Session(id),
                    None => Lifetime::Always,
                },
                workspace,
            },
        ))
    }
}

/// The record of grants, kept in the state directory.
#[derive(Debug, Clone)]
pub struct Store {
    path: PathBuf,
}

impl Store {
    /// The record inside `home`, the state directory. Never a checkout's: nothing a repository
    /// carries is read here.
    pub fn new(home: &Path) -> Self {
        Self {
            path: home.join(FILE),
        }
    }

    /// Where the record is, which is what a person removing a grant by hand needs.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The grants in force for `session` in the workspace at `root`: every `always` grant and those
    /// made in that session, less those bound to another checkout, including another directory
    /// that is now at the path a grant was made in.
    ///
    /// Read afresh on every call, so a grant removed in another session stops applying here at the
    /// next plan, and a session that moved to another checkout reads the grants of that one.
    /// `None` is a session with nobody to put a prompt to, which reads no session grant.
    pub fn read(&self, session: Option<&str>, root: &Path) -> Vec<Grant> {
        let Ok(contents) = std::fs::read_to_string(&self.path) else {
            return Vec::new();
        };
        let mut held: Vec<Grant> = Vec::new();
        for (action, grant) in contents
            .lines()
            .filter_map(|line| serde_json::from_str::<Written>(line).ok())
            .filter_map(Written::into_grant)
        {
            held.retain(|existing| !existing.same_as(&grant));
            if action == Action::Allow {
                held.push(grant);
            }
        }
        held.retain(|grant| {
            let lasts = match &grant.lifetime {
                Lifetime::Always => true,
                Lifetime::Session(id) => session == Some(id.as_str()),
            };
            lasts && grant.applies_in(root)
        });
        held
    }

    /// Add `grant`. Whether a line was written.
    pub fn allow(&self, grant: &Grant) -> bool {
        self.append(Action::Allow, grant)
    }

    /// Remove `grant`. Whether a line was written.
    pub fn revoke(&self, grant: &Grant) -> bool {
        self.append(Action::Revoke, grant)
    }

    fn append(&self, action: Action, grant: &Grant) -> bool {
        if !may_be_added_to() {
            return false;
        }
        let Some(parent) = self.path.parent() else {
            return false;
        };
        if crate::home::create_directory(parent).is_err() {
            return false;
        }
        let Some(written) = Written::of(action, grant) else {
            return false;
        };
        let Ok(mut encoded) = serde_json::to_string(&written) else {
            return false;
        };
        encoded.push('\n');
        crate::home::append_to_file(&self.path)
            .and_then(|mut file| file.write_all(encoded.as_bytes()))
            .is_ok()
    }
}

/// Where `/reach` runs: the session it was typed in and the places it resolves names against.
pub struct Typed<'a> {
    /// The state directory the record is kept in.
    pub home: &'a Path,
    /// The user's profile directory, which `~` stands for and which no grant reaches whole.
    pub profile: Option<&'a Path>,
    /// The session a `session` grant belongs to.
    pub session: &'a str,
    /// The workspace root: the command line is compiled in it, the grants listed and removed are the
    /// ones in force in it, and a grant that is not for every checkout is bound to it.
    pub directory: &'a Path,
    /// The day the grant is dated.
    pub today: &'a str,
}

/// What `/reach` does with `argument`, as the sentence to show.
///
/// With nothing it lists. With `remove <number>` it removes the numbered row of that list. Otherwise
/// the form is `<scope or directory> [write] [always] -- <command line>`, and the reach is attached
/// to every step of the line.
pub fn command(typed: &Typed<'_>, argument: &str) -> String {
    let store = Store::new(typed.home);
    let argument = argument.trim();
    if argument.is_empty() {
        return listing(&store.read(Some(typed.session), typed.directory));
    }
    if let Some(number) = argument.strip_prefix("remove") {
        return match number.trim().parse::<usize>() {
            Ok(number) => remove(&store, typed.session, typed.directory, number),
            Err(_) => t!(reach_usage).to_string(),
        };
    }
    allow(&store, typed, argument)
}

fn listing(held: &[Grant]) -> String {
    if held.is_empty() {
        return t!(reach_none).to_string();
    }
    held.iter()
        .enumerate()
        .map(|(at, grant)| {
            t!(
                reach_listed,
                number = (at + 1).to_string(),
                command = grant.command(),
                access = grant.access(),
                entry = grant.reached.display(),
                date = grant.allowed.as_str(),
                lifetime = grant.lasting()
            )
            .to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn remove(store: &Store, session: &str, root: &Path, number: usize) -> String {
    let held = store.read(Some(session), root);
    let Some(grant) = number.checked_sub(1).and_then(|at| held.get(at)) else {
        return t!(reach_refused_number, number = number.to_string()).to_string();
    };
    if !store.revoke(grant) {
        return t!(reach_refused_incognito).to_string();
    }
    t!(
        reach_removed,
        command = grant.command(),
        access = grant.access(),
        entry = grant.reached.display()
    )
    .to_string()
}

fn allow(store: &Store, typed: &Typed<'_>, argument: &str) -> String {
    let Some((before, line)) = argument.split_once(" -- ") else {
        return t!(reach_usage).to_string();
    };
    let mut words = before.split_whitespace();
    let Some(entry) = words.next() else {
        return t!(reach_usage).to_string();
    };
    let (mut write, mut always) = (false, false);
    for word in words {
        match word {
            "write" => write = true,
            "always" => always = true,
            "read" | "session" => {}
            _ => return t!(reach_usage).to_string(),
        }
    }
    if !may_be_added_to() {
        return t!(reach_refused_incognito).to_string();
    }
    let Some(profile) = typed.profile else {
        return t!(reach_refused_no_home).to_string();
    };
    let reached = match Scope::named(entry) {
        Some(scope) => Reached::Scope(scope),
        None => {
            let named = match entry.strip_prefix("~/") {
                Some(rest) => profile.join(rest),
                None => PathBuf::from(entry),
            };
            match judged_directory(&named, profile) {
                Some(path) => Reached::Directory(path),
                None => return t!(reach_refused_entry, entry = entry).to_string(),
            }
        }
    };
    if write && matches!(reached, Reached::Scope(_)) {
        return t!(reach_refused_write).to_string();
    }
    let Ok(plan) =
        crate::cmdline::compile(line, typed.directory, Some(profile), &mut |_, _| Ok(()))
    else {
        return t!(reach_refused_line).to_string();
    };
    let keys: Vec<Key> = plan.steps().into_iter().map(key_of).collect();
    if keys.is_empty() || keys.iter().any(|key| key.assigned) {
        return t!(reach_refused_assignment).to_string();
    }
    if keys
        .iter()
        .any(|key| !key.args.is_empty() && operation_of(key.args).is_none())
    {
        return t!(reach_refused_option).to_string();
    }
    let here = Workspace::of(typed.directory);
    let mut made: Vec<Grant> = Vec::new();
    for key in keys {
        let operation = operation_of(key.args);
        let workspace = match Grant::is_global(&key.binary, operation.as_deref(), &reached) {
            true => None,
            false => match &here {
                Some(here) => Some(here.clone()),
                None => return t!(reach_refused_workspace).to_string(),
            },
        };
        let grant = Grant {
            binary: key.binary,
            operation,
            reached: reached.clone(),
            write,
            allowed: typed.today.to_string(),
            lifetime: match always {
                true => Lifetime::Always,
                false => Lifetime::Session(typed.session.to_string()),
            },
            workspace,
        };
        if !made.iter().any(|held| held.same_as(&grant)) {
            made.push(grant);
        }
    }
    let mut sentences = Vec::new();
    for grant in &made {
        if !store.allow(grant) {
            return t!(reach_refused_incognito).to_string();
        }
        sentences.push(
            t!(
                reach_added,
                command = grant.command(),
                access = grant.access(),
                entry = grant.reached.display(),
                lifetime = grant.lasting()
            )
            .to_string(),
        );
    }
    sentences.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::scratch_dir;

    /// A fresh directory under `target/`, as the shell canonicalises it.
    fn fresh(name: &str) -> PathBuf {
        let path = scratch_dir(&format!("reach-{name}"));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("create scratch");
        std::fs::canonicalize(path).expect("canonical scratch")
    }

    fn step(program: &str, resolved: &str, args: &[&str]) -> Step {
        Step {
            program: program.to_string(),
            resolved: PathBuf::from(resolved),
            started_as: PathBuf::from(resolved),
            args: args.iter().map(|argument| argument.to_string()).collect(),
            environment: Vec::new(),
            routes: Vec::new(),
        }
    }

    /// A grant for every checkout, of the scope the table gives `binary` and `operation`.
    fn scope_grant(binary: &str, operation: Option<&str>, lifetime: Lifetime) -> Grant {
        let args: Vec<String> = operation.map(str::to_string).into_iter().collect();
        let scope = Scope::of(Path::new(binary), &args, &[]).expect("a program the table names");
        Grant {
            binary: PathBuf::from(binary),
            operation: operation.map(str::to_string),
            reached: Reached::Scope(scope),
            write: false,
            allowed: "2026-10-07".to_string(),
            lifetime,
            workspace: None,
        }
    }

    /// A grant of `reached` to `binary` bound to the checkout at `root`.
    fn bound_grant(binary: &str, reached: Reached, lifetime: Lifetime, root: &Path) -> Grant {
        Grant {
            binary: PathBuf::from(binary),
            operation: None,
            reached,
            write: false,
            allowed: "2026-10-07".to_string(),
            lifetime,
            workspace: Some(Workspace::of(root).expect("an identity")),
        }
    }

    /// The places `/reach` resolves against: a state directory, a profile with a `.ssh` in it and a
    /// project directory to name.
    struct Place {
        home: PathBuf,
        profile: PathBuf,
        project: PathBuf,
        /// Two checkouts of one project, to type a row in one and use it in the other.
        first: PathBuf,
        second: PathBuf,
    }

    impl Place {
        fn new(name: &str) -> Self {
            let root = fresh(name);
            let (home, profile, project) = (
                root.join("state"),
                root.join("profile"),
                root.join("profile/project"),
            );
            std::fs::create_dir_all(&home).expect("state");
            std::fs::create_dir_all(profile.join(".ssh")).expect(".ssh");
            std::fs::create_dir_all(&project).expect("project");
            let (first, second) = (root.join("profile/first"), root.join("profile/second"));
            std::fs::create_dir_all(&first).expect("first checkout");
            std::fs::create_dir_all(&second).expect("second checkout");
            Self {
                home,
                profile,
                project,
                first,
                second,
            }
        }

        fn say_in(&self, session: &str, directory: &Path, argument: &str) -> String {
            command(
                &Typed {
                    home: &self.home,
                    profile: Some(&self.profile),
                    session,
                    directory,
                    today: "2026-10-07",
                },
                argument,
            )
        }

        fn held_in(&self, session: &str, directory: &Path) -> Vec<Grant> {
            Store::new(&self.home).read(Some(session), directory)
        }

        fn say(&self, session: &str, argument: &str) -> String {
            command(
                &Typed {
                    home: &self.home,
                    profile: Some(&self.profile),
                    session,
                    directory: &self.profile,
                    today: "2026-10-07",
                },
                argument,
            )
        }

        fn held(&self, session: &str) -> Vec<Grant> {
            Store::new(&self.home).read(Some(session), &self.profile)
        }
    }

    /// A grant attaches to the file its program resolved to and to its operation word. The
    /// regressions it rejects: matching on the name the line used, so a `make` earlier on the path
    /// than the one a person allowed takes the reach; matching on the program alone, so allowing
    /// `git push` reaches `git pull`; and an option in front of the operation, which hides what the
    /// step does, still matching.
    #[test]
    fn a_grant_covers_the_file_and_the_operation_it_was_made_for() {
        let push = scope_grant("/usr/bin/git", Some("push"), Lifetime::Always);

        assert!(push.covers(&step("git", "/usr/bin/git", &["push", "origin"])));
        assert!(!push.covers(&step("git", "/usr/bin/git", &["pull"])));
        assert!(!push.covers(&step("git", "/usr/bin/git", &["-C", "dir", "push"])));
        assert!(!push.covers(&step("git", "/usr/bin/git", &[])));
        assert!(!push.covers(&step("git", "/tmp/elsewhere/git", &["push"])));
        assert!(!push.covers(&step("ls", "/bin/ls", &["push"])));

        let bare = bound_grant(
            "/usr/bin/make",
            Reached::Scope(Scope::named("aws").expect("a scope")),
            Lifetime::Always,
            &fresh("covers"),
        );
        assert!(bare.covers(&step("make", "/usr/bin/make", &[])));
        assert!(!bare.covers(&step("make", "/usr/bin/make", &["-j4"])));
        assert!(!bare.covers(&step("make", "/usr/bin/make", &["check"])));
    }

    /// A step with an assignment in front of it is covered by no grant. The regression it rejects
    /// is a variable such as `GIT_SSH_COMMAND` naming a program to run with the reach a person
    /// gave the command without it.
    #[test]
    fn an_assignment_in_front_of_a_step_removes_every_grant() {
        let grant = scope_grant("/usr/bin/git", Some("push"), Lifetime::Always);
        let mut assigned = step("git", "/usr/bin/git", &["push"]);
        assigned.environment = vec![("GIT_SSH_COMMAND".to_string(), "ssh".to_string())];

        assert!(!grant.covers(&assigned));
    }

    /// What is written is what is read back, an `always` grant reaches every session and a
    /// `session` grant only the one that made it. The regressions it rejects: a session grant read
    /// by every session, which is a durable reach nobody chose, and a record that keeps nothing.
    #[test]
    fn a_grant_is_read_back_by_the_sessions_it_was_made_for() {
        let place = Place::new("round-trip");
        let store = Store::new(&place.home);
        let always = scope_grant("/usr/bin/aws", Some("s3"), Lifetime::Always);
        let once = scope_grant("/usr/bin/kubectl", None, Lifetime::Session("one".into()));
        assert!(store.allow(&always) && store.allow(&once));

        assert_eq!(
            store.read(Some("one"), &place.profile),
            [always.clone(), once.clone()]
        );
        assert_eq!(
            store.read(Some("two"), &place.profile),
            std::slice::from_ref(&always)
        );
        assert_eq!(store.read(None, &place.profile), [always]);
    }

    /// A removed grant stays removed, and a grant allowed again after it comes back. The
    /// regressions it rejects: a revoke that matches on the date, and a record read without its
    /// order so the allow always wins.
    #[test]
    fn a_revoked_grant_is_gone_until_it_is_allowed_again() {
        let place = Place::new("revoke");
        let store = Store::new(&place.home);
        let kept = scope_grant("/usr/bin/aws", None, Lifetime::Always);
        let mut dropped = scope_grant("/usr/bin/docker", None, Lifetime::Always);
        assert!(store.allow(&kept) && store.allow(&dropped));

        dropped.allowed = "2030-01-01".to_string();
        assert!(store.revoke(&dropped));
        assert_eq!(
            store.read(None, &place.profile),
            std::slice::from_ref(&kept)
        );

        assert!(store.allow(&dropped));
        assert_eq!(store.read(None, &place.profile), [kept, dropped]);
    }

    /// A line that is not a grant this build understands grants nothing, and the lines around it
    /// still count. The regressions it rejects: a file that fails whole on one bad line, so one
    /// stray byte drops every grant, a line naming a scope this build does not know, a write to a
    /// scope or a relative directory being read as the nearest thing it can be, and a line with no
    /// checkout (every line written before grants were bound) being read as one for every checkout
    /// when its program is not one the scope table names.
    #[test]
    fn a_line_that_is_not_a_grant_grants_nothing() {
        let place = Place::new("bad-lines");
        let store = Store::new(&place.home);
        let good = scope_grant("/usr/bin/aws", None, Lifetime::Always);
        assert!(store.allow(&good));
        let before = std::fs::read_to_string(store.path()).expect("record");
        let bad = [
            "not json".to_string(),
            r#"{"action":"allow","binary":"/b","operation":null,"scope":"everything","directory":null,"write":false,"allowed":"d","session":null}"#.to_string(),
            r#"{"action":"allow","binary":"/b","operation":null,"scope":"aws","directory":null,"write":true,"allowed":"d","session":null}"#.to_string(),
            r#"{"action":"allow","binary":"/b","operation":null,"scope":null,"directory":"relative/dir","write":false,"allowed":"d","session":null}"#.to_string(),
            r#"{"action":"allow","binary":"/b","operation":null,"scope":"aws","directory":"/tmp","write":false,"allowed":"d","session":null}"#.to_string(),
            r#"{"action":"allow","binary":"/b","operation":null,"scope":"aws","directory":null,"write":false,"allowed":"d","session":null,"extra":1}"#.to_string(),
            r#"{"action":"allow","binary":"/usr/bin/make","operation":null,"scope":null,"directory":"/tmp","write":false,"allowed":"d","session":null}"#.to_string(),
            r#"{"action":"allow","binary":"/usr/bin/make","operation":null,"scope":"aws","directory":null,"write":false,"allowed":"d","session":null}"#.to_string(),
        ];
        std::fs::write(store.path(), format!("{}\n{before}", bad.join("\n"))).expect("seed");

        assert_eq!(store.read(None, &place.profile), [good]);
    }

    /// A grant made for one command is attached to that command. The regression it rejects: a grant
    /// keyed on nothing, which gives every program the reach one of them was allowed.
    #[test]
    fn a_grant_made_for_one_command_is_made_for_that_command_only() {
        let place = Place::new("one-command");

        let said = place.say("s", "aws -- cat /etc/hostname");
        let held = place.held("s");

        assert!(said.contains("cat"), "{said}");
        assert_eq!(held.len(), 1, "{held:?}");
        let cat = crate::cmdline::compile(
            "cat /etc/hostname",
            &place.profile,
            None,
            &mut |_, _| Ok(()),
        )
        .expect("compiles");
        assert!(held[0].covers(cat.steps()[0]));
        let ls = crate::cmdline::compile("ls", &place.profile, None, &mut |_, _| Ok(()))
            .expect("compiles");
        assert!(!held[0].covers(ls.steps()[0]));
        assert_eq!(
            held[0].reached,
            Reached::Scope(Scope::named("aws").unwrap())
        );
        assert_eq!(held[0].lifetime, Lifetime::Session("s".to_string()));
    }

    /// A command that starts with an option has no operation to key on, so it gets no grant, and a
    /// grant for a bare command does not follow it once it is given arguments. The regression it
    /// rejects: a grant made for `sh -c 'aws s3 ls'` attached to every `sh -c <script>` the model
    /// writes afterwards.
    #[test]
    fn a_command_that_starts_with_an_option_carries_no_grant() {
        let place = Place::new("option-first");

        let refused = place.say("s", "aws -- sh -c 'exit 1'");
        assert_eq!(refused, t!(reach_refused_option).to_string());
        assert!(place.held("s").is_empty());

        place.say("s", "aws -- ls");
        let held = place.held("s");
        assert_eq!(held.len(), 1, "{held:?}");
        let compile = |line: &str| {
            crate::cmdline::compile(line, &place.profile, None, &mut |_, _| Ok(()))
                .expect("compiles")
        };
        assert!(held[0].covers(compile("ls").steps()[0]));
        assert!(!held[0].covers(compile("ls -la").steps()[0]));
    }

    /// Every stage of a line gets the reach, once for each shape. The regression it rejects: only
    /// the first stage getting it, and a repeated stage writing the same line twice.
    #[test]
    fn a_pipeline_gets_one_grant_for_each_distinct_stage() {
        let place = Place::new("pipeline");

        place.say("s", "aws always -- ls | cat | cat");
        let held = place.held("s");

        assert_eq!(held.len(), 2, "{held:?}");
        assert!(held.iter().all(|grant| grant.lifetime == Lifetime::Always));
        assert_eq!(place.held("another").len(), 2);
    }

    /// A directory is read unless the person said write, and the grant keeps the directory they
    /// named. The regression it rejects: write as the default.
    #[test]
    fn a_directory_is_read_unless_the_person_said_write() {
        let place = Place::new("directory");
        let named = place.project.display().to_string();

        place.say("s", &format!("{named} -- ls"));
        place.say("s", &format!("{named} write -- cat"));
        let held = place.held("s");

        assert_eq!(held.len(), 2, "{held:?}");
        assert!(!held[0].write && held[1].write);
        assert_eq!(held[0].reached, Reached::Directory(place.project.clone()));
        let tilde = place.say("s", "~/project -- sh");
        assert_eq!(place.held("s").len(), 3, "{tilde}");
    }

    /// The home, what is above it, `~/.ssh`, a path that is not there and one that is not absolute
    /// are each refused, and nothing is written. The regression it rejects: the person's own words
    /// reaching the sandbox unjudged.
    #[test]
    fn a_directory_that_holds_a_key_or_does_not_exist_is_refused() {
        let place = Place::new("refused-directory");
        let above = place
            .profile
            .parent()
            .expect("a parent")
            .display()
            .to_string();
        let entries = [
            place.profile.display().to_string(),
            above,
            place.profile.join(".ssh").display().to_string(),
            "~/.ssh".to_string(),
            place.profile.join("missing").display().to_string(),
            "relative/dir".to_string(),
            format!("{}/../profile/project", place.project.display()),
        ];

        for entry in entries {
            let said = place.say("s", &format!("{entry} -- ls"));
            assert_eq!(
                said,
                t!(reach_refused_entry, entry = entry.as_str()).to_string()
            );
        }
        assert!(place.held("s").is_empty());
        assert!(!Store::new(&place.home).path().exists());
    }

    /// A directory that was fine when it was allowed is judged again when it is used. The
    /// regression it rejects: the check made once, so replacing the directory with a link to
    /// `~/.ssh` later gives a program the keys.
    #[cfg(unix)]
    #[test]
    fn a_directory_replaced_by_a_link_to_the_keys_is_refused_at_use() {
        let place = Place::new("repointed");
        let named = place.profile.join("shared");
        std::fs::create_dir(&named).expect("shared");
        place.say("s", &format!("{} -- ls", named.display()));
        let held = place.held("s");
        assert_eq!(held.len(), 1);
        assert!(held[0].directory(&place.profile).is_some());

        std::fs::remove_dir(&named).expect("remove");
        std::os::unix::fs::symlink(place.profile.join(".ssh"), &named).expect("link");

        assert_eq!(held[0].directory(&place.profile), None);
    }

    /// Write is a directory's alone, and a line with an assignment in front of a stage is refused.
    /// The regressions they reject: a write row for a credential scope, and a grant that a
    /// variable in the line would route elsewhere.
    #[test]
    fn a_scope_is_never_written_and_an_assignment_is_never_granted() {
        let place = Place::new("refused-shape");

        let write = place.say("s", "aws write -- ls");
        let assigned = place.say("s", "aws -- GIT_SSH_COMMAND=ssh ls");
        let unparsed = place.say("s", "aws -- 'ls");

        assert_eq!(write, t!(reach_refused_write));
        assert_eq!(assigned, t!(reach_refused_assignment));
        assert_eq!(unparsed, t!(reach_refused_line));
        assert!(place.held("s").is_empty());
    }

    /// A line that does not say what to do is told how, and does not write.
    #[test]
    fn a_line_without_a_command_is_told_the_usage() {
        let place = Place::new("usage");

        for argument in ["aws", "aws sideways -- ls", "-- ls", "remove", "remove two"] {
            assert_eq!(place.say("s", argument), t!(reach_usage), "{argument}");
        }
        assert!(place.held("s").is_empty());
    }

    /// The list is numbered and `remove <n>` removes the row that number names. The regressions it
    /// rejects: counting from zero, removing the wrong row, and a number past the end removing
    /// anything.
    #[test]
    fn remove_takes_away_the_row_the_list_numbers() {
        let place = Place::new("remove");
        assert_eq!(place.say("s", ""), t!(reach_none));
        place.say("s", "aws -- ls");
        place.say("s", "docker -- cat");
        let listed = place.say("s", "");
        let (first, second) = (
            listed.lines().next().expect("a row"),
            listed.lines().nth(1).expect("a second row"),
        );
        assert!(first.contains("ls") && first.contains("aws"), "{listed}");
        assert!(
            second.contains("cat") && second.contains("docker"),
            "{listed}"
        );

        assert_eq!(
            place.say("s", "remove 0"),
            t!(reach_refused_number, number = "0")
        );
        assert_eq!(
            place.say("s", "remove 3"),
            t!(reach_refused_number, number = "3")
        );
        assert_eq!(place.held("s").len(), 2);
        place.say("s", "remove 1");

        let held = place.held("s");
        assert_eq!(held.len(), 1);
        assert_eq!(
            held[0].reached,
            Reached::Scope(Scope::named("docker").unwrap())
        );
    }

    /// A grant made in another session is not listed, so not removable by number here. The
    /// regression it rejects: a list of the whole record, whose numbers a second session could use
    /// to remove the first's.
    #[test]
    fn another_sessions_grant_is_not_listed() {
        let place = Place::new("other-session");
        place.say("one", "aws -- ls");

        assert_eq!(place.say("two", ""), t!(reach_none));
        assert_eq!(
            place.say("two", "remove 1"),
            t!(reach_refused_number, number = "1")
        );
        assert_eq!(place.held("one").len(), 1);
    }

    /// With no profile directory a name cannot be judged, so nothing is granted.
    #[test]
    fn a_session_with_no_profile_grants_nothing() {
        let place = Place::new("no-profile");
        let said = command(
            &Typed {
                home: &place.home,
                profile: None,
                session: "s",
                directory: &place.profile,
                today: "2026-10-07",
            },
            "aws -- ls",
        );

        assert_eq!(said, t!(reach_refused_no_home));
        assert!(place.held("s").is_empty());
    }

    /// A directory row is for the checkout it was typed in. The regressions it rejects: a row read
    /// in every checkout, so `/cd` into another one carries the first one's directory; a row bound
    /// to a session only, so an `always` row follows the person into every project; and a session
    /// row that outlives its session.
    #[test]
    fn a_directory_row_is_in_force_only_in_the_checkout_it_was_typed_in() {
        let place = Place::new("directory-bound");
        let named = place.project.display().to_string();

        place.say_in("s", &place.first, &format!("{named} write -- ls"));
        place.say_in("s", &place.first, &format!("{named} always -- cat"));

        assert_eq!(place.held_in("s", &place.first).len(), 2);
        assert_eq!(place.held_in("s", &place.second), [], "after /cd");
        assert_eq!(
            place.held_in("new", &place.first).len(),
            1,
            "a new session: the always row only"
        );
        assert_eq!(place.held_in("new", &place.second), [], "always, elsewhere");
        let listed = place.say_in("s", &place.first, "");
        assert!(
            listed.contains(&place.first.display().to_string()),
            "{listed}"
        );
        assert_eq!(place.say_in("s", &place.second, ""), t!(reach_none));
    }

    /// A credential scope for a program the scope table gives it to is for every checkout. The
    /// regression it rejects: every row bound, so `gh` has to be allowed again in each project
    /// though its configuration directory is the same in all of them.
    #[test]
    fn a_scope_for_a_program_the_table_names_is_in_force_in_every_checkout() {
        let place = Place::new("scope-global");

        place.say_in("s", &place.first, "remote always -- git fetch");
        place.say_in("s", &place.first, "remote -- git push");

        let (here, there) = (
            place.held_in("s", &place.first),
            place.held_in("s", &place.second),
        );
        assert_eq!(here.len(), 2, "{here:?}");
        assert_eq!(here, there);
        assert!(here.iter().all(|grant| grant.workspace.is_none()));
        assert_eq!(place.held_in("new", &place.second).len(), 1);
    }

    /// A scope typed for a program the table does not name for it is bound, not global. The
    /// regression it rejects: the scope being what makes a row global, so `/reach aws -- make
    /// check` hands every checkout's `make check` the AWS credentials.
    #[test]
    fn a_scope_for_a_program_the_table_does_not_name_is_bound_to_the_checkout() {
        let place = Place::new("scope-bound");

        place.say_in("s", &place.first, "aws always -- make check");

        let here = place.held_in("s", &place.first);
        assert_eq!(here.len(), 1, "{here:?}");
        assert!(here[0].workspace.is_some());
        assert_eq!(place.held_in("s", &place.second), []);
    }

    /// A directory row typed where the workspace root has no readable identity is refused and
    /// recorded nowhere. The regression it rejects: the row being written with no checkout, which
    /// reads as a row for every checkout.
    #[test]
    fn a_directory_row_is_refused_when_the_checkout_has_no_identity() {
        let place = Place::new("no-identity");
        let named = place.project.display().to_string();
        let gone = place.profile.join("gone");

        assert_eq!(
            place.say_in("s", &gone, &format!("{named} always -- ls")),
            t!(reach_refused_workspace)
        );
        assert!(!Store::new(&place.home).path().exists());
        assert_eq!(place.held_in("s", &gone), []);
    }

    /// A row bound to a checkout is dropped when the directory at its path is another one, and a
    /// line from before rows were bound, whose program the table does not name, grants nothing.
    /// The regressions they reject: the path alone deciding, so a checkout deleted and cloned again
    /// inherits what was said of the first; and an old line read as global, which carries every old
    /// directory row into every project.
    #[test]
    fn a_row_bound_to_another_directory_at_the_same_path_is_dropped() {
        let place = Place::new("same-path");
        let named = place.project.display().to_string();
        place.say_in("s", &place.first, &format!("{named} always -- ls"));
        assert_eq!(place.held_in("s", &place.first).len(), 1);
        let store = Store::new(&place.home);
        let before = std::fs::read_to_string(store.path()).expect("record");

        for field in ["made", "inode"] {
            let mut line: serde_json::Value = serde_json::from_str(before.trim()).expect("json");
            let identity = &mut line["workspace"]["identity"][field];
            let Some(number) = identity.as_u64() else {
                continue;
            };
            *identity = (number + 1).into();
            std::fs::write(store.path(), format!("{line}\n")).expect("rewrite");

            assert_eq!(place.held_in("s", &place.first), [], "{field} differs");
        }

        let mut old: serde_json::Value = serde_json::from_str(before.trim()).expect("json");
        old.as_object_mut().expect("object").remove("workspace");
        std::fs::write(store.path(), format!("{old}\n")).expect("rewrite");
        assert_eq!(
            place.held_in("s", &place.first),
            [],
            "a line with no checkout"
        );
    }

    /// A row is removed by the number the list gives in this checkout, and only that one. The
    /// regression it rejects: a list or a removal across checkouts, whose numbers a person in one
    /// checkout would use to remove a row of another.
    #[test]
    fn remove_reaches_only_the_rows_of_this_checkout() {
        let place = Place::new("remove-bound");
        let named = place.project.display().to_string();
        place.say_in("s", &place.first, &format!("{named} -- ls"));
        place.say_in("s", &place.second, &format!("{named} -- cat"));

        assert_eq!(
            place.say_in("s", &place.first, "remove 2"),
            t!(reach_refused_number, number = "2")
        );
        place.say_in("s", &place.first, "remove 1");

        assert_eq!(place.held_in("s", &place.first), []);
        assert_eq!(place.held_in("s", &place.second).len(), 1);
    }
}
