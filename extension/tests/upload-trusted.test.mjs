// The tool of the upload-trusted group, which this extension checks and sends to
// the BraveBot Trusted Input extension: what has to hold before anything is sent,
// what is sent (the file the relay resolved, and nothing of its own), and what the
// reply becomes. Run once against the other extension's own code, over a fake
// debugger, for the form of the request.
//
// Run with `node --test extension/tests/`.

import assert from "node:assert/strict";
import { afterEach, mock, test } from "node:test";
import { readFileSync } from "node:fs";
import { activityLog } from "../activity.js";
import {
  CREDENTIAL_EXACT,
  CREDENTIAL_PREFIXES,
  CREDENTIAL_SUFFIXES,
  MAX_FILE_BYTES,
  PATH_LIMIT,
  fileWords,
  looksLikeACredential,
  problemWithFile,
  sizeWords,
} from "../files.js";
import { GROUPS, NO_ACCESS } from "../groups.js";
import { RUNS_KEY } from "../lifetimes.js";
import { setTool, turnOn } from "../switches.js";
import {
  ALLOWED_SITES_KEY,
  BLOCKED_SITES_KEY,
  BUSY_MESSAGE,
  GROUP_SWITCHES_KEY,
  SITE_SCOPES_KEY,
  handle,
} from "../tools.js";
import { NOT_THERE, SECOND_ID, STATUSES, VERSION } from "../trusted.js";
import * as seconds from "../../extension-trusted/files.js";
import { METHODS, STATUSES as SECONDS_STATUSES, parse } from "../../extension-trusted/requests.js";
import { fakeChrome } from "./fake-chrome.mjs";

afterEach(() => mock.timers.reset());

const NEWS = "https://news.example";
const PAGE = `${NEWS}/form`;
const BANK = "https://bank.example";
const METHOD = "choose_file";
const FILE = { path: "/srv/relay/uploads/report.txt", size: 12, file_name: "report.txt" };
const CALL = { url: PAGE, name: "Resume", ...FILE };
const DONE = { v: VERSION, result: { status: "chose", origin: NEWS } };

// A browser in which the person turned upload-trusted on and allowed the sites in
// `allowed`, and in which the other extension answers each message with `answered`.
async function trusted({ allowed = [NEWS], blocked = [], answered = DONE, local = {} } = {}) {
  const chrome = fakeChrome({
    held: {},
    local: { [ALLOWED_SITES_KEY]: allowed, [BLOCKED_SITES_KEY]: blocked, ...local },
    tabs: [{ id: 1, windowId: 1, title: "Form", url: PAGE }],
  });
  chrome.sent = [];
  chrome.runtime.sendMessage = async (id, request) => {
    chrome.sent.push([id, request]);
    return typeof answered === "function" ? answered(id, request) : answered;
  };
  assert.equal(await turnOn(chrome, "upload-trusted", NO_ACCESS), true);
  return chrome;
}

const call = (chrome, method, params) => handle({ id: 7, method, params }, chrome);

test("the two extensions hold the same limits, the same list of credential names and the same words for a file", () => {
  assert.equal(MAX_FILE_BYTES, seconds.MAX_FILE_BYTES);
  assert.equal(MAX_FILE_BYTES, 10 * 1024 * 1024);
  assert.equal(PATH_LIMIT, seconds.PATH_LIMIT);
  assert.deepEqual([...CREDENTIAL_EXACT], [...seconds.CREDENTIAL_EXACT]);
  assert.deepEqual([...CREDENTIAL_PREFIXES], [...seconds.CREDENTIAL_PREFIXES]);
  assert.deepEqual([...CREDENTIAL_SUFFIXES], [...seconds.CREDENTIAL_SUFFIXES]);
  assert.equal(fileWords(FILE), seconds.fileWords(FILE));
  assert.equal(fileWords(FILE), "report.txt, 12 bytes, /srv/relay/uploads/report.txt");
  assert.deepEqual([...STATUSES], [...SECONDS_STATUSES]);
  assert.ok(STATUSES.includes("chose"));
  assert.equal(fileWords({ ...FILE, size: 1536 }), "report.txt, 1536 bytes (1.5 KiB), /srv/relay/uploads/report.txt");
  assert.equal(sizeWords(10 * 1024 * 1024), "10485760 bytes (10.0 MiB)");
  assert.equal(sizeWords(0), "0 bytes");
});

