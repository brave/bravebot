//! Whether a newer bravebot has been published, and the command that takes it.
//!
//! No model, no turn, and no token: a version number, asked of the registry this copy was
//! installed from and compared against the one compiled in.
//!
//! # Startup waits for none of it
//!
//! The line shown at startup comes out of `~/.bravebot`, where an earlier launch wrote the answer
//! down. Asking again happens on a thread nothing joins, at most once a day, and what it learns is
//! for the next launch. Somebody opening a session on a train waits for nothing and, with no
//! answer on disk yet, is told nothing.
//!
//! # Only an installation there is a command for
//!
//! Two ways of installing bravebot have an update command this program can name: the npm package,
//! whose launcher says which it is, and the install script, which writes down where it put the
//! binary. Everything else, a build from source above all, is left alone. Telling somebody who
//! compiled this that a release exists, without a command that would install it, is noise.
//!
//! # What the fetched number decides
//!
//! Whether one line is printed, and nothing else. It reaches no request field, no turn and no
//! planner's context. The version in that line is composed from the three numbers parsed out of
//! the answer rather than from the answer's own bytes, so nothing a registry says is quoted back
//! onto somebody's screen.

use bravebot_core::capability::{Capability, CapabilitySet};
use bravebot_core::label::Label;
use bravebot_core::policy::{Policy, ReleasePlan, Routing};
use bravebot_i18n::t;
use bravebot_net::{Egress, Request};
use std::fmt;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use bravebot_session::audit::Trail;

/// How this copy was installed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Install {
    /// The npm package. Its launcher runs this binary and says so.
    Npm,
    /// The install script, which recorded where it put the binary.
    Script,
}

/// What the npm launcher sets [`bravebot_config::env_var::INSTALLED_VIA`] to.
const NPM: &str = "npm";

/// The registry entry for the npm package, which answers with the published version.
const NPM_URL: &str = "https://registry.npmjs.org/@brave/bravebot/latest";

/// The newest release of this repository, which answers with the tag it was published under.
const RELEASES_URL: &str = "https://api.github.com/repos/brave/bravebot/releases/latest";

/// What updates an npm install.
const NPM_COMMAND: &str = "npm install -g @brave/bravebot@latest";

/// What updates a script install: the same line that installed it, which takes the newest release
/// and puts it where this one already is.
const SCRIPT_COMMAND: &str =
    "curl -fsSL https://raw.githubusercontent.com/brave/bravebot/main/install.sh | sh";

/// The file the install script writes the installed path into.
const INSTALLED_BY_FILE: &str = "installed-by";

/// Where the last ask of each registry is recorded.
const CACHE_FILE: &str = "update-check";

/// Written here and renamed, so an interrupted write cannot leave half a line to be read back.
const CACHE_TEMPORARY: &str = "update-check.tmp";

/// How long an ask stands before it is worth making another.
///
/// Releases happen on the order of days, and the cost of a stale answer is being told about an
/// update one launch later than it existed. The cost of asking on every launch is a request per
/// launch to somebody else's registry, for a number that will not have changed.
const GOOD_FOR: u64 = 24 * 60 * 60;

impl Install {
    /// The command that updates a copy installed this way.
    ///
    /// A literal per installation, never composed: this is a line somebody pastes into a shell,
    /// so no part of it comes from a file, an environment variable or a response.
    pub fn update_command(self) -> &'static str {
        match self {
            Self::Npm => NPM_COMMAND,
            Self::Script => SCRIPT_COMMAND,
        }
    }

    /// Who is asked what the newest version is.
    fn url(self) -> &'static str {
        match self {
            Self::Npm => NPM_URL,
            Self::Script => RELEASES_URL,
        }
    }

    /// The field of that answer holding the version.
    fn field(self) -> &'static str {
        match self {
            Self::Npm => "version",
            Self::Script => "tag_name",
        }
    }

    /// What a stored answer records about where it came from.
    ///
    /// Kept in the file so an answer from one registry is not read as an answer from the other on
    /// a machine where both installations have existed.
    fn source(self) -> &'static str {
        match self {
            Self::Npm => "npm",
            Self::Script => "releases",
        }
    }

    /// The other way of installing, whose record shares the file with this one's.
    fn other(self) -> Self {
        match self {
            Self::Npm => Self::Script,
            Self::Script => Self::Npm,
        }
    }
}

