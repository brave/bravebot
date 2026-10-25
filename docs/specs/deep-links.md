---
id: LINK
title: Deep links
status: normative
governs:
  - crates/cli/src/deep_link.rs
  - crates/cli/src/main.rs
  - crates/tui/src/app.rs
  - crates/tui/src/state.rs
  - crates/tui/src/state/editing.rs
  - crates/tui/src/render/hint.rs
  - crates/config/src/settings.rs
  - install.sh
  - crates/tui/src/app/directories.rs
  - crates/tui/src/app/opening.rs
  - crates/tui/src/app/records.rs
  - crates/tui/src/app/side_requests.rs
  - crates/tui/src/app/turns.rs
documented-by:
  - docs/website/docs/reference/cli.md
  - docs/website/docs/customize/configuration.md
---

## Scope

A `bravebot://open` link opens a session with some words already in the box and a directory to
start in. The operating system hands the link to `bravebot --handle-uri`, which the installer
registers for the scheme. Any page can offer a link, so what a link may ask for is the whole of this
spec: a directory it may name and words it may put in the box, and nothing that decides how the
session runs.

What the words become once they are in the box is not new. They enter as a paste does
([pasting.md](pasting.md)), and Enter sends them as it sends a paste, so the questions a session
opens with ([trust-map.md](trust-map.md)), the permission rules ([permissions.md](permissions.md)) and
the checkout's instructions ([instructions.md](instructions.md)) apply as in any session.

## Clauses

<a id="LINK-1"></a>
### LINK-1: a link is `bravebot://open` with a query, read strictly

