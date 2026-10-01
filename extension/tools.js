// What each tool does in the browser, and the settings that decide which of
// them may run.
//
// Every function takes the `chrome` object it calls, rather than reading the
// global, so the tests can hand it a fake one. The method names are the tool
// names bravebot-browser offers. Each takes that tool's arguments as its
// parameters: crates/browser/src/tools.rs is the other half of that contract.

// The most text read_page returns from one page, in UTF-16 code units. A page
// longer than this is cut here and says so. A session gets the start of a very
// long page rather than nothing.
export const PAGE_TEXT_LIMIT = 100_000;

// The most results a search returns.
export const MAX_RESULTS = 100;

// How many results a search returns where the call does not say.
export const DEFAULT_RESULTS = 20;

// Where the settings are kept in chrome.storage.local.
export const SETTINGS_KEY = "tools";

// Which tools may run until a person changes it in the options page. Only the
// platform check starts on: it tells nothing about the person. Every other tool
// reaches what they have open, visited or saved, so it reads nothing until they
// turn it on.
export const DEFAULT_SETTINGS = Object.freeze({
  get_platform_info: true,
  list_tabs: false,
  list_frames: false,
  read_page: false,
  search_history: false,
  search_bookmarks: false,
});

// JSON-RPC's code for a method there is no such thing as.
const METHOD_NOT_FOUND = -32601;

// JSON-RPC's code for a call that was refused or could not be done.
const SERVER_ERROR = -32000;

// JSON-RPC's code for arguments that are not what the tool takes.
const INVALID_PARAMS = -32602;

// An error whose message is the reply's message.
export class ToolError extends Error {
  constructor(message, code = SERVER_ERROR) {
    super(message);
    this.code = code;
  }
}

// Which tools may run: the defaults, with what a person set on top.
export async function settings(chrome) {
  const stored = await chrome.storage.local.get(SETTINGS_KEY);
  return { ...DEFAULT_SETTINGS, ...(stored[SETTINGS_KEY] ?? {}) };
}

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

// The tab at exactly `url`, which is the tab a person approved.
async function tabAt(chrome, url) {
  const tabs = await chrome.tabs.query({});
  const tab = tabs.find((candidate) => candidate.url === url);
  if (!tab) {
    throw new ToolError(`no open tab is at ${url}`);
  }
  return tab;
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

export const TOOLS = {
  // What Brave runs on, which is enough to show the extension is installed and
  // answering, and nothing about the person. Only these fields are passed on,
  // whatever else the browser adds to its answer.
  async get_platform_info(chrome) {
    const { os, arch, nacl_arch } = await chrome.runtime.getPlatformInfo();
    return { os, arch, nacl_arch };
  },

  async list_tabs(chrome) {
    const tabs = await chrome.tabs.query({});
    return tabs.map((tab) => ({
      id: tab.id,
      window_id: tab.windowId,
      title: tab.title ?? "",
      url: tab.url ?? "",
    }));
  },

  async list_frames(chrome, params) {
    const url = text(params, "url");
    const tab = await tabAt(chrome, url);
    const frames = await framesAt(chrome, tab, url);
    return frames
      .filter((frame) => webUrl(frame.url))
      .map((frame) => ({ url: frame.url, top: frame.frameId === 0 }));
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
    const tab = await tabAt(chrome, url);
    let frame;
    if (frameUrl !== undefined) {
      if (!webUrl(frameUrl)) {
        throw new ToolError(
          "frame_url must be an HTTP or HTTPS URL from list_frames",
          INVALID_PARAMS,
        );
      }
      const matches = (await framesAt(chrome, tab, url)).filter(
        (candidate) => candidate.url === frameUrl,
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
      injected = await chrome.scripting.executeScript({
        target: {
          tabId: tab.id,
          ...(frame ? { frameIds: [frame.frameId] } : {}),
        },
        func: (expectedUrl) => {
          if (location.href !== expectedUrl) {
            return { url: location.href };
          }
          return {
            url: location.href,
            title: document.title,
            text: document.body?.innerText ?? "",
          };
        },
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
    // Without a start time the history API searches the last 24 hours only.
    const items = await chrome.history.search({
      text: query,
      startTime: 0,
      maxResults: count(params),
    });
    return items.map((item) => ({
      title: item.title ?? "",
      url: item.url ?? "",
      last_visit:
        item.lastVisitTime === undefined
          ? null
          : new Date(item.lastVisitTime).toISOString(),
    }));
  },

  async search_bookmarks(chrome, params) {
    const query = text(params, "query");
    const nodes = await chrome.bookmarks.search(query);
    return nodes
      .filter((node) => typeof node.url === "string")
      .slice(0, count(params))
      .map((node) => ({ title: node.title ?? "", url: node.url }));
  },
};

// The reply to one request from the native host: its id, and the tool's result
// or why there is none. A tool that is off is refused before it touches the
// browser.
export async function handle(message, chrome) {
  const id = message?.id ?? null;
  try {
    const method = message?.method;
    const tool = Object.hasOwn(TOOLS, method) ? TOOLS[method] : undefined;
    if (!tool) {
      throw new ToolError(
        `there is no method named ${JSON.stringify(method)}`,
        METHOD_NOT_FOUND,
      );
    }
    const allowed = await settings(chrome);
    if (allowed[method] !== true) {
      throw new ToolError(
        `${method} is turned off in the BraveBot extension's options`,
      );
    }
    return {
      jsonrpc: "2.0",
      id,
      result: wellFormed(await tool(chrome, message.params ?? {})),
    };
  } catch (error) {
    const code = error instanceof ToolError ? error.code : SERVER_ERROR;
    return {
      jsonrpc: "2.0",
      id,
      error: { code, message: String(error?.message ?? error) },
    };
  }
}
