---
name: update-docs
description:
  'Bring the documentation site under docs/website/ up to date with bravebot.
  Reviews what has landed in a span of commits or inspects unmapped clauses,
  folds the user-facing behaviour into the pages it belongs on, and builds the
  site cleanly. Triggers on: update docs, /update-docs, sync docs with specs,
  docs drift.'
argument-hint: '[rev-range] [dry-run]'
allowed-tools: Bash(make -C docs/website *), Bash(git *), Bash(grep *), Bash(rg *), Read, Edit, Write
---

# Bring the Documentation Site Up to Date With Brave Bot

This skill documents user-facing behaviour changes across a span of commits or spec clauses, folding them into the appropriate pages under `docs/website/docs/`.

* **Range run** (`/update-docs <rev-range>`): inspects commits in `<rev-range>` (e.g., `HEAD~20..HEAD` or `<tag>..HEAD`), determines what user-facing behaviour or configuration changed, and updates the relevant pages.
* **Unmapped check**: runs `make check-spec` to find normative clauses lacking a page reference or unverified against pages.
* `dry-run`: reports what would change and writes nothing.

---

## Writing Style Rules

1. **User-facing perspective**: The reader is using Brave Bot, not developing it. Explain what happens, how to use it, and why it is safe. Never mention Rust internal types, internal struct names, or low-level crate architectures unless they are part of user configuration.
2. **Concise and direct**: Add or amend sentences only where a reader would act differently or be misled without the detail. Avoid transcribing internal spec rationale.
3. **Relative links**: All links between documentation pages must be relative and end in `.md`.
4. **Zero em dashes**: Never use em dashes anywhere. Use colons, commas, parentheses, or separate sentences instead.

---

## The Topic Mapping

When a change passes the gate, map it to the corresponding page in `docs/website/docs/`:

| A bravebot spec about | Belongs on |
|---|---|
| labels, who may read what | `docs/website/docs/security/trust.md` |
| routing, where an effect may land | `docs/website/docs/security/permissions.md` |
| the trust map, vouched paths | `docs/website/docs/security/trust.md`, `docs/website/docs/customize/skills.md` |
| a tool's arguments, refusals, results | `docs/website/docs/reference/tools.md` |
| shell mode, `run` | `docs/website/docs/using/shell-mode.md` |
| terminal input, the transcript | `docs/website/docs/using/interactive-mode.md`, `docs/website/docs/using/transcript.md` |
| sessions, compaction, undo | `docs/website/docs/using/sessions.md`, `docs/website/docs/using/context.md` |
| prompting, permission gates | `docs/website/docs/security/permissions.md` |
| skills, AGENTS.md, instructions | `docs/website/docs/customize/skills.md`, `docs/website/docs/customize/instructions.md` |
| the trace and audit trail | `docs/website/docs/security/audit-trail.md` |
| flags, environment variables, CLI defaults | `docs/website/docs/reference/cli.md`, `docs/website/docs/customize/configuration.md` |
| backends, model rosters, providers | `docs/website/docs/customize/configuration.md` |
| `settings.json`, its keys, precedence | `docs/website/docs/customize/configuration.md` |
| slash commands | `docs/website/docs/reference/commands.md` |
| premium credentials | `docs/website/docs/customize/premium.md` |

Check the span against [coverage.md](coverage.md) before concluding an update.

---

## Verification

After making page updates, verify the site:

```sh
make -C docs/website check
```

This must pass cleanly. `onBrokenLinks` and `onBrokenAnchors` throw errors on any broken link or reference.
