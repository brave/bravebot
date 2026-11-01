# The BraveBot Trusted Input extension

A second Brave extension that does what the [BraveBot extension](../extension/README.md) cannot do
with the permissions Brave lets an extension ask for later. It sends real mouse and keyboard input
to a page, takes a picture of the whole of a page, reads a page's console, and chooses a file on a
page's file input. It does these through the browser's debugger, after the BraveBot extension asks
and after you press **Allow once** in a window of its own. It holds one permission, `debugger`, and nothing else.

[docs/specs/browser.md](../docs/specs/browser.md) (BROWSER-37, -38, -43, -44 and -45) is what it owes the
BraveBot extension, and [docs/design/browser-extension.md](../docs/design/browser-extension.md) is why
it is built this way.

It is for `input-trusted`, `capture-trusted`, `console-trusted` and `upload-trusted`, four groups of the BraveBot
extension. A session that uses none of them does not need it.

## What it is and is not

- It is not a native messaging client. It never starts the relay, and `bravebot-browser install`
  records the BraveBot extension's id only. The BraveBot extension stays the only extension that a
  session reaches.
- It takes a request from the BraveBot extension's id and from nothing else. A web page, another
  extension and a session cannot send it one. A request is a method name and arguments from closed
  lists and bounds: an address, a role from a list of ten, an accessible name, a text of at most
  2,000 characters, a key from a list of eleven, a mouse button and a click count, for the console a
  level and a count, and for a file its absolute path, size and name. A selector, a node number or a
  coordinate in a request is refused and not read.
- It has no switches, site lists, options page or activity log of its own. The BraveBot extension
  checks its own switches, the group's lifetime, the allowed sites and the blocked sites before it
  sends anything here, and this extension asks you before anything happens.
- It sends the BraveBot extension a status word and the page's origin, or a sentence from a fixed
  list. The exceptions are the picture of `screenshot_full_page` and the entries of `read_console`.
  It returns nothing else that a page wrote.

## The permission, and why it has no off switch in Brave

Chrome does not let an extension ask for `debugger` later, so it cannot be one of the optional
permissions that the BraveBot extension asks for when you turn a group on. This extension holds it
from the time it is installed and enabled. Brave lists two warnings for it, that it accesses the
page debugger backend and that it reads and changes data on all websites.

For that reason this extension has no off state that Brave holds the way it holds the BraveBot
extension's groups. The BraveBot extension's **Turn everything off** turns off the four groups that
send it requests. It cannot disable this extension, which would need the `management` permission that
this project does not use.

**Keep it turned off in `brave://extensions` until you want it,** and turn it on when you turn on one
of its groups. A turned-off extension cannot use the debugger. While it is on it still does nothing by
itself: it acts only on a request from the BraveBot extension, and acts on a page only after you
press **Allow once** for that action.

## Installing it

Install the BraveBot extension first, as its [README](../extension/README.md) describes, and then
this one beside it.

1. Open `brave://extensions`, turn on **Developer mode**, choose **Load unpacked** and pick this
   directory, `extension-trusted/`. It needs a Brave built on Chrome 120 or later.
2. The `key` in `manifest.json` fixes its id at `lmdelobjcmmjoapnmnkfpmjlfikgepbl` wherever it is
   loaded from. The BraveBot extension has the id `fcjamhpiedeihbbbpndcjngkjaogfgep`.
3. Open the BraveBot extension's options and turn on the group you want, `input-trusted`,
   `capture-trusted`, `console-trusted` or `upload-trusted`. The groups ask Brave for no permission and are limited to
   the sites under **Allowed sites**, with no choice of all sites.
4. Declare the servers. `bravebot-browser install --groups input-trusted,capture-trusted,console-trusted,upload-trusted`
   prints one `bravebot mcp add` line for each, with `--ask-every-call` and `--tool-timeout 120`.

On Windows `bravebot-browser` only prints that the platform is not supported and exits, so no server
starts and neither half runs.

If this extension is not installed, is turned off, or does not accept the BraveBot extension, a call
to one of its tools fails with one sentence that names those three possibilities, and nothing else
changes. Brave words the three alike, so the session cannot tell which it is.

## The two ids

The two extensions name each other by id. Each id is the one the extension's manifest `key` gives
when it is loaded unpacked, and a test derives each from its key (`each_extensions_id_is_the_one_its_key_gives_and_the_second_names_the_first`
in `crates/browser/tests/extension.rs`).

| Where | What it holds |
|---|---|
| `extension/trusted.js` | `SECOND_ID`, the id the BraveBot extension sends requests to |
| `extension-trusted/pair.js` | `FIRST_ID`, the only sender this extension answers, when the message has no tab (a script of that extension that runs in a page has one), and `SECOND_ID`, its own id |
| `extension-trusted/manifest.json` | `externally_connectable.ids`, which names the BraveBot extension's id so that Brave delivers a message from it and from no one else, and `key`, which gives this extension its id |
| `extension/manifest.json` | `key`, which gives the BraveBot extension its id |
| `crates/browser/src/install.rs` | `EXTENSION_ID`, the BraveBot extension's id, which `install` writes into the host manifest's `allowed_origins` |

