//! Terminal input keybindings.
//!
//! Maps interactive terminal actions to keyboard chords, allowing users to override
//! bindings that collide with historical terminal conventions (such as Ctrl-S for XOFF
//! flow control or Ctrl-O for tty output discard).
//!
//! A configured chord has to carry Ctrl or Alt, and four of those are spoken for anyway. Anything
//! else, and anything the parser cannot read, leaves the action on the chord it had.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::collections::{BTreeMap, HashSet};

/// A parsed keyboard chord (code and modifier set).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyChord {
    pub code: KeyCode,
    pub modifiers: KeyModifiers,
}

impl KeyChord {
    /// Create a new key chord.
    pub const fn new(code: KeyCode, modifiers: KeyModifiers) -> Self {
        Self { code, modifiers }
    }

    /// Convenience for a Ctrl + letter chord.
    pub const fn ctrl(c: char) -> Self {
        Self {
            code: KeyCode::Char(c),
            modifiers: KeyModifiers::CONTROL,
        }
    }

    /// Convenience for an Alt/Option + letter chord.
    pub const fn alt(c: char) -> Self {
        Self {
            code: KeyCode::Char(c),
            modifiers: KeyModifiers::ALT,
        }
    }

    /// Whether this chord matches a Crossterm key event.
    pub fn matches(&self, event: &KeyEvent) -> bool {
        self.code == event.code && self.modifiers == event.modifiers
    }

    /// Format this chord into a lowercase hyphen-separated name (e.g. "ctrl-s", "alt-o").
    pub fn display(&self) -> String {
        let mut parts = Vec::new();
        if self.modifiers.contains(KeyModifiers::CONTROL) {
            parts.push("ctrl");
        }
        if self.modifiers.contains(KeyModifiers::ALT) {
            parts.push("alt");
        }
        if self.modifiers.contains(KeyModifiers::SHIFT) {
            parts.push("shift");
        }
        let code_str = match self.code {
            KeyCode::Char(c) => c.to_string(),
            KeyCode::Enter => "enter".to_string(),
            KeyCode::Esc => "esc".to_string(),
            KeyCode::Tab => "tab".to_string(),
            KeyCode::BackTab => "backtab".to_string(),
            KeyCode::Backspace => "backspace".to_string(),
            KeyCode::Up => "up".to_string(),
            KeyCode::Down => "down".to_string(),
            KeyCode::Left => "left".to_string(),
            KeyCode::Right => "right".to_string(),
            KeyCode::PageUp => "pgup".to_string(),
            KeyCode::PageDown => "pgdn".to_string(),
            KeyCode::Home => "home".to_string(),
            KeyCode::End => "end".to_string(),
            KeyCode::F(n) => format!("f{n}"),
            _ => "key".to_string(),
        };
        parts.push(&code_str);
        parts.join("-")
    }

    /// Parse a chord from text, supporting formats like "ctrl-s", "ctrl+s", "alt-o", "meta+p".
    pub fn parse(text: &str) -> Option<Self> {
        let s = text.trim().to_ascii_lowercase();
        if s.is_empty() {
            return None;
        }

        let delimiter = if s.contains('+') {
            '+'
        } else if s.contains('-') {
            '-'
        } else {
            ' '
        };

        let parts: Vec<&str> = s
            .split(delimiter)
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .collect();
        if parts.is_empty() {
            return None;
        }

        let mut modifiers = KeyModifiers::NONE;
        let mut key_part = None;

        for part in parts {
            match part {
                "ctrl" | "control" => modifiers |= KeyModifiers::CONTROL,
                "alt" | "opt" | "option" | "meta" => modifiers |= KeyModifiers::ALT,
                "shift" => modifiers |= KeyModifiers::SHIFT,
                other => {
                    if key_part.is_some() {
                        return None;
                    }
                    key_part = Some(other);
                }
            }
        }

        let key_str = key_part?;
        let code = match key_str {
            "enter" | "return" => KeyCode::Enter,
            "esc" | "escape" => KeyCode::Esc,
            "tab" => KeyCode::Tab,
            "backtab" => KeyCode::BackTab,
            "backspace" => KeyCode::Backspace,
            "up" => KeyCode::Up,
            "down" => KeyCode::Down,
            "left" => KeyCode::Left,
            "right" => KeyCode::Right,
            "pageup" | "pgup" => KeyCode::PageUp,
            "pagedown" | "pgdn" => KeyCode::PageDown,
            "home" => KeyCode::Home,
            "end" => KeyCode::End,
            f if f.starts_with('f')
                && f.len() > 1
                && f[1..].chars().all(|c| c.is_ascii_digit()) =>
            {
                let num: u8 = f[1..].parse().ok()?;
                KeyCode::F(num)
            }
            c if c.chars().count() == 1 => KeyCode::Char(c.chars().next()?),
            _ => return None,
        };

        Some(Self { code, modifiers })
    }

