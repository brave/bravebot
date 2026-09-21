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
    /// A value assigned to a name that says it is a secret, rare enough to be one.
    Assigned,
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
            Kind::Assigned => "a secret assigned by name",
        }
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
    let mut found = Vec::new();

    for (index, line) in lines.iter().enumerate() {
        for (kind, value) in shaped(line) {
            found.push(finding(kind, path, index + 1, &value, salt));
        }
        if let Some(value) = assigned(line) {
            found.push(finding(Kind::Assigned, path, index + 1, &value, salt));
        }
        if let Some(value) = armoured_key(&lines, index) {
            found.push(finding(Kind::PrivateKey, path, index + 1, &value, salt));
        }
    }

    // One value is one finding. A provider key assigned to a name that says it is a secret is
    // recognised by both layers, and reporting it twice would have a person looking for a second
    // credential that is not there. The first layer to name it is the more specific one.
    let mut seen = std::collections::BTreeSet::new();
    found.retain(|finding| seen.insert((finding.line, finding.fingerprint.clone())));
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
fn is_token(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '+' | '/' | '=' | '.')
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
                found.push((*kind, value));
                break;
            }
        }
    }
    found
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

/// What the format put around a value, taken off: spaces, quotes, and a separator at the end.
fn trim_quoting(value: &str) -> &str {
    value
        .trim()
        .trim_end_matches([',', ';'])
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
    let lowered = value.to_ascii_lowercase();
    if STAND_INS.iter().any(|word| lowered.contains(word)) {
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
