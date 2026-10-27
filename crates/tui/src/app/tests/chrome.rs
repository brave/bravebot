//! MOUSE-24 and MOUSE-27: the words the interface draws for a click are drawn only where the mouse
//! is reported, since nothing else reaches them.

use super::*;

/// MOUSE-27. Rejects: the word that copies a fenced block drawn where `tui.mouse` is false, where no
/// click reaches it and the terminal's own selection takes it for text. The frame is drawn with and
/// without the mouse by one session, so a layout kept from one is not taken for the other's, and the
/// block is copied by `/copy block` either way.
#[test]
fn the_word_that_copies_a_block_is_drawn_only_where_the_mouse_is_reported() {
    let mut session = Session::new("none");
    session.transcript.push(crate::state::Entry::assistant(
        "run:\n```sh\nmake\n```",
        Vec::new(),
    ));
    for (configured, drawn_word) in [(None, true), (Some(false), false), (Some(true), true)] {
        session.adopt_mouse(configured);
        let buffer = drawn_at(&mut session, 80, 24);
        assert_eq!(
            !cells_of(&buffer, "copy").is_empty(),
            drawn_word,
            "tui.mouse {configured:?}"
        );
        assert_eq!(
            session
                .laid
                .targets
                .iter()
                .any(|target| matches!(target.hit, crate::state::Hit::Block(..))),
            drawn_word,
            "tui.mouse {configured:?}"
        );
    }
    assert_eq!(session.blocks_newest_first().collect::<Vec<_>>(), ["make"]);
}

/// MOUSE-24. Rejects: the way back drawn in a delegate's header where `tui.mouse` is false, which
/// names a click nothing reports. The words of the header stay, and the keys that leave the view are
/// the ones that do.
#[test]
fn the_way_back_out_of_a_delegates_view_is_drawn_only_where_the_mouse_is_reported() {
    let (mut session, with) = a_finished_delegates_view();
    assert!(!cells_of(&with, "\u{2039} back").is_empty());
    session.adopt_mouse(Some(false));
    let without = drawn(&mut session);
    assert!(cells_of(&without, "\u{2039} back").is_empty());
    assert!(
        !cells_of(&without, "reader").is_empty(),
        "the header lost its words"
    );
    assert!(
        session.laid.targets.iter().all(|target| {
            target.hit != crate::state::Hit::Back || target.space != crate::state::Space::Screen
        }),
        "the label is still a place to click"
    );
    session.adopt_mouse(None);
    let with_again = drawn(&mut session);
    assert!(!cells_of(&with_again, "\u{2039} back").is_empty());
}
