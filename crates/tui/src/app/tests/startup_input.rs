//! INPUT-52 in the terminal: where some of what was typed while the terminal was asked about itself
//! at startup could not be kept, the session says so on the screen.

use super::*;

/// The screen the session draws, one string per row.
fn screen(session: &mut Session) -> Vec<String> {
    let buffer = drawn(session);
    let area = buffer.area;
    (area.top()..area.bottom())
        .map(|y| {
            (area.left()..area.right())
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect()
}

/// INPUT-52. Rejects: the note left unmade, made where nothing was lost, or made without reaching
/// the screen.
#[test]
fn a_session_that_lost_keys_typed_at_startup_says_so_on_the_screen() {
    let said = t!(session_startup_input_lost);
    let shown = |session: &mut Session| screen(session).iter().any(|row| row.contains(said));

    let mut session = Session::new("none");
    note_startup_input_lost(&mut session, 0);
    assert!(
        !shown(&mut session),
        "nothing was lost, and the screen says something was"
    );

    let mut session = Session::new("none");
    note_startup_input_lost(&mut session, 2);
    assert!(
        shown(&mut session),
        "two bytes were lost and the screen does not say so"
    );
}
