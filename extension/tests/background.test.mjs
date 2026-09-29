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

function port() {
  return {
    onMessage: event(),
    onDisconnect: event(),
    postMessage() {},
  };
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
