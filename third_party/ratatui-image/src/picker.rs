//! Helper module to build a protocol, and swap protocols at runtime

use std::{
    env,
    io::{self, Write},
};

use crate::{
    FontSize, Resize, Result,
    errors::Errors,
    protocol::{
        Protocol, StatefulProtocol, StatefulProtocolType,
        halfblocks::Halfblocks,
        iterm2::Iterm2,
        kitty::{Kitty, StatefulKitty},
        sixel::Sixel,
    },
};
use cap_parser::{Parser, QueryStdioOptions, Response};
use image::{DynamicImage, Rgba};
use rand::random;
use ratatui::layout::Size;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

pub mod cap_parser;

#[derive(Debug, PartialEq, Clone)]
pub enum Capability {
    /// Reports supporting kitty graphics protocol.
    Kitty,
    /// Reports supporting sixel graphics protocol.
    Sixel,
    /// Reports supporting rectangular ops.
    RectangularOps,
    /// Reports being able to inflate a zlib-compressed kitty transmission
    /// (`o=z`).
    ///
    /// Only probed for, and so only ever present, when
    /// [`cap_parser::QueryStdioOptions::kitty_compression`] is set: it
    /// optimises for bandwidth at the cost of render latency, so it is off
    /// by default and you probably want it off. See that field's doc for
    /// when it's worth turning on.
    KittyCompression,
    /// Reports font size in pixels.
    CellSize(Option<(u16, u16)>),
    /// Reports supporting text sizing protocol.
    TextSizingProtocol,
    /// Reports a background color.
    Background(u8, u8, u8),
}

const STDIN_READ_TIMEOUT_MILLIS: u64 = 2000;

#[derive(Clone, Debug)]
pub struct Picker {
    font_size: FontSize,
    protocol_type: ProtocolType,
    background_color: Option<Rgba<u8>>,
    pub(crate) is_tmux: bool,
    capabilities: Vec<Capability>,
}

/// Serde-friendly protocol-type enum for [Picker].
#[derive(PartialEq, Eq, Clone, Debug, Copy)]
#[cfg_attr(
    feature = "serde",
    derive(Deserialize, Serialize),
    serde(rename_all = "lowercase")
)]
pub enum ProtocolType {
    Halfblocks,
    Sixel,
    Kitty,
    Iterm2,
}

impl ProtocolType {
    pub fn next(&self) -> ProtocolType {
        match self {
            ProtocolType::Halfblocks => ProtocolType::Sixel,
            ProtocolType::Sixel => ProtocolType::Kitty,
            ProtocolType::Kitty => ProtocolType::Iterm2,
            ProtocolType::Iterm2 => ProtocolType::Halfblocks,
        }
    }
}

/// Helper for building widgets
impl Picker {
    /// Query terminal stdio for graphics capabilities and font-size with some escape sequences.
    ///
    /// This writes and reads from stdio momentarily. WARNING: this method should be called after
    /// entering alternate screen but before reading terminal events.
    ///
    /// # Example
    /// ```rust
    /// use ratatui_image::picker::Picker;
    /// let mut picker = Picker::from_query_stdio();
    /// ```
    ///
    pub fn from_query_stdio() -> Result<Self> {
        Picker::from_query_stdio_with_options(QueryStdioOptions::default())
    }

    /// This should ONLY be used if [Capability::TextSizingProtocol] is needed for some external
    /// reason.
    ///
    /// Query for additional capabilities, currently supports querying for [Text Sizing Protocol].
    ///
    /// The result can be checked by searching for [Capability::TextSizingProtocol] in [Picker::capabilities].
    ///
    /// [Text Sizing Protocol] <https://sw.kovidgoyal.net/kitty/text-sizing-protocol//>
    pub fn from_query_stdio_with_options(options: QueryStdioOptions) -> Result<Self> {
        Self::from_query_stdio_keeping_input(options, &mut Vec::new())
    }