/// A published version, as three numbers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    major: u64,
    minor: u64,
    patch: u64,
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// One registry as it was last asked: when the ask was made, and what it learned.
///
/// `latest` is the version the last ask that learned one named, not the highest ever seen: a
/// registry that answers with something older has withdrawn a release, and the line says nothing
/// rather than going on offering it. It is `None` until an ask learns a version at all, since a
/// registry that refuses, one that cannot be reached, and an answer that is not three numbers all
/// leave the stamp standing without one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Record {
    at: u64,
    latest: Option<Version>,
}

/// The line to say at startup, having started the ask that answers the next launch.
///
/// Nothing waits on that ask. Whatever is said here was already on disk when the process started.
pub fn at_startup() -> Option<String> {
    let install = installed_how()?;
    let stored = stored_record(install);
    refresh(install, stored);
    line(install, running_version()?, stored?.latest?)
}

/// What to say about a version that is out, or nothing when this copy is not behind it.
///
/// Separated from the files and the clock so what is said can be tested without either.
fn line(install: Install, running: Version, latest: Version) -> Option<String> {
    (latest > running).then(|| {
        t!(
            update_available,
            version = latest,
            running = running,
            command = install.update_command()
        )
    })
}

/// The version this binary was built as.
fn running_version() -> Option<Version> {
    parse_version(env!("CARGO_PKG_VERSION"))
}

/// How this copy was installed, or `None` for one nothing here can offer a command for.
fn installed_how() -> Option<Install> {
    method(
        std::env::var(bravebot_config::env_var::INSTALLED_VIA)
            .ok()
            .as_deref(),
        // Compared against the known install locations to report how bravebot was installed.
        // A wrong answer downgrades to "unknown"; nothing is granted on the strength of it.
        // nosemgrep: rust.lang.security.current-exe.current-exe
        std::env::current_exe().ok().as_deref(),
        recorded_install().as_deref(),
    )
}

/// Decide from the launcher's word and the recorded path, so neither the environment nor the
/// filesystem is needed to test it.
///
/// The recorded path has to be the binary that is actually running. A checkout built from source,
/// on a machine where the script installed a copy as well, is not a script install: it is a build
/// nothing here knows how to update, and telling its user to curl over the top of it would replace
/// somebody else's copy rather than theirs.
fn method(via: Option<&str>, running: Option<&Path>, recorded: Option<&Path>) -> Option<Install> {
    if via == Some(NPM) {
        return Some(Install::Npm);
    }
    match (running, recorded) {
        (Some(running), Some(recorded)) if same_file(running, recorded) => Some(Install::Script),
        _ => None,
    }
}

/// Whether two paths name the same binary, following symlinks where they resolve.
///
/// A directory on `PATH` is often a link, so the path a process was started by and the path the
/// script wrote down can spell the same file differently.
fn same_file(left: &Path, right: &Path) -> bool {
    let resolved = |path: &Path| std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    left == right || resolved(left) == resolved(right)
}

/// Where the install script says it put the binary.
fn recorded_install() -> Option<PathBuf> {
    let path = bravebot_agent::home::directory()?.join(INSTALLED_BY_FILE);
    let contents = std::fs::read_to_string(path).ok()?;
    let recorded = contents.lines().next()?.trim();
    (!recorded.is_empty()).then(|| PathBuf::from(recorded))
}

/// What an earlier launch recorded about this installation's registry, where it recorded anything.
fn stored_record(install: Install) -> Option<Record> {
    let path = bravebot_agent::home::directory()?.join(CACHE_FILE);
    parse_record(&std::fs::read_to_string(path).ok()?, install)
}

