#![forbid(unsafe_code)]
//! The anonymous usage ping: one request a day to Brave saying that a copy of bravebot ran.
//!
//! The request carries the platform, the version, how the copy was installed, which front end
//! started, which kind of service answers the default model, whether a premium key is held, four
//! flags saying whether this is the first ping of the install, day, week and month, and the Monday
//! of the week the install first pinged. Nothing else is sent, and every value is one of a fixed
//! set or a date, so nothing a person typed or a file holds can reach the request.
//!
//! The ping is sent only while [`bravebot_session::store::usage_ping`] says it is on and after a
//! front end has shown the notice that says what it sends. An incognito session sends nothing.
//!
//! Dates are UTC days. The workspace has no timezone database and this crate may not call the C
//! library, so a person far from UTC has their day change at a different hour than their clock's.

use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use bravebot_agent::calendar::{civil_from_days, days_from_civil};
use bravebot_config::install::Install;
use bravebot_config::{Config, Serves};
use bravebot_core::capability::{Capability, CapabilitySet};
use bravebot_core::label::Label;
use bravebot_core::policy::{Policy, ReleasePlan, Routing};
use bravebot_net::{Egress, Request, Timeouts};
use bravebot_session::audit::Trail;

/// Where the ping goes unless [`bravebot_config::env_var::USAGE_PING_URL`] says otherwise.
///
/// The host is not yet confirmed by the service's owners; bead bravebot-en9.1 tracks it.
const BASE_URL: &str = "https://laptop-updates.brave.com";

/// The path the service answers the ping on.
pub const PATH: &str = "/1/usage/bravebot";

/// The record of when the ping last went, beside the other state files.
const STATE_FILE: &str = "usage-ping-state";

/// Held while one process decides whether to ping and sends it, so two starts at once send once.
const LOCK_FILE: &str = "usage-ping.lock";

/// How long a failed ping waits before the next start tries again.
const RETRY_AFTER: u64 = 60 * 60;

const SECONDS_PER_DAY: u64 = 86_400;

/// The version this build reports. Sent only when it is three numbers and nothing else.
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The front end that started.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Surface {
    /// The interactive terminal.
    Tui,
    /// The desktop app.
    Desktop,
    /// A run without the interactive terminal: `-p`, `--plain` and ACP.
    Cli,
}

impl Surface {
    /// The word the service takes.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Tui => "tui",
            Self::Desktop => "desktop",
            Self::Cli => "cli",
        }
    }
}

/// The kind of service that answers the default model.
///
/// A fixed set, so a gateway's id, which a person chose, is never sent: one this does not know is
/// [`Backend::Other`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend {
    /// Brave's own endpoint.
    Brave,
    /// An AWS account.
    Bedrock,
    /// The ChatGPT account.
    Chatgpt,
    /// The xAI account.
    Xai,
    /// OpenAI's API, through a gateway entry.
    Openai,
    /// Anthropic's API, through a gateway entry.
    Anthropic,
    /// OpenRouter, through a gateway entry.
    Openrouter,
    /// A local Ollama.
    Ollama,
    /// A local LM Studio.
    Lmstudio,
    /// A local llama-server.
    Llamaserver,
    /// A gateway entry of an id not listed here.
    Other,
    /// No model is configured.
    None,
}

impl Backend {
    /// The word the service takes: lowercase letters, ten at most.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Brave => "brave",
            Self::Bedrock => "bedrock",
            Self::Chatgpt => "chatgpt",
            Self::Xai => "xai",
            Self::Openai => "openai",
            Self::Anthropic => "anthropic",
            Self::Openrouter => "openrouter",
            Self::Ollama => "ollama",
            Self::Lmstudio => "lmstudio",
            Self::Llamaserver => "llamacpp",
            Self::Other => "other",
            Self::None => "none",
        }
    }

    /// The kind of service a gateway entry's id names.
    fn of_gateway(id: &str) -> Self {
        match id {
            "openai" => Self::Openai,
            "anthropic" => Self::Anthropic,
            "openrouter" => Self::Openrouter,
            "ollama" => Self::Ollama,
            "lmstudio" => Self::Lmstudio,
            "llama-server" => Self::Llamaserver,
            _ => Self::Other,
        }
    }
}