    /// [`Picker::from_query_stdio_with_options`], with every byte read off stdin that was not part
    /// of a reply appended to `typed`, in the order it was read.
    ///
    /// Those are keys typed while the query waited. A caller that reads the terminal afterwards
    /// hands them on rather than losing them. The query reads on the calling thread and starts no
    /// reader that outlives it, and it reads the terminal itself rather than `Stdin`, so nothing is
    /// left in the standard library's buffer. On Unix that is the file descriptor. On Windows it is
    /// the console's input records, and `typed` holds each record that is not part of a reply as
    /// the 20 bytes the record occupies in memory, which crossterm's `event::push_input` takes.
    pub fn from_query_stdio_keeping_input(
        options: QueryStdioOptions,
        typed: &mut Vec<u8>,
    ) -> Result<Self> {
        // Detect tmux, and only if positive then take some risky guess for iTerm2 support.
        let (is_tmux, tmux_proto) = detect_tmux_and_outer_protocol_from_env();

        let mut options_with_blacklist = options;
        let is_wezterm = env::var("WEZTERM_EXECUTABLE").is_ok_and(|s| !s.is_empty());
        let is_konsole = env::var("KONSOLE_VERSION").is_ok_and(|s| !s.is_empty());
        if is_wezterm || is_konsole {
            // WezTerm could use Sixel, but iTerm2 (detected later is better).
            // Konsole's Sixel implementation is buggy: https://github.com/ratatui/ratatui-image?tab=readme-ov-file#compatibility-matrix
            // Neither implement the placeholder part of kitty correctly.
            options_with_blacklist.blacklist_protocols =
                vec![ProtocolType::Kitty, ProtocolType::Sixel];
        }

        // Write and read to stdin to query protocol capabilities and font-size.
        match query_with_timeout(is_tmux, options_with_blacklist, typed) {
            Ok((capability_proto, font_size, caps)) => {
                let iterm2_proto = iterm2_from_env();

                // IO-based detection is authoritative; env-based hints are fallbacks
                // (env vars like KITTY_WINDOW_ID can be stale in tmux sessions).
                let protocol_type = capability_proto
                    .or(tmux_proto)
                    .or(iterm2_proto)
                    .unwrap_or(ProtocolType::Halfblocks);

                if let Some(font_size) = font_size {
                    Ok(Self {
                        font_size,
                        background_color: None,
                        protocol_type,
                        is_tmux,
                        capabilities: caps,
                    })
                } else {
                    let mut p = DEFAULT_PICKER.clone();
                    p.is_tmux = is_tmux;
                    Ok(p)
                }
            }
            // The terminal did not answer the query, but it may still be possible to figure out
            // the font-size with an ioctl, and env vars may still hint at iTerm2 support. This
            // happens for example on Windows ConPTY, which does not reliably deliver the
            // responses to the child process.
            Err(Errors::NoCap | Errors::NoStdinResponse | Errors::NoFontSize) => {
                Ok(fallback_picker(
                    is_tmux,
                    tmux_proto.or_else(iterm2_from_env),
                    font_size_fallback(),
                ))
            }
            Err(err) => Err(err),
        }
    }

    /// The picker [`Picker::from_query_stdio_with_options`] falls back to when the terminal does not
    /// answer, built without asking: what the environment and the font-size ioctl say.
    pub fn without_query() -> Self {
        let (is_tmux, tmux_proto) = detect_tmux_and_outer_protocol_from_env();
        fallback_picker(
            is_tmux,
            tmux_proto.or_else(iterm2_from_env),
            font_size_fallback(),
        )
    }

    /// Create a picker that is guaranteed to only work with Halfblocks.
    ///
    /// # Example
    /// ```rust
    /// use ratatui_image::picker::Picker;
    ///
    /// let mut picker = Picker::halfblocks();
    /// ```
    pub fn halfblocks() -> Self {
        // Detect tmux, ignore iTerm2 as we don't have font-size.
        let (is_tmux, _tmux_proto) = detect_tmux_and_outer_protocol_from_env();

        Self {
            font_size: FontSize::new(10, 20),
            background_color: None,
            protocol_type: ProtocolType::Halfblocks,
            is_tmux,
            capabilities: Vec::new(),
        }
    }

