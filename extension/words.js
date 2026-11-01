// The closed lists of words and the limits a request is checked against: the
// roles an element is asked for by, the keys, directions and buttons, and the
// most characters of each word a person is shown. This file is copied byte for
// byte into the second extension (extension-trusted/), so it imports nothing;
// shared-files.json lists every file that is. input.js exports these names too.

// The roles an element can be asked for by. Anything else is refused before the
// page is touched. A search field counts as a textbox, and a native select as a
// combobox.
export const ROLES = Object.freeze([
  "button",
  "link",
  "textbox",
  "checkbox",
  "radio",
  "combobox",
  "menuitem",
  "tab",
  "option",
  "switch",
]);

// The keys press_key sends, each alone: no modifier and no character.
export const KEYS = Object.freeze([
  "Enter",
  "Escape",
  "Tab",
  "ArrowUp",
  "ArrowDown",
  "ArrowLeft",
  "ArrowRight",
  "Home",
  "End",
  "PageUp",
  "PageDown",
]);

export const DIRECTIONS = Object.freeze(["up", "down", "top", "bottom"]);
export const BUTTONS = Object.freeze(["left", "right"]);

// The most characters of a name or an option, and of typed text.
export const NAME_LIMIT = 200;
export const TYPED_TEXT_LIMIT = 2_000;

// The most characters of the page's words that a question shows: the name, the
// start of the element's text, where a link or a form goes and the tab's URL.
export const EXCERPT_LIMIT = 80;
export const TARGET_LIMIT = 200;
export const URL_LIMIT = 300;

// The most characters of a name, of the start of an element's text and of where
// it goes that are read from a page to be shown. The window cuts each to its
// limit above and says how many characters it left out, which it can say only of
// a word it was given whole, so a word longer than this is not shown and the
// element is not acted on.
export const SHOWN_CAP = 2_000;
