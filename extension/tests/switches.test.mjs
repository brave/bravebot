// Turning a group on and off against a fake browser that stores what is saved
// and holds the permissions the person has granted. The options page and the
// service worker call these; their own tests cover what they draw and when
// they call.
//
// Run with `node --test extension/tests/`.

import assert from "node:assert/strict";
import { test } from "node:test";
import { AWAKE_ALARM, AWAKE_KEY } from "../awake.js";
import { HELD_KEY } from "../held.js";
import { GROUPS, NO_ACCESS, PRESETS, SCOPE_ALL } from "../groups.js";
import {
  HOUR_MS,
  LIFETIMES_KEY,
  ONE_HOUR,
  lifetimeEnded,
  recordOf,
} from "../lifetimes.js";
import {
  GROUP_SWITCHES_KEY,
  allowSites,
  applyPreset,
  groupSwitches,
  groupsOn,
  isOn,
  presetGroups,
  setScope,
  setTool,
  turnEverythingOff,
  turnOff,
  turnOffLapsed,
  turnOffUnheld,
  turnOn,
} from "../switches.js";
import {
  ALLOWED_SITES_KEY,
  SETTINGS_KEY,
  SITE_SCOPES_KEY,
  settings,
} from "../tools.js";
import { withMenus } from "./fake-menus.mjs";
import { allOptional, fakePermissions } from "./fake-permissions.mjs";
import { fakeArea } from "./fake-storage.mjs";

// Every group that reads pages on all sites: the scope a test chooses where it
// is about the two host patterns and not about the allowed sites.
const EVERY_SITE = {
  allowed: [],
  scopes: Object.fromEntries(
    Object.entries(GROUPS)
      .filter(([, group]) => group.hosts)
      .map(([id]) => [id, SCOPE_ALL]),
  ),
};

// What a browser that holds every optional permission still holds once the
// groups of the presets are turned off: the permissions of the groups in no
// preset that these tests do not turn on, in the order of the manifest.
const IN_NO_PRESET = [
  "webRequest",
  "privacy",
  "contentSettings",
  "fontSettings",
  "system.cpu",
  "system.memory",
  "system.display",
  "system.storage",
  "idle",
  "power",
  "browsingData",
  "downloads",
  "pageCapture",
  "cookies",
  "contextMenus",
  "desktopCapture",
  "userScripts",
];

// A fake `chrome` with storage and permissions. `stored` is the object the
// storage keeps, `session` the object session storage keeps, and `calls`
// records the permission calls.
function browser({ stored = {}, session = {}, held = {}, decision = true } = {}) {
  const calls = [];
  const permissions = fakePermissions({ held, calls, decision });
  const chrome = {
    permissions,
    storage: { local: fakeArea(stored), session: fakeArea(session) },
  };
  return { chrome, stored, session, calls, permissions };
}

const removals = (calls) =>
  calls.filter(([name]) => name === "permissions.remove").map(([, query]) => query);

// A stored value that is not an object of true switches is read as every group
// off, so a list that cannot be read never turns a group on.
test("a stored group list that is not true switches in an object leaves the group off", async () => {
  for (const stored of [
    undefined,
    null,
    "tabs-read",
    ["tabs-read"],
    { "tabs-read": "true" },
    { "tabs-read": 1 },
    { "tabs-read": false },
    { platform: true },
    { "no-such-group": true },
  ]) {
    const { chrome } = browser({ stored: { [GROUP_SWITCHES_KEY]: stored } });
    assert.deepEqual(await groupSwitches(chrome), {}, JSON.stringify(stored));
  }
  const { chrome } = browser({
    stored: { [GROUP_SWITCHES_KEY]: { "tabs-read": true, "history-read": false } },
  });
  assert.deepEqual(await groupSwitches(chrome), { "tabs-read": true });
});

// The browser takes the request only from a click, so the request is the first
// call. Turning on the group's tools with the group is what lets a person turn
// one group on and have its tools work, and a tool of another group is left as
// it was.
test("turning a group on requests its permissions first and then turns on its own tools", async () => {
  const { chrome, stored, calls } = browser({
    stored: { [SETTINGS_KEY]: { open_tab: true } },
  });
  assert.equal(await turnOn(chrome, "history-read"), true);
  assert.deepEqual(calls[0], [
    "permissions.request",
    { permissions: ["history", "topSites", "sessions"] },
  ]);
  assert.deepEqual(stored[GROUP_SWITCHES_KEY], { "history-read": true });
  const tools = await settings(chrome);
  assert.equal(tools.search_history, true);
  assert.equal(tools.open_tab, true, "a tool of another group was changed");
  assert.equal(tools.search_bookmarks, false);
  assert.equal(tools.list_tabs, false);
});

test("a group the browser did not grant, or refused to ask for, is not turned on", async () => {
  const declined = browser({ decision: false });
  assert.equal(await turnOn(declined.chrome, "tabs-read"), false);
  assert.deepEqual(declined.stored, {});

  const policy = new Error("Permissions are blocked by enterprise policy.");
  const blocked = browser({ decision: policy });
  await assert.rejects(turnOn(blocked.chrome, "tabs-read"), policy);
  assert.deepEqual(blocked.stored, {});
});

// An empty request is not a question the browser answers, so a group that
// needs nothing is turned on without one. `input-trusted` lists no permission and
// reads no host, because the second extension does its work.
test("a group that needs nothing is turned on without asking the browser", async () => {
  const { chrome, stored, calls } = browser();
  assert.equal(await turnOn(chrome, "input-trusted"), true);
  assert.deepEqual(calls, []);
  assert.deepEqual(stored[GROUP_SWITCHES_KEY], { "input-trusted": true });
});

// The browser grants by permission, so a permission two groups list stays while
// either is on. Each case turns one group off with the others as they are.
test("turning a group off takes back only what no group that is on still needs", async () => {
  const cases = [
    {
      why: "alone, everything it needs",
      on: ["tabs-read"],
      off: "tabs-read",
      removed: [
        { permissions: ["tabs", "tabGroups", "webNavigation", "sessions"] },
      ],
    },
    {
      why: "page-read keeps what tabs-read needs",
      on: ["tabs-read", "page-read"],
      off: "page-read",
      removed: [
        {
          permissions: ["scripting"],
          origins: ["<all_urls>"],
        },
      ],
    },
    {
      why: "page-read keeps tabs and webNavigation, so tabGroups and sessions go",
      on: ["tabs-read", "page-read"],
      off: "tabs-read",
      removed: [{ permissions: ["tabGroups", "sessions"] }],
    },
    {
      why: "history-read keeps the sessions tabs-read shares with it",
      on: ["tabs-read", "history-read"],
      off: "tabs-read",
      removed: [{ permissions: ["tabs", "tabGroups", "webNavigation"] }],
    },
    {
      why: "tabs-read keeps the sessions history-read shares with it",
      on: ["tabs-read", "history-read"],
      off: "history-read",
      removed: [{ permissions: ["history", "topSites"] }],
    },
    {
      why: "navigate shares tabs, sessions and webNavigation with tabs-read, so only search goes",
      on: ["tabs-read", "navigate"],
      off: "navigate",
      removed: [{ permissions: ["search"] }],
    },
    {
      why: "cookies alone takes back its permission and the two host patterns",
      on: ["cookies"],
      off: "cookies",
      removed: [
        { permissions: ["cookies"], origins: ["https://*/*", "http://*/*"] },
      ],
    },
    {
      why: "network-watch keeps the two host patterns that cookies shares with it",
      on: ["cookies", "network-watch"],
      off: "cookies",
      removed: [{ permissions: ["cookies"] }],
    },
    {
      why: "page-read lists <all_urls> and not the two patterns, so cookies takes them back",
      on: ["cookies", "page-read"],
      off: "cookies",
      removed: [
        { permissions: ["cookies"], origins: ["https://*/*", "http://*/*"] },
      ],
    },
    {
      why: "a group that is off keeps nothing, and hosts go with the permissions",
      on: ["page-read"],
      off: "page-read",
      removed: [
        {
          permissions: PAGES,
          origins: ["<all_urls>"],
        },
      ],
    },
  ];
  for (const { why, on, off, removed } of cases) {
    const { chrome, stored, calls, permissions } = browser({
      held: allOptional(),
      stored: {
        [GROUP_SWITCHES_KEY]: Object.fromEntries(on.map((id) => [id, true])),
        [SITE_SCOPES_KEY]: EVERY_SITE.scopes,
      },
    });
    await turnOff(chrome, off);
    assert.deepEqual(removals(calls), removed, why);
    assert.deepEqual(
      Object.keys(stored[GROUP_SWITCHES_KEY]).sort(),
      on.filter((id) => id !== off).sort(),
      why,
    );
    for (const permission of removed.flatMap((query) => query.permissions ?? [])) {
      assert.equal(permissions.held.permissions.has(permission), false, why);
    }
    for (const kept of ["bookmarks"]) {
      assert.equal(permissions.held.permissions.has(kept), true, why);
    }
  }
});

test("turning a group off turns off its tools and no others", async () => {
  const { chrome } = browser({
    held: allOptional(),
    stored: {
      [GROUP_SWITCHES_KEY]: { "tabs-read": true, "history-read": true },
      [SETTINGS_KEY]: { list_tabs: true, list_frames: true, search_history: true },
    },
  });
  await turnOff(chrome, "tabs-read");
  const tools = await settings(chrome);
  assert.equal(tools.list_tabs, false);
  assert.equal(tools.list_frames, false);
  assert.equal(tools.search_history, true);
  assert.equal(tools.get_platform_info, true);
});