/// The kind of service that answers `model`, or the configured default where `model` is `None`.
///
/// [`Backend::None`] where neither names a model.
pub fn backend(config: &Config, model: Option<&str>) -> Backend {
    let model = model.unwrap_or(&config.default_model);
    if model.trim().is_empty() {
        return Backend::None;
    }
    match config.serves(model) {
        Serves::Bedrock(_) => Backend::Bedrock,
        Serves::Gateway(provider, _) => Backend::of_gateway(&provider.id),
        Serves::Chatgpt(_) => Backend::Chatgpt,
        Serves::Xai(_) => Backend::Xai,
        Serves::Aichat => Backend::Brave,
    }
}

/// Whether the configuration holds a premium key for Brave's endpoint.
pub fn premium(config: &Config) -> bool {
    bravebot_agent::subscription::PremiumKey::held(config).is_some()
}

/// What one call to [`send`] did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The service accepted the ping.
    Sent,
    /// The ping is off, or the session is incognito.
    Off,
    /// No front end has shown the notice yet.
    NoticeNotShown,
    /// This platform or version is not one the service takes, or this is a development build
    /// with nowhere named to send it.
    Unsupported,
    /// A ping was already accepted today.
    AlreadyToday,
    /// A ping failed less than an hour ago.
    Waiting,
    /// Another process is sending it now.
    Busy,
    /// The record could not be written, or the request failed.
    Failed,
}

/// Send the ping on a thread of its own, so a start never waits on it.
///
/// For the interactive terminal, the desktop app and the runs without the terminal. A scheduled or
/// background run does not call this. Dropping what this returns leaves the thread running.
pub fn at_startup(surface: Surface, backend: Backend, premium: bool) -> Sending {
    let (done, finished) = std::sync::mpsc::channel();
    #[expect(
        clippy::let_underscore_must_use,
        reason = "a thread that cannot start sends nothing, which `settle` reports as not finished"
    )]
    let _ = std::thread::Builder::new()
        .name("usage-ping".into())
        .spawn(move || {
            #[expect(
                clippy::let_underscore_must_use,
                reason = "nobody waiting for the outcome is no failure of the ping"
            )]
            let _ = done.send(send(surface, backend, premium));
        });
    Sending { finished }
}

/// A ping [`at_startup`] started.
pub struct Sending {
    finished: std::sync::mpsc::Receiver<Outcome>,
}

impl Sending {
    /// Wait at most `limit` for the ping to finish, for a process about to exit, whose exit would
    /// otherwise end the thread mid-request. `None` where it had not finished.
    pub fn settle(self, limit: Duration) -> Option<Outcome> {
        self.finished.recv_timeout(limit).ok()
    }
}

/// Send the ping now if it is due, waiting for the answer.
pub fn send(surface: Surface, backend: Backend, premium: bool) -> Outcome {
    if !bravebot_session::store::usage_ping().is_on() {
        return Outcome::Off;
    }
    if !bravebot_session::store::usage_ping_notice_shown() {
        return Outcome::NoticeNotShown;
    }
    let Some(directory) = bravebot_agent::home::writable() else {
        return Outcome::Off;
    };
    let Some(platform) = platform(std::env::consts::OS) else {
        return Outcome::Unsupported;
    };
    if !strict_version(VERSION) {
        return Outcome::Unsupported;
    }
    let Some(base) = base_url() else {
        return Outcome::Unsupported;
    };
    let fields = Fields {
        platform,
        version: VERSION,
        install: install_word(bravebot_config::install::installed_how()),
        surface,
        backend,
        premium,
    };
    send_in(&directory, &fields, now(), &base, request)
}

