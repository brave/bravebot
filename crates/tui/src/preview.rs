//! Showing a person a picture back to themselves, in the terminal.
//!
//! Two places draw one. A picture pasted or dropped on the box used to be only a marker,
//! `[Image #1]`, and the person had to trust that it was the screenshot they meant; a thumbnail
//! under the box shows it. And a vetting prompt about a picture used to give only the path of a
//! copy to open; where the terminal draws real pictures, the prompt draws it too.
//!
//! What is drawn is always bytes a person is about to be asked about, or handed over themselves.
//! Nothing here decides anything and nothing decoded here goes anywhere but the screen.
//!
//! The terminal is asked what it can draw once, at start-up ([`sense`]). Where it cannot be asked,
//! or `NO_COLOR` is set, or the picture will not decode, there is no picture and the screen is what
//! it was before.
//!
//! # Decoding a picture nobody vouched for
//!
//! The bytes behind a vetting prompt are attacker-owned, and this decodes them in this process,
//! which holds the keys. That is the exposure `docs/specs/tools/vet-content.md` (VET-4) declined
//! for stripping a picture, and it is taken here knowingly, so what limits it is:
//!
//! - only PNG and JPEG are compiled in, both by memory-safe decoders;
//! - the size is read from the header, and a picture over [`MAX_PIXELS_PER_SIDE`] a side or
//!   [`MAX_ALLOC_BYTES`] of memory is refused before anything is allocated for it;
//! - the work happens on a thread of its own, so a slow decode does not stop the interface, a panic
//!   in a decoder is a missing picture rather than a crash, and only [`MAX_IN_FLIGHT`] run at once.
//!
//! None of that isolates the process the way a confined child would. There is no such child here.

use crate::theme;
use bravebot_agent::workspace::MAX_ATTACHMENT_BYTES;
use image::ImageFormat;
use ratatui::Frame;
use ratatui::layout::{Rect, Size};
use ratatui_image::{Image, Resize, picker::Picker, picker::ProtocolType, protocol::Protocol};
use std::io::Cursor;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, TryRecvError};

/// The tallest a thumbnail under the input box is, in rows.
pub const ROWS: u16 = 6;

/// The widest a thumbnail under the input box is, in columns.
pub const COLUMNS: u16 = 24;

/// The widest and tallest picture that is decoded, in pixels.
///
/// A picture is decoded here to be drawn and its pixels are then thrown away, so a file declaring
/// an enormous canvas is refused rather than allocated. Generous for a screenshot of a large
/// display.
const MAX_PIXELS_PER_SIDE: u32 = 8192;

/// The most memory the decoder may ask for.
const MAX_ALLOC_BYTES: u64 = 256 * 1024 * 1024;

/// How many pictures are being decoded at once. A person pasting a fifth while four are still
/// decoding gets no thumbnail for it, which costs nothing: the marker is in the line.
const MAX_IN_FLIGHT: usize = 4;

static IN_FLIGHT: AtomicUsize = AtomicUsize::new(0);

/// What the terminal can draw, or `None` where nothing is to be drawn.
static PICKER: OnceLock<Option<Picker>> = OnceLock::new();

/// Ask the terminal what it can draw, once.
///
/// Called after the colour query has finished and before any event is read. That order is not
/// incidental: the picker writes its questions to the terminal and reads the answers straight
/// off the tty, so it must not overlap the background-colour query (which does the same) and it
/// must not run once the event loop is reading, which would take the replies as keystrokes. Both
/// give up after a timeout, so a terminal that answers neither costs a short pause and no more.
pub fn sense() {
    PICKER.get_or_init(|| {
        // The same request that stills the colours stills the pictures: a person who asked for a
        // plain terminal has not asked for graphics.
        if theme::no_color() {
            return None;
        }
        Picker::from_query_stdio().ok()
    });
}

fn picker() -> Option<&'static Picker> {
    PICKER.get()?.as_ref()
}

fn is_real(protocol: ProtocolType) -> bool {
    !matches!(protocol, ProtocolType::Halfblocks)
}

