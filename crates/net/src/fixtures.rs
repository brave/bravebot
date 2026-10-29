//! A TLS server of the tests' own, and the throwaway certificates it and its clients use.
//!
//! The server can require a client certificate, which is the only way to observe what an egress
//! request presents. Every certificate here is made for these tests and used for nothing else.
//! They are valid from 2020 to 2120 so that no run depends on the date. `CA` signed `SERVER`,
//! `GOOD` and `CHANGED`; `OTHER_CA` signed `STRANGER`, which the server does not trust.
//!
//! PEM armour is assembled rather than written, so no literal in this file is a private key to a
//! scanner.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};

const CA: [&str; 7] = [
    "MIIBSTCB8aADAgECAgIQADAKBggqhkjOPQQDAjAbMRkwFwYDVQQDDBBicmF2ZWJv",
    "dC10ZXN0LWNhMCAXDTIwMDEwMTAwMDAwMFoYDzIxMjAwMTAxMDAwMDAwWjAbMRkw",
    "FwYDVQQDDBBicmF2ZWJvdC10ZXN0LWNhMFkwEwYHKoZIzj0CAQYIKoZIzj0DAQcD",
    "QgAEEOEmSbZp2jroeAA4f7+91d6nl164aOeBcTHu3xAdMLS2oEaKtisyby1kvkv+",
    "+nTg7WJVqORxItHPmuK5kuDCHKMjMCEwDwYDVR0TAQH/BAUwAwEB/zAOBgNVHQ8B",
    "Af8EBAMCAgQwCgYIKoZIzj0EAwIDRwAwRAIgVMaghhzTGeucsont1HcGBdFlIpcP",
    "NR8mxtYJ89ESwV8CIDeHpQDS0mKwsVQBI281WW2xtQWV5oh+PWzBlD3i8259",
];
const SERVER_CERTIFICATE: [&str; 8] = [
    "MIIBbDCCARKgAwIBAgICEAIwCgYIKoZIzj0EAwIwGzEZMBcGA1UEAwwQYnJhdmVi",
    "b3QtdGVzdC1jYTAgFw0yMDAxMDEwMDAwMDBaGA8yMTIwMDEwMTAwMDAwMFowHzEd",
    "MBsGA1UEAwwUYnJhdmVib3QtdGVzdC1zZXJ2ZXIwWTATBgcqhkjOPQIBBggqhkjO",
    "PQMBBwNCAATWjSmYLARdrlrHAc4g01uYfUH43Fxm0T/b0tdSztbsb780o95CXbxb",
    "6Xz+4YZb2ENSKnchyKk0mkxPaQ1S7VjSo0AwPjAJBgNVHRMEAjAAMBMGA1UdJQQM",
    "MAoGCCsGAQUFBwMBMAsGA1UdDwQEAwIHgDAPBgNVHREECDAGhwR/AAABMAoGCCqG",
    "SM49BAMCA0gAMEUCIDRD54nsgzacO4mTZOal3FcafPx87b8Bsb/wHBmCobyzAiEA",
    "6e8O3yv3zQ1P6OP+QeTES1lAZaIUrO+E8w+e6e2rli0=",
];
const SERVER_KEY: [&str; 3] = [
    "MIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQgcNaM/KumI3NW9a6x",
    "VxNYmvD6BRKRCAkWjMQcpuxPWPyhRANCAATWjSmYLARdrlrHAc4g01uYfUH43Fxm",
    "0T/b0tdSztbsb780o95CXbxb6Xz+4YZb2ENSKnchyKk0mkxPaQ1S7VjS",
];
const GOOD_CERTIFICATE: [&str; 8] = [
    "MIIBXDCCAQGgAwIBAgICEAMwCgYIKoZIzj0EAwIwGzEZMBcGA1UEAwwQYnJhdmVi",
    "b3QtdGVzdC1jYTAgFw0yMDAxMDEwMDAwMDBaGA8yMTIwMDEwMTAwMDAwMFowHzEd",
    "MBsGA1UEAwwUYnJhdmVib3QtdGVzdC1jbGllbnQwWTATBgcqhkjOPQIBBggqhkjO",
    "PQMBBwNCAASU/wEKbrftYOWJNnTLuTrxvK7VpTeiK0tM9rt0ms1hGo7ce6uNNJC1",
    "PVt8rTLKPxQgH3m05b6Nm/JXiUPOsqhtoy8wLTAJBgNVHRMEAjAAMBMGA1UdJQQM",
    "MAoGCCsGAQUFBwMCMAsGA1UdDwQEAwIHgDAKBggqhkjOPQQDAgNJADBGAiEAtlPA",
    "rOb1wXYytlcFGufVOFZyUXjbgc0n5zNmOZDXcPYCIQDOAZhDLvUHFWksUZ8ogYpL",
    "C5LhsLrfFYBhppLj8PSyzg==",
];
const GOOD_KEY: [&str; 3] = [
    "MIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQgLc/ShtgYplFn8YSO",
    "c4qnrAP9rZi7U0qTlC09QP0hagWhRANCAASU/wEKbrftYOWJNnTLuTrxvK7VpTei",
    "K0tM9rt0ms1hGo7ce6uNNJC1PVt8rTLKPxQgH3m05b6Nm/JXiUPOsqht",
];
const CHANGED_CERTIFICATE: [&str; 9] = [
    "MIIBXzCCAQWgAwIBAgICEAQwCgYIKoZIzj0EAwIwGzEZMBcGA1UEAwwQYnJhdmVi",
    "b3QtdGVzdC1jYTAgFw0yMDAxMDEwMDAwMDBaGA8yMTIwMDEwMTAwMDAwMFowIzEh",
    "MB8GA1UEAwwYYnJhdmVib3QtdGVzdC1jbGllbnQtdHdvMFkwEwYHKoZIzj0CAQYI",
    "KoZIzj0DAQcDQgAERTdWOfIq35jdyrAiYOTsOgKUo4fOO+letWWofF6xnjn+/oji",
    "HDh88jMS5+PpNtdusDtStp2071s0HrKkyD5HOaMvMC0wCQYDVR0TBAIwADATBgNV",
    "HSUEDDAKBggrBgEFBQcDAjALBgNVHQ8EBAMCB4AwCgYIKoZIzj0EAwIDSAAwRQIg",
    "E157U//TjL/1g7SLq5kC/XzmllKkPe1iiIukMwl48DYCIQCUGe2AkYFADn1I0iy3",
    "gb8aj7SeBBWeSpzieNEtArx29g==",
    "",
];
const CHANGED_KEY: [&str; 3] = [
    "MIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQgSrJyaK/mIuGze1tk",
    "imB2mV7EaT+kuFFuZtv3fFV9YpWhRANCAARFN1Y58irfmN3KsCJg5Ow6ApSjh847",
    "6V61Zah8XrGeOf7+iOIcOHzyMxLn4+k2126wO1K2nbTvWzQesqTIPkc5",
];
const STRANGER_CERTIFICATE: [&str; 9] = [
    "MIIBYjCCAQmgAwIBAgICEAUwCgYIKoZIzj0EAwIwITEfMB0GA1UEAwwWYnJhdmVi",
    "b3QtdGVzdC1vdGhlci1jYTAgFw0yMDAxMDEwMDAwMDBaGA8yMTIwMDEwMTAwMDAw",
    "MFowITEfMB0GA1UEAwwWYnJhdmVib3QtdGVzdC1zdHJhbmdlcjBZMBMGByqGSM49",
    "AgEGCCqGSM49AwEHA0IABJDe08QNB2wiYbWNcwyGA/m0Ro3bMrkizZdK1bxYZbTD",
    "l2Xr7jLGKiXjtrA0WdQwzX+sFYn9x7fOZqlBcTPqDuOjLzAtMAkGA1UdEwQCMAAw",
    "EwYDVR0lBAwwCgYIKwYBBQUHAwIwCwYDVR0PBAQDAgeAMAoGCCqGSM49BAMCA0cA",
    "MEQCIFcCUj10+V9lAR30XDVQihRC1Y3m9uNHLpbIpLFr5y9HAiBCtW3DrLcUMu9S",
    "By+P7P+K4UpetUtOIU97unY99gf6PA==",
    "",
];
const STRANGER_KEY: [&str; 3] = [
    "MIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQgr95OUTB4xulmUD2k",
    "dpTl8VNk4pzhQaIzrwaJoL3MjzqhRANCAASQ3tPEDQdsImG1jXMMhgP5tEaN2zK5",
    "Is2XStW8WGW0w5dl6+4yxiol47awNFnUMM1/rBWJ/ce3zmapQXEz6g7j",
];

