// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! What a phone and the host compute when the phone pairs.
//!
//! The phone has the invitation's secret and the host's key hash from the code it scanned. TLS has
//! shown each end the other's key, so the phone knows it is talking to the host the code named, and
//! the host knows which key the phone holds. What is left to show is that this key belongs to the
//! phone of the person who started the pairing, and that is the secret: the phone sends a keyed hash
//! of the value both ends take from this connection's TLS session and of its own key's hash, and the
//! host checks it with the same secret.
//!
//! The value taken from the session is different for every connection, so a proof seen on one
//! connection proves nothing on another. The proof binds the phone's key, so it is not the proof of
//! another key. Neither the proof nor the confirmation code below is a secret once used.
//!
//! The confirmation code is a shorter value of the same inputs, shown on the phone and typed on the
//! host. A program that watches the host's screen cannot answer it, because the host does not show it.

use crate::identity::KeyHash;
use ring::hmac;

/// The label both ends give the TLS library for the value taken from a session.
pub const EXPORTER_LABEL: &[u8] = b"EXPORTER-bravebot-pairing-v1";

/// The context both ends give with it. Neither platform of a phone takes an empty one.
pub const EXPORTER_CONTEXT: &[u8] = b"bravebot";

/// The method of the one line a device that is not paired may send.
pub const BEGIN: &str = "pair.begin";

/// What the secret of an invitation is: 32 bytes.
pub type Secret = [u8; 32];

/// The value both ends take from a connection's TLS session.
pub type Exporter = [u8; 32];

fn keyed(secret: &Secret, purpose: &[u8], exporter: &Exporter, device: &KeyHash) -> hmac::Tag {
    let key = hmac::Key::new(hmac::HMAC_SHA256, secret);
    let mut message = Vec::with_capacity(purpose.len() + 64);
    message.extend(purpose);
    message.extend(exporter);
    message.extend(device);
    hmac::sign(&key, &message)
}

/// What the phone sends to show it has the secret, on the connection that has `exporter`, with the
/// key that hashes to `device`.
pub fn proof(secret: &Secret, exporter: &Exporter, device: &KeyHash) -> [u8; 32] {
    let tag = keyed(secret, b"bravebot-pairing-proof-v1", exporter, device);
    let mut out = [0u8; 32];
    out.copy_from_slice(tag.as_ref());
    out
}

/// Whether `candidate` is the proof, in time that does not depend on how much of it is right.
pub fn proof_is_right(
    secret: &Secret,
    exporter: &Exporter,
    device: &KeyHash,
    candidate: &[u8],
) -> bool {
    let key = hmac::Key::new(hmac::HMAC_SHA256, secret);
    let mut message = Vec::with_capacity(96);
    message.extend(b"bravebot-pairing-proof-v1");
    message.extend(exporter);
    message.extend(device);
    hmac::verify(&key, &message, candidate).is_ok()
}

/// The six digits the phone shows and the person types on the host.
pub fn confirmation_code(secret: &Secret, exporter: &Exporter, device: &KeyHash) -> String {
    let tag = keyed(secret, b"bravebot-pairing-code-v1", exporter, device);
    let bytes = tag.as_ref();
    let number = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    format!("{:06}", number % 1_000_000)
}

/// Whether two codes are equal, in time that does not depend on where they differ.
pub fn codes_match(first: &str, second: &str) -> bool {
    // Each is hashed under a key, and the hashes are compared by the library's own check, so how
    // much of a guess is right is not in how long the comparison takes.
    let key = hmac::Key::new(hmac::HMAC_SHA256, b"bravebot-code-comparison");
    let expected = hmac::sign(&key, second.as_bytes());
    hmac::verify(&key, first.as_bytes(), expected.as_ref()).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: Secret = [7; 32];
    const OTHER_SECRET: Secret = [8; 32];
    const EXPORTER: Exporter = [1; 32];
    const OTHER_EXPORTER: Exporter = [2; 32];
    const DEVICE: KeyHash = [3; 32];
    const OTHER_DEVICE: KeyHash = [4; 32];

    /// Each input changes the proof and the code, and the same inputs give the same answers, so a
    /// proof made on another connection, for another key or with another secret is not this one.
    #[test]
    fn the_proof_and_the_code_follow_each_of_their_inputs() {
        let proof_of = |secret, exporter, device| proof(&secret, &exporter, &device);
        let code_of = |secret, exporter, device| confirmation_code(&secret, &exporter, &device);
        let base = proof_of(SECRET, EXPORTER, DEVICE);
        assert_eq!(base, proof_of(SECRET, EXPORTER, DEVICE));
        assert_ne!(base, proof_of(OTHER_SECRET, EXPORTER, DEVICE));
        assert_ne!(base, proof_of(SECRET, OTHER_EXPORTER, DEVICE));
        assert_ne!(base, proof_of(SECRET, EXPORTER, OTHER_DEVICE));
        let code = code_of(SECRET, EXPORTER, DEVICE);
        assert_eq!(code, code_of(SECRET, EXPORTER, DEVICE));
        assert_ne!(code, code_of(OTHER_SECRET, EXPORTER, DEVICE));
        assert_ne!(code, code_of(SECRET, OTHER_EXPORTER, DEVICE));
        assert_ne!(code, code_of(SECRET, EXPORTER, OTHER_DEVICE));
    }

    /// The code and the proof are different values of the same inputs, so the code on a phone's
    /// screen is not the start of the proof.
    #[test]
    fn the_code_is_not_made_from_the_start_of_the_proof() {
        let code = confirmation_code(&SECRET, &EXPORTER, &DEVICE);
        let proof = proof(&SECRET, &EXPORTER, &DEVICE);
        let from_proof = u32::from_be_bytes([proof[0], proof[1], proof[2], proof[3]]) % 1_000_000;
        assert_ne!(code, format!("{from_proof:06}"));
    }

    #[test]
    fn a_code_is_six_digits() {
        for seed in 0..200u8 {
            let code = confirmation_code(&[seed; 32], &EXPORTER, &DEVICE);
            assert_eq!(code.len(), 6, "{code}");
            assert!(code.bytes().all(|byte| byte.is_ascii_digit()), "{code}");
        }
    }

    #[test]
    fn a_proof_is_right_only_when_every_input_is_and_every_byte_is() {
        let right = proof(&SECRET, &EXPORTER, &DEVICE);
        assert!(proof_is_right(&SECRET, &EXPORTER, &DEVICE, &right));
        assert!(!proof_is_right(&OTHER_SECRET, &EXPORTER, &DEVICE, &right));
        assert!(!proof_is_right(&SECRET, &OTHER_EXPORTER, &DEVICE, &right));
        assert!(!proof_is_right(&SECRET, &EXPORTER, &OTHER_DEVICE, &right));
        assert!(!proof_is_right(&SECRET, &EXPORTER, &DEVICE, &right[..31]));
        assert!(!proof_is_right(&SECRET, &EXPORTER, &DEVICE, &[]));
        for at in 0..32 {
            let mut wrong = right;
            wrong[at] ^= 1;
            assert!(
                !proof_is_right(&SECRET, &EXPORTER, &DEVICE, &wrong),
                "byte {at}"
            );
        }
    }

    #[test]
    fn two_codes_match_only_when_equal() {
        assert!(codes_match("123456", "123456"));
        assert!(!codes_match("123456", "123457"));
        assert!(!codes_match("123456", "12345"));
        assert!(!codes_match("", "123456"));
    }
}
