//! Finding a credential in text, and describing it without repeating it.
//!
//! Two layers run over the text, in the order a reader would try them. The first recognises the
//! providers whose keys have a shape: a fixed prefix, a length, and an alphabet, which together
//! say what a value is rather than guessing. The second reads an assignment whose *name* says the
//! value is a secret, and asks whether the value is rare enough to be one, which is how a
//! generated framework key is caught when no provider stamped a prefix on it.
//!
//! Both run here in the process. Nothing is sent anywhere to be classified and no value is tried
//! against the service that issued it: asking a hosted model whether bytes are a credential
//! discloses the credential to that model, and exercising a key is a use of it.
//!
//! # What a finding is allowed to hold
//!
//! The kind, where it is, a fingerprint salted per run, and a preview with every character
//! masked. Never the value, and never a piece of one. A finding that quoted what it found would
//! be a second copy of the credential, written by the thing that was checking for copies, and a
//! collection of them is a map of every secret in the tree.
//!
//! **The preview is a description rather than a sample.** The familiar form, a few characters of
//! each end with stars in the middle, is a prefix and a suffix of the value, which is most of what
//! an attacker needs to recognise one they already hold and a fair start on guessing the rest. An
//! interior sample is no better: it is still the value, in pieces. What is left that still tells
//! one finding from another is how long the value is and which classes of character are in it.
//!
//! # What it cannot do
//!
//! It finds what a layer recognises. A credential in a format nobody has written a rule for is
//! not found, and a clean result means nothing was matched rather than that nothing is there.

use std::hash::{BuildHasher, Hasher};
use std::sync::OnceLock;

/// What a layer recognised, which is as much as is ever said about a value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// An AWS access key id, by its prefix and its fixed length.
    AwsAccessKey,
    /// A GitHub token, personal, installation or refresh.
    GitHubToken,
    /// A Slack token of any of its five audiences.
    SlackToken,
    /// A Google API key, by its prefix and its fixed length.
    GoogleApiKey,
    /// A live Stripe key, secret or restricted.
    StripeKey,
    /// An Anthropic API key.
    AnthropicKey,
    /// The body of a private key in PEM armour.
    PrivateKey,
    /// The password in a connection string, which the URL's own syntax says is one.
    UrlPassword,
    /// A value assigned to a name that says it is a secret, rare enough to be one.
    Assigned,
    /// A rare value standing as the whole of a file, with nothing beside it saying what it is.
    Standalone,
}

impl Kind {
    /// What this kind is called in a line a person reads.
    fn describe(self) -> &'static str {
        match self {
            Kind::AwsAccessKey => "an AWS access key id",
            Kind::GitHubToken => "a GitHub token",
            Kind::SlackToken => "a Slack token",
            Kind::GoogleApiKey => "a Google API key",
            Kind::StripeKey => "a live Stripe key",
            Kind::AnthropicKey => "an Anthropic API key",
            Kind::PrivateKey => "a private key",
            Kind::UrlPassword => "a password in a connection string",
            Kind::Assigned => "a secret assigned by name",
            Kind::Standalone => "a secret standing as a file's whole contents",
        }
    }

    /// Whether the value said what it was, or whether a layer inferred it.
    ///
    /// A shape is a provider's own prefix over its own alphabet at its own length, and a URL names
    /// its password field in its syntax: those values declare themselves, and a match is not a
    /// judgement anybody needs to review. [`Kind::Assigned`] is the other thing, a name that
    /// sounds like a secret beside a value that looks rare, and it is right often enough to be
    /// worth running and wrong often enough that refusing on it alone would refuse a k8s manifest,
    /// a local development password and a test fixture, none of which anybody should have to argue
    /// with a scanner about.
    ///
    /// [`Kind::Standalone`] is inferred for the same reason and reads as weaker still: nothing on
    /// the line says the value is a secret, only that the file holds one value and nothing else.
    /// A commit id, an identifier and a digest are written that way too, and a turn refused
    /// outright for one of those would have no way to say otherwise.
    ///
    /// The distinction exists because the two deserve different answers, not because one matters
    /// less: see [`Scanned::refused`] and [`Scanned::to_approve`].
    pub fn is_declared(self) -> bool {
        !matches!(self, Kind::Assigned | Kind::Standalone)
    }
}

/// One credential, recorded so that the record is not itself a credential.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Which layer recognised it, and as what.
    pub kind: Kind,
    /// The file it is in, as the person watching sees the path written.
    pub path: String,
    /// The line it is on, counted from one.
    pub line: usize,
    /// The same value twice gives the same fingerprint, and no value gives the value back.
    ///
    /// Salted per run, so it identifies a value within a run and says nothing outside one.
    pub fingerprint: String,
    /// How long the value is and what it is made of. Every character is masked.
    pub preview: String,
}

impl Finding {
    /// The sentence a person is shown, which holds nothing of the value.
    pub fn describe(&self) -> String {
        format!(
            "{} at {}:{} ({}, {})",
            self.kind.describe(),
            self.path,
            self.line,
            self.preview,
            self.fingerprint
        )
    }
}

/// What a scan of a change found, split by who put it there.
///
/// Two lists rather than a flag on each finding: they are answered differently, and a caller that
/// had to filter one list would be a caller that could forget to.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Scanned {
    /// Values this change would add, which nothing in the file held before it.
    pub authored: Vec<Finding>,
    /// Values the file already held at this path, which the change carries along.
    pub carried: Vec<Finding>,
    /// Whether what the write would leave is one value and nothing else.
    ///
    /// The shape creating a credential takes, because there the file *is* the secret rather than
    /// a document mentioning one. It decides what a turn is told rather than what is found: a
    /// file holding only a key has nowhere for a reference to go, so telling a planner to write
    /// one into it is advice that cannot be followed.
    ///
    /// Read off the body rather than off the findings, so it is the same answer whichever layer
    /// named the value.
    pub only_the_value: bool,
}

