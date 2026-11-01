// What Brave's own prompt says when it is asked for the permissions and hosts of
// a group, for the options page to show before the person turns the group on.
//
// The wording is Brave 1.98.62's, read through
// chrome.management.getPermissionWarningsByManifest, which needs no permission
// (docs/design/browser-extension.md, "Checked against Brave"). Brave words some
// permissions differently from Chrome's permission list, says nothing for those
// that show no prompt, and merges the warnings of some permissions asked for
// together (warningsFor).

import { ALL_URLS, HOST_PATTERNS } from "./groups.js";

// The warning for access to the pages of every site.
export const HOSTS_WARNING = "Read and change all your data on all websites";

// What Brave says for each permission a group lists when it is asked for alone.
// A permission that shows no warning is null. `browsingData`, `downloads` and
// `pageCapture` were read through that call as well.
export const BROWSER_WARNINGS = {
  tabs: "Read your browsing history",
  tabGroups: "View and manage your tab groups",
  webNavigation: "Read your browsing history",
  sessions: null,
  history: "Read and change your browsing history on all your signed-in devices",
  topSites: "Read a list of your most frequently visited websites",
  bookmarks: "Read and change your bookmarks",
  readingList: "Read and change entries in the reading list",
  scripting: null,
  search: null,
  cookies: null,
  contextMenus: null,
  webRequest: null,
  privacy: "Change your privacy-related settings",
  contentSettings:
    "Change and grant access to features such as geolocation, microphone, camera, cookies, etc., for all your websites and extensions, including this extension.",
  fontSettings: null,
  "system.cpu": null,
  "system.memory": null,
  "system.display": null,
  "system.storage": "Identify and eject storage devices",
  idle: null,
  power: null,
  browsingData: null,
  downloads: "Manage your downloads",
  pageCapture: HOSTS_WARNING,
  desktopCapture: "Capture content of your screen",
  userScripts: null,
};

// The permissions that carry the warning for access to every site by
// themselves, with no host asked for. Brave says that line for pageCapture
// whatever hosts are asked for with it, and merges the warnings it subsumes as
// it does for access to every site (warningsFor).
const EVERY_SITE_PERMISSIONS = ["pageCapture"];

// The warning for access to the pages of some sites. Brave names up to three
// hosts, sorted and without a scheme or port, and otherwise says a number of
// websites.
export function sitesWarning(origins) {
  const hosts = [
    ...new Set(origins.map((origin) => new URL(origin).hostname)),
  ].sort();
  if (hosts.length > 3) {
    return "Read and change your data on a number of websites";
  }
  const named =
    hosts.length === 3
      ? `${hosts[0]}, ${hosts[1]}, and ${hosts[2]}`
      : hosts.join(" and ");
  return `Read and change your data on ${named}`;
}

// What Brave says for `tabs` asked for together with `sessions`.
export const HISTORY_ALL_DEVICES =
  "Read your browsing history on all your signed-in devices";

// The permissions whose warnings are about the browsing history. Brave says one
// line for the ones asked for together: `history` is the line of all of them;
// otherwise `tabs` with `sessions` reads the history of all signed-in devices,
// `tabs` or `webNavigation` reads it, and `topSites` alone lists the most
// visited websites. `sessions` alone says nothing. Access to every site says
// that the pages may be read and changed, and leaves out all of these except
// `history`.
const HISTORY_FAMILY = ["tabs", "webNavigation", "sessions", "history", "topSites"];

function historyWarning(permissions, everySite) {
  const asked = (name) => permissions.includes(name);
  if (asked("history")) {
    return BROWSER_WARNINGS.history;
  }
  if (everySite) {
    return null;
  }
  if (asked("tabs") && asked("sessions")) {
    return HISTORY_ALL_DEVICES;
  }
  if (asked("tabs") || asked("webNavigation")) {
    return BROWSER_WARNINGS.tabs;
  }
  return asked("topSites") ? BROWSER_WARNINGS.topSites : null;
}

// What Brave asks the person to allow when the permissions in `query` are
// requested, each line once, as Brave merges them.
export function warningsFor(query) {
  const { permissions = [], origins = [] } = query;
  const everySite =
    permissions.some((permission) =>
      EVERY_SITE_PERMISSIONS.includes(permission),
    ) ||
    origins.some(
      (pattern) => pattern === ALL_URLS || HOST_PATTERNS.includes(pattern),
    );
  const lines = permissions.map((permission) => {
    if (!Object.hasOwn(BROWSER_WARNINGS, permission)) {
      throw new Error(`no browser warning is written for ${permission}`);
    }
    return HISTORY_FAMILY.includes(permission)
      ? historyWarning(permissions, everySite)
      : BROWSER_WARNINGS[permission];
  });
  if (everySite) {
    lines.push(HOSTS_WARNING);
  } else if (origins.length > 0) {
    lines.push(
      sitesWarning(origins.map((pattern) => pattern.replace(/\/\*$/, ""))),
    );
  }
  return [...new Set(lines.filter((line) => line !== null))];
}
