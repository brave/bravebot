---
id: VET
title: vet_content
status: normative
governs:
  - crates/agent/src/tools.rs
documented-by: docs/website/docs/security/vetting.md
---

## Scope

Asking to be shown one quarantined slot after a confined check has read it. The reference naming
the slot is routing; `expects` is the planner's own sentence and is neither. What the check is,
what it may say, and what a person's answer does is [vetting.md](../vetting.md).

This is the one of the three checked prompts the planner asks for by name, and so the only one that
carries an `expects`. The other two come with a read the planner made for its own reasons:
[read-output.md](read-output.md) and the vouch offer in [trust-map.md](../trust-map.md#TRUST-8).
[CHECK-10](../vetting.md#CHECK-10) covers all three.

## Clauses

<a id="VET-1"></a>
### VET-1: the routing field is one slot, and it is the field a person approves

The call names one reference and says in a sentence what the planner expects it to hold. The
reference is the only field that decides anything, and the person is shown the bytes behind it,
so the field they approve is the field the call turns on. For a picture they are shown a path to
a copy of those bytes instead ([VET-4](#VET-4)).

`expects` is the planner's own words. It is sent to the check, so that a page can be judged
against what it was supposed to be, and it is drawn on the prompt, so that a person can see why
the planner wants this. It is not a destination and must not be private.

Where the content came from is said in the driver's words rather than in the planner's, and it is
said as a path, a URL or a command rather than as a reference name. A reference means something to
the planner and nothing at all to the person being asked about it.

`verified-by: bravebot_agent::turn::content_a_person_reads_after_a_check_reaches_the_planner`
`verified-by: bravebot_core::policy::a_private_expectation_cannot_direct_a_check`
`verified-by: bravebot_core::policy::a_check_says_where_the_content_came_from_and_not_which_slot_it_is_in`
`verified-by: bravebot_core::policy::a_check_over_a_file_reference_says_the_path_and_not_the_reference`

<a id="VET-2"></a>
### VET-2: what it refuses

A reference to nothing, since there is nothing to check or to show. A private sentence, per VET-1.
A call from a delegate, which is neither offered the tool nor answered when it names it anyway. And
a picture, a PDF included, in the cases [VET-4](#VET-4) lists and in no others: otherwise a check
looks at the file itself ([CHECK-15](../vetting.md#CHECK-15)) and the person is handed a copy of it.

A picture is never promoted as text, whoever endorsed it. The bytes behind a picture slot are a data
URI, which a model reads as characters rather than looking at, so a picture only ever comes back as
an attachment.

A reference to a file nothing has read yet is opened rather than refused: naming one is the
ordinary way for the planner to ask about a file it may not read.

**Why a delegate is not offered it.** The prompt belongs to the person who set the sub-task going,
about content they never asked to see, in the middle of work they are not reading. What crosses
back from a delegate is [delegation.md](../delegation.md)'s question, and this would put bytes
into a context nobody at the keyboard is watching.

`verified-by: bravebot_core::policy::a_check_over_nothing_is_refused`
`verified-by: bravebot_core::policy::a_picture_is_never_promoted_as_text_whoever_endorsed_it`
`verified-by: bravebot_core::policy::text_is_never_attached_as_a_picture`
`verified-by: bravebot_agent::turn::a_model_listed_as_taking_no_pictures_is_never_asked_about_one`
`verified-by: bravebot_agent::turn::a_picture_with_nowhere_to_copy_it_is_kept_back`
`verified-by: bravebot_agent::tools::a_delegate_is_never_offered_a_way_to_promote_a_slot`
`verified-by: bravebot_agent::turn::a_delegate_naming_vet_content_is_told_there_is_no_such_tool`
`verified-by: bravebot_agent::turn::an_unread_reference_to_a_picture_is_opened_as_a_picture`

<a id="VET-3"></a>
### VET-3: the result is the bytes, or a refusal that says to work without them

Where the person agrees, the content comes back as text the planner may read. Where they do not,
the planner is told the slot was kept back from it, and told to work with what it has or to say what
it needed, rather than being left to guess or to ask again. Nothing the check wrote goes back either
way, and neither does who decided: a refusal on a screened run with nobody to ask reads the same as a
refusal somebody typed, which is the only wording that is true of both.

Where auto-vetting is on and the check found nothing, the content comes back with no prompt drawn
([CHECK-12](../vetting.md#CHECK-12)). Where it is on and the check said anything else on a run that
puts no prompts to anybody, that is the refusal above. The result the planner reads is the same in
every case: it is told the bytes or told they are not coming, and never which of the three answered
or what the check said, so nothing it writes can be aimed at one path rather than another.

A picture is answered the same way and comes back differently: the result says it is attached
rather than holding it as text, and [VET-4](#VET-4) says where it goes instead.

`verified-by: bravebot_agent::turn::content_a_person_reads_after_a_check_reaches_the_planner`
`verified-by: bravebot_agent::turn::content_a_person_refuses_after_a_check_stays_out_of_the_planner`
`verified-by: bravebot_agent::turn::with_auto_vetting_a_safe_verdict_reaches_the_planner_unasked`
`verified-by: bravebot_agent::turn::screening_an_unattended_run_keeps_back_content_a_check_objected_to`
`verified-by: bravebot_agent::turn::screening_an_unattended_run_promotes_content_a_check_found_nothing_in`
`verified-by: bravebot_agent::turn::a_picture_a_person_keeps_out_is_never_attached`

<a id="VET-4"></a>
### VET-4: a picture or a PDF is vetted as text is, and put to a person as a file to open

The check half of it is [CHECK-15](../vetting.md#CHECK-15).

**What it covers.** Every file [READ-5](read-file.md#READ-5) reads as bytes, which is the whole of
the closed table: PNG, JPEG, GIF, WebP and PDF. This clause says picture for all five, as that
clause does, and says where a PDF goes differently. What a person is asked about is what their
viewer draws, the pixels of a raster picture and the pages of a PDF, and those are what a model
reads words off, so somebody who looks closely enough sees what is drawn. What no viewer puts
plainly in front of them is the first cost below, and a PDF holds more of it than a raster picture
does: text that no page draws, such as the text layer behind a scanned page. The check is given a
PDF as a processor is given one, so where its backend hands the model that text the check reads it,
and the person is told what it found rather than shown the text.

**It goes the way text goes.** A check looks at the picture first, and its verdict counts as a
verdict about text does ([CHECK-15](../vetting.md#CHECK-15)). With auto-vetting off the prompt is
drawn carrying the verdict, and a yes promotes. With it on, a safe verdict promotes with no prompt
drawn and any other verdict draws one ([CHECK-12](../vetting.md#CHECK-12)). A one-shot run refuses
unless it was started with `--vet`, and then a safe verdict promotes
([PROMPT-9](../prompting.md#PROMPT-9)). A run bypassing permissions that asked for screening
promotes on a safe verdict and refuses on any other, and one that asked for none promotes with no
check made, as it answers yes, unshown, to the same prompt about text
([MODE-4](../permission-modes.md#MODE-4)). That mode is somebody saying in advance that nothing is
to be asked, and a picture is not an exception they were offered.

**Where it is refused, whatever a check would say.** A delegate is still neither offered the tool
nor answered. A session whose model the gateway's roster lists inputs for, and not the one this file
needs, refuses before a check is made: `image` for a raster picture and `file` for a PDF, in the
roster's own words. The check runs on that model, and a promotion would put the file into every
request after it. That roster is the only place this is known from: one that says nothing about a
model's inputs claims nothing, and neither does a backend with no roster, so those go ahead. It is
the first thing here to decide anything on what a roster says a model takes. The terminal client
reads the roster's list for the model in force when the session opens and again at every change of
model, and the one-shot command line for the model it runs on; the desktop window reads none. The
list speaks for that model alone, so a turn addressed to a definition that names another model is
told nothing by it, and there, as in the desktop window, a picture goes ahead to the check. And a
machine naming no cache directory refuses wherever a prompt would be drawn, since there is nowhere
to put a copy only the person can read.

**Which picture is the planner's choice.** It names the slot, as [VET-1](#VET-1) has it for text,
and the person answers about that one slot. Nothing the planner says changes what the file they are
shown holds.

**What the person is shown.** A path to a file holding the picture, which they open in their own
viewer before pressing `y` to let the planner see it. The file is a copy of the picture the slot
holds, the bytes its data URI encodes. Turning that URI back into bytes cannot fail on anything the
picture holds, because the driver made the encoding itself at the read. The copy is written as the
prompt is drawn, into `bravebot/vetting` under the person's cache directory, a directory only their
account can read and that no program the session confines may write: `~/Library/Caches` on macOS,
`%LOCALAPPDATA%` on Windows, and `$XDG_CACHE_HOME` or `~/.cache` elsewhere. It is never the system
temporary directory, which every confined program may write
([SANDBOX-12](../sandboxing.md#SANDBOX-12)), never the working directory, and never `~/.bravebot`,
which an incognito session adds nothing to. It is named at random with the extension the driver's
table gives its media type, created only where no file of that name exists, and removed when the
prompt closes, however it closes. Writing it is a release of the slot's bytes to a file, at a gate
of its own that the trail records. It is not the release that draws text on a prompt, since a file
outlives the screen and is read by a program that is not this one. Beside the path are where the
picture came from, in the driver's words as VET-1 has it, the planner's `expects`, the picture's
media type and its size in bytes, what the check said, and a sentence saying that a model reads
words in a picture that a person can miss, which for a PDF adds that it can hold text no page draws.

**Why a copy, and not the path it was read from.** The file can be rewritten between the read and
the prompt, by the planner's own write or by anything else on the machine. Opening that path would
show what is there now, and a yes releases what the slot holds, which is what was there at the read.
The copy is the slot's bytes, so what the person opens is what the planner is given.

**Why a file first, and a drawing only where the terminal can show one.** The desktop window draws
no picture, and a terminal draws one only if it speaks a graphics protocol. A file opens in the
person's own viewer, where they can zoom in, which is how faint or tiny text is found, so the path
is on every prompt about a picture and is never replaced by a drawing. The copy is still the bytes,
one step away, rather than a description of them, which is why
[PROMPT-1](../prompting.md#PROMPT-1) makes room for it.

**The drawing.** The terminal client asks the terminal once, at start-up, which graphics protocol
it draws. Where the answer is Kitty, iTerm2 or Sixel, the prompt for a PNG or JPEG also draws the
picture, above the path and inside the margin bar every other line of the prompt sits behind, sized
to the space the terminal has. The drawing is left out, and the prompt is what it was without it,
when any of these holds: the protocol is halfblocks or unknown, because a picture made of coloured
blocks cannot show small writing and a person who saw one would take it for having looked; the
`NO_COLOR` setting asks for a plain terminal; the file is a PDF, a GIF or a WebP, none of which is
decoded; the picture is over 8192 pixels a side or would need over 256 MiB to decode; it does not
decode; or the prompt is scrolled or sized so that the whole drawing does not fit in the visible
body. Under it a line says that small or faint writing may not show at that size and to open the
copy.

**What a promotion does.** The result tells the planner the picture is attached. Its next request
carries the picture in a message of its own after that round's results, beside the driver's words
naming the reference it was held under and saying it came from a file and not from the user, since
no backend here is built to carry a picture inside a tool result. The session record tags that
message as the driver's ([LAYER-6](../layering.md#LAYER-6)), so a cut never counts it as a prompt
the person typed and the desktop window draws it back as a picture let through; the terminal
client, which has no row of its own for it, draws its words plainly, and those say it came from a
file and not from the user. It is never carried as base64 in the text of a result, where a model
reads a data URI as characters rather than looking at a picture. From then on it is part of the
conversation, the way the bytes of a promoted text slot are. It is never joined to a message of the
person's own, which is what a paste is ([PASTE-2](../pasting.md#PASTE-2)): what lets it through is
an endorsement of one slot ([CHECK-8](../vetting.md#CHECK-8)), and the trail records it as that. A
refusal is VET-3's, word for word.

**What it costs.**

- **A yes about a picture is weaker than a yes about text.** A raster file holds bytes no viewer
  draws, such as its metadata, and a format with more than one frame is shown to a person as a
  sequence while a model is given whatever the backend takes from it, which may be one frame. A PDF
  holds more: a text layer, text drawn too small or in the colour of the page, annotations, form
  fields and attached files, and the pages a person reads need show none of it. What a backend hands
  its model of any of this is out of sight of this side, so for a PDF the check and the person can
  each be shown something the other is not. Stripping it would mean decoding attacker-owned bytes in
  the process that holds the keys, for five formats, one of them PDF, which is a larger exposure
  than the one it removes.
- **Drawing a PNG or JPEG decodes attacker-owned bytes in the process that holds the keys.** That is
  the exposure the bullet above declined for stripping, taken for these two formats to put the
  picture on the prompt. It is not isolated: there is no confined child for it, and the decoders
  are memory-safe Rust and not a sandbox. What limits it is that only PNG and JPEG are compiled in,
  that the size is read from the header and refused over 8192 pixels a side or 256 MiB before any
  pixel is allocated, that the file is capped at the attachment size, and that the decode runs on a
  thread of its own with at most four at once, so a slow one does not stop the prompt and a panic
  is a missing picture. A decoder fault that is not a panic runs with the process's keys. A person
  who wants none of this runs without a graphics terminal, where nothing is decoded, or sets
  `NO_COLOR`.
- **A drawing is not a look at the whole picture.** It is scaled to a few dozen columns, so writing
  that a viewer at full size shows is often not legible in it. It is drawn to help a person notice
  what the picture is, not to stand in for opening the copy, and the line under it says so.
- **Bypassing with no screening lets through a picture nothing looked at.** Neither a person nor a
  model has seen it, as neither has seen text the mode promotes. That is the mode as asked for
  ([MODE-4](../permission-modes.md#MODE-4)), and what bounds it is what bounds text: one slot, once,
  `(T,priv)`, and no trust rule.
- **A check is easier to talk into `safe` about a picture than about text.**
  [CHECK-15](../vetting.md#CHECK-15) says why, and where a safe verdict answers alone, a picture that
  manages it reaches the planner with nobody having seen it.
- **Nothing knows whether the copy was opened.** A yes from somebody who did not open it looks
  exactly like one from somebody who did, as a yes about text nobody read does. The check's sentence
  is on the prompt and the picture is not, so a person who does not open the copy answers on that
  sentence alone.
- **The copy is on the machine the session runs on.** Somebody at a terminal attached to another
  machine opens it there or not at all, and the prompt cannot make the path reachable from where
  they sit. Answering no is what is left to them.
- **Opening it hands attacker-owned bytes to the person's own viewer.** Whatever that viewer's
  decoder does with a hostile file happens outside every confinement this process has. A PDF viewer
  can do more with one than decode it: some run scripts a document carries, or follow its links when
  asked, and none of that is anything this process sees.
- **The copy carries no label.** A viewer is no surface of this program's, so nothing marks what it
  draws as untrusted, as [LABEL-10](../labels.md#LABEL-10) has every other carrier of released
  content do. The prompt beside the path is where the person is told where it came from.
- **On the wire, the picture is in the user's role.** A message after a tool result is a user
  message on every backend here, and Bedrock merges it into the turn that holds the result, so a
  model can take the picture as something the person sent. The driver's words beside it say where
  it came from, and nothing mechanical holds that. What keeps it apart from a paste is the session
  record and the trail, not what the model is sent.
- **A program running unconfined as the person's account can rewrite the copy.** Nothing compares it
  at the answer, so the person may say yes about a picture other than the one the planner is given.
  Such a program can do more than that as that account already.
- **A crash leaves the copy behind.** It is removed as the prompt closes, so a process killed with
  the prompt open leaves one file in that directory, in an incognito session as in any other.
- **A promotion puts the picture in the session record.** It is part of the conversation from then
  on, so outside an incognito session it is written down with the rest and comes back on a resume,
  as a pasted picture does ([PASTE-9](../pasting.md#PASTE-9)).

`verified-by: bravebot_agent::turn::a_picture_a_person_opens_and_lets_through_is_attached_after_the_results`
`verified-by: bravebot_agent::turn::a_picture_a_person_keeps_out_is_never_attached`
`verified-by: bravebot_agent::turn::with_auto_vetting_a_safe_verdict_attaches_a_picture_unasked`
`verified-by: bravebot_agent::turn::bypassing_with_no_screening_attaches_a_picture_unshown`
`verified-by: bravebot_agent::turn::a_model_listed_as_taking_no_pictures_is_never_asked_about_one`
`verified-by: bravebot_agent::turn::a_list_for_another_model_refuses_no_picture`
`verified-by: bravebot_agent::turn::a_picture_with_nowhere_to_copy_it_is_kept_back`
`verified-by: bravebot_agent::turn::a_pdf_a_person_lets_through_is_attached_as_a_file`
`verified-by: bravebot_agent::vet::a_copy_of_a_picture_is_private_and_removed_with_its_prompt`
`verified-by: bravebot_tui::confirm::a_picture_is_drawn_on_the_prompt_beside_the_path_to_its_copy`
`verified-by: bravebot_tui::confirm::a_drawn_picture_sits_inside_the_margin_bar`
`verified-by: bravebot_tui::confirm::a_picture_is_not_drawn_without_a_real_graphics_protocol`
`verified-by: bravebot_tui::confirm::only_a_real_protocol_and_a_raster_picture_are_given_a_drawing`
`verified-by: bravebot_tui::confirm::the_drawing_is_sized_to_the_terminal_and_dropped_when_it_would_not_fit`
`verified-by: bravebot_tui::confirm::a_picture_that_does_not_fit_the_visible_prompt_is_not_drawn`
`verified-by: bravebot_tui::preview::a_picture_declaring_a_huge_canvas_is_refused_before_it_is_allocated`
`verified-by: bravebot_tui::preview::a_decode_that_panics_is_a_missing_picture_and_not_a_crash`
`verified-by: bravebot_core::policy::a_picture_the_model_is_listed_as_not_taking_is_refused_before_a_check`
`verified-by: bravebot_core::policy::a_picture_is_promoted_once_by_any_endorsement_and_attached_as_itself`
`verified-by: bravebot_core::policy::a_copy_of_a_picture_is_a_recorded_release`
`verified-by: bravebot_tui::confirm::a_picture_is_put_to_the_person_as_a_copy_to_open`
`verified-by: bravebot_tui::app::the_inputs_adopted_are_the_chosen_models_own`
