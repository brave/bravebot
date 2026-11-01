// The warnings the options page says Brave will show, held to the wording Brave
// 1.98.62 gave for them, which docs/design/browser-extension.md quotes in
// "Checked against Brave".
//
// Run with `node --test extension/tests/`.

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import {
  ALL_URLS,
  GROUPS,
  HOST_PATTERNS,
  NO_ACCESS,
  PRESETS,
  SCOPE_ALL,
  needed,
  neededTogether,
} from "../groups.js";
import {
  BROWSER_WARNINGS,
  HISTORY_ALL_DEVICES,
  HOSTS_WARNING,
  sitesWarning,
  warningsFor,
} from "../warnings.js";

const MANIFEST = JSON.parse(
  readFileSync(new URL("../manifest.json", import.meta.url), "utf8"),
);
// With the line breaks of the paragraphs taken out, since a quotation wraps.
const DESIGN = readFileSync(
  new URL("../../docs/design/browser-extension.md", import.meta.url),
  "utf8",
).replace(/\s+/g, " ");

// What `chrome.management.getPermissionWarningsByManifest` returned for each
// optional permission asked for alone.
const BRAVE_1_98 = {
  tabs: "Read your browsing history",
  tabGroups: "View and manage your tab groups",
  webNavigation: "Read your browsing history",
  sessions: null,
  history: "Read and change your browsing history on all your signed-in devices",
  topSites: "Read a list of your most frequently visited websites",
  bookmarks: "Read and change your bookmarks",
  readingList: "Read and change entries in the reading list",
  scripting: null,
  search: null,
  cookies: null,
  contextMenus: null,
  webRequest: null,
  privacy: "Change your privacy-related settings",
  contentSettings:
    "Change and grant access to features such as geolocation, microphone, camera, cookies, etc., for all your websites and extensions, including this extension.",
  fontSettings: null,
  "system.cpu": null,
  "system.memory": null,
  "system.display": null,
  "system.storage": "Identify and eject storage devices",
  idle: null,
  power: null,
  browsingData: null,
  downloads: "Manage your downloads",
  pageCapture: "Read and change all your data on all websites",
  desktopCapture: "Capture content of your screen",
  userScripts: null,
};

test("each permission's warning is the wording Brave 1.98 gave for it", () => {
  assert.deepEqual(BROWSER_WARNINGS, BRAVE_1_98);
});

// A permission with no entry makes the page throw when it draws the group.
test("every optional permission the manifest offers has a warning, or none, written for it", () => {
  for (const permission of MANIFEST.optional_permissions) {
    assert.ok(
      Object.hasOwn(BROWSER_WARNINGS, permission),
      `${permission} has no entry`,
    );
  }
  for (const permission of Object.keys(BROWSER_WARNINGS)) {
    assert.ok(
      MANIFEST.optional_permissions.includes(permission),
      `${permission} is not an optional permission`,
    );
  }
  for (const [id, group] of Object.entries(GROUPS)) {
    assert.doesNotThrow(
      () => warningsFor({ permissions: group.permissions }),
      id,
    );
  }
  assert.throws(
    () => warningsFor({ permissions: ["debugger"] }),
    /no browser warning is written for debugger/,
  );
});

test("the design document quotes every warning the page shows", () => {
  const quoted = [
    ...Object.values(BROWSER_WARNINGS).filter((line) => line !== null),
    HISTORY_ALL_DEVICES,
    HOSTS_WARNING,
    sitesWarning(["https://example.com"]),
    sitesWarning(["https://a.example", "https://b.example"]),
    sitesWarning(["https://a.example", "https://b.example", "https://c.example"]),
    sitesWarning([
      "https://a.example",
      "https://b.example",
      "https://c.example",
      "https://d.example",
    ]),
  ];
  for (const line of quoted) {
    assert.ok(DESIGN.includes(line), `the design document lacks: ${line}`);
  }
});

test("a host line is Brave's: the hosts sorted, once each, without a scheme or port, and up to three", () => {
  const prefix = "Read and change your data on";
  assert.equal(sitesWarning(["https://example.com"]), `${prefix} example.com`);
  assert.equal(
    sitesWarning(["https://b.example", "https://a.example"]),
    `${prefix} a.example and b.example`,
  );
  assert.equal(
    sitesWarning(["https://c.example", "https://a.example", "https://b.example"]),
    `${prefix} a.example, b.example, and c.example`,
  );
  assert.equal(
    sitesWarning([
      "https://a.example",
      "https://b.example",
      "https://c.example",
      "https://d.example",
    ]),
    `${prefix} a number of websites`,
  );
  // The scheme and the port are not part of what Brave names.
  assert.equal(
    sitesWarning(["https://a.example", "http://a.example"]),
    `${prefix} a.example`,
  );
  assert.equal(
    sitesWarning(["http://127.0.0.1:1", "http://127.0.0.1:2"]),
    `${prefix} 127.0.0.1`,
  );
  assert.equal(sitesWarning(["http://[::1]:8080"]), `${prefix} [::1]`);
  // Hosts are counted, not patterns: three hosts on two schemes are three.
  assert.equal(
    sitesWarning([
      "https://c.example",
      "http://c.example",
      "https://a.example",
      "http://a.example",
      "https://b.example",
      "http://b.example",
    ]),
    `${prefix} a.example, b.example, and c.example`,
  );
  assert.equal(
    sitesWarning(["https://z.example.org", "https://a.example.com", "https://m.example.net"]),
    `${prefix} a.example.com, m.example.net, and z.example.org`,
  );
});

