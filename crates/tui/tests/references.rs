//! File references typed with `@`, against a real directory.
//!
//! The property that matters is at the end: a file named with `@` reaches the turn as **trusted**
//! context, because the user typed the path. These check the way there, since a completion that
//! offered the wrong file would be admitting the wrong contents.

use bravebot_tui::Session;
use bravebot_tui::app::{Action, handle_key, handle_paste};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::path::PathBuf;

struct Scratch {
    path: PathBuf,
}

impl Scratch {
    fn new(name: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("bravebot-references-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(path.join("src")).expect("create");
        std::fs::write(path.join("Cargo.toml"), "[package]").expect("write");
        std::fs::write(path.join("README.md"), "# notes").expect("write");
        std::fs::write(path.join("src/main.rs"), "fn main() {}").expect("write");
        Self { path }
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn typing(session: &mut Session, text: &str) {
    for c in text.chars() {
        handle_key(session, key(KeyCode::Char(c)));
    }
}

fn session(scratch: &Scratch) -> Session {
    Session::new("none").in_workspace(&scratch.path)
}

/// An `@` offers what is in the workspace, so a user can see which files they may admit.
#[test]
fn an_at_sign_offers_the_workspace() {
    let scratch = Scratch::new("offer");
    let mut session = session(&scratch);
    typing(&mut session, "look at @");

    assert!(session.is_completing(), "nothing was offered");
    let offered: Vec<String> = match session.offered() {
        bravebot_tui::state::Offered::Files(entries) => {
            entries.into_iter().map(|e| e.path).collect()
        }
        other => panic!("files were not offered: {other:?}"),
    };
    assert_eq!(
        offered,
        vec![
            "src/".to_string(),
            "Cargo.toml".to_string(),
            "README.md".to_string()
        ],
        "directories come first, then files, each alphabetical"
    );
}

/// Tab completes the reference in place, leaving the sentence it was written into alone.
#[test]
fn tab_completes_a_reference_without_disturbing_the_sentence() {
    let scratch = Scratch::new("tab");
    let mut session = session(&scratch);
    typing(&mut session, "please read @READ");

    assert_eq!(handle_key(&mut session, key(KeyCode::Tab)), Action::Redraw);
    assert_eq!(session.input(), "please read @README.md ");
}

/// A name may hold an `@` of its own, and taking the offered entry has to produce exactly that
/// entry: a line rewritten into a path nobody chose is a file nobody vouched for.
#[test]
fn a_name_holding_an_at_sign_completes_to_itself() {
    let scratch = Scratch::new("at-in-name");
    std::fs::write(scratch.path.join("logo@2x.png"), "x").expect("write");
    let mut session = session(&scratch);
    typing(&mut session, "look at @logo@2");

    assert_eq!(handle_key(&mut session, key(KeyCode::Tab)), Action::Redraw);
    assert_eq!(session.input(), "look at @logo@2x.png ");
}

/// A directory completes without a trailing space, so the path can be typed onwards into it. A file
/// gets one, since the reference is finished.
#[test]
fn a_directory_completes_so_typing_can_continue_into_it() {
    let scratch = Scratch::new("walk");
    let mut session = session(&scratch);
    typing(&mut session, "@sr");
    handle_key(&mut session, key(KeyCode::Tab));
    assert_eq!(session.input(), "@src/");

    typing(&mut session, "mai");
    handle_key(&mut session, key(KeyCode::Tab));
    assert_eq!(session.input(), "@src/main.rs ");
}

/// NAME-11: a fragment that begins nothing in the root is looked for below it, so a person who
/// remembers part of a name and not its directory gets the file, and Tab writes the whole path
/// without disturbing the sentence it is in.
#[test]
fn a_fragment_finds_a_file_below_and_tab_writes_the_whole_path() {
    let scratch = Scratch::new("found-tab");
    let mut session = session(&scratch);
    typing(&mut session, "explain @mnrs");

    assert_eq!(offered_paths(&session), ["src/main.rs"]);
    assert_eq!(handle_key(&mut session, key(KeyCode::Tab)), Action::Redraw);
    assert_eq!(session.input(), "explain @src/main.rs ");
}

/// NAME-7, NAME-11: Enter on a fragment takes the file found below instead of sending the fragment,
/// and the line then names that file, which is the one the person was shown.
#[test]
fn enter_completes_a_fragment_to_the_file_found_below_and_names_that_file() {
    let scratch = Scratch::new("found-enter");
    let mut session = session(&scratch);
    typing(&mut session, "explain @mnrs");

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Redraw
    );
    assert_eq!(session.input(), "explain @src/main.rs ");
    assert!(
        session.transcript.is_empty(),
        "a half-typed reference was sent"
    );
    assert_eq!(
        bravebot_mentions::referenced(session.input()),
        ["src/main.rs"]
    );
}

/// The arrows walk the offered files, and Enter takes one rather than sending a half-typed path.
#[test]
fn the_arrows_and_enter_choose_among_the_offered_files() {
    let scratch = Scratch::new("choose");
    let mut session = session(&scratch);
    typing(&mut session, "@");

    handle_key(&mut session, key(KeyCode::Down));
    handle_key(&mut session, key(KeyCode::Down));
    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Redraw
    );
    assert_eq!(session.input(), "@README.md ", "the third entry");
    assert!(
        session.transcript.is_empty(),
        "a half-typed reference was sent"
    );
}

/// A finished name is finished even when a directory shares its prefix and sorts above it in the
/// list. Enter must send what the user typed rather than quietly swapping in a path they did not:
/// a workspace holding both `test` and `tests/` is ordinary.
#[test]
fn enter_sends_a_finished_reference_a_directory_shares_a_prefix_with() {
    let scratch = Scratch::new("prefix");
    std::fs::write(scratch.path.join("test"), "a").expect("write");
    std::fs::create_dir_all(scratch.path.join("tests")).expect("create");
    let mut session = session(&scratch);
    typing(&mut session, "explain @test");

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Submit("explain @test".to_string())
    );
}