    /// Create a picker from a given terminal [FontSize].
    #[deprecated(
        since = "9.0.0",
        note = "use `from_query_stdio` or `halfblocks` instead"
    )]
    pub fn from_fontsize(font_size: FontSize) -> Self {
        // Detect tmux, and if positive then take some risky guess for iTerm2 support.
        let (is_tmux, tmux_proto) = detect_tmux_and_outer_protocol_from_env();

        // Disregard protocol-from-capabilities if some env var says that we could try iTerm2.
        let iterm2_proto = iterm2_from_env();

        let protocol_type = tmux_proto
            .or(iterm2_proto)
            .unwrap_or(ProtocolType::Halfblocks);

        Self {
            font_size,
            background_color: None,
            protocol_type,
            is_tmux,
            capabilities: Vec::new(),
        }
    }

    /// Returns the current protocol type.
    pub fn protocol_type(&self) -> ProtocolType {
        self.protocol_type
    }

    /// Force a protocol type.
    pub fn set_protocol_type(&mut self, protocol_type: ProtocolType) {
        self.protocol_type = protocol_type;
    }

    /// Returns the [FontSize] detected by [Picker::from_query_stdio].
    pub fn font_size(&self) -> FontSize {
        self.font_size
    }

    /// Whether launch-time detection (from `TERM`/`TERM_PROGRAM`) concluded this
    /// app is running inside tmux.
    ///
    /// This is a best-effort guess, not a guarantee — tmux does not announce
    /// itself unambiguously in every configuration.
    pub fn tmux_detected(&self) -> bool {
        self.is_tmux
    }

    /// Change the default background color (transparent black).
    pub fn set_background_color<T: Into<Rgba<u8>>>(&mut self, background_color: Option<T>) {
        self.background_color = background_color.map(Into::into);
    }

    /// Returns the capabilities detected by [Picker::from_query_stdio].
    pub fn capabilities(&self) -> &Vec<Capability> {
        &self.capabilities
    }

    /// Returns a new protocol.
    ///
    /// The image must match the given area at the terminal's current font size.
    pub(crate) fn new_protocol_raw(&self, image: DynamicImage, size: Size) -> Result<Protocol> {
        match self.protocol_type {
            ProtocolType::Halfblocks => Ok(Protocol::Halfblocks(Halfblocks::new(image, size)?)),
            ProtocolType::Sixel => Ok(Protocol::Sixel(Sixel::new(image, size, self.is_tmux)?)),
            ProtocolType::Kitty => Ok(Protocol::Kitty(Kitty::new(
                image,
                size,
                rand::random(),
                self.is_tmux,
                self.capabilities.contains(&Capability::KittyCompression),
            )?)),
            ProtocolType::Iterm2 => Ok(Protocol::ITerm2(Iterm2::new(image, size, self.is_tmux)?)),
        }
    }

    /// Returns a new protocol for [`crate::Image`] widgets that fits into the given size.
    pub fn new_protocol(
        &self,
        image: DynamicImage,
        size: Size,
        resize: Resize,
    ) -> Result<Protocol> {
        let desired =
            Resize::round_pixel_size_to_cells(image.width(), image.height(), self.font_size);
        let (image, area) =
            match resize.needs_resize(&image, Some(desired), self.font_size, None, size, false) {
                Some(area) => {
                    let image = resize.resize(&image, self.font_size, area, self.background_color);
                    (image, area)
                }
                None => (image, desired),
            };

        self.new_protocol_raw(image, area)
    }

    /// Returns a new *stateful* protocol for [`crate::StatefulImage`] widgets.
    pub fn new_resize_protocol(&self, image: DynamicImage) -> StatefulProtocol {
        let protocol_type = match self.protocol_type {
            ProtocolType::Halfblocks => StatefulProtocolType::Halfblocks(Halfblocks::default()),
            ProtocolType::Sixel => StatefulProtocolType::Sixel(Sixel {
                is_tmux: self.is_tmux,
                ..Sixel::default()
            }),
            ProtocolType::Kitty => StatefulProtocolType::Kitty(StatefulKitty::new(
                random(),
                self.is_tmux,
                self.capabilities.contains(&Capability::KittyCompression),
            )),
            ProtocolType::Iterm2 => StatefulProtocolType::ITerm2(Iterm2 {
                is_tmux: self.is_tmux,
                ..Iterm2::default()
            }),
        };
        StatefulProtocol::new(image, self.font_size, self.background_color, protocol_type)
    }
}

static DEFAULT_PICKER: Picker = Picker {
    // This is completely arbitrary. For halfblocks, it doesn't have to be precise
    // since we're not rendering pixels. It should be roughly 1:2 ratio, and some
    // reasonable size.
    font_size: FontSize::new(10, 20),
    background_color: None,
    protocol_type: ProtocolType::Halfblocks,
    is_tmux: false,
    capabilities: Vec::new(),
};

/// Build a picker from whatever could be detected without the terminal answering the query.
///
/// A protocol other than halfblocks can only render meaningfully with an actual font-size, so
/// without one the `protocol_type` hint is discarded, just like on the query's success path.
fn fallback_picker(
    is_tmux: bool,
    protocol_type: Option<ProtocolType>,
    font_size: Option<FontSize>,
) -> Picker {
    let mut picker = DEFAULT_PICKER.clone();
    picker.is_tmux = is_tmux;
    if let Some(font_size) = font_size {
        picker.font_size = font_size;
        picker.protocol_type = protocol_type.unwrap_or(ProtocolType::Halfblocks);
    }
    picker
}

