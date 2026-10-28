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
//! What a read finds, in this order: the files a file manager copied, which are only named here and
//! are read by the drop handling; a picture, in the first of its flavours the clipboard holds; and
//! text.
//!
//! Nothing labelled passes through here. What is copied was read off the screen, or is a reply's
//! text as the transcript holds it for `/copy`, and both were released for display before they were
//! drawn. What is pasted is the user's own input, on the footing of the prompt it lands in, which
//! [`bravebot_core::policy::Policy::admit_pasted_image`] states in full.

use bravebot_agent::picture::downscaled;
use bravebot_agent::turn::MAX_PASTED_IMAGE_BYTES;
use std::ffi::OsStr;
use std::io::Write;
use std::path::PathBuf;
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

/// The command that starts a clipboard tool found as `program`.
///
/// A tool that is a script or a launcher starts programs by name too, so it is given only the
/// `PATH` entries the lookup searched (SANDBOX-30).
fn start_tool(program: PathBuf) -> Command {
    bravebot_sandbox::programs::start(
        &bravebot_sandbox::programs::Resolved::found_earlier(program),
        bravebot_sandbox::programs::Inherits::All,
        bravebot_sandbox::programs::SearchPath::Reduced,
    )
}

/// Run a tool and write the text to it, reporting whether it took it.
fn pipe_into(program: &str, arguments: &[&str], text: &str) -> bool {
    pipe_into_found_by(bravebot_sandbox::programs::find, program, arguments, text)
}