/// And the list being capped for display must not cost it either. The offered entries are cut to
/// a readable number, directories first, so enough siblings sharing the prefix push the file out
/// of the list entirely. A name is no less finished for not being shown: deciding from what is
/// displayed means the number of directories a workspace happens to hold decides whether Enter
/// sends the file the user named or replaces it with a directory they never chose.
#[test]
fn enter_sends_a_finished_reference_the_offered_list_is_too_short_to_show() {
    let scratch = Scratch::new("prefix-capped");
    std::fs::write(scratch.path.join("test"), "a").expect("write");
    // Comfortably past the cap, so the file is cut whatever the exact number is.
    for n in 0..60 {
        std::fs::create_dir_all(scratch.path.join(format!("test{n:02}"))).expect("create");
    }
    let mut session = session(&scratch);
    typing(&mut session, "explain @test");

    let offered = match session.offered() {
        bravebot_tui::state::Offered::Files(entries) => entries,
        other => panic!("files were not offered: {other:?}"),
    };
    assert!(
        !offered.iter().any(|entry| entry.path == "test"),
        "the file was still offered, so the cap was never reached"
    );
    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Submit("explain @test".to_string())
    );
}

/// Sharing that prefix must not cost the arrows their meaning: a row the user walked to is a row
/// they chose, and Enter takes it even though the typed name is a file of its own.
#[test]
fn the_arrows_still_choose_a_row_over_a_finished_reference() {
    let scratch = Scratch::new("prefix-arrows");
    std::fs::write(scratch.path.join("test"), "a").expect("write");
    std::fs::create_dir_all(scratch.path.join("tests")).expect("create");
    std::fs::create_dir_all(scratch.path.join("test-utils")).expect("create");
    let mut session = session(&scratch);
    typing(&mut session, "explain @test");

    handle_key(&mut session, key(KeyCode::Down));
    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Redraw
    );
    assert_eq!(session.input(), "explain @tests/", "the second entry");
}

/// A paste is not a choice among the rows. The cursor an arrow left behind belongs to the list
/// that was on the screen when the arrow was pressed, and a paste narrows the list to something
/// else: read as a choice, it takes a name the user finished typing and completes it away into
/// whichever directory the stale row now lands on, so the finished sentence is not sent.
#[test]
fn a_paste_returns_the_cursor_to_the_top_so_enter_sends_a_finished_reference() {
    let scratch = Scratch::new("prefix-pasted");
    std::fs::write(scratch.path.join("test"), "a").expect("write");
    std::fs::create_dir_all(scratch.path.join("tests")).expect("create");
    std::fs::create_dir_all(scratch.path.join("test-utils")).expect("create");
    let mut session = session(&scratch);
    typing(&mut session, "explain @");

    handle_key(&mut session, key(KeyCode::Down));
    handle_paste(&mut session, "test");

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Submit("explain @test".to_string())
    );
}

