// The options page: presets, one switch per group with a switch per tool inside
// it, the list of sites a group that reads pages may reach, the list of sites
// no tool may touch and the log of the last calls. A tool switch and the blocked list are saved as soon as
// they change. A group switch or a preset asks the person to confirm and then
// asks the browser for the permissions of the groups it turns on, and a group
// switch also asks how long the group stays on. The allowed sites and a group's
// scope are saved by a click that asks the browser for the sites first
// (switches.js).

import { ACTIVITY_KEY, activityLog } from "./activity.js";
import {
  ALWAYS_ON,
  GROUPS,
  NO_ACCESS,
  PRESETS,
  SCOPE_ALL,
  SCOPE_ALLOWED,
  capturesPages,
  limitedToAllowed,
  needed,
  neededTogether,
  scopeOf,
} from "./groups.js";
import {
  LIFETIMES,
  ONE_HOUR,
  UNTIL_RESTART,
  UNTIL_TURNED_OFF,
  defaultLifetime,
  lifetimeRecords,
} from "./lifetimes.js";
import {
  allowSites,
  applyPreset,
  groupSwitches,
  isOn,
  presetGroups,
  setScope,
  setTool,
  turnOff,
  turnOn,
} from "./switches.js";
import {
  allowedSites,
  blockedSites,
  saveBlockedSites,
  settings,
  siteAccess,
} from "./tools.js";
import { CODE_LIMIT, RESULT_LIMIT, userScriptsAvailable } from "./scripts.js";
import { warningsFor } from "./warnings.js";

