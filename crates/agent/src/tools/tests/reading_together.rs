use super::arguments::{Scratch, told, with_tools};
use super::*;
use bravebot_core::cancel::Cancel;
use bravebot_core::capability::{Capability, CapabilitySet};
use bravebot_core::event::{Event, RecordingSink};
use bravebot_core::label::Integrity;
use bravebot_core::policy::{ReleasePlan, Routing};
use bravebot_core::trust::TrustStore;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

/// The facts of a turn in the default permission mode, which reaches nothing outside the
/// workspace.
fn asking() -> ReachFacts<'static> {
    ReachFacts {
        permission_mode: crate::LiveMode::new(crate::PermissionMode::Ask),
        profile: None,
    }
}

/// A path argument the driver wrote. Built through an existing constructor, so the tests
/// of this module add no `Labelled::trusted` site to the count LABEL pins.
fn trusted_text(text: &str) -> Labelled<String> {
    Produced::problem(text).text
}

/// Trusted files under `a`, `b`, `c` and the top, and one under `mystery`, which the map
/// distrusts, so a result naming it is quarantined.
fn fixture(name: &str) -> (Scratch, Workspace) {
    let scratch = Scratch::new(name);
    for (path, text) in [
        ("a/one.txt", "needle in a\n"),
        ("a/two.txt", "hay\n"),
        ("b/three.txt", "needle in b\nneedle again\n"),
        ("c/four.txt", "needle in c\n"),
        ("loose.txt", "needle at the top\n"),
        ("mystery/five.txt", "needle in the unvouched directory\n"),
    ] {
        let at = scratch.path.join(path);
        std::fs::create_dir_all(at.parent().expect("a parent")).unwrap();
        std::fs::write(at, text).unwrap();
    }
    let workspace = Workspace::new(&scratch.path).expect("workspace");
    (scratch, workspace)
}

fn policy(sink: &mut RecordingSink) -> Policy<'_, RecordingSink> {
    let mut store = TrustStore::new("/work");
    store.trust(".");
    store.distrust("mystery");
    let mut routing = Routing::new();
    routing.insert_trusted("task", "read several things at once");
    Policy::begin(
        routing,
        ReleasePlan::new(),
        CapabilitySet::from_iter([Capability::FileRead]),
        sink,
    )
    .expect("policy")
    .with_trust(store)
}

fn call(tool: &str, arguments: Value) -> ToolCall {
    serde_json::from_value(json!({
        "id": tool,
        "function": {"name": tool, "arguments": arguments.to_string()}
    }))
    .expect("a call")
}

fn searches(directories: &[&str]) -> Vec<ToolCall> {
    directories
        .iter()
        .map(|directory| {
            call(
                "search",
                json!({"pattern": "needle", "directory": directory}),
            )
        })
        .collect()
}

