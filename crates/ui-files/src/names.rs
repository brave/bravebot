//! Windows file names that open something other than what they spell.
//!
//! Each is refused rather than normalised, since a normalised name is one the caller did not
//! check. Compiled for tests everywhere, so the rule is exercised on every host.

/// Whether Windows would read `name` as something other than the one plain name it spells.
///
/// That is a stream (`name:stream`), a device (`CON`, `NUL.md`, `COM1`), a name Win32 strips
/// trailing dots or spaces from, a character no Windows name may hold, or an 8.3 short name,
/// which is another file's second spelling. A short name is accepted in a root: that is the
/// directory a person chose, and its short spelling names that one directory rather than reaching
/// a different one. The system temporary directory is often one (`RUNNER~1`).
pub(crate) fn misleads(name: &str, root: bool) -> bool {
    name.chars().any(|c| c < ' ' || "\\/:<>\"|?*".contains(c))
        || name.ends_with(['.', ' '])
        || device(name)
        || (!root && short(name))
}

/// A reserved device name, which Win32 reads as the device whatever extension follows it.
fn device(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or(name).trim_end_matches(' ');
    let stem = stem.to_ascii_uppercase();
    if matches!(
        stem.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
    ) {
        return true;
    }
    let Some(number) = stem
        .strip_prefix("COM")
        .or_else(|| stem.strip_prefix("LPT"))
    else {
        return false;
    };
    let mut number = number.chars();
    matches!(
        (number.next(), number.next()),
        (Some('0'..='9' | '¹' | '²' | '³'), None)
    )
}

/// The shape of an 8.3 name NTFS generates: at most eight characters ending in `~` and digits,
/// then at most three after a dot.
fn short(name: &str) -> bool {
    let (base, extension) = name.split_once('.').unwrap_or((name, ""));
    let Some((_, digits)) = base.rsplit_once('~') else {
        return false;
    };
    !digits.is_empty()
        && digits.bytes().all(|b| b.is_ascii_digit())
        && base.chars().count() <= 8
        && extension.chars().count() <= 3
        && !extension.contains('.')
}

#[cfg(test)]
mod tests {
    use super::misleads;

    #[test]
    fn a_name_windows_reads_as_another_is_refused() {
        for name in [
            "test.md:stream",
            "test.md::$DATA",
            "a\\b",
            "a/b",
            "C:",
            "what?",
            "star*",
            "<a>",
            "pipe|",
            "quote\"",
            "bell\u{7}",
            "test.md.",
            "test.md ",
            "bots.",
            "CON",
            "con",
            "CON.md",
            "con.tar.gz",
            "CON .md",
            "NUL",
            "PRN.txt",
            "AUX",
            "COM1",
            "com9.md",
            "LPT0",
            "LPT¹",
            "COM³.md",
            "CONIN$",
            "conout$.md",
            "TEST~1.md",
            "PROGRA~1",
            "BRAVEB~1",
            "AB12~123.TXT",
            "~1",
        ] {
            assert!(misleads(name, false), "{name:?} must be refused");
        }
    }

    #[test]
    fn a_plain_name_is_accepted() {
        for name in [
            "test.md",
            ".bravebot-ui",
            ".gitignore",
            "hooks.json",
            "bots",
            "CONFIG",
            "console.md",
            "COM10",
            "COM",
            "LPT",
            "LPTX.md",
            "NULL",
            "a~b.md",
            "notes~draft.md",
            "longer-name~1.md",
            "TEST~1.markdown",
            "TEST~X.md",
            "é.md",
        ] {
            assert!(!misleads(name, false), "{name:?} must be accepted");
        }
    }

    #[test]
    fn a_short_name_is_accepted_only_in_a_root() {
        assert!(misleads("RUNNER~1", false));
        assert!(!misleads("RUNNER~1", true));
        // Everything else is refused in a root all the same.
        for name in ["CON", "a:b", "trailing.", "trailing "] {
            assert!(misleads(name, true), "{name:?} must be refused in a root");
        }
    }
}
