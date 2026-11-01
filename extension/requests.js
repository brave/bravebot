// The requests each tab makes, kept from the moment the tab loads a page, for
// list_requests to read.
//
// The browser tells a listener about a request only as much as the listener
// asks for. These listeners give a URL filter and nothing else, so what reaches
// them is a request's URL, type, frame and outcome: no header, no cookie and no
// body, which the browser hands over only to a listener that names the extra
// information it wants. None here does, and a test holds that.
//
// A request is kept only while the network-watch group and list_requests are
// both on, so a tool that is off keeps nothing. The browser reports an event
// only to a listener registered as the worker starts, before the switches can be
// read, so the listeners are registered whatever the switches are and each one
// asks whether requests are being kept. The worker tells it with keepRequests.
// Until it has, nothing is kept.
//
// The requests are kept in the worker's memory, in a record for each tab, and
// nowhere else: not in storage, which the options page reads, and not in a
// value a tool returns whole. A record is the requests of one load of one tab,
// found by the tab's id, and list_requests reads the record of the one tab a
// URL names. The browser may stop the worker. The records go with it, and a
// worker that starts again has none, so a page that loaded before it started
// has no record and is told so. A request is kept only for a tab whose load
// this worker saw begin, so a record is never the end of a load.

// The most requests kept for one load. The first ones are kept, and the record
// says that more came.
export const REQUEST_LIMIT = 500;

// The pages the listeners are told about. A host the extension holds no
// permission for is not reported whatever this says.
const FILTER = Object.freeze({ urls: ["http://*/*", "https://*/*"] });

// For each `chrome` object being watched: the records by tab id and how to stop.
const watches = new WeakMap();

// For each `chrome` object: whether requests are being kept now. A `chrome` the
// worker has not told keeps none.
const keeping = new WeakMap();

// Says whether requests are kept. Turning it off forgets the records, so that
// what a tool that is off kept is not left to be read once it is on again.
export function keepRequests(chrome, on) {
  keeping.set(chrome, on === true);
  if (on !== true) {
    watches.get(chrome)?.loads.clear();
  }
}

// Starts keeping the requests of each tab, where the browser lets the
// extension see them: the `webRequest` permission is held, which makes
// `chrome.webRequest` exist. Does nothing where it is not, or where this
// `chrome` is watched already. Called as the worker starts, before it has
// awaited anything, since the browser reports an event only to a listener
// registered then.
export function watchRequests(chrome) {
  if (!chrome.webRequest || watches.has(chrome)) {
    return;
  }
  const loads = new Map();

  const started = (details) => {
    if (
      keeping.get(chrome) !== true ||
      details.tabId < 0 ||
      details.documentLifecycle === "prerender"
    ) {
      return;
    }
    let load = loads.get(details.tabId);
    // A redirect of the page reports the same request again, which is part of
    // the load it began.
    if (details.type === "main_frame" && !load?.seen.has(details.requestId)) {
      load = { frames: new Map(), requests: [], seen: new Map(), more: false };
      loads.set(details.tabId, load);
    }
    if (!load) {
      return;
    }
    if (details.type === "main_frame" || details.type === "sub_frame") {
      load.frames.set(details.frameId, {
        frameId: details.frameId,
        parentFrameId: details.parentFrameId,
        url: details.url,
      });
    }
    if (load.requests.length >= REQUEST_LIMIT) {
      load.more = true;
      return;
    }
    const entry = {
      url: details.url,
      type: details.type,
      status: null,
      error: null,
      frameId: details.frameId,
      parentFrameId: details.parentFrameId,
      frameUrl: load.frames.get(details.frameId)?.url,
    };
    load.requests.push(entry);
    load.seen.set(details.requestId, entry);
  };
  const entryOf = (details) =>
    loads.get(details.tabId)?.seen.get(details.requestId);
  const redirected = (details) => {
    const entry = entryOf(details);
    if (entry) {
      entry.status = details.statusCode;
    }
  };
  const completed = (details) => {
    const entry = entryOf(details);
    if (entry) {
      entry.status = details.statusCode;
      loads.get(details.tabId).seen.delete(details.requestId);
    }
  };
  const failed = (details) => {
    const entry = entryOf(details);
    if (entry) {
      entry.error = details.error;
      loads.get(details.tabId).seen.delete(details.requestId);
    }
  };
  const closed = (tabId) => {
    loads.delete(tabId);
  };

  const listeners = [
    [chrome.webRequest.onBeforeRequest, started, FILTER],
    [chrome.webRequest.onBeforeRedirect, redirected, FILTER],
    [chrome.webRequest.onCompleted, completed, FILTER],
    [chrome.webRequest.onErrorOccurred, failed, FILTER],
    [chrome.tabs.onRemoved, closed],
  ];
  for (const [event, listener, filter] of listeners) {
    if (filter) {
      event.addListener(listener, filter);
    } else {
      event.addListener(listener);
    }
  }
  watches.set(chrome, {
    loads,
    stop() {
      for (const [event, listener] of listeners) {
        event.removeListener(listener);
      }
    },
  });
}

// Stops keeping requests and forgets every one kept. A watch started later
// has records of its own.
export function stopWatching(chrome) {
  const watch = watches.get(chrome);
  if (!watch) {
    return;
  }
  watches.delete(chrome);
  watch.stop();
}

// A copy of what was kept for the load of one tab: its requests in the order
// they began, the frames they were made in, and whether more came than were
// kept. Undefined where nothing is kept for the tab, which is when this worker
// did not see the page begin loading.
export function requestsOf(chrome, tabId) {
  const load = watches.get(chrome)?.loads.get(tabId);
  return (
    load && {
      requests: load.requests.map((request) => ({ ...request })),
      frames: [...load.frames.values()].map((frame) => ({ ...frame })),
      more: load.more,
    }
  );
}
