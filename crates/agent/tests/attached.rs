//! Reading a file dropped onto a line whose argument reaches a planner that cannot read.
//!
//! The point of these is not that a file can be read. It is where the read happens and on whose
//! authority: before the planner's policy exists, under one holding `FileRead` alone, with the path
//! fixed by the gesture. A planner asked a question holds `WebFetch` and nothing else, so a test
//! here that only checked the bytes came back would pass just as well against a read the planner
//! did itself, which is the thing that must never happen.

use bravebot_agent::attached::{self, Carried};
use bravebot_agent::turn::Attachment;
use bravebot_agent::workspace::Workspace;
use bravebot_core::capability::Capability;
use bravebot_core::event::{Event, RecordingSink};
use bravebot_core::trust::TrustStore;
use std::path::PathBuf;

struct Scratch {
    path: PathBuf,
}

impl Scratch {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("bravebot-attached-{name}"));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("create scratch");
        Self { path }
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// The two bytes a PNG starts with, which is all any of this reads.
const PIXELS: [u8; 2] = [0x89, 0x50];

fn dropped(path: &str) -> Vec<Attachment> {
    vec![Attachment {
        path: path.to_string(),
        media: "image/png".to_string(),
    }]
}

/// The bytes are what the request can hold, so a marker standing for a picture has to come back as
/// something a message can carry. A name would leave the planner told which file to look at and
/// unable to look at it, which is the whole defect for a request with no tools in it.
#[test]
fn a_dropped_picture_comes_back_as_the_bytes_a_request_can_hold() {
    let scratch = Scratch::new("carried");
    std::fs::write(scratch.path.join("shot.png"), PIXELS).unwrap();
    let workspace = Workspace::new(&scratch.path).expect("workspace");
    let mut sink = RecordingSink::new();

    let carried = attached::read(
        &workspace,
        &dropped("shot.png"),
        TrustStore::new(&scratch.path),
        &mut sink,
    )
    .expect("a dropped picture is read");

    assert_eq!(
        carried,
        vec![Carried::Shown {
            path: "shot.png".to_string(),
            uri: "data:image/png;base64,iVA=".to_string(),
        }]
    );
}

/// Dropping the file is the grant, and the grant is recorded before the read: a rule on the file
/// beats whatever covers the directory around it (DROP-2). Asserted against a directory the person
/// declined, because that is the case a vouch recorded after the read, or not at all, gets wrong:
/// the bytes would come back described rather than shown and a picture would reach the planner as a
/// sentence about a picture.
#[test]
fn a_dropped_picture_is_shown_even_from_a_directory_nobody_vouched_for() {
    let scratch = Scratch::new("distrusted");
    std::fs::write(scratch.path.join("shot.png"), PIXELS).unwrap();
    let workspace = Workspace::new(&scratch.path).expect("workspace");

    let mut trust = TrustStore::new(&scratch.path);
    trust.distrust("");
    let mut sink = RecordingSink::new();

    let carried = attached::read(&workspace, &dropped("shot.png"), trust, &mut sink)
        .expect("a dropped picture is read");

    assert!(
        matches!(carried.as_slice(), [Carried::Shown { .. }]),
        "the drop did not vouch for the file: {carried:?}"
    );
}

/// The read is gated and named in the trail, which is the property that makes doing it here rather
/// than in the planner worth anything. A read that reached the bytes without the gate would leave a
/// request carrying a picture that no line of the trail accounts for.
#[test]
fn reading_a_dropped_picture_is_gated_and_named_in_the_trail() {
    let scratch = Scratch::new("trail");
    std::fs::write(scratch.path.join("shot.png"), PIXELS).unwrap();
    let workspace = Workspace::new(&scratch.path).expect("workspace");
    let mut sink = RecordingSink::new();

    attached::read(
        &workspace,
        &dropped("shot.png"),
        TrustStore::new(&scratch.path),
        &mut sink,
    )
    .expect("a dropped picture is read");

    assert!(
        sink.events().iter().any(|event| matches!(
            event,
            Event::GatePassed { gate: "precommit", detail } if detail.contains("dropped_0")
        )),
        "the path was not precommitted before the read: {:?}",
        sink.events()
    );
    assert!(
        sink.events().iter().any(|event| matches!(
            event,
            Event::Observed {
                capability: Capability::FileRead,
                ..
            }
        )),
        "the read was not named in the trail: {:?}",
        sink.events()
    );
}

