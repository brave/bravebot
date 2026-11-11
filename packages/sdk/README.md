# @brave/bravebot-sdk

A thin typed wrapper that runs the `bravebot` command line and reads its event stream. It starts
`bravebot --json-stream` for one run (`run`, `stream`) or `bravebot --json-stream --input-format
lines` for a conversation (`startSession`), and returns typed events and result objects. See
[the spec](../../docs/specs/sdk.md) and [the guide](../../docs/website/docs/using/sdk.md).

```
src/args.ts      the closed list of options and the command line each one becomes
src/run.ts       run and stream
src/session.ts   startSession and the Session class
src/process.ts   the child process, its lines and its exit
src/types.ts     the shapes of events and result objects
tests            node:test suites, including ones that run a real bravebot
```

## Use

```ts
import { run } from '@brave/bravebot-sdk'

const { result } = await run('what does src/main.rs do?', { cwd: '/path/to/project' })
console.log(result.status, result.reply)
```

The package is private and is used from this repository. It requires Node 22 or later. Its manifest
already lists `files`, `repository` and `publishConfig`, and its version follows the repository's
(`make check-versions`), so a first publish needs only the workflow, the registry's trusted
publisher and removing `private` ([RELEASE-10](../../docs/specs/releases.md#RELEASE-10)).

## Check

`make check-sdk` from the repository root builds `bravebot`, installs, type-checks, builds and runs
the tests. Inside the package, `pnpm run check` does the same once `bravebot` is built; set
`BRAVEBOT_BIN` to use a binary other than `target/debug/bravebot`. The tests use an empty home, a
scratch project and a model service of their own, and read no credentials or settings.

## Notes for embedders

- `reply`, `message`, `reason` and a call's `target` can hold text from outside the run. Show them as
  plain text and do not run them or put them in a command.
- The library has no option that lowers a permission, vouches for a path or approves an effect, and
  refuses an option it does not list.
- A later turn of a session cannot work on a reference an earlier turn made, since each turn is its
  own task.
- The agent is told that the user chose the position an `onQuestion` handler returned, including
  when the program picked it.
- `trustInputLines` tells the command line that the lines are the person's own words. Leave it off
  unless they are.
- `env`, when given, is the process's whole environment.
