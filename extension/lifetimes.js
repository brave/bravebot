// How long a group stays on once a person turns it on: until they turn it off,
// until Brave restarts, or for one hour.
//
// The lifetime is saved with the group's switch and read at each call (handle
// in tools.js), so a group whose time has passed is refused whether or not the
// service worker has turned it off yet, and whether or not an alarm fired.
// What is saved:
//   chrome.storage.local   LIFETIMES_KEY: an object from a group id to its
//                          record, { lifetime } or, for an hour, { lifetime,
//                          expires } with the time in milliseconds since 1970
//   chrome.storage.session RUNS_KEY: an object from a group id to true for each
//                          group turned on until Brave restarts. The browser
//                          empties session storage when it restarts, and when
//                          the extension is reloaded or updated, so a group
//                          with no marker has lapsed.
// A group with no record has the default lifetime of its group.

export const UNTIL_TURNED_OFF = "until-turned-off";
export const UNTIL_RESTART = "until-restart";
export const ONE_HOUR = "one-hour";

// What a lifetime that cannot be read ends as.
export const UNREADABLE = "unreadable";

// The lifetimes a person can choose, in the order the options page offers them.
export const LIFETIMES = Object.freeze([
  UNTIL_TURNED_OFF,
  UNTIL_RESTART,
  ONE_HOUR,
]);

// How long an hour is, in milliseconds.
export const HOUR_MS = 60 * 60 * 1000;

export const LIFETIMES_KEY = "group_lifetimes";
export const RUNS_KEY = "group_runs";

// The groups that reach what a person would want back after a day: they delete
// data, read cookies, run code, act on a page, capture the screen, put files on
// disk or hold page text a person selected. They are on until Brave restarts
// unless the person chooses another lifetime. Every other group is on until it
// is turned off.
const UNTIL_RESTART_BY_DEFAULT = Object.freeze([
  "downloads",
  "cookies",
  "remove-data",
  "scripts",
  "input",
  "input-trusted",
  "capture-trusted",
  "console-trusted",
  "upload-trusted",
  "screen",
  "held",
]);

// The lifetime a group has where the person chooses none.
export function defaultLifetime(id) {
  return UNTIL_RESTART_BY_DEFAULT.includes(id)
    ? UNTIL_RESTART
    : UNTIL_TURNED_OFF;
}

// What a refusal says about a lifetime that has ended, after "turned on".
const ENDED = Object.freeze({
  [UNTIL_RESTART]: "until Brave restarts",
  [ONE_HOUR]: "for one hour",
  [UNREADABLE]: "for a time the extension cannot read",
});

// The words for a lifetime that has ended, as lifetimeEnded names it.
export function endedText(ended) {
  return ENDED[ended];
}

// Throws for a lifetime that is not one of the three.
export function assertLifetime(lifetime) {
  if (!LIFETIMES.includes(lifetime)) {
    throw new Error(`there is no lifetime named ${lifetime}`);
  }
}

// What is saved for a group turned on at `now` with `lifetime`.
export function recordOf(lifetime, now) {
  assertLifetime(lifetime);
  return lifetime === ONE_HOUR
    ? { lifetime, expires: now + HOUR_MS }
    : { lifetime };
}

// An object stored under `key`, or undefined where nothing is.
async function stored(area, key) {
  return (await area.get(key))[key];
}

function isRecordObject(value) {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

// The records saved, or null where what is saved is not an object of them, so a
// value that cannot be read ends every lifetime and does not lift one.
export async function lifetimeRecords(chrome) {
  const value = await stored(chrome.storage.local, LIFETIMES_KEY);
  if (value === undefined) {
    return {};
  }
  return isRecordObject(value) ? value : null;
}

// The groups turned on until Brave restarts that have not been through a
// restart since. Session storage that cannot be read holds none.
export async function runMarkers(chrome) {
  try {
    const value = await stored(chrome.storage.session, RUNS_KEY);
    return isRecordObject(value) ? value : {};
  } catch {
    return {};
  }
}

// What ended the group's lifetime, as the lifetime that ended, or null while it
// has not. A record that is not one of the three, or an hour with no time to
// end at, has ended: a value that cannot be read must not keep a group on. An
// hour whose end is more than an hour away has ended too, since it was saved as
// the hour after the group was turned on and the clock has since been set back.
export async function lifetimeEnded(chrome, id, now = Date.now()) {
  const records = await lifetimeRecords(chrome);
  if (records === null) {
    return UNREADABLE;
  }
  const record = Object.hasOwn(records, id)
    ? records[id]
    : { lifetime: defaultLifetime(id) };
  switch (record?.lifetime) {
    case UNTIL_TURNED_OFF:
      return null;
    case UNTIL_RESTART:
      return (await runMarkers(chrome))[id] === true ? null : UNTIL_RESTART;
    case ONE_HOUR:
      return Number.isFinite(record.expires) &&
        now < record.expires &&
        record.expires - now <= HOUR_MS
        ? null
        : ONE_HOUR;
    default:
      return UNREADABLE;
  }
}
