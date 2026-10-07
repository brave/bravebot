//! Moving text and pictures between the terminal and the user's clipboard.
//!
//! Copying goes out two ways, tried in order, because neither works everywhere. The platform's own
//! tool is what works locally and is always there on macOS and Windows; the escape sequence is what
//! works over ssh, where the clipboard that matters belongs to the terminal at the other end and no
//! local tool can reach it.
//!
//! Pasting comes back in one way, and only because the terminal cannot do it. Command-V on macOS
//! never reaches this process at all: the byte stream over a pty has no encoding for that modifier,
//! and the terminal claims the chord for itself in any case. What the terminal does instead is
//! write the clipboard's *text* into the pty, and an image has no text, so the picture a user is
//! looking at is the one thing the ordinary paste cannot carry. Reading the clipboard here goes
//! around the pty entirely, which is why Control-V can move what Command-V cannot.
//!
//! Nothing labelled passes through here. What is copied was read off the screen, or is a reply's
//! text as the transcript holds it for `/copy`, and both were released for display before they were
//! drawn. What is pasted is the user's own input, on the footing of the prompt it lands in, which
//! [`bravebot_core::policy::Policy::admit_pasted_image`] states in full.

use bravebot_agent::turn::MAX_PASTED_IMAGE_BYTES;
use std::io::Write;
use std::process::{Command, Stdio};

/// Put `text` on the clipboard, reporting whether anything took it.
pub fn copy(text: &str) -> bool {
    for (program, arguments) in COPY_TOOLS {
        if pipe_into(program, arguments, text) {
            return true;
        }
    }
    write_escape_sequence(text)
}

/// The clipboard tools worth trying, in the order they are worth trying.
///
/// One per platform that has a certain one, and on the rest the three that a desktop session
/// might have. A tool that is not installed fails to spawn, which is the next one's turn.
#[cfg(target_os = "macos")]
const COPY_TOOLS: &[(&str, &[&str])] = &[("pbcopy", &[])];

#[cfg(target_os = "windows")]
const COPY_TOOLS: &[(&str, &[&str])] = &[("clip", &[])];

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
const COPY_TOOLS: &[(&str, &[&str])] = &[
    ("wl-copy", &[]),
    ("xclip", &["-selection", "clipboard"]),
    ("xsel", &["--clipboard", "--input"]),
];

/// Run a tool and write the text to it, reporting whether it took it.
fn pipe_into(program: &str, arguments: &[&str], text: &str) -> bool {
    let Ok(mut child) = Command::new(program)
        .args(arguments)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    else {
        return false;
    };

    // Taken rather than borrowed so the pipe closes here: a tool that reads to end of input
    // would otherwise wait for a handle this process is still holding.
    if let Some(mut stdin) = child.stdin.take()
        && stdin.write_all(text.as_bytes()).is_err()
    {
        let _ = child.wait();
        return false;
    }

    matches!(child.wait(), Ok(status) if status.success())
}

/// Ask the terminal itself to take the text, with OSC 52.
///
/// The fallback for a machine with no clipboard tool, and the only thing that works over ssh,
/// since it is the terminal at the near end that holds the clipboard the user will paste from.
/// Terminals that do not implement it ignore the sequence, and there is no reply to tell the two
/// apart, so this reports what it managed to write and not what the terminal did with it.
fn write_escape_sequence(text: &str) -> bool {
    use base64::Engine;
    let encoded = base64::engine::general_purpose::STANDARD.encode(text);

    let mut out = std::io::stdout();
    write!(out, "\x1b]52;c;{encoded}\x07").is_ok() && out.flush().is_ok()
}

/// The longest side, in pixels, a picture over the cap is scaled down to before it is tried again.
const DOWNSCALED_SIDE: u32 = 2048;

/// The largest canvas a picture over the cap may declare and still be decoded to be scaled down.
const MAX_DECODED_SIDE: u32 = 16384;

/// The most memory a decode of a picture over the cap may allocate.
const MAX_DECODE_ALLOC_BYTES: u64 = 512 * 1024 * 1024;