const LABELS = {
  get_platform_info: [
    "Check the extension is installed",
    "The operating system and architecture Brave runs on, and nothing else.",
  ],
  list_tabs: [
    "List open tabs",
    "The title and URL of every open tab, except at blocked sites.",
  ],
  list_frames: [
    "List frames in an open tab",
    "The exact web URLs embedded in a tab, named by the tab's URL, except at blocked sites.",
  ],
  list_windows: [
    "List open windows",
    "The type, state and focus of every open window, and whether it is private. No tab is listed.",
  ],
  list_tab_groups: [
    "List tab groups",
    "The title, color and window of every tab group, and whether it is collapsed.",
  ],
  list_recently_closed: [
    "List recently closed tabs and windows",
    "When each closed, and the title and URL of its tabs, except at blocked sites.",
  ],
  read_page: [
    "Read an open page or frame",
    "The text at a tab URL, or at one exact frame URL in that tab.",
  ],
  search_history: [
    "Search history",
    "Pages you visited, by words in their title or URL, except at blocked sites.",
  ],
  get_visits: [
    "List the visits to a page",
    "When you visited one exact URL, and how you got there, except at blocked sites.",
  ],
  list_top_sites: [
    "List most visited sites",
    "The sites Brave counts as the ones you visit most, except at blocked sites.",
  ],
  list_synced_tabs: [
    "List tabs on your other devices",
    "The title and URL of the tabs open on each device you sync, except at blocked sites.",
  ],
  search_bookmarks: [
    "Search bookmarks",
    "Your bookmarks, by words in their title or URL, except at blocked sites.",
  ],
  list_bookmark_folder: [
    "List a bookmark folder",
    "The bookmarks and folders directly in one folder, named by its exact title, except bookmarks at blocked sites.",
  ],
  list_recent_bookmarks: [
    "List recently added bookmarks",
    "Your newest bookmarks and when each was added, except at blocked sites.",
  ],
  list_reading_list: [
    "List the reading list",
    "The title and address of each reading list entry, whether you read it and when it was added, except at blocked sites.",
  ],
  open_tab: [
    "Open a page in a new tab",
    "An HTTP or HTTPS URL, in a background tab with your cookies. The tab is closed if the page goes to another host.",
  ],
  navigate_tab: [
    "Go to a page in a tab",
    "An HTTP or HTTPS URL, in the open tab at another URL, with your cookies. The tab is left blank if the page goes to another host.",
  ],
  reload_tab: [
    "Reload a tab",
    "The open tab at a URL, with your cookies. The tab is left blank if the page goes to another host.",
  ],
  go_back: [
    "Go back in a tab",
    "The open tab at a URL, to the page before it in that tab. The tab is left blank if that page is on another host.",
  ],
  go_forward: [
    "Go forward in a tab",
    "The open tab at a URL, to the page after it in that tab. The tab is left blank if that page is on another host.",
  ],
  open_window: [
    "Open a page in a new window",
    "An HTTP or HTTPS URL, in a new window behind the one you are using, with your cookies. The window is closed if the page goes to another host.",
  ],
  restore_closed: [
    "Reopen a closed tab",
    "The recently closed tab that was at a URL, with your cookies. The tab is closed again if the page goes to another host.",
  ],
  search_web: [
    "Search the web",
    "The words given, sent to your default search provider in a background tab, with your cookies. The tab is closed if the results do not load as a web page.",
  ],
  wait_for_load: [
    "Wait for a tab to finish loading",
    "The open tab at a URL, for up to 20 seconds. It says whether the tab finished loading and nothing of the page.",
  ],
  read_page_links: [
    "List the links on an open page",
    "The text and web address of each link at a tab URL, except at blocked sites.",
  ],
  read_page_forms: [
    "List the forms on an open page",
    "Where each form at a tab URL submits, and the name and type of each field. What is typed in a field is not read.",
  ],
  list_page_elements: [
    "Number the elements of an open page",
    "A list of up to 40 buttons, links, fields and other controls at a tab URL, each with a number, its role, its name and where it goes, and no value that a field holds. It comes back to you and not to the model. You give the model a number from it, which names that element for an action you allow in Brave. The list is kept in memory for 5 minutes.",
  ],
  screenshot_page: [
    "Take a picture of an open page",
    "A JPEG of what the tab at a URL shows, if it is the one its window shows. It needs the group on all sites, and the picture comes back to you and not to the model.",
  ],
  capture_screen: [
    "Take a picture of your screen",
    "A JPEG of the screen or the window you choose in the browser's picker, which is shown for each call. Nothing is captured until you choose. The picture comes back to you and not to the model.",
  ],
  record_gif: [
    "Record an open page as a GIF",
    "Up to 20 pictures of the tab at a URL, if it is the one its window shows, at least 0.6 seconds apart, as an animated GIF no larger than 480 pixels on a side. It needs the group on all sites, and the GIF comes back to you and not to the model.",
  ],
  save_page_archive: [
    "Save a page as a file",
    "The page of the tab at a URL, with its pictures and frames, as an MHTML file in your Downloads folder. It saves only a page at a site under Allowed sites. The file is named for the site, a file already there is not replaced, and a page over 16 MiB is not saved. What comes back names the file and holds none of the page, and the model reads the file only if you give it.",
  ],
  focus_tab: [
    "Switch to a tab",
    "Make the tab at a URL the one you see, and bring its window forward.",
  ],
  pin_tab: ["Pin or unpin a tab", "The tab at a URL."],
  mute_tab: ["Mute or unmute a tab", "The tab at a URL."],
  move_tab: [
    "Move a tab",
    "The tab at a URL, to another position in its window.",
  ],
  group_tabs: [
    "Put tabs in a group",
    "The tabs at the URLs given, in one window, into a new tab group with the title given.",
  ],
  ungroup_tabs: [
    "Take tabs out of their groups",
    "The tabs at the URLs given.",
  ],
  update_tab_group: [
    "Change a tab group",
    "The title, color or collapsed state of the group the tab at a URL is in.",
  ],
  discard_tab: [
    "Unload a tab from memory",
    "The tab at a URL stays in the tab strip and loads again when you open it. What the page does not save can be lost.",
  ],
  duplicate_tab: [
    "Duplicate a tab",
    "Opens a copy of the tab at a URL, which loads that address again with your cookies.",
  ],
  set_zoom: [
    "Zoom a tab",
    "The zoom of the tab at a URL. Brave may apply it to other tabs at the same site.",
  ],
  focus_window: [
    "Bring a window forward",
    "The window that holds the tab at a URL, with its tabs as they are.",
  ],
  resize_window: [
    "Resize a window",
    "The width and height of the window that holds the tab at a URL, in pixels. Its position and tabs stay as they are, and Brave may keep a maximized or full screen window as it is.",
  ],
  list_cookie_names: [
    "List the cookies of a site",
    "The name, domain, path, flags and expiry of each cookie Brave sends to one exact URL, except at blocked sites. A cookie's value is never read.",
  ],
  delete_cookie: [
    "Delete a cookie",
    "The cookie with the name given that Brave sends to one exact URL, when it is the only one. No value is read and no cookie is set.",
  ],
  list_requests: [
    "List the requests of an open page",
    "The address, kind and status of each request a tab at one exact URL has made since it loaded. No headers, cookies or bodies.",
  ],
  get_privacy_settings: [
    "Read Brave's privacy settings",
    "Whether each of Brave's privacy settings, such as safe browsing, do not track and the third-party cookie setting, is on, and who controls it. Nothing is changed.",
  ],
  get_content_settings: [
    "Read what Brave allows a site to do",
    "Whether Brave allows, blocks or asks about cookies, scripts, images, location, notifications, camera and more at one exact URL, except at blocked sites.",
  ],
  get_font_settings: [
    "Read Brave's font settings",
    "The font chosen for each generic family and the font sizes. The fonts installed are not listed.",
  ],
  get_system_info: [
    "Read the computer's hardware",
    "The processor, memory, displays and storage devices: their names and sizes. How busy the processors are is not read.",
  ],
  get_idle_state: [
    "Check whether you are at the computer",
    "Whether you are active, idle after a number of seconds without input, or the screen is locked.",
  ],
  get_access_flags: [
    "Check where the extension is allowed to run",
    "Whether you allowed it on file: addresses and in private windows.",
  ],
  keep_awake: [
    "Keep the computer awake",
    "Stops the computer from sleeping, or the display as well, for the minutes asked, at most 240. It ends when they pass, or at once when 0 is asked.",
  ],
  create_bookmark: [
    "Add a bookmark",
    "An HTTP or HTTPS address, with a title if one is given, in the folder with the exact title given or where Brave puts new bookmarks.",
  ],
  create_bookmark_folder: [
    "Add a bookmark folder",
    "A folder with the title given, inside the folder with the exact title given or where Brave puts new folders.",
  ],
  move_bookmark: [
    "Move a bookmark",
    "The bookmark at an address, to the end of the folder with the exact title given.",
  ],
  rename_bookmark: [
    "Rename a bookmark",
    "The title of the bookmark at an address.",
  ],
  add_reading_list_entry: [
    "Add to the reading list",
    "An HTTP or HTTPS address with a title, unread unless the call says it was read.",
  ],
  update_reading_list_entry: [
    "Change a reading list entry",
    "The title, or whether you read it, of the entry at an address.",
  ],
  close_tab: [
    "Close a tab",
    "The tab at a URL. What the page has not saved is lost.",
  ],
  close_window: [
    "Close a window",
    "The window that holds the tab at a URL, with every tab in it. What the pages have not saved is lost. A window with a tab at a blocked site is not closed.",
  ],
  delete_history_entry: [
    "Delete a page from history",
    "The history entry at one exact URL, with every visit to it. It cannot be undone.",
  ],
  delete_history_range: [
    "Delete a period of history",
    "The pages visited between two times, both given, for every site including blocked ones. A page that was also visited outside the period is not removed. It cannot be undone.",
  ],
  clear_browsing_data: [
    "Clear browsing data",
    "The kinds of data named, such as cookies, cache or form data, that built up since a time given, for every site including blocked ones. It cannot be undone.",
  ],
  remove_bookmark: [
    "Delete a bookmark",
    "The one bookmark at an exact URL. It fails where more than one bookmark is at the URL. It cannot be undone.",
  ],
  remove_bookmark_folder: [
    "Delete a bookmark folder",
    "The one bookmark folder with an exact title, and only if it is empty. A folder with anything in it is left as it is. It fails where more than one folder has the title. It cannot be undone.",
  ],
  remove_reading_list_entry: [
    "Remove a reading list entry",
    "The reading list entry at an exact URL. It cannot be undone.",
  ],
  delete_downloaded_file: [
    "Delete a downloaded file",
    "The file of a finished download, from the disk, found by its exact path or the exact URL it came from. The download stays in the list with its file gone. It fails where the file is already gone or more than one download has the path or URL. It cannot be undone.",
  ],
  download_file: [
    "Download a file",
    "Downloads one HTTP or HTTPS address into your Downloads folder, with your cookies for that site, under the name Brave gives it. A file already there is not replaced. It is cancelled, and a file it had already saved is deleted, if the address goes to another host.",
  ],
  list_downloads: [
    "List downloads",
    "The path, address, state and size of your downloads, newest first, and whether each file is still on the disk. Downloads from blocked sites are left out.",
  ],
  pause_download: [
    "Pause a download",
    "The download in progress with an exact path or address. It fails where more than one download has it.",
  ],
  resume_download: [
    "Resume a download",
    "The paused or interrupted download with an exact path or address, if Brave can resume it. It fails where more than one has it.",
  ],
  cancel_download: [
    "Cancel a download",
    "The download in progress with an exact path or address. A finished download is left as it is and its file is not deleted.",
  ],
  show_download: [
    "Show a download in its folder",
    "Opens your file manager at the file of the download with an exact path or address. The file is not opened.",
  ],
  run_script: [
    "Run code in an open page",
    `Code the session wrote, run once in the top document of the tab at an exact URL, once you press Allow once in a window of this extension that shows the site, the address and every line of the code. Nothing limits what the code does except that you read it. It is at most ${CODE_LIMIT.toLocaleString("en-US")} characters, and what it answers comes back to you and not to the model.`,
  ],
  click_element: [
    "Click an element of an open page",
    "The one element at a tab URL with a role and an accessible name, or with a number from the list of the page, once you press Allow once in the window it opens. It sends mouse events the page can tell from yours, to a link, a button or any other element of the page.",
  ],
  type_text: [
    "Type in a field of an open page",
    "Replaces what the one text field with a role and an accessible name, or a number from the list of the page, holds, with text up to 2,000 characters, once you press Allow once in the window it opens. A password field, a payment card field and a file chooser are never typed in.",
  ],
  select_option: [
    "Choose an option in a list of an open page",
    "The option with the text given in the one list with a role and an accessible name, or a number from the list of the page, once you press Allow once in the window it opens.",
  ],
  press_key: [
    "Press a key in an open page",
    "Enter, Escape, Tab, an arrow, Home, End, Page Up or Page Down, alone, sent to the element that has the focus, once you press Allow once in the window it opens. The browser does not act on the key itself.",
  ],
  scroll_page: [
    "Scroll an open page",
    "The page at a tab URL, by up to 5 screens up or down, or to its top or bottom. It asks nothing in the browser.",
  ],
  wait_for_element: [
    "Wait for an element of an open page",
    "Up to 30 seconds for the page at a tab URL to show exactly one element with a role and an accessible name. It says whether it did and nothing of the element, and it acts on nothing.",
  ],
  click_element_trusted: [
    "Click an element of an open page, as you",
    "The one element at a tab URL with a role and an accessible name, with the left or right button, once or twice, once you press Allow once in the window the BraveBot Trusted Input extension opens. The click is a real one, which the page cannot tell from yours.",
  ],
  type_text_trusted: [
    "Type in a field of an open page, as you",
    "Replaces what the one text field with a role and an accessible name holds, with text up to 2,000 characters, once you press Allow once in the window the BraveBot Trusted Input extension opens. The typing is real, and a password field, a payment card field and a file chooser are never typed in.",
  ],
  press_key_trusted: [
    "Press a key in an open page, as you",
    "Enter, Escape, Tab, an arrow, Home, End, Page Up or Page Down, alone, on the element that has the focus, once you press Allow once in the window the BraveBot Trusted Input extension opens. The key press is a real one, so the browser acts on it.",
  ],
  hover_element: [
    "Hover over an element of an open page",
    "Moves the pointer over the one element at a tab URL with a role and an accessible name, once you press Allow once in the window the BraveBot Trusted Input extension opens. The page sees a real pointer, so a menu that opens on hover opens.",
  ],
  drag_element: [
    "Drag one element of an open page onto another",
    "Presses on the one element with a role and an accessible name, moves in steps and releases on the one element with a second role and name, once you press Allow once in the window the BraveBot Trusted Input extension opens. The window shows both elements.",
  ],
  screenshot_full_page: [
    "Take a picture of all of an open page",
    "A JPEG of the whole page at a tab URL, from the top to the bottom and the left to the right, including the parts out of view and any that cannot be seen, scaled to at most 2048 pixels on the long side, once you press Allow once in the window the BraveBot Trusted Input extension opens. The window shows the site and the size of the page. The picture comes back to you and not to the model.",
  ],
  read_console: [
    "Read the console of an open page",
    "The entries in the console of the tab at a URL: what the page's scripts wrote with console.log and the others, the errors they threw and what Brave reported about the page, such as a request that failed. Up to 200 of the newest, of the levels asked for, once you press Allow once in the window the BraveBot Trusted Input extension opens. The window shows the site. The text comes back to you and not to the model.",
  ],
  choose_file: [
    "Choose a file on a file input of an open page",
    "Gives a file you put in the uploads directory of the BraveBot browser directory, of up to 10 MiB and not named like a credential, to the one file input with an accessible name at a tab URL, once you press Allow once in the window the BraveBot Trusted Input extension opens. The window shows the site, the input, and the file's name, size and full path. Brave reads the file and gives it to the page, and a page whose script sends a file when it is chosen gets it at once.",
  ],
  take_held: [
    "Return what you held out",
    "The selections, links and tabs you held out with BraveBot in Brave's context menu, each with the page it was held on, and then forgets them. It takes no argument and reads no page.",
  ],
};

