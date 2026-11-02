---
id: NAME
title: Naming a file with `@`
status: normative
governs:
  - crates/mentions/src/lib.rs
  - crates/tui/src/state.rs
  - crates/tui/src/state/attachments.rs
  - crates/tui/src/state/completion.rs
  - crates/cli/src/main.rs
  - crates/cli/src/invocation.rs
  - crates/cli/src/flags.rs
  - crates/cli/src/workspace.rs
  - crates/ui-bridge/src/mentions.rs
  - crates/agent/src/named.rs
  - crates/session/src/excerpt.rs
documented-by: docs/website/docs/using/context.md
---

## Scope

Writing `@src/main.rs` in a prompt, and `--file` on the command line: what each puts into the turn
and what it vouches for.

Three gestures put content into a turn on the user's own footing, and each has its own spec:
[naming-files.md](naming-files.md) for `@` in a prompt, [pasting.md](pasting.md) for Ctrl-V, and
[dropping.md](dropping.md) for a file dragged onto the window.

## Why a named file is trusted

Because the user typed the path and sending the line is the grant. Nothing a model said chose the
file, and nothing inspects the contents.

Trusted is the point here rather than a detail. The planner may read those contents, compare them,
and act on what they say, which is exactly what a read of a file nobody vouched for withholds. So
name a file when you want it worked on.

## Clauses

<a id="NAME-1"></a>
### NAME-1: a named file's contents enter the turn as trusted input

`@path` in a prompt and `--file` on the command line do the same thing and are trusted for the same
reason.

`verified-by: bravebot_agent::turn::a_turn_includes_requested_file_contents`
`verified-by: bravebot_agent::turn::a_turn_includes_referenced_file_contents`
`verified-by: bravebot_agent::turn::a_referenced_file_is_trusted_though_the_workspace_is_not`

<a id="NAME-2"></a>
### NAME-2: the rule is for that file, and nothing beside it

A rule on a file is more specific than any rule on the tree around it, so `@vendor/lib.js` is
trusted inside a `vendor` marked untrusted, and the rest of that directory stays exactly as it was.

`verified-by: bravebot_core::policy::naming_a_file_vouches_for_nothing_beside_it`
`verified-by: bravebot_core::policy::a_named_file_is_trusted_inside_an_untrusted_tree`
`verified-by: bravebot_agent::turn::naming_one_file_leaves_the_rest_of_the_workspace_quarantined`
`verified-by: bravebot_agent::turn::a_file_named_with_an_at_sign_is_vouched_for_alone_whatever_its_name`

<a id="NAME-3"></a>
### NAME-3: the rule outlives the read

The file can be edited afterwards, which is usually the point of naming it. Naming one also works
in a directory declined at startup.

`verified-by: bravebot_agent::turn::a_named_file_is_still_trusted_after_it_is_edited`
`verified-by: bravebot_agent::turn::a_referenced_file_is_trusted_though_the_workspace_is_not`

<a id="NAME-4"></a>
### NAME-4: typing `@` offers what is in the workspace, so the choice is informed

The list opens on the root with directories first, a prefix narrows it, a slash descends, a
fragment is also looked for below the directory it is typed in (NAME-11), and version-control and
build directories are not offered, nor is a name that is not valid UTF-8, since a line written from
it would name another file. Tab completes without disturbing the rest of the sentence, a
directory completes so typing can continue into it, and the arrows and Enter choose among what is
offered.

**Why.** A path typed blind is a path the user did not really choose.

