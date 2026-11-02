---
id: BROWSER
title: Reaching the browser through an extension
status: normative
governs:
  - crates/browser/Cargo.toml
  - crates/browser/src/main.rs
  - crates/browser/src/lib.rs
  - crates/browser/src/framing.rs
  - crates/browser/src/host.rs
  - crates/browser/src/install.rs
  - crates/browser/src/lines.rs
  - crates/browser/src/paths.rs
  - crates/browser/src/profile.rs
  - crates/browser/src/relay.rs
  - crates/browser/src/server.rs
  - crates/browser/src/tools.rs
  - crates/browser/src/uploads.rs
  - crates/browser/tests/clean_profile.rs
  - crates/browser/tests/extension.rs
  - crates/browser/tests/file-names.json
  - crates/browser/tests/relay.rs
  - crates/browser/tests/tool_surface.rs
  - crates/agent/tests/browser_group_servers.rs
  - crates/config/tests/browser_group_aliases.rs
  - crates/core/tests/browser_group_aliases.rs
  - crates/core/tests/draws-as-nothing.json
  - extension/README.md
  - extension/manifest.json
  - extension/actions.js
  - extension/activity.js
  - extension/approval.js
  - extension/awake.js
  - extension/background.js
  - extension/badge.js
  - extension/capture.html
  - extension/capture.js
  - extension/confirm.css
  - extension/confirm.html
  - extension/confirm.js
  - extension/confirmation.js
  - extension/errors.js
  - extension/files.js
  - extension/gif.js
  - extension/groups.js
  - extension/held.js
  - extension/input.js
  - extension/lifetimes.js
  - extension/numbering.js
  - extension/pictures.js
  - extension/requests.js
  - extension/screen.js
  - extension/scripts.js
  - extension/shared-files.json
  - extension/shown.js
  - extension/switches.js
  - extension/tools.js
  - extension/trusted.js
  - extension/consoles.js
  - extension/warnings.js
  - extension/words.js
  - extension/options.js
  - extension/options.html
  - extension/popup.html
  - extension/popup.js
  - extension/tests/activity.test.mjs
  - extension/tests/capture-trusted.test.mjs
  - extension/tests/awake.test.mjs
  - extension/tests/background.test.mjs
  - extension/tests/confirmation.test.mjs
  - extension/tests/fake-chrome.mjs
  - extension/tests/fake-menus.mjs
  - extension/tests/fake-permissions.mjs
  - extension/tests/fake-storage.mjs
  - extension/tests/fake-dom.mjs
  - extension/tests/fake-window.mjs
  - extension/tests/gif-reader.mjs
  - extension/tests/gif.test.mjs
  - extension/tests/groups.test.mjs
  - extension/tests/held.test.mjs
  - extension/tests/input.test.mjs
  - extension/tests/lifetimes.test.mjs
  - extension/tests/namespaces.test.mjs
  - extension/tests/numbering.test.mjs
  - extension/tests/options.test.mjs
  - extension/tests/popup.test.mjs
  - extension/tests/screen.test.mjs
  - extension/tests/scripts.test.mjs
  - extension/tests/shared.test.mjs
  - extension/tests/switches.test.mjs
  - extension/tests/tools.test.mjs
  - extension/tests/trusted.test.mjs
  - extension/tests/unseen.test.mjs
  - extension/tests/upload-trusted.test.mjs
  - extension/tests/console-trusted.test.mjs
  - extension/tests/upgrade.test.mjs
  - extension/tests/warnings.test.mjs
  - extension-trusted/README.md
  - extension-trusted/manifest.json
  - extension-trusted/popup.html
  - extension-trusted/popup.css
  - extension-trusted/popup.js
  - extension-trusted/background.js
  - extension-trusted/pair.js
  - extension-trusted/requests.js
  - extension-trusted/sentences.js
  - extension-trusted/words.js
  - extension-trusted/capture.js
  - extension-trusted/cdp.js
  - extension-trusted/consoles.js
  - extension-trusted/resolve.js
  - extension-trusted/tools.js
  - extension-trusted/actions.js
  - extension-trusted/approval.js
  - extension-trusted/confirm.css
  - extension-trusted/confirm.html
  - extension-trusted/confirm.js
  - extension-trusted/confirmation.js
  - extension-trusted/dialog.css
  - extension-trusted/dialog.html
  - extension-trusted/dialog.js
  - extension-trusted/errors.js
  - extension-trusted/files.js
  - extension-trusted/shown.js
  - extension-trusted/windows.js
  - extension-trusted/tests/fake-debugger.mjs
  - extension-trusted/tests/fake-panels.mjs
  - extension-trusted/tests/fake-window.mjs
  - extension-trusted/tests/actions.test.mjs
  - extension-trusted/tests/call.test.mjs
  - extension-trusted/tests/capture.test.mjs
  - extension-trusted/tests/confirmation.test.mjs
  - extension-trusted/tests/dialog.test.mjs
  - extension-trusted/tests/windows.test.mjs
  - extension-trusted/tests/scripts.mjs
  - extension-trusted/tests/manifest.test.mjs
  - extension-trusted/tests/background.test.mjs
  - extension-trusted/tests/cdp.test.mjs
  - extension-trusted/tests/consoles.test.mjs
  - extension-trusted/tests/requests.test.mjs
  - extension-trusted/tests/resolve.test.mjs
  - extension-trusted/tests/tools.test.mjs
  - extension-trusted/tests/world.mjs
  - extension-trusted/tests/recorded/hidden-inputs.html
  - extension-trusted/tests/recorded/hidden-world.json
  - extension-trusted/tests/recorded/page.html
  - extension-trusted/tests/recorded/record.js
  - extension-trusted/tests/recorded/world.json
  - docs/design/browser-extension.md
documented-by: docs/website/docs/customize/mcp/brave.md
---

## Scope

How BraveBot reaches a Brave extension that reads pages and profile data for it: the processes
between the two, who starts each, how they find and trust each other, and what is sent between
them. The design and its reasons are [browser-extension.md](../design/browser-extension.md). What
an MCP server may do once it runs is [mcp.md](mcp.md) and [mcp-servers.md](mcp-servers.md), and
nothing here changes either: what the extension returns reaches a session as any server's result
does.

## What exists today

The relay and the two extensions are built, on macOS and Linux. The BraveBot extension is loaded
unpacked from `extension/` and BraveBot Trusted Input, which holds the debugger, from
`extension-trusted/`. `extension/README.md` and `extension-trusted/README.md` say how to install them.
On Windows `bravebot-browser` only prints that the platform is not supported and exits, so no server
starts and neither half runs.

## The processes

```
BraveBot ──MCP──▶ bravebot-browser mcp ──socket──▶ bravebot-browser (native host) ◀──▶ extension
                  (started by BraveBot,             (started by Brave,             native
                   confined)                         not confined)                 messaging
```

One program, `bravebot-browser`, runs in both roles. BraveBot starts it as a stdio MCP server,
declared with `bravebot mcp add` like any other. Brave starts it as a native messaging host when the
extension connects, with the extension's origin as its first argument. The two copies talk over a
Unix socket, one line of JSON at a time, and the first line a peer sends is the secret.

## What the confinement allows

A stdio server runs confined. How that confinement treats a Unix socket decides which side listens
and where the socket is. Measured against the profile each platform's confinement writes for a
stdio server with egress and a `--dir`:

| From inside the confinement | macOS (Seatbelt) | Linux (Landlock ABI 8) |
|---|---|---|
| connect, socket in `--dir` | allowed | allowed |
| connect, socket outside every grant | allowed | allowed |
| connect, egress withheld | refused | not measured |
| bind, in `--dir` | refused | allowed |
| bind, outside every grant | refused | refused |

On macOS a connect to a Unix socket is a network operation and a bind is refused outright. On Linux
the right that governs connecting to a pathname socket is Landlock ABI 9, which the confinement
handles where the kernel carries it, so on such a kernel a connect outside the write grants is
refused. That row was not measured, because the kernel tried carries ABI 8.

## Clauses

<a id="BROWSER-1"></a>
### BROWSER-1: the native host listens and the MCP server connects

The copy Brave starts creates the socket and accepts on it. The copy BraveBot starts connects to it
and never creates one, or anything else, in its directory.

**Why.** A confined server cannot bind a Unix socket on macOS, in any directory. Connecting out is
allowed on both platforms.

`verified-by: bravebot_browser::relay::a_tool_call_reaches_the_extension_through_the_socket_the_host_listens_on`
`verified-by: bravebot_browser::relay::the_mcp_server_never_creates_the_socket`

<a id="BROWSER-2"></a>
### BROWSER-2: the socket is in the directory the server's declaration names

The socket and the secret are in one directory, `~/.bravebot-browser` unless
`BRAVEBOT_BROWSER_DIR` names another, with no access for anyone but the account. The host makes the
directory, or narrows an existing one to that, and refuses a link in its place. The MCP server is
declared with that directory as its `--dir`, and finds the socket in the directory it starts in, so
neither half is told a path:

```
bravebot mcp add brave -s user --dir ~/.bravebot-browser -- <path to>/bravebot-browser mcp
```

**Why.** On a Linux kernel carrying Landlock ABI 9, a confined process connects only to sockets
under its write grants, and `--dir` is the one write grant a declaration adds. A socket anywhere
else works on today's kernels and stops working on a newer one. The host is started by Brave with no
argument of ours, and the server is confined with a home of its own, so the account's home and the
starting directory are the two things each can find without being told.

`verified-by: bravebot_browser::relay::the_socket_and_secret_are_in_a_directory_only_this_account_can_reach`
`verified-by: bravebot_browser::paths::the_host_finds_the_directory_under_home_unless_one_is_named`
`verified-by: bravebot_browser::paths::the_directory_is_made_or_narrowed_to_this_account_alone`
`verified-by: bravebot_browser::paths::a_link_in_place_of_the_directory_is_refused`
`verified-by: bravebot_browser::paths::a_private_file_is_readable_by_this_account_alone`

<a id="BROWSER-3"></a>
### BROWSER-3: the native host serves only a peer holding the secret in that directory

The native host writes a new random secret into the socket's directory as it starts, readable by
the account alone. A connection whose first line is not that secret is closed before anything it
sent is forwarded. A connection that has not presented it within 2 seconds of being accepted is
closed, however it spends them, and at most 32 connections wait to present it at once: one past that
is closed as soon as it is accepted.

**Why.** The confinement does not keep other processes off the socket. Every stdio server BraveBot
starts has egress, and on macOS egress is what reaches a Unix socket, so any of them can connect.
Only a process granted the directory can read the secret, and only the declaration naming it is
granted it. Any of them can also open connections and never present the secret, and a host that
waited on each without end would give every one a thread until it had none left for a session.

`verified-by: bravebot_browser::relay::a_peer_without_the_secret_is_closed_and_nothing_it_sent_is_forwarded`
`verified-by: bravebot_browser::relay::a_connection_that_does_not_present_the_secret_in_time_is_closed`
`verified-by: bravebot_browser::relay::a_connection_trickling_bytes_without_the_secret_is_closed_in_time`
`verified-by: bravebot_browser::relay::connections_waiting_for_the_secret_are_held_to_a_number`
`verified-by: bravebot_browser::host::a_connection_has_two_seconds_and_thirty_two_places_to_present_the_secret`
`verified-by: bravebot_browser::host::only_the_same_secret_matches`
`verified-by: bravebot_browser::host::each_secret_is_new`

<a id="BROWSER-4"></a>
### BROWSER-4: the native host speaks only to our extension

The host manifest's `allowed_origins` names one extension id. The host also checks the origin Brave
passes it against the id `install` recorded in the directory, and exits without creating anything
when it names any other extension or none was recorded.

**Why.** `allowed_origins` is enforced by the browser. Checking the origin again in the host keeps
a manifest someone edited from handing the relay to a different extension.

`verified-by: bravebot_browser::relay::the_host_refuses_an_extension_it_was_not_installed_for`

<a id="BROWSER-5"></a>
### BROWSER-5: the socket exists while the extension is connected

The native host runs from the extension's connect until its port closes, and removes the socket and
the secret as it exits. While no extension is connected there is no socket. One host serves the
directory at a time: a host holds a lock on the directory's `lock` file for as long as it runs, and
one that cannot take it exits before it touches the socket or the secret.

**Why.** Brave owns the host's lifetime and nothing else can start it. A socket left behind would
point a connecting server at a relay that is not there. Two hosts starting together would each find
no socket, each write its secret over the other's, and the first to exit would remove the files the
other still serves.

`verified-by: bravebot_browser::relay::the_socket_and_secret_go_when_the_extension_disconnects`
`verified-by: bravebot_browser::relay::a_host_does_not_start_while_another_holds_the_lock`

<a id="BROWSER-6"></a>
### BROWSER-6: a call with no extension connected fails at once

A tool call made while the socket is absent or refuses the connection is a failure saying no
extension is connected, and naming the two reasons: Brave is not running, or the extension is not
installed in it. A call the extension disconnects during is a failure saying so. A call the
extension does not answer within 30 seconds is a failure saying that. None of them is an empty
result, and none waits for the extension to appear.

A call to a tool of `scripts`, `input`, `input-trusted`, `capture-trusted`, `console-trusted`, `upload-trusted` or `screen` waits 90 seconds, and the failure says 90. The
extension asks the person in a window of the browser before it acts, and the person has 60 seconds
to answer there ([BROWSER-34](#BROWSER-34), [BROWSER-38](#BROWSER-38), [BROWSER-42](#BROWSER-42)).
The 30 more are for finding the element, bringing the tab forward, acting and sending the reply, and
are also the longest `wait_for_element` may wait. For `run_script` they are for finding the tab,
the script, which the extension gives up on after 15 seconds ([BROWSER-46](#BROWSER-46)), and the
reply. The servers started for `scripts`, `input`, `input-trusted`,
`capture-trusted`, `console-trusted`, `upload-trusted` and `screen` are the only ones that wait longer than 30 seconds,
and the server started for no group waits 30. The declaration `install` prints for each of the seven
bounds a call at 120 seconds ([BROWSER-21](#BROWSER-21)), above the wait, so that the relay's own
failure reaches the session before the agent stops the server. For `input-trusted`, `capture-trusted`, `console-trusted` and `upload-trusted` the
waits nest: the person's 60 seconds, the 80 seconds the extension
waits for the second extension's reply ([BROWSER-12](#BROWSER-12)), the relay's 90 seconds and the
declaration's 120.

**Why.** An empty result would read as "the page is empty" or "no history matches". Waiting would
hold the turn on something only a person can fix. A wait of 60 seconds or less would fail a call
while its window still waited for the person, and an action the person allowed late would then be
done after the session had been told it failed. A wait of the extension for the second extension
that ended after the relay's would leave the relay to answer first, with a failure that names no
extension.

`verified-by: bravebot_browser::relay::a_call_with_no_extension_connected_fails_at_once_and_says_why`
`verified-by: bravebot_browser::relay::a_call_to_a_stale_socket_fails_at_once_and_says_why`
`verified-by: bravebot_browser::relay::a_call_the_extension_disconnects_during_fails_and_says_so`
`verified-by: bravebot_browser::relay::a_call_the_extension_does_not_answer_times_out_and_says_so`
`verified-by: bravebot_browser::relay::the_production_reply_timeout_is_thirty_seconds`
`verified-by: bravebot_browser::relay::the_wait_for_a_person_is_ninety_seconds_and_the_declared_bound_is_thirty_more`
`verified-by: bravebot_browser::server::a_call_to_input_waits_for_the_extension_as_long_as_a_person_does_and_others_do_not`
`verified-by: bravebot_browser::extension::the_wait_for_input_is_longer_than_the_persons_and_the_declared_bound_is_longer_still`
`verified-by: bravebot_browser::server::a_call_to_input_trusted_waits_for_the_extension_as_long_as_a_person_does`
`verified-by: bravebot_browser::extension::the_waits_of_a_trusted_input_call_grow_from_the_persons_window_to_the_declared_bound`

<a id="BROWSER-7"></a>
### BROWSER-7: each reply reaches the session that asked

Several BraveBot sessions may be connected to one native host at once. The host gives each request
an id unique across its connections before sending it to the extension, and hands each reply back
only on the connection the request came in on, with the id that connection used. A connection that
does not take a line the host writes it within 5 seconds, a reply or an error, is closed, and the
replies after it go on to theirs.

**Why.** There is one extension and one native messaging port. Two sessions each numbering requests
from 1 would otherwise receive each other's replies. The host hands replies over on one thread in
the order the extension answers, so a connection that stopped reading would otherwise hold every
later reply to every other session.

`verified-by: bravebot_browser::relay::each_reply_reaches_the_session_that_asked`
`verified-by: bravebot_browser::relay::a_peer_that_stops_reading_holds_up_no_other_session`
`verified-by: bravebot_browser::relay::a_peer_that_sends_lines_and_does_not_read_is_closed`
`verified-by: bravebot_browser::host::a_peer_has_five_seconds_to_take_a_line`

<a id="BROWSER-8"></a>
### BROWSER-8: no message to the extension exceeds the platform limit

A message from the host to the extension is at most 1 MiB (1,048,576 bytes), the limit native
messaging sets, and a request that would exceed it is refused with an error under its own id and
not sent. An id that is itself 1 MiB or longer is not kept, and the refusal carries none. A message
from the extension may be up to 64 MiB, and a longer one ends the host. Brave itself refuses to send
one over 64 MiB from the extension (`postMessage` throws), so the host reads a longer one only from
a program that is not Brave.

**Why.** Brave closes the port on a message over 1 MiB, which disconnects every session at once.
Keeping an id of any length would mean holding as much of a request as a peer cares to send, where
the rest of it is drained unread; JSON-RPC answers a request whose id it could not read with no id.

`verified-by: bravebot_browser::relay::a_request_over_the_limit_is_refused_and_never_reaches_the_extension`
`verified-by: bravebot_browser::relay::a_request_far_over_the_limit_keeps_its_id_and_the_next_request`
`verified-by: bravebot_browser::relay::a_request_whose_id_is_too_long_to_keep_is_refused_under_no_id`
`verified-by: bravebot_browser::relay::a_request_padded_after_its_id_is_refused_under_that_id`
`verified-by: bravebot_browser::relay::a_message_over_the_limit_from_the_extension_ends_the_host`
`verified-by: bravebot_browser::framing::the_limits_are_one_megabyte_out_and_sixty_four_in`
`verified-by: bravebot_browser::framing::a_message_over_the_limit_to_the_extension_is_refused_and_nothing_is_written`
`verified-by: bravebot_browser::framing::a_length_over_the_limit_from_the_extension_is_an_error`
`verified-by: bravebot_browser::framing::a_written_message_reads_back_as_itself`
`verified-by: bravebot_browser::framing::a_message_cut_short_is_an_error`
`verified-by: bravebot_browser::lines::a_line_past_the_limit_is_too_long`

<a id="BROWSER-9"></a>
### BROWSER-9: installing writes one manifest in the locations Brave reads

`bravebot-browser install [<extension id>]` writes one host manifest named `com.brave.bravebot`,
naming this program by its absolute path and that extension alone, into the per-user location
Brave's stable channel reads. Given no id, it names the extension in `extension/`, whose id its
pinned key fixes:

| Platform | Where |
|---|---|
| macOS | `~/Library/Application Support/Google/Chrome/NativeMessagingHosts/` |
| Linux | `$XDG_CONFIG_HOME/BraveSoftware/Brave-Browser/NativeMessagingHosts/`, `~/.config` where unset |

It records the id in the socket's directory for the host to check, writes nothing else, changes no
other host's manifest, and writes nothing at all for an id that is not 32 letters from `a` to `p`.
A word it does not take is answered with its usage, which lists the groups and the presets, and
nothing is written. Such a word is `--manifest-dir` with no directory after it, `--groups` or
`--preset` with no value or naming what is not a group or a preset, and both of those flags, or one
of them twice. What `--groups` and `--preset` change is the declarations it prints. It says which
extension the host now serves, the one named or the one in `extension/`, and its usage says that
an id is recorded in place of the one before, so running it again to print declarations without
the id records the extension in `extension/`.

**Why.** On macOS Brave reads Chrome's location rather than its own, so a manifest in a Brave
directory there is never read. Chrome reads the same file, and `allowed_origins` is what keeps
Chrome from starting the host for anything but our extension. On Linux the location is under
Brave's own profile directory, and a channel other than stable has its own.
Brave reads the location for every user data directory it runs on, so a Brave started on a
directory of its own finds the same manifest ([BROWSER-36](#BROWSER-36)).

`verified-by: bravebot_browser::relay::installing_writes_one_manifest_for_our_extension_alone`
`verified-by: bravebot_browser::relay::installing_with_no_id_records_the_extension_in_this_repository`
`verified-by: bravebot_browser::relay::installing_says_which_extension_the_host_serves`
`verified-by: bravebot_browser::relay::installing_with_no_manifest_directory_uses_the_location_brave_reads`
`verified-by: bravebot_browser::relay::installing_refuses_what_is_not_an_extension_id`
`verified-by: bravebot_browser::relay::installing_with_a_flag_missing_its_value_prints_the_usage`
`verified-by: bravebot_browser::relay::installing_with_a_group_or_preset_it_does_not_know_prints_the_usage_and_writes_nothing`
`verified-by: bravebot_browser::install::install_refuses_words_it_does_not_take`
`verified-by: bravebot_browser::install::the_manifest_goes_where_brave_reads_it`
`verified-by: bravebot_browser::install::an_extension_id_is_32_letters_from_a_to_p`

<a id="BROWSER-10"></a>
### BROWSER-10: the tool list is fixed, and a tool not on it is refused

The MCP server, started with no group, offers the same list whether or not the extension is
connected: `get_platform_info`, `list_tabs`, `list_frames`, `read_page`, `search_history`,
`search_bookmarks` and `open_tab`. Each calls the extension method of the same name with the tool's arguments. A
call naming any other tool is refused by the server and nothing is sent to the extension.
`list_frames` names a tab by its exact URL, `read_page` names that tab and, optionally, one
frame by its exact URL, and `open_tab` names the HTTP or HTTPS URL to open. `get_platform_info` returns the operating system and architecture Brave
runs on, and nothing else, so a person can see the extension answer without it telling anything
about them. A tool that is not one of these seven is offered by the server of its group and by no
other ([BROWSER-21](#BROWSER-21)): `read_page_links` and `read_page_forms` in `page-read`, which
name a tab by its exact URL ([BROWSER-13](#BROWSER-13)), and `list_page_elements` in `page-read`, which
takes that URL and nothing else and whose result is the page's controls as numbered text
([BROWSER-41](#BROWSER-41)), and `screenshot_page` in `page-read`, which takes that URL and nothing
else and whose result is a picture ([BROWSER-35](#BROWSER-35)), and `capture_screen`, which takes
nothing, and `record_gif`, which takes a URL and a count of frames, in `screen`, whose results are
pictures ([BROWSER-42](#BROWSER-42)), and `save_page_archive` in
`downloads`, which takes that URL and nothing else, saves the page as a file in the Downloads folder
and returns the file's name ([BROWSER-40](#BROWSER-40)), and `focus_tab`, `pin_tab`, `mute_tab`,
`move_tab`, `group_tabs`, `ungroup_tabs`, `update_tab_group`, `discard_tab`, `duplicate_tab`,
`set_zoom`, `focus_window` and `resize_window` in `tabs-manage`, which name a tab, or for the two that
group several the tabs, by exact URL and take only the switch, position, title, color, factor or size
they set ([BROWSER-23](#BROWSER-23)), and `create_bookmark`, `create_bookmark_folder`,
`move_bookmark`, `rename_bookmark`, `add_reading_list_entry` and `update_reading_list_entry` in
`bookmarks-write`, which name a bookmark or reading list entry by its URL and a folder by its exact
title and take no id ([BROWSER-28](#BROWSER-28)), and `navigate_tab`, `reload_tab`, `go_back`,
`go_forward`, `open_window` and `restore_closed` in `navigate`, which take a URL, and for
`navigate_tab` the address to go to, and no tab id, window id or session id
([BROWSER-16](#BROWSER-16)), and `search_web` in `navigate`, which takes the words to search for, at
most 256 characters, and `wait_for_load`, which takes a tab's URL ([BROWSER-16](#BROWSER-16)), and
`list_cookie_names`, which takes a URL, and `delete_cookie`, which takes a cookie's name and a URL, in
`cookies`, and no tool takes a cookie's value ([BROWSER-30](#BROWSER-30)), and `click_element`,
`type_text`, `select_option`, `press_key`, `scroll_page` and `wait_for_element` in `input`, which
name a tab by its exact URL and an element by a role and an accessible name, or for the first three
by the number the element has in the last list `list_page_elements` made for that URL, and take no
selector, node id or coordinate ([BROWSER-33](#BROWSER-33)), and `take_held` in `held`, which takes no
argument and returns what the person held out in the browser ([BROWSER-39](#BROWSER-39)). The server
started for `input` offers the six with a list of words for each argument that has a closed set (the
role, the key, the direction and the button) and a largest size for each text and number, the element
number included, and describes each action as asked in a window
of the browser ([BROWSER-34](#BROWSER-34)), with a password, a payment card and a file field
refused and the events synthetic. `click_element_trusted`, `type_text_trusted`, `press_key_trusted`,
`hover_element` and `drag_element` are in `input-trusted`. They name a tab by its exact URL and an
element by a role and an accessible name in the same way, and `drag_element` names the element it
drops on by a second role and name, `onto_role` and `onto_name`. No argument of the five is a
selector, a node id or a coordinate ([BROWSER-33](#BROWSER-33), [BROWSER-38](#BROWSER-38)). The
server started for `input-trusted` offers the five with the same lists of words and the same
largest sizes, and describes each as done by the BraveBot Trusted Input extension, as real input the
page cannot tell from a person's, and as asked in a window of that extension.
`screenshot_full_page` is in `capture-trusted`. It takes a tab's URL and no other argument, and the
server started for `capture-trusted` offers it described as done by the BraveBot Trusted Input
extension, as asked in a window of that extension, as a picture the model does not see, and with
the limits of its picture ([BROWSER-43](#BROWSER-43)).

`read_console` is in `console-trusted`. It takes a tab's URL, a `levels` of three words and a number
of entries, and the server started for `console-trusted` offers it described as done by the BraveBot
Trusted Input extension, as asked in a window of that extension, as a text the model cannot read
directly, and with the limits of the text ([BROWSER-44](#BROWSER-44)).

`choose_file` is in `upload-trusted`. It takes a tab's URL, the name of one file input as the page
gives it, optionally the input's role, and `path`, at most 900 characters, and the server started for
`upload-trusted` offers it described as done by the BraveBot Trusted Input extension, as asked in a
window of that extension, as limited to a file of the `uploads` directory that the relay's server
looked at first, and as answering only that it was done ([BROWSER-45](#BROWSER-45)).

`run_script` is in `scripts`. It takes a tab's URL, at most 300 characters, and `code`, at most 2,000
characters, both required, and the server started for `scripts` offers it described as run in the page
only after the person has read all of the code in a window of the extension, as limited by nothing but
that reading, and as answering with a text the model cannot read directly
([BROWSER-46](#BROWSER-46)).

Started with `--group <id>`, the server offers the tools of that group and no others, and refuses a
call naming a tool of another group in the same way ([BROWSER-21](#BROWSER-21)). The tools of each
of the twenty-one groups are the ones `extension/groups.js` lists, and
[the design](../design/browser-extension.md#capability-groups) lists them with the Chrome methods
behind each.

`list_windows`, `list_tab_groups` and `list_recently_closed` are in `tabs-read`, and `get_visits`,
`list_top_sites` and `list_synced_tabs` are in `history-read`. Each calls the extension method of the
same name, and is offered by the server started for its group and not by the one started with no
group ([BROWSER-21](#BROWSER-21)). `get_visits` names the URL whose visits it lists, exactly as
written, and returns the newest visits up to the count asked for. The other five take no argument.
`list_windows` and `list_tab_groups` return no tab, `list_recently_closed` returns the browser's
recently closed tabs and windows, `list_top_sites` the sites it counts as most visited, and
`list_synced_tabs` the tabs on the person's other devices, by device.

`list_bookmark_folder`, `list_recent_bookmarks` and `list_reading_list` are in `bookmarks-read`, and
`list_requests` is in `network-watch`; each is offered by the server of its group alone and calls
the extension method of the same name. `list_bookmark_folder` names the folder by its exact title,
which may be the title of a built-in folder at the top of the tree such as Bookmarks bar or Other
bookmarks ([BROWSER-28](#BROWSER-28)), and fails where no folder has that title or more than one
does. It returns the title and URL of each bookmark in the folder and the title of each folder in
it. Where no folder is named it returns the title of each built-in folder and nothing else, no id and
no URL, so a title can be learned in a browser set to another language. `list_recent_bookmarks`
returns the newest bookmarks and `list_reading_list` the reading list, each newest first with the
time it was added, and `list_reading_list` also whether the entry was read. Each of the three takes a
count, at most 100 and 20 where none is given, and `list_bookmark_folder` also the optional title of
the folder; none takes another argument. `list_requests` names a tab by its exact URL and is described in
[BROWSER-25](#BROWSER-25).

`get_privacy_settings`, `get_content_settings`, `get_font_settings`, `get_system_info`,
`get_idle_state`, `get_access_flags` and `keep_awake` are in `browser-info`, and each is offered by
the server of that group alone and calls the extension method of the same name. The first three
read settings with `get` and never set or clear one; each setting is read by a call of its own, so
one the browser will not report is listed with its error and one the browser does not have is left
out. `get_content_settings` names one HTTP or HTTPS URL, exactly as written, and gives the browser
that URL alone. `get_font_settings` does not list the fonts installed. `get_system_info` gives the
processor count, architecture, model and features, the memory, the name, size, position and density of
each display, and the name, type and capacity of each storage device, and does not eject a device.
`get_idle_state` takes a whole number of seconds above 0, 60 where none is given. `get_access_flags`
says whether the person allowed the extension on `file:` URLs and in private windows. `keep_awake`
sets something on the computer and is described in [BROWSER-26](#BROWSER-26).

`close_tab`, `close_window`, `delete_history_entry`, `delete_history_range`, `clear_browsing_data`,
`remove_bookmark`, `remove_bookmark_folder`, `remove_reading_list_entry` and `delete_downloaded_file`
are in `remove-data`, and each is offered by the server of that group alone and calls the extension
method of the same name. Each deletes, and they are described in [BROWSER-31](#BROWSER-31).

`save_page_archive`, `download_file`, `list_downloads`, `pause_download`, `resume_download`,
`cancel_download` and `show_download` are in `downloads`, and each is offered by the server of that
group alone and calls the extension method of the same name. `save_page_archive` is described in
[BROWSER-40](#BROWSER-40). `download_file` names the HTTP or HTTPS URL to download, the
other four that act on a download name it by the path or the address the browser holds for it, and
`list_downloads` takes a count. They are described in [BROWSER-32](#BROWSER-32).

Every tool is named in lower-case words joined by underscores, a verb and what it acts on. Every
argument is one a person can judge in the question before the call: an address, a name or a short
text, a number or a time they can read, one of a short list of words, or yes or no. A tab id, a node
id and a selector read from a page are the argument of no tool. The routing field of a call is the
alias and the tool, `brave-<group>:<tool>` for a server started with `--group` and `brave:<tool>`
for one started with none, and never an argument ([ROUTE-7](routing.md#ROUTE-7),
[SERVERS-7](mcp-servers.md#SERVERS-7)). A tool whose routing would come from page content is not
built. The tools of `input` and `input-trusted` take the element they act on from the planner's
words, and the person is shown the element those words led to before anything is done
([BROWSER-33](#BROWSER-33)); their routing fields are `brave-input:<tool>` and
`brave-input-trusted:<tool>`. A tool is listed with a name, a description and an input schema
and nothing of its result, which is untrusted and private whichever tool made it
([MCP-1](mcp.md#MCP-1)) and which the planner never reads. The
[design](../design/browser-extension.md#tool-surface) gives the routing field and the arguments of
each tool and what the question shows for each argument.

The description of a tool that deletes, closes, navigates or sets something says what it changes
and how far: `close_window` says it closes the window with every tab in it, `clear_browsing_data`
that it clears for every site, and a tool that deletes names what it deletes. Every argument that is
a list states `maxItems` at 50, the length the extension refuses a longer list at
([BROWSER-27](#BROWSER-27)), so a client sees the bound before it calls.

**Why.** A person answers for a server's tool list at the start of a session. A list
that changed with whether the extension was connected would offer tools they were not shown.
A name off the list reaching the extension would be a tool nobody vouched for. The description is
where the extent of a change is said, so a description that lost the words "every tab" or "every
site" would be a server approved on less than it does. The question before
each call shows its arguments, and a URL is one a person can judge there where a tab id is not, so
`get_visits` takes a URL and no tool takes a tab id or a window id. A node id or a selector read from
a page is no better, since it says nothing about which page it is or where an action lands. The
arguments the tools take are held to a list, so an argument that is new is added by a person who has
read [ROUTE-7](routing.md#ROUTE-7) and can say what the question shows for it.

`verified-by: bravebot_browser::relay::the_tool_list_is_the_same_whether_or_not_the_extension_is_connected`
`verified-by: bravebot_browser::relay::the_list_of_a_server_with_no_group_is_the_seven_tools_with_their_descriptions_and_schemas`
`verified-by: bravebot_browser::relay::a_tool_that_deletes_closes_navigates_or_sets_something_says_what_it_changes`
`verified-by: bravebot_browser::tools::every_list_argument_states_the_most_entries_a_call_may_hold`
`verified-by: bravebot_browser::extension::the_bound_the_relay_states_on_a_list_is_the_one_the_extension_enforces`
`verified-by: bravebot_browser::extension::the_design_document_names_every_tool_the_server_offers`
`verified-by: bravebot_browser::extension::the_design_documents_group_tables_are_the_ones_the_extension_and_the_relay_hold`
`verified-by: bravebot_browser::relay::a_tools_arguments_are_the_extension_methods_parameters`
`verified-by: bravebot_browser::relay::a_tool_that_is_not_on_the_list_is_refused_without_asking_the_extension`
`verified-by: bravebot_browser::tools::a_tool_is_found_by_its_exact_name`
`verified-by: bravebot_browser::tools::every_tool_takes_an_object`
`verified-by: bravebot_browser::tools::a_page_is_asked_for_by_its_url_and_optional_frame_url`
`verified-by: bravebot_browser::tools::the_visits_to_a_page_are_asked_for_by_its_url_and_a_count`
`verified-by: bravebot_browser::tools::a_tab_is_acted_on_by_its_url_and_what_is_asked_of_it`
`verified-by: bravebot_browser::relay::a_tool_outside_the_seven_is_called_through_its_group_alone`
`verified-by: bravebot_browser::tools::a_folder_is_asked_for_by_its_title_and_the_requests_of_a_page_by_its_url`
`verified-by: bravebot_browser::tools::the_browser_info_tools_take_a_url_a_number_of_seconds_or_minutes_and_nothing_else`
`verified-by: bravebot_browser::tools::a_bookmark_is_named_by_its_url_and_a_folder_by_its_title_and_never_by_an_id`
`verified-by: bravebot_browser::relay::a_bookmark_is_changed_through_its_group_with_the_url_and_title_named`
`verified-by: bravebot_browser::tools::a_tab_is_moved_by_its_url_and_an_address_and_never_by_an_id`
`verified-by: bravebot_browser::relay::a_tab_is_taken_to_an_address_through_its_group_with_both_urls_named`
`verified-by: bravebot_browser::tools::a_search_is_asked_for_by_short_text_and_a_wait_by_a_url`
`verified-by: bravebot_browser::tools::the_cookie_tools_take_a_url_and_a_name_and_no_tool_takes_a_cookie_value`
`verified-by: bravebot_browser::relay::a_search_a_wait_and_the_cookie_tools_are_called_through_their_group_with_what_they_name`
`verified-by: bravebot_browser::tools::what_is_deleted_is_named_by_a_url_two_moments_or_a_list_of_kinds_and_nothing_else`
`verified-by: bravebot_browser::tools::a_download_is_started_by_one_url_and_acted_on_by_filename_or_url_and_nothing_else`
`verified-by: bravebot_browser::tool_surface::every_tool_is_named_in_lower_case_words_joined_by_underscores`
`verified-by: bravebot_browser::tool_surface::every_argument_of_every_tool_is_one_a_person_can_judge_in_the_question`
`verified-by: bravebot_browser::tool_surface::a_schema_names_its_arguments_in_properties_and_each_is_a_word_a_number_a_choice_or_a_list_of_words`
`verified-by: bravebot_browser::tool_surface::a_tool_is_listed_with_a_name_a_description_and_an_input_schema_and_no_result_shape`
`verified-by: bravebot_browser::extension::the_design_document_states_the_routing_field_and_the_arguments_of_every_tool`
`verified-by: bravebot_browser::extension::the_input_tools_offer_the_lists_of_words_the_extension_accepts`
`verified-by: bravebot_browser::tools::the_input_tools_are_offered_by_the_server_of_input_alone`
`verified-by: bravebot_browser::tools::the_input_descriptions_say_that_the_person_is_asked_in_the_browser_and_what_is_refused`
`verified-by: bravebot_browser::tools::every_argument_of_an_input_tool_is_a_word_from_a_list_or_has_a_largest_size`
`verified-by: bravebot_browser::tools::an_element_is_named_by_a_role_and_a_name_or_by_the_number_a_list_gave`
`verified-by: bravebot_browser::tools::the_list_of_elements_takes_a_url_and_its_description_says_the_model_does_not_read_it`
`verified-by: bravebot_browser::extension::the_relay_states_the_bounds_of_the_list_the_extension_makes`
`verified-by: bravebot_browser::relay::an_input_tool_is_called_through_its_group_alone_with_what_it_names`
`verified-by: bravebot_browser::tools::the_trusted_input_tools_are_offered_by_the_server_of_input_trusted_alone`
`verified-by: bravebot_browser::tools::the_trusted_input_descriptions_say_who_acts_that_the_person_is_asked_in_a_window_and_what_is_refused`
`verified-by: bravebot_browser::relay::a_trusted_input_tool_is_called_through_its_group_alone_with_what_it_names`
`verified-by: bravebot_browser::tools::the_tools_that_return_a_picture_are_the_four_and_take_a_url_a_count_or_nothing`
`verified-by: bravebot_browser::tools::the_screenshot_description_says_the_picture_is_not_seen_by_the_model_and_what_it_shows`
`verified-by: bravebot_mcp::stdio::a_tool_result_is_labelled_untrusted_and_private`
`verified-by: by-construction (extension/tests/tools.test.mjs asserts that get_platform_info answers with the operating system and architecture alone, a field the browser adds besides them left out, and reaches no API but the platform's; it runs where the test under BROWSER-13 runs)`
`verified-by: by-construction (extension/tests/tools.test.mjs asserts that list_windows passes on a window's id, type, state, focus and privacy and asks the browser for no tab, that list_tab_groups passes on a group's id, window, title, color and collapsed state, that list_recently_closed gives the time a tab or window closed from the seconds the browser reports and the title and URL of its tabs, that get_visits gives the newest visits first up to the count asked for with the URL passed to the browser as written, that list_top_sites gives each site's title and URL, and that list_synced_tabs gives the tabs of each device with a window's tabs flattened; it also asserts that list_bookmark_folder lists the folder whose title is exactly the one asked for, with its bookmarks and folders and not those of a folder inside it or of a folder or bookmark whose title only contains it, and refuses a title that is no folder or is several, that list_recent_bookmarks and list_reading_list give the newest entries first with the time each was added and the count asked for, and that all three take a count and nothing else; it runs where the test under BROWSER-13 runs)`
`verified-by: by-construction (extension/tests/tools.test.mjs asserts that get_privacy_settings gives each setting the browser has with its value and who controls it, in order, with the error of one it will not report and nothing for one it lacks, and calls get with no details and no call that sets or clears; that get_content_settings gives the setting of each kind of content the browser has for the URL asked for, passing that URL as the primary URL alone, with the error of a kind it will not report, and refuses a call with no URL or a URL that is not HTTP or HTTPS; that get_font_settings gives the font of each of the seven generic families and the three sizes with who controls them and never asks for the list of installed fonts; that get_system_info gives the fields named above and no other, calls none that ejects a device, and fails with the browser's words where one of the four does; that get_idle_state asks the browser about the seconds given or 60 and refuses seconds that are not a whole number above 0; and that get_access_flags gives true for each flag the browser says is true and false for any other answer; it runs where the test under BROWSER-13 runs)`

<a id="BROWSER-11"></a>
### BROWSER-11: a socket that exists is one that accepts

The host binds the socket under a name of its own and renames it into place only once it listens.
A server that finds the socket can connect to it.

**Why.** A socket file exists from its bind, before its listen. A server connecting between the two
is refused, and would report that no extension is connected while one is.

`verified-by: bravebot_browser::host::the_socket_is_published_only_after_it_accepts`

<a id="BROWSER-12"></a>
### BROWSER-12: the extension answers each tool the server offers, under the host install names

The extension asks Brave for the host by the name `install` gives its manifest, and answers exactly
the methods the server's tools call, with one switch for each tool in its options and one for each
group of tools ([BROWSER-19](#BROWSER-19)). A tool runs only while both are on
([BROWSER-14](#BROWSER-14)). The `key` in its manifest fixes its id, and that id is the one `install`
records given none. Its manifest requires `nativeMessaging`, `storage` and `alarms`. The permissions
the tools call are optional permissions, and each group in `extension/groups.js` lists those its own
tools call, so the browser holds a group's permissions only once the person turned the group on
([BROWSER-18](#BROWSER-18)). Its optional hosts are `https://*/*`, `http://*/*` and `<all_urls>`, which
`screenshot_page` and `record_gif` need ([BROWSER-35](#BROWSER-35), [BROWSER-42](#BROWSER-42)). Every string in an answer is well formed: an unpaired surrogate is sent
as U+FFFD.

The extension takes requests from that native messaging port alone. Its manifest declares no
`externally_connectable`, and no script registers `onMessageExternal` or `onConnectExternal`, so no
web page and no other extension can send it a request. It sends requests to one other extension,
the BraveBot Trusted Input extension ([BROWSER-37](#BROWSER-37)), by its id, and takes none from
any extension. The send to another extension is `runtime.sendMessage` with an id in
`extension/trusted.js`, called once and in no other script, to the id that the second extension's
key gives, and the tools of `input-trusted`, `capture-trusted`, `console-trusted` and `upload-trusted` are the only
callers ([BROWSER-38](#BROWSER-38), [BROWSER-44](#BROWSER-44)). The
capture window of `screen` calls `runtime.sendMessage` once with a message and no id, which reaches
this extension's own worker ([BROWSER-42](#BROWSER-42)).

The extension does not use the side panel: its manifest declares no `sidePanel` permission and no
`side_panel` key, and no script names `sidePanel`.

**Why.** The two halves are written in two languages and released together, and nothing at run time
would say they disagreed: a tool the extension does not answer fails like a page that cannot be
read, and an id the host was not installed for is refused like an extension it should not serve.
JSON carries an unpaired surrogate as an escape the host refuses to parse, so an answer holding one
would leave the call waiting out its timeout. A web page can message an extension only if the
manifest's `externally_connectable` names it, and a message from another extension reaches only a
script that listens for one, so declaring neither leaves the port as the one way to run a tool. A
send needs no listener on the sending side, so the door stays one: the second extension takes a
message from this one alone, and the person's switches, lifetimes and site lists, which only this
extension holds, come before the send.
Brave merged the side panel into its Sidebar, which has no control to open a side-panel extension
(brave-browser #31334), and `sidePanel.open` from a popup window crashes the browser (#59256), so
the toolbar popup and the options page are where the extension shows anything.

`verified-by: bravebot_browser::extension::the_extension_answers_every_tool_the_server_offers_and_no_other`
`verified-by: bravebot_browser::extension::the_options_page_labels_every_tool_the_server_offers_and_no_other`
`verified-by: bravebot_browser::extension::the_extension_connects_to_the_host_install_names`
`verified-by: bravebot_browser::extension::the_extension_has_each_permission_its_tools_require`
`verified-by: bravebot_browser::extension::the_id_install_records_is_the_one_the_extensions_key_gives`
`verified-by: bravebot_browser::extension::the_extension_accepts_no_message_from_a_web_page_or_another_extension`
`verified-by: bravebot_browser::extension::the_extension_sends_a_message_to_another_extension_from_one_call_in_trusted_js`
`verified-by: bravebot_browser::extension::each_extensions_id_is_the_one_its_key_gives_and_the_second_names_the_first`
`verified-by: bravebot_browser::extension::the_extension_does_not_use_the_side_panel`
`verified-by: by-construction (extension/tests/namespaces.test.mjs marks the side panel reserved in the design document and asserts that a reserved namespace is neither called nor declared, in the permissions or as the side_panel key, and that the manifest declares no externally_connectable and no script listens for an external message; extension/tests/trusted.test.mjs asserts that trusted.js is the only script that names sendMessage, once, and that the id it sends to is the one the second extension's own script names; both run where the test under BROWSER-13 runs)`
`verified-by: by-construction (extension/tests/tools.test.mjs asserts that answers from list_tabs, list_frames and read_page holding an unpaired surrogate reach JSON with no surrogate escape and with U+FFFD in its place; it runs where the test under BROWSER-13 runs)`

<a id="BROWSER-13"></a>
### BROWSER-13: a page or frame is read at exactly the URLs asked for

`list_frames` returns only HTTP and HTTPS frame URLs from the open tab whose URL is the one asked
for, character for character, and says which is the top frame. Where no tab is at that URL it fails
without asking for frames. Where the tab leaves that URL before its frames are returned it fails
without returning any frame URL.

`read_page` with no `frame_url` reads the top page of the tab at `url`. With a `frame_url`, it reads
only the one document at that exact HTTP or HTTPS URL inside the tab at `url`, using the document
identity Brave returned with the frame rather than a reusable frame number. Where there is no such
frame, or more than one, it fails without running a script. It also fails without returning any
text if the frame leaves `frame_url`, or the outer tab leaves `url`, before the read completes.

A successful read returns at most 100,000 characters of the page's text, never ending it on half a
character, and says whether it cut the page short. A page the browser will not let an extension
read, such as its own settings, is a failure saying so.

Every tool that names an open tab names it by its URL, character for character: `list_frames`,
`read_page`, `read_page_links`, `read_page_forms`, `list_page_elements`, `screenshot_page`,
`record_gif`, `save_page_archive`,
`list_requests`, `navigate_tab`, `reload_tab`, `go_back`, `go_forward`, `wait_for_load`, `close_tab`, `close_window`
and the tools of `tabs-manage`,
which for `group_tabs` and `ungroup_tabs` is each URL of `urls`. A URL at which more than one tab is
open names none of them. The call fails and says how many tabs are at the URL and nothing else
about them, before it reaches a script, a frame, a window or a tab, and `group_tabs` and
`ungroup_tabs` change nothing when any one of their URLs is at more than one tab. A tool that
names a tab finds it in one function, so the rule holds for a tool added later.

`read_page_links` and `read_page_forms` read the top document of the tab at `url` in the same way:
they fail, before a tab is looked for, for a `url` that is not HTTP or HTTPS, which a tab can have
where the browser lets the extension into `file:` and `ftp:` pages; they fail where no tab is at that
URL, where the page cannot be read or the script returns nothing,
and, without returning anything of the page, where the tab leaves `url` before the script ran. They
enter every shadow root of the document, a closed one through `chrome.dom.openOrClosedShadowRoot`
and an open one through the element where that call is missing. `read_page_links` returns the text
and the URL of each link whose URL is HTTP or HTTPS, in the order met, the text with its spaces
collapsed and cut at 200 characters. `read_page_forms` returns, for each `form` element, its action
as written, its method (`get`, `post` or `dialog`) and the name and type of each of its fields,
a field's name being its `name` attribute or, where it has none, its `id`. The script never reads
what a field holds, so no value comes back, and a control that is in no form is not listed. Each
list ends before the first item that would take the strings returned past 100,000 characters and
says so, never cutting an item in the middle.

**Why.** Both URLs are what the person saw in the question before the call. A `list_frames` result
is untrusted and private under [MCP-1](mcp.md#MCP-1), [LABEL-3](labels.md#LABEL-3) gives the planner
only its quarantined reference, and [VIEW-3](terminal-transcript.md#VIEW-3) shows its marked content
to the person. The person chooses a URL from that result and supplies it in a later message. A tab
or frame whose URL only resembles one is different content, which they could have refused. A
non-web frame URL such as `about:blank` does not tell them which content it will expose. Two frames
at the same URL cannot be told apart by the argument they approved, and neither can two tabs: the
first of them may be in another window or hold other work than the one they meant.

`verified-by: bravebot_mcp::stdio::a_tool_result_is_labelled_untrusted_and_private`
`verified-by: bravebot_core::policy::untrusted_content_is_presented_as_a_reference`
`verified-by: bravebot_core::reference::a_quarantined_presentation_shows_no_content`
`verified-by: bravebot_tui::render::quarantined_content_is_shown_and_marked_on_every_line`
`verified-by: bravebot_agent::turn::quarantined_content_reaches_the_person_and_not_the_planner`
`verified-by: by-construction (extension/tests/tools.test.mjs uses tabs and frames with distinct ids, URLs and text to assert exact tab and frame selection, each navigation check and refusal, the text bound, and that a URL no tab or unique frame is at reaches no script, that a URL at which two or three tabs are open fails for each tool that names a tab, giving the number and nothing of the tabs and reaching no script, frame, window or tab API, that a tool given a URL is in the table of those tests or in the list of tools that name no open tab, and that a URL one tab is at still names it beside tabs at URLs that only resemble it, and, for the links and forms, that a tab at an address that is not HTTP or HTTPS is refused before a tab is looked for, that only the web links of the one tab asked for are listed with their text collapsed and cut, that closed and open shadow roots are entered and an open one still is without chrome.dom, that each field is listed with its name and type and no value is read, whether a field is in a form or not, that a name and the list are cut at the limits, and that a page that cannot be read, gave nothing or moved reaches no result; make check-extension runs it, and check-ui-build depends on that target, so the Front end CI job runs it on every change the classifier gives the ui area, which a change under extension/ is)`

<a id="BROWSER-14"></a>
### BROWSER-14: only the platform check starts on, and a tool that is off touches nothing

Only the platform check starts on. Every other tool, such as listing tabs and their frames, reading a
page or frame, searching history and bookmarks, and opening a tab, starts off until a person turns on
the group it is in, in the extension's options ([BROWSER-19](#BROWSER-19)).
A call to a tool that is off is refused before the browser is asked anything. A tool that is on is
refused the same way where its group is off, where the group's lifetime has ended
([BROWSER-24](#BROWSER-24)), where the group reads pages and the URL the call names is not at an
allowed site ([BROWSER-22](#BROWSER-22)), or where the browser does not hold the permissions of its
group ([BROWSER-18](#BROWSER-18)). A tool that is on still leaves out and refuses a site the person
has blocked ([BROWSER-17](#BROWSER-17)). A history
search covers all of history, and a search returns at most 100 results. A tool that is off keeps
nothing either: the requests `list_requests` reads are kept only while its switch and its group's are
both on ([BROWSER-25](#BROWSER-25)), and the entries in the browser's context menu that a person holds
something out with exist while the `held` group is on, and keep what is held only while the group and
the switch of `take_held` are both on ([BROWSER-39](#BROWSER-39)).

The settings a version without groups saved still read: the switch of each tool under the key
`tools` and the blocked sites under `blocked_sites` keep their meaning. After the update no group is
on, so a tool whose switch that version left on is refused until the person turns on its group. The
permissions that version required at install may still be held after the update; holding them
turns on no group. Turning a group on turns on all of its tools, including one that version had
off, and the person can turn a single tool off afterwards.

**Why.** Every tool but the platform check reaches what a person has open, has visited or has
saved, or sends their cookies to a site, so none of them does so before the person has said it
may. The platform check tells nothing about them, and is how they see the extension answer before
turning anything on. The
browser's history search covers the last day unless it is given a start time, which would answer a
search of all of history with a day of it.

`verified-by: by-construction (extension/tests/tools.test.mjs asserts the defaults, that a tool turned off reaches none of the browser's APIs, the start time and the bound on results; it runs where the test under BROWSER-13 runs)`
`verified-by: by-construction (extension/tests/upgrade.test.mjs reads settings saved in the shape of the version without groups, with every permission still held, and asserts that the tool switches and blocked sites read as written, that a tool whose switch is on is refused naming its group and reaches no tab, that the platform check answers, that turning a group on runs its tools and still leaves out a blocked site, that its tools are all on, and that a tool of a group still off stays refused; it runs where the test under BROWSER-13 runs)`
`verified-by: by-construction (extension/tests/background.test.mjs asserts that a request is kept only while the switch of the network-watch group and the switch of list_requests are both on, and is forgotten and no longer kept when either is turned off; it runs where the test under BROWSER-13 runs)`
`verified-by: by-construction (extension/tests/background.test.mjs asserts that the entries of the context menu are made as the worker starts while the held group is on and not while it is off, are made when the group is turned on, and hold an item only while it is on; extension/tests/switches.test.mjs asserts that turning the group off, Turn everything off, a preset that does not name it, the end of its lifetime and the permission taken back each forget what was held, remove the entries before the permission goes, and that turning it on forgets what an earlier run left; they run where the test under BROWSER-13 runs)`

<a id="BROWSER-15"></a>
### BROWSER-15: the extension keeps its port open while Brave runs

The extension opens the native messaging port as it starts and as Brave starts, and opens it again
within a minute of losing it. A reply goes back on the port its request came in on, and never on a
port opened since.

**Why.** The port is what keeps the host, and so the socket, alive. An extension that opened it only
when asked would leave no socket for a session to find. A port opened since belongs to another host,
which numbers its requests from 1 as well, so a reply reaching it could be taken for the answer to a
request of its own and handed to a session that never asked for it.

`verified-by: by-construction (extension/tests/background.test.mjs loads the real service worker against fake runtime, alarm, storage, tab and native-port events; it asserts the worker connects at load and on startup and reconnects on the one-minute alarm after a disconnect, and that a request answered after its port closed and another opened reaches only its own port; make check-extension runs it, and check-ui-build depends on that target, so the Front end CI job runs it on every change the classifier gives the ui area, which a change under extension/ is)`

<a id="BROWSER-16"></a>
### BROWSER-16: a tab or window is opened, or a tab moved, at the URL asked for and kept only on the host that was asked for

`open_tab` takes one argument, `url`, which must be an HTTP or HTTPS URL; any other scheme, or no
URL, fails without a tab being created. It opens the URL in a new tab that is not made active,
waits for the tab's top frame to commit, and replies with only the URL it committed. If that URL is
on a different host from the one asked for, or the page fails to load or does not commit within 20
seconds, it closes the tab and fails, naming the URL that was asked for and not the one the tab went
to. The reply holds no page content. Reading what was opened is `read_page` at a URL a person
supplies.

`open_window` takes `url` as `open_tab` does and opens it in a new window that is not brought
forward, under the same checks, and closes the window where `open_tab` closes the tab.
`restore_closed` takes the HTTP or HTTPS `url` of a tab that `list_recently_closed` showed, restores
the one recently closed tab that was at it by the session id the browser gave that tab, and closes
the restored tab where `open_tab` would close its tab. It fails and restores nothing where no closed
tab was at the URL, where more than one was, where the browser gave the tab no id, and for a URL that
only a tab in a closed window was at, since a closed window is not restored.
`navigate_tab` takes `url`, which names an open tab as [BROWSER-13](#BROWSER-13) says, and `to`, an
HTTP or HTTPS URL, and navigates that tab to `to`. `reload_tab`, `go_back` and `go_forward` take
`url` alone, which must be an HTTP or HTTPS URL, and reload that tab or move it one page back or
forward in its history. Each of the four waits as `open_tab` does, for the tab's top frame to commit
or to report a move within the document, to a fragment or by the page's history API, and replies
with only the URL it reached. The page has to be on the host of `to` for `navigate_tab`, and on the
host of `url` for the other three. A page on another host, a failure to load, no commit within 20
seconds, and a commit at a listed site ([BROWSER-17](#BROWSER-17)) make the call fail, naming `url`
for `reload_tab`, `go_back` and `go_forward` and `to` for `navigate_tab`, and not the page the tab
went to. A tab the person had open is not closed for that, since closing it is what `close_tab` does
in `remove-data`. It is moved to `about:blank`, which also ends a load still going. A call that is
refused for a listed site, or for a URL that is not HTTP or HTTPS, moves nothing.

`search_web` takes `query`, text of at most 256 characters, and searches for it with the person's
default search provider. It makes a tab at `about:blank` that is not made active, shows the search in
that tab by its id with `search.query` and no disposition, waits as `open_tab` does, and replies with
only the URL it committed. The commit of the blank page is not the answer. The question shows the
words and not the provider's address, which no API tells the extension, so the page is held to no
host: it has to be a web page and not at a listed site, and commit within 20 seconds, or the tab is
closed and the call fails. `wait_for_load` takes `url`, which names an open tab as
[BROWSER-13](#BROWSER-13) says, and asks the browser for the tab's status every 250 milliseconds, for
at most 20 seconds. It replies with `url` and `loaded`, or `timed out` if the tab was still loading
when the time ran out, and fails, naming the URL asked for, where the tab is closed or is at another
URL when asked. A listed site is refused before a tab is looked for. Neither returns page content.

**Why.** The question before the call shows the URL, and opening it is a request to that site with
the person's cookies. A redirect to another host is one nobody was shown, which is
[FETCH-4](tools/fetch-url.md#FETCH-4) for a tab, and a failure that named where the tab went would
hand the planner a server's bytes as the driver's words, as [FETCH-6](tools/fetch-url.md#FETCH-6)
describes. The reply goes through [MCP-1](mcp.md#MCP-1), so the final URL is untrusted and private.
The question for a tab that is moved shows the URL of the tab and the address, so the address is
the host that was shown. A move back or forward shows only the tab's URL, since the page before or
after it is not one the question can show, and a reload can be redirected, so the host that was
shown is the tab's. A closed tab is named by the URL the person read in `list_recently_closed`, never
by a session id, which says nothing a person can judge, and the browser restores the session closed
last where it is given none. Restoring a window would load every tab in it where the question shows
one URL. A search shows the words, so the host cannot be what was shown, and it is sent to the
provider the person chose, so the checks left are that the page is a web page, is not a listed
site, and loads. `search.query` fails given both a tab and a disposition and opens `NEW_TAB` in the
foreground without naming the tab, which is why the extension makes the tab itself: it can then keep
it in the background and close it ([the design](../design/browser-extension.md)). A wait that
ended with a different page in the tab would say nothing about the page the person was shown.

`verified-by: bravebot_browser::tools::a_tab_is_opened_by_its_url_alone`
`verified-by: bravebot_browser::tools::a_tab_is_moved_by_its_url_and_an_address_and_never_by_an_id`
`verified-by: bravebot_browser::tools::a_search_is_asked_for_by_short_text_and_a_wait_by_a_url`
`verified-by: by-construction (extension/tests/tools.test.mjs asserts that search_web makes one blank background tab, shows the words in it by its id with no disposition, ignores the commit of the blank page, replies with the URL of the page the tab committed for any web page whatever its host, and closes the tab and fails when the page is not a web page, fails to load, is at a listed site, does not commit within twenty seconds and not before, or the browser will not search, that it takes text of one to 256 characters and makes no tab for anything else, and that wait_for_load replies loaded at once for a tab that finished, asks again after each interval until it has, replies timed out as a result after twenty seconds and not before, fails where the tab was closed or went to another URL without naming where, and refuses a listed site before a tab is looked for; it runs where the test under BROWSER-13 runs)`
`verified-by: by-construction (extension/tests/tools.test.mjs asserts that navigate_tab, reload_tab, go_back and go_forward move the one tab at the URL asked for with nothing but the move, and reply with the URL the page committed or the move within the page reported, that a page on another host than the one shown, which for navigate_tab includes the host the tab was on, a look-alike host, a port, a subdomain and a browser error page, a load that fails and one that does not commit within twenty seconds, and not before, each leave the tab at about:blank without closing it and fail naming the URL shown and not where the tab went, and a failure to blank the tab does not change what the call says, that a tab at a URL that is not HTTP or HTTPS is not moved by reload_tab, go_back or go_forward, that go_back and go_forward with no page to go to leave the tab as it was, that only the tab's own top-frame events decide, that open_window opens one window that is not brought forward and closes it, naming the URL asked for, for the same failures and for a browser that does not say its tab, and that restore_closed restores the session id of the one closed tab at the exact URL and no other, restores nothing for a URL that a closed window's tab, several closed tabs or none were at or for a closed tab with no id, and closes the restored tab for the same failures; it runs where the test under BROWSER-13 runs)`
`verified-by: by-construction (extension/tests/tools.test.mjs asserts that open_tab creates one background tab at the URL asked for and replies with the committed URL alone, that a commit on another host, a different port or a browser error page, a load error and a commit that never comes within twenty seconds, and not before, each close the tab and fail naming the URL asked for and not the one the tab went to, that a same-host redirect is kept, that only the tab's own top frame is read, and that a non-web scheme or no URL reaches no tab; it runs where the test under BROWSER-13 runs)`

<a id="BROWSER-17"></a>
### BROWSER-17: a site on the person's blocked list is not listed, read, searched or opened

The extension's options hold a list of sites the person writes, one to a line, kept beside the tool
switches. It starts empty. A site is an HTTP or HTTPS origin, compared as written:
`https://example.com` covers every URL with that scheme, host and port, a `blob:` or `view-source:`
URL of it included, and does not cover `https://www.example.com`, `http://example.com` or another
port. A path after the origin is dropped. A line that is not one HTTP or HTTPS address of a host
stops the save, and the options page names it and leaves the list saved before in force, so a
mistyped line cannot drop the site it replaced. The options page saves the list when the person
leaves the box, since a save on every keystroke would take a site off the list while its line is
edited, and says an edit is not saved yet until then. A call uses the list as it stood when the call
began.

The options page holds a second list beside it, the allowed sites ([BROWSER-22](#BROWSER-22)),
written the same way. It limits which sites a group that reads pages reaches, and widens nothing: a
site on both lists is blocked, and the blocked list holds for every tool whatever the scope of its
group. A group on all sites reaches every site that is not blocked, and a group limited to the
allowed sites reaches the allowed sites that are not blocked. The allowed sites limit no group that
reads no page, so such a group is held by the blocked list alone, except `input-trusted`,
`capture-trusted`, `console-trusted` and `upload-trusted`, which read no host themselves and are limited to the
allowed sites whatever their scope ([BROWSER-18](#BROWSER-18)). The two lists fail in opposite
ways: a blocked list that cannot be read stops every tool that reaches a site, and an allowed list
that cannot be read is an empty one, which reaches no site.

`list_tabs`, `search_history`, `search_bookmarks`, `list_bookmark_folder`, `list_recent_bookmarks`,
`list_reading_list`, `list_top_sites`, `list_recently_closed`, `list_synced_tabs` and `list_downloads`
leave out an entry at a listed site, and `list_tabs` also a tab that is loading one or leaving one.
A download counts as at a listed site when it started at one or ended at one after its redirects. A closed window or a device that has tabs at a listed site keeps its other tabs.
`take_held` leaves out an item held at a listed site, forgets it, and takes nothing where the list
cannot be read ([BROWSER-39](#BROWSER-39)).
`list_frames` leaves out a frame at a listed site and a frame inside one, and `read_page` does not
find such a frame. `read_page_links` leaves out a link at a listed site, and `list_page_elements`
an element whose link or form goes to one. `list_requests` leaves out a
request to a listed site and a request made by a frame at a listed site or inside one.
`list_frames`, `read_page`, `read_page_links`, `read_page_forms`, `list_page_elements`,
`screenshot_page`, `save_page_archive`, `get_visits`, `get_content_settings`, `list_requests`, `focus_tab`, `pin_tab`, `mute_tab`, `move_tab`, `group_tabs`, `ungroup_tabs`, `update_tab_group`,
`discard_tab`, `duplicate_tab`, `set_zoom`, `focus_window`, `resize_window`, `close_tab`,
`close_window`, `create_bookmark`, `move_bookmark`, `rename_bookmark`, `add_reading_list_entry`,
`update_reading_list_entry`, `delete_history_entry`, `remove_bookmark`,
`remove_reading_list_entry`, `delete_downloaded_file`, `download_file`, `pause_download`,
`resume_download`, `cancel_download`, `show_download`,
`open_tab`, `navigate_tab`, `reload_tab`, `go_back`, `go_forward`, `open_window`, `restore_closed`,
`wait_for_load`, `list_cookie_names`, `delete_cookie`, `click_element`, `type_text`, `select_option`,
`press_key`, `scroll_page`, `wait_for_element`, `click_element_trusted`, `type_text_trusted`,
`press_key_trusted`, `hover_element` and `drag_element`
fail when a URL they are given, any one of the list for `group_tabs` and `ungroup_tabs` and the `to`
of `navigate_tab`, is at a listed site, naming the list and not the URL, before the browser is asked
for a tab, a frame, a visit, a history entry, a bookmark, a reading list entry, a download, a closed tab, a window or a cookie, so
the failure does not say whether the site is open or saved. `open_tab`, `open_window`,
`restore_closed` and `search_web` also close a tab or window that commits at a listed site and fail
the same way, and `navigate_tab`, `reload_tab`, `go_back` and `go_forward` leave a tab that commits
at one at `about:blank` ([BROWSER-16](#BROWSER-16)). A cookie belongs to a host and is sent to each
scheme and port of it, so for `list_cookie_names` and `delete_cookie` a listed site covers its host:
`https://bank.example` refuses `http://bank.example/` and `https://bank.example:8443/` as well, and a
cookie whose domain is the host of a listed site is left out of a list and stops a `delete_cookie`
([BROWSER-30](#BROWSER-30)). `close_window` also fails, naming the list and not the site, and closes
nothing, where a tab in the window it would close is at a listed site or loading one.
`delete_downloaded_file` given a filename asks the browser for the download first, since the address
is not known before, and fails the same way, deleting nothing, where the download started at a listed
site or ended at one, and so do `pause_download`, `resume_download`, `cancel_download` and
`show_download`, acting on nothing. `download_file` also cancels a download that ends at a listed
site after its redirects ([BROWSER-32](#BROWSER-32)). `delete_history_range`,
`clear_browsing_data` and `remove_bookmark_folder` name no site and leave none out, and a list that
cannot be read stops them as it stops the others. `remove_bookmark_folder` deletes only a folder that
holds nothing, neither a bookmark nor a folder, so it deletes no bookmark at a listed site or any
other. While the list is not empty, a history search asks the browser for the 1,000 newest matches
and returns the first of the others up to the count asked for, so leaving entries out does not
return fewer than there are. A stored blocked list that is not a list of strings fails every tool
that reaches a site, and the options page says it cannot read it until the next save replaces it.
The tools that reach no site do not read the list and still answer, as the platform check does:
`list_windows`, `list_tab_groups`, `create_bookmark_folder`, and the tools of `browser-info` other
than `get_content_settings`.

**Why.** Turning a tool on makes every open page, history entry and bookmark reachable by any call a
person approves, and a standing allow rule removes the question before the call. A person cannot
exclude a bank, a mail client or a password manager once and for all except by leaving the tool off.
The list is their own setting and a second control beside [BROWSER-14](#BROWSER-14), not a
replacement for it. The allowed sites narrow one kind of group and the blocked list removes a site
from every group. A site on both is blocked, since the extension keeps no order between the two
edits and the block is the one that takes access away. A tool that reaches no site has no site to
leave out, so an unreadable list leaves it as it was. The comparison is made in the extension, which
is outside the driver, between the person's list and a URL, and the driver reads no part of a result to make it. The refusal comes
before any tab is looked for so that it cannot answer whether a page is open, and it names the
list because it goes back to the session. What [VIEW-3](terminal-transcript.md#VIEW-3) shows the
person is what the extension returns, so a listing with entries left out holds fewer rows than
Brave has, and the options page says so. A list that could not be read and was taken for an empty
one would leave every site open and tell nobody.

`verified-by: by-construction (extension/tests/tools.test.mjs asserts that the tools list, search, read and open nothing at a listed origin, over a table of URLs that are at it and URLs that only resemble it, that closed tabs, the most visited sites and the tabs of other devices at a listed origin are left out while a window or device keeps its other tabs and that the visits to a URL at one and the content settings for a URL at one are refused before the browser is asked, that a tab, a history entry and a bookmark at one are not closed or deleted, that a window with a tab at one or loading one is not closed while another window is, that bookmarks in a folder, the newest bookmarks and reading list entries at a listed origin are left out, that the newest bookmarks are not shortened by the entries left out, that the tools that add or change a bookmark or a reading list entry refuse a URL at a listed origin before the browser is asked for a bookmark or an entry, that the requests of a tab to a listed site and by a frame at one or inside one are left out and a tab at one is refused, that a tab loading a listed site, a frame at one or inside one in an open tab and the first matches of a history or bookmark search at one are left out without shortening the result, that the refusals are one message naming the list and not the URL and reach no tab, frame or script API, that a tab redirected to a listed site is closed, or for a tab that was open left at about:blank, with the same message, that the url and the to of navigate_tab at a listed site reach no tab, that the cookie tools refuse a URL at the host of a listed site whatever its scheme or port, leave out a cookie whose domain is that host, refuse to delete one, and do not take a host that only resembles it, that a page a search commits at a listed site is closed, that a list that cannot be read refuses every tool that reaches a site while the platform check still answers, that every such tool, given a listed site open in a tab, in history and in bookmarks, returns nothing of it and reaches no tab, frame or script, that a line that is not one address of a host, a wildcard or two sites on a line included, stops the save and leaves the list saved before in force, and that what the options page saves is what the tools read; extension/tests/options.test.mjs asserts that the page's field shows the saved list, saves nothing while typing and says so, saves on leaving the box, names a line that is not an address and keeps the list saved before, reports a save that fails, does not replace text typed during a save, says when the stored list cannot be read, and saves a switch without replacing the list; both run where the test under BROWSER-13 runs)`
`verified-by: by-construction (extension/tests/tools.test.mjs asserts that a site on both lists is blocked with the refusal that names the list and reaches no tab, that a group on all sites reaches every site that is not blocked and refuses one that is, that the allowed sites limit only the groups that read pages, that an allowed list that cannot be read reaches no site, and that the tools that reach no site answer when the blocked list cannot be read; it runs where the test under BROWSER-13 runs)`
`verified-by: by-construction (extension/tests/trusted.test.mjs asserts that the five tools of input-trusted send nothing for a site on the blocked list, a site that is not allowed or a page that is not on the web, and extension/tests/tools.test.mjs holds a row naming a blocked site for each of them, with the site also allowed so that the blocked list is what refuses it; both run where the test under BROWSER-13 runs)`
`verified-by: by-construction (extension/tests/tools.test.mjs asserts that take_held, among every tool, returns nothing of a blocked site and reaches no tab, that it leaves out and forgets what was held at one and returns the rest, and that it takes nothing where the blocked list cannot be read; it runs where the test under BROWSER-13 runs)`

<a id="BROWSER-18"></a>
### BROWSER-18: each tool is in one group, and runs only while the browser holds its group's permissions

The extension's tools are in groups, listed in one table in `extension/groups.js`, a line to a group:
`platform`, `tabs-read`, `history-read`, `bookmarks-read`, `page-read`, `tabs-manage`,
`bookmarks-write`, `navigate`, `downloads`, `cookies`, `network-watch`, `browser-info`,
`remove-data`, `scripts`, `input`, `input-trusted`, `capture-trusted`, `console-trusted`, `upload-trusted`, `screen` and `held`. A tool is in exactly one group. A group lists the
browser permissions its tools call and whether they read pages, which needs hosts: those of the
allowed sites, or `https://*/*` and `http://*/*`, or for `page-read` and `screen`, whose
`screenshot_page` and `record_gif` need it, `<all_urls>` ([BROWSER-22](#BROWSER-22),
[BROWSER-35](#BROWSER-35), [BROWSER-42](#BROWSER-42)). It lists only what its tools call, so a group
with no tool yet lists none. `run_script` is in `scripts`, which lists `userScripts`, reads pages and
carries `allowedOnly` ([BROWSER-46](#BROWSER-46)).
`click_element`, `type_text`, `select_option`, `press_key`,
`scroll_page` and `wait_for_element` are in `input`, which lists `tabs` and `scripting`, reads pages
and is in no preset ([BROWSER-33](#BROWSER-33)). Its server is declared to ask about every call
([BROWSER-21](#BROWSER-21)). `click_element_trusted`, `type_text_trusted`, `press_key_trusted`,
`hover_element` and `drag_element` are in `input-trusted`, which follows `input` in the table. It
lists no permission and reads no host itself, since a second extension holds the browser's
`debugger` and does what its tools ask ([BROWSER-37](#BROWSER-37), [BROWSER-38](#BROWSER-38)), and
this extension sends each call to it ([BROWSER-12](#BROWSER-12)). The line of the group carries
`allowedOnly`, which limits it to the allowed sites whatever the scope: a call whose URL is not at
an allowed site is refused as for a group that reads pages and is limited to them
([BROWSER-22](#BROWSER-22)), and the options page shows no choice of all sites for it. Its server is
declared to ask about every call ([BROWSER-21](#BROWSER-21)), and the group is in no preset
([BROWSER-20](#BROWSER-20)). `read_console` is in `console-trusted`, which follows `capture-trusted`
in the table and is shaped as `input-trusted` is: it lists no permission and reads no host itself, since the second
extension reads the page's console ([BROWSER-44](#BROWSER-44)), its line carries `allowedOnly`, its
server is declared to ask about every call ([BROWSER-21](#BROWSER-21)) and the group is in no preset
([BROWSER-20](#BROWSER-20)). It is a group of its own and not a sixth tool of `input-trusted`, because it
sends no input and reads what the page's scripts printed, so turning it on is described as that and
not as clicks and typing. `choose_file` is in `upload-trusted`, which follows `console-trusted` in the table and is
shaped the same way: no permission, no host, `allowedOnly`, a server declared to ask about every call and no
preset ([BROWSER-45](#BROWSER-45)). It is a group of its own because it hands a file of the person's to a
page, so turning it on is described as that and not as clicks, pictures or a console.
`capture_screen` and `record_gif` are in `screen`, which lists `desktopCapture` and `tabs`, reads
pages for `record_gif` and is in no preset ([BROWSER-42](#BROWSER-42)). Its server is declared to ask
about every call ([BROWSER-21](#BROWSER-21)). `take_held` is in `held`, which lists `contextMenus`, reads no page and
lists no host, and is in no preset ([BROWSER-39](#BROWSER-39)). Brave shows no warning for
`contextMenus`.
`get_platform_info` is the `platform` group's and needs nothing. `list_tabs`, `list_frames`,
`list_windows`, `list_tab_groups` and `list_recently_closed` are in `tabs-read`, which lists `tabs`,
`tabGroups`, `webNavigation` and `sessions`. `read_page`, `read_page_links`, `read_page_forms`,
`list_page_elements` and `screenshot_page` are in `page-read`, which lists `tabs`,
`scripting` and `webNavigation` and reads pages. `focus_tab`,
`pin_tab`, `mute_tab`, `move_tab`, `group_tabs`, `ungroup_tabs`, `update_tab_group`, `discard_tab`,
`duplicate_tab`, `set_zoom`, `focus_window` and `resize_window` are in `tabs-manage`, which lists `tabs` and `tabGroups`. `search_history`, `get_visits`, `list_top_sites`
and `list_synced_tabs` are in `history-read`, which lists `history`, `topSites` and `sessions`.
`search_bookmarks`, `list_bookmark_folder`, `list_recent_bookmarks` and `list_reading_list` are in
`bookmarks-read`, which lists `bookmarks` and `readingList`. `create_bookmark`,
`create_bookmark_folder`, `move_bookmark`, `rename_bookmark`, `add_reading_list_entry` and
`update_reading_list_entry` are in `bookmarks-write`, which lists the same two permissions and
reads no page. `open_tab`, `navigate_tab`, `reload_tab`, `go_back`, `go_forward`, `open_window` and
`restore_closed`, `search_web` and `wait_for_load` are in `navigate`, which lists `tabs`, `sessions`,
`search` and `webNavigation`, reads no page, and is not limited to the allowed sites. `list_cookie_names` and `delete_cookie` are in `cookies`, which lists `cookies` and reads
pages, so it needs the hosts as `page-read` does ([BROWSER-30](#BROWSER-30)).
`list_requests` is in `network-watch`, which lists `webRequest` and reads pages, so it needs the
hosts as `page-read` does. `get_privacy_settings`, `get_content_settings`, `get_font_settings`,
`get_system_info`, `get_idle_state`, `get_access_flags` and `keep_awake` are in `browser-info`, which
lists `privacy`, `contentSettings`, `fontSettings`, `system.cpu`, `system.memory`, `system.display`,
`system.storage`, `idle` and `power` and reads no page. `get_access_flags` calls
`extension.isAllowedFileSchemeAccess` and `extension.isAllowedIncognitoAccess`, which need no
permission. Brave asks the person to allow `privacy`, `contentSettings` and `system.storage`, and
shows no warning for the other six. Brave asks to read and change the bookmarks, and the reading
list, for the reading and the changing alike, so the browser cannot tell `bookmarks-read` from
`bookmarks-write`, and the difference between them is the tool switches and the group switches.
For `navigate` Brave asks one line, to read the browsing history on all signed-in devices, for
`tabs` with `sessions`, and `webNavigation` adds none. It shows no warning for `search` or
`cookies`, and for `cookies` shows the line for the hosts. The wording the options page shows for
each permission and for the hosts is Brave 1.98's, held to the design document by
`extension/tests/warnings.test.mjs`. Brave merges the warnings of permissions asked for together, so
the page shows the lines Brave shows for all of a group's permissions and hosts at once, which can
be fewer than the group has permissions: `history` stands for every other permission about the
browsing history, `tabs` stands for `webNavigation`, and access to every site leaves out the lines
about reading the history except that of `history`. A preset shows the lines for the union of its
groups.
`close_tab`, `close_window`, `delete_history_entry`,
`delete_history_range`, `clear_browsing_data`, `remove_bookmark`, `remove_bookmark_folder`,
`remove_reading_list_entry` and `delete_downloaded_file` are in `remove-data`, which lists `tabs`,
`history`, `bookmarks`, `readingList`, `browsingData` and `downloads` and reads no page. Brave asks
four lines: to read and change the browsing history on all signed-in devices, which stands for
`tabs` as well, to read and change the bookmarks, to read and change entries in the reading list,
and to manage the downloads. It shows no warning for `browsingData`.
`save_page_archive`, `download_file`, `list_downloads`, `pause_download`, `resume_download`,
`cancel_download` and `show_download` are in `downloads`, which lists `downloads`, `pageCapture` and
`tabs`, reads no host and has no scope, and Brave asks the person to allow it to manage their
downloads and to read and change all their data on all websites, which is its line for
`pageCapture`, and shows no line for `tabs` beside it ([BROWSER-40](#BROWSER-40)). `tabs` is listed
because `save_page_archive` finds its tab by its address, which the browser leaves out of every tab
for an extension that holds neither `tabs` nor a host covering the tab. `save_page_archive` is
limited to the allowed sites by name ([BROWSER-22](#BROWSER-22)).

The manifest requires `nativeMessaging`, `storage` and `alarms`. Every other permission a group
lists is an optional permission, the two host patterns and `<all_urls>` are optional host
permissions, and the manifest asks for no host at install. A fresh install holds none of what a group lists, so the
browser refuses the APIs of a group nobody turned on. `minimum_chrome_version` is 116 or the first
Chrome version with the API behind a permission a group lists, whichever is newer, so it is raised
only as far as a group with a tool needs. `readingList`, which `bookmarks-read` lists, is in Chrome
120 and no earlier. `userScripts`, which `scripts` lists for `userScripts.execute`, is usable for a
script run at once in Chrome 135 and no earlier, so `minimum_chrome_version` is 135. The newest API `browser-info` calls is
`idle.queryState` as a promise, in Chrome 116, so that group raises nothing. `page-read` calls
`tabs.captureVisibleTab` as a promise, in Chrome 88, and raises nothing either, and `held` calls
`contextMenus.create`, `removeAll` and `onClicked`, which a Manifest V3 service worker has from Chrome
88, and raises nothing. The calls `navigate`
adds are `tabs.goBack` and `tabs.goForward`, in Chrome 72 and as promises in 88, so it raises
nothing either, and the promise forms `remove-data` and `downloads` call are in Chrome 96 or, for `readingList`, Chrome 120. A browser older than
that does not install the extension, so no group is refused on one. The permissions that Chrome says
cannot be optional, `debugger`, `declarativeNetRequest`, `devtools`, `geolocation`, `mdns`, `proxy`,
`tts`, `ttsEngine` and `wallpaper`, are in no list of the manifest and in no group. Only `debugger`
could serve a group. `input` acts through functions the extension injects with `scripting` and does
not use it ([BROWSER-33](#BROWSER-33)), and `input-trusted` is served by a second extension that
declares it ([BROWSER-37](#BROWSER-37)). `debugger` is declared by the second extension and never
by this one.

A tool runs only while its switch is on. A tool other than the platform check also runs only while
its group's switch is on, the group's lifetime ([BROWSER-24](#BROWSER-24)) has not ended, and the
browser holds every permission and host its group lists. The browser is asked when the call arrives,
and the answer is not kept for the next call, so a permission the person takes back in the browser's
extension settings is not held at the next call and the tools of every group that lists it are
refused from then on. The service worker also turns those groups off ([BROWSER-19](#BROWSER-19)). A
tool in no group, and a tool whose permission check fails or does not answer true, is refused as
well. Each refusal comes before any other API is reached and names the tool,
and the ones for a group that is off, a lifetime that has ended and a group not held name the
group. A permission is granted by name, so what is
taken back when a group is turned off is only what no group still on lists, and the difference
between two groups that list the same permission is the tool switches and the group switches.

**Why.** With every permission and host required at install, a tool that was off would be the
extension's own check and the browser would enforce nothing. With optional permissions the
browser holds what the person granted, and a permission they take back in the browser's extension
settings ends the tools that need it at their next call. The group's switch is checked as well
because the browser grants by permission: a tool switch that is on while its group is off, or a
permission another group was granted, must not run a tool whose group is off. A tool in two groups would run when either
was held, and a tool in none would have no permission to check. The browser cannot tell two groups
apart that list the same permission, so a permission one group still needs is not taken from it.

`verified-by: bravebot_browser::extension::the_extension_has_each_permission_its_tools_require`
`verified-by: bravebot_browser::extension::the_group_table_lists_the_groups_and_each_tool_in_one`
`verified-by: bravebot_browser::extension::the_extension_answers_every_tool_the_server_offers_and_no_other`
`verified-by: bravebot_browser::extension::the_relays_group_table_is_the_one_the_extension_holds`
`verified-by: by-construction (extension/tests/input.test.mjs asserts that each tool of input is refused before a tab or a page is reached when its tool switch is off, its group is off, its lifetime has ended, or the browser lacks scripting, tabs or the hosts the group lists; it runs where the test under BROWSER-13 runs)`
`verified-by: by-construction (extension/tests/warnings.test.mjs asserts that the warning the page lists for each optional permission is the one Brave 1.98 gave, that the lines the page lists for each group, for each preset with no host, one site and every site, for every combination of the five permissions about the browsing history and for access to every site are the lines Brave gave for the permissions asked together, that every permission the manifest offers has an entry, that a host line names the hosts sorted, once each, without a scheme or port, up to three and then a number of websites, and that the design document quotes each line; it runs where the test under BROWSER-13 runs)`
`verified-by: by-construction (extension/tests/groups.test.mjs asserts the twenty-one groups in order, that each tool is in one group and the tools are in the groups named here, that only the platform group's tools start on and that group needs nothing, what each group needs of the browser as a copy, including the two permissions of bookmarks-write, the four of navigate and the permission and hosts of cookies, that held lists contextMenus and no host, that downloads lists pageCapture and tabs with downloads and no host, that the Read and Browse presets ask for the same permissions as before, that save_page_archive is the one tool limited to the allowed sites by name and no preset asks for pageCapture, and what turning a group off takes back, over a table in which two groups list one permission or one host, and that bookmarks-read and bookmarks-write keep their two permissions for each other, and that none of the nine permissions that cannot be optional is in the manifest, required or optional, or in a group, and that input lists none and has no tool, and that input-trusted lists no permission, reads no host, is the one group with allowedOnly and needs nothing from the browser whatever sites are allowed; extension/tests/tools.test.mjs asserts that each tool is refused, naming its group, when any one permission or host of its group is missing and runs when exactly those are held, that a fresh install answers only the platform check, that a permission taken back between two calls refuses the second, that a check that fails or does not answer true refuses, and that a tool in no group is refused; both run where the test under BROWSER-13 runs)`
`verified-by: by-construction (extension/tests/trusted.test.mjs asserts that a call to a tool of input-trusted is refused, and nothing is sent to the second extension, while its group is off, its tool is off, its lifetime has ended or its site is not allowed, and that a scope of all sites stored for the group, which the options page never writes, or an allowed list that cannot be read does not let a call reach a site nobody allowed; extension/tests/warnings.test.mjs asserts that the group shows no line from Brave with sites allowed or none; both run where the test under BROWSER-13 runs)`
`verified-by: by-construction (extension/tests/upgrade.test.mjs asserts that a tool whose switch is on, with every permission held through another group, is refused naming its group while that group's switch is off and runs once it is on, and that minimum_chrome_version equals the newest of 116 and the first Chrome version of each permission a group lists, a permission with no version listed failing the test; it runs where the test under BROWSER-13 runs)`

<a id="BROWSER-19"></a>
### BROWSER-19: a group is turned on in the options page, after the person confirms and the browser grants its permissions

The options page lists each group that has a tool, in the order of the table. Each has a switch, a
line on what a session could then see or do, and what Brave will ask the person to allow for the
permissions and hosts the group lists. Only the platform group's switch is on at first, and it
cannot be turned off. The switches of a group's tools are inside the group and can be changed only
while the group is on.

Turning a group's switch on does not ask the browser. The switch stays off, and the page says what
the group lets a session do and what comes back, with a button to turn it on and one to cancel.
The click on that button calls `permissions.request` for what the group lists as the first thing it
does, from the page and not from the service worker; for a group that lists nothing, `input-trusted`,
`capture-trusted`, `console-trusted` or `upload-trusted`, there is nothing to request and the group is saved on at the click. When the browser grants it, the group's switch,
its lifetime ([BROWSER-24](#BROWSER-24)) and the switches of its tools are saved on together. A request the person declines, or one the
browser refuses to put, such as one an organization blocks, leaves the group and its tools off and
the page says why. Cancelling asks the browser for nothing.

When the browser refuses the request with "Permissions are blocked by enterprise policy.", which is
what the `blocked_permissions` key of an organization's `ExtensionSettings` policy makes it do, the
group's row says it is blocked by the organization, its switch is disabled and stays off, and its
confirmation closes. The page does not ask again until it is opened again. A request that fails with
any other message is shown with that message and the group is not drawn as blocked. The README of
the extension names the two policy keys, `blocked_permissions` and `runtime_blocked_hosts`.

Turning a group off saves the group and its tools off, and then calls `permissions.remove` for the
permissions and hosts that no other group whose switch is on lists, or for nothing where there are
none. A group counts as on in the page, and its tools can be changed, only while its switch is
saved and the browser holds everything it lists. When a permission is taken back in the browser's
extension settings, the service worker turns off every group whose switch is on and that lists it,
with the group's tools, and takes back what only those groups listed. It does the same check at
each start of the browser. A check that fails turns nothing off. The page draws its switches again
when the browser reports a permission added or removed and when the saved switches change.

The functions that read the saved switches, tool switches, lifetimes or scopes and write them back
(turning a group on or off, a preset, a tool switch, a scope, and the worker's checks for a lifetime
that ended and for a permission taken back) run one after another across the options page, the
popup and the service worker, and each reads what is saved when its turn comes. Each context loads
a copy of the code, so each function takes the lock named `bravebot-switches` of the browser's Web
Locks (`navigator.locks`) for the whole of its read and write, and the contexts share one line. A
context with no `navigator.locks`, or whose browser refuses the lock before the function starts,
runs its functions one after another among themselves without the lock, and they can overlap those
of another context. The request to the browser comes before a function waits for its turn, since
the browser takes it only from a click. A function that fails releases the lock and does not stop
the ones after it.

The badge on the toolbar action counts the groups that are on, other than the platform group, by
the test the page draws its switches by: the person turned the group on, its lifetime has not ended
and the browser holds what it lists. The badge is empty when none is on. The title of the action
says how many groups are on and lists them by id, or says that no group is on. The service worker
draws the badge as it starts, when the saved switches, lifetimes, allowed sites or scopes change,
and when the browser reports a permission added or removed. A call added to the activity log
([BROWSER-27](#BROWSER-27)) does not redraw it. A read that fails leaves the badge as it was and is
reported. A group whose hour has passed is counted until the worker turns it off, which is within a
minute ([BROWSER-24](#BROWSER-24)).

**Why.** The browser takes a request for an optional permission only from a click, and the service
worker has none. Brave shows no warning for some permissions, such as `scripting`, so the page's own
words are the only confirmation for a group that lists only those. The page shows them for every
group, so the confirmation does not depend on which permissions a group lists. The browser grants by permission, so turning off a group that
shares a permission with one still on must not take that permission back. A tool switch left on
after its group went would run again as soon as another group was granted the same permission, so
the tools go off with the group, and also when the permission is taken back and no page is open. A
saved switch is not a grant, so the page does not draw a group as on from the switch alone. A
person who declined can turn the group on again, and an organization's policy gives the same
refusal to every request, so a line that only says the group was not turned on invites a click that
can only fail the same way, and naming the organization says who to ask. The badge
shows what a session could reach without the person opening the options page. The platform group is
on in every install, so counting it would put a badge on an extension nobody turned anything on in.
The log changes with every call and the badge does not, and a redraw asks the browser what it holds.
Two writers that read the saved switches together would each write back a copy that predates the
other's write, and the later write would turn a group back on, with its tools, that the earlier one
turned off. The worker starts two such checks at once when the browser starts, and the popup's Turn
everything off can arrive while the worker is writing. The contexts are separate JavaScript
contexts that share no variable, so only a lock the browser holds for all of them orders the
writes.

`verified-by: by-construction (extension/tests/background.test.mjs asserts that the badge counts the groups that are on and its title lists them, that it is empty with none on and does not count the platform group, that a group whose hour has passed or whose permission is not held is not counted, that it follows a switch, a lifetime, an allowed site and a permission added or taken back, that a call added to the log or a tool switched does not redraw it, and that a read that fails leaves it and is reported; extension/tests/switches.test.mjs asserts the groups that are on; it runs where the test under BROWSER-13 runs)`
`verified-by: by-construction (extension/tests/options.test.mjs asserts that the page draws each group that has a tool with its risk, the browser's warning and a switch for each tool, that turning a switch on asks the browser for nothing and shows what the group does and returns, that confirming makes the browser request as the first call of that click with exactly the group's permissions and hosts and then saves the group and only its tools on, that a declined request and a cancelled confirmation leave everything off, that a request an organization's policy blocks draws the group as blocked by the organization with its switch off and disabled, its confirmation closed and no second request from the page, that a request failing with another message is shown with it and the group is not drawn as blocked, that turning a group off keeps the permissions another group on lists, and that the page follows a permission added or removed and a switch another page saved; extension/tests/switches.test.mjs asserts the same rules without the page, over groups that share permissions, that the switches are saved off before a permission is taken back, that a check that fails turns nothing off, that a group is drawn as off when the browser cannot be asked what it holds, that overlapping turn-offs from the worker's two checks, from a turn-off, Turn everything off and a tool switch leave every group and tool as the last of them set it whichever order the storage answers in, in a context with no locks, and in the worker, the popup and the options page, each with its own copy of the code, with one lock name that is never held twice at once, that a writer that fails does not stop the next with locks and without, and that a browser that refuses the lock before a writer starts still runs the writer once; extension/tests/background.test.mjs asserts that the worker turns off the groups and tools of a permission taken back, at the event and at each start; all run where the test under BROWSER-13 runs)`

<a id="BROWSER-20"></a>
### BROWSER-20: a preset turns on a set of groups with one request to the browser, and turns off the others

The options page offers four presets, listed in `extension/groups.js` a line to a preset: `off`
names the `platform` group, `look` names `tabs-read`, `history-read` and `bookmarks-read`, `read`
names those and `page-read`, and `browse` names those, `tabs-manage`, `bookmarks-write` and
`navigate`. A preset names only groups that have a tool. `bravebot-browser install --preset` names
the same groups for the same preset names. The two permissions `bookmarks-write` lists are those
`bookmarks-read` lists, so the one request for `browse` is the one for `read`. `remove-data`, `cookies`, `downloads`, `network-watch`, `browser-info`, `scripts`,
`input`, `input-trusted`, `capture-trusted`, `console-trusted`, `upload-trusted`, `screen` and `held` are in no preset and are turned on one at a time with their own switches.

Choosing a preset other than `off` does not ask the browser. The page says, for each group the
preset turns on, what it lets a session do and what comes back, with what Brave will ask the person
to allow for all of them together and the groups the preset will turn off, and has a button to turn
the preset on and one to cancel. The page draws the confirmation again when the allowed sites, the
scopes or the switches change before the person answers, so the lines and the groups it names are
those the request is made for. The click on that button calls `permissions.request` once, as the
first thing it does, from the page, for the union of what the groups list, each permission and host
once. When the browser grants it, the preset's groups and the switches of their tools are saved on,
every other group and its tools are saved off, in one write, and then the permissions that no group
left on lists are taken back. A group that was on before keeps the tool switches and the lifetime
the person set, unless its lifetime has ended ([BROWSER-24](#BROWSER-24)); a group that is turned on is
given the default lifetime of its group. A
request the person declines, or one the browser refuses to put, saves nothing. Cancelling asks the
browser for nothing.

A group that reads pages and that the preset turns on is limited to the allowed sites
([BROWSER-22](#BROWSER-22)), whatever scope it had. The request is for the hosts of the allowed
sites, the scope is saved in the same write as the switches, and the access to all sites that the
group held is taken back unless a group left on lists it. The confirmation says so for a group that
was on all sites.

`off` turns off every group at once, takes back every optional permission and host the extension
holds, including the sites that saving the allowed sites granted while no group was on, asks the
browser for nothing, and needs no confirmation. The allowed sites stay listed.

The toolbar action opens a popup that lists the groups that are on and has a button, **Turn
everything off**. The manifest also declares a keyboard command, `turn-everything-off`, with no
default key, since a default key can collide with one the person or Brave already uses. The person
gives it a key in the browser's shortcut settings. The button, from the click on it in the popup, and
the command, in the service worker, each call `turnEverythingOff` in `extension/switches.js`, which
applies `off`: the switches are saved off, a `keep_awake` request is released before `power` is taken
back ([BROWSER-26](#BROWSER-26)), and every optional permission and host is given back. Neither asks
the browser for anything. The popup says that the groups are off, or the error the browser reported
once the switches were saved, and a command that fails is reported.

**Why.** A preset is the set of groups it names, so one that leaves the groups it does not name on
would not mean what its name says, and turning groups off only narrows what a session can reach. The
browser shows one prompt for one request, so a set of groups is allowed once and the prompt lists
every warning together. The page still says what each group does, as for a single group
([BROWSER-19](#BROWSER-19)), since Brave shows no warning for some permissions and the person
chooses a set of groups there. A group that deletes data, reads cookies or acts on a page is a
choice of its own, so no preset includes one. Read and Browse are page reading on the allowed sites,
so a scope of all sites chosen earlier does not carry into them, and a person who has chosen Off
holds nothing the extension could use again without asking, whether or not a group was on. The popup
and the command share the function of `off` so that a person who ends everything from the toolbar
has the permission sharing and the `keep_awake` release the options page has, and the popup acts from
the click in its own page and does not depend on the service worker being awake.

`verified-by: bravebot_browser::extension::the_relays_presets_are_the_ones_the_extension_holds`
`verified-by: bravebot_browser::extension::the_toolbar_action_opens_a_popup_and_the_keyboard_command_has_no_default_key`
`verified-by: by-construction (extension/tests/popup.test.mjs asserts that the popup lists the groups that are on, that its button turns every group and tool off from the click with no service worker, takes back every optional permission and host and asks the browser for nothing, ends a keep_awake request before power is taken back, says an error the browser reports with the groups already off, and is disabled while it works; extension/tests/background.test.mjs asserts that the keyboard command does the same, that another command does nothing, and that a failure is reported with the groups off; extension/tests/switches.test.mjs asserts the same for the function both call; all run where the test under BROWSER-13 runs)`
`verified-by: by-construction (extension/tests/groups.test.mjs asserts the four presets and the groups each names, that Read is Look and one group and Browse is Read and three groups, that every group a preset names has a tool, that the groups turned on one at a time are in no preset, and that what several groups need lists each permission and host once and Browse asks the browser for what Read does; extension/tests/switches.test.mjs asserts that a preset requests the union as its first call and once, turns on its groups and their tools, turns off the others and takes back only what no group left on lists, saves the switches before taking anything back, leaves the tool switches of a group already on, changes nothing when the request is declined or refused, asks for nothing for off and takes back every optional permission and host held, the sites saving the allowed sites granted included and a permission the manifest requires not, limits the page-reading group of Read and Browse to the allowed sites in the request, the saved scopes and the hosts given back, and refuses a name that is not a preset; extension/tests/options.test.mjs asserts that the page offers the four presets, that choosing one shows each group's words, Brave's warnings once, the groups to be turned off and a group that was on all sites being limited, and asks the browser for nothing, that the confirmation is drawn again when the allowed sites or the switches change before it is answered and the request is for what it then shows, that confirming makes the one request as the first call of the click and then turns the groups on, that off applies at once, that a declined or refused request and a cancelled confirmation change nothing, and that a permission the browser would not take back is said; all run where the test under BROWSER-13 runs)`

<a id="BROWSER-21"></a>
### BROWSER-21: a server is started for one group and offers that group's tools and no others

`bravebot-browser mcp --group <id>` offers the tools of one group of [BROWSER-18](#BROWSER-18) and
no others, and the table in `crates/browser/src/tools.rs` says which tools each group has. That table
lists the groups and tools of `extension/groups.js`, no more and no fewer. The list
is the same whether or not the extension is connected, and a group with no tool yet offers an empty
list. `bravebot-browser mcp` with no group offers the seven tools of [BROWSER-10](#BROWSER-10). A
call naming a tool the server does not offer, a tool of another group included, is refused by the
server and nothing is sent to the extension. A call naming a tool it offers is sent, and the
extension refuses it while the tool's switch is off, while its group's switch is off, once the
group's lifetime has ended ([BROWSER-24](#BROWSER-24)), while the browser does not hold the
group's permissions ([BROWSER-18](#BROWSER-18)), for a group that reads pages, and for
`input-trusted`, `capture-trusted`, `console-trusted` and `upload-trusted`, when the URL is not at an allowed site ([BROWSER-22](#BROWSER-22)), and at a blocked site
([BROWSER-17](#BROWSER-17)); [BROWSER-14](#BROWSER-14) lists these refusals. Each is the call's
failure, as the extension worded it. A word after `mcp` other than `--group` and the id of one group is answered with the
usage, and no server starts.

`install --groups <id>,<id>` and `install --preset <name>` print, after the manifest is written, one
declaration for each group named, once each and in the order given:

```
bravebot mcp add brave-<id> -s user --dir <directory> -- <path to>/bravebot-browser mcp --group <id>
```

For `remove-data`, `cookies`, `scripts`, `input`, `input-trusted`, `capture-trusted`, `console-trusted`, `upload-trusted` and `screen` the declaration also has
`--ask-every-call` before the `--`, so that every call to a tool of any of the nine servers is
asked about whatever a rule or an earlier answer says ([SERVERS-21](mcp-servers.md#SERVERS-21)), and the line is

```
bravebot mcp add brave-<id> -s user --ask-every-call --dir <directory> -- <path to>/bravebot-browser mcp --group <id>
```

For `scripts`, `input`, `input-trusted`, `capture-trusted`, `console-trusted`, `upload-trusted` and `screen`, whose actions the person also answers in a window or a picker of the browser,
the line also has `--tool-timeout 120`, which `bravebot mcp add` takes in seconds from 1 to 3600
([SERVERS-15](mcp-servers.md#SERVERS-15)), and is

```
bravebot mcp add brave-scripts -s user --ask-every-call --tool-timeout 120 --dir <directory> -- <path to>/bravebot-browser mcp --group scripts
bravebot mcp add brave-input -s user --ask-every-call --tool-timeout 120 --choice click_element:element_number --choice type_text:element_number --choice select_option:element_number --dir <directory> -- <path to>/bravebot-browser mcp --group input
bravebot mcp add brave-input-trusted -s user --ask-every-call --tool-timeout 120 --dir <directory> -- <path to>/bravebot-browser mcp --group input-trusted
bravebot mcp add brave-capture-trusted -s user --ask-every-call --tool-timeout 120 --dir <directory> -- <path to>/bravebot-browser mcp --group capture-trusted
bravebot mcp add brave-console-trusted -s user --ask-every-call --tool-timeout 120 --dir <directory> -- <path to>/bravebot-browser mcp --group console-trusted
bravebot mcp add brave-upload-trusted -s user --ask-every-call --tool-timeout 120 --dir <directory> -- <path to>/bravebot-browser mcp --group upload-trusted
bravebot mcp add brave-screen -s user --ask-every-call --tool-timeout 120 --dir <directory> -- <path to>/bravebot-browser mcp --group screen
```

The server waits 90 seconds for the extension's reply to a call ([BROWSER-6](#BROWSER-6)), so the
agent's bound of 120 is above it, and the agent does not stop the server for a call that waited for
the person.

The line for `input` also has a `--choice` for each of its tools that takes `element_number`,
`click_element`, `type_text` and `select_option`, after `--tool-timeout 120` and before `--dir`. They
declare that argument as one a choice may fill, and `bravebot mcp add` takes them because the line
also has `--ask-every-call` ([SERVERS-22](mcp-servers.md#SERVERS-22)). No other line has one,
`input-trusted` included, whose tools take a role and a name. The relay links no workspace crate, so
the pairs are written out in `crates/browser/src/install.rs`, and a test holds them to the tools whose
schema has `element_number`.

The declaration for `held` is the plain one, with no `--ask-every-call`. `take_held` takes no
argument, so the question before a call holds the alias and the tool and nothing a session chose, and
what it returns is what the person held out and no more. A rule, a standing answer or a mode answers
it as it answers `read_page`, and a person who wants every fetch put to them can add the flag to the
declaration ([BROWSER-39](#BROWSER-39)).

The directory and the program path are written for a POSIX shell. A name made of letters, digits
and `_ - . / : , + @` is printed as it is, and any other is in single quotes with each `'` written
`'\''`, so a name with a space, a quote, `$`, a backtick, a `;`, a newline or a `~` at the start
pastes as the one word it is, and `bravebot mcp add` declares the directory and program that were
installed. A name that is not UTF-8 is printed as the bytes it has. The lines are for a POSIX
shell. On Windows `bravebot-browser` only prints that the platform is not supported and exits, so no server
starts and neither half runs.

The relay's table in `crates/browser/src/tools.rs` says which groups these are, and the table in
`extension/groups.js` has no such field. The presets are `off` (`platform`), `look` (`tabs-read`, `history-read`, `bookmarks-read`), `read`
(`look` and `page-read`) and `browse` (`read`, `tabs-manage`, `bookmarks-write` and `navigate`). Given neither flag, `install` prints
the one declaration of [BROWSER-2](#BROWSER-2), which offers every tool.

Two groups are two servers to everything that keys on an alias. Declarations of one program that
differ only in `--group` have different digests, so an approval, an answer 2 given about a tool in a
project, an `Mcp(alias)` rule, a grant to call and a worker's `mcpServers:` line each reach the group
they name and no other, and an alias that is the start of another's reaches only itself.
`deferMcpToolsAbove` ([SERVERS-16](mcp-servers.md#SERVERS-16)) weighs the definitions of every
declared group together, so groups that are each under the size are offered by name when they are
over it as a whole, and loading one tool offers that definition alone.

**Why.** A server's grant, its approval and its digest belong to its alias
([SERVERS-9](mcp-servers.md#SERVERS-9), [SERVERS-5](mcp-servers.md#SERVERS-5)), and one alias that
offered every group would be granted and approved as one, so a person could not accept listing tabs
without accepting a page read. A group nobody declared is not offered to a session, and a group that
was declared is still refused by the extension while it is off, so both have to agree before a tool
runs. The group is a word the person wrote in the declaration, and it selects part of a fixed table.
The relay reads no byte of a request or a result to choose it, and finds a tool by its name in that
table as [BROWSER-10](#BROWSER-10) does. A word that is not a group starts no server, since a mistyped
group that offered every tool would give a session tools the person did not declare. A preset
names only groups that have a tool, since a server for a group with none offers nothing to
approve.

`verified-by: bravebot_browser::relay::a_server_started_for_a_group_lists_that_groups_tools_whether_or_not_the_extension_is_connected`
`verified-by: bravebot_browser::relay::the_tool_list_is_the_same_whether_or_not_the_extension_is_connected`
`verified-by: bravebot_browser::relay::a_server_started_for_something_that_is_not_a_group_prints_the_usage_and_serves_nothing`
`verified-by: bravebot_browser::relay::a_server_for_one_group_answers_no_other_groups_tool_and_asks_the_extension_for_none`
`verified-by: bravebot_browser::relay::a_tool_of_a_declared_group_that_is_off_in_the_extension_fails_with_its_refusal`
`verified-by: bravebot_browser::relay::installing_with_no_group_prints_the_one_declaration_of_every_tool`
`verified-by: bravebot_browser::relay::installing_with_groups_prints_one_declaration_for_each_group_named`
`verified-by: bravebot_browser::relay::installing_with_a_preset_prints_one_declaration_for_each_group_it_names`
`verified-by: bravebot_browser::relay::installing_declares_only_remove_data_cookies_scripts_the_input_groups_capture_trusted_console_trusted_upload_trusted_and_screen_to_ask_about_every_call`
`verified-by: bravebot_browser::relay::what_a_person_held_out_is_called_through_its_group_alone_and_arrives_as_one_text_part`
`verified-by: bravebot_browser::tools::a_group_is_found_by_its_exact_id`
`verified-by: bravebot_browser::tools::only_cookies_remove_data_scripts_the_input_groups_capture_trusted_console_trusted_upload_trusted_and_screen_are_asked_about_every_call`
`verified-by: bravebot_browser::tools::only_scripts_and_the_input_groups_capture_trusted_console_trusted_upload_trusted_and_screen_ask_the_person_in_the_browser_and_are_asked_about_every_call`
`verified-by: bravebot_browser::tools::every_tool_is_in_one_listed_group_and_a_group_is_listed_once`
`verified-by: bravebot_browser::tools::a_group_offers_its_own_tools_and_no_others`
`verified-by: bravebot_browser::tools::take_held_is_offered_by_the_server_of_held_alone_and_takes_no_argument`
`verified-by: bravebot_browser::tools::with_no_group_the_seven_original_tools_are_offered_in_order`
`verified-by: bravebot_browser::tools::a_tool_added_to_a_group_is_not_offered_with_no_group`
`verified-by: bravebot_browser::install::install_with_no_group_asks_for_the_one_server_of_every_tool`
`verified-by: bravebot_browser::install::groups_are_read_from_one_word_in_any_order_and_each_is_kept_once`
`verified-by: bravebot_browser::install::a_preset_names_the_groups_that_have_a_tool`
`verified-by: bravebot_browser::install::a_declaration_names_the_group_in_the_alias_and_in_the_program_words`
`verified-by: bravebot_browser::install::the_declaration_of_a_group_asked_every_call_says_so_before_the_program`
`verified-by: bravebot_browser::install::the_declarations_of_scripts_the_input_groups_capture_trusted_console_trusted_upload_trusted_and_screen_ask_about_every_call_and_bound_a_call_above_the_servers_wait`
`verified-by: bravebot_browser::install::the_declaration_of_the_input_group_declares_the_element_number_of_its_three_tools_as_taking_a_choice`
`verified-by: bravebot_browser::install::the_declared_choices_are_the_tools_whose_schema_takes_an_element_number`
`verified-by: bravebot_browser::install::no_preset_names_a_group_that_is_asked_about_every_call`
`verified-by: bravebot_browser::install::a_name_is_one_shell_word_in_single_quotes_unless_it_is_made_of_plain_characters`
`verified-by: bravebot_browser::install::a_declaration_quotes_the_directory_and_the_program`
`verified-by: bravebot_browser::relay::a_declaration_is_read_by_a_shell_as_the_directory_and_the_program_it_names`
`verified-by: bravebot_browser::relay::installing_prints_declarations_a_shell_reads_as_the_paths_installed`
`verified-by: bravebot_browser::relay::installing_prints_a_directory_that_is_not_utf8_as_the_bytes_it_has`
`verified-by: bravebot_browser::extension::the_relays_group_table_is_the_one_the_extension_holds`
`verified-by: bravebot_config::browser_group_aliases::the_alias_of_each_group_is_one_a_declaration_may_have`
`verified-by: bravebot_config::browser_group_aliases::two_groups_of_one_program_digest_apart`
`verified-by: bravebot_config::browser_group_aliases::approving_one_group_approves_no_other_and_the_others_have_not_changed`
`verified-by: bravebot_config::browser_group_aliases::an_alias_pointed_at_another_group_is_asked_about_again_and_the_other_aliases_keep_their_answers`
`verified-by: bravebot_config::browser_group_aliases::a_standing_answer_for_one_groups_tool_covers_no_other_group`
`verified-by: bravebot_config::browser_group_aliases::removing_one_groups_declaration_leaves_the_others_declared`
`verified-by: bravebot_core::browser_group_aliases::a_grant_for_one_group_is_not_a_grant_for_another`
`verified-by: bravebot_core::browser_group_aliases::an_mcp_rule_about_one_group_reaches_no_other_group`
`verified-by: bravebot_core::browser_group_aliases::the_policy_asks_and_refuses_each_group_by_its_own_rules`
`verified-by: bravebot_core::browser_group_aliases::an_endorsement_for_one_groups_tool_is_not_spent_on_another_groups`
`verified-by: bravebot_core::browser_group_aliases::a_worker_naming_one_group_holds_that_groups_server_and_no_other`
`verified-by: bravebot_core::browser_group_aliases::a_reader_or_a_checker_naming_a_group_holds_no_server`
`verified-by: bravebot_agent::browser_group_servers::several_groups_are_deferred_by_their_combined_size_and_one_loads_alone`
`verified-by: bravebot_agent::browser_group_servers::a_worker_whose_definition_names_one_group_is_offered_only_that_groups_tools`
`verified-by: bravebot_agent::browser_group_servers::answer_two_given_for_one_group_stops_the_question_for_that_group_only`

<a id="BROWSER-22"></a>
### BROWSER-22: a group that reads pages reaches the sites the person allowed, or all sites except the blocked ones

The options page holds a list of allowed sites beside the blocked sites of
[BROWSER-17](#BROWSER-17), under a key of its own. It is written one site to a line, with the same
rules: an HTTP or HTTPS origin compared as written, a path dropped, and a line that is not one
address stopping the save and being named. It starts empty.

A group whose line in the table of [BROWSER-18](#BROWSER-18) says it reads pages and carries no
`allowedOnly`, which are `page-read`, `cookies`, `network-watch`, `input` and `screen`, has one of two
scopes: only the allowed sites, or all sites except the blocked ones. It is the allowed sites until the person chooses the other, and a
stored scope that is anything but the choice of all sites is the allowed sites. Under the first
scope the group needs the host pattern of each allowed site, such as `https://news.example/*`, so
with no site allowed it needs no host and a group the person turns on reaches no page. Under the
second it needs `https://*/*` and `http://*/*`, except that `page-read` and `screen`, which each
have a tool that takes a picture of a tab, need `<all_urls>` in their place
([BROWSER-35](#BROWSER-35), [BROWSER-42](#BROWSER-42)). A group that does not read pages has no
scope. `scripts` reads pages, since the browser runs a user script only in a page whose host the
extension holds, and carries `allowedOnly` ([BROWSER-46](#BROWSER-46)): it needs the host pattern of
each allowed site and is limited to them whatever is stored as a scope. `input-trusted` reads no page
itself, since a second extension does what its tools ask, and is limited to the allowed sites
whatever is stored as a scope ([BROWSER-18](#BROWSER-18)): a call to it is refused unless its `url`
is at an allowed site, as for a group limited to them, and the options page offers it no choice of
all sites. The same holds for `capture-trusted`, `console-trusted` and `upload-trusted`.

A call to a tool of a group limited to the allowed sites is refused unless the `url` it names, and the
`frame_url` where it names one, is at an allowed site, compared as in [BROWSER-17](#BROWSER-17). A call
that names no `url` is refused. The refusal is one message that names the list and not the site, and
comes before a tab is looked for and before the browser is asked for permissions. A site on both
lists is blocked, by [BROWSER-17](#BROWSER-17). An allowed list that is not a list of strings is an
empty one, and the options page says it cannot read it until the next save replaces it.

The options page saves the allowed sites when the person clicks its button, not when they leave the
box. The click asks the browser for the host of every site written as its first call, and saves the
list only if the browser grants it. A line that is not an address, or a site the browser does not
grant, leaves the list saved before in force, and no request is made for a mistyped line. Once
saved, the click asks the browser to take back the host of each site that is no longer written.
Where the browser does not take one back, the list stays saved and the page says that access to a
site that was removed was not taken back. Choosing a scope for a group that is on asks the browser
first for what the group needs under that scope and saves the scope only if it is granted; narrowing
to the allowed sites then takes back what the old scope held and no group left on lists. A group
that is off saves the scope and asks for nothing. Turning a group on, a preset and turning a group
off ([BROWSER-19](#BROWSER-19), [BROWSER-20](#BROWSER-20)) ask for and take back these hosts.

Brave's own site access setting for the extension, with the choices on click, on specific sites and
on all sites, can withhold a host the person allowed. No documented API says which is set. A call it
prevents is refused as a permission the browser does not hold, and for a group that reads pages the
refusal says the setting can be the cause. A group is turned off, as for any permission, when the
browser stops holding a site on the list.

`downloads` lists `pageCapture` and `tabs` for `save_page_archive` ([BROWSER-40](#BROWSER-40)). The
browser lets an extension that holds `pageCapture` archive any open tab whatever hosts the extension
holds, and Brave words it as "Read and change all your data on all websites". `downloads` reads no host and has no scope, so
`save_page_archive` is named in `ALLOWED_ONLY_TOOLS` of `groups.js` and is limited to the allowed
sites whatever any group's scope is: a call to it is refused as above unless its `url` is at an
allowed site, and the other tools of `downloads`, which name a file to fetch and not a page, are not
held to the list. The refusal made by the extension is the only limit to the allowed sites, and the
options page says so in the note under the group.

**Why.** Turning on a group that reads pages without a list of allowed sites asks Brave for every
site at once, so the only way to keep it from a site is to block it, one site at a time. A person who names the sites a group may
read gives it a short list, and the warning Brave shows names those sites. Brave holds one set of
hosts for the whole extension, so a group limited to the allowed sites would reach any site another
group holds if only the browser decided. The extension, which is outside the driver, compares the
call's URL with the person's list, and the driver reads no part of a result to do so. The browser
takes a request only from a click, which is why the list has a button. A failure the site access
setting causes is reported as a failure, as [BROWSER-6](#BROWSER-6) and [MCP-5](mcp.md#MCP-5) require,
so a session is never told a page is empty because the setting kept the extension out of it.

`verified-by: by-construction (extension/tests/groups.test.mjs asserts that a group needs the host of each allowed site, none while none is allowed, or the two patterns on all sites, that a scope belongs to its own group, and what turning a group off and narrowing a scope take back; extension/tests/tools.test.mjs asserts that a group reaches no site until one is allowed, that the page and the frame it names must both be at allowed sites, that the refusal names the list and not the site and reaches no tab or script, that the browser is asked for the hosts of the allowed sites and for the two patterns on all sites, that a site on both lists is blocked, that a list or scope that cannot be read narrows, that a group that reads no page is not limited, and that a host the browser does not hold is refused naming the site access setting; extension/tests/switches.test.mjs asserts that turning a group on, a preset and turning a group off ask for and take back the hosts of the allowed sites, that saving the sites requests them first and saves only if granted, stops on a mistyped line before any request, gives back a site taken off the list after saving and reports a site the browser would not take back while keeping the list saved, and that choosing a scope requests first and gives back all sites only when narrowing, and then only when no other group that is on still needs them; extension/tests/options.test.mjs asserts that the page has the list beside the blocked sites, that saving is a click that asks the browser as its first call and says when the browser would not take back a site removed from the list, that the two scopes are offered with the allowed sites chosen at first, that what Brave will ask follows the sites and the scope, and that choosing a scope asks as the first call of the click; all run where the test under BROWSER-13 runs)`
`verified-by: by-construction (extension/tests/tools.test.mjs asserts that read_page_links and read_page_forms, in the group that reads pages, are refused with the one message naming the list when the URL is at a site that is not allowed, whether or not a tab is open at it, or is missing, before a tab is looked for, a script run or the browser asked for permissions, and answer for a URL at an allowed site; it runs where the test under BROWSER-13 runs)`

<a id="BROWSER-23"></a>
### BROWSER-23: a tab, the group it is in or its window is changed at exactly the URL asked for

`focus_tab`, `pin_tab`, `mute_tab` and `move_tab` each take `url`, which names the open tab whose
URL is that one, character for character, and act on that tab alone. `focus_tab` makes it the active
tab of its window and brings the window forward. `pin_tab` takes an optional `pinned` and
`mute_tab` an optional `muted`, each true where the call leaves it out, and a value that is not
`true` or `false` fails without a tab being looked for. `move_tab` takes `index`, a whole number
from -1 up, and moves the tab to that position in the window it is in, -1 being last, never to
another window; any other value fails the same way. A URL at a listed site fails as
[BROWSER-17](#BROWSER-17) says, and where no tab is at the URL the call fails and no tab is
changed. The reply holds the URL asked for and, for the last three, the state the browser reports
after the change: whether the tab is pinned, whether it is muted, or the position it was put at,
which differs from the one asked for where the browser keeps pinned tabs ahead of the others. The
tab is changed by its id once it is found, so a tab that navigates in between receives the change at
its new page.

`group_tabs` and `ungroup_tabs` take `urls`, a list of one or more URLs that each name an open tab as
above, a URL given twice naming its tab once. `group_tabs` puts those tabs into a new tab group in
the window they are in, with the optional `title`, and fails without grouping any of them where they
are not all in one window, so that no tab is moved to another window. `ungroup_tabs` takes them out
of their groups. A group has no address, so `update_tab_group` takes the `url` of a tab in it, and at
least one of `title`, `color` (grey, blue, red, yellow, green, pink, purple, cyan or orange) and
`collapsed`, changes only those, and fails where the tab is in no group. `discard_tab` unloads the
tab from memory and `duplicate_tab` opens a copy of it, which loads the same address again.
`set_zoom` takes `factor`, a number from 0 to 5, where 1 is 100% and 0 is the browser's default for
the page. `focus_window` brings the window that holds the tab forward and leaves its active tab as it
is. `resize_window` takes `width` and `height`, whole numbers of pixels from 1, and sets the size of
the window that holds the tab, leaving its position, state and tabs as they are. An argument that is not what the tool takes, including a list with an empty entry, an unknown
color, a call to `update_tab_group` that changes nothing, a factor outside the range and a size that
is not a whole number from 1, fails
without a tab being looked for. A URL of the list at a listed site fails the whole call before any tab
is looked for, and a URL that no tab is at fails it before any tab is changed. The reply holds the
URL, or `urls`, and the state the browser reports afterwards: for `group_tabs` the group's title and
color, for `ungroup_tabs` whether every tab is now in no group, for `update_tab_group` the group's
title, color and whether it is collapsed, for `discard_tab` whether the browser discarded the tab,
which it does not for the tab being shown, for `duplicate_tab` whether it made a copy and nothing of
the copy, for `set_zoom` the factor it holds, for `focus_window` whether the window is focused and
for `resize_window` the width and height it reports, which differ from the size asked for where the
browser keeps a window above its smallest size or leaves a maximized window as it is.

**Why.** The question before a call shows its arguments, and a URL is one a person can judge there
where a tab id or a window id is a number that says nothing about which page it is
([ROUTE-7](routing.md#ROUTE-7), [SERVERS-7](mcp-servers.md#SERVERS-7)). The routing field is the
alias and the tool, and a person can approve `brave-tabs-manage:pin_tab` without the URL, since
none of the tools opens a page for them, reads one or changes what one holds: each changes how a tab
is shown, grouped, zoomed or sized, or where it sits. That holds for the eight added later. `duplicate_tab`
loads again an address the tab already shows, as the browser's own duplicate command does, asks
nothing of a site that no tab is at, and returns nothing of the copy. `discard_tab` can lose what a
page does not save, which a person who approves the tool accepts for any tab, and it always names a
tab, since the browser discards a tab of its own choosing where it is given none. A group has no
address and an id says nothing a person can judge, so `update_tab_group` names a group by a tab in
it. `group_tabs` fails across windows, since a tab moved to another window is a change the URL does
not show. The reply goes through [MCP-1](mcp.md#MCP-1), so it is untrusted and private. It
says what the browser did because a tab the browser will not pin is not pinned. A string `"false"` is
true in JavaScript, and a call that read it as a switch would pin or mute the tab it was meant to
leave.

`verified-by: bravebot_browser::tools::a_tab_is_acted_on_by_its_url_and_what_is_asked_of_it`
`verified-by: bravebot_browser::relay::a_tool_outside_the_seven_is_called_through_its_group_alone`
`verified-by: by-construction (extension/tests/tools.test.mjs asserts that group_tabs groups the tabs at exactly the URLs asked for, in the window they are in, with the title and a URL given twice once, and fails and groups nothing for a URL no tab is at, tabs in two windows, a URL at a listed site among open ones or an argument that is not a list of URLs and a string title, that ungroup_tabs and update_tab_group report the state the browser holds when it left the group as it was, that update_tab_group sends only the fields given, accepts each of the nine colors and refuses another, a call that changes nothing and a tab in no group, that discard_tab and duplicate_tab name the tab asked for and report whether the browser did it, that set_zoom refuses a factor outside 0 to 5 and reports the factor the browser applied, that focus_window brings the window forward without a tab update, and that resize_window sets only the width and height of the window of the tab asked for, reports the size the browser holds where it keeps a window larger than asked, and refuses a size that is missing, below 1, not whole or not a number before any tab is looked for; it runs where the test under BROWSER-13 runs)`
`verified-by: by-construction (extension/tests/tools.test.mjs asserts that focus_tab makes the tab at exactly the URL asked for active and then brings its window forward, that pin_tab and mute_tab set the state, clear it on false and report what the browser did when it left the tab as it was, that a value that is not true or false, including the string false, and an index that is not a whole number from -1 up, reach no tab, that move_tab reports the position the browser chose, and that a listed site is refused with the one message before any tab is looked for; it runs where the test under BROWSER-13 runs)`

<a id="BROWSER-24"></a>
### BROWSER-24: a group is on for the lifetime the person chose, and each call checks it

When the person turns a group on, they choose how long it stays on: until they turn it off, until
Brave restarts, or for one hour. The confirmation of [BROWSER-19](#BROWSER-19) offers the three with
the default of the group chosen: until turned off for every group but `downloads`, `cookies`,
`remove-data`, `scripts`, `input`, `input-trusted`, `capture-trusted`, `console-trusted`, `upload-trusted`, `screen` and `held`, and until Brave restarts for those
eleven. A preset
([BROWSER-20](#BROWSER-20)) gives each group it turns on that default.

The lifetime is saved with the group's switch. An hour is saved as the time it ends at. A group
turned on until Brave restarts is marked in session storage, which Brave empties when it restarts
and when the extension is reloaded or updated, and the marker is saved before the switch. A group
with no saved lifetime has the default of its group. Each call to a tool of the group reads them
and is refused when the hour has passed, when the marker is missing, or when what is saved cannot
be read, naming the group and how long it was turned on for. An hour saved with an end more than an
hour from now, which a clock set back or a damaged value leaves, counts as one that cannot be read. The call does not depend on an alarm
firing or on the service worker having run since the lifetime ended.

The service worker turns off a group whose lifetime has ended, with its tools, and takes back the
permissions that no group still on lists, as turning it off does ([BROWSER-19](#BROWSER-19)). It does
so each time it starts, which includes each start of the browser and each reload of the extension,
and on the alarm that reconnects the port, which is once a minute. A lifetime that cannot be read
ends every group's. Saving the switches leaves such a value as it is while a group other than those
being turned on is on, so no group takes a lifetime from a record that is not its own, and writes the
lifetimes afresh once none is. The options page draws a group whose lifetime has ended as off, and says for a
group that is on when its hour ends or that it ends when Brave restarts.

**Why.** An alarm can be late or lost: the browser may clear alarms when it restarts, and a stopped
service worker runs nothing until an event wakes it. A lifetime enforced by an alarm alone would
last until the alarm fired, so the call reads the saved time and the marker. Session storage is
the browser's own record that it has not restarted, and it is not kept in the worker's memory,
which a restart of the worker loses. The eleven groups delete data, read cookies, run code, act on a
page, capture the screen, put files on disk or hold text a person selected, so they end by default
when the browser does and the person has to choose to keep them longer. A value that cannot be read ends a lifetime so that
damage to it never keeps a group on.

`verified-by: by-construction (extension/tests/lifetimes.test.mjs asserts the default lifetime of each of the twenty-one groups, that turning a group on saves the lifetime chosen, an hour as the time it ends at and a marker for a run, that an unknown lifetime is refused before the browser is asked, that a call is refused with no alarm at the millisecond the hour ends and answers the millisecond before, that a service worker restart does not end a run and an empty session storage does, that every shape of an unreadable record ends the lifetime while a group with no record has its default, that the worker turns off only the groups that have ended and takes back only what no group left on lists, that an unreadable value turns off every group and is left as it is by a save while another group is on and written afresh once none is, that an end more than an hour away, a text, a list or an infinite number ends an hour, that session storage that cannot be read holds no marker, that a preset renews a group whose hour has passed and leaves one still on, and that no switch is saved when the marker cannot be; extension/tests/background.test.mjs asserts that the worker turns off a group turned on until Brave restarts at its first start after a restart, leaves it within a run, and turns off a group whose hour has passed on the alarm; extension/tests/options.test.mjs asserts that each group offers the three lifetimes with the default of the group chosen, which is until Brave restarts for the eleven groups named above and until turned off for the others, that the one chosen is saved, that confirming remove-data with the choice left as offered saves until Brave restarts, that the page says when an hour or a run ends, and that a group whose hour has passed is drawn as off; all run where the test under BROWSER-13 runs)`

<a id="BROWSER-25"></a>
### BROWSER-25: the requests of a tab are kept from its page load, in memory, with no header, cookie or body

`list_requests` names a tab by its exact URL, as `list_frames` does, and returns the URL, type and
status of each request the tab has made since it loaded its page, in the order they began, and
whether more were made than are kept. Its group reads pages, so a call is limited to the allowed
sites, or to all sites that are not blocked, by the group's scope ([BROWSER-22](#BROWSER-22)). A request that failed has no status and carries the browser's
error. The extension registers its `webRequest` listeners with a URL filter and asks for no extra
information, so the browser gives it no header, cookie or body, and no line of its code outside a
comment names `extraHeaders`, `requestHeaders`, `responseHeaders` or `requestBody`.

The requests are kept only while the switch of the `network-watch` group and the switch of
`list_requests` are both on. The listeners are registered whatever the switches are, since the
browser reports an event only to a listener registered as the service worker starts, and each one
keeps a request only if a copy of the two switches, held in the worker's memory, says both are on.
The worker reads the switches as it starts and again whenever either is saved. Until the first read
has finished, and after a read that fails, nothing is kept. Turning either switch off forgets every
record. A group whose lifetime has ended ([BROWSER-24](#BROWSER-24)) is turned off by the worker, which
ends the keeping.

The requests are kept in the service worker's memory, in one record for each tab. A record begins
with the request that loads the tab's page, and ends when the next page load of the tab begins, the
tab closes, `webRequest` or a host is taken back, or keeping ends. A redirect of the page is part of the load it
began. It holds the first 500 requests of the load. A request is kept only for a tab whose page load
the worker saw begin, and one made by a page loaded ahead of time is not. The call reads the record of
the one tab whose URL is the one asked for. Where the tab has no record, which is a page that loaded
before the worker started or before the permission was granted, the call fails saying so and returns
nothing. It also fails for a tab
that is loading another page, and for a tab that has left the URL by the time the requests are read.

**Why.** A header reaches a listener only when the listener names it, and `Cookie`, `Authorization`
and `Set-Cookie` come only with `extraHeaders`, so leaving it out keeps a session token out of every
result without any filtering. The browser may stop the worker and start it again, which loses the
records. A list made of the requests seen since then would show the end of a load as all of it, so
a load whose start was missed has no record and the call fails. The records are in memory and in no
storage, which the options page reads, and the call reads one tab's record by the tab its URL names,
so a result holds no request of another tab. A request's URL can carry a token, so the result is
untrusted and private under [MCP-1](mcp.md#MCP-1) and goes to the person, as every result does. A
tool that is off touches nothing ([BROWSER-14](#BROWSER-14)), so a request seen while either switch
is off is not kept. A page that loads before the worker has read the switches has no record, as one
that loads before the worker starts has none, and the call says so.

`verified-by: bravebot_browser::extension::the_extension_never_asks_the_browser_for_a_requests_headers_or_body`
`verified-by: bravebot_browser::extension::the_extension_has_each_permission_its_tools_require`
`verified-by: bravebot_browser::tools::a_folder_is_asked_for_by_its_title_and_the_requests_of_a_page_by_its_url`
`verified-by: by-construction (extension/tests/tools.test.mjs asserts that list_requests gives the URL, type and status or error of each request of the tab in order, a redirect as two requests, and nothing else the browser adds, that it gives the requests of the tab at the URL asked for and of no other tab, that a new page load starts a new record and a redirect does not, that a page whose load the worker did not see, a restarted worker, a browser without webRequest and a closed tab have no record and the call says so, that a page loaded ahead of time is not the tab's page, that the requests are cut at 500 with the first kept and the cut said, that the listeners are registered with a URL filter and no extra information, that requests to a listed site and by a frame at one or inside one are left out, and that a tab loading another page or one that left the URL is refused, and that a URL outside the allowed sites is refused before a tab is looked for; extension/tests/background.test.mjs asserts that the worker registers the listeners as it starts, keeps a request only while the group's switch and the switch of list_requests are both on, forgets the records when either is turned off and keeps requests again from the next load when it is on, keeps nothing before the first read of the switches has finished or when it fails, lets the read begun last decide, ignores saves of other keys and of session storage, forgets the records and removes the listeners when webRequest or a host is taken back, keeps them again only while the group is still held under its scope, and starts them when the permission is granted to a running worker; both run where the test under BROWSER-13 runs)`

<a id="BROWSER-26"></a>
### BROWSER-26: keep_awake keeps the computer awake for the minutes asked, and the request ends

`keep_awake` takes a whole number of `minutes` from 0 to 240 and a `level` that is `system`, which
is the default, or `display`. Any other value is refused before the browser is asked. With 1 to
240 minutes it saves the level and the time the request ends in session storage, makes an alarm for
that time, and then calls `power.requestKeepAwake` with the level, in that order. A request made
before is replaced. It returns the level, the minutes and the time the request ends. With 0 minutes
it calls `power.releaseKeepAwake`, clears the alarm, forgets the saved request and returns that it
released.

The service worker releases the request when its time has come, at each start and on every alarm. A
saved value that is not a level and an end time counts as ended. Releasing calls
`power.releaseKeepAwake` before it forgets the saved request, and calls nothing of `power` where the
browser has no such API. `releaseAwake` in `extension/awake.js` is the one function that releases a
request, for the tool, the worker and any other part of the extension that has to end it.

The request also ends when the person turns the `keep_awake` switch off in the options page, since
a call with 0 minutes is refused once the tool is off, and when the `browser-info` group stops being
on: when the person turns the group off, when a preset or Off turns it off, and when the service worker turns it off because its
lifetime ([BROWSER-24](#BROWSER-24)) ended or because a permission it lists was taken back in the
browser's settings. The group's switches are saved, then the request is released, and then the
permissions are taken back, since `chrome.power` is gone once `power` is. A release that fails does
not stop the permissions being taken back. When the person took `power` itself back, the API is gone
before the worker is told, so the extension cannot tell the browser to release, and it forgets the
saved request and its alarm. Turning the group on releases a request left from before, once the
browser has granted the group's permissions, so a request from the group's earlier time on does not
carry into the next.

A call can be past the checks of [BROWSER-18](#BROWSER-18) when the group is turned off, and then
make its request after the release described above. The tool reads the group's switch and its own again once
the request is made. Where either is off, or cannot be read, the tool releases the request,
clears its alarm and forgets it, and refuses with a message that names the group or the tool, so no request
stands with the group off, whichever way the group was turned off and between whichever two steps
of the call.

**Why.** The `power` API has no time limit. A request stands until it is released or the extension is
unloaded, so one that nothing ended would keep the computer awake for as long as Brave runs. The tool
sets something on the person's computer, so it takes a time that is shown in the question and has a
ceiling, is undone by the same tool, and ends by itself. The end is saved before the browser is asked,
so a worker stopped between the two leaves a record to release and never a request that no record
names. Session storage is emptied when the browser restarts, which ends the request, and when the
extension is reloaded or updated, which unloads it. An alarm can be late or lost, so the worker
checks at each start and on every alarm as well. A request that stood after its group went would keep
the computer awake with the group off or its hour over, and a person who turned the group off
expects nothing it set to remain. The popup and the keyboard command turn the group off in a page
or in the worker, and a call may be running in the worker, so the call looks at the group after its
request and does not depend on being told.

`verified-by: bravebot_browser::tools::the_browser_info_tools_take_a_url_a_number_of_seconds_or_minutes_and_nothing_else`
`verified-by: by-construction (extension/tests/tools.test.mjs asserts that keep_awake saves the end and makes the alarm before it asks the browser, returns the level, minutes and end, uses the system level where none is given, accepts 240 minutes, replaces an earlier request, releases and forgets the request with 0 minutes, and refuses minutes that are not a whole number from 0 to 240 and a level that is not the browser's before anything is asked; extension/tests/awake.test.mjs asserts that a request is left standing until its time and released at it, that a saved value that is not a request is released, that nothing is released where nothing is saved, that the alarm and record are cleared where the browser has no power API, and that the browser is told to release before the record is forgotten; extension/tests/background.test.mjs asserts that the worker releases a request whose time has passed as it starts and on an alarm, leaves one that has not, and reports a check that fails, and that it releases the request of a browser-info group whose hour has ended, at its start and on the alarm, and of one whose permission was taken back, before it takes permissions back; extension/tests/switches.test.mjs asserts that turning the keep_awake switch off releases the request and keeps the permissions while no other tool switch does, that turning the group off, a preset and Off, the end of a one-hour lifetime and a permission of the group taken back each release the request before the power permission is taken back, that a group that lists no keep_awake leaves the request alone, that a lifetime that has not ended releases nothing, that the saved request and alarm are forgotten where the power API is already gone, that turning the group on releases a request left from before after the browser's request, and that a release that fails does not stop the permissions being taken back; all run where the test under BROWSER-13 runs)`
`verified-by: by-construction (extension/tests/tools.test.mjs asserts that a keep_awake call whose group is turned off by its switch or by Turn everything off, or whose tool is turned off, while its end is being saved, or while its alarm is being made, leaves no request standing, no saved request and the alarm cleared and is refused naming the group or the tool, and that a call that cannot read whether its group is still on releases its request; it runs where the test under BROWSER-13 runs)`

<a id="BROWSER-27"></a>
### BROWSER-27: every call that reaches the extension is logged with its group, tool, outcome and the origins of its URLs

The extension adds every request it receives on the native messaging port to a log, before it
returns the answer. An entry holds when the request arrived, the group and tool it named, the
outcome, and the origins of the URLs in its `url`, `frame_url`, `to` and `urls` arguments, each origin
once, at most ten, and each cut at 255 characters, so 200 entries stay under about 1 MB of the 10 MB
that `storage.local` holds. A list argument of more than 50 entries (`urls`, `types` and any other)
is refused before the origins are taken, with an error that names the argument and a log entry whose
outcome is `failed`, so a request of any size gets a reply and an entry. The outcome is `ran` when the tool returned a result, `refused` when a rule of the
extension stopped the call, and `failed` for anything else, such as a page that is not open or an
error from the browser. The rules are a tool's switch, the group's switch and lifetime
([BROWSER-24](#BROWSER-24)), the blocked and allowed sites ([BROWSER-17](#BROWSER-17),
[BROWSER-22](#BROWSER-22)), a list of sites that cannot be read, the browser's permissions
([BROWSER-18](#BROWSER-18)), and a method the extension does not have.

A call is logged whether or not its group is on, and whether or not the person is watching the
session that made it. The entry holds those fields and no other: no path, query, fragment or
credentials of a URL, no page text, search text or title, no part of a result, and, for a method the
extension does not have, not the name that was sent.

The log is the last 200 entries in `storage.local`, the oldest dropped, and is added to one call at
a time. A log that cannot be written is reported and does not change the answer. A stored value that
is not a list of entries of this shape, and an entry whose time is outside the range of dates,
reads as the entries that are. The options page shows the log
newest first, with the time, group, tool, outcome and sites of each, says so when it is empty, and
draws it again when a call is added without drawing the switches again.

**Why.** Any process that can read the secret of [BROWSER-3](#BROWSER-3) can send the extension a
request, and the extension cannot tell it from the session the person is watching. A request can be
as large as the message limit allows ([BROWSER-8](#BROWSER-8)), so the bounds on a list, on the
origins of an entry and on the length of an origin keep the log, which has to fit in the storage and
be drawn by the options page, small. The switches
decide what such a call can do, and the log is where the person sees what was asked, including a call
the switches refused, which is the sign that a session wanted something that was off. A path or a
query can carry a token or the words of a search, so only the origin is kept, which says which site a
call named without saying what for. Two calls that finish together would each read the log and the
later write would drop the earlier entry, so entries are added one at a time. A name taken from a
request the extension does not recognise is not stored, so the log holds only text the extension
chose.

`verified-by: by-construction (extension/tests/activity.test.mjs asserts that a call that runs, one a rule refuses and one that fails are logged with their group, tool and outcome, that each rule is logged as refused and a tool that is tried and fails as failed, that a call is logged with nothing turned on, that an entry holds the five fields and nothing of the arguments or result beyond the origins of the URLs, which are those of url, frame_url, to and urls once each and at most ten, each cut at 255 characters, that a list of more than 50 entries in urls or types, or in a call of a method the extension does not have, is answered with an error and logged as failed, and that exactly 50 is read, that its time is when the request arrived, that the log keeps the last 200 calls, that calls finishing together are all kept, that a log that cannot be written does not change the answer, that a stored log that is not entries, or an entry whose time is no date, reads as the entries that are, and that an unknown method is logged without the name sent; extension/tests/options.test.mjs asserts that the page lists the log newest first with time, group, tool, result and sites, says when it is empty, draws a call added while it is open without drawing the switches, and says what the log keeps; all run where the test under BROWSER-13 runs)`

<a id="BROWSER-28"></a>
### BROWSER-28: a bookmark is changed at exactly the URL asked for and a folder at exactly its title, and nothing is deleted

`create_bookmark`, `create_bookmark_folder`, `move_bookmark` and `rename_bookmark` change the
person's bookmarks, and `add_reading_list_entry` and `update_reading_list_entry` their reading list.
They are in `bookmarks-write` ([BROWSER-18](#BROWSER-18)). None deletes anything, and none takes the
id the browser gives a bookmark or a folder. A bookmark and a reading list entry are named by their
URL and a folder by its title, each exactly as written. Of what the browser returns for a search by
URL or title, a node at another URL or with another title is not taken as the bookmark, the folder
or the entry, as `remove_bookmark`, `remove_bookmark_folder` and `remove_reading_list_entry` do not
take one ([BROWSER-31](#BROWSER-31)). Two addresses are one where the browser writes them alike.

`move_bookmark` and `rename_bookmark` take the `url` of the bookmark. The browser is asked for the
bookmarks at that URL, and the call fails, changing nothing, where there is none or more than one.
The failure for more than one says how many and nothing of them: `<n> bookmarks are at <url>, so the
URL does not name one bookmark`. A folder is named by its title in the same way by `create_bookmark`
and `create_bookmark_folder`, for the folder the new node goes in, and by `move_bookmark` for the
folder the bookmark goes to: the call fails, creating and moving nothing, where no folder has the
title or more than one does. The bookmark and the folder of `move_bookmark` are both found before
anything is moved.

The built-in folders at the top of the tree, which the browser makes and titles in the person's
language, are named by their title like any other folder, by `create_bookmark`,
`create_bookmark_folder`, `move_bookmark` and `list_bookmark_folder`. `remove_bookmark_folder` does not
take them, since the browser refuses to remove them. The browser's search by title does not return
them, so the extension reads
them from the bookmark tree and counts them with what the search returns, once each. A node at the top
of the tree is a folder only if it has no URL, and its title must be exactly the one asked for. A
folder a person made with the same title as a built-in folder makes the title name neither. The tree
is read with the `bookmarks` permission both bookmark groups already hold.

`create_bookmark` takes `url`, an HTTP or HTTPS address, an optional `title` and an optional
`folder`; where no folder is named the browser puts the bookmark where it puts a new one. Any other
scheme fails, since a `javascript:` bookmark runs in a page when it is clicked. `create_bookmark_folder`
takes `title` and an optional `parent`, a folder named as above. `move_bookmark` takes `url` and
`folder` and puts the bookmark at the end of the folder. `rename_bookmark` takes `url` and `title` and
changes the title alone. `add_reading_list_entry` takes an HTTP or HTTPS `url`, a `title` and an
optional `read`, false where it is left out, and fails where the list has an entry at the URL.
`update_reading_list_entry` takes `url` and at least one of `title` and `read`, changes only what is
given, and fails where the list has no entry at the URL. A title that is empty or not text, a `read`
that is not `true` or `false`, and a call to `update_reading_list_entry` that changes nothing fail
before the browser is asked for a bookmark, a folder or an entry. A URL at a listed site fails as
[BROWSER-17](#BROWSER-17) says.

The reply holds the URL and what the browser reports afterwards: for `create_bookmark` its title and
folder, for `create_bookmark_folder` its title and parent, for `move_bookmark` the folder, for
`rename_bookmark` the title, and for the reading list tools the title and whether the entry was read.
A bookmark at a URL that has one, and a folder with the title of one, are added as the browser adds
them, and `move_bookmark`, `rename_bookmark` and the calls that name a folder then fail for that URL
or title until one of the two is renamed or removed in the browser.

**Why.** The question before a call shows its arguments
([SERVERS-7](mcp-servers.md#SERVERS-7)), and a URL and a title are what a person can judge there,
where a node id read from an earlier result is not ([ROUTE-7](routing.md#ROUTE-7)). Two bookmarks at
one URL cannot be told apart by the argument the person approved, as two tabs at one URL cannot
([BROWSER-13](#BROWSER-13)), and a change to the first of them may be to the one they did not mean.
The routing field is the alias and the tool, and a person can approve
`brave-bookmarks-write:rename_bookmark` without the URL: none of the tools opens a page, reads one or
deletes anything, and each adds or changes an entry the person sees in the browser and can change
there. A bookmark that is not a web address could run code in a page when the person clicks it, which
no argument shown in the question says. The reply says what the browser holds because a browser that
keeps nothing has not made the change. It goes through [MCP-1](mcp.md#MCP-1), so it is untrusted and
private.

`verified-by: bravebot_browser::tools::a_bookmark_is_named_by_its_url_and_a_folder_by_its_title_and_never_by_an_id`
`verified-by: bravebot_browser::relay::a_bookmark_is_changed_through_its_group_with_the_url_and_title_named`
`verified-by: bravebot_browser::extension::the_group_table_lists_the_groups_and_each_tool_in_one`
`verified-by: by-construction (extension/tests/tools.test.mjs asserts that create_bookmark and create_bookmark_folder add the node in the folder with exactly the title asked for or where the browser puts it, send only the fields given, and report the title and folder the browser holds, that an address that is not HTTP or HTTPS, text that is empty or not text and a folder that is no folder or several fail without a node being made, that move_bookmark and rename_bookmark find the bookmark at exactly the URL, fail saying how many for two or three and say nothing of them, fail for none and for a URL that only resembles one, move nothing for a folder that is not exactly one, and report what the browser holds, that add_reading_list_entry adds an unread entry unless told it was read, refuses a duplicate address and an entry the browser did not keep, and that update_reading_list_entry sends only the fields given, including a read of false, and refuses a call that changes nothing and an address with no entry; that a search by URL or title that answers with other nodes or entries makes none of them the bookmark, the folder or the entry changed; that a built-in folder at the top of the tree, which a search that leaves it out does not return, is named by its exact title by list_bookmark_folder, move_bookmark, create_bookmark and create_bookmark_folder, is found once when the search returns it too, is not named by another case, spelling or a bookmark at the top of the tree, and with a folder a person made with the same title names neither and moves nothing, and that list_bookmark_folder with no folder returns the title of each built-in folder and nothing else, up to the count; and that each tool refuses a URL at a listed site before the browser is asked for anything; extension/tests/activity.test.mjs asserts that these calls are logged with the group, tool, outcome and origin of the address and nothing of the title or folder; extension/tests/switches.test.mjs asserts that bookmarks-read and bookmarks-write keep their permissions for each other and that Off and Look turn the tools of bookmarks-write off; extension/tests/lifetimes.test.mjs asserts that a bookmark is added within the hour of bookmarks-write, refused after it, and that the worker then turns the group off; all run where the test under BROWSER-13 runs)`

<a id="BROWSER-29"></a>
### BROWSER-29: a video of a tab, acting in a frame, dragging by a distance and a processor's choice beyond `element_number` are not built

The manifest declares none of `debugger`, `tabCapture` and `offscreen`, no group lists one, and no
script of the extension calls one. The relay offers no tool named `capture_tab_video`. `input` was one
of the entries and is built ([BROWSER-33](#BROWSER-33)), and so are the picture of a page that the
entry for pictures covered ([BROWSER-35](#BROWSER-35)), the file of a page that the entry for files
covered ([BROWSER-40](#BROWSER-40)), `capture_screen` and a recording of a page as a GIF, which were
the rest of `screen` ([BROWSER-42](#BROWSER-42)), `run_script`, which was the entry for `scripts`
([BROWSER-46](#BROWSER-46)), and a target a processor chooses from the numbered list, for the three
tools of `input` that take `element_number` ([ROUTE-10](routing.md#ROUTE-10)). Each of the remaining
entries below waits for a
ruling that a maintainer of this repository makes, or for a browser to allow it. No code for one is
written until a clause replaces its entry, in the change that adds the code. The options, the
default recommended and what a reversal changes are in [the
design](../design/browser-extension.md#open-decisions).

**`input`, what is not built.** The tools of `input` are built on injected functions
([BROWSER-33](#BROWSER-33)). Trusted input events, hovering and dragging by an element are the
tools of `input-trusted`, which a second extension with its own id does with the `debugger`
permission, which Chrome does not let an extension make optional
([BROWSER-37](#BROWSER-37), [BROWSER-38](#BROWSER-38)); `debugger` is never declared in this
extension, and [BROWSER-4](#BROWSER-4), [BROWSER-5](#BROWSER-5) and [BROWSER-9](#BROWSER-9) name
one id because the second extension is not a native messaging client. Choosing a file is the tool
of `upload-trusted` ([BROWSER-45](#BROWSER-45)). Acting in a frame, answering a dialog that no action of a
tool opened, and dragging by a distance or a position are not built.

**`capture_tab_video`.** It would use `tabCapture`. Brave 1.98.62 refused `getMediaStreamId` and
`capture` with "Extension has not been invoked for the current page (see activeTab permission)."
while the extension held `tabCapture`, `tabs` and `<all_urls>`, and again after the extension opened
its own popup, so the call works only on a tab the person has invoked the extension on, with its
button or its shortcut, and a session cannot start it
([the design](../design/browser-extension.md#taking-a-picture-of-the-screen)). A video is also none
of the four types [MCP-6](mcp.md#MCP-6) takes. Default: not built. Before code, a clause here says
what the person does on the tab for each call, how long a call waits for it, and how a clip is
carried, which is as a GIF of the four types or in a type that [MCP-6](mcp.md#MCP-6) is changed to take. The invoked path
was not run, since a headless browser has no toolbar to click.

**A target chosen by a processor, what is not built.** A processor's choice fills only `element_number`
of `click_element`, `type_text` and `select_option`, and only where the declaration of the server
names it ([SERVERS-22](mcp-servers.md#SERVERS-22)). A choice for another tool or argument, and a
window that says a processor chose the element, are not built: the question the person is asked first
says it, and the window shows the element as it does for a number the person gave
([BROWSER-41](#BROWSER-41)).

**Why.** Each of the entries widens what a session can do in a person's browser, and each needs a
clause of another spec to change or to be written first. A tool added before the ruling would put
its permission in an install prompt and its name in a list a person accepts, with no clause saying
what it may reach. The test holds the manifest, the relay and the code free of what the tools would
need, so the first tool to land fails it and the change that adds the tool replaces the entry here.
`input` was replaced in that way by [BROWSER-33](#BROWSER-33), the picture of a page by
[BROWSER-35](#BROWSER-35), the file of a page by [BROWSER-40](#BROWSER-40), `capture_screen` by
[BROWSER-42](#BROWSER-42), and `run_script` by [BROWSER-46](#BROWSER-46).

`verified-by: bravebot_browser::extension::the_entries_that_wait_for_a_ruling_have_no_tool_permission_or_call`
`verified-by: bravebot_browser::extension::the_extension_has_each_permission_its_tools_require`

<a id="BROWSER-30"></a>
### BROWSER-30: the tools of `cookies` list names and delete by name, and no tool reads or sets a cookie's value

`list_cookie_names` takes `url`, an HTTP or HTTPS URL, and replies with that URL and, for each cookie
the browser sends to it, the cookie's name, domain and path, whether it is `secure`, `http_only`,
`host_only` and a `session` cookie, its `same_site` setting and its expiry as an ISO 8601 time, null
for a session cookie. `delete_cookie` takes `name` and `url`. It asks the browser for the cookies of
that name sent to the URL and deletes only where exactly one is, replying with the URL, the name and
the domain and path of the cookie deleted. Where none is, where more than one is, where one of them
is a listed site's cookie ([BROWSER-17](#BROWSER-17)) and where the browser says it deleted
nothing, it fails and says so, giving the number where there are several, and deletes nothing. A
cookie is a listed site's when the site's host is the cookie's domain or a host inside it, with or
without the dot a domain cookie begins with, so a cookie for `.example.com` is the cookie of a listed
`mail.example.com`, and a list leaves it out whichever host the URL names. The
group lists `cookies` and reads pages ([BROWSER-18](#BROWSER-18)), so a call is limited to the
allowed sites or to all sites that are not blocked ([BROWSER-22](#BROWSER-22)), `url` is the page it
names, and the group is on until Brave restarts unless the person chose otherwise
([BROWSER-24](#BROWSER-24)). `install` declares its server with `--ask-every-call`
([BROWSER-21](#BROWSER-21)), so every call is put to the person and no answer is remembered.

The browser gives the extension each cookie with its value. A tool builds its result from the named
fields above, so no value is read, kept, logged or returned, and the extension calls no method of
`chrome.cookies` but `getAll` and `remove`. No tool reads a cookie's value or sets a cookie. This is
the default until a maintainer rules on the question below, and the first tool that does replaces
this paragraph in the change that adds it.

**The question.** May a session read the value of a cookie, or set one? Default: no. To change it
the maintainer settles three things. The first is what reaches the planner. A result is shown to the
person and not given to the planner ([MCP-1](mcp.md#MCP-1)), so a value reaches the planner only if
the person puts it into a message, which puts a credential in the planner's context. The second is
what the person approves. A value cannot be judged in the question as a URL or a name can, and
[ROUTE-7](routing.md#ROUTE-7) builds no tool whose routing field a person could not approve alone,
so the ruling says whether a value to set is part of the routing field. The third is what
[PERM-9](permissions.md#PERM-9) asks of a call whose arguments carry a token the planner was given,
which is asked whatever a rule or a standing answer says.

**Why.** A cookie value is usually a session token: whoever holds it acts as the person at that site.
A name, domain, path and flags show which sites the person is signed in to and are not a credential.
Deleting a cookie signs the person out of one site and takes nothing from it. The browser's
`cookies.remove` deletes every cookie of the name that the URL is sent, and answers as though it
deleted one when none exists, so the tool counts them first, and the question, which shows one name
and one URL, names one cookie only where one is sent. A cookie belongs to a host and is sent to each
scheme and port of it, so a listed site covers its host here, where for a page it covers one origin, and a cookie for a
parent domain is sent to that host as well.
The extension, which is outside the driver, compares the person's list with a URL and a cookie's
domain, and the driver reads no part of a result to do so.

`verified-by: bravebot_browser::extension::no_tool_reads_a_cookie_value_or_sets_a_cookie`
`verified-by: bravebot_browser::tools::the_cookie_tools_take_a_url_and_a_name_and_no_tool_takes_a_cookie_value`
`verified-by: bravebot_browser::relay::a_search_a_wait_and_the_cookie_tools_are_called_through_their_group_with_what_they_name`
`verified-by: bravebot_browser::relay::installing_declares_only_remove_data_cookies_scripts_the_input_groups_capture_trusted_console_trusted_upload_trusted_and_screen_to_ask_about_every_call`
`verified-by: by-construction (extension/tests/tools.test.mjs asserts that list_cookie_names gives a cookie's name, domain, path, flags and expiry and nothing else the browser holds of it, with no read of the value, which the fake records, and none in the reply; that delete_cookie deletes the one cookie of the name the URL is sent after listing those cookies, reads no value, and says the domain and path; that it deletes nothing for none, for several, for one at a listed host and when the browser says it deleted none; that both refuse a URL that is not HTTP or HTTPS and a name that is not text, before any cookie is asked for; that both refuse the host of a listed site under another scheme or port and leave out its cookies, and do not take a host that only resembles it; that a cookie for a parent domain of a listed host, with or without the leading dot, is left out of a list and not deleted through another host while a cookie of a sibling or of a name that only ends alike is kept; that both are limited to the allowed sites; that the log keeps the origin and not the name; and that no tool reads a value, sets a cookie or calls a method of chrome.cookies but getAll and remove; it runs where the test under BROWSER-13 runs)`

<a id="BROWSER-31"></a>
### BROWSER-31: the tools of remove-data delete only what the call names, and a call that names no bound or kind deletes nothing

`remove-data` is the group whose tools delete. It is on until Brave restarts unless the person
chooses another time ([BROWSER-24](#BROWSER-24)), and its server is declared so that every call to
it is asked about and no answer is kept ([BROWSER-21](#BROWSER-21)).

`close_tab` takes `url` and closes the tab at that URL, character for character. `close_window` takes
`url` and closes the window that holds the tab at that URL, with every tab in it, and does not
close it where a tab in the window is at a listed site or loading one ([BROWSER-17](#BROWSER-17)). A
URL at which more than one tab is open names none ([BROWSER-13](#BROWSER-13)). Each reply holds the
URL and whether the browser no longer has the tab or the window, and `close_window` also the number
of tabs the window held.

`delete_history_entry` takes `url` and deletes the history entry at that URL with every visit to it.
It fails where there is no entry, and its reply holds the URL, the number of visits there were and
whether none is left. `remove_bookmark` takes `url` and deletes the bookmark at that URL, character
for character. It fails where no bookmark is at the URL and where more than one is, saying how many
and nothing about them, and deletes none. A folder has no URL and is not found. Its reply holds the
URL and whether the bookmark is gone.

`delete_history_range` takes `start` and `end`, and `clear_browsing_data` takes `since` and `types`.
Each moment is an ISO 8601 date and time with its offset, such as `2026-10-01T00:00:00Z`. A moment that
is missing, is not a string, has no time or no offset, names a day the month does not have, or is
before 1970 fails the call before the browser is asked. `delete_history_range` also fails where
`start` is not before `end`, and the browser removes a page from the history only where every visit to
it is inside the range. `types` is a list of one or more of `cache`, `cacheStorage`, `cookies`,
`downloads`, `fileSystems`, `formData`, `history`, `indexedDB`, `localStorage` and
`serviceWorkers`, each once, and `clear_browsing_data` clears those kinds on every site since
`since`. A list that is empty, is missing, has another entry in it, such as `passwords`, or has one
that is not a string fails the whole call. The replies hold the moments as the browser was given
them and, for `clear_browsing_data`, the kinds.

`remove_bookmark_folder` takes `folder` and deletes the bookmark folder with that title, character
for character, only where it is empty. It calls `bookmarks.remove`, which deletes a bookmark or an
empty folder, and never `bookmarks.removeTree`, which deletes what is in a folder. A folder with
anything in it is left as it is, and the failure says how many entries it holds and names none of
them, so its bookmarks are deleted one at a time by `remove_bookmark` or kept. A title that no folder
has fails the call, and so does a title that more than one folder has, saying how many and nothing
about them; a bookmark with that title is not a folder. A folder the browser keeps and does not allow
to be removed fails with the browser's words. The reply holds the title and whether the folder is
gone.

`remove_reading_list_entry` takes `url` and removes the reading list entry at that URL, character for
character, with `readingList.removeEntry`. The reading list holds one entry for an address, so the
URL names one entry or none, and the call fails where there is none. The reply holds the URL and
whether the entry is gone.

`delete_downloaded_file` takes one of `filename`, the absolute path the browser holds for a
download, and `url`, the address the download started from, each character for character. It
deletes the file of a finished download from the disk with `downloads.removeFile`, and calls no
other method that changes a download: not `downloads.erase`, so the download stays in the list with
its file gone, which is how the browser keeps it. A call that gives both names, neither, or text
that is not text fails before the browser is asked. The browser is searched with `downloads.search`
for the name, and a download whose file the browser says is gone is not a candidate. The call fails,
and deletes nothing, where no download has the name, where each download that has it has a file the
browser says is gone, where more than one download with a file on the disk has it, saying how many
and nothing about them, and where the one download is not finished. A file that is gone before the
browser has noticed it is asked of `downloads.removeFile`, which fails, and the browser's words are
the failure. The reply holds the path and the URL the browser holds for the download, and whether the
browser now says the file is gone.

A bound, a kind, a URL, a title, a path or a filename is the whole of what a call deletes. No tool
takes a tab id, a window id, a bookmark id, a download id or a value read from an earlier result,
and none has a default: the browser reads a missing `since` as the start of time, so a call that left
it out would clear everything.

**Why.** A deletion cannot be undone, and the person approving it reads the arguments in the question
before every call. A URL, a title, a path, two moments and a list of kinds are what they can judge
there, where an id is a number. A folder is deleted only when it is empty because what is in it is
not in the question, and the person who wants it gone with its bookmarks deletes each bookmark from
a question of its own. The entry of a download is left in the list, so the list still shows which
file was deleted. The routing field of a call is the alias and the tool, as in
[ROUTE-7](routing.md#ROUTE-7) and [SERVERS-7](mcp-servers.md#SERVERS-7), and the extent of each
deletion is in the arguments that question shows. A bound the call could leave out would become a
bound the person did not read, and a number is read as 0 or as a time in another unit, so each is
refused. Two bookmarks at one URL are two the question cannot tell apart. A window closed with a
tab at a listed site would end that site's page, which the list says no tool touches. The two
tools that delete by time or by kind name no site, so no entry can be left out for the list, and they
fail when the list cannot be read as every other tool that acts on what the person has open or kept
does, and so does the tool that deletes an empty folder. A file downloaded from a listed site is
not deleted for the same reason a bookmark at one is not listed: the list says no tool acts on it. A reply goes through [MCP-1](mcp.md#MCP-1), so it is untrusted and private, and it carries
nothing of what was deleted beyond the URL the call named.

`verified-by: bravebot_browser::tools::what_is_deleted_is_named_by_a_url_two_moments_or_a_list_of_kinds_and_nothing_else`
`verified-by: bravebot_browser::tools::a_folder_an_entry_and_a_downloaded_file_are_named_by_title_url_or_filename_and_nothing_else`
`verified-by: bravebot_browser::extension::the_group_table_lists_the_groups_and_each_tool_in_one`
`verified-by: bravebot_browser::relay::a_server_started_for_a_group_lists_that_groups_tools_whether_or_not_the_extension_is_connected`
`verified-by: by-construction (extension/tests/tools.test.mjs asserts that close_tab closes the tab at exactly the URL asked for and says whether the browser closed it, that close_window closes the window of that tab with its tabs counted and does not close one that has a tab at a listed site or loading one, that delete_history_entry deletes the entry with every visit and no other and fails where there is none, that delete_history_range passes the browser the moments with their offsets read and refuses every bound that is missing, a number, a date alone, a time without an offset, a day the month lacks, before 1970 or not before the end, that clear_browsing_data passes the browser the start and exactly the kinds named once each and refuses a missing start or kind, a start that is a number and every kind that is not on the list, that remove_bookmark deletes the one bookmark at exactly the URL and none where there are several, saying how many, that remove_bookmark_folder deletes the empty folder with exactly the title, never a bookmark with it, and leaves a folder with a bookmark or a folder in it, or with a title that two folders have, as it is, saying how many, that remove_reading_list_entry removes the one entry at exactly the URL and none for a URL that only resembles it, that delete_downloaded_file calls downloads.removeFile with the id of the one download that has exactly the filename or the URL given and never downloads.erase, takes the file of one download of two that share a filename when the other file is gone, and deletes nothing, saying why, for a name that is not exactly one, two names, none, a file that is gone, a download that is not finished, a file the browser has not noticed is gone and a download from a listed site named by its filename, that the tools that name a URL refuse a listed site before the browser is asked, that the tools of the group are refused until the browser holds tabs, history, bookmarks, readingList, browsingData and downloads, and that a list of sites that cannot be read stops all nine; extension/tests/options.test.mjs asserts that the page offers the group with its warnings and its tools and turns it on until Brave restarts by default; all run where the test under BROWSER-13 runs)`

<a id="BROWSER-32"></a>
### BROWSER-32: download_file fetches only the URL asked for into the Downloads folder, a download is named by its path or address, and no tool reads the file

`downloads` is the group whose tools start a download and act on the downloads the browser holds. It
is on until Brave restarts unless the person chooses another time ([BROWSER-24](#BROWSER-24)). It
lists the `downloads` permission, `pageCapture` and `tabs` for `save_page_archive`
([BROWSER-40](#BROWSER-40)) and no host, since no tool reads a page through a host, so the allowed
sites list of [BROWSER-22](#BROWSER-22) applies to that tool by name and to no other tool of the
group.

`download_file` takes `url`, which must be an HTTP or HTTPS URL; any other scheme, or no URL, fails
without the browser being asked. It starts the download with `downloads.download`, given the URL,
`saveAs` false and `conflictAction` `uniquify`, and nothing else: no folder, file name, method,
header or body is an argument, and any such argument in a call is dropped. The browser saves the file
in the person's Downloads folder under the name it chooses, asks nothing, and does not replace a
file that is there. A URL at a listed site is refused before the browser is asked
([BROWSER-17](#BROWSER-17)). The request carries the person's cookies for the site, so the rule of
[BROWSER-16](#BROWSER-16) holds as it does for `open_tab`: once the browser has started the
download, one whose address after its redirects is on another host than the one asked for, or at a
listed site, is cancelled with `downloads.cancel`, a file it had already finished is deleted with
`downloads.removeFile`, and the call fails naming the URL asked for and not where the download
went. The reply holds the URL, the path the browser holds for the file and the state of the download.

`list_downloads` takes an optional count, at most 100 and 20 where none is given, and lists the
downloads newest first. Each entry holds `filename`, the absolute path the browser holds, `url`, the
address the download started from, its state, whether it is paused, whether the browser says its file
is still on the disk, the bytes received and the size where the browser knows it, and when it
started. It holds no download id. A download that started at a listed site or ended at one is left
out.

`pause_download`, `resume_download`, `cancel_download` and `show_download` take one of `filename` and
`url`, each character for character as `list_downloads` shows them, as `delete_downloaded_file` does
([BROWSER-31](#BROWSER-31)). A call that gives both, neither, or a name that is empty or is not
text fails before the browser is asked. The browser is searched with `downloads.search` for the name, and the call acts
on the one download with that name that the tool applies to: `pause_download` to one in progress and
not paused, `resume_download` to one the browser says can be resumed, `cancel_download` to one in
progress, paused or not, and `show_download` to one whose file the browser does not say is gone. It
fails, and acts on nothing, where no download has the name, where none that has it can be acted on,
saying what state the download is in, where more than one can be, saying how many and nothing about
them, and where the download started at a listed site or ended at one. `cancel_download` does not
delete the file of a finished download, and `show_download` shows the folder of the file in the
computer's file manager and does not open the file. The replies of the first three hold the path, the
URL, and the state, whether it is paused and the error the browser reports, read from the browser
again afterwards. The reply of `show_download` holds the path and the URL.

**The file.** A downloaded file is in the person's Downloads folder, which is outside the workspace
and has no rule in the trust map, so the map treats it as unvouched
([TRUST-1](trust-map.md#TRUST-1)). The file is reachable by the file tools only after the person
opens the folder with `/add-dir` ([TRUST-9](trust-map.md#TRUST-9)), which trusts every file in it,
the downloaded ones included. No tool of the extension reads, opens or hands on the content of a
downloaded file: the only methods of `chrome.downloads` it calls are `download`, `search`, `pause`,
`resume`, `cancel`, `show` and `removeFile`. `save_page_archive` ([BROWSER-40](#BROWSER-40)) puts a
file in the same folder in the same way and the same holds for it.

**Why.** A download is a request to a site with the person's cookies, so the URL in the question is
the one it may fetch from, and a redirect to a host nobody was shown is
[FETCH-4](tools/fetch-url.md#FETCH-4) for a download. The group reads no page except through
`save_page_archive`, which is held to the allowed sites by name ([BROWSER-22](#BROWSER-22)); for the
other tools the allowed sites list, which limits what a page may reveal, has nothing to limit. The
blocked list applies to all of them. No
folder or file name is an argument, since a name a session chooses could place a file outside the
Downloads folder, and no dialog asks where, so a file lands in the same folder for every call. A
download id is a number, where a path or an address is something a person can judge in the question,
and two downloads that both can be paused are two the question cannot tell apart. The routing field of a call is the alias and the
tool, as in [ROUTE-7](routing.md#ROUTE-7) and [SERVERS-7](mcp-servers.md#SERVERS-7). The content
of a downloaded file is untrusted, and keeping it out of every tool keeps it out of the driver and
the planner. A reply goes through [MCP-1](mcp.md#MCP-1), so it is untrusted and private, and holds
nothing of the file.

`verified-by: bravebot_browser::tools::a_download_is_started_by_one_url_and_acted_on_by_filename_or_url_and_nothing_else`
`verified-by: bravebot_browser::extension::the_group_table_lists_the_groups_and_each_tool_in_one`
`verified-by: bravebot_browser::relay::a_server_started_for_a_group_lists_that_groups_tools_whether_or_not_the_extension_is_connected`
`verified-by: bravebot_core::trust::a_downloads_folder_outside_the_workspace_is_untrusted_until_a_person_vouches_for_it`
`verified-by: by-construction (extension/tests/tools.test.mjs asserts that download_file passes the browser the URL as written with no dialog and no replacing and no other option, whatever folder, file name, method, header or body the call carries, that a scheme other than HTTP or HTTPS and a missing or non-text URL reach no download, that a download whose address after its redirects is on another host, at another port or at a listed origin is cancelled and a file it had finished is deleted with the one message naming the URL asked for or the list, that the same host is kept, and that a listed site is refused before the browser is asked; that list_downloads lists the newest first with the path and the address and no id, keeps the count between 1 and 100, leaves out a download that started at a listed site or ended at one without returning fewer than there are, and that pause_download, resume_download and cancel_download act on the one download they apply to by exact filename or URL and say what the browser reports afterwards, that a finished download is neither cancelled nor deleted, that a name several downloads have names the one the tool applies to and none where more than one, that a call with both names, neither or text fails before the browser is asked, that show_download shows the folder of a file the browser does not say is gone and opens no file, that a download from a listed site is refused with the one message, and that the only methods of chrome.downloads the extension calls are download, search, pause, resume, cancel, show and removeFile; it runs where the test under BROWSER-13 runs)`

<a id="BROWSER-33"></a>
### BROWSER-33: the tools of `input` act on the one element a role and an accessible name lead to, in the top document of one tab, and nothing the planner wrote is run, parsed or used as a selector

`input` is the group whose tools click, type, choose, press a key, scroll and wait for an element in a
page the person has open: `click_element`, `type_text`, `select_option`, `press_key`, `scroll_page`
and `wait_for_element`. It lists `tabs` and `scripting` and reads pages, so it needs the hosts of the
allowed sites, or the two patterns on all sites ([BROWSER-22](#BROWSER-22)). It is on until Brave
restarts unless the person chooses another time ([BROWSER-24](#BROWSER-24)) and is in no preset. It
does not use `debugger`. The server started for `input` offers the six tools, and its declaration
asks about every call ([BROWSER-21](#BROWSER-21)).

**The call.** Every tool takes `url`, the exact URL of one open tab, found as every tool that names a
tab finds it: a URL at no tab, or at more than one, fails saying how many, and a URL at a listed
site, or at no allowed site where the group is limited to them, fails before a tab is looked for
([BROWSER-13](#BROWSER-13), [BROWSER-17](#BROWSER-17)), and so does a URL that is not HTTP or HTTPS,
which a tab can have where the browser lets the extension into `file:` and `ftp:` pages or the
permissions cover them. The tools that act on an element also take
`role`, one of `button`, `link`, `textbox`, `checkbox`, `radio`, `combobox`, `menuitem`, `tab`,
`option` and `switch`, and `name`, the accessible name, at most 200 characters. `click_element`,
`type_text` and `select_option` take `element_number` in place of `role` and `name`: a whole number
from 1 to 40, the number an element has in the last list `list_page_elements` made for that URL
([BROWSER-41](#BROWSER-41)). One form is given and not both. `press_key`, `scroll_page` and
`wait_for_element` take no number, since they act on the element with the focus, on none, or wait
for one a list may not have yet. `click_element` takes
`button`, `left` or `right`, and `count`, 1 or 2, and not both a right button and a count of 2.
`type_text` takes `text`, 1 to 2,000 characters. `select_option` takes `option`, the text of the
option, at most 200 characters. `press_key` takes `key`, one of `Enter`, `Escape`, `Tab`,
`ArrowUp`, `ArrowDown`, `ArrowLeft`, `ArrowRight`, `Home`, `End`, `PageUp` and `PageDown`, alone.
`scroll_page` takes `direction`, `up`, `down`, `top` or `bottom`, and `pages`, 1 to 5. `wait_for_element`
takes `seconds`, 1 to 30, 10 where none is given. A word outside these lists and a number outside
these ranges fail before a tab or a page is reached, and so does a call that gives a number with a
role or a name. No tool takes a selector, a node id, a coordinate or a tab id, and an argument that
is not listed is dropped.

**The page.** The extension runs the functions it holds, `inPage`, `scrollInPage` and
`clearOutlineInPage` in `extension/input.js`, with `chrome.scripting.executeScript` in the top frame
of the tab, in the extension's isolated world, each given one object of known fields, and each
call is given up after 15 seconds so that a page that never lets a function finish does not hold
the tab or the one action that may wait at a time. Nothing the planner wrote is
evaluated, parsed or used as a selector or HTML: the role and the name are compared with the roles and
names the elements have, the elements are listed by walking the tree and not by a selector, and typed
text is given to the field as text. The isolated world keeps the page's scripts from changing what
the functions call, and does not keep the page's markup from answering for a node: a form answers a
property of its own, such as `action`, `method`, `elements`, `length`, `parentElement`, `children`,
`id` or `getAttribute`, with the control it holds that has that name, and the document does the
same for an image, form or frame with a name. Every function injected into a page therefore reads
a node through its prototype with the receiver the node (`pageReads` in `extension/input.js`) and
never by the node's own property, for the destination of a link or a form, for the tree, and for
every other property. An `element_number` does not reach the page: the page is given the
place in the document and the fingerprint that the list kept for it. The elements are those of the top document and of the open and
closed shadow roots in it. A frame is not entered. The role of an element is its `role` attribute, if
a word of it is on the list, or the role its tag gives it. Its accessible name is computed from
`aria-labelledby`, `aria-label`, its labels, a button's value, an image's alt text, its content for
the roles that take their name from content, and its title and placeholder, in that order, with
white space collapsed, and compared with the name given without regard to case.

**One element.** A call resolves to the one displayed element with that role and name. It fails, and
says how many matched and nothing of them, where none does or more than one does. An element is not
displayed, and is not counted, when it or an element above it is not rendered, is invisible or fully
transparent, is hidden by `hidden` or `aria-hidden`, or is one pixel or less across. Where the only
elements with that role and name are not displayed the call says so.

**One element, by number.** A call with an `element_number` resolves to the element that number had
in the list kept for that URL ([BROWSER-41](#BROWSER-41)). It is refused, before a page is reached,
where no list was made, where the list was made for another URL or is more than five minutes old, and
where the number is past the end of the list. The page then finds the element at the place the list
recorded and accepts it only if it is displayed and has the tag, role, name, place and destination
the list recorded. Any other element is refused with a sentence of ours, nothing is done and the
person is not asked. The element is then shown, outlined and acted on as for a role and a name, and
the page checks the fingerprint again at the moment of the action. A field that is a password, a
payment card field or a file input when it is acted on is refused for that, whatever it was when the
list was made.

**Fields never acted on.** A password field, a field whose `autocomplete` holds `current-password`,
`new-password` or a word that starts `cc-`, and a file input are refused by every tool that acts on an
element, `press_key` included, which acts on the element that has the focus. A disabled or read-only
element, an element covered at its middle by another that is not its label or inside it, and an
element whose middle is outside the window fail without a change to the page.

**Asked in the browser.** The tools that act on an element, `click_element`, `type_text`,
`select_option` and `press_key`, find the element and then ask the person in their browser, before
anything is done, as [BROWSER-34](#BROWSER-34) says, with `description`: the action, the tab's URL
and origin, the element's tag, role and name, the start of its text, where a click on it goes (below),
its rectangle, and the text, option or key. Every value in it is text or a number. The function that
acts, `performAction`, takes the token that window made for that resolution and refuses without one,
with a token for another resolution, or a second time, and does not reach the page. Where no window
is set, or the person denies, nothing is done. The page then finds the element again at the moment
of the action and acts only if it has the same tag, role, name, place in the document and
destination (where a click on it goes, below) as when it was found, and is still at the URL.
`scroll_page` and `wait_for_element` act on no element and are not asked in the browser.

**Where a click goes.** The page works out where a click sent to the element goes, and no word of it
is the planner's. A click goes up through the element and every element above it, as an event does,
and the first of them that decides it does: a link with an address (`a` or `area` with `href`,
which for a link in a drawing may be `xlink:href`) goes to its address, and a button that submits
a form (a `button` of any type but `button` and `reset`, an `input` of type `submit` or `image`
that has a form) sends the form to its own `formaction` if it has one and to the form's `action`
otherwise, an empty one being the page's own address. A link inside the element is not followed,
since the click is sent to the element and not to what is inside it. A field is also asked where
Enter goes: an `input` that is not a button or hidden, in a form, submits it through the first
submit button of the form, whose `formaction` takes the place of the form's `action`, or through
the form's `action` where there is no such button. Where the click and Enter go to two places, or
the element is an image with a map or an object with one, which divides the image among links the
page does not place, the element goes to a place that cannot be told, and the window and the list
say `unknown`. `list_page_elements` and the blocked list use the same place as the window: an
element whose place is at a blocked site is left out, and so is one whose place cannot be told
while any site is blocked. What a script does when the click arrives is not in it.

**The effect.** A click sends the pointer and mouse events and `click`, once or twice, at the middle
of the element; a right click sends `contextmenu`. `type_text` replaces the value of an input or text
area through the prototype's setter and sends `input` and `change`, or inserts the text into an
editing host with the editing command. `select_option` selects the one option with the text, which
a native select has. `press_key` sends `keydown`, `keypress` for `Enter`, and `keyup` to the element
that has the focus. The events are not trusted: the page can tell them from a person's. A click runs
the default action of what it is sent to, so a link around the element is followed and a form is
submitted to the address the window showed, and a key does none of what it would do by itself. A site that checks that, a file
chooser, a clipboard action, a JavaScript dialog and a page drawn on a canvas are out of reach of the
tools of `input`. The tools of `input-trusted` send real input, which the page cannot tell from a
person's, and answer a dialog that an input of theirs opened, through a second extension
([BROWSER-37](#BROWSER-37), [BROWSER-38](#BROWSER-38)), and `choose_file` of `upload-trusted` gives a
file to a file input ([BROWSER-45](#BROWSER-45)).

**What comes back.** The reply of a tool that is done holds a fixed word, such as `clicked`, and the
origin of the page, and nothing of the page or the element. A failure holds a sentence from a closed
list, and a count where it has one, and nothing the page wrote. Every call is added to the activity log
with its origin alone ([BROWSER-27](#BROWSER-27)). The reply is untrusted and private
([MCP-1](mcp.md#MCP-1)) and the planner does not read it.

**Why.** The planner holds no page content except what a person lets through with `vet_content`, and
no tool reads a page's text to it, so it does not say where to click from what is on the page. It
names a page, and a role and a name or a number that it was given, as it names a URL, and a person can
judge those in the question ([ROUTE-9](routing.md#ROUTE-9) says why that meets the routing rule). The page
decides which element the name leads to, so the extension finds it, shows it to the person where the
page cannot rewrite what is shown, and acts only on that element. A decoy with the same name makes
the match fail instead of taking the click, and a page that has only a decoy is the case the
person's answer exists for. The token keeps a code path from acting on an element nobody was shown.
The injected functions are fixed and run in the isolated world, so the page cannot change what they
call, and nothing the planner wrote can become code, a selector or markup in the page. A password, a
card field and a file chooser hold what a session should not be given, so no tool reaches them. A
frame is not entered because the extension would have to decide which of its elements to trust. The
accessible-name computation is a simplification of the browser's and differs from it in cases the
design document lists, which fail by finding no element or more than one, and not by choosing
wrongly without the person seeing the element.

`verified-by: by-construction (extension/tests/input.test.mjs asserts, against a fake page, that a click is made only after the window's token, with the element, its role, name, text, target and rectangle shown, that a name matching none or several fails with the count and no word of the page, that a hidden element is not counted, that the accessible name and the role are the ones named here, that open and closed shadow roots are entered and a frame is not, that a password field, a card field and a file chooser are refused for every action and for a key, that a covered, disabled, off-screen, swapped, moved, renamed or navigated element is not acted on, that performAction refuses without a token, with another's and a second time, that the call and the reply and the log hold no word of the page, that the injected function holds no way to run, parse or select with a string and a hostile word reaches none, that typing and choosing and keys and scrolling and waiting do as described, and that every argument is checked before a tab is reached; that over a fake page whose nodes can be clobbered, a form with a control named for each property it has is read, listed and acted on exactly as one without, with the destination of a form-submitting control the form's real action, that no function injected into a page reads a node by its name, with the three lines of pageReads in each that calls them, that a list is not held for ever by a parent that is never null, that the outline of an earlier action is found among the world's own globals, that a name, a text and a destination reach the window whole up to 2,000 characters and an element with a longer one, or a tab address longer than that, is refused before the window opens, that waiting finds an element whatever the length of its words, and that a page that never lets an injected function finish fails the call after 15 seconds and the next action is not refused as busy, and that the six tools and list_page_elements refuse a URL that is not HTTP or HTTPS before a tab or a page is reached, and that where a click goes is the address of the link around the element, of the button that is or is around it, and for a field of the form's first submit button, the innermost deciding, that a link inside the element is not followed or shown, that an image map and a field in a link to another place than its form's are unknown, that the list and the blocked sites judge by the same place, leaving out an unknown one while a site is blocked, and that an element whose place changes or stops being unknown after the window is not clicked; extension/tests/tools.test.mjs asserts the same clobbering for the links, forms and text of a page, and that a tool that manages a tab still names a tab at an address that is not on the web; it runs where the test under BROWSER-13 runs)`
`verified-by: by-construction (extension/tests/numbering.test.mjs asserts that a number resolves to the element it was given to, that two elements with one role and name are told apart by number, and that the window is told the number and the place among the displayed elements of that role and name, and is told neither for an element named by its role and name, that each of the three tools takes a number, that the window is shown the same description as for a role and a name, that a swapped, renamed, retargeted, hidden, moved or removed element, a page that left, a page that changed while the window waited, and a request for a place with no fingerprint each leave the page untouched and the person not asked, that a field turned into a password field is refused as one, and that the number is checked with the other arguments before a tab or page is reached and is not given with a role or a name; and that the blocked sites and the destination column are judged on the whole address of a link; it runs where the test under BROWSER-13 runs)`
`verified-by: bravebot_browser::extension::the_input_tools_offer_the_lists_of_words_the_extension_accepts`
`verified-by: bravebot_browser::extension::the_entries_that_wait_for_a_ruling_have_no_tool_permission_or_call`
`verified-by: bravebot_browser::tools::the_input_tools_are_offered_by_the_server_of_input_alone`
`verified-by: bravebot_browser::tools::the_input_descriptions_say_that_the_person_is_asked_in_the_browser_and_what_is_refused`
`verified-by: bravebot_browser::relay::installing_declares_only_remove_data_cookies_scripts_the_input_groups_capture_trusted_console_trusted_upload_trusted_and_screen_to_ask_about_every_call`

<a id="BROWSER-34"></a>
### BROWSER-34: an action on an element is done only after the person allows it in a window of the extension, and nothing a web page does can answer the question, hide it or change what it shows

The tools that act on an element, `click_element`, `type_text`, `select_option` and `press_key`
([BROWSER-33](#BROWSER-33)), put the element they found to the person before they do anything to
the page. `scroll_page` and `wait_for_element` act on no element and open no window.

**The question.** The tab is made the active tab of its window, the element is brought into view and
outlined, and a popup window of the extension's page `confirm.html` opens in front, 600 by 640. It
shows, as text, the action in words (click, double-click, right-click, type this text, choose this
option, press this key), the text, option or key in full in a frame between quotation marks with its
number of characters, the origin and the address of the tab, and the element's role, tag, name, the
start of its text and where a click on it goes ([BROWSER-33](#BROWSER-33)), or the window's own
word `unknown` in a box of its own where that cannot be told. A page chose the last five. When the element was
named by the number a list gave it ([BROWSER-41](#BROWSER-41)), the window also shows that number and
which of the displayed elements with the same role and name it is, such as `2 in the last list, 2 of 3
with this role and name`, so that two elements the page words alike are told apart in the window and
not only by the outline. These are whole numbers the extension made and words of the window's own, and
a number or a place that is not whole numbers cannot be shown. Each of the five is given to the
window whole, and the window cuts it to a fixed length (200 characters for the name and the tag,
80 for the text, 200 for where it goes, 300 for the address) and says how many characters it left
out. A name, text or destination of more than 2,000 characters, and an address of more than 2,000,
is not given to the window: the call is refused before the window opens, with a sentence that says
so, since the window can say how many characters it left out only of a word it was given whole.
Every control, format, line and paragraph separator, private use and unpaired surrogate character,
every space but the plain one, and every default ignorable character is shown as its code point in
an element of its own that the page's characters cannot imitate. The default ignorable characters
include the variation selectors, the Mongolian free variation selectors and the Khmer inherent
vowels, which are combining marks that draw nothing and that a name in JavaScript may hold, so two
names that look alike can be two variables. Every character that `draws_as_nothing` in
`crates/core/src/invisible.rs` lists is among those shown, so the window marks what the terminal's
question marks. The list is held in `crates/core/tests/draws-as-nothing.json`, which a test of that
function and a test of both copies of `shown.js` each read. The object replacement character U+FFFC
is the one character the file lists as left unmarked, in the window and in the terminal's question:
it draws as a visible box, so a person sees that something is there. A combining mark is shown as
its code point too unless it follows a visible character other than a space and is one of the first
three after it, and so is the combining grapheme joiner, which draws nothing. Every row of the page's words is cut off at its own box, so
what a character draws beyond the row, above or below it, does not reach another row, the address
or the destination. A row of the element is at most five lines tall and scrolls inside its box,
which shows its scroll bar whenever the row holds more than the box does, so a word of 200
combining marks, each in a box of its own, takes five lines of the window and the rest is reached
by scrolling the row. The site and the address are each in a box of 12em, which is 168 pixels. The
window shows at most 300 characters of an address, and a browser reports an address in ASCII
(`chrome.tabs.query` in Brave 1.98.63 returned a zero width space, a combining mark, a right-to-left
override and a variation selector in the fragment, a right-to-left override and a variation selector
in the path, and a combining mark in the query, each as percent escapes). 300 of the widest letter
take 142 pixels in a window 600 wide, so the address as the window shows it is not behind a scroll
bar, and for `run_script`, which refuses an address of more than 300 characters, that is the whole
address. A row of 300 marks, which a browser's address does not hold, scrolls in
its box and takes 174 pixels of the window. Where a click goes is the first row of the element,
above the role, the tag, the name and the text, and the text, option or key a call types is shown
below the element, so that none of them is between the top of the window and the destination. A line
break, a right-to-left override, a character with no width or a stack of combining marks cannot
start a sentence, reorder a word, draw over another row or hide in a name. Text to type is shown whole. A description the window cannot show whole (a
text over 2,000 characters, a field missing, an action it does not name) cannot be allowed. The
window says that the outline is a hint and that the facts are in the window. An action that acts on
no element, which `run_script` has in this extension ([BROWSER-46](#BROWSER-46)) and the picture and
the console read have in the second ([BROWSER-43](#BROWSER-43), [BROWSER-44](#BROWSER-44)), has no
such block: the window does not show the heading, the rows or the note about the outline for it. The row of an
action in each extension's `actions.js` says whether the action acts on an element, and the window
reads that row, so an action is added to the table with its answer.

**The words are text.** The window's script puts a word of the page in the window with
`replaceChildren` and text nodes and never with `innerHTML`, `outerHTML`, `insertAdjacentHTML`,
`document.write`, `eval` or `Function`. Its page has no inline script, style or handler, no image,
frame, form or font, and a content security policy of `default-src 'none'; script-src 'self';
style-src 'self'`. The manifest lists no web accessible resource and no content script, and the
window carries no icon or image of the page, so no web page can open it, frame it or send it a
message.

**The answer.** The window has Deny, which has the focus, and Allow once. Allow once is not
available until a second after the window is shown or comes back to the front, and not while it is
out of front, so a click made as the window appears under the pointer, or after a page took the
front, does nothing. When the box that holds the text, option, key, code or path holds more than it
shows, Allow once is not available until the box has been scrolled to its end, and the status line
says so. A box that has been scrolled to its end stays read until the window is made another size,
which starts the reading again. A box that holds no more than it shows leaves Allow once as the
second sets it, and a box that is not laid out is not read. It takes a click or a key the person made: a click a script made, or made with
no user activation, does nothing. Deny, Escape, closing the window and no answer within 60 seconds
each deny, and a denial says which in a sentence of ours. The answer goes to the service worker over
a port, and the worker takes it from one port: of this extension, with the name of the window's
port, from `confirm.html` in its top frame, in the window it opened, while the question waits, and
only a message `answer` with `allow` true allows. Allow once is the only way to the token
([BROWSER-33](#BROWSER-33)), and the token is for that resolution alone.

**The foot.** The line that says how long the person has, the status and the two buttons are one bar
at the foot of the window, which stays in view while the rest of the page scrolls under it, so Deny
and Allow once are in view however much the page's words fill. Brave 1.98.62 gave a window made 480
by 640 a viewport 568 pixels high, and the page of a description with the longest words came to 820
to 970 pixels, so without the bar the buttons were 250 to 400 pixels below the viewport. In a
window 600 by 640 with the notice, the viewport is 552 pixels high, and the destination is at 167 to
189 pixels for a name of a letter and 200 marks, a tag of 198 marks and a text of 80 marks, at 288 to
364 pixels for an address of 300 of the widest letter and a destination of 187, and at 411 to 486
pixels for a text of 2,000 characters typed with the marks above, a site of 292 characters, that
address and that destination. The bar is 84 pixels high and starts at 468 or 469, or is 109 pixels
high and starts at 443 while the note that a box has not been scrolled to its end shows, and the
buttons are at 506 to 540 in each of them. In the last case the first two lines of the destination
are above the bar and the rest is reached by scrolling the page.

**The browser's notice.** While an extension debugs the browser, Brave 1.98.63 shows a bar across
the top of each popup window, as wide as the window and 56 pixels high, that reads `"BraveBot
Trusted Input" started debugging this browser` with a Cancel button and a close button. The sentence
takes about 350 pixels and the buttons about 110. A window 480 wide cut it to `...Bot Trusted
Input" started debugging this browser`, and a window 560 wide read it in full. The window is made
600 by 640, which leaves room for a font wider than the one measured and a viewport of 552 pixels
below the notice. The notice is Brave's and the page cannot hide or restyle it.

**One at a time.** A second action while one waits is refused with a sentence saying why, reaches no
page and opens no window, and the same for `press_key`. It is refused and not queued, so that no
question waits behind another and none runs past the relay's wait. The first is not disturbed. The
place in line is this extension's, and a call of `input-trusted` that this extension has sent to the
second extension ([BROWSER-38](#BROWSER-38)) holds it until the reply comes, so one question is open
across both extensions.

**The service worker.** While the person decides, the worker is kept running by the open native
messaging port and by a message from the window every 15 seconds. A worker that stops ends the
question: the call is lost with the worker's connection to the host, the window finds its port
closed, says that nothing was done and closes, a window left open is closed when the next worker
starts, and a late answer from such a window reaches no question. A browser that does not say whether it
opened the window does not hold the call: the 60 seconds end the question, no window it has not
reported is removed, and a window that opens after the question has ended is sent away and closes
itself. A browser that does not answer the closing of the window holds the end of the call for 1
second and no longer, so the person's 60 seconds, that second, and the bounds of the second
extension's part of a call (5 seconds before the window, 11 after the answer and 2 to detach) come to
79 of the 80 seconds the extension waits for the second extension ([BROWSER-6](#BROWSER-6),
[BROWSER-38](#BROWSER-38)). The relay waits 90 seconds for
the extension's reply to a call to these tools, 30 more than the 60 the person has, for finding the
element and acting ([BROWSER-6](#BROWSER-6)), so it does not fail a call while the window waits.

**The outline.** It is an element the extension adds to the page, with a closed shadow root that
holds one box and no text, positioned over the element, taking no pointer event and above the page.
The page cannot read or restyle what is in the root. It is removed before the action and whichever
way the question ends, and hides itself after 70 seconds if the worker never removes it. A page can
hide the element it sits in or draw an outline of its own, so the approval does not depend on it.

**Why.** A page can write the name of an element, its text, where it goes and the tag, and can move
or replace the element, and it can try to look like BraveBot's own question. The question therefore
is in a window the page cannot reach, draws the page's words as text a person can see the shape of,
and is answered by a click only the person can make, a second after it is in front. A button named
"Cancel" that submits a payment is shown with the address its form goes to. A timeout, a worker that
stops and a closed window each leave without an allow, so nothing is done in any of them. With one
question at a time, a page or a session cannot put a question behind another. The extension cannot
see what a page's own script does when the click arrives, so the window shows the element and its
destination and not the effect.

`verified-by: by-construction (extension/tests/confirmation.test.mjs asserts, with the page of the window and the worker both run for real over a fake browser, that the window opens as a small popup of the extension's page and shows the action, the page and the element as text, that Deny has the focus and Allow once is not available for a second after the window is shown or comes back to the front and not out of front, that a click a script made or with no user activation does not allow, that Allow once gives a token for that resolution once and closes the window, that Deny, Escape, a closed window, a closed port and the time limit each deny with their reason and give no token, that an answer other than allow true denies, that a second question is refused and the first is not disturbed, that a window that cannot be opened is a failure, that a window the browser never says it opened does not hold the question, which the time limit ends, and a window that opens later is sent away and closes itself, that a browser which does not answer the closing of the window holds the end of the question for CLOSE_WAIT_MS and no longer whether the person allowed or denied, that only the window opened for the question can answer, once, and a window with no question or from before a restart is sent away, that a worker that stops leaves a denial and the window ends itself, that the window sends a message every 15 seconds while it waits and stops when answered, that markup in a name is shown as text and control, direction and invisible characters are marked with their code points, that every word is cut to its limit and typed text is shown whole, that a description the window cannot show whole cannot be allowed, that windows an earlier worker left are closed, that the scripts never turn a string into markup or code, that the page loads nothing and has no inline script, style or handler, that the wait line, the status and the buttons are one bar fixed at the foot of the window, that Allow once is not available until a box that scrolls has been scrolled to its end, with the second still counted, the box read after a scroll back or after the window leaves the front and returns, and the reading started again by a window made another size, that a box that is not laid out is not read while a box that holds what it shows and the hidden box of a call that carries nothing leave Allow once as it was, that a combining mark after a letter is text up to three times and every other one is marked, a name of a letter and 200 marks being a letter, three marks and the rest marked, while accented, Vietnamese, Thai and Arabic text keep their marks, that a variation selector inside a name in the code of run_script is written out, that the site and the address are in boxes taller than five lines and not taller than 16em, that every row of the page's words is cut off at its box, that the destination comes before the role, tag, name and text, which are each a box of at most five lines that scrolls and shows its scroll bar, and the typed text is below them, that the window is wide enough for the browser's debugging notice, that a destination that cannot be told is the window's own mark and a page's word of the same letters is text and a note that is not a plain yes cannot be shown, that an element named by its number is shown with that number and its place among the elements of its name and one named otherwise with neither, that a number or place that is not whole numbers cannot be allowed, that every action of this extension but run_script acts on an element, says so in its row and shows its block, and that run_script acts on none and shows the code whole as lines instead (BROWSER-46), that the heading and the title name the extension by its manifest, that a confirmation made for another page opens that page and takes an answer from that page alone, and that the manifest has no web accessible resource, content script or external connection; extension/tests/unseen.test.mjs asserts, for both copies of shown.js, that each kind of character a person cannot see is marked by its code point, that every character in the ranges of crates/core/tests/draws-as-nothing.json, which bravebot_core::invisible::the_list_holds_every_block_that_draws_as_nothing_and_no_ordinary_character holds draws_as_nothing to for every code point, is marked after a letter and after a combining mark, that the character the file lists as left unmarked, U+FFFC, is shown as it is, and that the variation selectors, the Mongolian free variation selectors and the Khmer inherent vowels, which a name in JavaScript may hold, are marked in code; extension/tests/shared.test.mjs asserts that the files the window shares with the second extension are listed, import only each other and the files each extension writes for itself, and, where the second extension holds a copy, are the same bytes in it; extension/tests/input.test.mjs asserts that the outline is a closed shadow root with no word of the page and no pointer event, placed at the element, that it is removed whichever way the question ends and before the click, that the tab is brought forward before the window asks, that only one action waits at a time, that where a link or form goes is part of what was shown, and that the page is reached only by the three fixed functions; extension/tests/trusted.test.mjs asserts that a call waiting for the second extension holds the place, so that another call of the group and an action on an element are refused and send nothing, and that the place is free again however the call ended; extension/tests/options.test.mjs asserts what the options page says of the window; all run where the test under BROWSER-13 runs)`
`verified-by: bravebot_browser::extension::the_wait_for_input_is_longer_than_the_persons_and_the_declared_bound_is_longer_still`
`verified-by: bravebot_browser::server::a_call_to_input_waits_for_the_extension_as_long_as_a_person_does_and_others_do_not`

<a id="BROWSER-35"></a>
### BROWSER-35: screenshot_page takes a picture of the visible tab at one URL, only with access to all sites, and returns the picture and nothing about the page

`screenshot_page` is in `page-read`. It takes the `url` of a tab, as `read_page` does, and returns a
JPEG of what that tab's window shows. Its routing field is `brave-page-read:screenshot_page` and its
one argument is the URL the question before the call shows, so a person can approve the field and
the argument alone.

**Access to all sites.** The browser lets an extension capture a tab only when it holds the host
permission `<all_urls>`. Brave 1.98.62 refused `chrome.tabs.captureVisibleTab` when the extension
held the two patterns `https://*/*` and `http://*/*`, `*://*/*`, the pattern of the page's own
origin or `activeTab`, and took the capture with `<all_urls>`
([the design](../design/browser-extension.md#taking-a-picture-of-a-page)). So `page-read` needs
`<all_urls>` in place of the two patterns when its scope is all sites ([BROWSER-22](#BROWSER-22)),
and no other group's scope asks for it except that of `screen`, whose `record_gif` takes a picture of
a tab in the same way ([BROWSER-42](#BROWSER-42)). The manifest offers `<all_urls>` as an optional host beside
the two patterns, which it contains, and Brave words it as the two patterns, "Read and change all
your data on all websites". Under the scope of the allowed sites the call is refused, whatever the
browser holds, with a message that says the group is limited to the allowed sites and that the
browser lets an extension capture a tab only with access to all sites. Where the group is on all
sites and the browser does not hold `<all_urls>`, the refusal for a group not held
([BROWSER-18](#BROWSER-18)) says the same of the group. The Read and Browse presets leave `page-read`
on the allowed sites, so a session on either takes no picture, and the options page says so in the
text of the group, under each scope and in the line of each of the two presets.

**What the extension checks.** `<all_urls>` is wider than the person's lists, so the extension's own
checks limit the call, and each comes before anything is captured. The `url` must be an HTTP or
HTTPS URL, since the lists name web origins. A URL at a blocked site is refused before a tab is
looked for ([BROWSER-17](#BROWSER-17)). The tab is found by its exact URL by the function every tool
uses, so a URL at more than one tab names none ([BROWSER-13](#BROWSER-13)), whichever of them is
visible. The tab must be the active tab of its window, and its window must not be minimized, or the
call fails and says which. The tab is not brought forward, since that changes what the person sees,
and `focus_tab` in `tabs-manage` does it. `captureVisibleTab` takes a window and no tab, so the tab
is read again just before the capture and just after it, and a tab that has left the URL or stopped
being the visible one gives no picture.

**Pace and size.** The browser rejects a third capture in a rolling second. The extension takes the
captures of one browser one at a time and at least 600 milliseconds apart, and a rejection by the
browser is told as one to call again. The picture is the JPEG at quality 80 that the browser makes,
which was 0.02 to 1.6 MB decoded for a page 2048 pixels wide where a PNG was 4 to 7.4 MiB. A picture over 8 MiB
decoded, the size [MCP-6](mcp.md#MCP-6) takes from a tool, is refused with a message that says so,
and a picture is never cut. Only a `data:image/jpeg;base64,` URL from the browser is passed on.

**The result.** `{"image": {"mimeType": "image/jpeg", "data": <base64>}}` and no other field: no
text, title or URL of the page. A failure says what happened to the window, the tab, the permission
or the quota and nothing about the page. The activity log keeps the group, the tool, the outcome and
the origin of the call ([BROWSER-27](#BROWSER-27)) and never the picture.

**The relay.** The table marks `screenshot_page` as a tool whose result is a picture. The server
answers a call to it with one MCP `image` part whose `data` and `mimeType` are those of the `image`
object in the extension's reply, carried as they are. That the result is a picture is decided by the
table before the reply is looked at, and a reply that has no such object gives a part whose fields
are null, which the MCP client leaves out and counts. Nothing else of the reply is passed on. A
failure is text, as for every tool. The description says that the result is a picture the model does
not see, that it becomes available to the model only through `spawn_processor` or `vet_content`
after a person looked at it, that the tab must be the visible one, that the group must be on all
sites and why, and that a frame of another site is in the picture.

**Where the picture goes.** It is a tool result, so it is untrusted and private ([MCP-1](mcp.md#MCP-1)),
held in a quarantined slot of its own, and the planner is given a reference that names its type and
size ([MCP-6](mcp.md#MCP-6)). A processor can be given it as a picture
([PROC-15](processors.md#PROC-15)), and it reaches the planner only through `vet_content`
([VET-4](tools/vet-content.md#VET-4)).

**Why.** A maintainer ruled that a screenshot is supported under the vetting rules that already
hold for a picture: the screen is an untrusted reference, and the model looks at one only after its
content is vetted. The browser allows the capture with one permission, so the group asks for that
one and says so where the person turns it on. The extension keeps the picture to a tab named by a
URL the person read, because the browser's own permission names every site. The relay decides
nothing about what it relays. It reads from an extension's reply whether it holds an `error` or a
`result`, the `message` of an error, and whether a result is a string or other JSON, which it writes
as text. Whether a result is a picture comes from a table the relay was built with, and the client
decides what of it is taken.

`verified-by: bravebot_browser::extension::the_extension_captures_a_tab_in_one_call_with_all_urls_and_within_the_clients_cap`
`verified-by: bravebot_browser::extension::the_extension_has_each_permission_its_tools_require`
`verified-by: bravebot_browser::tools::the_tools_that_return_a_picture_are_the_four_and_take_a_url_a_count_or_nothing`
`verified-by: bravebot_browser::tools::the_screenshot_description_says_the_picture_is_not_seen_by_the_model_and_what_it_shows`
`verified-by: bravebot_browser::server::a_picture_tool_is_answered_with_an_image_part_made_of_two_fields_of_the_reply`
`verified-by: bravebot_browser::server::the_table_and_not_the_reply_decides_whether_a_result_is_an_image`
`verified-by: bravebot_browser::server::a_picture_tool_that_fails_is_a_text_failure`
`verified-by: bravebot_browser::relay::a_screenshot_is_called_through_its_group_alone_and_arrives_as_an_image_part`
`verified-by: bravebot_browser::relay::a_screenshot_answered_with_no_picture_is_an_empty_image_part_and_a_failure_is_text`
`verified-by: by-construction (extension/tests/tools.test.mjs asserts that screenshot_page returns the JPEG of the visible tab at the URL, taken as a JPEG at quality 80 of the window of that tab, with the reply holding the picture and nothing of the page and the activity log holding the origin and not the picture; that it is refused under the scope of the allowed sites even at an allowed site and with <all_urls> held, and where the browser holds the two patterns of all sites or nothing, each time saying that the browser lets an extension capture a tab only with access to all sites; that it is refused for a tab its window does not show, without bringing the tab forward, for a minimized window, for two tabs at the URL, for a blocked site before a tab is looked for and for a URL that is not HTTP or HTTPS; that two captures that arrive together are 600 ms apart; that the browser's quota is told as a call to make again and another refusal in the browser's words; that a picture of 8 MiB is returned and one a byte over is refused, with padding counted; that a picture of a tab that left the URL or stopped being visible while it was taken is dropped; and that only a JPEG data URL is passed on; extension/tests/groups.test.mjs asserts that only page-read needs <all_urls> and only on all sites and that the manifest offers every host a group can need; extension/tests/warnings.test.mjs asserts that <all_urls> warns as the two patterns do; extension/tests/options.test.mjs asserts that the page says a picture needs all sites in the group, under each scope and in the two presets; all run where the test under BROWSER-13 runs)`

<a id="BROWSER-36"></a>
### BROWSER-36: `profile` makes a user data directory of its own for Brave and prints the command that starts Brave on it with the extension loaded

`bravebot-browser profile --profile-dir <dir> --extension-dir <dir> [--brave <path>]` is run by a
person in a terminal. It makes the profile directory if it is not there, private to the account,
writes a file `bravebot-profile` in it, and prints one line to standard output: the command that
starts Brave with `--user-data-dir=<profile>`, `--load-extension=<extension>`, `--no-first-run` and
`--no-default-browser-check`. Both directories are printed as absolute paths, and every word is
quoted for a POSIX shell, so a name with a space, a quote or a leading dash, or with bytes that are
not text, reaches Brave as the name it has. A word that is not one of the three flags, a flag given
twice, and a missing profile or extension are answered with the usage, and nothing is made.

The two directories and the Brave are read from the command line as the bytes it holds, and a file
system that refuses such a name refuses it as any other failure, with the exit code 1. The native
host, `mcp` and `install` take text. An argument of theirs that is not UTF-8 is answered with a
sentence saying so and the exit code 2, and nothing is written, so that no word is rewritten or
dropped.

The command is printed and not run. Starting a program is done by `bravebot-sandbox` alone
([SANDBOX-30](sandboxing.md#SANDBOX-30)) and this crate links no workspace crate
([layering.md](layering.md)), so the person runs the line. No tool offers it and nothing in
BraveBot runs it. It reads no message of the relay and is outside the path a call takes.

**Which directory.** The profile directory is refused when it is a link or not a directory, and when
it holds `Default` or `Local State` and no `bravebot-profile`. Brave leaves those in a directory it
has run on, so such a directory is the profile of a browser someone uses, and the extension is not
loaded into it. A directory this command made keeps its marker and is used again after Brave has run
on it. A link in the place of the marker is not the marker. `--dir` is a write grant of the
confined server ([BROWSER-2](#BROWSER-2)), so the documentation puts the profile outside the
relay's directory, where the server has no write grant.

**What else is refused.** The extension directory must hold `manifest.json`, and its path must have
no comma, since Brave reads `--load-extension` as a list separated by commas. Brave is the file
`--brave` names. Without it, on macOS it is `/Applications/Brave Browser.app/Contents/MacOS/Brave
Browser`, and elsewhere it is the first executable file named `brave-browser` or `brave`, in that
order, in an absolute entry of `PATH`. Nothing else is searched. When any of this is refused nothing
is made or written, and the profile directory is not made.

**The relay.** The profile has no host manifest of its own. Brave reads the manifest `install`
writes ([BROWSER-9](#BROWSER-9)) from one place whatever user data directory it runs on:
`BraveMainDelegate::PreSandboxStartup` in brave-core (master, read 2026-10-31) points both the
per-user and the system-wide directory at Chrome's. Brave 1.98.62 on macOS started the host from a
manifest in Chrome's per-user directory and did not find the same manifest in `NativeMessagingHosts`
of the user data directory it was started on. The extension in the profile therefore reaches the
relay that `install` set up, and `install` is run first. The command inherits the environment of the
terminal, so a `BRAVEBOT_BROWSER_DIR` set there reaches the host Brave starts.

**What the profile holds.** It is a new Brave profile. It has none of the history, bookmarks,
cookies, logins, sync chain or settings of another, and `--no-first-run` skips the first-run tasks
of Brave, which include the offer to import them. The extension's switches, lists of sites and activity log are that profile's own, so
every group is off in it until it is turned on in that profile's options page
([BROWSER-19](#BROWSER-19)). Only one Brave profile is connected to the relay at a time
([Known costs](#known-costs)).

**A Brave already on the directory.** Where the directory holds `SingletonLock`, the line is still
printed and a note on standard error says a Brave may already be running on it. The link outlasts a
Brave that stopped without removing it, so the note says "may". A Brave started on a directory that
one is running on does not start a second browser: headless Brave 1.98.62 stopped with "Failed to
create a ProcessSingleton" and exit code 21.

**Why.** A signed-in profile holds a person's sessions, and a tool that reads pages or opens tabs
there acts as that person at every site. A profile with nothing in it gives a session a browser
that has no such sessions, and the groups still have to be turned on in it. A directory that holds a
profile of a browser someone uses is the one place the extension could be loaded by mistake with the
person's data around it, so it is refused by what Brave leaves there, and the marker is what makes a
directory this command made usable again. The command is built as words and quoted once, and not
assembled from text, so a directory named with a quote or a dollar sign cannot become another
command in the person's shell.

`verified-by: bravebot_browser::clean_profile::profile_prints_the_line_that_starts_brave_on_a_directory_of_its_own_with_the_extension_loaded`
`verified-by: bravebot_browser::clean_profile::profile_refuses_a_directory_that_holds_someone_elses_profile_and_writes_nothing`
`verified-by: bravebot_browser::clean_profile::profile_runs_again_on_a_directory_it_made_after_a_browser_has_used_it`
`verified-by: bravebot_browser::clean_profile::profile_makes_no_directory_for_an_extension_brave_would_not_load_or_a_brave_that_is_not_there`
`verified-by: bravebot_browser::clean_profile::the_line_is_the_same_words_to_a_shell`
`verified-by: bravebot_browser::clean_profile::profile_given_a_directory_whose_name_is_not_text_takes_the_bytes_it_has`
`verified-by: bravebot_browser::clean_profile::profile_given_an_extension_or_a_brave_whose_name_is_not_text_refuses_it_and_makes_nothing`
`verified-by: bravebot_browser::clean_profile::a_subcommand_that_takes_text_refuses_a_word_that_is_not_text_and_writes_nothing`
`verified-by: bravebot_browser::clean_profile::profile_says_a_brave_may_already_be_running_where_the_profile_holds_its_lock`
`verified-by: bravebot_browser::clean_profile::profile_given_words_it_does_not_take_prints_the_usage`
`verified-by: bravebot_browser::clean_profile::profile_says_how_to_name_a_brave_when_none_is_on_path`
`verified-by: bravebot_browser::profile::the_command_is_one_word_for_the_program_and_each_switch`
`verified-by: bravebot_browser::profile::a_word_is_quoted_only_where_a_shell_would_split_or_expand_it`
`verified-by: bravebot_browser::profile::a_directory_holding_a_profile_this_command_did_not_make_is_refused`
`verified-by: bravebot_browser::profile::a_link_in_place_of_the_marker_does_not_mark_a_profile`
`verified-by: bravebot_browser::profile::brave_is_found_at_the_places_each_platform_keeps_it_and_nowhere_else`
`verified-by: bravebot_browser::profile::a_brave_that_is_named_is_the_one_used`
`verified-by: bravebot_browser::profile::an_extension_directory_needs_a_manifest_and_no_comma`
`verified-by: bravebot_browser::profile::profile_takes_the_two_directories_and_a_brave_and_no_other_word`

<a id="BROWSER-37"></a>
### BROWSER-37: the second extension answers requests from the BraveBot extension alone, holds the debugger and no other permission, and uses the debugger in one place

`extension-trusted/` is a second extension, "BraveBot Trusted Input", loaded unpacked beside the
first. It is never a native messaging client and never starts the host, so the BraveBot extension
stays the only one ([BROWSER-4](#BROWSER-4), [BROWSER-5](#BROWSER-5), [BROWSER-9](#BROWSER-9)). It
answers a message the BraveBot extension sends it, for eight methods, `click_element_trusted`,
`type_text_trusted`, `press_key_trusted`, `hover_element`, `drag_element`, `screenshot_full_page`,
`read_console` and `choose_file`.

**The manifest.** `permissions` is exactly `["debugger"]`. The manifest declares no host, optional
permission, content script, web accessible resource, command or options page, and neither
`nativeMessaging` nor `storage`. `externally_connectable` holds `ids`, which is the id of the
BraveBot extension, and no `matches`, so the browser delivers a message from that extension alone
and from no web page and no other extension. The policy of its pages is `default-src 'none'` with
scripts and styles from the extension itself. Brave lists two lines when it is installed, that it
accesses the page debugger backend and that it reads and changes data on all websites. Its popup is
static: it says what the extension is, whose requests it answers, and that it is turned off at
`brave://extensions`.

**The ids.** Each extension's id is the one its manifest key gives. `pair.js` holds the BraveBot
extension's id, which the service worker compares a sender with and the manifest lets message it,
and this extension's own, which `extension/trusted.js` holds as `SECOND_ID` and the BraveBot
extension sends to. A test derives each id from its key and holds all four places to it. Only the
public key is in the repository. A copy of either extension made with another key, such as one from the Chrome Web
Store, has another id and is refused by the other until both ids are changed.

**The door.** The service worker registers `onMessageExternal` once, at the top level, because the
browser reports an event only to a listener registered as the worker starts. It replies to a
sender whose id is the BraveBot extension's and that has no tab, and for any other sender it returns
with no reply of any content. The BraveBot extension asks from its service worker, which has no
tab. A script of the BraveBot extension that runs in a page has its id and a tab, and is not
answered, whatever its frame and address say: its content is what the page's content can work
beside, and nothing in the BraveBot extension sends from one. The address and frame of a sender
belong to a tab and are not read. The one other listener is `onConnect`, for a page of the extension's own: the
confirmation window and the dialog panel of [BROWSER-38](#BROWSER-38) connect to the worker, and
`windows.js` hands each connection to the window it belongs to. The manifest has no content script
and no page that may connect, so the browser gives `onConnect` the extension's own pages alone, and
a sender from outside is still the BraveBot extension alone. Only those two pages open a
connection, no script sends a message, and no script registers a listener for any other message or
connection. A request is
`{v: 1, method, params}` and nothing else. Each argument is checked before anything is touched: a
role, key, button or count is a word or number of the lists the BraveBot extension checks against
([BROWSER-33](#BROWSER-33)), a name is at most 200 characters, a text is 1 to 2,000, an address is
a page on the web, a right click is made once, and an argument the method does not take, such as a
selector, a node number, a tab number or a coordinate, is refused and not read. One request is
answered at a time, and a second while one runs is refused and starts nothing. A reply is
`{v: 1, result: {status, origin}}`, with one status word from a closed list and the origin of the
address the request named, or, for `screenshot_full_page`, `{v: 1, result: {image: {mimeType, data}}}` ([BROWSER-43](#BROWSER-43)), or, for `read_console`, `{v: 1, result: {origin, omitted, entries}}`
([BROWSER-44](#BROWSER-44)), or `{v: 1, error: {kind, message}}`, with `refusal` or `failed` and a
sentence from a closed list that may hold the name of an argument or a count. It holds no word of
a page, except the entries of a console in the report of `read_console`, and none the request wrote,
and a message an error carries is never sent.

**One place.** `cdp.js` is the only file that names the debugger's API, and `command()` there is the only
function that sends the browser a method. The list of methods is frozen. It holds the reads
`Accessibility.enable`, `Accessibility.getFullAXTree`, `CSS.enable`, `CSS.getComputedStyleForNode`,
`DOM.enable`, `DOM.getBoxModel`, `DOM.getFlattenedDocument`, `DOM.getNodeForLocation`,
`DOM.scrollIntoViewIfNeeded`, `Page.bringToFront`, `Page.enable`, `Page.getFrameTree` and
`Page.getLayoutMetrics`, and the methods that send input, `DOM.focus`, `Input.dispatchKeyEvent`,
`Input.dispatchMouseEvent` and `Input.insertText`, `DOM.setFileInputFiles`, which gives one file to a file
input and is sent in the way [BROWSER-45](#BROWSER-45) says, `Console.enable` and `Log.enable`, which start the
browser's report of the page's console and are sent only with a token as input is
([BROWSER-44](#BROWSER-44)), `Page.captureScreenshot`, which reads the page's
pixels and is sent only with a token as input is ([BROWSER-43](#BROWSER-43)), and `Page.handleJavaScriptDialog`, which answers
a dialog and is sent in the way [BROWSER-38](#BROWSER-38) says. It holds no method
of `Runtime`, so nothing, not even a fixed function, runs in a page, and none of `Network`,
`Storage`, `Fetch`, `Target`, `Browser` or `Emulation`, nor `Page.navigate`, `Page.printToPDF` or
`Input.dispatchTouchEvent`. A method that sends input is sent only with a
token that the approval given to the session accepts by answering `true`, and a session given no
approval accepts none. No input is sent before `Page.enable` has been sent in the session, since the
browser lets only a session that had Page enabled when a dialog opened answer it. A session is made
for one request and listens for three events of the debugger, for its own tab:
`Page.javascriptDialogOpening`, from which it reads the kind, the message and the suggested answer of
the dialog and nothing else, and `Console.messageAdded` and `Log.entryAdded` while a read of the
console runs, which `cdp.js` hands to the tool whole and reads none of. It stops listening when it
ends. It is attached at protocol version 1.3
to the one tab of type `page` at the request's address, character for character, and detached in a
`finally` however the request ends. None or several tabs at the address fails with the count and
names no tab. The debugger lists the address and title of every tab, and no script returns, logs,
stores or sends them, fetches, evaluates a string or uses another extension API.

**Why.** An optional permission cannot be `debugger`, so there is no later grant for the browser to
hold off: while this extension is turned on it holds the debugger on every site, and turning it
off at `brave://extensions` is the only browser-held off state. The extension is therefore made
small and closed, so that what holds the capability can be read in one sitting. It is the second
door into an extension next to the native port ([BROWSER-12](#BROWSER-12)), so the door is narrow:
the browser refuses every id but one, the worker checks the id again, a request is a closed set of
words and bounded strings, and the reply carries no page's words to the sender but the entries of a
console, which the sender checks and passes on as text the model cannot read directly ([BROWSER-44](#BROWSER-44)). The debugger
is used in one file with one list of methods, which is the reason a scan of the code can say that
nothing evaluates in a page and that input is sent from one function. Input is gated by a token and
not by a check at each caller, so a caller added later cannot send it by leaving the check out.

`verified-by: by-construction (extension-trusted/tests/manifest.test.mjs asserts that the permissions are the debugger alone and the manifest has no other key that grants a host, a page, a script or a command, that externally_connectable names the BraveBot extension's id and nothing else, that each id is the one its key gives, that the key is a public RSA key and no private key or key file is in the directory, that the policy of the pages allows scripts and styles from the extension alone, that the popup runs one script and no inline code, and that the least browser version has the calls this extension uses and is not newer than the BraveBot extension's; extension-trusted/tests/background.test.mjs asserts that the worker registers its listeners at the top level, one for external messages and one each for the connections and the removed windows of its own two windows, that the scripts register a listener for an event of the browser in three files only, answers the BraveBot extension, and gives any other sender no reply and reaches nothing, a script of the BraveBot extension with a tab included whatever its frame and address, and that it sets its windows up as it starts and detaches the debugger from every tab it lists as attached, which a worker that stopped left, and from no other, before a call that comes with the start attaches; extension-trusted/tests/requests.test.mjs asserts that the table holds the eight methods, that a method gets the arguments it takes and no others, that a method, a message, an argument or an extra argument outside the lists and bounds is refused with a sentence that names no word of the request, that the lists are the BraveBot extension's, that a reply holds a status word and an origin or a sentence of the table and never an error's own words, that one request runs at a time, and that each method runs the tool of its name; extension-trusted/tests/cdp.test.mjs asserts that the list of methods is frozen and has no Runtime method, that a method off the list is never sent, that a method that sends input needs a token the approval accepts by answering true and with no approval is never sent, that no input is sent before Page is enabled, that a session attaches once at version 1.3 and detaches when the work returns or throws or a command throws and listens for the debugger's events only while it lasts, that an ended or forged session sends nothing, that a session reads a dialog's kind, words and suggested answer from the event for its own tab and nothing else, that input which opens a dialog is not waited for past the dialog, that a dialog is answered with the person's answer alone and only once, that the answer to a dialog is named in cdp.js alone and a token is made in the window code alone, that the tab is the one page at the address and none or several fails with the count and no address or title, and that the debugger and the sending of a method appear once in the code and no script evaluates, fetches, stores, logs or names a Runtime method; make check-extension runs them, and check-ui-build depends on that target, so the Front end CI job runs them on every change the classifier gives the ui area, which a change under extension-trusted/ is)`
`verified-by: bravebot_browser::extension::each_extensions_id_is_the_one_its_key_gives_and_the_second_names_the_first`
`verified-by: bravebot_browser::extension::the_second_extension_holds_the_debugger_and_the_first_extensions_id_alone`
`verified-by: bravebot_browser::extension::the_second_extension_takes_a_request_from_the_first_extension_and_a_connection_from_its_own_windows_and_nothing_else`

<a id="BROWSER-38"></a>
### BROWSER-38: the tools of `input-trusted` act on the one element a role and an accessible name lead to, as the browser's accessibility tree names it, and only after the person allows it in a window of the second extension

Five of the methods of [BROWSER-37](#BROWSER-37) name an element by a role of the ten of
[BROWSER-33](#BROWSER-33) and an accessible name. This clause says how the second extension finds
the element, the window the person allows the action in, the panel the person answers a dialog in,
and what each of the five tools sends. The BraveBot extension's part is the paragraph that follows
the lead.

**The BraveBot extension's side.** The tools of `input-trusted` in `extension/tools.js` are checked
as every tool is, and a call is sent to this extension only once each of these holds, in this
order: the tool's switch is on, the group's switch is on, its lifetime has not ended, the URL is at
an allowed site whatever the scope ([BROWSER-18](#BROWSER-18), [BROWSER-22](#BROWSER-22)), the
arguments are in the lists and bounds of [BROWSER-37](#BROWSER-37), the URL is a page on the web
and is at no blocked site ([BROWSER-17](#BROWSER-17)), and no other action waits
([BROWSER-34](#BROWSER-34)). The request is `{v: 1, method, params}` with the arguments the call
gave, those it did not give that have a default filled in, and no other. `chrome.runtime.sendMessage`
in `extension/trusted.js` sends it to the id of this extension and waits 80 seconds for the reply,
which is 20 more than the person has and 10 fewer than the relay waits ([BROWSER-6](#BROWSER-6)). A
reply is taken as a result only if it is `{v: 1, result: {status, origin}}` with a status word of
the closed list in `extension/trusted.js` and an origin as the browser writes one, and then the call answers those two
words. It is taken as an error only if it is `{v: 1, error: {kind, message}}` with `refusal` or
`failed`, and a message of at most 300 characters in which no character is one the window would
mark as unseen; a `refusal` is then a refusal of the BraveBot extension and a `failed` a
failure, each with the message after a fixed sentence that names this extension. A reply of any
other shape, a message that is too long or holds an unseen character, and a result and an error
together are a failure with one fixed sentence that holds nothing of the reply. A browser that has
nothing to deliver to, which is what it says when this extension is not installed, is turned off or
does not accept the sender, is one sentence of refusal that names the three, and changes no switch,
permission or list of any group. A worker that stops before it answers is a failure saying the
action may or may not have been done, and a reply that does not come in 80 seconds is another.
The activity log keeps the group, the tool, the outcome and the origin of the page for each call
and nothing the call wrote ([BROWSER-27](#BROWSER-27)).

**Resolution.** `resolve.js` finds the element in a session on one tab. The role is asked for as the
roles the browser's accessibility tree gives it: a search field is a `searchbox` there and counts
as a `textbox`. The name is the one the browser computes, and it is compared with the planner's after
lower-casing both and collapsing white space, as the BraveBot extension compares. A name of white
space only and a role outside the ten are refused before the browser is asked. The page is read with
`Page.getFrameTree`, `Accessibility.getFullAXTree`, `DOM.getFlattenedDocument` with `pierce`,
`CSS.getComputedStyleForNode`, `DOM.getBoxModel`, `DOM.scrollIntoViewIfNeeded`,
`Page.getLayoutMetrics` and `DOM.getNodeForLocation`, which are on the list of reads of
[BROWSER-37](#BROWSER-37). Nothing is run in the page and no input is sent, so no token is needed.
The tab has to be at the address it was found at, its fragment counted, or the resolution fails. Frames
are not entered, and an element of a closed shadow root is found. The element that has the focus is
resolved by the same rules, as the one node of the tree other than the page itself that the tree
says is focused, and its role is the tree's word for it. The focus on the page alone, or on
several nodes, is a failure.

**Which element.** Exactly one displayed element must match. An element is displayed when the tree
keeps it, nothing above it carries `hidden` or `aria-hidden`, its box is wider and taller than one
pixel, it is displayed and visible, and neither it nor any element above it has an opacity of 0. None
fails, several fail with how many, and when every match is hidden the failure says so. An element
the browser leaves out of the tree, such as one with `display: none` or `visibility: hidden`, is not
found. `choose_file` is the one tool that finds an element the page hides, and only a file input
([BROWSER-45](#BROWSER-45)). A page with more than 100,000 nodes in either tree, or with more than 50
elements of the role and the name, is not searched.

**What is never acted on.** The one element is refused when it is a password field, which is an
`input` of type `password` or one whose `autocomplete` holds `current-password` or `new-password`; a
payment card field, which is any element whose `autocomplete` holds a word that starts `cc-`; or a
file chooser, which is an `input` of type `file`, by every tool but `choose_file`, which acts on a file
chooser and on nothing else ([BROWSER-45](#BROWSER-45)). The tree calls a file chooser a button, so the
attribute decides. It fails when the tree says the element is disabled or read-only, which it does for
a disabled fieldset and `aria-disabled` as well. The element is then scrolled into view, and the
middle of its box has to be in the window. The element at the middle, asked for in page coordinates
as the browser takes them, has to be the element or inside it and in the page's own frame. Another
element, a frame, or a node made since the document was read covers it and the resolution fails. A
layer that takes no pointer event does not cover it.

**What a resolution holds.** The node's number, the point to press, which is the middle of the box in
whole pixels in the window, whether text can be typed into the element, a description and a
fingerprint. The description is the tag, the role,
the name (200 characters), the start of the text taken from the tree's text (80), where a click on it
goes (200) or that this cannot be told, and the rectangle. Where a click goes is worked out from the
document as [BROWSER-33](#BROWSER-33) says, from the node a pointer at the middle of the element
reaches, which is the element or inside it, and every element above it, so that a link inside the
element, a link around it, a button's own `formaction` and the form's `action` each show. A key starts
at the element itself, and only Enter goes anywhere: in a field it goes where the form's first submit
button sends the form, found among the form's controls in the order the browser lists them, which is
the order of the page for controls that are not inside one another, and a `form` attribute puts a
control in the form of that name. A button, a field or a link in a shadow root belongs to no form
outside it. An image map or two places that differ cannot be told: the description then has
`goesUnknown`, and the window shows `unknown`. It is text and whole numbers, the page wrote
most of it, and it is for the window: no reply holds a word of it ([BROWSER-37](#BROWSER-37)). The
fingerprint is the node's number, tag, role, name and destination, and whether it could be told, with
the numbers of the nodes above it. A node that was replaced or moved, or whose name or destination changed, gives another fingerprint
when it is resolved again, and the action that finds them different fails as changed and sends
nothing.

**The window.** No input is sent before the person has allowed it in a window of this extension,
`confirm.html`. It is the window of [BROWSER-34](#BROWSER-34), kept as the same bytes in both
extensions: `shared-files.json` lists the files, and a test compares each copy with the original. It
opens in front, 600 by 640, with Deny focused and Allow once available a second after it is in front
and only for a click the person made. Its buttons are in the bar at the foot of the window, in view
whatever the words come to ([BROWSER-34](#BROWSER-34)), and Allow once waits until a box that
scrolls, such as the one that holds the path of a file, has been scrolled to its end. It shows the page's words as text, with every
character that cannot be seen marked by its code point and each word cut to its limit, it denies when the person
denies, closes it or does not answer within 60 seconds, and a worker that stops takes the question
with it. Its heading and its title name this extension by its manifest, so that a person who has both
extensions can tell whose window is open. `actions.js` is this extension's own and words twelve
actions: click, double-click, right-click, type, key, hover, drag, the picture of
[BROWSER-43](#BROWSER-43), the console read of [BROWSER-44](#BROWSER-44) and the choice of a file of
[BROWSER-45](#BROWSER-45), which has three: on an input that is displayed, on one the page hides
that a label, an `aria-label` or a `title` names, and on one the page hides that only its id or
name names. The row of an action
says whether it acts on an element. The picture and the console read act on none, so the window
does not show the block of the element for them and does not count the characters of the size or
of what is read that it shows. Typed text and a key are
shown in the frame of the window, whole. A drag has two elements. The one it takes is shown in the rows of
the element, and the one it drops onto is shown in the frame, as one text that holds its role, tag,
name, start of text and where it goes, each in quotation marks with a quotation mark or a backslash
in it escaped and each cut to the limits of the rows, so that a word of the page cannot end its own
quotation and begin another. A denial, a closed window, no answer in time and another question
waiting are sentences of the table of [BROWSER-37](#BROWSER-37), and a window that cannot be opened
is a failure of it. The shared window's own sentences are turned into those codes in `windows.js`, so
that no reply holds a word of a window. One listener of the worker gives each connection to the
window of the page it came from, and a page with no question waiting for it is sent away, so the
window and the panel cannot answer each other's questions.

**The actions.** A tool finds the element or elements its arguments name, brings the tab to the
front, and asks the person in the window about them. It then finds them again and sends nothing
unless each is the element that was shown. A click, a type, a hover and a drag name an element by
role and name, a key acts on the element that has the focus, and a drag names a second element the
same way. No argument is a coordinate, a selector or a node number, and one that is refused. A tool
answers `clicked`, `double clicked`, `right clicked`, `typed`, `pressed`, `hovered` or `dragged`
with the origin of the address, or a sentence of the table of [BROWSER-37](#BROWSER-37).

A click moves the pointer to the point of the element, asks the browser again what is at the point and
the page again for what is inside the element, and presses and releases the button asked for, `left` or `right`, with click count 1. A double click
does it a second time with click count 2, asking again before the second press. A click that fails
after a press, because the browser refuses the press or the release or does not answer it, releases
the button at the point it pressed at, which is the point the release goes to, and tries a release
that failed once more, so that no button is left held. The release sent after a failure is sent at
once and waited for as the time below says. A click that fails before the press releases
nothing, and a double click that is stopped between its clicks has nothing to release. A hover moves
the pointer to the point and presses nothing. A drag moves to the point of the first element, asks again, presses the left
button, moves in 8 equal steps to the point of the second element with the button held, asks again
about the second element, and releases there. A drag that fails after the press, because the browser
refuses an input or does not answer it, something covers the second element, the answer to a dialog
fails or the debugger is gone, releases the button at the point of the first element, where it
pressed, so that no button is left held and the release lands on the element the person was shown,
and not on a point of the path or on what now covers the second element. A drag that fails before
the press releases nothing, and the release at the second element is not sent again if it fails.
While a dialog is open the browser answers no input, so no release is sent then. Both elements have
to be in the window together, and a page that cannot show both fails before any input. Text is typed into a `textarea`, an `input` of
a type that holds text, or an element made editable, and the call fails before the window for
any other element. The tool focuses the element, fails as changed unless the focus is then on that
element, selects all of it with the browser's editing command, and inserts the text with
`Input.insertText`, so the text replaces what the field held. A key is one of the eleven keys of the
BraveBot extension, sent down and up alone with no modifier, and Enter also sends its character. A
key goes to whatever has the focus, which a page can move at any time, so the accessibility tree is
read just before the key goes down and again just before it comes up, and the key is sent only if
the one element that has the focus is the element that was shown, with the name it had. A focus that
has moved before the key fails as changed and sends nothing, and one that has moved between the down
and the up sends no up, which would go to another element than the down did. Typing reads it just
before the text is inserted too. The window for a key shows the element that has the focus when the
window is asked, with where Enter goes if the key is Enter and nowhere for another key. A
key down or up that fails, because the browser refuses it or does not answer it, is followed by the
same up once more, so that no key is left down, and the failure of that second up is dropped. A
down that failed may have been applied, so it is released too. Typing does this for the key it
sends to select the text, and the text itself is one `Input.insertText` sent after that key is up,
so a text that fails has no key to release. A call that fails before the key down, as when the
focus does not land on the element, releases nothing, and while a dialog is open no key up is sent.

The question to the browser is `DOM.getNodeForLocation` at the point with the page's scroll added,
and the answer has to be the element or a node inside it, in the page's frame, with the tab still at
its address. What is inside the element is read for each question: the document is read again,
after the answer, so a node that was inside the element when the resolution was made and that the
page has moved out of it since, as a layer over the window when the pointer moved, is not inside it. The
destination of the node the pointer now reaches is worked out again from that document, and a click
that would go anywhere but where the window said fails as changed. A cover, a frame, a node
that is not in the document, a point outside the window or a page that left its address fails and
nothing more is sent. A node made inside the element since the resolution, with nothing else
different, is a part of it. Every input is sent with the token of
the window, and the first is a move, a focus or a key and never a press. An input that may open a
dialog is sent in the way of the next paragraph. An input the browser has not answered 5 seconds
after the wait for a dialog ends the call with a failure. The debugger is detached when the call
ends, however it ends.

**The dialog.** A page can open a dialog (an alert, a question, a prompt or a question about
leaving) when input reaches it, and while it is open the browser answers none of the input. There is
no tool that answers a dialog and no argument that says how to, since the planner cannot read what
the page said. The session enables Page before it sends input and learns of a dialog from the event
of its own tab. Input that may open one, such as the release of a click, is sent without waiting for
the browser's answer past a dialog: the call waits until the browser answers, a dialog opens or 2
seconds have passed. A dialog that has opened is shown in the panel, `dialog.html`, which follows the
rules of the window: Dismiss has the focus, Accept is available a second after the panel is in front
and for a click the person made, the buttons are in a bar at the foot of the panel that stays in view
under a message of 2,000 characters, and the page's words are text, marked and cut. It shows the kind of
dialog in words of its own, the site and the address, what the dialog says (2,000 characters) and,
for a prompt, the page's suggested answer (200) and a field, empty, for the person's own answer.
Accept and Dismiss are both answers, and Escape is Dismiss. The panel tells the worker which one,
and for an accepted prompt the text, in the message that gives the worker its token, and the worker
takes them from the panel it admitted and from no other page. `handleDialog()` sends
`Page.handleJavaScriptDialog` and takes no parameter from its caller: it sends the accept flag, and
for a prompt the text, of an answer whose token was made for the dialog this session has open, once.

**The time.** One deadline of 60 seconds is counted from the first window of a call, the one that
asked about the action, and the panel counts down to what is left of it. A call has no time for a
second 60 seconds, so that the relay's wait for the extension ([BROWSER-6](#BROWSER-6)) is not
passed while a panel waits. A dialog the person does not answer, because the panel is closed, no
answer comes in time, the panel cannot be opened or no time is left, stays open in the page,
nothing is dismissed for the person, and the call fails with a sentence that says the page opened a
dialog that was not answered.

The steps before the first window have 5 seconds together, counted from the start of the call and
stopped when the person is asked: looking for the tab, attaching, reading the page, finding the
element and bringing the tab forward, and for `read_console` the look for the tab. When they pass
the call ends with a failure that says the browser did not answer in time before the person was
asked and that nothing was done. Nothing has been sent to the page and the person is not asked.

The time after the person's answer is bounded for an input the browser stops answering. An input
that is not answered ends the call 7 seconds after it was sent, 2 for a dialog to open and 5 for the
answer. The release that follows a failed press, key down or release is sent at once, and the
call waits 2 seconds for a dialog to open and 1 second for the answer to it, and drops its failure,
so a call whose browser stops answering input ends 10 seconds after the person's answer, 7 for the
input that failed and 3 for the release.

A call of any of the eight tools also has 11 seconds from the person's answer to end, whatever it is
waiting for: looking for the tab and attaching, which `read_console` does after the answer, finding
the element again, a read, an input or its release. The wait of the picture ([BROWSER-43](#BROWSER-43))
is 10 seconds and the wait of the console read ([BROWSER-44](#BROWSER-44)) is 10, so both end inside
the 11. The answer is the Allow of the first window or,
for a dialog the page opened, the answer given in the panel, so the 11 seconds do not run while the
person reads the panel and count again from their answer there. When they pass, the call ends with
the failure that says the browser did not finish in time after the person allowed it, so the action
may or may not have been done. The session ends at that moment, so the work that is left sends
nothing. A call that ends by any way with a button or key pressed and not released, this one
included, has the release sent once by its end, before it detaches: the release of the button at the
point it was pressed at, or the key up. The tool records it before it sends the press or the key
down, and forgets it when the release is answered or when the tool sends the release itself, so it
is never sent twice. It is not sent while the page has a dialog open, since the browser answers no
input and would apply it when the person answers the dialog. A key up is sent at the deadline
whichever element has the focus then, since nothing can be asked; a check before the key up that
fails with an answer sends none, as above. The release waits at most 1 second for the browser, and
the release and the detach together wait at most 2 seconds. An attach the browser answers after the
call gave up on it is detached. A call is over at most 13 seconds after the person's last answer.
With the 5 seconds before the first window, the person's 60 and the 1 second that a browser which
does not answer the closing of the window can hold the end of the question ([BROWSER-34](#BROWSER-34)),
that is at most 79 seconds from the start of the call, which leaves 1 second for the reply before the
BraveBot extension stops waiting at 80 ([BROWSER-12](#BROWSER-12)).

**Starting.** As the worker starts it detaches the debugger from every tab it lists as attached, and
a call that arrives with the start attaches only after that is done. A session lasts for one call,
so one that is still attached belongs to a worker that stopped, and the browser keeps it, so that
every later call on the tab fails to attach until the debugging bar is cancelled. A tab that
another debugger holds is not this extension's, the detach fails and nothing changes.

**Why.** The browser's tree names an element as the person's own browser and its assistive software
do, so the planner's words and the page's differ less than with a second computation of names, and the
tree needs no script in the page, which the list of methods does not allow. The ways it differs from the
BraveBot extension's matcher are listed in the design document. Everything that depends on what a
person can see is decided before the window and again at the action, so the person is asked about an
element that can be pressed, and a page that swaps or moves it between the question and the action
changes the fingerprint and gets nothing. A press lands on the pixel at the element's middle and not
on the node, so the browser and the document are asked again after the move and before each press and
before the release of a drag, which narrows the time in which a page can put another element there
and does not remove it. A click goes to what is under the pointer and up through everything above it,
so the window shows where the nearest link or submitting button decides, and not only where the
element itself goes. The browser takes a location in page coordinates and an
input event in window coordinates, so the cover test adds the scroll, and a test holds that it does.
The window is the BraveBot extension's, kept as the same bytes, so that a fix to one cannot be left
out of the other, and a copy is checked and not shared because an extension cannot load a file from
outside its own directory. A dialog is answered during the call that opened it because the browser
lets only a session that had Page enabled when the dialog opened answer it, so a call made later
finds a dialog it cannot answer and cannot see. The person answers it in a panel, as they allow an
action in a window, since the dialog's words are the page's and what to answer is a choice only
they can make. No answer leaves the dialog open and does not dismiss it, because a dismissal is an
answer to the page that nobody gave.

`verified-by: by-construction (extension/tests/trusted.test.mjs asserts that each tool sends one request that the second extension's own check accepts, to its id and in its version, with the arguments the call gave and the defaults and no other, that nothing is sent while the group or the tool is off, the lifetime has ended, the site is not allowed or is blocked or the page is not on the web, that an argument outside the lists and bounds is refused before anything is sent, that a result holds the status word and the origin and nothing else the reply held, that a refusal and a failure arrive as the two kinds with their message after the fixed sentence and are logged as refused and failed, that a reply of any other shape or with a message that is too long or holds an unseen character is the one fixed failure, that an extension that is not there is one sentence of refusal and changes nothing in any other group, that a worker that stops is a failure that says the action may have been done, that the wait ends at 80 seconds and not before and leaves no timer behind, that a call that waits holds the place of the BraveBot extension's own actions and leaves it however it ended, and that the log keeps the group, tool, outcome and origin and none of the name, text or path; make check-extension runs it, as for BROWSER-37)`
`verified-by: bravebot_browser::extension::the_waits_of_a_trusted_input_call_grow_from_the_persons_window_to_the_declared_bound`
`verified-by: bravebot_browser::extension::the_group_table_lists_the_groups_and_each_tool_in_one`
`verified-by: by-construction (extension-trusted/tests/resolve.test.mjs asserts, against a page recorded in Brave 1.98.62 (recorded/page.html, recorded/world.json), that the ten roles are asked for as the roles of the tree with a search field and a select among them, that a name is compared without regard to case and with white space collapsed and a blank name or another role is refused, that none and two fail with the count and a page past the caps is not searched, that an element transparent itself or through the element above it or one pixel across is not counted and fails as hidden when it is the only match, that an element the tree leaves out is not found, that a hidden mark, a style, a missing box and a small box each hide an element the tree keeps, that a password, a card and a file field are refused by their attributes and a disabled or read-only element fails, that an element covered by another or by a frame fails and one under a layer that takes no pointer event or with its own text hit does not, that the element is scrolled to before it is measured, that its middle is asked for in page coordinates and a middle outside the window fails, that the point, the rectangle and the description are as recorded, with the name, the text and the destination whole up to 2,000 characters for the window to cut, that a fingerprint is the same for the same element and differs for a replaced, moved, renamed, retagged or redirected one, that a tab that moved on, its fragment counted, fails, that only reads are sent with an approval that allows nothing, that the element which has the focus is resolved like a named one, with the tree's word for its role, and fails when only the page has it, when several have it or by the rules above, that whether text can be typed into an element follows its tag, its type and whether it is editable, that a later question about a resolution's point is asked in page coordinates without scrolling and fails as covered, offscreen or left when another node, a frame, a node made since, nothing, a smaller window or another address is found, that it asks the document again so that a node the page moved out of the element or removed is covered and one left inside is not, and fails as changed when a link the element is in or the pointer now reaches would send the click elsewhere than the window said, that where a click goes is the address of the nearest link or submitting button from the node the pointer reaches and every element above it, a link inside the element, a link around it, a button's formaction or type, an input of type submit, image or another, a shadow root ending a form, the first submit button of a form for Enter in a field, no submit button, two places that differ and an image map, and that a key starts at the element and only Enter goes anywhere, and that the focus is read again as one element with the name and the page it had, failing as changed when it moved to another node of the same name, none, another name, another address, a tree over the cap or a resolution this file did not make, and that what a failure says is a sentence of the closed table with a count at most; that a name, a text and a destination are given whole up to 2,000 characters and an element with a longer one is refused; and, in cdp.test.mjs, that a read, the list of tabs, an attach and a detach the browser never answers fail after 15 seconds and the session still ends, and that input is not held to that wait; make check-extension runs it, as for BROWSER-37)`
`verified-by: by-construction (extension-trusted/tests/tools.test.mjs asserts, against the recorded page and a fake debugger that records every input in order, that a click is a move, a press and a release of the button asked for at the middle of the element with the click count, that a double click does it twice with the counts 1 and 2, that an element below the fold is pressed at the point in the window and asked about at the point in the page, that the person is asked once and after the page is brought to the front and before any input, that a denial, no answer, a value that is not a token, a token made for another action and a token used twice each send nothing, that a node replaced, moved, renamed or redirected, a page that left its address and a cover each send nothing, that a cover after the move or after the first click of a double click stops the next press, that a hover is a move alone, that typing focuses the field, selects all by the editing command and inserts the text with no letter key, stops when the focus is not on the field and does not type into an element that is not a text field, that a password, a payment card, a file and a disabled field are refused by every tool and a key in one is refused, that each of the eleven keys is sent down and up alone with its code and Enter with its character, that the window for a key shows the element with the focus and the key and nothing is asked when none has it or the focus moved, that a drag presses at the first element, moves in eight steps to the second with the button held and releases there, shows both, and sends nothing when the second is missing, hidden, covered or out of the window with the first or when either changed, that a node inside the element when the document was read and moved out of it by the move stops the press and one left inside does not, that the window shows where the click goes for a link inside the element, a link around it, a button's formaction, the form's action and an image map, that a link the pointer reaches after the window stops the press, that the window for Enter in a field shows the form's first submit button's address and for Tab nowhere, and that a page which moves the focus after the second look, between the key down and up, or after the look that follows the focus of typing stops the key with no key sent, sends no up after a down or inserts no text, and that a drag that fails after the press at the press, a move, the last move, a cover before the release, a dialog it answered and then a cover, or a browser that stops answering releases the button where it pressed, that one that fails before the press releases nothing, that the release at the second element is not sent again, and that a dialog nobody answers sends no release, that a key down or up of typing and of a key press that fails, with the browser then answering or not, is followed by the same up once, that a text that fails, a focus that cannot be moved or lands elsewhere sends no up, and that a dialog opened by a key down or up goes to the panel and lets the call go on or is left open and sends no up, that a click, a double click and a right click that fail at the press or the release of any click release the button where it pressed and try a release that failed once more, that one that fails before the press or after a completed click releases nothing, and that a hover releases nothing on any path, that each tool answers its status word, that a reply holds no word of the page, that a call detaches the debugger however it ends and an input the browser does not answer ends it with one sentence, that on a fake clock with the person answering 59 seconds into their 60 and the browser then answering no input, a click, a double click, a key press, typing, a drag and a hover each end the call 10 seconds after the answer, or 7 where nothing is released, with the release sent once, so that the 60 seconds, the 10 and 5 more fit in the 80 seconds the BraveBot extension waits, that on the same clock a browser that stops answering at any one step after the person's answer (the list of tabs, the attach, each read, each input, the detach) ends the call of each of the seven tools within 11 seconds of the answer and 2 more for the detach, and a detach alone holds the reply for 2 seconds, that a browser that answers every step after 7 seconds ends each call at the 11 seconds with the one sentence for it (the picture after 10, by its own wait), that a call ended that way sends nothing when the browser answers later and an attach answered late is detached, that a call whose time runs out with a button or key pressed (the press of a click, of a double click, of a right click, the key of typing, a key press, a drag) releases it before it detaches, once, when the browser took the press and answers nothing after it, and that with the reads quick the call's own release is the only one, that the steps before the first window end the call within 5 seconds, whichever tool it is and whichever step the browser stops answering at or answers slowly, with the sentence that nothing was done and the person not asked, that the time before the window stops when the person is asked, that no timer is left when a call ends, that a page whose open dialog holds the browser's answers cannot hold the call or its place and nothing is asked, pressed or detached again when the dialog is answered later, that the time does not run while the person answers a dialog in the panel and counts again from their answer, that an argument the tool does not take, a coordinate and a count out of range are refused before the browser is asked, and that a dialog opened by a press or a release goes to the panel, is answered once by the person and lets the call go on, or is left open and fails the call, and that every code a script throws is a sentence of the table, and extension-trusted/tests/cdp.test.mjs asserts that a detach the browser does not answer holds the end of a call for 2 seconds and no longer, that an attach answered after the call gave up on it is detached, that the time of a call counts 5 seconds from begin() with the failure slow and 11 from start() with late instead of it, and nothing while stopped, that an input recorded as owed is sent once before the detach whether the work throws, returns or runs out of time, is not sent when it was settled or when the page has a dialog open, waits 1 second for the browser and the detach after it with the two together at most 2 seconds, and is recorded only if it is on the list, sends input and has the approval's token, and that work that goes on after the call ended can record or send nothing, and that leftover sessions are detached for the tabs listed as attached and a call waits for that for the time before the window only; make check-extension runs both, as for BROWSER-37)`
`verified-by: by-construction (extension-trusted/tests/confirmation.test.mjs runs the BraveBot extension's window tests against this extension's copies of the window and asserts that the window opens as a small popup of the extension's own page, that Deny has the focus and Allow once is not available for a second or out of front, that a click a script made or with no user activation does not allow, that Allow once gives a token for that resolution once, that Deny, Escape, a closed window, a closed port and the time limit each deny with their reason and give no token, that only the window opened for the question can answer, that markup, control, direction and invisible characters in a name are shown as text with code points, that words are cut to their limits, that the heading and the title name this extension, that a combining mark after a letter is text up to three times and every other one is marked, that every row of the page's words is cut off at its box, that the destination comes before the role, tag, name and text, which are each a box of at most five lines that scrolls and shows its scroll bar, and the typed text is below them, that the window is wide enough for the browser's debugging notice, that a window the browser never says it opened and a browser that does not answer the closing of the window hold the end of the question for no longer than the time limit and CLOSE_WAIT_MS, that those bounds, the five seconds before the window, the eleven after the answer and the two to detach fit in the 80 seconds the BraveBot extension waits, that Allow once is not available until the box that holds the path of a file has been scrolled to its end, that a destination that cannot be told is the window's own mark, that each of the nine actions is worded and its row says whether it acts on an element, that the picture says the debugger reads everything on the page and the console read says it reads the page's console, that each of them has no block of the element and no count for what it shows while every other action of the table has them, that the block is one section of the page, and that a drag shows the element it takes in the rows and the element it drops onto in the frame with a hostile name quoted and marked, that the scripts never turn a string into markup or code, that the page loads nothing, that the wait line, the status and the buttons are one bar fixed at the foot of the window, and that the manifest has no web accessible resource or content script and lets the BraveBot extension alone connect; extension-trusted/tests/actions.test.mjs asserts the words, whether each action acts on an element, what a description holds, that the window is given a destination that cannot be told only from an element that says so and a drag says it in words a page's quoted word cannot be, that an action that acts on no element is described with none whatever element it is given and every other with the one it is given, and that the text of the element a drag drops onto quotes, escapes and cuts the page's words; extension-trusted/tests/windows.test.mjs asserts that each way an action is not allowed is a refusal or a failure of the table with no word of the window, that the panel and the window cannot answer each other's questions or be sent away by each other's confirmation, that a page with no question waiting is sent away, that the answer to a dialog is taken from the page the confirmation admitted and not from a second page or one that sent it first, that the panel counts down to the time left of the call counted from the first window, that a dialog nobody answers fails the call with one sentence however the panel ends and that none is asked about with no time left, and that an answer the panel's own page sent before it was shown leaves the dialog open and unanswered, and that the panel and the window are as wide as the browser's debugging notice needs; extension-trusted/tests/dialog.test.mjs asserts that the panel names the kind of dialog and shows the site, the address and the dialog's words as text with the unseen characters marked and cut to their limits, a field for a prompt only, that Dismiss has the focus and Accept follows the rules of Allow once, that Accept and Dismiss and Escape each answer with the text typed for an accepted prompt only, that a dialog the panel cannot show whole is not answered from it, that the panel counts down and keeps the worker running and ends itself when the worker stops, that its wait line, status and buttons are one bar fixed at the foot of the panel, and that its page loads nothing and runs nothing of its own; extension-trusted/tests/cdp.test.mjs asserts the dialog behaviour of the session as under BROWSER-37; extension-trusted/tests/call.test.mjs puts a call together from these parts, as a tool will, and asserts that a click that opens a dialog is answered once, by the person, in the panel, with Accept, with Dismiss or Escape, and with the text typed for a prompt, that a click that opens none opens no panel and answers no dialog, that a dialog nobody answers is left open, is answered by nothing and fails the call with one sentence with the session detached, that the panel has the time left of the 60 seconds counted from the first window, and that an action the person denies sends no input; make check-extension runs them, as for BROWSER-37)`

<a id="BROWSER-39"></a>
### BROWSER-39: take_held returns what the person held out in the browser and forgets it, and the extension reads nothing from a page to make an item

`held` is the group whose one tool, `take_held`, returns the items the person held out with the
extension and forgets them. It lists `contextMenus` and no host, so it is an optional permission
([BROWSER-18](#BROWSER-18)) and no page is within its reach. It is on until Brave restarts unless the
person chooses another time ([BROWSER-24](#BROWSER-24)), and is in no preset
([BROWSER-20](#BROWSER-20)). The server started for `held` offers `take_held` and no other tool.

**How an item is made.** While the group is on, the extension adds three entries to the browser's
context menu: "Hold this selection for BraveBot" on a selection, "Hold this link for BraveBot" on a
link, and "Hold this tab for BraveBot" on the page. Using one makes one item from what the browser
gives the extension: the selected text, the address of the link or the address of the page, with the
address of the page the entry was used on. An item exists only because the person used an entry. The
extension calls no tab, script, navigation or window API to make one, builds no markup, and
neither sends nor accepts a message, so a web page cannot make an item or ask for one, and the page
is read by nothing but the browser's own hand-over of the selection. An entry used while the group is
off, or while the switch of `take_held` is off, holds nothing, whether or not the browser has removed
it. The extension asks again once an item is written, and forgets every item if the group or the
switch has been turned off by then, so that an entry used as the group is turned off leaves nothing
held.

**What is kept.** At most 10 items, in session storage, which the browser holds in memory, never
writes to disk and empties when it restarts and when the extension is reloaded or updated. A
selection is cut at 20,000 characters and an address at 2,000, never through a character, and an item
that was cut says so. The origin of the page is read from its whole address and kept beside the cut
one, and the blocked sites ([BROWSER-17](#BROWSER-17)) are matched against that origin. An item older than 30 minutes is forgotten, and holding an eleventh forgets
the oldest; a fetch says how many were forgotten for either. Every item is forgotten, and the
entries removed first, when the group is turned off by its switch, by a preset, by Turn everything
off, by the end of its lifetime or by the permission being taken back, and when the group is turned
on ([BROWSER-19](#BROWSER-19), [BROWSER-24](#BROWSER-24)). The popup says how many items are held.

**What comes back.** `take_held` takes no argument, and a call that names one is refused before
anything is taken. It returns the items in the order held, each with its number, its kind
(`selection`, `link` or `tab`), the origin of the page it was held on and the text or the address,
and the number forgotten, and it forgets them, so a second call finds none. An item held at a
blocked site is left out and forgotten ([BROWSER-17](#BROWSER-17)). The reply is a tool result, so
it is untrusted and private and the session is given a reference and no word of it
([MCP-1](mcp.md#MCP-1), [LABEL-3](labels.md#LABEL-3)). A model reads it only through a processor that
answers a question about it, or at `vet_content`, the question to ask to read the exact text, and the
description of the tool says so without saying who answers it
([VET-5](tools/vet-content.md#VET-5)). Holding something out is not vouching for it
([LABEL-8](labels.md#LABEL-8)).

**Why.** What a person selects on a page is that page's text. A range includes text the page hides,
at zero opacity, off screen or in the colour of its background, which the gesture does not show.
The gesture says which part of the page matters and is not read as a statement that the bytes can
be trusted, so an item takes the label of any result of a server, and this process and the model's
context read none of it. The browser hands the extension the selection, so the group needs no host
and no script in a page, and the page has no way to add to what is held. The call is asked about as
a call to `read_page` is: it names nothing, so the question holds the alias and the tool, and what it
returns is what the person chose and no more. Asking about every call would add a question with
nothing in it to decide, and a person who wants it can declare the server that way.

**Known costs.** The text of a selection can include text the person did not see, and a long
selection is long to read, so the reading at `vet_content`, by the person or by whatever answers it in that session ([VET-5](tools/vet-content.md#VET-5)), is the only check on it. A tab
is held as its address, since its title needs the `tabs` permission and the group lists none of the
permissions that read a tab. Whether the browser cuts a long selection before it gives it to the
extension, what the entries do on a page that scripts its own menu, in a private window and in a frame,
and the look of the entries were not observed, because no click on a context menu can be made in a
headless Brave. In Brave 1.98.62 the permission can be optional, a request for it is granted with no
prompt, `chrome.contextMenus` exists from then on and its calls say they are not available after the
permission is taken back, and 10 items of 20,000 characters take 323 KB of the 10 MiB of session storage.

`verified-by: bravebot_browser::tools::take_held_is_offered_by_the_server_of_held_alone_and_takes_no_argument`
`verified-by: bravebot_browser::tools::the_take_held_description_says_the_result_is_page_text_the_model_cannot_read_and_how_it_becomes_readable`
`verified-by: bravebot_browser::tools::no_description_says_a_person_answers_vet_content`
`verified-by: bravebot_browser::relay::what_a_person_held_out_is_called_through_its_group_alone_and_arrives_as_one_text_part`
`verified-by: bravebot_browser::extension::what_a_person_holds_out_is_given_by_the_browser_and_read_from_no_page`
`verified-by: bravebot_browser::extension::the_design_documents_group_tables_are_the_ones_the_extension_and_the_relay_hold`
`verified-by: by-construction (extension/tests/held.test.mjs asserts the three entries and the context each is in, the item each makes from what the browser gives and that an entry that is not ours or has nothing to hold makes none, that a selection and an address are cut at their limits and not through a character, that at most 10 items are kept and the oldest is forgotten, that an item older than 30 minutes or from a clock set back is not returned and is counted, that taking empties what is kept, that a record that cannot be read holds nothing, that items held together are all kept, that the entries are made once after the old ones are removed, that a use of an entry holds an item only while the group is on, that an item written after the group was turned off is forgotten again and one written while it stays on is kept, and that an item remembers the origin of the whole address of its page when the address is cut; it runs where the test under BROWSER-13 runs)`
`verified-by: by-construction (extension/tests/tools.test.mjs asserts what take_held returns, in order, with the origin of the page and the text or address of each item, that a second call finds none, that it says what was cut and how many were forgotten, that an argument is refused before anything is taken, that it leaves out and forgets what was held at a blocked site, including one whose address was cut, and that it is refused, taking nothing, while its tool or group is off, its lifetime has ended or contextMenus is not held; extension/tests/switches.test.mjs, extension/tests/background.test.mjs and extension/tests/popup.test.mjs assert what the group's moments of ending, the worker and the popup do with the items and the entries, and background.test.mjs that a use of an entry holds nothing while the switch of take_held is off and an item once it is on; extension/tests/options.test.mjs asserts that the page says what is kept, for how long, that no page is read, and how the model can read the result; they run where the test under BROWSER-13 runs)`
`verified-by: bravebot_agent::browser_group_servers::what_a_person_held_out_in_the_browser_is_a_reference_and_the_planner_never_holds_it`
`verified-by: bravebot_agent::browser_group_servers::an_item_held_out_is_given_to_a_processor_as_text_and_never_to_the_planner`
`verified-by: bravebot_agent::browser_group_servers::an_item_held_out_reaches_the_planner_only_when_a_person_has_read_it_and_every_call_after_is_put_to_the_person`
`verified-by: bravebot_agent::browser_group_servers::a_call_to_take_held_from_a_context_that_has_met_untrusted_content_is_refused_before_anybody_is_asked`

<a id="BROWSER-40"></a>
### BROWSER-40: save_page_archive saves the page of one tab at one URL as a file in the Downloads folder, and returns the file's name and nothing of the page

`save_page_archive` is in `downloads`. It takes the `url` of a tab, as `read_page` does, and no other
argument; any other argument in a call is dropped. Its routing field is
`brave-downloads:save_page_archive` and its one argument is the URL the question before the call
shows, so a person can approve the field and the argument alone. The URL must be an HTTP or HTTPS
URL and at an allowed site whatever scope any group has, since the tool is in `ALLOWED_ONLY_TOOLS`
([BROWSER-22](#BROWSER-22)); a URL at a listed site is refused ([BROWSER-17](#BROWSER-17)); and
exactly one tab must be at the URL ([BROWSER-13](#BROWSER-13)). Each comes before the browser is
asked for the archive, the first before a tab is looked for and before the browser is asked for
permissions. A URL that is not HTTP or HTTPS is not at an allowed site and is refused as one that
is not. The tab need not be the visible tab of its window, and it is not brought forward.

**The archive.** The extension asks the browser for the archive of the tab with
`pageCapture.saveAsMHTML`, and then reads the tab again: if it is gone or is no longer at the URL,
the call fails and saves nothing. An archive that is missing or empty fails, and an archive of more
than 16 MiB fails saying so, before it is turned into anything else. A failure of the browser's
capture is told in the browser's words. The archive holds the frames of other origins inside the
tab, and the blocked list names the tab's own URL only, so a frame at a listed site is in the file
(Known costs).

**The file.** The extension hands the archive to `downloads.download` as a data URL of type
`multipart/related`, with a file name it makes, `saveAs` false and `conflictAction` `uniquify`, and
nothing else. The name is `bravebot-` and the host of the URL cut to 100 characters, with every
character but a letter, a digit, a dot and a hyphen replaced by a hyphen, and `.mhtml`. No folder,
name or option is an argument, and the name holds nothing else a call wrote. The browser puts the file
in the person's Downloads folder, asks nothing and does not replace a file that is there. The call
asks the browser for the state of the download up to 50 times, 100 ms apart, until it is no longer
in progress. A download the browser interrupted fails the call, naming the browser's reason. The
reply is the URL, the path the browser holds for the file, the state of the download and the size of
the archive in bytes, and nothing of the page: no text, title, link or byte of the archive. The
activity log keeps the origin of the URL ([BROWSER-27](#BROWSER-27)) and none of the file.

**Who may read the file.** The file is in the Downloads folder, outside the workspace, and the trust
map treats it as unvouched ([BROWSER-32](#BROWSER-32), [TRUST-1](trust-map.md#TRUST-1)). No tool of
the extension reads it, and the reply is untrusted and private ([MCP-1](mcp.md#MCP-1)), so the model
is not told where the file is. It reaches the model as any file outside the workspace does, by the
person naming it ([naming-files.md](naming-files.md)) or dropping it
([dropping.md](dropping.md)). It is then an untrusted reference of the file's type and under the
vetting rules of that file. The relay and the client's table of media types carry no file.

**Permissions.** `downloads` lists `pageCapture` for the archive beside `downloads` to save it and
`tabs` to find the tab, and reads no host. The tool finds the tab by its address, and Brave leaves
the address out of every tab it gives an extension that holds neither `tabs` nor a host that covers
the tab, so without `tabs` in the group the call fails with no tab at the URL for every URL. Brave
words `pageCapture` as "Read and change all your data on all websites", says nothing more for
`tabs` beside it, and words `downloads` as "Manage your downloads", so turning the group on asks for
those two lines under every set of allowed sites. `page-read` lists neither `pageCapture` nor
`downloads`, so the Read and Browse presets ask for neither, and they list `tabs` already
([BROWSER-20](#BROWSER-20)). The options page says in the note under the group that the browser
would let the extension archive any open tab, so the allowed sites and the blocked list are this
extension's limit ([BROWSER-22](#BROWSER-22)). The other tools of the group stay off until their
switches are on ([BROWSER-14](#BROWSER-14)).

**Why.** A page is saved only from the sites the person chose for reading pages, and the browser
does not limit `pageCapture` to any site, so the extension names the tool in `ALLOWED_ONLY_TOOLS`
and `handle` checks the allowed sites for every call of it. The tool does not follow the scope of
`page-read`: a group turned on for downloads must not read pages that `page-read` is off for. Saving
the file through `downloads` keeps the page out of the result, and so out of the planner, the driver
and a tool-result slot, and gives the person a file they can name, open or delete. The name is made
by the extension so a session cannot choose a place or a name for a file. The 16 MiB bound is below
the 20 MB data URL Brave 1.98.62 saved in a test and bounds what the service worker holds at once.
The options for placing the tool and the run in Brave are in
[the design](../design/browser-extension.md#saving-a-page-as-a-file).

`verified-by: bravebot_browser::extension::the_extension_archives_a_tab_in_one_call_and_saves_it_through_downloads_with_no_dialog`
`verified-by: bravebot_browser::extension::the_extension_has_each_permission_its_tools_require`
`verified-by: bravebot_browser::tools::the_archive_tool_takes_a_url_alone_saves_a_file_and_says_where_and_what_is_left_out`
`verified-by: bravebot_browser::relay::an_archive_is_called_through_its_group_alone_and_its_answer_is_text`
`verified-by: by-construction (extension/tests/tools.test.mjs asserts that save_page_archive passes the browser the archive of the tab at the URL as a data URL with a name made from the host, no dialog and no replacing and no other option, whatever name, path or option the call carries, and returns the path, the state and the size and nothing of the page, with the activity log holding the origin alone; that the name holds only letters, digits, dots and hyphens and at most 100 characters of the host; that a blocked site, a URL that is not HTTP or HTTPS, a site that is not allowed whatever scope page-read or downloads has, an allowed list that cannot be read, two tabs at the URL and no tab at it each reach no archive and no download, and that download_file is not held to the allowed sites; that an archive of 16 MiB is saved and one a byte over, an empty one and a missing one are not; that a tab that left the URL or closed while the archive was made saves nothing; that the browser's words are told when it cannot make or save the archive and that an interrupted download fails the call; that the call waits up to 50 polls of 100 ms for the download to finish; and that it finds its tab with exactly the permissions the downloads group lists, in a fake browser that leaves the address out of a tab for an extension that holds neither tabs nor a host covering the tab, as Brave does; extension/tests/groups.test.mjs, warnings.test.mjs, switches.test.mjs and options.test.mjs assert the permissions and the warnings of the group, that no preset asks for pageCapture or downloads, and that the options page says who limits the sites; it runs where the test under BROWSER-13 runs)`

<a id="BROWSER-41"></a>
### BROWSER-41: list_page_elements numbers the controls of a page as text, and a number leads to the element it was given to and to no other

`list_page_elements` is in `page-read`. It takes `url`, the exact URL of one open tab, and finds the
tab as every tool that names one does: a URL at a listed site, or at no allowed site where the group
is limited to them, fails before a tab is looked for, and a URL at no tab, or at more than one, fails
saying how many ([BROWSER-13](#BROWSER-13), [BROWSER-17](#BROWSER-17)). It calls `scripting` and no
permission the group does not list.

**The list.** The extension runs `inPage` ([BROWSER-33](#BROWSER-33)) with the step `list` in the top
frame of the tab, in its isolated world. The elements are the ones `click_element` could find: those of
the top document and of the open and closed shadow roots in it, in tree order, that have one of the ten
roles. An element that is not displayed, one that is disabled or read-only, and a password field, a
payment card field and a file input are not numbered, and the first line of the result counts them. An
element whose link or form goes to a listed site is left out without a count, as `read_page_links`
leaves out a link, and so is one whose tag, role, name, place and destination take more than 2,000
characters together, which is counted. The destination is judged and its origin taken from the
whole address, and the page cuts none of it. At most 40 elements are numbered, from 1, with no number
skipped. The list does not read what a field holds.

The result is one text. Its first line is the extension's own words: how many elements are numbered,
how long the numbers hold, and how many were left out and why. The second names the columns. Then a line
for each element holds its number, its role, its accessible name cut at 120 characters, with three
dots where it was cut, and the origin, or the scheme where there is no origin, that it goes to. A name
has its white space collapsed, so it cannot start a line, and a character that cannot be seen is
written as its code point, so it cannot reorder the text round it.

**What is kept.** The extension keeps one list in `chrome.storage.session`, which ends with the
browser: the URL, the time, and for each numbered element its place in the document and its
fingerprint. It keeps no text and no value of a field. A second list replaces the first, whatever page
it was made for, so a number refers to one URL and one list. The list can be used for five minutes
from the time it was made. It is forgotten when a group that lists `list_page_elements`,
`click_element`, `type_text` or `select_option` is turned off, which Turn everything off does, and
when such a group is turned on. A stored list that is not an address, a time and at most 40 elements with a
place and a fingerprint is no list.

**A number.** `element_number` is checked with the other arguments of the call, before a tab or a page
is reached ([BROWSER-33](#BROWSER-33)). The extension then refuses it, with a sentence of its own that
holds no word of a page, where no list was made, where the list was made for another URL, where it is
more than five minutes old and where the number is past the last element. The page is given the place
and the fingerprint that the list kept for the number and nothing the planner wrote. It accepts the
element at that place only if it is displayed and has that fingerprint, so a swapped, renamed,
retargeted, hidden, moved or removed element is refused, nothing is done and the person is not asked.
The element is then shown, outlined and acted on after the person allows it, as for a role and a name
([BROWSER-34](#BROWSER-34)), and the window also shows its number and which of the displayed elements
with its role and name it is, which the page works out in the same call by counting them in document
order. The number reaches the extension as `element_number` whoever supplied it: the person through
the planner, or a processor through [ROUTE-10](routing.md#ROUTE-10). The extension does not know
which, and the question the person is asked first says.

**What comes back.** The text of the list is the tool's result, which is untrusted and private
([MCP-1](mcp.md#MCP-1)). The person reads it in the transcript
([VIEW-3](terminal-transcript.md#VIEW-3)) and the planner is given a reference it cannot read. The
number the planner writes comes from the person's words, or from the list when the person let it
through with `vet_content`; [ROUTE-9](routing.md#ROUTE-9) says why that meets the routing rule. Where
a processor chose the number, the planner writes no number and names the choice instead
([ROUTE-10](routing.md#ROUTE-10)). The
reply to an action by number is the one an action by name has.

**Why.** Two controls with one role and name cannot be told apart by a name, and a person may not
know the accessible name of the one they mean. A number read off a list, next to the role and name of
the element it stands for, is a word they can check, which a node id read from a page is not, and the
window shows the element it leads to before anything is done. The page writes the list, so it can
word a line to mislead. What protects the person is that the window shows the element itself and
where it goes, and that the page cannot change what is shown. The fingerprint is compared at the
number and again at the action, so a page that changes after the list gets nothing. The list is kept
for minutes and for one page so that page text does not stay in memory and a number given for one
page cannot lead anywhere on another.

`verified-by: by-construction (extension/tests/numbering.test.mjs asserts that a list numbers the controls in document order with their role, name and the origin they go to, and gives the same numbers for the same page, that it is cut at 40 elements and 120 characters of a name and says how many it left out, that an element with a fingerprint over the limit is left out and counted with no gap in the numbers, that it holds no value of a field and never reads one, leaves out and counts the fields that are never acted on and the controls that are not displayed, writes a name that holds a line break or an unseen character as one line of text, leaves out a link to a listed site, and refuses a listed site, a site that is not allowed, no tab and two tabs before a page is reached; that a list is kept once, with a place and a fingerprint and no text, and a second replaces it; that a number with no list, for another URL, past its age, past the end and from a list that cannot be read is refused with a fixed sentence and reaches no page; that a list is good for five minutes and no longer; that Turn everything off and turning off either group forget the list, and so does turning a group on after its lifetime ended; and that an element in a shadow root is numbered and acted on; and that a link whose address is longer than 200 characters is judged and shown by its whole address; it runs where the test under BROWSER-13 runs)`
`verified-by: bravebot_browser::tools::the_list_of_elements_takes_a_url_and_its_description_says_the_model_does_not_read_it`
`verified-by: bravebot_browser::tools::an_element_is_named_by_a_role_and_a_name_or_by_the_number_a_list_gave`
`verified-by: bravebot_browser::extension::the_relay_states_the_bounds_of_the_list_the_extension_makes`
`verified-by: bravebot_browser::tool_surface::every_argument_of_every_tool_is_one_a_person_can_judge_in_the_question`

<a id="BROWSER-42"></a>
### BROWSER-42: `screen` takes one picture of what a person chooses in the browser's picker, and records a page as a GIF, and returns the picture and nothing else

`capture_screen` and `record_gif` are in `screen`. Their routing fields are
`brave-screen:capture_screen`, which takes no argument, and `brave-screen:record_gif`, which takes the
`url` of a tab and, optionally, a whole number of `frames` from 2 to 20. A person can approve each
field and its arguments alone from the question before the call. The group lists `desktopCapture` and
`tabs`, and the browser words the first as "Capture content of your screen"
([the design](../design/browser-extension.md#taking-a-picture-of-the-screen)). The group is on until
Brave restarts unless the person chooses otherwise ([BROWSER-24](#BROWSER-24)), and in no preset.

**Asked at every call.** A capture of the screen holds whatever is on it, including what is not a
page of Brave, and a recording holds a page as it changes. The server of `screen` is declared with
`--ask-every-call` ([SERVERS-21](mcp-servers.md#SERVERS-21)), so each call is put to the person
whatever a rule, a mode or an earlier answer says, and an answer is not kept. The browser shows its
own picker for each call to `capture_screen` as well, so the person answers two questions, and
neither is answered by the other. The declaration also has `--tool-timeout 120`, as the one for
`input` does ([BROWSER-34](#BROWSER-34)).

**Routing.** Neither tool acts on an element, so [ROUTE-9](routing.md#ROUTE-9)'s second answer does
not apply, and [ROUTE-7](routing.md#ROUTE-7), [SERVERS-7](mcp-servers.md#SERVERS-7) and
[SERVERS-21](mcp-servers.md#SERVERS-21) hold the group as they hold every server that asks every
call. The rules that make a call the person's own are the same ones: a call from a context that has
met untrusted content is refused with nobody asked, no rule or mode answers a call, a one-shot run
and a delegate are refused, and each result is a reference the planner cannot read. `capture_screen`
takes no argument, and a call to it from a context that has met untrusted content is refused all the
same.

**`capture_screen`.** The extension opens a small window of its own, `capture.html`, for each call,
one at a time; a second call while one waits is refused. The page asks the browser for its picker
with `chrome.desktopCapture.chooseDesktopMedia`, offering screens and windows and not tabs. Nothing
is captured until the person chooses. The page then opens a stream of what was chosen with
`getUserMedia`, takes the second frame of it, since the first can be blank, or the first when no second comes within a second of it, since a window whose content does not change sends no second, scales it so its longer
side is at most 2048 pixels, encodes it as a JPEG at quality 0.8, stops the stream, and sends the
worker the data URL in a message. The worker takes the message only from the window it opened, from
this extension, in the top frame ([BROWSER-17](#BROWSER-17) does not apply, since no page is
named), and passes on only a `data:image/jpeg;base64,` URL that decodes to at most 8 MiB. The person
has 60 seconds. The call fails, with a sentence of the extension's and nothing of the screen, when
the person closes the picker, closes the window or lets the time run out, and when the browser will
not show its picker, open what was chosen or give a frame; the page names which of those four as a
word the worker maps to a sentence, and a word it does not know is no reason. The window is closed and the stream is stopped on every
path, and the picker is hidden when the person cancels in the window or the window goes away.

The picker and the stream are in one page, and no offscreen document holds the stream. The browser
lets only the frame that asked for a stream use it: the same id in an offscreen document failed with
"Error starting tab capture", as an id never issued did, and an offscreen document has
`chrome.runtime` and no `chrome.desktopCapture`. A service worker may not call
`chooseDesktopMedia` without a target tab, and a target tab limits the stream to frames of that tab.
So the group lists `desktopCapture` and not `offscreen`, which stays reserved
([BROWSER-29](#BROWSER-29)).

**What the group reaches.** `capture_screen` names no page and reads none, so it is not limited to
the allowed sites ([BROWSER-22](#BROWSER-22)) and needs no host. It is refused, as every tool is,
unless the group is on within its lifetime and the browser holds `desktopCapture` and `tabs`
([BROWSER-18](#BROWSER-18)). The group lists `tabs` for `record_gif`. A picture of a screen may show a
page of a blocked site ([BROWSER-17](#BROWSER-17)), since the blocked list names the address of a
tab.

**`record_gif`.** It records the tab at the exact `url`, with the checks `screenshot_page` makes
([BROWSER-35](#BROWSER-35)) before each frame and after it: the URL is HTTP or HTTPS and not at a blocked
site, the group is on all sites and the browser holds `<all_urls>`, exactly one tab is at the URL and
is the visible tab of its window, the window is not minimized, and the tab is still there after the
capture. The tab is not brought forward. A frame is taken as `screenshot_page` takes a picture, so
frames are at least 600 milliseconds apart, and a frame that fails ends the recording with a
failure that says which frame and why, and no GIF. The default is 10 frames and the most is 20. Each
frame is decoded by the browser (`createImageBitmap`), scaled so its longer side is at most 480
pixels and to the size of the first frame, and read as pixels (`OffscreenCanvas`). `extension/gif.js`
writes them as a GIF89a that repeats for ever, with one palette of at most 256 colours for the
whole recording made by median cut over colours counted to 5 bits a channel, so a gradient shows
bands, no dithering, no transparency and every frame written whole. A frame stays as long as the next
took to arrive, from 2 to 300 hundredths of a second, and the last as long as the one before. A frame of N
pixels encodes to under 1.5 N bytes, so 20 frames of 480 by 480 are under 7 MB, below the 8 MiB the
client takes, whatever the page shows. The result is `image/gif`, one of the four types of
[MCP-6](mcp.md#MCP-6). A viewer plays the GIF, and how much of it a model reads when it is
vetted depends on the model's backend ([VET-4](tools/vet-content.md#VET-4)).

**The result.** `{"image": {"mimeType": "image/jpeg" or "image/gif", "data": <base64>}}` and no
other field. The activity log keeps the group, the tool, the outcome and the origin of the call and
never the picture ([BROWSER-27](#BROWSER-27)). The relay marks both tools as returning a picture in its
table and sends the `image` object as an MCP image part as for `screenshot_page`
([BROWSER-35](#BROWSER-35)), deciding nothing from the reply. The server of `screen` waits 90
seconds for the extension, 60 for the person and 30 more for the picture, and a server of any group
other than `scripts`, `input`, `input-trusted`, `capture-trusted`, `console-trusted`, `upload-trusted` and `screen` waits 30
([BROWSER-6](#BROWSER-6), [BROWSER-34](#BROWSER-34)). A picture is untrusted and
private ([MCP-1](mcp.md#MCP-1)), is held in a quarantined slot, is given to a processor as a picture
([PROC-15](processors.md#PROC-15)), and reaches the planner only through `vet_content`
([VET-4](tools/vet-content.md#VET-4)).

**Why.** A maintainer ruled that nothing the issue lists is left out and that the screen is an
untrusted reference the model looks at only after its content is vetted. The group asks for what the
browser needs and says so where the person turns it on. The call is asked every time because the
screen holds more than any page the lists name, and the picker is the browser's, so the extension
never chooses what is captured. The relay and the driver read no byte of the picture.

`verified-by: bravebot_browser::extension::the_extension_has_each_permission_its_tools_require`
`verified-by: bravebot_browser::extension::the_entries_that_wait_for_a_ruling_have_no_tool_permission_or_call`
`verified-by: bravebot_browser::extension::the_extension_captures_a_tab_in_one_call_with_all_urls_and_within_the_clients_cap`
`verified-by: bravebot_browser::extension::the_wait_for_input_is_longer_than_the_persons_and_the_declared_bound_is_longer_still`
`verified-by: bravebot_browser::tools::the_tools_that_return_a_picture_are_the_four_and_take_a_url_a_count_or_nothing`
`verified-by: bravebot_browser::tools::the_screen_descriptions_say_what_is_captured_who_chooses_and_that_the_model_does_not_see_it`
`verified-by: bravebot_browser::tools::only_scripts_and_the_input_groups_capture_trusted_console_trusted_upload_trusted_and_screen_ask_the_person_in_the_browser_and_are_asked_about_every_call`
`verified-by: bravebot_browser::relay::the_screen_tools_are_called_through_their_group_alone_and_arrive_as_image_parts`
`verified-by: bravebot_browser::relay::installing_declares_only_remove_data_cookies_scripts_the_input_groups_capture_trusted_console_trusted_upload_trusted_and_screen_to_ask_about_every_call`
`verified-by: bravebot_agent::browser_group_servers::a_call_to_a_screen_tool_from_a_context_that_has_met_untrusted_content_is_refused_before_anybody_is_asked`
`verified-by: bravebot_agent::browser_group_servers::neither_a_rule_nor_bypassing_answers_a_call_to_a_screen_tool`
`verified-by: bravebot_agent::browser_group_servers::a_run_with_nobody_to_ask_refuses_each_call_to_a_screen_tool`
`verified-by: bravebot_agent::browser_group_servers::a_delegate_is_refused_each_call_to_a_screen_tool_with_nobody_asked`
`verified-by: bravebot_agent::browser_group_servers::what_a_tool_of_screen_answers_is_a_reference_and_the_planner_never_holds_it`
`verified-by: bravebot_agent::browser_group_servers::a_reference_named_in_an_argument_of_a_screen_tool_reaches_the_server_as_the_words_written`
`verified-by: bravebot_agent::browser_group_servers::after_a_vet_a_call_to_a_screen_tool_is_put_to_the_person`
`verified-by: by-construction (extension/tests/screen.test.mjs asserts that capture_screen opens the capture window and closes it, answers the picture the window sends, takes an outcome from the window it opened in its top frame from this extension and from no other sender or kind of message, fails with its own words when the picker is closed, the window is closed or 60 seconds pass, says only a sentence of the worker's for a failure the window names and none for a word it does not know, refuses a second capture while one waits, is not limited to the allowed sites and needs no host, is refused before the window opens when the group is off or desktopCapture or tabs is not held and says nothing of sites, and passes on only a JPEG data URL of at most 8 MiB, and that capture.js asks the picker for screens and windows, opens only what was chosen, takes the first frame of a window whose content does not change when no second comes and fails on a first frame that has no size, scales a larger screen to 2048 pixels, encodes at quality 0.8, sends that nothing was chosen when the picker is closed, names why it failed, stops the stream on every path and hides the picker; that record_gif answers one GIF as its only field, scaled to fit, in order, each frame as long as the next took, takes frames 600 ms apart, takes 2 to 20 frames and 10 by default, refuses any other count before a tab is touched, scales every frame to the size of the first, is refused under the allowed sites, without <all_urls>, at a blocked site, for a URL that is not a page and where two tabs are at the URL, and ends with the frame's number when a frame fails; extension/tests/gif.test.mjs asserts the bytes of a small GIF, that every length of LZW data across each change of code size and a full table decodes to the same indices with a reader written apart from the encoder, that a picture of up to 256 colours is written exactly at 5 bits of precision, that more colours are cut to 256 near the original, that delays are rounded and bounded, that sizes the format cannot hold are refused, and that the largest recording the tool allows is under 8 MiB; extension/tests/groups.test.mjs, warnings.test.mjs, options.test.mjs, switches.test.mjs and upgrade.test.mjs assert the group, its permissions, Brave's words for them and the page's text for the group; all run where the test under BROWSER-13 runs)`

<a id="BROWSER-43"></a>
### BROWSER-43: screenshot_full_page takes a picture of the whole top document of one tab at one URL, only after the person allows it in a window of the second extension, and returns the picture and nothing about the page

`screenshot_full_page` is the one tool of `capture-trusted`. It takes `url`, the exact URL of one open
tab, and no other argument: the BraveBot extension sends the URL alone, so a size, a scale, a clip, a
format, a quality or a tab number in a call is dropped there, and the second extension refuses a
request that carries one. Its routing field is `brave-capture-trusted:screenshot_full_page` and its
one argument is the URL the question before the call shows, so a person can approve the field and
the argument alone. The group is shaped as `input-trusted` is ([BROWSER-18](#BROWSER-18)): it lists
no permission and reads no host itself, it is limited to the allowed sites whatever the scope, it is
in no preset, it stays on until Brave restarts unless the person chooses another time, its server
is declared with `--ask-every-call` and `--tool-timeout 120`, and its calls wait 90 seconds
([BROWSER-6](#BROWSER-6)). It is a group of its own and not a sixth tool of `input-trusted`, because
it sends no input and reads the whole of a page, including what nobody sees, so turning it on is
described as that. It is not in `screen`, which holds the picture of a screen and the recording of a page
([BROWSER-42](#BROWSER-42)).

**Before anything is sent.** The BraveBot extension checks the tool and group switches, the lifetime,
that the URL is an HTTP or HTTPS address, that it is not at a listed site ([BROWSER-17](#BROWSER-17))
and that it is at an allowed site ([BROWSER-22](#BROWSER-22)), and that no other action is waiting
for a person, and then sends the call to the second extension ([BROWSER-12](#BROWSER-12)). The
second extension finds the one tab at the URL ([BROWSER-13](#BROWSER-13)), which may be a tab in the
background, attaches the debugger, reads the address of the page and the size of its content in CSS
pixels, brings the tab forward and asks the person in the window of [BROWSER-38](#BROWSER-38). The
window names the action in words that say the picture takes in the parts out of view and any that
cannot be seen and that the browser's debugger is attached to the page and reads everything on it,
shows the origin and the address of the page and its size, and shows no element: the heading, the
rows and the note of the element are not on the page, and the size is not counted in characters. A
page with no size, or so long and narrow that a picture of at most 2,048
pixels on the long side would be less than a pixel wide, is refused before the person is asked.

**The picture.** Nothing is captured before the person has allowed it. After they have, the second
extension reads the address and the size again and refuses if the page left the URL, and then sends
`Page.captureScreenshot`, which is sent only with the token of that Allow
([BROWSER-37](#BROWSER-37)). The picture is a JPEG of quality 80 of a clip that starts at the top left
corner of the page and is the size of its content, taken beyond the viewport, at the scale that makes
the long side at most 2,048 pixels and is never above 1, so a small page is not enlarged. The scale
allows for the pixels the screen makes of a CSS pixel and is rounded down to four decimals. The
second extension reads the size of the picture from its header, and if the long side is over 2,048
pixels it takes the picture once more at the scale that size gives, and if that is over too it
refuses. A picture of more than 8 MiB once decoded is refused with a sentence and never cut or
resized. A capture that is not answered 10 seconds after the Allow ends the call, inside the 11 seconds that
[BROWSER-38](#BROWSER-38) gives a call, and the debugger
is detached however the call ends. The top-level document is what is captured: the clip is the size
of that document's content. A frame is in the picture as the page draws it, in the box the page
gives it, and what a frame holds beyond its box is not.

**The reply.** The second extension replies `{v: 1, result: {image: {mimeType, data}}}` and nothing
else: `mimeType` is `image/jpeg` and `data` is base64 in its canonical form, within 8 MiB once decoded.
The BraveBot extension checks the same of the reply, with no other field in the result or in `image`,
and passes the object on as `screenshot_page` does ([BROWSER-35](#BROWSER-35)). Which tool's reply is
a picture is said by a table in each of the three places, `PICTURE_METHODS` in `extension/trusted.js`,
`PICTURES` in `extension-trusted/requests.js` and `PICTURES` in `crates/browser/src/tools.rs`, and by
no reply: a picture as the reply to any other method, or a status word as the reply to this one, is
a reply that is not understood. Every other tool's reply is still a status word and an origin. The
relay sends the object as one MCP `image` part without reading a byte of it, and the client takes
it as a picture of a listed type within 8 MiB, quarantined and private, which becomes available only
through a processor or a person's vet ([MCP-6](mcp.md#MCP-6), [MCP-1](mcp.md#MCP-1)). The activity
log keeps the group, the tool, the outcome and the origin of the URL ([BROWSER-27](#BROWSER-27)), and
none of the picture.

**Why.** `captureVisibleTab` takes the visible part of the visible tab and nothing else, so a picture
of the whole page or of a tab in the background needs the debugger, which only the second extension
holds. The picture shows what the browser paints, including the parts a person has not scrolled to,
so the person is asked each time, in a window that shows the page and its size, and the model is
shown the picture only if the person lets it through at `vet_content`. A JPEG is asked for because a PNG of a page 2,048 pixels wide was 4 to 10 MiB
in Brave 1.98.62 where the JPEG was 0.5 to 2 MiB. The 8 MiB bound is the client's, so a picture is
never accepted here and left out there. The long side of 2,048 is the bound pasted pictures have.

`verified-by: bravebot_browser::tools::screenshot_full_page_is_in_capture_trusted_alone_and_takes_a_url_and_nothing_else`
`verified-by: bravebot_browser::tools::the_full_page_description_says_what_it_reads_who_is_asked_and_where_the_picture_goes`
`verified-by: bravebot_browser::tools::the_tools_that_return_a_picture_are_the_four_and_take_a_url_a_count_or_nothing`
`verified-by: bravebot_browser::server::the_full_page_picture_is_an_image_part_made_of_two_fields_of_the_reply`
`verified-by: bravebot_browser::server::a_call_to_capture_trusted_waits_for_the_extension_as_long_as_a_person_does`
`verified-by: bravebot_browser::extension::the_extension_captures_a_tab_in_one_call_with_all_urls_and_within_the_clients_cap`
`verified-by: bravebot_browser::extension::the_waits_of_a_trusted_input_call_grow_from_the_persons_window_to_the_declared_bound`
`verified-by: by-construction (extension-trusted/tests/capture.test.mjs asserts that the picture is taken as a JPEG of quality 80 of a clip from the top left corner of the page's content, beyond the viewport, after the window; that the window is asked after the page is brought forward and before anything is captured and shows the origin, the address and the size of the page and no element; that a denial, an answer of nothing and a token made for another question each capture nothing and detach; that the scale makes the long side 2,048 pixels or less, never enlarges, allows for the pixels the screen makes of a CSS pixel and rounds down; that a picture the browser made too large is taken again once at the scale its size gives and then refused; that a picture of exactly 8 MiB is returned and one a byte over is refused with the fixed sentence; that a page that left the address, a page with no size and a page too long and narrow are not captured and not asked about; that data which is not a JPEG of canonical base64 is the generic failure; that a capture nobody answers ends the call at its bound, detaches and frees the extension; that the debugger is detached on every path; that the request takes the address alone; that the reply holds the picture and nothing else; that the size of a JPEG is read from its header wherever the header is, from pictures a real Brave made; and that Page.captureScreenshot is named in cdp.js and sent from capture.js alone, only with a token; extension-trusted/tests/cdp.test.mjs asserts the list of methods with Page.captureScreenshot needing a token and no Emulation or Runtime method; extension-trusted/tests/confirmation.test.mjs asserts that the window for a picture says the debugger reads everything on the page, has no block of the element and no count of characters for the size, that every other action has them, and that the block is one section of the page; extension/tests/capture-trusted.test.mjs asserts that the call sends the address alone to the second extension and returns its picture and nothing else, that nothing is sent while the group or tool is off, the lifetime has ended or the site is not allowed or is blocked, that a reply is passed on as a picture only when it is a JPEG as canonical base64 within 8 MiB in an object that holds the picture alone, that the table and no reply says which tool's reply is a picture, that a refusal and a failure of the second extension are told with its sentence, that an absent extension is one sentence and a silent one ends the call at the wait, that no other action starts while the picture is taken, that the activity log holds nothing of the picture, and that a call through the second extension's own code returns the picture the browser took and a denial returns a refusal and takes none; extension/tests/options.test.mjs and groups.test.mjs and lifetimes.test.mjs assert what the options page says of the group, that it asks the browser for nothing and that it stays on until Brave restarts; make check-extension runs them)`
`verified-by: bravebot_agent::browser_group_servers::a_call_to_the_capture_tool_from_a_context_that_has_met_untrusted_content_is_refused_before_anybody_is_asked`

<a id="BROWSER-44"></a>
### BROWSER-44: read_console returns the console of one tab at one URL as text, only after the person allows it in a window of the second extension, and attaches the debugger to nothing before then

`read_console` is the one tool of `console-trusted`. It takes `url`, the exact URL of one open tab,
and two optional arguments: `levels`, one of `all`, `warnings_and_errors` and `errors`, which is `all`
when left out, and `max_entries`, a whole number from 1 to 200, which is 100 when left out. The
BraveBot extension sends those three and nothing else, so a tab number, a filter text, an expression
or a time in a call is dropped there, and the second extension refuses a request that carries one. Its
routing field is `brave-console-trusted:read_console`, and its arguments are an address, one of three
words and a count, which the question before the call shows, so a person can approve the field and
the arguments alone. The group is shaped as `input-trusted` is ([BROWSER-18](#BROWSER-18)): it lists
no permission and reads no host itself, it is limited to the allowed sites whatever the scope, it is
in no preset, it stays on until Brave restarts unless the person chooses another time, its server is
declared with `--ask-every-call` and `--tool-timeout 120`, and its calls wait 90 seconds
([BROWSER-6](#BROWSER-6)). It is a group of its own and not a sixth tool of `input-trusted`, because it
sends no input and reads what a page's scripts printed, which can include data from the person's
account, so turning it on is described as that.

**Before anything is attached.** The BraveBot extension checks the tool and group switches, the
lifetime, that the URL is an HTTP or HTTPS address, that it is not at a listed site
([BROWSER-17](#BROWSER-17)), that it is at an allowed site ([BROWSER-22](#BROWSER-22)), the two
arguments, and that no other action is waiting for a person, and then sends the call to the second
extension ([BROWSER-12](#BROWSER-12)). The second extension finds the one tab at the URL
([BROWSER-13](#BROWSER-13)), which may be a tab in the background, by asking the browser for its tabs and
without attaching the debugger, and asks the person in the window of [BROWSER-38](#BROWSER-38). The
window names the action in words that say the browser's debugger is attached to the page and reads
the page's console, which can hold anything the page's scripts printed, shows the origin and the
address of the page and what is read, such as "errors, the newest 20 at most", and shows no
element: the heading, the rows and the note of the element are not on the page, and what is read
is not counted in characters. It does not bring the tab forward. Nothing is
attached, enabled or read before the person allows it, and a denial, a closed window or no answer
attaches to nothing.

**What is read.** After the Allow, the second extension attaches, sends `Page.enable`, `Console.enable`
and `Log.enable`, and waits 500 milliseconds after the second answer. `Console.enable` and
`Log.enable` are on the list of [BROWSER-37](#BROWSER-37) as methods that need a token, and are sent
with the token of that Allow, so a read of the console is not sent without it. The Console domain
reports what a page's scripts wrote with `console.log` and the others and the errors they threw, and
the Log domain reports what the browser says about the page, such as a request that failed or a rule
of the page that was broken. Each sends what it collected so far when it is enabled and then the new
ones. Neither evaluates anything, and no method of `Runtime` is on the list. The session keeps the
events of its own tab, in the order the browser reported them, and reads the address of the page
again at the end and fails if the page left the URL. It detaches however the call ends, and the whole
of it after the Allow is bounded at 10 seconds, inside the 11 that [BROWSER-38](#BROWSER-38) gives a
call.

**The entries.** An entry is `{level, source, text, cut, url, line}`: a `level` of `debug`, `log`,
`info`, `warning` or `error`, with the browser's `verbose` as `debug` and a level not on the list as
`log`; a `source` of a closed list of fourteen words, `console-api` for a call of `console`, the
browser's own for the rest, and `other` for one not on it; the `text`, cut to 500 characters, and
`cut`, whether it was; the `url` of the script, cut to 300 characters; and the `line`, or 0. The
second extension keeps the entries of the levels asked for, the newest `max_entries` of them, and
counts the older ones it left out as `omitted`. It reads no other member of an event, so a stack, an
argument that is an object and a request number are not in the reply.

**The reply and the text.** The second extension replies `{v: 1, result: {origin, omitted, entries}}`
and nothing else. The BraveBot extension checks that the result holds exactly those three members,
that the origin is the origin of an address on the web, that there are at most 200 entries, and that
each entry holds exactly the six members within the bounds above, and a reply that does not is
not understood and none of it is passed on. Which tool's reply is a console is said by a table in each
of two places, `REPORT_METHODS` in `extension/trusted.js` and `REPORTS` in
`extension-trusted/requests.js`, and by no reply: a console as the reply to any other method, or a
status word as the reply to this one, is a reply that is not understood. The text the tool returns is
made in the BraveBot extension from the checked parts: a first line holding the origin, the number of
entries and `omitted`, and a line for each entry, each line one JSON object, with every character a
person cannot see written as the `\u` escape JSON has for it. A page's text therefore cannot end a
line, forge another, or change how a line is drawn, and an escape this code wrote is told from text
that spells one, because a backslash in the page's text is written twice. The relay sends the text as
the one text part of the result without reading it, and the client takes it as untrusted and private,
which becomes available to the model only through a processor or a person's vet
([MCP-1](mcp.md#MCP-1)). The activity log keeps the group, the tool, the outcome and the origin of the
URL ([BROWSER-27](#BROWSER-27)), and none of the console or of the arguments.

**Why.** The BraveBot extension's own APIs give no access to a page's console, and the Log domain does
not carry `console.log` or the errors a script throws: in Brave 1.98.62 it carried only a request that
failed with a 404. The `Runtime` events carry them with their arguments, and need `Runtime.enable`, a method
of the domain that can evaluate code, which [BROWSER-37](#BROWSER-37) keeps off the list so that a scan
can say nothing runs in a page. The Console domain carries the calls of `console` and, as a message
with the text `Uncaught`, an error nothing caught, and evaluates nothing, so the list gains two methods
that need a token and no method of `Runtime`. The token is the person's Allow, asked for in a window
that shows the page, because a console can hold anything a page printed: a token, a user's name,
the answer of a request. The person is asked without seeing what the console holds, as with
`read_page`, and what comes back goes to the person and to a processor, and to the model only if the
person lets it through at `vet_content`.

`verified-by: bravebot_browser::tools::read_console_is_in_console_trusted_alone_and_takes_a_url_a_level_word_and_a_bounded_count`
`verified-by: bravebot_browser::tools::the_console_description_says_who_reads_it_who_is_asked_what_is_returned_and_that_the_model_cannot_read_it`
`verified-by: bravebot_browser::server::a_call_to_console_trusted_waits_for_the_extension_as_long_as_a_person_does`
`verified-by: bravebot_browser::server::the_console_is_one_text_part_made_of_the_reply_as_it_is`
`verified-by: bravebot_browser::extension::the_waits_of_a_trusted_input_call_grow_from_the_persons_window_to_the_declared_bound`
`verified-by: by-construction (extension-trusted/tests/consoles.test.mjs asserts that the person is asked first, after only the browser's list of tabs, and shown the origin, the address and what is read and no element, that a denial, a window that answers nothing, something that is not a token and a token made for another question each enable no domain and read nothing and attach to nothing or detach, that the entries of both domains come in the order the browser reported them with the closed shape and no other member of the event, that levels keeps every entry, the warnings and errors or the errors alone, that no more than max_entries of the newest are kept and omitted counts the older ones of the levels asked for, that text and an address are cut at their limits by characters and a level, source or line outside the lists is the neutral value, that only the events of the tab that is read are collected and none before the domains are enabled or after the read ended, that the read waits a short time after the browser's answer, that a page that left the address is not reported, that a read the browser never answers ends at its limit and detaches and frees the extension, that the debugger is detached after every path after the Allow, that no tab or several end the call before the person is asked, that the request takes the address, levels and max_entries and nothing else, that a reply holds the report and nothing else and a report off the closed shape is the generic failure, and that the domains and their events are named in cdp.js alone and no script names Runtime; extension-trusted/tests/cdp.test.mjs asserts the list of methods with Console.enable and Log.enable needing a token and no Runtime method; extension-trusted/tests/confirmation.test.mjs asserts that the window for a console read says that the debugger reads the page's console, shows the origin, the address and what is read, and has no block of the element and no count of characters for what is read; extension/tests/console-trusted.test.mjs asserts that the words, limits and table are the same in both extensions, that the call sends the address, levels and count and nothing else and returns the text of the console, that a levels or a count out of bounds is refused before anything is sent, that nothing is sent while the group or tool is off, the lifetime has ended or the site is not allowed or blocked and that the group is limited to the allowed sites whatever scope is stored, that a reply is passed on only when it is of the closed shape and each reply that is not is not understood with none of it passed on, that the table and no reply says which tool's reply is a console, that the page's words are written so that they cannot end a line, forge another or draw differently and each line reads back as the entry it was made from, that an error of the second extension is a refusal or a failure with its sentence and an absent extension, a stopped worker and a silent one are told apart, that no other action starts while the console is read, that the activity log holds nothing of the console, and that a call through the second extension's own code returns the console the browser reported and a denial attaches to nothing; extension/tests/groups.test.mjs, lifetimes.test.mjs, options.test.mjs and warnings.test.mjs assert the group, its default lifetime, that the page says what the console holds, that another extension reads it after an Allow and that the model does not read it, and that it asks the browser for nothing; make check-extension runs them)`
`verified-by: bravebot_agent::browser_group_servers::a_call_to_the_console_tool_from_a_context_that_has_met_untrusted_content_is_refused_before_anybody_is_asked`

<a id="BROWSER-45"></a>
### BROWSER-45: choose_file gives one file of the uploads directory to one file input of one tab at one URL, after the relay has looked at it and the person has allowed it in a window of the second extension

`choose_file` is the one tool of `upload-trusted`. It takes `url`, the exact URL of one open tab,
`name`, the name of one file input as the page gives it (its accessible name, or for an input the page
hides the name its markup gives it), `path`, the file, and optionally `role`, which for a file input is
`button`. Its routing field is `brave-upload-trusted:choose_file`, and its arguments are
an address, a name and a path the person reads in the question before the call, so a person can
approve the field and the arguments alone ([ROUTE-9](routing.md#ROUTE-9)). The group is shaped as
`input-trusted` is ([BROWSER-18](#BROWSER-18)): it lists no permission and reads no host itself, it is
limited to the allowed sites whatever the scope, it is in no preset, it stays on until Brave restarts
unless the person chooses another time, its server is declared with `--ask-every-call` and
`--tool-timeout 120`, and its calls wait 90 seconds ([BROWSER-6](#BROWSER-6)). It is a group of its own
because it hands a file of the person's to a page, which turning on a group that clicks or reads does
not say.

**The directory.** The server's declaration names a directory with `--dir`, which is the directory
that holds the relay's socket and secret ([BROWSER-2](#BROWSER-2)): `~/.bravebot-browser` unless
`BRAVEBOT_BROWSER_DIR` names another. The server starts in it, reads it as `.`, and is confined with
that directory as its one write grant. A file can be chosen only from the directory `uploads` inside
it. The person makes that directory, which has to be a directory of its own and not a link, and puts
there the files the model may give to a page. The declared directory itself is not used because it
holds the secret.

**What the server looks at.** The server started for `upload-trusted` reads the `path` of a call of
`choose_file` and nothing else of the arguments, and looks at the file it leads to. A relative path is
taken from `uploads`, and an absolute path has to lead into it. The path is text of at most 900
characters. The file is the one the path leads to with every link followed, and it has to be inside
`uploads` with every link followed, so a link that leads out is refused and one that leads to another
file inside is that file. It has to be a regular file with one hard link and at most 10 MiB, its path
may hold no character a person cannot see, and no part of its name may look like a credential: `.env`, `.env.*`, `.netrc`, `.npmrc`, `.pgpass`, `.git-credentials`, the
directories `.ssh`, `.aws` and `.gnupg`, `id_rsa*`, `id_dsa*`, `id_ecdsa*`, `id_ed25519*`,
`credentials*`, `*.pem`, `*.key`, `*.p12`, `*.pfx` and `*.kdbx`, compared without regard to case. A
relative path is looked at for these as written, every part of it, and an absolute one by its last part,
before the file system is asked about it, so such a name is refused whether or not the file exists, and
the parts of the path the file leads to below `uploads` are looked at as well. The file's content is
never opened. A call that fails is answered with one of five sentences and nothing is sent to the
extension: the path is missing or not text; there is no `uploads` directory of its own; the path is not a
regular file inside `uploads`, said alike for a path that does not exist, one that leaves the directory,
a link that leads out, a directory, a file with more than one hard link, a path that holds a character
a person cannot see and a path that is not text once the links are followed (its bytes are not replaced
with U+FFFD, since the name that results can be another file's), so the answer does not tell what is
outside it; the name looks like a credential;
the file is over 10 MiB. A call that passes is sent with `path` replaced by the file's
canonical path and `size` and `file_name` added from the file, in place of any the planner wrote, and
every other argument as it was. The person reads the same sentence in the result of the call that the
planner is told. A character a person cannot see is one of the Unicode categories Cc (control), Cf
(format), Zl (line separator), Zp (paragraph separator) and Co (private use), in the relay and in both
extensions alike.

**How this fits the relay's rule.** The relay relays and decides nothing about what it relays
([layering.md](layering.md)). This is the second named exception to reading no argument of a call, after
the picture of [BROWSER-35](#BROWSER-35), which takes two fields of a reply. It is a decision about a
request and not about a result: the path is a word the planner wrote in a context that has met nothing
untrusted ([LABEL-5](labels.md#LABEL-5)), the directory is the one the person declared, and what is looked
at is the file system under the confinement's look at any path ([SANDBOX-13](sandboxing.md#SANDBOX-13)).
No byte of a page or of a result is read to take it, the outcomes are a closed list of sentences, and
the driver is not involved. Which tool takes a file is said by a table in the relay
(`tools::FILES`) and by no byte of a request or a result. The half Brave starts unconfined, the host, reads
the `id` of a message and nothing else, as before ([BROWSER-7](#BROWSER-7)).

**Before the window.** The BraveBot extension checks the tool and group switches, the lifetime, that the
URL is an HTTP or HTTPS address, that it is not at a listed site ([BROWSER-17](#BROWSER-17)), that it is at an
allowed site ([BROWSER-22](#BROWSER-22)), that the name and the role are of the shapes of
[BROWSER-33](#BROWSER-33), that the file is of the form the relay sends (an absolute path of at most 900
characters with nothing a person cannot see, a size from 0 to 10 MiB, the name the last part of the path)
and that no part of the path after the last directory named `uploads` looks like a credential (any part,
for a path with no such directory), and that no other action is waiting for a person,
and sends the call to the second extension ([BROWSER-12](#BROWSER-12)). The second extension checks the same
of the request, from the same `files.js`, before it looks for the tab. It finds the one tab at the URL
([BROWSER-13](#BROWSER-13)), attaches, and finds the one file input of the top document with the name and,
where one is given, the role, by the tag and the type attribute and not by the role, since the tree calls
every file input a button whatever it is named. It is not disabled, and it is refused as a payment card
field like any element ([BROWSER-38](#BROWSER-38)). This is the one resolution that does not refuse a file
chooser, and it refuses every element that is not one.

**An input the page hides.** Many sites hide the file input (`display: none`, the `hidden` attribute,
`visibility: hidden`, no opacity, no size or a clip) and draw a label or a button that opens it, so the
input is found whether the page shows it or hides it. An input the tree holds is named as the tree names
it. One the tree leaves out is named by its own markup, with no script of the page run and with the reads
of [BROWSER-37](#BROWSER-37) only: the elements its `aria-labelledby` points to, its `aria-label`, the
labels it has (the `label` whose `for` is its id, and the `label` around it that has no `for` and holds it
as the first element a label can name, each only within the same document or shadow root), and its
`title`, in that order, and the first that has words is its name. The text of a label or of an element
`aria-labelledby` points to is the text the tree holds in it, so a label that is not in the tree names
nothing. An input that none of them names has its `name` attribute and its `id`, and either is its name.
A frame's document is not searched. A button whose script opens the input is not tied to it by any
markup, so such an input is found only by its `name` or its `id`, or not at all. The inputs with the name
that are displayed are counted, and the ones the page hides are counted only where none is displayed, as
the other tools count only the displayed elements. One is the input, and two are refused as several. An
input the page hides is not scrolled to and not tested for a cover, since the browser is given the node and
not a point, and a disabled one, also by a `fieldset`, fails like any element.

**The window.** The person is asked in the window of [BROWSER-38](#BROWSER-38). It names the action in
words that say the browser reads the file from the person's computer itself and gives it to the page as if
the person had chosen it in the file picker, that a page whose script sends a file when it is chosen
receives it as soon as the person presses Allow once, and that no byte of the file passes through
BraveBot. For an input the page hides it says that the page hides the input, so that it cannot be seen in
the page, and that the name shown is what the page's own label, `aria-label` or `title` calls it, or, for
an input named only by its id or name, that the page gives it no label and the name is the id or name of
the input in the page's markup, which says little of what it is for. It shows the site and address, the
input in the rows of an element, with where its form goes, and in
a frame of its own the file's name, its size in bytes and the full path. Allow once is asked for on every
call. Nothing is attached beyond reading the page, and no file is chosen, before the person allows it, and
a denial, a closed window or no answer chooses nothing.

**After the Allow.** The second extension finds the input again and acts only if it is the same element, with
the same name, destination and way of being named, so an input the page shows or renames while the person
decides is not chosen on. It sends `DOM.setFileInputFiles` once, with the one path and the input's
backend node number, with the token of the Allow, after `Page.enable`. The method is on the list of
[BROWSER-37](#BROWSER-37) in a class of its own: it needs the token as input does and `command()` sends it only
with one path that `files.js` accepts and a node number, so a second file, a relative path, a path with `..`,
a credential name or any other parameter is refused and not sent. The browser reads the file and sets it on
the input, and the page's `change` event runs as if the person had chosen it. A dialog the page opens
in response is answered by the person in the panel of [BROWSER-38](#BROWSER-38). The reply is `chose` and the
origin of the page, and nothing of the page, the input or the file. The activity log keeps the group, the
tool, the outcome and the origin ([BROWSER-27](#BROWSER-27)), and never the path or the name of the file.

**Why.** An extension cannot read a file or put one on an input without the browser. The debugger can ask the
browser to set a file on an input, and the browser reads the file, so no byte of it passes through native
messaging, the relay, the BraveBot extension or the agent, and the 1 MB a message to the extension may hold
([BROWSER-8](#BROWSER-8)) does not limit the file. Something has to say which files may be given, and the
extensions cannot look at the disk. The server is confined to the directory the person declared and sees
the planner's own word, so it is the place that can refuse before anybody is asked, and a person who has
put a file in `uploads` has said the model may offer it. The window is the second answer
[ROUTE-9](routing.md#ROUTE-9) requires: choosing a file sends nothing by itself, but the page decides what
its script does with it, so the person is told which page gets which file.

`verified-by: bravebot_browser::uploads::a_file_in_the_uploads_directory_is_resolved_to_its_canonical_path_size_and_name`
`verified-by: bravebot_browser::uploads::a_path_that_leaves_the_uploads_directory_is_refused_with_the_sentence_of_a_missing_file`
`verified-by: bravebot_browser::uploads::a_link_that_leaves_the_directory_is_refused_and_one_inside_it_is_followed`
`verified-by: bravebot_browser::uploads::a_directory_is_not_a_file`
`verified-by: bravebot_browser::uploads::a_file_over_ten_mebibytes_is_refused_and_one_of_ten_is_chosen`
`verified-by: bravebot_browser::uploads::a_name_that_looks_like_a_credential_is_refused_as_written_and_as_resolved`
`verified-by: bravebot_browser::uploads::the_directories_above_the_uploads_directory_are_not_looked_at_for_a_credential`
`verified-by: bravebot_browser::uploads::the_names_of_the_list_both_halves_read_get_the_result_the_list_gives`
`verified-by: bravebot_browser::uploads::a_character_is_unseen_for_exactly_the_code_points_of_the_list_both_halves_read`
`verified-by: bravebot_browser::uploads::a_file_with_another_hard_link_is_refused_and_a_symbolic_link_to_a_single_one_is_followed`
`verified-by: bravebot_browser::uploads::an_uploads_directory_that_is_missing_a_file_or_a_link_is_refused`
`verified-by: bravebot_browser::uploads::a_missing_empty_or_overlong_path_is_refused`
`verified-by: bravebot_browser::uploads::a_path_or_name_that_is_not_text_is_refused_and_the_one_spelled_like_it_is_sent_as_itself`
`verified-by: bravebot_browser::uploads::a_name_that_is_not_utf8_is_refused_and_is_not_its_lossy_lookalike`
`verified-by: bravebot_browser::uploads::the_request_carries_the_files_own_path_size_and_name_and_none_the_planner_wrote`
`verified-by: bravebot_browser::server::choose_file_is_sent_with_the_path_size_and_name_of_the_file_it_leads_to`
`verified-by: bravebot_browser::server::a_path_that_is_refused_is_never_sent_and_is_answered_with_one_sentence`
`verified-by: bravebot_browser::server::choose_file_is_answered_by_the_server_of_upload_trusted_alone`
`verified-by: bravebot_browser::tools::choose_file_is_in_upload_trusted_alone_and_takes_a_url_a_name_a_role_and_a_path`
`verified-by: bravebot_browser::tools::the_upload_description_says_who_chooses_it_which_files_and_that_the_page_may_take_it`
`verified-by: bravebot_browser::relay::a_file_is_chosen_through_its_group_with_the_files_own_path_size_and_name_and_a_refused_path_is_never_sent`
`verified-by: bravebot_browser::extension::the_relay_and_both_extensions_hold_the_same_credential_names_and_the_same_limits_for_a_file`
`verified-by: bravebot_browser::extension::the_waits_of_a_trusted_input_call_grow_from_the_persons_window_to_the_declared_bound`
`verified-by: bravebot_agent::browser_group_servers::a_call_to_the_upload_tool_from_a_context_that_has_met_untrusted_content_is_refused_before_anybody_is_asked`
`verified-by: bravebot_agent::browser_group_servers::neither_a_rule_nor_bypassing_answers_a_call_to_the_upload_tool`
`verified-by: bravebot_agent::browser_group_servers::a_run_with_nobody_to_ask_refuses_a_call_to_the_upload_tool`
`verified-by: bravebot_agent::browser_group_servers::a_delegate_is_refused_a_call_to_the_upload_tool_with_nobody_asked`
`verified-by: bravebot_agent::browser_group_servers::after_a_vet_a_call_to_the_upload_tool_is_put_to_the_person`
`verified-by: by-construction (extension-trusted/tests/tools.test.mjs asserts that a file is chosen on the one file input a name leads to only after the person allows it, with the browser asked for that one file on that node and nothing else, that the window was asked first with the input and the file, that a denial and a window that gives no token choose nothing, that the role may be left out or be button and another role finds none, that an element that is not a file input is not found whatever it is called, that a file is chosen on an input the page hides in a window that says the page hides it and how the page names it, that one the page moves, shows or renames while the person decides is not chosen on, that a disabled input and a payment card field are not chosen on, hidden or not, that a path, size or name that is not what the relay sends and a name like a credential are refused before the tab is touched and before any window, that an input the page replaces while the person decides is not chosen on, and that the reply is the word chose and the origin; extension-trusted/tests/resolve.test.mjs asserts that resolveFileInput finds the one displayed file input by its tag and type with or without the role while resolveElement still refuses it, finds no element that is not a file input, counts two inputs with one name as several, and holds a file input to the rules of disabled, covered and payment card, and, against a page recorded in Brave 1.98.63 (recorded/hidden-inputs.html, recorded/hidden-world.json), that it finds an input the page hides in each way the page hides it by the name the page gives it, in the order of aria-labelledby, aria-label, labels and title and then the name or id, with the labels of its own document or shadow root only, finds none in a frame's document or by a name a label outranks, prefers a displayed input to hidden ones of the name and counts two hidden ones and more than the cap as several, scrolls to and tests the cover of a displayed input only, and gives a different fingerprint to an input the page shows, moves or names another way; extension-trusted/tests/cdp.test.mjs asserts that DOM.setFileInputFiles needs the token and is sent only with one path files.js accepts and a node number and nothing else; extension-trusted/tests/requests.test.mjs asserts the arguments of the method, that a path and a name are counted in code points, and the reply; extension-trusted/tests/confirmation.test.mjs and actions.test.mjs assert that the window says the browser reads the file and shows the file in a frame of its own, in the three actions, and that the two for an input the page hides say that the page hides it and what the name is; extension/tests/upload-trusted.test.mjs asserts that both copies of files.js give each name of the list crates/browser/tests/file-names.json the result the relay gives it, that the unseen characters of the engine are the ranges of that list, that only the parts after the last directory named uploads are looked at for a credential, and that the two extensions hold the same limits and credential names, that the call sends the address, name and the file the relay resolved and no other argument, that nothing is sent while the group or tool is off, the lifetime has ended or the site is not allowed or blocked, that a file of another form or named like a credential is refused first, and that the activity log holds nothing of the file; extension/tests/groups.test.mjs, lifetimes.test.mjs, options.test.mjs and warnings.test.mjs assert the group, its default lifetime, what the page says of it and that it asks the browser for nothing; make check-extension runs them)`

<a id="BROWSER-46"></a>
### BROWSER-46: run_script runs the code the planner wrote in the top document of one tab at one URL after the person has read all of it in a window of the BraveBot extension, and nothing but that reading limits what the code does

`run_script` is the one tool of `scripts`. It takes `url`, the exact URL of one open tab, an HTTP or
HTTPS address of at most 300 characters, and `code`, text of at most 2,000 characters. Its routing
field is `brave-scripts:run_script`, and its arguments are an address and code that the person reads
in full in a window before the code runs, so a person can approve the field and the arguments alone
([ROUTE-11](routing.md#ROUTE-11)). The group follows `remove-data` in the table
([BROWSER-18](#BROWSER-18)). It lists `userScripts` and reads pages, and its line carries `allowedOnly`:
it needs the host of each site the person allowed and never `https://*/*`, `http://*/*` or
`<all_urls>`, a call whose URL is not at an allowed site is refused ([BROWSER-22](#BROWSER-22)), and the
options page shows no choice of all sites for it. It is in no preset, it stays on until Brave restarts
unless the person chooses another time ([BROWSER-24](#BROWSER-24)), and its server is declared with
`--ask-every-call` and `--tool-timeout 120` ([BROWSER-21](#BROWSER-21)), its calls waiting 90 seconds
([BROWSER-6](#BROWSER-6)). It is a group of its own because running code is not what turning on a group
that reads or clicks says.

**What it needs of the browser.** The manifest offers `userScripts` as an optional permission, and its
`minimum_chrome_version` is 135, the first Chrome with `userScripts.execute`, so a browser older than
that does not install the extension ([BROWSER-18](#BROWSER-18)). In Brave 1.98.63 a request for the
permission resolved granted at once, and the list of install warnings for a manifest that holds it was
empty. The
browser lets an extension use `userScripts` only while the person has turned on Allow User Scripts for
it on its details page at `brave://extensions` (in a browser older than Chrome 138, Developer mode),
which the extension cannot turn on. With the permission held and the switch off, `chrome.userScripts`
was an object whose methods threw that they are not available in this context (`getWorldConfigurations`
in Brave 1.98.63, `execute`, `register`, `configureWorld` and `getScripts` in 1.98.62), so the
extension calls `getWorldConfigurations` to find out and does not go by the type of the namespace.
While that call fails, `run_script` is refused before anybody is asked with one sentence: `run_script
is refused until Allow User Scripts is turned on for the BraveBot extension on its details page at
brave://extensions (in a browser older than Chrome 138, until Developer mode is turned on at
brave://extensions)`. The options page says the same under the group, and says whether the switch is
on once the group is on, since the browser shows the extension its API only then.

**Before the window.** The extension checks the tool and group switches, the lifetime, the permission
and the hosts ([BROWSER-18](#BROWSER-18)), that the URL is at an allowed site ([BROWSER-22](#BROWSER-22))
and not a blocked one ([BROWSER-17](#BROWSER-17)), that `code` is text with something in it, of at most
2,000 characters, and not only the name of a reference (`ref:` and a number), that the URL is an HTTP
or HTTPS address of at most 300 characters, that exactly one open tab is at the URL
([BROWSER-13](#BROWSER-13)), that `userScripts` is usable, and that no other action is waiting for a
person ([BROWSER-34](#BROWSER-34)). A code or an address over its limit is refused and is never cut,
because the window cannot be allowed for what it cannot show whole. Each failure is answered before a
window is opened.

**The policy is tested first.** The extension makes a name for a world that no other call uses,
configures it with the content security policy `default-src 'none'` and `messaging` false, and runs a
probe in it, in the top document of the tab, before the window is opened. The probe is
`fetch("data:,x")` with its two outcomes named: it answers `closed` where the policy stopped the request
and `open` where it did not. It is text of this extension, reads nothing of the page, sets no variable
and holds none of the planner's code, which does not reach the browser before the person allows it.
Where the browser fails the probe, the call fails with the browser's words, as a failure of the code
does; where it answers anything but `closed`, the call is refused with `the browser did not apply the
policy that stops the script's own requests in this page, so nothing was run; reload the page and ask
again`, and no window is opened. The document the probe ran in is the document the person is shown, and
the code is run in it and in no other. The probe is answered `closed` for the first ten worlds a
document is given in Brave 1.98.63 and `open` after that, so the eleventh call on one page, a call the
person denied counting as one, is refused until the page is reloaded.

**The window.** The person is asked in the window of [BROWSER-34](#BROWSER-34). It shows the site, the
exact address in a box that holds all 300 characters of the largest address the tool takes, and in a
frame of its own the whole code: every line as a line, each line feed and each character a person
cannot see written out by its code point, including a variation selector inside a name, a count of
the lines and of the characters, and no part cut. Code of more lines than the frame shows is
scrolled, and Allow once is not available until the frame has been scrolled to its end. It shows no element, since the action acts on none. Its words are: Run the
code below in the page, as you, with the session you are signed in with. Read all of it. Nothing limits
what it does except that you read it: it can read everything on the page, including what is hidden,
click and submit the page's forms, and send what it read to another address by loading a frame or
going to a link. It starts once. Timers, listeners and observers it sets keep running in the page until
the page is closed or reloaded, and each can read and send without asking again. What it answers is
sent to BraveBot. Allow once is asked for on every call, and a denial, a closed window and no answer
within 60 seconds run nothing. The files of the window are those both extensions share ([BROWSER-37](#BROWSER-37)). The question BraveBot puts
before the call ([SERVERS-7](mcp-servers.md#SERVERS-7)) draws the same code in the terminal and the
plain interface, one numbered row to a line with each character a person cannot see written out and
counted ([ROUTE-11](routing.md#ROUTE-11)), and the person then reads it again in the window with the
exact address. The desktop window and an editor are asked nothing and refuse the call, since the
server asks about every call ([SERVERS-21](mcp-servers.md#SERVERS-21)).

**After the Allow.** The extension finds that exactly one open tab is at the URL and that it is the tab the
person was shown, and fails the call with nothing run if not. It calls `userScripts.execute` once, in the
world of the call, with the code as written and the document the probe ran in named as `documentIds` of
the target, waiting at most 15 seconds. A page that reloaded or went to another address while the person
decided has another document, which the browser refuses with its own words, and the call fails with
nothing run. The code runs as a script and not as the body of a function, so its answer is the value of its
last expression or of the promise it ends with, and a top-level `return` or `await`, or a character
that JavaScript does not accept, such as a zero-width space outside a string or a comment, is a syntax
error. The tool's description says so, and tells the planner to wrap the code in an async function
and call it. The world's configuration is reset when the call ends, however it ends. That deletes the
stored configuration and nothing the code started: its world, with its timers, intervals, listeners and
observers, stays in the page until the page is closed or reloaded. The tab has to be at the URL when
the code has returned, and the answer of a tab that left it is not returned. The reply is
`ran`, the origin of the page and the answer as text of at most 100,000 characters, with `cut` where it
was cut. A string is the text, and any other answer is its JSON. A script that answers nothing, one
that throws and one with a syntax error are all the answer `null` in Brave 1.98.63, which does not say
which. A script that has not answered within 15 seconds fails the call and is not stopped. A failure of
the browser is told in the browser's words after the address, which for a page Brave protects from
scripts is its sentence. The activity log keeps the group, the tool, the outcome and the origin
([BROWSER-27](#BROWSER-27)), and never the code or the answer.

**What limits the code.** Brave 1.98.62 and, for the cases named, 1.98.63 were run with a world
configured with `default-src 'none'` and a page and two other origins on 127.0.0.1. The policy stopped
`fetch` to another origin and to the page's own origin (1.98.63 as well), `XMLHttpRequest` (1.98.63 as well),
`sendBeacon`, which still returns true and sends nothing (1.98.63 as well), `WebSocket` (1.98.63 as well), `EventSource`,
an image element (1.98.63 as well), a `link` element for a style or a prefetch, a `script` element and
`import()`. It did not stop a frame whose `src` carries the page's hidden field in its address (1.98.63
as well), a frame with `srcdoc` or a `data:` address whose script fetches, an image in a style, an
`audio` or `object` element, a `Worker` made from a blob whose script fetches, a handler set as an
attribute and then clicked, a navigation by assigning `location` (1.98.63 as well), by submitting a form
(1.98.63 as well), by clicking a link the code made or by a `meta` refresh, or `window.open` and
`target=_blank` once the page has had a click, which return nothing without one (1.98.63 as well for
`window.open`). The code read
everything in the document, hidden fields included, the cookies that are not `HttpOnly` and the storage
of the page's origin, and the page's own handlers ran when the code clicked, filled in or submitted.
The page's own JavaScript values were not reachable from it. What the code started in the page was
still running after the call had answered `ran`: in Brave 1.98.63 an interval went on changing the
page's title, a `keydown` listener counted a key typed after the call, and a `setTimeout` of 3.5 seconds
navigated the tab to another origin with the title in the address. A reload of the page ended them. The
check that the tab is at the URL when the code returns does not see a navigation the code started,
since the page commits after the answer:
the `location` assignment and the form submission were each answered `ran`, and the other origin
received the request. So the policy stops some of the ways the code can send what it read to an address
and leaves open others that reach any address, and the one limit on what the code reaches is that the
person read all of it.

**How many worlds the policy holds for.** Brave 1.98.63 applied the policy to the first ten worlds a
document was given, whether or not the call reset them. The eleventh world was made with no policy: a
`fetch` of another origin reached the other origin's server. The twelfth and later calls ran in the
eleventh's world and saw its variables. A reload of the page started the count again, and another tab
had a count of its own. A page whose own policy forbade `connect-src` did not change the answer of the
probe. One world named by every call did not run out (25 calls were stopped), shared its variables
between calls and failed a second `const` of the same name (the answer was `null`), which is why each
call has its own world and the extension tests the policy instead.

**Why.** Code in a page cannot be turned into a smaller thing a person can judge, and no part of this
process may read it, so what a person can approve is the text itself for one page. The window is the
second answer [ROUTE-11](routing.md#ROUTE-11) requires, in a place the page cannot rewrite, and it says
in its own words what the code can do so that a person is not told the policy protects them. The policy
is configured anyway, since it stops the code's own requests of the fetch family without a person
having to notice them in the code, and a call is refused where the browser did not apply it, so that
what the window and the options page say of it holds for every call that runs. The probe runs before the
window so that a person is not asked to read code that will not run, and it is the only code `run_script`
runs in a page before the person allows the call.

`verified-by: bravebot_browser::tools::run_script_is_in_scripts_alone_and_takes_a_url_and_code_with_their_largest_sizes`
`verified-by: bravebot_browser::tools::the_script_description_says_the_person_reads_all_the_code_nothing_else_limits_it_and_the_result_is_private`
`verified-by: bravebot_browser::server::a_call_to_scripts_waits_for_the_extension_as_long_as_a_person_does`
`verified-by: bravebot_browser::relay::a_script_is_called_through_its_group_alone_and_the_extension_is_sent_the_code_as_written`
`verified-by: bravebot_browser::extension::the_wait_for_input_is_longer_than_the_persons_and_the_declared_bound_is_longer_still`
`verified-by: bravebot_browser::install::the_declarations_of_scripts_the_input_groups_capture_trusted_console_trusted_upload_trusted_and_screen_ask_about_every_call_and_bound_a_call_above_the_servers_wait`
`verified-by: bravebot_agent::browser_group_servers::neither_a_rule_nor_bypassing_answers_a_call_to_the_script_tool`
`verified-by: bravebot_browser::extension::the_words_that_say_the_code_starts_once_and_what_it_sets_going_stays_are_the_same_where_the_person_decides`
`verified-by: by-construction (extension/tests/scripts.test.mjs asserts that the code reaches userScripts.execute as written, for the document the probe ran in, in a world configured with default-src 'none' and no messaging, a world that no other call uses and that is reset after the call however it ends, only after the person was asked and allowed it; that the probe is text of the extension's own, run in that world before the person is asked, and that none of the planner's code reaches the browser before; that a world the browser gave no policy, any answer of the probe but closed, and the eleventh call on one document are each refused with the one sentence before anybody is asked, and a reloaded page runs again; that a probe the browser fails, one the page does not answer in 15 seconds and one with no result or no document fail the call before anybody is asked; that a page reloaded at the same address while the person decided is not scripted; that exactly one tab, the one that was shown, has to be at the address when the person allows; that the window says the code starts once and what it sets going keeps running; that a denial, a closed window, no answer, a token made for another call and a token used before run nothing; that a blank code, one over 2,000 characters and one that is only the name of a reference, an address over 300 characters, an address that is not a web address, a blocked site, a site that is not allowed, a tab that is not exactly one, a group that is off, a tool that is off, a permission or host that is not held and a userScripts that is absent or switched off are each refused before anybody is asked, the last with the one sentence; that the window is given the complete code and the exact address and nothing else, and through the real window that the code runs when Allow once is pressed and not before; that a tab which left the address while the person decided is not scripted and one which left while the script ran returns nothing; that an answer is text, is cut at 100,000 characters without half of a character and says so, and that a failure of the browser, a script that does not answer in 15 seconds and a second call while one waits are each told; that the activity log keeps no code; and that the manifest offers userScripts as an optional permission only, holds no host and asks for Chrome 135; extension/tests/confirmation.test.mjs asserts that the window shows every line of the code with each line feed and each unseen character written out, the count of lines and characters, and no element, that it says in its own words that nothing limits the code except the reading, that code of exactly 2,000 characters is shown whole and one more cannot be allowed, and that a line feed in typed text is written out and does not break the line; extension/tests/groups.test.mjs asserts that the group lists userScripts and the hosts of the allowed sites and never all sites whatever scope is stored, and is in no preset; extension/tests/switches.test.mjs asserts that turning it on asks for userScripts and the allowed sites only and that it has no scope; extension/tests/options.test.mjs asserts that the page says nothing limits the code but the reading, shows no choice of sites for the group, says how to turn on Allow User Scripts and, once the group is on, whether it is on, that the code starts once and what it sets going keeps running, and that a call is refused where the browser did not apply the policy; extension/tests/upgrade.test.mjs asserts that minimum_chrome_version is the newest Chrome any group needs, which userScripts makes 135; extension/tests/shared.test.mjs asserts that the files of the window are the same in both extensions; all run where the test under BROWSER-13 runs)`

## Known costs

- A line of the list `list_page_elements` makes is the page's words. A page can name a control
  "Cancel" and make it submit a payment, and a person who gives that number is shown the element, its
  name as the page gave it and where it leads, in the window ([BROWSER-41](#BROWSER-41)). A person who
  allows without comparing the window with the line they chose has let the page choose. The question
  shows `element_number` and the number and not the line, which is in the transcript.
- A picture of the screen holds everything on it, including a site on the blocked list, a password
  manager, another program's window and BraveBot's own capture window if the person chooses the whole
  screen. The blocked list names the address of a tab and the extension cannot read a screen
  ([BROWSER-42](#BROWSER-42)). The person chooses what is shared, and the model does not see it unless the person lets it through at
  `vet_content`.
- The picker and the stream were observed in a headless Brave with a fake screen and a switch that
  answers the picker. A real picker, a real screen and the permission prompt for `desktopCapture`
  were not seen ([BROWSER-42](#BROWSER-42)).
- A real click, a press and a release land on a point in the window and not on a node. A page can put
  another element at the point between the last question to the browser and the event, and the event
  reaches it. [BROWSER-38](#BROWSER-38) asks again after the move and before each press, which
  narrows the time and does not remove it. What is inside the element is read again for each
  question, so a node made inside the element, or moved into it, since the resolution is a part
  of it, and a layer the page puts inside the element before the question catches the press. The
  focus has the same time: a key and typed text go to whatever has the focus, which is asked just
  before each of them and not at the moment the browser handles it. A drag ends with a
  release over the second element, and the browser may send a `click` to the nearest element above
  both ends, whose link or form is not shown.
- A drag that fails after the press releases the button at the point it pressed at, the element the
  person was shown, and not at the second element, where something may now sit. A click that fails
  after the press releases it at that point too. Where the browser answers no input then, as while a
  dialog nobody answered is open or once the debugger is gone, the release does not arrive and the
  button stays held in the page until the next mouse event. The page sees the release as the end of
  a drag that went nowhere. A release tried again after one whose answer was lost reaches a page that
  had already seen the first as a second `pointerup` and `mouseup` and no `click`, which a release
  with no press makes in Brave 1.98.62.
- A key up tried again after one whose answer was lost reaches a page that had already seen the first
  as a second `keyup` with no `keydown` before it, which a key up with no key down makes in Brave
  1.98.62. Where the browser answers no input, the key stays down in the page's view until a key up
  arrives, and so it does where the focus moved between the key down and the key up, since the up
  would go to another element ([BROWSER-38](#BROWSER-38)). A call that runs out of time sends the
  release it owes at the end, and where the browser has stopped answering that release is no more
  certain to arrive than the others; it is sent once and its answer is waited for 1 second.
- The `input-trusted`, `capture-trusted`, `console-trusted` and `upload-trusted` groups have no off state held by the browser. The second extension holds
  `debugger` on every site while it is enabled, because the permission cannot be optional, and
  nothing asks the person for it when a call is made. The states that stop it are the switch and
  lifetime of the group in the BraveBot extension, the allowed sites, and the second extension
  being turned off at `brave://extensions`, which the BraveBot extension cannot do. A call that
  reaches it still does nothing until the person allows it in its window.
- Saving an allowed site asks the browser for access to it for the BraveBot extension, whether or
  not a group that reads pages is on ([BROWSER-22](#BROWSER-22)). A person who uses `input-trusted`, `capture-trusted`, `console-trusted` or
  `upload-trusted` alone therefore gives that extension access to the sites they allow, which the group does not use.
- The second extension has not been run by a person at a headed Brave. Its code was tested against a fake
  debugger and run in a headless Brave 1.98.62, and its debugging notice and windows were measured in a
  visible Brave 1.98.63, on macOS. A person's click on Allow once, the install prompt, what **Cancel** on
  the debugging notice does to a call, a window losing the front to another, a native context menu from a
  right click, a native drag in a headed window, an action on a page of the three domains Brave protects
  from scripts, Linux, and the two extensions with the real relay and host together were not observed
  ([BROWSER-37](#BROWSER-37), [BROWSER-38](#BROWSER-38)).
- The second extension sees a dialog only while one call's input is being sent. A dialog the page
  opens after the browser has answered that input, or with no input of ours, stays open in the page
  and is answered by the person in Brave, since the browser lets no later session answer it.
- The secret in [BROWSER-3](#BROWSER-3) is only as private as the directory. Any process of the
  person's that can read it can use the relay, which is the same account boundary a keychain gives.
  Such a process is not held to the group a server was declared for, since the group is chosen by
  the server ([BROWSER-21](#BROWSER-21)) and the extension answers every method it has. What stops
  it is what the extension checks at each call: the tool's switch, the group's switch and lifetime,
  the blocked and allowed sites, and the permissions the browser holds when the call arrives
  ([BROWSER-14](#BROWSER-14)). A group the person turned on is open to it as it is to a session,
  without the question BraveBot puts to a session. The call is added to the activity log whether it
  ran or was refused ([BROWSER-27](#BROWSER-27)), which is where the person can see it.
- One Brave profile at a time. A second profile with the extension installed starts a second host,
  which finds the first one holding the lock and exits. A profile made by `profile`
  ([BROWSER-36](#BROWSER-36)) is such a profile: while the extension in one is connected, the
  extension in the other is refused, and it connects within a minute of the first disconnecting.
- A confined server can connect to the socket only while it has egress. A declaration that withholds
  egress, where a future version allows one to, cannot reach the relay on macOS.
- Linux with Landlock ABI 9 was reasoned about and not measured.
- Moving `bravebot-browser` after installing leaves the manifest naming the old path, and Brave
  cannot start the host until it is installed again.
- On Windows `bravebot-browser` only prints that the platform is not supported and exits, so no
  server starts and neither half runs. Whether a named pipe or a Unix socket is the right transport
  there is open.
- A blocked site is matched by the origin written. The same server reached under another name, such
  as a trailing dot, an address or another host name, is another origin and is not covered, and the
  person lists each one they use. The allowed sites are matched in the same way, so a name that is
  not written is not allowed.
- Two options pages open at once each save what they show, and the last save wins. An edit to a list
  that is not saved because the tab was closed before the person left the blocked sites box, or
  chose the button that saves the allowed sites, is lost, and the page said it was not saved yet.
  Writers of the saved switches run one at a time across the options page, the popup and the worker
  through one Web Locks lock, which Brave 1.98.62 has in each of them. A browser without it orders
  the writers of each context only among themselves, so a write from the popup or another page can
  overlap one from the worker, which writes back what it read before the other wrote.
- The listeners that keep the requests of a tab stay registered while the browser holds `webRequest`
  and the hosts, since the browser reports an event only to a listener registered as the service
  worker starts. The worker drops what they report while the switch of the group or of
  `list_requests` is off, and also before it has read the switches. Brave may stop the service
  worker, and a page that loaded before it started or before it read the switches has no requests
  kept. A group whose hour has ended keeps requests until the worker turns it off, within a minute,
  and no tool can read them in the meantime.
- A refusal says that a site is on the blocked list, or for a group limited to the allowed sites
  that it is not on the allowed list, so a session that guesses a URL learns whether its origin is
  on either list. It learns nothing of which tabs are open.
- A URL at an origin that is not listed can redirect to one that is, for example from `http://` to
  `https://` on the same host. `open_tab` closes the tab once it commits there, after the request,
  with the person's cookies, has been made. `open_window` and `restore_closed` close theirs, and
  `navigate_tab`, `reload_tab`, `go_back` and `go_forward` leave their tab at `about:blank`, after the
  request has been made in the same way. The same holds for a redirect to another host: the request
  has been made when the tab is moved off the page.
- `go_back` and `go_forward` fail, and leave the tab at `about:blank`, whenever the page before or
  after is on another host than the tab's, which is common after following a link. The question
  cannot show that page, so no other rule keeps a request from being one the person did not see.
  A person who wants it uses the browser's own button.
- The descriptions a session sees for the tools do not say that results leave out blocked sites or
  that a group is limited to the allowed sites, since changing them changes the tool list a person
  accepted. A session that asks whether a page is
  open can be told no for a page that is.
- While a site is blocked, a history search drops listed entries from the 1,000 newest matches the
  browser returns. A search whose newest 1,000 matches are all at listed sites returns nothing
  although older matches exist.
- Removing the host of one allowed site in Brave's extension settings turns off every group that lists
  it, since a group is held only while every host it lists is. Taking back all sites from a group
  while a group limited to the allowed sites is on can end that group as well, if Brave counted its
  sites as covered when they were allowed and never held them as patterns of their own. That, the
  prompt Brave shows for a host, and what its site access setting does to an optional host were not
  run in a real Brave.
- Off takes back what `permissions.getAll` reports among the optional permissions and every host
  it reports, since the manifest asks for no host at install. Taking back a permission and every
  host in one `permissions.remove` was not run in a real Brave, and `permissions.getAll` was.
- Reloading or updating the extension empties session storage, which turns off every group that was
  turned on until Brave restarts. Whether Brave keeps a permission that an update moves from required
  to optional, as the update to the version with groups does, was not run in a real Brave. Either way
  no tool runs until its group is turned on, and turning a group on asks for only what is not held.
- The tools of `browser-info` were run against a fake that follows the Chrome documentation and not
  in a real Brave. Brave may lack some `chrome.privacy` settings, which the tool then leaves out.
  Whether the browser keeps a `keep_awake` request across a stop of the service worker, and whether it
  releases one when the `power` permission is taken back, was not run in a real Brave. A request is
  released by the worker at the end of its time, by a call with 0 minutes and when the group stops
  being on. Where the person takes `power` back in the browser's settings the extension cannot
  release it, and one the browser keeps then stands until the group is next turned on or the
  extension is unloaded.
- The tools of `remove-data` were run against a fake that follows the Chrome documentation and not in a
  real Brave. The reply of `close_tab` and `close_window` says the browser no longer has the tab or
  window when `tabs.get` or `windows.get` fails for it, which was not run, and a page that asks to stay
  open may or may not be closed by `tabs.remove`. `close_window` closes every tab of the window that
  holds the tab at the URL, at whatever sites they are, and only a tab at a blocked site stops it;
  the question shows the one URL and not the others. `delete_history_range` and
  `clear_browsing_data` also delete the entries and data of sites on the blocked list, since they
  name no site, and no list of sites limits either.
- `remove_bookmark_folder` and `delete_downloaded_file` were run against a fake as well. Whether the
  browser refuses to remove the folders it keeps (the bookmarks bar, other bookmarks and managed
  folders) with the words passed on, and whether `downloads.removeFile` reports a file that is gone
  before the browser noticed it, were not run. The browser refreshes whether a download's file exists
  only from time to time, so the reply of `delete_downloaded_file` may say the file is not gone
  after it was deleted. Two downloads that share an address whose files are both on the disk can be
  told apart only by their filenames.
- The tools of `downloads` were run against a fake that follows the Chrome documentation and not in a
  real Brave. That the browser knows the address a download ended at, after its redirects, when
  `downloads.download` answers, was not run; where it does not, `download_file` takes the address
  it started from for the one it ended at. The request to another host has been made, with the
  person's cookies for that host, when `download_file` cancels, as for `open_tab`. A small file can be
  finished by then, and is deleted. How `pause`, `resume` and `cancel` answer for a download in a
  state they do not apply to was not run, and the tools do not ask the browser to. The file the
  browser saves is checked by the browser's own protections and not by the extension.
- The groups were not all exercised in a real Brave. Through the extension's own tool code, in a headless
  Brave 1.98.62 or 1.98.63 with a scratch profile and local pages: `screenshot_page` of `page-read`,
  `list_page_elements` and, in `input`, `click_element`, `type_text` and `select_option` (the window's Allow
  once was answered through the DevTools protocol and not by a person's click), `capture_screen` and
  `record_gif` of `screen` (with a fake screen and a picker answered by a switch), `save_page_archive` of
  `downloads`, `run_script` of `scripts`, `take_held` of `held` (no menu entry was clicked),
  `get_platform_info` of `platform`, the refusal of a tool of `tabs-read` whose group is off or whose
  permissions are not held, and the tools of `input-trusted`, `capture-trusted`, `console-trusted` and
  `upload-trusted`. No run in a real Brave is recorded for the tools of `tabs-read`, `history-read`,
  `bookmarks-read`, `tabs-manage`, `bookmarks-write`, `navigate`, `cookies`, `network-watch`, `browser-info`
  and `remove-data`, for `read_page`, `read_page_links` and `read_page_forms` of `page-read`, for the
  other tools of `input` and `downloads`, and they were run only against a fake that follows the Chrome
  documentation. For `cookies` the Chrome API was observed with a probe extension and the two tools were
  not run.
- Only `remove-data`, `cookies`, `scripts`, `input`, `input-trusted`, `capture-trusted`, `console-trusted`, `upload-trusted` and `screen` are declared to ask about every call ([BROWSER-21](#BROWSER-21)).
  For the other groups an `allow` rule, or answer 2 given for a tool, covers the later calls to it
  with whatever arguments they carry. `download_file` then fetches a URL the session chose, with the
  person's cookies for its host, and writes the file to the Downloads folder with no question, and
  the tools of `navigate` that load a page, `open_tab` among them, load one the session chose in the
  same way. The allowed sites do not limit `downloads` or `navigate`, since they read no page, so
  only the blocked sites and the same-host rule of [BROWSER-16](#BROWSER-16) hold them back. A
  person who wants each call asked declares the group with `--ask-every-call`.
- A running session keeps the property its server was started with. Declaring a group with
  `--ask-every-call` after a session started its server without it does not make that session ask
  about every call. The next session does ([SERVERS-21](mcp-servers.md#SERVERS-21)).
- The activity log keeps the sites that sessions named, in the browser profile, until 200 newer calls
  replace them. Turning everything off does not clear it and nothing in the options page does.
- The popup is closed by Brave when it loses focus. If that happens while Turn everything off is
  taking permissions back, the groups are already off, since their switches are saved first, and a
  permission can stay held until Turn everything off is used again. The keyboard command runs in the
  service worker and is not affected. The toolbar badge, the popup, the keyboard command and the
  shortcut settings were not run in a real Brave.
- The extension's id is fixed only while it is loaded unpacked. A copy from the Web Store or
  bundled with Brave has an id of its own, and `install` has to be given it.
- The two extensions name each other by id ([BROWSER-37](#BROWSER-37)). A copy of either with
  another id, such as one from the Web Store, is refused by the other until the ids in
  `extension/trusted.js`, `extension-trusted/pair.js` and `extension-trusted/manifest.json` are
  changed together, and until then the tools of `input-trusted`, `capture-trusted`, `console-trusted` and `upload-trusted` say the second extension is not
  installed. The browser words an absent extension, one that is turned off and one that does not
  accept the sender alike, so the sentence names all three and the session cannot tell which.
- `search_web` cannot hold the page to a host, since no API tells the extension the default search
  provider's address. The words have been sent to the provider when the page commits, and a provider
  that redirects to a host of its own, such as a consent page, is followed. If the provider's host is
  at a listed site the tab is closed after the request. Brave's default provider and its behaviour
  when none is set were not run.
- `wait_for_load` looks at the tab every 250 milliseconds, so a tab that finishes in the last
  interval before the time runs out is reported as timed out. A discarded tab never loads and times
  out.
- `list_cookie_names` and `delete_cookie` use the browser's default cookie store. Cookies of a
  private window, and partitioned cookies, which the browser's `getAll` returns only when asked for
  their partition, are not listed or deleted. A cookie set for a parent domain of a listed site is
  shared with it and is listed for the parent's URL, since only a cookie whose domain is the listed
  host is left out. The set of cookies can change between the count and the deletion.
  `delete_cookie` cannot delete one of several cookies of a name sent to a URL, which a person does
  in Brave's own settings.
- A picture of a page holds whatever the visible tab shows. A frame of another origin inside the
  tab is in the picture, and the blocked sites are matched by the tab's own URL
  ([BROWSER-17](#BROWSER-17)), so a blocked site shown in a frame of an allowed or unlisted page is
  not blocked. A page can draw anything in what it shows, so the picture is untrusted whatever the
  tab's URL is ([BROWSER-35](#BROWSER-35)). The browser holds `<all_urls>` while `page-read` or `screen` is on all
  sites, which is wider than the blocked list, and the extension's checks on the URL are what limit
  the call. `<all_urls>` also covers `file:` and other schemes the lists do not name, so the
  extension refuses a URL that is not HTTP or HTTPS. Which of `https://*/*`, `http://*/*` and
  `<all_urls>` the browser takes back when `permissions.remove` is given one of them was not run in
  Brave. A group on all sites that lists the two patterns and `page-read` or `screen`, which list
  `<all_urls>`, are taken back separately, and if the browser removes the patterns that `<all_urls>` contains with
  it, the group that listed them is turned off when the browser reports the removal
  ([BROWSER-19](#BROWSER-19)).
- A saved page is a file on the disk. It stays in the Downloads folder until the person deletes it,
  and any program of theirs can read it, which `remove-data` can do with `delete_downloaded_file`
  only for a download still in the browser's list. It holds whatever the tab held, including a page
  the person is signed in to, and the pages of frames of other origins inside the tab, so a blocked
  site shown in a frame is saved, since the blocked list names the tab's own URL only.
- `pageCapture` lets the browser archive any open tab with no host held and is worded as access to all
  websites, so the install prompt of `downloads` does not narrow with the allowed sites, and the
  limit to them is this extension's own check ([BROWSER-22](#BROWSER-22)). Turning on `downloads`
  asks for that line beside the one for downloads, although only one tool of the group reads a page.
  The group also lists `tabs`, which gives the extension the address and title of every open tab,
  for the tool to find its tab by address. Brave shows no line for it beside the one for all
  websites, so the prompt does not say it.
  The grant of both through Brave's prompt, a page at a site Brave protects from scripts and the
  person's setting to be asked where each file is saved were not run.
- A picture over the browser's quota is refused, not queued past one call: two captures a second is
  the browser's limit, the extension spaces its own at 600 milliseconds, and another extension or
  page that captures in the same second can still make the browser refuse one. Whether a window
  covered by another application's window captures what the person sees was not run; headless Brave
  has no such window.
- `screenshot_full_page` ([BROWSER-43](#BROWSER-43)) gives a picture of what the browser paints for the
  whole page, including the parts out of view. A frame of another site is in it as the page draws
  it, and a site on the blocked list that a page shows in a frame is in it, since the list names the
  tab's own URL only. The group has no off state held by the browser, as `input-trusted` has none.
  The captures were taken in a headless Brave, which has no covered or minimized window, so how a
  headed Brave draws a tab in the background of a window that is hidden by another window is not
  observed. The second picture taken at a smaller scale, for a screen that makes more pixels of a
  CSS pixel than it reports, is tested against a fake debugger and has not been seen in a browser.
- `read_console` ([BROWSER-44](#BROWSER-44)) reads the Console and Log domains, which do not carry
  everything the browser's developer tools show. An error a script throws and nothing catches is an
  entry with the text `Uncaught` at its line and the level `log`, without the error's message or
  its stack, which only `Runtime.exceptionThrown` carries. An argument that is an object is
  `[object Object]`, and the stack of `console.trace` is not in the entry. The session is attached
  to the tab, and a frame that Brave runs in another process has a session of its own that is not
  attached, so what such a frame printed is not read. The Console domain is marked deprecated in the
  protocol, and a Brave that removed it would make every read return the Log entries alone. A
  maintainer can decide to add `Runtime.enable` for the exceptions' messages, which puts a method of
  `Runtime` on the list of [BROWSER-37](#BROWSER-37). The reads were run in a headless Brave, which
  shows no debugging notice, so what a headed Brave shows while the console is read and what its
  Cancel does were not observed.
- The console is the page's, and pages print what they hold: a token, a name, the body of a response.
  The person is asked about the page and not about what its console holds, and the text goes to them
  and to a processor, and to the model only after a vet. The text is not filtered for secrets.
- `choose_file`: the file is looked at when the call is made and read by the browser after the person allows it, which is up
  to a minute later. A file replaced or changed in between is given as it is then, and the size and name the
  window showed are those of the look.
- `choose_file`: a link in `uploads` that leads to another file inside it is followed. A file with more than one hard
  link is refused whichever of its names is given, since its other name can be anywhere, a credential included, and the
  name in `uploads` does not say where. A file that a person or a backup tool hard-linked has to be copied to be
  chosen. The extension looks for a credential name only after the last directory called `uploads`, since it cannot
  tell which directory the relay chose from. A path the relay refuses for a credential name in a directory between two
  of them, such as `uploads/id_rsa/uploads/a.txt`, is a path the extension does not refuse; the relay does not send it.
- `choose_file`: a file input the page hides is found only by the names its markup gives it, so an input that a
  button opens by script and that has no label, `aria-label`, `title`, `name` or `id` the person can name is not
  found, which happens on some sites, and a label that is not in the tree (`display: none`) names nothing. The
  person cannot see an input the page hides, so the window says that the page hides it, and what the person
  can judge is the page, the name and the file. The name of an input that only its id or name names is a
  word for a program. Dropping a file on a zone the page draws is not built.
- `choose_file`: an input that takes several files gets the one, and a call replaces what the input held.
- `choose_file`: a page that sends a file on `change` has it before the person can look at what the page does next. The
  window says so, and the person allows a site by listing it.
- `choose_file`: the server's look at the file system under the confinement was taken from the specification of the
  confinement ([SANDBOX-13](sandboxing.md#SANDBOX-13)) and was not run under the agent's sandbox on either
  platform. Run unconfined, it was exercised with a real Brave on macOS.
- `run_script` ([BROWSER-46](#BROWSER-46)): the code runs in the person's signed-in session and can read everything in the
  page, hidden fields, cookies that are not `HttpOnly` and storage included, click and submit its forms through the page's
  own handlers, and send what it read to another address by a frame or a navigation. Nothing limits it except that the
  person read it. The window says so in its own words.
- `run_script`: the code starts once and what it started stays. A timer, an interval, a listener or an observer it
  set keeps running in the page until the page is closed or reloaded, and each can read and send without a window
  being opened, so the person's yes covers more than the run they read about. The window, the tool's description and
  the options page say so, and the code a person reads is the whole of what those can do.
- `run_script`: the policy holds for the first ten worlds of a document in Brave 1.98.63 and a call that finds it
  missing is refused. A page open for a long time refuses `run_script` after ten calls, a call the person denied
  counting as one, until it is reloaded, and the extension cannot free a world. Where a later Brave applies the policy
  to every world the probe passes and the limit does not show.
- `run_script`: what the world's policy stops is what Brave 1.98.62 and 1.98.63 did on macOS for an unpacked extension, a
  page with no policy of its own and origins on 127.0.0.1. A store-installed extension, a page with a content security
  policy of its own, Windows and Linux were not run. Of the three domains Brave protects from scripts only
  `account.brave.com` was run, served by a local TLS server, and only through `scripting.executeScript`, which
  Brave 1.98.63 rejected with "This site is protected and cannot be scripted."; `userScripts.execute`, which
  this tool calls, and the other two domains were not run.
- `run_script`: the document the code runs in is the one the probe ran in before the window opened. The id comes from
  the answer of `userScripts.execute`, since `webNavigation` names a document otherwise and the group does not list
  it. A page that reloads after the probe is refused by the browser. One that reloads between the lookup of the tab
  and the probe, a few milliseconds, is the page that is shown. The probe runs in the page before the person has
  allowed anything, and holds nothing of the planner's.
- `run_script`: a script that throws, one with a syntax error and one that answers nothing are all `null` in Brave
  1.98.63, so a result of `null` does not say that the code ran to its end. A script that has not answered in 15 seconds
  is left running in the page.
- `run_script`: letters of a right-to-left script, Hebrew or Arabic, reorder inside a line of code in the window and in
  a terminal that follows the bidirectional algorithm, and no control character is there to write out. A line that mixes
  them with other text can read in a different order than the code has, and nothing marks it.
- `run_script`: the planner writes the code without having read the page, since the page never reaches it, so code that
  depends on what a page says needs the person to tell the model what it says, or a result they let through with
  `vet_content`, and the model's code for a page it has not seen can fail on the page.
