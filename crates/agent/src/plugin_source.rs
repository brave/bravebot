//! Where a plugin is installed from (PLUGIN-3, PLUGIN-5, PLUGIN-14).
//!
//! What a person types after `/plugin add` is read here, from the typed text alone, into a local
//! path, a git URL with an optional ref, or the reason it is neither. No byte of a staged tree or
//! of git's output reaches this module except a resolved commit, which [`Commit::parse`] admits
//! only as 40 lowercase hex digits. Nothing here opens a file, resolves a name or starts a program.
//!
//! A [`Source`] is what the install record and the install prompt name as the origin of a plugin.

use bravebot_core::url::host_of;
use std::fmt;
use std::path::PathBuf;

/// A full commit id: 40 lowercase hex digits and nothing else. The shape is checked and nothing is
/// decided from the value, so a commit that git printed is carried to the prompt and the record
/// without the driver reading it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Commit(String);

impl Commit {
    /// The number of hex digits of a commit id.
    pub const LENGTH: usize = 40;

    /// `text` as a commit where it is [`Self::LENGTH`] lowercase hex digits.
    pub fn parse(text: &str) -> Option<Self> {
        let hex = text.len() == Self::LENGTH
            && text
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
        hex.then(|| Self(text.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Commit {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// The origin of an installed plugin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// A directory on this machine. Once staged it is the canonical absolute path.
    Local(PathBuf),
    /// A git repository, the ref the person asked for and the commit it resolved to.
    Git {
        url: String,
        /// The text after `#`, or `None` where the person named no ref.
        requested_ref: Option<String>,
        commit: Commit,
    },
}

impl Source {
    /// The resolved commit, which a local directory has none of.
    pub fn commit(&self) -> Option<&Commit> {
        match self {
            Source::Local(_) => None,
            Source::Git { commit, .. } => Some(commit),
        }
    }

    /// Whether `other` is the same thing to install from: the same directory, or the same URL and
    /// the same requested ref. The commit a ref resolved to is not part of it, since a branch
    /// moves and the same source then resolves to another commit.
    pub fn same_origin(&self, other: &Source) -> bool {
        match (self, other) {
            (Source::Local(a), Source::Local(b)) => a == b,
            (
                Source::Git {
                    url: a,
                    requested_ref: a_ref,
                    ..
                },
                Source::Git {
                    url: b,
                    requested_ref: b_ref,
                    ..
                },
            ) => a == b && a_ref == b_ref,
            _ => false,
        }
    }
}

impl fmt::Display for Source {
    /// The source as the record and the listings write it: the path, or the URL without a ref.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Source::Local(path) => write!(formatter, "{}", path.display()),
            Source::Git { url, .. } => formatter.write_str(url),
        }
    }
}

/// A git URL and ref as typed, before anything is fetched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitSource {
    /// The URL as typed, without the `#ref`. It holds no control character or whitespace.
    pub url: String,
    /// The host the URL names, lowercased, taken from the URL's own authority with userinfo
    /// dropped (`user@host`, a port and brackets are not part of it). It is the host a person is
    /// asked about before a request leaves.
    pub host: String,
    /// The text after the first `#`. It is not empty, does not start with `-` and holds no control
    /// character or whitespace, so git reads it as a ref and never as an option.
    pub requested_ref: Option<String>,
}

/// What a typed source is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Requested {
    /// A directory, read in place. The text is taken whole, so a `#` in it is part of the name.
    Local(PathBuf),
    Git(GitSource),
}

/// Why typed text is not a source. No host is asked about and nothing is fetched for any of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotASource {
    /// A URL whose path ends as an archive does.
    Archive,
    /// `@scope/name` or `npm:name`.
    NpmPackage,
    /// A web address that is not a git URL: an `http` URL, or an `https` URL whose path does not
    /// end in `.git`.
    PlainUrl,
    /// A `file://` URL. A local directory is typed as its path.
    FileUrl,
    /// A scheme other than `https` and `ssh`, such as `git://`, `ftp://` or `git+https://`.
    UnsupportedScheme,
    /// A URL with no usable host or path, with userinfo that could hold a secret or an option, or
    /// holding a control character or whitespace.
    BadUrl,
    /// A ref that is empty, starts with `-` or holds a control character or whitespace.
    BadRef,
}

