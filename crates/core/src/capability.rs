//! Capabilities and the labels their output carries.
//!
//! A capability names a class of observation or effect. Every capability that
//! *produces* data declares the label that data arrives with, so a fetcher cannot
//! decide its own output is trustworthy.
//!
//! The set here is deliberately coding-shaped. Anything domain-specific belongs
//! behind MCP rather than in this enum. See [`Capability::WebFetch`] for the one
//! general-purpose fetch primitive.

use crate::label::Label;
use std::fmt;

/// The local name a person gave one declared MCP server.
///
/// A name a person typed, and never one a server reported about itself: what a server calls
/// itself is display text, so it cannot become an identifier a grant is written against.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ServerAlias(String);

impl ServerAlias {
    pub fn new(alias: impl Into<String>) -> Self {
        Self(alias.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ServerAlias {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A named capability. Holding the corresponding [`CapabilityToken`] is what permits
/// an operation; the enum itself is just an identifier.
///
/// Not `Copy`, because one variant names the server it is about.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Capability {
    /// Read a file from the workspace.
    ///
    /// Output is untrusted: a file may contain anything, including text fetched from
    /// the network by an earlier step. It is private because workspace contents are
    /// the user's and must not leave without declassification.
    FileRead,
    /// Write a file in the workspace. An effect, not an observation.
    FileWrite,
    /// Execute a subprocess. The most dangerous capability: its argument is
    /// simultaneously destination and payload, so it cannot be split into routing and
    /// content the way a file write can.
    ShellExec,
    /// Read repository state: log, diff, status.
    GitRead,
    /// Mutate repository state: commit, branch, tag.
    GitWrite,
    /// Fetch a URL. Output is untrusted and public: it is attacker-influenceable but
    /// carries no confidentiality of ours.
    WebFetch,
    /// Call a tool on the MCP server declared under this alias. Output is untrusted;
    /// confidentiality depends on the server, so this label is the conservative floor and
    /// a server may raise it.
    ///
    /// The alias is part of what the capability *is*, rather than an argument checked
    /// beside it. One variant covering the protocol would make adding a second server a
    /// widening of what the first may be asked to do, which is the opposite of what adding
    /// a server should mean. Naming the server here instead means a grant answers about
    /// that server alone, and a server nobody granted anything to is reachable by nobody.
    McpCall(ServerAlias),
    /// Ask a language server where a symbol is. Separate from [`Capability::FileRead`]
    /// because they are not the same act: a read opens one named file, while a server
    /// reads the whole tree and the dependency sources beside it and keeps a process
    /// alive doing so. A set that could not tell those apart could not describe the
    /// narrower one.
    ///
    /// Output is untrusted and private, exactly as a file read is: what a server reports
    /// was computed from files that may contain anything. That the *locations* in it are
    /// reportable anyway is LSP-3's separate argument about structure, made where the
    /// answer is rendered rather than by a label here.
    LanguageServer,
}

impl Capability {
    /// Every variant, in declaration order.
    ///
    /// Nothing in Rust enumerates an enum, so this is written out, and it is one list to keep in
    /// step rather than one per test that needs the whole set. What forces a new variant to be
    /// accounted for is the exhaustive match a test walking this makes against it, which stops
    /// compiling until somebody says what the new capability observes.
    ///
    /// The alias in the MCP variant stands for every server. What a capability observes,
    /// whether it is an effect, and whether it needs the network are properties of the
    /// protocol, and those are the questions a walk over this list asks. Which server a
    /// grant is about is [`CapabilitySet`]'s question rather than this one.
    pub fn all() -> [Self; 8] {
        [
            Self::FileRead,
            Self::FileWrite,
            Self::ShellExec,
            Self::GitRead,
            Self::GitWrite,
            Self::WebFetch,
            Self::McpCall(ServerAlias::new("any")),
            Self::LanguageServer,
        ]
    }

    /// The label data produced by this capability arrives with.
    ///
    /// `None` for pure effects, which produce no observation to label.
    pub fn output_label(&self) -> Option<Label> {
        match self {
            // Workspace content is ours (private) and may contain anything (untrusted). A
            // language server's answer was computed from that same content, so it arrives on
            // the same footing.
            Self::FileRead | Self::GitRead | Self::LanguageServer => {
                Some(Label::untrusted_private())
            }
            // Remote content is attacker-influenceable but not confidential to us.
            Self::WebFetch | Self::McpCall(_) => Some(Label::untrusted_public()),
            // Effects produce no labelled observation.
            Self::FileWrite | Self::GitWrite => None,
            // Command output can contain anything the workspace contains.
            Self::ShellExec => Some(Label::untrusted_private()),
        }
    }

    /// Whether this capability changes the world, as opposed to observing it.
    ///
    /// Effects are what the action gates guard; observations only need labelling.
    pub fn is_effect(&self) -> bool {
        matches!(
            self,
            Self::FileWrite | Self::GitWrite | Self::ShellExec | Self::McpCall(_)
        )
    }

    /// Whether this capability requires network egress, and so must pass through the
    /// single egress chokepoint.
    pub fn needs_network(&self) -> bool {
        matches!(self, Self::WebFetch | Self::McpCall(_))
    }

    /// The class of effect, which is what this capability *is* rather than what one
    /// instance of it is called. An MCP call's server is not in it: see [`fmt::Display`],
    /// which is what names a capability to a person and to the trail.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::FileRead => "file_read",
            Self::FileWrite => "file_write",
            Self::ShellExec => "shell_exec",
            Self::GitRead => "git_read",
            Self::GitWrite => "git_write",
            Self::WebFetch => "web_fetch",
            Self::McpCall(_) => "mcp_call",
            Self::LanguageServer => "language_server",
        }
    }
}

impl fmt::Display for Capability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            // The server is part of what this capability is, so it is part of what it is
            // called. A refusal or a trail line naming only the protocol would not say
            // which server was asked for, and with a grant per server that is the whole
            // of what the reader needs.
            Self::McpCall(alias) => write!(f, "mcp_call:{alias}"),
            other => f.write_str(other.as_str()),
        }
    }
}