/// Whether a vetting prompt draws a picture of this media type, given what the terminal draws.
///
/// Only a real graphics protocol. A picture drawn in blocks of colour is too coarse to show small
/// or faint writing, which is what a person looking at it is looking for, and a screen that
/// looked at the picture without being able to show it would be false assurance. A PDF is never
/// drawn: it holds pages, and text that no page draws.
pub fn draws_in_a_vetting_prompt(protocol: Option<ProtocolType>, media: &str) -> bool {
    protocol.is_some_and(is_real) && matches!(media, "image/png" | "image/jpeg")
}

/// Where the bytes of a picture come from.
pub enum Source {
    /// Bytes already in hand, a paste.
    Bytes(Vec<u8>),
    /// A file, read on the thread that decodes it.
    File(PathBuf),
}

/// How large a picture is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fit {
    /// The small thumbnail under the input box.
    Thumbnail,
    /// Up to this many columns by rows, for a prompt that gives the picture a screen of its own.
    Within(u16, u16),
}

impl Fit {
    fn size(self) -> Size {
        match self {
            Self::Thumbnail => Size::new(COLUMNS, ROWS),
            Self::Within(columns, rows) => Size::new(columns, rows),
        }
    }
}

/// A picture ready to draw at a fixed size.
pub struct Thumb {
    protocol: Protocol,
}

impl std::fmt::Debug for Thumb {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let size = self.protocol.size();
        write!(f, "Thumb({}x{})", size.width, size.height)
    }
}

impl Thumb {
    /// How many columns it takes.
    pub fn width(&self) -> u16 {
        self.protocol.size().width
    }

    /// How many rows it takes.
    pub fn height(&self) -> u16 {
        self.protocol.size().height
    }

    /// Draw it in `area`, which must be at least as large as [`Thumb::width`] by [`Thumb::height`].
    pub fn draw(&self, frame: &mut Frame, area: Rect) {
        frame.render_widget(Image::new(&self.protocol), area);
    }
}

/// A picture being made ready to draw, or made and ready, or given up on.
#[derive(Debug)]
pub struct Preview {
    state: State,
}

#[derive(Debug)]
enum State {
    Decoding(Receiver<Option<Thumb>>),
    Ready(Thumb),
    Nothing,
}

/// Held for as long as a decode runs, so the count comes back down on every way out of the thread.
struct InFlight;

impl InFlight {
    fn take() -> Option<Self> {
        let before = IN_FLIGHT.fetch_add(1, Ordering::AcqRel);
        if before >= MAX_IN_FLIGHT {
            IN_FLIGHT.fetch_sub(1, Ordering::AcqRel);
            return None;
        }
        Some(Self)
    }
}

impl Drop for InFlight {
    fn drop(&mut self) {
        IN_FLIGHT.fetch_sub(1, Ordering::AcqRel);
    }
}

impl Preview {
    /// Start making a picture from `source`, or give up at once where the terminal draws none.
    pub fn start(source: Source, fit: Fit) -> Self {
        let Some(picker) = picker() else {
            return Self::nothing();
        };
        Self::start_with(picker.clone(), source, fit)
    }

    pub(crate) fn start_with(picker: Picker, source: Source, fit: Fit) -> Self {
        let Some(slot) = InFlight::take() else {
            return Self::nothing();
        };
        let (sent, received) = mpsc::channel();
        let spawned = std::thread::Builder::new()
            .name("picture-preview".into())
            .spawn(move || {
                let _slot = slot;
                let _ = sent.send(make(&picker, source, fit));
            });
        match spawned {
            Ok(_) => Self {
                state: State::Decoding(received),
            },
            Err(_) => Self::nothing(),
        }
    }

    /// No picture, and none coming.
    pub fn nothing() -> Self {
        Self {
            state: State::Nothing,
        }
    }

    /// A picture made already, for a test that has no terminal to ask.
    #[cfg(test)]
    pub(crate) fn ready(thumb: Thumb) -> Self {
        Self {
            state: State::Ready(thumb),
        }
    }

    /// Take a finished decode, if there is one. True when that changed what can be drawn.
    ///
    /// A thread that ended without answering, which is what a panic in a decoder looks like from
    /// here, is a picture that will not come.
    pub fn settle(&mut self) -> bool {
        let State::Decoding(received) = &self.state else {
            return false;
        };
        match received.try_recv() {
            Ok(Some(thumb)) => {
                self.state = State::Ready(thumb);
                true
            }
            Ok(None) | Err(TryRecvError::Disconnected) => {
                self.state = State::Nothing;
                false
            }
            Err(TryRecvError::Empty) => false,
        }
    }

