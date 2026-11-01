// The capability groups: which tools belong to which, and the browser
// permissions each group needs.
//
// The manifest requires only nativeMessaging, storage and alarms. The APIs a
// group's tools call are optional permissions, and the hosts they read are
// optional host permissions, so a fresh install holds none of them and the
// browser itself refuses an API whose group nobody turned on. A tool runs only
// while the browser holds what its group needs (handle in tools.js).
//
// Each row is one group, on one line, and is JSON, so crates/browser/tests can
// read the table without running it:
//   permissions  the optional permissions the group's tools call
//   hosts        whether they read pages, which needs the hosts of the allowed
//                sites or HOST_PATTERNS, by the group's scope, or ALL_URLS for
//                a group with a tool in NEEDS_ALL_URLS (needed)
//   allowedOnly  present and true for a group limited to the allowed sites
//                whatever the scope: it has no scope to choose, and the check
//                of the allowed sites runs for every call (handle in tools.js).
//                A group with `hosts` and `allowedOnly` needs the host pattern
//                of each allowed site and never HOST_PATTERNS or ALL_URLS
//   tools        the tools of the group, which are the methods in tools.js
// A group lists only what its tools use today. A group with no tool yet lists
// none, and a tool added to a group adds what it calls to that group's line.
// `input` acts through functions the extension injects (scripting), and never
// through `debugger`, which cannot be an optional permission.
// `input-trusted`, `capture-trusted`, `console-trusted` and `upload-trusted` list no
// permission and read no host: their tools are done by the BraveBot Trusted Input extension
// (extension-trusted/), which holds `debugger`, and trusted.js sends each call
// to it. `capture-trusted` is apart from `input-trusted` because its one tool
// sends no input and reads the whole of a page, including what nobody sees, so
// turning it on says that and nothing about clicks. `console-trusted` is apart
// from both because its one tool sends no input and reads the console of a page,
// which holds whatever the page's scripts printed. `upload-trusted` is apart from
// all three because its one tool gives a file of the person's to a page, which
// the page's scripts may send on at once. `held` reads no page: the
// browser gives the extension what a person selects or points at when they use
// its entry in the context menu (held.js). `downloads` lists pageCapture and
// tabs for save_page_archive: the browser makes the archive of a tab with
// pageCapture, which asks for no host and which Brave words as access to every
// site, and the extension saves it with downloads. The tool finds the tab by its
// address, which Brave leaves out of every tab for an extension that holds
// neither tabs nor the tab's site, and the group reads no host, so it lists
// tabs. The group has no scope, so the tool is limited to the allowed sites by
// ALLOWED_ONLY_TOOLS. `scripts` lists userScripts for run_script, which runs code
// in one page (scripts.js). It reads hosts, since the browser runs a user script
// only in a page the extension holds the host of, and is limited to the allowed
// sites with no choice of all sites. userScripts also needs the person's own
// "Allow User Scripts" switch on the extension's details page, which an
// extension cannot turn on.
export const GROUPS = Object.freeze({
  "platform": { "permissions": [], "hosts": false, "tools": ["get_platform_info"] },
  "tabs-read": { "permissions": ["tabs", "tabGroups", "webNavigation", "sessions"], "hosts": false, "tools": ["list_tabs", "list_frames", "list_windows", "list_tab_groups", "list_recently_closed"] },
  "history-read": { "permissions": ["history", "topSites", "sessions"], "hosts": false, "tools": ["search_history", "get_visits", "list_top_sites", "list_synced_tabs"] },
  "bookmarks-read": { "permissions": ["bookmarks", "readingList"], "hosts": false, "tools": ["search_bookmarks", "list_bookmark_folder", "list_recent_bookmarks", "list_reading_list"] },
  "page-read": { "permissions": ["tabs", "scripting", "webNavigation"], "hosts": true, "tools": ["read_page", "read_page_links", "read_page_forms", "list_page_elements", "screenshot_page"] },
  "tabs-manage": { "permissions": ["tabs", "tabGroups"], "hosts": false, "tools": ["focus_tab", "pin_tab", "mute_tab", "move_tab", "group_tabs", "ungroup_tabs", "update_tab_group", "discard_tab", "duplicate_tab", "set_zoom", "focus_window", "resize_window"] },
  "bookmarks-write": { "permissions": ["bookmarks", "readingList"], "hosts": false, "tools": ["create_bookmark", "create_bookmark_folder", "move_bookmark", "rename_bookmark", "add_reading_list_entry", "update_reading_list_entry"] },
  "navigate": { "permissions": ["tabs", "sessions", "search", "webNavigation"], "hosts": false, "tools": ["open_tab", "navigate_tab", "reload_tab", "go_back", "go_forward", "open_window", "restore_closed", "search_web", "wait_for_load"] },
  "downloads": { "permissions": ["downloads", "pageCapture", "tabs"], "hosts": false, "tools": ["save_page_archive", "download_file", "list_downloads", "pause_download", "resume_download", "cancel_download", "show_download"] },
  "cookies": { "permissions": ["cookies"], "hosts": true, "tools": ["list_cookie_names", "delete_cookie"] },
  "network-watch": { "permissions": ["webRequest"], "hosts": true, "tools": ["list_requests"] },
  "browser-info": { "permissions": ["privacy", "contentSettings", "fontSettings", "system.cpu", "system.memory", "system.display", "system.storage", "idle", "power"], "hosts": false, "tools": ["get_privacy_settings", "get_content_settings", "get_font_settings", "get_system_info", "get_idle_state", "get_access_flags", "keep_awake"] },
  "remove-data": { "permissions": ["tabs", "history", "bookmarks", "readingList", "browsingData", "downloads"], "hosts": false, "tools": ["close_tab", "close_window", "delete_history_entry", "delete_history_range", "clear_browsing_data", "remove_bookmark", "remove_bookmark_folder", "remove_reading_list_entry", "delete_downloaded_file"] },
  "scripts": { "permissions": ["userScripts"], "hosts": true, "allowedOnly": true, "tools": ["run_script"] },
  "input": { "permissions": ["tabs", "scripting"], "hosts": true, "tools": ["click_element", "type_text", "select_option", "press_key", "scroll_page", "wait_for_element"] },
  "input-trusted": { "permissions": [], "hosts": false, "allowedOnly": true, "tools": ["click_element_trusted", "type_text_trusted", "press_key_trusted", "hover_element", "drag_element"] },
  "capture-trusted": { "permissions": [], "hosts": false, "allowedOnly": true, "tools": ["screenshot_full_page"] },
  "console-trusted": { "permissions": [], "hosts": false, "allowedOnly": true, "tools": ["read_console"] },
  "upload-trusted": { "permissions": [], "hosts": false, "allowedOnly": true, "tools": ["choose_file"] },
  "screen": { "permissions": ["desktopCapture", "tabs"], "hosts": true, "tools": ["capture_screen", "record_gif"] },
  "held": { "permissions": ["contextMenus"], "hosts": false, "tools": ["take_held"] },
});

