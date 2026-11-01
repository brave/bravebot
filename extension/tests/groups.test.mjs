// The group table: which tool is in which group, and what a group needs of the
// browser. What a call does with it is tested in tools.test.mjs.
//
// Run with `node --test extension/tests/`.

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import {
  ALL_URLS,
  ALLOWED_ONLY_TOOLS,
  ALWAYS_ON,
  GROUPS,
  HOST_PATTERNS,
  NEEDS_ALL_URLS,
  NO_ACCESS,
  PRESETS,
  SCOPE_ALL,
  SCOPE_ALLOWED,
  combine,
  groupOf,
  needed,
  neededTogether,
  patternOf,
  releasedBy,
  scopeOf,
  unsharedNeeds,
} from "../groups.js";
import { DEFAULT_SETTINGS, TOOLS } from "../tools.js";

// What page-read lists.
const PAGE_READ = ["tabs", "scripting", "webNavigation"];

// Every group that reads pages on all sites, which needs the two host patterns,
// or <all_urls> where a tool of the group takes pictures.
const EVERY_SITE = Object.freeze({
  allowed: [],
  scopes: Object.freeze(
    Object.fromEntries(
      Object.entries(GROUPS)
        .filter(([, group]) => group.hosts)
        .map(([id]) => [id, SCOPE_ALL]),
    ),
  ),
});

// The groups in the order the design lists them. A group is added or renamed on
// purpose, here as well.
const IDS = [
  "platform",
  "tabs-read",
  "history-read",
  "bookmarks-read",
  "page-read",
  "tabs-manage",
  "bookmarks-write",
  "navigate",
  "downloads",
  "cookies",
  "network-watch",
  "browser-info",
  "remove-data",
  "scripts",
  "input",
  "input-trusted",
  "capture-trusted",
  "console-trusted",
  "upload-trusted",
  "screen",
  "held",
];

test("the table holds the twenty-one groups, in order", () => {
  assert.deepEqual(Object.keys(GROUPS), IDS);
  assert.equal(ALWAYS_ON, "platform");
});

// A tool in two groups would run when either group's permissions were held, and
// a tool in none has no permissions to check.
test("each tool the extension answers is in one group, and each group tool is answered", () => {
  const listed = Object.values(GROUPS).flatMap((group) => group.tools);
  assert.equal(new Set(listed).size, listed.length);
  assert.deepEqual([...listed].sort(), Object.keys(TOOLS).sort());
});

test("each tool is in the group the design puts it in", () => {
  const where = Object.fromEntries(
    Object.keys(TOOLS).map((tool) => [tool, groupOf(tool)]),
  );
  assert.deepEqual(where, {
    get_platform_info: "platform",
    list_tabs: "tabs-read",
    list_frames: "tabs-read",
    list_windows: "tabs-read",
    list_tab_groups: "tabs-read",
    list_recently_closed: "tabs-read",
    read_page: "page-read",
    search_history: "history-read",
    get_visits: "history-read",
    list_top_sites: "history-read",
    list_synced_tabs: "history-read",
    search_bookmarks: "bookmarks-read",
    list_bookmark_folder: "bookmarks-read",
    list_recent_bookmarks: "bookmarks-read",
    list_reading_list: "bookmarks-read",
    open_tab: "navigate",
    navigate_tab: "navigate",
    reload_tab: "navigate",
    go_back: "navigate",
    go_forward: "navigate",
    open_window: "navigate",
    restore_closed: "navigate",
    search_web: "navigate",
    wait_for_load: "navigate",
    read_page_links: "page-read",
    read_page_forms: "page-read",
    list_page_elements: "page-read",
    screenshot_page: "page-read",
    save_page_archive: "downloads",
    focus_tab: "tabs-manage",
    pin_tab: "tabs-manage",
    mute_tab: "tabs-manage",
    move_tab: "tabs-manage",
    group_tabs: "tabs-manage",
    ungroup_tabs: "tabs-manage",
    update_tab_group: "tabs-manage",
    discard_tab: "tabs-manage",
    duplicate_tab: "tabs-manage",
    set_zoom: "tabs-manage",
    focus_window: "tabs-manage",
    resize_window: "tabs-manage",
    list_cookie_names: "cookies",
    delete_cookie: "cookies",
    list_requests: "network-watch",
    get_privacy_settings: "browser-info",
    get_content_settings: "browser-info",
    get_font_settings: "browser-info",
    get_system_info: "browser-info",
    get_idle_state: "browser-info",
    get_access_flags: "browser-info",
    keep_awake: "browser-info",
    create_bookmark: "bookmarks-write",
    create_bookmark_folder: "bookmarks-write",
    move_bookmark: "bookmarks-write",
    rename_bookmark: "bookmarks-write",
    add_reading_list_entry: "bookmarks-write",
    update_reading_list_entry: "bookmarks-write",
    close_tab: "remove-data",
    close_window: "remove-data",
    delete_history_entry: "remove-data",
    delete_history_range: "remove-data",
    clear_browsing_data: "remove-data",
    remove_bookmark: "remove-data",
    remove_bookmark_folder: "remove-data",
    remove_reading_list_entry: "remove-data",
    delete_downloaded_file: "remove-data",
    download_file: "downloads",
    list_downloads: "downloads",
    pause_download: "downloads",
    resume_download: "downloads",
    cancel_download: "downloads",
    show_download: "downloads",
    click_element: "input",
    type_text: "input",
    select_option: "input",
    press_key: "input",
    scroll_page: "input",
    wait_for_element: "input",
    click_element_trusted: "input-trusted",
    type_text_trusted: "input-trusted",
    press_key_trusted: "input-trusted",
    hover_element: "input-trusted",
    drag_element: "input-trusted",
    screenshot_full_page: "capture-trusted",
    read_console: "console-trusted",
    choose_file: "upload-trusted",
    run_script: "scripts",
    capture_screen: "screen",
    record_gif: "screen",
    take_held: "held",
  });
  assert.equal(groupOf("toString"), undefined);
});

