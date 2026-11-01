# The BraveBot extension

Lets a BraveBot session use Brave in groups of tools. They read what is open, visited and saved,
the requests a page made and how Brave and the computer are set, organize tabs and bookmarks,
navigate and search, download, read the names of cookies, keep the computer awake and, in
`remove-data`, delete data. Each group is off until you turn it on, and the permissions it needs are
requested then. The extension answers requests from `bravebot-browser`, the relay in
[crates/browser](../crates/browser), over native messaging.
[docs/specs/browser.md](../docs/specs/browser.md) is what the two halves owe each other, and
[docs/design/browser-extension.md](../docs/design/browser-extension.md) is why it is built this way.

A second extension, [BraveBot Trusted Input](../extension-trusted/README.md), holds the browser's
`debugger` permission and does what this one cannot: real mouse and keyboard input, a picture of the
whole of a page, a page's console and a file chosen on a page's file input. Four groups, `input-trusted`,
`capture-trusted`, `console-trusted` and `upload-trusted`, send their calls to it, and no other group needs it.

## Trying it

On macOS or Linux, from the repository root:

1. Build the relay and install its host manifest where Brave reads it:

   ```sh
   cargo build --release -p bravebot-browser
   target/release/bravebot-browser install
   ```

   The last line it prints is the declaration for step 3. The manifest names the program by its
   path, so install again after moving it.

2. Load the extension. Open `brave://extensions`, turn on **Developer mode**, choose **Load
   unpacked** and pick this directory. It needs a Brave built on Chrome 135 or later. The `key` in
   `manifest.json` fixes its id at `fcjamhpiedeihbbbpndcjngkjaogfgep` wherever it is loaded from,
   which is the id `install` records. Brave starts the relay as the extension loads, and again each
   time Brave starts.

   To use `input-trusted`, `capture-trusted`, `console-trusted` or `upload-trusted`, load the second extension the same
   way, from `extension-trusted/`. Its id is `lmdelobjcmmjoapnmnkfpmjlfikgepbl`. It holds `debugger`,
   which Chrome does not let an extension ask for later, so it has no off state in Brave except
   turning it off in `brave://extensions`: keep it off until you want one of those groups.
   [Its README](../extension-trusted/README.md) says how it is paired with this one, what it asks you
   and what it refuses.

3. Declare the server with the line `install` printed:

   ```sh
   bravebot mcp add brave -s user --dir ~/.bravebot-browser -- <path to>/bravebot-browser mcp
   ```

   To declare some of the tools and not the rest, run `install` with `--groups tabs-read,history-read`
   or with `--preset look`, and it prints one such line for each group, naming the server
   `brave-<group>` and ending in `mcp --group <group>`. Each is granted and approved on its own, and
   offers the tools of its group alone. The groups are the ones in `groups.js`, and the platform
   check of step 4 is in `platform`. The presets are `off`, `look`, `read` and `browse`. `install`
   takes `--groups` or `--preset`, not both and not either twice; with a group or preset it does
   not know, or any other word it does not take, it prints the usage and the groups and presets, and
   writes nothing. `--manifest-dir <dir>` writes the host manifest to a directory of your choosing
   instead of the one Brave reads, and a word without a dash is the id of the extension, for a copy
   whose id is not the one in this directory. For `remove-data`, `cookies`, `scripts`, `input`,
   `input-trusted`, `capture-trusted`, `console-trusted`, `upload-trusted` and `screen` the line also has
   `--ask-every-call` after `-s user`, and BraveBot then asks about every call to that server and
   remembers no answer. A one-shot run, a delegate and the desktop app cannot ask, so a call to
   any of these groups from one of them is refused. A session that is already running keeps the
   property its server started with, so declare these groups with the flag before you start it.
   The lines for `scripts`, `input`, `input-trusted`, `capture-trusted`, `console-trusted`, `upload-trusted` and `screen` also have
   `--tool-timeout 120`, which lets a call take two minutes: the server waits 90 seconds for the
   extension, and you have 60 to answer in the browser. The line for `input` also has a `--choice`
   for each of its tools that takes an element number, `click_element:element_number`,
   `type_text:element_number` and `select_option:element_number`.

4. Check it answers. Start a session and ask it to call `brave:get_platform_info`, or
   `brave-platform:get_platform_info` where you declared groups, which returns the operating system
   and architecture Brave runs on and nothing about you. It is the only tool on until you turn
   others on.

5. Turn on the groups you want. In `brave://extensions`, choose **Details** on BraveBot, then
   **Extension options**. Each group has a switch. Turning it on shows what the group lets a session
   do and what comes back, and asks how long it stays on: until you turn it off, until Brave
   restarts, or for one hour. **Turn on** then asks Brave for the group's permissions, and Brave
   shows its own prompt for those that warn. If you decline, the group stays off. Once the group is
   on, its tools are on, and opening the group shows a switch for each tool. A switch saves as soon
   as it changes, and the next call uses it.

   A group that reads pages, such as `page-read` or `cookies`, reaches no site until you add one
   under **Allowed sites** on the same page and choose **Save allowed sites**, or choose **All sites
   except the blocked ones** for the group.

   To turn on several groups at once, choose a preset at the top of the page: **Look** (tabs,
   history and bookmarks), **Read** (Look, and the text of pages you have open at the allowed
   sites) or **Browse** (Read, managing the tabs you have open, adding and changing bookmarks and reading list
   entries, and opening a page in a new tab, taking a tab to another page and searching the web).
   Neither takes a picture of a page, which needs all sites.
   The page shows what each of its groups lets a session do, and **Turn on** then asks Brave once
   for everything they need. A preset turns off every group it does not name. **Read** and
   **Browse** limit the group that reads pages to the allowed sites, even if you had chosen all
   sites for it, and give back the access to all sites. **Off** turns off every group and takes back
   every optional permission and site Brave holds for the extension, including the sites you allowed
   while no group was on. The allowed sites stay in the list, and turning on a group that reads pages,
   or choosing **Save allowed sites**, asks Brave for them again.

6. See what is on. The toolbar button of BraveBot shows a badge with the number of groups that are on,
   and its tooltip lists them. Choosing the button opens a popup with the groups that are on and
   **Turn everything off**, which turns off every group and gives back every optional permission and
   site, as **Off** does. The extension also declares a keyboard command for the same, with no default
   key, so it takes none that you or Brave already use. To give it one, open
   `brave://extensions/shortcuts`, find **Turn every BraveBot group off** under BraveBot and click in
   its box to press the keys. The options page ends with **Activity**, the last 200 calls that reached
   the extension, newest first: when, the group, the tool, whether it ran, was refused or failed, and
   the site of any address it named. It keeps no page text, search text or title, lists a call to a
   group that is off as refused, and is not cleared by **Turn everything off**.

A session offers `brave:get_platform_info`, `brave:list_tabs`, `brave:list_frames`,
`brave:read_page`, `brave:search_history`, `brave:search_bookmarks` and `brave:open_tab`. The tools
added since, such as `read_page_links`, `focus_tab` and `group_tabs`, are offered by the server of
their group alone, so declare that group with `--groups` to use them. It asks you to accept that list
the first time, and again whenever the list changes, then asks you before each call, as it does for
any server.

