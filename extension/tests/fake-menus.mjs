// A fake context menu for the tests of the held group: the entries that exist,
// every call to it in order, and the listeners of onClicked, which `click` runs
// as the browser does when the person uses an entry.
//
// Given the permission calls of the fake (`permissions`), the API is there only
// while the browser holds `contextMenus`, and a call after it is taken back
// throws as Brave's does: `contextMenus` stays an object and its methods say
// they are not available in this context. `calls`, where given, records each
// call beside the permission calls, so a test sees which came first.
//
//   failWith   the error the browser reports to the next removeAll, which a
//              callback reads from chrome.runtime.lastError
//
// With `absentUntilHeld`, `chrome.contextMenus` is undefined until the browser
// holds the permission, as in a worker started before the person granted it.

export function withMenus(
  chrome,
  { calls = [], permissions, absentUntilHeld = false } = {},
) {
  const entries = new Map();
  const listeners = [];
  chrome.runtime ??= {};
  chrome.runtime.lastError = undefined;
  const settle = (done, error) => {
    chrome.runtime.lastError = error ? { message: error } : undefined;
    done();
    chrome.runtime.lastError = undefined;
  };
  const available = (method) => {
    if (permissions && !permissions.held.permissions.has("contextMenus")) {
      throw new Error(`'contextMenus.${method}' is not available in this context.`);
    }
  };
  const menus = {
    failWith: undefined,
    removeAll(done) {
      available("removeAll");
      calls.push(["contextMenus.removeAll"]);
      entries.clear();
      const error = menus.failWith;
      menus.failWith = undefined;
      settle(done, error);
    },
    create(properties, done) {
      available("create");
      calls.push(["contextMenus.create", properties.id]);
      entries.set(properties.id, properties);
      settle(done);
    },
    onClicked: { addListener: (listener) => listeners.push(listener) },
  };
  if (absentUntilHeld) {
    Object.defineProperty(chrome, "contextMenus", {
      configurable: true,
      get: () => (permissions.held.permissions.has("contextMenus") ? menus : undefined),
    });
  } else {
    chrome.contextMenus = menus;
  }
  return {
    entries,
    calls,
    listeners,
    menus,
    click: (info) => listeners.forEach((listener) => listener(info, {})),
  };
}
