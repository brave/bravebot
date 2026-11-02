//! The tools the MCP server offers, the group each is in, and the extension method each one calls.
//!
//! The list is fixed here rather than asked of the extension. A person answers for a server's tool
//! list at the start of a session, so a list that changed with whether the extension
//! was connected would offer tools they were not shown. Each tool calls the extension method
//! of the same name with the tool's arguments as its parameters, and which of these the extension
//! answers is its own business: a tool it does not answer fails like any other call.
//!
//! A server started for a group offers the tools of that group and no others. The group is a word
//! on the server's command line, chosen by the person who declares it, and selects a part of this
//! table. Nothing a request or a result holds does.
//!
//! Every argument is one a person can judge from the question BraveBot asks before a call, which
//! shows the arguments as the planner wrote them. A page is named by its URL for that reason: a
//! tab id is a number that says nothing about which page it is.

use serde_json::{Value, json};

/// The most entries a list argument of a call may hold. The extension refuses a longer list before
/// it reads one (BROWSER-27), so every list a schema states says the same, and a client sees the
/// bound before it calls.
pub const LIST_LIMIT: usize = 50;

/// One tool: its name, which is also the extension method it calls, the group it is in, what it
/// does, and its schema.
pub struct Tool {
    pub name: &'static str,
    pub group: &'static str,
    pub description: &'static str,
    schema: fn() -> Value,
}

impl Tool {
    /// The tool as `tools/list` describes it.
    pub fn listed(&self) -> Value {
        json!({
            "name": self.name,
            "description": self.description,
            "inputSchema": (self.schema)(),
        })
    }

    /// Whether the tool returns a picture, which the server sends on as an MCP `image` part where
    /// every other tool's result is sent as text. The answer comes from this table and from no
    /// byte of a result.
    pub fn returns_a_picture(&self) -> bool {
        PICTURES.contains(&self.name)
    }

    /// Whether the tool names a file by a `path`, which the server turns into the file's canonical
    /// path, size and name before it sends the call (`uploads`). The answer comes from this table
    /// and from no byte of a request or a result.
    pub fn takes_a_file(&self) -> bool {
        FILES.contains(&self.name)
    }
}

/// The tools that take a file: the planner's `path` is looked at by the server and replaced, in
/// the request, by what was looked at.
pub const FILES: &[&str] = &["choose_file"];

/// The tools whose result is a picture: the extension answers with an `image` object holding the
/// picture's media type and its base64, and nothing else.
pub const PICTURES: &[&str] = &[
    "screenshot_page",
    "capture_screen",
    "record_gif",
    "screenshot_full_page",
];

/// A group a server can be started for.
pub struct Group {
    pub id: &'static str,
    /// Whether the declaration `install` prints for this group's server says every call to it is
    /// asked about, whatever a rule or an earlier answer says (SERVERS-21). It is set for the
    /// groups whose tools act on data a standing allowance should not cover.
    pub ask_every_call: bool,
    /// Whether the person answers in a window of the browser during a call to this group's tools,
    /// by allowing an action (BROWSER-34) or by choosing what to share in the browser's picker
    /// (BROWSER-42), so that the server waits for the extension's reply longer than it otherwise
    /// does and the declaration bounds a call above that wait. It is set only for a group that is
    /// also asked about every call.
    pub asks_in_browser: bool,
}

impl Group {
    const fn new(id: &'static str) -> Self {
        Self {
            id,
            ask_every_call: false,
            asks_in_browser: false,
        }
    }

    const fn asked_every_call(id: &'static str) -> Self {
        Self {
            id,
            ask_every_call: true,
            asks_in_browser: false,
        }
    }

    const fn asked_in_browser(id: &'static str) -> Self {
        Self {
            id,
            ask_every_call: true,
            asks_in_browser: true,
        }
    }
}

/// The groups a server can be started for, in the order the extension lists them. A tool is in
/// exactly one. A group none of the tools is in has no tool yet and offers an empty list.
pub const GROUPS: &[Group] = &[
    Group::new("platform"),
    Group::new("tabs-read"),
    Group::new("history-read"),
    Group::new("bookmarks-read"),
    Group::new("page-read"),
    Group::new("tabs-manage"),
    Group::new("bookmarks-write"),
    Group::new("navigate"),
    Group::new("downloads"),
    Group::asked_every_call("cookies"),
    Group::new("network-watch"),
    Group::new("browser-info"),
    Group::asked_every_call("remove-data"),
    Group::asked_in_browser("scripts"),
    Group::asked_in_browser("input"),
    Group::asked_in_browser("input-trusted"),
    Group::asked_in_browser("capture-trusted"),
    Group::asked_in_browser("console-trusted"),
    Group::asked_in_browser("upload-trusted"),
    Group::asked_in_browser("screen"),
    Group::new("held"),
];

/// The ids of [`GROUPS`], in order.
pub fn ids() -> impl Iterator<Item = &'static str> {
    GROUPS.iter().map(|group| group.id)
}

/// The roles an element can be asked for by, as `extension/words.js` lists them.
const ROLES: [&str; 10] = [
    "button", "link", "textbox", "checkbox", "radio", "combobox", "menuitem", "tab", "option",
    "switch",
];

/// The most elements a list of `list_page_elements` numbers, as `MAX_LISTED` in
/// `extension/numbering.js` has it, and so the largest `element_number`. A person reads that
/// number off the list the call made, so it is an argument they can judge in the question.
const MAX_LISTED: u64 = 40;

/// The keys `press_key` sends, as `extension/words.js` lists them.
const KEYS: [&str; 11] = [
    "Enter",
    "Escape",
    "Tab",
    "ArrowUp",
    "ArrowDown",
    "ArrowLeft",
    "ArrowRight",
    "Home",
    "End",
    "PageUp",
    "PageDown",
];

/// The mouse buttons `click_element_trusted` presses, as `extension/words.js` lists them.
const BUTTONS: [&str; 2] = ["left", "right"];

/// The values of `levels` for `read_console`, as `extension/consoles.js` lists them: every entry,
/// the warnings and errors, or the errors alone.
const CONSOLE_LEVELS: [&str; 3] = ["all", "warnings_and_errors", "errors"];

/// The most entries `read_console` returns and the number it returns when `max_entries` is left
/// out, as `extension/consoles.js` has them.
const MAX_CONSOLE_ENTRIES: u64 = 200;

/// The most characters of a `path` a call of `choose_file` carries, as `uploads::PATH_LIMIT` has it.
const MAX_PATH: u64 = 900;

/// The most characters of `code` a call of `run_script` carries, as `TYPED_TEXT_LIMIT` in
/// `extension/confirm.js` has it: the window shows the whole of the code or the call is refused.
const MAX_SCRIPT_CODE: u64 = 2000;

/// The most characters of the `url` of a call of `run_script`, as `URL_LIMIT` in
/// `extension/confirm.js` has it.
const MAX_SCRIPT_URL: u64 = 300;