/// [`pipe_into`] with the lookup that turns the tool's name into the program started.
fn pipe_into_found_by(
    find: impl Fn(&OsStr) -> Option<PathBuf>,
    program: &str,
    arguments: &[&str],
    text: &str,
) -> bool {
    let Some(program) = find(OsStr::new(program)) else {
        return false;
    };
    let Ok(mut child) = start_tool(program)
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

/// What the clipboard had when it was asked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pasted {
    Text(String),
    Image(Image),
    /// Files a file manager copied, as the absolute paths the clipboard names.
    ///
    /// Only the paths: what is at each is read by the drop handling, the one gate they pass
    /// (PASTE-12).
    Files(Vec<String>),
    /// A list of copied files with one whose name cannot be carried as a path: it is not text, or it
    /// holds a newline. None of the files are taken (PASTE-12).
    FilesRefused,
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
///
/// Files a file manager copied win over both, since copying a file leaves its name behind as text
/// the way copying an image in a browser leaves its page. They are only named here: the drop
/// handling reads them, which is what treating a path on the clipboard as a drop means (PASTE-12).
pub fn paste() -> Pasted {
    files_or(files_on_clipboard(), || {
        chosen(image_on_clipboard(), text_on_clipboard)
    })
}

/// The files when there are any, a refusal of them when one cannot be carried, and otherwise what
/// `rest` reads.
///
/// Apart from the reads for the reason [`chosen`] is: the precedence is the decision, and it is
/// here. `rest` is a closure so that a clipboard naming files costs no read of a picture or of text,
/// and a refused list is not swapped for the text beside it.
fn files_or(files: Result<Vec<String>, Refused>, rest: impl FnOnce() -> Pasted) -> Pasted {
    match files {
        Err(Refused) => Pasted::FilesRefused,
        Ok(files) if files.is_empty() => rest(),
        Ok(files) => Pasted::Files(files),
    }
}

/// A list of copied files with a name that cannot be carried as a path.
#[derive(Debug, PartialEq, Eq)]
struct Refused;

/// The paths a clipboard's file list names that a drop could take, or a refusal of the list.
///
/// A path is kept when it is absolute and names a regular file. A relative path would be read
/// against whichever directory the process is in, which is not what the copy named, and a directory
/// or a file that is gone names nothing a drop takes ([`crate::dropped::dropped_with`] treats them
/// the same).
///
/// The list is refused, and not read without the file, when a name cannot be carried as one line of
/// text, since the files taken would not be the files copied. A name holding U+FFFD is what a name
/// that is not text arrives as, and the file under that spelling is another one or none, so it is
/// refused before any file is looked up. A kept name holding a newline or a carriage return would
/// end the line a drop is read from and begin another, and a file named `a<newline>/etc/hosts`
/// would be taken for two. `exists` answers whether a path names a file, as it does there.
fn usable_files(paths: Vec<String>, exists: impl Fn(&str) -> bool) -> Result<Vec<String>, Refused> {
    if paths.iter().any(|path| path.contains('\u{FFFD}')) {
        return Err(Refused);
    }
    let kept: Vec<String> = paths
        .into_iter()
        .filter(|path| std::path::Path::new(path).is_absolute() && exists(path))
        .collect();
    if kept.iter().any(|path| path.contains(['\n', '\r'])) {
        return Err(Refused);
    }
    Ok(kept)
}

fn files_on_clipboard() -> Result<Vec<String>, Refused> {
    platform::files()
        .and_then(|paths| usable_files(paths, |path| std::path::Path::new(path).is_file()))
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

/// A picture the clipboard can hold, and what a picture read in that flavour is sent as.
#[cfg(not(target_os = "windows"))]
struct Flavour {
    /// What the platform calls the flavour: a pasteboard type on macOS, a media type on Linux.
    offered: &'static str,
    /// The media type the picture is sent as. One of this module's own literals and never a string
    /// read from the clipboard, for the reason [`Image::media_type`] gives.
    media_type: &'static str,
}

/// The first picture in the first of `order` that `read` finds bytes for.
///
/// The order is the decision, so it is a table read here rather than a sequence of reads written
/// out where each platform makes them. A flavour that comes back empty is the same as one the
/// clipboard does not hold, and the flavours after the first that answers are never read.
#[cfg(not(target_os = "windows"))]
fn first_picture(
    order: &[Flavour],
    mut read: impl FnMut(&Flavour) -> Option<Vec<u8>>,
) -> Option<(&'static str, Vec<u8>)> {
    order.iter().find_map(|flavour| {
        read(flavour)
            .filter(|bytes| !bytes.is_empty())
            .map(|bytes| (flavour.media_type, bytes))
    })
}

/// The paths in a JSON array of strings, which is how a script reports a clipboard's file list: a
/// name holding a newline cannot be told from two names in a list of lines, and in JSON it is
/// escaped. Anything that is not such an array names no files.
#[cfg(any(target_os = "macos", target_os = "windows"))]
fn paths_from_json(text: &str) -> Vec<String> {
    serde_json::from_str(text.trim()).unwrap_or_default()
}

#[cfg(target_os = "macos")]
mod platform {
    use super::{Flavour, Refused, first_picture, run_text};

    pub const PNG: &str = "public.png";
    pub const JPEG: &str = "public.jpeg";
    pub const TIFF: &str = "public.tiff";

    /// The pasteboard a script reads when it is given no name: the general one, which is what the
    /// person copies to. Only a test names another, so that what it puts on a pasteboard of its own
    /// is never the person's clipboard.
    const CLIPBOARD: &str = "";

    /// The pasteboard types a picture can be on, in the order they are tried.
    ///
    /// The system offers TIFF for any picture on the pasteboard whatever it was copied as, so it is
    /// the last resort that finds a GIF or a HEIC. Nothing in this crate reads TIFF, so AppKit
    /// renders it as PNG and the media type is PNG's.
    pub const FLAVOURS: &[Flavour] = &[
        Flavour {
            offered: PNG,
            media_type: "image/png",
        },
        Flavour {
            offered: JPEG,
            media_type: "image/jpeg",
        },
        Flavour {
            offered: TIFF,
            media_type: "image/png",
        },
    ];

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
    ///
    /// The first argument names the pasteboard, empty for the general one, and the second is the
    /// pasteboard type, always one of [`FLAVOURS`].
    const READ: &str = "function run(argv) { \
                        ObjC.import('AppKit'); \
                        const d = $.NSPasteboard.pasteboardWithName(\
                        argv[0] || $.NSPasteboardNameGeneral).dataForType(argv[1]); \
                        return d.isNil() ? '' : $.NSString.alloc.initWithDataEncoding(\
                        d.base64EncodedDataWithOptions(0), $.NSUTF8StringEncoding).js; }";

    /// [`READ`] for a type nothing here decodes: the same bytes, drawn by AppKit and written as PNG.
    ///
    /// The third and fourth arguments are the most pixels a side may have and the most memory the
    /// bitmap may take, the limits [`bravebot_agent::picture::downscaled`] holds a decode to. A picture that declares
    /// more is not drawn: the canvas is read from the header before anything is decoded, since
    /// this process decodes and re-encodes whatever the clipboard holds and a few megabytes of
    /// compressed TIFF can declare a canvas that takes gigabytes.
    const READ_AS_PNG: &str = "function run(argv) { \
                               ObjC.import('AppKit'); \
                               const d = $.NSPasteboard.pasteboardWithName(\
                               argv[0] || $.NSPasteboardNameGeneral).dataForType(argv[1]); \
                               if (d.isNil()) return ''; \
                               const drawn = $.NSBitmapImageRep.imageRepWithData(d); \
                               if (drawn.isNil()) return ''; \
                               const wide = drawn.pixelsWide; \
                               const high = drawn.pixelsHigh; \
                               if (wide > Number(argv[2]) || high > Number(argv[2]) \
                               || wide * high * 4 > Number(argv[3])) return ''; \
                               const png = drawn.representationUsingTypeProperties(\
                               $.NSBitmapImageFileTypePNG, $()); \
                               return png.isNil() ? '' : $.NSString.alloc.initWithDataEncoding(\
                               png.base64EncodedDataWithOptions(0), $.NSUTF8StringEncoding).js; }";

    pub fn image() -> Option<(&'static str, Vec<u8>)> {
        image_from(CLIPBOARD)
    }

    /// [`image`] for the pasteboard called `pasteboard`.
    pub(super) fn image_from(pasteboard: &str) -> Option<(&'static str, Vec<u8>)> {
        use base64::Engine;
        let side = bravebot_agent::picture::MAX_DECODED_SIDE.to_string();
        let bytes = bravebot_agent::picture::MAX_DECODE_ALLOC_BYTES.to_string();
        first_picture(FLAVOURS, |flavour| {
            let mut arguments = vec!["-l", "JavaScript", "-e"];
            if flavour.offered == TIFF {
                arguments.extend([READ_AS_PNG, pasteboard, flavour.offered, &side, &bytes]);
            } else {
                arguments.extend([READ, pasteboard, flavour.offered]);
            }
            let encoded = run_text("osascript", &arguments)?;
            // Half a picture is worse than none: it would be sent, rejected by the endpoint, and
            // reported as a fault of the request rather than of the read that truncated it.
            base64::engine::general_purpose::STANDARD
                .decode(encoded.trim())
                .ok()
        })
    }

    /// Asked about the flavours that matter, and only about whether one is on offer.
    ///
    /// Reading the list of types the pasteboard advertises never materialises what is behind one,
    /// which is what makes this affordable on a focus change. The older way of asking, through
    /// AppleScript's `clipboard info`, priced a yes-or-no question at the size of the picture.
    const PROBE: &str = "function run(argv) { \
                         ObjC.import('AppKit'); \
                         const types = $.NSPasteboard.pasteboardWithName(\
                         argv[0] || $.NSPasteboardNameGeneral).types; \
                         return argv.slice(1).some(type => types.containsObject(type)) \
                         ? 'yes' : 'no'; }";

    pub fn has_image() -> bool {
        has_image_in(CLIPBOARD)
    }

    /// [`has_image`] for the pasteboard called `pasteboard`.
    pub(super) fn has_image_in(pasteboard: &str) -> bool {
        let mut arguments = vec!["-l", "JavaScript", "-e", PROBE, pasteboard];
        arguments.extend(FLAVOURS.iter().map(|flavour| flavour.offered));
        run_text("osascript", &arguments).is_some_and(|offered| offered.trim() == "yes")
    }

    /// The files a copy in Finder leaves on the pasteboard, as a JSON array of their paths.
    ///
    /// Each item holding a file URL is asked for its path, so a reference URL of the form
    /// `file:///.file/id=...` resolves to the file it stands for. The list is JSON for the reason
    /// [`super::paths_from_json`] gives. The argument names the pasteboard, as for [`READ`].
    const FILES: &str = "function run(argv) { \
                         ObjC.import('AppKit'); \
                         const items = $.NSPasteboard.pasteboardWithName(\
                         argv[0] || $.NSPasteboardNameGeneral).pasteboardItems; \
                         const paths = []; \
                         for (let i = 0; i < items.count; i++) { \
                         const text = items.objectAtIndex(i).stringForType('public.file-url'); \
                         if (text.isNil()) continue; \
                         const url = $.NSURL.URLWithString(text); \
                         if (url.isNil()) continue; \
                         const located = url.filePathURL; \
                         if (located.isNil()) continue; \
                         paths.push(ObjC.unwrap(located.path)); } \
                         return JSON.stringify(paths); }";

    pub fn files() -> Result<Vec<String>, Refused> {
        Ok(files_from(CLIPBOARD))
    }

    /// [`files`] for the pasteboard called `pasteboard`. The pasteboard's names are text.
    pub(super) fn files_from(pasteboard: &str) -> Vec<String> {
        run_text("osascript", &["-l", "JavaScript", "-e", FILES, pasteboard])
            .map(|json| super::paths_from_json(&json))
            .unwrap_or_default()
    }

    pub const TEXT_TOOLS: &[(&str, &[&str])] = &[("pbpaste", &[])];
}

#[cfg(target_os = "windows")]
mod platform {
    use super::{Refused, run_text};

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

    /// The files a copy in Explorer leaves on the clipboard, as a JSON array of their paths.
    ///
    /// Written as base64 of the UTF-8 for the reason the picture is: what PowerShell prints goes
    /// through the console's encoding, which turns a name outside it into another one.
    const FILES: &str = "Add-Type -AssemblyName System.Windows.Forms; \
                         $files = @([Windows.Forms.Clipboard]::GetFileDropList()); \
                         if ($files.Count) { [Convert]::ToBase64String(\
                         [Text.Encoding]::UTF8.GetBytes(\
                         (ConvertTo-Json -Compress -InputObject $files))) }";

    pub fn files() -> Result<Vec<String>, Refused> {
        use base64::Engine;
        let Some(encoded) = run_text("powershell", &["-NoProfile", "-Command", FILES]) else {
            return Ok(Vec::new());
        };
        let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(encoded.trim()) else {
            return Ok(Vec::new());
        };
        Ok(String::from_utf8(bytes)
            .map(|json| super::paths_from_json(&json))
            .unwrap_or_default())
    }

    pub const TEXT_TOOLS: &[(&str, &[&str])] =
        &[("powershell", &["-NoProfile", "-Command", "Get-Clipboard"])];
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod platform {
    use super::{Flavour, Refused, first_picture, run_bytes_found_by};
    use std::ffi::OsStr;
    use std::path::PathBuf;

    pub const PNG: &str = "image/png";
    pub const JPEG: &str = "image/jpeg";

    /// The media types a picture can be on, in the order they are tried. Nothing here converts
    /// between formats, so each is sent as it is.
    pub const FLAVOURS: &[Flavour] = &[
        Flavour {
            offered: PNG,
            media_type: PNG,
        },
        Flavour {
            offered: JPEG,
            media_type: JPEG,
        },
    ];

    const FLAVOUR_TOOLS: &[(&str, &[&str])] = &[
        ("wl-paste", &["--list-types"]),
        ("xclip", &["-selection", "clipboard", "-t", "TARGETS", "-o"]),
    ];

    /// The types a file manager leaves the files it copied on, tried in this order: the standard
    /// `text/uri-list`, and the one GNOME's file managers leave, whose payload is a `copy` or `cut`
    /// line and then one URI per line.
    const FILE_TYPES: &[&str] = &["text/uri-list", "x-special/gnome-copied-files"];

    pub fn image() -> Option<(&'static str, Vec<u8>)> {
        image_found_by(bravebot_sandbox::programs::find)
    }

    /// What either display server's tool prints for the type `offered`, when it prints anything.
    ///
    /// Both are asked, since which server is running is not something this can assume and a tool
    /// for the other simply fails to spawn.
    fn read_type(find: &impl Fn(&OsStr) -> Option<PathBuf>, offered: &str) -> Option<Vec<u8>> {
        let wl_paste = ["--no-newline", "--type", offered];
        let xclip = ["-selection", "clipboard", "-t", offered, "-o"];
        [("wl-paste", &wl_paste[..]), ("xclip", &xclip[..])]
            .into_iter()
            .find_map(|(program, arguments)| {
                run_bytes_found_by(find, program, arguments).filter(|bytes| !bytes.is_empty())
            })
    }

    /// [`image`] with the lookup that turns a tool's name into the program started.
    pub(super) fn image_found_by(
        find: impl Fn(&OsStr) -> Option<PathBuf>,
    ) -> Option<(&'static str, Vec<u8>)> {
        first_picture(FLAVOURS, |flavour| read_type(&find, flavour.offered))
    }

    pub fn has_image() -> bool {
        has_image_found_by(bravebot_sandbox::programs::find)
    }

    /// [`has_image`] with the lookup that turns a tool's name into the program started.
    pub(super) fn has_image_found_by(find: impl Fn(&OsStr) -> Option<PathBuf>) -> bool {
        FLAVOUR_TOOLS.iter().any(|(program, arguments)| {
            run_bytes_found_by(&find, program, arguments)
                .and_then(|types| String::from_utf8(types).ok())
                .is_some_and(|types| {
                    FLAVOURS
                        .iter()
                        .any(|flavour| types.contains(flavour.offered))
                })
        })
    }

    pub fn files() -> Result<Vec<String>, Refused> {
        files_found_by(bravebot_sandbox::programs::find)
    }

    /// [`files`] with the lookup that turns a tool's name into the program started.
    ///
    /// A clipboard that does not hold a type makes either tool fail, which reads as no files. A list
    /// that is not text, or names a file whose name is not, is refused as a whole.
    pub(super) fn files_found_by(
        find: impl Fn(&OsStr) -> Option<PathBuf>,
    ) -> Result<Vec<String>, Refused> {
        for offered in FILE_TYPES {
            let Some(bytes) = read_type(&find, offered) else {
                continue;
            };
            let list = String::from_utf8(bytes).map_err(|_| Refused)?;
            let paths = crate::dropped::uri_list(&list).ok_or(Refused)?;
            if !paths.is_empty() {
                return Ok(paths);
            }
        }
        Ok(Vec::new())
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
    run_bytes_found_by(bravebot_sandbox::programs::find, program, arguments)
}

/// [`run_bytes`] with the lookup that turns the tool's name into the program started.
fn run_bytes_found_by(
    find: impl Fn(&OsStr) -> Option<PathBuf>,
    program: &str,
    arguments: &[&str],
) -> Option<Vec<u8>> {
    let program = find(OsStr::new(program))?;
    let output = start_tool(program)
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
    use bravebot_agent::picture::{DOWNSCALED_SIDE, MAX_DECODED_SIDE};

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

    #[cfg(unix)]
    fn script(directory: &std::path::Path, name: &str, body: &str) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::create_dir_all(directory).unwrap();
        let file = directory.join(name);
        std::fs::write(&file, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    /// Makes each path read-only, and writable again when dropped, so a failed assertion leaves a
    /// scratch directory that can still be removed.
    #[cfg(unix)]
    struct ReadOnly(Vec<PathBuf>);

    #[cfg(unix)]
    impl ReadOnly {
        fn new(paths: &[PathBuf]) -> Self {
            for path in paths {
                set_mode(path, 0o555);
            }
            Self(paths.to_vec())
        }
    }

    #[cfg(unix)]
    impl Drop for ReadOnly {
        fn drop(&mut self) {
            for path in &self.0 {
                set_mode(path, 0o755);
            }
        }
    }

    #[cfg(unix)]
    fn set_mode(path: &std::path::Path, mode: u32) {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode));
    }

    /// Makes `directory` and every directory under it writable again, so a run that was killed
    /// while a tree was read-only leaves none the next run cannot remove.
    #[cfg(unix)]
    fn reopen(directory: &std::path::Path) {
        set_mode(directory, 0o755);
        for entry in std::fs::read_dir(directory).into_iter().flatten().flatten() {
            if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                reopen(&entry.path());
            }
        }
    }

    /// A clipboard tool is started with the session's access to the person's clipboard and
    /// nothing confining it, so a file a confined stage wrote under the tool's name must never be
    /// the one that runs. The system's own `true` would also succeed, so a lookup that ignored the
    /// kept-out directory, or that fell back to the bare name, copies and fails this.
    #[cfg(unix)]
    #[test]
    fn a_copy_tool_found_only_in_a_directory_a_stage_may_write_is_not_started() {
        use bravebot_sandbox::programs::find_in;
        let session = crate::testutil::scratch_dir("clipboard-copy-kept-out");
        let _ = std::fs::remove_dir_all(&session);
        let marker = session.join("started");
        script(
            &session.join("bin"),
            "true",
            &format!("touch '{}'", marker.display()),
        );
        let path = std::env::join_paths([session.join("bin")]).unwrap();
        let writable = [session.clone()];

        let copied = pipe_into_found_by(|p| find_in(p, &path, &writable), "true", &[], "hello");

        let started = marker.exists();
        let _ = std::fs::remove_dir_all(&session);
        assert!(!started, "the planted tool was started");
        assert!(!copied);
    }

    /// The same lookup finds a tool installed outside every directory a stage may write, so the
    /// refusal above is the directory's and not a lookup that finds nothing.
    #[cfg(unix)]
    #[test]
    fn a_copy_tool_installed_outside_every_writable_directory_is_started() {
        use bravebot_sandbox::programs::find_in;
        let session = crate::testutil::scratch_dir("clipboard-copy-installed-session");
        let system = crate::testutil::scratch_dir("clipboard-copy-installed-system");
        let _ = std::fs::remove_dir_all(&session);
        let _ = std::fs::remove_dir_all(&system);
        // The shell's own `read` takes the text, so the tool looks no program up on this process's
        // `PATH`, which the start now reduces by the session's real kept-out directories.
        script(
            &system,
            "bravebot-test-copy-tool",
            "while read -r _; do :; done",
        );
        let path = std::env::join_paths([&system]).unwrap();
        let writable = [session.clone()];

        let copied = pipe_into_found_by(
            |p| find_in(p, &path, &writable),
            "bravebot-test-copy-tool",
            &[],
            "hello",
        );

        let _ = std::fs::remove_dir_all(&system);
        assert!(copied);
    }

    /// Reading the clipboard starts a tool the same way copying does. The system's own `uname`
    /// would print something, so a lookup that ignored the kept-out directory, or that fell back
    /// to the bare name, returns bytes and fails this.
    #[cfg(unix)]
    #[test]
    fn a_paste_tool_found_only_in_a_directory_a_stage_may_write_is_not_started() {
        use bravebot_sandbox::programs::find_in;
        let session = crate::testutil::scratch_dir("clipboard-paste-kept-out");
        let _ = std::fs::remove_dir_all(&session);
        script(&session.join("bin"), "uname", "echo planted");
        let path = std::env::join_paths([session.join("bin")]).unwrap();
        let writable = [session.clone()];

        let read = run_bytes_found_by(|p| find_in(p, &path, &writable), "uname", &[]);

        let _ = std::fs::remove_dir_all(&session);
        assert_eq!(read, None);
    }

    /// The control for the refusal above: a tool outside every writable directory is started and
    /// what it printed is read.
    #[cfg(unix)]
    #[test]
    fn a_paste_tool_installed_outside_every_writable_directory_is_read() {
        use bravebot_sandbox::programs::find_in;
        let session = crate::testutil::scratch_dir("clipboard-paste-installed-session");
        let system = crate::testutil::scratch_dir("clipboard-paste-installed-system");
        let _ = std::fs::remove_dir_all(&session);
        let _ = std::fs::remove_dir_all(&system);
        script(&system, "bravebot-test-paste-tool", "echo installed");
        let path = std::env::join_paths([&system]).unwrap();
        let writable = [session.clone()];

        let read = run_bytes_found_by(
            |p| find_in(p, &path, &writable),
            "bravebot-test-paste-tool",
            &[],
        );

        let _ = std::fs::remove_dir_all(&system);
        assert_eq!(read, Some(b"installed\n".to_vec()));
    }

    /// A clipboard tool that a stage given the whole machine wrote, then made read-only with the
    /// directories above it, is still one the account owns, and the account can set its mode back.
    /// The lookup leaves it out as it does a writable one, so the copy never starts it. The system's
    /// own `true` would also succeed, so a lookup that fell back to the bare name copies and fails
    /// this.
    #[cfg(unix)]
    #[test]
    fn a_copy_tool_the_account_made_read_only_in_a_held_row_is_not_started() {
        use bravebot_sandbox::programs::find_in_with;
        let row = crate::testutil::scratch_dir("clipboard-copy-read-only");
        reopen(&row);
        let _ = std::fs::remove_dir_all(&row);
        let bin = row.join("held").join("bin");
        let marker = row.join("started");
        script(&bin, "true", &format!("touch '{}'", marker.display()));
        let path = std::env::join_paths([&bin]).unwrap();
        let restore = ReadOnly::new(&[bin.join("true"), bin.clone(), row.join("held")]);

        let copied = pipe_into_found_by(
            |p| find_in_with(p, &path, &[], std::slice::from_ref(&row)),
            "true",
            &[],
            "hello",
        );

        let started = marker.exists();
        drop(restore);
        let _ = std::fs::remove_dir_all(&row);
        assert!(!started, "the read-only tool was started");
        assert!(!copied);
    }

    /// The same for reading the clipboard, which starts its tool through [`run_bytes_found_by`].
    #[cfg(unix)]
    #[test]
    fn a_paste_tool_the_account_made_read_only_in_a_held_row_is_not_started() {
        use bravebot_sandbox::programs::find_in_with;
        let row = crate::testutil::scratch_dir("clipboard-paste-read-only");
        reopen(&row);
        let _ = std::fs::remove_dir_all(&row);
        let bin = row.join("held").join("bin");
        script(&bin, "uname", "echo planted");
        let path = std::env::join_paths([&bin]).unwrap();
        let restore = ReadOnly::new(&[bin.join("uname"), bin.clone(), row.join("held")]);

        let read = run_bytes_found_by(
            |p| find_in_with(p, &path, &[], std::slice::from_ref(&row)),
            "uname",
            &[],
        );

        drop(restore);
        let _ = std::fs::remove_dir_all(&row);
        assert_eq!(read, None);
    }

    /// `copy` and every read of the clipboard reach a tool through [`pipe_into`] and
    /// [`run_bytes`], so those are what must resolve a tool's name through the session's lookup.
    /// The tests above hand the lookup in and so cannot see a caller that stops using it. This one
    /// runs the test binary again with `PATH` holding only a directory the session keeps out, a
    /// `true` and a `uname` planted in it, and calls both entry points with their own lookup.
    /// A caller that starts the bare name instead finds the planted files and fails the child.
    #[cfg(unix)]
    #[test]
    fn the_clipboard_entry_points_start_no_tool_found_only_in_a_directory_a_stage_may_write() {
        const TEST: &str = "clipboard::tests::the_clipboard_entry_points_start_no_tool_found_only_in_a_directory_a_stage_may_write";
        const SESSION: &str = "BRAVEBOT_TEST_CLIPBOARD_KEPT_OUT_SESSION";

        if let Some(session) = std::env::var_os(SESSION) {
            let session = PathBuf::from(session);
            bravebot_sandbox::programs::keep_out(&session);
            std::fs::write(session.join("ran"), "").unwrap();

            let copied = pipe_into("true", &[], "hello");
            let read = run_bytes("uname", &[]);

            assert!(
                !session.join("started").exists(),
                "pipe_into started the planted tool"
            );
            assert!(!copied);
            assert_eq!(read, None, "run_bytes started the planted tool");
            return;
        }

        let session = crate::testutil::scratch_dir("clipboard-entry-points-kept-out");
        let _ = std::fs::remove_dir_all(&session);
        let bin = session.join("bin");
        script(
            &bin,
            "true",
            &format!(": > '{}'", session.join("started").display()),
        );
        script(&bin, "uname", "echo planted");

        // nosemgrep: rust.lang.security.current-exe.current-exe
        let child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", TEST, "--test-threads=1", "--nocapture"])
            .env(SESSION, &session)
            .env("PATH", &bin)
            .output()
            .unwrap();

        let ran = session.join("ran").exists();
        let report = String::from_utf8_lossy(&child.stderr).into_owned();
        let _ = std::fs::remove_dir_all(&session);
        assert!(ran, "the child did not run the test: {report}");
        assert!(child.status.success(), "{report}");
    }

    /// Ctrl-V and the hint that it would paste a picture start every tool they read the clipboard
    /// with through the session's lookup, on whichever platform this runs. The tests that hand a
    /// lookup in cannot see a reader that stops using it. This one runs the test binary again with
    /// `PATH` holding only a directory the session keeps out, a tool of every name a reader starts
    /// planted in it, and calls both entry points. A reader that starts a bare name finds a planted
    /// file and fails the child.
    #[cfg(unix)]
    #[test]
    fn the_reads_of_the_clipboard_start_no_tool_found_only_in_a_directory_a_stage_may_write() {
        const TEST: &str = "clipboard::tests::the_reads_of_the_clipboard_start_no_tool_found_only_in_a_directory_a_stage_may_write";
        const SESSION: &str = "BRAVEBOT_TEST_CLIPBOARD_READS_KEPT_OUT_SESSION";

        if let Some(session) = std::env::var_os(SESSION) {
            let session = PathBuf::from(session);
            bravebot_sandbox::programs::keep_out(&session);
            std::fs::write(session.join("ran"), "").unwrap();

            let pasted = paste();
            let hint = holds_an_image();

            assert!(
                !session.join("started").exists(),
                "a read of the clipboard started the planted tool"
            );
            assert_eq!(pasted, Pasted::Nothing);
            assert!(!hint);
            return;
        }

        let session = crate::testutil::scratch_dir("clipboard-reads-kept-out");
        let _ = std::fs::remove_dir_all(&session);
        let bin = session.join("bin");
        for tool in [
            "osascript",
            "pbpaste",
            "wl-paste",
            "xclip",
            "xsel",
            "powershell",
        ] {
            script(
                &bin,
                tool,
                &format!(": > '{}'", session.join("started").display()),
            );
        }

        // nosemgrep: rust.lang.security.current-exe.current-exe
        let child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", TEST, "--test-threads=1", "--nocapture"])
            .env(SESSION, &session)
            .env("PATH", &bin)
            .output()
            .unwrap();

        let ran = session.join("ran").exists();
        let report = String::from_utf8_lossy(&child.stderr).into_owned();
        let _ = std::fs::remove_dir_all(&session);
        assert!(ran, "the child did not run the test: {report}");
        assert!(child.status.success(), "{report}");
    }

    #[cfg(unix)]
    const INTERPRETER_SESSION: &str = "BRAVEBOT_TEST_CLIPBOARD_INTERPRETER_SESSION";

    /// Run `test` again in a child whose `PATH` is a directory the session keeps out, then an
    /// installed one, then the system's. `bravebot-test-tool` is installed and is a script with an
    /// `env` shebang naming `bravebot-test-interp`, which is how a shim or a launcher starts its
    /// interpreter. An interpreter that records `planted-ran` is planted in the kept-out directory
    /// ahead of the installed one, which records `installed-ran`, prints `installed` and reads its
    /// standard input to the end. Reports whether the child reached the test and whether it
    /// passed, with what it printed.
    #[cfg(unix)]
    fn tool_started_with_a_planted_interpreter_ahead(
        test: &str,
        name: &str,
    ) -> (bool, bool, String) {
        let session = crate::testutil::scratch_dir(name);
        let tools = crate::testutil::scratch_dir(&format!("{name}-tools"));
        for directory in [&session, &tools] {
            let _ = std::fs::remove_dir_all(directory);
        }
        let planted = session.join(".venv/bin");
        let installed = tools.join("bravebot-test-interp");
        script(
            &planted,
            "bravebot-test-interp",
            &format!(
                ": > '{}'\nexec '{}' \"$@\"",
                session.join("planted-ran").display(),
                installed.display()
            ),
        );
        script(
            &tools,
            "bravebot-test-interp",
            &format!(
                ": > '{}'\necho installed\nwhile read -r _; do :; done",
                session.join("installed-ran").display()
            ),
        );
        // The tool is only a shebang naming the interpreter, so the interpreter is looked up by
        // name. Writing over the script keeps its executable mode.
        script(&tools, "bravebot-test-tool", "");
        std::fs::write(
            tools.join("bravebot-test-tool"),
            "#!/usr/bin/env bravebot-test-interp\n",
        )
        .unwrap();
        let path = std::env::join_paths([
            planted,
            tools.clone(),
            PathBuf::from("/usr/bin"),
            PathBuf::from("/bin"),
        ])
        .unwrap();

        // nosemgrep: rust.lang.security.current-exe.current-exe
        let child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", test, "--test-threads=1", "--nocapture"])
            .env(INTERPRETER_SESSION, &session)
            .env("PATH", &path)
            .output()
            .unwrap();

        let outcome = (
            session.join("ran").exists(),
            child.status.success(),
            String::from_utf8_lossy(&child.stderr).into_owned(),
        );
        for directory in [&session, &tools] {
            let _ = std::fs::remove_dir_all(directory);
        }
        outcome
    }

    /// The child half of [`tool_started_with_a_planted_interpreter_ahead`]: keeps the session out of
    /// the search, as a session's own directory is, for the rest of this process. Returns `None`
    /// when this process is not the child, and the test then runs the parent half.
    #[cfg(unix)]
    fn keep_the_session_out_in_the_child() -> Option<PathBuf> {
        let session = PathBuf::from(std::env::var_os(INTERPRETER_SESSION)?);
        bravebot_sandbox::programs::keep_out(&session);
        std::fs::write(session.join("ran"), "").unwrap();
        Some(session)
    }

    /// SANDBOX-30: a copy tool that is a script or a launcher looks the programs it starts up on the
    /// `PATH` it was started with, and it is started with the person's access to their clipboard
    /// and nothing confining it. An interpreter a confined stage wrote into a directory the
    /// session keeps out, ahead of the installed one on bravebot's `PATH`, must not be the one that
    /// runs. A tool started with the whole `PATH` runs the planted file first, then the installed
    /// one, and still copies, so the marker is what tells the two apart.
    #[cfg(unix)]
    #[test]
    fn a_copy_tool_that_starts_another_program_by_name_never_runs_one_a_stage_may_have_written() {
        const TEST: &str = "clipboard::tests::a_copy_tool_that_starts_another_program_by_name_never_runs_one_a_stage_may_have_written";
        if let Some(session) = keep_the_session_out_in_the_child() {
            assert!(
                pipe_into("bravebot-test-tool", &[], "hello"),
                "the installed interpreter did not take the text"
            );
            assert!(
                session.join("installed-ran").exists(),
                "the tool did not run the installed interpreter"
            );
            assert!(
                !session.join("planted-ran").exists(),
                "the tool looked its interpreter up in a directory a stage may write"
            );
            return;
        }

        let (ran, passed, report) =
            tool_started_with_a_planted_interpreter_ahead(TEST, "clipboard-copy-by-name");

        assert!(ran, "the child did not run the test: {report}");
        assert!(passed, "{report}");
    }

    /// The same for reading the clipboard, which starts its tool through [`run_bytes`], a start of
    /// its own: what the installed interpreter printed is what comes back.
    #[cfg(unix)]
    #[test]
    fn a_paste_tool_that_starts_another_program_by_name_never_runs_one_a_stage_may_have_written() {
        const TEST: &str = "clipboard::tests::a_paste_tool_that_starts_another_program_by_name_never_runs_one_a_stage_may_have_written";
        if let Some(session) = keep_the_session_out_in_the_child() {
            assert_eq!(
                run_bytes("bravebot-test-tool", &[]),
                Some(b"installed\n".to_vec())
            );
            assert!(
                !session.join("planted-ran").exists(),
                "the tool looked its interpreter up in a directory a stage may write"
            );
            return;
        }

        let (ran, passed, report) =
            tool_started_with_a_planted_interpreter_ahead(TEST, "clipboard-paste-by-name");

        assert!(ran, "the child did not run the test: {report}");
        assert!(passed, "{report}");
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

    /// Copying a file leaves its name behind as text, as copying an image in a browser leaves its
    /// page, so a clipboard holding files holds text beside them too. The files are what was copied,
    /// and neither a picture nor the text is so much as read for them.
    #[test]
    fn files_win_over_the_picture_and_the_text_beside_them() {
        let paths = vec!["/tmp/shot.png".to_string()];

        let pasted = files_or(Ok(paths.clone()), || {
            panic!("the clipboard was read past the files it named")
        });

        assert_eq!(pasted, Pasted::Files(paths));
    }

    /// A clipboard that names no files is read as it always was, so a picture and text still reach
    /// their own paths.
    #[test]
    fn a_clipboard_naming_no_files_reads_what_else_it_holds() {
        assert_eq!(
            files_or(Ok(Vec::new()), || chosen(None, || Some(
                "words".to_string()
            ))),
            Pasted::Text("words".to_string())
        );
        assert!(matches!(
            files_or(Ok(Vec::new()), || chosen(
                Some(("image/png", b"pixels".to_vec())),
                || None
            )),
            Pasted::Image(_)
        ));
    }

    /// A list with a name that cannot be carried is refused, and not swapped for the text beside it:
    /// that text is the name of the file the person copied, written out, and it would paste a name
    /// that is not the file.
    #[test]
    fn a_refused_list_is_not_swapped_for_the_text_beside_it() {
        let pasted = files_or(Err(Refused), || {
            panic!("the clipboard was read past a list it refused")
        });

        assert_eq!(pasted, Pasted::FilesRefused);
    }

    #[cfg(unix)]
    fn all_exist(_: &str) -> bool {
        true
    }

    /// A relative path would be read against whichever directory this process is in, which is not
    /// what the copy named.
    #[cfg(unix)]
    #[test]
    fn a_relative_path_on_the_clipboard_is_not_a_file_that_was_copied() {
        let kept = usable_files(
            vec![
                "shot.png".into(),
                "./shot.png".into(),
                "/tmp/shot.png".into(),
            ],
            all_exist,
        );

        assert_eq!(kept, Ok(vec!["/tmp/shot.png".to_string()]));
    }

    /// A name holding a newline would end the line a drop is read from and begin another, so a file
    /// named `a<newline>/etc/hosts` would be taken for two files and the second is a real one. A
    /// carriage return is a line break to the box a path is written into. The whole list is refused,
    /// since the files taken would not be the files copied.
    #[cfg(unix)]
    #[test]
    fn a_name_holding_a_line_break_refuses_the_whole_list_rather_than_being_read_as_two_files() {
        for hostile in ["/tmp/a\n/etc/hosts", "/tmp/a\r/etc/hosts"] {
            assert_eq!(
                usable_files(vec![hostile.into()], all_exist),
                Err(Refused),
                "{hostile:?}"
            );
            assert_eq!(
                usable_files(vec!["/tmp/ok.png".into(), hostile.into()], all_exist),
                Err(Refused),
                "{hostile:?}"
            );
        }
    }

    /// What a name that is not text arrives as is U+FFFD, and a file under that spelling is another
    /// file or none. It is refused even where no file has the name, since a list read without it
    /// would be the files copied minus one and say nothing.
    #[cfg(unix)]
    #[test]
    fn a_name_that_arrived_as_the_replacement_character_refuses_the_whole_list() {
        let lossy = "/tmp/a\u{FFFD}.png".to_string();

        assert_eq!(usable_files(vec![lossy.clone()], |_| false), Err(Refused));
        assert_eq!(usable_files(vec![lossy], all_exist), Err(Refused));
        assert_eq!(
            usable_files(vec!["/tmp/a.png".into()], all_exist),
            Ok(vec!["/tmp/a.png".to_string()])
        );
    }

    /// A control character other than a newline is the drop handling's to judge, as it is for a
    /// dragged file, and refuses a shell line there (DROP-6). Left out here, the rest of a list
    /// would be written into a command without the file that was copied with it.
    #[cfg(unix)]
    #[test]
    fn a_name_holding_a_tab_or_an_escape_is_left_for_the_drop_handling_to_judge() {
        let names: Vec<String> = vec!["/tmp/a\tb.png".into(), "/tmp/a\u{1b}b.png".into()];

        assert_eq!(usable_files(names.clone(), all_exist), Ok(names));
    }

    /// A directory, or a path that no longer names anything, is not a file a drop could take. One
    /// named with a newline is left out like any other directory, and is not a refusal.
    #[cfg(unix)]
    #[test]
    fn a_directory_and_a_missing_path_are_not_files_that_were_copied() {
        let directory = crate::testutil::scratch_dir("clipboard-usable-files");
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(directory.join("folder.png")).unwrap();
        std::fs::create_dir_all(directory.join("folder\n")).unwrap();
        std::fs::write(directory.join("real.png"), b"x").unwrap();
        let named = |name: &str| directory.join(name).to_string_lossy().into_owned();

        let kept = usable_files(
            vec![
                named("folder.png"),
                named("folder\n"),
                named("gone.png"),
                named("real.png"),
            ],
            |path| std::path::Path::new(path).is_file(),
        );

        let _ = std::fs::remove_dir_all(&directory);
        assert_eq!(kept, Ok(vec![named("real.png")]));
    }

    /// The order files are copied in is the order they are staged in, which is the order their
    /// markers number them.
    #[cfg(unix)]
    #[test]
    fn the_files_keep_the_order_they_were_copied_in() {
        let paths: Vec<String> = ["/tmp/c.png", "/tmp/a.png", "/tmp/b.png"]
            .map(String::from)
            .to_vec();

        assert_eq!(usable_files(paths.clone(), all_exist), Ok(paths));
    }

    #[cfg(not(target_os = "windows"))]
    fn flavour_named(offered: &str) -> &'static Flavour {
        platform::FLAVOURS
            .iter()
            .find(|flavour| flavour.offered == offered)
            .unwrap_or_else(|| panic!("no flavour {offered}"))
    }

    /// Reads the flavours in `held` and records which were asked for, in order.
    #[cfg(not(target_os = "windows"))]
    fn first_held(held: &[(&str, &[u8])]) -> (Option<(&'static str, Vec<u8>)>, Vec<&'static str>) {
        let mut asked = Vec::new();
        let found = first_picture(platform::FLAVOURS, |flavour| {
            asked.push(flavour.offered);
            held.iter()
                .find(|(offered, _)| *offered == flavour.offered)
                .map(|(_, bytes)| bytes.to_vec())
        });
        (found, asked)
    }

    /// A clipboard offering several flavours of one picture sends the first, and PNG is what was
    /// sent before any other was read, so it still wins and the others cost no read.
    #[cfg(not(target_os = "windows"))]
    #[test]
    fn a_png_is_read_before_a_jpeg_beside_it() {
        let png = platform::PNG;
        let jpeg = platform::JPEG;

        let (found, asked) = first_held(&[(png, b"png bytes"), (jpeg, b"jpeg bytes")]);

        assert_eq!(found, Some(("image/png", b"png bytes".to_vec())));
        assert_eq!(asked, vec![png], "a flavour was read after one answered");
    }

    /// A picture copied as a JPEG and as nothing else is a picture, sent as the JPEG it is.
    #[cfg(not(target_os = "windows"))]
    #[test]
    fn a_clipboard_holding_only_a_jpeg_yields_that_picture() {
        let jpeg = flavour_named(platform::JPEG);

        let (found, _) = first_held(&[(jpeg.offered, b"jpeg bytes")]);

        assert_eq!(found, Some(("image/jpeg", b"jpeg bytes".to_vec())));
    }

    /// A tool that ran and printed nothing has said there is no such flavour.
    #[cfg(not(target_os = "windows"))]
    #[test]
    fn a_flavour_that_reads_as_empty_is_not_a_picture() {
        let png = platform::PNG;
        let jpeg = platform::JPEG;

        let (found, _) = first_held(&[(png, b""), (jpeg, b"jpeg bytes")]);

        assert_eq!(found, Some(("image/jpeg", b"jpeg bytes".to_vec())));
    }

    /// A clipboard offering no picture flavour has no picture to send, so Ctrl-V falls through to the
    /// text on it. A picture reported for a clipboard that holds none would paste an image nobody
    /// copied in place of the words that were.
    #[cfg(not(target_os = "windows"))]
    #[test]
    fn a_clipboard_holding_no_picture_flavour_yields_none() {
        assert_eq!(first_held(&[]).0, None);
    }

    /// The media type of a flavour is what ends up in the data URL, so every one is a type the
    /// turn admits, which is the set a front end's `media` string selects from (PASTE-3).
    #[cfg(not(target_os = "windows"))]
    #[test]
    fn every_flavour_is_sent_as_a_media_type_the_turn_carries() {
        for flavour in platform::FLAVOURS {
            assert!(
                bravebot_agent::turn::PASTED_IMAGE_MEDIA.contains(&flavour.media_type),
                "{} is sent as {}, which the turn does not carry",
                flavour.offered,
                flavour.media_type
            );
        }
    }

    /// The system offers TIFF for a picture it holds in any form, and nothing here decodes TIFF, so
    /// the platform writes it as PNG and that is what it is sent as.
    #[cfg(target_os = "macos")]
    #[test]
    fn a_tiff_is_sent_as_the_png_the_platform_wrote_it_as() {
        let tiff = flavour_named(platform::TIFF);

        let (found, _) = first_held(&[(tiff.offered, b"png written from the tiff")]);

        assert_eq!(
            found,
            Some(("image/png", b"png written from the tiff".to_vec()))
        );
    }

    /// A JPEG over the cap is decoded and sent as the PNG a PNG over the cap is, since the scaling
    /// guesses the format from the bytes and a build that left the JPEG decoder out would refuse it.
    #[test]
    fn a_jpeg_is_decoded_and_scaled_down_to_a_png_as_a_png_over_the_cap_is() {
        let mut jpeg = Vec::new();
        image::RgbImage::from_pixel(3000, 1000, image::Rgb([200, 30, 30]))
            .write_to(
                &mut std::io::Cursor::new(&mut jpeg),
                image::ImageFormat::Jpeg,
            )
            .unwrap();

        let scaled = downscaled(&jpeg).expect("a JPEG was not decoded");

        assert!(scaled.starts_with(b"\x89PNG\r\n\x1a\n"), "not PNG bytes");
        let decoded = image::load_from_memory(&scaled).expect("the result decodes");
        assert_eq!(decoded.width(), DOWNSCALED_SIDE);
    }

    /// A name holding a newline is one path in the JSON a script prints and two in a list of lines.
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    #[test]
    fn a_script_reports_a_name_holding_a_newline_as_one_path() {
        let names = vec!["/tmp/a b.png".to_string(), "/tmp/a\n/etc/hosts".to_string()];

        let json = serde_json::to_string(&names).unwrap();

        assert!(!json.contains('\n'));
        assert_eq!(paths_from_json(&json), names);
    }

    /// What a script prints that is not an array of strings names no files, so a script that fails
    /// or prints an error is a clipboard with no files on it.
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    #[test]
    fn output_that_is_not_a_list_of_paths_names_no_files() {
        assert!(paths_from_json("").is_empty());
        assert!(paths_from_json("Error: no pasteboard").is_empty());
        assert!(paths_from_json("{\"path\":\"/tmp/a.png\"}").is_empty());
        assert!(paths_from_json("[1, 2]").is_empty());
    }

    /// Fake clipboard tools for the Linux readers: a directory holding `wl-paste` and `xclip`
    /// scripts, and the lookup that finds only there.
    #[cfg(all(unix, not(target_os = "macos")))]
    fn fake_tools(
        name: &str,
        wl_paste: Option<&str>,
        xclip: Option<&str>,
    ) -> (PathBuf, impl Fn(&OsStr) -> Option<PathBuf>) {
        let directory = crate::testutil::scratch_dir(name);
        let _ = std::fs::remove_dir_all(&directory);
        for (tool, body) in [("wl-paste", wl_paste), ("xclip", xclip)] {
            if let Some(body) = body {
                script(&directory, tool, body);
            }
        }
        std::fs::create_dir_all(&directory).unwrap();
        let path = std::env::join_paths([directory.clone()]).unwrap();
        let unrelated = crate::testutil::scratch_dir(&format!("{name}-writable"));
        (directory, move |program: &OsStr| {
            bravebot_sandbox::programs::find_in(program, &path, std::slice::from_ref(&unrelated))
        })
    }

    /// A fake tool that prints `jpeg bytes` for the JPEG type and fails for anything else, the way
    /// `wl-paste` does for a type the clipboard does not hold.
    #[cfg(all(unix, not(target_os = "macos")))]
    const WL_PASTE_WITH_ONLY_A_JPEG: &str = r#"
case "$*" in
  *"--type image/jpeg"*) printf 'jpeg bytes' ;;
  *) exit 1 ;;
esac"#;

    /// A picture copied as a JPEG is a picture on a Wayland desktop too. `wl-paste` is asked for each
    /// flavour in turn, fails for the ones the clipboard does not hold, and the JPEG it does hold is
    /// sent as it is, where a reader that asked for PNG alone would paste nothing.
    #[cfg(all(unix, not(target_os = "macos")))]
    #[test]
    fn a_jpeg_only_clipboard_yields_a_picture_through_wl_paste() {
        let (directory, find) = fake_tools(
            "clipboard-linux-wl-jpeg",
            Some(WL_PASTE_WITH_ONLY_A_JPEG),
            None,
        );

        let found = platform::image_found_by(find);

        let _ = std::fs::remove_dir_all(&directory);
        assert_eq!(found, Some(("image/jpeg", b"jpeg bytes".to_vec())));
    }

    /// The other display server's tool is asked for the JPEG with its own arguments.
    #[cfg(all(unix, not(target_os = "macos")))]
    #[test]
    fn a_jpeg_only_clipboard_yields_a_picture_through_xclip() {
        let (directory, find) = fake_tools(
            "clipboard-linux-xclip-jpeg",
            None,
            Some(
                r#"
case "$*" in
  "-selection clipboard -t image/jpeg -o") printf 'jpeg bytes' ;;
  *) exit 1 ;;
esac"#,
            ),
        );

        let found = platform::image_found_by(find);

        let _ = std::fs::remove_dir_all(&directory);
        assert_eq!(found, Some(("image/jpeg", b"jpeg bytes".to_vec())));
    }

    /// With both a PNG and a JPEG on offer the PNG is the one asked for first, so a JPEG-only
    /// answer from the same tool is never reached.
    #[cfg(all(unix, not(target_os = "macos")))]
    #[test]
    fn a_png_is_asked_for_before_a_jpeg_through_the_tools() {
        let (directory, find) = fake_tools(
            "clipboard-linux-png-first",
            Some(
                r#"
case "$*" in
  *"--type image/png"*) printf 'png bytes' ;;
  *"--type image/jpeg"*) printf 'jpeg bytes' ;;
  *) exit 1 ;;
