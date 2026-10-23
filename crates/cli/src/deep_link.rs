//! `bravebot --handle-uri <link>`: the program a `bravebot://open` link starts (LINK-1).
//!
//! The link comes from outside the machine: any page can offer one. So the reading here is strict
//! and has no way of being asked for more than a directory and some words. What the words are is
//! never looked at: they are carried to the box as a paste is, and the only decisions made here are
//! about the link's own shape and about which directory the session may start in.

use bravebot_i18n::t;
use std::path::{Path, PathBuf};

/// The most of a link read. An operating system hands a handler what the page gave it, and a link
/// longer than the least any of them passes along has been cut somewhere already.
pub(crate) const MAX_BYTES: usize = 8 * 1024;

/// What a link asked for.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Link {
    /// The directory it named, which is a request and not yet where the session starts.
    pub(crate) dir: Option<PathBuf>,
    /// The words for the box, as the link wrote them.
    pub(crate) prompt: Option<String>,
}

/// Why a link is not read at all.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Refused {
    /// Not `bravebot://open`, with nothing but a query after it.
    NotALink,
    /// Longer than [`MAX_BYTES`].
    TooLong,
    /// A parameter other than `dir` and `prompt`, or one given twice. Not named in the message: the
    /// name is the link's own text.
    Parameter,
    /// A percent escape that is not two hex digits, bytes that are not UTF-8, or a NUL.
    Encoding,
    /// A `dir` that is not an absolute path.
    Directory,
}

impl Refused {
    pub(crate) fn said(&self) -> String {
        match self {
            Refused::NotALink => t!(deep_link_not_a_link).to_string(),
            Refused::TooLong => t!(deep_link_too_long, limit = MAX_BYTES).to_string(),
            Refused::Parameter => t!(deep_link_parameter).to_string(),
            Refused::Encoding => t!(deep_link_encoding).to_string(),
            Refused::Directory => t!(deep_link_directory).to_string(),
        }
    }
}

/// The link `text` is, or why it is none.
///
/// Only `dir` and `prompt` are read. Anything else is refused rather than ignored, so a link that
/// asked for a mode, a tool or `--dangerously-skip-permissions` starts nothing at all instead of a
/// session that quietly dropped the part that mattered.
pub(crate) fn parse(text: &str) -> Result<Link, Refused> {
    if text.len() > MAX_BYTES {
        return Err(Refused::TooLong);
    }
    let rest = strip_prefix_ignoring_case(text, "bravebot://open").ok_or(Refused::NotALink)?;
    let rest = rest.strip_prefix('/').unwrap_or(rest);
    let query = match rest {
        "" => "",
        _ => rest.strip_prefix('?').ok_or(Refused::NotALink)?,
    };
    let mut link = Link {
        dir: None,
        prompt: None,
    };
    for pair in query.split('&').filter(|pair| !pair.is_empty()) {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        match key {
            "dir" if link.dir.is_none() => {
                let dir = PathBuf::from(decode(value)?);
                if !dir.is_absolute() {
                    return Err(Refused::Directory);
                }
                link.dir = Some(dir);
            }
            "prompt" if link.prompt.is_none() => link.prompt = Some(decode(value)?),
            _ => return Err(Refused::Parameter),
        }
    }
    Ok(link)
}

fn strip_prefix_ignoring_case<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    let head = text.get(..prefix.len())?;
    head.eq_ignore_ascii_case(prefix)
        .then(|| &text[prefix.len()..])
}

/// `value` with its percent escapes resolved, `+` read as a space as a form writes it.
fn decode(value: &str) -> Result<String, Refused> {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' => {
                let pair = bytes.get(index + 1..index + 3).ok_or(Refused::Encoding)?;
                // Checked here, since `from_str_radix` takes a leading `+` and `%+1` would be a byte.
                if !pair.iter().all(u8::is_ascii_hexdigit) {
                    return Err(Refused::Encoding);
                }
                let pair = std::str::from_utf8(pair).map_err(|_| Refused::Encoding)?;
                out.push(u8::from_str_radix(pair, 16).map_err(|_| Refused::Encoding)?);
                index += 3;
            }
            b'+' => {
                out.push(b' ');
                index += 1;
            }
            byte => {
                out.push(byte);
                index += 1;
            }
        }
    }
    if out.contains(&0) {
        return Err(Refused::Encoding);
    }
    String::from_utf8(out).map_err(|_| Refused::Encoding)
}

/// Where the session starts, and the sentence that says so where it is not where the link asked.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Placement {
    pub(crate) start_in: Option<PathBuf>,
    pub(crate) note: Option<String>,
}

