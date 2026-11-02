# Proposal: BraveBot access to the browser through an extension

## Goal

Let BraveBot, running locally, visit pages, read their content, and
query profile data such as tabs, history and bookmarks, from the
user's real profile.

## Why an extension

The extension runs in the user's real profile and can use two kinds of
API:

- **Existing extension APIs** cover most of what BraveBot needs:
  - [`tabs`](https://developer.chrome.com/docs/extensions/reference/api/tabs)
  - [`windows`](https://developer.chrome.com/docs/extensions/reference/api/windows)
  - [`tabGroups`](https://developer.chrome.com/docs/extensions/reference/api/tabGroups)
  - [`scripting`](https://developer.chrome.com/docs/extensions/reference/api/scripting)
  - [`history`](https://developer.chrome.com/docs/extensions/reference/api/history)
  - [`bookmarks`](https://developer.chrome.com/docs/extensions/reference/api/bookmarks)
  - [`readingList`](https://developer.chrome.com/docs/extensions/reference/api/readingList)
  - [`sessions`](https://developer.chrome.com/docs/extensions/reference/api/sessions)

  Full list: https://developer.chrome.com/docs/extensions/reference/api
- **Brave-private extension APIs** fill the gaps. We add them when
  needed, and only our extension's ID can use them.

The first gap is semantic search over history. Standard APIs don't
expose the browser's embedding index, so a private API will run
semantic queries against it. This reuses the browser's own embedder,
so the extension doesn't need to run a model or keep its own index. It
needs a privacy review, because it lets BraveBot search the user's
entire browsing history by meaning.

## Architecture

```
BraveBot ──MCP──▶ bravebot-browser mcp ◀──socket──▶ bravebot-browser ◀──▶ extension
                  (MCP server, started              (native host,     native
                   by BraveBot, confined)            started by       messaging
                                                     Brave, relay)
```

The MCP server and the native host are one program, `bravebot-browser`,
run as two processes. The first argument picks the role: `mcp` when
BraveBot starts it, the extension's origin when Brave does.

- **MCP server:** declares the tools and forwards each call. Started with `--group <id>` it
  declares the tools of one group and answers no others. `install --groups` and `install --preset`
  print one `bravebot mcp add` line for each group, with the alias `brave-<id>`, since BraveBot
  grants, approves and remembers answers for a server by its alias: a person can then accept
  listing tabs without accepting a page read, and a group nobody declared is not offered to a
  session. The extension's own switches still apply to a group that was declared.
- **Native host:** a thin relay started by the browser. It forwards
  messages between the extension and the MCP server over a Unix socket
  or named pipe.
- **Extension:** runs each request with existing or Brave-private APIs
  and returns the result.
- **Second extension:** BraveBot Trusted Input (`extension-trusted/`) holds `debugger`, which Chrome
  does not let an extension ask for later, and does what the first extension cannot do with the
  permissions it asks for later: real mouse and keyboard input, a picture of the whole of a page, a
  page's console and a file chosen on a page's file input. It is not a native messaging client. The
  first extension sends it the calls of four groups with `runtime.sendMessage` to its id, and it
  takes a message from the first extension's id alone
  ([BROWSER-37](../specs/browser.md#BROWSER-37)).

```
BraveBot ──MCP──▶ bravebot-browser mcp ◀──socket──▶ bravebot-browser ◀──▶ extension
                                                                              │ runtime.sendMessage
                                                                              ▼
                                                      the tab ◀──CDP── second extension (debugger)
```

The relay is needed because only the browser can start a native host,
and it controls the host's lifetime and stdin/stdout. BraveBot can't
connect to the extension directly.

Details: https://developer.chrome.com/docs/extensions/develop/concepts/native-messaging

## Why native messaging

- Nothing listens on a port. Only extension IDs listed in the host
  manifest's `allowed_origins` can reach the host.
- The extension takes requests from its native messaging port alone. Its
  manifest has no `externally_connectable`, which a web page needs to
  message an extension, and no script listens for a message from another
  extension ([BROWSER-12](../specs/browser.md#BROWSER-12)). The second extension takes a message from
  the first extension's id and from no other sender
  ([BROWSER-37](../specs/browser.md#BROWSER-37)).
- Rejected alternative: a localhost WebSocket is simpler, but any local
  process or web page can reach it. That's not acceptable when it
  exposes history.

## Why the socket needs a secret

Native messaging keeps other extensions off the host, but the socket
between the host and the MCP server is a file any local process may try
to connect to. Confinement does not prevent it: every stdio server
BraveBot starts has egress, and on macOS egress is what reaches a Unix
socket.

- The host writes a new random secret beside the socket each time it
  starts, readable by the account alone, in a directory only the account
  can enter.
- A peer sends the secret as its first line. A peer that sends anything
  else, or sends it late, is closed before anything it sent is
  forwarded.
- Only a server whose declaration grants that directory can read the
  secret.
- A process of the same account that can read the file can use the
  relay. The extension's per-tool switches are the control for that
  case.

[BROWSER-3](../specs/browser.md#BROWSER-3) has the limits and the tests.

## Capability groups

The tools are in twenty-one groups, and a tool is in one. A group is off until a person turns it on
in the options page, and a tool runs only while its group is on, the group's lifetime has not ended
and the browser holds the permissions the group lists
([BROWSER-18](../specs/browser.md#BROWSER-18), [BROWSER-19](../specs/browser.md#BROWSER-19),
[BROWSER-24](../specs/browser.md#BROWSER-24)). `extension/groups.js` holds the group table the
extension reads and `crates/browser/src/tools.rs` holds the one the relay reads. The tables below
are built from those two, from `extension/lifetimes.js` and from the code of the tools.

`crates/browser/tests/extension.rs` reads this section and holds it to the tree. The groups are
those of `groups.js` in its order, each with the permissions and hosts that file lists, the default
lifetime of `lifetimes.js`, the presets of `groups.js` and the asked-every-call setting of the
relay's table. The tools of each group are those of `groups.js` in its order, so a tool dropped from
the document, moved to another group or added to the server without a row fails it. Each Chrome
method named is called in the code of the extension, and each call in `tools.js`, `requests.js`,
`awake.js` and `held.js` is named.

### The groups

| Group | Permissions | Hosts | Default lifetime | Presets | Asked every call | Tools |
|---|---|---|---|---|---|---|
| `platform` | none | no | until turned off | `off` | no | 1 |
| `tabs-read` | `tabs`, `tabGroups`, `webNavigation`, `sessions` | no | until turned off | `look`, `read`, `browse` | no | 5 |
| `history-read` | `history`, `topSites`, `sessions` | no | until turned off | `look`, `read`, `browse` | no | 4 |
| `bookmarks-read` | `bookmarks`, `readingList` | no | until turned off | `look`, `read`, `browse` | no | 4 |
| `page-read` | `tabs`, `scripting`, `webNavigation` | yes | until turned off | `read`, `browse` | no | 5 |
| `tabs-manage` | `tabs`, `tabGroups` | no | until turned off | `browse` | no | 12 |
| `bookmarks-write` | `bookmarks`, `readingList` | no | until turned off | `browse` | no | 6 |
| `navigate` | `tabs`, `sessions`, `search`, `webNavigation` | no | until turned off | `browse` | no | 9 |
| `downloads` | `downloads`, `pageCapture`, `tabs` | no | until Brave restarts | none | no | 7 |
| `cookies` | `cookies` | yes | until Brave restarts | none | yes | 2 |
| `network-watch` | `webRequest` | yes | until turned off | none | no | 1 |
| `browser-info` | `privacy`, `contentSettings`, `fontSettings`, `system.cpu`, `system.memory`, `system.display`, `system.storage`, `idle`, `power` | no | until turned off | none | no | 7 |
| `remove-data` | `tabs`, `history`, `bookmarks`, `readingList`, `browsingData`, `downloads` | no | until Brave restarts | none | yes | 9 |
| `scripts` | `userScripts` | yes | until Brave restarts | none | yes | 1 |
| `input` | `tabs`, `scripting` | yes | until Brave restarts | none | yes | 6 |
| `input-trusted` | none | no | until Brave restarts | none | yes | 5 |
| `capture-trusted` | none | no | until Brave restarts | none | yes | 1 |
| `console-trusted` | none | no | until Brave restarts | none | yes | 1 |
| `upload-trusted` | none | no | until Brave restarts | none | yes | 1 |
| `screen` | `desktopCapture`, `tabs` | yes | until Brave restarts | none | yes | 2 |
| `held` | `contextMenus` | no | until Brave restarts | none | no | 1 |

- Permissions are the optional permissions the group's tools call. A group lists only what its tools
  call, so a group with no tool lists none. The manifest requires `nativeMessaging`, `storage` and
  `alarms` and every permission here is optional
  ([BROWSER-18](../specs/browser.md#BROWSER-18)).
- Hosts is yes for a group whose tools read pages. It needs the host of each allowed site, or
  `https://*/*` and `http://*/*` where the person chose all sites for it
  ([BROWSER-22](../specs/browser.md#BROWSER-22)). `page-read` needs `<all_urls>` instead of the two
  patterns on all sites, because `screenshot_page` calls `captureVisibleTab`, which the browser lets
  an extension call only with it ([BROWSER-35](../specs/browser.md#BROWSER-35)).
- `scripts` lists `userScripts` for `run_script` and reads the hosts of the allowed sites, since the
  browser runs a user script only in a page the extension holds the host of. Its line carries
  `allowedOnly`, so it has no choice of all sites and never needs `https://*/*`, `http://*/*` or
  `<all_urls>` ([BROWSER-46](../specs/browser.md#BROWSER-46)).
- `downloads` lists `pageCapture` and `tabs` for `save_page_archive`
  ([BROWSER-40](../specs/browser.md#BROWSER-40)). Brave words `pageCapture` as "Read and change all
  your data on all websites" whatever hosts are asked for with it, and the browser lets the
  extension archive any open tab once it holds it. The tool finds the tab by its address, and Brave
  leaves the address out of every tab for an extension that holds neither `tabs` nor a host that
  covers the tab, so the group, which holds no host, lists `tabs`. Brave shows no line for `tabs`
  beside the line for `pageCapture`, so the prompt of the group is the same with it. The group has
  no hosts and no scope, so the tool is named in `ALLOWED_ONLY_TOOLS` of `groups.js` and `handle`
  holds it to the allowed sites whatever the group, and the blocked list applies as well. The Read
  and Browse presets hold `page-read` and not `downloads`, so they ask for neither `pageCapture`
  nor `downloads`, and `page-read` already lists `tabs`, so their request is the same
  ([Saving a page as a file](#saving-a-page-as-a-file)).
- Default lifetime is how long the group stays on when the person turns it on and chooses no
  other time ([BROWSER-24](../specs/browser.md#BROWSER-24)). `platform` is on in every install and
  cannot be turned off, so its lifetime does not end.
- Presets are the presets that turn the group on ([BROWSER-20](../specs/browser.md#BROWSER-20)). A
  group in none is turned on by itself.
- Asked every call is whether `bravebot-browser install` declares the group's server with
  `--ask-every-call`, so that each call is put to the person and no answer is kept
  ([BROWSER-21](../specs/browser.md#BROWSER-21)).
- `scripts` is built ([BROWSER-46](../specs/browser.md#BROWSER-46)) and is in no preset. Its one tool,
  `run_script`, runs code the planner wrote in a page after the person has read all of it. It lists
  `userScripts`, reads the hosts of the allowed sites and nothing wider, `install` declares it with
  `--ask-every-call`, and its server waits and is bounded as `input`'s does
  ([Is code the planner wrote a routing field?](#is-code-the-planner-wrote-a-routing-field-scripts-decided)).
- `screen` is built ([BROWSER-42](../specs/browser.md#BROWSER-42)) and is in no preset. Its two tools,
  `capture_screen` and `record_gif`, take a picture of what the person chooses in the browser's
  picker and record a page as a GIF. It lists `desktopCapture` and `tabs`, `install` declares it with
  `--ask-every-call`, and its server waits and is bounded as `input`'s does
  ([Taking a picture of the screen](#taking-a-picture-of-the-screen)).
- `input` is built ([BROWSER-33](../specs/browser.md#BROWSER-33)) and is in no preset. Its server
  offers the six tools, `install` declares it with `--ask-every-call`, and each action that acts on
  an element is also answered in a window of the browser
  ([BROWSER-34](../specs/browser.md#BROWSER-34)). The person has 60 seconds there, so the server
  waits 90 seconds for the extension's reply, where any other waits 30
  ([BROWSER-6](../specs/browser.md#BROWSER-6)), and the declaration also has `--tool-timeout 120`,
  which bounds a call above the server's wait. `bravebot mcp add` takes `--tool-timeout` in
  seconds from 1 to 3600 and the agent's default is 120
  ([SERVERS-15](../specs/mcp-servers.md#SERVERS-15)).
- `input-trusted` is built ([BROWSER-38](../specs/browser.md#BROWSER-38)) and is in no preset. Its five
  tools are done by a second extension, BraveBot Trusted Input in `extension-trusted/`, which holds
  `debugger` ([BROWSER-37](../specs/browser.md#BROWSER-37)). This extension lists no permission for the
  group and reads no host for it, checks the same switches, lifetime, blocked sites and allowed sites as
  for any group, and then sends the call to the other extension by its id
  ([BROWSER-12](../specs/browser.md#BROWSER-12)). The group is limited to the allowed sites whatever the
  scope, so it has no choice of scope (`allowedOnly` in `groups.js`,
  [BROWSER-18](../specs/browser.md#BROWSER-18)). The person answers in a window of the other extension
  ([BROWSER-38](../specs/browser.md#BROWSER-38)), so its server waits and is bounded as `input`'s does:
  the person has 60 seconds, this extension waits 80 seconds for the other's reply, the server waits
  90 seconds and the declaration bounds a call at 120 seconds.
- `capture-trusted` is built ([BROWSER-43](../specs/browser.md#BROWSER-43)) and is in no preset. Its one
  tool, `screenshot_full_page`, is done by the same second extension, and the group is shaped as
  `input-trusted` is: no permission, no host, limited to the allowed sites whatever the scope, until
  Brave restarts by default, asked every call, and the same four waits. It is a group of its own and
  not a sixth tool of `input-trusted` because it sends no input and reads the whole of a page,
  including the parts nobody sees, so the options page can say that in the group's own words and
  turning the group on says nothing about clicks. It is not in `screen`, which holds
  `desktopCapture` for the picture of a screen ([BROWSER-42](../specs/browser.md#BROWSER-42)).
- `console-trusted` is built ([BROWSER-44](../specs/browser.md#BROWSER-44)) and is in no preset. Its one
  tool, `read_console`, is done by the second extension as the tools of `input-trusted` are, and the
  group is shaped as that one is: it lists no permission, reads no host, is limited to the allowed sites
  whatever the scope, stays on until Brave restarts by default, and has a server that waits and is
  bounded as `input-trusted`'s does. It is a group of its own because it sends no input and reads
  what a page's scripts printed, so turning it on is described as that
  ([Reading a page's console](#reading-a-pages-console)).
- `held` is built ([BROWSER-39](../specs/browser.md#BROWSER-39)) and is in no preset. It reads no page
  and lists no host. Its one tool returns what the person held out with the extension's entries in
  the browser's context menu, and forgets it ([What a person holds out](#what-a-person-holds-out)).
  Its server is asked about as any other is, and not about every call.

### The Chrome methods behind each tool

Each row names the methods of the Chrome extension API a tool calls, as `namespace.method`. That
includes the calls that find the tab, window, bookmark or download the tool's URL, title or name
stands for. Some methods are shared, and their rows are not repeated for each tool.

| Used by | Chrome methods |
|---|---|
| every tool but the platform check, before it runs | `storage.local.get`, `storage.session.get`, `permissions.contains` |
| every call, once it has been answered | `storage.local.set` |
| a tool that opens a page or moves one, the commit events | `webNavigation.onCommitted`, `webNavigation.onHistoryStateUpdated`, `webNavigation.onReferenceFragmentUpdated`, `webNavigation.onErrorOccurred` |

The first row is the reads of the saved switches, blocked sites, lifetime, allowed sites and
scopes, and the question to the browser whether it holds what the group lists. The second is the
line added to the activity log ([BROWSER-27](../specs/browser.md#BROWSER-27)). A tool that opens a
page or moves one watches the commit events for the top frame of its tab, to see where the page
committed or that it failed, and its row says commit events.

| Group | Tool | Chrome methods |
|---|---|---|
| `platform` | `get_platform_info` | `runtime.getPlatformInfo` |
| `tabs-read` | `list_tabs` | `tabs.query` |
| `tabs-read` | `list_frames` | `tabs.query`, `webNavigation.getAllFrames`, `tabs.get` |
| `tabs-read` | `list_windows` | `windows.getAll` |
| `tabs-read` | `list_tab_groups` | `tabGroups.query` |
| `tabs-read` | `list_recently_closed` | `sessions.getRecentlyClosed` |
| `history-read` | `search_history` | `history.search` |
| `history-read` | `get_visits` | `history.getVisits` |
| `history-read` | `list_top_sites` | `topSites.get` |
| `history-read` | `list_synced_tabs` | `sessions.getDevices` |
| `bookmarks-read` | `search_bookmarks` | `bookmarks.search` |
| `bookmarks-read` | `list_bookmark_folder` | `bookmarks.search` and `bookmarks.getTree` for a folder, `bookmarks.getChildren` |
| `bookmarks-read` | `list_recent_bookmarks` | `bookmarks.getRecent` |
| `bookmarks-read` | `list_reading_list` | `readingList.query` |
| `page-read` | `read_page` | `tabs.query`, `scripting.executeScript`, and with a `frame_url` `webNavigation.getAllFrames`, `tabs.get` |
| `page-read` | `read_page_links` | `tabs.query`, `scripting.executeScript`, and in the page `dom.openOrClosedShadowRoot` |
| `page-read` | `read_page_forms` | `tabs.query`, `scripting.executeScript`, and in the page `dom.openOrClosedShadowRoot` |
| `page-read` | `list_page_elements` | `tabs.query`, `scripting.executeScript`, `storage.session.set`, and in the page `dom.openOrClosedShadowRoot` |
| `page-read` | `screenshot_page` | `tabs.query`, `tabs.get`, `windows.get`, `tabs.captureVisibleTab`, a call that returns a promise from Chrome 88 |
| `tabs-manage` | `focus_tab` | `tabs.query`, `tabs.update`, `windows.update` |
| `tabs-manage` | `pin_tab` | `tabs.query`, `tabs.update` |
| `tabs-manage` | `mute_tab` | `tabs.query`, `tabs.update` |
| `tabs-manage` | `move_tab` | `tabs.query`, `tabs.move` |
| `tabs-manage` | `group_tabs` | `tabs.query`, `tabs.group`, `tabGroups.update`, `tabGroups.get` |
| `tabs-manage` | `ungroup_tabs` | `tabs.query`, `tabs.ungroup`, `tabs.get` |
| `tabs-manage` | `update_tab_group` | `tabs.query`, `tabGroups.update`, `tabGroups.get` |
| `tabs-manage` | `discard_tab` | `tabs.query`, `tabs.discard` |
| `tabs-manage` | `duplicate_tab` | `tabs.query`, `tabs.duplicate` |
| `tabs-manage` | `set_zoom` | `tabs.query`, `tabs.setZoom`, `tabs.getZoom` |
| `tabs-manage` | `focus_window` | `tabs.query`, `windows.update` |
| `tabs-manage` | `resize_window` | `tabs.query`, `windows.update` |
| `bookmarks-write` | `create_bookmark` | `bookmarks.search` and `bookmarks.getTree` for a folder, `bookmarks.create`, `bookmarks.get` |
| `bookmarks-write` | `create_bookmark_folder` | `bookmarks.search` and `bookmarks.getTree` for the parent, `bookmarks.create`, `bookmarks.get` |
| `bookmarks-write` | `move_bookmark` | `bookmarks.search`, `bookmarks.getTree` for the folder, `bookmarks.get`, `bookmarks.move` |
| `bookmarks-write` | `rename_bookmark` | `bookmarks.search`, `bookmarks.update` |
| `bookmarks-write` | `add_reading_list_entry` | `readingList.query`, `readingList.addEntry` |
| `bookmarks-write` | `update_reading_list_entry` | `readingList.query`, `readingList.updateEntry` |
| `navigate` | `open_tab` | `tabs.create`, `tabs.remove`, the commit events |
| `navigate` | `navigate_tab` | `tabs.query`, `tabs.update`, the commit events |
| `navigate` | `reload_tab` | `tabs.query`, `tabs.reload`, `tabs.update`, the commit events |
| `navigate` | `go_back` | `tabs.query`, `tabs.goBack`, `tabs.update`, the commit events |
| `navigate` | `go_forward` | `tabs.query`, `tabs.goForward`, `tabs.update`, the commit events |
| `navigate` | `open_window` | `windows.create`, `windows.remove`, the commit events |
| `navigate` | `restore_closed` | `sessions.getRecentlyClosed`, `sessions.restore`, `tabs.remove`, the commit events |
| `navigate` | `search_web` | `tabs.create`, `search.query`, `tabs.remove`, the commit events |
| `navigate` | `wait_for_load` | `tabs.query`, `tabs.get` |
| `downloads` | `save_page_archive` | `tabs.query`, `pageCapture.saveAsMHTML`, `tabs.get`, `downloads.download`, `downloads.search`; the promise form of `saveAsMHTML` is from Chrome 116 |
| `downloads` | `download_file` | `downloads.download`, `downloads.search`, `downloads.cancel`, `downloads.removeFile` |
| `downloads` | `list_downloads` | `downloads.search` |
| `downloads` | `pause_download` | `downloads.search`, `downloads.pause` |
| `downloads` | `resume_download` | `downloads.search`, `downloads.resume` |
| `downloads` | `cancel_download` | `downloads.search`, `downloads.cancel` |
| `downloads` | `show_download` | `downloads.search`, `downloads.show` |
| `cookies` | `list_cookie_names` | `cookies.getAll` |
| `cookies` | `delete_cookie` | `cookies.getAll`, `cookies.remove` |
| `network-watch` | `list_requests` | `tabs.query`, `tabs.get`, and from the start of the worker `webRequest.onBeforeRequest`, `webRequest.onBeforeRedirect`, `webRequest.onCompleted`, `webRequest.onErrorOccurred`, `tabs.onRemoved` |
| `browser-info` | `get_privacy_settings` | `privacy.network.*.get`, `privacy.services.*.get`, `privacy.websites.*.get` |
| `browser-info` | `get_content_settings` | `contentSettings.*.get` for ten kinds of content |
| `browser-info` | `get_font_settings` | `fontSettings.getFont`, `fontSettings.getDefaultFontSize`, `fontSettings.getDefaultFixedFontSize`, `fontSettings.getMinimumFontSize` |
| `browser-info` | `get_system_info` | `system.cpu.getInfo`, `system.memory.getInfo`, `system.display.getInfo`, `system.storage.getInfo` |
| `browser-info` | `get_idle_state` | `idle.queryState` |
| `browser-info` | `get_access_flags` | `extension.isAllowedFileSchemeAccess`, `extension.isAllowedIncognitoAccess` |
| `browser-info` | `keep_awake` | `power.requestKeepAwake`, `power.releaseKeepAwake`, `alarms.create`, `alarms.clear`, `storage.session.set`, `storage.session.remove` |
| `remove-data` | `close_tab` | `tabs.query`, `tabs.remove`, `tabs.get` |
| `remove-data` | `close_window` | `tabs.query`, `windows.remove`, `windows.get` |
| `remove-data` | `delete_history_entry` | `history.getVisits`, `history.deleteUrl` |
| `remove-data` | `delete_history_range` | `history.deleteRange` |
| `remove-data` | `clear_browsing_data` | `browsingData.remove` |
| `remove-data` | `remove_bookmark` | `bookmarks.search`, `bookmarks.remove` |
| `remove-data` | `remove_bookmark_folder` | `bookmarks.search`, `bookmarks.getChildren`, `bookmarks.remove` |
| `remove-data` | `remove_reading_list_entry` | `readingList.query`, `readingList.removeEntry` |
| `remove-data` | `delete_downloaded_file` | `downloads.search`, `downloads.removeFile` |
| `scripts` | `run_script` | `tabs.query`, `tabs.get`, `userScripts.getWorldConfigurations`, `userScripts.configureWorld`, `userScripts.execute`, `userScripts.resetWorldConfiguration` |
| `input` | `click_element` | `tabs.query`, `scripting.executeScript`, and in the page `dom.openOrClosedShadowRoot` |
| `input` | `type_text` | `tabs.query`, `scripting.executeScript`, and in the page `dom.openOrClosedShadowRoot` |
| `input` | `select_option` | `tabs.query`, `scripting.executeScript`, and in the page `dom.openOrClosedShadowRoot` |
| `input` | `press_key` | `tabs.query`, `scripting.executeScript`, and in the page `dom.openOrClosedShadowRoot` |
| `input` | `scroll_page` | `tabs.query`, `scripting.executeScript` |
| `input` | `wait_for_element` | `tabs.query`, `scripting.executeScript`, and in the page `dom.openOrClosedShadowRoot` |
| `input-trusted` | `click_element_trusted` | `runtime.sendMessage` |
| `input-trusted` | `type_text_trusted` | `runtime.sendMessage` |
| `input-trusted` | `press_key_trusted` | `runtime.sendMessage` |
| `input-trusted` | `hover_element` | `runtime.sendMessage` |
| `input-trusted` | `drag_element` | `runtime.sendMessage` |
| `capture-trusted` | `screenshot_full_page` | `runtime.sendMessage` |
| `console-trusted` | `read_console` | `runtime.sendMessage` |
| `upload-trusted` | `choose_file` | `runtime.sendMessage` |
| `screen` | `capture_screen` | `windows.create`, `windows.remove`, `windows.onRemoved`, `runtime.getURL`, `runtime.id`, `runtime.onMessage`, and in the capture window `desktopCapture.chooseDesktopMedia`, `desktopCapture.cancelChooseDesktopMedia`, `runtime.lastError` and `runtime.sendMessage` |
| `screen` | `record_gif` | `tabs.query`, `tabs.get`, `windows.get`, `tabs.captureVisibleTab`, and in the worker `createImageBitmap` and `OffscreenCanvas`, which need no permission |
| `held` | `take_held` | `storage.session.get`, `storage.session.set`, `storage.session.remove`, `contextMenus.removeAll`, `contextMenus.create`, `contextMenus.onClicked` |

A few rows need a word more than the table gives:

- `get_privacy_settings` calls `get` on 19 settings of the three areas, never `set` or `clear`, and
  `get_content_settings` calls `get` on ten kinds of content with the one URL it was given. The
  settings and kinds are named in `tools.js`, and each is read by a call of its own
  ([BROWSER-10](../specs/browser.md#BROWSER-10)).
- `keep_awake` makes an alarm for the end of the request. It and the worker release the request with
  `power.releaseKeepAwake` and forget it in session storage
  ([BROWSER-26](../specs/browser.md#BROWSER-26)).
- `take_held` reads the items from `storage.session` and empties them. The other calls of its row are
  made by `held.js` outside the tool: `storage.session.set` keeps an item when the person uses an
  entry, `contextMenus.create` and `contextMenus.removeAll` make and remove the entries while the
  group is on, and `contextMenus.onClicked` is how the extension hears that the person used one
  ([BROWSER-39](../specs/browser.md#BROWSER-39)).
- `close_window` closes every tab of the window that holds the named tab, so it reads the window's
  tabs with `tabs.query` first and does not close a window with a tab at a blocked site
  ([BROWSER-31](../specs/browser.md#BROWSER-31)).
- `remove_bookmark_folder` calls `bookmarks.remove` for an empty folder and never
  `bookmarks.removeTree`, so `bookmarks.getChildren` counts what the folder holds first.
- `tools.js`, `requests.js`, `awake.js`, `held.js`, `screen.js`, `capture.js` and `scripts.js` call no
  method that is not named above. No tool sets a cookie's value or opens or runs a downloaded file
  ([BROWSER-30](../specs/browser.md#BROWSER-30), [BROWSER-32](../specs/browser.md#BROWSER-32)).
  `capture_screen` takes a picture of what the person chooses in the browser's picker
  ([BROWSER-42](../specs/browser.md#BROWSER-42)), and `run_script` runs the code the planner wrote in
  one page after the person has read all of it
  ([BROWSER-46](../specs/browser.md#BROWSER-46)).

### Tool surface

A tool is held to four rules before it is built. `crates/browser/tests/tool_surface.rs` holds the
relay's table to the name, the arguments and the shape of a listing, and
`crates/browser/tests/extension.rs` holds the argument table and the routing table to the relay's
table, so a tool that breaks a rule, or is added without its rows, fails a test.

1. **Name.** A tool is named in lower-case words joined by underscores, a verb and what it acts on,
   such as `list_tabs` and `delete_cookie`.
2. **Arguments.** The question before a call shows the tool as `alias:tool` and the arguments as the
   planner wrote them ([SERVERS-7](../specs/mcp-servers.md#SERVERS-7)). Each argument is one a person
   can judge there: an address, a name or a short text, a number or a time they can read, one of a
   short list of words, or yes or no. A page, tab, window, bookmark, folder or download is named by
   its address, title or path, and never by an id the browser gave it. A tab id, a node id or a
   selector read from a page is the argument of no tool, because it tells the person nothing about
   which page it is or where an action lands ([BROWSER-13](../specs/browser.md#BROWSER-13)). The
   table below lists every argument any tool takes and what the question shows for it.
3. **Result.** A result is untrusted and private whichever tool made it
   ([MCP-1](../specs/mcp.md#MCP-1)): the label is put on it by BraveBot and not by anything the
   server says. The planner never reads it. The person reads it
   ([VIEW-3](../specs/terminal-transcript.md#VIEW-3)) and supplies what the planner needs in a later
   message, as with `list_frames` and `read_page`. A tool is listed with a name, a description and an
   input schema, and declares no shape for its result.
4. **Routing field.** The routing field of a call is the alias and the tool, `brave-<group>:<tool>`
   for a server started with `--group` ([BROWSER-21](../specs/browser.md#BROWSER-21)), and never an
   argument ([ROUTE-7](../specs/routing.md#ROUTE-7),
   [SERVERS-7](../specs/mcp-servers.md#SERVERS-7)). The relay's table fixes it, so no page content
   decides it. A person can approve that field alone: the alias names the group, whose permissions,
   hosts and lifetime are in [the table of the groups](#the-groups), and the tool names what is done
   with them, while the arguments are shown in the question and no rule matches them. The seven tools of
   a server declared with no group have the alias `brave`. A tool that clicks or types is built only
   as the `input` tools are: the page is named by its exact URL, the element by a role and an
   accessible name the planner got from the person or from content a person let through with
   `vet_content`, and the person is shown the element the name led to in the browser before anything
   is done ([ROUTE-9](../specs/routing.md#ROUTE-9), [BROWSER-33](../specs/browser.md#BROWSER-33)). A tool
   that took a selector, a node id or a coordinate read from a page would take its routing from page
   content and is not built.

| Argument | What the question shows |
|---|---|
| `url` | An address, written out in full, that names the page, tab, bookmark, reading list entry, history entry, cookie site or download the tool acts on, or the page it opens. |
| `urls` | A list of addresses, one for each tab to group or ungroup, at most 50. |
| `frame_url` | The address of one frame of the page, which the person chose from the result of `list_frames`. |
| `to` | The address `navigate_tab` takes the tab to. |
| `title` | The title of a tab group, bookmark, bookmark folder or reading list entry, as text. |
| `folder`, `parent` | The exact title of a bookmark folder: the one to list or delete, or to put a bookmark in or move it to (`folder`), or to put a new folder in (`parent`). |
| `query` | The words to search for. |
| `name` | The name of a cookie, or the accessible name of the element an `input` or `input-trusted` tool acts on, or of the file input `choose_file` chooses a file on, as the page labels it. |
| `path` | The name of a file in the uploads directory, or its absolute path inside it, at most 900 characters, as `choose_file` is given it. The question shows the planner's text, and the window shows the path the file leads to with its name and size ([Choosing a file](#choosing-a-file)). |
| `role` | One of ten roles an element can be asked for by: button, link, textbox, checkbox, radio, combobox, menuitem, tab, option, switch. |
| `onto_role`, `onto_name` | The role and the accessible name of the element `drag_element` drops the first on, with the same lists and limits as `role` and `name`. |
| `element_number` | A whole number from 1 to 40, given in place of `role` and `name`: the number of a line of the list `list_page_elements` made for that URL, which the person read in the transcript beside the role and name of the element on that line. It leads to the element that line was made for, and the window shows that element. |
| `text` | The text `type_text` puts in a field, written out in full, at most 2,000 characters. |
| `code` | The code `run_script` runs, at most 2,000 characters. The question draws it one numbered row to a line, under a row that counts the lines, with each character that cannot be seen written out as `‹U+XXXX›` and counted ([PROMPT-13](../specs/prompting.md#PROMPT-13)). A window of the extension shows it again, with the exact address, before anything runs ([BROWSER-46](../specs/browser.md#BROWSER-46)). |
| `option` | The text of the option `select_option` chooses, at most 200 characters. |
| `key` | One key from a fixed list, alone: Enter, Escape, Tab, an arrow, Home, End, Page Up or Page Down. |
| `button`, `count` | The mouse button, left or right, and whether a click is made once or twice. |
| `direction`, `pages` | Up, down, top or bottom, and how many screens, from 1 to 5. |
| `filename` | The path the browser holds for a download, as `list_downloads` shows it. |
| `max_results` | A count. |
| `max_entries` | A count, from 1 to 200: how many of the newest console entries `read_console` returns. |
| `levels` | One of three words: `all`, `warnings_and_errors` or `errors`. Which entries of the console `read_console` keeps. |
| `index` | A position in a window, counting from 0, or -1 for last. |
| `factor` | A zoom factor, where 1 is 100%. |
| `frames` | How many pictures a `record_gif` recording takes, from 2 to 20, 10 where the call does not say. |
| `width`, `height` | A size in pixels. |
| `minutes`, `seconds` | A time in minutes or in seconds. |
| `since`, `start`, `end` | A moment, as a date and time with its offset. |
| `types` | A list of kinds of browsing data, each from a fixed list, at most 50. |
| `color` | One of the colors Brave gives a tab group. |
| `level` | `system` or `display`. |
| `muted`, `pinned`, `collapsed`, `read` | Yes or no. |

### The routing field and arguments of each tool

Each row gives the routing field of a tool, in the order of the table of the groups, and the
arguments it takes in alphabetical order. The routing field is that of the server started for the tool's group.
The server declared with no group offers `get_platform_info`, `list_tabs`, `list_frames`,
`read_page`, `search_history`, `search_bookmarks` and `open_tab`, and the field of each there is
`brave:<tool>`.

| Group | Tool | Routing field | Arguments |
|---|---|---|---|
| `platform` | `get_platform_info` | `brave-platform:get_platform_info` | none |
| `tabs-read` | `list_tabs` | `brave-tabs-read:list_tabs` | none |
| `tabs-read` | `list_frames` | `brave-tabs-read:list_frames` | `url` |
| `tabs-read` | `list_windows` | `brave-tabs-read:list_windows` | none |
| `tabs-read` | `list_tab_groups` | `brave-tabs-read:list_tab_groups` | none |
| `tabs-read` | `list_recently_closed` | `brave-tabs-read:list_recently_closed` | none |
| `history-read` | `search_history` | `brave-history-read:search_history` | `max_results`, `query` |
| `history-read` | `get_visits` | `brave-history-read:get_visits` | `max_results`, `url` |
| `history-read` | `list_top_sites` | `brave-history-read:list_top_sites` | none |
| `history-read` | `list_synced_tabs` | `brave-history-read:list_synced_tabs` | none |
| `bookmarks-read` | `search_bookmarks` | `brave-bookmarks-read:search_bookmarks` | `query` |
| `bookmarks-read` | `list_bookmark_folder` | `brave-bookmarks-read:list_bookmark_folder` | `folder`, `max_results` |
| `bookmarks-read` | `list_recent_bookmarks` | `brave-bookmarks-read:list_recent_bookmarks` | `max_results` |
| `bookmarks-read` | `list_reading_list` | `brave-bookmarks-read:list_reading_list` | `max_results` |
| `page-read` | `read_page` | `brave-page-read:read_page` | `frame_url`, `url` |
| `page-read` | `read_page_links` | `brave-page-read:read_page_links` | `url` |
| `page-read` | `read_page_forms` | `brave-page-read:read_page_forms` | `url` |
| `page-read` | `list_page_elements` | `brave-page-read:list_page_elements` | `url` |
| `page-read` | `screenshot_page` | `brave-page-read:screenshot_page` | `url` |
| `tabs-manage` | `focus_tab` | `brave-tabs-manage:focus_tab` | `url` |
| `tabs-manage` | `pin_tab` | `brave-tabs-manage:pin_tab` | `pinned`, `url` |
| `tabs-manage` | `mute_tab` | `brave-tabs-manage:mute_tab` | `muted`, `url` |
| `tabs-manage` | `move_tab` | `brave-tabs-manage:move_tab` | `index`, `url` |
| `tabs-manage` | `group_tabs` | `brave-tabs-manage:group_tabs` | `title`, `urls` |
| `tabs-manage` | `ungroup_tabs` | `brave-tabs-manage:ungroup_tabs` | `urls` |
| `tabs-manage` | `update_tab_group` | `brave-tabs-manage:update_tab_group` | `collapsed`, `color`, `title`, `url` |
| `tabs-manage` | `discard_tab` | `brave-tabs-manage:discard_tab` | `url` |
| `tabs-manage` | `duplicate_tab` | `brave-tabs-manage:duplicate_tab` | `url` |
| `tabs-manage` | `set_zoom` | `brave-tabs-manage:set_zoom` | `factor`, `url` |
| `tabs-manage` | `focus_window` | `brave-tabs-manage:focus_window` | `url` |
| `tabs-manage` | `resize_window` | `brave-tabs-manage:resize_window` | `height`, `url`, `width` |
| `bookmarks-write` | `create_bookmark` | `brave-bookmarks-write:create_bookmark` | `folder`, `title`, `url` |
| `bookmarks-write` | `create_bookmark_folder` | `brave-bookmarks-write:create_bookmark_folder` | `parent`, `title` |
| `bookmarks-write` | `move_bookmark` | `brave-bookmarks-write:move_bookmark` | `folder`, `url` |
| `bookmarks-write` | `rename_bookmark` | `brave-bookmarks-write:rename_bookmark` | `title`, `url` |
| `bookmarks-write` | `add_reading_list_entry` | `brave-bookmarks-write:add_reading_list_entry` | `read`, `title`, `url` |
| `bookmarks-write` | `update_reading_list_entry` | `brave-bookmarks-write:update_reading_list_entry` | `read`, `title`, `url` |
| `navigate` | `open_tab` | `brave-navigate:open_tab` | `url` |
| `navigate` | `navigate_tab` | `brave-navigate:navigate_tab` | `to`, `url` |
| `navigate` | `reload_tab` | `brave-navigate:reload_tab` | `url` |
| `navigate` | `go_back` | `brave-navigate:go_back` | `url` |
| `navigate` | `go_forward` | `brave-navigate:go_forward` | `url` |
| `navigate` | `open_window` | `brave-navigate:open_window` | `url` |
| `navigate` | `restore_closed` | `brave-navigate:restore_closed` | `url` |
| `navigate` | `search_web` | `brave-navigate:search_web` | `query` |
| `navigate` | `wait_for_load` | `brave-navigate:wait_for_load` | `url` |
| `downloads` | `save_page_archive` | `brave-downloads:save_page_archive` | `url` |
| `downloads` | `download_file` | `brave-downloads:download_file` | `url` |
| `downloads` | `list_downloads` | `brave-downloads:list_downloads` | `max_results` |
| `downloads` | `pause_download` | `brave-downloads:pause_download` | `filename`, `url` |
| `downloads` | `resume_download` | `brave-downloads:resume_download` | `filename`, `url` |
| `downloads` | `cancel_download` | `brave-downloads:cancel_download` | `filename`, `url` |
| `downloads` | `show_download` | `brave-downloads:show_download` | `filename`, `url` |
| `cookies` | `list_cookie_names` | `brave-cookies:list_cookie_names` | `url` |
| `cookies` | `delete_cookie` | `brave-cookies:delete_cookie` | `name`, `url` |
| `network-watch` | `list_requests` | `brave-network-watch:list_requests` | `url` |
| `browser-info` | `get_privacy_settings` | `brave-browser-info:get_privacy_settings` | none |
| `browser-info` | `get_content_settings` | `brave-browser-info:get_content_settings` | `url` |
| `browser-info` | `get_font_settings` | `brave-browser-info:get_font_settings` | none |
| `browser-info` | `get_system_info` | `brave-browser-info:get_system_info` | none |
| `browser-info` | `get_idle_state` | `brave-browser-info:get_idle_state` | `seconds` |
| `browser-info` | `get_access_flags` | `brave-browser-info:get_access_flags` | none |
| `browser-info` | `keep_awake` | `brave-browser-info:keep_awake` | `level`, `minutes` |
| `remove-data` | `close_tab` | `brave-remove-data:close_tab` | `url` |
| `remove-data` | `close_window` | `brave-remove-data:close_window` | `url` |
| `remove-data` | `delete_history_entry` | `brave-remove-data:delete_history_entry` | `url` |
| `remove-data` | `delete_history_range` | `brave-remove-data:delete_history_range` | `end`, `start` |
| `remove-data` | `clear_browsing_data` | `brave-remove-data:clear_browsing_data` | `since`, `types` |
| `remove-data` | `remove_bookmark` | `brave-remove-data:remove_bookmark` | `url` |
| `remove-data` | `remove_bookmark_folder` | `brave-remove-data:remove_bookmark_folder` | `folder` |
| `remove-data` | `remove_reading_list_entry` | `brave-remove-data:remove_reading_list_entry` | `url` |
| `remove-data` | `delete_downloaded_file` | `brave-remove-data:delete_downloaded_file` | `filename`, `url` |
| `scripts` | `run_script` | `brave-scripts:run_script` | `code`, `url` |
| `input` | `click_element` | `brave-input:click_element` | `button`, `count`, `element_number`, `name`, `role`, `url` |
| `input` | `type_text` | `brave-input:type_text` | `element_number`, `name`, `role`, `text`, `url` |
| `input` | `select_option` | `brave-input:select_option` | `element_number`, `name`, `option`, `role`, `url` |
| `input` | `press_key` | `brave-input:press_key` | `key`, `url` |
| `input` | `scroll_page` | `brave-input:scroll_page` | `direction`, `pages`, `url` |
| `input` | `wait_for_element` | `brave-input:wait_for_element` | `name`, `role`, `seconds`, `url` |
| `input-trusted` | `click_element_trusted` | `brave-input-trusted:click_element_trusted` | `button`, `count`, `name`, `role`, `url` |
| `input-trusted` | `type_text_trusted` | `brave-input-trusted:type_text_trusted` | `name`, `role`, `text`, `url` |
| `input-trusted` | `press_key_trusted` | `brave-input-trusted:press_key_trusted` | `key`, `url` |
| `input-trusted` | `hover_element` | `brave-input-trusted:hover_element` | `name`, `role`, `url` |
| `input-trusted` | `drag_element` | `brave-input-trusted:drag_element` | `name`, `onto_name`, `onto_role`, `role`, `url` |
| `capture-trusted` | `screenshot_full_page` | `brave-capture-trusted:screenshot_full_page` | `url` |
| `console-trusted` | `read_console` | `brave-console-trusted:read_console` | `levels`, `max_entries`, `url` |
| `upload-trusted` | `choose_file` | `brave-upload-trusted:choose_file` | `name`, `path`, `role`, `url` |
| `screen` | `capture_screen` | `brave-screen:capture_screen` | none |
| `screen` | `record_gif` | `brave-screen:record_gif` | `frames`, `url` |
| `held` | `take_held` | `brave-held:take_held` | none |

### Page interaction

The `input` group ([BROWSER-33](../specs/browser.md#BROWSER-33)) clicks, types, chooses, presses a
key, scrolls and waits for an element in a page. The planner holds page content only when a person
lets it through with `vet_content`, and no tool hands it the page's text, so it names the element
in words it was given and does not read it off the page: it gives the exact URL of one open tab, a
role from a list of ten and an accessible name, in words it got from the person or from content a
person let through with `vet_content`. The extension finds the element, the person is shown which
element that was, and the extension acts only on that element.
[ROUTE-9](../specs/routing.md#ROUTE-9) says why this meets the rule.

A call that acts on an element has three steps, in `extension/tools.js`, `extension/input.js` and
`extension/approval.js`.

1. `resolveElement` finds the tab by its exact URL, as every tool does, and injects `inPage` with
   the step `find`. The page answers a status and, for one displayed element, its fingerprint (tag,
   role, name, its place in the tree as child numbers and where it goes), and the facts the window
   shows. With `outline` the page also scrolls the element to the middle of its window and draws an
   outline around it ([the confirmation window](#the-confirmation-window)). A status other than `ok`
   ends the call with a sentence from a closed list and a count.
2. `confirm` puts the resolution to the window and returns its answer. The window is set with
   `setConfirmation`. It is given the resolution, whose `description` holds the action, the tab's
   URL (300 characters) and origin, the element's tag, role and name (200), the start of its text
   (80), where a link or a form goes (200), its rectangle in the page's window, and the text, option
   or key. All of it is text or numbers, and the page wrote the name, the text and where it goes. It
   answers with `approve(resolution)` when the person allows and otherwise throws a refusal that
   says why. Until a window is set nobody allows anything.
3. `performAction` spends the token, which is valid once and for that resolution alone, and refuses
   without one. It injects `inPage` with the step `act`. The page finds the element again with the
   same role and name, compares the fingerprint, scrolls the element to the middle of the window,
   refuses if another element is at its middle, and then dispatches the events.

Between the second and third steps the outline is taken out of the page, whatever the answer was.
Only one call at a time is between the first and the third: a second is refused.

Every function runs in the isolated world of the extension, so a page that replaces `getAttribute`,
a `value` setter or `Event` in its own world does not change what the function calls. The two
functions were run in Brave 1.98.62, headless with a scratch profile on 127.0.0.1, in an isolated
world made through the DevTools protocol, against a page that replaced all three. They found the
elements, put a field's value in through the prototype's setter, which the page had replaced in its
own world, and the page's handlers saw events with `isTrusted` false. That run was not an
extension's content-script world, and `scripting.executeScript` with `world: "ISOLATED"` was not
called in a real Brave.

**Synthetic events.** The events are made by script, so the page sees `isTrusted` false. A site that
checks it ignores the click or the key. A synthetic event does not do what the browser does by
itself for the real one: a key press does not submit a form, move the focus or scroll, a click on a
file input opens no chooser, a click in a canvas has no element to land on, a paste or copy needs
the clipboard, and a JavaScript dialog blocks the page's script and cannot be answered from the
page. A click on a link or a button does follow it, since dispatching `click` runs the activation
behaviour. The `description` names where a link goes for that reason. Trusted input events,
answering a dialog and dragging need `debugger`, which the second extension holds
([The second extension's windows](#the-second-extensions-windows),
[open decisions](#open-decisions)). Acting in a frame is done by neither extension.

**What the matcher is not.** It is not the browser's accessibility tree. It computes the role and
the name from a short list of rules and differs from the browser in these ways, each of which makes
a call find no element, or more than one, or an element the person is shown and can refuse:

- The role is the first word of the `role` attribute that is on the list, and the tag gives the
  role of a link, button, text input, text area, select, option, checkbox, radio and editing host.
  No other implicit role is computed: a heading, a list item and a summary have none, and a number
  input and a multiple select have none on the list.
- The name leaves out `aria-describedby`, generated content (`::before`), the text of a `slot`'s
  assigned nodes, the `alt` of an `area`, the caption of a table and the legend of a fieldset, and
  does not apply `text-transform`, so a name read from a screenshot of upper case text is compared
  without regard to case. `aria-labelledby` finds elements in the tree of the element only.
- An element counts as hidden when it or an element above it is not rendered, invisible, transparent
  or marked hidden, or is one pixel or less across. An element the browser leaves out of the
  accessibility tree for another reason is still found, and an element it keeps in is not found
  where this rule hides it.
- A frame is never entered, so the elements of a frame are not found, and a frame over an element is
  a cover. A closed shadow root is entered through `chrome.dom.openOrClosedShadowRoot`, which exists
  in the extension's isolated world and was not available in the headless run, where only open roots
  were entered.
- The cover test asks which element is at the middle of the rectangle after scrolling it into view.
  An element that is only partly covered, or covered at a corner, is not found covered.

A page can still put a decoy where the person's attention is. A decoy with the same role and name
makes the match fail, since a name that leads to two elements names none, and a page with only the
decoy gives the person an element to refuse that is not the one they meant. The window is where that
is caught, and the `description` is what it has to show them.

### Looking at a page

The planner and the driver never hold a page's content, so a model that is to act on a page does not
read the page unaided. A tool's result is held as a reference that names its type and size and
nothing it holds ([MCP-1](../specs/mcp.md#MCP-1), [LABEL-3](../specs/labels.md#LABEL-3)). Two things
turn a reference into something the model reads:

- `vet_content`. The planner names the reference. The person is shown the exact text, or a copy of
  the picture to open in their own viewer, and the model reads it once if the person answers yes
  ([VET-1](../specs/tools/vet-content.md#VET-1)). Where auto-vetting is on, a check, which is a
  model, answers in the person's place when it finds nothing. Nobody reads the content then, and the
  check can be wrong ([CHECK-12](../specs/vetting.md#CHECK-12)). A delegate is not offered
  `vet_content`.
- The person's own words. They read the result in the transcript
  ([VIEW-3](../specs/terminal-transcript.md#VIEW-3)) and write what the session needs.

What the model reads is what the person wrote, the files and configuration they vouched for, and a
result let through the first way. What it cannot read is a result before then, whichever tool made
it: the text of a page, its links and forms, the list of its elements, a screenshot, a recording of
a page or a screen, its console, what the person held out, and a processor's answer, which is a
reference as well. A result let through stays private, so the arguments the model writes after it
are private and each call is asked about whatever a rule says
([PERM-9](../specs/permissions.md#PERM-9)), and letting it through unlocks no egress.

A model that looks at a screenshot can act on the page in rounds of four steps:

1. **Look.** `screenshot_page` (`page-read` on all sites) or `screenshot_full_page`
   (`capture-trusted`) returns a picture the model holds as a reference. The model calls
   `vet_content` on it, the person opens the copy and answers yes, and the model sees the picture
   once.
2. **Name.** The model writes the role and the accessible name of the element in its call, from what
   it saw. It can write an `element_number` instead, from the list `list_page_elements` made. The
   person told the model the number after reading the list, or let the model read the list at
   `vet_content`.
3. **Confirm.** The terminal asks about the call. The extension finds the one element, brings the tab
   forward, outlines it, and a window of the browser shows it and where it leads. Nothing is done
   until the person presses **Allow once** there.
4. **Act.** The extension acts on that element if the page still shows it as the person was shown.

The cost of a round is one vetting step for each look and one confirmation for each action on an
element. A screenshot is a new reference each time and a yes covers one reference once, so a second
look is a second vetting step. An action on an element is two answers, the terminal's question about
the call and the window. `scroll_page` and `wait_for_element` have the terminal's question only. The
call that took the picture is asked about too, unless a rule or an earlier answer covers it. Where
auto-vetting answers the look, the person's part of a round is the confirmation. A one-shot run, a
delegate and a client that cannot draw the question refuse an action on an element with nobody asked
([ROUTE-9](../specs/routing.md#ROUTE-9)).

[ROUTE-9](../specs/routing.md#ROUTE-9) says why the round meets the rule. The model writes words,
the page decides which element they lead to, and the person is shown that element before anything
is done. The model chooses nothing from the page that it was not let read.

### Numbering the elements of a page

An element can be named by a role and an accessible name, or by a number
([BROWSER-41](../specs/browser.md#BROWSER-41)). A name fails when two elements have it, and a person
may not know the accessible name of the one they mean. `list_page_elements`, a tool of `page-read`,
numbers the controls of the top document of one tab and returns the list as text. `click_element`,
`type_text` and `select_option` then accept `element_number` in place of `role` and `name`.

The list is a tool's result, so it is untrusted and private: the person reads it in the transcript
and the planner holds a reference it cannot read. The person tells the session which line, in their
own words, as they would give a URL, or the session reads the list after the person let it through
with `vet_content`. [ROUTE-9](../specs/routing.md#ROUTE-9) says why that meets the routing rule. A
processor can also choose the number, and a gate of the kernel then carries it into the call after
the person answers it ([ROUTE-10](../specs/routing.md#ROUTE-10)).

1. `list_page_elements` injects `inPage` with the step `list`. The page walks the elements as the
   matcher does and answers, for each element that has a role, is displayed and is not refused, its
   role, its name cut at 120 characters, where it goes, its place in the document and its
   fingerprint, in tree order and at most 40, and counts of what it left out and why. It reads no
   value of a field, and a password, payment card or file field, a disabled or read-only element and
   one that is not displayed are counted and not numbered. The worker drops an element whose link or
   form goes to a blocked site, builds the text, and keeps the places and fingerprints with the URL
   and the time in `chrome.storage.session` (`extension/numbering.js`).
2. A call with `element_number` reads that record. It is refused, with a sentence of ours and before
   the page is reached, when there is none, when it is for another URL, when it is more than five
   minutes old, and when the number is past its end. Otherwise `resolveElement` injects `inPage` with
   the step `find`, the place as `at` and the fingerprint as `expect`, and no word the planner wrote.
3. The page finds the element at that place and answers `stale` unless it is displayed and has the
   fingerprint, before it draws the outline. The rest is the path of a role and a name: the facts for
   the window, the outline, the answer, and the fingerprint compared again by the step `act`.

**What the list is bounded by.** One list is kept, so a second `list_page_elements` replaces the first
and a number refers to one URL. The record holds at most 40 entries, each a place and a fingerprint of
at most 2,000 characters, so an element whose name and destination are longer is not numbered. It is
kept in memory that ends with the browser and is forgotten when `page-read` or `input` is turned off,
which Turn everything off does, and when either is turned on. Five minutes is how long a person has
to read a list of up to 40 lines, give the number, answer the question at the terminal and answer
the window. The element is compared again at the action, so the age does not decide whether a number
is the element listed. It bounds how long the page's names stay in memory.

**What a number is not.** It is not a node id. A node id read from a page says nothing a person can
check, and a number that is a line of a list they read, next to the role and name of the element, is
what they can check. A page can still word a line to mislead, as it can word a name, and the window
is where the element itself is shown, so the person who allows without comparing the window with the
line they chose has let the page choose. When two elements are identical in the window, with the same
role, name, text and destination, the window tells them apart by the number and by which of the
displayed elements with that role and name the element is, such as 2 of 3, besides the outline. The
page counts them in document order in the same call that finds the element by its place, with the
same comparison of role and name as a search by name, and the extension passes on the two whole
numbers. The window is the one shared with the second extension, so it has a row for the number that
stays hidden for a description without one, and the second extension's descriptions have none.

**Seen in Brave 1.98.62** (headless, a throwaway profile, 127.0.0.1 only, a copy of the extension with
`tabs`, `scripting`, `webNavigation` and the hosts made required so that no prompt was needed, the real
`handle`, windows and `inPage` running). On a local page with two buttons named Buy, a link to another
site with a query, a `javascript:` link, a text field with a value, a password field with a value, a
payment card field, a file input, a disabled button, a hidden button, a select and a button whose
name held a right-to-left override: the list numbered seven elements, wrote the override as
`[U+202E]`, gave `https://elsewhere.invalid` and `javascript:` for the two links with no path or query,
counted three controls not displayed (the hidden button and the two options of the select, which have
no box while the select is closed) and four refused, and held no value of a field. The record in
session storage held a place and a fingerprint for each of the seven and no value. A click by name on
Buy failed with a count of two; a click by the second Buy's number opened the window with the same
rows as for a name, and after a click on Allow once sent to the window through the debugging protocol
the page had a click on the second button only, with `isTrusted` false and the outline gone. Typing by the number of
the text field and choosing in the select by number worked the same way. A button renamed, an element
hidden, a link retargeted and a new element put before the button each got the sentence for an element
that is not the one listed, with nothing done and no outline; a field turned into a password field got
the sentence for a password field. A number past the end, 0, a number for another page after a list of
a second page were each refused with their sentence, and Turn everything off left no list in session
storage. A list older than five minutes was not waited for; it is tested with a clock.

### What the second extension's matcher is

The second extension ([BROWSER-38](../specs/browser.md#BROWSER-38)) does not run a function in the
page. It resolves a role and a name from what the browser says about the page, in
`extension-trusted/resolve.js`, with reads only: the accessibility tree says what each element is
called and what it is, the flattened document says what attributes it has and what is above it, and
the box, the computed style and a hit test say whether it is shown and reachable. The steps are
the BraveBot extension's: find, show the person, find again, act only if it is the element shown.

**What was seen in Brave 1.98.62** (Chromium 156.0.8078.74, macOS, headless, a throwaway profile,
127.0.0.1 only, the second extension's own worker calling the debugger):

- `DOM.getFlattenedDocument` with a depth of -1 and `pierce` lists every node once, with `nodeId`,
  `parentId`, `backendNodeId`, `nodeType`, `localName` and the attributes as one list of names and
  values. The list is not in document order, so it gives a node's parent and not its place among its
  siblings. A shadow root is not in the list. It is inside its host's `shadowRoots`, closed and
  user-agent ones included, and its children name it as their parent. A frame's document is inside
  the frame element's `contentDocument`, and its nodes are in the list. A `nodeId` from the list is
  accepted by `CSS.getComputedStyleForNode` at once. A page of 45,000 elements gave 75,011 nodes,
  11 MB and 0.37 s for it, and 90,004 nodes, 34 MB and 1.5 s for the accessibility tree.
- The element above a button can be transparent while the button is not: `getComputedStyleForNode`
  gave the button an opacity of 1 inside a `div` of 0, and the tree kept it. The opacity of each
  element above is read, one call each, and the answers are kept for the next candidate.
- The tree's roles: `input type=search` is `searchbox`; text, password and card inputs and
  `textarea` are `textbox`; `type=number` is `spinbutton`; an input with `list` is `combobox`; a
  `select` is `combobox` and a multiple one `listbox`; `role=menuitem`, `tab` and `switch` are as
  written, and a checkbox input with `role=switch` is `switch`; a file input is `button`; a
  `contenteditable` element is `generic` with an `editable` property; an `option` of a closed select
  is `option` below a `MenuListPopup`.
- Left out of the tree: `display: none`, `hidden` and `visibility: hidden` give no node, and
  `aria-hidden` gives an ignored node with the role `none` and no name. Kept in it: `opacity: 0`, on
  the element or above it, and a box of one pixel.
- Properties: `disabled` is true for a disabled button, for `aria-disabled` and for a button in a
  disabled fieldset, and `readonly` is true for a read-only input.
- The tree of the main frame holds an `Iframe` node and nothing of its content. A tree for the frame
  needs its `frameId` and is not used.
- `DOM.getBoxModel` answers in the window's coordinates and `DOM.getNodeForLocation` takes page
  coordinates, which are the window's plus the scroll. A location outside the window answers "No node
  found at given location". For a button with text it returns the button. For a point over a frame it
  returns the node inside the frame with the frame's id, and for a closed shadow root the button
  inside it. Asked in window coordinates after a scroll, it answered with other elements or none,
  which is why the cover test adds the scroll.
- `DOM.scrollIntoViewIfNeeded` leaves a wholly visible element where it is, scrolls another to the
  middle of the window as far as the end of the page allows, and does nothing for a fixed element
  outside the window.
- `Page.getFrameTree` gives the frame's address without its fragment and the fragment as
  `urlFragment`. `getTargets` gives the address with it.
- Once the debugging notice was shown, the window lost 56 pixels of height (475 to 419 in this
  headless window), and `Page.getLayoutMetrics` said so. The metrics are read at every resolution.
- The record in `extension-trusted/tests/recorded/` (a page of 135 elements) gives through
  `resolve.js` the same outcome, point, rectangle and description as Brave gives for each of 49
  requests, with the window's height set to the 419 pixels Brave had.

**How it differs from the BraveBot extension's matcher** (the list above in "What the matcher is
not" is the other half):

- The role and the name are the browser's own. The gaps listed for the first matcher do not apply:
  an implicit role it does not compute, generated content, the text of a slot, the `alt` of an
  `area`, a table caption, a legend. The comparison is the same: both sides lower-cased, white space
  collapsed.
- A `searchbox` counts as a `textbox`, whether it is an `input type=search` or an element with
  `role=searchbox`, which the first matcher does not find. A `contenteditable` element is a
  `textbox` to the first matcher and is not found by this one unless it carries `role=textbox`, since
  the tree calls it `generic`. A number input and a multiple select have no role of the ten in
  either.
- An element with `display: none`, `hidden`, `visibility: hidden` or `aria-hidden` is not in the
  tree, so a call for it fails as no element, where the first matcher says it is hidden. Transparent
  elements and boxes of one pixel are counted and fail as hidden in both. The rules of the first
  matcher for a hidden mark and a style are applied here as well, to what the tree keeps.
- A label over a control is a cover. The first matcher does not count a label of the control, or
  what is inside it, as a cover. A control whose label is drawn over its middle fails here and is
  pressed there.
- Disabled and read-only come from the tree, which includes `aria-readonly`.
- The cover test, the scroll and the off-screen test are the first matcher's, with the scroll
  explained above. A closed shadow root is entered without `chrome.dom`, which the headless run of
  the first matcher did not have.
- The fingerprint holds the node's number and the numbers of the nodes above it, where the first
  matcher's holds the child numbers from the document down. The node is the same node while it
  exists, so a node that is replaced or moved to another parent differs, and a node that gets a new
  sibling does not.
- The text shown is the start of the tree's text nodes, and where a form goes is worked out from the
  attributes and the page's base address, since the document read gives no resolved address.
- The tab's address is compared with the frame's, fragment included, at each resolution.
- A page with more than 100,000 nodes in the tree or the document, or more than 50 elements with the
  role and the name, is not searched.

### The confirmation window

`extension/confirmation.js` in the service worker opens the window and waits for the person's
answer. `extension/confirm.html`, `confirm.css` and `confirm.js` are the window, and
`extension/shown.js` makes the page's words visible in it. `extension/actions.js` words each
action, `extension/approval.js` holds the token and the numbers the window and the worker agree
on, and `extension/errors.js` and `extension/words.js` hold the errors and the lists of words. The
window's heading and title name the extension by its manifest. `extension/shared-files.json` lists
the files that a second extension holds as copies.
[BROWSER-34](../specs/browser.md#BROWSER-34) is the clause.

**Why a window of its own.** The question has to be somewhere a page cannot draw, restyle, click or
close. An overlay in the page would be drawn by the page's own document, and a page that wanted a
click on Allow would only have to cover or restyle it. A popup window of the extension is another
origin in another window: the page has no handle on it (`window.opener` is null in it), cannot fetch
or frame its page (it is not web accessible), and cannot send the extension a message (there is no
`externally_connectable`, and the page has no `chrome.runtime.sendMessage`). The overlay in the page
is kept as a hint for the person, and the approval does not depend on it.

**The foot of the window.** The wait line, the status and the two buttons are one bar,
`position: sticky` at the bottom of the window, with an opaque background, and the rest of the page
scrolls under it. The window is made 600 by 640. Measured in Brave 1.98.62, headless, in a window 480 wide, through the
real worker and windows of a copy of each extension (a throwaway profile, 127.0.0.1 only, nothing
left running), the page had a viewport 568 pixels high (outer height 600). A short description fit.
The longest descriptions did not: typed text of 2,000 characters with a name of 200, a text of 80
and an address of 300 came to 907 pixels in the first extension, a drag with two such elements to
946, a type of the longest text to 929, and the panel for a dialog with a message of 2,000
characters and a prompt to 823. The buttons were then at 861 to 934 pixels, or 522 to 556 after the
page was scrolled to its end, so Allow once and Deny were outside the viewport until the person
scrolled. With the bar they were at 522 to 556 pixels at the top of the page and at the end of it, in
every case. The alternative of making the window taller was not taken, since the longest case would
still be taller than a small screen, and the page's words are bounded by the limits of `words.js`
only, not by a size.

**The browser's notice, the width and the order of the rows.** Measured in a visible Brave 1.98.63
(Chromium 156) on macOS, with a throwaway profile, 127.0.0.1 only, both extensions loaded from
copies, the second extension's debugger attached to a page, and each window photographed through the
browser's own window capture with the title of the window chosen by
`--auto-select-window-capture-source-by-title`. While an extension debugs the browser, Brave shows a
56 pixel bar across the top of every popup window, in the windows of both extensions, as wide as the
window: `"BraveBot Trusted Input" started debugging this browser`, a Cancel button and a close
button. In a window 480 wide the sentence was cut at its start (`...Bot Trusted Input" started
debugging this browser`). In a window 560 wide it read in full with 48 pixels to spare at its
left; the sentence is about 350 pixels wide and the buttons about 110. The window is 600 wide. The
viewport under the notice is 552 pixels high, and the bar of the buttons is 83 of them.

With the window 480 wide, no limit on a row's height and the destination below the name and the
text, a name of a letter and 200 marks (435 pixels tall), a tag of 198 marks and a text of 80 marks
put the destination at 1241 to 1263 pixels of a page 1421 pixels tall, a drag with a name of 200
marks at 935 to 958, and a text of 2,000 characters typed with those words at 1595 to 1684. In the
window now, 600 wide, a row of the element is at most five lines (`5em`) tall and scrolls, with its
scroll bar drawn by the page's style sheet so that a platform that hides a scroll bar until it is
used still shows that the row holds more, the destination is the first row of the element and the
typed text is below the element. The site and the address are in boxes of `12em`, as the next
section sets out. The same words put the destination at 167 to 189
pixels and 183 to 205 for the drag. A row of 200 marks holds 336 pixels in a box 76 tall.

**The site and the address.** `run_script` refuses an address of more than 300 characters and shows
it whole. The box of five lines held 76 of the 159 pixels that 300 of the widest letter take, so the
rest of the address was behind a scroll bar. Brave 1.98.63 (Chromium 156) reports the address of a
tab in ASCII. A probe extension with `tabs` called `chrome.tabs.query` for pages at `/#` followed by a
zero width space, a combining mark, a right-to-left override and a variation selector, at `/p`
followed by a right-to-left override and at `/a` followed by a variation selector, and at `/?q=a`
followed by a combining mark, and every one came back as percent escapes (`Target.getTargets` did
the same for zero width spaces in the path and the query), so a row of marks is not the address of a
tab. The site and the
address are in boxes of `12em`, which is 168 pixels. The longest address, 300 of the widest letter,
takes 142 pixels, and 300 `a` take 91. A row of 300 marks, which no tab reports, would take 499
pixels, and the box shows 174 of them and scrolls, so it cannot push the destination out of view.

Measured with the real `confirm.html`, `confirm.css` and `confirm.js` served over http on 127.0.0.1
to a throwaway headless Brave 1.98.63, a stand-in for the port, and a viewport of 600 by 552, which
is the window under the notice. The pixels are the row of the address, the destination (top and
bottom of its box), the top of the bar and the buttons at the top of the page, before and after:

| Description | Address | Destination | Bar from | Buttons |
|---|---|---|---|---|
| A click; address of 300 `W`; destination of 187 `W` | 76 (159 of content), then 142 | 222 to 298, then 288 to 364 | 426, then 468 | 464 to 498, then 506 to 540 |
| A click; address of 300 `a` | 76 (91), then 91 | 222 to 298, then 237 to 313 | 426, then 441 | 464 to 498, then 479 to 513 |
| `run_script`; address of 300 `W`; 60 lines of code | 76 (159), then 142 | no element | 443 | 506 to 540 |
| A name of a letter and 200 marks, a tag of 198 marks, a text of 79 marks | 23 | 169 to 192, unchanged | 468 | 506 to 540 |
| A right-to-left override in the name, the text and the address | 40 | 186 to 209, unchanged | 388 | 426 to 460 |
| A word of 1,200 characters as the name, the text and the destination | 23 | 169 to 243, unchanged | 439 | 477 to 511 |
| Typed text of 2,000 characters, a site of 292, an address of 300 `W`, a destination of 187 `W`, a name of 200 marks, a tag of 198, a text of 79, a number and a place | 76, then 142 (site 76, then 125) | 296 to 371, then 411 to 486 | 443 | 506 to 540 |
| An address of 278 zero width spaces | 76, then 174 (499 of content) | 222 to 298, then 320 to 396 | 426, then 468 | 464 to 498, then 506 to 540 |

The bar of the typed text is 109 pixels high, 25 more than the 84 of the others, while the note that
the box has not been scrolled to its end shows. In the last of the long cases the first two lines of
the destination are above the bar and the rest is reached by scrolling the page, where before the
destination was whole above it. In a viewport of 640 the buttons were at 594 to 628 at the lowest.
Where the page scrolls, the buttons were at 494 to 528 at its end. Allow once and Deny were in view
in all of them.

**Allow once and the box.** A probe extension that held the real `confirmation.js` and the files of
the window opened a real popup in a throwaway headless Brave 1.98.63, 600 by 568 inside, for 100
lines of code that filled 718 pixels of a box 301 pixels tall. Allow once was disabled when the
window was shown and 1.5 seconds later, with the note in the status line. A click made with the
debugging protocol on it left the call waiting, and a wheel of 150 pixels left it disabled. A wheel
to the end of the box enabled it, a click then ended the call as allowed, and no window was left.
With the keyboard, Shift+Tab from Deny focused the box, End scrolled it to its end and enabled Allow
once, and Tab went to Deny and then to Allow once. Deny ended the call with its sentence and left
no window. In the page served over http, a window made 420 wide made a box that had been read
scroll again and disabled Allow once, a box that holds what it shows enabled Allow once after the
second as before, and the box of a call that carries nothing, which has no size, held nothing back.

**What happens in a call.**

1. The page scrolls the element to the middle of its window and draws the outline.
2. The tab is made the active tab of its window, so the outline can be seen. `tabs.update` with
   `active` needs no permission the group lacks.
3. The worker creates the popup window, 600 by 640, and starts a timer of 60 seconds
   (`CONFIRMATION_WAIT_MS`). A browser that never says whether it opened the window leaves the
   question to the timer, which ends it as a denial.
4. The window connects a port named `bravebot-confirmation`. The worker checks that the port is from
   this extension, from `confirm.html` in its top frame and from the window it created, and sends
   `show` with the description and the time it will deny at. A port that fails any of these is sent
   `ended`, which closes the window, and its messages are dropped.
5. The window draws the description with text nodes. Deny has the focus. Allow once is disabled, and
   is enabled one second after the window is shown or gets the focus back (`ALLOW_DELAY_MS`), and is
   disabled again when the window loses the focus or is hidden. When the box of the code, the text or
   the path holds more than it shows, Allow once also waits until the box has been scrolled to its
   end, and the status line says so; the reading starts again when the window is made another size.
6. A click on Allow once counts only if `isTrusted` is true and `navigator.userActivation.isActive`
   is not false. It sends `answer` with `allow: true`. Deny, Escape, closing the window and the
   timer send, or amount to, a denial. Only `allow` exactly `true` allows.
7. The worker closes the window, waiting at most one second (`CLOSE_WAIT_MS`) for the browser to
   do it, settles the question with `approve(resolution)` or a refusal that names the reason, and
   the outline is cleared. A window that opens after the question has ended finds none waiting, is
   sent `ended` and closes itself.

**What keeps the worker running.** Chrome's documentation says an extension service worker is
stopped after 30 seconds without activity, and that an open native messaging port counts as activity
in Chrome 105 and later. Brave 1.98.62 stopped an idle worker within 40 seconds. The port was not
observed here, since the run had no host. The window also sends `alive` every 15 seconds
(`HEARTBEAT_MS`), each of which is an event the worker receives, so a worker without a native host
(a host that crashed, or the run here) is kept running for the 60 seconds as well, which was
observed. A worker that stops anyway takes the question with it: its connection to the host closes,
so the relay fails the call; the window finds its port closed, says nothing was done, disables both
buttons and closes (observed); and the next worker closes any window of the page it finds with
`runtime.getContexts` as it starts (not separately observed, since the window had closed itself).
The outline, which only the worker could remove, hides itself (the box inside its root fades with an
animation of `opacity` that has a delay of 70 seconds, `OUTLINE_MS`) and is removed by the next
input call or by a navigation.

**Choices.**

| Choice | Chosen | Alternative | Why |
|---|---|---|---|
| Time to answer | 60 seconds | 30 (the relay's wait today) or 100 | The person has to find the window, read it and wait a second. The relay's wait for the group is 90 seconds and the bound its declaration puts on a call 120, which a relay test holds above this and above the longest `wait_for_element`. |
| A second action while one waits | Refused with a sentence | Queued | A queue puts a second window behind the first and its wait runs on past the relay's. Refusing leaves the first window the only one the person can see. |
| Delay of Allow once | 1 second after the window is shown or in front | None, or longer | A click made as the window appears under the pointer, or after another window took the front, is the click a page would try to cause. |
| Allow once and a box that scrolls | Not available until the box has been scrolled to its end, and read until the window is made another size | Available after the second whatever the box holds | A person who allows code, text or a path they have not scrolled to the end of has allowed what they did not read, and the line count does not stop them. The terminal question takes no yes before the last row has been drawn ([PROMPT-4](../specs/prompting.md#PROMPT-4)). |
| The closing of the window | Waited for at most 1 second | Waited for without limit, or not waited for | A browser that never answers would hold the call, its place in line and the 80 seconds of the other extension. Not waiting would let the next question open while the window is still there. 5 + 60 + 1 + 11 + 2 is 79 seconds of the 80. |
| How the window and the worker talk | A port | `runtime.sendMessage` | A port reports a closed window or a stopped worker to the other end at once. |
| Which events count | `isTrusted` and user activation | Any click | A click a script made in the window is refused where the platform lets that be told. Input sent through the debugging protocol is trusted and cannot be told apart. |
| Scroll | No window | A window | `scroll_page` acts on no element and changes no state of the page; the question in the terminal has been asked. A window for each scroll would be a prompt per screen. |
| The tab | Brought forward | Left where it is | The outline and the element cannot be seen in a background tab. |

**What a page can try, and what happens.**

| The page | Result |
|---|---|
| Hides the outline, removes it or draws its own | The action is not affected. The window still asks, shows the facts, and says the outline is a hint. |
| Reads or restyles what is in the outline's root | `host.shadowRoot` is null and `attachShadow` on the host throws. A page that replaced `Element.prototype.attachShadow` in its own world to capture roots captured nothing, because the extension's function runs in its own world. The page can style the host element, which holds no content. |
| Opens, frames, fetches or messages the window or the worker | It cannot: not web accessible, no `externally_connectable`, no `chrome` object in a page. |
| Dispatches a click on the window's buttons | It has no handle on the window. A click a script makes in the window is refused by the `isTrusted` check. |
| Gets the person to click where the window is about to appear | Allow once is disabled for a second, and Deny has the focus, so a click or Enter lands on Deny. |
| Takes the front with a popup or `focus()` | Allow once is disabled while the window is out of front, and for a second after it returns. |
| Names a button "Cancel" that submits a payment | The window shows where the form goes, including the button's own `formaction`. The fingerprint includes it. |
| Writes code with a variation selector inside a name, so that two names that look alike are two variables | The window writes the selector out as its code point, as it does every default ignorable character, and the terminal question does the same. |
| Writes a name that looks like another part of the window, with line breaks or overrides | The name is shown inside its own row, cut to 200 characters, and every line break, override and invisible character is shown as its code point. |
| Names a control of a form after one of the form's properties, such as `action` or `parentElement`, so the form answers with the control | Every function injected into a page reads a node through its prototype (`pageReads` in `input.js`), which a control named for a property does not stand in front of. The destination of a form is its real action, the loops up the tree end by the number of elements, and the outline is found among the isolated world's own globals. The page can still change which element a name leads to, which the window shows. |
| Changes the element after the window shows it | `performAction` finds it again and refuses on any difference of tag, role, name, place or destination. |
| Floods the person with questions | Only a session starts one, each is asked in the terminal first, and a second while one is open is refused. |
| Leaves the window unanswered, or the person closes it | Both deny. |

What the extension cannot see is what the page's own script does when the click arrives. A button
that shows one address and posts to another by script is shown as what it is in the markup.

**Observed in Brave 1.98.62**, headless with a scratch profile on 127.0.0.1 and the extension loaded
unpacked with `tabs`, `scripting` and the two host patterns made required in the copy. The real
`handle`, `confirmation.js` and `confirm.js` ran, a real popup window opened (type `popup`, in
front, `document.hasFocus()` true), and the DevTools protocol pressed its buttons with trusted input
events. Seen: the window shows the description; Deny has the focus; Allow once is disabled at first;
a click a script made in the window did nothing; a protocol click after a second allowed the click,
and the page saw it with `isTrusted` false; Deny, Escape and closing the window each denied with
their reason and left no window and no outline; no answer for 60 seconds denied, with nothing
attached to the worker and only the window's messages as activity (a worker with no question and
nothing attached had stopped within 40 seconds); stopping the worker closed the window;
`runtime.getContexts` lists the window with its `windowId`; a window opened with no question waiting
was sent `ended` and closed itself; a button named "Cancel" in a form that posted to `/cancel`, with
a `formaction` to another host, showed the other host as where it goes; a name with markup and a
right-to-left override, and a text with a line break, an override, a bell and `<b>`, appeared as
text with one marked element for each unseen character and no `img` or `b` element in the window
(screenshots were taken and looked at); the page could not read the outline's root or capture it,
could not fetch the window's page, and its frame of it showed the blocked page; the window's policy
blocked an inline script, an image, a request to 127.0.0.1 and an inline style attribute. Not seen,
because headless has no real window system: a person's click, the window's look and placement, a
window losing the front to another window or to a page's popup (`document.hasFocus()` stays true
under the protocol, so the blur handling is tested with fakes only), the native messaging port
keeping the worker running (the run had no host), and another system than macOS. The outline's
hiding was seen with a delay of 2 seconds in place of 70: the box inside the root faded to an
opacity of about 1e-13 when the delay passed. An animation of the host itself did nothing, since the
host's own style is fixed with `!important` and that wins over an animation, so the box is what
fades. The 70 seconds were not waited out.

### The second extension's windows

[BROWSER-38](../specs/browser.md#BROWSER-38) is the clause. The second extension asks the person
before any input in the window of the previous section. `extension-trusted/` holds byte-for-byte
copies of `confirmation.js`, `confirm.html`, `confirm.css`, `confirm.js`, `shown.js`, `approval.js`,
`errors.js` and `words.js`, since an extension cannot load a file from outside its own directory
and the repository has no build step. `extension/tests/shared.test.mjs` compares each copy with the
original, and `extension-trusted/tests/confirmation.test.mjs` runs the original's window tests
against the copies. What each extension writes for itself is `actions.js`, which words its actions
for the window, and the heading, which the window reads from the manifest's name.

**A drag has two elements.** The window has one set of rows for the element, and one frame that
shows text whole, which `type` and `key` use for their argument. A drag shows the element it takes
in the rows and the element it drops onto in the frame, as `role "button", tag "div", name "Trash",
text "Drop here", goes to ""`. Each word of the page is in quotation marks with a quotation mark or
a backslash escaped, and cut to the limit of the row it would have had, so that a name that holds
`", goes to "https://good.example` reads as a name. The alternative is a second set of rows in the
window, which would change `confirm.html` and `confirm.js` in both extensions.

**Two pages, one port name.** The shared confirmation takes the port named `bravebot-confirmation`
and sends away every such port that is not the window it opened, because a window it did not open
has no business answering. The dialog panel uses the same code and the same port name, so two
confirmations on one `onConnect` would send each other's pages away. `windows.js` registers one
listener and hands a connection to the confirmation made for the page it came from, by the address
of its sender, through a view of the browser that holds only that page's listeners. A connection
from a page with no question waiting for it is sent away, as the shared code does.

**What the shared window cannot carry.** A confirmation answers allow or deny, and a dialog has two
answers that both need the person: accept and dismiss, and for a prompt the text. The panel's two
buttons both send `allow: true`, so the worker gets a token for either, and add `accept` and `text`
to the same message. The view of `windows.js` reads them from that message only for the port the
confirmation sent `show` to, which is the port it admitted, and keeps the first. A token alone
carries no choice, so the choice is part of the answer `handleDialog()` is given, and the token is
checked against the question for this session's dialog before it is sent.

**The time.** The shared confirmation fixes its wait when it is made. The panel's confirmation is
made for each question, with the time left of the call: `CONFIRMATION_WAIT_MS` from the moment the
action's window was asked. The panel counts down to that time, and its listeners are taken away when
the question ends.

**Observed in Brave 1.98.62**, headless with a scratch profile, the second extension loaded unpacked
beside a page on 127.0.0.1 (the harness appends probe lines to a copy of `background.js`, not the
repository file). With `debugger` as its only permission the extension opened its popup windows
(`windows.create` needs no other permission) and the real worker code ran against a real tab:

- The confirmation window opened in front with the heading "Allow BraveBot Trusted Input to do this
  in the page?" and `document.hasFocus()` true, Deny focused, and showed the element, tag, role,
  name, site and address. A click a script made on Allow once after it was enabled did nothing, and a
  click made with `Input.dispatchMouseEvent` allowed the action, which then pressed the real button:
  the page's handler saw `isTrusted` true.
- A click on a button whose handler calls `confirm()` stopped the browser's answer to
  `mouseReleased`. The session received `Page.javascriptDialogOpening` with the type, the message
  and the suggested answer, and `sendWatching()` returned the dialog while the release was
  unanswered. The panel opened, showed the message as text (the markup stayed text and the override
  character was shown as `U+202E`), and counted down to 58 seconds. A click a script made on Accept
  did nothing. A click made through the protocol on Accept answered `true` in the page, on Dismiss
  `false`, and for a prompt the text typed into the panel's field reached the page.
- A panel closed without an answer failed the call with `dialog_open`. The page stayed blocked: a
  `Runtime.evaluate` in the page did not return in 3 seconds.

Not seen, because headless has no window system: a person's click, the windows' placement and look,
a window losing the front to another window (the focus handling is tested with fakes), and the
debugging notice. A dialog the page opens after the browser has answered the input was not tried.

### What the second extension's tools send

[BROWSER-38](../specs/browser.md#BROWSER-38) is the clause. `extension-trusted/tools.js` holds the
five tools and `requests.js` runs them. A call is one session on one tab: `Page.enable`, the
resolution of each element, `Page.bringToFront`, the window, the resolution again, and the input.
The window comes after the reads and before the first input, and the second resolution is what
decides whether the input is sent, since a node that was replaced or moved has another
fingerprint. The approval of a call accepts the one token the window made for the call and the
same token again for the later inputs of the same action, because a token is spent by the first
check.

**Where a click goes, in each extension.** A click goes up through the node it lands on and every
element above it, and the first link or submitting button decides where. The BraveBot extension's
page function sends the click to the element, so it starts there and a link inside the element is not
followed; the second extension presses at the middle of the element, so it starts at the node the
browser says is there, which can be inside the element, and a link laid over the element in a page
with no script is then what the click follows. The page function reads the live nodes through their
prototypes (`goesOf` in `input.js`). The second extension has no live node: it has the flattened
document, whose nodes carry their attributes and their parent, so it takes `href`, `type`,
`formaction`, `form` and `action` from the attributes, finds the form of a control by its `form`
attribute or the nearest form above it in the same tree, and finds the default button of a form as
the first submit button among the nodes in the order the document lists them, which is document order
for nodes that are not inside one another. Both stop at the same things they cannot tell, an image
map and two places that differ, and call them unknown. The list of the BraveBot extension and its
blocked-site check use `goesOf` and no other computation. Before each press the second extension
reads the document again, after asking the browser what is at the point, and works the place out
again from it, so a page that moves a node out of the element, or puts a link where the pointer is,
between the window and the press, is stopped by the same two questions; before a key and before
typed text it reads the tree for the one focused element, since both go to whatever has the focus.

**What each tool sends.** A click is `mouseMoved`, then for each click `mousePressed` and
`mouseReleased` at the point with the button, the buttons held and the click count. The browser
is asked what is at the point after the move and before each press. Typing is `DOM.focus`, a
`rawKeyDown` of the key `Unidentified` with the command `selectAll`, its `keyUp`, and
`Input.insertText`. A page sees a key down and up of no key it knows and no `a`, and the old text
is replaced in an input, a text area and an editable element. After the focus, `resolveFocused`
must answer the same node, because the text goes to whichever element has the focus. A key is a
`keyDown` with the character for Enter and a `rawKeyDown` for the others, then `keyUp`, with the
Windows virtual key code, no modifier and no command. A hover is one `mouseMoved`. A drag is a
move to the first element, the check, a press, eight `mouseMoved` with the left button held,
the check on the second element, and a release at the second element. A drag that fails after the
press releases the button at the first element, where it pressed, because the point the check found
the first element at is the one point of the path the person was shown; a release at a point of the
path or at the second element could land on an element nobody was shown.
A click that fails after its press releases the button at the point it pressed at, which is the
point its release goes to, and tries a release that failed once more. A page that had already seen
the first release sees the second as a `pointerup` and a `mouseup` and no `click` (Brave 1.98.62).
The release sent after a failure is sent at once, and its failure is dropped, so the call waits 2
seconds for a dialog and 1 for the answer and not the 5 an input is given. An input the browser
stops answering therefore ends a call 7 seconds after it was sent and the release 3 seconds later,
10 after the person's answer and 70 after the first window opened, inside the 80 the first
extension waits.

A call also has 11 seconds from the person's answer for everything it does, so that a browser that
answers each step slowly cannot hold it past the 80. The answer is the Allow or, for a dialog, the
answer given in the panel: the time stops while the person reads the panel and counts again from the
answer. `withSession` holds the call to it, at the search for the tab, the attach and the work, which
makes it one place for the eight tools; `read_console` attaches after the answer, so the attach is
inside it. The picture's wait (10 seconds) and the console's (10) are inside it as well. When the 11
seconds pass the session ends at once, so the work that is left sends nothing, and the call is the
failure that says the browser did not finish in time.

The steps before the first window have 5 seconds together, counted from the start of the call by the
same clock (`callTime()`), and the count stops when the person is asked. Looking for the tab,
attaching, reading the page, finding the element and bringing the tab forward took 1.0 second before
the window on a page of 6,000 rows of four elements in headless Brave 1.98.63 (18 ms on a page of one
button), and the steps after the answer 1.2 seconds. A call that passes the 5 seconds is the failure
that says the browser did not answer before the person was asked and that nothing was done.

A button or key that a call had pressed and not released is released by the end of the call, however
it ended. The tool records the release (`owe()` in `cdp.js`) before it sends the press or the key
down and forgets it (`settle()`) once the release is answered or once the tool sends the release
itself on a failure, so the end of the call sends it only when the call was ended in between: by the
deadline, or by a check that stopped answering. The end of the call sends it through the same
function that sends every input, with the call's token, unless the page has a dialog open, waits 1
second for the browser's answer and then detaches, the two together within 2 seconds. The order is
the 5 seconds before the window, 60 for the person, 1 for a browser that does not answer the closing
of the window, 11 for the call, 2 for the release and the detach: 79 after the request arrived, 1
second before the BraveBot extension gives up. At the
deadline a key up is sent although the focus may have moved since the check before it, since the
browser cannot be asked then; the up of one of the eleven keys activates nothing. The release is
sent and may not arrive where the browser has stopped answering.

As the worker starts it detaches the debugger from every tab it lists as attached
(`detachLeftovers()`), and a call that arrives with the start waits for that before it attaches. A
session lasts for one call, so one still attached belongs to a worker that stopped, and the browser
keeps it.

**Observed in Brave 1.98.62**, headless with a scratch profile, the second extension loaded unpacked
beside a page on 127.0.0.1 (the harness appends probe lines to a copy of `background.js`, not the
repository file), and the real window allowed by a click made through the protocol:

- A click, a double click and a right click reached the page as `pointerdown`, `mousedown`,
  `mouseup` and `click` (`dblclick` after the second click, `contextmenu` for the right button),
  all with `isTrusted` true, with `detail` 1 and 2 and `button` 0 and 2. A hover gave `mouseover`
  and `mouseenter` and `:hover` matched.
- Typing replaced "old text" in an input and "old notes" in a text area. The page saw `keydown` and
  `keyup` with the key `Unidentified` and key code 0, no modifier, then trusted `beforeinput` and
  `input` with `insertText`. A letter `a` with the command also replaced the text but the page saw
  the letter; the same command with `meta` or `ctrl` showed the page that modifier.
- Each of the eleven keys gave trusted `keydown` and `keyup` with the code and key code of the
  key. Enter on a button gave `keypress` and a trusted `click` with `detail` 0, and Tab moved the
  focus. A key with the focus in a password field was refused, and so was a key with the focus on
  the page alone.
- A drag between two elements with `draggable` gave trusted `dragstart`, `dragenter`, `drop` and
  `dragend`, and a drag between two elements with mouse listeners gave `mousedown`, `mousemove` with
  the button held, `mouseover` on the second element and `mouseup` there.
- A button whose hover adds a layer over itself was moved to, the browser was asked again, found the
  layer and the press was not sent. An element the page replaced while the window was open gave
  "changed" and nothing was sent. A denial sent nothing. An element below the fold was scrolled to
  and pressed at the point in the window.
- A click whose release opens `alert`, and one whose press opens it, each opened the dialog panel;
  accepting it let the call end with the click done.
- The confirmation window for a drag holds 561 pixels of content in an inner height of 512 pixels in
  headless Brave, so Allow once is below the visible part and has to be scrolled to. Whether a
  headed window of the size the extension asks for leaves room is not known.

Not seen, because headless has no window system: a person's click, the debugging notice, a headed
window's size and placement, the native context menu of a right click, and whether a native drag of
an element in a headed window stops the browser answering the mouse events that carry it.

### Taking a picture of the whole page

[BROWSER-43](../specs/browser.md#BROWSER-43) is the clause. `captureVisibleTab` takes the visible
part of the visible tab, so a picture of the whole page or of a tab in the background needs the
debugger, which the second extension holds. `extension-trusted/capture.js` holds the tool. A call is
one session on one tab: `Page.enable`, the address (`Page.getFrameTree`), the size of the content
(`Page.getLayoutMetrics`), `Page.bringToFront`, the window, the address and size again, and
`Page.captureScreenshot` with the person's token. No `Emulation` method is used and `Runtime` is
not on the list.

**The window.** The window is the shared one, and a picture acts on no element, so the row of the
picture in its `actions.js` says so and the window leaves out the heading, the rows and the note
about the outline of the element. The size is a value that is not text, so the window does not
count its characters. The words of the action say that the browser's debugger is attached to the page
and reads everything on it. In Brave 1.98.62, headless, with a viewport of 568 pixels, Allow once and
Deny stayed in view in the bar at the foot of the window: at 332 to 366 for a short address, at 434
to 468 for the longest origin and address, and at 522 to 556 at the top of the page, 510 to 544 at
its end, where the size text was 2,000 characters long.

**Choices.** The group is `capture-trusted` and the alias `brave-capture-trusted`. A read in
`input-trusted` would be turned on under the words "real clicks and typing", which this tool does not
do, and `screen` lists `desktopCapture`, a permission of the first extension that
picks a screen or a window. The group is shaped as `input-trusted` is, so one second extension serves
both, and the options page, the install line and the waits are the same.

**What was measured** in Brave 1.98.62 (Chromium 156), headless, on pages served from 127.0.0.1.
`Page.captureScreenshot` with `captureBeyondViewport` and a clip of the content size gave these
JPEGs at quality 80, for pages of 1280 CSS pixels in width except the third.

| Page | Clip scale | Picture | Bytes |
|---|---|---|---|
| 1280 by 6040 | 0.339 | 434 by 2048 | 66 KB |
| 1280 by 40000 | 0.0512 | 66 by 2048 | 4 KB |
| 6000 by 663 | 0.3413 | 2048 by 226 | 8 KB |
| 1280 by 719 | 1 | 1280 by 719 | 7 KB |

With `--force-device-scale-factor=2` a picture is `clip * scale * 2` pixels, so a clip at the scale
for one pixel to a CSS pixel gave 868 by 4096. The `contentSize` of `Page.getLayoutMetrics`, which
is in device pixels, was 2560 by 12080 where `cssContentSize` was 1280 by 6040, so their ratio is
the device scale factor and the scale is worked out with it, which gave one capture of 434 by 2048.
At scale 1 a page 40,000 pixels tall at a device scale factor of 2 was not returned as a JPEG, and
at the scale for 2,048 pixels it was. The picture's size is read from its JPEG header, which Brave
puts at byte 158 or byte 632 after a colour profile, so a header that does not end in the first
48 KiB is refused. If the long side were over 2,048 the picture is taken once more at the scale its
size gives.

**Tabs and frames.** A tab in the background (`document.visibilityState` was `hidden`) was brought
to the front by `Page.bringToFront` and captured. A page with a frame of another origin has the
frame in the picture in the box the page gives it: a frame 600 by 300 whose own document was 900
pixels tall shows 300 of it.

**Not observed.** A headed Brave with a window covered by another window or minimized; the
height of the debugging notice and what Cancel on it does to a capture; a screen whose device scale
factor the browser does not report in the layout metrics, which the second capture is for.

### Choosing a file

[BROWSER-45](../specs/browser.md#BROWSER-45) is the clause. `choose_file` is the one tool of
`upload-trusted`. The relay's `uploads.rs` looks at the file, `extension/files.js` and its copy hold the
limits and the list of credential names, `extension-trusted/resolve.js` finds the file input, and `cdp.js`
sends `DOM.setFileInputFiles`.

**Who reads the file.** The browser does. `DOM.setFileInputFiles` takes the path of a file and sets the
files of an input to it, and the browser opens the file as it would for the file picker. No byte of the
file passes through native messaging, the relay, the BraveBot extension, the second extension or BraveBot,
so the 1 MB a message to the extension may hold ([BROWSER-8](../specs/browser.md#BROWSER-8)) is not a
limit on the file, and a data-transfer built in a page from bytes sent to the extension, which was the
first design for this tool, is not needed. The method is the one on the list of BROWSER-37 that takes a
path, so it has a class of its own: `command()` sends it only with the person's token, one path that
`files.js` accepts and a node number.

**Which files.** Neither extension can look at the disk, and the planner is not to be trusted with a
path to anything. The relay's server is confined to the directory its declaration names with `--dir`,
which is the directory that holds the socket and the secret ([BROWSER-2](../specs/browser.md#BROWSER-2)).
A file has to be inside `uploads` in it. The declared directory is not used as it is because it holds the
secret, and the person makes `uploads` and puts the files there, which says which files the model may
offer. `uploads` has to be a directory and not a link, so that the directory granted is the one named in
the declaration. The alternatives are the whole of `--dir`, with the relay's own four files refused by
name, and a second word on the server's command line naming a directory, which would let a person offer
`~/Documents`; both widen what a declaration grants, and either is a change to one constant and the
README.

**The relay reads an argument.** The relay's rule is that it decides nothing about what it relays
(`lib.rs`, [layering.md](../specs/layering.md)). Looking at a path is a decision about a request, and the
only place that can take it before the person is asked is a program that sees the path and the disk. The
extensions see neither. The decision is taken by the confined half only, from the planner's own word and
the facts of a directory the person declared, with a closed list of five sentences and no byte of a page or
a result. It is the second named exception in the layering row, after the picture. The host half, which
Brave starts unconfined, is unchanged.

**Finding the input.** A file input is a `button` in the accessibility tree whatever it is called, with its
`<label>`, its `aria-label` or the browser's "Choose File" as the name. The resolver therefore takes the inputs
of the document by tag and type, and the role in a call is optional. Many sites hide the input and draw a
label or a button that opens it, so an input the page hides is found too.

What the protocol gives for an input the page hides, observed in Brave 1.98.63 (Chromium 156.0.8078.80,
macOS, headless, a page with an input hidden in each way):

| The page hides the input with | `Accessibility.getFullAXTree` | `Accessibility.getPartialAXTree` for its node | `DOM.getBoxModel` | `DOM.getFlattenedDocument` |
|---|---|---|---|---|
| `display: none`, the `hidden` attribute | not there | one node, role `none`, ignored as `notRendered`, no name | fails, "Could not compute box model." | there, with its attributes |
| `visibility: hidden` | not there | the same, ignored as `notVisible` | 253 by 21 | there |
| `opacity: 0` and no size | there, a `button` named by its label | the same node | 0 by 0 | there |
| a clip to one pixel | there, a `button` named by its label | the same node | 1 by 1 | there |

A displayed label is in the tree as `LabelText` with its words as `StaticText` children, and
`Accessibility.queryAXTree` with a name finds those words and never the input. `DOM.describeNode` and
`DOM.getAttributes` give an input's attributes and no name, and `DOM.querySelectorAll` gives node numbers. No
method of the protocol names an input the tree leaves out, so none is added to the list of BROWSER-37. The
input is named from the document the resolver already reads: the tree names the input wherever it holds it,
which is the browser's own computation, and the markup names it where it does not.

**Which names.** In the order the browser takes them, the elements `aria-labelledby` points to, `aria-label`,
the labels (a `label` whose `for` is the input's id, and the `label` around it that has no `for` and holds it
as the first element a label can name, each within its own document or shadow root) and `title`; the first with
words is the name. The words of a label are the `StaticText` the tree holds in it, so they are the words the
person sees, and a label the browser leaves out names nothing. An input none of them names has its `name`
attribute and its `id`, either of which names it. That is the weakest name, a word for a program, and the
window says so. This is a simplification of the browser's computation. Where a `label` is itself hidden or an
`aria-labelledby` points to an element the tree leaves out, the input gets the next name or none, and the
window shows the name it got. An input the tree holds and the page hides (no opacity, no size, a clip) is
named by the tree, not by the markup, because it is the browser's word.

**Which input.** The displayed inputs with the name are counted first, as the other tools count only the
displayed elements, so a page that hides a second input with the same label for a small screen still has the
one the person sees. Where none is displayed, the hidden ones with the name are counted, and one is the input
and two are several. An input the page hides is not scrolled to and has no middle to reach, since
`DOM.setFileInputFiles` takes the node, and is held to the rules of a disabled input (its own attribute, a
`fieldset` above it or the tree) and of a payment card field. It is searched for only in the top document, as
every element is.

**What the window says.** The action is one of three, as the page shows the input, hides it and names it with a
label, or hides it and names it only by its id or name (`upload`, `upload_hidden`, `upload_hidden_id` in
`actions.js`), so the words are the window's own and a page cannot write them. The second says that the page
hides the input and that the name is what the page's own label, `aria-label` or `title` calls it, and the third
that the page gives the input no label and the name is its id or name in the markup. The rows still show the
tag, the role `button`, the name and where the input's form goes. The resolution carries how the page names the
input in its fingerprint, so an input the page shows, renames or moves while the person decides is not chosen
on.

**Observed.** With the second extension loaded in that Brave, the real window answered with protocol mouse
events and a page that sends the file on `change`: each of nine inputs hidden in the ways above received the
file, its bytes equal to the file's, with an event whose `isTrusted` was true, and the window showed the third
action for the input named by its name attribute and the second for the others. A name two inputs had was
refused, and a name only a button that opens the input by script had was not found. Not observed: a headed
Brave, another system than macOS, and the sites that hide their inputs in the ways of a framework, such as a
`div` that wraps a drop zone with no `label`, which are not named by their markup and are found only by an id
or a name.

**What the page gets.** The `change` event runs as if the person had chosen the file; the page read the
file's name, size and bytes in the real Brave runs, and a page that sends the file on `change` sent it at
once, which is why the window says so. A page that opens a dialog on `change` is answered in the panel of
the second extension.

### Reading a page's console

[BROWSER-44](../specs/browser.md#BROWSER-44) is the clause. `extension-trusted/consoles.js` holds
the tool and `cdp.js` the two methods it adds to the list, `Console.enable` and `Log.enable`, which
are sent with the token of the person's Allow as input is.

**Why the Console domain.** The browser reports a page's console in three places. `Log.entryAdded`
carries what the browser itself reports about a page: a request that failed, a rule of the page that
was broken, an API that is going away. It carries no call of `console.log` and no error a script
threw. `Runtime.consoleAPICalled` and `Runtime.exceptionThrown` carry both, with the arguments and the
stack, and need `Runtime.enable`, a method of the domain that can evaluate code, which the list does not
hold ([BROWSER-37](../specs/browser.md#BROWSER-37)). The Console domain, which the protocol marks as
deprecated, is the third: `Console.enable` has the browser send the messages it collected so far as
`Console.messageAdded` events, and then the new ones, each with a source, a level, a text, the address
of the script and a line. It evaluates nothing. The tool enables the Console and Log domains and
collects both, in the order the browser reports them.

**What the Console domain does not carry.** An error a script throws and nothing catches is a message
with the text `Uncaught` at the script's line, and its level is `log`: the error's own message and its
stack are in `Runtime.exceptionThrown` alone. An error given to `console.error` is a message with the
error's first line. Arguments that are objects are `[object Object]`, and the stack of
`console.trace` is not included. A frame that Brave runs in another process has a session of its own,
which the second extension does not attach to, so its console is not read. These are Known costs of
BROWSER-44, and `Runtime.enable` is the change that would remove the first three. It is for a
maintainer to decide, since it puts a method of `Runtime` on the list.

**The call.** The tab is found by its address without attaching ([BROWSER-13](../specs/browser.md#BROWSER-13)).
The window is asked first, with the origin, the address and what is read, and shows no element: the
row of the console in the `actions.js` of the second extension says it acts on none, so the window
leaves out the heading, the rows and the note of the element and does not count the characters of
what is read, and its words say that the browser's debugger is attached to the page and reads the
page's console. In Brave 1.98.62, headless, with a viewport of 568 pixels, Allow once and Deny stayed
in view in the bar at the foot of the window: at 332 to 366 for a short address, at 434 to 468 for
the longest origin and address, and at 522 to 556 at the top of the page, 510 to 544 at its end,
where the text of what is read was 2,000 characters long. Nothing is attached before the person
allows it. After the Allow the session attaches, sends `Page.enable`,
`Console.enable` and `Log.enable`, waits 500 milliseconds after the second answer for what comes with the
replay, reads the address of the page again and detaches, however the call ends. The whole of it is
bounded at 10 seconds, inside the 80 the first extension waits.

**The result.** The second extension keeps the entries of the levels asked for, the newest `max_entries`,
and counts the older ones it left out. Each is cut to a level and a source of closed lists, a text of at
most 500 characters, the script's address of at most 300 and a line. The reply is the report, which the
first extension checks again before it passes anything on. The text it returns is one JSON object on
each line, with every character a person cannot see written as the `\u` escape of JSON, so a
character of the page cannot end a line, forge another or change how a line is drawn.

**Observed in Brave 1.98.62**, headless with a scratch profile, both extensions loaded unpacked beside
pages on 127.0.0.1 (the harness appends probe lines to copies of the two background scripts, not the
repository files), the real window allowed by a click made through the protocol, and the browser
resolving every name but 127.0.0.1 to nothing:

- `Log.enable` replayed three entries for a page that called `console.log`, `console.info`,
  `console.debug`, `console.warn`, `console.error`, `console.assert`, `console.trace` and
  `console.table`, threw an error from a timer, rejected a promise and asked for two missing files:
  the three 404s, with the source `network`, and no call of `console` and no thrown error. A call made
  after `Log.enable` was not reported either. `Console.enable` replayed twelve messages for the same
  page, each call of `console` with the source `console-api` and the level `log`, `info`, `debug`,
  `warning` or `error`, a text, the script's address and a line counted from 1, and a call made after
  it was reported at once. `console.assert(false, "x")` was an `error` and `console.trace("x")` a
  `log`, both with the text `x`.
- A thrown error and a rejected promise were messages with the text `Uncaught` and
  `Uncaught (in promise)`, the level `log`, the source `console-api` and the line of the throw.
  `console.error(new TypeError("x"))` was `TypeError: x`, and an object logged was `[object Object]`.
- A page whose Content-Security-Policy blocked an image, an inline script and an inline handler gave
  entries from `Log.enable` with the source `security`, the level `error`, the policy's sentence and the
  page's line. A request for a missing file gave a `network` entry with the level `error`, the file's
  address and the line 0.
- A read of that page, allowed in the window, sent no debugger call before the answer: the log of the
  second extension held `ATTACH`, `Page.enable`, `Console.enable`, `Log.enable`, `Page.getFrameTree` and
  `DETACH`, in that order, after it. A denial left the log empty, and the debugger could be attached to
  the tab afterwards by another client, as it could after an allowed read. It took 2.1 seconds with the
  1.4 seconds the harness waited for Allow once to be available.
- A page that wrote a warning every 60 milliseconds gave the entries from before the read and those that
  came in the half second after it. A page with 250 entries and 10 hostile ones gave the newest 200 and
  `omitted` 60. The hostile ones, a right-to-left override, a line break, a forged line of JSON, an
  escape sequence of a terminal, a bell, a null, a zero-width space, a soft hyphen and a private use
  character, came out as `\u` escapes inside the strings of 201 lines for 200 entries, with no
  character a person cannot see in the text.
- A frame of the same site was read with the page. A frame of another site, which Brave ran as a
  target of its own, was not: its message did not appear.

Not seen, because headless has no window system: a person's click, the debugging notice and what
Cancel on it does while a read runs, and a headed window's size for this action.

### What a person holds out

A person who selects text, or points at a link or a page, can hold it out for a session
([BROWSER-39](../specs/browser.md#BROWSER-39)). While the `held` group is on, the extension adds three
entries to the browser's context menu, **Hold this selection for BraveBot**, **Hold this link for
BraveBot** and **Hold this tab for BraveBot**. Using one makes an item of what the browser gives the
extension for it: the selected text, the address of the link, or the address of the page, with the
address of the page the entry was used on. The session fetches the items with `take_held`, which
takes no argument and forgets what it returns.

Designs considered:

| Design | The gesture vouches for the bytes | Verdict |
|---|---|---|
| The extension pushes the selection into the session's box | yes | Not built. The bytes come from a page, so a person's gesture would give them a better label than their source has, and the driver cannot see the gesture ([DROP-10](../specs/dropping.md#DROP-10)). Nothing in a running session listens for the browser: the host writes only replies to requests it forwarded ([BROWSER-7](../specs/browser.md#BROWSER-7)), and a channel into a session that a program of the same user can name is one the project avoids ([RPCVIEW-8](../specs/session-view.md#RPCVIEW-8)) |
| A named channel, such as an inbox in the state directory, that delivers the item as untrusted | no | Not built. It adds an inbound surface to every client and gives the item no better label than a tool result has |
| The extension keeps the items and `take_held` returns them | no | Built. It adds one table entry to the relay and one method to the extension, and the item reaches every client as a reference |
| A prompt the person runs, with the items as its text ([MCP-11](../specs/mcp.md#MCP-11)) | no | Not built. The relay would have to answer `prompts/list` and `prompts/get`, and the tool is enough |
| The clipboard | yes, as a paste | Exists with no code. A paste is the person's own text ([pasting](../specs/pasting.md)) |

**What the gesture means.** It says that an item exists and which part of the page the person meant.
It does not say the bytes can be trusted. A selected range includes text the page hides, at zero
opacity, off screen or in the colour of its background, and the address of a link or of a page is
chosen by the page. So an item has the label of any result of a server, untrusted and private
([MCP-1](../specs/mcp.md#MCP-1)), the planner is given a reference to it, and a model reads it only
through a processor ([PROC-1](../specs/processors.md#PROC-1)) or after `vet_content` has been answered for
it. The row for it in [LABEL-8](../specs/labels.md#LABEL-8) records that the decision
is that the gesture gives it no label. The reading at `vet_content` is the only check on text
they did not see.

**The permission.** The browser hands the extension the selection when the entry is used, so the group
lists `contextMenus` and no host, and the extension has no script in any page. An element picker that
outlines any element, which is how the other agents let a person point at one, would need `scripting`
and the hosts, a function injected in the page and a rule for which clicks it accepts. A link is the one
element the browser names for an extension, by its address, so it is the one held. A tab is held as its
address, since its title would need `tabs`.

**What is kept.** The items are in `chrome.storage.session`, which the browser holds in memory, never
writes to disk, and empties when it restarts and when the extension is reloaded or updated. The popup and
the options page run the code that turns a group off, in their own contexts, and share that storage with
the service worker, so every way a group ends can forget the items. A variable in the worker would be
out of their reach. Ten items of 20,000 characters take about 323 KB of its 10 MiB.

**Checked against Brave 1.98.62** in a headless run on 2026-10-31 (a throwaway profile, the debugging
port on 127.0.0.1 only, no site loaded):
- `contextMenus` is accepted in `optional_permissions`. A fresh install holds it not and has no
  `chrome.contextMenus`. `chrome.permissions.request` for it resolved true with no prompt, and
  `management.getPermissionWarningsByManifest` returns no warning for it.
- After the grant `chrome.contextMenus.create`, `removeAll` and `onClicked` exist in the service worker
  and the options page. `removeAll` returns a promise; the callback form works. A second `create` with an
  id already made reported no error, so the extension removes every entry before it makes them.
- After `chrome.permissions.remove`, `chrome.contextMenus` is still an object and each call throws that
  it is not available in this context, so the extension removes the entries first, as it releases a
  keep-awake request before `power` goes.
- Holding 12 selections of 30,000 characters kept 10 of them cut at 19,999 characters, the last of which
  would have split a character, and counted 4 as forgotten. Taking them emptied the store, an item
  30 minutes old was not returned, and `take_held` through `handle` left out an item held at a blocked
  site.

**Not observed.** A click on a context menu entry, since no menu can be opened in a headless Brave:
whether the browser cuts a long selection before it gives it to the extension, what `pageUrl` holds
in a frame and in a private window, and how the entries look. The permission prompt of a headed Brave
for `contextMenus` was not seen, though no warning is listed for it.

## Chrome API namespaces

The Chrome extension API reference lists 85 namespaces, as read on 2026-10-30. Each is a row of
exactly one of the three tables below: the namespaces the tools of a group call, the namespaces no
session reaches, and the namespaces the extension uses for itself. A namespace that several groups
call is one row naming each group. The page of a namespace is
`https://developer.chrome.com/docs/extensions/reference/api/<namespace>`, with a dot in the name
written as a slash, and each name below links to it.

`extension/tests/namespaces.test.mjs` reads the tables. It holds that every namespace of the
reference is in one row, that the groups named are those of `extension/groups.js`, that each
permission a group lists is on the row of a namespace that names the group, that nothing the
manifest declares or the code calls is in the second table, and that a namespace marked reserved
is neither called nor declared.

### Namespaces the groups use

The groups are those of `extension/groups.js`. A group lists a permission there once one of its
tools calls the namespace, so a row can name a group before the group's line lists the permission.
Hosts are not namespaces and have no row; `extension/groups.js` records which groups read pages.
The Permission column is the optional permission that gives the group the namespace, or none. The
row of `debugger` names the groups whose tools the second extension does with it. This extension
never declares it, and it cannot be optional (below).

| Namespace | Groups | Permission |
|---|---|---|
| [`tabs`] | `tabs-read`, `page-read`, `tabs-manage`, `navigate`, `downloads`, `remove-data`, `input`, `screen` | `tabs` |
| [`tabGroups`] | `tabs-read`, `tabs-manage` | `tabGroups` |
| [`webNavigation`] | `tabs-read`, `page-read`, `navigate` | `webNavigation` |
| [`sessions`] | `tabs-read`, `history-read`, `navigate` | `sessions` |
| [`windows`] | `tabs-read`, `page-read`, `tabs-manage`, `navigate`, `remove-data`, `input`, `screen` | none |
| [`history`] | `history-read`, `remove-data` | `history` |
| [`topSites`] | `history-read` | `topSites` |
| [`bookmarks`] | `bookmarks-read`, `bookmarks-write`, `remove-data` | `bookmarks` |
| [`readingList`] | `bookmarks-read`, `bookmarks-write`, `remove-data` | `readingList` |
| [`scripting`] | `page-read`, `input` | `scripting` |
| [`pageCapture`] | `downloads` | `pageCapture` |
| [`dom`] | `page-read`, `input` | none |
| [`search`] | `navigate` | `search` |
| [`downloads`] | `downloads`, `remove-data` | `downloads` |
| [`cookies`] | `cookies` | `cookies` |
| [`contextMenus`] | `held` | `contextMenus` |
| [`webRequest`] | `network-watch` | `webRequest` |
| [`privacy`] | `browser-info` | `privacy` |
| [`contentSettings`] | `browser-info` | `contentSettings` |
| [`fontSettings`] | `browser-info` | `fontSettings` |
| [`system.cpu`] | `browser-info` | `system.cpu` |
| [`system.display`] | `browser-info` | `system.display` |
| [`system.memory`] | `browser-info` | `system.memory` |
| [`system.storage`] | `browser-info` | `system.storage` |
| [`idle`] | `browser-info` | `idle` |
| [`power`] | `browser-info` | `power` |
| [`extension`] | `browser-info` | none |
| [`browsingData`] | `remove-data` | `browsingData` |
| [`userScripts`] | `scripts` | `userScripts` |
| [`debugger`] | `input-trusted`, `capture-trusted`, `console-trusted`, `upload-trusted` | `debugger` |
| [`desktopCapture`] | `screen` | `desktopCapture` |
| [`tabCapture`] | `screen` | `tabCapture` |

`windows`, `dom` and `extension` need no permission. The tools that list, focus or resize a window
call `windows`, closed shadow roots are opened with `dom.openOrClosedShadowRoot`, and
`get_access_flags` reads `extension.isAllowedFileSchemeAccess` and `isAllowedIncognitoAccess`.
`userScripts` also needs the person to turn on "Allow User Scripts" on the extension's details page
(Chrome 138 and later; Developer mode before), which an extension cannot do. `run_script` in `scripts`
lists `userScripts` and the manifest offers it as an optional permission. `tabCapture` has a row for a
group whose tool is not built, `capture_tab_video` in `screen`. That group does not list the
permission, and the manifest does not declare it. `input` acts through functions the
extension injects with `scripting`, and not through `debugger`. `debugger` cannot be optional, so the
second extension holds it, as a required permission, and this extension never declares it. The four
groups on the row call it in `extension-trusted/cdp.js`, and none of them lists it in `groups.js`. The
protocol domains `Accessibility`, `CSS`, `Console`, `DOM`, `Input`, `Log` and `Page` it uses belong to
`debugger` and are not namespaces.

Nine permissions cannot be specified as optional: `debugger`, `declarativeNetRequest`, `devtools`,
`geolocation`, `mdns`, `proxy`, `tts`, `ttsEngine` and `wallpaper`. This extension declares none of
them, required or optional, and no group lists one; `extension/tests/groups.test.mjs` fails if the
manifest or a group does. Of the nine only `debugger` serves a group, and the second extension holds it. `input` lists `tabs`
and `scripting` and not `debugger`. The `scripts` group lists `userScripts`, which the manifest offers as an
optional permission, and the options page tells the person how to turn on "Allow User Scripts"
([BROWSER-46](../specs/browser.md#BROWSER-46)).

### Not exposed to a session, and why

No group lists a namespace below, no tool is named for one, and the manifest declares none. The
reasons are those of the proposal that introduced the groups (brave/bravebot#3583). The row for
`webRequestBlocking` is for a permission of `webRequest` that the reference does not list as a
namespace, so it is not one of the 85.

| Namespaces | Reason |
|---|---|
| [`management`] | Lists installed extensions with their permissions and can disable or remove them, including security extensions. No use to a browsing task |
| [`identity`] | Brave disables Google sign-in, so `getAuthToken` fails there. `launchWebAuthFlow` hands OAuth tokens to the extension |
| [`gcm`], [`instanceID`] | A remote server can push messages into the extension (an input channel nobody vouched for), and `instanceID` is a stable per-install identifier. Brave disables GCM by default |
| [`declarativeNetRequest`], [`proxy`] | Cannot be optional, so they would appear in every install prompt; they change the person's traffic and persist |
| [`webRequestBlocking`] | Policy-installed extensions only under Manifest V3 |
| [`declarativeContent`], [`devtools.inspectedWindow`], [`devtools.network`], [`devtools.panels`], [`devtools.performance`], [`devtools.recorder`] | Change the extension's own UI, or need DevTools open on the tab. `debugger` covers the same ground without that |
| [`ttsEngine`] | Receives all text any extension speaks; its warning is "Read all text spoken using synthesized speech." |
| [`webAuthenticationProxy`] | Intercepts passkey requests; designed for remote desktop software |
| [`mimeHandler`], [`printerProvider`] | Replace the PDF viewer, or serve print jobs |
| [`dns`], [`processes`] | Chrome Dev channel only |
| [`enterprise.deviceAttributes`], [`enterprise.hardwarePlatform`], [`enterprise.networkingAttributes`], [`enterprise.platformKeys`], [`enterprise.login`] | Policy-installed extensions only |
| [`accessibilityFeatures`] | Every setting but `animationPolicy` is ChromeOS only |
| [`audio`], [`certificateProvider`], [`documentScan`], [`fileBrowserHandler`], [`fileSystemProvider`], [`input.ime`], [`loginState`], [`platformKeys`], [`printing`], [`printingMetrics`], [`systemLog`], [`vpnProvider`], [`wallpaper`] | ChromeOS only |

### Used by the extension itself

These are not offered as tools. A namespace is `used` when the extension's code calls it, or for
`events` when the code adds a listener, and `reserved` when nothing calls or declares it. A reserved
namespace becomes used in the same change that first calls it.

| Namespaces | State | Use |
|---|---|---|
| [`storage`] | used | The tool switches, group switches, lifetimes, site lists and the activity log, in `storage.local`, the groups that last until Brave restarts and the items a person holds out ([BROWSER-39](../specs/browser.md#BROWSER-39)), in `storage.session`. The manifest requires the permission. The functions that read the switches, lifetimes and scopes and write them back take the lock `bravebot-switches` of `navigator.locks`, a web platform API that needs no permission, so that the options page, the popup and the service worker, which each load their own copy of the code, write one at a time. In Brave 1.98.62 `navigator.locks.request` exists in the service worker, the options page and the popup. A lock held in the service worker for 600 ms made a `setTool` call in the options page wait about 500 ms. Over 120 rounds of `setTool` calls from the popup and the options page at the same time, two runs with the lock lost no write, and a run without it lost writes in 5 rounds seen from the popup and 9 seen from the options page (headless run, 2026-10-30) |
| [`alarms`] | used | The alarm that reconnects to the native host and the one that turns off a group whose lifetime has passed. The manifest requires the permission |
| [`runtime`] | used | `connectNative` opens the port to the native host and needs the `nativeMessaging` permission, `getPlatformInfo` answers `get_platform_info`, `sendMessage` sends a call of `input-trusted`, `capture-trusted`, `console-trusted` or `upload-trusted` to the second extension by its id and needs no permission, `getURL`, `id`, `onMessage` and, in the capture window, `lastError` and `sendMessage` without an id serve the capture window of `screen`, and `onStartup` and `onInstalled` start the connection |
| [`permissions`] | used | The control model: `request`, `contains`, `remove`, `onAdded` and `onRemoved` |
| [`events`] | used | The type of every event the extension listens to, such as `alarms.onAlarm`. It has no call of its own |
| [`types`], [`extensionTypes`] | reserved | Type declarations other namespaces use. They have no call of their own |
| [`offscreen`] | reserved | Not used by `screen`: an offscreen document has `chrome.runtime` only, and a stream id from `chooseDesktopMedia` is refused in it ([Taking a picture of the screen](#taking-a-picture-of-the-screen)) |
| [`i18n`] | reserved | The strings of the options page are literals in `extension/options.html` and `extension/options.js` |
| [`publicSuffix`] | reserved | Chrome 153 and later; it could match blocked sites by registrable domain. It needs its own `publicSuffix` permission |
| [`action`] | used | The toolbar badge, which counts the groups that are on and lists them in its title, and the popup, `extension/popup.html`, with **Turn everything off** |
| [`commands`] | used | The keyboard command `turn-everything-off`, which does what the popup button does. The manifest gives it no default key |
| [`omnibox`] | reserved | A person could send words to the session from the address bar. Nothing uses it |
| [`notifications`] | reserved | Tells the person that an approval is waiting |
| [`tts`] | reserved | The proposal lists it with no use. It cannot be optional, so the manifest would require it |
| [`sidePanel`] | reserved | The proposal also rules it out: Brave merged the side panel into its Sidebar, which has no control to open a side-panel extension (brave-browser #31334), and `sidePanel.open` from a popup window crashes the browser (#59256) |

### External messages

The extension declares no `externally_connectable` and no listener for `runtime.onMessageExternal`
or `runtime.onConnectExternal`, so no web page or other extension can send it a message. It sends
one to another extension: `trusted.js`, the only script that calls `runtime.sendMessage` with an
extension id, sends the calls of `input-trusted`, `capture-trusted`, `console-trusted` and `upload-trusted` to the BraveBot Trusted Input extension, which
takes a message from this extension alone ([BROWSER-12](../specs/browser.md#BROWSER-12),
[BROWSER-37](../specs/browser.md#BROWSER-37)). The capture window of `screen` calls
`runtime.sendMessage` with a message and no id, which reaches this extension's own worker
([BROWSER-42](../specs/browser.md#BROWSER-42)). A send needs no listener on this side. The
reference says that without `externally_connectable` all extensions can connect and no web page
can. `onMessageExternal` fires for a message from another extension, and `onConnectExternal` fires
for a connection from another extension or from an externally connectable web site. The extension
listens to neither, so a message from another extension has no receiver. The test fails if the
manifest gains `externally_connectable` or the code adds either listener.

### Evaluated: asking for one site when a session names it

`permissions.addHostAccessRequest` was evaluated as a way to ask for one site when a session names
it. The extension does not use it, and nothing in the design depends on it.

What Chrome's documentation states, read on 2026-10-30. The reference page gives the method as
Chrome 133, `addHostAccessRequest(request)`, with a `tabId` or a `documentId` (the top-level
document of a tab) and an optional `pattern` for the URLs the request can be shown at, and says that
an accepted request grants persistent access to the site's top origin. It gives no example and does
not say what the person sees. The Chrome extensions blog post that introduced it (2 January 2025,
"New extensions menu") says that when a person has withheld access to a page, an extension can ask
for it with this API, that the person then sees an "Allow" message beside the extension's puzzle
piece in the toolbar, that the API is enabled by default from Chrome 133.0.6860.0, and that the
menu feature needed the "Extensions Menu Access Control" flag in `chrome://flags`. The post links a
sample, `cookbook.permissions-addhostaccessrequest`.

What this means for the design. The documentation describes the API for a page whose access the
person withheld, and this extension declares no host at install; its hosts are optional host
permissions that the options page requests from a click. The documentation read does not say
whether a request applies to a host that is only optional. The API is called from the service
worker with no click, so a call that names a site could put a request in the toolbar for it, and
the person's answer would be given outside the options page, where the confirmation of
[BROWSER-19](../specs/browser.md#BROWSER-19) and the allowed sites of
[BROWSER-22](../specs/browser.md#BROWSER-22) are shown. Sites are added in the options page, as
before.

Not run: no browser was run for this evaluation, so it is not known whether Brave shows the
request, where, or whether the flag is needed in Brave stable. Nothing here has been tried against
an extension that declares only optional hosts. The pages were read as summaries, so the wording
above is a paraphrase of them.

### What was not verified

Nothing in this section ran in a Brave. The pages of the reference were read on 2026-10-30 for
these rows of the second table, and each agrees with the reason given: the thirteen ChromeOS-only
namespaces (each page says the API works only on ChromeOS), `dns` ("only available in Chrome Dev")
and `processes` (Dev channel), `accessibilityFeatures` (every property but `animationPolicy` is
marked ChromeOS only), the five `enterprise` namespaces (each page says the API is only for
extensions installed by a policy), `webAuthenticationProxy`, `printerProvider`, `mimeHandler`
(`application/pdf` is the only MIME type it takes, as of Chrome 151), `management` (lists, enables,
disables and uninstalls extensions), `declarativeContent` (shows or hides the toolbar action and
sets its icon), `webRequestBlocking` (the permission is no longer available to most extensions
under Manifest V3), and `ttsEngine`. The permissions page lists `debugger`, `declarativeNetRequest`,
`devtools`, `geolocation`, `mdns`, `proxy`, `tts`, `ttsEngine` and `wallpaper` as unable to be
optional, and gives `ttsEngine` the warning quoted above.

Not read on a Chrome page and taken from the proposal: that `management` can disable security
extensions, the statements about Brave for `identity`, `gcm` and `instanceID`, that `instanceID`
is a stable per-install identifier, the reasons for the `devtools` rows, and the two Brave issues
for `sidePanel`. Whether `gcm`, `instanceID`, `userScripts`, `debugger`, `offscreen`, `readingList`
and `search` work in current Brave stable is in [Checked against Brave](#checked-against-brave).

Where the proposal and the reference differ:

- The proposal places 84 of the 85 namespaces and one name that is a permission,
  `webRequestBlocking`. It does not place `windows`. This document places `windows` with the groups
  that call it: `list_windows`, `open_window`, `focus_window`, `resize_window` and `close_window`,
  `screenshot_page` and `record_gif` for the window that shows a tab, and the windows the extension
  opens to ask a person (`input`) or to record the screen (`screen`).
- The proposal lists `sidePanel` as used by the extension and also says the side panel is not used.
  It is reserved here, with the proposal's reasons.
- The proposal lists `tts` among the namespaces the extension uses and gives no use. The permissions
  page says `tts` cannot be optional.
- `publicSuffix` and `sidePanel` each need a permission of their own, which the proposal does not
  mention. Neither is declared today.
- `page-read` lists `tabs` in `extension/groups.js` because `read_page` calls `tabs.query`, and the
  proposal's row for it does not. The table above follows the code.

[`accessibilityFeatures`]: https://developer.chrome.com/docs/extensions/reference/api/accessibilityFeatures
[`action`]: https://developer.chrome.com/docs/extensions/reference/api/action
[`alarms`]: https://developer.chrome.com/docs/extensions/reference/api/alarms
[`audio`]: https://developer.chrome.com/docs/extensions/reference/api/audio
[`bookmarks`]: https://developer.chrome.com/docs/extensions/reference/api/bookmarks
[`browsingData`]: https://developer.chrome.com/docs/extensions/reference/api/browsingData
[`certificateProvider`]: https://developer.chrome.com/docs/extensions/reference/api/certificateProvider
[`commands`]: https://developer.chrome.com/docs/extensions/reference/api/commands
[`contentSettings`]: https://developer.chrome.com/docs/extensions/reference/api/contentSettings
[`contextMenus`]: https://developer.chrome.com/docs/extensions/reference/api/contextMenus
[`cookies`]: https://developer.chrome.com/docs/extensions/reference/api/cookies
[`debugger`]: https://developer.chrome.com/docs/extensions/reference/api/debugger
[`declarativeContent`]: https://developer.chrome.com/docs/extensions/reference/api/declarativeContent
[`declarativeNetRequest`]: https://developer.chrome.com/docs/extensions/reference/api/declarativeNetRequest
[`desktopCapture`]: https://developer.chrome.com/docs/extensions/reference/api/desktopCapture
[chrome-desktopcapture]: https://developer.chrome.com/docs/extensions/reference/api/desktopCapture
[`devtools.inspectedWindow`]: https://developer.chrome.com/docs/extensions/reference/api/devtools/inspectedWindow
[`devtools.network`]: https://developer.chrome.com/docs/extensions/reference/api/devtools/network
[`devtools.panels`]: https://developer.chrome.com/docs/extensions/reference/api/devtools/panels
[`devtools.performance`]: https://developer.chrome.com/docs/extensions/reference/api/devtools/performance
[`devtools.recorder`]: https://developer.chrome.com/docs/extensions/reference/api/devtools/recorder
[`dns`]: https://developer.chrome.com/docs/extensions/reference/api/dns
[`documentScan`]: https://developer.chrome.com/docs/extensions/reference/api/documentScan
[`dom`]: https://developer.chrome.com/docs/extensions/reference/api/dom
[`downloads`]: https://developer.chrome.com/docs/extensions/reference/api/downloads
[`enterprise.deviceAttributes`]: https://developer.chrome.com/docs/extensions/reference/api/enterprise/deviceAttributes
[`enterprise.hardwarePlatform`]: https://developer.chrome.com/docs/extensions/reference/api/enterprise/hardwarePlatform
[`enterprise.login`]: https://developer.chrome.com/docs/extensions/reference/api/enterprise/login
[`enterprise.networkingAttributes`]: https://developer.chrome.com/docs/extensions/reference/api/enterprise/networkingAttributes
[`enterprise.platformKeys`]: https://developer.chrome.com/docs/extensions/reference/api/enterprise/platformKeys
[`events`]: https://developer.chrome.com/docs/extensions/reference/api/events
[`extension`]: https://developer.chrome.com/docs/extensions/reference/api/extension
[`extensionTypes`]: https://developer.chrome.com/docs/extensions/reference/api/extensionTypes
[`fileBrowserHandler`]: https://developer.chrome.com/docs/extensions/reference/api/fileBrowserHandler
[`fileSystemProvider`]: https://developer.chrome.com/docs/extensions/reference/api/fileSystemProvider
[`fontSettings`]: https://developer.chrome.com/docs/extensions/reference/api/fontSettings
[`gcm`]: https://developer.chrome.com/docs/extensions/reference/api/gcm
[`history`]: https://developer.chrome.com/docs/extensions/reference/api/history
[`i18n`]: https://developer.chrome.com/docs/extensions/reference/api/i18n
[`identity`]: https://developer.chrome.com/docs/extensions/reference/api/identity
[`idle`]: https://developer.chrome.com/docs/extensions/reference/api/idle
[`input.ime`]: https://developer.chrome.com/docs/extensions/reference/api/input/ime
[`instanceID`]: https://developer.chrome.com/docs/extensions/reference/api/instanceID
[`loginState`]: https://developer.chrome.com/docs/extensions/reference/api/loginState
[`management`]: https://developer.chrome.com/docs/extensions/reference/api/management
[`mimeHandler`]: https://developer.chrome.com/docs/extensions/reference/api/mimeHandler
[`notifications`]: https://developer.chrome.com/docs/extensions/reference/api/notifications
[`offscreen`]: https://developer.chrome.com/docs/extensions/reference/api/offscreen
[`omnibox`]: https://developer.chrome.com/docs/extensions/reference/api/omnibox
[`pageCapture`]: https://developer.chrome.com/docs/extensions/reference/api/pageCapture
[`permissions`]: https://developer.chrome.com/docs/extensions/reference/api/permissions
[`platformKeys`]: https://developer.chrome.com/docs/extensions/reference/api/platformKeys
[`power`]: https://developer.chrome.com/docs/extensions/reference/api/power
[`printerProvider`]: https://developer.chrome.com/docs/extensions/reference/api/printerProvider
[`printing`]: https://developer.chrome.com/docs/extensions/reference/api/printing
[`printingMetrics`]: https://developer.chrome.com/docs/extensions/reference/api/printingMetrics
[`privacy`]: https://developer.chrome.com/docs/extensions/reference/api/privacy
[`processes`]: https://developer.chrome.com/docs/extensions/reference/api/processes
[`proxy`]: https://developer.chrome.com/docs/extensions/reference/api/proxy
[`publicSuffix`]: https://developer.chrome.com/docs/extensions/reference/api/publicSuffix
[`readingList`]: https://developer.chrome.com/docs/extensions/reference/api/readingList
[`runtime`]: https://developer.chrome.com/docs/extensions/reference/api/runtime
[`scripting`]: https://developer.chrome.com/docs/extensions/reference/api/scripting
[`search`]: https://developer.chrome.com/docs/extensions/reference/api/search
[`sessions`]: https://developer.chrome.com/docs/extensions/reference/api/sessions
[`sidePanel`]: https://developer.chrome.com/docs/extensions/reference/api/sidePanel
[`storage`]: https://developer.chrome.com/docs/extensions/reference/api/storage
[`system.cpu`]: https://developer.chrome.com/docs/extensions/reference/api/system/cpu
[`system.display`]: https://developer.chrome.com/docs/extensions/reference/api/system/display
[`system.memory`]: https://developer.chrome.com/docs/extensions/reference/api/system/memory
[`system.storage`]: https://developer.chrome.com/docs/extensions/reference/api/system/storage
[`systemLog`]: https://developer.chrome.com/docs/extensions/reference/api/systemLog
[`tabCapture`]: https://developer.chrome.com/docs/extensions/reference/api/tabCapture
[`tabGroups`]: https://developer.chrome.com/docs/extensions/reference/api/tabGroups
[`tabs`]: https://developer.chrome.com/docs/extensions/reference/api/tabs
[`topSites`]: https://developer.chrome.com/docs/extensions/reference/api/topSites
[`tts`]: https://developer.chrome.com/docs/extensions/reference/api/tts
[`ttsEngine`]: https://developer.chrome.com/docs/extensions/reference/api/ttsEngine
[`types`]: https://developer.chrome.com/docs/extensions/reference/api/types
[`userScripts`]: https://developer.chrome.com/docs/extensions/reference/api/userScripts
[`vpnProvider`]: https://developer.chrome.com/docs/extensions/reference/api/vpnProvider
[`wallpaper`]: https://developer.chrome.com/docs/extensions/reference/api/wallpaper
[`webAuthenticationProxy`]: https://developer.chrome.com/docs/extensions/reference/api/webAuthenticationProxy
[`webNavigation`]: https://developer.chrome.com/docs/extensions/reference/api/webNavigation
[`webRequest`]: https://developer.chrome.com/docs/extensions/reference/api/webRequest
[`webRequestBlocking`]: https://developer.chrome.com/docs/extensions/reference/api/webRequest
[`windows`]: https://developer.chrome.com/docs/extensions/reference/api/windows

## Checked against Brave

The proposal for the capability groups states several facts about Chrome and Brave that this
document and the extension depend on. They were checked against public sources and against a running
Brave on 2026-10-30. This section says which statement comes from a source, which was observed in
the run, and which was not verified. A quoted sentence is as the page read on that date.

### The run

Brave Browser 1.98.62 (stable, Chromium 156.0.8078.74, macOS on Apple silicon) ran headless with a
profile made for the run and removed afterwards. This Brave honours `--load-extension`, and the
extension in `extension/` loaded with the id `fcjamhpiedeihbbbpndcjngkjaogfgep`. The debugging port
listened on 127.0.0.1 only, name resolution for every host except 127.0.0.1 was switched off, and
the one web page loaded was served from 127.0.0.1, besides `about:blank`, extension pages and
`chrome://extensions`. Commands went through the debugging port to the extension's service worker
and its pages. Throwaway extensions outside this repository stood in for APIs this extension does
not call, each requiring the permissions it tested. `chrome://extensions` supplied the install
warnings of an extension and the "Allow User Scripts" setting.

The run could not answer a permission prompt, show an infobar, or load a site. A user gesture for
`permissions.request` came from the debugging protocol (`userGesture` on `Runtime.evaluate`) and not
from a click.

### What a fresh install holds, and what an API of a group that is off does

- `permissions.getAll()` listed `alarms`, `nativeMessaging` and `storage` and no host.
  `permissions.contains` was false for each of the 19 entries of `optional_permissions` and for the
  two host patterns.
- In the service worker, `chrome.tabs`, `windows`, `storage`, `alarms`, `runtime`, `permissions` and
  `management` exist. `chrome.history`, `webNavigation`, `topSites`, `sessions`, `tabGroups`,
  `bookmarks`, `readingList`, `webRequest`, `privacy`, `contentSettings`, `fontSettings`, `system`,
  `idle`, `power` and `scripting` are `undefined`, so a call to one is a `TypeError` from the page
  and not an error the browser words. In the options page `chrome.scripting` and `chrome.history`
  were `undefined` too.
- `permissions.request` without a user gesture rejects with "This function must be called during a
  user gesture". With one, `scripting`, which shows no warning, resolved true with no prompt;
  `chrome.scripting` then existed in the service worker without a reload, `contains` was true and
  `getAll` listed it.
- `permissions.remove` resolved true, `contains` was false and `getAll` no longer listed
  `scripting`. In the page that had held it, `chrome.scripting` stayed an object and `executeScript`
  rejected with "'scripting.executeScript' is not available in this context." A second `request` of
  `scripting` resolved true again. Removing `storage` rejected with "You cannot remove required
  permissions."
- A request for `history`, which warns, and one for `https://*/*` neither resolved nor rejected for
  10 and 6 seconds. No grant of a permission that warns was observed.
- With the stored switches set for `tabs-read` and `list_tabs` and no permission held, `handle`
  answered "list_tabs needs the tabs-read group's browser permissions, which are not granted in the
  BraveBot extension's options". With the group off it answered "list_tabs is turned off in the
  BraveBot extension's options", and `get_platform_info` answered.

The Chrome reference for `permissions` says that after a permission is removed, a later `request`
"usually adds the permission back without prompting the user" ([permissions][chrome-permissions],
updated 2026-10-29). Chromium skips the prompt when the permissions requested add no warning
(`extensions/browser/api/permissions/permissions_api.cc`, "We don't need to show the prompt if there
are no new warnings"), and subtracts what the extension was already granted at runtime. So a person
who turns a group on again after turning it off may see no prompt from Brave. That was not observed
for a permission that warns.

### The eight APIs of the proposal

The Run column says what a call did, in a profile with no account, with the API's permission
required by a throwaway extension.

| API | Public source | Run |
|---|---|---|
| `gcm` | Brave's wiki lists "Google Cloud Messaging is disabled" and "Firebase Cloud Messaging is disabled". brave-core registers `brave.gcm.channel_status` with default false and calls `gcm_driver->SetEnabled` with it. brave-core's test filter disables `GcmApiTest.*` | `chrome.gcm` exists. `register` rejected with "GCM is currently disabled." |
| `instanceID` | Not mentioned by Brave | `getID()` returned an id. `getToken` rejected with "Instance ID is currently disabled." |
| `userScripts` | The reference says `chrome.userScripts` is `undefined` until the person turns on "Allow User Scripts" (Chrome 138 and later). brave-core's filter disables `UserScriptsUITest.ToggleControls_UserScriptsAPIUsage`, with the comment "user scripts toggle is not show in extension details page (data.userScriptsAccess is false). Needs further investigation as to why." | `chrome.userScripts` was `undefined`. The extensions page reported `userScriptsAccess` as `{isEnabled: true, isActive: false}`. After the extensions page's configuration API turned `userScriptsAccess` on (a click on the toggle was not tested), `chrome.userScripts` existed in a page opened afterwards, `getScripts` returned `[]`, `register` succeeded and `execute` returned a value from a script run in the local page |
| `debugger` | The reference says it cannot be optional. brave-core's filter disables `DebuggerApiTest.*` and `CrossProfileDebuggerApiTest.GetTargets` | `attach` and `sendCommand("Runtime.evaluate")` worked on the local page. Required in the throwaway manifest |
| `offscreen` | brave-core's filter disables `RuntimeGetContextsApiTest.GetOffscreenDocumentContext` and `DeveloperPrivateApiTest.InspectOffscreenDocument` | `createDocument` with reason `DOM_PARSER` resolved and `hasDocument()` was true |
| `readingList` | Not mentioned by Brave | `addEntry` and `query` returned the entry |
| `search` | brave-core's filter disables `SearchApiTest.*` | `chrome.search` exists. `query` was not called, because it loads the default search engine's page |
| `storage.sync` | The reference says that with syncing off it behaves like `storage.local`. The wiki says Google accounts integration is disabled | `set` then `get` returned the value, in a profile with no account. That data syncs to another device was not observed |

A disabled test in brave-core's filters shows that the test does not run in Brave's CI. Apart from
the `userScripts` comment, the filters give no reason for these entries, and many entries of the
file disable a whole class of tests. They do not say that the API fails. The filter file was last
changed on 2026-10-29 ([browser_tests.filter][brave-filter], `master` at aa6e4658e). brave-core's
[`_api_features.json`][brave-features] only adds `webui` contexts for `bookmarks`,
`extension.inIncognitoContext`, `tabs` and `topSites`, and removes no API. The wiki is [Deviations
from Chromium][brave-wiki] (last commit 2026-10-28).

### Sites Brave refuses to script

brave-core overrides `ExtensionsClient::IsScriptableURL`
([brave_extensions_client.cc][brave-client], `master` at aa6e4658e, read 2026-10-30). For a URL with
`skus::IsSafeOrigin(url)` it sets the error `"This site is protected and cannot be scripted."` and
returns false. The list is `kSafeOrigins` in [skus_utils.h][brave-skus]:
`https://account.brave.com`, `https://account.bravesoftware.com` and
`https://account.brave.software`, under the comment "please open a security review when appending to
this list". Chromium calls the hook from `PermissionsData::IsRestrictedUrl`, which
`PermissionsData::CanRunOnPage` calls and answers with the error as the reason the page is denied
(`extensions/common/permissions/permissions_data.cc`, chromium `main` at ffcf4813e).

On 2026-10-31, in a headless Brave 1.98.63 with a throwaway profile, `chrome.scripting.executeScript`
on `https://account.brave.com/` was rejected with exactly "This site is protected and cannot be
scripted.". The name was mapped to a local TLS server on 127.0.0.1 with `--host-resolver-rules`, and the
browser was started with `--ignore-certificate-errors`. The same server addressed as
`https://127.0.0.1:18443/` was scripted normally. `account.bravesoftware.com` and
`account.brave.software` were not run, and neither was `userScripts.execute`, which `run_script` calls,
on any of the three. `extension/tests/tools.test.mjs` holds the extension's side: for each of the three
origins, `read_page`, `read_page_links` and `read_page_forms` answer a failure, "the page at <url>
cannot be read: This site is protected and cannot be scripted.", when the browser rejects
`executeScript` with that message, and return nothing of the page. `run_script` of `scripts` fails
in the browser's words in the same way ("the page at <url> cannot be scripted: This site is
protected and cannot be scripted."), which `extension/tests/scripts.test.mjs` holds for each of the
three origins and for one address that is not one of them.

### The debugging notice

Chromium creates the notice when an extension attaches the debugger, unless the process was started
with `--silent-debugger-extension-api` or the extension was installed by a policy
(`chrome/browser/extensions/api/debugger/debugger_api.cc`, chromium `main` at ffcf4813e, read
2026-10-30). On Windows, macOS, Linux and ChromeOS it is an infobar with the text `"<extension
name>" started debugging this browser` and a single button, Cancel, and it does not expire when the
page navigates (`extension_dev_tools_infobar_delegate.cc`). The text's description in
`generated_resources.grd` says the label "does not disappear until the user dismisses it, even if
the debugger is detached". The [`debugger` reference][chrome-debugger] does not mention the notice.
A GitHub code search of brave-core on 2026-10-30 found no reference to the switch or to the infobar
class, so Brave appears to use Chromium's behaviour. The statement in #3583 that a Web Store page
says the person sees a warning bar, and that dismissing it ends the session, was not found on the
[policies][store-policies], [Manifest V3 requirements][store-mv3], review process or network
requests pages. In the run, `attach` succeeded; a headless browser has no window to show an infobar
in, so whether Brave shows the notice was not observed.

### The warning for a broad host pattern

The [permissions list][chrome-list] (updated 2026-09-09) has no row for a host pattern. The
[permission warnings][chrome-warnings] page (updated 2024-02-05) says only that a warning such as
the one for `tabs` is not shown when the extension also requests `<all_urls>`. Chromium's string is
`IDS_EXTENSION_PROMPT_WARNING_ALL_HOSTS`, "Read and change all your data on all websites"
(`chrome/app/generated_resources.grd`, chromium `main` at ffcf4813e). A set of host patterns warns
this way when `URLPatternSet::MatchesAllURLs` is true, which holds when any pattern has an empty
host with subdomain matching, as `https://*/*` and `http://*/*` have (`permission_set.cc`,
`url_pattern_set.cc`). The permission rules hide the `tabs`, `webNavigation` and `topSites` warnings
behind it (`chrome_permission_message_rules.cc`).

`chrome.management.getPermissionWarningsByManifest`, which the reference says needs no permission,
returned these warnings in Brave 1.98.62 for a manifest holding the permissions and hosts in the
first column:

| Requires | Warnings |
|---|---|
| `https://*/*`, or `http://*/*`, or both, or `<all_urls>`, or `*://*/*` | Read and change all your data on all websites |
| `https://example.com/*` | Read and change your data on example.com |
| `https://*.example.com/*` | Read and change your data on all example.com sites |
| two sites | Read and change your data on a.example and b.example |
| three sites | Read and change your data on a.example, b.example, and c.example |
| four sites | Read and change your data on a number of websites |
| `tabs`, `scripting`, `webNavigation` with both patterns (`page-read`, all sites) | Read and change all your data on all websites |
| the same with `https://example.com/*` | Read and change your data on example.com; Read your browsing history |
| `webRequest` with both patterns (`network-watch`, all sites) | Read and change all your data on all websites |
| `debugger` | Access the page debugger backend; Read and change all your data on all websites |

For the permissions of the groups, one permission at a time: `tabs` and `webNavigation` show "Read
your browsing history"; `history` shows "Read and change your browsing history on all your signed-in
devices"; `topSites` "Read a list of your most frequently visited websites"; `tabGroups` "View and
manage your tab groups"; `bookmarks` "Read and change your bookmarks"; `readingList` "Read and
change entries in the reading list"; `privacy` "Change your privacy-related settings";
`contentSettings` "Change and grant access to features such as geolocation, microphone, camera,
cookies, etc., for all your websites and extensions, including this extension."; `system.storage`
"Identify and eject storage devices"; `downloads` "Manage your downloads"; `desktopCapture`
"Capture content of your screen". `sessions`, `scripting`,
`search`, `cookies`, `webRequest`, `fontSettings`, `system.cpu`, `system.memory`, `system.display`,
`idle`, `power` and `browsingData` show none. That
`contentSettings` wording is Chromium's current string and differs from the one on the permissions
list page. The options page lists these lines in `extension/warnings.js`, and
`extension/tests/warnings.test.mjs` holds the table to them and to this section.

Asked for together, some permissions change each other's line. The permissions about the browsing
history give one line between them. With `history` the line is the one of `history`, whatever else is
asked with it. Otherwise `tabs` with `sessions` shows "Read your browsing history on all your
signed-in devices", `tabs` or `webNavigation` without that pair shows "Read your browsing history",
`topSites` alone shows its line and `sessions` alone shows none. `tabs` with `topSites` shows only
the `tabs` line. All 31 combinations of `tabs`, `webNavigation`, `sessions`, `history` and `topSites`
were read. `tabs`, `tabGroups`, `webNavigation` and `sessions` (the `tabs-read` group) show "Read your
browsing history on all your signed-in devices" and "View and manage your tab groups". `history`,
`topSites` and `sessions` (`history-read`) show only the line of `history`, and `tabs`, `sessions`,
`search` and `webNavigation` (`navigate`) show one line, "Read your browsing history on all your
signed-in devices". `tabs`, `history`, `bookmarks`, `readingList`, `browsingData` and `downloads`
(`remove-data`) show four lines: the one of `history`, the bookmarks, the reading list and the
downloads. `cookies` shows no line of its own, and shows the host line for the hosts it is asked
with.

Asked for with access to every site, the lines about reading the history are left out except the one
of `history`: `tabs`, `webNavigation`, `topSites` or `sessions` with both patterns show only "Read
and change all your data on all websites", and `history` with them shows that line and its own. With
a few sites the lines stay, so `tabs` with `https://example.com/*` shows the line for example.com and
"Read your browsing history". The presets of `extension/groups.js` (`look`, `read`, `browse`) were
read with no host, both patterns, one site, two sites and four sites, and the lines were the union
of those of the groups merged as above. Further, 400 combinations of the 23 permissions the groups
listed at the time of that run, with no host, both patterns, one, two and four sites, were read, and
the rules above, with the lines of the other permissions unchanged, gave the lines Brave showed for
each of them. The groups now list 27 permissions, and the combinations were not read again. Brave puts
the lines in an order of its own, which the page does not follow.

A host line names the hosts sorted, once each, without a scheme or a port: `http://a.example/*` and
`https://a.example/*` give "a.example", and two ports of `127.0.0.1` give "127.0.0.1". Three hosts
read "a.example, b.example, and c.example", counting hosts and not patterns, and four or more read
"a number of websites". A pattern for the subdomains of a site is "all example.com sites" and is
listed before the other hosts. The extension asks for no such pattern, since the sites a person
allows are written as origins.

These are the warnings Chromium builds for an install or a request, as observed through the
management API. The prompt that shows them was not displayed in the run.

### Optional host patterns

The [permissions reference][chrome-permissions] (updated 2026-10-29) says: "If you want to request
hosts that you only discover at runtime, include "https://*/*" in your extension's
optional_host_permissions field. This lets you specify any origin in "Permissions.origins" as long
as it has a matching scheme." It does not mention `<all_urls>`, and neither does the [declare
permissions][chrome-declare] page. The permissions it says cannot be optional are `debugger`,
`declarativeNetRequest`, `devtools`, `geolocation`, `mdns`, `proxy`, `tts`, `ttsEngine` and
`wallpaper`, and none of them is a host pattern.

Chromium decides by pattern containment. `UnpackOriginPermissions` in
`extensions/browser/api/permissions/permissions_api_helpers.cc` accepts a requested pattern only
when one pattern of the manifest's `optional_host_permissions` contains it (`URLPattern::Contains`),
and `permissions_api.cc` otherwise rejects with "Only permissions specified in the manifest may be
requested." (chromium `main` at ffcf4813e). A manifest pattern `<all_urls>` contains any pattern
whose schemes it covers, and a requested pattern loses the chrome scheme, and the file scheme
without file access, before the comparison. The rule is the one for the two scheme patterns, and the
reference states it for `https://*/*` only.

Observed in Brave 1.98.62, with `contains` false and never throwing for any pattern:

- With `optional_host_permissions` of `https://*/*` and `http://*/*`: a request for `<all_urls>`,
  `*://*/*`, `ftp://example.com/*` or `file:///*` rejected with the error above, and a request for
  `https://*/*` reached the prompt and waited.
- With `<all_urls>`: a request for `<all_urls>` or `https://*/*` reached the prompt and waited,
  `chrome://settings/*` rejected with the error above, and `file:///*` resolved true at once with no
  prompt.

The extension lists the two scheme patterns and `<all_urls>`. It requests a site's pattern, which
each of them contains, the two patterns, or `<all_urls>` for `page-read` on all sites. A manifest
that listed only the two scheme patterns would have a request for `<all_urls>` rejected, which is
why `<all_urls>` is listed ([Taking a picture of a page](#taking-a-picture-of-a-page)). What the
prompt shows on screen and what a person's answer does were not observed.

### Taking a picture of a page

`screenshot_page` calls `chrome.tabs.captureVisibleTab(windowId, {format: "jpeg", quality: 80})`,
which returns a promise from Chrome 88 and so raises no `minimum_chrome_version`. The reference says
the call needs "the `<all_urls>` permission or `activeTab`" ([tabs][chrome-tabs], read 2026-10-30).
Observed in Brave 1.98.62, headless, with probe extensions that differed only in what they held, on
a local page of one solid colour in the active tab:

| What the extension held | `captureVisibleTab` |
|---|---|
| nothing, or the page's own origin, or another site's origin, or `tabs` and the page's origin | refused: "Either the '<all_urls>' or 'activeTab' permission is required." |
| `https://*/*` and `http://*/*`, or `*://*/*` | refused with the same words |
| `activeTab`, in the manifest or granted by `permissions.request`, called from the worker with no gesture | refused with the same words |
| `<all_urls>` | taken: a PNG of the window's viewport, the colour of the page |

- The call takes a window and no tab. It returned the active tab of the window it was given, and
  the window last focused when given none. A tab in the background of a window is not captured.
  With `<all_urls>` held, a page of `about:blank` or the first tab is refused ("Cannot access
  contents of the page"), as is a `chrome://` page or an extension page.
- Three calls started together took two and rejected the third with "This request exceeds the
  MAX_CAPTURE_VISIBLE_TAB_CALLS_PER_SECOND quota." Calls 500 ms apart or more were all taken in 16.
  The extension spaces its own at 600 ms.
- A window 2048 pixels wide gave a data URL of 26,623 characters of JPEG for a page with a heading,
  534,971 for a page of a photograph and text, and 2,077,483 for every pixel random, about 0.02, 0.40
  and 1.56 MB decoded. A PNG gave 5,594,802 and 10,406,630 characters for the last two, 4.00 and 7.44
  MiB decoded. A JPEG at quality 80 from the browser was the same length as the JPEG a worker
  re-encoded, so the extension does not re-encode. A device pixel ratio of 2 was not run.
- A grant of `<all_urls>` through the prompt was not run, since a headless Brave shows none. The
  options page requests it as a click, as it requests the two patterns.

So a screenshot needs the host permission `<all_urls>` and nothing narrower. The smallest change that
gives it is to let `page-read` ask for `<all_urls>` in place of the two patterns when the person
chooses all sites for the group: Brave words it as the two patterns, the scope the person already
chooses is named "all sites", and the request of no other group changes, except that of `screen`,
whose `record_gif` takes a picture in the same way and asks for it under the same scope ([Taking a
picture of the screen](#taking-a-picture-of-the-screen)). Under the other scope the tool is
refused, so the Read and Browse presets, which leave `page-read` on the allowed sites, take no
picture. The alternatives were for every group on all sites to ask for `<all_urls>`, which gives
`cookies`, `network-watch` and `input` a pattern that also covers `file:` and `ftp:` URLs they do not
need, and a third scope for pictures alone, which adds a control for the same prompt.

Because `<all_urls>` is wider than the person's lists, the extension checks the URL itself (the URL
is HTTP or HTTPS, the site is not blocked, and exactly one tab is at it), that the tab is the active
tab of its window and the window is not minimized, and the tab again before and after the capture. It
does not bring the tab forward, since that changes what the person sees and `focus_tab` in
`tabs-manage` already does it. A frame of another origin inside the visible tab is in the picture, and
the blocked sites are matched by the tab's own URL, so a blocked site shown in a frame is not blocked.
The reply is `{image: {mimeType, data}}` and nothing about the page, the relay sends it as an MCP
`image` part, and the client holds it as a picture the planner is given a reference to
([MCP-6](../specs/mcp.md#MCP-6)).

The tool as built was run in Brave 1.98.62, headless, against a local page of one colour, from a
page of each of three copies of the extension (a service worker cannot `import()` the module) that
held `<all_urls>`, the two patterns, and neither. With `<all_urls>` it returned a JPEG 1280 by 663
pixels whose corner was the page's colour, in a reply with the one field `image`. With the two
patterns, or nothing, it was refused before any capture, with the refusal for a group not held, which
says a picture needs all sites. Three calls
started together were taken about 0.6 s apart and none was rejected. A tab behind another in its
window was refused as not the visible one, and the tab in front was captured. `permissions.contains`
answered true for `https://*/*`, for both patterns and for `http://127.0.0.1/*` while `<all_urls>`
was held, and false for `<all_urls>` while only the two patterns were.

### A picture at the cap, through the whole chain

Run on 2026-10-31 in Brave 1.98.63 (Chromium 156), macOS 26.6 on Apple silicon, headless, with a
profile made for the run and removed afterwards, name resolution for every host except 127.0.0.1
switched off, and the manifest of the host written to a scratch home that `CFFIXED_USER_HOME`
pointed Brave at. Nothing was stood in for. Brave started the host (`bravebot-browser
chrome-extension://<id>/`) through native messaging, the first extension ran from a copy that held
`<all_urls>`, `tabs`, `webNavigation` and `scripting` as required permissions (a headless Brave
cannot answer the prompt for them), `page-read` was turned on for all sites, and the calls were
`tools/call screenshot_page` on the stdio line of `bravebot-browser mcp --group page-read`, from a
script and from `StdioServer`, the client the agent uses. The page was a canvas the size of the
window, with the first N pixels in reading order random colours from a fixed seed and the rest grey,
so N sets the size of the JPEG. The window was 2048 pixels wide and 6519 pixels tall (device pixel
ratio 1).

A JPEG at quality 80 of random pixels was 0.69 bytes per random pixel (690,606 bytes for 1,000,000),
and pixels in squares of 2, 3, 4 and 6 were smaller. Of the contents tried, random pixels gave the
largest JPEG, and with them a window 2048 pixels wide reached 8 MiB at about 6,500 pixels tall. The
page of a photograph and text in the run above was 0.40 MB at the same width, so the cap is far from
the pictures an ordinary page gives.

| Picture | Decoded | Result | Time of the call |
|---|---|---|---|
| N = 13,205,864, 2048 by 6519 | 8,385,775 bytes, 2,833 under the cap | one `image` part, `image/jpeg`, 11,181,036 characters of base64, a line of 11,181,150 bytes | 430 to 472 ms in five calls (the browser's capture alone 119 ms) |
| N = 13,207,494, 1,630 random pixels more | 8,390,729 bytes, 2,121 over | one text part, the extension's sentence below, a line of 237 bytes | 124 to 132 ms in six calls |
| every pixel random, 2048 by 6519 | 8,476,947 bytes | the same sentence | 123 ms |
| every pixel random, 4096 by 7919 | 20,587,276 bytes, a data URL of 27.4 MB | the same sentence | 304 ms |
| every pixel random, 8192 by 9919 | 51,574,444 bytes, a data URL of 68,765,951 characters | the same sentence | 780 ms |
| a grey window, 2048 by 6519 | 79,093 bytes | one `image` part | 50 ms |

- The sentence is "the picture of the tab at `<url>` is over 8 MiB, which is the most a picture may be; make the
  window smaller and call again". The reply of a picture over the cap holds that one text part and
  no `image` part, so nothing partial arrives. The extension refuses before it sends the data URL,
  and those calls took about 300 ms less than the one that sent 11 MB.
- `StdioServer` read the near-cap reply as one picture of type `image/jpeg`, held as a `data:`
  URI of 11,181,059 characters labelled untrusted and private, with nothing left out and no text.
  The call took 541 ms. Over the cap it returned the tool failure with the same sentence in 138 ms,
  and for the grey window one picture of 105,483 characters.
- The picture arrived byte for byte. A capture taken in the extension's worker with
  `captureVisibleTab` straight after had the same length and the same SHA-256 as the picture that
  came through the chain, and the part was canonical base64 whose bytes began with a JPEG marker
  (SOF0, three components) and ended with the end marker.
- The relay's resident memory (`/usr/bin/time -l`) was 2.3 MiB after `initialize` and `tools/list`,
  36.4 MiB after one near-cap picture, and 81.3 MiB at the end of a session that made five of them
  and the refusals. The host started at 2.1 MiB and was at 59 MiB after the first near-cap
  pictures, and stayed there.
- No limit refused before the cap. At the cap the reply is 17% of the 64 MiB that native messaging
  allows from the extension, and the line of 10.7 MiB is under the 16 MiB line the client reads
  ([MCP-13](../specs/mcp.md#MCP-13)) and the 64 MiB the relay reads from the host. The 8 MiB check in
  the extension is the first that binds, for every size above it, including a picture the browser
  made that was larger than 64 MiB as base64.
- A message from the extension to the host was measured directly, with a port opened from the worker
  to the host when no other host was running (a second host exits at once while the first holds the
  socket). Brave delivered one of 20 MiB and one of 67,108,764 bytes (64 MiB less 100), for which
  the host reached 130 MiB resident, and `postMessage` threw "Message exceeded maximum allowed size
  of 64MiB." for one of 67,109,864 bytes (64 MiB and 1,000), which the host never saw. The limit
  is Brave's, in the sender, and it is 64 MiB.
- A message from the host to the extension was measured through the relay with the `url` of
  `screenshot_page`. A `url` of 1,048,034 characters was sent and the extension answered it. One of
  1,060,034 and one of 1,200,034 were refused by the host in 29 to 33 ms with "the request is over
  native messaging's limit of 1048576 bytes and was not sent", and nothing was written. Brave's own
  check of this direction was not exercised, since the host refuses first.

### Taking a picture of the screen

`capture_screen` uses the picker the browser shows, `chrome.desktopCapture.chooseDesktopMedia`, and
`record_gif` uses `captureVisibleTab` as `screenshot_page` does. Observed in Brave 1.98.62, headless,
with a throwaway profile, on 2026-10-31. The run used `--use-fake-device-for-media-stream`, which
gives a stream of a fake 800 by 600 screen so that no real screen is read, and
`--auto-select-desktop-capture-source`, which makes the picker answer without a window. Without that
switch a headless browser shows no picker and the call does not return.

| Call | Observed |
|---|---|
| `chooseDesktopMedia(["screen", "window"], callback)` from the service worker | refused: "A target tab is required when called from a service worker context." |
| the same from an extension page in a tab, without a gesture, no picker switch | does not return in 6 seconds |
| the same with the switch | returns a stream id and `{canRequestAudioTrack: false}` in 32 to 86 ms, and a number from the call that `cancelChooseDesktopMedia` takes |
| `getUserMedia` with `chromeMediaSource: "desktop"` and that id, in the page that asked | a stream whose track is `screen:1:0`, 800 by 600 |
| the same id in an offscreen document | `AbortError`, "Error starting tab capture", as for an id that was never issued |
| `chrome.desktopCapture`, `chrome.tabs`, `chrome.windows`, `chrome.storage` in an offscreen document | `undefined`; `chrome.runtime` is the one namespace there |
| `management.getPermissionWarningsByManifest` for `desktopCapture` | "Capture content of your screen" |
| the same for `desktopCapture` and `tabs` with `<all_urls>` | "Read and change all your data on all websites"; "Capture content of your screen" |
| the same for `desktopCapture` and `tabs` with `https://example.com/*` | "Read and change your data on example.com"; "Read your browsing history"; "Capture content of your screen" |
| `tabCapture.getMediaStreamId` and `tabCapture.capture`, with `tabCapture`, `tabs` and `<all_urls>` held, for a tab at a local page | refused: "Extension has not been invoked for the current page (see activeTab permission). Chrome pages cannot be captured." |
| the same after `action.openPopup()` | `openPopup` refused ("Extension does not have a popup on the active tab."), and the same refusal for `tabCapture` |

The reference for `desktopCapture` says of `targetTab` that the stream "can only be used by frames in
the given tab whose security origin matches `tab.url`", and that without it the stream "can be used
only by the calling extension" ([desktopCapture][chrome-desktopcapture], read 2026-10-31). So the
picker is called from an extension page, the stream is opened in that page, and the page encodes the
frame. An offscreen document cannot hold the stream, because it has no `desktopCapture` and the id is
refused there, so the group lists `desktopCapture` and `tabs` and not `offscreen`. The extension page is
`capture.html`, opened in a small popup window for each call and closed on every path.

With the real extension loaded in that browser and the permissions held at install, `capture_screen`
returned `{image: {mimeType: "image/jpeg", data}}` and nothing else, a JPEG of 798 by 516 pixels, and
no window of the extension was left. `record_gif`, called for 4 frames of a local page of one solid
colour, answered in 3.5 seconds with `GIF89a` of 480 by 270 pixels, which Brave's own decoder read, and
whose corner pixel was the page's colour at the precision of the encoder (206 for 204). The GIF
encoder (`extension/gif.js`) was also read by macOS `sips` for sizes from 1 by 1 to 640 by 360 and
palettes of 1 to 255 colours, and every pixel of the first frame matched.

Observed in a visible Brave 1.98.63 on macOS with Screen Recording in effect, a throwaway profile
and the picker answered by `--auto-select-window-capture-source-by-title`, against a window of the
test page only: a window whose content does not change gave a video that had its size and a frame
(`readyState` 4, 2400 by 1900) when `play()` returned, one `requestVideoFrameCallback` about 1.1
seconds later, and no other in the next 8 seconds. A window with a ticker gave one every 100
milliseconds. The first frame was not blank in either. A capture that waited for two callbacks
failed on the still window after 5.3 seconds with "what was chosen gave no picture". The page now
waits up to 5 seconds for the first callback and then up to one second for a second, and takes the
frame it has when none comes: the still window gave its picture in 3.5 seconds from the start of
the page, the ticker window in 0.27 seconds.

What was not run: a real picker (a window with a person choosing), a real screen (the fake stream is
not one), the permission prompt for `desktopCapture` (headless shows none), `tabCapture` after a
person has clicked the extension's button, and `desktopCapture` on Windows or Linux.

### Saving a page as a file

`save_page_archive` calls `chrome.pageCapture.saveAsMHTML({tabId})`, which returns a promise of a
`Blob` from Chrome 116 (the reference, read 2026-10-31, says "The Promise form has been available
since Chrome 116+"). A service worker has no `URL.createObjectURL`, so the extension turns the blob
into a data URL and gives it to `chrome.downloads.download` with a name of its own, `saveAs: false`
and `conflictAction: "uniquify"`, as `download_file` does. Observed in Brave 1.98.62, headless, in
probe extensions that held `pageCapture`, `downloads` and `tabs`, against local pages in the active
tab, with the profile and the download folder made new for each run and the browser's own download
folder preference pointed at the latter:

- `saveAsMHTML` returned a blob of type `""` that began `From: <Saved by Blink>` and held the
  address and the title of the page, with no host permission held at all (`permissions.getAll()`
  listed no origin). It archived a page with an iframe of another path. So `pageCapture` is
  not limited by the hosts the extension holds.
- `chrome.management.getPermissionWarningsByManifest` gave "Read and change all your data on all
  websites" for `pageCapture` alone, for `pageCapture` with the pattern of one site, and for it with
  the two patterns. Beside it `tabs`, `webNavigation`, `sessions` and `topSites` give no line of their
  own, `history` keeps its line, and `downloads` gives "Manage your downloads". The five permissions
  of `page-read` with both added, `tabs`, `scripting`, `webNavigation`, `pageCapture` and `downloads`,
  gave those two lines under every scope, and the three the `downloads` group lists, `downloads`,
  `pageCapture` and `tabs`, give them too, as `downloads` and `pageCapture` alone do. The Read and
  Browse presets, which hold `page-read` and not `downloads`, give the lines of Look and of the hosts
  alone.
- A probe extension that held `pageCapture` and `downloads` and no host, with a tab open at a page of
  127.0.0.1, was given `url: null` and `title: null` for every tab by `chrome.tabs.query` and by
  `chrome.tabs.get`, as an extension that holds neither `tabs` nor a host covering a tab is. With
  `tabs` added the same calls gave the address and the title. The tool finds its tab by its address,
  so the `downloads` group lists `tabs`.
- A data URL of 1,372 bytes, 1.07 MB, 8.5 MB and 20 MB (26.7 million characters) was downloaded to
  completion in 0.25 to 0.4 s, in the folder the preference named. With `filename` set the file had
  that name; without it Brave named it `download.mht`. A second download of the same `filename` with
  `uniquify` was named `<name> (1)`, `".."` was refused with "Invalid filename", and a name with
  slashes made folders, so the extension never lets a name carry one. `Browser.setDownloadBehavior`
  of the DevTools protocol replaced the name and the conflict rule, so a run that set it showed
  neither and was discarded.
- A second way, an offscreen document that made a blob URL and clicked an anchor with a `download`
  name, saved three files in a row with no gesture and no `downloads` permission. It needs the
  `offscreen` permission, gives the extension no path and no state of the download, cannot be told
  whether the person's "ask where to save" setting made it wait, and needs the archive passed to the
  document as a message. `download_file` already uses `chrome.downloads`, so the tool uses it too, and
  the `downloads` group lists `pageCapture` beside it.
- Not run: the grant of `pageCapture` and `downloads` through Brave's prompt (a headless Brave shows
  none), the person's "ask where to save each file" setting (the prompt is not shown headless, and
  `saveAs: false` is the extension's request), a page at `account.brave.com` (the archive was not
  taken of one, which a local server standing in for the name allows, as under "Sites Brave refuses to
  script"), a device pixel ratio or a page with cross-origin frames in other processes.

The tool as built was run in Brave 1.98.62, headless, against local pages, from a page of a copy of
the extension that held the permissions at install (a service worker cannot `import()` the module).
It saved an archive of 1,372 bytes as `bravebot-127.0.0.1.mhtml` in the download folder, a second
call as `bravebot-127.0.0.1 (1).mhtml`, an archive of 6.4 MB as `... (2).mhtml`, and the same page
under the scope of the allowed sites as `... (3).mhtml`. It refused a page of 20 MB with the message
for more than 16 MiB, two tabs at one address, an address no tab was at, a blocked site and a site
that was not allowed, each before or without a file being written, and nothing was written to the
person's own Downloads folder.

That run was in a copy that held `tabs` at install. A run of `handle` from a page of two copies that
held `nativeMessaging`, `storage`, `alarms`, `downloads` and `pageCapture` and no host, one with the
line of the `downloads` group as it was and one with `tabs` in it and held, gave `no open tab is at`
the address for the first and a saved file with the state `complete` for the second.

Why this placement. The tool is in the `downloads` group, which lists `pageCapture`, `downloads` and
`tabs`, so `page-read` lists what its read tools call and the Read and Browse presets ask for
neither `downloads` nor `pageCapture`. A person who turns on `downloads` is asked for "Read and
change all your data on all websites" as well as "Manage your downloads", because Brave words
`pageCapture` that way whatever hosts come with it, and for nothing more for `tabs`. The group has no
hosts and no scope, and the browser limits `pageCapture` to no site, so a group turned on for
downloads could archive any open tab. `ALLOWED_ONLY_TOOLS` in `groups.js` names the tool, `handle` in `tools.js` then runs the check of
the allowed sites for every call of it as it does for a group with `allowedOnly`, and the tool
reads the blocked list itself. A tool of the group that names a download is not held to the list.
The tool does not follow the scope of `page-read`: a group turned on for downloads must not read a
page that `page-read` is off for, and the allowed list is the person's list of pages a session may
read. The alternatives were `page-read`, which makes Read and Browse ask for `downloads` and for
`pageCapture`, which is what the first version of the tool did, a flag on the whole `downloads`
group, which would hold `download_file` to the allowed sites although it names a file to fetch, a
group of its own for the archive that lists the three permissions, which leaves `downloads` asking
for the downloads line alone and adds a switch, a row in each table and a server for one tool, and
the offscreen document, which loses the file's name and state and adds a permission.

The size bound is 16 MiB of archive. A larger archive is refused saying so, where the extension would
hold the archive, a data URL a third longer and a binary string at once. The largest data URL run
was 20 MB. The tool asks the browser for the file's state up to 50 times, 100 ms apart, and reports
`in_progress` if it is still running after that.

### Cookies

Observed in Brave 1.98.62, headless, with a probe extension holding `cookies` and a host
permission for `http://127.0.0.1/*` and `https://*.probe.example/*`, in a profile made for the run:

- `cookies.getAll({url})` returned every cookie sent to the URL, including an `httpOnly` one and one
  with a `Path` the URL is below, and each had `name`, `value`, `domain`, `hostOnly`, `path`,
  `secure`, `httpOnly`, `sameSite`, `session` and `storeId`, and `expirationDate` only for a cookie
  with an expiry. `getAll({url, name})` returned the cookies of that name.
- `cookies.remove({url, name})` deleted both cookies named `sid` that were sent to the URL, one at
  path `/` and one at `/a/`, and left a cookie of another name. For a name no cookie had it resolved
  with the URL, the name and the store id, as it did for the cookies it deleted.
- `getAll` for `https://not-held.example/`, a host the extension held no permission for, resolved
  without an error.

### Not verified

Needs a person at a Brave window: the prompt Brave shows for a permission that warns and for a host,
what answering it grants, the debugging notice while `debugger` is attached, the text Brave shows
for the "Allow User Scripts" toggle as a person uses it (Brave 1.98.62 showed its row and a trusted
click on it turned it on, and Brave 1.98.63 turned it on and off through the extensions page's API).
Needs an account or another device: whether `storage.sync` syncs, whether `gcm` works once the
setting that enables it is on, and what `sessions.getDevices` (`list_synced_tabs`) returns for a
signed-in account's other devices. Not run: a page on `account.bravesoftware.com` or
`account.brave.software`, `userScripts.execute` on any of the three protected sites, Brave's beta and
nightly channels, Windows and Linux, `search.query`, `instanceID.getToken` and `gcm.register` with the
network on, and whether Chromium keeps a permission that an update moves from required to optional.

[brave-filter]: https://github.com/brave/brave-core/blob/master/test/filters/browser_tests.filter
[store-policies]: https://developer.chrome.com/docs/webstore/program-policies/policies
[store-mv3]: https://developer.chrome.com/docs/webstore/program-policies/mv3-requirements
[brave-client]: https://github.com/brave/brave-core/blob/master/common/extensions/brave_extensions_client.cc
[brave-skus]: https://github.com/brave/brave-core/blob/master/components/skus/common/skus_utils.h
[brave-features]: https://github.com/brave/brave-core/blob/master/chromium_src/chrome/common/extensions/api/_api_features.json
[brave-wiki]: https://github.com/brave/brave-browser/wiki/Deviations-from-Chromium-(features-we-disable-or-remove)
[chrome-warnings]: https://developer.chrome.com/docs/extensions/develop/concepts/permission-warnings
[chrome-declare]: https://developer.chrome.com/docs/extensions/develop/concepts/declare-permissions
[chrome-list]: https://developer.chrome.com/docs/extensions/reference/permissions-list
[chrome-permissions]: https://developer.chrome.com/docs/extensions/reference/api/permissions
[chrome-tabs]: https://developer.chrome.com/docs/extensions/reference/api/tabs
[chrome-debugger]: https://developer.chrome.com/docs/extensions/reference/api/debugger

## Protocol

JSON-RPC between the MCP server and the extension:

- request `{id, method, params}`
- reply `{id, result}` or `{id, error}`, matched by `id`

Initial tools: `get_platform_info`, `list_tabs`, `list_frames`, `read_page`,
`search_history`, `search_bookmarks` and `open_tab`. `open_tab` opens an HTTP or HTTPS URL a person
approved in a background tab, closes it if the page that loads is on another host, and returns only
the URL that loaded. `read_page` can read the top page or one exact HTTP or
HTTPS frame URL a person chose from `list_frames` and supplied in a later message; the planner does
not read the untrusted list itself.

Tools added to a group since, offered only by the server of that group: `list_windows`,
`list_tab_groups` and `list_recently_closed` in `tabs-read`, and `get_visits`, `list_top_sites` and
`list_synced_tabs` in `history-read`. `list_windows` and `list_tab_groups` return no tab.
`get_visits` takes one exact URL, as `list_frames` does, and `list_synced_tabs` returns the tabs on
the person's other devices as Brave last synced them. Each leaves out an address on the blocked list.

Another tool of `page-read`, `screenshot_page`, takes a picture of the visible tab at an exact URL
and is described under [Taking a picture of a page](#taking-a-picture-of-a-page). Two more tools
are in the `page-read` group, and each names an open tab by its exact URL and
reads its top document. `read_page_links` lists the HTTP and HTTPS links with the text each is shown
with, and `read_page_forms` lists the forms, where each submits and how, and the name and type of
each field. Neither reads what a field holds. Both enter shadow roots, closed ones through
`chrome.dom.openOrClosedShadowRoot`, which needs no permission, and the open ones where that call
is missing. Twelve tools are in the `tabs-manage` group, and each names an open tab, or for the two
that group several, the open tabs, by exact URL: `focus_tab` makes it the active tab and brings its
window forward, `pin_tab` and `mute_tab` set or clear that state, and `move_tab` moves it within its
window. `group_tabs` puts tabs that are in one window into a new tab group and `ungroup_tabs` takes
tabs out of theirs. `update_tab_group` changes the title, color or collapsed state of the group a
tab is in, since a group has no address and its id says nothing a person can judge. `discard_tab`
unloads a tab from memory, `duplicate_tab` opens a copy of it, `set_zoom` sets its zoom and
`focus_window` brings its window forward without changing which tab is active and `resize_window`
sets the width and height of that window in pixels. They change how a tab looks or where it sits,
and return the URL and the state the browser reports
([BROWSER-23](../specs/browser.md#BROWSER-23)). The group lists `tabs` and `tabGroups`, and Brave
asks to "View and manage your tab groups" for the second
([permissions list](https://developer.chrome.com/docs/extensions/reference/permissions-list)).

`bookmarks-read` has `list_bookmark_folder`, `list_recent_bookmarks` and `list_reading_list` as
well. `list_bookmark_folder` names a folder by its exact title, which a person can judge in the
question where a bookmark id from an earlier result is not, and fails when no folder or more than one
has that title. The browser's search by title does not return the built-in folders at the top of the
tree (Bookmarks bar, Other bookmarks), so the extension reads them with `bookmarks.getTree` and counts
them with what the search returns, once each; `list_bookmark_folder` and the tools that create or move a bookmark take their titles, and `remove_bookmark_folder` does not, since the browser refuses to remove them.
With no folder, `list_bookmark_folder` returns the title of each built-in folder. `getTree` belongs to
the `bookmarks` permission the group already lists.

`bookmarks-write` has six tools that add and change bookmarks and reading list entries and delete
none: `create_bookmark`, `create_bookmark_folder`, `move_bookmark`, `rename_bookmark`,
`add_reading_list_entry` and `update_reading_list_entry`. It lists `bookmarks` and `readingList`, the
two permissions `bookmarks-read` lists, and Brave asks to "Read and change your bookmarks" and to
"Read and change entries in the reading list"
([permissions list](https://developer.chrome.com/docs/extensions/reference/permissions-list)).
A bookmark is named by its exact URL and a folder by its exact title, the two things a person can
judge in the question before the call, and the browser's node id, which comes from an earlier result
and says nothing a person can judge, is an argument to none of them. A URL of more than one bookmark
and a title of more than one folder name none, and the call fails with the number and nothing of
what is saved. Only a web address is bookmarked, since a `javascript:` bookmark runs in a page when
it is clicked. The reply holds the title and folder the browser reports
([BROWSER-28](../specs/browser.md#BROWSER-28)). The browser's call to add a reading list entry takes a title, so
`add_reading_list_entry` requires one.

`navigate` has `open_tab` and six more tools that make Brave request a page as the person, with their
cookies: `navigate_tab`, `reload_tab`, `go_back`, `go_forward`, `open_window` and `restore_closed`.
It lists `tabs`, to find a tab by its URL, `sessions`, to restore a closed tab, `search`, for
`search_web`, and `webNavigation`, to see where a page committed. A tab is named by its exact URL,
and `navigate_tab` takes the address to go to as well. The question shows the URL and the address, and the page that loads has to be on the
host of the last one shown, as `open_tab` requires ([BROWSER-16](../specs/browser.md#BROWSER-16)).
A tab or window the tool made is closed when it is not. A tab the person had open is left blank
instead, since closing it is what `close_tab` would do, and the page it was moved to is not left
showing. `go_back` and `go_forward` take no address, since the page before or after is not one the
question could show, so the host that page must be on is the host of the tab's URL, and a move across
hosts fails and leaves the tab blank. `restore_closed` restores a closed tab by the URL that
`list_recently_closed` shows and never by a session id, and does not restore a closed window, whose
tabs would all load. A site on the blocked list is not navigated to or from, and is closed or blanked
if a page commits there. The group reads no page, so the allowed sites do not limit it, as they do
not limit `open_tab`. Brave asks to "Read your browsing history", and for `sessions` to read it on
all signed-in devices
([permissions list](https://developer.chrome.com/docs/extensions/reference/permissions-list)).

`search_web` and `wait_for_load` are in the group as well. `search_web` takes the words to search
for, at most 256 characters, and sends them to the person's default search provider with
`search.query`. That call takes either a tab to show the results in or a disposition, and fails with
"Cannot set both 'disposition' and 'tabId'." when given both. The tool makes a blank background tab,
gives its id, and passes no disposition. The disposition `NEW_TAB` opens a foreground tab and
replies with nothing, so the extension could neither keep the tab in the background nor name it to
close it (`chrome/browser/extensions/api/search/search_api.cc`, chromium `main`, read 2026-10-30).
The question shows the words and not the provider's address, which no API tells an
extension, so the page cannot be held to the host the person was shown. It has to be a web page, not
at a blocked site, and commit within 20 seconds, or the tab is closed and the call fails. The reply
is the URL of the results page, and the query has been sent to the provider by then. `wait_for_load`
takes the URL of an open tab and asks the browser for the tab's status every 250 milliseconds, for
at most 20 seconds. It replies `loaded` or `timed out` with the URL asked for, and fails where the
tab was closed or went to another URL meanwhile. Neither returns page content.

`cookies` has `list_cookie_names` and `delete_cookie`, and lists `cookies` and the hosts, as
`page-read` does, so it is limited to the allowed sites or to all sites that are not blocked
([BROWSER-22](../specs/browser.md#BROWSER-22)). Brave shows no prompt for `cookies` and shows the
host line for the hosts, which is what the person is asked to allow. `list_cookie_names` takes an
HTTP or HTTPS URL and gives the name, domain, path, flags and expiry of each cookie the browser sends
to it. `delete_cookie` takes a name and a URL and deletes that cookie. The browser's `cookies.remove`
deletes every cookie of the name sent to the URL, and answers as if it had deleted one when none
exists, so the tool lists the cookies of that name first and deletes only where exactly one is sent.
The browser hands the extension each cookie with its value, which is a session token, and the tools
build their result from named fields, so no value is read or sent on (BROWSER-30). A cookie belongs
to a host and is sent to every scheme and port of it, so the blocked list covers the host for these
two tools: a URL at the host of a blocked site is refused, whatever its scheme or port, and a cookie
whose domain is that host is left out of a list and stops a delete. The group is asked about every
call: `install` declares its server with `--ask-every-call`.

`network-watch` has `list_requests`, which gives the URL, type and status of each request the tab at
one exact URL has made since it loaded its page, and no header, cookie or body. The extension asks
`webRequest` for a URL filter and none of the extra information that adds request headers, so
`Cookie` and `Authorization` are never given to it. It keeps the requests in the service worker's
memory, in a record for each tab, from the request that begins a page load, and in no storage. A
record is dropped when its tab closes or when `webRequest` or a host is taken back. The listeners are
registered as the worker starts, whatever the switches are, because the browser reports an event only to
a listener registered then. Each one keeps a request only while the group's switch and the switch of
`list_requests` are both on, as a copy of the two held in memory says, read as the worker starts and
whenever either is saved. Before the first read has finished nothing is kept, and turning either off
drops the records. Brave may stop the worker, which loses the records. A page that loaded before the
worker started or read the switches, or before the group was turned on, has no record, and the call
says so and returns nothing, since requests that arrive for a load whose start was missed are not the
whole of it. Requests to a blocked site and
requests made by a frame at one or inside one are left out of the result. The group reads pages, so
a call is limited to the allowed sites, or to all sites that are not blocked, as `read_page` is
([BROWSER-25](../specs/browser.md#BROWSER-25)).

`browser-info` has seven tools that say how Brave and the computer are set. It lists `privacy`,
`contentSettings`, `fontSettings`, `system.cpu`, `system.memory`, `system.display`, `system.storage`,
`idle` and `power`, none of which Chrome bars from being optional, and Brave asks to "Change your
privacy-related settings" for `privacy`, to "Change your settings that control websites' access to
features such as cookies, JavaScript, plugins, geolocation, microphone, camera etc." for
`contentSettings` and to "Identify and eject storage devices" for `system.storage`
([permissions list](https://developer.chrome.com/docs/extensions/reference/permissions-list)). The
tools call the promise forms of those APIs, the newest of which is `idle.queryState` in Chrome 116, so
the group raises nothing in `minimum_chrome_version`. `get_privacy_settings`,
`get_content_settings` and `get_font_settings` only read: they call `get` on a setting and never `set`
or `clear`, and each setting is read by a call of its own, so one the browser will not report comes
back with its error and a setting the browser does not have is left out. `get_content_settings` takes
one HTTP or HTTPS URL, as `get_visits` does, and refuses one at a blocked site. `get_font_settings`
does not list the fonts installed. `get_system_info` passes on the processor count, architecture,
model and features, the memory, each display's size, position and density, and each storage device's
name, type and capacity, and does not call `ejectDevice` or read how busy a processor is.
`get_idle_state` takes a number of seconds. `get_access_flags` calls `extension.isAllowedFileSchemeAccess`
and `extension.isAllowedIncognitoAccess`, which need no permission, and says whether the person allowed
the extension on `file:` URLs and in private windows.

`keep_awake` is the one tool of the group that changes anything. The `power` API has no time limit,
so the tool takes a number of minutes, at most 240, and an optional level (`system`, or `display`
to keep the screen on as well). It saves when the request ends in session storage, makes an alarm for
that time, and then asks the browser, so a worker that stops part way leaves a record to release and
never a request without one. The worker releases a request whose time has come as it starts and on
every alarm, and a call with 0 minutes releases it at once ([BROWSER-26](../specs/browser.md#BROWSER-26)).
`releaseAwake` in `awake.js` releases the request and forgets it, and is what anything else that has to
end the request calls. The group calls it whenever it stops being on, after its switches are saved and
before its permissions are taken back, since `chrome.power` is gone once `power` is: when it is turned
off, by a preset or Off, when its lifetime ends and when a permission it lists is taken back in the
browser. When it is `power` that was taken back the API is already gone, so the extension can only
forget the request and its alarm, and turning the group on releases what the browser may have kept.
A call that is past the checks when the group is turned off makes its request after that release, so
the tool reads the group's switch again once it has asked the browser, and releases the request
and refuses where the group is off or the switch cannot be read.
Whether the browser keeps a request across a stop of the service worker, and
whether it releases one when the `power` permission is taken back, were not run in Brave.

`remove-data` has nine tools that delete, and its server is declared so that every call is asked about
and no answer is kept ([BROWSER-21](../specs/browser.md#BROWSER-21)). It lists `tabs`, `history`,
`bookmarks`, `readingList`, `browsingData` and `downloads`; Brave asks to "Read your browsing
history", to "Read and change your browsing history on all your signed-in devices", to "Read and
change your bookmarks", to "Read and change entries in the reading list" and to "Manage your
downloads", and shows no warning for `browsingData`
([permissions list](https://developer.chrome.com/docs/extensions/reference/permissions-list)). It
is on until Brave restarts unless the person chooses another time. `close_tab` and `close_window`
name a tab by its exact URL, and `close_window` closes the window that holds it with every tab in
it, except where one of them is at a blocked site. `delete_history_entry` and `remove_bookmark`
name an entry or bookmark by its exact URL; a history entry is one per URL, and `remove_bookmark`
fails and counts where more than one bookmark is at the URL. `delete_history_range` takes two moments
and `clear_browsing_data` takes a moment and a list of kinds of data, each moment written as an ISO
8601 date and time with its offset. The browser reads a missing `since` as 0, which clears all
time, and `history.deleteRange` takes bounds in milliseconds, so neither tool takes a number, a date
without a time or a time without an offset, and a call that leaves out a bound or a kind deletes
nothing. The kinds are the ten of the browser's data set that its reference does not mark deprecated
(`passwords`, `appcache`, `webSQL`, `pluginData` and `serverBoundCertificates` are not among them).
The two tools that delete by time or kind name no site, so the blocked list does not leave
anything out of them. `remove_bookmark_folder` names a folder by its exact title and calls
`bookmarks.remove`, which removes a bookmark or an empty folder, so a folder with anything in it is
left as it is and the failure counts its entries; `bookmarks.removeTree`, which would delete what is
in it, is not called. `remove_reading_list_entry` names an entry by its exact URL and calls
`readingList.removeEntry` (Chrome 120), and the reading list holds one entry for an address.
`delete_downloaded_file` names a download by its exact `filename`, the absolute path the browser
holds, or by the exact `url` it started from, finds it with `downloads.search` and deletes its file
with `downloads.removeFile`, which "removes the downloaded file if it exists and the DownloadItem is
complete" and otherwise fails. It does not call `downloads.erase`, which "erases matching
DownloadItem from history without deleting the downloaded file", so the entry stays in the list with
`exists` false. It does not ask the browser to delete a file the browser says is gone, and a name
that more than one download with a file has names none of them. Each reply says what the browser reports afterwards and carries nothing of the
page, entry, bookmark or download ([BROWSER-31](../specs/browser.md#BROWSER-31)). Whether Brave closes
a tab whose page asks to stay open, whether `tabs.get` and `windows.get` fail for a tab and a window
that are gone, whether `bookmarks.remove` refuses the folders Brave keeps with an error, and how soon
a download's `exists` shows a file deleted by `downloads.removeFile`, were not run in Brave.

`downloads` has seven tools. `save_page_archive` is described under [Saving a page as a
file](#saving-a-page-as-a-file), and the other six are below. The group lists the `downloads`
permission, which Brave asks the person to allow as "Manage your downloads", `pageCapture` and `tabs`
for the archive, and no host. It is on until Brave restarts unless the person chooses
another time, and it is in no preset. `download_file` calls `downloads.download` with the URL,
`saveAs` false and `conflictAction` `uniquify`, and no `filename`, `method`, `headers` or `body`.
The reference says the request "will include all cookies currently set for its hostname" and that
the call resolves with the id of the new item once the download has started, and documents no host
permission for it. The file goes to the Downloads folder, outside the workspace, where the trust map
treats it as unvouched ([TRUST-1](../specs/trust-map.md#TRUST-1)). Because the request carries the
person's cookies, `download_file` applies the rule `open_tab` does: it takes only an HTTP or HTTPS
URL, refuses a blocked site before the browser is asked, and cancels a download whose `finalUrl`
("the absolute URL that this download is being made from, after all redirects") is on another host or
at a blocked origin, deleting a file that had already finished. The allowed sites list does not
apply, since no page is read. `list_downloads` calls `downloads.search` with `orderBy` `-startTime`
and a `limit`, and shows each download's `filename` ("Absolute local path") and `url` ("the absolute
URL that this download initiated from, before any redirects"), which are what the other tools name
a download by, and never its id. `pause_download`, `resume_download`, `cancel_download` and
`show_download` find the download with `downloads.search` by that exact path or address, keep the
ones the call applies to (`state` in progress and not `paused`, `canResume`, `state` in progress, or
`exists` not false), and call `downloads.pause`, `downloads.resume`, `downloads.cancel` or
`downloads.show` for the one that is left. `downloads.pause`, `resume` and `cancel` return a Promise
from Chrome 96, and `show` returns nothing. No tool calls `downloads.open`, which would run the file,
`downloads.acceptDanger`, `downloads.drag` or `downloads.erase`, and none reads a downloaded file
([BROWSER-32](../specs/browser.md#BROWSER-32)). Whether `finalUrl` is set when `downloads.download`
answers, and what the browser says to a `pause` or `resume` of a download in the wrong state, were not
run in Brave.

## Constraints

- **Size:** native messaging caps a message to the extension at 1 MB
  and a reply from it at 64 MB. Return trimmed page text, not full
  HTML, so a page fits the model's context.
- **Availability:** if the extension isn't connected, fail tool calls
  with a clear error instead of hanging.
- **Consent:** the extension's own UI gates every tool that reaches tabs, frames, history or
  bookmarks, and Brave gates the APIs they call: the manifest requires only `nativeMessaging`,
  `storage` and `alarms`, the tools are in groups, and a tool runs only while Brave holds the
  optional permissions of its group ([BROWSER-18](../specs/browser.md#BROWSER-18)). A person turns a
  group on in the options page, which confirms in its own words, asks how long the group stays on
  (until turned off, until Brave restarts, or one hour) and then asks Brave for the group's
  permissions ([BROWSER-19](../specs/browser.md#BROWSER-19)). Each call checks the lifetime, so
  it does not depend on an alarm ([BROWSER-24](../specs/browser.md#BROWSER-24)). A preset turns on a set of
  groups and turns off the rest, with one request for what they list together, limits a group that
  reads pages to the allowed sites, and Off gives back every optional permission and host
  ([BROWSER-20](../specs/browser.md#BROWSER-20)). A frame read
  names both its outer tab URL and its exact frame URL; one approval never widens to every embedded
  origin. A list of blocked sites in the same options page is a second control: no tool lists,
  searches, reads or opens a site on it, whatever is switched on
  ([BROWSER-17](../specs/browser.md#BROWSER-17)). A list of allowed sites beside it limits a
  group that reads pages to the sites on it, unless the person chose all sites for that group, and
  the hosts a group asks Brave for are those of the allowed sites, or the two patterns for all
  sites ([BROWSER-22](../specs/browser.md#BROWSER-22)). Brave's own site access setting can narrow
  what the extension holds further. No documented API reports it, so a call it prevents fails and the
  refusal says it can be the cause. The MCP layer doesn't enforce this.
- **The second extension:** it holds `debugger` from the time it is installed and enabled, since
  Chrome allows no optional `debugger`, so the four groups it serves have no off state that the
  browser holds except turning that extension off in `brave://extensions`. It takes a message from the
  first extension alone, uses the debugger in one file with a frozen list of protocol methods that
  holds no method of `Runtime`, attaches for one call, and sends input or captures only with the
  token of the person's **Allow once** in its own window
  ([BROWSER-37](../specs/browser.md#BROWSER-37), [BROWSER-38](../specs/browser.md#BROWSER-38)). Its
  README says to keep it turned off until it is wanted, and the first extension's **Turn everything
  off** cannot disable it, since that needs `management`, which is not used.
- **Visibility:** the toolbar badge counts the groups that are on and its title lists them
  ([BROWSER-19](../specs/browser.md#BROWSER-19)). The popup of the toolbar action has **Turn
  everything off**, and a keyboard command with no default key does the same; both apply the Off
  preset ([BROWSER-20](../specs/browser.md#BROWSER-20)). The options page lists the last 200 calls
  that reached the extension, with the origin of each URL named and nothing from a page, a query or a
  title, including calls the switches refused
  ([BROWSER-27](../specs/browser.md#BROWSER-27)). The side panel is not used, since Brave's Sidebar
  has no control to open it and opening it from a popup window crashes the browser
  ([BROWSER-12](../specs/browser.md#BROWSER-12)).
- **Organizations:** the permissions of a group are optional, so an organization's
  `ExtensionSettings` policy with `blocked_permissions` leaves the extension installed and makes
  Brave refuse the request for a blocked group's permissions with "Permissions are blocked by
  enterprise policy." The options page shows that group as blocked by the organization and keeps it
  off ([BROWSER-19](../specs/browser.md#BROWSER-19)). `runtime_blocked_hosts` keeps the extension
  from the hosts it lists, and set for the extension it makes `debugger.attach()` fail on every
  target, with "Host access is restricted by policy." The README names both keys. Neither was run
  in a Brave.

## Open decisions

Four questions were for a maintainer of this repository: the `scripts` row of the table of groups of
issue #3583, called the issue below, and its decisions 1, 2 and 5. The issue lists options for its
decisions 1 and 2. For the other two the options are the ones the specs leave. Its decisions 3, 4
and 6 are in the README of the extension. Each entry below gives the question, the options, the
default recommended and what a reversal changes. A default stands until a maintainer rules
otherwise. The table says what is decided and built and what waits.

| Question | State |
|---|---|
| Is code the planner wrote a routing field? (`scripts`) | Decided and built: complete code for one page, asked every call, with nothing limiting it but the person's reading ([ROUTE-11](../specs/routing.md#ROUTE-11)). |
| Does `debugger` live in a second extension? (decision 1) | Decided and built: `input-trusted`, `capture-trusted`, `console-trusted` and `upload-trusted`. |
| How does `input` meet the rule? (decision 2) | Decided and built for a target named by a role and a name, by a number the person gave, or by a number a processor chose (the ninth Known cost, ROUTE-10). Choosing a file for an upload is built ([BROWSER-45](../specs/browser.md#BROWSER-45)). |
| How does a picture or a file reach the person? (decision 5) | Decided and built for a page, the archive of a page, the screen and a recording of a page. Not built: `capture_tab_video`. |

[BROWSER-29](../specs/browser.md#BROWSER-29) records what does not exist and what has to exist
before it does.

### Is code the planner wrote a routing field? (`scripts`, decided)

`run_script` runs code the planner wrote in one page, shown in full before the call, through
`userScripts.execute`. `scripting.executeScript` cannot take a string. [ROUTE-7](../specs/routing.md#ROUTE-7)
builds no tool whose routing field a person could not approve alone, and gives a shell string as its
example because it is destination and payload at once. Code in a page is the same: it names the
requests it makes, the elements it acts on and the data it carries in one string.

`run` takes a command line and compiles it to stages, each a resolved binary and a literal argument
vector. A person approves that plan, and the sandbox confines what it starts (RUN-1, RUN-2). Code in a
page has no such form, and nothing in this tree confines what it does in the page.

- Options. The code is not a routing field, so `run_script` is not built. Or code shown in full and
  approved for one exact page URL is a routing field, as an exact plan is for `run`, and the routing
  spec says what limits its reach.
- Decided and built: the second option, as [ROUTE-11](../specs/routing.md#ROUTE-11) and
  [BROWSER-46](../specs/browser.md#BROWSER-46) state it. A maintainer ruled that bravebot has no
  feature gap against Codex and Claude Desktop, which run code in a page (brave/bravebot#3583). The code is the planner's
  own argument, the person reads every line of it and the exact address in a window of the extension
  before each run, and no rule, mode or earlier answer answers that question. The question BraveBot
  puts before the call draws the code as numbered rows, one to a line, with the characters a person
  cannot see written out ([SERVERS-7](../specs/mcp-servers.md#SERVERS-7)); the desktop window and an
  editor refuse the call with nobody asked, as for every group that is asked about every call.
- What limits the code. The extension configures the world the code runs in with the policy
  `default-src 'none'`. Experiment C-2, run in Brave 1.98.62 and run again for `fetch`, `sendBeacon`,
  frames, form submission and navigation in 1.98.63, showed that the policy stops the code's own
  `fetch`, `XMLHttpRequest`, `sendBeacon`, `WebSocket` and `EventSource` requests, image, link and script
  elements and `import()`, and does not stop a frame's source, a style's image, audio and object
  elements, a worker, a navigation, a form submission, a link click, a meta refresh, or `window.open`
  after the page has had a click. The code reads everything in the page, and the page's own buttons
  and forms work under its control. The one limit on what the code reaches is therefore that the
  person read all of it, and the clause and the window say so. The code starts once and what it sets
  going, such as a timer or a listener, keeps running until the page is closed or reloaded. Brave 1.98.63
  gave the policy to the first ten worlds of a document only, so a probe of the extension's own, a `fetch`
  of a `data:` address, runs in the world of each call before the window opens, and a call is refused
  where the probe is not stopped. Its answer also names the document the code is then run in.
- What the experiment did not cover. The window and the policy in a page of `account.brave.com`,
  `account.bravesoftware.com` or `account.brave.software` (not run, which a local server standing
  in for the name allows, as under "Sites Brave refuses to script"), a store-installed extension (the policy's `script-src` for an unpacked extension names
  `localhost`), a page with a content security policy of its own, and Windows and Linux.
- Gates. `userScripts` is an optional permission, the group reads the hosts of the allowed sites, and
  the person turns on "Allow User Scripts" on the extension's details page (Chrome 138 and later;
  Developer mode before), which the extension cannot do. With the permission held and the switch off,
  Brave 1.98.63 leaves `chrome.userScripts` an object whose methods each throw "is not available in this
  context", so the extension calls `getWorldConfigurations` before it asks anybody, and refuses with
  one sentence that names the switch. A request for the permission resolves granted with the switch off
  and no dialog, and Brave's list of warnings for a manifest that holds `userScripts` is empty.
- `minimum_chrome_version` is 135, the first Chrome with `userScripts.execute`
  ([BROWSER-18](../specs/browser.md#BROWSER-18)).

**Observed through the whole chain, 2026-10-31**, in Brave 1.98.63 (Chromium 156) on macOS 26.6, headless,
with a profile made for the run, a page served from 127.0.0.1 and name resolution for every other host
switched off. Brave read the native messaging host's manifest from a scratch directory, written by
`bravebot-browser install --groups scripts`, and started the real host from it. The real relay
(`bravebot-browser mcp --group scripts`) spoke MCP to a test client, the extension was a copy of
`extension/`, and Allow and Deny were pressed in the real confirmation window with protocol mouse events.
Seen:

- The tool list held one tool, `run_script`, with `url` (at most 300 characters) and `code` (at most
  2,000), both required.
- With the permission held, the group and the tool on and the site allowed, and "Allow User Scripts"
  off, the call was refused in 3 ms and no window opened. The sentence was "run_script is refused until
  Allow User Scripts is turned on for the BraveBot extension on its details page at brave://extensions
  (in a browser older than Chrome 138, until Developer mode is turned on at brave://extensions)". The
  switch was then turned on with `developerPrivate.updateExtensionConfiguration` from a
  `chrome://extensions` target.
- The window showed the code in full: for seven lines holding a tab and a zero width space it gave the
  heading "The code, all of it", "7 lines, 176 characters", the site and the exact address, no section
  for an element, "Nothing is done if you do not answer in 60 seconds.", and one row to a line with each
  line feed, the tab and the zero width space written out as `U+000A`, `U+0009` and `U+200B`.
- Allow once ran the code. The result came back through the relay as one text part with `isError`
  false, and the code had read a field the page hides. The window closed after the answer.
- Deny ran nothing and left the page's title as it was. No answer for 60 seconds failed the call after
  59.9 seconds with the sentence that nothing was done, ran nothing, and closed the window.
- An address other than the tab's was refused before any window opened, in 1 to 2 ms each: the same page
  with another query, another page of the same site, a second origin that was not in the allowed sites,
  and `file:///etc/hosts`.
- The world was new for each call. The first call set a global, a top level `var` and an attribute on
  the page. The second answered `undefined,undefined,yes`, so the two variables were gone and the
  attribute stayed.
- A page that navigated away failed the call in each of three ways. The tab left while the window was
  open: "the tab left the address before the script ran, so nothing was run". The tab left 1.2 seconds
  into a 5 second script: the call failed after 16.6 seconds with the sentence that the page did not
  answer within 15 seconds, and the late answer was not returned. The code navigated its own page: the
  call failed, with the sentence that the tab left while the script ran in one run and the one about 15
  seconds in another.
- The policy `default-src 'none'` stopped `fetch` to another origin and to the page's own, `XMLHttpRequest`
  (status 0), `sendBeacon` (it returned true and the other origin received nothing), `WebSocket` (an
  error, no upgrade received) and an image (not loaded). It did not stop a frame whose address carried the
  hidden field (the request was received), `location.assign` (the request was received and the call
  answered `ran`) or a form submission (the post was received). `window.open` with no click returned
  null and nothing was received.
- A zero width space outside a string or a comment is a syntax error, and the answer was null. A
  top level `return` also answered null.

Not observed. The group was turned on by writing its settings to `chrome.storage.local`, `userScripts`
was requested from the options page with a user gesture from the protocol and granted at once, and the host
permission `http://127.0.0.1/*` was added to the copy's manifest as a required permission, because
Brave's prompt for an optional host permission cannot be answered when headless. The options page's own
flow and the optional host request were not driven, and neither was a click on the details page's toggle.
That the result reaches the planner only as a reference was not run with the real agent; a test with a
stand-in server holds it, and the relay was seen to return the text part. `EventSource`, link and script
elements, `import()`, `srcdoc` and `data:` frames, audio and object elements, blob workers, attribute
handlers, a meta refresh and a link click were not measured again in this run. A headed Brave, Windows,
Linux, a store-installed extension, a page with a content security policy of its own and the three
sites Brave refuses to script were not run.

### Does `debugger` live in a second extension? (decision 1, decided)

Chrome does not allow `debugger` as an optional permission ([declare permissions][chrome-declare]).
Declared in this extension it would put "Access the page debugger backend" and "Read and change all
your data on all websites" in every install prompt (the table in [The warning for a broad host
pattern](#the-warning-for-a-broad-host-pattern)) and would remove the browser-held off state that
every other group has (BROWSER-18). An update that adds a permission with a warning disables the
extension until the person accepts (the issue).

- Options. `debugger` in this extension. Or a second extension with its own id, named in the host
  manifest's `allowed_origins` and in the host's origin check, so that BROWSER-4 names two ids. The
  issue adds that Chrome's single-purpose policy favours separating functions that are clearly
  separate (README of the extension, "Chrome Web Store").
- Decided and built: a second extension that is not a native messaging client.
  `extension-trusted/` holds BraveBot Trusted Input, whose manifest holds `debugger` and nothing else
  and names this extension's id in `externally_connectable`. This extension stays the only native
  messaging client and the only door, so the host manifest still names one id and BROWSER-4, -5 and
  -9 do not change. It checks its own switches, lifetime and site lists and then sends the call of
  `input-trusted`, `capture-trusted`, `console-trusted` or `upload-trusted` to the second extension
  with `runtime.sendMessage` to its id ([BROWSER-12](../specs/browser.md#BROWSER-12),
  [BROWSER-37](../specs/browser.md#BROWSER-37)). The cost is two ids that name each other in
  `extension/trusted.js`, `extension-trusted/pair.js` and `extension-trusted/manifest.json`, and a
  test that derives each from its key. The alternative of a second native client needed `install` to
  record two ids, a check that accepts either, and a rule for two extensions sharing one relay or having
  a relay each.
- The cost that remains. While the second extension is enabled it holds `debugger` on every site, so
  its four groups have no off state that the browser holds except turning the extension off in
  `brave://extensions`. This extension's switches decide whether a call is sent, and the second
  extension asks the person in its own window before any input or capture. Its README says to keep it
  turned off until it is wanted.
- What is left. The second extension has not been run in a headed Brave: its install prompt, the
  debugging notice and its **Cancel**, a click on **Allow once** by a person, the windows' size and
  placement, Linux, a page on a site Brave protects from scripting, and the two extensions with the
  real relay and host. Whether it has a Web Store listing of its own is not decided, and a build with
  another id needs the ids in both extensions changed together. Acting
  in a frame, dragging by a distance and `Runtime.enable` are not built. `Runtime.enable` would give
  the console read the messages of uncaught errors, puts a method of `Runtime` on the list of
  BROWSER-37, and is for a maintainer to decide
  ([BROWSER-44](../specs/browser.md#BROWSER-44)).
- A reversal to `debugger` in this extension removes the second extension, `trusted.js`, the
  forwarding of the four groups and BROWSER-37, -38, -43, -44 and -45, puts the two install lines above in
  every install prompt, and removes the browser-held off state of the groups that use it.

### How does `input` meet the rule? (decision 2, decided)

Clicking, typing, pressing keys, scrolling, answering a dialog, choosing a file and waiting for an
element land where the page decides, as "the Sign in button" does. Routing that comes from page
content is refused (ROUTE-1, ROUTE-2). The planner holds none of the page, so it cannot name an
element in a form that a person can judge in the question before the call. A node id or a selector
read from a page is not such a form, as a tab id is not (`crates/browser/src/tools.rs`).

- Options, from the issue. The person picks the target in the browser, with the gesture of #1573,
  and the pick travels as a reference. Or a plan fixes destinations before anything is read, with
  bounded branches (#3498). Or `input` is not built and `scripts` is the interface.
- None of the three is taken. A pick is a reference that names no file, which ROUTE-6 refuses as a
  destination, and ROUTE-2 lets a reference be named only from a context that has met nothing
  untrusted. Bounded branches change LABEL-5 and MANIFEST-5 and add a branch step to the plan schema
  (#3498 lists them). The third depends on the ruling on `scripts` above.
- Decided. A maintainer ruled that BraveBot does page interaction as Codex and Claude desktop do,
  within the rule. The planner names a page by its exact URL and an element by a role and an
  accessible name, in words from the person or from content a person let through with
  `vet_content`. The extension finds the element and fails where it finds none or several, and the
  person is shown the one it found, in a window of the browser, before anything is done. The server
  is declared with `--ask-every-call`. ROUTE-9 is the clause that says why this meets ROUTE-7.
  [BROWSER-33](../specs/browser.md#BROWSER-33) is the tools,
  [BROWSER-34](../specs/browser.md#BROWSER-34) is the window, and [Page
  interaction](#page-interaction) and [The confirmation window](#the-confirmation-window) say how
  they work.
- Built since. Input events the page cannot tell from a person's, hovering, dragging and a JavaScript
  dialog an action opens are the tools of `input-trusted`
  ([BROWSER-38](../specs/browser.md#BROWSER-38)), a picture of the whole page is `capture-trusted`
  ([BROWSER-43](../specs/browser.md#BROWSER-43)) and a page's console is `console-trusted`
  ([BROWSER-44](../specs/browser.md#BROWSER-44)), all in the second extension. The part of choosing
  the target from what a page shows that needs no change of the rule is built: the person reads a
  numbered list of the page's controls ([Numbering the elements of a page](#numbering-the-elements-of-a-page))
  and gives the number in their own words, which is a word they can check against the line it stands
  for, or lets the model read the list at `vet_content`
  ([Looking at a page](#looking-at-a-page)).
- Built since, as the ninth Known cost. Choosing the target from what a page shows, through a
  processor. A processor reads the numbered list and answers with one number. `bravebot-choice`, a
  crate that is not the driver, reads that number from the answer once
  ([PROC-16](../specs/processors.md#PROC-16)). `Policy::carry_a_choice` carries it into the one
  argument that a person's declaration names with a flag, `--choice tool:argument`, after the person
  answered the call, and the question draws the number and that a processor chose it
  ([ROUTE-10](../specs/routing.md#ROUTE-10), [MCP-14](../specs/mcp.md#MCP-14)). The window shows the
  element as it does for a number the person gave, and the person allows it there. Choosing a file
  for an upload is built as the tool of `upload-trusted`
  ([BROWSER-45](../specs/browser.md#BROWSER-45)). Acting in a frame is not built.
- A reversal removes the six tools from `groups.js` and the relay's table, and ROUTE-9, BROWSER-33
  and BROWSER-34, and puts the entry back in BROWSER-29.

The issue says #1573 records that page interaction "was declined earlier" under the gap id
`parity-browser-page-interaction`. The reason is not recorded in the tree or the tracker. The id is
in no line of `docs/peer-features-reviewed`, which has no line that mentions the browser, in no
file of the tree and in no commit (`git log --all -S` finds none). The only issues and comments that
hold it are #1573 and #3583 (searched 2026-10-30). The reason on record is the rule, ROUTE-2 and
ROUTE-7 as above. A maintainer who knows the earlier reason can add it here.

### How does a picture or a file reach the person? (decision 5, decided for a page, and `screen`)

`screenshot_page` in `page-read`, `save_page_archive` in `downloads`, and `capture_screen` and
`capture_tab_video` in `screen`, return a picture, a file or a video. A result is untrusted and
private (MCP-1), so the planner and the driver may not read it. The issue says only that such a
result has to reach the person's screen.

- Decided for `screenshot_page`. A maintainer ruled that a screenshot is supported under the vetting
  rules and references that exist: the screen is an untrusted reference, and the model looks at one
  only after its content is vetted. MCP-6 takes an `image` part of four types, within 8 MiB, as a
  picture the planner is given a reference to, a processor is given as a picture and `vet_content`
  can let through after a person looked at it. The relay sends the extension's `image` object as
  that part. [BROWSER-35](../specs/browser.md#BROWSER-35) is the tool, and [Taking a picture of a
  page](#taking-a-picture-of-a-page) says what it needs of the browser.
- Decided for `save_page_archive`. A maintainer ruled that files and pictures fall under the normal
  vetting rules and references. The archive is not carried in a result: the extension saves it in
  the person's Downloads folder through `downloads.download`, and the result names the file. The file
  is outside the workspace and the trust map treats it as unvouched, and the model reaches it only
  as it reaches any file the person gives it, by naming it
  ([BROWSER-32](../specs/browser.md#BROWSER-32), [BROWSER-40](../specs/browser.md#BROWSER-40)). The
  client's table of media types is unchanged and MCP-6 takes no file. [Saving a page as a
  file](#saving-a-page-as-a-file) says what it needs of the browser.
- Decided for `screen`. A maintainer ruled that nothing the issue lists is left out and that a screen is
  an untrusted reference like a screenshot. `capture_screen` and `record_gif` are built
  ([BROWSER-42](../specs/browser.md#BROWSER-42)) and return a JPEG or a GIF as the same `image` part.
  [Taking a picture of the screen](#taking-a-picture-of-the-screen) says what each needs of the
  browser. `capture_tab_video` is not built: `tabCapture` is refused unless the person has invoked
  the extension on the tab, and a video is not one of the four types MCP-6 takes. The entry stays in
  [BROWSER-29](../specs/browser.md#BROWSER-29).
- What is left. `capture_tab_video` returns a video of a tab, and MCP-6 takes no video. Where the
  bytes of a video are kept, how each front end shows one and what a processor may do with it are
  not decided. That tool is not built, and its default is that it stays so.
- A reversal of the decision for a page removes `screenshot_page` from `groups.js` and the relay's
  table, `<all_urls>` from the manifest and from `groups.js`, and BROWSER-35, and puts the entry back
  in BROWSER-29. A reversal for `screen` removes the group's tools and permission, `capture.html`,
  `capture.js`, `screen.js`, `gif.js` and BROWSER-42.
- A reversal of the decision for the archive removes `save_page_archive` from `groups.js` and the
  relay's table, `pageCapture` and `tabs` from the line of `downloads`, `pageCapture` from the
  manifest, `ALLOWED_ONLY_TOOLS`, and BROWSER-40, and puts the entry back in BROWSER-29. Moving the
  tool to `page-read` instead puts `pageCapture` and `downloads` in that line, so the Read and Browse
  presets ask for both, and takes the tool out of `ALLOWED_ONLY_TOOLS`, which gives it the scope of
  the group.