/// What the clipboard had when it was asked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pasted {
    Text(String),
    Image(Image),
    /// A picture too big to send even scaled down, with the size it was as pasted.
    TooLarge(usize),
    /// Nothing this can use: an empty clipboard, or one holding something that is neither.
    Nothing,
}

/// A picture off the clipboard, in a form the API takes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    /// Always one of this module's own constants, never a string read from anywhere.
    ///
    /// It ends up in the data URL, where it is routing. Taking it from a filename or from what a
    /// tool printed would be letting whoever wrote that choose it.
    pub media_type: &'static str,
    pub bytes: Vec<u8>,
}

/// Read the clipboard.
///
/// A picture wins over text when the clipboard holds both, which it often does: copying an image in
/// a browser leaves the page's URL as the text flavour, and copying a spreadsheet range leaves a
/// rendering of it. The tie is broken that way because text has another route in and a picture has
/// none. Command-V still pastes the text flavour, works everywhere, and is what the fingers already
/// know, so preferring it here would leave one of the two flavours reachable by nothing at all.
pub fn paste() -> Pasted {
    chosen(image_on_clipboard(), text_on_clipboard)
}

/// Which flavour a read of the clipboard becomes, given what each flavour had on it.
///
/// Apart from the reads themselves, because the reads are the one part of this that cannot be
/// exercised: they shell out to whatever the desktop session happens to have installed, so a test
/// driving [`paste`] would report what the machine it ran on had on its clipboard rather than
/// which of the two flavours this prefers. The tie-break is the whole of the decision, and it is
/// here.
///
/// Text arrives as a closure rather than as a value so that a picture still costs one process
/// instead of two: finding one means the text is never wanted, and reading it would be a second
/// tool spawned for an answer nothing looks at.
fn chosen(image: Option<(&'static str, Vec<u8>)>, text: impl FnOnce() -> Option<String>) -> Pasted {
    match image {
        Some((_, bytes)) if bytes.len() > MAX_PASTED_IMAGE_BYTES => {
            return match downscaled(&bytes) {
                Some(smaller) if smaller.len() <= MAX_PASTED_IMAGE_BYTES => Pasted::Image(Image {
                    media_type: "image/png",
                    bytes: smaller,
                }),
                _ => Pasted::TooLarge(bytes.len()),
            };
        }
        Some((media_type, bytes)) => return Pasted::Image(Image { media_type, bytes }),
        None => {}
    }

    match text() {
        Some(text) if !text.is_empty() => Pasted::Text(text),
        _ => Pasted::Nothing,
    }
}

/// A picture over the cap, scaled so its longest side is at most [`DOWNSCALED_SIDE`] and encoded as
/// PNG, or `None` where it will not decode within the limits.
///
/// The pixels are decoded and re-encoded and never looked at: nothing here branches on what the
/// picture shows. The media type of the result is the reader's own literal, `image/png`.
fn downscaled(bytes: &[u8]) -> Option<Vec<u8>> {
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_DECODED_SIDE);
    limits.max_image_height = Some(MAX_DECODED_SIDE);
    limits.max_alloc = Some(MAX_DECODE_ALLOC_BYTES);
    reader.limits(limits);
    let decoded = reader.decode().ok()?;
    let scaled = decoded.resize(
        DOWNSCALED_SIDE,
        DOWNSCALED_SIDE,
        image::imageops::FilterType::Triangle,
    );
    let mut out = Vec::new();
    scaled
        .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
        .ok()?;
    Some(out)
}

/// Whether the clipboard has a picture on it, without reading the picture.
///
/// Asked so the interface can say that Control-V would do something, which is the whole of how
/// anyone finds out: a chord nothing mentions is a chord nobody presses. It asks the platform for
/// the list of flavours rather than for the bytes, since a screenshot runs to megabytes and
/// fetching one to answer a yes-or-no question would be felt.
///
/// The comparison is against a literal, and what it yields is a constant. Whoever filled the
/// clipboard therefore chooses whether a hint appears and nothing else: not the media type, which
/// this module owns, and not where anything lands.
pub fn holds_an_image() -> bool {
    image_flavour_on_clipboard()
}

#[cfg(target_os = "macos")]
mod platform {
    use super::run_text;

