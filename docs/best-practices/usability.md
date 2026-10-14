# Usability

<!-- applicability: always -->

What a reviewer checks about the effort a change asks of the person using bravebot. A change can
pass every test and still take more commands, keystrokes or setup to use than it should.

---

<a id="USE-001"></a>

## A change adds no step the person does not need

**A change asks the person for no command, keystroke, answer or setup that the feature does not
need and that no existing spec clause requires.** A reviewer looks for these in a diff:

- a prompt, confirmation or choice added in front of something that ran without one, or one that
  asks again for an answer the person already gave in the same session;
- a flag, setting or environment variable that has to be set before the feature works, where a
  default would serve most people;
- a value the person has to type or copy that the program already holds, such as a path, an id or
  a name it printed a moment ago;
- a step repeated each session or each directory where doing it once would do;
- a feature reached by more commands or keystrokes than the one it replaces or sits beside.

A prompt, refusal, opt-in or limit on how long an answer lasts that a clause in
[docs/specs/](../specs/README.md) already requires stays, and the diff names the clause. A clause
the same diff adds does not count. [PROMPT-11](../specs/prompting.md#PROMPT-11) registers every kept
answer and how long it lasts, and [TRUST-6](../specs/trust-map.md#TRUST-6) asks at every session
start. This entry is no reason to remove one, to make a key or default approve one, to turn an
opt-in into a default, or to carry an answer further than its clause allows.

**Why:** an added step costs every person who meets it, and neither the tests nor the author, who
already knows what to type, show that cost.
