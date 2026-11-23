//! An incognito session that resumes an ordinary session's record leaves that record alone.
//!
//! # Why a binary of its own
//!
//! The ordinary session that writes the record has to run before the mode is engaged, and engaging
//! is one-way for the life of a process. [`incognito`](../incognito.rs) engages at the top of every
//! test, so it cannot hold a test that begins with an ordinary session. This binary holds one test
//! that writes as an ordinary session, engages, and then works on as an incognito one.

use bravebot_session::audit::Stamped;
use bravebot_session::sessions::{self, Front, Handle};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The standing state a session saves, with the conversation, turn count and token count a test
/// chooses and nothing else set.
macro_rules! standing {
    ($conversation:expr, $turns:expr, $tokens:expr) => {
        bravebot_session::sessions::Standing {
            history: None,
            conversation: &$conversation.snapshot(),
            turns: $turns,
            tokens: $tokens,
            split: Default::default(),
            spend: &BTreeMap::new(),
            timing: &BTreeMap::new(),
            model: None,
            todos: &BTreeMap::new(),
            asides: &[],
            trust: &bravebot_core::trust::TrustStore::new("/work"),
            programs: &bravebot_core::programs::TrustedPrograms::from_iter([]),
            directories: &[],
            manifest: None,
            rewind: &[],
            checkouts: &[],
            touched: None,
            answered_by_mode: false,
        }
    };
}

/// A conversation of `exchanges` question and answer pairs.
fn conversation_of(exchanges: usize) -> bravebot_agent::Conversation {
    let mut conversation = bravebot_agent::Conversation::new();
    for exchange in 1..=exchanges {
        conversation.push(bravebot_aichat::protocol::Message::user(format!(
            "question {exchange}"
        )));
        conversation.push(bravebot_aichat::protocol::Message::assistant(format!(
            "answer {exchange}"
        )));
    }
    conversation
}

/// One gate decision, which is what the audit trail holds.
fn decided(detail: &str) -> Stamped {
    Stamped {
        at: 1_700_000_000,
        from: None,
        event: bravebot_core::event::Event::GatePassed {
            gate: "a-gate",
            detail: detail.to_string(),
        },
    }
}

/// Every file under `dir` with the bytes it holds, keyed by its path relative to `root`.
///
/// Bytes rather than names, so a record rewritten in place is told apart from one left alone.
fn bytes_under(root: &Path, dir: &Path, found: &mut BTreeMap<PathBuf, Vec<u8>>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            bytes_under(root, &path, found);
        } else if let Ok(relative) = path.strip_prefix(root) {
            found.insert(
                relative.to_path_buf(),
                std::fs::read(&path).expect("a readable file"),
            );
        }
    }
}

/// Fail naming the files whose bytes differ between `before` and `now`, which includes one added
/// or removed, rather than printing the bytes.
fn assert_unchanged(
    now: &BTreeMap<PathBuf, Vec<u8>>,
    before: &BTreeMap<PathBuf, Vec<u8>>,
    what: &str,
) {
    let differing: std::collections::BTreeSet<String> = now
        .keys()
        .chain(before.keys())
        .filter(|path| now.get(*path) != before.get(*path))
        .map(|path| {
            path.file_name()
                .map_or_else(String::new, |name| name.to_string_lossy().into_owned())
        })
        .collect();
    assert!(
        differing.is_empty(),
        "{what}; these files differ: {differing:?}"
    );
}

/// The text of each decision in a session's audit trail, by the turn that made it.
fn trail_text(project: &Path, id: &str) -> BTreeMap<usize, Vec<String>> {
    sessions::audit_of(project, id)
        .into_iter()
        .map(|(turn, lines)| (turn, lines.into_iter().map(|line| line.text).collect()))
        .collect()
}

