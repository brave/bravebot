// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! A device's side of a connection, for tests.
//!
//! It makes a key and a certificate as a phone does, pins the host's key as an invitation tells it
//! to, and speaks lines over TLS 1.3. Nothing in the host's own code uses it: it is built only for
//! the tests of this crate and of the host, behind the `testing` feature.

use crate::identity::{Identity, KeyHash, hex, public_key_hash};
use crate::pairing::{self, Secret};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::client::{ClientSessionMemoryCache, Resumption};
use rustls::crypto::{WebPkiSupportedAlgorithms, ring as provider};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName, UnixTime};
use rustls::{
    ClientConfig, ClientConnection, DigitallySignedStruct, Error, HandshakeKind, ProtocolVersion,
    SignatureScheme, StreamOwned,
};
use std::collections::HashMap;
use std::io::{self, BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// How long a test waits on a read or a write before it calls the connection stuck.
const PATIENCE: Duration = Duration::from_secs(5);

/// Accepts the host whose key hashes to `pin` and no other.
#[derive(Debug)]
struct Pinned {
    pin: KeyHash,
    algorithms: WebPkiSupportedAlgorithms,
}

impl ServerCertVerifier for Pinned {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, Error> {
        if public_key_hash(end_entity.as_ref()) == Some(self.pin) {
            Ok(ServerCertVerified::assertion())
        } else {
            Err(Error::InvalidCertificate(
                rustls::CertificateError::ApplicationVerificationFailure,
            ))
        }
    }

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
        rustls::crypto::verify_tls13_signature(message, cert, dss, &self.algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![SignatureScheme::ECDSA_NISTP256_SHA256]
    }
}

/// A phone: a key, and the sessions it has kept.
pub struct Phone {
    identity: Identity,
    sessions: Arc<ClientSessionMemoryCache>,
    /// The configuration of the last connection, kept because a client offers a session it kept only
    /// under the configuration that kept it.
    configs: Mutex<HashMap<(KeyHash, bool), Arc<ClientConfig>>>,
}

/// One connection of a phone, after its handshake.
pub struct Connection {
    stream: BufReader<StreamOwned<ClientConnection, TcpStream>>,
    key: KeyHash,
}

impl Phone {
    pub fn new() -> io::Result<Self> {
        Ok(Self {
            identity: Identity::generate("a-phone").map_err(io::Error::other)?,
            sessions: Arc::new(ClientSessionMemoryCache::new(8)),
            configs: Mutex::new(HashMap::new()),
        })
    }

    /// The hash the host names this phone by.
    pub fn key_hash(&self) -> KeyHash {
        self.identity.key_hash()
    }

    /// Connect to `address`, accepting only a host whose key hashes to `pin`, and showing this
    /// phone's certificate. Fails where the handshake does, which includes a host key that is not
    /// the pinned one. A phone the host will refuse still gets here: TLS 1.3 tells a client its
    /// certificate was refused after the client has finished.
    pub fn connect(&self, address: SocketAddr, pin: KeyHash) -> io::Result<Connection> {
        self.connect_as(address, pin, true)
    }

    /// As [`Phone::connect`], showing no certificate.
    pub fn connect_without_certificate(
        &self,
        address: SocketAddr,
        pin: KeyHash,
    ) -> io::Result<Connection> {
        self.connect_as(address, pin, false)
    }

    /// The configuration for connections that pin `pin` and show a certificate or not.
    fn config(&self, pin: KeyHash, certificate: bool) -> io::Result<Arc<ClientConfig>> {
        let mut configs = self
            .configs
            .lock()
            .map_err(|_| io::Error::other("the phone's configurations cannot be read"))?;
        if let Some(config) = configs.get(&(pin, certificate)) {
            return Ok(Arc::clone(config));
        }
        let crypto = Arc::new(provider::default_provider());
        let builder = ClientConfig::builder_with_provider(Arc::clone(&crypto))
            .with_protocol_versions(&[&rustls::version::TLS13])
            .map_err(io::Error::other)?
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(Pinned {
                pin,
                algorithms: crypto.signature_verification_algorithms,
            }));
        let mut config = if certificate {
            builder
                .with_client_auth_cert(
                    vec![CertificateDer::from(self.identity.certificate().to_vec())],
                    PrivateKeyDer::from(PrivatePkcs8KeyDer::from(
                        self.identity.private_key().to_vec(),
                    )),
                )
                .map_err(io::Error::other)?
        } else {
            builder.with_no_client_auth()
        };
        config.resumption = Resumption::store(self.sessions.clone());
        let config = Arc::new(config);
        configs.insert((pin, certificate), Arc::clone(&config));
        Ok(config)
    }

    fn connect_as(
        &self,
        address: SocketAddr,
        pin: KeyHash,
        certificate: bool,
    ) -> io::Result<Connection> {
        let config = self.config(pin, certificate)?;
        let mut tcp = TcpStream::connect_timeout(&address, PATIENCE)?;
        tcp.set_read_timeout(Some(PATIENCE))?;
        tcp.set_write_timeout(Some(PATIENCE))?;
        let name = ServerName::try_from("bravebot-host").map_err(io::Error::other)?;
        let mut connection = ClientConnection::new(config, name).map_err(io::Error::other)?;
        while connection.is_handshaking() {
            connection.complete_io(&mut tcp)?;
        }
        Ok(Connection {
            stream: BufReader::new(StreamOwned::new(connection, tcp)),
            key: self.identity.key_hash(),
        })
    }
}

