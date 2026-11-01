//! Installing the native messaging host: one manifest where Brave reads it, and the one extension
//! id the host serves.
//!
//! Brave reads Chrome's per-user manifest directory on macOS and its own profile's on Linux. The
//! manifest names this program by its absolute path, so a copy moved after installing is installed
//! again.
//!
//! It also says how to declare the MCP server, one for every tool or one for each group of tools a
//! person names, since a server is granted and approved under its own alias.
//!
//! Details: <https://developer.chrome.com/docs/extensions/develop/concepts/native-messaging>

use crate::paths::{self, EXTENSION};
use crate::relay;
use crate::tools;
use serde_json::json;
use std::ffi::{OsStr, OsString};
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

/// The host's name, which is also the manifest's file name without `.json`.
pub const HOST_NAME: &str = "com.brave.bravebot";

/// The id of the extension in `extension/`, which the `key` in its manifest fixes wherever it is
/// loaded from. `install` records this one where it is given none.
pub const EXTENSION_ID: &str = "fcjamhpiedeihbbbpndcjngkjaogfgep";

/// Where Brave's stable channel reads a per-user host manifest on this platform.
pub fn manifest_directory() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").filter(|home| !home.is_empty())?;
    let config = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from);
    Some(manifest_directory_for(
        cfg!(target_os = "macos"),
        Path::new(&home),
        config.as_deref(),
    ))
}

/// [`manifest_directory`] for a given platform, home and `XDG_CONFIG_HOME`.
///
/// On macOS Brave reads Chrome's directory rather than one of its own. On Linux it reads its own
/// profile's, under the configuration directory, which a relative `XDG_CONFIG_HOME` does not name.
fn manifest_directory_for(macos: bool, home: &Path, config: Option<&Path>) -> PathBuf {
    if macos {
        return home.join("Library/Application Support/Google/Chrome/NativeMessagingHosts");
    }
    let config = config
        .filter(|config| config.is_absolute())
        .map_or_else(|| home.join(".config"), Path::to_path_buf);
    config.join("BraveSoftware/Brave-Browser/NativeMessagingHosts")
}

/// Whether `id` is spelt as an extension id is: 32 letters from `a` to `p`.
pub fn is_extension_id(id: &str) -> bool {
    id.len() == 32 && id.bytes().all(|byte| (b'a'..=b'p').contains(&byte))
}

/// The groups each preset of `--preset` declares a server for, by name. A preset names the groups
/// that have a tool.
pub const PRESETS: &[(&str, &[&str])] = &[
    ("off", &["platform"]),
    ("look", &["tabs-read", "history-read", "bookmarks-read"]),
    (
        "read",
        &["tabs-read", "history-read", "bookmarks-read", "page-read"],
    ),
    (
        "browse",
        &[
            "tabs-read",
            "history-read",
            "bookmarks-read",
            "page-read",
            "tabs-manage",
            "bookmarks-write",
            "navigate",
        ],
    ),
];

/// What the words after `install` ask for.
#[derive(Debug, PartialEq, Eq)]
pub struct Arguments<'a> {
    /// The directory for the manifest, where the platform's own is not wanted.
    pub manifests: Option<&'a str>,
    /// The groups to say how to declare a server for, from `--groups` or `--preset`, each once.
    /// Empty where neither was given, which is one server for every tool.
    pub groups: Vec<&'static str>,
    /// The extension id, where it is not the one in `extension/`.
    pub extension: Option<&'a str>,
}

