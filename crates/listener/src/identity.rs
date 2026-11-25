// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! A key, the certificate that holds it, and the hash a key is named by.
//!
//! The host and each device have a P-256 key and a self-signed certificate for it. No authority is
//! involved: a certificate is only the form TLS carries a public key in. What each side checks is
//! the SHA-256 hash of the other's public key, written as a [`KeyHash`], against the one it was told
//! to expect. The host's hash is in the invitation a person scans, and a device's is what the host
//! keeps when it pairs the device.
//!
//! The certificate is written here, a few dozen bytes of DER, so no certificate library is in this
//! crate's dependencies. [`public_key_hash`] reads a certificate a stranger sent, so it reads the
//! one position it needs and answers `None` for any shape but the one a P-256 key has.

use crate::der::{self, SEQUENCE};
use ring::digest;
use ring::rand::{SecureRandom, SystemRandom};
use ring::signature::{ECDSA_P256_SHA256_ASN1_SIGNING, EcdsaKeyPair, KeyPair};

/// The SHA-256 hash of a public key as TLS carries it, which is how a key is named.
pub type KeyHash = [u8; 32];

/// What an `ecdsa-with-SHA256` signature algorithm is, as an AlgorithmIdentifier.
const ECDSA_SHA256: [u8; 8] = [0x2A, 0x86, 0x48, 0xCE, 0x3D, 0x04, 0x03, 0x02];

/// `id-ecPublicKey` and `prime256v1`.
const EC_PUBLIC_KEY: [u8; 7] = [0x2A, 0x86, 0x48, 0xCE, 0x3D, 0x02, 0x01];
const PRIME256V1: [u8; 8] = [0x2A, 0x86, 0x48, 0xCE, 0x3D, 0x03, 0x01, 0x07];

/// `commonName`.
const COMMON_NAME: [u8; 3] = [0x55, 0x04, 0x03];

/// The bytes a P-256 key's SubjectPublicKeyInfo starts with: the SEQUENCE, the AlgorithmIdentifier
/// of an elliptic curve key on prime256v1, and the BIT STRING that holds the 65-byte point.
const SPKI_START: [u8; 27] = [
    0x30, 0x59, 0x30, 0x13, 0x06, 0x07, 0x2A, 0x86, 0x48, 0xCE, 0x3D, 0x02, 0x01, 0x06, 0x08, 0x2A,
    0x86, 0x48, 0xCE, 0x3D, 0x03, 0x01, 0x07, 0x03, 0x42, 0x00, 0x04,
];

/// A key may be named only by what its SubjectPublicKeyInfo is: this many bytes for a P-256 key.
const SPKI_LENGTH: usize = 91;

/// Why a key could not be made.
#[derive(Debug, PartialEq, Eq)]
pub enum IdentityError {
    /// The system could not give random bytes, or the key could not be made from them.
    Key,
}

impl std::fmt::Display for IdentityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("the system could not make a key")
    }
}

impl std::error::Error for IdentityError {}

/// A private key and the self-signed certificate for it. It is made in memory and is written
/// nowhere.
pub struct Identity {
    pair: EcdsaKeyPair,
    pkcs8: Vec<u8>,
    certificate: Vec<u8>,
    hash: KeyHash,
}