/// Waits until `count` readers are inside at once, or gives up after five seconds and
/// counts that, so a read run one after another fails the test and does not hang it.
fn wait_for(arrived: &AtomicUsize, count: usize, gave_up: &AtomicUsize) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while arrived.load(Ordering::SeqCst) < count {
        if Instant::now() >= deadline {
            gave_up.fetch_add(1, Ordering::SeqCst);
            return;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}

/// What a call left behind that a person or the trail could tell apart.
type Answer = (
    String,
    String,
    Label,
    String,
    bool,
    bool,
    Option<(String, usize, usize)>,
);

/// Dispatches `calls` in order under a fresh policy, either as a sequential run does or
/// with the reads taken beforehand. The answers, the trail, and the activity shown.
fn dispatched(
    workspace: &Workspace,
    calls: &[ToolCall],
    beforehand: bool,
) -> (Vec<Answer>, Vec<Event>, crate::report::RecordingReporter) {
    let mut sink = RecordingSink::new();
    let mut reporter = crate::report::RecordingReporter::default();
    let mut answers = Vec::new();
    {
        let mut policy = policy(&mut sink);
        let mut reads: Vec<Option<Ahead>> = if beforehand {
            let named: Vec<&ToolCall> = calls.iter().collect();
            read_ahead(
                &policy,
                workspace,
                &asking(),
                &named,
                READS_AT_ONCE,
                &Cancel::new(),
            )
        } else {
            calls.iter().map(|_| None).collect()
        };
        assert!(
            !beforehand || reads.iter().all(Option::is_some),
            "a read was not taken beforehand"
        );
        for (call, read) in calls.iter().zip(reads.iter_mut()) {
            let output = with_tools(workspace, |tools| {
                dispatch_with(
                    &mut policy,
                    tools,
                    &mut crate::confirm::ApproveWrites,
                    &mut reporter,
                    call,
                    read.take(),
                )
            });
            answers.push((
                output.tool.clone(),
                told(&mut policy, &output.text),
                output.text.label(),
                output.origin.clone(),
                output.incomplete,
                output.content,
                output
                    .entries
                    .as_ref()
                    .map(|entries| (entries.origin.clone(), entries.count, entries.directories)),
            ));
        }
    }
    (answers, sink.events().to_vec(), reporter)
}

/// The regression it rejects: reads taken beforehand answering a round differently from
/// a sequential run. The events a branch buffered are emitted where the tool would have
/// run, so the trail is the one a sequential run writes; emitted when the read finished
/// they would all come before the first call's own lines. The fixture holds the answers
/// that differ in kind: trusted text, a quarantined search, a quarantined listing, a
/// refused pattern, a missing directory and a count.
#[test]
fn reads_taken_beforehand_leave_the_trail_the_screen_and_the_answers_a_sequential_run_does() {
    let (_scratch, workspace) = fixture("ahead-equals-sequential");
    let calls = vec![
        call("search", json!({"pattern": "needle", "directory": "a"})),
        call("list_files", json!({"directory": "b"})),
        call("search", json!({"pattern": "needle"})),
        call("list_files", json!({"directory": "mystery"})),
        call("search", json!({"pattern": "("})),
        call("list_files", json!({"directory": "nowhere"})),
        call(
            "search",
            json!({"pattern": ["needle in c", "hay"], "output": "count"}),
        ),
    ];

    let (sequential, trail, screen) = dispatched(&workspace, &calls, false);
    let (beforehand, trail_beforehand, screen_beforehand) = dispatched(&workspace, &calls, true);

    assert_eq!(beforehand, sequential);
    assert_eq!(trail_beforehand, trail);
    // Each dispatch takes a new id, so the two runs agree on the lines and on which
    // finish ends which start, not on the ids' values.
    let ids = |lines: &[Activity]| lines.iter().map(|line| line.call).collect::<Vec<_>>();
    let without_ids = |lines: &[Activity]| {
        lines
            .iter()
            .cloned()
            .map(|line| Activity { call: None, ..line })
            .collect::<Vec<_>>()
    };
    assert_eq!(ids(&screen.finished), ids(&screen.started));
    assert_eq!(
        ids(&screen_beforehand.finished),
        ids(&screen_beforehand.started)
    );
    assert_eq!(
        without_ids(&screen_beforehand.started),
        without_ids(&screen.started)
    );
    assert_eq!(
        without_ids(&screen_beforehand.finished),
        without_ids(&screen.finished)
    );
    assert_eq!(sequential.len(), calls.len());
    assert!(
        sequential.iter().any(|answer| answer.6.is_some()),
        "the fixture produced no quarantined listing: {sequential:?}"
    );
    assert!(
        sequential.iter().any(|answer| !answer.2.is_trusted()),
        "the fixture produced no untrusted search: {sequential:?}"
    );
}

/// MODE-4, PATHREQ-7: the reads of a round that run beside each other each hold the reach the
/// live mode gives them, so a listing and a search of a sibling directory are answered as
/// they would be one after another, and the directory is recorded once. The control is the
/// same round under the mode that asks, which is refused both. The regression it rejects is a
/// read taken beforehand without the reach, which answers the round with a refusal the
/// sequential run would not give.
#[test]
fn reads_taken_beforehand_hold_the_reach_the_mode_gives_them() {
    for (mode, reached) in [
        (crate::PermissionMode::Bypass, true),
        (crate::PermissionMode::Ask, false),
    ] {
        let (_scratch, workspace) = fixture(&format!("ahead-reach-{mode:?}"));
        let beside = Scratch::new(&format!("ahead-reach-beside-{mode:?}"));
        std::fs::write(beside.path.join("n.txt"), "needle outside\n").unwrap();
        let home = Scratch::new(&format!("ahead-reach-home-{mode:?}"));
        let facts = ReachFacts {
            permission_mode: crate::LiveMode::new(mode),
            profile: Some(&home.path),
        };
        let directory = beside.path.display().to_string();
        let calls = [
            call("list_files", json!({"directory": directory})),
            call(
                "search",
                json!({"pattern": "needle", "directory": directory}),
            ),
        ];
        let named: Vec<&ToolCall> = calls.iter().collect();
        let mut sink = RecordingSink::new();
        let mut policy = policy(&mut sink);
        let reads = read_ahead(
            &policy,
            &workspace,
            &facts,
            &named,
            READS_AT_ONCE,
            &Cancel::new(),
        );
        for read in reads {
            let produced = read.expect("a read taken beforehand").rejoin(&mut policy);
            let said = told(&mut policy, &produced.text);
            assert_eq!(
                !said.contains("outside the workspace"),
                reached,
                "{mode:?}: {said}"
            );
        }
        assert_eq!(
            workspace.reached_directories().len(),
            usize::from(reached),
            "{mode:?}"
        );
    }
}

/// The regression it rejects: reads that take turns. Each read holds in its capture until
/// three are inside together, so a run one after another never gets there and gives up.
#[test]
fn the_reads_of_a_round_are_inside_their_captures_at_the_same_time() {
    let (_scratch, workspace) = fixture("ahead-overlap");
    let arrived = Arc::new(AtomicUsize::new(0));
    let gave_up = Arc::new(AtomicUsize::new(0));
    let (inside, failed) = (arrived.clone(), gave_up.clone());
    workspace.hold_reads(move |_| {
        inside.fetch_add(1, Ordering::SeqCst);
        wait_for(&inside, 3, &failed);
    });
    let mut sink = RecordingSink::new();
    let policy = policy(&mut sink);
    let calls = [
        call("search", json!({"pattern": "needle", "directory": "a"})),
        call("list_files", json!({"directory": "b"})),
        call("search", json!({"pattern": "needle", "directory": "c"})),
    ];
    let named: Vec<&ToolCall> = calls.iter().collect();

    let read = read_ahead(
        &policy,
        &workspace,
        &asking(),
        &named,
        READS_AT_ONCE,
        &Cancel::new(),
    );

    assert_eq!(read.iter().flatten().count(), 3);
    assert_eq!(arrived.load(Ordering::SeqCst), 3);
    assert_eq!(
        gave_up.load(Ordering::SeqCst),
        0,
        "a read waited for the others and they never came"
    );
}

/// The regression it rejects: a limit that is not kept, whether no limit or a thread per
/// call. Eight calls under a limit of four: each read waits until four are inside, then
/// stays long enough for a fifth to arrive if the limit were not held.
#[test]
fn no_more_reads_run_at_once_than_the_limit() {
    let (_scratch, workspace) = fixture("ahead-limit");
    let inside = Arc::new(AtomicUsize::new(0));
    let highest = Arc::new(AtomicUsize::new(0));
    let gave_up = Arc::new(AtomicUsize::new(0));
    let (now, most, failed) = (inside.clone(), highest.clone(), gave_up.clone());
    workspace.hold_reads(move |_| {
        let here = now.fetch_add(1, Ordering::SeqCst) + 1;
        most.fetch_max(here, Ordering::SeqCst);
        wait_for(&now, 4, &failed);
        std::thread::sleep(Duration::from_millis(200));
        most.fetch_max(now.load(Ordering::SeqCst), Ordering::SeqCst);
        now.fetch_sub(1, Ordering::SeqCst);
    });
    let mut sink = RecordingSink::new();
    let policy = policy(&mut sink);
    let calls = searches(&["a", "b", "c", "a", "b", "c", "a", "b"]);
    let named: Vec<&ToolCall> = calls.iter().collect();

    let read = read_ahead(&policy, &workspace, &asking(), &named, 4, &Cancel::new());

    assert_eq!(read.iter().flatten().count(), 8);
    assert_eq!(gave_up.load(Ordering::SeqCst), 0, "four never ran together");
    assert_eq!(highest.load(Ordering::SeqCst), 4);
}

/// The regression it rejects: threads that start a read after the person cancelled. One
/// thread takes the reads in turn and the first cancels, so the others are never started.
#[test]
fn a_cancel_stops_the_reads_not_yet_started() {
    let (_scratch, workspace) = fixture("ahead-cancel");
    let cancel = Cancel::new();
    let started = Arc::new(AtomicUsize::new(0));
    let (counted, stopping) = (started.clone(), cancel.clone());
    workspace.hold_reads(move |_| {
        counted.fetch_add(1, Ordering::SeqCst);
        stopping.cancel();
    });
    let mut sink = RecordingSink::new();
    let policy = policy(&mut sink);
    let calls = searches(&["a", "b", "c"]);
    let named: Vec<&ToolCall> = calls.iter().collect();

    let read = read_ahead(&policy, &workspace, &asking(), &named, 1, &cancel);

    assert_eq!(started.load(Ordering::SeqCst), 1);
    assert!(read[0].is_some());
    assert!(read[1].is_none() && read[2].is_none());
}

/// The regression it rejects: a read taken beforehand that captures bytes while an effect
/// is entering. The test holds an effect's entry, which a capture must wait for.
#[test]
fn reads_taken_beforehand_wait_for_an_effect_entering_a_file() {
    let (_scratch, workspace) = fixture("ahead-waits-for-effect");
    let mut sink = RecordingSink::new();
    let policy = policy(&mut sink);
    let authority = policy.file_authority();
    let calls = searches(&["a", "b"]);
    let named: Vec<&ToolCall> = calls.iter().collect();
    let (done, finished) = std::sync::mpsc::channel();

    let entering = authority.capture();
    std::thread::scope(|scope| {
        scope.spawn(|| {
            let taken = read_ahead(
                &policy,
                &workspace,
                &asking(),
                &named,
                READS_AT_ONCE,
                &Cancel::new(),
            );
            done.send(taken.iter().flatten().count()).unwrap();
        });
        assert!(
            finished.recv_timeout(Duration::from_millis(300)).is_err(),
            "a read captured bytes while an effect held the boundary"
        );
        drop(entering);
        assert_eq!(finished.recv_timeout(Duration::from_secs(5)), Ok(2));
    });
}

/// The regression it rejects: a `read_file` that takes the boundary exclusive, so it queues
/// behind a search or listing that is part way through its walk. The test holds the
/// boundary as a reader, which a read must not wait for.
#[test]
fn a_read_file_does_not_wait_for_a_reader_holding_the_boundary() {
    let (_scratch, workspace) = fixture("ahead-read-file-beside-reader");
    let mut sink = RecordingSink::new();
    let mut policy = policy(&mut sink);
    let authority = policy.file_authority();
    let (done, finished) = std::sync::mpsc::channel();

    let searching = authority.capture_shared();
    std::thread::scope(|scope| {
        scope.spawn(|| {
            let page = workspace.read_page_unless_unchanged(
                &mut policy,
                &trusted_text("loose.txt"),
                1,
                10,
                None,
            );
            done.send(page.is_ok()).unwrap();
        });
        let outcome = finished.recv_timeout(Duration::from_secs(5));
        // Released before the assertion, so a read that did wait is let through and the
        // test fails and does not hang on the scope joining it.
        drop(searching);
        assert_eq!(outcome, Ok(true), "a read_file waited for a reader");
    });
}

/// The regression it rejects: a `read_file` that writes its trail lines, or the observation
/// of its path, while it holds the boundary shared. Each event is reported to a sink that,
/// from another thread, tries to enter an effect and waits 300 ms for it. An event emitted
/// inside a shared capture would find the entry blocked.
#[test]
fn a_read_file_emits_nothing_while_it_holds_the_boundary() {
    use bravebot_core::file_authority::FileAuthority;
    use std::sync::Mutex;

    struct Probing {
        authority: Arc<Mutex<Option<FileAuthority>>>,
        entered: Arc<Mutex<Vec<bool>>>,
    }
    impl bravebot_core::event::Sink for Probing {
        fn emit(&mut self, _event: Event) {
            let Some(authority) = self.authority.lock().unwrap().clone() else {
                return;
            };
            let (done, finished) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                drop(authority.capture());
                let _ = done.send(());
            });
            let entered = finished.recv_timeout(Duration::from_millis(300)).is_ok();
            self.entered.lock().unwrap().push(entered);
        }
        fn recording_for(&mut self, _delegate: Option<bravebot_core::delegate::DelegateId>) {}
    }

    let (_scratch, workspace) = fixture("ahead-read-file-emits-outside");
    let authority = Arc::new(Mutex::new(None));
    let entered = Arc::new(Mutex::new(Vec::new()));
    let mut sink = Probing {
        authority: authority.clone(),
        entered: entered.clone(),
    };
    let mut store = TrustStore::new("/work");
    store.trust(".");
    let mut routing = Routing::new();
    routing.insert_trusted("task", "read a file");
    let mut policy = Policy::begin(
        routing,
        ReleasePlan::new(),
        CapabilitySet::from_iter([Capability::FileRead]),
        &mut sink,
    )
    .expect("policy")
    .with_trust(store);
    // The probe enters through the policy's own authority, which `with_trust` replaced.
    *authority.lock().unwrap() = Some(policy.file_authority());

    let page =
        workspace.read_page_unless_unchanged(&mut policy, &trusted_text("loose.txt"), 1, 10, None);
    assert!(page.is_ok());
    let entered = entered.lock().unwrap().clone();
    assert!(
        !entered.is_empty(),
        "the read recorded nothing, so the probe proved nothing"
    );
    assert!(
        entered.iter().all(|entered| *entered),
        "an event reached the sink while the read held the boundary: {entered:?}"
    );
}

