//! The keys of the Sessions screen that a settings file can move (SCREEN-14).
//!
//! A `sessionsKeys` block names an action and the key it is to answer. The keys here are the
//! screen's own, read while the cursor is on the list: a letter typed into a search or a rename is
//! text whatever is bound, so a bare character does not take a letter out of anything that is typed.
//! The keys the screen keeps for itself (Enter, Escape, the arrows, Tab, the digits that choose a
//! tab) are not on offer.

use crate::keybindings::{KeyChord, settle};
use crate::render::printable;
use bravebot_i18n::t;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::collections::BTreeMap;

/// What a key of the Sessions screen does, by the name a settings file calls it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Up,
    Down,
    New,
    Fork,
    Rename,
    Stop,
    Archive,
    Delete,
    Hide,
    Pin,
    Group,
    Search,
    Help,
}

impl Action {
    /// Every action that can be moved, in the order the table keeps them.
    pub const ALL: [Self; 13] = [
        Self::Up,
        Self::Down,
        Self::New,
        Self::Fork,
        Self::Rename,
        Self::Stop,
        Self::Archive,
        Self::Delete,
        Self::Hide,
        Self::Pin,
        Self::Group,
        Self::Search,
        Self::Help,
    ];

    /// The name a settings file calls the action.
    pub fn word(self) -> &'static str {
        match self {
            Self::Up => "up",
            Self::Down => "down",
            Self::New => "new",
            Self::Fork => "fork",
            Self::Rename => "rename",
            Self::Stop => "stop",
            Self::Archive => "archive",
            Self::Delete => "delete",
            Self::Hide => "hide",
            Self::Pin => "pin",
            Self::Group => "group",
            Self::Search => "search",
            Self::Help => "help",
        }
    }

    /// The key the action answers unless a settings file moves it.
    pub fn default_chord(self) -> KeyChord {
        fn plain(c: char) -> KeyChord {
            KeyChord::new(KeyCode::Char(c), KeyModifiers::NONE)
        }
        match self {
            Self::Up => plain('k'),
            Self::Down => plain('j'),
            Self::New => plain('n'),
            Self::Fork => plain('f'),
            Self::Rename => plain('r'),
            Self::Stop => plain('x'),
            Self::Archive => plain('a'),
            Self::Delete => KeyChord::new(KeyCode::Backspace, KeyModifiers::NONE),
            Self::Hide => plain('h'),
            Self::Pin => plain('p'),
            Self::Group => plain('g'),
            Self::Search => plain('/'),
            Self::Help => plain('?'),
        }
    }

    fn index(self) -> usize {
        Self::ALL
            .iter()
            .position(|action| *action == self)
            .unwrap_or(0)
    }
}

/// Why a request in the block was not taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Why {
    /// The name is not one of the actions above.
    NoSuchAction,
    /// The spelling is not a key: it names nothing, or an event a terminal never reports (Shift over
    /// a character, Ctrl and Alt together).
    Unreadable,
    /// The key is one the screen keeps for itself.
    Kept,
    /// Another action stands on the key.
    Contested,
}

/// One request that was not taken, and the action it leaves where it was.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refused {
    /// The name as written, pictured.
    pub name: String,
    /// The spelling as written, pictured.
    pub spelling: String,
    pub why: Why,
    pub action: Option<Action>,
}

/// The keys the Sessions screen answers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionsKeys {
    chords: [KeyChord; Action::ALL.len()],
}

impl Default for SessionsKeys {
    fn default() -> Self {
        Self {
            chords: Action::ALL.map(Action::default_chord),
        }
    }
}

/// The most of a name or a spelling said back in a sentence.
const SAID: usize = 24;

