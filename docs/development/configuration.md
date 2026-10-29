# Configuration

Development uses [direnv](https://direnv.net/) (on macOS, `brew install direnv`).
Copy the template and fill it in:

```sh
cp contrib/envrc.example .envrc
direnv allow
```

`.envrc` is gitignored and must never be committed, because it holds a signing key.

The build captures whatever is set at build time, so the resulting binary works in any
directory rather than needing direnv wherever it is started. A build with nothing set **fails**,
rather than producing a binary that only works in the tree it came from; to build one
deliberately, set `BRAVEBOT_ALLOW_UNCONFIGURED_BUILD=1` and supply the variables at run time.

A build that bravebot's `run` tool starts fails this way unless it sets
`BRAVEBOT_ALLOW_UNCONFIGURED_BUILD=1`. `run` removes `SERVICES_KEY_AICHAT` and
`BRAVE_SERVICES_KEY_ID` from the environment of every program it starts
([RUN-12](../specs/tools/run.md#RUN-12)), so the build script reports both as not set whatever the
shell holds. `BRAVE_AI_CHAT_ENDPOINT` is not removed, so it is reported only where the shell lacks it.
Build from a terminal, or start the build with the override. The script's failure notes the removal,
and the description of `run` tells the planner that a program reporting one of these unset says
nothing about the shell.

The environment still wins when set, which is how a released binary is pointed at a local
backend without rebuilding it. One thing outranks it: a machine-level file an administrator
deploys, which pins where a request goes for everybody on that machine and is what
`docs/specs/backends.md` calls the managed layer. Nothing on a development machine has one unless
somebody wrote it as root. Baked values are masked so `strings` on the binary does not
print them; that is obfuscation and not encryption, so a binary built with a live key should
be treated as holding one.

`BRAVE_AI_CHAT_PREMIUM_ENDPOINT` is optional. The premium host is compiled in, as `DEV_PREMIUM_ENDPOINT`
and `PROD_PREMIUM_ENDPOINT` in `crates/config/src/lib.rs`, chosen by the free endpoint: one under
`brave.software` or `bravesoftware.com` gets the dev host and any other gets the prod host. Both are the
dev host while the premium service is live only in development. The variable, baked into a build or
exported, replaces the compiled-in host. A value with no scheme, or with a `?` or a `#`, is not used and
the compiled-in host stands. A build that reaches no Brave backend (Bedrock only) has no premium host.
`.envrc` sets no premium variable and no key: the premium key is not configuration, and each person
stores their own with `bravebot auth login brave`.

`BACKTRACE_SUBMIT_URL` is also optional. It is the
submission URL of the Backtrace project that crash reports go to, token included, and a build made
without it writes and sends no report ([CRASH-3](../specs/crash-reports.md#CRASH-3)). Unlike the
others it is not read from the environment at run time at all. Leave it out of `.envrc` for day-to-day
work: a build that has it reports its own panics too, marked as debug builds. Releases are built by
the Jenkins job, so the variable has to be set there as well for released binaries to report.

The cross-build container does not inherit the host environment, so `make all-platforms`
forwards these variables as a BuildKit secret rather than a build argument, which would record
the signing key in the image metadata.

Run `bravebot doctor` to check configuration and confinement without revealing the signing key.

In this source checkout, doctor also reports the agent instructions discovery link and whether
`direnv` is on PATH. Both checks are read-only development advice; ordinary user workspaces
need neither. The direnv check does not test shell-hook activation or `.envrc` approval.
