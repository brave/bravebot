// The extension's service worker keeps a native messaging port open to
// bravebot-browser and answers each request on it.
//
// The port is what the native host lives for. Brave starts the host when the
// port opens and ends it when the port closes. While it is open, the host's
// socket exists for a session to reach. An open port also keeps this service
// worker from being stopped for idleness. The worker also draws the toolbar
// badge, runs the keyboard command that turns every group off, and keeps what a
// person holds out from the context menu.

import { setConfirmation } from "./approval.js";
import { releaseLapsedAwake } from "./awake.js";
import { showBadge } from "./badge.js";
import { createConfirmation } from "./confirmation.js";
import { groupOf, holds } from "./groups.js";
import { listenForHolds, showMenus } from "./held.js";
import { LIFETIMES_KEY } from "./lifetimes.js";
import { keepRequests, stopWatching, watchRequests } from "./requests.js";
import { listenForCapture } from "./screen.js";
import {
  GROUP_SWITCHES_KEY,
  groupSwitches,
  groupsOn,
  turnEverythingOff,
  turnOffLapsed,
  turnOffUnheld,
} from "./switches.js";
import {
  ALLOWED_SITES_KEY,
  SETTINGS_KEY,
  SITE_SCOPES_KEY,
  handle,
  settings,
  siteAccess,
} from "./tools.js";

// The name of the keyboard command in the manifest. It has no key until the
// person gives it one in the browser's shortcut settings.
const TURN_OFF_COMMAND = "turn-everything-off";

// The name `bravebot-browser install` gives the host's manifest.
const HOST = "com.brave.bravebot";

// Checks for a missing port this often, in minutes. Brave's shortest period
// for an alarm.
const RECONNECT_MINUTES = 1;

let port = null;

function connect() {
  if (port) {
    return;
  }
  const current = chrome.runtime.connectNative(HOST);
  port = current;
  current.onMessage.addListener(async (message) => {
    const reply = await handle(message, chrome);
    // On the port the request came in on. A port opened since belongs to
    // another host, whose ids can match the reply to a request of its own.
    try {
      current.postMessage(reply);
    } catch {
      // The port closed while the tool ran. The host tells the session that
      // asked.
    }
  });
  current.onDisconnect.addListener(() => {
    // Read so Brave does not report it as unchecked. Missing host, host
    // exited, or refused.
    console.info(
      "bravebot-browser disconnected:",
      chrome.runtime.lastError?.message,
    );
    port = null;
  });
}

// A group whose permissions the browser no longer holds is turned off, so a
// permission taken back in the browser's extension settings ends the group and
// does not return with the next grant of it. Permissions can also change while
// this worker is not running, so each start checks as well.
function turnOffGroupsNotHeld() {
  turnOffUnheld(chrome).catch((error) => {
    console.info("bravebot groups not checked:", error?.message);
  });
}

// The badge counts the groups that are on, so it is drawn as the worker starts
// and again whenever what decides that changes: the switches, the lifetimes, the
// sites and the permissions the browser holds. A group whose hour has passed is
// counted until the worker turns it off, which is within a minute.
function drawBadge() {
  showBadge(chrome).catch((error) => {
    console.info("bravebot badge not drawn:", error?.message);
  });
}

// The tool that returns what a person held out, and so the group whose entries
// the context menu shows.
const HELD_TOOL = "take_held";

// Whether the group of take_held is on now.
async function heldGroupIsOn() {
  return (await groupsOn(chrome)).includes(groupOf(HELD_TOOL));
}

// Whether a use of an entry holds an item: the group of take_held and the switch
// of take_held are both on, as for the requests list_requests reads. A tool that
// is off has nothing held for it.
async function heldIsOn() {
  const [group, tools] = await Promise.all([heldGroupIsOn(), settings(chrome)]);
  return group && tools[HELD_TOOL] === true;
}

// The entries of the context menu exist while the group of take_held is on, and
// are made again whenever what decides that changes, the same moments as the
// badge. Turning the group off removes them (giveBack in switches.js), at which
// point the permission is still held, so nothing removes them here. With the
// switch of take_held off they stay and hold nothing.
function showHeldMenus() {
  heldGroupIsOn()
    .then((on) => on && showMenus(chrome, true))
    .catch((error) => {
      console.info("bravebot context menu not changed:", error?.message);
    });
}

// A use of an entry holds an item only while the group is on. The listener is
// registered as the worker starts, and again when `contextMenus` is granted,
// since the API does not exist before.
function holdFromMenu() {
  listenForHolds(chrome, heldIsOn);
}

// The values the badge is drawn from. The activity log is written by every
// call and is not among them.
const BADGE_KEYS = [
  GROUP_SWITCHES_KEY,
  LIFETIMES_KEY,
  ALLOWED_SITES_KEY,
  SITE_SCOPES_KEY,
];

function badgeSwitchesChanged(changes, area) {
  if (area === "local" && BADGE_KEYS.some((key) => key in changes)) {
    drawBadge();
    showHeldMenus();
  }
}