/// Read this installation's record out of the file's contents.
///
/// A machine where both installations have been used holds a line for each, and each installation
/// reads only the one naming the registry it would ask.
fn parse_record(contents: &str, install: Install) -> Option<Record> {
    contents.lines().find_map(|line| record_on(line, install))
}

/// One line of that file, where it is this installation's record.
///
/// Separate from the I/O so the rules are testable, and there are three: the line is the shape
/// this program writes, it came from the registry this installation would ask, and its version is
/// three numbers or is absent. Anything else is no record, which is silence rather than an error.
fn record_on(line: &str, install: Install) -> Option<Record> {
    let mut fields = line.splitn(3, '\t');
    let (Some(at), Some(source), Some(latest)) = (fields.next(), fields.next(), fields.next())
    else {
        return None;
    };
    if source != install.source() {
        return None;
    }
    Some(Record {
        at: at.parse().ok()?,
        latest: match latest {
            "" => None,
            text => Some(parse_version(text)?),
        },
    })
}

/// One record as the line it is stored as, without its newline.
///
/// An ask that learned no version leaves that field empty, which is what tells a stamp with
/// nothing behind it from a line of some other shape.
fn encode_record(install: Install, record: Record) -> String {
    let latest = record.latest.map(|latest| latest.to_string());
    format!(
        "{}\t{}\t{}",
        record.at,
        install.source(),
        latest.unwrap_or_default()
    )
}

/// A version from `major.minor.patch`, with the single `v` a tag carries allowed in front.
///
/// Three numbers and nothing else. A prerelease, a build suffix, or anything else that is not a
/// number in one of the three places is no answer at all, so a `0.6.0-rc.1` on the registry leaves
/// everybody where they were rather than sending them to a release candidate.
fn parse_version(text: &str) -> Option<Version> {
    let trimmed = text.trim();
    let mut parts = trimmed.strip_prefix('v').unwrap_or(trimmed).split('.');
    let (Some(major), Some(minor), Some(patch), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return None;
    };
    let number = |part: &str| {
        part.chars()
            .all(|c| c.is_ascii_digit())
            .then(|| part.parse().ok())
            .flatten()
    };
    Some(Version {
        major: number(major)?,
        minor: number(minor)?,
        patch: number(patch)?,
    })
}

/// Ask again, on a thread nothing joins, where the last ask is old enough to be worth it.
///
/// Nothing waits on the thread and nothing reads what it learns: it writes the record down, and
/// the next launch is what says anything about it.
fn refresh(install: Install, stored: Option<Record>) {
    if !worth_asking(
        bravebot_agent::home::writable().is_some(),
        now(),
        stored.map(|record| record.at),
    ) {
        return;
    }
    std::thread::spawn(move || {
        // The stamp goes down before the request rather than after it, because the thread is not
        // joined: a session that ends while the ask is still out would otherwise record nothing,
        // and a registry that accepts the connection and then says nothing holds the thread for
        // the whole reply timeout, which outlasts most sessions. Recording the ask first bounds
        // the requests to one a day whatever becomes of this one.
        store(install, recorded(stored, None, now()));
        if let Some(latest) = ask(install) {
            store(install, recorded(stored, Some(latest), now()));
        }
    });
}

/// What to write down once an ask has been made, given what was written down before it.
///
/// The stamp is of the ask, whatever came back. A registry that refuses, that cannot be reached,
/// or that answers with a version this program will not offer has still been asked, and recording
/// nothing would send the next launch straight back to it: the registries in trouble would be the
/// ones asked every single session. The version stands until an ask learns another, since a
/// registry being unreachable today does not make what it said yesterday untrue.
fn recorded(previous: Option<Record>, asked: Option<Version>, at: u64) -> Record {
    Record {
        at,
        latest: asked.or(previous.and_then(|record| record.latest)),
    }
}