    /// Read the pasteboard through AppKit rather than through AppleScript's own clipboard.
    ///
    /// Both go by way of `osascript`, whose startup costs a few tens of milliseconds and is not
    /// the problem. `the clipboard as «class PNGf»` is: it materialises the picture into an
    /// AppleScript value before anything can be done with it, and for a screenshot that alone runs
    /// to the better part of half a second, which is long enough to feel as lag on the keypress.
    /// The bridge asks the pasteboard for its own bytes, so what is left to pay for is the read.
    ///
    /// Base64 rather than the hex AppleScript hands back: it is what `NSData` already offers, it
    /// is two thirds of the characters, and a stray byte in it fails the decode rather than
    /// silently producing a different picture.
    const SCRIPT: &str = "ObjC.import('AppKit'); \
                          const d = $.NSPasteboard.generalPasteboard.dataForType('public.png'); \
                          d.isNil() ? '' : $.NSString.alloc.initWithDataEncoding(\
                          d.base64EncodedDataWithOptions(0), $.NSUTF8StringEncoding).js";

    pub fn image() -> Option<(&'static str, Vec<u8>)> {
        use base64::Engine;
        let encoded = run_text("osascript", &["-l", "JavaScript", "-e", SCRIPT])?;
        // Half a picture is worse than none: it would be sent, rejected by the endpoint, and
        // reported as a fault of the request rather than of the read that truncated it.
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(encoded.trim())
            .ok()?;
        (!bytes.is_empty()).then_some(("image/png", bytes))
    }

    /// Asked about the one flavour that matters, and only about whether it is on offer.
    ///
    /// Reading the list of types the pasteboard advertises never materialises what is behind one,
    /// which is what makes this affordable on a focus change. The older way of asking, through
    /// AppleScript's `clipboard info`, priced a yes-or-no question at the size of the picture.
    const PROBE: &str = "ObjC.import('AppKit'); \
                         $.NSPasteboard.generalPasteboard.types.containsObject('public.png') \
                         ? 'yes' : 'no'";

    pub fn has_image() -> bool {
        run_text("osascript", &["-l", "JavaScript", "-e", PROBE])
            .is_some_and(|offered| offered.trim() == "yes")
    }

    pub const TEXT_TOOLS: &[(&str, &[&str])] = &[("pbpaste", &[])];
}

#[cfg(target_os = "windows")]
mod platform {
    use super::run_text;

    /// PowerShell renders whatever picture the clipboard holds as PNG and prints it base64, since
    /// binary down a pipe from PowerShell is mangled by the console encoding.
    const SCRIPT: &str = "Add-Type -AssemblyName System.Windows.Forms,System.Drawing; \
                          $image = [Windows.Forms.Clipboard]::GetImage(); \
                          if ($image) { $stream = New-Object IO.MemoryStream; \
                          $image.Save($stream, [Drawing.Imaging.ImageFormat]::Png); \
                          [Convert]::ToBase64String($stream.ToArray()) }";

    pub fn image() -> Option<(&'static str, Vec<u8>)> {
        use base64::Engine;
        let encoded = run_text("powershell", &["-NoProfile", "-Command", SCRIPT])?;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(encoded.trim())
            .ok()?;
        (!bytes.is_empty()).then_some(("image/png", bytes))
    }

    /// No cheaper answer than the picture itself here, so this reads it and throws it away. Only
    /// asked when the terminal regains focus, which is rare enough to afford it.
    pub fn has_image() -> bool {
        image().is_some()
    }

