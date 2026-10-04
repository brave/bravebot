//! The turns a session can be put back to, held to the depth and budget a record may keep.

use crate::sessions::{MAX_REWIND_POINTS, RewindPoint, TurnSnapshot};
use bravebot_agent::rewind::{CoverageGap, RewindCoverage};
use bravebot_agent::workspace::{Backup, Before, MAX_REWIND_BYTES, Workspace};
use std::collections::{BTreeSet, HashSet};

/// The points a session can be put back to, oldest first.
///
/// The list is private because the depth and the budget hold over the whole of it rather than
/// over any one point: a caller that could push onto it would be a caller that could grow it
/// without bound.
#[derive(Debug, Clone, Default)]
pub struct RewindStack {
    points: Vec<RewindPoint>,
}

impl RewindStack {
    /// Points read off a record, or placed again by a front end, held to what a session may keep.
    pub fn of(points: Vec<RewindPoint>) -> Self {
        let mut stack = Self { points };
        stack.hold();
        stack
    }

    /// The points, oldest first.
    pub fn points(&self) -> &[RewindPoint] {
        &self.points
    }

    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    /// Open a point for the turn about to begin.
    pub fn open(&mut self, snapshot: TurnSnapshot, prompt: String) {
        self.points.push(RewindPoint {
            coverage: Default::default(),
            snapshot,
            backups: Vec::new(),
            prompt,
        });
        self.hold();
    }

    /// Keep what the turn that just ended wrote over, against the point it opened.
    ///
    /// Dropped where no point is open, which is a turn whose window something closed while it
    /// ran: the backups belong to a point nothing can rewind to, and holding them would spend
    /// the budget on bytes no rewind will ever read.
    pub fn keep_backups(&mut self, backups: Vec<Backup>) {
        let Some(point) = self.points.last_mut() else {
            return;
        };
        point.backups = backups;
        self.hold();
    }

    /// Give up every point.
    ///
    /// A point describes the session as it stood before some turn, so anything that changes the
    /// session outside a turn leaves every one of them describing something else. Rewinding to a
    /// stale point would undo that change as well, silently and under a line saying the session
    /// went back to a turn. All of them go rather than the newest, since the change lands after
    /// the newest and therefore before none of them.
    pub fn close(&mut self) {
        self.points.clear();
    }

    /// Bind resumed and newly opened points to the workspace before the next turn.
    pub fn bind_coverage(&mut self, workspace: &Workspace) {
        // Resuming cannot prove a previous server's untracked descendants have stopped.
        if self
            .points
            .iter()
            .any(|point| point.coverage.gaps().contains(&CoverageGap::LanguageServer))
        {
            workspace.mark_rewind_gap(CoverageGap::LanguageServer);
        }
        for point in &mut self.points {
            point.coverage.rebind(workspace.rewind_coverage());
        }
    }

    pub fn record_gap(&mut self, gap: CoverageGap) {
        for point in &mut self.points {
            point.coverage.record([gap]);
        }
    }

    /// Take the last `steps` turns' points, and everything needed to put the tree back.
    ///
    /// `None` where the stack holds fewer than `steps` points, so asking to go further back
    /// than it remembers rewinds nothing: landing on the furthest point it happens to hold would
    /// report a session put back somewhere it is not.
    ///
    /// One backup per path, the oldest, since that is the state being asked for. A path written
    /// in two of the undone turns goes back to what it held before the first of them, and
    /// carrying the later copy as well would write the middle state over the answer, or report a
    /// path as refused when the copy that mattered did go back.
    pub fn take(&mut self, steps: usize) -> Option<RewindPoint> {
        if steps == 0 || steps > self.points.len() {
            return None;
        }
        let mut undone = self.points.split_off(self.points.len() - steps);
        let gaps: BTreeSet<_> = undone
            .iter()
            .flat_map(|point| point.coverage.gaps())
            .collect();
        for point in &mut self.points {
            point.coverage.record(gaps.iter().copied());
        }
        let mut seen = HashSet::new();
        let mut backups = Vec::new();
        for point in &mut undone {
            for backup in std::mem::take(&mut point.backups) {
                if seen.insert(backup.path.clone()) {
                    backups.push(backup);
                }
            }
        }
        undone.into_iter().next().map(|mut point| {
            point.backups = backups;
            point.coverage = RewindCoverage::restored(gaps);
            point
        })
    }