/// Whether to ask the registry again.
///
/// A session that may not write to `~/.bravebot` does not ask at all. An incognito session is that
/// case, and a request whose answer is thrown away is one this program made for nothing.
///
/// A stamp in the future is asked again rather than waited out: a clock that moved, or a file
/// somebody else wrote, must not leave this silent until the date catches up with the file.
fn worth_asking(recordable: bool, now: u64, asked_at: Option<u64>) -> bool {
    recordable
        && match asked_at {
            None => true,
            Some(at) => now.checked_sub(at).is_none_or(|age| age >= GOOD_FOR),
        }
}

/// Ask the registry this installation came from what the newest version is.
///
/// Through the egress gate like everything else, since that gate is the only way out. The
/// destination is one of two literals in this file, fixed before the request is made, so nothing
/// fetched has any say in where this goes.
fn ask(install: Install) -> Option<Version> {
    let mut sink = Trail::new();
    let egress = Egress::new();

    let mut routing = Routing::new();
    routing.insert_trusted("update", install.url());

    let mut policy = Policy::begin(
        routing,
        ReleasePlan::new(),
        CapabilitySet::from_iter([Capability::WebFetch]),
        &mut sink,
    )
    .ok()?;

    let request = Request::get(install.url())
        .header("accept", "application/json")
        // GitHub answers a request without one with 403, and a name is politer than a library's
        // default to a registry serving this for free.
        .header("user-agent", "bravebot");

    let response = egress
        .fetch(&mut policy, request, Label::untrusted_public())
        .ok()?;
    let label = response.body.label();
    let (bytes, _) = policy
        .decode_transport("update check", label)
        .decode(response.body);
    version_in(install, &bytes)
}

/// The version an answer states, or `None` for anything that is not the shape expected.
///
/// Separate from the request so both shapes can be read without a server.
fn version_in(install: Install, bytes: &[u8]) -> Option<Version> {
    let answer: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    parse_version(answer.get(install.field())?.as_str()?)
}

/// Write the record down for the next launch.
///
/// Best effort throughout: a machine with no home, a read-only disk and an incognito session all
/// mean the next launch asks again, which is the same thing that happens on a first run.
fn store(install: Install, record: Record) {
    let Some(directory) = bravebot_agent::home::writable() else {
        return;
    };
    if bravebot_agent::home::create_directory(&directory).is_err() {
        return;
    }
    let path = directory.join(CACHE_FILE);
    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    let temporary = directory.join(CACHE_TEMPORARY);
    let contents = merged(&existing, install, record);
    if bravebot_agent::home::write_file(&temporary, contents.as_bytes()).is_ok() {
        let _ = std::fs::rename(&temporary, path);
    }
}