impl SessionsKeys {
    /// Read a block of action names and key spellings, leaving every action on a key of its own.
    ///
    /// A request that cannot be read, that names a key the screen keeps, or that names no action
    /// leaves things as they were. So does a key two actions would answer, and both requests are
    /// given up, as for the input box's chords ([INPUT-32](terminal-input.md#INPUT-32)); two
    /// actions trading keys are not a contest. What was refused comes back with the table, so that
    /// it can be said.
    pub fn from_map(configured: &BTreeMap<String, String>) -> (Self, Vec<Refused>) {
        let defaults: [KeyChord; Action::ALL.len()] = Action::ALL.map(Action::default_chord);
        let mut asked: [Option<KeyChord>; Action::ALL.len()] = [None; Action::ALL.len()];
        let mut refused = Vec::new();
        for (name, spelling) in configured {
            let said = |why, action| Refused {
                name: said_of(name),
                spelling: said_of(spelling),
                why,
                action,
            };
            let Some(action) = Action::ALL.into_iter().find(|action| action.word() == name) else {
                refused.push(said(Why::NoSuchAction, None));
                continue;
            };
            match read(spelling) {
                Ok(chord) => asked[action.index()] = Some(chord),
                Err(why) => refused.push(said(why, Some(action))),
            }
        }
        let wanted = asked;
        settle(&defaults, &mut asked);
        for action in Action::ALL {
            if wanted[action.index()].is_some() && asked[action.index()].is_none() {
                refused.push(Refused {
                    name: said_of(action.word()),
                    spelling: configured
                        .get(action.word())
                        .map(|spelling| said_of(spelling))
                        .unwrap_or_default(),
                    why: Why::Contested,
                    action: Some(action),
                });
            }
        }
        let keys = Self {
            chords: std::array::from_fn(|i| asked[i].unwrap_or(defaults[i])),
        };
        (keys, refused)
    }

    /// The key an action answers now.
    pub fn chord(&self, action: Action) -> KeyChord {
        self.chords[action.index()]
    }

    /// Whether a settings file moved the action off its default.
    pub fn is_moved(&self, action: Action) -> bool {
        self.chord(action) != action.default_chord()
    }

    /// The name of the key an action answers, as the footer and the list `?` write it.
    pub fn name(&self, action: Action) -> String {
        self.chord(action).display()
    }

    /// Whether this press is the key a moved action answers.
    fn is(&self, action: Action, key: &KeyEvent) -> bool {
        let chord = self.chord(action);
        key.code == chord.code && key.modifiers.difference(KeyModifiers::SHIFT) == chord.modifiers
    }

    /// What the screen's own handling should read in place of this press.
    ///
    /// A key a settings file moved an action onto is read as the key that action answers by
    /// default, so the screen has one place that says what each action does. The key such an action
    /// was moved off does nothing. Every other press is read as it came.
    pub fn translate(&self, key: &KeyEvent) -> Option<KeyEvent> {
        for action in Action::ALL {
            if self.is_moved(action) && self.is(action, key) {
                let default = action.default_chord();
                return Some(KeyEvent::new(default.code, default.modifiers));
            }
        }
        let left = Action::ALL.into_iter().any(|action| {
            if !self.is_moved(action) {
                return false;
            }
            let default = action.default_chord();
            let delete = action == Action::Delete && key.code == KeyCode::Delete;
            delete
                || (key.code == default.code
                    && key.modifiers.difference(KeyModifiers::SHIFT) == default.modifiers)
        });
        match left {
            true => None,
            false => Some(*key),
        }
    }

    /// Whether this press is the key the help action answers.
    pub fn is_help(&self, key: &KeyEvent) -> bool {
        self.is(Action::Help, key)
    }

    /// The sentence that says what was refused and where the action stays.
    pub fn sentence(&self, refused: &Refused) -> String {
        let name = refused.name.as_str();
        let spelling = refused.spelling.as_str();
        match (refused.why, refused.action) {
            (Why::NoSuchAction, _) | (_, None) => {
                t!(sessions_screen_keys_no_such_action, name = name)
            }
            (why, Some(action)) => {
                let key = self.name(action);
                match why {
                    Why::Unreadable => t!(
                        sessions_screen_keys_unreadable,
                        name = name,
                        spelling = spelling,
                        key = key
                    ),
                    Why::Kept => t!(
                        sessions_screen_keys_kept,
                        name = name,
                        spelling = spelling,
                        key = key
                    ),
                    _ => t!(
                        sessions_screen_keys_contested,
                        name = name,
                        spelling = spelling,
                        key = key
                    ),
                }
            }
        }
    }
}

/// A name or a spelling as it can be said: pictured, and cut where it is long.
fn said_of(text: &str) -> String {
    printable(text).chars().take(SAID).collect()
}

