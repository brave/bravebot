//! What a `turn.send` carries as bytes: pictures a person pasted and pictures or PDFs a person
//! dropped, and what a front end is told about a file it is about to stage.
//!
//! Each list is checked in full before a turn starts, and anything this side can tell is wrong
//! refuses the send. A turn that lost the picture it was sent with answers without what it was
//! asked about and reports success, which is worse than a send the caller is told to retry.

use crate::protocol::{Failure, Request};
use base64::Engine;
use bravebot_agent::turn::{MAX_PASTED_IMAGE_BYTES, PASTED_IMAGE_MEDIA, PastedImage};
use bravebot_agent::workspace::MAX_ATTACHMENT_BYTES;
use bravebot_filetype::by_name::{Kind, kind_of, media_for};
use serde_json::{Value, json};
use std::path::Path;

/// The pictures a person pasted into this prompt (PASTE-2).
///
/// `images` is a list of `{ "media", "data" }`, the data standard base64. An absent or `null`
/// list is none. Any other shape, a type outside [`PASTED_IMAGE_MEDIA`], data that does not decode, an
/// empty picture or one over [`MAX_PASTED_IMAGE_BYTES`] refuses the send.
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
    let Some(media_type) = PASTED_IMAGE_MEDIA
        .iter()
        .copied()
        .find(|known| *known == media)
    else {
        return Err(Failure::bad_request(format!(
            "a pasted picture is one of {}, and {media} is not",
            PASTED_IMAGE_MEDIA.join(", ")
        )));
    };
    // Refused before decoding, so an oversized string is never decoded into memory.
    if data.len() > MAX_PASTED_IMAGE_BYTES.div_ceil(3) * 4 {
        return Err(too_large(data.len() / 4 * 3));
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data)
        .map_err(|_| Failure::bad_request("a pasted picture's `data` is not standard base64"))?;
    if bytes.is_empty() {
        return Err(Failure::bad_request("a pasted picture holds no bytes"));
    }
    if bytes.len() > MAX_PASTED_IMAGE_BYTES {
        return Err(too_large(bytes.len()));
    }
    Ok(PastedImage { media_type, bytes })
}

/// The type a front end that re-encodes a pasted picture writes it as. PNG, because it is lossless
/// and every clipboard offers a picture as one.
const REENCODED: &str = "image/png";

/// Whether a front end may stage a pasted picture of `bytes` (PASTE-11): `{ "ok": true }` with the
/// type to name it by once re-encoded and the noun its marker uses, or `{ "ok": false }` with the
/// note to show for one too large. The cap is the one [`pasted`] holds a send to and the terminal's
/// clipboard reader stages by, the note is the terminal's, and the noun is the one the terminal
/// writes, so a picture one front end takes the other takes under the same marker.
pub fn paste_check(request: &Request) -> Result<Value, Failure> {
    let Some(bytes) = request.param("bytes").as_u64() else {
        return Err(Failure::bad_request("`bytes` must be a whole number"));
    };
    let most = MAX_PASTED_IMAGE_BYTES as u64;
    Ok(if bytes > most {
        json!({ "ok": false, "note": bravebot_i18n::sizes::paste_too_large(bytes, most) })
    } else {
        json!({ "ok": true, "media": REENCODED, "noun": Kind::Attachment(REENCODED).noun() })
    })
}

fn too_large(size: usize) -> Failure {
    Failure::bad_request(format!(
        "a pasted picture of {size} bytes is over the {MAX_PASTED_IMAGE_BYTES} a paste may be"
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
    if let Some(note) = over_the_cap(path, metadata.len()) {
        return Err(Failure::bad_request(note));
    }
    Ok(media)
}

/// The note for a dropped picture or PDF of `bytes` at `path` that is larger than the agent will
/// carry, or `None` when it is within the cap. The window shows it as it stands, so it is in the
/// terminal's words and in the locale `bravebot-rpc` chose, which is `en-US`.
fn over_the_cap(path: &Path, bytes: u64) -> Option<String> {
    let most = MAX_ATTACHMENT_BYTES as u64;
    (bytes > most).then(|| {
        let name = path
            .file_name()
            .map_or_else(|| path.to_string_lossy(), |name| name.to_string_lossy());
        bravebot_i18n::sizes::attachment_too_large(&name, bytes, most)
    })
}

/// More files than one drop names; a longer list is not a drop.
const MOST_IN_ONE_DROP: usize = 100;

/// What each dropped file is, for a front end staging a drop (DROP-11).
///
/// `files` is a list of `{ "path", "bytes" }`, the size being what the front end found on disk, and
/// the answer is one entry per file in the same order: `null` for a type nothing takes, whose path
/// the front end writes out as text (DROP-4); `{ "note" }` for a picture or PDF larger than the
/// agent will carry, which the front end shows and leaves out; and otherwise the kind and the noun
/// its marker uses. The rules are `bravebot_filetype`'s, the ones the terminal stages a drop by,
/// and the cap is the agent's, the one `turn.send` holds an attachment to. No file is opened.
pub fn classified(request: &Request) -> Result<Value, Failure> {
    let refused = || {
        Failure::bad_request(format!(
            "`files` must be a list of at most {MOST_IN_ONE_DROP} objects with a string `path` and a whole `bytes`"
        ))
    };
    let Some(files) = request
        .param("files")
        .as_array()
        .filter(|files| files.len() <= MOST_IN_ONE_DROP)
    else {
        return Err(refused());
    };
    let files = files
        .iter()
        .map(|file| {
            let (Some(path), Some(bytes)) = (
                file.get("path").and_then(Value::as_str),
                file.get("bytes").and_then(Value::as_u64),
            ) else {
                return Err(refused());
            };
            let Some(kind) = kind_of(path) else {
                return Ok(Value::Null);
            };
            // Text has no cap: the turn reads it the way it reads a file named with `@`.
            if matches!(kind, Kind::Attachment(_))
                && let Some(note) = over_the_cap(Path::new(path), bytes)
            {
                return Ok(json!({ "note": note }));
            }
            Ok(json!({ "kind": word(kind), "noun": kind.noun() }))
        })
        .collect::<Result<Vec<_>, Failure>>()?;
    Ok(json!({ "files": files }))
}

/// The kind as the window's `DropKind` spells it.
fn word(kind: Kind) -> &'static str {
    match kind {
        Kind::Attachment(bravebot_core::vetting::PDF) => "pdf",
        Kind::Attachment(_) => "image",
        Kind::Text => "text",
    }
}
