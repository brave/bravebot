# Proposal: BraveBot access to the browser through an extension

## Goal

Let BraveBot, running locally, visit pages, read their content, and
query profile data such as tabs, history and bookmarks, from the
user's real profile.

## Why an extension

The extension runs in the user's real profile and can use two kinds of
API:

- **Existing extension APIs** cover most of what BraveBot needs:
  - [`tabs`](https://developer.chrome.com/docs/extensions/reference/api/tabs)
  - [`windows`](https://developer.chrome.com/docs/extensions/reference/api/windows)
  - [`tabGroups`](https://developer.chrome.com/docs/extensions/reference/api/tabGroups)
  - [`scripting`](https://developer.chrome.com/docs/extensions/reference/api/scripting)
  - [`history`](https://developer.chrome.com/docs/extensions/reference/api/history)
  - [`bookmarks`](https://developer.chrome.com/docs/extensions/reference/api/bookmarks)
  - [`readingList`](https://developer.chrome.com/docs/extensions/reference/api/readingList)
  - [`sessions`](https://developer.chrome.com/docs/extensions/reference/api/sessions)

  Full list: https://developer.chrome.com/docs/extensions/reference/api
- **Brave-private extension APIs** fill the gaps. We add them when
  needed, and only our extension's ID can use them.

The first gap is semantic search over history. Standard APIs don't
expose the browser's embedding index, so a private API will run
semantic queries against it. This reuses the browser's own embedder,
so the extension doesn't need to run a model or keep its own index. It
needs a privacy review, because it lets BraveBot search the user's
entire browsing history by meaning.

## Architecture

```
BraveBot ──MCP──▶ bravebot-browser mcp ◀──socket──▶ bravebot-browser ◀──▶ extension
                  (MCP server, started              (native host,     native
                   by BraveBot, confined)            started by       messaging
                                                     Brave, relay)
```

The MCP server and the native host are one program, `bravebot-browser`,
run as two processes. The first argument picks the role: `mcp` when
BraveBot starts it, the extension's origin when Brave does.

- **MCP server:** declares the tools and forwards each call.
- **Native host:** a thin relay started by the browser. It forwards
  messages between the extension and the MCP server over a Unix socket
  or named pipe.
- **Extension:** runs each request with existing or Brave-private APIs
  and returns the result.

The relay is needed because only the browser can start a native host,
and it controls the host's lifetime and stdin/stdout. BraveBot can't
connect to the extension directly.

Details: https://developer.chrome.com/docs/extensions/develop/concepts/native-messaging

## Why native messaging

- Nothing listens on a port. Only extension IDs listed in the host
  manifest's `allowed_origins` can reach the host.
- Rejected alternative: a localhost WebSocket is simpler, but any local
  process or web page can reach it. That's not acceptable when it
  exposes history.

## Why the socket needs a secret

Native messaging keeps other extensions off the host, but the socket
between the host and the MCP server is a file any local process may try
to connect to. Confinement does not prevent it: every stdio server
BraveBot starts has egress, and on macOS egress is what reaches a Unix
socket.

- The host writes a new random secret beside the socket each time it
  starts, readable by the account alone, in a directory only the account
  can enter.
- A peer sends the secret as its first line. A peer that sends anything
  else, or sends it late, is closed before anything it sent is
  forwarded.
- Only a server whose declaration grants that directory can read the
  secret.
- A process of the same account that can read the file can use the
  relay. The extension's per-tool switches are the control for that
  case.

[BROWSER-3](../specs/browser.md#BROWSER-3) has the limits and the tests.

## Protocol

JSON-RPC between the MCP server and the extension:

- request `{id, method, params}`
- reply `{id, result}` or `{id, error}`, matched by `id`

Initial tools: `get_platform_info`, `list_tabs`, `list_frames`, `read_page`,
`search_history` and `search_bookmarks`. `read_page` can read the top page or one exact HTTP or
HTTPS frame URL a person chose from `list_frames` and supplied in a later message; the planner does
not read the untrusted list itself.

## Constraints

- **Size:** native messaging caps a message to the extension at 1 MB
  and a reply from it at 64 MB. Return trimmed page text, not full
  HTML, so a page fits the model's context.
- **Availability:** if the extension isn't connected, fail tool calls
  with a clear error instead of hanging.
- **Consent:** the extension's own UI gates every tool that reaches tabs, frames, history or
  bookmarks. A frame read names both its outer tab URL and its exact frame URL; one approval never
  widens to every embedded origin. The MCP layer doesn't enforce this.