/// Every tool, in the order `tools/list` gives them.
pub const TOOLS: &[Tool] = &[
    Tool {
        name: "get_platform_info",
        group: "platform",
        description: "Say the operating system and architecture Brave runs on, which shows the \
            extension is installed and answering. It tells nothing about the person.",
        schema: || json!({"type": "object", "properties": {}}),
    },
    Tool {
        name: "list_tabs",
        group: "tabs-read",
        description: "List the tabs open in Brave: each tab's id, window id, title and URL.",
        schema: || json!({"type": "object", "properties": {}}),
    },
    Tool {
        name: "list_frames",
        group: "tabs-read",
        description: "List the exact HTTP and HTTPS frame URLs in the open tab at this URL, for a \
            person to choose and provide in a later read_page call.",
        schema: || {
            json!({
                "type": "object",
                "properties": {"url": {"type": "string"}},
                "required": ["url"],
            })
        },
    },
    Tool {
        name: "list_windows",
        group: "tabs-read",
        description: "List the windows open in Brave: each window's id, type, state, whether it \
            has focus and whether it is private. No tab is listed.",
        schema: || json!({"type": "object", "properties": {}}),
    },
    Tool {
        name: "list_tab_groups",
        group: "tabs-read",
        description: "List the tab groups in Brave: each group's id, window id, title and color, \
            and whether it is collapsed.",
        schema: || json!({"type": "object", "properties": {}}),
    },
    Tool {
        name: "list_recently_closed",
        group: "tabs-read",
        description: "List the tabs and windows closed recently in Brave: when each closed, and \
            the title and URL of the tab, or of each tab in the window.",
        schema: || json!({"type": "object", "properties": {}}),
    },
    Tool {
        name: "read_page",
        group: "page-read",
        description: "Read the text of the open tab at this URL, or the exact frame_url in it that \
            a person chose after list_frames.",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "url": {"type": "string"},
                    "frame_url": {"type": "string"},
                },
                "required": ["url"],
            })
        },
    },
    Tool {
        name: "search_history",
        group: "history-read",
        description: "Search Brave's browsing history by words in a page's title or URL.",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string"},
                    "max_results": {"type": "integer", "minimum": 1, "maximum": 100},
                },
                "required": ["query"],
            })
        },
    },
    Tool {
        name: "get_visits",
        group: "history-read",
        description: "List the visits Brave recorded to this exact URL, newest first: when each \
            happened, how the person got there and whether it was on this device.",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "url": {"type": "string"},
                    "max_results": {"type": "integer", "minimum": 1, "maximum": 100},
                },
                "required": ["url"],
            })
        },
    },
    Tool {
        name: "list_top_sites",
        group: "history-read",
        description: "List the sites Brave counts as the ones the person visits most: each \
            site's title and URL.",
        schema: || json!({"type": "object", "properties": {}}),
    },
    Tool {
        name: "list_synced_tabs",
        group: "history-read",
        description: "List the tabs open on the person's other devices, as Brave last synced \
            them: each device's name, and the title and URL of each of its tabs.",
        schema: || json!({"type": "object", "properties": {}}),
    },
    Tool {
        name: "search_bookmarks",
        group: "bookmarks-read",
        description: "Search Brave's bookmarks by words in a bookmark's title or URL.",
        schema: || {
            json!({
                "type": "object",
                "properties": {"query": {"type": "string"}},
                "required": ["query"],
            })
        },
    },
    Tool {
        name: "list_bookmark_folder",
        group: "bookmarks-read",
        description: "List the bookmarks and folders directly in the bookmark folder with this \
            exact title: each bookmark's title and URL, and each folder's title. It fails if no \
            folder, or more than one, has that title. The built-in folders at the top of the \
            tree (such as Bookmarks bar and Other bookmarks) are named by the title the browser \
            shows. With no folder, it lists the titles of those built-in folders.",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "folder": {"type": "string"},
                    "max_results": {"type": "integer", "minimum": 1, "maximum": 100},
                },
            })
        },
    },
    Tool {
        name: "list_recent_bookmarks",
        group: "bookmarks-read",
        description: "List the bookmarks added most recently, newest first: each bookmark's \
            title, URL and when it was added.",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "max_results": {"type": "integer", "minimum": 1, "maximum": 100},
                },
            })
        },
    },
    Tool {
        name: "list_reading_list",
        group: "bookmarks-read",
        description: "List the reading list, newest first: each entry's title and URL, whether \
            it has been read and when it was added.",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "max_results": {"type": "integer", "minimum": 1, "maximum": 100},
                },
            })
        },
    },
    Tool {
        name: "open_tab",
        group: "navigate",
        description: "Open this HTTP or HTTPS URL in a new background tab and say the URL the \
            tab loaded. The tab is closed and the call fails if the page is on another host. It \
            returns no page content.",
        schema: || {
            json!({
                "type": "object",
                "properties": {"url": {"type": "string"}},
                "required": ["url"],
            })
        },
    },
    Tool {
        name: "navigate_tab",
        group: "navigate",
        description: "Take the open tab at `url` to the HTTP or HTTPS address `to`, with the \
            person's cookies, and say the URL the tab loaded. The tab is left blank and the call \
            fails if the page is on another host than `to`. It returns no page content.",
        schema: || {
            json!({
                "type": "object",
                "properties": {"url": {"type": "string"}, "to": {"type": "string"}},
                "required": ["url", "to"],
            })
        },
    },
    Tool {
        name: "reload_tab",
        group: "navigate",
        description: "Reload the open tab at this HTTP or HTTPS URL and say the URL the tab \
            loaded. The tab is left blank and the call fails if the page is on another host. It \
            returns no page content.",
        schema: || {
            json!({
                "type": "object",
                "properties": {"url": {"type": "string"}},
                "required": ["url"],
            })
        },
    },
    Tool {
        name: "go_back",
        group: "navigate",
        description: "Move the open tab at this HTTP or HTTPS URL back one page in its history \
            and say the URL the tab loaded. The tab is left blank and the call fails if that page \
            is on another host than the URL. It returns no page content.",
        schema: || {
            json!({
                "type": "object",
                "properties": {"url": {"type": "string"}},
                "required": ["url"],
            })
        },
    },
    Tool {
        name: "go_forward",
        group: "navigate",
        description: "Move the open tab at this HTTP or HTTPS URL forward one page in its history \
            and say the URL the tab loaded. The tab is left blank and the call fails if that page \
            is on another host than the URL. It returns no page content.",
        schema: || {
            json!({
                "type": "object",
                "properties": {"url": {"type": "string"}},
                "required": ["url"],
            })
        },
    },
    Tool {
        name: "open_window",
        group: "navigate",
        description: "Open this HTTP or HTTPS URL in a new window that is not brought forward and \
            say the URL the window loaded. The window is closed and the call fails if the page is \
            on another host. It returns no page content.",
        schema: || {
            json!({
                "type": "object",
                "properties": {"url": {"type": "string"}},
                "required": ["url"],
            })
        },
    },
    Tool {
        name: "restore_closed",
        group: "navigate",
        description: "Reopen the recently closed tab that was at this HTTP or HTTPS URL, as \
            `list_recently_closed` shows it, and say the URL the tab loaded. The tab is closed \
            and the call fails if the page is on another host, and when no closed tab or more than \
            one was at the URL. A closed window is not restored. It returns no page content.",
        schema: || {
            json!({
                "type": "object",
                "properties": {"url": {"type": "string"}},
                "required": ["url"],
            })
        },
    },
    Tool {
        name: "search_web",
        group: "navigate",
        description: "Search the web for these words with the person's default search provider, \
            in a new background tab, and say the URL of the results page. The provider's address \
            is not shown to the person, so the page is not held to a host: the tab is closed and \
            the call fails if the page is not a web page or fails to load. It returns no page \
            content.",
        schema: || {
            json!({
                "type": "object",
                "properties": {"query": {"type": "string", "maxLength": 256}},
                "required": ["query"],
            })
        },
    },
    Tool {
        name: "wait_for_load",
        group: "navigate",
        description: "Wait, for at most 20 seconds, until the open tab at this URL has finished \
            loading, and say whether it loaded or the time ran out. It fails if the tab is closed \
            or goes to another URL meanwhile. It returns no page content.",
        schema: || {
            json!({
                "type": "object",
                "properties": {"url": {"type": "string"}},
                "required": ["url"],
            })
        },
    },
    Tool {
        name: "read_page_links",
        group: "page-read",
        description: "List the links in the open tab at this URL: the text and the HTTP or HTTPS \
            URL of each, for a person to choose from and provide in a later call.",
        schema: || {
            json!({
                "type": "object",
                "properties": {"url": {"type": "string"}},
                "required": ["url"],
            })
        },
    },
    Tool {
        name: "read_page_forms",
        group: "page-read",
        description: "List the forms in the open tab at this URL: where each submits and how, and \
            the name and type of each field. It does not read what a field holds.",
        schema: || {
            json!({
                "type": "object",
                "properties": {"url": {"type": "string"}},
                "required": ["url"],
            })
        },
    },
    Tool {
        name: "list_page_elements",
        group: "page-read",
        description: "Number the controls of the top document of the open tab at this URL that a \
            click, typing or a choice can act on, in document order: at most 40, each with its \
            role, its accessible name cut to 120 characters and the origin it goes to, and no \
            value that a field holds. An element that is hidden or disabled, and a password, \
            payment card or file field, is not listed. The result is text from the page. It \
            arrives as a reference that the model does not read: a person reads the list and \
            tells the model which number, or the model asks for the text with vet_content, \
            which may release it for the model to read. The number is then given as \
            element_number to click_element, \
            type_text or select_option for the same URL, within 5 minutes and only while the \
            page still shows that element. A later list replaces this one.",
        schema: || {
            json!({
                "type": "object",
                "properties": {"url": {"type": "string"}},
                "required": ["url"],
            })
        },
    },
    Tool {
        name: "screenshot_page",
        group: "page-read",
        description: "Take a picture of the open tab at this URL, as a JPEG, as its window shows \
            it. The tab has to be the visible tab of its window, and it is not brought forward; \
            focus_tab does that. The group has to be on all sites, because the browser lets an \
            extension capture a tab only with access to all sites. The result is a picture the \
            model does not see. It arrives as a reference, and the picture becomes available to \
            the model only through spawn_processor, which reads it for a question the model \
            gives, or through vet_content, which is the question to ask to read it. A frame of \
            another site inside the page is in the picture, and the blocked sites list names the \
            tab's own URL only.",
        schema: || {
            json!({
                "type": "object",
                "properties": {"url": {"type": "string"}},
                "required": ["url"],
            })
        },
    },
    Tool {
        name: "capture_screen",
        group: "screen",
        description: "Take a picture of what a person chooses to share in the browser's own \
            picker, as a JPEG: a screen or a window of any program. The browser shows the picker \
            for each call and nothing is captured until the person chooses, so the call waits up \
            to 60 seconds and fails if the picker is closed. A screen holds whatever is on it, \
            including what is not a page of Brave. The result is a picture the model does not \
            see. It arrives as a reference, and the picture becomes available to the model only \
            through spawn_processor, which reads it for a question the model gives, or through \
            vet_content, which is the question to ask to read it.",
        schema: || {
            json!({
                "type": "object",
                "properties": {},
            })
        },
    },
    Tool {
        name: "record_gif",
        group: "screen",
        description: "Record the open tab at this URL as an animated GIF: frames pictures of it, \
            at least 0.6 seconds apart, each scaled to at most 480 pixels on a side, in 256 \
            colours, without dithering. The tab has to be the visible tab of its window, and it \
            is not brought forward; focus_tab does that. A recording ends, with no result, if \
            the tab changes while it is taken. The group has to be on all sites, because the \
            browser lets an extension capture a tab only with access to all sites. The result is \
            a picture the model does not see. It arrives as a reference, and the picture \
            becomes available to the model only through spawn_processor, which reads it for a \
            question the model gives, or through vet_content, which is the question to ask to \
            read it. A frame of another site inside the page is in the picture, and the \
            blocked sites list names the tab's own URL only.",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "url": {"type": "string"},
                    "frames": {"type": "integer", "minimum": 2, "maximum": 20},
                },
                "required": ["url"],
            })
        },
    },
    Tool {
        name: "save_page_archive",
        group: "downloads",
        description: "Save the page of the open tab at this URL, with its pictures and frames, as \
            an MHTML file in the person's Downloads folder. The extension names the file for the \
            site, a file already there is not replaced, and an archive over 16 MiB is not saved. \
            The result names the file and says how the download stands. It holds nothing of the \
            page, and no tool reads the file; it is available to the model only if the person \
            gives it the file. It saves only a page at a site the person allowed in the \
            extension's options. A frame of another site inside the page is in the archive, and \
            the blocked sites list names the tab's own URL only.",
        schema: || {
            json!({
                "type": "object",
                "properties": {"url": {"type": "string"}},
                "required": ["url"],
            })
        },
    },
    Tool {
        name: "focus_tab",
        group: "tabs-manage",
        description: "Make the open tab at this URL the active tab of its window and bring the \
            window forward.",
        schema: || {
            json!({
                "type": "object",
                "properties": {"url": {"type": "string"}},
                "required": ["url"],
            })
        },
    },
    Tool {
        name: "pin_tab",
        group: "tabs-manage",
        description: "Pin the open tab at this URL, or unpin it when pinned is false.",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "url": {"type": "string"},
                    "pinned": {"type": "boolean"},
                },
                "required": ["url"],
            })
        },
    },
    Tool {
        name: "mute_tab",
        group: "tabs-manage",
        description: "Mute the open tab at this URL, or unmute it when muted is false.",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "url": {"type": "string"},
                    "muted": {"type": "boolean"},
                },
                "required": ["url"],
            })
        },
    },
    Tool {
        name: "move_tab",
        group: "tabs-manage",
        description: "Move the open tab at this URL to this position in its window, counting from \
            0, or to the end when the index is -1.",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "url": {"type": "string"},
                    "index": {"type": "integer", "minimum": -1},
                },
                "required": ["url", "index"],
            })
        },
    },
    Tool {
        name: "group_tabs",
        group: "tabs-manage",
        description: "Put the open tabs at these URLs, which are all in one window, into a new tab \
            group in that window, with this title if one is given.",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "urls": {
                        "type": "array",
                        "items": {"type": "string"},
                        "minItems": 1,
                        "maxItems": LIST_LIMIT,
                    },
                    "title": {"type": "string"},
                },
                "required": ["urls"],
            })
        },
    },
    Tool {
        name: "ungroup_tabs",
        group: "tabs-manage",
        description: "Take the open tabs at these URLs out of the tab groups they are in.",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "urls": {
                        "type": "array",
                        "items": {"type": "string"},
                        "minItems": 1,
                        "maxItems": LIST_LIMIT,
                    },
                },
                "required": ["urls"],
            })
        },
    },
    Tool {
        name: "update_tab_group",
        group: "tabs-manage",
        description: "Change the title, color or collapsed state of the tab group that the open \
            tab at this URL is in. At least one of them is given.",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "url": {"type": "string"},
                    "title": {"type": "string"},
                    "color": {
                        "type": "string",
                        "enum": [
                            "grey", "blue", "red", "yellow", "green", "pink", "purple", "cyan",
                            "orange",
                        ],
                    },
                    "collapsed": {"type": "boolean"},
                },
                "required": ["url"],
            })
        },
    },
    Tool {
        name: "discard_tab",
        group: "tabs-manage",
        description: "Unload the open tab at this URL from memory. It stays in the tab strip and \
            loads again when it is shown.",
        schema: || {
            json!({
                "type": "object",
                "properties": {"url": {"type": "string"}},
                "required": ["url"],
            })
        },
    },
    Tool {
        name: "duplicate_tab",
        group: "tabs-manage",
        description: "Open a copy of the open tab at this URL, which loads the same address \
            again. It returns nothing of the copy.",
        schema: || {
            json!({
                "type": "object",
                "properties": {"url": {"type": "string"}},
                "required": ["url"],
            })
        },
    },
    Tool {
        name: "set_zoom",
        group: "tabs-manage",
        description: "Set the zoom of the open tab at this URL, where 1 is 100% and 0 is the \
            browser's default for the page.",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "url": {"type": "string"},
                    "factor": {"type": "number", "minimum": 0, "maximum": 5},
                },
                "required": ["url", "factor"],
            })
        },
    },
    Tool {
        name: "focus_window",
        group: "tabs-manage",
        description: "Bring forward the window that holds the open tab at this URL, leaving the \
            tabs in it as they are.",
        schema: || {
            json!({
                "type": "object",
                "properties": {"url": {"type": "string"}},
                "required": ["url"],
            })
        },
    },
    Tool {
        name: "resize_window",
        group: "tabs-manage",
        description: "Set the size in pixels of the window that holds the open tab at this URL, \
            leaving its position and its tabs as they are. The reply says the size the browser \
            holds, which can differ from the one asked for.",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "url": {"type": "string"},
                    "width": {"type": "integer", "minimum": 1},
                    "height": {"type": "integer", "minimum": 1},
                },
                "required": ["url", "width", "height"],
            })
        },
    },
    Tool {
        name: "list_cookie_names",
        group: "cookies",
        description: "List the cookies Brave sends to this HTTP or HTTPS URL: each one's name, \
            domain, path, flags and expiry. It never returns or reads a cookie's value.",
        schema: || {
            json!({
                "type": "object",
                "properties": {"url": {"type": "string"}},
                "required": ["url"],
            })
        },
    },
    Tool {
        name: "delete_cookie",
        group: "cookies",
        description: "Delete the cookie with this name that Brave sends to this HTTP or HTTPS \
            URL. It fails and deletes nothing if no cookie or more than one has that name there. \
            It never reads or sets a cookie's value.",
        schema: || {
            json!({
                "type": "object",
                "properties": {"name": {"type": "string"}, "url": {"type": "string"}},
                "required": ["name", "url"],
            })
        },
    },
    Tool {
        name: "list_requests",
        group: "network-watch",
        description: "List the requests the open tab at this URL has made since it loaded its \
            page, in order: each request's URL, type and status. It returns no header, cookie or \
            body, and fails for a page loaded before the extension was watching.",
        schema: || {
            json!({
                "type": "object",
                "properties": {"url": {"type": "string"}},
                "required": ["url"],
            })
        },
    },
    Tool {
        name: "get_privacy_settings",
        group: "browser-info",
        description: "Say how each of Brave's privacy settings is set, such as safe browsing, do \
            not track and third-party cookies, and who controls it. It changes nothing.",
        schema: || json!({"type": "object", "properties": {}}),
    },
    Tool {
        name: "get_content_settings",
        group: "browser-info",
        description: "Say whether Brave allows, blocks or asks about each kind of content, such as \
            cookies, scripts, images, location, notifications, camera and microphone, at this \
            HTTP or HTTPS URL. It changes nothing.",
        schema: || {
            json!({
                "type": "object",
                "properties": {"url": {"type": "string"}},
                "required": ["url"],
            })
        },
    },
    Tool {
        name: "get_font_settings",
        group: "browser-info",
        description: "Say which font Brave uses for each generic family and its three font sizes. \
            It does not list the fonts installed.",
        schema: || json!({"type": "object", "properties": {}}),
    },
    Tool {
        name: "get_system_info",
        group: "browser-info",
        description: "Say what the computer has: its processor, memory, displays and storage \
            devices. It does not say how busy the processors are.",
        schema: || json!({"type": "object", "properties": {}}),
    },
    Tool {
        name: "get_idle_state",
        group: "browser-info",
        description: "Say whether the person is active, idle after this many seconds without \
            input (60 if none is given), or has locked the screen.",
        schema: || {
            json!({
                "type": "object",
                "properties": {"seconds": {"type": "integer", "minimum": 1}},
            })
        },
    },
    Tool {
        name: "get_access_flags",
        group: "browser-info",
        description: "Say whether the person allowed the extension to run on file: URLs and in \
            private windows.",
        schema: || json!({"type": "object", "properties": {}}),
    },
    Tool {
        name: "keep_awake",
        group: "browser-info",
        description: "Keep the computer from sleeping for this many minutes, at most 240, or its \
            display on as well when the level is display. A request made before is replaced. It \
            ends when the minutes pass, or at once when the minutes are 0.",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "minutes": {"type": "integer", "minimum": 0, "maximum": 240},
                    "level": {"type": "string", "enum": ["system", "display"]},
                },
                "required": ["minutes"],
            })
        },
    },
    Tool {
        name: "create_bookmark",
        group: "bookmarks-write",
        description: "Add a bookmark at this HTTP or HTTPS URL, with this title if one is given, \
            in the bookmark folder with this exact title or, if none is given, where Brave puts \
            new bookmarks. It fails if no folder, or more than one, has that title.",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "url": {"type": "string"},
                    "title": {"type": "string"},
                    "folder": {"type": "string"},
                },
                "required": ["url"],
            })
        },
    },
    Tool {
        name: "create_bookmark_folder",
        group: "bookmarks-write",
        description: "Add a bookmark folder with this title, inside the bookmark folder with the \
            exact title given as parent or, if none is given, where Brave puts new folders. It \
            fails if no folder, or more than one, has the parent's title.",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "title": {"type": "string"},
                    "parent": {"type": "string"},
                },
                "required": ["title"],
            })
        },
    },
    Tool {
        name: "move_bookmark",
        group: "bookmarks-write",
        description: "Move the bookmark at this exact URL to the end of the bookmark folder with \
            this exact title. It fails, and moves nothing, if no bookmark or more than one is at \
            the URL, or if no folder or more than one has the title.",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "url": {"type": "string"},
                    "folder": {"type": "string"},
                },
                "required": ["url", "folder"],
            })
        },
    },
    Tool {
        name: "rename_bookmark",
        group: "bookmarks-write",
        description: "Give the bookmark at this exact URL a new title. It fails, and changes \
            nothing, if no bookmark or more than one is at the URL.",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "url": {"type": "string"},
                    "title": {"type": "string"},
                },
                "required": ["url", "title"],
            })
        },
    },
    Tool {
        name: "add_reading_list_entry",
        group: "bookmarks-write",
        description: "Add an entry for this HTTP or HTTPS URL, with this title, to the reading \
            list, unread unless read is true. It fails if the list has an entry at the URL.",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "url": {"type": "string"},
                    "title": {"type": "string"},
                    "read": {"type": "boolean"},
                },
                "required": ["url", "title"],
            })
        },
    },
    Tool {
        name: "update_reading_list_entry",
        group: "bookmarks-write",
        description: "Change the title, or whether it has been read, of the reading list entry at \
            this exact URL. At least one of title and read is given. It fails if the list has no \
            entry at the URL.",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "url": {"type": "string"},
                    "title": {"type": "string"},
                    "read": {"type": "boolean"},
                },
                "required": ["url"],
            })
        },
    },
    Tool {
        name: "close_tab",
        group: "remove-data",
        description: "Close the open tab at this exact URL. What the page has not saved is lost. \
            It fails if more than one tab is at the URL.",
        schema: || {
            json!({
                "type": "object",
                "properties": {"url": {"type": "string"}},
                "required": ["url"],
            })
        },
    },
    Tool {
        name: "close_window",
        group: "remove-data",
        description: "Close the window that holds the open tab at this exact URL, with every tab \
            in it. What the pages have not saved is lost. It fails if more than one tab is at \
            the URL, and it does not close a window that has a tab at a blocked site.",
        schema: || {
            json!({
                "type": "object",
                "properties": {"url": {"type": "string"}},
                "required": ["url"],
            })
        },
    },
    Tool {
        name: "delete_history_entry",
        group: "remove-data",
        description: "Delete the history entry at this exact URL, with every visit to it. It \
            fails if the history has no entry at the URL.",
        schema: || {
            json!({
                "type": "object",
                "properties": {"url": {"type": "string"}},
                "required": ["url"],
            })
        },
    },
    Tool {
        name: "delete_history_range",
        group: "remove-data",
        description: "Delete the history between two moments, both given as an ISO 8601 date and \
            time with its offset, such as 2026-10-01T00:00:00Z. A page that was also visited \
            outside the range is not removed. It fails if start is not before end.",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "start": {"type": "string", "format": "date-time"},
                    "end": {"type": "string", "format": "date-time"},
                },
                "required": ["start", "end"],
            })
        },
    },
    Tool {
        name: "clear_browsing_data",
        group: "remove-data",
        description: "Clear the kinds of browsing data named, for every site, that built up since \
            a moment given as an ISO 8601 date and time with its offset, such as \
            2026-10-01T00:00:00Z. Both the kinds and the moment are required.",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "since": {"type": "string", "format": "date-time"},
                    "types": {
                        "type": "array",
                        "minItems": 1,
                        "maxItems": LIST_LIMIT,
                        "items": {
                            "type": "string",
                            "enum": [
                                "cache",
                                "cacheStorage",
                                "cookies",
                                "downloads",
                                "fileSystems",
                                "formData",
                                "history",
                                "indexedDB",
                                "localStorage",
                                "serviceWorkers",
                            ],
                        },
                    },
                },
                "required": ["since", "types"],
            })
        },
    },
    Tool {
        name: "remove_bookmark",
        group: "remove-data",
        description: "Delete the bookmark at this exact URL. It fails if no bookmark, or more \
            than one, is at the URL.",
        schema: || {
            json!({
                "type": "object",
                "properties": {"url": {"type": "string"}},
                "required": ["url"],
            })
        },
    },
    Tool {
        name: "remove_bookmark_folder",
        group: "remove-data",
        description: "Delete the bookmark folder with this exact title, only if it is empty. It \
            fails if no folder, or more than one, has the title, and it leaves a folder with \
            anything in it as it is.",
        schema: || {
            json!({
                "type": "object",
                "properties": {"folder": {"type": "string"}},
                "required": ["folder"],
            })
        },
    },
    Tool {
        name: "remove_reading_list_entry",
        group: "remove-data",
        description: "Remove the reading list entry at this exact URL. It fails if the reading \
            list has no entry at the URL.",
        schema: || {
            json!({
                "type": "object",
                "properties": {"url": {"type": "string"}},
                "required": ["url"],
            })
        },
    },
    Tool {
        name: "delete_downloaded_file",
        group: "remove-data",
        description: "Delete the file of a finished download from the disk. Name the download by \
            its exact filename, the absolute path the browser holds, or by the exact URL it was \
            started from: give one of the two. The download stays in the list with its file \
            gone. It fails if no download, or more than one with a file on the disk, has the \
            name, if the file is already gone, and if the download is not finished.",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "filename": {"type": "string"},
                    "url": {"type": "string"},
                },
            })
        },
    },
    Tool {
        name: "download_file",
        group: "downloads",
        description: "Download the file at this exact HTTP or HTTPS URL into the person's \
            Downloads folder, with their cookies for that site. The browser names the file, a \
            file already there is not replaced, and the download is cancelled if it goes to \
            another host. Nothing in the file is returned, and no tool reads it.",
        schema: || {
            json!({
                "type": "object",
                "properties": {"url": {"type": "string"}},
                "required": ["url"],
            })
        },
    },
    Tool {
        name: "list_downloads",
        group: "downloads",
        description: "List the downloads Brave holds, newest first: the absolute path the browser \
            holds for each and the URL it started from, which name it in the other download \
            tools, with its state, size and whether its file is still on the disk.",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "max_results": {"type": "integer", "minimum": 1, "maximum": 100},
                },
            })
        },
    },
    Tool {
        name: "pause_download",
        group: "downloads",
        description: "Pause the download in progress with this exact filename, the absolute \
            path the browser holds, or with this exact URL it started from: give one of the \
            two. It fails if no download, or more than one that can be paused, has the name.",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "filename": {"type": "string"},
                    "url": {"type": "string"},
                },
            })
        },
    },
    Tool {
        name: "resume_download",
        group: "downloads",
        description: "Resume the paused or interrupted download with this exact filename, the \
            absolute path the browser holds, or with this exact URL it started from: give one \
            of the two. It fails if no download, or more than one that can be resumed, has the \
            name.",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "filename": {"type": "string"},
                    "url": {"type": "string"},
                },
            })
        },
    },
    Tool {
        name: "cancel_download",
        group: "downloads",
        description: "Cancel the download in progress with this exact filename, the absolute \
            path the browser holds, or with this exact URL it started from: give one of the \
            two. A finished download is left as it is and its file is not deleted. It fails if \
            no download, or more than one in progress, has the name.",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "filename": {"type": "string"},
                    "url": {"type": "string"},
                },
            })
        },
    },
    Tool {
        name: "show_download",
        group: "downloads",
        description: "Show the file of the download with this exact filename, the absolute path \
            the browser holds, or with this exact URL it started from, in its folder in the \
            computer's file manager: give one of the two. The file is not opened. It fails if no \
            download, or more than one with a file on the disk, has the name.",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "filename": {"type": "string"},
                    "url": {"type": "string"},
                },
            })
        },
    },
    Tool {
        name: "run_script",
        group: "scripts",
        description: "Run this code in the top document of the open tab at this exact URL, as the \
            person who is signed in to the page. Each time it asks the person in a window \
            in the browser, which shows the site, the address and all of the code, line by line \
            with the characters they cannot see written out, and the code runs only if they \
            press Allow once there; a denial, a closed window and no answer within 60 seconds \
            run nothing. Nothing limits the code except that the person reads it: it can read \
            everything on the page, including what is hidden, click and submit the page's forms, \
            and send what it read to another address by loading a frame or going to a link. The \
            code starts once. Timers, listeners and observers it sets keep running in the page \
            until the page is closed or reloaded, and each can read and send without asking \
            again. The code is at most 2,000 characters, and a longer one is refused and never \
            cut. It runs as a script and not as the body of a function, in a world that no other \
            call shares and that keeps no variable for the next call. Its answer is the value \
            of its last expression, or of the promise it ends with, so a top-level `return` or \
            `await` is a syntax error whose answer is null: wrap the code in an async function \
            and call it. The call fails if the script has not answered in 15 seconds or the tab \
            is no longer at the URL. It is refused, with nothing run, when the browser has not \
            applied the policy that stops the script's own requests in the page, which Brave \
            stops doing after ten calls on one page: the person reloads the page. It fails \
            until the person has turned on Allow User Scripts for the BraveBot extension in the \
            browser. The result is the code's answer as text: it is page content that the model \
            cannot read, whoever wrote the code, and it arrives as a reference that becomes \
            readable only through spawn_processor, which reads it for a question the model \
            gives, or through vet_content, which is the question to ask to read it.",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "url": {"type": "string", "maxLength": MAX_SCRIPT_URL},
                    "code": {"type": "string", "maxLength": MAX_SCRIPT_CODE},
                },
                "required": ["url", "code"],
            })
        },
    },
    Tool {
        name: "click_element",
        group: "input",
        description: "Click the one element with this role and this accessible name in the top \
            document of the open tab at this exact URL, with the left or right button, once or \
            twice. The accessible name is what the page labels the element with: its aria-label, \
            its label or its text. Give element_number in place of the role and the name to click \
            the element that has that number in the last list_page_elements made for this URL; \
            give one form and not both. Each click asks the person in a window in the browser, \
            which shows the element, and is made only if they allow it. It fails if no element, \
            or more than one, has the role and name, if the page no longer shows the element \
            that has the number, or if the element is hidden, covered by another or disabled. A \
            password field, a payment card field and a file chooser are refused. Frames are not \
            searched. The click is synthetic: the page can tell it from a person's. The reply \
            says only that it was done.",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "url": {"type": "string"},
                    "role": {"type": "string", "enum": ROLES},
                    "name": {"type": "string", "maxLength": 200},
                    "element_number": {"type": "integer", "minimum": 1, "maximum": MAX_LISTED},
                    "button": {"type": "string", "enum": ["left", "right"]},
                    "count": {"type": "integer", "minimum": 1, "maximum": 2},
                },
                "required": ["url"],
            })
        },
    },
    Tool {
        name: "type_text",
        group: "input",
        description: "Replace what the one text field with this role and this accessible name \
            holds, in the top document of the open tab at this exact URL, with this text. Give \
            element_number in place of the role and the name for the field that has that number \
            in the last list_page_elements made for this URL; give one form and not both. Each \
            time it asks the person in a window in the browser, which shows the field and the \
            text, and the text is put in only if they allow it. It fails as click_element does, \
            and for an element that is not a text field. A password field, a payment card field \
            and a file chooser are refused. The input is synthetic: the page can tell it from \
            typing. The reply says only that it was done.",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "url": {"type": "string"},
                    "role": {"type": "string", "enum": ROLES},
                    "name": {"type": "string", "maxLength": 200},
                    "element_number": {"type": "integer", "minimum": 1, "maximum": MAX_LISTED},
                    "text": {"type": "string", "minLength": 1, "maxLength": 2000},
                },
                "required": ["url", "text"],
            })
        },
    },
    Tool {
        name: "select_option",
        group: "input",
        description: "Choose the option with this text in the one native select with this role \
            and this accessible name, in the top document of the open tab at this exact URL. \
            Give element_number in place of the role and the name for the select that has that \
            number in the last list_page_elements made for this URL; give one form and not \
            both. Each time it asks the person in a window in the browser, which shows the \
            select, and the option is chosen only if they allow it. It fails as click_element \
            does, and where the element is not a native select or no option, or more than one, \
            has the text. A password field, a payment card field and a file chooser are \
            refused. The choice is synthetic: the page can tell it from a person's. The reply \
            says only that it was done.",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "url": {"type": "string"},
                    "role": {"type": "string", "enum": ROLES},
                    "name": {"type": "string", "maxLength": 200},
                    "element_number": {"type": "integer", "minimum": 1, "maximum": MAX_LISTED},
                    "option": {"type": "string", "maxLength": 200},
                },
                "required": ["url", "option"],
            })
        },
    },
    Tool {
        name: "press_key",
        group: "input",
        description: "Press one key, alone, on the element that has the focus in the top \
            document of the open tab at this exact URL. Each press asks the person in a window \
            in the browser, which shows the element, and the key is sent only if they allow it. \
            It is refused where the focus is in a password field, a payment card field or a file \
            chooser. The key press is synthetic: the page can tell it from a person's, and the \
            browser does not do what the key would do by itself, so it reaches only a page that \
            handles the key in its own script. The reply says only that it was done.",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "url": {"type": "string"},
                    "key": {"type": "string", "enum": KEYS},
                },
                "required": ["url", "key"],
            })
        },
    },
    Tool {
        name: "scroll_page",
        group: "input",
        description: "Scroll the top document of the open tab at this exact URL up or down by \
            this many screens, or to its top or bottom. No element is acted on and no window \
            asks the person. The reply says only that it was done.",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "url": {"type": "string"},
                    "direction": {"type": "string", "enum": ["up", "down", "top", "bottom"]},
                    "pages": {"type": "integer", "minimum": 1, "maximum": 5},
                },
                "required": ["url", "direction"],
            })
        },
    },
    Tool {
        name: "wait_for_element",
        group: "input",
        description: "Wait until the top document of the open tab at this exact URL shows \
            exactly one element with this role and this accessible name, for at most this many \
            seconds. It acts on nothing and no window asks the person. It fails if none appears \
            in time, and the reply says only that one was found.",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "url": {"type": "string"},
                    "role": {"type": "string", "enum": ROLES},
                    "name": {"type": "string", "maxLength": 200},
                    "seconds": {"type": "integer", "minimum": 1, "maximum": 30},
                },
                "required": ["url", "role", "name"],
            })
        },
    },
    Tool {
        name: "click_element_trusted",
        group: "input-trusted",
        description: "Click the one element with this role and this accessible name in the top \
            document of the open tab at this exact URL, with the left or right button, once or \
            twice, using real mouse input that the page cannot tell from a person's. The \
            BraveBot Trusted Input extension does it, and each click asks the person in a window \
            of that extension, which shows the element, and is made only if they allow it. It \
            fails if that extension is not installed or is turned off, if no element, or more \
            than one, has the role and name, or if the element is hidden, covered by another or \
            disabled. A password field, a payment card field and a file chooser are refused. \
            Frames are not searched. The reply says only that it was done.",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "url": {"type": "string"},
                    "role": {"type": "string", "enum": ROLES},
                    "name": {"type": "string", "maxLength": 200},
                    "button": {"type": "string", "enum": BUTTONS},
                    "count": {"type": "integer", "minimum": 1, "maximum": 2},
                },
                "required": ["url", "role", "name"],
            })
        },
    },
    Tool {
        name: "type_text_trusted",
        group: "input-trusted",
        description: "Replace what the one text field with this role and this accessible name \
            holds, in the top document of the open tab at this exact URL, with this text, typed \
            as real keyboard input that the page cannot tell from a person's. The BraveBot \
            Trusted Input extension does it, and each time it asks the person in a window of \
            that extension, which shows the field and the text, and the text is put in only if \
            they allow it. It fails as click_element_trusted does, and for an element that is \
            not a text field. A password field, a payment card field and a file chooser are \
            refused. The reply says only that it was done.",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "url": {"type": "string"},
                    "role": {"type": "string", "enum": ROLES},
                    "name": {"type": "string", "maxLength": 200},
                    "text": {"type": "string", "minLength": 1, "maxLength": 2000},
                },
                "required": ["url", "role", "name", "text"],
            })
        },
    },
    Tool {
        name: "press_key_trusted",
        group: "input-trusted",
        description: "Press one key, alone, on the element that has the focus in the top \
            document of the open tab at this exact URL, as a real key press that the browser \
            acts on. The BraveBot Trusted Input extension does it, and each press asks the \
            person in a window of that extension, which shows the element, and the key is sent \
            only if they allow it. It fails if no element has the focus, and it is refused where \
            the focus is in a password field, a payment card field or a file chooser. The reply \
            says only that it was done.",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "url": {"type": "string"},
                    "key": {"type": "string", "enum": KEYS},
                },
                "required": ["url", "key"],
            })
        },
    },
    Tool {
        name: "hover_element",
        group: "input-trusted",
        description: "Move the pointer over the one element with this role and this accessible \
            name in the top document of the open tab at this exact URL, as a real pointer that \
            the page cannot tell from a person's, so that a menu or a tip that opens on hover \
            opens. The BraveBot Trusted Input extension does it, and each time it asks the \
            person in a window of that extension, which shows the element, and the pointer is \
            moved only if they allow it. It fails as click_element_trusted does. The reply \
            says only that it was done.",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "url": {"type": "string"},
                    "role": {"type": "string", "enum": ROLES},
                    "name": {"type": "string", "maxLength": 200},
                },
                "required": ["url", "role", "name"],
            })
        },
    },
    Tool {
        name: "drag_element",
        group: "input-trusted",
        description: "Press on the one element with this role and this accessible name in the \
            top document of the open tab at this exact URL, move to the one element with the \
            second role and name (onto_role and onto_name) in eight steps and release there, \
            with real mouse input that the page cannot tell from a person's. Both elements are \
            named, and neither a position nor a distance is. The BraveBot Trusted Input \
            extension does it, and each drag asks the person in a window of that extension, \
            which shows both elements, and is made only if they allow it. It fails as \
            click_element_trusted does for either element. The reply says only that it was done.",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "url": {"type": "string"},
                    "role": {"type": "string", "enum": ROLES},
                    "name": {"type": "string", "maxLength": 200},
                    "onto_role": {"type": "string", "enum": ROLES},
                    "onto_name": {"type": "string", "maxLength": 200},
                },
                "required": ["url", "role", "name", "onto_role", "onto_name"],
            })
        },
    },
    Tool {
        name: "screenshot_full_page",
        group: "capture-trusted",
        description: "Take a picture of the whole top document of the open tab at this exact URL, \
            as a JPEG, from the top of the page to the bottom and from the left to the right, \
            including the parts out of view and the parts nobody can see. The tab can be a \
            background tab. The picture is at most 2048 pixels on its long side and 8 MiB; a \
            larger one is refused, never cut. The BraveBot Trusted Input extension takes it, and \
            each time it asks the person in a window of that extension, which shows the site, the \
            address and the size of the page, and takes the picture only if they allow it. A \
            frame is in the picture as the page draws it. It fails as click_element_trusted does \
            for a page it cannot find. The result is a picture the model does not see. It arrives \
            as a reference, and the picture becomes available to the model only through \
            spawn_processor, which reads it for a question the model gives, or through \
            vet_content, which is the question to ask to read it. A frame of another site \
            inside the page is in the picture, and the blocked sites list names the tab's own URL \
            only.",
        schema: || {
            json!({
                "type": "object",
                "properties": {"url": {"type": "string"}},
                "required": ["url"],
            })
        },
    },
    Tool {
        name: "read_console",
        group: "console-trusted",
        description: "Read the console of the open tab at this exact URL: what the page's scripts \
            wrote with console.log and the others, the errors they threw and what the browser \
            reported about the page, such as a request that failed. The BraveBot Trusted Input \
            extension reads it, and each time it asks the person in a window of that extension, \
            which shows the site and the address, and reads it only if they allow it. It returns \
            the newest entries, at most max_entries (1 to 200, 100 when left out), of the levels \
            asked for: all, warnings_and_errors or errors. The result is text, one JSON object on \
            each line: a first line with the origin, how many entries follow and how many older \
            ones were left out, then for each entry its level, source, text (cut at 500 \
            characters, and cut says whether it was), url and line. A script that threw an error \
            appears as Uncaught with its line and without the error's message, and a frame that \
            Brave runs in another process is not read. It fails as click_element_trusted does for \
            a page it cannot find. The result is page text that the model cannot read directly, \
            whoever wrote the page: it arrives as a reference, and becomes readable only through \
            spawn_processor, which reads it for a question the model gives, or through \
            vet_content, which is the question to ask to read it.",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "url": {"type": "string"},
                    "levels": {"type": "string", "enum": CONSOLE_LEVELS},
                    "max_entries": {"type": "integer", "minimum": 1, "maximum": MAX_CONSOLE_ENTRIES},
                },
                "required": ["url"],
            })
        },
    },
    Tool {
        name: "choose_file",
        group: "upload-trusted",
        description: "Choose a file from the uploads directory on the one file input with this \
            name in the top document of the open tab at this exact URL, so that the page \
            receives the file as if a person had chosen it in the browser's file picker. The \
            name is the input's accessible name or, for an input the page hides, which many \
            pages do behind a label or a button, the name the page's own label, aria-label or \
            title gives it, or its id or name where nothing else names it. The path names the \
            file by the name of a file in the uploads directory, or by its absolute path inside \
            it; a file that is a link out of it, a directory, a file with more than one hard \
            link, a file over 10 MiB and a file whose name looks like a credential, a key or a \
            secret (.env, *.pem, *.key, id_rsa*, credentials*, .netrc, .ssh and .aws) are \
            refused here, before the browser is asked, and nothing is sent. The role is \
            optional and, when given, is \
            the file input's role in the accessibility tree. The BraveBot Trusted Input \
            extension does it, and each time it asks the person in a window of that extension, \
            which shows the site, the address, the file input, and the name, size and full path \
            of the file, and says that the browser reads the file and gives it to the page; the \
            file is chosen only if they allow it. Choosing a file sends nothing by itself, but \
            a page whose script sends the file when it is chosen receives it at once. It fails \
            as click_element_trusted does for a file input it cannot find. The reply says only \
            that it was done.",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "url": {"type": "string"},
                    "role": {"type": "string", "enum": ROLES},
                    "name": {"type": "string", "maxLength": 200},
                    "path": {"type": "string", "maxLength": MAX_PATH},
                },
                "required": ["url", "name", "path"],
            })
        },
    },
    Tool {
        name: "take_held",
        group: "held",
        description: "Return what the person held out in Brave with the BraveBot entries of its \
            context menu, oldest first, and forget it. Each item is a selection of text, the \
            address of a link or the address of a tab, with the origin of the page it was held on. \
            At most 10 items are kept, for 30 minutes, and a selection is cut at 20,000 \
            characters. It takes no argument and names no page. The result is page text that the \
            model cannot read directly, whoever selected it: it arrives as a reference, and \
            becomes readable only through spawn_processor, which reads it for a question the model \
            gives, or through vet_content, which is the question to ask to read it.",
        schema: || json!({"type": "object", "properties": {}}),
    },
];

