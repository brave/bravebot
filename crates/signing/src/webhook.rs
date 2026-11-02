//! The signature on a delivery to `bravebot trigger serve` (TRIGGER-3).
//!
//! The sender signs the event name the person registered, a line feed and the body:
//! `hmac_sha256(secret, event + "\n" + body)`, written as `sha256=<lowercase hex>`. The event name
//! is part of the signed message, so a delivery signed for one event does not verify for another,
//! and nothing the sender writes in a header selects the event.
//!
//! GitHub's `X-Hub-Signature-256` signs the body alone and is not this scheme.

use hmac::{Hmac, Mac};
use sha2::Sha256;

/// What the signature value starts with.
pub const PREFIX: &str = "sha256=";

/// The hex of `hmac_sha256(key, message)`.
pub fn hmac_sha256_hex(key: &[u8], message: &[u8]) -> String {
    let mut mac = keyed(key);
    mac.update(message);
    mac.finalize()
        .into_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// An HMAC-SHA256 keyed with `key`.
#[expect(
    clippy::expect_used,
    reason = "HMAC accepts a key of any length, so new_from_slice has no error to return"
)]
fn keyed(key: &[u8]) -> Hmac<Sha256> {
    Hmac::<Sha256>::new_from_slice(key).expect("HMAC accepts keys of any length")
}

fn mac_of(secret: &str, event: &str, body: &[u8]) -> Hmac<Sha256> {
    let mut mac = keyed(secret.as_bytes());
    mac.update(event.as_bytes());
    mac.update(b"\n");
    mac.update(body);
    mac
}

/// The value a sender puts in the signature header for `event` and `body`.
pub fn signature(secret: &str, event: &str, body: &[u8]) -> String {
    let bytes = mac_of(secret, event, body).finalize().into_bytes();
    let hex: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
    format!("{PREFIX}{hex}")
}

/// Whether `presented` is the signature of `event` and `body` under `secret`. The comparison takes
/// the same time wherever the two differ.
pub fn verify(secret: &str, event: &str, body: &[u8], presented: &str) -> bool {
    let Some(hex) = presented.strip_prefix(PREFIX) else {
        return false;
    };
    let Some(bytes) = decode_hex(hex) else {
        return false;
    };
    mac_of(secret, event, body).verify_slice(&bytes).is_ok()
}

fn decode_hex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) || !text.is_ascii() {
        return None;
    }
    text.as_bytes()
        .chunks(2)
        .map(|pair| {
            let high = (pair[0] as char).to_digit(16)?;
            let low = (pair[1] as char).to_digit(16)?;
            u8::try_from(high * 16 + low).ok()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RFC 4231 test case 2, so the primitive is pinned against a published vector and not
    /// against itself.
    #[test]
    fn hmac_matches_the_rfc_4231_vector() {
        assert_eq!(
            hmac_sha256_hex(b"Jefe", b"what do ya want for nothing?"),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
    }

    /// The message is the event, a line feed, then the body: the signature equals the primitive
    /// over that text. The fault it rejects is a message that leaves the event out, which would let
    /// a delivery signed for one event pass as another.
    #[test]
    fn the_signature_covers_the_event_a_line_feed_and_the_body() {
        assert_eq!(
            signature("k", "push", b"{}"),
            format!("{PREFIX}{}", hmac_sha256_hex(b"k", b"push\n{}"))
        );
        assert_ne!(
            signature("k", "push", b"{}"),
            signature("k", "issues", b"{}")
        );
    }

    /// A signature verifies for the event, body and secret it was made for and for no other, and a
    /// malformed one is refused rather than compared.
    #[test]
    fn a_signature_verifies_only_for_what_it_signed() {
        let good = signature("secret", "push", b"body");
        assert!(verify("secret", "push", b"body", &good));
        assert!(!verify("secret", "issues", b"body", &good));
        assert!(!verify("secret", "push", b"body!", &good));
        assert!(!verify("other", "push", b"body", &good));
        let hex = good.strip_prefix(PREFIX).expect("prefixed");
        for bad in [
            "",
            "sha256=",
            hex,
            &format!("sha256={}", &hex[..hex.len() - 2]),
            &format!("sha256={hex}00"),
            &format!("sha256={}zz", &hex[..hex.len() - 2]),
            &format!("sha1={hex}"),
        ] {
            assert!(!verify("secret", "push", b"body", bad), "{bad}");
        }
        assert!(verify(
            "secret",
            "push",
            b"body",
            &format!("{PREFIX}{}", hex.to_uppercase())
        ));
    }
}
