// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! The TLS the listener speaks: version 1.3 only, a certificate required from the device, and
//! neither tickets nor a store of sessions.
//!
//! A device is not known by who signed its certificate, since it signs its own. It is known by the
//! hash of its public key, which the listener reads after the handshake and looks up in the
//! registry. What the handshake proves is that the device holds the private key of the certificate
//! it showed.
//!
//! Nothing resumes. A resumed session would skip the certificate exchange, so a device the person
//! revoked could return on a ticket it kept. The listener sends no ticket and keeps no session, so
//! every connection is a full handshake and a full look at the registry.

use crate::identity::{Identity, public_key_hash};
use rustls::client::danger::HandshakeSignatureValid;
use rustls::crypto::{WebPkiSupportedAlgorithms, ring as provider};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, UnixTime};
use rustls::server::NoServerSessionStorage;
use rustls::server::danger::{ClientCertVerified, ClientCertVerifier};
use rustls::{DigitallySignedStruct, DistinguishedName, Error, ServerConfig, SignatureScheme};
use std::sync::Arc;

/// The one scheme a device signs the handshake with: ECDSA over P-256 and SHA-256, the scheme of its
/// key.
const SCHEME: SignatureScheme = SignatureScheme::ECDSA_NISTP256_SHA256;

/// Asks a device for a certificate and accepts any that names a P-256 key.
#[derive(Debug)]
struct AnyDevice {
    algorithms: WebPkiSupportedAlgorithms,
}

impl ClientCertVerifier for AnyDevice {
    fn root_hint_subjects(&self) -> &[DistinguishedName] {
        &[]
    }

    /// The certificate is accepted when it holds a key the host can name. Whether that key is a
    /// device the person paired is the registry's answer, and is asked once the handshake is over.
    fn verify_client_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _now: UnixTime,
    ) -> Result<ClientCertVerified, Error> {
        public_key_hash(end_entity.as_ref())
            .map(|_| ClientCertVerified::assertion())
            .ok_or(Error::InvalidCertificate(
                rustls::CertificateError::BadEncoding,
            ))
    }

    /// No handshake is made below TLS 1.3, so none is signed this way.
    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        Err(Error::PeerIncompatible(
            rustls::PeerIncompatible::Tls12NotOffered,
        ))
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        if dss.scheme != SCHEME {
            return Err(Error::PeerMisbehaved(
                rustls::PeerMisbehaved::SignedHandshakeWithUnadvertisedSigScheme,
            ));
        }
        rustls::crypto::verify_tls13_signature(message, cert, dss, &self.algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![SCHEME]
    }

    fn offer_client_auth(&self) -> bool {
        true
    }

    fn client_auth_mandatory(&self) -> bool {
        true
    }
}

/// The configuration of every connection the listener accepts.
pub(crate) fn server_config(identity: &Identity) -> Result<Arc<ServerConfig>, Error> {
    let crypto = Arc::new(provider::default_provider());
    let verifier = Arc::new(AnyDevice {
        algorithms: crypto.signature_verification_algorithms,
    });
    let mut config = ServerConfig::builder_with_provider(crypto)
        .with_protocol_versions(&[&rustls::version::TLS13])?
        .with_client_cert_verifier(verifier)
        .with_single_cert(
            vec![CertificateDer::from(identity.certificate().to_vec())],
            PrivateKeyDer::from(PrivatePkcs8KeyDer::from(identity.private_key().to_vec())),
        )?;
    config.send_tls13_tickets = 0;
    config.session_storage = Arc::new(NoServerSessionStorage {});
    Ok(Arc::new(config))
}