fn detect_tmux_and_outer_protocol_from_env() -> (bool, Option<ProtocolType>) {
    // Check if we're inside tmux.
    if !env::var("TERM").is_ok_and(|term| term.starts_with("tmux"))
        && !env::var("TERM_PROGRAM").is_ok_and(|term_program| term_program == "tmux")
    {
        return (false, None);
    }

    // The published crate started `tmux set -p allow-passthrough on` here, by bare name and with
    // the whole environment. Nothing is started: a person who wants pictures in tmux sets
    // `allow-passthrough` themselves (SANDBOX-30, third_party/README.md).

    // Crude guess based on the *existence* of some magic program specific env vars.
    // Note: kitty is detected via io query (which works through tmux passthrough),
    // not env vars, since KITTY_WINDOW_ID is often stale in tmux sessions.
    const OUTER_TERM_HINTS: [(&str, ProtocolType); 2] = [
        ("ITERM_SESSION_ID", ProtocolType::Iterm2),
        ("WEZTERM_EXECUTABLE", ProtocolType::Iterm2),
    ];
    for (hint, proto) in OUTER_TERM_HINTS {
        if env::var(hint).is_ok_and(|s| !s.is_empty()) {
            return (true, Some(proto));
        }
    }
    (true, None)
}

fn iterm2_from_env() -> Option<ProtocolType> {
    if env::var("TERM_PROGRAM").is_ok_and(|term_program| {
        term_program.contains("iTerm")
            || term_program.contains("WezTerm")
            || term_program.contains("mintty")
            || term_program.contains("vscode")
            || term_program.contains("Tabby")
            || term_program.contains("Hyper")
            || term_program.contains("rio")
            || term_program.contains("Bobcat")
            || term_program.contains("WarpTerminal")
    }) {
        return Some(ProtocolType::Iterm2);
    }
    if env::var("LC_TERMINAL").is_ok_and(|lc_term| lc_term.contains("iTerm")) {
        return Some(ProtocolType::Iterm2);
    }
    None
}

#[cfg(not(windows))]
fn enable_raw_mode() -> Result<impl FnOnce() -> Result<()>> {
    use rustix::termios::{self, LocalModes, OptionalActions};

    let stdin = io::stdin();
    let mut termios = termios::tcgetattr(&stdin)?;
    let termios_original = termios.clone();

    // Disable canonical mode to read without waiting for Enter, disable echoing.
    termios.local_modes &= !LocalModes::ICANON;
    termios.local_modes &= !LocalModes::ECHO;
    // A read returns after a tenth of a second with nothing, so the query can stop at its timeout
    // on the thread that asked rather than leave a read blocked behind it.
    termios.special_codes[termios::SpecialCodeIndex::VMIN] = 0;
    termios.special_codes[termios::SpecialCodeIndex::VTIME] = 1;
    termios::tcsetattr(&stdin, OptionalActions::Drain, &termios)?;

    Ok(move || {
        Ok(termios::tcsetattr(
            io::stdin(),
            OptionalActions::Now,
            &termios_original,
        )?)
    })
}

/// The console's input handle, set so a read waits for no line and echoes nothing, and what puts
/// its mode back and closes the handle. The handle is closed on every path that returns an error.
#[cfg(windows)]
fn enable_raw_mode() -> Result<(
    windows::Win32::Foundation::HANDLE,
    impl FnOnce() -> Result<()>,
)> {
    use windows::{
        Win32::{
            Foundation::{self, GENERIC_READ, GENERIC_WRITE, HANDLE},
            Storage::FileSystem::{
                self, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
            },
            System::Console::{
                self, CONSOLE_MODE, ENABLE_ECHO_INPUT, ENABLE_LINE_INPUT, ENABLE_PROCESSED_INPUT,
            },
        },
        core::PCWSTR,
    };

    let utf16: Vec<u16> = "CONIN$\0".encode_utf16().collect();
    let utf16_ptr: *const u16 = utf16.as_ptr();

    let in_handle = unsafe {
        FileSystem::CreateFileW(
            PCWSTR(utf16_ptr),
            (GENERIC_READ | GENERIC_WRITE).0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            FILE_FLAGS_AND_ATTRIBUTES(0),
            HANDLE::default(),
        )
    }?;

    let mut original_in_mode = CONSOLE_MODE::default();
    // SAFETY: `in_handle` is the console input handle opened above, still open, and
    // `original_in_mode` is a local the call writes and that outlives it.
    // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
    let set = unsafe { Console::GetConsoleMode(in_handle, &mut original_in_mode) }.and_then(|()| {
        let requested_in_modes =
            !ENABLE_ECHO_INPUT & !ENABLE_LINE_INPUT & !ENABLE_PROCESSED_INPUT;
        let in_mode = original_in_mode & requested_in_modes;
        // SAFETY: `in_handle` is still open, and the call takes it and a mode by value and reads
        // no memory.
        // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
        unsafe { Console::SetConsoleMode(in_handle, in_mode) }
    });
    if let Err(err) = set {
        // The mode is unchanged, so closing the handle is all there is to undo.
        // SAFETY: `in_handle` was opened above and is owned here. It is closed once, and this
        // path returns without using it again or handing it out.
        // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
        let _ = unsafe { Foundation::CloseHandle(in_handle) };
        return Err(err.into());
    }

    Ok((in_handle, move || {
        // SAFETY: the handle stays open until the call below closes it, and the call takes it and
        // a mode by value and reads no memory.
        // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
        let restored = unsafe { Console::SetConsoleMode(in_handle, original_in_mode) };
        // SAFETY: the handle is the one opened above, and this closure is `FnOnce`, so it is closed
        // once. The caller uses the copy it was handed only before calling this closure.
        // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
        let closed = unsafe { Foundation::CloseHandle(in_handle) };
        restored?;
        closed?;
        Ok(())
    }))
}

