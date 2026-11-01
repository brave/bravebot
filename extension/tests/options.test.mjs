// The options page's script against the page's own markup, with a fake
// `chrome` that stores what is saved and holds the permissions a person has
// granted, and a fake `document` that has an element for each id in
// options.html and builds the rest as the script asks.
//
// Run with `node --test extension/tests/`.

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { ACTIVITY_KEY } from "../activity.js";
import { CONFIRMATION_WAIT_MS } from "../approval.js";
import { GROUPS, SCOPE_ALL } from "../groups.js";
import { HOUR_MS, LIFETIMES_KEY, RUNS_KEY } from "../lifetimes.js";
import { GROUP_SWITCHES_KEY } from "../switches.js";
import {
  ALLOWED_SITES_KEY,
  BLOCKED_SITES_KEY,
  SETTINGS_KEY,
  SITE_SCOPES_KEY,
} from "../tools.js";
import { allOptional, fakePermissions } from "./fake-permissions.mjs";
import { fakeArea } from "./fake-storage.mjs";

class Element {
  value = "";
  checked = false;
  disabled = false;
  hidden = false;
  className = "";
  name = "";
  type = "";
  dataset = {};
  attributes = {};
  children = [];
  #text = "";
  #listeners = new Map();

  constructor(tagName) {
    this.tagName = tagName;
  }

  append(...nodes) {
    this.children.push(...nodes);
  }

  get textContent() {
    return (
      this.#text +
      this.children
        .map((child) => (typeof child === "string" ? child : child.textContent))
        .join("")
    );
  }

  set textContent(text) {
    this.#text = text;
    this.children = [];
  }

  setAttribute(name, value) {
    this.attributes[name] = value;
  }

  focus() {
    focused = this;
  }

  addEventListener(type, listener) {
    this.#listeners.set(type, listener);
  }

  // A person editing the control: the listener for that event runs, and the
  // save it starts is awaited.
  async fire(type) {
    const listener = this.#listeners.get(type);
    assert.ok(listener, `nothing listens for ${type} on this control`);
    await listener();
  }

  // This element and each element under it, in document order.
  *tree() {
    yield this;
    for (const child of this.children) {
      if (typeof child !== "string") {
        yield* child.tree();
      }
    }
  }
}

let focused = null;
let pages = 0;

// The scope of every group that reads pages when a test chooses none. A group
// limited to the allowed sites has no scope to store.
const EVERY_SITE = Object.fromEntries(
  Object.entries(GROUPS)
    .filter(([, group]) => group.hosts && !group.allowedOnly)
    .map(([id]) => [id, SCOPE_ALL]),
);

const markup = readFileSync(
  new URL("../options.html", import.meta.url),
  "utf8",
);
const ids = [...markup.matchAll(/\bid="([^"]+)"/g)].map(([, id]) => id);

// The page's script is loaded again for each page, with a fresh document and a
// fresh browser, since it reads the globals when a control fires and not only
// when it loads. A page keeps the globals until the next page is opened.
//
//   stored    what chrome.storage.local holds; unless it holds scopes, every
//             group that reads pages is on all sites
//   session   what chrome.storage.session holds
//   held      the permissions the person has granted
//   decision  what permissions.request answers (see fake-permissions.mjs)
async function openPage({
  stored = {},
  session = {},
  held = {},
  decision = true,
  userScripts = undefined,
} = {}) {
  stored = { [SITE_SCOPES_KEY]: EVERY_SITE, ...stored };
  const elements = new Map(ids.map((id) => [id, new Element("div")]));
  const made = [];
  const calls = [];
  const permissions = fakePermissions({ held, calls, decision });
  const changed = new Set();
  // What the next save does before it stores: `held` is a promise it waits on,
  // and `failure` is an error it throws instead.
  const next = { held: null, failure: null };
  globalThis.document = {
    getElementById(id) {
      if (!elements.has(id)) {
        throw new Error(`options.html has no element with the id ${id}`);
      }
      return elements.get(id);
    },
    createElement(tag) {
      const element = new Element(tag);
      made.push(element);
      return element;
    },
  };
  globalThis.chrome = {
    permissions,
    ...(userScripts ? { userScripts } : {}),
    // Turning on browser-info releases a keep_awake request left from before,
    // which clears the alarm it made.
    alarms: { clear: async () => true },
    storage: {
      onChanged: { addListener: (listener) => changed.add(listener) },
      session: fakeArea(session),
      local: {
        async get(key) {
          return key in stored ? { [key]: stored[key] } : {};
        },
        async set(items) {
          const { held: waiting, failure } = next;
          next.held = next.failure = null;
          await waiting;
          if (failure) {
            throw failure;
          }
          Object.assign(stored, items);
          for (const listener of changed) {
            listener(
              Object.fromEntries(Object.keys(items).map((key) => [key, {}])),
              "local",
            );
          }
        },
      },
    },
  };
  await import(`../options.js?page=${(pages += 1)}`);
  await flush();
  return {
    stored,
    session,
    calls,
    permissions,
    next,
    element: (id) => elements.get(id),
    control(name) {
      const found = made.find((element) => element.name === name);
      assert.ok(found, `the page has no control named ${name}`);
      return found;
    },
    // The block the page draws for one group.
    group(id) {
      const found = made.find((element) => element.dataset.group === id);
      assert.ok(found, `the page has no block for the group ${id}`);
      return found;
    },
    groupIds: () =>
      made.filter((element) => element.dataset.group).map((e) => e.dataset.group),
    // The part of a group's block that has the class.
    part(id, className) {
      const found = [...this.group(id).tree()].find(
        (element) => element.className === className,
      );
      assert.ok(found, `the block of ${id} has no ${className}`);
      return found;
    },
  };
}

// The page starts its reads without waiting for them. One turn of the event
// loop runs every continuation they have queued.
const flush = () => new Promise((resolve) => setImmediate(resolve));

const callsNamed = (page, name) =>
  page.calls.filter(([called]) => called === name).map(([, query]) => query);

// The tests that follow each other on one page come first, since opening
// another page replaces the globals this one's controls read.
const main = await openPage({
  stored: {
    [BLOCKED_SITES_KEY]: ["https://bank.example", "http://intranet:8080"],
  },
});
const stored = main.stored;
const field = main.element("blocked-sites");
const status = main.element("blocked-status");
const next = main.next;

test("the blocked sites field shows the saved list, one site to a line", () => {
  assert.equal(field.value, "https://bank.example\nhttp://intranet:8080");
});

// Typing writes nothing, since a save on every keystroke would take a site
// off the list while its line is being edited, and the page says the edit is
// not saved yet.
test("an edit is not saved until the person leaves the box", async () => {
  field.value = "https://bank.exampl";
  await field.fire("input");
  assert.deepEqual(stored[BLOCKED_SITES_KEY], [
    "https://bank.example",
    "http://intranet:8080",
  ]);
  assert.match(status.textContent, /^Not saved yet\./);
});

// A line that is not an address stops the save, so the list saved before keeps
// applying: saving the other lines would drop the bank whose line lost its
// scheme. The line is named so it can be fixed.
test("a line that is not an address stops the save and is named", async () => {
  field.value = "https://news.example\nbank.example";
  await field.fire("change");
  assert.deepEqual(stored[BLOCKED_SITES_KEY], [
    "https://bank.example",
    "http://intranet:8080",
  ]);
  assert.match(status.textContent, /^Not saved: bank\.example\./);
  assert.match(status.textContent, /list saved before still applies/);
  assert.equal(field.value, "https://news.example\nbank.example");
});

// Leaving the box saves and writes the sites back as the origins that were kept.
test("leaving the box saves the list and shows the origins kept", async () => {
  field.value = "https://news.example/front-page\n\nhttps://NEWS.example:443/";
  await field.fire("change");
  assert.deepEqual(stored[BLOCKED_SITES_KEY], ["https://news.example"]);
  assert.equal(status.textContent, "Saved.");
  assert.equal(field.value, "https://news.example");
});

// The write-back happens after the save, and the person may have typed since.
test("text typed while a save runs is not replaced", async () => {
  let release;
  next.held = new Promise((resolve) => {
    release = resolve;
  });
  field.value = "https://mail.example/inbox";
  const leaving = field.fire("change");
  field.value = "https://mail.example/inbox\nhttps://typed.example";
  release();
  await leaving;
  assert.deepEqual(stored[BLOCKED_SITES_KEY], ["https://mail.example"]);
  assert.equal(
    field.value,
    "https://mail.example/inbox\nhttps://typed.example",
  );
});

// A save the browser refuses must not leave the last "Saved." on the page.
test("a save that fails says so", async () => {
  status.textContent = "Saved.";
  next.failure = new Error("storage is full");
  field.value = "https://other.example";
  await field.fire("change");
  assert.equal(status.textContent, "Not saved: storage is full");
  assert.deepEqual(stored[BLOCKED_SITES_KEY], ["https://mail.example"]);
});

// The settings and the list are saved under keys of their own, so saving a
// switch leaves the list and the group switches alone.
test("a tool switch is saved beside the blocked list and not over it", async () => {
  const platform = main.control("get_platform_info");
  assert.equal(platform.checked, true);
  platform.checked = false;
  await platform.fire("change");
  assert.equal(stored[SETTINGS_KEY].get_platform_info, false);
  assert.deepEqual(stored[BLOCKED_SITES_KEY], ["https://mail.example"]);
  assert.equal(stored[GROUP_SWITCHES_KEY], undefined);
});

// A stored list of the wrong shape fails every tool, so the page says it
// cannot read it where it would otherwise show an empty box. Leaving the box
// with a list written then replaces it.
test("a list that cannot be read is said so and replaced by the next save", async () => {
  const page = await openPage({
    stored: { [BLOCKED_SITES_KEY]: "https://bank.example" },
  });
  const unreadable = page.element("blocked-sites");
  assert.equal(unreadable.value, "");
  assert.match(page.element("blocked-status").textContent, /cannot be read/);
  unreadable.value = "https://bank.example";
  await unreadable.fire("change");
  assert.deepEqual(page.stored[BLOCKED_SITES_KEY], ["https://bank.example"]);
});

