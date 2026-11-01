// The scripts group: run_script, which runs code the planner wrote in the top
// document of one open page after the person has read all of it in a window of
// the extension. The browser is a fake that records every call in order, its
// userScripts API is a fake that answers as a test says, and the person is a
// double that answers as a test says (or, in one test, the real window of
// confirmation.js and confirm.js over the fake browser of fake-window.mjs).
//
// Run with `node --test extension/tests/`.

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { afterEach, beforeEach, mock, test } from "node:test";
import { ACTIVITY_KEY, activityLog } from "../activity.js";
import { ACTIONS } from "../actions.js";
import {
  ALLOW_DELAY_MS,
  approve,
  setConfirmation,
} from "../approval.js";
import { Refusal } from "../errors.js";
import { GROUPS, PRESETS } from "../groups.js";
import { RUNS_KEY } from "../lifetimes.js";
import {
  CODE_LIMIT,
  POLICY_NOT_APPLIED,
  POLICY_PROBE,
  RESULT_LIMIT,
  UNAVAILABLE,
  WORLD_CSP,
  describeScript,
  namesAReference,
  resultText,
  userScriptsAvailable,
} from "../scripts.js";
import {
  ALLOWED_SITES_KEY,
  BLOCKED_SITES_KEY,
  BUSY_MESSAGE,
  GROUP_SWITCHES_KEY,
  PAGE_WAIT_MS,
  SETTINGS_KEY,
  TOOLS,
  handle,
} from "../tools.js";
import { TYPED_TEXT_LIMIT, URL_LIMIT } from "../words.js";
import { fakePermissions } from "./fake-permissions.mjs";
import { fakeArea } from "./fake-storage.mjs";
import { flush, world } from "./fake-window.mjs";

const MANIFEST = JSON.parse(
  readFileSync(new URL("../manifest.json", import.meta.url), "utf8"),
);

const SHOP = "https://shop.example/cart";
const ORIGIN = "https://shop.example";
const TAB = { id: 1, windowId: 1, title: "Cart", url: SHOP };

// Code with leading and trailing spaces, a tab, a line feed and a character no
// one sees, so that a call that trims, wraps or normalizes the code is told apart
// from one that sends it as written.
const CODE = "  const title = document.title;\n\ttitle + \"​\"  ";

const PROTECTED = "This site is protected and cannot be scripted.";
const NOT_AVAILABLE = "'userScripts.getWorldConfigurations' is not available in this context.";

// The probe is answered by how many worlds the document holds, as the browser does.
const BY_WORLDS = Symbol("by the worlds the document holds");

beforeEach(() => mock.timers.enable({ apis: ["setTimeout", "setInterval", "Date"] }));
afterEach(() => {
  setConfirmation();
  mock.timers.reset();
});

