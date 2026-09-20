# What a terminal coding agent's documentation is expected to cover

A checklist of topics, not a table of contents. It exists because the gate in
[SKILL.md](SKILL.md) judges a commit on its own terms and cannot notice that a whole subject
never arrived. A change can fail the gate honestly five times running and leave a feature
undocumented, because no per-commit decision asks "is there a topic missing here".

Use it two ways:

- **Folding in a span**: when a commit touches one of these topics, the bar for excluding it is
  higher than the gate's default. These are the things people go looking for.
- **Occasionally, on its own**: read the list against the site and ask which rows have no page.
  A row with nothing behind it is either a gap or a feature this tool does not have. Both are
  worth knowing; only the first is worth fixing.

The rows are drawn from what comparable tools document. They are not a claim that bravebot has
the feature. Inventing a page for a feature that does not exist is worse than the gap.

## Configuration is the row that gets missed

Anything a person must **write into a file or export before the tool works the way they want**
belongs on this site, and the gate reads these commits badly: a settings key looks like plumbing
right up until you notice nobody can use the feature without knowing its name.

- the settings file: where it lives, what shape it is, which block is read
- what wins when a value is set in more than one place
- every key that selects a backend, a model, a region, or a credential
- what a settings file is not trusted for: permissions, paths, capabilities
- how a misconfiguration reports itself, and where
- what a diagnostic command prints, and what it withholds

**A key name is documentation, not an implementation detail.** If the specs do not carry it, read
the source for the spelling and write it down. `SKILL.md`'s ban on reconstructing behaviour from
source is about inventing a story. A flag's exact name is the opposite of invented, and a
configuration page missing its key names is not usable.

## The rest of the list

| Topic | Why somebody looks it up |
|---|---|
| install, first run, upgrade | before anything else works |
| models: choosing, defaults, what a name routes to | the choice that changes every answer |
| **backends and providers**: reaching a model through your own cloud account | billing and credentials are the user's |
| authentication and sign-in, including interactive flows and expiry | it blocks work when it lapses |
| credentials: where they are kept, who can read them | trust |
| permissions: what is asked, what is refused, what can be pre-granted | the thing people fight with daily |
| sandboxing and confinement, and what it degrades to | what an effect can reach |
| standing instructions files | the main way behaviour is shaped |
| skills, and where they are found | extension |
| tools: arguments, refusals, limits | when one behaves unexpectedly |
| shell access and command execution | the sharpest tool |
| external tool servers (MCP or equivalent) | integration |
| hooks or event-driven automation | wiring the tool into a workflow |
| sessions: resuming, listing, where they are stored | recovering yesterday's work |
| context window and compaction | why a long conversation changes character |
| non-interactive and scripted use | CI |
| slash commands | discovery |
| CLI flags and exit codes | scripting |
| environment variables | overriding without rebuilding |
| keybindings and terminal setup | comfort, and terminals that misbehave |
| interface: themes, transcript, scrolling | comfort |
| localization | reading it in another language |
| audit trail and logs | after the fact |
| costs, usage, rate limits | budget |
| troubleshooting and error reference | when it breaks |
| data handling and retention | what leaves the machine |

## Rows deliberately absent

Not on this list, because a reader of this site is using bravebot on their own project rather
than working on bravebot:

- how the project is built, released, tested, or specified
- how a contributor sets up a checkout, an editor, or the agents that work on the repository
- internal structure: crates, modules, types, spec clause ids
