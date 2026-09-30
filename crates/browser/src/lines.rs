//! Newline-delimited JSON over the socket, read one bounded line at a time.
//!
//! The first line a peer sends is the secret. Every line after it is one JSON-RPC message. A line
//! is read up to a limit, so a peer that never sends a newline cannot make the reader hold an
//! unbounded buffer.

use std::io::{self, BufRead};

/// One line read from a stream.
#[derive(Debug, PartialEq, Eq)]
pub enum Line {
    /// The line's bytes, without the newline.
    Read(Vec<u8>),
    /// The stream ended with nothing after the last newline.
    End,
    /// The line reached the limit before its newline.
    TooLong,
}

/// Reads one line of at most `limit` bytes, not counting the newline.
///
/// A stream that ends partway through a line returns what it had, since a peer that writes one
/// last message and closes has sent that message.
pub fn read_line(input: &mut impl BufRead, limit: usize) -> io::Result<Line> {
    read_line_observed(input, limit, |_| {})
}

/// Reads as [`read_line`] does, and hands every byte before the newline to `observe`, including
/// bytes past `limit` that are drained rather than kept.
pub fn read_line_observed(
    input: &mut impl BufRead,
    limit: usize,
    mut observe: impl FnMut(&[u8]),
) -> io::Result<Line> {
    let mut line = Vec::new();
    loop {
        let available = match input.fill_buf() {
            Ok(available) => available,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        };
        if available.is_empty() {
            return Ok(if line.is_empty() {
                Line::End
            } else {
                Line::Read(line)
            });
        }
        let (taken, ends) = match available.iter().position(|&byte| byte == b'\n') {
            Some(newline) => (newline, true),
            None => (available.len(), false),
        };
        observe(&available[..taken]);
        if line.len() + taken > limit {
            input.consume(if ends { taken + 1 } else { taken });
            let mut ended = ends;
            while !ended {
                let available = input.fill_buf()?;
                if available.is_empty() {
                    break;
                }
                let newline = available.iter().position(|&byte| byte == b'\n');
                let consumed = newline.map_or(available.len(), |at| at + 1);
                let content = newline.unwrap_or(available.len());
                observe(&available[..content]);
                input.consume(consumed);
                ended = newline.is_some();
            }
            return Ok(Line::TooLong);
        }
        line.extend_from_slice(&available[..taken]);
        input.consume(if ends { taken + 1 } else { taken });
        if ends {
            return Ok(Line::Read(line));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Several lines on one stream are read as several, in order.
    #[test]
    fn lines_are_read_one_at_a_time() {
        let mut input = io::Cursor::new(b"first\nsecond\n".to_vec());
        assert_eq!(
            read_line(&mut input, 16).unwrap(),
            Line::Read(b"first".to_vec())
        );
        assert_eq!(
            read_line(&mut input, 16).unwrap(),
            Line::Read(b"second".to_vec())
        );
        assert_eq!(read_line(&mut input, 16).unwrap(), Line::End);
    }

    /// A peer that never sends a newline is stopped at the limit rather than buffered without end.
    #[test]
    fn a_line_past_the_limit_is_too_long() {
        let mut input = io::Cursor::new(b"0123456789abcdef!\n".to_vec());
        assert_eq!(read_line(&mut input, 16).unwrap(), Line::TooLong);
        assert_eq!(read_line(&mut input, 16).unwrap(), Line::End);

        let mut input = io::Cursor::new(b"0123456789abcdef\n".to_vec());
        assert_eq!(
            read_line(&mut input, 16).unwrap(),
            Line::Read(b"0123456789abcdef".to_vec())
        );
    }

    /// The observer receives the whole line even where the buffer keeps only its start.
    #[test]
    fn a_line_past_the_limit_is_observed_and_drained() {
        let mut input = io::Cursor::new(b"0123456789abcdef!\nnext\n".to_vec());
        let mut observed: Vec<u8> = Vec::new();
        assert_eq!(
            read_line_observed(&mut input, 4, |bytes| observed.extend(bytes)).unwrap(),
            Line::TooLong
        );
        assert_eq!(observed, b"0123456789abcdef!");
        assert_eq!(
            read_line(&mut input, 16).unwrap(),
            Line::Read(b"next".to_vec())
        );
    }

    /// A last message with no newline after it is still the message that was sent.
    #[test]
    fn a_last_line_without_a_newline_is_read() {
        let mut input = io::Cursor::new(b"last".to_vec());
        assert_eq!(
            read_line(&mut input, 16).unwrap(),
            Line::Read(b"last".to_vec())
        );
        assert_eq!(read_line(&mut input, 16).unwrap(), Line::End);
    }
}