// The group whose tools start on. Its tools tell nothing about the person and
// call no API that needs a permission, so it is the only group a fresh install
// can use.
export const ALWAYS_ON = "platform";

// The presets of the options page: each is a set of groups turned on together,
// with one request to the browser for everything they list. A preset names only
// groups that have a tool. `bravebot-browser install --preset` names the same
// groups. The groups `downloads`, `cookies`, `network-watch`,
// `browser-info`, `remove-data`, `scripts`, `input`, `input-trusted`,
// `capture-trusted`, `console-trusted`, `upload-trusted`, `screen` and `held` are in no
// preset and are turned on one at a time.
//
// Each row is one preset, on one line, and is JSON, as the rows of GROUPS are.
export const PRESETS = Object.freeze({
  "off": ["platform"],
  "look": ["tabs-read", "history-read", "bookmarks-read"],
  "read": ["tabs-read", "history-read", "bookmarks-read", "page-read"],
  "browse": ["tabs-read", "history-read", "bookmarks-read", "page-read", "tabs-manage", "bookmarks-write", "navigate"],
});

// The hosts a group with `hosts` reads when its scope is all sites, as the
// manifest lists them in optional_host_permissions. They are the patterns the
// permissions page documents, and a request may ask for a subset of them.
export const HOST_PATTERNS = Object.freeze(["https://*/*", "http://*/*"]);

// The host permission chrome.tabs.captureVisibleTab requires. It is this
// pattern as written and no other: Brave 1.98 refused the call with the two
// patterns above, with `*://*/*`, with a site's own pattern, and with
// `activeTab` held, and ran it with this one. It is an optional host permission
// in the manifest beside the two patterns, which it contains.
export const ALL_URLS = "<all_urls>";

// The tools that need ALL_URLS. A group with one of them needs it, in place of
// HOST_PATTERNS, when its scope is all sites. Under the scope of the allowed
// sites the tool is refused, since the browser would refuse the call.
export const NEEDS_ALL_URLS = Object.freeze(["screenshot_page", "record_gif"]);

// The tools of a group with `hosts` that name no page and read none, so they are
// not limited to the allowed sites and do not need a host. capture_screen takes
// what the person chooses in the browser's picker, which is not a page.
export const NAMES_NO_PAGE = Object.freeze(["capture_screen"]);

// The tools limited to the allowed sites whatever their group, because the
// browser permission they call names no host and so limits them to no site. The
// check of the allowed sites runs for every call of one (handle in tools.js),
// as it does for a group with `allowedOnly`, and a tool of the same group that
// names a URL for another reason is not held to it.
export const ALLOWED_ONLY_TOOLS = Object.freeze(["save_page_archive"]);

