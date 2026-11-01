// The activity log: the last calls that reached the extension, kept in
// storage.local for the options page to show.
//
// An entry holds when the call arrived, the group and tool it named, what came
// of it, and the origins of the URLs it named. It holds nothing of the
// arguments or the result beyond those origins, so no page text, search text
// or title is stored. A call is logged whether or not its group is on, and
// whether or not the person is watching the session that made it, since a
// refused call is what tells them a session tried.
//
// Every function takes the `chrome` object it calls, as tools.js does.

// Where the log is kept.
export const ACTIVITY_KEY = "activity_log";

// The most entries kept. The oldest is dropped to make room.
export const ACTIVITY_LIMIT = 200;

// The most origins one entry keeps, for a call that names many URLs.
export const ORIGINS_PER_ENTRY = 10;

// The most characters of an origin an entry keeps. A host name is at most 253,
// so an origin that is cut here was not a real one, and the 200 entries of ten
// origins stay under about 1 MB of the 10 MB storage.local holds.
export const ORIGIN_LENGTH_LIMIT = 255;

// What came of a call: the tool ran, the extension refused it by one of its
// rules (a switch, the group, its lifetime, a list of sites, the browser's
// permissions), or it was tried and did not complete.
export const RAN = "ran";
export const REFUSED = "refused";
export const FAILED = "failed";
const OUTCOMES = [RAN, REFUSED, FAILED];

// Whether a stored entry has the shape recordCall writes. A group or tool is
// null where the call named none the extension has.
function wellFormed(entry) {
  return (
    typeof entry === "object" &&
    entry !== null &&
    Number.isFinite(entry.time) &&
    Number.isFinite(new Date(entry.time).getTime()) &&
    (entry.group === null || typeof entry.group === "string") &&
    (entry.tool === null || typeof entry.tool === "string") &&
    OUTCOMES.includes(entry.outcome) &&
    Array.isArray(entry.origins) &&
    entry.origins.every((origin) => typeof origin === "string")
  );
}

// The entries kept, oldest first. A stored value that is not a list, and an
// entry in it that is not well formed, is left out.
export async function activityLog(chrome) {
  const stored = (await chrome.storage.local.get(ACTIVITY_KEY))[ACTIVITY_KEY];
  return Array.isArray(stored) ? stored.filter(wellFormed) : [];
}

// Calls are added one at a time: two that finish together would each read the
// log as it was and the later write would drop the earlier entry.
let pending = Promise.resolve();

// Adds one call to the log, dropping the oldest past the limit. Only the five
// fields are stored, whatever else the argument holds. A log that cannot be
// written is reported and does not fail the call it describes.
export function recordCall(chrome, { time, group, tool, outcome, origins }) {
  const entry = {
    time,
    group,
    tool,
    outcome,
    origins: origins
      .slice(0, ORIGINS_PER_ENTRY)
      .map((origin) => origin.slice(0, ORIGIN_LENGTH_LIMIT)),
  };
  const written = pending.then(async () => {
    const log = await activityLog(chrome);
    log.push(entry);
    await chrome.storage.local.set({
      [ACTIVITY_KEY]: log.slice(-ACTIVITY_LIMIT),
    });
  });
  pending = written.catch(() => {});
  return written.catch((error) => {
    console.info("bravebot activity not logged:", error?.message);
  });
}
