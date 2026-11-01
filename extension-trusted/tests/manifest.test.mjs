// The manifest, the popup page and the ids: what the browser is told the
// extension holds, who may message it, and the ids the two extensions name each
// other by.
//
// Run with `node --test extension-trusted/tests/`.

import assert from "node:assert/strict";
import { createHash, createPublicKey } from "node:crypto";
import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import { test } from "node:test";
import { FIRST_ID, SECOND_ID } from "../pair.js";
import { METHODS, PICTURES, REPORTS } from "../requests.js";

const read = (path) => readFileSync(new URL(path, import.meta.url), "utf8");
const MANIFEST = JSON.parse(read("../manifest.json"));
const FIRST_MANIFEST = JSON.parse(read("../../extension/manifest.json"));

// The id a browser gives an unpacked extension with this `key`: the first 16
// bytes of the SHA-256 of the key's DER, each half-byte written as a letter
// from `a`.
function idOf(key) {
  const digest = createHash("sha256").update(Buffer.from(key, "base64")).digest();
  return [...digest.subarray(0, 16)]
    .flatMap((byte) => [byte >> 4, byte & 15])
    .map((half) => String.fromCharCode(97 + half))
    .join("");
}

// Every file of the extension outside its tests, by relative path.
function files(directory = "..", prefix = "") {
  return readdirSync(new URL(`${directory}/`, import.meta.url)).flatMap((name) => {
    const path = `${directory}/${name}`;
    if (name === "tests" || name === "node_modules") {
      return [];
    }
    return statSync(new URL(path, import.meta.url)).isDirectory()
      ? files(path, `${prefix}${name}/`)
      : [`${prefix}${name}`];
  });
}

test("the manifest holds the permission debugger and no other permission, host or page", () => {
  assert.deepEqual(MANIFEST.permissions, ["debugger"]);
  // Every key the manifest has is one of these. A key that grants a host, a
  // page of its own, a content script or a command is not.
  assert.deepEqual(Object.keys(MANIFEST).sort(), [
    "action",
    "background",
    "content_security_policy",
    "description",
    "externally_connectable",
    "key",
    "manifest_version",
    "minimum_chrome_version",
    "name",
    "permissions",
    "version",
  ]);
  for (const name of [
    "optional_permissions",
    "host_permissions",
    "optional_host_permissions",
    "content_scripts",
    "web_accessible_resources",
    "commands",
    "options_ui",
    "side_panel",
    "devtools_page",
    "chrome_url_overrides",
    "sandbox",
    "declarative_net_request",
  ]) {
    assert.equal(MANIFEST[name], undefined, `the manifest has ${name}`);
  }
  for (const permission of ["nativeMessaging", "storage", "tabs", "scripting"]) {
    assert.equal(MANIFEST.permissions.includes(permission), false);
  }
  assert.equal(MANIFEST.manifest_version, 3);
});

test("the only extension that may message this one is the BraveBot extension, and no web page may", () => {
  assert.deepEqual(MANIFEST.externally_connectable, { ids: [FIRST_ID] });
  assert.equal(idOf(FIRST_MANIFEST.key), FIRST_ID);
});

test("each id is the one its key gives, and the two are not the same", () => {
  assert.equal(idOf(MANIFEST.key), SECOND_ID);
  assert.equal(idOf(FIRST_MANIFEST.key), FIRST_ID);
  assert.notEqual(SECOND_ID, FIRST_ID);
  assert.notEqual(MANIFEST.key, FIRST_MANIFEST.key);
  assert.match(SECOND_ID, /^[a-p]{32}$/);
});

test("the key is a public RSA key of 2048 bits or more, and no private key is in the extension's directory", () => {
  const key = createPublicKey({
    key: Buffer.from(MANIFEST.key, "base64"),
    format: "der",
    type: "spki",
  });
  assert.equal(key.asymmetricKeyType, "rsa");
  assert.ok(key.asymmetricKeyDetails.modulusLength >= 2048);
  const found = files();
  assert.ok(found.includes("manifest.json") && found.includes("background.js"));
  for (const path of found) {
    assert.doesNotMatch(path, /\.(pem|key|p12|pfx|crx)$/i, path);
    assert.doesNotMatch(read(`../${path}`), /PRIVATE KEY/, path);
  }
});

