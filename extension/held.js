// What a person holds out in the browser for a session to fetch: a selection, a
// link or a tab. The person uses one of the extension's entries in the
// browser's context menu, and the extension keeps one item until take_held
// returns it or it is forgotten.
//
// The group lists `contextMenus` and no host, and the extension reads nothing
// from a page. The browser hands it the text selected, the address of the link or
// of the page where the person used the entry, and nothing else. A web page can
// neither use the entry nor send the extension a message that makes an item, so
// an item exists only because the person made it. What an item holds is still
// the page's: the text is whatever the page put in the selected range, hidden
// parts included, and the addresses are the page's.
//
// The items are kept in session storage, which the browser holds in memory and
// never writes to disk, and empties when it restarts and when the extension is
// reloaded or updated. They are also forgotten when the group stops being on
// (forgetHeld, called by giveBack in switches.js), when it is turned on again,
// and when their time has passed.
//   chrome.storage.session   HELD_KEY: { items, forgotten } with the items
//                            oldest first and, since the last take, how many
//                            were forgotten for their age or for the limit
//
// Every function takes the `chrome` object it calls, as tools.js does.

// The most items kept. Holding another forgets the oldest.
export const MAX_ITEMS = 10;

// The most characters of a selection kept, and of an address. A longer one is
// cut and the item says so.
export const MAX_TEXT = 20_000;
export const MAX_ADDRESS = 2_000;

// How long an item is kept, in milliseconds.
export const MAX_AGE_MS = 30 * 60 * 1000;

export const HELD_KEY = "held_items";

// The entries the extension adds to the browser's context menu while the group
// is on. A selection and a link are held where the person used the entry on
// one, and a tab on the page itself.
export const MENU = Object.freeze([
  {
    id: "bravebot-hold-selection",
    kind: "selection",
    title: "Hold this selection for BraveBot",
    contexts: ["selection"],
  },
  {
    id: "bravebot-hold-link",
    kind: "link",
    title: "Hold this link for BraveBot",
    contexts: ["link"],
  },
  {
    id: "bravebot-hold-tab",
    kind: "tab",
    title: "Hold this tab for BraveBot",
    contexts: ["page", "frame"],
  },
]);

// `value` cut to `limit` UTF-16 code units, one fewer where the last of them
// would be the first half of a character, and whether it was cut.
function limited(value, limit) {
  if (value.length <= limit) {
    return { value, cut: false };
  }
  const kept = value.slice(0, limit);
  const last = kept.charCodeAt(kept.length - 1);
  return {
    value: last >= 0xd800 && last <= 0xdbff ? kept.slice(0, -1) : kept,
    cut: true,
  };
}

// The origin of an address, or null where it has none.
function originOfAddress(address) {
  try {
    const { origin } = new URL(address);
    return origin === "null" ? null : origin;
  } catch {
    return null;
  }
}

// The item a use of an entry makes, or null where it makes none: the entry is
// not one of ours, or the browser gave nothing to hold. `page` is the address of
// the page the entry was used on, and `address` the one the item is about.
export function itemFrom(info, now = Date.now()) {
  const entry = MENU.find((candidate) => candidate.id === info?.menuItemId);
  if (!entry) {
    return null;
  }
  const given = (value) =>
    typeof value === "string" && value !== "" ? value : null;
  const page = given(info.pageUrl);
  const item = { kind: entry.kind, time: now };
  let cut = false;
  const keep = (name, value, limit) => {
    const kept = limited(value, limit);
    item[name] = kept.value;
    cut = cut || kept.cut;
  };
  switch (entry.kind) {
    case "selection": {
      const text = given(info.selectionText);
      if (text === null) {
        return null;
      }
      keep("text", text, MAX_TEXT);
      break;
    }
    case "link": {
      const address = given(info.linkUrl);
      if (address === null) {
        return null;
      }
      keep("address", address, MAX_ADDRESS);
      break;
    }
    default:
      if (page === null) {
        return null;
      }
      break;
  }
  if (page !== null) {
    keep("page", page, MAX_ADDRESS);
    // The address is cut, so the site it names is read from the whole of it.
    const origin = originOfAddress(page);
    if (origin !== null) {
      item.origin = origin;
    }
  }
  return cut ? { ...item, cut: true } : item;
}

function isItem(value) {
  return (
    typeof value === "object" &&
    value !== null &&
    MENU.some((entry) => entry.kind === value.kind) &&
    Number.isFinite(value.time) &&
    ["text", "address", "page", "origin"].every(
      (name) => value[name] === undefined || typeof value[name] === "string",
    )
  );
}

