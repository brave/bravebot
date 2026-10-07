---
id: PASTE
title: Pasting
status: normative
governs:
  - crates/tui/src/clipboard.rs
  - crates/tui/src/state.rs
  - crates/tui/src/app.rs
  - crates/tui/src/invisible.rs
  - crates/ui-bridge/src/attached.rs
guards:
  - symbol: Policy::admit_pasted_image
documented-by: docs/website/docs/using/context.md
---

## Scope

What Ctrl-V puts into a turn, whether it is text or a picture, and on what footing. Pasted text is
presentation and holds no labels: it is the user's own words either way, and the only question is
that the box stays readable. A pasted picture is a claim about provenance, and most of this spec is
about that claim. A paste leaves a marker in the box, and deleting the marker takes the paste off.

Three gestures put content into a turn on the user's own footing, and each has its own spec:
[naming-files.md](naming-files.md) for `@` in a prompt, [pasting.md](pasting.md) for Ctrl-V, and
[dropping.md](dropping.md) for a file dragged onto the window.

## Why a pasted picture is trusted

A pasted picture is trusted because the user pasted it, exactly as they typed the words beside it.
It arrives in their own message, on the footing of the prompt it came with, and the gate that
admits it records that provenance and asserts nothing else. Nothing inspects the pixels and nothing
could. Being a picture establishes nothing: the trust comes from the gesture and from nowhere else.

It is carried whole rather than quarantined because a processor takes text slots and returns text,
so there is no reference an image could be reduced to. That is a fact about the mechanism, and not
a second reason to trust it.

## Clauses

<a id="PASTE-1"></a>
### PASTE-1: a long paste folds to a marker, and the words are what get sent

More than a couple of lines folds to `[Pasted text #2 +40 lines]`, counting the lines a person
would count. The words around it are left alone, and a short paste lands whole. A paste into a
command line is never folded. A paste ending in a newline does not send.

The words are put back where the line leaves the box, so the request, the transcript and the
history all hold the paste itself. The marker goes no further than the box: a prompt coming back
for editing after a stop comes back behind it.

**Why.** A stack trace would otherwise push the reply being read off the screen. Nothing is hidden:
what is about to be sent is what the prompt says, and deleting the marker drops the words.

**Why the marker stops at the box.** It is a handle on text that only the session holding it can
put back. In the transcript it makes the conversation claim something the planner was never given.
In the history it comes back in a later session naming nothing, and the placeholder is sent in
place of everything the person pasted, with nothing on the screen to say so.

`verified-by: bravebot_tui::state::a_folded_paste_counts_the_lines_a_person_would_count`
`verified-by: bravebot_tui::state::a_folded_paste_leaves_the_words_around_it_alone`
`verified-by: bravebot_tui::state::a_folded_paste_is_put_back_where_the_line_leaves_the_box`
`verified-by: bravebot_tui::state::the_transcript_shows_the_words_a_folded_paste_stood_for`
`verified-by: bravebot_tui::state::a_folded_paste_is_remembered_as_the_words_it_stood_for`
`verified-by: bravebot_tui::state::a_paste_queued_behind_a_turn_is_remembered_as_its_words`
`verified-by: bravebot_tui::state::a_recalled_prompt_carries_the_words_that_were_pasted_into_it`
`verified-by: bravebot_tui::state::a_stopped_turn_puts_a_folded_paste_back_behind_its_marker`
`verified-by: bravebot_tui::state::a_short_paste_lands_in_the_box_whole`
`verified-by: bravebot_tui::state::a_paste_into_a_command_line_is_never_folded`
`verified-by: bravebot_tui::render::a_folded_paste_keeps_its_lines_off_the_screen`
`verified-by: bravebot_tui::app::a_paste_that_ends_in_a_newline_does_not_send_it`
`verified-by: bravebot_tui::app::a_pasted_prompt_is_sent_when_the_user_says_so`
`verified-by: bravebot_tui::app::a_long_paste_folds_while_a_turn_is_running`
`verified-by: bravebot_tui::app::a_folded_paste_in_a_btw_question_is_the_words_that_were_pasted`
`verified-by: bravebot_tui::app::a_folded_paste_in_a_loop_prompt_is_the_words_that_were_pasted`
`verified-by: bravebot_tui::app::a_folded_paste_in_a_manifest_task_is_the_words_that_were_pasted`


<a id="PASTE-2"></a>
### PASTE-2: only a picture a human pasted

