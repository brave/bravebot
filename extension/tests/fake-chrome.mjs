// A small fake `chrome` for the tests of groups, lifetimes and stored settings:
// the two storage areas, the permissions the person has granted, and the two
// APIs get_platform_info and list_tabs call. `calls` records every permission
// and tab call, so a test can say what a refused call never reached.
//
// `fakeToolbar` is the toolbar action and the keyboard command the service
// worker draws and listens to, for the tests that load background.js.

import { allOptional, fakePermissions } from "./fake-permissions.mjs";
import { fakeArea } from "./fake-storage.mjs";

//   local     what chrome.storage.local holds
//   session   what chrome.storage.session holds
//   held      what the person has granted: { permissions, origins }
//   tabs      the open tabs
//
// `local` and `session` are the objects the areas keep, so a test builds a
// second browser over the same `local` and a new `session` to restart Brave, or
// over both to restart the service worker.
export function fakeChrome({
  local = {},
  session = {},
  held = allOptional(),
  tabs = [],
  decision = true,
} = {}) {
  const calls = [];
  return {
    calls,
    local,
    session,
    permissions: fakePermissions({ held, calls, decision }),
    storage: { local: fakeArea(local), session: fakeArea(session) },
    runtime: {
      async getPlatformInfo() {
        return { os: "mac", arch: "arm64" };
      },
    },
    tabs: {
      async query(filter) {
        calls.push(["tabs.query", filter]);
        return tabs;
      },
    },
  };
}

// Whether the browser was asked for the API.
export function reached(chrome, api) {
  return chrome.calls.some(([name]) => name === api);
}

// The toolbar action and the keyboard commands of a fake browser.
//
//   api     the `action`, `commands` and `windows` members of the fake `chrome`
//   shown   the badge text and the title the toolbar shows now, and `drawn`,
//           every badge text set, in order
//   press   the person using a keyboard command: the listeners run with its name
export function fakeToolbar() {
  const listeners = new Set();
  const shown = { text: "", title: "", drawn: [] };
  return {
    shown,
    api: {
      action: {
        async setBadgeText({ text }) {
          shown.text = text;
          shown.drawn.push(text);
        },
        async setTitle({ title }) {
          shown.title = title;
        },
      },
      commands: {
        onCommand: { addListener: (listener) => listeners.add(listener) },
      },
      // The confirmation window's events, which nothing in these tests fires.
      windows: { onRemoved: { addListener() {} } },
    },
    press(command) {
      for (const listener of [...listeners]) {
        listener(command);
      }
    },
  };
}