// A fake browser with the scripts group on, one site allowed and the tab open at
// it. `calls` is the one list of everything the extension reached, in order, and
// the person's double writes to it as well.
//
//   tabs        the open tabs; a tab is found by its address and its current
//               address is `current`'s, which a test can change
//   granted     what the person has granted
//   stored      added to what storage.local holds
//   userScripts what chrome.userScripts is: "ready", "absent", or "switched off"
//   answer      what a script answers: a value or a function of the injection
//   failing     an error `execute` rejects with for the code of the call
//   hanging     `execute` never answers the code of the call
//   empty       `execute` answers no injection result for the code of the call
//   stopped     how many worlds of one document the browser gives the policy, as
//               Brave 1.98.63 gives ten; the probe answers "open" for the others
//   probe       what the probe is answered: a value, a function of the injection,
//               an Error to reject with, or { reply } for the whole of the browser's
//               answer
//   probeHangs  `execute` never answers the probe
//
// Each tab has a document, `documents`, that `chrome.reload(id)` replaces with
// another at the same address. `execute` runs only in the document a target names,
// as the browser does, and `ran` lists the code that ran and in which document.
function browser({
  tabs = [TAB],
  granted = { permissions: ["userScripts"], origins: [`${ORIGIN}/*`] },
  stored = {},
  userScripts = "ready",
  answer = "Cart - Shop",
  failing,
  hanging = false,
  empty = false,
  stopped = Infinity,
  probe = BY_WORLDS,
  probeHangs = false,
} = {}) {
  const calls = [];
  const ran = [];
  const documents = new Map(tabs.map((tab) => [tab.id, `doc-${tab.id}`]));
  const worlds = new Map();
  const local = {
    [SETTINGS_KEY]: { run_script: true },
    [GROUP_SWITCHES_KEY]: { scripts: true },
    [ALLOWED_SITES_KEY]: [ORIGIN],
    ...stored,
  };
  const current = new Map(tabs.map((tab) => [tab.id, { ...tab }]));
  const ready = {
    async getWorldConfigurations() {
      calls.push(["userScripts.getWorldConfigurations"]);
      return [];
    },
    async configureWorld(configuration) {
      calls.push(["userScripts.configureWorld", configuration]);
    },
    async execute(injection) {
      calls.push(["userScripts.execute", injection]);
      const { tabId, documentIds } = injection.target;
      const document = documents.get(tabId);
      if (document === undefined) {
        throw new Error(`No tab with id: ${tabId}.`);
      }
      if (documentIds && !documentIds.includes(document)) {
        throw new Error(`No document with id ${documentIds[0]} in tab with id ${tabId}`);
      }
      if (injection.js[0].code === POLICY_PROBE) {
        if (probeHangs) {
          return new Promise(() => {});
        }
        const held = worlds.get(document) ?? new Set();
        held.add(injection.worldId);
        worlds.set(document, held);
        const told = typeof probe === "function" ? probe(injection) : probe;
        if (told instanceof Error) {
          throw told;
        }
        if (told !== null && typeof told === "object" && "reply" in told) {
          return told.reply;
        }
        return [
          {
            documentId: document,
            frameId: 0,
            result: told === BY_WORLDS ? (held.size <= stopped ? "closed" : "open") : told,
          },
        ];
      }
      ran.push([injection.js[0].code, document]);
      chrome.during?.();
      if (failing) {
        throw failing;
      }
      if (hanging) {
        return new Promise(() => {});
      }
      if (empty) {
        return [];
      }
      return [
        {
          documentId: document,
          frameId: 0,
          result: typeof answer === "function" ? answer(injection) : answer,
        },
      ];
    },
    async resetWorldConfiguration(worldId) {
      calls.push(["userScripts.resetWorldConfiguration", worldId]);
    },
  };
  const switchedOff = {
    async getWorldConfigurations() {
      calls.push(["userScripts.getWorldConfigurations"]);
      throw new Error(NOT_AVAILABLE);
    },
  };
  let reloads = 0;
  const chrome = {
    calls,
    local,
    current,
    // The code that ran, as [code, id of the document it ran in].
    ran,
    // The tab's page is replaced by another at the same address.
    reload(id) {
      documents.set(id, `doc-${id}-${(reloads += 1)}`);
    },
    // Called while the script runs, which is when a test moves the tab.
    during: undefined,
    permissions: fakePermissions({ held: granted, calls }),
    storage: {
      local: fakeArea(local),
      session: fakeArea({ [RUNS_KEY]: { scripts: true } }),
    },
    tabs: {
      async query(filter) {
        calls.push(["tabs.query", filter]);
        return [...current.values()];
      },
      async get(id) {
        calls.push(["tabs.get", id]);
        return current.get(id);
      },
    },
    ...(userScripts === "ready" ? { userScripts: ready } : {}),
    ...(userScripts === "switched off" ? { userScripts: switchedOff } : {}),
  };
  return chrome;
}

const call = (chrome, params, method = "run_script") =>
  handle({ id: 1, method, params }, chrome);

const reached = (chrome, api) => chrome.calls.some(([name]) => name === api);
const namesOf = (chrome) => chrome.calls.map(([name]) => name);

// The calls of userScripts.execute that were the probe, and those that were code.
const executes = (chrome) =>
  chrome.calls.filter(([name]) => name === "userScripts.execute").map(([, injection]) => injection);
const probes = (chrome) => executes(chrome).filter((injection) => injection.js[0].code === POLICY_PROBE);
const codes = (chrome) => executes(chrome).filter((injection) => injection.js[0].code !== POLICY_PROBE);

// The person's window as a double: it records what it was shown, in order with
// the browser's calls, and answers with a token for it unless `answer` says
// otherwise.
function person(chrome, answer = approve) {
  setConfirmation(async (resolution) => {
    chrome.calls.push(["asked", resolution]);
    return answer(resolution);
  });
  return () => chrome.calls.filter(([name]) => name === "asked").map(([, shown]) => shown);
}

const SAYS = { url: SHOP, code: CODE };

test("the code runs only after the person has been asked and allowed it, and not before", async () => {
  const chrome = browser();
  const asked = person(chrome);
  const reply = await call(chrome, SAYS);
  assert.deepEqual(reply.result, { status: "ran", origin: ORIGIN, result: "Cart - Shop" });
  assert.equal(asked().length, 1);
  const question = namesOf(chrome).indexOf("asked");
  const at = (injection) => chrome.calls.findIndex(([, call]) => call === injection);
  assert.equal(codes(chrome).length, 1);
  assert.ok(at(codes(chrome)[0]) > question, "the code reached the browser before the person was asked");
  assert.ok(
    namesOf(chrome).indexOf("userScripts.getWorldConfigurations") < question,
    "the person was asked before the extension knew it could run the code",
  );
  // What runs before the person is asked is the extension's probe and nothing of the planner's.
  for (const probe of probes(chrome)) {
    assert.ok(at(probe) < question);
    assert.deepEqual(probe.js, [{ code: POLICY_PROBE }]);
  }
  assert.deepEqual(chrome.ran.map(([code]) => code), [CODE]);
});

