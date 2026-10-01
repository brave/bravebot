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
  - crates/browser/src/relay.rs
  - crates/browser/src/server.rs
  - crates/browser/src/tools.rs
  - crates/browser/tests/extension.rs
  - extension/manifest.json
  - extension/background.js
  - extension/tools.js
  - extension/options.js
  - extension/tests/background.test.mjs
  - extension/tests/tools.test.mjs
  - docs/design/browser-extension.md
documented-by: none (gap: a "Connecting Brave" section of docs/website/docs/customize/mcp-servers.md, once bravebot-browser and the extension ship in a release)
---

## Scope

How BraveBot reaches a Brave extension that reads pages and profile data for it: the processes
between the two, who starts each, how they find and trust each other, and what is sent between
them. The design and its reasons are [browser-extension.md](../design/browser-extension.md). What
an MCP server may do once it runs is [mcp.md](mcp.md) and [mcp-servers.md](mcp-servers.md), and
nothing here changes either: what the extension returns reaches a session as any server's result
does.

## What exists today

The relay and the extension are built, on macOS and Linux. The extension is loaded unpacked from
`extension/`, and neither ships in a release yet: `extension/README.md` is how to try the two. Windows
runs neither half: it starts no stdio server, and `bravebot-browser` says the platform is not
supported there.

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

**Why.** An empty result would read as "the page is empty" or "no history matches". Waiting would
hold the turn on something only a person can fix.

`verified-by: bravebot_browser::relay::a_call_with_no_extension_connected_fails_at_once_and_says_why`
`verified-by: bravebot_browser::relay::a_call_to_a_stale_socket_fails_at_once_and_says_why`
`verified-by: bravebot_browser::relay::a_call_the_extension_disconnects_during_fails_and_says_so`
`verified-by: bravebot_browser::relay::a_call_the_extension_does_not_answer_times_out_and_says_so`
`verified-by: bravebot_browser::relay::the_production_reply_timeout_is_thirty_seconds`

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

A message from the host to the extension is at most 1 MB, the limit native messaging sets, and a
request that would exceed it is refused with an error under its own id and not sent. An id that is
itself 1 MB or longer is not kept, and the refusal carries none. A message from the extension may
be up to 64 MB, and a longer one ends the host.

**Why.** Brave closes the port on a message over 1 MB, which disconnects every session at once.
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
A word it does not take, such as `--manifest-dir` with no directory after it, is answered with its
usage, and nothing is written.

**Why.** On macOS Brave reads Chrome's location rather than its own, so a manifest in a Brave
directory there is never read. Chrome reads the same file, and `allowed_origins` is what keeps
Chrome from starting the host for anything but our extension. On Linux the location is under
Brave's own profile directory, and a channel other than stable has its own.

`verified-by: bravebot_browser::relay::installing_writes_one_manifest_for_our_extension_alone`
`verified-by: bravebot_browser::relay::installing_with_no_id_records_the_extension_in_this_repository`
`verified-by: bravebot_browser::relay::installing_refuses_what_is_not_an_extension_id`
`verified-by: bravebot_browser::relay::installing_with_a_flag_missing_its_value_prints_the_usage`
`verified-by: bravebot_browser::install::the_manifest_goes_where_brave_reads_it`
`verified-by: bravebot_browser::install::an_extension_id_is_32_letters_from_a_to_p`

<a id="BROWSER-10"></a>
### BROWSER-10: the tool list is fixed, and a tool not on it is refused

The MCP server offers the same list whether or not the extension is connected:
`get_platform_info`, `list_tabs`, `list_frames`, `read_page`, `search_history` and
`search_bookmarks`. Each calls the extension method of the same name with the tool's arguments. A
call naming any other tool is refused by the server and nothing is sent to the extension.
`list_frames` names a tab by its exact URL, and `read_page` names that tab and, optionally, one
frame by its exact URL. `get_platform_info` returns the operating system and architecture Brave
runs on, and nothing else, so a person can see the extension answer without it telling anything
about them.

**Why.** A person vouches for a server's tool list once and BraveBot records a digest of it. A list
that changed with whether the extension was connected would be put to them again each time it did.
A name off the list reaching the extension would be a tool nobody vouched for. The question before
each call shows its arguments, and a URL is one a person can judge there where a tab id is not.

`verified-by: bravebot_browser::relay::the_tool_list_is_the_same_whether_or_not_the_extension_is_connected`
`verified-by: bravebot_browser::relay::a_tools_arguments_are_the_extension_methods_parameters`
`verified-by: bravebot_browser::relay::a_tool_that_is_not_on_the_list_is_refused_without_asking_the_extension`
`verified-by: bravebot_browser::tools::a_tool_is_found_by_its_exact_name`
`verified-by: bravebot_browser::tools::every_tool_takes_an_object`
`verified-by: bravebot_browser::tools::a_page_is_asked_for_by_its_url_and_optional_frame_url`
`verified-by: by-construction (extension/tests/tools.test.mjs asserts that get_platform_info answers with the operating system and architecture alone, a field the browser adds besides them left out, and reaches no API but the platform's; it runs where the test under BROWSER-13 runs)`

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
the methods the server's tools call, with one switch for each in its options. The `key` in its
manifest fixes its id, and that id is the one `install` records given none. It has the permissions
those methods use, including `webNavigation` to list a tab's frames. Every string in an answer is
well formed: an unpaired surrogate is sent as U+FFFD.

