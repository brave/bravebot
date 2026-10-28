//! Whether a newer bravebot has been published, and the command that takes it.
//!
//! No model, no turn, and no token: a version number, asked of the registry this copy was
//! installed from and compared against the one compiled in.
//!
//! # Startup waits for none of it
//!
//! The line shown at startup comes out of `~/.bravebot`, where an earlier launch wrote the answer
//! down. Asking again happens on a thread nothing joins, at most once an hour, and what it learns is
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
//! planner's context, and it is not in the line either: the line names the version that is
//! running, never the one a registry answered with.
//!
//! That is for accuracy as much as containment. The answer on disk is as old as the last launch
//! that asked, which can be weeks, while the command installs whatever is newest when it runs. A
//! copy on 0.9.0 that last asked while 0.10.0 was newest would be told 0.10.0 is out, and updating
//! would install 0.11.0.
//!
//! # The desktop app
//!
//! The desktop app ships its own copy of the agent, released on the same GitHub release as the
//! script's binary, and no shell command updates it. It asks the release listing as the script
//! install does and shares that record, and what it is told is a boolean: whether the recorded
//! version is newer than the one running ([`desktop_status`]). The window offers a link to the
//! release page rather than a command (UPDATE-12).

#![forbid(unsafe_code)]

use bravebot_config::install::{Install, installed_how};
use bravebot_core::capability::{Capability, CapabilitySet};
use bravebot_core::label::Label;
use bravebot_core::policy::{Policy, ReleasePlan, Routing};
use bravebot_i18n::t;
use bravebot_net::{Egress, Request};
use std::fmt;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use bravebot_session::audit::Trail;

/// The registry entry for the npm package, which answers with the published version.
const NPM_URL: &str = "https://registry.npmjs.org/@brave/bravebot/latest";

/// The newest release of this repository, which answers with the tag it was published under.
const RELEASES_URL: &str = "https://api.github.com/repos/brave/bravebot/releases/latest";

/// Where the last ask of each registry is recorded.
const CACHE_FILE: &str = "update-check";

/// Written here and renamed, so an interrupted write cannot leave half a line to be read back.
const CACHE_TEMPORARY: &str = "update-check.tmp";

/// How long an ask stands before it is worth making another.
///
/// Releases happen on the order of days, and the cost of a stale answer is being told about an
/// update one launch later than it existed. The cost of asking on every launch is a request per
/// launch to somebody else's registry, for a number that will rarely have changed. An hour keeps
/// a person who restarts to pick up a release from waiting a day for the notice, while a burst of
/// launches still makes one request.
const GOOD_FOR: u64 = 60 * 60;

/// Who is asked what the newest version is.
///
/// Its own type rather than methods on [`Install`], because the registries are this crate's
/// business and one asker, the desktop app, is no installation: the type in the configuration
/// surface says how a copy was installed and what command updates it, and nothing there asks
/// anybody what the newest version is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Registry {
    /// The npm package's registry entry.
    Npm,
    /// The release listing of this repository, which every other installation and the desktop app
    /// are published on.
    Releases,
}

impl Registry {
    /// The registry an installation came from.
    fn of(install: Install) -> Self {
        match install {
            Install::Npm => Self::Npm,
            Install::Script | Install::PowerShell | Install::Homebrew | Install::Winget => {
                Self::Releases
            }
        }
    }

    /// The registry the desktop app asks. Its installers are assets of the GitHub release, so it
    /// shares the script install's record (UPDATE-7).
    const DESKTOP: Self = Self::Releases;

