//! `bravebot-browser`: the relay between a BraveBot session and a Brave extension.
//!
//! Brave starts it with the extension's origin as its first argument, and it serves as the native
//! messaging host. BraveBot starts it as `bravebot-browser mcp`, and it serves as a stdio MCP
//! server, for one group of tools where it is given `--group`. A person runs
//! `bravebot-browser install` once, naming an extension id only where it is not the one in
//! `extension/`, and `bravebot-browser profile` to be given the command that starts a Brave of
//! its own for the extension.
//!
//! **This is the only file in the crate that ends the process.** The native host's stdout is
//! native messaging's stream and the MCP server's is the protocol, so nothing else prints to it.

#![forbid(unsafe_code)]

#[cfg(unix)]
fn main() {
    use bravebot_browser::{host, install, paths, profile, server, tools};
    use std::ffi::{OsStr, OsString};
    use std::io::Write;
    use std::os::unix::ffi::OsStrExt;
    use std::path::{Path, PathBuf};

    // Brave names the extension here, and a person names a subcommand. A command-line program has
    // no other way to learn either. The origin is checked against the recorded extension, and the
    // secret, not this, is what keeps other processes off the relay.
    // nosemgrep: rust.lang.security.args.args
    let arguments: Vec<OsString> = std::env::args_os().skip(1).collect();
    let words = command_words(&arguments);

    let result = match words.as_slice() {
        [origin, ..] if origin.starts_with("chrome-extension://") => {
            match paths::host_directory() {
                Some(directory) => host::run(&directory, origin),
                None => Err(std::io::Error::other("there is no home directory to use")),
            }
        }
        ["mcp", rest @ ..] => {
            // The group is a word of the declaration a person wrote, and one that is not a group
            // starts no server, so a typo cannot offer every tool.
            let group = match rest {
                [] => None,
                ["--group", id] => Some(tools::group(id).unwrap_or_else(|| usage())),
                _ => usage(),
            };
            server::run(
                Path::new("."),
                group,
                std::io::stdin().lock(),
                &mut std::io::stdout(),
            )
        }
        ["install", rest @ ..] => {
            let Some(asked) = install::arguments(rest) else {
                usage()
            };
            let manifests = match asked.manifests {
                Some(manifests) => Some(PathBuf::from(manifests)),
                None => install::manifest_directory(),
            };
            let extension = asked.extension.unwrap_or(install::EXTENSION_ID);
            let installed = match (manifests, paths::host_directory()) {
                // The manifest names the program the person ran, for their own browser to start.
                // It is written as that person and grants nothing they could not write themselves.
                // nosemgrep: rust.lang.security.current-exe.current-exe
                (Some(manifests), Some(directory)) => std::env::current_exe().and_then(|program| {
                    install::install(extension, &program, &manifests, &directory)
                }),
                _ => Err(std::io::Error::other("there is no home directory to use")),
            };
            installed.and_then(|installed| {
                // A declaration is written as the bytes it holds, since a directory or a program
                // whose name is not UTF-8 has no text to print.
                let mut out = std::io::stdout().lock();
                writeln!(out, "Wrote {}", installed.manifest.display())?;
                writeln!(out, "The host serves the extension {extension}.")?;
                let lines: Vec<_> = if asked.groups.is_empty() {
                    writeln!(out, "Declare the server with:")?;
                    vec![installed.declaration(None)]
                } else {
                    writeln!(out, "Declare a server for each group with:")?;
                    asked
                        .groups
                        .iter()
                        .map(|group| installed.declaration(Some(group)))
                        .collect()
                };
                writeln!(out)?;
                for line in lines {
                    out.write_all(b"  ")?;
                    out.write_all(line.as_bytes())?;
                    out.write_all(b"\n")?;
                }
                Ok(())
            })
        }
        ["profile"] => {
            let rest: Vec<&OsStr> = arguments.iter().skip(1).map(OsString::as_os_str).collect();
            let Some(asked) = profile::arguments(&rest) else {
                usage()
            };
            let path = std::env::var_os("PATH");
            profile::start_command(&asked, path.as_deref()).and_then(|start| {
                if start.may_be_running {
                    eprintln!(
                        "bravebot-browser: a Brave may already be running on {}, which holds {}. \
                         A Brave started on it again does not start a second browser.",
                        Path::new(asked.profile).display(),
                        profile::SINGLETON_LOCK
                    );
                }
                // The command is bytes, since a directory whose name is not UTF-8 is still the
                // directory Brave is to be started on.
                let mut out = std::io::stdout().lock();
                out.write_all(start.line.as_bytes())?;
                out.write_all(b"\n")
            })
        }
        _ => usage(),
    };

    if let Err(error) = result {
        eprintln!("bravebot-browser: {error}");
        std::process::exit(1);
    }
}

/// The words `main` matches on. `profile` takes directories, which are bytes, so it reads the
/// arguments itself and is matched on its name alone. Every other word is text, and one that is
/// not ends the process, since a rewritten or dropped word would change what was asked.
#[cfg(unix)]
fn command_words(arguments: &[std::ffi::OsString]) -> Vec<&str> {
    if arguments.first().is_some_and(|word| word == "profile") {
        return vec!["profile"];
    }
    let text: Option<Vec<&str>> = arguments.iter().map(|word| word.to_str()).collect();
    text.unwrap_or_else(|| {
        eprintln!(
            "bravebot-browser: an argument is not valid UTF-8, which this subcommand does not take"
        );
        std::process::exit(2);
    })
}

#[cfg(unix)]
fn usage() -> ! {
    use bravebot_browser::{install, tools};

    eprintln!(
        "usage: bravebot-browser install [--manifest-dir <dir>] [--groups <id>,<id> | --preset <name>]"
    );
    eprintln!("                                [<extension id>]");
    eprintln!("       without an extension id, install records the one in extension/");
    eprintln!(
        "       bravebot-browser profile --profile-dir <dir> --extension-dir <dir> [--brave <path>]"
    );
    eprintln!("       bravebot-browser mcp [--group <id>]");
    eprintln!("groups: {}", tools::ids().collect::<Vec<_>>().join(", "));
    let presets: Vec<&str> = install::PRESETS.iter().map(|(name, _)| *name).collect();
    eprintln!("presets: {}", presets.join(", "));
    std::process::exit(2);
}

#[cfg(not(unix))]
fn main() {
    eprintln!("bravebot-browser: this platform is not supported yet");
    std::process::exit(1);
}