impl Identity {
    /// A new key and a certificate that names it `common_name`.
    pub fn generate(common_name: &str) -> Result<Self, IdentityError> {
        let random = SystemRandom::new();
        let pkcs8 = EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, &random)
            .map_err(|_| IdentityError::Key)?;
        let pair =
            EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, pkcs8.as_ref(), &random)
                .map_err(|_| IdentityError::Key)?;
        let mut serial = [0u8; 16];
        random.fill(&mut serial).map_err(|_| IdentityError::Key)?;
        // A positive INTEGER: the top bit clear, and never a leading zero byte that would shorten it.
        serial[0] = (serial[0] & 0x7F) | 0x01;
        let certificate = self_signed(&pair, &random, common_name, &serial)?;
        let hash = public_key_hash(&certificate).ok_or(IdentityError::Key)?;
        Ok(Self {
            pair,
            pkcs8: pkcs8.as_ref().to_vec(),
            certificate,
            hash,
        })
    }

    /// The certificate, in DER.
    pub fn certificate(&self) -> &[u8] {
        &self.certificate
    }

    /// The private key, as PKCS #8 DER, for the TLS library that signs with it.
    pub fn private_key(&self) -> &[u8] {
        &self.pkcs8
    }

    /// The hash a person's invitation names this key by.
    pub fn key_hash(&self) -> KeyHash {
        self.hash
    }

    /// An ECDSA signature over `message`, in DER, made with this key.
    pub fn sign(&self, message: &[u8]) -> Result<Vec<u8>, IdentityError> {
        self.pair
            .sign(&SystemRandom::new(), message)
            .map(|signature| signature.as_ref().to_vec())
            .map_err(|_| IdentityError::Key)
    }
}

impl std::fmt::Debug for Identity {
    /// Names the key by its hash and holds back the private key, so a log line or a failed
    /// assertion shows nothing that signs.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Identity({})", hex(&self.hash))
    }
}

/// The SubjectPublicKeyInfo of a P-256 key whose public point is `point`.
fn spki(point: &[u8]) -> Vec<u8> {
    let mut bits = vec![0u8];
    bits.extend(point);
    der::sequence(&[
        der::sequence(&[der::tlv(0x06, &EC_PUBLIC_KEY), der::tlv(0x06, &PRIME256V1)]),
        der::tlv(0x03, &bits),
    ])
}

/// A self-signed X.509 version 3 certificate for `pair`, with no extensions.
fn self_signed(
    pair: &EcdsaKeyPair,
    random: &SystemRandom,
    common_name: &str,
    serial: &[u8],
) -> Result<Vec<u8>, IdentityError> {
    let name = der::sequence(&[der::tlv(
        0x31,
        &der::sequence(&[
            der::tlv(0x06, &COMMON_NAME),
            der::tlv(0x0C, common_name.as_bytes()),
        ]),
    )]);
    // Neither side reads these: a certificate stands for a key and is not trusted for how long it
    // says it lasts. They are there because the encoding has to say something.
    let validity = der::sequence(&[
        der::tlv(0x17, b"261101000000Z"),
        der::tlv(0x17, b"361101000000Z"),
    ]);
    let algorithm = der::sequence(&[der::tlv(0x06, &ECDSA_SHA256)]);
    let to_be_signed = der::sequence(&[
        der::tlv(0xA0, &der::tlv(0x02, &[2])),
        der::tlv(0x02, serial),
        algorithm.clone(),
        name.clone(),
        validity,
        name,
        spki(pair.public_key().as_ref()),
    ]);
    let signature = pair
        .sign(random, &to_be_signed)
        .map_err(|_| IdentityError::Key)?;
    let mut bits = vec![0u8];
    bits.extend(signature.as_ref());
    let certificate = der::sequence(&[to_be_signed, algorithm, der::tlv(0x03, &bits)]);
    if certificate.is_empty() {
        return Err(IdentityError::Key);
    }
    Ok(certificate)
}

/// The hash of the public key in `certificate`, or `None` when it does not hold exactly a P-256
/// key in the position a certificate has one.
///
/// The certificate came from the other end of a connection, so each step is a read that can fail.
/// The TLS library has checked that its signature handshake was made with the key in this position;
/// this is where the key is named.
pub fn public_key_hash(certificate: &[u8]) -> Option<KeyHash> {
    let (_, certificate, _) = der::read(certificate).filter(|(tag, _, _)| *tag == SEQUENCE)?;
    let (_, mut fields, _) = der::read(certificate).filter(|(tag, _, _)| *tag == SEQUENCE)?;
    // The version is an optional element in a tag of its own, ahead of the serial number.
    if let Some((0xA0, _, after)) = der::read(fields) {
        fields = after;
    }
    // The serial number, the signature algorithm, the issuer, the validity and the subject.
    for _ in 0..5 {
        let (_, _, after) = der::read(fields)?;
        fields = after;
    }
    let spki = der::whole_element(fields)?;
    if spki.len() != SPKI_LENGTH || spki[..SPKI_START.len()] != SPKI_START {
        return None;
    }
    let mut hash = [0u8; 32];
    hash.copy_from_slice(digest::digest(&digest::SHA256, spki).as_ref());
    Some(hash)
}