// bookmarks-read and bookmarks-write list the same two permissions, since the
// browser grants bookmarks and readingList for reading and for changing alike.
// Whichever is turned off first keeps them for the other, and the last to go
// takes both back.
test("bookmarks-read and bookmarks-write keep the two permissions they list for each other until both are off", async () => {
  for (const [first, second, left] of [
    ["bookmarks-write", "bookmarks-read", "search_bookmarks"],
    ["bookmarks-read", "bookmarks-write", "create_bookmark"],
  ]) {
    const { chrome, calls, permissions } = browser({
      held: allOptional(),
      stored: {
        [GROUP_SWITCHES_KEY]: { "bookmarks-read": true, "bookmarks-write": true },
        [SETTINGS_KEY]: { search_bookmarks: true, create_bookmark: true },
      },
    });
    await turnOff(chrome, first);
    assert.deepEqual(removals(calls), [], `${first} first`);
    assert.equal(permissions.held.permissions.has("bookmarks"), true, first);
    assert.equal(permissions.held.permissions.has("readingList"), true, first);
    const tools = await settings(chrome);
    assert.equal(tools[left], true, `${first} first left ${left} on`);
    assert.equal(
      tools[left === "create_bookmark" ? "search_bookmarks" : "create_bookmark"],
      false,
      first,
    );

    await turnOff(chrome, second);
    assert.deepEqual(
      removals(calls),
      [{ permissions: ["bookmarks", "readingList"] }],
      `${second} last`,
    );
    assert.equal(permissions.held.permissions.has("bookmarks"), false, second);
    assert.equal(permissions.held.permissions.has("readingList"), false, second);
  }
});

// page-read lists tabs to find a tab by its address and downloads lists it for
// the same reason, so whichever is turned off first keeps it for the other, and
// the last to go takes it back. The permissions only one of them lists go with
// that group.
test("page-read and downloads keep tabs for each other until both are off", async () => {
  for (const [first, second, firstTaken, secondTaken, left, gone] of [
    [
      "page-read",
      "downloads",
      { permissions: ["scripting", "webNavigation"], origins: ["<all_urls>"] },
      { permissions: ["downloads", "pageCapture", "tabs"] },
      "save_page_archive",
      "read_page",
    ],
    [
      "downloads",
      "page-read",
      { permissions: ["downloads", "pageCapture"] },
      { permissions: ["tabs", "scripting", "webNavigation"], origins: ["<all_urls>"] },
      "read_page",
      "save_page_archive",
    ],
  ]) {
    const { chrome, calls, permissions } = browser({
      held: allOptional(),
      stored: {
        [GROUP_SWITCHES_KEY]: { "page-read": true, downloads: true },
        [SITE_SCOPES_KEY]: EVERY_SITE.scopes,
        [SETTINGS_KEY]: { read_page: true, save_page_archive: true },
      },
    });
    await turnOff(chrome, first);
    assert.deepEqual(removals(calls), [firstTaken], `${first} first`);
    assert.equal(permissions.held.permissions.has("tabs"), true, `${first} first`);
    const tools = await settings(chrome);
    assert.equal(tools[left], true, `${first} first left ${left} on`);
    assert.equal(tools[gone], false, `${first} first turned ${gone} off`);

    await turnOff(chrome, second);
    assert.deepEqual(removals(calls), [firstTaken, secondTaken], `${second} last`);
    assert.equal(permissions.held.permissions.has("tabs"), false, `${second} last`);
  }
});

// If the browser cannot take a permission back, the tools must already be off.
test("the switches are saved off before the permissions are taken back", async () => {
  const { chrome, stored, permissions } = browser({
    held: allOptional(),
    stored: {
      [GROUP_SWITCHES_KEY]: { "history-read": true },
      [SETTINGS_KEY]: { search_history: true },
    },
  });
  let seen;
  const failure = new Error("the browser would not remove it");
  permissions.remove = async () => {
    seen = structuredClone(stored);
    throw failure;
  };
  await assert.rejects(turnOff(chrome, "history-read"), failure);
  assert.deepEqual(seen[GROUP_SWITCHES_KEY], {});
  assert.equal(seen[SETTINGS_KEY].search_history, false);
  assert.deepEqual(stored[GROUP_SWITCHES_KEY], {});
});

test("one tool is switched alone", async () => {
  const { chrome, stored } = browser({
    stored: { [SETTINGS_KEY]: { list_tabs: true, open_tab: true } },
  });
  await setTool(chrome, "open_tab", false);
  assert.equal(stored[SETTINGS_KEY].open_tab, false);
  assert.equal(stored[SETTINGS_KEY].list_tabs, true);
  assert.equal(stored[SETTINGS_KEY].get_platform_info, true);
});

// Taking back `tabs` ends every group that lists it. A group that does not list
// it stays on, and what only the ended groups used is given back. Granting
// `tabs` again for another reason must not bring the ended groups' tools back.
test("a group whose permission was taken back is turned off with its tools, and no other", async () => {
  const { chrome, stored, calls, permissions } = browser({
    held: { ...allOptional(), origins: ["<all_urls>"] },
    stored: {
      [GROUP_SWITCHES_KEY]: {
        "tabs-read": true,
        "page-read": true,
        "history-read": true,
      },
      [SITE_SCOPES_KEY]: EVERY_SITE.scopes,
      [SETTINGS_KEY]: {
        list_tabs: true,
        list_frames: true,
        read_page: true,
        search_history: true,
      },
    },
  });
  await permissions.revoke({ permissions: ["tabs"] });
  await turnOffUnheld(chrome);

  assert.deepEqual(stored[GROUP_SWITCHES_KEY], { "history-read": true });
  const tools = await settings(chrome);
  assert.deepEqual(
    [tools.list_tabs, tools.list_frames, tools.read_page, tools.search_history],
    [false, false, false, true],
  );
  assert.deepEqual(
    [...permissions.held.permissions].sort(),
    [
      "bookmarks",
      "history",
      "readingList",
      "search",
      "sessions",
      "topSites",
      ...IN_NO_PRESET,
    ].sort(),
  );
  assert.deepEqual([...permissions.held.origins], []);
  assert.deepEqual(removals(calls), [
    { permissions: ["tabGroups"] },
    {
      permissions: PAGES,
      origins: ["<all_urls>"],
    },
  ]);

  await permissions.request({ permissions: ["tabs", "webNavigation"] });
  await turnOffUnheld(chrome);
  const again = await settings(chrome);
  assert.equal(again.list_tabs, false);
  assert.deepEqual(stored[GROUP_SWITCHES_KEY], { "history-read": true });
});

test("groups whose permissions are all held are left on", async () => {
  const { chrome, stored, calls } = browser({
    held: allOptional(),
    stored: {
      [GROUP_SWITCHES_KEY]: { "tabs-read": true, "page-read": true },
      [SETTINGS_KEY]: { list_tabs: true, read_page: true },
    },
  });
  await turnOffUnheld(chrome);
  assert.deepEqual(stored[GROUP_SWITCHES_KEY], {
    "tabs-read": true,
    "page-read": true,
  });
  assert.deepEqual(removals(calls), []);
  assert.equal((await settings(chrome)).read_page, true);
});

// A check that fails says nothing about what is held, and turning a group off
// takes permissions back, so a failing check changes nothing.
test("a permission check that fails turns nothing off", async () => {
  const { chrome, stored, permissions, calls } = browser({
    held: allOptional(),
    stored: { [GROUP_SWITCHES_KEY]: { "tabs-read": true } },
  });
  const failure = new Error("the browser did not answer");
  permissions.contains = async () => {
    throw failure;
  };
  await assert.rejects(turnOffUnheld(chrome), failure);
  assert.deepEqual(stored[GROUP_SWITCHES_KEY], { "tabs-read": true });
  assert.deepEqual(removals(calls), []);
});

const requests = (calls) =>
  calls.filter(([name]) => name === "permissions.request").map(([, query]) => query);

// The tools of each group a preset can turn on, to read which tool switches a
// preset left on.
const TOOLS_OF = {
  "tabs-read": [
    "list_tabs",
    "list_frames",
    "list_windows",
    "list_tab_groups",
    "list_recently_closed",
  ],
  "history-read": [
    "search_history",
    "get_visits",
    "list_top_sites",
    "list_synced_tabs",
  ],
  "bookmarks-read": [
    "search_bookmarks",
    "list_bookmark_folder",
    "list_recent_bookmarks",
    "list_reading_list",
  ],
  "page-read": ["read_page", "read_page_links", "read_page_forms"],
  "tabs-manage": [
    "focus_tab",
    "pin_tab",
    "mute_tab",
    "move_tab",
    "group_tabs",
    "ungroup_tabs",
    "update_tab_group",
    "discard_tab",
    "duplicate_tab",
    "set_zoom",
    "focus_window",
    "resize_window",
  ],
  "bookmarks-write": [
    "create_bookmark",
    "create_bookmark_folder",
    "move_bookmark",
    "rename_bookmark",
    "add_reading_list_entry",
    "update_reading_list_entry",
  ],
  navigate: [
    "open_tab",
    "navigate_tab",
    "reload_tab",
    "go_back",
    "go_forward",
    "open_window",
    "restore_closed",
    "search_web",
    "wait_for_load",
  ],
};