// Each tool's switch starts off but the platform check's, whichever group it
// is in.
test("only the platform group's tools start on", () => {
  assert.deepEqual(
    Object.keys(DEFAULT_SETTINGS).sort(),
    Object.keys(TOOLS).sort(),
  );
  for (const [tool, on] of Object.entries(DEFAULT_SETTINGS)) {
    assert.equal(on, groupOf(tool) === ALWAYS_ON, tool);
  }
});

// The permissions the Chrome permissions page says cannot be specified as
// optional, as read on 2026-10-30. None is in an optional list of the manifest or
// in the permissions of a group, which are the optional ones. Of the nine only
// `debugger` could serve a group, `input`. That group acts through functions the
// extension injects and lists `tabs` and `scripting`, so it holds none of the nine
// (the proposal, brave/bravebot#3583). `input-trusted` lists no permission: a
// second extension holds `debugger` and does what its tools ask.
const CANNOT_BE_OPTIONAL = [
  "debugger",
  "declarativeNetRequest",
  "devtools",
  "geolocation",
  "mdns",
  "proxy",
  "tts",
  "ttsEngine",
  "wallpaper",
];

const MANIFEST = JSON.parse(
  readFileSync(new URL("../manifest.json", import.meta.url), "utf8"),
);

test("no permission that cannot be optional is listed as optional by the manifest or by a group, and input holds none", () => {
  assert.equal(new Set(CANNOT_BE_OPTIONAL).size, 9);
  for (const name of CANNOT_BE_OPTIONAL) {
    assert.ok(
      !MANIFEST.optional_permissions.includes(name),
      `the manifest lists ${name} as optional`,
    );
    assert.ok(
      !(MANIFEST.optional_host_permissions ?? []).includes(name),
      `the manifest lists ${name} as an optional host`,
    );
    assert.ok(
      !(MANIFEST.permissions ?? []).includes(name),
      `the manifest requires ${name}`,
    );
    for (const [id, group] of Object.entries(GROUPS)) {
      assert.ok(!group.permissions.includes(name), `${id} lists ${name}`);
    }
  }
  assert.deepEqual(GROUPS.input.permissions, ["tabs", "scripting"]);
  assert.equal(GROUPS.input.hosts, true);
  assert.equal(MANIFEST.devtools_page, undefined);
  assert.equal(MANIFEST.declarative_net_request, undefined);
});

