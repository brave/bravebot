# Brave Bot documentation

The documentation site for [Brave Bot](https://github.com/brave/bravebot), a general-purpose
agent with structural resistance to indirect prompt injection.

**Read it at [brave.github.io/bravebot](https://brave.github.io/bravebot/).**

Built with [Docusaurus](https://docusaurus.io/). `.github/workflows/gh-pages.yml` builds this
directory and publishes it on every push to `main`; nothing outside `docs/website/build` is
served, and no `gh-pages` branch is involved.

### Getting started

```sh
make install  # install dependencies
make start    # serve locally with live reload
```

`make` on its own lists every target. The npm scripts still work if you prefer them:
`npm install`, `npm start`, `npm run build`.

### Build

```sh
make build
```

Generates static content into `build/`, which can be served by any static host. Broken links
and anchors are build errors rather than warnings, so a clean build is also the correctness
check:

```sh
make check
```

From the repository root, `make check-docs` runs the same build the CI job runs, and
`make check-all` includes it.

### Where the content comes from

Everything here describes behaviour that is specified clause by clause in
[docs/specs](../specs/README.md). Where the two disagree, the specs are the source of truth:
fix this site rather than documenting around it.

`make check-spec` at the repository root holds the specs to this site. A clause no page
documents is listed in [`undocumented-clauses.txt`](../../undocumented-clauses.txt), and
`make write-undocumented` rewrites that list once a clause has been documented, or once one
has been added. To fold a span of bravebot commits into these pages, run the
[update-docs](../../agents/skills/update-docs/SKILL.md) skill.

## License

[MPL-2.0](LICENSE)
