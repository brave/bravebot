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
so the field they approve is the field the call turns on. For a picture, which [VET-4](#VET-4)
specifies and nothing builds yet, they are shown a path to a copy of those bytes instead.

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

A reference to nothing, since there is nothing to check or to show. A picture, whether or not a
check is made first: a check reads text, and the bytes behind a picture slot are a data URI that a
promotion would hand the planner as text it may trust. A private sentence, per VET-1. And a call
from a delegate, which is neither offered the tool nor answered when it names it anyway.

[VET-4](#VET-4) specifies a narrower refusal for a picture, and nothing builds it yet. Until
something does, the picture sentence above is the whole of what happens to one. Built, that
sentence refuses a picture only where VET-4 refuses one, a PDF included, since a check then looks
at the file itself ([CHECK-15](../vetting.md#CHECK-15)).

A reference to a file nothing has read yet is opened rather than refused: naming one is the
ordinary way for the planner to ask about a file it may not read.

**Why a delegate is not offered it.** The prompt belongs to the person who set the sub-task going,
about content they never asked to see, in the middle of work they are not reading. What crosses
back from a delegate is [delegation.md](../delegation.md)'s question, and this would put bytes
into a context nobody at the keyboard is watching.

`verified-by: bravebot_core::policy::a_check_over_nothing_is_refused`
`verified-by: bravebot_core::policy::a_check_over_a_picture_is_refused`
`verified-by: bravebot_core::policy::a_picture_is_never_promoted_whoever_endorsed_it`
`verified-by: bravebot_agent::turn::bypassing_with_no_screening_still_refuses_to_promote_a_picture`
`verified-by: bravebot_agent::tools::a_delegate_is_never_offered_a_way_to_promote_a_slot`

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

A picture, which [VET-4](#VET-4) specifies and nothing builds yet, is answered the same way and
comes back differently: the result says it is attached rather than holding it as text, and VET-4
says where it goes instead.

`verified-by: bravebot_agent::turn::content_a_person_reads_after_a_check_reaches_the_planner`
`verified-by: bravebot_agent::turn::content_a_person_refuses_after_a_check_stays_out_of_the_planner`
`verified-by: bravebot_agent::turn::with_auto_vetting_a_safe_verdict_reaches_the_planner_unasked`
`verified-by: bravebot_agent::turn::screening_an_unattended_run_keeps_back_content_a_check_objected_to`
`verified-by: bravebot_agent::turn::screening_an_unattended_run_promotes_content_a_check_found_nothing_in`

<a id="VET-4"></a>
### VET-4: a picture or a PDF is vetted as text is, and put to a person as a file to open

Nothing builds this yet. Until something does, [VET-2](#VET-2) refuses every picture, and this
clause is a design to be argued about rather than a description of what a session does. The check
half of it is [CHECK-15](../vetting.md#CHECK-15).

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
promotes on a safe verdict and refuses on any other ([MODE-4](../permission-modes.md#MODE-4)).

**Where it is refused, whatever a check would say.** Three cases refuse a picture, as VET-2 today
refuses every one. A delegate is still neither offered the tool nor answered. A session whose model
the gateway's roster lists inputs for, and not the one this file needs, refuses before a check is
made: `image` for a raster picture and `file` for a PDF, in the roster's own words. The check runs
on that model, and a promotion would put the file into every request after it. That roster is the
only place this is known from: one that says nothing about a model's inputs claims nothing, and
neither does a backend with no roster, so those go ahead, and this would be the first thing to
decide anything on what a roster says a model takes. And a run bypassing permissions with no
screening asked for refuses, where it would answer yes, unshown, to the same prompt about text. That
mode makes no check ([MODE-4](../permission-modes.md#MODE-4)), so a picture would reach the planner
with nothing having looked at it, neither a person nor a model. Refusing is the narrower answer, and
a later change can widen it without taking away anything a run relied on. What such a run loses is
the pixels and not the words: a processor can look at the picture, and its answer is text the mode
promotes like any other.

**Which picture is the planner's choice.** It names the slot, as [VET-1](#VET-1) has it for text,
and the person answers about that one slot. Nothing the planner says changes what the file they are
shown holds.

**What the person is shown.** A path to a file holding the picture, which they open in their own
viewer before pressing `y` to let the planner see it. The file is a copy of the picture the slot
holds, the bytes its data URI encodes. Turning that URI back into bytes cannot fail on anything the
picture holds, because the driver made the encoding itself at the read. The copy is written as the
prompt is drawn, into a directory of its own under the person's cache directory that only their
account can read and that no program the session confines may write. It is never the system
temporary directory, which every confined program may write
([SANDBOX-12](../sandboxing.md#SANDBOX-12)), never the working directory, and never `~/.bravebot`,
which an incognito session adds nothing to. It is named at random with the extension the picture was
read under, created only where no file of that name exists, and removed when the prompt closes,
however it closes. Writing it is a release of the slot's bytes to a file, at a gate of its own that
the trail records. It is not the release that draws text on a prompt, since a file outlives the
screen and is read by a program that is not this one. Beside the path are where the picture came
from, in the driver's words as VET-1 has it, the planner's `expects`, the picture's media type and
its size in bytes, what the check said, and a sentence saying that a model reads words in a picture
that a person can miss, which for a PDF adds that it can hold text no page draws.

**Why a copy, and not the path it was read from.** The file can be rewritten between the read and
the prompt, by the planner's own write or by anything else on the machine. Opening that path would
show what is there now, and a yes releases what the slot holds, which is what was there at the read.
The copy is the slot's bytes, so what the person opens is what the planner is given.

**Why a file, and not the picture on the prompt.** Neither front end, the terminal client or the
desktop window, draws a picture. A file opens in the person's own viewer, where they can zoom in,
which is how faint or tiny text is found. The copy is still the bytes, one step away, rather than a
description of them, which is why [PROMPT-1](../prompting.md#PROMPT-1) makes room for it.

**What a promotion does.** The result tells the planner the picture is attached. Its next request
carries the picture in a message of its own after that result, beside the driver's words naming the
path it was read from, since no backend here is built to carry a picture inside a tool result. It is
never carried as base64 in the text of a result, where a model reads a data URI as characters rather
than looking at a picture. From then on it is part of the conversation, the way the bytes of a
promoted text slot are. It is never joined to a message of the person's own, which is what a paste
is ([PASTE-2](../pasting.md#PASTE-2)): what lets it through is an endorsement of one slot
([CHECK-8](../vetting.md#CHECK-8)), and the trail records it as that. A refusal is VET-3's, word for
word.

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

`verified-by: none`