test("all sites, whether one pattern or both, is the warning for all websites, and a site is named", () => {
  assert.equal(HOSTS_WARNING, "Read and change all your data on all websites");
  assert.deepEqual(warningsFor({ origins: HOST_PATTERNS }), [HOSTS_WARNING]);
  assert.deepEqual(warningsFor({ origins: [HOST_PATTERNS[0]] }), [HOSTS_WARNING]);
  assert.deepEqual(warningsFor({ origins: ["https://example.com/*"] }), [
    "Read and change your data on example.com",
  ]);
  // A warning is said once, and a permission that shows none adds no line.
  assert.deepEqual(
    warningsFor({ permissions: ["tabs", "webNavigation", "scripting", "cookies"] }),
    ["Read your browsing history"],
  );
  assert.deepEqual(warningsFor({ permissions: ["cookies"], origins: HOST_PATTERNS }), [
    HOSTS_WARNING,
  ]);
});

// What Brave 1.98.62 said through the same call, in a run on 2026-10-30, when the
// permissions of a group were asked for together. Brave orders the lines its own
// way, so lines are compared as sets.
const READS = "Read your browsing history";
const HISTORY = "Read and change your browsing history on all your signed-in devices";
const TOP_SITES = "Read a list of your most frequently visited websites";
const TAB_GROUPS = "View and manage your tab groups";
const BOOKMARKS = "Read and change your bookmarks";
const READING_LIST = "Read and change entries in the reading list";
const DOWNLOADS = "Manage your downloads";
const PRIVACY = "Change your privacy-related settings";
const CONTENT = BROWSER_WARNINGS.contentSettings;
const EJECT = "Identify and eject storage devices";
const EXAMPLE = "Read and change your data on example.com";
const SCREEN = "Capture content of your screen";

const BRAVE_GROUPS = {
  platform: [],
  "tabs-read": [HISTORY_ALL_DEVICES, TAB_GROUPS],
  "history-read": [HISTORY],
  "bookmarks-read": [BOOKMARKS, READING_LIST],
  "page-read": [READS],
  "tabs-manage": [READS, TAB_GROUPS],
  "bookmarks-write": [BOOKMARKS, READING_LIST],
  navigate: [HISTORY_ALL_DEVICES],
  // pageCapture, which downloads lists for save_page_archive, says the line for
  // every site, and tabs, which it lists to find the tab, adds none beside it.
  downloads: [HOSTS_WARNING, DOWNLOADS],
  cookies: [],
  "network-watch": [],
  "browser-info": [EJECT, CONTENT, PRIVACY],
  "remove-data": [HISTORY, BOOKMARKS, READING_LIST, DOWNLOADS],
  scripts: [],
  input: [READS],
  "input-trusted": [],
  "capture-trusted": [],
  "console-trusted": [],
  "upload-trusted": [],
  screen: [READS, SCREEN],
  held: [],
};

const sorted = (lines) => [...lines].sort();

// Brave says one line for permissions asked for together, two for tabs-read.
test("the lines for each group are the ones Brave said for the group's permissions asked for together", () => {
  assert.deepEqual(Object.keys(BRAVE_GROUPS), Object.keys(GROUPS));
  for (const id of Object.keys(GROUPS)) {
    const lines = warningsFor(needed(id, GROUPS, NO_ACCESS));
    assert.deepEqual(sorted(lines), sorted(BRAVE_GROUPS[id]), id);
    assert.equal(new Set(lines).size, lines.length, `${id} says a line twice`);
  }
});