**Why.** The two halves are written in two languages and released together, and nothing at run time
would say they disagreed: a tool the extension does not answer fails like a page that cannot be
read, and an id the host was not installed for is refused like an extension it should not serve.
JSON carries an unpaired surrogate as an escape the host refuses to parse, so an answer holding one
would leave the call waiting out its timeout.

`verified-by: bravebot_browser::extension::the_extension_answers_every_tool_the_server_offers_and_no_other`
`verified-by: bravebot_browser::extension::the_extension_connects_to_the_host_install_names`
`verified-by: bravebot_browser::extension::the_extension_has_each_permission_its_tools_require`
`verified-by: bravebot_browser::extension::the_id_install_records_is_the_one_the_extensions_key_gives`
`verified-by: by-construction (extension/tests/tools.test.mjs asserts that answers from list_tabs, list_frames and read_page holding an unpaired surrogate reach JSON with no surrogate escape and with U+FFFD in its place; it runs where the test under BROWSER-13 runs)`

<a id="BROWSER-13"></a>
### BROWSER-13: a page or frame is read at exactly the URLs asked for

`list_frames` returns only HTTP and HTTPS frame URLs from the open tab whose URL is the one asked
for, character for character, and says which is the top frame. Where no tab is at that URL it fails
without asking for frames. Where the tab leaves that URL before its frames are returned it fails
without returning any frame URL.

`read_page` with no `frame_url` reads that tab's top page as before. With a `frame_url`, it reads
only the one frame at that exact HTTP or HTTPS URL inside the tab at `url`. Where there is no such
frame, or more than one, it fails without running a script. It also fails without returning any
text if the frame leaves `frame_url`, or the outer tab leaves `url`, before the read completes.

A successful read returns at most 100,000 characters of the page's text, never ending it on half a
character, and says whether it cut the page short. A page the browser will not let an extension
read, such as its own settings, is a failure saying so.

**Why.** Both URLs are what the person saw in the question before the call. A `list_frames` result
is untrusted content, so the planner does not read it: the person chooses a URL from the result and
supplies it in a later message. A tab or frame whose URL only resembles one is different content,
which they could have refused. A non-web frame URL such as `about:blank` does not tell them which
content it will expose. Two frames at the same URL cannot be told apart by the argument they
approved.

`verified-by: by-construction (extension/tests/tools.test.mjs uses tabs and frames with distinct ids, URLs and text to assert exact tab and frame selection, each navigation check and refusal, the text bound, and that a URL no tab or unique frame is at reaches no script; make check-extension runs it, and check-ui-build depends on that target, so the Front end CI job runs it on every change the classifier gives the ui area, which a change under extension/ is)`

<a id="BROWSER-14"></a>
### BROWSER-14: only the platform check starts on, and a tool that is off touches nothing

Only the platform check starts on. Listing tabs and their frames, reading a page or frame, and
searching history and bookmarks start off until a person turns them on in the extension's options.
A call to a tool that is off is refused before the browser is asked anything. A history search
covers all of history, and a search returns at most 100 results.

**Why.** Every tool but the platform check reaches what a person has open, has visited or has
saved, so none of them reads anything before the person has said it may. The platform check tells
nothing about them, and is how they see the extension answer before turning anything on. The
browser's history search covers the last day unless it is given a start time, which would answer a
search of all of history with a day of it.

`verified-by: by-construction (extension/tests/tools.test.mjs asserts the defaults, that a tool turned off reaches none of the browser's APIs, the start time and the bound on results; it runs where the test under BROWSER-13 runs)`

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

## Known costs

- The secret in [BROWSER-3](#BROWSER-3) is only as private as the directory. Any process of the
  person's that can read it can use the relay, which is the same account boundary a keychain gives.
- One Brave profile at a time. A second profile with the extension installed starts a second host,
  which finds the first one holding the lock and exits.
- A confined server can connect to the socket only while it has egress. A declaration that withholds
  egress, where a future version allows one to, cannot reach the relay on macOS.
- Linux with Landlock ABI 9 was reasoned about and not measured.
- Moving `bravebot-browser` after installing leaves the manifest naming the old path, and Brave
  cannot start the host until it is installed again.
- Windows starts no stdio server today, so the MCP half cannot run there at all, and whether a named
  pipe or a Unix socket is the right transport is open.
- The extension's id is fixed only while it is loaded unpacked. A copy from the Web Store or
  bundled with Brave has an id of its own, and `install` has to be given it.
