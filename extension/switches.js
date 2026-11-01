// Turning a group on and off, and keeping the stored switches in step with what
// the browser holds.
//
// A group has a switch of its own, stored beside the tool switches. Turning it
// on asks the browser for what the group needs and then turns on the tools of
// the group. Turning it off turns them off and gives back only what no other
// group that is on needs. A group is turned on for a lifetime the person
// chooses (lifetimes.js), and the service worker turns it off once that has
// ended. A call checks the tool's switch, the group's switch and lifetime, and
// the browser (handle in tools.js), so a group that is off or has ended has its
// tools off too.
// A preset turns on the groups it names with one request to the browser, and
// turns off the rest. What a group that reads pages needs depends on the sites
// the person allowed and on its scope (`access`, read by siteAccess in
// tools.js), so the functions that ask the browser are given the access the page
// drew, which keeps the request the first thing a click does.
//
// A group that lists keep_awake sets something on the computer that outlives a
// call, so the request it made is released when the group stops being on, and
// whenever it is turned on again (see giveBack). A group that lists take_held
// keeps what a person held out, so that and its entries in the context menu are
// forgotten at the same moments.
//
// The functions that write the stored switches, lifetimes and scopes read what is
// stored and write it back, so they run one at a time (serialised): two that
// overlapped would each write back a copy read before the other wrote, and the
// later write would turn a group the earlier one turned off back on. They run
// one at a time across the options page, the popup and the service worker, which
// take one named lock of the browser (Web Locks) around each. Where a context
// has no locks, the functions of that context still run one at a time among
// themselves and can overlap those of another context. The browser's request
// for permissions is made before a function joins the line, since the browser
// takes it only from a click.
//
// Every function takes the `chrome` object it calls, as tools.js does.

import { releaseAwake } from "./awake.js";
import { forgetHeld, showMenus } from "./held.js";
import {
  ALWAYS_ON,
  GROUPS,
  NO_ACCESS,
  PRESETS,
  SCOPE_ALL,
  SCOPE_ALLOWED,
  combine,
  holds,
  limitedToAllowed,
  needed,
  neededTogether,
  patternOf,
  releasedBy,
  scopeOf,
  unsharedNeeds,
} from "./groups.js";
import {
  LIFETIMES_KEY,
  RUNS_KEY,
  UNTIL_RESTART,
  assertLifetime,
  defaultLifetime,
  lifetimeEnded,
  lifetimeRecords,
  recordOf,
  runMarkers,
} from "./lifetimes.js";
import { NUMBERED_TOOLS, forgetNumbers } from "./numbering.js";
import {
  ALLOWED_SITES_KEY,
  GROUP_SWITCHES_KEY,
  SETTINGS_KEY,
  SITE_SCOPES_KEY,
  groupSwitches,
  parseSites,
  settings,
  siteAccess,
} from "./tools.js";

// The group switches are read by handle, so tools.js holds them.
export { GROUP_SWITCHES_KEY, groupSwitches };

// The name of the lock every context takes, so that the page, the popup and the
// worker, which each load their own copy of this module, share one line.
const WRITERS_LOCK = "bravebot-switches";

// The writers of this context waiting or running, as a promise that never
// rejects.
let writers = Promise.resolve();

// Runs `task` holding the browser's lock, which is granted in the order it was
// asked for and held until the task has finished. A context without locks, and a
// browser that refuses the lock before the task starts, run the task without it.
async function locked(task) {
  const locks = globalThis.navigator?.locks;
  if (typeof locks?.request !== "function") {
    return task();
  }
  let started = false;
  try {
    return await locks.request(WRITERS_LOCK, () => {
      started = true;
      return task();
    });
  } catch (error) {
    if (started) {
      throw error;
    }
    return task();
  }
}

// Runs `task` after the tasks before it have finished, in this context and, where
// there is the lock, in every other, whether they succeeded or not, and answers
// what it answers. A task does not call a function of this module that
// is serialised itself, which would wait for the task that calls it.
function serialised(task) {
  const run = writers.then(() => locked(task));
  writers = run.catch(() => {});
  return run;
}