test("the lines for a group that reads pages follow Brave's for its permissions and the sites", () => {
  const everySite = { allowed: [], scopes: { "page-read": SCOPE_ALL, cookies: SCOPE_ALL, "network-watch": SCOPE_ALL } };
  const oneSite = { allowed: ["https://example.com"], scopes: {} };
  const lines = (id, access) => sorted(warningsFor(needed(id, GROUPS, access)));
  // Access to every site says the pages may be read and changed, and Brave then
  // leaves out the line that says the history is read.
  assert.deepEqual(lines("page-read", everySite), [HOSTS_WARNING]);
  assert.deepEqual(lines("cookies", everySite), [HOSTS_WARNING]);
  assert.deepEqual(lines("network-watch", everySite), [HOSTS_WARNING]);
  assert.deepEqual(lines("page-read", oneSite), sorted([EXAMPLE, READS]));
  assert.deepEqual(lines("cookies", oneSite), [EXAMPLE]);
  // Read through the same call in Brave 1.98.62 (2026-10-31): the screen group
  // with one site, and with <all_urls> in place of the history line.
  const screenEverySite = { allowed: [], scopes: { screen: SCOPE_ALL } };
  assert.deepEqual(lines("screen", screenEverySite), sorted([HOSTS_WARNING, SCREEN]));
  assert.deepEqual(lines("screen", oneSite), sorted([EXAMPLE, READS, SCREEN]));
});

// The group that a second extension serves lists no permission, so Brave is asked
// for nothing and says nothing for it, with or without sites allowed. The second
// extension's own install prompt was shown when it was installed.
test("the input-trusted and capture-trusted groups show no line from Brave, with sites allowed or none", () => {
  const oneSite = { allowed: ["https://example.com"], scopes: {} };
  for (const access of [NO_ACCESS, oneSite]) {
    for (const id of ["input-trusted", "capture-trusted"]) {
      assert.deepEqual(warningsFor(needed(id, GROUPS, access)), [], id);
    }
  }
});

test("the console-trusted group shows no line from Brave, with sites allowed or none", () => {
  const oneSite = { allowed: ["https://example.com"], scopes: {} };
  for (const access of [NO_ACCESS, oneSite]) {
    assert.deepEqual(warningsFor(needed("console-trusted", GROUPS, access)), []);
  }
});

test("the upload-trusted group shows no line from Brave, with sites allowed or none", () => {
  const oneSite = { allowed: ["https://example.com"], scopes: {} };
  for (const access of [NO_ACCESS, oneSite]) {
    assert.deepEqual(warningsFor(needed("upload-trusted", GROUPS, access)), []);
  }
});

test("the lines for each preset are the ones Brave said for the union of its groups' permissions and hosts", () => {
  const nothing = NO_ACCESS;
  const everySite = { allowed: [], scopes: { "page-read": SCOPE_ALL } };
  const oneSite = { allowed: ["https://example.com"], scopes: {} };
  const rest = sorted([HISTORY, BOOKMARKS, READING_LIST, TAB_GROUPS]);
  // None of them holds downloads, so none says the line for downloads or the
  // one for every site that pageCapture would add.
  for (const name of ["look", "read", "browse"]) {
    const ids = PRESETS[name];
    assert.deepEqual(sorted(warningsFor(neededTogether(ids, GROUPS, nothing))), rest, name);
    assert.deepEqual(sorted(warningsFor(neededTogether(ids, GROUPS, oneSite))), name === "look" ? rest : sorted([EXAMPLE, ...rest]), name);
    assert.deepEqual(
      sorted(warningsFor(neededTogether(ids, GROUPS, everySite))),
      name === "look" ? rest : sorted([HOSTS_WARNING, ...rest]),
      name,
    );
  }
});

// Every combination of the permissions that are about the browsing history,
// asked for without hosts.
test("the history lines of tabs, webNavigation, sessions, history and topSites follow Brave's for every combination", () => {
  const BRAVE_WITHOUT_HISTORY = {
    "tabs": READS,
    "webNavigation": READS,
    "tabs+webNavigation": READS,
    "sessions": null,
    "tabs+sessions": HISTORY_ALL_DEVICES,
    "webNavigation+sessions": READS,
    "tabs+webNavigation+sessions": HISTORY_ALL_DEVICES,
    "topSites": TOP_SITES,
    "tabs+topSites": READS,
    "webNavigation+topSites": READS,
    "tabs+webNavigation+topSites": READS,
    "sessions+topSites": TOP_SITES,
    "tabs+sessions+topSites": HISTORY_ALL_DEVICES,
    "webNavigation+sessions+topSites": READS,
    "tabs+webNavigation+sessions+topSites": HISTORY_ALL_DEVICES,
  };
  const others = ["tabs", "webNavigation", "sessions", "topSites"];
  let seen = 0;
  for (let mask = 1; mask < 2 ** others.length; mask += 1) {
    const asked = others.filter((_, index) => mask & (1 << index));
    const expected = BRAVE_WITHOUT_HISTORY[asked.join("+")];
    assert.deepEqual(warningsFor({ permissions: asked }), expected === null ? [] : [expected], asked.join("+"));
    // With `history` Brave says its line and none of the others.
    assert.deepEqual(warningsFor({ permissions: ["history", ...asked] }), [HISTORY], `history+${asked.join("+")}`);
    seen += 1;
  }
  assert.equal(seen, Object.keys(BRAVE_WITHOUT_HISTORY).length);
  assert.deepEqual(warningsFor({ permissions: ["history"] }), [HISTORY]);
});

