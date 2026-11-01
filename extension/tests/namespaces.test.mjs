// The design document places every namespace of the Chrome extension API
// reference in one of three tables: what the tools of a group call, what no
// session reaches, and what the extension uses for itself. These tests hold the
// tables to the reference's namespace list, to the group table in groups.js,
// and to the extension's own files.
//
// Run with `node --test extension/tests/`.

import assert from "node:assert/strict";
import { readdirSync, readFileSync } from "node:fs";
import { test } from "node:test";
import { GROUPS } from "../groups.js";

const read = (path) => readFileSync(new URL(path, import.meta.url), "utf8");

const DOCUMENT = read("../../docs/design/browser-extension.md");
const MANIFEST = JSON.parse(read("../manifest.json"));

// The code of the extension, a line at a time, without the comment lines.
const CODE = readdirSync(new URL("..", import.meta.url))
  .filter((name) => name.endsWith(".js"))
  .flatMap((name) => read(`../${name}`).split("\n"))
  .filter((line) => !line.trimStart().startsWith("//"));

// The namespaces the reference lists at
// https://developer.chrome.com/docs/extensions/reference/api, as read on
// 2026-10-30. A namespace the reference gains or drops is changed here and in
// the document together.
const REFERENCE = `
  accessibilityFeatures action alarms audio bookmarks browsingData
  certificateProvider commands contentSettings contextMenus cookies debugger
  declarativeContent declarativeNetRequest desktopCapture
  devtools.inspectedWindow devtools.network devtools.panels
  devtools.performance devtools.recorder dns documentScan dom downloads
  enterprise.deviceAttributes enterprise.hardwarePlatform enterprise.login
  enterprise.networkingAttributes enterprise.platformKeys events extension
  extensionTypes fileBrowserHandler fileSystemProvider fontSettings gcm
  history i18n identity idle input.ime instanceID loginState management
  mimeHandler notifications offscreen omnibox pageCapture permissions
  platformKeys power printerProvider printing printingMetrics privacy
  processes proxy publicSuffix readingList runtime scripting search sessions
  sidePanel storage system.cpu system.display system.memory system.storage
  systemLog tabCapture tabGroups tabs topSites tts ttsEngine types
  userScripts vpnProvider wallpaper webAuthenticationProxy webNavigation
  webRequest windows
`
  .trim()
  .split(/\s+/);

// Names the document places that are not namespaces of the reference. The
// second table names webRequestBlocking, a permission of webRequest, and its
// link goes to the webRequest page.
const NOT_NAMESPACES = ["webRequestBlocking"];

const API_PAGE = "https://developer.chrome.com/docs/extensions/reference/api/";

// The lines of `### <heading>` up to the next heading.
function section(heading) {
  const lines = DOCUMENT.split("\n");
  const start = lines.indexOf(`### ${heading}`);
  assert.notEqual(start, -1, `the design document has no "### ${heading}"`);
  const end = lines.findIndex(
    (line, index) => index > start && /^#{1,3} /.test(line),
  );
  return lines.slice(start + 1, end === -1 ? undefined : end);
}

const LINK = /\[`([^`]+)`\]/g;

// The rows of the table in a section. The first cell holds the namespaces,
// each as a link, and `cells` are the others.
function rows(heading) {
  return section(heading)
    .filter((line) => line.startsWith("| ["))
    .map((line) => {
      const [first, ...cells] = line
        .split("|")
        .slice(1, -1)
        .map((cell) => cell.trim());
      assert.equal(
        first.replace(LINK, "").replaceAll(",", "").trim(),
        "",
        `${line}: the first cell holds only linked names`,
      );
      return {
        names: [...first.matchAll(LINK)].map((match) => match[1]),
        cells,
      };
    });
}

const ticked = (text) =>
  [...text.matchAll(/`([^`]+)`/g)].map((match) => match[1]);

const GROUP_ROWS = rows("Namespaces the groups use");
const NOT_EXPOSED = rows("Not exposed to a session, and why");
const USED = rows("Used by the extension itself");

const namesOf = (table) => table.flatMap((row) => row.names);

