//! `bravebot completion <shell>`: a shell completion script on stdout (CLI-20).
//!
//! The words completed are the ones bravebot defines, listed below. The scripts are built from those
//! lists and nothing read at run time, so the command touches no state, and a test holds the lists
//! to what `--help` prints.

use std::io::Write;

/// The shells a script is written for, in the order the usage line names them.
pub(crate) const SHELLS: [&str; 3] = ["bash", "zsh", "fish"];

/// The subcommands, as the usage table lists them.
pub(crate) const COMMANDS: [&str; 9] = [
    "doctor",
    "auth",
    "mcp",
    "sessions",
    "attach",
    "reply",
    "import-leo-creds",
    "import-providers",
    "completion",
];

/// What follows `auth`.
const AUTH: [&str; 2] = ["login", "logout"];

/// What follows `mcp`.
const MCP: [&str; 8] = [
    "add", "get", "list", "approve", "enable", "disable", "remove", "forget",
];

/// A flag, as the usage table spells it: the long form, the short form where there is one, and
/// whether a value follows.
struct Flag {
    long: &'static str,
    short: Option<char>,
    value: bool,
}

const fn flag(long: &'static str, short: Option<char>, value: bool) -> Flag {
    Flag { long, short, value }
}

const FLAGS: [Flag; 26] = [
    flag("plain", None, false),
    flag("bg", None, false),
    flag("resume", Some('r'), false),
    flag("continue", Some('c'), false),
    flag("fork", Some('f'), false),
    flag("from-pr", None, true),
    flag("file", None, true),
    flag("add-dir", None, true),
    flag("settings", None, true),
    flag("agent", None, true),
    flag("system-prompt", None, true),
    flag("append-system-prompt", None, true),
    flag("mode", None, true),
    flag("model", None, true),
    flag("effort", None, true),
    flag("advisor", None, true),
    flag("print", Some('p'), false),
    flag("trace", None, false),
    flag("json", None, false),
    flag("json-stream", None, false),
    flag("incognito", None, false),
    flag("safe", None, false),
    flag("vet", None, false),
    flag("dangerously-skip-permissions", None, false),
    flag("help", Some('h'), false),
    flag("version", Some('V'), false),
];

/// Every flag as typed, long and short, for the shells that take them as one list.
fn flag_words() -> String {
    let mut words = Vec::new();
    for flag in &FLAGS {
        words.push(format!("--{}", flag.long));
        if let Some(short) = flag.short {
            words.push(format!("-{short}"));
        }
    }
    words.join(" ")
}

/// The script for `shell`, or `None` for a name that is not one of [`SHELLS`].
pub(crate) fn script(shell: &str) -> Option<String> {
    match shell {
        "bash" => Some(bash()),
        "zsh" => Some(zsh()),
        "fish" => Some(fish()),
        _ => None,
    }
}

/// `bravebot completion`: print the script the one argument names.
///
/// `None` is a command line this does not take: no shell, a shell it has no script for, or
/// anything after it.
pub(crate) fn command(args: &[String]) -> Option<()> {
    let [shell] = args else {
        return None;
    };
    let script = script(shell)?;
    // A closed stdout is a caller that stopped reading, as it is for the reply.
    let _ = std::io::stdout().lock().write_all(script.as_bytes());
    Some(())
}

fn bash() -> String {
    format!(
        r#"_bravebot() {{
    local cur="${{COMP_WORDS[COMP_CWORD]}}"
    COMPREPLY=()
    if [ "$COMP_CWORD" -eq 2 ]; then
        case "${{COMP_WORDS[1]}}" in
            auth) COMPREPLY=($(compgen -W "{auth}" -- "$cur")); return 0 ;;
            mcp) COMPREPLY=($(compgen -W "{mcp}" -- "$cur")); return 0 ;;
            completion) COMPREPLY=($(compgen -W "{shells}" -- "$cur")); return 0 ;;
        esac
    fi
    case "$cur" in
        -*) COMPREPLY=($(compgen -W "{flags}" -- "$cur")) ;;
        *)
            if [ "$COMP_CWORD" -eq 1 ]; then
                COMPREPLY=($(compgen -W "{commands}" -- "$cur"))
            fi
            ;;
    esac
    return 0
}}
complete -o default -F _bravebot bravebot
"#,
        auth = AUTH.join(" "),
        mcp = MCP.join(" "),
        shells = SHELLS.join(" "),
        flags = flag_words(),
        commands = COMMANDS.join(" "),
    )
}

fn zsh() -> String {
    format!(
        r#"#compdef bravebot
_bravebot() {{
    if (( CURRENT == 3 )); then
        case "$words[2]" in
            auth) compadd -- {auth}; return ;;
            mcp) compadd -- {mcp}; return ;;
            completion) compadd -- {shells}; return ;;
        esac
    fi
    if [[ "$words[CURRENT]" == -* ]]; then
        compadd -- {flags}
    elif (( CURRENT == 2 )); then
        compadd -- {commands}
        _files
    else
        _files
    fi
}}
if [ "$funcstack[1]" = "_bravebot" ]; then
    _bravebot "$@"
elif (( $+functions[compdef] )); then
    compdef _bravebot bravebot
fi
"#,
        auth = AUTH.join(" "),
        mcp = MCP.join(" "),
        shells = SHELLS.join(" "),
        flags = flag_words(),
        commands = COMMANDS.join(" "),
    )
}

fn fish() -> String {
    let mut script = String::new();
    script.push_str(&format!(
        "complete -c bravebot -f -n __fish_use_subcommand -a '{}'\n",
        COMMANDS.join(" ")
    ));
    for (command, words) in [
        ("auth", AUTH.join(" ")),
        ("mcp", MCP.join(" ")),
        ("completion", SHELLS.join(" ")),
    ] {
        script.push_str(&format!(
            "complete -c bravebot -f -n '__fish_seen_subcommand_from {command}' -a '{words}'\n"
        ));
    }
    for flag in &FLAGS {
        let mut line = String::from("complete -c bravebot");
        if let Some(short) = flag.short {
            line.push_str(&format!(" -s {short}"));
        }
        line.push_str(&format!(" -l {}", flag.long));
        if flag.value {
            line.push_str(" -r");
        }
        script.push_str(&line);
        script.push('\n');
    }
    script
}
