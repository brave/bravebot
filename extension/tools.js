// What each tool does in the browser, and the settings that decide which of
// them may run.
//
// Every function takes the `chrome` object it calls, rather than reading the
// global, so the tests can hand it a fake one. The method names are the tool
// names bravebot-browser offers. Each takes that tool's arguments as its
// parameters: crates/browser/src/tools.rs is the other half of that contract.

import {
  ALLOWED_ONLY_TOOLS,
  ALWAYS_ON,
  GROUPS,
  NAMES_NO_PAGE,
  NEEDS_ALL_URLS,
  NO_ACCESS,
  SCOPE_ALL,
  SCOPE_ALLOWED,
  groupOf,
  holds,
  scopeOf,
} from "./groups.js";
import {
  LEVELS,
  MAX_AWAKE_MINUTES,
  keepAwake,
  releaseAwake,
} from "./awake.js";
import {
  FAILED,
  ORIGINS_PER_ENTRY,
  RAN,
  REFUSED,
  recordCall,
} from "./activity.js";
import { confirm, spend } from "./approval.js";
import { Refusal, SERVER_ERROR, ToolError } from "./errors.js";
import {
  DEFAULT_WAIT_SECONDS,
  MAX_PAGES,
  MAX_WAIT_SECONDS,
  OUTLINE_MS,
  WAIT_POLL_MS,
  clearOutlineInPage,
  failureText,
  inPage,
  isFailure,
  scrollInPage,
} from "./input.js";
import {
  DEFAULT_ENTRIES,
  DEFAULT_LEVELS,
  LEVELS as CONSOLE_LEVELS,
  MAX_ENTRIES,
} from "./consoles.js";
import { takeHeld } from "./held.js";
import { endedText, lifetimeEnded } from "./lifetimes.js";
import {
  LISTING_LIMITS,
  MAX_LISTED,
  listingText,
  recall,
  remember,
} from "./numbering.js";
import { problemWithFile } from "./files.js";
import {
  CODE_LIMIT,
  POLICY_NOT_APPLIED,
  POLICY_PROBE,
  POLICY_STOPPED,
  UNAVAILABLE,
  WORLD_CSP,
  describeScript,
  namesAReference,
  newWorldId,
  resultText,
  userScriptsAvailable,
} from "./scripts.js";
import { MAX_PICTURE_BYTES, decodedSize } from "./pictures.js";
import { requestsOf } from "./requests.js";
import { forward } from "./trusted.js";
import {
  DEFAULT_GIF_FRAMES,
  MAX_GIF_FRAMES,
  captureScreen,
  recordGif,
  toBase64,
} from "./screen.js";
import {
  BUTTONS,
  DIRECTIONS,
  KEYS,
  NAME_LIMIT,
  ROLES,
  SHOWN_CAP,
  TYPED_TEXT_LIMIT,
  URL_LIMIT,
} from "./words.js";

// The most text read_page returns from one page, in UTF-16 code units. A page
// longer than this is cut here and says so. A session gets the start of a very
// long page rather than nothing.
export const PAGE_TEXT_LIMIT = 100_000;

// The most text of one link or one form field read_page_links and
// read_page_forms keep, in UTF-16 code units. The rest of it is cut off.
export const ITEM_TEXT_LIMIT = 200;

// The most results a search returns.
export const MAX_RESULTS = 100;

// How many results a search returns where the call does not say.
export const DEFAULT_RESULTS = 20;

// Where the settings are kept in chrome.storage.local.
export const SETTINGS_KEY = "tools";

// Where the group switches are kept in chrome.storage.local: an object from a
// group id to true.
export const GROUP_SWITCHES_KEY = "groups";

// Where the blocked sites are kept, beside the settings: a list of origins such
// as "https://mail.example.com" that a person wrote in the options page.
export const BLOCKED_SITES_KEY = "blocked_sites";

// Where the allowed sites are kept: a list of origins written as the blocked
// ones are. A group that reads pages and is limited to them reaches no other
// site.
export const ALLOWED_SITES_KEY = "allowed_sites";

// Where the scope of each group that reads pages is kept: an object from a
// group id to "all" for a group on all sites except the blocked ones. A group
// not in it is limited to the allowed sites.
export const SITE_SCOPES_KEY = "site_scopes";

// How many history entries a search looks at while a site is blocked. Entries
// at a blocked site are dropped after the search, so a search that asked the
// browser for no more than it returns would come back short, or empty, where
// the newest matches were all at a blocked site.
const BLOCKED_SCAN_LIMIT = 10 * MAX_RESULTS;

// Which tools may run until a person changes it in the options page. Only the
// platform check starts on: it tells nothing about the person. Every other tool
// reaches what they have open, visited or saved, so it reads nothing until they
// turn it on. The tools are those of the groups.
export const DEFAULT_SETTINGS = Object.freeze(
  Object.fromEntries(
    Object.entries(GROUPS).flatMap(([id, group]) =>
      group.tools.map((tool) => [tool, id === ALWAYS_ON]),
    ),
  ),
);

// How long open_tab waits for the new tab to commit a page, in milliseconds.
// It is under the 30 seconds the host waits for any reply, so a page that never
// commits is answered with why and the tab is closed.
export const OPEN_TIMEOUT_MS = 20_000;

// How often wait_for_load asks the browser whether the tab has finished loading,
// in milliseconds. It waits as long as open_tab does.
export const LOAD_POLL_MS = 250;

// The most text search_web takes, in UTF-16 code units. The question shows it
// whole, so it is short enough to read there.
export const SEARCH_TEXT_LIMIT = 256;

// The least time between two screenshots, in milliseconds. Brave allows two
// calls of captureVisibleTab in a rolling second and rejects a third
// (MAX_CAPTURE_VISIBLE_TAB_CALLS_PER_SECOND). Calls 500 ms apart were all taken
// in Brave 1.98, so this adds a margin.
export const CAPTURE_GAP_MS = 600;

// The quality of the JPEG a screenshot is taken as. Brave 1.98 gave 0.02 to 1.6
// MB at this quality for a page 2048 pixels wide, and 4 to 7.4 MiB as a PNG.
export const PICTURE_QUALITY = 80;

// The largest screenshot a call returns is MAX_PICTURE_BYTES (pictures.js).
export { MAX_PICTURE_BYTES };

// How a data URL for the JPEG the browser takes begins.
const PICTURE_PREFIX = "data:image/jpeg;base64,";

// The largest archive save_page_archive saves, in bytes. The service worker
// builds the archive into a data URL in memory, a third longer than the archive,
// because it has no URL.createObjectURL to give downloads.download. Brave 1.98
// saved an archive of 20 MB that way, and 16 MiB leaves a margin. A larger one
// is refused, saying so, where saving it would use memory the person did not
// expect a call to use.
export const MAX_ARCHIVE_BYTES = 16 * 1024 * 1024;

// The type the data URL of an archive is given, which is the type of an MHTML
// file.
const ARCHIVE_TYPE = "multipart/related";

// The longest host, in characters, the name of an archive holds, so the name is
// well within the 255 bytes a file name may be.
const ARCHIVE_HOST_LIMIT = 100;

// How often, in milliseconds, and how many times save_page_archive asks the
// browser whether it has written the file. Brave 1.98 wrote an archive of 8 MB
// in under 400 ms. A file not finished after the last time is told as in
// progress.
export const ARCHIVE_POLL_MS = 100;
const ARCHIVE_POLLS = 50;

// JSON-RPC's code for a method there is no such thing as.
const METHOD_NOT_FOUND = -32601;

// JSON-RPC's code for arguments that are not what the tool takes.
const INVALID_PARAMS = -32602;

// The most entries a list argument of a call may hold. A call names a handful of
// URLs or kinds of data, and the log and the question show them, so a longer
// list is refused before anything reads it.
export const LIST_LIMIT = 50;

// The two kinds of error a call ends in are those of errors.js, which the second
// extension shares.
export { Refusal, ToolError };

// Which tools may run: the defaults, with what a person set on top.
export async function settings(chrome) {
  const stored = await chrome.storage.local.get(SETTINGS_KEY);
  return { ...DEFAULT_SETTINGS, ...(stored[SETTINGS_KEY] ?? {}) };
}

// The groups whose switch is on, as an object from id to true. A stored value
// that is not an object, or a switch that is not true, is off, so a list that
// cannot be read leaves every group off.
export async function groupSwitches(chrome) {
  const stored = (await chrome.storage.local.get(GROUP_SWITCHES_KEY))[
    GROUP_SWITCHES_KEY
  ];
  if (typeof stored !== "object" || stored === null || Array.isArray(stored)) {
    return {};
  }
  return Object.fromEntries(
    Object.keys(GROUPS)
      .filter((id) => id !== ALWAYS_ON && stored[id] === true)
      .map((id) => [id, true]),
  );
}

// The origins a person wrote under `key`, in the order they wrote them. A
// stored value that is not a list of strings is a failure, not an empty list.
async function siteList(chrome, key, name) {
  const stored = await chrome.storage.local.get(key);
  const list = stored[key] ?? [];
  if (!Array.isArray(list) || list.some((site) => typeof site !== "string")) {
    throw new Refusal(
      `the ${name} sites in the BraveBot extension's options cannot be read`,
    );
  }
  return list;
}

// The origins a person blocked. A list that cannot be read must not leave every
// site open, so it is a failure.
export function blockedSites(chrome) {
  return siteList(chrome, BLOCKED_SITES_KEY, "blocked");
}

// The origins a person allowed. The options page says when the list cannot be
// read; siteAccess takes it for empty, which reaches no site.
export function allowedSites(chrome) {
  return siteList(chrome, ALLOWED_SITES_KEY, "allowed");
}

// The allowed sites and the group scopes, as the groups read them. A list that
// cannot be read is empty and a scope that is not "all" is the allowed sites, so
// a value that cannot be read narrows what a group reaches.
export async function siteAccess(chrome) {
  let allowed = [];
  try {
    allowed = await allowedSites(chrome);
  } catch (error) {
    if (!(error instanceof ToolError)) {
      throw error;
    }
  }
  const stored = (await chrome.storage.local.get(SITE_SCOPES_KEY))[
    SITE_SCOPES_KEY
  ];
  const named = typeof stored === "object" && stored !== null ? stored : {};
  return {
    allowed,
    scopes: Object.fromEntries(
      Object.keys(GROUPS)
        .filter((id) => GROUPS[id].hosts && named[id] === SCOPE_ALL)
        .map((id) => [id, SCOPE_ALL]),
    ),
  };
}

// The origin of a URL, which is how the blocked sites are written, or null
// where it has none. A blob: URL has the origin of the page that made it. A
// view-source: URL shows the page named after it.
function originOf(value) {
  if (typeof value !== "string") {
    return null;
  }
  try {
    const { origin } = new URL(value.replace(/^view-source:/i, ""));
    return origin === "null" ? null : origin;
  } catch {
    return null;
  }
}

// What a web host looks like once the URL parser has lowercased it and turned
// a name outside ASCII into punycode. The parser also takes `*` and `,` in a
// host, so https://*.bank.example would be kept as an origin no page has, and
// the site it was meant to cover would stay open.
const HOST = /^(?:[a-z0-9_-]+(?:\.[a-z0-9_-]+)*\.?|\[[0-9a-f:.]+\])$/;

// Whether a line is one address. The parser puts whatever follows the first
// `/` into the path, so `https://a.example/, https://b.example/` would be kept
// as the first site and lose the second without a word.
function oneAddress(entry) {
  return !/\s/.test(entry) && entry.indexOf("://") === entry.lastIndexOf("://");
}

// The sites a person wrote, as the origins to keep and the lines that are not
// one HTTP or HTTPS address of a host. A path after the origin is dropped, since
// the list is of sites, and a site named twice is kept once.
export function parseSites(input) {
  const sites = [];
  const rejected = [];
  for (const line of input.split(/\r?\n/)) {
    const entry = line.trim();
    if (entry === "") {
      continue;
    }
    const origin =
      oneAddress(entry) && webUrl(entry) && HOST.test(new URL(entry).hostname)
        ? originOf(entry)
        : null;
    if (origin === null) {
      rejected.push(entry);
    } else if (!sites.includes(origin)) {
      sites.push(origin);
    }
  }
  return { sites, rejected };
}

// Keeps the sites in `input` as the blocked list. A line that is not one HTTP
// or HTTPS address stops the save and is returned, so the options page can say
// which. Saving the others would drop a site whose line was mistyped.
export async function saveBlockedSites(chrome, input) {
  const { sites, rejected } = parseSites(input);
  if (rejected.length === 0) {
    await chrome.storage.local.set({ [BLOCKED_SITES_KEY]: sites });
  }
  return { sites, rejected };
}

// Whether `url` is at one of the origins in `sites`. Origins are compared as
// written: https://example.com does not cover https://www.example.com or
// http://example.com.
function isListed(sites, url) {
  const origin = originOf(url);
  return origin !== null && sites.includes(origin);
}

// Whether `url` is at one of the blocked origins.
function isBlocked(sites, url) {
  return isListed(sites, url);
}

// Whether `frame` is at a blocked site or inside a frame that is. What a
// blocked frame embeds is part of what that site shows, so it goes with it.
function behindBlocked(sites, frames, frame) {
  let current = frame;
  for (let depth = 0; current && depth <= frames.length; depth += 1) {
    if (isBlocked(sites, current.url)) {
      return true;
    }
    const parent = current.parentFrameId;
    current = frames.find((candidate) => candidate.frameId === parent);
  }
  return false;
}

// What a call that names a blocked site is told. It goes back to the session,
// so it names the list and nothing of the site.
const BLOCKED_MESSAGE =
  "that site is in the blocked sites list in the BraveBot extension's options";

// Refuses a call that names a blocked site, before any tab is looked for:
// whether one is open is not told.
function refuseBlocked(sites, url) {
  if (isBlocked(sites, url)) {
    throw new Refusal(BLOCKED_MESSAGE);
  }
}

// What a call to a group limited to the allowed sites is told when it names
// another. It goes back to the session, so it names the list and nothing of the
// site.
const NOT_ALLOWED_MESSAGE =
  "that site is not in the allowed sites list in the BraveBot extension's options, which limits this group to the sites on it";

// Refuses a call to a group limited to the allowed sites unless the page it
// names, and the frame inside it where it names one, is at an allowed site. A
// call that names no page names no allowed site. This comes before any tab is
// looked for, as the refusal for a blocked site does.
function refuseUnlessAllowed(sites, params) {
  const named = [params?.url];
  if (params?.frame_url !== undefined) {
    named.push(params.frame_url);
  }
  if (!named.every((url) => isListed(sites, url))) {
    throw new Refusal(NOT_ALLOWED_MESSAGE);
  }
}

// Added to the refusal for a group that reads pages. Brave lets a person narrow
// the sites an extension reaches in its own site access setting, and no API says
// what that setting is, so a host the browser withholds is told as one not held.
const SITE_ACCESS_NOTE =
  ". Brave's site access setting for the extension can also withhold a site from it.";

// Why a screenshot needs a group on all sites. The browser lets an extension
// capture a tab only when it holds access to all sites, which is the permission
// <all_urls> and not the patterns of some sites.
const ALL_SITES_REASON =
  "the browser lets an extension capture a tab only with access to all sites";

// What a call to a tool that needs <all_urls> is told besides the refusal for a
// group not held: the group has to be on all sites for the tool to run.
const ALL_SITES_NOTE = ` This tool needs the group on all sites in the BraveBot extension's options, because ${ALL_SITES_REASON}.`;

// A string argument that has to be there and have something in it.
function text(params, name) {
  const value = params?.[name];
  if (typeof value !== "string" || value.trim() === "") {
    throw new ToolError(
      `${name} must be a string that is not empty`,
      INVALID_PARAMS,
    );
  }
  return value;
}

// Whether a frame URL names web content a person can judge in a question.
// Opaque and browser-owned URLs do not say what a later read would expose.
function webUrl(value) {
  try {
    return ["http:", "https:"].includes(new URL(value).protocol);
  } catch {
    return false;
  }
}

// The tab at exactly `url`, which is the tab a person approved. Two tabs at one
// URL are different tabs the person could not tell apart by the URL, so the call
// fails and says how many there are, as it does for two frames at one URL. Every
// tool that names a tab finds it here.
async function tabAt(chrome, url) {
  const tabs = await chrome.tabs.query({});
  const matches = tabs.filter((candidate) => candidate.url === url);
  if (matches.length === 0) {
    throw new ToolError(`no open tab is at ${url}`);
  }
  if (matches.length > 1) {
    throw new ToolError(
      `${matches.length} open tabs are at ${url}, so the URL does not name one tab`,
    );
  }
  return matches[0];
}

// The frames in the approved tab, followed by a check that the tab did not
// navigate while Brave found them.
async function framesAt(chrome, tab, url) {
  const frames =
    (await chrome.webNavigation.getAllFrames({ tabId: tab.id })) ?? [];
  const current = await chrome.tabs.get(tab.id);
  if (current?.url !== url) {
    throw new ToolError(`the tab left ${url} before its frames were read`);
  }
  return frames;
}

// When the last screenshot of each browser was taken, and the screenshots
// waiting their turn in it. The quota is the browser's, so the state is kept
// for the browser object a call was given.
const captures = new WeakMap();

