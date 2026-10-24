//! `bravebot_bedrock::eventstream::FrameDecoder`: the binary frames a streamed Bedrock reply
//! arrives in.
//!
//! Raw bytes fail the prelude checksum at once, so they never reach the parsing of headers and
//! payload. The second mode builds frames from fields with correct checksums, changes one length
//! or byte of the last and computes the checksums again, which is how a lie about a length gets
//! past the decoder's first check. Every stream is delivered in the reads the input cuts it into.

#![no_main]
#![forbid(unsafe_code)]

use arbitrary::Arbitrary;
use bravebot_bedrock::eventstream::{Event, FrameDecoder, FrameError};
use bravebot_fuzz::frames::{Damage, Frame, Model};
use bravebot_fuzz::splits::pieces;
use libfuzzer_sys::fuzz_target;

#[derive(Arbitrary, Debug)]
enum Input {
    Raw {
        bytes: Vec<u8>,
        cuts: Vec<u8>,
    },
    Frames {
        frames: Vec<Frame>,
        /// A change to the last frame.
        damage: Option<Damage>,
        /// How many bytes of the last frame go missing, modulo its length, when nothing else
        /// changed it.
        missing: u16,
        cuts: Vec<u8>,
    },
}

/// What a decoder did with a stream.
#[derive(Debug, PartialEq, Eq)]
struct Outcome {
    events: Vec<Event>,
    error: Option<FrameError>,
    /// Whether bytes were left over, which only means something where nothing was refused: after
    /// a refusal what is left depends on how much had arrived when it came.
    mid_frame: Option<bool>,
}

/// Feed `stream` to a decoder in the reads `cuts` makes of it, stopping at the first refusal.
fn decode(stream: &[u8], cuts: &[u8]) -> Outcome {
    let mut decoder = FrameDecoder::new();
    let mut events = Vec::new();
    let mut error = None;
    'reads: for read in pieces(stream, cuts) {
        for item in decoder.events(read) {
            match item {
                Ok(event) => events.push(event),
                Err(refusal) => {
                    error = Some(refusal);
                    break 'reads;
                }
            }
        }
    }
    let mid_frame = error.is_none().then(|| decoder.is_mid_frame());
    Outcome {
        events,
        error,
        mid_frame,
    }
}

fn models(events: &[Event]) -> Vec<Model> {
    events
        .iter()
        .map(|event| match event {
            Event::Named { name, payload } => Model::Named {
                name: name.clone(),
                payload: payload.clone(),
            },
            Event::Failed { kind } => Model::Failed { kind: kind.clone() },
        })
        .collect()
}

fuzz_target!(|input: Input| {
    let (stream, cuts, expected) = match input {
        Input::Raw { bytes, cuts } => (bytes, cuts, None),
        Input::Frames {
            frames,
            damage,
            missing,
            cuts,
        } => {
            let mut encoded: Vec<Vec<u8>> = frames.iter().map(Frame::encode).collect();
            let mut models: Vec<Option<Model>> = frames.iter().map(Frame::model).collect();
            let mut expected = None;
            if let Some(last) = encoded.pop() {
                let last_model = models.pop().flatten();
                let earlier: Vec<Model> = models.into_iter().flatten().collect();
                match damage {
                    Some(damage) => {
                        encoded.push(damage.apply(last));
                        // What a decoder does with the other changes depends on where the new
                        // lengths put the frame's end, so only a refusal is asserted.
                        if damage.is_refused() {
                            expected = Some((earlier, true, false));
                        }
                    }
                    None => {
                        let dropped = usize::from(missing) % (last.len() + 1);
                        let kept = last[..last.len() - dropped].to_vec();
                        let mut whole = earlier;
                        if dropped == 0 {
                            whole.extend(last_model);
                        }
                        // Bytes of a frame that never finished are held, never refused.
                        expected = Some((whole, false, dropped != 0 && dropped != last.len()));
                        encoded.push(kept);
                    }
                }
            }
            (encoded.concat(), cuts, expected)
        }
    };

    let whole = decode(&stream, &[]);
    let split = decode(&stream, &cuts);
    assert_eq!(whole, split, "the reads changed what was decoded");

    // `push` gives up the events a corrupt frame came after, and agrees otherwise.
    match FrameDecoder::new().push(&stream) {
        Ok(events) => assert!(whole.error.is_none() && events == whole.events),
        Err(refusal) => assert_eq!(Some(refusal), whole.error),
    }

    if let Some((events, refused, mid_frame)) = expected {
        assert_eq!(models(&whole.events), events);
        assert_eq!(whole.error.is_some(), refused, "{:?}", whole.error);
        if !refused {
            assert_eq!(whole.mid_frame, Some(mid_frame));
        }
    }
});