/// A drop comes from wherever somebody dragged it from, which is rarely inside the project
/// (DROP-3). Confining this read would refuse the ordinary case, and what makes reaching out sound
/// is that a gesture put the path there rather than where the file sits.
#[test]
fn a_picture_dropped_from_outside_the_workspace_is_read_all_the_same() {
    let elsewhere = Scratch::new("elsewhere");
    std::fs::write(elsewhere.path.join("shot.png"), PIXELS).unwrap();
    let scratch = Scratch::new("here");
    let workspace = Workspace::new(&scratch.path).expect("workspace");
    let mut sink = RecordingSink::new();

    let outside = elsewhere
        .path
        .join("shot.png")
        .to_string_lossy()
        .to_string();
    let carried = attached::read(
        &workspace,
        &dropped(&outside),
        TrustStore::new(&scratch.path),
        &mut sink,
    )
    .expect("a dropped picture is carried wherever it came from");

    assert!(
        matches!(carried.as_slice(), [Carried::Shown { .. }]),
        "a drop from outside the workspace was refused: {carried:?}"
    );
}

/// The request is not assembled from a file that could not be read. A failure swallowed here would
/// send the line with its marker still in it, which is the defect this whole path exists to end.
#[test]
fn a_picture_that_is_not_there_fails_rather_than_being_carried_as_nothing() {
    let scratch = Scratch::new("missing");
    let workspace = Workspace::new(&scratch.path).expect("workspace");
    let mut sink = RecordingSink::new();

    let error = attached::read(
        &workspace,
        &dropped("gone.png"),
        TrustStore::new(&scratch.path),
        &mut sink,
    )
    .expect_err("a file that is not there cannot be carried");

    assert!(
        error.to_string().contains("gone.png"),
        "the failure did not say which file: {error}"
    );
}

/// Every line is read through here and nearly none of them dropped anything, so the empty case is
/// the common one. A policy needs routing to precommit, so one begun for no files would be refused
/// and every question anybody asked would fail.
#[test]
fn a_line_that_dropped_nothing_reads_nothing_and_asks_nobody() {
    let scratch = Scratch::new("nothing");
    let workspace = Workspace::new(&scratch.path).expect("workspace");
    let mut sink = RecordingSink::new();

    let carried = attached::read(&workspace, &[], TrustStore::new(&scratch.path), &mut sink)
        .expect("a line that dropped nothing is not a failure");

    assert!(carried.is_empty(), "something was carried: {carried:?}");
    assert!(
        sink.events().is_empty(),
        "a trail was written for a line that dropped nothing: {:?}",
        sink.events()
    );
}

/// The markers in a line number the files it named, so a planner reading `[Image #2]` counts to the
/// second part of the message. Out of order, it would be told about the wrong picture.
#[test]
fn two_dropped_pictures_come_back_in_the_order_their_markers_number_them() {
    let scratch = Scratch::new("two");
    std::fs::write(scratch.path.join("first.png"), PIXELS).unwrap();
    std::fs::write(scratch.path.join("second.png"), [0x89u8, 0x51]).unwrap();
    let workspace = Workspace::new(&scratch.path).expect("workspace");
    let mut sink = RecordingSink::new();

    let carried = attached::read(
        &workspace,
        &[
            Attachment {
                path: "first.png".to_string(),
                media: "image/png".to_string(),
            },
            Attachment {
                path: "second.png".to_string(),
                media: "image/png".to_string(),
            },
        ],
        TrustStore::new(&scratch.path),
        &mut sink,
    )
    .expect("both are read");

    let paths: Vec<_> = carried
        .iter()
        .map(|one| match one {
            Carried::Shown { path, .. } | Carried::Described { path, .. } => path.as_str(),
        })
        .collect();
    assert_eq!(paths, ["first.png", "second.png"]);
}
