//! Verifying the ID token of a sign-in against the authorization server's key set
//! ([CHATGPT-5](../../../../docs/specs/chatgpt-sign-in.md#CHATGPT-5)).
//!
//! The token and the key set are bytes this program did not write. The token is checked for its
//! size and its shape, its header is read for the algorithm and the key it names, the key set is
//! fetched with no credential and no redirect, the signature is checked with RS256 against that
//! key, and then five claims are compared: the issuer, the audience, the expiry, the nonce and the
//! subject. No other claim is read. What leaves this module is whether the token verified, or the
//! class of the key set request that failed, and no claim of the token.

use super::encoding::{base64url_decode, same};
use super::exchange::class_of;
use super::{Class, Hosts};
use crate::Secret;
use crate::secret::Document;
use ring::signature::{RSA_PKCS1_2048_8192_SHA256, RsaPublicKeyComponents};

/// The issuer an ID token of this server names. A constant of the source, read from the program the
/// spec was written from, and not observed from the service.
const ISSUER: &str = "https://auth.openai.com";

/// The one algorithm accepted.
const ALGORITHM: &str = "RS256";

/// How long a token may be. A token of this server is a few kilobytes, and one longer is refused
/// before it is decoded.
const TOKEN_LIMIT: usize = 16 * 1024;

/// How long the key set's reply may be. A key set is a few keys of a few hundred bytes each.
const KEY_SET_LIMIT: u64 = 64 * 1024;

/// Why the token did not verify.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Failed {
    /// The key set could not be had: the request failed in this class.
    KeySet(Class),
    /// The token is not one this sign-in accepts.
    Token,
}

/// Decoded bytes of a token, cleared when they go: the claims are text somebody else chose about
/// the person, and a token is a credential for as long as it is valid.
struct Cleared(Vec<u8>);

impl Drop for Cleared {
    fn drop(&mut self) {
        self.0.fill(0);
        std::hint::black_box(&self.0);
    }
}

/// Verify `token` as the ID token of a sign-in under `client` that sent `nonce`, at the instant
/// `now`.
///
/// The token's header is read before the key set is asked for, so a token of the wrong shape or
/// algorithm sends nothing.
pub(super) fn verify(
    agent: &ureq::Agent,
    hosts: &Hosts,
    token: &Secret,
    client: &str,
    nonce: &Secret,
    now: u64,
) -> Result<(), Failed> {
    let text = token.expose();
    if text.len() > TOKEN_LIMIT {
        return Err(Failed::Token);
    }
    let mut parts = text.split('.');
    let (Some(header), Some(payload), Some(signature), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err(Failed::Token);
    };
    let kid = key_named(header)?;
    let keys = key_set(agent, hosts)?;
    let (n, e) = key(keys.read(), &kid).ok_or(Failed::Token)?;

    let signature = base64url_decode(signature).ok_or(Failed::Token)?;
    // The signed bytes are the two encoded parts as the token holds them, not a re-encoding.
    let signed = text
        .get(..header.len() + 1 + payload.len())
        .ok_or(Failed::Token)?;
    RsaPublicKeyComponents { n: &n, e: &e }
        .verify(&RSA_PKCS1_2048_8192_SHA256, signed.as_bytes(), &signature)
        .map_err(|_| Failed::Token)?;

    let claims = Cleared(base64url_decode(payload).ok_or(Failed::Token)?);
    let claims = Document::of(serde_json::from_slice(&claims.0).map_err(|_| Failed::Token)?);
    if claims_hold(claims.read(), client, nonce, now) {
        Ok(())
    } else {
        Err(Failed::Token)
    }
}

/// The `kid` of a header that names RS256 and nothing that has to be understood.
///
/// `none`, an HMAC and every other algorithm are refused here, so a token signed with a key that
/// is public, such as the modulus, is never checked as if it were a signature. A header with
/// `crit` names an extension the signature depends on, and none is understood.
fn key_named(header: &str) -> Result<String, Failed> {
    let header = Cleared(base64url_decode(header).ok_or(Failed::Token)?);
    let header = Document::of(serde_json::from_slice(&header.0).map_err(|_| Failed::Token)?);
    let header = header.read();
    let fields = header.as_object().ok_or(Failed::Token)?;
    if fields.get("alg").and_then(serde_json::Value::as_str) != Some(ALGORITHM)
        || fields.contains_key("crit")
    {
        return Err(Failed::Token);
    }
    fields
        .get("kid")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .ok_or(Failed::Token)
}