/// Archive endings, compared with the path of a URL in lower case.
const ARCHIVES: [&str; 9] = [
    ".tar", ".tar.gz", ".tgz", ".tar.bz2", ".tbz2", ".tar.xz", ".txz", ".zip", ".gz",
];

/// Read `text`, what the person typed after `/plugin add`, as a source.
///
/// Only the text is read. Anything that is not a URL and is not `user@host:path` is a local path,
/// including a bare word, so `demo` names the directory `demo`. `@scope/name` and `npm:name` are the
/// only spellings read as an npm package, since a local directory is never typed that way.
pub fn classify(text: &str) -> Result<Requested, NotASource> {
    let text = text.trim();
    let (location, fragment) = match text.split_once('#') {
        Some((location, fragment)) => (location, Some(fragment)),
        None => (text, None),
    };
    if let Some((scheme, rest)) = location.split_once("://")
        && is_scheme(scheme)
    {
        return classify_url(location, scheme, rest, fragment);
    }
    if text.starts_with("npm:") || (text.starts_with('@') && text.contains('/')) {
        return Err(NotASource::NpmPackage);
    }
    if let Some(parsed) = scp_like(location) {
        let host = parsed?;
        return git(location, host, fragment);
    }
    Ok(Requested::Local(PathBuf::from(text)))
}

fn is_scheme(text: &str) -> bool {
    let mut chars = text.chars();
    chars
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}

fn classify_url(
    location: &str,
    scheme: &str,
    rest: &str,
    fragment: Option<&str>,
) -> Result<Requested, NotASource> {
    let scheme = scheme.to_ascii_lowercase();
    if scheme == "file" {
        return Err(NotASource::FileUrl);
    }
    let authority_end = rest.find(['/', '?']).unwrap_or(rest.len());
    let (authority, path) = rest.split_at(authority_end);
    let path_only = path
        .split('?')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    if ARCHIVES.iter().any(|ending| path_only.ends_with(ending)) {
        return Err(NotASource::Archive);
    }
    match scheme.as_str() {
        "https" | "ssh" => {}
        "http" => return Err(NotASource::PlainUrl),
        _ => return Err(NotASource::UnsupportedScheme),
    }
    if bad_text(location) || location.starts_with('-') {
        return Err(NotASource::BadUrl);
    }
    let (user, host) = authority_parts(authority).ok_or(NotASource::BadUrl)?;
    if scheme == "https" {
        let lowered = path.to_ascii_lowercase();
        if lowered
            .strip_suffix('/')
            .unwrap_or(&lowered)
            .ends_with(".git")
        {
            // A token or password in the URL would be written to the record and drawn in the
            // prompt, so credentials come from git's own helpers.
            if user.is_some() {
                return Err(NotASource::BadUrl);
            }
        } else {
            return Err(NotASource::PlainUrl);
        }
    } else if user
        .is_some_and(|user| user.is_empty() || user.contains(':') || user.starts_with('-'))
        || path.trim_start_matches('/').is_empty()
    {
        return Err(NotASource::BadUrl);
    }
    git(location, host, fragment)
}

/// `user@host:path` as a git URL. `None` where the text is not that form, `Some(Err(..))` where it
/// is and a part is unusable, and the host where it is usable.
fn scp_like(location: &str) -> Option<Result<String, NotASource>> {
    let (before, path) = location.split_once(':')?;
    if before.contains('/') || !before.contains('@') {
        return None;
    }
    if bad_text(location) || location.starts_with('-') || path.is_empty() {
        return Some(Err(NotASource::BadUrl));
    }
    Some(match authority_parts(before) {
        Some((Some(user), host)) if !user.is_empty() => Ok(host),
        _ => Err(NotASource::BadUrl),
    })
}

fn git(location: &str, host: String, fragment: Option<&str>) -> Result<Requested, NotASource> {
    let requested_ref = match fragment {
        None => None,
        Some(name) => {
            if name.is_empty() || name.starts_with('-') || bad_text(name) {
                return Err(NotASource::BadRef);
            }
            Some(name.to_string())
        }
    };
    Ok(Requested::Git(GitSource {
        url: location.to_string(),
        host,
        requested_ref,
    }))
}

