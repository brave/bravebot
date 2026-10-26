// The options page: one switch per tool, and the list of sites no tool may
// touch. Each is saved as soon as it changes.

import {
  DEFAULT_SETTINGS,
  SETTINGS_KEY,
  blockedSites,
  saveBlockedSites,
  settings,
} from "./tools.js";

const LABELS = {
  get_platform_info: [
    "Check the extension is installed",
    "The operating system and architecture Brave runs on, and nothing else.",
  ],
  list_tabs: [
    "List open tabs",
    "The title and URL of every open tab, except at blocked sites.",
  ],
  list_frames: [
    "List frames in an open tab",
    "The exact web URLs embedded in a tab, named by the tab's URL, except at blocked sites.",
  ],
  read_page: [
    "Read an open page or frame",
    "The text at a tab URL, or at one exact frame URL in that tab.",
  ],
  search_history: [
    "Search history",
    "Pages you visited, by words in their title or URL, except at blocked sites.",
  ],
  search_bookmarks: [
    "Search bookmarks",
    "Your bookmarks, by words in their title or URL, except at blocked sites.",
  ],
  open_tab: [
    "Open a page in a new tab",
    "An HTTP or HTTPS URL, in a background tab with your cookies. The tab is closed if the page goes to another host.",
  ],
};

async function render() {
  const form = document.getElementById("tools");
  const current = await settings(chrome);
  for (const name of Object.keys(DEFAULT_SETTINGS)) {
    const [title, note] = LABELS[name];
    const box = document.createElement("input");
    box.type = "checkbox";
    box.name = name;
    box.checked = current[name] === true;
    box.addEventListener("change", save);
    const label = document.createElement("label");
    label.append(box, ` ${title}`);
    const detail = document.createElement("div");
    detail.className = "note";
    detail.textContent = note;
    label.append(detail);
    form.append(label);
  }
}

async function save() {
  const chosen = {};
  for (const box of document.querySelectorAll("#tools input[type=checkbox]")) {
    chosen[box.name] = box.checked;
  }
  await chrome.storage.local.set({ [SETTINGS_KEY]: chosen });
}

// A list that cannot be read shows as empty, and the next save replaces it.
async function renderBlocked() {
  const field = document.getElementById("blocked-sites");
  try {
    field.value = (await blockedSites(chrome)).join("\n");
  } catch {
    document.getElementById("blocked-status").textContent =
      "The saved list cannot be read. Saving replaces it.";
  }
  field.addEventListener("input", () => {
    document.getElementById("blocked-status").textContent =
      "Not saved yet. Click outside the box to save it.";
  });
  field.addEventListener("change", saveBlocked);
}

// Saves the sites written when the person leaves the box, and writes them back
// as the origins that were kept, unless more was typed while the save ran. A
// save on every keystroke would take a site off the list while its line is
// being edited. A line that is not one HTTP or HTTPS address stops the save and
// is named, and the list saved before stays in force.
async function saveBlocked() {
  const field = document.getElementById("blocked-sites");
  const status = document.getElementById("blocked-status");
  const written = field.value;
  try {
    const { sites, rejected } = await saveBlockedSites(chrome, written);
    if (rejected.length > 0) {
      status.textContent = `Not saved: ${rejected.join(", ")}. A site is one http:// or https:// address, such as https://mail.example.com. The list saved before still applies.`;
      return;
    }
    status.textContent = "Saved.";
    if (field.value === written) {
      field.value = sites.join("\n");
    }
  } catch (error) {
    status.textContent = `Not saved: ${error.message}`;
  }
}

render();
renderBlocked();
