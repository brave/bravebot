// What the confirmation window says each action is. confirm.js, which the
// extensions share, reads these tables, and each extension has an actions.js of
// its own with the actions it can ask about (shared-files.json lists it as not
// copied).

// One row of the table of actions: the words the window says of the action, and
// whether the action acts on an element. A row says which, so that an action is
// not added without deciding it. The window shows the heading, the rows and the
// note about the outline of the element only for an action that acts on one.
const action = (words, element) => Object.freeze({ words, element });

// What each action is called, in words the window owns, and whether it acts on an
// element.
export const ACTIONS = Object.freeze({
  click: action("Click", true),
  double_click: action("Double-click", true),
  right_click: action("Right-click, which opens the page's context menu", true),
  type: action("Type this text, in place of what the field holds", true),
  select: action("Choose this option", true),
  key: action("Press this key, on the element that has the focus", true),
  // Acts on no element: it runs code in the page, which the window shows whole.
  // The words say what the code can do without saying that a policy limits it,
  // because nothing limits it except that the person reads it (BROWSER-46).
  run_script: action(
    "Run the code below in the page, as you, with the session you are signed in with. Read all of it. Nothing limits what it does except that you read it: it can read everything on the page, including what is hidden, click and submit the page's forms, and send what it read to another address by loading a frame or going to a link. It starts once. Timers, listeners and observers it sets keep running in the page until the page is closed or reloaded, and each can read and send without asking again. What it answers is sent to BraveBot",
    false,
  ),
});

// The action's own argument, the name it has in the description, and what the
// window calls it. A third entry of false says the value is not text, so the
// window does not count its characters. A fourth entry of true says the value is
// code: each line break in it is drawn as a line break as well as written out,
// and the count says the lines.
export const CARRIED = Object.freeze({
  type: ["text", "Text to type"],
  select: ["option", "Option to choose"],
  key: ["key", "Key to press"],
  run_script: ["code", "The code, all of it", true, true],
});
