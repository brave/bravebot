//! `bravebot shell-init <shell>`: the hook that gives `@bravebot` the commands a person ran (SHELLINT-1).
//!
//! The scripts are fixed text. They read nothing at run time and name no path of the machine that
//! printed them, so the command touches no state. What a script does once a shell has sourced it
//! is [SHELLINT-2] to [SHELLINT-5], and the tests run the bash one against a real `bash`.

use std::io::Write;

/// The script for `shell`, or `None` for a name that is not one of [`crate::completion::SHELLS`].
pub(crate) fn script(shell: &str) -> Option<&'static str> {
    match shell {
        "bash" => Some(BASH),
        "zsh" => Some(ZSH),
        "fish" => Some(FISH),
        _ => None,
    }
}

/// `bravebot shell-init`: print the script the one argument names.
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

const BASH: &str = r#"# bravebot shell integration: add `eval "$(bravebot shell-init bash)"` to ~/.bashrc
__bravebot_dir="${HOME}/.bravebot/shell"
__bravebot_file="${__bravebot_dir}/$$"
__bravebot_last=
(umask 077; mkdir -p "$__bravebot_dir" && chmod 700 "${HOME}/.bravebot" "$__bravebot_dir" && : > "$__bravebot_file")
__bravebot_record() {
    local status=$? line number
    if [ -z "$BRAVEBOT_INCOGNITO" ]; then
        line=$(HISTTIMEFORMAT= builtin history 1)
        line=${line#"${line%%[![:space:]]*}"}
        number=${line%%[[:space:]]*}
        if [ "$number" != "$__bravebot_last" ]; then
            __bravebot_last=$number
            line=${line#"$number"}
            line=${line#"${line%%[![:space:]]*}"}
            line=${line//$'\n'/ }
            case $line in
                ''|'@bravebot'|'@bravebot'[[:space:]]*) ;;
                *)
                    [ -e "$__bravebot_file" ] || (umask 077; : > "$__bravebot_file")
                    printf '%s\n' "$line" >> "$__bravebot_file"
                    ;;
            esac
        fi
    fi
    return $status
}
case "$(declare -p PROMPT_COMMAND 2>/dev/null)" in
    "declare -a"*) PROMPT_COMMAND=(__bravebot_record "${PROMPT_COMMAND[@]}") ;;
    *) PROMPT_COMMAND="__bravebot_record${PROMPT_COMMAND:+;$PROMPT_COMMAND}" ;;
esac
__bravebot_chain_exit() {
    eval "set -- $(trap -p EXIT)"
    trap "rm -f -- \"\$__bravebot_file\"; ${3-}" EXIT
}
__bravebot_chain_exit
@bravebot() {
    {
        tail -n 200 "$__bravebot_file" 2>/dev/null | tail -c 65536
        [ -e "$__bravebot_file" ] && : > "$__bravebot_file"
    } | command bravebot ${BRAVEBOT_INCOGNITO:+--incognito} -p "$@"
}
"#;

const ZSH: &str = r#"# bravebot shell integration: add `eval "$(bravebot shell-init zsh)"` to ~/.zshrc
__bravebot_dir="${HOME}/.bravebot/shell"
__bravebot_file="${__bravebot_dir}/$$"
(umask 077; mkdir -p "$__bravebot_dir" && chmod 700 "${HOME}/.bravebot" "$__bravebot_dir" && : > "$__bravebot_file")
__bravebot_record() {
    [[ -n $BRAVEBOT_INCOGNITO ]] && return 0
    local line=${1//$'\n'/ }
    case $line in
        ''|'@bravebot'|'@bravebot'[[:space:]]*) ;;
        *)
            [[ -e $__bravebot_file ]] || (umask 077; : > "$__bravebot_file")
            print -r -- "$line" >> "$__bravebot_file"
            ;;
    esac
    return 0
}
__bravebot_exit() {
    rm -f -- "$__bravebot_file"
}
autoload -Uz add-zsh-hook
add-zsh-hook preexec __bravebot_record
add-zsh-hook zshexit __bravebot_exit
@bravebot() {
    {
        tail -n 200 "$__bravebot_file" 2>/dev/null | tail -c 65536
        [[ -e $__bravebot_file ]] && : > "$__bravebot_file"
    } | command bravebot ${BRAVEBOT_INCOGNITO:+--incognito} -p "$@"
}
"#;

const FISH: &str = r#"# bravebot shell integration: add `bravebot shell-init fish | source` to ~/.config/fish/config.fish
set -g __bravebot_dir $HOME/.bravebot/shell
set -g __bravebot_file $__bravebot_dir/$fish_pid
begin
    set -l old (umask)
    umask 077
    mkdir -p $__bravebot_dir
    chmod 700 $HOME/.bravebot $__bravebot_dir
    printf '' > $__bravebot_file
    umask $old
end
function __bravebot_record --on-event fish_preexec
    test -n "$BRAVEBOT_INCOGNITO"; and return 0
    set -l line (string join ' ' (string split \n -- $argv[1]))
    switch $line
        case '' '@bravebot' '@bravebot *'
            return 0
    end
    if not test -e $__bravebot_file
        set -l old (umask)
        umask 077
        printf '' > $__bravebot_file
        umask $old
    end
    printf '%s\n' $line >> $__bravebot_file
end
function __bravebot_exit --on-event fish_exit
    rm -f -- $__bravebot_file
end
function @bravebot
    set -l flags
    test -n "$BRAVEBOT_INCOGNITO"; and set flags --incognito
    begin
        tail -n 200 $__bravebot_file 2>/dev/null | tail -c 65536
        test -e $__bravebot_file; and printf '' > $__bravebot_file
    end | command bravebot $flags -p $argv
end
"#;
