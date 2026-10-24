//! The small encodings the sign-in is written in, spelled here rather than taken from a crate.
//!
//! This crate depends on no other crate here and adds none for these: base64 for the PKCE values
//! and the parts of an ID token, percent encoding for a URL and a form, a comparison that does not
//! stop at the first different byte, and a date for the one line that says when a token expires.

/// Base64 with the URL-safe alphabet and no padding, which is what PKCE and the `state` are
/// written in.
pub(crate) fn base64url(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);

    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let triple = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        // A chunk of n input bytes makes n + 1 characters, and the rest would be padding.
        for shift in [18, 12, 6, 0].into_iter().take(chunk.len() + 1) {
            out.push(ALPHABET[(triple >> shift) as usize & 0x3f] as char);
        }
    }

    out
}

/// The bytes [`base64url`] text stands for, or `None` where it is not text that function writes: a
/// character outside the alphabet, padding, a length no number of bytes encodes to, or bits after
/// the last byte that are not zero.
///
/// Strict because a token is read with it, and two spellings of one token would be two tokens a
/// signature covers one of.
pub(crate) fn base64url_decode(text: &str) -> Option<Vec<u8>> {
    fn value(character: u8) -> Option<u32> {
        match character {
            b'A'..=b'Z' => Some(u32::from(character - b'A')),
            b'a'..=b'z' => Some(u32::from(character - b'a') + 26),
            b'0'..=b'9' => Some(u32::from(character - b'0') + 52),
            b'-' => Some(62),
            b'_' => Some(63),
            _ => None,
        }
    }

    let characters = text.as_bytes();
    if characters.len() % 4 == 1 {
        return None;
    }
    let mut out = Vec::with_capacity(characters.len() / 4 * 3 + 2);
    for chunk in characters.chunks(4) {
        let mut quad = 0u32;
        for (at, &character) in chunk.iter().enumerate() {
            quad |= value(character)? << (18 - 6 * at);
        }
        // A chunk of n characters holds n - 1 bytes, and the bits after them were zero when written.
        let bytes = chunk.len() - 1;
        if quad & (0x00ff_ffff >> (8 * bytes)) != 0 {
            return None;
        }
        out.extend_from_slice(&quad.to_be_bytes()[1..=bytes]);
    }
    Some(out)
}

/// 32 bytes from the operating system's random source, encoded as [`base64url`].
///
/// The verifier, the `state` and the `nonce` are each one of these. A failure of the source is not
/// survivable and there is no weaker one to fall back to, as for [`crate::new_request_id`].
pub(crate) fn random_value() -> String {
    let mut bytes = [0u8; 32];
    crate::fill_random(&mut bytes);
    let encoded = base64url(&bytes);
    // The bytes are the value, so they are not left on the stack once it is encoded.
    bytes.fill(0);
    std::hint::black_box(&bytes);
    encoded
}

/// Percent-encode `text` for a URL query or a form body: every byte outside RFC 3986's unreserved
/// set is written as `%XX`.
pub(crate) fn percent_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for &byte in text.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// The bytes `text` stands for, or `None` where a `%` is not followed by two hexadecimal digits.
///
/// A `+` is a `+`: the values read from a callback are opaque tokens, and none of them has a space
/// in it to be written either way.
pub(crate) fn percent_decode(text: &str) -> Option<Vec<u8>> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'%' {
            let pair = bytes.get(at + 1..at + 3)?;
            // Checked before it is parsed: the parse accepts a leading `+`, which is not a digit.
            if !pair.iter().all(u8::is_ascii_hexdigit) {
                return None;
            }
            let digits = std::str::from_utf8(pair).ok()?;
            out.push(u8::from_str_radix(digits, 16).ok()?);
            at += 3;
        } else {
            out.push(bytes[at]);
            at += 1;
        }
    }
    Some(out)
}

/// Whether two values are equal, taking as long to say so whatever the first byte that differs.
///
/// Where the lengths differ the answer is no at once, which says how long the expected value is
/// and nothing about what is in it.
pub(crate) fn same(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let difference = a.iter().zip(b).fold(0u8, |seen, (x, y)| seen | (x ^ y));
    std::hint::black_box(difference) == 0
}

/// A UTC instant, as the number of seconds since 1970-01-01, written `2026-10-22T21:05:27Z`.
pub(crate) fn utc(seconds: u64) -> String {
    let days = (seconds / 86_400) as i64;
    let of_day = seconds % 86_400;
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        of_day / 3600,
        of_day % 3600 / 60,
        of_day % 60
    )
}

/// The civil date `days` after 1970-01-01: Howard Hinnant's `civil_from_days`, which counts from
/// March so the leap day is the last day of its year.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let of_era = shifted.rem_euclid(146_097);
    let year_of_era = (of_era - of_era / 1460 + of_era / 36_524 - of_era / 146_096) / 365;
    let day_of_year = of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_from_march = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * month_from_march + 2) / 5 + 1) as u32;
    let month = if month_from_march < 10 {
        month_from_march + 3
    } else {
        month_from_march - 9
    } as u32;
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

