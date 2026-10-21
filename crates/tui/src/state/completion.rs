//! Completion of the word at the caret: skills, sessions and arguments.

use super::*;

/// What a half-typed line could still become.
///
/// One kind at a time: a word opening with a slash is a command or a skill's name, and one opening
/// with an `@` is a file reference, so the keys that walk the list never have to ask which of the
/// two they are walking.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Offered {
    /// Nothing is being typed towards, so the list is closed.
    Nothing,
    /// The commands first and then the skills, walked as one list. Commands only where the word is
    /// the whole line, since anywhere else a command is a prompt.
    Slash {
        commands: Vec<crate::command_table::Command>,
        skills: Vec<crate::skills::Skill>,
    },
    Files(Vec<bravebot_mentions::Entry>),
    /// Every key and marker, listed under the box. Not a completion: there is nothing to choose,
    /// which is why the keys that walk a list leave this one alone.
    Shortcuts,
}

/// How long an answer from [`Session::offered`] is trusted without the line changing.
const OFFERED_FRESH: Duration = Duration::from_secs(2);

/// What [`Session::offered`] answered, with everything it answered from.
#[derive(Debug)]
pub(crate) struct OfferedFor {
    at: Instant,
    epoch: u64,
    status: Status,
    shortcuts: bool,
    shell: bool,
    input: String,
    workspace: std::path::PathBuf,
    reference_sources: Vec<(String, std::path::PathBuf)>,
    offered: Offered,
}

impl Session {
    /// Hold the skills a slash word could become while the line holds a slash word, and let them go
    /// once it holds none.
    ///
    /// Held for the whole line rather than for the word being typed, because the names are also
    /// what says a finished word is a skill, drawn in its own colour with what it takes after it.
    pub fn settle_skills(&mut self, resolve: impl FnOnce() -> Vec<crate::skills::Skill>) {
        let typing =
            !self.shell && self.status != Status::Working && crate::skills::mentioned(&self.input);
        match (typing, self.skills.is_some()) {
            (true, false) => {
                self.skills = Some(resolve());
                self.offered_epoch += 1;
            }
            (false, true) => {
                self.skills = None;
                self.offered_epoch += 1;
            }
            _ => {}
        }
    }

    /// Hold the past sessions an `@` could name while the line ends in a reference, and let them go
    /// once it does not.
    ///
    /// `resolve` returns a row for each session of the directory, newest first, and is called once
    /// per reference rather than once per key.
    pub fn settle_sessions(&mut self, resolve: impl FnOnce() -> Vec<bravebot_mentions::Entry>) {
        let typing = !self.shell
            && self.status != Status::Working
            && bravebot_mentions::typed_reference(&self.input).is_some();
        match (typing, self.past_sessions.is_some()) {
            (true, false) => {
                self.past_sessions = Some(resolve());
                self.offered_epoch += 1;
            }
            (false, true) => {
                self.past_sessions = None;
                self.offered_epoch += 1;
            }
            _ => {}
        }
    }

    /// The skills held while a slash word is being typed, and none otherwise.
    ///
    /// None while work runs, even if the line that started it was holding some: the loop that
    /// calls [`Session::settle_skills`] is not reached again until the work ends, so what was held
    /// when Enter was pressed would otherwise be drawn as recognised for the whole turn.
    pub fn held_skills(&self) -> &[crate::skills::Skill] {
        if self.status == Status::Working {
            return &[];
        }
        self.skills.as_deref().unwrap_or_default()
    }

    /// Forget what was last offered, so the next [`Session::offered`] reads the disk again.
    ///
    /// Called as each key is taken, which is what keeps a list that is up honest about a file that
    /// has gone since it was drawn, while the many callers a key press has share one reading.
    pub fn reread_offers(&mut self) {
        self.offered_epoch += 1;
    }

    /// What the half-typed line could still become: a command or a skill, or a file reference.
    ///
    /// Asked at least once a frame and by about twenty callers a key press, and answering reads a
    /// directory after an `@`, so the answer is kept for as long as everything it was worked out
    /// from is unchanged: the line, the mode, the directories and the skills and sessions held.
    /// Kept for [`OFFERED_FRESH`] at most, since the one thing that changes it unseen is the disk.
    pub fn offered(&self) -> Offered {
        self.offered_memo
            .get(
                |kept| {
                    kept.at.elapsed() < OFFERED_FRESH
                        && kept.epoch == self.offered_epoch
                        && kept.status == self.status
                        && kept.shortcuts == self.shortcuts
                        && kept.shell == self.shell
                        && kept.input == self.input
                        && kept.workspace == self.workspace
                        && kept.reference_sources == self.reference_sources
                },
                || OfferedFor {
                    at: Instant::now(),
                    epoch: self.offered_epoch,
                    status: self.status,
                    shortcuts: self.shortcuts,
                    shell: self.shell,
                    input: self.input.clone(),
                    workspace: self.workspace.clone(),
                    reference_sources: self.reference_sources.clone(),
                    offered: self.compute_offered(),
                },
            )
            .offered
            .clone()
    }