/// The values a ping carries other than its dates.
struct Fields {
    platform: &'static str,
    version: &'static str,
    install: &'static str,
    surface: Surface,
    backend: Backend,
    premium: bool,
}

/// [`send`] once the switches have been read, with the clock and the request handed in.
///
/// The record is read once without the lock, so the ordinary start that already pinged today
/// opens no file for writing. Only that answer is taken from it: an attempt it records may be one
/// still in flight, which is the lock's to report. Read again under the lock, since the process
/// that held it may have just pinged. The attempt is recorded before the request goes, so a start that crashes during
/// it waits an hour like one that failed.
fn send_in(
    directory: &Path,
    fields: &Fields,
    now: u64,
    base: &str,
    fetch: impl FnOnce(&str) -> bool,
) -> Outcome {
    let state_path = directory.join(STATE_FILE);
    let today = day_of(now);
    if let Err(Outcome::AlreadyToday) = due(&State::read(&state_path), today, now) {
        return Outcome::AlreadyToday;
    }
    if bravebot_agent::home::create_directory(directory).is_err() {
        return Outcome::Failed;
    }
    let Some(lock) = open_lock(&directory.join(LOCK_FILE)) else {
        return Outcome::Failed;
    };
    if lock.try_lock().is_err() {
        return Outcome::Busy;
    }

    let state = State::read(&state_path);
    if let Err(outcome) = due(&state, today, now) {
        return outcome;
    }
    let woi = state.woi.unwrap_or_else(|| monday_of(today));
    let flags = Flags::between(state.success, today);
    let attempted = State {
        attempt: Some(now),
        ..state
    };
    if !attempted.write(&state_path) {
        return Outcome::Failed;
    }

    if !fetch(&url(base, fields, flags, woi)) {
        return Outcome::Failed;
    }
    // Recorded only once a ping is accepted, so a failure late in one week does not date the install
    // to it when the first ping that arrives is in the next.
    let succeeded = State {
        woi: Some(woi),
        success: Some(today),
        ..attempted
    };
    match succeeded.write(&state_path) {
        true => Outcome::Sent,
        false => Outcome::Failed,
    }
}

/// Whether a ping is due, or why not.
///
/// An attempt stamped in the future is no reason to wait, so a clock set back does not stop the
/// ping for however far it moved.
fn due(state: &State, today: i64, now: u64) -> Result<(), Outcome> {
    if state.success == Some(today) {
        return Err(Outcome::AlreadyToday);
    }
    if state
        .attempt
        .is_some_and(|at| at <= now && now - at < RETRY_AFTER)
    {
        return Err(Outcome::Waiting);
    }
    Ok(())
}

/// The lock file, created readable only by this user.
fn open_lock(path: &Path) -> Option<std::fs::File> {
    let mut options = std::fs::OpenOptions::new();
    options.create(true).truncate(false).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path).ok()
}

/// Make the request, answering whether the service accepted it.
///
/// The body is not read for anything: a status in the 2xx range is the whole answer.
fn request(url: &str) -> bool {
    let mut sink = Trail::new();
    let egress = Egress::with_timeouts(Timeouts {
        resolve: Duration::from_secs(10),
        connect: Duration::from_secs(10),
        send: Duration::from_secs(10),
        reply: Duration::from_secs(15),
        idle: Duration::from_secs(10),
    });
    let mut routing = Routing::new();
    routing.insert_trusted("usage ping", url);
    let Ok(mut policy) = Policy::begin(
        routing,
        ReleasePlan::new(),
        CapabilitySet::from_iter([Capability::WebFetch]),
        &mut sink,
    ) else {
        return false;
    };
    let request = Request::get(url).header("user-agent", "bravebot");
    egress
        .fetch(&mut policy, request, Label::untrusted_public())
        .is_ok_and(|response| (200..300).contains(&response.status))
}

