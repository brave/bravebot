//! Choosing how the input box edits text, and whether the daily usage ping is sent.
//!
//! Shown by `/config`. Two questions so far. The first is whether the box edits the way vi does, and
//! the choice is written to `~/.bravebot`: it outlives the session that made it and applies in every
//! directory, the same as the model, the theme and the effort level. The second is whether the
//! once-a-day usage ping goes, and that choice is written to `~/.bravebot/usage-ping`.
//!
//! Drawn as a centred panel over the session, so the transcript stays visible. Enter takes the row
//! under the cursor. Escape leaves what was in force alone.
//!
//! Nothing labelled is involved. The words are drawn for a person, who picks one, and what their pick
//! changes is which keys move a caret in a box they are typing into, or whether one request a day is
//! made.
//!
//! # A ping turned off elsewhere
//!
//! Where the ping was turned off by something stronger than this switch, a variable, a settings file,
//! the administrator or an incognito session, the panel shows one row saying it is off and what turned
//! it off, in place of the two it could otherwise choose between. Offering `on` there would be a row
//! that does nothing when picked.
//!
//! # Why the panel exists
//!
//! Because the alternative is a command per preference: `/theme` and `/effort` each took a word of the
//! command space for one choice. A panel is where the next ones go without another verb being
//! invented, and it is the surface somebody coming from Claude Code already looks for.

use bravebot_config::usage_ping::UsagePing;
use bravebot_i18n::t;
use ratatui::Frame;
use ratatui::Terminal;
use ratatui::backend::Backend;

use crate::list_picker::{self, ListPicker, Panel, PickerRow};
use crate::vim::Editing;

/// One row of the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Row {
    /// A style of editing to choose.
    Editing(Editing),
    /// Turn the usage ping on, or off.
    UsagePing(bool),
    /// The usage ping is off for a reason this switch cannot change. Picking it changes nothing.
    UsagePingLocked(UsagePing),
    /// What is in force, never drawn: the picker marks one row as current, and this panel has a
    /// current answer to each of its questions.
    InForce { editing: Editing, usage_ping: bool },
}

impl Row {
    /// Every row, editing first, the ordinary box first among those since it is what most people are
    /// already using.
    fn all(usage_ping: UsagePing) -> Vec<Row> {
        let mut rows: Vec<Row> = Editing::ALL.into_iter().map(Row::Editing).collect();
        if usage_ping.person_may_change() {
            rows.extend([Row::UsagePing(true), Row::UsagePing(false)]);
        } else {
            rows.push(Row::UsagePingLocked(usage_ping));
        }
        rows
    }

    /// The words shown for this row.
    ///
    /// An editing style is the word the setting is spelled with, untranslated: it appears in a
    /// settings file and in the documentation of the tools this borrows the name from, so somebody
    /// comparing what they picked against what they wrote in a file is comparing the same word.
    fn name(self) -> String {
        match self {
            Row::Editing(editing) => editing.as_str().to_string(),
            Row::UsagePing(true) => t!(config_usage_ping_on).to_string(),
            Row::UsagePing(false) => t!(config_usage_ping_off).to_string(),
            Row::UsagePingLocked(why) => t!(config_usage_ping_locked, reason = reason(why)),
            Row::InForce { .. } => String::new(),
        }
    }

    /// What this row means, in one line under the list.
    ///
    /// A match rather than a lookup, so every sentence is named in the source: nothing computed while
    /// running can choose which of them a person is shown.
    fn hint(self) -> &'static str {
        match self {
            Row::Editing(Editing::Ordinary) => t!(config_editing_hint_ordinary),
            Row::Editing(Editing::Vi) => t!(config_editing_hint_vi),
            Row::UsagePing(true) => t!(config_usage_ping_hint_on),
            Row::UsagePing(false) => t!(config_usage_ping_hint_off),
            Row::UsagePingLocked(_) => t!(config_usage_ping_hint_locked),
            Row::InForce { .. } => "",
        }
    }
}

/// What turned the ping off, in the words the locked row is drawn with.
///
/// Each names the one place a person would have to go to change it. `On` and `OffByUser` are never
/// locked, and are given the words for the switch itself only so the match is complete.
pub fn reason(usage_ping: UsagePing) -> String {
    match usage_ping {
        UsagePing::OffByEnvironment(variable) => {
            t!(config_usage_ping_reason_environment, variable = variable)
        }
        UsagePing::OffBySetting => t!(config_usage_ping_reason_setting).to_string(),
        UsagePing::OffByAdmin => t!(config_usage_ping_reason_admin).to_string(),
        UsagePing::Incognito => t!(config_usage_ping_reason_incognito).to_string(),
        UsagePing::On | UsagePing::OffByUser => t!(config_usage_ping_off).to_string(),
    }
}

impl PickerRow for Row {
    fn name(&self) -> String {
        Row::name(*self)
    }

    fn hint(&self) -> Option<&'static str> {
        Some(Row::hint(*self))
    }

    fn same_as(&self, other: &Self) -> bool {
        match (self, other) {
            (Row::Editing(editing), Row::InForce { editing: held, .. }) => editing == held,
            (Row::UsagePing(on), Row::InForce { usage_ping, .. }) => on == usage_ping,
            _ => self == other,
        }
    }
}

/// What the picker is showing and where the cursor is.
pub type Picker = ListPicker<Row>;