/// This installation's record, with the other installation's kept as it stood.
///
/// A machine that has both installations uses them in turn, and a launch of one that threw the
/// other's stamp away would send the next launch of the other back to its registry, which is the
/// request per session the day exists to prevent. Nothing else in the file survives: a line of any
/// other shape is not a record and is not carried forward.
fn merged(existing: &str, install: Install, record: Record) -> String {
    let mut contents = format!("{}\n", encode_record(install, record));
    if let Some(other) = existing
        .lines()
        .find(|line| record_on(line, install.other()).is_some())
    {
        contents.push_str(other);
        contents.push('\n');
    }
    contents
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version(major: u64, minor: u64, patch: u64) -> Version {
        Version {
            major,
            minor,
            patch,
        }
    }

    /// The whole point of the line is the command, and a person reading it should not have to work
    /// out which of the two installations they have.
    #[test]
    fn a_newer_version_is_named_along_with_the_command_that_installs_it() {
        let said = line(Install::Npm, version(0, 5, 0), version(0, 6, 1)).expect("a newer version");
        assert!(
            said.contains("0.6.1"),
            "the newer version is missing: {said}"
        );
        assert!(
            said.contains("0.5.0"),
            "the running version is missing: {said}"
        );
        assert!(
            said.contains("npm install -g @brave/bravebot@latest"),
            "the command is missing: {said}"
        );
    }

    /// A script install is told to run the script again, never the npm command, which would leave
    /// two copies on the machine and update the one that is not running.
    #[test]
    fn a_script_install_is_given_the_script_again() {
        let said =
            line(Install::Script, version(0, 5, 0), version(1, 0, 0)).expect("a newer version");
        assert!(
            said.contains("install.sh"),
            "the install script is missing: {said}"
        );
        assert!(!said.contains("npm install"), "the wrong command: {said}");
    }

    /// Saying nothing is the ordinary case: most launches are of the newest version, and a line
    /// about it every time is what makes people stop reading the startup screen.
    #[test]
    fn the_version_in_hand_is_not_an_update() {
        assert_eq!(line(Install::Npm, version(0, 5, 0), version(0, 5, 0)), None);
    }

    /// A registry that answers with something older, which is what a yanked release looks like,
    /// must not send anybody backwards.
    #[test]
    fn an_older_published_version_says_nothing() {
        assert_eq!(line(Install::Npm, version(0, 6, 0), version(0, 5, 9)), None);
    }

    /// Each part is compared as a number. Compared as text, 10 sorts before 9 and a release would
    /// go unannounced for as long as the numbering stayed in that decade.
    #[test]
    fn versions_are_ordered_by_number_rather_than_by_spelling() {
        assert!(version(0, 10, 0) > version(0, 9, 0));
        assert!(version(0, 5, 10) > version(0, 5, 9));
        assert!(version(1, 0, 0) > version(0, 999, 999));
    }

    #[test]
    fn a_tag_is_read_with_or_without_the_v_it_is_published_under() {
        assert_eq!(parse_version("v1.2.3"), Some(version(1, 2, 3)));
        assert_eq!(parse_version("1.2.3"), Some(version(1, 2, 3)));
    }

    /// Nobody is sent to a release candidate by a notice at startup, so a version that is not
    /// three plain numbers is not an answer.
    #[test]
    fn a_prerelease_is_not_a_version_to_offer() {
        assert_eq!(parse_version("0.6.0-rc.1"), None);
        assert_eq!(parse_version("0.6.0+build.7"), None);
    }

    #[test]
    fn a_version_that_is_not_three_numbers_is_no_answer() {
        for text in [
            "",
            "1",
            "1.2",
            "1.2.3.4",
            "one.two.three",
            "1.-2.3",
            " ",
            "vv1.2.3",
        ] {
            assert_eq!(parse_version(text), None, "{text} was read as a version");
        }
    }

    /// The two registries answer in their own shapes, and each installation reads its own.
    #[test]
    fn the_registry_states_a_version_and_the_release_listing_states_a_tag() {
        let registry = br#"{"name":"@brave/bravebot","version":"0.7.2"}"#;
        let listing = br#"{"tag_name":"v0.7.2","name":"0.7.2"}"#;
        assert_eq!(
            version_in(Install::Npm, registry),
            Some(version(0, 7, 2)),
            "the registry answer was not read"
        );
        assert_eq!(
            version_in(Install::Script, listing),
            Some(version(0, 7, 2)),
            "the release listing was not read"
        );
    }

    /// The line is composed from the three numbers that were read, so nothing a registry sends is
    /// quoted back onto somebody's screen.
    #[test]
    fn a_version_is_said_as_the_numbers_read_rather_than_as_the_text_that_arrived() {
        let latest = version_in(Install::Npm, br#"{"version":"v0.06.0"}"#).expect("a version");
        let said = line(Install::Npm, version(0, 5, 0), latest).expect("a newer version");
        assert!(said.contains("0.6.0"), "{said}");
        assert!(
            !said.contains("0.06.0"),
            "the answer was quoted back: {said}"
        );
    }

    /// An answer of the wrong shape is silence. A startup notice is the last place an error from a
    /// third party's registry should surface.
    #[test]
    fn an_answer_that_is_not_the_shape_expected_is_no_version() {
        for bytes in [
            &b"not json at all"[..],
            &b"{}"[..],
            &br#"{"version":null}"#[..],
            &br#"{"version":"latest"}"#[..],
            &br#"{"tag_name":"v0.7.2"}"#[..],
        ] {
            assert_eq!(version_in(Install::Npm, bytes), None);
        }
    }

    #[test]
    fn a_stored_answer_is_read_back_as_it_was_written() {
        let record = Record {
            at: 1_700_000_000,
            latest: Some(version(0, 6, 0)),
        };
        let line = encode_record(Install::Npm, record);
        assert_eq!(parse_record(&line, Install::Npm), Some(record));
    }

    /// One machine can have had both installations. An answer from the release listing says
    /// nothing about what npm has published, so it is not read as though it did.
    #[test]
    fn an_answer_from_the_other_registry_is_not_read() {
        let stored = encode_record(
            Install::Script,
            Record {
                at: 1_700_000_000,
                latest: Some(version(9, 9, 9)),
            },
        );
        assert_eq!(parse_record(&stored, Install::Npm), None);
    }

    #[test]
    fn a_file_that_is_not_the_shape_written_here_is_no_answer() {
        for contents in [
            "",
            "nonsense",
            "1700000000\tnpm",
            "later\tnpm\t0.6.0",
            "1700000000\tnpm\t0.6.0-rc.1",
            "1700000000\tnpm\tlatest",
        ] {
            assert_eq!(parse_record(contents, Install::Npm), None, "{contents}");
        }
    }

    /// A registry that refuses, or that answers with something this program will not offer, is the
    /// case where asking again on the next launch is a request to somebody else's registry per
    /// session, and it is the case a registry in trouble is in.
    #[test]
    fn an_ask_that_learned_nothing_still_holds_the_next_one_off_for_the_day() {
        let asked_at = 1_000_000;
        let written = encode_record(Install::Npm, recorded(None, None, asked_at));
        let read_back = parse_record(&written, Install::Npm).expect("the record just written");

        assert_eq!(read_back.latest, None, "a version was invented: {written}");
        assert!(
            !worth_asking(true, asked_at + GOOD_FOR - 1, Some(read_back.at)),
            "the registry is asked again within the day: {written}"
        );
        assert!(
            worth_asking(true, asked_at + GOOD_FOR, Some(read_back.at)),
            "the registry is never asked again: {written}"
        );
    }

    /// A registry being unreachable today does not make what it said yesterday untrue, so the
    /// notice a person has already been shown does not disappear for as long as the outage lasts.
    /// An ask that does learn a version is what replaces it.
    #[test]
    fn a_version_stands_until_an_ask_learns_another() {
        let previous = Record {
            at: 1_000_000,
            latest: Some(version(0, 6, 0)),
        };

        let after_nothing = recorded(Some(previous), None, 2_000_000);
        assert_eq!(
            after_nothing.latest,
            Some(version(0, 6, 0)),
            "the version already known was thrown away"
        );
        assert_eq!(after_nothing.at, 2_000_000, "the ask was not stamped");

        let after_an_answer = recorded(Some(previous), Some(version(0, 7, 0)), 2_000_000);
        assert_eq!(
            after_an_answer.latest,
            Some(version(0, 7, 0)),
            "what the ask learned was discarded for what was already recorded"
        );
    }

    /// Both installations can have been used on one machine, and a launch of one that threw the
    /// other's stamp away would send the next launch of the other straight back to its registry,
    /// which is the per-session request the day exists to prevent.
    #[test]
    fn recording_one_registrys_ask_keeps_the_others() {
        let script = Record {
            at: 1_000_000,
            latest: Some(version(0, 6, 0)),
        };
        let npm = Record {
            at: 2_000_000,
            latest: None,
        };
        let existing = format!("{}\n", encode_record(Install::Script, script));

        let file = merged(&existing, Install::Npm, npm);

        assert_eq!(
            parse_record(&file, Install::Npm),
            Some(npm),
            "the launch's own ask was not recorded: {file:?}"
        );
        assert_eq!(
            parse_record(&file, Install::Script),
            Some(script),
            "the other registry's record was lost: {file:?}"
        );
    }

    /// The file holds one line per registry, so a launch that wrote its own record twice would
    /// grow it without bound and leave two stamps for one registry to choose between.
    #[test]
    fn a_registry_has_one_record_however_often_it_is_asked() {
        let first = Record {
            at: 1_000_000,
            latest: Some(version(0, 6, 0)),
        };
        let second = Record {
            at: 2_000_000,
            latest: Some(version(0, 7, 0)),
        };

        let file = merged(&merged("", Install::Npm, first), Install::Npm, second);

        assert_eq!(file.lines().count(), 1, "{file:?}");
        assert_eq!(parse_record(&file, Install::Npm), Some(second), "{file:?}");
    }

    /// Asking on every launch would be a request to somebody else's registry per session, for a
    /// number that changes on the order of days.
    #[test]
    fn a_fresh_answer_is_not_asked_for_again() {
        assert!(!worth_asking(
            true,
            1_000_000,
            Some(1_000_000 - GOOD_FOR + 1)
        ));
        assert!(worth_asking(true, 1_000_000, Some(1_000_000 - GOOD_FOR)));
        assert!(worth_asking(true, 1_000_000, None));
    }

    /// A clock that moved, or a stamp somebody else wrote, must not leave this silent until the
    /// date catches up with the file.
    #[test]
    fn an_answer_stamped_in_the_future_is_asked_again() {
        assert!(worth_asking(true, 1_000_000, Some(2_000_000)));
    }

    /// A session that keeps nothing has nowhere to put an answer, so asking for one would be a
    /// request made for something that is thrown away on arrival.
    #[test]
    fn a_session_that_records_nothing_asks_nothing() {
        assert!(!worth_asking(false, 1_000_000, None));
        assert!(!worth_asking(false, 1_000_000, Some(1)));
    }

    /// The launcher is the one thing that knows it is the npm package, since the binary it starts
    /// is an ordinary file wherever npm happened to unpack it.
    #[test]
    fn the_launcher_saying_npm_is_what_makes_it_an_npm_install() {
        assert_eq!(
            method(Some("npm"), None, None),
            Some(Install::Npm),
            "the launcher was not believed"
        );
        assert_eq!(
            method(Some("something else"), None, None),
            None,
            "a word nothing sets was taken for an installation"
        );
    }

    #[test]
    fn the_binary_the_script_recorded_is_a_script_install() {
        assert_eq!(
            method(
                None,
                Some(Path::new("/usr/local/bin/bravebot")),
                Some(Path::new("/usr/local/bin/bravebot"))
            ),
            Some(Install::Script)
        );
    }

    /// A checkout built from source on a machine that also has a script install is a build nothing
    /// here can update, and a command that replaced the other copy would be worse than silence.
    #[test]
    fn a_binary_other_than_the_recorded_one_is_not_a_script_install() {
        assert_eq!(
            method(
                None,
                Some(Path::new("/home/someone/bravebot/target/debug/bravebot")),
                Some(Path::new("/usr/local/bin/bravebot"))
            ),
            None
        );
    }

    /// A build from source, which is neither installation, is told nothing at all.
    #[test]
    fn an_installation_nothing_recorded_is_left_alone() {
        assert_eq!(method(None, Some(Path::new("/tmp/bravebot")), None), None);
    }

    /// The version compiled in has to be readable as three numbers, or nothing is ever compared
    /// against it and the notice silently never appears.
    #[test]
    fn the_version_this_was_built_as_is_three_numbers() {
        assert!(running_version().is_some(), "{}", env!("CARGO_PKG_VERSION"));
    }
}
