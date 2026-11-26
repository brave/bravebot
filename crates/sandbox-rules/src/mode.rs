//! How much a program `run` starts is held to, as a person chose it, and whether the network is
//! open to it (SANDBOX-20, SANDBOX-22).

use std::fmt;

/// What `workspace-write` was called before it was renamed.
const RENAMED_WORKSPACE_WRITE: &str = "standard";

/// The bundle of rows a confined program runs under.
///
/// The modes are not a line. `strict` reads less than `read-only` and writes the session's
/// directories, which `read-only` does not, so neither is the stricter of the two. A caller asks
/// [`SandboxMode::is_looser_than`] and never compares two modes by position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SandboxMode {
    /// The deny-by-default profile: the base, the list the program's binary brings, and the scope
    /// its argv names. The machine is not read.
    Strict,
    /// The machine except the credential table (SANDBOX-12), read, with nothing in the session's
    /// directories or the toolchain caches written.
    ReadOnly,
    /// The machine except the credential table (SANDBOX-12), with the session's directories
    /// writable. What a session runs under unless a person chose otherwise.
    #[default]
    WorkspaceWrite,
    /// No confinement.
    Off,
}

impl SandboxMode {
    /// Every mode, in the order they are listed to a person.
    pub const ALL: [Self; 4] = [
        Self::Strict,
        Self::ReadOnly,
        Self::WorkspaceWrite,
        Self::Off,
    ];