// One prompt for the whole set, as the first thing the click does. A request
// per group would be several prompts, and a read of storage first would spend
// the time the browser allows after a click.
test("a preset asks the browser once for everything its groups need, and then turns on those groups and their tools", async () => {
  const look = [
    "tabs",
    "tabGroups",
    "webNavigation",
    "sessions",
    "history",
    "topSites",
    "bookmarks",
    "readingList",
  ];
  // A group that reads pages is on the allowed sites in a preset, even where
  // the page drew it on all sites.
  const wanted = {
    look: { permissions: look },
    read: { permissions: [...look, ...PAGE_ONLY], origins: [`${NEWS}/*`] },
    // Browse asks for `search` as well, which only navigate lists.
    browse: {
      permissions: [...look, ...PAGE_ONLY, "search"],
      origins: [`${NEWS}/*`],
    },
  };
  for (const [name, query] of Object.entries(wanted)) {
    const { chrome, stored, calls, permissions } = browser();
    const access = { allowed: [NEWS], scopes: EVERY_SITE.scopes };
    assert.equal(await applyPreset(chrome, name, access), true, name);
    assert.deepEqual(calls[0], ["permissions.request", query], name);
    assert.equal(requests(calls).length, 1, name);
    assert.deepEqual(removals(calls), [], name);
    assert.deepEqual(
      Object.keys(stored[GROUP_SWITCHES_KEY]),
      PRESETS[name],
      name,
    );
    const tools = await settings(chrome);
    for (const [id, names] of Object.entries(TOOLS_OF)) {
      for (const tool of names) {
        assert.equal(tools[tool], PRESETS[name].includes(id), `${name}: ${tool}`);
      }
    }
    assert.equal(tools.get_platform_info, true, name);
    assert.deepEqual([...permissions.held.permissions].sort(), [...query.permissions].sort());
    assert.deepEqual([...permissions.held.origins], query.origins ?? [], name);
  }
});

test("a preset the browser did not grant, or refused to ask for, changes nothing", async () => {
  const before = {
    [GROUP_SWITCHES_KEY]: { "history-read": true },
    [SETTINGS_KEY]: { search_history: true },
  };
  const declined = browser({ decision: false, stored: structuredClone(before) });
  assert.equal(await applyPreset(declined.chrome, "browse"), false);
  assert.deepEqual(declined.stored, before);
  assert.deepEqual(removals(declined.calls), []);

  const policy = new Error("Permissions are blocked by enterprise policy.");
  const blocked = browser({ decision: policy, stored: structuredClone(before) });
  await assert.rejects(applyPreset(blocked.chrome, "browse"), policy);
  assert.deepEqual(blocked.stored, before);
});

// Look after Browse leaves Look: what Browse added goes, with its tools, but
// tabs and webNavigation stay since tabs-read still lists them.
test("a preset turns off the groups it does not name, and takes back only what no group left on needs", async () => {
  const { chrome, stored, calls, permissions } = browser({
    held: allOptional(),
    stored: {
      [GROUP_SWITCHES_KEY]: Object.fromEntries(PRESETS.browse.map((id) => [id, true])),
      [SETTINGS_KEY]: {
        list_tabs: true,
        read_page: true,
        open_tab: true,
        search_history: true,
        create_bookmark: true,
      },
    },
  });
  assert.equal(await applyPreset(chrome, "look", EVERY_SITE), true);
  assert.deepEqual(Object.keys(stored[GROUP_SWITCHES_KEY]), PRESETS.look);
  const tools = await settings(chrome);
  assert.deepEqual(
    [
      tools.list_tabs,
      tools.search_history,
      tools.read_page,
      tools.open_tab,
      tools.create_bookmark,
    ],
    [true, true, false, false, false],
  );
  assert.deepEqual(removals(calls), [
    {
      permissions: ["scripting", "search"],
      origins: ["<all_urls>"],
    },
  ]);
  assert.deepEqual(
    [...permissions.held.permissions].sort(),
    [
      "bookmarks",
      "history",
      "readingList",
      "sessions",
      "tabGroups",
      "tabs",
      "topSites",
      "webNavigation",
      ...IN_NO_PRESET,
    ].sort(),
  );
});

test("Off turns off every group, takes every optional permission and host back and asks the browser for nothing", async () => {
  const { chrome, stored, calls, permissions } = browser({
    held: allOptional(),
    stored: {
      [GROUP_SWITCHES_KEY]: Object.fromEntries(PRESETS.browse.map((id) => [id, true])),
      [SETTINGS_KEY]: {
        list_tabs: true,
        read_page: true,
        open_tab: true,
        create_bookmark: true,
        add_reading_list_entry: true,
      },
    },
  });
  assert.equal(await applyPreset(chrome, "off", EVERY_SITE), true);
  assert.deepEqual(requests(calls), []);
  assert.deepEqual(stored[GROUP_SWITCHES_KEY], {});
  const tools = await settings(chrome);
  assert.deepEqual(
    [
      tools.list_tabs,
      tools.read_page,
      tools.open_tab,
      tools.create_bookmark,
      tools.add_reading_list_entry,
      tools.get_platform_info,
    ],
    [false, false, false, false, false, true],
  );
  // The browser in this test holds the permissions of the groups in no preset
  // as well, which no group that was on lists. Off takes them back too.
  assert.deepEqual(removals(calls), [allOptional()]);
  assert.deepEqual([...permissions.held.permissions], []);
  assert.deepEqual([...permissions.held.origins], []);
});

// Saving the allowed sites asks the browser for each site whether or not a
// group is on, so access can be held with every group off. Off gives it back,
// and a host the list no longer names that the browser still held with it. A
// permission the manifest requires is held as well, and is not given back.
test("Off takes back the access that saving the allowed sites granted while no group was on", async () => {
  const OLD = "https://old.example";
  const { chrome, calls, permissions } = browser({
    held: { permissions: ["nativeMessaging", "storage"], origins: [`${OLD}/*`] },
  });
  assert.equal((await allowSites(chrome, `${NEWS}\n${INTRANET}`)).granted, true);
  assert.deepEqual([...permissions.held.origins], [`${OLD}/*`, `${NEWS}/*`, `${INTRANET}/*`]);
  assert.equal(await applyPreset(chrome, "off", allowing(NEWS, INTRANET)), true);
  assert.deepEqual(removals(calls), [
    { origins: [`${OLD}/*`, `${NEWS}/*`, `${INTRANET}/*`] },
  ]);
  assert.deepEqual([...permissions.held.origins], []);
  assert.deepEqual([...permissions.held.permissions], ["nativeMessaging", "storage"]);
});

// Read and Browse are Look and page-read on the allowed sites. A scope the
// person set to all sites does not carry into them: the request is for the
// allowed sites, the scope is saved as the allowed sites, and the access to all
// sites that the group held is given back. Look names no group that reads pages
// and leaves the scopes alone.
test("the Read and Browse presets limit page-read to the allowed sites and give back the access to all sites", async () => {
  const everySite = ["<all_urls>"];
  for (const name of ["read", "browse"]) {
    const { chrome, stored, calls, permissions } = browser({
      held: { permissions: PAGES, origins: everySite },
      stored: {
        [GROUP_SWITCHES_KEY]: { "page-read": true },
        [SITE_SCOPES_KEY]: { "page-read": SCOPE_ALL, "network-watch": SCOPE_ALL },
        [ALLOWED_SITES_KEY]: [NEWS],
      },
    });
    const access = {
      allowed: [NEWS],
      scopes: { "page-read": SCOPE_ALL, "network-watch": SCOPE_ALL },
    };
    assert.equal(await applyPreset(chrome, name, access), true, name);
    assert.deepEqual(calls[0][1].origins, [`${NEWS}/*`], name);
    assert.deepEqual(stored[SITE_SCOPES_KEY], { "network-watch": SCOPE_ALL }, name);
    assert.deepEqual(removals(calls), [{ origins: everySite }], name);
    assert.deepEqual([...permissions.held.origins], [`${NEWS}/*`], name);
  }

  const look = browser({ stored: { [ALLOWED_SITES_KEY]: [NEWS] } });
  await applyPreset(look.chrome, "look", allowing(NEWS));
  assert.equal(look.stored[SITE_SCOPES_KEY], undefined);
});

// page-read was on all sites before it was turned off, and its scope was kept. The
// preset puts it on the allowed sites, so the access to all sites that
// network-watch held is no longer needed by any group left on.
test("a preset gives back the access to all sites of a group it turns off, though a group it turns on had all sites before", async () => {
  const everySite = ["https://*/*", "http://*/*"];
  const scopes = { "page-read": SCOPE_ALL, "network-watch": SCOPE_ALL };
  const { chrome, calls, permissions } = browser({
    held: { permissions: ["webRequest"], origins: everySite },
    stored: { [GROUP_SWITCHES_KEY]: { "network-watch": true }, [SITE_SCOPES_KEY]: scopes },
  });
  assert.equal(await applyPreset(chrome, "read", { allowed: [NEWS], scopes }), true);
  assert.deepEqual(removals(calls), [{ permissions: ["webRequest"], origins: everySite }]);
  assert.deepEqual([...permissions.held.origins], [`${NEWS}/*`]);
});