// A line of the document that gives a name's page: [`tabs`]: https://...
const DEFINITIONS = new Map(
  DOCUMENT.split("\n")
    .map((line) => line.match(/^\[`([^`]+)`\]: (\S+)$/))
    .filter(Boolean)
    .map((match) => [match[1], match[2]]),
);

// Whether the extension's code calls `chrome.<name>`.
function calls(name) {
  const call = new RegExp(`chrome\\.${name.replaceAll(".", "\\.")}\\b`);
  return CODE.some((line) => call.test(line));
}

const declared = (name) =>
  [...MANIFEST.permissions, ...MANIFEST.optional_permissions].includes(name);

// Dropping a namespace from the tables, or placing one twice, leaves a
// namespace of the reference with no group, no reason and no owner, or with two
// answers to whether a session reaches it.
test("every namespace of the reference is in exactly one row of the three tables", () => {
  assert.equal(new Set(REFERENCE).size, REFERENCE.length);
  const found = [GROUP_ROWS, NOT_EXPOSED, USED].flatMap(namesOf);
  const twice = found.filter((name, index) => found.indexOf(name) !== index);
  assert.deepEqual(twice, [], "placed in more than one row");
  const expected = [...REFERENCE, ...NOT_NAMESPACES];
  assert.deepEqual(
    expected.filter((name) => !found.includes(name)),
    [],
    "in the reference and in no table",
  );
  assert.deepEqual(
    found.filter((name) => !expected.includes(name)),
    [],
    "in a table and not in the reference",
  );
  for (const name of NOT_NAMESPACES) {
    assert.ok(
      namesOf(NOT_EXPOSED).includes(name),
      `${name} is in the second table`,
    );
  }
});

test("each namespace links to its page in the reference", () => {
  const found = [GROUP_ROWS, NOT_EXPOSED, USED].flatMap(namesOf);
  for (const name of found) {
    const page = NOT_NAMESPACES.includes(name) ? "webRequest" : name;
    assert.equal(
      DEFINITIONS.get(name),
      `${API_PAGE}${page.replaceAll(".", "/")}`,
      `${name} links to the wrong page or to none`,
    );
  }
  assert.deepEqual(
    [...DEFINITIONS.keys()].filter((name) => !found.includes(name)),
    [],
    "a link for a name no row has",
  );
});

// A group the document names that groups.js lacks is a rename in one place. A
// permission a group lists that the document gives another group, or none, is
// a group the reader is told does not use it.
test("the groups in the document are those of groups.js, and each lists the permissions the document gives it", () => {
  const known = Object.keys(GROUPS);
  const permissionOf = (row) => {
    const [permission] = ticked(row.cells[1]);
    assert.ok(
      permission !== undefined || row.cells[1] === "none",
      `${row.names}: the permission is a name or none`,
    );
    return permission;
  };
  for (const row of GROUP_ROWS) {
    const ids = ticked(row.cells[0]);
    assert.ok(ids.length > 0, `${row.names} names no group`);
    assert.equal(
      new Set(ids).size,
      ids.length,
      `${row.names} names a group twice`,
    );
    for (const id of ids) {
      assert.ok(
        known.includes(id),
        `${row.names} names ${id}, which groups.js lacks`,
      );
    }
  }
  for (const [id, group] of Object.entries(GROUPS)) {
    for (const permission of group.permissions) {
      assert.ok(
        GROUP_ROWS.some(
          (row) =>
            permissionOf(row) === permission &&
            ticked(row.cells[0]).includes(id),
        ),
        `${id} lists ${permission}, and no row of the first table gives it to ${id}`,
      );
    }
  }
  const given = GROUP_ROWS.map(permissionOf);
  for (const permission of MANIFEST.optional_permissions) {
    assert.ok(
      given.includes(permission),
      `the manifest offers ${permission}, which no row of the first table gives a group`,
    );
  }
});

// A namespace the second table keeps from a session has to be absent from
// everything that could give one a way in: a group's permissions, a tool named
// for it, the manifest, and a call in the code.
test("nothing a group, a tool, the manifest or the code names is a namespace no session reaches", () => {
  const kept = [...namesOf(NOT_EXPOSED)];
  assert.ok(kept.length > 0);
  // The manifest keys that turn on an API of the second table.
  const keys = ["devtools_page", "declarative_net_request", "input_components"];
  for (const key of keys) {
    assert.equal(MANIFEST[key], undefined, `the manifest has ${key}`);
  }
  for (const name of kept) {
    assert.equal(declared(name), false, `the manifest declares ${name}`);
    assert.equal(calls(name), false, `the extension calls chrome.${name}`);
    const parts = name.toLowerCase().split(".");
    for (const [id, group] of Object.entries(GROUPS)) {
      assert.equal(
        group.permissions.includes(name),
        false,
        `${id} lists ${name}`,
      );
      for (const tool of group.tools) {
        const words = tool.split("_");
        const named = words.some((_, start) =>
          parts.every((part, offset) => words[start + offset] === part),
        );
        assert.equal(named, false, `${tool} in ${id} is named for ${name}`);
      }
    }
  }
});

// The manifest keys other than a permission by which a namespace of the third
// table is turned on or read.
const MANIFEST_KEYS = {
  action: "action",
  commands: "commands",
  i18n: "default_locale",
  omnibox: "omnibox",
  sidePanel: "side_panel",
};

// The state is the document's word for whether a reader can find the
// namespace in the code. A used row the code does not call, or a reserved row
// it does, leaves the document saying something the code contradicts.
test("a namespace the document calls used is called, and one it calls reserved is neither called nor declared", () => {
  assert.ok(USED.length > 0);
  for (const row of USED) {
    const [state] = row.cells;
    assert.ok(
      state === "used" || state === "reserved",
      `${row.names}: the state is used or reserved, not ${state}`,
    );
    for (const name of row.names) {
      // Every listener the extension adds is an events.Event, which has no
      // call of its own.
      const reached =
        name === "events"
          ? CODE.some((line) => line.includes(".addListener("))
          : calls(name);
      if (state === "used") {
        assert.ok(
          reached,
          `${name} is marked used and the extension does not call chrome.${name}`,
        );
      } else {
        assert.equal(
          reached,
          false,
          `${name} is marked reserved and the extension calls it: mark it used in docs/design/browser-extension.md`,
        );
        assert.equal(declared(name), false, `${name} is reserved and declared`);
        const key = MANIFEST_KEYS[name];
        assert.ok(
          key === undefined || MANIFEST[key] === undefined,
          `${name} is reserved and the manifest has ${key}`,
        );
      }
    }
  }
});

// A web page can message an extension only through externally_connectable, and
// another extension only through a listener for an external message, so an
// extension with neither cannot be reached from outside.
test("the extension declares nothing a web page or another extension could send a message to", () => {
  assert.equal(
    MANIFEST.externally_connectable,
    undefined,
    "the manifest declares externally_connectable",
  );
  for (const line of CODE) {
    assert.doesNotMatch(
      line,
      /onMessageExternal|onConnectExternal/,
      `the code listens for an external message: ${line.trim()}`,
    );
  }
  // The scan reads the files it is meant to: a listener for the extension's own
  // port is among them.
  assert.ok(CODE.some((line) => line.includes("chrome.runtime.onStartup")));
});
