//! The event-stream side of the `event_stream` target: frames built from fields, damaged in one
//! place, and what a decoder should make of them.
//!
//! A random prelude fails its CRC, so raw bytes never reach the parsing of headers and payload. The
//! encoder here writes a correct prelude and checksums, then changes one length or byte and
//! computes both checksums again, which is how a damaged frame gets past them. It implements
//! CRC-32 itself and reads nothing from `bravebot_bedrock`.

use arbitrary::Arbitrary;

/// The most a frame may claim to be. Private in the decoder, so restated here: the target fails
/// when the two disagree about where a claim becomes impossible.
pub const MAX_FRAME_BYTES: u64 = 16 * 1024 * 1024;

/// A prelude, no headers, no payload and the trailing checksum.
const MIN_FRAME_BYTES: usize = 16;

/// CRC-32 as the framing uses it: the reflected polynomial, a bit at a time.
pub fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

/// A header's value, one alternative for each type the format defines.
#[derive(Arbitrary, Debug, Clone)]
pub enum Value {
    True,
    False,
    Byte(u8),
    Short(u16),
    Int(u32),
    Long(u64),
    Bytes(Vec<u8>),
    Text(Vec<u8>),
    Timestamp(u64),
    Uuid([u8; 16]),
    /// A type the format does not define, with whatever bytes follow it. Its width is unknown, so
    /// a reader cannot get past it.
    Unknown(u8, Vec<u8>),
}

impl Value {
    fn encode(&self, out: &mut Vec<u8>) {
        match self {
            Self::True => out.push(0),
            Self::False => out.push(1),
            Self::Byte(v) => out.extend([2, *v]),
            Self::Short(v) => {
                out.push(3);
                out.extend(v.to_be_bytes());
            }
            Self::Int(v) => {
                out.push(4);
                out.extend(v.to_be_bytes());
            }
            Self::Long(v) => {
                out.push(5);
                out.extend(v.to_be_bytes());
            }
            Self::Bytes(v) => Self::counted(6, v, out),
            Self::Text(v) => Self::counted(7, v, out),
            Self::Timestamp(v) => {
                out.push(8);
                out.extend(v.to_be_bytes());
            }
            Self::Uuid(v) => {
                out.push(9);
                out.extend(v);
            }
            Self::Unknown(kind, bytes) => {
                out.push(10 + kind % 246);
                out.extend(bytes);
            }
        }
    }

    /// `value` as far as a length of two bytes can say.
    fn cut(value: &[u8]) -> &[u8] {
        &value[..value.len().min(usize::from(u16::MAX))]
    }

    /// The bytes of a text value as the encoding keeps them.
    fn text(&self) -> Option<&[u8]> {
        match self {
            Self::Text(bytes) => Some(Self::cut(bytes)),
            _ => None,
        }
    }

    /// A type whose value is preceded by its length in two bytes.
    fn counted(kind: u8, value: &[u8], out: &mut Vec<u8>) {
        let value = Self::cut(value);
        out.push(kind);
        out.extend((value.len() as u16).to_be_bytes());
        out.extend(value);
    }
}

/// A header's name. The two the decoder reads are named, so generated frames carry them often.
#[derive(Arbitrary, Debug, Clone)]
pub enum Name {
    EventType,
    ExceptionType,
    Other(Vec<u8>),
}

impl Name {
    fn bytes(&self) -> &[u8] {
        let bytes = match self {
            Self::EventType => b":event-type".as_slice(),
            Self::ExceptionType => b":exception-type".as_slice(),
            Self::Other(bytes) => bytes,
        };
        &bytes[..bytes.len().min(usize::from(u8::MAX))]
    }
}

#[derive(Arbitrary, Debug, Clone)]
pub struct Header {
    pub name: Name,
    pub value: Value,
}

impl Header {
    fn encode(&self, out: &mut Vec<u8>) {
        let name = self.name.bytes();
        out.push(name.len() as u8);
        out.extend(name);
        self.value.encode(out);
    }
}

