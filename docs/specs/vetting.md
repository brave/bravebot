---
id: CHECK
title: Vetting quarantined content
status: normative
governs:
  - crates/core/src/vetting.rs
  - crates/core/src/policy.rs
  - crates/agent/src/vet.rs
  - crates/tui/src/store.rs
  - crates/tui/src/status.rs
guards:
  - symbol: VettingSpec::new
  - symbol: Policy::before_vetting
  - symbol: Policy::before_vetting_a_path
  - symbol: Policy::compose_vetting_input
  - symbol: Policy::vetting_verdict
  - symbol: Policy::promote_vetted
---

## Scope

Reading quarantined content with a second model that holds nothing, so that the person deciding
whether the planner may have those bytes has a second opinion in front of them. What the check is
given, what it may say, which prompts run one, and what a person's answer to it does.

The tool the planner calls to ask for one slot is [tools/vet-content.md](tools/vet-content.md). The
prompts a check is drawn on are the ones [prompting.md](prompting.md) governs.

## Why it exists

Quarantined content has three ways out, and all three end at a person. What a program printed can
be read aloud, where a person reads the bytes and says the planner may have them
([tools/read-output.md](tools/read-output.md)). A file has the trust map, answered by naming the
path, by opening a directory, or at the startup question ([trust-map.md](trust-map.md)). One slot's
bytes can be promoted once, on the planner's request, and leave nothing behind
([tools/vet-content.md](tools/vet-content.md)). Everything else stays quarantined for the life of
the session.

Each of those is a person being asked to promote bytes they were not expecting and cannot audit at
speed, and the answer is the whole of the defence. A check is what puts a second opinion beside the
bytes before they answer, so the question is not the first time anything has looked at what the
content holds.

The single-use promotion is also there for the friction. Somebody who wants the agent to use a
fetched page otherwise has two moves: read every byte of it themselves, or vouch for a whole path,
which is very much larger. That third way covers one slot's bytes, once, and leaves nothing behind.

## Clauses

<a id="CHECK-1"></a>
### CHECK-1: a check holds less than a processor, and can write nothing at all

| | |
|---|---|
| Tools | none, and the request carries no tool list at all |
| Memory | none: the messages are built from nothing each time |
| Conversation | one request, one reply, no loop to steer |
| Reads | one piece of content, carried in the spec and fixed before the call |
| Writes | **nothing**: no slot, no reference, no destination of any kind |

A processor mints one slot or none. A check mints none, ever, and what it is fixed by holds no
field that could name a place for a result to go. The only things that come back are a word from
a fixed set and free text for a person to read.

The content is carried in the spec rather than named there, which is what "fixed before the call"
means: whatever the check will read is settled when the spec is built, and no store is left for a
second piece to be reached through. A check asked for by the planner takes it out of the slot it
names; a check before a vouch prompt takes it from the file, where there is no slot yet.

**Why.** The isolation is expressed as a type that cannot write rather than as a subprocess,
which is the same reasoning [processors.md](processors.md) gives for a processor: there is no
untrusted code here, so an operating-system boundary would confine the wrong thing. What is new
is that a check needs to be narrower still, because unlike a processor it produces something a
person is invited to act on.

What it is told about itself says so, and says the opposite of what a processor is told: a
processor that notices an injection attempt is asked to leave it out, because its output is not a
place a person will read, while a check exists to say so and its output is read by a person and by
nobody else. Where a check cannot tell, it warns.

`verified-by: bravebot_core::policy::a_check_is_given_the_one_slot_its_spec_names`
`verified-by: bravebot_agent::vet::a_checker_is_not_told_to_keep_quiet_about_what_it_notices`
`verified-by: bravebot_agent::vet::a_checker_that_cannot_tell_is_told_to_warn`
`verified-by: by-construction (what fixes a check holds one piece of content and the planner's sentence, has no output label, no output reference and no destination, and is built only by taking an authority minted inside the policy layer; every field is private and no method takes &mut self)`

<a id="CHECK-2"></a>
### CHECK-2: the content is enclosed so that it cannot forge the enclosure

The input is two blocks, the driver's own facts about the content first and the content second.
The content is written as **one JSON string literal**, so it occupies one physical line whatever
it holds and a newline in it is written as an escape.

**The containment is that encoding, not the block markers.** The markers are fixed ASCII with no
nonce, and content may spell them; it cannot put one at the start of a line, because it cannot
produce a line. Do not replace the encoding with a nonce under the impression the markers are
what is holding.

