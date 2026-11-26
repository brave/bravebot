//! A Seatbelt profile as a list of rules, and the one place they are written as text.
//!
//! `bravebot-sandbox` works out from a policy which paths are granted and refused, in which order,
//! and which names are protected, and hands them to [`rules`] as [`Parts`]. [`rules`] puts them in
//! a profile with the rules every profile holds, and [`write`] is the only thing that turns a rule
//! into profile syntax, so every value in one is written through
//! [`crate::quote::seatbelt_literal`] (SANDBOX-4). Verus proves what [`rules`] ensures. What the
//! platform does with the text is not proved.

use crate::quote::seatbelt_literal;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

/// Whether a rule lets the operation through or refuses it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(verus_keep_ghost, verus_verify, derive(Structural))]
pub enum Action {
    Allow,
    Deny,
}

/// The operation a rule is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(verus_keep_ghost, verus_verify, derive(Structural))]
pub enum Operation {
    /// Every operation, which only the rule that opens the profile names.
    Default,
    ProcessExec,
    ProcessInfo,
    ProcessInfoPidinfo,
    ProcessFork,
    SysctlRead,
    MachLookup,
    FileRead,
    FileReadMetadata,
    FileWrite,
    FileWriteUnlink,
    FileWriteCreate,
    NetworkOutbound,
    NetworkInbound,
    Signal,
}

/// What a rule matches.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    verus_keep_ghost,
    verus_verify,
    verifier::allow(autoderive_clone_without_spec)
)]
pub enum Target {
    /// Everything the operation reaches.
    Any,
    /// A path and everything beneath it.
    Subpath(String),
    /// One path.
    Literal(String),
    /// Every path the regular expression matches.
    Regex(String),
    /// Every path a regular expression fixed in this crate's caller matches, written as Seatbelt's
    /// raw string `#"..."`. Only a constant is written this way, since a raw string has no escape.
    RawRegex(&'static str),
    /// Any of these Mach service names.
    GlobalNames(Vec<String>),
    /// Any of these sysctl names.
    SysctlNames(Vec<String>),
    /// The calling process.
    SelfProcess,
    /// Any process started under this profile.
    SameSandbox,
    /// A remote address, `localhost:<port>` or any where `None`.
    RemoteIp(Option<String>),
    /// A local address.
    LocalIp(String),
}

/// One rule of a profile.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    verus_keep_ghost,
    verus_verify,
    verifier::allow(autoderive_clone_without_spec)
)]
pub struct Rule {
    pub action: Action,
    pub operation: Operation,
    pub target: Target,
}

/// A path a row grants or refuses, for the section of the profile it is in.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    verus_keep_ghost,
    verus_verify,
    verifier::allow(autoderive_clone_without_spec)
)]
pub struct Row {
    pub path: String,
    pub allows: bool,
}

/// The kind of write a [`Hold`] is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(verus_keep_ghost, verus_verify, derive(Structural))]
pub enum Write {
    /// Any write, `file-write*`.
    Any,
    /// Removing or renaming a name away, `file-write-unlink`.
    Unlink,
    /// Making a name, `file-write-create`.
    Create,
}

/// A rule about writing that keeps a protected name or a refused path in place, which the
/// profile holds after the write rows.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    verus_keep_ghost,
    verus_verify,
    verifier::allow(autoderive_clone_without_spec)
)]
pub struct Hold {
    pub action: Action,
    pub write: Write,
    pub target: Target,
}

/// What `bravebot-sandbox` works out from a policy, in the order the profile holds each part.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    verus_keep_ghost,
    verus_verify,
    verifier::allow(autoderive_clone_without_spec)
)]
pub struct Parts {
    /// The read rows and refusals, from the widest path to the narrowest.
    pub reads: Vec<Row>,
    /// The write rows and refusals, from the widest path to the narrowest.
    pub writes: Vec<Row>,
    /// The rules about writing that follow the write rows.
    pub holds: Vec<Hold>,
    /// The paths whose sockets a connect may reach, from the widest to the narrowest. Written only
    /// where `network` is set.
    pub sockets: Vec<Row>,
    /// Whether the profile allows egress.
    pub network: bool,
    /// The one address egress is held to, `localhost:<port>`, where it is held to one.
    pub egress_to: Option<String>,
    /// Whether a process may listen on and connect to the loopback address.
    pub loopback: bool,
    /// Whether a process may start another.
    pub subprocesses: bool,
}

