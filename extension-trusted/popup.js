// The popup of the toolbar action. It is static: it says which extension this
// is and whose requests it answers, and it acts on nothing.

import { FIRST_ID } from "./pair.js";

document.getElementById("first").textContent = FIRST_ID;
document.getElementById("version").textContent =
  `Version ${chrome.runtime.getManifest().version}`;
