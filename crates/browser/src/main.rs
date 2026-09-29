//! `bravebot-browser`: the relay between a BraveBot session and a Brave extension.
//!
//! Brave starts it with the extension's origin as its first argument, and it serves as the native
//! messaging host. BraveBot starts it as `bravebot-browser mcp`, and it serves as a stdio MCP
//! server. A person runs `bravebot-browser install` once, naming an extension id only where it is
//! not the one in `extension/`.
//!
//! **This is the only file in the crate that ends the process.** The native host's stdout is
//! native messaging's stream and the MCP server's is the protocol, so nothing else prints to it.

#![forbid(unsafe_code)]

#[cfg(unix)]
fn main() {
    use bravebot_browser::{host, install, paths, server};
    use std::path::{Path, PathBuf};

    // Brave names the extension here, and a person names a subcommand. A command-line program has
    // no other way to learn either. The origin is checked against the recorded extension, and the
    // secret, not this, is what keeps other processes off the relay.
    // nosemgrep: rust.lang.security.args.args
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let words: Vec<&str> = arguments.iter().map(String::as_str).collect();

    let result = match words.as_slice() {
        [origin, ..] if origin.starts_with("chrome-extension://") => {
            match paths::host_directory() {
                Some(directory) => host::run(&directory, origin),
                None => Err(std::io::Error::other("there is no home directory to use")),
            }
        }
        ["mcp"] => server::run(
            Path::new("."),
            std::io::stdin().lock(),
            &mut std::io::stdout(),
        ),
        ["install", rest @ ..] => {
            let (manifests, rest) = match rest {
                ["--manifest-dir", manifests, rest @ ..] => (Some(PathBuf::from(manifests)), rest),
                rest => (install::manifest_directory(), rest),
            };
            let extension = match rest {
                [] => install::EXTENSION_ID,
                [extension] => *extension,
                _ => usage(),
            };
            let installed = match (manifests, paths::host_directory()) {
                // The manifest names the program the person ran, for their own browser to start.
                // It is written as that person and grants nothing they could not write themselves.
                // nosemgrep: rust.lang.security.current-exe.current-exe
                (Some(manifests), Some(directory)) => std::env::current_exe().and_then(|program| {
                    install::install(extension, &program, &manifests, &directory)
                }),
                _ => Err(std::io::Error::other("there is no home directory to use")),
            };
            installed.map(|installed| {
                println!("Wrote {}", installed.manifest.display());
                println!("Declare the server with:");
                println!();
                println!(
                    "  bravebot mcp add brave -s user --dir {} -- {} mcp",
                    installed.directory.display(),
                    installed.program.display()
                );
            })
        }
        _ => usage(),
    };

    if let Err(error) = result {
        eprintln!("bravebot-browser: {error}");
        std::process::exit(1);
    }
}

#[cfg(unix)]
fn usage() -> ! {
    eprintln!("usage: bravebot-browser install [--manifest-dir <dir>] [<extension id>]");
    eprintln!("       bravebot-browser mcp");
    std::process::exit(2);
}

#[cfg(not(unix))]
fn main() {
    eprintln!("bravebot-browser: this platform is not supported yet");
    std::process::exit(1);
}