    /// The word a person types for this mode, which is also how every report spells it.
    pub fn name(self) -> &'static str {
        match self {
            Self::Strict => "strict",
            Self::ReadOnly => "read-only",
            Self::WorkspaceWrite => "workspace-write",
            Self::Off => "off",
        }
    }

    /// The network a stage under this mode is held to, given the session's setting: `read-only`
    /// closes it whatever the setting says, because it holds back what a program changes off this
    /// machine as well as on it (SANDBOX-22). The one place that rule is decided, so what the
    /// sandbox enforces and what the screens report cannot drift apart.
    pub fn network_in_force(self, setting: Network) -> Network {
        match self {
            Self::ReadOnly => Network::Closed,
            _ => setting,
        }
    }

    /// Whether a stage under this mode is confined by a profile at all. Only `off` is not.
    pub fn confines(self) -> bool {
        self != Self::Off
    }

    /// Whether a stage under this mode reads the machine except the credential table
    /// (SANDBOX-12), on a platform that is not Windows and in a session that names a home
    /// directory. `strict` never does, and on Windows and without a home no mode does, since the
    /// credential rows are rows under the home and a read of the whole machine with none to
    /// subtract is the one grant that must not be made (SANDBOX-18).
    pub fn reads_the_machine(self, windows: bool, has_home: bool) -> bool {
        !windows && has_home && self != Self::Strict
    }

    /// Whether a stage under this mode writes the directories the session was opened on and its
    /// scratch directory. `read-only` does not (SANDBOX-22).
    pub fn writes_the_session(self) -> bool {
        self != Self::ReadOnly
    }

    /// Whether a stage under this mode writes every toolchain's cache: only where it reads the
    /// machine, and not under `read-only`, which writes no cache (SANDBOX-15, SANDBOX-22).
    pub fn writes_every_cache(self, reads_the_machine: bool) -> bool {
        reads_the_machine && self != Self::ReadOnly
    }

    /// Whether a stage under this mode is given a toolchain's cache to write as well as its list
    /// to read. `read-only` gives the list alone (SANDBOX-22).
    pub fn writes_a_toolchain_cache(self) -> bool {
        self != Self::ReadOnly
    }

    /// Whether a grant for writing a path a person let programs reach gives its write as well as
    /// its read. Under `read-only` it gives the read alone, as a new request for it would
    /// (SANDBOX-22, SANDBOX-28).
    pub fn writes_what_was_granted(self) -> bool {
        self != Self::ReadOnly
    }

    /// Whether a stage under this mode is given every path a request for writing would be granted,
    /// without the request: only under `workspace-write`, where the person allowed it and the
    /// machine is read (SANDBOX-28). `strict` asked for less reach than the default, and `off` has
    /// no profile.
    pub fn writes_without_a_request(self, allowed: bool, reads_the_machine: bool) -> bool {
        allowed && self == Self::WorkspaceWrite && reads_the_machine
    }

    /// Whether a stage under this mode may keep the network for a reason it carries where the
    /// session closed it: a toolchain that fetches, a credential scope, or a program that talks to a
    /// remote. `read-only` gives no stage a reason (SANDBOX-20, SANDBOX-22).
    pub fn keeps_the_network_for_a_reason(self) -> bool {
        self != Self::ReadOnly
    }

    /// Whether a stage under this mode may be lent the remote scope, which is how it pushes and
    /// which writes `~/.ssh/known_hosts`. `read-only` lends it to no stage (SANDBOX-16, SANDBOX-22).
    pub fn lends_the_remote_scope(self) -> bool {
        self != Self::ReadOnly
    }

    /// Whether a stage under this mode must be refused on a kernel that cannot keep it from a unix
    /// socket that has a path: `strict` and `read-only`, which promise less reach than that kernel
    /// gives (SANDBOX-3, SANDBOX-22).
    pub fn needs_sockets_governed(self) -> bool {
        matches!(self, Self::Strict | Self::ReadOnly)
    }

    /// The mode a word names, or `None`.
    ///
    /// Exact and lower case, as the permission modes are read: a value that is almost a mode is
    /// not obeyed as one, because the modes differ in what a program may reach and a guess would
    /// choose for the person. `standard`, the word `workspace-write` replaced, still names it, so a
    /// settings file or a managed pin written before the rename keeps its meaning.
    pub fn parse(word: &str) -> Option<Self> {
        if word == RENAMED_WORKSPACE_WRITE {
            return Some(Self::WorkspaceWrite);
        }
        Self::ALL.into_iter().find(|mode| mode.name() == word)
    }

    /// Whether a program under this mode can read or write more than under `other`.
    ///
    /// On Linux and macOS `strict` and `read-only` are each looser than the other: `read-only`
    /// reads the machine, and `strict` writes the session's directories. A managed pin of one
    /// therefore refuses the other. On Windows no mode but `off` reads the machine, so `read-only`
    /// is no looser than `strict` and a pin of `strict` keeps it.
    pub fn is_looser_than(self, other: Self) -> bool {
        self.looser_than(other).is_some()
    }

    /// Which of reading and writing a program under this mode does more of than under `other`, or
    /// `None` where it does no more of either.
    ///
    /// What a refusal under a managed pin gives as its reason. The modes are not a line, so
    /// "looser" alone does not tell a person under a `strict` pin why `read-only` is refused.
    pub fn looser_than(self, other: Self) -> Option<Looser> {
        self.looser_than_on(other, cfg!(windows))
    }

    /// [`Self::looser_than`] for a named platform, so the Windows ranking is testable on a host
    /// that is not Windows.
    pub fn looser_than_on(self, other: Self, windows: bool) -> Option<Looser> {
        match (
            self.reads(windows) > other.reads(windows),
            self.writes() > other.writes(),
        ) {
            (true, true) => Some(Looser::Both),
            (true, false) => Some(Looser::Reads),
            (false, true) => Some(Looser::Writes),
            (false, false) => None,
        }
    }

    /// How much of the machine a program can read, as a rank. On Windows only `off` reads it: the
    /// other three build a policy that grants no read of the machine.
    fn reads(self, windows: bool) -> u8 {
        match (self, windows) {
            (Self::Off, true) => 1,
            (_, true) => 0,
            (Self::Strict, false) => 0,
            (Self::ReadOnly | Self::WorkspaceWrite, false) => 1,
            (Self::Off, false) => 2,
        }
    }

    /// How much a program can write, as a rank.
    fn writes(self) -> u8 {
        match self {
            Self::ReadOnly => 0,
            Self::Strict => 1,
            Self::WorkspaceWrite => 2,
            Self::Off => 3,
        }
    }
}

/// What one mode does more of than another ([`SandboxMode::looser_than`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Looser {
    /// It reads more of the machine and writes no more.
    Reads,
    /// It writes more and reads no more of the machine.
    Writes,
    /// It reads more of the machine and writes more.
    Both,
}

impl fmt::Display for SandboxMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// What a session decided about the network for the programs it starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Network {
    /// Every stage keeps the egress the base grants.
    #[default]
    Open,
    /// A stage has no egress unless it carries a reason to.
    Closed,
}

impl Network {
    /// The word a settings file, a flag and a report spell it with.
    pub fn name(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Closed => "closed",
        }
    }

    /// The setting a word names, or `None` for a word that names neither.
    pub fn parse(word: &str) -> Option<Self> {
        match word.trim() {
            "open" => Some(Self::Open),
            "closed" => Some(Self::Closed),
            _ => None,
        }
    }

    /// Whether this is the setting that removes reach.
    pub fn is_closed(self) -> bool {
        self == Self::Closed
    }
}
