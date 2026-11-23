---
id: PASTE
title: Pasting
status: normative
governs:
  - crates/tui/src/clipboard.rs
  - crates/agent/src/picture.rs
  - crates/tui/src/state.rs
  - crates/tui/src/state/attachments.rs
  - crates/tui/src/state/queue.rs
  - crates/tui/src/state/editing.rs
  - crates/tui/src/app.rs
  - crates/tui/src/app/turns.rs
  - crates/tui/src/app/side_requests.rs
  - crates/tui/src/app/keys.rs
  - crates/tui/src/invisible.rs
  - crates/core/src/invisible.rs
  - crates/tui/src/peek_mode.rs
  - crates/tui/src/state/peek.rs
  - crates/ui-bridge/src/attached.rs
  - ui/src/main/pastes.ts
  - ui/src/preload/index.ts
  - ui/src/renderer/staging.ts
  - ui/src/shared/pastes.ts
  - ui/src/renderer/components/PastedTextPreview.tsx
  - ui/scripts/pastes.test.mjs
  - ui/scripts/drive-paste.mjs
guards:
  - symbol: Policy::admit_pasted_image
documented-by: docs/website/docs/using/context.md
---

## Scope

What Ctrl-V puts into a turn, whether it is text, a picture or files a file manager copied, and on
what footing. Pasted text is
presentation and holds no labels: it is the user's own words either way, and the only question is
that the box stays readable. A pasted picture is a claim about provenance, and most of this spec is
about that claim. A paste leaves a marker in the box, and deleting the marker takes the paste off. A
folded paste can be opened in the info panel and read whole, without sending it
([PASTE-13](#PASTE-13)). The desktop window folds a long paste too, and opens it in a modal
([PASTE-14](#PASTE-14)).

Three gestures put content into a turn on the user's own footing, and each has its own spec:
[naming-files.md](naming-files.md) for `@` in a prompt, [pasting.md](pasting.md) for Ctrl-V, and
[dropping.md](dropping.md) for a file dragged onto the window.

## Why a pasted picture is trusted

A pasted picture is trusted because the user pasted it, exactly as they typed the words beside it.
It arrives in their own message, on the footing of the prompt it came with, and the gate that
admits it records that provenance and asserts nothing else. Nothing inspects the pixels and nothing
could. Being a picture establishes nothing: the trust comes from the gesture and from nowhere else.

It is carried whole rather than quarantined because it arrives in the user's own message, which
is not quarantined. A picture that is quarantined is held in a slot, and a processor is given it
there as a picture ([PROC-15](processors.md#PROC-15)). That is a fact about the mechanism, and not
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

`verified-by: bravebot_tui::app::tests::pasting::a_picture_off_the_clipboard_becomes_a_marker_in_the_line`
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
is sent; a string that matches none, `IMAGE/PNG` included, refuses the send. Every type the
terminal's clipboard reader names ([PASTE-7](#PASTE-7)) is an entry.

`verified-by: by-construction (the type is a static string from the clipboard reader's literals to the data URL it is formatted into, so a media type read from a filename or from what a tool printed does not compile)`
`verified-by: bravebot_ui_bridge::attaching::a_picture_the_bridge_cannot_carry_refuses_the_send`
`verified-by: bravebot_tui::clipboard::every_flavour_is_sent_as_a_media_type_the_turn_carries`


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
2048 pixels, re-encoded as PNG and sent in that form when the result is under 10 MB. A dropped
picture is scaled by the same function, `bravebot_agent::picture::downscaled`
([DROP-4](dropping.md#DROP-4)). The two limits are separate constants and are not the same number:
a paste is held to `MAX_PASTED_IMAGE_BYTES`, 10 MB, both as it arrives and once scaled, and a drop
to `MAX_ATTACHMENT_BYTES`, 8 MiB, for the scaled result, with a PNG or JPEG read for scaling only up
to 64 MiB. Nothing in the code or the specs gives a reason for the difference. Each constant states
only what it bounds: the paste limit what one request costs to encode, and the attachment limit the
cost of a picture that every later round re-sends. Only when it will
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

`verified-by: bravebot_tui::app::tests::pasting::a_picture_is_refused_in_shell_mode_rather_than_written_into_the_command`
`verified-by: bravebot_tui::app::tests::pasting::a_picture_pasted_into_a_question_goes_with_it`
`verified-by: bravebot_tui::app::tests::pasting::a_picture_pasted_into_a_task_goes_with_the_plan`
`verified-by: bravebot_tui::app::tests::pasting::a_picture_pasted_into_a_loop_goes_with_its_first_tick`
`verified-by: bravebot_tui::app::tests::pasting::a_picture_named_on_a_question_that_waited_still_goes_with_it`
`verified-by: bravebot_tui::app::tests::pasting::a_picture_a_command_line_named_is_carried_out_as_words_rather_than_as_its_marker`
`verified-by: bravebot_tui::app::tests::pasting::a_picture_named_on_a_command_that_waited_is_words_by_the_time_it_is_carried_out`
`verified-by: bravebot_tui::app::tests::pasting::a_command_line_whose_marker_was_deleted_says_nothing_about_a_picture`
`verified-by: bravebot_tui::app::tests::pasting::a_picture_too_large_to_send_says_so_with_its_size`
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
meant, except while the scroller ([SCROLL-2](scroller.md#SCROLL-2)), the delegate view
([WATCH-9](watching.md#WATCH-9)), a picker ([CMD-8](commands.md#CMD-8)) or the prompt search
([INPUT-19](terminal-input.md#INPUT-19)) is open, where every paste is ignored and none reads the
clipboard. A picture wins over text when the clipboard holds both, since copying an image in a browser
leaves the page's URL behind as text and text has another key. Files the clipboard names win over
both ([PASTE-12](#PASTE-12)).

A picture is read in the first of the flavours the clipboard holds, tried in this order, and the
flavour decides the media type, which is one of the reader's own literals ([PASTE-3](#PASTE-3)):
PNG as `image/png`, then JPEG as `image/jpeg`, then on macOS TIFF. The system offers TIFF for a
picture it holds in any form, so it is how a GIF or a HEIC is reached, and nothing here decodes it:
AppKit writes it as PNG and it is sent as `image/png`. AppKit draws it only within the decode limits
that [PASTE-6](#PASTE-6) holds a picture over the cap to, read from the header before anything is
decoded, and a TIFF declaring more reads as no picture. A flavour the clipboard does not hold, or one
that reads as empty, is the next one's turn, and the flavours after the first that answers are
never read. Windows asks the system for whatever picture the clipboard holds and gets PNG. A JPEG
over the cap is scaled down and sent as PNG, as a PNG is ([PASTE-6](#PASTE-6)), and the hint that
Ctrl-V would paste a picture appears for any of the flavours.

On macOS this reads the pasteboard through `osascript`. On Linux it needs `wl-paste` or `xclip`.

`verified-by: bravebot_tui::app::tests::pasting::an_empty_paste_goes_and_reads_the_clipboard_instead`
`verified-by: bravebot_tui::app::tests::pasting::an_empty_paste_mid_turn_goes_and_reads_the_clipboard_too`
`verified-by: bravebot_tui::app::tests::pasting::ctrl_v_reads_the_clipboard_during_a_turn_too`
`verified-by: bravebot_tui::app::tests::pasting::a_paste_that_carried_text_is_left_alone`
`verified-by: bravebot_tui::app::tests::pasting::which_key_carries_a_picture_is_said_once_per_session`
`verified-by: bravebot_tui::clipboard::a_picture_wins_over_the_text_beside_it`
`verified-by: bravebot_tui::clipboard::the_text_beside_a_picture_is_never_read`
`verified-by: bravebot_tui::clipboard::a_png_is_read_before_a_jpeg_beside_it`
`verified-by: bravebot_tui::clipboard::a_clipboard_holding_only_a_jpeg_yields_that_picture`
`verified-by: bravebot_tui::clipboard::a_flavour_that_reads_as_empty_is_not_a_picture`
`verified-by: bravebot_tui::clipboard::a_clipboard_holding_no_picture_flavour_yields_none`
`verified-by: bravebot_tui::clipboard::a_tiff_is_sent_as_the_png_the_platform_wrote_it_as`
`verified-by: bravebot_tui::clipboard::a_jpeg_is_decoded_and_scaled_down_to_a_png_as_a_png_over_the_cap_is`
`verified-by: bravebot_tui::clipboard::a_pasteboard_holding_a_png_is_read_as_that_png`
`verified-by: bravebot_tui::clipboard::a_pasteboard_holding_only_a_jpeg_is_read_as_that_jpeg`
`verified-by: bravebot_tui::clipboard::a_pasteboard_holding_only_a_tiff_is_read_as_a_png_of_the_same_picture`
`verified-by: bravebot_tui::clipboard::a_pasteboard_holding_only_text_reads_as_no_picture`
`verified-by: bravebot_tui::clipboard::a_pasteboard_holding_a_tiff_declaring_a_canvas_past_the_limit_is_not_drawn`
`verified-by: bravebot_tui::clipboard::a_pasteboard_offering_any_picture_flavour_is_said_to_hold_an_image`
`verified-by: bravebot_tui::clipboard::a_jpeg_only_clipboard_yields_a_picture_through_wl_paste`
`verified-by: bravebot_tui::clipboard::a_jpeg_only_clipboard_yields_a_picture_through_xclip`
`verified-by: bravebot_tui::clipboard::a_png_is_asked_for_before_a_jpeg_through_the_tools`
`verified-by: bravebot_tui::clipboard::a_clipboard_offering_a_jpeg_is_said_to_hold_an_image`
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
that arrives while a turn runs, since all of them come through the same function. A text paste into
the desktop window's composer goes through that function too, in the bridge, and the window shows
the same note ([PASTE-11](#PASTE-11)).

**Why.** A paste lands in the person's own message with no label, so the words in it are taken as
theirs. Text copied from a page can hold an instruction written in characters nothing draws, and
the person would send it without having read it. Removing what cannot be seen makes the message
what the box shows, and the count lets them compare it to what they copied. This is a character
map. It reads no meaning from the text, decides nothing about any effect, and its count goes only to
the person's screen.

**Why the joiners and selectors are not all kept.** Zero-width bits between two letters of ASCII,
and a run of selectors behind one symbol, are how a message is hidden in text that looks plain.
Persian, Indic and emoji text needs only the first of each, after a character that is not ASCII.

`verified-by: bravebot_core::invisible::a_message_written_in_tag_characters_is_removed_and_counted`
`verified-by: bravebot_core::invisible::bidirectional_controls_and_zero_width_characters_are_removed`
`verified-by: bravebot_core::invisible::a_paste_with_nothing_hidden_is_returned_as_it_came`
`verified-by: bravebot_core::invisible::the_joiners_persian_and_indic_scripts_are_written_with_are_kept`
`verified-by: bravebot_core::invisible::joiners_between_ascii_letters_are_removed`
`verified-by: bravebot_core::invisible::a_run_of_joiners_after_a_letter_keeps_only_the_first`
`verified-by: bravebot_core::invisible::emoji_sequences_are_kept_whole`
`verified-by: bravebot_core::invisible::the_flags_of_england_scotland_and_wales_are_kept_whole`
`verified-by: bravebot_core::invisible::tags_behind_a_black_flag_that_are_not_one_of_the_three_flags_are_removed`
`verified-by: bravebot_core::invisible::a_run_of_selectors_after_an_emoji_keeps_only_the_first`
`verified-by: bravebot_core::invisible::a_selector_that_follows_nothing_it_modifies_is_removed`
`verified-by: bravebot_core::invisible::a_selector_after_a_digit_is_kept_only_for_a_keycap`
`verified-by: bravebot_tui::state::a_paste_loses_the_characters_a_terminal_draws_as_nothing_and_says_how_many`
`verified-by: bravebot_tui::state::one_removed_character_is_said_in_the_singular`
`verified-by: bravebot_tui::state::a_paste_with_nothing_hidden_arrives_whole_and_says_nothing`
`verified-by: bravebot_tui::state::a_paste_of_nothing_but_hidden_characters_writes_nothing_and_says_so`
`verified-by: bravebot_tui::state::a_folded_paste_is_put_back_without_what_a_terminal_draws_as_nothing`
`verified-by: bravebot_tui::state::a_paste_into_a_command_line_loses_what_a_terminal_draws_as_nothing`
`verified-by: bravebot_tui::app::a_paste_loses_what_a_terminal_draws_as_nothing`
`verified-by: bravebot_tui::app::text_read_off_the_clipboard_loses_what_a_terminal_draws_as_nothing`
`verified-by: bravebot_tui::app::a_pasted_prompt_waits_in_the_box_without_what_a_terminal_draws_as_nothing`
`verified-by: bravebot_ui_bridge::attaching::a_window_asks_the_bridge_to_clean_a_paste_and_is_told_the_terminals_note`
`verified-by: bravebot_ui_bridge::english_notes::a_note_for_the_window_is_in_english_whatever_the_machine_says`
`verified-by: by-construction (the preload and the renderer are not crates this workspace compiles, so ui/scripts/pastes.test.mjs pins pasteText, which writes the cleaned text over the composer's selection and writes nothing for empty text, and cleanPaste, which passes on the text, count and note a stub pastes.clean gives and has nothing pasted when the bridge does not answer or answers something else; make check-ui runs the tests)`

<a id="PASTE-11"></a>
### PASTE-11: the desktop window takes a picture only from a trusted paste, off the clipboard itself

The window keeps [PASTE-2](#PASTE-2) by never letting the page supply the bytes. The preload, in
its isolated world, listens for the paste itself and looks only at an event the browser marks
trusted, inside the composer. When the clipboard holds a picture, the picture wins over any text
beside it, as in [PASTE-7](#PASTE-7): the browser's own paste is cancelled and the main process is
asked to stage what is on the clipboard. A paste of text alone is cancelled as well, and the text
on the trusted event goes to the main process, which asks the bridge's `pastes.clean` to remove the
characters [PASTE-10](#PASTE-10) removes. The composer writes the cleaned text over its selection and
shows the terminal's note when any were removed. A bridge that does not answer has nothing pasted,
and the window says the paste could not be checked. A paste with neither a picture nor text is left
to the browser. The main
process reads the operating system's clipboard itself, decodes the picture and writes it out again
as PNG, so the type is always `image/png`, never something the content claims
([PASTE-3](#PASTE-3)). The main process asks the bridge's `pastes.check` about the picture's
size, as it came off the clipboard and again once re-encoded. The bridge holds it to the 10 MB cap
`turn.send` and the terminal's clipboard reader hold a paste to, and answers with that type and the
noun the terminal writes, or with the terminal's note for a picture too large, built from the same
catalog message in `en-US` because the window is in English only. The window holds no cap, type or
wording of its own. A picture over the cap is refused with that note, which gives its size and the
limit, and nothing is staged. An empty
clipboard stages nothing, and so does a bridge that does not answer. The bytes stay in the main
process against an opaque grant bound to the session, and the page is told the grant and a small
drawing of the picture. A page that dispatches its own paste event, whatever it puts in it, has
nothing staged, and the page has no call that reaches the clipboard read.

At send the page names grants, never bytes. The main process takes away every `images` list the
page sent and composes `images` from the grants the draft still names, in the order they were
staged. A grant this session does not hold refuses the send rather than starting a turn without
the picture. Grants end with the session.

The rest follows the terminal where a window can. The marker is `[Image #n]`, numbered from the one
counter a conversation's drops use, so a drop and then a paste in one draft are `#1` and `#2`. A
chip with the drawing sits above the box; deleting the marker takes the picture off and removing
the chip deletes the marker ([PASTE-6](#PASTE-6)). The transcript keeps the marker in the prompt,
and a reopened session shows those words and draws no bytes ([PASTE-9](#PASTE-9)).

One part does not follow. A message queued while a turn runs keeps its pictures and sends them as
its own turn when the running one ends, as a dropped file does
([DROP-12](dropping.md#DROP-12)), rather than becoming words; the window's queue never joins a
running turn.

`verified-by: bravebot_ui_bridge::attaching::a_window_asks_the_bridge_whether_a_paste_fits_and_is_told_the_terminals_note`
`verified-by: bravebot_ui_bridge::english_notes::a_note_for_the_window_is_in_english_whatever_the_machine_says`
`verified-by: by-construction (the preload and the renderer are not crates this workspace compiles, so ui/scripts/pastes.test.mjs pins the shared counter, the markers, and the main process's grants, its re-encode, its stripping of a window's images, and its use of the answer, type, noun and note a stub bridge gives with values the agent does not use, and fails if any file under ui/src says a size in megabytes; ui/scripts/drive-paste.mjs pastes from the real clipboard into the real app and bridge against a stub model, asserts an oversized picture is refused with the bridge's note, and asserts a page-dispatched paste stages nothing; make check-ui runs the tests)`

<a id="PASTE-12"></a>
### PASTE-12: a file the clipboard names is treated as a drop

When Ctrl-V finds files on the clipboard, as copying a file in Finder, Explorer or a Linux file
manager leaves, the paths are put to the terminal's drop handling as the line a paste of them would
have been ([DROP-7](dropping.md#DROP-7)), and that is the only gate they pass. Each file is then
read under a drop's rules ([DROP-4](dropping.md#DROP-4)): a picture or a PDF is carried as bytes, a
text file becomes context, and any other type has its path written into the line. Each leaves a
drop's rule in the trust map ([DROP-2](dropping.md#DROP-2)) and may be from outside the workspace
([DROP-3](dropping.md#DROP-3)). In shell mode the paths are written as words and nothing is staged
([DROP-6](dropping.md#DROP-6)), and the markers, the numbering and the order are a drop's. The
clipboard's own bytes for a file are never read, so nothing goes through the gate
[PASTE-2](#PASTE-2) states, which still admits a picture a person pasted and nothing else.

The files win over a picture and over the text beside them, since copying a file leaves its name
behind as text as copying an image in a browser leaves its page. A path is taken when it is
absolute and names a regular file. One that is not, a directory ([DROP-5](dropping.md#DROP-5)) or a
file that is gone, is left out, and a clipboard left with none is read as the rest of it would be.
A name that is not text, or that holds a line break, refuses the whole copy with a note that none
of the files were pasted, and the text beside it is not pasted in its place. A name that is not
UTF-8 has no spelling as text, and what it arrives as, a name holding U+FFFD, is the name of
another file or of none, so a name holding U+FFFD is refused as the desktop window refuses it
([DROP-12](dropping.md#DROP-12)), whether or not a file has that name. A newline or a carriage
return would end the line a drop is read from and begin another, so that a file named
`a<newline>/etc/hosts` would be taken for two. Refusing the whole copy is for the reason
[DROP-6](dropping.md#DROP-6) refuses the whole drop: the files taken would not be the files
copied. Any other control character is the drop handling's to judge, so a copied name holding one
refuses a shell line as a dragged one does. A file gone by the time the line is read says there is
nothing on the clipboard to paste. It applies while a turn runs, as every Ctrl-V does
([PASTE-7](#PASTE-7)).

The files are read from the macOS pasteboard's file URLs through `osascript`, on Linux from the
`text/uri-list` or the GNOME file managers' `x-special/gnome-copied-files` that `wl-paste` or
`xclip` reads, and on Windows from the clipboard's file drop list through PowerShell.

**Why.** What a drop's rules rest on is where its path came from: a person's gesture put it there,
and nothing a model said or a file held could. A path on the clipboard has the same origin, a
person's copy of a file in their own file manager followed by the key that pastes it, and the file
is read when the turn is built, as a dropped one is, so that justification is the drop's and
needs no second gate. A picture pasted from its bytes is a different claim, about content that
arrived on the clipboard, and keeping the two apart is what keeps PASTE-2 narrow.

`verified-by: bravebot_tui::clipboard::files_win_over_the_picture_and_the_text_beside_them`
`verified-by: bravebot_tui::clipboard::a_clipboard_naming_no_files_reads_what_else_it_holds`
`verified-by: bravebot_tui::clipboard::a_relative_path_on_the_clipboard_is_not_a_file_that_was_copied`
`verified-by: bravebot_tui::clipboard::a_name_holding_a_line_break_refuses_the_whole_list_rather_than_being_read_as_two_files`
`verified-by: bravebot_tui::clipboard::a_name_that_arrived_as_the_replacement_character_refuses_the_whole_list`
`verified-by: bravebot_tui::clipboard::a_name_holding_a_tab_or_an_escape_is_left_for_the_drop_handling_to_judge`
`verified-by: bravebot_tui::clipboard::a_refused_list_is_not_swapped_for_the_text_beside_it`
`verified-by: bravebot_tui::clipboard::a_file_whose_name_is_not_text_refuses_the_list_it_is_copied_in`
`verified-by: bravebot_tui::clipboard::the_reads_of_the_clipboard_start_no_tool_found_only_in_a_directory_a_stage_may_write`
`verified-by: bravebot_tui::clipboard::a_directory_and_a_missing_path_are_not_files_that_were_copied`
`verified-by: bravebot_tui::clipboard::the_files_keep_the_order_they_were_copied_in`
`verified-by: bravebot_tui::clipboard::a_script_reports_a_name_holding_a_newline_as_one_path`
`verified-by: bravebot_tui::clipboard::output_that_is_not_a_list_of_paths_names_no_files`
`verified-by: bravebot_tui::clipboard::a_file_manager_copy_is_read_from_the_uri_list_it_leaves`
`verified-by: bravebot_tui::clipboard::a_gnome_file_manager_copy_is_read_from_the_type_it_leaves`
`verified-by: bravebot_tui::clipboard::the_files_on_a_pasteboard_are_read_as_their_paths_in_order`
`verified-by: bravebot_tui::clipboard::a_pasteboard_holding_only_a_picture_names_no_files`
`verified-by: bravebot_tui::dropped::the_line_of_paths_reads_back_as_those_paths`
`verified-by: bravebot_tui::dropped::a_path_with_a_space_is_one_word_of_the_line`
`verified-by: bravebot_tui::dropped::a_uri_list_names_the_files_it_lists`
`verified-by: bravebot_tui::dropped::the_verb_line_of_a_gnome_copied_files_payload_is_not_a_path`
`verified-by: bravebot_tui::dropped::a_name_holding_a_newline_stays_one_entry_of_a_uri_list`
`verified-by: bravebot_tui::dropped::a_uri_list_line_that_is_not_a_local_file_names_nothing`
`verified-by: bravebot_tui::dropped::a_uri_naming_localhost_is_a_file_on_this_machine`
`verified-by: bravebot_tui::dropped::a_name_that_is_not_text_refuses_the_list_and_is_not_taken_for_its_lookalikes`
`verified-by: bravebot_tui::app::tests::pasting::a_file_the_clipboard_names_is_staged_as_dropping_it_stages_it`
`verified-by: bravebot_tui::app::tests::pasting::a_copied_file_is_carried_read_or_written_out_by_its_type_as_a_dropped_one_is`
`verified-by: bravebot_tui::app::tests::pasting::a_copied_file_named_with_a_space_and_a_quote_is_one_file`
`verified-by: bravebot_tui::app::tests::pasting::a_copied_file_is_written_as_its_path_in_shell_mode_and_nothing_is_staged`
`verified-by: bravebot_tui::app::tests::pasting::a_copied_file_that_is_gone_says_there_is_nothing_to_paste`
`verified-by: bravebot_tui::app::tests::pasting::a_copied_file_is_staged_while_a_turn_runs_too`
`verified-by: bravebot_tui::app::tests::pasting::a_refused_list_of_copied_files_says_so_and_stages_nothing`
`verified-by: bravebot_tui::app::tests::pasting::a_copied_file_with_an_escape_in_its_name_refuses_a_shell_line_as_a_dragged_one_does`

<a id="PASTE-13"></a>
### PASTE-13: a folded paste can be opened and read whole in the panel

A left click on the cells of a `[Pasted text #N +M lines]` marker in the box, and the `peek` chord
(default Alt-V) with the caret on a marker, open the info panel in a state that shows that paste's
text. The chord with the caret on no marker, or on a marker of another kind, does nothing and says
nothing. A click anywhere else in the box puts the caret there as before
([MOUSE-16](mouse.md#MOUSE-16)). A click on a marker puts the caret at the marker's start, as any click on it does
([MOUSE-16](mouse.md#MOUSE-16)), and opens the state. With two markers in the box the one the caret is on, or the one
clicked, is the one shown. The state is entered at rest and while a turn runs.

The state draws, top to bottom, a heading row with the marker's number and the paste's line count, as
the marker counts them, and a close control (`x`) at its right end; the whole pasted text, after the
removal of [PASTE-10](#PASTE-10), from the row it is scrolled to; and a last row naming the state's
keys. A line longer than the panel wraps to the next row, and an empty line is an empty row. A control
character in the text is drawn as the visible character that names it, so no character of a paste moves
the cursor or changes the terminal's state.

The state is a state of the info panel, entered and left as the files state is
([FILES-3](files-mode.md#FILES-3)). Opening it opens the panel if it was closed. `q`, Escape and
Ctrl-C end it, and so does a click on its `x`, and ending it returns the panel to what it was before:
closed if it was closed, in its ordinary state if it was open. Ctrl-C ends the state and does not stop
a turn that is running. The `panel` chord hides the panel and the state with it. Opening and closing
write nothing to `~/.bravebot/panel` ([PANEL-5](info-panel.md#PANEL-5)). Up, Down, `k`, `j`, PageUp,
PageDown, Home, End, `g` and `G` scroll the text, and the wheel scrolls it with the pointer over the
panel, each stopping at the first row and at the position that leaves the last row at the bottom.
Every other key is the state's and does nothing, so no character reaches the box, Enter sends nothing
and a paste is dropped. On a terminal too narrow for the panel ([PANEL-6](info-panel.md#PANEL-6)) a note
says how wide the terminal has to be and nothing opens, for the key and the click alike.

The state holds a copy of the text and the number of its marker. Opening it does not change the line in the box, the caret or the
paste table, and they are the same after it closes as when it opened. It sends nothing to a model, adds nothing
to the request or the transcript, and writes nothing to the session record. The text is the person's
own words and holds no label ([PASTE-7](#PASTE-7)). Nothing is decided from its content: what opens the
state is a caret position, a click cell and a key, which marker is shown is found by the marker the box
holds, and the text is carried to the screen after each character is mapped to what draws it.

`peek` is one of the actions a settings file can move ([INPUT-32](terminal-input.md#INPUT-32)), and
the list `?` puts up names it.

The desktop window has no info panel. Its form of this clause is a read-only modal, opened from the
paste's chip or with Alt-V ([PASTE-14](#PASTE-14)).

**Why.** The marker keeps a long paste out of the way of the reply being read
([PASTE-1](#PASTE-1)), and a person who pasted it has no other way to check what the marker holds,
short of sending it. Showing it in the panel, as `/diff` shows the files, uses a place that already
has a way in and out, keeps the box and the transcript as they were, and needs no new kind of screen.
It sends nothing, so it is safe to use while the turn runs and before deciding whether to send.

`verified-by: bravebot_tui::app::tests::peeking::a_click_on_the_marker_opens_the_viewer_and_leaves_the_line`
`verified-by: bravebot_tui::app::tests::peeking::a_click_on_other_box_text_moves_the_caret_and_opens_nothing`
`verified-by: bravebot_tui::app::tests::peeking::the_key_opens_the_viewer_with_the_caret_on_a_marker`
`verified-by: bravebot_tui::app::tests::peeking::the_key_does_nothing_with_the_caret_off_a_marker`
`verified-by: bravebot_tui::app::tests::peeking::the_marker_under_the_caret_or_the_pointer_is_the_one_shown`
`verified-by: bravebot_tui::app::tests::peeking::the_key_works_while_a_turn_runs_and_ctrl_c_closes_only_the_viewer`
`verified-by: bravebot_tui::app::tests::peeking::a_click_opens_the_viewer_while_a_turn_runs`
`verified-by: bravebot_tui::app::tests::peeking::every_way_of_closing_returns_the_panel_to_what_it_was`
`verified-by: bravebot_tui::app::tests::peeking::a_terminal_too_narrow_gets_a_note_and_nothing_opens`
`verified-by: bravebot_tui::app::tests::peeking::the_keys_scroll_the_text_between_its_ends`
`verified-by: bravebot_tui::app::tests::peeking::the_wheel_scrolls_the_text_under_the_pointer_only`
`verified-by: bravebot_tui::app::tests::peeking::long_lines_wrap_and_control_characters_are_drawn_visibly`
`verified-by: bravebot_tui::app::tests::peeking::the_line_and_caret_are_untouched_by_the_viewer`
`verified-by: bravebot_tui::keybindings::the_peek_chord_is_alt_v_and_can_be_moved`
`verified-by: bravebot_tui::keybindings::another_action_cannot_take_the_peek_chord_and_the_peek_action_cannot_take_another`
`verified-by: bravebot_tui::render::the_shortcut_list_names_the_peek_chord`
`verified-by: by-construction (the state is a copy of the text held in the session and is drawn from it by the panel's row builder, which takes no handle to the request, the transcript's writer or the session record; the click and key tests assert that the transcript is empty after opening and closing)`

<a id="PASTE-14"></a>
### PASTE-14: the desktop window folds a long paste and opens it whole in a modal

A text paste into the desktop composer that holds three or more newlines, after `\r\n` and `\r`
are read as `\n`, folds to `[Pasted text #N +M lines]`, as in [PASTE-1](#PASTE-1). The threshold is
the terminal's `FOLD_AT_NEWLINES`, the marker is the English form of the terminal's `paste-folded`
message, and the lines are counted as the terminal counts them, so a trailing newline does not
start a line. `N` comes from the one counter a draft's drops and pasted pictures use
([PASTE-11](#PASTE-11)), so a drop, a picture and a folded paste in one draft are `#1`, `#2` and
`#3`, and a number is not used again after a send. The marker is written at the caret as a pasted
picture's is, and the words around it stay as they were. A paste over a selection writes the marker
in its place, as the browser's own paste would. A shorter paste is left to the browser
and lands whole, and so is one longer than 200000 characters, the most a saved folded paste holds,
so every folded paste survives a reopen. A paste the preload took as a picture is not read as text. The window has no shell
mode, so PASTE-1's command line has no case here.

The words are put back wherever the draft leaves the box: a send, a message queued behind a turn, a
line interjected into a running turn, the question `/btw` asks and the argument `/schedule` takes.
Every copy of a marker is put back, a marker the person deleted puts back nothing, and a picture's
marker is left for the grants ([PASTE-11](#PASTE-11)), which are found on the folded draft. The
request, the transcript row and the queued message hold the words and never the marker. A send the bridge refuses, and an aside that fails, put the words back into the box rather
than a marker that the cleared staging no longer names.

The draft is saved to disk with the conversation, and the folded pastes it still names are saved
beside it. When the conversation opens again they are staged again and the counter starts above
their numbers, so a reopened draft unfolds as it did before it was closed. A saved list that is
not a list is dropped, and so is an entry whose marker, text, number or line count is not of its
type.

A folded paste has a chip above the box with a text glyph, the label `Pasted text #N` and its line
count. The chip's controls name the marker for a screen reader. Removing the chip deletes the marker, and deleting the marker takes the chip off. The chip is not
marked as trusted context, because pasted text holds no labels ([PASTE-7](#PASTE-7)).

The window has no info panel, so its form of [PASTE-13](#PASTE-13) is a read-only modal. A click on
the chip's label, and Alt-V with the caret at the start, inside or at the end of a folded marker,
open it on that paste. Alt-V is matched by the physical key, because Option-V types a character on
macOS. With the caret on plain text or on a picture's marker the key is not cancelled and nothing
opens. The modal is titled `Pasted text #N`, says how many lines the paste has and that viewing
sends nothing, and shows the whole text in a `<pre>` with a `Wrap` toggle and a `Done` button.
Escape and the close control end it as well. It works at rest and while a turn runs. It holds a copy
of the text, and the draft, the caret and the staging are the same after it closes as before it
opened. It sends nothing to a model and adds nothing to the transcript.

`verified-by: by-construction (the renderer is not a crate this workspace compiles, so ui/scripts/pastes.test.mjs test "a paste of fewer newlines than the threshold is left to the browser, and one at it folds" pins the threshold and the \r\n and \r forms; make check-ui runs the test)`
`verified-by: by-construction (ui/scripts/pastes.test.mjs test "lines are counted as a person would count them" pins that a trailing newline starts no line; make check-ui runs the test)`
`verified-by: by-construction (ui/scripts/pastes.test.mjs test "a folded paste lands at the caret and leaves the words around it as they were" pins where the marker is written; make check-ui runs the test)`
`verified-by: by-construction (ui/scripts/pastes.test.mjs test "a folded paste over a selection replaces the selected words" pins that the selection is replaced; make check-ui runs the test)`
`verified-by: by-construction (ui/scripts/pastes.test.mjs test "a paste longer than a saved folded paste may be is left to the browser" pins the length cap and that the longest folded paste is saved whole; make check-ui runs the test)`
`verified-by: by-construction (ui/scripts/pastes.test.mjs test "a drop, a picture and a folded paste share one counter, and a number never comes round again" pins the shared counter; make check-ui runs the test)`
`verified-by: by-construction (ui/scripts/pastes.test.mjs test "unfolding puts back every copy of a marker, drops a deleted one, and leaves pictures alone" pins unfolding and that a folded paste is never a grant; make check-ui runs the test)`
`verified-by: by-construction (ui/scripts/pastes.test.mjs test "the folded paste under the caret is found at its start, inside it and at its end" pins which marker the chord finds, and none on plain text or a picture's marker; make check-ui runs the test)`
`verified-by: by-construction (ui/scripts/pastes.test.mjs test "a draft leaving the box carries the words, and what it names is found on the folded draft" pins the words a draft leaves with and the staged items found on the folded draft; make check-ui runs the test)`
`verified-by: by-construction (ui/scripts/pastes.test.mjs test "every exit of a draft holding a folded paste carries the pasted words" pins the send, the queued message, the /btw question and the /schedule argument; make check-ui runs the test)`
`verified-by: by-construction (ui/scripts/interjecting.test.mjs test "a line holding a folded paste goes into the turn as the pasted words, and is queued as them if declined" pins the interjected line; make check-ui runs the test)`
`verified-by: by-construction (ui/scripts/pastes.test.mjs test "a saved draft keeps its folded pastes through the preferences file, and a malformed value is dropped" pins saving and the validation of the saved list; make check-ui runs the test)`
`verified-by: by-construction (ui/scripts/pastes.test.mjs test "folded pastes saved with a draft are staged again, unfold, and the next marker is numbered above them" pins restaging when a conversation opens; make check-ui runs the test)`
`verified-by: by-construction (ui/scripts/pastes.test.mjs test "the folded marker and threshold are the terminal’s" reads crates/i18n/locales/en-US.ftl and crates/tui/src/state/attachments.rs and fails if the window's marker wording or threshold differ from the terminal's; make check-ui runs the test)`
`verified-by: by-construction (ui/scripts/drive-paste.mjs pastes from the real clipboard into the real app and bridge against a stub model under xvfb-run, and asserts that a two-line paste lands whole, a five-line paste folds to its marker and a chip not marked as trusted context, deleting the marker takes the chip off, the chip and Alt-V at the marker's start, middle and end open the modal on all five lines while Alt-V on plain text or an [Image #n] marker opens nothing, Done, Escape and the close control leave the draft, caret and chip as they were, a paste folds and opens while a turn runs, and the model and the transcript row receive the words and never the marker)`

## Known costs

- **The desktop decodes a pasted picture in its main process.** The clipboard's bytes come from
  whatever application copied them, and the main process decodes them to write them out again as
  PNG and to draw the chip's drawing. A dropped picture is decoded in the sandboxed renderer
  instead. The decode is the one Electron's clipboard image read always did, and one over the cap
  as it came off the clipboard is refused before it is decoded.

- **Two ways out of the desktop box do not unfold a paste.** Handing a draft off to a new chat
  sends the draft as it stands, so a folded marker in it is sent as the marker
  ([PASTE-14](#PASTE-14)). Enhancing a draft is refused while it holds a folded paste, as it is
  while it holds a picture or a file, rather than enhancing the words behind the marker.

- **The desktop does not remove invisible characters from a folded paste.** The viewer and the
  request carry whatever the clipboard held, because [PASTE-10](#PASTE-10) runs only in the
  terminal. The text is drawn in a `<pre>`, so a control character cannot change the page, but a
  bidirectional control can reorder what is drawn.

- **Whatever put the files on the clipboard chose them.** Ctrl-V reads the list as it finds it, so
  a program that wrote one there has the file read as a drop would be. The marker, or the path of a
  type nothing takes, is in the line before anything is sent, so the person sees which files.
  A file read this way is trusted and may be from outside the workspace, as a dropped one is.

- **A read of the clipboard starts a process for each thing it looks for.** Ctrl-V asks for files,
  then for each picture flavour, then for text, by starting the platform's tool each time, and
  stops at the first answer. On macOS a clipboard holding only text starts five, `osascript` for the
  files, the PNG, the JPEG and the TIFF and then `pbpaste`, at about 70 ms each. On Windows each
  read is a PowerShell that loads the clipboard assembly.

- **A TIFF can paste a picture where the text beside it was meant.** On macOS the system offers a
  TIFF for any picture it holds, and the TIFF is the last picture flavour tried, so an application
  that offers a drawing of what it copied only as a TIFF is pasted as that drawing by Ctrl-V.
  Command-V pastes the text.

- **The Windows readers run in no test.** CI lints the Windows target and does not run it, so the
  PowerShell scripts that read a clipboard's file list and its picture are checked by reading only.

- **A pasted picture lands on disk.** It is written into the session record so a resume can restore
  it (PASTE-9), which means a screenshot pasted into a session outlives the session. Deleting the
  session removes it.

- **A screenshot of a hostile page puts a stranger's words into the planner's context as though
  the user had typed them.** Nothing inspects the pixels and nothing could. What justifies it is
  that the user chose what to copy, can see on their own screen what they pasted, and is the party
  this serves. It is the cost shell mode carries, reached by another route.

- **Only a paste is cleaned.** Text typed at the keyboard, a path a file dropped onto a prompt
  writes ([dropping.md](dropping.md)), and a prompt given with `-p` or in the desktop application's
  message box are not run through the filter, so a character nobody can see can still reach a
  message by those routes. A drop onto a shell line where a name holds such a character writes nothing
  ([DROP-6](dropping.md#DROP-6)).

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