    /// Worked out afresh, reading whatever it has to from the disk.
    ///
    /// One of the two at most. Both are the last word, and a word opens with a slash or with an
    /// `@`, so nothing can be both.
    fn compute_offered(&self) -> Offered {
        // First, before either guard below. The list of keys is documentation somebody asked for by
        // pressing a key, not machinery for finishing the line, so neither a turn in flight nor a
        // command line has anything to say about it. It takes the place of what is offered, since
        // the two answer the same space and only one of them was asked for.
        //
        // Answered after the guards, the flag was set by the press and the list had nowhere to go,
        // so `?` during a turn did nothing on screen and then put the list up unasked when the turn
        // ended. A turn going somewhere the person did not want is when they most want a key.
        if self.shortcuts {
            return Offered::Shortcuts;
        }
        // A command typed while work runs is carried out or queued (CMD-8), so the words are
        // offered to be found. Skills and files are not: the skills are not read while work runs.
        if self.status == Status::Working {
            let commands = if self.shell {
                Vec::new()
            } else {
                crate::command_table::completions(&self.input)
            };
            return if commands.is_empty() {
                Offered::Nothing
            } else {
                Offered::Slash {
                    commands,
                    skills: Vec::new(),
                }
            };
        }
        // A command line is neither a slash command nor a sentence with a file reference in it.
        // `/usr/bin/env` and an address with an `@` in it are ordinary arguments here, and
        // completing either would rewrite the line under someone typing a path.
        if self.shell {
            return Offered::Nothing;
        }
        let commands = crate::command_table::completions(&self.input);
        let skills = match (crate::skills::typed(&self.input), &self.skills) {
            (Some(typed), Some(held)) => crate::skills::matching(held, typed),
            _ => Vec::new(),
        };
        if !commands.is_empty() || !skills.is_empty() {
            return Offered::Slash { commands, skills };
        }
        match bravebot_mentions::typed_reference(&self.input) {
            Some(typed) => {
                let mut entries =
                    bravebot_mentions::matching(&self.workspace, &typed, &self.reference_sources);
                // After the files: a session is named by a title or an id rather than walked
                // into, and a path being typed towards is what a person most often means.
                entries.extend(self.sessions_matching(&typed));
                if entries.is_empty() {
                    Offered::Nothing
                } else {
                    Offered::Files(entries)
                }
            }
            None => Offered::Nothing,
        }
    }

    /// The past sessions a half-typed reference could still become: those whose reference starts
    /// with it, and, for words that are no path, those whose title holds them.
    fn sessions_matching(&self, typed: &str) -> Vec<bravebot_mentions::Entry> {
        const MOST: usize = 10;
        let Some(held) = &self.past_sessions else {
            return Vec::new();
        };
        let wanted = typed.to_lowercase();
        held.iter()
            .filter(|row| {
                typed.is_empty()
                    || row.path.starts_with(typed)
                    || (!typed.contains('/')
                        && row
                            .title
                            .as_deref()
                            .is_some_and(|title| title.to_lowercase().contains(&wanted)))
            })
            .take(MOST)
            .cloned()
            .collect()
    }

    /// The commands the half-typed line could still become.
    pub fn completions(&self) -> Vec<crate::command_table::Command> {
        match self.offered() {
            Offered::Slash { commands, .. } => commands,
            _ => Vec::new(),
        }
    }

    /// Which offered command is under the cursor, or `None` when no command is offered.
    ///
    /// Clamped here rather than when the input changes, because the list is a function of the
    /// input: typing a letter can shorten it, and a cursor past the end would otherwise choose
    /// nothing at the moment Tab was pressed.
    pub fn highlighted_completion(&self) -> Option<crate::command_table::Command> {
        let Offered::Slash { commands, skills } = self.offered() else {
            return None;
        };
        let at = self.completion.min(commands.len() + skills.len() - 1);
        commands.get(at).copied()
    }

    /// Which offered skill is under the cursor, or `None` when the cursor is on a command or no
    /// skill is offered.
    pub fn highlighted_skill(&self) -> Option<crate::skills::Skill> {
        let Offered::Slash { commands, skills } = self.offered() else {
            return None;
        };
        let at = self.completion.min(commands.len() + skills.len() - 1);
        skills.get(at.checked_sub(commands.len())?).cloned()
    }

    /// Which offered file is under the cursor, or `None` when no file is offered.
    pub fn highlighted_entry(&self) -> Option<bravebot_mentions::Entry> {
        let Offered::Files(entries) = self.offered() else {
            return None;
        };
        entries
            .get(self.completion.min(entries.len().saturating_sub(1)))
            .cloned()
    }

    /// Whether something is being offered, so the keys that walk the list belong to it.
    ///
    /// The shortcuts are not: there is nothing to choose among them, so Tab and the arrows keep
    /// meaning what they mean everywhere else while the list is up.
    pub fn is_completing(&self) -> bool {
        matches!(self.offered(), Offered::Slash { .. } | Offered::Files(_))
    }

