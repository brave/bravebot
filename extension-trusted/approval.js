// Who may say an action is allowed, and the numbers the confirmation window and
// the worker that opens it agree on. This file is copied byte for byte into the
// second extension (extension-trusted/), so it imports nothing; shared-files.json
// lists every file that is.

// How long the person has to answer the confirmation window before it denies,
// in milliseconds. The relay's wait for a reply from the extension has to be
// longer than this, with room for finding the element and acting on it.
export const CONFIRMATION_WAIT_MS = 60_000;

// The name of the port the confirmation window connects to the service worker
// with, how long after the window is shown, or comes back to the front, Allow
// once stays unavailable, and how often the window tells the worker it is still
// there, which keeps the worker running while it waits.
export const CONFIRMATION_PORT = "bravebot-confirmation";
export const ALLOW_DELAY_MS = 1_000;
export const HEARTBEAT_MS = 15_000;

// Who may say an action is allowed: a token made for one resolution, by the
// window that put the element in front of the person, and used once. The tool
// that acts refuses without one, and a token made for one resolution does not
// serve another.
const approvals = new WeakMap();

export function approve(resolution) {
  const token = Object.freeze({});
  approvals.set(token, resolution);
  return token;
}

// Whether `token` was made for `resolution`. A token is spent by the first
// check, whatever it says.
export function spend(token, resolution) {
  if (typeof token !== "object" || token === null) {
    return false;
  }
  const made = approvals.get(token);
  approvals.delete(token);
  return made !== undefined && made === resolution;
}

// The window that asks the person. It is given the resolution, whose
// `description` is what to show, and answers with approve(resolution) when the
// person allows the action. When they do not it answers with null, or throws a
// refusal that says why (confirmation.js does). Until a window is set, and after
// setConfirmation() with none, nobody allows anything.
const nobody = async () => null;
let askPerson = nobody;

export function setConfirmation(ask = nobody) {
  askPerson = ask;
}

export function confirm(resolution) {
  return askPerson(resolution);
}