// Runs `capture` once CAPTURE_GAP_MS has passed since the last one in this
// browser ended, and one at a time, so calls that arrive together are spaced as
// well. `capture` is given a function to call as the browser's capture ends,
// which starts the next gap.
function spaced(chrome, capture) {
  const state = captures.get(chrome) ?? {
    last: -Infinity,
    turn: Promise.resolve(),
  };
  captures.set(chrome, state);
  const result = state.turn.then(async () => {
    const wait = state.last + CAPTURE_GAP_MS - Date.now();
    if (wait > 0) {
      await new Promise((resolve) => setTimeout(resolve, wait));
    }
    return capture(() => {
      state.last = Date.now();
    });
  });
  state.turn = result.catch(() => {});
  return result;
}

// Fails unless the tab is still at `url` and is the one its window shows, and
// answers the tab. The browser captures the visible tab of a window and takes no
// tab to capture, so this, before and after the capture, is what ties a picture
// to the page the call named. `before` is whether the capture is still to come.
async function showing(chrome, tabId, url, before) {
  const current = await chrome.tabs.get(tabId).catch(() => undefined);
  if (current?.url !== url) {
    throw new ToolError(
      `the tab left ${url} ${before ? "before" : "while"} its picture was taken`,
    );
  }
  if (current.active !== true) {
    throw new ToolError(
      before
        ? `the tab at ${url} is not the visible tab of its window, and the browser captures only the visible tab; bring it forward first (focus_tab in the tabs-manage group)`
        : `the tab at ${url} stopped being the visible tab of its window while its picture was taken`,
    );
  }
  return current;
}

// What a failure of the browser's captureVisibleTab is told as. The quota is
// the browser's, and a call over it succeeds when it is made again.
function captureFailure(url, error) {
  const message = String(error?.message ?? error);
  if (message.includes("MAX_CAPTURE_VISIBLE_TAB_CALLS_PER_SECOND")) {
    return "the browser allows two screenshots a second and refused this one; call again";
  }
  return `the picture of the tab at ${url} cannot be taken: ${message}`;
}

// The tab a picture of a page is taken of: `params.url` has to be an HTTP or
// HTTPS URL that is not at a blocked site, the group of `tool` has to be on all
// sites, and exactly one tab has to be at the URL. Nothing is captured. It is
// what screenshot_page and record_gif both stop at before the browser's capture
// is called.
async function pageToCapture(chrome, params, tool) {
  const url = text(params, "url");
  if (!webUrl(url)) {
    throw new ToolError("url must be an HTTP or HTTPS URL", INVALID_PARAMS);
  }
  refuseBlocked(await blockedSites(chrome), url);
  const group = groupOf(tool);
  if (scopeOf(group, await siteAccess(chrome)) !== SCOPE_ALL) {
    throw new Refusal(
      `${tool} is refused: the ${group} group is limited to the allowed sites in the BraveBot extension's options, and ${ALL_SITES_REASON}. Choose all sites for the group there`,
    );
  }
  return { url, tab: await tabAt(chrome, url) };
}

// The base64 of the JPEG data URL `taken`, or a failure saying so when it is
// not a JPEG data URL or decodes to more than MAX_PICTURE_BYTES. `giver` is who
// made it, `what` names the picture, and `advice` is what to do about one that
// is too large.
function pictureData(taken, { giver, what, advice }) {
  if (typeof taken !== "string" || !taken.startsWith(PICTURE_PREFIX)) {
    throw new ToolError(
      `${giver} gave a ${what} in a form this extension does not pass on`,
    );
  }
  const data = taken.slice(PICTURE_PREFIX.length);
  if (decodedSize(data) > MAX_PICTURE_BYTES) {
    throw new ToolError(
      `the ${what} is over ${MAX_PICTURE_BYTES / 1024 / 1024} MiB, which is the most a picture may be; ${advice}`,
    );
  }
  return data;
}

// One picture of `tab`, which is at `url`, as the base64 of a JPEG. It waits its
// turn among the captures of this browser, checks that the tab is the one its
// window shows and not minimized, takes the picture, and checks again that the
// tab is still at `url` and still shown, which is what ties the picture to the
// page the call named. Nothing is brought forward.
async function jpegOfTab(chrome, tab, url) {
  const taken = await spaced(chrome, async (ended) => {
    const shown = await showing(chrome, tab.id, url, true);
    const holder = await chrome.windows
      .get(shown.windowId)
      .catch(() => undefined);
    if (holder?.state === "minimized") {
      throw new ToolError(
        `the window of the tab at ${url} is minimized, so there is nothing to take a picture of`,
      );
    }
    let picture;
    try {
      picture = await chrome.tabs.captureVisibleTab(shown.windowId, {
        format: "jpeg",
        quality: PICTURE_QUALITY,
      });
    } catch (error) {
      throw new ToolError(captureFailure(url, error));
    } finally {
      ended();
    }
    await showing(chrome, tab.id, url, false);
    return picture;
  });
  return pictureData(taken, {
    giver: "the browser",
    what: `picture of the tab at ${url}`,
    advice: "make the window smaller and call again",
  });
}

// The name the archive of the page at `url` is saved as: the host of the page,
// with every character but a letter, a digit, a dot and a hyphen replaced, in a
// frame the extension fixes. It holds no folder and nothing a session wrote
// beyond the host of the address it named, and the browser numbers it where the
// name is taken.
function archiveName(url) {
  const host = new URL(url).hostname
    .replace(/[^a-z0-9.-]/gi, "-")
    .slice(0, ARCHIVE_HOST_LIMIT);
  return `bravebot-${host}.mhtml`;
}

// The archive as a data URL, which is how a service worker hands a file to
// downloads.download.
async function dataUrlOf(archive) {
  const bytes = new Uint8Array(await archive.arrayBuffer());
  let binary = "";
  for (let at = 0; at < bytes.length; at += 0x8000) {
    binary += String.fromCharCode(...bytes.subarray(at, at + 0x8000));
  }
  return `data:${ARCHIVE_TYPE};base64,${btoa(binary)}`;
}

// Runs `start`, which opens a page in a tab or moves one, and waits for the tab's
// top frame to commit or fail to load, or for OPEN_TIMEOUT_MS to pass. Gives the
// tab `start` returns and what happened to it, which is nothing where time ran
// out. A move within the same document, such as to a fragment of the page, is
// reported as a commit at the URL it ended at. The listeners are in place before
// `start` runs, since a fast page can commit before the call that created it
// returns, so each event is kept by tab id until the id is known. A commit at
// `skipped`, the blank page a tab is made with, is not the page being waited for.
async function openedTab(chrome, start, skipped) {
  const seen = new Map();
  let wake = () => {};
  const record = (kind) => (details) => {
    if (
      details.frameId === 0 &&
      !seen.has(details.tabId) &&
      (skipped === undefined || details.url !== skipped)
    ) {
      seen.set(details.tabId, { kind, url: details.url });
      wake();
    }
  };
  const onCommitted = record("committed");
  const onError = record("error");
  const commits = [
    chrome.webNavigation.onCommitted,
    chrome.webNavigation.onHistoryStateUpdated,
    chrome.webNavigation.onReferenceFragmentUpdated,
  ];
  commits.forEach((event) => event.addListener(onCommitted));
  chrome.webNavigation.onErrorOccurred.addListener(onError);
  let timer;
  try {
    const tab = await start();
    const timedOut = new Promise((resolve) => {
      timer = setTimeout(resolve, OPEN_TIMEOUT_MS, "timeout");
    });
    while (!seen.has(tab.id)) {
      const woken = new Promise((resolve) => {
        wake = () => resolve("event");
      });
      if ((await Promise.race([woken, timedOut])) === "timeout") {
        break;
      }
    }
    return { tab, event: seen.get(tab.id) };
  } finally {
    clearTimeout(timer);
    commits.forEach((event) => event.removeListener(onCommitted));
    chrome.webNavigation.onErrorOccurred.removeListener(onError);
  }
}

// The URL a page committed, once it is known to be the page the person was
// shown the question for: a web page on the host of `url`, which is the URL the
// question shows, and not at a blocked site. Otherwise `leave` takes the tab or
// window off that page and the call fails. A redirect to another host is one
// nobody was shown, and the failure names `url` and not where the page went.
// Where the question shows no address, as for a search, `url` is undefined and
// there is no host to hold the page to: any web page passes that is not at a
// blocked site.
async function checkLanding(sites, url, event, { subject, left, leave }) {
  const refused = async (error) => {
    await leave();
    return error;
  };
  if (!event) {
    throw await refused(new ToolError(`${subject} did not load in time`));
  }
  if (event.kind === "error") {
    throw await refused(
      new ToolError(
        url === undefined
          ? `${subject} failed to load`
          : `the page at ${url} failed to load`,
      ),
    );
  }
  if (
    !webUrl(event.url) ||
    (url !== undefined && new URL(event.url).host !== new URL(url).host)
  ) {
    throw await refused(
      new ToolError(
        url === undefined
          ? `${subject} did not load a web page, so ${left}`
          : `${subject} went to another host, so ${left}`,
      ),
    );
  }
  if (isBlocked(sites, event.url)) {
    throw await refused(new Refusal(BLOCKED_MESSAGE));
  }
  return event.url;
}

// The first `limit` UTF-16 code units of `text`, one fewer where the last of
// them would be the first half of a character.
function cut(text, limit) {
  const kept = text.slice(0, limit);
  const last = kept.charCodeAt(kept.length - 1);
  return last >= 0xd800 && last <= 0xdbff ? kept.slice(0, -1) : kept;
}

// `value` with every string in it well formed: an unpaired surrogate becomes
// U+FFFD. JSON carries one as an escape the host cannot parse, so a reply
// holding one would be refused and the call left waiting.
function wellFormed(value) {
  if (typeof value === "string") {
    return value.toWellFormed();
  }
  if (Array.isArray(value)) {
    return value.map(wellFormed);
  }
  if (value && typeof value === "object") {
    return Object.fromEntries(
      Object.entries(value).map(([key, inner]) => [key, wellFormed(inner)]),
    );
  }
  return value;
}

// How many results to return: what the call asked for, kept between 1 and
// MAX_RESULTS.
function count(params) {
  const asked = params?.max_results;
  if (typeof asked !== "number" || !Number.isFinite(asked)) {
    return DEFAULT_RESULTS;
  }
  return Math.min(MAX_RESULTS, Math.max(1, Math.floor(asked)));
}

// A time the browser reports, in milliseconds since the epoch, as an ISO 8601
// string, or null where it gives none.
function isoTime(milliseconds) {
  return Number.isFinite(milliseconds)
    ? new Date(milliseconds).toISOString()
    : null;
}

// The tabs a session holds that are not at a blocked site: the one tab of a
// closed tab, or the tabs of a window. A window keeps its other tabs.
function sessionTabs(sites, session) {
  const tabs = session.tab ? [session.tab] : (session.window?.tabs ?? []);
  return tabs
    .filter((tab) => !isBlocked(sites, tab.url))
    .map((tab) => ({ title: tab.title ?? "", url: tab.url ?? "" }));
}

// Runs in the page, so it can use nothing of this module. Gives the links or
// the forms of the page's top document, with the addresses and names as the
// page wrote them, and no value any field holds. A closed shadow root is opened
// where the browser allows it, and an open one is entered where it does not.
// Every node is read through its prototype (pageReads in input.js), so a control
// of a form that is named like a property is not taken for it.
export function listInPage(expectedUrl, kind) {
  if (location.href !== expectedUrl) {
    return { url: location.href };
  }
  const read = (node, name) => Reflect.get(Object.getPrototypeOf(node), name, node);
  const call = (node, name, ...args) => Reflect.apply(read(node, name), node, args);
  const dom = globalThis.chrome?.dom;
  const elements = new Set();
  const visit = (root) => {
    for (const element of call(root, "querySelectorAll", "*")) {
      elements.add(element);
      const shadow =
        dom?.openOrClosedShadowRoot(element) ?? read(element, "shadowRoot");
      if (shadow) {
        visit(shadow);
      }
    }
  };
  visit(document);
  if (kind === "links") {
    const links = [];
    for (const element of elements) {
      if (read(element, "localName") === "a" && typeof read(element, "href") === "string") {
        links.push({
          href: read(element, "href"),
          text: (read(element, "textContent") ?? "").replace(/\s+/g, " ").trim(),
        });
      }
    }
    return { url: location.href, links };
  }
  const forms = new Map();
  for (const element of elements) {
    if (read(element, "localName") === "form") {
      const method = (call(element, "getAttribute", "method") ?? "").toLowerCase();
      forms.set(element, {
        action: call(element, "getAttribute", "action") ?? "",
        method: method === "post" || method === "dialog" ? method : "get",
        fields: [],
      });
    }
  }
  for (const element of elements) {
    const owner = ["input", "select", "textarea", "button"].includes(
      read(element, "localName"),
    )
      ? forms.get(read(element, "form"))
      : undefined;
    if (owner) {
      owner.fields.push({
        name: call(element, "getAttribute", "name") ?? call(element, "getAttribute", "id") ?? "",
        type: typeof read(element, "type") === "string" ? read(element, "type") : "",
      });
    }
  }
  return { url: location.href, forms: [...forms.values()] };
}

// Runs in the page, as listInPage does. Gives the title and the text of the
// top document of the page.
export function readInPage(expectedUrl) {
  if (location.href !== expectedUrl) {
    return { url: location.href };
  }
  const read = (node, name) => Reflect.get(Object.getPrototypeOf(node), name, node);
  const body = read(document, "body");
  return {
    url: location.href,
    title: read(document, "title"),
    text: (body ? read(body, "innerText") : "") ?? "",
  };
}

// How long a page gets to let a script injected into it finish, in milliseconds.
// A page can hold a function for ever, with a loop or with markup that makes the
// function's work very long, and the call, and the one place an action on an
// element may wait in, would be held with it. The relay gives up on a call after
// 30 seconds, and an action on an element waits longer for the person, but that
// wait begins after the page has answered.
export const PAGE_WAIT_MS = 15_000;

// Runs the script `details` describes in a page and gives what it answered, or
// fails if the page has not let it finish within PAGE_WAIT_MS. The script is not
// stopped, and what it answers later is dropped.
async function runInPage(chrome, details) {
  return withinPageWait(() => chrome.scripting.executeScript(details));
}

// Starts `work` and gives what it answers, or fails if it has not answered within
// PAGE_WAIT_MS.
async function withinPageWait(work) {
  let timer;
  const late = new Promise((_, reject) => {
    timer = setTimeout(
      () => reject(new ToolError(`the page did not answer within ${PAGE_WAIT_MS / 1000} seconds`)),
      PAGE_WAIT_MS,
    );
  });
  try {
    return await Promise.race([work(), late]);
  } finally {
    clearTimeout(timer);
  }
}

// What the injected function `listInPage` gives for the top document of the
// open tab at `url`, which is found by its URL exactly. The URL has to be an
// HTTP or HTTPS address, as it is for pageNamed. Fails, giving nothing of the
// page, where the tab moved before the script ran.
async function listedIn(chrome, url, kind) {
  if (!webUrl(url)) {
    throw new ToolError("url must be an HTTP or HTTPS URL", INVALID_PARAMS);
  }
  const sites = await blockedSites(chrome);
  refuseBlocked(sites, url);
  const tab = await tabAt(chrome, url);
  let injected;
  try {
    injected = await runInPage(chrome, {
      target: { tabId: tab.id },
      world: "ISOLATED",
      func: listInPage,
      args: [url, kind],
    });
  } catch (error) {
    throw new ToolError(`the page at ${url} cannot be read: ${error.message}`);
  }
  const page = injected?.[0]?.result;
  if (!page) {
    throw new ToolError(`the page at ${url} returned nothing when read`);
  }
  if (page.url !== url) {
    throw new ToolError(`the tab left ${url} before it was read`);
  }
  return { sites, page };
}

// A count of characters that runs out: `take` says whether the strings fit in
// what is left of PAGE_TEXT_LIMIT, and uses them up if they do.
function characters() {
  let room = PAGE_TEXT_LIMIT;
  return (...strings) => {
    const size = strings.reduce((total, string) => total + string.length, 0);
    if (size > room) {
      return false;
    }
    room -= size;
    return true;
  };
}

// The tab at the URL the call names, found after the blocked list is read and
// before any tab is looked for, as list_frames does.
async function tabNamed(chrome, params) {
  const url = text(params, "url");
  const sites = await blockedSites(chrome);
  refuseBlocked(sites, url);
  return { url, tab: await tabAt(chrome, url) };
}

// tabNamed for a tool that reads or acts on the elements of a page. The URL has
// to be an HTTP or HTTPS address, as it is for the tools that capture a page, so
// a tab at a file: or ftp: address, which the browser shows an extension that is
// allowed on them, is not read or acted on. It is checked before any tab is
// looked for.
async function pageNamed(chrome, params) {
  if (!webUrl(text(params, "url"))) {
    throw new ToolError("url must be an HTTP or HTTPS URL", INVALID_PARAMS);
  }
  return tabNamed(chrome, params);
}

// What the tab is moved to where a navigation did not end on the host the person
// was shown. The page it reached is replaced without a request to anything.
const BLANK_PAGE = "about:blank";

// Moves the tab at the URL the call names with `move`, which navigates it, and
// gives the URL its top frame committed. The question shows that URL and, for a
// navigation to another address, `to`; the page has to commit on the host of the
// one the person was shown last, as a tab opened by open_tab does. A tab the
// person had open is not closed for failing that, which would be what
// close_tab does: it is left blank, which also ends a load still going. Both
// URLs are checked against the blocked sites before any tab is looked for.
async function movedTab(chrome, params, to, move) {
  const url = to === undefined ? webAddress(params) : text(params, "url");
  const approved = to ?? url;
  const sites = await blockedSites(chrome);
  refuseBlocked(sites, url);
  refuseBlocked(sites, approved);
  const tab = await tabAt(chrome, url);
  const { event } = await openedTab(chrome, async () => {
    await move(tab);
    return tab;
  });
  return {
    url: await checkLanding(sites, approved, event, {
      subject: to === undefined ? `the tab at ${url}` : `the tab moved to ${to}`,
      left: "the tab was left blank",
      leave: () =>
        chrome.tabs.update(tab.id, { url: BLANK_PAGE }).catch(() => {}),
    }),
  };
}

