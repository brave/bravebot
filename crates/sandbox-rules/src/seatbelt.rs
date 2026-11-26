//! A Seatbelt profile as a list of rules, and the one place they are written as text.
//!
//! `bravebot-sandbox` decides what a profile holds: which paths are granted and refused, in which
//! order, and which names are protected. It hands this crate the list, and [`write`] is the only
//! thing that turns a rule into profile syntax, so every value in one is written through
//! [`crate::quote::seatbelt_literal`] (SANDBOX-4). What Verus proves of a list is in
//! `crate::spec`. What the platform does with the text is not proved.

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

impl Action {
    fn word(self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Deny => "deny",
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
