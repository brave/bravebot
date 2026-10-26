//! The panic hook: leave one file and make no request (CRASH-2).
//!
//! A panicking process may be holding any lock, may be out of memory, and with `panic = "abort"`
//! is about to end whatever this does, so the hook does the least that keeps the report: it builds
//! the text from numbers and words, writes one small file, and returns. The file is sent by a later
//! launch, from a process that is working.

use crate::report::{self, Identity, Panic, Program};
use crate::store;
use std::panic::PanicHookInfo;
use std::path::{Path, PathBuf};

/// Run [`leave_a_report`] on every panic, then whatever hook was there before, so the message the
/// person sees is the one they would have seen.
pub(crate) fn install(program: Program, state: PathBuf) {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        leave_a_report(&program, &state, info);
        previous(info);
    }));
}

/// Write the report for one panic. Every failure is dropped: nothing here may be the reason a
/// process that is already failing says less about why.
fn leave_a_report(program: &Program, state: &Path, info: &PanicHookInfo<'_>) {
    let body = report::document(program, &panic_of(info), &Identity::fresh());
    #[expect(
        clippy::let_underscore_must_use,
        reason = "a panicking process has nowhere to put a failure to write, and says less about why otherwise"
    )]
    let _ = store::write(
        state,
        body.as_bytes(),
        store::now_millis(),
        std::process::id(),
    );
}

/// Where `info` says the panic was, and its message where the source wrote it.
///
/// A payload that is a `&'static str` is a message with no run-time values in it: the standard
/// library hands one over only when the format string had no arguments, and every other message
/// arrives as a `String` it built. The type is therefore what tells the two apart, and a `String`
/// is left out (CRASH-1).
fn panic_of(info: &PanicHookInfo<'_>) -> Panic {
    let message = info.payload().downcast_ref::<&'static str>().copied();
    match info.location() {
        Some(at) => Panic::at(at.file(), at.line(), at.column(), message),
        None => Panic::at("", 0, 0, message),
    }
}