/// INCOG-3: an ordinary session from before may still be resumed and read, and it is not updated
/// while the incognito session runs.
///
/// An incognito session that resumes a record has a handle whose `wrote` flag is set, since the
/// record it came from is the one it would write back to. Three mistakes would reach the file from
/// there: a save or an audit append that resolves the directory for reading and so finds the old
/// record, a rewrite for a new title that reads and amends it, and a discard that removes it. Each
/// is exercised and the bytes on disk are compared after every step, so the failure names the step.
/// The record and its trail are also read back while engaged, because a read refused in this mode
/// would leave nothing to resume.
#[test]
fn an_ordinary_sessions_record_and_audit_are_left_as_they_were_by_an_incognito_one() {
    let root = std::env::temp_dir().join(format!(
        "bravebot-incognito-resume-test-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let home = root.join("home");
    let project = root.join("project");
    std::fs::create_dir_all(&home).expect("create home");
    std::fs::create_dir_all(&project).expect("create project");
    // SAFETY: this is the only test in this binary, so nothing else reads or writes `HOME` while it
    // runs.
    unsafe { std::env::set_var("HOME", &home) };
    unsafe { std::env::remove_var("BRAVEBOT_HOME") };
    let own_directory = home.join(".bravebot");
    let on_disk = || {
        let mut found = BTreeMap::new();
        bytes_under(&own_directory, &own_directory, &mut found);
        found
    };

    // An ordinary session writes a record and a trail, then ends.
    let id = {
        let mut ordinary = Handle::begin(&project, Front::Terminal, bravebot_stamp::BUILD);
        ordinary.save(
            "what an ordinary session worked on",
            standing!(conversation_of(1), 1, 1_200),
        );
        ordinary.append_audit(1, &[decided("the first decision")]);
        ordinary.id().to_string()
    };
    let before = on_disk();
    assert!(
        before
            .keys()
            .any(|path| path.to_string_lossy().ends_with(&format!("{id}.json")))
            && before.keys().any(|path| path
                .to_string_lossy()
                .ends_with(&format!("{id}.audit.jsonl"))),
        "the ordinary session wrote no record and trail, so this test proves nothing"
    );
    let listed_before: Vec<_> = sessions::list(&project)
        .into_iter()
        .map(|summary| (summary.id, summary.title))
        .collect();
    assert_eq!(
        listed_before,
        vec![(id.clone(), "what an ordinary session worked on".to_string())]
    );
    let trail_before = trail_text(&project, &id);
    assert_eq!(
        trail_before.keys().copied().collect::<Vec<_>>(),
        vec![1],
        "the seeded trail was not read back"
    );

    // The next session is incognito. It can list the record, read it and resume it.
    bravebot_core::incognito::engage();
    let listed = sessions::list(&project);
    assert_eq!(
        listed.len(),
        1,
        "an incognito session could not list the ordinary session"
    );
    let record = sessions::load(&project, &id).expect("an incognito session reads the record");
    assert_eq!(record.title, "what an ordinary session worked on");
    assert_eq!(record.turns, 1);
    assert_eq!(record.conversation.messages.len(), 2);
    assert_eq!(
        trail_text(&project, &id),
        trail_before,
        "an incognito session could not read the trail"
    );

    // It works on: another turn is saved and audited, the session is named, a rewind drops the
    // trail from the second turn, and the work is thrown away.
    let mut resumed = Handle::resuming(&project, &record, Front::Terminal, bravebot_stamp::BUILD)
        .expect("an incognito session resumes a record nothing else holds");
    assert_eq!(resumed.id(), id);

    resumed.save("a later question", standing!(conversation_of(2), 2, 2_400));
    assert_unchanged(
        &on_disk(),
        &before,
        "an incognito session saved over the record it resumed",
    );

    resumed.append_audit(2, &[decided("a decision in the incognito session")]);
    assert_unchanged(
        &on_disk(),
        &before,
        "an incognito session added to the audit trail it resumed",
    );

    assert!(resumed.rename("a new title"));
    assert_unchanged(
        &on_disk(),
        &before,
        "an incognito session retitled the record it resumed",
    );

    resumed.truncate_audit(1);
    assert_unchanged(
        &on_disk(),
        &before,
        "an incognito session cut turns out of the audit trail it resumed",
    );

    resumed.discard_unwritten("");
    assert_unchanged(
        &on_disk(),
        &before,
        "an incognito session removed the record and trail it resumed",
    );

    // And afterwards the record is what the ordinary session left, still readable.
    let after = sessions::load(&project, &id).expect("the record is still there");
    assert_eq!(after.title, "what an ordinary session worked on");
    assert_eq!(after.turns, 1);
    assert_eq!(after.tokens, 1_200);
    assert_eq!(after.revision, record.revision);
    assert_eq!(trail_text(&project, &id), trail_before);

    let _ = std::fs::remove_dir_all(&root);
}
