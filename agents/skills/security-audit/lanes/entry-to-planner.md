### from an entry point to the planner's context

Every road untrusted content takes into this program is supposed to be a gate in
`crates/core/src/policy.rs`, and LABEL-8 in `{labels_spec}` is the table of them. Read that table
first: it is the checklist for this lane.

For each road, trace forward and answer one question: **can the bytes, or a value computed from them,
reach a model's context or steer a turn already running?**

Roads to walk, with where they start:

- a file read, and a read of a path that does not exist yet
- `fetch_url`, including every redirect hop
- a subprocess or pipeline's output, and a shell line a person typed
- piped standard input, and a file named in a prompt or dropped on the window
- a pasted image, and an interjection typed mid-turn
- a person's own configuration and skills
- model output coming out of a transport envelope
- a processor's answer, and a vetting verdict
- a delegate's report
- an MCP tool result
- an LSP location, a file watch firing, and a hook's output

The last three are supposed to carry no content at all: LSP separates the location from the text, a
watch reports a path, and hooks deliberately read nothing. Confirm that rather than assuming it.

Two soft spots are documented and worth the time:

- `bravebot-skus` opens its own HTTP client, so its traffic does not pass the gate in `bravebot-net`.
  `docs/specs/layering.md` records this under its own known cost and claims it is safe because the
  socket carries only credentials and an order id. Check the claim: does anything on that path parse a
  response into a value that leaves the crate?
- `bravebot-mcp` labels its results untrusted inside the crate, and nothing depends on the crate
  today. It is compiled and unreached. Where it is wired in, tool names get namespaced and the
  routing and content split has to survive that. Report what would break, not that it is unused.

  Three things in that crate are worth the walk before it is wired, because a clause written now is
  cheaper than an audit later:

  - **A result is labelled and a descriptor is not.** `call_tool` returns `Labelled<String>` on
    both transports. `list_tools` returns a bare `Vec<ToolDescriptor>`, and a descriptor carries
    `name`, `description` and `input_schema`. `protocol.rs` says the schema is passed to the model
    unchanged. MCP-1 pins what a server *returns*; ask which clause pins what a server *claims to
    offer*, and whether a description a server chose is untrusted content arriving in the planner's
    tool list.
  - **Only one transport checks the id it asked about.** `StdioServer::send_request` compares a
    reply's id against the request's and skips a line that does not match. `HttpServer::send`
    increments the same counter, sends it, and never compares it. Ask whether the transport
    guarantees what the check would, and what MCP-7's "the last SSE payload wins" selects when a
    stream carries two payloads whose ids differ. A difference between the two roads is worth
    reporting even where neither is wrong alone, for the reason `supply-chain` gives: the one a
    person read is not the one that runs.
  - **A name the server chose becoming a routing field.** Once descriptors are namespaced, ask what
    a tool name decides, and whether two servers can claim one name. CLAUDE.md's test for a new
    tool is whether a person could approve its routing field alone, and a name the peer supplies is
    a field they did not.

Where a road ends somewhere legitimate, say where. The interesting answers are a road with no gate, a
gate that labels the bytes and a later path that reads them anyway, and a value derived from untrusted
bytes that ends up in a string handed to the model. That last one is the rule failing, and a message
to the model **is** the planner's context: `{review_doc}` says "'It is only for a message to the
model' does not help either".