impl Connection {
    /// Send one line.
    pub fn send(&mut self, line: &str) -> io::Result<()> {
        let stream = self.stream.get_mut();
        writeln!(stream, "{line}")?;
        stream.flush()
    }

    /// Ask to pair with the secret of an invitation, as a phone does after it scans one: send the
    /// proof for this connection and this phone's key, and say what the phone calls itself. Returns
    /// the code the phone shows for the person to type on the host. The host answers on this
    /// connection once the person has typed it.
    pub fn begin_pairing(&mut self, secret: &Secret, label: Option<&str>) -> io::Result<String> {
        let exporter = self.exporter(pairing::EXPORTER_LABEL, Some(pairing::EXPORTER_CONTEXT))?;
        let proof = pairing::proof(secret, &exporter, &self.key);
        self.send_proof(&proof, label)?;
        Ok(pairing::confirmation_code(secret, &exporter, &self.key))
    }

    /// Send `proof` as the proof of the secret, whatever it is made of, for a test of one that is
    /// not this connection's.
    pub fn send_proof(&mut self, proof: &[u8], label: Option<&str>) -> io::Result<()> {
        let mut params = serde_json::json!({"proof": hex(proof)});
        if let Some(label) = label {
            params["label"] = serde_json::Value::String(label.to_string());
        }
        let line = serde_json::json!({"id": 1, "method": pairing::BEGIN, "params": params});
        self.send(&line.to_string())
    }

    /// Send these bytes as they are.
    pub fn send_bytes(&mut self, bytes: &[u8]) -> io::Result<()> {
        let stream = self.stream.get_mut();
        stream.write_all(bytes)?;
        stream.flush()
    }

    /// The next line, `Ok(None)` once the host has ended the connection by any means, and an error
    /// where nothing came in the time a test waits, which is a connection that is still open.
    pub fn receive(&mut self) -> io::Result<Option<String>> {
        let mut line = String::new();
        match self.stream.read_line(&mut line) {
            Ok(0) => Ok(None),
            Ok(_) => Ok(Some(line.trim_end_matches('\n').to_string())),
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::UnexpectedEof
                        | io::ErrorKind::ConnectionReset
                        | io::ErrorKind::ConnectionAborted
                        | io::ErrorKind::BrokenPipe
                ) =>
            {
                Ok(None)
            }
            // An alert from the host, such as a certificate it required, is the host ending it.
            Err(error)
                if error
                    .get_ref()
                    .is_some_and(|inner| inner.downcast_ref::<Error>().is_some()) =>
            {
                Ok(None)
            }
            Err(error) => Err(error),
        }
    }

    /// As [`Connection::receive`], waiting `patience` and not the usual time.
    pub fn receive_within(&mut self, patience: Duration) -> io::Result<Option<String>> {
        self.stream
            .get_ref()
            .sock
            .set_read_timeout(Some(patience))?;
        let heard = self.receive();
        self.stream
            .get_ref()
            .sock
            .set_read_timeout(Some(PATIENCE))?;
        heard
    }

    /// Whether the host has ended this connection, found by reading what it would send.
    pub fn ended(&mut self) -> bool {
        matches!(self.receive(), Ok(None))
    }

    /// The value both ends compute from this connection's keys, for `label` and `context`.
    pub fn exporter(&self, label: &[u8], context: Option<&[u8]>) -> io::Result<[u8; 32]> {
        let mut out = [0u8; 32];
        self.stream
            .get_ref()
            .conn
            .export_keying_material(&mut out, label, context)
            .map_err(io::Error::other)?;
        Ok(out)
    }

    /// Whether this connection's handshake was a full one and not a resumed session.
    pub fn was_a_full_handshake(&self) -> bool {
        self.stream.get_ref().conn.handshake_kind() == Some(HandshakeKind::Full)
    }

    pub fn protocol(&self) -> Option<ProtocolVersion> {
        self.stream.get_ref().conn.protocol_version()
    }
}
