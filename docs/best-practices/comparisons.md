# Comparisons

<!-- applicability: always -->

A host, a socket and a path each have a helper in this tree that reads its structure. A comparison
that decides what a rule covers, where a request or write may go, or what a person is told about a
destination is only as sound as the value it was given. What a reviewer checks here, which no lint
decides because `contains` and `starts_with` are correct on text that is only text, is whether the
comparison reads the value with the helper first.

---

<a id="CMP-001"></a>

## A host, socket or path that decides something is read with its helper before it is compared

**A comparison whose result decides a rule, a grant, a label shown to a person or where a request
or write may go reads the host, socket or path with the helper for it first, and applies no
`contains`, `starts_with`, `ends_with`, `find`, `split` or regular expression to the whole
string.** For a URL the helpers are `bravebot_core::url::host_of` for the host, `authority_of` for
host and port and `socket_of` for scheme, host and port (`crates/core/src/url.rs`). For a file, a
path holding `..` is refused first, the path is resolved through its links, and the result is
compared by components with `Path::starts_with`. The resolver is `Workspace::resolve` for a path a
tool names, which only code in `bravebot_agent` can call, and
`bravebot_sandbox::rules::resolved` for a path a rule or grant covers.

A reviewer looks in the diff for:

- a string method on an endpoint, URL, host or path that feeds an `if`, a `match`, a set lookup or
  a rule;
- a path compared while it still holds a link or a `..`;
- a suffix comparison of a host that leaves out the leading dot (`ends_with("brave.com")` also
  matches `evilbrave.com`).

A string method on a URL or path that only draws or logs it is display and is fine.

```rust
// Wrong: text after `#` or `?`, and a name that merely holds the string, read as the host.
// `https://attacker.example#.brave.com` is prod and `https://localhost.attacker.example` is local.
if endpoint.contains(".brave.com") { prod } else if endpoint.contains("localhost") { local }

// Right: the host is read first and compared whole or by a suffix that starts at a dot.
let Some(host) = bravebot_core::url::host_of(endpoint) else { return custom };
if host == "localhost" { local } else if host.ends_with(".brave.com") { prod } else { custom }
```

**Why:** the string a person or a file sets is the text that was written, and the transport
connects to the host in it and the system opens the file it leads to. A substring test accepts
whatever carries the expected text anywhere, so a fragment, a query, a subdomain or userinfo makes
a destination read as another one, and a link or a `..` makes a file read as another file. The
rule then decides, or the person is told, about the wrong destination.