    /// Whether a decode is still running.
    pub fn is_pending(&self) -> bool {
        matches!(self.state, State::Decoding(_))
    }

    /// The picture, once it is ready.
    pub fn thumb(&self) -> Option<&Thumb> {
        match &self.state {
            State::Ready(thumb) => Some(thumb),
            _ => None,
        }
    }
}

fn make(picker: &Picker, source: Source, fit: Fit) -> Option<Thumb> {
    let bytes = match source {
        Source::Bytes(bytes) => bytes,
        Source::File(path) => {
            let length = std::fs::metadata(&path).ok()?.len();
            if length > MAX_ATTACHMENT_BYTES as u64 {
                return None;
            }
            std::fs::read(path).ok()?
        }
    };
    thumbnail_with(picker, &bytes, fit)
}

/// A picture of `bytes` at `fit`, or `None` where they are not a PNG or JPEG within the limits.
pub(crate) fn thumbnail_with(picker: &Picker, bytes: &[u8], fit: Fit) -> Option<Thumb> {
    if bytes.len() > MAX_ATTACHMENT_BYTES {
        return None;
    }
    let reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    if !matches!(reader.format()?, ImageFormat::Png | ImageFormat::Jpeg) {
        return None;
    }
    // The canvas a file declares is read from its header, and judged, before a pixel is allocated.
    let (width, height) = reader.into_dimensions().ok()?;
    if width == 0 || height == 0 || width > MAX_PIXELS_PER_SIDE || height > MAX_PIXELS_PER_SIDE {
        return None;
    }
    let mut reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_PIXELS_PER_SIDE);
    limits.max_image_height = Some(MAX_PIXELS_PER_SIDE);
    limits.max_alloc = Some(MAX_ALLOC_BYTES);
    reader.limits(limits);
    let decoded = reader.decode().ok()?;
    let protocol = picker
        .new_protocol(decoded, fit.size(), Resize::Fit(None))
        .ok()?;
    Some(Thumb { protocol })
}

