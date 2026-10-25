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
/// spec was written from. It equals the `issuer` of the service's discovery document, and no ID
/// token's `iss` is recorded.
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
        let bytes = self.0.as_mut_slice();
        bytes.fill(0);
        std::hint::black_box(&*bytes);
        #[cfg(test)]
        CLEARED.with(|log| {
            log.borrow_mut()
                .push((bytes.len(), bytes.iter().all(|byte| *byte == 0)));
        });
    }
}

#[cfg(test)]
thread_local! {
    /// What each [`Cleared`] that went on this thread held: how long it was, and whether every byte
    /// of it was zero by the time it went. A test reads it to see which parts of a token were held in
    /// one.
    static CLEARED: std::cell::RefCell<Vec<(usize, bool)>> =
        const { std::cell::RefCell::new(Vec::new()) };
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
    // The modulus has to be 2048 bits or more, which the signature check's parameters require of it,
    // and the exponent 65537 or more, which they do not: they take 3.
    exponent_is_large_enough(&e).then_some((n, e))
}

/// Whether a public exponent, big-endian, is 65537 or more, the least FIPS 186-5 allows.
fn exponent_is_large_enough(exponent: &[u8]) -> bool {
    let at = exponent.iter().take_while(|byte| **byte == 0).count();
    let significant = &exponent[at..];
    match significant.len() {
        0..=2 => false,
        3 => significant >= [1u8, 0, 1].as_slice(),
        _ => true,
    }
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
    use super::super::encoding::base64url;
    use super::super::stand_in::{Reply, serve};
    use super::super::stand_in_key::{
        KID, another_public, header, key_set, public, sign_input, signed, signed_by_another_key,
    };
    use super::*;
    use hmac::Mac;
    use serde_json::{Value, json};
    use sha2::{Sha256, Sha512};

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

    // The tests below hand `verify` a token and a key set and read what it said, so each is one
    // rule of the check made visible. The key set is served by the stand-in, which counts the
    // requests it took: a token the header refuses sends none.

    const CLIENT: &str = "issued-client";
    const NONCE: &str = "the-nonce-of-this-sign-in";
    const NOW: u64 = 1_800_000_000;

    /// The claims of a token this sign-in accepts at [`NOW`].
    fn claims() -> Value {
        json!({
            "iss": ISSUER,
            "aud": CLIENT,
            "exp": NOW + 3600,
            "nonce": NONCE,
            "sub": "a-subject",
        })
    }

    /// What checking `token` at `now` against the key set `keys` came to, and how many requests the
    /// server of the key set took.
    fn checked_at(token: &str, keys: &str, now: u64) -> (Result<(), Failed>, usize) {
        let server = serve(vec![Reply::json(200, keys)]);
        let agent = super::super::exchange::agent(ureq::Agent::config_builder());
        let result = verify(
            &agent,
            &Hosts::at(&server.base),
            &Secret::new(token),
            CLIENT,
            &Secret::new(NONCE),
            now,
        );
        (result, server.finish().len())
    }

    /// [`checked_at`] at [`NOW`] against the key set that holds the key tokens are signed with.
    fn checked(token: &str) -> (Result<(), Failed>, usize) {
        checked_at(token, &key_set(), NOW)
    }

    /// A token with `claims` under the header a token this sign-in accepts has, signed with the key.
    fn with(claims: &Value) -> String {
        signed(&header(), claims)
    }

    /// Assert that `token` is refused, in the step the header, the signature or a claim is, and that
    /// the key set was asked for `asked` times.
    fn assert_refused(case: &str, token: &str, asked: usize) {
        let (result, requests) = checked(token);
        assert_eq!(result, Err(Failed::Token), "{case}: {token}");
        assert_eq!(requests, asked, "{case}: the key set was asked for");
    }

    /// `claims` with the one claim `name` set to `value`, or removed where `value` is `None`.
    fn changing(name: &str, value: Option<Value>) -> Value {
        let mut changed = claims();
        match value {
            Some(value) => changed[name] = value,
            None => drop(changed.as_object_mut().map(|claims| claims.remove(name))),
        }
        changed
    }

    /// The text of `header` and `payload` as a token holds them, signed as they are spelled.
    fn spelled(header: &str, payload: &str) -> String {
        let input = format!("{header}.{payload}");
        format!("{input}.{}", sign_input(&input))
    }

    /// `bytes` in the standard alphabet of RFC 4648, padded or not.
    fn standard(bytes: &[u8], padded: bool) -> String {
        const ALPHABET: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for chunk in bytes.chunks(3) {
            let quad = chunk.iter().enumerate().fold(0u32, |quad, (at, byte)| {
                quad | u32::from(*byte) << (16 - 8 * at)
            });
            for at in 0..=chunk.len() {
                out.push(ALPHABET[(quad >> (18 - 6 * at) & 63) as usize] as char);
            }
            if padded {
                out.push_str(&"=".repeat(3 - chunk.len()));
            }
        }
        out
    }

    /// A token that verifies, to start from: the key set signed it, its claims are this sign-in's
    /// and its expiry is a second past the clock. Asking for the key set once is all it takes.
    #[test]
    fn a_token_the_key_set_signs_for_this_sign_in_verifies_with_one_request() {
        for expires in [NOW + 1, NOW + 3600, u64::MAX] {
            let (result, asked) = checked(&with(&changing("exp", Some(json!(expires)))));
            assert_eq!(result, Ok(()), "{expires}");
            assert_eq!(asked, 1);
        }
    }

    /// The header names the algorithm, RS256 spelled so, and the key, by a string `kid`, or the token
    /// is refused before the key set is asked for. Each case is a header one rule alone refuses: a
    /// key set that holds the one key does not stand in for a missing `kid`, a lowercase `rs256` is
    /// not the algorithm, and a header that is not an object is refused and does not stop the check.
    #[test]
    fn a_header_that_names_no_key_or_not_rs256_or_is_not_an_object_is_refused_before_the_key_set() {
        let cases: Vec<(&str, Value)> = vec![
            ("no kid", json!({"alg": "RS256", "typ": "JWT"})),
            ("a kid that is a number", json!({"alg": "RS256", "kid": 7})),
            ("a kid that is null", json!({"alg": "RS256", "kid": null})),
            (
                "a kid that is a list",
                json!({"alg": "RS256", "kid": [KID]}),
            ),
            ("no alg", json!({"kid": KID})),
            ("a lowercase alg", json!({"alg": "rs256", "kid": KID})),
            ("an alg with a space", json!({"alg": "RS256 ", "kid": KID})),
            ("an alg that is a number", json!({"alg": 256, "kid": KID})),
            (
                "an alg that is a list",
                json!({"alg": ["RS256"], "kid": KID}),
            ),
            ("an alg that is null", json!({"alg": null, "kid": KID})),
            ("PS256", json!({"alg": "PS256", "kid": KID})),
            ("ES256", json!({"alg": "ES256", "kid": KID})),
            ("an empty object", json!({})),
            ("a list", json!([])),
            ("a list that holds the algorithm", json!(["RS256", KID])),
            ("a string", json!("RS256")),
            ("a number", json!(7)),
            ("null", json!(null)),
            ("a boolean", json!(true)),
        ];
        for (case, header) in cases {
            assert_refused(case, &signed(&header, &claims()), 0);
        }
        // A kid is a string that names a key: one the set does not hold is a request and a refusal.
        assert_refused(
            "an empty kid",
            &signed(&json!({"alg": "RS256", "kid": ""}), &claims()),
            1,
        );
    }

    /// A header may carry `jku`, `x5u`, `jwk` and `x5c`, each of which says where a key is or is the
    /// key. None is followed or used: the key is the key set's, so a token signed with the key set's
    /// key verifies with them present and the places they name get no request, and a token signed
    /// with another key is refused though the header carries that key's public half.
    #[test]
    fn a_key_the_header_names_or_carries_is_never_the_key_the_signature_is_checked_with() {
        let elsewhere = serve(vec![]);
        let (n, e) = another_public();
        let header = json!({
            "alg": "RS256",
            "kid": KID,
            "jku": format!("{}/keys", elsewhere.base),
            "x5u": format!("{}/chain", elsewhere.base),
            "jwk": {"kty": "RSA", "alg": "RS256", "use": "sig", "n": base64url(&n), "e": base64url(&e)},
            "x5c": [standard(b"not a certificate", true)],
        });

        let (result, asked) = checked(&signed(&header, &claims()));
        assert_eq!(
            result,
            Ok(()),
            "the key set's own key, with key locations in the header"
        );
        assert_eq!(asked, 1);

        let (result, asked) = checked(&signed_by_another_key(&header, &claims()));
        assert_eq!(
            result,
            Err(Failed::Token),
            "a token signed with the key its own header carries was accepted"
        );
        assert_eq!(asked, 1);
        assert!(
            elsewhere.finish().is_empty(),
            "a place the header named was asked for"
        );
    }

    /// The key set the service publishes holds several keys, and some entries carry an `x5c` list that
    /// no check reads. A token names one entry by its `kid`. That entry is the key used, whatever its
    /// place in the list and whatever else it holds, and a header that names another entry is checked
    /// against that entry's key. The older tests give the reader one key, or keys that share a modulus,
    /// with exactly the fields it reads. This test rejects a reader that refuses an entry for a field it
    /// does not read, which would refuse every sign-in. It rejects a reader that takes the modulus of an
    /// entry other than the named one, which would verify under the wrong key. It rejects a reader that
    /// accepts a signature from any key of the set, which would accept a token under a key its header
    /// does not name.
    #[test]
    fn a_key_set_of_several_keys_some_with_an_x5c_list_gives_the_key_the_kid_names_and_no_other() {
        let (n, e) = public();
        let (other_n, other_e) = another_public();
        let entry = |kid: &str, n: &[u8], e: &[u8]| {
            json!({
                "kty": "RSA",
                "use": "sig",
                "alg": "RS256",
                "kid": kid,
                "n": base64url(n),
                "e": base64url(e),
            })
        };
        // Four entries. The key tokens are signed with is the second. The first and the second carry an
        // `x5c` list and the other two carry none.
        let mut keys = vec![
            entry("first-other-key", &other_n, &other_e),
            entry(KID, &n, &e),
            entry("second-other-key", &other_n, &other_e),
            entry("third-other-key", &other_n, &other_e),
        ];
        for listed in &mut keys[..2] {
            listed["x5c"] = json!([standard(b"not a certificate", true)]);
        }
        let set = set_of(&keys);
        assert_eq!(key(&set, KID), Some((n, e)), "the entry the kid names");
        assert_eq!(
            key(&set, "second-other-key"),
            Some((other_n, other_e)),
            "the entry a header that names another key points at"
        );

        let served = set.to_string();
        assert_eq!(
            checked_at(&with(&claims()), &served, NOW),
            (Ok(()), 1),
            "a token signed with the key its kid names"
        );
        let names_another = signed(
            &json!({"alg": "RS256", "kid": "second-other-key", "typ": "JWT"}),
            &claims(),
        );
        assert_eq!(
            checked_at(&names_another, &served, NOW),
            (Err(Failed::Token), 1),
            "a token signed with a key of the set that its header does not name"
        );
    }

    /// The expiry is a whole number of seconds that is later than the clock. A missing one, a string,
    /// a fraction, a negative number, a value that is not a number, and the instant of the clock
    /// itself are each refused, and one second later is not.
    #[test]
    fn an_expiry_that_is_missing_not_a_whole_number_or_not_after_now_is_refused() {
        let cases: Vec<(&str, Option<Value>)> = vec![
            ("missing", None),
            (
                "a string of the instant",
                Some(json!((NOW + 3600).to_string())),
            ),
            ("a fraction", Some(json!(NOW as f64 + 3600.5))),
            (
                "a whole number written as a fraction",
                Some(json!(NOW as f64 + 3600.0)),
            ),
            ("negative", Some(json!(-1))),
            ("negative and large", Some(json!(-(NOW as i64) - 3600))),
            ("null", Some(Value::Null)),
            ("a boolean", Some(json!(true))),
            ("a list", Some(json!([NOW + 3600]))),
            ("an object", Some(json!({"exp": NOW + 3600}))),
            ("the instant of the clock", Some(json!(NOW))),
            ("a second before the clock", Some(json!(NOW - 1))),
            ("zero", Some(json!(0))),
        ];
        for (case, exp) in cases {
            assert_refused(case, &with(&changing("exp", exp)), 1);
        }
    }

    /// The audience is a string that is the client, or a list that holds a string that is. A missing
    /// one, a number, `null`, an empty list, a list of values that are not strings, a list of other
    /// clients, a string that holds the client without being it, and another case are each refused.
    #[test]
    fn an_audience_that_is_not_the_client_or_a_list_holding_it_is_refused() {
        let cases: Vec<(&str, Option<Value>)> = vec![
            ("missing", None),
            ("a number", Some(json!(7))),
            ("null", Some(Value::Null)),
            ("a boolean", Some(json!(true))),
            ("an empty list", Some(json!([]))),
            ("a list of numbers", Some(json!([1, 2]))),
            (
                "a list of values that are not strings",
                Some(json!([1, null, true, {"aud": CLIENT}])),
            ),
            ("a list in a list", Some(json!([[CLIENT]]))),
            (
                "a list of other clients",
                Some(json!(["another", "a-third"])),
            ),
            ("an object", Some(json!({CLIENT: true}))),
            ("another case", Some(json!(CLIENT.to_uppercase()))),
            ("the client and a space", Some(json!(format!("{CLIENT} ")))),
            (
                "a string that starts with the client",
                Some(json!(format!("{CLIENT}-2"))),
            ),
            (
                "a string that ends with the client",
                Some(json!(format!("my-{CLIENT}"))),
            ),
            (
                "a list of such strings",
                Some(json!([format!("{CLIENT}-2"), format!("my-{CLIENT}")])),
            ),
            ("an empty string", Some(json!(""))),
        ];
        for (case, aud) in cases {
            assert_refused(case, &with(&changing("aud", aud)), 1);
        }
        for (case, aud) in [
            ("the client", json!(CLIENT)),
            ("a list of the client", json!([CLIENT])),
            ("a list that ends with it", json!(["another", CLIENT])),
            ("a list that starts with it", json!([CLIENT, "another"])),
            (
                "a list with values that are not strings beside it",
                json!([7, null, CLIENT]),
            ),
        ] {
            let (result, _) = checked(&with(&changing("aud", Some(aud))));
            assert_eq!(result, Ok(()), "{case}");
        }
    }

    /// The issuer is the constant, and the nonce is the one this sign-in sent, each as a string and
    /// as the whole of it; the subject is a string that is not empty. A claim that is another type,
    /// missing, empty or a longer or shorter text is refused.
    #[test]
    fn an_issuer_a_subject_and_a_nonce_that_are_not_strings_or_not_the_expected_ones_are_refused() {
        let not_a_string = |expected: &str| -> Vec<(String, Option<Value>)> {
            vec![
                ("missing".to_string(), None),
                ("a number".to_string(), Some(json!(7))),
                ("null".to_string(), Some(Value::Null)),
                ("a boolean".to_string(), Some(json!(true))),
                ("a list holding it".to_string(), Some(json!([expected]))),
                ("an object".to_string(), Some(json!({"value": expected}))),
            ]
        };
        for (name, expected, texts) in [
            (
                "iss",
                ISSUER,
                vec![
                    String::new(),
                    format!("{ISSUER}/"),
                    ISSUER.replace("https", "http"),
                    ISSUER.to_uppercase(),
                    format!("{ISSUER}.invalid"),
                    ISSUER[..ISSUER.len() - 1].to_string(),
                ],
            ),
            (
                "nonce",
                NONCE,
                vec![
                    String::new(),
                    format!("{NONCE}x"),
                    NONCE[..NONCE.len() - 1].to_string(),
                    NONCE.to_uppercase(),
                    format!("{NONCE} "),
                ],
            ),
            ("sub", "a-subject", vec![String::new()]),
        ] {
            let mut cases = not_a_string(expected);
            cases.extend(
                texts
                    .into_iter()
                    .map(|text| (format!("the text {text:?}"), Some(json!(text)))),
            );
            for (case, value) in cases {
                assert_refused(&format!("{name}: {case}"), &with(&changing(name, value)), 1);
            }
        }
        // A subject is any text that is not empty: nothing says what it looks like.
        let (result, _) = checked(&with(&changing("sub", Some(json!(" ")))));
        assert_eq!(result, Ok(()));
    }

    /// A token is spelled as RFC 7515 writes it: base64url, no padding, and no bit after the last
    /// byte that is not zero, in each of its three parts. Padding, the standard alphabet and stray
    /// bits are other spellings of the same bytes. The signature covers the spelling of the header and
    /// the payload, so a token spelled one of those ways here is signed as spelled and it is the
    /// spelling that is refused. The third part is not covered by the signature, so an accepted
    /// spelling of it would be a second token that verifies.
    #[test]
    fn a_token_spelled_with_padding_the_standard_alphabet_or_stray_bits_is_refused_in_every_part() {
        let alphabet = |text: &str| text.replace('-', "+").replace('_', "/");
        // A text of the part with a filler that is long enough for its last group to be short, and
        // starts with characters that fill the places the alphabets differ in, wherever it falls.
        let with_filler = |mut value: Value| {
            let mut filler = "~~~~~~~~~???????????".to_string();
            loop {
                value["filler"] = json!(filler);
                let text = value.to_string();
                if text.len() % 3 == 1 {
                    return text;
                }
                filler.push('x');
            }
        };
        let header_text = with_filler(json!({"alg": "RS256", "kid": KID}));
        let payload_text = with_filler(claims());
        let (header_bytes, payload_bytes) = (header_text.as_bytes(), payload_text.as_bytes());
        let (header_url, payload_url) = (base64url(header_bytes), base64url(payload_bytes));
        assert!(
            header_url.contains(['-', '_']) && payload_url.contains(['-', '_']),
            "the parts do not hold the characters the alphabets differ in"
        );
        let (header_std, payload_std) = (
            standard(header_bytes, false),
            standard(payload_bytes, false),
        );
        assert_ne!(header_url, header_std);
        assert_ne!(payload_url, payload_std);
        assert!(
            header_bytes.len() % 3 != 0 && payload_bytes.len() % 3 != 0,
            "a part that is a whole number of groups has no padding to add"
        );
        // The unpadded form of each part is a spelling the check accepts, as a control.
        assert_eq!(checked(&spelled(&header_url, &payload_url)).0, Ok(()));

        let signature = sign_input(&format!("{header_url}.{payload_url}"));
        // The last character of a part whose bytes are not a whole number of groups holds bits after
        // the last byte, which are zero: four of them in each of the three parts here. Set the lowest.
        let with_a_stray_bit = |part: &str| {
            let last = part.chars().last().expect("a part");
            let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
            let at = alphabet
                .iter()
                .position(|character| *character as char == last)
                .expect("a character of the alphabet");
            let other = alphabet[at ^ 1] as char;
            format!("{}{other}", &part[..part.len() - 1])
        };
        let strays = with_a_stray_bit(&signature);
        assert_ne!(strays, signature);
        let signature_std = alphabet(&signature);
        assert_ne!(
            signature_std, signature,
            "the signature has no character to respell"
        );

        let whole = |header: &str, payload: &str, signature: &str| {
            format!("{header}.{payload}.{signature}")
        };
        let cases: Vec<(&str, String)> = vec![
            (
                "a header padded",
                spelled(
                    &standard(header_bytes, true)
                        .replace('+', "-")
                        .replace('/', "_"),
                    &payload_url,
                ),
            ),
            (
                "a header in the standard alphabet",
                spelled(&header_std, &payload_url),
            ),
            (
                "a header padded in the standard alphabet",
                spelled(&standard(header_bytes, true), &payload_url),
            ),
            (
                "a payload padded",
                spelled(
                    &header_url,
                    &standard(payload_bytes, true)
                        .replace('+', "-")
                        .replace('/', "_"),
                ),
            ),
            (
                "a payload in the standard alphabet",
                spelled(&header_url, &payload_std),
            ),
            (
                "a payload padded in the standard alphabet",
                spelled(&header_url, &standard(payload_bytes, true)),
            ),
            (
                "a signature padded",
                whole(&header_url, &payload_url, &format!("{signature}==")),
            ),
            (
                "a signature in the standard alphabet",
                whole(&header_url, &payload_url, &signature_std),
            ),
            (
                "a signature with a bit set after its last byte",
                whole(&header_url, &payload_url, &strays),
            ),
            (
                "a header with a bit set after its last byte",
                spelled(&with_a_stray_bit(&header_url), &payload_url),
            ),
            (
                "a payload with a bit set after its last byte",
                spelled(&header_url, &with_a_stray_bit(&payload_url)),
            ),
            (
                "a line break after the token",
                format!("{}\n", whole(&header_url, &payload_url, &signature)),
            ),
            (
                "a space before the token",
                format!(" {}", whole(&header_url, &payload_url, &signature)),
            ),
            (
                "a line break inside the signature",
                whole(
                    &header_url,
                    &payload_url,
                    &format!("{}\n{}", &signature[..100], &signature[100..]),
                ),
            ),
        ];
        for (case, token) in cases {
            let (result, _) = checked(&token);
            assert_eq!(result, Err(Failed::Token), "{case}: {token}");
        }
    }

    /// A payload that is signed and is not an object is refused. A list that holds the claims in the
    /// order a structure would take them is one a reader that fills a structure from a list would
    /// accept, so the lists are the orders such a reader is likely to use.
    #[test]
    fn a_payload_that_is_signed_and_is_not_an_object_of_claims_is_refused() {
        let (iss, aud, exp, nonce, sub) = (ISSUER, CLIENT, NOW + 3600, NONCE, "a-subject");
        let payloads: Vec<(&str, Value)> = vec![
            (
                "a list of the claims in the order they are compared",
                json!([iss, aud, exp, nonce, sub]),
            ),
            (
                "a list of the claims in the order they are written",
                json!([iss, sub, aud, exp, nonce]),
            ),
            (
                "a list of the claims with the time first",
                json!([exp, iss, aud, nonce, sub]),
            ),
            ("a list of one object of the claims", json!([claims()])),
            ("an empty list", json!([])),
            ("an empty object", json!({})),
            ("a string", json!("a string")),
            ("a number", json!(7)),
            ("null", json!(null)),
            ("a boolean", json!(true)),
            (
                "a string that holds the claims",
                json!(claims().to_string()),
            ),
        ];
        for (case, payload) in payloads {
            assert_refused(case, &signed(&header(), &payload), 1);
        }
    }

    /// A token that names an HMAC and is keyed with a spelling of the key's public half is the
    /// confusion between a signature and a MAC: a verifier that took its algorithm from the header and
    /// its secret from the key it holds would accept it. It is refused for its header, before the key
    /// set is asked for, and a token that names RS256 and carries a MAC is refused for its signature.
    #[test]
    fn a_token_keyed_with_the_public_key_as_an_hmac_secret_is_refused() {
        fn der(tag: u8, body: &[u8]) -> Vec<u8> {
            let mut out = vec![tag];
            match body.len() {
                length @ 0..=127 => out.push(length as u8),
                length => {
                    let bytes: Vec<u8> = length
                        .to_be_bytes()
                        .into_iter()
                        .skip_while(|byte| *byte == 0)
                        .collect();
                    out.push(0x80 | bytes.len() as u8);
                    out.extend(bytes);
                }
            }
            out.extend(body);
            out
        }
        fn integer(bytes: &[u8]) -> Vec<u8> {
            let mut value: Vec<u8> = bytes
                .iter()
                .copied()
                .skip_while(|byte| *byte == 0)
                .collect();
            if value.first().is_some_and(|byte| byte & 0x80 != 0) {
                value.insert(0, 0);
            }
            der(0x02, &value)
        }

        let (n, e) = public();
        let pkcs1 = der(0x30, &[integer(&n), integer(&e)].concat());
        let spki = der(
            0x30,
            &[
                der(
                    0x30,
                    &[
                        // rsaEncryption, 1.2.840.113549.1.1.1, and the NULL parameters.
                        vec![
                            0x06, 0x09, 0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x01, 0x01,
                        ],
                        vec![0x05, 0x00],
                    ]
                    .concat(),
                ),
                der(0x03, &[vec![0], pkcs1.clone()].concat()),
            ]
            .concat(),
        );
        let pem = {
            let body = standard(&spki, true);
            let lines: Vec<&str> = body
                .as_bytes()
                .chunks(64)
                .map(|line| std::str::from_utf8(line).expect("text"))
                .collect();
            format!(
                "-----BEGIN PUBLIC KEY-----\n{}\n-----END PUBLIC KEY-----\n",
                lines.join("\n")
            )
        };
        let secrets: Vec<(&str, Vec<u8>)> = vec![
            ("the modulus", n.clone()),
            (
                "the modulus and the exponent",
                [n.clone(), e.clone()].concat(),
            ),
            ("the key as PKCS#1", pkcs1),
            ("the key as SubjectPublicKeyInfo", spki),
            ("the key as PEM", pem.into_bytes()),
            (
                "the modulus as the key set writes it",
                base64url(&n).into_bytes(),
            ),
            ("the key set", key_set().into_bytes()),
        ];
        for (named, secret) in &secrets {
            for algorithm in ["HS256", "HS512"] {
                let hmac_header = json!({"alg": algorithm, "kid": KID}).to_string();
                let input = format!(
                    "{}.{}",
                    base64url(hmac_header.as_bytes()),
                    base64url(claims().to_string().as_bytes())
                );
                let mac = if algorithm == "HS256" {
                    let mut mac = hmac::Hmac::<Sha256>::new_from_slice(secret).expect("a key");
                    mac.update(input.as_bytes());
                    mac.finalize().into_bytes().to_vec()
                } else {
                    let mut mac = hmac::Hmac::<Sha512>::new_from_slice(secret).expect("a key");
                    mac.update(input.as_bytes());
                    mac.finalize().into_bytes().to_vec()
                };
                let token = format!("{input}.{}", base64url(&mac));
                assert_refused(&format!("{algorithm} keyed with {named}"), &token, 0);

                // The same bytes under a header that says RS256 are a signature that is not one.
                let input = format!(
                    "{}.{}",
                    base64url(header().to_string().as_bytes()),
                    base64url(claims().to_string().as_bytes())
                );
                let token = format!("{input}.{}", base64url(&mac));
                assert_refused(&format!("{named} as the signature"), &token, 1);
            }
        }
    }

    /// An RS256 signature is as long as the modulus. A longer one, which a reader that took it as a
    /// number would read as the same number with a zero in front, a shorter one, an empty one, one made
    /// of the same byte, and one twice as long are each refused.
    #[test]
    fn a_signature_of_another_length_than_the_modulus_is_refused() {
        let token = with(&claims());
        let (signed_part, signature) = token.rsplit_once('.').expect("three parts");
        let signature = base64url_decode(signature).expect("a signature");
        let (n, _) = public();
        assert_eq!(signature.len(), n.len());
        let with_signature = |bytes: &[u8]| format!("{signed_part}.{}", base64url(bytes));
        assert_eq!(checked(&with_signature(&signature)).0, Ok(()));

        let cases: Vec<(&str, Vec<u8>)> = vec![
            ("a zero in front", [vec![0], signature.clone()].concat()),
            ("a zero behind", [signature.clone(), vec![0]].concat()),
            ("a byte short", signature[..signature.len() - 1].to_vec()),
            ("the first byte short", signature[1..].to_vec()),
            ("empty", Vec::new()),
            ("one byte", vec![1]),
            (
                "twice as long",
                [signature.clone(), signature.clone()].concat(),
            ),
            ("all zero", vec![0; signature.len()]),
            ("all one bits", vec![0xff; signature.len()]),
            ("the modulus, which is not below it", n.clone()),
        ];
        for (case, bytes) in cases {
            let (result, asked) = checked(&with_signature(&bytes));
            assert_eq!(result, Err(Failed::Token), "{case}");
            assert_eq!(asked, 1, "{case}");
        }
    }

    /// The key a signature is checked with has a modulus of 2048 bits or more and an exponent of
    /// 65537 or more. A key of 1024 bits and one with the exponent 3 each signed a token whose
    /// claims are this sign-in's, and both are refused. The signatures were made with
    /// `openssl dgst -sha256 -sign` and verify with `openssl dgst -sha256 -verify`, so what is
    /// refused is the key. The signature check's own parameters refuse the short modulus and take the
    /// exponent 3, which is why the exponent is a rule of its own here.
    #[test]
    fn a_key_of_fewer_than_2048_bits_or_with_a_small_exponent_is_refused_whatever_its_signature_is()
    {
        struct Weak {
            kid: &'static str,
            n: &'static str,
            e: &'static str,
            header: &'static str,
            payload: &'static str,
            signature: &'static str,
        }
        let weak = [
            Weak {
                kid: "weak-short",
                n: "wdFXnBc9Z203gWA5Dvm1Lp8lKsHdaiZwwP2rtzq_17N6mk3af9u6fSClmQrIKa233qCHaSWBF2aVn8Q5AAO3aeuL3NTCCZAZQjrcU16alNXoL2OFVTwre6sc6SawwBNE4cXWgod2pA0U6h8spDU8FwoV1gEWMACCt1vemtDyWwU",
                e: "AQAB",
                header: "eyJhbGciOiJSUzI1NiIsImtpZCI6IndlYWstc2hvcnQiLCJ0eXAiOiJKV1QifQ",
                payload: "eyJpc3MiOiJodHRwczovL2F1dGgub3BlbmFpLmNvbSIsImF1ZCI6Imlzc3VlZC1jbGllbnQiLCJleHAiOjQxMDI0NDQ4MDAsIm5vbmNlIjoidGhlLW5vbmNlLW9mLXRoaXMtc2lnbi1pbiIsInN1YiI6ImEtc3ViamVjdCJ9",
                signature: "qX91Vb_uA_e4-NFvwb3TrSpmys0rIiQ0Oklot77ZyO-XO5RyP3JleX4wy2QaNHaAl9fLVk47oxMCLg6DdMA8mu6zV4bMj3a9Oh0UllWJIA8sr3kkr9dEhgtpbOg9bWf_HzHw0NemjcqTnf4oQ60p-6AqWOGhnekHs-euH3CK7lE",
            },
            Weak {
                kid: "weak-exponent",
                n: "n9PxNzi56heewMEYehDZlp885hDgq6Y9PaWf_jTxO2ZVh5WiBEiSIvbfNnKDPzxfQs9lSxjHMTI9sQMKFI6fSVXU_POzIV0RpmGW1YD7Pbnisn2nqU0pb8H6ErrtfSl6_t8fB7vgGatLTr-NHFeO3qkyaiYOM_RhdKCuM3FpfdCEThGgye5oL38f_AYY5qCdS_ZIjqgGsvdaWO3lzOinFrvs1IrFcwg4rccsn0BdPL1ZUYZN9CK1MUvov2s6-g4BZC8d_A7o7w7zztW2QWi7Y5OR38rlGUykg1Ty9ZDZOadNxzf1tBT0-9AaGwMMLh8Vnw5hdtfGK38jsdCkuOy3EQ",
                e: "Aw",
                header: "eyJhbGciOiJSUzI1NiIsImtpZCI6IndlYWstZXhwb25lbnQiLCJ0eXAiOiJKV1QifQ",
                payload: "eyJpc3MiOiJodHRwczovL2F1dGgub3BlbmFpLmNvbSIsImF1ZCI6Imlzc3VlZC1jbGllbnQiLCJleHAiOjQxMDI0NDQ4MDAsIm5vbmNlIjoidGhlLW5vbmNlLW9mLXRoaXMtc2lnbi1pbiIsInN1YiI6ImEtc3ViamVjdCJ9",
                signature: "L8d5U7rI-ey6xHo2omOd5v9yOqCUC2dOGSrmKHAubTHEfFT7sL2w3BPeZIkSS-NxPV_T5AB0g_AZZDcI3uuHp-5WdGVcFmKsfuBUAKwrG2TXoy5tlc_C7gUU3EdBLdLass5V9Fnz15HM_5Bx4twnyD6ubLzj3UexEoxWMhWCO55ip8hSK5cZc5Cn69Vd-7xJwaciMTSgn2mMPH8VFuARHMPdbxN_v5PmfCXH83-fXbEtxd1cuAk03-IMDmbCn4eTb3EMitgEttoqB26ni0O9LJ6axFwgelrR-2qAXI1-M6yXovVsYyoNxsp2BK2PrMJyAH74hg7EWKrzHDecpW08Jw",
            },
        ];
        for key in weak {
            let keys = json!({"keys": [{
                "kty": "RSA", "kid": key.kid, "use": "sig", "alg": "RS256", "n": key.n, "e": key.e,
            }]})
            .to_string();
            let token = format!("{}.{}.{}", key.header, key.payload, key.signature);
            // The claims in the token are this sign-in's, so a key the check accepted would verify it.
            let decoded = base64url_decode(key.payload).expect("a payload");
            let said: Value = serde_json::from_slice(&decoded).expect("claims");
            assert_eq!(said["nonce"], NONCE);
            assert_eq!(said["aud"], CLIENT);
            let (result, asked) = checked_at(&token, &keys, NOW);
            assert_eq!(result, Err(Failed::Token), "{}", key.kid);
            assert_eq!(asked, 1, "{}", key.kid);
        }
    }

    /// The exponent of a key is 65537 or more, which is how many bytes it takes and then which number
    /// it is: the least FIPS 186-5 allows, which a real key's `AQAB` is. 65536 is refused and 65537 and
    /// 65539 are not. A leading zero byte is not a longer number, so it neither lets a small exponent
    /// through nor refuses a large one, and an exponent of no bytes or of nothing but zero is none.
    #[test]
    fn an_exponent_of_65537_or_more_is_used_and_a_smaller_one_is_not_whatever_its_leading_zeros() {
        let with_exponent = |e: &str| {
            let mut entry = rsa("this");
            entry["e"] = json!(e);
            key(&set_of(&[entry]), "this").is_some()
        };
        for (written, bytes) in [
            ("AQAB", "65537, a real key's"),
            ("AQAD", "65539"),
            ("AQAAAA", "2^24, four bytes"),
            ("AAEAAQ", "65537 with a zero byte in front"),
            ("AAAAAQAB", "65537 with three"),
        ] {
            assert!(with_exponent(written), "{bytes} was refused");
        }
        // RFC 7518 writes the modulus and the exponent in the fewest bytes. A key written with a zero
        // byte in front of either is not one the signature check takes, though it is the same number,
        // so a token it signed is refused and not read as signed by a key it is not.
        let (n, e) = public();
        let padded = |bytes: &[u8]| base64url(&[&[0u8][..], bytes].concat());
        for (case, modulus, exponent) in [
            (
                "a modulus with a zero byte in front",
                padded(&n),
                base64url(&e),
            ),
            (
                "an exponent with a zero byte in front",
                base64url(&n),
                padded(&e),
            ),
        ] {
            let keys = json!({"keys": [{"kty": "RSA", "kid": KID, "use": "sig", "alg": "RS256",
                "n": modulus, "e": exponent}]})
            .to_string();
            let (result, asked) = checked_at(&with(&claims()), &keys, NOW);
            assert_eq!((result, asked), (Err(Failed::Token), 1), "{case}");
        }
        assert_eq!(
            checked(&with(&claims())).0,
            Ok(()),
            "the same key, as it is written"
        );

        for (written, bytes) in [
            ("Aw", "3"),
            ("AQAA", "65536"),
            ("__8", "65535"),
            ("AAAD", "3 with a zero byte in front"),
            ("AAAAAw", "3 with three"),
            ("AAEA", "256 with a zero byte in front"),
            ("AA", "0"),
            ("AAAA", "0 in three bytes"),
            ("", "no bytes"),
        ] {
            assert!(!with_exponent(written), "{bytes} was used");
        }
    }

    /// The decoded header and the decoded payload are each held in a buffer that is cleared where it
    /// goes, in each way a check ends: every buffer that was made was zero when it was dropped, a
    /// token the header refuses made the one for its header, one the signature refuses made that one
    /// and no more, and one whose claims are read made both, each as long as the text it holds.
    #[test]
    fn the_decoded_header_and_payload_are_held_in_buffers_that_are_cleared_in_each_way_a_check_ends()
     {
        let header_length = header().to_string().len();
        let payload_length = claims().to_string().len();
        let cleared = |token: &str| {
            CLEARED.with(|log| log.borrow_mut().clear());
            let _ = checked(token);
            CLEARED.with(|log| log.borrow().clone())
        };

        // Checked to the end, and accepted.
        assert_eq!(
            cleared(&with(&claims())),
            [(header_length, true), (payload_length, true)]
        );
        // Refused for a claim, after both were read.
        let another = changing("aud", Some(json!("another")));
        assert_eq!(
            cleared(&with(&another)),
            [(header_length, true), (another.to_string().len(), true)]
        );
        // Refused by the signature, before the payload is decoded.
        let (signed_part, _) = with(&claims())
            .rsplit_once('.')
            .map(|(signed_part, rest)| (signed_part.to_string(), rest.to_string()))
            .expect("three parts");
        assert_eq!(
            cleared(&format!("{signed_part}.{}", base64url(&[1u8; 256]))),
            [(header_length, true)]
        );
        // Refused by the header, which is all that was read.
        let other = json!({"alg": "none", "kid": KID});
        assert_eq!(
            cleared(&signed(&other, &claims())),
            [(other.to_string().len(), true)]
        );
        // A header that is not JSON was a buffer too.
        assert_eq!(
            cleared(&format!("{}.e30.AA", base64url(b"not json"))),
            [(8, true)]
        );
    }

    /// The decoded header and payload are overwritten where they lie when the check is done with
    /// them: what a buffer held is zero after it is dropped. A buffer that was dropped without being
    /// overwritten is logged as one that was not, which is what a `Cleared` that stopped writing the
    /// zeros would leave.
    #[test]
    fn a_decoded_part_is_overwritten_where_it_lies() {
        let part = br#"{"sub":"a-subject"}"#.to_vec();
        let length = part.len();
        CLEARED.with(|log| log.borrow_mut().clear());

        drop(Cleared(part));

        assert_eq!(CLEARED.with(|log| log.borrow().clone()), [(length, true)]);
    }
}