impl Scanned {
    /// The findings a write is refused for outright, with nobody asked.
    ///
    /// A value that declared itself: a provider's prefix over its own alphabet, or a password in
    /// the field a URL reserves for one. There is no judgement to put to anybody, and a prompt
    /// that can be answered "write it anyway" is a prompt a turn will eventually get past.
    pub fn refused(&self) -> Vec<&Finding> {
        self.authored
            .iter()
            .filter(|finding| finding.kind.is_declared())
            .collect()
    }

    /// The findings a person decides about, shown on the diff they are already approving.
    ///
    /// A guess from a name and an entropy score. Refusing on one of these alone stops a turn
    /// writing a Kubernetes manifest, a local development password or a test fixture, with no way
    /// to say otherwise; see [`Kind::is_declared`]. The person is standing in the write-approval
    /// path anyway, and this is a judgement rather than a rule, so it goes to them.
    pub fn to_approve(&self) -> Vec<&Finding> {
        self.authored
            .iter()
            .filter(|finding| !finding.kind.is_declared())
            .collect()
    }
}

/// The salt every fingerprint in this run is taken under.
///
/// Drawn once per process and kept nowhere, so a fingerprint means something while the run lasts
/// and nothing afterwards. A salt that persisted would let two runs be compared, which is what an
/// allowlist of accepted findings needs and is a thing to build with the store that holds one:
/// until then, a value that outlives the process is a value somebody has to keep somewhere.
pub fn run_salt() -> u64 {
    static SALT: OnceLock<u64> = OnceLock::new();
    *SALT.get_or_init(|| {
        std::collections::hash_map::RandomState::new().hash_one("bravebot credential fingerprint")
    })
}

/// A value's fingerprint under a salt: equal for equal values, and not reversible to one.
///
/// The salt goes in first and the length after it, so a fingerprint is bound to this run and two
/// different values cannot be run together into a third that matches one of them.
fn fingerprint(salt: u64, value: &str) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    hasher.write_u64(salt);
    hasher.write_usize(value.len());
    hasher.write(value.as_bytes());
    format!("{:016x}", hasher.finish())
}

/// How long the value is and which classes of character it holds, and nothing else.
///
/// Not a prefix, not a suffix, and not an interior sample: see the note at the top of this file
/// about why each of those is the value rather than a description of it.
fn mask(value: &str) -> String {
    let mut classes: Vec<&str> = Vec::new();
    if value.chars().any(|c| c.is_ascii_uppercase()) {
        classes.push("upper case");
    }
    if value.chars().any(|c| c.is_ascii_lowercase()) {
        classes.push("lower case");
    }
    if value.chars().any(|c| c.is_ascii_digit()) {
        classes.push("digits");
    }
    if value.chars().any(|c| !c.is_ascii_alphanumeric()) {
        classes.push("punctuation");
    }
    let count = value.chars().count();
    match classes.is_empty() {
        true => format!("{count} characters"),
        false => format!("{count} characters of {}", classes.join(", ")),
    }
}

/// Everything the layers recognise in one file's text.
///
/// The path is carried through rather than looked at: it is where the finding is, and no layer
/// decides anything from it. Lines are numbered from one, as an editor numbers them.
///
/// The salt is the caller's, so that two texts compared against each other are fingerprinted
/// alike. [`run_salt`] is the one every scan in a run takes.
pub fn scan(path: &str, text: &str, salt: u64) -> Vec<Finding> {
    let lines: Vec<&str> = text.lines().collect();
    let standing = standing_alone(&lines);
    let mut found = Vec::new();

    for (index, line) in lines.iter().enumerate() {
        // Gathered as values and reduced before any of them is fingerprinted. One value is one
        // finding, and two layers reading the same key can spell it differently: a shape cuts at
        // its alphabet where an assignment keeps whatever punctuation the format left on the end.
        // Comparing fingerprints would call those two different credentials and report both.
        let mut on_this_line: Vec<(Kind, String)> = shaped(line);
        on_this_line.extend(url_password(line).map(|value| (Kind::UrlPassword, value)));
        if let Some(value) = assigned(line) {
            on_this_line.push((Kind::Assigned, value));
        }
        // Last of the three, so the reduction below drops it wherever a layer that says more
        // about the value has already matched the same one. This layer says only that the value
        // is the whole file.
        if let Some((at, value)) = &standing
            && *at == index
        {
            on_this_line.push((Kind::Standalone, value.clone()));
        }

        // The more specific layer wins. A value another finding already spans is the same
        // credential seen with more of the surrounding format attached to it.
        let mut kept: Vec<(Kind, String)> = Vec::new();
        for (kind, value) in on_this_line {
            let spanned_by_kept = kept
                .iter()
                .any(|(_, held)| held.contains(&value) || value.contains(held.as_str()));
            if !spanned_by_kept {
                kept.push((kind, value));
            }
        }
        for (kind, value) in kept {
            found.push(finding(kind, path, index + 1, &value, salt));
        }

        if let Some(value) = armoured_key(&lines, index) {
            found.push(finding(Kind::PrivateKey, path, index + 1, &value, salt));
        }
    }
    found
}

/// Build the record, which is the one place a value turns into something that is not one.
fn finding(kind: Kind, path: &str, line: usize, value: &str, salt: u64) -> Finding {
    Finding {
        kind,
        path: path.to_string(),
        line,
        fingerprint: fingerprint(salt, value),
        preview: mask(value),
    }
}

/// The characters a credential is written in, which is what a run of one is bounded by.
///
/// Wider than any single format: the run is cut out first and matched afterwards, so a token
/// sitting in quotes, in YAML, or in a URL comes out the same either way.
///
/// `=` is not one of them, and that is the separator rather than an omission. A rule matches a run
/// that *begins* with its prefix, so admitting `=` would make `KEY=ghp_...` a single run beginning
/// with `KEY`, and every shape below unreachable in the one form a key is most often written in.
/// No [`Body`] admits `=` either, so a value is cut at the same place whether it is here or not.
fn is_token(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '+' | '/' | '.')
}