// The preset that names no group to leave on.
const OFF_PRESET = "off";

// The tool that keeps the computer awake.
const AWAKE_TOOL = "keep_awake";

const listsAwake = (id) => GROUPS[id].tools.includes(AWAKE_TOOL);

// Releases the keep_awake request if one of the groups lists the tool. Where the
// browser no longer has `power`, which is so when the person took the
// permission back in the browser's settings, there is nothing to tell the
// browser, and the saved request and its alarm are forgotten.
async function releaseAwakeOf(chrome, ids) {
  if (ids.some(listsAwake)) {
    await releaseAwake(chrome);
  }
}

// The tool that returns what a person held out.
const HELD_TOOL = "take_held";

const listsHeld = (id) => GROUPS[id].tools.includes(HELD_TOOL);

// Forgets what was held out if one of the groups lists the tool, and removes the
// entries from the context menu. A menu that cannot be changed does not stop the
// items being forgotten: an entry that stays holds nothing while the group is
// off (listenForHolds).
async function forgetHeldOf(chrome, ids) {
  if (!ids.some(listsHeld)) {
    return;
  }
  try {
    await forgetHeld(chrome);
  } finally {
    await showMenus(chrome, false).catch(() => {});
  }
}

const usesNumbers = (id) =>
  GROUPS[id].tools.some((tool) => NUMBERED_TOOLS.includes(tool));

// Forgets the numbers list_page_elements gave if one of the groups lists a tool
// that makes or uses them, so that a list made while the group was on is not
// used after it was turned off or when it is turned on again.
async function forgetNumbersOf(chrome, ids) {
  if (ids.some(usesNumbers)) {
    await forgetNumbers(chrome);
  }
}

// Gives the browser back `back`, for groups that have stopped being on: turned
// off, or turned on again after their lifetime ended. A keep_awake request is
// released first, while the browser still holds `power`, since `chrome.power`
// is gone once it is taken back, and the entries of the context menu are removed
// before `contextMenus` goes in the same way. A release that fails does not stop
// the permissions being taken back, and neither does a list of numbers that
// cannot be forgotten.
async function giveBack(chrome, ids, back) {
  try {
    try {
      await releaseAwakeOf(chrome, ids);
    } finally {
      await forgetHeldOf(chrome, ids);
    }
  } finally {
    try {
      await forgetNumbersOf(chrome, ids);
    } finally {
      if (Object.keys(back).length > 0) {
        await chrome.permissions.remove(back);
      }
    }
  }
}

// Whether the group is on now: its switch is on, its lifetime has not ended and
// the browser holds what it needs. The platform group is on whenever the
// extension is. A permission the browser cannot be asked about is not held.
export async function isOn(chrome, id, switches, access) {
  if (id === ALWAYS_ON) {
    return true;
  }
  if (switches[id] !== true) {
    return false;
  }
  try {
    return (
      (await lifetimeEnded(chrome, id)) === null &&
      (await holds(chrome, id, access))
    );
  } catch {
    return false;
  }
}

// The groups that are on now, in the order of the table: the groups the person
// turned on, other than the platform check, that `isOn` says are on. This is
// what the toolbar badge and the popup show.
export async function groupsOn(chrome) {
  const [switches, access] = await Promise.all([
    groupSwitches(chrome),
    siteAccess(chrome),
  ]);
  const on = [];
  for (const id of Object.keys(GROUPS)) {
    if (id !== ALWAYS_ON && (await isOn(chrome, id, switches, access))) {
      on.push(id);
    }
  }
  return on;
}