/// Reports the first time a gate passes with a detail starting as given, which a test uses
/// to learn that the code under test got past a point without watching its thread.
struct Reaching {
    gate: &'static str,
    detail: &'static str,
    reached: std::sync::mpsc::Sender<()>,
}

impl bravebot_core::event::Sink for Reaching {
    fn emit(&mut self, event: Event) {
        if matches!(event, Event::GatePassed { gate, detail }
            if gate == self.gate && detail.starts_with(self.detail))
        {
            let _ = self.reached.send(());
        }
    }
    fn recording_for(&mut self, _delegate: Option<bravebot_core::delegate::DelegateId>) {}
}

/// A policy for one call, on the file authority a test holds.
fn on_authority<'a>(
    sink: &'a mut Reaching,
    authority: &bravebot_core::file_authority::FileAuthority,
    capabilities: impl IntoIterator<Item = Capability>,
) -> Policy<'a, Reaching> {
    let mut routing = Routing::new();
    routing.insert_trusted("task", "make one call");
    Policy::begin(
        routing,
        ReleasePlan::new(),
        CapabilitySet::from_iter(capabilities),
        sink,
    )
    .expect("policy")
    .with_file_authority(authority.clone())
}

/// Dispatches `call` on a second thread while this one holds the boundary as a reader, and
/// says whether the call reached `gate` with a detail starting `detail` within five seconds.
/// The reader is released before the answer is returned, so a call that waited for it is
/// let through and does not hang.
fn reaches_while_a_reader_holds_the_boundary(
    workspace: &Workspace,
    call: &ToolCall,
    (gate, detail): (&'static str, &'static str),
    capabilities: &[Capability],
    confirmer: impl Confirmer + Send,
) -> bool {
    let authority = bravebot_core::file_authority::FileAuthority::new({
        let mut store = TrustStore::new("/work");
        store.trust(".");
        store
    });
    let (reached, signal) = std::sync::mpsc::channel();
    let searching = authority.capture_shared();
    std::thread::scope(|scope| {
        scope.spawn(|| {
            let mut confirmer = confirmer;
            let mut sink = Reaching {
                gate,
                detail,
                reached,
            };
            let mut policy = on_authority(&mut sink, &authority, capabilities.iter().cloned());
            with_tools(workspace, |tools| {
                dispatch(
                    &mut policy,
                    tools,
                    &mut confirmer,
                    &mut crate::report::IgnoreReports,
                    call,
                );
            });
        });
        let outcome = signal.recv_timeout(Duration::from_secs(5));
        drop(searching);
        outcome.is_ok()
    })
}