/// The row it returns to is the top of the list the paste narrowed to, rather than whichever row
/// the stale index still fits inside. Reading the cursor against the new list rescues one that
/// points past the end and nothing else: an index that still lands somewhere highlights a file
/// nobody has looked at, and Tab takes it.
#[test]
fn a_paste_returns_the_cursor_to_the_top_of_the_narrowed_list() {
    let scratch = Scratch::new("narrowed-under-a-cursor");
    std::fs::write(scratch.path.join("test"), "a").expect("write");
    std::fs::create_dir_all(scratch.path.join("tests")).expect("create");
    std::fs::create_dir_all(scratch.path.join("test-utils")).expect("create");
    let mut session = session(&scratch);
    typing(&mut session, "explain @");

    for _ in 0..2 {
        handle_key(&mut session, key(KeyCode::Down));
    }
    handle_paste(&mut session, "test");
    handle_key(&mut session, key(KeyCode::Tab));

    assert_eq!(
        session.input(),
        "explain @test-utils/",
        "the first entry of the narrowed list"
    );
}

/// A reference finished by a space closes the list, so the arrows go back to history and scrolling
/// and the rest of the sentence can be typed.
#[test]
fn a_finished_reference_closes_the_list() {
    let scratch = Scratch::new("closed");
    let mut session = session(&scratch);
    typing(&mut session, "@README.md ");
    assert!(!session.is_completing(), "the list stayed open");

    typing(&mut session, "what does it say");
    assert!(!session.is_completing());
}

/// The completion must not become a way to browse the filesystem.
#[test]
fn a_reference_cannot_climb_out_of_the_workspace() {
    let scratch = Scratch::new("escape");
    let mut session = session(&scratch);
    typing(&mut session, "@../");
    assert!(!session.is_completing(), "a path outside was offered");

    session.clear_input();
    typing(&mut session, "@/etc/");
    assert!(!session.is_completing(), "an absolute path was offered");
}

/// The list is read from the workspace every time it is drawn, so a file that goes away while the
/// list is up shortens it under a cursor already further down. Nothing about the line changed, so
/// nothing put the cursor back, and reading the highlighted row must still name something rather
/// than pointing past the end: Tab on a row that named nothing completed nothing at all.
#[test]
fn a_cursor_past_the_end_of_a_narrowed_list_still_names_a_file() {
    let scratch = Scratch::new("narrow");
    let mut session = session(&scratch);
    typing(&mut session, "@");
    for _ in 0..2 {
        handle_key(&mut session, key(KeyCode::Down));
    }

    // Every row the list held but the first, so the cursor is left past the end of what is left.
    std::fs::remove_file(scratch.path.join("Cargo.toml")).expect("remove");
    std::fs::remove_file(scratch.path.join("README.md")).expect("remove");
    handle_key(&mut session, key(KeyCode::Tab));
    assert_eq!(session.input(), "@src/");
}

/// Everything a key press and its frame ask about what is offered is answered from one reading of
/// the directory. The file is removed between two questions with no key between them, so the second
/// can only name it if the disk was not read again.
#[test]
fn the_callers_of_one_key_press_share_one_reading_of_the_directory() {
    let scratch = Scratch::new("shared");
    let mut session = session(&scratch);
    typing(&mut session, "@");
    let first = offered_paths(&session);
    assert!(first.iter().any(|path| path == "README.md"), "{first:?}");

    std::fs::remove_file(scratch.path.join("README.md")).expect("remove");
    assert_eq!(
        offered_paths(&session),
        first,
        "a question with no key before it read the directory again"
    );

    handle_key(&mut session, key(KeyCode::Down));
    assert!(
        !offered_paths(&session)
            .iter()
            .any(|path| path == "README.md"),
        "the next key press was answered from the reading before it"
    );
}

/// An ordinary sentence containing an address is not a reference: only a word beginning with `@`
/// is, and only while it is the one being typed.
#[test]
fn an_address_in_a_sentence_is_not_a_reference() {
    let scratch = Scratch::new("address");
    let mut session = session(&scratch);
    typing(&mut session, "mail me@example.com");
    assert!(!session.is_completing());
}

/// A directory is somewhere to type through, not a file to read, so naming one includes nothing.
#[test]
fn a_directory_reference_is_not_included_as_a_file() {
    assert!(bravebot_mentions::referenced("look in @src/").is_empty());
}

