//! One spelling for a path below a root, whatever the host separates with.
//!
//! A permission pattern is matched segment by segment against a name split on `/`
//! ([`crate::permissions`]), and every key the trust map holds is `/`-spelled ([`crate::trust`]).
//! A host that separates with something else hands a name back spelled its own way, and split on
//! `/` alone such a name is a single opaque segment: a rule about a directory reaches nothing under
//! it, and a rule about a bare name reaches nothing below the top. Respelling it first is what makes
//! a rule apply to the path it was written for.
//!
//! Whether a backslash separates is the host's answer, supplied by the caller. This crate has no
//! filesystem and does not ask the host anything, and the answer cannot be read off the string
//! either: where paths are spelled from `/` alone a backslash is a legal filename byte, so
//! `C:\notes` there is one file at the top of the project rather than a file called `notes` below a
//! directory called `C:`.

use std::borrow::Cow;

/// `path` with `/` between its segments, for a name below a root.
///
/// A name that carries a root of its own is left exactly as it arrived, because this changes the
/// spelling of a name and not which namespace it is in: a root is a slash here
/// ([`crate::trust::is_absolute_key`]), a drive letter is not one, and a name respelled into
/// several segments while still reading as relative would be matched against the rules written
/// about the workspace, so a pattern anchored at the project would reach a file outside it. Leaving
/// it whole keeps that name exactly as opaque as it was, which is the Windows gap
/// [issue #842](https://github.com/brave/bravebot/issues/842) is about and not this function's to
/// close.
///
/// Borrowed where there is nothing to change, which is every call on a host that separates with a
/// slash alone and most calls on one that does not.
pub fn to_slash(path: &str, backslash_separates: bool) -> Cow<'_, str> {
    match backslash_separates && path.contains('\\') && !carries_a_root(path) {
        true => Cow::Owned(path.replace('\\', "/")),
        false => Cow::Borrowed(path),
    }
}

/// Whether `path` begins with a root there is no `/`-spelling for: a drive letter, a share, or the
/// root of whichever drive the process is on.
///
/// Asked only of a host that separates with a backslash, so on every other one a name shaped like
/// this is an ordinary filename and never reaches here.
fn carries_a_root(path: &str) -> bool {
    let bytes = path.as_bytes();
    let drive = bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':';
    bytes.first() == Some(&b'\\') || drive
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A name a host separated its own way has to reach the rules written about the path it names,
    /// and every one of those is matched on `/`.
    #[test]
    fn a_name_below_a_root_is_respelled_where_a_backslash_separates() {
        assert_eq!(to_slash("src\\main.rs", true), "src/main.rs");
        assert_eq!(to_slash("a\\b\\c.txt", true), "a/b/c.txt");
        assert_eq!(to_slash("src/main.rs", true), "src/main.rs");
    }

    /// Where a slash is the only separator a backslash is a legal filename byte, so respelling one
    /// would put a file at the top of the project under a rule written about a directory nobody
    /// has, and a rule that grants would grant further than it was written for.
    #[test]
    fn a_name_holding_a_backslash_is_left_alone_where_a_slash_is_the_only_separator() {
        assert_eq!(to_slash("src\\main.rs", false), "src\\main.rs");
        assert_eq!(to_slash("C:\\notes", false), "C:\\notes");
        assert_eq!(to_slash("src/main.rs", false), "src/main.rs");
    }

    /// A name carrying a root is in the namespace of names read under the working directory, since
    /// a root here is a leading slash and none of these has one. Cutting it into segments would
    /// leave it there and match it against the patterns written about the project, so a rule
    /// anchored at the workspace would reach a file that is not in the workspace.
    #[test]
    fn a_name_carrying_a_root_of_its_own_is_left_whole() {
        for named in [
            "C:\\Users\\someone\\Desktop\\.env",
            "c:\\notes\\secret",
            "\\\\server\\share\\.env",
            "\\Users\\someone\\.env",
        ] {
            assert_eq!(
                to_slash(named, true),
                named,
                "'{named}' was cut into segments and left reading as a relative name"
            );
        }
    }
}