// An optional true-or-false argument, which is undefined where the call leaves
// it out.
function optionalFlag(params, name) {
  const value = params?.[name];
  if (value !== undefined && typeof value !== "boolean") {
    throw new ToolError(`${name} must be true or false`, INVALID_PARAMS);
  }
  return value;
}

// An optional true-or-false argument, which is true where the call leaves it out.
function flag(params, name) {
  return optionalFlag(params, name) ?? true;
}

// The colors the browser gives a tab group.
const GROUP_COLORS = Object.freeze([
  "grey",
  "blue",
  "red",
  "yellow",
  "green",
  "pink",
  "purple",
  "cyan",
  "orange",
]);

// The id the browser gives a tab that is in no group.
const NO_GROUP = -1;

// The most a page can be zoomed to, as a factor where 1 is 100%.
const MAX_ZOOM = 5;

// The title a call gives a tab group, which is undefined where it gives none.
function groupTitle(params) {
  const title = params?.title;
  if (title !== undefined && typeof title !== "string") {
    throw new ToolError("title must be a string", INVALID_PARAMS);
  }
  return title;
}

// The tabs at the URLs a call lists, each found by its URL exactly. The blocked
// list is read and every URL checked against it before any tab is looked for,
// so a list that holds one blocked URL does not say whether the others are open.
async function tabsNamed(chrome, params) {
  const value = params?.urls;
  if (
    !Array.isArray(value) ||
    value.length === 0 ||
    value.some((url) => typeof url !== "string" || url.trim() === "")
  ) {
    throw new ToolError(
      "urls must be a list of one or more URLs, none of them empty",
      INVALID_PARAMS,
    );
  }
  const urls = [...new Set(value)];
  const sites = await blockedSites(chrome);
  for (const url of urls) {
    refuseBlocked(sites, url);
  }
  const tabs = [];
  for (const url of urls) {
    tabs.push(await tabAt(chrome, url));
  }
  return { urls, tabs };
}

// A string argument that may be left out, which is undefined where it is, and
// otherwise one that has something in it.
function optionalText(params, name) {
  return params?.[name] === undefined ? undefined : text(params, name);
}

// The HTTP or HTTPS URL a call gives as `name`. A bookmark or reading list entry
// made at any other address could be a bookmarklet, which runs in a page when it
// is opened.
function webAddress(params, name = "url") {
  const url = text(params, name);
  if (!webUrl(url)) {
    throw new ToolError(`${name} must be an HTTP or HTTPS URL`, INVALID_PARAMS);
  }
  return url;
}

// Whether two addresses are one, once the URL parser has written both the same
// way, which is how the browser holds an address. A browser's search that
// answers with an address that only resembles the one asked for is not this.
function sameAddress(held, asked) {
  try {
    return new URL(held).href === new URL(asked).href;
  } catch {
    return held === asked;
  }
}

// The folders at the top of the bookmark tree, which the browser makes and
// titles in the person's language.
async function builtInFolders(chrome) {
  const [root] = await chrome.bookmarks.getTree();
  return (root?.children ?? []).filter((node) => typeof node.url !== "string");
}

// The one bookmark folder with exactly this title, which is how a folder is
// named: its id is a number that says nothing a person can judge. A title that
// is no folder's, or is the title of more than one, names none. A node the
// browser returns with another title is not kept. The browser's search leaves
// out the built-in folders at the top of the tree (Bookmarks bar, Other
// bookmarks), so those are read from the tree and counted with the search's
// finds, once each.
async function bookmarkFolder(chrome, title) {
  const found = await foldersTitled(chrome, title);
  if (found.length === 0) {
    throw new ToolError(`no bookmark folder is titled ${title}`);
  }
  if (found.length > 1) {
    throw new ToolError(`more than one bookmark folder is titled ${title}`);
  }
  return found[0];
}

// Every folder with exactly this title, the built-in ones included, each once.
async function foldersTitled(chrome, title) {
  const searched = await chrome.bookmarks.search({ title });
  const found = [];
  for (const node of [...searched, ...(await builtInFolders(chrome))]) {
    if (
      typeof node.url !== "string" &&
      node.title === title &&
      !found.some((held) => held.id === node.id)
    ) {
      found.push(node);
    }
  }
  return found;
}

// The one bookmark at exactly this URL, which is how a bookmark is named. Two
// bookmarks at one URL are different bookmarks a person could not tell apart by
// the URL, so the call fails and says how many there are, as it does for two
// tabs at one URL. Folders have no URL, so none is found. A node the browser
// returns at another address is not kept.
async function bookmarkAt(chrome, url) {
  const found = (await chrome.bookmarks.search({ url })).filter(
    (node) => typeof node.url === "string" && sameAddress(node.url, url),
  );
  if (found.length === 0) {
    throw new ToolError(`no bookmark is at ${url}`);
  }
  if (found.length > 1) {
    throw new ToolError(
      `${found.length} bookmarks are at ${url}, so the URL does not name one bookmark`,
    );
  }
  return found[0];
}

// The title of the bookmark folder with this id, as the browser holds it.
async function folderTitled(chrome, id) {
  const [folder] = await chrome.bookmarks.get(id);
  return folder?.title ?? "";
}

// The reading list entry at exactly this URL, which is how an entry is named.
// The list holds one entry for an address. An entry the browser returns at
// another address is not kept.
async function entryAt(chrome, url) {
  const entry = (await chrome.readingList.query({ url })).find((held) =>
    sameAddress(held.url, url),
  );
  if (!entry) {
    throw new ToolError(`no reading list entry is at ${url}`);
  }
  return entry;
}

// The size of a window a call gives, in pixels.
function windowSize(params, name) {
  const size = params?.[name];
  if (!Number.isInteger(size) || size < 1) {
    throw new ToolError(
      `${name} must be a whole number of pixels, 1 or more`,
      INVALID_PARAMS,
    );
  }
  return size;
}

// The host of a URL or an origin, or null where it has none.
function hostOf(value) {
  try {
    return new URL(value).hostname;
  } catch {
    return null;
  }
}

// Whether `url` is at the host of a blocked site, whatever its scheme or port. A
// cookie belongs to a host and is sent to each scheme and port of it, so the
// blocked list, which tells one origin from another for pages, covers the host
// where cookies are concerned.
function atBlockedHost(sites, url) {
  const host = hostOf(url);
  return host !== null && sites.some((site) => hostOf(site) === host);
}

// Whether a cookie is a blocked site's: the host of a blocked site is its
// domain or a host inside it. A cookie for `.example.com` is also sent to a
// blocked `mail.example.com`, so it is the blocked site's wherever it is found.
function cookieAtBlockedHost(sites, cookie) {
  const domain =
    typeof cookie.domain === "string"
      ? cookie.domain.replace(/^\./, "").toLowerCase()
      : "";
  return (
    domain !== "" &&
    sites.some((site) => {
      const host = hostOf(site);
      return host === domain || host?.endsWith(`.${domain}`) === true;
    })
  );
}

// The browser's privacy settings that get_privacy_settings reads, as the area
// of chrome.privacy and the setting in it. A setting this browser does not have
// is left out.
const PRIVACY_SETTINGS = Object.freeze([
  ["network", "networkPredictionEnabled"],
  ["network", "webRTCIPHandlingPolicy"],
  ["services", "alternateErrorPagesEnabled"],
  ["services", "autofillAddressEnabled"],
  ["services", "autofillCreditCardEnabled"],
  ["services", "passwordSavingEnabled"],
  ["services", "safeBrowsingEnabled"],
  ["services", "safeBrowsingExtendedReportingEnabled"],
  ["services", "searchSuggestEnabled"],
  ["services", "spellingServiceEnabled"],
  ["services", "translationServiceEnabled"],
  ["websites", "adMeasurementEnabled"],
  ["websites", "doNotTrackEnabled"],
  ["websites", "fledgeEnabled"],
  ["websites", "hyperlinkAuditingEnabled"],
  ["websites", "protectedContentEnabled"],
  ["websites", "referrersEnabled"],
  ["websites", "thirdPartyCookiesAllowed"],
  ["websites", "topicsEnabled"],
]);

// The kinds of content that get_content_settings reads a setting for. The
// kinds the browser no longer lets a setting change are not among them.
const CONTENT_TYPES = Object.freeze([
  "cookies",
  "images",
  "javascript",
  "location",
  "notifications",
  "popups",
  "automaticDownloads",
  "camera",
  "microphone",
  "clipboard",
]);

// The generic families get_font_settings reads the font of.
const FONT_FAMILIES = Object.freeze([
  "standard",
  "sansserif",
  "serif",
  "fixed",
  "cursive",
  "fantasy",
  "math",
]);

// How many seconds without input make get_idle_state call the system idle where
// the call gives none, which is the browser's own default.
const IDLE_SECONDS = 60;

// Runs each of `reads`, a list of [name, read], and gives { name, ...what read
// gave } for each that answered and { name, error } with the browser's words
// for each that did not, so one setting the browser will not say does not take
// the others with it.
async function readEach(reads) {
  return Promise.all(
    reads.map(async ([name, read]) => {
      try {
        return { name, ...(await read()) };
      } catch (error) {
        return { name, error: String(error?.message ?? error) };
      }
    }),
  );
}

// The kinds of browsing data clear_browsing_data may clear, as the keys of the
// browser's data set. A kind the Chrome reference marks deprecated (appcache,
// passwords, pluginData, serverBoundCertificates, webSQL) is not among them.
const BROWSING_DATA_TYPES = Object.freeze([
  "cache",
  "cacheStorage",
  "cookies",
  "downloads",
  "fileSystems",
  "formData",
  "history",
  "indexedDB",
  "localStorage",
  "serviceWorkers",
]);

// A moment a call names: a date and a time with its offset, so the question
// shows a time a person can read and no zone is assumed.
const INSTANT =
  /^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2}):(\d{2})(?:\.\d{1,3})?(?:Z|[+-]\d{2}:\d{2})$/;

// A moment the call names in the argument `name`, in milliseconds since 1970. A
// number, a date with no time, a time with no offset, a day the month does not
// have and a moment before 1970 are refused. The browser reads 0 as the start
// of everything, so a bound that is left out or written as a number is never
// passed on as a time the person read. Date.parse refuses a month, minute or
// second out of range and accepts a day past the end of the month and hour 24,
// which are refused here.
function instant(params, name) {
  const value = params?.[name];
  const parts = typeof value === "string" ? INSTANT.exec(value) : null;
  const [year, month, day, hour] = (parts ?? []).slice(1, 5).map(Number);
  const milliseconds = parts === null ? NaN : Date.parse(value);
  if (
    !(milliseconds >= 0) ||
    day > new Date(Date.UTC(year, month, 0)).getUTCDate() ||
    hour > 23
  ) {
    throw new ToolError(
      `${name} must be an ISO 8601 date and time with its offset, such as 2026-10-01T00:00:00Z, from 1970 on`,
      INVALID_PARAMS,
    );
  }
  return milliseconds;
}

// What close_window says when a tab in the window is at a blocked site. It
// goes back to the session, so it names the list and nothing of the site.
const WINDOW_BLOCKED_MESSAGE =
  "that window has a tab at a site in the blocked sites list in the BraveBot extension's options, so it is not closed";

// The downloads the browser lists that have the name a call gives: `filename`,
// the absolute path the browser holds for a download, or `url`, the address it
// started from, one of the two and exactly as the browser holds it. A download
// is never named by its id, which a person cannot read in the question. A
// call that gives both, neither or text that is no name fails before the
// browser is asked, and so does a URL at a blocked site. Fails where no
// download has the name.
async function downloadsNamed(chrome, params) {
  const named = ["filename", "url"].filter(
    (name) => params?.[name] !== undefined,
  );
  if (named.length !== 1) {
    throw new ToolError(
      "give the filename or the url of the download, one of them",
      INVALID_PARAMS,
    );
  }
  const [by] = named;
  const value = text(params, by);
  const sites = await blockedSites(chrome);
  if (by === "url") {
    refuseBlocked(sites, value);
  }
  const matching = (await chrome.downloads.search({ [by]: value })).filter(
    (item) => item[by] === value,
  );
  if (matching.length === 0) {
    throw new ToolError(`no download has the ${by} ${value}`);
  }
  return { by, value, matching, sites };
}

// Whether a download started at a blocked site or ended at one, after its
// redirects.
function fromBlocked(sites, item) {
  return isBlocked(sites, item.url) || isBlocked(sites, item.finalUrl);
}

// What pause_download, resume_download, cancel_download and show_download each
// ask of the browser, the word a refusal uses for it, and which of the
// downloads that have a name it applies to. A download in progress is paused or
// cancelled, one that is paused or interrupted and can be resumed is resumed,
// and the folder of a file the browser does not say is gone is shown.
const DOWNLOAD_ACTIONS = Object.freeze({
  pause: {
    run: (chrome, id) => chrome.downloads.pause(id),
    done: "paused",
    fits: (item) => item.state === "in_progress" && item.paused !== true,
  },
  resume: {
    run: (chrome, id) => chrome.downloads.resume(id),
    done: "resumed",
    fits: (item) => item.canResume === true,
  },
  cancel: {
    run: (chrome, id) => chrome.downloads.cancel(id),
    done: "cancelled",
    fits: (item) => item.state === "in_progress",
  },
  show: {
    run: (chrome, id) => chrome.downloads.show(id),
    done: "shown",
    fits: (item) => item.exists !== false,
  },
});

// How a refusal describes the state of a download, and that its file is gone
// where the browser says so.
function downloadState(item) {
  const state =
    item.state === "in_progress"
      ? item.paused === true
        ? "paused"
        : "in progress"
      : String(item.state);
  return item.exists === false ? `${state} with its file gone` : state;
}

// The one download `action` applies to among those with the name the call
// gives. Two that it applies to are two the question cannot tell apart, so the
// call fails and says how many, and a download that started at a blocked site
// is refused.
async function downloadFor(chrome, params, action) {
  const { by, value, matching, sites } = await downloadsNamed(chrome, params);
  const fit = matching.filter(action.fits);
  if (fit.length === 0) {
    throw new ToolError(
      matching.length === 1
        ? `the download with the ${by} ${value} is ${downloadState(matching[0])}, so it cannot be ${action.done}`
        : `none of the ${matching.length} downloads with the ${by} ${value} can be ${action.done}`,
    );
  }
  if (fit.length > 1) {
    throw new ToolError(
      `${fit.length} downloads with the ${by} ${value} can be ${action.done}, so it does not name one download`,
    );
  }
  if (fromBlocked(sites, fit[0])) {
    throw new Refusal(BLOCKED_MESSAGE);
  }
  return fit[0];
}

// Pauses, resumes or cancels the download the call names, and says what the
// browser reports for it afterwards, which is read again and not assumed.
async function changeDownload(chrome, params, action) {
  const item = await downloadFor(chrome, params, action);
  await action.run(chrome, item.id);
  const [now] = await chrome.downloads.search({ id: item.id });
  return {
    filename: item.filename,
    url: item.url,
    state: now?.state ?? null,
    paused: now?.paused === true,
    error: now?.error ?? null,
  };
}

// An argument that is one of a fixed list of words, or `fallback` where the
// call leaves it out and there is one.
function oneOf(params, name, list, fallback) {
  const value = params?.[name];
  if (value === undefined && fallback !== undefined) {
    return fallback;
  }
  if (typeof value !== "string" || !list.includes(value)) {
    throw new ToolError(
      `${name} must be one of ${list.join(", ")}`,
      INVALID_PARAMS,
    );
  }
  return value;
}

// A string argument that has something in it and is at most `limit` long.
function shortText(params, name, limit) {
  const value = text(params, name);
  if (value.length > limit) {
    throw new ToolError(
      `${name} may hold at most ${limit} characters`,
      INVALID_PARAMS,
    );
  }
  return value;
}

// A whole number argument from `least` to `most`, or `fallback` where the call
// leaves it out.
function wholeNumber(params, name, least, most, fallback) {
  const value = params?.[name] ?? fallback;
  if (!Number.isInteger(value) || value < least || value > most) {
    throw new ToolError(
      `${name} must be a whole number from ${least} to ${most}`,
      INVALID_PARAMS,
    );
  }
  return value;
}

// The element a call names: a role from the list and an accessible name. A tool
// that acts on an element (`numbered`) may name it instead by element_number,
// the number it has in the last list_page_elements, and takes one form and not
// both. The number is checked against the list when the page is reached
// (resolveElement).
function elementNamed(params, numbered = false) {
  if (numbered && params?.element_number !== undefined) {
    if (params.role !== undefined || params.name !== undefined) {
      throw new ToolError(
        "name the element by element_number, or by role and name, and not both",
        INVALID_PARAMS,
      );
    }
    return { number: wholeNumber(params, "element_number", 1, MAX_LISTED) };
  }
  return {
    role: oneOf(params, "role", ROLES),
    name: shortText(params, "name", NAME_LIMIT),
  };
}

// Where a listed element goes, as a list shows it: the origin of a web address,
// the scheme alone for another kind (`javascript:`, `mailto:`), and nothing for an
// element that goes nowhere.
function destinationOf(goes) {
  if (goes === "") {
    return "";
  }
  return originOf(goes) ?? /^[a-z][a-z0-9+.-]*:/i.exec(goes)?.[0].toLowerCase() ?? "";
}

