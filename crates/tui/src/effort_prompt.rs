//! Choosing how hard the model thinks before it answers.
//!
//! Shown by `/effort`. The list is the levels the protocol defines plus the row that asks for
//! none. The picker is open in one of two scopes, which say how long a pick lasts: the level every
//! later session opens on, written to `~/.bravebot`, or the one session it is made in. The title and
//! the line under the list say which, and Tab switches between them (VIEW-46).
//!
//! Drawn as a centred panel over the session, so the transcript stays visible. Enter takes the row
//! under the cursor in the scope in force. Escape leaves the level that was in force alone.
//!
//! Nothing labelled is involved. The words never reach a model as a decision: they are drawn for a
//! person, who picks one, and their pick is what endorses the request field it lands in.

use bravebot_aichat::protocol::Effort;
use bravebot_i18n::t;
use ratatui::Frame;
use ratatui::Terminal;
use ratatui::backend::Backend;

use crate::list_picker::{self, ListPicker, Panel, PickerRow};
use crate::model_prompt::Scope;

/// One row of the list.
///
/// The row that asks for no level is offered because it is a choice somebody may want back: a
/// picker of five levels and no way out makes the first pick permanent, and the absent field is
/// what every service applies its own default to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Row(pub Option<Effort>);

impl Row {
    /// Every row, the service's own default first and then the levels cheapest first.
    fn all() -> Vec<Row> {
        std::iter::once(Row(None))
            .chain(Effort::ALL.into_iter().map(|level| Row(Some(level))))
            .collect()
    }

    /// The word shown for this row.
    ///
    /// A level is drawn as the word it is sent and stored as, untranslated: it is an identifier
    /// that appears in a settings file and in the request, and a person comparing what they picked
    /// against what a service documents is comparing the same word.
    fn name(self) -> String {
        match self.0 {
            Some(level) => level.as_str().to_string(),
            None => t!(effort_unset).to_string(),
        }
    }

    /// What this row is for, in one line under the list.
    ///
    /// A match rather than a lookup, so every sentence is named in the source: nothing computed
    /// while running can choose which of them a person is shown.
    fn hint(self) -> &'static str {
        match self.0 {
            None => t!(effort_hint_unset),
            Some(Effort::Low) => t!(effort_hint_low),
            Some(Effort::Medium) => t!(effort_hint_medium),
            Some(Effort::High) => t!(effort_hint_high),
            Some(Effort::Xhigh) => t!(effort_hint_xhigh),
            Some(Effort::Max) => t!(effort_hint_max),
        }
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
        self == other
    }
}

/// What the picker is showing and where the cursor is. It always has a scope: it opens in the one
/// that records the pick for every session, and [`ListPicker::starting_in`] opens it in the other.
pub type Picker = ListPicker<Row>;

impl Picker {
    /// Open on the level in use, since that is the row a person is looking for, in the scope that
    /// records the pick for every session.
    pub fn new(current: Option<Effort>) -> Self {
        let rows = Row::all();
        let previous = Row(current);
        let selected = rows.iter().position(|row| *row == previous).unwrap_or(0);
        Self::open(rows, selected, previous).starting_in(Scope::EverySession)
    }
}

/// Where [`draw`] draws the panel in a frame of `frame`.
pub fn placed(frame: ratatui::layout::Rect, picker: &Picker) -> list_picker::Placed {
    list_picker::placed(frame, picker, &panel(scope_of(picker)))
}

/// Draw the panel over whatever is already in the frame.
pub fn draw(frame: &mut Frame, picker: &Picker) {
    list_picker::draw(frame, picker, &panel(scope_of(picker)));
}

/// Show the list as a centred panel over whatever `behind` draws, open in `scope`, and return the
/// row the person picked with the scope in force when they took it, or `None` if they kept what was
/// in force.
///
/// `behind` is the session already on screen. Drawn first each frame so Escape leaves the person
/// looking at the same session they opened the picker from.
pub fn choose<B: Backend>(
    terminal: &mut Terminal<B>,
    current: Option<Effort>,
    scope: Scope,
    wheel: u16,
    behind: impl FnMut(&mut Frame),
) -> Option<(Row, Scope)> {
    let picker = Picker::new(current).starting_in(scope);
    list_picker::choose_with(
        terminal,
        picker,
        |scope| panel(scope.unwrap_or_default()),
        wheel,
        behind,
    )
    .map(|(row, scope)| (row, scope.unwrap_or_default()))
}

