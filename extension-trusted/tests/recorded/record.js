// How world.json was made. This is an expression to run in the service worker
// of the extension, in Brave, with page.html served and open in a tab at an
// address ending /page.html. It attaches to that tab and writes down what the
// browser answers for the reads that resolve.js makes, so that resolve.test.mjs
// can run against a real page and not one drawn by hand. The result is the
// object that world.json holds, apart from the "recorded" note and the address,
// which was rewritten to https://example.test. It is not part of the extension
// and no test runs it. hidden-world.json was made the same way from
// hidden-inputs.html, with PAGE set to "/hidden-inputs.html".
(async () => {
  const PAGE = "/page.html";
  const tabs = (await chrome.debugger.getTargets()).filter((x) => x.type === "page");
  const tab = tabs.find((x) => x.url.includes(PAGE));
  const tabId = tab.tabId;
  const send = (m, p = {}) => chrome.debugger.sendCommand({ tabId }, m, p);
  const quiet = async (m, p) => { try { return await send(m, p); } catch { return null; } };
  await chrome.debugger.attach({ tabId }, "1.3");
  try {
    await send("Accessibility.enable"); await send("DOM.enable"); await send("CSS.enable");
    const tree = await send("Page.getFrameTree");
    const frameTree = {
      frameTree: {
        frame: { id: tree.frameTree.frame.id, url: tree.frameTree.frame.url, ...(tree.frameTree.frame.urlFragment ? { urlFragment: tree.frameTree.frame.urlFragment } : {}) },
        childFrames: (tree.frameTree.childFrames ?? []).map((c) => ({ frame: { id: c.frame.id, parentId: c.frame.parentId, url: c.frame.url } })),
      },
    };
    const layout = await send("Page.getLayoutMetrics");
    const metrics = { cssLayoutViewport: layout.cssLayoutViewport, cssContentSize: layout.cssContentSize };
    const ax = (await send("Accessibility.getFullAXTree")).nodes;
    const flat = (await send("DOM.getFlattenedDocument", { depth: -1, pierce: true })).nodes;
    const boxes = {}; const styles = {}; const hits = {};
    const withBox = [];
    for (const n of flat) {
      if (n.nodeType !== 1) continue;
      const cs = await quiet("CSS.getComputedStyleForNode", { nodeId: n.nodeId });
      if (cs) styles[n.nodeId] = Object.fromEntries(cs.computedStyle.filter((p) => ["display", "opacity", "visibility"].includes(p.name)).map((p) => [p.name, p.value]));
      const b = await quiet("DOM.getBoxModel", { backendNodeId: n.backendNodeId });
      boxes[n.backendNodeId] = b ? { border: b.model.border, width: b.model.width, height: b.model.height } : null;
      if (b) withBox.push(n);
    }
    // What is at the middle of each box, asked after the element is scrolled into
    // view and in page coordinates, as resolve.js asks.
    for (const n of withBox) {
      await quiet("DOM.scrollIntoViewIfNeeded", { backendNodeId: n.backendNodeId });
      const b = await quiet("DOM.getBoxModel", { backendNodeId: n.backendNodeId });
      const view = (await send("Page.getLayoutMetrics")).cssLayoutViewport;
      if (!b) continue;
      const q = b.model.border;
      const x = Math.round((q[0] + q[2] + q[4] + q[6]) / 4), y = Math.round((q[1] + q[3] + q[5] + q[7]) / 4);
      if (x < 0 || y < 0 || x >= view.clientWidth || y >= view.clientHeight) continue;
      const px = x + Math.round(view.pageX), py = y + Math.round(view.pageY);
      const h = await quiet("DOM.getNodeForLocation", { x: px, y: py, includeUserAgentShadowDOM: false, ignorePointerEventsNone: false });
      if (h) hits[`${px},${py}`] = { backendNodeId: h.backendNodeId, frameId: h.frameId, ...(h.nodeId === undefined ? {} : { nodeId: h.nodeId }) };
    }
    return { frameTree, metrics, ax, flat, boxes, styles, hits };
  } finally { await chrome.debugger.detach({ tabId }).catch(() => {}); }
})()