    /// Whether this chord is one the box already answers, and so is not an action's to take.
    ///
    /// A chord has to carry Ctrl or Alt, because every unmodified key is spoken for: a letter or a
    /// punctuation mark is typed into the line, Enter sends, Escape clears it, Tab takes what is
    /// offered, and the arrows walk the caret and the history (INPUT-4, INPUT-13).
    ///
    /// Shift over a character is refused for a different reason: a terminal reports Shift-A as `A`
    /// with Shift held, so a chord written `shift-a` or `ctrl-shift-a` names an event that never
    /// arrives. Taking it would leave the action quietly dead instead of on a key that works.
    ///
    /// Four chords are spoken for even carrying Ctrl. Ctrl-C stops the nearest thing there is to
    /// stop and leaves from an empty box, and Ctrl-D leaves (INPUT-4); Ctrl-J and Shift-Enter start
    /// a line (INPUT-2).
    pub fn is_reserved(&self) -> bool {
        if !self.modifiers.contains(KeyModifiers::CONTROL)
            && !self.modifiers.contains(KeyModifiers::ALT)
        {
            return true;
        }
        if self.code == KeyCode::Enter {
            return true;
        }
        if self.modifiers.contains(KeyModifiers::SHIFT) && matches!(self.code, KeyCode::Char(_)) {
            return true;
        }
        self.modifiers == KeyModifiers::CONTROL
            && matches!(
                self.code,
                KeyCode::Char('c') | KeyCode::Char('d') | KeyCode::Char('j')
            )
    }
}

/// Active keybindings for the interactive TUI prompt and navigation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Keybindings {
    pub stash: KeyChord,
    pub scroller: KeyChord,
    pub editor: KeyChord,
    pub history: KeyChord,
    pub trail: KeyChord,
    pub watch: KeyChord,
    pub paste: KeyChord,
}

impl Default for Keybindings {
    fn default() -> Self {
        Self {
            stash: KeyChord::ctrl('s'),
            scroller: KeyChord::ctrl('o'),
            editor: KeyChord::ctrl('g'),
            history: KeyChord::ctrl('r'),
            trail: KeyChord::ctrl('t'),
            watch: KeyChord::ctrl('l'),
            paste: KeyChord::ctrl('v'),
        }
    }
}

impl Keybindings {
    /// Build keybindings from a configured action-to-chord string map.
    ///
    /// Malformed chords, reserved chords, or chords that conflict with another
    /// action revert gracefully to their defaults.
    pub fn from_map(configured: &BTreeMap<String, String>) -> Self {
        let defaults = Self::default();
        let mut active = defaults.clone();
        let mut used = HashSet::new();

        // Helper to resolve one action binding safely.
        let mut resolve_action = |name: &str, default_chord: KeyChord, slot: &mut KeyChord| {
            if let Some(chord) = configured
                .get(name)
                .and_then(|spec| KeyChord::parse(spec))
                .filter(|chord| !chord.is_reserved() && !used.contains(chord))
            {
                *slot = chord;
                used.insert(chord);
                return;
            }
            // Retain or fallback to default
            if !used.contains(&default_chord) {
                *slot = default_chord;
                used.insert(default_chord);
            }
        };

        resolve_action("stash", defaults.stash, &mut active.stash);
        resolve_action("scroller", defaults.scroller, &mut active.scroller);
        resolve_action("editor", defaults.editor, &mut active.editor);
        resolve_action("history", defaults.history, &mut active.history);
        resolve_action("trail", defaults.trail, &mut active.trail);
        resolve_action("watch", defaults.watch, &mut active.watch);
        resolve_action("paste", defaults.paste, &mut active.paste);

        active
    }

    pub fn is_stash(&self, key: &KeyEvent) -> bool {
        self.stash.matches(key)
    }

    pub fn is_scroller(&self, key: &KeyEvent) -> bool {
        self.scroller.matches(key)
    }

    pub fn is_editor(&self, key: &KeyEvent) -> bool {
        self.editor.matches(key)
    }

    pub fn is_history(&self, key: &KeyEvent) -> bool {
        self.history.matches(key)
    }

    pub fn is_trail(&self, key: &KeyEvent) -> bool {
        self.trail.matches(key)
    }

    pub fn is_watch(&self, key: &KeyEvent) -> bool {
        self.watch.matches(key)
    }

    pub fn is_paste(&self, key: &KeyEvent) -> bool {
        self.paste.matches(key)
    }

    pub fn stash_name(&self) -> String {
        self.stash.display()
    }

    pub fn scroller_name(&self) -> String {
        self.scroller.display()
    }

    pub fn editor_name(&self) -> String {
        self.editor.display()
    }

    pub fn history_name(&self) -> String {
        self.history.display()
    }

