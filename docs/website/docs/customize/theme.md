---
sidebar_position: 13
title: Choosing a theme
description: Pick a theme with /theme, write one of your own, and where the choice is kept.
---

# Choosing a theme

```
/theme
```

opens a picker that live-previews over your own transcript; `/theme <name>` applies one directly. The
choice is written to `~/.bravebot/theme`, so it outlives the session that made it and applies in every
directory, exactly as [the model choice](model.md) does. A name that matches no theme, and an empty or corrupt
file, is no choice at all and falls back to `brave`. A choice saved under the earlier name `system`
still finds `brave` rather than being silently lost.

A theme of your own is a JSON file under `~/.bravebot/themes/`, named for the theme: `nord.json` is
the theme `nord`. `brave.json` and `system.json` are refused, both names reaching the built-in theme.
Each key is one role, and any you leave out inherits from `brave`:

```json
{
  "defs": { "ink": "#cdd6f4", "shell": "#1e1e2e" },
  "background": "shell",
  "text": "ink",
  "muted": "#6c7086",
  "ok": "#a6e3a1",
  "fail": "#f38ba8",
  "running": "#f9e2af",
  "accent": "#cba6f7",
  "note": "#fab387",
  "primary": "#89b4fa"
}
```

A value is a `#rrggbb` colour, a name from `defs`, or `none` to leave that role to your terminal's own
default. A `defs` entry that names another `defs` entry is refused rather than chased, so a palette
cannot loop. A file that will not parse is left out of the list rather than stopping the session.

An ink may also be a **pair**, one colour for each terminal background:

```json
{ "muted": { "dark": "#6c7086", "light": "#8c8fa1" } }
```

The arm matching the background sensed at startup is the one used. A pair composes with `defs` and
with `none` exactly as a lone value does. A pair missing an arm is refused and the file holding it is
not a theme, because filling the missing arm in would let a typo paint half a palette. Write a pair
where a scheme was published for a light terminal and a dark one: that is one theme, not two. A theme
that gives at least one pair says so under the picker's list.

:::note
Themes are read from `~/.bravebot/themes` and from nowhere else. A `.bravebot/themes` directory inside
a project is **not** consulted. A repository you have just cloned must not be able to decide how your
interface is painted.
:::

See [Reading the transcript](../using/transcript.md#themes) for what each role paints.