/// The Mach services every confined process may look up (SANDBOX-31). Each answers a question
/// about the machine (accounts, groups, network configuration, log and notification delivery) or
/// is the keychain service the base reads a keychain file for. None starts a program, reads or
/// writes the pasteboard, or opens a file or URL on the caller's behalf, which is what
/// LaunchServices, the pasteboard and launchd do, so a lookup of any of those is refused.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[expect(
    clippy::redundant_static_lifetimes,
    reason = "Verus reads a constant's type only with its lifetime written"
)]
pub const MACH_SERVICES: [&'static str; 8] = [
    "com.apple.SecurityServer",
    "com.apple.SystemConfiguration.DNSConfiguration",
    "com.apple.SystemConfiguration.configd",
    "com.apple.bsd.dirhelper",
    "com.apple.logd",
    "com.apple.system.notification_center",
    "com.apple.system.opendirectoryd.libinfo",
    "com.apple.system.opendirectoryd.membership",
];

/// The certificate trust services, looked up only by a process that may reach the network
/// (SANDBOX-31). Trust evaluation makes these fetch an issuer from the certificate's Authority
/// Information Access URL from their own unconfined process, so a profile with no egress that
/// allowed them would let the process make a request to any URL it names.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[expect(
    clippy::redundant_static_lifetimes,
    reason = "Verus reads a constant's type only with its lifetime written"
)]
pub const TRUST_SERVICES: [&'static str; 2] = ["com.apple.trustd", "com.apple.trustd.agent"];

/// The machine-information queries a process may make: no process arguments or environment
/// query is among them.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[expect(
    clippy::redundant_static_lifetimes,
    reason = "Verus reads a constant's type only with its lifetime written"
)]
pub const SYSCTL_NAMES: [&'static str; 10] = [
    "hw.machine",
    "hw.memsize",
    "hw.ncpu",
    "hw.pagesize",
    "hw.pagesize_compat",
    "kern.argmax",
    "kern.hostname",
    "kern.osrelease",
    "kern.ostype",
    "kern.version",
];

/// The platform resolver's socket, which every host name lookup goes through.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[expect(
    clippy::redundant_static_lifetimes,
    reason = "Verus reads a constant's type only with its lifetime written"
)]
pub const RESOLVER: &'static str = "/private/var/run/mDNSResponder";

/// Any port on this machine's loopback address.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[expect(
    clippy::redundant_static_lifetimes,
    reason = "Verus reads a constant's type only with its lifetime written"
)]
pub const LOOPBACK: &'static str = "localhost:*";

impl Action {
    fn word(self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Deny => "deny",
        }
    }
}

impl Write {
    /// The operation a rule about this write is about.
    fn operation(self) -> Operation {
        match self {
            Self::Any => Operation::FileWrite,
            Self::Unlink => Operation::FileWriteUnlink,
            // Making a name, which a refusal names to keep a refused path from being moved there.
            Self::Create => Operation::FileWriteCreate,
        }
    }
}

impl Operation {
    fn word(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::ProcessExec => "process-exec",
            Self::ProcessInfo => "process-info*",
            Self::ProcessInfoPidinfo => "process-info-pidinfo",
            Self::ProcessFork => "process-fork",
            Self::SysctlRead => "sysctl-read",
            Self::MachLookup => "mach-lookup",
            Self::FileRead => "file-read*",
            Self::FileReadMetadata => "file-read-metadata",
            Self::FileWrite => "file-write*",
            Self::FileWriteUnlink => "file-write-unlink",
            Self::FileWriteCreate => "file-write-create",
            Self::NetworkOutbound => "network-outbound",
            Self::NetworkInbound => "network-inbound",
            Self::Signal => "signal",
        }
    }
}

