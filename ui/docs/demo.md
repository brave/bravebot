# Recording a demo

The [drivers](testing.md) assert; `npm run demo` performs. Same Playwright, same real window, same
class names — but paced in beats rather than in milliseconds, and it films itself:

```bash
npm run demo -- --record             # cached world: no new model calls; first run seeds it
npm run demo -- --record --live      # plus a real turn, an approval card, a question,
                                     #   a bot at work — its face looking down at the page —
                                     #   writing its own memory, and what it kept
npm run demo -- --only 08-fork       # one scene, for a retake
npm run demo -- --list               # what it would film, in order
```

`--record` points macOS's own `screencapture` at the window's rectangle and drops a WebVTT
beside the film with every caption already timed to it — so a run produces the video *and*
its subtitle track, rather than producing something somebody then has to remember to record.
It needs Screen Recording permission for whatever is running it; without that the film comes
out empty and the run says so.

The capture is then re-encoded, because the raw one is not a file anybody wants to be sent.
`screencapture` records at the display's own resolution and refresh rate — retina at
120 fps on the display used for the original recording — which produced
around 45 MB for three minutes of a mostly stationary window, and nothing in
its flag list changes that. So it is fixed afterwards: halved to logical resolution, dropped to
30 fps, and encoded at a CRF where the app's own text is still sharp at 1:1. That is about 4 MB
for the same footage. `--width 0` keeps the retina resolution, `--crf` and `--fps` move the
other two, `--keep-raw` keeps the capture as well, and `--no-compress` skips it.

This is the one step that wants something the repository does not depend on: **`ffmpeg`**. A
machine without it keeps the raw capture and is told why it is large. macOS's own `avconvert`
was tried instead and is not an alternative — it re-encodes at the source resolution and frame
rate, and saved almost nothing.

## Nothing real is filmed

A demo of this app is a demo of somebody's session list, and a session list is a record of
what they have actually been doing: real project names, real prompts, real paths, and — in
the file tree — the real contents of a real directory. So the demo does not film the machine.

It launches with `$HOME` pointed at a **demo world** under `/Users/Shared/bravebot-ui-demo`, and
with `--user-data-dir` pointed inside it. Both, and it took both: the agent finds `~/.bravebot` by
reading `HOME` and nothing else (`crates/agent/src/home.rs`), so that half sanitises the sessions —
but on macOS Electron derives `userData` from the password database rather than from the
environment, so with `HOME` alone a run still read and wrote the machine's *real*
`bravebot-ui.json`. That put real things on camera: the recents list is real project paths and
File ▸ Open Recent is filmed, and the bots list is real names and purposes and the bots tab is
filmed. With both redirections the session list, the recents, the bots, the column widths and the
file tree are all the world's. `/Users/Shared` rather than the home directory because every path in
it ends up on screen, and a home directory has somebody's name in it.

The projects under `scripts/demo/project/` are fictional demo fixtures; their READMEs
describe the sample world rather than this application's setup.

The world holds two invented checkouts — `harbour-lights` and `tide-tables`, copied out of
`scripts/demo/project/` — the sessions the demo earned in them, two resident bots (`RESIDENT_BOTS`:
one never spoken to, one with a session and a line in its memory file, so the bots scene has a
waiting face and a remembering one to point at), and the bot the bots scene makes in
`harbour-lights` each time it plays. **Earned, not written.**
Seeding the records by hand would mean encoding the agent's own on-disk format here: a format
that is upstream, is not ours, and is rewritten after every turn. A demo that hard-coded it
would break silently on a bump, months later, in a recording somebody was about to publish. So
the world is built by driving the product — a few real prompts, once, cached for every run
afterwards. It costs a few tokens the first time and nothing after that, and `--rebuild` throws
it away and does it again. `scripts/demo/world.mjs` is all of it.

`--real` films this machine's own sessions instead. That is for working on the demo, not for
recording one.

Two things are drawn into the page for the camera, because the window alone cannot show them.
Playwright drives Electron over CDP and CDP moves no cursor, so the pointer is drawn in and
glided to each target, the real one is hidden with `cursor: none` (two pointers, one of them
motionless, reads as a rendering fault), and each click leaves a ripple. And a caption names
each feature as it plays, docking to whichever half of the window the pointer is not in.

| Flag | What |
| --- | --- |
| `--record [path]` | Film the window. Default lands in `/tmp/bravebot-ui/demo/` |
| `--live` | Include the live scene: a real turn, a command approval, a series of questions |
| `--speed 1.4` | Multiply every pause. Higher is slower; the whole video re-times at once |
| `--size 1600x1000` | Frame it larger than the app's own default of 1280×820 |
| `--theme light` | `dark` (default), `light` or `system` |
| `--session <words>` | Pin which stored session gets filmed, so a second take matches the first |
| `--only <id>` / `--from <id>` | One scene, or from one scene on — for retakes |
| `--countdown 5` | Hold before the first scene, and before the recorder starts |
| `--width 1280` | Encoded width. `0` keeps the capture's retina resolution |
| `--fps 30` / `--crf 28` | The other two encoding dials. Higher CRF is smaller and softer |
| `--keep-raw` / `--no-compress` | Keep the raw capture too, or skip encoding entirely |
| `--rebuild` | Throw the demo world away and build it again |
| `--world <path>` | Somewhere other than `/Users/Shared/bravebot-ui-demo` |
| `--real` | Film this machine's own sessions. **Not sanitised** |

A `--real` run restores the state file it shares with the drivers, so a video that dragged the
columns somewhere cinematic does not leave `drive:columns` measuring the demo's idea of a
layout. A world run cannot reach that file at all.

**A feature that earns a `drive:*` driver earns a scene.** One file per feature under
`scripts/demo/scenes/`, listed in `scripts/demo/manifest.mjs`, which is checked against the
directory at startup — a scene added and not listed is an error rather than a scene that
silently never plays. A feature that needs something to happen in the world to be visible
also earns a prompt in `RECIPES` in `scripts/demo/world.mjs`, and a `--rebuild`.

