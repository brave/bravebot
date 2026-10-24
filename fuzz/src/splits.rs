//! Cutting a byte stream into the reads a decoder is fed.

/// `bytes` cut into reads. Each of `cuts` takes up to that many bytes modulo 32, so a read can be
/// empty, and what is left after the last cut is one more read. The reads joined are `bytes`.
pub fn pieces<'a>(bytes: &'a [u8], cuts: &[u8]) -> Vec<&'a [u8]> {
    let mut reads = Vec::new();
    let mut rest = bytes;
    for cut in cuts {
        let (read, after) = rest.split_at(usize::from(cut % 32).min(rest.len()));
        reads.push(read);
        rest = after;
    }
    reads.push(rest);
    reads
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The targets compare one whole read with the same bytes in pieces, so the pieces have to be the
    /// same bytes.
    #[test]
    fn the_reads_joined_are_the_stream_whatever_the_cuts() {
        let stream: Vec<u8> = (0..100).collect();
        for cuts in [
            &[][..],
            &[0, 0, 7][..],
            &[255, 255, 255, 255][..],
            &[1; 200][..],
        ] {
            assert_eq!(pieces(&stream, cuts).concat(), stream);
        }
    }

    /// An empty read is one a decoder has to handle, and a cut past the end must not stop the target
    /// before the decoder is reached.
    #[test]
    fn a_cut_of_zero_is_an_empty_read_and_a_cut_past_the_end_takes_the_rest() {
        assert_eq!(pieces(b"abc", &[0, 2]), vec![&b""[..], b"ab", b"c"]);
        assert_eq!(pieces(b"abc", &[31, 5]), vec![&b"abc"[..], b"", b""]);
    }
}