/// The rule as one line of profile syntax, ending in a newline. Every value a target holds is
/// written through [`seatbelt_literal`], so none is read as syntax.
#[cfg_attr(verus_keep_ghost, verifier::external)]
fn line(rule: &Rule) -> String {
    let head = format!("({} {}", rule.action.word(), rule.operation.word());
    let tail = match &rule.target {
        Target::Any => String::new(),
        Target::Subpath(path) => format!(" (subpath {})", seatbelt_literal(path)),
        Target::Literal(path) => format!(" (literal {})", seatbelt_literal(path)),
        Target::Regex(pattern) => format!(" (regex {})", seatbelt_literal(pattern)),
        Target::RawRegex(pattern) => format!(" (regex #\"{pattern}\")"),
        Target::GlobalNames(names) => names
            .iter()
            .map(|name| format!(" (global-name {})", seatbelt_literal(name)))
            .collect(),
        Target::SysctlNames(names) => names
            .iter()
            .map(|name| format!(" (sysctl-name {})", seatbelt_literal(name)))
            .collect(),
        Target::SelfProcess => " (target self)".to_string(),
        Target::SameSandbox => " (target same-sandbox)".to_string(),
        Target::RemoteIp(None) => " (remote ip)".to_string(),
        Target::RemoteIp(Some(address)) => format!(" (remote ip {})", seatbelt_literal(address)),
        Target::LocalIp(address) => format!(" (local ip {})", seatbelt_literal(address)),
    };
    format!("{head}{tail})\n")
}

/// `rules` as a Seatbelt profile: its version line, then each rule on a line of its own in order.
#[cfg_attr(verus_keep_ghost, verifier::external)]
pub fn write(rules: &[Rule]) -> String {
    let mut out = String::from("(version 1)\n");
    for rule in rules {
        out.push_str(&line(rule));
    }
    out
}

/// `allows` as the action a row takes.
fn action(allows: bool) -> Action {
    if allows { Action::Allow } else { Action::Deny }
}

/// Each name of `list` as a string, in order.
fn names<const N: usize>(list: &[&str; N]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut at = 0;
    while at < N {
        out.push(list[at].to_string());
        at += 1;
    }
    out
}

/// The Mach services the profile for `parts` allows a lookup of: the fixed list, and the trust
/// services only where the network is allowed.
fn services(parts: &Parts) -> Vec<String> {
    let mut out = names(&MACH_SERVICES);
    if !parts.network {
        return out;
    }
    let mut at = 0;
    while at < TRUST_SERVICES.len() {
        out.push(TRUST_SERVICES[at].to_string());
        at += 1;
    }
    out
}

/// A rule about everything `operation` reaches.
fn any(action: Action, operation: Operation) -> Rule {
    Rule {
        action,
        operation,
        target: Target::Any,
    }
}

/// Push a rule taking `operation` for each of `rows` onto `out`, in order.
fn push_rows(out: &mut Vec<Rule>, rows: &[Row], operation: Operation) {
    let mut at = 0;
    while at < rows.len() {
        let row = &rows[at];
        let rule = Rule {
            action: action(row.allows),
            operation,
            target: Target::Subpath(row.path.clone()),
        };
        out.push(rule);
        at += 1;
    }
}

/// The rules of the profile for `parts`, in the order the profile holds them.
///
/// The profile refuses everything first and then adds what the policy grants. The last matching
/// rule wins, so each section follows the rules it narrows. What a rule may be is
/// `crate::spec::may_hold`, and Verus proves every rule after the first is one.
pub fn rules(parts: &Parts) -> Vec<Rule> {
    let mut out = Vec::new();
    out.push(any(Action::Deny, Operation::Default));
    // Without these the process cannot start at all: the loader must exec the binary and the
    // system libraries look up the services in `MACH_SERVICES`, and `TRUST_SERVICES` where the
    // network is allowed. Those grant no filesystem or network reach of their own, and no other
    // service is reachable.
    out.push(any(Action::Allow, Operation::ProcessExec));
    // KERN_PROCARGS2 can be authorised through either sysctl-read or process-info*. Refuse both
    // routes: emptying a child's environment does not erase the parent's exec-time environment.
    // Only named machine-information queries are allowed.
    out.push(any(Action::Deny, Operation::ProcessInfo));
    // System tools query their own pidinfo during startup. This exception cannot name the parent
    // or any other process, including another process in the same sandbox.
    let own = Rule {
        action: Action::Allow,
        operation: Operation::ProcessInfoPidinfo,
        target: Target::SelfProcess,
    };
    out.push(own);
    // CPU count and page size serve language runtimes (the numeric page-size query resolves to
    // pagesize_compat). hw.memsize is the physical memory size, which sort reads to size its
    // buffer. kern.argmax is the one system-wide argument-size limit that xargs, find -exec {} +
    // and getconf ARG_MAX read. The remaining names are uname's fields, used by Python's package
    // platform detection. No process arguments or environment query is in this list.
    let sysctls = names(&SYSCTL_NAMES);
    let sysctl = Rule {
        action: Action::Allow,
        operation: Operation::SysctlRead,
        target: Target::SysctlNames(sysctls),
    };
    out.push(sysctl);
    let lookup = Rule {
        action: Action::Allow,
        operation: Operation::MachLookup,
        target: Target::GlobalNames(services(parts)),
    };
    out.push(lookup);
    push_files(&mut out, parts);
    push_network(&mut out, parts);
    out
}