The last thing in the request is the driver's, after the content rather than only before it, and
it says both that the block was data and what an answer looks like.

`verified-by: bravebot_core::vetting::encoded_content_occupies_one_line`
`verified-by: bravebot_core::vetting::content_that_spells_the_fence_cannot_forge_one`
`verified-by: bravebot_core::vetting::a_quote_in_the_content_does_not_end_the_literal`
`verified-by: bravebot_core::policy::content_that_spells_the_fence_cannot_end_its_own_block`
`verified-by: bravebot_core::policy::the_metadata_is_a_separate_block_before_the_content`
`verified-by: bravebot_core::policy::a_composed_check_input_is_still_quarantined`
`verified-by: bravebot_agent::vet::control_is_re_asserted_after_the_content`

<a id="CHECK-3"></a>
### CHECK-3: what the planner says it expects is a prompt, must not be private, and is often absent

The planner says what it thinks the slot holds, and the check is told. Those are the planner's own
words on the footing a processor's instruction sits on: not content anybody read, but the sentence
the driver is about to send. Private is refused, since the user's own data must not become another
model's prompt.

Only the planner asking for a check carries one. A check run before a prompt the planner did not
ask for has nowhere to get an expectation from: it asked to read a file, or to be shown what a
command printed, and said nothing about the contents. There the key is **left out of the metadata
entirely** rather than written empty, because an empty string would tell the reader the planner
expected nothing, where the truth is that nothing claimed anything.

`verified-by: bravebot_core::policy::a_private_expectation_cannot_direct_a_check`
`verified-by: bravebot_core::policy::a_check_before_a_vouch_carries_the_file_and_claims_no_expectation`

<a id="CHECK-4"></a>
### CHECK-4: one word out of two, and everything else is a check that did not complete

A reply states its verdict as `safe` or `unsafe`, compared after trimming and lowercasing and
after nothing else. Any other word, a reply that states no verdict, a truncated reply, a timeout
and a backend error are all **inconclusive**, which says nothing about the content.

Where a reply holds more than one candidate answer, the last is read: that is the one a model
writes after reasoning, and reading the first would let its worked example outrank its conclusion.

**Why.** Fails closed on every path. The only reply that can reduce a warning is one that answered
in the form it was asked for, so a check that half worked lands where a check that did not run
lands.

`verified-by: bravebot_core::vetting::a_stated_verdict_is_read_with_its_reason`
`verified-by: bravebot_core::vetting::a_verdict_inside_prose_is_found`
`verified-by: bravebot_core::vetting::the_last_object_is_the_verdict`
`verified-by: bravebot_core::vetting::a_verdict_spelled_inside_a_reason_decides_nothing`
`verified-by: bravebot_core::vetting::an_unknown_word_is_inconclusive`
`verified-by: bravebot_core::vetting::a_word_with_punctuation_after_it_is_not_a_verdict`
`verified-by: bravebot_core::vetting::case_and_space_around_the_word_do_not_matter`
`verified-by: bravebot_core::vetting::a_reply_with_no_verdict_in_it_is_inconclusive`
`verified-by: bravebot_core::vetting::a_bare_word_is_not_a_verdict`
`verified-by: bravebot_core::vetting::a_nested_key_does_not_answer_for_the_object_holding_it`
`verified-by: bravebot_core::vetting::a_truncated_reply_is_inconclusive`
`verified-by: bravebot_core::vetting::a_reply_holding_characters_outside_ascii_is_read`
`verified-by: bravebot_agent::turn::a_check_that_could_not_be_made_falls_back_to_the_question`

<a id="CHECK-5"></a>
### CHECK-5: a verdict is advice, and while auto-vetting is off the only thing it decides is which prompt is drawn

