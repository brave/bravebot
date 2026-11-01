// The toolbar badge: how many groups are on, and a title that lists them. The
// platform check is on whenever the extension is, so it is not counted, and
// with no group on the badge is empty.
//
// Every function takes the `chrome` object it calls, as tools.js does.

import { groupsOn } from "./switches.js";

// The text and title the toolbar shows for the groups that are on.
export function badgeFor(ids) {
  return ids.length === 0
    ? { text: "", title: "BraveBot: no group is on" }
    : {
        text: String(ids.length),
        title: `BraveBot: ${ids.length === 1 ? "1 group is" : `${ids.length} groups are`} on: ${ids.join(", ")}`,
      };
}

// The latest call decides: reads can finish in either order, and one that began
// before another must not overwrite it.
let latest = 0;

// Shows the groups that are on now. A read that fails leaves the badge as it
// was and is thrown.
export async function showBadge(chrome) {
  latest += 1;
  const mine = latest;
  const ids = await groupsOn(chrome);
  if (mine !== latest) {
    return;
  }
  const { text, title } = badgeFor(ids);
  await Promise.all([
    chrome.action.setBadgeText({ text }),
    chrome.action.setTitle({ title }),
  ]);
}
