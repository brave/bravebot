---
id: CONSULT
title: Consulting a bot by name
status: proposed
governs:
  - crates/mentions/src/lib.rs
  - crates/core/src/delegate.rs
  - crates/core/src/policy.rs
  - crates/agent/src/agents.rs
  - crates/agent/src/tools.rs
  - crates/agent/src/tools/offered.rs
  - crates/agent/src/turn.rs
  - crates/agent/src/turn/prompts.rs
  - crates/agent/src/turn/setup.rs
  - crates/session/src/sessions.rs
  - crates/tui/src/app.rs
  - crates/tui/src/state/completion.rs
  - crates/tui/src/render/transcript.rs
  - crates/ui-bridge/src/bridge.rs
  - crates/ui-bridge/src/consult.rs
  - crates/ui-bridge/src/mentions.rs
  - ui/src/renderer/components/BotDetails.tsx
  - ui/src/shared/mentions.ts
  - ui/src/renderer/components/MentionList.tsx
  - ui/src/renderer/components/Composer.tsx
documented-by:
  - docs/website/docs/using/consulting-a-bot.md
  - docs/website/docs/customize/agents.md
---

## Scope

Writing `@name` in a conversation to put one question or one task to another bot, and getting its
answer back in the same conversation. What the line looks like, what the asked bot is given, what it
may do, what comes back, and how a bot in the conversation may suggest asking another.

A clause reads `verified-by: none` until the work it describes lands.

A **bot** here is a definition, as [delegation.md](delegation.md) defines one, whose name is a slug:
the desktop's bots are definitions of that kind ([definition-memory.md](definition-memory.md)). The
**host** is the run whose conversation the tag is written in: the session's own planner, or the
definition the session's turns address. A **consult** is the run started for the bot that was tagged.
The **brief** is everything the consult is given to work from.

What a definition is, where one is read from and what it may hold is
[delegation.md](delegation.md). Running a definition in place of the planner for a whole turn is
[addressing-a-definition.md](addressing-a-definition.md), and `/agent` is unchanged by this file.
What `@` names when it names a file or a past session is [naming-files.md](naming-files.md). How
quarantined content is promoted, and the second opinion a promotion prompt carries, is
[vetting.md](vetting.md). Where a clause here changes one of those, that file says so at the place it
changes.

## The comparison

| | Addressing a definition (`/agent`) | Delegating to one | Consulting one (`@name`) |
|---|---|---|---|
| who starts it | a person's line | the planner | a person's line, or a person approving the host's proposal |
| what it works from | the whole conversation | a task the planner wrote | a brief the person sent |
| who is watching | the person | nobody | the person |
| what crosses back | everything; the exchange is the session's | the report | the reply |
| may ask the person | yes | no | yes |