`verified-by: bravebot_tui::references::an_at_sign_offers_the_workspace`
`verified-by: bravebot_tui::references::tab_completes_a_reference_without_disturbing_the_sentence`
`verified-by: bravebot_tui::references::a_name_holding_an_at_sign_completes_to_itself`
`verified-by: bravebot_tui::references::a_directory_completes_so_typing_can_continue_into_it`
`verified-by: bravebot_tui::references::the_arrows_and_enter_choose_among_the_offered_files`
`verified-by: bravebot_tui::references::a_finished_reference_closes_the_list`
`verified-by: bravebot_mentions::lib::an_empty_reference_lists_the_root_with_directories_first`
`verified-by: bravebot_mentions::lib::a_prefix_narrows_the_list`
`verified-by: bravebot_mentions::lib::a_slash_lists_what_is_inside_that_directory`
`verified-by: bravebot_mentions::lib::noise_directories_are_not_offered`
`verified-by: bravebot_mentions::lib::a_name_that_is_not_text_is_told_apart_from_its_lookalike`
`verified-by: bravebot_mentions::lib::a_name_that_is_not_text_is_not_offered_under_its_lookalike`
`verified-by: bravebot_tui::references::a_paste_returns_the_cursor_to_the_top_of_the_narrowed_list`

<a id="NAME-5"></a>
### NAME-5: a name cannot leave the workspace

`..` and an absolute path are refused rather than resolved, so a named file is always inside the
working directory or a directory opened for the session.

`verified-by: bravebot_tui::references::a_reference_cannot_climb_out_of_the_workspace`
`verified-by: bravebot_mentions::lib::a_reference_cannot_climb_out_of_the_workspace`
`verified-by: bravebot_mentions::lib::dots_and_an_absolute_path_are_not_finished_names`
`verified-by: bravebot_agent::turn::a_named_file_outside_the_workspace_is_refused_and_never_sent`

<a id="NAME-6"></a>
### NAME-6: a directory names nothing, and neither does prose

A directory is somewhere to type through rather than a file to read, so naming one includes
nothing. An address inside a sentence is not a reference, and a bare `@` names nothing.

**Why.** Including a file because somebody wrote something that looked like a path would put
content into the turn that no gesture chose.

`verified-by: bravebot_tui::references::a_directory_reference_is_not_included_as_a_file`
`verified-by: bravebot_tui::references::an_address_in_a_sentence_is_not_a_reference`
`verified-by: bravebot_mentions::lib::a_directory_is_not_collected_as_a_file`
`verified-by: bravebot_mentions::lib::a_bare_at_sign_names_nothing`
`verified-by: bravebot_mentions::lib::an_address_inside_a_sentence_is_not_collected_as_a_reference`
`verified-by: bravebot_mentions::lib::what_counts_as_a_reference_being_typed`
`verified-by: bravebot_mentions::lib::every_referenced_file_is_collected`

<a id="NAME-7"></a>
### NAME-7: sending finishes a half-typed name

Enter sends a prompt ending in a finished reference, and completes one ending in a half-typed
reference rather than sending the fragment.

`verified-by: bravebot_tui::references::enter_sends_a_prompt_that_ends_in_a_finished_reference`
`verified-by: bravebot_tui::references::enter_completes_a_prompt_that_ends_in_a_half_typed_reference`
`verified-by: bravebot_tui::references::enter_sends_a_finished_reference_a_directory_shares_a_prefix_with`
`verified-by: bravebot_tui::references::enter_sends_a_finished_reference_the_offered_list_is_too_short_to_show`
`verified-by: bravebot_mentions::lib::what_counts_as_already_naming_a_file`
`verified-by: bravebot_mentions::lib::a_symlink_is_a_finished_name_because_the_list_offers_it_as_one`
`verified-by: bravebot_mentions::lib::what_enter_does_with_a_half_typed_or_finished_name`
`verified-by: bravebot_tui::references::the_arrows_still_choose_a_row_over_a_finished_reference`
`verified-by: bravebot_tui::references::the_files_a_submitted_line_would_include`
`verified-by: bravebot_tui::references::a_cursor_past_the_end_of_a_narrowed_list_still_names_a_file`
`verified-by: bravebot_tui::references::a_paste_returns_the_cursor_to_the_top_so_enter_sends_a_finished_reference`
`verified-by: bravebot_tui::state::a_recalled_prompt_returns_the_cursor_to_the_top`

<a id="NAME-8"></a>
### NAME-8: a backslash keeps the whitespace or backslash after it in the name