/// A prompt ending in a finished reference sends on Enter. It reads as still being completed, since
/// the last word is what the list is about, but the sentence is done: completing there left a user
/// pressing Enter twice to say something perfectly well formed.
#[test]
fn enter_sends_a_prompt_that_ends_in_a_finished_reference() {
    let scratch = Scratch::new("finished");
    let mut session = session(&scratch);
    typing(&mut session, "explain @README.md");

    assert!(session.is_completing(), "the list is about the last word");
    assert!(
        !session.completion_would_change_the_line(),
        "taking the offer would change nothing, so Enter must send"
    );
    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Submit("explain @README.md".to_string())
    );
}

/// And one ending in a half-typed reference completes instead, which is the case that made Enter
/// worth overloading at all.
#[test]
fn enter_completes_a_prompt_that_ends_in_a_half_typed_reference() {
    let scratch = Scratch::new("half");
    let mut session = session(&scratch);
    typing(&mut session, "explain @READ");

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Redraw
    );
    assert_eq!(session.input(), "explain @README.md ");
}

/// The reading the event loop does when it builds a turn, kept beside the typing tests so the two
/// cannot drift. That the contents then reach the model as trusted context is checked against a real
/// turn in the agent crate, where the streaming harness lives:
/// `a_turn_includes_referenced_file_contents`.
#[test]
fn the_files_a_submitted_line_would_include() {
    let scratch = Scratch::new("collect");
    let mut session = session(&scratch);
    typing(&mut session, "compare @src/main.rs with @README.md");

    let sent = match handle_key(&mut session, key(KeyCode::Enter)) {
        Action::Submit(sent) => sent,
        other => panic!("the line was not sent: {other:?}"),
    };
    assert_eq!(
        bravebot_mentions::referenced(&sent),
        vec!["src/main.rs".to_string(), "README.md".to_string()]
    );
}

/// A name holding a space completes to the escaped form, and that line names the same file when it
/// is sent. Without the escape the line ends the reference at the space and names a different path.
#[test]
fn a_name_with_a_space_completes_to_a_reference_that_names_it() {
    let scratch = Scratch::new("space-in-name");
    std::fs::create_dir_all(scratch.path.join("My Documents")).expect("create");
    std::fs::write(scratch.path.join("My Documents/notes.md"), "x").expect("write");
    let mut session = session(&scratch);
    typing(&mut session, "read @My");

    handle_key(&mut session, key(KeyCode::Tab));
    assert_eq!(session.input(), "read @My\\ Documents/");

    typing(&mut session, "no");
    handle_key(&mut session, key(KeyCode::Tab));
    assert_eq!(session.input(), "read @My\\ Documents/notes.md ");

    let sent = match handle_key(&mut session, key(KeyCode::Enter)) {
        Action::Submit(sent) => sent,
        other => panic!("the line was not sent: {other:?}"),
    };
    assert_eq!(
        bravebot_mentions::referenced(&sent),
        vec!["My Documents/notes.md".to_string()]
    );
}

/// A file whose name is another's with a no-break space at its end is not offered, since a line
/// is trimmed when it is sent and would name the other file. Only the file that can be named is
/// listed, so a pick names the file it shows.
#[test]
fn a_name_ending_in_a_no_break_space_is_not_offered_beside_the_name_it_looks_like() {
    let scratch = Scratch::new("no-break-space-trailing");
    std::fs::write(scratch.path.join("a.md"), "x").expect("write");
    std::fs::write(scratch.path.join("a.md\u{a0}"), "x").expect("write");
    let mut session = session(&scratch);
    typing(&mut session, "read @a.md");
    assert_eq!(offered_starting(&session, "a.md"), vec!["a.md"]);

    handle_key(&mut session, key(KeyCode::Tab));
    assert_eq!(session.input(), "read @a.md ");
    let sent = match handle_key(&mut session, key(KeyCode::Enter)) {
        Action::Submit(sent) => sent,
        other => panic!("the line was not sent: {other:?}"),
    };
    assert_eq!(
        bravebot_mentions::referenced(&sent),
        vec!["a.md".to_string()]
    );
}