#[derive(Arbitrary, Debug, Clone)]
pub struct Frame {
    pub headers: Vec<Header>,
    pub payload: Vec<u8>,
}

/// What one frame is worth to a decoder, worked out from its fields.
#[derive(Debug, PartialEq, Eq)]
pub enum Model {
    Named { name: String, payload: Vec<u8> },
    Failed { kind: String },
}

impl Frame {
    /// The frame as bytes, with a correct prelude and both checksums.
    pub fn encode(&self) -> Vec<u8> {
        let mut headers = Vec::new();
        for header in &self.headers {
            header.encode(&mut headers);
        }
        let total = 12 + headers.len() + self.payload.len() + 4;
        let mut out = Vec::with_capacity(total);
        out.extend((total as u32).to_be_bytes());
        out.extend((headers.len() as u32).to_be_bytes());
        out.extend(crc32(&out[..8]).to_be_bytes());
        out.extend(&headers);
        out.extend(&self.payload);
        out.extend(crc32(&out).to_be_bytes());
        out
    }

    /// The event this frame yields: a failure if a text header names one, else an event if a text
    /// header names that, else nothing. The first header with a wanted name that holds text is the
    /// one read, and text that is not UTF-8 is no name at all, even where a later header has one.
    /// Headers after one of an unknown type are not read.
    pub fn model(&self) -> Option<Model> {
        let text_of = |wanted: &[u8]| -> Option<String> {
            for header in &self.headers {
                // A value of a type with no known width ends the walk, so no later header is read.
                if let Value::Unknown(..) = header.value {
                    return None;
                }
                if header.name.bytes() == wanted
                    && let Some(text) = header.value.text()
                {
                    return String::from_utf8(text.to_vec()).ok();
                }
            }
            None
        };
        if let Some(kind) = text_of(b":exception-type") {
            return Some(Model::Failed { kind });
        }
        text_of(b":event-type").map(|name| Model::Named {
            name,
            payload: self.payload.clone(),
        })
    }
}

/// One change to the last frame of a stream. Both checksums are computed again after it, except
/// where the change is to a checksum.
#[derive(Arbitrary, Debug, Clone)]
pub enum Damage {
    /// A total length past [`MAX_FRAME_BYTES`], which the decoder must refuse.
    Oversized(u32),
    /// A total length below the smallest frame, which it must refuse.
    Undersized(u8),
    /// A headers length that runs past the end of the frame, which it must refuse.
    HeadersPastEnd(u16),
    /// Any other total length. A frame that is shorter than it claims has to wait for the rest.
    Total(u32),
    /// Any other headers length.
    Headers(u32),
    /// One byte of the frame changed.
    Byte { at: u16, to: u8 },
    /// One byte of the trailing checksum changed, and nothing else. The prelude still checks out
    /// and the frame is whole, so the decoder reads it all before it refuses it.
    Checksum { at: u8, mask: u8 },
}

impl Damage {
    /// Whether the decoder must report the damaged frame as corrupt.
    pub fn is_refused(&self) -> bool {
        matches!(
            self,
            Self::Oversized(_)
                | Self::Undersized(_)
                | Self::HeadersPastEnd(_)
                | Self::Checksum { .. }
        )
    }

