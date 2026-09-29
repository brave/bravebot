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

// Which tools may run until a person changes it in the options page. The two
// searches reach the whole of a person's past rather than what is open now, so
// they start off.
export const DEFAULT_SETTINGS = Object.freeze({
  list_tabs: true,
  read_page: true,
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
  async list_tabs(chrome) {
    const tabs = await chrome.tabs.query({});
    return tabs.map((tab) => ({
      id: tab.id,
      window_id: tab.windowId,
      title: tab.title ?? "",
      url: tab.url ?? "",
    }));
  },

  // The tab is found by its URL exactly, since the URL is what a person saw in
  // the question before the call. A tab whose URL only resembles it is a
  // different page.
  //
  // The script says which document it read. The tab can move to another
  // page between being found and being read, and that page is one nobody
  // approved, so what was read is refused unless it is at the same URL.
  async read_page(chrome, params) {
    const url = text(params, "url");
    const tabs = await chrome.tabs.query({});
    const tab = tabs.find((tab) => tab.url === url);
    if (!tab) {
      throw new ToolError(`no open tab is at ${url}`);
    }
    let injected;
    try {
      injected = await chrome.scripting.executeScript({
        target: { tabId: tab.id },
        func: () => ({
          url: location.href,
          title: document.title,
          text: document.body?.innerText ?? "",
        }),
      });
    } catch (error) {
      throw new ToolError(
        `the page at ${url} cannot be read: ${error.message}`,
      );
    }
    const page = injected?.[0]?.result;
    if (!page) {
      throw new ToolError(`the page at ${url} returned nothing when read`);
    }
    if (page.url !== url) {
      throw new ToolError(`the tab left ${url} before it was read`);
    }
    const truncated = page.text.length > PAGE_TEXT_LIMIT;
    return {
      url,
      title: page.title,
      text: truncated ? page.text.slice(0, PAGE_TEXT_LIMIT) : page.text,
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
      result: await tool(chrome, message.params ?? {}),
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
