// The options page: one switch per tool, saved as soon as it changes.

import { DEFAULT_SETTINGS, SETTINGS_KEY, settings } from "./tools.js";

const LABELS = {
  get_platform_info: [
    "Check the extension is installed",
    "The operating system and architecture Brave runs on, and nothing else.",
  ],
  list_tabs: ["List open tabs", "The title and URL of every open tab."],
  list_frames: [
    "List frames in an open tab",
    "The exact web URLs embedded in a tab, named by the tab's URL.",
  ],
  read_page: [
    "Read an open page or frame",
    "The text at a tab URL, or at one exact frame URL in that tab.",
  ],
  search_history: [
    "Search history",
    "Pages you visited, by words in their title or URL.",
  ],
  search_bookmarks: [
    "Search bookmarks",
    "Your bookmarks, by words in their title or URL.",
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

render();