/// Read the spelling of a key, or say why it is not on offer.
///
/// A character, alone or with Ctrl or Alt, and a function key. The digits that choose a tab, the
/// keys that move the cursor by a chord, Ctrl-C and F3 are the screen's and are not on offer, and
/// neither is a key that is not a character: Enter, Escape, Tab, the arrows and the rest.
fn read(spelling: &str) -> Result<KeyChord, Why> {
    let mut single = spelling.trim().chars();
    let chord = match (single.next(), single.next()) {
        (Some(only), None) => {
            KeyChord::new(KeyCode::Char(only.to_ascii_lowercase()), KeyModifiers::NONE)
        }
        _ => KeyChord::parse(spelling).ok_or(Why::Unreadable)?,
    };
    let modifiers = chord.modifiers;
    match chord.code {
        KeyCode::Char(c) => {
            let carried = modifiers == KeyModifiers::NONE
                || modifiers == KeyModifiers::CONTROL
                || modifiers == KeyModifiers::ALT;
            if !carried || c.is_control() {
                return Err(Why::Unreadable);
            }
            let kept = (modifiers == KeyModifiers::NONE && ('1'..='5').contains(&c))
                || (modifiers == KeyModifiers::CONTROL && matches!(c, 'p' | 'n' | 'b' | 'f' | 'c'));
            match kept {
                true => Err(Why::Kept),
                false => Ok(chord),
            }
        }
        KeyCode::F(n) => {
            if modifiers != KeyModifiers::NONE || !(1..=12).contains(&n) {
                Err(Why::Unreadable)
            } else if n == 3 {
                Err(Why::Kept)
            } else {
                Ok(chord)
            }
        }
        _ => Err(Why::Kept),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(name, key)| ((*name).to_string(), (*key).to_string()))
            .collect()
    }

    fn press(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, modifiers)
    }

    /// SCREEN-14. With nothing asked for, every action answers the key it always has.
    #[test]
    fn the_defaults_are_the_keys_the_screen_has_always_had() {
        let (keys, refused) = SessionsKeys::from_map(&BTreeMap::new());
        assert!(refused.is_empty());
        let names: Vec<String> = Action::ALL.into_iter().map(|a| keys.name(a)).collect();
        assert_eq!(
            names,
            [
                "k",
                "j",
                "n",
                "f",
                "r",
                "x",
                "a",
                "backspace",
                "h",
                "p",
                "g",
                "/",
                "?"
            ]
        );
        assert!(Action::ALL.into_iter().all(|a| !keys.is_moved(a)));
    }

    /// SCREEN-14. A character, a Ctrl or Alt character and a function key are on offer.
    #[test]
    fn a_character_a_ctrl_or_alt_character_and_a_function_key_can_be_asked_for() {
        let (keys, refused) = SessionsKeys::from_map(&map(&[
            ("stop", "z"),
            ("archive", "ctrl-e"),
            ("hide", "alt-h"),
            ("pin", "f5"),
            ("fork", "-"),
            ("help", "ctrl+/"),
        ]));
        assert!(refused.is_empty(), "{refused:?}");
        assert_eq!(keys.name(Action::Stop), "z");
        assert_eq!(keys.name(Action::Archive), "ctrl-e");
        assert_eq!(keys.name(Action::Hide), "alt-h");
        assert_eq!(keys.name(Action::Pin), "f5");
        assert_eq!(keys.name(Action::Fork), "-");
        assert_eq!(keys.name(Action::Help), "ctrl-/");
    }

    /// SCREEN-14. A key the screen keeps is refused, and the action stays where it was.
    #[test]
    fn a_key_the_screen_keeps_is_refused_and_the_action_stays() {
        for kept in [
            "enter", "esc", "tab", "up", "pgdn", "home", "3", "ctrl-c", "ctrl-p", "ctrl-n", "f3",
            "backtab",
        ] {
            let (keys, refused) = SessionsKeys::from_map(&map(&[("stop", kept)]));
            assert_eq!(keys.name(Action::Stop), "x", "{kept} was taken");
            assert_eq!(refused.len(), 1, "{kept}");
            assert_eq!(refused[0].why, Why::Kept, "{kept}");
            assert_eq!(refused[0].action, Some(Action::Stop));
        }
    }

    /// SCREEN-14. A spelling that names no key, or an event a terminal never reports, is refused.
    #[test]
    fn a_spelling_nothing_can_read_is_refused_and_the_action_stays() {
        for unread in ["banana", "ctrl-", "shift-x", "ctrl-alt-x", "f13", "alt-f5"] {
            let (keys, refused) = SessionsKeys::from_map(&map(&[("archive", unread)]));
            assert_eq!(keys.name(Action::Archive), "a", "{unread} was taken");
            assert_eq!(refused.len(), 1, "{unread}");
            assert_eq!(refused[0].why, Why::Unreadable, "{unread}");
        }
    }

    /// SCREEN-14. A name that is no action is refused and changes nothing.
    #[test]
    fn a_name_that_is_no_action_is_refused() {
        let (keys, refused) = SessionsKeys::from_map(&map(&[("explode", "z")]));
        assert_eq!(keys, SessionsKeys::default());
        assert_eq!(refused.len(), 1);
        assert_eq!(refused[0].why, Why::NoSuchAction);
        assert_eq!(refused[0].action, None);
    }

    /// SCREEN-14. Two actions asking for one key both stay, and so does one asking for the key
    /// another stands on; two actions trading keys is no contest.
    #[test]
    fn a_contested_key_is_given_up_and_a_trade_is_not() {
        let (keys, refused) = SessionsKeys::from_map(&map(&[("stop", "z"), ("pin", "z")]));
        assert_eq!(keys.name(Action::Stop), "x");
        assert_eq!(keys.name(Action::Pin), "p");
        assert_eq!(refused.len(), 2);
        assert!(refused.iter().all(|r| r.why == Why::Contested));

        let (keys, refused) = SessionsKeys::from_map(&map(&[("stop", "a")]));
        assert_eq!(
            keys.name(Action::Stop),
            "x",
            "a key another action stands on was taken"
        );
        assert_eq!(keys.name(Action::Archive), "a");
        assert_eq!(refused.len(), 1);

        let (keys, refused) = SessionsKeys::from_map(&map(&[("stop", "a"), ("archive", "x")]));
        assert!(refused.is_empty(), "{refused:?}");
        assert_eq!(keys.name(Action::Stop), "a");
        assert_eq!(keys.name(Action::Archive), "x");
    }

    /// SCREEN-14. A key a moved action answers is read as that action's default key, and the key it
    /// was moved off does nothing; every other press is read as it came.
    #[test]
    fn a_moved_key_is_read_as_the_action_and_the_old_key_does_nothing() {
        let (keys, _) = SessionsKeys::from_map(&map(&[("archive", "ctrl-e"), ("delete", "d")]));
        let none = KeyModifiers::NONE;
        assert_eq!(
            keys.translate(&press(KeyCode::Char('e'), KeyModifiers::CONTROL)),
            Some(press(KeyCode::Char('a'), none))
        );
        assert_eq!(keys.translate(&press(KeyCode::Char('a'), none)), None);
        assert_eq!(
            keys.translate(&press(KeyCode::Char('d'), none)),
            Some(press(KeyCode::Backspace, none))
        );
        assert_eq!(keys.translate(&press(KeyCode::Backspace, none)), None);
        assert_eq!(keys.translate(&press(KeyCode::Delete, none)), None);
        // An action that was not moved, and a key the screen keeps, are read as they came.
        let stop = press(KeyCode::Char('x'), none);
        assert_eq!(keys.translate(&stop), Some(stop));
        let enter = press(KeyCode::Enter, none);
        assert_eq!(keys.translate(&enter), Some(enter));
        let shifted = press(KeyCode::Char('?'), KeyModifiers::SHIFT);
        assert_eq!(keys.translate(&shifted), Some(shifted));
    }

    /// SCREEN-14. The sentence for a refusal names the action, the spelling and the key it stays on.
    #[test]
    fn a_refusal_says_the_action_the_spelling_and_the_key_it_stays_on() {
        let (keys, refused) = SessionsKeys::from_map(&map(&[("stop", "enter"), ("explode", "z")]));
        let said: Vec<String> = refused.iter().map(|r| keys.sentence(r)).collect();
        assert!(
            said.iter()
                .any(|s| s.contains("stop") && s.contains("enter") && s.contains('x')),
            "{said:?}"
        );
        assert!(said.iter().any(|s| s.contains("explode")), "{said:?}");
    }

    /// SCREEN-14. A name or a spelling written with a control character is said pictured and cut.
    #[test]
    fn what_is_said_back_is_pictured_and_cut() {
        let long = format!("\u{1b}[31m{}", "z".repeat(80));
        let (keys, refused) = SessionsKeys::from_map(&map(&[("stop", &long)]));
        let sentence = keys.sentence(&refused[0]);
        assert!(!sentence.contains('\u{1b}'), "{sentence:?}");
        assert!(sentence.chars().count() < 200, "{sentence:?}");
    }
}
