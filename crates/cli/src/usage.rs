//! The usage table printed for `--help` and for a command line that could not be parsed.

use crate::VERSION;
use bravebot_i18n::t;

pub(crate) fn print_help() {
    /// Wide enough for the longest invocation below, so a translated description starts in the
    /// same column as every other one rather than wherever hand-counted spaces left it.
    const FORM: usize = 39;
    /// The same, for the key column and the option column.
    const KEY: usize = 22;
    /// Wide enough for `--dangerously-skip-permissions` and a gap. A narrower column would leave
    /// that one flag touching its own description, since the padding below cannot go negative.
    const OPTION: usize = 32;

    println!("{}", t!(cli_tagline, version = VERSION));
    println!();
    println!("{}", t!(cli_usage_heading));
    for (form, description) in [
        ("bravebot", t!(cli_usage_interactive)),
        ("bravebot -i \"<prompt>\"", t!(cli_usage_interactive_prompt)),
        ("bravebot --plain", t!(cli_usage_plain)),
        ("bravebot \"<task>\" [--file <path>]...", t!(cli_usage_task)),
        ("cat file | bravebot -p \"<task>\"", t!(cli_usage_piped)),
        ("bravebot --resume [id]", t!(cli_usage_resume)),
        ("bravebot --from-pr <number|url>", t!(cli_usage_from_pr)),
        ("bravebot --continue", t!(cli_usage_continue)),
        (
            "bravebot -p \"<task>\" --resume <id>",
            t!(cli_usage_resume_task),
        ),
        (
            "bravebot -p \"<task>\" --continue",
            t!(cli_usage_continue_task),
        ),
        ("bravebot --fork <id>", t!(cli_usage_fork)),
        ("bravebot doctor", t!(cli_usage_doctor)),
        (
            "bravebot doctor --sandbox-check",
            t!(cli_usage_doctor_sandbox),
        ),
        (
            "bravebot doctor --push-identity [<host>/<owner>]",
            t!(cli_usage_doctor_push_identity),
        ),
        ("bravebot bug-report", t!(cli_usage_bug_report)),
        ("bravebot update", t!(cli_usage_update)),
        ("bravebot sessions [--json]", t!(cli_usage_sessions)),
        ("bravebot sessions stop <id>", t!(cli_usage_sessions_stop)),
        (
            "bravebot sessions list [--json]",
            t!(cli_usage_sessions_list),
        ),
        (
            "bravebot sessions import <tool>",
            t!(cli_usage_sessions_import),
        ),
        ("bravebot --bg <prompt>", t!(cli_usage_bg)),
        ("bravebot attach <id>", t!(cli_usage_attach)),
        ("bravebot reply <id> <prompt>", t!(cli_usage_reply)),
        ("bravebot auth login [way]", t!(cli_usage_auth_login)),
        ("bravebot auth logout <way>", t!(cli_usage_auth_logout)),
        ("bravebot auth status [way]", t!(cli_usage_auth_status)),
        ("bravebot import-leo-creds [channel]", t!(cli_usage_import)),
        ("bravebot import-providers", t!(cli_usage_import_providers)),
        ("bravebot mcp <command>", t!(cli_usage_mcp)),
        (
            "bravebot permissions check <call>",
            t!(cli_usage_permissions),
        ),
        (
            "bravebot completion <bash|zsh|fish>",
            t!(cli_usage_completion),
        ),
        (
            "bravebot shell-init <bash|zsh|fish>",
            t!(cli_usage_shell_init),
        ),
    ] {
        println!("  {form:<FORM$}{description}");
    }
    println!();

    println!("{}", t!(cli_keys_heading));
    for (keys, description) in [
        ("Enter", t!(cli_key_send)),
        ("Ctrl-T", t!(cli_key_audit)),
        ("Up/Down", t!(cli_key_history)),
        ("Ctrl-R", t!(cli_key_history_search)),
        ("Wheel, PageUp/Down", t!(cli_key_scroll)),
        ("Home/End", t!(cli_key_jump)),
        ("Esc", t!(cli_key_cancel)),
        ("Ctrl-C", t!(cli_key_leave)),
    ] {
        println!("  {keys:<KEY$}{description}");
    }
    println!();

    // Listed from the commands themselves, so one renamed or added cannot leave this advertising a
    // word that no longer works. The interface offers the same list when a slash is typed.
    println!("{}", t!(cli_commands_heading));
    for command in bravebot_tui::app::commands() {
        let hint = command.typed_hint();
        let word = if hint.is_empty() {
            command.name.to_string()
        } else {
            format!("{} {hint}", command.name)
        };
        println!("  {word:<20}  {}", command.description);
    }
    // Not a command, but typed in the same place and worth finding here.
    println!("  {:<20}  {}", "@<path>", t!(cli_name_a_file));
    println!();

    println!("{}", t!(cli_options_heading));
    for (flags, description) in [
        ("--file <path>", t!(cli_option_file)),
        ("--session-ref <id>", t!(cli_option_session_ref)),
        ("--add-dir <path>", t!(cli_option_add_dir)),
        ("--trust-workspace", t!(cli_option_trust_workspace)),
        ("--settings <path>", t!(cli_option_settings)),
        ("--run-network <open|closed>", t!(cli_option_run_network)),
        ("--sandbox <mode>", t!(cli_option_sandbox)),
        (
            "--sandbox-allow-read <path>",
            t!(cli_option_sandbox_allow_read),
        ),
        (
            "--sandbox-deny-read <path>",
            t!(cli_option_sandbox_deny_read),
        ),
        (
            "--sandbox-allow-write <path>",
            t!(cli_option_sandbox_allow_write),
        ),
        (
            "--sandbox-deny-write <path>",
            t!(cli_option_sandbox_deny_write),
        ),
        ("--log-level <error|info|debug>", t!(cli_option_log_level)),
        ("--agent <name>", t!(cli_option_agent)),
        ("--system-prompt <prompt>", t!(cli_option_system_prompt)),
        (
            "--append-system-prompt <prompt>",
            t!(cli_option_append_system_prompt),
        ),
        ("--mode <mode>", t!(cli_option_mode)),
        ("--model <name>", t!(cli_option_model)),
        ("--advisor <name>", t!(cli_option_advisor)),
        ("--tools <a,b,c>", t!(cli_option_tools)),
        ("--no-shell", t!(cli_option_no_shell)),
        ("--effort <level>", t!(cli_option_effort)),
        ("-p, --print", t!(cli_option_print)),
        ("-i, --interactive <prompt>", t!(cli_option_interactive)),
        ("--trace", t!(cli_option_trace)),
        ("--json", t!(cli_option_json)),
        ("--json-stream", t!(cli_option_json_stream)),
        ("--output-schema <path>", t!(cli_option_output_schema)),
        ("--incognito", t!(cli_option_incognito)),
        ("--safe", t!(cli_option_safe)),
        ("--locked", t!(cli_option_locked)),
        ("--vet", t!(cli_option_vet)),
        (
            "--dangerously-skip-permissions",
            t!(cli_option_dangerously_skip_permissions),
        ),
        ("-h, --help", t!(cli_option_help)),
        ("-V, --version", t!(cli_option_version)),
    ] {
        println!("  {flags:<OPTION$}{description}");
    }
}