// What the page says of each group that has a tool. `risk` is the one line shown
// beside the switch. `does` and `returns` are the confirmation: what turning the
// group on lets a session do, and what comes back. A group is added here when
// its first tool is.
const GROUP_LABELS = {
  platform: {
    title: "Platform check",
    risk: "Tells a session the operating system and architecture Brave runs on, and nothing about you. It needs no permission.",
  },
  "tabs-read": {
    title: "Tabs",
    risk: "A session can ask for the title and address of every page you have open, and of the tabs you closed lately.",
    does: "List your open tabs, windows and tab groups, the tabs and windows you closed lately, and the frames inside a tab you name.",
    returns:
      "The title and address of each tab, and the addresses of a tab's frames. Windows and tab groups come back without their tabs. Sites on your blocked list are left out.",
  },
  "history-read": {
    title: "History",
    risk: "A session can search every page you have visited, and see the tabs open on your other devices.",
    does: "Search your browsing history by words in a page's title or address, up to 100 results a call. List the visits to one address, the sites you visit most, and the tabs open on your other devices.",
    returns:
      "The title, address and last visit time of each match, the time of each visit to an address, your most visited sites, and the title and address of each tab on each other device. Sites on your blocked list are left out.",
  },
  "bookmarks-read": {
    title: "Bookmarks",
    risk: "A session can search all your bookmarks, list a folder of them and your reading list, and see what you saved lately.",
    does: "Search your bookmarks by words in their title or address. List the bookmarks and folders in a folder you name, the bookmarks you added lately, and your reading list.",
    returns:
      "The title and address of each bookmark, when it was added, and for a reading list entry whether you read it. Sites on your blocked list are left out.",
  },
  "page-read": {
    title: "Page text",
    risk: "A session can read what is on a page you have open, including pages you are signed in to, and take a picture of it. It reaches only the sites you allow, unless you choose all sites, and a picture is taken only with all sites.",
    does: "Read the text of an open page, or of one frame in it, at an exact address, list the links and the forms of an open page, and number its buttons, links and fields so that you can tell the model which one to act on. With all sites, take a picture of the page its window shows, which includes whatever frames from other sites it holds.",
    returns:
      "Up to 100,000 characters of the page's text, with its title, or the text and address of the page's links, or where its forms submit with the name and type of each field and never what is typed in one, or up to 40 numbered controls with the role, name and destination of each and no value a field holds, kept in memory for 5 minutes, or a JPEG of the page as the window shows it. It reaches only the sites under Allowed sites, none until you add one, unless you choose all sites for the group; sites on your blocked list stay out of reach. A picture needs all sites for the group, because the browser lets an extension capture a tab only with access to all sites, and a site on your blocked list can still appear in a frame of a picture.",
  },
  "tabs-manage": {
    title: "Tab management",
    risk: "A session can switch to, pin, mute, move, group, unload, duplicate and zoom the tabs you have open, and bring their windows forward and resize them.",
    does: "Switch to, pin or unpin, mute or unmute, move, group, ungroup, unload from memory, duplicate and zoom an open tab, change the group it is in, and bring its window forward or set its size. A tab is named by its exact address.",
    returns:
      "The address of the tab and the state Brave reports for it, such as where it ended up, the title and color of its group or the size of its window. Sites on your blocked list stay out of reach.",
  },
  "bookmarks-write": {
    title: "Bookmark changes",
    risk: "A session can add bookmarks, folders and reading list entries, move and rename bookmarks, and change reading list entries. It cannot delete any.",
    does: "Add a bookmark at an HTTP or HTTPS address, or a folder, in a folder you name by its exact title. Move a bookmark to a folder, and rename it, naming it by its exact address. Add an entry to your reading list, and change an entry's title or whether you read it.",
    returns:
      "The address, title and folder Brave holds for the bookmark, the title and parent of a folder, or the title and read state of the entry. A bookmark, folder or entry that is not named by exactly one address or title is not changed. Nothing is deleted, and no bookmark at a site on your blocked list is added or changed.",
  },
  navigate: {
    title: "Open pages",
    risk: "A session can make Brave request a web address as you, with your cookies, in a new tab or window or in a tab you have open, and can search the web with your default search provider.",
    does: "Open one HTTP or HTTPS address in a background tab or a new window, or reopen a tab you closed lately. Take an open tab, named by its exact address, to another address, reload it, or move it back or forward a page. A tab or window that ends on another host than the one in the question is closed, or for a tab you had open, left blank. Search the web for words you give, in a background tab, and wait for an open tab to finish loading.",
    returns:
      "The address the page loaded, or for a wait whether the tab finished loading, and nothing from the page. A tab is named by its exact address, and a site on your blocked list is neither opened nor navigated to. A search can end on any web page your search provider sends it to, since the question shows no address, and the tab is closed if that page is at a blocked site.",
  },
  cookies: {
    title: "Cookies",
    risk: "A session can see the name of every cookie Brave sends to a site, which shows what you are signed in to, and can delete one. It cannot read or set a cookie's value.",
    does: "List the cookies Brave sends to one exact HTTP or HTTPS address, and delete the cookie with a name you give for an address. A cookie's value is never read, and no cookie is set.",
    returns:
      "The name, domain, path, flags and expiry of each cookie, and for a delete the domain and path of the cookie removed. It reaches only the sites under Allowed sites, none until you add one, unless you choose all sites for the group. A site on your blocked list is out of reach whatever the scheme or port.",
  },
  "browser-info": {
    title: "Browser and computer info",
    risk: "A session can read how Brave and the computer are set up and whether you are at it, and can keep the computer awake for the minutes shown in the question.",
    does: "Read Brave's privacy settings, what Brave allows a site to do at one address, its font settings, the computer's processor, memory, displays and storage devices, whether you are active, idle or the screen is locked, and whether the extension may run on file addresses and in private windows. Keep the computer, or its display, awake for up to 240 minutes.",
    returns:
      "The value of each setting and who controls it, the names and sizes of the hardware, the idle state, and two yes or no answers. Keeping awake gives back when it ends, and it ends by itself, or at once when 0 minutes are asked. Nothing else about Brave or the computer is changed. Content settings are not given for a site on your blocked list.",
  },
  "network-watch": {
    title: "Network requests",
    risk: "A session can see every address a page you have open has asked for since it loaded, including addresses that carry a token.",
    does: "List the requests an open page has made since it loaded. The page is named by its exact address.",
    returns:
      "The address, kind and status of each request, in the order they were made. No headers, cookies or bodies. Requests to or from sites on your blocked list are left out. A page that loaded before you turned this on, or before the extension last restarted, has none. It reaches only the pages at the sites under Allowed sites, none until you add one, unless you choose all sites for the group.",
  },
  downloads: {
    title: "Downloads",
    risk: "A session can make Brave download a web address as you, with your cookies, into your Downloads folder, can save a page you have open there as a file, and can list, pause, resume and cancel your downloads.",
    does: "Download one HTTP or HTTPS address into your Downloads folder under the name Brave gives it, never replacing a file that is there. Save an open page at an exact address, with its pictures and frames, as an MHTML file in your Downloads folder under a name made from its site, never replacing a file that is there. List your downloads, pause, resume or cancel one, and show a downloaded file in its folder. A download is named by its exact file path or address as the list shows them. A download that goes to another host is cancelled.",
    returns:
      "The path and address of a download, its state and size, and whether its file is still on the disk, or the name of the file a page was saved as, with none of the page in it. No tool reads a downloaded file. The file is outside BraveBot's workspace, which BraveBot does not trust, stays in your Downloads folder until you delete it, and any program of yours can read it. Downloads from sites on your blocked list are left out. A page is saved only from the sites under Allowed sites, none until you add one, and this group has no choice of all sites for saving. A site on your blocked list can still appear in a frame of a saved page.",
  },
  input: {
    title: "Click and type",
    risk: "A session can click, type, choose and press keys in a page you have open, including pages you are signed in to, as you. Each action opens a window in Brave that shows it and waits for you to press Allow once or Deny. It reaches only the sites you allow, unless you choose all sites.",
    does: "Click an element, type in a text field, choose an option in a list, press a key and scroll in an open page at an exact address, and wait for an element to show. An element is named by its role and its accessible name, which the extension looks up in the page, or by its number in the last list of the page that you read and gave the model. Before it clicks, types, chooses or presses a key, the extension brings the tab forward, outlines the element, and opens a window that shows the action, the page's address, the element as the page names it and any text to be typed. Nothing happens until you press Allow once in that window. Deny, closing the window, or no answer within 60 seconds stops it, and only one window is open at a time. Scrolling and waiting open no window. A password field, a payment card field and a file chooser are never acted on. The clicks and keys are synthetic: the page can tell them from yours, and some pages ignore them.",
    returns:
      "A word that says what was done and the page's origin, and nothing of the page or the element. It reaches only the sites under Allowed sites, none until you add one, unless you choose all sites for the group; sites on your blocked list stay out of reach.",
  },
  "input-trusted": {
    title: "Real clicks and typing",
    risk: "A session can click, double click, right click, hover, drag, type and press keys in a page you have open, including pages you are signed in to, with input the page cannot tell from yours. The BraveBot Trusted Input extension does it, and Brave lets that extension read and change data on all websites. Each action opens a window of that extension that shows it and waits for you to press Allow once or Deny. It reaches only the sites you allow.",
    does: "Click, double click, right click, hover over, drag onto another element and type in an element of an open page at an exact address, and press a key, with real mouse and keyboard input. An element is named by its role and its accessible name, which the BraveBot Trusted Input extension looks up in the page. This needs that extension to be installed and turned on in Brave, and it is a separate extension: Brave showed you its own install prompt then and shows none now. Before it acts, that extension brings the tab forward and opens a window that shows the action, the page's address and the element or elements as the page names them. Nothing happens until you press Allow once in that window. Deny, closing the window, or no answer within 60 seconds stops it, and only one window is open at a time. A password field, a payment card field and a file chooser are never acted on. If the page opens a dialog, you are asked how to answer it.",
    returns:
      "A word that says what was done and the page's origin, and nothing of the page or the elements. It reaches only the sites under Allowed sites, none until you add one, and there is no choice of all sites for this group; sites on your blocked list stay out of reach.",
  },
  "capture-trusted": {
    title: "Picture of a whole page",
    risk: "A session can take a picture of all of a page you have open, including pages you are signed in to, and the picture shows what the page shows in parts you cannot see now and in parts nobody sees. The BraveBot Trusted Input extension does it, and Brave lets that extension read and change data on all websites. Each picture opens a window of that extension that shows the page and waits for you to press Allow once or Deny. It reaches only the sites you allow.",
    does: "Take a picture of the whole top document of an open page at an exact address, from its top to its bottom and its left to its right, including the parts out of view and a tab in the background, scaled down to at most 2048 pixels on the long side. This needs that extension to be installed and turned on in Brave, and it is a separate extension: Brave showed you its own install prompt then and shows none now. Before it takes the picture, that extension brings the tab forward and opens a window that shows the site, the page's address and the size of the page. Nothing is taken until you press Allow once in that window. Deny, closing the window, or no answer within 60 seconds stops it, and only one window is open at a time. A frame is in the picture as the page draws it.",
    returns:
      "A picture, and nothing else about the page. It comes back to you and not to the model: the model reads it only through a processor that answers a question about it, or after you have looked at it and agreed. A picture over 8 MiB is refused. It reaches only the sites under Allowed sites, none until you add one, and there is no choice of all sites for this group; sites on your blocked list stay out of reach.",
  },
  "console-trusted": {
    title: "Console of a page",
    risk: "A session can read the console of a page you have open, including pages you are signed in to. The console holds whatever the page's scripts printed, which can include data from your account, and the errors Brave reported for the page. The BraveBot Trusted Input extension does it, and Brave lets that extension read and change data on all websites. Each read opens a window of that extension that shows the page and waits for you to press Allow once or Deny. It reaches only the sites you allow.",
    does: "Read the entries in the console of an open page at an exact address: the calls of console.log and the others, the errors a script threw and what Brave reported about the page, such as a request that failed or a rule of the page that was broken, of the levels asked for and up to 200 of the newest. This needs that extension to be installed and turned on in Brave, and it is a separate extension: Brave showed you its own install prompt then and shows none now. Before it reads, that extension opens a window that shows the site and the page's address. Nothing is read until you press Allow once in that window, and Brave's notice that the extension is debugging the tab shows while it reads. Deny, closing the window, or no answer within 60 seconds stops it, and only one window is open at a time. A script that threw an error appears without the error's message.",
    returns:
      "The entries as text, one for each line, each with its level, where it came from, its text up to 500 characters, the address of its script and its line. The text is the page's, and BraveBot does not give it to the model: the model reads it only through a processor that answers a question about it, or after you have read the exact text and agreed. It reaches only the sites under Allowed sites, none until you add one, and there is no choice of all sites for this group; sites on your blocked list stay out of reach.",
  },
  "upload-trusted": {
    title: "Files to a page",
    risk: "A session can give a file of yours to a page you have open, including pages you are signed in to, and the page can send it on at once. The BraveBot Trusted Input extension does it, and Brave lets that extension read and change data on all websites. Each file opens a window of that extension that shows the page, the input and the file and waits for you to press Allow once or Deny. It reaches only the sites you allow, and only files you put in one directory.",
    does: "Choose a file on the one file input with an accessible name of an open page at an exact address, so that the page receives it as if you had chosen it in the browser's file picker. The file is one in the uploads directory of the directory the BraveBot browser server was declared with, of up to 10 MiB, and not named like a credential, a key or a secret; a link that leads out of the directory, a directory and any other path are refused before anything is sent. This needs that extension to be installed and turned on in Brave, and it is a separate extension: Brave showed you its own install prompt then and shows none now. Before it chooses the file, that extension opens a window that shows the site, the page's address, the file input as the page names it, and the file's name, size and full path, and says that Brave reads the file and gives it to the page. Nothing is chosen until you press Allow once in that window. Deny, closing the window, or no answer within 60 seconds stops it, and only one window is open at a time. A file input the page hides is chosen on as well, if the page's own label, aria-label or title names it, or its id or name where nothing else does, and the window says that the page hides it. Choosing a file sends nothing by itself, but a page whose script sends the file when it is chosen receives it as soon as you press Allow once.",
    returns:
      "A word that says the file was chosen and the page's origin, and nothing of the page, the input or the file. It reaches only the sites under Allowed sites, none until you add one, and there is no choice of all sites for this group; sites on your blocked list stay out of reach.",
  },
  screen: {
    title: "Screen",
    risk: "A session can take a picture of your screen or of a window of any program, which holds whatever is on it, including what is not a page of Brave. Each picture is chosen by you in the browser's picker. With all sites, a session can also record a page you have open as an animated GIF.",
    does: "Take one picture of the screen or the window you choose in the picker the browser shows for each call. Nothing is captured until you choose, and closing the picker or waiting 60 seconds ends the call. With all sites for the group, record up to 20 pictures of the page its window shows, at an exact address, as an animated GIF.",
    returns:
      "A JPEG of the screen or window you chose, or a GIF of the page, and nothing else. The model does not see either: BraveBot holds them as pictures you can open, and the model reads one only through a processor or after you looked at it and agreed. A recording needs all sites for the group, because the browser lets an extension capture a tab only with access to all sites; the picture of a screen needs no site. A site on your blocked list can still appear in a frame of the page or on the screen.",
  },
  scripts: {
    title: "Page scripts",
    risk: "A session can run JavaScript it wrote in a page you have open, including pages you are signed in to, as you. Each run opens a window in Brave that shows the address and all of the code, and waits for you to press Allow once or Deny. Nothing limits what the code does except that you read it: it can read everything on the page, click and submit its forms, and send what it read to another address. Timers, listeners and observers it sets keep running in the page until the page is closed or reloaded, and each can do the same without asking again. It reaches only the sites you allow. Brave also has to have Allow User Scripts turned on for this extension, which only you can do.",
    does: `Run code a session wrote in the top document of an open page at an exact address, in a world apart from the page's own scripts. The browser stops the code's own requests by fetch, XMLHttpRequest, sendBeacon and WebSocket there, and does not stop it loading a frame, going to an address or submitting a form, and a click in the page lets it open a window. The extension checks that the browser applied that policy to the world before it asks you, and refuses the call when it did not, which Brave does after ten calls on one page: reload the page and ask again. The code starts once. Timers, listeners and observers it sets keep running in the page until the page is closed or reloaded, and each can read and send without asking again. Before it runs, the extension opens a window that shows the site, the address and every line of the code, with the characters you cannot see written out, and says that nothing limits the code except that you read it. Nothing runs until you press Allow once in that window. Deny, closing the window, or no answer within 60 seconds stops it, and only one window is open at a time. The code is at most ${CODE_LIMIT.toLocaleString("en-US")} characters, and a longer one is refused and never cut. It runs only while Allow User Scripts is on for this extension, and run_script is refused until it is.`,
    returns: `What the code answered, as text of up to ${RESULT_LIMIT.toLocaleString("en-US")} characters, and the page's origin. It reaches only the sites under Allowed sites, none until you add one, and there is no choice of all sites for this group; sites on your blocked list stay out of reach.`,
  },
  held: {
    title: "Held items",
    risk: "A session can fetch the text you select, the links and the tabs you hold out with BraveBot in Brave's context menu. It reads no page, and it gets only what you hold out. A selection can include text the page hides.",
    does: "Fetch what you held out with Hold this selection, Hold this link and Hold this tab for BraveBot, entries this group adds to Brave's context menu. Brave gives the extension the text you selected, the address of the link or the address of the tab, and the extension reads nothing from the page. Up to 10 items are kept, in memory only, for 30 minutes. They are forgotten when they are fetched, when this group is turned off and when Brave restarts. A fetch takes no argument and names no page.",
    returns:
      "For each item its kind, the origin of the page it was held on, and the text selected, up to 20,000 characters, or the address, up to 2,000. An item held at a site on your blocked list is left out. The text is the page's, whoever selected it, and BraveBot does not give it to the model: the model reads it only through a processor that answers a question about it, or after you have read the exact text and agreed.",
  },
  "remove-data": {
    title: "Delete data",
    risk: "A session can close your tabs and windows and delete history, bookmarks and browsing data. Deleted data cannot be brought back.",
    does: "Close the tab at an exact address, or the window that holds it. Delete the history entry for an exact address, or the history between two times. Clear the kinds of browsing data named, such as cookies, cache or form data, since a time given. Delete the one bookmark at an exact address, or the one empty bookmark folder with an exact title. Remove the reading list entry at an exact address. Delete the file of a finished download from the disk.",
    returns:
      "The address, folder title or file path, the number of tabs or visits affected, and whether Brave reports the item gone. Deleting by time or by kind covers every site, including those on your blocked list. A window with a tab at a blocked site is not closed, and a file downloaded from one is not deleted.",
  },
};

