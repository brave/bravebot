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
use std::path::{Component, Path};

use crate::trust::canonical_key;

/// `path` with `/` between its segments, for a name below a root.
///
/// A name that carries a root of its own is left exactly as it arrived, because this changes the
/// spelling of a name and not which namespace it is in: a root is a slash here
/// ([`crate::trust::is_absolute_key`]), a drive letter is not one, and a name respelled into
/// several segments while still reading as relative would be matched against the rules written
/// about the workspace, so a pattern anchored at the project would reach a file outside it. Leaving
/// it whole keeps that name exactly as opaque as it was. A path a permission rule is asked about
/// and a trust key are spelled by [`to_key`] instead, which gives a drive-letter name the root
/// both read it under (PERM-3, TRUST-18).
///
/// Borrowed where there is nothing to change, which is every call on a host that separates with a
/// slash alone and most calls on one that does not.
pub fn to_slash(path: &str, backslash_separates: bool) -> Cow<'_, str> {
    match backslash_separates && path.contains('\\') && !carries_a_root(path) {
        true => Cow::Owned(path.replace('\\', "/")),
        false => Cow::Borrowed(path),
    }
}

/// `path` spelled the way the trust map holds a key and a permission rule reads a path, for a name
/// that may carry a root.
///
/// Where a backslash separates, a name rooted at a drive letter is spelled from `/` with the drive
/// as its first segment, so `C:\work` and `\\?\C:\work`, the form the platform canonicalises it
/// to, are both `/C:/work`. The map reads a name without a leading `/` under the working directory
/// ([`crate::trust::is_absolute_key`]), so a drive-letter name left as it arrived is decided by the
/// rule the answer about the project wrote, wherever on the disk it points (TRUST-18). A
/// permission rule reads the same line, so left as it arrived such a name is out of reach of a
/// rule written about a full path (PERM-3).
///
/// Every other name is spelled as [`to_slash`] spells it, which leaves a share, a device path and a
/// name starting at `\` whole, since none of those has a `/`-spelling. `C:work` is not rooted
/// either: it names wherever the process last was on that drive. A key comes back as it went in.
pub fn to_key(path: &str, backslash_separates: bool) -> Cow<'_, str> {
    match drive_rooted(path).filter(|_| backslash_separates) {
        Some(from_the_drive) => Cow::Owned(format!("/{}", from_the_drive.replace('\\', "/"))),
        None => to_slash(path, backslash_separates),
    }
}

/// What `name` is below `root`, or `None` where it is not at or below it.
///
/// Compared as [`Path`] components first, which is how the two read where both are spelled alike.
/// Where they are not, both are compared as keys ([`to_key`]), so a `root` the platform resolved to
/// `\\?\C:\proj` holds the name `C:\proj\x` a person typed: `Path::strip_prefix` reads the two
/// first components as different prefixes and finds nothing below. The result is `x` either way,
/// and the empty string for the root itself. The drive letter is compared in upper case, as
/// [`Path`] compares it on the host that has one.
///
/// Components are compared in the form every canonically equivalent spelling of a name shares
/// ([`crate::trust::canonical_key`]), so a name spelled `cafe` and a combining accent is below a
/// root spelled with the precomposed letter, and the reverse. A volume such as APFS opens both
/// spellings as one directory, so a rule about a file under the root has to be asked about a name
/// spelled either way. Case is not folded. The result is the part of the name after the root as the
/// name spelled it, whichever comparison found the root: a doubled separator or a `.` inside it is
/// kept and a separator at either end is not part of it. A component that only starts like the
/// root's (`cafe\u{301}x` against `caf\u{e9}`) is not the root's.
///
/// A name below a root is `/`-spelled where a backslash separates, which is the spelling a key is
/// asked about the map under. Nothing is resolved here: a link is not followed and a `..` is kept,
/// so a caller that has to refuse either does so before asking.
pub fn below(root: &Path, name: &str, backslash_separates: bool) -> Option<String> {
    if let Ok(inside) = Path::new(name).strip_prefix(root) {
        return Some(to_slash(&inside.to_string_lossy(), backslash_separates).into_owned());
    }
    // A root that is not text has no key, and its lossy rendering would be another root's.
    let root = root.to_str()?;
    let drive_in_upper_case = |key: Cow<'_, str>| -> String {
        let key = key.into_owned();
        let at_a_drive = backslash_separates
            && matches!(key.as_bytes(), [b'/', letter, b':', b'/', ..] | [b'/', letter, b':'] if letter.is_ascii_alphabetic());
        match at_a_drive {
            true => key
                .chars()
                .enumerate()
                .map(|(at, c)| if at == 1 { c.to_ascii_uppercase() } else { c })
                .collect(),
            false => key,
        }
    };
    let name = drive_in_upper_case(to_key(name, backslash_separates));
    let root = drive_in_upper_case(to_key(root, backslash_separates));
    // The remainder is the rest of the name as it was spelled, found by counting components, so no
    // byte offset in one form is applied to another: decomposing changes a name's length.
    let mut rest = Path::new(&name).components();
    for from_root in Path::new(&root).components() {
        let same = |named: Component<'_>| match (
            from_root.as_os_str().to_str(),
            named.as_os_str().to_str(),
        ) {
            (Some(root_part), Some(name_part)) => {
                canonical_key(root_part, false) == canonical_key(name_part, false)
            }
            _ => false,
        };
        if !rest.next().is_some_and(same) {
            return None;
        }
    }
    Some(rest.as_path().to_string_lossy().into_owned())
}