/// A name holding a no-break space beside the same name with a space completes to the escaped
/// form, and the line that is sent names the file picked. Without the escape the line ends the
/// reference at the no-break space and names a different path.
#[test]
fn a_name_with_a_no_break_space_completes_to_a_reference_that_names_it() {
    let scratch = Scratch::new("no-break-space-inside");
    std::fs::write(scratch.path.join("a b.md"), "x").expect("write");
    std::fs::write(scratch.path.join("a\u{a0}b.md"), "x").expect("write");
    let mut session = session(&scratch);
    typing(&mut session, "read @a");
    assert_eq!(
        offered_starting(&session, "a"),
        vec!["a b.md", "a\u{a0}b.md"]
    );

    handle_key(&mut session, key(KeyCode::Down));
    handle_key(&mut session, key(KeyCode::Tab));
    assert_eq!(session.input(), "read @a\\\u{a0}b.md ");

    let sent = match handle_key(&mut session, key(KeyCode::Enter)) {
        Action::Submit(sent) => sent,
        other => panic!("the line was not sent: {other:?}"),
    };
    assert_eq!(
        bravebot_mentions::referenced(&sent),
        vec!["a\u{a0}b.md".to_string()]
    );
}

/// A name ending in a backslash is written with two, so the space the completion adds does not
/// join it as an escaped space and the line that is sent names the file picked.
#[cfg(unix)]
#[test]
fn a_name_ending_in_a_backslash_completes_to_a_reference_that_names_it() {
    let scratch = Scratch::new("trailing-backslash");
    std::fs::write(scratch.path.join("a.md"), "x").expect("write");
    std::fs::write(scratch.path.join("a.md\\"), "x").expect("write");
    let mut session = session(&scratch);
    typing(&mut session, "read @a.md");
    assert_eq!(offered_starting(&session, "a.md"), vec!["a.md", "a.md\\"]);

    handle_key(&mut session, key(KeyCode::Down));
    handle_key(&mut session, key(KeyCode::Tab));
    assert_eq!(session.input(), "read @a.md\\\\ ");

    let sent = match handle_key(&mut session, key(KeyCode::Enter)) {
        Action::Submit(sent) => sent,
        other => panic!("the line was not sent: {other:?}"),
    };
    assert_eq!(
        bravebot_mentions::referenced(&sent),
        vec!["a.md\\".to_string()]
    );
}

/// Enter completes a half-typed reference that already holds an escaped space, rather than sending
/// the fragment.
#[test]
fn enter_completes_a_half_typed_reference_past_an_escaped_space() {
    let scratch = Scratch::new("space-half");
    std::fs::write(scratch.path.join("my notes.md"), "x").expect("write");
    let mut session = session(&scratch);
    typing(&mut session, "explain @my\\ no");

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Redraw
    );
    assert_eq!(session.input(), "explain @my\\ notes.md ");
}

fn two_sessions() -> Vec<bravebot_mentions::Entry> {
    vec![
        bravebot_mentions::session_entry("aaa-111", "fix the build", "2 days ago".to_string()),
        bravebot_mentions::session_entry("bbb-222", "rename the parser", "3 days ago".to_string()),
    ]
}

fn offered_paths(session: &Session) -> Vec<String> {
    match session.offered() {
        bravebot_tui::state::Offered::Files(entries) => {
            entries.into_iter().map(|e| e.path).collect()
        }
        other => panic!("files were not offered: {other:?}"),
    }
}

/// The offered files that begin with `prefix`, which come before the names a fragment finds below
/// the root (NAME-11).
fn offered_starting(session: &Session, prefix: &str) -> Vec<String> {
    let mut paths = offered_paths(session);
    paths.retain(|path| path.starts_with(prefix));
    paths
}

/// NAME-10: an `@` offers the past sessions after the files, and a title or an id narrows them.
#[test]
fn an_at_sign_offers_past_sessions_after_the_files() {
    let scratch = Scratch::new("sessions-offered");
    let mut session = session(&scratch);
    typing(&mut session, "carry on from @");
    session.settle_sessions(two_sessions);

    assert_eq!(
        offered_paths(&session),
        [
            "src/",
            "Cargo.toml",
            "README.md",
            "session:aaa-111",
            "session:bbb-222"
        ]
    );

    typing(&mut session, "pars");
    session.settle_sessions(|| panic!("the sessions were read again for one more letter"));
    assert_eq!(offered_paths(&session), ["session:bbb-222"]);
}

