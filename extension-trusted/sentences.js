// What a reply can say. A reply holds a kind and one sentence from this table,
// with at most an argument's name from a closed list or a count, and nothing
// the page or the request wrote. An error carries a code, and the reply is made
// from the code. The error's own message is never sent.

// The arguments a refusal can name. A request's own keys are never named.
export const ARGUMENTS = Object.freeze([
  "url",
  "role",
  "name",
  "button",
  "count",
  "text",
  "key",
  "onto_role",
  "onto_name",
  "path",
  "size",
  "file_name",
]);

// The sentence for each code, from the detail the error carries.
export const SENTENCES = Object.freeze({
  request: () => "the request is not one this extension answers",
  argument: (name) =>
    `the argument ${ARGUMENTS.includes(name) ? name : "of the request"} is missing or is not allowed`,
  extra: () => "the request has an argument this method does not take",
  busy: () => "another request is being answered, and this one was not started",
  method: () =>
    "the browser was asked for a method this extension does not use",
  not_allowed: () => "the person has not allowed this",
  denied: () => "it was denied in the confirmation window, so nothing was done",
  closed: () => "the confirmation window was closed, so nothing was done",
  timed_out: () =>
    "the confirmation window was not answered in time, so nothing was done",
  waiting: () => "another confirmation window is open, so nothing was done",
  no_window: () =>
    "the confirmation window could not be opened, so nothing was done",
  dialog_open: () =>
    "the page opened a dialog that was not answered, so it is still open in the page",
  ended: () => "the session with the tab has ended",
  tabs: (count) =>
    `${Number.isInteger(count) ? count : 0} open tabs are at that URL, and exactly one is needed`,
  failed: () => "the request could not be completed",
  slow: () =>
    "the browser did not answer in time before you were asked, so nothing was done",
  late: () =>
    "the browser did not finish in time after you allowed it, so the action may or may not have been done",
  none: () =>
    "no element has that role and that name in the top document of the page, and frames are not searched",
  several: (count) =>
    `${Number.isInteger(count) ? count : 0} elements have that role and that name, so the name does not name one`,
  hidden: () =>
    "the only element with that role and that name is hidden or has no size",
  covered: () => "another element covers the middle of it",
  offscreen: () => "the middle of it is outside the part of the page in view",
  disabled: () => "it is disabled or read-only",
  password: () => "it is a password field, which is never acted on",
  card: () => "it is a payment card field, which is never acted on",
  file: () => "it is a file chooser, which is never acted on",
  notfile: () => "it is not a file input, so a file cannot be chosen on it",
  credential: () =>
    "the name of the file looks like a credential, a key or a secret, so it is never given to a page",
  changed: () => "the element changed after it was found, so nothing was done",
  left: () => "the page left the URL before it was acted on",
  long: () =>
    "its name, its text or where it goes is longer than 2000 characters, so the window cannot show it whole and nothing was done",
  too_big: () => "the page has more elements than can be looked through",
  nofocus: () => "no element has the focus, so there is nothing to press a key on",
  unsupported: () => "it is not a text field, so text cannot be typed into it",
  empty: () => "the page has no size, so there is nothing to take a picture of",
  proportions: () =>
    "the page is too long and narrow for a picture of at most 2048 pixels on its long side",
  picture_big: () =>
    "the picture is over 8 MiB, which is the most a picture may be, so none was returned",
  picture_size: () =>
    "the browser's picture was over 2048 pixels on its long side, so none was returned",
});

// A request one of this extension's own rules stopped.
export class Refusal extends Error {
  constructor(code, detail) {
    super(code);
    this.code = code;
    this.detail = detail;
  }
}

// A request that was tried and did not complete.
export class Failure extends Error {
  constructor(code, detail) {
    super(code);
    this.code = code;
    this.detail = detail;
  }
}

// The reply's error for whatever was thrown. Anything that is not a Refusal or a
// Failure, or that carries a code the table lacks, is the generic failure.
export function errorOf(error) {
  const known =
    (error instanceof Refusal || error instanceof Failure) &&
    Object.hasOwn(SENTENCES, error.code);
  if (!known) {
    return { kind: "failed", message: SENTENCES.failed() };
  }
  return {
    kind: error instanceof Refusal ? "refusal" : "failed",
    message: SENTENCES[error.code](error.detail),
  };
}