/// Proof that a capability was granted.
///
/// Cannot be constructed outside this crate, so downstream code cannot forge a grant
/// It must receive one from a [`CapabilitySet`] built by the policy layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityToken {
    capability: Capability,
}

impl CapabilityToken {
    pub(crate) fn mint(capability: Capability) -> Self {
        Self { capability }
    }

    pub fn capability(&self) -> &Capability {
        &self.capability
    }
}

/// The capabilities granted for one run.
///
/// Deliberately immutable once built: a run cannot acquire new capabilities partway
/// through, which is what stops a compromised step from escalating.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CapabilitySet {
    granted: Vec<Capability>,
}

impl CapabilitySet {
    /// An empty set. Grants nothing, which is the right default for untrusted work.
    pub fn none() -> Self {
        Self::default()
    }

    pub fn contains(&self, capability: &Capability) -> bool {
        self.granted.contains(capability)
    }

    pub fn is_empty(&self) -> bool {
        self.granted.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = Capability> + '_ {
        self.granted.iter().cloned()
    }

    /// Withdraw the grant naming one MCP server, and say whether one was there to withdraw.
    ///
    /// The only mutation this type has, and it narrows: nothing here adds a grant, so a run
    /// still cannot acquire a capability partway through. What it can do is lose one, which
    /// is what makes a grant the thing a call asks about rather than a property the session
    /// recorded. A grant withdrawn stops answering at the next call rather than at the next
    /// session.
    pub fn revoke_mcp_call(&mut self, alias: &ServerAlias) -> bool {
        let withdrawn = Capability::McpCall(alias.clone());
        let before = self.granted.len();
        // Only the grant naming this server: the others are separate capabilities that this
        // one says nothing about.
        self.granted.retain(|granted| granted != &withdrawn);
        self.granted.len() != before
    }

    /// Hand out a token if this capability was granted.
    ///
    /// The token is the only way to satisfy an operation that requires a capability,
    /// so a caller cannot proceed by asserting it has permission.
    pub fn token_for(&self, capability: &Capability) -> Option<CapabilityToken> {
        self.contains(capability)
            .then(|| CapabilityToken::mint(capability.clone()))
    }
}

