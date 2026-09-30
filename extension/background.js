// The extension's service worker keeps a native messaging port open to
// bravebot-browser and answers each request on it.
//
// The port is what the native host lives for. Brave starts the host when the
// port opens and ends it when the port closes. While it is open, the host's
// socket exists for a session to reach. An open port also keeps this service
// worker from being stopped for idleness.

import { handle } from "./tools.js";

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
  port = chrome.runtime.connectNative(HOST);
  port.onMessage.addListener(async (message) => {
    const reply = await handle(message, chrome);
    try {
      port?.postMessage(reply);
    } catch {
      // The port closed while the tool ran. The host tells the session that
      // asked.
    }
  });
  port.onDisconnect.addListener(() => {
    // Read so Brave does not report it as unchecked. Missing host, host
    // exited, or refused.
    console.info(
      "bravebot-browser disconnected:",
      chrome.runtime.lastError?.message,
    );
    port = null;
  });
}

chrome.runtime.onStartup.addListener(connect);
chrome.runtime.onInstalled.addListener(connect);
chrome.alarms.onAlarm.addListener(connect);
chrome.alarms.create("reconnect", { periodInMinutes: RECONNECT_MINUTES });
connect();