// What the page says of each preset: its title and the line shown beside its
// button.
const PRESET_LABELS = {
  off: {
    title: "Off",
    line: "Only the platform check is on. Every other group is turned off.",
  },
  look: {
    title: "Look",
    line: "Tabs, history and bookmarks can be listed and searched. No page is read.",
  },
  read: {
    title: "Read",
    line: "Look, and the text of pages you have open at the sites you allow can be read. No picture of a page is taken, since that needs all sites.",
  },
  browse: {
    title: "Browse",
    line: "Read, tabs can be switched to, pinned, muted, moved, grouped, unloaded, duplicated and zoomed, bookmarks and reading list entries can be added and changed, a page can be opened in a new tab or window, an open tab taken to another page, and the web searched. No picture of a page is taken, since that needs all sites.",
  },
};

// What the confirmation of a group and of a preset say about the result.
const NOT_TO_THE_MODEL =
  "BraveBot shows what comes back to you and does not give it to the model. You choose what to tell it.";

// The sentence for the warnings in a list, or the one for none. A warning that
// ends with a period, as Brave's for contentSettings does, loses it to the
// separator.
function allowLine(warnings) {
  return warnings.length > 0
    ? `Brave asks you to allow: ${warnings.map((line) => line.replace(/\.$/, "")).join("; ")}.`
    : "Brave shows no prompt of its own.";
}

