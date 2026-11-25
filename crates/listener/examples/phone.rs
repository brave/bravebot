// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! A phone at a terminal, for trying a host that was started with `--listen` by hand.
//!
//! Type `pair NAME read|control WORKSPACE` on the host, then run
//! `cargo run -p bravebot-listener --example phone --features testing -- 'LINK'` with the link the
//! host shows. It writes a code, which you type on the host as `confirm CODE`. Once the host says it
//! is paired, each line you type is sent to the host as a request and the answers are written.
//! `{"id":1,"method":"instance.info","params":{}}` is one to try.

#![forbid(unsafe_code)]

fn main() {
    let link = match bravebot_listener::phone::link_from_arguments(std::env::args_os().skip(1)) {
        Ok(link) => link,
        Err(why) => {
            eprintln!("{why}");
            std::process::exit(2);
        }
    };
    let stdin = std::io::stdin();
    let result = bravebot_listener::phone::pair_and_talk(
        &link,
        &mut stdin.lock(),
        &mut std::io::stdout().lock(),
    );
    if let Err(error) = result {
        eprintln!("phone: {error}");
        std::process::exit(1);
    }
}