/// What a key's body is written in, after its prefix.
///
/// A length alone is not a shape. `ASIA-pacific-deployment-notes` is twenty-nine characters
/// beginning with an AWS prefix, and an alphabet is what says it is a phrase rather than a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Body {
    /// Capitals and digits, which is what an AWS key id is.
    Capitals,
    /// Letters either way and digits.
    Alphanumeric,
    /// The same, plus the two characters a URL-safe encoding adds.
    UrlSafe,
}

impl Body {
    fn admits(self, c: char) -> bool {
        match self {
            Body::Capitals => c.is_ascii_uppercase() || c.is_ascii_digit(),
            Body::Alphanumeric => c.is_ascii_alphanumeric(),
            Body::UrlSafe => c.is_ascii_alphanumeric() || matches!(c, '_' | '-'),
        }
    }
}

/// The providers whose keys say what they are: a prefix, an alphabet, and how much of it.
///
/// A rule matches a run that *begins* with the prefix, never one that contains it, so a variable
/// named after a provider is not a key from it.
const SHAPES: &[(Kind, &[&str], Body, usize)] = &[
    (Kind::AwsAccessKey, &["AKIA", "ASIA"], Body::Capitals, 16),
    (
        Kind::GitHubToken,
        &["ghp_", "gho_", "ghu_", "ghs_", "ghr_"],
        Body::Alphanumeric,
        36,
    ),
    (Kind::GitHubToken, &["github_pat_"], Body::UrlSafe, 22),
    (
        Kind::SlackToken,
        &["xoxb-", "xoxp-", "xoxa-", "xoxs-", "xoxr-"],
        Body::UrlSafe,
        10,
    ),
    (Kind::GoogleApiKey, &["AIza"], Body::UrlSafe, 35),
    (
        Kind::StripeKey,
        &["sk_live_", "rk_live_"],
        Body::Alphanumeric,
        16,
    ),
    (Kind::AnthropicKey, &["sk-ant-"], Body::UrlSafe, 24),
];

/// Every named provider key on one line, each cut to where its alphabet stops.
///
/// Cut rather than taken whole, because a key is written in prose and in configuration with
/// something after it: the sentence's full stop, a closing quote, a comma. Taking the run as it
/// stands would fingerprint the punctuation with the key, so the same key would look like a
/// different one each time it appeared.
fn shaped(line: &str) -> Vec<(Kind, String)> {
    let mut found = Vec::new();
    for run in line.split(|c: char| !is_token(c)) {
        for (kind, prefixes, body, least) in SHAPES {
            let matched = prefixes.iter().find_map(|prefix| {
                let tail = run.strip_prefix(prefix)?;
                let taken: String = tail.chars().take_while(|c| body.admits(*c)).collect();
                (taken.len() >= *least).then(|| format!("{prefix}{taken}"))
            });
            if let Some(value) = matched {
                // A prefix and a length are a shape, and a documented placeholder has both: the
                // line a README tells somebody to copy carries the real prefix and enough
                // characters after it, so the shape alone cannot tell it from an issued key. The
                // stand-in words are what says a human typed it as an example, and the rarity
                // layer already refuses on them — without this the two layers disagree about the
                // same value and a `.env.example` is refused.
                if !is_a_filler(&value) {
                    found.push((*kind, value));
                }
                break;
            }
        }
    }
    found
}

/// The password in a connection string, which is where a generated one most often reaches a tree.
///
/// `scheme://user:password@host` says the value is a password in its own syntax, so this needs no
/// name beside it and no guess about how rare it looks: the format has already declared what the
/// field is. That is why it is a layer of its own rather than a case in [`assigned`], which cuts a
/// line at the first `:` or `=` and would take `//user` as the name of a secret called `password`.
///
/// Only the password field is taken. The user, the host and the path are not credentials, and a
/// finding over the whole URL would fingerprint the database name with the secret.
fn url_password(line: &str) -> Option<String> {
    let (_, after_scheme) = line.split_once("://")?;
    // The authority ends at the first `/`, `?` or `#`; anything after that is a path, and an `@`
    // in a path is not a credential separator.
    let authority = after_scheme
        .split(['/', '?', '#'])
        .next()
        .unwrap_or(after_scheme);
    // The last `@` divides credentials from host: a password may itself contain one.
    let (userinfo, _) = authority.rsplit_once('@')?;
    let (_, password) = userinfo.split_once(':')?;
    let password = password.trim_end_matches(['"', '\'']);

    // A reference to a secret is not one, and an empty field is not a password.
    if password.is_empty() || password.contains("${") || password.starts_with('$') {
        return None;
    }
    if is_a_filler(password) || password.len() < 8 {
        return None;
    }
    Some(password.to_string())
}

/// The names that say the value beside them is a secret.
///
/// Matched against the name only. A keyword in a value decides nothing, which is what keeps a
/// document *about* secrets from reading as a file full of them.
const NAMES: &[&str] = &[
    "SECRET",
    "TOKEN",
    "PASSWORD",
    "PASSWD",
    "PASSPHRASE",
    "APIKEY",
    "API_KEY",
    "ACCESSKEY",
    "ACCESS_KEY",
    "PRIVATEKEY",
    "PRIVATE_KEY",
    "CREDENTIAL",
];

/// A value assigned to a name that says it is a secret, where the value is rare enough to be one.
///
/// `NAME=value`, `name: value` and `"name": "value"` are one shape with different punctuation
/// around it, so the name is taken from the left of the first separator and the value from the
/// right of it, each stripped of the quoting the format put there.
fn assigned(line: &str) -> Option<String> {
    let cut = line.find(['=', ':'])?;
    let (name, value) = line.split_at(cut);
    let value = trim_quoting(&value[1..]);

    let name = name
        .trim()
        .trim_start_matches("export ")
        .to_ascii_uppercase();
    let name = trim_quoting(&name);
    if !NAMES.iter().any(|keyword| name.contains(keyword)) {
        return None;
    }

    looks_rare(value).then(|| value.to_string())
}