    /// Whether taking what is offered would change the line.
    ///
    /// What Enter turns on. Tab may be pressed on a finished word harmlessly, but Enter has to
    /// choose between completing and sending, and a prompt ending in `@README.md` is a finished
    /// sentence even though the word is still what the list is about. Completing there would leave
    /// a user pressing Enter twice to say something perfectly well formed.
    pub fn completion_would_change_the_line(&self) -> bool {
        match self.offered() {
            Offered::Nothing | Offered::Shortcuts => false,
            Offered::Slash { skills, .. } => {
                let typed = crate::skills::typed(&self.input);
                // A skill's name typed in full is a finished sentence, as a file's is, whatever
                // the untouched cursor is on: `/review` names a skill of its own while `/review-pr`
                // or a command sharing the letters is listed above it.
                if self.completion == 0
                    && typed.is_some_and(|typed| skills.iter().any(|skill| skill.name == typed))
                {
                    return false;
                }
                if let Some(command) = self.highlighted_completion() {
                    return command.name != self.input.trim();
                }
                self.highlighted_skill()
                    .zip(typed)
                    .is_some_and(|(skill, typed)| skill.name != typed)
            }
            Offered::Files(entries) => {
                let Some(typed) = bravebot_mentions::typed_reference(&self.input) else {
                    return false;
                };
                bravebot_mentions::enter_completes(
                    &self.workspace,
                    &typed,
                    &entries,
                    self.completion,
                    &self.reference_sources,
                )
            }
        }
    }

    /// How many things are offered, which is what bounds the cursor that walks them.
    pub fn offered_count(&self) -> usize {
        match self.offered() {
            Offered::Slash { commands, skills } => commands.len() + skills.len(),
            Offered::Files(entries) => entries.len(),
            Offered::Nothing | Offered::Shortcuts => 0,
        }
    }

    /// Move down what is offered, stopping at the end.
    pub fn next_completion(&mut self) {
        let last = self.offered_count().saturating_sub(1);
        self.completion = (self.completion + 1).min(last);
    }

    /// Move up what is offered, stopping at the top.
    pub fn previous_completion(&mut self) {
        self.completion = self.completion.saturating_sub(1);
    }

    /// Take what is under the cursor.
    ///
    /// A command replaces the whole line, since a command *is* the line. A skill or a file
    /// replaces only the half-typed word, because the rest is the sentence it was written into.
    ///
    /// None adds a trailing space where there is more to type: a command expecting an argument
    /// gets one, and so do a skill and a file, while a directory does not, so the path can be
    /// typed onwards into it.
    pub fn accept_completion(&mut self) {
        match self.offered() {
            Offered::Slash { .. } => {
                if let Some(command) = self.highlighted_completion() {
                    let line = if command.argument.is_empty() {
                        command.name.to_string()
                    } else {
                        format!("{} ", command.name)
                    };
                    self.put_in_the_box(line);
                } else if let Some(skill) = self.highlighted_skill() {
                    let start = self.last_word_starts_at();
                    if !self.input[start..].starts_with('/') {
                        return;
                    }
                    let kept = self.input[..start].to_string();
                    self.put_in_the_box(format!("{kept}/{} ", skill.name));
                } else {
                    return;
                }
            }
            Offered::Files(_) => {
                let Some(entry) = self.highlighted_entry() else {
                    return;
                };
                // The `@` that opened the reference is the one at the head of the last word,
                // not the last `@` in the line. A file may have one in its name, and cutting
                // there rebuilds the line around a path nobody chose: `@logo@2` plus the
                // offered `logo@2x.png` becomes `@logo@logo@2x.png`.
                let start = bravebot_mentions::last_word_starts_at(&self.input);
                if !self.input[start..].starts_with('@') {
                    return;
                }
                let kept = self.input[..start].to_string();
                let trailing = if entry.is_directory { "" } else { " " };
                // Escaped, so a name holding a space is still one word when the line is read back.
                let path = bravebot_mentions::escape(&entry.path);
                self.put_in_the_box(format!("{kept}@{path}{trailing}"));
            }
            Offered::Nothing | Offered::Shortcuts => return,
        }
        // A choice from a list drawn over the line rather than typing, so `.` does not make it again:
        // a file chosen this way would be named a second time.
        if let Some(inserting) = &mut self.inserting {
            inserting.change = None;
        }
        self.completion = 0;
    }

    /// What Tab would make of a level started after `/effort `, with the caret at the end of it.
    pub fn argument_completion(&self) -> Option<String> {
        if self.shell || self.caret() != self.input.len() {
            return None;
        }
        crate::command_table::complete_argument(&self.input)
    }

    /// Finish the level started after `/effort ` as far as the levels that fit it agree.
    pub fn accept_argument_completion(&mut self) {
        if let Some(line) = self.argument_completion() {
            self.put_in_the_box(line);
        }
    }

    /// Where the line's last word begins, which is where a completion of it is written.
    fn last_word_starts_at(&self) -> usize {
        self.input
            .char_indices()
            .rev()
            .find(|(_, c)| c.is_whitespace())
            .map_or(0, |(at, c)| at + c.len_utf8())
    }
}