/// NAME-10: a word narrows the sessions by their title and not by the age and bound drawn beside
/// them, which every row shares.
#[test]
fn a_word_narrows_sessions_by_title_and_not_by_what_is_drawn_beside_them() {
    let scratch = Scratch::new("sessions-title-only");
    let mut session = session(&scratch);
    typing(&mut session, "@days");
    session.settle_sessions(two_sessions);
    assert!(
        !session.is_completing(),
        "the age beside a row narrowed the list: {:?}",
        session.offered()
    );
}

/// NAME-10: the sessions are not read until a reference is typed and are let go once it is done,
/// so a line with no `@` costs no directory scan per frame.
#[test]
fn the_sessions_are_read_only_while_a_reference_is_typed() {
    let scratch = Scratch::new("sessions-lazy");
    let mut session = session(&scratch);
    typing(&mut session, "no reference here");
    session.settle_sessions(|| panic!("read for a line with no reference"));
    typing(&mut session, " @");
    session.settle_sessions(two_sessions);
    assert!(offered_paths(&session).contains(&"session:aaa-111".to_string()));
    typing(&mut session, "README.md ");
    session.settle_sessions(|| panic!("a finished reference reads nothing"));
    typing(&mut session, "@");
    let mut reads = 0;
    session.settle_sessions(|| {
        reads += 1;
        Vec::new()
    });
    assert_eq!(
        reads, 1,
        "the held list was not let go when the reference ended"
    );
}

/// NAME-10: Tab completes a session to the reference that names it, with the space a finished
/// reference gets, and Enter sends a line ending in a finished one rather than completing it away.
#[test]
fn tab_completes_a_session_and_enter_sends_it_finished() {
    let scratch = Scratch::new("sessions-complete");
    let mut session = session(&scratch);
    typing(&mut session, "carry on from @session:bb");
    session.settle_sessions(two_sessions);
    assert_eq!(handle_key(&mut session, key(KeyCode::Tab)), Action::Redraw);
    assert_eq!(session.input(), "carry on from @session:bbb-222 ");
    assert!(bravebot_mentions::referenced(session.input()).is_empty());
    assert_eq!(
        bravebot_mentions::referenced_sessions(session.input()),
        ["bbb-222"]
    );

    let mut session = self::session(&scratch);
    typing(&mut session, "carry on from @session:aaa-111");
    session.settle_sessions(two_sessions);
    assert!(
        matches!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::Submit(_)
        ),
        "Enter completed a finished session reference instead of sending it"
    );
}

/// A session that surveys the files a line names, as the event loop sets one up.
fn surveying(scratch: &Scratch) -> Session {
    let workspace = bravebot_agent::Workspace::new(&scratch.path).expect("workspace");
    let mut session = Session::new("none").in_workspace(&scratch.path);
    session.adopt_workspace(&workspace);
    session
}

/// Every notice the session has added to the transcript, joined.
fn noticed(session: &Session) -> String {
    session
        .transcript
        .iter()
        .map(|entry| entry.text.clone())
        .collect::<Vec<_>>()
        .join("\n")
}

/// NAME-9: a name that is not a file in the project is refused at Enter. The line stays as typed,
/// nothing is sent, and the notice names the `@name` and says to remove the `@`. Rejects sending
/// the line and leaving the turn to fail on the read after the prompt was already sent.
#[test]
fn enter_refuses_a_name_that_is_not_a_file_and_keeps_the_line() {
    let scratch = Scratch::new("refuse-missing");
    let mut session = surveying(&scratch);
    typing(&mut session, "explain @alice now");

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Redraw
    );
    assert_eq!(session.input(), "explain @alice now");
    let said = noticed(&session);
    assert!(
        said.contains("@alice is not a file in this project"),
        "{said}"
    );
    assert!(said.contains("Remove the @"), "{said}");
}

