// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! The local control channel: an anonymous socket pair that a host-chosen program receives as its
//! standard input and output.
//!
//! [`launch`] is the launcher for the persistent host. The module is hidden from the crate's
//! documentation until a caller outside the tests settles what it offers.
//!
//! A named endpoint (a socket path, a port, a token in a file) can be found and used by any program
//! running as the same user, including a command a person approved and one that detached from its
//! parent, and nothing about such a peer (its user, its parent, its directory) tells it apart
//! from the client. This channel has no name. Whoever launches the program that speaks to the
//! bridge creates the pair, gives the program one end and keeps the other, and no other process
//! holds either end unless a descriptor leaks to it. A socket pair is required: on Linux a pipe
//! can be reopened by path through `/proc/<pid>/fd` and a socket cannot.
//!
//! The launcher must pick the executable and its arguments from the person's own configuration,
//! never from a tool result, a workspace file or a program a tool can replace. The host must keep
//! every descriptor it holds for a control channel closed on exec in every place it starts a tool.
//! `tests/control_boundary.rs` starts the bridge's own hook and `run` paths and checks that
//! neither hands a tool the channel.
//!
//! The standard library creates a socket pair and marks it close-on-exec in two steps on macOS, so
//! a process another thread starts in between inherits both ends. [`launch`] creates the pair and
//! starts the program under `bravebot_sandbox::process::creating_a_channel`, the one gate that
//! every other start of a program in the agent also takes (`starting_a_program`), so no start
//! overlaps the creation of a pair. A host that starts programs some other way takes the same gate
//! around those starts.
//!
//! A client that exits ends its channel. A replacement is a new launch with a new pair, chosen by
//! the host. A program that was not launched this way cannot attach to a running host, because
//! there is no name to find.

use bravebot_sandbox::process::creating_a_channel;
use std::io;
use std::os::fd::OwnedFd;
use std::os::unix::net::UnixStream;
use std::process::{Child, Command, Stdio};

/// A program launched with a control channel, and the launcher's end of that channel.
pub struct Linked {
    pub child: Child,
    /// The launcher's end. It has no name, and no other process holds its peer once the child
    /// exits.
    pub channel: UnixStream,
}

/// Starts `command` with the far end of a new socket pair as both its standard input and its
/// standard output, and returns the near end. Its standard error is as `command` set it.
///
/// Neither descriptor of the pair is inheritable once it is created, so the child holds the far
/// end only as descriptors 0 and 1, and a process the child starts holds it only if the child
/// passes those streams on. The launcher's copy of the far end is closed before this returns, so
/// the channel reaches end of file when the child exits.
pub fn launch(mut command: Command) -> io::Result<Linked> {
    creating_a_channel(|| {
        let (channel, far) = UnixStream::pair()?;
        let writing = far.try_clone()?;
        command
            .stdin(Stdio::from(OwnedFd::from(far)))
            .stdout(Stdio::from(OwnedFd::from(writing)));
        let child = command.spawn();
        drop(command);
        Ok(Linked {
            child: child?,
            channel,
        })
    })
}