// Saves the switches of the groups turned on and of the groups turned off, with
// the switches of their tools, in one write, so a reader never sees a group
// without its tools or half of a preset. A group turned on is given the
// lifetime in `chosen` or its default, from now, and a group turned off loses
// its lifetime. A group turned on until Brave restarts is marked in session
// storage first, so that a group is never saved on without the marker it needs.
// `scopes`, where given, is saved with them as the scopes of the groups that
// read pages. Lifetimes that cannot be read ended the lifetime of every group
// that is on, so they are left as they are while a group other than those
// turned on now is on, and no group takes a lifetime from a record that is not
// theirs. Once none is on, there is nothing for the damage to end, and the
// lifetimes are written afresh.
async function save(
  chrome,
  turnedOn,
  turnedOff,
  switches,
  chosen = {},
  scopes = undefined,
) {
  const tools = await settings(chrome);
  const stored = await lifetimeRecords(chrome);
  const records = stored ?? {};
  const runs = await runMarkers(chrome);
  const now = Date.now();
  const kept = { ...switches };
  for (const id of turnedOn) {
    kept[id] = true;
    for (const tool of GROUPS[id].tools) {
      tools[tool] = true;
    }
    const lifetime = chosen[id] ?? defaultLifetime(id);
    records[id] = recordOf(lifetime, now);
    if (lifetime === UNTIL_RESTART) {
      runs[id] = true;
    } else {
      delete runs[id];
    }
  }
  for (const id of turnedOff) {
    delete kept[id];
    for (const tool of GROUPS[id].tools) {
      tools[tool] = false;
    }
    delete records[id];
    delete runs[id];
  }
  const covered =
    stored === null && Object.keys(kept).some((id) => !turnedOn.includes(id));
  await chrome.storage.session.set({ [RUNS_KEY]: runs });
  await chrome.storage.local.set({
    [GROUP_SWITCHES_KEY]: kept,
    [SETTINGS_KEY]: tools,
    ...(covered ? {} : { [LIFETIMES_KEY]: records }),
    ...(scopes === undefined ? {} : { [SITE_SCOPES_KEY]: scopes }),
  });
}

// Asks the browser for what the group needs, and turns the group and its tools
// on for `lifetime` if it is granted. It answers whether it was. The browser
// takes the request only from a click, so the caller is the handler of one, and
// the request is the first thing this does. A request the browser refuses to
// put, such as one an organization blocks, is thrown.
export async function turnOn(
  chrome,
  id,
  access,
  lifetime = defaultLifetime(id),
) {
  assertLifetime(lifetime);
  const wanted = needed(id, GROUPS, access);
  if (
    Object.keys(wanted).length > 0 &&
    (await chrome.permissions.request(wanted)) !== true
  ) {
    return false;
  }
  return serialised(async () => {
    // A request left from before the group was turned off, or from a lifetime
    // that ended before the worker turned the group off, does not carry into
    // this one, and neither does an item held out then.
    await releaseAwakeOf(chrome, [id]);
    if (listsHeld(id)) {
      await forgetHeld(chrome);
    }
    await forgetNumbersOf(chrome, [id]);
    await save(chrome, [id], [], await groupSwitches(chrome), {
      [id]: lifetime,
    });
    return true;
  });
}

// Turns the group and its tools off, then gives the browser back what no group
// that is still on needs. The switches are saved first, so that if the browser
// cannot take a permission back the tools are already off.
export function turnOff(chrome, id) {
  return serialised(() => turnOffNow(chrome, id));
}

// turnOff, for a task that is already serialised.
async function turnOffNow(chrome, id) {
  const [switches, access] = await Promise.all([
    groupSwitches(chrome),
    siteAccess(chrome),
  ]);
  await save(chrome, [], [id], switches);
  const others = Object.keys(switches).filter((other) => other !== id);
  await giveBack(chrome, [id], unsharedNeeds(id, others, GROUPS, access));
}

// Every optional permission and host the browser holds for the extension: what
// the groups list among the permissions it holds, and every host, since the
// manifest asks for none at install.
async function heldOptional(chrome) {
  const held = await chrome.permissions.getAll();
  const listed = new Set(
    Object.values(GROUPS).flatMap((group) => group.permissions),
  );
  return combine([
    {
      permissions: (held.permissions ?? []).filter((name) => listed.has(name)),
      origins: held.origins ?? [],
    },
  ]);
}