fn bad_text(text: &str) -> bool {
    text.chars().any(|c| c.is_control() || c.is_whitespace())
}

/// The user and the lowercased host of `user@host:port`, or `None` where the authority holds more
/// than one `@`, no host, a host that is not letters, digits, `.`, `-` and `_` (or an IPv6
/// literal in brackets), a host starting with `-`, or a port that is not a number a port can be.
fn authority_parts(authority: &str) -> Option<(Option<&str>, String)> {
    let (user, hostport) = match authority.split_once('@') {
        Some((user, hostport)) => (Some(user), hostport),
        None => (None, authority),
    };
    if hostport.contains('@') {
        return None;
    }
    let host = match hostport.strip_prefix('[') {
        Some(rest) => {
            let (inside, after) = rest.split_once(']')?;
            let literal = !inside.is_empty()
                && inside
                    .chars()
                    .all(|c| c.is_ascii_hexdigit() || matches!(c, ':' | '.'));
            let port_ok = after.is_empty() || after.strip_prefix(':').is_some_and(is_port);
            (literal && port_ok).then_some(inside)?
        }
        None => {
            let (host, port) = match hostport.split_once(':') {
                Some((host, port)) => (host, Some(port)),
                None => (hostport, None),
            };
            let name = !host.is_empty()
                && !host.starts_with('-')
                && host
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'));
            (name && port.is_none_or(is_port)).then_some(host)?
        }
    };
    // The same reader a host question uses (FETCH-2) agrees with the one parsed here.
    let read = host_of(&format!("ssh://{hostport}"))?;
    (read == host.to_ascii_lowercase()).then_some((user, read))
}

