use crate::app::Action;
use crate::state::Session;
use ratatui::crossterm::event::KeyEvent;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    #[default]
    Default,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RightColumn {
    /// `columns` includes the panel's border.
    InfoPanel { columns: u16 },
}

impl Layout {
    pub fn name(self) -> &'static str {
        match self {
            Self::Default => "default",
        }
    }

    pub fn right_column(self, session: &Session, width: u16) -> Option<RightColumn> {
        match self {
            Self::Default => crate::panel::drawn(session, width).then(|| RightColumn::InfoPanel {
                columns: session.panel_columns(width),
            }),
        }
    }

    pub fn claims_key(self, _session: &mut Session, _key: KeyEvent) -> Option<Action> {
        match self {
            Self::Default => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::handle_key;
    use crate::select::Pane;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::buffer::Buffer;
    use ratatui::crossterm::event::{KeyCode, KeyModifiers};
    use ratatui::layout::Rect;

    const HEIGHT: u16 = 30;

    fn frame(session: &mut Session, width: u16) -> (Buffer, crate::state::Laid) {
        let mut terminal = Terminal::new(TestBackend::new(width, HEIGHT)).expect("terminal");
        let mut laid = None;
        terminal
            .draw(|frame| laid = Some(crate::render::draw(frame, session)))
            .expect("draw succeeds");
        let laid = laid.expect("the frame was drawn");
        session.note_layout(laid.clone());
        (terminal.backend().buffer().clone(), laid)
    }

    fn with_the_panel_open() -> Session {
        let mut session = Session::new("none");
        frame(&mut session, 140);
        session.toggle_panel();
        assert!(
            session.panel_open(),
            "the panel did not open at 140 columns"
        );
        session
    }

    fn the_panel_rect(laid: &crate::state::Laid) -> Option<Rect> {
        laid.panes
            .iter()
            .find(|(pane, _)| *pane == Pane::Panel)
            .map(|(_, rect)| *rect)
    }

    #[test]
    fn the_default_layout_is_named_default() {
        assert_eq!(Layout::default(), Layout::Default);
        assert_eq!(Layout::default().name(), "default");
    }

    /// PANEL-6.
    #[test]
    fn the_right_column_is_the_info_panel_from_100_columns_and_not_below() {
        let session = with_the_panel_open();

        assert_eq!(Layout::Default.right_column(&session, 99), None);
        assert_eq!(
            Layout::Default.right_column(&session, 100),
            Some(RightColumn::InfoPanel { columns: 36 })
        );
        assert_eq!(
            Layout::Default.right_column(&session, 140),
            Some(RightColumn::InfoPanel { columns: 36 })
        );
    }

    #[test]
    fn the_right_column_is_absent_while_the_panel_is_closed() {
        let session = Session::new("none");

        assert_eq!(Layout::Default.right_column(&session, 140), None);
        assert_eq!(Layout::Default.right_column(&session, 100), None);
    }

    #[test]
    fn the_right_column_is_as_wide_as_the_session_says_the_panel_is() {
        let mut session = with_the_panel_open();
        session.drag_panel_to(140 - 50);

        for width in [100, 120, 140] {
            assert_eq!(
                Layout::Default.right_column(&session, width),
                Some(RightColumn::InfoPanel {
                    columns: session.panel_columns(width)
                }),
                "at {width} columns"
            );
        }
        assert_eq!(
            Layout::Default.right_column(&session, 140),
            Some(RightColumn::InfoPanel { columns: 50 })
        );
    }

    #[test]
    fn a_frame_has_the_panel_where_the_panel_rule_puts_it() {
        for (width, open) in [(99, true), (100, true), (140, true), (140, false)] {
            let mut session = with_the_panel_open();
            if !open {
                session.toggle_panel();
            }
            let (buffer, laid) = frame(&mut session, width);

            let expected = crate::panel::drawn(&session, width).then(|| {
                let columns = session.panel_columns(width);
                Rect::new(width - columns, 0, columns, HEIGHT - 1)
            });
            assert_eq!(
                the_panel_rect(&laid),
                expected,
                "at {width} columns, panel open: {open}"
            );
            let transcript = laid
                .panes
                .iter()
                .find(|(pane, _)| *pane == Pane::Transcript)
                .map(|(_, rect)| rect.width)
                .expect("a transcript");
            assert_eq!(
                transcript,
                expected.map_or(width, |panel| width - panel.width),
                "at {width} columns, panel open: {open}"
            );

            if let Some(panel) = expected {
                let mut alone = Buffer::empty(Rect::new(0, 0, width, HEIGHT));
                crate::panel::draw(&mut alone, panel, &session);
                for y in panel.top()..panel.bottom() {
                    for x in panel.left()..panel.right() {
                        assert_eq!(
                            buffer[(x, y)].symbol(),
                            alone[(x, y)].symbol(),
                            "cell ({x}, {y}) at {width} columns"
                        );
                    }
                }
            }
        }
    }

    fn keys_swept() -> Vec<KeyEvent> {
        let plain = |code| KeyEvent::new(code, KeyModifiers::NONE);
        vec![
            plain(KeyCode::Esc),
            plain(KeyCode::Enter),
            plain(KeyCode::Tab),
            plain(KeyCode::Up),
            plain(KeyCode::Down),
            plain(KeyCode::Backspace),
            plain(KeyCode::Char('a')),
            plain(KeyCode::Char('j')),
            plain(KeyCode::Char('/')),
            plain(KeyCode::Char('?')),
            KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
            KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL),
            KeyEvent::new(KeyCode::Char('x'), KeyModifiers::CONTROL),
            KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT),
        ]
    }

    #[test]
    fn the_default_layout_claims_no_key() {
        for key in keys_swept() {
            let mut session = Session::new("none");
            assert_eq!(
                Layout::Default.claims_key(&mut session, key),
                None,
                "{key:?}"
            );
        }
    }

    #[test]
    fn a_key_in_the_default_layout_is_answered_by_the_prompt() {
        let expected = [
            ("Esc", Action::Redraw),
            ("Enter", Action::None),
            ("Tab", Action::None),
            ("Up", Action::Redraw),
            ("Down", Action::Redraw),
            ("Backspace", Action::Redraw),
            ("a", Action::Redraw),
            ("j", Action::Redraw),
            ("/", Action::Redraw),
            ("?", Action::Redraw),
            ("Ctrl-C", Action::Redraw),
            ("Ctrl-D", Action::Redraw),
            ("Ctrl-X", Action::Redraw),
            ("Shift-Enter", Action::Redraw),
        ];
        for (key, (name, action)) in keys_swept().into_iter().zip(expected) {
            let mut session = Session::new("none");
            assert_eq!(handle_key(&mut session, key), action, "{name}");
            if let KeyCode::Char(typed @ ('a' | 'j' | '/')) = key.code {
                assert_eq!(
                    session.input(),
                    typed.to_string(),
                    "{name} did not reach the box"
                );
            }
        }
    }

    #[test]
    fn backspace_in_the_default_layout_edits_the_box() {
        let mut session = Session::new("none");
        for typed in ['a', 'b'] {
            handle_key(
                &mut session,
                KeyEvent::new(KeyCode::Char(typed), KeyModifiers::NONE),
            );
        }
        assert_eq!(session.input(), "ab");

        handle_key(
            &mut session,
            KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE),
        );

        assert_eq!(session.input(), "a");
    }
}