#[cfg(not(windows))]
fn font_size_fallback() -> Option<FontSize> {
    use rustix::termios::{self, Winsize};

    let winsize = termios::tcgetwinsize(io::stdout()).ok()?;
    let Winsize {
        ws_xpixel: x,
        ws_ypixel: y,
        ws_col: cols,
        ws_row: rows,
    } = winsize;
    if x == 0 || y == 0 || cols == 0 || rows == 0 {
        return None;
    }

    Some(FontSize::new(x / cols, y / rows))
}

#[cfg(windows)]
fn font_size_fallback() -> Option<FontSize> {
    None
}

/// Query the terminal on this thread, giving up at the timeout.
///
/// Reads the console's input records rather than `Stdin`, whose buffer would keep what it read
/// past the replies, and reads only records the console already holds, so no read waits and none
/// outlives the timeout. While the console holds none it sleeps for a few milliseconds and asks
/// again, which needs no wait function from a part of the `windows` crate this one does not
/// enable. A reply arrives as key events carrying its characters. `enable_raw_mode` clears the
/// bits crossterm's raw mode clears (line input, echo, processed input) and keeps the rest, so the
/// replies are read a key at a time and none is echoed, under crossterm's raw mode or without it.
/// Every record that is not part of a reply is appended to `typed`, in the order read.
#[cfg(windows)]
fn query_stdio_capabilities(
    is_tmux: bool,
    options: QueryStdioOptions,
    input: windows::Win32::Foundation::HANDLE,
    typed: &mut Vec<u8>,
) -> Result<(Option<ProtocolType>, Option<FontSize>, Vec<Capability>)> {
    use std::time::{Duration, Instant};
    use windows::Win32::System::Console::{self, INPUT_RECORD, KEY_EVENT};

    /// How long to sleep while the console holds no record before asking again.
    const IDLE: Duration = Duration::from_millis(10);

    let timeout = options.timeout;
    let query = Parser::query(is_tmux, options);
    io::stdout().write_all(query.as_bytes())?;
    io::stdout().flush()?;

    let mut replies = ConsoleReplies::default();
    let mut records = Vec::new();
    // From the question rather than from the last record read, so a person typing while it waits
    // does not keep it waiting.
    let asked = Instant::now();
    let read = loop {
        if replies.done() {
            break Ok(());
        }
        let left = timeout.saturating_sub(asked.elapsed());
        if left.is_zero() {
            break Err(Errors::NoStdinResponse);
        }
        // Asked first because a read of an empty console waits for a record with no bound.
        let mut waiting = 0;
        // SAFETY: `input` is the handle `enable_raw_mode` opened, which stays open until the
        // caller restores the mode after this function returns, and `waiting` is a local the
        // call writes.
        // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
        if let Err(err) = unsafe { Console::GetNumberOfConsoleInputEvents(input, &mut waiting) } {
            break Err(err.into());
        }
        if waiting == 0 {
            std::thread::sleep(left.min(IDLE));
            continue;
        }
        let mut chunk = [INPUT_RECORD::default(); 32];
        let mut count = 0;
        // SAFETY: `input` is open as above. The call is given the slice `chunk` with its own
        // length, so it writes at most that many records, and `count` is a local it writes.
        // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
        if let Err(err) = unsafe { Console::ReadConsoleInputW(input, &mut chunk, &mut count) } {
            break Err(err.into());
        }
        for record in chunk.iter().take(count as usize) {
            if u32::from(record.EventType) == KEY_EVENT {
                // A record of this type holds a key event, and the character of one that
                // ReadConsoleInputW read is its UTF-16 unit.
                // SAFETY: `KEY_EVENT_RECORD` is integers only, so any bytes are a valid value, and
                // all sixteen bytes of the union are initialised: `chunk` was zeroed and the record
                // is one of the first `count`, which the call wrote.
                // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
                let key = unsafe { record.Event.KeyEvent };
                // SAFETY: both fields of `uChar` are integers over the same initialised bytes, so
                // reading either is valid.
                // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
                let unit = unsafe { key.uChar.UnicodeChar };
                replies.key(key.bKeyDown.as_bool(), key.wVirtualKeyCode, unit);
            } else {
                replies.other();
            }
            records.push(console_record_bytes(record));
        }
    };
    let (responses, kept) = replies.finish();
    for at in kept {
        typed.extend_from_slice(&records[at]);
    }
    read?;
    interpret_parser_responses(responses)
}