/// The regression it rejects: the `write_file` peek at the file it replaces taking the
/// boundary exclusive, so a write queues behind every reader and a reader behind it. The
/// test holds the boundary as a reader, which the peek must not wait for. The credential
/// scan is the first gate after the peek, and the confirmer declines the write, so the
/// test needs only that the peek finished.
#[test]
fn a_write_file_peek_does_not_wait_for_a_reader_holding_the_boundary() {
    let (_scratch, workspace) = fixture("write-peek-beside-reader");
    let call = call(
        "write_file",
        json!({"path": "loose.txt", "contents": "replacement\n"}),
    );
    assert!(
        reaches_while_a_reader_holds_the_boundary(
            &workspace,
            &call,
            ("credential-scan", "write_file: "),
            &[Capability::FileRead, Capability::FileWrite],
            crate::confirm::Unattended,
        ),
        "a write_file waited for a reader before its peek finished"
    );
}

/// The regression it rejects: the `run` label check taking the boundary exclusive, so a
/// line queues behind every reader. The test holds the boundary as a reader, which the
/// check must not wait for. The line's trail entry for the label is written after the
/// check, so the test needs only that the check finished.
#[test]
fn a_run_label_check_does_not_wait_for_a_reader_holding_the_boundary() {
    let (_scratch, workspace) = fixture("run-label-beside-reader");
    let call = call("run", json!({"command": "echo ok"}));
    assert!(
        reaches_while_a_reader_holds_the_boundary(
            &workspace,
            &call,
            ("provenance", "run: output labelled"),
            &[Capability::ShellExec],
            crate::confirm::ApproveRuns,
        ),
        "a run waited for a reader before its label check finished"
    );
}

