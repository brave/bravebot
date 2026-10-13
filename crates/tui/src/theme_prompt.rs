//! Choosing which theme paints the interface.
//!
//! Shown by `/theme`. The list is the built-in set plus JSON files from `~/.bravebot/themes`, and
//! the choice is written to `~/.bravebot`: it outlives the session that made it and applies in
//! every directory.
//!
//! Drawn as a centred panel over the session, so the transcript stays visible and a live preview
//! repaints what is behind the box. Moving the cursor live-previews. Enter keeps the preview and
//! persists the name. Escape puts back the theme that was in force when the picker opened.
//!
//! Nothing labelled is involved. The names never reach a model: they are drawn for a person, who
//! picks one.

use bravebot_i18n::t;
use ratatui::Frame;
use ratatui::Terminal;
use ratatui::backend::Backend;
use ratatui::crossterm::event;

use crate::input;

use crate::list_picker::{self, ListPicker, Outcome, Panel, PickerRow};
use crate::theme::{self, Theme};

impl PickerRow for Theme {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn hint(&self) -> Option<&'static str> {
        theme::hint(self)
    }

    fn same_as(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

/// What the picker is showing and where the cursor is. Everything on offer, `brave` first among the
/// built-ins, with the theme in force when the picker opened kept for the mark and for a cancel.
pub type Picker = ListPicker<Theme>;

impl Picker {
    /// Open on the theme in use, since that is the row a user is looking for.
    pub fn new(themes: Vec<Theme>, current: &str) -> Self {
        let previous = themes
            .iter()
            .find(|theme| theme.name == current)
            .cloned()
            .unwrap_or_else(|| Theme {
                name: theme::BRAVE.to_string(),
                palette: theme::brave_palette(false),
                adapts: true,
                listed: true,
            });
        let selected = themes
            .iter()
            .position(|theme| theme.name == current)
            .unwrap_or(0);
        Self::open(themes, selected, previous)
    }

    /// Put the theme under the cursor in force so the list itself shows what it would look like.
    fn preview(&self) {
        if let Some(theme) = self.chosen() {
            theme::apply(theme);
        }
    }

    fn restore(&self) {
        theme::apply(self.previous());
    }
}

/// Show the list as a centred panel over whatever `behind` draws, and return the theme the user
/// picked, or `None` if they kept the current one.
///
/// `behind` is the session (or anything else already on screen). Drawn first each frame so a live
/// preview repaints the transcript in the theme under the cursor, and so Escape leaves the person
/// looking at the same session they opened the picker from.
pub fn choose<B: Backend>(
    terminal: &mut Terminal<B>,
    themes: Vec<Theme>,
    current: &str,
    mut behind: impl FnMut(&mut Frame),
) -> Option<Theme> {
    let mut picker = Picker::new(themes, current);
    let panel = panel();
    picker.preview();

    loop {
        if terminal
            .draw(|frame| {
                behind(frame);
                list_picker::draw(frame, &picker, &panel);
            })
            .is_err()
        {
            picker.restore();
            return None;
        }

        let Ok(event) = input::read() else {
            picker.restore();
            return None;
        };
        let Some(key) = input::key_of(&event) else {
            continue;
        };
        if key.kind != event::KeyEventKind::Press {
            continue;
        }

        match list_picker::handle_key(&mut picker, key.code, key.modifiers) {
            Outcome::Continue => {
                picker.preview();
                continue;
            }
            Outcome::Cancel => {
                picker.restore();
                return None;
            }
            Outcome::Select => {
                let chosen = picker.chosen().cloned();
                if let Some(ref theme) = chosen {
                    theme::apply(theme);
                } else {
                    picker.restore();
                }
                return chosen;
            }
        }
    }
}

/// Narrower than the confirm box: a name list does not need the width a diff does, and OpenCode's
/// theme dialog is a compact select rather than a full-bleed page.
fn panel() -> Panel {
    Panel {
        title: t!(theme_picker_title),
        keys: t!(theme_picker_keys),
        width: 42,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::crossterm::event::{KeyCode, KeyModifiers};
    use ratatui::layout::Rect;

    fn offered() -> Vec<Theme> {
        theme::builtins()
    }

    #[test]
    fn the_picker_opens_on_the_theme_in_use() {
        let picker = Picker::new(offered(), "nord");
        assert_eq!(picker.chosen().expect("a theme").name, "nord");
    }

    #[test]
    fn a_current_theme_no_longer_offered_opens_at_the_top() {
        let picker = Picker::new(offered(), "withdrawn-theme");
        assert_eq!(picker.chosen().expect("a theme").name, theme::BRAVE);
    }

    #[test]
    fn preview_puts_the_cursor_theme_in_force_and_cancel_restores() {
        let _held = theme::exclusive();
        theme::apply_brave();
        let mut picker = Picker::new(offered(), theme::BRAVE);
        list_picker::handle_key(&mut picker, KeyCode::Down, KeyModifiers::NONE);
        picker.preview();
        assert_ne!(theme::name(), theme::BRAVE);
        picker.restore();
        assert_eq!(theme::name(), theme::BRAVE);
    }

    fn rendered(picker: &Picker) -> String {
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("terminal");
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

    #[test]
    fn the_theme_in_use_is_marked() {
        let output = rendered(&Picker::new(offered(), "nord"));
        assert!(output.contains("nord  current"), "{output}");
    }

    #[test]
    fn the_list_shows_names_a_person_reads() {
        let themes = offered();
        let output = rendered(&Picker::new(themes.clone(), theme::BRAVE));
        assert!(output.contains(&themes[0].name), "{output}");
        // The panel on 80x24 shows the start of the list; names near the end sit below the fold.
        assert!(output.contains(&themes[1].name), "{output}");
    }

    /// `brave` is the only theme whose inks depend on the terminal, and its name does not say so.
    #[test]
    fn the_terminal_following_theme_says_so_under_the_list() {
        let output = rendered(&Picker::new(offered(), theme::BRAVE));
        assert!(output.contains("follows your terminal"), "{output}");
    }

    /// The row stopped being about one theme once a family could follow the terminal too, and a
    /// person on the `gruvbox` row has the same question the `brave` row answers.
    #[test]
    fn a_family_that_follows_the_terminal_says_so_under_the_list() {
        let output = rendered(&Picker::new(theme::listed("gruvbox"), "gruvbox"));
        assert!(output.contains("follows your terminal"), "{output}");
    }

    #[test]
    fn a_theme_that_paints_every_ink_itself_has_nothing_to_add() {
        let output = rendered(&Picker::new(offered(), "nord"));
        assert!(!output.contains("follows your terminal"), "{output}");
    }

    /// The picker is a panel over the session, not a full-screen takeover: the title sits in a
    /// rounded border the confirm prompts already use.
    #[test]
    fn the_picker_is_drawn_as_a_centred_panel() {
        let output = rendered(&Picker::new(offered(), theme::BRAVE));
        assert!(output.contains("themes"), "{output}");
        // Corners of a rounded border, so a full-bleed list would not pass.
        assert!(
            output.contains('╭') && output.contains('╮'),
            "no rounded border: {output}"
        );
    }

    #[test]
    fn the_panel_stays_inside_a_tiny_terminal() {
        let picker = Picker::new(offered(), theme::BRAVE);
        let mut terminal = Terminal::new(TestBackend::new(30, 8)).expect("terminal");
        terminal
            .draw(|frame| list_picker::draw(frame, &picker, &panel()))
            .expect("draw succeeds");
        let area = list_picker::centred(Rect::new(0, 0, 30, 8), picker.rows().len(), panel().width);
        assert!(area.width <= 30);
        assert!(area.height <= 8);
        assert!(area.x + area.width <= 30);
        assert!(area.y + area.height <= 8);
    }
}