/// The base URL the ping goes to, from the variable where it is set.
///
/// A development build has none unless the variable names one, so neither `cargo run` nor a test
/// that starts the program is counted as a person's launch.
fn base_url() -> Option<String> {
    base_url_from(
        std::env::var_os(bravebot_config::env_var::USAGE_PING_URL),
        cfg!(debug_assertions),
    )
}

/// The base URL for the variable's value `named`, in a development build where `development`.
fn base_url_from(named: Option<std::ffi::OsString>, development: bool) -> Option<String> {
    // An address named in something other than text is refused rather than read as unset, which
    // in a released build would send to the default the person meant to replace.
    let named = match named {
        None => None,
        Some(value) => Some(value.into_string().ok()?),
    }
    .map(|value| value.trim().trim_end_matches('/').to_string())
    .filter(|value| !value.is_empty());
    match named {
        Some(named) => Some(named),
        None if development => None,
        None => Some(BASE_URL.to_string()),
    }
}

/// The whole request URL. Every value is a fixed word, a number or a date, so none needs escaping.
fn url(base: &str, fields: &Fields, flags: Flags, woi: i64) -> String {
    format!(
        "{base}{PATH}?platform={}&version={}&install={}&surface={}&backend={}&premium={}\
         &daily={}&weekly={}&monthly={}&first={}&woi={}",
        fields.platform,
        fields.version,
        fields.install,
        fields.surface.as_str(),
        fields.backend.as_str(),
        fields.premium,
        flags.daily,
        flags.weekly,
        flags.monthly,
        flags.first,
        ymd(woi),
    )
}

/// The service's word for the operating system, or `None` for one it does not take.
fn platform(os: &str) -> Option<&'static str> {
    match os {
        "macos" => Some("darwin"),
        "linux" => Some("linux"),
        "windows" => Some("windows"),
        _ => None,
    }
}

/// Whether `version` is three numbers separated by dots and nothing else.
fn strict_version(version: &str) -> bool {
    let parts: Vec<&str> = version.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
}

/// The word for how this copy was installed.
///
/// The desktop app is told apart by its surface, not by its install, so it reports what the copy
/// it runs says.
fn install_word(install: Option<Install>) -> &'static str {
    match install {
        Some(Install::Npm) => "npm",
        Some(Install::Script) => "script",
        Some(Install::PowerShell) => "powershell",
        Some(Install::Homebrew) => "homebrew",
        Some(Install::Winget) => "winget",
        Some(Install::Apt) => "apt",
        Some(Install::Dnf) => "dnf",
        None => "unknown",
    }
}

/// Whether this is the first ping of the install, and of the day, ISO week and month.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "each is a query parameter of its own that the service reads as a boolean"
)]
struct Flags {
    first: bool,
    daily: bool,
    weekly: bool,
    monthly: bool,
}

impl Flags {
    /// The flags for a ping today, after the last accepted one on `last`.
    fn between(last: Option<i64>, today: i64) -> Self {
        let Some(last) = last else {
            return Self {
                first: true,
                daily: true,
                weekly: true,
                monthly: true,
            };
        };
        let (last_year, last_month, _) = civil_from_days(last);
        let (year, month, _) = civil_from_days(today);
        Self {
            first: false,
            daily: last != today,
            weekly: monday_of(last) != monday_of(today),
            monthly: (last_year, last_month) != (year, month),
        }
    }
}

/// What the state file records, each as a day count since 1970-01-01 except the attempt, which is
/// seconds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct State {
    /// The Monday of the week of the install's first accepted ping. Written once.
    woi: Option<i64>,
    /// The day of the last ping the service accepted.
    success: Option<i64>,
    /// When the last ping was tried.
    attempt: Option<u64>,
}

impl State {
    /// The record at `path`, or an empty one where there is none or it cannot be read.
    fn read(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .map(|text| Self::parse(&text))
            .unwrap_or_default()
    }

