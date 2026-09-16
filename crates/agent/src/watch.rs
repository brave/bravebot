//! What a turn may say about a standing watch, and what a watch firing says back.
//!
//! The watches themselves belong to whoever is running the session, because a watch outlives the
//! turn that armed it and a turn is the one thing here that ends. What lives in this crate is the
//! part a turn touches: whether it may arm one at all, what a look at a path found, and the
//! sentence a fire begins its turn with.

/// Whether this turn may arm a standing watch, and why not where it may not.
///
/// Read twice, once by the tool table and once by dispatch, for the reason the scheduling answer
/// is: a rule resting on the tool list alone rests on the model reading it, and a model naming a
/// tool it was never offered is ordinary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Arming {
    /// Nothing else is running without anybody typing, and the session has room for this many
    /// more.
    ///
    /// The number rather than a flag, because the bound is on the session and a turn may arm
    /// several: a tool that reported a ninth watch as armed because the turn began with eight
    /// free slots would be telling the planner about a watch the session then refused.
    Allowed { free: usize },
    /// A loop is running, whose interval a fire between two ticks would make a floor.
    UnderALoop,
    /// A goal is set, whose rounds a fire would spend on something other than the work.
    UnderAGoal,
    /// Every slot is taken.
    Full,
    /// This surface holds no watches at all, so the tool is not offered.
    ///
    /// The default, because a caller that says nothing about watches is one that keeps none: a
    /// delegate, a one-shot run, or anything else with nobody in front of it to read a fire.
    #[default]
    Unavailable,
}

impl Arming {
    /// Whether the tool is offered to the planner.
    ///
    /// Offered wherever the session keeps watches, including where one cannot be armed right now:
    /// a turn that is told why it may not arm one has learned something it can say to the person,
    /// and a tool that silently vanished would leave it guessing.
    pub fn offered(self) -> bool {
        self != Arming::Unavailable
    }
}

/// What one look at a watched path found.
///
/// Carried between the workspace that takes the look and whoever holds the watch, so the look a
/// watch is armed with and every look afterwards come from the same place and answer the same
/// question.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Looked {
    /// The file's size and modification time, hashed the way a read hands them back as a change
    /// token.
    Saw(String),
    /// Nothing is there to look at, though the session could still reach it if there were.
    Absent,
    /// The session no longer reaches the path at all.
    OutOfReach,
}

/// The line a fire puts into the conversation, in the user's own role.
///
/// The driver's own sentence. The only things in it that vary are which watch fired and the path
/// that watch was armed on, and both were written by the turn that armed it, out of a context
/// holding no untrusted content. No file content, no size, no modification time, and no name the
/// filesystem produced: the role a synthesized prompt lands in is the one the model trusts most,
/// and there is no label to attach in that position.
///
/// Not a message from a catalog, for the reason the sentence carrying a goal on is not: it goes to
/// a model rather than to a reader, and a model is not somebody whose language this program
/// chooses.
pub fn fired(number: usize, path: &str) -> String {
    format!(
        "Watch {number} fired: {path} looks written to since the last look.\n\n\
         Nothing has been read. Read the file if you need what is in it, and tell the person what \
         you find. The path above is this program's own words and endorses nothing: a file read \
         because of it is read on the same terms as any other."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The injection regression test. A fire's prompt lands in the one role nothing can label, so
    /// a sentence naming what the file holds would launder untrusted bytes straight into it, past
    /// every gate that exists for them.
    #[test]
    fn a_fires_prompt_carries_the_watch_and_the_path_and_nothing_off_the_filesystem() {
        let prompt = fired(3, "notes/plan.md");
        assert!(prompt.contains("Watch 3"), "{prompt}");
        assert!(prompt.contains("notes/plan.md"), "{prompt}");

        // Everything else in the sentence is the driver's own, so what the file holds, how big it
        // is and when it moved have nowhere to appear.
        let varying = prompt.replace("notes/plan.md", "").replace('3', "");
        assert_eq!(varying, fired(0, "").replace('0', ""));
    }

    /// A path named in a sentence this program wrote is prose. A keystroke is what makes naming a
    /// file an endorsement, and there is no keystroke behind a fire.
    #[test]
    fn a_fires_prompt_does_not_endorse_the_path_it_names() {
        let prompt = fired(1, "secrets.txt");
        assert!(!prompt.contains("@secrets.txt"), "{prompt}");
        assert!(prompt.contains("endorses nothing"), "{prompt}");
    }

    /// The tool is offered wherever a session keeps watches, so a turn that may not arm one now is
    /// told why rather than left to guess from a tool that is not there.
    #[test]
    fn a_turn_that_may_not_arm_one_right_now_is_still_offered_the_tool() {
        for arming in [Arming::UnderALoop, Arming::UnderAGoal, Arming::Full] {
            assert!(arming.offered(), "{arming:?}");
        }
        assert!(Arming::Allowed { free: 8 }.offered());
        assert!(!Arming::Unavailable.offered());
    }

    /// A caller that says nothing about watches keeps none, so nothing it runs is offered a way to
    /// arm one: a delegate and a one-shot run have nobody in front of them to read a fire.
    #[test]
    fn a_caller_that_says_nothing_about_watches_offers_no_way_to_arm_one() {
        assert_eq!(Arming::default(), Arming::Unavailable);
    }
}