A copy of either extension with another id, such as a build from the Chrome Web Store, is refused by
the other until the places above are changed together. Until then the tools say this extension is not
installed.

Only the public key is in the repository. The private key is needed only to pack an extension by hand
and is kept outside the repository, issues, reports and logs. To make a pair of your own for a copy
with another id:

```sh
openssl genrsa -out key.pem 2048
openssl rsa -in key.pem -pubout -outform DER | base64 | tr -d '\n'
```

The second command prints the public key for the manifest's `key`. The id is the first 16 bytes of
the SHA-256 of the DER form of the public key, each half-byte written as a letter from `a` to `p`.

## What it does

Each tool belongs to a group of the BraveBot extension, shown in the Group column. The question in
the terminal, the group's switch and the site lists are the BraveBot extension's. What follows is
what this extension asks you and what it refuses.

| Tool | Group | What it does | What the window shows |
|---|---|---|---|
| `click_element_trusted` | `input-trusted` | Presses and releases the left or right mouse button once, or the left button twice, at the middle of the element | The action (click, double-click or right-click), the page's address, and the element's tag, role, name, start of its text and where it leads |
| `type_text_trusted` | `input-trusted` | Selects the field's text and inserts the new text in its place | The element as above, and the text to be typed in full |
| `press_key_trusted` | `input-trusted` | Presses one key, alone, on the element that has the focus | The element that has the focus, and the key |
| `hover_element` | `input-trusted` | Moves the pointer over the element and presses nothing | The element |
| `drag_element` | `input-trusted` | Presses on one element, moves in eight steps and releases on another | The element it takes, and the element it drops onto in a frame of its own |
| `screenshot_full_page` | `capture-trusted` | Takes a JPEG of the whole top document, scaled to at most 2048 pixels on its long side | The origin, the address and the size of the page, and no element |
| `read_console` | `console-trusted` | Returns the newest entries of the page's console as text | The origin, the address and what is read, and no element |
| `choose_file` | `upload-trusted` | Asks the browser to read one file and set it on the file input; the file is the one the relay's server resolved, and no byte of it passes through this extension | The page's address, the file input as the page names it, and in a frame of its own the file's name, size and full path, with a note that the browser reads the file and gives it to the page |