/// What the terminal draws, for the prompt deciding whether to draw a picture at all.
pub fn protocol() -> Option<ProtocolType> {
    picker().map(Picker::protocol_type)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A solid-colour PNG, built here so no fixture file is needed.
    pub(crate) fn png(width: u32, height: u32) -> Vec<u8> {
        let mut out = Vec::new();
        image::RgbaImage::from_pixel(width, height, image::Rgba([200, 60, 60, 255]))
            .write_to(&mut Cursor::new(&mut out), ImageFormat::Png)
            .expect("a PNG encodes");
        out
    }

    fn jpeg(width: u32, height: u32) -> Vec<u8> {
        let mut out = Vec::new();
        image::RgbImage::from_pixel(width, height, image::Rgb([20, 120, 200]))
            .write_to(&mut Cursor::new(&mut out), ImageFormat::Jpeg)
            .expect("a JPEG encodes");
        out
    }

    /// A picker that draws with `protocol`, as if the terminal had said it could.
    pub(crate) fn drawing_with(protocol: ProtocolType) -> Picker {
        let mut picker = Picker::halfblocks();
        picker.set_protocol_type(protocol);
        picker
    }

    #[test]
    fn a_wide_picture_is_fitted_inside_the_box_it_is_given() {
        let picker = Picker::halfblocks();
        let thumb = thumbnail_with(&picker, &png(1200, 300), Fit::Thumbnail).expect("decodes");
        assert!(thumb.width() <= COLUMNS, "{} columns", thumb.width());
        assert!(thumb.height() <= ROWS, "{} rows", thumb.height());
        assert!(thumb.width() > 0 && thumb.height() > 0);

        let large = thumbnail_with(&picker, &png(1200, 300), Fit::Within(60, 20)).expect("decodes");
        assert!(large.width() > COLUMNS, "{} columns", large.width());
        assert!(large.width() <= 60 && large.height() <= 20);
    }

    #[test]
    fn a_jpeg_is_drawn_as_well() {
        let picker = Picker::halfblocks();
        assert!(thumbnail_with(&picker, &jpeg(300, 200), Fit::Thumbnail).is_some());
    }

    #[test]
    fn bytes_that_are_not_a_picture_give_no_thumbnail() {
        let picker = Picker::halfblocks();
        assert!(thumbnail_with(&picker, b"definitely not a picture", Fit::Thumbnail).is_none());
        assert!(thumbnail_with(&picker, &[], Fit::Thumbnail).is_none());
    }

    #[test]
    fn a_picture_declaring_a_huge_canvas_is_refused_before_it_is_allocated() {
        let picker = Picker::halfblocks();
        // 20000 x 1 is well past the per-side limit; a one-colour PNG of it is small on disk,
        // which is exactly the shape of file the limit is for.
        assert!(thumbnail_with(&picker, &png(20_000, 1), Fit::Thumbnail).is_none());
        assert!(thumbnail_with(&picker, &png(1, 20_000), Fit::Thumbnail).is_none());
        // Just inside the limit still draws.
        assert!(thumbnail_with(&picker, &png(MAX_PIXELS_PER_SIDE, 1), Fit::Thumbnail).is_some());
    }

    #[test]
    fn a_file_over_the_attachment_size_is_not_read_for_a_picture() {
        let picker = Picker::halfblocks();
        let big = vec![0u8; MAX_ATTACHMENT_BYTES + 1];
        assert!(thumbnail_with(&picker, &big, Fit::Thumbnail).is_none());
    }

    #[test]
    fn a_format_that_is_not_compiled_in_is_not_drawn() {
        let picker = Picker::halfblocks();
        assert!(thumbnail_with(&picker, b"GIF89a\x01\x00\x01\x00", Fit::Thumbnail).is_none());
    }

    #[test]
    fn only_a_real_graphics_protocol_draws_in_a_vetting_prompt() {
        for real in [
            ProtocolType::Kitty,
            ProtocolType::Iterm2,
            ProtocolType::Sixel,
        ] {
            assert!(
                draws_in_a_vetting_prompt(Some(real), "image/png"),
                "{real:?}"
            );
            assert!(
                draws_in_a_vetting_prompt(Some(real), "image/jpeg"),
                "{real:?}"
            );
        }
        assert!(!draws_in_a_vetting_prompt(
            Some(ProtocolType::Halfblocks),
            "image/png"
        ));
        assert!(!draws_in_a_vetting_prompt(None, "image/png"));
    }

    #[test]
    fn a_pdf_and_the_formats_not_compiled_in_are_never_drawn_in_a_vetting_prompt() {
        for media in ["application/pdf", "image/gif", "image/webp", ""] {
            assert!(
                !draws_in_a_vetting_prompt(Some(ProtocolType::Kitty), media),
                "{media}"
            );
        }
    }

    #[test]
    fn a_decode_that_panics_is_a_missing_picture_and_not_a_crash() {
        let (sent, received) = mpsc::channel::<Option<Thumb>>();
        let worker = std::thread::spawn(move || {
            let _sent = sent;
            panic!("a decoder gave way");
        });
        assert!(worker.join().is_err());
        let mut preview = Preview {
            state: State::Decoding(received),
        };
        assert!(preview.is_pending());
        assert!(!preview.settle());
        assert!(!preview.is_pending());
        assert!(preview.thumb().is_none());
    }

    #[test]
    fn a_decode_that_finishes_is_taken_once_and_reported() {
        let picker = Picker::halfblocks();
        let mut preview = Preview::start_with(picker, Source::Bytes(png(64, 64)), Fit::Thumbnail);
        for _ in 0..500 {
            if preview.settle() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(preview.thumb().is_some(), "the decode never finished");
        assert!(!preview.settle(), "a settled preview reported a change");
    }

    #[test]
    fn a_file_that_will_not_decode_settles_to_nothing() {
        let picker = Picker::halfblocks();
        let mut preview = Preview::start_with(
            picker,
            Source::File(PathBuf::from("/nonexistent/bravebot-picture.png")),
            Fit::Thumbnail,
        );
        for _ in 0..500 {
            preview.settle();
            if !preview.is_pending() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(!preview.is_pending() && preview.thumb().is_none());
    }
}
