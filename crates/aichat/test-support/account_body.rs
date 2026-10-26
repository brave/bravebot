//! The body of a request to the ChatGPT account as the service reads it: the zstd frame the request
//! carried, decoded, and the level it asks for.
//!
//! Shared by the tests of the client that writes the frame and the tests of the built binary that
//! reads what it sent, which both have `ruzstd`.

use std::io::Read;

/// The body the service reads from an account request: `frame`, decoded.
pub fn decoded(frame: &[u8]) -> Vec<u8> {
    let mut decoder =
        ruzstd::decoding::StreamingDecoder::new(frame).expect("the body is a zstd frame");
    let mut plain = Vec::new();
    decoder.read_to_end(&mut plain).expect("the frame decodes");
    plain
}

/// The level the body of a request to the account carried, as the host reads it, or `None` where
/// the body has no `reasoning.effort`.
pub fn level_in(frame: &[u8]) -> Option<String> {
    let body: serde_json::Value =
        serde_json::from_slice(&decoded(frame)).expect("the body is JSON");
    body.get("reasoning")?
        .get("effort")?
        .as_str()
        .map(str::to_string)
}