/// Push the rules about files onto `out`: the root directory, the read rows, the write rows and
/// what holds them in place, and the look at any path.
fn push_files(out: &mut Vec<Rule>, parts: &Parts) {
    // The dynamic loader reads the root directory entry itself, which a subpath grant for e.g.
    // /usr does not cover. Without this the process dies with SIGABRT before main runs, which
    // looks like a mysterious crash rather than a denied read. Seatbelt has no operation that
    // separates opening `/` from listing it, so this row also lets a process list the entries of
    // `/`. That is the one directory listing SANDBOX-13 allows outside the grants; no file
    // contents are readable through it.
    let root = Rule {
        action: Action::Allow,
        operation: Operation::FileRead,
        target: Target::Literal("/".to_string()),
    };
    out.push(root);

    push_rows(out, &parts.reads, Operation::FileRead);
    push_rows(out, &parts.writes, Operation::FileWrite);
    let mut at = 0;
    while at < parts.holds.len() {
        let hold = &parts.holds[at];
        let rule = Rule {
            action: hold.action,
            operation: hold.write.operation(),
            target: hold.target.clone(),
        };
        out.push(rule);
        at += 1;
    }
    // Looking at a path answers what it is and not what it holds: this allows stat and readlink,
    // and neither a file's contents nor a directory's entries. Without it every path outside the
    // grants answers "refused" where it would answer "not there", and a search of PATH stops at
    // the first such entry rather than going on to the next, as node's does when it starts
    // anything; and node resolves its own script through each directory above it. Landlock
    // restricts no look at all, so this is the reach Linux already gives.
    out.push(any(Action::Allow, Operation::FileReadMetadata));
}

/// Push the rules about the network, starting another process and signals onto `out`.
fn push_network(out: &mut Vec<Rule>, parts: &Parts) {
    // A connect to a unix socket is network-outbound on Seatbelt, so the socket rows are written
    // only where egress is. Host names resolve through the platform resolver's socket, where Linux
    // resolves over IP, and its row follows the refusals so that none of them takes it away.
    //
    // Egress limited to a port is to that port on the loopback address and to nothing else by IP,
    // and the resolver is left out: the proxy on that port resolves the names it is asked for.
    if parts.network {
        push_rows(out, &parts.sockets, Operation::NetworkOutbound);
        let to = parts.egress_to.clone();
        // A line of its own, so the mutation in verify/mutations.txt can rewrite it.
        let any_address = to.is_none();
        let egress = Rule {
            action: Action::Allow,
            operation: Operation::NetworkOutbound,
            target: Target::RemoteIp(to),
        };
        out.push(egress);
        if any_address {
            let resolver = Rule {
                action: Action::Allow,
                operation: Operation::NetworkOutbound,
                target: Target::Literal(RESOLVER.to_string()),
            };
            out.push(resolver);
        }
    }
    // Listening is `network-inbound`, which covers binding the port as well as accepting on it.
    // Each row names this machine's loopback address, so a stage that listens is reached from
    // this machine and a stage that connects goes to this machine. A stage held to the proxy's
    // port keeps that one port: another local port could be a service that carries it past the
    // host list.
    if parts.loopback {
        let listen = Rule {
            action: Action::Allow,
            operation: Operation::NetworkInbound,
            target: Target::LocalIp(LOOPBACK.to_string()),
        };
        out.push(listen);
        if parts.egress_to.is_none() {
            let connect = Rule {
                action: Action::Allow,
                operation: Operation::NetworkOutbound,
                target: Target::RemoteIp(Some(LOOPBACK.to_string())),
            };
            out.push(connect);
        }
    }
    if parts.subprocesses {
        out.push(any(Action::Allow, Operation::ProcessFork));
    }
    // Without a signal row a process cannot stop a child it started, and a test runner or a
    // `timeout` waits on it for good. `same-sandbox` reaches what this profile started and no
    // process outside it: `pgrp` would reach the session, which shares the process group.
    let signal = Rule {
        action: Action::Allow,
        operation: Operation::Signal,
        target: Target::SameSandbox,
    };
    out.push(signal);
}
