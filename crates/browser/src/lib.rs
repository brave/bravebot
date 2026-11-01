//! Reaching a Brave extension from a BraveBot session.
//!
//! One program in two roles. BraveBot starts it as a stdio MCP server, confined, in the directory
//! its declaration names. Brave starts it as the extension's native messaging host, unconfined,
//! when the extension connects. The host listens on a Unix socket in that directory and the server
//! connects to it, since a confined process can connect to a Unix socket on every platform this
//! runs on and can create one on none of them reliably.
//!
//! This crate relays and decides nothing about what it relays. The host reads the `id` of each
//! message to route a reply back to the session that asked. The server reads a request's `method`
//! and a call's `name`, which it looks up in a table of its own, and passes the call's `arguments`
//! on unread. The relay reads the `error` and `result` of the extension's reply and passes what
//! they hold on as text, whatever it is.
//!
//! The one exception to passing a call's `arguments` on unread is the tool its table marks as
//! taking a file (`choose_file`). The server started for its group reads the call's `path`, which
//! the planner wrote and the person reads in the question, and looks at the file it leads to inside
//! the directory `uploads` of the directory it was declared with. It sends the extension that
//! file's canonical path, size and name in place of the text, or answers with one sentence of a
//! closed list and sends nothing. It reads no byte of a page or of a result, never opens the file,
//! and the half Brave starts unconfined does none of this ([`uploads`]).
//!
//! `docs/specs/browser.md` is what this has to do, clause by clause.

#![forbid(unsafe_code)]

pub mod framing;
pub mod tools;

#[cfg(unix)]
pub mod host;
#[cfg(unix)]
pub mod install;
#[cfg(unix)]
pub mod lines;
#[cfg(unix)]
pub mod paths;
#[cfg(unix)]
pub mod profile;
#[cfg(unix)]
pub mod relay;
#[cfg(unix)]
pub mod server;
#[cfg(unix)]
pub mod uploads;
