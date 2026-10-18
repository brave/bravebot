//! The repository configuration, read only to decline what this reader would get wrong.

use super::Declined;

/// Decline a configuration this reader would answer differently from git under.
///
/// Parsed by hand, to git's syntax, because only three facts are wanted from it and a
/// configuration crate would read `include.path` by following it. Anything that does not parse is
/// declined rather than guessed at: a key misread here is a key git reads another way.
pub(super) fn check_config(bytes: &[u8]) -> Result<(), Declined> {
    let entries = parse_config(bytes).ok_or(Declined::Format)?;
    for entry in entries {
        match (entry.section.as_str(), entry.subsection.is_some()) {
            ("include" | "includeif", _) => return Err(Declined::Include),
            ("core", false) if entry.key == "worktree" => return Err(Declined::Worktree),
            ("core", false) if entry.key == "repositoryformatversion" => {
                let value = entry.value.as_deref().unwrap_or_default();
                let version = std::str::from_utf8(value)
                    .ok()
                    .and_then(|v| v.trim().parse::<u32>().ok());
                if !matches!(version, Some(0 | 1)) {
                    return Err(Declined::Format);
                }
            }
            ("extensions", false) => {
                let value = entry
                    .value
                    .as_deref()
                    .map(|v| String::from_utf8_lossy(v).to_ascii_lowercase());
                let known = match entry.key.as_str() {
                    "noop" | "preciousobjects" | "partialclone" | "worktreeconfig" => true,
                    "objectformat" => value.as_deref() == Some("sha1"),
                    "refstorage" => value.as_deref() == Some("files"),
                    _ => false,
                };
                if !known {
                    return Err(Declined::Format);
                }
            }
            _ => {}
        }
    }
    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct ConfigEntry {
    pub(super) section: String,
    pub(super) subsection: Option<Vec<u8>>,
    pub(super) key: String,
    pub(super) value: Option<Vec<u8>>,
}

/// A section's name, and its subsection where it has one.
type Section = (String, Option<Vec<u8>>);

pub(super) fn parse_config(bytes: &[u8]) -> Option<Vec<ConfigEntry>> {
    let bytes = bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes);
    let mut text = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\r' && bytes.get(i + 1) == Some(&b'\n') {
            i += 1;
            continue;
        }
        text.push(bytes[i]);
        i += 1;
    }
    let b = text.as_slice();
    let mut entries = Vec::new();
    let mut section: Option<Section> = None;
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b' ' | b'\t' | b'\n' | b'\r' => i += 1,
            b'#' | b';' => i = end_of_line(b, i),
            b'[' => {
                let (header, next) = parse_header(b, i + 1)?;
                section = Some(header);
                i = next;
            }
            c if c.is_ascii_alphabetic() => {
                let (name, subsection) = section.clone()?;
                let start = i;
                while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'-') {
                    i += 1;
                }
                let key = String::from_utf8_lossy(&b[start..i]).to_ascii_lowercase();
                while i < b.len() && matches!(b[i], b' ' | b'\t' | b'\r') {
                    i += 1;
                }
                let value = match b.get(i) {
                    None | Some(b'\n') => None,
                    Some(b'#' | b';') => {
                        i = end_of_line(b, i);
                        None
                    }
                    Some(b'=') => {
                        let (value, next) = parse_value(b, i + 1)?;
                        i = next;
                        Some(value)
                    }
                    Some(_) => return None,
                };
                entries.push(ConfigEntry {
                    section: name,
                    subsection,
                    key,
                    value,
                });
            }
            _ => return None,
        }
    }
    Some(entries)
}

fn end_of_line(b: &[u8], mut i: usize) -> usize {
    while i < b.len() && b[i] != b'\n' {
        i += 1;
    }
    i
}

/// `[section]`, `[section "subsection"]` or the older `[section.subsection]`, from just past the
/// `[`. What follows the `]` on the same line is read as the rest of the file is, since git
/// allows a key there.
fn parse_header(b: &[u8], mut i: usize) -> Option<(Section, usize)> {
    let start = i;
    while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'-' || b[i] == b'.') {
        i += 1;
    }
    let name = String::from_utf8_lossy(&b[start..i]).to_ascii_lowercase();
    if name.is_empty() {
        return None;
    }
    match b.get(i)? {
        b']' => {
            let header = match name.split_once('.') {
                Some((section, sub)) if !section.is_empty() => {
                    (section.to_owned(), Some(sub.as_bytes().to_vec()))
                }
                Some(_) => return None,
                None => (name, None),
            };
            Some((header, i + 1))
        }
        b' ' | b'\t' => {
            while i < b.len() && matches!(b[i], b' ' | b'\t') {
                i += 1;
            }
            if b.get(i) != Some(&b'"') {
                return None;
            }
            i += 1;
            let mut sub = Vec::new();
            loop {
                match *b.get(i)? {
                    b'"' => break,
                    b'\n' => return None,
                    b'\\' => {
                        let next = *b.get(i + 1)?;
                        if next == b'\n' {
                            return None;
                        }
                        sub.push(next);
                        i += 2;
                    }
                    c => {
                        sub.push(c);
                        i += 1;
                    }
                }
            }
            if b.get(i + 1) != Some(&b']') {
                return None;
            }
            Some(((name, Some(sub)), i + 2))
        }
        _ => None,
    }
}

/// A value from just past its `=`, to git's rules: quotes toggle, a backslash escapes `n`, `t`,
/// `b`, `"`, `\` or the end of the line, an unquoted `#` or `;` starts a comment, and whitespace
/// at either end outside quotes is dropped.
fn parse_value(b: &[u8], mut i: usize) -> Option<(Vec<u8>, usize)> {
    let mut out = Vec::new();
    let mut quoted = false;
    let mut spaces = Vec::new();
    loop {
        let Some(&c) = b.get(i) else {
            if quoted {
                return None;
            }
            return Some((out, i));
        };
        i += 1;
        if c == b'\n' {
            if quoted {
                return None;
            }
            return Some((out, i));
        }
        if !quoted && (c == b'#' || c == b';') {
            return Some((out, end_of_line(b, i)));
        }
        if !quoted && (c == b' ' || c == b'\t') {
            spaces.push(c);
            continue;
        }
        if !out.is_empty() {
            out.append(&mut spaces);
        }
        spaces.clear();
        match c {
            b'\\' => {
                let next = *b.get(i)?;
                i += 1;
                match next {
                    b'\n' => {}
                    b'n' => out.push(b'\n'),
                    b't' => out.push(b'\t'),
                    b'b' => out.push(8),
                    b'"' | b'\\' => out.push(next),
                    _ => return None,
                }
            }
            b'"' => quoted = !quoted,
            _ => out.push(c),
        }
    }
}