    pub const TEXT_TOOLS: &[(&str, &[&str])] =
        &[("powershell", &["-NoProfile", "-Command", "Get-Clipboard"])];
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod platform {
    use super::{run_bytes, run_text};

    /// Both display servers, since which one is running is not something this can assume and a tool
    /// for the other simply fails to spawn.
    const IMAGE_TOOLS: &[(&str, &[&str])] = &[
        ("wl-paste", &["--no-newline", "--type", "image/png"]),
        (
            "xclip",
            &["-selection", "clipboard", "-t", "image/png", "-o"],
        ),
    ];

    const FLAVOUR_TOOLS: &[(&str, &[&str])] = &[
        ("wl-paste", &["--list-types"]),
        ("xclip", &["-selection", "clipboard", "-t", "TARGETS", "-o"]),
    ];

    pub fn image() -> Option<(&'static str, Vec<u8>)> {
        for (program, arguments) in IMAGE_TOOLS {
            match run_bytes(program, arguments) {
                Some(bytes) if !bytes.is_empty() => return Some(("image/png", bytes)),
                _ => {}
            }
        }
        None
    }

    pub fn has_image() -> bool {
        FLAVOUR_TOOLS.iter().any(|(program, arguments)| {
            run_text(program, arguments).is_some_and(|types| types.contains("image/png"))
        })
    }

    pub const TEXT_TOOLS: &[(&str, &[&str])] = &[
        ("wl-paste", &["--no-newline"]),
        ("xclip", &["-selection", "clipboard", "-o"]),
        ("xsel", &["--clipboard", "--output"]),
    ];
}

fn image_on_clipboard() -> Option<(&'static str, Vec<u8>)> {
    platform::image()
}

fn image_flavour_on_clipboard() -> bool {
    platform::has_image()
}

/// The clipboard's text, from the first tool that has any.
///
/// A tool that is not installed fails to spawn, which is the next one's turn, exactly as copying
/// works. A tool that runs and finds nothing has answered, and the answer is that there is nothing.
fn text_on_clipboard() -> Option<String> {
    for (program, arguments) in platform::TEXT_TOOLS {
        if let Some(text) = run_text(program, arguments) {
            return Some(text);
        }
    }
    None
}

/// Run a tool and take what it printed, or nothing if it could not run or refused.
///
/// A non-zero exit is a refusal: `osascript` returns one when the clipboard holds no picture, which
/// is the ordinary case and not a fault worth reporting.
fn run_bytes(program: &str, arguments: &[&str]) -> Option<Vec<u8>> {
    let output = Command::new(program)
        .args(arguments)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    output.status.success().then_some(output.stdout)
}

fn run_text(program: &str, arguments: &[&str]) -> Option<String> {
    String::from_utf8(run_bytes(program, arguments)?).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The sequence has to be exactly what a terminal recognises, and a test is the only place
    /// that can say so, since a terminal that does not recognise it says nothing at all.
    #[test]
    fn the_escape_sequence_is_the_one_terminals_read() {
        use base64::Engine;
        let encoded = base64::engine::general_purpose::STANDARD.encode("hello");
        assert_eq!(encoded, "aGVsbG8=");
        assert_eq!(format!("\x1b]52;c;{encoded}\x07"), "\x1b]52;c;aGVsbG8=\x07");
    }

    /// A tool that is not installed is not a failure to copy, it is the next tool's turn.
    #[test]
    fn a_missing_tool_is_not_taken_for_a_successful_copy() {
        assert!(!pipe_into(
            "a-clipboard-tool-that-does-not-exist",
            &[],
            "hello"
        ));
    }

    /// A machine with none of these tools installed is the ordinary case on a bare server, and it
    /// has to read as an empty clipboard rather than as anything a caller must handle.
    #[test]
    fn a_missing_tool_reads_as_nothing_on_the_clipboard() {
        assert_eq!(run_text("a-clipboard-tool-that-does-not-exist", &[]), None);
    }

    /// Copying an image in a browser leaves the page's URL behind as the text flavour, so the
    /// clipboard holding both is the ordinary case rather than the odd one. Text has Command-V,
    /// which works everywhere; a picture has this and nothing else, so preferring text here would
    /// leave the picture reachable by no key at all.
    #[test]
    fn a_picture_wins_over_the_text_beside_it() {
        let chosen = chosen(Some(("image/png", b"pixels".to_vec())), || {
            Some("https://example.invalid/the-page".to_string())
        });

        assert_eq!(
            chosen,
            Pasted::Image(Image {
                media_type: "image/png",
                bytes: b"pixels".to_vec(),
            })
        );
    }

    /// The text beside a picture is not read at all when the picture is there to be had. Reading
    /// it is another process spawned for an answer nothing goes on to look at, and the keypress
    /// waits on it.
    #[test]
    fn the_text_beside_a_picture_is_never_read() {
        let mut asked = false;
        let chosen = chosen(Some(("image/png", b"pixels".to_vec())), || {
            asked = true;
            None
        });

        assert!(matches!(chosen, Pasted::Image(_)));
        assert!(!asked, "the clipboard's text was read for nothing");
    }

    /// A noisy PNG whose encoding is over the cap, so a plain re-encode would not fit either.
    fn oversized_png(side: u32) -> Vec<u8> {
        let mut state = 0x2545_f491_u32;
        let noise = image::RgbImage::from_fn(side, side, |_, _| {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            image::Rgb([
                (state >> 20) as u8 & 0x3f,
                (state >> 12) as u8 & 0x3f,
                (state >> 4) as u8 & 0x3f,
            ])
        });
        let mut out = Vec::new();
        noise
            .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
            .expect("a PNG encodes");
        assert!(
            out.len() > MAX_PASTED_IMAGE_BYTES,
            "the fixture must be over the cap"
        );
        out
    }

    /// A PNG declaring `width` by `height` one-bit paletted pixels, all of them the first palette
    /// entry. The pixel data is stored rather than compressed, so the file is over the cap while
    /// the picture it declares decodes to three bytes a pixel.
    fn declared_png(width: u32, height: u32) -> Vec<u8> {
        fn crc32(parts: &[&[u8]]) -> u32 {
            let mut crc = u32::MAX;
            for byte in parts.iter().flat_map(|part| part.iter()) {
                crc ^= u32::from(*byte);
                for _ in 0..8 {
                    crc = (crc >> 1) ^ (0xedb8_8320 & 0u32.wrapping_sub(crc & 1));
                }
            }
            !crc
        }
        fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
            out.extend_from_slice(&(data.len() as u32).to_be_bytes());
            out.extend_from_slice(kind);
            out.extend_from_slice(data);
            out.extend_from_slice(&crc32(&[kind, data]).to_be_bytes());
        }

        let row = 1 + width.div_ceil(8) as usize;
        let raw = vec![0u8; row * height as usize];
        let (mut a, mut b) = (1u32, 0u32);
        let mut zlib = vec![0x78, 0x01];
        let blocks: Vec<&[u8]> = raw.chunks(u16::MAX as usize).collect();
        for (n, block) in blocks.iter().enumerate() {
            zlib.push(u8::from(n + 1 == blocks.len()));
            zlib.extend_from_slice(&(block.len() as u16).to_le_bytes());
            zlib.extend_from_slice(&(!(block.len() as u16)).to_le_bytes());
            zlib.extend_from_slice(block);
            for byte in *block {
                a = (a + u32::from(*byte)) % 65521;
                b = (b + a) % 65521;
            }
        }
        zlib.extend_from_slice(&((b << 16) | a).to_be_bytes());

        let mut header = Vec::new();
        header.extend_from_slice(&width.to_be_bytes());
        header.extend_from_slice(&height.to_be_bytes());
        header.extend_from_slice(&[1, 3, 0, 0, 0]);
        let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
        chunk(&mut out, b"IHDR", &header);
        chunk(&mut out, b"PLTE", &[0, 0, 0, 255, 255, 255]);
        chunk(&mut out, b"IDAT", &zlib);
        chunk(&mut out, b"IEND", &[]);
        assert!(
            out.len() > MAX_PASTED_IMAGE_BYTES,
            "the fixture must be over the cap"
        );
        out
    }