/// A rare value standing as the whole of a file, which is the shape creating one takes.
///
/// A value a turn generates arrives with nothing beside it: no provider stamped a prefix on it,
/// so [`shaped`] has nothing to match, and there is no name to the left of a separator, so
/// [`assigned`] has no name to believe. The file *is* the secret, which is how a framework key,
/// a signing key and a generated token reach a tree, and it was the one shape no layer here read.
///
/// What stands in for the name is the file itself. A document mentioning a secret says something
/// else as well: a key, a comment, a second line. A file that is one rare token and nothing else
/// is a value rather than a document, and that is as much as this layer claims, which is why
/// [`Kind::is_declared`] treats it as inferred.
///
/// Returns where the value is as well as what it is, because a finding says which line it is on
/// and a file may open with blank lines.
fn standing_alone(lines: &[&str]) -> Option<(usize, String)> {
    let mut only: Option<(usize, &str)> = None;
    for (index, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if only.replace((index, trimmed)).is_some() {
            return None;
        }
    }
    let (index, line) = only?;
    let value = trim_quoting(line);
    // Anything a value is not written in says this line is prose, an assignment or a fragment of
    // a format, and each of those is another layer's business.
    if !value.chars().all(is_token) {
        return None;
    }
    looks_rare(value).then(|| (index, value.to_string()))
}

/// Whether this text is one value and nothing else, which is what [`Scanned::only_the_value`]
/// carries to the caller deciding what to tell a turn.
pub fn stands_alone(text: &str) -> bool {
    standing_alone(&text.lines().collect::<Vec<&str>>()).is_some()
}

/// What the format put around a value, taken off: spaces, quotes, and the punctuation a container
/// closes with.
///
/// The closing brackets matter for agreement between layers rather than for tidiness. In
/// `{"apiKey":"AIza..."}` the shape layer cuts the value at its alphabet and this one used to keep
/// the trailing `"}`, so one key produced two spellings, two fingerprints, and two findings for a
/// person to chase. Taken in two passes because the quote sits inside the bracket.
fn trim_quoting(value: &str) -> &str {
    value
        .trim()
        .trim_end_matches([',', ';', '}', ']', ')'])
        .trim()
        .trim_matches(['"', '\''])
        .trim_end_matches([',', ';', '}', ']', ')'])
        .trim()
        .trim_matches(['"', '\''])
}

/// Words that say a value is a stand-in for one rather than one.
const STAND_INS: &[&str] = &[
    "xxxx",
    "changeme",
    "change-me",
    "change_me",
    "example",
    "placeholder",
    "redacted",
    "your-",
    "your_",
    "dummy",
    "sample",
    "fake",
    "notasecret",
];

/// The words that say a human typed this where a key goes, asked of a value that already matched
/// a provider's shape.
///
/// A narrower list than [`STAND_INS`], and deliberately so. `example` and `sample` are in that one
/// because a value the rarity layer is guessing about is not worth refusing over; here the shape
/// has already matched a prefix and an alphabet, and `AKIAIOSFODNN7EXAMPLE` is AWS's own
/// documented key id — well formed, the fixture every scanner tests against, and the thing a
/// person most wants told about if a turn writes it. So a blank somebody has to fill in is
/// recognised by the filling-in, not by the word `example` appearing anywhere in the value.
const FILLERS: &[&str] = &[
    "xxxx",
    "changeme",
    "change-me",
    "change_me",
    "replace",
    "placeholder",
    "redacted",
    "your-",
    "your_",
    "goes-here",
    "goes_here",
    "dummy",
    "fake",
    "notasecret",
];

/// Whether a value that matched a provider's shape is a blank rather than a key.
fn is_a_filler(value: &str) -> bool {
    let lowered = value.to_ascii_lowercase();
    FILLERS.iter().any(|word| lowered.contains(word))
}

/// Whether a value is rare enough that a name calling it a secret should be believed.
///
/// Four questions, and a value has to answer all of them. Is it long enough to be a key rather
/// than a word. Does it mix letters and digits, which a sentence somebody typed does not. Is it
/// spelt out here at all, rather than being a reference to somewhere else. And is it varied
/// enough, measured as bits per character, that it reads as generated.
fn looks_rare(value: &str) -> bool {
    if value.len() < 20 || value.contains(char::is_whitespace) {
        return false;
    }
    if !value.chars().any(|c| c.is_ascii_digit()) || !value.chars().any(|c| c.is_ascii_alphabetic())
    {
        return false;
    }
    if value.contains("${") || value.starts_with('<') || value.starts_with('$') {
        return false;
    }
    if STAND_INS
        .iter()
        .any(|word| value.to_ascii_lowercase().contains(word))
    {
        return false;
    }
    entropy(value) >= 3.0
}

/// Bits per character, counted over the value's own characters.
///
/// A generated key is close to the most its alphabet allows: four bits a character for hex, near
/// six for base64. English words are under three, which is where the bar sits.
fn entropy(value: &str) -> f64 {
    let total = value.chars().count() as f64;
    if total == 0.0 {
        return 0.0;
    }
    let mut counts: std::collections::BTreeMap<char, usize> = std::collections::BTreeMap::new();
    for c in value.chars() {
        *counts.entry(c).or_default() += 1;
    }
    counts
        .values()
        .map(|count| {
            let share = *count as f64 / total;
            -share * share.log2()
        })
        .sum()
}

/// The line a PEM block opens with, so the body after it can be taken whole.
const PEM_BEGINS: &str = "-----BEGIN";
/// What the opening line of a *private* key block says, which is the half of PEM that matters.
const PEM_PRIVATE: &str = "PRIVATE KEY-----";