// The tools of input-trusted are done by a second extension, so this one lists
// no permission for them and reads no host, whatever sites are allowed, and the
// group is limited to the allowed sites whatever its scope. Reading the sites in
// handle (tools.test.mjs) is what keeps it to them.
test("the input-trusted, capture-trusted and upload-trusted groups need no permission and no host and are limited to the allowed sites", () => {
  const sites = { allowed: ["https://news.example"], scopes: {} };
  for (const id of ["input-trusted", "capture-trusted", "upload-trusted"]) {
    const group = GROUPS[id];
    assert.deepEqual(group.permissions, [], id);
    assert.equal(group.hosts, false, id);
    assert.equal(group.allowedOnly, true, id);
    for (const access of [NO_ACCESS, sites, EVERY_SITE]) {
      assert.deepEqual(needed(id, GROUPS, access), {}, id);
    }
    assert.deepEqual(unsharedNeeds(id, [], GROUPS, sites), {}, id);
  }
  // Only these five groups have the flag, and no other group lists it as false.
  assert.deepEqual(
    Object.keys(GROUPS).filter((id) => "allowedOnly" in GROUPS[id]),
    ["scripts", "input-trusted", "capture-trusted", "console-trusted", "upload-trusted"],
  );
  // The picture tool is apart from the five that send input.
  assert.deepEqual(GROUPS["capture-trusted"].tools, ["screenshot_full_page"]);
});

// read_console is done by the second extension too, and is in a group of its own
// that lists nothing and is limited to the allowed sites.
test("the console-trusted group has the one tool, needs no permission and no host and is limited to the allowed sites", () => {
  const group = GROUPS["console-trusted"];
  assert.deepEqual(group.tools, ["read_console"]);
  assert.deepEqual(group.permissions, []);
  assert.equal(group.hosts, false);
  assert.equal(group.allowedOnly, true);
  const sites = { allowed: ["https://news.example"], scopes: {} };
  for (const access of [NO_ACCESS, sites, EVERY_SITE]) {
    assert.deepEqual(needed("console-trusted", GROUPS, access), {});
  }
  assert.deepEqual(unsharedNeeds("console-trusted", [], GROUPS, sites), {});
  assert.equal(groupOf("read_console"), "console-trusted");
  assert.ok(!GROUPS["input-trusted"].tools.includes("read_console"));
});

// run_script runs code with the browser's userScripts API, which runs a script
// only in a page the extension holds the host of. The group lists that
// permission and the hosts of the allowed sites, and is limited to them whatever
// scope is stored for it: a scope of all sites would be a request for every site
// for code that a person read once.
test("the scripts group lists userScripts and the hosts of the allowed sites, and a stored scope of all sites adds none", () => {
  const group = GROUPS.scripts;
  assert.deepEqual(group.tools, ["run_script"]);
  assert.equal(groupOf("run_script"), "scripts");
  assert.deepEqual(group.permissions, ["userScripts"]);
  assert.equal(group.hosts, true);
  assert.equal(group.allowedOnly, true);
  assert.deepEqual(needed("scripts"), { permissions: ["userScripts"] });
  const allowed = ["https://news.example", "http://127.0.0.1:8080"];
  const wanted = {
    permissions: ["userScripts"],
    origins: ["https://news.example/*", "http://127.0.0.1:8080/*"],
  };
  assert.deepEqual(needed("scripts", GROUPS, { allowed, scopes: {} }), wanted);
  assert.deepEqual(
    needed("scripts", GROUPS, { allowed, scopes: { scripts: SCOPE_ALL } }),
    wanted,
  );
  assert.deepEqual(needed("scripts", GROUPS, EVERY_SITE), {
    permissions: ["userScripts"],
  });
  // It is in no preset, so the permission is asked for by its own switch alone.
  assert.ok(!Object.values(PRESETS).flat().includes("scripts"));
});

// The platform group is the one a fresh install can use, so it asks the browser
// for nothing.
test("the platform group needs no permission and no host", () => {
  assert.deepEqual(GROUPS.platform.permissions, []);
  assert.equal(GROUPS.platform.hosts, false);
  assert.deepEqual(needed("platform"), {});
});