test("the code is sent as written to a world of its own that has the policy and is reset after the run, and the next call has another world", async () => {
  const chrome = browser();
  person(chrome);
  await call(chrome, SAYS);
  await call(chrome, SAYS);
  const configured = chrome.calls.filter(([name]) => name === "userScripts.configureWorld");
  const reset = chrome.calls.filter(([name]) => name === "userScripts.resetWorldConfiguration");
  assert.equal(configured.length, 2);
  const [first, second] = configured.map(([, configuration]) => configuration);
  assert.equal(first.csp, "default-src 'none'");
  assert.equal(first.csp, WORLD_CSP);
  assert.equal(first.messaging, false);
  assert.match(first.worldId, /^bravebot-[0-9a-f-]{36}$/);
  assert.notEqual(first.worldId, second.worldId, "two calls shared a world");
  // The code is run in the top document of the tab that the probe named, in the
  // world the probe was run in.
  assert.deepEqual(codes(chrome)[0], {
    target: { tabId: 1, documentIds: ["doc-1"] },
    world: "USER_SCRIPT",
    worldId: first.worldId,
    js: [{ code: CODE }],
  });
  assert.deepEqual(
    probes(chrome).map((injection) => injection.worldId),
    [first.worldId, second.worldId],
  );
  assert.deepEqual(
    codes(chrome).map((injection) => injection.worldId),
    [first.worldId, second.worldId],
  );
  assert.deepEqual(
    reset.map(([, worldId]) => worldId),
    [first.worldId, second.worldId],
  );
  // The policy is configured before anything runs in the world, and the world is
  // reset after the code.
  const names = namesOf(chrome);
  assert.ok(names.indexOf("userScripts.configureWorld") < names.indexOf("userScripts.execute"));
  assert.ok(names.indexOf("userScripts.execute") < names.indexOf("userScripts.resetWorldConfiguration"));
  assert.ok(chrome.calls.findIndex(([, call]) => call === codes(chrome)[0]) < names.indexOf("userScripts.resetWorldConfiguration"));
});

test("the person is given the complete code and the exact address, and nothing the page wrote", async () => {
  const chrome = browser();
  const asked = person(chrome);
  const long = `${"a".repeat(CODE_LIMIT - 20)}\n${"b".repeat(19)}`;
  await call(chrome, { url: SHOP, code: long });
  const [resolution] = asked();
  assert.deepEqual(resolution.description, {
    action: "run_script",
    origin: ORIGIN,
    url: SHOP,
    tag: "",
    role: null,
    name: "",
    excerpt: "",
    goes: "",
    code: long,
  });
  assert.deepEqual(resolution.description, describeScript({ origin: ORIGIN, url: SHOP, code: long }));
  assert.equal(resolution.description.code.length, CODE_LIMIT);
  assert.ok(Object.isFrozen(resolution.description));
});

// The real window: the worker that opens it and the page the person answers in.
// The code is on the screen as lines before anything runs, and runs only when the
// person presses Allow once.
test("through the real window the person reads every line of the code first, and the code runs only when they press Allow once", async () => {
  const chrome = browser();
  const fake = world();
  setConfirmation(fake.confirmation.ask);
  const code = "const a = 1;\nconst b = 2;\na + b";
  const running = call(chrome, { url: SHOP, code });
  await flush();
  const page = fake.page;
  assert.equal(page.element("url").textContent, SHOP);
  assert.equal(page.element("typed-count").textContent, `3 lines, ${code.length} characters`);
  assert.equal(page.element("typed").textContent, "const a = 1;U+000A\nconst b = 2;U+000A\na + b");
  assert.deepEqual(codes(chrome), [], "the code ran before the person answered");
  mock.timers.tick(ALLOW_DELAY_MS);
  await flush();
  await page.click("allow");
  const reply = await running;
  assert.equal(reply.result.status, "ran");
  assert.equal(codes(chrome)[0].js[0].code, code);

  const denied = browser();
  const second = world();
  setConfirmation(second.confirmation.ask);
  const refusing = call(denied, { url: SHOP, code });
  await flush();
  await second.page.click("deny");
  const refused = await refusing;
  assert.match(refused.error.message, /you denied the action in the browser, so nothing was done/);
  assert.deepEqual(codes(denied), []);
  assert.equal(reached(denied, "userScripts.resetWorldConfiguration"), true, "the world of a denied call was left configured");
});

test("nothing runs unless the person's answer is a token for this call", async () => {
  const answers = {
    "nobody is there to ask": () => null,
    "the person denied": () => {
      throw new Refusal("you denied the action in the browser, so nothing was done");
    },
    "a token made for another call": () => approve(Object.freeze({ other: true })),
    "an answer that is not a token": () => true,
  };
  for (const [why, answer] of Object.entries(answers)) {
    const chrome = browser();
    person(chrome, answer);
    const reply = await call(chrome, SAYS);
    assert.equal(reply.result, undefined, why);
    assert.ok(reply.error, why);
    assert.deepEqual(codes(chrome), [], why);
    assert.equal(reached(chrome, "userScripts.resetWorldConfiguration"), true, `${why}: the world was left configured`);
    assert.equal((await activityLog(chrome))[0].outcome, "refused", why);
  }
});

