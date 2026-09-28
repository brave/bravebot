//! Configuration read from the environment, populated by direnv, falling back to
//! whatever was baked in at build time.
//!
//! The signing key is wrapped in [`Secret`] so it cannot be printed. That matters
//! more than usual here: this is a public repository, and the natural debugging
//! reflex, dumping the config, would otherwise put a live credential in a log or
//! an issue report.

#![forbid(unsafe_code)]

use bravebot_sandbox::swap::LockedText;
use std::env;
use std::fmt;
use std::time::Duration;

/// Environment variable names, kept together so the set is auditable at a glance.
pub mod env_var {
    include!("env_var.rs");
}

pub mod hooks;
mod managed;
pub mod mcp;
mod obfuscate;
pub mod scrub;
mod settings;
#[cfg(test)]
mod testutil;

pub use managed::{Managed, Refusal, Rule, Server, managed_file};
pub use settings::{
    Attribution, NotADocument, PermissionLists, Settings, check_document, name_a_settings_file,
    named_settings_file, user_settings_file,
};

pub mod bedrock;
pub mod import;
pub mod provider;

include!(concat!(env!("OUT_DIR"), "/baked.rs"));

/// The server treats an unrecognised model as [`DEFAULT_MODEL`], so that is also our default rather
/// than pinning a name that may silently stop existing.
///
/// Bravebot's own triage entry. Leo uses `automatic` for a different routing policy; requests from
/// this product identify themselves with `Brave-Product: bravebot` so the listing and completions
/// endpoints offer this name instead.
pub const DEFAULT_MODEL: &str = "automatic-bravebot";

/// The automatic routing name Leo uses. Remembered choices and configuration may still say it from
/// before bravebot had its own entry; [`normalize_model`] rewrites those to [`DEFAULT_MODEL`].
const LEGACY_AUTOMATIC: &str = "automatic";

/// Rewrite a model name from an older choice or configuration to what this product sends now.
pub fn normalize_model(name: &str) -> &str {
    if name == LEGACY_AUTOMATIC {
        DEFAULT_MODEL
    } else {
        name
    }
}

/// The model a name asks for: a tier word resolved to a model that exists, anything else as written.
///
/// `opus`, `sonnet` and `haiku` name a tier rather than a model, which is what those words mean in
/// the settings file the model key is copied from. Left as the word they reach a service that has
/// never heard of them, so they are resolved: the tier's own ARN where an AWS account named one,
/// and the Brave roster's name for that tier otherwise.
///
/// The AWS account wins where it named the tier, because somebody who configured it asked for it
/// by name. A tier they left unset falls through to Brave rather than being guessed at, an ARN not
/// being derivable from a word, except on a build that cannot reach Brave: there a Brave name
/// reaches a service this build cannot sign for, so the strongest tier the AWS account did name is
/// the only thing that could answer.
///
/// `reaches_brave` is whether this build holds the credentials to sign a Brave request, which is
/// [`holds_brave_credentials`], and `bedrock` is whichever configured AWS account would serve the
/// request, which is [`tier_account`]. An endpoint on its own decides neither: a source build with
/// a URL and no key id signs nothing, and an account a `provider` block named answers as readily as
/// one the tier variables did.
///
/// Shared by every route to a model rather than belonging to the settings key, so a name means the
/// same model wherever it was written down. See [`Config::model_named`].
fn resolved_model(name: &str, bedrock: Option<&bedrock::Bedrock>, reaches_brave: bool) -> String {
    let named = match bedrock::Tier::from_alias(name) {
        Some(tier) => match bedrock {
            Some(bedrock) => bedrock
                .model_for(tier)
                .or_else(|| match reaches_brave {
                    false => bedrock.default_model(),
                    true => None,
                })
                .unwrap_or(tier.brave_model()),
            None => tier.brave_model(),
        },
        None => name,
    };
    normalize_model(named).to_string()
}

/// Whether this build holds everything needed to sign a request to the Brave backend.
///
/// All three fields, because a request needs all three: the URL to send to, the key id naming the
/// credential, and the key that signs with it. Any one of them blank leaves a request that fails
/// unsigned, so a build in that state cannot reach Brave whatever the other two say. A `provider`
/// block or a Bedrock account makes each of them optional independently, which is how "endpoint
/// set, credentials blank" arises. See [`Config::serves_aichat`].
fn holds_brave_credentials(endpoint: &str, key_id: &str, signing_key: &str) -> bool {
    !endpoint.is_empty() && !key_id.is_empty() && !signing_key.is_empty()
}

/// The AWS account a tier word resolves against: the tier variables' account, else a `provider`
/// block's.
///
/// The tier variables are asked first, being the older way to name a Bedrock model and the one a
/// tier word names a tier of, which is the order [`Config::bedrock_for`] resolves a model in. An
/// account that named no model at all is passed over rather than shadowing one that did, since a
/// `BRAVEBOT_USE_BEDROCK=1` with a region and no tier set is an account nothing can be asked of.
///
/// One account, not the strongest model across all of them. Comparing two accounts' models would
/// mean ranking an inference-profile ARN against another one, which nothing here can do: a block
/// names models rather than tiers, so there is no tier to compare them by.
fn tier_account<'a>(
    bedrock: Option<&'a bedrock::Bedrock>,
    providers: &'a [provider::Provider],
) -> Option<&'a bedrock::Bedrock> {
    bedrock
        .into_iter()
        .chain(providers.iter().filter_map(|entry| entry.bedrock.as_ref()))
        .find(|account| account.default_model().is_some())
}

/// How many prompt tokens a conversation may reach before it is compacted.
///
/// Low enough to be under any window worth calling a context window, since a budget above the
/// real one never fires and the session dies of the thing compaction exists to prevent. The cost
/// of being wrong downward is a summary sooner than it was needed; the cost of being wrong upward
/// is the feature not existing. See [`Config::context_budget`].
///
/// Only a fallback now. Where the endpoint says what a model's window is, that is used instead:
/// see [`budget_for_window`]. This stands in for [`DEFAULT_MODEL`], whose model is chosen per
/// request so no one window describes it, and for an entry that reports nothing.
///
/// This was 100_000, which is not under any window the backend serves, so compaction could not
/// fire: recorded sessions ended at 28,600, 34,751 and 34,800 prompt tokens having never once been
/// summarised. Chosen to sit under the smallest window on the roster rather than to suit the
/// largest, because being wrong upward costs a summary sooner than it was needed and being wrong
/// downward removes compaction altogether.
///
/// [`env_var::CONTEXT_BUDGET`] overrides both this and anything discovered.
pub const DEFAULT_CONTEXT_BUDGET: u64 = 24_000;

/// The smallest context window worth using, in prompt tokens.
///
/// Not a limit anything enforces. It exists so [`DEFAULT_CONTEXT_BUDGET`] can be checked against
/// the claim its own documentation makes, which is the claim that quietly stopped being true.
pub const SMALLEST_USEFUL_WINDOW: u64 = 32_000;

/// What the endpoint sends for a model whose window it does not know.
///
/// Not a small window: a placeholder standing where a figure would be. Taken as nothing said, since
/// a budget of one token would compact before a turn could start.
const NO_WINDOW_ADVERTISED: u64 = 1;

/// The budget to use for a model the endpoint described, or `None` to fall back.
///
/// `advertised` is what `/v1/models` reported, which is already a token count with a fifth held
/// back for the reply. It is used as it stands: there is nothing to convert, and no reserve to take
/// off a figure that arrives discounted.
///
/// A small figure is taken at its word rather than raised to something more comfortable. Some models
/// on the roster really do have windows of a few thousand tokens, and a floor that lifted them to
/// the default would put the budget above the window, which does not make compaction late but
/// removes it: every round asks, no round qualifies, and the session runs to exhaustion looking
/// exactly like one with nothing to summarise. That is the failure this budget exists to prevent, so
/// a cramped budget is the better of the two wrongs.
///
/// `None` only where nothing was advertised, which is the caller's cue to use
/// [`DEFAULT_CONTEXT_BUDGET`].
pub fn budget_for_window(advertised: Option<u64>) -> Option<u64> {
    advertised.filter(|window| *window > NO_WINDOW_ADVERTISED)
}

// Checked where it cannot be skipped. A budget above the window does not make compaction late, it
// removes it, and the removal is silent: every round asks, no round qualifies, and the session
// runs to exhaustion looking exactly like one that had nothing to summarise. A compile is a
// better place to find that out than a transcript.
const _: () = assert!(DEFAULT_CONTEXT_BUDGET < SMALLEST_USEFUL_WINDOW);

/// A value that must not be printed.
///
/// `Debug` and `Display` are deliberately redacting, and the inner value is only
/// reachable through [`Secret::expose`], a name chosen so that reading a credential
/// is visible at the call site during review.
///
/// Intentionally missing: equality, `Deref`, `AsRef`, `Borrow`, `Serialize` and `From`.
/// An operator that answers a question about the bytes recovers them a guess at a time, and
/// the derived answer takes time proportional to the shared prefix, so a credential that has
/// to be compared is compared in constant time by the component that owns it.
///
/// Constructing one and asking whether it is empty is ordinary, which is what makes the
/// refusal below a refusal of equality rather than of the name:
///
/// ```
/// use bravebot_config::Secret;
/// assert!(!Secret::new("a").is_empty());
/// ```
///
/// Comparing two of them does not compile:
///
/// ```compile_fail
/// use bravebot_config::Secret;
/// let _ = Secret::new("a") == Secret::new("a");
/// ```
///
/// The value is held in pages of its own, which the kernel is asked to keep out of swap and which
/// are overwritten before they are handed back
/// ([CRED-23](../../../docs/specs/credential-protection.md#CRED-23)). The `String` it was made from
/// is overwritten once the value has been copied out of it. Cloning makes a second copy, held and
/// cleared the same way.
///
/// What this reaches is the value this type holds and a `String` it was handed, which is what
/// CRED-23 promises and all it promises. Handed a `&str`, it copies it into a `String` of its own
/// first, so the buffer that borrow points into is the caller's to clear. A value that was copied
/// on its way in, by an allocator growing a `String` or by a library between here and a socket,
/// left a copy nothing here holds a pointer to.
#[derive(Clone)]
pub struct Secret(LockedText);

impl Secret {
    pub fn new(value: impl Into<String>) -> Self {
        let mut value = value.into();
        Self(hold(&mut value))
    }

    /// Read the secret. Call sites should be rare and obvious.
    pub fn expose(&self) -> &str {
        self.0.as_str()
    }

    pub fn is_empty(&self) -> bool {
        self.0.as_str().is_empty()
    }
}

/// Copy a credential into pages of its own and overwrite the string it arrived in, which would
/// otherwise go back to the allocator holding it on the heap the lock was taken to keep it off.
fn hold(value: &mut String) -> LockedText {
    let held = LockedText::new(value);
    scrub(value);
    held
}

/// Overwrite a string's bytes where they lie, leaving the buffer as many zero bytes long as the
/// value was.
///
/// `clear` is not this and is the mistake it exists to avoid: it sets the length to nothing and
/// leaves every byte where it was, so the credential is still in the allocation when the
/// allocator hands it to whoever asks next. Clearing and then pushing the replacement back writes
/// over the original bytes in place, because `clear` keeps the capacity and the replacement is
/// exactly as long as what it replaces, so nothing reallocates.
///
/// The write is held down by handing the bytes to [`std::hint::black_box`]. Nothing reads them
/// back, and a compiler that can see the whole life of the buffer is entitled to delete a store
/// no one observes; an opaque use of the bytes is how safe code says otherwise. It is a barrier
/// rather than a guarantee the language makes, which is the price of doing this in a crate that
/// forbids `unsafe` and so cannot write the bytes volatile.
pub(crate) fn scrub(value: &mut String) {
    let length = value.len();
    value.clear();
    value.extend(std::iter::repeat_n('\0', length));
    std::hint::black_box(value.as_bytes());
}

/// Overwrite every value in a parsed settings document, which is where the copy of a credential the
/// parse made lives until the document goes.
///
/// Every value rather than the names a credential is known to arrive under: whoever wrote the file
/// chose where a token went, and by the time this runs the document has been read and nothing will
/// ask it anything again. What a map's keys are cannot be reached through one and is a name rather
/// than a value, so what this covers is what each name was set to.
///
/// Assigning `Value::Null` over a string is the mistake available here: it drops the string, which
/// is the thing that leaves the bytes where the allocator can hand them on.
pub(crate) fn scrub_document(root: &mut serde_json::Map<String, serde_json::Value>) {
    root.values_mut().for_each(scrub_value);
}

/// Overwrite one value of a parsed document, and whatever is under it.
///
/// Public because a parse is how a credential arrives in more than one crate: a settings file
/// here, and the JSON the AWS CLI answers a credential request with. A crate that may depend on
/// this one uses this rather than spelling the walk again, which is the arrangement
/// [CRED-23](../../../docs/specs/credential-protection.md#CRED-23) names.
///
/// Recursive over a shape a parse produced, which is a shape [`serde_json`] refused past 128 levels
/// of nesting, so the depth here is the parser's bound rather than the file's.
pub fn scrub_value(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::String(text) => scrub(text),
        serde_json::Value::Array(values) => values.iter_mut().for_each(scrub_value),
        serde_json::Value::Object(map) => map.values_mut().for_each(scrub_value),
        serde_json::Value::Null | serde_json::Value::Bool(_) | serde_json::Value::Number(_) => {}
    }
}

/// Overwrite a byte buffer where it lies, leaving it as many zero bytes long as it was.
///
/// What [`scrub`] promises, for bytes that were never a `String`. A credential arrives this way
/// whenever a subprocess writes one to a pipe: what the AWS CLI answers with is a `Vec<u8>` of
/// JSON holding a live session key, and it is as readable there as in any other buffer.
///
/// A slice rather than a `Vec`, so that truncating the buffer is not one of the things a caller
/// can reach for. The length is where the credential is, and setting it to nothing leaves every
/// byte of it for whoever the allocator hands the memory to next.
pub fn scrub_bytes(bytes: &mut [u8]) {
    bytes.fill(0);
    // The barrier [`scrub`] explains: nothing reads these zeros back, and a compiler that can see
    // the whole life of the buffer is entitled to delete a store no one observes.
    std::hint::black_box(&*bytes);
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(<redacted>)")
    }
}

impl fmt::Display for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<redacted>")
    }
}

/// Which of the three gates between the tiers a credential's walk failed at.
///
/// Numbered top down from Delegated, as the walk asks them: gate 1 separates Delegated from
/// Granted, gate 2 Granted from Held briefly, gate 3 Held briefly from Held. A drop names the
/// gate rather than the tier it landed on because the gate is the question that was answered,
/// and the tier is only where answering it left the credential.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gate {
    /// Can something the agent cannot impersonate decide each use, and refuse?
    One,
    /// Can the issuer mint a bounded derivative, enforced beyond the agent's reach?
    Two,
    /// Can the issuer mint on demand and enforce death, unrefreshable without authority?
    Three,
}

impl Gate {
    /// The number the walk asks this gate at, which is what a report states.
    ///
    /// Taken from the record rather than written into each sentence a person reads, so a report
    /// cannot name a gate the record did not fail.
    pub const fn number(self) -> u8 {
        match self {
            Self::One => 1,
            Self::Two => 2,
            Self::Three => 3,
        }
    }
}

