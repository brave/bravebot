//! Which build this is, for a record that will be read after the fact.
//!
//! A session record carries the stamp, and so does `bravebot --version`, so a transcript can be
//! matched to the code that wrote it rather than inferred from its own symptoms. Every front end
//! writes the same string, which is why the stamp is taken here rather than in one of them: two
//! surfaces computing it separately is two answers to agree by coordination, and a surface that
//! draws nothing would be linking a terminal library to ask.

#![forbid(unsafe_code)]

/// What this build is: the version, the commit it was built from, and whether the tree had
/// uncommitted changes at the time.
///
/// Written into every session record, so a transcript read later can be matched to the code that
/// produced it rather than inferred from its own symptoms.
pub const BUILD: &str = env!("BRAVEBOT_BUILD");

#[cfg(test)]
mod tests {
    use super::BUILD;

    /// The stamp is what a session record is matched against later, so a stamp that does not name
    /// the version says nothing about which code wrote the transcript in front of you.
    #[test]
    fn the_build_stamp_names_the_version_it_was_built_from() {
        assert!(
            BUILD.starts_with(env!("CARGO_PKG_VERSION")),
            "the build stamp does not name the version: {BUILD}"
        );
    }
}