// A token is spent by the check, so a token kept from a call cannot be used for
// another call with the same code.
test("a token the person gave once is no answer to the next call", async () => {
  const chrome = browser();
  let kept;
  person(chrome, (resolution) => {
    kept ??= approve(resolution);
    return kept;
  });
  assert.equal((await call(chrome, SAYS)).result.status, "ran");
  const again = await call(chrome, SAYS);
  assert.equal(again.result, undefined);
  assert.equal(codes(chrome).length, 1);
});

test("a call that cannot run is refused or failed before anybody is asked, and the browser's scripting is never reached", async () => {
  const cases = {
    "no code": [{ url: SHOP }, /code must be a string that is not empty/],
    "blank code": [{ url: SHOP, code: " \n\t " }, /code must be a string that is not empty/],
    "code that is not text": [{ url: SHOP, code: 5 }, /code must be a string that is not empty/],
    "code over the most characters": [
      { url: SHOP, code: "x".repeat(CODE_LIMIT + 1) },
      new RegExp(`code may hold at most ${CODE_LIMIT} characters`),
    ],
    "the name of a reference": [{ url: SHOP, code: "ref:3" }, /not the name of a reference/],
    "the name of a reference with space around it": [
      { url: SHOP, code: "\n ref:12 \n" },
      /not the name of a reference/,
    ],
    "no address": [{ code: CODE }, /not in the allowed sites list/],
    "an address that is not a web address": [
      { url: "file:///etc/passwd", code: CODE },
      /not in the allowed sites list/,
    ],
    "a site that is not allowed": [
      { url: "https://other.example/", code: CODE },
      /not in the allowed sites list/,
    ],
    "no open tab at the address": [
      { url: `${ORIGIN}/other`, code: CODE },
      /no open tab is at https:\/\/shop\.example\/other/,
    ],
  };
  for (const [why, [params, error]] of Object.entries(cases)) {
    const chrome = browser();
    const asked = person(chrome);
    const reply = await call(chrome, params);
    assert.equal(reply.result, undefined, why);
    assert.match(reply.error.message, error, why);
    assert.equal(asked().length, 0, `${why}: the person was asked`);
    assert.equal(reached(chrome, "userScripts.configureWorld"), false, why);
    assert.equal(reached(chrome, "userScripts.execute"), false, why);
  }
});

// The allowed sites are web origins, so handle refuses an address that is not one
// before the tool is reached; the tool checks it as well, for the day a list holds
// another kind of address.
test("the tool itself refuses an address that is not a web address before anybody is asked", async () => {
  const chrome = browser({ stored: { [ALLOWED_SITES_KEY]: ["file:///"] } });
  const asked = person(chrome);
  await assert.rejects(
    TOOLS.run_script(chrome, { url: "file:///etc/passwd", code: CODE }),
    /url must be an HTTP or HTTPS URL/,
  );
  assert.equal(asked().length, 0);
  assert.equal(reached(chrome, "tabs.query"), false);
});

test("a blocked site, two tabs at the address and an address too long to show whole are refused before anybody is asked", async () => {
  const blocked = browser({ stored: { [BLOCKED_SITES_KEY]: [ORIGIN] } });
  const asked = person(blocked);
  const reply = await call(blocked, SAYS);
  assert.match(reply.error.message, /blocked sites list/);
  assert.equal(asked().length, 0);
  assert.equal(reached(blocked, "tabs.query"), false, "whether a tab is open at a blocked site was looked up");
  assert.equal(reached(blocked, "userScripts.getWorldConfigurations"), false);

  const twice = browser({ tabs: [TAB, { ...TAB, id: 2 }] });
  const askedTwice = person(twice);
  const several = await call(twice, SAYS);
  assert.match(several.error.message, /2 open tabs are at https:\/\/shop\.example\/cart/);
  assert.equal(askedTwice().length, 0);
  assert.equal(reached(twice, "userScripts.execute"), false);

  const long = `${SHOP}?${"q".repeat(URL_LIMIT)}`;
  const tall = browser({ tabs: [{ ...TAB, url: long }] });
  const askedTall = person(tall);
  const refused = await call(tall, { url: long, code: CODE });
  assert.match(refused.error.message, new RegExp(`longer than ${URL_LIMIT} characters, so the window cannot show it whole`));
  assert.equal(askedTall().length, 0);
  assert.equal((await activityLog(tall))[0].outcome, "refused");
});