test("a group needs the hosts only if it reads pages", () => {
  assert.deepEqual(HOST_PATTERNS, ["https://*/*", "http://*/*"]);
  assert.equal(ALL_URLS, "<all_urls>");
  // page-read has a tool that takes pictures, which the browser lets an
  // extension do only with <all_urls>.
  assert.deepEqual(needed("page-read", GROUPS, EVERY_SITE), {
    permissions: PAGE_READ,
    origins: ["<all_urls>"],
  });
  assert.deepEqual(needed("tabs-read"), {
    permissions: ["tabs", "tabGroups", "webNavigation", "sessions"],
  });
  assert.deepEqual(needed("history-read"), {
    permissions: ["history", "topSites", "sessions"],
  });
  assert.deepEqual(needed("tabs-manage"), {
    permissions: ["tabs", "tabGroups"],
  });
  // navigate lists what its tools call: tabs to find a tab by its URL,
  // sessions to restore a closed tab, search to search with the default
  // provider, and webNavigation to see where a page committed.
  assert.deepEqual(needed("navigate"), {
    permissions: ["tabs", "sessions", "search", "webNavigation"],
  });
  // cookies reads the cookies a site is sent, so like page-read it needs the
  // hosts of the allowed sites, or the two patterns on all sites.
  assert.equal(GROUPS.cookies.hosts, true);
  assert.deepEqual(GROUPS.cookies.permissions, ["cookies"]);
  assert.deepEqual(needed("cookies"), { permissions: ["cookies"] });
  assert.deepEqual(needed("cookies", GROUPS, EVERY_SITE), {
    permissions: ["cookies"],
    origins: ["https://*/*", "http://*/*"],
  });
  // input finds a tab by its URL and acts through injected functions, so like
  // page-read it needs the hosts of the allowed sites, or the two patterns.
  assert.deepEqual(needed("input"), { permissions: ["tabs", "scripting"] });
  assert.deepEqual(needed("input", GROUPS, EVERY_SITE), {
    permissions: ["tabs", "scripting"],
    origins: ["https://*/*", "http://*/*"],
  });
  // screen takes a picture of the screen without a host and records a page only
  // with ALL_URLS, so on the allowed sites it needs the hosts of those sites
  // like the others, and on all sites the one pattern of the browser's capture.
  assert.deepEqual(needed("screen"), { permissions: ["desktopCapture", "tabs"] });
  assert.deepEqual(needed("screen", GROUPS, EVERY_SITE), {
    permissions: ["desktopCapture", "tabs"],
    origins: [ALL_URLS],
  });
  assert.deepEqual(needed("bookmarks-read"), {
    permissions: ["bookmarks", "readingList"],
  });
  assert.deepEqual(needed("bookmarks-write"), {
    permissions: ["bookmarks", "readingList"],
  });
  assert.equal(GROUPS["network-watch"].hosts, true);
  assert.deepEqual(GROUPS["network-watch"].permissions, ["webRequest"]);
  assert.deepEqual(needed("network-watch", GROUPS, EVERY_SITE), {
    permissions: ["webRequest"],
    origins: ["https://*/*", "http://*/*"],
  });
  assert.deepEqual(needed("network-watch"), { permissions: ["webRequest"] });
  assert.deepEqual(needed("browser-info"), {
    permissions: [
      "privacy",
      "contentSettings",
      "fontSettings",
      "system.cpu",
      "system.memory",
      "system.display",
      "system.storage",
      "idle",
      "power",
    ],
  });
  assert.deepEqual(needed("remove-data"), {
    permissions: [
      "tabs",
      "history",
      "bookmarks",
      "readingList",
      "browsingData",
      "downloads",
    ],
  });
  // downloads lists pageCapture for save_page_archive and tabs, without which
  // the browser leaves the address out of every tab and the tool finds none,
  // and no host.
  assert.deepEqual(needed("downloads"), {
    permissions: ["downloads", "pageCapture", "tabs"],
  });
  // held reads no page: the browser gives the extension what the person selected
  // or pointed at in its context menu entry, so it lists one permission and no
  // host, on every scope.
  assert.deepEqual(GROUPS.held.permissions, ["contextMenus"]);
  assert.equal(GROUPS.held.hosts, false);
  assert.deepEqual(needed("held"), { permissions: ["contextMenus"] });
  assert.deepEqual(needed("held", GROUPS, EVERY_SITE), {
    permissions: ["contextMenus"],
  });
  for (const id of IDS.filter(
    (id) =>
      !["page-read", "cookies", "network-watch", "scripts", "input", "screen"].includes(id),
  )) {
    assert.equal(GROUPS[id].hosts, false, id);
  }
});

// Asking for a group's permissions again must not widen it, and the table the
// browser reads from must not change under a caller.
test("what a group needs is a copy", () => {
  needed("page-read", GROUPS, EVERY_SITE).permissions.push("cookies");
  needed("page-read", GROUPS, EVERY_SITE).origins.push("<all_urls>");
  assert.ok(!GROUPS["page-read"].permissions.includes("cookies"));
  assert.deepEqual(needed("page-read", GROUPS, EVERY_SITE).origins, [ALL_URLS]);
  needed("cookies", GROUPS, EVERY_SITE).origins.push("<all_urls>");
  assert.deepEqual(needed("cookies", GROUPS, EVERY_SITE).origins, HOST_PATTERNS);
  const allowed = { allowed: ["https://news.example"], scopes: {} };
  needed("page-read", GROUPS, allowed).origins.push("<all_urls>");
  assert.deepEqual(allowed.allowed, ["https://news.example"]);
});

