//! The question put the first time a plugin's hook would fire (PLUGIN-7).

use bravebot_agent::confirm::HookRequest;
use bravebot_i18n::t;
use ratatui::Terminal;
use ratatui::backend::Backend;
use ratatui::style::{Modifier, Style};
use ratatui::text::Span;

use crate::theme;

use super::chrome::{answer_keys, centred, indented, panel};
use super::pinned::{self, Drawn, Question, Seen};
use super::plugin::untrusted;
use super::{Answer, answer_for, ask_plain};

/// Put the question about one plugin hook to the person, blocking until answered. Only a yes
/// approves. A lost terminal and Ctrl-C answer no.
pub fn ask_hook<B: Backend>(terminal: &mut Terminal<B>, request: &HookRequest) -> Answer {
    ask_plain(
        terminal,
        |frame, scroll, seen| draw_hook(frame, request, scroll, seen),
        answer_for,
    )
}

/// Draw the question. The plugin's name and every word of the argv are the plugin's own and are
/// drawn behind the margin, each word on a row, uncut: a yes is for exactly these words, so none is
/// left to be read later. Every row decides the question, and no yes is taken until all have been
/// drawn.
pub(super) fn draw_hook(
    frame: &mut ratatui::Frame,
    request: &HookRequest,
    scroll: u16,
    seen: &mut Seen,
) -> Drawn {
    let area = centred(frame.area());
    let inside = panel(frame, area, theme::note(), t!(plugin_hook_title));
    let width = inside.width as usize;
    let bold = Style::default().add_modifier(Modifier::BOLD);
    let muted = Style::default().fg(theme::muted());
    let margin = Span::styled("┃ ", Style::default().fg(theme::running()));

    let mut lines = Vec::new();
    lines.extend(indented(t!(plugin_hook_plugin), muted, width));
    lines.extend(untrusted(&request.plugin, bold, &margin, width, None, false).0);
    lines.extend(indented(t!(plugin_hook_moment), muted, width));
    // The driver's own word for the moment, so no margin.
    lines.extend(indented(request.moment, bold, width));
    lines.extend(indented(t!(plugin_hook_argv), muted, width));
    for (number, word) in request.argv.iter().enumerate() {
        let text = format!("{}. {word}", number + 1);
        lines.extend(untrusted(&text, bold, &margin, width, None, false).0);
    }
    lines.push(ratatui::text::Line::raw(""));
    lines.extend(indented(t!(plugin_hook_explained), muted, width));
    let consequence = match request.records {
        true => t!(
            plugin_hook_recorded,
            file = bravebot_config::hooks_approved::approvals_file(std::path::Path::new(
                "~/.bravebot"
            ))
            .display()
            .to_string()
        ),
        false => t!(plugin_hook_session_only).to_string(),
    };
    lines.extend(indented(consequence, muted, width));

    let keys = |answerable| {
        vec![answer_keys(
            t!(plugin_hook_yes),
            t!(plugin_hook_no),
            answerable,
        )]
    };
    pinned::draw(
        frame,
        inside,
        Question::scrolled(lines, usize::MAX, 0, &keys),
        scroll,
        seen,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::confirm::Next;
    use ratatui::backend::TestBackend;
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use std::collections::VecDeque;

    fn a_hook(argv: Vec<String>, records: bool) -> HookRequest {
        HookRequest {
            plugin: "demo-plugin".to_string(),
            moment: "turn-started",
            argv,
            records,
        }
    }

    /// The answer to a script of keys, and the last screen drawn. A script that runs out is a lost
    /// terminal.
    fn answered(request: &HookRequest, keys: &[KeyCode], rows: u16) -> (Answer, String) {
        let mut terminal = Terminal::new(TestBackend::new(100, rows)).unwrap();
        let mut queue: VecDeque<KeyCode> = keys.iter().copied().collect();
        let answer = crate::confirm::ask_scrolled_reading(
            &mut terminal,
            Answer::Reject,
            |frame, scroll, seen| draw_hook(frame, request, scroll, seen),
            |pressed, drawn| answer_for(pressed, drawn).map(crate::confirm::Step::from),
            || match queue.pop_front() {
                Some(code) => Next::Press(KeyEvent::new(code, KeyModifiers::NONE)),
                None => Next::Lost,
            },
        );
        let buffer = terminal.backend().buffer().clone();
        let area = buffer.area;
        let screen = (0..area.height)
            .map(|y| {
                (0..area.width)
                    .map(|x| buffer[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");
        (answer, screen)
    }

    /// S10, S13: the question names the plugin, the moment and every argv word, with the plugin's
    /// words behind the margin and the moment in the driver's own, and says what a yes records.
    ///
    /// Rejects: a question that leaves out the argv or the plugin, and one that draws the plugin's
    /// text without the margin.
    #[test]
    fn the_question_names_the_plugin_the_moment_and_every_word_of_the_argv() {
        let request = a_hook(
            vec![
                "/h/.bravebot/plugins/demo-plugin/hook.sh".into(),
                "--flag".into(),
            ],
            true,
        );
        let (_, screen) = answered(&request, &[KeyCode::Char('n')], 40);
        for shown in [
            "run this plugin's hook?",
            "demo-plugin",
            "turn-started",
            "1. /h/.bravebot/plugins/demo-plugin/hook.sh",
            "2. --flag",
            "Yes, run it",
            "No, do not run it",
            "hooks-approved",
        ] {
            assert!(screen.contains(shown), "{shown}\n{screen}");
        }
        let margin = |needle: &str| {
            screen
                .lines()
                .find(|row| row.contains(needle))
                .is_some_and(|row| row.contains('┃'))
        };
        assert!(
            margin("demo-plugin\n") || margin("┃ demo-plugin"),
            "{screen}"
        );
        assert!(margin("2. --flag"), "{screen}");
        assert!(
            !screen
                .lines()
                .find(|row| row.contains("turn-started"))
                .is_some_and(|row| row.contains('┃')),
            "the driver's own word for the moment is behind the plugin's margin\n{screen}"
        );
    }

    /// S13: where a yes is not written (incognito) the question says it lasts for the session only.
    ///
    /// Rejects: a question that promises a record the session will not make.
    #[test]
    fn a_yes_that_will_not_be_written_says_so() {
        let (_, screen) = answered(&a_hook(vec!["/p".into()], false), &[KeyCode::Char('n')], 40);
        assert!(screen.contains("not written anywhere"), "{screen}");
        assert!(!screen.contains("hooks-approved"), "{screen}");
    }

    /// S14, PLUGIN-7: only a y approves. Enter, any other key, a lost terminal and Ctrl-C do not.
    ///
    /// Rejects: a question that takes Enter or a lost terminal as a yes.
    #[test]
    fn only_a_y_approves_and_a_lost_terminal_is_a_no() {
        let request = a_hook(vec!["/p".into()], true);
        assert_eq!(
            answered(&request, &[KeyCode::Char('y')], 40).0,
            Answer::Approve
        );
        for keys in [
            vec![KeyCode::Enter, KeyCode::Char('n')],
            vec![KeyCode::Char('x'), KeyCode::Esc],
            vec![KeyCode::Char('n')],
            Vec::new(),
        ] {
            assert_eq!(answered(&request, &keys, 40).0, Answer::Reject, "{keys:?}");
        }
    }

    /// S10: a yes is not taken while an argv word is still off the screen. The argv is longer than
    /// the box, so a y pressed at once is ignored and the question waits.
    ///
    /// Rejects: a yes for a program whose last words nobody was shown.
    #[test]
    fn no_yes_is_taken_before_every_word_of_a_long_argv_has_been_drawn() {
        let words: Vec<String> = (0..80).map(|n| format!("word{n:03}")).collect();
        let request = a_hook(words, true);
        let (answer, _) = answered(&request, &[KeyCode::Char('y')], 20);
        assert_eq!(
            answer,
            Answer::Reject,
            "a yes was taken with the argv still off screen"
        );
        let mut scrolled = vec![KeyCode::PageDown; 40];
        scrolled.push(KeyCode::Char('y'));
        assert_eq!(answered(&request, &scrolled, 20).0, Answer::Approve);
    }
}
