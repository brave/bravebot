// The page recorded in a real browser (recorded/world.json, made from
// recorded/page.html), and the edits tests make to a copy of it to give it a state
// the page does not hold: another attribute, a replaced node, a changed focus.

import { readFileSync } from "node:fs";

export const WORLD = JSON.parse(
  readFileSync(new URL("./recorded/world.json", import.meta.url), "utf8"),
);
export const URL_A = "https://example.test/page.html";

// A copy of the record to change.
export const fresh = () => structuredClone(WORLD);

// The page of file inputs that the page hides (recorded/hidden-world.json, made from
// recorded/hidden-inputs.html), and a copy of it to change. Its tab is at URL_HIDDEN.
export const HIDDEN_WORLD = JSON.parse(
  readFileSync(new URL("./recorded/hidden-world.json", import.meta.url), "utf8"),
);
export const URL_HIDDEN = "https://example.test/hidden-inputs.html";
export const hiddenFresh = () => structuredClone(HIDDEN_WORLD);

// The backend number of the file input of the hidden record with the attribute
// `name` set to `value`, such as its id.
export const inputWith = (world, name, value) =>
  world.flat.find(
    (node) =>
      node.localName === "input" &&
      node.attributes?.some((each, at) => at % 2 === 0 && each === name && node.attributes[at + 1] === value),
  ).backendNodeId;

// The node of the tree with `role` and `name`, the node of the document with a backend
// number, and the backend number of the node with a role and a name.
export const axOf = (world, role, name) =>
  world.ax.find((node) => node.role?.value === role && node.name?.value === name);
export const flatOf = (world, backend) => world.flat.find((node) => node.backendNodeId === backend);
export const backendOf = (world, role, name) => axOf(world, role, name).backendDOMNodeId;

export function setAttribute(world, backend, name, value) {
  const node = flatOf(world, backend);
  node.attributes ??= [];
  const at = node.attributes.findIndex((each, index) => index % 2 === 0 && each === name);
  if (at >= 0) {
    node.attributes[at + 1] = value;
  } else {
    node.attributes.push(name, value);
  }
}

// The node `old` becomes another node, `replacement`, as when a page replaces it.
export function replaceNode(world, old, replacement) {
  for (const node of world.ax) {
    if (node.backendDOMNodeId === old) {
      node.backendDOMNodeId = replacement;
    }
  }
  flatOf(world, old).backendNodeId = replacement;
  world.boxes[replacement] = world.boxes[old];
  delete world.boxes[old];
  for (const hit of Object.values(world.hits)) {
    if (hit.backendNodeId === old) {
      hit.backendNodeId = replacement;
    }
  }
}

// The middle of a recorded box, in whole pixels, worked out from its corners.
export function middleOf(border) {
  const sum = (indexes) => indexes.reduce((total, at) => total + border[at], 0);
  return { x: Math.round(sum([0, 2, 4, 6]) / 4), y: Math.round(sum([1, 3, 5, 7]) / 4) };
}

// The focus moves to the node with `backend`, as the browser reports it: the node
// has the property focused, and so does the root of the page. With `backend`
// null the focus is on the page alone, as it is when no element has it.
export function focusOn(world, backend) {
  const focused = { name: "focused", value: { type: "booleanOrUndefined", value: true } };
  for (const node of world.ax) {
    if (node.properties?.some((entry) => entry.name === "focused")) {
      node.properties = node.properties.filter((entry) => entry.name !== "focused");
    }
    if (node.role?.value === "RootWebArea" || node.backendDOMNodeId === backend) {
      node.properties = [...(node.properties ?? []), focused];
    }
  }
}

// A node the page adds to the document under the node `parent` (a backend number),
// as an element with `attributes` (a flat list of names and values). It has no
// box and is not in the accessibility tree. Answers its backend number.
export function addNode(world, parent, localName, attributes = []) {
  const backend = 9_000 + world.flat.length;
  world.flat.push({
    backendNodeId: backend,
    nodeId: backend,
    parentId: flatOf(world, parent).nodeId,
    nodeType: 1,
    nodeName: localName.toUpperCase(),
    localName,
    nodeValue: "",
    attributes,
  });
  return backend;
}

// The page shows the node `backend`, which it had hidden with no opacity and no size:
// it is opaque, has a box in the window, and is what a pointer at the middle of the
// box reaches. The middle of the box is 54,40.
export function showNode(world, backend) {
  world.styles[flatOf(world, backend).nodeId].opacity = "1";
  world.boxes[backend] = { border: [8, 30, 100, 30, 100, 50, 8, 50], width: 92, height: 20 };
  world.hits["54,40"] = { backendNodeId: backend, frameId: world.frameTree.frameTree.frame.id };
}

// The page moves the node `backend` to be a child of the node `parent`.
export function moveNode(world, backend, parent) {
  flatOf(world, backend).parentId = flatOf(world, parent).nodeId;
}

// What the browser answers at a point of the page (the window's point, with the
// scroll of a page that is not scrolled): the node `backend`.
export function hitOn(world, { x, y }, backend) {
  world.hits[`${x},${y}`] = { ...world.hits[`${x},${y}`], backendNodeId: backend };
}
