---
sidebar_position: 14
title: Choosing a language
description: The sources the interface language is read from in order, how to name one, and what stays in English.
---

# Choosing a language

Brave Bot reads the interface in your language where a translation for it has shipped, and in English
otherwise. It takes the first of these that names a language:

1. `BRAVEBOT_LOCALE`, in the environment.
2. The choice saved with [`/language`](../reference/commands.md#language-tag--auto).
3. The `language` key of `~/.bravebot/settings.json`.
4. `LC_ALL`, `LC_MESSAGES` and `LANG`, in that order.

So on a machine already set up for French there is nothing to do, and a program started from a launcher,
which has no shell profile to edit, can be given a language in the settings file.

```sh
bravebot                          # whatever your settings and shell say
BRAVEBOT_LOCALE=fr bravebot       # this once
export BRAVEBOT_LOCALE=fr         # from now on, in this shell
```

```json
{ "language": "fr" }
```

`language` is a locale tag, or `auto` to leave the question to the next source. It is read from your own
`~/.bravebot/settings.json` and from nowhere else, before the command line is read. A project's
`.bravebot/settings.json`, `.bravebot/settings.local.json` and a file `--settings` names cannot set it,
and `bravebot doctor` names one that tried. The interface is in one language for a whole session, so a
change takes effect at the next start. It applies to the terminal client. The desktop window is in
English only.

`/language fr` saves a choice without editing a file, in `~/.bravebot/language`, and `/language auto`
removes it. A saved choice answers over the settings file, since it is the later act.
`BRAVEBOT_LOCALE` answers over both, so one launch can name its own language.

A request widens rather than failing: `fr-CA` and `fr-BE` are answered by the French catalog where
they have none of their own, and a language nothing has shipped for reads in English. `bravebot doctor`
names a `language` key that no catalog answers. `LC_ALL=C` asks for no translation at all, where nothing
above it in the list names a language. English and French are what ship today. What a shell appends to say which
encoding or modifier it wants is not part of the name.

Widening is per message, not per language, so a single message a translation has not reached yet reads
in English inside an otherwise translated screen. A message that **counts** something is pluralised by
the rules of the language it was written in, which for one falling back is English's, rather than
having English's rule applied to French text.

## What stays in English

- **The names of the slash commands**, so `/model` is `/model` everywhere.
- **The letters a question is answered with**, `y` and `n`. These are both the key drawn and the key
  matched, so a French reader is told to press `y` for *oui*.
- **The audit trail.** It is fixed columns of gate and capability names that are identifiers, read
  against the specs that use those same names.
- **The words on the working indicator**, unless a language supplies its own list.

Digit grouping and currency forms are not localized either. A catalog says only what separates a
whole number from its fraction.

**Nothing the model is sent changes with your language.** Tool descriptions, the preamble and the
sentence a refused tool answers with all stay as they are, because the words in them are load-bearing
on what the planner does. Switching language changes what you read and never what the agent does.
