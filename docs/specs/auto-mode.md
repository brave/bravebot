---
id: AUTO
title: A permission mode in which a classifier answers run and fetch questions
status: normative
governs:
  - crates/agent/src/classifier.rs
  - crates/agent/src/permission_mode.rs
  - crates/ui-bridge/src/wire.rs
  - crates/session/src/jobs.rs
documented-by:
  - none (gap: a page on the auto mode, owed with the settings that are not built)
---

## Scope

A fifth permission mode, `auto`, in which a model that reads only trusted content answers the run
question and the first question about fetching a URL, and in which every other question is put to
the person as it is in asking. What it is given, what it may return, what is recorded, and the
questions it never answers.

The tracking issue is [#3568](https://github.com/brave/bravebot/issues/3568). Claude Code's auto mode
is the reference for the idea. This spec differs from it in one respect that matters: the classifier
here can only remove a question, so its no is the person being asked and never a refusal. Claude
Code's counters for repeated blocks have nothing to count here.

What a mode is and what the other modes answer is [permission-modes.md](permission-modes.md). What
every prompt owes the reader and where an answer is kept is [prompting.md](prompting.md). The other
place a model's word answers a prompt is auto-vetting, [vetting.md](vetting.md#CHECK-11). The rule
that untrusted content never reaches the driver or the planner is
[AGENTS.md](../../agents/AGENTS.md).

## Clauses

<a id="AUTO-1"></a>
### AUTO-1: the key and a flag on a one-shot run reach auto, and nothing else does

The mode key stops on auto between planning and bypassing, so the press after planning lands in auto
and the one after that in bypassing where that is reachable ([MODE-5](permission-modes.md#MODE-5)).
A click on the mode under the box passes it by ([MOUSE-14](mouse.md#MOUSE-14)). `--auto` opens a
one-shot run in it, and is refused beside `--dangerously-skip-permissions` and `--accept-edits`, and
where the line starts no one-shot run, since a session has the key. `/plan` does not set it, the
desktop window does not offer it and a request naming it is refused
([MODE-11](permission-modes.md#MODE-11)), a background session cannot be in it
([BG-7](background-sessions.md#BG-7)), no settings file selects it, and no recorded session restores
it ([MODE-10](permission-modes.md#MODE-10)).

**Why.** Bypassing is on the key's ladder, and auto answers fewer questions than it does, so the key
reaches it. A click is the input a stray press can make, and the mode hands questions to a model, so
a click does not grant it. A checkout or a file a person did not write must not be able to choose
who answers.

`verified-by: bravebot_agent::permission_mode::the_key_reaches_auto_after_planning_and_before_bypass`
`verified-by: bravebot_tui::app::a_click_on_the_permission_mode_moves_as_the_key_does_and_never_into_bypass`
`verified-by: bravebot_ui_bridge::wire::a_window_cannot_choose_auto`
`verified-by: bravebot_session::jobs::a_background_session_cannot_be_in_auto`
`verified-by: bravebot_cli::running::auto_is_refused_with_the_other_mode_flags_and_outside_a_one_shot_run`

<a id="AUTO-2"></a>
### AUTO-2: auto answers a write as accepting edits does, and asks every question no other clause here gives to the classifier

A write goes through unasked, except one that would create a credential and one to a protected path,
which are put to the person ([MODE-2](permission-modes.md#MODE-2)). A deny rule still holds
([MODE-6](permission-modes.md#MODE-6)). Every other question is put to the confirmer behind the mode,
which is a person, or refuses where nobody can be asked ([MODE-7](permission-modes.md#MODE-7)).

**Why.** Claude Code lets reads and working-directory edits skip the classifier. A write is the
question a person can review afterwards in a diff, so no model is needed for it.

`verified-by: bravebot_agent::permission_mode::auto_answers_a_write_as_accepting_edits_does`
`verified-by: bravebot_agent::permission_mode::auto_puts_a_run_and_a_fetch_to_the_confirmer_behind_it`
`verified-by: bravebot_cli::running::an_auto_run_writes_and_a_plain_run_does_not`
`verified-by: bravebot_cli::running::an_auto_run_refuses_a_write_that_would_create_a_credential`

<a id="AUTO-3"></a>
### AUTO-3: the classifier is consulted for a confined run and for the first question about a URL, and for nothing else

A run is put to it only where the sandbox confines the line, the line did not ask to start with no
profile ([SANDBOX-29](sandboxing.md#SANDBOX-29)), no standard input comes from a reference, no
credential scope was asked for, and nothing private is released. A fetch is put to it for
`fetch_url`, for the first question about the URL, and not for `download_url`, which puts bytes into
a file, nor for the host a redirect led to ([FETCH-4](tools/fetch-url.md#FETCH-4)). It is consulted
only in auto, only in a turn of a session the person has typed to, and not for a delegate, whose task
is a sentence the driver composed. Where it is not consulted the question goes to the confirmer
behind the mode.

**Why.** The classifier is shown the words of the line and not the binary they resolve to, which can
be a file in the tree. The sandbox is what holds the line to those words. The prompts it may answer
are the ones whose routing field the planner typed from a context that holds nothing untrusted, so a
person could approve the field alone ([RUN-2](tools/run.md#RUN-2), [FETCH-2](tools/fetch-url.md#FETCH-2)).

`verified-by: bravebot_agent::permission_mode::auto_answers_a_run_only_on_the_classifiers_allow_word_for_a_confined_line`
`verified-by: bravebot_agent::permission_mode::auto_answers_a_fetch_only_on_the_classifiers_allow_word_and_not_a_redirect`
`verified-by: bravebot_agent::turn::a_run_with_nothing_to_hold_it_is_not_put_to_the_classifier`
`verified-by: bravebot_agent::turn::no_mode_but_auto_consults_the_classifier`
`verified-by: bravebot_agent::turn::a_run_with_a_scope_a_reference_or_a_private_release_is_not_put_to_the_classifier`

<a id="AUTO-4"></a>
### AUTO-4: the classifier answers no question that a mode may not answer, and none whose bytes can derive from untrusted content

These stay with the person in this mode: a question the planner posed and a line the person typed
([MODE-7](permission-modes.md#MODE-7)), the plan prompt ([MODE-14](permission-modes.md#MODE-14)), the
limit and repeated-call questions ([TURN-8](turns.md#TURN-8), [TURN-9](turns.md#TURN-9)), a proposed
rule ([RULE-3](tools/propose-rule.md#RULE-3)), a message to another session
([MSG-3](session-messages.md#MSG-3)), a credential write and a write to a protected path, the exposing
read, the vouch, the list of a server's tools, a server's start and move, a path request, a host
question, a language server's start, the startup trust question, and a plan. It also never answers
reading output, a vetted read, or a case whose routing field is a reference (`path_ref`,
`contents_ref`, `stdin_ref`).

**Why.** Each of these is a question the person alone answers, because a standing effect follows from
it or because the bytes it shows can be an attacker's. A model's word is not a person having read
something ([PROMPT-9](prompting.md#PROMPT-9)).

`verified-by: bravebot_agent::permission_mode::auto_puts_every_question_a_mode_may_not_answer_to_the_person`

<a id="AUTO-5"></a>
### AUTO-5: the classifier is given the person's typed lines and one rendering of the action

The lines are the ones recorded as typed when they arrived, never chosen by reading their words
([LAYER-6](layering.md#LAYER-6)). A line sent with files, a message nobody recorded the origin of, a
file the person named, the planner's words and a result are not among them. The newest twelve are
sent, each cut to 4000 bytes. The action is the line the planner typed and its directory, or the URL
and its host. Both are plain text the driver already holds, so nothing is released from a label to
build the request and the request carries no tool, no tool result, no body, no output, no listing
name, no resolved binary, no remark of a processor, no description a server wrote and no instruction
file from the checkout. A row for the reply is in [LABEL-8](labels.md#LABEL-8).

**Why.** The verdict is a function of its input, and when the input holds only trusted content the
verdict is labelled trusted and the driver may read it ([LABEL-8](labels.md#LABEL-8),
[GOAL-5](goal.md#GOAL-5)). This is narrower than the goal judge, which is given everything the planner
was given. The planner may hold vouched files, and a vouched file is the tree's words.

`verified-by: bravebot_agent::conversation::only_the_lines_recorded_as_typed_are_handed_to_the_classifier`
`verified-by: bravebot_agent::classifier::the_request_holds_the_persons_lines_and_the_action_and_offers_no_tools`
`verified-by: bravebot_agent::classifier::a_long_history_is_bounded_and_a_line_is_cut_at_a_character_boundary`
`verified-by: bravebot_agent::classifier::a_fetch_names_the_url_and_the_host`

<a id="AUTO-6"></a>
### AUTO-6: the classifier returns one word, and anything but the allow word puts the question to the person

The reply is the word `allow` after trimming and lowercasing and after nothing else, so `allow.` and
`allow, but` are not it ([CHECK-4](vetting.md#CHECK-4)). A block, an unreadable reply, a timeout, an
error, a reply labelled quarantined and a refused model are the same result: the question goes to the
person, or is refused where nobody can be asked ([CHECK-12](vetting.md#CHECK-12)). The classifier is
not consulted at all where the planner's context holds private content
([ADVISOR-3](tools/advisor.md#ADVISOR-3)) or has met something untrusted, or where the machine's
settings refuse the model. It runs on the session's own model.

**Why.** The classifier removes a question and never refuses one on its own word, so a person is asked
at most once more than in asking. A reply an attacker could make fail is a reply that must not
approve ([MODE-4](permission-modes.md#MODE-4)).

`verified-by: bravebot_agent::classifier::only_the_allow_word_alone_allows`
`verified-by: bravebot_agent::turn::auto_answers_a_confined_run_on_the_classifiers_allow_word_and_asks_the_person_otherwise`
`verified-by: bravebot_agent::turn::a_context_that_is_private_or_untrusted_is_not_put_to_the_classifier`
`verified-by: bravebot_cli::running::an_auto_run_runs_a_command_the_classifier_allows_and_refuses_one_it_does_not`

<a id="AUTO-7"></a>
### AUTO-7: the planner is told nothing about what decided, and the classifier's reason is neither kept nor shown

A question the classifier did not allow is worded as any other refusal or question is
([TOOL-4](tools/tool-surface.md#TOOL-4), [CHECK-9](vetting.md#CHECK-9)). The planner is not told that a
classifier exists, and no sentence it reads says a person is or is not shown the question
([RUN-14](tools/run.md#RUN-14)). Nothing but the word is read from the reply, so the reason the
classifier wrote reaches neither the trail ([TRACE-2](trace.md#TRACE-2)) nor the planner.

**Why.** A planner that learns a model decides is handed something to argue with, and a reason is free
text.

`verified-by: none`

<a id="AUTO-8"></a>
### AUTO-8: the trail names the classifier and its model, and tells its answer from a person's

Each question put to the classifier writes an `auto_mode` entry naming the model, the kind of question
(`run` or `fetch`), the word, and what the call cost. It holds no word of what the classifier was
shown or wrote. The `approval` entry after it reads "answered by auto mode, nobody was asked" where
the classifier's word answered the question, and "no mode answered, left to the confirmer" where it
did not, so a question left with the person is not credited to the mode
([TRACE-7](trace.md#TRACE-7), [MODE-8](permission-modes.md#MODE-8)). The same holds for the first
question about a URL, which no mode's answer was recorded for before.

**Why.** A reader of the trail must be able to tell a question a person answered from one a model did
([CHECK-8](vetting.md#CHECK-8)), and a release credited to the wrong one is a defect.

`verified-by: bravebot_agent::turn::auto_answers_a_confined_run_on_the_classifiers_allow_word_and_asks_the_person_otherwise`
`verified-by: bravebot_agent::turn::auto_answers_a_fetch_on_the_classifiers_allow_word_and_asks_the_person_otherwise`
`verified-by: bravebot_agent::permission_mode::the_record_credits_auto_only_for_a_question_the_classifier_answered`

<a id="AUTO-9"></a>
### AUTO-9: a delegate is answered as the turn that spawned it, and the classifier is not consulted for it

A delegate inherits the mode ([MODE-9](permission-modes.md#MODE-9)). Its questions reach the
confirmer behind the mode as the spawning turn's do, and the classifier is not put any of them, since
a delegate's task is a sentence the driver composed and no line the person typed is in its
conversation.

**Why.** A classifier shown a task the driver wrote would be judging the driver's words, which says
nothing about what the person wanted.

`verified-by: none`

## Known costs

- The classifier cannot judge the body of a write or the binary a program name resolves to, because
  both can derive from the tree. A write is answered as in accepting edits. A run relies on the
  sandbox to hold what the line does, which is why only a confined line is put to it.
- GHSA-mhg7-666j-cqg4 is a defect in another agent in which a validator judged whether a command was
  safe to run. Here the classifier's yes answers one question and is bound to the exact line shown
  ([PROMPT-5](prompting.md#PROMPT-5)), and the sandbox holds the line.
- After a slot is promoted, the planner's arguments are private, and the classifier is not consulted
  for the rest of the session (AUTO-6). The questions go to the person.
- Each run or fetch question costs one model round trip, on the session's own model.
- A confined plain line in a trusted directory already runs unasked ([RUN-5](tools/run.md#RUN-5)), so
  the classifier is consulted in a directory the person has not trusted.

## Not built

- A setting that makes the mode unreachable for a layer, as `permissions.bypassUnreachable` does for
  bypassing ([PERM-17](permissions.md#PERM-17), [PERM-18](permissions.md#PERM-18)). Until it exists an
  administrator cannot remove the mode from the ladder.
- A model of its own for the classifier, read from the home layer only
  ([BACKEND-24](backends.md#BACKEND-24), [COMPACT-15](compaction.md#COMPACT-15)). It runs on the
  session's model.
- The desktop window, which offers three modes ([MODE-11](permission-modes.md#MODE-11)).
- A flag that opens an interactive session in the mode. The key reaches it.
- Instruction files as input to the classifier, which Claude Code includes.