/// One condition of one gate, which is what CRED-3 asks a drop to name.
///
/// The set is the questions the gates ask rather than the answers this configuration happens to
/// record: a gate fails on a condition, and a record holding only the conditions something fails
/// on today could not say that a second one had started failing too.
///
/// Which gate a condition belongs to is [`Condition::gate`] and not a field beside it, since a
/// condition is a condition of exactly one gate and a pair of fields can be set to disagree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Condition {
    /// Gate 1: nothing the agent cannot impersonate decides each use.
    ///
    /// CRED-6 is what this asks: where the agent can redeem what it holds unaided, the thing it
    /// holds is the credential under another name.
    NothingDecidesEachUse,
    /// Gate 1: what decides each use cannot refuse one.
    ///
    /// Separate from the condition above because a performer that cannot say no is an
    /// authorisation step that always authorises.
    NothingCanRefuseAUse,
    /// Gate 2: no bound on what the value may do is fixed before it is issued.
    ///
    /// CRED-7's first half. A value that arrives carrying whatever its holder already had is
    /// unbounded whoever minted it.
    NoBoundFixedBeforeIssue,
    /// Gate 2: the bound is checked within the agent's reach.
    ///
    /// CRED-7's second half: a program running as the person presents itself as any other, so a
    /// bound the agent's own code enforces answers to whoever is asking.
    BoundEnforcedWithinReach,
    /// Gate 2: the bound has no end.
    ///
    /// CRED-8. Narrow and permanent is a static secret with a small reach.
    BoundWithNoEnd,
    /// Gate 3: the value is not minted for one named step.
    NotMintedForOneStep,
    /// Gate 3: the issuer puts no end on it.
    IssuerEndsNothing,
    /// Gate 3: the agent can renew it without further authority.
    ///
    /// CRED-9's false pass: a fifteen-minute token the agent refreshes by itself is a permanent
    /// credential with extra steps.
    RenewableWithoutAuthority,
}

impl Condition {
    /// Every condition a gate asks about, so something reporting a walk can be held to all of
    /// them rather than to the ones a credential happens to fail on today.
    pub const fn all() -> [Self; 8] {
        [
            Self::NothingDecidesEachUse,
            Self::NothingCanRefuseAUse,
            Self::NoBoundFixedBeforeIssue,
            Self::BoundEnforcedWithinReach,
            Self::BoundWithNoEnd,
            Self::NotMintedForOneStep,
            Self::IssuerEndsNothing,
            Self::RenewableWithoutAuthority,
        ]
    }

    /// The gate this condition is a condition of.
    pub const fn gate(self) -> Gate {
        match self {
            Self::NothingDecidesEachUse | Self::NothingCanRefuseAUse => Gate::One,
            Self::NoBoundFixedBeforeIssue
            | Self::BoundEnforcedWithinReach
            | Self::BoundWithNoEnd => Gate::Two,
            Self::NotMintedForOneStep
            | Self::IssuerEndsNothing
            | Self::RenewableWithoutAuthority => Gate::Three,
        }
    }
}

/// Whether a condition went unmet because the counterparty refused or because nobody attempted it.
///
/// CRED-3 keeps these apart because they end at the same tier and mean opposite things. A refusal
/// is a fact about the world, and nothing on this side changes it by trying harder. An unattempted
/// gate is a decision somebody made here, and it is the one that is ours to revisit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Attempt {
    /// The counterparty offers the arrangement to nobody, so asking for it changes nothing.
    Refused,
    /// The counterparty offers the arrangement, or could, and nothing here asks for it.
    NotAttempted,
}

/// One drop of a credential's gate walk: the condition of the gate that failed, and whether the
/// counterparty refused or nobody attempted it.
///
/// Both halves, because CRED-3 asks for both and a tier without them is an assertion: Held says
/// where a credential stands and says nothing about whether it could have stood anywhere else.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GateDrop {
    /// Which of the gate's conditions was not met. The gate is [`Condition::gate`].
    pub condition: Condition,
    /// Which of the two answers the condition has.
    pub attempt: Attempt,
}

/// Where a credential's walk down the gates stopped, which is its tier.
///
/// CRED-2 asks that every credential in use carry one, and that it be arrived at by walking down
/// from [`Tier::Delegated`] rather than asserted. The walk is what separates a credential somebody
/// reasoned about from one that ended up wherever it ended up, so the default for custody with no
/// recorded walk is [`Tier::Held`], the tier that assumes nothing: a tier that defaulted upward
/// would let silence claim the strongest one.
///
/// Four tiers and three gates, and a drop is worth one tier: gate 1 asks whether something this
/// program cannot impersonate decides each use, gate 2 whether the issuer mints a bounded
/// derivative it enforces beyond this program's reach, and gate 3 whether the issuer mints on
/// demand and ends the value without this program's help. Passing a gate stops the walk where it
/// is, which is why [`Tier::Granted`] and [`Tier::HeldBriefly`] are both bounded and expiring
/// arrangements: they differ in whether the bound or the lifetime is the thing the issuer enforces.
///
/// A credential with no custody and no refusal is on no tier at all, which CRED-5 covers and this
/// type deliberately cannot express: an arrangement nobody can refuse would otherwise be recorded
/// at the top of a scale it is not on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    /// Nothing is held here and something this program cannot impersonate decides each use.
    Delegated,
    /// A real secret, bounded before it was issued by an issuer that enforces the bound.
    Granted,
    /// A real secret whose lifetime, rather than its reach, is what its issuer enforces.
    HeldBriefly,
    /// A permanent secret, bounded by the surface that revokes it and by nothing else.
    Held,
}

impl Tier {
    /// Every tier a credential can stand at, so something reporting them cannot omit one.
    ///
    /// Written from the top down, in walk order, because that is the order the gates are asked in
    /// and a reader checking a record against the walk reads them the same way.
    pub const ALL: [Self; 4] = [
        Self::Delegated,
        Self::Granted,
        Self::HeldBriefly,
        Self::Held,
    ];
}

/// A credential this program holds itself, and so owes an account of what would end it.
///
/// CRED-25 asks three things about every credential at Held or Held briefly: who issued it, the
/// surface that revokes it, and anything minted from it that revocation would not reach. All three
/// are facts about the arrangement a credential arrived under rather than about its bytes, so what
/// is recorded beside the value is which arrangement it is, and the three answers follow from that.
/// The words a person reads are in the message catalog, since this crate holds none of its own.
///
/// CRED-2 asks a fourth thing of every credential in the list, whatever its tier: the tier itself,
/// arrived at by walking down from Delegated. [`Held::tier`] is that record, and it is the one fact
/// here that is not a CRED-25 obligation, which is why the list reaches past the two tiers CRED-25
/// asks an account of. A credential in use with no recorded tier is the outcome CRED-2 forbids, so
/// the record naming the arrangements is where the tier belongs: the alternative is a second list
/// that can be short by one.
///
/// CRED-10 asks a fifth thing, of every credential at Held briefly and of any other worth saying it
/// of: how quickly a leak of one would be noticed and acted on. That figure is a judgement about
/// the deployment rather than a fact about the arrangement, and it establishes no tier and moves
/// none, so it is recorded here beside the tier rather than derived from it: [`Held::tier`] and
/// [`Held::noticed_within`].
///
/// CRED-3 asks a sixth thing, of every credential here: how it reached the tier it stands at. Each
/// drop of the walk down from Delegated records which of the gate's conditions failed and whether
/// the counterparty refused or nobody attempted it, which is [`Held::walk`]. A tier without that
/// is an assertion, and the two answers matter separately: a gate the counterparty refused is a
/// fact about the world, and a gate nobody attempted is a decision made here.
///
/// The obligation is detection followed by something a person can act on. An expiry, which is all
/// that was kept before, says when a credential stops working and nothing about how to stop it
/// working sooner; removing the file it came from ends this run's custody and leaves it live at its
/// issuer.
///
/// A gateway's bearer token is in this list on the same footing as the rest. It is not this
/// build's own credential, but a block that named a variable or wrote a token into a settings
/// file is one this configuration holds for the length of a run, and `provider.rs` is the third
/// path CRED governs, so the clause reaches it. That the block names no issuer is the answer to a
/// different question: the host it names is the surface the token is presented to and the only one
/// that revokes it, which is the address the clause asks be written down.
///
/// An imported subscription's credential batch is in it for the same reason and on a different
/// tier. `skus/store.rs` is among the paths CRED governs, the batch sits in a file under the
/// machine for the length of its validity window, and a request spends one credential from it, so
/// it is a credential in use and owes a tier. That the tier is [`Tier::Granted`] rather than Held
/// is what the walk answers, not what being in this list decides.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Held<'a> {
    /// The HMAC signing key baked into this build, which signs every request to the Brave backend.
    ///
    /// Held. It carries no expiry, and one build's key is every install's, so nothing on this
    /// machine ends it: the surface is the backend that derives its copy from a master seed and
    /// [`Config::key_id`], and revoking means retiring that id there and shipping another build.
    SigningKey,
    /// A long-lived AWS access key, as the AWS CLI resolved it for a profile.
    ///
    /// Held. `aws iam delete-access-key` ends it at IAM, and does not reach a session credential
    /// already minted from it.
    AwsAccessKey,
    /// An AWS session credential: an SSO session or an assumed role, as the AWS CLI resolved it.
    ///
    /// Held. It states an expiry, and the expiry ends that copy rather than this program's
    /// access: credentials are resolved as each request is built, and the export that renews them
    /// asks nobody. Ending it is done at its issuer, because `aws sso logout` clears this
    /// machine's copy rather than the session.
    AwsSession,
    /// The bearer token a `provider` block carries or names a variable for, held for the run.
    ///
    /// Held. It carries no expiry and nothing on this machine ends it: `host` is where the token
    /// is presented, so it is both the issuer as far as this program can know one and the only
    /// surface that revokes it. Deleting the value from the settings file, or unsetting the
    /// variable, ends this machine's custody and leaves the token live there.
    ///
    /// The host rather than the block's id, because the id is a name somebody chose for a section
    /// of their own file and the report is read by somebody going to the gateway to end a token.
    GatewayToken {
        /// The host the block's endpoint names, from [`provider::Provider::host`].
        host: &'a str,
    },
    /// The single-use credential batch an imported Leo Premium subscription was minted as, kept in
    /// `~/.bravebot/leo-premium.json` and spent one credential per premium request.
    ///
    /// Granted. Gate 1 fails, because the batch is presented by this program and nothing else
    /// decides a use of it. Gate 2 passes: the batch is what Brave's subscription service minted
    /// from the order, each credential is good for one presentation inside one window, and the
    /// backend verifying it is what enforces both, so the bound was fixed before issue and sits
    /// beyond this program's reach. The walk stops there, which is why nothing here sizes it
    /// against detection: a window is not what bounds it.
    ///
    /// No parameter, unlike a gateway. There is one batch whatever channel it was imported from,
    /// the issuer is Brave's own service, and the surface is the order rather than an address this
    /// configuration could name differently on two machines.
    SubscriptionBatch,
}

impl<'a> Held<'a> {
    /// Every credential this program can hold, so something reporting them cannot omit one.
    ///
    /// The gateway arrangement is a credential plus the host that would end it, so a list of the
    /// kinds has to be given one: a caller walking this is asking which arrangements exist rather
    /// than which are configured, and the host it passes is what the gateway entry will name.
    pub const fn all(gateway_host: &'a str) -> [Self; 5] {
        [
            Self::SigningKey,
            Self::AwsAccessKey,
            Self::AwsSession,
            Self::GatewayToken { host: gateway_host },
            Self::SubscriptionBatch,
        ]
    }