// The browser grants by permission, so what is taken back when a group goes off
// is what no other group on still needs.
test("turning a group off takes back only what no other group on needs", () => {
  // webNavigation and tabs are used by three groups. tabGroups is used by
  // tabs-read alone, and sessions by it, navigate and history-read.
  assert.deepEqual(unsharedNeeds("tabs-read", ["page-read", "navigate"]), {
    permissions: ["tabGroups"],
  });
  assert.deepEqual(
    unsharedNeeds("tabs-read", ["page-read", "navigate", "history-read"]),
    { permissions: ["tabGroups"] },
  );
  assert.deepEqual(unsharedNeeds("history-read", ["tabs-read"]), {
    permissions: ["history", "topSites"],
  });
  assert.deepEqual(unsharedNeeds("navigate", ["tabs-read"]), {
    permissions: ["search"],
  });
  assert.deepEqual(unsharedNeeds("navigate", ["page-read"]), {
    permissions: ["sessions", "search"],
  });
  assert.deepEqual(unsharedNeeds("navigate", []), needed("navigate"));
  assert.deepEqual(unsharedNeeds("page-read", ["tabs-read"], GROUPS, EVERY_SITE), {
    permissions: ["scripting"],
    origins: ["<all_urls>"],
  });
  assert.deepEqual(unsharedNeeds("tabs-read", ["page-read"]), {
    permissions: ["tabGroups", "sessions"],
  });
  assert.deepEqual(unsharedNeeds("tabs-read", []), needed("tabs-read"));
  assert.deepEqual(unsharedNeeds("platform", ["tabs-read"]), {});
  // bookmarks-read and bookmarks-write list the same two permissions: each
  // keeps them for the other, and the last of the two takes them back.
  assert.deepEqual(unsharedNeeds("bookmarks-write", ["bookmarks-read"]), {});
  assert.deepEqual(unsharedNeeds("bookmarks-read", ["bookmarks-write"]), {});
  assert.deepEqual(unsharedNeeds("bookmarks-write", ["tabs-manage"]), {
    permissions: ["bookmarks", "readingList"],
  });
});

// bookmarks covers reading and writing, and a host pattern covers every group
// that reads pages: the permission stays while one of them is on, and the last
// to go takes it back.
test("a permission two groups share stays until both are off", () => {
  const table = {
    read: { permissions: ["bookmarks"], hosts: false, tools: [] },
    write: { permissions: ["bookmarks", "readingList"], hosts: false, tools: [] },
    pages: { permissions: ["scripting"], hosts: true, tools: [] },
    cookies: { permissions: ["cookies"], hosts: true, tools: [] },
  };
  const every = { allowed: [], scopes: { pages: SCOPE_ALL, cookies: SCOPE_ALL } };
  assert.deepEqual(unsharedNeeds("read", ["write"], table, every), {});
  assert.deepEqual(unsharedNeeds("write", ["read"], table, every), {
    permissions: ["readingList"],
  });
  assert.deepEqual(unsharedNeeds("write", [], table, every), {
    permissions: ["bookmarks", "readingList"],
  });
  assert.deepEqual(unsharedNeeds("pages", ["cookies"], table, every), {
    permissions: ["scripting"],
  });
  assert.deepEqual(unsharedNeeds("cookies", ["pages", "read"], table, every), {
    permissions: ["cookies"],
  });
  assert.deepEqual(unsharedNeeds("pages", ["read"], table, every), {
    permissions: ["scripting"],
    origins: ["https://*/*", "http://*/*"],
  });
});

// A preset names the groups the relay declares for the same name, in the same
// order, and only groups that have a tool, so a person who applies a preset in
// the options and one who installs it for a session get the same groups.
test("the presets are Off, Look, Read and Browse, and name groups that have a tool", () => {
  assert.deepEqual(PRESETS, {
    off: ["platform"],
    look: ["tabs-read", "history-read", "bookmarks-read"],
    read: ["tabs-read", "history-read", "bookmarks-read", "page-read"],
    browse: [
      "tabs-read",
      "history-read",
      "bookmarks-read",
      "page-read",
      "tabs-manage",
      "bookmarks-write",
      "navigate",
    ],
  });
  assert.deepEqual(Object.keys(PRESETS), ["off", "look", "read", "browse"]);
  for (const [name, ids] of Object.entries(PRESETS)) {
    assert.equal(new Set(ids).size, ids.length, name);
    for (const id of ids) {
      assert.ok(GROUPS[id]?.tools.length > 0, `${name}: ${id} has no tool`);
    }
  }
});