// The most characters of the page's words that the page gives back when it shows
// an element. The window cuts them to its own limits and says how much it left
// out, so they are given whole up to a cap, and an element with more is refused.
const SHOWN_LIMITS = Object.freeze({
  name: SHOWN_CAP,
  excerpt: SHOWN_CAP,
  target: SHOWN_CAP,
});

// The name the description gives what a call typed, picked or pressed.
const TYPED_AS = Object.freeze({ type: "text", select: "option", key: "key" });

// What each action says it did.
const ACTED = Object.freeze({
  click: "clicked",
  double_click: "double clicked",
  right_click: "right clicked",
  type: "typed",
  select: "selected",
  key: "pressed",
});

// A failure the page told with a status word. It holds a sentence from a
// closed list and a count, and nothing the page wrote. A field that is never
// acted on, and an element that is not the one a number was given to, are a
// refusal of the extension, and the rest are a page that is not as the name said.
function pageFailure(answer, wanting = "") {
  if (!isFailure(answer?.status)) {
    return new ToolError("the page answered something the extension does not know");
  }
  const sentence = `${failureText(answer)}${wanting}`;
  return ["password", "card", "file", "stale", "long"].includes(answer.status)
    ? new Refusal(sentence)
    : new ToolError(sentence);
}

// Runs `func` in the top document of the tab, in the extension's isolated world,
// with the one object of arguments the call built, and gives what it answers.
// The page cannot change what the function calls, and nothing of the call but
// that object reaches it.
async function inTab(chrome, tabId, func, request) {
  let injected;
  try {
    injected = await runInPage(chrome, {
      target: { tabId },
      world: "ISOLATED",
      func,
      args: [request],
    });
  } catch (error) {
    throw new ToolError(
      `the page at ${request.url} cannot be reached: ${error.message}`,
    );
  }
  const answer = injected?.[0]?.result;
  if (!answer) {
    throw new ToolError(`the page at ${request.url} returned nothing`);
  }
  return answer;
}

// What the window is given when an element was named by its number: the number
// the call gave and which of the displayed elements with the same role and name
// the element is, as whole numbers the extension made. Nothing is added for an
// element named by its role and name, or when the page does not answer a place
// that is a number from 1 to the count.
function numberShown(number, position) {
  const whole = (value) => Number.isSafeInteger(value) && value >= 1;
  if (number === undefined || !whole(number)) {
    return {};
  }
  if (!whole(position?.index) || !whole(position?.of) || position.index > position.of) {
    return { number };
  }
  return {
    number,
    position: Object.freeze({ index: position.index, of: position.of }),
  };
}

// The limits of a look that shows the person nothing, as wait_for_element does:
// an element is found however long its words are.
const SEEN_LIMITS = Object.freeze({
  name: Number.MAX_SAFE_INTEGER,
  excerpt: Number.MAX_SAFE_INTEGER,
  target: Number.MAX_SAFE_INTEGER,
});

// Finds the element a call names, in the tab at its URL, and says what it is
// for the person to be shown. With `outline` the page also brings the element
// into view and outlines it, and the caller removes the outline (clearOutline)
// whatever comes of the question. The resolution keeps the call's own words,
// what the page said of the element and its `description`, which is text and
// numbers only and is what a window shows: the action, the tab's URL and its
// origin, the element's tag, role and name, the start of its text, where a link
// or a form goes, and its rectangle. A call that names no element, `target`
// null, is about the element that has the focus. A target with a `number` is the
// element that has that number in the last list made for this tab's URL, which
// the page finds by its place in the document and accepts only if it still has
// the fingerprint the list recorded.
export async function resolveElement(
  chrome,
  params,
  { action, target, typed, outline = false },
) {
  const { url, tab } = await pageNamed(chrome, params);
  if (url.length > SHOWN_CAP) {
    throw new Refusal(
      `the address of the tab is longer than ${SHOWN_CAP} characters, so the window cannot show it whole and nothing was done`,
    );
  }
  const named =
    target?.number === undefined
      ? { role: target ? target.role : null, name: target ? target.name : "" }
      : await recall(chrome, url, target.number);
  const request = { url, ...named, limit: SHOWN_LIMITS };
  const found = await inTab(chrome, tab.id, inPage, {
    ...request,
    step: "find",
    outline: outline ? OUTLINE_MS : 0,
  });
  if (found.status !== "ok") {
    throw pageFailure(found);
  }
  return Object.freeze({
    tabId: tab.id,
    request,
    action,
    typed,
    fingerprint: found.fingerprint,
    description: Object.freeze({
      action,
      ...(typed === undefined ? {} : { [TYPED_AS[action]]: typed }),
      origin: originOf(url),
      url,
      tag: found.tag,
      role: found.role,
      name: found.name,
      excerpt: found.excerpt,
      goes: found.goes,
      ...(found.goesUnknown === true ? { goesUnknown: true } : {}),
      ...numberShown(target?.number, found.position),
      rect: Object.freeze({ ...found.rect }),
    }),
  });
}

// Takes the outline resolveElement drew out of the page, if the page has it. A
// tab that is gone or a page that cannot be reached has none to take, so this
// never fails.
export async function clearOutline(chrome, resolution) {
  try {
    await inTab(chrome, resolution.tabId, clearOutlineInPage, {
      url: resolution.request.url,
    });
  } catch {
    // The outline went with the page.
  }
}

// Does the action of a resolution, to the element it found, once the person
// allowed it. `approved` is the token the window made for this resolution
// (input.js), and a call without one, or with one made for another resolution
// or used already, does nothing and does not touch the page. The page finds the
// element again at this moment and acts only if it is the one that was shown.
export async function performAction(chrome, resolution, approved) {
  if (!spend(approved, resolution)) {
    throw new Refusal(
      "the action was not allowed in the browser, so nothing was done",
    );
  }
  const done = await inTab(chrome, resolution.tabId, inPage, {
    ...resolution.request,
    step: "act",
    action: resolution.action,
    text: resolution.typed,
    fingerprint: resolution.fingerprint,
  });
  if (done.status !== "done") {
    throw pageFailure(done);
  }
  return { status: ACTED[resolution.action], origin: originOf(resolution.request.url) };
}

// What a second action is told while one waits for the person's answer. Only
// one is put to the person at a time, so a window is never hidden behind
// another, and the second is refused and not queued, so that its wait does not
// run on beyond the relay's.
export const BUSY_MESSAGE =
  "another action is waiting for your answer in the browser, so nothing was done; ask again after it is answered";

let asking = false;

// The whole of an action on an element: find it and outline it, bring its tab
// forward and put it in front of the person to wait for their answer, take the
// outline away, then act. The outline goes whether the person allowed it or
// not, and whether the window answered or failed. Only one action is on the way
// at a time.
async function actOnElement(chrome, params, spec) {
  if (asking) {
    throw new Refusal(BUSY_MESSAGE);
  }
  asking = true;
  try {
    const resolution = await resolveElement(chrome, params, { ...spec, outline: true });
    let approved;
    try {
      try {
        await chrome.tabs.update(resolution.tabId, { active: true });
      } catch (error) {
        throw new ToolError(
          `the tab could not be brought forward to show the element: ${error.message}`,
        );
      }
      approved = await confirm(resolution);
    } finally {
      await clearOutline(chrome, resolution);
    }
    return await performAction(chrome, resolution, approved);
  } finally {
    asking = false;
  }
}

// The whole of a call to a tool of input-trusted: the page is named by a web
// URL that is not at a blocked site, then the call is sent to the BraveBot
// Trusted Input extension (trusted.js), which finds the element, asks the person
// in a window of its own and acts. It takes the same place in line as
// actOnElement does, so one window is open at a time across the two extensions.
// `args` are the checked arguments besides the URL, and nothing else is sent.
async function actTrusted(chrome, method, params, args) {
  const url = text(params, "url");
  if (!webUrl(url)) {
    throw new ToolError("url must be an http or https address", INVALID_PARAMS);
  }
  refuseBlocked(await blockedSites(chrome), url);
  if (asking) {
    throw new Refusal(BUSY_MESSAGE);
  }
  asking = true;
  try {
    return await forward(chrome, method, { url, ...args });
  } finally {
    asking = false;
  }
}

// Configures the world of a call and runs the probe of scripts.js in it, before
// the person is asked, and gives the id of the top document the probe ran in.
// Fails, running nothing of the person's, where the browser did not apply the
// policy to the world. The code of the call is run in this world and in this
// document only.
async function probeWorld(chrome, tabId, url, worldId) {
  let probed;
  try {
    await chrome.userScripts.configureWorld({ worldId, csp: WORLD_CSP, messaging: false });
    probed = await withinPageWait(() =>
      chrome.userScripts.execute({
        target: { tabId },
        world: "USER_SCRIPT",
        worldId,
        js: [{ code: POLICY_PROBE }],
      }),
    );
  } catch (error) {
    throw new ToolError(`the page at ${url} cannot be scripted: ${error.message}`);
  }
  const answered = probed?.[0];
  if (!answered) {
    throw new ToolError(`the page at ${url} returned nothing`);
  }
  if (answered.result !== POLICY_STOPPED) {
    throw new Refusal(POLICY_NOT_APPLIED);
  }
  if (typeof answered.documentId !== "string" || answered.documentId === "") {
    throw new ToolError(`the page at ${url} did not name its document, so nothing was run`);
  }
  return answered.documentId;
}

// Fails unless exactly one open tab is at `url` and it is the tab `tabId`.
async function stillTheOneTab(chrome, tabId, url) {
  const open = (await chrome.tabs.query({})).filter((candidate) => candidate.url === url);
  if (open.length > 1) {
    throw new ToolError(
      `${open.length} open tabs are at ${url} now, so the URL does not name one tab and nothing was run`,
    );
  }
  if (open.length === 0 || open[0].id !== tabId) {
    throw new ToolError(`the tab left ${url} before the script ran, so nothing was run`);
  }
}

// The whole of a call to run_script: the code and the page are checked, the
// world of the call is made and the browser's policy for it is tested with a
// probe of the extension's own, the person is shown all of the code and the
// exact address of the page in a window of this extension, and only if they allow
// it is the code started, once, in that world and in the document the probe
// found (scripts.js). The code is the planner's argument and is sent to the
// browser as written: nothing here reads it except to count it, to refuse one that
// is only the name of a reference, and to show it.
// When the person answers, exactly one tab has to be at the address and it has to
// be the one they were shown, and the browser is asked to run the code in the
// document that was shown, so a page that reloads or navigates meanwhile is not
// scripted. The tab has to be at the address again when the code has returned,
// and an answer from a tab that left it is not returned. What the code set going
// in the page, such as a timer or a listener, is not stopped when the call ends.
// It takes the same place in line as an action on an element does.
async function runScript(chrome, params) {
  const code = shortText(params, "code", CODE_LIMIT);
  if (namesAReference(code)) {
    throw new Refusal(
      "code is the planner's own JavaScript and not the name of a reference, whose bytes are never run, so nothing was run",
    );
  }
  const { url, tab } = await pageNamed(chrome, params);
  if (url.length > URL_LIMIT) {
    throw new Refusal(
      `the address of the tab is longer than ${URL_LIMIT} characters, so the window cannot show it whole and nothing was run`,
    );
  }
  if (!(await userScriptsAvailable(chrome))) {
    throw new Refusal(UNAVAILABLE);
  }
  if (asking) {
    throw new Refusal(BUSY_MESSAGE);
  }
  asking = true;
  const worldId = newWorldId();
  try {
    const documentId = await probeWorld(chrome, tab.id, url, worldId);
    const resolution = Object.freeze({
      tabId: tab.id,
      url,
      code,
      description: describeScript({ origin: originOf(url), url, code }),
    });
    if (!spend(await confirm(resolution), resolution)) {
      throw new Refusal("the script was not allowed in the browser, so nothing was run");
    }
    await stillTheOneTab(chrome, tab.id, url);
    let injected;
    try {
      injected = await withinPageWait(() =>
        chrome.userScripts.execute({
          target: { tabId: tab.id, documentIds: [documentId] },
          world: "USER_SCRIPT",
          worldId,
          js: [{ code }],
        }),
      );
    } catch (error) {
      throw new ToolError(`the page at ${url} cannot be scripted: ${error.message}`);
    }
    const answered = injected?.[0];
    if (!answered) {
      throw new ToolError(`the page at ${url} returned nothing`);
    }
    const after = await chrome.tabs.get(tab.id).catch(() => undefined);
    if (after?.url !== url) {
      throw new ToolError(`the tab left ${url} while the script ran, so its answer is not returned`);
    }
    const { text: result, cut } = resultText(answered.result);
    return { status: "ran", origin: originOf(url), result, ...(cut ? { cut: true } : {}) };
  } finally {
    try {
      await chrome.userScripts.resetWorldConfiguration(worldId);
    } catch {
      // The entry for a world that was used once is left to the browser.
    }
    asking = false;
  }
}

// What a call is told about the file it carries (files.js), by the word of the
// first thing wrong with it.
const FILE_PROBLEMS = Object.freeze({
  path: "path must be the absolute path of one file, as text that a person can read",
  size: "size must be a whole number of bytes from 0 to 10485760",
  file_name: "file_name must be the last part of path",
  credential: "the name of that file looks like a credential, a key or a secret, so it is never given to a page",
});