// What is kept, with anything that cannot be read as an item left out. A record
// that cannot be read holds nothing.
async function saved(chrome) {
  const record = (await chrome.storage.session.get(HELD_KEY))[HELD_KEY];
  if (typeof record !== "object" || record === null) {
    return { items: [], forgotten: 0 };
  }
  const items = Array.isArray(record.items) ? record.items.filter(isItem) : [];
  const forgotten = Number.isSafeInteger(record.forgotten)
    ? Math.max(record.forgotten, 0)
    : 0;
  return { items, forgotten };
}

// Whether an item made at `time` is still kept at `now`. One made after `now`
// was made by a clock that has since been set back, and is not.
function current(item, now) {
  return now - item.time >= 0 && now - item.time <= MAX_AGE_MS;
}

// The writers of this context, one at a time, so that two uses of an entry and a
// take do not each write back a copy read before the other wrote. A take is the
// only reader that empties the record, and it runs in the service worker with
// the uses of the entries.
let writers = Promise.resolve();

function serialised(task) {
  const run = writers.then(task);
  writers = run.catch(() => {});
  return run;
}

// Keeps the item `info` makes, after the ones whose time has passed are
// forgotten, and forgets the oldest if that is more than MAX_ITEMS. It answers
// whether an item was kept.
export function holdItem(chrome, info, now = Date.now()) {
  return serialised(async () => {
    const item = itemFrom(info, now);
    if (item === null) {
      return false;
    }
    const before = await saved(chrome);
    const fresh = before.items.filter((kept) => current(kept, now));
    const items = [...fresh, item].slice(-MAX_ITEMS);
    const forgotten =
      before.forgotten + (before.items.length + 1 - items.length);
    await chrome.storage.session.set({ [HELD_KEY]: { items, forgotten } });
    return true;
  });
}

// The items kept and still current, oldest first, and how many were forgotten
// since the last take. What is kept is emptied.
export function takeHeld(chrome, now = Date.now()) {
  return serialised(async () => {
    const before = await saved(chrome);
    await chrome.storage.session.remove(HELD_KEY);
    const items = before.items.filter((kept) => current(kept, now));
    return {
      items,
      forgotten: before.forgotten + (before.items.length - items.length),
    };
  });
}

// Forgets every item. The popup and the options page call it through
// switches.js, in their own contexts, so it does not wait for the writers of
// the service worker.
export async function forgetHeld(chrome) {
  await chrome.storage.session.remove(HELD_KEY);
}

// How many items are kept now, for the popup to say.
export async function heldCount(chrome, now = Date.now()) {
  return (await saved(chrome)).items.filter((kept) => current(kept, now))
    .length;
}

// What the browser said to a call of the context menu API that took the callback
// form, which every version the extension runs on has, as the settling of a
// promise: its error if it reported one.
function settled(chrome, resolve, reject) {
  return () => {
    const error = chrome.runtime?.lastError;
    if (error) {
      reject(new Error(error.message));
    } else {
      resolve();
    }
  };
}

// The menu changes of this context, one at a time, so that two that overlap do
// not both create an entry.
let changes = Promise.resolve();

// Makes the entries exist while the group is `on`, and removes them otherwise.
// Does nothing where the browser has no `contextMenus`, which is so while the
// permission is not held. The entries are created after the old ones are
// removed, so that none exists twice.
export function showMenus(chrome, on) {
  const run = changes.then(async () => {
    if (!chrome.contextMenus) {
      return;
    }
    await new Promise((resolve, reject) => {
      chrome.contextMenus.removeAll(settled(chrome, resolve, reject));
    });
    if (on) {
      for (const { id, title, contexts } of MENU) {
        await new Promise((resolve, reject) => {
          chrome.contextMenus.create(
            { id, title, contexts },
            settled(chrome, resolve, reject),
          );
        });
      }
    }
  });
  changes = run.catch(() => {});
  return run;
}

// The `chrome` objects whose entries are listened to.
const listening = new WeakSet();

// Keeps the item of each use of an entry, where `mayHold` answers true: that the
// group is on. The browser reports an event only to a listener registered as
// the worker starts, so this is called then and again when the permission is
// granted. An entry that outlived its group, which is possible if the browser
// does not remove an entry when it takes the permission back, holds nothing.
// Turning the group off forgets what is held, so an item written after that is
// forgotten again when `mayHold` no longer answers true once it is written.
export function listenForHolds(chrome, mayHold) {
  if (!chrome.contextMenus || listening.has(chrome)) {
    return;
  }
  listening.add(chrome);
  chrome.contextMenus.onClicked.addListener((info) => {
    mayHold()
      .then(async (may) => {
        if (may === true && (await holdItem(chrome, info))) {
          if ((await mayHold()) !== true) {
            await forgetHeld(chrome);
          }
        }
      })
      .catch((error) => {
        console.info("bravebot item not held:", error?.message);
      });
  });
}
