//! What a file's name says it is: a picture or a PDF carried as bytes, text read into a turn, or
//! neither.
//!
//! Decided from the name alone, never from the bytes. A file dropped on the terminal, a file
//! dropped on the desktop window and a file a processor is asked about are the same kinds of file,
//! because each asks this module. A second list in any of them would be a second answer waiting to
//! disagree.

use std::path::Path;

/// Extensions carried as bytes, with the media type to name in the URI.
///
/// The set Claude Code takes. Decided by extension rather than by looking at the file: naming a
/// type from the bytes would be the driver deciding something from content nobody has vouched for,
/// and the type ends up in a `data:` URI where it is routing.
pub const ATTACHABLE: &[(&str, &str)] = &[
    ("png", "image/png"),
    ("jpg", "image/jpeg"),
    ("jpeg", "image/jpeg"),
    ("gif", "image/gif"),
    ("webp", "image/webp"),
    ("pdf", "application/pdf"),
];

/// The media type for `path`, where its extension names one carried as bytes.
///
/// The extension is compared without regard to case, since `SHOT.PNG` is the same kind of file as
/// `shot.png` and a person naming one means the picture either way.
pub fn media_for(path: &str) -> Option<&'static str> {
    let extension = path.rsplit_once('.')?.1.to_ascii_lowercase();
    ATTACHABLE
        .iter()
        .find(|(named, _)| *named == extension)
        .map(|(_, media)| *media)
}

/// What a dropped file is, and therefore what happens to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Carried to the model as bytes, in the media type named here.
    Attachment(&'static str),
    /// Read into the turn as text, the way a file named with `@` is.
    Text,
}

impl Kind {
    /// The word a marker uses, so a person can tell one dropped thing from another: `[Image #1]`,
    /// `[PDF #2]`, `[File #3]`.
    ///
    /// Not from a catalog: this word goes into the marker the planner is sent, so it is part of
    /// what the model reads rather than something a person is being told.
    pub fn noun(self) -> &'static str {
        match self {
            Kind::Attachment("application/pdf") => "PDF",
            Kind::Attachment(_) => "Image",
            Kind::Text => "File",
        }
    }
}

/// Extensions read as text, which is what the model wants of them anyway.
const TEXTUAL: &[&str] = &[
    "txt",
    "md",
    "markdown",
    "rst",
    "adoc",
    "org",
    "rs",
    "py",
    "js",
    "jsx",
    "mjs",
    "cjs",
    "ts",
    "tsx",
    "vue",
    "svelte",
    "json",
    "jsonc",
    "yaml",
    "yml",
    "toml",
    "ini",
    "cfg",
    "conf",
    "properties",
    "env",
    "html",
    "htm",
    "xml",
    "svg",
    "css",
    "scss",
    "sass",
    "less",
    "sh",
    "bash",
    "zsh",
    "fish",
    "ps1",
    "bat",
    "c",
    "h",
    "cc",
    "cpp",
    "cxx",
    "hpp",
    "hh",
    "java",
    "kt",
    "kts",
    "go",
    "rb",
    "php",
    "swift",
    "m",
    "mm",
    "cs",
    "scala",
    "clj",
    "cljs",
    "ex",
    "exs",
    "erl",
    "hs",
    "lua",
    "pl",
    "pm",
    "r",
    "jl",
    "dart",
    "zig",
    "nim",
    "sql",
    "graphql",
    "proto",
    "csv",
    "tsv",
    "log",
    "diff",
    "patch",
    "lock",
    "gradle",
    "tf",
    "tfvars",
    "dockerfile",
    "mk",
    "cmake",
];

/// Names that are text without an extension to say so.
const TEXTUAL_NAMES: &[&str] = &[
    "makefile",
    "dockerfile",
    "readme",
    "license",
    "licence",
    "changelog",
    "authors",
    "notice",
    "gemfile",
    "rakefile",
    "procfile",
    "justfile",
    "vagrantfile",
    "brewfile",
];

/// What a path's name says it is, or `None` for something neither carried nor read.
///
/// A `.dmg` lands here, and a front end writes its path out rather than pretending to attach it.
pub fn kind_of(path: &str) -> Option<Kind> {
    let name = Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())?;

    // Split on the last dot by hand rather than asking for an extension, so a name that is all
    // extension, `.gitignore`, is read as a name and not as an extension of nothing.
    let extension = name.rsplit_once('.').map(|(stem, ext)| {
        if stem.is_empty() {
            String::new()
        } else {
            ext.to_string()
        }
    });

    if let Some(extension) = &extension {
        if let Some((_, media)) = ATTACHABLE.iter().find(|(ext, _)| ext == extension) {
            return Some(Kind::Attachment(media));
        }
        if TEXTUAL.contains(&extension.as_str()) {
            return Some(Kind::Text);
        }
    }

    // A name with no usable extension: `Makefile`, or `.gitignore`, whose whole name is the name.
    let bare = name.strip_prefix('.').unwrap_or(&name);
    if TEXTUAL_NAMES.contains(&bare) || bare.starts_with("gitignore") || bare.starts_with("gitattr")
    {
        return Some(Kind::Text);
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_recognised_types_are_the_ones_claude_code_takes() {
        for name in ["a.png", "a.jpg", "a.jpeg", "a.gif", "a.webp"] {
            assert!(
                matches!(kind_of(name), Some(Kind::Attachment(media)) if media.starts_with("image/")),
                "{name} is not an image"
            );
        }
        assert_eq!(kind_of("a.pdf"), Some(Kind::Attachment("application/pdf")));
        for name in [
            "a.rs",
            "a.md",
            "a.json",
            "Makefile",
            "Dockerfile",
            ".gitignore",
            ".gitattributes",
            "/a/README",
        ] {
            assert_eq!(kind_of(name), Some(Kind::Text), "{name} is not text");
        }
        for name in [
            "a.dmg",
            "a.zip",
            "a.mp4",
            "a.so",
            "/a/notes.",
            "/a/archive.tar.gz",
            "/a/plain",
        ] {
            assert_eq!(kind_of(name), None, "{name} should not be taken");
        }
    }

    /// Case is the filesystem's business, not the person's.
    #[test]
    fn an_extension_is_recognised_whatever_its_case() {
        assert_eq!(
            kind_of("/tmp/SHOT.PNG"),
            Some(Kind::Attachment("image/png"))
        );
        assert_eq!(
            kind_of("/tmp/scan.Pdf"),
            Some(Kind::Attachment("application/pdf"))
        );
    }

    /// A name that is all extension is a name, so `.png` is not a picture.
    #[test]
    fn a_name_that_is_all_extension_is_not_that_type() {
        assert_eq!(kind_of("/a/.png"), None);
    }

    #[test]
    fn the_noun_names_what_was_dropped() {
        assert_eq!(Kind::Attachment("image/png").noun(), "Image");
        assert_eq!(Kind::Attachment("image/webp").noun(), "Image");
        assert_eq!(Kind::Attachment("application/pdf").noun(), "PDF");
        assert_eq!(Kind::Text.noun(), "File");
    }
}