/// The regression it rejects: a `read_file` that captures bytes while an effect is
/// entering a file, which the shared boundary must still keep out.
#[test]
fn a_read_file_waits_for_an_effect_holding_the_boundary() {
    let (_scratch, workspace) = fixture("ahead-read-file-waits-for-effect");
    let mut sink = RecordingSink::new();
    let mut policy = policy(&mut sink);
    let authority = policy.file_authority();
    let (done, finished) = std::sync::mpsc::channel();

    let entering = authority.capture();
    std::thread::scope(|scope| {
        scope.spawn(|| {
            let page = workspace.read_page_unless_unchanged(
                &mut policy,
                &trusted_text("loose.txt"),
                1,
                10,
                None,
            );
            done.send(page.is_ok()).unwrap();
        });
        assert!(
            finished.recv_timeout(Duration::from_millis(300)).is_err(),
            "a read_file captured bytes while an effect held the boundary"
        );
        drop(entering);
        assert_eq!(finished.recv_timeout(Duration::from_secs(5)), Ok(true));
    });
}

/// The regressions it rejects: a listing that holds the boundary for the whole of its walk,
/// which keeps an effect out until the walk ends; and one that labels its paths before the
/// walk, which reads a state of the map an effect has since changed. The walk is parked in
/// the test hook with no boundary held. An effect enters a file the listing is about to
/// report, which must not wait for the walk, and the listing is then labelled by the map
/// as the effect left it.
#[test]
fn a_listing_walk_does_not_hold_an_effect_out_and_is_labelled_after_it() {
    let (_scratch, workspace) = fixture("ahead-list-walk-beside-effect");
    let mut sink = RecordingSink::new();
    let mut policy = policy(&mut sink);
    let authority = policy.file_authority();
    let (walking, parked) = std::sync::mpsc::channel();
    let (resume, resumed) = std::sync::mpsc::channel::<()>();
    let (resume, resumed) = (
        std::sync::Mutex::new(resume),
        std::sync::Mutex::new(resumed),
    );
    let walking = std::sync::Mutex::new(walking);
    workspace.hold_reads(move |_| {
        walking.lock().unwrap().send(()).unwrap();
        let _ = resumed.lock().unwrap().recv_timeout(Duration::from_secs(5));
    });
    let (listed, listing) = std::sync::mpsc::channel();
    let (entered, entering) = std::sync::mpsc::channel();

    std::thread::scope(|scope| {
        scope.spawn(|| {
            let result = workspace.list(&mut policy, &trusted_text("b"), None, None);
            listed.send(result).unwrap();
        });
        parked
            .recv_timeout(Duration::from_secs(5))
            .expect("the listing reached its walk");
        scope.spawn(|| {
            // Entering takes the boundary exclusive, so it waits for any reader.
            let effect = authority.capture().begin("b/three.txt");
            entered.send(effect).unwrap();
        });
        let effect = entering.recv_timeout(Duration::from_secs(2));
        // Released before the assertion, so a listing that held the boundary is let
        // through and the test fails and does not hang on the scope joining it.
        resume.lock().unwrap().send(()).unwrap();
        let effect = effect
            .expect("an effect waited for a listing's walk")
            .expect("the path was free");
        let listing = listing
            .recv_timeout(Duration::from_secs(5))
            .expect("the listing finished")
            .expect("a listing");
        assert!(
            !listing.label().is_trusted(),
            "the listing was labelled by the map from before the effect entered"
        );
        drop(effect);
    });
}