impl Picker {
    /// Open on the style in use, since that is the row a person is looking for.
    pub fn new(current: Editing, usage_ping: UsagePing) -> Self {
        let rows = Row::all(usage_ping);
        let previous = Row::InForce {
            editing: current,
            usage_ping: usage_ping.is_on(),
        };
        let selected = rows
            .iter()
            .position(|row| *row == Row::Editing(current))
            .unwrap_or(0);
        Self::open(rows, selected, previous)
    }
}

/// Where [`draw`] draws the panel in a frame of `frame`.
pub fn placed(frame: ratatui::layout::Rect, picker: &Picker) -> list_picker::Placed {
    list_picker::placed(frame, picker, &panel())
}

/// Draw the panel over whatever is already in the frame.
pub fn draw(frame: &mut Frame, picker: &Picker) {
    list_picker::draw(frame, picker, &panel());
}

/// Show the list as a centred panel over whatever `behind` draws, and return the row the person
/// picked, or `None` if they kept what was in force.
///
/// `behind` is the session already on screen. Drawn first each frame so Escape leaves the person
/// looking at the same session they opened the picker from.
pub fn choose<B: Backend>(
    terminal: &mut Terminal<B>,
    current: Editing,
    usage_ping: UsagePing,
    wheel: u16,
    behind: impl FnMut(&mut Frame),
) -> Option<Row> {
    list_picker::choose(
        terminal,
        Picker::new(current, usage_ping),
        &panel(),
        wheel,
        behind,
    )
}

fn panel() -> Panel {
    Panel {
        title: t!(config_picker_title),
        keys: t!(config_picker_keys),
        width: 64,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bravebot_config::env_var;

    /// Every state the switch can be in that it cannot change.
    const LOCKED: [UsagePing; 4] = [
        UsagePing::OffByEnvironment(env_var::USAGE_PING),
        UsagePing::OffBySetting,
        UsagePing::OffByAdmin,
        UsagePing::Incognito,
    ];

    /// The picker opens on the style in force, since that is the row somebody is looking for: opening
    /// on the first row would make the list say the ordinary box was chosen when it was not.
    #[test]
    fn the_picker_opens_on_the_style_in_force() {
        let picker = Picker::new(Editing::Vi, UsagePing::On);
        assert_eq!(picker.chosen(), Some(&Row::Editing(Editing::Vi)));
    }

    /// Every row says what it means, so the panel is legible to somebody who does not already know
    /// what `vim` names here, or what a usage ping sends. A row with nothing under it would be a word
    /// and a guess.
    #[test]
    fn every_row_says_what_it_means() {
        for state in [UsagePing::On, UsagePing::OffByUser]
            .into_iter()
            .chain(LOCKED)
        {
            for row in Row::all(state) {
                assert!(!row.hint().is_empty(), "{} has no hint", row.name());
            }
        }
    }

    fn rendered(picker: &Picker) -> String {
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 24)).expect("terminal");
        terminal
            .draw(|frame| list_picker::draw(frame, picker, &panel()))
            .expect("draw succeeds");
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect()
    }

    /// The words drawn are the words the setting is spelled with, so somebody comparing what they
    /// picked against what they wrote in a file is reading the same word.
    #[test]
    fn the_list_shows_the_words_the_setting_is_spelled_with() {
        let output = rendered(&Picker::new(Editing::Ordinary, UsagePing::On));
        for style in Editing::ALL {
            assert!(output.contains(style.as_str()), "{output}");
        }
    }

    /// The style in force is marked on the screen and not only in the state, since the panel is where
    /// somebody goes to find out which one it is.
    #[test]
    fn the_style_in_force_is_drawn_as_current() {
        let output = rendered(&Picker::new(Editing::Vi, UsagePing::On));
        assert!(output.contains("vim  current"), "{output}");
    }

    /// Whether the ping is on is marked as current beside the editing style, since one panel answers
    /// both questions and a person reading it needs the answer to each.
    #[test]
    fn whether_the_ping_is_on_is_drawn_as_current() {
        let on = rendered(&Picker::new(Editing::Vi, UsagePing::On));
        let wanted = format!("{}  current", t!(config_usage_ping_on));
        assert!(on.contains(&wanted), "{on}");

        let off = rendered(&Picker::new(Editing::Vi, UsagePing::OffByUser));
        let wanted = format!("{}  current", t!(config_usage_ping_off));
        assert!(off.contains(&wanted), "{off}");
        assert!(off.contains("vim  current"), "{off}");
    }

    /// A ping turned off by something stronger than the switch is shown as off with what turned it
    /// off, and no row offers to turn it on: a row that did nothing when picked would tell the person
    /// they had turned it on when they had not.
    #[test]
    fn a_ping_turned_off_elsewhere_is_shown_with_its_reason_and_offers_no_switch() {
        let reasons = [
            "set by BRAVEBOT_USAGE_PING",
            "usagePing in ~/.bravebot/settings.json",
            "managed by your administrator",
            "incognito",
        ];
        for (state, words) in LOCKED.into_iter().zip(reasons) {
            let picker = Picker::new(Editing::Ordinary, state);
            let rows: Vec<Row> = Row::all(state);
            assert!(
                !rows.iter().any(|row| matches!(row, Row::UsagePing(_))),
                "{state:?} offers a switch: {rows:?}"
            );
            assert!(rows.contains(&Row::UsagePingLocked(state)), "{state:?}");
            let output = rendered(&picker);
            assert!(output.contains(words), "{state:?}: {output}");
        }
    }
}