// The person may have turned a tool off inside a group that is on. Applying a
// preset that names the group leaves that choice alone.
test("a preset leaves the tool switches of a group that was already on", async () => {
  const { chrome, stored } = browser({
    held: allOptional(),
    stored: {
      [GROUP_SWITCHES_KEY]: { "tabs-read": true },
      [SETTINGS_KEY]: { list_tabs: true, list_frames: false },
    },
  });
  await applyPreset(chrome, "look");
  const tools = await settings(chrome);
  assert.deepEqual([tools.list_tabs, tools.list_frames], [true, false]);
  assert.equal(tools.search_history, true);
  assert.deepEqual(Object.keys(stored[GROUP_SWITCHES_KEY]), PRESETS.look);
});

test("a preset saves the switches before it takes anything back", async () => {
  const { chrome, stored, permissions } = browser({
    held: allOptional(),
    stored: {
      [GROUP_SWITCHES_KEY]: { "page-read": true },
      [SETTINGS_KEY]: { read_page: true },
    },
  });
  let seen;
  const failure = new Error("the browser would not remove it");
  permissions.remove = async () => {
    seen = structuredClone(stored);
    throw failure;
  };
  await assert.rejects(applyPreset(chrome, "off"), failure);
  assert.deepEqual(seen[GROUP_SWITCHES_KEY], {});
  assert.equal(seen[SETTINGS_KEY].read_page, false);
});

test("a name that is not a preset is refused before the browser is asked", async () => {
  const { chrome, calls, stored } = browser();
  for (const name of ["Look", "", "toString", "__proto__", "look,read"]) {
    assert.throws(() => presetGroups(name), /there is no preset named/, name);
    await assert.rejects(applyPreset(chrome, name), /there is no preset named/, name);
  }
  assert.deepEqual(calls, []);
  assert.deepEqual(stored, {});
});

// The sites a group that reads pages reaches. What it needs of the browser is
// the host of each allowed site, or <all_urls> for all sites, and the request is
// still the first call of the click.
const NEWS = "https://news.example";
const INTRANET = "http://intranet:8080";
const PAGES = ["tabs", "scripting", "webNavigation"];
// What page-read lists that tabs-read does not.
const PAGE_ONLY = ["scripting"];
const allowing = (...allowed) => ({ allowed, scopes: {} });

test("turning page-read on asks for the allowed sites, none, or all of them by its scope", async () => {
  const cases = [
    ["none are allowed", NO_ACCESS, { permissions: PAGES }],
    [
      "two are allowed",
      allowing(NEWS, INTRANET),
      { permissions: PAGES, origins: [`${NEWS}/*`, `${INTRANET}/*`] },
    ],
    [
      "all sites",
      { allowed: [NEWS], scopes: { "page-read": SCOPE_ALL } },
      { permissions: PAGES, origins: ["<all_urls>"] },
    ],
  ];
  for (const [why, access, query] of cases) {
    const { chrome, stored, calls } = browser();
    assert.equal(await turnOn(chrome, "page-read", access), true, why);
    assert.deepEqual(calls[0], ["permissions.request", query], why);
    assert.equal(calls.length, 1, why);
    assert.deepEqual(stored[GROUP_SWITCHES_KEY], { "page-read": true }, why);
  }
});

// Read is Look and page-read on the allowed sites, so with none allowed it asks
// for no host.
test("the Read preset asks for the hosts of the allowed sites only", async () => {
  const look = {
    permissions: ["tabs", "tabGroups", "webNavigation", "sessions", "history", "topSites", "bookmarks", "readingList"],
  };
  const none = browser();
  await applyPreset(none.chrome, "read", NO_ACCESS);
  assert.deepEqual(none.calls[0], [
    "permissions.request",
    { permissions: [...look.permissions, ...PAGE_ONLY] },
  ]);
  const some = browser();
  await applyPreset(some.chrome, "read", allowing(NEWS));
  assert.deepEqual(some.calls[0], [
    "permissions.request",
    { permissions: [...look.permissions, ...PAGE_ONLY], origins: [`${NEWS}/*`] },
  ]);
});

test("turning page-read off takes back the hosts of the allowed sites with its permissions", async () => {
  const { chrome, calls } = browser({
    held: { permissions: PAGES, origins: [`${NEWS}/*`] },
    stored: {
      [GROUP_SWITCHES_KEY]: { "page-read": true },
      [ALLOWED_SITES_KEY]: [NEWS],
    },
  });
  await turnOff(chrome, "page-read");
  assert.deepEqual(removals(calls), [{ permissions: PAGES, origins: [`${NEWS}/*`] }]);
});

// One of the sites loses its host in Brave's own settings. The group lists it,
// so the group is no longer held and goes off with its tools.
test("a group limited to the allowed sites is turned off when the browser stops holding one of them", async () => {
  for (const [held, goesOff] of [
    [{ permissions: PAGES, origins: [`${NEWS}/*`, `${INTRANET}/*`] }, false],
    [{ permissions: PAGES, origins: [`${NEWS}/*`] }, true],
    [{ permissions: PAGES, origins: ["https://*/*", "http://*/*"] }, false],
    [{ permissions: PAGES, origins: ["<all_urls>"] }, false],
  ]) {
    const { chrome, stored } = browser({
      held,
      stored: {
        [GROUP_SWITCHES_KEY]: { "page-read": true },
        [SETTINGS_KEY]: { read_page: true },
        [ALLOWED_SITES_KEY]: [NEWS, INTRANET],
      },
    });
    await turnOffUnheld(chrome);
    assert.equal(
      stored[GROUP_SWITCHES_KEY]["page-read"] === undefined,
      goesOff,
      JSON.stringify(held),
    );
    assert.equal(stored[SETTINGS_KEY].read_page, !goesOff, JSON.stringify(held));
  }
});

// The browser accepts a request only from a click, so the request for the
// sites is the first call, before the list is read or saved.
test("saving the allowed sites asks the browser for every site written, first, and then saves them", async () => {
  const { chrome, stored, calls } = browser({
    stored: { [ALLOWED_SITES_KEY]: [NEWS] },
  });
  const result = await allowSites(
    chrome,
    `${NEWS}/front\n\n${INTRANET}/\nhttps://NEWS.example:443/`,
  );
  assert.deepEqual(result, {
    sites: [NEWS, INTRANET],
    rejected: [],
    granted: true,
    notTakenBack: null,
  });
  assert.deepEqual(calls, [
    ["permissions.request", { origins: [`${NEWS}/*`, `${INTRANET}/*`] }],
  ]);
  assert.deepEqual(stored[ALLOWED_SITES_KEY], [NEWS, INTRANET]);
});

test("a site Brave does not grant, or refuses to ask for, is not saved", async () => {
  const declined = browser({ decision: false, stored: { [ALLOWED_SITES_KEY]: [NEWS] } });
  const result = await allowSites(declined.chrome, `${NEWS}\n${INTRANET}`);
  assert.equal(result.granted, false);
  assert.deepEqual(result.rejected, []);
  assert.deepEqual(declined.stored[ALLOWED_SITES_KEY], [NEWS]);
  assert.deepEqual(removals(declined.calls), []);

  const policy = new Error("Permissions are blocked by enterprise policy.");
  const blocked = browser({ decision: policy, stored: { [ALLOWED_SITES_KEY]: [NEWS] } });
  await assert.rejects(allowSites(blocked.chrome, INTRANET), policy);
  assert.deepEqual(blocked.stored[ALLOWED_SITES_KEY], [NEWS]);
});

// A mistyped line stops the save as it does for the blocked list, and the
// browser is not asked about the other lines.
test("a line that is not one address stops the save before the browser is asked", async () => {
  const { chrome, stored, calls } = browser({ stored: { [ALLOWED_SITES_KEY]: [NEWS] } });
  const result = await allowSites(chrome, `${INTRANET}\nnews.example\nhttps://*.bank.example`);
  assert.equal(result.granted, false);
  assert.deepEqual(result.rejected, ["news.example", "https://*.bank.example"]);
  assert.deepEqual(calls, []);
  assert.deepEqual(stored[ALLOWED_SITES_KEY], [NEWS]);
});

// The list is saved before access is taken back, so a failure to take it back
// leaves the list the person wrote in force and says what was not done.
test("a site taken off the list is saved off before its access is taken back, and only its access", async () => {
  const { chrome, stored, permissions } = browser({
    held: { permissions: PAGES, origins: [`${NEWS}/*`, `${INTRANET}/*`] },
    stored: { [ALLOWED_SITES_KEY]: [NEWS, INTRANET] },
  });
  let listWhenTaken;
  const remove = permissions.remove;
  permissions.remove = async (query) => {
    listWhenTaken = [...stored[ALLOWED_SITES_KEY]];
    return remove(query);
  };
  const result = await allowSites(chrome, NEWS);
  assert.equal(result.granted, true);
  assert.deepEqual(listWhenTaken, [NEWS]);
  assert.deepEqual([...permissions.held.origins], [`${NEWS}/*`]);
  assert.deepEqual([...permissions.held.permissions], PAGES);

  permissions.remove = async () => {
    throw new Error("cannot remove");
  };
  const failed = await allowSites(chrome, "");
  assert.equal(failed.granted, true);
  assert.equal(failed.notTakenBack, "cannot remove");
  assert.deepEqual(stored[ALLOWED_SITES_KEY], []);
});

// An empty list asks the browser for nothing: an empty request is not a
// question it answers.
test("saving an empty list asks the browser for nothing", async () => {
  const { chrome, stored, calls } = browser({ stored: { [ALLOWED_SITES_KEY]: [] } });
  const result = await allowSites(chrome, "\n  \n");
  assert.equal(result.granted, true);
  assert.deepEqual(calls, []);
  assert.deepEqual(stored[ALLOWED_SITES_KEY], []);
});

