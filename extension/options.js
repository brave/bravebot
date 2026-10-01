// The options page: one switch per tool, saved as soon as it changes.

import { DEFAULT_SETTINGS, SETTINGS_KEY, settings } from "./tools.js";

const LABELS = {
  get_platform_info: [
    "Check the extension is installed",
    "The operating system and architecture Brave runs on, and nothing else.",
  ],
  list_tabs: ["List open tabs", "The title and URL of every open tab."],
  read_page: [
    "Read an open page",
    "The text of a tab you have open, named by its URL.",
  ],
  search_history: [
    "Search history",
    "Pages you visited, by words in their title or URL.",
  ],
  search_bookmarks: [
    "Search bookmarks",
    "Your bookmarks, by words in their title or URL.",
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
