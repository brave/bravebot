//! Reading a credential at the terminal without drawing it.
//!
//! A key typed at a prompt that echoes it is on the screen for anybody behind the person, and in the
//! terminal's scrollback after the command ends. Raw mode turns the terminal's echo off on every
//! platform the terminal backend supports, so nothing typed is drawn, and the keys are read one at a
//! time with the rules below.

use bravebot_config::Secret;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::crossterm::terminal;
use std::io::Write;

/// The room made for a key before the first one is typed, which a gateway key is far shorter than,
/// so one is not moved as it is typed.
const ROOM: usize = 4096;

/// Ask `question` on stderr and read the answer with nothing drawn as it is typed.
///
/// `None` where the person stopped, with Escape, Ctrl-C or Ctrl-D, or pressed Enter on nothing.
/// Ctrl-C has to be read here: raw mode is what keeps it from reaching the process as a signal.
pub fn read(question: &str) -> std::io::Result<Option<Secret>> {
    let mut stderr = std::io::stderr();
    write!(stderr, "{question} ")?;
    stderr.flush()?;
    let answer = {
        let _raw = Raw::enable()?;
        let mut typed = Typed::new();
        loop {
            if let Event::Key(key) = event::read()?
                && let Some(answer) = typed.answer_to(key)
            {
                break answer;
            }
        }
    };
    writeln!(stderr)?;
    Ok(answer)
}

/// Raw mode for as long as this is held, left however the read ends, a panic included.
struct Raw;

impl Raw {
    fn enable() -> std::io::Result<Self> {
        terminal::enable_raw_mode()?;
        Ok(Self)
    }
}

impl Drop for Raw {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
    }
}

/// What one key did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    Typing,
    Done,
    Stopped,
}

/// What has been typed so far, overwritten when it goes.
struct Typed {
    text: String,
}

impl Typed {
    fn new() -> Self {
        Self {
            text: String::with_capacity(ROOM),
        }
    }

    /// Apply one key.
    ///
    /// A release is passed over, since Windows reports one for every press and taking both would
    /// type each character twice. A character with Ctrl held is a command rather than part of a
    /// key, so it is never added, except with Alt held too: that is how Windows reports AltGr,
    /// which types `@`, `{` or `\` on many layouts.
    fn press(&mut self, key: KeyEvent) -> Step {
        if key.kind == KeyEventKind::Release {
            return Step::Typing;
        }
        let control = key.modifiers.contains(KeyModifiers::CONTROL)
            && !key.modifiers.contains(KeyModifiers::ALT);
        match key.code {
            KeyCode::Enter => Step::Done,
            KeyCode::Esc => Step::Stopped,
            KeyCode::Char('c' | 'd') if control => Step::Stopped,
            KeyCode::Char('u') if control => {
                self.wipe();
                Step::Typing
            }
            KeyCode::Backspace => {
                self.text.pop();
                Step::Typing
            }
            KeyCode::Char(typed) if !control => {
                self.make_room(typed.len_utf8());
                self.text.push(typed);
                Step::Typing
            }
            _ => Step::Typing,
        }
    }

    /// Apply one key, and say what the read answers where it ends it: the key typed, or `None`
    /// where the person stopped.
    fn answer_to(&mut self, key: KeyEvent) -> Option<Option<Secret>> {
        match self.press(key) {
            Step::Typing => None,
            Step::Done => Some(self.secret()),
            Step::Stopped => Some(None),
        }
    }

    /// What was typed, without the space around it, or `None` where that is nothing.
    fn secret(&self) -> Option<Secret> {
        let key = self.text.trim();
        (!key.is_empty()).then(|| Secret::new(key))
    }

    /// Overwrite the whole allocation and start again in it.
    fn wipe(&mut self) {
        self.text = overwritten(std::mem::take(&mut self.text));
    }

    /// Move to an allocation twice the size where `more` bytes would not fit, overwriting the one
    /// left, since a `String` that grows by itself hands its old allocation back as it was.
    fn make_room(&mut self, more: usize) {
        if self.text.len() + more <= self.text.capacity() {
            return;
        }
        let mut larger = String::with_capacity((self.text.capacity() * 2).max(ROOM));
        larger.push_str(&self.text);
        drop(overwritten(std::mem::replace(&mut self.text, larger)));
    }
}

/// `text`'s allocation, empty, with every byte of it overwritten.
///
/// The whole of it rather than the part in use, because a character Backspace took off is still in
/// the bytes past the end.
fn overwritten(text: String) -> String {
    let mut bytes = text.into_bytes();
    let room = bytes.capacity();
    bytes.resize(room, 0);
    bravebot_config::scrub_bytes(&mut bytes);
    bytes.clear();
    // Empty, so this is valid text, and the allocation it is given back in is the one just
    // overwritten.
    String::from_utf8(bytes).unwrap_or_default()
}