    fn url(self) -> &'static str {
        match self {
            Self::Npm => NPM_URL,
            Self::Releases => RELEASES_URL,
        }
    }

    /// The field of that answer holding the version.
    fn field(self) -> &'static str {
        match self {
            Self::Npm => "version",
            Self::Releases => "tag_name",
        }
    }

    /// Kept in the file so an answer from one registry is not read as an answer from the other on
    /// a machine where both installations have existed.
    fn source(self) -> &'static str {
        match self {
            Self::Npm => "npm",
            Self::Releases => "releases",
        }
    }

    /// The other registry, whose record shares the file with this one's.
    fn other(self) -> Self {
        match self {
            Self::Npm => Self::Releases,
            Self::Releases => Self::Npm,
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
///
/// `setting` is what the `updateCheck` key said. Switched off, by it or by the environment, this
/// returns before anything is read, asked or written (UPDATE-11).
pub fn at_startup(setting: Option<bool>) -> Option<String> {
    if !enabled(setting) {
        return None;
    }
    let install = installed_how()?;
    let registry = Registry::of(install);
    let stored = stored_record(registry);
    refresh(registry, stored);
    line(install, running_version()?, stored?.latest?)
}

/// What the desktop app is told about updating: whether the check is on, and, where an earlier ask
/// recorded a version, whether it is newer than the one running.
///
/// The recorded version itself is not in the answer, only the comparison, so no number a registry
/// sent reaches the window (UPDATE-5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Status {
    /// `false` when the setting or the variable turned the check off (UPDATE-11).
    pub check: bool,
    /// `None` when there is no record to compare with, which is also what any failure to read one
    /// is (UPDATE-9).
    pub newer: Option<bool>,
}

/// What the desktop app is told about updating, from what an earlier ask recorded.
///
/// Reads one file and asks nobody. With the check off nothing is read.
pub fn desktop_status(setting: Option<bool>) -> Status {
    let directory = bravebot_agent::home::directory();
    status_in(enabled(setting), directory.as_deref(), running_version())
}

/// [`desktop_status`] from its inputs, so the rules need neither the environment nor a home.
fn status_in(enabled: bool, directory: Option<&Path>, running: Option<Version>) -> Status {
    if !enabled {
        return Status {
            check: false,
            newer: None,
        };
    }
    let latest = directory
        .and_then(|directory| record_in(directory, Registry::DESKTOP))
        .and_then(|record| record.latest);
    Status {
        check: true,
        newer: running
            .zip(latest)
            .map(|(running, latest)| latest > running),
    }
}

/// What the packaged desktop app sets [`bravebot_config::env_var::INSTALLED_VIA`] to when it
/// starts its agent.
pub const DESKTOP_LAUNCHER: &str = "desktop";

/// Start the ask that answers the desktop app's next launch, on a thread nothing joins.
///
/// The same hourly stamp the terminal keeps, and the same refusal to ask from a session that can
/// record nothing (UPDATE-6, UPDATE-8). With the check off nothing is read or asked. Only an agent
/// the packaged app started asks: one built from source and run by hand, by a test or by a window
/// in development is left alone, as a build from source is in the terminal.
pub fn desktop_refresh(setting: Option<bool>) {
    let launcher = std::env::var(bravebot_config::env_var::INSTALLED_VIA).ok();
    if !desktop_asks(enabled(setting), launcher.as_deref()) {
        return;
    }
    refresh(Registry::DESKTOP, stored_record(Registry::DESKTOP));
}

/// Whether the desktop app's agent asks: the check is on and the packaged app started it.
fn desktop_asks(enabled: bool, launcher: Option<&str>) -> bool {
    enabled && launcher == Some(DESKTOP_LAUNCHER)
}

/// Whether the check is on, from the setting and the variable.
fn enabled(setting: Option<bool>) -> bool {
    check_enabled(
        setting,
        std::env::var(bravebot_config::env_var::UPDATE_CHECK)
            .ok()
            .as_deref(),
    )
}

/// Whether the person has left the check on. Either the key being `false` or the variable being
/// `0` turns it off, and neither can turn it on again against the other.
fn check_enabled(setting: Option<bool>, env: Option<&str>) -> bool {
    setting != Some(false) && env != Some("0")
}

/// What to say when a newer version is out, or nothing when this copy is not behind it.
///
/// Separated from the files and the clock so what is said can be tested without either.
fn line(install: Install, running: Version, latest: Version) -> Option<String> {
    (latest > running).then(|| {
        t!(
            update_available,
            running = running,
            command = install.update_command()
        )
    })
}

/// The version this binary was built as.
fn running_version() -> Option<Version> {
    parse_version(env!("CARGO_PKG_VERSION"))
}

/// What an earlier launch recorded about this registry, where it recorded anything.
fn stored_record(registry: Registry) -> Option<Record> {
    record_in(&bravebot_agent::home::directory()?, registry)
}

/// The record of `registry` in the state directory `directory`.
fn record_in(directory: &Path, registry: Registry) -> Option<Record> {
    parse_record(
        &std::fs::read_to_string(directory.join(CACHE_FILE)).ok()?,
        registry,
    )
}

/// Read this registry's record out of the file's contents.
///
/// A machine where both installations have been used holds a line for each, and each installation
/// reads only the one naming the registry it would ask.
fn parse_record(contents: &str, registry: Registry) -> Option<Record> {
    contents.lines().find_map(|line| record_on(line, registry))
}

/// One line of that file, where it is this registry's record.
///
/// Separate from the I/O so the rules are testable, and there are three: the line is the shape
/// this program writes, it came from the registry this installation would ask, and its version is
/// three numbers or is absent. Anything else is no record, which is silence rather than an error.
fn record_on(line: &str, registry: Registry) -> Option<Record> {
    let mut fields = line.splitn(3, '\t');
    let (Some(at), Some(source), Some(latest)) = (fields.next(), fields.next(), fields.next())
    else {
        return None;
    };
    if source != registry.source() {
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
fn encode_record(registry: Registry, record: Record) -> String {
    let latest = record.latest.map(|latest| latest.to_string());
    format!(
        "{}\t{}\t{}",
        record.at,
        registry.source(),
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
fn refresh(registry: Registry, stored: Option<Record>) {
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
        // the requests to one an hour whatever becomes of this one, so an ask whose stamp did not
        // land is not made: nothing would hold the next launch back from making it again.
        if !store(registry, recorded(stored, None, now())) {
            return;
        }
        if let Some(latest) = ask(registry) {
            store(registry, recorded(stored, Some(latest), now()));
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

/// Ask the registry what the newest version is.
///
/// Through the egress gate like everything else, since that gate is the only way out. The
/// destination is one of two literals in this file, fixed before the request is made, so nothing
/// fetched has any say in where this goes.
fn ask(registry: Registry) -> Option<Version> {
    let mut sink = Trail::new();
    let egress = Egress::new();

    let mut routing = Routing::new();
    routing.insert_trusted("update", registry.url());

    let mut policy = Policy::begin(
        routing,
        ReleasePlan::new(),
        CapabilitySet::from_iter([Capability::WebFetch]),
        &mut sink,
    )
    .ok()?;

    let request = Request::get(registry.url())
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
    version_in(registry, &bytes)
}

/// The version an answer states, or `None` for anything that is not the shape expected.
///
/// Separate from the request so both shapes can be read without a server.
fn version_in(registry: Registry, bytes: &[u8]) -> Option<Version> {
    let answer: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    parse_version(answer.get(registry.field())?.as_str()?)
}

/// Write the record down for the next launch, answering whether it landed.
///
/// A machine with no home, a directory this user cannot write to and an incognito session all
/// answer `false`, and nothing is reported (UPDATE-9).
fn store(registry: Registry, record: Record) -> bool {
    let Some(directory) = bravebot_agent::home::writable() else {
        return false;
    };
    if bravebot_agent::home::create_directory(&directory).is_err() {
        return false;
    }
    let path = directory.join(CACHE_FILE);
    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    let temporary = directory.join(CACHE_TEMPORARY);
    let contents = merged(&existing, registry, record);
    bravebot_agent::home::write_file(&temporary, contents.as_bytes()).is_ok()
        && std::fs::rename(&temporary, path).is_ok()
}

/// This installation's record, with the other installation's kept as it stood.
///
/// A machine that has both installations uses them in turn, and a launch of one that threw the
/// other's stamp away would send the next launch of the other back to its registry, which is the
/// request per session the day exists to prevent. Nothing else in the file survives: a line of any
/// other shape is not a record and is not carried forward.
fn merged(existing: &str, registry: Registry, record: Record) -> String {
    let mut contents = format!("{}\n", encode_record(registry, record));
    if let Some(other) = existing
        .lines()
        .find(|line| record_on(line, registry.other()).is_some())
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
    fn a_newer_version_is_announced_along_with_the_command_that_installs_it() {
        let said = line(Install::Npm, version(0, 5, 0), version(0, 6, 1)).expect("a newer version");
        assert!(
            said.contains("0.5.0"),
            "the running version is missing: {said}"
        );
        assert!(
            said.contains("npm install -g @brave/bravebot@latest"),
            "the command is missing: {said}"
        );
    }

    /// The recorded version is whatever was newest at the last launch that asked, and the command
    /// installs whatever is newest now. Naming the recorded one told somebody on 0.9.0 that 0.10.0
    /// was out when updating installed 0.11.0.
    #[test]
    fn the_line_names_the_running_version_and_not_the_recorded_one() {
        let said =
            line(Install::Npm, version(0, 9, 0), version(0, 10, 0)).expect("a newer version");
        assert!(
            said.contains("0.9.0"),
            "the running version is missing: {said}"
        );
        assert!(
            !said.contains("0.10"),
            "the recorded version, which may not be the newest, was named: {said}"
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

    /// A PowerShell script install is told to run that script again, never the shell script's line,
    /// which cannot run on Windows, and never the npm one. Like the shell script's, its newest
    /// version is read from the release listing.
    #[test]
    fn a_powershell_install_is_given_the_powershell_script_again() {
        let said =
            line(Install::PowerShell, version(0, 5, 0), version(1, 0, 0)).expect("a newer version");
        assert!(
            said.contains("install.ps1"),
            "the PowerShell install script is missing: {said}"
        );
        assert!(!said.contains("install.sh"), "the wrong command: {said}");
        assert!(!said.contains("npm install"), "the wrong command: {said}");
        assert_eq!(
            version_in(
                Registry::of(Install::PowerShell),
                br#"{"tag_name":"v0.7.2"}"#
            ),
            Some(version(0, 7, 2)),
            "the release listing was not read"
        );
        assert_eq!(
            Registry::of(Install::PowerShell),
            Registry::of(Install::Script)
        );
        assert_eq!(Registry::of(Install::PowerShell).other(), Registry::Npm);
    }

    /// A package manager's copy is told its manager's own upgrade command, read from the release
    /// listing like the script's, and never the npm or script line, which would put a second copy
    /// beside the one the manager owns.
    #[test]
    fn a_package_manager_install_is_given_its_managers_upgrade_command() {
        let brew = line(Install::Homebrew, version(0, 5, 0), version(1, 0, 0)).expect("newer");
        assert!(brew.contains("brew upgrade --cask bravebot"), "{brew}");
        let winget = line(Install::Winget, version(0, 5, 0), version(1, 0, 0)).expect("newer");
        assert!(winget.contains("winget upgrade Brave.Bravebot"), "{winget}");
        for said in [&brew, &winget] {
            assert!(!said.contains("npm install"), "the wrong command: {said}");
            assert!(!said.contains("install.sh"), "the wrong command: {said}");
        }
        let listing = br#"{"tag_name":"v0.7.2"}"#;
        for install in [Install::Homebrew, Install::Winget] {
            assert_eq!(
                version_in(Registry::of(install), listing),
                Some(version(0, 7, 2)),
                "{install:?} did not read the release listing"
            );
            assert_eq!(Registry::of(install), Registry::of(Install::Script));
            assert_eq!(Registry::of(install).other(), Registry::Npm);
        }
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
            version_in(Registry::of(Install::Npm), registry),
            Some(version(0, 7, 2)),
            "the registry answer was not read"
        );
        assert_eq!(
            version_in(Registry::of(Install::Script), listing),
            Some(version(0, 7, 2)),
            "the release listing was not read"
        );
    }

    /// What a registry sends decides whether the line is said and is not part of it, so nothing a
    /// registry sends reaches somebody's screen, as text or as the numbers read out of it.
    #[test]
    fn nothing_a_registry_answered_reaches_the_line() {
        let latest =
            version_in(Registry::of(Install::Npm), br#"{"version":"v0.06.0"}"#).expect("a version");
        let said = line(Install::Npm, version(0, 5, 0), latest).expect("a newer version");
        assert!(
            !said.contains("0.06.0"),
            "the answer was quoted back: {said}"
        );
        assert!(
            !said.contains("0.6.0"),
            "the version read from the answer was named: {said}"
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
            assert_eq!(version_in(Registry::of(Install::Npm), bytes), None);
        }
    }

    #[test]
    fn a_stored_answer_is_read_back_as_it_was_written() {
        let record = Record {
            at: 1_700_000_000,
            latest: Some(version(0, 6, 0)),
        };
        let line = encode_record(Registry::of(Install::Npm), record);
        assert_eq!(
            parse_record(&line, Registry::of(Install::Npm)),
            Some(record)
        );
    }

    /// One machine can have had both installations. An answer from the release listing says
    /// nothing about what npm has published, so it is not read as though it did.
    #[test]
    fn an_answer_from_the_other_registry_is_not_read() {
        let stored = encode_record(
            Registry::of(Install::Script),
            Record {
                at: 1_700_000_000,
                latest: Some(version(9, 9, 9)),
            },
        );
        assert_eq!(parse_record(&stored, Registry::of(Install::Npm)), None);
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
            assert_eq!(
                parse_record(contents, Registry::of(Install::Npm)),
                None,
                "{contents}"
            );
        }
    }

    /// A registry that refuses, or that answers with something this program will not offer, is the
    /// case where asking again on the next launch is a request to somebody else's registry per
    /// session, and it is the case a registry in trouble is in.
    #[test]
    fn an_ask_that_learned_nothing_still_holds_the_next_one_off_for_the_day() {
        let asked_at = 1_000_000;
        let written = encode_record(Registry::of(Install::Npm), recorded(None, None, asked_at));
        let read_back =
            parse_record(&written, Registry::of(Install::Npm)).expect("the record just written");

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
        let existing = format!("{}\n", encode_record(Registry::of(Install::Script), script));

        let file = merged(&existing, Registry::of(Install::Npm), npm);

        assert_eq!(
            parse_record(&file, Registry::of(Install::Npm)),
            Some(npm),
            "the launch's own ask was not recorded: {file:?}"
        );
        assert_eq!(
            parse_record(&file, Registry::of(Install::Script)),
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

        let file = merged(&merged("", Registry::Npm, first), Registry::Npm, second);

        assert_eq!(file.lines().count(), 1, "{file:?}");
        assert_eq!(
            parse_record(&file, Registry::of(Install::Npm)),
            Some(second),
            "{file:?}"
        );
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

    /// A person restarting to pick up a release is not made to wait a day for the notice, and a
    /// burst of launches is still one request.
    #[test]
    fn a_registry_is_asked_again_once_an_hour_has_passed() {
        let asked_at = 1_000_000;
        assert!(!worth_asking(true, asked_at + 59 * 60, Some(asked_at)));
        assert!(worth_asking(true, asked_at + 60 * 60, Some(asked_at)));
    }

    /// A clock that moved, or a stamp somebody else wrote, must not leave this silent until the
    /// date catches up with the file.
    #[test]
    fn an_answer_stamped_in_the_future_is_asked_again() {
        assert!(worth_asking(true, 1_000_000, Some(2_000_000)));
    }

    /// A person who turned the check off gets no read, no request and no line, however they did it.
    #[test]
    fn either_switch_turns_the_check_off() {
        assert!(check_enabled(None, None));
        assert!(check_enabled(Some(true), None));
        assert!(check_enabled(None, Some("1")));
        assert!(check_enabled(None, Some("")));
        assert!(!check_enabled(Some(false), None));
        assert!(!check_enabled(None, Some("0")));
        assert!(!check_enabled(Some(true), Some("0")));
        assert!(!check_enabled(Some(false), Some("1")));
    }

    /// A session that keeps nothing has nowhere to put an answer, so asking for one would be a
    /// request made for something that is thrown away on arrival.
    #[test]
    fn a_session_that_records_nothing_asks_nothing() {
        assert!(!worth_asking(false, 1_000_000, None));
        assert!(!worth_asking(false, 1_000_000, Some(1)));
    }

    /// The version compiled in has to be readable as three numbers, or nothing is ever compared
    /// against it and the notice silently never appears.
    #[test]
    fn the_version_this_was_built_as_is_three_numbers() {
        assert!(running_version().is_some(), "{}", env!("CARGO_PKG_VERSION"));
    }

    /// The desktop app's installers are assets of the GitHub release, so it reads the release
    /// listing and the record a script install wrote, and a record the npm registry answered is not
    /// read as one about the release.
    #[test]
    fn the_desktop_app_asks_the_release_listing_and_shares_the_scripts_record() {
        assert_eq!(Registry::DESKTOP, Registry::of(Install::Script));
        assert_eq!(Registry::DESKTOP.url(), RELEASES_URL);
        assert_eq!(
            version_in(Registry::DESKTOP, br#"{"tag_name":"v0.7.2"}"#),
            Some(version(0, 7, 2))
        );
        let state = tempfile::tempdir().expect("a state directory");
        let script = Record {
            at: 1_000_000,
            latest: Some(version(9, 0, 0)),
        };
        std::fs::write(
            state.path().join(CACHE_FILE),
            merged("", Registry::of(Install::Script), script),
        )
        .expect("a record");
        assert_eq!(record_in(state.path(), Registry::DESKTOP), Some(script));

        let npm_only = tempfile::tempdir().expect("a state directory");
        std::fs::write(
            npm_only.path().join(CACHE_FILE),
            merged("", Registry::Npm, script),
        )
        .expect("a record");
        assert_eq!(record_in(npm_only.path(), Registry::DESKTOP), None);
    }

    /// The window is told whether the recorded version is newer than the one running, and the
    /// version itself is not in the answer (UPDATE-5, UPDATE-12).
    #[test]
    fn the_desktop_app_is_told_only_whether_a_newer_version_is_recorded() {
        let state = tempfile::tempdir().expect("a state directory");
        let running = Some(version(0, 9, 0));
        assert_eq!(
            status_in(true, Some(state.path()), running),
            Status {
                check: true,
                newer: None
            },
            "a first run has no record"
        );
        let seed = |latest| {
            let record = Record {
                at: 1_000_000,
                latest: Some(latest),
            };
            std::fs::write(
                state.path().join(CACHE_FILE),
                merged("", Registry::DESKTOP, record),
            )
            .expect("a record");
        };
        seed(version(0, 10, 0));
        assert_eq!(
            status_in(true, Some(state.path()), running).newer,
            Some(true)
        );
        seed(version(0, 9, 0));
        assert_eq!(
            status_in(true, Some(state.path()), running).newer,
            Some(false)
        );
        seed(version(0, 8, 9));
        assert_eq!(
            status_in(true, Some(state.path()), running).newer,
            Some(false)
        );
        assert_eq!(status_in(true, None, running).newer, None, "no home");
    }

    /// With the check off the desktop app is told so, and nothing a record says changes that.
    #[test]
    fn a_desktop_check_turned_off_says_nothing_of_a_record() {
        let state = tempfile::tempdir().expect("a state directory");
        let record = Record {
            at: 1_000_000,
            latest: Some(version(99, 0, 0)),
        };
        std::fs::write(
            state.path().join(CACHE_FILE),
            merged("", Registry::DESKTOP, record),
        )
        .expect("a record");
        assert_eq!(
            status_in(false, Some(state.path()), Some(version(0, 9, 0))),
            Status {
                check: false,
                newer: None
            }
        );
    }

    /// A record that is not the shape written here is no record, so the window shows no notice
    /// rather than an error (UPDATE-9).
    #[test]
    fn a_desktop_record_that_is_not_the_shape_written_is_no_answer() {
        let state = tempfile::tempdir().expect("a state directory");
        std::fs::write(state.path().join(CACHE_FILE), "later\treleases\t99.0.0\n")
            .expect("a record");
        assert_eq!(
            status_in(true, Some(state.path()), Some(version(0, 9, 0))).newer,
            None
        );
    }

    /// Only an agent the packaged app started asks, and not with the check off: a build run by
    /// hand, by a test or by a window in development asks nobody.
    #[test]
    fn only_the_packaged_desktop_app_asks_and_not_with_the_check_off() {
        assert!(desktop_asks(true, Some(DESKTOP_LAUNCHER)));
        assert!(!desktop_asks(false, Some(DESKTOP_LAUNCHER)));
        for launcher in [None, Some("npm"), Some(""), Some("Desktop")] {
            assert!(!desktop_asks(true, launcher), "{launcher:?}");
        }
    }
}