/// NAME-9: a directory named without its slash, a binary file and a path outside the project are
/// each refused, by the same survey the desktop app uses. Rejects a check that stops at whether
/// the name exists, which lets these through to fail the turn.
#[test]
fn enter_refuses_a_directory_a_binary_file_and_a_path_outside_the_project() {
    let scratch = Scratch::new("refuse-kinds");
    std::fs::write(scratch.path.join("blob.bin"), b"\0\0binary").expect("write");
    let outside = scratch.path.with_file_name(format!(
        "bravebot-references-{}-outside.txt",
        std::process::id()
    ));
    std::fs::write(&outside, "elsewhere").expect("write");
    let outside_name = format!(
        "../{}",
        outside.file_name().and_then(|n| n.to_str()).expect("name")
    );

    for (name, why) in [
        ("src", "is not a file in this project"),
        ("blob.bin", "is not a text file"),
        (outside_name.as_str(), "is not a file in this project"),
    ] {
        let mut session = surveying(&scratch);
        let line = format!("explain @{name} now");
        typing(&mut session, &line);

        let pressed = handle_key(&mut session, key(KeyCode::Enter));
        assert_eq!(pressed, Action::Redraw, "{name} was sent");
        assert_eq!(session.input(), line, "{name} changed the line");
        let said = noticed(&session);
        assert!(said.contains(&format!("@{name} {why}")), "{name}: {said}");
    }
    let _ = std::fs::remove_file(outside);
}

/// NAME-9: every name that fails is named, once, and a name that is fine is not.
#[test]
fn the_refusal_names_each_bad_name_and_not_the_good_one() {
    let scratch = Scratch::new("refuse-each");
    let mut session = surveying(&scratch);
    typing(&mut session, "compare @alice with @README.md and @bob now");

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Redraw
    );
    let said = noticed(&session);
    assert!(said.contains("@alice is not a file"), "{said}");
    assert!(said.contains("@bob is not a file"), "{said}");
    assert!(!said.contains("@README.md"), "{said}");
}

/// NAME-9, PASTE-1: the refused line keeps a folded paste as its marker, with the paste's words
/// still stored behind it, so nothing typed or pasted is lost. Rejects a refusal that clears the
/// box or that puts the expanded words back in it.
#[test]
fn a_refused_line_keeps_its_folded_paste() {
    let scratch = Scratch::new("refuse-paste");
    let mut session = surveying(&scratch);
    typing(&mut session, "explain @alice now ");
    handle_paste(&mut session, "one\ntwo\nthree\nfour\n");
    let before = session.input().to_string();
    assert!(before.contains("Pasted text #1"), "{before}");

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Redraw
    );
    assert_eq!(session.input(), before);
    assert!(
        session
            .unfolded(session.input())
            .contains("one\ntwo\nthree"),
        "the paste's words were lost"
    );
}

/// NAME-1, NAME-6, NAME-7: a line whose names are all real text files is sent as before, and so
/// are lines that name nothing: a directory with its slash, a bare `@` and an address. Rejects a
/// refusal that fires on every line holding an `@`.
#[test]
fn enter_still_sends_lines_that_name_real_files_or_nothing() {
    let scratch = Scratch::new("send-good");
    for line in [
        "compare @src/main.rs with @README.md now",
        "look in @src/ now",
        "a lone @ now",
        "mail me@example.com now",
    ] {
        let mut session = surveying(&scratch);
        typing(&mut session, line);
        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::Submit(line.to_string()),
            "{line}"
        );
    }
}

/// NAME-9, REFER-6: a name under a reference's alias is surveyed in the directory it resolves
/// to. A file there is sent, and a name there that is no file is refused under the name the
/// person typed. Rejects a survey that skips the alias resolution, which would refuse the real
/// file as absent from the project, or one that resolves and then lets a missing file through.
#[test]
fn a_name_under_a_reference_alias_is_surveyed_in_its_directory() {
    let scratch = Scratch::new("refuse-alias");
    let library = scratch.path.with_file_name(format!(
        "bravebot-references-{}-library",
        std::process::id()
    ));
    std::fs::create_dir_all(library.join("src")).expect("create");
    std::fs::write(library.join("src/lexer.rs"), "fn lex() {}\n").expect("write");
    let workspace = bravebot_agent::Workspace::new(&scratch.path)
        .expect("workspace")
        .with_references(
            &[bravebot_config::Reference {
                alias: "parser".to_string(),
                path: library.display().to_string(),
                description: None,
            }],
            &[],
        );
    assert_eq!(
        workspace.reference_sources().len(),
        1,
        "the reference opened"
    );
    let aliased = |line: &str| {
        let mut session = Session::new("none").in_workspace(&scratch.path);
        session.adopt_workspace(&workspace);
        typing(&mut session, line);
        let pressed = handle_key(&mut session, key(KeyCode::Enter));
        (pressed, noticed(&session))
    };

    let line = "read @parser/src/lexer.rs now";
    assert_eq!(aliased(line).0, Action::Submit(line.to_string()));

    let (pressed, said) = aliased("read @parser/src/parser.rs now");
    assert_eq!(pressed, Action::Redraw);
    assert!(
        said.contains("@parser/src/parser.rs is not a file"),
        "{said}"
    );
    let _ = std::fs::remove_dir_all(library);
}

