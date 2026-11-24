//! An incognito session reads the record of directories a stage may write and writes nothing to it
//! (INCOG-3, INCOG-5, SANDBOX-30).
//!
//! A binary of its own: engaging incognito lasts for the process, and `HOME` is process-wide.
#![cfg(unix)]
#![allow(
    clippy::unwrap_used,
    reason = "a test that unwraps fails at the line that did"
)]

use bravebot_agent::stage_record;
use bravebot_agent::workspace::Workspace;
use std::ffi::OsStr;

#[path = "../../sandbox/test-support/scratch_root.rs"]
mod scratch_root;

/// An incognito session finds the directories an ordinary one recorded and adds none: the record is
/// byte for byte what it was and nothing else appears beside it in the state directory. Rejects a
/// loader that skips the record under the mode (the planted program is found), a sink that writes
/// through the reading answer of the state directory instead of the writing one (a line appears),
/// and a session that makes the state directory's other files.
#[test]
fn an_incognito_session_reads_the_record_and_adds_nothing_to_it() {
    let root = scratch_root::scratch_dir(&format!("stage-record-incognito-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let home = root.join("home");
    let state = home.join(".bravebot");
    std::fs::create_dir_all(&state).unwrap();
    let root = root.canonicalize().unwrap();
    let home = home.canonicalize().unwrap();
    let state = home.join(".bravebot");
    let granted = root.join("granted");
    let installed_bin = root.join("installed").join("bin");
    for bin in [granted.join("bin"), installed_bin.clone()] {
        std::fs::create_dir_all(&bin).unwrap();
        let program = bin.join("bravebot-incognito-probe");
        std::fs::write(&program, "#!/bin/sh\n").unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let held = format!("directory\t{}\n", granted.display());
    std::fs::write(state.join(stage_record::FILE), &held).unwrap();
    // SAFETY: the only test of this binary, and nothing starts a thread before it.
    unsafe {
        std::env::set_var("HOME", &home);
        std::env::remove_var(bravebot_agent::home::STATE_VARIABLE);
    }
    bravebot_core::incognito::engage();
    assert!(bravebot_agent::home::writable().is_none());
    let path = std::env::join_paths([granted.join("bin"), installed_bin.clone()]).unwrap();
    let program = OsStr::new("bravebot-incognito-probe");
    assert_eq!(
        bravebot_sandbox::programs::find_on(program, &path),
        Some(granted.join("bin").join("bravebot-incognito-probe")),
        "the control: nothing is loaded yet, so the lookup takes the first entry"
    );

    stage_record::load();
    stage_record::follow();
    let opened = root.join("opened");
    // Its own checkout: the scratch root is inside this repository, whose root would otherwise be
    // the checkout the session records and would hold the installed directory too.
    std::fs::create_dir_all(opened.join(".git")).unwrap();
    bravebot_sandbox::programs::keep_out(&root.join("recorded-by-a-stage-row"));
    bravebot_sandbox::programs::keep_out_where_writable(&root.join("a-machine-row"));
    let _workspace = Workspace::new(&opened).unwrap();

    assert_eq!(
        bravebot_sandbox::programs::find_on(program, &path),
        Some(installed_bin.join("bravebot-incognito-probe")),
        "the record was not read in an incognito session"
    );
    assert_eq!(
        std::fs::read_to_string(state.join(stage_record::FILE)).unwrap(),
        held,
        "an incognito session added to the record"
    );
    let beside: Vec<_> = std::fs::read_dir(&state)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(beside, [std::ffi::OsString::from(stage_record::FILE)]);
}
