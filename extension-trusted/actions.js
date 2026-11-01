// What the confirmation window says each action is, and the description it is
// given. confirm.js, which the two extensions share, reads ACTIONS and CARRIED,
// and each extension writes an actions.js of its own (shared-files.json lists it
// as not copied). Most actions here are done with the mouse and the keyboard as a
// person would, and a drag names two elements. The picture action acts on no
// element: it takes a picture of the whole page, and the window shows the page and
// its size. The console action reads the page's console: it acts on no element,
// and the window shows the page and how much is read. Each action says in its row
// whether it acts on an element, and the window and describe() both read that.

import { EXCERPT_LIMIT, NAME_LIMIT, TARGET_LIMIT } from "./words.js";

// One row of the table of actions: the words the window says of the action, and
// whether the action acts on an element. A row says which, so that an action is
// not added without deciding it. The window shows the heading, the rows and the
// note about the outline of the element only for an action that acts on one, and
// describe() gives an action that acts on none no element.
const action = (words, element) => Object.freeze({ words, element });

// The words of the three actions that choose a file. The page hides the input of
// the second and third, and the third names it by its id or its name, which are
// words for a program and say little of what the input is for.
const CHOOSE = "Choose the file listed under File on the file input listed under The element.";
const READ =
  "The browser reads the file from your computer itself and gives it to the page, as if you had chosen it in the file picker, and a page whose script sends a file when it is chosen receives it as soon as you press Allow once. No bytes of the file pass through BraveBot";

// What each action is called, in words the window owns, and whether it acts on an
// element.
export const ACTIONS = Object.freeze({
  click: action("Click with the mouse, as a person would", true),
  double_click: action("Double-click with the mouse", true),
  right_click: action("Right-click with the mouse, which opens the page's context menu", true),
  type: action("Type this text on the keyboard, in place of what the field holds", true),
  key: action("Press this key on the keyboard, on the element that has the focus", true),
  hover: action("Move the mouse pointer over this element, without clicking", true),
  drag: action(
    "Drag the element listed under The element with the mouse, and drop it onto the element in the frame",
    true,
  ),
  upload: action(`${CHOOSE} ${READ}`, true),
  upload_hidden: action(
    `${CHOOSE} The page hides this input, so you cannot see it in the page, and the name below is what the page's own label, aria-label or title calls it. ${READ}`,
    true,
  ),
  upload_hidden_id: action(
    `${CHOOSE} The page hides this input and gives it no label, so the name below is the id or name of the input in the page's markup, which says little of what it is for. ${READ}`,
    true,
  ),
  picture: action(
    "Take a picture of the whole page, including the parts out of view and any that cannot be seen, scaled down to at most 2048 pixels on its long side, and send it to BraveBot. To do this the browser's debugger is attached to the page, and it reads everything on the page. No element is acted on",
    false,
  ),
  console: action(
    "Read what the page has written to its console and the errors and warnings the browser reported for it, and send it to BraveBot. To do this the browser's debugger is attached to the page, and it reads the page's console, which can hold anything the page's scripts printed. No element is acted on",
    false,
  ),
});

// The action's own argument, the name it has in the description, and what the
// window calls it. The window shows it in a frame of its own, whole. A third
// entry of false says the value is not text, so the window does not count its
// characters.
export const CARRIED = Object.freeze({
  type: ["text", "Text to type"],
  key: ["key", "Key to press"],
  drag: ["onto", "Drop it onto this element"],
  upload: ["file", "File: name, size and full path", false],
  upload_hidden: ["file", "File: name, size and full path", false],
  upload_hidden_id: ["file", "File: name, size and full path", false],
  picture: ["size", "Size of the page", false],
  console: ["limits", "What is read", false],
});

// A page's word as the text of one quoted value: cut to `limit` characters, with
// the quotation mark and the backslash escaped, so that a word cannot end the
// value and start another. The window marks every character that cannot be seen.
function quoted(value, limit) {
  const kept = [...String(value ?? "")].slice(0, limit).join("");
  return `"${kept.replace(/[\\"]/g, "\\$&")}"`;
}

// The element a drag drops onto, as one text for the frame of the window: the
// role, tag, name, start of the text and where it goes, each in quotation marks
// and cut to the limits the window cuts the element it names to. `destination`
// is the description of a resolution (resolve.js).
export function ontoText({ role, tag, name, excerpt, goes, goesUnknown }) {
  return [
    `role ${quoted(role, NAME_LIMIT)}`,
    `tag ${quoted(tag, NAME_LIMIT)}`,
    `name ${quoted(name, NAME_LIMIT)}`,
    `text ${quoted(excerpt, EXCERPT_LIMIT)}`,
    // A page's word is always in quotation marks, so this one cannot be a page's.
    `goes to ${goesUnknown === true ? "unknown" : quoted(goes, TARGET_LIMIT)}`,
  ].join(", ");
}

// The element rows of an action that acts on none. They are empty, and the window
// does not show them.
const NO_ELEMENT = Object.freeze({ tag: "", role: null, name: "", excerpt: "", goes: "" });

// What the window is given for `action`: the origin and the address of the page,
// the element from the description of its resolution (which says with goesUnknown
// that where it goes cannot be told), and the action's own argument. `text` is typed, `key` is pressed, `destination` is the
// description of the element a drag drops onto, `size` is the size of the page a
// picture is taken of, in words, `limits` is what a console read takes, in words,
// and `file` is the file an upload chooses, in words (files.js fileWords).
export function describe(action, { origin, url, element, text, key, destination, size, limits, file }) {
  if (!Object.hasOwn(ACTIONS, action)) {
    throw new Error("the window has no words for that action");
  }
  const acted = ACTIONS[action].element === false ? NO_ELEMENT : element;
  const description = {
    action,
    origin,
    url,
    tag: acted.tag,
    role: acted.role,
    name: acted.name,
    excerpt: acted.excerpt,
    goes: acted.goes,
    ...(acted.goesUnknown === true ? { goesUnknown: true } : {}),
  };
  if (action === "type") {
    description.text = text;
  }
  if (action === "key") {
    description.key = key;
  }
  if (action === "drag") {
    description.onto = ontoText(destination);
  }
  // upload, upload_hidden and upload_hidden_id choose a file.
  if (action.startsWith("upload")) {
    description.file = file;
  }
  if (action === "picture") {
    description.size = size;
  }
  if (action === "console") {
    description.limits = limits;
  }
  return Object.freeze(description);
}