A consult takes from each column the half whose reason applies to it. That is the whole argument of
[CONSULT-1](#CONSULT-1).

## What it is

<a id="CONSULT-1"></a>
### CONSULT-1: a consult is a person's own run whose context is the brief, and only its reply crosses back

A consult holds the screen, the confirmer and the way to ask a person a question, as a run a person
addresses does. Its context is the brief and what it reads while it works, not the host's
conversation. When it ends, its reply is what reaches the host; what it read, ran and narrated stays
with it, as a delegate's does.

**Why each half.** A delegate may not ask a person anything because its task came from a planner and
nobody set it up. A consult's brief is one a person sent, and the person is watching, so that reason
is absent and the consult may ask. A delegate's reading stays behind so that the asker is told the
answer rather than handed the log. That reason holds whoever is watching, so a consult keeps it.
Neither clause is relaxed: each is kept or left exactly where its reason is.

`verified-by: bravebot_tui::app::a_consult_runs_over_its_own_conversation_and_the_hosts_is_set_aside`
`verified-by: bravebot_agent::turn::a_consult_works_from_its_line_and_hands_back_only_its_reply`

## The line

<a id="CONSULT-2"></a>
### CONSULT-2: `@name` is a consult where the name is a bot this session resolved, and a file otherwise

At send, each `@word` in a line a person wrote is compared against the bots this session resolved:
the definitions a turn starting now would resolve, less the ones a `deny` rule covers and the ones
that opted out ([CONSULT-14](#CONSULT-14)), whose name is a slug. The three kinds `reader`, `checker` and `worker` are not bots and are never matched. A word
that matches a bot is a consult of that bot. A word that does not is a file or a past session as
[naming-files.md](naming-files.md) already says.

A bot wins over a file of the same name. `@./name` always names the file, because a slug holds no
`/`. Which one a word named is decided once, at send, and written into the session's record beside
the line, so a bot made later does not change what an earlier line meant. The desktop draws a bot's
face beside each word the bridge says consulted it, when the line is sent and when the session is
resumed, and draws no other word as a tag.

A slug is lowercase letters and digits in runs joined by single hyphens, at most 64 characters, the
class a definition that keeps a memory is already held to. So `@README.md`, `@src/lib.rs` and any
name holding a space or a colon are never bots, and `@session:<id>` can never collide with one,
since a definition's name may carry no colon.

Sentence punctuation straight after a bot's name is not part of it: a word that is a bot's name
followed by any of `,` `:` `;` `.` `!` `?`, a closing bracket or a closing quote consults that bot,
and names no file. So `@reviewer, is this safe?` and `ask @reviewer: why?` consult `reviewer`. A word
that is not a bot's name once that punctuation is set aside is read as before, punctuation and all,
and a name that runs on past a dot, such as `@reviewer.md`, is a file.

**Why a bot wins.** The person typing `@reviewer` with a bot by that name in the list is asking the
bot far more often than reading a file with no extension at the root. The file stays one character
away, and the list says which is which ([CONSULT-3](#CONSULT-3)).

**Why only slugs.** A definition that keeps no memory may otherwise be named anything, including a
sentence. A name that becomes a word in somebody's prompt is held to a class that cannot read like
one.

This amends [ADDRESS-2](addressing-a-definition.md#ADDRESS-2), which rejected `@name` for
definitions because one prefix would mean two things. The prefix now means two things, settled at
send by a fixed order, and the list draws the difference before the line is sent.

`verified-by: bravebot_mentions::lib::a_word_naming_a_bot_consults_it_and_names_no_file`
`verified-by: bravebot_mentions::lib::a_session_reference_is_never_consulted`
`verified-by: bravebot_mentions::lib::punctuation_after_a_bots_name_is_not_part_of_it`
`verified-by: bravebot_tui::app::a_line_naming_a_bot_consults_it`
`verified-by: bravebot_tui::app::a_bots_name_followed_by_punctuation_consults_it`
`verified-by: bravebot_tui::app::a_word_that_consults_a_bot_names_no_file`
`verified-by: bravebot_tui::app::a_bot_that_opted_out_is_not_consulted`
`verified-by: bravebot_ui_bridge::consult::a_line_naming_a_bot_consults_it`
`verified-by: bravebot_ui_bridge::consult::a_bot_that_opted_out_is_not_consulted`
`verified-by: bravebot_ui_bridge::mentions::a_word_naming_a_bot_is_not_refused_as_a_missing_file`
`verified-by: bravebot_mentions::lib::a_tag_is_each_word_that_consults_a_bot`
`verified-by: bravebot_ui_bridge::wire::a_resumed_line_is_tagged_with_the_bots_that_answered_it`
`verified-by: bravebot_ui_bridge::consulting::a_consult_turn_tags_only_the_bots_its_line_consulted`

<a id="CONSULT-3"></a>
### CONSULT-3: typing `@` offers bots first, and taking a row writes a word and runs nothing

The `@` list offers the bots this session resolved, as [CONSULT-2](#CONSULT-2) counts them, before
the files and the past sessions, so a bot that opted out is not offered. A bot's
row is marked as a bot and carries its model and its bound. Taking the row writes `@name ` into the
line. Nothing runs until the line is sent and, where the brief waits in a card, until the person sends
that too ([CONSULT-5](#CONSULT-5)).

**Why this is allowed now.** [ADDRESS-6](addressing-a-definition.md#ADDRESS-6) keeps definition
names out of the list because Enter on a row there would write a command line that starts a run, so
a name written to read like an instruction would be one keystroke from dispatching itself. Here a row
writes one word of a sentence, and sending the sentence is a second act.
A name is also a slug ([CONSULT-2](#CONSULT-2)), which no instruction fits in.

This amends [ADDRESS-6](addressing-a-definition.md#ADDRESS-6) for the `@` list only. `/agent` still
offers no names. It amends the order in [naming-files.md](naming-files.md), where the list opens on
the workspace and offers past sessions after the files.

`verified-by: bravebot_mentions::lib::the_bots_a_word_could_become_are_offered_marked_as_bots`
`verified-by: bravebot_mentions::lib::a_bot_typed_in_full_sends_rather_than_completes`
`verified-by: bravebot_tui::app::a_bot_is_offered_ahead_of_the_files_and_taking_it_writes_its_name`
`verified-by: bravebot_ui_bridge::mentions::a_bot_is_offered_first_and_marked_as_one`

<a id="CONSULT-4"></a>
### CONSULT-4: only a person starts a consult, by sending a line or approving a proposal

A consult starts from a line a person sent, or from a person approving a proposal the host made
([CONSULT-12](#CONSULT-12)). A `@name` in anything else is text: a reply, a brief, a file, a
command's output, a tool's result, or a line recalled from a transcript without being sent again.

**Why.** A consult runs under a prompt the host did not write, which is a decision a turn may not
take on its own. The endorsement is the person's act, so only that act may produce one.

`verified-by: bravebot_tui::app::only_a_line_a_person_sent_to_the_host_consults`

## The brief

<a id="CONSULT-5"></a>
### CONSULT-5: the host drafts the brief, and a brief that has met nothing to read again is sent at once

The host drafts a brief from its conversation for each bot the line consults, whether the session's
turns address a definition or not. The brief opens with the person's line as they wrote it, and the
host's draft follows.

A brief that holds nothing private is sent as soon as it is drafted, and is drawn as the person's
line in the transcript, so what the bot was sent is on the screen. That holds for one the planner
could have held without anyone's yes and for one a person's yes on its promotion card kept
([CONSULT-6](#CONSULT-6)). Sending the line was the grant, and for a promoted brief so was the yes.

A brief drafted after the host was shown private content waits in full, editable, in the box the
line was typed in (the terminal's input line, the desktop's composer), under a note naming the bot
and its bound. Sending it is what starts that consult. Declining it with Esc starts nothing, and the line
goes back into the box. Wiping the brief out of the box declines it as well, so what is typed next
goes to the host.

A line that reaches the host without being briefed, such as one queued behind a running turn, goes
to the host and consults nothing.

The files and past sessions the person's line names come with the brief as they would to any turn,
and no others: a file the host read on its own is not handed to the consult because the draft
mentions it.

A quarantined slot the host holds goes into the brief as a reference, never as its bytes. The consult
is handed the same reference, which it may pass to a processor and may not read as trusted.

**Why a draft.** The person tagging a bot mid-conversation means "about this", and the host is the
one that knows what "this" is.

**Why a clean brief does not wait.** Sending the line is the person's act, which is what
[CONSULT-4](#CONSULT-4) asks for. A brief the planner could have held carries nothing out of
quarantine, and a consult may do no more than its host may ([CONSULT-7](#CONSULT-7)), so the bot is
given no way out the host did not already have. What waiting would add is a chance to reword the brief,
and that is not worth a second keystroke on every consult.

**Why a promoted brief does not wait either.** Its card drew the whole brief, so the yes was given to
the words that are sent. Asking again would be a second yes to the same words.

**Why a private brief still waits.** It hands the bot what the host held, and the person sees what is
shared before it is.

**Why every host.** A plain session's host knows what "this" is as well as a definition's does. A bot
sent only the line answers a question it cannot see, and asks what "this" means.

**Why not the whole conversation.** The consult would start out holding everything the host was ever
shown, and its reply would be labelled by all of it.

`verified-by: bravebot_agent::aside::a_brief_is_asked_over_the_hosts_exchange_and_opens_with_the_line`
`verified-by: bravebot_tui::app::a_line_naming_a_bot_asks_the_host_for_a_brief`
`verified-by: bravebot_tui::app::a_line_consults_only_through_its_brief`
`verified-by: bravebot_tui::app::a_plain_session_sends_its_hosts_brief_as_a_definitions_does`
`verified-by: bravebot_tui::app::a_brief_the_host_could_have_held_is_sent_at_once`
`verified-by: bravebot_tui::app::a_private_brief_waits_in_the_box_to_be_sent`
`verified-by: bravebot_agent::turn::a_brief_drafted_over_an_untrusted_exchange_is_kept_only_on_a_persons_yes`
`verified-by: bravebot_ui_bridge::consulting::the_hosts_brief_is_offered_and_only_sending_it_consults_the_bot`
`verified-by: bravebot_ui_bridge::consulting::the_brief_is_drafted_on_the_model_the_window_asks_for`
`verified-by: bravebot_ui_bridge::consulting::a_line_consults_a_bot_only_through_a_drafted_brief`
`verified-by: bravebot_tui::app::a_private_brief_is_offered_under_a_note_naming_its_bound`
`verified-by: bravebot_tui::app::esc_on_a_brief_puts_the_line_back`
`verified-by: bravebot_tui::app::a_declined_brief_is_not_offered`
`verified-by: bravebot_ui_bridge::drafted_tests::a_brief_holding_nothing_private_is_marked_to_go_at_once`
`verified-by: bravebot_ui_bridge::drafted_tests::a_private_brief_is_marked_to_wait_and_held_as_private`
`verified-by: bravebot_ui_bridge::drafted_tests::a_declined_draft_holds_no_brief`
`verified-by: bravebot_ui_bridge::drafted_tests::a_stopped_draft_holds_no_brief`
`verified-by: bravebot_ui_bridge::consulting::a_held_brief_whose_bot_can_no_longer_be_consulted_is_refused`

<a id="CONSULT-6"></a>
### CONSULT-6: a brief drafted by a host that has met untrusted content is a promotion, and is checked first

A host whose context has met untrusted content drafts a brief labelled by that context, so the draft
is quarantined. The card that sends it is then a promotion prompt. The second opinion every promotion
prompt carries is taken over the whole brief before the card is drawn, and the card shows what it
said. A yes promotes that brief once, for that consult. Auto-vetting never answers this card: a
person always does.

A brief the host drafted after it was shown private content is private, and the consult starts out
holding what the host held.

**Why not refuse.** A delegate is refused to a run that has met untrusted content because the
delegate's task is written by that run and nobody reads it. A brief is written by that run too, but
a person reads all of it before it is sent, with a second opinion beside it. That is the rule every
other way out of quarantine already follows, so the brief is one more of them, and not an exception
to the refusal.

This amends [DELEGATE-2](delegation.md#DELEGATE-2) for a consult only, and adds a row to the table
of promotion prompts in [CHECK-10](vetting.md#CHECK-10): the brief card, where a yes promotes one
brief once and the check reads the whole brief.

`verified-by: bravebot_core::policy::a_brief_is_promoted_once_for_the_bot_it_was_endorsed_for_and_stays_private`
`verified-by: bravebot_tui::app::a_declined_brief_is_not_offered`
`verified-by: bravebot_tui::app::a_private_brief_sends_a_consult_that_holds_private_content`
`verified-by: bravebot_agent::tools::a_brief_from_an_untrusted_context_is_refused_before_any_card`

## What it may do

<a id="CONSULT-7"></a>
### CONSULT-7: a consult holds its bot's capabilities narrowed by the host's, and keeping a memory adds none

A consult holds what the host's turn holds, intersected with its bot's kind, tools and servers, by
the same rule a run a person addresses is narrowed by. A bot of a wider kind than the host gets the
host's reach. It works in the host's working directory.

A consult keeps its bot's memory there, under the same rule as any run of that definition: it writes
the file with the tools it holds, the write is shown to the person, and a write carrying quarantined
content leaves the memory untrusted in later sessions too. A `reader` writes nothing, so it does not
change its memory, and nor does a `worker` consulted from a host that may not write.

**Why no exception for the memory.** A tool that wrote only a bot's own memory would be a write gated
on which file it is. Every other write is gated on whether the data and the destination are trusted,
and that is the gate a memory needs.

`verified-by: bravebot_core::policy::a_consult_holds_no_more_than_the_definition_it_was_consulted_from`
`verified-by: bravebot_ui_bridge::consulting::a_consult_holds_no_more_than_the_definition_it_was_consulted_from`
`verified-by: bravebot_core::policy::a_consult_from_a_host_the_session_did_not_resolve_does_not_run`
`verified-by: bravebot_agent::turn::a_consult_precommits_the_host_it_was_sent_from`

<a id="CONSULT-8"></a>
### CONSULT-8: a consult cannot consult or propose a consult

A consult is not offered the proposal tool, and a `@name` in what it writes is text
([CONSULT-4](#CONSULT-4)). It may start the delegates its definition names, as a run a person
addresses may.

**Why.** A consult started by another consult would run on a brief no person sent, three speakers
deep, where a person approving a write cannot tell which question it belongs to. Delegates do not
raise this, because their depth and number are already bounded and nothing of theirs reaches the
screen as a speaker.

`verified-by: bravebot_agent::turn::setup::a_consult_is_proposed_only_where_a_person_can_send_it_and_never_from_a_consult`
`verified-by: bravebot_agent::tools::a_proposal_nobody_was_offered_is_answered_as_an_unknown_tool`

<a id="CONSULT-9"></a>
### CONSULT-9: several bots tagged in one line are consulted one after another

A line tagging several bots consults them one at a time, in the order the line names them. Each
consult is a turn of its own, given a brief of its own, and its reply is handed to the host when it
arrives, before the next bot is asked. The host drafts the next bot's brief once the consult before
it has replied, so the draft can draw on that reply, and that brief is sent or offered as the first
would be ([CONSULT-5](#CONSULT-5)).

A bot that may not be consulted, such as one whose model needs a sign-in
([CONSULT-10](#CONSULT-10)), is noted and skipped, and the bots after it are still asked. A consult
that fails or is stopped ends the rest: the line goes back into the box as it does for a single
consult. So does Esc on any brief that waits in the box. Sent again unchanged, that line consults
only the bots that had not yet replied.

The next bot is asked before anything the person queued behind the line, since the line is not
finished until each bot it named has replied or the rest were ended.

**Not built: consulting them at once.** Running the consults alongside each other, with their
questions to the person asked one at a time and each saying which bot asks, would need several
consults drawn on the screen together. Two consults writing one path would then meet as two writers
of one path do: the second is refused while the first is in progress.

`verified-by: bravebot_tui::app::bots_named_together_are_consulted_in_the_order_the_line_names_them`
`verified-by: bravebot_tui::state::a_line_put_back_after_a_reply_resumes_at_the_next_bot`
`verified-by: bravebot_tui::app::a_refused_bot_is_skipped_and_the_next_is_consulted`
`verified-by: bravebot_tui::app::a_later_brief_that_is_not_sent_puts_the_line_back`
`verified-by: bravebot_tui::state::the_next_bot_is_consulted_only_once_the_one_before_replied`
`verified-by: bravebot_tui::state::the_bots_after_a_brief_go_with_it_and_esc_drops_them`
`verified-by: bravebot_agent::consult::bots_named_together_are_consulted_in_the_order_the_line_names_them`
`verified-by: bravebot_agent::consult::a_refused_bot_is_skipped_and_the_next_is_consulted`
`verified-by: bravebot_ui_bridge::consult::only_the_bots_left_are_picked_from`
`verified-by: bravebot_ui_bridge::bridge::the_bots_a_line_left_are_consulted_by_the_next_poll_and_never_given_to_the_host`
`verified-by: bravebot_ui_bridge::bridge::a_consult_still_running_keeps_the_rest_and_stopping_it_drops_them`
`verified-by: bravebot_ui_bridge::consulting::the_next_bot_is_briefed_from_the_line_and_not_the_brief_before_it`

<a id="CONSULT-10"></a>
### CONSULT-10: the model that will answer and the most the consult may spend are named before it starts

The model a bot runs on and the bound it runs under, its rounds and its token limit where it has
one, are named on its row in the `@` list, in the note shown while its brief is drafted, and in the
note under a brief that waits in the box. A bot runs on the model its definition names, and on the
session's model where it names none. A bot whose model needs a sign-in this machine has not made is
noted as refused, with the model it asked for, and is not consulted. The host's model is never
substituted.

**Why.** Consulting a bot on a costly model from a conversation on a cheap one spends past a boundary
the person may not know is there. The draft is the last moment they can stop it with the cost in
view, where the brief is sent at once.

`verified-by: bravebot_tui::app::the_drafting_note_names_the_bots_model_and_bound`
`verified-by: bravebot_tui::app::a_private_brief_is_offered_under_a_note_naming_its_bound`
`verified-by: bravebot_tui::app::a_bot_whose_model_needs_a_sign_in_is_refused_and_not_consulted`
`verified-by: bravebot_agent::tools::a_proposed_bot_whose_model_needs_a_sign_in_is_refused_on_the_card`
`verified-by: bravebot_tui::confirm::a_refused_consult_card_names_the_model_and_takes_no_yes`
`verified-by: bravebot_agent::consult::a_bot_whose_model_the_machine_refuses_is_not_consulted`
`verified-by: bravebot_agent::consult::a_bot_whose_model_needs_a_sign_in_is_not_consulted`
`verified-by: bravebot_agent::turn::a_proposed_consult_runs_on_its_bots_model_over_the_command_lines`

## What comes back

<a id="CONSULT-11"></a>
### CONSULT-11: the reply is drawn under the bot's name and labelled by the consult's own context

The reply is drawn in the conversation under the name the driver matched, as a speaker of its own.
What the consult streams before it, its narration and its tool lines, is drawn under the same name
from the moment the consult starts, so none of it reads as the host's. The name is the driver's
routing, never a model's words. The session's record keeps which bot answered, so a resumed session
and an exported transcript show the same speaker.

The reply reaches the host labelled by the consult's own context, as a delegate's report does. A
consult that met nothing untrusted hands the host its words. One that did hands the host a reference
to them, and the person may promote it through the ordinary promotion prompt, with its second opinion.

**Why the consult's own label.** Quarantining every reply would make the host unable to act on a
review that read nothing but the person's own trusted files, so the person would restate each one.
Trusting every reply would let a consult that read an untrusted page steer the host. The label is
already the fact that tells the two apart.

`verified-by: bravebot_agent::consult::a_trusted_reply_reaches_the_host_under_the_drivers_note`
`verified-by: bravebot_agent::consult::an_untrusted_reply_is_a_reference_and_leaves_the_host_trusted`
`verified-by: bravebot_agent::consult::a_private_reply_leaves_the_host_holding_private_content`
`verified-by: bravebot_agent::consult::a_consult_that_failed_leaves_the_host_alone`
`verified-by: bravebot_tui::state::a_resumed_consult_is_drawn_as_the_bots_reply`
`verified-by: bravebot_tui::app::a_line_naming_a_bot_consults_it`
`verified-by: bravebot_ui_bridge::wire::a_consulted_reply_crosses_under_the_bot_that_answered`
`verified-by: by-construction (the desktop renderer is not a crate this workspace compiles, so ui/scripts/drive-consult.mjs drives the real app and bridge against a stub model whose reviewer narrates and reads a file before it replies, and asserts the narration, the read and the reply sit in one group under one header naming the reviewer, that the tester has its own, and that none of the reviewer's narration is drawn outside its group)`

<a id="CONSULT-15"></a>
### CONSULT-15: the host is told, in the driver's words, which bot answered a line it did not answer

After a consult a person's line started, the host's next turn holds the line, then a note the driver
writes, naming the bot, its definition and the model that answered, and then the reply as
[CONSULT-11](#CONSULT-11) labels it. This holds whether or not the session's turns address a
definition, and whatever the brief said beyond the line. The note is never taken from the reply.

**Why a note.** Without one, the line reads to the host as addressed to it and unanswered, and the
reply reads as its own words or as the person's. Which bot answered is a fact the driver holds,
because it is the one that matched the name, and a reply is model output that could claim any name.

**Why the reply too.** A person who writes "fix the first thing it raised" next is pointing at the
reply, and a host that had been shown only that a consult happened would have to ask them what it
said.

`verified-by: bravebot_agent::consult::a_trusted_reply_reaches_the_host_under_the_drivers_note`
`verified-by: bravebot_agent::consult::the_line_comes_before_the_reply`
`verified-by: bravebot_agent::consult::a_reply_is_handed_back_to_the_host`
`verified-by: bravebot_agent::consult::a_reply_the_host_may_not_be_given_becomes_the_consults_refusal`
`verified-by: bravebot_tui::app::a_stopped_consult_leaves_the_host_as_it_was`
`verified-by: bravebot_ui_bridge::consulting::the_host_is_handed_the_line_and_not_the_hosts_draft`

<a id="CONSULT-12"></a>
### CONSULT-12: a host may propose a consult, and the proposal runs nothing until a person sends it

The host is offered a tool that proposes a consult: a bot's name, matched against the bots this
session resolved, and a brief. The proposal is put to the person on a card, under
[CONSULT-5](#CONSULT-5), [CONSULT-6](#CONSULT-6) and [CONSULT-10](#CONSULT-10). Sent, the consult runs
and the reply is the tool's result, labelled as [CONSULT-11](#CONSULT-11) says. Declined, the result
says it was declined, and the host's turn goes on. A name that matches no bot, a bot that opted out
among them, is refused with the names that do match.

A bot whose model is refused on this machine, or needs a sign-in it has not made, is put on the card
as refused, naming the model it asked for. The card takes no yes, nothing is sent, and the result
says so.

The proposed brief is trusted by construction, so it never needs the promotion
[CONSULT-6](#CONSULT-6) describes. The driver reads the tool's arguments as the planner's own words
only from a trusted context and refuses them from any other. The brief carries that context's
label, so it may be private, and the consult is held to that label.

The consult runs inside the host's turn. While it runs, its words are drawn under the bot's name,
and its reply is drawn the same way when it ends. Its tokens and its time count toward the host
turn's. What a person decides inside it, such as a command they let run or a file they vouched for,
is kept for the host afterwards, as a delegate's is.

Where nobody can be asked, such as a run in bypassing mode or one with no person at it, the tool is
not offered.

**Why.** A bot that has noticed it is out of its depth should be able to say whom to ask, and the
person should not have to retype the brief it already wrote. Only sending the card is the grant.

`verified-by: bravebot_agent::tools::a_sent_card_hands_the_turn_the_consult_to_run`
`verified-by: bravebot_agent::tools::a_declined_card_sends_nothing_and_says_so`
`verified-by: bravebot_agent::tools::a_name_that_is_no_bot_is_refused_with_the_names_that_are`
`verified-by: bravebot_agent::tools::a_proposal_nobody_was_offered_is_answered_as_an_unknown_tool`
`verified-by: bravebot_agent::turn::setup::a_consult_is_proposed_only_where_a_person_can_send_it_and_never_from_a_consult`
`verified-by: bravebot_ui_bridge::wire::a_proposed_consult_crosses_with_its_bot_model_bound_and_brief`
`verified-by: bravebot_ui_bridge::refusal::a_proposed_consult_is_put_to_the_window_and_sent_only_on_its_own_yes`
`verified-by: bravebot_ui_bridge::dispatch::a_consult_answer_for_no_waiting_proposal_is_refused_as_an_answer_to_nothing`
`verified-by: bravebot_agent::permission_mode::bypassing_refuses_a_consult_and_every_other_mode_asks`
`verified-by: by-construction (the desktop renderer is not a crate this workspace compiles, so the card is pinned by ui/scripts/consult-card.test.mjs, which asserts that it names the bot, its definition, model and bound, draws the brief and its label as text, offers to send it or not, takes only a consult answer, and that the main process lets consult.reply through; ui/scripts/shown.test.mjs asserts that a yes waits on the card being on screen; ui/scripts/drive-consult.mjs drives the real app and bridge against a stub model and asserts nothing reaches the bot before the yes and the brief does after it)`
`verified-by: bravebot_agent::tools::a_proposed_bot_whose_model_needs_a_sign_in_is_refused_on_the_card`
`verified-by: bravebot_agent::tools::a_brief_from_an_untrusted_context_is_refused_before_any_card`
`verified-by: bravebot_agent::tools::a_brief_from_a_private_context_is_trusted_and_private`
`verified-by: bravebot_agent::turn::a_proposed_consult_is_reported_under_its_bot_and_counted_to_the_host`
`verified-by: bravebot_agent::shared::a_consult_neither_moves_the_turns_prompt_nor_replaces_its_total`
`verified-by: bravebot_core::policy::what_a_person_decided_inside_a_proposed_consult_is_kept`
`verified-by: bravebot_tui::state::a_proposed_consults_words_are_drawn_under_the_bot`
`verified-by: bravebot_tui::confirm::a_refused_consult_card_names_the_model_and_takes_no_yes`

## How long it lasts

<a id="CONSULT-13"></a>
### CONSULT-13: a consult lasts one reply

The consult ends when it replies. The next line goes to the host. Following up means tagging the bot
again, with a new brief. Nothing in the input box changes while a consult runs or after it.

**Why.** A bot that stayed in the conversation would need answers to what the next line means, who
holds the conversation so far, and what leaves it. A follow-up costs one word.

`verified-by: bravebot_tui::app::a_consult_leaves_nothing_running_after_its_reply`
`verified-by: bravebot_tui::app::a_consult_lasts_one_reply_and_leaves_the_box_alone`

## Which bots

<a id="CONSULT-14"></a>
### CONSULT-14: a definition with `consult: off` is not a bot, and nothing else about it changes

A definition whose `consult:` key is `off` is left out of the bots [CONSULT-2](#CONSULT-2) counts.
It is not offered in the `@` list, a `@word` naming it names a file as though no bot had that name,
and a proposal naming it is refused. It is addressed with `/agent` and spawned by a planner exactly
as before. Any other value, and no key, leaves it a bot.

A later definition of the same name decides the key as it decides the description: its value
replaces the earlier one, whichever way that goes.

**Why replace and not meet.** The fields a later definition can only narrow are the ones that decide
what a run may reach. This one decides who may start a run, and a consult is only ever started by a
person sending a card, so turning it back on reaches nothing the person did not choose.

**Why a key and not a list in settings.** A helper written for one planner to spawn is noise in the
list a person reads, and the person who knows that is whoever wrote the definition.

`verified-by: bravebot_agent::agents::a_bot_is_a_slug_named_definition_that_did_not_opt_out`
`verified-by: bravebot_agent::agents::a_later_definition_of_a_name_decides_whether_it_is_a_bot`
`verified-by: bravebot_agent::agents::a_definition_that_opted_out_is_still_a_definition`

<a id="CONSULT-16"></a>
### CONSULT-16: the desktop's form for a bot shows `consult:` as a switch and writes it

The form for editing a bot shows whether it may be consulted as a switch. Turning it off writes
`consult: off`, and turning it on removes the line. An edit that leaves the switch where it was
leaves the `consult:` line as the file has it. Every other line is kept as
[MEMORY-9](definition-memory.md#MEMORY-9) keeps it.

This amends [MEMORY-9](definition-memory.md#MEMORY-9), adding `consult:` to the lines an edit
rewrites.

`verified-by: bravebot_agent::agents::turning_consults_off_writes_the_line_and_turning_them_on_removes_it`
`verified-by: bravebot_ui_bridge::definitions::the_consult_switch_writes_and_removes_the_line`

## Known costs

- **A file shares its name with a bot only through `@./`.** A person who keeps a file named
  `reviewer` at the root and makes a bot of that name reaches the file differently from then on.
- **The prefix means two things.** That is what [ADDRESS-2](addressing-a-definition.md#ADDRESS-2)
  refused, and the cost it named is real: a person typing fast sees the list draw the difference
  only if they look at it.
- **A brief from a host that met untrusted content costs a check.** Every such brief waits on a second
  opinion before the card is drawn.
- **A consult from a read-only host cannot keep notes.** Its memory is a file, and the host may not
  write. A person who wants a bot to keep notes when it is consulted consults it from a session that
  may write.