Never bytes a tool read, never anything a processor produced, never an image a path in model
output named. Each of those is content, and routing it here would launder it.

**Why.** The justification cannot be checked from the bytes, so it lives at the call site. There
are two. One is the TUI's Ctrl-V. The other is the `images` list a front end sends with
`turn.send`, each entry a `media` type and the picture's bytes as standard base64. The bridge cannot
see the gesture, so a front end putting a picture there is saying that a person pasted it, as a
`dropped` path says a person dropped it ([DROP-10](dropping.md#DROP-10)). The bridge holds the list
to what a string can be held to: a type from [PASTE-3](#PASTE-3)'s set, data that decodes, and no
more than the 10 MB cap (`MAX_PASTED_IMAGE_BYTES` in the agent crate) that the terminal also reads.
Anything else, or a list of any other shape, refuses the send rather than starting a turn without
the picture.

A picture a tool read has one way into the planner's context, and it is not this one.
[VET-4](tools/vet-content.md#VET-4)'s `vet_content` lets one picture through where a person handed a
copy of it said yes, where auto-vetting took a check's safe verdict as the answer, or where a run
bypassing permissions answered for them. That route does not come through here and widens nothing
here. What it rests on is an endorsement of one slot, recorded as that, and the picture never joins
the person's own message. This gate still admits a paste and nothing else.

`verified-by: bravebot_tui::app::a_picture_off_the_clipboard_becomes_a_marker_in_the_line`
`verified-by: bravebot_agent::turn::a_picture_is_never_shown_to_the_planner`
`verified-by: bravebot_agent::turn::a_processor_is_given_a_picture_as_a_picture`
`verified-by: bravebot_agent::turn::a_picture_a_person_opens_and_lets_through_is_attached_after_the_results`
`verified-by: bravebot_ui_bridge::attaching::a_pasted_picture_reaches_the_model_with_the_prompt`
`verified-by: bravebot_ui_bridge::attaching::a_picture_the_bridge_cannot_carry_refuses_the_send`


<a id="PASTE-3"></a>
### PASTE-3: the media type is the driver's, never the content's

It ends up in the data URL, where it is routing, so it comes from a fixed set
the clipboard reader owns, and never from a filename or from what a tool printed.

The bridge's set is `PASTED_IMAGE_MEDIA` in the agent crate: `image/png`, `image/jpeg`,
`image/gif` and `image/webp`. A front end's `media` string selects an entry, and the entry is what
is sent; a string that matches none, `IMAGE/PNG` included, refuses the send.

`verified-by: by-construction (the type is a static string from the clipboard reader's literals to the data URL it is formatted into, so a media type read from a filename or from what a tool printed does not compile)`
`verified-by: bravebot_ui_bridge::attaching::a_picture_the_bridge_cannot_carry_refuses_the_send`


<a id="PASTE-4"></a>
### PASTE-4: the picture is inlined, never linked

A URL would have the endpoint fetch it over a connection this process never makes, which is an
egress `bravebot-net` could not gate.

`verified-by: bravebot_agent::turn::a_pasted_image_reaches_the_model_with_the_prompt`


<a id="PASTE-5"></a>
### PASTE-5: a paste does not lower context integrity

It says nothing about content the planner has met. Lowering it here would have a screenshot mark
everything the planner then wrote as untrusted, on the strength of the user's own input.

`verified-by: bravebot_core::policy::a_pasted_image_does_not_lower_what_the_context_has_met`


<a id="PASTE-6"></a>
### PASTE-6: what is sent is what the prompt says

The marker is written where the caret is and the picture goes wherever that text goes. Deleting
the marker unsends it. A prompt recalled from the history carries no pictures and names none: the
marker is not what gets remembered, because it stands for a screenshot only the session that
pasted it holds, and the words around it are what the person meant. A picture is refused in shell
mode rather than written into the command. A command whose argument reaches a model carries the
picture with it: `/btw` sends a question and `/manifest` plans from a task, and either one about a
screenshot, answered without the screenshot, is answered about nothing. Waiting changes nothing about
that, so one the queue reaches when a turn ends carries it too. `/loop` carries it on the first tick,
which is the turn the person pasted into, and every tick after that sends the same line with the
marker put back to words, because the picture went with the tick that took it and a loop sends the
same line however long it runs. A command the driver carries out itself has no turn for a picture to
travel in, so a marker in one is put back to words, whether the command is dispatched at rest or the
queue reaches it when the turn ends. Those words say a picture was pasted and cannot be shown, which
is what a picture on a line queued mid-turn becomes ([dropping.md](dropping.md#DROP-8)), and the
person is told it did not go. A picture over 10 MB is decoded, scaled so its longest side is at most
2048 pixels, re-encoded as PNG and sent in that form when the result is under 10 MB. Only when it will
not decode, or is still over, is it refused rather than sent, and it says so with the size it was
pasted at.

**The thumbnail.** Where the terminal draws pictures, a small drawing of each picture named in the
line sits under the box, so a person can see it is the screenshot they meant. It is drawn for
a pasted picture and for a dropped PNG or JPEG. It follows the marker: rubbing the marker out takes
the drawing with it, and sending or clearing the line does the same. A line that goes back into the
box, after a stopped turn or an unqueue, has its drawings made again from the pictures it names. It
is presentation only. The picture that is sent is the bytes that were pasted, whether or not a
drawing was made of them, and a picture that will not decode, or a terminal that answers no query,
leaves the box as it was.

`verified-by: bravebot_tui::app::a_picture_is_refused_in_shell_mode_rather_than_written_into_the_command`
`verified-by: bravebot_tui::app::a_picture_pasted_into_a_question_goes_with_it`
`verified-by: bravebot_tui::app::a_picture_pasted_into_a_task_goes_with_the_plan`
`verified-by: bravebot_tui::app::a_picture_pasted_into_a_loop_goes_with_its_first_tick`
`verified-by: bravebot_tui::app::a_picture_named_on_a_question_that_waited_still_goes_with_it`
`verified-by: bravebot_tui::app::a_picture_a_command_line_named_is_carried_out_as_words_rather_than_as_its_marker`
`verified-by: bravebot_tui::app::a_picture_named_on_a_command_that_waited_is_words_by_the_time_it_is_carried_out`
`verified-by: bravebot_tui::app::a_command_line_whose_marker_was_deleted_says_nothing_about_a_picture`
`verified-by: bravebot_tui::app::a_picture_too_large_to_send_says_so_with_its_size`
`verified-by: bravebot_tui::clipboard::a_picture_over_the_cap_is_scaled_down_and_sent`
`verified-by: bravebot_tui::clipboard::a_picture_over_the_cap_that_will_not_decode_is_refused_rather_than_swapped_for_the_text`
`verified-by: bravebot_tui::clipboard::a_picture_over_the_cap_declaring_too_wide_a_canvas_is_refused_unread`
`verified-by: bravebot_tui::clipboard::a_picture_over_the_cap_that_would_decode_past_the_allocation_limit_is_refused_unread`
`verified-by: bravebot_tui::clipboard::a_picture_still_over_the_cap_after_scaling_is_refused_with_its_pasted_size`
`verified-by: bravebot_tui::state::the_first_tick_of_a_loop_carries_the_picture_pasted_into_it`
`verified-by: bravebot_tui::state::a_later_tick_of_a_loop_says_the_picture_went_with_the_first`
`verified-by: bravebot_tui::loops::a_pasted_picture_goes_to_one_tick_and_the_settled_line_to_every_other`
`verified-by: bravebot_tui::state::a_recalled_prompt_does_not_name_a_picture_that_went_with_the_line`
`verified-by: bravebot_tui::state::settling_a_marker_for_the_history_does_not_take_the_picture_off_the_turn`
`verified-by: bravebot_tui::state::a_thumbnail_that_finishes_after_the_paste_is_picked_up_and_drawn_while_named`
`verified-by: bravebot_tui::state::only_a_dropped_picture_is_given_a_thumbnail`
`verified-by: bravebot_tui::state::a_cancelled_turn_restages_the_thumbnail_with_the_picture`
`verified-by: bravebot_tui::state::taking_the_queue_back_restages_the_thumbnails_of_what_it_named`
`verified-by: bravebot_tui::render::a_staged_picture_is_drawn_under_the_box_while_the_line_names_it`
`verified-by: bravebot_tui::render::rubbing_out_the_marker_takes_the_thumbnail_with_it`
`verified-by: bravebot_tui::sessions::cancelled_attachments_return_to_the_editor_and_the_next_request`
`verified-by: bravebot_agent::turn::a_picture_pasted_into_a_question_reaches_the_model_with_it`
`verified-by: bravebot_agent::manifest::a_picture_pasted_into_the_task_reaches_the_planner`


<a id="PASTE-7"></a>
### PASTE-7: reading the clipboard is presentation plumbing and holds no labels

Command-V is the terminal's chord and never reaches this process: the byte stream over a pty has
no encoding for that modifier, and the terminal writes the clipboard's *text* into the pty instead,
which is why a picture silently arrives as nothing. Ctrl-V comes through as a byte, so the TUI goes
around the terminal and reads the clipboard itself. An empty paste is read as the picture that was
meant. A picture wins over text when the clipboard holds both, since copying an image in a browser
leaves the page's URL behind as text and text has another key.

On macOS this reads the pasteboard through `osascript`. On Linux it needs `wl-paste` or `xclip`.

`verified-by: bravebot_tui::app::an_empty_paste_goes_and_reads_the_clipboard_instead`
`verified-by: bravebot_tui::app::an_empty_paste_mid_turn_goes_and_reads_the_clipboard_too`
`verified-by: bravebot_tui::app::ctrl_v_reads_the_clipboard_during_a_turn_too`
`verified-by: bravebot_tui::app::a_paste_that_carried_text_is_left_alone`
`verified-by: bravebot_tui::app::which_key_carries_a_picture_is_said_once_per_session`
`verified-by: bravebot_tui::clipboard::a_picture_wins_over_the_text_beside_it`
`verified-by: bravebot_tui::clipboard::the_text_beside_a_picture_is_never_read`
`verified-by: bravebot_tui::clipboard::a_picture_over_the_cap_that_will_not_decode_is_refused_rather_than_swapped_for_the_text`
`verified-by: bravebot_tui::clipboard::text_alone_is_the_paste`
`verified-by: bravebot_tui::clipboard::an_empty_clipboard_reads_as_nothing_rather_than_as_empty_text`
`verified-by: bravebot_tui::clipboard::a_missing_tool_reads_as_nothing_on_the_clipboard`
`verified-by: bravebot_tui::clipboard::a_missing_tool_is_not_taken_for_a_successful_copy`


<a id="PASTE-8"></a>
### PASTE-8: every paste is named in the audit trail

With its type and size, so `--trace` and Ctrl-T account for the pictures as well as the words. Each
of the three requests a picture can travel in takes the record where it holds the policy: a turn's
prompt, a question asked beside the work, and the task a plan is made from.

`verified-by: bravebot_core::policy::a_pasted_image_is_recorded_in_the_audit_trail`
`verified-by: bravebot_agent::turn::a_pasted_image_is_named_in_the_audit_trail`
`verified-by: bravebot_agent::turn::a_picture_pasted_into_a_question_is_named_in_the_audit_trail`
`verified-by: bravebot_agent::manifest::a_picture_pasted_into_the_task_is_named_in_the_audit_trail`

<a id="PASTE-9"></a>
### PASTE-9: a pasted picture is kept with the session and comes back on resume

The picture is part of the user's own message, so it is written into the session record alongside
the words and a resumed turn still has it. The redrawn transcript shows what the interface recorded
about the attachment rather than the bytes, because a data URI in the scrollback is not a
transcript.

**Why.** Resuming a session that turned on a screenshot, without the screenshot, would leave the
planner answering about something it can no longer see. Quarantined content is not written down at
all, and this is not an exception to that: a pasted picture was never quarantined, it is the user's
own input.

`verified-by: bravebot_tui::sessions::a_pasted_picture_is_kept_with_the_session_and_comes_back_on_a_resume`
`verified-by: bravebot_tui::state::a_resumed_prompt_that_carried_a_picture_shows_its_words_and_not_the_bytes`
`verified-by: bravebot_ui_bridge::attaching::a_pasted_picture_is_still_in_the_conversation_after_the_session_is_reopened`

<a id="PASTE-10"></a>
### PASTE-10: characters a terminal draws as nothing are removed from a paste, and counted

Pasted text loses the tag block, bidirectional controls, zero-width spaces and word joiners, the
soft hyphen and the supplementary variation selectors, wherever they stand. The zero-width joiner
and non-joiner stay where a script or an emoji needs them, after a non-ASCII character that is not
whitespace, and one emoji or text selector stays after an emoji or before a keycap mark. The flags of
England, Scotland and Wales, which are tag characters behind a black flag, stay, and no other tags do. The notice
`removed 3 invisible characters from that paste` follows the paste and gives the count. A paste
that held none says nothing. It applies to a paste in shell mode, to one that folds, and to one
that arrives while a turn runs, since all of them come through the same function.

**Why.** A paste lands in the person's own message with no label, so the words in it are taken as
theirs. Text copied from a page can hold an instruction written in characters nothing draws, and
the person would send it without having read it. Removing what cannot be seen makes the message
what the box shows, and the count lets them compare it to what they copied. This is a character
map. It reads no meaning from the text, decides nothing about any effect, and its count goes only to
the person's screen.

**Why the joiners and selectors are not all kept.** Zero-width bits between two letters of ASCII,
and a run of selectors behind one symbol, are how a message is hidden in text that looks plain.
Persian, Indic and emoji text needs only the first of each, after a character that is not ASCII.

`verified-by: bravebot_tui::invisible::a_message_written_in_tag_characters_is_removed_and_counted`
`verified-by: bravebot_tui::invisible::bidirectional_controls_and_zero_width_characters_are_removed`
`verified-by: bravebot_tui::invisible::a_paste_with_nothing_hidden_is_returned_as_it_came`
`verified-by: bravebot_tui::invisible::the_joiners_persian_and_indic_scripts_are_written_with_are_kept`
`verified-by: bravebot_tui::invisible::joiners_between_ascii_letters_are_removed`
`verified-by: bravebot_tui::invisible::a_run_of_joiners_after_a_letter_keeps_only_the_first`
`verified-by: bravebot_tui::invisible::emoji_sequences_are_kept_whole`
`verified-by: bravebot_tui::invisible::the_flags_of_england_scotland_and_wales_are_kept_whole`
`verified-by: bravebot_tui::invisible::tags_behind_a_black_flag_that_are_not_one_of_the_three_flags_are_removed`
`verified-by: bravebot_tui::invisible::a_run_of_selectors_after_an_emoji_keeps_only_the_first`
`verified-by: bravebot_tui::invisible::a_selector_that_follows_nothing_it_modifies_is_removed`
`verified-by: bravebot_tui::invisible::a_selector_after_a_digit_is_kept_only_for_a_keycap`
`verified-by: bravebot_tui::state::a_paste_loses_the_characters_a_terminal_draws_as_nothing_and_says_how_many`
`verified-by: bravebot_tui::state::one_removed_character_is_said_in_the_singular`
`verified-by: bravebot_tui::state::a_paste_with_nothing_hidden_arrives_whole_and_says_nothing`
`verified-by: bravebot_tui::state::a_paste_of_nothing_but_hidden_characters_writes_nothing_and_says_so`
`verified-by: bravebot_tui::state::a_folded_paste_is_put_back_without_what_a_terminal_draws_as_nothing`
`verified-by: bravebot_tui::state::a_paste_into_a_command_line_loses_what_a_terminal_draws_as_nothing`
`verified-by: bravebot_tui::app::a_paste_loses_what_a_terminal_draws_as_nothing_at_rest_and_while_a_turn_runs`
`verified-by: bravebot_tui::app::text_read_off_the_clipboard_loses_what_a_terminal_draws_as_nothing`
`verified-by: bravebot_tui::app::a_pasted_prompt_waits_in_the_box_without_what_a_terminal_draws_as_nothing`

## Known costs

- **A pasted picture lands on disk.** It is written into the session record so a resume can restore
  it (PASTE-9), which means a screenshot pasted into a session outlives the session. Deleting the
  session removes it.

- **A screenshot of a hostile page puts a stranger's words into the planner's context as though
  the user had typed them.** Nothing inspects the pixels and nothing could. What justifies it is
  that the user chose what to copy, can see on their own screen what they pasted, and is the party
  this serves. It is the cost shell mode carries, reached by another route.

- **Only a paste is cleaned.** Text typed at the keyboard, a path a dropped file names
  ([dropping.md](dropping.md)), and a prompt given with `-p` or in the desktop application's
  message box are not run through the filter, so a character nobody can see can still reach a
  message by those routes.

- **A listed character is removed even when it was meant.** A soft hyphen or a word joiner in text
  a person copied from a typeset page goes too. The notice says how many, and the words read the
  same.

- **A hidden message can use a character this list does not name.** The list is the characters
  that draw as nothing in a terminal; a visible lookalike is not one.

- **Text that is arranged by a removed control reads in another order.** Left-to-right and
  right-to-left marks, embeddings and isolates are removed, so Hebrew or Arabic text mixed with Latin
  names or digits can be drawn in a different order than it was copied. The characters are the same
  and are sent in the order they were pasted, and the notice says controls were removed.

- **A variation selector on an ideograph or a math symbol is removed.** A Japanese name written
  with an ideographic variation sequence is pasted in its default glyph. A selector behind an
  ideograph is also how a byte is hidden behind it, which is why it is not kept.