// The groups a preset turns on. The platform group is always on, so a preset
// that names it turns on nothing for it.
export function presetGroups(name) {
  if (!Object.hasOwn(PRESETS, name)) {
    throw new Error(`there is no preset named ${name}`);
  }
  return PRESETS[name].filter((id) => id !== ALWAYS_ON);
}

// Makes the groups of the preset the groups that are on: asks the browser once
// for everything they list, turns on the groups that were off with their tools,
// turns off every other group with its tools, and gives back what no group
// left on needs. A group of the preset that reads pages is limited to the
// allowed sites, whatever scope it had, and the access to all sites it held is
// given back unless a group left on lists it. A preset that names no group,
// which is Off, gives back every optional permission and host the browser holds
// for the extension, including the sites that saving the allowed sites granted
// while no group was on. It answers whether the browser granted the request. A
// group that was already on keeps the tool switches and the lifetime it has;
// one that was off, or whose lifetime has ended, is given the default lifetime
// of its group. As in turnOn, the request is the first thing this does, since
// the caller is the handler of a click, and a request the browser refuses to
// put is thrown. Nothing is saved unless the request is granted, and the
// switches are saved before anything is taken back.
export async function applyPreset(chrome, name, access = NO_ACCESS) {
  const on = presetGroups(name);
  const limited = limitedToAllowed(on, access);
  const wanted = neededTogether(on, GROUPS, limited);
  if (
    Object.keys(wanted).length > 0 &&
    (await chrome.permissions.request(wanted)) !== true
  ) {
    return false;
  }
  return serialised(async () => {
    const switches = await groupSwitches(chrome);
    const off = Object.keys(switches).filter((id) => !on.includes(id));
    const lapsed = [];
    for (const id of on) {
      if (switches[id] === true && (await lifetimeEnded(chrome, id)) !== null) {
        lapsed.push(id);
      }
    }
    const turnedOn = on.filter(
      (id) => switches[id] !== true || lapsed.includes(id),
    );
    const widened = on.filter((id) => scopeOf(id, access) === SCOPE_ALL);
    await save(
      chrome,
      turnedOn,
      off,
      switches,
      {},
      widened.length > 0 ? limited.scopes : undefined,
    );
    const back =
      on.length === 0
        ? await heldOptional(chrome)
        : combine([
            ...off.map((id) => unsharedNeeds(id, on, GROUPS, limited)),
            releasedBy(
              on.filter((id) => switches[id] === true),
              access,
              limited,
            ),
          ]);
    await giveBack(chrome, [...off, ...turnedOn], back);
    return true;
  });
}

// Turns every group off and gives the browser back every optional permission and
// host, which is what the Off preset does, so a keep_awake request is released
// before `power` is taken back. The popup and the keyboard command both call
// this. It asks the browser for nothing.
export async function turnEverythingOff(chrome) {
  await applyPreset(chrome, OFF_PRESET);
}

// Turns one tool on or off, keeping the other tools' switches as they are.
// Turning keep_awake off releases the request it made: a call to undo it is
// refused once the tool is off, and the request would stand until its time.
export function setTool(chrome, tool, on) {
  return serialised(async () => {
    const tools = await settings(chrome);
    tools[tool] = on;
    await chrome.storage.local.set({ [SETTINGS_KEY]: tools });
    if (tool === AWAKE_TOOL && on !== true) {
      await releaseAwake(chrome);
    }
  });
}

// Turns off each group whose switch is on and whose permissions the browser no
// longer holds, as it does when a permission is taken back in the browser's
// extension settings. The group's tools are turned off with it, so that
// granting the permission again for another group does not bring them back. A
// check that fails turns nothing off.
export function turnOffUnheld(chrome) {
  return serialised(async () => {
    const access = await siteAccess(chrome);
    for (const id of Object.keys(GROUPS)) {
      const switches = await groupSwitches(chrome);
      if (switches[id] === true && !(await holds(chrome, id, access))) {
        await turnOffNow(chrome, id);
      }
    }
  });
}