test("the group is a group of its own that lists no permission and no host, is limited to the allowed sites, and holds the one tool", () => {
  const group = GROUPS["upload-trusted"];
  assert.deepEqual(group.tools, [METHOD]);
  assert.deepEqual(group.permissions, []);
  assert.equal(group.hosts, false);
  assert.equal(group.allowedOnly, true);
  for (const [id, other] of Object.entries(GROUPS)) {
    if (id !== "upload-trusted") {
      assert.ok(!other.tools.includes(METHOD), id);
    }
  }
});

test("a call sends the address, the name and the file the relay resolved, to the other extension, and no other argument", async () => {
  const chrome = await trusted();
  const got = await call(chrome, METHOD, { ...CALL, selector: "#f", x: 1, tab_id: 1, files: ["/etc/passwd"], backendNodeId: 9 });
  assert.equal(got.error, undefined, got.error?.message);
  assert.deepEqual(got.result, { status: "chose", origin: NEWS });
  assert.deepEqual(chrome.sent, [[SECOND_ID, { v: VERSION, method: METHOD, params: { ...CALL } }]]);

  const roled = await trusted();
  await call(roled, METHOD, { ...CALL, role: "button" });
  assert.deepEqual(roled.sent.at(-1)[1].params, { ...CALL, role: "button" });
  assert.equal("role" in chrome.sent[0][1].params, false);
});

test("the request is one the other extension's own check accepts, with the arguments the call gave", async () => {
  assert.ok(Object.hasOwn(METHODS, METHOD));
  const chrome = await trusted();
  await call(chrome, METHOD, CALL);
  const { method, args } = parse(chrome.sent[0][1]);
  assert.equal(method, METHOD);
  assert.deepEqual({ ...args }, { ...CALL });
});

