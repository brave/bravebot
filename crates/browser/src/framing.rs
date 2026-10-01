//! Native messaging's framing: each message is its length as four bytes in the platform's own
//! order, then that many bytes of UTF-8 JSON.
//!
//! The two directions have different limits. Brave closes the port on a message from the host
//! over [`TO_EXTENSION_LIMIT`], which disconnects every session at once, so a message that would
//! exceed it is refused before anything is written. A message to the host may be up to
//! [`FROM_EXTENSION_LIMIT`], and a length above that is a stream that has gone wrong rather than
//! a message to allocate for.
//!
//! Details: <https://developer.chrome.com/docs/extensions/develop/concepts/native-messaging>

use std::io::{self, Read, Write};

/// The largest message the host may send to the extension.
pub const TO_EXTENSION_LIMIT: usize = 1024 * 1024;

/// The largest message the extension may send to the host.
pub const FROM_EXTENSION_LIMIT: usize = 64 * 1024 * 1024;

/// Writes one message, or writes nothing and refuses one over [`TO_EXTENSION_LIMIT`].
pub fn write_message(out: &mut impl Write, message: &[u8]) -> io::Result<()> {
    if message.len() > TO_EXTENSION_LIMIT {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "a message of {} bytes is over native messaging's limit of {TO_EXTENSION_LIMIT}",
                message.len()
            ),
        ));
    }
    // Within the limit above, so the length fits in the four bytes the header has.
    let length = u32::try_from(message.len()).map_err(io::Error::other)?;
    out.write_all(&length.to_ne_bytes())?;
    out.write_all(message)?;
    out.flush()
}

/// Reads one message, or `None` where the stream ended between two messages.
pub fn read_message(input: &mut impl Read) -> io::Result<Option<Vec<u8>>> {
    let mut header = [0u8; 4];
    match input.read_exact(&mut header) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(error) => return Err(error),
    }
    let length = u32::from_ne_bytes(header) as usize;
    if length > FROM_EXTENSION_LIMIT {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("a message of {length} bytes is over the limit of {FROM_EXTENSION_LIMIT}"),
        ));
    }
    let mut message = vec![0u8; length];
    input.read_exact(&mut message)?;
    Ok(Some(message))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The two limits are the ones native messaging sets and the spec states. Every other test
    /// sizes its input from these constants, so it would pass at any value.
    #[test]
    fn the_limits_are_one_megabyte_out_and_sixty_four_in() {
        assert_eq!(TO_EXTENSION_LIMIT, 1024 * 1024);
        assert_eq!(FROM_EXTENSION_LIMIT, 64 * 1024 * 1024);
    }

    /// The extension reads exactly what was written, and the next message starts where this one
    /// ended, so a stream of several is read back as several.
    #[test]
    fn a_written_message_reads_back_as_itself() {
        let mut stream = Vec::new();
        write_message(&mut stream, br#"{"id":1}"#).unwrap();
        write_message(&mut stream, br#"{"id":2}"#).unwrap();
        let mut input = stream.as_slice();
        assert_eq!(read_message(&mut input).unwrap().unwrap(), br#"{"id":1}"#);
        assert_eq!(read_message(&mut input).unwrap().unwrap(), br#"{"id":2}"#);
        assert!(read_message(&mut input).unwrap().is_none());
    }

    /// Brave closes the port on a message over the limit, which would disconnect every session.
    /// Nothing is written, not even the header, so the stream stays in step.
    #[test]
    fn a_message_over_the_limit_to_the_extension_is_refused_and_nothing_is_written() {
        let mut stream = Vec::new();
        let at_limit = vec![b' '; TO_EXTENSION_LIMIT];
        write_message(&mut stream, &at_limit).unwrap();
        let written = stream.len();

        let over = vec![b' '; TO_EXTENSION_LIMIT + 1];
        let error = write_message(&mut stream, &over).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert_eq!(stream.len(), written);
    }

    /// A header claiming more than the extension may send is a broken stream, and is not
    /// allocated for.
    #[test]
    fn a_length_over_the_limit_from_the_extension_is_an_error() {
        let length = u32::try_from(FROM_EXTENSION_LIMIT + 1).unwrap();
        let stream = length.to_ne_bytes();
        let error = read_message(&mut stream.as_slice()).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    }

    /// A stream cut inside a message is an error, not a message that ended early.
    #[test]
    fn a_message_cut_short_is_an_error() {
        let mut stream = 8u32.to_ne_bytes().to_vec();
        stream.extend_from_slice(b"{\"id\"");
        let error = read_message(&mut stream.as_slice()).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::UnexpectedEof);
    }
}