function element(tag, className, text) {
  const made = document.createElement(tag);
  if (className) {
    made.className = className;
  }
  if (text !== undefined) {
    made.textContent = text;
  }
  return made;
}

function button(name, text) {
  const made = element("button", "", text);
  made.type = "button";
  made.name = name;
  return made;
}

function checkbox(name) {
  const made = document.createElement("input");
  made.type = "checkbox";
  made.name = name;
  return made;
}

function radio(name, value) {
  const made = document.createElement("input");
  made.type = "radio";
  made.name = name;
  made.value = value;
  return made;
}

// The switch and the note of one tool, inside its group.
function toolRow(name) {
  const [title, note] = LABELS[name];
  const box = checkbox(name);
  box.addEventListener("change", () => saveTool(name, box));
  const label = element("label");
  label.append(box, ` ${title}`, element("div", "note", note));
  return { name, box, label };
}

// What each group's controls are, by id, so that a change in what the browser
// holds or what is saved can redraw them.
const parts = new Map();

// What Brave says when an organization's policy blocks the permissions the
// extension asks for (ExtensionSettings blocked_permissions). A group whose
// request fails so is drawn as blocked by the organization and its switch stays
// off until the page is opened again, so the browser is not asked for it twice
// by this page. Any other failure is shown as Brave words it.
const POLICY_BLOCKED = "Permissions are blocked by enterprise policy.";
const BLOCKED_LINE =
  "Blocked by your organization: Brave says permissions are blocked by enterprise policy, so this group stays off. Ask whoever manages this browser, and open this page again after the policy changes.";