    /// Random pixels in every bit of every channel, so the file does not compress and a scaled
    /// copy still has the noise in it.
    fn incompressible_png(side: u32) -> Vec<u8> {
        let mut state = 0x9e37_79b9_u32;
        let noise = image::RgbaImage::from_fn(side, side, |_, _| {
            let mut next = || {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (state >> 16) as u8
            };
            image::Rgba([next(), next(), next(), next()])
        });
        let mut out = Vec::new();
        noise
            .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
            .expect("a PNG encodes");
        assert!(
            out.len() > MAX_PASTED_IMAGE_BYTES,
            "the fixture must be over the cap"
        );
        out
    }

    /// A picture that declares a canvas wider than the decoder is allowed to open is refused with
    /// the size it was pasted at. Decoding it anyway lets one clipboard entry claim memory the
    /// size of its declared canvas rather than of its file.
    #[test]
    fn a_picture_over_the_cap_declaring_too_wide_a_canvas_is_refused_unread() {
        let wide = declared_png(MAX_DECODED_SIDE + 1, 8192);
        let size = wide.len();

        let pasted = chosen(Some(("image/png", wide)), || None);
        assert!(
            matches!(pasted, Pasted::TooLarge(pasted_size) if pasted_size == size),
            "the picture was not refused at its pasted size"
        );
    }