// The keyboard command does what Turn everything off in the popup does.
function commandRun(command) {
  if (command === TURN_OFF_COMMAND) {
    turnEverythingOff(chrome).catch((error) => {
      console.info("bravebot groups not all turned off:", error?.message);
    });
  }
}

// A group whose lifetime has ended is turned off, with its tools, and the
// permissions only it listed go back to the browser. A call is refused as soon
// as the lifetime ends whether or not this has run, so this only makes the
// switches and the permissions follow. It runs each time this worker starts,
// which includes each start of the browser and each reload of the extension
// (the points at which a group turned on until Brave restarts ends), and on the
// alarm that reconnects the port.
function turnOffGroupsLapsed() {
  turnOffLapsed(chrome).catch((error) => {
    console.info("bravebot group lifetimes not checked:", error?.message);
  });
}

// A keep_awake request is released when its time has come. The tool made an
// alarm for that time; this runs on every alarm and at each start of the worker
// as well, since an alarm can be lost and a request must not outlive its time.
function releaseKeepAwakeLapsed() {
  releaseLapsedAwake(chrome).catch((error) => {
    console.info("bravebot keep awake not checked:", error?.message);
  });
}

// The requests of each tab are watched for while the browser holds what the
// network-watch group needs (and kept while its switches are on, as
// followRequestSwitches says). They are forgotten when the permission or a host
// is taken back, and kept again from the next page load if the group is still
// held, so a page loaded before a host went has nothing kept.
function keepRequestsWhileHeld(removed) {
  if (
    !(removed.permissions ?? []).includes("webRequest") &&
    (removed.origins ?? []).length === 0
  ) {
    return;
  }
  stopWatching(chrome);
  siteAccess(chrome)
    .then((access) => holds(chrome, "network-watch", access))
    .then((held) => held && watchRequests(chrome))
    .catch((error) => {
      console.info("bravebot requests not watched:", error?.message);
    });
}

// The requests of tabs are kept only while the switch of the network-watch group
// and the switch of list_requests are both on. The listeners that see them are
// registered as the worker starts, so they ask for a copy of the two switches
// that is kept here: read as the worker starts, and read again whenever either
// is saved. Until the first read finishes nothing is kept, and a read that fails
// keeps nothing. Of two reads, the one begun last decides, since they can finish
// in either order.
const REQUESTS_TOOL = "list_requests";
let switchReads = 0;
function followRequestSwitches() {
  switchReads += 1;
  const read = switchReads;
  Promise.all([groupSwitches(chrome), settings(chrome)])
    .then(([groups, tools]) => {
      if (read === switchReads) {
        keepRequests(
          chrome,
          groups[groupOf(REQUESTS_TOOL)] === true &&
            tools[REQUESTS_TOOL] === true,
        );
      }
    })
    .catch((error) => {
      if (read === switchReads) {
        keepRequests(chrome, false);
      }
      console.info("bravebot requests not kept:", error?.message);
    });
}

function requestSwitchesChanged(changes, area) {
  if (
    area === "local" &&
    (GROUP_SWITCHES_KEY in changes || SETTINGS_KEY in changes)
  ) {
    followRequestSwitches();
  }
}

// The window that asks the person before a tool of the input group acts in a
// page. The tools do nothing until it allows them. A window left open by a
// worker that stopped is closed as this one starts, since its question died with
// that worker.
const confirmation = createConfirmation(chrome);
confirmation.listen();
setConfirmation(confirmation.ask);
confirmation.closeLeftovers();

// The window the person chooses in for capture_screen sends the worker what it
// took, and the worker is told when the person closes it.
listenForCapture(chrome);

chrome.runtime.onStartup.addListener(connect);
chrome.runtime.onStartup.addListener(turnOffGroupsNotHeld);
chrome.runtime.onInstalled.addListener(connect);
chrome.permissions.onRemoved.addListener(turnOffGroupsNotHeld);
chrome.permissions.onRemoved.addListener(keepRequestsWhileHeld);
chrome.permissions.onRemoved.addListener(drawBadge);
chrome.permissions.onAdded.addListener(() => watchRequests(chrome));
chrome.permissions.onAdded.addListener(holdFromMenu);
chrome.permissions.onAdded.addListener(drawBadge);
chrome.permissions.onAdded.addListener(showHeldMenus);
chrome.alarms.onAlarm.addListener(connect);
chrome.alarms.onAlarm.addListener(turnOffGroupsLapsed);
chrome.alarms.onAlarm.addListener(releaseKeepAwakeLapsed);
chrome.storage.onChanged.addListener(requestSwitchesChanged);
chrome.storage.onChanged.addListener(badgeSwitchesChanged);
chrome.commands.onCommand.addListener(commandRun);
chrome.alarms.create("reconnect", { periodInMinutes: RECONNECT_MINUTES });
connect();
turnOffGroupsLapsed();
releaseKeepAwakeLapsed();
drawBadge();
// Registered before anything is awaited, as the browser reports an event only
// to a listener registered as the worker starts.
holdFromMenu();
showHeldMenus();
watchRequests(chrome);
followRequestSwitches();