/// A console input record as the 20 bytes it occupies in memory: its type, two bytes of padding
/// written as zero, and its event. crossterm's `event::push_input` reads them back on Windows.
#[cfg(windows)]
fn console_record_bytes(record: &windows::Win32::System::Console::INPUT_RECORD) -> [u8; 20] {
    use windows::Win32::System::Console::INPUT_RECORD_0;

    let mut bytes = [0u8; 20];
    bytes[..2].copy_from_slice(&record.EventType.to_le_bytes());
    // The event is sixteen bytes of integers, all of them written: the buffer it was read into was
    // zeroed and a union is copied whole.
    // SAFETY: the sizes match, which `transmute` checks when it compiles, and `[u8; 16]` accepts
    // any initialised bytes. Every byte is initialised: the key and mouse variants each cover all
    // sixteen with no padding, and the record was read into a zeroed buffer.
    // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
    let event = unsafe { std::mem::transmute::<INPUT_RECORD_0, [u8; 16]>(record.Event) };
    bytes[4..].copy_from_slice(&event);
    bytes
}

fn interpret_parser_responses(
    responses: Vec<Response>,
) -> Result<(Option<ProtocolType>, Option<FontSize>, Vec<Capability>)> {
    if responses.is_empty() {
        return Err(Errors::NoCap);
    }

    let mut capabilities = Vec::new();

    let mut proto = None;
    let mut font_size = None;

    let mut cursor_position_reports = vec![];
    for response in &responses {
        if let Some(capability) = match response {
            Response::Kitty => {
                proto = Some(ProtocolType::Kitty);
                Some(Capability::Kitty)
            }
            Response::Sixel => {
                if proto.is_none() {
                    // Only if kitty is not supported.
                    proto = Some(ProtocolType::Sixel);
                }
                Some(Capability::Sixel)
            }
            Response::RectangularOps => Some(Capability::RectangularOps),
            Response::KittyCompression => Some(Capability::KittyCompression),
            Response::CellSize(cell_size) => {
                if let Some((w, h)) = cell_size {
                    font_size = Some((*w, *h).into());
                }
                Some(Capability::CellSize(*cell_size))
            }
            Response::CursorPositionReport(x, y) => {
                cursor_position_reports.push((x, y));
                None
            }
            Response::Background(r, g, b) => Some(Capability::Background(*r, *g, *b)),
            Response::Status => None,
        } {
            capabilities.push(capability);
        }
    }

    // In case some terminal didn't support the cell-size query.
    font_size = font_size.or_else(font_size_fallback);

    if let [(x1, _y1), (x2, _y2), (x3, _y3)] = cursor_position_reports[..] {
        // Test if the cursor advanced exactly two columns (instead of one) on both the width and
        // scaling queries of the protocol.
        // The documentation is a bit ambiguous, as it only says the cursor positions "need to be
        // different from each other".
        // However from my testing on Kitty and other terminals that do not support the feature,
        // the cursor always advances at least one column since it is printing a space, so the CPRs
        // will always be different from each other (unless we would move the cursor to a known
        // position or something like that - and this also begs the question of needing to do this
        // anyway, for the edge case of the cursor being at the very end of a line).
        // My interpretation is that the cursor should advance 2 columns, instead of one, with both
        // queries, and only then can we interpret it as supported.
        // The Foot terminal notably reports a 2 column movement but fortunately only for the `w=2`
        // query.
        //
        // The row part can be ignored.
        if *x2 == x1 + 2 && *x3 == x2 + 2 {
            capabilities.push(Capability::TextSizingProtocol);
        }
    }

    Ok((proto, font_size, capabilities))
}

/// What was read in answer to the query: the replies, and the bytes that were not part of one.
///
/// Public, and hidden from the documentation, only so `crates/tui/src/preview.rs` can test it:
/// this crate declares no test targets and its unit tests do not build in this workspace.
#[doc(hidden)]
#[derive(Default)]
pub struct Replies {
    parser: Parser,
    responses: Vec<Response>,
    /// Every byte taken, in order.
    taken: Vec<u8>,
    /// Where in `taken` the bytes start that were pushed since the parser last held nothing, not
    /// yet known to be a reply or not.
    unclaimed: usize,
    /// The places in `taken` of the bytes that were not part of a reply.
    typed: Vec<usize>,
    /// The Device Status reply, the last question's, has been read.
    done: bool,
}