/// The scope `picker` is in. A picker made by [`Picker::new`] always has one.
fn scope_of(picker: &Picker) -> Scope {
    picker.scope().unwrap_or_default()
}

/// The panel for `scope`: a title naming it, and the line saying what Enter and Tab do in it.
fn panel(scope: Scope) -> Panel {
    let (title, scope_line) = match scope {
        Scope::EverySession => (
            t!(effort_picker_heading_every_session),
            t!(effort_picker_scope_every_session),
        ),
        Scope::ThisSession => (
            t!(effort_picker_heading_this_session),
            t!(effort_picker_scope_this_session),
        ),
    };
    Panel {
        title,
        keys: t!(effort_picker_keys),
        scope_line: Some(scope_line),
        width: 52,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::crossterm::event::{KeyCode, KeyModifiers};
    use ratatui::layout::Rect;

    #[test]
    fn the_picker_opens_on_the_level_in_use() {
        let picker = Picker::new(Some(Effort::Xhigh));
        assert_eq!(picker.chosen(), Some(&Row(Some(Effort::Xhigh))));
    }

    /// Somebody who has never chosen opens on the row that asks for no level, which is what their
    /// requests are already doing.
    #[test]
    fn having_chosen_nothing_opens_on_the_row_that_asks_for_none() {
        let picker = Picker::new(None);
        assert_eq!(picker.chosen(), Some(&Row(None)));
    }

    /// A first pick must not be permanent: the row that asks for no level is how somebody gets
    /// back to whatever the service does on its own.
    #[test]
    fn asking_for_no_level_is_one_of_the_rows() {
        let picker = Picker::new(Some(Effort::Max));
        assert!(picker.rows().contains(&Row(None)));
    }

    /// The text of every row of an 80x24 frame the picker is drawn in, joined.
    fn rendered(picker: &Picker) -> String {
        screen_at(picker, 80, 24).concat()
    }

    /// The text of each row of a `width` by `height` frame the picker is drawn in.
    fn screen_at(picker: &Picker, width: u16, height: u16) -> Vec<String> {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
        terminal
            .draw(|frame| draw(frame, picker))
            .expect("draw succeeds");
        terminal
            .backend()
            .buffer()
            .content()
            .chunks(usize::from(width))
            .map(|line| line.iter().map(|cell| cell.symbol()).collect())
            .collect()
    }

    /// The row of `screen` that holds `text`.
    fn row_holding(screen: &[String], text: &str) -> usize {
        screen
            .iter()
            .position(|row| row.contains(text))
            .unwrap_or_else(|| panic!("{text:?} is not drawn: {screen:#?}"))
    }

    /// A key that leaves the picker open.
    fn press(picker: &mut Picker, code: KeyCode, modifiers: KeyModifiers) {
        let outcome = list_picker::handle_key(picker, code, modifiers);
        assert_eq!(outcome, list_picker::Outcome::Continue, "{code:?}");
    }

    /// The words drawn are the words sent and stored, so somebody comparing what they picked
    /// against what a service documents is reading the same word.
    #[test]
    fn the_list_shows_the_words_a_request_carries() {
        let output = rendered(&Picker::new(None));
        for level in Effort::ALL {
            assert!(output.contains(level.as_str()), "{output}");
        }
    }

    #[test]
    fn the_level_in_use_is_marked() {
        let output = rendered(&Picker::new(Some(Effort::High)));
        assert!(output.contains("high  current"), "{output}");
    }

    /// Every row says what it is for. A list of five ranked words says nothing about which one to
    /// pick, and the whole point of the panel is that somebody can decide.
    #[test]
    fn the_row_under_the_cursor_says_what_it_is_for() {
        let output = rendered(&Picker::new(Some(Effort::Max)));
        assert!(output.contains("the most thinking, cost aside"), "{output}");
    }

    #[test]
    fn the_picker_is_drawn_as_a_centred_panel() {
        let output = rendered(&Picker::new(None));
        assert!(
            output.contains(t!(effort_picker_heading_every_session)),
            "{output}"
        );
        assert!(
            output.contains('╭') && output.contains('╮'),
            "no rounded border: {output}"
        );
    }

    #[test]
    fn the_panel_stays_inside_a_tiny_terminal() {
        for scope in [Scope::EverySession, Scope::ThisSession] {
            let picker = Picker::new(None).starting_in(scope);
            let mut terminal = Terminal::new(TestBackend::new(30, 8)).expect("terminal");
            terminal
                .draw(|frame| draw(frame, &picker))
                .expect("draw succeeds");
            let area = placed(Rect::new(0, 0, 30, 8), &picker).area;
            assert!(area.width <= 30, "{scope:?}");
            assert!(area.height <= 8, "{scope:?}");
            assert!(area.x + area.width <= 30, "{scope:?}");
            assert!(area.y + area.height <= 8, "{scope:?}");
        }
    }

    /// VIEW-46. Rejects: a picker opened with no scope named that is in the scope that records nothing,
    /// which would make `/effort` stop recording, and a `starting_in` that is ignored.
    #[test]
    fn a_picker_opens_in_the_every_session_scope_unless_told_otherwise() {
        assert_eq!(Picker::new(None).scope(), Some(Scope::EverySession));
        assert_eq!(
            Picker::new(Some(Effort::Low)).scope(),
            Some(Scope::EverySession)
        );
        let session_only = Picker::new(None).starting_in(Scope::ThisSession);
        assert_eq!(session_only.scope(), Some(Scope::ThisSession));
    }

    /// VIEW-46. Tab and Shift-Tab each switch the scope, in both directions from either scope,
    /// and move nothing else: the row under the cursor, the row marked `current` and the rows stay.
    ///
    /// Rejects: a Shift-Tab that does nothing, a Tab that stops at the second scope, a Tab that
    /// also moves the cursor, and one that changes the row marked `current` or the rows.
    #[test]
    fn tab_switches_the_scope_in_both_directions_and_touches_nothing_else() {
        for (start, other) in [
            (Scope::EverySession, Scope::ThisSession),
            (Scope::ThisSession, Scope::EverySession),
        ] {
            let mut picker = Picker::new(Some(Effort::High)).starting_in(start);
            press(&mut picker, KeyCode::Down, KeyModifiers::NONE);
            let cursor = Some(Row(Some(Effort::Xhigh)));
            let rows = picker.rows().to_vec();
            let untouched = |picker: &Picker| {
                assert_eq!(picker.chosen().copied(), cursor);
                assert_eq!(picker.previous(), &Row(Some(Effort::High)));
                assert_eq!(picker.rows(), rows.as_slice());
            };
            untouched(&picker);

            press(&mut picker, KeyCode::Tab, KeyModifiers::NONE);
            assert_eq!(picker.scope(), Some(other), "Tab from {start:?}");
            untouched(&picker);
            press(&mut picker, KeyCode::Tab, KeyModifiers::NONE);
            assert_eq!(picker.scope(), Some(start), "Tab back to {start:?}");
            untouched(&picker);
            press(&mut picker, KeyCode::BackTab, KeyModifiers::SHIFT);
            assert_eq!(picker.scope(), Some(other), "Shift-Tab from {start:?}");
            untouched(&picker);
            press(&mut picker, KeyCode::BackTab, KeyModifiers::SHIFT);
            assert_eq!(picker.scope(), Some(start), "Shift-Tab back");
            untouched(&picker);
        }
    }

    /// The scope a picker is in is told on the top border of its panel and on the first footer row,
    /// and the keys are on the row under the scope. The words of the other scope are drawn nowhere.
    fn assert_scope_is_drawn(screen: &[String], title: &str, footer: &str, absent: [&str; 2]) {
        let top = row_holding(screen, title);
        assert!(
            screen[top].contains('╭'),
            "the title is not on the border: {screen:#?}"
        );
        let at = row_holding(screen, footer);
        assert!(at > top, "the footer is not under the title: {screen:#?}");
        let keys = row_holding(screen, t!(effort_picker_keys));
        assert_eq!(
            keys,
            at + 1,
            "the keys are not under the scope: {screen:#?}"
        );
        for other in absent {
            assert!(
                screen.iter().all(|row| !row.contains(other)),
                "{other:?} is drawn: {screen:#?}"
            );
        }
    }

    /// VIEW-46. Rejects: a footer that does not say Enter records the pick for every session,
    /// one that names no Tab target, a title that is the same words in both scopes, and a scope
    /// drawn nowhere but the footer.
    #[test]
    fn a_picker_opened_by_bare_effort_draws_the_every_session_scope_in_title_and_footer() {
        let screen = screen_at(&Picker::new(None), 80, 24);
        assert_scope_is_drawn(
            &screen,
            t!(effort_picker_heading_every_session),
            t!(effort_picker_scope_every_session),
            [
                t!(effort_picker_heading_this_session),
                t!(effort_picker_scope_this_session),
            ],
        );
        let title = &screen[row_holding(&screen, t!(effort_picker_heading_every_session))];
        assert!(title.contains("effort · every session"), "{title}");
        let footer = &screen[row_holding(&screen, t!(effort_picker_scope_every_session))];
        assert!(
            footer.contains("Enter: every session  ·  Tab: this session only"),
            "{footer}"
        );
    }

    /// VIEW-46. Rejects: a picker opened in the session-only scope that still draws the
    /// every-session words, a Tab hint that names the scope already in force, and a title that does
    /// not change with the scope.
    #[test]
    fn a_picker_opened_by_effort_session_draws_the_this_session_scope_in_title_and_footer() {
        let picker = Picker::new(None).starting_in(Scope::ThisSession);
        let screen = screen_at(&picker, 80, 24);
        assert_scope_is_drawn(
            &screen,
            t!(effort_picker_heading_this_session),
            t!(effort_picker_scope_this_session),
            [
                t!(effort_picker_heading_every_session),
                t!(effort_picker_scope_every_session),
            ],
        );
        let title = &screen[row_holding(&screen, t!(effort_picker_heading_this_session))];
        assert!(title.contains("effort · this session only"), "{title}");
        let footer = &screen[row_holding(&screen, t!(effort_picker_scope_this_session))];
        assert!(
            footer.contains("Enter: this session only  ·  Tab: every session"),
            "{footer}"
        );
    }

    /// VIEW-46. Rejects: a draw that reads the scope the picker was opened with and not the one Tab left
    /// it in, so the footer says what Enter does in the other scope.
    #[test]
    fn tab_changes_what_the_title_and_footer_say_in_both_directions() {
        let mut picker = Picker::new(None);
        press(&mut picker, KeyCode::Tab, KeyModifiers::NONE);
        let output = rendered(&picker);
        assert!(
            output.contains(t!(effort_picker_heading_this_session)),
            "{output}"
        );
        assert!(
            output.contains(t!(effort_picker_scope_this_session)),
            "{output}"
        );
        assert!(
            !output.contains(t!(effort_picker_scope_every_session)),
            "{output}"
        );

        press(&mut picker, KeyCode::BackTab, KeyModifiers::SHIFT);
        let output = rendered(&picker);
        assert!(
            output.contains(t!(effort_picker_heading_every_session)),
            "{output}"
        );
        assert!(
            output.contains(t!(effort_picker_scope_every_session)),
            "{output}"
        );
        assert!(
            !output.contains(t!(effort_picker_scope_this_session)),
            "{output}"
        );
    }

    /// VIEW-46. The keys row names how to move and how to leave, `↑↓ choose  ·  Esc keep current`,
    /// and does not say that Enter selects, since the scope row says what Enter does.
    ///
    /// Rejects: a keys row that names Enter, `↑↓ choose  ·  Enter select  ·  Esc keep current`,
    /// which gives Enter two meanings once the scope row is drawn.
    #[test]
    fn the_keys_row_does_not_say_enter_selects() {
        assert_eq!(t!(effort_picker_keys), "↑↓ choose  ·  Esc keep current");
        for scope in [Scope::EverySession, Scope::ThisSession] {
            let screen = screen_at(&Picker::new(None).starting_in(scope), 80, 24);
            let keys = &screen[row_holding(&screen, "↑↓ choose")];
            assert!(keys.contains("↑↓ choose  ·  Esc keep current"), "{keys}");
            assert!(
                screen.iter().all(|row| !row.contains("Enter select")),
                "{screen:#?}"
            );
        }
    }

    /// The Enter half and the Tab half of the scope line, as the catalog words them.
    fn halves_of(scope: Scope) -> Vec<String> {
        let line = match scope {
            Scope::EverySession => t!(effort_picker_scope_every_session),
            Scope::ThisSession => t!(effort_picker_scope_this_session),
        };
        line.split('·')
            .map(|half| half.trim().to_string())
            .collect()
    }

    /// VIEW-46. At 40 columns the scope line is wider than the panel, so it is drawn whole on a second
    /// row instead of being cut after "Tab: thi". Rejects: a scope line left on one row, which the
    /// terminal cuts so the Tab half is lost, a wrapped line that drops a half, and a panel not made
    /// a row taller for it, which hides the last level behind the footer.
    #[test]
    fn the_scope_line_wraps_whole_at_forty_columns() {
        for scope in [Scope::EverySession, Scope::ThisSession] {
            let picker = Picker::new(None).starting_in(scope);
            let screen = screen_at(&picker, 40, 24);
            let halves = halves_of(scope);
            assert_eq!(halves.len(), 2, "{halves:?}");
            let enter = row_holding(&screen, &halves[0]);
            let tab = row_holding(&screen, &halves[1]);
            assert!(enter < tab, "the halves are not on two rows: {screen:#?}");
            let keys = row_holding(&screen, "↑↓ choose");
            assert_eq!(
                keys,
                tab + 1,
                "the keys are not under the scope: {screen:#?}"
            );
            let wide = placed(Rect::new(0, 0, 80, 24), &picker).area.height;
            let narrow = placed(Rect::new(0, 0, 40, 24), &picker).area.height;
            assert_eq!(narrow, wide + 1, "the panel is not one row taller");
            assert_every_level_is_in_the_list(&picker, 40, 24);
        }
    }

    /// VIEW-46. At 80 columns the whole scope line fits one row and is not wrapped: both halves are on
    /// the one row, directly above the keys. Rejects: a wrap that is made whether or not the line
    /// fits, which takes a row from the list in every panel.
    #[test]
    fn the_scope_line_stays_on_one_row_at_eighty_columns() {
        for scope in [Scope::EverySession, Scope::ThisSession] {
            let picker = Picker::new(None).starting_in(scope);
            let screen = screen_at(&picker, 80, 24);
            let halves = halves_of(scope);
            let enter = row_holding(&screen, &halves[0]);
            assert_eq!(enter, row_holding(&screen, &halves[1]), "{screen:#?}");
            assert_eq!(enter + 1, row_holding(&screen, "↑↓ choose"), "{screen:#?}");
        }
    }

    /// Each level the picker offers is drawn on its own row of the list, the default row first.
    fn assert_every_level_is_in_the_list(picker: &Picker, width: u16, height: u16) {
        let list = placed(Rect::new(0, 0, width, height), picker).list;
        let screen = screen_at(picker, width, height);
        assert!(list.height >= 6, "the list has {} rows", list.height);
        for (offset, name) in ["default", "low", "medium", "high", "xhigh", "max"]
            .iter()
            .enumerate()
        {
            let row = &screen[usize::from(list.y) + offset];
            assert!(
                row.contains(name),
                "row {offset} reads {row:?}: {screen:#?}"
            );
        }
    }

    /// VIEW-46. Rejects: a panel one row too short for the extra footer row, which leaves the list the
    /// row the footer is drawn over, so the last level is hidden behind it at 80x24, where the five
    /// levels and the default row fit.
    #[test]
    fn the_extra_footer_row_takes_no_row_from_a_list_that_fits() {
        for scope in [Scope::EverySession, Scope::ThisSession] {
            let picker = Picker::new(None).starting_in(scope);
            assert_every_level_is_in_the_list(&picker, 80, 24);
            let screen = screen_at(&picker, 80, 24);
            assert!(
                screen[row_holding(&screen, "max")..]
                    .iter()
                    .any(|row| row.contains(t!(effort_picker_keys))),
                "{screen:#?}"
            );
        }
    }
}
