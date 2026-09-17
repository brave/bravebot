# Configuration

Development uses [direnv](https://direnv.net/) (on macOS, `brew install direnv`).
Copy the template and fill it in:

```sh
cp .envrc.example .envrc
direnv allow
```

`.envrc` is gitignored and must never be committed, because it holds a signing key.

The build captures whatever is set at build time, so the resulting binary works in any
directory rather than needing direnv wherever it is started. A build with nothing set **fails**,
rather than producing a binary that only works in the tree it came from; to build one
deliberately, set `BRAVEBOT_ALLOW_UNCONFIGURED_BUILD=1` and supply the variables at run time.

The environment still wins when set, which is how a released binary is pointed at a local
backend without rebuilding it. Baked values are masked so `strings` on the binary does not
print them; that is obfuscation and not encryption, so a binary built with a live key should
be treated as holding one.

The cross-build container does not inherit the host environment, so `make all-platforms`
forwards these variables as a BuildKit secret rather than a build argument, which would record
the signing key in the image metadata.

Run `bravebot doctor` to check configuration and confinement without revealing the signing key.

In this source checkout, doctor also reports the agent instructions discovery link and whether
`direnv` is on PATH. Both checks are read-only development advice; ordinary user workspaces
need neither. The direnv check does not test shell-hook activation or `.envrc` approval.