    /// `frame`, damaged.
    pub fn apply(&self, mut frame: Vec<u8>) -> Vec<u8> {
        debug_assert!(frame.len() >= MIN_FRAME_BYTES);
        let actual = frame.len();
        let put = |at: usize, value: u32, frame: &mut Vec<u8>| {
            frame[at..at + 4].copy_from_slice(&value.to_be_bytes());
        };
        match self {
            Self::Oversized(extra) => {
                let claim = (MAX_FRAME_BYTES + 1).saturating_add(u64::from(*extra));
                put(0, u32::try_from(claim).unwrap_or(u32::MAX), &mut frame);
            }
            Self::Undersized(claim) => put(0, u32::from(*claim % 16), &mut frame),
            Self::HeadersPastEnd(extra) => {
                let claim = actual - MIN_FRAME_BYTES + 1 + usize::from(*extra);
                put(4, claim as u32, &mut frame);
            }
            Self::Total(claim) => put(0, *claim, &mut frame),
            Self::Headers(claim) => put(4, *claim, &mut frame),
            Self::Byte { at, to } => frame[usize::from(*at) % actual] = *to,
            Self::Checksum { at, mask } => {
                // `| 1` so that the byte does change.
                frame[actual - 4 + usize::from(*at % 4)] ^= *mask | 1;
                return frame;
            }
        }
        let prelude = crc32(&frame[..8]);
        frame[8..12].copy_from_slice(&prelude.to_be_bytes());
        let message = crc32(&frame[..actual - 4]);
        frame[actual - 4..].copy_from_slice(&message.to_be_bytes());
        frame
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(name: Name, value: &str) -> Header {
        Header {
            name,
            value: Value::Text(value.as_bytes().to_vec()),
        }
    }

    /// The canonical check value for CRC-32. The decoder is held to the checksum the format
    /// states, not to one that only agrees with the encoder.
    #[test]
    fn the_checksum_is_the_standard_crc32() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(crc32(b""), 0);
    }

    /// The decoder is held to frames the format describes, so an encoder that wrote another layout
    /// would have the target fuzzing a stream no service sends.
    #[test]
    fn a_frame_encodes_to_the_layout_the_format_states() {
        let frame = Frame {
            headers: vec![text(Name::EventType, "x")],
            payload: b"{}".to_vec(),
        };
        let bytes = frame.encode();
        let headers_len = 1 + 11 + 1 + 2 + 1;
        assert_eq!(bytes.len(), 12 + headers_len + 2 + 4);
        assert_eq!(bytes[..4], (bytes.len() as u32).to_be_bytes());
        assert_eq!(bytes[4..8], (headers_len as u32).to_be_bytes());
        assert_eq!(bytes[8..12], crc32(&bytes[..8]).to_be_bytes());
        assert_eq!(&bytes[12..13], &[11]);
        assert_eq!(&bytes[13..24], b":event-type");
        assert_eq!(&bytes[24..28], &[7, 0, 1, b'x']);
        assert_eq!(&bytes[28..30], b"{}");
        assert_eq!(
            bytes[bytes.len() - 4..],
            crc32(&bytes[..bytes.len() - 4]).to_be_bytes()
        );
    }

    /// The target compares what the decoder yields with this model, so the model has to read headers
    /// as the decoder is documented to: the first text header of a wanted name, skipping a header of
    /// that name that holds another type.
    #[test]
    fn the_model_reads_the_first_text_header_of_a_wanted_name() {
        let frame = Frame {
            headers: vec![
                Header {
                    name: Name::EventType,
                    value: Value::Bytes(b"not text".to_vec()),
                },
                text(Name::EventType, "first"),
                text(Name::EventType, "second"),
            ],
            payload: b"p".to_vec(),
        };
        assert_eq!(
            frame.model(),
            Some(Model::Named {
                name: "first".to_string(),
                payload: b"p".to_vec()
            })
        );
    }

    /// The decoder returns at the first header that names the wanted type, valid or not.
    #[test]
    fn the_model_gives_nothing_for_a_name_that_is_not_utf8_even_if_a_later_header_has_one() {
        let frame = Frame {
            headers: vec![
                Header {
                    name: Name::EventType,
                    value: Value::Text(vec![0xFF]),
                },
                text(Name::EventType, "later"),
            ],
            payload: Vec::new(),
        };
        assert_eq!(frame.model(), None);
    }

