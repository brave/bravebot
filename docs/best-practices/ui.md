# Desktop UI

<!-- applicability: paths:ui/ -->

A change under `ui/`. For one a person sees or interacts with (components, layout, styles, icons,
copy, dialogs, menus, cards) the quality bar itself is
[the design skill](../../agents/skills/design/SKILL.md). What follows is what a
reviewer decides from the diff, and none of it restates a rule `check-nala` already enforces.

---

<a id="UI-001"></a>

## A UI change is held to the quality bar

**A pull request that adds or changes a surface says which parts of the quality bar it checked and
how.** The skill is the bar, and the description names the evidence: a gallery screenshot in light
and dark, a driver that ran, or reasoning where neither could. A state that was not captured or a
driver that was not run is said to be so. A description that claims a visual result nobody saw is
the failure this rule exists for.

---

<a id="UI-002"></a>

## New chrome has to earn its place

**A new element, banner, status line, or button answers what it helps someone reading or steering
the agent right now.** A marker that restates state the UI already shows (a working row, a sidebar
dot, the composer) is removed or merged, not restyled. Content sits where it belongs: history in
the transcript, a blocker on sending at the composer, an app problem as a toast, and nothing but
identity and actions in a header.

---

<a id="UI-003"></a>

## No control that approves for the person

**A UI change adds no shortcut, default, or focus behaviour that lets a keypress or a stray click
approve a decision card.** A card that appears does not take focus from the composer, and a
decision is reached by a deliberate action on the card. A file reaches a turn as context in one of
four ways: the native picker's grant, an `@` name in the prompt the person sends, a file a person
dropped on the window, or a picture a person pasted into the composer. An `@` name is trusted
context on the terminal's terms ([NAME-1](../specs/naming-files.md#NAME-1),
[NAME-2](../specs/naming-files.md#NAME-2)). The bridge reads each `@` name back out of the sent
prompt itself and the agent's workspace confines it to the conversation's folder, so neither the
renderer nor the main process supplies a path. A dropped file is taken by the preload only from a
drop event the browser marks trusted (`isTrusted`), and the main process checks it and grants it by
an opaque id ([DROP-11](../specs/dropping.md#DROP-11)), so the page never holds the path. A pasted
picture is taken only from a paste event the browser marks trusted, the main process reads the
clipboard itself and grants the picture by an opaque id
([PASTE-11](../specs/pasting.md#PASTE-11)), so the page never holds the bytes. The reasoning is in
`ui/docs/security.md` and `ui/docs/file-access-security.md`.

---

<a id="UI-004"></a>

## Security markings survive a restyle

**A diff that touches the quarantine, untrusted-card, or trust-label styling leaves each one as
recognisable as it was.** The marking does not depend on colour or a background image alone, so it
still reads under forced colours. A restyle that makes an untrusted card look like a trusted one
fails here even when every check passes.

---

<a id="UI-005"></a>

## A moved label or location moves its drivers

**A change that renames a visible label or relocates an element updates the drivers and demo scenes
that look for it, in the same pull request.** Button texts that tests or the security docs pin stay
word-for-word. A driver left asserting on the old place is a broken gate.

---

<a id="UI-006"></a>

## The desktop UI reuses existing code instead of copying it

**A change under `ui/` follows [SHARE-001](shared-implementation.md#SHARE-001) for shared
Rust behavior and reuses existing UI code.** Reach the owning Rust crates through
`bravebot-ui-bridge` or `bravebot-ui-files` as appropriate.

A new component, hook, helper, or constant uses and, where needed, extends the existing UI
implementation instead of copying it. Window-only behavior such as layout, focus, animation,
and menu wiring stays in `ui/`.