const blockedByOrganization = new Set();

// The allowed sites and group scopes as the page last read them. A click asks
// the browser for what these say a group needs, and as its first call, so the
// click does not read them again.
let access = NO_ACCESS;

// The words for each scope of a group that reads pages.
const SCOPE_LABELS = {
  [SCOPE_ALLOWED]: "Only the allowed sites",
  [SCOPE_ALL]: "All sites except the blocked ones",
};

// Whether the group has a tool that saves a page, which asks the browser for
// pageCapture.
function savesPages(id) {
  return GROUPS[id].permissions.includes("pageCapture");
}

// What the note under a group that saves pages says of the sites. The group has
// no scope, and the permission Brave is asked for names no site and warns as
// access to all websites, so the browser would let the extension save any open
// page, and saving is limited to the allowed sites by the extension.
function savingLine() {
  const count = access.allowed.length;
  const sites =
    count === 0
      ? "No site is allowed yet, so a page is not saved until you add one under Allowed sites."
      : `A page is saved only from the ${count === 1 ? "1 site" : `${count} sites`} under Allowed sites.`;
  return `${sites} Brave lets the extension save any open page once it has the permission to save pages, which names no site and which Brave words as access to all websites, so saving a page is limited to those sites by this extension and not by Brave. The group also holds the permission to read the title and address of your open tabs, which the tool needs to find the page by its address, and Brave shows no line of its own for it beside the one for all websites.`;
}

// What the sites line under a group's scope says now. A group with a tool that
// takes pictures says that the picture needs all sites.
function sitesLine(id, scope) {
  const pictures = capturesPages(id)
    ? ` A picture of a page is ${scope === SCOPE_ALL ? "taken only with this choice" : "refused with this choice"}, because the browser lets an extension capture a tab only with access to all sites.`
    : "";
  if (scope === SCOPE_ALL) {
    return `Every site is within reach except the ones under Blocked sites.${pictures}`;
  }
  if (access.allowed.length === 0) {
    return `No site is allowed yet, so a session reaches no page until you add one under Allowed sites.${pictures}`;
  }
  return `A session reaches only the ${access.allowed.length === 1 ? "1 site" : `${access.allowed.length} sites`} under Allowed sites.${pictures}`;
}

// How the person turns on the switch Brave keeps for running code, which an
// extension cannot turn on. Chrome 138 moved it from Developer mode to a toggle of
// the extension's own details page.
const ALLOW_USER_SCRIPTS =
  "Brave lets an extension run code only while Allow User Scripts is on for it, and only you can turn that on. In Brave built on Chrome 138 or later, open brave://extensions, choose Details for BraveBot and turn on Allow User Scripts. In an older Brave, turn on Developer mode at brave://extensions.";

// What the page says of that switch under a group that runs code. The browser
// shows the extension its API only once the group's permission is held, so a group
// that is off can say how to turn the switch on and not whether it is on.
async function userScriptsLine(on) {
  if (!on) {
    return ALLOW_USER_SCRIPTS;
  }
  return (await userScriptsAvailable(chrome))
    ? "Allow User Scripts is on for this extension, so run_script can run once you allow it."
    : `Allow User Scripts is off for this extension, so run_script is refused. ${ALLOW_USER_SCRIPTS}`;
}

// The choice of sites a group that reads pages reaches: one radio for each
// scope and a line on what that means now. Choosing one changes the scope at
// once, asking the browser first for what the group then needs if it is on.
function renderScope(id) {
  const fieldset = element("fieldset", "scope");
  fieldset.append(element("legend", "", "Sites it may reach"));
  const radios = {};
  for (const scope of [SCOPE_ALLOWED, SCOPE_ALL]) {
    const choice = radio(`scope:${id}`, scope);
    choice.addEventListener("change", () => chooseScope(id, scope));
    radios[scope] = choice;
    const label = element("label");
    label.append(choice, ` ${SCOPE_LABELS[scope]}`);
    fieldset.append(label);
  }
  const line = element("div", "note");
  const status = element("div", "note");
  status.setAttribute("role", "status");
  fieldset.append(line, status);
  return { fieldset, radios, line, status };
}

// The words for each lifetime a group can be turned on for.
const LIFETIME_LABELS = {
  [UNTIL_TURNED_OFF]: "Until I turn it off",
  [UNTIL_RESTART]: "Until Brave restarts",
  [ONE_HOUR]: "For one hour",
};

// The choice of how long a group stays on: one radio for each lifetime, with
// the default of the group chosen.
function renderLifetime(id) {
  const fieldset = element("fieldset", "lifetime");
  fieldset.append(element("legend", "", "How long it stays on"));
  const radios = {};
  for (const lifetime of LIFETIMES) {
    const choice = radio(`lifetime:${id}`, lifetime);
    choice.checked = lifetime === defaultLifetime(id);
    radios[lifetime] = choice;
    const label = element("label");
    label.append(choice, ` ${LIFETIME_LABELS[lifetime]}`);
    fieldset.append(label);
  }
  return { fieldset, radios };
}

// What the page says of a group that is on until a time or until Brave
// restarts, and nothing for one that stays on until it is turned off.
function endsLine(record) {
  if (record?.lifetime === UNTIL_RESTART) {
    return "On until Brave restarts.";
  }
  if (record?.lifetime === ONE_HOUR && Number.isFinite(record.expires)) {
    const at = new Date(record.expires).toLocaleTimeString([], {
      hour: "numeric",
      minute: "2-digit",
    });
    return `On until ${at}.`;
  }
  return "";
}

// Draws a group: its switch, its risk, what Brave will ask, the confirmation
// that opens when the switch is turned on, and its tools.
function renderGroup(id) {
  const label = GROUP_LABELS[id];
  const always = id === ALWAYS_ON;
  const box = checkbox(`group:${id}`);
  const title = element("label", "group-title");
  title.append(box, ` ${label.title}`);
  const block = element("section", "group");
  block.dataset.group = id;
  block.append(title, element("div", "note", label.risk));
  const asks = element("div", "note");
  const ends = element("div", "note");
  const blocked = element("div", "blocked");
  blocked.setAttribute("role", "status");
  block.append(asks, ends, blocked);
  const scope =
    GROUPS[id].hosts && !GROUPS[id].allowedOnly ? renderScope(id) : null;
  if (scope) {
    block.append(scope.fieldset);
  }
  const sites =
    GROUPS[id].allowedOnly || savesPages(id) ? element("div", "note") : null;
  if (sites) {
    block.append(sites);
  }
  const userScripts = GROUPS[id].permissions.includes("userScripts")
    ? element("div", "note")
    : null;
  if (userScripts) {
    block.append(userScripts);
  }

  const confirm = element("div", "confirm");
  confirm.hidden = true;
  const allow = button(`allow:${id}`, "Turn on");
  const cancel = button(`cancel:${id}`, "Cancel");
  const lifetime = always ? null : renderLifetime(id);
  if (!always) {
    confirm.append(
      element("div", "", `Turn on ${label.title.toLowerCase()}?`),
      element("div", "", `This lets a session: ${label.does}`),
      element("div", "", `What comes back: ${label.returns}`),
      element("div", "", NOT_TO_THE_MODEL),
      lifetime.fieldset,
      allow,
      cancel,
    );
  }
  block.append(confirm);

  const tools = GROUPS[id].tools.map(toolRow);
  const details = element("details");
  details.append(
    element(
      "summary",
      "",
      tools.length === 1 ? "1 tool" : `${tools.length} tools`,
    ),
    ...tools.map((tool) => tool.label),
  );
  const status = element("div", "note");
  status.setAttribute("role", "status");
  block.append(details, status);

  box.addEventListener("change", () => switchGroup(id));
  allow.addEventListener("click", () => allowGroup(id));
  cancel.addEventListener("click", () => {
    confirm.hidden = true;
    status.textContent = "";
    box.focus();
  });
  parts.set(id, {
    box,
    asks,
    ends,
    blocked,
    scope,
    sites,
    userScripts,
    lifetime,
    confirm,
    allow,
    cancel,
    status,
    tools,
  });
  return block;
}

