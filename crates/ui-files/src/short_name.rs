//! The shape of a Windows 8.3 short name.
//!
//! A file of its own, included by path wherever the question is asked (`names.rs` for a file name,
//! `bravebot-core` for a program word), so the shape is written once and none of those crates
//! depends on another for it.

/// The shape of an 8.3 name NTFS generates: at most eight characters ending in `~` and digits,
/// then at most three after a dot.
pub(crate) fn short(name: &str) -> bool {
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