    /// Read the record out of the file's contents, one `key value` pair a line. A line not
    /// understood is skipped.
    fn parse(text: &str) -> Self {
        let mut state = Self::default();
        for line in text.lines() {
            let Some((key, value)) = line.trim().split_once(' ') else {
                continue;
            };
            let value = value.trim();
            match key {
                "woi" => state.woi = parse_ymd(value).or(state.woi),
                "success" => state.success = parse_ymd(value).or(state.success),
                "attempt" => state.attempt = value.parse().ok().or(state.attempt),
                _ => {}
            }
        }
        state
    }

    fn render(&self) -> String {
        let mut text = String::new();
        if let Some(woi) = self.woi {
            text.push_str(&format!("woi {}\n", ymd(woi)));
        }
        if let Some(success) = self.success {
            text.push_str(&format!("success {}\n", ymd(success)));
        }
        if let Some(attempt) = self.attempt {
            text.push_str(&format!("attempt {attempt}\n"));
        }
        text
    }

    /// Write the record by rename, answering whether it landed.
    fn write(&self, path: &Path) -> bool {
        bravebot_agent::home::replace(path, self.render()).is_ok()
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_secs())
        .unwrap_or(0)
}

/// The UTC day `seconds` falls on, counted from 1970-01-01.
fn day_of(seconds: u64) -> i64 {
    (seconds / SECONDS_PER_DAY) as i64
}

/// The Monday on or before `day`. 1970-01-01 was a Thursday.
fn monday_of(day: i64) -> i64 {
    day - (day + 3).rem_euclid(7)
}

fn ymd(day: i64) -> String {
    let (year, month, date) = civil_from_days(day);
    format!("{year:04}-{month:02}-{date:02}")
}

