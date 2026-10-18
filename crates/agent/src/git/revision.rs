//! Revision text the planner wrote: splitting it into a ref and a path, and checking a name.

use super::{Declined, Query};

/// A revision as written: one, two for a diff, or a range.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Spec<'a> {
    One(&'a str),
    Pair(&'a str, &'a str),
    Range(&'a str, &'a str),
}

/// The one revision a query takes: a range or a pair is declined, as the query it was given to
/// says.
pub(super) fn single(query: Query, text: &str) -> Result<&str, Declined> {
    match spec(text)? {
        Spec::One(one) => Ok(one),
        Spec::Range(..) => Err(Declined::TakesOne(query, text.trim().to_owned())),
        Spec::Pair(..) => Err(Declined::PairIsForDiff(text.trim().to_owned())),
    }
}

/// Everything after the first `:` is a path, read as a name whatever it holds, so only the
/// revision before it is read for git's syntax.
pub(super) fn spec(revision: &str) -> Result<Spec<'_>, Declined> {
    let text = revision.trim_start();
    let (rev, path) = match text.split_once(':') {
        Some((rev, path)) => (rev, Some(path)),
        None => (text.trim_end(), None),
    };
    let unsupported = |what| Err(Declined::Unsupported(text.trim_end().to_owned(), what));
    for (needle, what) in [
        ("...", "a symmetric difference (A...B)"),
        ("@{", "a reflog or upstream selector (@{...})"),
        ("^{/", "a message search (^{/...})"),
        ("^@", "the parents selector (^@)"),
        ("^!", "the ^! selector"),
        ("^-", "the ^- selector"),
    ] {
        if rev.contains(needle) {
            return unsupported(what);
        }
    }
    if rev.starts_with('-') {
        return unsupported("git's options");
    }
    if rev.trim().is_empty() {
        return match path {
            Some(_) => unsupported("the index (:path) or a message search (:/...)"),
            None => Err(Declined::Unknown(revision.to_owned())),
        };
    }
    let words: Vec<&str> = rev.split_whitespace().collect();
    if path.is_some() {
        return match words.as_slice() {
            [one] if one.contains("..") => unsupported("a path inside a range"),
            [_] => Ok(Spec::One(text)),
            _ => unsupported("a path inside a pair of revisions"),
        };
    }
    match words.as_slice() {
        [one] => match one.split_once("..") {
            Some((a, b)) => Ok(Spec::Range(
                if a.is_empty() { "HEAD" } else { a },
                if b.is_empty() { "HEAD" } else { b },
            )),
            None => Ok(Spec::One(one)),
        },
        [a, b] => {
            if a.contains("..") || b.contains("..") || a.starts_with('-') || b.starts_with('-') {
                return unsupported("a range or an option inside a pair of revisions");
            }
            Ok(Spec::Pair(a, b))
        }
        _ => unsupported("more than two revisions"),
    }
}

/// The path a single revision names after its `:`, relative to the repository's root, so the
/// caller can hold it against the trust map before anything is opened.
pub fn path_in(revision: &str) -> Option<&str> {
    let (_, path) = revision.trim_start().split_once(':')?;
    Some(path)
}

/// A path relative to the repository's root, with any trailing `/` dropped, or why it is not one.
/// The root itself is the empty string.
pub(super) fn clean(path: &str) -> Result<String, Declined> {
    if path == "." {
        return Ok(String::new());
    }
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() {
        return if path.is_empty() {
            Ok(String::new())
        } else {
            Err(Declined::PathInvalid(path.to_owned()))
        };
    }
    let bad = trimmed.split('/').any(|component| {
        component.is_empty() || component == "." || component == ".." || component.contains('\0')
    });
    if bad {
        return Err(Declined::PathInvalid(path.to_owned()));
    }
    Ok(trimmed.to_owned())
}

/// Whether `name` could be a ref, by the rules git checks a ref name against. The ref store checks
/// too; this runs first so a revision's own syntax is never handed to it as a name.
///
/// A name under `worktrees/` or `main-worktree/` is refused as well: the ref store reads those
/// from another worktree's directory, which [`survey`] never lists.
pub fn plausible_ref(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with("worktrees/")
        && !name.starts_with("main-worktree/")
        && !name.starts_with(['-', '/'])
        && !name.ends_with(['/', '.'])
        && !name.to_ascii_lowercase().ends_with(".lock")
        && !name.contains("..")
        && !name.contains("@{")
        && name != "@"
        && name
            .chars()
            .all(|c| !c.is_control() && !" ~^:?*[\\".contains(c))
        && name
            .split('/')
            .all(|component| !component.is_empty() && !component.starts_with('.'))
}

pub(super) fn is_hex(word: &str) -> bool {
    !word.is_empty() && word.bytes().all(|b| b.is_ascii_hexdigit())
}