    /// A header of a type with no known width ends the decoder's walk, because what follows it
    /// cannot be told from a header name. A model that read past it would report the decoder for
    /// not finding an event it is documented not to find.
    #[test]
    fn a_header_of_an_unknown_type_hides_the_headers_after_it() {
        let named_after = |before: Vec<Header>| Frame {
            headers: [before, vec![text(Name::EventType, "late")]].concat(),
            payload: Vec::new(),
        };
        let unknown = Header {
            name: Name::Other(b"x".to_vec()),
            value: Value::Unknown(0, vec![1, 2, 3]),
        };
        assert_eq!(named_after(vec![unknown.clone()]).model(), None);
        assert_eq!(
            named_after(vec![text(Name::EventType, "early"), unknown]).model(),
            Some(Model::Named {
                name: "early".to_string(),
                payload: Vec::new()
            })
        );
        let mut bytes = Vec::new();
        Value::Unknown(0, vec![7]).encode(&mut bytes);
        assert_eq!(bytes, [10, 7]);
        bytes.clear();
        Value::Unknown(246, Vec::new()).encode(&mut bytes);
        assert_eq!(
            bytes,
            [10],
            "the type wraps back to the first one the format leaves open"
        );
    }

    /// A frame that names both a failure and an event is a failure, as the decoder documents. A model
    /// that disagreed would have every such frame reported as a decoder defect.
    #[test]
    fn a_failure_is_read_before_an_event() {
        let frame = Frame {
            headers: vec![
                text(Name::EventType, "e"),
                text(Name::ExceptionType, "boom"),
            ],
            payload: Vec::new(),
        };
        assert_eq!(
            frame.model(),
            Some(Model::Failed {
                kind: "boom".to_string()
            })
        );
    }

    /// The frame has to stay whole and its prelude valid, or the decoder would refuse it for the
    /// prelude and the branch that checks the trailing checksum would still not be reached.
    #[test]
    fn a_damaged_trailing_checksum_leaves_the_prelude_and_the_length_alone() {
        let frame = Frame {
            headers: vec![text(Name::EventType, "x")],
            payload: Vec::new(),
        }
        .encode();
        let damaged = Damage::Checksum { at: 7, mask: 0 }.apply(frame.clone());
        assert_eq!(damaged.len(), frame.len());
        assert_eq!(damaged[..frame.len() - 4], frame[..frame.len() - 4]);
        assert_eq!(damaged[8..12], crc32(&damaged[..8]).to_be_bytes());
        assert_eq!(
            damaged[frame.len() - 4..frame.len() - 1],
            frame[frame.len() - 4..frame.len() - 1]
        );
        assert_ne!(damaged[frame.len() - 1], frame[frame.len() - 1]);
        assert_ne!(
            damaged[damaged.len() - 4..],
            crc32(&damaged[..damaged.len() - 4]).to_be_bytes()
        );
        assert!(Damage::Checksum { at: 0, mask: 0 }.is_refused());
    }

    /// A frame refused for its checksum says nothing about the length checks. Damage to a length has
    /// to leave both checksums valid for the length to be the reason the decoder refuses it.
    #[test]
    fn damage_that_must_be_refused_leaves_the_checksums_valid() {
        let frame = Frame {
            headers: vec![text(Name::EventType, "x")],
            payload: Vec::new(),
        }
        .encode();
        for damage in [
            Damage::Oversized(0),
            Damage::Undersized(3),
            Damage::HeadersPastEnd(0),
        ] {
            let bytes = damage.apply(frame.clone());
            assert_eq!(bytes.len(), frame.len());
            assert_eq!(bytes[8..12], crc32(&bytes[..8]).to_be_bytes());
            assert_eq!(
                bytes[bytes.len() - 4..],
                crc32(&bytes[..bytes.len() - 4]).to_be_bytes()
            );
            assert!(damage.is_refused());
        }
        let total = u32::from_be_bytes(
            Damage::Oversized(0).apply(frame.clone())[..4]
                .try_into()
                .unwrap(),
        );
        assert_eq!(u64::from(total), MAX_FRAME_BYTES + 1);
    }
}