test("nothing is sent while the group is off, the tool is off, the lifetime has ended, the site is not allowed or blocked", async () => {
  const off = await trusted();
  off.local[GROUP_SWITCHES_KEY] = {};
  assert.match((await call(off, METHOD, CALL)).error.message, /in the upload-trusted group, which is turned off/);
  assert.deepEqual(off.sent, []);

  const toolOff = await trusted();
  await setTool(toolOff, METHOD, false);
  assert.match((await call(toolOff, METHOD, CALL)).error.message, /is turned off in the BraveBot extension's options/);
  assert.deepEqual(toolOff.sent, []);

  const lapsed = await trusted();
  lapsed.session[RUNS_KEY] = {};
  assert.match((await call(lapsed, METHOD, CALL)).error.message, /was turned on until Brave restarts, and that has ended/);
  assert.deepEqual(lapsed.sent, []);

  const elsewhere = await trusted({ allowed: [BANK] });
  assert.match((await call(elsewhere, METHOD, CALL)).error.message, /not in the allowed sites list/);
  assert.deepEqual(elsewhere.sent, []);

  const widened = await trusted({ allowed: [BANK], local: { [SITE_SCOPES_KEY]: { "upload-trusted": "all" } } });
  assert.match((await call(widened, METHOD, CALL)).error.message, /not in the allowed sites list/);
  assert.deepEqual(widened.sent, []);

  const blocked = await trusted({ allowed: [NEWS], blocked: [NEWS] });
  assert.match((await call(blocked, METHOD, CALL)).error.message, /in the blocked sites list/);
  assert.deepEqual(blocked.sent, []);
});

test("a file that is not what the relay sends, or is named like a credential, is refused before anything is sent", async () => {
  const bad = [
    { path: undefined },
    { path: "" },
    { path: 7 },
    { path: "report.txt" },
    { path: "./report.txt" },
    { path: "/srv/uploads/../x.txt", file_name: "x.txt" },
    { path: "/srv//x.txt", file_name: "x.txt" },
    { path: "/srv/uploads/" , file_name: ""},
    { path: "/srv/uploads/a\nb.txt", file_name: "a\nb.txt" },
    { path: "/srv/uploads/a‮b.txt", file_name: "a‮b.txt" },
    { path: `/${"a".repeat(PATH_LIMIT)}`, file_name: "a".repeat(PATH_LIMIT) },
    { size: undefined },
    { size: -1 },
    { size: 1.5 },
    { size: "12" },
    { size: MAX_FILE_BYTES + 1 },
    { file_name: undefined },
    { file_name: "other.txt" },
    { file_name: "" },
    { name: "" },
    { name: "x".repeat(201) },
    { role: "div" },
    { url: undefined },
    { url: "file:///etc/passwd" },
    { path: "/srv/uploads/.env", file_name: ".env" },
    { path: "/srv/uploads/.ENV.local", file_name: ".ENV.local" },
    { path: "/srv/uploads/id_rsa", file_name: "id_rsa" },
    { path: "/srv/uploads/key.pem", file_name: "key.pem" },
    { path: "/srv/.ssh/notes.txt", file_name: "notes.txt" },
    { path: "/srv/.aws/c.txt", file_name: "c.txt" },
  ];
  for (const change of bad) {
    const chrome = await trusted();
    const reply = await call(chrome, METHOD, { ...CALL, ...change });
    assert.ok(reply.error, JSON.stringify(change));
    assert.deepEqual(chrome.sent, [], JSON.stringify(change));
  }
  const exact = await trusted();
  assert.equal((await call(exact, METHOD, { ...CALL, size: MAX_FILE_BYTES })).error, undefined);
  const empty = await trusted();
  assert.equal((await call(empty, METHOD, { ...CALL, size: 0 })).error, undefined);
});

test("the sentence of a credential name says why, and names neither the file nor the path", async () => {
  const chrome = await trusted();
  const reply = await call(chrome, METHOD, { ...CALL, path: "/srv/uploads/.env", file_name: ".env" });
  assert.match(reply.error.message, /looks like a credential, a key or a secret/);
  assert.equal(reply.error.message.includes(".env"), false);
});

test("problemWithFile names the first thing wrong, and nothing is wrong with a file of the form the relay sends", () => {
  assert.equal(problemWithFile(FILE), null);
  assert.equal(problemWithFile({ ...FILE, path: "x" }), "path");
  assert.equal(problemWithFile({ ...FILE, size: -1 }), "size");
  assert.equal(problemWithFile({ ...FILE, file_name: "x" }), "file_name");
  assert.equal(problemWithFile({ ...FILE, path: "/a/.ssh/report.txt" }), "credential");
  // The check of the form comes before the check of the name.
  assert.equal(problemWithFile({ path: "/a/.env", size: -1, file_name: ".env" }), "size");
  for (const name of [".env", ".ENV", ".env.local", "id_rsa", "ID_RSA.pub", "id_ed25519", "a.pem", "A.KEY", "credentials", "credentials.json", ".netrc", ".npmrc", ".pgpass", ".git-credentials", "v.kdbx", "c.p12", "c.pfx", ".ssh", ".aws", ".gnupg"]) {
    assert.ok(looksLikeACredential(name), name);
  }
  for (const name of ["environment.txt", "keyboard.txt", "monkey.txt", "pem.txt", "id.txt", "report.txt", "envelope.pdf", "donkey.png"]) {
    assert.equal(looksLikeACredential(name), false, name);
  }
});

// The list of file names that crates/browser/src/uploads.rs reads in a test of its
// own: for each, the result the relay gives the file in `uploads` and the result
// problemWithFile gives the canonical path the relay would send.
const LIST = JSON.parse(
  readFileSync(new URL("../../crates/browser/tests/file-names.json", import.meta.url), "utf8"),
).files;
const RANGES = JSON.parse(
  readFileSync(new URL("../../crates/browser/tests/file-names.json", import.meta.url), "utf8"),
).unseen_ranges;
const GIVEN = { chosen: null, refused: "path", credential: "credential" };

test("problemWithFile gives each name of the list the result the relay gives it, with the directories above uploads and the unseen characters of a name", () => {
  assert.ok(LIST.length >= 100, `the list holds ${LIST.length} names`);
  for (const check of [problemWithFile, seconds.problemWithFile]) {
    for (const { above, file, result, note } of LIST) {
      const path = `/srv/${above === "" ? "" : `${above}/`}uploads/${file}`;
      assert.equal(
        check({ path, size: 1, file_name: file.split("/").at(-1) }),
        GIVEN[result],
        `${path} (${result}): ${note}`,
      );
    }
  }
});

// The characters a person cannot see are the engine's Cc, Cf, Zl, Zp and Co, and
// the relay holds the ranges of them in a table of its own, which a test of
// uploads.rs holds to the same list. An engine that knows a range the list does
// not, as a newer Unicode adds, shows the two have come apart.
test("the unseen characters the engine knows are the ranges of the list the relay's table is held to", () => {
  const unseen = /[\p{Cc}\p{Cf}\p{Zl}\p{Zp}\p{Co}]/u;
  const ranges = [];
  let first = null;
  for (let point = 0; point <= 0x110000; point += 1) {
    const inside = point <= 0x10ffff && unseen.test(String.fromCodePoint(point));
    if (inside && first === null) {
      first = point;
    }
    if (!inside && first !== null) {
      ranges.push([first, point - 1]);
      first = null;
    }
  }
  assert.deepEqual(ranges, RANGES);
  // problemWithFile refuses a path with the first and the last of each, except the
  // null character, which no name holds.
  for (const [start, end] of RANGES) {
    for (const point of new Set([Math.max(start, 1), end])) {
      const name = `a${String.fromCodePoint(point)}b.txt`;
      assert.equal(problemWithFile({ path: `/srv/uploads/${name}`, size: 1, file_name: name }), "path", `U+${point.toString(16)}`);
    }
  }
});

test("only the parts below the last uploads directory are looked at for a credential, and all of them where there is none", () => {
  const file = (path) => ({ path, size: 1, file_name: path.split("/").at(-1) });
  for (const check of [problemWithFile, seconds.problemWithFile]) {
    assert.equal(check(file("/home/me/credentials-old/uploads/a.txt")), null);
    assert.equal(check(file("/home/me/.aws/relay/uploads/a.txt")), null);
    assert.equal(check(file("/home/me/my.key/uploads/a.txt")), null);
    assert.equal(check(file("/home/me/uploads/credentials-old/relay/uploads/a.txt")), null);
    assert.equal(check(file("/home/me/relay/uploads/.ssh/a.txt")), "credential");
    assert.equal(check(file("/home/me/relay/uploads/sub/id_rsa")), "credential");
    assert.equal(check(file("/home/me/relay/uploads/uploads/.env")), "credential");
    // A path with no uploads directory is none the relay sends, and all of it is looked at.
    assert.equal(check(file("/home/me/.aws/a.txt")), "credential");
    assert.equal(check(file("/srv/.ssh/notes.txt")), "credential");
    assert.equal(check(file("/home/me/relay/uploads")), null);
  }
});

test("a lone surrogate is a character no one sees, which a name from the relay never holds", () => {
  for (const check of [problemWithFile, seconds.problemWithFile]) {
    assert.equal(check({ path: "/srv/uploads/a\ud800b.txt", size: 1, file_name: "a\ud800b.txt" }), "path");
  }
});

test("the list of credential names is the relay's", () => {
  const rust = readFileSync(new URL("../../crates/browser/src/uploads.rs", import.meta.url), "utf8");
  const listed = (name) => {
    const body = new RegExp(`pub const ${name}: \\[&str; \\d+\\] = \\[([^\\]]*)\\]`).exec(rust)?.[1];
    assert.ok(body, name);
    return [...body.matchAll(/"([^"]*)"/g)].map((match) => match[1]);
  };
  assert.deepEqual(listed("CREDENTIAL_EXACT"), [...CREDENTIAL_EXACT]);
  assert.deepEqual(listed("CREDENTIAL_PREFIXES"), [...CREDENTIAL_PREFIXES]);
  assert.deepEqual(listed("CREDENTIAL_SUFFIXES"), [...CREDENTIAL_SUFFIXES]);
  assert.match(rust, new RegExp(`pub const MAX_BYTES: u64 = 10 \\* 1024 \\* 1024;`));
  assert.match(rust, new RegExp(`pub const PATH_LIMIT: usize = ${PATH_LIMIT};`));
});

test("a refusal or a failure of the other extension is passed on as one, with its sentence after fixed words, and the log keeps the origin and no file", async () => {
  const sentence = "it is not a file input, so a file cannot be chosen on it";
  const refused = await trusted({ answered: { v: VERSION, error: { kind: "refusal", message: sentence } } });
  const refusal = await call(refused, METHOD, CALL);
  assert.equal(refusal.error.message, `the BraveBot Trusted Input extension refused it: ${sentence}`);

  const absent = await trusted();
  absent.runtime.sendMessage = async () => {
    throw new Error("Could not establish connection. Receiving end does not exist.");
  };
  assert.equal((await call(absent, METHOD, CALL)).error.message, NOT_THERE);

  const chrome = await trusted();
  await call(chrome, METHOD, CALL);
  const log = JSON.stringify(await activityLog(chrome));
  assert.ok(log.includes("choose_file"));
  assert.ok(log.includes(NEWS));
  for (const word of ["report.txt", "/srv", "Resume", "uploads"]) {
    assert.equal(log.includes(word), false, word);
  }
});

test("a second call while one waits for the person's answer is refused, and the wait is not held", async () => {
  let release;
  const chrome = await trusted({
    answered: () => new Promise((resolve) => {
      release = () => resolve(DONE);
    }),
  });
  const first = call(chrome, METHOD, CALL);
  for (let turn = 0; turn < 50; turn += 1) {
    await Promise.resolve();
  }
  const second = await call(chrome, METHOD, CALL);
  assert.equal(second.error.message, BUSY_MESSAGE);
  release();
  assert.equal((await first).error, undefined);
});
