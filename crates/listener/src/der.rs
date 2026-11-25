// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! The few DER shapes the listener writes and reads.
//!
//! The host writes its own self-signed certificate, and a device's certificate is read for the one
//! thing the host names a device by: its public key. Both are short, fixed shapes, so this is a
//! tag-length-value writer and a reader that returns `None` for anything it cannot place, and never
//! indexes past the bytes it was given. What a certificate means is the TLS library's to decide.

/// The tag of a SEQUENCE.
pub const SEQUENCE: u8 = 0x30;

/// A tag, a length and the content, with the length written the shortest way DER allows.
///
/// A content of more than 65,535 bytes is not written: no certificate this crate makes is near it,
/// and the empty result is not a certificate anything accepts.
pub fn tlv(tag: u8, content: &[u8]) -> Vec<u8> {
    let mut out = vec![tag];
    let length = content.len();
    if let Ok(short) = u8::try_from(length) {
        if short < 0x80 {
            out.push(short);
        } else {
            out.extend([0x81, short]);
        }
    } else if let Ok(long) = u16::try_from(length) {
        out.push(0x82);
        out.extend(long.to_be_bytes());
    } else {
        return Vec::new();
    }
    out.extend(content);
    out
}

/// A SEQUENCE of the parts written one after another.
pub fn sequence(parts: &[Vec<u8>]) -> Vec<u8> {
    tlv(SEQUENCE, &parts.concat())
}

/// One element at the start of `data`: its tag, its content, and the bytes after it. `None` where
/// the length is not a form this reads, or runs past the end.
pub fn read(data: &[u8]) -> Option<(u8, &[u8], &[u8])> {
    let (&tag, after_tag) = data.split_first()?;
    let (&first, after_first) = after_tag.split_first()?;
    let (length, body) = if first < 0x80 {
        (usize::from(first), after_first)
    } else if first == 0x81 {
        let (&length, body) = after_first.split_first()?;
        (usize::from(length), body)
    } else if first == 0x82 {
        let (length, body) = after_first.split_at_checked(2)?;
        (
            usize::from(u16::from_be_bytes([length[0], length[1]])),
            body,
        )
    } else {
        return None;
    };
    let (content, rest) = body.split_at_checked(length)?;
    Some((tag, content, rest))
}

/// The bytes of the first element at the start of `data`, tag and length included.
pub fn whole_element(data: &[u8]) -> Option<&[u8]> {
    let (_, _, rest) = read(data)?;
    data.get(..data.len() - rest.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What is written is read back as written, at each of the three length forms.
    #[test]
    fn an_element_is_read_back_at_each_length_form() {
        for length in [0, 1, 127, 128, 255, 256, 65_535] {
            let content = vec![7u8; length];
            let written = tlv(0x04, &content);
            let (tag, read_back, rest) = read(&written).unwrap();
            assert_eq!(tag, 0x04, "length {length}");
            assert_eq!(read_back, content.as_slice(), "length {length}");
            assert!(rest.is_empty(), "length {length}");
        }
    }

    /// A content past what the encoder writes gives nothing, so it cannot be mistaken for a
    /// certificate.
    #[test]
    fn a_content_too_long_to_write_gives_nothing() {
        assert!(tlv(0x04, &vec![0; 65_536]).is_empty());
    }

    /// Every way the bytes can end early, and the forms this does not read, give `None` and no panic.
    #[test]
    fn bytes_that_do_not_hold_an_element_are_not_read() {
        let written = tlv(0x04, &[1, 2, 3, 4, 5]);
        for end in 0..written.len() {
            assert!(read(&written[..end]).is_none(), "cut at {end}");
        }
        // An indefinite length, and a length of three bytes.
        assert!(read(&[0x30, 0x80, 0x00, 0x00]).is_none());
        assert!(read(&[0x30, 0x83, 0x00, 0x00, 0x01, 0xff]).is_none());
        // A length that says more than the bytes hold.
        assert!(read(&[0x04, 0x05, 1, 2]).is_none());
        assert!(read(&[]).is_none());
    }

    #[test]
    fn the_whole_of_an_element_is_its_tag_length_and_content_and_nothing_after() {
        let mut bytes = tlv(0x04, &[9; 200]);
        let element = bytes.clone();
        bytes.extend([0xde, 0xad]);
        assert_eq!(whole_element(&bytes), Some(element.as_slice()));
    }
}