esac"#,
            ),
            None,
        );

        let found = platform::image_found_by(find);

        let _ = std::fs::remove_dir_all(&directory);
        assert_eq!(found, Some(("image/png", b"png bytes".to_vec())));
    }

    /// The hint that Control-V would do something is shown for a JPEG as it is for a PNG.
    #[cfg(all(unix, not(target_os = "macos")))]
    #[test]
    fn a_clipboard_offering_a_jpeg_is_said_to_hold_an_image() {
        let (directory, find) = fake_tools(
            "clipboard-linux-types-jpeg",
            Some("printf 'text/plain\\nimage/jpeg\\n'"),
            None,
        );
        let (text_only, find_text_only) = fake_tools(
            "clipboard-linux-types-text",
            Some("printf 'text/plain\\n'"),
            None,
        );

        let holds_jpeg = platform::has_image_found_by(find);
        let holds_text_only = platform::has_image_found_by(find_text_only);

        let _ = std::fs::remove_dir_all(&directory);
        let _ = std::fs::remove_dir_all(&text_only);
        assert!(holds_jpeg);
        assert!(!holds_text_only);
    }

    /// GNOME's file managers leave their own type, a `copy` line and then one URI per line, and the
    /// `text/uri-list` is not one a clipboard tool can be assumed to be offered.
    #[cfg(all(unix, not(target_os = "macos")))]
    #[test]
    fn a_gnome_file_manager_copy_is_read_from_the_type_it_leaves() {
        let (directory, find) = fake_tools(
            "clipboard-linux-gnome-files",
            Some(
                r#"
case "$*" in
  *"--type x-special/gnome-copied-files"*) printf 'copy\nfile:///tmp/a%%20b.png\nfile:///tmp/c.txt' ;;
  *) exit 1 ;;
esac"#,
            ),
            None,
        );

        let files = platform::files_found_by(find);

        let _ = std::fs::remove_dir_all(&directory);
        assert_eq!(
            files,
            Ok(vec!["/tmp/a b.png".to_string(), "/tmp/c.txt".to_string()])
        );
    }

    /// A file manager's copy is a `text/uri-list` of `file://` URIs, and the paths come back
    /// decoded. A tool that fails for the type is a clipboard that holds no files.
    #[cfg(all(unix, not(target_os = "macos")))]
    #[test]
    fn a_file_manager_copy_is_read_from_the_uri_list_it_leaves() {
        let (directory, find) = fake_tools(
            "clipboard-linux-files",
            Some(
                r#"
case "$*" in
  *"--type text/uri-list"*) printf '# copied\r\nfile:///tmp/a%%20b.png\r\nfile:///tmp/c.txt\r\n' ;;
  *) exit 1 ;;
esac"#,
            ),
            None,
        );
        let (none, find_none) = fake_tools(
            "clipboard-linux-no-files",
            Some("printf 'text/plain'; exit 1"),
            None,
        );

        let files = platform::files_found_by(find);
        let no_files = platform::files_found_by(find_none);

        let _ = std::fs::remove_dir_all(&directory);
        let _ = std::fs::remove_dir_all(&none);
        assert_eq!(
            files,
            Ok(vec!["/tmp/a b.png".to_string(), "/tmp/c.txt".to_string()])
        );
        assert_eq!(no_files, Ok(Vec::new()));
    }

    /// A name that is not UTF-8 has no spelling as a `String`, so the list holding it is refused and
    /// not read without that file or with another file's name. The URI form of it and the bytes
    /// themselves are both a refusal, and the file whose name is the lookalike is an ordinary one.
    #[cfg(all(unix, not(target_os = "macos")))]
    #[test]
    fn a_file_whose_name_is_not_text_refuses_the_list_it_is_copied_in() {
        let tool = |body: &str, name: &str| {
            let (directory, find) = fake_tools(name, Some(body), None);
            let files = platform::files_found_by(find);
            let _ = std::fs::remove_dir_all(&directory);
            files
        };

        let in_a_uri = tool(
            "printf 'file:///tmp/ok.png\\nfile:///tmp/a%%FF.png\\n'",
            "clipboard-linux-not-text-uri",
        );
        let as_bytes = tool(
            "printf 'file:///tmp/a\\377.png\\n'",
            "clipboard-linux-not-text-bytes",
        );
        let lookalike = tool(
            "printf 'file:///tmp/a%%EF%%BF%%BD.png\\n'",
            "clipboard-linux-lookalike",
        );

        assert_eq!(in_a_uri, Err(Refused));
        assert_eq!(as_bytes, Err(Refused));
        assert_eq!(lookalike, Ok(vec!["/tmp/a\u{FFFD}.png".to_string()]));
    }

    /// A pasteboard of the test's own, so what it puts there is never what the person copied. It is
    /// named for the test and the process, and released when it goes out of scope.
    #[cfg(target_os = "macos")]
    struct PrivatePasteboard {
        name: String,
    }

    #[cfg(target_os = "macos")]
    impl PrivatePasteboard {
        /// A script that empties the pasteboard named by its first argument and then puts on it
        /// either the file at the third argument as the type named by the second, or, when the
        /// second is `files`, the file URLs of every argument after it.
        const SET: &'static str = "function run(argv) { \
            ObjC.import('AppKit'); \
            const pb = $.NSPasteboard.pasteboardWithName(argv[0]); \
            pb.clearContents; \
            if (argv[1] === 'files') { \
            const urls = $.NSMutableArray.alloc.init; \
            for (const file of argv.slice(2)) urls.addObject($.NSURL.fileURLWithPath(file)); \
            pb.writeObjects(urls); \
            } else { \
            pb.setDataForType($.NSData.dataWithContentsOfFile(argv[2]), argv[1]); } \
            return 'done'; }";

        const RELEASE: &'static str = "function run(argv) { \
            ObjC.import('AppKit'); \
            $.NSPasteboard.pasteboardWithName(argv[0]).releaseGlobally; \
            return 'done'; }";

        fn named(test: &str) -> Self {
            Self {
                name: format!("bravebot-test-{test}-{}", std::process::id()),
            }
        }

        fn run(script: &str, arguments: &[&str]) {
            let mut all = vec!["-l", "JavaScript", "-e", script];
            all.extend(arguments);
            assert_eq!(
                run_text("osascript", &all).as_deref().map(str::trim),
                Some("done"),
                "the pasteboard script did not run"
            );
        }

        /// Put `bytes` on the pasteboard as the type `uti`, and nothing else.
        fn holding(test: &str, uti: &str, bytes: &[u8]) -> Self {
            let pasteboard = Self::named(test);
            let file = crate::testutil::scratch_dir(&format!("clipboard-pasteboard-{test}"));
            let _ = std::fs::remove_dir_all(&file);
            std::fs::create_dir_all(&file).unwrap();
            let file = file.join("data");
            std::fs::write(&file, bytes).unwrap();
            Self::run(Self::SET, &[&pasteboard.name, uti, &file.to_string_lossy()]);
            let _ = std::fs::remove_dir_all(file.parent().unwrap());
            pasteboard
        }

        /// Put the files at `paths` on the pasteboard, the way Finder's copy does.
        fn holding_files(test: &str, paths: &[String]) -> Self {
            let pasteboard = Self::named(test);
            let mut arguments = vec![pasteboard.name.as_str(), "files"];
            arguments.extend(paths.iter().map(String::as_str));
            Self::run(Self::SET, &arguments);
            pasteboard
        }
    }

    #[cfg(target_os = "macos")]
    impl Drop for PrivatePasteboard {
        fn drop(&mut self) {
            Self::run(Self::RELEASE, &[&self.name]);
        }
    }

    #[cfg(target_os = "macos")]
    fn picture_as(format: image::ImageFormat) -> Vec<u8> {
        let mut out = Vec::new();
        image::RgbImage::from_pixel(8, 8, image::Rgb([200, 30, 30]))
            .write_to(&mut std::io::Cursor::new(&mut out), format)
            .unwrap();
        out
    }

    /// A big-endian RGB TIFF of one strip, `width` by `height` pixels, with the strip as given and
    /// `compression` the TIFF code for how it is packed. The `image` crate here has no TIFF encoder,
    /// and a TIFF is what the test needs the pasteboard to hold.
    #[cfg(target_os = "macos")]
    fn tiff_with(width: u16, height: u16, compression: u16, strip: &[u8]) -> Vec<u8> {
        let mut tiff = vec![b'M', b'M', 0, 42, 0, 0, 0, 8, 0, 9];
        // Tag, type (3 is SHORT, 4 is LONG), count, then the value or the offset of it. The values
        // that are SHORT sit in the high half of the four bytes.
        let entries: [(u16, u16, u32, u32); 9] = [
            (256, 3, 1, u32::from(width) << 16),       // ImageWidth
            (257, 3, 1, u32::from(height) << 16),      // ImageLength
            (258, 3, 3, 122),                          // BitsPerSample, at 122
            (259, 3, 1, u32::from(compression) << 16), // Compression
            (262, 3, 1, 2 << 16),                      // PhotometricInterpretation: RGB
            (273, 4, 1, 128),                          // StripOffsets
            (277, 3, 1, 3 << 16),                      // SamplesPerPixel
            (278, 3, 1, u32::from(height) << 16),      // RowsPerStrip
            (279, 4, 1, strip.len() as u32),           // StripByteCounts
        ];
        for (tag, kind, count, value) in entries {
            tiff.extend(tag.to_be_bytes());
            tiff.extend(kind.to_be_bytes());
            tiff.extend(count.to_be_bytes());
            tiff.extend(value.to_be_bytes());
        }
        tiff.extend([0, 0, 0, 0]);
        tiff.extend([0, 8, 0, 8, 0, 8]);
        tiff.extend(strip);
        tiff
    }

    /// A one pixel, uncompressed TIFF of a red pixel.
    #[cfg(target_os = "macos")]
    fn red_pixel_tiff() -> Vec<u8> {
        tiff_with(1, 1, 1, &[255, 0, 0])
    }

    /// A TIFF of black, `side` pixels square, packed with PackBits: a few megabytes that declare a
    /// canvas of gigabytes once it is drawn.
    #[cfg(target_os = "macos")]
    fn compressed_black_tiff(side: u16) -> Vec<u8> {
        let row_bytes = usize::from(side) * 3;
        // A run of `n` zero bytes is the header `257 - n` and the byte, and `n` is at most 128.
        let mut row = Vec::new();
        for run in (0..row_bytes).step_by(128) {
            let n = (row_bytes - run).min(128);
            row.extend([(257 - n) as u8, 0]);
        }
        tiff_with(side, side, 32773, &row.repeat(usize::from(side)))
    }

    /// The reader's own script, against a pasteboard that holds a PNG: what the person pasted before
    /// any other flavour was read is what comes back, byte for byte.
    #[cfg(target_os = "macos")]
    #[test]
    fn a_pasteboard_holding_a_png_is_read_as_that_png() {
        let png = picture_as(image::ImageFormat::Png);
        let pasteboard = PrivatePasteboard::holding("png", platform::PNG, &png);

        assert_eq!(
            platform::image_from(&pasteboard.name),
            Some(("image/png", png))
        );
    }

    /// A pasteboard holding only a JPEG is read as that JPEG. The system offers a TIFF for it as
    /// well, so an order that tried TIFF before JPEG would send a PNG drawn from the picture, and
    /// the bytes would differ from what was copied.
    #[cfg(target_os = "macos")]
    #[test]
    fn a_pasteboard_holding_only_a_jpeg_is_read_as_that_jpeg() {
        let jpeg = picture_as(image::ImageFormat::Jpeg);
        let pasteboard = PrivatePasteboard::holding("jpeg", platform::JPEG, &jpeg);

        assert_eq!(
            platform::image_from(&pasteboard.name),
            Some(("image/jpeg", jpeg))
        );
    }

    /// A pasteboard holding only a TIFF is read as a PNG that shows the same picture, not as the
    /// TIFF bytes under a PNG label.
    #[cfg(target_os = "macos")]
    #[test]
    fn a_pasteboard_holding_only_a_tiff_is_read_as_a_png_of_the_same_picture() {
        let pasteboard = PrivatePasteboard::holding("tiff", platform::TIFF, &red_pixel_tiff());

        let (media_type, bytes) = platform::image_from(&pasteboard.name).expect("no picture read");

        assert_eq!(media_type, "image/png");
        assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"), "not PNG bytes");
        let drawn = image::load_from_memory_with_format(&bytes, image::ImageFormat::Png)
            .unwrap()
            .to_rgb8();
        assert_eq!(drawn.get_pixel(0, 0).0, [255, 0, 0]);
    }

    /// A TIFF of a few megabytes that declares a canvas of gigabytes is not drawn: converting it
    /// takes seconds and a gigabyte inside a process this one waits on, and the cap on the picture
    /// that comes back is no limit on what it took to make it. The same bytes at a size the decode
    /// limits allow are drawn.
    #[cfg(target_os = "macos")]
    #[test]
    fn a_pasteboard_holding_a_tiff_declaring_a_canvas_past_the_limit_is_not_drawn() {
        let past = PrivatePasteboard::holding(
            "tiff-past-limit",
            platform::TIFF,
            &compressed_black_tiff(12000),
        );
        let within = PrivatePasteboard::holding(
            "tiff-within-limit",
            platform::TIFF,
            &compressed_black_tiff(1000),
        );

        assert_eq!(platform::image_from(&past.name), None);
        let (media_type, _) = platform::image_from(&within.name).expect("a TIFF within the limits");
        assert_eq!(media_type, "image/png");
    }

    /// Text on the pasteboard is not a picture in any flavour.
    #[cfg(target_os = "macos")]
    #[test]
    fn a_pasteboard_holding_only_text_reads_as_no_picture() {
        let pasteboard = PrivatePasteboard::holding("text", "public.utf8-plain-text", b"words");

        assert_eq!(platform::image_from(&pasteboard.name), None);
        assert!(!platform::has_image_in(&pasteboard.name));
    }

    /// The hint that Ctrl-V would paste a picture is shown for a JPEG and for a TIFF as it is for a
    /// PNG.
    #[cfg(target_os = "macos")]
    #[test]
    fn a_pasteboard_offering_any_picture_flavour_is_said_to_hold_an_image() {
        let png = PrivatePasteboard::holding(
            "hint-png",
            platform::PNG,
            &picture_as(image::ImageFormat::Png),
        );
        let jpeg = PrivatePasteboard::holding(
            "hint-jpeg",
            platform::JPEG,
            &picture_as(image::ImageFormat::Jpeg),
        );
        let tiff = PrivatePasteboard::holding("hint-tiff", platform::TIFF, &red_pixel_tiff());

        assert!(platform::has_image_in(&png.name));
        assert!(platform::has_image_in(&jpeg.name));
        assert!(platform::has_image_in(&tiff.name));
    }

    /// Files copied in Finder come back as their paths in the order they were copied, and a path
    /// holding a newline stays one path, since a list of lines would make it two.
    #[cfg(target_os = "macos")]
    #[test]
    fn the_files_on_a_pasteboard_are_read_as_their_paths_in_order() {
        let directory = crate::testutil::scratch_dir("clipboard-pasteboard-files");
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).unwrap();
        // The last is a file under a directory whose name ends in a newline, so the line after the
        // newline in its path is a real file: `/etc/hosts` under the scratch directory.
        std::fs::create_dir_all(directory.join("a\n/etc")).unwrap();
        let paths: Vec<String> = ["b second.png", "it's.png", "a\n/etc/hosts"]
            .iter()
            .map(|name| {
                let at = directory.join(name);
                std::fs::write(&at, b"x").unwrap();
                at.to_string_lossy().into_owned()
            })
            .collect();
        let pasteboard = PrivatePasteboard::holding_files("files", &paths);

        let read = platform::files_from(&pasteboard.name);

        let _ = std::fs::remove_dir_all(&directory);
        assert_eq!(read, paths);
    }

    /// A pasteboard that names no files reads as no files, whatever else it holds.
    #[cfg(target_os = "macos")]
    #[test]
    fn a_pasteboard_holding_only_a_picture_names_no_files() {
        let pasteboard = PrivatePasteboard::holding(
            "no-files",
            platform::PNG,
            &picture_as(image::ImageFormat::Png),
        );

        assert!(platform::files_from(&pasteboard.name).is_empty());
    }
}