/// The regression it rejects: a listing that looks its paths up without the boundary, so
/// that an effect entering at that moment is read half way. The test holds the boundary as
/// an effect entering would.
#[test]
fn a_listing_waits_for_an_effect_holding_the_boundary_to_label_its_paths() {
    let (_scratch, workspace) = fixture("ahead-list-waits-for-effect");
    let mut sink = RecordingSink::new();
    let mut policy = policy(&mut sink);
    let authority = policy.file_authority();
    let (done, finished) = std::sync::mpsc::channel();

    let entering = authority.capture();
    std::thread::scope(|scope| {
        scope.spawn(|| {
            let result = workspace.list(&mut policy, &trusted_text("b"), None, None);
            done.send(result.is_ok()).unwrap();
        });
        assert!(
            finished.recv_timeout(Duration::from_millis(300)).is_err(),
            "a listing labelled its paths while an effect held the boundary"
        );
        drop(entering);
        assert_eq!(finished.recv_timeout(Duration::from_secs(5)), Ok(true));
    });
}

/// The regressions it rejects: a search that holds the boundary for the whole of its walk,
/// which keeps an effect out until the walk ends; and one that labels its files before the
/// walk, or without the boundary, which reads a state of the map an effect has since
/// changed. The search is parked in the test hook with no boundary held. An effect enters a
/// file the search is about to read, which must not wait for it, and the result is then
/// labelled by the map as the effect left it.
#[test]
fn a_search_walk_does_not_hold_an_effect_out_and_is_labelled_after_it() {
    let (_scratch, workspace) = fixture("ahead-grep-walk-beside-effect");
    let mut sink = RecordingSink::new();
    let mut policy = policy(&mut sink);
    let authority = policy.file_authority();
    let (walking, parked) = std::sync::mpsc::channel();
    let (resume, resumed) = std::sync::mpsc::channel::<()>();
    let (resume, resumed) = (
        std::sync::Mutex::new(resume),
        std::sync::Mutex::new(resumed),
    );
    let walking = std::sync::Mutex::new(walking);
    workspace.hold_reads(move |_| {
        walking.lock().unwrap().send(()).unwrap();
        let _ = resumed.lock().unwrap().recv_timeout(Duration::from_secs(5));
    });
    let (searched, search) = std::sync::mpsc::channel();
    let (entered, entering) = std::sync::mpsc::channel();

    std::thread::scope(|scope| {
        scope.spawn(|| {
            let result = workspace.grep_around(
                &mut policy,
                &[trusted_text("needle")],
                &trusted_text("b"),
                None,
                true,
                1,
                0,
            );
            searched.send(result).unwrap();
        });
        parked
            .recv_timeout(Duration::from_secs(5))
            .expect("the search reached its walk");
        scope.spawn(|| {
            // Entering takes the boundary exclusive, so it waits for any reader.
            let effect = authority.capture().begin("b/three.txt");
            entered.send(effect).unwrap();
        });
        let effect = entering.recv_timeout(Duration::from_secs(2));
        // Released before the assertion, so a search that held the boundary is let
        // through and the test fails and does not hang on the scope joining it.
        resume.lock().unwrap().send(()).unwrap();
        let effect = effect
            .expect("an effect waited for a search's walk")
            .expect("the path was free");
        let search = search
            .recv_timeout(Duration::from_secs(5))
            .expect("the search finished")
            .expect("a search");
        assert!(
            !search.label().is_trusted(),
            "the search was labelled by the map from before the effect entered"
        );
        drop(effect);
    });
}