/// A certificate and key the server accepts.
pub(crate) const GOOD: Pair = Pair {
    certificate: &GOOD_CERTIFICATE,
    key: &GOOD_KEY,
};
/// A second certificate and key the server accepts, which is a different pair from `GOOD`.
pub(crate) const CHANGED: Pair = Pair {
    certificate: &CHANGED_CERTIFICATE,
    key: &CHANGED_KEY,
};
/// A certificate and key from an authority the server does not trust.
pub(crate) const STRANGER: Pair = Pair {
    certificate: &STRANGER_CERTIFICATE,
    key: &STRANGER_KEY,
};

#[derive(Clone, Copy)]
pub(crate) struct Pair {
    certificate: &'static [&'static str],
    key: &'static [&'static str],
}

fn armoured(kind: &str, body: &[&str]) -> String {
    let mut pem = format!("-----BEGIN {kind}-----\n");
    for line in body.iter().filter(|line| !line.is_empty()) {
        pem.push_str(line);
        pem.push('\n');
    }
    pem.push_str(&format!("-----END {kind}-----\n"));
    pem
}

fn key_armoured(body: &[&str]) -> String {
    armoured(&format!("{} KEY", "PRIVATE"), body)
}

impl Pair {
    pub(crate) fn certificate_pem(&self) -> String {
        armoured("CERTIFICATE", self.certificate)
    }