// Choosing all sites for a group that is on asks for <all_urls> first, which
// page-read needs for a picture, and nothing is saved unless the browser grants
// it. A group that is off only
// remembers the choice.
test("widening a group that is on asks for all sites first and saves the scope if granted", async () => {
  const on = browser({
    held: { permissions: PAGES, origins: [`${NEWS}/*`] },
    stored: { [GROUP_SWITCHES_KEY]: { "page-read": true } },
  });
  assert.equal(await setScope(on.chrome, "page-read", SCOPE_ALL, allowing(NEWS), true), true);
  assert.deepEqual(on.calls[0], [
    "permissions.request",
    { permissions: PAGES, origins: ["<all_urls>"] },
  ]);
  assert.deepEqual(removals(on.calls), []);
  assert.deepEqual(on.stored[SITE_SCOPES_KEY], { "page-read": SCOPE_ALL });

  const declined = browser({ decision: false });
  assert.equal(
    await setScope(declined.chrome, "page-read", SCOPE_ALL, NO_ACCESS, true),
    false,
  );
  assert.equal(declined.stored[SITE_SCOPES_KEY], undefined);

  const off = browser();
  assert.equal(await setScope(off.chrome, "page-read", SCOPE_ALL, NO_ACCESS, false), true);
  assert.deepEqual(off.calls, []);
  assert.deepEqual(off.stored[SITE_SCOPES_KEY], { "page-read": SCOPE_ALL });
});

// Back to the allowed sites asks for their hosts, since <all_urls> may have been
// held when they were allowed, and then gives back <all_urls>.
test("narrowing a group that is on asks for the allowed sites and gives back all sites", async () => {
  const { chrome, stored, calls, permissions } = browser({
    held: { permissions: PAGES, origins: ["<all_urls>"] },
    stored: {
      [GROUP_SWITCHES_KEY]: { "page-read": true },
      [SITE_SCOPES_KEY]: { "page-read": SCOPE_ALL },
    },
  });
  const access = { allowed: [NEWS], scopes: { "page-read": SCOPE_ALL } };
  assert.equal(await setScope(chrome, "page-read", "allowed", access, true), true);
  assert.deepEqual(calls[0], [
    "permissions.request",
    { permissions: PAGES, origins: [`${NEWS}/*`] },
  ]);
  assert.deepEqual(stored[SITE_SCOPES_KEY], {});
  assert.deepEqual(removals(calls), [{ origins: ["<all_urls>"] }]);
  assert.deepEqual([...permissions.held.origins], [`${NEWS}/*`]);
});

// Brave holds one set of hosts for the extension, so the access to all sites that
// a group no longer needs is given back only if no other group that is on needs
// it: not a group that is off, and not a group on the allowed sites. A group
// that reads pages but takes no pictures lists the two patterns where page-read
// lists <all_urls>, so it keeps the two patterns and not <all_urls>.
test("narrowing a group takes back all sites only when no other group that is on still needs them", async () => {
  const everySite = ["https://*/*", "http://*/*"];
  const all = ["<all_urls>"];
  const cases = [
    ["network-watch is on, on all sites", true, SCOPE_ALL, [{ origins: all }], everySite],
    ["network-watch is on, on the allowed sites", true, "allowed", [{ origins: all }], []],
    ["network-watch is off, though its scope is all sites", false, SCOPE_ALL, [{ origins: all }], []],
  ];
  for (const [why, otherIsOn, otherScope, taken, kept] of cases) {
    // As siteAccess gives them: a group on the allowed sites has no entry.
    const scopes = {
      "page-read": SCOPE_ALL,
      ...(otherScope === SCOPE_ALL ? { "network-watch": SCOPE_ALL } : {}),
    };
    const { chrome, stored, calls, permissions } = browser({
      held: {
        permissions: [...PAGES, "webRequest"],
        origins: [...all, ...(otherIsOn && otherScope === SCOPE_ALL ? everySite : [])],
      },
      stored: {
        [GROUP_SWITCHES_KEY]: { "page-read": true, "network-watch": otherIsOn },
        [SITE_SCOPES_KEY]: scopes,
      },
    });
    const access = { allowed: [NEWS], scopes };
    assert.equal(await setScope(chrome, "page-read", "allowed", access, true), true, why);
    assert.deepEqual(removals(calls), taken, why);
    assert.deepEqual(
      stored[SITE_SCOPES_KEY],
      otherScope === SCOPE_ALL ? { "network-watch": SCOPE_ALL } : {},
      why,
    );
    assert.deepEqual([...permissions.held.origins], [...kept, `${NEWS}/*`], why);
  }
});

// Access to all sites that two groups that are on both need as <all_urls>, which
// page-read does for its pictures and screen does for record_gif, stays when one
// of them is narrowed. What the narrowed group needed is judged against every
// group that is on, and not against the narrowed group alone.
test("narrowing a group keeps the access to all sites another group that is on needs for itself", async () => {
  const scopes = { "page-read": SCOPE_ALL, screen: SCOPE_ALL };
  const { chrome, stored, calls, permissions } = browser({
    held: { permissions: [...PAGES, "desktopCapture"], origins: ["<all_urls>"] },
    stored: {
      [GROUP_SWITCHES_KEY]: { "page-read": true, screen: true },
      [SITE_SCOPES_KEY]: scopes,
    },
  });
  const access = { allowed: [NEWS], scopes };
  assert.equal(await setScope(chrome, "page-read", "allowed", access, true), true);
  assert.deepEqual(removals(calls), []);
  assert.deepEqual(stored[SITE_SCOPES_KEY], { screen: SCOPE_ALL });
  assert.deepEqual([...permissions.held.origins], ["<all_urls>", `${NEWS}/*`]);

  // With screen off, nothing else needs <all_urls> and it is given back.
  const alone = browser({
    held: { permissions: [...PAGES, "desktopCapture"], origins: ["<all_urls>"] },
    stored: {
      [GROUP_SWITCHES_KEY]: { "page-read": true, screen: false },
      [SITE_SCOPES_KEY]: scopes,
    },
  });
  assert.equal(await setScope(alone.chrome, "page-read", "allowed", access, true), true);
  assert.deepEqual(removals(alone.calls), [{ origins: ["<all_urls>"] }]);
  assert.deepEqual([...alone.permissions.held.origins], [`${NEWS}/*`]);
});

test("a scope is saved only for a group that reads pages, and only the two that exist", async () => {
  const { chrome, stored, calls } = browser();
  await assert.rejects(
    setScope(chrome, "tabs-read", SCOPE_ALL, NO_ACCESS, true),
    /does not read pages/,
  );
  await assert.rejects(
    setScope(chrome, "page-read", "everything", NO_ACCESS, true),
    /no scope named everything/,
  );
  // A group limited to the allowed sites has no scope to choose, so a request
  // for all sites for it is refused before the browser is asked.
  await assert.rejects(
    setScope(chrome, "scripts", SCOPE_ALL, NO_ACCESS, true),
    /scripts is limited to the allowed sites and has no scope/,
  );
  assert.deepEqual(calls, []);
  assert.deepEqual(stored, {});
});

// Running code is asked for with the permission that runs it and the hosts of the
// sites the person allowed, and never for all sites, whatever a stored scope says.
test("turning scripts on asks for userScripts and the allowed sites, and never for all sites", async () => {
  for (const [why, access, query] of [
    ["none are allowed", NO_ACCESS, { permissions: ["userScripts"] }],
    [
      "two are allowed",
      allowing(NEWS, INTRANET),
      { permissions: ["userScripts"], origins: [`${NEWS}/*`, `${INTRANET}/*`] },
    ],
    [
      "a scope of all sites is stored",
      { allowed: [NEWS], scopes: { scripts: SCOPE_ALL } },
      { permissions: ["userScripts"], origins: [`${NEWS}/*`] },
    ],
  ]) {
    const { chrome, stored, calls } = browser();
    assert.equal(await turnOn(chrome, "scripts", access), true, why);
    assert.deepEqual(calls[0], ["permissions.request", query], why);
    assert.equal(calls.length, 1, why);
    assert.deepEqual(stored[GROUP_SWITCHES_KEY], { scripts: true }, why);
    assert.equal((await settings(chrome)).run_script, true, why);
  }
});

// The keep_awake request. `power` exists only while the browser holds the
// permission, as the API does, and the calls to it and to the alarm are
// recorded with the permission calls, so a test sees which came first.
function withAwake(made) {
  const { chrome, calls, permissions } = made;
  const power = {
    requestKeepAwake: (level) => calls.push(["power.requestKeepAwake", level]),
    releaseKeepAwake: () => calls.push(["power.releaseKeepAwake"]),
  };
  Object.defineProperty(chrome, "power", {
    get: () => (permissions.held.permissions.has("power") ? power : undefined),
  });
  chrome.alarms = {
    async clear(name) {
      calls.push(["alarms.clear", name]);
      return true;
    },
  };
  return made;
}

const standing = () => ({
  [AWAKE_KEY]: { level: "system", until: Date.now() + HOUR_MS },
});

const RELEASE_THEN_TAKE_BACK = [
  "power.releaseKeepAwake",
  "alarms.clear",
  "permissions.remove",
];

// The names of the calls that release the request or take permissions back.
const awakeCalls = (calls) =>
  calls
    .map(([name]) => name)
    .filter((name) => RELEASE_THEN_TAKE_BACK.includes(name));