/// The key set, fetched from the host compiled in for it, with no credential and following no
/// redirect.
fn key_set(agent: &ureq::Agent, hosts: &Hosts) -> Result<Document, Failed> {
    let mut response = agent
        .get(&hosts.keys)
        .header("accept", "application/json")
        .call()
        .map_err(|_| Failed::KeySet(Class::Transport))?;
    let status = response.status().as_u16();
    if !(200..300).contains(&status) {
        return Err(Failed::KeySet(class_of(status)));
    }
    let text = response
        .body_mut()
        .with_config()
        .limit(KEY_SET_LIMIT)
        .read_to_string()
        .map_err(|_| Failed::KeySet(Class::Incomplete))?;
    let parsed: serde_json::Value =
        serde_json::from_str(&text).map_err(|_| Failed::KeySet(Class::Incomplete))?;
    if !parsed.get("keys").is_some_and(serde_json::Value::is_array) {
        return Err(Failed::KeySet(Class::Incomplete));
    }
    Ok(Document::of(parsed))
}

/// The modulus and exponent of the one key in `keys` named `kid` that is an RSA key for RS256
/// signatures, or `None` where there is not exactly one.
fn key(keys: &serde_json::Value, kid: &str) -> Option<(Vec<u8>, Vec<u8>)> {
    let text = |key: &serde_json::Value, name: &str| {
        key.get(name)
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
    };
    let mut named = keys
        .get("keys")?
        .as_array()?
        .iter()
        .filter(|key| text(key, "kid").as_deref() == Some(kid));
    let key = named.next()?;
    if named.next().is_some() {
        return None;
    }
    let for_signing = text(key, "kty").as_deref() == Some("RSA")
        && key
            .get("use")
            .is_none_or(|used| used.as_str() == Some("sig"))
        && key
            .get("alg")
            .is_none_or(|alg| alg.as_str() == Some(ALGORITHM))
        && key.get("key_ops").is_none_or(|operations| {
            operations
                .as_array()
                .is_some_and(|listed| listed.iter().any(|op| op.as_str() == Some("verify")))
        });
    if !for_signing {
        return None;
    }
    let n = base64url_decode(&text(key, "n")?)?;
    let e = base64url_decode(&text(key, "e")?)?;
    Some((n, e))
}

/// Whether the five claims the sign-in compares hold. No other claim is read.
fn claims_hold(claims: &serde_json::Value, client: &str, nonce: &Secret, now: u64) -> bool {
    let text = |name: &str| claims.get(name).and_then(serde_json::Value::as_str);
    let audience = match claims.get("aud") {
        Some(serde_json::Value::String(one)) => one == client,
        Some(serde_json::Value::Array(several)) => {
            several.iter().any(|one| one.as_str() == Some(client))
        }
        _ => false,
    };
    let unexpired = claims
        .get("exp")
        .and_then(serde_json::Value::as_u64)
        .is_some_and(|expires| expires > now);
    let nonce_sent =
        text("nonce").is_some_and(|said| same(said.as_bytes(), nonce.expose().as_bytes()));
    let subject = text("sub").is_some_and(|subject| !subject.is_empty());

    text("iss") == Some(ISSUER) && audience && unexpired && nonce_sent && subject
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set_of(keys: &[serde_json::Value]) -> serde_json::Value {
        serde_json::json!({ "keys": keys })
    }

    /// An RSA key for RS256 signatures under `kid`.
    fn rsa(kid: &str) -> serde_json::Value {
        serde_json::json!({
            "kty": "RSA",
            "kid": kid,
            "use": "sig",
            "alg": "RS256",
            "n": "AQAB",
            "e": "AQAB",
        })
    }

    /// The key is the one the `kid` names, and only an RSA key a signature may be checked with is
    /// used: one for encryption, for another algorithm, for operations that do not include verifying,
    /// or of another type is not. Two keys under one `kid` are neither, since the set does not say
    /// which signed. A key with no `use`, `alg` or `key_ops` says nothing against signing.
    #[test]
    fn only_an_rsa_key_for_rs256_signatures_named_by_the_kid_is_used() {
        let usable = set_of(&[rsa("other"), rsa("this")]);
        assert_eq!(key(&usable, "this"), Some((vec![1, 0, 1], vec![1, 0, 1])));
        assert_eq!(key(&usable, "missing"), None);

        let mut bare = rsa("this");
        for field in ["use", "alg"] {
            bare.as_object_mut().expect("an object").remove(field);
        }
        assert!(key(&set_of(&[bare]), "this").is_some());

        for (field, value) in [
            ("use", serde_json::json!("enc")),
            ("alg", serde_json::json!("RS512")),
            ("alg", serde_json::json!("HS256")),
            ("kty", serde_json::json!("EC")),
            ("kty", serde_json::json!("oct")),
            ("key_ops", serde_json::json!(["encrypt"])),
            ("key_ops", serde_json::json!("verify")),
            ("n", serde_json::json!("AQAB=")),
            ("e", serde_json::json!(65537)),
        ] {
            let mut changed = rsa("this");
            changed[field] = value.clone();
            assert_eq!(key(&set_of(&[changed]), "this"), None, "{field} {value}");
        }
        let mut verifying = rsa("this");
        verifying["key_ops"] = serde_json::json!(["verify"]);
        assert!(key(&set_of(&[verifying]), "this").is_some());

        assert_eq!(key(&set_of(&[rsa("this"), rsa("this")]), "this"), None);
    }
}
