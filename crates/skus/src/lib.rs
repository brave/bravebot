//! Signing in to the model services that sign in with an account, and keeping the sign-ins.
//!
//! # Why this crate has no labels
//!
//! Everything else that reaches the network in this repository carries a [`bravebot_core`] label and
//! passes a policy gate. This crate deliberately does neither, and the reason is that there is
//! nothing here for a gate to protect.
//!
//! A label answers "may this value influence what happens next", and the whole apparatus exists
//! because a turn mixes the user's instructions with bytes an attacker may have written. A sign-in
//! runs before any of that: a person types `bravebot auth login` at a shell, no planner exists, no
//! model has a context, and no untrusted document is in play. The endpoints are compiled-in
//! constants, and the result goes into a file in the user's own directory. Nothing an attacker
//! controls is anywhere in that path.
//!
//! So this is provisioning, not a turn. Do not wire it into [`bravebot_net::Egress`] or hand it a
//! `Policy`: that would add ceremony that protects nothing and would suggest, wrongly, that a
//! sign-in is the kind of thing the information-flow rules were written about.

#![deny(unsafe_code)]

// The seam that points the ChatGPT and xAI sign-ins at a stand-in exists for tests, and a release build
// that turned it on, by `--all-features` or a manifest, would let a variable choose where a token goes.
// Rust has no `cfg` for the release profile, so the refusal reads the two settings that profile has
// and a test build does not: no debug assertions, and a panic that aborts. A release with
// `debug-assertions = true` aborts on a panic, and `cargo test --release` unwinds and has no assertions,
// so each is refused by one of the two. `crates/cli/tests/layering.rs` compiles this line under each
// combination and reads the root manifest for the release profile's `panic = "abort"`.
#[cfg(all(feature = "test-hosts", any(not(debug_assertions), panic = "abort")))]
compile_error!(
    "the test-hosts feature is for tests: it is refused where debug assertions are off or a panic aborts, as in a release build"
);

pub mod chatgpt;
pub mod oauth;
pub mod secret;
pub mod store;
#[cfg(test)]
mod testutil;
pub mod xai;

pub use secret::Secret;
pub use store::StoreError;

/// A random uuid, for an identifier that must not be reused or derived from anything predictable.
///
/// Written out rather than adding a uuid dependency for one value; only the randomness matters, and
/// that comes from the OS.
pub fn new_request_id() -> String {
    let mut bytes = [0u8; 16];
    fill_random(&mut bytes);

    // Set the version and variant bits, so this is a well-formed v4 uuid rather than 16 random
    // bytes that merely look like one.
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;

    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

/// Fill `bytes` from the OS random source.
///
/// A failure is not survivable: a predictable id could collide with another, so there is no
/// sensible fallback to a weaker source.
pub(crate) fn fill_random(bytes: &mut [u8]) {
    use rand::RngCore;
    rand::rngs::OsRng.fill_bytes(bytes);
}