const BROWSER_INFO_ON = {
  [GROUP_SWITCHES_KEY]: { "browser-info": true },
  [SETTINGS_KEY]: { keep_awake: true, get_idle_state: true },
};

// The browser's `power` API is gone once its permission is taken back, so the
// request is released while the group still holds it.
test("turning browser-info off releases the keep_awake request before the power permission is taken back", async () => {
  const { chrome, session, calls, permissions } = withAwake(
    browser({ held: allOptional(), stored: { ...BROWSER_INFO_ON }, session: standing() }),
  );
  await turnOff(chrome, "browser-info");
  assert.deepEqual(awakeCalls(calls), RELEASE_THEN_TAKE_BACK);
  assert.ok(!(AWAKE_KEY in session));
  assert.equal(permissions.held.permissions.has("power"), false);
  assert.deepEqual(calls.find(([name]) => name === "alarms.clear"), ["alarms.clear", AWAKE_ALARM]);
});

test("a group that lists no keep_awake leaves the request alone when it is turned off", async () => {
  const { chrome, session, calls } = withAwake(
    browser({
      held: allOptional(),
      stored: { [GROUP_SWITCHES_KEY]: { "browser-info": true, "tabs-read": true } },
      session: standing(),
    }),
  );
  await turnOff(chrome, "tabs-read");
  assert.deepEqual(awakeCalls(calls), ["permissions.remove"]);
  assert.ok(AWAKE_KEY in session);
});

// The tool's switch is the only way to turn keep_awake off while its group stays
// on, and a call to undo the request (0 minutes) is refused once the tool is
// off, so the switch itself ends what the tool made.
test("turning the keep_awake switch off releases the request and keeps the permissions, and no other tool switch does", async () => {
  const { chrome, session, calls, permissions, stored } = withAwake(
    browser({ held: allOptional(), stored: { ...BROWSER_INFO_ON }, session: standing() }),
  );
  await setTool(chrome, "get_idle_state", false);
  await setTool(chrome, "keep_awake", true);
  assert.deepEqual(awakeCalls(calls), []);
  assert.ok(AWAKE_KEY in session);
  await setTool(chrome, "keep_awake", false);
  assert.deepEqual(awakeCalls(calls), ["power.releaseKeepAwake", "alarms.clear"]);
  assert.deepEqual(calls.find(([name]) => name === "alarms.clear"), ["alarms.clear", AWAKE_ALARM]);
  assert.ok(!(AWAKE_KEY in session));
  assert.equal(stored[SETTINGS_KEY].keep_awake, false);
  assert.equal(stored[SETTINGS_KEY].get_idle_state, false);
  assert.equal(permissions.held.permissions.has("power"), true);
  assert.deepEqual(stored[GROUP_SWITCHES_KEY], { "browser-info": true });
});

test("a preset that does not name browser-info releases the keep_awake request before it takes the power permission back", async () => {
  for (const name of ["look", "off"]) {
    const { chrome, session, calls, permissions } = withAwake(
      browser({
        held: allOptional(),
        stored: { ...BROWSER_INFO_ON },
        session: standing(),
      }),
    );
    assert.equal(await applyPreset(chrome, name), true, name);
    assert.deepEqual(awakeCalls(calls), RELEASE_THEN_TAKE_BACK, name);
    assert.ok(!(AWAKE_KEY in session), name);
    assert.equal(permissions.held.permissions.has("power"), false, name);
  }
});

// The hour is over, and the worker turns the group off at its start or on the
// alarm. A request made in the hour's last minutes does not outlive it.
test("the end of a one-hour lifetime releases the keep_awake request, and a lifetime that has not ended does not", async () => {
  const lifetime = (expires) => ({
    [LIFETIMES_KEY]: { "browser-info": { lifetime: "one-hour", expires } },
  });
  const ended = withAwake(
    browser({
      held: allOptional(),
      stored: { ...BROWSER_INFO_ON, ...lifetime(Date.now() - 1) },
      session: standing(),
    }),
  );
  await turnOffLapsed(ended.chrome);
  assert.deepEqual(awakeCalls(ended.calls), RELEASE_THEN_TAKE_BACK);
  assert.ok(!(AWAKE_KEY in ended.session));
  assert.deepEqual(ended.stored[GROUP_SWITCHES_KEY], {});

  const running = withAwake(
    browser({
      held: allOptional(),
      stored: { ...BROWSER_INFO_ON, ...lifetime(Date.now() + HOUR_MS) },
      session: standing(),
    }),
  );
  await turnOffLapsed(running.chrome);
  assert.deepEqual(awakeCalls(running.calls), []);
  assert.ok(AWAKE_KEY in running.session);
});

// The person takes a permission back in the browser's settings. `power` may
// still be there when the event arrives, and then the request is released. If
// it is the permission that went, the API is gone: nothing can be told to the
// browser, and the saved request and its alarm are forgotten.
test("a permission of browser-info taken back in the browser releases the keep_awake request where the power API is still there", async () => {
  const { chrome, session, calls, permissions } = withAwake(
    browser({ held: allOptional(), stored: { ...BROWSER_INFO_ON }, session: standing() }),
  );
  await permissions.revoke({ permissions: ["idle"] });
  await turnOffUnheld(chrome);
  assert.deepEqual(awakeCalls(calls), RELEASE_THEN_TAKE_BACK);
  assert.ok(!(AWAKE_KEY in session));
});

test("the power permission taken back in the browser forgets the keep_awake request and its alarm, and does not fail", async () => {
  const { chrome, session, calls, stored, permissions } = withAwake(
    browser({ held: allOptional(), stored: { ...BROWSER_INFO_ON }, session: standing() }),
  );
  await permissions.revoke({ permissions: ["power"] });
  assert.equal(chrome.power, undefined);
  await turnOffUnheld(chrome);
  assert.deepEqual(awakeCalls(calls), ["alarms.clear", "permissions.remove"]);
  assert.ok(!(AWAKE_KEY in session));
  assert.deepEqual(stored[GROUP_SWITCHES_KEY], {});
});

// The group is on again with the power API present. A request the browser kept
// from before, or one made in a lifetime that ended before the worker turned
// the group off, is not carried into the new one.
test("turning browser-info on releases a keep_awake request left from before, after the browser has granted the request", async () => {
  const { chrome, session, calls } = withAwake(
    browser({
      stored: {
        ...BROWSER_INFO_ON,
        [LIFETIMES_KEY]: {
          "browser-info": { lifetime: "one-hour", expires: Date.now() - 1 },
        },
      },
      session: standing(),
    }),
  );
  assert.equal(await turnOn(chrome, "browser-info", NO_ACCESS, "one-hour"), true);
  assert.equal(calls[0][0], "permissions.request");
  assert.deepEqual(awakeCalls(calls), ["power.releaseKeepAwake", "alarms.clear"]);
  assert.ok(!(AWAKE_KEY in session));
});

test("a release that fails does not stop the permissions being taken back", async () => {
  const { chrome, session, calls, stored } = withAwake(
    browser({ held: allOptional(), stored: { ...BROWSER_INFO_ON }, session: standing() }),
  );
  const failure = new Error("the alarm could not be cleared");
  chrome.alarms.clear = async () => {
    throw failure;
  };
  await assert.rejects(turnOff(chrome, "browser-info"), failure);
  assert.deepEqual(stored[GROUP_SWITCHES_KEY], {});
  assert.equal(removals(calls).length, 1);
  assert.deepEqual(removals(calls)[0].permissions.includes("power"), true);
  assert.ok(AWAKE_KEY in session);
});

// What a person held out. The items are in session storage and the entries are
// in the context menu, which exists only while the browser holds `contextMenus`
// and says it is not available once the permission is taken back, so both are
// let go while the group still holds the permission, as the keep_awake request
// is.
const HELD_ON = {
  [GROUP_SWITCHES_KEY]: { held: true },
  [SETTINGS_KEY]: { take_held: true },
};

const aHeldItem = () => ({
  [HELD_KEY]: {
    items: [{ kind: "tab", page: "https://news.example/", time: Date.now() }],
    forgotten: 0,
  },
});

// A browser that holds everything, with an item held and the entries in its menu.
function holdingOut(extra = {}) {
  const made = browser({
    held: allOptional(),
    stored: { ...HELD_ON, ...extra.stored },
    session: { ...aHeldItem(), ...extra.session },
  });
  const menus = withMenus(made.chrome, {
    calls: made.calls,
    permissions: made.permissions,
  });
  menus.entries.set("bravebot-hold-tab", {});
  return { ...made, menus };
}

const HELD_RELEASED = ["contextMenus.removeAll", "permissions.remove"];

const heldCalls = (calls) =>
  calls
    .map(([name]) => name)
    .filter((name) => name.startsWith("contextMenus.") || name === "permissions.remove");

test("turning held off forgets the items and removes the entries before the contextMenus permission is taken back", async () => {
  const { chrome, session, calls, menus, permissions } = holdingOut();
  await turnOff(chrome, "held");
  assert.deepEqual(heldCalls(calls), HELD_RELEASED);
  assert.ok(!(HELD_KEY in session));
  assert.equal(menus.entries.size, 0);
  assert.equal(permissions.held.permissions.has("contextMenus"), false);
});

test("a group that lists no take_held leaves what was held out alone when it is turned off", async () => {
  const { chrome, session, calls, menus } = holdingOut({
    stored: { [GROUP_SWITCHES_KEY]: { held: true, "tabs-read": true } },
  });
  await turnOff(chrome, "tabs-read");
  assert.deepEqual(heldCalls(calls), ["permissions.remove"]);
  assert.equal(session[HELD_KEY].items.length, 1);
  assert.equal(menus.entries.size, 1);
});

