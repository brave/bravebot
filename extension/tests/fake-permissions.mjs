// A fake `chrome.permissions` for the tests that run the extension against a
// fake browser. It holds the optional permissions and hosts the person has
// granted, answers `contains` and `getAll`, and keeps `request` and `remove` to
// what the real calls return and fire. `getAll` lists only what was granted,
// where the browser's also lists the permissions the manifest requires.
//
// A host pattern counts as held when it is held as written, when a pattern for
// all hosts of its scheme, such as https://*/*, is held, or when <all_urls> is
// held and the pattern is for a web scheme. Removing a pattern
// removes only that pattern as written, so a site granted while all hosts were
// held is not held again once they are taken back.

import { readFileSync } from "node:fs";

const manifest = JSON.parse(
  readFileSync(new URL("../manifest.json", import.meta.url), "utf8"),
);

// Everything the manifest lets a person grant, for a test that has granted it
// all, as a fresh browser holds nothing of it.
export function allOptional() {
  return {
    permissions: [...manifest.optional_permissions],
    origins: [...manifest.optional_host_permissions],
  };
}

// Whether the held host pattern `held` covers `asked`.
function covers(held, asked) {
  return (
    held === asked ||
    (held === "<all_urls>" && /^(https?|\*):\/\//.test(asked)) ||
    (held.endsWith("://*/*") && asked.startsWith(held.slice(0, -3)))
  );
}

// The `permissions` API of a fake browser.
//
//   held      what the person has granted: { permissions, origins }
//   calls     the list the browser records each call in, as [name, query]
//   decision  what `request` answers: true grants it, false is the person
//             declining, and an Error is thrown, as the browser does for a
//             request an organization's policy blocks
//
// The returned object has three members that are not part of the real API: `held`,
// the sets a test reads to see what is granted now, `revoke`, which is the
// person taking a permission back in the browser's extension settings: it
// removes what is named and fires `onRemoved`, and records no call, and
// `reaches`, which says whether a held host pattern covers a page's URL, as the
// browser decides when it hands the extension a tab's address.
export function fakePermissions({
  held = {},
  calls = [],
  decision = true,
} = {}) {
  const state = {
    permissions: new Set(held.permissions ?? []),
    origins: new Set(held.origins ?? []),
  };
  const listeners = { onAdded: new Set(), onRemoved: new Set() };
  const asked = (query) => ({
    permissions: query?.permissions ?? [],
    origins: query?.origins ?? [],
  });
  const announce = (event, query) => {
    for (const listener of [...listeners[event]]) {
      listener(asked(query));
    }
  };
  const take = (query) => {
    const { permissions, origins } = asked(query);
    permissions.forEach((name) => state.permissions.delete(name));
    origins.forEach((pattern) => state.origins.delete(pattern));
    announce("onRemoved", query);
    return true;
  };
  const event = (name) => ({
    addListener: (listener) => listeners[name].add(listener),
    removeListener: (listener) => listeners[name].delete(listener),
    hasListener: (listener) => listeners[name].has(listener),
  });
  return {
    held: state,
    reaches(url) {
      const origin = URL.canParse(url) ? new URL(url).origin : "null";
      return [...state.origins].some((held) => covers(held, `${origin}/*`));
    },
    async contains(query) {
      calls.push(["permissions.contains", query]);
      const { permissions, origins } = asked(query);
      return (
        permissions.every((name) => state.permissions.has(name)) &&
        origins.every((pattern) =>
          [...state.origins].some((held) => covers(held, pattern)),
        )
      );
    },
    async request(query) {
      calls.push(["permissions.request", query]);
      if (decision instanceof Error) {
        throw decision;
      }
      if (decision !== true) {
        return false;
      }
      const { permissions, origins } = asked(query);
      permissions.forEach((name) => state.permissions.add(name));
      origins.forEach((pattern) => state.origins.add(pattern));
      announce("onAdded", query);
      return true;
    },
    async getAll() {
      calls.push(["permissions.getAll"]);
      return {
        permissions: [...state.permissions],
        origins: [...state.origins],
      };
    },
    async remove(query) {
      calls.push(["permissions.remove", query]);
      return take(query);
    },
    async revoke(query) {
      return take(query);
    },
    onAdded: event("onAdded"),
    onRemoved: event("onRemoved"),
  };
}