fn is_port(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 5
        && text.bytes().all(|b| b.is_ascii_digit())
        && text.parse::<u16>().is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";

    fn git_of(text: &str) -> GitSource {
        match classify(text) {
            Ok(Requested::Git(git)) => git,
            other => panic!("{text:?} is not a git source: {other:?}"),
        }
    }

    /// PLUGIN-14: the text after the first `#` is the ref and the rest is the URL, for each form
    /// of a git URL, and a name with `-` inside it is still a ref.
    ///
    /// Rejects: a ref left inside the URL, a split at the last `#`, a dropped ref, and a ref
    /// refused for holding a `-` that is not its first character.
    #[test]
    fn a_git_url_is_split_from_its_ref_at_the_first_hash() {
        let cases = [
            ("https://example.com/a/b.git", "example.com", None),
            (
                "https://example.com/a/b.git#main",
                "example.com",
                Some("main"),
            ),
            (
                "https://Example.com:8443/b.git#v1.2.0",
                "example.com",
                Some("v1.2.0"),
            ),
            (
                "ssh://git@host.test:2222/team/b#feature-x",
                "host.test",
                Some("feature-x"),
            ),
            ("git@host.test:team/b.git#a#b", "host.test", Some("a#b")),
            ("git@host.test:team/b.git", "host.test", None),
            ("ssh://[2001:db8::1]:22/b.git", "2001:db8::1", None),
        ];
        for (text, host, requested) in cases {
            let parsed = git_of(text);
            let url = text.split('#').next().unwrap();
            assert_eq!(parsed.url, url, "{text}");
            assert_eq!(parsed.host, host, "{text}");
            assert_eq!(parsed.requested_ref.as_deref(), requested, "{text}");
        }
        let full = git_of(&format!("https://example.com/b.git#{COMMIT}"));
        assert_eq!(full.requested_ref.as_deref(), Some(COMMIT));
    }

    /// `https://host/x.git/` is the repository `https://host/x.git`, which git accepts, so one
    /// trailing slash after `.git` is a git source. A path below the repository, a second slash
    /// and a slash after a name that does not end in `.git` are not.
    ///
    /// Rejects: a classifier that refuses the slash form as a plain URL, and one that accepts any
    /// path that merely contains `.git/`.
    #[test]
    fn one_trailing_slash_after_dot_git_is_a_git_source() {
        let parsed = git_of("https://example.com/x.git/#main");
        assert_eq!(parsed.url, "https://example.com/x.git/");
        assert_eq!(parsed.requested_ref.as_deref(), Some("main"));
        for text in [
            "https://example.com/x.git//",
            "https://example.com/x.git/tree",
            "https://example.com/x/",
        ] {
            assert_eq!(classify(text), Err(NotASource::PlainUrl), "{text}");
        }
    }

    /// A ref that git could read as an option, an empty one and one holding a control character
    /// or whitespace are refused, and a ref that only holds a `-` later is not.
    ///
    /// Rejects: no check for a leading `-`, a check on the URL instead of the ref, a control
    /// character let through, and an empty ref read as no ref.
    #[test]
    fn a_ref_that_git_could_read_as_an_option_or_that_holds_a_control_character_is_refused() {
        for bad in [
            "-x",
            "--upload-pack=touch /tmp/x",
            "-",
            "",
            "a\u{7}b",
            "a\nb",
            "a\u{1b}[2J",
            "a b",
            "a\u{85}b",
        ] {
            assert_eq!(
                classify(&format!("https://example.com/b.git#{bad}")),
                Err(NotASource::BadRef),
                "{bad:?}"
            );
            assert_eq!(
                classify(&format!("git@host.test:b.git#{bad}")),
                Err(NotASource::BadRef),
                "{bad:?}"
            );
        }
        assert!(classify("https://example.com/b.git#a-b-").is_ok());
    }

    /// PLUGIN-3: an archive, an npm package and a plain URL are not sources, and each is told
    /// apart, so the notice names the right one.
    ///
    /// Rejects: any `://` read as a git URL, an archive that is checked after the `.git` suffix
    /// and so reads as a plain URL, `http` accepted, and `file://` read as a path.
    #[test]
    fn an_archive_an_npm_name_and_a_plain_url_are_refused_each_for_its_own_reason() {
        let cases = [
            ("https://example.com/x.tar.gz", NotASource::Archive),
            ("https://example.com/x.tgz#main", NotASource::Archive),
            ("https://example.com/x.ZIP", NotASource::Archive),
            ("https://example.com/x.zip?download=1", NotASource::Archive),
            ("ssh://git@example.com/x.tar.xz", NotASource::Archive),
            ("@scope/pkg", NotASource::NpmPackage),
            ("@scope/pkg@1.2.3", NotASource::NpmPackage),
            ("npm:left-pad", NotASource::NpmPackage),
            ("https://example.com/docs", NotASource::PlainUrl),
            ("https://example.com", NotASource::PlainUrl),
            (
                "https://example.com/repo.git/tree/main",
                NotASource::PlainUrl,
            ),
            ("http://example.com/x.git", NotASource::PlainUrl),
            ("git+ftp://example.com/x", NotASource::UnsupportedScheme),
            (
                "git+https://example.com/x.git",
                NotASource::UnsupportedScheme,
            ),
            ("git://example.com/x.git", NotASource::UnsupportedScheme),
            ("ftp://example.com/x.git", NotASource::UnsupportedScheme),
            ("file:///tmp/plugin", NotASource::FileUrl),
            ("FILE:///tmp/plugin.git", NotASource::FileUrl),
        ];
        for (text, why) in cases {
            assert_eq!(classify(text), Err(why), "{text}");
        }
    }

    /// A directory is a source, and a word that is only a bare name is the directory of that
    /// name. A `#` in a directory name stays in it.
    ///
    /// Rejects: a bare word or a relative path refused as an npm name, a scheme-less text refused,
    /// and a ref split off a local path.
    #[test]
    fn a_local_path_is_a_source_taken_whole() {
        for text in [
            "demo",
            "./demo",
            "../demo",
            "/abs/demo",
            "a/b",
            "name with spaces",
            "src-demo#x",
            "dir/a://b",
            "C:\\plugins\\demo",
            "~/plugins/demo",
        ] {
            assert_eq!(
                classify(text),
                Ok(Requested::Local(PathBuf::from(text))),
                "{text}"
            );
        }
        assert_eq!(
            classify("  ./demo  "),
            Ok(Requested::Local(PathBuf::from("./demo")))
        );
    }

    /// The host is the one the URL's authority names. Userinfo is not the host, two `@` name no
    /// host, and a host or user that starts with `-` could be read by ssh as an option.
    ///
    /// Rejects: `example.com@evil.test` read as `example.com`, the text after the first `@` read as
    /// the host of `git@h:a@b/c`, a second `@` accepted, an option-like host or user, an empty
    /// host or path, and a secret kept in an `https` URL.
    #[test]
    fn the_host_is_the_one_connected_to_and_an_option_like_authority_is_refused() {
        assert_eq!(git_of("ssh://example.com@evil.test/x").host, "evil.test");
        assert_eq!(git_of("git@h.test:a@b/c.git").host, "h.test");
        for bad in [
            "https://a@b@c.test/x.git",
            "https://user:token@example.com/x.git",
            "https://user@example.com/x.git",
            "https:///x.git",
            "https://:443/x.git",
            "https://exa mple.com/x.git",
            "https://example.com:port/x.git",
            "https://example.com:99999/x.git",
            "ssh://-oProxyCommand=x/b",
            "ssh://-x@example.com/b",
            "ssh://user:secret@example.com/b",
            "ssh://example.com",
            "ssh://example.com/",
            "ssh://exa_mple.com:/b",
            "-oProxyCommand=x@h.test:b",
            "-x@h.test:b",
            "a@-h.test:b",
            "a@b@h.test:b",
            "@h.test:b",
            "git@h.test:",
            "git@h t.test:b",
            "git@[::1:b",
        ] {
            assert_eq!(classify(bad), Err(NotASource::BadUrl), "{bad}");
        }
    }

    /// A control character or whitespace in the URL is refused, and a commit is carried only as
    /// 40 lowercase hex digits.
    ///
    /// Rejects: a newline or escape kept in a URL that is then written to the record, and a
    /// commit of another length or with a letter past `f` or an uppercase digit.
    #[test]
    fn a_url_with_a_control_character_is_refused_and_a_commit_is_forty_lowercase_hex_digits() {
        for bad in [
            "https://example.com/a\u{7}.git",
            "https://example.com/a\n.git",
            "git@h.test:a\u{1b}b",
            "ssh://h.test/a\tb",
        ] {
            assert_eq!(classify(bad), Err(NotASource::BadUrl), "{bad:?}");
        }
        assert_eq!(
            Commit::parse(COMMIT).map(|c| c.to_string()),
            Some(COMMIT.to_string())
        );
        for bad in [
            "",
            "main",
            &COMMIT[..39],
            &format!("{COMMIT}0"),
            &COMMIT.to_uppercase(),
            &format!("{}g", &COMMIT[..39]),
            &format!(" {}", &COMMIT[..39]),
        ] {
            assert_eq!(Commit::parse(bad), None, "{bad:?}");
        }
    }

    /// Two sources are the same thing to install from when the URL and the requested ref agree,
    /// whatever commit each resolved to.
    ///
    /// Rejects: a comparison that includes the commit (a moved branch reads as another source), one
    /// that ignores the ref, and a local directory equal to a URL of the same text.
    #[test]
    fn the_same_url_and_ref_is_one_source_whatever_commit_it_resolved_to() {
        let at = |url: &str, requested: Option<&str>, commit: &str| Source::Git {
            url: url.to_string(),
            requested_ref: requested.map(str::to_string),
            commit: Commit::parse(commit).unwrap(),
        };
        let other = "f".repeat(40);
        let base = at("https://h/x.git", Some("main"), COMMIT);
        assert!(base.same_origin(&at("https://h/x.git", Some("main"), &other)));
        assert!(!base.same_origin(&at("https://h/x.git", Some("dev"), COMMIT)));
        assert!(!base.same_origin(&at("https://h/x.git", None, COMMIT)));
        assert!(!base.same_origin(&at("https://h/y.git", Some("main"), COMMIT)));
        assert!(!base.same_origin(&Source::Local(PathBuf::from("https://h/x.git"))));
        assert!(Source::Local("/a".into()).same_origin(&Source::Local("/a".into())));
    }
}
