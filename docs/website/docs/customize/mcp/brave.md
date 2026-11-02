---
sidebar_position: 4
title: Connecting Brave
description: Let a session use Brave's tabs, history, bookmarks and more through the BraveBot extension, one group of tools at a time, each off until you turn it on.
---

# Connecting Brave

This page assumes you have read [MCP servers](../mcp-servers.md), which covers
[adding a server](../mcp-servers.md#adding-one), [approving it](../mcp-servers.md#approving-one)
and [what a server can reach](../mcp-servers.md#what-a-server-can-reach).

The BraveBot extension lets a session use Brave. Its tools read what you have open, visited and
saved, the requests a page made, and how Brave and the computer are set. They also organize tabs and
bookmarks, open pages as you, download files, read the names of cookies, keep the computer awake and
delete data. The tools are in groups, and **every group is off until you turn it on** in the
extension's options page and Brave grants the permissions the group needs.

There are three pieces:

- The extension runs in Brave and answers requests.
- `bravebot-browser` is the relay. Brave starts it as the extension loads, and a session starts it
  again as an MCP server that reaches the extension through a socket in `~/.bravebot-browser`.
- Your session declares that server with `bravebot mcp add`, as for any server.

A second extension, BraveBot Trusted Input, does the work of four groups that need the browser's
debugger. [The second extension](#the-second-extension) says what it holds and how to install it.

This page starts from `bravebot-browser` and the extension being installed, and does not say
where to get them. They run on macOS and Linux, in a Brave built on Chrome 135 or later. On Windows
`bravebot-browser` only prints that the platform is not supported and exits, so no server starts and
neither half runs.

1. Write the host manifest and print the declarations. Name the groups you want with `--groups`, or
   a preset with `--preset`:

   ```sh
   bravebot-browser install --preset look
   ```

   ```
   Wrote /Users/you/Library/Application Support/Google/Chrome/NativeMessagingHosts/com.brave.bravebot.json
   Declare a server for each group with:

     bravebot mcp add brave-tabs-read -s user --dir /Users/you/.bravebot-browser -- /path/to/bravebot-browser mcp --group tabs-read
     bravebot mcp add brave-history-read -s user --dir /Users/you/.bravebot-browser -- /path/to/bravebot-browser mcp --group history-read
     bravebot mcp add brave-bookmarks-read -s user --dir /Users/you/.bravebot-browser -- /path/to/bravebot-browser mcp --group bookmarks-read
   ```

   The manifest is `com.brave.bravebot.json`. It names `bravebot-browser` by its absolute path and
   names the BraveBot extension as the only one Brave may start it for, so install again after
   moving the program. On macOS it is written to Chrome's `NativeMessagingHosts` directory, which
   Brave reads there. On Linux it is written to
   `~/.config/BraveSoftware/Brave-Browser/NativeMessagingHosts/`. It also records the extension's id in
   `~/.bravebot-browser` for the relay to check, writes nothing else, and changes no other
   program's manifest.

   - `--groups tabs-read,history-read` prints one declaration for each group named. `--preset look`
     prints one for each group in the preset, which are the same groups the options page's preset
     names. Give one of the two, once. `--groups` and `--preset` change only the lines it prints.
   - With neither, it prints one declaration, for a server named `brave` that offers seven tools: the
     platform check, `list_tabs`, `list_frames`, `read_page`, `search_history`, `search_bookmarks`
     and `open_tab`. That server is granted and approved as one, so you cannot accept listing tabs
     without accepting reading a page. A server for one group offers that group's tools and no
     others, and is granted and approved on its own. The tools added after the first seven are
     offered only by the server of their group.
   - `--manifest-dir <dir>` writes the manifest to a directory you choose instead of the one Brave
     reads. A word without a dash is the id of the extension, 32 letters from `a` to `p`, for a copy
     whose id is not the one below. Nothing is written for anything else.
   - A group or preset it does not know, a flag with no value, or any other word prints the usage,
     the groups and the presets, and writes nothing.

2. Have the extension in Brave. Open `brave://extensions`, where it is listed as BraveBot. Brave
   starts the relay as the extension loads, and again each time Brave starts. A copy loaded unpacked
   from the extension's directory has the id `fcjamhpiedeihbbbpndcjngkjaogfgep`, which `install`
   records when you give it no id. A copy with another id needs that id:
   `bravebot-browser install <extension id>`.

3. Run the `bravebot mcp add` lines it printed. Each shows what it declares and then asks whether to
   use the server. Answer yes once the line is what you expect. A group's line starts the server
   with `mcp --group <group>`, and `--dir` is the directory that holds the socket. For `remove-data`
   and `cookies`, and for `scripts`, `input`, `input-trusted`, `capture-trusted`, `console-trusted`,
   `upload-trusted` and `screen`, the line also has `--ask-every-call`, so that
   [every call is asked about](../mcp-servers.md#servers-that-ask-every-call) and no answer is
   remembered. The last seven also have `--tool-timeout 120`, which lets a call take two minutes
   while you answer in a window or a picker of the browser. The `input` line also has
   `--choice click_element:element_number`, `--choice type_text:element_number` and
   `--choice select_option:element_number`, which name the argument that takes an element's number
   as one that [takes a choice](../mcp-servers.md#arguments-that-take-a-choice). A session that is already running keeps
   the property its server started with, so declare these groups before you start it. Like any server, each one is then read to you before its
   tools are [offered to the model](../mcp-servers.md#offering-its-tools-to-the-model), and again if
   its list changes, and every call is put to you.

4. Check that it answers. Start a session and ask it to call `brave-platform:get_platform_info`,
   or `brave:get_platform_info` where you declared the server with no group. If you did not name
   `platform`, `bravebot-browser install --groups platform` prints its line. The tool returns the
   operating system and architecture Brave runs on and nothing about you, and it is the only tool
   that is on until you turn others on. If no extension is connected the call fails at once and
   says so: Brave is not running, the extension is off or not loaded, or Brave could not start the
   relay.

5. Turn on the groups you want, as the next section describes.

## Turning a group on

In `brave://extensions`, choose **Details** on BraveBot, then **Extension options**. Each group has
a switch. Choosing it does not turn the group on. The page says what the group lets a session do,
what comes back and what Brave will ask you to allow, and asks how long the group stays on. **Turn
on** then asks Brave for the group's permissions, and Brave shows its own prompt for those that
carry a warning. If you decline, the group stays off. Once the group is on, its tools are on, and
opening the group shows a switch for each tool. A switch saves as it changes, and the next call uses
it.

A call to a tool is refused before Brave is asked anything unless the tool's switch is on, its
group is on and has not run out of time, and Brave holds every permission of the group. If you take
a permission back in Brave's extension settings, the groups that use it turn off. Turning a group off
gives back its permissions, except those another group that is on still uses.

## The groups

| Group | A session can | Preset | Asked every call |
|---|---|---|---|
| `platform` | read the operating system and architecture Brave runs on. Always on | `off` | no |
| `tabs-read` | list your open tabs, windows and tab groups, the tabs and windows you closed lately, and the frames in a tab | `look`, `read`, `browse` | no |
| `history-read` | search your history, list the visits to one address, your most visited sites and the tabs open on your other devices | `look`, `read`, `browse` | no |
| `bookmarks-read` | search your bookmarks, list the contents of a folder (including the Bookmarks bar and Other bookmarks, named by the title Brave shows) or the folders at the top, your newest bookmarks and your reading list | `look`, `read`, `browse` | no |
| `page-read` | read the text of an open page or one frame, up to 100,000 characters, the links in a page, and where its forms submit with the name and type of each field and never what is typed in one. Also number the buttons, links and fields of a page, up to 40, for you to name one to the session, and take a picture of an open tab (a JPEG, with the group on all sites). The model sees the picture only if you look at it and agree | `read`, `browse` | no |
| `tabs-manage` | switch to, pin, mute, move, group, unload, duplicate and zoom open tabs, bring a window forward and resize it | `browse` | no |
| `bookmarks-write` | add bookmarks, folders and reading list entries, move and rename bookmarks, and change reading list entries. It deletes none | `browse` | no |
| `navigate` | open an address in a background tab or a new window as you, take an open tab to another address, reload it, move it back or forward, reopen a closed tab, search with your default search provider and wait for a tab to load | `browse` | no |
| `downloads` | download an address into your Downloads folder with your cookies for that site, list, pause, resume, cancel and show downloads, and save an open page at a site you allowed as a file in your Downloads folder, which the model does not read unless you give it the file | none | no |
| `cookies` | list the name, domain, path, flags and expiry of the cookies Brave sends to an address, and delete one by name. It never reads or sets a value | none | yes |
| `network-watch` | list the address, type and status of the requests a page made since it loaded, with no header, cookie or body | none | no |
| `browser-info` | read Brave's privacy, content and font settings, the computer's processor, memory, displays and storage, whether you are idle, and whether the extension may run on `file:` addresses and in private windows. Also keep the computer awake for up to 240 minutes | none | no |
| `remove-data` | close tabs and windows, delete history entries and ranges, bookmarks, empty bookmark folders, reading list entries and downloaded files, and clear browsing data. Deleted data cannot be brought back | none | yes |
| `held` | fetch what you held out for it in Brave's context menu: text you selected, a link, or a tab. It reads no page | none | no |
| `input` | click, type in, choose in and press a key in an element of an open page, scroll it and wait for an element, as you. Each action on an element opens a window in Brave that shows it and waits for you to press Allow once | none | yes |
| `input-trusted` | click, double click, right click, hover, drag, type and press a key in an element of an open page with real input that the page cannot tell from yours, done by the separate BraveBot Trusted Input extension. Each action opens a window of that extension that waits for you to press Allow once. It reaches only the sites you allow | none | yes |
| `capture-trusted` | take a picture of the whole of an open page, from top to bottom and left to right, including the parts out of view and a tab in the background, as a JPEG of at most 2048 pixels on its long side and 8 MiB. The BraveBot Trusted Input extension takes it after you press Allow once in its window, which shows the site and the size of the page. The picture comes back to you, and the model sees it only if you look at it and agree. It reaches only the sites you allow | none | yes |
| `console-trusted` | read the console of an open page: what its scripts wrote with `console.log` and the others, the errors they threw and what Brave reported about the page, such as a request that failed. The separate BraveBot Trusted Input extension reads it after you press Allow once in its window, which shows the site. The text comes back to you, and the model reads it only if you agree after reading it. A script's error that nothing caught shows without its message. It reaches only the sites you allow | none | yes |
| `upload-trusted` | give a file of yours to a file input of an open page, as if you had chosen it in the browser's file picker. The file has to be in a directory named `uploads` inside the directory the server is declared with (`~/.bravebot-browser` unless `BRAVEBOT_BROWSER_DIR` names another), which you make and put the files in. A file over 10 MiB, one whose name looks like a credential (`.env`, `*.pem`, `*.key`, `id_rsa*`, `credentials*`, `.ssh/`, `.aws/` and the like), a directory, a file with more than one hard link and a link that leads out of the directory are refused before anything is asked. The separate BraveBot Trusted Input extension asks Brave to read the file, after you press Allow once in its window, which shows the site, the input and the file's name, size and full path. A page whose script sends a file when it is chosen has it at once. An input the page hides is chosen on as well, if the page's own label, `aria-label` or `title` names it, or its `id` or `name` where nothing else does, and the window says that the page hides it; one that a button opens by script and that nothing names is not found. It reaches only the sites you allow | none | yes |
| `screen` | take one picture of a screen or a window you choose in the picker Brave shows for each call, and record an open page as an animated GIF of up to 20 pictures. The model sees neither picture unless you look at it and agree. A recording needs the group on all sites | none | yes |
| `scripts` | run JavaScript the model wrote in the top document of an open page, as you, with the session you are signed in with. The code is at most 2,000 characters. Before it runs, the extension opens a window that shows the site, the address and every line of the code, with each line feed and each character you cannot see written out, and the code runs only after you press Allow once. The question BraveBot asks in the terminal before that draws the same code one numbered row to a line, with each character you cannot see written out, and the desktop app and an editor do not ask, so they refuse the call. Read all of it: nothing limits what it does except that you read it. It can read everything on the page, click and submit its forms, and send what it read to another address. The code starts once. Timers, listeners and observers it sets keep running in the page until the page is closed or reloaded, and each can read and send without asking again. Brave stopped the code's own `fetch`, `XMLHttpRequest`, `sendBeacon` and `WebSocket` requests in the world it runs in, and did not stop a frame that loads an address, a navigation or a form submission. Brave did this for the first ten calls on one page, so the extension checks that it did before it asks you, and refuses the call when it did not: reload the page and ask again. What the code answers comes back to you as text of up to 100,000 characters, and the model reads it only if you agree after reading it. It needs **Allow User Scripts** to be on for the extension: in a Brave built on Chrome 138 or later, open `brave://extensions`, choose **Details** for BraveBot and turn on **Allow User Scripts**; in an older Brave, turn on **Developer mode** there. Until you do, the tool is refused and says so. It reaches only the sites you allow | none | yes |

A tab,
window, bookmark, folder or download is named by its address, title or path. No tool takes a tab id,
a node id or a selector read from a page. The question for a call shows the server and the
tool, such as `brave-tabs-read:list_windows`, and then the arguments.

### Naming an element by number

A tool of `input` names an element by its role and accessible name, or by a number. To use a number,
turn on `page-read` and `input`, and ask the session to list the elements of the page. The list is
a result, so you read it in the transcript and the model does not. Each line has a number, a role,
the element's name and the site a link goes to, and nothing a field holds. A password field, a
payment card field, a file chooser, a disabled field and an element that is not shown are left out,
and the first line says how many. Tell the session which line to act on, such as "click number 7".
The extension finds that element again, shows it to you in a window of Brave and acts only if you
press Allow once there. It refuses the number, and does nothing, when the page has changed so that
the element is no longer the one listed, when the list is more than 5 minutes old or was made for
another page, and when no list was made. A second list replaces the first, and turning off either
group, which **Turn everything off** does, forgets it.

## Looking at a page

A session reads a page, a list of its elements, a picture or a console only after you let it through.
What a tool returns is held as a reference that names its type and size, and the model reads nothing
of what it holds. Two things change that:

- **`vet_content`.** The model names the reference. BraveBot shows you the exact text, or a copy of
  the picture to open in your own viewer, and the model reads it once if you answer yes. With
  [auto-vetting](../../security/vetting.md#auto-vetting-is-off-until-you-turn-it-on) on, a check by a
  second model answers for you when it finds nothing. Nobody reads the content then, and the check
  can be wrong. A delegate is not offered `vet_content`, so it cannot look at a page.
- **Your own message.** You read the result in the transcript and write what the session needs.

Until then the model reads what you wrote, the files and settings you vouched for, and nothing that a
tool of the browser returned.

A session that has looked at a screenshot can act on the page. A round has four steps:

1. **Look.** The model calls `screenshot_page`, which needs `page-read` on all sites, or
   `screenshot_full_page`, and then `vet_content` on the picture. You open the copy, look at it and
   answer yes. The model sees the picture once.
2. **Name.** The model writes the role and accessible name of an element in its call, from what it
   saw, such as a button named "Pay now". It can write an element number instead, from the list
   in [Naming an element by number](#naming-an-element-by-number), which you read and told the
   session, or which you let the model read at `vet_content`.
3. **Confirm.** BraveBot asks you about the call in the terminal. The extension finds the one
   element, brings its tab forward, outlines it and opens a window of the browser that shows the
   element and where it leads. Nothing happens until you press **Allow once** there.
4. **Act.** The extension acts on that element if the page still shows it as you were shown.

A round costs one vetting step for each look and two answers for each action on an element, the
question in the terminal and the window in the browser. A second look is a second vetting step, since
a new screenshot is a new reference and a yes covers one reference once. Scrolling and waiting open no
window. The call that took the picture is asked about as well, unless a rule or an earlier answer
covers it. With auto-vetting on, your part of a round is the confirmation. A one-shot run, a delegate
and the desktop application cannot ask the questions, so they refuse an action on an element.

No tool takes a selector, a node id or a coordinate from a page, so the model's part is the words of
step 2, and the page decides which element they lead to. You see that element before anything is
done to it. What the model reads from a look stays private, so the arguments it writes afterwards are
private and every call is asked about whatever a rule says.

## The second extension

`input-trusted`, `capture-trusted`, `console-trusted` and `upload-trusted` are done by a separate extension, BraveBot
Trusted Input. The tools need the browser's debugger, and Chrome does not let an extension ask for
that permission later, as the BraveBot extension does for every group. The BraveBot extension checks
the group's switch, its time and the site lists, and then sends the call to the second extension.

- **What it holds.** The `debugger` permission and nothing else. Brave lists it as accessing the
  page debugger backend and as reading and changing your data on all websites. It acts only when the
  BraveBot extension asks, and only on a page you allowed, after you press **Allow once** in a window
  of its own.
- **It has no off switch in Brave except turning it off.** The BraveBot extension's **Turn
  everything off** stops its requests and cannot disable the second extension. **Keep it turned off
  in `brave://extensions` until you want one of the four groups.**
- **Installing it.** Load it unpacked from the `extension-trusted` directory beside the BraveBot
  extension, as in step 2 above, with **Developer mode** on. Its id is
  `lmdelobjcmmjoapnmnkfpmjlfikgepbl` and the BraveBot extension's is
  `fcjamhpiedeihbbbpndcjngkjaogfgep`. Each extension names the other by its id. A copy of either with
  another id, such as a build from the Chrome Web Store, is refused by the other until the ids in both
  are changed. It is not a native messaging client, so `bravebot-browser install` records only the
  BraveBot extension's id, and there is no second host.
- **If it is missing.** A call to one of the four groups fails with one sentence that says the
  extension is not installed, is turned off in Brave, or does not accept the BraveBot extension.
  Brave words the three alike. Nothing else changes.

What its window asks you depends on the tool:

| Tool | What it does | What the window shows |
|---|---|---|
| `click_element_trusted` | click, double-click or right-click an element | the action, the page's address, and the element's role, name, start of its text and where it leads |
| `type_text_trusted` | replace the text of a field | the element, and the text in full |
| `press_key_trusted` | press one key on the element that has the focus | that element and the key |
| `hover_element` | move the pointer over an element | the element |
| `drag_element` | drag one element onto another | both elements |
| `screenshot_full_page` | take a picture of the whole page | the site, the address and the size of the page |
| `read_console` | return the newest console entries, 100 unless the call says up to 200 | the site, the address and what is read |
| `choose_file` | give a file from the `uploads` directory to a file input | the site, the address, the file input, and the file's name, size and full path |

In each window **Deny** has the focus, **Allow once** is available a second after the window is in
front and only for a click you make, and Deny, Escape, closing the window and 60 seconds without an
answer all deny. A page that opens a dialog while input is sent gets a panel of the extension, which
shows you the dialog and has **Accept** and **Dismiss**, and a field for your own answer to a prompt.
Leaving it unanswered leaves the dialog open in the page and fails the call.

It refuses, and does nothing, when no element or several have the role and name, when another
element covers the element's middle, for a password field, a payment card field and a disabled or
read-only element, for a file chooser unless the tool is `choose_file`, when the page changed after it
showed you the element, and when the page has more than 100,000 nodes. `choose_file` refuses every
element that is not a file input. Elements in frames are not found. A picture is refused over 8 MiB,
and a long side over 2048 pixels is scaled down.

While it works on a tab Brave may show a notice that the extension started debugging the browser,
with a **Cancel** button. That notice and the install prompt were not seen in Brave.

## Presets

The top of the options page offers four presets, each a set of groups turned on with one request to
Brave:

| Preset | Turns on |
|---|---|
| **Off** | only the platform check. Every other group is turned off |
| **Look** | `tabs-read`, `history-read` and `bookmarks-read` |
| **Read** | Look, and `page-read` |
| **Browse** | Read, and `tabs-manage`, `bookmarks-write` and `navigate` |

Choosing a preset other than **Off** shows what each group lets a session do, what Brave will ask
you to allow for all of them together and the groups the preset will turn off, and **Turn on**
makes the request. A preset turns off every group it does not name. **Read** and **Browse** limit
`page-read` to the allowed sites even if you had chosen all sites for it. `remove-data`, `cookies`,
`downloads`, `network-watch`, `browser-info`, `scripts`, `input`, `input-trusted`, `capture-trusted`, `console-trusted`, `upload-trusted`, `screen` and `held` are in no preset, and each is turned on by
its own switch. **Off** needs no confirmation. It turns off every group and gives back every optional
permission and site Brave holds for the extension.

## How long a group stays on

When you turn a group on you choose **Until I turn it off**, **Until Brave restarts** or **For one
hour**. The default is until you turn it off, except for `downloads`, `cookies`, `remove-data`, `scripts`, `input`, `input-trusted`, `capture-trusted`, `console-trusted`, `upload-trusted`, `screen` and `held`,
which default to until Brave restarts. A call is refused as soon as the time has ended. The
extension then turns the group off and gives back its permissions, within a minute while Brave runs
and at the next start otherwise. Reloading or updating the extension also ends a group turned on
until Brave restarts.

## Sites

**Allowed sites** limits the groups that read pages, which are `page-read`, `cookies`,
`network-watch`, `input` and `screen`. Such a group reaches only the sites on the list, and nothing
is allowed at first, so turning one on reaches no page until you add a site. `scripts`, `input-trusted`,
`capture-trusted`, `console-trusted` and `upload-trusted` are limited to the list whatever you choose, and the options
page offers no choice of all sites for them. Write one site to a line, such as
`https://news.example.com`, and choose **Save allowed sites**. Brave asks for access to those
sites only from that click, and access to a site you take off the list is given back. Instead of a
list you can choose **All sites except the blocked ones** for a group, and Brave then shows its
warning for access to all websites.

**Blocked sites** is a list you write in the same way, and it saves when you click outside the box.
No tool lists, searches, reads or opens a
site on it, whatever is switched on. Tabs, frames, history entries and bookmarks at a blocked site
are left out of results, so a list can hold fewer rows than Brave has, and a call that names one is
refused with a message that names the list and not the site. A site on both lists is blocked.

A site is matched as written: `https://example.com` does not cover `https://www.example.com` or
`http://example.com`, so list each one you use. The cookie tools match by host instead: a cookie for
`.example.com` counts as the cookie of a blocked `https://mail.example.com`, and a list leaves it
out whichever host the address names. A line that is not one `http://` or `https://`
address is not accepted, the page says which, and the list saved before still applies. The groups
that read no page have no scope. `navigate` and `downloads` can load or fetch a page at a site you
did not allow, and only **Blocked sites** stops them. `delete_history_range` and
`clear_browsing_data` name no site, so no list limits them.

Saving a page as a file is a tool of `downloads`. It asks Brave for a permission that Brave words as
access to all websites, so turning on `downloads` shows that line beside "Manage your downloads",
and the pages that are saved are limited to the sites under **Allowed sites** only because the
extension checks them. The group also holds the permission to read the title and address of your open
tabs, which the tool needs to find the page by its address, and Brave shows no line of its own for it
beside the line for all websites. `downloads` has no choice of all sites, so a site has to be on that list for
its page to be saved, and the other tools of `downloads` are not held to it. The file is named for
the site, goes to your Downloads folder and stays there until you delete it, and any program of
yours can read it. It holds whatever the tab held, including what a frame from another site showed.
**Read** and **Browse** do not turn on `downloads`, so they ask Brave for neither the permission to
save pages nor the one to manage downloads.

Brave has its own **Site access** setting for an extension. It can withhold a site you allowed here,
and a call it prevents fails with a message that says the setting can be the cause.

## Seeing and ending what is on

The BraveBot button in the toolbar shows a badge with the number of groups that are on, and its
tooltip lists them. Choosing it opens a popup with the groups that are on and **Turn everything
off**, which does what the **Off** preset does. The extension also has a keyboard command for the
same, **Turn every BraveBot group off**, with no key set. To give it one, open
`brave://extensions/shortcuts`, find it under BraveBot and press the keys in its box.

The options page ends with **Activity**, the last 200 calls that reached the extension, newest
first. Each shows when it arrived, the group, the tool, whether it ran, was refused or failed, and
the site of any address it named. It keeps no page text, search text, title, path or query. It lists
calls whichever session made them and whether or not you are watching it, and a call to a group
that is off is listed as refused. **Turn everything off** does not clear it.

## A Brave profile of its own

A session can use Brave without your signed-in profile if the extension is loaded in a profile made
for it. `bravebot-browser install` is still run first, because Brave reads the host manifest from
one place whatever profile it runs on. Then run:

```sh
bravebot-browser profile --profile-dir ~/.bravebot-profile --extension-dir /path/to/extension
```

It makes the directory, private to your account, and prints one command that starts Brave on it with
the extension loaded unpacked. Run the command in your terminal. `bravebot-browser` prints it and
does not run it. `--brave <path>` names the Brave to start. Without it, `profile` uses the Brave in
`/Applications` on macOS and the first of `brave-browser` and `brave` on `PATH` on Linux.

The profile is a new Brave profile. It has none of your history, bookmarks, logins or settings, and
Brave is told to skip its first-run tasks, among them the offer to import them. The extension's
switches belong to the profile, so every group is off in it until you turn it on in that profile's
options page, and the declarations `install` printed are the same, since both profiles reach the
same relay. Only one Brave profile is connected to the relay at a time. If the extension is also
running in your regular profile, the one that connected first keeps it, and the other connects
within a minute of the first one disconnecting.

Keep the profile out of `~/.bravebot-browser`. A server declared with `--dir ~/.bravebot-browser`
may write everything in that directory, and the profile holds the history and settings of the
browser on it.

`profile` refuses a directory that holds `Default` or `Local State` and has no `bravebot-profile`
file, because Brave leaves those in a profile somebody uses. It also refuses an extension directory
with no `manifest.json` or with a comma in its path, and writes nothing in any of these cases. If
the directory holds `SingletonLock`, a Brave may already be running on it, and a Brave started on it
again does not start a second browser. If your Brave ignores `--load-extension`, load the extension
directory in that profile's `brave://extensions` yourself.

## Holding something out

The `held` group lets you hand a session a piece of a page without it reading the page. While the
group is on, Brave's context menu has three more entries: **Hold this selection for BraveBot** on
selected text, **Hold this link for BraveBot** on a link and **Hold this tab for BraveBot** on the
page. Using one keeps what Brave gives the extension for it: the text you selected, the address of
the link, or the address of the page, with the address of the page you used it on. The extension
reads nothing from any page, and the group asks Brave for the `contextMenus` permission alone, which
shows no prompt.

You then tell the session, in your own words, to look at what you held. It calls
`brave-held:take_held`, which takes no argument, returns the items in the order you held them and
forgets them. Up to 10 items are kept for 30 minutes, in memory that Brave never writes to disk, and
the toolbar popup says how many are held. They are also forgotten when you turn the group off, press
**Turn everything off** or restart Brave. A selection is cut at 20,000 characters and an address at
2,000, an item held at a site on your blocked list is left out, and a tab is held as its address.

The text is the page's, including text the page hides inside the range you selected, and holding it
out does not make it yours. It reaches the session as any result does, as a reference the model does
not read. A processor can answer a question about it, and `vet_content` shows you the exact text, which
the model reads only if you agree after reading it. A call to `take_held` is asked about like a page
read and not every time. Add `--ask-every-call` to the declaration of `brave-held` to be asked each
time.

## What a session gets back

What the extension returns reaches the session as any server's result does: it is untrusted and
private, and the model is handed a reference to it and not the text. You read it in the transcript
and give the session what it needs in a later message, or you let the model read it at
`vet_content` ([Looking at a page](#looking-at-a-page)). The result of `list_frames` is shown to you
in the same way, and you provide the frame address you choose in your next message for
`read_page`. See [what a server returns](../mcp-servers.md#each-call) and
[vetting](../../security/vetting.md).

## What you have handed over

- **A group that is on lets a session use Brave as you.** `navigate` and `downloads` load an address
  with your cookies for its host, and `page-read` reads pages you are signed in to, at the sites you
  allowed. If the page ends on another host, `navigate` closes the tab it made, after the request has
  been made.
- **Only `remove-data`, `cookies`, `scripts`, `input`, `input-trusted`, `capture-trusted`,
  `console-trusted`, `upload-trusted` and `screen` are declared to ask every call.** For the others, an `allow` rule or answer 2 covers
  the later calls to a tool with whatever arguments they carry, so `download_file` and the `navigate`
  tools can load a session's chosen address with no question. Add `--ask-every-call` to a group's
  declaration to be asked each time.
- **The second extension holds the debugger on every site while it is turned on.** It sends no input
  and takes no picture without your **Allow once**, and the page may still do something other than
  what its window showed: a real click lands at a point, and a page can put another element there
  between the check and the click.
- **`remove-data` deletes.** `close_window` closes every tab of the window, at any site, and is
  stopped only by a tab at a blocked site. Nothing here brings deleted data back.
- **The socket's secret is in `~/.bravebot-browser`.** Any process of yours that can read that
  directory can send the extension requests, and is not held to the group a server was declared for.
  What stops it is the checks at each call, described under turning a group on and under sites, and
  the call is in **Activity**.
- **One Brave profile at a time.** A second profile with the extension starts a second relay, which
  finds the first one running and exits.
- **A refusal says which list holds a site,** so a session that guesses an address learns whether its
  origin is on a list. It learns nothing of which tabs are open.

## What has not been verified

- Most of the extension was run against a stand-in for Brave's `chrome` object that follows
  Chrome's documentation. A run in Brave 1.98.62 on macOS covered what a fresh install holds and
  how the permission calls answer.
- The warning lines Brave reports for the permissions of each group were read in Brave 1.98.62
  through its extension API. The permission prompts Brave shows for a group that warns, and for a
  site, were not run in a real Brave. Nor were the toolbar badge, the popup, the keyboard command and its settings page, or what
  the **Site access** setting does to an optional permission.
- The tools of `browser-info`, `remove-data` and `downloads` were run against that stand-in only.
- BraveBot Trusted Input was run in a headless Brave 1.98.62 on macOS. The install prompt, the
  notice that an extension is debugging the browser and its **Cancel**, a click on **Allow once** by a
  person, the size and placement of its windows, and the two extensions together with the real relay
  were not seen, and Linux was not run.
- Linux and Windows were not run for the extension. On Windows `bravebot-browser` only prints that
  the platform is not supported and exits, so no server starts and neither half runs.
- The profile command was run in a headless Brave 1.98.62 on macOS: the extension loaded from the
  command it printed and started the relay's host there. It was not run on Linux.

## Removing it

1. `bravebot mcp remove brave`, and `bravebot mcp remove brave-<group>` for each group you declared.
2. Choose **Remove** on BraveBot in `brave://extensions`. The relay stops with it, and the
   permissions it held go with it. Chrome's documentation says it clears an extension's storage on
   removal, which holds the switches, the lists of sites and the activity log. That was not run in
   Brave. If you loaded BraveBot Trusted Input, choose **Remove** on it there as well. Turning it off
   is enough to take its `debugger` permission away.
3. Delete the host manifest `install` wrote, `com.brave.bravebot.json`, from
   `~/Library/Application Support/Google/Chrome/NativeMessagingHosts/` on macOS or
   `~/.config/BraveSoftware/Brave-Browser/NativeMessagingHosts/` on Linux.
4. Delete `~/.bravebot-browser`.
5. If you made a profile with `profile`, quit the Brave on it and delete its directory.
