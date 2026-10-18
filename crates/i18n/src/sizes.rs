//! The notes that tell a person something was too large to attach, shared by the terminal and the
//! desktop bridge so both say it in the same words (SHARE-001). Each is in the locale the process
//! chose ([`crate::locale`]).
//!
//! The messages are reached by their generated items, which is what [`crate::t!`] expands to: an
//! exported macro cannot be named by path inside the crate that generates it.

use crate::messages::{AttachmentTooLarge, Megabytes, PasteTooLarge, number_decimal_separator};

/// A byte count as a person would say it, since nobody reads seven digits off a screen.
pub fn in_megabytes(bytes: u64) -> String {
    let size =
        format!("{:.1}", bytes as f64 / (1024.0 * 1024.0)).replace('.', number_decimal_separator());
    Megabytes { size }.render()
}

/// A pasted picture of `size` bytes, refused for being over the `limit` a paste carries.
pub fn paste_too_large(size: u64, limit: u64) -> String {
    PasteTooLarge {
        size: in_megabytes(size),
        limit: in_megabytes(limit),
    }
    .render()
}

/// A dropped file called `name` of `size` bytes, refused for being over the `limit` an attachment
/// carries.
pub fn attachment_too_large(name: &str, size: u64, limit: u64) -> String {
    AttachmentTooLarge {
        name,
        size: in_megabytes(size),
        limit: in_megabytes(limit),
    }
    .render()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One decimal, so a picture just over the cap does not read as the same size as the cap.
    #[test]
    fn a_size_is_said_in_megabytes_to_one_decimal() {
        assert_eq!(in_megabytes(10 * 1024 * 1024), "10.0 MB");
        assert_eq!(in_megabytes(10 * 1024 * 1024 + 60 * 1024), "10.1 MB");
        assert_eq!(in_megabytes(0), "0.0 MB");
    }

    #[test]
    fn a_paste_too_large_says_its_size_and_the_cap() {
        assert_eq!(
            paste_too_large(20 * 1024 * 1024, 10 * 1024 * 1024),
            "that picture is 20.0 MB, and a paste carries at most 10.0 MB"
        );
    }

    #[test]
    fn an_attachment_too_large_says_its_name_its_size_and_the_cap() {
        assert_eq!(
            attachment_too_large("scan.pdf", 9 * 1024 * 1024, 8 * 1024 * 1024),
            "scan.pdf is 9.0 MB, and an attachment carries at most 8.0 MB"
        );
    }
}