test("with access to every site the history lines that are not about changing it are left out", () => {
  const query = (permissions) => ({ permissions, origins: HOST_PATTERNS });
  for (const permissions of [["tabs"], ["webNavigation"], ["topSites"], ["sessions"], ["tabs", "sessions", "topSites"]]) {
    assert.deepEqual(warningsFor(query(permissions)), [HOSTS_WARNING], permissions.join("+"));
  }
  assert.deepEqual(sorted(warningsFor(query(["history", "tabs"]))), sorted([HOSTS_WARNING, HISTORY]));
  assert.deepEqual(sorted(warningsFor(query(["bookmarks", "tabs"]))), sorted([HOSTS_WARNING, BOOKMARKS]));
  // A few sites keep the line.
  assert.deepEqual(sorted(warningsFor({ permissions: ["tabs"], origins: ["https://example.com/*"] })), sorted([EXAMPLE, READS]));
});

// <all_urls> is the host a screenshot needs. Brave 1.98.62 gave it the same line
// as the two patterns (docs/design/browser-extension.md, "The warning for a
// broad host pattern"), alone and beside every combination of the permissions
// about the browsing history, so the page shows one line for either.
test("<all_urls> warns as the two patterns of all sites do", () => {
  assert.deepEqual(warningsFor({ origins: [ALL_URLS] }), [HOSTS_WARNING]);
  const family = ["tabs", "webNavigation", "sessions", "history", "topSites"];
  for (let mask = 0; mask < 1 << family.length; mask += 1) {
    const permissions = family.filter((_, bit) => (mask >> bit) & 1);
    assert.deepEqual(
      warningsFor({ permissions, origins: [ALL_URLS] }),
      warningsFor({ permissions, origins: HOST_PATTERNS }),
      permissions.join(","),
    );
  }
  assert.deepEqual(
    warningsFor(needed("page-read", GROUPS, { allowed: [], scopes: { "page-read": SCOPE_ALL } })),
    ["Read and change all your data on all websites"],
  );
});

// pageCapture alone, and with hosts or the other permissions of page-read, as
// Brave 1.98.62 said them through the same call in a run on 2026-10-31. Brave
// says the line for every site for pageCapture whatever hosts come with it, and
// merges the lines it subsumes as it does for access to every site.
test("pageCapture warns as access to every site does, whatever hosts come with it", () => {
  assert.deepEqual(warningsFor({ permissions: ["pageCapture"] }), [HOSTS_WARNING]);
  assert.deepEqual(
    warningsFor({ permissions: ["pageCapture"], origins: ["https://a.example/*"] }),
    [HOSTS_WARNING],
  );
  assert.deepEqual(
    warningsFor({ permissions: ["pageCapture"], origins: HOST_PATTERNS }),
    [HOSTS_WARNING],
  );
  // The lines about the history that every site subsumes are left out, and
  // `history` and `downloads` keep theirs.
  for (const permissions of [["tabs"], ["webNavigation"], ["sessions"], ["topSites"], ["tabs", "sessions"]]) {
    assert.deepEqual(
      warningsFor({ permissions: [...permissions, "pageCapture"] }),
      [HOSTS_WARNING],
      permissions.join("+"),
    );
  }
  assert.deepEqual(
    sorted(warningsFor({ permissions: ["tabs", "history", "pageCapture"] })),
    sorted([HOSTS_WARNING, HISTORY]),
  );
  assert.deepEqual(
    sorted(warningsFor({ permissions: ["tabs", "scripting", "webNavigation", "pageCapture", "downloads"], origins: ["https://a.example/*"] })),
    sorted([HOSTS_WARNING, DOWNLOADS]),
  );
  // The permissions of the downloads group, with and without tabs, gave the same
  // two lines in Brave 1.98.62 (2026-10-31), and tabs with downloads alone gave
  // the line about the history.
  for (const permissions of [["downloads", "pageCapture"], ["downloads", "pageCapture", "tabs"]]) {
    assert.deepEqual(sorted(warningsFor({ permissions })), sorted([HOSTS_WARNING, DOWNLOADS]), permissions.join("+"));
  }
  assert.deepEqual(
    sorted(warningsFor({ permissions: ["downloads", "tabs"] })),
    sorted([READS, DOWNLOADS]),
  );
});