// The groups that have a tool, and what Brave will ask for each, as Brave 1.98.62
// said when the group's permissions were asked for together (warnings.test.mjs).
const GROUPS_WITH_TOOLS = [
  ["platform", ["get_platform_info"], "Brave shows no prompt of its own."],
  [
    "tabs-read",
    [
      "list_tabs",
      "list_frames",
      "list_windows",
      "list_tab_groups",
      "list_recently_closed",
    ],
    "Brave asks you to allow: Read your browsing history on all your signed-in devices; View and manage your tab groups.",
  ],
  [
    "history-read",
    ["search_history", "get_visits", "list_top_sites", "list_synced_tabs"],
    "Brave asks you to allow: Read and change your browsing history on all your signed-in devices.",
  ],
  [
    "bookmarks-read",
    [
      "search_bookmarks",
      "list_bookmark_folder",
      "list_recent_bookmarks",
      "list_reading_list",
    ],
    "Brave asks you to allow: Read and change your bookmarks; Read and change entries in the reading list.",
  ],
  [
    "page-read",
    [
      "read_page",
      "read_page_links",
      "read_page_forms",
      "list_page_elements",
      "screenshot_page",
    ],
    "Brave asks you to allow: Read and change all your data on all websites.",
  ],
  [
    "tabs-manage",
    [
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
    "Brave asks you to allow: Read your browsing history; View and manage your tab groups.",
  ],
  [
    "bookmarks-write",
    [
      "create_bookmark",
      "create_bookmark_folder",
      "move_bookmark",
      "rename_bookmark",
      "add_reading_list_entry",
      "update_reading_list_entry",
    ],
    "Brave asks you to allow: Read and change your bookmarks; Read and change entries in the reading list.",
  ],
  [
    "navigate",
    [
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
    "Brave asks you to allow: Read your browsing history on all your signed-in devices.",
  ],
  [
    "downloads",
    [
      "save_page_archive",
      "download_file",
      "list_downloads",
      "pause_download",
      "resume_download",
      "cancel_download",
      "show_download",
    ],
    "Brave asks you to allow: Manage your downloads; Read and change all your data on all websites.",
  ],
  [
    "cookies",
    ["list_cookie_names", "delete_cookie"],
    "Brave asks you to allow: Read and change all your data on all websites.",
  ],
  [
    "network-watch",
    ["list_requests"],
    "Brave asks you to allow: Read and change all your data on all websites.",
  ],
  [
    "browser-info",
    [
      "get_privacy_settings",
      "get_content_settings",
      "get_font_settings",
      "get_system_info",
      "get_idle_state",
      "get_access_flags",
      "keep_awake",
    ],
    "Brave asks you to allow: Change your privacy-related settings; Change and grant access to features such as geolocation, microphone, camera, cookies, etc., for all your websites and extensions, including this extension; Identify and eject storage devices.",
  ],
  [
    "remove-data",
    [
      "close_tab",
      "close_window",
      "delete_history_entry",
      "delete_history_range",
      "clear_browsing_data",
      "remove_bookmark",
      "remove_bookmark_folder",
      "remove_reading_list_entry",
      "delete_downloaded_file",
    ],
    "Brave asks you to allow: Read and change your browsing history on all your signed-in devices; Read and change your bookmarks; Read and change entries in the reading list; Manage your downloads.",
  ],
  ["scripts", ["run_script"], "Brave shows no prompt of its own."],
  [
    "input",
    [
      "click_element",
      "type_text",
      "select_option",
      "press_key",
      "scroll_page",
      "wait_for_element",
    ],
    "Brave asks you to allow: Read and change all your data on all websites.",
  ],
  [
    "input-trusted",
    [
      "click_element_trusted",
      "type_text_trusted",
      "press_key_trusted",
      "hover_element",
      "drag_element",
    ],
    "Brave shows no prompt of its own.",
  ],
  ["capture-trusted", ["screenshot_full_page"], "Brave shows no prompt of its own."],
  ["console-trusted", ["read_console"], "Brave shows no prompt of its own."],
  ["upload-trusted", ["choose_file"], "Brave shows no prompt of its own."],
  [
    "screen",
    ["capture_screen", "record_gif"],
    "Brave asks you to allow: Capture content of your screen; Read and change all your data on all websites.",
  ],
  ["held", ["take_held"], "Brave shows no prompt of its own."],
];

// What the browser is asked for when each group is turned on. A group that
// lists no permission, input-trusted, capture-trusted, console-trusted and
// upload-trusted, asks for nothing.
const REQUESTS = {
  "tabs-read": {
    permissions: ["tabs", "tabGroups", "webNavigation", "sessions"],
  },
  "history-read": { permissions: ["history", "topSites", "sessions"] },
  "bookmarks-read": { permissions: ["bookmarks", "readingList"] },
  "page-read": {
    permissions: ["tabs", "scripting", "webNavigation"],
    origins: ["<all_urls>"],
  },
  "tabs-manage": { permissions: ["tabs", "tabGroups"] },
  "bookmarks-write": { permissions: ["bookmarks", "readingList"] },
  navigate: { permissions: ["tabs", "sessions", "search", "webNavigation"] },
  downloads: { permissions: ["downloads", "pageCapture", "tabs"] },
  cookies: {
    permissions: ["cookies"],
    origins: ["https://*/*", "http://*/*"],
  },
  "network-watch": {
    permissions: ["webRequest"],
    origins: ["https://*/*", "http://*/*"],
  },
  "browser-info": {
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
  },
  "remove-data": {
    permissions: [
      "tabs",
      "history",
      "bookmarks",
      "readingList",
      "browsingData",
      "downloads",
    ],
  },
  scripts: { permissions: ["userScripts"] },
  input: {
    permissions: ["tabs", "scripting"],
    origins: ["https://*/*", "http://*/*"],
  },
  screen: {
    permissions: ["desktopCapture", "tabs"],
    origins: ["<all_urls>"],
  },
  held: { permissions: ["contextMenus"] },
};

const toolsOf = (page, id) =>
  [...page.group(id).tree()].filter(
    (element) =>
      element.type === "checkbox" && !element.name.startsWith("group:"),
  );

// A person sees each group with its own switch, how risky it is, and what
// Brave will ask, before turning anything on. Only the platform check is on,
// and it is on whatever the browser holds, since it needs nothing.
test("the page lists each group with its risk and the browser's warning, off except the platform check", async () => {
  const page = await openPage();
  assert.deepEqual(
    page.groupIds(),
    GROUPS_WITH_TOOLS.map(([id]) => id),
  );
  for (const [id, , warning] of GROUPS_WITH_TOOLS) {
    const text = page.group(id).textContent;
    assert.ok(text.includes(warning), `${id} does not say: ${warning}`);
    const box = page.control(`group:${id}`);
    assert.equal(box.checked, id === "platform", id);
    assert.equal(box.disabled, id === "platform", id);
    const risk = page.group(id).children.find(
      (child) => child.className === "note",
    );
    assert.ok(risk.textContent.length > 0, `${id} says nothing of its risk`);
  }
  assert.deepEqual(callsNamed(page, "permissions.request"), []);
});

// Opening a group shows its tools, one switch each, and they cannot be
// switched on while the group is off: a tool switched on in a group that is off
// would run once another group got its permission.
test("a group holds a switch for each of its tools, which can be changed only while it is on", async () => {
  const page = await openPage();
  for (const [id, tools] of GROUPS_WITH_TOOLS) {
    const boxes = toolsOf(page, id);
    assert.deepEqual(
      boxes.map((box) => box.name),
      tools,
    );
    for (const box of boxes) {
      assert.equal(box.disabled, id !== "platform", `${box.name} in ${id}`);
      assert.equal(box.checked, id === "platform", `${box.name} in ${id}`);
    }
    const summary = [...page.group(id).tree()].find(
      (element) => element.tagName === "summary",
    );
    assert.equal(
      summary.textContent,
      tools.length === 1 ? "1 tool" : `${tools.length} tools`,
    );
  }
});

// The group's own words come before the browser's prompt, and some groups get
// no prompt from the browser. Turning the switch asks the person, and asks
// the browser for nothing.
test("turning a group's switch on says what it lets a session do and what comes back, and asks the browser for nothing", async () => {
  const says = {
    "tabs-read": /open tabs/,
    "history-read": /browsing history by words/,
    "bookmarks-read": /Search your bookmarks/,
    "page-read": /text of an open page/,
    "tabs-manage": /pin or unpin/,
    "bookmarks-write": /Add a bookmark at an HTTP or HTTPS address/,
    navigate: /background tab/,
    downloads: /Download one HTTP or HTTPS address into your Downloads folder/,
    cookies: /List the cookies Brave sends/,
    "network-watch": /requests an open page has made/,
    "browser-info": /Keep the computer, or its display, awake/,
    "remove-data": /Close the tab at an exact address/,
    scripts: /Run code a session wrote in the top document of an open page/,
    input: /Click an element, type in a text field/,
    held: /Fetch what you held out/,
    screen: /one picture of the screen or the window you choose/,
  };
  for (const id of Object.keys(REQUESTS)) {
    const page = await openPage();
    const box = page.control(`group:${id}`);
    const confirm = page.part(id, "confirm");
    assert.equal(confirm.hidden, true, id);
    box.checked = true;
    await box.fire("change");
    assert.equal(box.checked, false, `${id} was switched on before it was allowed`);
    assert.equal(confirm.hidden, false, id);
    assert.match(confirm.textContent, /This lets a session: /, id);
    assert.match(confirm.textContent, says[id], id);
    assert.match(confirm.textContent, /What comes back: /, id);
    assert.match(confirm.textContent, /does not give it to the model/, id);
    assert.equal(focused.name, `allow:${id}`, id);
    assert.deepEqual(callsNamed(page, "permissions.request"), [], id);
    assert.equal(page.stored[GROUP_SWITCHES_KEY], undefined, id);
  }
});

// The held group reads no page. What a person holds out is the page's text
// whoever selects it, so the page says that, what is kept and for how long, that
// a fetch takes no argument, and that the model reads the result only through a
// processor or after the person has read it.
test("the page says of the held group what is kept, for how long, that it reads no page and how the model can read it", async () => {
  const page = await openPage();
  const risk = page.group("held").children.find((child) => child.className === "note");
  assert.match(risk.textContent, /reads no page/);
  assert.match(risk.textContent, /include text the page hides/);
  const box = page.control("group:held");
  box.checked = true;
  await box.fire("change");
  const says = page.part("held", "confirm").textContent;
  for (const [pattern, what] of [
    [/Hold this selection, Hold this link and Hold this tab for BraveBot/, "the three entries"],
    [/the extension reads nothing from the page/, "that no page is read"],
    [/Up to 10 items are kept, in memory only, for 30 minutes/, "what is kept"],
    [/forgotten when they are fetched, when this group is turned off and when Brave restarts/, "when they are forgotten"],
    [/takes no argument and names no page/, "that a fetch names no page"],
    [/up to 20,000 characters/, "the size of a selection"],
    [/blocked list is left out/, "the blocked sites"],
    [/The text is the page's, whoever selected it/, "whose text it is"],
    [/only through a processor that answers a question about it, or after you have read the exact text and agreed/, "how the model reads it"],
  ]) {
    assert.match(says, pattern, `the held group does not say ${what}`);
  }
  assert.ok(
    ![...page.group("held").tree()].some((element) => element.className === "scope"),
    "a group that reads no page has no sites to choose",
  );
});

// The input group lets a session act as the person in a page, so what the page
// says of it names the question that stands between a session and the page, and
// the limits that no answer lifts.
// The group that runs code says what the window and the clause say: nothing
// limits the code except that the person reads it. It has no scope to choose,
// since it reaches the allowed sites only, and it tells the person how to turn on
// the browser's own switch, which the extension cannot.
test("the page says of the scripts group that nothing limits the code but reading it, that it reaches the allowed sites only, and how to turn on Allow User Scripts", async () => {
  const page = await openPage();
  const block = page.group("scripts");
  const notes = [...block.tree()]
    .filter((element) => element.className === "note")
    .map((element) => element.textContent);
  assert.match(notes[0], /Nothing limits what the code does except that you read it/);
  assert.match(notes[0], /click and submit its forms, and send what it read to another address/);
  assert.match(
    notes[0],
    /Timers, listeners and observers it sets keep running in the page until the page is closed or reloaded, and each can do the same without asking again/,
  );
  assert.ok(
    ![...block.tree()].some((element) => element.className === "scope"),
    "a group limited to the allowed sites has no scope to choose",
  );
  assert.ok(
    notes.some((note) => /No site is allowed yet, so a session reaches no page/.test(note)),
    "the allowed sites are not said to be the reach",
  );
  const switchNote = notes.find((note) => /Allow User Scripts/.test(note) && /Chrome 138/.test(note));
  assert.ok(switchNote, "the page does not say how to turn on Allow User Scripts");
  assert.match(
    switchNote,
    /In Brave built on Chrome 138 or later, open brave:\/\/extensions, choose Details for BraveBot and turn on Allow User Scripts\. In an older Brave, turn on Developer mode at brave:\/\/extensions\./,
  );
  const box = page.control("group:scripts");
  box.checked = true;
  await box.fire("change");
  const says = page.part("scripts", "confirm").textContent;
  assert.match(says, /every line of the code, with the characters you cannot see written out/);
  assert.match(says, /does not stop it loading a frame, going to an address or submitting a form/);
  assert.match(
    says,
    /The code starts once\. Timers, listeners and observers it sets keep running in the page until the page is closed or reloaded, and each can read and send without asking again\./,
  );
  assert.match(
    says,
    /checks that the browser applied that policy to the world before it asks you, and refuses the call when it did not, which Brave does after ten calls on one page: reload the page and ask again/,
  );
  assert.match(says, /at most 2,000 characters, and a longer one is refused and never cut/);
  assert.match(says, /What the code answered, as text of up to 100,000 characters/);
});

// The browser shows the extension userScripts only once the group's permission is
// held and the person's switch is on, so a group that is on can say whether the
// switch is on, and a group that is off can only say how to turn it on.
test("a scripts group that is on says whether Allow User Scripts is on, by whether the browser lets the extension use userScripts", async () => {
  const on = {
    held: { permissions: ["userScripts"] },
    stored: { [GROUP_SWITCHES_KEY]: { scripts: true } },
    session: { [RUNS_KEY]: { scripts: true } },
  };
  const switchNote = (page) =>
    [...page.group("scripts").tree()]
      .filter((element) => element.className === "note")
      .map((element) => element.textContent)
      .find((note) => /^Allow User Scripts is (on|off) for this extension/.test(note));
  const off = await openPage({
    ...on,
    userScripts: {
      async getWorldConfigurations() {
        throw new Error("'userScripts.getWorldConfigurations' is not available in this context.");
      },
    },
  });
  assert.equal(off.control("group:scripts").checked, true);
  assert.match(
    switchNote(off),
    /^Allow User Scripts is off for this extension, so run_script is refused\. Brave lets an extension run code only while Allow User Scripts is on for it/,
  );
  const absent = await openPage(on);
  assert.match(switchNote(absent), /^Allow User Scripts is off for this extension/);
  const ready = await openPage({
    ...on,
    userScripts: { async getWorldConfigurations() { return []; } },
  });
  assert.equal(
    switchNote(ready),
    "Allow User Scripts is on for this extension, so run_script can run once you allow it.",
  );
  const turnedOff = await openPage({
    userScripts: { async getWorldConfigurations() { return []; } },
  });
  assert.equal(switchNote(turnedOff), undefined, "a group that is off said the switch was on");
});

test("the page says of the input group that each action is allowed in a window, that password, card and file fields are refused and that the clicks are synthetic", async () => {
  const page = await openPage();
  const risk = page.group("input").children.find((child) => child.className === "note");
  assert.match(risk.textContent, /opens a window in Brave/);
  assert.match(risk.textContent, /Allow once or Deny/);
  assert.match(risk.textContent, /as you/);
  const box = page.control("group:input");
  box.checked = true;
  await box.fire("change");
  const says = page.part("input", "confirm").textContent;
  for (const [pattern, what] of [
    [/brings the tab forward, outlines the element/, "the tab and the outline"],
    [/shows the action, the page's address, the element as the page names it and any text to be typed/, "what the window shows"],
    [/Nothing happens until you press Allow once in that window/, "the order"],
    [/Deny, closing the window, or no answer within 60 seconds stops it/, "what stops it"],
    [/only one window is open at a time/, "one window"],
    [/Scrolling and waiting open no window/, "what asks nothing"],
    [/password field, a payment card field and a file chooser are never acted on/, "the refused fields"],
    [/synthetic: the page can tell them from yours/, "synthetic events"],
    [/nothing of the page or the element/, "what comes back"],
  ]) {
    assert.match(says, pattern, `the input group does not say ${what}`);
  }
  assert.equal(CONFIRMATION_WAIT_MS, 60_000, "the page says 60 seconds");
  // The four tools that act on an element each say where the answer is given.
  const labelled = page.group("input").textContent.match(/Allow once in the window it opens/g);
  assert.equal(labelled?.length, 4);
});

// A number names an element by a line of a list the person read, so what the
// page says of the two groups names the list, who it is for, what is left out
// of it and how long it is kept.
test("the page says of page-read that it numbers the controls of a page for the person and keeps no field's value, and of input that an element may be named by that number", async () => {
  const page = await openPage();
  for (const id of ["page-read", "input"]) {
    const box = page.control(`group:${id}`);
    box.checked = true;
    await box.fire("change");
  }
  const reads = page.part("page-read", "confirm").textContent;
  for (const [pattern, what] of [
    [/number its buttons, links and fields so that you can tell the model which one to act on/, "what the list is for"],
    [/up to 40 numbered controls with the role, name and destination of each and no value a field holds/, "what the list holds"],
    [/kept in memory for 5 minutes/, "how long it is kept"],
  ]) {
    assert.match(reads, pattern, `page-read does not say ${what}`);
  }
  assert.match(
    page.group("page-read").textContent,
    /Number the elements of an open page[\s\S]*It comes back to you and not to the model/,
  );
  const acts = page.part("input", "confirm").textContent;
  assert.match(acts, /or by its number in the last list of the page that you read and gave the model/);
  // The three tools that act on an element say so, and the other three do not.
  const named = page.group("input").textContent.match(/a number from the list of the page/g);
  assert.equal(named?.length, 3);
});

// The browser accepts the request only from a click. The service worker has no
// click, so the page asks, and asks as the first thing the click does: a save
// first would spend the time the browser allows after a click.
test("confirming asks the browser for the group's permissions from that click, and then turns on the group and its tools", async () => {
  for (const [id, request] of Object.entries(REQUESTS)) {
    const page = await openPage();
    const box = page.control(`group:${id}`);
    box.checked = true;
    await box.fire("change");
    const before = page.calls.length;
    const clicking = page.control(`allow:${id}`).fire("click");
    assert.deepEqual(
      page.calls.slice(before),
      [["permissions.request", request]],
      `${id} did not ask the browser first`,
    );
    await clicking;

    assert.deepEqual(page.stored[GROUP_SWITCHES_KEY], { [id]: true }, id);
    const tools = page.stored[SETTINGS_KEY];
    for (const [other, names] of GROUPS_WITH_TOOLS) {
      for (const name of names) {
        const expected = other === id || other === "platform";
        assert.equal(tools[name] === true, expected, `${name} after ${id}`);
      }
    }
    assert.equal(box.checked, true, id);
    assert.equal(page.part(id, "confirm").hidden, true, id);
    for (const tool of toolsOf(page, id)) {
      assert.equal(tool.checked, true, tool.name);
      assert.equal(tool.disabled, false, tool.name);
    }
  }
});

test("a request the person declines leaves the group off and says so", async () => {
  const page = await openPage({ decision: false });
  const box = page.control("group:tabs-read");
  box.checked = true;
  await box.fire("change");
  await page.control("allow:tabs-read").fire("click");
  assert.equal(box.checked, false);
  assert.equal(page.stored[GROUP_SWITCHES_KEY], undefined);
  assert.equal(page.stored[SETTINGS_KEY], undefined);
  assert.match(
    page.group("tabs-read").textContent,
    /Not turned on: Brave did not grant what the group needs\./,
  );
  assert.equal(page.part("tabs-read", "confirm").hidden, false);
  for (const tool of toolsOf(page, "tabs-read")) {
    assert.equal(tool.checked, false, tool.name);
  }
});

// An organization's policy (ExtensionSettings blocked_permissions) makes the
// request fail with a message of Brave's. The group is drawn as blocked by the
// organization, in its own row, and its switch stays off. The page does not ask
// again: it is opened again to try after the policy changes.
const POLICY = "Permissions are blocked by enterprise policy.";

test("a request an organization's policy blocks draws the group as blocked by the organization, off, and the page asks once", async () => {
  const page = await openPage({ decision: new Error(POLICY) });
  const box = page.control("group:history-read");
  box.checked = true;
  await box.fire("change");
  await page.control("allow:history-read").fire("click");
  await flush();
  const row = page.group("history-read");
  assert.match(
    page.part("history-read", "blocked").textContent,
    /^Blocked by your organization: Brave says permissions are blocked by enterprise policy, so this group stays off\./,
  );
  assert.doesNotMatch(row.textContent, /Not turned on/);
  assert.equal(box.checked, false);
  assert.equal(box.disabled, true);
  assert.equal(page.part("history-read", "confirm").hidden, true);
  assert.equal(page.stored[GROUP_SWITCHES_KEY], undefined);
  assert.equal(page.stored[SETTINGS_KEY], undefined);

  // The page redraws for an event of the browser or a saved switch and does not
  // ask again, and another group is not drawn as blocked.
  await page.permissions.revoke({ permissions: ["bookmarks"] });
  await flush();
  assert.equal(callsNamed(page, "permissions.request").length, 1);
  assert.notEqual(page.part("history-read", "blocked").textContent, "");
  assert.equal(page.part("tabs-read", "blocked").textContent, "");
  assert.equal(page.control("group:tabs-read").disabled, false);

  // A page opened later asks again when the person turns the group on.
  const later = await openPage({ decision: true });
  assert.equal(later.part("history-read", "blocked").textContent, "");
  assert.equal(later.control("group:history-read").disabled, false);
});

// Only that message is the organization's. A request that fails for another
// reason is shown in Brave's words, the group is not drawn as blocked, and the
// person can turn it on again.
test("a request that fails for another reason is shown as Brave words it, and the group is not drawn as blocked", async () => {
  const page = await openPage({
    decision: new Error("This function must be called during a user gesture"),
  });
  const box = page.control("group:history-read");
  box.checked = true;
  await box.fire("change");
  await page.control("allow:history-read").fire("click");
  await flush();
  assert.match(
    page.group("history-read").textContent,
    /Not turned on: This function must be called during a user gesture/,
  );
  assert.equal(page.part("history-read", "blocked").textContent, "");
  assert.doesNotMatch(page.group("history-read").textContent, /organization/);
  assert.equal(box.disabled, false);
  assert.equal(page.part("history-read", "confirm").hidden, false);
  assert.equal(page.stored[GROUP_SWITCHES_KEY], undefined);

  // The same click can be made again, and Brave is asked again.
  await page.control("allow:history-read").fire("click");
  assert.equal(callsNamed(page, "permissions.request").length, 2);
  assert.equal(page.part("history-read", "blocked").textContent, "");
});

test("cancelling the confirmation asks the browser for nothing and leaves the group off", async () => {
  const page = await openPage();
  const box = page.control("group:bookmarks-read");
  box.checked = true;
  await box.fire("change");
  await page.control("cancel:bookmarks-read").fire("click");
  assert.equal(page.part("bookmarks-read", "confirm").hidden, true);
  assert.equal(box.checked, false);
  assert.equal(focused, box);
  assert.deepEqual(callsNamed(page, "permissions.request"), []);
  assert.equal(page.stored[GROUP_SWITCHES_KEY], undefined);
});

// The state of a page where tabs-read and page-read are on.
function twoOn() {
  return {
    held: allOptional(),
    stored: {
      [GROUP_SWITCHES_KEY]: { "tabs-read": true, "page-read": true },
      [SETTINGS_KEY]: {
        list_tabs: true,
        list_frames: true,
        read_page: true,
        search_history: false,
      },
    },
  };
}

// The permissions page-read shares with tabs-read stay, since tabs-read still
// needs them. Taking everything page-read lists would end tabs-read.
test("turning a group off turns off its tools and takes back only what no group still on needs", async () => {
  const page = await openPage(twoOn());
  const box = page.control("group:page-read");
  assert.equal(box.checked, true);
  box.checked = false;
  await box.fire("change");

  assert.deepEqual(callsNamed(page, "permissions.remove"), [
    { permissions: ["scripting"], origins: ["<all_urls>"] },
  ]);
  assert.deepEqual(page.stored[GROUP_SWITCHES_KEY], { "tabs-read": true });
  assert.equal(page.stored[SETTINGS_KEY].read_page, false);
  assert.equal(page.stored[SETTINGS_KEY].list_tabs, true);
  assert.equal(box.checked, false);
  assert.equal(page.control("read_page").disabled, true);
  assert.equal(page.control("group:tabs-read").checked, true);

  const tabs = page.control("group:tabs-read");
  tabs.checked = false;
  await tabs.fire("change");
  assert.deepEqual(callsNamed(page, "permissions.remove").at(-1), {
    permissions: ["tabs", "tabGroups", "webNavigation", "sessions"],
  });
  assert.deepEqual(page.stored[GROUP_SWITCHES_KEY], {});
});

test("a tool switch in an open group saves that tool and leaves the others", async () => {
  const page = await openPage(twoOn());
  const frames = page.control("list_frames");
  assert.equal(frames.checked, true);
  frames.checked = false;
  await frames.fire("change");
  assert.equal(page.stored[SETTINGS_KEY].list_frames, false);
  assert.equal(page.stored[SETTINGS_KEY].list_tabs, true);
  assert.equal(page.stored[SETTINGS_KEY].read_page, true);
  assert.deepEqual(page.stored[GROUP_SWITCHES_KEY], {
    "tabs-read": true,
    "page-read": true,
  });
  assert.equal(page.control("group:tabs-read").checked, true);
});

// A permission taken back in Brave's extension settings, or granted from
// another options page, changes what is held. The page draws it without being
// reloaded.
test("the page follows a permission taken back in the browser's settings", async () => {
  const page = await openPage(twoOn());
  assert.equal(page.control("group:tabs-read").checked, true);
  assert.equal(page.control("group:page-read").checked, true);
  assert.equal(page.control("read_page").disabled, false);

  await page.permissions.revoke({ permissions: ["scripting"] });
  await flush();
  assert.equal(page.control("group:page-read").checked, false);
  assert.equal(page.control("read_page").disabled, true);
  assert.equal(page.control("group:tabs-read").checked, true);
  assert.equal(page.control("list_tabs").disabled, false);
});

test("the page follows a permission granted while it is open", async () => {
  const page = await openPage({
    stored: {
      [GROUP_SWITCHES_KEY]: { "history-read": true },
      [SETTINGS_KEY]: { search_history: true },
    },
  });
  assert.equal(page.control("group:history-read").checked, false);
  assert.equal(page.control("search_history").disabled, true);

  // The group is held as a whole: history alone leaves it off.
  await page.permissions.request({ permissions: ["history"] });
  await flush();
  assert.equal(page.control("group:history-read").checked, false);
  assert.equal(page.control("search_history").disabled, true);

  await page.permissions.request({ permissions: ["topSites", "sessions"] });
  await flush();
  assert.equal(page.control("group:history-read").checked, true);
  assert.equal(page.control("search_history").disabled, false);
});

// A permission granted in one options page is announced before that page has
// saved the group's switch, so a page that only listens for permissions draws
// the switch as it was.
test("the page follows a switch another options page saved", async () => {
  const page = await openPage({
    held: { permissions: ["bookmarks", "readingList"] },
  });
  assert.equal(page.control("group:bookmarks-read").checked, false);
  await chrome.storage.local.set({
    [GROUP_SWITCHES_KEY]: { "bookmarks-read": true },
    [SETTINGS_KEY]: { search_bookmarks: true },
  });
  await flush();
  assert.equal(page.control("group:bookmarks-read").checked, true);
  assert.equal(page.control("search_bookmarks").checked, true);
  assert.equal(page.control("search_bookmarks").disabled, false);
});

// The switches say they are on only when the browser holds what the group
// needs, so a saved switch is not a grant.
test("a group saved as on is drawn off while the browser holds none of what it needs", async () => {
  const page = await openPage({
    stored: { [GROUP_SWITCHES_KEY]: { "tabs-read": true } },
  });
  assert.equal(page.control("group:tabs-read").checked, false);
});

test("a switch that cannot be saved is said, and the page shows what is saved", async () => {
  const page = await openPage(twoOn());
  const frames = page.control("list_frames");
  frames.checked = false;
  page.next.failure = new Error("storage is full");
  await frames.fire("change");
  assert.equal(
    page.element("groups-status").textContent,
    "Not saved: storage is full",
  );
  assert.equal(frames.checked, true);
  assert.equal(page.stored[SETTINGS_KEY].list_frames, true);
});

const NEWS = "https://news.example";
const INTRANET = "http://intranet:8080";

// The groups each preset turns on, and what Brave is asked for them together.
// Read and Browse ask for the sites that are allowed, which are the one site
// the pages below allow.
const PRESET_ON = {
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
};
const LOOK_PERMISSIONS = [
  "tabs",
  "tabGroups",
  "webNavigation",
  "sessions",
  "history",
  "topSites",
  "bookmarks",
  "readingList",
];
const PRESET_REQUESTS = {
  look: { permissions: LOOK_PERMISSIONS },
  read: {
    permissions: [...LOOK_PERMISSIONS, "scripting"],
    origins: [`${NEWS}/*`],
  },
};
// Browse asks for what Read does and `search`, which only navigate lists.
PRESET_REQUESTS.browse = {
  permissions: [...PRESET_REQUESTS.read.permissions, "search"],
  origins: PRESET_REQUESTS.read.origins,
};
const NEWS_ALLOWED = { [ALLOWED_SITES_KEY]: [NEWS] };

const choose = async (page, name) => page.control(`preset:${name}`).fire("click");
const confirmOf = (page) => page.element("preset-confirm");

test("the page offers Off, Look, Read and Browse, each with a line on what it allows", async () => {
  const page = await openPage();
  const list = page.element("presets");
  const rows = list.children.map((row) => row.textContent);
  assert.equal(rows.length, 4);
  assert.match(rows[0], /^Off.*Only the platform check is on/);
  assert.match(rows[1], /^Look.*No page is read/);
  assert.match(rows[2], /^Read.*text of pages you have open/);
  assert.match(rows[3], /^Browse.*bookmarks and reading list entries can be added.*opened in a new tab/);
  for (const name of ["off", "look", "read", "browse"]) {
    assert.equal(page.control(`preset:${name}`).type, "button", name);
  }
  assert.equal(confirmOf(page).hidden, true);
  assert.deepEqual(callsNamed(page, "permissions.request"), []);
});

// The person reads what each group does before Brave's one prompt, and the
// prompt is not asked for until they say so.
test("choosing a preset says what each of its groups lets a session do, what Brave will ask for them together, and asks the browser for nothing", async () => {
  const says = {
    "tabs-read": /open tabs/,
    "history-read": /browsing history by words/,
    "bookmarks-read": /Search your bookmarks/,
    "page-read": /text of an open page/,
    "tabs-manage": /pin or unpin/,
    "bookmarks-write": /Add a bookmark at an HTTP or HTTPS address/,
    navigate: /background tab/,
  };
  const prompts = {
    look: "Brave asks you to allow: Read and change your browsing history on all your signed-in devices; View and manage your tab groups; Read and change your bookmarks; Read and change entries in the reading list.",
    read: "Brave asks you to allow: Read and change your browsing history on all your signed-in devices; View and manage your tab groups; Read and change your bookmarks; Read and change entries in the reading list; Read and change your data on news.example.",
  };
  prompts.browse = prompts.read;
  for (const [name, ids] of Object.entries(PRESET_ON)) {
    const page = await openPage({ stored: NEWS_ALLOWED });
    await choose(page, name);
    const confirm = confirmOf(page);
    assert.equal(confirm.hidden, false, name);
    const text = confirm.textContent;
    for (const id of ids) {
      assert.match(text, says[id], `${name}: ${id}`);
    }
    for (const id of Object.keys(says).filter((id) => !ids.includes(id))) {
      assert.doesNotMatch(text, says[id], `${name}: ${id} is not in it`);
    }
    assert.equal(text.match(/What comes back: /g).length, ids.length, name);
    assert.match(text, /does not give it to the model/, name);
    assert.ok(text.includes(prompts[name]), `${name}: ${text}`);
    assert.equal(text.split("Brave asks you to allow").length, 2, name);
    // The page draws every group that reads pages on all sites here, and a
    // preset limits the ones it turns on to the allowed sites.
    assert.equal(
      text.includes("This limits Page text to the allowed sites, and gives back the access to all sites."),
      ids.includes("page-read"),
      name,
    );
    assert.equal(focused.name, "apply-preset", name);
    assert.deepEqual(callsNamed(page, "permissions.request"), [], name);
    assert.equal(page.stored[GROUP_SWITCHES_KEY], undefined, name);
    assert.equal(page.control("group:tabs-read").checked, false, name);
  }
});

// The confirmation is not modal: the allowed sites can be saved, and a group
// turned on or off, in this page or another, before the click on Turn on. The
// request names what the page holds then, so the lines and the list of groups
// turned off follow it.
test("a preset's confirmation is drawn again when the sites or the groups change before it is answered, and the request asks for what it shows", async () => {
  const page = await openPage({
    held: allOptional(),
    stored: { ...NEWS_ALLOWED, [SITE_SCOPES_KEY]: {}, [GROUP_SWITCHES_KEY]: { "cookies": true } },
  });
  await choose(page, "read");
  const text = () => confirmOf(page).textContent;
  assert.ok(text().includes("Read and change your data on news.example."));
  assert.match(text(), /This turns off: .*Cookies/);
  assert.doesNotMatch(text(), /intranet/);
  assert.doesNotMatch(text(), /Downloads/);

  await chrome.storage.local.set({
    [ALLOWED_SITES_KEY]: [NEWS, INTRANET],
    [GROUP_SWITCHES_KEY]: { cookies: true, downloads: true },
  });
  await flush();
  assert.equal(confirmOf(page).hidden, false);
  assert.ok(text().includes("Read and change your data on intranet and news.example."));
  assert.match(text(), /This turns off: .*Cookies.*Downloads|This turns off: .*Downloads.*Cookies/);
  assert.equal(text().split("Brave asks you to allow").length, 2);
  assert.equal(focused.name, "apply-preset");

  // Cancelling closes it, and a change after that draws nothing.
  await page.control("cancel-preset").fire("click");
  assert.equal(confirmOf(page).hidden, true);
  const closed = text();
  await chrome.storage.local.set({ [ALLOWED_SITES_KEY]: [NEWS] });
  await flush();
  assert.equal(confirmOf(page).hidden, true);
  assert.equal(text(), closed);

  // What is requested is what the last drawing named.
  await chrome.storage.local.set({ [ALLOWED_SITES_KEY]: [NEWS, INTRANET] });
  await flush();
  await choose(page, "read");
  await chrome.storage.local.set({ [ALLOWED_SITES_KEY]: [NEWS] });
  await flush();
  assert.ok(text().includes("Read and change your data on news.example."));
  const before = page.calls.length;
  const clicking = page.control("apply-preset").fire("click");
  const [asked] = page.calls.slice(before);
  assert.deepEqual(asked[1].origins, [`${NEWS}/*`]);
  await clicking;
});

// Brave takes the request only from a click, and takes one request as one
// prompt. The groups are then on with their tools, and the others are as they
// were.
test("confirming a preset makes one request as the first call of the click, and then turns on its groups and their tools", async () => {
  for (const [name, ids] of Object.entries(PRESET_ON)) {
    const page = await openPage({ stored: NEWS_ALLOWED });
    await choose(page, name);
    const before = page.calls.length;
    const clicking = page.control("apply-preset").fire("click");
    assert.deepEqual(
      page.calls.slice(before),
      [["permissions.request", PRESET_REQUESTS[name]]],
      `${name} did not ask the browser first, or asked more than once`,
    );
    await clicking;

    assert.deepEqual(Object.keys(page.stored[GROUP_SWITCHES_KEY]), ids, name);
    for (const [id, tools] of GROUPS_WITH_TOOLS) {
      assert.equal(
        page.control(`group:${id}`).checked,
        id === "platform" || ids.includes(id),
        `${name}: ${id}`,
      );
      for (const tool of tools) {
        assert.equal(
          page.stored[SETTINGS_KEY][tool] === true,
          id === "platform" || ids.includes(id),
          `${name}: ${tool}`,
        );
      }
    }
    assert.equal(confirmOf(page).hidden, true, name);
    assert.match(page.element("presets-status").textContent, new RegExp(`^${name}`, "i"));
    assert.equal(callsNamed(page, "permissions.request").length, 1, name);
  }
});

// A scope of all sites that the person chose earlier does not carry into the
// preset, which says Read is the allowed sites. The page says so before it
// asks, asks for the allowed sites only, and shows the scope it left.
test("a preset limits a group that reads pages to the allowed sites, says so, and shows the scope it left", async () => {
  const everySite = ["<all_urls>"];
  const page = await openPage({
    held: { permissions: PAGE_PERMISSIONS, origins: everySite },
    stored: { [GROUP_SWITCHES_KEY]: { "page-read": true }, ...NEWS_ALLOWED },
  });
  assert.equal(radio(page, "page-read", "all").checked, true);
  await choose(page, "read");
  assert.match(
    confirmOf(page).textContent,
    /This limits Page text to the allowed sites, and gives back the access to all sites\./,
  );
  assert.equal(radio(page, "page-read", "all").checked, true, "nothing changed yet");

  const before = page.calls.length;
  const clicking = page.control("apply-preset").fire("click");
  assert.deepEqual(page.calls.slice(before), [
    ["permissions.request", PRESET_REQUESTS.read],
  ]);
  await clicking;
  assert.deepEqual(page.stored[SITE_SCOPES_KEY], {
    cookies: SCOPE_ALL,
    "network-watch": SCOPE_ALL,
    input: SCOPE_ALL,
    screen: SCOPE_ALL,
  });
  assert.equal(radio(page, "page-read", "allowed").checked, true);
  assert.equal(radio(page, "page-read", "all").checked, false);
  assert.deepEqual(callsNamed(page, "permissions.remove"), [{ origins: everySite }]);

  // Where the group is on the allowed sites already, the preset says nothing.
  const limited = await openPage({ stored: { [SITE_SCOPES_KEY]: {} } });
  await choose(limited, "read");
  assert.doesNotMatch(confirmOf(limited).textContent, /This limits/);
});

test("a preset says which groups it turns off before it does, and turns them off with their permissions", async () => {
  const page = await openPage(twoOn());
  await choose(page, "look");
  assert.match(confirmOf(page).textContent, /This turns off: Page text\./);
  await page.control("apply-preset").fire("click");

  assert.deepEqual(Object.keys(page.stored[GROUP_SWITCHES_KEY]), PRESET_ON.look);
  assert.equal(page.stored[SETTINGS_KEY].read_page, false);
  assert.equal(page.control("group:page-read").checked, false);
  assert.equal(page.control("read_page").disabled, true);
  assert.equal(page.control("group:history-read").checked, true);
  assert.deepEqual(callsNamed(page, "permissions.remove"), [
    { permissions: ["scripting"], origins: ["<all_urls>"] },
  ]);

  const fresh = await openPage();
  await choose(fresh, "read");
  assert.doesNotMatch(confirmOf(fresh).textContent, /This turns off/);
});

// Nothing is turned on and nothing needs a prompt, so the person is not asked
// to confirm turning things off, as for a group's own switch.
test("Off turns every group off at once, asks the browser for nothing and shows no confirmation", async () => {
  const page = await openPage(twoOn());
  await choose(page, "off");
  assert.deepEqual(callsNamed(page, "permissions.request"), []);
  assert.equal(confirmOf(page).hidden, true);
  assert.deepEqual(page.stored[GROUP_SWITCHES_KEY], {});
  assert.deepEqual(callsNamed(page, "permissions.remove"), [allOptional()]);
  assert.equal(page.control("group:tabs-read").checked, false);
  assert.equal(page.control("group:page-read").checked, false);
  assert.equal(page.control("group:platform").checked, true);
  assert.equal(page.control("get_platform_info").checked, true);
  assert.match(page.element("presets-status").textContent, /^Off is applied/);
});

test("a preset the person declines or the browser refuses to ask for leaves everything as it was and says so", async () => {
  const declined = await openPage({ decision: false });
  await choose(declined, "read");
  await declined.control("apply-preset").fire("click");
  assert.equal(declined.stored[GROUP_SWITCHES_KEY], undefined);
  assert.equal(declined.stored[SETTINGS_KEY], undefined);
  assert.match(
    declined.element("presets-status").textContent,
    /Not applied: Brave did not grant what the groups need\./,
  );
  assert.equal(confirmOf(declined).hidden, false);
  assert.equal(declined.control("group:tabs-read").checked, false);

  const blocked = await openPage({
    decision: new Error("Permissions are blocked by enterprise policy."),
  });
  await choose(blocked, "look");
  await blocked.control("apply-preset").fire("click");
  assert.match(
    blocked.element("presets-status").textContent,
    /Permissions are blocked by enterprise policy\./,
  );
  assert.equal(blocked.stored[GROUP_SWITCHES_KEY], undefined);
  assert.equal(blocked.control("group:history-read").checked, false);
});

test("cancelling a preset asks the browser for nothing and changes nothing", async () => {
  const page = await openPage();
  await choose(page, "browse");
  await page.control("cancel-preset").fire("click");
  assert.equal(confirmOf(page).hidden, true);
  assert.equal(focused, page.control("preset:browse"));
  assert.deepEqual(callsNamed(page, "permissions.request"), []);
  assert.equal(page.stored[GROUP_SWITCHES_KEY], undefined);
  assert.equal(page.control("group:navigate").checked, false);
});

// The switches are saved before anything is taken back, so the page shows the
// preset on even when the browser would not give a permission back.
test("a preset whose permissions cannot all be taken back says so and shows what is on", async () => {
  const page = await openPage(twoOn());
  page.permissions.remove = async () => {
    throw new Error("the browser would not remove it");
  };
  await choose(page, "look");
  await page.control("apply-preset").fire("click");
  assert.match(
    page.element("presets-status").textContent,
    /Brave reported an error: the browser would not remove it\. The switches below show what is on\./,
  );
  assert.deepEqual(Object.keys(page.stored[GROUP_SWITCHES_KEY]), PRESET_ON.look);
});

// The sites a group that reads pages may reach. The list is saved by a click,
// since the browser takes the request for the sites only from one.
const PAGE_PERMISSIONS = ["tabs", "scripting", "webNavigation"];
const radio = (page, id, value) => {
  const found = [...page.group(id).tree()].find(
    (element) => element.type === "radio" && element.value === value,
  );
  assert.ok(found, `${id} has no scope ${value}`);
  return found;
};
const allowedField = (page) => page.element("allowed-sites");
const allowedStatus = (page) => page.element("allowed-status");
const saveAllowed = (page) => page.element("save-allowed").fire("click");

test("the page has an Allowed sites list beside Blocked sites, which shows the saved sites", async () => {
  const page = await openPage({ stored: { [ALLOWED_SITES_KEY]: [NEWS, INTRANET] } });
  assert.equal(allowedField(page).value, `${NEWS}\n${INTRANET}`);
  const markup = readFileSync(new URL("../options.html", import.meta.url), "utf8");
  assert.ok(markup.indexOf('id="allowed-heading"') < markup.indexOf('id="blocked-heading"'));
  assert.match(markup, /<h2 id="allowed-heading">Allowed sites<\/h2>/);
  assert.match(markup, /<h2 id="blocked-heading">Blocked sites<\/h2>/);
  assert.match(markup, /also under\s+Blocked sites is blocked/);
});

test("typing in the allowed sites saves nothing and asks the browser for nothing", async () => {
  const page = await openPage({ stored: { [ALLOWED_SITES_KEY]: [NEWS] } });
  allowedField(page).value = `${NEWS}\n${INTRANET}`;
  await allowedField(page).fire("input");
  assert.deepEqual(page.stored[ALLOWED_SITES_KEY], [NEWS]);
  assert.match(allowedStatus(page).textContent, /^Not saved yet\. Choose Save allowed sites/);
  assert.deepEqual(page.calls, []);
});

// The browser accepts the request only from a click, so it is the first call of
// the click and the list is saved after it.
test("saving the allowed sites asks the browser for every site as the first call of the click, and then saves them", async () => {
  const page = await openPage({ stored: { [ALLOWED_SITES_KEY]: [NEWS] } });
  allowedField(page).value = `${NEWS}/front\n${INTRANET}/\n`;
  const before = page.calls.length;
  const clicking = saveAllowed(page);
  assert.deepEqual(page.calls.slice(before), [
    ["permissions.request", { origins: [`${NEWS}/*`, `${INTRANET}/*`] }],
  ]);
  await clicking;
  assert.deepEqual(page.stored[ALLOWED_SITES_KEY], [NEWS, INTRANET]);
  assert.equal(allowedStatus(page).textContent, "Saved.");
  assert.equal(allowedField(page).value, `${NEWS}\n${INTRANET}`);
  assert.equal(page.element("save-allowed").disabled, false);
  assert.deepEqual(callsNamed(page, "permissions.remove"), []);
});

test("a site the person does not grant, or the browser refuses to ask for, is not saved", async () => {
  const declined = await openPage({
    decision: false,
    stored: { [ALLOWED_SITES_KEY]: [NEWS] },
  });
  allowedField(declined).value = INTRANET;
  await saveAllowed(declined);
  assert.deepEqual(declined.stored[ALLOWED_SITES_KEY], [NEWS]);
  assert.match(
    allowedStatus(declined).textContent,
    /^Not saved: Brave did not grant access to the sites\. The list saved before still applies\./,
  );
  assert.equal(allowedField(declined).value, INTRANET);

  const blocked = await openPage({
    decision: new Error("Permissions are blocked by enterprise policy."),
  });
  allowedField(blocked).value = INTRANET;
  await saveAllowed(blocked);
  assert.equal(blocked.stored[ALLOWED_SITES_KEY], undefined);
  assert.match(
    allowedStatus(blocked).textContent,
    /^Not saved: Permissions are blocked by enterprise policy\./,
  );
  assert.equal(blocked.element("save-allowed").disabled, false);
});

test("a line that is not an address stops the save, is named, and asks the browser for nothing", async () => {
  const page = await openPage({ stored: { [ALLOWED_SITES_KEY]: [NEWS] } });
  allowedField(page).value = `${INTRANET}\nnews.example\nhttps://*.bank.example`;
  await saveAllowed(page);
  assert.deepEqual(page.stored[ALLOWED_SITES_KEY], [NEWS]);
  assert.match(
    allowedStatus(page).textContent,
    /^Not saved: news\.example, https:\/\/\*\.bank\.example\. A site is one http:\/\/ or https:\/\/ address/,
  );
  assert.match(allowedStatus(page).textContent, /list saved before still applies/);
  assert.deepEqual(callsNamed(page, "permissions.request"), []);
});

test("a site taken off the list has its access given back, and a failure to do so is said", async () => {
  const held = { permissions: PAGE_PERMISSIONS, origins: [`${NEWS}/*`, `${INTRANET}/*`] };
  const page = await openPage({
    held,
    stored: { [ALLOWED_SITES_KEY]: [NEWS, INTRANET] },
  });
  allowedField(page).value = NEWS;
  await saveAllowed(page);
  assert.deepEqual(page.stored[ALLOWED_SITES_KEY], [NEWS]);
  assert.deepEqual(callsNamed(page, "permissions.remove"), [{ origins: [`${INTRANET}/*`] }]);

  page.permissions.remove = async () => {
    throw new Error("the browser would not remove it");
  };
  allowedField(page).value = "";
  await saveAllowed(page);
  assert.deepEqual(page.stored[ALLOWED_SITES_KEY], []);
  assert.equal(
    allowedStatus(page).textContent,
    "Saved, but Brave did not take back access to a site you removed: the browser would not remove it",
  );
});

test("text typed while the allowed sites are saved is not replaced, and a list that cannot be read is said", async () => {
  const page = await openPage();
  let release;
  page.next.held = new Promise((resolve) => {
    release = resolve;
  });
  allowedField(page).value = `${NEWS}/front`;
  const saving = saveAllowed(page);
  allowedField(page).value = `${NEWS}/front\n${INTRANET}`;
  release();
  await saving;
  assert.deepEqual(page.stored[ALLOWED_SITES_KEY], [NEWS]);
  assert.equal(allowedField(page).value, `${NEWS}/front\n${INTRANET}`);

  const unreadable = await openPage({ stored: { [ALLOWED_SITES_KEY]: NEWS } });
  assert.equal(allowedField(unreadable).value, "");
  assert.match(allowedStatus(unreadable).textContent, /cannot be read/);
});

// Only a group that reads pages has a scope to choose. Allowed sites is the one
// that is chosen at first, and the page says what that reaches.
test("a group that reads pages offers two scopes, the allowed sites chosen at first", async () => {
  const page = await openPage({ stored: { [SITE_SCOPES_KEY]: {} } });
  assert.equal(radio(page, "page-read", "allowed").checked, true);
  assert.equal(radio(page, "page-read", "all").checked, false);
  const block = page.group("page-read").textContent;
  assert.match(block, /Only the allowed sites/);
  assert.match(block, /All sites except the blocked ones/);
  assert.match(block, /No site is allowed yet, so a session reaches no page until you add one under Allowed sites\./);
  for (const id of ["tabs-read", "history-read", "bookmarks-read", "navigate", "platform"]) {
    assert.ok(
      ![...page.group(id).tree()].some(
        (e) => e.type === "radio" && e.name.startsWith("scope:"),
      ),
      id,
    );
  }

  const some = await openPage({
    stored: { [SITE_SCOPES_KEY]: {}, [ALLOWED_SITES_KEY]: [NEWS, INTRANET] },
  });
  assert.match(some.group("page-read").textContent, /A session reaches only the 2 sites under Allowed sites\./);
  const all = await openPage({ stored: { [SITE_SCOPES_KEY]: { "page-read": "all" } } });
  assert.equal(radio(all, "page-read", "all").checked, true);
  assert.match(all.group("page-read").textContent, /Every site is within reach except the ones under Blocked sites\./);
});

// The group that a second extension serves is limited to the allowed sites and
// has no scope to choose, so the page draws no choice for it, and a scope saved
// for it (the page never writes one) changes nothing it says. It lists no
// permission, so Brave is asked for nothing and says nothing for it.
test("the input-trusted group has no choice of scope, says it reaches only the allowed sites, and shows no line from Brave", async () => {
  const none = await openPage({
    stored: { [SITE_SCOPES_KEY]: { "input-trusted": "all" } },
  });
  const block = none.group("input-trusted").textContent;
  assert.ok(
    ![...none.group("input-trusted").tree()].some(
      (e) => e.type === "radio" && e.name.startsWith("scope:"),
    ),
    "the page offers a scope for input-trusted",
  );
  assert.match(block, /No site is allowed yet, so a session reaches no page until you add one under Allowed sites\./);
  assert.doesNotMatch(block, /All sites except the blocked ones|Every site is within reach/);
  assert.match(block, /Brave shows no prompt of its own\./);

  const some = await openPage({
    stored: { [SITE_SCOPES_KEY]: {}, [ALLOWED_SITES_KEY]: [NEWS, INTRANET] },
  });
  const two = some.group("input-trusted").textContent;
  assert.match(two, /A session reaches only the 2 sites under Allowed sites\./);
  assert.match(two, /Brave shows no prompt of its own\./);
  assert.doesNotMatch(two, /Read and change your data on/);
});

// The group acts in a page as the person, through another extension, so what the
// page says of it names that extension, the window that stands between a session
// and the page, and the limits no answer lifts.
test("the page says of the input-trusted group that another extension does it, that each action is allowed in a window of that extension and what it never touches", async () => {
  const page = await openPage();
  const risk = page.group("input-trusted").children.find((child) => child.className === "note");
  assert.match(risk.textContent, /BraveBot Trusted Input extension/);
  assert.match(risk.textContent, /read and change data on all websites/);
  assert.match(risk.textContent, /Allow once or Deny/);
  assert.match(risk.textContent, /cannot tell from yours/);
  assert.match(risk.textContent, /only the sites you allow/);
  const box = page.control("group:input-trusted");
  box.checked = true;
  await box.fire("change");
  const says = page.part("input-trusted", "confirm").textContent;
  for (const [pattern, what] of [
    [/needs that extension to be installed and turned on in Brave/, "that it is needed"],
    [/Brave showed you its own install prompt then and shows none now/, "where the prompt was"],
    [/Nothing happens until you press Allow once in that window/, "the order"],
    [/Deny, closing the window, or no answer within 60 seconds stops it/, "what stops it"],
    [/only one window is open at a time/, "one window"],
    [/password field, a payment card field and a file chooser are never acted on/, "the refused fields"],
    [/asked how to answer it/, "the dialog"],
    [/nothing of the page or the elements/, "what comes back"],
    [/there is no choice of all sites for this group/, "the scope"],
    [/How long it stays on/, "the lifetime"],
  ]) {
    assert.match(says, pattern, `the input-trusted group does not say ${what}`);
  }
  const labelled = page
    .group("input-trusted")
    .textContent.match(/Allow once in the window the BraveBot Trusted Input extension opens/g);
  assert.equal(labelled?.length, 5);
});

// A group that lists no permission asks the browser for nothing when it is turned
// on, and it is on until Brave restarts unless the person chooses another time.
test("turning input-trusted on asks the browser for nothing, and saves the group and its five tools on until Brave restarts", async () => {
  const page = await openPage();
  const box = page.control("group:input-trusted");
  box.checked = true;
  await box.fire("change");
  await page.control("allow:input-trusted").fire("click");
  assert.deepEqual(callsNamed(page, "permissions.request"), []);
  assert.deepEqual(page.stored[GROUP_SWITCHES_KEY], { "input-trusted": true });
  const tools = page.stored[SETTINGS_KEY];
  for (const name of [
    "click_element_trusted",
    "type_text_trusted",
    "press_key_trusted",
    "hover_element",
    "drag_element",
  ]) {
    assert.equal(tools[name], true, name);
  }
  assert.equal(page.stored[LIFETIMES_KEY]["input-trusted"].lifetime, "until-restart");
  assert.deepEqual(page.session[RUNS_KEY], { "input-trusted": true });
  assert.equal(box.checked, true);
  assert.match(page.group("input-trusted").textContent, /On until Brave restarts\./);
});

// The group takes a picture of a whole page through another extension, so what
// the page says of it names that extension and the window, says that the picture
// shows what nobody sees, that it goes to the person and not to the model, and
// that it reaches only the allowed sites.
test("the page says of the capture-trusted group that another extension takes the picture, that it shows parts nobody sees, that each is allowed in a window and that the picture does not go to the model", async () => {
  const page = await openPage();
  const risk = page.group("capture-trusted").children.find((child) => child.className === "note");
  assert.match(risk.textContent, /take a picture of all of a page you have open/);
  assert.match(risk.textContent, /parts you cannot see now and in parts nobody sees/);
  assert.match(risk.textContent, /BraveBot Trusted Input extension/);
  assert.match(risk.textContent, /read and change data on all websites/);
  assert.match(risk.textContent, /Allow once or Deny/);
  assert.match(risk.textContent, /only the sites you allow/);
  assert.doesNotMatch(risk.textContent, /click|type|key/i);
  assert.ok(
    ![...page.group("capture-trusted").tree()].some((e) => e.type === "radio" && e.name.startsWith("scope:")),
    "the page offers a scope for capture-trusted",
  );
  assert.match(page.group("capture-trusted").textContent, /No site is allowed yet, so a session reaches no page until you add one under Allowed sites\./);
  const box = page.control("group:capture-trusted");
  box.checked = true;
  await box.fire("change");
  const says = page.part("capture-trusted", "confirm").textContent;
  for (const [pattern, what] of [
    [/needs that extension to be installed and turned on in Brave/, "that it is needed"],
    [/Brave showed you its own install prompt then and shows none now/, "where the prompt was"],
    [/Nothing is taken until you press Allow once in that window/, "the order"],
    [/Deny, closing the window, or no answer within 60 seconds stops it/, "what stops it"],
    [/at most 2048 pixels on the long side/, "the size"],
    [/A frame is in the picture as the page draws it/, "frames"],
    [/comes back to you and not to the model/, "where it goes"],
    [/A picture over 8 MiB is refused/, "the cap"],
    [/there is no choice of all sites for this group/, "the scope"],
    [/How long it stays on/, "the lifetime"],
  ]) {
    assert.match(says, pattern, `the capture-trusted group does not say ${what}`);
  }
  const labelled = page
    .group("capture-trusted")
    .textContent.match(/Allow once in the window the BraveBot Trusted Input extension opens/g);
  assert.equal(labelled?.length, 1);
});

test("turning capture-trusted on asks the browser for nothing, and saves the group and its tool on until Brave restarts", async () => {
  const page = await openPage();
  const box = page.control("group:capture-trusted");
  box.checked = true;
  await box.fire("change");
  await page.control("allow:capture-trusted").fire("click");
  assert.deepEqual(callsNamed(page, "permissions.request"), []);
  assert.deepEqual(page.stored[GROUP_SWITCHES_KEY], { "capture-trusted": true });
  assert.equal(page.stored[SETTINGS_KEY].screenshot_full_page, true);
  assert.equal(page.stored[SETTINGS_KEY].click_element_trusted ?? false, false);
  assert.equal(page.stored[LIFETIMES_KEY]["capture-trusted"].lifetime, "until-restart");
  assert.deepEqual(page.session[RUNS_KEY], { "capture-trusted": true });
  assert.match(page.group("capture-trusted").textContent, /On until Brave restarts\./);
});

// The group reads the console of a page through the other extension, so what the
// page says of it names that extension, says what a console holds, that nothing
// is read before the person allows it in that extension's window, that the
// result goes to the person and not to the model, and what it leaves out.
test("the page says of the console-trusted group what the console holds, that another extension reads it after the person allows it, and that the model does not read it", async () => {
  const page = await openPage();
  const risk = page.group("console-trusted").children.find((child) => child.className === "note");
  assert.match(risk.textContent, /console of a page you have open/);
  assert.match(risk.textContent, /whatever the page's scripts printed/);
  assert.match(risk.textContent, /BraveBot Trusted Input extension/);
  assert.match(risk.textContent, /read and change data on all websites/);
  assert.match(risk.textContent, /Allow once or Deny/);
  assert.match(risk.textContent, /only the sites you allow/);
  assert.doesNotMatch(risk.textContent, /click|type|key press/i);
  const box = page.control("group:console-trusted");
  box.checked = true;
  await box.fire("change");
  const says = page.part("console-trusted", "confirm").textContent;
  for (const [pattern, what] of [
    [/needs that extension to be installed and turned on in Brave/, "that it is needed"],
    [/Nothing is read until you press Allow once in that window/, "the order"],
    [/Brave's notice that the extension is debugging the tab shows while it reads/, "the notice"],
    [/Deny, closing the window, or no answer within 60 seconds stops it/, "what stops it"],
    [/only one window is open at a time/, "one window"],
    [/A script that threw an error appears without the error's message/, "the cost"],
    [/BraveBot does not give it to the model/, "that the model does not read it"],
    [/a processor that answers a question about it, or after you have read the exact text and agreed/, "how it can be read"],
    [/there is no choice of all sites for this group/, "the scope"],
    [/How long it stays on/, "the lifetime"],
  ]) {
    assert.match(says, pattern, `the console-trusted group does not say ${what}`);
  }
  assert.equal(
    page.group("console-trusted").textContent.match(/Allow once in the window the BraveBot Trusted Input extension opens/g)?.length,
    1,
  );
  assert.ok(
    ![...page.group("console-trusted").tree()].some((e) => e.type === "radio" && e.name.startsWith("scope:")),
    "the page offers a scope for console-trusted",
  );
});

// The group gives a file of the person's to a page through the other extension, so
// what the page says of it names that extension and the directory the files are in,
// says what is refused and that nothing is chosen before the person allows it, and
// that a page may send the file at once.
test("the page says of the upload-trusted group which files can be chosen, that another extension chooses it after the person allows it, and that a page may send it at once", async () => {
  const page = await openPage();
  const risk = page.group("upload-trusted").children.find((child) => child.className === "note");
  assert.match(risk.textContent, /give a file of yours to a page you have open/);
  assert.match(risk.textContent, /the page can send it on at once/);
  assert.match(risk.textContent, /BraveBot Trusted Input extension/);
  assert.match(risk.textContent, /read and change data on all websites/);
  assert.match(risk.textContent, /Allow once or Deny/);
  assert.match(risk.textContent, /only the sites you allow, and only files you put in one directory/);
  assert.doesNotMatch(risk.textContent, /click|type|key press/i);
  const box = page.control("group:upload-trusted");
  box.checked = true;
  await box.fire("change");
  const says = page.part("upload-trusted", "confirm").textContent;
  for (const [pattern, what] of [
    [/uploads directory of the directory the BraveBot browser server was declared with/, "where the file is"],
    [/up to 10 MiB, and not named like a credential, a key or a secret/, "what is refused"],
    [/a link that leads out of the directory, a directory and any other path are refused before anything is sent/, "the refusals"],
    [/needs that extension to be installed and turned on in Brave/, "that it is needed"],
    [/the file input as the page names it, and the file's name, size and full path, and says that Brave reads the file and gives it to the page/, "what the window shows"],
    [/Nothing is chosen until you press Allow once in that window/, "the order"],
    [/Deny, closing the window, or no answer within 60 seconds stops it/, "what stops it"],
    [/A file input the page hides is chosen on as well, if the page's own label, aria-label or title names it, or its id or name where nothing else does, and the window says that the page hides it/, "that a hidden input is chosen on"],
    [/a page whose script sends the file when it is chosen receives it as soon as you press Allow once/, "that the page may take it"],
    [/there is no choice of all sites for this group/, "the scope"],
    [/How long it stays on/, "the lifetime"],
  ]) {
    assert.match(says, pattern, `the upload-trusted group does not say ${what}`);
  }
  assert.ok(
    ![...page.group("upload-trusted").tree()].some((e) => e.type === "radio" && e.name.startsWith("scope:")),
    "the page offers a scope for upload-trusted",
  );
});

test("turning upload-trusted on asks the browser for nothing, and saves the group and its tool on until Brave restarts, and no tool of another group", async () => {
  const page = await openPage();
  const box = page.control("group:upload-trusted");
  box.checked = true;
  await box.fire("change");
  await page.control("allow:upload-trusted").fire("click");
  assert.deepEqual(callsNamed(page, "permissions.request"), []);
  assert.deepEqual(page.stored[GROUP_SWITCHES_KEY], { "upload-trusted": true });
  const tools = page.stored[SETTINGS_KEY];
  assert.equal(tools.choose_file, true);
  for (const name of ["click_element_trusted", "read_console", "screenshot_full_page"]) {
    assert.notEqual(tools[name], true, name);
  }
  assert.equal(page.stored[LIFETIMES_KEY]["upload-trusted"].lifetime, "until-restart");
  assert.deepEqual(page.session[RUNS_KEY], { "upload-trusted": true });
  assert.match(page.group("upload-trusted").textContent, /On until Brave restarts\./);
});

test("turning console-trusted on asks the browser for nothing, and saves the group and its tool on until Brave restarts, and no tool of another group", async () => {
  const page = await openPage();
  const box = page.control("group:console-trusted");
  box.checked = true;
  await box.fire("change");
  await page.control("allow:console-trusted").fire("click");
  assert.deepEqual(callsNamed(page, "permissions.request"), []);
  assert.deepEqual(page.stored[GROUP_SWITCHES_KEY], { "console-trusted": true });
  const tools = page.stored[SETTINGS_KEY];
  assert.equal(tools.read_console, true);
  for (const name of ["click_element_trusted", "read_page", "screenshot_page"]) {
    assert.notEqual(tools[name], true, name);
  }
  assert.equal(page.stored[LIFETIMES_KEY]["console-trusted"].lifetime, "until-restart");
  assert.deepEqual(page.session[RUNS_KEY], { "console-trusted": true });
  assert.match(page.group("console-trusted").textContent, /On until Brave restarts\./);
});

// What Brave asks follows the scope: nothing for a host while no site is
// allowed, a line naming the sites, and the warning for all websites.
test("what Brave will ask for a group that reads pages follows the sites and the scope", async () => {
  const none = await openPage({ stored: { [SITE_SCOPES_KEY]: {} } });
  assert.ok(none.group("page-read").textContent.includes(
    "Brave asks you to allow: Read your browsing history.",
  ));
  const two = await openPage({
    stored: { [SITE_SCOPES_KEY]: {}, [ALLOWED_SITES_KEY]: [NEWS, INTRANET] },
  });
  assert.ok(two.group("page-read").textContent.includes(
    "Brave asks you to allow: Read your browsing history; Read and change your data on intranet and news.example.",
  ));
  const many = await openPage({
    stored: {
      [SITE_SCOPES_KEY]: {},
      [ALLOWED_SITES_KEY]: [NEWS, INTRANET, "https://a.example", "https://b.example"],
    },
  });
  assert.ok(many.group("page-read").textContent.includes(
    "Read and change your data on a number of websites.",
  ));
  const all = await openPage({ stored: { [SITE_SCOPES_KEY]: { "page-read": "all" } } });
  assert.ok(all.group("page-read").textContent.includes(
    "Read and change all your data on all websites.",
  ));
});

// Saving a page asks for pageCapture, which Brave words as access to all
// websites with or without hosts, and downloads lists it. So the downloads
// group says the same under every set of allowed sites, and the sites do not
// change what Brave asks.
test("what Brave will ask for the downloads group is the warning for all websites and for downloads, whatever sites are allowed", async () => {
  const line = "Brave asks you to allow: Manage your downloads; Read and change all your data on all websites.";
  for (const stored of [
    { [SITE_SCOPES_KEY]: {} },
    { [SITE_SCOPES_KEY]: {}, [ALLOWED_SITES_KEY]: [NEWS, INTRANET] },
    { [SITE_SCOPES_KEY]: { "page-read": "all" } },
  ]) {
    const page = await openPage({ stored });
    const text = page.group("downloads").textContent;
    assert.ok(text.includes(line), text);
    assert.ok(!text.includes("Read your browsing history"));
    assert.ok(!page.group("page-read").textContent.includes("Manage your downloads"));
  }
});

test("turning page-read on asks for the allowed sites and no more, or for none", async () => {
  for (const [stored, request] of [
    [{ [SITE_SCOPES_KEY]: {} }, { permissions: PAGE_PERMISSIONS }],
    [
      { [SITE_SCOPES_KEY]: {}, [ALLOWED_SITES_KEY]: [NEWS] },
      { permissions: PAGE_PERMISSIONS, origins: [`${NEWS}/*`] },
    ],
  ]) {
    const page = await openPage({ stored });
    const box = page.control("group:page-read");
    box.checked = true;
    await box.fire("change");
    const before = page.calls.length;
    const clicking = page.control("allow:page-read").fire("click");
    assert.deepEqual(page.calls.slice(before), [["permissions.request", request]]);
    await clicking;
    assert.deepEqual(page.stored[GROUP_SWITCHES_KEY], { "page-read": true });
    assert.equal(box.checked, true);
  }
});

test("the Read preset asks for no host while no site is allowed", async () => {
  const page = await openPage({ stored: { [SITE_SCOPES_KEY]: {} } });
  await choose(page, "read");
  assert.ok(confirmOf(page).textContent.includes(
    "Brave asks you to allow: Read and change your browsing history on all your signed-in devices; View and manage your tab groups; Read and change your bookmarks; Read and change entries in the reading list.",
  ));
  const before = page.calls.length;
  const clicking = page.control("apply-preset").fire("click");
  assert.deepEqual(page.calls.slice(before), [
    [
      "permissions.request",
      {
        permissions: ["tabs", "tabGroups", "webNavigation", "sessions", "history", "topSites", "bookmarks", "readingList", "scripting"],
      },
    ],
  ]);
  await clicking;
  assert.equal(page.control("group:page-read").checked, true);
});

// Choosing all sites for a group that is on asks for the two patterns in that
// click, and saves the choice only if they are granted.
test("choosing all sites for a group that is on asks the browser first, and saves the scope if it is granted", async () => {
  const state = () => ({
    held: { permissions: PAGE_PERMISSIONS, origins: [`${NEWS}/*`] },
    stored: {
      [SITE_SCOPES_KEY]: {},
      [ALLOWED_SITES_KEY]: [NEWS],
      [GROUP_SWITCHES_KEY]: { "page-read": true },
      [SETTINGS_KEY]: { read_page: true },
    },
  });
  const page = await openPage(state());
  assert.equal(page.control("group:page-read").checked, true);
  const before = page.calls.length;
  const choosing = radio(page, "page-read", "all").fire("change");
  assert.deepEqual(page.calls.slice(before), [
    [
      "permissions.request",
      { permissions: PAGE_PERMISSIONS, origins: ["<all_urls>"] },
    ],
  ]);
  await choosing;
  assert.deepEqual(page.stored[SITE_SCOPES_KEY], { "page-read": "all" });
  assert.equal(radio(page, "page-read", "all").checked, true);
  assert.equal(radio(page, "page-read", "allowed").checked, false);
  assert.equal(page.control("group:page-read").checked, true);

  const declined = await openPage({ ...state(), decision: false });
  await radio(declined, "page-read", "all").fire("change");
  assert.deepEqual(declined.stored[SITE_SCOPES_KEY], {});
  assert.equal(radio(declined, "page-read", "allowed").checked, true);
  assert.match(
    declined.group("page-read").textContent,
    /Not changed: Brave did not grant what the group needs\./,
  );
});

test("choosing the allowed sites again asks for their hosts and gives back all sites", async () => {
  const page = await openPage({
    held: { permissions: PAGE_PERMISSIONS, origins: ["<all_urls>"] },
    stored: {
      [SITE_SCOPES_KEY]: { "page-read": "all" },
      [ALLOWED_SITES_KEY]: [NEWS],
      [GROUP_SWITCHES_KEY]: { "page-read": true },
    },
  });
  await radio(page, "page-read", "allowed").fire("change");
  assert.deepEqual(callsNamed(page, "permissions.request"), [
    { permissions: PAGE_PERMISSIONS, origins: [`${NEWS}/*`] },
  ]);
  assert.deepEqual(callsNamed(page, "permissions.remove"), [
    { origins: ["<all_urls>"] },
  ]);
  assert.deepEqual(page.stored[SITE_SCOPES_KEY], {});
  assert.equal(page.control("group:page-read").checked, true);
  assert.equal(radio(page, "page-read", "allowed").checked, true);
});

test("choosing a scope for a group that is off only remembers it", async () => {
  const page = await openPage({ stored: { [SITE_SCOPES_KEY]: {} } });
  await radio(page, "page-read", "all").fire("change");
  assert.deepEqual(page.stored[SITE_SCOPES_KEY], { "page-read": "all" });
  assert.deepEqual(page.calls, []);
  assert.equal(page.control("group:page-read").checked, false);
});

test("a scope the browser reports an error for is said, and the page shows what is saved", async () => {
  const page = await openPage({
    decision: new Error("Permissions are blocked by enterprise policy."),
    held: { permissions: PAGE_PERMISSIONS, origins: [`${NEWS}/*`] },
    stored: {
      [SITE_SCOPES_KEY]: {},
      [ALLOWED_SITES_KEY]: [NEWS],
      [GROUP_SWITCHES_KEY]: { "page-read": true },
    },
  });
  await radio(page, "page-read", "all").fire("change");
  assert.match(
    page.group("page-read").textContent,
    /The choice shown is the one saved\. Brave reported an error: Permissions are blocked by enterprise policy\./,
  );
  assert.deepEqual(page.stored[SITE_SCOPES_KEY], {});
  assert.equal(radio(page, "page-read", "allowed").checked, true);
  assert.equal(radio(page, "page-read", "allowed").disabled, false);
});

const lifetimeRadio = (page, id, value) => {
  const found = [...page.group(id).tree()].find(
    (element) =>
      element.type === "radio" &&
      element.name === `lifetime:${id}` &&
      element.value === value,
  );
  assert.ok(found, `${id} has no lifetime ${value}`);
  return found;
};

// A group is turned on for as long as the person chooses, in the same
// confirmation that asks them to turn it on. The platform group has no switch
// to turn on, so it has no choice.
test("each group that can be turned on offers three lifetimes, until turned off chosen for the groups that read and organize and until Brave restarts for the groups that reach more", async () => {
  const page = await openPage();
  for (const id of page.groupIds()) {
    if (id === "platform") {
      assert.ok(
        ![...page.group(id).tree()].some((e) => e.name === `lifetime:${id}`),
        id,
      );
      continue;
    }
    const chosen = ["until-turned-off", "until-restart", "one-hour"].filter(
      (value) => lifetimeRadio(page, id, value).checked,
    );
    const untilRestart = [
      "downloads",
      "cookies",
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
    assert.deepEqual(
      chosen,
      [untilRestart.includes(id) ? "until-restart" : "until-turned-off"],
      id,
    );
    assert.match(page.part(id, "confirm").textContent, /How long it stays on/, id);
  }
});

test("the lifetime chosen in the confirmation is the one saved, and the page says when an hour or a run ends", async () => {
  const hour = await openPage();
  hour.control("group:tabs-read").checked = true;
  await hour.control("group:tabs-read").fire("change");
  lifetimeRadio(hour, "tabs-read", "until-turned-off").checked = false;
  lifetimeRadio(hour, "tabs-read", "one-hour").checked = true;
  const before = Date.now();
  await hour.control("allow:tabs-read").fire("click");
  const record = hour.stored[LIFETIMES_KEY]["tabs-read"];
  assert.equal(record.lifetime, "one-hour");
  assert.ok(record.expires >= before + HOUR_MS && record.expires <= Date.now() + HOUR_MS);
  assert.deepEqual(hour.session[RUNS_KEY], {});
  assert.match(hour.group("tabs-read").textContent, /On until \d{1,2}:\d{2}/);

  const restart = await openPage();
  restart.control("group:history-read").checked = true;
  await restart.control("group:history-read").fire("change");
  lifetimeRadio(restart, "history-read", "until-turned-off").checked = false;
  lifetimeRadio(restart, "history-read", "until-restart").checked = true;
  await restart.control("allow:history-read").fire("click");
  assert.deepEqual(restart.stored[LIFETIMES_KEY], {
    "history-read": { lifetime: "until-restart" },
  });
  assert.deepEqual(restart.session[RUNS_KEY], { "history-read": true });
  assert.match(restart.group("history-read").textContent, /On until Brave restarts\./);

  const plain = await openPage();
  plain.control("group:bookmarks-read").checked = true;
  await plain.control("group:bookmarks-read").fire("change");
  await plain.control("allow:bookmarks-read").fire("click");
  assert.deepEqual(plain.stored[LIFETIMES_KEY], {
    "bookmarks-read": { lifetime: "until-turned-off" },
  });
  assert.doesNotMatch(plain.group("bookmarks-read").textContent, /On until/);
});

// A group that deletes data is on until Brave restarts unless the person chooses
// longer, so confirming it with the choice left as the page offered it saves
// that lifetime, marks the run, and says the group ends with the browser.
test("turning remove-data on with the lifetime left as offered saves until Brave restarts", async () => {
  const page = await openPage();
  page.control("group:remove-data").checked = true;
  await page.control("group:remove-data").fire("change");
  await page.control("allow:remove-data").fire("click");
  assert.deepEqual(page.stored[LIFETIMES_KEY], {
    "remove-data": { lifetime: "until-restart" },
  });
  assert.deepEqual(page.session[RUNS_KEY], { "remove-data": true });
  assert.match(page.group("remove-data").textContent, /On until Brave restarts\./);
});

// The switch can still be saved on in the browser's storage after the hour has
// passed, until the service worker turns it off. The page does not show it on.
test("a group whose hour has passed is drawn as off", async () => {
  const page = await openPage({
    held: allOptional(),
    stored: {
      [GROUP_SWITCHES_KEY]: { "tabs-read": true, "history-read": true },
      [LIFETIMES_KEY]: {
        "tabs-read": { lifetime: "one-hour", expires: Date.now() - 1 },
        "history-read": { lifetime: "one-hour", expires: Date.now() + HOUR_MS },
      },
    },
  });
  assert.equal(page.control("group:tabs-read").checked, false);
  assert.equal(page.control("group:history-read").checked, true);
  assert.doesNotMatch(page.group("tabs-read").textContent, /On until/);
});

// The cells of the rows the page drew for the log, newest first.
const activityRows = (page) =>
  page
    .element("activity-rows")
    .children.map((row) => row.children.map((cell) => cell.textContent));

const ENTRIES = [
  { time: Date.UTC(2026, 9, 30, 9, 0, 0), group: "platform", tool: "get_platform_info", outcome: "ran", origins: [] },
  { time: Date.UTC(2026, 9, 30, 9, 1, 0), group: "history-read", tool: "search_history", outcome: "refused", origins: [] },
  {
    time: Date.UTC(2026, 9, 30, 9, 2, 0),
    group: "tabs-read",
    tool: "list_frames",
    outcome: "failed",
    origins: ["https://a.example", "http://b.example:8080"],
  },
  { time: Date.UTC(2026, 9, 30, 9, 3, 0), group: null, tool: null, outcome: "refused", origins: [] },
];

test("the page lists the log newest first, with time, group, tool, result and the sites named", async () => {
  const page = await openPage({ stored: { [ACTIVITY_KEY]: ENTRIES } });
  const time = (entry) => new Date(entry.time).toLocaleString();
  assert.deepEqual(activityRows(page), [
    [time(ENTRIES[3]), "none", "none", "refused", "none"],
    [time(ENTRIES[2]), "tabs-read", "list_frames", "failed", "https://a.example, http://b.example:8080"],
    [time(ENTRIES[1]), "history-read", "search_history", "refused", "none"],
    [time(ENTRIES[0]), "platform", "get_platform_info", "ran", "none"],
  ]);
  const [stamp] = page.element("activity-rows").children[0].children[0].children;
  assert.equal(stamp.tagName, "time");
  assert.equal(stamp.attributes.datetime, new Date(ENTRIES[3].time).toISOString());
  assert.equal(page.element("activity-status").textContent, "");
});

test("the page says when no call has reached the extension, and shows an empty log as no rows", async () => {
  const page = await openPage();
  assert.deepEqual(activityRows(page), []);
  assert.equal(page.element("activity-status").textContent, "No call has reached the extension yet.");
});

test("a stored log that cannot be read as entries shows no rows", async () => {
  const page = await openPage({ stored: { [ACTIVITY_KEY]: [{ outcome: "ran" }, "text"] } });
  assert.deepEqual(activityRows(page), []);
});

// A time outside the range of dates cannot be written as one, which would stop the
// page drawing the rest of the log.
test("an entry whose time is no date is left out and the others are drawn", async () => {
  const entry = { time: 5, group: "tabs-read", tool: "list_tabs", outcome: "ran", origins: [] };
  const page = await openPage({
    stored: { [ACTIVITY_KEY]: [entry, { ...entry, time: 1e300 }, { ...entry, time: 6 }] },
  });
  assert.equal(activityRows(page).length, 2);
});

// The log changes with every call, and each switch redraw asks the browser what
// it holds. A page that redrew the switches for each call would ask it for
// each call.
test("a call added while the page is open is drawn, and the switches are not redrawn for it", async () => {
  const page = await openPage({
    held: allOptional(),
    stored: { [GROUP_SWITCHES_KEY]: { "tabs-read": true } },
  });
  const held = () => callsNamed(page, "permissions.contains").length;
  const before = held();
  assert.ok(before > 0, "a group that is on is checked when the page is drawn");
  await chrome.storage.local.set({ [ACTIVITY_KEY]: ENTRIES.slice(0, 2) });
  await flush();
  assert.equal(activityRows(page).length, 2);
  assert.equal(held(), before, "the switches were redrawn for a call");
  // A switch another page saved is still followed.
  await chrome.storage.local.set({ [GROUP_SWITCHES_KEY]: {} });
  await flush();
  assert.ok(held() >= before);
  assert.equal(page.control("group:tabs-read").checked, false);
});

test("the page tells what the log keeps and what it leaves out", async () => {
  const note = markup.match(/<p id="activity-note">([\s\S]*?)<\/p>/)[1].replace(/\s+/g, " ");
  assert.match(note, /last 200 calls/);
  assert.match(note, /whether or not you are watching/);
  assert.match(note, /No page text, search text or title is kept/);
  assert.match(note, /group that is off is listed too, as refused/);
});

// The extension's content security policy lets a page run only a script file of
// its own, and the options page and the popup load nothing from elsewhere.
test("the options page and the popup load only scripts of their own and nothing from elsewhere", () => {
  for (const name of ["options.html", "popup.html"]) {
    const html = readFileSync(new URL(`../${name}`, import.meta.url), "utf8");
    const scripts = [...html.matchAll(/<script\b([^>]*)>([\s\S]*?)<\/script>/g)];
    assert.equal(scripts.length, 1, name);
    assert.match(scripts[0][1], /^\s*type="module"\s+src="[a-z]+\.js"\s*$/, name);
    assert.equal(scripts[0][2], "", `${name} has an inline script`);
    assert.doesNotMatch(html, /\son[a-z]+\s*=/i, `${name} has an inline handler`);
    assert.doesNotMatch(html, /javascript:/i, name);
    assert.doesNotMatch(html, /<(?:link|img|iframe|object|embed)\b/i, name);
    assert.doesNotMatch(html, /@import|url\(/i, name);
  }
});

// A picture of a page needs the browser's access to all sites, so the page says
// so wherever it describes the group: in what turning it on lets a session do,
// in the line under each scope, and in the two presets that leave the group on
// the allowed sites.
test("the page says that a picture of a page needs all sites, in the group, under each scope and in the presets", async () => {
  const allowed = await openPage({ stored: { [SITE_SCOPES_KEY]: {} } });
  const box = allowed.control("group:page-read");
  box.checked = true;
  await box.fire("change");
  const text = allowed.group("page-read").textContent;
  assert.match(text, /and take a picture of it/);
  assert.match(text, /a picture is taken only with all sites/);
  assert.match(text, /With all sites, take a picture of the page its window shows/);
  assert.match(text, /A picture needs all sites for the group, because the browser lets an extension capture a tab only with access to all sites/);
  assert.match(text, /Take a picture of an open page/);
  assert.match(
    text,
    /A picture of a page is refused with this choice, because the browser lets an extension capture a tab only with access to all sites\./,
  );
  assert.equal(allowed.control("screenshot_page").disabled, true);

  const all = await openPage({ stored: { [SITE_SCOPES_KEY]: { "page-read": "all" } } });
  assert.match(
    all.group("page-read").textContent,
    /A picture of a page is taken only with this choice, because the browser lets an extension capture a tab only with access to all sites\./,
  );

  // The other groups that read pages take no picture and say nothing of one.
  for (const id of ["cookies", "network-watch", "input"]) {
    assert.doesNotMatch(allowed.group(id).textContent, /picture/, id);
  }

  const rows = allowed.element("presets").children.map((row) => row.textContent);
  assert.match(rows[2], /^Read.*No picture of a page is taken, since that needs all sites\./);
  assert.match(rows[3], /^Browse.*No picture of a page is taken, since that needs all sites\./);
});

// Saving a page asks for pageCapture, which the browser holds for any site, so
// the page says under the downloads group that the extension and not Brave
// limits the sites a page is saved from, which are the allowed sites whatever
// scope page-read has, and says where the file goes.
test("the page says that saving a page is limited to the allowed sites by this extension and not by Brave, and that the file stays in Downloads", async () => {
  for (const [stored, sites] of [
    [{ [SITE_SCOPES_KEY]: {} }, /No site is allowed yet, so a page is not saved until you add one under Allowed sites\./],
    [
      { [SITE_SCOPES_KEY]: {}, [ALLOWED_SITES_KEY]: [NEWS] },
      /A page is saved only from the 1 site under Allowed sites\./,
    ],
    [
      { [SITE_SCOPES_KEY]: {}, [ALLOWED_SITES_KEY]: [NEWS, INTRANET] },
      /A page is saved only from the 2 sites under Allowed sites\./,
    ],
    [
      { [SITE_SCOPES_KEY]: { "page-read": "all" }, [ALLOWED_SITES_KEY]: [NEWS] },
      /A page is saved only from the 1 site under Allowed sites\./,
    ],
  ]) {
    const page = await openPage({ stored });
    const text = page.group("downloads").textContent;
    assert.match(text, sites, JSON.stringify(stored));
    assert.match(text, /Save an open page at an exact address/);
    assert.match(text, /Save a page as a file/);
    assert.match(text, /a file already there is not replaced, and a page over 16 MiB is not saved/);
    assert.match(
      text,
      /saving a page is limited to those sites by this extension and not by Brave\./,
    );
    assert.match(text, /stays in your Downloads folder until you delete it, and any program of yours can read it/);
    assert.match(
      text,
      /also holds the permission to read the title and address of your open tabs, which the tool needs to find the page by its address, and Brave shows no line of its own for it/,
    );
    assert.equal(page.control("save_page_archive").disabled, true);
    // The group has no choice of all sites for saving.
    const scopes = [...page.group("downloads").tree()].filter(
      (element) => element.type === "radio" && ["allowed", "all"].includes(element.value),
    );
    assert.equal(scopes.length, 0);
    // The groups that read pages save no page and say nothing of one.
    for (const id of ["page-read", "cookies", "network-watch", "input"]) {
      assert.doesNotMatch(page.group(id).textContent, /saving a page|MHTML|Save a page/, id);
    }
  }
});
