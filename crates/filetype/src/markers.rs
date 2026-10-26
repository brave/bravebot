//! What stands in a line for a marker when the thing it marks cannot go with the line (DROP-8).
//!
//! A turn already running fixed the shape of its context before it read anything, so it cannot be
//! handed a file or a picture. A dropped file becomes its name, which the planner can go and read
//! through the gate any file is read through. A picture has no such recourse and becomes a
//! sentence saying so. The terminal and the desktop window both put a line through these, so the
//! two hand the planner the same words.
//!
//! Not from the message catalog: the planner reads this, so it is part of what the model is given
//! and not something a person is being told.

/// A line with each marker in `named` replaced by the name beside it.
pub fn put_back_to_names<'a>(
    line: &str,
    named: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> String {
    let mut settled = line.to_string();
    for (marker, name) in named {
        settled = settled.replace(marker, name);
    }
    settled
}

/// What stands where a picture was pasted and cannot travel with the line.
///
/// Why it cannot travel is the caller's to say, because a picture misses a turn already running
/// for one reason and a command for another, and the one thing this must not do is guess.
pub fn picture_in_words(marker: &str, why: &str) -> String {
    format!(
        "({marker} was pasted here, but it cannot be shown to you: {why}. Ask for it again if you \
         need to see it.)"
    )
}

/// A line with each marker in `markers` replaced by the sentence that takes the picture's place.
pub fn put_back_to_words<'a>(
    line: &str,
    markers: impl IntoIterator<Item = &'a str>,
    why: &str,
) -> String {
    let mut settled = line.to_string();
    for marker in markers {
        settled = settled.replace(marker, &picture_in_words(marker, why));
    }
    settled
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dropped_file_becomes_its_name_and_text_around_it_is_kept() {
        let line = put_back_to_names("look at [File #1] now", [("[File #1]", "src/a.rs")]);
        assert_eq!(line, "look at src/a.rs now");
    }

    #[test]
    fn a_picture_becomes_a_sentence_naming_its_marker_and_the_reason() {
        let line = put_back_to_words("see [Image #2]", ["[Image #2]"], "it is too late");
        assert_eq!(
            line,
            "see ([Image #2] was pasted here, but it cannot be shown to you: it is too late. Ask \
             for it again if you need to see it.)"
        );
    }
}