    /// Hold the points to what a session may keep: the depth, and the bytes.
    ///
    /// The oldest go first. A rewind is reached for about the turn just gone or one of the few
    /// before it, so the point furthest back is the one whose loss costs least, and dropping a
    /// newer point to keep an older one would leave a stack with a hole nothing can walk past.
    /// The newest is never dropped: a turn whose own writes fill the budget still has to be
    /// undoable, which is the turn most likely to be worth undoing.
    fn hold(&mut self) {
        while self.points.len() > MAX_REWIND_POINTS {
            self.points.remove(0);
        }
        while self.points.len() > 1 && held_bytes(&self.points) > MAX_REWIND_BYTES {
            self.points.remove(0);
        }
    }
}

/// What the points are holding in memory, which is what the budget is spent on.
fn held_bytes(points: &[RewindPoint]) -> usize {
    points
        .iter()
        .flat_map(|point| &point.backups)
        .map(|backup| match &backup.was {
            Before::Bytes(bytes) => bytes.len(),
            Before::Nothing | Before::NotKept => 0,
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The state before some turn, for a test that only needs a point to exist.
    fn snapshot_before(turns: usize) -> TurnSnapshot {
        TurnSnapshot {
            conversation: bravebot_agent::Conversation::new().snapshot(),
            turns,
            tokens: 10,
            spend: Default::default(),
            timing: Default::default(),
            cached: None,
            cached_prompt_tokens: None,
            trust: bravebot_core::trust::TrustStore::new("/work"),
            programs: bravebot_core::programs::TrustedPrograms::default(),
            transcript_len: turns,
            title: "a session".to_string(),
            was_wrote: true,
        }
    }

    /// What a path held before a turn wrote to it.
    fn held(path: &str, was: Before) -> Backup {
        Backup {
            captured_trust: bravebot_core::label::Integrity::Trusted,
            path: std::path::PathBuf::from(path),
            was,
        }
    }

    /// A snapshot describes the session as it stood before the last turn. Once something other
    /// than a turn has changed the session, rewinding to it would undo that change too, under a
    /// line saying one turn was rewound.
    #[test]
    fn closing_the_rewind_window_leaves_nothing_to_rewind_to() {
        let mut s = RewindStack::default();
        s.open(snapshot_before(0), "the first thing".into());
        s.keep_backups(vec![held("/tmp/whatever", Before::Nothing)]);
        s.open(snapshot_before(1), "the second thing".into());

        s.close();

        assert!(s.points().is_empty());
    }

    /// Loaded server warnings cover new turns too; a new workspace cannot prove children ended.
    #[test]
    fn resumed_server_coverage_reaches_new_points_and_repeated_undo() {
        let root = crate::testutil::scratch_dir("undo-resumed-coverage");
        std::fs::create_dir_all(&root).unwrap();
        let workspace = Workspace::new(&root).unwrap();
        let mut s = RewindStack::default();
        s.open(snapshot_before(0), "earlier".into());
        s.record_gap(CoverageGap::LanguageServer);
        s.open(snapshot_before(1), "later".into());
        s.bind_coverage(&workspace);
        for _ in 0..2 {
            let point = s.take(1).unwrap();
            assert_eq!(point.coverage.gaps(), [CoverageGap::LanguageServer].into());
        }
        assert!(!workspace.rewind_coverage().is_complete());
        std::fs::remove_dir_all(root).unwrap();
    }

    /// The point the issue is about: a mistake is usually noticed a turn or two after it was
    /// made, so a session that remembers only the turn that just ended remembers the one case
    /// least likely to need it.
    #[test]
    fn a_rewind_reaches_past_the_turn_that_just_ended() {
        let mut s = RewindStack::default();
        s.open(snapshot_before(0), "the first thing".into());
        s.keep_backups(vec![held("/work/one", Before::Nothing)]);
        s.open(snapshot_before(1), "the second thing".into());
        s.keep_backups(vec![held("/work/two", Before::Nothing)]);

        let RewindPoint {
            snapshot, backups, ..
        } = s.take(2).expect("two turns to go back");

        assert_eq!(snapshot.turns, 0, "two turns back is not before the first");
        assert_eq!(backups.len(), 2, "one of the two turns' writes was dropped");
        assert!(
            s.points().is_empty(),
            "a point that was rewound past is still offered"
        );
    }

    /// Going back further than the session remembers has no honest answer, and landing on the
    /// furthest point it happens to hold would report a tree put back somewhere it is not.
    #[test]
    fn going_back_further_than_the_session_remembers_rewinds_nothing() {
        let mut s = RewindStack::default();
        s.open(snapshot_before(0), "the only thing".into());

        assert!(s.take(2).is_none());
        assert_eq!(
            s.points().len(),
            1,
            "the point that could not be reached was consumed anyway"
        );
    }

    /// A path two of the undone turns wrote to goes back to what it held before the first of
    /// them. Carrying the later copy as well would write the middle state over the answer.
    #[test]
    fn a_path_written_in_two_undone_turns_goes_back_to_before_the_first() {
        let mut s = RewindStack::default();
        s.open(snapshot_before(0), "the first thing".into());
        s.keep_backups(vec![held(
            "/work/notes",
            Before::Bytes(b"original".to_vec()),
        )]);
        s.open(snapshot_before(1), "the second thing".into());
        s.keep_backups(vec![held(
            "/work/notes",
            Before::Bytes(b"after the first turn".to_vec()),
        )]);

        let RewindPoint { backups, .. } = s.take(2).expect("two turns to go back");

        assert_eq!(backups.len(), 1, "the same path is put back twice");
        assert_eq!(
            backups[0].was,
            Before::Bytes(b"original".to_vec()),
            "the path went back to the middle of the rewind"
        );
    }

    /// The depth is what bounds a record written after every turn, so it holds however many
    /// turns the session has had. The oldest goes, since a rewind walks back from the newest and
    /// a stack with a hole in it cannot be walked past one.
    #[test]
    fn a_session_keeps_no_more_points_than_it_may() {
        let mut s = RewindStack::default();
        for turn in 0..MAX_REWIND_POINTS + 2 {
            s.open(snapshot_before(turn), format!("thing {turn}"));
        }

        assert_eq!(s.points().len(), MAX_REWIND_POINTS);
        assert_eq!(
            s.points()[0].snapshot.turns,
            2,
            "the points dropped were not the oldest"
        );
    }

    /// The budget is what is held at once rather than what each turn may add, so a turn that
    /// spends it takes the room from the turns behind it. The newest survives whatever it costs:
    /// a turn whose own writes fill the budget is the one most likely to be worth undoing.
    #[test]
    fn one_turns_writes_can_cost_the_session_the_turns_behind_it() {
        let mut s = RewindStack::default();
        s.open(snapshot_before(0), "the first thing".into());
        s.keep_backups(vec![held("/work/one", Before::Bytes(vec![0; 1024]))]);
        s.open(snapshot_before(1), "the second thing".into());
        s.keep_backups(vec![held(
            "/work/two",
            Before::Bytes(vec![0; MAX_REWIND_BYTES]),
        )]);

        assert_eq!(s.points().len(), 1, "the budget was not held to");
        assert_eq!(
            s.points()[0].snapshot.turns,
            1,
            "the turn that spent the budget is the one that was dropped"
        );
    }

    /// A record holding more than a session may keep comes back held to the same limits.
    #[test]
    fn points_read_off_a_record_are_held_to_the_depth() {
        let points = (0..MAX_REWIND_POINTS + 1)
            .map(|turn| RewindPoint {
                coverage: Default::default(),
                snapshot: snapshot_before(turn),
                backups: Vec::new(),
                prompt: format!("thing {turn}"),
            })
            .collect();

        let s = RewindStack::of(points);

        assert_eq!(s.points().len(), MAX_REWIND_POINTS);
        assert_eq!(s.points()[0].snapshot.turns, 1);
    }

    /// A turn whose window something closed while it ran has no point to hang its writes on, and
    /// keeping them would spend the budget on bytes no rewind can ever read.
    #[test]
    fn backups_with_no_point_to_hang_them_on_are_dropped() {
        let mut s = RewindStack::default();
        s.keep_backups(vec![held("/work/one", Before::Bytes(vec![0; 1024]))]);

        assert!(s.points().is_empty());
    }
}