test("Turn everything off and a preset that does not name held forget what was held out and remove the entries first", async () => {
  for (const name of ["off", "look"]) {
    const { chrome, session, calls, permissions } = holdingOut();
    assert.equal(await applyPreset(chrome, name), true, name);
    assert.deepEqual(heldCalls(calls), HELD_RELEASED, name);
    assert.ok(!(HELD_KEY in session), name);
    assert.equal(permissions.held.permissions.has("contextMenus"), false, name);
  }
  const { chrome, session, calls } = holdingOut();
  await turnEverythingOff(chrome);
  assert.deepEqual(heldCalls(calls), HELD_RELEASED);
  assert.ok(!(HELD_KEY in session));
});

test("the end of a one-hour lifetime forgets what was held out, and a lifetime that has not ended does not", async () => {
  const lifetime = (expires) => ({
    [LIFETIMES_KEY]: { held: { lifetime: "one-hour", expires } },
  });
  const ended = holdingOut({ stored: lifetime(Date.now() - 1) });
  await turnOffLapsed(ended.chrome);
  assert.deepEqual(heldCalls(ended.calls), HELD_RELEASED);
  assert.ok(!(HELD_KEY in ended.session));
  assert.deepEqual(ended.stored[GROUP_SWITCHES_KEY], {});

  const running = holdingOut({ stored: lifetime(Date.now() + HOUR_MS) });
  await turnOffLapsed(running.chrome);
  assert.deepEqual(heldCalls(running.calls), []);
  assert.equal(running.session[HELD_KEY].items.length, 1);
});

// The person takes the permission back in the browser's settings. The API is
// gone, so the entries cannot be removed, and the items are forgotten all the
// same.
test("the contextMenus permission taken back in the browser forgets what was held out and does not fail", async () => {
  const { chrome, session, stored, permissions } = holdingOut();
  await permissions.revoke({ permissions: ["contextMenus"] });
  await turnOffUnheld(chrome);
  assert.ok(!(HELD_KEY in session));
  assert.deepEqual(stored[GROUP_SWITCHES_KEY], {});
});

// The group is on again. Items from before, which a lifetime that ended before
// the worker turned the group off can leave, are not carried into the new one.
test("turning held on forgets what was held out before, after the browser has granted the request", async () => {
  const { chrome, session, calls } = browser({
    stored: {
      ...HELD_ON,
      [LIFETIMES_KEY]: { held: { lifetime: "one-hour", expires: Date.now() - 1 } },
    },
    session: aHeldItem(),
  });
  assert.equal(await turnOn(chrome, "held", NO_ACCESS), true);
  assert.deepEqual(calls[0], ["permissions.request", { permissions: ["contextMenus"] }]);
  assert.ok(!(HELD_KEY in session));
  assert.deepEqual(await groupsOn(chrome), ["held"]);
});

test("turning held on asks for contextMenus alone, whatever the scope", async () => {
  const { chrome, calls } = browser();
  assert.equal(await turnOn(chrome, "held", EVERY_SITE), true);
  assert.deepEqual(calls[0], ["permissions.request", { permissions: ["contextMenus"] }]);
  assert.equal(calls.length, 1);
});

test("a menu that cannot be changed does not stop the items being forgotten or the permission being taken back", async () => {
  const { chrome, session, calls, menus, stored } = holdingOut();
  menus.menus.failWith = "the menu is busy";
  await turnOff(chrome, "held");
  assert.ok(!(HELD_KEY in session));
  assert.deepEqual(stored[GROUP_SWITCHES_KEY], {});
  assert.equal(removals(calls).length, 1);
  assert.deepEqual(removals(calls)[0], { permissions: ["contextMenus"] });
});

// The toolbar badge and the popup show the groups that are on, and a group is
// on when the person turned it on and nothing has ended it since.
test("the groups that are on are those the person turned on, still within their lifetime and still held, and never the platform check", async () => {
  const stored = {
    [GROUP_SWITCHES_KEY]: {
      "tabs-read": true,
      "history-read": true,
      "bookmarks-read": true,
      "browser-info": true,
    },
    [LIFETIMES_KEY]: { "history-read": recordOf(ONE_HOUR, Date.now() - 2 * HOUR_MS) },
  };
  // history-read has run out its hour, bookmarks-read lost its permission, and
  // the person never turned on page-read.
  const held = allOptional();
  held.permissions = held.permissions.filter((name) => name !== "bookmarks");
  const { chrome } = browser({ held, stored });
  assert.deepEqual(await groupsOn(chrome), ["tabs-read", "browser-info"]);

  const none = browser({ held: allOptional() });
  assert.deepEqual(await groupsOn(none.chrome), []);
});

// A check that fails is not a yes, so the page, the badge and the popup do not
// draw a group as on that the browser could not be asked about.
test("a group is drawn as off when the browser cannot be asked what it holds", async () => {
  const { chrome } = browser({
    held: allOptional(),
    stored: { [GROUP_SWITCHES_KEY]: { "tabs-read": true, "bookmarks-read": true } },
  });
  assert.deepEqual(await groupsOn(chrome), ["tabs-read", "bookmarks-read"]);
  chrome.permissions.contains = async () => {
    throw new Error("the browser did not answer");
  };
  assert.deepEqual(await groupsOn(chrome), []);
  assert.equal(await isOn(chrome, "tabs-read", { "tabs-read": true }, NO_ACCESS), false);
});

// Turn everything off, from the popup and from the keyboard command, is the Off
// preset: a keep_awake request ends before `power` goes, every group and tool
// is off, and every optional permission and host is given back.
test("turning everything off releases the keep_awake request, turns off every group and its tools and takes back every optional permission and host, asking for nothing", async () => {
  const { chrome, stored, session, calls, permissions } = withAwake(
    browser({
      held: allOptional(),
      stored: {
        [GROUP_SWITCHES_KEY]: { "browser-info": true, "tabs-read": true, "page-read": true },
        [SETTINGS_KEY]: { keep_awake: true, list_tabs: true, read_page: true },
        [ALLOWED_SITES_KEY]: ["https://news.example"],
      },
      session: standing(),
    }),
  );
  await turnEverythingOff(chrome);
  assert.deepEqual(requests(calls), []);
  assert.deepEqual(stored[GROUP_SWITCHES_KEY], {});
  const tools = await settings(chrome);
  assert.deepEqual(
    [tools.keep_awake, tools.list_tabs, tools.read_page, tools.get_platform_info],
    [false, false, false, true],
  );
  assert.deepEqual(awakeCalls(calls), RELEASE_THEN_TAKE_BACK);
  assert.ok(!(AWAKE_KEY in session));
  assert.deepEqual([...permissions.held.permissions], []);
  assert.deepEqual([...permissions.held.origins], []);
  assert.deepEqual(await groupsOn(chrome), []);
});

test("turning everything off with the browser unable to take a permission back leaves every group off and says why", async () => {
  const { chrome, stored, permissions } = browser({
    held: allOptional(),
    stored: { [GROUP_SWITCHES_KEY]: { "tabs-read": true } },
  });
  const failure = new Error("Brave refused");
  permissions.remove = async () => {
    throw failure;
  };
  await assert.rejects(turnEverythingOff(chrome), failure);
  assert.deepEqual(stored[GROUP_SWITCHES_KEY], {});
});

// A browser whose storage takes a different number of turns for each read and
// write, as the browser's does, so writers that overlap interleave. The numbers
// come from a generator that starts at `seed`, so a run can be repeated.
function withLatency(chrome, seed) {
  let state = seed;
  const turns = async () => {
    state = (state * 1103515245 + 12345) % 2147483648;
    for (let turn = 0; turn < state % 4; turn += 1) {
      await new Promise((resolve) => setImmediate(resolve));
    }
  };
  for (const area of [chrome.storage.local, chrome.storage.session]) {
    for (const name of ["get", "set"]) {
      const real = area[name].bind(area);
      area[name] = async (...args) => {
        await turns();
        const answer = await real(...args);
        await turns();
        return answer;
      };
    }
  }
  return chrome;
}

// Runs `body` with `locks` as the `navigator.locks` of the context, and with
// none when it is undefined, whatever the host running the tests has.
async function withLocks(locks, body) {
  const saved = Object.getOwnPropertyDescriptor(globalThis, "navigator");
  Object.defineProperty(globalThis, "navigator", {
    value: { locks },
    configurable: true,
    writable: true,
  });
  try {
    return await body();
  } finally {
    Object.defineProperty(globalThis, "navigator", saved);
  }
}

// The lock manager of a browser, which every context of the extension shares: a
// request for a name is granted once the requests before it for that name have
// finished, in the order they were made. `names` are the names asked for and
// `peak` the most holders of one name there were at once.
function fakeLocks() {
  const tails = new Map();
  const holders = new Map();
  const locks = {
    names: new Set(),
    peak: 0,
    async request(name, callback) {
      locks.names.add(name);
      const run = (tails.get(name) ?? Promise.resolve()).then(async () => {
        holders.set(name, (holders.get(name) ?? 0) + 1);
        locks.peak = Math.max(locks.peak, holders.get(name));
        try {
          return await callback({ name, mode: "exclusive" });
        } finally {
          holders.set(name, holders.get(name) - 1);
        }
      });
      tails.set(name, run.catch(() => {}));
      return run;
    },
  };
  return locks;
}