/// The regression it rejects: a search that reads a file's bytes, or the map's answer for
/// it, without the boundary, so that an effect entering that file is read half way
/// (TRUST-4). The test holds the boundary as an effect entering would.
#[test]
fn a_search_waits_for_an_effect_holding_the_boundary_to_read_a_file() {
    let (_scratch, workspace) = fixture("ahead-grep-waits-for-effect");
    let mut sink = RecordingSink::new();
    let mut policy = policy(&mut sink);
    let authority = policy.file_authority();
    let (done, finished) = std::sync::mpsc::channel();

    let entering = authority.capture();
    std::thread::scope(|scope| {
        scope.spawn(|| {
            let result = workspace.grep_around(
                &mut policy,
                &[trusted_text("needle")],
                &trusted_text("b"),
                None,
                true,
                1,
                0,
            );
            done.send(result.is_ok()).unwrap();
        });
        assert!(
            finished.recv_timeout(Duration::from_millis(300)).is_err(),
            "a search read a file while an effect held the boundary"
        );
        drop(entering);
        assert_eq!(finished.recv_timeout(Duration::from_secs(5)), Ok(true));
    });
}

/// A pipe named `out` in a fresh workspace, with a reader holding it open without waiting so
/// that a writer's open returns at once. Then `tool` is dispatched with `arguments` on a
/// thread. Says whether the call returned, and whether the exclusive boundary could then be
/// taken, each within ten seconds, and whether a program ran. `vouch_the_script` writes
/// `gen.sh` and vouches for it, which is what makes a line running it trusted and so makes
/// the scan of its destinations read them. `None` where `mkfifo` is not installed.
///
/// A read of the pipe for the file it would replace waits for a writer, and the writer is
/// the call itself, which opens the destination only after that read. So a call that
/// returns here did not wait on it, and a boundary that is free is not held by a read that
/// is still waiting.
#[cfg(unix)]
fn over_a_pipe(
    name: &str,
    tool: &'static str,
    arguments: Value,
    capabilities: &'static [Capability],
    vouch_the_script: bool,
    confirmer: impl Confirmer + Send + 'static,
) -> Option<(bool, bool, bool)> {
    use std::os::unix::fs::OpenOptionsExt as _;
    let (scratch, workspace) = fixture(name);
    let pipe = scratch.path.join("out");
    let made = std::process::Command::new("mkfifo").arg(&pipe).status();
    if !made.is_ok_and(|status| status.success()) {
        eprintln!("mkfifo is not installed, so a pipe is not tried");
        return None;
    }
    let _reader = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(rustix::fs::OFlags::NONBLOCK.bits().cast_signed())
        .open(&pipe)
        .expect("a reader on the pipe");
    let root = scratch.path.canonicalize().expect("the tree exists");
    let vouched = if vouch_the_script {
        use std::os::unix::fs::PermissionsExt as _;
        let script = root.join("gen.sh");
        std::fs::write(&script, "#!/bin/sh\necho x\n").expect("a script");
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755))
            .expect("executable");
        bravebot_core::programs::TrustedPrograms::from_iter([
            bravebot_core::programs::Command::new(script, Vec::new(), root.clone()),
        ])
    } else {
        bravebot_core::programs::TrustedPrograms::new()
    };
    let authority = bravebot_core::file_authority::FileAuthority::new({
        let mut store = TrustStore::new(workspace.root());
        store.trust(".");
        store
    });
    let (returned, answer) = std::sync::mpsc::channel();
    {
        let authority = authority.clone();
        std::thread::spawn(move || {
            let mut confirmer = confirmer;
            let mut sink = RecordingSink::new();
            let mut routing = Routing::new();
            routing.insert_trusted("task", "write over a pipe");
            let mut policy = Policy::begin(
                routing,
                ReleasePlan::new(),
                CapabilitySet::from_iter(capabilities.iter().cloned()),
                &mut sink,
            )
            .expect("policy")
            .with_file_authority(authority)
            .with_root(&root)
            .with_programs(vouched);
            let output = with_tools(&workspace, |tools| {
                dispatch(
                    &mut policy,
                    tools,
                    &mut confirmer,
                    &mut crate::report::IgnoreReports,
                    &call(tool, arguments),
                )
            });
            let _ = returned.send(output.ran_a_program);
        });
    }
    let answered = answer.recv_timeout(Duration::from_secs(10));
    let (call_returned, ran) = (answered.is_ok(), answered.unwrap_or(false));
    let (entered, can_enter) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        drop(authority.capture());
        let _ = entered.send(());
    });
    let boundary_free = can_enter.recv_timeout(Duration::from_secs(10)).is_ok();
    Some((call_returned, boundary_free, ran))
}