/// Reads the words after `install`, or `None` for words it does not take, which are answered with
/// the usage and write nothing. A flag takes the word after it, so one at the end has no value and
/// is not taken, and a flag given twice, or both `--groups` and `--preset`, is ambiguous.
pub fn arguments<'a>(words: &[&'a str]) -> Option<Arguments<'a>> {
    let mut asked = Arguments {
        manifests: None,
        groups: Vec::new(),
        extension: None,
    };
    let mut chosen = false;
    let mut rest = words;
    while let [word, tail @ ..] = rest {
        rest = match (*word, tail) {
            ("--manifest-dir", [directory, tail @ ..]) if asked.manifests.is_none() => {
                asked.manifests = Some(*directory);
                tail
            }
            ("--groups", [list, tail @ ..]) if !chosen => {
                asked.groups = groups(list)?;
                chosen = true;
                tail
            }
            ("--preset", [name, tail @ ..]) if !chosen => {
                asked.groups = preset(name)?.to_vec();
                chosen = true;
                tail
            }
            // A word that starts like a flag is one this does not take, or one missing its value.
            (extension, tail) if !extension.starts_with('-') && asked.extension.is_none() => {
                asked.extension = Some(extension);
                tail
            }
            _ => return None,
        };
    }
    Some(asked)
}

/// The groups in a comma-separated list, each once, or `None` if any of them is not a group.
fn groups(list: &str) -> Option<Vec<&'static str>> {
    let mut groups = Vec::new();
    for id in list.split(',') {
        let group = tools::group(id)?;
        if !groups.contains(&group) {
            groups.push(group);
        }
    }
    Some(groups)
}

/// The groups of the preset of this name, if there is one.
fn preset(name: &str) -> Option<&'static [&'static str]> {
    PRESETS
        .iter()
        .find(|(preset, _)| *preset == name)
        .map(|(_, groups)| *groups)
}

/// What installing wrote, and the program the manifest names.
pub struct Installed {
    pub manifest: PathBuf,
    pub directory: PathBuf,
    pub program: PathBuf,
}

/// Records `extension` as the one the host serves and writes the manifest naming `program` into
/// `manifests`. Writes nothing where the id is not an extension id.
pub fn install(
    extension: &str,
    program: &Path,
    manifests: &Path,
    directory: &Path,
) -> io::Result<Installed> {
    if !is_extension_id(extension) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{extension:?} is not an extension id: that is 32 letters from a to p"),
        ));
    }
    let program = program.canonicalize()?;
    let manifest = json!({
        "name": HOST_NAME,
        "description": "BraveBot's relay to the Brave extension",
        "path": program,
        "type": "stdio",
        "allowed_origins": [format!("chrome-extension://{extension}/")],
    });

    paths::prepare(directory)?;
    paths::write_private(directory, EXTENSION, extension.as_bytes())?;

    std::fs::create_dir_all(manifests)?;
    let file = format!("{HOST_NAME}.json");
    let staged = manifests.join(format!(".{file}.{}", std::process::id()));
    let mut contents = serde_json::to_vec_pretty(&manifest).map_err(io::Error::other)?;
    contents.push(b'\n');
    std::fs::write(&staged, contents)?;
    let written = manifests.join(file);
    std::fs::rename(&staged, &written).inspect_err(|_| {
        let _ = std::fs::remove_file(&staged);
    })?;

    Ok(Installed {
        manifest: written,
        directory: directory.to_path_buf(),
        program,
    })
}

/// `word` as one word of a POSIX shell. A word of letters, digits and `_ - . / : , + @` is left as
/// it is. Any other is put in single quotes, with each `'` in it written `'\''`, so a space, a
/// quote, `$`, a backtick or a `~` at the start reaches the command as part of the word. The bytes
/// are copied as they are, since a name that is not UTF-8 is still the name the shell is given.
fn quoted(word: &OsStr) -> OsString {
    let plain = |byte: &u8| byte.is_ascii_alphanumeric() || b"_-./:,+@".contains(byte);
    let bytes = word.as_bytes();
    if !bytes.is_empty() && bytes.iter().all(plain) {
        return word.to_os_string();
    }
    let mut quoted = OsString::from("'");
    for (at, part) in bytes.split(|byte| *byte == b'\'').enumerate() {
        if at > 0 {
            quoted.push("'\\''");
        }
        quoted.push(OsStr::from_bytes(part));
    }
    quoted.push("'");
    quoted
}

