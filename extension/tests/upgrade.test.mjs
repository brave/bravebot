// What a person who upgrades from a version without groups finds: the settings
// that version saved still read, nothing runs until its group is turned on, and
// the manifest asks for the Chrome version the groups need.
//
// Run with `node --test extension/tests/`.

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { GROUPS, NO_ACCESS, groupOf } from "../groups.js";
import { GROUP_SWITCHES_KEY, turnOn } from "../switches.js";
import {
  SETTINGS_KEY,
  blockedSites,
  groupSwitches,
  handle,
  settings,
} from "../tools.js";
import { fakeChrome, reached } from "./fake-chrome.mjs";

// What the version before groups saved, under the keys it used: a switch for
// each tool it had, the blocked sites, and nothing else. That version required every permission at
// install, so a person who upgrades may still hold them all.
const BEFORE_GROUPS = {
  tools: {
    get_platform_info: true,
    list_tabs: true,
    list_frames: true,
    read_page: true,
    search_history: false,
    search_bookmarks: true,
    open_tab: true,
  },
  blocked_sites: ["https://bank.example", "http://intranet:8080"],
};

const tabs = [
  { id: 1, windowId: 1, title: "News", url: "https://news.example/" },
  { id: 2, windowId: 1, title: "Bank", url: "https://bank.example/account" },
];

const call = (chrome, method) => handle({ id: 1, method }, chrome);

test("the tool switches and blocked sites the earlier version saved still read", async () => {
  const chrome = fakeChrome({ local: structuredClone(BEFORE_GROUPS) });
  const tools = await settings(chrome);
  assert.equal(tools.list_tabs, true);
  assert.equal(tools.read_page, true);
  assert.equal(tools.search_history, false);
  assert.equal(tools.open_tab, true);
  // A tool the earlier version had no switch for takes the default.
  assert.equal(tools.list_windows, false);
  assert.deepEqual(await blockedSites(chrome), BEFORE_GROUPS.blocked_sites);
  assert.deepEqual(await groupSwitches(chrome), {});
});

// The switch an earlier version saved is on, and the browser may still hold the
// permissions it required. Neither starts a group: the person turns each group
// on once.
test("after an upgrade a tool whose switch is on is refused until its group is turned on", async () => {
  for (const method of ["list_tabs", "list_frames", "read_page", "search_bookmarks", "open_tab"]) {
    const chrome = fakeChrome({ local: structuredClone(BEFORE_GROUPS), tabs });
    const reply = await call(chrome, method);
    assert.equal(
      reply.error?.message,
      `${method} is in the ${groupOf(method)} group, which is turned off in the BraveBot extension's options`,
    );
    assert.ok(!reached(chrome, "tabs.query"), method);
    assert.ok(!reached(chrome, "permissions.contains"), method);
  }
  const chrome = fakeChrome({ local: structuredClone(BEFORE_GROUPS), tabs });
  assert.equal((await call(chrome, "get_platform_info")).result.os, "mac");
});

test("turning a group on after an upgrade runs its tools, and the blocked sites still apply", async () => {
  const chrome = fakeChrome({ local: structuredClone(BEFORE_GROUPS), tabs });
  assert.equal(await turnOn(chrome, "tabs-read", NO_ACCESS), true);
  const reply = await call(chrome, "list_tabs");
  assert.deepEqual(
    reply.result.map((tab) => tab.url),
    ["https://news.example/"],
  );
  // Turning a group on turns on all its tools, so a switch the earlier version
  // had off is on. A tool of a group still off stays off by its group.
  assert.equal(await turnOn(chrome, "history-read", NO_ACCESS), true);
  assert.equal((await settings(chrome)).search_history, true);
  const open = await call(chrome, "open_tab");
  assert.match(open.error.message, /^open_tab is in the navigate group, which is turned off/);
});