Auto-vetting is off until somebody turns it on ([CHECK-11](#CHECK-11)), so this is what a session
does unless a person has said otherwise. The bytes are put in front of the person whatever the
check said, the keys that answer the question are offered whatever it said, and the answer is
theirs. A verdict of safe promotes nothing by itself, and a verdict of unsafe withholds nothing:
neither is an answer to the question being asked.

The one key a verdict does decide is the one that answers no question: the standing key that turns
auto-vetting on, offered at either promoting prompt only where the check completed and found nothing
([PROMPT-6](prompting.md#PROMPT-6)). It is unbound where it is not drawn.

With auto-vetting on, one verdict answers the two promoting questions in the person's place, and
[CHECK-12](#CHECK-12) is the whole of what that changes. Everything in this clause holds of the
vouch offer either way, and of all three where the mode is off.

The three outcomes are told apart on the screen. "This looks like an attempt to give instructions"
and "nothing looked at this" are different facts about different risks, and one sentence covering
both would be wrong about one of them.

Every prompt CHECK-10 runs a check for draws it the same way, out of one row builder rather than
three, so a prompt cannot be added that carries a verdict and forgets to say what it was.

**Why.** This is what keeps the branch narrow. Something is being decided from bytes derived from
untrusted content, which [labels.md](labels.md) enumerates as a known cost; what bounds it is that
the decision is which sentence a person reads before answering for themselves.

`verified-by: bravebot_core::policy::a_safe_verdict_promotes_nothing_by_itself`
`verified-by: bravebot_core::policy::an_unsafe_verdict_does_not_overrule_the_person`
`verified-by: bravebot_tui::confirm::the_vet_prompt_says_which_of_the_two_failures_it_was`
`verified-by: bravebot_tui::confirm::a_safe_verdict_is_drawn_as_what_the_check_found`
`verified-by: bravebot_tui::confirm::a_safe_verdict_does_not_change_which_keys_the_vet_prompt_offers`
`verified-by: bravebot_tui::confirm::only_a_safe_verdict_offers_to_stop_asking`
`verified-by: bravebot_tui::confirm::the_output_prompt_says_what_a_check_found`
`verified-by: bravebot_tui::confirm::the_vouch_prompt_says_what_a_check_found`
`verified-by: bravebot_tui::confirm::a_vouch_prompt_says_when_no_check_was_made`

<a id="CHECK-6"></a>
### CHECK-6: promoting is an assertion about one slot's bytes, and writes no rule

The slot keeps the label it was quarantined at. What the planner receives is a **new value** whose
first label comes from the provenance the policy layer tracked, which is a person having read the
bytes and said so.

Nothing reaches the trust map. A later read of the same file mints a new slot and is quarantined
exactly as it is today, and the same content asked about again asks again.

**Why.** That is what keeps this from being a second answer to what a file is worth.
[tools/read-output.md](tools/read-output.md) refuses a file outright on the grounds that a second
route to a path's worth would be a way to disagree with the trust map. This is not that route: the
trust map still says the path is untrusted, and reading it back still quarantines it.

`verified-by: bravebot_core::policy::vetted_content_a_person_vouched_for_comes_back_trusted`
`verified-by: bravebot_core::policy::promoting_a_vetted_slot_does_not_relabel_it`
`verified-by: bravebot_core::policy::vetting_a_slot_vouches_for_no_path`
`verified-by: bravebot_agent::turn::content_a_person_reads_after_a_check_reaches_the_planner`

<a id="CHECK-7"></a>
### CHECK-7: confidentiality is never lowered, so vetting unlocks no egress

What comes back is private. The bytes may have come out of the workspace, and nothing about a
check or about a person reading them makes them fit to leave the machine or to become a routing
field.

`verified-by: bravebot_core::policy::a_private_slot_promotes_to_private_and_never_to_public`

<a id="CHECK-8"></a>
### CHECK-8: an approval covers one slot once, and answers no other question

A single-use endorsement naming that exact slot is what authorises the promotion. It cannot be
replayed, and an approval given to a different question is not one of these: an approval to read
what a program printed promotes nothing.

Two things mint one and nothing else does: a person answering the prompt, and, where auto-vetting
is on, a safe verdict on either route [CHECK-12](#CHECK-12) names. Which of the two it was is
recorded, on both routes, because a trail that credited a person who was never shown the bytes would
be the one record a reader cannot check. Everything else about the endorsement is the same either
way: one slot, once, and no other question answered.

`verified-by: bravebot_core::policy::content_cannot_be_promoted_without_an_endorsement`
`verified-by: bravebot_core::policy::an_approval_to_vet_cannot_be_replayed`
`verified-by: bravebot_core::policy::the_trail_says_when_nobody_was_asked`
`verified-by: bravebot_core::policy::the_trail_says_which_of_the_two_released_the_output`
`verified-by: bravebot_core::policy::a_promotion_nobody_was_asked_about_is_no_wider`
`verified-by: bravebot_core::policy::an_approval_to_read_output_is_not_an_approval_to_vet`
`verified-by: bravebot_tui::remote_confirm::an_approved_output_read_does_not_approve_a_vetted_read`
`verified-by: bravebot_tui::remote_confirm::a_closed_channel_refuses_a_vetted_read`

<a id="CHECK-9"></a>
### CHECK-9: what the check says reaches a person, and no model

The verdict word and the sentence the check wrote go to the prompt. Neither reaches the planner,
whichever way the person answers, and neither reaches a second check.

The sentence is free text about content an attacker may own, so it is untrusted and private like
the content, and it is drawn inside the margin a prompt puts round anything nobody vouched for.
The audit trail records the word and never the sentence: a trail is read by people entitled to
assume the driver is talking.

`verified-by: bravebot_core::policy::what_a_check_says_is_as_untrusted_as_what_it_read`
`verified-by: bravebot_core::policy::the_trail_records_the_verdict_and_never_the_reason`
`verified-by: bravebot_agent::turn::content_a_person_refuses_after_a_check_stays_out_of_the_planner`
`verified-by: bravebot_agent::turn::a_vouch_offer_carries_what_a_check_said_about_the_whole_file`
`verified-by: bravebot_agent::turn::an_output_offer_carries_what_a_check_said`
`verified-by: bravebot_tui::confirm::what_the_check_said_is_drawn_inside_the_margin_too`

<a id="CHECK-10"></a>
### CHECK-10: every prompt that would promote quarantined content runs a check first

All three ways out of quarantine end at a person answering a prompt, and a check runs before the
prompt is drawn, not after it is answered:

| The prompt | What a yes does | What the check reads |
|---|---|---|
| [tools/vet-content.md](tools/vet-content.md) | promotes one slot's bytes once | the slot the planner named |
| [tools/read-output.md](tools/read-output.md) | releases what a program printed to the planner | the slot the planner named |
| the vouch offer in [tools/read-file.md](tools/read-file.md) | writes a rule about the path | **the whole file**, not the preview |

**Why the whole file.** A vouch prompt shows the head of the file and a yes grants all of it, so a
check over the preview would report on the part an injection attempt has the least reason to be in.
The check reads what the answer covers.

**Why all three.** Otherwise the quickest way past a check is to ask for the prompt that does not
run one. A person asked to promote content is asked on the strength of what they can see; two of
these prompts having a second opinion on them and the third not is a gap the planner chooses, and
the trust map's rule is the largest grant of the three.

The one exemption is [permission-modes.md](permission-modes.md)'s bypassing mode, which draws no
prompt at all. There a check would be a model call whose word nobody reads. A verdict is still
filled in and it is `inconclusive`, which claims nothing.

`verified-by: bravebot_agent::turn::a_vouch_offer_carries_what_a_check_said_about_the_whole_file`
`verified-by: bravebot_agent::turn::an_output_offer_carries_what_a_check_said`
`verified-by: bravebot_agent::turn::content_a_person_reads_after_a_check_reaches_the_planner`
`verified-by: bravebot_core::policy::a_check_before_a_vouch_carries_the_file_and_claims_no_expectation`

<a id="CHECK-11"></a>
### CHECK-11: auto-vetting is off until somebody turns it on, and there are three ways in

Nothing about a check answers a question until a person has said it may. Three routes say so, and
they differ in how long the answer lasts rather than in what it says:

| Route | Read from | Lasts |
|---|---|---|
| `--vet` | the command line ([cli.md](cli.md#CLI-15)) | this run |
| `~/.bravebot/vetting`, one word, written by the standing key at a vetting prompt | the person's own directory | until they change it |
| `"vetting": { "auto": true }` | `~/.bravebot/settings.json`, the **home layer only** | every session |

Any one of them is enough. A choice recorded in the person's own directory outranks the settings
key, which is the precedence the editing style already uses: somebody who turned the mode off has
made a decision that has to outlast the session, so off is written to that file rather than the
file being removed, and a settings file cannot turn it back on for them tomorrow. `--vet` outranks
both, a recorded `off` included, because it is the narrowest in time: somebody typing it has said
what they want of the run in front of them, and that is the footing
[permission-modes.md](permission-modes.md)'s own flag sits on, which is a strictly larger thing
anything able to pass `--vet` could pass instead. There is no flag the other way
([CLI-15](cli.md#CLI-15)).

The key that writes that file is drawn on the vetting prompt, at the moment the mode would have
saved the person a keystroke, which is where [trust-map.md](trust-map.md#TRUST-8) offers its own
larger grant. It is offered only where the check found nothing, and
[PROMPT-6](prompting.md#PROMPT-6) is why. Nothing writes the settings file: no shipping code here
edits a person's `settings.json`, and doing it would mean a JSON rewrite that preserves their
comments and key order.

The recorded choice is read only by a session that records one. A session asked to leave nothing
behind reads the model and the theme, because those decide what it looks like; this decides whether
somebody is asked, and a private session inheriting that answer is the one read worth refusing.

**A session that opened with the mode on says so, and goes on saying so.** It is said once at the
top of the transcript, before the first slot can reach it, and reported in `/status` for the rest
of the session with the file it is kept in named. A note scrolls away, and the one thing a person
cannot read off a transcript is a question that was never put: a standing answer that stops a
prompt appearing has to be readable at the moment they wonder why. That is the rule
[permission-modes.md](permission-modes.md)'s own modes follow, and a session that is asking says
nothing, since a line reporting the ordinary state is a line people learn to skim.

**The settings key is read from the home layer and no other.** A project `.bravebot/settings.json`
naming it, a machine-local one, and a file the command line named are each reported by `doctor` and
not obeyed. Every other name in those files configures where a request goes or how the interface
behaves; this one says whether a person is asked before content nobody vouched for reaches the
planner, so a line in a checkout could turn the asking off for whoever opened it. A word the file
does not know, and a value that is not a boolean, are no answer at all rather than a guess.

`verified-by: bravebot_core::vetting::nothing_is_auto_vetted_until_somebody_asks_for_it`
`verified-by: bravebot_core::vetting::each_of_the_three_routes_turns_it_on_by_itself`
`verified-by: bravebot_core::vetting::a_recorded_choice_outranks_the_settings_key`
`verified-by: bravebot_core::vetting::the_flag_outranks_a_recorded_choice`
`verified-by: bravebot_config::settings::the_home_layer_may_ask_for_auto_vetting`
`verified-by: bravebot_config::settings::a_project_layer_cannot_turn_auto_vetting_on`
`verified-by: bravebot_config::settings::the_local_layer_cannot_turn_auto_vetting_on_either`
`verified-by: bravebot_config::settings::a_named_layer_cannot_turn_auto_vetting_on`
`verified-by: bravebot_config::settings::a_project_layer_does_not_override_what_the_home_layer_said_about_vetting`
`verified-by: bravebot_config::settings::a_vetting_key_that_is_not_a_boolean_says_nothing`
`verified-by: bravebot_tui::store::a_recorded_answer_about_vetting_is_read_back_both_ways`
`verified-by: bravebot_tui::store::a_file_naming_no_answer_about_vetting_is_not_a_choice`
`verified-by: bravebot_tui::state::a_session_asks_until_something_says_otherwise`
`verified-by: bravebot_tui::state::the_flag_and_the_settings_key_each_reach_the_session`
`verified-by: bravebot_tui::state::pressing_the_standing_key_turns_vetting_on_for_the_session`
`verified-by: bravebot_tui::confirm::only_a_safe_verdict_offers_to_stop_asking`
`verified-by: bravebot_tui::status::a_session_that_stopped_asking_about_a_check_says_so`
`verified-by: bravebot_tui::status::an_ordinary_session_says_nothing_about_a_check`
`verified-by: bravebot_config::settings::a_layer_that_named_only_vetting_is_not_a_layer_that_said_nothing`
`verified-by: bravebot_cli::main::the_vet_flag_is_taken_out_wherever_it_appears`

<a id="CHECK-12"></a>
### CHECK-12: with it on, a safe verdict promotes one slot, and nothing else does

Where auto-vetting is on, a check that completed and found nothing answers in the person's place at
either prompt that promotes one slot's bytes: no prompt is drawn and the bytes reach the planner.
Every other verdict falls back to that prompt, carrying the banner it would have carried anyway, and
[CHECK-5](#CHECK-5) governs it from there. Unsafe and a check that did not complete are still told
apart on the screen, because the reason for asking is different in the two cases.

**It covers the promotions and not the rule.** The three prompts a check runs for divide two to one:

| The prompt | What a yes does | With the mode on |
|---|---|---|
| [tools/vet-content.md](tools/vet-content.md) | promotes one slot's bytes once | a safe verdict answers |
| [tools/read-output.md](tools/read-output.md) | promotes one slot's bytes once | a safe verdict answers |
| the vouch offer in [tools/read-file.md](tools/read-file.md) | writes a rule about the path | still asks |

**Why the line falls there.** It falls on the shape of the grant, not on which tool produced the
bytes. Both promotions cover one slot's bytes once and leave nothing behind ([CHECK-6](#CHECK-6)),
so what a verdict can buy is bounded by a single slot either way: the same question, about the same
kind of content, answered by the same check, ending in the same `(T,priv)` value and the same
single-use endorsement. A person offered the standing answer at one of them and not the other would
be reading which tool the planner happened to call, which is not a fact about the risk they are
being asked to take. A trust rule is different in kind, being a standing decision about a whole path
rather than about bytes in front of a reader, and it is the one grant the mode does not touch.
Widening the mode to that would be letting a check answer a bigger question than the one it read.

**What a person reads instead of being asked.** The route the mode covers most often in practice is
the output prompt: a run's output is quarantined by default, so it is the prompt a person meets when
they ask what a command printed. That is the reason the mode reaches it, and equally the reason the
mode is off until somebody turns it on.

What the mode never decides is anything but who answers. The slot is the planner's choice either
way, the label is `(T,priv)` either way ([CHECK-7](#CHECK-7)), the endorsement is single-use either
way, and no trust rule is written either way. [labels.md](labels.md) enumerates what an attacker who
owns the content gains from this, which is the reason it is off by default.

`verified-by: bravebot_agent::turn::with_auto_vetting_a_safe_verdict_reaches_the_planner_unasked`
`verified-by: bravebot_agent::turn::with_auto_vetting_an_unsafe_verdict_still_asks`
`verified-by: bravebot_agent::turn::with_auto_vetting_a_check_that_could_not_be_made_still_asks`
`verified-by: bravebot_agent::turn::with_auto_vetting_a_safe_verdict_releases_command_output_unasked`
`verified-by: bravebot_agent::turn::with_auto_vetting_an_unsafe_verdict_still_asks_about_command_output`
`verified-by: bravebot_agent::turn::with_auto_vetting_a_broken_check_still_asks_about_command_output`
`verified-by: bravebot_agent::turn::auto_vetting_does_not_answer_the_vouch_offer`
`verified-by: bravebot_core::policy::a_promotion_nobody_was_asked_about_is_no_wider`
`verified-by: bravebot_core::policy::output_released_by_a_safe_verdict_is_no_wider`
`verified-by: bravebot_core::policy::the_trail_says_which_of_the_two_released_the_output`

## Known costs

- **A quarantined slot's contents reach the backend, a second time.** A check is a model call, so
  asking about a page sends it where the page would only have gone if a processor had been asked
  about it. The destination is the one every other call already goes to, and the reader holds
  nothing, but the call happens whether or not the person then approves.

- **A read of a quarantined file now sends the file, before anybody has agreed to anything.**
  CHECK-10 puts a check in front of the vouch prompt, and the check is a model call over the whole
  file. So a planner that reads a path nobody vouched for causes those bytes to reach the backend
  in the confined conversation even where the person then says no, and the refusal keeps the bytes
  out of the planner rather than off the wire. That is a real widening of what a `read_file` on a
  quarantined path does, and it is the price of the person being told something before they answer.
  It is bounded by the offer being made once per path per turn, by never being made for a picture
  or a directory, and by the confined conversation holding no tools, no memory and no destination.

- **The check is a model, so it is wrong sometimes, in both directions.** A safe verdict on
  content that is an attack draws the quiet banner under bytes the person still reads; an unsafe
  verdict on an ordinary page draws the warning under bytes that are fine. Neither changes what
  approving does, and the second is the direction the prompt is built to fail in.

- **A plausible sentence may persuade somebody to skim.** The check's own words are attacker
  reachable and can lie: nothing holds a sentence against the content it describes, and nothing
  could. What keeps it from deciding anything is that the bytes are on the same screen, so a
  person who reads them sees what they are agreeing to whatever the sentence said. The residue is
  the same alarm fatigue as [issue #23](https://github.com/brave/bravebot/issues/23).

- **With auto-vetting on, the bytes are on no screen at all.** [CHECK-12](#CHECK-12) is a person
  saying in advance that a check finding nothing is enough, so on the routes it covers nobody reads
  the content and a model's word is the whole of what stood between the planner's context and a
  fetched page, or what a program printed. That is the mode working as asked rather than a flaw in
  it, and it is why the mode is off until somebody turns it on and why the settings key that turns it
  on is not readable from a checkout. What bounds it is everything the verdict does not decide: one
  slot, once, `(T,priv)`, no trust rule, and no other prompt. [labels.md](labels.md) writes out what
  an attacker who owns the content gains.