test("the pages load scripts and styles from the extension alone, and nothing else", () => {
  const policy = Object.fromEntries(
    MANIFEST.content_security_policy.extension_pages
      .split(";")
      .map((directive) => directive.trim().split(/\s+/))
      .map(([name, ...sources]) => [name, sources]),
  );
  assert.deepEqual(Object.keys(MANIFEST.content_security_policy), ["extension_pages"]);
  assert.deepEqual(policy["default-src"], ["'none'"]);
  assert.deepEqual(policy["script-src"], ["'self'"]);
  assert.deepEqual(policy["style-src"], ["'self'"]);
  for (const directive of ["object-src", "base-uri", "form-action", "frame-src"]) {
    assert.deepEqual(policy[directive], ["'none'"], directive);
  }
  assert.doesNotMatch(
    MANIFEST.content_security_policy.extension_pages,
    /unsafe|\*|https?:|data:|blob:|filesystem:/,
  );
});

test("the toolbar action opens a static popup that runs one script of the extension and no inline code", () => {
  const popup = MANIFEST.action.default_popup;
  assert.equal(popup, "popup.html");
  const page = read(`../${popup}`);
  const scripts = [...page.matchAll(/<script\b[^>]*>/g)].map((match) => match[0]);
  assert.deepEqual(scripts, ['<script type="module" src="popup.js">']);
  assert.doesNotMatch(page, /<script[^>]*>[^<]*\S[^<]*<\/script>/);
  assert.doesNotMatch(page, /\son\w+\s*=/i);
  assert.doesNotMatch(page, /<style|\sstyle\s*=|<img|<iframe|<form|<object|<embed|<link[^>]*\bhref="https?:/i);
  assert.match(page, /brave:\/\/extensions/);
  assert.match(page, /turn[\s\S]*off/i);
  for (const path of [popup, "popup.js", "popup.css", MANIFEST.background.service_worker]) {
    assert.ok(existsSync(new URL(`../${path}`, import.meta.url)), path);
  }
  assert.deepEqual(MANIFEST.background, { service_worker: "background.js", type: "module" });
});

test("the description fits the store's limit and says what the extension does and for whom", () => {
  assert.ok(MANIFEST.description.length <= 132, `${MANIFEST.description.length} characters`);
  for (const word of ["real mouse and keyboard input", "BraveBot", "allow"]) {
    assert.ok(MANIFEST.description.includes(word), `the description lacks ${word}`);
  }
});

// The words the description and the popup use for each kind of tool the
// extension answers: input, a picture of the whole page and the page's console.
const KINDS = Object.freeze({
  input: ["real mouse and keyboard input", "real mouse clicks, typing and key presses"],
  picture: ["picture of a whole page", "picture of all of a page"],
  console: ["its console", "its console"],
  upload: ["a chosen file", "chooses a file of yours on a page's file input"],
});

test("the description and the popup name every kind of tool the extension answers, so that a tool added without them fails here", () => {
  // A method that is a picture or a report is its own kind, and the others send input.
  const kindOf = (method) =>
    PICTURES.includes(method)
      ? "picture"
      : REPORTS.includes(method)
        ? "console"
        : method === "choose_file"
          ? "upload"
          : "input";
  assert.deepEqual(Object.keys(METHODS), [
    "click_element_trusted",
    "type_text_trusted",
    "press_key_trusted",
    "hover_element",
    "drag_element",
    "screenshot_full_page",
    "read_console",
    "choose_file",
  ]);
  const answered = [...new Set(Object.keys(METHODS).map(kindOf))];
  assert.deepEqual(answered.sort(), Object.keys(KINDS).sort());
  const popup = read("../popup.html").replace(/\s+/g, " ");
  for (const kind of answered) {
    const [description, page] = KINDS[kind];
    assert.ok(MANIFEST.description.includes(description), `the description lacks the ${kind} tools: ${description}`);
    assert.ok(popup.includes(page), `the popup lacks the ${kind} tools: ${page}`);
  }
  // Each asks in a window of its own.
  assert.ok(popup.includes("each action in a window of its own"));
});

// The BraveBot extension needs a newer browser than this one for the group that
// runs code (userScripts.execute, Chrome 135), so this extension asks for the
// least version its own calls need and never more than the BraveBot extension.
test("the least version of the browser has the calls this one uses and is not newer than the BraveBot extension's", () => {
  const mine = Number(MANIFEST.minimum_chrome_version);
  assert.ok(mine >= 116);
  assert.ok(mine <= Number(FIRST_MANIFEST.minimum_chrome_version));
});