export const TOOLS = {
  // What Brave runs on, which is enough to show the extension is installed and
  // answering, and nothing about the person. Only these fields are passed on,
  // whatever else the browser adds to its answer.
  async get_platform_info(chrome) {
    const { os, arch, nacl_arch } = await chrome.runtime.getPlatformInfo();
    return { os, arch, nacl_arch };
  },

  // A tab whose page or next page is at a blocked site is left out. A tab that
  // has not committed has no URL yet and a title that is the address it is
  // loading, so the address it is going to counts as well.
  async list_tabs(chrome) {
    const sites = await blockedSites(chrome);
    const tabs = await chrome.tabs.query({});
    return tabs
      .filter(
        (tab) =>
          !isBlocked(sites, tab.url) && !isBlocked(sites, tab.pendingUrl),
      )
      .map((tab) => ({
        id: tab.id,
        window_id: tab.windowId,
        title: tab.title ?? "",
        url: tab.url ?? "",
      }));
  },

  // Frames at a blocked site, and frames inside one, are left out of the list,
  // as tabs are.
  async list_frames(chrome, params) {
    const url = text(params, "url");
    const sites = await blockedSites(chrome);
    refuseBlocked(sites, url);
    const tab = await tabAt(chrome, url);
    const frames = await framesAt(chrome, tab, url);
    return frames
      .filter(
        (frame) => webUrl(frame.url) && !behindBlocked(sites, frames, frame),
      )
      .map((frame) => ({ url: frame.url, top: frame.frameId === 0 }));
  },

  // Windows are listed with no tab in them, so no address or title is asked
  // for, and the blocked sites have nothing to leave out. Only these fields are
  // passed on, whatever else the browser adds to a window.
  async list_windows(chrome) {
    const windows = await chrome.windows.getAll({ populate: false });
    return windows.map((window) => ({
      id: window.id,
      type: window.type,
      state: window.state,
      focused: window.focused,
      incognito: window.incognito,
    }));
  },

  // A group has a title and a color a person chose, and no address.
  async list_tab_groups(chrome) {
    const groups = await chrome.tabGroups.query({});
    return groups.map((group) => ({
      id: group.id,
      window_id: group.windowId,
      title: group.title ?? "",
      color: group.color,
      collapsed: group.collapsed,
    }));
  },

  // A tab at a blocked site is left out, and so is a closed window with
  // nothing left to show. The browser keeps at most 25 closed tabs and windows
  // (sessions.MAX_SESSION_RESULTS), and gives the time each closed in seconds.
  async list_recently_closed(chrome) {
    const sites = await blockedSites(chrome);
    const closed = await chrome.sessions.getRecentlyClosed();
    return closed.flatMap((session) => {
      const tabs = sessionTabs(sites, session);
      if (tabs.length === 0) {
        return [];
      }
      const at = isoTime(session.lastModified * 1000);
      return [
        session.tab
          ? { kind: "tab", closed: at, ...tabs[0] }
          : { kind: "window", closed: at, tabs },
      ];
    });
  },

  // The tab is found by its URL exactly, since the URL is what a person saw in
  // the question before the call. A tab whose URL only resembles it is a
  // different page.
  //
  // A frame is likewise found by the exact web URL list_frames returned. Two
  // frames at that URL are refused because the question cannot distinguish
  // them. The script says which document it read, and a framed read also checks
  // that its outer tab did not move while the script ran.
  async read_page(chrome, params) {
    const url = text(params, "url");
    const frameUrl =
      params?.frame_url === undefined ? undefined : text(params, "frame_url");
    const sites = await blockedSites(chrome);
    refuseBlocked(sites, url);
    if (frameUrl !== undefined) {
      refuseBlocked(sites, frameUrl);
    }
    const tab = await tabAt(chrome, url);
    let frame;
    if (frameUrl !== undefined) {
      if (!webUrl(frameUrl)) {
        throw new ToolError(
          "frame_url must be an HTTP or HTTPS URL from list_frames",
          INVALID_PARAMS,
        );
      }
      const frames = await framesAt(chrome, tab, url);
      // A frame inside a blocked one is not found, as it is not listed.
      const matches = frames.filter(
        (candidate) =>
          candidate.url === frameUrl &&
          !behindBlocked(sites, frames, candidate),
      );
      if (matches.length === 0) {
        throw new ToolError(`no frame in ${url} is at ${frameUrl}`);
      }
      if (matches.length > 1) {
        throw new ToolError(`more than one frame is at ${frameUrl} in ${url}`);
      }
      [frame] = matches;
    }
    let injected;
    try {
      injected = await runInPage(chrome, {
        target: {
          tabId: tab.id,
          ...(frame ? { documentIds: [frame.documentId] } : {}),
        },
        world: "ISOLATED",
        func: readInPage,
        args: [frameUrl ?? url],
      });
    } catch (error) {
      throw new ToolError(
        `the page at ${frameUrl ?? url} cannot be read: ${error.message}`,
      );
    }
    const page = injected?.[0]?.result;
    if (!page) {
      throw new ToolError(
        `the page at ${frameUrl ?? url} returned nothing when read`,
      );
    }
    if (page.url !== (frameUrl ?? url)) {
      const subject = frame ? "the frame" : "the tab";
      throw new ToolError(
        `${subject} left ${frameUrl ?? url} before it was read`,
      );
    }
    if (frame) {
      const current = await chrome.tabs.get(tab.id);
      if (current?.url !== url) {
        throw new ToolError(`the tab left ${url} before its frame was read`);
      }
    }
    const truncated = page.text.length > PAGE_TEXT_LIMIT;
    return {
      url,
      ...(frame ? { frame_url: frameUrl } : {}),
      title: page.title,
      text: truncated ? cut(page.text, PAGE_TEXT_LIMIT) : page.text,
      truncated,
    };
  },

  async search_history(chrome, params) {
    const query = text(params, "query");
    const sites = await blockedSites(chrome);
    const wanted = count(params);
    // Without a start time the history API searches the last 24 hours only.
    const items = await chrome.history.search({
      text: query,
      startTime: 0,
      maxResults: sites.length === 0 ? wanted : BLOCKED_SCAN_LIMIT,
    });
    return items
      .filter((item) => !isBlocked(sites, item.url))
      .slice(0, wanted)
      .map((item) => ({
        title: item.title ?? "",
        url: item.url ?? "",
        last_visit:
          item.lastVisitTime === undefined
            ? null
            : new Date(item.lastVisitTime).toISOString(),
      }));
  },

  // The visits to one URL, which is the one a person read in the question. A
  // URL at a blocked site is refused before the browser is asked. The newest
  // visits are returned, up to the count asked for.
  async get_visits(chrome, params) {
    const url = text(params, "url");
    const sites = await blockedSites(chrome);
    refuseBlocked(sites, url);
    const visits = await chrome.history.getVisits({ url });
    return [...visits]
      .sort((a, b) => b.visitTime - a.visitTime)
      .slice(0, count(params))
      .map((visit) => ({
        time: isoTime(visit.visitTime),
        transition: visit.transition,
        local: visit.isLocal,
      }));
  },

  // The browser decides which sites are the most visited. A site on the
  // blocked list is left out.
  async list_top_sites(chrome) {
    const sites = await blockedSites(chrome);
    const top = await chrome.topSites.get();
    return top
      .filter((site) => !isBlocked(sites, site.url))
      .map((site) => ({ title: site.title ?? "", url: site.url ?? "" }));
  },

  // The tabs the person has open on their other devices, as Brave last synced
  // them, by device. A device is listed whatever its tabs, with the ones at a
  // blocked site left out.
  async list_synced_tabs(chrome) {
    const sites = await blockedSites(chrome);
    const devices = await chrome.sessions.getDevices();
    return devices.map((device) => ({
      device: device.deviceName ?? "",
      tabs: (device.sessions ?? []).flatMap((session) =>
        sessionTabs(sites, session),
      ),
    }));
  },

  async search_bookmarks(chrome, params) {
    const query = text(params, "query");
    const sites = await blockedSites(chrome);
    const nodes = await chrome.bookmarks.search(query);
    return nodes
      .filter((node) => typeof node.url === "string")
      .filter((node) => !isBlocked(sites, node.url))
      .slice(0, count(params))
      .map((node) => ({ title: node.title ?? "", url: node.url }));
  },

  // The folder is found by its title, exactly as written, since the title is
  // what a person read in the question. A title that is a folder's in more
  // than one place is refused, as two frames at one URL are, because the
  // question cannot tell them apart. A bookmark at a blocked site is left out
  // and a folder keeps its other entries. A call that names no folder lists
  // the titles of the built-in folders at the top of the tree, which are the
  // titles the other bookmark tools take.
  async list_bookmark_folder(chrome, params) {
    const title = optionalText(params, "folder");
    if (title === undefined) {
      return (await builtInFolders(chrome))
        .slice(0, count(params))
        .map((node) => ({ kind: "folder", title: node.title ?? "" }));
    }
    const sites = await blockedSites(chrome);
    const folder = await bookmarkFolder(chrome, title);
    const children = await chrome.bookmarks.getChildren(folder.id);
    return children
      .filter(
        (node) => typeof node.url !== "string" || !isBlocked(sites, node.url),
      )
      .slice(0, count(params))
      .map((node) =>
        typeof node.url === "string"
          ? { kind: "bookmark", title: node.title ?? "", url: node.url }
          : { kind: "folder", title: node.title ?? "" },
      );
  },

  // The newest bookmarks, newest first, each with the title of the folder it
  // sits in, so a plan can move a bookmark into a folder that already exists
  // rather than make a second of the same title. While a site is blocked the
  // browser is asked for more than the count, as a history search is, so that
  // leaving out the entries at a blocked site does not return fewer than there
  // are.
  async list_recent_bookmarks(chrome, params) {
    const sites = await blockedSites(chrome);
    const wanted = count(params);
    const nodes = await chrome.bookmarks.getRecent(
      sites.length === 0 ? wanted : BLOCKED_SCAN_LIMIT,
    );
    const kept = nodes
      .filter((node) => typeof node.url === "string")
      .filter((node) => !isBlocked(sites, node.url))
      .sort((a, b) => (b.dateAdded ?? 0) - (a.dateAdded ?? 0))
      .slice(0, wanted);
    const folders = new Map();
    for (const id of new Set(kept.map((node) => node.parentId))) {
      // A folder the browser cannot find names no folder, rather than failing the list.
      if (id !== undefined) {
        folders.set(id, await folderTitled(chrome, id).catch(() => ""));
      }
    }
    return kept.map((node) => ({
      title: node.title ?? "",
      url: node.url,
      added: isoTime(node.dateAdded),
      folder: folders.get(node.parentId) ?? "",
    }));
  },

  // The reading list, newest first. An entry at a blocked site is left out.
  async list_reading_list(chrome, params) {
    const sites = await blockedSites(chrome);
    const entries = await chrome.readingList.query({});
    return entries
      .filter((entry) => !isBlocked(sites, entry.url))
      .sort((a, b) => (b.creationTime ?? 0) - (a.creationTime ?? 0))
      .slice(0, count(params))
      .map((entry) => ({
        title: entry.title ?? "",
        url: entry.url ?? "",
        read: entry.hasBeenRead === true,
        added: isoTime(entry.creationTime),
      }));
  },

  // Opens `url` in a new background tab, which is a request to that site with
  // the person's cookies, so the URL is the one they read in the question. The
  // tab is closed and the call fails if the page that commits is on another
  // host: a redirect to a host nobody was shown is not what was approved. The
  // reply carries the committed URL and no page content. A site on the blocked
  // list is not opened, and a tab that commits at one is closed.
  async open_tab(chrome, params) {
    const url = text(params, "url");
    if (!webUrl(url)) {
      throw new ToolError("url must be an HTTP or HTTPS URL", INVALID_PARAMS);
    }
    const sites = await blockedSites(chrome);
    refuseBlocked(sites, url);
    const { tab, event } = await openedTab(chrome, () =>
      chrome.tabs.create({ url, active: false }),
    );
    return {
      url: await checkLanding(sites, url, event, {
        subject: `the tab opened for ${url}`,
        left: "it was closed",
        leave: () => chrome.tabs.remove(tab.id).catch(() => {}),
      }),
    };
  },

  // Navigates the tab at `url` to `to`, an HTTP or HTTPS URL, with the person's
  // cookies. The tab has to commit on the host of `to`, or it is left blank and
  // the call fails. The reply carries the URL it committed and no page content.
  async navigate_tab(chrome, params) {
    const to = webAddress(params, "to");
    return movedTab(chrome, params, to, (tab) =>
      chrome.tabs.update(tab.id, { url: to }),
    );
  },

  // Loads the page of the tab at `url` again. A page that sends the tab to
  // another host is not what the person was shown, so the tab is left blank.
  async reload_tab(chrome, params) {
    return movedTab(chrome, params, undefined, (tab) =>
      chrome.tabs.reload(tab.id),
    );
  },

  // Moves the tab at `url` back one entry in its history. The question cannot
  // show the entry, so the tab has to land on the host of `url`, or it is left
  // blank and the call fails.
  async go_back(chrome, params) {
    return movedTab(chrome, params, undefined, (tab) =>
      chrome.tabs.goBack(tab.id),
    );
  },

  // Moves the tab at `url` forward one entry in its history, as go_back does
  // back.
  async go_forward(chrome, params) {
    return movedTab(chrome, params, undefined, (tab) =>
      chrome.tabs.goForward(tab.id),
    );
  },

  // Opens `url` in a new window that is not brought forward, as open_tab opens a
  // tab that is not made active. The window is closed and the call fails when
  // its page is on another host, at a blocked site, or does not load.
  async open_window(chrome, params) {
    const url = webAddress(params);
    const sites = await blockedSites(chrome);
    refuseBlocked(sites, url);
    let opened;
    const { event } = await openedTab(chrome, async () => {
      opened = await chrome.windows.create({ url, focused: false });
      const [tab] = opened.tabs ?? [];
      if (!tab) {
        await chrome.windows.remove(opened.id).catch(() => {});
        throw new ToolError(
          `the browser did not say which tab the window opened for ${url} holds, so it was closed`,
        );
      }
      return tab;
    });
    return {
      url: await checkLanding(sites, url, event, {
        subject: `the window opened for ${url}`,
        left: "it was closed",
        leave: () => chrome.windows.remove(opened.id).catch(() => {}),
      }),
    };
  },

  // Restores the recently closed tab that was at `url`, which is how
  // list_recently_closed shows one. A closed window is not restored by this: its
  // tabs would all load, and the question shows one URL. The restored tab loads
  // its page, so it is held to the host of `url` as a tab opened by open_tab is.
  async restore_closed(chrome, params) {
    const url = webAddress(params);
    const sites = await blockedSites(chrome);
    refuseBlocked(sites, url);
    const found = (await chrome.sessions.getRecentlyClosed()).filter(
      (session) => session.tab?.url === url,
    );
    if (found.length === 0) {
      throw new ToolError(`no recently closed tab was at ${url}`);
    }
    if (found.length > 1) {
      throw new ToolError(
        `${found.length} recently closed tabs were at ${url}, so the URL does not name one tab`,
      );
    }
    // Without an id the browser restores the session closed last, whatever it is.
    const sessionId = found[0].tab.sessionId;
    if (typeof sessionId !== "string" || sessionId === "") {
      throw new ToolError(`the browser gave no id for the closed tab at ${url}`);
    }
    const { tab, event } = await openedTab(chrome, async () => {
      const restored = await chrome.sessions.restore(sessionId);
      if (!restored?.tab) {
        throw new ToolError(`the browser did not say which tab it restored for ${url}`);
      }
      return restored.tab;
    });
    return {
      url: await checkLanding(sites, url, event, {
        subject: `the tab restored for ${url}`,
        left: "it was closed",
        leave: () => chrome.tabs.remove(tab.id).catch(() => {}),
      }),
    };
  },

  // Searches for `query` with the person's default search provider, in a new
  // background tab that is made blank first, because search.query takes either
  // a tab to show the results in or a disposition, and only a tab the extension
  // made is one it can name and close. The provider's address is not told to the
  // extension or shown in the question, so the page is not held to a host: it has
  // to be a web page, not at a blocked site, and to commit within the limit, or
  // the tab is closed. The reply carries the URL of the results page and no page
  // content.
  async search_web(chrome, params) {
    const query = text(params, "query");
    if (query.length > SEARCH_TEXT_LIMIT) {
      throw new ToolError(
        `query must be at most ${SEARCH_TEXT_LIMIT} characters`,
        INVALID_PARAMS,
      );
    }
    const sites = await blockedSites(chrome);
    let made;
    const { event } = await openedTab(
      chrome,
      async () => {
        made = await chrome.tabs.create({ url: BLANK_PAGE, active: false });
        try {
          await chrome.search.query({ text: query, tabId: made.id });
        } catch (error) {
          await chrome.tabs.remove(made.id).catch(() => {});
          throw error;
        }
        return made;
      },
      BLANK_PAGE,
    );
    return {
      url: await checkLanding(sites, undefined, event, {
        subject: "the tab opened for the search",
        left: "it was closed",
        leave: () => chrome.tabs.remove(made.id).catch(() => {}),
      }),
    };
  },

  // Waits until the open tab at `url` has finished loading, for at most
  // OPEN_TIMEOUT_MS, and says whether it did. The tab is asked about by its id
  // once found by its URL, and the call fails where the tab was closed or has
  // gone to another URL, since the load that finished would not be the page the
  // person was shown. The reply holds the URL asked for, whether the tab loaded
  // or the time ran out, and no page content.
  async wait_for_load(chrome, params) {
    const url = text(params, "url");
    const sites = await blockedSites(chrome);
    refuseBlocked(sites, url);
    const { id } = await tabAt(chrome, url);
    let timer;
    let poll;
    const timedOut = new Promise((resolve) => {
      timer = setTimeout(resolve, OPEN_TIMEOUT_MS, true);
    });
    try {
      for (;;) {
        const current = await chrome.tabs.get(id).catch(() => undefined);
        if (!current) {
          throw new ToolError(
            `the tab at ${url} was closed before it finished loading`,
          );
        }
        if (current.url !== url) {
          throw new ToolError(`the tab left ${url} before it finished loading`);
        }
        if (current.status === "complete") {
          return { url, status: "loaded" };
        }
        const pause = new Promise((resolve) => {
          poll = setTimeout(resolve, LOAD_POLL_MS, false);
        });
        if (await Promise.race([pause, timedOut])) {
          return { url, status: "timed out" };
        }
      }
    } finally {
      clearTimeout(timer);
      clearTimeout(poll);
    }
  },

  // The links of the top document, in the order the page has them, with the
  // text each is shown with. Only HTTP and HTTPS links are listed, since those
  // are the addresses a later call can name, and a link at a blocked site is
  // left out. The list is cut, not in the middle of a link, where it would pass
  // PAGE_TEXT_LIMIT characters.
  async read_page_links(chrome, params) {
    const url = text(params, "url");
    const { sites, page } = await listedIn(chrome, url, "links");
    const take = characters();
    const links = [];
    let truncated = false;
    for (const link of page.links) {
      if (!webUrl(link.href) || isBlocked(sites, link.href)) {
        continue;
      }
      const shown = cut(link.text, ITEM_TEXT_LIMIT);
      if (!take(link.href, shown)) {
        truncated = true;
        break;
      }
      links.push({ text: shown, url: link.href });
    }
    return { url, links, truncated };
  },

  // The forms of the top document, each with where it submits, how, and the
  // name and type of each of its fields. What a field holds is not read: the
  // function injected into the page never touches a value, so a password, a
  // token in a hidden field or half-typed text does not come back.
  async read_page_forms(chrome, params) {
    const url = text(params, "url");
    const { page } = await listedIn(chrome, url, "forms");
    const take = characters();
    const forms = [];
    let truncated = false;
    for (const form of page.forms) {
      const action = cut(form.action, ITEM_TEXT_LIMIT);
      if (!take(action, form.method)) {
        truncated = true;
        break;
      }
      const fields = [];
      for (const field of form.fields) {
        const name = cut(field.name, ITEM_TEXT_LIMIT);
        if (!take(name, field.type)) {
          truncated = true;
          break;
        }
        fields.push({ name, type: field.type });
      }
      forms.push({ action, method: form.method, fields });
      if (truncated) {
        break;
      }
    }
    return { url, forms, truncated };
  },

  // The elements of the top document of the tab at `url` that the input tools
  // can act on, numbered in document order, as text for a person to read and to
  // name in a later call (numbering.js). Each line is a number, a role, a name
  // cut to a length and the origin the element goes to, and no value a field
  // holds. An element that is not displayed, is never acted on or is disabled is
  // counted and not numbered, and a link to a blocked site is left out as
  // read_page_links leaves it out. The numbers replace those of any earlier
  // list, and refer to this URL.
  async list_page_elements(chrome, params) {
    const { url, tab } = await pageNamed(chrome, params);
    const sites = await blockedSites(chrome);
    const answer = await inTab(chrome, tab.id, inPage, {
      url,
      step: "list",
      limit: LISTING_LIMITS,
    });
    if (answer.status !== "ok") {
      throw pageFailure(answer);
    }
    // An element that goes to a blocked site is left out, and so is one that
    // might, where it cannot be told where it goes and some site is blocked.
    const elements = answer.listed.filter((element) =>
      element.unknown ? sites.length === 0 : !isBlocked(sites, element.goes),
    );
    await remember(chrome, url, elements);
    return listingText(
      elements.map(({ role, name, cut, goes, unknown }) => ({
        role,
        name,
        cut,
        destination: unknown ? "unknown" : destinationOf(goes),
      })),
      answer.left,
    );
  },

  // A picture of the tab at `url` as its window shows it, as a JPEG. The
  // browser's captureVisibleTab needs access to all sites, takes a window and
  // not a tab, and captures the tab that window shows, so the call is refused
  // under the scope of the allowed sites, for a URL that is not HTTP or HTTPS
  // or is at a blocked site, and where the tab at `url` is not the visible one,
  // none of which touches the browser's capture. The tab is not brought forward,
  // since that changes what the person sees. The tab is checked again before
  // and after the capture, and a picture of a tab that changed is not returned.
  // Frames of other origins inside the tab are in the picture, and the blocked
  // list names the tab's own URL only. The reply holds the picture and no
  // field about the page.
  async screenshot_page(chrome, params) {
    const { url, tab } = await pageToCapture(chrome, params, "screenshot_page");
    const data = await jpegOfTab(chrome, tab, url);
    return { image: { mimeType: "image/jpeg", data } };
  },

  // A picture of what the person chooses in the browser's own picker, as a
  // JPEG: a screen or a window of any program. The picker is shown for each
  // call, nothing is captured until the person chooses, and the call fails when
  // they close it or take too long. The reply holds the picture and no field
  // about the screen.
  async capture_screen(chrome) {
    const taken = await captureScreen(chrome);
    const data = pictureData(taken, {
      giver: "the capture window",
      what: "picture of the screen",
      advice: "choose a smaller screen or window and call again",
    });
    return { image: { mimeType: "image/jpeg", data } };
  },

  // An animated GIF of the tab at `url`: `frames` pictures of it taken as
  // screenshot_page takes one, at least CAPTURE_GAP_MS apart, each scaled down
  // and written by gif.js. The checks of screenshot_page apply to every frame,
  // and one that fails ends the recording.
  async record_gif(chrome, params) {
    const frames = wholeNumber(
      params,
      "frames",
      2,
      MAX_GIF_FRAMES,
      DEFAULT_GIF_FRAMES,
    );
    const { url, tab } = await pageToCapture(chrome, params, "record_gif");
    const bytes = await recordGif(frames, () => jpegOfTab(chrome, tab, url));
    return { image: { mimeType: "image/gif", data: toBase64(bytes) } };
  },

  // The page of the tab at `url`, saved as an MHTML file in the person's
  // Downloads folder. The browser's pageCapture makes the archive from the tab,
  // which it does at any site, so the allowed sites, which handle checks for
  // this tool whatever its group (ALLOWED_ONLY_TOOLS), and the blocked list are
  // what keep this to the sites the person chose, and both come before the tab
  // is looked for. The file goes through downloads.download as
  // download_file does: the name is the extension's own (archiveName), no dialog
  // asks where, and a file already there is never replaced. The reply names the
  // file and says how the download stands, and holds nothing of the page. The
  // tab is checked again after the archive is made, and an archive of a tab that
  // left the URL is not saved. Frames of other origins inside the tab are in the
  // archive, and the blocked list names the tab's own URL only.
  async save_page_archive(chrome, params) {
    const url = text(params, "url");
    if (!webUrl(url)) {
      throw new ToolError("url must be an HTTP or HTTPS URL", INVALID_PARAMS);
    }
    refuseBlocked(await blockedSites(chrome), url);
    const tab = await tabAt(chrome, url);
    let archive;
    try {
      archive = await chrome.pageCapture.saveAsMHTML({ tabId: tab.id });
    } catch (error) {
      throw new ToolError(
        `the archive of the tab at ${url} cannot be made: ${error?.message ?? error}`,
      );
    }
    const current = await chrome.tabs.get(tab.id).catch(() => undefined);
    if (current?.url !== url) {
      throw new ToolError(`the tab left ${url} while its archive was made`);
    }
    if (!(archive?.size > 0)) {
      throw new ToolError(`the browser gave no archive of the tab at ${url}`);
    }
    if (archive.size > MAX_ARCHIVE_BYTES) {
      throw new ToolError(
        `the archive of the tab at ${url} is over ${MAX_ARCHIVE_BYTES / 1024 / 1024} MiB, which is the most one may be, so it was not saved`,
      );
    }
    const id = await chrome.downloads.download({
      url: await dataUrlOf(archive),
      filename: archiveName(url),
      saveAs: false,
      conflictAction: "uniquify",
    });
    let [item] = await chrome.downloads.search({ id });
    for (
      let polls = 0;
      item?.state === "in_progress" && polls < ARCHIVE_POLLS;
      polls += 1
    ) {
      await new Promise((resolve) => setTimeout(resolve, ARCHIVE_POLL_MS));
      [item] = await chrome.downloads.search({ id });
    }
    if (item?.state === "interrupted") {
      throw new ToolError(
        `the browser could not save the archive of the tab at ${url}: ${item.error}`,
      );
    }
    return {
      url,
      filename: item?.filename ?? "",
      state: item?.state ?? null,
      bytes: archive.size,
    };
  },

  // Makes the tab at `url` the active tab of its window and brings the window
  // forward.
  async focus_tab(chrome, params) {
    const { url, tab } = await tabNamed(chrome, params);
    await chrome.tabs.update(tab.id, { active: true });
    await chrome.windows.update(tab.windowId, { focused: true });
    return { url };
  },

  async pin_tab(chrome, params) {
    const pinned = flag(params, "pinned");
    const { url, tab } = await tabNamed(chrome, params);
    const changed = await chrome.tabs.update(tab.id, { pinned });
    return { url, pinned: changed.pinned === true };
  },

  async mute_tab(chrome, params) {
    const muted = flag(params, "muted");
    const { url, tab } = await tabNamed(chrome, params);
    const changed = await chrome.tabs.update(tab.id, { muted });
    return { url, muted: changed.mutedInfo?.muted === true };
  },

  // Moves the tab within its window. The reply carries the position the
  // browser put it at, which differs from the one asked for where the browser
  // keeps pinned tabs first.
  async move_tab(chrome, params) {
    const index = params?.index;
    if (!Number.isInteger(index) || index < -1) {
      throw new ToolError(
        "index must be a whole number, 0 or more, or -1 for last",
        INVALID_PARAMS,
      );
    }
    const { url, tab } = await tabNamed(chrome, params);
    const moved = await chrome.tabs.move(tab.id, { index });
    return { url, index: moved.index };
  },

  // Puts the tabs at the listed URLs into one new group, in the window of the
  // first of them. Tabs in another window are not moved there: the call fails.
  // The reply carries the title and color the browser gave the group.
  async group_tabs(chrome, params) {
    const title = groupTitle(params);
    const { urls, tabs } = await tabsNamed(chrome, params);
    const windowId = tabs[0].windowId;
    if (tabs.some((tab) => tab.windowId !== windowId)) {
      throw new ToolError("the tabs are not all in one window");
    }
    const groupId = await chrome.tabs.group({
      tabIds: tabs.map((tab) => tab.id),
      createProperties: { windowId },
    });
    if (title !== undefined) {
      await chrome.tabGroups.update(groupId, { title });
    }
    const made = await chrome.tabGroups.get(groupId);
    return { urls, title: made.title ?? "", color: made.color };
  },

  // Takes the tabs at the listed URLs out of their groups. The reply says
  // whether the browser now holds every one of them in no group.
  async ungroup_tabs(chrome, params) {
    const { urls, tabs } = await tabsNamed(chrome, params);
    await chrome.tabs.ungroup(tabs.map((tab) => tab.id));
    const now = await Promise.all(tabs.map((tab) => chrome.tabs.get(tab.id)));
    return { urls, ungrouped: now.every((tab) => tab?.groupId === NO_GROUP) };
  },

  // Changes the group that the tab at `url` is in, which is how a group is
  // named: a group has no address, and its id says nothing a person can judge.
  // At least one of title, color and collapsed is given. The reply carries what
  // the browser holds for the group afterwards.
  async update_tab_group(chrome, params) {
    const title = groupTitle(params);
    const color = params?.color;
    if (color !== undefined && !GROUP_COLORS.includes(color)) {
      throw new ToolError(
        `color must be one of ${GROUP_COLORS.join(", ")}`,
        INVALID_PARAMS,
      );
    }
    const collapsed = optionalFlag(params, "collapsed");
    const changes = Object.fromEntries(
      Object.entries({ title, color, collapsed }).filter(
        ([, value]) => value !== undefined,
      ),
    );
    if (Object.keys(changes).length === 0) {
      throw new ToolError(
        "title, color or collapsed must be given",
        INVALID_PARAMS,
      );
    }
    const { url, tab } = await tabNamed(chrome, params);
    if (!Number.isInteger(tab.groupId) || tab.groupId === NO_GROUP) {
      throw new ToolError(`the tab at ${url} is in no group`);
    }
    await chrome.tabGroups.update(tab.groupId, changes);
    const group = await chrome.tabGroups.get(tab.groupId);
    return {
      url,
      title: group.title ?? "",
      color: group.color,
      collapsed: group.collapsed === true,
    };
  },

  // Unloads the tab from memory. It stays in the tab strip and loads again
  // when it is next shown. The reply says whether the browser discarded it,
  // which it does not for the tab being shown.
  async discard_tab(chrome, params) {
    const { url, tab } = await tabNamed(chrome, params);
    const discarded = await chrome.tabs.discard(tab.id);
    return { url, discarded: discarded?.discarded === true };
  },

  // Opens a copy of the tab, which loads the same address again. The reply
  // says whether the browser made one and carries nothing of it.
  async duplicate_tab(chrome, params) {
    const { url, tab } = await tabNamed(chrome, params);
    const copy = await chrome.tabs.duplicate(tab.id);
    return { url, duplicated: copy?.id !== undefined };
  },

  // Sets the zoom factor of the tab, 1 being 100%, or the browser's default for
  // the page where it is 0. The browser keeps a zoom per site unless a tab was
  // given its own, so other tabs at the site may follow. The reply carries the
  // factor the browser reports afterwards.
  async set_zoom(chrome, params) {
    const factor = params?.factor;
    if (
      typeof factor !== "number" ||
      !Number.isFinite(factor) ||
      factor < 0 ||
      factor > MAX_ZOOM
    ) {
      throw new ToolError(
        `factor must be a number from 0 to ${MAX_ZOOM}, where 1 is 100% and 0 is the default`,
        INVALID_PARAMS,
      );
    }
    const { url, tab } = await tabNamed(chrome, params);
    await chrome.tabs.setZoom(tab.id, factor);
    return { url, factor: await chrome.tabs.getZoom(tab.id) };
  },

  // Brings the window that holds the tab at `url` forward and leaves the tabs
  // of that window as they are. The reply says whether the window is focused.
  async focus_window(chrome, params) {
    const { url, tab } = await tabNamed(chrome, params);
    const focused = await chrome.windows.update(tab.windowId, {
      focused: true,
    });
    return { url, focused: focused?.focused === true };
  },

  // Sets the size of the window that holds the tab at `url`, in pixels, and
  // leaves its position and tabs as they are. The browser keeps a window
  // between the sizes it allows and may keep a maximized or full screen window
  // as it is, so the reply carries the size the browser reports afterwards.
  async resize_window(chrome, params) {
    const width = windowSize(params, "width");
    const height = windowSize(params, "height");
    const { url, tab } = await tabNamed(chrome, params);
    const resized = await chrome.windows.update(tab.windowId, {
      width,
      height,
    });
    return {
      url,
      width: resized?.width ?? null,
      height: resized?.height ?? null,
    };
  },

  // The cookies the browser sends to the HTTP or HTTPS `url`: each one's name,
  // domain, path, flags and expiry. The browser returns every cookie with its
  // value, and the result is built from the named fields alone, so a value
  // never leaves the extension (BROWSER-30). A cookie the browser also sends to a
  // blocked site's host is left out, and a URL at that host is refused whatever
  // its scheme or port.
  async list_cookie_names(chrome, params) {
    const url = webAddress(params);
    const sites = await blockedSites(chrome);
    if (atBlockedHost(sites, url)) {
      throw new Refusal(BLOCKED_MESSAGE);
    }
    const found = await chrome.cookies.getAll({ url });
    return {
      url,
      cookies: found
        .filter((cookie) => !cookieAtBlockedHost(sites, cookie))
        .map((cookie) => ({
          name: cookie.name,
          domain: cookie.domain,
          path: cookie.path,
          secure: cookie.secure === true,
          http_only: cookie.httpOnly === true,
          host_only: cookie.hostOnly === true,
          session: cookie.session === true,
          same_site: cookie.sameSite ?? null,
          expires: isoTime(cookie.expirationDate * 1000),
        })),
    };
  },

  // Deletes the cookie called `name` that the browser sends to the HTTP or HTTPS
  // `url`. The browser's remove deletes every cookie of that name sent to the
  // URL, so the call fails and deletes nothing where more than one is, or none,
  // and where one of them is also sent to a blocked site's host. It reads no
  // cookie value and sets none (BROWSER-30). The reply names the domain and path
  // of the cookie deleted.
  async delete_cookie(chrome, params) {
    const name = text(params, "name");
    const url = webAddress(params);
    const sites = await blockedSites(chrome);
    if (atBlockedHost(sites, url)) {
      throw new Refusal(BLOCKED_MESSAGE);
    }
    const found = await chrome.cookies.getAll({ url, name });
    if (found.some((cookie) => cookieAtBlockedHost(sites, cookie))) {
      throw new Refusal(BLOCKED_MESSAGE);
    }
    if (found.length === 0) {
      throw new ToolError(`no cookie named ${name} is sent to ${url}`);
    }
    if (found.length > 1) {
      throw new ToolError(
        `${found.length} cookies named ${name} are sent to ${url}, so the name and URL do not name one cookie`,
      );
    }
    const [{ domain, path }] = found;
    if (!(await chrome.cookies.remove({ url, name }))) {
      throw new ToolError(
        `the browser did not delete the cookie named ${name} at ${url}`,
      );
    }
    return { url, name, domain, path };
  },

  // The requests the tab at this URL has made since it loaded, with the URL,
  // type and outcome of each and nothing else: no header, cookie or body is
  // ever asked of the browser (requests.js). The tab is the one whose URL is
  // exactly the one asked for. A request made by a frame at a blocked site or
  // inside one, and a request to one, is left out.
  //
  // The requests are kept in the service worker's memory, while this tool and
  // its group are on. A page that loaded before the worker started or read the
  // switches, before the group and this tool were turned on, or while the
  // extension did not hold webRequest, has none kept, and the call says so and
  // returns nothing.
  async list_requests(chrome, params) {
    const url = text(params, "url");
    const sites = await blockedSites(chrome);
    refuseBlocked(sites, url);
    const tab = await tabAt(chrome, url);
    if (tab.pendingUrl !== undefined && tab.pendingUrl !== url) {
      throw new ToolError(`the tab at ${url} is loading another page`);
    }
    const load = requestsOf(chrome, tab.id);
    if (load === undefined) {
      throw new ToolError(
        `the extension has seen no requests of the tab at ${url} since it loaded, because it was not watching then or has restarted since; reload the page and ask again`,
      );
    }
    const current = await chrome.tabs.get(tab.id);
    if (current?.url !== url) {
      throw new ToolError(`the tab left ${url} before its requests were read`);
    }
    return {
      url,
      requests: load.requests
        .filter(
          (request) =>
            !isBlocked(sites, request.url) &&
            !behindBlocked(sites, load.frames, {
              frameId: request.frameId,
              parentFrameId: request.parentFrameId,
              url: request.frameUrl,
            }),
        )
        .map(({ url: address, type, status, error }) => ({
          url: address,
          type,
          status,
          error,
        })),
      truncated: load.more,
    };
  },

  // The browser's privacy settings and who controls each: this extension, other
  // extensions or the browser. Nothing is changed. A setting the browser will
  // not report comes back with its error.
  async get_privacy_settings(chrome) {
    const reads = PRIVACY_SETTINGS.filter(
      ([area, name]) => chrome.privacy?.[area]?.[name],
    ).map(([area, name]) => [
      `${area}.${name}`,
      async () => {
        const got = await chrome.privacy[area][name].get({});
        return { value: got.value, control: got.levelOfControl };
      },
    ]);
    return { settings: await readEach(reads) };
  },

  // What the browser does with each kind of content at one URL: allow it, block
  // it or ask. The URL is the one a person read in the question, and one at a
  // blocked site is refused before the browser is asked. The regular profile is
  // read, and no second URL is given, so a kind that depends on the page that
  // embeds the content is answered for the URL alone.
  async get_content_settings(chrome, params) {
    const url = text(params, "url");
    if (!webUrl(url)) {
      throw new ToolError("url must be an HTTP or HTTPS URL", INVALID_PARAMS);
    }
    const sites = await blockedSites(chrome);
    refuseBlocked(sites, url);
    const reads = CONTENT_TYPES.filter(
      (type) => chrome.contentSettings?.[type],
    ).map((type) => [
      type,
      async () => ({
        value: (await chrome.contentSettings[type].get({ primaryUrl: url }))
          .setting,
      }),
    ]);
    return { url, settings: await readEach(reads) };
  },

  // The font for each generic family, for the script every page falls back to,
  // and the three font sizes. The fonts installed are not listed.
  async get_font_settings(chrome) {
    const fonts = FONT_FAMILIES.map((family) => [
      `font.${family}`,
      async () => {
        const got = await chrome.fontSettings.getFont({ genericFamily: family });
        return { value: got.fontId, control: got.levelOfControl };
      },
    ]);
    const sizes = [
      ["default", "getDefaultFontSize"],
      ["fixed", "getDefaultFixedFontSize"],
      ["minimum", "getMinimumFontSize"],
    ].map(([name, method]) => [
      `size.${name}`,
      async () => {
        const got = await chrome.fontSettings[method]();
        return { value: got.pixelSize, control: got.levelOfControl };
      },
    ]);
    return { settings: await readEach([...fonts, ...sizes]) };
  },

  // The processor, memory, displays and storage devices of the computer. Only
  // these fields are passed on, whatever else the browser adds. How busy each
  // processor is and which storage device is which are not given, and nothing
  // is changed or ejected.
  async get_system_info(chrome) {
    const [cpu, memory, displays, storage] = await Promise.all([
      chrome.system.cpu.getInfo(),
      chrome.system.memory.getInfo(),
      chrome.system.display.getInfo(),
      chrome.system.storage.getInfo(),
    ]);
    return {
      cpu: {
        processors: cpu.numOfProcessors,
        arch: cpu.archName,
        model: cpu.modelName,
        features: cpu.features ?? [],
      },
      memory: { capacity: memory.capacity, available: memory.availableCapacity },
      displays: displays.map((display) => ({
        name: display.name,
        primary: display.isPrimary,
        bounds: display.bounds,
        work_area: display.workArea,
        dpi_x: display.dpiX,
        dpi_y: display.dpiY,
        rotation: display.rotation,
        touch: display.hasTouchSupport,
      })),
      storage: storage.map((unit) => ({
        name: unit.name,
        type: unit.type,
        capacity: unit.capacity,
      })),
    };
  },

  // Whether the person is at the computer: active, idle after the seconds asked
  // for without input (60 where none are given), or locked.
  async get_idle_state(chrome, params) {
    const asked = params?.seconds;
    if (asked !== undefined && !(Number.isInteger(asked) && asked > 0)) {
      throw new ToolError(
        "seconds must be a whole number above 0",
        INVALID_PARAMS,
      );
    }
    const seconds = asked ?? IDLE_SECONDS;
    return { state: await chrome.idle.queryState(seconds), seconds };
  },

  // Whether the person allowed the extension on file: URLs and in private
  // windows, in its details page in Brave. No permission is needed to ask.
  async get_access_flags(chrome) {
    const [files, incognito] = await Promise.all([
      chrome.extension.isAllowedFileSchemeAccess(),
      chrome.extension.isAllowedIncognitoAccess(),
    ]);
    return { file_urls: files === true, incognito: incognito === true };
  },

  // Keeps the computer from sleeping for the minutes asked for, at the level
  // asked for: the system stays awake, or the display stays on as well (the
  // system where none is given). It replaces a request made before. It ends when
  // the minutes have passed, which is at most MAX_AWAKE_MINUTES, or at once with
  // 0 minutes, and awake.js ends it at other times. Turning the group or the tool
  // off releases what stands at that moment, and this call may not have made its
  // request yet, so both switches are read again after it and the request is
  // released where either is off or cannot be read.
  async keep_awake(chrome, params) {
    const minutes = params?.minutes;
    if (
      !Number.isInteger(minutes) ||
      minutes < 0 ||
      minutes > MAX_AWAKE_MINUTES
    ) {
      throw new ToolError(
        `minutes must be a whole number from 0 to ${MAX_AWAKE_MINUTES}, where 0 releases the request`,
        INVALID_PARAMS,
      );
    }
    const level = params?.level === undefined ? "system" : params.level;
    if (!LEVELS.includes(level)) {
      throw new ToolError(
        `level must be one of ${LEVELS.join(", ")}`,
        INVALID_PARAMS,
      );
    }
    if (minutes === 0) {
      await releaseAwake(chrome);
      return { released: true };
    }
    const kept = await keepAwake(chrome, level, minutes);
    const group = groupOf("keep_awake");
    let groupOn = false;
    let toolOn = false;
    try {
      groupOn = (await groupSwitches(chrome))[group] === true;
      toolOn = (await settings(chrome)).keep_awake === true;
    } finally {
      if (!groupOn || !toolOn) {
        await releaseAwake(chrome);
      }
    }
    if (!groupOn || !toolOn) {
      throw new Refusal(
        `keep_awake is refused: the ${groupOn ? "tool" : `${group} group`} was turned off while the request was being made, and the request is released`,
      );
    }
    return { level, minutes, until: isoTime(kept.until) };
  },

  // Adds a bookmark at an HTTP or HTTPS URL, in the folder with exactly the
  // title given or, where none is given, where the browser puts a new
  // bookmark. A URL that already has a bookmark gets another, as it does in the
  // browser. The reply carries the title and folder the browser holds for it.
  async create_bookmark(chrome, params) {
    const url = webAddress(params);
    const title = optionalText(params, "title");
    const inside = optionalText(params, "folder");
    refuseBlocked(await blockedSites(chrome), url);
    const folder =
      inside === undefined ? undefined : await bookmarkFolder(chrome, inside);
    const made = await chrome.bookmarks.create({
      url,
      ...(title === undefined ? {} : { title }),
      ...(folder === undefined ? {} : { parentId: folder.id }),
    });
    return {
      url,
      title: made.title ?? "",
      folder: await folderTitled(chrome, made.parentId),
    };
  },

  // Adds a folder, inside the folder with exactly the title given as `parent`
  // or, where none is given, where the browser puts a new folder. A title that
  // is already a folder's is refused and nothing is made: a folder is named by
  // its title, so a second would leave the title naming neither, and a later
  // move to it would fail. The reply carries the title and parent the browser
  // holds for it.
  async create_bookmark_folder(chrome, params) {
    const title = text(params, "title");
    const inside = optionalText(params, "parent");
    const parent =
      inside === undefined ? undefined : await bookmarkFolder(chrome, inside);
    if ((await foldersTitled(chrome, title)).length > 0) {
      throw new ToolError(
        `a bookmark folder is already titled ${title}, and a second would leave the title naming neither`,
      );
    }
    const made = await chrome.bookmarks.create({
      title,
      ...(parent === undefined ? {} : { parentId: parent.id }),
    });
    return {
      title: made.title ?? "",
      parent: await folderTitled(chrome, made.parentId),
    };
  },

  // Moves the bookmark at `url` to the end of the folder with exactly the title
  // given as `folder`. Both are found before anything is moved, so a URL of two
  // bookmarks or a title of two folders moves nothing. The reply carries the
  // folder the browser says the bookmark is in.
  async move_bookmark(chrome, params) {
    const url = text(params, "url");
    const title = text(params, "folder");
    refuseBlocked(await blockedSites(chrome), url);
    const bookmark = await bookmarkAt(chrome, url);
    const folder = await bookmarkFolder(chrome, title);
    // The folder it leaves, so a person reading the reply can put it back.
    const from =
      bookmark.parentId === undefined
        ? ""
        : await folderTitled(chrome, bookmark.parentId).catch(() => "");
    const moved = await chrome.bookmarks.move(bookmark.id, {
      parentId: folder.id,
    });
    return { url, from, folder: await folderTitled(chrome, moved.parentId) };
  },

  // Changes the title of the bookmark at `url`. The reply carries the title it
  // replaced, so a person reading it can give that title back, and the title the
  // browser holds.
  async rename_bookmark(chrome, params) {
    const url = text(params, "url");
    const title = text(params, "title");
    refuseBlocked(await blockedSites(chrome), url);
    const bookmark = await bookmarkAt(chrome, url);
    const replaced = bookmark.title ?? "";
    const renamed = await chrome.bookmarks.update(bookmark.id, { title });
    return { url, replaced, title: renamed.title ?? "" };
  },

  // Adds an entry for an HTTP or HTTPS URL to the reading list, unread unless
  // `read` says otherwise. A URL the list already has is refused, since the list
  // holds one entry for an address. The reply carries the entry as the browser
  // holds it.
  async add_reading_list_entry(chrome, params) {
    const url = webAddress(params);
    const title = text(params, "title");
    const read = optionalFlag(params, "read") ?? false;
    refuseBlocked(await blockedSites(chrome), url);
    if ((await chrome.readingList.query({ url })).length > 0) {
      throw new ToolError(`the reading list already has an entry at ${url}`);
    }
    await chrome.readingList.addEntry({ url, title, hasBeenRead: read });
    const added = await entryAt(chrome, url);
    return { url, title: added.title ?? "", read: added.hasBeenRead === true };
  },

  // Changes the title or whether it was read of the reading list entry at
  // `url`, with only what the call gives. The reply carries the entry as the
  // browser holds it.
  async update_reading_list_entry(chrome, params) {
    const url = text(params, "url");
    const title = optionalText(params, "title");
    const read = optionalFlag(params, "read");
    if (title === undefined && read === undefined) {
      throw new ToolError("title or read must be given", INVALID_PARAMS);
    }
    refuseBlocked(await blockedSites(chrome), url);
    await entryAt(chrome, url);
    await chrome.readingList.updateEntry({
      url,
      ...(title === undefined ? {} : { title }),
      ...(read === undefined ? {} : { hasBeenRead: read }),
    });
    const updated = await entryAt(chrome, url);
    return {
      url,
      title: updated.title ?? "",
      read: updated.hasBeenRead === true,
    };
  },

  // Closes the tab at `url`, which is what a person read in the question. The
  // page's unsaved state is lost. The reply says whether the browser no longer
  // has the tab, which it may still have if the page asked to stay open.
  async close_tab(chrome, params) {
    const { url, tab } = await tabNamed(chrome, params);
    await chrome.tabs.remove(tab.id);
    const left = await chrome.tabs.get(tab.id).catch(() => undefined);
    return { url, closed: left === undefined };
  },

  // Closes the window that holds the tab at `url`, with every tab in it. A
  // window with a tab at a blocked site is not closed, since that site is
  // never touched. The reply says how many tabs the window held and whether the
  // browser no longer has it.
  async close_window(chrome, params) {
    const url = text(params, "url");
    const sites = await blockedSites(chrome);
    refuseBlocked(sites, url);
    const tab = await tabAt(chrome, url);
    const inWindow = (await chrome.tabs.query({})).filter(
      (other) => other.windowId === tab.windowId,
    );
    if (
      inWindow.some(
        (other) =>
          isBlocked(sites, other.url) || isBlocked(sites, other.pendingUrl),
      )
    ) {
      throw new Refusal(WINDOW_BLOCKED_MESSAGE);
    }
    await chrome.windows.remove(tab.windowId);
    const left = await chrome.windows.get(tab.windowId).catch(() => undefined);
    return { url, tabs: inWindow.length, closed: left === undefined };
  },

  // Deletes the history entry at exactly `url`, with every visit to it. The
  // browser keeps one entry for a URL, so the URL names one entry or none. The
  // reply says how many visits there were and whether none is left.
  async delete_history_entry(chrome, params) {
    const url = text(params, "url");
    const sites = await blockedSites(chrome);
    refuseBlocked(sites, url);
    const visits = await chrome.history.getVisits({ url });
    if (visits.length === 0) {
      throw new ToolError(`no history entry is at ${url}`);
    }
    await chrome.history.deleteUrl({ url });
    const left = await chrome.history.getVisits({ url });
    return { url, visits: visits.length, deleted: left.length === 0 };
  },

  // Deletes the history between two moments, both given. The browser removes a
  // page only where every visit to it is inside the range. The call names no
  // site, so no entry is left out for the blocked list, but a list that cannot
  // be read stops the call as it stops every tool that reaches a site.
  async delete_history_range(chrome, params) {
    const start = instant(params, "start");
    const end = instant(params, "end");
    if (start >= end) {
      throw new ToolError("start must be before end", INVALID_PARAMS);
    }
    await blockedSites(chrome);
    await chrome.history.deleteRange({ startTime: start, endTime: end });
    return { start: isoTime(start), end: isoTime(end) };
  },

  // Clears the kinds of browsing data named that built up since the moment
  // named, on every site. Both are required: a call that names no kind, or no
  // start, clears nothing. The call names no site, so no site is left out for
  // the blocked list, but a list that cannot be read stops the call as it stops
  // every tool that reaches a site.
  async clear_browsing_data(chrome, params) {
    const since = instant(params, "since");
    const named = params?.types;
    if (
      !Array.isArray(named) ||
      named.length === 0 ||
      named.some((type) => !BROWSING_DATA_TYPES.includes(type))
    ) {
      throw new ToolError(
        `types must be a list of one or more of ${BROWSING_DATA_TYPES.join(", ")}`,
        INVALID_PARAMS,
      );
    }
    const types = [...new Set(named)];
    await blockedSites(chrome);
    await chrome.browsingData.remove(
      { since },
      Object.fromEntries(types.map((type) => [type, true])),
    );
    return { since: isoTime(since), types };
  },

  // Deletes the bookmark at exactly `url`. Two bookmarks at one URL are two
  // the question cannot tell apart, so the call fails and says how many there
  // are. A folder has no URL and is not found. The reply says whether the
  // bookmark is gone.
  async remove_bookmark(chrome, params) {
    const url = text(params, "url");
    const sites = await blockedSites(chrome);
    refuseBlocked(sites, url);
    const at = async () =>
      (await chrome.bookmarks.search({ url })).filter(
        (node) => node.url === url,
      );
    const found = await at();
    if (found.length === 0) {
      throw new ToolError(`no bookmark is at ${url}`);
    }
    if (found.length > 1) {
      throw new ToolError(
        `${found.length} bookmarks are at ${url}, so the URL does not name one bookmark`,
      );
    }
    await chrome.bookmarks.remove(found[0].id);
    return { url, removed: (await at()).length === 0 };
  },

  // Deletes the bookmark folder titled exactly `folder`, and only where it is
  // empty: the browser's remove deletes an empty folder, and removeTree, which
  // would delete what is in it, is not called. A folder with anything in it is
  // left as it is and the failure says how many entries it holds, so the
  // bookmarks in it are deleted by name, one call each, or kept. Two folders
  // with one title are two the question cannot tell apart, so the call fails
  // and says how many there are. The reply says whether the folder is gone.
  async remove_bookmark_folder(chrome, params) {
    const title = text(params, "folder");
    await blockedSites(chrome);
    const at = async () =>
      (await chrome.bookmarks.search({ title })).filter(
        (node) => typeof node.url !== "string" && node.title === title,
      );
    const found = await at();
    if (found.length === 0) {
      throw new ToolError(`no bookmark folder is titled ${title}`);
    }
    if (found.length > 1) {
      throw new ToolError(
        `${found.length} bookmark folders are titled ${title}, so the title does not name one folder`,
      );
    }
    const [folder] = found;
    const held = (await chrome.bookmarks.getChildren(folder.id)).length;
    if (held > 0) {
      throw new ToolError(
        `the bookmark folder titled ${title} has ${held} ${held === 1 ? "entry" : "entries"} in it, so it is not removed`,
      );
    }
    await chrome.bookmarks.remove(folder.id);
    return {
      folder: title,
      removed: !(await at()).some((node) => node.id === folder.id),
    };
  },

  // Removes the reading list entry at exactly `url`. The browser keeps one
  // entry for an address, so the URL names one entry or none. The reply says
  // whether the entry is gone.
  async remove_reading_list_entry(chrome, params) {
    const url = text(params, "url");
    const sites = await blockedSites(chrome);
    refuseBlocked(sites, url);
    const at = async () =>
      (await chrome.readingList.query({ url })).filter(
        (entry) => entry.url === url,
      );
    if ((await at()).length === 0) {
      throw new ToolError(`no reading list entry is at ${url}`);
    }
    await chrome.readingList.removeEntry({ url });
    return { url, removed: (await at()).length === 0 };
  },

  // Deletes the file of a finished download from the disk, with
  // downloads.removeFile. The download is named by `filename`, the absolute
  // path the browser holds for it, or by `url`, the address it started from,
  // one of the two and exactly as the browser holds it. The history entry of the
  // download stays, as the browser keeps it with the file gone, since
  // downloads.erase, which removes the entry, is not called. A name that
  // several downloads with a file on the disk have names none of them, and the
  // failure says how many. A download whose file the browser says is gone is
  // not asked to delete it again, and one that is not finished has no file to
  // delete. A download that came from a blocked site is refused. The reply says
  // whether the browser now reports the file gone.
  async delete_downloaded_file(chrome, params) {
    const { by, value, matching, sites } = await downloadsNamed(chrome, params);
    const present = matching.filter((item) => item.exists !== false);
    if (present.length === 0) {
      throw new ToolError(
        `the file for the ${by} ${value} is already gone, so none is deleted`,
      );
    }
    if (present.length > 1) {
      throw new ToolError(
        `${present.length} downloads with a file on the disk have the ${by} ${value}, so it does not name one download`,
      );
    }
    const [item] = present;
    if (fromBlocked(sites, item)) {
      throw new Refusal(BLOCKED_MESSAGE);
    }
    if (item.state !== "complete") {
      throw new ToolError(
        `the download with the ${by} ${value} is ${item.state}, so it has no finished file to delete`,
      );
    }
    await chrome.downloads.removeFile(item.id);
    const [now] = await chrome.downloads.search({ id: item.id });
    return {
      filename: item.filename,
      url: item.url,
      deleted: now?.exists === false,
    };
  },

  // Starts a download of `url` with downloads.download, which is a request to
  // that site with the person's cookies, so the URL is the one they read in the
  // question. Only an HTTP or HTTPS URL is taken, and a site on the blocked list
  // is refused before the browser is asked. The file goes to the person's
  // Downloads folder under the name the browser gives it: no folder or name is
  // an argument, no dialog asks where, and a file already there is never
  // replaced. If the download turns out to come from another host than the one
  // asked for, or from a blocked site, after its redirects, it is cancelled
  // and a file that had already finished is deleted, as open_tab closes a tab
  // that goes there. The reply says where the browser put the file and how far
  // the download is, and holds nothing of the file.
  async download_file(chrome, params) {
    const url = text(params, "url");
    if (!webUrl(url)) {
      throw new ToolError("url must be an HTTP or HTTPS URL", INVALID_PARAMS);
    }
    const sites = await blockedSites(chrome);
    refuseBlocked(sites, url);
    const id = await chrome.downloads.download({
      url,
      saveAs: false,
      conflictAction: "uniquify",
    });
    const [item] = await chrome.downloads.search({ id });
    const landed = item?.finalUrl || item?.url || url;
    const stop = async () => {
      await chrome.downloads.cancel(id).catch(() => {});
      const [now] = await chrome.downloads.search({ id });
      if (now?.state === "complete") {
        await chrome.downloads.removeFile(id).catch(() => {});
      }
    };
    if (!webUrl(landed) || new URL(landed).host !== new URL(url).host) {
      await stop();
      throw new ToolError(
        `the download of ${url} went to another host, so it was cancelled`,
      );
    }
    if (isBlocked(sites, landed)) {
      await stop();
      throw new Refusal(BLOCKED_MESSAGE);
    }
    return { url, filename: item?.filename ?? "", state: item?.state ?? null };
  },

  // The downloads the browser lists, newest first, with the path and the
  // address each can be named by in the tools that act on a download. A
  // download that started at a blocked site, or ended at one, is left out. While
  // a site is blocked the browser is asked for more than the count, as a history
  // search is, so that leaving them out does not return fewer than there are.
  async list_downloads(chrome, params) {
    const sites = await blockedSites(chrome);
    const wanted = count(params);
    const items = await chrome.downloads.search({
      orderBy: ["-startTime"],
      limit: sites.length === 0 ? wanted : BLOCKED_SCAN_LIMIT,
    });
    return items
      .filter((item) => !fromBlocked(sites, item))
      .slice(0, wanted)
      .map((item) => ({
        filename: item.filename ?? "",
        url: item.url ?? "",
        state: item.state,
        paused: item.paused === true,
        exists: item.exists !== false,
        bytes_received: item.bytesReceived ?? null,
        total_bytes: item.totalBytes >= 0 ? item.totalBytes : null,
        started: typeof item.startTime === "string" ? item.startTime : null,
      }));
  },

  // Pauses the download in progress with this filename or URL. See
  // downloadsNamed for the names, and downloadFor for which download is meant.
  async pause_download(chrome, params) {
    return changeDownload(chrome, params, DOWNLOAD_ACTIONS.pause);
  },

  // Resumes the paused or interrupted download with this filename or URL that
  // the browser says can be resumed.
  async resume_download(chrome, params) {
    return changeDownload(chrome, params, DOWNLOAD_ACTIONS.resume);
  },

  // Cancels the download in progress with this filename or URL, paused or not.
  // A finished download has nothing to cancel, and its file is not deleted.
  async cancel_download(chrome, params) {
    return changeDownload(chrome, params, DOWNLOAD_ACTIONS.cancel);
  },

  // Shows the file of the download with this filename or URL in its folder in
  // the computer's file manager, with downloads.show. The file is not opened,
  // and the browser does not say whether the folder opened.
  async show_download(chrome, params) {
    const item = await downloadFor(chrome, params, DOWNLOAD_ACTIONS.show);
    await DOWNLOAD_ACTIONS.show.run(chrome, item.id);
    return { filename: item.filename, url: item.url };
  },

  // The scripts group. Runs the code in the top document of the tab at the exact
  // URL, in a user script world whose policy is default-src 'none', after the
  // person has read all of the code in a window of this extension and pressed
  // Allow once. The code is the planner's own argument. What it answers comes back
  // as text, cut at RESULT_LIMIT, and a script that threw answers null. Nothing
  // the code does is limited except by the person having read it (runScript).
  async run_script(chrome, params) {
    return runScript(chrome, params);
  },

  // The input group. Each of these names one tab by its exact URL, and an
  // element in the top document of it by a role from the list and an accessible
  // name, or, for the tools that act on an element, by the number the element has
  // in the last list_page_elements. The page decides which element a name leads
  // to, so the extension finds it, shows it to the person in the browser, and
  // does the action only if they allow it and the page still shows the same
  // element (resolveElement and performAction). A number leads to the element it
  // was given to, and to no other. The reply holds a fixed word and the origin
  // of the page, and nothing of the element or the page.

  // Clicks the element with the role and name, or the element with the number,
  // with the left button once unless the call says otherwise. A right click sends
  // the context menu event.
  async click_element(chrome, params) {
    const target = elementNamed(params, true);
    const button = oneOf(params, "button", BUTTONS, "left");
    const count = wholeNumber(params, "count", 1, 2, 1);
    if (button === "right" && count !== 1) {
      throw new ToolError("a right click is made once", INVALID_PARAMS);
    }
    const action =
      button === "right" ? "right_click" : count === 2 ? "double_click" : "click";
    return actOnElement(chrome, params, { action, target });
  },

  // Puts the text in the field with the role and name, in place of what it
  // holds. The text is given to the field as text. A password field, a payment
  // card field and a file chooser are never acted on.
  async type_text(chrome, params) {
    const target = elementNamed(params, true);
    const typed = shortText(params, "text", TYPED_TEXT_LIMIT);
    return actOnElement(chrome, params, { action: "type", target, typed });
  },

  // Picks the option with this text in the native select with the role and name.
  async select_option(chrome, params) {
    const target = elementNamed(params, true);
    const typed = shortText(params, "option", NAME_LIMIT);
    return actOnElement(chrome, params, { action: "select", target, typed });
  },

  // Sends one key from the list to the element that has the focus in the page.
  async press_key(chrome, params) {
    const typed = oneOf(params, "key", KEYS);
    return actOnElement(chrome, params, { action: "key", target: null, typed });
  },

  // Scrolls the page by whole pages, or to its top or bottom. No element is
  // acted on, so the person is not asked in the browser.
  async scroll_page(chrome, params) {
    const direction = oneOf(params, "direction", DIRECTIONS);
    const pages = wholeNumber(params, "pages", 1, MAX_PAGES, 1);
    const { url, tab } = await pageNamed(chrome, params);
    const done = await inTab(chrome, tab.id, scrollInPage, {
      url,
      direction,
      pages,
    });
    if (done.status !== "done") {
      throw pageFailure(done);
    }
    return { status: "scrolled", origin: originOf(url) };
  },

  // Waits until the page shows exactly one element with the role and name, for
  // at most the seconds asked. It answers that one was found, or fails; it
  // acts on nothing and reads nothing of the element.
  async wait_for_element(chrome, params) {
    const target = elementNamed(params);
    const seconds = wholeNumber(
      params,
      "seconds",
      1,
      MAX_WAIT_SECONDS,
      DEFAULT_WAIT_SECONDS,
    );
    const { url, tab } = await pageNamed(chrome, params);
    const request = { url, ...target, step: "find", limit: SEEN_LIMITS };
    let timer;
    let poll;
    const timedOut = new Promise((resolve) => {
      timer = setTimeout(resolve, seconds * 1000, true);
    });
    try {
      for (;;) {
        const seen = await inTab(chrome, tab.id, inPage, request);
        if (seen.status === "ok") {
          return { status: "found", origin: originOf(url) };
        }
        if (!["none", "hidden", "several"].includes(seen.status)) {
          throw pageFailure(seen);
        }
        const pause = new Promise((resolve) => {
          poll = setTimeout(resolve, WAIT_POLL_MS, false);
        });
        if (await Promise.race([pause, timedOut])) {
          throw new ToolError(
            `no single element with that role and that name appeared within ${seconds} seconds: ${failureText(seen)}`,
          );
        }
      }
    } finally {
      clearTimeout(timer);
      clearTimeout(poll);
    }
  },

  // The input-trusted group. Each of these names one tab by its exact URL, and
  // an element in the top document of it by a role from the list and an
  // accessible name, as the input group does, and is done by the BraveBot
  // Trusted Input extension with real mouse and keyboard input once the person
  // allows it in a window of that extension. Nothing is sent to it for a site
  // that is not allowed, or is blocked. The reply holds a fixed word and the
  // origin of the page, and nothing of the element or the page.

  // Clicks the element with the role and name, with the left button once unless
  // the call says otherwise.
  async click_element_trusted(chrome, params) {
    const target = elementNamed(params);
    const button = oneOf(params, "button", BUTTONS, "left");
    const count = wholeNumber(params, "count", 1, 2, 1);
    if (button === "right" && count !== 1) {
      throw new ToolError("a right click is made once", INVALID_PARAMS);
    }
    return actTrusted(chrome, "click_element_trusted", params, {
      ...target,
      button,
      count,
    });
  },

  // Puts the text in the field with the role and name, in place of what it
  // holds.
  async type_text_trusted(chrome, params) {
    const target = elementNamed(params);
    const typed = shortText(params, "text", TYPED_TEXT_LIMIT);
    return actTrusted(chrome, "type_text_trusted", params, {
      ...target,
      text: typed,
    });
  },

  // Sends one key from the list to the element that has the focus.
  async press_key_trusted(chrome, params) {
    const key = oneOf(params, "key", KEYS);
    return actTrusted(chrome, "press_key_trusted", params, { key });
  },

  // Moves the pointer over the element with the role and name.
  async hover_element(chrome, params) {
    const target = elementNamed(params);
    return actTrusted(chrome, "hover_element", params, target);
  },

  // Drags the element with the role and name onto the element with the second
  // role and name.
  async drag_element(chrome, params) {
    const target = elementNamed(params);
    const onto = {
      onto_role: oneOf(params, "onto_role", ROLES),
      onto_name: shortText(params, "onto_name", NAME_LIMIT),
    };
    return actTrusted(chrome, "drag_element", params, { ...target, ...onto });
  },

  // The capture-trusted group. It names one tab by its exact URL and has the
  // BraveBot Trusted Input extension take a picture of the whole top document of
  // it, once the person allows it in a window of that extension. Nothing is sent
  // for a site that is not allowed, or is blocked. The reply is the picture as
  // {image: {mimeType, data}} and nothing else: a JPEG of at most 2048 pixels on
  // its long side and 8 MiB, which the MCP client takes as a picture the model
  // does not see. Frames are in it as the page draws them.
  async screenshot_full_page(chrome, params) {
    return actTrusted(chrome, "screenshot_full_page", params, {});
  },

  // The console-trusted group. It names one tab by its exact URL and has the
  // BraveBot Trusted Input extension read the console of it: what the page's
  // scripts wrote with console.log and the others, the errors they threw and what
  // the browser reported about the page, as many of the newest entries as asked
  // for, of the levels asked for, once the person allows it in a window of that
  // extension. Nothing is sent for a site that is not allowed, or is blocked. The
  // reply is the text of the console, one JSON object on each line: the page's own
  // words, which reach a session as a result it cannot read directly.
  async read_console(chrome, params) {
    const levels = oneOf(params, "levels", CONSOLE_LEVELS, DEFAULT_LEVELS);
    const entries = wholeNumber(params, "max_entries", 1, MAX_ENTRIES, DEFAULT_ENTRIES);
    return actTrusted(chrome, "read_console", params, { levels, max_entries: entries });
  },

  // The upload-trusted group. It names one tab by its exact URL, one file input in
  // the top document of it by its name (the accessible name, or the name the page's
  // markup gives an input it hides) and, where the call gives one, its role, and a
  // file by the path the relay's server resolved: the file's canonical path, its
  // size and its name (files.js), none of them read here. The BraveBot Trusted
  // Input extension has the browser read the file and give it to the input once the
  // person allows it in a window of that extension, which shows the input and the
  // file. Nothing is sent for a site that is not allowed, or is blocked. The reply
  // holds a fixed word and the origin of the page.
  async choose_file(chrome, params) {
    const name = shortText(params, "name", NAME_LIMIT);
    const role = params?.role === undefined ? {} : { role: oneOf(params, "role", ROLES) };
    const problem = problemWithFile(params ?? {});
    if (problem !== null) {
      throw new ToolError(FILE_PROBLEMS[problem], INVALID_PARAMS);
    }
    const { path, size, file_name: fileName } = params;
    return actTrusted(chrome, "choose_file", params, {
      ...role,
      name,
      path,
      size,
      file_name: fileName,
    });
  },

  // The things the person held out in the browser since the last call, in the
  // order they held them, and then forgets them. It takes no argument and names
  // no page: what it returns is what the person chose, with the address of the
  // page it was held on (held.js). An item held at a blocked site is left out
  // without being counted, as a tab there is left out of list_tabs. The text and
  // the addresses are the page's, whoever selected them, and they reach a
  // session as a result it cannot read directly.
  async take_held(chrome, params) {
    if (params !== undefined && Object.keys(params).length > 0) {
      throw new ToolError("take_held takes no argument", INVALID_PARAMS);
    }
    const sites = await blockedSites(chrome);
    const { items, forgotten } = await takeHeld(chrome);
    // The address of the page is kept cut, and the origin of the whole address
    // beside it, so the blocked sites are matched against the origin.
    const held = items
      .filter((item) => !isBlocked(sites, item.origin ?? item.page))
      .map((item, index) => ({
        number: index + 1,
        kind: item.kind,
        origin: originOf(item.origin ?? item.page),
        ...(item.kind === "selection" ? { text: item.text } : {}),
        ...(item.kind === "tab" ? { address: item.page } : {}),
        ...(item.kind === "link" ? { address: item.address } : {}),
        ...(item.cut === true ? { cut: true } : {}),
      }));
    return { held, forgotten };
  },
};

