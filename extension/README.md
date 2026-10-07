# The BraveBot extension

Lets a BraveBot session read the tabs open in Brave, and your history and bookmarks, each once you
turn it on. It answers requests from `bravebot-browser`, the relay in
[crates/browser](../crates/browser), over native messaging.
[docs/specs/browser.md](../docs/specs/browser.md) is what the two halves owe each other, and
[docs/design/browser-extension.md](../docs/design/browser-extension.md) is why it is built this way.

## Trying it

On macOS or Linux, from the repository root:

1. Build the relay and install its host manifest where Brave reads it:

   ```sh
   cargo build --release -p bravebot-browser
   target/release/bravebot-browser install
   ```

   The last line it prints is the declaration for step 3. The manifest names the program by its
   path, so install again after moving it.

2. Load the extension. Open `brave://extensions`, turn on **Developer mode**, choose **Load
   unpacked** and pick this directory. The `key` in `manifest.json` fixes its id at
   `fcjamhpiedeihbbbpndcjngkjaogfgep` wherever it is loaded from, which is the id `install`
   records. Brave starts the relay as the extension loads, and again each time Brave starts.

3. Declare the server with the line `install` printed:

   ```sh
   bravebot mcp add brave -s user --dir ~/.bravebot-browser -- <path to>/bravebot-browser mcp
   ```

4. Check it answers. Start a session and ask it to call `brave:get_platform_info`, which returns
   the operating system and architecture Brave runs on and nothing about you. It is the only tool
   on until you turn others on.

5. Turn on the tools you want. In `brave://extensions`, choose **Details** on BraveBot, then
   **Extension options**. A switch saves as soon as it changes, and the next call uses it.

A session offers `brave:get_platform_info`, `brave:list_tabs`, `brave:list_frames`,
`brave:read_page`, `brave:search_history`, `brave:search_bookmarks` and `brave:open_tab`. It asks you to accept that
list the first time, and again whenever the list changes, then asks you before each call, as it does
for any server.

## What it may do

The extension's options page has a switch for each tool. Only the platform check starts on. Listing
tabs and their frames, reading a page or frame, and searching history and bookmarks start off,
since they reach what you have open, have visited and have saved. A tool that is off is refused
before the browser is asked anything.

`list_frames` finds the tab whose URL is exactly the one asked for, and returns the HTTP and HTTPS
URLs of its frames. Its result is shown to you and kept out of the session's planner: choose one URL
and provide it in your next message. `read_page` reads the top page, or the exact `frame_url` you
provided after `list_frames`, and returns up to 100,000 characters of its text, saying when it cut
the text short. A duplicate frame URL is refused because it does not identify one frame.

`open_tab` opens the HTTP or HTTPS URL you approved in a background tab, signed in as you are, and
returns the URL the tab loaded and no page content. If the page that loads is on a different host,
for example after a redirect, it closes the tab and fails. It starts off like the other tools. To
read what it opened, provide that URL in a later message for `read_page`. Brave's
own pages, such as settings, and the Web Store cannot be read.

What the extension returns reaches the session as any server's result does: untrusted, and private.

## When it does not answer

- **No Brave extension is connected.** Brave is not running, the extension is off or not loaded in
  `brave://extensions`, or Brave could not start the relay. Run `install` again if the relay has
  moved since you last did, since the manifest names it by its path.
- **`<tool>` is turned off in the BraveBot extension's options.** Turn it on, as in step 5.
- **The Brave extension did not reply within 30 seconds.** The call can be made again.

## Removing it

1. `bravebot mcp remove brave`.
2. Choose **Remove** on BraveBot in `brave://extensions`. The relay stops with it.
3. Delete the host manifest `install` wrote, `com.brave.bravebot.json` in
   `~/Library/Application Support/Google/Chrome/NativeMessagingHosts/` on macOS, or
   `~/.config/BraveSoftware/Brave-Browser/NativeMessagingHosts/` on Linux.
4. Delete `~/.bravebot-browser`.

## Tests

```sh
make check-extension
```

runs `extension/tests/` against a fake `chrome` object. `package.json` is there only to tell Node
the scripts are modules. There is nothing to install. `cargo test -p bravebot-browser --test
extension` holds this directory to the relay's side: the extension id, the tool names, and the
host name.

## Not yet decided

- **How it ships.** Loaded unpacked, the pinned key gives it a fixed id. A copy from the Web Store
  or bundled with Brave would have an id of its own, and `install` would record that one.
- **Windows.** BraveBot starts no stdio server there, so neither half runs.