`@My\ Documents/notes.md` names the file `My Documents/notes.md`: a whitespace character written
after a backslash belongs to the reference, and any whitespace written without one ends it. A
backslash written after a backslash is one backslash of the name, so a name ending in a backslash is
written with two. A backslash before anything else, a line break included, is an ordinary character,
so prose containing one still names nothing and a line ending in a backslash does not take the first
word of the next line into its reference. Completing an entry whose name holds whitespace or a
backslash writes each behind a backslash, so the line reads back as the path that was chosen. NAME-5
applies to the unescaped path.

The list does not offer a name the line cannot carry to the turn: a file or directory whose name
holds a line break (U+000A, U+000D, U+2028 or U+2029), and a file whose name ends in whitespace or
U+FEFF, because a line is trimmed when it is sent and the name would lose its end. `--file` names
such a file.

**Why.** Without the escaped form, a file whose name holds whitespace cannot be named, and the
picker would offer an entry that completes to text naming a different path. An entry no line can
write has the same result, so it is left out. Where a reference ends is decided from the line the
user typed, not from file contents.

`verified-by: bravebot_mentions::lib::a_backslash_before_a_space_continues_a_reference`
`verified-by: bravebot_mentions::lib::a_backslash_before_whitespace_or_a_backslash_keeps_it_in_the_name`
`verified-by: bravebot_mentions::lib::an_escaped_path_reads_back_unchanged`
`verified-by: bravebot_mentions::lib::a_name_with_a_space_is_listed_and_finished`
`verified-by: bravebot_mentions::lib::every_awkward_name_is_offered_and_read_back_as_the_file_it_is`
`verified-by: bravebot_mentions::lib::a_pick_among_names_that_look_alike_names_the_one_picked`
`verified-by: bravebot_mentions::lib::a_name_a_line_cannot_carry_is_not_offered`
`verified-by: bravebot_tui::references::a_name_with_a_space_completes_to_a_reference_that_names_it`
`verified-by: bravebot_tui::references::a_name_with_a_no_break_space_completes_to_a_reference_that_names_it`
`verified-by: bravebot_tui::references::a_name_ending_in_a_backslash_completes_to_a_reference_that_names_it`
`verified-by: bravebot_tui::references::a_name_ending_in_a_no_break_space_is_not_offered_beside_the_name_it_looks_like`
`verified-by: bravebot_tui::references::enter_completes_a_half_typed_reference_past_an_escaped_space`

<a id="NAME-9"></a>
### NAME-9: the desktop message box names a file on the same terms

The clauses above are worded for the terminal, and the desktop app's message box keeps each of them,
because both front ends call the same `bravebot-mentions` code: the same list (NAME-4) with the
names found below it (NAME-11), `..`, an absolute path and a link leading out of the workspace
refused (NAME-5), a name ending in `/`, a bare `@` and an address in a sentence naming nothing
(NAME-6), and Enter completing a half-typed name and sending a finished one (NAME-7), and a
backslash before whitespace or a backslash keeping it in a name (NAME-8). The bridge reads the names
back out of the prompt at `turn.send` rather than taking a list from the window, so the files that
go are the ones the sent line names. Each is surveyed by the read the turn will make, and a name that
would fail it (outside the workspace, missing, a directory, or not text) refuses the send before a
turn starts, on the same verdict of the survey as in the terminal (NAME-12). The project is the folder
the conversation runs in, which for a bot's conversation is the bot's home folder or the project
chosen for it. A prompt
the app composes itself names no file. A list or a name asked for while a turn runs does not wait
for the turn: bridge requests are read on one thread, and the turn may be waiting on an answer
queued behind the request, so a turn holding the session's state leaves out the directories the
session added, as a poisoned lock does.

**Why.** One person names files in both front ends, and a name the window treated differently from
the terminal would send a file nobody chose or drop one somebody did.