/// Seconds since 1970-01-01 now, or 0 for a clock set before it.
pub(crate) fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RFC 4648's test vectors, in the URL-safe alphabet. The two characters that differ from the
    /// standard alphabet are the ones a PKCE challenge is likely to contain, and a count of
    /// characters per chunk that is off by one would pad where there is no padding.
    #[test]
    fn base64url_writes_the_rfc_vectors_without_padding_and_in_the_url_alphabet() {
        for (bytes, expected) in [
            (&b""[..], ""),
            (b"f", "Zg"),
            (b"fo", "Zm8"),
            (b"foo", "Zm9v"),
            (b"foob", "Zm9vYg"),
            (b"fooba", "Zm9vYmE"),
            (b"foobar", "Zm9vYmFy"),
            (&[0xfb, 0xff, 0xfe][..], "-__-"),
        ] {
            assert_eq!(base64url(bytes), expected, "{bytes:?}");
        }
    }

    /// What is encoded decodes back, and text no encoding writes is refused: padding, a character of
    /// the standard alphabet, a length no number of bytes makes, and bits after the last byte that
    /// are not zero, which would let two spellings stand for one token.
    #[test]
    fn base64url_reads_back_what_it_writes_and_nothing_else() {
        for bytes in [
            &b""[..],
            b"f",
            b"fo",
            b"foo",
            b"foob",
            b"fooba",
            b"foobar",
            &[0xfb, 0xff, 0xfe][..],
            &[0u8; 33][..],
        ] {
            assert_eq!(
                base64url_decode(&base64url(bytes)).as_deref(),
                Some(bytes),
                "{bytes:?}"
            );
        }
        for text in [
            "Zg==", "Zm9v+", "Zm9/", "Z", "Zm9vY", "Zh", "Zm9", "Zm 9v", "Zm.v",
        ] {
            assert_eq!(base64url_decode(text), None, "{text}");
        }
    }

    /// The challenge of RFC 7636 appendix B, which is the one value a verifier encoded or hashed
    /// wrongly cannot produce.
    #[test]
    fn a_challenge_is_the_rfc_7636_one_for_its_verifier() {
        use sha2::{Digest, Sha256};

        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let challenge = base64url(&Sha256::digest(verifier.as_bytes()));
        assert_eq!(challenge, "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
    }

    /// Everything outside the unreserved set is encoded, including the characters that end a query
    /// value (`&`, `=`, `#`) and a space, and what is encoded decodes back.
    #[test]
    fn percent_encoding_leaves_only_the_unreserved_characters_and_round_trips() {
        let text = "a b&c=d#e/f?g%h~i-j.k_l\u{e9}";
        let encoded = percent_encode(text);
        assert_eq!(encoded, "a%20b%26c%3Dd%23e%2Ff%3Fg%25h~i-j.k_l%C3%A9");
        assert_eq!(
            percent_decode(&encoded).expect("what was encoded"),
            text.as_bytes()
        );
    }

    /// A `%` with fewer than two hexadecimal digits after it stands for nothing, and is not read as
    /// the byte its first digit would make.
    #[test]
    fn a_percent_sign_not_followed_by_two_hexadecimal_digits_is_refused() {
        for text in ["%", "%4", "%zz", "a%4", "%4g", "%+f", "%-1", "%+%+"] {
            assert_eq!(percent_decode(text), None, "{text}");
        }
        assert_eq!(percent_decode("a+b").expect("a plus"), b"a+b");
    }

    /// A value that differs in its last byte, one that is a prefix of the other and an empty one
    /// are each different, and only the same bytes are the same.
    #[test]
    fn two_values_are_the_same_only_when_every_byte_is() {
        assert!(same(b"abc", b"abc"));
        assert!(same(b"", b""));
        assert!(!same(b"abc", b"abd"));
        assert!(!same(b"abc", b"abcd"));
        assert!(!same(b"abc", b""));
    }

    /// The date is checked at the points a conversion goes wrong: the epoch, the day before a leap
    /// day and the leap day itself, the end of a year, and an instant past 2038, which a 32-bit
    /// count would not reach.
    #[test]
    fn an_instant_is_written_as_the_utc_date_and_time_it_is() {
        for (seconds, expected) in [
            (0, "1970-01-01T00:00:00Z"),
            (951_782_399, "2000-02-28T23:59:59Z"),
            (951_782_400, "2000-02-29T00:00:00Z"),
            (1_798_761_599, "2026-12-31T23:59:59Z"),
            (1_792_703_127, "2026-10-22T21:05:27Z"),
            (4_102_444_800, "2100-01-01T00:00:00Z"),
        ] {
            assert_eq!(utc(seconds), expected, "{seconds}");
        }
    }

    /// 32 random bytes encode to 43 characters, and two values are not the same: a value that was
    /// fixed would be a verifier anyone could guess.
    #[test]
    fn a_random_value_is_43_url_safe_characters_and_a_new_one_each_time() {
        let first = random_value();
        let second = random_value();
        assert_eq!(first.len(), 43);
        assert!(
            first
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        );
        assert_ne!(first, second);
    }
}