test("each preset after Look adds groups to the one before it", () => {
  assert.deepEqual(PRESETS.read, [...PRESETS.look, "page-read"]);
  assert.deepEqual(PRESETS.browse, [
    ...PRESETS.read,
    "tabs-manage",
    "bookmarks-write",
    "navigate",
  ]);
});

// Each of these is a choice of its own, so a preset never turns one on.
test("the groups turned on one at a time are in no preset", () => {
  const named = new Set(Object.values(PRESETS).flat());
  for (const id of [
    "remove-data",
    "cookies",
    "downloads",
    "network-watch",
    "browser-info",
    "scripts",
    "input",
    "input-trusted",
    "console-trusted",
    "upload-trusted",
    "capture-trusted",
    "held",
    "screen",
  ]) {
    assert.ok(Object.hasOwn(GROUPS, id), id);
    assert.ok(!named.has(id), `${id} is in a preset`);
  }
});

// One request covers every group of a preset, so a permission two groups list
// is asked for once.
test("what several groups need lists each permission and host once", () => {
  assert.deepEqual(neededTogether(PRESETS.look), {
    permissions: [
      "tabs",
      "tabGroups",
      "webNavigation",
      "sessions",
      "history",
      "topSites",
      "bookmarks",
      "readingList",
    ],
  });
  assert.deepEqual(neededTogether(PRESETS.browse, GROUPS, EVERY_SITE), {
    permissions: [
      "tabs",
      "tabGroups",
      "webNavigation",
      "sessions",
      "history",
      "topSites",
      "bookmarks",
      "readingList",
      "scripting",
      "search",
    ],
    origins: ["<all_urls>"],
  });
  // Browse adds tabs-manage, bookmarks-write and navigate to Read, and asks the
  // browser for one permission more than Read does, search, which only navigate
  // lists: the others of the three are among those of Look and page-read.
  const read = neededTogether(PRESETS.read, GROUPS, EVERY_SITE);
  assert.deepEqual(neededTogether(PRESETS.browse, GROUPS, EVERY_SITE), {
    ...read,
    permissions: [...read.permissions, "search"],
  });
  assert.deepEqual(neededTogether(PRESETS.off), {});
  assert.deepEqual(neededTogether([]), {});
  assert.deepEqual(
    combine([{ permissions: ["a"] }, { origins: ["x"] }, { permissions: ["a", "b"] }]),
    { permissions: ["a", "b"], origins: ["x"] },
  );
  neededTogether(PRESETS.read).permissions.push("cookies");
  assert.ok(!neededTogether(PRESETS.read).permissions.includes("cookies"));
});

// A group that reads pages is limited to the allowed sites unless the person
// chose all sites for it, so a fresh install lists no host and a group the
// person turns on reaches nothing until a site is allowed.
test("a group that reads pages needs the hosts of the allowed sites, or on all sites the two patterns, or <all_urls> for pictures", () => {
  assert.equal(scopeOf("page-read", NO_ACCESS), SCOPE_ALLOWED);
  assert.deepEqual(needed("page-read"), { permissions: PAGE_READ });
  const allowed = {
    allowed: ["https://news.example", "http://intranet:8080"],
    scopes: {},
  };
  assert.deepEqual(needed("page-read", GROUPS, allowed), {
    permissions: PAGE_READ,
    origins: ["https://news.example/*", "http://intranet:8080/*"],
  });
  const all = { ...allowed, scopes: { "page-read": SCOPE_ALL } };
  assert.equal(scopeOf("page-read", all), SCOPE_ALL);
  assert.deepEqual(needed("page-read", GROUPS, all).origins, [ALL_URLS]);
  assert.deepEqual(
    needed("cookies", GROUPS, { ...all, scopes: { cookies: SCOPE_ALL } }).origins,
    HOST_PATTERNS,
  );
  assert.equal(patternOf("https://news.example"), "https://news.example/*");
  // Another group's scope is not this group's, and a group that reads no page
  // lists no host whatever is allowed.
  assert.equal(scopeOf("page-read", { allowed: [], scopes: { cookies: SCOPE_ALL } }), SCOPE_ALLOWED);
  assert.deepEqual(needed("tabs-read", GROUPS, all), {
    permissions: ["tabs", "tabGroups", "webNavigation", "sessions"],
  });
});