`verified-by: bravebot_ui_bridge::mentions::the_window_is_offered_the_terminals_list_of_the_project`
`verified-by: bravebot_ui_bridge::mentions::an_entry_with_a_space_is_offered_escaped_over_the_whole_word`
`verified-by: bravebot_ui_bridge::mentions::an_entry_holding_a_no_break_space_is_written_so_the_prompt_names_it`
`verified-by: bravebot_ui_bridge::mentions::a_fragment_is_offered_the_file_below_it_and_enter_completes_it`
`verified-by: bravebot_ui_bridge::mentions::a_prompt_names_only_text_files_inside_the_project`
`verified-by: bravebot_ui_bridge::mentions::a_send_naming_a_file_that_cannot_go_is_refused_before_the_turn`
`verified-by: bravebot_ui_bridge::mentions_running_tests::offering_rows_does_not_wait_for_a_turn_holding_the_session`
`verified-by: bravebot_ui_bridge::mentions_running_tests::naming_a_file_does_not_wait_for_a_turn_holding_the_session`
`verified-by: by-construction (the window's half is not a crate this workspace compiles, so ui/scripts/drive-at-mentions.mjs drives the real app and bridge against a stub model and asserts the list, a fragment finding a file in a directory below with Tab writing its whole path, its keys, Tab into a directory, Enter completing a half-typed name, a directory holding a space completed to the escaped text and Enter completing a name already holding an escaped space (ui/scripts/mentions.test.mjs pins the splice), the file's contents in the model request, the Read row, the refusal of @../outside.txt at send, and that a bot's conversation with no project lists and reads the bot's home folder and refuses a project file)`

<a id="NAME-10"></a>
### NAME-10: `@session:<id>` adds the newest part of an earlier session to the message

After the files, the `@` list offers the other sessions of this directory, each with its title, its
age and the most a mention adds. A title or an id narrows them, and choosing one writes
`@session:<id>`, which is no path: a file is never read for it. Sending the line adds to the
person's own message the typed prompts and the planner's replies of that session, newest turn
first, and stops at 8,000 characters, so the oldest turns are the ones left out, and the newest is
cut rather than dropped if it alone is longer. Tool results and file contents are not in a record,
so the excerpt says they are not included. The transcript says how many characters were added, and
the trail records the session id and that count and none of the words. `--session-ref <id>` does the
same from the command line, and an id that cannot be quoted refuses the run before anything is sent.
In a session the same id leaves the turn without the excerpt and says why in the transcript.

A manifest run, a session with no words in it, a record another program wrote or a front end this
build does not know, and a session whose planner had been shown private content are not quoted. Only
a line the person wrote is read for mentions, so a prompt the driver composed adds nothing. The
desktop message box does not offer sessions and sends the line as typed.

**Why.** A person who wants something from an older conversation otherwise has to resume all of it.
The record holds only what the planner was allowed to hold ([sessions.md](sessions.md)), and the person chose the
session by typing its name, so its words are trusted the way a named file is. A session whose
planner had been shown private content is left out because the excerpt would carry that content
into a turn whose confidentiality the old session never set. The bound is fixed and shown on the
row so the cost is known before sending.

`verified-by: bravebot_session::excerpt::an_excerpt_holds_the_prompts_and_replies_newest_turn_first`
`verified-by: bravebot_session::excerpt::an_excerpt_is_not_quoted_inside_the_next_one`
`verified-by: bravebot_session::excerpt::a_long_session_is_cut_at_the_bound_from_the_oldest_turn`
`verified-by: bravebot_session::excerpt::one_turn_longer_than_the_bound_is_cut_rather_than_dropped`
`verified-by: bravebot_session::excerpt::a_session_that_used_tools_says_their_results_are_not_included`
`verified-by: bravebot_session::excerpt::a_record_that_is_not_the_planners_to_repeat_is_refused`
`verified-by: bravebot_session::excerpt::an_id_that_is_not_a_session_name_is_not_looked_up`
`verified-by: bravebot_mentions::lib::a_session_reference_is_not_a_file_and_is_read_back_by_its_id`
`verified-by: bravebot_mentions::lib::a_session_row_completes_to_its_reference_and_enter_sends_it_finished`
`verified-by: bravebot_tui::references::an_at_sign_offers_past_sessions_after_the_files`
`verified-by: bravebot_tui::references::a_word_narrows_sessions_by_title_and_not_by_what_is_drawn_beside_them`
`verified-by: bravebot_tui::references::the_sessions_are_read_only_while_a_reference_is_typed`
`verified-by: bravebot_tui::references::tab_completes_a_session_and_enter_sends_it_finished`
`verified-by: bravebot_tui::render::a_session_row_carries_its_title_and_a_file_row_does_not`
`verified-by: bravebot_tui::app::naming_a_past_session_adds_its_words_only_from_a_line_the_person_wrote`
`verified-by: bravebot_tui::app::the_sessions_an_at_offers_leave_out_the_open_one`
`verified-by: bravebot_agent::turn::a_named_session_is_added_to_the_message_and_the_trail_holds_only_its_size`
`verified-by: bravebot_cli::main::a_session_ref_flag_names_a_session_once`
`verified-by: bravebot_cli::running::a_session_ref_brings_an_earlier_session_into_a_new_run`