/// The arguments of a group's tools that take the number of an element, which `install` declares as
/// taking a choice so that a processor can be asked to nominate the element (SERVERS-22). The
/// relay links no workspace crate, so the pairs are written out here, and a test holds them to the
/// tools whose schema has `element_number`.
const CHOICES: &[(&str, &str, &str)] = &[
    ("input", "click_element", "element_number"),
    ("input", "type_text", "element_number"),
    ("input", "select_option", "element_number"),
];

impl Installed {
    /// The command that declares a server for `group` of the tools, or for every tool where there
    /// is none, to be pasted into a POSIX shell. A group's server has the alias `brave-<group>` and
    /// the others' is `brave`, so each is granted, approved and remembered apart. The declaration
    /// of a group that [`tools::Group::ask_every_call`] is set for also has `--ask-every-call`, and
    /// that of a group that [`tools::Group::asks_in_browser`] is set for also has `--tool-timeout`
    /// with the bound of [`relay::IN_BROWSER_TOOL_TIMEOUT`], which is above the wait of its server.
    /// The declaration of a group with tools in [`CHOICES`] also has a `--choice <tool>:<argument>`
    /// for each, after those. The directory and the program are quoted as [`quoted`] says, and the
    /// line is bytes, since either may be a name that is not UTF-8.
    pub fn declaration(&self, group: Option<&str>) -> OsString {
        let choices: String = CHOICES
            .iter()
            .filter(|(id, ..)| Some(*id) == group)
            .map(|(_, tool, argument)| format!(" --choice {tool}:{argument}"))
            .collect();
        let (alias, asked, bound, group) = match group {
            Some(group) => (
                format!("brave-{group}"),
                tools::asks_every_call(group),
                tools::asks_in_browser(Some(group)),
                format!(" --group {group}"),
            ),
            None => ("brave".to_string(), false, false, String::new()),
        };
        let asked = if asked { " --ask-every-call" } else { "" };
        let bound = if bound {
            format!(
                " --tool-timeout {}",
                relay::IN_BROWSER_TOOL_TIMEOUT.as_secs()
            )
        } else {
            String::new()
        };
        let mut line = OsString::from(format!(
            "bravebot mcp add {alias} -s user{asked}{bound}{choices} --dir "
        ));
        line.push(quoted(self.directory.as_os_str()));
        line.push(" -- ");
        line.push(quoted(self.program.as_os_str()));
        line.push(format!(" mcp{group}"));
        line
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A manifest anywhere else is never read, so the location is the one Brave reads on each
    /// platform: Chrome's on macOS, and Brave's own profile's on Linux.
    #[test]
    fn the_manifest_goes_where_brave_reads_it() {
        let home = Path::new("/home/a-person");
        assert_eq!(
            manifest_directory_for(true, home, None),
            home.join("Library/Application Support/Google/Chrome/NativeMessagingHosts")
        );
        assert_eq!(
            manifest_directory_for(false, home, None),
            home.join(".config/BraveSoftware/Brave-Browser/NativeMessagingHosts")
        );
        assert_eq!(
            manifest_directory_for(false, home, Some(Path::new("/xdg"))),
            Path::new("/xdg/BraveSoftware/Brave-Browser/NativeMessagingHosts")
        );
        assert_eq!(
            manifest_directory_for(false, home, Some(Path::new("relative"))),
            home.join(".config/BraveSoftware/Brave-Browser/NativeMessagingHosts")
        );
    }

    /// Brave's ids are 32 letters from a to p, and nothing else is one.
    #[test]
    fn an_extension_id_is_32_letters_from_a_to_p() {
        assert!(is_extension_id("abcdefghijklmnopabcdefghijklmnop"));
        assert!(!is_extension_id("abcdefghijklmnopabcdefghijklmno"));
        assert!(!is_extension_id("abcdefghijklmnopabcdefghijklmnoq"));
        assert!(!is_extension_id("ABCDEFGHIJKLMNOPABCDEFGHIJKLMNOP"));
        assert!(!is_extension_id("abcdefghijklmnop/../../../../../"));
    }

    fn asked<'a>(words: &[&'a str]) -> Arguments<'a> {
        arguments(words).unwrap_or_else(|| panic!("{words:?} were not taken"))
    }

    /// With no flag the words are a directory for the manifest and an extension id, each optional,
    /// and no group, which is the one server for every tool.
    #[test]
    fn install_with_no_group_asks_for_the_one_server_of_every_tool() {
        let none = Arguments {
            manifests: None,
            groups: Vec::new(),
            extension: None,
        };
        assert_eq!(asked(&[]), none);
        assert_eq!(
            asked(&["--manifest-dir", "/m", "abcdefghijklmnopabcdefghijklmnop"]),
            Arguments {
                manifests: Some("/m"),
                extension: Some("abcdefghijklmnopabcdefghijklmnop"),
                ..none
            }
        );
    }

    /// `--groups` names groups in one comma-separated word, the flags may come in any order, and a
    /// group named twice is declared once, in the order it was first named.
    #[test]
    fn groups_are_read_from_one_word_in_any_order_and_each_is_kept_once() {
        let expected = Arguments {
            manifests: Some("/m"),
            groups: vec!["history-read", "tabs-read"],
            extension: None,
        };
        assert_eq!(
            asked(&[
                "--groups",
                "history-read,tabs-read,history-read",
                "--manifest-dir",
                "/m"
            ]),
            expected
        );
        assert_eq!(
            asked(&["--manifest-dir", "/m", "--groups", "history-read,tabs-read"]),
            expected
        );
        assert_eq!(asked(&["--groups", "remove-data"]).groups, ["remove-data"]);
    }

    /// A word `install` does not take is answered with the usage and writes nothing, so a group
    /// that is misspelt, a flag with no value and two flags that disagree each declare nothing
    /// where a person meant something.
    #[test]
    fn install_refuses_words_it_does_not_take() {
        let id = "abcdefghijklmnopabcdefghijklmnop";
        for words in [
            &["--groups"][..],
            &["--groups", "nope"],
            &["--groups", "tabs-read,nope"],
            &["--groups", "tabs-read,"],
            &["--groups", ","],
            &["--groups", ""],
            &["--groups", "Tabs-Read"],
            &["--groups", "--preset", "look"],
            &["--preset"],
            &["--preset", "nope"],
            &["--preset", "Look"],
            &["--preset", "look,read"],
            &["--groups", "tabs-read", "--preset", "look"],
            &["--preset", "look", "--groups", "tabs-read"],
            &["--groups", "tabs-read", "--groups", "history-read"],
            &["--preset", "look", "--preset", "read"],
            &["--manifest-dir"],
            &["--manifest-dir", "/a", "--manifest-dir", "/b"],
            &["--group", "tabs-read"],
            &["--unknown"],
            &[id, id],
            &["--groups", "tabs-read", "-"],
        ] {
            assert_eq!(arguments(words), None, "{words:?}");
        }
    }

    /// A preset names its groups exactly, so the lines printed for it are the ones its name
    /// promises, and each group it names is one a server can be started for and has a tool.
    #[test]
    fn a_preset_names_the_groups_that_have_a_tool() {
        let groups = |name| asked(&["--preset", name]).groups;
        assert_eq!(groups("off"), ["platform"]);
        assert_eq!(
            groups("look"),
            ["tabs-read", "history-read", "bookmarks-read"]
        );
        assert_eq!(
            groups("read"),
            ["tabs-read", "history-read", "bookmarks-read", "page-read"]
        );
        assert_eq!(
            groups("browse"),
            [
                "tabs-read",
                "history-read",
                "bookmarks-read",
                "page-read",
                "tabs-manage",
                "bookmarks-write",
                "navigate"
            ]
        );
        let names: Vec<&str> = PRESETS.iter().map(|(name, _)| *name).collect();
        assert_eq!(names, ["off", "look", "read", "browse"]);
        for (name, groups) in PRESETS {
            for group in *groups {
                assert!(tools::group(group).is_some(), "{name}: {group}");
                assert!(
                    tools::offered(Some(group)).next().is_some(),
                    "{name}: {group} has no tool"
                );
            }
        }
    }

    /// A group's server has an alias of its own and is started for its group, and the server for
    /// every tool is declared as `brave` with no group word.
    #[test]
    fn a_declaration_names_the_group_in_the_alias_and_in_the_program_words() {
        let installed = Installed {
            manifest: PathBuf::from("/m/com.brave.bravebot.json"),
            directory: PathBuf::from("/d"),
            program: PathBuf::from("/p/bravebot-browser"),
        };
        assert_eq!(
            installed.declaration(None),
            "bravebot mcp add brave -s user --dir /d -- /p/bravebot-browser mcp"
        );
        assert_eq!(
            installed.declaration(Some("tabs-read")),
            "bravebot mcp add brave-tabs-read -s user --dir /d -- /p/bravebot-browser mcp \
             --group tabs-read"
        );
    }

    /// A name made of the characters a path usually has is printed as it is, and any other is one
    /// word in single quotes, so what a shell reads is the name and nothing it would act on: a
    /// space, a quote, `$`, a backtick, a `~` at the start, a newline. The empty name is a word
    /// too. A name that is not UTF-8 is copied byte for byte, and differs from the name its
    /// lossy rendering spells.
    #[test]
    fn a_name_is_one_shell_word_in_single_quotes_unless_it_is_made_of_plain_characters() {
        let word = |name: &[u8]| quoted(OsStr::from_bytes(name));
        for (name, written) in [
            (
                &b"/Users/me/.bravebot-browser"[..],
                &b"/Users/me/.bravebot-browser"[..],
            ),
            (b"/opt/a_b-c.d:e,f+g@h", b"/opt/a_b-c.d:e,f+g@h"),
            (b"-dir", b"-dir"),
            (b"", b"''"),
            (b"/with space/x", b"'/with space/x'"),
            (b"/it's/x", br"'/it'\''s/x'"),
            (b"'", br"''\'''"),
            (b"''", br"''\'''\'''"),
            (b"/a$b/$(c)", b"'/a$b/$(c)'"),
            (b"/`c`", b"'/`c`'"),
            (b"/a;b&c|d>e<f", b"'/a;b&c|d>e<f'"),
            (br#"/a"b\c"#, br#"'/a"b\c'"#),
            (b"~/x", b"'~/x'"),
            (b"=x", b"'=x'"),
            (b"#x", b"'#x'"),
            (b"/a*?[b]", b"'/a*?[b]'"),
            (b"/a\nb", b"'/a\nb'"),
            (b"/d\xff/x", b"'/d\xff/x'"),
            (b"/d\xef\xbf\xbd/x", b"'/d\xef\xbf\xbd/x'"),
        ] {
            assert_eq!(
                word(name).as_bytes(),
                written,
                "{:?}",
                String::from_utf8_lossy(name)
            );
        }
        assert_ne!(word(b"/d\xff/x"), word("/d\u{FFFD}/x".as_bytes()));
    }

    /// The directory and the program are the two words of a declaration that are paths, and each
    /// is quoted where it needs to be, with the rest of the line as it was.
    #[test]
    fn a_declaration_quotes_the_directory_and_the_program() {
        let installed = Installed {
            manifest: PathBuf::from("/m/com.brave.bravebot.json"),
            directory: PathBuf::from("/my dir/it's"),
            program: PathBuf::from("/Users/Jane Doe/$HOME/bravebot-browser"),
        };
        assert_eq!(
            installed.declaration(Some("cookies")),
            r"bravebot mcp add brave-cookies -s user --ask-every-call --dir '/my dir/it'\''s' -- '/Users/Jane Doe/$HOME/bravebot-browser' mcp --group cookies"
        );
        assert_eq!(
            installed.declaration(None),
            r"bravebot mcp add brave -s user --dir '/my dir/it'\''s' -- '/Users/Jane Doe/$HOME/bravebot-browser' mcp"
        );
        let raw = Installed {
            directory: PathBuf::from(OsStr::from_bytes(b"/d\xff")),
            ..installed
        };
        let line = raw.declaration(None);
        assert!(
            line.as_bytes().windows(5).any(|bytes| bytes == b"'/d\xff'"),
            "{line:?}"
        );
    }

    /// A group whose tools act on data a standing allowance should not cover is declared to ask
    /// about every call, with the flag before the `--` that starts the program, where `mcp add`
    /// reads its own flags.
    #[test]
    fn the_declaration_of_a_group_asked_every_call_says_so_before_the_program() {
        let installed = Installed {
            manifest: PathBuf::from("/m/com.brave.bravebot.json"),
            directory: PathBuf::from("/d"),
            program: PathBuf::from("/p/bravebot-browser"),
        };
        assert_eq!(
            installed.declaration(Some("remove-data")),
            "bravebot mcp add brave-remove-data -s user --ask-every-call --dir /d -- \
             /p/bravebot-browser mcp --group remove-data"
        );
        assert_eq!(
            installed.declaration(Some("cookies")),
            "bravebot mcp add brave-cookies -s user --ask-every-call --dir /d -- \
             /p/bravebot-browser mcp --group cookies"
        );
    }

    /// The group whose actions the person answers in a window of the browser is declared to ask
    /// about every call and with a bound on a call above the wait of its server, both before the
    /// `--`. Without the bound a call is held to the agent's default, which a change to it would
    /// move under the 90 seconds the server waits, and the agent would stop the server while it
    /// still waited for the person.
    #[test]
    fn the_declarations_of_scripts_the_input_groups_capture_trusted_console_trusted_upload_trusted_and_screen_ask_about_every_call_and_bound_a_call_above_the_servers_wait()
     {
        let installed = Installed {
            manifest: PathBuf::from("/m/com.brave.bravebot.json"),
            directory: PathBuf::from("/d"),
            program: PathBuf::from("/p/bravebot-browser"),
        };
        for group in [
            "scripts",
            "input-trusted",
            "capture-trusted",
            "console-trusted",
            "upload-trusted",
            "screen",
        ] {
            assert_eq!(
                installed.declaration(Some(group)),
                format!(
                    "bravebot mcp add brave-{group} -s user --ask-every-call --tool-timeout 120 \
                     --dir /d -- /p/bravebot-browser mcp --group {group}"
                )
                .as_str()
            );
        }
        for other in tools::ids().filter(|id| {
            ![
                "scripts",
                "input",
                "input-trusted",
                "capture-trusted",
                "console-trusted",
                "upload-trusted",
                "screen",
            ]
            .contains(id)
        }) {
            assert!(
                !installed
                    .declaration(Some(other))
                    .to_string_lossy()
                    .contains("--tool-timeout"),
                "{other}"
            );
        }
        assert!(
            !installed
                .declaration(None)
                .to_string_lossy()
                .contains("--tool-timeout")
        );
    }

    /// The `input` group is the one whose tools take the number of an element, so its declaration
    /// names that argument of each of the three as taking a choice (SERVERS-22), after the flags
    /// that say every call is asked about and before the `--`. Every other declaration names none,
    /// `input-trusted` included, since its tools take a role and a name and no number. The
    /// regressions it rejects are a declaration with no pairs, one with a pair missing, and one
    /// that gives the pairs to a group that has no such argument.
    #[test]
    fn the_declaration_of_the_input_group_declares_the_element_number_of_its_three_tools_as_taking_a_choice()
     {
        let installed = Installed {
            manifest: PathBuf::from("/m/com.brave.bravebot.json"),
            directory: PathBuf::from("/d"),
            program: PathBuf::from("/p/bravebot-browser"),
        };
        assert_eq!(
            installed.declaration(Some("input")),
            "bravebot mcp add brave-input -s user --ask-every-call --tool-timeout 120 \
             --choice click_element:element_number --choice type_text:element_number \
             --choice select_option:element_number --dir /d -- /p/bravebot-browser mcp --group input"
        );
        for other in tools::ids().filter(|id| *id != "input") {
            assert!(
                !installed
                    .declaration(Some(other))
                    .to_string_lossy()
                    .contains("--choice"),
                "{other}"
            );
        }
        assert!(
            !installed
                .declaration(None)
                .to_string_lossy()
                .contains("--choice")
        );
    }

    /// The pairs `install` declares are the arguments the relay's schemas call `element_number`,
    /// in the group that holds the tool, and a group that declares a choice is asked about every
    /// call. The regressions it rejects are a tool that takes an element number and is left out
    /// of the declaration, and a pair that names a tool or a group the table does not have.
    #[test]
    fn the_declared_choices_are_the_tools_whose_schema_takes_an_element_number() {
        let mut taking: Vec<(&str, &str, &str)> = tools::TOOLS
            .iter()
            .filter(|tool| {
                tool.listed()["inputSchema"]["properties"]
                    .get("element_number")
                    .is_some()
            })
            .map(|tool| (tool.group, tool.name, "element_number"))
            .collect();
        let mut declared = CHOICES.to_vec();
        taking.sort();
        declared.sort();
        assert_eq!(declared, taking);
        for (group, ..) in CHOICES {
            assert!(tools::asks_every_call(group), "{group}");
        }
    }

    /// ROUTE-11: no declaration names an argument of `run_script` as taking a choice, so the one
    /// number a processor's answer can supply never reaches the code or the address of a script.
    /// Every pair in the table is the `element_number` of a tool of a group that is not `scripts`,
    /// and the declaration of `scripts` has no `--choice`. The regression it rejects is a row such
    /// as `("scripts", "run_script", "code")` or `("scripts", "run_script", "url")`, which
    /// would let a page steer a processor's number into the text the person approves as a whole.
    #[test]
    fn the_declared_choices_name_no_tool_of_scripts_and_no_argument_that_holds_code_or_an_address()
    {
        for (group, tool, argument) in CHOICES {
            assert_ne!(*group, "scripts", "{tool}");
            assert_ne!(*tool, "run_script", "{group}");
            assert_eq!(*argument, "element_number", "{group}:{tool}");
        }
        let installed = Installed {
            manifest: PathBuf::from("/m/com.brave.bravebot.json"),
            directory: PathBuf::from("/d"),
            program: PathBuf::from("/p/bravebot-browser"),
        };
        assert!(
            !installed
                .declaration(Some("scripts"))
                .to_string_lossy()
                .contains("--choice")
        );
    }

    /// A preset is a set of groups a person turns on together, and a group that acts on a page,
    /// deletes data or reads cookies is a choice of its own (BROWSER-20), so no preset prints the
    /// declaration of a group that is asked about every call, `input` among them.
    #[test]
    fn no_preset_names_a_group_that_is_asked_about_every_call() {
        for (name, groups) in PRESETS {
            for group in *groups {
                assert!(!tools::asks_every_call(group), "{name}: {group}");
                assert_ne!(*group, "scripts", "{name}");
                assert_ne!(*group, "input", "{name}");
                assert_ne!(*group, "input-trusted", "{name}");
                assert_ne!(*group, "capture-trusted", "{name}");
                assert_ne!(*group, "console-trusted", "{name}");
                assert_ne!(*group, "upload-trusted", "{name}");
                assert_ne!(*group, "screen", "{name}");
            }
        }
        assert!(tools::asks_every_call("scripts"));
        assert!(tools::asks_every_call("input"));
        assert!(tools::asks_every_call("input-trusted"));
        assert!(tools::asks_every_call("capture-trusted"));
        assert!(tools::asks_every_call("console-trusted"));
        assert!(tools::asks_every_call("upload-trusted"));
        assert!(tools::asks_every_call("screen"));
    }
}
