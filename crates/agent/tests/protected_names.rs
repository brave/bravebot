//! The names the sandbox holds back beneath a write row are the names the file tools put to the
//! person (SANDBOX-14, PERM-9).

use bravebot_core::protected::{DIRECTORIES, FILES, is_protected};
use bravebot_sandbox::policy::{PROTECTED_DIRECTORIES, PROTECTED_FILES};

/// A name added to one list and not the other leaves a write the file tools ask about, or the
/// sandbox refuses, on one road only.
#[test]
fn the_sandbox_and_the_file_tools_hold_back_the_same_names() {
    assert_eq!(DIRECTORIES, PROTECTED_DIRECTORIES);
    assert_eq!(FILES, PROTECTED_FILES);
}

/// The names in the list are the ones `is_protected` accepts, so a list that drifted to a name
/// the file tools do not read is caught here as well as by the equality above.
#[test]
fn every_name_the_sandbox_holds_back_is_protected_for_the_file_tools() {
    for name in PROTECTED_DIRECTORIES.iter().chain(PROTECTED_FILES.iter()) {
        assert!(is_protected(name), "{name} is not protected");
        assert!(
            is_protected(&format!("sub/{name}/entry")),
            "{name} is not protected beneath a directory"
        );
    }
}
