//! Scaling a picture down, for the two gestures that carry one: a paste ([`crate::turn`]) and a drop
//! ([`crate::workspace::Workspace::read_dropped_attachment`]).
//!
//! The pixels are decoded and re-encoded and never looked at: nothing here branches on what a
//! picture shows. What is read is the header's canvas size and the file's length, which are facts
//! about the file's shape and not about what it says. The caller decides which gesture's bytes
//! come here, so a picture read by `read_file` or by a tool never does.

use std::io::Cursor;

/// The longest side, in pixels, a picture is scaled down to.
pub const DOWNSCALED_SIDE: u32 = 2048;

/// The largest canvas a picture may declare and still be decoded to be scaled down.
pub const MAX_DECODED_SIDE: u32 = 16384;

/// The most memory a decode of a picture to be scaled down may allocate.
pub const MAX_DECODE_ALLOC_BYTES: u64 = 512 * 1024 * 1024;

fn reader(bytes: &[u8]) -> Option<image::ImageReader<Cursor<&[u8]>>> {
    let mut reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_DECODED_SIDE);
    limits.max_image_height = Some(MAX_DECODED_SIDE);
    limits.max_alloc = Some(MAX_DECODE_ALLOC_BYTES);
    reader.limits(limits);
    Some(reader)
}

/// The longer side, in pixels, of the canvas `bytes` declares, or `None` where they are not a
/// picture this build can decode (GIF and WebP have no codec here, and a damaged header names no
/// canvas). Only the header is read, so no pixel is allocated.
pub fn longest_side(bytes: &[u8]) -> Option<u32> {
    let (width, height) = reader(bytes)?.into_dimensions().ok()?;
    Some(width.max(height))
}

/// A picture scaled so its longest side is at most [`DOWNSCALED_SIDE`] and encoded as PNG, or
/// `None` where it will not decode within the limits.
///
/// A picture already inside that side is re-encoded at its own size and never enlarged. The media
/// type of the result is the caller's own literal, `image/png`.
pub fn downscaled(bytes: &[u8]) -> Option<Vec<u8>> {
    let decoded = reader(bytes)?.decode().ok()?;
    let scaled = if decoded.width().max(decoded.height()) > DOWNSCALED_SIDE {
        decoded.resize(
            DOWNSCALED_SIDE,
            DOWNSCALED_SIDE,
            image::imageops::FilterType::Triangle,
        )
    } else {
        decoded
    };
    let mut out = Vec::new();
    scaled
        .write_to(&mut Cursor::new(&mut out), image::ImageFormat::Png)
        .ok()?;
    Some(out)
}