// The two scopes a group with `hosts` offers. A group limited to the allowed
// sites needs the host pattern of each site the person allowed. A group on all
// sites needs HOST_PATTERNS, or ALL_URLS where a tool of the group needs it, and
// the sites the person blocked stay out of reach by the blocked list, which the
// browser knows nothing of.
export const SCOPE_ALLOWED = "allowed";
export const SCOPE_ALL = "all";

// The sites the person allowed, as origins, and the groups whose scope is all
// sites. A group not named in `scopes` is limited to the allowed sites, so with
// nothing saved a group that reads pages needs no host and reaches no site.
// siteAccess in tools.js reads the saved values.
export const NO_ACCESS = Object.freeze({
  allowed: Object.freeze([]),
  scopes: Object.freeze({}),
});

// The scope of the group under `access`.
export function scopeOf(id, access) {
  return access.scopes[id] === SCOPE_ALL ? SCOPE_ALL : SCOPE_ALLOWED;
}

// `access` with the groups among `ids` that read pages limited to the allowed
// sites, as a preset leaves them.
export function limitedToAllowed(ids, access, groups = GROUPS) {
  const scopes = { ...access.scopes };
  for (const id of ids) {
    if (groups[id].hosts) {
      delete scopes[id];
    }
  }
  return { allowed: access.allowed, scopes };
}

// The host pattern of one site, which is written as an origin.
export function patternOf(origin) {
  return `${origin}/*`;
}

// Whether the group has a tool that needs ALL_URLS.
export function capturesPages(id, groups = GROUPS) {
  return groups[id].tools.some((tool) => NEEDS_ALL_URLS.includes(tool));
}

// The host patterns a group with `hosts` needs under `access`.
function hostsOf(id, groups, access) {
  if (groups[id].allowedOnly === true || scopeOf(id, access) !== SCOPE_ALL) {
    return access.allowed.map(patternOf);
  }
  return capturesPages(id, groups) ? [ALL_URLS] : [...HOST_PATTERNS];
}

// The id of the group that holds `tool`, or undefined where none does.
export function groupOf(tool) {
  return Object.keys(GROUPS).find((id) => GROUPS[id].tools.includes(tool));
}

// What the browser has to hold for the group's tools to run, in the form
// chrome.permissions takes. A part the group does not need is left out, so a
// group that reads pages and is limited to allowed sites, with none allowed,
// needs no host.
export function needed(id, groups = GROUPS, access = NO_ACCESS) {
  const { permissions, hosts } = groups[id];
  const origins = hosts ? hostsOf(id, groups, access) : [];
  return {
    ...(permissions.length > 0 ? { permissions: [...permissions] } : {}),
    ...(origins.length > 0 ? { origins } : {}),
  };
}

// The union of chrome.permissions queries, each permission and host once, in
// the order first met. A part nobody lists is left out.
export function combine(queries) {
  const each = (kind) => [
    ...new Set(queries.flatMap((query) => query[kind] ?? [])),
  ];
  const permissions = each("permissions");
  const origins = each("origins");
  return {
    ...(permissions.length > 0 ? { permissions } : {}),
    ...(origins.length > 0 ? { origins } : {}),
  };
}

// What the browser has to hold for all of the groups, which is what one request
// for them asks.
export function neededTogether(ids, groups = GROUPS, access = NO_ACCESS) {
  return combine(ids.map((id) => needed(id, groups, access)));
}

// Whether the browser holds everything the group needs now. A group that needs
// nothing is held without asking the browser. An answer that is not true is not
// a yes.
export async function holds(chrome, id, access = NO_ACCESS) {
  const wanted = needed(id, GROUPS, access);
  if (Object.keys(wanted).length === 0) {
    return true;
  }
  return (await chrome.permissions.contains(wanted)) === true;
}

// What is in `wanted` and in no query of `kept`.
function without(wanted, kept) {
  const unshared = (kind) =>
    (wanted[kind] ?? []).filter(
      (entry) => !kept.some((other) => (other[kind] ?? []).includes(entry)),
    );
  const permissions = unshared("permissions");
  const origins = unshared("origins");
  return {
    ...(permissions.length > 0 ? { permissions } : {}),
    ...(origins.length > 0 ? { origins } : {}),
  };
}

// What the group needs that no group in `others` needs: what can be taken back
// from the browser when the group is turned off. The browser grants by
// permission, so a permission two groups use, such as bookmarks for reading and
// writing, stays while either of them is on.
export function unsharedNeeds(id, others, groups = GROUPS, access = NO_ACCESS) {
  return without(
    needed(id, groups, access),
    others.map((other) => needed(other, groups, access)),
  );
}

// What the groups in `ids` needed under `before` and need no longer under
// `after`: what can be taken back from the browser when a group's scope is
// narrowed.
export function releasedBy(ids, before, after, groups = GROUPS) {
  return without(neededTogether(ids, groups, before), [
    neededTogether(ids, groups, after),
  ]);
}
