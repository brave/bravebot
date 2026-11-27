use super::*;

/// A build log's verdict is at the end and its first error is near the beginning, so a
/// sample that kept only the front would answer neither question a reader of one has.
#[test]
fn a_capped_output_keeps_its_head_and_its_tail() {
    let mut log = String::new();
    while log.len() <= OUTPUT_CAP * 2 {
        log.push_str("a line in the middle of a long build log\n");
    }
    let log = format!("the first line\n{log}the last line\n");

    let sample = bounded(&log, OUTPUT_CAP).expect("an output twice the cap is capped");

    assert!(sample.len() < log.len(), "nothing was dropped");
    assert!(
        sample.starts_with("the first line\n"),
        "the head went: {}",
        &sample[..40]
    );
    assert!(sample.ends_with("the last line\n"), "the tail went");
    // In the driver's own words, so a planner knows it is reading a sample rather than a
    // short result. How many bytes went, because that is what says how much is missing.
    assert!(
        sample.contains("the middle of this output was dropped"),
        "the sample does not say that it is one"
    );
}

/// CMDLINE-11: the line between head and tail names how many bytes and lines went, and the
/// figures are those of the dropped middle. Thirty lines of ten bytes under a cap of a hundred
/// keep five lines each end and drop twenty, two hundred bytes, so a figure taken from either
/// kept part, from the whole output or from the cap is a different number.
#[test]
fn a_capped_output_says_how_many_bytes_and_lines_went() {
    let log: String = (10..40).map(|n| format!("line {n}..\n")).collect();
    assert_eq!(log.len(), 300);

    let sample = bounded(&log, 100).expect("an output three times the cap is capped");

    assert!(
        sample.contains(
            "(the middle of this output was dropped: 200 bytes, about 20 lines, from byte 50.)"
        ),
        "the dropped middle is not counted: {sample}"
    );
    assert!(sample.starts_with("line 10..\n"), "the head went");
    assert!(sample.ends_with("line 39..\n"), "the tail went");
}

/// The cap is a bound on a long result, not a transformation every result goes through: a
/// planner told that a two-line answer was cut short would narrow a command that answered it.
#[test]
fn an_output_inside_the_cap_is_left_alone() {
    assert!(bounded("two\nlines\n", OUTPUT_CAP).is_none());
    assert!(bounded(&"x".repeat(OUTPUT_CAP), OUTPUT_CAP).is_none());
}

/// The cap the caller was given is the one that holds, in both directions: a person who raised
/// it gets the bytes they paid for, and one who lowered it is not handed the built-in figure.
/// Without this the key parses, `doctor` reports it, and the output is cut where it always was.
#[test]
fn output_is_cut_to_the_cap_it_was_given() {
    let log = "x".repeat(OUTPUT_CAP * 4);

    assert!(
        bounded(&log, OUTPUT_CAP * 8).is_none(),
        "a raised cap still cut an output that fits under it"
    );
    let lowered = bounded(&log, 512).expect("an output past a lowered cap is cut");
    let default = bounded(&log, OUTPUT_CAP).expect("an output past the built-in cap is cut");
    assert!(
        lowered.len() < default.len(),
        "a lowered cap kept as much as the built-in one: {} against {}",
        lowered.len(),
        default.len()
    );

    // A cap an odd number of bytes wide, and a character that does not fit in one byte: the cut
    // lands on a character boundary or the turn ends on output nobody chose.
    let wide = "\u{3053}\u{3093}\u{306b}\u{3061}\u{306f}".repeat(100);
    let cut = bounded(&wide, 101).expect("an output past a narrow cap is cut");
    assert!(cut.contains("the middle of this output was dropped"));
}

/// A page of kept output starts and stops on a character, even where the offset or the cap
/// lands inside one, and says where the next page begins. A cap narrower than a character
/// still moves forward, or a planner paging by the offset it was given would never finish.
#[test]
fn a_page_starts_and_stops_on_a_character_and_names_the_next() {
    let slot = SlotId::new("ref:3");
    // Boundaries at 0, 1, 2, 5, 6 and 7.
    let text = "ab\u{3053}cd";
    let page = |offset, cap| page_of(text, &slot, offset, cap);
    assert_eq!(
        page(0, 3),
        "ab\n\n(bytes 0 to 2 of 7 in ref:3; read_output with offset 2 gives the next part.)"
    );
    assert_eq!(
        page(3, 4),
        "\u{3053}c\n\n(bytes 2 to 6 of 7 in ref:3; read_output with offset 6 gives the next \
         part.)"
    );
    assert_eq!(
        page(2, 1),
        "\u{3053}\n\n(bytes 2 to 5 of 7 in ref:3; read_output with offset 5 gives the next \
         part.)"
    );
    assert_eq!(
        page(5, 100),
        "cd\n\n(bytes 5 to 7 of 7 in ref:3; which is the end of it.)"
    );
    assert_eq!(
        page(7, 100),
        "(ref:3 holds 7 bytes, so nothing starts at byte 7.)"
    );
}