/// `path` from its drive letter on, for a name rooted at one, without the `\\?\` a canonical name
/// carries in front of it.
fn drive_rooted(path: &str) -> Option<&str> {
    let bare = path.strip_prefix(r"\\?\").unwrap_or(path);
    match bare.as_bytes() {
        [letter, b':', b'\\' | b'/', ..] if letter.is_ascii_alphabetic() => Some(bare),
        _ => None,
    }
}

/// Whether `path` begins with a root this does not respell: a drive letter, a share, or the
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

    /// A drive letter is a root on the host that has one, so a key about a file on it has to read
    /// as a full path, or the rule the answer about the project wrote decides a file anywhere on
    /// the disk. The canonical spelling and the one a person types are one key.
    #[test]
    fn a_name_rooted_at_a_drive_letter_is_keyed_from_slash_where_a_backslash_separates() {
        assert_eq!(to_key(r"\\?\C:\other", true), "/C:/other");
        assert_eq!(to_key(r"C:\other\notes.md", true), "/C:/other/notes.md");
        assert_eq!(to_key("C:/other", true), "/C:/other");
        assert_eq!(
            to_key("/C:/other", true),
            "/C:/other",
            "a key was respelled again"
        );
        assert_eq!(to_key(r"src\main.rs", true), "src/main.rs");
    }

    /// A share, a device path, a name from the current drive's root and one relative to wherever
    /// the process last was on a drive have no `/`-spelling, so none is given one: the workspace
    /// refuses to open a directory named that way rather than key it inside the project.
    #[test]
    fn a_name_with_no_slash_spelling_is_not_keyed_as_a_full_path() {
        for named in [
            r"\\server\share\x",
            r"\\?\UNC\server\share\x",
            r"\\.\C:\other",
            r"\other",
            r"C:other",
        ] {
            assert!(
                !to_key(named, true).starts_with('/'),
                "'{named}' was keyed as a full path"
            );
        }
    }

    /// Where a slash is the only separator `C:\notes` is a file at the top of the project, and
    /// keying it as a drive would take it out from under the project's rules.
    #[test]
    fn a_name_shaped_like_a_drive_is_a_file_where_a_slash_is_the_only_separator() {
        assert_eq!(to_key(r"C:\other", false), r"C:\other");
        assert_eq!(to_key(r"\\?\C:\other", false), r"\\?\C:\other");
    }

    /// The platform resolves a root to `\\?\C:\proj` and a person or a program names a file in it
    /// as `C:\proj\x`. `Path` reads the two first components as different prefixes, so a
    /// comparison by components finds nothing below the root and a rule about the file is never
    /// asked. Every spelling of the drive path is below the root, and the names that only look
    /// like it (`proj-other`, another drive, the drive root) are not.
    #[test]
    fn a_name_is_below_a_verbatim_root_whichever_way_the_drive_path_is_spelled() {
        let root = Path::new(r"\\?\C:\proj");
        for (named, below_it) in [
            (r"C:\proj\src\a.rs", Some("src/a.rs")),
            ("C:/proj/src/a.rs", Some("src/a.rs")),
            (r"c:\proj\src\a.rs", Some("src/a.rs")),
            (r"\\?\C:\proj\src\a.rs", Some("src/a.rs")),
            (r"C:\proj", Some("")),
            (r"C:\proj-other\a.rs", None),
            (r"C:\other\a.rs", None),
            (r"D:\proj\a.rs", None),
            (r"C:\", None),
        ] {
            assert_eq!(
                below(root, named, true).as_deref(),
                below_it,
                "'{named}' under '{}'",
                root.display()
            );
        }
    }

    /// Where a slash is the only separator a backslash is a filename byte, so `C:\proj\a.rs` is one
    /// file at the top of the project and is not below a root spelled `\\?\C:\proj`. A name spelled
    /// like its root is below it on either kind of host.
    #[test]
    fn a_drive_spelling_is_not_below_a_root_where_a_slash_is_the_only_separator() {
        assert_eq!(
            below(Path::new(r"\\?\C:\proj"), r"C:\proj\a.rs", false),
            None
        );
        for backslash_separates in [false, true] {
            let root = Path::new("/work");
            assert_eq!(
                below(root, "/work/src/a.rs", backslash_separates).as_deref(),
                Some("src/a.rs")
            );
            assert_eq!(
                below(root, "/work", backslash_separates).as_deref(),
                Some("")
            );
            assert_eq!(below(root, "/workshop/a.rs", backslash_separates), None);
        }
    }

    /// A volume such as APFS opens a directory under any canonically equivalent spelling of its
    /// name, so a name below the root is below it whichever form either is spelled in. The part
    /// after the root is the name's own spelling, and a name that only starts like the root's last
    /// component is not below it.
    #[test]
    fn a_name_is_below_a_root_spelled_in_another_unicode_form() {
        let decomposed = "/work/cafe\u{301}";
        let composed = "/work/caf\u{e9}";
        for (root, named, below_it) in [
            (
                decomposed,
                "/work/caf\u{e9}/held/src/lib.rs",
                Some("held/src/lib.rs"),
            ),
            (
                composed,
                "/work/cafe\u{301}/held/src/lib.rs",
                Some("held/src/lib.rs"),
            ),
            (
                decomposed,
                "/work/cafe\u{301}/held/src/lib.rs",
                Some("held/src/lib.rs"),
            ),
            (
                composed,
                "/work/caf\u{e9}/held/src/lib.rs",
                Some("held/src/lib.rs"),
            ),
            (decomposed, "/work/caf\u{e9}", Some("")),
            (composed, "/work/cafe\u{301}", Some("")),
            (composed, "/work/cafe\u{301}x/a.rs", None),
            (decomposed, "/work/caf\u{e9}x/a.rs", None),
            (composed, "/work/cafe/a.rs", None),
            (composed, "/other/caf\u{e9}/a.rs", None),
            (composed, "/work", None),
        ] {
            for backslash_separates in [false, true] {
                assert_eq!(
                    below(Path::new(root), named, backslash_separates).as_deref(),
                    below_it,
                    "'{named}' under '{root}'"
                );
            }
        }
        // The remainder is the name as spelled, not its decomposed form.
        assert_eq!(
            below(
                Path::new("/work/cafe\u{301}"),
                "/work/caf\u{e9}/\u{e9}t\u{e9}.rs",
                false
            )
            .as_deref(),
            Some("\u{e9}t\u{e9}.rs")
        );
    }

    /// The part after the root is the name's own spelling whichever comparison found the root, and
    /// nothing is resolved: a trailing separator is not part of it, a doubled separator or a `.` in
    /// the middle is kept, a `..` is kept, and a root with no components holds a relative name. The
    /// drive-letter rows run on any host, since the host's answer about the separator is given.
    #[test]
    fn what_is_below_the_root_is_the_name_as_spelled_whichever_comparison_found_the_root() {
        let same_form = Path::new("/work/caf\u{e9}");
        let other_form = Path::new("/work/cafe\u{301}");
        for (named, below_it) in [
            ("/work/caf\u{e9}/held/", Some("held")),
            ("/work/caf\u{e9}/a//b.rs", Some("a//b.rs")),
            ("/work/caf\u{e9}/a/./b.rs", Some("a/./b.rs")),
            ("/work/caf\u{e9}/../etc", Some("../etc")),
            ("/work/caf\u{e9}/", Some("")),
        ] {
            let other = named.replace('\u{e9}', "e\u{301}");
            for root in [same_form, other_form] {
                for (spelled, expected) in [(named, below_it), (other.as_str(), below_it)] {
                    assert_eq!(
                        below(root, spelled, false).as_deref(),
                        expected,
                        "'{spelled}' under '{}'",
                        root.display()
                    );
                }
            }
        }
        assert_eq!(
            below(Path::new(""), "src/a.rs", false).as_deref(),
            Some("src/a.rs")
        );
        assert_eq!(
            below(Path::new("/work/caf\u{e9}"), "", false),
            None,
            "an empty name is below no root"
        );

        let verbatim = Path::new("\\\\?\\C:\\proj\\caf\u{e9}");
        for (named, below_it) in [
            ("c:\\proj\\cafe\u{301}\\x.rs", Some("x.rs")),
            ("C:/proj/caf\u{e9}/a//b.rs", Some("a//b.rs")),
            ("C:\\proj\\cafe\u{301}\\", Some("")),
            ("C:\\proj\\cafe\u{301}x\\a.rs", None),
            ("D:\\proj\\cafe\u{301}\\a.rs", None),
        ] {
            assert_eq!(
                below(verbatim, named, true).as_deref(),
                below_it,
                "'{named}' under '{}'",
                verbatim.display()
            );
        }
    }
}