/// A `YYYY-MM-DD` date, refused where it is not a real one.
fn parse_ymd(text: &str) -> Option<i64> {
    let mut parts = text.split('-');
    let (year, month, date) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() || year.len() != 4 || month.len() != 2 || date.len() != 2 {
        return None;
    }
    let (year, month, date): (i64, u32, u32) =
        (year.parse().ok()?, month.parse().ok()?, date.parse().ok()?);
    let day = days_from_civil(year, month, date);
    (civil_from_days(day) == (year, month, date)).then_some(day)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(text: &str) -> i64 {
        parse_ymd(text).unwrap_or_else(|| panic!("{text} is a date"))
    }

    fn fields() -> Fields {
        Fields {
            platform: "darwin",
            version: "1.2.3",
            install: "npm",
            surface: Surface::Tui,
            backend: Backend::Brave,
            premium: false,
        }
    }

    /// A released build sends to the default when no address is named, and to none when the one
    /// named is not text, rather than to the default the person meant to replace.
    #[cfg(unix)]
    #[test]
    fn a_released_build_refuses_an_address_that_is_not_text() {
        use std::os::unix::ffi::OsStrExt;
        let not_text = std::ffi::OsStr::from_bytes(b"\xff\xfe").to_os_string();
        assert_eq!(base_url_from(None, false), Some(BASE_URL.to_string()));
        assert_eq!(base_url_from(Some(not_text.clone()), false), None);
        assert_eq!(base_url_from(Some(not_text), true), None);
        assert_eq!(
            base_url_from(Some("http://example.test/ ".into()), false),
            Some("http://example.test".to_string())
        );
    }

    /// The first ping sets every flag.
    #[test]
    fn the_first_ping_sets_every_flag() {
        let flags = Flags::between(None, day("2026-11-11"));
        assert_eq!(
            flags,
            Flags {
                first: true,
                daily: true,
                weekly: true,
                monthly: true
            }
        );
    }

    /// A ping the next day in the same week and month is daily alone.
    #[test]
    fn the_next_day_in_the_same_week_is_daily_alone() {
        let flags = Flags::between(Some(day("2026-11-10")), day("2026-11-11"));
        assert_eq!(
            flags,
            Flags {
                first: false,
                daily: true,
                weekly: false,
                monthly: false
            }
        );
    }

    /// Sunday to Monday crosses an ISO week, and the last of a month to the first crosses a month.
    #[test]
    fn the_week_turns_on_monday_and_the_month_on_the_first() {
        // 2026-11-15 is a Sunday.
        let monday = Flags::between(Some(day("2026-11-15")), day("2026-11-16"));
        assert!(monday.weekly && !monday.monthly);
        let saturday = Flags::between(Some(day("2026-11-09")), day("2026-11-14"));
        assert!(!saturday.weekly);
        // 2026-12-01 is a Tuesday, so the week does not turn with the month.
        let first = Flags::between(Some(day("2026-11-30")), day("2026-12-01"));
        assert!(first.monthly && !first.weekly);
        // The same week number a year apart is a different week.
        let year = Flags::between(Some(day("2025-11-10")), day("2026-11-09"));
        assert!(year.weekly && year.monthly);
        // The year turning inside one ISO week: 2026-12-31 is a Thursday.
        let new_year = Flags::between(Some(day("2026-12-31")), day("2027-01-01"));
        assert!(!new_year.weekly && new_year.monthly);
    }

    /// The week of install is the Monday on or before the day, whatever day that is.
    #[test]
    fn woi_is_the_monday_on_or_before_the_day() {
        for text in ["2026-11-09", "2026-11-10", "2026-11-12", "2026-11-15"] {
            assert_eq!(ymd(monday_of(day(text))), "2026-11-09", "{text}");
        }
        assert_eq!(ymd(monday_of(day("2026-11-16"))), "2026-11-16");
        assert_eq!(ymd(monday_of(day("1970-01-01"))), "1969-12-29");
    }

    /// Only the three platforms the service takes are named, macOS as `darwin`.
    #[test]
    fn platforms_are_named_as_the_service_takes_them() {
        assert_eq!(platform("macos"), Some("darwin"));
        assert_eq!(platform("linux"), Some("linux"));
        assert_eq!(platform("windows"), Some("windows"));
        assert_eq!(platform("freebsd"), None);
    }

    /// A version is sent only as three numbers.
    #[test]
    fn only_a_three_number_version_is_sent() {
        for (version, strict) in [
            ("0.21.0", true),
            ("10.200.3000", true),
            ("0.21.0-beta.1", false),
            ("0.21", false),
            ("0.21.0.1", false),
            ("0..1", false),
            ("v0.21.0", false),
            ("", false),
        ] {
            assert_eq!(strict_version(version), strict, "{version:?}");
        }
    }

    /// Every word the service is sent for an open field is alphanumeric and ten bytes at most.
    #[test]
    fn every_open_field_word_is_short_and_alphanumeric() {
        let installs = [
            None,
            Some(Install::Npm),
            Some(Install::Script),
            Some(Install::PowerShell),
            Some(Install::Homebrew),
            Some(Install::Winget),
            Some(Install::Apt),
            Some(Install::Dnf),
        ]
        .map(install_word);
        let backends = [
            Backend::Brave,
            Backend::Bedrock,
            Backend::Chatgpt,
            Backend::Xai,
            Backend::Openai,
            Backend::Anthropic,
            Backend::Openrouter,
            Backend::Ollama,
            Backend::Lmstudio,
            Backend::Llamaserver,
            Backend::Other,
            Backend::None,
        ]
        .map(Backend::as_str);
        for word in installs.iter().chain(backends.iter()) {
            assert!(
                !word.is_empty()
                    && word.len() <= 10
                    && word.bytes().all(|byte| byte.is_ascii_alphanumeric()),
                "{word:?}"
            );
        }
    }

    /// A gateway's id is sent only when it is one of the known ones.
    #[test]
    fn a_gateway_id_not_known_is_other() {
        assert_eq!(Backend::of_gateway("openai"), Backend::Openai);
        assert_eq!(Backend::of_gateway("llama-server"), Backend::Llamaserver);
        assert_eq!(Backend::of_gateway("lmstudio"), Backend::Lmstudio);
        assert_eq!(Backend::of_gateway("my-company-proxy"), Backend::Other);
    }

    /// The state file reads back what it wrote, and a line it does not understand is skipped.
    #[test]
    fn the_state_file_round_trips() {
        let state = State {
            woi: Some(day("2026-11-09")),
            success: Some(day("2026-11-11")),
            attempt: Some(1_762_876_800),
        };
        assert_eq!(State::parse(&state.render()), state);
        assert_eq!(
            State::parse("woi 2026-11-31\nsuccess yesterday\nattempt soon\nnoise\n"),
            State::default()
        );
    }

    /// The URL holds every field the service requires, in order.
    #[test]
    fn the_url_holds_every_field() {
        let flags = Flags::between(Some(day("2026-11-10")), day("2026-11-11"));
        assert_eq!(
            url("http://x", &fields(), flags, day("2026-11-09")),
            "http://x/1/usage/bravebot?platform=darwin&version=1.2.3&install=npm&surface=tui\
             &backend=brave&premium=false&daily=true&weekly=false&monthly=false&first=false\
             &woi=2026-11-09"
        );
    }

    /// A failure is retried an hour later and not before, and an attempt in the future does not
    /// hold the ping back.
    #[test]
    fn a_failure_waits_an_hour() {
        let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let now = day("2026-11-11") as u64 * SECONDS_PER_DAY + 10;
        let failed = send_in(directory.path(), &fields(), now, "http://x", |_| false);
        assert_eq!(failed, Outcome::Failed);
        let soon = send_in(
            directory.path(),
            &fields(),
            now + RETRY_AFTER - 1,
            "http://x",
            |_| true,
        );
        assert_eq!(soon, Outcome::Waiting);
        let later = send_in(
            directory.path(),
            &fields(),
            now + RETRY_AFTER,
            "http://x",
            |_| true,
        );
        assert_eq!(later, Outcome::Sent);

        let back = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let _ = send_in(back.path(), &fields(), now, "http://x", |_| false);
        let earlier = send_in(back.path(), &fields(), now - 10, "http://x", |_| true);
        assert_eq!(earlier, Outcome::Sent);
    }

    /// The week of install is kept from the first accepted ping, and the first flag stays on until
    /// a ping is accepted.
    #[test]
    fn woi_is_set_once_and_first_holds_until_accepted() {
        let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let monday = day("2026-11-09") as u64 * SECONDS_PER_DAY;
        let mut sent = Vec::new();
        let _ = send_in(directory.path(), &fields(), monday, "", |_| false);
        let wednesday = monday + 2 * SECONDS_PER_DAY;
        let _ = send_in(directory.path(), &fields(), wednesday, "", |url| {
            sent.push(url.to_string());
            true
        });
        let next_week = monday + 8 * SECONDS_PER_DAY;
        let _ = send_in(directory.path(), &fields(), next_week, "", |url| {
            sent.push(url.to_string());
            true
        });
        assert_eq!(sent.len(), 2);
        assert!(
            sent[0].contains("&first=true&woi=2026-11-09"),
            "{}",
            sent[0]
        );
        assert!(sent[1].contains("&weekly=true"), "{}", sent[1]);
        assert!(
            sent[1].contains("&first=false&woi=2026-11-09"),
            "{}",
            sent[1]
        );
    }

    /// A ping that fails on a Sunday does not date the install to that week when the first one
    /// accepted is on the Monday after.
    #[test]
    fn woi_is_the_week_of_the_first_accepted_ping() {
        let directory = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let sunday = day("2026-11-15") as u64 * SECONDS_PER_DAY;
        assert_eq!(
            send_in(directory.path(), &fields(), sunday, "", |_| false),
            Outcome::Failed
        );
        let mut sent = String::new();
        let monday = sunday + SECONDS_PER_DAY;
        let outcome = send_in(directory.path(), &fields(), monday, "", |url| {
            sent = url.to_string();
            true
        });
        assert_eq!(outcome, Outcome::Sent);
        assert!(sent.contains("&first=true&woi=2026-11-16"), "{sent}");
    }
}