<a id="NAME-11"></a>
### NAME-11: a fragment is looked for in every directory below, so a file is found without its directory

A fragment with no slash is looked for in every directory of the workspace as well as in the one it
is typed in, and so is a fragment with a slash where the directory it names offers nothing. A name
matches when its characters hold those of the fragment in order, without regard to case, and a
directory is a name like a file. Each match is a row with its path from the workspace root, and
choosing one completes to that whole path as a row of the directory's own does, so what the line
names, and what the trust rule is applied to, is the path the person was shown. Enter decides a
finished name from the workspace (NAME-7), never from the rows offered.

The rows come after what the directory offers, at most ten of them, and the list holds at most 40
rows, so what the directory offers leaves less room for them. They are ordered best first: a name that matches before a
name whose path matches, then a match that starts the text before one inside it before one with
other characters between, then the shorter path, then the alphabetical one. An empty fragment and a
name under a reference directory's alias are answered by the one directory alone.

The walk starts at the root and goes one level at a time, reading at most 10,000 names. It offers
only the names NAME-4 offers, steps over `.claude/worktrees` as well as the directories NAME-4
leaves out, and follows no link: a link is a name of its own, as the directory's list offers it,
and nothing inside a linked directory is (NAME-5). A name made since the last key is found. Both
front ends offer these rows (NAME-9).

**Why.** A person who remembers part of a name and not its directory otherwise has to walk the tree
by hand. The rows are names shown to the person typing, the release the list already makes, and none
reaches the planner before the line is sent, so nothing is decided from what a file holds. The bound
keeps a very large tree from stalling a key, and going one level at a time leaves out the deepest
names rather than the nearest. Ten rows are what a person chooses from: a short fragment has its
letters in the path of most of a large tree, and the terminal draws every row under the box.

`verified-by: bravebot_mentions::lib::a_bare_fragment_finds_a_name_in_a_directory_below`
`verified-by: bravebot_mentions::lib::a_fragment_matches_a_name_in_every_script_without_regard_to_case`
`verified-by: bravebot_mentions::lib::a_bare_fragment_finds_a_directory_below_and_offers_it_as_one`
`verified-by: bravebot_mentions::lib::a_fragment_with_a_slash_is_looked_for_below_only_where_its_directory_offers_nothing`
`verified-by: bravebot_mentions::lib::a_name_ranks_before_a_path_and_a_closer_match_before_a_looser_one`
`verified-by: bravebot_mentions::lib::what_a_directory_offers_comes_first_and_is_not_repeated_below`
`verified-by: bravebot_mentions::lib::no_more_than_ten_names_found_below_follow_what_the_directory_offers`
`verified-by: bravebot_mentions::lib::the_list_never_holds_more_than_forty_rows`
`verified-by: bravebot_mentions::lib::the_names_every_walk_skips_are_skipped_at_every_depth`
`verified-by: bravebot_mentions::lib::the_walk_follows_no_link_out_of_the_workspace_or_back_into_it`
`verified-by: bravebot_mentions::lib::the_walk_stops_at_its_bound_having_read_the_shallow_names_first`
`verified-by: bravebot_mentions::lib::a_name_made_after_a_walk_is_found_by_the_next`
`verified-by: bravebot_mentions::lib::the_directories_a_walk_read_are_kept_for_the_next_key`
`verified-by: bravebot_mentions::lib::a_name_with_a_space_found_below_reads_back_as_that_file`
`verified-by: bravebot_mentions::lib::a_name_under_an_alias_is_not_looked_for_in_the_workspace`
`verified-by: bravebot_mentions::lib::enter_sends_a_finished_name_and_completes_a_fragment_to_a_found_row`
`verified-by: bravebot_tui::references::a_fragment_finds_a_file_below_and_tab_writes_the_whole_path`
`verified-by: bravebot_tui::references::enter_completes_a_fragment_to_the_file_found_below_and_names_that_file`
`verified-by: bravebot_ui_bridge::mentions::a_fragment_is_offered_the_file_below_it_and_enter_completes_it`

