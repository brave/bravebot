// The request keep_awake makes to keep the computer from sleeping, and the end
// of it.
//
// The browser's `power` API has no time limit: a request stands until the
// extension releases it or is unloaded. So the request is made with a time it
// ends at, saved in session storage, and the extension releases it when that
// time comes, on the alarm made for it, at each start of the service worker and
// on the alarm that reconnects the port. Session storage is emptied when the
// browser restarts, which also ends the request, and when the extension is
// reloaded or updated, which unloads it.
//
// The saved record is the one handle on the request. releaseAwake is how
// anything else, such as the end of a group's lifetime, ends it.
//   chrome.storage.session   AWAKE_KEY: { level, until } with the time in
//                            milliseconds since 1970, or no value while no
//                            request stands

// The levels of the browser's `power` API: the system stays awake, or the
// display stays on as well.
export const LEVELS = Object.freeze(["system", "display"]);

// The most minutes one request may last. A longer time is asked for again.
export const MAX_AWAKE_MINUTES = 240;

export const AWAKE_KEY = "keep_awake";

// The name of the alarm made for the end of a request.
export const AWAKE_ALARM = "keep-awake";

const MINUTE_MS = 60 * 1000;

// Whether `value` is a request: a level of the browser's and a time it ends at.
function isRequest(value) {
  return LEVELS.includes(value?.level) && Number.isFinite(value.until);
}

// Keeps the computer awake at `level` for `minutes`, in place of a request that
// stands. The end is saved and the alarm made before the browser is asked, so a
// worker stopped part way leaves a record to release and never a request with
// none.
export async function keepAwake(chrome, level, minutes, now = Date.now()) {
  const until = now + minutes * MINUTE_MS;
  await chrome.storage.session.set({ [AWAKE_KEY]: { level, until } });
  await chrome.alarms.create(AWAKE_ALARM, { when: until });
  chrome.power.requestKeepAwake(level);
  return { level, until };
}

// Releases the request that stands, and forgets it. The browser is asked first,
// while the permission is still held, since `chrome.power` is gone once it is
// taken back. Does nothing to the browser where it has no `power`.
export async function releaseAwake(chrome) {
  chrome.power?.releaseKeepAwake();
  await chrome.alarms.clear(AWAKE_ALARM);
  await chrome.storage.session.remove(AWAKE_KEY);
}

// Releases the request if its time has come, or if what is saved cannot be read
// as a request. Run at each start of the worker and on each alarm.
export async function releaseLapsedAwake(chrome, now = Date.now()) {
  const saved = (await chrome.storage.session.get(AWAKE_KEY))[AWAKE_KEY];
  if (saved !== undefined && (!isRequest(saved) || now >= saved.until)) {
    await releaseAwake(chrome);
  }
}