/// The body of a private key whose armour opens on this line, joined into one value.
///
/// A key is armoured across as many lines as it takes, so the value is the body rather than any
/// one line of it: fingerprinting a line would make the same key look like a dozen findings, and
/// the same key reflowed look like a different one.
fn armoured_key(lines: &[&str], index: usize) -> Option<String> {
    let opening = lines[index].trim();
    if !opening.starts_with(PEM_BEGINS) || !opening.ends_with(PEM_PRIVATE) {
        return None;
    }
    let body: String = lines[index + 1..]
        .iter()
        .take_while(|line| !line.trim().starts_with("-----END"))
        .map(|line| line.trim())
        .collect();
    (!body.is_empty()).then_some(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A salt is what keeps a fingerprint from being a lookup: the same well-known key
    /// fingerprints differently in two runs, so a collection of them is not a dictionary anybody
    /// can match a guess against.
    #[test]
    fn the_same_value_fingerprints_differently_under_two_salts() {
        let value = "AKIAIOSFODNN7EXAMPLE";
        assert_eq!(fingerprint(1, value), fingerprint(1, value));
        assert_ne!(fingerprint(1, value), fingerprint(2, value));
    }

    /// Attribution rests on this: a value the turn wrote is told from one already in the file by
    /// comparing fingerprints, so two spellings of the same secret have to agree and two
    /// different secrets have to differ.
    #[test]
    fn a_fingerprint_follows_the_value_and_not_where_it_was_found() {
        assert_eq!(
            fingerprint(7, "hunter2hunter2hunter2"),
            fingerprint(7, "hunter2hunter2hunter2")
        );
        assert_ne!(
            fingerprint(7, "hunter2hunter2hunter2"),
            fingerprint(7, "hunter2hunter2hunter3")
        );
    }

    /// A preview that quotes either end of the value has published the part of it an attacker
    /// uses to recognise the one they are holding, which is the whole reason the familiar
    /// star-in-the-middle form is refused here. Checked from four characters up, because that is
    /// what that form shows of each end.
    #[test]
    fn a_preview_is_no_part_of_the_value_it_describes() {
        for value in [
            "AKIAIOSFODNN7EXAMPLE",
            // A fixture, not a token: the shape is the whole point of it, so this file's own
            // rules and the repository's own secret scan both recognise it.
            "ghp_0123456789abcdefghijklmnopqrstuvwxyzAB", // nosemgrep: generic.secrets.gitleaks.github-pat.github-pat
            "c8f1a0b4d2e6f7a9c3b5d8e0f2a4c6b8",
        ] {
            let preview = mask(value);
            for taken in 4..=value.len() {
                assert!(
                    !preview.contains(&value[..taken]),
                    "the preview {preview} quotes the first {taken} characters of {value}"
                );
                assert!(
                    !preview.contains(&value[value.len() - taken..]),
                    "the preview {preview} quotes the last {taken} characters of {value}"
                );
            }
            assert!(
                !value.contains(&preview),
                "the preview {preview} is a piece of {value}"
            );
        }
    }

    /// The preview is what tells one finding from another on a person's screen, so it has to say
    /// the two things that differ between credentials of the same kind.
    #[test]
    fn a_preview_says_the_length_and_the_character_classes() {
        assert_eq!(
            mask("AKIAIOSFODNN7EXAMPLE"),
            "20 characters of upper case, digits"
        );
        assert_eq!(
            mask("abc-123"),
            "7 characters of lower case, digits, punctuation"
        );
    }

    /// The prefix layer is what recognises a key nothing in the file says is one, which is the
    /// case a name-based rule cannot reach.
    #[test]
    fn a_provider_key_is_recognised_with_nothing_around_it_saying_so() {
        let found = scan("t.txt", "the value is AKIAIOSFODNN7EXAMPLE here\n", 1);
        assert_eq!(found.len(), 1, "got {found:?}");
        assert_eq!(found[0].kind, Kind::AwsAccessKey);
        assert_eq!(found[0].line, 1);
    }

    /// A rule that matched a prefix anywhere in a run would report every identifier a provider's
    /// name appears in, and a scan reporting ordinary code is a scan people turn off.
    #[test]
    fn a_prefix_inside_a_longer_word_is_not_a_key() {
        assert!(scan("t.txt", "MYAKIAIOSFODNN7EXAMPLE = 1\n", 1).is_empty());
    }

    /// A length after a prefix is not a shape. A phrase beginning with a provider's prefix is
    /// long enough to be a key of that provider and is written in an alphabet no key uses.
    #[test]
    fn a_phrase_beginning_with_a_prefix_is_not_a_key() {
        assert!(scan("notes.md", "ASIA-pacific-deployment-notes\n", 1).is_empty());
    }

    /// The same key in prose and in a quoted field has to fingerprint alike, or attribution
    /// compares a key against the same key with a full stop on the end and calls them different
    /// credentials.
    #[test]
    fn a_key_is_the_key_and_not_the_punctuation_after_it() {
        let quoted = scan("a.yml", "  key: \"AKIAIOSFODNN7EXAMPLE\"\n", 1);
        let prose = scan("b.md", "the key is AKIAIOSFODNN7EXAMPLE.\n", 1);
        assert_eq!(quoted.len(), 1, "got {quoted:?}");
        assert_eq!(prose.len(), 1, "got {prose:?}");
        assert_eq!(quoted[0].fingerprint, prose[0].fingerprint);
        assert_eq!(quoted[0].preview, "20 characters of upper case, digits");
    }

    /// `KEY=value` is where a credential is written more often than anywhere else, and the prefix
    /// layer has to reach into it. It did not: `=` was a token character, so the name and the key
    /// were one run, the run began with the name, and no shape matched. What caught a key in a
    /// `.env` at all was the rarity layer underneath, and only when the name held one of [`NAMES`]
    /// — so `GH_PAT=ghp_...` and `gcp_key=AIza...` were found by nothing.
    #[test]
    fn a_provider_key_is_recognised_when_it_is_assigned_to_a_name() {
        // Each body below is a counted-off alphabet at exactly the shape's declared minimum, so a
        // fixture carries the length and the character classes a rule matches on and reads as
        // nothing an issuer would hand out. AWS is the vendor's own documented example value.
        for (line, kind) in [
            (
                "GH_PAT=ghp_0123456789abcdefghijklmnopqrstuvwxyz\n", // nosemgrep: generic.secrets.gitleaks.github-pat.github-pat, generic.secrets.security.detected-github-token.detected-github-token
                Kind::GitHubToken,
            ),
            (
                "gcp_key=AIza0123456789abcdefghijklmnopqrstuvwxy\n",
                Kind::GoogleApiKey,
            ),
            ("STRIPE=sk_live_0123456789abcdef\n", Kind::StripeKey),
            ("SLACK_BOT=xoxb-0123456789\n", Kind::SlackToken),
            (
                "ANTHROPIC=sk-ant-0123456789abcdefghijklmn\n",
                Kind::AnthropicKey,
            ),
            (
                "AWS_ACCESS_KEY_ID=AKIAIOSFODNN7EXAMPLE\n",
                Kind::AwsAccessKey,
            ),
            (
                "export GH=ghp_0123456789abcdefghijklmnopqrstuvwxyz\n", // nosemgrep: generic.secrets.gitleaks.github-pat.github-pat, generic.secrets.security.detected-github-token.detected-github-token
                Kind::GitHubToken,
            ),
        ] {
            let found = scan(".env", line, 1);
            assert!(
                found.iter().any(|finding| finding.kind == kind),
                "no {kind:?} found in {line:?}, got {found:?}"
            );
        }
    }

    /// The same key assigned and standing alone is the same key, so attribution compares the two
    /// as equal. Cutting at `=` is what makes the value the key rather than the name and the key
    /// together.
    #[test]
    fn a_key_fingerprints_alike_assigned_and_alone() {
        let assigned = scan(".env", "AWS_ACCESS_KEY_ID=AKIAIOSFODNN7EXAMPLE\n", 1);
        let alone = scan("t.txt", "AKIAIOSFODNN7EXAMPLE\n", 1);
        assert_eq!(assigned.len(), 1, "got {assigned:?}");
        assert_eq!(alone.len(), 1, "got {alone:?}");
        assert_eq!(assigned[0].fingerprint, alone[0].fingerprint);
    }

    /// A `.env.example` is a file a project is expected to hold, and the line in it carries the
    /// provider's real prefix and enough characters after it to satisfy the shape — that is what
    /// makes it copyable. So the prefix layer has to consult the stand-in words too: the rarity
    /// layer refused these already, and a shape that did not would have the two layers disagree
    /// about one value and refuse a write of the documentation telling somebody what to set.
    #[test]
    fn a_documented_placeholder_carrying_a_real_prefix_is_not_a_key() {
        for line in [
            "ANTHROPIC_API_KEY=sk-ant-api03-REPLACE-THIS-WITH-YOUR-REAL-KEY\n",
            "GITHUB_TOKEN=ghp_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx\n", // nosemgrep: generic.secrets.gitleaks.github-pat.github-pat, generic.secrets.security.detected-github-token.detected-github-token
            "GOOGLE_API_KEY=AIzaYOUR_GOOGLE_API_KEY_GOES_HERE_1234\n",
        ] {
            assert!(
                scan(".env.example", line, 1).is_empty(),
                "a documented placeholder was reported as a key: {line:?}"
            );
        }
    }

    /// A connection string is how a generated password most often reaches a tree, and neither
    /// other layer sees it: the URL carries no name a rule matches, and [`assigned`] cuts the line
    /// at the first `:`, which in a URL is the one after the scheme.
    #[test]
    fn a_password_in_a_connection_string_is_a_finding() {
        for line in [
            "DATABASE_URL=postgres://appuser:p9Kx2mQ7vL4nR8tZ3wY6@db.internal:5432/app\n",
            "REDIS_URL=redis://:s3cretP9Kx2mQ7vL4nR8tZ@cache:6379/0\n", // nosemgrep: trailofbits.generic.redis-unencrypted-transport.redis-unencrypted-transport
            "AMQP=amqp://svc:9zQmR4tL7vX2nB8kC5wY3jH6@mq:5672\n", // nosemgrep: trailofbits.generic.amqp-unencrypted-transport.amqp-unencrypted-transport
            "  url: \"mongodb://admin:Tr0ub4dor3xKx2mQ7vL4@cluster0/db\"\n", // nosemgrep: trailofbits.generic.mongodb-insecure-transport.mongodb-insecure-transport
        ] {
            let found = scan(".env", line, 1);
            assert!(
                found.iter().any(|f| f.kind == Kind::UrlPassword),
                "no password found in {line:?}, got {found:?}"
            );
        }
    }

    /// The password and nothing else. A finding over the whole URL would fingerprint the host and
    /// the database name along with the secret, so the same password at a second host would read
    /// as a different credential and attribution would not match it.
    #[test]
    fn a_connection_string_finding_is_the_password_and_not_the_url() {
        let at_one_host = scan(
            "a.env",
            "DATABASE_URL=postgres://u:p9Kx2mQ7vL4nR8tZ3wY6@host-one:5432/app\n",
            1,
        );
        let at_another = scan(
            "b.env",
            "DATABASE_URL=postgres://u:p9Kx2mQ7vL4nR8tZ3wY6@host-two:5432/other\n",
            1,
        );
        let one = at_one_host
            .iter()
            .find(|f| f.kind == Kind::UrlPassword)
            .expect("a finding");
        let two = at_another
            .iter()
            .find(|f| f.kind == Kind::UrlPassword)
            .expect("a finding");
        assert_eq!(one.fingerprint, two.fingerprint);
        assert_eq!(
            one.preview,
            "20 characters of upper case, lower case, digits"
        );
    }

    /// A URL that spells no password, or spells a reference to one, is not a credential. A path
    /// holding an `@` is not a credential separator either.
    #[test]
    fn a_connection_string_without_a_password_is_not_a_finding() {
        for line in [
            "DATABASE_URL=postgres://appuser@db.internal:5432/app\n",
            "DATABASE_URL=postgres://appuser:${DB_PASSWORD}@db.internal/app\n",
            "DATABASE_URL=postgres://appuser:$DB_PASSWORD@db.internal/app\n",
            "DATABASE_URL=postgres://appuser:your-password-here@db/app\n",
            "DOCS=https://example.com/guide/user:pass@notes\n",
            "REDIS_URL=redis://cache:6379/0\n", // nosemgrep: trailofbits.generic.redis-unencrypted-transport.redis-unencrypted-transport
        ] {
            let found: Vec<_> = scan(".env", line, 1)
                .into_iter()
                .filter(|f| f.kind == Kind::UrlPassword)
                .collect();
            assert!(found.is_empty(), "reported for {line:?}: {found:?}");
        }
    }

    /// One value is one finding, however many layers recognise it. A provider key in a quoted JSON
    /// field is seen by the shape layer, which cuts at its alphabet, and by the assignment layer,
    /// which used to keep the `"}` the container closed with, giving two spellings of one key, two
    /// fingerprints, so a person chasing a second credential that was never there.
    #[test]
    fn one_key_in_a_json_field_is_one_finding() {
        for line in [
            "{\"apiKey\": \"AIza0123456789abcdefghijklmnopqrstuvwxy\"}\n",
            "  {\"token\": \"ghp_0123456789abcdefghijklmnopqrstuvwxyz\"},\n", // nosemgrep: generic.secrets.gitleaks.github-pat.github-pat
            "secrets: [\"sk_live_0123456789abcdef\"]\n",
        ] {
            let found = scan("conf.json", line, 1);
            assert_eq!(found.len(), 1, "for {line:?} got {found:?}");
        }
    }

    /// A length is half of what a shape is, and nothing pinned the halves. Each rule is checked at
    /// its declared minimum and one character short of it, so a threshold cannot drift without a
    /// test saying so.
    #[test]
    fn each_shape_matches_at_its_minimum_and_not_below_it() {
        for (kind, prefixes, body, least) in SHAPES {
            // Written in the rule's own alphabet, or the length would not be the thing under test:
            // an AWS body admits capitals only, so a lower case filler fails it for the wrong
            // reason.
            let alphabet: String = ('a'..='z')
                .chain('A'..='Z')
                .chain('0'..='9')
                .filter(|c| body.admits(*c))
                .collect();
            for prefix in *prefixes {
                let body: String = alphabet.chars().cycle().take(*least).collect();
                let at_minimum = format!("{prefix}{body}");
                let found = scan("t.txt", &format!("{at_minimum}\n"), 1);
                assert!(
                    found.iter().any(|f| f.kind == *kind),
                    "{kind:?} did not match at its minimum of {least}: {at_minimum}"
                );

                let short: String = alphabet.chars().cycle().take(least - 1).collect();
                let below = format!("{prefix}{short}");
                let found = scan("t.txt", &format!("{below}\n"), 1);
                assert!(
                    !found.iter().any(|f| f.kind == *kind),
                    "{kind:?} matched one character below its minimum: {below}"
                );
            }
        }
    }

    /// A generated framework key carries no provider prefix, so the only thing that says what it
    /// is is the name beside it and how rare it looks. This is the case the clause is written
    /// about.
    #[test]
    fn a_generated_key_is_recognised_from_its_name_and_its_rarity() {
        let line = "SECRET_KEY_BASE=c8f1a0b4d2e6f7a9c3b5d8e0f2a4c6b8d1e3f5a7\n";
        let found = scan(".env", line, 1);
        assert_eq!(found.len(), 1, "got {found:?}");
        assert_eq!(found[0].kind, Kind::Assigned);
    }

    /// The shape creating a credential takes, and the one no layer read: a turn asked to finish
    /// setting a project up generates a key, and the file it writes *is* the key. There is no
    /// provider prefix to match and no name to the left of a separator to believe, so the value
    /// went into the tree with nothing found and nothing said.
    #[test]
    fn a_generated_key_standing_as_a_whole_file_is_recognised() {
        let found = scan(
            "config/master.key",
            "c8f1a0b4d2e6f7a9c3b5d8e0f2a4c6b8d1e3f5a7\n",
            1,
        );
        assert_eq!(found.len(), 1, "got {found:?}");
        assert_eq!(found[0].kind, Kind::Standalone);
        assert_eq!(found[0].line, 1);
    }

    /// What stands in for a name here is the file being nothing else. A rule that read a lone
    /// *line* instead would report the one bare token in a document full of them, which is a
    /// changelog, a list of hashes and half the fixtures in a test tree.
    #[test]
    fn a_rare_token_with_a_document_around_it_is_not_a_whole_file() {
        for text in [
            "# the key for this environment\nc8f1a0b4d2e6f7a9c3b5d8e0f2a4c6b8d1e3f5a7\n",
            "c8f1a0b4d2e6f7a9c3b5d8e0f2a4c6b8d1e3f5a7\nc8f1a0b4d2e6f7a9c3b5d8e0f2a4c6b9d1e3f5a7\n",
            "the key is c8f1a0b4d2e6f7a9c3b5d8e0f2a4c6b8d1e3f5a7\n",
        ] {
            assert!(scan("notes.md", text, 1).is_empty(), "reported: {text}");
        }
    }

    /// Blank lines around the value are the file as an editor leaves it, and a rule counting them
    /// as contents would read the commonest spelling of this file as a document.
    #[test]
    fn blank_lines_around_the_value_are_not_contents() {
        let found = scan(
            "master.key",
            "\n\nc8f1a0b4d2e6f7a9c3b5d8e0f2a4c6b8d1e3f5a7\n\n",
            1,
        );
        assert_eq!(found.len(), 1, "got {found:?}");
        assert_eq!(found[0].kind, Kind::Standalone);
        assert_eq!(found[0].line, 3, "a finding has to say where the value is");
    }

    /// Two layers reading the same value would report one key twice, and cut it differently:
    /// attribution compares fingerprints, so the same secret under two spellings is two
    /// credentials and the second is never excused by the file that already held it.
    #[test]
    fn a_provider_key_standing_alone_is_one_finding_and_not_two() {
        // A counted-off alphabet at the shape's declared minimum, as the fixtures above are.
        let key = "ghp_0123456789abcdefghijklmnopqrstuvwxyzAB"; // nosemgrep: generic.secrets.gitleaks.github-pat.github-pat
        let found = scan("token.txt", &format!("{key}\n"), 1);
        assert_eq!(found.len(), 1, "got {found:?}");
        assert_eq!(
            found[0].kind,
            Kind::GitHubToken,
            "the layer that says less about the value won"
        );
    }

    /// Nothing on the line says this value is a secret, only that the file holds one value. A
    /// commit id, a machine identifier and a digest are written exactly that way, so a turn
    /// refused outright for one of them would have no way to say otherwise, which is the reason
    /// the rarity layer is a question rather than a rule.
    #[test]
    fn a_value_standing_as_a_whole_file_is_a_question_and_not_a_rule() {
        let found = scan(
            "master.key",
            "c8f1a0b4d2e6f7a9c3b5d8e0f2a4c6b8d1e3f5a7\n",
            1,
        );
        let scanned = Scanned {
            authored: found,
            carried: Vec::new(),
            only_the_value: true,
        };
        assert!(scanned.refused().is_empty(), "refused without asking");
        assert_eq!(scanned.to_approve().len(), 1, "nobody was asked");
    }

    /// The difference between a credential copied into a document and one created as a file, which
    /// is what decides whether a turn is told to write a reference or told that nothing was
    /// created. Asked of the body, so a value another layer named answers it the same way.
    #[test]
    fn a_file_that_is_the_value_is_told_from_one_that_mentions_it() {
        let value = "c8f1a0b4d2e6f7a9c3b5d8e0f2a4c6b8d1e3f5a7";
        assert!(stands_alone(&format!("{value}\n")));
        assert!(!stands_alone(&format!("SECRET_KEY_BASE={value}\n")));
        assert!(!stands_alone(&format!("# generated\n{value}\n")));
        assert!(!stands_alone("a file of ordinary prose about nothing\n"));
    }

    /// A configuration file naming the variable it wants set is the commonest thing in a tree,
    /// and a scan that refused a write of one would refuse most writes of configuration.
    #[test]
    fn a_name_with_a_stand_in_beside_it_is_not_a_secret() {
        for line in [
            "SECRET_KEY_BASE=changeme\n",
            "API_KEY=your-api-key-goes-here1\n",
            "DATABASE_PASSWORD=${DATABASE_PASSWORD}\n",
            "GITHUB_TOKEN=<your token here>\n",
            "SECRET_KEY_BASE=\n",
        ] {
            assert!(
                scan(".env", line, 1).is_empty(),
                "reported a stand-in: {line}"
            );
        }
    }

    /// Prose about credentials is what this repository's own documents are full of, and a rule
    /// reading the value rather than the name would report every one of them.
    #[test]
    fn a_keyword_in_the_value_rather_than_the_name_is_not_a_secret() {
        let line = "description: the deploy step needs a SECRET_KEY_BASE of 64 hex digits\n";
        assert!(scan("docs.md", line, 1).is_empty());
    }

    /// A key is armoured across as many lines as it takes. Reported per line it would be a dozen
    /// findings for one key, and reflowing it would produce a dozen different ones.
    #[test]
    fn an_armoured_private_key_is_one_finding_over_its_whole_body() {
        let pem = armoured("RSA PRIVATE", &["MIIBOgIBAAJBAK7", "Z9fGh2kQ=="]);
        let found = scan("id_rsa", &pem, 1);
        assert_eq!(found.len(), 1, "got {found:?}");
        assert_eq!(found[0].kind, Kind::PrivateKey);
        assert_eq!(found[0].line, 1);
        assert_eq!(
            found[0].fingerprint,
            fingerprint(1, "MIIBOgIBAAJBAK7Z9fGh2kQ==")
        );
    }

    /// A public key is armoured the same way and is not a credential. A rule keyed on the armour
    /// alone would report every certificate in a tree.
    #[test]
    fn a_public_key_in_the_same_armour_is_not_a_finding() {
        let pem = armoured("PUBLIC", &["MIIBOgIBAAJBAK7"]);
        assert!(scan("id_rsa.pub", &pem, 1).is_empty());
    }

    /// A PEM block of a given kind, assembled rather than written out.
    ///
    /// A literal block of private key armour in this file is a hard-coded private key to every
    /// scanner that reads source, this repository's own included, and a fixture that has to be
    /// excused by each of them in turn is a fixture worth not writing. Assembled, the two tests
    /// above also differ in the one word that is the difference between them.
    fn armoured(kind: &str, body: &[&str]) -> String {
        let mut pem = format!("-----BEGIN {kind} KEY-----\n");
        for line in body {
            pem.push_str(line);
            pem.push('\n');
        }
        pem.push_str(&format!("-----END {kind} KEY-----\n"));
        pem
    }

    /// Where a finding is is half of what a person needs to act on one, and a line number counted
    /// from zero sends them to the line above it in every editor there is.
    #[test]
    fn a_finding_says_which_line_it_is_on_counted_from_one() {
        let text = "first\nsecond\nAKIAIOSFODNN7EXAMPLE\n";
        let found = scan("t.txt", text, 1);
        assert_eq!(found.len(), 1, "got {found:?}");
        assert_eq!(found[0].line, 3);
    }

    /// Everything a person is shown about a finding goes through this sentence, so the value
    /// leaking anywhere leaks here.
    #[test]
    fn nothing_a_finding_says_repeats_the_value() {
        let value = "c8f1a0b4d2e6f7a9c3b5d8e0f2a4c6b8d1e3f5a7";
        let found = scan(".env", &format!("SECRET_KEY_BASE={value}\n"), 1);
        let said = found[0].describe();
        assert!(!said.contains(value), "the value is in the record: {said}");
        for window in value.as_bytes().windows(8) {
            let piece = std::str::from_utf8(window).unwrap();
            assert!(!said.contains(piece), "{piece} of the value is in {said}");
        }
    }
}
