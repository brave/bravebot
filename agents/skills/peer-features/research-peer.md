# What does {{peer_name}} offer that bravebot does not?

List what {{peer_name}} documents, and find the gaps worth an issue. This review starts from the other
tool, so it finds capabilities that no bravebot spec mentions. Today is {{today}}.

## The tool

{{peer_name}}, whose source is: {{peer_source}}

Use its own documentation site, repository, README and changelog. Use whatever web search and fetch
tools you have.

The tree is `{{root}}`, which is `{{ref}}` at `{{commit}}`. Every path is relative to it. Read files
and run commands in it, but edit nothing: no tracked file, no configuration, no test.

## Other tools, for the `beyond` test

A `beyond` gap has to be about 10% better than the best of the tools, so where you consider one, the
others matter:

{{peers}}

## Read these first

- `agents/AGENTS.md`, for the rule that overrides everything. A gap is only a gap if closing it keeps it.
- `docs/specs/README.md`, which lists every area bravebot specifies and in one line what each covers.
  Open the spec for any capability you are checking, including its `## Known costs`. A cost listed
  there is a decision, and it is not a gap unless this tool shows the cost can be avoided without
  breaking the rule.
- `docs/website/docs/`, for what a person is told bravebot does.
- `docs/development/labelling-issues.md`, for how a title reads.

## The steps

1. From {{peer_name}}'s documentation, list its capabilities: commands, settings, tools, modes,
   integrations, file formats, safety features and the things it shows a person. One short sentence
   each.
2. For each, decide whether bravebot has it. Search the specs, the website docs and the code by the
   words the tree uses. A capability bravebot has in another shape is not missing.
3. Classify the ones worth an issue as `parity` (bravebot lacks it) or `beyond` (bravebot has it and a
   concrete change would beat {{peer_name}} and the others), drop everything that fails the limits
   below, and keep at most {{max_gaps}}.
4. Check `{{known_gaps}}` and `{{tracker}}` for each one you keep.
5. Write the result.

{{gap_rules}}