/// The group of this id, if it is one of [`GROUPS`].
pub fn group(id: &str) -> Option<&'static str> {
    ids().find(|group| *group == id)
}

/// Whether the declaration of the server of the group of this id says every call is asked about.
/// False for an id that is not a group.
pub fn asks_every_call(id: &str) -> bool {
    GROUPS
        .iter()
        .any(|group| group.id == id && group.ask_every_call)
}

/// The tools a server started for no group offers: these seven. A person who accepted that list
/// accepted those tools, so a tool outside it is offered by the server of its group alone.
const UNGROUPED: &[&str] = &[
    "get_platform_info",
    "list_tabs",
    "list_frames",
    "read_page",
    "search_history",
    "search_bookmarks",
    "open_tab",
];

/// Whether the server started for `group` makes calls the extension asks the person about in the
/// browser first. False where it is started for none, and for an id that is not a group.
pub fn asks_in_browser(group: Option<&str>) -> bool {
    group.is_some_and(|id| {
        GROUPS
            .iter()
            .any(|group| group.id == id && group.asks_in_browser)
    })
}

/// The tools a server offers: those in `group`, or the seven of [`UNGROUPED`] where it is started
/// for none.
pub fn offered<'a>(group: Option<&'a str>) -> impl Iterator<Item = &'static Tool> + 'a {
    TOOLS.iter().filter(move |tool| match group {
        Some(group) => tool.group == group,
        None => UNGROUPED.contains(&tool.name),
    })
}

