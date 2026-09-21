# bravebot-ui

A macOS and Linux desktop interface to [bravebot](https://github.com/brave/bravebot), the
prompt-injection-resistant coding agent. Built with Electron, React and a Rust bridge,
it uses the agent as a pinned dependency without modifying its sources.

The app shares saved sessions under `~/.bravebot` with the terminal client. It adds
conversation search, drafts and queues, file previews, approval controls, export,
and persistent bots with conversation histories and project memory.

## Quick start

Install your platform's development tools, current stable Rust, and Node 22.12+ with npm
(Node 24 is used in CI). See [setup](docs/setup.md) for prerequisites and credentials.

```bash
git clone --recurse-submodules https://github.com/brave-experiments/brave-bot-ui.git
cd brave-bot-ui
npm ci
npm run dev
```

The app can build and open without backend credentials. Sending prompts requires
[backend configuration](docs/setup.md#credentials); credentials from an existing
terminal installation are not automatically copied into a fresh checkout.

## Documentation

- [Setup](docs/setup.md): prerequisites, credentials, submodule updates and troubleshooting.
- [Development and packaging](docs/development.md): commands, builds and macOS bundles.
- [The interface](docs/interface.md): conversations, approvals, bots, memory, keys and themes.
- [Testing](docs/testing.md): regression checks, Electron drivers and CI coverage.
- [Security](docs/security.md): process boundaries, approvals and renderer restrictions.
- [File access and retention](docs/file-access-security.md): previews, attachments and memory storage.
- [Bridge protocol](docs/phase-0-rpc-protocol.md): transport, requests, events and design rationale.
- [Recording a demo](docs/demo.md): fixtures, live calls and recording controls.
- [Redesign audit](docs/ui-ux-redesign.md): historical implementation evidence and reproduction.

## Licence

MPL-2.0.