// The controls of the presets: the preset waiting for the person's answer, the
// buttons that answer it and the one block that holds the confirmation.
const presetParts = {
  pending: null,
  allow: null,
  cancel: null,
  buttons: new Map(),
};

// Draws the presets: a button and a line for each, and the confirmation that
// opens when one other than Off is chosen.
function renderPresets() {
  const list = document.getElementById("presets");
  for (const name of Object.keys(PRESETS)) {
    const label = PRESET_LABELS[name];
    const choose = button(`preset:${name}`, label.title);
    choose.addEventListener("click", () => choosePreset(name));
    const row = element("div", "preset");
    row.append(choose, element("span", "note", label.line));
    list.append(row);
    presetParts.buttons.set(name, choose);
  }
  presetParts.allow = button("apply-preset", "Turn on");
  presetParts.cancel = button("cancel-preset", "Cancel");
  presetParts.allow.addEventListener("click", () =>
    finishPreset(presetParts.pending),
  );
  presetParts.cancel.addEventListener("click", () => {
    document.getElementById("preset-confirm").hidden = true;
    document.getElementById("presets-status").textContent = "";
    presetParts.buttons.get(presetParts.pending)?.focus();
    presetParts.pending = null;
  });
  document.getElementById("preset-confirm").hidden = true;
}

// A preset that turns nothing on, as Off does, asks the browser for nothing and
// is applied at once. Any other first shows what each group it turns on lets a
// session do and what comes back, what Brave will ask for them together, and
// the groups it turns off, and waits for the click on Turn on.
async function choosePreset(name) {
  const status = document.getElementById("presets-status");
  status.textContent = "";
  if (presetGroups(name).length === 0) {
    await finishPreset(name);
    return;
  }
  let switches;
  try {
    switches = await groupSwitches(chrome);
  } catch (error) {
    status.textContent = `The switches cannot be read: ${error.message}`;
    return;
  }
  drawPresetConfirm(name, switches);
  presetParts.pending = name;
  document.getElementById("preset-confirm").hidden = false;
  presetParts.allow.focus();
}

// Draws the confirmation of the preset from the switches and the allowed sites
// as they are now. It is drawn again when they change before the person answers,
// so the lines and the groups named are the ones the request will be made for.
function drawPresetConfirm(name, switches) {
  const on = presetGroups(name);
  const off = Object.keys(switches).filter((id) => !on.includes(id));
  const confirm = document.getElementById("preset-confirm");
  confirm.textContent = "";
  confirm.append(
    element("div", "", `Turn on ${PRESET_LABELS[name].title}?`),
    element(
      "div",
      "",
      `This turns on: ${on.map((id) => GROUP_LABELS[id].title).join(", ")}.`,
    ),
  );
  for (const id of on) {
    const label = GROUP_LABELS[id];
    confirm.append(
      element("div", "", `${label.title}. This lets a session: ${label.does}`),
      element("div", "", `What comes back: ${label.returns}`),
    );
  }
  confirm.append(element("div", "", NOT_TO_THE_MODEL));
  if (off.length > 0) {
    confirm.append(
      element(
        "div",
        "",
        `This turns off: ${off.map((id) => GROUP_LABELS[id]?.title ?? id).join(", ")}.`,
      ),
    );
  }
  const widened = on.filter((id) => scopeOf(id, access) === SCOPE_ALL);
  if (widened.length > 0) {
    confirm.append(
      element(
        "div",
        "",
        `This limits ${widened.map((id) => GROUP_LABELS[id].title).join(", ")} to the allowed sites, and gives back the access to all sites.`,
      ),
    );
  }
  confirm.append(
    element(
      "div",
      "",
      allowLine(
        warningsFor(neededTogether(on, GROUPS, limitedToAllowed(on, access))),
      ),
    ),
    presetParts.allow,
    presetParts.cancel,
  );
}

// The person confirmed, or chose Off. The request to the browser is made first,
// in this click, since the browser accepts it only from a click.
async function finishPreset(name) {
  const status = document.getElementById("presets-status");
  const applying = applyPreset(chrome, name, access);
  presetParts.allow.disabled = true;
  presetParts.cancel.disabled = true;
  try {
    const applied = await applying;
    status.textContent = applied
      ? `${PRESET_LABELS[name].title} is applied.`
      : "Not applied: Brave did not grant what the groups need.";
    document.getElementById("preset-confirm").hidden = applied;
    if (applied) {
      presetParts.pending = null;
    }
  } catch (error) {
    status.textContent = `Brave reported an error: ${error.message}. The switches below show what is on.`;
  }
  presetParts.allow.disabled = false;
  presetParts.cancel.disabled = false;
  await refreshAndSay();
}

// The groups that have a tool, in the order of the table.
async function render() {
  renderPresets();
  const form = document.getElementById("groups");
  for (const [id, group] of Object.entries(GROUPS)) {
    if (group.tools.length > 0) {
      form.append(renderGroup(id));
    }
  }
  await refresh();
  chrome.permissions.onAdded.addListener(refreshAndSay);
  chrome.permissions.onRemoved.addListener(refreshAndSay);
  // The log changes with every call, and the switches do not.
  chrome.storage.onChanged.addListener((changes) => {
    if (Object.keys(changes).some((key) => key !== ACTIVITY_KEY)) {
      refreshAndSay();
    }
  });
}

// Draws every switch as it stands: a group is on when its switch is saved and
// the browser holds what it needs, and its tools can be changed only then. What
// Brave will ask, and the scope of a group that reads pages, follow the sites
// that are allowed.
async function refresh() {
  const [current, switches, sites, records] = await Promise.all([
    settings(chrome),
    groupSwitches(chrome),
    siteAccess(chrome),
    lifetimeRecords(chrome),
  ]);
  access = sites;
  if (presetParts.pending !== null && !presetParts.allow.disabled) {
    drawPresetConfirm(presetParts.pending, switches);
  }
  for (const [id, part] of parts) {
    const on = await isOn(chrome, id, switches, access);
    const blocked = !on && blockedByOrganization.has(id);
    part.box.checked = on;
    part.box.disabled = id === ALWAYS_ON || blocked;
    part.blocked.textContent = blocked ? BLOCKED_LINE : "";
    part.asks.textContent = allowLine(
      warningsFor(needed(id, GROUPS, access)),
    );
    part.ends.textContent = on ? endsLine(records?.[id]) : "";
    if (part.scope) {
      const scope = scopeOf(id, access);
      for (const [value, choice] of Object.entries(part.scope.radios)) {
        choice.checked = value === scope;
      }
      part.scope.line.textContent = sitesLine(id, scope);
    }
    if (part.sites) {
      part.sites.textContent = savesPages(id)
        ? savingLine()
        : sitesLine(id, SCOPE_ALLOWED);
    }
    if (part.userScripts) {
      part.userScripts.textContent = await userScriptsLine(on);
    }
    for (const tool of part.tools) {
      tool.box.checked = current[tool.name] === true;
      tool.box.disabled = !on;
    }
  }
}