impl Replies {
    /// Whether the last question's reply has been read.
    pub fn done(&self) -> bool {
        self.done
    }

    pub fn take(&mut self, bytes: &[u8]) {
        for byte in bytes {
            let at = self.taken.len();
            self.taken.push(*byte);
            if self.done {
                self.typed.push(at);
                self.unclaimed = self.taken.len();
                continue;
            }
            let mut more = self.parser.push(char::from(*byte));
            if !more.is_empty() {
                self.unclaimed = self.taken.len();
                if more[..] == [Response::Status] {
                    self.done = true;
                } else {
                    self.responses.append(&mut more);
                }
            } else if self.parser.is_idle() {
                // An escape that ends what came before starts the next sequence, and the parser
                // drops it on the way, so it stays with what follows it.
                let end = self.taken.len() - usize::from(*byte == 0x1b);
                self.typed.extend(self.unclaimed..end);
                self.unclaimed = end;
            }
        }
    }

    pub fn finish(self) -> (Vec<Response>, Vec<u8>) {
        let taken = self.taken.clone();
        let (responses, typed) = self.finish_at();
        (responses, typed.into_iter().map(|at| taken[at]).collect())
    }

    /// The replies, and the places among the bytes taken of those that were not part of one.
    pub fn finish_at(mut self) -> (Vec<Response>, Vec<usize>) {
        self.typed.extend(self.unclaimed..self.taken.len());
        (self.responses, self.typed)
    }
}

/// What the Windows reader read in answer to the query, record by record: [`Replies`] over the
/// characters key events carry, and which records are not part of a reply.
///
/// A console delivers a reply as key events carrying its characters, a press for each and perhaps a
/// release. The character of each press goes to [`Replies`], and a release goes with the press of
/// the same key and character before it, so the release of a reply's character is the reply's too.
/// Any other record is not part of a reply. Platform-independent, and public and hidden from the
/// documentation for the same reason as [`Replies`].
#[doc(hidden)]
#[derive(Default)]
pub struct ConsoleReplies {
    replies: Replies,
    /// For each record taken, the press it is or is the release of, by its place in `presses`.
    records: Vec<Option<usize>>,
    /// For each character given to `replies`: the virtual key and the unit of its press, and
    /// whether its release has been taken.
    presses: Vec<(u16, u16, bool)>,
}

impl ConsoleReplies {
    /// Whether the last question's reply has been read.
    pub fn done(&self) -> bool {
        self.replies.done()
    }

    /// A key event: pressed or released, its virtual key, and the UTF-16 unit it carries, or zero
    /// where it carries none.
    pub fn key(&mut self, down: bool, virtual_key: u16, unit: u16) {
        if unit == 0 {
            self.records.push(None);
        } else if down {
            self.records.push(Some(self.presses.len()));
            self.presses.push((virtual_key, unit, false));
            // No reply holds a character outside ASCII. NUL stands for one, being in no reply
            // either, where a printable stand-in could complete one.
            let byte = u8::try_from(unit).ok().filter(u8::is_ascii).unwrap_or(0);
            self.replies.take(&[byte]);
        } else {
            let press = self.presses.iter().rposition(|&(key, carried, released)| {
                !released && key == virtual_key && carried == unit
            });
            if let Some(press) = press {
                self.presses[press].2 = true;
            }
            self.records.push(press);
        }
    }

    /// A record that is not a key event.
    pub fn other(&mut self) {
        self.records.push(None);
    }

    /// The replies, and the places among the records taken of those that are not part of one, in
    /// the order taken.
    pub fn finish(self) -> (Vec<Response>, Vec<usize>) {
        let (responses, typed) = self.replies.finish_at();
        let mut in_reply = vec![true; self.presses.len()];
        for press in typed {
            in_reply[press] = false;
        }
        let kept = (0..self.records.len())
            .filter(|&at| !matches!(self.records[at], Some(press) if in_reply[press]))
            .collect();
        (responses, kept)
    }
}

/// Query the terminal on this thread, giving up at the timeout.
#[cfg(not(windows))]
fn query_stdio_capabilities(
    is_tmux: bool,
    options: QueryStdioOptions,
    typed: &mut Vec<u8>,
) -> Result<(Option<ProtocolType>, Option<FontSize>, Vec<Capability>)> {
    use std::time::Instant;

    let timeout = options.timeout;
    let query = Parser::query(is_tmux, options);
    io::stdout().write_all(query.as_bytes())?;
    io::stdout().flush()?;

    let stdin = io::stdin();
    let mut replies = Replies::default();
    // From the question rather than from the last byte read, so a person typing while it waits
    // does not keep it waiting.
    let asked = Instant::now();
    let read = loop {
        if replies.done {
            break Ok(());
        }
        if asked.elapsed() >= timeout {
            break Err(Errors::NoStdinResponse);
        }
        let mut chunk = [0u8; 50];
        // The descriptor rather than `Stdin`, whose buffer would keep what it read past the reply
        // where no later reader of the terminal looks. Returns with nothing after a tenth of a
        // second, as `enable_raw_mode` set it to.
        match rustix::io::read(&stdin, &mut chunk) {
            Ok(0) | Err(rustix::io::Errno::INTR) => {}
            Ok(n) => replies.take(&chunk[..n]),
            Err(errno) => break Err(errno.into()),
        }
    };
    let (responses, mut rest) = replies.finish();
    typed.append(&mut rest);
    read?;
    interpret_parser_responses(responses)
}

