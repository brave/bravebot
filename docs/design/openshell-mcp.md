# OpenShell sandboxes for MCP servers

## Overview

bravebot can run a local stdio MCP server inside an NVIDIA OpenShell sandbox made from a container image. The server sees its image instead of the person's filesystem, and reaches only the hosts approved for its declaration. The backend is opt-in per server and serves manifest runs.

It adds a confinement option for third-party MCP code without changing bravebot's trust model: planning, capability checks, approvals and labels stay on the host, and OpenShell confines the process behind the approved MCP interface. A server declared for OpenShell **never falls back** to native execution when its sandbox cannot start.

Specified by [SERVERS-23](../specs/mcp-servers.md#SERVERS-23), [SERVERS-24](../specs/mcp-servers.md#SERVERS-24), [SANDBOX-33](../specs/sandboxing.md#SANDBOX-33) and [MANIFEST-15](../specs/manifest.md#MANIFEST-15).

## Why OpenShell

bravebot already confines native MCP processes and restricts their network destinations. OpenShell adds a separate execution boundary:

- **Image-based filesystem:** the server sees its image and the sandbox's writable locations, never host files. No host directory is mounted.
- **Per-program egress:** approved hosts become enforced OpenShell rules for the server's resolved executable. With no approved hosts, the server has no network.
- **Enforcement outside the server:** OpenShell enforces and logs network decisions in its own process, readable with `openshell logs <sandbox>`.
- **A path to protected credentials:** OpenShell providers can later hand the server endpoint-bound placeholders instead of real secrets.

This is **defense in depth for untrusted MCP server code**. bravebot's own trusted components do not move into a sandbox.

## Scope and trust boundary

| In scope | Out of scope |
| --- | --- |
| Local stdio MCP servers called by manifest `mcp_call` steps | Remote HTTP MCP servers, which have no local process to confine |
| One OpenShell sandbox per opted-in server | Running bravebot's planner or process tree inside OpenShell |
| Image, network, lifecycle and approval integration | Connector authorities, which hold credentials and draw approvals on the host |

MCP server output stays untrusted whatever the backend.

### Design choices

| Concern | Behavior |
| --- | --- |
| Opt-in | Per stdio declaration; no session-wide default |
| Availability | Manifest runs only; turns neither offer these tools nor list or fetch their prompts |
| Ownership | The MCP session owns the server and its sandbox |
| Image identity | A content digest is required; `mcp add` resolves a tag before recording the declaration |
| Failure | Fail closed; no fallback to native confinement |
| Compatibility | OpenShell 0.1.7 minimum; newer stable releases accepted |
| macOS driver | The gateway's configured driver; Docker Desktop is the documented, tested path |

## Declaring a sandboxed server

A local stdio declaration may hold:

```json
{
  "backend": "openshell",
  "image": "registry.example/server:tag@sha256:<digest>"
}
```

Both fields are part of the declaration's approval digest, so changing either asks for approval again. Declarations without them digest as before.

```sh
bravebot mcp add <alias> --openshell <image> [--allowed-hosts <list>] -- <program> [args]
```

### Image identity and the program

An image is pinned before use. Given a tag, `mcp add` asks the registry, through bravebot's egress gate, for the manifest the tag names and writes the reference with the SHA-256 of that manifest. The lookup is anonymous, using Docker Hub's and GHCR's pull tokens where they require one. An index is requested first, so the digest covers every platform and does not depend on the local one. Only the digest is released from the registry's reply.

A private or unreachable image, or a reference whose registry or repository is malformed, stops the add with nothing written. A session refuses a declaration whose image carries no digest, as a hand-written one may give. The program is looked up **inside the image**, not on the host's `PATH`.

In this first integration a declaration for OpenShell cannot name variables, stored values, files to read, `--dir` or `--http`. Nothing of the host is shared with the sandbox.

### Managed settings

A `mcp.deny` entry matches a sandboxed server by its program and arguments as the image names them. No `mcp.allow` entry names it, since the image decides what is at a path inside it, so where `mcp.allow` is in force the server is not started.

## Session lifecycle

Tool lists are discovered and approved **before** a manifest is planned, so the sandbox belongs to the MCP session, not to one manifest run.

1. **Approve the declaration.** The question names the OpenShell backend, the pinned image and the allowed hosts.
2. **Create the sandbox.** The session creates it on the server's own thread, within the declaration's startup bound, with the filesystem policy, and waits for `Ready`. Where the server has hosts, it resolves the program's real path in the image and loads the network rules with `policy set --wait`.
3. **Start MCP over stdio.** Only once the sandbox is ready and its policy confirmed does bravebot start the program with `sandbox exec --no-tty`. The usual parsing, line-size, handshake and call limits apply ([MCP-7](../specs/mcp.md#MCP-7), [MCP-13](../specs/mcp.md#MCP-13), [SERVERS-15](../specs/mcp-servers.md#SERVERS-15)).
4. **Approve the tool list.** The person vouches for the tools ([SERVERS-8](../specs/mcp-servers.md#SERVERS-8)), and manifest planning may then offer them.
5. **Reuse and release.** Approved calls use the same stdio connection. The sandbox is deleted when the session releases the server.

If creation, policy confirmation or launch fails, the server is not started and a sandbox already created is deleted.

### Ownership and cleanup

A sandbox is named `bb-` and twelve random characters. Where the state directory is writable, bravebot records the name, the owning process and a stable machine identifier in `openshell-sandboxes` **before** creation.

The first OpenShell start in a process deletes recorded sandboxes of the same machine whose owning process has ended, which recovers from abrupt termination. A sandbox of a running process, another machine, or with no record is never deleted. Where the state directory may not be written, as in an incognito session, nothing is recorded or swept.

### Operation bounds

`openshell` commands start through the process spawn gate and are waited for outside it. A command and its output are bounded, and the command is killed past its bound:

| Operation | Limit |
| --- | --- |
| Sandbox creation, including an image pull | 300 seconds |
| Policy loading | The policy wait plus 30 seconds |
| Any other `openshell` command | 30 seconds |

## Isolation and network policy

The policy is built by a pure function of the approved declaration and the session's network decision.

**Filesystem.** The image only; no host mounts. System directories are read-only and `/tmp` is writable. Landlock is set to `compatibility: hard_requirement`, so a kernel that cannot enforce it refuses the sandbox rather than running it unconfined.

**Network.** Each declared host becomes an enforced port-443 rule for the program's real path inside the image, after narrowing: hosts the session's allowed list does not cover, and hosts any session denied entry overlaps, are removed, and names are converted to OpenShell's spelling with unsupported ones dropped. Where the session's network is closed, no hosts are declared, or a denied entry cannot be read, no host is granted.

**No widening.** bravebot writes `enforcement: enforce` rules, never `audit`, and OpenShell's Policy Advisor stays off, so the server cannot have its network policy widened during a session. Registry hosts a package manager would need are not added.

**TLS inspection.** OpenShell inspects TLS through a CA the sandbox trusts. This differs from bravebot's own proxy, which never decrypts ([SANDBOX-24](../specs/sandboxing.md#SANDBOX-24)). Clients that pin certificates or ignore the sandbox's trust settings fail to connect.

## What a person sees

| Surface | Shows |
| --- | --- |
| Approval question | Backend, pinned image, allowed hosts |
| `bravebot mcp get` and `mcp list` | That the server runs in OpenShell, from what image, for manifest runs only |
| `/status` and `/mcp` | The sandbox and image, permitted hosts or none, image-only reads, manifest runs only; an unread tool list is said to be put before a manifest run |
| `doctor` | The servers declared for OpenShell, the OpenShell version and gateway connectivity |
| Session notes | Each refusal with its reason, and stale sandboxes deleted |

`doctor` fails where a server is declared for OpenShell and the CLI is older than 0.1.7 or the gateway is not connected. A session does not check the version itself. On macOS with the Docker driver, the gateway needs `supervisor_network_mode = "bridge"`; it reports connected in host mode too, so this is documented rather than checked.

## Compatibility and security evidence

bravebot tracks the latest stable OpenShell release, with **0.1.7 the minimum** the live tests passed against. Each release's notes are read for changes to `exec`, policy, providers and compute drivers before the minimum moves.

A tool's result keeps bravebot's existing treatment: untrusted and private, shown to the person and never given to the planner ([MCP-1](../specs/mcp.md#MCP-1)). bravebot does not read OpenShell's allow and deny events; they stay in `openshell logs`, outside planning and outside any automatic decision. They are **evidence, not a complete record**: OpenShell logs enforcement events, not every system call or file access, and keeps a limited in-memory log per sandbox.

Indicative figures on macOS with Docker Desktop: about 4 seconds to create a sandbox from an uncached image and under 1 second cached, 360 milliseconds from `exec` to the MCP `initialize` reply, and about 1.3 milliseconds per tool call. They vary with the machine and driver.

### Test coverage

- **Unit:** policy generation, quoting, host normalisation and narrowing, image-reference validation, sandbox names and ownership records, cleanup, command bounds, managed-settings matching.
- **Integration:** turns exclude these servers' tools and prompts; a manifest run discovers, approves, plans and calls.
- **Live (`BRAVEBOT_OPENSHELL_TESTS=1`):** stdio over `exec`, allowed and denied hosts, a missing program, a long-lived connection with an idle pause and a 12 MiB reply, stale-sandbox recovery, a full session start, and Docker Hub pinning.

## Current limitations

These servers serve manifest runs in a session (the terminal's `/manifest` or the desktop app), not ordinary turns. Two limits apply to every MCP server under the manifest model and are not introduced by OpenShell:

- **One-shot runs:** a tool list is vouched for per process, so an unattended one-shot run cannot use MCP tools without a separately specified way to persist approvals.
- **Tool output:** an `mcp_call` reply is shown to the person but cannot fill a later manifest step ([MANIFEST-15](../specs/manifest.md#MANIFEST-15)), so these servers suit calls that act rather than calls whose data a later step needs.

## Possible extensions

| Extension | Consideration |
| --- | --- |
| Credential providers | Endpoint-bound placeholders instead of real secrets; must account for TLS inspection and session-scoped cleanup |
| Interactive turns | How a sandboxed server takes part in a turn's read-and-decide loop |
| macOS microVM driver | A Docker-free option with a kernel per sandbox; not yet validated |
| Persisted tool-list approvals | Approvals bound to the declaration and tool-list digests, for one-shot runs; a spec change of its own |

## References

- bravebot specs: [manifest.md](../specs/manifest.md), [mcp.md](../specs/mcp.md), [mcp-servers.md](../specs/mcp-servers.md), [sandboxing.md](../specs/sandboxing.md), [labels.md](../specs/labels.md).
- NVIDIA OpenShell 0.1.7 documentation: sandbox runtimes, network policies, provider profiles, platform support.