async function refreshAndSay() {
  const status = document.getElementById("groups-status");
  try {
    await refresh();
    status.textContent = "";
  } catch (error) {
    status.textContent = `The switches cannot be read: ${error.message}`;
  }
}

// The person turned a group's switch. Turning it on does not ask the browser
// yet: the switch stays off and the group says what it will do, and the click
// on Turn on is what asks. Turning it off takes the group's permissions back.
async function switchGroup(id) {
  const part = parts.get(id);
  if (part.box.checked) {
    part.box.checked = false;
    part.status.textContent = "";
    part.confirm.hidden = false;
    part.allow.focus();
    return;
  }
  part.box.disabled = true;
  try {
    await turnOff(chrome, id);
    part.status.textContent = "";
  } catch (error) {
    part.status.textContent = `Turned off, but Brave did not take its permissions back: ${error.message}`;
  }
  await refreshAndSay();
}

// The person confirmed, choosing how long the group stays on. The request to the
// browser is made first, in this click, since the browser accepts it only from a
// click.
async function allowGroup(id) {
  const part = parts.get(id);
  const lifetime = LIFETIMES.find((name) => part.lifetime.radios[name].checked);
  const turning = turnOn(chrome, id, access, lifetime);
  part.allow.disabled = true;
  part.cancel.disabled = true;
  try {
    const granted = await turning;
    part.status.textContent = granted
      ? ""
      : "Not turned on: Brave did not grant what the group needs.";
    part.confirm.hidden = granted;
  } catch (error) {
    if (String(error?.message).includes(POLICY_BLOCKED)) {
      blockedByOrganization.add(id);
      part.status.textContent = "";
      part.confirm.hidden = true;
    } else {
      part.status.textContent = `Not turned on: ${error.message}`;
    }
  }
  part.allow.disabled = false;
  part.cancel.disabled = false;
  await refreshAndSay();
}

// The person chose a scope for a group that reads pages. The request to the
// browser, for a group that is on, is the first call of this click.
async function chooseScope(id, scope) {
  const part = parts.get(id);
  const { radios, status } = part.scope;
  const changing = setScope(chrome, id, scope, access, part.box.checked);
  Object.values(radios).forEach((choice) => {
    choice.disabled = true;
  });
  try {
    status.textContent = (await changing)
      ? ""
      : "Not changed: Brave did not grant what the group needs.";
  } catch (error) {
    status.textContent = `The choice shown is the one saved. Brave reported an error: ${error.message}`;
  }
  Object.values(radios).forEach((choice) => {
    choice.disabled = false;
  });
  await refreshAndSay();
}

// Saves one tool's switch beside the others. A save that fails is said and the
// switch is drawn as it stands.
async function saveTool(name, box) {
  let failure = "";
  try {
    await setTool(chrome, name, box.checked);
  } catch (error) {
    failure = `Not saved: ${error.message}`;
  }
  await refreshAndSay();
  if (failure) {
    document.getElementById("groups-status").textContent = failure;
  }
}

// A list that cannot be read shows as empty, and the next save replaces it.
async function renderAllowed() {
  const field = document.getElementById("allowed-sites");
  const status = document.getElementById("allowed-status");
  try {
    field.value = (await allowedSites(chrome)).join("\n");
  } catch {
    status.textContent = "The saved list cannot be read. Saving replaces it.";
  }
  field.addEventListener("input", () => {
    status.textContent =
      "Not saved yet. Choose Save allowed sites to ask Brave for access to them and save the list.";
  });
  document
    .getElementById("save-allowed")
    .addEventListener("click", saveAllowed);
}

// Asks Brave for access to the sites written, as the first call of the click,
// and saves them if it grants it. The list saved before stays in force where
// Brave does not grant it or a line is not one address, and the sites a
// person took off the list are given back to Brave. The sites are written back
// as the origins that were kept, unless more was typed while the save ran.
async function saveAllowed() {
  const field = document.getElementById("allowed-sites");
  const status = document.getElementById("allowed-status");
  const save = document.getElementById("save-allowed");
  const written = field.value;
  const saving = allowSites(chrome, written);
  save.disabled = true;
  try {
    const { sites, rejected, granted, notTakenBack } = await saving;
    if (rejected.length > 0) {
      status.textContent = `Not saved: ${rejected.join(", ")}. A site is one http:// or https:// address, such as https://news.example.com. The list saved before still applies.`;
    } else if (!granted) {
      status.textContent =
        "Not saved: Brave did not grant access to the sites. The list saved before still applies.";
    } else {
      status.textContent = notTakenBack
        ? `Saved, but Brave did not take back access to a site you removed: ${notTakenBack}`
        : "Saved.";
      if (field.value === written) {
        field.value = sites.join("\n");
      }
    }
  } catch (error) {
    status.textContent = `Not saved: ${error.message}`;
  }
  save.disabled = false;
}

// A list that cannot be read shows as empty, and the next save replaces it.
async function renderBlocked() {
  const field = document.getElementById("blocked-sites");
  try {
    field.value = (await blockedSites(chrome)).join("\n");
  } catch {
    document.getElementById("blocked-status").textContent =
      "The saved list cannot be read. Saving replaces it.";
  }
  field.addEventListener("input", () => {
    document.getElementById("blocked-status").textContent =
      "Not saved yet. Click outside the box to save it.";
  });
  field.addEventListener("change", saveBlocked);
}

// Saves the sites written when the person leaves the box, and writes them back
// as the origins that were kept, unless more was typed while the save ran. A
// save on every keystroke would take a site off the list while its line is
// being edited. A line that is not one HTTP or HTTPS address stops the save and
// is named, and the list saved before stays in force.
async function saveBlocked() {
  const field = document.getElementById("blocked-sites");
  const status = document.getElementById("blocked-status");
  const written = field.value;
  try {
    const { sites, rejected } = await saveBlockedSites(chrome, written);
    if (rejected.length > 0) {
      status.textContent = `Not saved: ${rejected.join(", ")}. A site is one http:// or https:// address, such as https://mail.example.com. The list saved before still applies.`;
      return;
    }
    status.textContent = "Saved.";
    if (field.value === written) {
      field.value = sites.join("\n");
    }
  } catch (error) {
    status.textContent = `Not saved: ${error.message}`;
  }
}

// One row of the log: when the call arrived, its group and tool, what came of
// it, and the sites of the URLs it named. A call that named no group, tool or
// site says none.
function activityRow(entry) {
  const row = element("tr");
  const time = element("time", "", new Date(entry.time).toLocaleString());
  time.setAttribute("datetime", new Date(entry.time).toISOString());
  const cell = (...content) => {
    const made = element("td");
    made.append(...content);
    return made;
  };
  row.append(
    cell(time),
    cell(entry.group ?? "none"),
    cell(entry.tool ?? "none"),
    cell(entry.outcome),
    cell(entry.origins.length > 0 ? entry.origins.join(", ") : "none"),
  );
  return row;
}

// Draws the log, newest first, and again whenever a call is added to it.
async function renderActivity() {
  const rows = document.getElementById("activity-rows");
  const status = document.getElementById("activity-status");
  try {
    const log = await activityLog(chrome);
    rows.textContent = "";
    rows.append(...log.toReversed().map(activityRow));
    status.textContent =
      log.length === 0 ? "No call has reached the extension yet." : "";
  } catch (error) {
    status.textContent = `The activity cannot be read: ${error.message}`;
  }
}

function followActivity() {
  chrome.storage.onChanged.addListener((changes, area) => {
    if (area === "local" && ACTIVITY_KEY in changes) {
      renderActivity();
    }
  });
  return renderActivity();
}

render();
renderAllowed();
renderBlocked();
followActivity();
