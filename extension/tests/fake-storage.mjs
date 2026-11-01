// A fake `chrome.storage` area, for the tests that run the extension against a
// fake browser. `local` and `session` are two areas that keep what is saved in
// objects of their own, as the browser keeps them apart. Session storage is
// emptied when the browser restarts, so a test that restarts it builds a new
// browser with an empty `session` and the same `local` object.

// The area `get` and `set` of one object, read and written by key. `data` is
// the object, for a test that reads or changes what is stored. `announce`, where
// given, is called with the changes a `set` made, as the browser's
// `storage.onChanged` is, once they are stored.
export function fakeArea(data = {}, announce = undefined) {
  return {
    data,
    async get(key) {
      return key in data ? { [key]: data[key] } : {};
    },
    async set(items) {
      const changes = Object.fromEntries(
        Object.entries(items).map(([key, newValue]) => [
          key,
          { ...(key in data ? { oldValue: data[key] } : {}), newValue },
        ]),
      );
      Object.assign(data, items);
      announce?.(changes);
    },
    async remove(key) {
      delete data[key];
    },
  };
}