`bravebot --handle-uri <link>` takes one argument, which is the link, and nothing else: no second
word, and none of the flags that compose with other ways of starting a session
([CLI-31](cli.md#CLI-31)), `--dangerously-skip-permissions` and `--agent` among them. The scheme and
the host are matched without regard to case, the host may be followed by one `/`, and what follows
is either nothing or `?` and a query. Any other shape, a fragment included, is not a link. Percent
escapes are resolved and `+` is a space. A link whose escape is not two hex digits, whose bytes are
not UTF-8 or that holds a NUL is refused, as is one longer than 8192 bytes. A refusal is the status
for an argument ([CLI-6](cli.md#CLI-6)), is said before the terminal is taken, starts no session and
does not repeat the link, since its text is the sender's and can hold anything a terminal draws.

**Why.** The link arrives from outside the machine. A reading that is lenient about shape is a reading
that has to decide what a malformed link meant, and the only safe decision is to start nothing.

`verified-by: bravebot_cli::deep_link::a_link_gives_its_directory_and_its_prompt_decoded`
`verified-by: bravebot_cli::deep_link::only_bravebot_open_is_a_link`
`verified-by: bravebot_cli::deep_link::a_link_that_does_not_decode_to_text_is_refused`
`verified-by: bravebot_cli::running::a_link_that_asks_for_more_or_is_switched_off_starts_nothing`

<a id="LINK-2"></a>
### LINK-2: a link carries a `dir` and a `prompt`, and no parameter that decides how the session runs

Only `dir` and `prompt` are read, each at most once. A link naming any other parameter, a mode, a
tool, a model or `dangerously-skip-permissions` among them, or naming one twice, is refused whole
([LINK-1](#LINK-1)) rather than opened with the part it recognised, since a session that quietly
dropped the part that mattered is not the one the link asked for, and a link has no business asking.

**Why.** A permission mode or a tool is a decision the person makes at the keyboard
([permission-modes.md](permission-modes.md)). If a link could carry one, every page on the web would
hold a way to make that decision for them. Refusing, rather than ignoring, means the day somebody adds
such a parameter on the sender's side is the day it fails visibly.

`verified-by: bravebot_cli::deep_link::a_link_asking_for_anything_else_is_refused_whole`
`verified-by: bravebot_cli::running::a_link_that_asks_for_more_or_is_switched_off_starts_nothing`

<a id="LINK-3"></a>
### LINK-3: a link starts in the directory it names only where a person already vouched for it

`dir` must be an absolute path. The session starts there only when it is a directory that a kept
answer to the startup question covers ([TRUST-23](trust-map.md#TRUST-23)), that is the answer a
session started there by hand would have settled on without asking. Otherwise the session starts in
the person's home directory, and its first line says that the link named a directory that is not
trusted here, quoting the path escaped so a control character in it is not written to the terminal. A
link naming no directory starts in the home directory, and says nothing. The startup question is asked
of the directory the session did start in, as it is for any session, so a link can neither grant trust
nor skip the question.

**Why.** Starting in a directory is what makes every file there the session's to read and its settings
and instructions the session's to load ([instructions.md](instructions.md)). A link that could choose
any directory would choose where somebody else's files are read from, one a person never opened.

`verified-by: bravebot_cli::deep_link::a_directory_nobody_vouched_for_starts_in_home_and_says_so`
`verified-by: bravebot_cli::deep_link::a_relative_directory_is_refused`
`verified-by: bravebot_cli::deep_link::the_note_does_not_carry_a_control_character_raw`

<a id="LINK-4"></a>
### LINK-4: the words go in the box and are not sent, and the line under the box says where they came from

Once the questions the session opens with are answered, the `prompt` is put in the box as a paste is
put there: what a terminal draws as nothing is removed and counted, and the count is said even where
nothing else is left ([PASTE-10](pasting.md#PASTE-10)). Unlike a paste a long one is not folded behind
a marker ([PASTE-1](pasting.md#PASTE-1)), since a marker is a label for words the box then does not
show and these are the words a page chose. Nothing sends it, and nothing queues it: it waits for
Enter. While the box holds anything, the line under it says that the words
came from an external link and are not sent. Over 1000 characters it says how many there are and asks
for a scroll and a review, since that is more than the box shows. The line goes when the words are
sent or the box is emptied, and does not come back over what is typed afterwards. A `prompt` that is
only whitespace puts nothing in the box and draws no line.

**Why.** The person has to see the words before the planner does, and has to be told they are not
theirs. A session that sent them would act on a page's request before anybody read it. A long prompt
is where a request hides: the box shows the first rows and the person presses Enter on the rest.

`verified-by: bravebot_tui::app::the_words_a_link_carries_are_in_the_box_and_are_not_sent`
`verified-by: bravebot_tui::app::the_banner_goes_when_the_words_are_sent_or_cleared_and_stays_gone`
`verified-by: bravebot_tui::app::a_link_with_no_words_leaves_the_box_empty`
`verified-by: bravebot_tui::app::a_long_link_prompt_is_written_whole_and_not_folded`
`verified-by: bravebot_tui::render::the_hint_line_names_the_link_and_gives_the_length_of_a_long_prompt`

<a id="LINK-5"></a>
### LINK-5: sent, the words are the person's, on the footing of a paste

Enter on the words is Enter on a paste ([PASTE-6](pasting.md#PASTE-6)): the transcript, the history and
the record hold them as the person's own message, and the planner reads them as the person's request.
They are not labelled untrusted and not carried as quarantined content. The banner of
[LINK-4](#LINK-4) and the person's reading are what stand between a link and the planner.

**Why.** The rule of this repository is that the planner never has untrusted content in its context.
A message the person typed or pasted and then chose to send is not untrusted content, it is their
words, and a paste has always been that ([pasting.md](pasting.md)). Quarantining the link's words
would leave the planner unable to act on them, so a link would open a session whose request nobody
could carry out. What the design can do is refuse to send before the person has had the chance to read.

**Known cost.** A page can put a request in front of a person, and a person who presses Enter
without reading it has sent it. That is the cost of every paste, and of `-i` for the script that
wrote the prompt ([CLI-31](cli.md#CLI-31)). The two things the link does not do are what bound it:
it does not send, and it cannot raise what the session may do, since [LINK-2](#LINK-2) refuses a
parameter that would.

`verified-by: bravebot_tui::app::the_banner_goes_when_the_words_are_sent_or_cleared_and_stays_gone`

<a id="LINK-6"></a>
### LINK-6: the installer registers the handler on Linux, and a setting stops links being opened

On Linux, `install.sh` writes `bravebot-handler.desktop` into `$XDG_DATA_HOME/applications` (or
`~/.local/share/applications`), with `Terminal=true`, an `Exec` line running the installed binary
with `--handle-uri %u`, and `MimeType=x-scheme-handler/bravebot;`, then points the scheme at it
through `xdg-mime` where that is installed. Nothing is registered where `BRAVEBOT_NO_DEEP_LINKS` is
set, where there is no data directory to write in, or where the binary's path holds a character an
`Exec` line would read as something else. A registration that fails does not fail the install. The
`deepLinks` setting, when it is the boolean `false`, makes `--handle-uri` refuse every link, with the
status for a configuration problem ([CLI-6](cli.md#CLI-6)), before a link is read; a quoted `"false"`
leaves links on. An administrator who wants the registration gone removes the entry.

macOS and Windows are not built: the first needs an application bundle that declares the scheme, and
the second a registry entry written by the installer package. Until they are, a link does nothing
there.

**Why.** Registration is how an operating system finds the program, and it is the person's to
withhold. The setting is the one thing a person can say once and keep, for a machine whose
registration they did not choose.

**Known cost.** The environment variable stops the installer registering and the setting stops
links being honoured; neither reads the other, because a shell script cannot read the layers a
settings file is merged from. A person who sets only the setting keeps an entry that refuses.

`verified-by: bravebot_config::settings::only_a_boolean_turns_deep_links_off`
`verified-by: bravebot_cli::running::a_link_that_asks_for_more_or_is_switched_off_starts_nothing`
`verified-by: bravebot_cli::installer_deep_link::the_entry_runs_the_binary_in_a_terminal_with_the_link`
`verified-by: bravebot_cli::installer_deep_link::nothing_is_registered_where_it_is_switched_off_or_the_path_is_unsafe`