/// NAME-9: a refused line keeps the pictures pasted into it, marker and bytes, so the person
/// corrects the name and sends the same request. Rejects a refusal that submits first (which
/// moves the staged attachments to the sent set) or clears what was staged.
#[test]
fn a_refused_line_keeps_its_attachments() {
    let scratch = Scratch::new("refuse-attached");
    let mut session = surveying(&scratch);
    typing(&mut session, "explain @alice with ");
    session.attach(bravebot_tui::clipboard::Image {
        media_type: "image/png",
        bytes: vec![1, 2, 3],
    });
    let before = session.input().to_string();
    assert_eq!(session.pasted_named(&before).len(), 1);

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Redraw
    );
    assert_eq!(session.input(), before);
    assert_eq!(session.pasted_named(session.input()).len(), 1);
    assert!(session.sent_attachments().is_empty());
}

/// NAME-9, PASTE-1: a handle that is only inside a folded paste is surveyed, because the words
/// are what is sent and the box shows the marker. The issue's case: `cc @alice` in a paste.
/// Rejects a survey of the box text, which sees only `[Pasted text #1 +4 lines]` and lets the
/// line through to fail the turn.
#[test]
fn a_handle_inside_a_folded_paste_is_refused_and_the_marker_stays() {
    let scratch = Scratch::new("refuse-in-paste");
    let mut session = surveying(&scratch);
    typing(&mut session, "propose labels for these ");
    handle_paste(
        &mut session,
        "one\ncc @alice for thoughts\n@README.md\nfour\n",
    );
    let before = session.input().to_string();
    assert!(
        !before.contains("@alice"),
        "the paste was not folded: {before}"
    );

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Redraw
    );
    assert_eq!(session.input(), before);
    let said = noticed(&session);
    assert!(said.contains("@alice is not a file"), "{said}");
    assert!(!said.contains("@README.md"), "{said}");
}

/// NAME-1: a line the driver composes names nothing. `/help` is sent with a prompt the driver
/// wrote around the person's question; the survey reads the box, which `begin_turn` has emptied, so
/// a handle in the question is not asked of the project at that point. Rejects a survey that reads
/// the prompt of the turn starting, or the last transcript entry, instead of the box.
#[test]
fn a_prompt_the_driver_composed_is_never_surveyed() {
    let scratch = Scratch::new("driver-line");
    let mut session = surveying(&scratch);
    let _ = session.start_help("what did @alice mean");
    assert_eq!(session.input(), "");
    assert!(!session.refuse_unsendable_names());
    assert!(!noticed(&session).contains("is not a file"));
}

/// A session nobody gave a workspace to survey in sends as it did, so a bare `Session::new` in
/// another test or caller is not made to refuse everything.
#[test]
fn a_session_with_no_workspace_surveys_nothing() {
    let mut session = Session::new("none");
    typing(&mut session, "explain @alice now");
    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Submit("explain @alice now".to_string())
    );
}

/// NAME-9: while a turn works, a line naming a missing file is refused and not queued, and a line
/// naming a real file is queued as before. Rejects checking only at rest, where a queued line
/// would be sent later and fail the turn it starts.
#[test]
fn while_a_turn_works_a_bad_name_is_refused_and_not_queued() {
    use bravebot_tui::app::handle_key_while_working;
    let scratch = Scratch::new("refuse-working");
    let mut session = surveying(&scratch);
    session.status = bravebot_tui::Status::Working;

    for c in "explain @alice now".chars() {
        handle_key_while_working(&mut session, key(KeyCode::Char(c)));
    }
    handle_key_while_working(&mut session, key(KeyCode::Enter));
    assert!(session.queued.is_empty(), "the line was queued");
    assert_eq!(session.input(), "explain @alice now");
    assert!(noticed(&session).contains("@alice is not a file"));

    let mut session = surveying(&scratch);
    session.status = bravebot_tui::Status::Working;
    for c in "explain @README.md now".chars() {
        handle_key_while_working(&mut session, key(KeyCode::Char(c)));
    }
    handle_key_while_working(&mut session, key(KeyCode::Enter));
    assert_eq!(session.queued.len(), 1, "a real file was not queued");
}