/// The regression it rejects: a `write_file` whose destination is a pipe reading it for
/// the diff a person approves, inside the shared boundary, and waiting for a writer that
/// is this very call. The next effect then waits behind that boundary, and so does every
/// file tool after it.
#[cfg(unix)]
#[test]
fn a_write_file_over_a_pipe_returns_and_leaves_the_boundary_free() {
    let Some((returned, free, _)) = over_a_pipe(
        "write-over-pipe",
        "write_file",
        json!({"path": "out", "contents": "replacement\n"}),
        &[Capability::FileRead, Capability::FileWrite],
        false,
        crate::confirm::ApproveWrites,
    ) else {
        return;
    };
    assert!(returned, "a write_file over a pipe waited for a writer");
    assert!(free, "the boundary was still held after the write returned");
}

/// The regression it rejects: a `run` redirecting into a pipe reading the destination for
/// the scan of what the line leaves, inside the exclusive boundary, so that every file tool
/// waits for a writer that is the line itself.
#[cfg(unix)]
#[test]
fn a_run_redirecting_into_a_pipe_returns_and_leaves_the_boundary_free() {
    let Some((returned, free, ran)) = over_a_pipe(
        "redirect-into-pipe",
        "run",
        json!({"command": "./gen.sh > out"}),
        &[
            Capability::ShellExec,
            Capability::FileRead,
            Capability::FileWrite,
        ],
        true,
        crate::confirm::ApproveRuns,
    ) else {
        return;
    };
    assert!(
        returned,
        "a run redirecting into a pipe waited for a writer"
    );
    assert!(ran, "the line did not run, so no destination was read");
    assert!(free, "the boundary was still held after the line returned");
}

/// The regression it rejects: using a read after the planner's context met something
/// untrusted, which the call it answers would have been refused under.
#[test]
fn a_read_is_used_only_while_the_context_is_what_it_ran_under() {
    let (_scratch, workspace) = fixture("ahead-context");
    let mut sink = RecordingSink::new();
    let policy = policy(&mut sink);
    let calls = searches(&["a", "b"]);
    let named: Vec<&ToolCall> = calls.iter().collect();
    let mut read = read_ahead(
        &policy,
        &workspace,
        &asking(),
        &named,
        READS_AT_ONCE,
        &Cancel::new(),
    );

    let mut later_sink = RecordingSink::new();
    let later = self::policy(&mut later_sink).resuming(Integrity::Untrusted);

    assert!(read[0].take().unwrap().if_still_current(&policy).is_some());
    assert!(read[1].take().unwrap().if_still_current(&later).is_none());
}

/// The regression it rejects: a read of a tool the run does not offer, a tool that asks a
/// question, or a call whose arguments are not JSON being run beforehand.
#[test]
fn only_a_read_the_run_offers_is_taken_beforehand() {
    let both = ["list_files".to_string(), "search".to_string()];
    let only_reading = ["read_file".to_string()];
    let search = call("search", json!({"pattern": "x"}));
    let namespaced = call("functions_list_files", json!({}));
    let not_json: ToolCall = serde_json::from_value(json!({
        "id": "x",
        "function": {"name": "search", "arguments": "{not json"}
    }))
    .unwrap();

    assert!(can_read_ahead(&search, None, None));
    assert!(can_read_ahead(&search, Some(&both), None));
    assert!(can_read_ahead(&namespaced, None, None));
    assert!(!can_read_ahead(&search, Some(&only_reading), None));
    assert!(!can_read_ahead(&not_json, None, None));
    for asks_or_acts in [
        "read_file",
        "read_git",
        "lsp",
        "write_file",
        "run",
        "fetch_url",
    ] {
        assert!(
            !can_read_ahead(&call(asks_or_acts, json!({})), None, None),
            "{asks_or_acts}"
        );
    }
}