test("the code limit is the most the window shows whole, and a code of exactly that many characters is asked about", async () => {
  assert.equal(CODE_LIMIT, TYPED_TEXT_LIMIT);
  const chrome = browser();
  const asked = person(chrome);
  const most = "y".repeat(CODE_LIMIT);
  assert.equal((await call(chrome, { url: SHOP, code: most })).result.status, "ran");
  assert.equal(asked()[0].description.code.length, CODE_LIMIT);
  const over = await call(chrome, { url: SHOP, code: `${most}y` });
  assert.ok(over.error);
  assert.equal(asked().length, 1, "a code over the limit was asked about");
});

test("while userScripts is not usable the call is refused with the one sentence, before anybody is asked", async () => {
  for (const [why, state] of [
    ["the permission is held and the person's switch is off", "switched off"],
    ["the namespace is absent", "absent"],
  ]) {
    const chrome = browser({ userScripts: state });
    const asked = person(chrome);
    const reply = await call(chrome, SAYS);
    assert.equal(reply.error.message, UNAVAILABLE, why);
    assert.equal(asked().length, 0, why);
    assert.equal((await activityLog(chrome))[0].outcome, "refused", why);
  }
  assert.match(UNAVAILABLE, /Allow User Scripts is turned on for the BraveBot extension on its details page at brave:\/\/extensions/);
  assert.match(UNAVAILABLE, /older than Chrome 138, until Developer mode is turned on/);
  assert.equal(await userScriptsAvailable(browser({ userScripts: "ready" })), true);
  assert.equal(await userScriptsAvailable(browser({ userScripts: "switched off" })), false);
  assert.equal(await userScriptsAvailable(browser({ userScripts: "absent" })), false);
});

test("the tool is refused when its group is off, its permission or the site's host is not held, or the site is not allowed, and nothing is reached", async () => {
  const cases = {
    "the group is off": [{ stored: { [GROUP_SWITCHES_KEY]: {} } }, /run_script is in the scripts group, which is turned off/],
    "the tool is off": [{ stored: { [SETTINGS_KEY]: { run_script: false } } }, /run_script is turned off/],
    "userScripts is not granted": [
      { granted: { permissions: [], origins: [`${ORIGIN}/*`] } },
      /run_script needs the scripts group's browser permissions/,
    ],
    "the host of the site is not granted": [
      { granted: { permissions: ["userScripts"], origins: [] } },
      /run_script needs the scripts group's browser permissions/,
    ],
    "a scope of all sites is stored and no site is allowed": [
      { stored: { [ALLOWED_SITES_KEY]: [], site_scopes: { scripts: "all" } } },
      /not in the allowed sites list/,
    ],
  };
  for (const [why, [options, error]] of Object.entries(cases)) {
    const chrome = browser(options);
    const asked = person(chrome);
    const reply = await call(chrome, SAYS);
    assert.match(reply.error.message, error, why);
    assert.equal(asked().length, 0, why);
    assert.equal(reached(chrome, "tabs.query"), false, why);
    assert.equal(reached(chrome, "userScripts.getWorldConfigurations"), false, why);
  }
});

test("a tab that left the address while the person decided is not scripted, and one that left while the script ran returns nothing", async () => {
  const left = browser();
  person(left, (resolution) => {
    left.current.get(1).url = "https://elsewhere.example/";
    return approve(resolution);
  });
  const before = await call(left, SAYS);
  assert.equal(before.error.message, `the tab left ${SHOP} before the script ran, so nothing was run`);
  assert.deepEqual(codes(left), []);
  assert.equal(reached(left, "userScripts.resetWorldConfiguration"), true);

  for (const [why, move] of [
    ["the page navigated", (chrome) => (chrome.current.get(1).url = "https://elsewhere.example/")],
    ["the tab was closed", (chrome) => chrome.current.delete(1)],
  ]) {
    const chrome = browser({ answer: "the other page's text" });
    person(chrome);
    chrome.during = () => move(chrome);
    const reply = await call(chrome, SAYS);
    assert.equal(reply.result, undefined, why);
    assert.equal(
      reply.error.message,
      `the tab left ${SHOP} while the script ran, so its answer is not returned`,
      why,
    );
    assert.ok(!JSON.stringify(reply).includes("other page"), why);
    assert.equal(
      reached(chrome, "userScripts.resetWorldConfiguration"),
      true,
      `${why}: the world was left configured`,
    );
  }
});

