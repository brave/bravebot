# Brave Bot documentation

The documentation site for [Brave Bot](https://github.com/brave-experiments/bravebot), a
general-purpose agent with structural resistance to indirect prompt injection.

**Read it at [brave-experiments.github.io/bravebot-docs](https://brave-experiments.github.io/bravebot-docs/).**

Built with [Docusaurus](https://docusaurus.io/) and published from `main` on every push.

### Getting started

```sh
make init     # install dependencies and link agents/ into .claude/ and .bravebot/
make start    # serve locally with live reload
```

`make` on its own lists every target. The npm scripts still work if you prefer them:
`npm install`, `npm start`, `npm run build`.

### Build

```sh
make build
```

Generates static content into `build/`, which can be served by any static host. Broken
links and anchors are build errors rather than warnings, so a clean build is also the
correctness check, and it is the whole of what CI runs.

### Where the content comes from

Everything here describes behaviour that is specified clause by clause in the
[mini-specs](https://github.com/brave-experiments/bravebot/tree/main/docs/specs) in the
main repository. Where the two disagree, the specs are the source of truth: fix this site
rather than documenting around it.

[`docs-updated-to-sha`](docs-updated-to-sha) records the bravebot commit this site was
last brought up to.

```sh
make docs-updated-to-sha   # where the docs stand against bravebot
make docs-changes          # what has landed since
```

Both read a bravebot checkout at `../bravebot`, or wherever `BRAVEBOT_REPO` points. To
fold the gap in, run the [update-docs](agents/skills/update-docs/SKILL.md) skill: it
reviews the intervening commits, updates the pages that went stale, then records the new
commit and commits that too.

### Agent configuration

`agents/` is the checked-in source of truth for skills, `AGENTS.md` and each tool's
settings. No tool reads it
directly. `make init` symlinks it into `.claude/` for Claude Code and `.bravebot/` for
bravebot, so a skill is written once and both find it. The links are generated and
gitignored; `make agents` shows their state and `make unlink` removes them.

## License

[MPL-2.0](LICENSE)