<a id="NAME-12"></a>
### NAME-12: a name that cannot be sent is refused where Enter is pressed

Before a line is sent or queued, the terminal surveys each `@name` it holds, in the line the person
wrote with a folded paste unfolded, and the desktop app does the same at `turn.send` (NAME-9). A
name under a reference directory's alias is surveyed in the directory it resolves to. A name that
is not a text file in the project (missing, a directory written without its slash, binary, outside
the workspace, or under a reference directory whose path has no text spelling) refuses the send.
A name that is a file the program is not allowed to read, or that the disk fails to read, refuses
it too, with a sentence saying it could not be read.
The refusal is one line for each such name, naming the `@name` and saying to remove the `@` to send
it as text, and the line stays in the box as typed, with its paste markers and attachments. Both
front ends take the sentences from the same catalog entries through `bravebot_agent::named`.

A line that names only text files, a directory with its slash, a bare `@` or an address in a
sentence is sent as before (NAME-6). A line this program composed names no file and is never
refused (NAME-1). A rule is recorded for a named file only when the survey passes, so a name the
read refuses leaves nothing in the trust map (NAME-2).

Whether a name is refused depends on the verdict of the survey: the path, the kind of file, and
whether the first 8 KiB of the file is text or binary under `decode_text`, which honours a byte
order mark and decodes UTF-8. [labels.md](labels.md) lists that check as the seventh place that
looks at untrusted bytes. `Workspace::survey` takes the verdict and `bravebot_agent::named::surveyed`
asks it for each name. The driver uses only the verdict and does not read the contents. The name and
the verdict never reach the planner. The person's screen, the session record and the trust map's
vouching depend on them.

**Why.** A send that is not refused leaves a trusted rule for a path that does not exist, and ends
the turn on a workspace failure after the prompt is recorded. A pasted request in it cannot be got
back.

`verified-by: bravebot_tui::references::enter_refuses_a_name_that_is_not_a_file_and_keeps_the_line`
`verified-by: bravebot_tui::references::enter_refuses_a_directory_a_binary_file_and_a_path_outside_the_project`
`verified-by: bravebot_tui::references::the_refusal_names_each_bad_name_and_not_the_good_one`
`verified-by: bravebot_tui::references::a_refused_line_keeps_its_folded_paste`
`verified-by: bravebot_tui::references::a_handle_inside_a_folded_paste_is_refused_and_the_marker_stays`
`verified-by: bravebot_tui::references::a_refused_line_keeps_its_attachments`
`verified-by: bravebot_tui::references::a_name_under_a_reference_alias_is_surveyed_in_its_directory`
`verified-by: bravebot_tui::references::a_prompt_the_driver_composed_is_never_surveyed`
`verified-by: bravebot_tui::references::enter_still_sends_lines_that_name_real_files_or_nothing`
`verified-by: bravebot_tui::references::a_session_with_no_workspace_surveys_nothing`
`verified-by: bravebot_tui::references::while_a_turn_works_a_bad_name_is_refused_and_not_queued`
`verified-by: bravebot_agent::workspace::surveying_a_lines_names_answers_each_in_order_through_the_resolver`
`verified-by: bravebot_tui::render::transcript::a_refusal_note_quoting_a_name_with_an_escape_is_drawn_with_the_escape_made_visible`
`verified-by: bravebot_agent::turn::a_missing_named_file_leaves_no_rule_in_the_trust_map`
`verified-by: bravebot_ui_bridge::mentions::a_send_naming_a_file_that_cannot_go_is_refused_before_the_turn`