/// Where a session opened by `asked` starts (LINK-3).
///
/// In the directory the link named only where `listed` says a person already vouched for it, which
/// is the answer a session there would have kept ([`bravebot_agent::trusted::honoured`]) and not
/// something the link can supply. Otherwise the person's home directory, with a note, since a link
/// that named a place nobody trusted would otherwise open a session in the one directory the page
/// chose. A link naming no directory starts in the home directory and says nothing, since a
/// handler run from a shell would otherwise load the settings and instructions of wherever it was.
pub(crate) fn place(
    asked: Option<&Path>,
    home: Option<&Path>,
    listed: impl FnOnce(&Path) -> bool,
) -> Placement {
    let Some(asked) = asked else {
        return Placement {
            start_in: home.map(Path::to_path_buf),
            note: None,
        };
    };
    if asked.is_dir() && listed(asked) {
        return Placement {
            start_in: Some(asked.to_path_buf()),
            note: None,
        };
    }
    Placement {
        start_in: home.map(Path::to_path_buf),
        // Debug-escaped, since the path is the link's own text and may hold a control character.
        note: Some(t!(deep_link_started_in_home, dir = format!("{asked:?}")).to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prompt(text: &str) -> String {
        parse(text).unwrap().prompt.unwrap()
    }

    /// LINK-1: the directory and the prompt come out as the link wrote them, decoded.
    #[test]
    fn a_link_gives_its_directory_and_its_prompt_decoded() {
        let link =
            parse("bravebot://open?dir=%2Fsrv%2Fmy%20app&prompt=fix+the+test%0Aplease").unwrap();
        assert_eq!(link.dir, Some(PathBuf::from("/srv/my app")));
        assert_eq!(link.prompt.as_deref(), Some("fix the test\nplease"));
        assert_eq!(
            parse("BraveBot://Open/").unwrap(),
            Link {
                dir: None,
                prompt: None
            }
        );
        assert_eq!(prompt("bravebot://open?prompt=%E2%9C%93"), "✓");
    }

    /// LINK-2: a parameter that is not `dir` or `prompt`, or one given twice, refuses the whole link,
    /// so a mode, a tool or the flag that skips permissions cannot ride along and be dropped.
    #[test]
    fn a_link_asking_for_anything_else_is_refused_whole() {
        for text in [
            "bravebot://open?prompt=hi&mode=bypass",
            "bravebot://open?dangerously-skip-permissions=1",
            "bravebot://open?tool=run&prompt=hi",
            "bravebot://open?prompt=hi&prompt=again",
            "bravebot://open?dir=/a&dir=/b",
            "bravebot://open?%70rompt=hi",
            "bravebot://open?model=x",
        ] {
            assert_eq!(parse(text), Err(Refused::Parameter), "{text}");
        }
    }

    /// LINK-1: only the one address is a link.
    #[test]
    fn only_bravebot_open_is_a_link() {
        for text in [
            "https://open?prompt=hi",
            "bravebot://opens?prompt=hi",
            "bravebot://open/extra?prompt=hi",
            "bravebot://open#prompt=hi",
            "bravebot:open?prompt=hi",
            "",
            "-p",
        ] {
            assert_eq!(parse(text), Err(Refused::NotALink), "{text}");
        }
    }

    /// LINK-1: bad escapes, bytes that are not text and a NUL are refused, and so is a length past
    /// the cap.
    #[test]
    fn a_link_that_does_not_decode_to_text_is_refused() {
        for text in [
            "bravebot://open?prompt=%",
            "bravebot://open?prompt=%4",
            "bravebot://open?prompt=%zz",
            "bravebot://open?prompt=%+1",
            "bravebot://open?prompt=%-1",
            "bravebot://open?prompt=%ff",
            "bravebot://open?prompt=a%00b",
        ] {
            assert_eq!(parse(text), Err(Refused::Encoding), "{text}");
        }
        let long = format!("bravebot://open?prompt={}", "a".repeat(MAX_BYTES));
        assert_eq!(parse(&long), Err(Refused::TooLong));
        let at_the_cap = format!("bravebot://open?prompt={}", "a".repeat(MAX_BYTES - 23));
        assert!(parse(&at_the_cap).is_ok());
    }

    /// LINK-3: a relative directory names nothing the link could know about, so it is refused.
    #[test]
    fn a_relative_directory_is_refused() {
        for text in [
            "bravebot://open?dir=src",
            "bravebot://open?dir=",
            "bravebot://open?dir=..%2F..",
        ] {
            assert_eq!(parse(text), Err(Refused::Directory), "{text}");
        }
    }

    /// LINK-3: the link cannot make a directory trusted. The only thing that puts it in the
    /// session's way is an answer a person already gave, asked here through `listed`.
    #[test]
    fn a_directory_nobody_vouched_for_starts_in_home_and_says_so() {
        let home = Path::new("/home/person");
        let here = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let listed = place(Some(&here), Some(home), |_| true);
        assert_eq!(listed.start_in.as_deref(), Some(here.as_path()));
        assert_eq!(listed.note, None);

        assert_eq!(
            place(None, Some(home), |_| false),
            Placement {
                start_in: Some(home.to_path_buf()),
                note: None
            },
            "a link naming no directory did not start in home"
        );

        let unlisted = place(Some(&here), Some(home), |_| false);
        assert_eq!(unlisted.start_in.as_deref(), Some(home));
        assert!(unlisted.note.is_some(), "the move to home went unsaid");

        let missing = Path::new("/no/such/directory/anywhere");
        let gone = place(Some(missing), Some(home), |_| true);
        assert_eq!(
            gone.start_in.as_deref(),
            Some(home),
            "a directory that is not there was entered"
        );
        assert!(gone.note.is_some());
    }

    /// LINK-3: a path with a control character is said escaped, not written to the transcript raw.
    #[test]
    fn the_note_does_not_carry_a_control_character_raw() {
        let said = place(Some(Path::new("/a\u{1b}[2Jb")), None, |_| false)
            .note
            .unwrap();
        assert!(!said.contains('\u{1b}'), "{said:?}");
    }
}
