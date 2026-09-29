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
BraveBot ──MCP tool call──▶ MCP server ◀──socket──▶ native host ◀──▶ extension
                                                      (relay)    native
                                                               messaging
```

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

## Protocol

JSON-RPC between the MCP server and the extension:

- request `{id, method, params}`
- reply `{id, result}` or `{id, error}`, matched by `id`

Initial tools: `list_tabs`, `navigate`, `read_page`, `search_history`,
`search_bookmarks`, `semantic_search_history`.

## Constraints

- **Size:** native messaging caps a message to the extension at 1 MB
  and a reply from it at 64 MB. Return trimmed page text, not full
  HTML, so a page fits the model's context.
- **Availability:** if the extension isn't connected, fail tool calls
  with a clear error instead of hanging.
- **Consent:** the extension's own UI gates sensitive tools (history,
  bookmarks, semantic search) with a confirmation step or an allow-list.
  The MCP layer doesn't enforce this.
