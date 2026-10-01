// The service worker against fake extension events and native messaging ports.

import assert from "node:assert/strict";
import { test } from "node:test";

function event() {
  const listeners = [];
  return {
    addListener(listener) {
      listeners.push(listener);
    },
    fire(...arguments_) {
      for (const listener of listeners) {
        listener(...arguments_);
      }
    },
  };
}

// A port that records what is posted to it, and refuses once disconnected as
// a real one does.
function port() {
  const made = {
    sent: [],
    attempts: 0,
    disconnected: false,
    onMessage: event(),
    onDisconnect: event(),
    postMessage(message) {
      made.attempts += 1;
      if (made.disconnected) {
        throw new Error("Attempting to use a disconnected port object");
      }
      made.sent.push(message);
    },
  };
  made.onDisconnect.addListener(() => {
    made.disconnected = true;
  });
  return made;
}

// Lets queued promise callbacks run, a bounded number of times, until
// `condition` holds.
async function settle(condition) {
  for (let turn = 0; turn < 100 && !condition(); turn += 1) {
    await new Promise((resolve) => setImmediate(resolve));
  }
  return condition();
}

test("the worker connects at each start and after a disconnect", async () => {
  const connected = [];
  const madeAlarms = [];
  const runtime = {
    lastError: undefined,
    onStartup: event(),
    onInstalled: event(),
    connectNative(host) {
      const connectedPort = port();
      connected.push({ host, port: connectedPort });
      return connectedPort;
    },
  };
  const alarms = {
    onAlarm: event(),
    create(name, options) {
      madeAlarms.push({ name, options });
    },
  };
  const previousChrome = globalThis.chrome;
  const previousInfo = console.info;
  globalThis.chrome = { runtime, alarms };
  console.info = () => {};
  try {
    await import(`../background.js?test=${Date.now()}`);
    assert.equal(connected.length, 1);
    assert.equal(connected[0].host, "com.brave.bravebot");
    assert.deepEqual(madeAlarms, [
      { name: "reconnect", options: { periodInMinutes: 1 } },
    ]);

    runtime.onStartup.fire();
    assert.equal(connected.length, 1, "an open port was replaced");
    connected[0].port.onDisconnect.fire();
    runtime.onStartup.fire();
    assert.equal(connected.length, 2);

    connected[1].port.onDisconnect.fire();
    alarms.onAlarm.fire({ name: "reconnect" });
    assert.equal(connected.length, 3);
  } finally {
    globalThis.chrome = previousChrome;
    console.info = previousInfo;
  }
});

// A request answered after its port closed and another opened goes back on
// its own port, which refuses it, and never to the new port's host. That host
// numbers its requests from 1 as well, so a reply reaching it could be taken
// for the answer to a request of its own.
test("a reply goes back on the port its request came in on", async () => {
  const connected = [];
  let release;
  const read = new Promise((resolve) => {
    release = resolve;
  });
  const runtime = {
    lastError: undefined,
    onStartup: event(),
    onInstalled: event(),
    connectNative() {
      const made = port();
      connected.push(made);
      return made;
    },
  };
  const previousChrome = globalThis.chrome;
  const previousInfo = console.info;
  globalThis.chrome = {
    runtime,
    alarms: { onAlarm: event(), create() {} },
    storage: { local: { get: () => read } },
    tabs: { query: async () => [] },
  };
  console.info = () => {};
  try {
    await import(`../background.js?reply=${Date.now()}`);
    const [first] = connected;
    first.onMessage.fire({ id: 5, method: "list_tabs", params: {} });
    first.onDisconnect.fire();
    runtime.onStartup.fire();
    const second = connected[1];
    assert.ok(second, "the worker did not reconnect");

    release({});
    assert.ok(await settle(() => first.attempts === 1));
    assert.deepEqual(second.sent, [], "a reply reached the new port's host");

    second.onMessage.fire({ id: 1, method: "list_tabs", params: {} });
    assert.ok(await settle(() => second.sent.length === 1));
    assert.equal(second.sent[0].id, 1);
  } finally {
    globalThis.chrome = previousChrome;
    console.info = previousInfo;
  }
});