### A Brave profile of its own

To let a session use Brave without your signed-in profile, give the extension a profile of its own.
After step 1, run:

```sh
target/release/bravebot-browser profile --profile-dir ~/.bravebot-profile --extension-dir extension
```

It makes the profile directory, private to your account, and prints one line: the command that
starts Brave on that directory with the extension loaded unpacked. Run that line in your terminal.
`bravebot-browser` prints it and does not run it. The profile is a new Brave profile with none of
your history, bookmarks, logins or settings, and the command has `--no-first-run`, which skips the
first-run tasks of Brave, among them the offer to import them. The extension's switches belong to
the profile, so every group is off in it. Turn on the groups you want in that profile's options
page, as in step 5. The declarations of step 3 are the same, since both profiles reach the same
relay.

- Step 1 is still needed. Brave reads the host manifest from one place whatever profile it runs on,
  so a manifest in the profile directory is not read.
- `--brave <path>` names the Brave to start. Without it, `profile` uses the Brave in `/Applications`
  on macOS, and on Linux the first of `brave-browser` and `brave` on `PATH`.
- Keep the profile out of `~/.bravebot-browser`. A server declared with `--dir ~/.bravebot-browser`
  may write everything in that directory, and the profile holds the history and settings of the
  browser on it.
- The profile directory has to be absent, empty or one `profile` made. A directory that holds
  `Default` or `Local State` and has no `bravebot-profile` file is the profile of a browser somebody
  uses, and `profile` refuses it. It also refuses an extension directory that has no `manifest.json`
  or a comma in its path, and writes nothing in any of these cases.
- One Brave profile is connected to the relay at a time. If the extension is also running in your
  regular profile, the one that connected first keeps the relay, and the other connects within a
  minute of the first one disconnecting.
- If a Brave ignores `--load-extension` (1.98.62 honours it), load the extension directory in that
  profile's `brave://extensions`, as in step 2.
- A Brave started a second time on a directory one is running on starts no second browser. `profile`
  says that a Brave may be running when the directory holds the `SingletonLock` link.

## What it may do

The extension's options page has a switch for each group, and inside it a switch for each tool.
Only the platform check starts on. Every other tool starts off, since it reaches what you have open,
have visited or have saved, changes it, or sends your cookies to a site. A tool that is off is
refused before the browser is asked anything.