A file input is named by the name the page gives it and, where given, the role `button`, which is what the browser calls every file input. An input the page shows is named by its accessible name. An input the page hides, as many sites do behind a label or a button, is found too: it is named by what its own markup says, and no script of the page is run. That is the element its `aria-labelledby` points to, its `aria-label`, its labels (the `label` whose `for` is its id, or the `label` around it) and its `title`, the first of them that has words, and only where none has, its `name` attribute or its `id`. The text of a label is the text the browser's tree holds in it, so a label the browser leaves out names nothing, and a label of another document or shadow root does not name the input. A frame's document is not searched, and a button that opens the input by script is not tied to it, so an input that nothing in its markup names is not found. The window says that the page hides the input and whether a label or only its id or name names it. An input the page shows is preferred to the ones it hides, two with the name are refused, and an input the page hides is not scrolled to or tested for a cover, since the browser is given the node. Every other tool refuses a file input, and this tool refuses every element that is not one. The file has to be an absolute path of at most 900 characters, within 10 MiB, with no part named like a credential (`.env`, `*.pem`, `id_rsa*` and the others the relay's server refuses first), and the one request is the only way a path reaches the browser.

An element is named by a role from `button`, `link`, `textbox`, `checkbox`, `radio`, `combobox`,
`menuitem`, `tab`, `option` and `switch`, and by its accessible name as the browser's accessibility
tree gives it. Exactly one displayed element must have both. A page is named by the exact address of
one open tab.

In every window **Deny** has the focus, **Allow once** becomes available a second after the window
is in front and only for a click you make, and, where the box that holds the text, the path of a file
or the element a drag drops onto is longer than it shows, only after you have scrolled the box to its
end. Deny, Escape, closing the window and no answer in 60 seconds all deny. The two buttons stay in
view at the foot of the window however long the page's words are. Every word the page wrote is shown
as text, cut to a limit, with each control or invisible character shown as its code point.

A call sends no input, captures nothing and chooses no file before you allow it. For the five input
tools, the picture and the file, the extension attaches the debugger to the one tab at the address, reads the page and
brings the tab forward before it asks, so it can show you what it found. It then reads the page
again, and sends input or captures only if the page is as it was when you were shown it. For
`read_console` it attaches only after you press **Allow once**.

### What it refuses

It refuses, and sends nothing, when:

- no element, or more than one, has the role and the name, or the only one is hidden or has no size;
- another element, or a frame, covers the middle of the element, or the middle is outside the part of
  the page in view;
- the element is a password field or a payment card field, which is never acted on, or a file chooser
  for any tool but `choose_file`, or is disabled or read-only;
- text is to be typed into an element that is not a text field, or a key is to be pressed with no
  element focused;
- the element, its name or where it goes changed after it was found, or the page left the address;
- a name, text or destination is longer than 2,000 characters, the page has more than 100,000 nodes,
  or more than 50 elements have the role and the name;
- the page has no size, or is so long and narrow that a picture would be less than a pixel wide, or
  the picture is over 8 MiB;
- more than one tab is at the address, none is, or another request or window is open.

Frames are not entered, and the elements of a frame are not found. A closed shadow root is entered.

### A dialog the page opens

A click or a key can make a page open an alert, a question, a prompt or a question about leaving. The
browser answers no input while one is open. No tool answers a dialog and no argument says how to.
This extension opens a panel that shows the kind of dialog, the page's address, what it says and, for
a prompt, the page's suggested answer. **Dismiss** has the focus. **Accept** is available a second
after the panel is in front. For a prompt the panel has a field for your own answer. A panel you close,
or leave unanswered, leaves the dialog open in the page and the call fails.

### Time

The steps before the first window, looking for the tab and reading the page, have 5 seconds together,
and a call that passes them says that nothing was done. You then have 60 seconds for a call, counted
from its first window, and the panel for a dialog counts down what is left of them. The BraveBot
extension waits 80 seconds for this extension's reply, the relay waits 90, and the declaration
`install` prints bounds a call at 120. A press or key that fails is followed by a release, so that no
button or key is left down. An input the browser stops answering ends the call 7 seconds after it was
sent. A call has 11 seconds from your last answer, the Allow or the answer to a dialog, to end
whatever it is waiting for, and 2 more to release what it still holds down and to detach, so a
browser that is slow or stops answering cannot hold it past the 80. A call that runs out of them says
so and may have done the action. It releases a button or key it had pressed once, before it detaches,
unless the page has a dialog open, and where the browser has stopped answering that release may not
arrive.

As the worker starts it detaches the debugger from every tab it lists as attached, which a worker that
stopped left, and a call that arrives with the start waits for that.

### What the debugger shows

Chromium puts a notice on the browser, `"BraveBot Trusted Input" started debugging this browser`
with a **Cancel** button, while an extension has the debugger attached, and keeps it until it is
dismissed. The extension attaches for one call and detaches when it ends, however it ends. A visible
Brave 1.98.63 showed the notice as a bar across the top of every popup window, in the windows of both
extensions, with the sentence, **Cancel** and a close button; the design document records the
measurement. What **Cancel** does to a call was not observed.

## Turning it off and removing it

- Turn it off at `brave://extensions`. This takes the `debugger` permission away from it, and the
  four groups answer that it is not installed or is turned off.
- Remove it with **Remove** on BraveBot Trusted Input there.
- Turning off the four groups in the BraveBot extension, or **Turn everything off**, stops requests
  from being sent to it. It stays enabled.

## What is here

The window code is shared with the BraveBot extension as copies, since an extension cannot load a
file from outside its own directory and the repository has no build step. `extension/shared-files.json`
lists them, and `extension/tests/shared.test.mjs` compares each copy with the original.

| File | What it does |
|---|---|
| `manifest.json`, `package.json` | The manifest, which holds `debugger` alone, and the marker that tells Node the scripts are modules |
| `background.js` | The service worker, which registers the one listener for a message from another extension |
| `pair.js`, `requests.js`, `sentences.js` | The two ids, the table of the eight methods and their arguments, and the closed list of sentences a reply can hold |
| `cdp.js` | The one file that names the debugger's API, with the frozen list of protocol methods and the token that gates the ones that send input or capture |
| `resolve.js` | Finds the one element a role and a name lead to, from the accessibility tree and the flattened document |
| `tools.js`, `capture.js`, `consoles.js` | The five input tools and the file choice, the full-page picture and the console read |
| `windows.js`, `dialog.html`, `dialog.css`, `dialog.js`, `actions.js` | The panel for a dialog, the hand-over of each window to its question, and the words of each action |
| `popup.html`, `popup.css`, `popup.js` | The static popup, which says what the extension is and how to turn it off |
| `approval.js`, `confirm.html`, `confirm.css`, `confirm.js`, `confirmation.js`, `errors.js`, `files.js`, `shown.js`, `words.js` | Copies of the BraveBot extension's confirmation window, its lists of words and its checks of a file's path, size and name |

## Tests

```sh
make check-extension
```

runs `extension/tests/` and `extension-trusted/tests/`. The second directory uses a fake debugger that
records every protocol method in order, and a page recorded in Brave 1.98.62 (`tests/recorded/`) for
the accessibility tree. `cargo test -p bravebot-browser --test extension` holds the manifest, the
ids, the permission and the use of the debugger to the spec.

## Not verified

A person at a headed Brave has not run this extension. These were not observed: the install prompt
and what **Cancel** on the debugging notice does to a call, a click on **Allow once** by a person, the windows'
size and placement in a headed window and a window losing the front to another, a native context
menu from a right click, a native drag in a headed window, an action on `account.brave.com` and the
two other sites Brave protects from scripting, and Linux. The two extensions were not run with the real relay and host together. The runs that were
made used a headless Brave 1.98.62 on macOS and are recorded in the design document.