/// The bytes a string of hexadecimal digits holds, or `None` where it is not one.
pub fn unhex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) || !text.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let digit = |byte: u8| {
        char::from(byte)
            .to_digit(16)
            .and_then(|digit| u8::try_from(digit).ok())
    };
    text.as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|[high, low]| Some(digit(*high)? << 4 | digit(*low)?))
        .collect()
}

/// A hash as the lower-case hexadecimal a person or a log reads.
pub fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        // Writing to a string cannot fail.
        write!(out, "{byte:02x}").ok();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustls::pki_types::CertificateDer;
    use webpki::EndEntityCert;

    /// The SubjectPublicKeyInfo of the key, derived here from the public point and a written-out
    /// header, so the hash is checked against bytes this crate's own reader did not produce.
    fn independent_spki_hash(identity: &Identity) -> KeyHash {
        let point = identity.pair.public_key().as_ref();
        assert_eq!(point.len(), 65);
        let mut spki = vec![
            0x30, 0x59, 0x30, 0x13, 0x06, 0x07, 0x2A, 0x86, 0x48, 0xCE, 0x3D, 0x02, 0x01, 0x06,
            0x08, 0x2A, 0x86, 0x48, 0xCE, 0x3D, 0x03, 0x01, 0x07, 0x03, 0x42, 0x00,
        ];
        spki.extend(point);
        let mut hash = [0u8; 32];
        hash.copy_from_slice(digest::digest(&digest::SHA256, &spki).as_ref());
        hash
    }

    /// The certificate is read by the library TLS checks a connection's signature with, and that
    /// library finds in it the key that made the signature.
    #[test]
    fn the_certificate_is_one_the_tls_library_reads_and_holds_the_key_that_signs() {
        let identity = Identity::generate("a-device").unwrap();
        let der = CertificateDer::from(identity.certificate());
        let certificate = EndEntityCert::try_from(&der).unwrap();
        let message = b"a message the key signs";
        let signature = identity.sign(message).unwrap();
        certificate
            .verify_signature(webpki::ring::ECDSA_P256_SHA256, message, &signature)
            .unwrap();
        assert!(
            certificate
                .verify_signature(webpki::ring::ECDSA_P256_SHA256, b"another", &signature)
                .is_err()
        );
    }

    #[test]
    fn a_key_is_named_by_the_hash_of_its_own_public_key_info() {
        let first = Identity::generate("one").unwrap();
        let second = Identity::generate("one").unwrap();
        assert_eq!(
            public_key_hash(first.certificate()),
            Some(independent_spki_hash(&first))
        );
        assert_eq!(first.key_hash(), independent_spki_hash(&first));
        assert_ne!(first.key_hash(), second.key_hash());
    }

    /// The name in the certificate is not what the key is named by: two certificates for different
    /// keys under one name differ, and the hash does not read the name.
    #[test]
    fn the_name_in_the_certificate_does_not_change_the_hash_of_its_key() {
        let identity = Identity::generate("host").unwrap();
        let again = EcdsaKeyPair::from_pkcs8(
            &ECDSA_P256_SHA256_ASN1_SIGNING,
            identity.private_key(),
            &SystemRandom::new(),
        )
        .unwrap();
        let renamed = self_signed(&again, &SystemRandom::new(), "someone-else", &[1]).unwrap();
        assert_eq!(public_key_hash(&renamed), Some(identity.key_hash()));
    }

    /// A certificate cut short anywhere, and one with any single byte changed, gives a hash or
    /// `None` and never a panic. A change outside the key's own bytes leaves the hash as it was or
    /// finds no key, and a change inside them never leaves the hash as it was.
    #[test]
    fn a_certificate_that_is_cut_or_changed_is_read_without_a_panic() {
        let identity = Identity::generate("a-device").unwrap();
        let certificate = identity.certificate().to_vec();
        let key_at = certificate
            .windows(SPKI_START.len())
            .position(|window| window == SPKI_START)
            .unwrap();
        let key_bytes = key_at..key_at + SPKI_LENGTH;
        for end in 0..certificate.len() {
            assert_eq!(public_key_hash(&certificate[..end]), None, "cut at {end}");
        }
        for at in 0..certificate.len() {
            for flipped in [0x00, 0xff, certificate[at] ^ 0x01] {
                if flipped == certificate[at] {
                    continue;
                }
                let mut changed = certificate.clone();
                changed[at] = flipped;
                match public_key_hash(&changed) {
                    Some(hash) if key_bytes.contains(&at) => {
                        assert_ne!(hash, identity.key_hash(), "byte {at} set to {flipped:#x}");
                    }
                    Some(hash) => {
                        assert_eq!(hash, identity.key_hash(), "byte {at} set to {flipped:#x}");
                    }
                    None => {}
                }
            }
        }
    }

    /// Only a P-256 key is named: another curve, another key type and a point that is not
    /// uncompressed are not, so a device cannot present a key the host could not check a signature of.
    #[test]
    fn a_key_that_is_not_p256_is_not_named() {
        let identity = Identity::generate("a-device").unwrap();
        let certificate = identity.certificate().to_vec();
        let at = certificate
            .windows(PRIME256V1.len())
            .position(|window| window == PRIME256V1)
            .unwrap();
        let mut other_curve = certificate.clone();
        other_curve[at + PRIME256V1.len() - 1] ^= 0x02;
        assert_eq!(public_key_hash(&other_curve), None);
        let at = certificate
            .windows(EC_PUBLIC_KEY.len())
            .position(|window| window == EC_PUBLIC_KEY)
            .unwrap();
        let mut other_type = certificate.clone();
        other_type[at + EC_PUBLIC_KEY.len() - 1] ^= 0x02;
        assert_eq!(public_key_hash(&other_type), None);
        let mut compressed = certificate;
        let point = compressed
            .windows(SPKI_START.len())
            .position(|window| window == SPKI_START)
            .unwrap()
            + SPKI_START.len()
            - 1;
        compressed[point] = 0x02;
        assert_eq!(public_key_hash(&compressed), None);
    }

    #[test]
    fn hexadecimal_is_read_back_and_anything_else_is_not() {
        assert_eq!(unhex("000fa0ff"), Some(vec![0x00, 0x0f, 0xa0, 0xff]));
        assert_eq!(unhex("000FA0FF"), Some(vec![0x00, 0x0f, 0xa0, 0xff]));
        assert_eq!(unhex(""), Some(Vec::new()));
        for bad in ["0", "abc", "zz", "+1", "0x", "é1", "a b "] {
            assert_eq!(unhex(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn a_hash_is_written_as_lower_case_hexadecimal_two_digits_a_byte() {
        assert_eq!(hex(&[0x00, 0x0f, 0xa0, 0xff]), "000fa0ff");
        let identity = Identity::generate("a-device").unwrap();
        assert_eq!(hex(&identity.key_hash()).len(), 64);
    }

    #[test]
    fn the_debug_form_of_an_identity_names_the_key_and_holds_back_the_private_key() {
        let identity = Identity::generate("a-device").unwrap();
        let shown = format!("{identity:?}");
        assert!(shown.contains(&hex(&identity.key_hash())));
        assert!(!shown.contains(&hex(identity.private_key())));
    }
}
