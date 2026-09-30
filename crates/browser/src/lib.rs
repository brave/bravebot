//! Reaching a Brave extension from a BraveBot session.
//!
//! One program in two roles. BraveBot starts it as a stdio MCP server, confined, in the directory
//! its declaration names. Brave starts it as the extension's native messaging host, unconfined,
//! when the extension connects. The host listens on a Unix socket in that directory and the server
//! connects to it, since a confined process can connect to a Unix socket on every platform this
//! runs on and can create one on none of them reliably.
//!
//! This crate relays and decides nothing about what it relays. It reads the `id` of each message
//! to route a reply back to the session that asked, and no other field.
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
pub mod relay;
#[cfg(unix)]
pub mod server;