/// A searched page numbers every line it shows with the byte that line starts at, so the
/// planner can read on from a match with one offset call, and separates runs of lines that
/// are not adjacent. The cap on matches is reported with the byte the next one starts at
/// rather than silently dropping it.
#[test]
fn matching_lines_carry_the_byte_each_starts_at() {
    let slot = SlotId::new("ref:3");
    let text = "alpha\nbeta\ngamma\ndelta\nepsilon\nzeta\neta\ntheta\niota\nkappa\n";
    let one = crate::regex::Regex::compile("gamma").expect("compiles");
    let found = matches_of(text, &slot, std::slice::from_ref(&one), 4096);
    // alpha 0, beta 6, gamma 11, delta 17, epsilon 23.
    assert!(found.contains("1-0- alpha\n"), "{found}");
    assert!(found.contains("3:11: gamma\n"), "{found}");
    assert!(found.contains("5-23- epsilon\n"), "{found}");
    assert!(!found.contains("zeta"), "{found}");
    assert!(
        found.contains("1 matching line in ref:3, 1 shown."),
        "{found}"
    );

    let far = crate::regex::Regex::compile("alpha|kappa").expect("compiles");
    let two = matches_of(text, &slot, std::slice::from_ref(&far), 4096);
    assert!(
        two.contains("--\n"),
        "two distant matches ran together: {two}"
    );
    assert!(two.contains("2 matching lines in ref:3, 2 shown."), "{two}");

    let nothing = crate::regex::Regex::compile("omega").expect("compiles");
    assert_eq!(
        matches_of(text, &slot, std::slice::from_ref(&nothing), 4096),
        format!(
            "(nothing in ref:3 matches, out of 10 lines in {} bytes.)",
            text.len()
        )
    );

    let every = crate::regex::Regex::compile("a|e|i|o|u").expect("compiles");
    let cut = matches_of(text, &slot, std::slice::from_ref(&every), 1);
    assert!(
        cut.contains("the next from byte"),
        "a result stopped by the cap does not say where to carry on: {cut}"
    );
}

/// OUTPUT-5: no more bytes than the cap, however long the lines around a match are.
///
/// The cap used to be checked only before a match started, so one match whose own two lines
/// of context ran well past it still wrote all of them in full: a check at the start of a
/// loop bounds how many times the loop runs, not what one pass through it writes. A single
/// long line already overshoots on its own, which is what this holds a line length past the
/// cap to; two matches far enough apart to need a `--` separator confirms the second is
/// bounded by what the first already spent rather than by its own share of the cap.
#[test]
fn a_match_whose_context_is_long_is_still_held_to_the_cap() {
    let slot = SlotId::new("ref:4");
    let long = "x".repeat(500);
    let text = format!("{long}\nNEEDLE one\n{long}\n\n{long}\nNEEDLE two\n{long}\n");
    let pattern = crate::regex::Regex::compile("NEEDLE").expect("compiles");

    // Large enough to reach past the long line in front of the match, so the needle itself
    // is still in what is kept, and small enough that the match's own trailing context and
    // the second match stay past it: what the cap bounds either side is still visible.
    let cap = 600;
    let found = matches_of(&text, &slot, std::slice::from_ref(&pattern), cap);
    let shown = found.split("\n(").next().unwrap_or(&found);
    assert!(
        shown.len() <= cap,
        "a match's own context overshot the cap of {cap}: {} bytes in {found:?}",
        shown.len()
    );
    assert!(found.contains("NEEDLE one"), "{found}");
    assert!(
        !found.contains("NEEDLE two"),
        "a second match was drawn past the cap: {found}"
    );

    // A cap smaller than the first match's own context still returns something, cut to a
    // character boundary, rather than nothing: the floor `page_of` holds a page to.
    let one_byte = matches_of(&text, &slot, std::slice::from_ref(&pattern), 1);
    let shown = one_byte.split("\n(").next().unwrap_or(&one_byte);
    assert!(!shown.is_empty(), "a cap of 1 came back with nothing shown");
    assert!(
        shown.len() < long.len(),
        "a cap of 1 was not held to a character boundary within it: {} bytes",
        shown.len()
    );
}