// Two groups on the allowed sites share their hosts, and a group on all sites
// lists the two patterns, which is not the pattern of any one site.
test("turning a group off takes back the hosts that no group still on needs", () => {
  const table = {
    pages: { permissions: ["scripting"], hosts: true, tools: [] },
    cookies: { permissions: ["cookies"], hosts: true, tools: [] },
  };
  const sites = { allowed: ["https://news.example"], scopes: {} };
  assert.deepEqual(unsharedNeeds("pages", ["cookies"], table, sites), {
    permissions: ["scripting"],
  });
  assert.deepEqual(unsharedNeeds("pages", [], table, sites), {
    permissions: ["scripting"],
    origins: ["https://news.example/*"],
  });
  const mixed = { allowed: ["https://news.example"], scopes: { cookies: SCOPE_ALL } };
  assert.deepEqual(unsharedNeeds("pages", ["cookies"], table, mixed), {
    permissions: ["scripting"],
    origins: ["https://news.example/*"],
  });
  assert.deepEqual(unsharedNeeds("cookies", ["pages"], table, mixed), {
    permissions: ["cookies"],
    origins: ["https://*/*", "http://*/*"],
  });
  // The real groups that read cookies and requests, on all sites, keep the two
  // patterns for each other. page-read lists <all_urls> instead, which is not
  // the pattern of either, so it keeps neither of them and neither keeps its.
  assert.deepEqual(
    unsharedNeeds("cookies", ["network-watch"], GROUPS, EVERY_SITE),
    { permissions: ["cookies"] },
  );
  assert.deepEqual(
    unsharedNeeds("cookies", ["page-read"], GROUPS, EVERY_SITE),
    { permissions: ["cookies"], origins: HOST_PATTERNS },
  );
  assert.deepEqual(
    unsharedNeeds("page-read", ["cookies", "network-watch"], GROUPS, EVERY_SITE),
    { permissions: PAGE_READ, origins: [ALL_URLS] },
  );
});

// Narrowing a group from all sites to the allowed ones gives back the two
// patterns unless another group still on is on all sites.
test("narrowing a scope releases the hosts the old scope held and the new one does not", () => {
  const table = {
    pages: { permissions: ["scripting"], hosts: true, tools: [] },
    cookies: { permissions: ["cookies"], hosts: true, tools: [] },
  };
  const before = { allowed: ["https://news.example"], scopes: { pages: SCOPE_ALL } };
  const after = { allowed: ["https://news.example"], scopes: {} };
  assert.deepEqual(releasedBy(["pages"], before, after, table), {
    origins: ["https://*/*", "http://*/*"],
  });
  const both = { ...before, scopes: { pages: SCOPE_ALL, cookies: SCOPE_ALL } };
  const one = { ...before, scopes: { cookies: SCOPE_ALL } };
  assert.deepEqual(releasedBy(["pages", "cookies"], both, one, table), {});
  assert.deepEqual(releasedBy(["pages"], after, after, table), {});
});

// The browser lets an extension capture a tab only with <all_urls>. Only a group
// with a tool that takes a picture lists it, and only on all sites, so a group
// that reads pages for another reason asks for no more than the two patterns.
test("only a group with a tool that takes pictures needs <all_urls>, and only on all sites", () => {
  assert.deepEqual(NEEDS_ALL_URLS, ["screenshot_page", "record_gif"]);
  const taking = Object.keys(GROUPS).filter((id) =>
    GROUPS[id].tools.some((tool) => NEEDS_ALL_URLS.includes(tool)),
  );
  assert.deepEqual(taking, ["page-read", "screen"]);
  const allowed = { allowed: ["https://news.example"], scopes: {} };
  for (const id of Object.keys(GROUPS)) {
    const listed = (access) => needed(id, GROUPS, access).origins ?? [];
    assert.equal(
      listed(EVERY_SITE).includes(ALL_URLS),
      taking.includes(id),
      id,
    );
    assert.ok(!listed(allowed).includes(ALL_URLS), id);
    assert.ok(!listed(NO_ACCESS).includes(ALL_URLS), id);
  }
});