#[cfg(not(windows))]
fn query_with_timeout(
    is_tmux: bool,
    options: QueryStdioOptions,
    typed: &mut Vec<u8>,
) -> Result<(Option<ProtocolType>, Option<FontSize>, Vec<Capability>)> {
    let disable_raw_mode = enable_raw_mode()?;
    let result = query_stdio_capabilities(is_tmux, options, typed);
    disable_raw_mode()?;
    result
}

#[cfg(windows)]
fn query_with_timeout(
    is_tmux: bool,
    options: QueryStdioOptions,
    typed: &mut Vec<u8>,
) -> Result<(Option<ProtocolType>, Option<FontSize>, Vec<Capability>)> {
    let (input, disable_raw_mode) = enable_raw_mode()?;
    let result = query_stdio_capabilities(is_tmux, options, input, typed);
    disable_raw_mode()?;
    result
}

#[cfg(test)]
mod tests {
    use std::assert_eq;

    use crate::{
        FontSize,
        picker::{Capability, Picker, ProtocolType},
    };

    use super::{cap_parser::Response, fallback_picker, interpret_parser_responses};

    #[test]
    fn test_cycle_protocol() {
        let mut proto = ProtocolType::Halfblocks;
        proto = proto.next();
        assert_eq!(proto, ProtocolType::Sixel);
        proto = proto.next();
        assert_eq!(proto, ProtocolType::Kitty);
        proto = proto.next();
        assert_eq!(proto, ProtocolType::Iterm2);
        proto = proto.next();
        assert_eq!(proto, ProtocolType::Halfblocks);
    }

    #[test]
    fn test_tmux_detected_reflects_is_tmux() {
        let mut picker = Picker {
            font_size: FontSize::new(10, 20),
            protocol_type: ProtocolType::Kitty,
            background_color: None,
            is_tmux: true,
            capabilities: Vec::new(),
        };
        assert!(picker.tmux_detected());

        picker.is_tmux = false;
        assert!(!picker.tmux_detected());
    }

    #[test]
    fn test_from_query_stdio_no_hang() {
        let _ = Picker::from_query_stdio();
    }

    #[test]
    fn test_fallback_picker() {
        // The terminal did not answer, but the font-size is known from the ioctl fallback and
        // some env var hinted at iTerm2 support: use it instead of halfblocks.
        let picker = fallback_picker(
            false,
            Some(ProtocolType::Iterm2),
            Some(FontSize::new(8, 16)),
        );
        assert_eq!(picker.protocol_type(), ProtocolType::Iterm2);
        assert_eq!(
            (picker.font_size().width, picker.font_size().height),
            (8, 16)
        );

        // Without a font-size, no other protocol can be rendered meaningfully.
        let picker = fallback_picker(false, Some(ProtocolType::Iterm2), None);
        assert_eq!(picker.protocol_type(), ProtocolType::Halfblocks);

        // Without a hint, stay on halfblocks.
        let picker = fallback_picker(false, None, Some(FontSize::new(8, 16)));
        assert_eq!(picker.protocol_type(), ProtocolType::Halfblocks);
    }

    #[test]
    fn test_interpret_parser_responses_text_sizing_protocol() {
        let (_, _, caps) = interpret_parser_responses(vec![
            // Example response from Kitty.
            Response::CursorPositionReport(1, 1),
            Response::CursorPositionReport(3, 1),
            Response::CursorPositionReport(5, 1),
        ])
        .unwrap();
        assert!(caps.contains(&Capability::TextSizingProtocol));
    }

    #[test]
    fn test_interpret_parser_responses_text_sizing_protocol_incomplete() {
        let (_, _, caps) = interpret_parser_responses(vec![
            // Example response from Foot, notably moves 2 columns only on `w=2` query, but not
            // `s=2`.
            Response::CursorPositionReport(1, 22),
            Response::CursorPositionReport(3, 22),
            Response::CursorPositionReport(4, 22),
        ])
        .unwrap();
        assert!(!caps.contains(&Capability::TextSizingProtocol));
    }
}