// Another JavaScript context of the same browser, as the popup is beside the
// options page and the worker: its own copy of switches.js, which keeps a line
// of writers of its own, loaded under `name`.
function contextCopy(name) {
  return import(`../switches.js?context=${name}`);
}

// A `chrome` for that context over the storage and the permissions of `first`.
function chromeOver(first) {
  return {
    permissions: first.permissions,
    storage: {
      local: fakeArea(first.stored),
      session: fakeArea(first.session),
    },
  };
}

// The worker starts both checks when the browser starts: one group has run out
// its lifetime and another lost its permission while the browser was closed.
// Each read the switches, turned its group off and wrote them back.
function startedWithTwoToTurnOff() {
  const held = allOptional();
  held.permissions = held.permissions.filter((name) => name !== "history");
  return browser({
    held,
    stored: {
      [GROUP_SWITCHES_KEY]: {
        downloads: true,
        "history-read": true,
        "bookmarks-read": true,
      },
      [SETTINGS_KEY]: {
        ...Object.fromEntries(GROUPS.downloads.tools.map((tool) => [tool, true])),
        ...Object.fromEntries(GROUPS["history-read"].tools.map((tool) => [tool, true])),
        ...Object.fromEntries(GROUPS["bookmarks-read"].tools.map((tool) => [tool, true])),
      },
    },
  });
}

test("overlapping turn-offs at the start of the worker leave both groups and their tools off, whatever the order the storage answers in, in a context with no locks", async () => {
  await withLocks(undefined, async () => {
    for (let seed = 1; seed <= 150; seed += 1) {
      const { chrome, stored } = startedWithTwoToTurnOff();
      withLatency(chrome, seed);
      await Promise.all([turnOffLapsed(chrome), turnOffUnheld(chrome)]);
      assert.deepEqual(stored[GROUP_SWITCHES_KEY], { "bookmarks-read": true }, `seed ${seed}`);
      for (const group of ["downloads", "history-read"]) {
        for (const tool of GROUPS[group].tools) {
          assert.equal(stored[SETTINGS_KEY][tool], false, `${tool}, seed ${seed}`);
        }
      }
      for (const tool of GROUPS["bookmarks-read"].tools) {
        assert.equal(stored[SETTINGS_KEY][tool], true, `${tool}, seed ${seed}`);
      }
    }
  });
});

test("a turn-off, Turn everything off and a tool switch that overlap leave every group off and the tool switch as the last call set it, in a context with no locks", async () => {
  await withLocks(undefined, async () => {
    for (let seed = 1; seed <= 150; seed += 1) {
      const { chrome, stored } = browser({
        held: allOptional(),
        stored: {
          [GROUP_SWITCHES_KEY]: { "tabs-read": true, "history-read": true, "bookmarks-read": true },
          [SETTINGS_KEY]: { list_tabs: true, search_history: true, search_bookmarks: true },
        },
      });
      withLatency(chrome, seed);
      await Promise.all([
        turnOff(chrome, "tabs-read"),
        turnEverythingOff(chrome),
        turnOff(chrome, "history-read"),
        setTool(chrome, "open_tab", true),
      ]);
      assert.deepEqual(stored[GROUP_SWITCHES_KEY], {}, `seed ${seed}`);
      for (const tool of ["list_tabs", "search_history", "search_bookmarks"]) {
        assert.equal(stored[SETTINGS_KEY][tool], false, `${tool}, seed ${seed}`);
      }
      assert.equal(stored[SETTINGS_KEY].open_tab, true, `seed ${seed}`);
    }
  });
});

// The options page, the popup and the worker each load their own copy of
// switches.js, so the line of writers a copy keeps cannot order the writers of
// another. The popup's Turn everything off, the page's tool switch and the
// worker's turn-offs read the same storage and write it back.
test("writers in the worker, the popup and the options page that overlap leave every group off and the tool switch as the last call set it, whichever order the storage answers in", async () => {
  const popup = await contextCopy("popup");
  const page = await contextCopy("page");
  for (let seed = 1; seed <= 150; seed += 1) {
    const locks = fakeLocks();
    await withLocks(locks, async () => {
      const worker = browser({
        held: allOptional(),
        stored: {
          [GROUP_SWITCHES_KEY]: { "tabs-read": true, "history-read": true, "bookmarks-read": true },
          [SETTINGS_KEY]: { list_tabs: true, search_history: true, search_bookmarks: true },
        },
      });
      const chromes = [worker.chrome, chromeOver(worker), chromeOver(worker)];
      const [inWorker, inPopup, inPage] = chromes;
      chromes.forEach((chrome, index) => withLatency(chrome, seed * 3 + index));
      await Promise.all([
        turnOffLapsed(inWorker),
        turnOffUnheld(inWorker),
        popup.turnEverythingOff(inPopup),
        page.turnOff(inPage, "history-read"),
        page.setTool(inPage, "open_tab", true),
        turnOff(inWorker, "tabs-read"),
      ]);
      const { stored } = worker;
      assert.deepEqual(stored[GROUP_SWITCHES_KEY], {}, `seed ${seed}`);
      for (const tool of ["list_tabs", "search_history", "search_bookmarks"]) {
        assert.equal(stored[SETTINGS_KEY][tool], false, `${tool}, seed ${seed}`);
      }
      assert.equal(stored[SETTINGS_KEY].open_tab, true, `seed ${seed}`);
      assert.equal(locks.names.size, 1, `one lock name, seed ${seed}`);
      assert.equal(locks.peak, 1, `one holder at a time, seed ${seed}`);
    });
  }
});

test("a writer that fails does not stop the writers after it, with locks and without", async () => {
  for (const locks of [undefined, fakeLocks()]) {
    await withLocks(locks, async () => {
      const { chrome, stored } = browser({
        held: allOptional(),
        stored: { [GROUP_SWITCHES_KEY]: { "tabs-read": true, "history-read": true } },
      });
      const failure = new Error("Brave refused");
      const remove = chrome.permissions.remove.bind(chrome.permissions);
      let refused = 0;
      chrome.permissions.remove = async (query) => {
        refused += 1;
        if (refused === 1) {
          throw failure;
        }
        return remove(query);
      };
      const [first, second] = await Promise.allSettled([
        turnOff(chrome, "tabs-read"),
        turnOff(chrome, "history-read"),
      ]);
      assert.equal(first.status, "rejected");
      assert.equal(first.reason, failure);
      assert.equal(second.status, "fulfilled");
      assert.deepEqual(stored[GROUP_SWITCHES_KEY], {});
      assert.equal(refused, 2, "the failed writer ran again");
    });
  }
});

test("a browser that refuses the lock before a writer starts still lets the writer run, once", async () => {
  const locks = {
    async request() {
      throw new DOMException("not allowed here", "SecurityError");
    },
  };
  await withLocks(locks, async () => {
    const { chrome, stored } = browser({
      held: allOptional(),
      stored: { [GROUP_SWITCHES_KEY]: { "tabs-read": true } },
    });
    await turnOff(chrome, "tabs-read");
    await setTool(chrome, "open_tab", true);
    assert.deepEqual(stored[GROUP_SWITCHES_KEY], {});
    assert.equal(stored[SETTINGS_KEY].open_tab, true);
  });
});

// Lifetimes that cannot be read end the lifetime of every group that is on. A
// save that wrote `{}` over them would give each of those groups its default,
// which is until it is turned off for most groups.
test("a save leaves lifetimes that cannot be read as they are while another group is on, so those groups stay ended", async () => {
  for (const damaged of ["garbage", ["tabs-read"], 7, null]) {
    const { chrome, stored } = browser({
      held: allOptional(),
      stored: {
        [GROUP_SWITCHES_KEY]: { "tabs-read": true, "history-read": true },
        [LIFETIMES_KEY]: damaged,
      },
    });
    assert.equal(await turnOn(chrome, "bookmarks-read"), true);
    assert.deepEqual(stored[LIFETIMES_KEY], damaged, JSON.stringify(damaged));
    assert.deepEqual(stored[GROUP_SWITCHES_KEY], {
      "tabs-read": true,
      "history-read": true,
      "bookmarks-read": true,
    });
    for (const id of ["tabs-read", "history-read", "bookmarks-read"]) {
      assert.equal(await lifetimeEnded(chrome, id), "unreadable", `${id} ${JSON.stringify(damaged)}`);
    }
  }
});

// Nothing is left on for the damage to end once the worker has turned the groups
// off, so the lifetimes are written afresh and a group can be turned on again.
test("lifetimes that cannot be read are written afresh once no other group is on", async () => {
  const { chrome, stored } = browser({
    held: allOptional(),
    stored: {
      [GROUP_SWITCHES_KEY]: { "tabs-read": true, "history-read": true },
      [LIFETIMES_KEY]: "garbage",
    },
  });
  await turnOffLapsed(chrome);
  assert.deepEqual(stored[GROUP_SWITCHES_KEY], {});
  assert.deepEqual(stored[LIFETIMES_KEY], {});
  assert.equal(await turnOn(chrome, "tabs-read"), true);
  assert.equal(await lifetimeEnded(chrome, "tabs-read"), null);

  const alone = browser({
    held: allOptional(),
    stored: { [LIFETIMES_KEY]: "garbage" },
  });
  assert.equal(await turnOn(alone.chrome, "downloads"), true);
  assert.deepEqual(alone.stored[LIFETIMES_KEY], { downloads: { lifetime: "until-restart" } });
});
