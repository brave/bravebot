---
id: READ
title: read_file
status: normative
governs:
  - crates/agent/src/workspace.rs
---

## Scope

Reading one file. `path`, `offset` and `limit` are routing; there are no content arguments. The
result is the lines, or a reference when the planner may not see them.

## Clauses

<a id="READ-1"></a>
### READ-1: a read the planner may not see does not open the file

Where the result would be quarantined, the path is checked, the file is confirmed present and
text, and the label is fixed from the trust map, all at the moment the planner asks. The bytes are
read only when a processor or a write needs them, and the path is checked **again** then, so a
file that lost its trust in between is read at the lower label.

**Why.** Most of what an agent reads in a directory nobody vouched for is a file it turns out not
to want. Re-checking on the second pass is what stops a reference issued at one label delivering
bytes at another.

`verified-by: bravebot_agent::turn::a_file_the_planner_may_not_see_is_reserved_rather_than_opened`
`verified-by: bravebot_agent::turn::a_page_of_a_file_the_planner_may_not_see_is_reserved_too`
`verified-by: bravebot_core::policy::a_path_that_lost_its_trust_fills_the_slot_untrusted`
`verified-by: bravebot_core::policy::a_read_from_an_unvouched_path_is_untrusted`
`verified-by: bravebot_core::policy::a_read_from_a_trusted_path_is_trusted`

<a id="READ-2"></a>
### READ-2: a long file is paged, and says where to continue from

Reads cap at 500 lines and 2000 characters per line, report the range returned, and give the
offset to continue from.

`verified-by: bravebot_agent::workspace::a_paged_read_is_capped_and_says_where_to_continue`
`verified-by: bravebot_agent::workspace::the_reported_next_offset_returns_the_following_lines`
`verified-by: bravebot_agent::workspace::an_over_long_line_is_shortened_and_counted`
`verified-by: bravebot_agent::workspace::an_offset_past_the_end_returns_nothing_and_says_the_length`
`verified-by: bravebot_agent::turn::the_model_can_ask_for_a_later_page`

<a id="READ-3"></a>
### READ-3: a file that is not text is reported as binary

Never as a decoding error, which would read as a fault rather than as a fact about the file. A
picture is the exception and is [READ-5](#READ-5): there is something to do with one, so saying
"binary" would be refusing a read that can be answered.

`verified-by: bravebot_agent::workspace::a_binary_file_is_reported_as_binary`
`verified-by: bravebot_agent::workspace::a_paged_read_of_a_binary_file_is_refused`
`verified-by: bravebot_agent::workspace::text_files_are_not_mistaken_for_binary`
`verified-by: bravebot_agent::workspace::an_empty_file_is_not_binary`

<a id="READ-4"></a>
### READ-4: the planner may choose which file to read

A read changes nothing and is confined to the working directory, so the choice is promoted rather
than put to a person, and every such choice is recorded as a promotion so an audit can separate
the planner's decisions from the user's.

The promotion grants routing and not reach, so the confinement it rests on holds however the result
comes back. A picture read into a `data:` URI is held to the working directory and the directories
the user added exactly as a page of text is: what the file turns out to contain is not a reason to
resolve its path differently.

`verified-by: bravebot_core::policy::a_model_proposal_can_be_promoted_for_a_confined_read`
`verified-by: bravebot_core::policy::a_read_and_a_write_leave_different_trails`
`verified-by: bravebot_agent::turn::a_model_cannot_escape_the_workspace`
`verified-by: bravebot_agent::turn::a_model_cannot_escape_the_workspace_with_a_picture`
`verified-by: bravebot_agent::workspace::only_a_dropped_attachment_may_come_from_outside_the_workspace`
`verified-by: bravebot_agent::workspace::an_attachment_inside_an_added_directory_is_readable`

<a id="READ-5"></a>
### READ-5: a picture is quarantined whatever the trust map says, and only a processor looks at it

A file whose extension names a picture or a PDF is read as bytes, encoded into a `data:` URI, and
handed back as a reference. The planner is never shown one, and a vouched-for directory does not
change that: what the trust map answers is whether a file's **text** may be read, and a picture has
none. Nothing offers to vouch for one either, because a yes would grant the reading of text there
is none of.

The reference says what kind of thing it is rather than how many lines it has, because a line count
over base64 describes nothing a reader can act on. Given to `spawn_processor`, it reaches the
processor as a picture in its own part of the request, so the model looks at it rather than reading
base64 as words. The answer is quarantined like any other processor's, per
[PROC-5](../processors.md#PROC-5).

**Why the trust map does not decide this.** A screenshot carries whatever words are in it, and a
picture reaching a planner's context is exactly what [PASTE-2](../pasting.md#PASTE-2) restricts to
pictures a person put there themselves. That clause names the case directly: never an image a path
in model output named. This is that path, so the picture goes where untrusted content goes, and the
one component that may read untrusted content is the one that looks at it.

**The media type is the driver's.** From a closed table of extensions, shared with the one a drop
uses, never sniffed from the bytes. It ends up in the `data:` URI where it is routing, so deciding
it from content would be deciding a destination from content. A file cannot become a picture by
holding something that looks like one, and a picture cannot become text by being called `.txt`:
either way the extension is what was decided from, and it is part of a path a person can read.

`verified-by: bravebot_agent::turn::a_picture_is_never_shown_to_the_planner`
`verified-by: bravebot_agent::turn::a_picture_is_not_offered_for_vouching`
`verified-by: bravebot_agent::turn::a_processor_is_given_a_picture_as_a_picture`
`verified-by: bravebot_agent::workspace::the_media_type_comes_from_the_extension`
`verified-by: bravebot_agent::workspace::a_file_that_names_no_picture_is_not_one`

<a id="READ-6"></a>
### READ-6: the tool's own description says that a read settles no question about change

`read_file`'s description must say that a read is a sample of one moment, carrying no modification
time and no hash, so no pair of reads settles whether a file changed: two of them cannot tell a file
nobody touched from one changed twice and changed back, and neither says when anything happened.
Where that is the question, the description must send the planner to watching the file rather than
reading it, and must say what watching is, in both of its lengths. Inside this turn it is a watcher
started in the background and then one bounded wait covering a window, which is
[run.md](run.md). Past the end of the turn there is nothing the planner can start, so the description
must have it say so instead of answering from a read: only a loop outlives a turn, and a loop is a
person's to start, by typing `/loop`, which is [loop.md](../loop.md). Either way it must say to report
a sample as a sample: which window was watched, and where nothing is watching the file, that too.

**And it must not offer a comparison the planner cannot make.** Nothing in the description may have
the planner read a modification time or a hash in order to compare it with a later one. Neither of
those programs is among the audited few whose output may be read, so what they print comes back as a
reference the planner never sees, and a description recommending that comparison describes work that
cannot be done. The failure it invites is the one this clause exists to stop: a turn that reports a
comparison it never made.

**Why a clause about wording.** A tool's description is the only instruction the planner reliably
reads, so wording that decides what a turn does is behaviour and belongs in a spec;
[command-line.md](command-line.md) states that generally. The case here is that a read looks like an
answer. Asked when a file last changed, a session read it, described what it held, and left nothing
watching; asked again, it read again and said nothing had changed. Every step of that is a correct use
of this tool, which is why the correction has to be in the sentence the planner reads before it picks
the tool.

`verified-by: bravebot_agent::tools::read_file_says_one_read_cannot_answer_whether_something_changed`