    /// Every drop of this credential's walk down from Delegated, in the order the gates are asked.
    ///
    /// CRED-2 puts a credential's tier at where its walk stopped, and CRED-3 asks each drop to say
    /// which of that gate's conditions failed and whether the counterparty refused or nobody
    /// attempted it. Recorded per credential rather than per tier, because two credentials at Held
    /// can have got there for opposite reasons and only one of them is worth revisiting.
    ///
    /// The length is the tier: failing a gate drops exactly one tier and nothing skips one, so one
    /// drop is Granted, two is Held briefly and three is Held. That is the same claim
    /// [`Held::tier`] makes, and the two are pinned against each other rather than derived from one
    /// another, so a walk revised without the tier fails rather than quietly restating it.
    ///
    /// Every drop here is one nobody attempted. Each of these credentials is handed over whole and
    /// used directly, and at each gate the counterparty either offers the stronger arrangement
    /// already, as AWS does in minting a bounded session, or has never been asked for it. None of
    /// them is a refusal, so none of them is excused by the world being as it is.
    pub fn walk(self) -> &'static [GateDrop] {
        match self {
            // The key signs the request digest in this process, so nothing decides a use. The
            // backend derives its copy from a master seed and the key id and offers nothing
            // narrower and nothing shorter, and it is this project's own backend: what stops each
            // of these gates is that nobody has built the other side of it.
            Self::SigningKey => &[
                GateDrop {
                    condition: Condition::NothingDecidesEachUse,
                    attempt: Attempt::NotAttempted,
                },
                GateDrop {
                    condition: Condition::NoBoundFixedBeforeIssue,
                    attempt: Attempt::NotAttempted,
                },
                GateDrop {
                    condition: Condition::NotMintedForOneStep,
                    attempt: Attempt::NotAttempted,
                },
            ],
            // AWS mints a session bounded by a policy it enforces and the agent cannot widen, and
            // ends it, which is gates 2 and 3 both answered on the counterparty's side. What this
            // program does with a long-lived key is sign with it, so all three drops are ours.
            Self::AwsAccessKey => &[
                GateDrop {
                    condition: Condition::NothingDecidesEachUse,
                    attempt: Attempt::NotAttempted,
                },
                GateDrop {
                    condition: Condition::NoBoundFixedBeforeIssue,
                    attempt: Attempt::NotAttempted,
                },
                GateDrop {
                    condition: Condition::NotMintedForOneStep,
                    attempt: Attempt::NotAttempted,
                },
            ],
            // Three drops, although the session states an expiry and the issuer that minted it
            // could end it early. Gate 2 fails on the bound: the session carries whatever the
            // profile's role or SSO grant allows, and nothing here asks STS to narrow it to this
            // run. Gate 3 fails on renewal, which is CRED-9's false pass. Credentials are resolved
            // as each request is built, and what renews them is `aws configure
            // export-credentials`, which asks nobody: the next session is minted from whatever the
            // profile chains to, for as long as that lasts. How long that is is a question about
            // `~/.aws/config`, which nothing here reads. A `role_arn` over a long-lived key in a
            // file lasts until somebody deletes the key, and an SSO chain until its own token runs
            // out; the two arrive here as one arrangement, the CLI answering an export.
            //
            // Not attempted rather than refused: reading the profile chain, and finding one whose
            // renewal a person has to sit through, would be a different arrangement and would
            // answer differently, and nothing here has asked for one.
            Self::AwsSession => &[
                GateDrop {
                    condition: Condition::NothingDecidesEachUse,
                    attempt: Attempt::NotAttempted,
                },
                GateDrop {
                    condition: Condition::NoBoundFixedBeforeIssue,
                    attempt: Attempt::NotAttempted,
                },
                GateDrop {
                    condition: Condition::RenewableWithoutAuthority,
                    attempt: Attempt::NotAttempted,
                },
            ],
            // A block names a host and a variable and never an issuer, so there is nothing here
            // to ask for a performer, a narrower token or a shorter one. Not attempted rather
            // than refused: what the gateway at the other end offers is not something this
            // program has been told.
            Self::GatewayToken { .. } => &[
                GateDrop {
                    condition: Condition::NothingDecidesEachUse,
                    attempt: Attempt::NotAttempted,
                },
                GateDrop {
                    condition: Condition::NoBoundFixedBeforeIssue,
                    attempt: Attempt::NotAttempted,
                },
                GateDrop {
                    condition: Condition::NotMintedForOneStep,
                    attempt: Attempt::NotAttempted,
                },
            ],
            // One drop, because gate 2 passes: Brave's subscription service minted the batch from
            // the order, each credential is good for one presentation inside one window, and the
            // backend verifying it enforces both beyond this program's reach. Gate 1 fails because
            // this program presents the credential itself and nothing decides a use of it, and
            // nobody has asked that service for something that would.
            Self::SubscriptionBatch => &[GateDrop {
                condition: Condition::NothingDecidesEachUse,
                attempt: Attempt::NotAttempted,
            }],
        }
    }

    /// Where this credential's walk down the gates stopped, which CRED-2 asks every credential in
    /// use to carry.
    ///
    /// A claim about the arrangement, so it moves only when the arrangement does: CRED-4. Nothing
    /// a person could rewrite is in it, and no variant stands at [`Tier::Delegated`], because
    /// every arrangement here hands material over and this program presents it itself.
    ///
    /// The reasoning for each answer is on the variant, where the arrangement it describes is, and
    /// not repeated here: a second copy of it is a second thing to keep true.
    pub fn tier(self) -> Tier {
        match self {
            Self::SubscriptionBatch => Tier::Granted,
            Self::SigningKey
            | Self::AwsAccessKey
            | Self::AwsSession
            | Self::GatewayToken { .. } => Tier::Held,
        }
    }

    /// How quickly a leak of this credential would be noticed and acted on, which CRED-10 asks the
    /// record to say for every credential at Held briefly.
    ///
    /// Owed at Held too, and for the stronger reason. The obligations ratchet downward, so a
    /// credential that drops a tier keeps what it owed and picks up more, and detection is the
    /// whole of what a permanent credential is owed: it outlives every decision made about it.
    /// What the drop takes away is what the figure decides. At Held briefly it says whether the
    /// window is short enough for what the credential reaches; at Held there is no window, and it
    /// says how long a leak runs before anybody goes to the surface [`Held`] already names.
    ///
    /// So a credential [`Held::tier`] puts at [`Tier::HeldBriefly`] has to have one, and a
    /// credential at Held may. Recorded rather than derived either way, so a credential added at
    /// Held briefly and left unsized is a disagreement a test sees.
    ///
    /// It does not establish the tier and cannot move it. A rota that stopped watching would make
    /// this number larger and leave the session credential exactly where the gate walk left it.
    ///
    /// **Where the fifteen minutes comes from.** It is a judgement about this deployment, which is
    /// one person's machine, and not a property of the arrangement. Nothing here watches for a use
    /// of a credential this program holds: the notice comes from the AWS account's own trail,
    /// where a call made with the session appears rather than here, and ending the session before
    /// its expiry is then one request at its issuer. Fifteen minutes is how long that takes
    /// somebody who is reading the trail. Where nobody reads it, nothing notices at all, which is
    /// the judgement this figure exists to let a person make.
    pub fn noticed_within(self) -> Option<Duration> {
        match self {
            Self::AwsSession => Some(Duration::from_secs(15 * 60)),
            Self::SigningKey
            | Self::AwsAccessKey
            | Self::GatewayToken { .. }
            | Self::SubscriptionBatch => None,
        }
    }

    /// Whether revoking this credential at its issuer leaves something minted from it working.
    ///
    /// The one of the three facts that is a claim about what happens rather than an address, and
    /// the one somebody acting on a leak gets wrong: deleting an access key is the obvious move,
    /// and it does not reach the session credentials STS has already handed out under it, each of
    /// which runs to its own expiry.
    ///
    /// False for a gateway token: the gateway that revokes it is the only place it is presented,
    /// and nothing here mints anything from it. A copy of the same token in a file or a shell
    /// profile is that credential rather than something derived from it, so revoking reaches it.
    ///
    /// False for a subscription batch on the other side of the same question: the batch is what
    /// was minted, and nothing is minted from a credential in it. A re-import mints a further
    /// batch from the order rather than from this one, so it is the order that has something under
    /// it, and the order is not a credential this program holds.
    pub fn outlives_revocation(self) -> bool {
        matches!(self, Self::AwsAccessKey)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum ConfigError {
    Missing(&'static str),
    Empty(&'static str),
    InvalidEndpoint { value: String },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing(name) => write!(
                f,
                "{name} is not set and was not built in; copy .envrc.example to .envrc and run `direnv allow`"
            ),
            Self::Empty(name) => write!(f, "{name} is set but empty"),
            Self::InvalidEndpoint { value } => write!(
                f,
                "endpoint must start with http:// or https://, got '{value}'"
            ),
        }
    }
}

impl std::error::Error for ConfigError {}

/// Everything needed to talk to the backends this build can reach.
#[derive(Debug, Clone)]
pub struct Config {
    /// Bedrock configuration, when a settings file or the environment asked for it.
    ///
    /// Additive, and not a choice of backend: the tiers named here are offered alongside the aichat
    /// roster and the model a person picks is what decides where a request goes. The aichat fields
    /// below may still be blank, since somebody pointing the agent at their own AWS account is not
    /// required to hold Brave service credentials as well.
    pub bedrock: Option<bedrock::Bedrock>,
    /// Gateways the settings file configured, in the order it listed them.
    ///
    /// Additive on the same terms as [`Config::bedrock`]: the models named here are offered beside
    /// the other rosters, and the model a person picks is what decides where a request goes.
    pub providers: Vec<provider::Provider>,
    /// HMAC signing key. Never transmitted; used only to sign the request digest.
    pub signing_key: Secret,
    /// Key id sent in the Authorization header. The server derives its copy of the
    /// signing key from a master seed plus this id, so the two are a matched pair.
    pub key_id: String,
    /// Base URL. The API path is appended by the client.
    pub endpoint: String,
    /// Base URL for the premium tier, when this build has one.
    ///
    /// `None` leaves premium unavailable, which is the right outcome for a build that was never
    /// given the host: falling back to the free endpoint with a subscription credential attached
    /// would send the credential somewhere it does not belong.
    pub premium_endpoint: Option<String>,
    /// Model to request when the user has not picked one. The server may substitute a different
    /// one regardless.
    pub default_model: String,
    /// How many prompt tokens one request may reach before the conversation is compacted.
    ///
    /// A guess, and it has to be one. The server reports what a request cost but never what it
    /// had room for, the default model is `automatic` and resolves per request, and there is no
    /// tokeniser here to count with. So this is a number chosen to be comfortably under the
    /// smallest window worth using, and [`env_var::CONTEXT_BUDGET`] is the way out of it for
    /// anyone whose model is smaller or much larger.
    pub context_budget: u64,
    /// Whether [`Config::context_budget`] was set by hand.
    ///
    /// A budget somebody typed outranks one the endpoint advertised: they may know something about
    /// their model that the listing does not say, and silently replacing it would make the setting
    /// look broken. Nothing reads this except [`Config::adopt_window`].
    budget_was_chosen: bool,
    /// Whether the context budget was adopted from an advertised window.
    budget_was_advertised: bool,
}

/// A value captured when this binary was built, or `None` if the build had none.
///
/// The masking is undone here rather than at startup so a credential is materialised
/// only for the variable actually being read.
fn built_in(name: &str) -> Option<String> {
    let (_, masked) = BAKED.iter().find(|(baked, _)| *baked == name)?;
    String::from_utf8(obfuscate::mask(masked)).ok()
}

/// Pick between what the environment says and what the build baked in.
fn resolve(
    name: &str,
    from_env: Option<String>,
    baked: impl Fn(&str) -> Option<String>,
) -> Option<String> {
    match from_env {
        Some(value) if !value.trim().is_empty() => Some(value),
        // A placeholder .envrc leaves a variable blank, which is not a request to
        // override, so a built-in value still applies. With none, the blank is handed
        // on so the error reports it as empty rather than missing.
        blank => baked(name).or(blank),
    }
}

/// Which model to request, from the three places that may name one.
///
/// Ranked differently from every other value, which is why it is written out rather than going
/// through [`resolve`]. The settings file sits *above* the baked-in value here: every release bakes
/// in a default model, so a `model` key ranked below it would lose on every binary anybody was
/// given, leaving a key that parses, is reported by `doctor`, and changes nothing outside a source
/// build.
///
/// It sits above an exported variable too, the one place a person's settings file outranks one. The
/// variable names a default, which is what a `.envrc` that exports it for every checkout means by
/// it, and a file that names a model is somebody choosing one. A variable ranked above the file
/// would also rank above a saved `/model` pick, which answers as the person's own file does.
/// `env.BRAVE_AI_CHAT_DEFAULT_MODEL` is read last, below the baked-in value, because that block is
/// variables and is ranked like them.
fn resolve_model(
    from_env: Option<String>,
    settings: &Settings,
    baked: impl Fn(&str) -> Option<String>,
) -> Option<String> {
    settings
        .model()
        .map(str::to_string)
        .or_else(|| from_env.filter(|value| !value.trim().is_empty()))
        .or_else(|| baked(env_var::DEFAULT_MODEL))
        .or_else(|| settings.get(env_var::DEFAULT_MODEL).map(str::to_string))
}

impl Config {
    /// Read configuration from the process environment, falling back to the values
    /// built into this binary.
    ///
    /// The environment wins over both, so a developer can point a released binary at a local
    /// backend without rebuilding it. What the machine-level file pinned wins over the environment
    /// for every value, and a `model` key in a settings file wins over it for the model, since the
    /// exported model is a default (`resolve_model` says why).
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_env_and_settings(&Settings::load(), &Managed::load())
    }

    /// Read configuration from the environment, with a settings file underneath it and the
    /// machine-level layer above it.
    ///
    /// The environment wins over the settings file for the same reason it wins over a baked-in
    /// value: a variable someone exported for one session is the most specific thing they said, and
    /// a file that overrode it would make `AWS_PROFILE=other bravebot` do nothing.
    ///
    /// A name the managed layer pinned is the exception, and the inversion is the point of that
    /// layer: everything else here is the individual's to change, so a pin an exported variable
    /// outranked would pin nothing. It answers for the names it pinned and for no others, which is
    /// [`Managed`]'s to decide.
    ///
    /// The `model` key is the other exception, and sits above the baked-in value rather than below
    /// it. Every release bakes in a default model, so a `model` key ranked like the `env` block
    /// would lose to it on every binary anybody was given: the key would parse, be reported by
    /// `doctor`, and change nothing outside a source build. It outranks an exported variable too,
    /// for the reason [`resolve_model`] gives.
    pub fn from_env_and_settings(
        settings: &Settings,
        managed: &Managed,
    ) -> Result<Self, ConfigError> {
        Self::from_lookup_with_providers(
            |key| match managed.get(key) {
                Some(pinned) => Some(pinned.to_string()),
                None => match key {
                    env_var::DEFAULT_MODEL => resolve_model(env::var(key).ok(), settings, built_in),
                    _ => resolve(key, env::var(key).ok(), built_in)
                        .or_else(|| settings.get(key).map(str::to_string)),
                },
            },
            // A gateway is a destination too, so an approved endpoint pins nothing while anybody can
            // add one beside it. The managed block replaces the person's rather than merging with
            // it, empty included, which is the only way to say that there are to be none.
            match managed.gateways() {
                Some(gateways) => gateways.to_vec(),
                None => settings.providers().to_vec(),
            },
        )
    }

    /// Read configuration from an arbitrary lookup, so tests need not mutate global
    /// process state.
    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Result<Self, ConfigError> {
        Self::from_lookup_with_providers(lookup, Vec::new())
    }

    /// Providers must be known before validating credentials for the other backends.
    fn from_lookup_with_providers(
        lookup: impl Fn(&str) -> Option<String>,
        providers: Vec<provider::Provider>,
    ) -> Result<Self, ConfigError> {
        // Applied here rather than where either route to an account is read, because a `provider`
        // block is parsed with no environment to consult and both routes have to answer the same.
        // Nonsense falls back to what the models state, on the footing the context budget is read
        // on: a mistyped figure should cost the setting, not the ceiling that lets a reply finish.
        let output_budget = lookup(env_var::OUTPUT_BUDGET)
            .and_then(|value| value.trim().parse::<u64>().ok())
            .filter(|budget| *budget > 0);
        let bedrock =
            bedrock::Bedrock::from_lookup(&lookup).map(|it| it.with_output_budget(output_budget));
        let providers: Vec<provider::Provider> = providers
            .into_iter()
            .map(|mut entry| {
                entry.bedrock = entry
                    .bedrock
                    .map(|account| account.with_output_budget(output_budget));
                entry
            })
            .collect();

        // With Bedrock or a gateway configured these are not requirements. A user of another
        // backend need not hold Brave service credentials as well. Released binaries have them
        // baked in anyway, so the demand there would be satisfied by values nothing goes on to read.
        //
        // Blank rather than absent is what the rest of this reads to decide whether the Brave roster
        // is offered at all. See [`Config::serves_aichat`].
        let optional = bedrock.is_some() || !providers.is_empty();

        let required = |name: &'static str| -> Result<String, ConfigError> {
            match lookup(name) {
                Some(v) if !v.trim().is_empty() => Ok(v),
                _ if optional => Ok(String::new()),
                None => Err(ConfigError::Missing(name)),
                Some(_) => Err(ConfigError::Empty(name)),
            }
        };

        let signing_key = Secret::new(required(env_var::SIGNING_KEY)?);
        let key_id = required(env_var::KEY_ID)?;
        let endpoint = required(env_var::ENDPOINT)?;

        // A blank endpoint only reaches here when another backend is configured and no aichat URL
        // is built. Checking the scheme of a value nothing will use would refuse a working
        // configuration over a field it does not have.
        let names_a_scheme = endpoint.starts_with("https://") || endpoint.starts_with("http://");
        if !endpoint.is_empty() && !names_a_scheme {
            return Err(ConfigError::InvalidEndpoint { value: endpoint });
        }

        // [`DEFAULT_MODEL`] for everyone, Bedrock block or not: it is the Brave backend's own
        // default for this product and a settings block adds a roster rather than changing what
        // answers when nobody has picked. The exception is a build that cannot reach Brave at all,
        // where that name reaches a backend with no credentials and the strongest configured tier
        // is the only thing that can answer.
        let reaches_brave = holds_brave_credentials(&endpoint, &key_id, signing_key.expose());
        let account = tier_account(bedrock.as_ref(), &providers);
        let default_model = resolved_model(
            &lookup(env_var::DEFAULT_MODEL)
                .filter(|m| !m.trim().is_empty())
                .or_else(|| match account {
                    Some(account) if !reaches_brave => account.default_model().map(str::to_string),
                    _ => None,
                })
                .unwrap_or_else(|| DEFAULT_MODEL.to_string()),
            account,
            reaches_brave,
        );

        // A premium host that is present but malformed is dropped rather than rejected: it only
        // matters to someone who has imported a subscription, and failing every run over it would
        // punish everyone else.
        let premium_endpoint = lookup(env_var::PREMIUM_ENDPOINT)
            .map(|value| value.trim().to_string())
            .filter(|value| value.starts_with("https://") || value.starts_with("http://"))
            .map(|value| value.trim_end_matches('/').to_string());

        // Nonsense falls back to the default rather than disabling compaction, which is what a
        // zero or an unparseable value would otherwise quietly do. A mistyped budget should cost
        // someone the setting they wanted, not the thing that keeps their session alive.
        let chosen_budget = lookup(env_var::CONTEXT_BUDGET)
            .and_then(|value| value.trim().parse::<u64>().ok())
            .filter(|budget| *budget > 0);

        // No Bedrock window applied here, though the block may be present. Both rosters are offered
        // at once now, so the window belongs to whichever model was picked rather than to the
        // configuration, and `adopt_window` takes it from the entry that named it. Applying the
        // Bedrock figure to a session that goes on to use a Brave model would set a budget for a
        // window that is not the one in force.
        let context_budget = chosen_budget.unwrap_or(DEFAULT_CONTEXT_BUDGET);
        let budget_was_chosen = chosen_budget.is_some();
        let budget_was_advertised = false;

        Ok(Self {
            bedrock,
            providers,
            signing_key,
            key_id,
            endpoint: endpoint.trim_end_matches('/').to_string(),
            premium_endpoint,
            default_model,
            context_budget,
            budget_was_chosen,
            budget_was_advertised,
        })
    }

    /// Whether the context budget is a guess rather than set by hand or advertised by the model.
    pub fn budget_is_guessed(&self) -> bool {
        !self.budget_was_chosen && !self.budget_was_advertised
    }

    /// Take the window the endpoint advertised for the model in use, where it is worth taking.
    ///
    /// Ignored when a budget was set by hand, and when the endpoint advertised nothing or something
    /// too small to work in: in both cases what is already here stands. Returns whether the budget
    /// changed, so a caller can say so once rather than every turn.
    ///
    /// A window that was not advertised still stops the budget claiming to be one. What stands is
    /// then a figure taken for a model that is no longer in force, which is a guess in the sense
    /// [`Config::budget_is_guessed`] means: good enough to compact against, not good enough to
    /// state a percentage against without saying so.
    pub fn adopt_window(&mut self, advertised: Option<u64>) -> bool {
        if self.budget_was_chosen {
            return false;
        }
        let advertised = budget_for_window(advertised);
        self.budget_was_advertised = advertised.is_some();
        match advertised {
            Some(budget) if budget != self.context_budget => {
                self.context_budget = budget;
                true
            }
            _ => false,
        }
    }

    /// The gateway serving `model`, and the name to ask it for.
    ///
    /// The model names the service, so this is the whole of how a request reaches a gateway. Two
    /// spellings reach one:
    ///
    /// - `openrouter/z-ai/glm-4.6`, the provider's own id and then the name that gateway knows the
    ///   model by. Split at the first separator only, because the remainder is the gateway's to
    ///   spell and most of them contain one.
    /// - the gateway's bare name, where the block listed it. Kept because a name chosen before this
    ///   crate qualified anything is still sitting in `~/.bravebot/model`.
    ///
    /// A qualified name is tried first. It is the one spelling that says which service is meant, so
    /// a bare name that two blocks both list resolves to the first of them while a qualified one
    /// cannot be ambiguous at all.
    ///
    /// The returned name is what goes on the wire. A gateway has never heard of the id this crate
    /// files it under, so returning them together is what stops the qualified form reaching it.
    pub fn provider_for<'name>(
        &self,
        model: &'name str,
    ) -> Option<(&provider::Provider, &'name str)> {
        if let Some((id, wire)) = model.split_once('/').filter(|(_, wire)| !wire.is_empty())
            && let Some(provider) = self
                .providers
                .iter()
                .find(|provider| provider.id == id && provider.bedrock.is_none())
        {
            return Some((provider, wire));
        }
        self.providers
            .iter()
            .filter(|provider| provider.bedrock.is_none())
            .find(|provider| provider.offers(model))
            .map(|provider| (provider, model))
    }

    /// The AWS account that serves `model`, from the tier variables or from a `provider` block.
    ///
    /// The tier variables are asked first, being the older way to name a Bedrock model and the one
    /// a tier word resolves against. A name neither offers belongs to some other service.
    pub fn bedrock_for(&self, model: &str) -> Option<&bedrock::Bedrock> {
        if let Some(bedrock) = self.bedrock.as_ref().filter(|it| it.offers(model)) {
            return Some(bedrock);
        }
        self.providers
            .iter()
            .filter_map(|provider| provider.bedrock.as_ref())
            .find(|bedrock| bedrock.offers(model))
    }

    /// Every AWS account a `provider` block named, in the order the file listed them.
    pub fn bedrock_providers(
        &self,
    ) -> impl Iterator<Item = (&provider::Provider, &bedrock::Bedrock)> {
        self.providers
            .iter()
            .filter_map(|provider| provider.bedrock.as_ref().map(|bedrock| (provider, bedrock)))
    }

    /// Whether the Brave backend can be reached at all.
    ///
    /// Every ordinary build can: the credentials are baked in, and a missing one is refused at
    /// startup. A build from source pointed at Bedrock or a gateway is the exception: credentials
    /// may be blank, and offering that roster would list models whose requests fail unsigned.
    pub fn serves_aichat(&self) -> bool {
        holds_brave_credentials(&self.endpoint, &self.key_id, self.signing_key.expose())
    }

    /// The AWS account a tier word resolves against, which is [`tier_account`] over this build.
    fn tier_account(&self) -> Option<&bedrock::Bedrock> {
        tier_account(self.bedrock.as_ref(), &self.providers)
    }

    /// The credentials this build holds itself, each owing an account of what would end it.
    ///
    /// Both AWS arrangements wherever an account is configured, because which one a profile
    /// resolves to is the AWS CLI's answer and asking it means running it, which a report about
    /// configuration does not do. A resolved credential says which it is for itself.
    ///
    /// A build that cannot reach the Brave backend holds no signing key to account for, which is
    /// the build-from-source case [`Config::serves_aichat`] describes: the field is blank there,
    /// and listing a credential this install does not have would send somebody to retire a key id
    /// on the strength of a leak that cannot have come from here.
    ///
    /// Then one entry per gateway whose block says anywhere a token lives, which is custody this
    /// configuration arranged however the value arrives. Whether the variable a block names is
    /// set is a fact about an environment this reads none of, and asking would make the record
    /// differ between two runs of the same configuration; the AWS pair above is listed on the
    /// same footing, for the same reason. A block naming nowhere for a token has said none is
    /// needed and is left out, as is one reaching Bedrock, whose credentials are the AWS pair and
    /// would otherwise be counted twice.
    ///
    /// `subscription_imported` is asked of the caller because the batch lives in a file
    /// `bravebot-skus` owns and nothing here depends on that crate. It is a fact about the machine
    /// rather than about this configuration, which is why it arrives as an argument rather than
    /// being read: the same configuration on a machine with no import holds one credential fewer.
    /// A build that cannot reach the Brave backend is left out of it whatever the file holds,
    /// because the batch buys the premium half of that backend's roster and nothing else reads it,
    /// and CRED-2 gives a credential nothing reads no tier.
    pub fn held(&self, subscription_imported: bool) -> Vec<Held<'_>> {
        let mut held = Vec::new();
        if self.serves_aichat() {
            held.push(Held::SigningKey);
        }
        if self.bedrock.is_some() || self.bedrock_providers().next().is_some() {
            held.push(Held::AwsAccessKey);
            held.push(Held::AwsSession);
        }
        held.extend(
            self.providers
                .iter()
                .filter(|provider| provider.bedrock.is_none() && provider.names_a_credential())
                .map(|provider| Held::GatewayToken {
                    host: provider.host(),
                }),
        );
        if subscription_imported && self.serves_aichat() {
            held.push(Held::SubscriptionBatch);
        }
        held
    }

    /// The model a name asks for, resolved the way the configured model is.
    ///
    /// For every other route to a model: a flag, a remembered choice, anything a person types. The
    /// three tier words and the older spelling of the routing entry are configuration's business
    /// rather than the settings key's, so a route that resolved names itself would accept a
    /// different set of them, and one that resolved none would send a word the service has never
    /// heard of and be answered by whatever it substitutes.
    pub fn model_named(&self, name: &str) -> String {
        resolved_model(name, self.tier_account(), self.serves_aichat())
    }

    /// Full URL for the OpenAI-compatible chat completions endpoint.
    ///
    /// This is the v2 API: the version is inferred from the path by the server, so
    /// there is no `/v2/` route to construct.
    pub fn chat_completions_url(&self) -> String {
        format!("{}/v1/chat/completions", self.endpoint)
    }

    /// Full URL for the model listing.
    ///
    /// The free host, always: the listing reports which models are premium rather than differing
    /// between the two, and asking the premium host would spend a subscription credential to learn
    /// something the free one says for nothing. Callers send `Brave-Product: bravebot` so the
    /// roster is the one curated for this agent rather than Leo's.
    pub fn models_url(&self) -> String {
        format!("{}/v1/models", self.endpoint)
    }

    /// The same endpoint on the premium host, when this build has one.
    pub fn premium_chat_completions_url(&self) -> Option<String> {
        self.premium_endpoint
            .as_ref()
            .map(|base| format!("{base}/v1/chat/completions"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn complete_env(key: &str) -> Option<String> {
        match key {
            env_var::SIGNING_KEY => Some("test-signing-key".into()),
            env_var::KEY_ID => Some("test-key-id".into()),
            env_var::ENDPOINT => Some("https://example.invalid".into()),
            _ => None,
        }
    }

    #[test]
    fn reads_a_complete_environment() {
        let config = Config::from_lookup(complete_env).unwrap();
        assert_eq!(config.key_id, "test-key-id");
        assert_eq!(config.signing_key.expose(), "test-signing-key");
    }

    #[test]
    fn the_default_model_is_automatic() {
        let config = Config::from_lookup(complete_env).unwrap();
        assert_eq!(config.default_model, DEFAULT_MODEL);
    }

    #[test]
    fn the_default_model_can_be_overridden() {
        let config = Config::from_lookup(|k| match k {
            env_var::DEFAULT_MODEL => Some("some-pinned-model".into()),
            other => complete_env(other),
        })
        .unwrap();
        assert_eq!(config.default_model, "some-pinned-model");
    }

    #[test]
    fn the_context_budget_has_a_default() {
        let config = Config::from_lookup(complete_env).unwrap();
        assert_eq!(config.context_budget, DEFAULT_CONTEXT_BUDGET);
    }

    /// The point of the whole exercise: the endpoint says what the model's window is, so the
    /// default stops standing in for it. Recorded sessions compacted at 24,000 against a model
    /// advertising 102,400, which is four times the conversation it could have held.
    #[test]
    fn an_advertised_window_replaces_the_default() {
        let mut config = Config::from_lookup(complete_env).unwrap();
        assert!(config.adopt_window(Some(102_400)));
        assert_eq!(config.context_budget, 102_400);
    }

    /// A cramped window is taken at its word. Raising it to something more comfortable would put
    /// the budget above the window, and a budget above the window does not delay compaction, it
    /// removes it: the failure this whole figure exists to prevent.
    #[test]
    fn a_small_advertised_window_is_believed_rather_than_raised() {
        let mut config = Config::from_lookup(complete_env).unwrap();
        assert!(config.adopt_window(Some(6_400)));
        assert_eq!(config.context_budget, 6_400);
    }

    /// The placeholder the endpoint sends for a model whose window it does not know. A budget of one
    /// token would compact before a turn could start, so it means "did not say".
    #[test]
    fn the_placeholder_window_is_not_adopted() {
        let mut config = Config::from_lookup(complete_env).unwrap();
        assert!(!config.adopt_window(Some(1)));
        assert_eq!(config.context_budget, DEFAULT_CONTEXT_BUDGET);
    }

    /// `automatic` resolves per request, so nothing was advertised and the default has to stand.
    #[test]
    fn nothing_advertised_leaves_the_default_alone() {
        let mut config = Config::from_lookup(complete_env).unwrap();
        assert!(!config.adopt_window(None));
        assert_eq!(config.context_budget, DEFAULT_CONTEXT_BUDGET);
    }

    /// A budget somebody typed outranks one the endpoint advertised: they may know something about
    /// their model the listing does not say, and overriding it would make the setting look broken.
    #[test]
    fn a_budget_set_by_hand_is_not_replaced_by_an_advertised_one() {
        let mut config = Config::from_lookup(|k| match k {
            env_var::CONTEXT_BUDGET => Some("4096".into()),
            other => complete_env(other),
        })
        .unwrap();
        assert!(!config.adopt_window(Some(102_400)));
        assert_eq!(config.context_budget, 4096);
    }

    /// Adopting the budget already in force is not a change, so nothing is said about it twice.
    #[test]
    fn adopting_the_budget_already_in_use_reports_no_change() {
        let mut config = Config::from_lookup(complete_env).unwrap();
        assert!(!config.adopt_window(Some(DEFAULT_CONTEXT_BUDGET)));
    }

    #[test]
    fn a_default_budget_is_marked_as_guessed() {
        let config = Config::from_lookup(complete_env).unwrap();
        assert!(config.budget_is_guessed());
    }

    #[test]
    fn an_advertised_budget_is_not_marked_as_guessed() {
        let mut config = Config::from_lookup(complete_env).unwrap();
        assert!(config.adopt_window(Some(102_400)));
        assert!(!config.budget_is_guessed());
    }

    /// A budget that came from one model's window is not a claim about the next one, and the
    /// figure on the hint line says so. Reverting to the default instead would raise the budget
    /// above a cramped window still in force, and a budget above the window does not delay
    /// compaction, it removes it.
    #[test]
    fn a_window_nobody_advertised_leaves_an_adopted_budget_standing_and_marks_it_guessed() {
        let mut config = Config::from_lookup(complete_env).unwrap();
        assert!(config.adopt_window(Some(6_400)));
        assert!(!config.budget_is_guessed());

        assert!(!config.adopt_window(None));
        assert_eq!(config.context_budget, 6_400);
        assert!(config.budget_is_guessed());
    }

    #[test]
    fn a_budget_set_by_hand_is_not_marked_as_guessed() {
        let config = Config::from_lookup(|k| match k {
            env_var::CONTEXT_BUDGET => Some("4096".into()),
            other => complete_env(other),
        })
        .unwrap();
        assert!(!config.budget_is_guessed());
    }

    /// The default is a guess at a window nobody reports, so someone running a model it is wrong
    /// for needs a way to say so without rebuilding.
    #[test]
    fn the_context_budget_can_be_overridden() {
        let config = Config::from_lookup(|k| match k {
            env_var::CONTEXT_BUDGET => Some(" 4096 ".into()),
            other => complete_env(other),
        })
        .unwrap();
        assert_eq!(config.context_budget, 4096);
    }

    /// A mistyped budget must cost someone the setting they wanted, never compaction itself: a
    /// zero or a word would otherwise read as a conversation that may grow forever, which is the
    /// failure the budget exists to prevent.
    #[test]
    fn a_budget_that_makes_no_sense_falls_back_rather_than_disabling_compaction() {
        for value in ["0", "", "   ", "lots", "-1", "1e6", "100_000"] {
            let config = Config::from_lookup(|k| match k {
                env_var::CONTEXT_BUDGET => Some(value.into()),
                other => complete_env(other),
            })
            .unwrap();
            assert_eq!(
                config.context_budget, DEFAULT_CONTEXT_BUDGET,
                "{value:?} was read as a budget"
            );
        }
    }

    /// The variable is prefixed like every other one. A bare `MODEL` is a name anything in a
    /// shared shell profile might already export, and reading it would take that as an
    /// instruction about which model to send.
    #[test]
    fn a_bare_model_variable_is_not_read() {
        let config = Config::from_lookup(|k| match k {
            "MODEL" => Some("something-else-set-this".into()),
            other => complete_env(other),
        })
        .unwrap();
        assert_eq!(config.default_model, DEFAULT_MODEL);
    }

    /// An empty value should fall back rather than sending "" and being silently
    /// reset by the server.
    #[test]
    fn an_empty_default_model_falls_back() {
        let config = Config::from_lookup(|k| match k {
            env_var::DEFAULT_MODEL => Some("   ".into()),
            other => complete_env(other),
        })
        .unwrap();
        assert_eq!(config.default_model, DEFAULT_MODEL);
    }

    /// The point of the whole feature: a settings file naming an AWS account is enough to reach a
    /// backend, without the Brave service credentials the other one needs.
    #[test]
    fn bedrock_configures_a_backend_without_any_aichat_credentials() {
        let config = Config::from_lookup(|k| match k {
            env_var::USE_BEDROCK => Some("1".into()),
            env_var::AWS_REGION => Some("us-west-2".into()),
            env_var::BEDROCK_OPUS_MODEL => Some("opus-arn".into()),
            _ => None,
        })
        .expect("bedrock alone is a working configuration");
        let bedrock = config.bedrock.expect("bedrock configured");
        assert_eq!(bedrock.default_model(), Some("opus-arn"));
    }

    /// CRED-25: what is reported as held is what this build actually holds. The signing key field
    /// exists on every configuration and is blank on a build from source pointed elsewhere, so a
    /// rule reading the field rather than whether it is usable would hand somebody the account of
    /// a credential this install has none of, and send them to retire a key id over a leak that
    /// cannot have come from here.
    #[test]
    fn a_build_that_cannot_sign_for_itself_holds_no_signing_key_to_account_for() {
        let bedrock_only = Config::from_lookup(|k| match k {
            env_var::USE_BEDROCK => Some("1".into()),
            env_var::AWS_REGION => Some("us-west-2".into()),
            _ => None,
        })
        .expect("bedrock alone is a working configuration");
        assert!(!bedrock_only.serves_aichat());
        assert_eq!(
            bedrock_only.held(false),
            [Held::AwsAccessKey, Held::AwsSession],
            "a build with a blank signing key holds no signing key"
        );

        let brave_only = Config::from_lookup(complete_env).expect("configured");
        assert_eq!(
            brave_only.held(false),
            [Held::SigningKey],
            "a build with no AWS account has no AWS credential to account for"
        );
    }

    /// CRED-9: gate 3 is decided by whether renewing a credential needs authority this agent
    /// cannot supply, and for none of the credentials this program holds that reach it does it. An
    /// AWS session credential is the one that reads as briefer than the rest, since it states an
    /// expiry and an issuer that could end it minted it, so a record deciding the tier from the
    /// value would put it a tier above the others. What the clause asks instead is who has to be
    /// asked for the next one, and nothing here has to ask anybody: a record putting it at Held
    /// briefly reports a permanent credential as a bounded one, and offers waiting out an expiry
    /// as though it were a disposition.
    ///
    /// The session's drop at gate 3 is on the clause's own condition, since the other two are
    /// ones it meets: STS mints it for the profile and AWS ends it. A drop recorded on either of
    /// those would state a reason that is not the reason.
    #[test]
    fn no_credential_this_program_holds_stands_at_held_briefly() {
        for held in Held::all("gateway.invalid") {
            assert_ne!(
                held.tier(),
                Tier::HeldBriefly,
                "{held:?} is recorded at Held briefly, and nothing establishes that renewing it needs a person"
            );
        }

        let at_gate_three: Vec<Condition> = Held::AwsSession
            .walk()
            .iter()
            .map(|drop| drop.condition)
            .filter(|condition| condition.gate() == Gate::Three)
            .collect();
        assert_eq!(
            at_gate_three,
            [Condition::RenewableWithoutAuthority],
            "the session credential drops at gate 3 on something other than being renewed unaided"
        );
    }

    /// CRED-25: both AWS arrangements, because which one a profile resolves to is the AWS CLI's
    /// answer and this is configuration rather than a resolved credential. They differ in what
    /// would end them, so reporting one of the two would be wrong for half the machines that read
    /// it, and a resolved credential says which it is for itself.
    #[test]
    fn an_aws_account_holds_both_arrangements_and_they_end_differently() {
        let config = Config::from_lookup(|k| match k {
            env_var::USE_BEDROCK => Some("1".into()),
            env_var::AWS_REGION => Some("us-west-2".into()),
            other => complete_env(other),
        })
        .expect("configured");
        assert_eq!(
            config.held(false),
            [Held::SigningKey, Held::AwsAccessKey, Held::AwsSession]
        );

        // The one disposition somebody reaches for after a leak that leaves something behind.
        assert!(Held::AwsAccessKey.outlives_revocation());
        assert!(!Held::AwsSession.outlives_revocation());
        assert!(!Held::SigningKey.outlives_revocation());
    }

    /// CRED-25: a gateway's bearer token is a credential this configuration holds, so it is in the
    /// record with the host that would end it. Left out, the person whose gateway key appears in a
    /// pasted log is given nothing to act on, which the clause's Why calls a notification.
    ///
    /// Four blocks rather than one, because the shapes that must not be recorded are what a rule
    /// reading "every provider" would get wrong: a block naming nowhere for a token has said none
    /// is needed, and a block reaching Bedrock holds the AWS pair above rather than a bearer token
    /// of its own, so recording one for it would count the same credentials twice and send
    /// somebody to revoke a token at an endpoint that issues none. The two that are recorded
    /// differ in where the value sits, the file and a variable, since custody is the same either
    /// way and a rule reading only `apiKey` would pass on the one the report is read from most.
    #[test]
    fn a_gateway_token_is_a_credential_this_configuration_holds() {
        let settings = Settings::parse(
            r#"{"provider": {
                "in-the-file": {
                    "options": {"baseURL": "https://in-the-file.invalid/v1", "apiKey": "sk-live"}
                },
                "in-a-variable": {
                    "env": ["A_GATEWAY_TOKEN"],
                    "options": {"baseURL": "https://in-a-variable.invalid/v1"}
                },
                "needs-none": {"options": {"baseURL": "http://localhost:11434/v1"}},
                "amazon-bedrock": {"options": {"region": "us-west-2"}, "models": {"an-arn": {}}}
            }}"#,
        );
        let config =
            Config::from_lookup_with_providers(complete_env, settings.providers().to_vec())
                .expect("configured");

        // The gateways in the order the blocks are read, which is by id.
        assert_eq!(
            config.held(false),
            [
                Held::SigningKey,
                Held::AwsAccessKey,
                Held::AwsSession,
                Held::GatewayToken {
                    host: "in-a-variable.invalid"
                },
                Held::GatewayToken {
                    host: "in-the-file.invalid"
                },
            ],
            "the record is not the credentials this configuration holds"
        );

        // Nothing is minted from a bearer token here, so revoking it at the gateway reaches every
        // copy of it. A line saying otherwise would send somebody chasing a session that does not
        // exist.
        assert!(
            !Held::GatewayToken {
                host: "in-the-file.invalid"
            }
            .outlives_revocation()
        );
    }

    /// CRED-3: a credential's walk holds one drop per gate it failed, asked in order from the top,
    /// and stops where its tier says it stopped. A walk that skipped a gate, repeated one or ran
    /// past the tier would be a record of a different credential than the one standing there, and
    /// the tier is the one thing about the walk anything else already reads.
    ///
    /// Each drop's gate comes from the condition it names, so a condition filed under the wrong
    /// gate arrives here as a walk whose gates are out of order.
    #[test]
    fn a_credentials_walk_holds_one_drop_per_gate_it_failed_and_stops_at_its_tier() {
        for held in Held::all("gateway.invalid") {
            let gates: Vec<Gate> = held
                .walk()
                .iter()
                .map(|drop| drop.condition.gate())
                .collect();

            assert_eq!(
                gates,
                [Gate::One, Gate::Two, Gate::Three][..gates.len()],
                "{held:?} records a walk that skips a gate or asks them out of order"
            );
            assert_eq!(
                held.walk().len(),
                match held.tier() {
                    Tier::Delegated => 0,
                    Tier::Granted => 1,
                    Tier::HeldBriefly => 2,
                    Tier::Held => 3,
                },
                "{held:?} stands at one tier and records the walk of another"
            );
        }
    }

    /// CRED-3: a drop says whether the counterparty refused or nobody attempted it, and every drop
    /// this configuration records is one nobody attempted. The two answers end at the same tier
    /// and only the second is ours to revisit, so recording a refusal where the arrangement is
    /// available files a decision made here as a fact about the world, and the credential stops
    /// being worth looking at again.
    ///
    /// True of all five: AWS mints a bounded session and ends it, this project's own backend
    /// issues the signing key, a gateway block names no issuer for anything to have asked, and
    /// Brave's subscription service mints a batch nobody has asked to gate use of. None of those
    /// is a counterparty saying no.
    #[test]
    fn every_drop_this_configuration_records_is_one_nobody_attempted() {
        for held in Held::all("gateway.invalid") {
            for drop in held.walk() {
                assert_eq!(
                    drop.attempt,
                    Attempt::NotAttempted,
                    "{held:?} excuses gate {} as refused",
                    drop.condition.gate().number()
                );
            }
        }
    }

    /// CRED-2: every credential in the record stands at a tier the walk arrived at, and nothing
    /// that handed material over is recorded at the top of the scale. The clause's Why is the
    /// failure this catches: a record that answered every credential with one tier would let
    /// silence claim the strongest, and the record would say nothing about any of them while
    /// appearing to say something about all of them.
    ///
    /// Held against the whole of the record rather than against a variant, because the credential a
    /// tier is missing from is the one nobody thought about, and against the distinct answers the
    /// walks arrive at because a constant is what a record with no walk behind it looks like from
    /// the outside.
    #[test]
    fn every_credential_in_the_record_stands_at_a_tier_the_walk_arrived_at() {
        let every = Held::all("gateway.invalid");

        for held in every {
            assert_ne!(
                held.tier(),
                Tier::Delegated,
                "{held:?} hands material over and is recorded as holding nothing"
            );
        }

        let mut tiers: Vec<String> = every
            .iter()
            .map(|held| format!("{:?}", held.tier()))
            .collect();
        tiers.sort();
        tiers.dedup();
        assert_eq!(
            tiers,
            ["Granted", "Held"],
            "the record does not answer its credentials with the tiers their walks stopped at"
        );

        // Each answer is the gate the arrangement stops at, and the reasoning is on the variant.
        assert_eq!(Held::SigningKey.tier(), Tier::Held);
        assert_eq!(Held::AwsAccessKey.tier(), Tier::Held);
        assert_eq!(Held::AwsSession.tier(), Tier::Held);
        assert_eq!(Held::SubscriptionBatch.tier(), Tier::Granted);

        // CRED-4: the tier is a claim about the arrangement, and a gateway's address is not part of
        // one. Two hosts answered differently would be a tier that moved without the arrangement
        // moving, which is the shape of a tier that can be argued.
        assert_eq!(
            Held::GatewayToken {
                host: "one.invalid"
            }
            .tier(),
            Held::GatewayToken {
                host: "two.invalid"
            }
            .tier(),
            "a gateway's tier is read off its address"
        );
    }

    /// CRED-2: an imported subscription's credential batch is a credential this machine holds and
    /// spends, so it is in the record with a tier of its own. Left out, a batch sitting in a file
    /// under the machine and spent by every premium request has no recorded tier at all, which is
    /// the outcome the clause forbids.
    ///
    /// Both directions, because the record is what a report walks: a rule that pushed the entry
    /// unconditionally would account for a subscription on every machine that has never imported
    /// one, and send somebody to a batch that does not exist. A Bedrock-only build is held to the
    /// absent case with a batch present, since nothing there reads it and CRED-2 gives a credential
    /// nothing reads no tier.
    #[test]
    fn an_imported_subscription_is_a_credential_this_configuration_holds() {
        let brave = Config::from_lookup(complete_env).expect("configured");
        assert!(brave.serves_aichat());
        assert_eq!(
            brave.held(true),
            [Held::SigningKey, Held::SubscriptionBatch],
            "an imported batch is not in the record"
        );
        assert_eq!(
            brave.held(false),
            [Held::SigningKey],
            "a machine that has imported nothing is accounted for a batch it does not hold"
        );

        let bedrock_only = Config::from_lookup(|k| match k {
            env_var::USE_BEDROCK => Some("1".into()),
            env_var::AWS_REGION => Some("us-west-2".into()),
            _ => None,
        })
        .expect("bedrock alone is a working configuration");
        assert!(!bedrock_only.serves_aichat());
        assert_eq!(
            bedrock_only.held(true),
            [Held::AwsAccessKey, Held::AwsSession],
            "a build that cannot reach the Brave backend is accounted for a batch nothing there spends"
        );
    }

    /// CRED-10: a credential the record stands at Held briefly is owed a figure for how quickly a
    /// leak of it would be noticed and acted on. The window is the whole of what bounds such a
    /// credential, so a window nobody sized is a number somebody liked.
    ///
    /// One direction and not both. The obligations ratchet downward, so a credential that drops
    /// to Held keeps the figure it owed and is owed detection besides, and a record that made the
    /// figure exclusive to Held briefly would drop it on the way down, which is the direction
    /// that needs it most.
    ///
    /// The tier and the figure are kept apart and pinned here so that a credential standing at
    /// Held briefly and left unsized fails rather than passing quietly.
    #[test]
    fn a_credential_at_held_briefly_is_sized_against_detection() {
        for held in Held::all("gateway.invalid") {
            assert!(
                held.tier() != Tier::HeldBriefly || held.noticed_within().is_some(),
                "{held:?} stands at Held briefly with no figure for how quickly a leak is noticed"
            );

            let Some(window) = held.noticed_within() else {
                continue;
            };
            assert!(
                window >= Duration::from_secs(60),
                "a figure under a minute is the unsized case wearing a number: {window:?}"
            );
            // Whole minutes, because minutes are what the report states: a figure of ninety
            // seconds would reach a person as one minute, which is a smaller window than
            // anybody recorded.
            assert_eq!(
                window.as_secs() % 60,
                0,
                "a figure that is not whole minutes is reported as a shorter one: {window:?}"
            );
        }
    }

    /// Without Bedrock the aichat credentials are still required. Relaxing them for everyone would
    /// turn a missing key into a confusing runtime failure instead of a startup error.
    #[test]
    fn the_aichat_credentials_are_still_required_without_bedrock() {
        let err = Config::from_lookup(|k| match k {
            env_var::SIGNING_KEY => None,
            other => complete_env(other),
        })
        .unwrap_err();
        assert_eq!(err, ConfigError::Missing(env_var::SIGNING_KEY));
    }

    /// A released binary has the aichat values baked in, so a Bedrock user's config carries them
    /// whether they wanted them or not. Their presence must not switch the backend back.
    #[test]
    fn bedrock_wins_over_baked_in_aichat_values() {
        let config = Config::from_lookup(|k| match k {
            env_var::USE_BEDROCK => Some("1".into()),
            env_var::AWS_REGION => Some("us-west-2".into()),
            other => complete_env(other),
        })
        .expect("configured");
        assert!(config.bedrock.is_some());
    }

    /// A Bedrock block adds a roster; it does not move the budget. Both backends are offered at once,
    /// so a session configured for Bedrock may spend every turn on a Brave model, and a budget set
    /// from the Bedrock window would sit above the window actually in force, which does not delay
    /// compaction but removes it. The figure is adopted when a tier is picked instead.
    #[test]
    fn a_bedrock_block_does_not_move_the_budget_off_the_default() {
        let mut config = Config::from_lookup(|k| match k {
            env_var::USE_BEDROCK => Some("1".into()),
            env_var::AWS_REGION => Some("us-west-2".into()),
            env_var::BEDROCK_OPUS_MODEL => Some("opus-arn".into()),
            other => complete_env(other),
        })
        .expect("configured");
        assert_eq!(config.context_budget, DEFAULT_CONTEXT_BUDGET);
        assert!(config.adopt_window(Some(bedrock::CONTEXT_WINDOW)));
        assert_eq!(config.context_budget, bedrock::CONTEXT_WINDOW);
    }

    /// A settings block adds to what a person may choose rather than changing what answers when they
    /// have chosen nothing. A new user has Brave and a block must not silently take that away.
    #[test]
    fn a_bedrock_block_does_not_change_the_default_model() {
        let config = Config::from_lookup(|k| match k {
            env_var::USE_BEDROCK => Some("1".into()),
            env_var::AWS_REGION => Some("us-west-2".into()),
            env_var::BEDROCK_OPUS_MODEL => Some("opus-arn".into()),
            other => complete_env(other),
        })
        .expect("configured");
        assert_eq!(config.default_model, DEFAULT_MODEL);
    }

    /// A gateway is additive on the same terms a Bedrock block is. Naming one adds to what a person
    /// may choose and must not decide what answers when they have chosen nothing, nor set the budget
    /// from a window belonging to a model the session may never use.
    #[test]
    fn a_provider_block_changes_neither_the_default_model_nor_the_budget() {
        let settings = Settings::parse(
            r#"{"provider": {"openrouter": {
                "options": {"baseURL": "https://openrouter.example.invalid/api/v1"},
                "models": {"z-ai/glm-4.6": {"limit": {"context": 1000000, "output": 64000}}}
            }}}"#,
        );
        let mut config =
            Config::from_lookup_with_providers(complete_env, settings.providers().to_vec())
                .expect("configured");

        assert!(config.serves_aichat());
        assert_eq!(config.signing_key.expose(), "test-signing-key");
        assert_eq!(config.key_id, "test-key-id");
        assert_eq!(config.endpoint, "https://example.invalid");
        assert_eq!(config.default_model, DEFAULT_MODEL);
        assert_eq!(config.context_budget, DEFAULT_CONTEXT_BUDGET);
        // Taken only once that model is the one in force, which is what picking it does.
        assert!(config.adopt_window(Some(1_000_000)));
    }

    #[test]
    fn a_gateway_configures_without_brave_credentials_or_a_model_roster() {
        let settings = Settings::parse(
            r#"{
            "model": "openrouter/z-ai/glm-4.6",
            "provider": {"openrouter": {"options": {"apiKey": "test-token"}}}
        }"#,
        );
        for blank in [false, true] {
            let config = Config::from_lookup_with_providers(
                |key| match key {
                    env_var::DEFAULT_MODEL => resolve_model(None, &settings, |_| None),
                    _ if blank => Some(String::new()),
                    _ => None,
                },
                settings.providers().to_vec(),
            )
            .expect("the selected gateway needs no Brave credentials");
            assert!(!config.serves_aichat());
            assert_eq!(config.default_model, "openrouter/z-ai/glm-4.6");
            let (provider, model) = config.provider_for(&config.default_model).expect("gateway");
            assert_eq!(model, "z-ai/glm-4.6");
            assert_eq!(provider.base_url, "https://openrouter.ai/api/v1");
            assert!(matches!(
                provider.credential(|_| None),
                provider::Credential::Token(token) if token.expose() == "test-token"
            ));
            assert!(provider.models.is_empty());
        }
    }

    #[test]
    fn a_gateway_can_read_its_own_environment_token_without_brave_credentials() {
        let settings = Settings::parse(
            r#"{
            "provider": {"openrouter": {"env": ["OPENROUTER_API_KEY"]}}
        }"#,
        );
        let config = Config::from_lookup_with_providers(|_| None, settings.providers().to_vec())
            .expect("gateway configured before its token is resolved");
        let provider = &config.providers[0];
        assert!(matches!(
            provider.credential(|name| match name {
                "OPENROUTER_API_KEY" => Some("environment-token".into()),
                _ => None,
            }),
            provider::Credential::Token(token) if token.expose() == "environment-token"
        ));
        // Missing gateway tokens are reported by the gateway client, not as missing Brave keys.
        assert!(matches!(
            provider.credential(|_| None),
            provider::Credential::Absent
        ));
        assert!(!config.serves_aichat());
    }

    #[test]
    fn an_absent_or_invalid_gateway_does_not_relax_brave_validation() {
        for text in [
            "{}",
            r#"{"provider": {}}"#,
            r#"{"provider": {"unknown": {}}}"#,
            r#"{"provider": {"unknown": {"options": {"baseURL": ""}}}}"#,
        ] {
            let settings = Settings::parse(text);
            for blank in [false, true] {
                let error = Config::from_lookup_with_providers(
                    |_| blank.then(String::new),
                    settings.providers().to_vec(),
                )
                .unwrap_err();
                assert_eq!(
                    error,
                    if blank {
                        ConfigError::Empty(env_var::SIGNING_KEY)
                    } else {
                        ConfigError::Missing(env_var::SIGNING_KEY)
                    }
                );
            }
        }
    }

    /// The two blocks reach different services and a name has to go to the one that recognises it.
    /// Routed as a gateway, an AWS model is sent OpenAI-compatible chat completions over a bearer
    /// token that does not exist, which is the gap a `provider` block for Bedrock exists to close.
    #[test]
    fn a_model_an_aws_block_named_reaches_bedrock_rather_than_a_gateway() {
        let settings = Settings::parse(
            r#"{"provider": {
                "amazon-bedrock": {
                    "options": {"region": "us-west-2", "profile": "sso"},
                    "models": {"openai.gpt-5.6-sol": {}}
                },
                "openrouter": {
                    "options": {"baseURL": "https://openrouter.ai/api/v1"},
                    "models": {"z-ai/glm-4.6": {}}
                }
            }}"#,
        );
        let config =
            Config::from_lookup_with_providers(complete_env, settings.providers().to_vec())
                .expect("configured");

        let bedrock = config
            .bedrock_for("openai.gpt-5.6-sol")
            .expect("the AWS account that serves it");
        assert_eq!(bedrock.region, "us-west-2");
        assert!(
            config.provider_for("openai.gpt-5.6-sol").is_none(),
            "an AWS model was offered to the gateway path"
        );

        // The gateway beside it is untouched by any of this.
        assert!(config.provider_for("z-ai/glm-4.6").is_some());
        assert!(config.bedrock_for("z-ai/glm-4.6").is_none());
    }

    /// A qualified name says which service is meant, and the AWS entry is not one a gateway request
    /// can be built for, so naming it must not produce one.
    #[test]
    fn a_name_qualified_by_the_aws_id_is_not_a_gateway_either() {
        let settings = Settings::parse(
            r#"{"provider": {"amazon-bedrock": {
                "options": {"region": "us-west-2"},
                "models": {"openai.gpt-5.6-sol": {}}
            }}}"#,
        );
        let config =
            Config::from_lookup_with_providers(complete_env, settings.providers().to_vec())
                .expect("configured");
        assert!(
            config
                .provider_for("amazon-bedrock/openai.gpt-5.6-sol")
                .is_none()
        );
    }

    #[test]
    fn a_gateway_does_not_hide_an_invalid_brave_endpoint() {
        let settings = Settings::parse(r#"{"provider": {"openrouter": {}}}"#);
        let error = Config::from_lookup_with_providers(
            |key| match key {
                env_var::ENDPOINT => Some("invalid".into()),
                other => complete_env(other),
            },
            settings.providers().to_vec(),
        )
        .unwrap_err();
        assert_eq!(
            error,
            ConfigError::InvalidEndpoint {
                value: "invalid".into()
            }
        );
    }

    #[test]
    fn a_gateway_keeps_bedrock_available_without_brave_credentials() {
        let settings = Settings::parse(r#"{"provider": {"openrouter": {}}}"#);
        let config = Config::from_lookup_with_providers(
            |key| match key {
                env_var::USE_BEDROCK => Some("1".into()),
                env_var::AWS_REGION => Some("us-west-2".into()),
                env_var::BEDROCK_OPUS_MODEL => Some("opus-arn".into()),
                _ => None,
            },
            settings.providers().to_vec(),
        )
        .expect("both backends configured");
        assert!(!config.serves_aichat());
        assert_eq!(config.default_model, "opus-arn");
        assert!(config.bedrock.as_ref().unwrap().offers("opus-arn"));
        assert!(config.provider_for("openrouter/z-ai/glm-4.6").is_some());
    }

    /// A gateway configured for the tests below, offering one model the block names.
    fn with_a_gateway(models: &str) -> Config {
        let settings = Settings::parse(&format!(
            r#"{{"provider": {{"openrouter": {{
                "options": {{"baseURL": "https://openrouter.example.invalid/api/v1"}},
                "models": {models}
            }}}}}}"#
        ));
        let mut config = Config::from_lookup(complete_env).expect("configured");
        config.providers = settings.providers().to_vec();
        config
    }

    /// The name says which service is meant, which is what lets a gateway offer a model it did not
    /// have to be configured with. Only the part after the id reaches the gateway: the id is this
    /// crate's own filing, and no service has heard of it.
    #[test]
    fn a_name_qualified_by_a_provider_id_names_the_gateway_and_the_model_separately() {
        let config = with_a_gateway("{}");
        let (provider, wire) = config
            .provider_for("openrouter/z-ai/glm-4.6")
            .expect("a gateway");
        assert_eq!(provider.id, "openrouter");
        assert_eq!(wire, "z-ai/glm-4.6");
    }

    /// A name chosen before this crate qualified anything is still sitting in `~/.bravebot/model`, so
    /// a bare name the block lists has to keep reaching the gateway that lists it.
    #[test]
    fn a_bare_name_the_block_lists_still_finds_its_gateway() {
        let config = with_a_gateway(r#"{"z-ai/glm-4.6": {}}"#);
        let (provider, wire) = config.provider_for("z-ai/glm-4.6").expect("a gateway");
        assert_eq!(provider.id, "openrouter");
        assert_eq!(wire, "z-ai/glm-4.6");
    }

    /// The other two rosters share this name space. A Brave slug carries no separator and a Bedrock
    /// ARN's leading segment is no provider's id, so neither is claimed by a gateway that was asked
    /// about a name belonging to somebody else.
    #[test]
    fn a_name_no_gateway_was_configured_for_reaches_no_gateway() {
        let config = with_a_gateway(r#"{"z-ai/glm-4.6": {}}"#);
        for name in [
            "claude-3-sonnet",
            DEFAULT_MODEL,
            "arn:aws:bedrock:us-west-2:1:application-inference-profile/abc",
            "moonshot/kimi-k2",
            "openrouter/",
            "openrouter",
        ] {
            assert!(
                config.provider_for(name).is_none(),
                "{name:?} reached a gateway"
            );
        }
    }

    /// The exception: with no Brave credentials, `automatic` names a backend that cannot be reached at
    /// all, so the strongest configured tier is the only thing that could answer.
    #[test]
    fn without_brave_credentials_the_default_is_the_strongest_bedrock_tier() {
        for (state, endpoint) in [
            ("nothing configured for Brave", None),
            (
                "an endpoint and no credentials",
                Some("https://example.invalid"),
            ),
        ] {
            let config = Config::from_lookup(|k| match k {
                env_var::USE_BEDROCK => Some("1".into()),
                env_var::AWS_REGION => Some("us-west-2".into()),
                env_var::BEDROCK_SONNET_MODEL => Some("sonnet-arn".into()),
                env_var::ENDPOINT => endpoint.map(str::to_string),
                _ => None,
            })
            .expect("configured");
            assert!(!config.serves_aichat(), "{state}");
            assert_eq!(config.default_model, "sonnet-arn", "{state}");
        }

        // The account a `provider` block named is the only reachable service here too.
        let settings = Settings::parse(
            r#"{"provider": {"amazon-bedrock": {
                "options": {"region": "us-west-2"},
                "models": {"openai.gpt-5.6-sol": {}}
            }}}"#,
        );
        let config = Config::from_lookup_with_providers(|_| None, settings.providers().to_vec())
            .expect("configured");
        assert!(!config.serves_aichat());
        assert_eq!(config.default_model, "openai.gpt-5.6-sol");
    }

    /// A budget somebody typed outranks an advertised one: they may know which model an opaque ARN
    /// actually resolves to.
    #[test]
    fn a_budget_set_by_hand_outranks_the_assumed_bedrock_window() {
        let config = Config::from_lookup(|k| match k {
            env_var::USE_BEDROCK => Some("1".into()),
            env_var::AWS_REGION => Some("us-west-2".into()),
            env_var::CONTEXT_BUDGET => Some("4096".into()),
            other => complete_env(other),
        })
        .expect("configured");
        assert_eq!(config.context_budget, 4096);
    }

    /// A variable exported for one session is the most specific thing the person said. A file that
    /// overrode it would make `AWS_PROFILE=other bravebot` do nothing at all.
    #[test]
    fn the_environment_outranks_the_settings_file() {
        let settings = Settings::parse(r#"{"env": {"AWS_REGION": "from-the-file"}}"#);
        let chosen = resolve(env_var::AWS_REGION, Some("from-the-env".into()), |_| None)
            .or_else(|| settings.get(env_var::AWS_REGION).map(str::to_string));
        assert_eq!(chosen.as_deref(), Some("from-the-env"));
    }

    /// The file is consulted when the environment is silent, which is the case it exists for: a
    /// session started from a shell that never exported anything.
    #[test]
    fn the_settings_file_applies_when_the_environment_is_silent() {
        let settings = Settings::parse(r#"{"env": {"AWS_REGION": "from-the-file"}}"#);
        let chosen = resolve(env_var::AWS_REGION, None, |_| None)
            .or_else(|| settings.get(env_var::AWS_REGION).map(str::to_string));
        assert_eq!(chosen.as_deref(), Some("from-the-file"));
    }

    /// The resolution [`Config::from_env_and_settings`] performs, gateways included, over sources a
    /// test names rather than over the process environment, whatever this binary was built with, and
    /// a file only root can write.
    fn resolved(
        settings: &Settings,
        exported: impl Fn(&str) -> Option<String>,
        baked: impl Fn(&str) -> Option<String>,
    ) -> Result<Config, ConfigError> {
        resolved_under(&Managed::default(), settings, exported, baked)
    }

    /// As [`resolved`], with a machine-level layer over it.
    fn resolved_under(
        managed: &Managed,
        settings: &Settings,
        exported: impl Fn(&str) -> Option<String>,
        baked: impl Fn(&str) -> Option<String>,
    ) -> Result<Config, ConfigError> {
        Config::from_lookup_with_providers(
            |key| match managed.get(key) {
                Some(pinned) => Some(pinned.to_string()),
                None => match key {
                    env_var::DEFAULT_MODEL => resolve_model(exported(key), settings, &baked),
                    _ => resolve(key, exported(key), &baked)
                        .or_else(|| settings.get(key).map(str::to_string)),
                },
            },
            match managed.gateways() {
                Some(gateways) => gateways.to_vec(),
                None => settings.providers().to_vec(),
            },
        )
    }

    /// The names a settings file may set are the names something reads, and this is all of them. A
    /// configuration surface described but never named is one nobody can use without reading this
    /// crate, which is how a whole backend reached people undocumented.
    #[test]
    fn every_name_a_settings_block_may_set_reaches_the_configuration() {
        let settings = Settings::parse(
            r#"{"env": {
                "SERVICES_KEY_AICHAT": "signing-key-from-the-file",
                "BRAVE_SERVICES_KEY_ID": "key-id-from-the-file",
                "BRAVE_AI_CHAT_ENDPOINT": "https://endpoint.invalid",
                "BRAVE_AI_CHAT_PREMIUM_ENDPOINT": "https://premium.invalid",
                "BRAVE_AI_CHAT_DEFAULT_MODEL": "model-from-the-file",
                "BRAVEBOT_CONTEXT_BUDGET": "4096",
                "BRAVEBOT_OUTPUT_BUDGET": "48000",
                "BRAVEBOT_USE_BEDROCK": "1",
                "AWS_REGION": "eu-west-1",
                "AWS_PROFILE": "profile-from-the-file",
                "ANTHROPIC_DEFAULT_OPUS_MODEL": "opus-arn",
                "ANTHROPIC_DEFAULT_SONNET_MODEL": "sonnet-arn",
                "ANTHROPIC_DEFAULT_HAIKU_MODEL": "haiku-arn"
            }}"#,
        );
        let config = resolved(&settings, |_| None, |_| None).expect("configured");

        assert_eq!(config.signing_key.expose(), "signing-key-from-the-file");
        assert_eq!(config.key_id, "key-id-from-the-file");
        assert_eq!(config.endpoint, "https://endpoint.invalid");
        assert_eq!(
            config.premium_endpoint.as_deref(),
            Some("https://premium.invalid")
        );
        assert_eq!(config.default_model, "model-from-the-file");
        assert_eq!(config.context_budget, 4096);

        let bedrock = config.bedrock.expect("an aws block");
        assert_eq!(bedrock.region, "eu-west-1");
        assert_eq!(bedrock.profile.as_deref(), Some("profile-from-the-file"));
        assert_eq!(bedrock.output_limit("opus-arn"), 48_000);
        for (tier, named) in [
            (bedrock::Tier::Opus, "opus-arn"),
            (bedrock::Tier::Sonnet, "sonnet-arn"),
            (bedrock::Tier::Haiku, "haiku-arn"),
        ] {
            assert_eq!(bedrock.model_for(tier), Some(named));
        }
    }

    /// A tier has no block to state a ceiling in, so the variable is the only thing that can raise
    /// one for the three models most people reach this backend through. It has to reach both
    /// routes to an AWS account and outrank a figure a file stated, which is the order
    /// BACKEND-35 gives for everything else here.
    #[test]
    fn an_exported_reply_ceiling_outranks_every_stated_one() {
        let settings = Settings::parse(
            r#"{
                "env": {
                    "BRAVEBOT_USE_BEDROCK": "1",
                    "AWS_REGION": "eu-west-1",
                    "ANTHROPIC_DEFAULT_OPUS_MODEL": "opus-arn"
                },
                "provider": {"amazon-bedrock": {
                    "options": {"region": "us-west-2"},
                    "models": {"anthropic.claude-sonnet-4-5": {
                        "limit": {"context": 200000, "output": 64000}
                    }}
                }}
            }"#,
        );

        // Without it, each route answers with what it knows: the block's figure for the model that
        // stated one, the assumed figure for the tier that cannot.
        let config = resolved(&settings, |_| None, |_| None).expect("configured");
        let stated = config
            .bedrock_for("anthropic.claude-sonnet-4-5")
            .expect("the block's account");
        assert_eq!(stated.output_limit("anthropic.claude-sonnet-4-5"), 64_000);
        assert_eq!(
            config
                .bedrock
                .as_ref()
                .expect("the tier account")
                .output_limit("opus-arn"),
            bedrock::OUTPUT_LIMIT
        );

        // With it, both. 48,000 is neither of the two figures above, so a reading that kept either
        // of them is distinguishable from one that took the variable.
        let exported = |name: &str| (name == env_var::OUTPUT_BUDGET).then(|| "48000".to_string());
        let config = resolved(&settings, exported, |_| None).expect("configured");
        assert_eq!(
            config
                .bedrock_for("anthropic.claude-sonnet-4-5")
                .expect("the block's account")
                .output_limit("anthropic.claude-sonnet-4-5"),
            48_000,
            "a stated ceiling outranked the exported one"
        );
        assert_eq!(
            config
                .bedrock
                .as_ref()
                .expect("the tier account")
                .output_limit("opus-arn"),
            48_000,
            "the variable did not reach a tier"
        );
    }

    /// A mistyped budget costs the setting, not the ceiling. Read as zero it would cap every reply
    /// at nothing, which is the failure the setting exists to prevent.
    #[test]
    fn a_reply_ceiling_that_is_not_a_figure_leaves_the_stated_one_standing() {
        let settings = Settings::parse(
            r#"{"env": {"BRAVEBOT_USE_BEDROCK": "1", "AWS_REGION": "eu-west-1",
                "ANTHROPIC_DEFAULT_OPUS_MODEL": "opus-arn"}}"#,
        );
        for typed in ["0", "lots", "", "-1", "64_000"] {
            let exported = |name: &str| (name == env_var::OUTPUT_BUDGET).then(|| typed.to_string());
            let config = resolved(&settings, exported, |_| None).expect("configured");
            assert_eq!(
                config
                    .bedrock
                    .as_ref()
                    .expect("the tier account")
                    .output_limit("opus-arn"),
                bedrock::OUTPUT_LIMIT,
                "{typed:?} was read as a ceiling"
            );
        }
    }

    /// The inversion the machine-level layer exists for. Everything else a person can set, the
    /// environment included, so a pin an exported variable outranked would be a pin an organisation
    /// could not rely on for anything.
    #[test]
    fn a_managed_pin_outranks_an_exported_variable() {
        let managed = managed::scratch(
            "resolve-pin-over-the-environment",
            r#"{"env": {"BRAVE_AI_CHAT_ENDPOINT": "https://approved.example"}}"#,
        );
        let settings = Settings::parse(
            r#"{"env": {"BRAVE_AI_CHAT_ENDPOINT": "https://from-the-file.invalid"}}"#,
        );
        let config = resolved_under(
            &managed,
            &settings,
            |name| match name {
                env_var::ENDPOINT => Some("https://exported.invalid".into()),
                _ => None,
            },
            |name| match name {
                env_var::ENDPOINT => Some("https://baked.invalid".into()),
                other => complete_env(other),
            },
        )
        .expect("configured");
        assert_eq!(config.endpoint, "https://approved.example");
    }

    /// A name it did not pin is resolved exactly as it would be with no such file, so deploying one
    /// to pin a host does not quietly take over the rest of somebody's configuration.
    #[test]
    fn a_name_the_managed_layer_did_not_pin_resolves_as_it_would_have() {
        let managed = managed::scratch(
            "resolve-pin-leaves-other-names",
            r#"{"env": {"BRAVE_AI_CHAT_ENDPOINT": "https://approved.example"}}"#,
        );
        let settings = Settings::parse(r#"{"env": {"AWS_REGION": "from-the-file"}}"#);
        let config = resolved_under(
            &managed,
            &settings,
            |name| match name {
                env_var::USE_BEDROCK => Some("1".into()),
                env_var::AWS_PROFILE => Some("from-the-environment".into()),
                _ => None,
            },
            complete_env,
        )
        .expect("configured");
        let bedrock = config.bedrock.expect("an aws block");
        assert_eq!(bedrock.region, "from-the-file");
        assert_eq!(bedrock.profile.as_deref(), Some("from-the-environment"));
    }

    /// The switch is one of the pinned names, so the exported one stops deciding.
    #[test]
    fn a_pinned_switch_outranks_the_exported_one() {
        let managed = managed::scratch(
            "resolve-refuse-bedrock",
            r#"{"env": {"BRAVEBOT_USE_BEDROCK": "0"}}"#,
        );
        let config = resolved_under(
            &managed,
            &Settings::default(),
            |name| match name {
                env_var::USE_BEDROCK => Some("1".into()),
                env_var::AWS_REGION => Some("us-west-2".into()),
                _ => None,
            },
            complete_env,
        )
        .expect("configured");
        assert!(
            config.bedrock.is_none(),
            "the pinned switch decides, not the exported one"
        );
    }

    /// "Our account or nothing" takes both halves. A gateway entry can name an AWS account as well,
    /// so pinning the switch off while leaving the gateways alone refuses nothing.
    #[test]
    fn refusing_a_personal_cloud_account_takes_the_switch_and_the_gateways() {
        let mine = Settings::parse(
            r#"{"provider": {"amazon-bedrock": {"options": {"region": "us-west-2", "profile": "mine"}}}}"#,
        );

        let switch_alone = managed::scratch(
            "resolve-switch-alone",
            r#"{"env": {"BRAVEBOT_USE_BEDROCK": "0"}}"#,
        );
        let config =
            resolved_under(&switch_alone, &mine, |_| None, complete_env).expect("configured");
        assert!(
            config
                .bedrock_providers()
                .any(|(_, bedrock)| bedrock.profile.as_deref() == Some("mine")),
            "the switch alone leaves a gateway naming an account reachable"
        );

        let both = managed::scratch(
            "resolve-switch-and-gateways",
            r#"{"env": {"BRAVEBOT_USE_BEDROCK": "0"}, "provider": {}}"#,
        );
        let config = resolved_under(&both, &mine, |_| None, complete_env).expect("configured");
        assert!(config.bedrock.is_none());
        assert_eq!(config.bedrock_providers().count(), 0);
    }

    /// An approved endpoint pins nothing while a gateway somebody configured is still reachable, so
    /// the managed block replaces the person's rather than adding to it.
    #[test]
    fn a_managed_gateway_block_replaces_the_one_in_the_settings() {
        let managed = managed::scratch(
            "resolve-gateways-replaced",
            r#"{"provider": {"approved": {"options": {"baseURL": "https://approved.example/v1"}}}}"#,
        );
        let settings = Settings::parse(
            r#"{"provider": {"mine": {"options": {"baseURL": "https://mine.invalid/v1"}}}}"#,
        );
        let config =
            resolved_under(&managed, &settings, |_| None, complete_env).expect("configured");
        let ids: Vec<&str> = config.providers.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids, vec!["approved"]);
    }

    /// The only way to say there are to be no gateways at all, which is half of what pinning a
    /// destination means.
    #[test]
    fn an_empty_managed_gateway_block_leaves_no_gateways() {
        let managed = managed::scratch("resolve-gateways-none", r#"{"provider": {}}"#);
        let settings = Settings::parse(
            r#"{"provider": {"mine": {"options": {"baseURL": "https://mine.invalid/v1"}}}}"#,
        );
        let config =
            resolved_under(&managed, &settings, |_| None, complete_env).expect("configured");
        assert!(config.providers.is_empty());
    }

    /// A file that says nothing about gateways must leave them alone, or every managed layer would
    /// silently take away a gateway it never mentioned.
    #[test]
    fn a_managed_layer_silent_on_gateways_keeps_the_configured_ones() {
        let managed = managed::scratch(
            "resolve-gateways-untouched",
            r#"{"env": {"BRAVE_AI_CHAT_ENDPOINT": "https://approved.example"}}"#,
        );
        let settings = Settings::parse(
            r#"{"provider": {"mine": {"options": {"baseURL": "https://mine.invalid/v1"}}}}"#,
        );
        let config =
            resolved_under(&managed, &settings, |_| None, complete_env).expect("configured");
        let ids: Vec<&str> = config.providers.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids, vec!["mine"]);
    }

    /// Every name but the top-level `model` key ranks below what the build baked in, so a released
    /// binary reaches the host it was built for rather than one a file in a checkout named.
    #[test]
    fn a_baked_in_value_outranks_the_settings_file() {
        let settings = Settings::parse(
            r#"{"env": {"BRAVE_AI_CHAT_ENDPOINT": "https://from-the-file.invalid"}}"#,
        );
        let config = resolved(
            &settings,
            |_| None,
            |name| match name {
                env_var::ENDPOINT => Some("https://baked.invalid".into()),
                other => complete_env(other),
            },
        )
        .expect("configured");
        assert_eq!(config.endpoint, "https://baked.invalid");
    }

    /// A name nothing reads is kept so that a file written for another tool, or for a later
    /// version, does not stop a session. Kept is the whole of it: the switch that hands this
    /// agent's credentials back to a subprocess is read from the environment alone, and a file
    /// spelling it changes nothing. Compared whole, so a field added later is covered too.
    #[test]
    fn a_name_nothing_consults_changes_nothing() {
        let named = Settings::parse(
            r#"{"env": {"BRAVEBOT_SUBPROCESS_ENV_SCRUB": "0", "SOMETHING_ELSE": "a value"}}"#,
        );
        assert_eq!(named.get(env_var::SUBPROCESS_ENV_SCRUB), Some("0"));

        let with_the_names = resolved(&named, |_| None, complete_env).expect("configured");
        let without = resolved(&Settings::default(), |_| None, complete_env).expect("configured");
        assert_eq!(
            with_the_names.signing_key.expose(),
            without.signing_key.expose()
        );
        assert_eq!(format!("{with_the_names:?}"), format!("{without:?}"));
    }

    /// The point of reading the key: a file naming a model is what decides, without the variable
    /// being exported anywhere.
    #[test]
    fn a_model_in_the_settings_file_becomes_the_default() {
        let settings = Settings::parse(r#"{"model": "llama-3-8b-instruct"}"#);
        let config = Config::from_lookup(|key| match key {
            env_var::DEFAULT_MODEL => resolve_model(None, &settings, |_| None),
            other => complete_env(other),
        })
        .unwrap();
        assert_eq!(config.default_model, "llama-3-8b-instruct");
    }

    /// The whole reason this value is ranked differently. Every release bakes a default model in, so
    /// a `model` key ranked below it would lose on every binary anybody was given: the key would
    /// parse, `doctor` would report it, and nothing outside a source build would change.
    #[test]
    fn a_model_in_the_settings_file_outranks_the_baked_in_one() {
        let settings = Settings::parse(r#"{"model": "opus"}"#);
        let chosen = resolve_model(None, &settings, |_| Some("automatic".into()));
        assert_eq!(chosen.as_deref(), Some("opus"));
    }

    /// The variable names a default, and a `.envrc` exports it for every checkout. Ranked above the
    /// file it would outrank every `model` key and every saved `/model` pick on any machine that
    /// sources one. Where no file names a model, the variable still decides over the build.
    #[test]
    fn a_model_in_the_settings_file_outranks_an_exported_one() {
        let settings = Settings::parse(r#"{"model": "opus"}"#);
        let chosen = resolve_model(Some("from-the-env".into()), &settings, |_| None);
        assert_eq!(chosen.as_deref(), Some("opus"));

        let chosen = resolve_model(Some("from-the-env".into()), &Settings::default(), |_| {
            Some("baked".into())
        });
        assert_eq!(chosen.as_deref(), Some("from-the-env"));
    }

    /// A blank variable is a placeholder rather than an instruction to discard what the build said.
    #[test]
    fn a_blank_exported_model_does_not_shadow_the_baked_in_one() {
        let chosen = resolve_model(Some("   ".into()), &Settings::default(), |_| {
            Some("baked".into())
        });
        assert_eq!(chosen.as_deref(), Some("baked"));
    }

    /// The `env` block is variables, and is ranked like them: below what the build baked in. Only
    /// the top-level key is promoted above it.
    #[test]
    fn the_env_block_spelling_stays_below_the_baked_in_value() {
        let settings =
            Settings::parse(r#"{"env": {"BRAVE_AI_CHAT_DEFAULT_MODEL": "from-the-env-block"}}"#);
        let chosen = resolve_model(None, &settings, |_| Some("baked".into()));
        assert_eq!(chosen.as_deref(), Some("baked"));

        // And is still read where the build baked nothing in, so the spelling keeps working.
        let chosen = resolve_model(None, &settings, |_| None);
        assert_eq!(chosen.as_deref(), Some("from-the-env-block"));
    }

    /// With nothing said anywhere the default stands, which is what a fresh machine has.
    #[test]
    fn no_model_named_anywhere_leaves_the_default() {
        let chosen = resolve_model(None, &Settings::default(), |_| None);
        assert_eq!(chosen, None);
    }

    /// Where those three words come from they name a tier, and the tier's model is what the same
    /// file already said. Left as the word, the request reaches a backend that has never heard of
    /// it: Bedrock refuses an unknown model rather than substituting one.
    #[test]
    fn a_tier_alias_resolves_to_the_model_that_tier_names() {
        for (alias, expected) in [
            ("opus", "opus-arn"),
            ("sonnet", "sonnet-arn"),
            ("haiku", "haiku-arn"),
            // A hand-written file says `Opus` as readily as `opus`.
            ("Opus", "opus-arn"),
        ] {
            let config = Config::from_lookup(|key| match key {
                env_var::USE_BEDROCK => Some("1".into()),
                env_var::AWS_REGION => Some("us-west-2".into()),
                env_var::BEDROCK_OPUS_MODEL => Some("opus-arn".into()),
                env_var::BEDROCK_SONNET_MODEL => Some("sonnet-arn".into()),
                env_var::BEDROCK_HAIKU_MODEL => Some("haiku-arn".into()),
                env_var::DEFAULT_MODEL => Some(alias.into()),
                other => complete_env(other),
            })
            .unwrap();
            assert_eq!(config.default_model, expected, "{alias} did not resolve");
        }
    }

    /// Everything that is not one of the three words is used as written, which is what carries an
    /// inference-profile ARN and a name from the Brave roster alike.
    #[test]
    fn a_model_that_is_not_a_tier_alias_is_used_as_written() {
        for name in [
            "arn:aws:bedrock:us-west-2:1:application-inference-profile/abc",
            "claude-opus-4-8",
            "llama-3-8b-instruct",
        ] {
            let config = Config::from_lookup(|key| match key {
                env_var::USE_BEDROCK => Some("1".into()),
                env_var::AWS_REGION => Some("us-west-2".into()),
                env_var::BEDROCK_OPUS_MODEL => Some("opus-arn".into()),
                env_var::DEFAULT_MODEL => Some(name.into()),
                other => complete_env(other),
            })
            .unwrap();
            assert_eq!(config.default_model, name);
        }
    }

    /// Every route to a model resolves a name the same way, so a flag, a remembered choice and the
    /// settings key cannot accept different spellings of the same model.
    #[test]
    fn a_name_from_anywhere_resolves_as_the_settings_key_does() {
        let config = Config::from_lookup(|key| match key {
            env_var::USE_BEDROCK => Some("1".into()),
            env_var::AWS_REGION => Some("us-west-2".into()),
            env_var::BEDROCK_OPUS_MODEL => Some("opus-arn".into()),
            other => complete_env(other),
        })
        .unwrap();

        assert_eq!(config.model_named("opus"), "opus-arn");
        assert_eq!(
            config.model_named("sonnet"),
            bedrock::Tier::Sonnet.brave_model()
        );
        assert_eq!(config.model_named("automatic"), DEFAULT_MODEL);
        assert_eq!(
            config.model_named("llama-3-8b-instruct"),
            "llama-3-8b-instruct"
        );
    }

    /// Leo's automatic routing name is rewritten to bravebot's own entry.
    #[test]
    fn the_legacy_automatic_name_becomes_the_brave_bot_default() {
        let config = Config::from_lookup(|key| match key {
            env_var::DEFAULT_MODEL => Some("automatic".into()),
            other => complete_env(other),
        })
        .unwrap();
        assert_eq!(config.default_model, DEFAULT_MODEL);
    }

    /// An ARN cannot be derived from a word, so a tier the AWS account left unset falls through to
    /// the roster every build can reach rather than being guessed at.
    #[test]
    fn an_alias_for_an_unconfigured_tier_falls_through_to_brave() {
        let config = Config::from_lookup(|key| match key {
            env_var::USE_BEDROCK => Some("1".into()),
            env_var::AWS_REGION => Some("us-west-2".into()),
            env_var::BEDROCK_OPUS_MODEL => Some("opus-arn".into()),
            env_var::DEFAULT_MODEL => Some("haiku".into()),
            other => complete_env(other),
        })
        .unwrap();
        assert_eq!(config.default_model, bedrock::Tier::Haiku.brave_model());
    }

    /// The case a settings file written for another tool actually lands in: no AWS account, so the
    /// word has to name something on the roster every build can reach. Left as the bare word it
    /// reaches an endpoint that has never heard of it and is silently reset, which is the key
    /// appearing to work and doing nothing.
    #[test]
    fn a_tier_alias_without_bedrock_resolves_against_the_brave_roster() {
        for (alias, tier) in [
            ("opus", bedrock::Tier::Opus),
            ("sonnet", bedrock::Tier::Sonnet),
            ("haiku", bedrock::Tier::Haiku),
        ] {
            let config = Config::from_lookup(|key| match key {
                env_var::DEFAULT_MODEL => Some(alias.into()),
                other => complete_env(other),
            })
            .unwrap();
            assert_eq!(config.default_model, tier.brave_model());
            // Never left as the bare word, which is the thing that silently did nothing.
            assert_ne!(config.default_model, alias);
        }
    }

    /// With no Brave credentials a Brave name reaches a service this build cannot sign for, so an
    /// unset tier resolves to the strongest one the AWS account did name instead.
    ///
    /// An endpoint is not a credential, and a Bedrock block makes each of the three fields optional
    /// on its own, so a source build can hold a URL and no key to sign with. Deciding this from the
    /// endpoint alone sends the word to the Brave roster on exactly the build that cannot sign for
    /// it.
    #[test]
    fn without_brave_credentials_an_unconfigured_tier_stays_on_aws() {
        for (state, endpoint) in [
            ("nothing configured for Brave", None),
            (
                "an endpoint and no credentials",
                Some("https://example.invalid"),
            ),
        ] {
            let config = Config::from_lookup(|key| match key {
                env_var::USE_BEDROCK => Some("1".into()),
                env_var::AWS_REGION => Some("us-west-2".into()),
                env_var::BEDROCK_OPUS_MODEL => Some("opus-arn".into()),
                env_var::DEFAULT_MODEL => Some("haiku".into()),
                env_var::ENDPOINT => endpoint.map(str::to_string),
                _ => None,
            })
            .unwrap();
            assert!(!config.serves_aichat(), "{state}");
            assert_eq!(config.default_model, "opus-arn", "{state}");
            assert_eq!(config.model_named("haiku"), "opus-arn", "{state}");
        }
    }

    /// The AWS account a `provider` block named answers a tier word as readily as the one the tier
    /// variables named. Consulting only the tier variables resolves the word to a Brave name on a
    /// build whose only reachable service is that account.
    #[test]
    fn a_tier_word_resolves_against_an_aws_account_a_provider_block_named() {
        const ARN: &str = "arn:aws:bedrock:us-west-2:1:application-inference-profile/abc";
        let settings = Settings::parse(&format!(
            r#"{{"model": "opus", "provider": {{"amazon-bedrock": {{
                "options": {{"region": "us-west-2"}},
                "models": {{"{ARN}": {{"name": "Sonnet (Bedrock)"}}}}
            }}}}}}"#
        ));
        let config = Config::from_lookup_with_providers(
            |key| match key {
                env_var::DEFAULT_MODEL => settings.model().map(str::to_string),
                _ => None,
            },
            settings.providers().to_vec(),
        )
        .expect("configured");

        assert!(!config.serves_aichat());
        assert_eq!(config.default_model, ARN);
        assert_ne!(config.default_model, bedrock::Tier::Opus.brave_model());
        // Every other route to a model resolves the word the same way.
        assert_eq!(config.model_named("sonnet"), ARN);
    }

    /// What a block with several models guarantees is that the word resolves to one of them, since
    /// entries a block named are model ids with no tier to rank them by. Naming a model the account
    /// does not offer is a request Bedrock refuses on the name.
    #[test]
    fn a_tier_word_resolves_to_a_model_the_block_actually_offers() {
        let settings = Settings::parse(
            r#"{"provider": {"amazon-bedrock": {
                "options": {"region": "us-west-2"},
                "models": {"zzz-opus": {}, "aaa-haiku": {}}
            }}}"#,
        );
        let config = Config::from_lookup_with_providers(|_| None, settings.providers().to_vec())
            .expect("configured");

        assert!(!config.serves_aichat());
        for word in ["opus", "sonnet", "haiku"] {
            let resolved = config.model_named(word);
            assert!(
                config.bedrock_for(&resolved).is_some(),
                "{word} resolved to {resolved}, which no configured account offers"
            );
        }
    }

    /// A tier the AWS account left unset still falls through to Brave where this build can sign for
    /// it, so asking the account first does not take the roster every credentialled build has.
    #[test]
    fn with_brave_credentials_a_tier_word_still_falls_through_to_brave() {
        let settings = Settings::parse(
            r#"{"provider": {"amazon-bedrock": {
                "options": {"region": "us-west-2"},
                "models": {"openai.gpt-5.6-sol": {}}
            }}}"#,
        );
        let config =
            Config::from_lookup_with_providers(complete_env, settings.providers().to_vec())
                .expect("configured");

        assert!(config.serves_aichat());
        assert_eq!(
            config.model_named("haiku"),
            bedrock::Tier::Haiku.brave_model()
        );
        assert_eq!(config.default_model, DEFAULT_MODEL);
    }

    #[test]
    fn a_missing_signing_key_is_an_error() {
        let err = Config::from_lookup(|k| match k {
            env_var::SIGNING_KEY => None,
            other => complete_env(other),
        })
        .unwrap_err();
        assert_eq!(err, ConfigError::Missing(env_var::SIGNING_KEY));
    }

    /// A placeholder .envrc leaves variables present but blank, which must not be
    /// mistaken for a real credential.
    #[test]
    fn an_empty_signing_key_is_an_error() {
        let err = Config::from_lookup(|k| match k {
            env_var::SIGNING_KEY => Some(String::new()),
            other => complete_env(other),
        })
        .unwrap_err();
        assert_eq!(err, ConfigError::Empty(env_var::SIGNING_KEY));
    }

    #[test]
    fn a_missing_key_id_is_an_error() {
        let err = Config::from_lookup(|k| match k {
            env_var::KEY_ID => None,
            other => complete_env(other),
        })
        .unwrap_err();
        assert_eq!(err, ConfigError::Missing(env_var::KEY_ID));
    }

    #[test]
    fn an_endpoint_without_a_scheme_is_rejected() {
        let err = Config::from_lookup(|k| match k {
            env_var::ENDPOINT => Some("ai-chat.example.invalid".into()),
            other => complete_env(other),
        })
        .unwrap_err();
        assert!(matches!(err, ConfigError::InvalidEndpoint { .. }));
    }

    #[test]
    fn a_trailing_slash_does_not_double_up_in_the_url() {
        let config = Config::from_lookup(|k| match k {
            env_var::ENDPOINT => Some("https://example.invalid/".into()),
            other => complete_env(other),
        })
        .unwrap();
        assert_eq!(
            config.chat_completions_url(),
            "https://example.invalid/v1/chat/completions"
        );
    }

    #[test]
    fn builds_the_openai_compatible_url() {
        let config = Config::from_lookup(complete_env).unwrap();
        assert_eq!(
            config.chat_completions_url(),
            "https://example.invalid/v1/chat/completions"
        );
    }

    /// A local development endpoint over plain http is allowed.
    #[test]
    fn http_endpoints_are_allowed_for_local_development() {
        let config = Config::from_lookup(|k| match k {
            env_var::ENDPOINT => Some("http://127.0.0.1:8000".into()),
            other => complete_env(other),
        })
        .unwrap();
        assert_eq!(
            config.chat_completions_url(),
            "http://127.0.0.1:8000/v1/chat/completions"
        );
    }

    /// The premium tier is a separate deployment, so its URL must come from its own variable
    /// rather than being derived from the free host.
    #[test]
    fn the_premium_endpoint_builds_its_own_url() {
        let config = Config::from_lookup(|k| match k {
            env_var::PREMIUM_ENDPOINT => Some("https://premium.invalid".into()),
            other => complete_env(other),
        })
        .unwrap();
        assert_eq!(
            config.premium_chat_completions_url().as_deref(),
            Some("https://premium.invalid/v1/chat/completions")
        );
    }

    /// The listing lives on the free host and reports which of its entries are premium, so asking
    /// the premium host would spend a subscription credential for the same answer.
    #[test]
    fn the_model_listing_url_is_on_the_free_host() {
        let config = Config::from_lookup(|k| match k {
            env_var::PREMIUM_ENDPOINT => Some("https://premium.invalid".into()),
            other => complete_env(other),
        })
        .unwrap();
        assert_eq!(config.models_url(), "https://example.invalid/v1/models");
    }

    /// A build without the premium host must report premium as unavailable rather than quietly
    /// using the free endpoint, which would send a subscription credential to the wrong host.
    #[test]
    fn a_build_without_a_premium_endpoint_has_no_premium_url() {
        let config = Config::from_lookup(complete_env).unwrap();
        assert_eq!(config.premium_endpoint, None);
        assert_eq!(config.premium_chat_completions_url(), None);
    }

    /// A blank or schemeless premium host is discarded, not turned into a request to a host with
    /// no scheme.
    #[test]
    fn a_malformed_premium_endpoint_is_discarded() {
        for value in ["", "   ", "premium.invalid"] {
            let config = Config::from_lookup(|k| match k {
                env_var::PREMIUM_ENDPOINT => Some(value.into()),
                other => complete_env(other),
            })
            .unwrap();
            assert_eq!(config.premium_endpoint, None, "accepted '{value}'");
        }
    }

    #[test]
    fn a_trailing_slash_on_the_premium_endpoint_does_not_double_up() {
        let config = Config::from_lookup(|k| match k {
            env_var::PREMIUM_ENDPOINT => Some("https://premium.invalid/".into()),
            other => complete_env(other),
        })
        .unwrap();
        assert_eq!(
            config.premium_chat_completions_url().as_deref(),
            Some("https://premium.invalid/v1/chat/completions")
        );
    }

    #[test]
    fn secrets_are_redacted_in_debug_and_display() {
        let secret = Secret::new("live-credential");
        assert_eq!(format!("{secret:?}"), "Secret(<redacted>)");
        assert_eq!(format!("{secret}"), "<redacted>");
        assert!(!format!("{secret:?}").contains("live-credential"));
    }

    /// A credential is handed over in a `String` the caller made, and copying it into locked pages
    /// protects nothing while that string goes back to the allocator holding it. The value held
    /// has to read back as what was handed over, so that a hold which cleared the string first is
    /// not taken for one that cleared it after.
    #[test]
    fn the_string_a_secret_is_made_from_is_overwritten_once_it_is_held() {
        let mut value = String::from("sk-live-0123456789abcdef");
        let length = value.len();

        let held = hold(&mut value);

        assert_eq!(held.as_str(), "sk-live-0123456789abcdef");
        assert_eq!(
            value.as_bytes(),
            vec![0u8; length],
            "the string the credential arrived in still holds it"
        );
    }

    /// The credential has to be gone from the allocation, not just from the length.
    ///
    /// Both halves matter and neither alone says it. A buffer of zeros at a fresh address leaves
    /// the original bytes where the allocator can hand them on, and an unmoved buffer still
    /// holding the value is what `String::clear` produces: the length reads zero and every byte
    /// is still there.
    #[test]
    fn scrubbing_overwrites_the_bytes_where_they_lie() {
        let mut value = String::from("sk-live-0123456789abcdef");
        let length = value.len();
        let address = value.as_ptr();

        scrub(&mut value);

        assert_eq!(
            value.as_ptr(),
            address,
            "the buffer moved, so the credential is still in the one that was left behind"
        );
        assert_eq!(
            value.as_bytes(),
            vec![0u8; length],
            "the buffer the credential was in still holds bytes of it"
        );
    }

    /// A token is where whoever wrote the settings file put it, which is inside two blocks rather
    /// than at the root of the document.
    ///
    /// The value has to stay a string of the same length for the bytes to have been written over:
    /// a name set to null, or to a shorter string, is a name whose buffer was dropped instead, and
    /// dropping it is what leaves the token for the allocator.
    #[test]
    fn scrubbing_a_document_reaches_a_token_inside_the_blocks_it_was_written_in() {
        let token = "sk-live-0123456789abcdef";
        let mut root = match serde_json::from_str(
            &serde_json::json!({
                "provider": {"gw": {"options": {"apiKey": token}, "models": [token]}}
            })
            .to_string(),
        ) {
            Ok(serde_json::Value::Object(root)) => root,
            _ => panic!("the fixture is not a document"),
        };

        scrub_document(&mut root);

        let gateway = &root["provider"]["gw"];
        assert_eq!(
            gateway["options"]["apiKey"].as_str().expect("a string"),
            "\0".repeat(token.len()),
            "the token is still in the buffer the parse put it in"
        );
        assert_eq!(
            gateway["models"][0].as_str().expect("a string"),
            "\0".repeat(token.len()),
            "a value inside a list was not reached, and a list is one of the shapes a file states"
        );
    }

    /// Bytes rather than characters, because a value that is not ASCII has more of the first
    /// than the second and the tail of it is what a count of characters would leave behind.
    ///
    /// A passphrase is where this arrives: nothing stops one holding a character that takes
    /// three bytes, and a scrub measured in characters would write one zero for it and leave
    /// the other two readable.
    #[test]
    fn scrubbing_counts_the_bytes_rather_than_the_characters() {
        let mut value = String::from("pass-phrase-\u{4e16}\u{754c}");
        let length = value.len();
        assert!(length > value.chars().count(), "the fixture is not ASCII");

        scrub(&mut value);

        assert_eq!(
            value.len(),
            length,
            "the buffer is as long as the value was"
        );
        assert!(
            value.bytes().all(|byte| byte == 0),
            "a byte of the value survived the scrub"
        );
    }

    /// An explicit variable must win, so a released binary can be pointed at a local
    /// backend without rebuilding it.
    #[test]
    fn the_environment_overrides_a_built_in_value() {
        let chosen = resolve(
            env_var::ENDPOINT,
            Some("http://127.0.0.1:8000".into()),
            |_| Some("https://baked.invalid".into()),
        );
        assert_eq!(chosen.as_deref(), Some("http://127.0.0.1:8000"));
    }

    /// The point of baking values in: a binary started outside the source tree has no
    /// direnv and must still be configured.
    #[test]
    fn a_built_in_value_applies_when_the_environment_is_silent() {
        let chosen = resolve(env_var::ENDPOINT, None, |_| {
            Some("https://baked.invalid".into())
        });
        assert_eq!(chosen.as_deref(), Some("https://baked.invalid"));
    }

    /// A blank variable is a placeholder, not an instruction to discard the built-in
    /// value, so it must not shadow one.
    #[test]
    fn a_blank_variable_does_not_shadow_a_built_in_value() {
        let chosen = resolve(env_var::ENDPOINT, Some("   ".into()), |_| {
            Some("https://baked.invalid".into())
        });
        assert_eq!(chosen.as_deref(), Some("https://baked.invalid"));
    }

    /// With nothing baked in, a blank must still be reported as empty rather than
    /// missing, since the two suggest different fixes.
    #[test]
    fn a_blank_variable_survives_when_nothing_was_built_in() {
        let err = Config::from_lookup(|k| match k {
            env_var::SIGNING_KEY => resolve(k, Some(String::new()), |_| None),
            other => complete_env(other),
        })
        .unwrap_err();
        assert_eq!(err, ConfigError::Empty(env_var::SIGNING_KEY));
    }

    /// Masking is what keeps a baked credential out of `strings` output, and it is
    /// only usable at all if it round-trips.
    #[test]
    fn masking_round_trips_and_does_not_leave_the_value_readable() {
        let secret = "live-credential-abc123";
        let masked = obfuscate::mask(secret.as_bytes());
        assert_ne!(masked, secret.as_bytes());
        assert_eq!(obfuscate::mask(&masked), secret.as_bytes());
    }

    /// Two values sharing a prefix must not produce a shared masked prefix, or the
    /// masking would advertise the relationship.
    #[test]
    fn values_with_a_shared_prefix_do_not_share_a_masked_prefix() {
        let one = obfuscate::mask(b"prefix-aaaa");
        let two = obfuscate::mask(b"prefix-bbbbbb");
        assert_ne!(one[0], two[0]);
    }

    /// Debug-printing the whole config is the obvious debugging reflex, so it must
    /// not leak the key.
    #[test]
    fn debugging_the_config_does_not_leak_the_key() {
        let config = Config::from_lookup(complete_env).unwrap();
        let shown = format!("{config:?}");
        assert!(!shown.contains("test-signing-key"), "leaked: {shown}");
        assert!(shown.contains("redacted"));
    }
}
