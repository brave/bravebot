// The service worker. It answers requests from the BraveBot extension and from
// no other sender. The only other thing it listens for is a connection from one
// of its own two windows, the confirmation window and the dialog panel, which
// windows.js hands to the window it belongs to. As it starts it closes the windows
// and detaches the debugger sessions that a worker which stopped left. The browser
// reports an event only to a listener registered as the worker starts, so every
// listener is registered here, at the top level.

import { setConfirmation } from "./approval.js";
import { detachLeftovers } from "./cdp.js";
import { isFirst } from "./pair.js";
import { answer } from "./requests.js";
import { createWindows, setDialogPanel } from "./windows.js";

chrome.runtime.onMessageExternal.addListener((message, sender, sendResponse) => {
  if (!isFirst(sender)) {
    // No reply of any content: the sender is told nothing, not even that a
    // request was refused.
    return false;
  }
  answer(message, chrome).then(sendResponse);
  return true;
});

const windows = createWindows(chrome);
setConfirmation(windows.ask);
setDialogPanel(windows.askDialog);
windows.listen();
windows.closeLeftovers();
detachLeftovers(chrome);