// Turns off each group whose switch is on and whose lifetime has ended, as the
// service worker does at each start and on each alarm. A call is refused as soon
// as a lifetime ends; this gives the permissions back and turns the group and
// its tools off. A lifetime that cannot be read has ended. The groups are chosen
// before the first is turned off, since that rewrites the lifetimes.
export function turnOffLapsed(chrome) {
  return serialised(async () => {
    const ended = [];
    for (const id of Object.keys(await groupSwitches(chrome))) {
      if ((await lifetimeEnded(chrome, id)) !== null) {
        ended.push(id);
      }
    }
    for (const id of ended) {
      await turnOffNow(chrome, id);
    }
  });
}

// Saves the sites the person wrote as the allowed sites. The browser is asked
// for access to every site written, first, since the browser accepts the
// request only from a click and the caller is the handler of one; a site it
// already grants is not asked about again. A site written and not granted
// stops the save, and so does a line that is not one address, which is
// returned in `rejected`. Once saved, the access of a site that is no longer
// written is taken back, and `notTakenBack` is the browser's message where it
// could not be. It answers `granted`: whether the list was saved.
export async function allowSites(chrome, input) {
  const { sites, rejected } = parseSites(input);
  if (rejected.length > 0) {
    return { sites, rejected, granted: false, notTakenBack: null };
  }
  if (
    sites.length > 0 &&
    (await chrome.permissions.request({ origins: sites.map(patternOf) })) !==
      true
  ) {
    return { sites, rejected, granted: false, notTakenBack: null };
  }
  const { allowed } = await siteAccess(chrome);
  await chrome.storage.local.set({ [ALLOWED_SITES_KEY]: sites });
  const gone = allowed.filter((site) => !sites.includes(site)).map(patternOf);
  let notTakenBack = null;
  if (gone.length > 0) {
    try {
      await chrome.permissions.remove({ origins: gone });
    } catch (error) {
      notTakenBack = error.message;
    }
  }
  return { sites, rejected, granted: true, notTakenBack };
}

// Changes which sites a group that reads pages reaches: only the allowed sites,
// or all sites except the blocked ones. `access` is what the person's click saw
// and `on` whether the group is on. A group that is on asks the browser first,
// in this click, for what it needs under the new scope, and the scope is saved
// only if that is granted. Narrowing it to the allowed sites then takes back
// the access the old scope held and the new one does not need, which is the
// access to all sites unless a group still on is on all sites too. A group that
// is off only saves the scope. It answers whether the scope was saved.
export async function setScope(chrome, id, scope, access, on) {
  if (!GROUPS[id].hosts) {
    throw new Error(`${id} does not read pages`);
  }
  if (GROUPS[id].allowedOnly === true) {
    throw new Error(`${id} is limited to the allowed sites and has no scope`);
  }
  if (scope !== SCOPE_ALLOWED && scope !== SCOPE_ALL) {
    throw new Error(`there is no scope named ${scope}`);
  }
  const scopes = { ...access.scopes };
  if (scope === SCOPE_ALL) {
    scopes[id] = SCOPE_ALL;
  } else {
    delete scopes[id];
  }
  const after = { allowed: access.allowed, scopes };
  const wanted = needed(id, GROUPS, after);
  if (
    on &&
    Object.keys(wanted).length > 0 &&
    (await chrome.permissions.request(wanted)) !== true
  ) {
    return false;
  }
  return serialised(async () => {
    await chrome.storage.local.set({ [SITE_SCOPES_KEY]: scopes });
    if (on && scope === SCOPE_ALLOWED) {
      const enabled = Object.keys(await groupSwitches(chrome));
      const back = releasedBy(enabled, access, after);
      if (Object.keys(back).length > 0) {
        await chrome.permissions.remove(back);
      }
    }
    return true;
  });
}