// The name of the first argument that is a list of more than LIST_LIMIT
// entries, or undefined where none is.
function longList(params) {
  if (typeof params !== "object" || params === null) {
    return undefined;
  }
  return Object.keys(params).find(
    (name) => Array.isArray(params[name]) && params[name].length > LIST_LIMIT,
  );
}

// The origins of the URLs a call names, each once, as the activity log keeps
// them. The path, query and fragment of a URL, and the rest of the arguments,
// are left out. `to` is the address a tab is taken to.
function originsOf(params) {
  const named = [params?.url, params?.frame_url, params?.to];
  if (Array.isArray(params?.urls)) {
    for (const url of params.urls) {
      named.push(url);
    }
  }
  const origins = new Set(
    named.map(originOf).filter((origin) => origin !== null),
  );
  return [...origins].slice(0, ORIGINS_PER_ENTRY);
}

// The reply to one request from the native host: its id, and the tool's result
// or why there is none. A tool that is off, or whose group is off or has outlived
// the lifetime it was turned on for, or whose group's browser permissions are
// not held when the call arrives, is refused before it touches the browser, and
// so is one with a list argument of more than LIST_LIMIT entries.
// Every request is added to the activity log, whatever came of it, before the
// reply is returned.
export async function handle(message, chrome) {
  const id = message?.id ?? null;
  const call = {
    time: Date.now(),
    group: null,
    tool: null,
    outcome: FAILED,
    origins: [],
  };
  let reply;
  try {
    const method = message?.method;
    const tool = Object.hasOwn(TOOLS, method) ? TOOLS[method] : undefined;
    const group = tool ? groupOf(method) : undefined;
    if (tool) {
      call.tool = method;
      call.group = group ?? null;
    }
    const long = longList(message?.params);
    if (long !== undefined) {
      throw new ToolError(
        `${long} may hold at most ${LIST_LIMIT} entries`,
        INVALID_PARAMS,
      );
    }
    call.origins = originsOf(message?.params);
    if (!tool) {
      throw new Refusal(
        `there is no method named ${JSON.stringify(method)}`,
        METHOD_NOT_FOUND,
      );
    }
    const allowed = await settings(chrome);
    if (allowed[method] !== true) {
      throw new Refusal(
        `${method} is turned off in the BraveBot extension's options`,
      );
    }
    if (group === undefined) {
      throw new Refusal(`${method} is in no group of the BraveBot extension`);
    }
    if (group !== ALWAYS_ON) {
      if ((await groupSwitches(chrome))[group] !== true) {
        throw new Refusal(
          `${method} is in the ${group} group, which is turned off in the BraveBot extension's options`,
        );
      }
      const ended = await lifetimeEnded(chrome, group);
      if (ended !== null) {
        throw new Refusal(
          `${method} is refused: the ${group} group was turned on ${endedText(ended)}, and that has ended. Turn it on again in the BraveBot extension's options`,
        );
      }
    }
    const reachesPages = GROUPS[group].hosts && !NAMES_NO_PAGE.includes(method);
    const allowedOnly =
      GROUPS[group].allowedOnly === true || ALLOWED_ONLY_TOOLS.includes(method);
    const access =
      reachesPages || allowedOnly ? await siteAccess(chrome) : NO_ACCESS;
    if (
      allowedOnly ||
      (reachesPages && scopeOf(group, access) === SCOPE_ALLOWED)
    ) {
      refuseUnlessAllowed(access.allowed, message.params);
    }
    if (!(await holds(chrome, group, access))) {
      throw new Refusal(
        `${method} needs the ${group} group's browser permissions, which are not granted in the BraveBot extension's options${
          reachesPages ? SITE_ACCESS_NOTE : ""
        }${NEEDS_ALL_URLS.includes(method) ? ALL_SITES_NOTE : ""}`,
      );
    }
    const result = wellFormed(await tool(chrome, message.params ?? {}));
    call.outcome = RAN;
    reply = { jsonrpc: "2.0", id, result };
  } catch (error) {
    const code = error instanceof ToolError ? error.code : SERVER_ERROR;
    call.outcome = error instanceof Refusal ? REFUSED : FAILED;
    reply = {
      jsonrpc: "2.0",
      id,
      error: { code, message: String(error?.message ?? error) },
    };
  }
  await recordCall(chrome, call);
  return reply;
}