// A request the browser takes has to be contained by a pattern the manifest
// offers as optional, so every host a group can need is among them.
test("every host a group can need is one the manifest offers as optional", () => {
  assert.deepEqual(MANIFEST.optional_host_permissions, [...HOST_PATTERNS, ALL_URLS]);
  const sites = { allowed: ["https://news.example"], scopes: {} };
  for (const id of Object.keys(GROUPS)) {
    for (const access of [NO_ACCESS, sites, EVERY_SITE]) {
      for (const pattern of needed(id, GROUPS, access).origins ?? []) {
        const offered = MANIFEST.optional_host_permissions.some(
          (held) =>
            held === pattern ||
            held === ALL_URLS ||
            pattern.startsWith(held.slice(0, -3)),
        );
        assert.ok(offered, `${id} needs ${pattern}`);
      }
    }
  }
});

// save_page_archive archives a tab with pageCapture, saves the file with
// downloads and finds the tab by its address, which the browser gives an
// extension only with tabs or a host that covers the tab, so downloads lists all
// three. The Read and Browse presets name page-read and not downloads, so they
// ask the browser for neither pageCapture nor downloads, and for tabs as they
// did before. The group reads no host and has no scope, so the limit to the
// allowed sites is the tool's own.
test("save_page_archive is in the downloads group, which lists pageCapture and tabs and no host, and no preset asks for pageCapture or downloads", () => {
  const listing = (permission) =>
    Object.entries(GROUPS)
      .filter(([, group]) => group.permissions.includes(permission))
      .map(([id]) => id);
  assert.deepEqual(listing("pageCapture"), ["downloads"]);
  assert.deepEqual(listing("downloads"), ["downloads", "remove-data"]);
  assert.deepEqual(listing("tabs"), [
    "tabs-read",
    "page-read",
    "tabs-manage",
    "navigate",
    "downloads",
    "remove-data",
    "input",
    "screen",
  ]);
  assert.equal(groupOf("save_page_archive"), "downloads");
  assert.ok(MANIFEST.optional_permissions.includes("pageCapture"));
  assert.ok(!MANIFEST.permissions.includes("pageCapture"));
  assert.ok(MANIFEST.optional_permissions.includes("tabs"));
  assert.ok(!MANIFEST.permissions.includes("tabs"));
  for (const [name, ids] of Object.entries(PRESETS)) {
    const asked = neededTogether(ids).permissions ?? [];
    assert.ok(!asked.includes("pageCapture"), name);
    assert.ok(!asked.includes("downloads"), name);
  }
  // tabs is listed by presets' groups already, so adding it to downloads
  // changes no preset's request.
  assert.deepEqual(neededTogether(PRESETS.read).permissions, [
    "tabs",
    "tabGroups",
    "webNavigation",
    "sessions",
    "history",
    "topSites",
    "bookmarks",
    "readingList",
    "scripting",
  ]);
  assert.deepEqual(needed("page-read"), { permissions: PAGE_READ });
  assert.deepEqual(unsharedNeeds("downloads", ["remove-data"]), {
    permissions: ["pageCapture"],
  });
  assert.deepEqual(unsharedNeeds("downloads", ["page-read"]), {
    permissions: ["downloads", "pageCapture"],
  });
  assert.deepEqual(unsharedNeeds("downloads", ["page-read", "remove-data"]), {
    permissions: ["pageCapture"],
  });
  assert.deepEqual(unsharedNeeds("page-read", ["downloads"]), {
    permissions: ["scripting", "webNavigation"],
  });
  // pageCapture asks for no host: the group needs none under any scope.
  const everywhere = { allowed: ["https://shop.example"], scopes: { downloads: SCOPE_ALL } };
  assert.deepEqual(needed("downloads", GROUPS, everywhere), {
    permissions: ["downloads", "pageCapture", "tabs"],
  });
});

// A tool in a group with no scope that reads a page the browser does not limit
// to a host is held to the allowed sites by name, and only that tool: the other
// tools of the group name a download or no page.
test("the tools limited to the allowed sites are named, and each is in a group with no scope of its own", () => {
  assert.deepEqual(ALLOWED_ONLY_TOOLS, ["save_page_archive"]);
  for (const tool of ALLOWED_ONLY_TOOLS) {
    const group = GROUPS[groupOf(tool)];
    assert.equal(group.hosts, false, tool);
    assert.ok(!("allowedOnly" in group), tool);
  }
  assert.ok(!ALLOWED_ONLY_TOOLS.includes("download_file"));
});
