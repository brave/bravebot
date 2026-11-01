// The popup of the toolbar action: which groups are on, how many items a person
// holds out for a session, and Turn everything off. The button does what the
// keyboard command does (turnEverythingOff in switches.js), from the click on
// it, in this page.

import { heldCount } from "./held.js";
import { groupsOn, turnEverythingOff } from "./switches.js";

const on = document.getElementById("on");
const held = document.getElementById("held");
const button = document.getElementById("turn-off");
const status = document.getElementById("status");

// Says which groups are on now.
async function show() {
  try {
    const ids = await groupsOn(chrome);
    on.textContent =
      ids.length === 0 ? "No group is on." : `Groups on: ${ids.join(", ")}.`;
  } catch (error) {
    on.textContent = `The switches cannot be read: ${error.message}`;
  }
  try {
    const count = await heldCount(chrome);
    held.textContent =
      count === 0
        ? ""
        : `${count === 1 ? "1 item is" : `${count} items are`} held out for a session to fetch.`;
  } catch {
    held.textContent = "";
  }
}

// Turns every group off and takes back every optional permission. The switches
// are saved before anything is taken back, so if Brave refuses to take one back
// the groups are off all the same.
async function turnOffAll() {
  button.disabled = true;
  status.textContent = "";
  try {
    await turnEverythingOff(chrome);
    status.textContent =
      "Every group is off, and Brave holds no optional permission for BraveBot.";
  } catch (error) {
    status.textContent = `Brave reported an error: ${error.message}. The line above shows what is on.`;
  }
  button.disabled = false;
  await show();
}

button.addEventListener("click", turnOffAll);
show();