test("a world the browser did not give the policy is refused with one sentence before anybody is asked, and none of the code reaches the browser", async () => {
  const chrome = browser({ stopped: 0 });
  const asked = person(chrome);
  const reply = await call(chrome, SAYS);
  assert.equal(reply.result, undefined);
  assert.equal(reply.error.message, POLICY_NOT_APPLIED);
  assert.match(POLICY_NOT_APPLIED, /did not apply the policy that stops the script's own requests in this page, so nothing was run; reload the page/);
  assert.equal(asked().length, 0);
  assert.deepEqual(codes(chrome), []);
  assert.deepEqual(chrome.ran, []);
  assert.equal(reached(chrome, "userScripts.resetWorldConfiguration"), true, "the world was left configured");
  assert.equal((await activityLog(chrome))[0].outcome, "refused");
});

// Brave 1.98.63 gave the policy to the first ten worlds of a document and none to
// the eleventh and later ones, so the eleventh call on one page ran with no policy.
test("the first ten calls on one page run, the eleventh is refused whole, and a page that was reloaded runs again", async () => {
  const chrome = browser({ stopped: 10 });
  const asked = person(chrome);
  for (let n = 1; n <= 10; n += 1) {
    assert.equal((await call(chrome, SAYS)).result.status, "ran", `call ${n}`);
  }
  const eleventh = await call(chrome, SAYS);
  assert.equal(eleventh.error.message, POLICY_NOT_APPLIED);
  assert.equal(asked().length, 10, "the person was asked about a call that could not be run");
  assert.equal(chrome.ran.length, 10, "the eleventh call ran");
  chrome.reload(1);
  assert.equal((await call(chrome, SAYS)).result.status, "ran");
  assert.deepEqual(chrome.ran.at(-1), [CODE, "doc-1-1"]);
});

test("only the answer closed lets a call go on, and any other answer of the probe is refused before anybody is asked", async () => {
  for (const told of ["open", "", undefined, null, true, 1, "Closed", ["closed"], { closed: true }]) {
    const chrome = browser({ probe: () => told });
    const asked = person(chrome);
    const reply = await call(chrome, SAYS);
    assert.equal(reply.error?.message, POLICY_NOT_APPLIED, JSON.stringify(told) ?? "undefined");
    assert.equal(asked().length, 0);
    assert.deepEqual(chrome.ran, []);
  }
});

test("the probe is the extension's own text, run in the world of the call and the document the code then runs in, before the window", async () => {
  assert.equal(POLICY_PROBE, 'fetch("data:,x").then(() => "open", () => "closed")');
  const chrome = browser();
  person(chrome);
  await call(chrome, { url: SHOP, code: "'probe-looking' + 'text'" });
  const [probe] = probes(chrome);
  const [code] = codes(chrome);
  assert.deepEqual(probe, {
    target: { tabId: 1 },
    world: "USER_SCRIPT",
    worldId: code.worldId,
    js: [{ code: POLICY_PROBE }],
  });
  assert.deepEqual(code.target, { tabId: 1, documentIds: ["doc-1"] });
  assert.equal(probes(chrome).length, 1);
  assert.equal(codes(chrome).length, 1);
});

test("a probe the browser fails or the page does not answer fails the call before anybody is asked, in the browser's words, and the world is reset", async () => {
  const failing = browser({ probe: new Error(PROTECTED) });
  const askedFailing = person(failing);
  const reply = await call(failing, SAYS);
  assert.equal(reply.error.message, `the page at ${SHOP} cannot be scripted: ${PROTECTED}`);
  assert.equal(askedFailing().length, 0);
  assert.deepEqual(failing.ran, []);
  assert.equal(reached(failing, "userScripts.resetWorldConfiguration"), true);
  assert.equal((await activityLog(failing))[0].outcome, "failed");

  const hanging = browser({ probeHangs: true });
  const askedHanging = person(hanging);
  let settled;
  const running = call(hanging, SAYS).then((answer) => {
    settled = answer;
  });
  await flush();
  assert.equal(settled, undefined);
  mock.timers.tick(PAGE_WAIT_MS);
  await running;
  assert.equal(
    settled.error.message,
    `the page at ${SHOP} cannot be scripted: the page did not answer within ${PAGE_WAIT_MS / 1000} seconds`,
  );
  assert.equal(askedHanging().length, 0);
  assert.deepEqual(hanging.ran, []);
  assert.equal(reached(hanging, "userScripts.resetWorldConfiguration"), true);

  const none = browser({ probe: () => ({ reply: [] }) });
  const askedNone = person(none);
  assert.equal((await call(none, SAYS)).error.message, `the page at ${SHOP} returned nothing`);
  const unnamed = browser({ probe: () => ({ reply: [{ frameId: 0, result: "closed" }] }) });
  person(unnamed);
  assert.equal(
    (await call(unnamed, SAYS)).error.message,
    `the page at ${SHOP} did not name its document, so nothing was run`,
  );
  assert.equal(askedNone().length, 0);
  assert.deepEqual([...none.ran, ...unnamed.ran], []);
});

test("a page that was reloaded at the same address while the person decided is not scripted, since the code is run in the document that was shown", async () => {
  const chrome = browser();
  person(chrome, (resolution) => {
    chrome.reload(1);
    return approve(resolution);
  });
  const reply = await call(chrome, SAYS);
  assert.equal(reply.result, undefined);
  assert.equal(
    reply.error.message,
    `the page at ${SHOP} cannot be scripted: No document with id doc-1 in tab with id 1`,
  );
  assert.deepEqual(chrome.ran, [], "the code ran in the page that replaced the one that was shown");
  assert.deepEqual(codes(chrome)[0].target, { tabId: 1, documentIds: ["doc-1"] });
  assert.equal(reached(chrome, "userScripts.resetWorldConfiguration"), true);
});

test("exactly one tab has to be at the address when the person allows, and it has to be the tab that was shown", async () => {
  const cases = {
    "a second tab arrived at the address": [
      (chrome) => chrome.current.set(2, { ...TAB, id: 2 }),
      `2 open tabs are at ${SHOP} now, so the URL does not name one tab and nothing was run`,
    ],
    "the tab left and another arrived at the address": [
      (chrome) => {
        chrome.current.get(1).url = "https://elsewhere.example/";
        chrome.current.set(2, { ...TAB, id: 2 });
      },
      `the tab left ${SHOP} before the script ran, so nothing was run`,
    ],
    "the tab left": [
      (chrome) => (chrome.current.get(1).url = "https://elsewhere.example/"),
      `the tab left ${SHOP} before the script ran, so nothing was run`,
    ],
    "the tab was closed": [
      (chrome) => chrome.current.delete(1),
      `the tab left ${SHOP} before the script ran, so nothing was run`,
    ],
  };
  for (const [why, [move, message]] of Object.entries(cases)) {
    const chrome = browser();
    person(chrome, (resolution) => {
      move(chrome);
      return approve(resolution);
    });
    const reply = await call(chrome, SAYS);
    assert.equal(reply.error?.message, message, why);
    assert.deepEqual(chrome.ran, [], `${why}: the code ran`);
    assert.deepEqual(codes(chrome), [], `${why}: the code was sent to the browser`);
  }
});

test("the window says the code starts once and that what it sets going keeps running and can read and send without asking again", () => {
  const words = ACTIONS.run_script.words;
  assert.match(
    words,
    /It starts once\. Timers, listeners and observers it sets keep running in the page until the page is closed or reloaded, and each can read and send without asking again\. What it answers is sent to BraveBot$/,
  );
  assert.ok(!/runs once|gone after/.test(words), "the window says the code is gone after a run");
});

test("a script's answer comes back as text, and a long one is cut and says so", () => {
  const cases = [
    ["a string is the string", "Cart", "Cart"],
    ["an object is JSON", { rows: [1, "a"], ok: true }, '{"rows":[1,"a"],"ok":true}'],
    ["a number is JSON", 7, "7"],
    ["null, which is also what a script that threw answers, is null", null, "null"],
    ["no value is no text", undefined, ""],
  ];
  for (const [why, value, text] of cases) {
    assert.deepEqual(resultText(value), { text, cut: false }, why);
  }
  assert.throws(
    () => resultText({ big: 10n }),
    (error) => error.message === "the script answered with a value that cannot be written as JSON",
  );
  const circular = {};
  circular.self = circular;
  assert.throws(() => resultText(circular), /cannot be written as JSON/);

  const exact = "z".repeat(RESULT_LIMIT);
  assert.deepEqual(resultText(exact), { text: exact, cut: false });
  const cut = resultText(`${exact}z`);
  assert.equal(cut.cut, true);
  assert.equal(cut.text.length, RESULT_LIMIT);
  // The cut does not leave half of a character that is two UTF-16 units.
  const pair = `${"z".repeat(RESULT_LIMIT - 1)}\u{1F600}`;
  const halved = resultText(pair);
  assert.equal(halved.cut, true);
  assert.equal(halved.text, "z".repeat(RESULT_LIMIT - 1));
  assert.equal(halved.text.isWellFormed(), true);
});

test("the reply holds a fixed word, the origin and the text of the answer, and says when the text was cut", async () => {
  const chrome = browser({ answer: "q".repeat(RESULT_LIMIT + 5) });
  person(chrome);
  const reply = await call(chrome, SAYS);
  assert.deepEqual(Object.keys(reply.result).sort(), ["cut", "origin", "result", "status"]);
  assert.equal(reply.result.cut, true);
  assert.equal(reply.result.result.length, RESULT_LIMIT);
  const none = browser({ empty: true });
  person(none);
  const empty = await call(none, SAYS);
  assert.equal(empty.error.message, `the page at ${SHOP} returned nothing`);
  const thrown = browser({ answer: null });
  person(thrown);
  assert.deepEqual((await call(thrown, SAYS)).result, { status: "ran", origin: ORIGIN, result: "null" });
});

test("a failure of the browser is told in the browser's words and the world is still reset", async () => {
  const chrome = browser({ failing: new Error(PROTECTED) });
  person(chrome);
  const reply = await call(chrome, SAYS);
  assert.equal(reply.error.message, `the page at ${SHOP} cannot be scripted: ${PROTECTED}`);
  assert.equal(reached(chrome, "userScripts.resetWorldConfiguration"), true);
  assert.equal((await activityLog(chrome))[0].outcome, "failed");
});

// Brave refuses a script on the three origins of its account site and says so as
// the browser's error (brave-core, common/extensions/brave_extensions_client.cc).
// The extension holds no list of them: the person is asked as for any page, and
// the failure the browser gives at the script is told with the address.
test("a page at each of the three origins Brave protects from scripts fails in the browser's words after the person was asked, and the world is reset", async () => {
  for (const url of [
    "https://account.brave.com/",
    "https://account.bravesoftware.com/",
    "https://account.brave.software/",
  ]) {
    const origin = new URL(url).origin;
    const chrome = browser({
      tabs: [{ id: 1, windowId: 1, title: "Account", url }],
      granted: { permissions: ["userScripts"], origins: [`${origin}/*`] },
      stored: { [ALLOWED_SITES_KEY]: [origin] },
      failing: new Error(PROTECTED),
    });
    const asked = person(chrome);
    const reply = await call(chrome, { url, code: CODE });
    assert.equal(reply.result, undefined, url);
    assert.equal(reply.error.message, `the page at ${url} cannot be scripted: ${PROTECTED}`, url);
    assert.equal(asked().length, 1, `${url}: the person was not asked`);
    assert.equal(reached(chrome, "userScripts.execute"), true, url);
    assert.equal(reached(chrome, "userScripts.resetWorldConfiguration"), true, url);
    assert.equal((await activityLog(chrome))[0].outcome, "failed", url);
  }
});

test("a script that does not answer in time fails the call", async () => {
  const chrome = browser({ hanging: true });
  person(chrome);
  let settled;
  const running = call(chrome, SAYS).then((reply) => {
    settled = reply;
  });
  await flush();
  assert.equal(settled, undefined);
  mock.timers.tick(PAGE_WAIT_MS);
  await running;
  assert.equal(
    settled.error.message,
    `the page at ${SHOP} cannot be scripted: the page did not answer within ${PAGE_WAIT_MS / 1000} seconds`,
  );
  assert.equal(reached(chrome, "userScripts.resetWorldConfiguration"), true);
});

test("a second call while the first waits for the person is refused and asks nobody, and the first is not disturbed", async () => {
  const chrome = browser();
  let allow;
  const waiting = new Promise((resolve) => {
    allow = resolve;
  });
  const asked = person(chrome, async (resolution) => {
    await waiting;
    return approve(resolution);
  });
  const first = call(chrome, SAYS);
  await flush();
  const second = await call(chrome, SAYS);
  assert.equal(second.error.message, BUSY_MESSAGE);
  assert.equal(asked().length, 1);
  allow();
  assert.equal((await first).result.status, "ran");
  assert.equal(codes(chrome).length, 1);
  // The place is free again.
  assert.equal((await call(chrome, SAYS)).result.status, "ran");
});

test("the activity log holds the group, the tool, the outcome and the origin, and none of the code or the answer", async () => {
  const chrome = browser({ answer: "the answer text" });
  person(chrome);
  await call(chrome, { url: SHOP, code: "document.querySelector('#secret-id').value" });
  const [entry] = await activityLog(chrome);
  assert.deepEqual(Object.keys(entry).sort(), ["group", "origins", "outcome", "time", "tool"]);
  assert.deepEqual(
    { group: entry.group, tool: entry.tool, outcome: entry.outcome, origins: entry.origins },
    { group: "scripts", tool: "run_script", outcome: "ran", origins: [ORIGIN] },
  );
  const stored = JSON.stringify(chrome.local[ACTIVITY_KEY]);
  assert.ok(!stored.includes("secret-id") && !stored.includes("the answer text"));
});

test("a reference's name is recognised only when it is all the code is", () => {
  for (const name of ["ref:3", " ref:12 ", "\nref:0\n"]) {
    assert.equal(namesAReference(name), true, JSON.stringify(name));
  }
  for (const code of ["document.title", "const ref = 3", "({ref:3})", "ref:3 + 1", "ref:", "xref:3", "ref:3a"]) {
    assert.equal(namesAReference(code), false, code);
  }
});

test("the group is in no preset, runs one tool, reads the allowed sites only and lists no permission but userScripts", () => {
  assert.deepEqual(GROUPS.scripts, {
    permissions: ["userScripts"],
    hosts: true,
    allowedOnly: true,
    tools: ["run_script"],
  });
  assert.deepEqual(Object.keys(TOOLS).filter((tool) => GROUPS.scripts.tools.includes(tool)), ["run_script"]);
  assert.ok(!Object.values(PRESETS).flat().includes("scripts"));
});

test("the manifest offers userScripts as an optional permission only, with no host held at install, and asks for Chrome 135", () => {
  assert.ok(MANIFEST.optional_permissions.includes("userScripts"));
  assert.ok(!MANIFEST.permissions.includes("userScripts"));
  assert.equal(MANIFEST.host_permissions, undefined);
  assert.equal(MANIFEST.minimum_chrome_version, "135");
  assert.equal(MANIFEST.content_scripts, undefined);
  assert.equal(MANIFEST.user_scripts, undefined);
});