The tools are in groups, listed in `groups.js`, and the extension requires only `nativeMessaging`,
`storage` and `alarms` when it is installed.
[The design](../docs/design/browser-extension.md#capability-groups) lists each group's permissions,
default lifetime and presets, and the Chrome methods behind each tool. The other permissions a
group uses, and the pages it reads, are optional permissions that Brave holds only once they are
granted. A tool whose switch is on is refused until Brave holds every permission of its group, and
again if one is taken back in Brave's extension settings. Taking one back there also turns off the
groups that use it, with their tools.

Turning a group off gives Brave back its permissions, except those another group that is on still
uses. `bookmarks` and `webNavigation` are examples of permissions two groups may share.

A group stays on until you turn it off, until Brave restarts, or for one hour, as you chose when you
turned it on. Each group offers until you turn it off first, except `downloads`, `cookies`,
`remove-data`, `scripts`, `input`, `input-trusted`, `capture-trusted`, `console-trusted`,
`upload-trusted`, `screen` and `held`, which offer until Brave restarts first. A tool is
refused as soon as its group's time has ended, and the extension then turns the group off and gives
Brave back its permissions, within a minute while Brave runs and at the next start otherwise.
Reloading or updating the extension also ends a group turned on until Brave restarts. A preset turns
its groups on until you turn them off.

If you had an earlier version of the extension, its switches for single tools and its blocked
sites are kept. No group is on after the update, so a tool that was on is refused until you turn on
its group, once. Brave may still hold the permissions that version asked for at install; they start
nothing. Turning a group on turns on all its tools, including one you had switched off before.

The presets name only groups that have a tool. `remove-data`, `cookies`, `downloads`,
`network-watch`, `browser-info`, `scripts`, `input`, `input-trusted`, `capture-trusted`,
`console-trusted`, `upload-trusted`, `screen` and `held` are in no preset, and each is turned on by its own switch.
`bravebot-browser install --preset` names the same groups for the same preset names.

**Allowed sites** is a list you write beside **Blocked sites**, one site to a line. A group that
reads pages, which are `page-read`, `network-watch`, `cookies`, `input` and `screen`, reaches only
the sites on it. `scripts`, `input-trusted`, `capture-trusted`, `console-trusted` and `upload-trusted` are limited to it whatever
you choose.
Nothing is allowed at first, so turning such a group on reaches no page until you add a site. Each
such group also has a choice of **Only the allowed sites**, which is the default, or **All sites
except the blocked ones**. Saving the list asks Brave for access to the sites you wrote, which Brave accepts only from the click on **Save
allowed sites**, and gives back the access to a site you took off. On all sites, the group holds the
`https://*/*` and `http://*/*` host permissions, and `page-read` holds `<all_urls>` instead of them,
which `screenshot_page` needs. Brave shows its warning for access to all websites when you choose it. The scope is checked by the extension at each call as well as by Brave,
since Brave holds one set of hosts for the whole extension: a group limited to the allowed sites
refuses a page at any other site even while another group holds all sites. A site on both lists is
blocked. The other groups have no scope: a call to `navigate` or `downloads` can load or fetch a page
at a site you did not allow, and only **Blocked sites** stops it.

Brave has its own **Site access** setting for an extension, with the choices on click, on specific
sites and on all sites, and it can withhold a site that you allowed here. No API says which choice is
set. A call that the setting prevents fails, and the message says that the setting can be the cause.
The groups that read pages are turned off, as for any permission, when Brave stops holding a site you
allowed. The warning text Brave gives for a host is in `docs/design/browser-extension.md`. The
prompt that shows it, and what the setting does to an optional permission, were not run in a real
Brave.

Below the switches, **Blocked sites** is a list you write, one site to a line, such as
`https://mail.example.com`. Whatever is switched on, no tool lists, searches, reads or opens a site
on it. Tabs, frames, history entries and bookmarks at a blocked site are left out of results, so a
list can hold fewer rows than Brave has, and a call that names one is refused with a message that
names the list and not the site. A site is matched as written: `https://example.com` does not cover
`https://www.example.com` or `http://example.com`, so list each one. A line that is not an
`http://` or `https://` address of a host, `https://*.example.com` for one, or that holds two sites,
is not accepted: nothing is saved until it is fixed, the page says which line, and the list saved
before still applies. The list starts empty, and it saves when you click outside the box, and says
"Not saved yet" until then.

`list_frames` finds the tab whose URL is exactly the one asked for, and returns the HTTP and HTTPS
URLs of its frames. Its result is shown to you and kept out of the session's planner: choose one URL
and provide it in your next message. `read_page` reads the top page, or the exact `frame_url` you
provided after `list_frames`, and returns up to 100,000 characters of its text, saying when it cut
the text short. A duplicate frame URL is refused because it does not identify one frame. A tool
that names an open tab, in any group, finds it by its exact URL and is refused when more than one
tab is at that URL, with a message that says how many are; close the others, or navigate one of
them, and ask again.

The `tabs-read` group also lists your windows without their tabs (`list_windows`), your tab groups
(`list_tab_groups`) and the tabs and windows you closed lately (`list_recently_closed`). The
`history-read` group also lists the visits to one exact URL (`get_visits`), the sites Brave counts as
the ones you visit most (`list_top_sites`) and the tabs open on your other devices
(`list_synced_tabs`). These six are offered by the server of their group and not by a server declared
with no group, which still offers the seven tools above. Entries at a blocked site are left out of
their results, and `get_visits` for a URL at one is refused.

`read_page_links` and `read_page_forms` read the top document of the tab at the URL asked for, and
reach into shadow roots on the page. The first returns the text and address of each HTTP or HTTPS
link, leaving out links to blocked sites, and the second returns where each form submits and how, and
the name and type of each field. It never reads what a field holds, so a password or a token in a
hidden field does not come back. Both stop at 100,000 characters and say so. A field outside a
`<form>` and a form inside a frame are not listed.

`list_page_elements` numbers the controls of the top document of the tab at the URL asked for, in
document order, so that you can tell the session which one to act on. It returns text: up to 40
lines, each a number, a role, the accessible name cut at 120 characters and the site a click on it
goes to, which is the link or the submitting button nearest the element, above it or the element
itself, or `unknown` for an image map. An element whose place cannot be told is left out while a site
is blocked. It leaves out what is not displayed, what is disabled or read-only, a password field, a
payment card field and a file input, and says how many, and it never reads what a field holds. A
link to a blocked site is left out. The text reaches the session as any result does, so you read it
in the transcript and the planner does not. You give the number in a message, such as "click number
7", and the session passes it to `click_element`, `type_text` or `select_option` as `element_number`.
The extension keeps the list in memory for 5 minutes, for that one URL, and a second list replaces
the first. It forgets the list when `page-read` or `input` is turned off, which **Turn everything
off** does. A number is refused when there is no list, when the list is for another page or is more
than 5 minutes old, when the number is past the end of the list, and when the page no longer shows
that element as it was listed: the same tag, role, name, place in the document and destination. The
element is then shown to you in the window below, as for a role and a name, with its number and
which of the elements with the same role and name it is, such as 2 of 3, so that two elements that
look alike in the window are told apart.

`screenshot_page` takes a JPEG of the tab at the URL asked for, as its window shows it, and it needs
the `page-read` group on **All sites except the blocked ones**: the browser lets an extension capture
a tab only with access to all sites, which is the host permission `<all_urls>`, so under **Only the
allowed sites** the tool is refused and says so, even where another group holds all sites. The tab
has to be the one its window shows and the window not minimized, or the call fails and says which;
the tab is not brought forward, and `focus_tab` does that. A URL at a blocked site, one that is not
HTTP or HTTPS and one that names more than one tab are refused before a tab is looked for or
anything is captured. The extension takes one picture at a time, at least 600 milliseconds after
the last, because the browser allows two in a second, and refuses a picture of more than 8 MiB. The
reply holds the picture and nothing of the page, the relay sends it as a picture in the result,
which the model holds as a reference and does not see, and it reaches the model only through
`spawn_processor` or after you look at it and agree at `vet_content`
([Looking at a page](#looking-at-a-page)). A frame of another site inside the page is in the picture, and a blocked site shown
in a frame is not blocked.

**Screen.** `capture_screen` takes one picture of what you choose in the picker Brave shows for each
call, a screen or a window of any program, and nothing is captured until you choose. The extension
opens a small window of its own for the picker, takes one frame of what you chose, stops the sharing,
closes the window and returns a JPEG of at most 2048 pixels on a side. You have 60 seconds; closing
the picker or the window, or waiting, ends the call with a failure. The picture of a screen holds
everything on it, including sites on your blocked list and other programs. `record_gif` records the
tab at a URL as an animated GIF: 2 to 20 pictures, 10 where the call does not say, taken as
`screenshot_page` takes one, at least 0.6 seconds apart, each scaled to at most 480 pixels on a side
and written in at most 256 colours without dithering. It needs the group on all sites for the same
reason, and a frame that cannot be taken ends the recording with no GIF. The group's server is
declared with `--ask-every-call` and `--tool-timeout 120`, so each call is asked about whatever a
rule or an earlier answer says, and the model sees either picture only after you look at it and
agree at `vet_content`. The group lists
`desktopCapture`, which Brave words as "Capture content of your screen", and `tabs`. It does not use
an offscreen document, because the browser lets only the page that asked for the stream use it.

`save_page_archive` saves the page of the tab at the URL asked for as an MHTML file, with its pictures
and frames, in your Downloads folder. The extension names the file `bravebot-` and the site's host
and `.mhtml`, a file already there is not replaced, no dialog asks where, and a page of more than
16 MiB is not saved. The tab need not be the one its window shows. A URL at a blocked site, one that is
not HTTP or HTTPS and one that names more than one tab are refused before a tab is looked for, and
so is a URL at a site that is not under **Allowed sites**, whatever choice of sites `page-read` has. The reply names the file and says
how the download stands, and holds nothing of the page, so the model is not told where the file is
and reads it only if you give it the file. The file is outside your workspace and Bravebot treats it
as untrusted. The tool is in the `downloads` group, which asks Brave for `pageCapture`, `downloads`
and `tabs`, so turning that group on shows "Read and change all your data on all websites" beside
"Manage your downloads", and the Read and Browse presets ask for neither `pageCapture` nor
`downloads`. The group lists `tabs` because the tool finds the tab by its address, which Brave
leaves out of every tab for an extension that holds neither `tabs` nor the tab's site; Brave shows
no line of its own for `tabs` beside the line for all websites. The group has no choice
of sites, so `groups.js` names this tool in `ALLOWED_ONLY_TOOLS`, which holds it to the allowed
sites, and the allowed sites limit what is saved because this extension checks them and not because
Brave does. The other tools of the group are not held to them. A frame of
another site inside the page is in the file, and a blocked site shown in a frame is not blocked. The
file stays in Downloads until you delete it and any program of yours can read it.

`focus_tab`, `pin_tab`, `mute_tab` and `move_tab` act on the open tab at the URL asked for, never on a
tab id. `focus_tab` makes it the active tab and brings its window forward. `pin_tab` and `mute_tab`
set the state, or clear it when called with `false`, and `move_tab` takes a position in the tab's own
window, counting from 0, with -1 for last. Each returns the URL and the state Brave reports. They
start off, in the `tabs-manage` group, which asks Brave for the `tabs` and `tabGroups` permissions.

The rest of the `tabs-manage` tools also name a tab by its exact URL. `group_tabs` and `ungroup_tabs`
take a list of URLs: the first puts those tabs, which have to be in one window, into a new tab group
in that window, with a title if one is given, and the second takes them out of their groups.
`update_tab_group` changes the title, color or collapsed state of the group the tab at a URL is in,
and fails where that tab is in none. `discard_tab` unloads a tab from memory, so what its page does not save can be
lost, and it stays in the tab strip. `duplicate_tab` opens a copy, which loads
that address again signed in as you are, and returns nothing of the copy. `set_zoom` takes a factor
where 1 is 100% and 0 is Brave's default, and Brave may apply it to other tabs at the same site.
`focus_window` brings the window that holds the tab forward and leaves its active tab alone.
`resize_window` takes a width and a height in pixels, whole numbers from 1, and sets the size of the
window that holds the tab, leaving its position and tabs as they are. Brave keeps a window above its
smallest size and may keep a maximized or full screen window as it is, so the reply gives the size
Brave reports. Each returns the URL and the state Brave reports afterwards.

The `bookmarks-read` group also lists the bookmarks and folders directly in one folder, named by its
exact title (`list_bookmark_folder`), your newest bookmarks (`list_recent_bookmarks`) and your
reading list (`list_reading_list`). These three are offered by the server of the group only. A title
that is no folder, or is the title of more than one, is refused.

The `bookmarks-write` group adds and changes bookmarks and reading list entries, and deletes none. It
asks Brave for the `bookmarks` and `readingList` permissions, which `bookmarks-read` asks for as well,
so turning one group off keeps them for the other while it is on. A bookmark is named by its exact
URL and a folder by its exact title, and no tool takes the id Brave gives a bookmark. `create_bookmark`
adds a bookmark at an HTTP or HTTPS address, with a title if one is given, in the folder with the
title given or, if none is given, where Brave puts new bookmarks. A bookmark at any other kind of
address, such as `javascript:`, is not made, since it runs in a page when it is clicked.
`create_bookmark_folder` adds a folder in the same way, inside the folder named as `parent`.
`move_bookmark` moves the bookmark at a URL to the end of a folder, and `rename_bookmark` gives it a
new title. A URL at which more than one bookmark is saved names none of them, and a title that is
the title of more than one folder names none of them: the call fails, says how many, and changes
nothing. Adding a bookmark at an address that has one, or a folder with the title of one, is allowed,
as Brave allows it, and the tools above then refuse that URL or title until one of the two is
renamed or removed in Brave. `add_reading_list_entry` adds an entry for an HTTP or HTTPS address
with a title, unread unless `read` is true, and fails where the list has an entry at that address.
`update_reading_list_entry` changes the title or whether it was read, of the entry at an address.
Each returns what Brave holds afterwards. A URL at a blocked site is refused, before Brave is asked
for a bookmark or an entry. The tools are offered by the server of the group only.

The `network-watch` group lists the URL, type and status of each request the tab at one exact URL
has made since it loaded its page (`list_requests`). It never reads a header, a cookie or a body.
The group reads pages, so a call is limited to the sites you allowed for it, or to every site that is
not blocked if you chose that, and Brave asks you to allow those. The extension keeps the requests in memory
only, from the start of each page load, and only while the group and the `list_requests` switch are both
on. It forgets them when the tab closes or when you turn either off. A page that loaded before you turned the group on, or before Brave last restarted the
extension, has none kept: the call says so and returns nothing, and reloading the page starts the
record. At most 500 requests of a load are kept, the first ones, and the result says when there
were more. Requests to a blocked site, and requests made by a frame at one, are left out.

The `browser-info` group says how Brave and the computer are set. `get_privacy_settings` gives the
value of each of Brave's privacy settings, such as safe browsing and do not track, and who controls
it. `get_content_settings` gives what Brave allows, blocks or asks about for cookies, scripts, images,
location, notifications, camera and more at one exact URL, and is refused for a site on the blocked
list. `get_font_settings` gives the font for each generic family and the three font sizes, and not the
fonts installed. `get_system_info` gives the processor, memory, displays and storage devices, with
their names and sizes. `get_idle_state` says whether you are active, idle after a number of seconds
without input, or the screen is locked. `get_access_flags` says whether you allowed the extension on
`file:` addresses and in private windows, which you set in its details page in Brave. None of these
changes anything. Brave asks you to allow "Change your privacy-related settings", "Change and grant
access to features such as geolocation, microphone, camera, cookies, etc., for all your websites and
extensions, including this extension." and "Identify and eject storage devices" when you turn the
group on.

`keep_awake` is the one tool of the group that changes anything. It keeps the computer from sleeping,
or its display on as well, for the number of minutes asked, at most 240, and shows that number in
the question before it runs. It ends by itself when the minutes pass, a call with 0 minutes ends it at
once, and a new call replaces the one before. Reloading or updating the extension, or restarting Brave,
also ends it, and so does turning the tool's own switch off, turning the group off, a preset that turns
it off, the end of the time you turned it on for, and Brave taking one of its permissions back. If it is the `power` permission that
Brave took back, the extension can no longer tell Brave to stop, and turning the group on again does.
A call that is under way when you turn the group off ends its request as well, and is refused.

The `remove-data` group deletes, and every deletion in it can lose what Brave does not keep elsewhere.
`close_tab` closes the tab at an exact URL, and `close_window` closes the window that holds the tab
at an exact URL with every tab in it, unless one of them is at a blocked site. `delete_history_entry`
deletes the history entry at an exact URL with every visit to it. `remove_bookmark` deletes the one
bookmark at an exact URL and fails if more than one is there. `remove_bookmark_folder` deletes the
one bookmark folder with an exact title, and only if it is empty: a folder with anything in it is
left as it is, and you delete its bookmarks one at a time first. `remove_reading_list_entry`
removes the reading list entry at an exact URL. `delete_downloaded_file` deletes the file of a
finished download from the disk, named by its exact path or the exact URL it came from. The download
stays in Brave's list with its file gone, and a file downloaded from a blocked site is not deleted.
`delete_history_range` deletes the
history between two times and `clear_browsing_data` clears the kinds of data named (cache, cookies,
form data, local storage and the like, not saved passwords) since a time, for every site, including
those on your blocked list. Both times are written with their offset, such as `2026-10-01T00:00:00Z`,
and a call that leaves out a time or a kind deletes nothing. The question shows each of these
arguments before the call runs. The group is on until Brave restarts unless you choose another time,
and `install` declares its server so that every call is asked about and no answer is remembered.
Brave asks you to allow "Read and change your browsing history on all your signed-in devices", "Read
and change your bookmarks", "Read and change entries in the reading list" and "Manage your downloads",
and shows nothing for the permission that clears browsing data.

The `downloads` group puts files on your disk. `download_file` downloads one HTTP or HTTPS address
into your Downloads folder, with your cookies for that site, under the name Brave gives it, and does
not replace a file that is there. No folder, name or dialog is part of the call. If the address goes
to another host, or to a site on your blocked list, the download is cancelled and a file it had
already finished is deleted. `list_downloads` shows the path and address of each download, newest
first, and `pause_download`, `resume_download`, `cancel_download` and `show_download` name a download
by exactly that path or address. `show_download` shows the file in its folder in your file manager and
does not open it, and `cancel_download` does not delete a finished file. No tool reads a downloaded
file. The Downloads folder is outside BraveBot's workspace, which does not trust it, unless you open it
with `/add-dir`. The group is on until Brave restarts unless you choose another time, and Brave asks
you to allow "Manage your downloads". Downloads from sites on your blocked list are left out of the
list and are not paused, resumed, cancelled or shown.

`open_tab`, in the `navigate` group, opens the HTTP or HTTPS URL you approved in a background tab,
signed in as you are, and returns the URL the tab loaded and no page content. If the page that loads
is on a different host, for example after a redirect, it closes the tab and fails. It starts off like
the other tools. To read what it opened, provide that URL in a later message for `read_page`.
Brave's own pages, such as settings, and the Web Store cannot be read.

The `navigate` group has six more tools that load a page as you are signed in. `navigate_tab` takes
the open tab at one URL to another address, `reload_tab` reloads a tab, `go_back` and `go_forward`
move a tab one page in its history, `open_window` opens an address in a new window behind the one you
are using, and `restore_closed` reopens the tab you closed that was at a URL. A tab is named by its
exact URL, and the page that loads has to be on the host of the address you were shown: the address
to go to for `navigate_tab`, and the tab's own URL for the others. A tab or window the call made is
closed when the page is on another host. A tab you had open is left blank, not closed, so a redirect
to a login page on another host, or a `go_back` to a page on another site, leaves a blank tab and a
failure. Each call returns the URL the page loaded and no page content, and a site on the blocked
list is neither loaded nor left on. `restore_closed` does not reopen a closed window. The group is
not limited to the allowed sites, as `open_tab` is not.

`search_web` searches for the words given, at most 256 characters, with your default search
provider, in a new background tab, and returns the URL of the results page. The question shows the
words and not the provider's address, so the page is not held to a host. The tab is closed and the
call fails if the page is not a web page, fails to load, is at a site on the blocked list or does not
load within 20 seconds. The provider has been sent the words by then. `wait_for_load` waits up to 20
seconds for the open tab at a URL to finish loading and returns `loaded` or `timed out`, and fails if
the tab is closed or goes to another URL meanwhile. Brave asks for no permission of its own for
`search`.

The `cookies` group has two tools. `list_cookie_names` returns the name, domain, path, flags and
expiry of each cookie Brave sends to one HTTP or HTTPS URL, and `delete_cookie` deletes the cookie
with a name that Brave sends to a URL, only when exactly one has that name there, since Brave
deletes every cookie of the name otherwise. The group reads pages, so it reaches only the sites you
allowed unless you chose all sites, and a site on the blocked list is out of reach under every
scheme and port, because a cookie belongs to a host. A cookie for a parent domain of a blocked host,
such as `.example.com` for a blocked `mail.example.com`, is out of reach too, since Brave sends it to
that host as well. It is on until Brave restarts, and `install`
declares its server to be asked about every call. Brave shows no prompt for the `cookies`
permission, and shows its warning for the hosts.

No tool reads or sets a cookie's value, and the extension builds its result from named fields
(**Not yet decided**).

The `input` group has six tools that act in a page you have open: `click_element`, `type_text`,
`select_option`, `press_key`, `scroll_page` and `wait_for_element`. A tool names one tab by its
exact URL and an element by a role and an accessible name, such as a button named "Pay now", or, for
`click_element`, `type_text` and `select_option`, by the number it has in the last
`list_page_elements` for that URL. The extension finds the one element with that role and name in
the top document, including open and
closed shadow roots and not frames, and fails if none or more than one has it. For a click, typing,
a choice or a key it then brings the tab forward, outlines the element, and opens a small window of
the extension that shows the action, the address of the page, the element as the page names it and
the text to be typed. Nothing happens until you press **Allow once** in that window. **Deny** and
**Allow once** are at the foot of the window and stay in view while the rest of the window scrolls,
however long the page's words are. **Deny** has
the focus, **Allow once** becomes available a second after the window is in front, and, where the
box that holds the text to type or the code is longer than it shows, only after you have scrolled the
box to its end. Escape, closing the window and no answer within 60 seconds deny. The page still has to show the same
element, with the same place in the document and the same destination, when the action is made. Only
one window is open at a time: a second action meanwhile is refused. The words in the window are the
page's own, cut to a fixed length, and a control or direction character in them is shown as its code
point. The outline is a hint, since a page can hide it or draw its own. Scrolling and waiting open
no window. A password field, a payment card field and a file chooser are never acted on. The page
can tell the events from yours, so a site that checks that ignores them, and a key does not do what
it would do by itself: Enter does not submit a form. The group reads pages, so it reaches only the
sites you allowed unless you chose all sites. It is on until Brave restarts and is in no preset.
`install` declares its server to be asked about every call, so BraveBot asks you in the terminal
and then the extension asks you in the browser.

The `input-trusted` group has five tools: `click_element_trusted`, `type_text_trusted`,
`press_key_trusted`, `hover_element` and `drag_element`. They name a tab and an element as the `input`
tools do, and `drag_element` names a second element to drop the first on, by a role and a name, with
no position or distance. They send real mouse and keyboard input, which the page cannot tell from
yours, and a second extension does it: BraveBot Trusted Input, loaded unpacked from
`extension-trusted/` the same way and holding the browser's `debugger` permission. This extension
does not hold that permission. It checks the group's switch, its time, the allowed and the blocked
sites and the arguments, and then sends the call to the other extension, which opens its own window
for you to press **Allow once** in, as above, with both elements shown for a drag. If the page opens a
dialog while the input is sent, you answer it in a panel of that extension. The group reaches only the
sites you allowed whatever you choose, and the options page offers no choice of all sites for it.
If the other extension is not installed or is turned off, the tools say so in one sentence and nothing
else changes. Turning it off at `brave://extensions` is the way to take its `debugger` permission
back, since this extension cannot. A copy of either extension with another id, such as one from the
Web Store, is refused by the other until the ids in both are changed.

The `capture-trusted` group has one tool, `screenshot_full_page`, which takes a picture of the whole
top document of the tab at an exact address, from the top of the page to the bottom and the left to
the right, including the parts out of view and a tab in the background. The same second extension
does it, because the browser lets this extension capture only the visible part of the visible tab.
It is a group of its own so that turning it on says what it reads, which is everything the browser
paints for the page, and says nothing about clicking. Before it captures anything, that extension
brings the tab forward and opens its window, which shows the site, the address and the size of the
page and no element, and the picture is taken only if you press **Allow once**. The picture is a
JPEG of quality 80, scaled so that its long side is at most 2048 pixels, and it is refused if it is
over 8 MiB. The reply is the picture and nothing else, and it comes back to you and not to the model,
as the picture of `screenshot_page` does: the model reads it only through a processor or after you
have looked at it and agreed. A frame of another site is in the picture as the page draws it. The
group reaches only the sites you allowed, is on until Brave restarts and is in no preset. `install`
declares its server to be asked about every call, with `--tool-timeout 120`.

The `scripts` group has one tool, `run_script`, which runs JavaScript the model wrote in the top document of
an open page, as you, with the session you are signed in with. It takes the page's URL and the code, of at
most 2,000 characters. Before anything runs, this extension opens a window of its own that shows the site, the
address and every line of the code, with each line feed and each character you cannot see written out, and the
code runs only after you press **Allow once**, which is available once you have scrolled the code to its end if it
does not fit in its box. Deny, closing the window or no answer within 60 seconds runs
nothing. **Read all of the code: nothing limits what it does except that you read it.** It can read everything
on the page, including what is hidden, and click and submit the page's forms as you do. The code starts once.
Timers, listeners and observers it sets keep running in the page until the page is closed or reloaded, and
each can read and send without asking again. The world it runs in has a content security policy of
`default-src 'none'`, which in Brave 1.98 stopped its own `fetch`, `XMLHttpRequest`, `sendBeacon` and
`WebSocket` requests. Brave 1.98.63 applied it to the first ten worlds a page was given, so the extension
checks before it asks you that Brave applied it, and refuses the call, with a sentence that says to reload the
page, when it did not. It did not stop a frame that loads an address, a navigation, a form submission or
`window.open` once the page has had a click, so the code can send what it read to another address. What the
code answers comes back to you as text of up to 100,000 characters, and not
to the model: the model reads it only through a processor or after you have looked at it and agreed. A script
that throws answers `null`. The group needs two things from you. It needs Brave to give the extension the
`userScripts` permission, which turning the group on asks for with no prompt of its own. And it needs
**Allow User Scripts** to be on for this extension, which only you can turn on: in Brave built on Chrome 138 or
later, open `brave://extensions`, choose **Details** for BraveBot and turn on **Allow User Scripts**; in an
older Brave, turn on **Developer mode** at `brave://extensions`. Until you do, `run_script` is refused with a
sentence that says so, and the options page says whether it is on once the group is on. The group reaches
only the sites you allowed, is on until Brave restarts and is in no preset. `install` declares its server to be
asked about every call, with `--tool-timeout 120` ([BROWSER-46](../docs/specs/browser.md#BROWSER-46),
[ROUTE-11](../docs/specs/routing.md#ROUTE-11)).

The `upload-trusted` group has one tool, `choose_file`, which gives a file of yours to the file input of an
open page, as if you had chosen it in the browser's file picker. It takes the page's URL, the input's
accessible name, optionally its role (`button`, which is what the browser calls every file input) and a
`path`. The file has to be in the `uploads` directory of the directory the server is declared with, which
is `~/.bravebot-browser` unless `BRAVEBOT_BROWSER_DIR` names another: make a directory named `uploads` there
and put the files you want the model to be able to offer in it. The directory has to be a directory of its
own and not a link, and a link inside it that leads out of it is not followed out. A path is refused, before
anything is asked or sent, when it does not lead to a regular file in that directory, when the file has more than
one hard link, when the file is over 10 MiB, or when its name or one of the directories above it in `uploads`
looks like a credential: `.env`, `.env.*`, `.netrc`, `.npmrc`, `.pgpass`, `.git-credentials`, `.ssh`, `.aws`, `.gnupg`, `id_rsa*`, `id_dsa*`, `id_ecdsa*`,
`id_ed25519*`, `credentials*`, `*.pem`, `*.key`, `*.p12`, `*.pfx` and `*.kdbx`. The relay's server looks at the file
and sends this extension the file's full path, size and name, and no byte of the file passes through the relay,
this extension or the model: the BraveBot Trusted Input extension asks the browser to read the file and set it on
the input. That extension opens its own window, which shows the page, the input and the file's name, size and
full path, says that the browser reads the file and gives it to the page, and chooses the file only after you
press **Allow once**. Deny, closing the window or no answer within 60 seconds chooses nothing. Choosing a file
sends nothing by itself, but a page whose script sends the file when it is chosen has it at once, so allow it for
the sites you trust with that file. A file input the page hides, as many sites do behind a label or a button,
is chosen on as well, named by the page's own label, `aria-label` or `title`, or by its `name` or `id` where
nothing else names it, and that extension's window says that the page hides it. An input that a button opens by
script and that nothing names is not found. A file input takes the one file. The setup is the one of
`input-trusted`: that extension must be installed and turned on, and this extension checks the group's switch,
its time, the allowed and the blocked sites and the arguments before it sends the call. The group reaches only the sites you allowed, is on until Brave restarts and is in no preset
([BROWSER-45](../docs/specs/browser.md#BROWSER-45)).

The `console-trusted` group has one tool, `read_console`, which reads the console of an open tab: what
the page's scripts wrote with `console.log` and the others, the errors they threw and what Brave
reported about the page, such as a request that failed or a rule of the page that was broken. It
takes a tab's URL, and optionally `levels` (`all`, `warnings_and_errors` or `errors`) and
`max_entries` (1 to 200, 100 when left out), and returns the newest entries as text, one line for
each, with the level, where it came from, the text, the address of the script and the line. The
BraveBot Trusted Input extension reads it, so the setup is the one of `input-trusted`: it must be
installed and turned on, this extension checks the group's switch, its time, the allowed and the
blocked sites and the arguments, and then sends the call. That extension opens its own window, which
says that the browser's debugger reads the page's console, shows the page and what is read, and
attaches to the tab only after you press **Allow once**. Deny,
closing the window or no answer within 60 seconds reads nothing. Brave shows its notice that the
extension is debugging the tab while it reads. A console holds whatever the page's scripts printed,
which can include data from your account, and the text is not filtered. It reaches the session as a
result does, so the model never reads it directly: a processor that answers a question about it can
read it, or `vet_content` shows it to you and the model reads it only if you agree. A script's error
that nothing caught appears as `Uncaught` with its line and without its message, and what a frame
that Brave runs in another process printed is not read. The group reaches only the sites you
allowed, is on until Brave restarts and is in no preset
([BROWSER-44](../docs/specs/browser.md#BROWSER-44)).

The `held` group lets you hand a session something you select or point at. While it is on, Brave's
context menu has three more entries: **Hold this selection for BraveBot** on selected text, **Hold
this link for BraveBot** on a link and **Hold this tab for BraveBot** on the page. Using one keeps
what Brave gives the extension for it: the selected text, the address of the link, or the address of
the page, with the address of the page you used it on. You then tell the session, in your own words,
to look at what you held, and it calls `take_held`, which takes no argument, returns the items in
the order you held them and forgets them. At most 10 are kept, for 30 minutes, in memory that Brave
never writes to disk. A selection is cut at 20,000 characters and an address at 2,000. They are
forgotten when you turn the group off, when you press **Turn everything off**, when its time ends
and when Brave restarts, and the popup says how many are held. The group lists only `contextMenus`,
for which Brave shows no prompt, and reads no page: the extension has no script in any page and
receives no message from one, so a page cannot add to what you hold. An item held at a site on your
blocked list is left out. A tab is held as its address, since its title would need the `tabs`
permission. The text of a selection is the page's text, including any the page hides in the range,
and holding it out does not make it yours. It reaches the session as a result does, so the model
never reads it directly. It can be given to a processor that answers a question about it, or shown
to you by `vet_content`, and the model reads it only if you agree after reading it. The group is on
until Brave restarts and is in no preset, and `install` declares its server as it does any other that
is not asked about every call
([BROWSER-39](../docs/specs/browser.md#BROWSER-39)).

What the extension returns reaches the session as any server's result does: untrusted, and private.
The planner holds a reference to it and does not read it until you let it through
([Looking at a page](#looking-at-a-page)). You read it in the transcript and can give the session
what it needs in a later message.

## Looking at a page

The model reads a page, a list of its elements, a picture or a console only after you vet it. What a
tool returns is held as a reference that names its type and size and nothing of what it holds. These
let the model read it:

- `vet_content`. The model names the reference. You are shown the exact text, or a copy of the
  picture to open in your own viewer, and the model reads it once if you answer yes. Where
  auto-vetting is on, a check, which is a second model, answers in your place when it finds nothing.
  Nobody reads the content then, and the check can be wrong
  ([Vetting quarantined content](../docs/website/docs/security/vetting.md)).
- Your own message. You read the result in the transcript and write what the session needs.

Until then the model reads what you wrote, the files and settings you vouched for, and nothing a
browser tool returned. A delegate is not offered `vet_content`, so it cannot look at a page.

A model that looks at a screenshot can then act on the page, one round at a time:

1. **Look.** The model calls `screenshot_page`, which needs `page-read` on all sites, or
   `screenshot_full_page`, and then `vet_content` on the picture. You open the picture and answer
   yes, and the model sees it once. A new screenshot is a new reference and is vetted again.
2. **Name.** The model writes the role and accessible name of an element in its call, from what it
   saw. It can also write an `element_number`, from the list that `list_page_elements` made, which
   you read in the transcript and told the session, or which you let the model read at
   `vet_content`.
3. **Confirm.** BraveBot asks you about the call in the terminal, and the extension finds the one
   element, brings the tab forward, outlines it and opens a window that shows it. Nothing happens
   until you press **Allow once** there.
4. **Act.** The extension acts on that element if the page still shows it as you were shown.

The cost of a round is one vetting step for each look and one confirmation in the terminal and one
in the window for each action on an element. Scrolling and waiting open no window. No tool takes a
selector, a node id or a coordinate from a page, so the model's part is the words of step 2, and what
the page does with them is shown to you in step 3. Content let through at `vet_content` stays
private, so the arguments the model writes after a vet are private and each call is asked about
whatever a rule says ([ROUTE-9](../docs/specs/routing.md#ROUTE-9)).

Every tool takes only arguments you can judge in the question BraveBot asks before a call: an
address, a name or a short text, a number or a time, one of a few words, or yes or no. No tool takes
a tab id, a node id or a selector read from a page, and a tab, window, bookmark, folder or download
is named by its address, title or path. The question shows the call as the name of the server and
the tool, such as `brave-tabs-read:list_windows`, and then the arguments.
[The design](../docs/design/browser-extension.md#tool-surface) lists the name and the arguments of
every tool.

## When it does not answer

- **No Brave extension is connected.** Brave is not running, the extension is off or not loaded in
  `brave://extensions`, or Brave could not start the relay. Run `install` again if the relay has
  moved since you last did, since the manifest names it by its path.
- **`<tool>` is turned off in the BraveBot extension's options.** Turn it on, as in step 5.
- **`<tool>` needs the `<group>` group's browser permissions, which are not granted in the BraveBot
  extension's options.** The tool's switch is on and Brave does not hold what the tool's group
  uses. A fresh install holds none of it. Turn the group on in the options page, as in step 5, and
  choose **Turn on** to let Brave ask. A group that shows as off there was turned off when a
  permission it uses was taken back, and is turned on the same way. For a group that reads pages the
  message adds that Brave's site access setting for the extension can withhold a site, and the fix is
  to set it to the sites you allowed or to all sites in `brave://extensions`.
- **The options page says a group is blocked by your organization.** Brave refused the request for
  the group's permissions with "Permissions are blocked by enterprise policy." The group stays off
  and the page does not ask again until it is opened again. See **Organizations**.
- **`<tool>` is in the `<group>` group, which is turned off in the BraveBot extension's options.**
  The tool's own switch is on and its group is off, for example after an update from a version
  without groups. Turn the group on, as in step 5.
- **`<tool>` is refused: the `<group>` group was turned on for one hour (or until Brave restarts), and
  that has ended.** Turn the group on again, as in step 5.
- **That site is in the blocked sites list in the BraveBot extension's options.** The site is on the
  list you wrote under **Blocked sites**. Take it off to let the tools reach it.
- **The blocked sites in the BraveBot extension's options cannot be read.** What is stored as the
  blocked list is not a list of sites. Every tool that reaches a site is refused until you save the
  list again in the options page. The platform check, and the tools that name no site, still
  answer.
- **That site is not in the allowed sites list in the BraveBot extension's options, which limits this
  group to the sites on it.** The group reads pages and is limited to the allowed sites, and the page
  asked for is at another site. Add the site under **Allowed sites**, or choose **All sites except the
  blocked ones** for the group.
- **`<n>` open tabs are at `<url>`, so the URL does not name one tab.** More than one open tab is at
  the URL the call named. The call did nothing. Close the other tabs, or open one of them at another
  address, and ask again.
- **The Brave extension did not reply within 30 seconds.** The call can be made again. A call to
  `scripts`, `input`, `input-trusted`, `capture-trusted`, `console-trusted`, `upload-trusted` or `screen` waits 90 seconds, since
  you answer in a window of the browser.
- **The BraveBot Trusted Input extension is not installed, is turned off in Brave, or does not accept
  this extension.** A tool of `input-trusted`, `capture-trusted`, `console-trusted` or `upload-trusted` sent its call
  and Brave had no extension to deliver it to. Load `extension-trusted/` and turn it on, as in
  step 2. A copy of either extension with another id is refused by the other until the ids in both
  are changed.
- **The BraveBot Trusted Input extension refused it, or could not do it.** The sentence after it is
  that extension's, from a fixed list, such as that you denied the action in its window, that the
  window was closed or not answered in 60 seconds, or that no element, or several, have the role
  and the name. [Its README](../extension-trusted/README.md) lists what it refuses.

## Organizations

An organization that manages Brave can limit the extension with two keys of Chrome's
`ExtensionSettings` policy, set for the extension's id or for `*`:

- `blocked_permissions` lists API permissions the extension may not use. The permissions of the
  groups are optional, so the extension installs as usual, and Chrome declines a blocked permission
  while the extension runs. When you turn on a group that lists one, Brave refuses the request with
  "Permissions are blocked by enterprise policy.", and the options page shows the group as blocked by
  your organization, leaves it off and does not ask again until the page is opened again. A group
  none of whose permissions is listed is not affected.
- `runtime_blocked_hosts` lists hosts the extension may not interact with or modify, as patterns
  such as `*://*.example.com`. A call that reaches a listed host fails with the error Brave gives,
  and the options page does not show which hosts are listed. Set for this extension, the key also
  makes `debugger.attach()` fail on every target, with "Host access is restricted by policy."
  This extension does not use `debugger`. BraveBot Trusted Input does, so the key stops its tools
  for that organization.

These keys are described in Chrome's documentation of the policy and of the `debugger` API. No Brave
with either key set was run for this extension.

## Removing it

1. `bravebot mcp remove brave`, and `bravebot mcp remove brave-<group>` for each group you declared.
2. Choose **Remove** on BraveBot in `brave://extensions`. The relay stops with it, and the
   permissions it held go with it. Chrome's documentation says it clears an extension's storage on
   removal, which holds the switches, the lists of sites and the activity log. That was not run in
   Brave.
3. Delete the host manifest `install` wrote, `com.brave.bravebot.json` in
   `~/Library/Application Support/Google/Chrome/NativeMessagingHosts/` on macOS, or
   `~/.config/BraveSoftware/Brave-Browser/NativeMessagingHosts/` on Linux.
4. Delete `~/.bravebot-browser`.
5. If you made a profile with `profile`, quit the Brave on it and delete its directory.
6. If you loaded BraveBot Trusted Input, choose **Remove** on it in `brave://extensions` too. Turning
   it off there is enough to take its `debugger` permission away.

## Tests

```sh
make check-extension
```

runs `extension/tests/` against a fake `chrome` object, and `extension-trusted/tests/` against a fake
debugger. `package.json` is there only to tell Node the scripts are modules. There is nothing to install. `cargo test -p bravebot-browser --test
extension` holds this directory to the relay's side: the extension id, the tool names, the host
name, the permissions the manifest requires and lists as optional, the groups and their tools, and
the tables of the design document. `cargo test -p bravebot-browser --test tool_surface` holds the
relay's tools to the name style and to arguments a person can judge.

## Not yet decided

- **How they ship.** Loaded unpacked, the pinned keys give the two extensions fixed ids. A copy of
  either from the Web Store or bundled with Brave would have an id of its own. `install` would
  record the BraveBot extension's, and the ids in `extension/trusted.js`,
  `extension-trusted/pair.js` and `extension-trusted/manifest.json` would change together. Whether
  BraveBot Trusted Input has a Web Store listing of its own, which would have its own review of an
  extension that holds `debugger`, is not decided.
- **Asking for one site when a session names it.** `permissions.addHostAccessRequest` (Chrome 133)
  was evaluated and is not used. Sites are added in the options page. The design document says what
  the API does and what was not run.
- **Chrome Web Store.** The store's
  [policies](https://developer.chrome.com/docs/webstore/program-policies/policies) (page dated
  2025-05-22, read 2026-10-30) bear on one extension that carries this many groups. An extension
  must have a single purpose that is narrow and easy to understand, and functions that are clearly
  separate belong in separate extensions. The groups that need `debugger` are in a second extension
  for that reason and for the permission, which Chrome does not let an extension ask for later. It
  must request the narrowest permissions its features
  need, and not a permission for a feature that does not exist yet. The policy page does not say
  whether that covers optional permissions, and #3583 treats it as covering them. Manifest V3
  forbids running logic fetched from a remote source, except through the Debugger and User Scripts
  APIs, and the policy pages do not say whether tool calls from a model count as commands fetched
  from a remote source. A maintainer has to settle, before publishing, whether the groups are one
  purpose or more than one extension, whether each group's permissions stay as narrow as its tools,
  and what the store says about tool calls from a model. The store gives a copy an id different from
  the pinned key's, so `install` is given that id, which it writes into `allowed_origins`.
- **Names and defaults.** The group ids in `groups.js`, the presets `off`, `look`, `read` and
  `browse`, and the default lifetimes are the ones #3583 proposed, and none is final. A group is on
  until you turn it off, except `downloads`, `cookies`, `remove-data`, `scripts`, `input`,
  `input-trusted`, `capture-trusted`, `console-trusted`, `upload-trusted`, `screen` and `held`, which are on until Brave restarts. The spec records the values in BROWSER-18,
  BROWSER-20 and BROWSER-24, and a rename changes those clauses, `groups.js`, `lifetimes.js`, the
  relay's table and what a person has declared with `bravebot mcp add`.
- **What is not built.** `capture_tab_video` is not built: Brave allows `tabCapture` only on a tab you invoked the extension
  on, so a session cannot start it, and a video is not a type MCP-6 takes. A processor's choice fills
  only `element_number` of `click_element`, `type_text` and `select_option`, for a server declared
  with `--choice` for it. You are asked about the call with the number shown and then allow the
  element in the window ([ROUTE-10](../docs/specs/routing.md#ROUTE-10)); nothing else a processor
  chooses from a page is built. Acting in a frame and dragging by a distance are not built, and a
  file in a result is not taken. A picture of a page is taken
  ([BROWSER-35](../docs/specs/browser.md#BROWSER-35)), and so are a picture of the screen and a
  recording of a page ([BROWSER-42](../docs/specs/browser.md#BROWSER-42)), code the model wrote is run
  in a page ([BROWSER-46](../docs/specs/browser.md#BROWSER-46)), and a file of yours is given to a file
  input ([BROWSER-45](../docs/specs/browser.md#BROWSER-45)). `input` has six tools. Real
  input, hovering, dragging, a picture of the whole page, a page's console and a file chosen on a file input are the eight tools of
  `input-trusted`, `capture-trusted`, `console-trusted` and `upload-trusted`, done by a second extension that holds
  `debugger` ([BROWSER-37](../docs/specs/browser.md#BROWSER-37)).
  [BROWSER-29](../docs/specs/browser.md#BROWSER-29) says what does not exist, and [Open
  decisions](../docs/design/browser-extension.md#open-decisions) gives the options and the
  recommended default for each.
- **Cookie values.** Whether a session may read the value of a cookie, or set a cookie, is not
  decided, and no tool does either. A value is usually a session token, and whoever holds it acts as
  you at that site. The extension calls only `getAll` and `remove` of `chrome.cookies`, and a test
  fails if it calls another or reads a value. Before a tool is added a maintainer settles what reaches
  the model when you put a value in a message, whether a value you cannot judge in the question can be
  part of what you approve, and what the prompt for a call that carries a token asks.
  [BROWSER-30](../docs/specs/browser.md#BROWSER-30) is the clause the first such tool replaces.
- **Starting the profile.** `profile` prints the command and does not run it, because starting a
  program is done by `bravebot-sandbox` alone and `bravebot-browser` links no workspace crate
  ([SANDBOX-30](../docs/specs/sandboxing.md#SANDBOX-30), [layering](../docs/specs/layering.md)). A
  command of `bravebot` that starts Brave on the profile and prints its process id needs a ruling
  on which crate holds the code that reads the profile's marker. The profile was run in a headless
  Brave 1.98.62 on macOS and not on Linux. [BROWSER-36](../docs/specs/browser.md#BROWSER-36) says
  what it does.
- **Windows.** On Windows `bravebot-browser` only prints that the platform is not supported and
  exits, so no server starts and neither half runs. Whether a named pipe or a Unix socket is the
  right transport there is open.
