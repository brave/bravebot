// The end of a keep_awake request: released when its time comes, when what is
// saved cannot be read, and when another part of the extension asks.
//
// Run with `node --test extension/tests/`.

import assert from "node:assert/strict";
import { test } from "node:test";
import {
  AWAKE_ALARM,
  AWAKE_KEY,
  keepAwake,
  releaseAwake,
  releaseLapsedAwake,
} from "../awake.js";

const NOW = 1_700_000_000_000;

// A browser with session storage holding `session`, an alarm and, unless
// `power` is false, the power API. `calls` records each call in order, storage
// writes included.
function browser({ session = {}, power = true } = {}) {
  const calls = [];
  return {
    calls,
    session,
    storage: {
      session: {
        async get(key) {
          return key in session ? { [key]: session[key] } : {};
        },
        async set(items) {
          calls.push(["storage.set", Object.keys(items)]);
          Object.assign(session, items);
        },
        async remove(key) {
          calls.push(["storage.remove", key]);
          delete session[key];
        },
      },
    },
    alarms: {
      async create(name, info) {
        calls.push(["alarms.create", name, info]);
      },
      async clear(name) {
        calls.push(["alarms.clear", name]);
        return true;
      },
    },
    ...(power
      ? {
          power: {
            requestKeepAwake(level) {
              calls.push(["power.requestKeepAwake", level]);
            },
            releaseKeepAwake() {
              calls.push(["power.releaseKeepAwake"]);
            },
          },
        }
      : {}),
  };
}

test("a request that has not ended is left standing, and one at the moment it ends is released", async () => {
  const chrome = browser({
    session: { [AWAKE_KEY]: { level: "system", until: NOW } },
  });
  await releaseLapsedAwake(chrome, NOW - 1);
  assert.deepEqual(chrome.calls, []);
  assert.deepEqual(chrome.session[AWAKE_KEY], { level: "system", until: NOW });

  await releaseLapsedAwake(chrome, NOW);
  assert.deepEqual(chrome.calls, [
    ["power.releaseKeepAwake"],
    ["alarms.clear", AWAKE_ALARM],
    ["storage.remove", AWAKE_KEY],
  ]);
  assert.ok(!(AWAKE_KEY in chrome.session));
});

test("with no request saved nothing is released or cleared", async () => {
  const chrome = browser();
  await releaseLapsedAwake(chrome, NOW);
  assert.deepEqual(chrome.calls, []);
});

// A record that is damaged cannot say when the request ends, so it is released.
test("a saved value that is not a request is released", async () => {
  for (const saved of [
    null,
    7,
    "system",
    [],
    {},
    { level: "system" },
    { until: NOW + 1000 },
    { level: "full", until: NOW + 1000 },
    { level: "system", until: "soon" },
    { level: "system", until: Number.NaN },
    { level: "display", until: Infinity },
  ]) {
    const chrome = browser({ session: { [AWAKE_KEY]: saved } });
    await releaseLapsedAwake(chrome, NOW);
    assert.deepEqual(
      chrome.calls.map(([name]) => name),
      ["power.releaseKeepAwake", "alarms.clear", "storage.remove"],
      JSON.stringify(saved),
    );
  }
});

// The permission is gone by the time a lapsed group is turned off in the
// browser's settings, so there may be no power API to release through.
test("a request is forgotten, with its alarm, where the browser has no power API", async () => {
  const chrome = browser({
    session: { [AWAKE_KEY]: { level: "system", until: NOW } },
    power: false,
  });
  await releaseLapsedAwake(chrome, NOW);
  assert.deepEqual(chrome.calls, [
    ["alarms.clear", AWAKE_ALARM],
    ["storage.remove", AWAKE_KEY],
  ]);
});

// A worker stopped after the browser was told to release would leave a record
// for a request that is gone, which is harmless. One stopped before the record
// was forgotten but after the browser was asked is the same. The other order
// would leave a request with no record.
test("a request is released in the browser before it is forgotten", async () => {
  const chrome = browser({
    session: { [AWAKE_KEY]: { level: "display", until: NOW + 1000 } },
  });
  await releaseAwake(chrome);
  assert.deepEqual(chrome.calls, [
    ["power.releaseKeepAwake"],
    ["alarms.clear", AWAKE_ALARM],
    ["storage.remove", AWAKE_KEY],
  ]);
});

test("a request is saved and given an alarm before the browser is asked to keep awake", async () => {
  const chrome = browser();
  const kept = await keepAwake(chrome, "display", 5, NOW);
  assert.deepEqual(kept, { level: "display", until: NOW + 5 * 60 * 1000 });
  assert.deepEqual(chrome.calls, [
    ["storage.set", [AWAKE_KEY]],
    ["alarms.create", AWAKE_ALARM, { when: NOW + 5 * 60 * 1000 }],
    ["power.requestKeepAwake", "display"],
  ]);
});
