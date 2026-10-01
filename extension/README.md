# The BraveBot extension

Lets a BraveBot session read the tabs open in Brave, and your history and bookmarks where you turn
that on. It answers requests from `bravebot-browser`, the relay in [crates/browser](../crates/browser),
over native messaging. [docs/specs/browser.md](../docs/specs/browser.md) is what the two halves
owe each other, and [docs/design/browser-extension.md](../docs/design/browser-extension.md) is why
it is built this way.

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

A session then offers `brave:get_platform_info`, `brave:list_tabs`, `brave:read_page`,
`brave:search_history` and `brave:search_bookmarks`, and asks you before each call, as it does for
any server. Ask it for the platform Brave runs on to check the extension answers: that tool tells
nothing about you.

## What it may do

The extension's options page has a switch for each tool. The platform check, listing and reading
open tabs start on. Searching history and bookmarks start off, since they reach everything you have
visited and saved, and a tool that is off is refused before the browser is asked anything.

`read_page` finds the tab whose URL is exactly the one asked for, and returns up to 100,000
characters of its text, saying when it cut a page short. Brave's own pages, such as settings, and
the Web Store cannot be read.

What the extension returns reaches the session as any server's result does: untrusted, and private.

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
