//! Where the `@` names of a turn are read from (NAME-12).
//!
//! A turn reads the files it vouches for and the sessions it quotes from the line the box showed
//! when the line left the box, with every folded paste still a marker. Afterwards a typed word and
//! a pasted one cannot be told apart, so no later step can recover the line from the words.
//!
//! The type has no constructor that takes words outside the session. [`NamesFrom::the_box_line`] is
//! visible to the session's own code only, at the points where a line leaves the box, and
//! [`NamesFrom::nothing`] says in the code that the turn reads no name. [`Session::begin_turn`] takes
//! one, a [`Commanded`] carries one, and the readers of a turn's names take one in place of the
//! words, so a path that starts a turn has to say where its names come from, and a reader cannot be
//! handed the unfolded prompt.
//!
//! [`Session::begin_turn`]: super::Session
//! [`Commanded`]: super::Commanded

/// The line a turn reads its names from, or nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamesFrom {
    line: Option<String>,
}

impl Default for NamesFrom {
    fn default() -> Self {
        Self::nothing()
    }
}

impl NamesFrom {
    /// No line: the turn reads no name.
    ///
    /// For a turn the driver begins from its own sentences, and for a command a click stands for,
    /// which was never typed in the box. Nothing is searched for, since every word of such a turn
    /// could have come from a paste.
    pub fn nothing() -> Self {
        Self { line: None }
    }

    /// The line as the box showed it, markers included.
    ///
    /// Only the session calls this, at the point a line leaves the box: a line sent, a command
    /// taken, a queued line, a queued command, a loop's own line.
    pub(super) fn the_box_line(line: impl Into<String>) -> Self {
        Self {
            line: Some(line.into()),
        }
    }

    /// A line standing for the one the box showed, for a test of a reader of names that has no
    /// session to take the line from.
    #[cfg(test)]
    pub(crate) fn the_line_in_a_test(line: impl Into<String>) -> Self {
        Self::the_box_line(line)
    }

    /// The lines of a batch the queue sends as one turn, joined as their prompts are.
    ///
    /// A part that is nothing contributes no words, so a batch holding one reads names from the
    /// others only.
    pub(super) fn joined(parts: Vec<NamesFrom>) -> Self {
        if parts.iter().all(|part| part.line.is_none()) {
            return Self::nothing();
        }
        let lines: Vec<String> = parts
            .into_iter()
            .map(|part| part.line.unwrap_or_default())
            .collect();
        Self::the_box_line(lines.join("\n"))
    }

    /// The line to read names from, empty where there is none.
    #[cfg(test)]
    pub(crate) fn line(&self) -> &str {
        self.line.as_deref().unwrap_or("")
    }

    /// The line a stopped turn puts back in the box, where there is one.
    pub(super) fn the_line(&self) -> Option<&str> {
        self.line.as_deref()
    }

    /// The files the line names with `@`, each resolved as a reference alias resolves it (REFER-6).
    ///
    /// The one place the words of a turn are read for file names. Read from the line as the box
    /// showed it, so the words behind a folded paste marker name nothing. A turn with no line names
    /// nothing.
    pub fn files(
        &self,
        root: &std::path::Path,
        sources: &bravebot_mentions::Sources,
    ) -> Vec<String> {
        bravebot_mentions::referenced(self.line.as_deref().unwrap_or(""))
            .into_iter()
            .map(|name| {
                // A reference directory with no text spelling leaves the name as typed. Nothing in
                // the workspace has that name, so the turn's read refuses it by name, and no other
                // directory is read in its place.
                bravebot_mentions::resolved(root, &name, sources).unwrap_or(name)
            })
            .collect()
    }

    /// The ids of the sessions the line names with `@session:<id>` (NAME-10), read as [`NamesFrom::files`]
    /// reads files.
    pub fn sessions(&self) -> Vec<String> {
        bravebot_mentions::referenced_sessions(self.line.as_deref().unwrap_or(""))
    }

    /// Whether there is no line to read names from.
    pub fn is_nothing(&self) -> bool {
        self.line.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::NamesFrom;

    /// The lines of a batch the queue sends as one turn are joined as their prompts are, so the turn
    /// reads the names of each. Rejects a join that keeps the first line only.
    #[test]
    fn the_lines_of_a_batch_are_read_as_the_lines_they_were_joined_from() {
        let joined = NamesFrom::joined(vec![
            NamesFrom::the_box_line("look at @one.md"),
            NamesFrom::the_box_line("then @two.md"),
        ]);
        assert_eq!(joined.line(), "look at @one.md\nthen @two.md");
        assert_eq!(
            joined.files(std::path::Path::new("."), &[]),
            vec!["one.md".to_string(), "two.md".to_string()]
        );
    }

    /// A part with no line contributes no words, and a batch of such parts reads nothing. Rejects a
    /// join that turns a part with no line into an empty line read as typed.
    #[test]
    fn a_part_with_no_line_adds_no_words_to_a_batch() {
        let joined = NamesFrom::joined(vec![
            NamesFrom::nothing(),
            NamesFrom::the_box_line("look at @one.md"),
        ]);
        assert_eq!(joined.line(), "\nlook at @one.md");
        assert!(!joined.is_nothing());

        let none = NamesFrom::joined(vec![NamesFrom::nothing(), NamesFrom::nothing()]);
        assert!(none.is_nothing());
        assert!(none.files(std::path::Path::new("."), &[]).is_empty());
        assert!(none.sessions().is_empty());
    }

    /// A line read for names gives the files with a reference alias resolved and the sessions with
    /// their ids, and a turn with no line gives neither.
    #[test]
    fn a_line_gives_its_files_and_sessions_and_no_line_gives_none() {
        let line = NamesFrom::the_box_line("see @src/lib.rs and @session:abc123");
        assert_eq!(
            line.files(std::path::Path::new("."), &[]),
            vec!["src/lib.rs".to_string()]
        );
        assert_eq!(line.sessions(), vec!["abc123".to_string()]);
        assert!(NamesFrom::default().is_nothing());
        assert!(
            NamesFrom::default()
                .files(std::path::Path::new("."), &[])
                .is_empty()
        );
    }
}