// A tool switch is not a grant of its group: another group that holds the same
// permission, or a permission the browser still holds from before, does not run
// it.
test("a tool runs only while its group's switch is on, as well as its own switch and the permissions", async () => {
  const local = {
    [SETTINGS_KEY]: { list_tabs: true },
    [GROUP_SWITCHES_KEY]: { "tabs-manage": true },
  };
  const off = fakeChrome({ local: { ...local }, tabs });
  const refused = await call(off, "list_tabs");
  assert.match(refused.error.message, /^list_tabs is in the tabs-read group, which is turned off/);
  assert.ok(!reached(off, "tabs.query"));

  const on = fakeChrome({
    local: { ...local, [GROUP_SWITCHES_KEY]: { "tabs-read": true } },
    tabs,
  });
  assert.equal((await call(on, "list_tabs")).result.length, 2);
});

const manifest = JSON.parse(
  readFileSync(new URL("../manifest.json", import.meta.url), "utf8"),
);

// The first Chrome version in which the API behind each permission a group lists
// can be used from a Manifest V3 service worker, from the Chrome extensions
// reference. Chrome 88 is the first with Manifest V3, so an API older than that
// is listed as 88.
const FIRST_CHROME = {
  tabs: 88,
  webNavigation: 88,
  history: 88,
  bookmarks: 88,
  scripting: 88,
  tabGroups: 89,
  sessions: 96,
  topSites: 96,
  webRequest: 88,
  readingList: 120,
  // chrome.userScripts.execute, which run_script calls, is in Chrome 135. The API
  // is in Chrome 120 and runs a script only by registering it first.
  userScripts: 135,
  // The promise forms of chrome.cookies and chrome.search, which these tools
  // call, come with Manifest V3 in Chrome 88.
  cookies: 88,
  search: 88,
  // chrome.contextMenus.create, removeAll and onClicked, which the held group
  // calls with callbacks, are in Manifest V3 service workers from Chrome 88.
  contextMenus: 88,
  // The promise form these tools call, which the Chrome reference dates:
  // chrome.system.* from 91, ChromeSetting.get, ContentSetting.get and
  // chrome.fontSettings from 96, and chrome.idle.queryState from 116.
  // chrome.power is older than Manifest V3. The promise form of
  // chrome.browsingData.remove, chrome.history.deleteUrl and deleteRange is
  // from 96, as is that of chrome.downloads.search and removeFile.
  privacy: 96,
  contentSettings: 96,
  fontSettings: 96,
  "system.cpu": 91,
  "system.memory": 91,
  "system.display": 91,
  "system.storage": 91,
  idle: 116,
  power: 88,
  browsingData: 96,
  downloads: 96,
  // The promise form of chrome.pageCapture.saveAsMHTML, which the Chrome
  // reference dates.
  pageCapture: 116,
  // chooseDesktopMedia takes a callback and has no promise form. It is in
  // Manifest V3 extensions from Chrome 88, the first to run them.
  desktopCapture: 88,
};

// What the service worker uses besides the groups' APIs: chrome.storage.session
// holds who was turned on until Brave restarts, and the injected script names
// a document by its id.
const WORKER_NEEDS = { "storage.session": 102, "scripting documentIds": 106 };

// What the manifest asked for before groups, which the worker's own APIs need.
const BASE = 116;

test("the manifest asks for the newest Chrome the built groups need, and no newer", () => {
  const listed = new Set(
    Object.values(GROUPS).flatMap((group) => group.permissions),
  );
  for (const permission of listed) {
    assert.ok(
      Object.hasOwn(FIRST_CHROME, permission),
      `add the first Chrome version of ${permission} to FIRST_CHROME, and raise minimum_chrome_version if it is newer than the manifest's`,
    );
  }
  const wanted = Math.max(
    BASE,
    ...[...listed].map((permission) => FIRST_CHROME[permission]),
  );
  assert.equal(Number(manifest.minimum_chrome_version), wanted);
  for (const [api, version] of Object.entries(WORKER_NEEDS)) {
    assert.ok(version <= wanted, api);
  }
});
