// The numbers list_page_elements gives the elements of a page, and the list a
// number refers to.
//
// A person reads the list and tells the session which number to act on, as they
// would tell it a URL. The extension keeps what it needs to find each listed
// element again: where it is in the document and its fingerprint, which is the
// tag, role, name, place and destination that inPage compares before it acts.
// Nothing else of the page is kept: no text, and no value a field holds.
//
// There is one list. A second list_page_elements replaces the first, whatever
// page it was made for, so a number refers to one page and one list. The list
// is kept in memory that ends with the browser (chrome.storage.session), for
// MAX_AGE_MS, and is forgotten when a group that lists or uses numbers is turned
// off (switches.js). A number resolves to an element only on the page the list
// was made for, and only if that element is still the one that was listed.
//   chrome.storage.session   NUMBERS_KEY: { url, at, elements } with the time in
//                            milliseconds since 1970, and for each element, in
//                            the order of its number, { path, fingerprint }

import { Refusal, ToolError } from "./errors.js";
import { shownParts } from "./shown.js";

export const NUMBERS_KEY = "element_numbers";

// The most elements a list numbers, which is also the largest element_number.
export const MAX_LISTED = 40;

// The most characters of a name a list shows.
export const LISTED_NAME_LIMIT = 120;

// The longest fingerprint an element can have and be numbered. A longer one is
// left out of the list, so that what is kept is small and compared whole.
export const FINGERPRINT_LIMIT = 2_000;

// How long a list can be used for, in milliseconds. The person reads it and
// tells the session which number, and the call is then put to them and to the
// window; the element is compared again at the moment of the action, so this
// bounds how long page text is kept and not whether an element is the one listed.
export const MAX_AGE_MS = 300_000;

// What inPage is given for the step "list".
export const LISTING_LIMITS = Object.freeze({
  listed: MAX_LISTED,
  listName: LISTED_NAME_LIMIT,
  fingerprint: FINGERPRINT_LIMIT,
});

// The tools that make or use the numbers. A group that lists one of them
// forgets the list when it is turned off.
export const NUMBERED_TOOLS = Object.freeze([
  "list_page_elements",
  "click_element",
  "type_text",
  "select_option",
]);

// Why a number leads to no element. Each is a sentence of ours, with no word of
// the page in it.
export const NOT_FOUND = Object.freeze({
  none: "no list of the elements of a page has been made, so no element has that number; call list_page_elements for the page first",
  other:
    "the last list of elements was made for another page, so its numbers do not apply to this one; call list_page_elements for this page first",
  old: `the last list of elements is more than ${MAX_AGE_MS / 60_000} minutes old; call list_page_elements again`,
  past: (count) =>
    `the last list has ${count} elements, so no element has that number`,
});

// Whether `value` is a list this module made: an address, a time and at most
// MAX_LISTED elements, each with a place and a fingerprint that are text.
function isList(value) {
  return (
    typeof value?.url === "string" &&
    Number.isFinite(value.at) &&
    Array.isArray(value.elements) &&
    value.elements.length <= MAX_LISTED &&
    value.elements.every(
      (entry) =>
        typeof entry?.path === "string" &&
        typeof entry.fingerprint === "string" &&
        entry.fingerprint.length <= FINGERPRINT_LIMIT,
    )
  );
}

// Keeps the elements inPage listed for the page at `url`, in place of any list
// there was.
export async function remember(chrome, url, listed, now = Date.now()) {
  const elements = listed
    .slice(0, MAX_LISTED)
    .map(({ path, fingerprint }) => ({ path, fingerprint }));
  const list = { url, at: now, elements };
  if (!isList(list)) {
    throw new ToolError("the page answered something the extension does not know");
  }
  await chrome.storage.session.set({ [NUMBERS_KEY]: list });
}

// The place and the fingerprint of the element that has this number in the list
// kept for the page at `url`. A call with no list, a list made for another
// page, a list that has aged, a list that cannot be read and a number past the
// end of the list are refused, with a sentence that holds no word of any page.
export async function recall(chrome, url, number, now = Date.now()) {
  const stored = (await chrome.storage.session.get(NUMBERS_KEY))[NUMBERS_KEY];
  if (!isList(stored)) {
    throw new Refusal(NOT_FOUND.none);
  }
  if (stored.url !== url) {
    throw new Refusal(NOT_FOUND.other);
  }
  const age = now - stored.at;
  if (!(age >= 0 && age <= MAX_AGE_MS)) {
    throw new Refusal(NOT_FOUND.old);
  }
  const entry = stored.elements[number - 1];
  if (entry === undefined) {
    throw new Refusal(NOT_FOUND.past(stored.elements.length));
  }
  return { at: entry.path, expect: entry.fingerprint };
}

// Forgets the list that is kept.
export async function forgetNumbers(chrome) {
  await chrome.storage.session.remove(NUMBERS_KEY);
}

// A page's words as one cell of a list: a character a person cannot see, or that
// changes how the text around it is drawn, is shown as its code point, and a
// word the page cut says so.
function shown(text, cut) {
  const { pieces } = shownParts(text);
  const whole = pieces
    .map((piece) => (typeof piece === "string" ? piece : `[${piece.mark}]`))
    .join("");
  return cut ? `${whole}...` : whole;
}

// The words for what a list left out, or none where it left out nothing.
function leftOut(left) {
  const reasons = [
    [left.hidden, "not displayed"],
    [left.refused, "disabled, read-only, or a password, payment card or file field"],
    [left.long, "with a name or destination too long to compare"],
    [left.more, `beyond the first ${MAX_LISTED}`],
  ].filter(([count]) => count > 0);
  return reasons.length === 0
    ? ""
    : ` Not listed: ${reasons.map(([count, why]) => `${count} ${why}`).join("; ")}.`;
}

// The text list_page_elements returns. The first lines are ours and the rest is
// the page's: for each listed element its number, role and name, and the origin
// or scheme where it goes. A name or a destination cannot start a line of its
// own, since the page collapses white space, and a character that is not seen is
// written as its code point. No value of a field is in it.
//   elements   what inPage listed, in order: { role, name, cut, destination },
//              the destination already cut to an origin or a scheme
//   left       how many were left out, by reason
export function listingText(elements, left) {
  const lines = [
    `Elements numbered: ${elements.length}, in document order. Give a number as element_number to click_element, type_text or select_option for this page. A number holds for ${MAX_AGE_MS / 60_000} minutes, or until another list is made.${leftOut(left)}`,
    "number\trole\tname\tgoes to",
  ];
  elements.forEach(({ role, name, cut, destination }, at) => {
    const cells = [String(at + 1), role, shown(name, cut)];
    if (destination !== "") {
      cells.push(shown(destination, false));
    }
    lines.push(cells.join("\t"));
  });
  return lines.join("\n");
}