    /// A picture whose canvas is within the side limit but would decode to more than the allocation
    /// limit is refused with the size it was pasted at.
    #[test]
    fn a_picture_over_the_cap_that_would_decode_past_the_allocation_limit_is_refused_unread() {
        let heavy = declared_png(MAX_DECODED_SIDE, MAX_DECODED_SIDE);
        let size = heavy.len();

        let pasted = chosen(Some(("image/png", heavy)), || None);
        assert!(
            matches!(pasted, Pasted::TooLarge(pasted_size) if pasted_size == size),
            "the picture was not refused at its pasted size"
        );
    }

    /// A picture that is still over the cap once it is scaled is refused with the size it was
    /// pasted at, and the over-cap copy is not sent in its place.
    #[test]
    fn a_picture_still_over_the_cap_after_scaling_is_refused_with_its_pasted_size() {
        let noisy = incompressible_png(2400);
        let size = noisy.len();
        let scaled = downscaled(&noisy).expect("the fixture decodes");
        assert!(
            scaled.len() > MAX_PASTED_IMAGE_BYTES,
            "the scaled fixture must still be over the cap"
        );

        let pasted = chosen(Some(("image/png", noisy)), || None);
        assert!(
            matches!(pasted, Pasted::TooLarge(pasted_size) if pasted_size == size),
            "the picture was not refused at its pasted size"
        );
    }

    /// A screenshot over the cap goes through scaled down rather than making the person shrink it
    /// by hand. The result has to be a picture that decodes, under the cap, whose longest side was
    /// brought to the fixed size, and whose type is the reader's own literal.
    #[test]
    fn a_picture_over_the_cap_is_scaled_down_and_sent() {
        let oversized = oversized_png(2800);
        let chosen = chosen(Some(("image/png", oversized)), || None);

        let Pasted::Image(image) = chosen else {
            panic!("an oversized picture that decodes was not admitted: {chosen:?}");
        };
        assert_eq!(image.media_type, "image/png");
        assert!(image.bytes.len() <= MAX_PASTED_IMAGE_BYTES);
        let decoded = image::load_from_memory(&image.bytes).expect("the result decodes");
        assert_eq!(decoded.width().max(decoded.height()), DOWNSCALED_SIDE);
    }

    /// A picture the decoder cannot read is still refused with the size it was pasted at, and not
    /// swapped for the text beside it: falling through would paste a page's URL in place of the
    /// screenshot somebody meant, with nothing said about the one they asked for.
    #[test]
    fn a_picture_over_the_cap_that_will_not_decode_is_refused_rather_than_swapped_for_the_text() {
        let oversized = vec![0u8; MAX_PASTED_IMAGE_BYTES + 1];
        let chosen = chosen(Some(("image/png", oversized)), || {
            Some("https://example.invalid/the-page".to_string())
        });

        assert_eq!(chosen, Pasted::TooLarge(MAX_PASTED_IMAGE_BYTES + 1));
    }

    /// With no picture, the text is the paste. This is what the ordinary case reduces to, and it
    /// is what makes the preference above a preference rather than a rule that drops text.
    #[test]
    fn text_alone_is_the_paste() {
        let chosen = chosen(None, || Some("some words".to_string()));

        assert_eq!(chosen, Pasted::Text("some words".to_string()));
    }

    /// A tool that ran and found an empty string has answered, and the answer is that there is
    /// nothing. Carried through as text it would be a paste that inserts nothing, which reads to
    /// the user as the key having failed.
    #[test]
    fn an_empty_clipboard_reads_as_nothing_rather_than_as_empty_text() {
        assert_eq!(chosen(None, || Some(String::new())), Pasted::Nothing);
        assert_eq!(chosen(None, || None), Pasted::Nothing);
    }
}