<a id="NAME-13"></a>
### NAME-13: a turn that ends on the read of a named file says which file and why

A name that passed the survey (NAME-12) can still fail when the turn reads it: the file was removed
or replaced after Enter, a file in another encoding is read by the turn and not by the survey, or a
`--file` or a front end other than the two above handed over a name nobody surveyed. The turn then
ends on that read, and the failure line it shows, and the reason stored in the session record, name
the `@name` as it was written and give the reason in the sentences NAME-12 uses (not a file in this
project, not a text file, text in an encoding other than UTF-8, or a file that could not be read)
taken from the same catalog
entries through `bravebot_agent::named`. Where the line names several files, the one whose read
failed is the one named. A failed read of anything else, or a failure that is not a read of a named
file, keeps the sentence for its category.

The name reaches only the line the person is shown and the record of the turn. The note the planner
receives about a failed turn is made from the category and the request count
([turns](turns.md#TURN-13)) and holds no
name, and the name is not a field of the diagnosis. Which sentence is chosen depends on the kind of
failure the read reported, which for a binary file or text in another encoding is the verdict of
`Workspace::survey` or of the decoding in the turn's read (NAME-12). `bravebot_agent::named` maps
the kind to a sentence, and the driver does not read the contents. The name and the sentence never
reach the planner. The name is made printable where the line is drawn, since it is text the person
typed or pasted.

**Why.** A failure line that says only that the workspace could not be used does not say which of
several files failed, or what is wrong with it.

`verified-by: bravebot_tui::sessions::a_named_file_that_cannot_be_read_is_named_with_its_reason_in_the_failure_line`
`verified-by: bravebot_tui::sessions::the_failure_line_names_the_file_that_failed_among_several`
`verified-by: bravebot_agent::turn::the_note_and_the_conversation_of_a_turn_that_failed_on_a_named_file_hold_no_name`
`verified-by: bravebot_agent::turn::a_failed_read_of_a_dropped_file_or_an_attachment_carries_no_name`
`verified-by: bravebot_agent::workspace::a_named_file_the_process_may_not_read_is_unreadable_and_not_a_missing_file`
`verified-by: bravebot_tui::render::transcript::a_failure_line_quoting_a_name_with_an_escape_is_drawn_with_the_escape_made_visible`
`verified-by: bravebot_ui_bridge::failure_tests::a_failure_on_a_named_file_is_sent_as_the_workspace_kind_with_no_name`

## Known costs

- **Content you have not read is content you are vouching for.** Be as careful naming a file as
  answering yes to a directory. The planner will act on what it says.
- **A tree of more than 10,000 names is searched only as far as the bound.** The walk goes one
  level at a time, so a fragment misses the deepest names of such a tree. Typing the path to one
  still lists it, since its own directory answers.
- **A directory is read whole before the walk takes names from it.** A directory of many thousands
  of names costs one read when the walk first reaches it, and can use the whole bound, which leaves
  the directories after it unread.
- **A fragment is looked for in the workspace, not in a reference directory.** A name under the alias
  of a reference directory ([references.md](references.md)) is found by typing its path from the
  alias.
- **A name behind a link to a directory is not found by a fragment.** The walk follows no link, so
  `docs` linked to `../docs` is a row of its own and its files are reached by typing through it,
  where NAME-5 decides whether the link leads outside the workspace.
- **Rows found below come before the sessions NAME-10 offers.** A word that is part of a session's
  title also matches file names, so the session rows follow those. `@session:` narrows to the
  sessions.
- **A file whose name holds a line break or ends in whitespace cannot be named with `@`.** The list
  leaves it out (NAME-8), so naming it takes `--file`. A line cannot hold the break, and the trim
  made of a line when it is sent would take the end of the name. A name typed by hand with its
  trailing whitespace escaped is read as written, and the trim leaves the backslash, so the line
  names the name with a backslash at its end.
