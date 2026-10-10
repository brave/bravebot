---
id: NAME
title: Naming a file with `@`
status: normative
governs:
  - crates/mentions/src/lib.rs
  - crates/tui/src/state.rs
  - crates/cli/src/main.rs
  - crates/ui-bridge/src/mentions.rs
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

<a id="NAME-3"></a>
### NAME-3: the rule outlives the read

The file can be edited afterwards, which is usually the point of naming it. Naming one also works
in a directory declined at startup.

`verified-by: bravebot_agent::turn::a_named_file_is_still_trusted_after_it_is_edited`
`verified-by: bravebot_agent::turn::a_referenced_file_is_trusted_though_the_workspace_is_not`

<a id="NAME-4"></a>
### NAME-4: typing `@` offers what is in the workspace, so the choice is informed

The list opens on the root with directories first, a prefix narrows it, a slash descends, and
version-control and build directories are not offered. Tab completes without disturbing the rest of
the sentence, a directory completes so typing can continue into it, and the arrows and Enter choose
among what is offered.

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
`verified-by: bravebot_tui::references::a_paste_returns_the_cursor_to_the_top_of_the_narrowed_list`

<a id="NAME-5"></a>
### NAME-5: a name cannot leave the workspace

`..` and an absolute path are refused rather than resolved, so a named file is always inside the
working directory or a directory opened for the session.

`verified-by: bravebot_tui::references::a_reference_cannot_climb_out_of_the_workspace`
`verified-by: bravebot_mentions::lib::a_reference_cannot_climb_out_of_the_workspace`
`verified-by: bravebot_mentions::lib::dots_and_an_absolute_path_are_not_finished_names`

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
### NAME-8: a backslash before a space keeps the space in the name

`@My\ Documents/notes.md` names the file `My Documents/notes.md`: a space written after a backslash
belongs to the reference, and any other whitespace ends it. A backslash anywhere else is an ordinary
character, so prose containing one still names nothing. Completing an entry whose name holds a space
writes the escaped form, so the line reads back as the path that was chosen. NAME-5 applies to the
unescaped path.

**Why.** Without the escaped form, a file whose name holds a space cannot be named, and the picker
would offer an entry that completes to text naming a different path. Where a reference ends is
decided from the line the user typed, not from file contents.

`verified-by: bravebot_mentions::lib::a_backslash_before_a_space_continues_a_reference`
`verified-by: bravebot_mentions::lib::an_escaped_path_reads_back_unchanged`
`verified-by: bravebot_mentions::lib::a_name_with_a_space_is_listed_and_finished`
`verified-by: bravebot_tui::references::a_name_with_a_space_completes_to_a_reference_that_names_it`
`verified-by: bravebot_tui::references::enter_completes_a_half_typed_reference_past_an_escaped_space`

<a id="NAME-9"></a>
### NAME-9: the desktop message box names a file on the same terms

The clauses above are worded for the terminal, and the desktop app's message box keeps each of
them, because both front ends call the same `bravebot-mentions` code: the same list (NAME-4), `..`,
an absolute path and a link leading out of the workspace refused (NAME-5), a name ending in `/`, a
bare `@` and an address in a sentence naming nothing (NAME-6), and Enter completing a half-typed
name and sending a finished one (NAME-7), and a backslash before a space keeping the space in a name
(NAME-8). The bridge reads the names back out of the prompt at
`turn.send` rather than taking a list from the window, so the files that go are the ones the sent
line names. Each is surveyed by the read the turn will make, and a name that would fail it (outside
the workspace, missing, a directory, or not text) refuses the send before a turn starts, where the
terminal would end the turn. The project is the folder the conversation runs in, which for a bot's
conversation is the bot's home folder or the project chosen for it. A prompt the app composes
itself names no file.

**Why.** One person names files in both front ends, and a name the window treated differently from
the terminal would send a file nobody chose or drop one somebody did.

`verified-by: bravebot_ui_bridge::mentions::the_window_is_offered_the_terminals_list_of_the_project`
`verified-by: bravebot_ui_bridge::mentions::a_prompt_names_only_text_files_inside_the_project`
`verified-by: bravebot_ui_bridge::mentions::a_send_naming_a_file_that_cannot_go_is_refused_before_the_turn`
`verified-by: by-construction (the window's half is not a crate this workspace compiles, so ui/scripts/drive-at-mentions.mjs drives the real app and bridge against a stub model and asserts the list, its keys, Tab into a directory, Enter completing a half-typed name, the file's contents in the model request, the Read row, the refusal of @../outside.txt at send, and that a bot's conversation with no project lists and reads the bot's home folder and refuses a project file)`

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

## Known costs

- **Content you have not read is content you are vouching for.** Be as careful naming a file as
  answering yes to a directory. The planner will act on what it says.