/// The tool of this name that a server started for `group` offers, if there is one.
pub fn named(name: &str, group: Option<&str>) -> Option<&'static Tool> {
    offered(group).find(|tool| tool.name == name)
}

/// The whole list a server started for `group` offers, as the `tools/list` result.
pub fn list(group: Option<&str>) -> Value {
    json!({"tools": offered(group).map(Tool::listed).collect::<Vec<_>>()})
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A name is looked up exactly, so a near miss calls nothing.
    #[test]
    fn a_tool_is_found_by_its_exact_name() {
        let found = |name| named(name, None).map(|tool| tool.name);
        assert_eq!(found("list_tabs"), Some("list_tabs"));
        assert!(found("list_tab").is_none());
        assert!(found("List_Tabs").is_none());
    }

    /// A group is looked up exactly, so a near miss starts a server for none: an id that matched
    /// loosely would offer a person a group they did not name.
    #[test]
    fn a_group_is_found_by_its_exact_id() {
        assert_eq!(group("tabs-read"), Some("tabs-read"));
        for near in [
            "",
            "tabs_read",
            "Tabs-Read",
            "tabs-read ",
            "tabs",
            "brave-tabs-read",
        ] {
            assert_eq!(group(near), None, "{near:?}");
        }
        for id in ids() {
            assert_eq!(group(id), Some(id));
        }
    }

    /// The groups whose declaration asks about every call are `cookies`, `remove-data`, `scripts`,
    /// `input`, `input-trusted`, `capture-trusted`, `console-trusted`, `upload-trusted` and
    /// `screen`, and a word that is not a group asks about nothing, so an id that matched loosely
    /// would not carry the flag of another group.
    #[test]
    fn only_cookies_remove_data_scripts_the_input_groups_capture_trusted_console_trusted_upload_trusted_and_screen_are_asked_about_every_call()
     {
        let asking: Vec<&str> = GROUPS
            .iter()
            .filter(|group| group.ask_every_call)
            .map(|group| group.id)
            .collect();
        assert_eq!(
            asking,
            [
                "cookies",
                "remove-data",
                "scripts",
                "input",
                "input-trusted",
                "capture-trusted",
                "console-trusted",
                "upload-trusted",
                "screen"
            ]
        );
        for id in ids() {
            assert_eq!(asks_every_call(id), asking.contains(&id), "{id}");
        }
        for near in [
            "",
            "Cookies",
            "cookies ",
            "brave-cookies",
            "remove_data",
            "Scripts",
            "scripts ",
            "brave-scripts",
            "script",
            "Input",
            "input_trusted",
            "input-trusted ",
            "brave-input-trusted",
            "Screen",
            "screen ",
            "brave-screen",
            "capture_trusted",
            "Capture-Trusted",
            "brave-capture-trusted",
            "console_trusted",
            "Console-Trusted",
            "brave-console-trusted",
            "upload_trusted",
            "Upload-Trusted",
            "brave-upload-trusted",
        ] {
            assert!(!asks_every_call(near), "{near:?}");
        }
    }

    /// `scripts`, `input`, `input-trusted`, `capture-trusted`, `console-trusted`, `upload-trusted`
    /// and `screen` have the person answer in a window of the browser, which is what gives their
    /// servers the longer wait, and all seven are also asked about every call. A server started
    /// for none, for a word that is not a group or for a group that is only asked about every
    /// call waits as before.
    #[test]
    fn only_scripts_and_the_input_groups_capture_trusted_console_trusted_upload_trusted_and_screen_ask_the_person_in_the_browser_and_are_asked_about_every_call()
     {
        let asking: Vec<&str> = GROUPS
            .iter()
            .filter(|group| group.asks_in_browser)
            .map(|group| group.id)
            .collect();
        assert_eq!(
            asking,
            [
                "scripts",
                "input",
                "input-trusted",
                "capture-trusted",
                "console-trusted",
                "upload-trusted",
                "screen"
            ]
        );
        for group in GROUPS {
            assert!(
                group.ask_every_call || !group.asks_in_browser,
                "{} asks in the browser and not every call",
                group.id
            );
            assert_eq!(
                asks_in_browser(Some(group.id)),
                [
                    "scripts",
                    "input",
                    "input-trusted",
                    "capture-trusted",
                    "console-trusted",
                    "upload-trusted",
                    "screen"
                ]
                .contains(&group.id)
            );
        }
        assert!(!asks_in_browser(None));
        for near in [
            "",
            "Scripts",
            "scripts ",
            "brave-scripts",
            "script",
            "Input",
            "input ",
            "brave-input",
            "inputs",
            "input_trusted",
            "Input-Trusted",
            "brave-input-trusted",
            "Screen",
            "screens",
            "capture_trusted",
            "Capture-Trusted",
            "brave-capture-trusted",
            "console_trusted",
            "Console-Trusted",
            "brave-console-trusted",
            "upload_trusted",
            "Upload-Trusted",
            "brave-upload-trusted",
        ] {
            assert!(!asks_in_browser(Some(near)), "{near:?}");
        }
    }

    /// A tool in two groups would be offered, and so approved, under either, and a tool in a group
    /// that is not listed could not be offered by any server started for a group.
    #[test]
    fn every_tool_is_in_one_listed_group_and_a_group_is_listed_once() {
        let mut sorted: Vec<&str> = ids().collect();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), GROUPS.len());
        for tool in TOOLS {
            let offering: Vec<&str> = ids()
                .filter(|group| offered(Some(group)).any(|offered| offered.name == tool.name))
                .collect();
            assert_eq!(offering, [tool.group], "{}", tool.name);
        }
    }

    /// A server started for a group offers that group's tools in the order of the whole list, and
    /// a group with no tool yet offers none.
    #[test]
    fn a_group_offers_its_own_tools_and_no_others() {
        let names = |group| -> Vec<&str> { offered(Some(group)).map(|tool| tool.name).collect() };
        assert_eq!(names("platform"), ["get_platform_info"]);
        assert_eq!(
            names("tabs-read"),
            [
                "list_tabs",
                "list_frames",
                "list_windows",
                "list_tab_groups",
                "list_recently_closed"
            ]
        );
        assert_eq!(
            names("page-read"),
            [
                "read_page",
                "read_page_links",
                "read_page_forms",
                "list_page_elements",
                "screenshot_page"
            ]
        );
        assert_eq!(
            names("tabs-manage"),
            [
                "focus_tab",
                "pin_tab",
                "mute_tab",
                "move_tab",
                "group_tabs",
                "ungroup_tabs",
                "update_tab_group",
                "discard_tab",
                "duplicate_tab",
                "set_zoom",
                "focus_window",
                "resize_window",
            ]
        );
        assert_eq!(
            names("bookmarks-write"),
            [
                "create_bookmark",
                "create_bookmark_folder",
                "move_bookmark",
                "rename_bookmark",
                "add_reading_list_entry",
                "update_reading_list_entry",
            ]
        );
        assert_eq!(
            names("history-read"),
            [
                "search_history",
                "get_visits",
                "list_top_sites",
                "list_synced_tabs"
            ]
        );
        assert_eq!(
            names("bookmarks-read"),
            [
                "search_bookmarks",
                "list_bookmark_folder",
                "list_recent_bookmarks",
                "list_reading_list"
            ]
        );
        assert_eq!(names("network-watch"), ["list_requests"]);
        assert_eq!(
            names("browser-info"),
            [
                "get_privacy_settings",
                "get_content_settings",
                "get_font_settings",
                "get_system_info",
                "get_idle_state",
                "get_access_flags",
                "keep_awake"
            ]
        );
        assert_eq!(
            names("navigate"),
            [
                "open_tab",
                "navigate_tab",
                "reload_tab",
                "go_back",
                "go_forward",
                "open_window",
                "restore_closed",
                "search_web",
                "wait_for_load",
            ]
        );
        assert_eq!(names("cookies"), ["list_cookie_names", "delete_cookie"]);
        assert_eq!(
            names("remove-data"),
            [
                "close_tab",
                "close_window",
                "delete_history_entry",
                "delete_history_range",
                "clear_browsing_data",
                "remove_bookmark",
                "remove_bookmark_folder",
                "remove_reading_list_entry",
                "delete_downloaded_file",
            ]
        );
        assert_eq!(
            names("downloads"),
            [
                "save_page_archive",
                "download_file",
                "list_downloads",
                "pause_download",
                "resume_download",
                "cancel_download",
                "show_download",
            ]
        );
        assert_eq!(
            names("input"),
            [
                "click_element",
                "type_text",
                "select_option",
                "press_key",
                "scroll_page",
                "wait_for_element",
            ]
        );
        assert_eq!(
            names("input-trusted"),
            [
                "click_element_trusted",
                "type_text_trusted",
                "press_key_trusted",
                "hover_element",
                "drag_element",
            ]
        );
        assert_eq!(names("capture-trusted"), ["screenshot_full_page"]);
        assert_eq!(names("console-trusted"), ["read_console"]);
        assert_eq!(names("held"), ["take_held"]);
        assert_eq!(names("scripts"), ["run_script"]);
        assert_eq!(names("screen"), ["capture_screen", "record_gif"]);

        assert!(named("read_page", Some("tabs-read")).is_none());
        assert!(named("get_platform_info", Some("tabs-read")).is_none());
        assert!(named("list_tabs", Some("tabs-read")).is_some());
    }

    /// With no group the list is the seven tools of BROWSER-10, so its tool list does not change
    /// under a person who accepted it, whatever tools a group has.
    #[test]
    fn with_no_group_the_seven_original_tools_are_offered_in_order() {
        let names: Vec<&str> = offered(None).map(|tool| tool.name).collect();
        assert_eq!(
            names,
            [
                "get_platform_info",
                "list_tabs",
                "list_frames",
                "read_page",
                "search_history",
                "search_bookmarks",
                "open_tab",
            ]
        );
        assert_eq!(list(None)["tools"].as_array().unwrap().len(), 7);
    }

    /// A tool of a group beyond the seven is offered by that group's server, which a person
    /// declares and approves on its own, and not by a server declared with no group.
    #[test]
    fn a_tool_added_to_a_group_is_not_offered_with_no_group() {
        for (name, group) in [
            ("list_windows", "tabs-read"),
            ("list_tab_groups", "tabs-read"),
            ("list_recently_closed", "tabs-read"),
            ("get_visits", "history-read"),
            ("list_top_sites", "history-read"),
            ("list_synced_tabs", "history-read"),
            ("read_page_links", "page-read"),
            ("read_page_forms", "page-read"),
            ("list_page_elements", "page-read"),
            ("screenshot_page", "page-read"),
            ("focus_tab", "tabs-manage"),
            ("pin_tab", "tabs-manage"),
            ("mute_tab", "tabs-manage"),
            ("move_tab", "tabs-manage"),
            ("group_tabs", "tabs-manage"),
            ("ungroup_tabs", "tabs-manage"),
            ("update_tab_group", "tabs-manage"),
            ("discard_tab", "tabs-manage"),
            ("duplicate_tab", "tabs-manage"),
            ("set_zoom", "tabs-manage"),
            ("focus_window", "tabs-manage"),
            ("resize_window", "tabs-manage"),
            ("navigate_tab", "navigate"),
            ("reload_tab", "navigate"),
            ("go_back", "navigate"),
            ("go_forward", "navigate"),
            ("open_window", "navigate"),
            ("restore_closed", "navigate"),
            ("search_web", "navigate"),
            ("wait_for_load", "navigate"),
            ("list_cookie_names", "cookies"),
            ("delete_cookie", "cookies"),
            ("list_bookmark_folder", "bookmarks-read"),
            ("list_recent_bookmarks", "bookmarks-read"),
            ("list_reading_list", "bookmarks-read"),
            ("list_requests", "network-watch"),
            ("get_privacy_settings", "browser-info"),
            ("get_content_settings", "browser-info"),
            ("get_font_settings", "browser-info"),
            ("get_system_info", "browser-info"),
            ("get_idle_state", "browser-info"),
            ("get_access_flags", "browser-info"),
            ("keep_awake", "browser-info"),
            ("create_bookmark", "bookmarks-write"),
            ("create_bookmark_folder", "bookmarks-write"),
            ("move_bookmark", "bookmarks-write"),
            ("rename_bookmark", "bookmarks-write"),
            ("add_reading_list_entry", "bookmarks-write"),
            ("update_reading_list_entry", "bookmarks-write"),
            ("close_tab", "remove-data"),
            ("close_window", "remove-data"),
            ("delete_history_entry", "remove-data"),
            ("delete_history_range", "remove-data"),
            ("clear_browsing_data", "remove-data"),
            ("remove_bookmark", "remove-data"),
            ("remove_bookmark_folder", "remove-data"),
            ("remove_reading_list_entry", "remove-data"),
            ("delete_downloaded_file", "remove-data"),
            ("save_page_archive", "downloads"),
            ("download_file", "downloads"),
            ("list_downloads", "downloads"),
            ("pause_download", "downloads"),
            ("resume_download", "downloads"),
            ("cancel_download", "downloads"),
            ("show_download", "downloads"),
            ("run_script", "scripts"),
            ("click_element", "input"),
            ("type_text", "input"),
            ("select_option", "input"),
            ("press_key", "input"),
            ("scroll_page", "input"),
            ("wait_for_element", "input"),
            ("click_element_trusted", "input-trusted"),
            ("type_text_trusted", "input-trusted"),
            ("press_key_trusted", "input-trusted"),
            ("hover_element", "input-trusted"),
            ("drag_element", "input-trusted"),
            ("screenshot_full_page", "capture-trusted"),
            ("read_console", "console-trusted"),
            ("choose_file", "upload-trusted"),
            ("take_held", "held"),
        ] {
            assert!(named(name, None).is_none(), "{name}");
            assert_eq!(named(name, Some(group)).map(|tool| tool.group), Some(group));
        }
    }

    /// The six tools of `input` are offered by the server started for `input` and by no other,
    /// the server started for none included, so a person who declared that server before the group
    /// had tools is not shown any of them, and a server for another group refuses a call to one.
    #[test]
    fn the_input_tools_are_offered_by_the_server_of_input_alone() {
        let names = [
            "click_element",
            "type_text",
            "select_option",
            "press_key",
            "scroll_page",
            "wait_for_element",
        ];
        let in_table: Vec<&str> = TOOLS
            .iter()
            .filter(|tool| tool.group == "input")
            .map(|tool| tool.name)
            .collect();
        assert_eq!(in_table, names);
        let listed: Vec<String> = list(Some("input"))["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|tool| tool["name"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(listed, names);
        for name in names {
            assert!(named(name, Some("input")).is_some(), "{name}");
            assert!(named(name, None).is_none(), "{name}");
            assert!(named(name, Some("page-read")).is_none(), "{name}");
            assert!(offered(None).all(|tool| tool.name != name), "{name}");
        }
    }

    /// The five tools of `input-trusted` are offered by the server started for `input-trusted` and
    /// by no other, the server started for none and the server of `input` included, so a person
    /// who accepted the tools of `input` is not shown real input they did not accept, and a
    /// server for another group refuses a call to one.
    #[test]
    fn the_trusted_input_tools_are_offered_by_the_server_of_input_trusted_alone() {
        let names = [
            "click_element_trusted",
            "type_text_trusted",
            "press_key_trusted",
            "hover_element",
            "drag_element",
        ];
        let in_table: Vec<&str> = TOOLS
            .iter()
            .filter(|tool| tool.group == "input-trusted")
            .map(|tool| tool.name)
            .collect();
        assert_eq!(in_table, names);
        let listed: Vec<String> = list(Some("input-trusted"))["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|tool| tool["name"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(listed, names);
        for name in names {
            assert!(named(name, Some("input-trusted")).is_some(), "{name}");
            for other in ids().filter(|id| *id != "input-trusted") {
                assert!(named(name, Some(other)).is_none(), "{name} in {other}");
            }
            assert!(named(name, None).is_none(), "{name}");
            assert!(offered(None).all(|tool| tool.name != name), "{name}");
        }
        assert!(group("input-trusted").is_some());
    }

    /// The description is what a person reads when they accept the server's tool list, and a
    /// session reads it to choose a tool. The four tools that act on an element say that each
    /// action asks the person in a window in the browser, that a password field, a payment card
    /// field and a file chooser are refused, and that the events are synthetic, and the two that
    /// act on none say that no window asks. A description that dropped one of these would let a
    /// person accept a tool believing it acts without asking, or on a field it never touches.
    #[test]
    fn the_input_descriptions_say_that_the_person_is_asked_in_the_browser_and_what_is_refused() {
        let description = |name: &str| -> String {
            TOOLS
                .iter()
                .find(|tool| tool.name == name)
                .unwrap()
                .description
                .to_string()
        };
        for name in ["click_element", "type_text", "select_option", "press_key"] {
            let said = description(name);
            assert!(
                said.contains("asks the person in a window in the browser"),
                "{name}: {said}"
            );
            assert!(said.contains("only if they allow it"), "{name}: {said}");
            assert!(
                said.contains("synthetic: the page can tell it from"),
                "{name}: {said}"
            );
        }
        for name in ["click_element", "type_text", "select_option"] {
            let said = description(name);
            assert!(
                said.contains(
                    "A password field, a payment card field and a file chooser are refused"
                ),
                "{name}: {said}"
            );
        }
        let key = description("press_key");
        assert!(
            key.contains("refused where the focus is in a password field, a payment card field or a file chooser"),
            "{key}"
        );
        for name in ["scroll_page", "wait_for_element"] {
            let said = description(name);
            assert!(said.contains("no window asks the person"), "{name}: {said}");
            assert!(!said.contains("only if they allow it"), "{name}: {said}");
        }
        assert!(
            description("click_element").contains("with the left or right button, once or twice")
        );
        assert!(description("type_text").contains("Replace what"));
        assert!(description("press_key").contains("browser does not do what the key would do"));
    }

    /// The five tools of `input-trusted` send real input, which the page cannot tell from a
    /// person's, through a second extension that the person has to have installed. A person who
    /// accepts the server's tool list reads that each action asks them in a window of that
    /// extension and is made only if they allow it, which extension does it, and, for the tools
    /// that act on a field or the focus, that a password field, a payment card field and a file
    /// chooser are refused. A description that dropped one would let a person accept real input
    /// believing it asks nothing, or believing the browser's own extension does it.
    #[test]
    fn the_trusted_input_descriptions_say_who_acts_that_the_person_is_asked_in_a_window_and_what_is_refused()
     {
        let description = |name: &str| -> String {
            TOOLS
                .iter()
                .find(|tool| tool.name == name)
                .unwrap()
                .description
                .to_string()
        };
        for tool in TOOLS.iter().filter(|tool| tool.group == "input-trusted") {
            let said = description(tool.name);
            assert!(
                said.contains("The BraveBot Trusted Input extension does it"),
                "{}: {said}",
                tool.name
            );
            assert!(
                said.contains("in a window of that extension, which shows"),
                "{}: {said}",
                tool.name
            );
            assert!(
                said.contains("only if they allow it"),
                "{}: {said}",
                tool.name
            );
            assert!(
                said.contains("The reply says only that it was done"),
                "{}: {said}",
                tool.name
            );
        }
        for name in [
            "click_element_trusted",
            "type_text_trusted",
            "hover_element",
            "drag_element",
        ] {
            let said = description(name);
            assert!(
                said.contains("that the page cannot tell from a person's"),
                "{name}: {said}"
            );
        }
        assert!(
            description("press_key_trusted")
                .contains("as a real key press that the browser acts on")
        );
        for name in ["click_element_trusted", "type_text_trusted"] {
            let said = description(name);
            assert!(
                said.contains(
                    "A password field, a payment card field and a file chooser are refused"
                ),
                "{name}: {said}"
            );
        }
        assert!(description("press_key_trusted").contains(
            "refused where the focus is in a password field, a payment card field or a file chooser"
        ));
        assert!(description("click_element_trusted").contains("is not installed or is turned off"));
        assert!(description("drag_element").contains("which shows both elements"));
        assert!(description("drag_element").contains("neither a position nor a distance is"));
    }

    /// `read_console` is in `console-trusted`, which is asked about every call and in the browser,
    /// and offered by that group's server alone. It takes the URL of the tab, a level out of three
    /// words and a number of entries from 1 to 200, and no tab number, filter text, expression or
    /// time, so the question before a call shows all that decides which console is read and how
    /// much of it. Its result is text and not a picture.
    #[test]
    fn read_console_is_in_console_trusted_alone_and_takes_a_url_a_level_word_and_a_bounded_count() {
        let tool = named("read_console", Some("console-trusted")).unwrap();
        assert_eq!(tool.group, "console-trusted");
        assert!(!tool.returns_a_picture());
        let schema = tool.listed()["inputSchema"].clone();
        assert_eq!(schema["required"], json!(["url"]));
        assert_eq!(
            schema["properties"],
            json!({
                "url": {"type": "string"},
                "levels": {"type": "string", "enum": ["all", "warnings_and_errors", "errors"]},
                "max_entries": {"type": "integer", "minimum": 1, "maximum": 200},
            })
        );
        assert!(named("read_console", None).is_none());
        for other in ids().filter(|id| *id != "console-trusted") {
            assert!(named("read_console", Some(other)).is_none(), "{other}");
        }
        assert!(asks_every_call("console-trusted"));
        assert!(asks_in_browser(Some("console-trusted")));
        assert_eq!(
            TOOLS
                .iter()
                .filter(|tool| tool.group == "console-trusted")
                .count(),
            1
        );
        assert!(
            TOOLS
                .iter()
                .filter(|tool| tool.group != "console-trusted")
                .all(|tool| tool.name != "read_console")
        );
    }

    /// `choose_file` is in `upload-trusted`, which is asked about every call and in the browser,
    /// and offered by that group's server alone. It takes the URL of the tab, the accessible name
    /// of the file input, its role where the person wants to give one, and a path of at most 900
    /// characters. It takes no size, base name, tab number, node number, selector or file content,
    /// so the question before a call shows all that decides which input gets which file. The table
    /// marks it as the one tool that takes a file, so the server resolves its path, and no other
    /// tool is resolved that way.
    #[test]
    fn choose_file_is_in_upload_trusted_alone_and_takes_a_url_a_name_a_role_and_a_path() {
        let tool = named("choose_file", Some("upload-trusted")).unwrap();
        assert_eq!(tool.group, "upload-trusted");
        assert!(!tool.returns_a_picture());
        assert!(tool.takes_a_file());
        let schema = tool.listed()["inputSchema"].clone();
        assert_eq!(schema["required"], json!(["url", "name", "path"]));
        assert_eq!(
            schema["properties"],
            json!({
                "url": {"type": "string"},
                "role": {"type": "string", "enum": ROLES},
                "name": {"type": "string", "maxLength": 200},
                "path": {"type": "string", "maxLength": 900},
            })
        );
        assert!(named("choose_file", None).is_none());
        for other in ids().filter(|id| *id != "upload-trusted") {
            assert!(named("choose_file", Some(other)).is_none(), "{other}");
        }
        assert!(asks_every_call("upload-trusted"));
        assert!(asks_in_browser(Some("upload-trusted")));
        assert_eq!(
            TOOLS
                .iter()
                .filter(|tool| tool.group == "upload-trusted")
                .count(),
            1
        );
        assert_eq!(
            TOOLS
                .iter()
                .filter(|tool| tool.takes_a_file())
                .map(|tool| tool.name)
                .collect::<Vec<_>>(),
            ["choose_file"]
        );
        // The module that holds the limit is built where the relay is (unix).
        #[cfg(unix)]
        assert_eq!(crate::uploads::PATH_LIMIT as u64, MAX_PATH);
    }

    /// The description is what a person reads when they accept the server's tool list. It says
    /// which extension chooses the file and that it asks the person in a window each time, where
    /// the file has to be, which files are refused here and that nothing is sent for them, what
    /// the window shows, and that a page which sends a file when it is chosen gets it at once. A
    /// description that dropped one would let a person accept a tool that hands a file to a page
    /// without knowing who asks them, which files it can reach, or that the page may take it.
    #[test]
    fn the_upload_description_says_who_chooses_it_which_files_and_that_the_page_may_take_it() {
        let said = named("choose_file", Some("upload-trusted"))
            .unwrap()
            .description;
        for phrase in [
            "BraveBot Trusted Input extension",
            "asks the person in a window of that extension",
            "the uploads directory",
            "a link out of it",
            "a directory",
            "a file with more than one hard link",
            "a file over 10 MiB",
            "looks like a credential",
            "refused here, before the browser is asked, and nothing is sent",
            "the name, size and full path of the file",
            "the browser reads the file and gives it to the page",
            "Choosing a file sends nothing by itself",
            "a page whose script sends the file when it is chosen receives it at once",
            "for an input the page hides",
        ] {
            assert!(said.contains(phrase), "{phrase}: {said}");
        }
    }

    /// `run_script` is the one tool of `scripts`, which is asked about every call and in the
    /// browser, and it is offered by that group's server alone. It takes the URL of the tab and the
    /// code, both required, with the most the window shows whole: an address of at most 300
    /// characters and code of at most 2,000, so a planner is not offered a call the extension
    /// refuses because the person could not read all of it. It takes no selector, tab number or
    /// file, and its result is not a picture.
    #[test]
    fn run_script_is_in_scripts_alone_and_takes_a_url_and_code_with_their_largest_sizes() {
        let tool = named("run_script", Some("scripts")).unwrap();
        assert_eq!(tool.group, "scripts");
        assert!(!tool.returns_a_picture());
        assert!(!tool.takes_a_file());
        let schema = tool.listed()["inputSchema"].clone();
        assert_eq!(schema["required"], json!(["url", "code"]));
        assert_eq!(
            schema["properties"],
            json!({
                "url": {"type": "string", "maxLength": 300},
                "code": {"type": "string", "maxLength": 2000},
            })
        );
        assert!(named("run_script", None).is_none());
        for other in ids().filter(|id| *id != "scripts") {
            assert!(named("run_script", Some(other)).is_none(), "{other}");
        }
        assert!(asks_every_call("scripts"));
        assert!(asks_in_browser(Some("scripts")));
        assert_eq!(
            TOOLS.iter().filter(|tool| tool.group == "scripts").count(),
            1
        );
    }

    /// The description is what a person reads when they accept the server's tool list. It says
    /// that the person is shown all of the code and the address in a window of the browser and
    /// that nothing runs unless they allow it, that nothing limits the code except their reading
    /// it, what it can do to the page and to where its answer goes, that a longer code is refused
    /// and not cut, that the code starts once and what it sets going keeps running in the page
    /// until the page is closed or reloaded, that a call is refused where the browser did not
    /// apply the policy, that Allow User Scripts must be on, and that the result is private to
    /// the model. A description that dropped one would let a person accept a tool that runs code
    /// without knowing it is shown to them in full, that nothing else stops it, that a timer or
    /// listener it sets outlives the call, or that the model never reads what it answers.
    #[test]
    fn the_script_description_says_the_person_reads_all_the_code_nothing_else_limits_it_and_the_result_is_private()
     {
        let said = named("run_script", Some("scripts")).unwrap().description;
        for phrase in [
            "Each time it asks the person in a window in the browser",
            "shows the site, the address and all of the code, line by line",
            "the characters they cannot see written out",
            "only if they press Allow once there",
            "a denial, a closed window and no answer within 60 seconds run nothing",
            "Nothing limits the code except that the person reads it",
            "click and submit the page's forms",
            "send what it read to another address",
            "at most 2,000 characters, and a longer one is refused and never cut",
            "in a world that no other call shares and that keeps no variable for the next call",
            "The code starts once. Timers, listeners and observers it sets keep running in the page \
             until the page is closed or reloaded, and each can read and send without asking again.",
            "It is refused, with nothing run, when the browser has not applied the policy that stops \
             the script's own requests in the page",
            "Its answer is the value of its last expression, or of the promise it ends with",
            "a top-level `return` or `await` is a syntax error whose answer is null",
            "Allow User Scripts",
            "page content that the model cannot read",
            "arrives as a reference",
            "spawn_processor",
            "vet_content",
        ] {
            assert!(said.contains(phrase), "{phrase}: {said}");
        }
        for gone in ["gone after the call", "runs once"] {
            assert!(!said.contains(gone), "{gone}: {said}");
        }
    }

    /// The description is what a person reads when they accept the server's tool list. It says
    /// which extension reads the console and that it asks the person in a window each time, what
    /// the result is made of and how many entries it holds, what a script's error looks like
    /// there, and that the model cannot read the result and how it can. A description that
    /// dropped one would let a person accept a read of a page's console without knowing who asks
    /// them, or expect the model to read it.
    #[test]
    fn the_console_description_says_who_reads_it_who_is_asked_what_is_returned_and_that_the_model_cannot_read_it()
     {
        let said = named("read_console", Some("console-trusted"))
            .unwrap()
            .description;
        for phrase in [
            "the console of the open tab at this exact URL",
            "the errors they threw and what the browser reported about the page",
            "The BraveBot Trusted Input extension reads it",
            "asks the person in a window of that extension",
            "which shows the site and the address, and reads it only if they allow it",
            "the newest entries, at most max_entries (1 to 200, 100 when left out)",
            "all, warnings_and_errors or errors",
            "one JSON object on each line",
            "cut at 500 characters",
            "appears as Uncaught with its line and without the error's message",
            "a frame that Brave runs in another process is not read",
            "page text that the model cannot read directly",
            "spawn_processor",
            "vet_content, which is the question to ask to read it",
        ] {
            assert!(said.contains(phrase), "{phrase}: {said}");
        }
    }

    /// `screenshot_page`, `capture_screen`, `record_gif` and `screenshot_full_page` are the tools
    /// whose result is a picture, which the server sends as an image part. Each is offered by its
    /// own group's server alone. `screenshot_page` takes the URL of the tab and nothing else,
    /// `record_gif` that and a count of frames with the bounds the extension enforces, and
    /// `capture_screen` takes nothing, so the question before a call shows all that decides what
    /// is captured. `screenshot_full_page` is checked by its own test.
    #[test]
    fn the_tools_that_return_a_picture_are_the_four_and_take_a_url_a_count_or_nothing() {
        let pictures: Vec<&str> = TOOLS
            .iter()
            .filter(|tool| tool.returns_a_picture())
            .map(|tool| tool.name)
            .collect();
        assert_eq!(
            pictures,
            [
                "screenshot_page",
                "capture_screen",
                "record_gif",
                "screenshot_full_page"
            ]
        );
        for name in PICTURES {
            assert!(TOOLS.iter().any(|tool| tool.name == *name), "{name}");
        }
        for (name, group) in [
            ("screenshot_page", "page-read"),
            ("capture_screen", "screen"),
            ("record_gif", "screen"),
        ] {
            let tool = named(name, Some(group)).unwrap();
            assert_eq!(tool.group, group);
            assert!(named(name, None).is_none(), "{name}");
            for other in ids().filter(|id| *id != group) {
                assert!(named(name, Some(other)).is_none(), "{name} {other}");
            }
        }
        let schema =
            |name, group| named(name, Some(group)).unwrap().listed()["inputSchema"].clone();
        let shot = schema("screenshot_page", "page-read");
        assert_eq!(shot["required"], json!(["url"]));
        assert_eq!(shot["properties"], json!({"url": {"type": "string"}}));
        let screen = schema("capture_screen", "screen");
        assert_eq!(screen["properties"], json!({}));
        assert!(screen.get("required").is_none());
        let gif = schema("record_gif", "screen");
        assert_eq!(gif["required"], json!(["url"]));
        assert_eq!(
            gif["properties"],
            json!({
                "url": {"type": "string"},
                "frames": {"type": "integer", "minimum": 2, "maximum": 20},
            })
        );
        assert!(
            TOOLS
                .iter()
                .filter(|tool| !PICTURES.contains(&tool.name))
                .all(|tool| !tool.returns_a_picture())
        );
    }

    /// What a person reads when they accept the server of `screen`: that the picker is the
    /// browser's and is shown for each call, that nothing is captured until a person chooses and
    /// how long the call waits, that a screen holds more than a page, that the model does not see
    /// the picture and how it becomes available, and for a recording its frames, spacing, size,
    /// colours, the visible tab and the all-sites requirement. A description that dropped one
    /// would let a person accept a tool that shows a screen without knowing which, or expect the
    /// model to read the picture.
    #[test]
    fn the_screen_descriptions_say_what_is_captured_who_chooses_and_that_the_model_does_not_see_it()
    {
        let said = |name| named(name, Some("screen")).unwrap().description;
        for phrase in [
            "the browser's own picker",
            "for each call",
            "nothing is captured until the person chooses",
            "up to 60 seconds",
            "including what is not a page of Brave",
            "a picture the model does not see",
            "spawn_processor",
            "vet_content, which is the question to ask to read it",
        ] {
            assert!(said("capture_screen").contains(phrase), "{phrase}");
        }
        for phrase in [
            "at least 0.6 seconds apart",
            "480 pixels on a side",
            "256 colours",
            "visible tab of its window",
            "not brought forward",
            "ends, with no result, if the tab changes",
            "on all sites, because the browser lets an extension capture a tab only with access \
             to all sites",
            "a picture the model does not see",
            "vet_content, which is the question to ask to read it",
            "A frame of another site inside the page is in the picture",
            "names the tab's own URL only",
        ] {
            assert!(said("record_gif").contains(phrase), "{phrase}");
        }
    }

    /// `screenshot_full_page` is in `capture-trusted`, which is asked about every call and in the
    /// browser, and offered by that group's server alone. It takes the URL of the tab and nothing
    /// else, so there is no size, scale, format, quality, clip or tab number for a session to
    /// choose, and the question before a call shows all that decides which page is captured.
    #[test]
    fn screenshot_full_page_is_in_capture_trusted_alone_and_takes_a_url_and_nothing_else() {
        let tool = named("screenshot_full_page", Some("capture-trusted")).unwrap();
        assert_eq!(tool.group, "capture-trusted");
        assert!(tool.returns_a_picture());
        let schema = tool.listed()["inputSchema"].clone();
        assert_eq!(schema["required"], json!(["url"]));
        assert_eq!(schema["properties"], json!({"url": {"type": "string"}}));
        assert!(named("screenshot_full_page", None).is_none());
        for other in ids().filter(|id| *id != "capture-trusted") {
            assert!(
                named("screenshot_full_page", Some(other)).is_none(),
                "{other}"
            );
        }
        assert!(asks_every_call("capture-trusted"));
        assert!(asks_in_browser(Some("capture-trusted")));
        assert_eq!(
            TOOLS
                .iter()
                .filter(|tool| tool.group == "capture-trusted")
                .count(),
            1
        );
    }

    /// The description is what a person reads when they accept the server's tool list. It says
    /// that the model does not see the picture and how it becomes available, that the person is
    /// asked in a window and what it shows, the limits on the picture, what a frame does, and
    /// that the blocked sites list names the tab's own URL only. A description that dropped one
    /// would let a person accept a tool that reads the whole of a page without knowing it, or
    /// expect the model to read the picture.
    #[test]
    fn the_full_page_description_says_what_it_reads_who_is_asked_and_where_the_picture_goes() {
        let said = named("screenshot_full_page", Some("capture-trusted"))
            .unwrap()
            .description;
        for phrase in [
            "the whole top document",
            "including the parts out of view and the parts nobody can see",
            "background tab",
            "at most 2048 pixels on its long side and 8 MiB",
            "refused, never cut",
            "BraveBot Trusted Input extension",
            "asks the person in a window of that extension",
            "shows the site, the address and the size of the page",
            "a picture the model does not see",
            "spawn_processor",
            "vet_content, which is the question to ask to read it",
            "A frame is in the picture as the page draws it",
            "names the tab's own URL only",
        ] {
            assert!(said.contains(phrase), "{phrase}: {said}");
        }
    }

    /// The description is what a person reads when they accept the server's tool list. It says that
    /// the model does not see the picture and how it becomes available, that the tab has to be the
    /// visible one and is not brought forward, that the group has to be on all sites and why, and
    /// that a frame of another site is in the picture. A description that dropped one would let a
    /// person accept a tool that shows a screen without knowing which screen, or expect the
    /// model to read the picture.
    #[test]
    fn the_screenshot_description_says_the_picture_is_not_seen_by_the_model_and_what_it_shows() {
        let said = named("screenshot_page", Some("page-read"))
            .unwrap()
            .description;
        for phrase in [
            "a picture the model does not see",
            "spawn_processor",
            "vet_content, which is the question to ask to read it",
            "visible tab of its window",
            "not brought forward",
            "on all sites, because the browser lets an extension capture a tab only with access \
             to all sites",
            "A frame of another site inside the page is in the picture",
            "names the tab's own URL only",
        ] {
            assert!(said.contains(phrase), "{phrase}: {said}");
        }
    }

    /// `take_held` is in `held` and offered by that group's server alone, and it takes no argument
    /// at all, so the question before a call has nothing in it a session could choose and no
    /// page, tab or number to name what is returned. A schema that took a URL, or an index into
    /// what is held, would be a way for a session to say which of the person's items it wants.
    #[test]
    fn take_held_is_offered_by_the_server_of_held_alone_and_takes_no_argument() {
        let tool = named("take_held", Some("held")).unwrap();
        assert_eq!(tool.group, "held");
        let schema = tool.listed()["inputSchema"].clone();
        assert_eq!(schema, json!({"type": "object", "properties": {}}));
        assert!(named("take_held", None).is_none());
        for other in ids().filter(|id| *id != "held") {
            assert!(named("take_held", Some(other)).is_none(), "{other}");
        }
        assert!(!asks_every_call("held"));
        assert!(!asks_in_browser(Some("held")));
        assert_eq!(
            TOOLS
                .iter()
                .filter(|tool| tool.group == "held")
                .map(|tool| tool.name)
                .collect::<Vec<_>>(),
            ["take_held"]
        );
    }

    /// The description is what a person reads when they accept the server's tool list. It says
    /// that the result is page text the model cannot read directly, whoever selected it, and how
    /// it becomes readable: through a processor, or through `vet_content`, the question to ask to
    /// read it. It also says what is kept and for how long, and that nothing is named. A
    /// description that dropped one would let a person accept a tool that hands a session what
    /// they selected without saying who reads it.
    #[test]
    fn the_take_held_description_says_the_result_is_page_text_the_model_cannot_read_and_how_it_becomes_readable()
     {
        let said = named("take_held", Some("held")).unwrap().description;
        for phrase in [
            "held out in Brave with the BraveBot entries of its context menu",
            "and forget it",
            "a selection of text, the address of a link or the address of a tab",
            "At most 10 items are kept, for 30 minutes",
            "cut at 20,000 characters",
            "It takes no argument and names no page",
            "page text that the model cannot read directly, whoever selected it",
            "arrives as a reference",
            "spawn_processor",
            "vet_content, which is the question to ask to read it",
        ] {
            assert!(said.contains(phrase), "{phrase}: {said}");
        }
        assert!(!said.contains('\u{2014}'), "an em-dash: {said}");
    }

    /// A description is the same for every session, so it cannot say that a person looks at what
    /// `vet_content` releases or agrees to it: with auto-vetting, or with permissions bypassed,
    /// nobody is asked (VET-5). The planner is told who answers in the `vet_content` description.
    #[test]
    fn no_description_says_a_person_answers_vet_content() {
        for tool in TOOLS {
            let said = tool.description;
            if !said.contains("vet_content") {
                continue;
            }
            for claim in [
                "agreed",
                "after a person",
                "a person reads the exact",
                "lets the text through",
            ] {
                assert!(
                    !said.contains(claim),
                    "{} says who answers vet_content ({claim:?}): {said}",
                    tool.name
                );
            }
        }
    }

    /// `save_page_archive` is in `downloads` alone, takes a URL and nothing else, and returns no
    /// picture: the file is saved by the extension and the relay carries only the answer. Its
    /// description is what a person reads when they accept the server's tool list. It says that
    /// the file goes to the Downloads folder, that the extension names it, that a file there is not
    /// replaced, that 16 MiB is the most saved, that the result holds nothing of the page, that no
    /// tool reads the file, that only a page at an allowed site is saved and that a frame of
    /// another site is in it. A description that dropped
    /// one would let a person accept a tool that writes a page to disk without knowing where.
    #[test]
    fn the_archive_tool_takes_a_url_alone_saves_a_file_and_says_where_and_what_is_left_out() {
        let tool = named("save_page_archive", Some("downloads")).unwrap();
        assert_eq!(tool.group, "downloads");
        assert!(!tool.returns_a_picture());
        let schema = tool.listed()["inputSchema"].clone();
        assert_eq!(schema["required"], json!(["url"]));
        assert_eq!(schema["properties"], json!({"url": {"type": "string"}}));
        assert!(named("save_page_archive", None).is_none());
        for other in ids().filter(|id| *id != "downloads") {
            assert!(named("save_page_archive", Some(other)).is_none(), "{other}");
        }
        for phrase in [
            "MHTML file in the person's Downloads folder",
            "The extension names the file for the site",
            "a file already there is not replaced",
            "an archive over 16 MiB is not saved",
            "It holds nothing of the page, and no tool reads the file",
            "only if the person gives it the file",
            "only a page at a site the person allowed in the extension's options",
            "A frame of another site inside the page is in the archive",
            "names the tab's own URL only",
        ] {
            assert!(
                tool.description.contains(phrase),
                "{phrase}: {}",
                tool.description
            );
        }
    }

    /// The arguments of the `input` tools are each a word from a list, a text or a number with a
    /// largest size, or the URL of the tab, so a planner cannot send a long text where a name
    /// goes, and a list, if a tool ever takes one, has a largest length. The words are enums so
    /// that the question shows one of a few and the extension, which refuses any other, is never
    /// asked for one.
    #[test]
    fn every_argument_of_an_input_tool_is_a_word_from_a_list_or_has_a_largest_size() {
        let mut checked = 0;
        for tool in TOOLS.iter().filter(|tool| {
            tool.group == "input"
                || tool.group == "input-trusted"
                || tool.group == "console-trusted"
                || tool.group == "upload-trusted"
                || tool.group == "scripts"
        }) {
            let schema = tool.listed()["inputSchema"].clone();
            for (name, property) in schema["properties"].as_object().unwrap() {
                let bounded = match property["type"].as_str().unwrap() {
                    "string" => {
                        name == "url"
                            || property["enum"].is_array()
                            || property["maxLength"].is_u64()
                    }
                    "integer" => property["minimum"].is_i64() && property["maximum"].is_i64(),
                    "array" => property["maxItems"].is_u64(),
                    other => panic!("{} takes {name} as a {other}", tool.name),
                };
                assert!(bounded, "{} takes {name} as {property}", tool.name);
                checked += 1;
            }
        }
        assert!(checked > 0, "no argument of an input tool was read");
        let schema = |name: &str| -> Value {
            named(name, Some("input")).unwrap().listed()["inputSchema"].clone()
        };
        assert!(schema("click_element")["properties"]["role"]["enum"].is_array());
        assert!(schema("click_element")["properties"]["button"]["enum"].is_array());
        assert!(schema("press_key")["properties"]["key"]["enum"].is_array());
        assert!(schema("scroll_page")["properties"]["direction"]["enum"].is_array());
        assert_eq!(schema("scroll_page")["properties"]["pages"]["maximum"], 5);
        let trusted = |name: &str| -> Value {
            named(name, Some("input-trusted")).unwrap().listed()["inputSchema"].clone()
        };
        for name in [
            "click_element_trusted",
            "type_text_trusted",
            "hover_element",
            "drag_element",
        ] {
            assert_eq!(
                trusted(name)["properties"]["role"]["enum"],
                json!(ROLES),
                "{name}"
            );
        }
        assert_eq!(
            trusted("press_key_trusted")["properties"]["key"]["enum"],
            json!(KEYS)
        );
        assert_eq!(
            trusted("click_element_trusted")["properties"]["button"]["enum"],
            json!(["left", "right"])
        );
        assert_eq!(
            trusted("click_element_trusted")["properties"]["count"]["maximum"],
            2
        );
        assert_eq!(
            trusted("type_text_trusted")["properties"]["text"]["maxLength"],
            2000
        );
        // A drag names its second element as it names its first, by a role from the list and a
        // name, and the schema has no other way to say where it ends.
        let drag = trusted("drag_element");
        assert_eq!(drag["properties"]["onto_role"]["enum"], json!(ROLES));
        assert_eq!(drag["properties"]["onto_name"]["maxLength"], 200);
        assert_eq!(
            drag["required"],
            json!(["url", "role", "name", "onto_role", "onto_name"])
        );
        let mut names: Vec<&String> = drag["properties"].as_object().unwrap().keys().collect();
        names.sort();
        assert_eq!(names, ["name", "onto_name", "onto_role", "role", "url"]);
    }

    /// `list_page_elements` is offered by the server of `page-read` alone and takes the URL of the
    /// tab and nothing else. Its description says that the result is text from the page which the
    /// model does not read, how it becomes readable, that it holds no value of a field and leaves
    /// out what is never acted on, and what the numbers are good for. A description that dropped
    /// one would let a person accept a tool that shows the page's controls without knowing that the
    /// model cannot read them, or expect a field's value in the list.
    #[test]
    fn the_list_of_elements_takes_a_url_and_its_description_says_the_model_does_not_read_it() {
        let tool = named("list_page_elements", Some("page-read")).unwrap();
        assert_eq!(tool.group, "page-read");
        let schema = tool.listed()["inputSchema"].clone();
        assert_eq!(schema["required"], json!(["url"]));
        assert_eq!(schema["properties"], json!({"url": {"type": "string"}}));
        assert!(named("list_page_elements", None).is_none());
        for other in ids().filter(|id| *id != "page-read") {
            assert!(
                named("list_page_elements", Some(other)).is_none(),
                "{other}"
            );
        }
        assert!(!tool.returns_a_picture());
        for phrase in [
            "at most 40",
            "120 characters",
            "no value that a field holds",
            "hidden or disabled",
            "a password, payment card or file field, is not listed",
            "The result is text from the page",
            "a reference that the model does not read",
            "tells the model which number",
            "the model asks for the text with vet_content, which may release it for the model to read",
            "as element_number to click_element, type_text or select_option for the same URL",
            "within 5 minutes",
            "only while the page still shows that element",
            "A later list replaces this one",
        ] {
            assert!(
                tool.description.contains(phrase),
                "{phrase}: {}",
                tool.description
            );
        }
    }

    /// `click_element`, `type_text` and `select_option` name the element they act on by a role and
    /// a name or by the number it has in the last list, which a person read. The number is a whole
    /// number from 1 to the most elements a list holds. The schema cannot say that one form is
    /// given and not both, so the description does, and neither form is required. The tools that
    /// act on no element, or on the one with the focus, take no number.
    #[test]
    fn an_element_is_named_by_a_role_and_a_name_or_by_the_number_a_list_gave() {
        let tool = |name: &str| named(name, Some("input")).unwrap();
        for name in ["click_element", "type_text", "select_option"] {
            let schema = tool(name).listed()["inputSchema"].clone();
            assert_eq!(
                schema["properties"]["element_number"],
                json!({"type": "integer", "minimum": 1, "maximum": 40}),
                "{name}"
            );
            for form in ["role", "name"] {
                assert!(
                    !schema["required"]
                        .as_array()
                        .unwrap()
                        .contains(&json!(form)),
                    "{name} requires {form}"
                );
                assert!(schema["properties"][form].is_object(), "{name}");
            }
            assert_eq!(schema["required"][0], "url", "{name}");
            for phrase in [
                "Give element_number in place of the role and the name",
                "last list_page_elements made for this URL",
                "give one form and not",
            ] {
                let said = tool(name).description.replace("\n", " ");
                assert!(said.contains(phrase), "{name}: {phrase}: {said}");
            }
        }
        for name in ["press_key", "scroll_page", "wait_for_element"] {
            let schema = tool(name).listed()["inputSchema"].clone();
            assert!(
                schema["properties"].get("element_number").is_none(),
                "{name}"
            );
            assert!(!tool(name).description.contains("element_number"), "{name}");
        }
        for other in TOOLS.iter().filter(|tool| tool.group != "input") {
            let schema = other.listed()["inputSchema"].clone();
            assert!(
                schema["properties"].get("element_number").is_none(),
                "{}",
                other.name
            );
        }
    }

    /// The question before a call shows its arguments, so the visits to a page are asked for by the
    /// one URL a person reads there, and the other five tools take no argument.
    #[test]
    fn the_visits_to_a_page_are_asked_for_by_its_url_and_a_count() {
        let schema =
            named("get_visits", Some("history-read")).unwrap().listed()["inputSchema"].clone();
        assert_eq!(schema["required"], json!(["url"]));
        assert_eq!(schema["properties"]["url"]["type"], "string");
        assert_eq!(schema["properties"]["max_results"]["type"], "integer");
        assert_eq!(schema["properties"].as_object().unwrap().len(), 2);
        for name in [
            "list_windows",
            "list_tab_groups",
            "list_recently_closed",
            "list_top_sites",
            "list_synced_tabs",
        ] {
            let schema = TOOLS
                .iter()
                .find(|tool| tool.name == name)
                .unwrap()
                .listed()["inputSchema"]
                .clone();
            assert_eq!(schema["properties"], json!({}), "{name}");
            assert!(schema.get("required").is_none(), "{name}");
        }
    }

    /// The question before a call shows its arguments, so a bookmark folder is asked for by the
    /// title a person can read there and not by a node id that comes from an earlier result, the
    /// requests of a page by the one URL of its tab, and the other two tools by a count alone.
    #[test]
    fn a_folder_is_asked_for_by_its_title_and_the_requests_of_a_page_by_its_url() {
        let schema = |name: &str| {
            TOOLS
                .iter()
                .find(|tool| tool.name == name)
                .unwrap()
                .listed()["inputSchema"]
                .clone()
        };
        let folder = schema("list_bookmark_folder");
        assert!(folder.get("required").is_none());
        assert_eq!(folder["properties"]["folder"]["type"], "string");
        assert_eq!(folder["properties"]["max_results"]["type"], "integer");
        assert_eq!(folder["properties"].as_object().unwrap().len(), 2);

        let requests = schema("list_requests");
        assert_eq!(requests["required"], json!(["url"]));
        assert_eq!(requests["properties"]["url"]["type"], "string");
        assert_eq!(requests["properties"].as_object().unwrap().len(), 1);

        for name in ["list_recent_bookmarks", "list_reading_list"] {
            let schema = schema(name);
            assert!(schema.get("required").is_none(), "{name}");
            assert_eq!(schema["properties"]["max_results"]["type"], "integer");
            assert_eq!(schema["properties"].as_object().unwrap().len(), 1, "{name}");
        }
    }

    /// The question before a call shows its arguments, so the content settings are asked for at
    /// the one URL a person reads there, the idle state by a number of seconds, and the time the
    /// computer is kept awake by a number of minutes with a ceiling and one of two levels. The other
    /// four tools of the group take no argument.
    #[test]
    fn the_browser_info_tools_take_a_url_a_number_of_seconds_or_minutes_and_nothing_else() {
        let schema =
            |name: &str| named(name, Some("browser-info")).unwrap().listed()["inputSchema"].clone();
        let content = schema("get_content_settings");
        assert_eq!(content["required"], json!(["url"]));
        assert_eq!(content["properties"]["url"]["type"], "string");
        assert_eq!(content["properties"].as_object().unwrap().len(), 1);

        let idle = schema("get_idle_state");
        assert!(idle.get("required").is_none());
        assert_eq!(idle["properties"]["seconds"]["type"], "integer");
        assert_eq!(idle["properties"].as_object().unwrap().len(), 1);

        let awake = schema("keep_awake");
        assert_eq!(awake["required"], json!(["minutes"]));
        assert_eq!(awake["properties"]["minutes"]["type"], "integer");
        assert_eq!(
            (
                &awake["properties"]["minutes"]["minimum"],
                &awake["properties"]["minutes"]["maximum"]
            ),
            (&json!(0), &json!(240))
        );
        assert_eq!(
            awake["properties"]["level"]["enum"],
            json!(["system", "display"])
        );
        assert_eq!(awake["properties"].as_object().unwrap().len(), 2);

        for name in [
            "get_privacy_settings",
            "get_font_settings",
            "get_system_info",
            "get_access_flags",
        ] {
            let schema = schema(name);
            assert_eq!(schema["properties"], json!({}), "{name}");
            assert!(schema.get("required").is_none(), "{name}");
        }
    }

    /// The question before a call shows its arguments, so a bookmark is named by its URL, a folder
    /// by its exact title, and a reading list entry by its URL, and never by the id the browser
    /// gives a node, which a person cannot judge there. What the call sets is a title or a switch.
    #[test]
    fn a_bookmark_is_named_by_its_url_and_a_folder_by_its_title_and_never_by_an_id() {
        let schema = |name: &str| {
            named(name, Some("bookmarks-write")).unwrap().listed()["inputSchema"].clone()
        };
        let properties = |name: &str| -> Vec<String> {
            let mut keys: Vec<String> = schema(name)["properties"]
                .as_object()
                .unwrap()
                .keys()
                .cloned()
                .collect();
            keys.sort();
            keys
        };
        for (name, expected, required) in [
            (
                "create_bookmark",
                vec!["folder", "title", "url"],
                vec!["url"],
            ),
            (
                "create_bookmark_folder",
                vec!["parent", "title"],
                vec!["title"],
            ),
            (
                "move_bookmark",
                vec!["folder", "url"],
                vec!["url", "folder"],
            ),
            (
                "rename_bookmark",
                vec!["title", "url"],
                vec!["url", "title"],
            ),
            (
                "add_reading_list_entry",
                vec!["read", "title", "url"],
                vec!["url", "title"],
            ),
            (
                "update_reading_list_entry",
                vec!["read", "title", "url"],
                vec!["url"],
            ),
        ] {
            assert_eq!(properties(name), expected, "{name}");
            assert_eq!(schema(name)["required"], json!(required), "{name}");
            for argument in schema(name)["properties"].as_object().unwrap().values() {
                assert!(
                    argument["type"] == "string" || argument["type"] == "boolean",
                    "{name}: {argument}"
                );
            }
        }
        for name in ["add_reading_list_entry", "update_reading_list_entry"] {
            assert_eq!(
                schema(name)["properties"]["read"]["type"],
                "boolean",
                "{name}"
            );
        }
        assert!(
            schema("create_bookmark_folder")["properties"]
                .get("url")
                .is_none()
        );
    }

    /// The question before a call shows its arguments, so what is deleted is named by an exact URL,
    /// two moments or a list of kinds of data, all of which a person can read there, and never by a
    /// tab id, a window id or a bookmark id. A deletion by time or by kind has no default: the
    /// moments and the kinds are all required, and the kinds are a closed list that excludes
    /// everything the browser has deprecated.
    #[test]
    fn what_is_deleted_is_named_by_a_url_two_moments_or_a_list_of_kinds_and_nothing_else() {
        let schema =
            |name: &str| named(name, Some("remove-data")).unwrap().listed()["inputSchema"].clone();
        for name in [
            "close_tab",
            "close_window",
            "delete_history_entry",
            "remove_bookmark",
        ] {
            let schema = schema(name);
            assert_eq!(schema["required"], json!(["url"]), "{name}");
            assert_eq!(schema["properties"]["url"]["type"], "string", "{name}");
            assert_eq!(schema["properties"].as_object().unwrap().len(), 1, "{name}");
        }

        let range = schema("delete_history_range");
        assert_eq!(range["required"], json!(["start", "end"]));
        for bound in ["start", "end"] {
            assert_eq!(range["properties"][bound]["type"], "string", "{bound}");
            assert_eq!(range["properties"][bound]["format"], "date-time", "{bound}");
        }
        assert_eq!(range["properties"].as_object().unwrap().len(), 2);

        let clear = schema("clear_browsing_data");
        assert_eq!(clear["required"], json!(["since", "types"]));
        assert_eq!(clear["properties"]["since"]["type"], "string");
        assert_eq!(clear["properties"]["since"]["format"], "date-time");
        assert_eq!(clear["properties"]["types"]["type"], "array");
        assert_eq!(clear["properties"]["types"]["minItems"], 1);
        assert_eq!(
            clear["properties"]["types"]["items"]["enum"],
            json!([
                "cache",
                "cacheStorage",
                "cookies",
                "downloads",
                "fileSystems",
                "formData",
                "history",
                "indexedDB",
                "localStorage",
                "serviceWorkers"
            ])
        );
        assert_eq!(clear["properties"].as_object().unwrap().len(), 2);
    }

    /// A bookmark folder is deleted by its title, a reading list entry by its URL and a downloaded
    /// file by its filename or its URL, never by a bookmark id or a download id, which a person
    /// cannot read in the question. The download takes one of two names, so neither is required by
    /// the schema, and no property says to delete what is in a folder.
    #[test]
    fn a_folder_an_entry_and_a_downloaded_file_are_named_by_title_url_or_filename_and_nothing_else()
    {
        let schema =
            |name: &str| named(name, Some("remove-data")).unwrap().listed()["inputSchema"].clone();
        for (name, property) in [
            ("remove_bookmark_folder", "folder"),
            ("remove_reading_list_entry", "url"),
        ] {
            let schema = schema(name);
            assert_eq!(schema["required"], json!([property]), "{name}");
            assert_eq!(schema["properties"][property]["type"], "string", "{name}");
            assert_eq!(schema["properties"].as_object().unwrap().len(), 1, "{name}");
        }

        let file = schema("delete_downloaded_file");
        assert_eq!(file["properties"]["filename"]["type"], "string");
        assert_eq!(file["properties"]["url"]["type"], "string");
        assert_eq!(file["properties"].as_object().unwrap().len(), 2);
        assert!(file.get("required").is_none());
    }

    /// The question before a call shows its arguments, so a file is downloaded from one URL a
    /// person reads there and nothing else: no destination folder or file name, no method, header
    /// or body, and no dialog to choose one. A download the other tools act on is named by its
    /// filename or the URL it started from, one of the two, never by the id the browser gave it,
    /// which a person cannot read in the question, and the list takes only a count.
    #[test]
    fn a_download_is_started_by_one_url_and_acted_on_by_filename_or_url_and_nothing_else() {
        let schema =
            |name: &str| named(name, Some("downloads")).unwrap().listed()["inputSchema"].clone();
        let start = schema("download_file");
        assert_eq!(start["required"], json!(["url"]));
        assert_eq!(start["properties"]["url"]["type"], "string");
        assert_eq!(start["properties"].as_object().unwrap().len(), 1);

        let list = schema("list_downloads");
        assert_eq!(list["properties"]["max_results"]["type"], "integer");
        assert_eq!(list["properties"].as_object().unwrap().len(), 1);
        assert!(list.get("required").is_none());

        for name in [
            "pause_download",
            "resume_download",
            "cancel_download",
            "show_download",
        ] {
            let schema = schema(name);
            assert_eq!(schema["properties"]["filename"]["type"], "string", "{name}");
            assert_eq!(schema["properties"]["url"]["type"], "string", "{name}");
            assert_eq!(schema["properties"].as_object().unwrap().len(), 2, "{name}");
            assert!(schema.get("required").is_none(), "{name}");
        }
    }

    /// The question before a call shows its arguments, so a page is asked for by the URL a person
    /// can read there rather than by a tab id that names nothing they can check.
    #[test]
    fn a_page_is_asked_for_by_its_url_and_optional_frame_url() {
        let schema = named("read_page", None).unwrap().listed()["inputSchema"].clone();
        assert_eq!(schema["required"], json!(["url"]));
        assert_eq!(schema["properties"]["url"]["type"], "string");
        assert_eq!(schema["properties"]["frame_url"]["type"], "string");
        assert!(
            !schema["required"]
                .as_array()
                .unwrap()
                .contains(&json!("frame_url"))
        );

        let frames = named("list_frames", None).unwrap().listed()["inputSchema"].clone();
        assert_eq!(frames["required"], json!(["url"]));
        assert_eq!(frames["properties"]["url"]["type"], "string");
    }

    /// The question before a call shows its arguments, so the tab that opens is named by the one
    /// URL a person reads there, and a call without it is not valid.
    #[test]
    fn a_tab_is_opened_by_its_url_alone() {
        let schema = named("open_tab", None).unwrap().listed()["inputSchema"].clone();
        assert_eq!(schema["required"], json!(["url"]));
        assert_eq!(schema["properties"]["url"]["type"], "string");
        assert_eq!(schema["properties"].as_object().unwrap().len(), 1);
    }

    /// The question before a call shows its arguments, so a tab that is moved is named by the URL a
    /// person reads there and, where it is taken to an address, by that address, never by a tab id
    /// or a window id. A reload and a move back or forward take no destination, since the page
    /// before or after is not one the question could show, and a window is opened and a closed tab
    /// restored by one URL, the one `list_recently_closed` shows, and never by a session id.
    #[test]
    fn a_tab_is_moved_by_its_url_and_an_address_and_never_by_an_id() {
        let schema =
            |name: &str| named(name, Some("navigate")).unwrap().listed()["inputSchema"].clone();
        let properties = |name: &str| -> Vec<String> {
            let mut keys: Vec<String> = schema(name)["properties"]
                .as_object()
                .unwrap()
                .keys()
                .cloned()
                .collect();
            keys.sort();
            keys
        };
        for (name, expected, required) in [
            ("navigate_tab", vec!["to", "url"], vec!["url", "to"]),
            ("reload_tab", vec!["url"], vec!["url"]),
            ("go_back", vec!["url"], vec!["url"]),
            ("go_forward", vec!["url"], vec!["url"]),
            ("open_window", vec!["url"], vec!["url"]),
            ("restore_closed", vec!["url"], vec!["url"]),
        ] {
            assert_eq!(properties(name), expected, "{name}");
            assert_eq!(schema(name)["required"], json!(required), "{name}");
            for argument in schema(name)["properties"].as_object().unwrap().values() {
                assert_eq!(argument["type"], "string", "{name}");
            }
        }
    }

    /// The question before a call shows its arguments, so a search is the words a person reads there,
    /// short enough to read whole, and a wait is for the tab at one URL, never a tab id.
    #[test]
    fn a_search_is_asked_for_by_short_text_and_a_wait_by_a_url() {
        let search = named("search_web", Some("navigate")).unwrap().listed()["inputSchema"].clone();
        assert_eq!(search["required"], json!(["query"]));
        assert_eq!(search["properties"]["query"]["type"], "string");
        assert_eq!(search["properties"]["query"]["maxLength"], 256);
        assert_eq!(search["properties"].as_object().unwrap().len(), 1);
        let wait =
            named("wait_for_load", Some("navigate")).unwrap().listed()["inputSchema"].clone();
        assert_eq!(wait["required"], json!(["url"]));
        assert_eq!(wait["properties"]["url"]["type"], "string");
        assert_eq!(wait["properties"].as_object().unwrap().len(), 1);
    }

    /// A cookie value is a session token, so the two cookie tools take the URL a cookie is sent to
    /// and, to delete one, its name, and nothing that names or carries a value. No tool in the
    /// table takes a value or a cookie, and no other tool is a cookie tool, until a maintainer rules
    /// that a session may read or set a value (BROWSER-30).
    #[test]
    fn the_cookie_tools_take_a_url_and_a_name_and_no_tool_takes_a_cookie_value() {
        let schema =
            |name: &str| named(name, Some("cookies")).unwrap().listed()["inputSchema"].clone();
        let list = schema("list_cookie_names");
        assert_eq!(list["required"], json!(["url"]));
        assert_eq!(list["properties"].as_object().unwrap().len(), 1);
        assert_eq!(list["properties"]["url"]["type"], "string");
        let delete = schema("delete_cookie");
        assert_eq!(delete["required"], json!(["name", "url"]));
        assert_eq!(delete["properties"].as_object().unwrap().len(), 2);
        assert_eq!(delete["properties"]["name"]["type"], "string");
        assert_eq!(delete["properties"]["url"]["type"], "string");

        let cookie_tools: Vec<&str> = TOOLS
            .iter()
            .filter(|tool| tool.name.contains("cookie"))
            .map(|tool| tool.name)
            .collect();
        assert_eq!(cookie_tools, ["list_cookie_names", "delete_cookie"]);
        for tool in TOOLS {
            let properties = tool.listed()["inputSchema"]["properties"].clone();
            for argument in properties.as_object().unwrap().keys() {
                assert!(
                    !["value", "cookie", "cookies", "token"].contains(&argument.as_str()),
                    "{} takes {argument}",
                    tool.name
                );
            }
        }
        for name in ["list_cookie_names", "delete_cookie"] {
            let description = named(name, Some("cookies")).unwrap().description;
            assert!(
                description.contains("cookie's value"),
                "{name}: {description}"
            );
        }
    }

    /// The question before a call shows its arguments, so a tool that acts on a tab names it by the
    /// URL a person reads there, never by a tab id or a window id, and what it is asked to do is a
    /// switch, position, title, color or zoom factor a person can judge. A page tool takes the URL
    /// alone. A tool that changes several tabs takes their URLs, and one that changes a tab group
    /// names it by a tab in it.
    #[test]
    fn a_tab_is_acted_on_by_its_url_and_what_is_asked_of_it() {
        let schema = |name: &str| {
            named(name, Some(tool_group(name))).unwrap().listed()["inputSchema"].clone()
        };
        let properties = |name: &str| -> Vec<String> {
            let mut keys: Vec<String> = schema(name)["properties"]
                .as_object()
                .unwrap()
                .keys()
                .cloned()
                .collect();
            keys.sort();
            keys
        };
        for (name, expected, required) in [
            ("read_page_links", vec!["url"], vec!["url"]),
            ("read_page_forms", vec!["url"], vec!["url"]),
            ("save_page_archive", vec!["url"], vec!["url"]),
            ("focus_tab", vec!["url"], vec!["url"]),
            ("pin_tab", vec!["pinned", "url"], vec!["url"]),
            ("mute_tab", vec!["muted", "url"], vec!["url"]),
            ("move_tab", vec!["index", "url"], vec!["url", "index"]),
            (
                "update_tab_group",
                vec!["collapsed", "color", "title", "url"],
                vec!["url"],
            ),
            ("discard_tab", vec!["url"], vec!["url"]),
            ("duplicate_tab", vec!["url"], vec!["url"]),
            ("set_zoom", vec!["factor", "url"], vec!["url", "factor"]),
            ("focus_window", vec!["url"], vec!["url"]),
            (
                "resize_window",
                vec!["height", "url", "width"],
                vec!["url", "width", "height"],
            ),
        ] {
            assert_eq!(properties(name), expected, "{name}");
            assert_eq!(schema(name)["required"], json!(required), "{name}");
            assert_eq!(
                schema(name)["properties"]["url"]["type"],
                "string",
                "{name}"
            );
        }
        assert_eq!(schema("pin_tab")["properties"]["pinned"]["type"], "boolean");
        assert_eq!(schema("mute_tab")["properties"]["muted"]["type"], "boolean");
        assert_eq!(schema("move_tab")["properties"]["index"]["type"], "integer");
        assert_eq!(schema("move_tab")["properties"]["index"]["minimum"], -1);
        assert_eq!(
            schema("update_tab_group")["properties"]["collapsed"]["type"],
            "boolean"
        );
        assert_eq!(
            schema("update_tab_group")["properties"]["title"]["type"],
            "string"
        );
        assert_eq!(
            schema("update_tab_group")["properties"]["color"]["enum"],
            json!([
                "grey", "blue", "red", "yellow", "green", "pink", "purple", "cyan", "orange"
            ])
        );
        for size in ["width", "height"] {
            let size = &schema("resize_window")["properties"][size];
            assert_eq!(size["type"], "integer");
            assert_eq!(size["minimum"], 1);
        }
        let factor = &schema("set_zoom")["properties"]["factor"];
        assert_eq!(factor["type"], "number");
        assert_eq!(
            (&factor["minimum"], &factor["maximum"]),
            (&json!(0), &json!(5))
        );

        // A group of tabs is named by the URLs of the tabs in it, a list of at least one, and
        // never by a group id or a tab id.
        for name in ["group_tabs", "ungroup_tabs"] {
            let urls = &schema(name)["properties"]["urls"];
            assert_eq!(urls["type"], "array", "{name}");
            assert_eq!(urls["items"]["type"], "string", "{name}");
            assert_eq!(urls["minItems"], 1, "{name}");
            assert_eq!(schema(name)["required"], json!(["urls"]), "{name}");
            assert!(schema(name)["properties"].get("url").is_none(), "{name}");
        }
        assert_eq!(
            schema("group_tabs")["properties"]["title"]["type"],
            "string"
        );
        assert_eq!(
            schema("group_tabs")["properties"]
                .as_object()
                .unwrap()
                .len(),
            2
        );
        assert_eq!(
            schema("ungroup_tabs")["properties"]
                .as_object()
                .unwrap()
                .len(),
            1
        );
    }

    /// The group a tool is in, for a test that looks a tool up as its group's server would.
    fn tool_group(name: &str) -> &'static str {
        TOOLS.iter().find(|tool| tool.name == name).unwrap().group
    }

    /// Every schema is an object schema, which is what an MCP client expects of `inputSchema`.
    #[test]
    fn every_tool_takes_an_object() {
        for tool in TOOLS {
            assert_eq!(
                tool.listed()["inputSchema"]["type"],
                "object",
                "{}",
                tool.name
            );
        }
    }

    /// The schemas in `schema`, itself included, that describe a list.
    fn lists(schema: &Value) -> Vec<&Value> {
        let inside: Vec<&Value> = match schema {
            Value::Object(members) => members.values().flat_map(lists).collect(),
            Value::Array(entries) => entries.iter().flat_map(lists).collect(),
            _ => Vec::new(),
        };
        let is_a_list = schema["type"] == "array";
        is_a_list
            .then_some(schema)
            .into_iter()
            .chain(inside)
            .collect()
    }

    /// A list argument states the most entries a call may hold, the bound the extension enforces,
    /// so a client sees it before it calls and a list added without one fails here. The tools with
    /// a list are named, so a walk that found none could not pass.
    #[test]
    fn every_list_argument_states_the_most_entries_a_call_may_hold() {
        let mut with_a_list = Vec::new();
        for tool in TOOLS {
            let schema = tool.listed()["inputSchema"].clone();
            for list in lists(&schema) {
                assert_eq!(list["maxItems"], LIST_LIMIT, "{}", tool.name);
                with_a_list.push(tool.name);
            }
        }
        assert_eq!(
            with_a_list,
            ["group_tabs", "ungroup_tabs", "clear_browsing_data"]
        );
    }
}