impl Drop for Typed {
    fn drop(&mut self) {
        self.wipe();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn control(letter: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(letter), KeyModifiers::CONTROL)
    }

    /// What the read answers once the keys are typed, `None` where none of them ended it.
    fn typed(keys: impl IntoIterator<Item = KeyEvent>) -> Option<Option<String>> {
        let mut typed = Typed::new();
        keys.into_iter()
            .find_map(|key| typed.answer_to(key))
            .map(|answer| answer.map(|secret| secret.expose().to_string()))
    }

    fn characters(text: &str) -> Vec<KeyEvent> {
        text.chars().map(|c| key(KeyCode::Char(c))).collect()
    }

    /// CLI-18: Enter takes what was typed, Backspace takes off the last character, and the space a
    /// paste brings around a key is not part of it.
    #[test]
    fn enter_takes_what_was_typed_and_backspace_takes_one_off() {
        let mut keys = characters(" abcx");
        keys.push(key(KeyCode::Backspace));
        keys.extend(characters("d "));
        keys.push(key(KeyCode::Enter));
        assert_eq!(typed(keys), Some(Some("abcd".to_string())));
    }

    /// CLI-18: Escape, Ctrl-C and Ctrl-D stop without a key, and so does Enter on nothing. Raw mode
    /// keeps Ctrl-C from reaching the process as a signal, so a reader that took it as a character
    /// would leave nobody a way out but closing the terminal.
    #[test]
    fn escape_and_control_c_stop_without_a_key() {
        for stop in [key(KeyCode::Esc), control('c'), control('d')] {
            let mut keys = characters("abc");
            keys.push(stop);
            assert_eq!(typed(keys), Some(None), "{stop:?}");
        }
        assert_eq!(typed([key(KeyCode::Enter)]), Some(None));
        assert_eq!(
            typed(characters("abc")),
            None,
            "a key ended a read nobody ended"
        );
    }

    /// CLI-18: Windows reports a release for every press, so a reader that took both would store
    /// each character twice, and a key with a letter typed under Ctrl would be one nobody issued.
    #[test]
    fn a_release_and_a_control_letter_type_nothing() {
        let mut released = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
        released.kind = KeyEventKind::Release;
        let keys = [
            key(KeyCode::Char('a')),
            released,
            control('x'),
            KeyEvent::new(KeyCode::Char('B'), KeyModifiers::SHIFT),
            key(KeyCode::Enter),
        ];
        assert_eq!(typed(keys), Some(Some("aB".to_string())));
    }

    /// CLI-18: Windows reports AltGr as Ctrl and Alt held together, so a reader that took every
    /// Ctrl as a command would drop the `@` or `\` a key holds on many layouts and store the rest.
    #[test]
    fn a_character_typed_with_altgr_is_part_of_the_key() {
        let altgr = KeyModifiers::CONTROL | KeyModifiers::ALT;
        let keys = [
            key(KeyCode::Char('a')),
            KeyEvent::new(KeyCode::Char('@'), altgr),
            KeyEvent::new(KeyCode::Char('\\'), altgr),
            key(KeyCode::Char('b')),
            key(KeyCode::Enter),
        ];
        assert_eq!(typed(keys), Some(Some("a@\\b".to_string())));
    }

    /// CLI-18 and CRED-23: a key longer than the room made for it is kept whole as it moves to a
    /// larger allocation.
    #[test]
    fn a_key_longer_than_the_room_made_for_it_is_kept_whole() {
        let long: String = ('a'..='z').cycle().take(ROOM * 2 + 3).collect();
        let mut keys = characters(&long);
        keys.push(key(KeyCode::Enter));
        assert_eq!(typed(keys), Some(Some(long)));
    }

    /// CLI-18 and CRED-23: Ctrl-U starts the key again, which is what a terminal's own line editing
    /// does with it, in the allocation it overwrote rather than a new one.
    #[test]
    fn control_u_starts_the_key_again() {
        let mut typed_so_far = Typed::new();
        for c in "wrong".chars() {
            typed_so_far.press(key(KeyCode::Char(c)));
        }
        typed_so_far.press(key(KeyCode::Backspace));
        typed_so_far.press(control('u'));
        assert!(typed_so_far.text.is_empty());
        assert!(
            typed_so_far.text.capacity() >= ROOM,
            "the overwritten allocation was given away rather than kept"
        );
        for c in "right".chars() {
            typed_so_far.press(key(KeyCode::Char(c)));
        }
        assert_eq!(
            typed_so_far
                .secret()
                .map(|secret| secret.expose().to_string()),
            Some("right".to_string())
        );
    }
}
