//! What a `turn.send` carries as bytes: pictures a person pasted and pictures or PDFs a person
//! dropped.
//!
//! Each list is checked in full before a turn starts, and anything this side can tell is wrong
//! refuses the send. A turn that lost the picture it was sent with answers without what it was
//! asked about and reports success, which is worse than a send the caller is told to retry.

use crate::protocol::{Failure, Request};
use base64::Engine;
use bravebot_agent::turn::PastedImage;
use bravebot_agent::workspace::{MAX_ATTACHMENT_BYTES, media_for};
use serde_json::Value;
use std::path::Path;

/// The most a pasted picture may weigh once decoded: the terminal's own cap on a paste, so a
/// screenshot one front end takes the other takes too (PASTE-6).
pub const MAX_PASTED_BYTES: usize = 10 * 1024 * 1024;

/// The media types a pasted picture may be sent as (PASTE-3).
///
/// The type lands in a `data:` URL, where it is routing. The caller's string only selects an
/// entry here, and the entry is what is sent.
const PASTED_MEDIA: &[&str] = &["image/png", "image/jpeg", "image/gif", "image/webp"];

/// The pictures a person pasted into this prompt (PASTE-2).
///
/// `images` is a list of `{ "media", "data" }`, the data standard base64. An absent or `null`
/// list is none. Any other shape, a type outside [`PASTED_MEDIA`], data that does not decode, an
/// empty picture or one over [`MAX_PASTED_BYTES`] refuses the send.
pub fn pasted(request: &Request) -> Result<Vec<PastedImage>, Failure> {
    let entries = match request.params.get("images") {
        None | Some(Value::Null) => return Ok(Vec::new()),
        Some(Value::Array(entries)) => entries,
        Some(_) => return Err(Failure::bad_request("`images` must be a list")),
    };
    entries.iter().map(pasted_image).collect()
}

fn pasted_image(entry: &Value) -> Result<PastedImage, Failure> {
    let (Some(media), Some(data)) = (
        entry.get("media").and_then(Value::as_str),
        entry.get("data").and_then(Value::as_str),
    ) else {
        return Err(Failure::bad_request(
            "a pasted picture is an object with a string `media` and a string `data`",
        ));
    };
    let Some(media_type) = PASTED_MEDIA.iter().copied().find(|known| *known == media) else {
        return Err(Failure::bad_request(format!(
            "a pasted picture is one of {}, and {media} is not",
            PASTED_MEDIA.join(", ")
        )));
    };
    // Refused before decoding, so an oversized string is never decoded into memory.
    if data.len() > MAX_PASTED_BYTES.div_ceil(3) * 4 {
        return Err(too_large(data.len() / 4 * 3));
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data)
        .map_err(|_| Failure::bad_request("a pasted picture's `data` is not standard base64"))?;
    if bytes.is_empty() {
        return Err(Failure::bad_request("a pasted picture holds no bytes"));
    }
    if bytes.len() > MAX_PASTED_BYTES {
        return Err(too_large(bytes.len()));
    }
    Ok(PastedImage { media_type, bytes })
}

fn too_large(size: usize) -> Failure {
    Failure::bad_request(format!(
        "a pasted picture of {size} bytes is over the {MAX_PASTED_BYTES} a paste may be"
    ))
}

/// The pictures and PDFs a person dropped onto this prompt, each with the media type its
/// extension names (DROP-10).
///
/// `attachments` is a list of absolute paths. Each must name a regular file whose extension the
/// agent carries as bytes, no larger than the agent will carry. The type comes from the agent's
/// table and never from the caller. An absent or `null` list is none, and any other shape
/// refuses the send.
pub fn dropped(request: &Request) -> Result<Vec<(String, &'static str)>, Failure> {
    let entries = match request.params.get("attachments") {
        None | Some(Value::Null) => return Ok(Vec::new()),
        Some(Value::Array(entries)) => entries,
        Some(_) => {
            return Err(Failure::bad_request(
                "`attachments` must be a list of paths",
            ));
        }
    };
    entries
        .iter()
        .map(|entry| {
            let named = entry
                .as_str()
                .ok_or_else(|| Failure::bad_request("`attachments` must be a list of paths"))?;
            dropped_attachment(named).map(|media| (named.to_string(), media))
        })
        .collect()
}

fn dropped_attachment(named: &str) -> Result<&'static str, Failure> {
    let path = Path::new(named);
    if !path.is_absolute() {
        return Err(Failure::bad_request(format!(
            "a dropped file is named by an absolute path, and {named} is not one"
        )));
    }
    if path.is_dir() {
        return Err(Failure::bad_request(format!(
            "{named} is a directory, and dropping one attaches nothing"
        )));
    }
    let metadata = path
        .metadata()
        .ok()
        .filter(std::fs::Metadata::is_file)
        .ok_or_else(|| Failure::bad_request(format!("{named} names no file to drop")))?;
    let Some(media) = media_for(named) else {
        return Err(Failure::bad_request(format!(
            "{named} is not a picture or a PDF; a text file belongs in `dropped`"
        )));
    };
    if metadata.len() > MAX_ATTACHMENT_BYTES as u64 {
        return Err(Failure::bad_request(format!(
            "{named} is {} bytes, over the {MAX_ATTACHMENT_BYTES} an attachment may be",
            metadata.len()
        )));
    }
    Ok(media)
}