impl FromIterator<Capability> for CapabilitySet {
    fn from_iter<I: IntoIterator<Item = Capability>>(capabilities: I) -> Self {
        let mut granted: Vec<_> = capabilities.into_iter().collect();
        granted.sort();
        granted.dedup();
        Self { granted }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn observations_are_untrusted() {
        for c in Capability::all() {
            let Some(label) = c.output_label() else {
                continue;
            };
            assert!(!label.is_trusted(), "{c} output must not be trusted");
        }
    }

    /// Nothing a capability produces is ever routing-safe. Routing must come from
    /// trusted input, so if any capability yielded `(T,pub)` the asymmetry would leak.
    #[test]
    fn no_capability_produces_routing_safe_output() {
        for c in Capability::all() {
            if let Some(label) = c.output_label() {
                assert_ne!(
                    label,
                    Label::trusted_public(),
                    "{c} must not produce routing-safe output"
                );
            }
        }
    }

    /// LSP-9, and the trap this capability could have fallen into. A location reaching the planner
    /// is LSP-3's argument about structure, made where the answer is rendered. It must not be
    /// reached by labelling the capability's output routing-safe, which would make every byte a
    /// server reported into something that can choose a destination.
    #[test]
    fn the_lsp_capability_produces_no_routing_safe_output() {
        let label = Capability::LanguageServer
            .output_label()
            .expect("a language server produces an observation");
        assert_ne!(label, Label::trusted_public());
        assert!(!label.is_trusted());
        // On the same footing as a file read, because it was computed from the same files.
        assert_eq!(label, Capability::FileRead.output_label().unwrap());
    }

    /// LSP-9: asking a server is not the same act as reading a file, so one grant is not the other.
    #[test]
    fn a_language_server_grant_is_separate_from_a_file_read() {
        let reads_only = CapabilitySet::from_iter([Capability::FileRead]);
        assert!(reads_only.token_for(&Capability::LanguageServer).is_none());

        let asks_only = CapabilitySet::from_iter([Capability::LanguageServer]);
        assert!(asks_only.token_for(&Capability::FileRead).is_none());
    }

    #[test]
    fn workspace_reads_are_private_and_remote_reads_are_public() {
        assert_eq!(
            Capability::FileRead.output_label(),
            Some(Label::untrusted_private())
        );
        assert_eq!(
            Capability::WebFetch.output_label(),
            Some(Label::untrusted_public())
        );
    }

    #[test]
    fn pure_effects_have_no_output_label() {
        assert_eq!(Capability::FileWrite.output_label(), None);
        assert_eq!(Capability::GitWrite.output_label(), None);
    }

    #[test]
    fn effects_and_observations_are_distinguished() {
        assert!(Capability::FileWrite.is_effect());
        assert!(Capability::ShellExec.is_effect());
        assert!(!Capability::FileRead.is_effect());
        assert!(!Capability::WebFetch.is_effect());
    }

    #[test]
    fn network_capabilities_are_identified() {
        assert!(Capability::WebFetch.needs_network());
        assert!(Capability::McpCall(ServerAlias::new("weather")).needs_network());
        assert!(!Capability::FileRead.needs_network());
        assert!(!Capability::ShellExec.needs_network());
    }

    #[test]
    fn an_empty_set_grants_nothing() {
        let set = CapabilitySet::none();
        assert!(set.is_empty());
        assert!(set.token_for(&Capability::FileRead).is_none());
    }

    #[test]
    fn a_token_is_issued_only_for_granted_capabilities() {
        let set = CapabilitySet::from_iter([Capability::FileRead]);
        let token = set.token_for(&Capability::FileRead).expect("granted");
        assert_eq!(token.capability(), &Capability::FileRead);
        assert!(set.token_for(&Capability::FileWrite).is_none());
    }

    /// SERVERS-9: a grant names one server, so it answers about that server and no other.
    /// The fault this rejects is a gate that reads the protocol out of the capability and
    /// stops there, which is what a single `McpCall` variant leaves every caller doing.
    #[test]
    fn a_grant_for_one_server_is_not_a_grant_for_another() {
        let weather = ServerAlias::new("weather");
        let payments = ServerAlias::new("payments");
        let set = CapabilitySet::from_iter([Capability::McpCall(weather.clone())]);

        assert!(set.contains(&Capability::McpCall(weather)));
        assert!(!set.contains(&Capability::McpCall(payments.clone())));
        assert!(set.token_for(&Capability::McpCall(payments)).is_none());
    }

    /// SERVERS-9: withdrawing a grant takes the server out of reach at once, and takes
    /// nothing else with it. The fault this rejects is a withdrawal written against the
    /// protocol rather than the server, which would silently drop every other server too.
    #[test]
    fn withdrawing_one_grant_leaves_the_others() {
        let weather = ServerAlias::new("weather");
        let payments = ServerAlias::new("payments");
        let mut set = CapabilitySet::from_iter([
            Capability::McpCall(weather.clone()),
            Capability::McpCall(payments.clone()),
            Capability::FileRead,
        ]);

        assert!(set.revoke_mcp_call(&weather));
        assert!(!set.contains(&Capability::McpCall(weather.clone())));
        assert!(set.contains(&Capability::McpCall(payments)));
        assert!(set.contains(&Capability::FileRead));

        // Nothing left to withdraw, and saying so is how a caller tells an absent grant
        // from one it has just dropped.
        assert!(!set.revoke_mcp_call(&weather));
    }

    /// A refusal has to say which server was asked for. `as_str` is the class of effect,
    /// which is what an MCP call shares with every other MCP call.
    #[test]
    fn a_call_is_named_by_its_server() {
        let capability = Capability::McpCall(ServerAlias::new("weather"));
        assert_eq!(capability.to_string(), "mcp_call:weather");
        assert_eq!(capability.as_str(), "mcp_call");
        assert_eq!(Capability::FileRead.to_string(), "file_read");
    }

    #[test]
    fn duplicate_grants_collapse() {
        let set = CapabilitySet::from_iter([
            Capability::FileRead,
            Capability::FileRead,
            Capability::ShellExec,
        ]);
        assert_eq!(set.iter().count(), 2);
    }
}