    pub fn trail_name(&self) -> String {
        self.trail.display()
    }

    pub fn watch_name(&self) -> String {
        self.watch.display()
    }

    pub fn paste_name(&self) -> String {
        self.paste.display()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hyphen_and_plus_delimiters() {
        assert_eq!(KeyChord::parse("ctrl-s"), Some(KeyChord::ctrl('s')));
        assert_eq!(KeyChord::parse("ctrl+s"), Some(KeyChord::ctrl('s')));
        assert_eq!(KeyChord::parse("CTRL+S"), Some(KeyChord::ctrl('s')));
        assert_eq!(KeyChord::parse("alt-o"), Some(KeyChord::alt('o')));
        assert_eq!(KeyChord::parse("meta+p"), Some(KeyChord::alt('p')));
        assert_eq!(KeyChord::parse("opt-o"), Some(KeyChord::alt('o')));
    }

    #[test]
    fn reserved_keys_are_rejected() {
        assert!(KeyChord::ctrl('c').is_reserved());
        assert!(KeyChord::new(KeyCode::Enter, KeyModifiers::NONE).is_reserved());
        assert!(KeyChord::new(KeyCode::Esc, KeyModifiers::NONE).is_reserved());

        let mut map = BTreeMap::new();
        map.insert("stash".to_string(), "ctrl-c".to_string());
        let bindings = Keybindings::from_map(&map);
        // Should fall back to default ctrl-s
        assert_eq!(bindings.stash, KeyChord::ctrl('s'));
    }

    /// A key the box already answers is not an action's to take. Every one of these parses, so what
    /// refuses them is the rule about which chords are on offer rather than the parser.
    #[test]
    fn a_key_the_box_already_answers_is_not_on_offer() {
        for spelling in [
            "x",            // a letter is typed into the line
            "9",            // so is a digit
            "shift-x",      // a shifted character arrives shifted, so this would never match
            "ctrl-shift-x", // nor would this
            "tab",          // takes what is offered
            "shift-tab",    // cycles what to ask before acting
            "up",           // walks the history
            "pgdn",         // scrolls the transcript
            "esc",          // clears the line
            "enter",        // sends
            "shift-enter",  // starts a line
            "ctrl-j",       // starts a line as well
            "ctrl-c",       // stops, then leaves
            "ctrl-d",       // leaves
        ] {
            let chord = KeyChord::parse(spelling).expect("the spelling should parse");
            assert!(chord.is_reserved(), "{spelling} was offered up");

            let mut map = BTreeMap::new();
            map.insert("stash".to_string(), spelling.to_string());
            assert_eq!(
                Keybindings::from_map(&map).stash,
                KeyChord::ctrl('s'),
                "{spelling} took the stash chord"
            );
        }
    }

    /// The chords a person would reach for instead are on offer, including the two the issue is
    /// about: Ctrl-S is XOFF on a terminal with flow control left on, and Ctrl-O discards output.
    #[test]
    fn a_chord_carrying_ctrl_or_alt_is_on_offer() {
        for spelling in [
            "ctrl-x",
            "alt-s",
            "alt-o",
            "ctrl-alt-p",
            "alt-f5",
            "ctrl-pgup",
        ] {
            let chord = KeyChord::parse(spelling).expect("the spelling should parse");
            assert!(!chord.is_reserved(), "{spelling} was refused");
        }
    }

    #[test]
    fn custom_chords_override_defaults() {
        let mut map = BTreeMap::new();
        map.insert("stash".to_string(), "alt-s".to_string());
        map.insert("scroller".to_string(), "alt-o".to_string());
        let bindings = Keybindings::from_map(&map);

        assert_eq!(bindings.stash, KeyChord::alt('s'));
        assert_eq!(bindings.scroller, KeyChord::alt('o'));
        assert_eq!(bindings.stash_name(), "alt-s");
        assert_eq!(bindings.scroller_name(), "alt-o");
        // Others retain default
        assert_eq!(bindings.editor, KeyChord::ctrl('g'));
    }

    #[test]
    fn conflicting_chords_fall_back_to_defaults() {
        let mut map = BTreeMap::new();
        map.insert("stash".to_string(), "alt-x".to_string());
        map.insert("scroller".to_string(), "alt-x".to_string());
        let bindings = Keybindings::from_map(&map);

        assert_eq!(bindings.stash, KeyChord::alt('x'));
        // scroller conflicted with stash so falls back to default ctrl-o
        assert_eq!(bindings.scroller, KeyChord::ctrl('o'));
    }

    #[test]
    fn invalid_chord_falls_back_to_default() {
        let mut map = BTreeMap::new();
        map.insert("stash".to_string(), "not-a-valid-chord".to_string());
        let bindings = Keybindings::from_map(&map);

        assert_eq!(bindings.stash, KeyChord::ctrl('s'));
    }
}