    pub(crate) fn key_pem(&self) -> String {
        key_armoured(self.key)
    }

    /// The certificate as the server sees it when it is presented.
    pub(crate) fn certificate_der(&self) -> Vec<u8> {
        CertificateDer::from_pem_slice(self.certificate_pem().as_bytes())
            .expect("the fixture certificate is PEM")
            .as_ref()
            .to_vec()
    }

    pub(crate) fn write_certificate(&self, path: &Path) {
        std::fs::write(path, self.certificate_pem()).expect("certificate file");
    }

    pub(crate) fn write_key(&self, path: &Path) {
        std::fs::write(path, self.key_pem()).expect("key file");
    }
}

/// The authority a client trusts to accept the server, written to `path`.
pub(crate) fn write_authority(path: &Path) {
    std::fs::write(path, armoured("CERTIFICATE", &CA)).expect("authority file");
}

/// What one connection to the server showed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Attempt {
    /// The DER of the certificate the client presented, if it presented one.
    pub(crate) presented: Option<Vec<u8>>,
    /// Whether the request was read, which needs a handshake the server accepted.
    pub(crate) read_request: bool,
}

pub(crate) type Hook = Arc<dyn Fn(usize) + Send + Sync>;

pub(crate) struct Server {
    pub(crate) port: u16,
    attempts: Arc<Mutex<Vec<Attempt>>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Server {
    /// Serves `replies` in turn, one per connection, repeating the last. With `require` a client
    /// that presents no certificate acceptable to the server is refused at the handshake.
    /// `before(n)` runs when connection `n` is accepted, before its handshake starts.
    pub(crate) fn start(require: bool, replies: Vec<String>, before: Option<Hook>) -> Self {
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let mut roots = rustls::RootCertStore::empty();
        roots
            .add(
                CertificateDer::from_pem_slice(armoured("CERTIFICATE", &CA).as_bytes())
                    .expect("authority"),
            )
            .expect("a root");
        let verifier = rustls::server::WebPkiClientVerifier::builder_with_provider(
            Arc::new(roots),
            provider.clone(),
        );
        let verifier = if require {
            verifier
        } else {
            verifier.allow_unauthenticated()
        }
        .build()
        .expect("a verifier");
        let certificate =
            CertificateDer::from_pem_slice(armoured("CERTIFICATE", &SERVER_CERTIFICATE).as_bytes())
                .expect("server certificate");
        let key = PrivateKeyDer::from_pem_slice(key_armoured(&SERVER_KEY).as_bytes())
            .expect("server key");
        let config = Arc::new(
            rustls::ServerConfig::builder_with_provider(provider)
                .with_safe_default_protocol_versions()
                .expect("protocol versions")
                .with_client_cert_verifier(verifier)
                .with_single_cert(vec![certificate], key)
                .expect("a server certificate"),
        );

        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        let port = listener.local_addr().expect("address").port();
        listener.set_nonblocking(true).expect("non-blocking");
        let attempts = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let thread = {
            let (attempts, stop) = (attempts.clone(), stop.clone());
            std::thread::spawn(move || {
                let mut served = 0usize;
                while !stop.load(Ordering::Relaxed) {
                    let Ok((mut tcp, _)) = listener.accept() else {
                        std::thread::sleep(Duration::from_millis(5));
                        continue;
                    };
                    tcp.set_nonblocking(false).expect("blocking");
                    tcp.set_read_timeout(Some(Duration::from_secs(5))).ok();
                    if let Some(before) = &before {
                        before(served);
                    }
                    let reply = replies
                        .get(served)
                        .or(replies.last())
                        .cloned()
                        .unwrap_or_default();
                    served += 1;
                    let Ok(mut connection) = rustls::ServerConnection::new(config.clone()) else {
                        continue;
                    };
                    let mut tls = rustls::Stream::new(&mut connection, &mut tcp);
                    let mut head = Vec::new();
                    let mut chunk = [0u8; 1024];
                    let read_request = loop {
                        match tls.read(&mut chunk) {
                            Ok(0) | Err(_) => break false,
                            Ok(n) => {
                                head.extend_from_slice(&chunk[..n]);
                                if head.windows(4).any(|w| w == b"\r\n\r\n") {
                                    break true;
                                }
                            }
                        }
                    };
                    let presented = tls
                        .conn
                        .peer_certificates()
                        .and_then(|chain| chain.first())
                        .map(|leaf| leaf.as_ref().to_vec());
                    if read_request {
                        let _ = tls.write_all(reply.as_bytes());
                        let _ = tls.flush();
                        tls.conn.send_close_notify();
                        let _ = tls.conn.complete_io(tls.sock);
                    }
                    attempts
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .push(Attempt {
                            presented,
                            read_request,
                        });
                }
            })
        };
        Self {
            port,
            attempts,
            stop,
            thread: Some(thread),
        }
    }

    pub(crate) fn url(&self, path: &str) -> String {
        format!("https://127.0.0.1:{}{path}", self.port)
    }

    /// What the connections made so far showed, once the server has finished with the last.
    pub(crate) fn attempts(&self) -> Vec<Attempt> {
        std::thread::sleep(Duration::from_millis(200));
        self.attempts
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// A reply with a body, for the server to send.
pub(crate) fn ok(body: &str) -> String {
    format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

/// A redirect to `location`, for the server to send.
pub(crate) fn redirect(location: &str) -> String {
    format!(
        "HTTP/1.1 302 Found\r\nLocation: {location}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    )
}
