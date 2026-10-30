---
id: SCREEN
title: The Sessions screen
status: normative
governs:
  - crates/session/src/listing.rs
  - crates/tui/src/sessions_screen.rs
  - crates/tui/src/sessions_screen/worker.rs
  - crates/tui/src/sessions_keys.rs
  - crates/tui/src/state/sessions_screen.rs
  - crates/tui/src/render/sessions_screen.rs
  - crates/tui/src/app/sessions_screen.rs
  - crates/cli/src/reopen.rs
documented-by:
  - docs/website/docs/using/sessions-screen.md
---

## Scope

One list of every session on the machine, opened from the plain prompt and while a turn runs, and by
`--resume`, `--from-pr` and `/resume`: what each session is, what its process is doing, how the list is grouped, filtered and searched, and what
the keys on a row do. The rows are the session records of every project, joined to the roster of
background sessions ([background-sessions.md](background-sessions.md)) and to the lock each record
carries ([sessions.md](sessions.md)).

The **Sessions screen** lists a row for each session record, and for each running background session
whose record is not yet written. The **cursor** is the row the keys act on.

What a label is and who may read what is [labels.md](labels.md). The list draws what
[BG-5](background-sessions.md#BG-5) allows a list to draw, and the same reasons hold here. The
delegates and asides of one session are not sessions, and are opened by [watching.md](watching.md).

## What exists today

`bravebot sessions` opens a full-screen list of the background sessions, grouped by state, from which
Enter attaches ([BG-16](background-sessions.md#BG-16)). It is made from the same listing as this screen
([SCREEN-16](#SCREEN-16)); it lists neither a session that is open in a terminal nor a saved one, and
cannot name, copy, stop, archive or delete a session. `--resume`, `--from-pr` and `/resume` open this
screen, which lists every directory, searches what was said in the sessions and gives the facts of
each session ([SCREEN-13](#SCREEN-13)).

## Clauses

<a id="SCREEN-1"></a>
### SCREEN-1: Left over an empty box opens the Sessions screen, and it takes every key until it is left

Left with no modifier opens the Sessions screen when the box holds nothing, so it has nowhere to move the
caret. `/sessions` opens it with the same result. Both work at rest and while a turn runs. They do
nothing while a command the person typed is running, while a list of commands or files is offered,
while the line is a command for the shell, while a brief waits to be sent, while a vi instruction is
half typed, and for a press that arrived in a run of keys read as one paste. Left held with Ctrl, Alt
or Shift is the caret's, and so is Left with anything typed.

While the Sessions screen is open every key is the Sessions screen's: nothing reaches the box, a paste is not put in
the box, and the box and the transcript are not drawn. Escape leaves it, after clearing the search, a
line being renamed, a question or the help, in that order. Ctrl-C leaves it the same way and does not
stop a turn or end the session. The transcript, the box and the turn are as they were when it is
left. A loop's tick, a watch and a queued prompt do not start a turn while it is open at rest.

**Why.** Left is the one arrow with nothing to do over an empty line, so using it costs a person
nothing they use. A mode that took only some keys would leave a letter typed into a box nobody can
see.

`verified-by: bravebot_tui::sessions_screen_tests::left_at_the_empty_box_opens_the_sessions_screen`
`verified-by: bravebot_tui::sessions_screen_tests::left_with_text_in_the_box_moves_the_caret_and_opens_nothing`
`verified-by: bravebot_tui::sessions_screen_tests::a_chord_on_left_is_not_the_key_that_opens_it`
`verified-by: bravebot_tui::sessions_screen_tests::left_is_the_boxs_while_something_else_has_the_line`
`verified-by: bravebot_tui::sessions_screen_tests::left_opens_the_sessions_screen_while_a_turn_runs_and_only_over_an_empty_box`
`verified-by: bravebot_tui::sessions_screen_tests::left_in_a_run_of_keys_is_not_the_key_that_opens_it`
`verified-by: bravebot_tui::app::the_sessions_screen_opens_over_a_running_turn_and_leaves_it_running`
`verified-by: bravebot_tui::sessions_screen_tests::the_sessions_screen_command_opens_the_sessions_screen_as_the_key_does`
`verified-by: bravebot_tui::sessions_screen_tests::the_sessions_screen_command_is_offered_while_it_is_typed`
`verified-by: bravebot_tui::sessions_screen_tests::the_sessions_screen_takes_every_key_and_leaves_the_box_as_it_was`
`verified-by: bravebot_tui::sessions_screen_tests::ctrl_c_and_escape_leave_the_sessions_screen_and_do_not_quit`
`verified-by: bravebot_tui::render::sessions_screen::the_box_and_the_transcript_are_not_drawn_while_the_sessions_screen_is_open`
`verified-by: bravebot_tui::sessions_screen::escape_ends_the_search_and_then_the_sessions_screen`
`verified-by: bravebot_tui::sessions_screen::help_takes_every_key_until_it_is_closed`
`verified-by: bravebot_tui::sessions_screen_tests::escape_and_ctrl_c_over_a_running_turn_leave_the_sessions_screen_and_do_not_stop_it`

<a id="SCREEN-2"></a>
### SCREEN-2: a session has a closed set of fields, and none of them is content

A session holds its id, the directory it ran in, its name, its status, the kind of prompt held where one
is, when its record was last written and when it began, the model that answered, the branch, the
turns and the tokens it spent, the last prompt a person typed where it has a roster entry, the mode a
background session runs in, the agent definition it was started with, the issue and pull request a
person linked, the id of the session it was handed off from, what it takes up on disk, and whether it
has a roster entry, a process running it, this terminal or a manifest run. Each is something a person typed, a fact about a process or a file, or a word from
a fixed set. There is no field for a reply, a tool result, a file's contents, a command's output or
the text of a question, and the reader parses only the fields it names, so the text of a turn is
never taken from the record.

The one exception is the line a search shows. While a person has typed text, the row of a session whose
words hold it carries, under it, the first line of the session's words that does, cut around the
match ([SESSION-33](sessions.md#SESSION-33)). It is a line the transcript draws, from the index
beside the record, shown only while the search holds the text, and nothing else on the screen is
decided by it: it orders no row, sets no status and is read by no key.

The strings among them are pictured when a row is made, so a control character in a name, a
directory, a model, a branch or a prompt is drawn as its picture on the list, in the details and in a
question. A name is its first line.

The list is the sessions of every project, newest first, read from the records and the roster. The
first read takes the 40 most recently written records and says whether older ones exist; "Show more"
adds 40, and a search reads all of them, so a session is found however old it is. A background session with a process running is listed whatever its age and takes none of the
room. A running background session that has written no record yet, as one in its first turn or
holding its first question, is listed from its roster entry alone, and one whose process is gone and
that has no record is not listed, since there is nothing to open. A record that does not parse, one
whose name is not its id, one archived and not written since, and one hidden in this Sessions screen are left
out. The list is read again every two seconds while it is open, at rest and while a turn runs, and a
record whose file has not changed is not parsed again.

**Why.** The list is on the screen for as long as the Sessions screen is open, and a name drawn from a reply
or a tool would be read as the program's own sentence. A closed set whose sources are named is what
lets a reader of a diff check that nothing untrusted is among them. Parsing 40 records each time would
be the cost of every refresh.

`verified-by: bravebot_session::listing::sessions_of_every_project_are_listed_with_their_fields`
`verified-by: bravebot_session::listing::a_page_holds_the_newest_and_says_when_there_are_older`
`verified-by: bravebot_session::listing::a_live_session_is_listed_past_the_room_and_takes_none`
`verified-by: bravebot_session::listing::a_running_session_with_no_record_yet_is_listed_from_its_roster_entry`
`verified-by: bravebot_session::listing::an_unreadable_a_misnamed_and_a_hidden_record_are_left_out`
`verified-by: bravebot_session::listing::a_record_is_parsed_again_only_when_its_file_changed`
`verified-by: bravebot_tui::sessions_screen::a_control_character_in_what_a_row_holds_is_pictured`
`verified-by: bravebot_tui::sessions_screen::a_name_of_several_lines_is_one_row_of_its_first`
`verified-by: bravebot_tui::render::sessions_screen::a_control_character_in_what_a_row_holds_is_never_drawn`
`verified-by: bravebot_tui::sessions_screen::a_hidden_task_stays_off_the_list_through_a_refresh`
`verified-by: bravebot_tui::sessions_screen_tests::a_hidden_task_stays_hidden_the_next_time_the_sessions_screen_is_opened_in_this_session`

<a id="SCREEN-3"></a>
### SCREEN-3: a session's status is a fact about a process or a file, and the details say nothing else

A session is in one of seven statuses. `needs input` is a background session holding a prompt, and
`interrupted` one whose process ended in a turn or a held prompt ([BG-6](background-sessions.md#BG-6)).
`working` is a turn running, in a background session or in this terminal. `ready` is a background
session with a process and no turn, or this terminal's session with none. `open elsewhere` is a record
another terminal holds, whose state is not known. `stopped` is a background session whose process was
stopped, and `saved` is a record nothing runs. A roster entry whose process is gone is not believed
([BG-6](background-sessions.md#BG-6)). The session this terminal is in is `working` or `ready` as the
terminal says, whatever the roster or the lock says.

Where the screen is 90 columns wide beside the list, the details of the session under the cursor stand
at the right: its name, status, model, mode, turns, tokens as read and written, directory, branch, agent, the
issue and pull request linked, the session it was handed off from, size, the whole id and the last
prompt. Where the screen is too narrow for them, a line under the list says the branch, the size and
the start of the id of the session under the cursor, and where the list is 100 columns wide the branch
and the size are columns of every row. A manifest run, which can be read and not continued, says so on
its row. A session that needs a person says that it does and which kind of prompt it holds, by a
word from the fixed set of [BG-5](background-sessions.md#BG-5), and says nothing of what the prompt
is about. An interrupted session says its process ended in the middle of a turn. The list and the
details draw no reply, tool result or question text.

**Why.** A status the list can show truthfully has to be one that can be stated without reading
content ([BG-6](background-sessions.md#BG-6)). A terminal other than this one is alive, and saying it
is idle or working would be a guess.

`verified-by: bravebot_session::listing::a_roster_entry_decides_the_status_and_a_dead_process_is_not_believed`
`verified-by: bravebot_session::listing::this_sessions_status_is_the_terminals_and_a_held_record_is_open_elsewhere`
`verified-by: bravebot_session::listing::every_status_is_in_the_set_its_tab_counts`
`verified-by: bravebot_tui::render::sessions_screen::the_details_stand_beside_the_list_only_where_the_screen_is_wide`
`verified-by: bravebot_tui::render::sessions_screen::a_task_that_needs_the_person_says_what_kind_of_thing_it_waits_for`
`verified-by: bravebot_tui::render::sessions_screen::the_status_and_age_columns_are_dropped_where_the_list_is_too_narrow`
`verified-by: bravebot_tui::render::sessions_screen::the_row_under_the_cursor_is_marked_and_filled_edge_to_edge`
`verified-by: bravebot_tui::render::sessions_screen::the_details_give_the_id_size_agent_links_and_where_it_was_handed_off_from`
`verified-by: bravebot_tui::render::sessions_screen::a_wide_list_has_the_branch_and_the_size_of_each_session_in_columns`
`verified-by: bravebot_tui::render::sessions_screen::a_narrow_list_says_the_branch_size_and_id_of_the_cursor_row_under_it`

<a id="SCREEN-4"></a>
### SCREEN-4: five tabs count the sets of sessions, the header says what is waiting, and a search narrows the rows and not the counts

The tabs are All, Needs you, Working, Ready and Inactive. Needs you holds `needs input` and
`interrupted`, Working holds `working`, Ready holds `ready` and `open elsewhere`, and Inactive holds
`stopped` and `saved`. Each tab carries the number of sessions in its set, and the number does not
change with a search. Tab moves to the next tab and Shift-Tab to the previous, each going round, and
the digits 1 to 5 choose the tab in that place. None of them does anything while a line is being
typed, where a digit is a letter of the line. A tab narrows the list to its set, except that the
session being renamed stays listed when its status leaves the set. A click on a tab chooses it
([SCREEN-12](#SCREEN-12)).

The first line of the screen says how many sessions are waiting on the person and how many are
working, the first in the colour of a failure and the second in that of success, or that nothing is
waiting on the person where there are neither. The count on the Needs you tab is drawn in the first
colour while it is above zero, and no other count has a colour of its own.

The search box above the list is always drawn ([SCREEN-7](#SCREEN-7)), and what is typed in it is matched, without regard to case, anywhere in a session's
name, last prompt, directory, branch, agent and id, and in the issue and pull request links. Where the
text is the whole of some session's link, a link matches only by being that text, so a pasted pull
request address finds the session that was given it and not the one whose number starts with the same
digits; the name and the branch still match by what they contain. A session also matches when its
words hold the text ([SESSION-33](sessions.md#SESSION-33)), which are read the first time a character
is typed and not when the screen opens, and the first line that holds it is shown under the row. A
`since:<n>h`, `since:<n>d` or `since:<n>w` word keeps only the sessions written within that time and
is not part of the text, and until it is finished it is text like any other. `bravebot --from-pr`
opens the screen already narrowed to the sessions whose pull request is the one named, by its address
or its number, and a number is whole and not a prefix, so `127` does not find the session linked to
`1270`. An issue link is never taken for a pull request. Where no session is linked, the screen says
nothing matches and starts no session.

**Why.** The counts are what a person scans to find what needs them, and a count that changed with
what was typed would stop being that. A person who looks at the screen to find out whether anything
is waiting should have the answer before reading a row.

`verified-by: bravebot_tui::sessions_screen::each_tab_counts_its_set_and_filters_to_it`
`verified-by: bravebot_tui::sessions_screen::a_digit_chooses_the_tab_in_that_place_and_is_text_in_a_line`
`verified-by: bravebot_tui::sessions_screen::a_search_narrows_the_list_and_not_the_counts`
`verified-by: bravebot_tui::sessions_screen::typing_part_of_a_branch_finds_the_session_that_ran_on_it`
`verified-by: bravebot_tui::sessions_screen::typing_part_of_an_issue_link_finds_the_session_given_it`
`verified-by: bravebot_tui::sessions_screen::a_pasted_pull_request_link_leaves_only_the_session_that_holds_it`
`verified-by: bravebot_tui::sessions_screen::a_pasted_link_keeps_a_session_whose_title_holds_it`
`verified-by: bravebot_tui::sessions_screen::part_of_a_pull_request_link_keeps_every_session_it_is_part_of`
`verified-by: bravebot_tui::sessions_screen::a_pull_request_narrows_the_list_by_number_or_address_and_not_by_a_prefix`
`verified-by: bravebot_tui::sessions_screen::typing_narrows_a_pull_requests_list_and_an_unlinked_one_lists_nothing`
`verified-by: bravebot_tui::sessions_screen::typing_words_said_in_a_session_finds_it_and_shows_the_line_that_did`
`verified-by: bravebot_tui::sessions_screen::a_session_whose_words_do_not_hold_the_phrase_is_left_out_until_it_is_cleared`
`verified-by: bravebot_tui::sessions_screen::a_name_that_holds_the_text_shows_no_line_of_what_was_said`
`verified-by: bravebot_tui::sessions_screen::a_since_word_keeps_the_sessions_written_within_it_and_is_text_until_finished`
`verified-by: bravebot_tui::sessions_screen::what_the_sessions_say_is_read_on_the_worker_after_the_first_character_and_the_keys_do_not_wait`
`verified-by: bravebot_tui::sessions_screen::leaving_the_search_takes_the_lines_of_what_was_said_away`
`verified-by: bravebot_tui::sessions_screen::a_search_asks_for_every_record_and_clearing_it_goes_back_to_a_page`
`verified-by: bravebot_tui::sessions_screen::delete_in_the_search_box_asks_nothing`
`verified-by: bravebot_tui::render::sessions_screen::a_pull_request_nothing_is_linked_to_is_named_and_says_nothing_matches`
`verified-by: bravebot_tui::render::sessions_screen::a_found_session_shows_the_line_that_matched_under_its_row`
`verified-by: bravebot_tui::sessions_screen::the_task_being_renamed_stays_listed_when_the_tab_would_leave_it_out`
`verified-by: bravebot_tui::render::sessions_screen::the_header_summarises_what_needs_attention_and_counts_each_tab`
`verified-by: bravebot_tui::render::sessions_screen::a_calm_header_says_so_and_only_the_waiting_count_has_a_colour`
`verified-by: bravebot_tui::render::sessions_screen::each_tab_and_row_is_recorded_where_it_is_drawn`

<a id="SCREEN-5"></a>
### SCREEN-5: the list is one list with the project in a column, `g` groups it by project, agent or model, pinned sessions come first, and the choices are kept

The list opens as one list with no headings. Its order is that of the attention each session asks
for: those waiting on the person, then those working, then those ready, then those not running, each
newest first ([SCREEN-6](#SCREEN-6)). Each row names its project in a column of its own, the last two
parts of its directory, where the list is 74 columns wide. Narrower, the column is left out and the
directory is in the details.

`g` goes through four layouts and round: together, by project, by agent and by model. A heading for
each project, agent or model carries the number of sessions under it, and `N of M` where a tab or a
search leaves some of them out. By project, the directory this session works in comes first, headed
Here. By agent, a heading names the agent definition the sessions were started with, and the sessions
started with none are under No agent; by model, a heading names the model that last answered, and
Unknown stands for a session with none. The project stays a column of the list under the last two
layouts. The headings follow in the order of the newest session each had when the layout was chosen,
and the rows under a heading keep the order above. The hint beside the title names what the key will
do (group by project, by agent, by model, then list together), and not the layout it is in, which is
plain from whether there are headings and what they name.

Pinned sessions come first in every layout, in the order they were pinned. With no headings each is
marked with a star before its name, and under the others they are a group of their own, headed
Pinned. `p` pins the session under the cursor or takes the pin off.

The layout and the pins are saved when they change and read when the Sessions screen opens
([SCREEN-11](#SCREEN-11)). A pin names a session by its id.

**Why.** A person looking at many sessions wants the ones that need them first, wherever they are.
A heading for each status would repeat what the tabs already filter by, so the headings that are left
are the ones a person looks a session up by: where it ran, which agent it was started with and which
model answered it. Saying what the key will do is what makes a one-key toggle readable.

`verified-by: bravebot_tui::sessions_screen::together_is_the_attention_order_and_a_project_is_a_partition_of_it`
`verified-by: bravebot_tui::sessions_screen::the_grouping_key_goes_between_the_layouts_and_asks_to_save_each`
`verified-by: bravebot_tui::sessions_screen::an_agent_or_a_model_is_a_heading_and_a_session_with_none_has_its_own`
`verified-by: bravebot_tui::render::sessions_screen::an_agent_heading_names_the_agent_and_the_sessions_with_none`
`verified-by: bravebot_tui::sessions_screen::pinned_rows_come_first_in_the_order_they_were_pinned`
`verified-by: bravebot_tui::sessions_screen::pinning_twice_unpins_and_each_press_asks_to_save`
`verified-by: bravebot_tui::render::sessions_screen::a_pinned_session_is_a_star_together_and_a_heading_by_project`
`verified-by: bravebot_tui::render::sessions_screen::the_project_is_a_column_of_the_list_that_is_not_grouped_and_a_heading_of_the_one_that_is`
`verified-by: bravebot_tui::render::sessions_screen::the_project_layout_puts_this_directory_first_and_says_so`
`verified-by: bravebot_tui::render::sessions_screen::a_heading_counts_what_is_shown_of_what_there_is_when_a_search_narrows_it`
`verified-by: bravebot_tui::sessions_screen_tests::the_grouping_and_the_pins_are_saved_and_the_next_sessions_screen_opens_with_them`
`verified-by: bravebot_session::listing::the_grouping_key_goes_through_the_layouts_and_round_and_each_has_a_word`

<a id="SCREEN-6"></a>
### SCREEN-6: a refresh keeps the rows where they were, and the cursor on its session

Within a status group the rows keep the order they had while they stay in the group. A session that
arrives, or whose status moved it to another group, is placed after the rows already in that group.
A project keeps the place it was given when it was first listed, so a group does not move because one
of its sessions was written. The cursor is on a session and not on a place in the list: a refresh
that moves the session leaves the cursor on it, and a refresh that removes it puts the cursor on the
first row shown.

The first read chooses where the cursor starts. It is on the first session waiting on the person
where there is one, and on the session this terminal is in where there is not.

**Why.** The list is read every two seconds while a person looks at it, and rows that reordered
themselves by when each was last written would move under a hand that is about to press Enter. A
person who opens the screen is most often asking what needs them.

`verified-by: bravebot_tui::sessions_screen::a_refresh_keeps_rows_in_place_while_they_stay_in_their_group`
`verified-by: bravebot_tui::sessions_screen::a_project_keeps_its_place_when_one_of_its_sessions_is_written`
`verified-by: bravebot_tui::sessions_screen::the_cursor_stays_on_its_task_when_a_refresh_moves_it`
`verified-by: bravebot_tui::sessions_screen::the_cursor_starts_on_what_is_waiting_on_the_person`
`verified-by: bravebot_tui::sessions_screen::the_cursor_starts_on_the_session_the_terminal_is_in`

<a id="SCREEN-7"></a>
### SCREEN-7: the keys of the list, and the lines typed in it

The search box is drawn above the list whatever has the focus, and is the cell before the first row:
Up, `k` and Ctrl-P move the cursor up, and Down, `j` and Ctrl-N down, from the first row to the box
and from the box to the last row (Up) or the first (Down), and from the last row to the box. While
the box has the focus no row is the cursor, the letters typed are the search, and Enter opens the
first match. A screen opened by `--resume`, `--from-pr` or `/resume` has the box focused, and one
opened by Left or `/sessions` has the cursor on a row. Home and End go to the first row and the last. Page Up, Page Down, Ctrl-B and Ctrl-F move a screen and
stop at either end without going round. "Show more" is the entry after the last session where older
sessions were left out, and the cursor goes to it like any row. Enter and Right open the session under the
cursor ([SCREEN-8](#SCREEN-8)).

`/` and F3 give the box the focus: the letters typed narrow the list to the sessions whose name,
last prompt, directory, branch or id holds them, case aside, and the first match is the one Enter
opens. Left and Right do nothing in it. `r` begins a
line holding the session's name ([SCREEN-9](#SCREEN-9)). While either line is being typed, a letter is
the line's and not the Sessions screen's key, Backspace takes one off, Ctrl-U clears the line, and the cursor
keys other than Enter and Escape do nothing in a rename. Escape clears the text in the box and gives
the focus to the list, and leaves the screen when the box is empty; it ends a rename first. `?` shows the
keys in three groups and takes every key until it or Escape closes it.

A key held with Ctrl or Alt is not the letter it carries: only the keys named above are read with
Ctrl, and Alt reads none. The Sessions screen draws a list longer than the screen from the row that keeps the
cursor in view, with a mark above or below where there is more, and a heading that begins the group
comes into view with its first row.

`verified-by: bravebot_tui::sessions_screen::up_and_down_pass_through_the_search_box_and_nothing_moves_while_a_name_is_typed`
`verified-by: bravebot_tui::sessions_screen::a_page_moves_by_the_height_and_stops_at_the_ends`
`verified-by: bravebot_tui::sessions_screen::a_screen_opened_for_typing_takes_letters_as_text_and_enter_opens_the_first_match`
`verified-by: bravebot_tui::sessions_screen::escape_clears_text_left_in_the_box_before_it_leaves`
`verified-by: bravebot_tui::sessions_screen::this_directory_is_compared_as_a_path`
`verified-by: bravebot_tui::render::sessions_screen::the_search_box_is_always_drawn_and_the_cursor_is_in_it_or_in_a_row`
`verified-by: bravebot_tui::sessions_screen::a_letter_typed_in_a_search_is_not_a_command`
`verified-by: bravebot_tui::sessions_screen::a_chord_is_not_the_letter_it_carries`
`verified-by: bravebot_tui::sessions_screen::enter_and_right_open_the_task_and_show_more_asks_for_more`
`verified-by: bravebot_tui::sessions_screen::new_is_begun_where_the_task_under_the_cursor_ran`
`verified-by: bravebot_tui::sessions_screen::stop_needs_a_running_process_and_the_rest_need_a_task`
`verified-by: bravebot_tui::render::sessions_screen::a_list_taller_than_the_screen_scrolls_to_keep_the_cursor_on_it`
`verified-by: bravebot_tui::render::sessions_screen::show_more_is_drawn_after_the_last_task_when_older_sessions_were_left_out`
`verified-by: bravebot_tui::render::sessions_screen::an_empty_list_says_whether_nothing_exists_or_nothing_matches`
`verified-by: bravebot_tui::render::sessions_screen::the_line_being_typed_has_its_label_and_the_caret_after_it`
`verified-by: bravebot_tui::render::sessions_screen::the_help_lists_the_keys_in_three_groups`
`verified-by: bravebot_tui::render::sessions_screen::the_footer_names_the_keys_that_answer_where_the_person_is`
`verified-by: bravebot_tui::sessions_screen::backspace_edits_a_line_and_deletes_a_task_only_outside_one`
`verified-by: bravebot_tui::render::sessions_screen::a_tiny_terminal_draws_without_a_panic`

<a id="SCREEN-8"></a>
### SCREEN-8: opening, copying, beginning and stopping a session, and where each lands

Enter opens the session under the cursor. The session this terminal is in closes the Sessions screen. A session a
background process runs is joined as `bravebot attach` joins it, and this terminal's interface ends
([BG-9](background-sessions.md#BG-9)). Any other session is a record: one of the directory this session
works in is carried on in this process, as `/resume` carries one on, and one of another directory is
carried on by `bravebot --resume` started in that directory, which takes the terminal as this process
leaves it. A record another terminal or a background process holds, and a manifest run, are refused
with the sentence `/resume` gives, and the Sessions screen stays open.

`f` copies the session's record as `--fork` does and carries on in the copy, in this process where the
record is of this directory and by `bravebot --fork` started there where it is not. `n` begins a
session in the directory of the session under the cursor, in this process where that is this
directory or no session is under the cursor and by `bravebot` started there where it is not. `x` ends the process of a background session, as `bravebot
sessions stop` does, and does nothing on a session no process runs. A directory that is gone is said
to be, and nothing ends.

Opening, copying and beginning a session end the session this terminal is in. While a turn runs
the Sessions screen asks once before doing any of them, and stops the turn only on `y`
([SCREEN-15](#SCREEN-15)). Stopping, renaming another session, archiving, deleting, pinning, grouping and reading more are done where they are
asked, at rest or mid-turn. When a session leaves the list, by archive, delete or hide, the cursor goes to
the session after it, or the session before it where it was the last.

**Why.** A process is the one writer of its record ([BG-9](background-sessions.md#BG-9)), so a session it
runs is joined and not resumed. A session is found from the directory it ran in, so a record of
another directory is carried on by a process started there. A turn in flight is not abandoned for a
session the person only meant to look at, which is why stopping it is asked about.

`verified-by: bravebot_tui::sessions_screen_tests::a_task_of_this_directory_is_resumed_in_this_process`
`verified-by: bravebot_tui::sessions_screen_tests::a_task_of_another_directory_is_carried_on_by_a_process_started_there`
`verified-by: bravebot_tui::sessions_screen_tests::a_task_a_process_runs_is_joined_and_not_resumed`
`verified-by: bravebot_tui::sessions_screen_tests::a_task_another_terminal_has_open_is_refused_and_the_sessions_screen_stays`
`verified-by: bravebot_tui::sessions_screen_tests::opening_the_session_you_are_in_closes_the_sessions_screen`
`verified-by: bravebot_tui::sessions_screen_tests::forking_copies_the_record_and_carries_on_in_the_copy`
`verified-by: bravebot_tui::sessions_screen_tests::a_manifest_run_is_not_forked`
`verified-by: bravebot_tui::sessions_screen_tests::a_new_session_is_begun_where_the_task_under_the_cursor_ran`
`verified-by: bravebot_tui::sessions_screen_tests::stopping_a_task_nothing_runs_says_so`
`verified-by: bravebot_tui::sessions_screen_tests::a_key_that_acts_on_a_task_asks_the_loop_for_it`
`verified-by: bravebot_tui::sessions_screen_tests::what_ends_the_session_is_given_back_and_the_rest_is_done_where_it_is_asked`
`verified-by: bravebot_tui::sessions_screen::removing_the_task_under_the_cursor_moves_to_its_neighbour`
`verified-by: bravebot_cli::reopen::each_way_of_carrying_on_is_the_option_that_does_it`
`verified-by: bravebot_cli::reopen::an_id_that_begins_with_a_dash_is_not_passed_on`

<a id="SCREEN-9"></a>
### SCREEN-9: renaming starts from the name, and a record a process holds is not written by another

`r` begins a line holding the session's name. Enter carries out a name that is not empty once trimmed,
and an empty one is not carried out and does not end the line. The name is cut to the length a
derived title is. Renaming the session this terminal is in is what `/rename` does. Renaming another
session rewrites its record, and its roster entry where it has one, taking the lock of the record
first; a record a process holds is refused with a sentence and left as it was, since that process is
the one that writes it ([SESSION-38](sessions.md#SESSION-38)). A rename is written beside the record
and renamed over it, so an interrupted write leaves the record it had.

`verified-by: bravebot_tui::sessions_screen::a_rename_starts_from_the_name_and_an_empty_name_is_not_carried_out`
`verified-by: bravebot_tui::sessions_screen_tests::rename_writes_the_record_unless_a_process_holds_it`
`verified-by: bravebot_tui::sessions_screen_tests::renaming_this_session_is_what_the_rename_command_does`
`verified-by: bravebot_session::sessions::a_stored_session_is_renamed_and_the_rest_of_its_record_is_left`
`verified-by: bravebot_session::sessions::a_stored_session_a_process_holds_is_not_renamed`

<a id="SCREEN-10"></a>
### SCREEN-10: archiving and deleting ask first, and only `y` answers

`a` asks to archive the session under the cursor and Backspace asks to delete it. The question names the
session and says what the key does; `y` carries it out, and Escape, `n` and Ctrl-C withdraw it. Every
other key does nothing while the question is up.

Archiving ends the process running the session, if there is one, and takes the session off the list until
its record is written again, so a session used after it was archived is back without anything having
to un-archive it. The record is kept. Deleting ends the process, if there is one, and removes the
record, its trail, its index and its lock, as `bravebot sessions delete` does. Neither is done to the
session this terminal is in, and deleting is not done to a record another terminal has open; each is
refused with a sentence. `h` hides a session from the Sessions screen for as long as the session this terminal is in lasts, through
closes and opens of the Sessions screen, and changes nothing on disk.

**Why.** Both are hard to undo, and one key typed for another letter must not carry either out. A
record is written by one process at a time, so the session in use and one held elsewhere are left to
the process that holds them.

`verified-by: bravebot_tui::sessions_screen::archive_and_delete_ask_and_only_y_answers`
`verified-by: bravebot_tui::sessions_screen::an_archived_task_leaves_the_list`
`verified-by: bravebot_tui::render::sessions_screen::archive_and_delete_put_a_question_over_the_list_naming_the_task`
`verified-by: bravebot_tui::sessions_screen_tests::an_archived_task_stays_off_the_list_until_its_session_is_written_again`
`verified-by: bravebot_tui::sessions_screen_tests::delete_removes_a_free_record_and_leaves_the_current_one_and_an_open_one`
`verified-by: bravebot_session::listing::an_archived_session_comes_back_when_it_is_used_again`

<a id="SCREEN-11"></a>
### SCREEN-11: what the Sessions screen keeps is three small files of ids and words

The grouping is one word, the pins are session ids one to a line in the order they were pinned, and
the archive is session ids each with the time it was archived. They are files in the state directory
([STATE-1](state-directory.md#STATE-1)), written beside and renamed over, and written by nothing in an
incognito session ([incognito.md](incognito.md)), which still reads them. A line that is not shaped like
a session's id, a word that is not a grouping, a duplicate and a pair that is not an id and a whole
number are dropped when the file is read, so a file edited by hand puts on the screen nothing but an
id and a word of the catalog. At most 200 pins and 5,000 archived ids are kept.

`verified-by: bravebot_session::listing::a_preference_file_edited_by_hand_puts_nothing_but_ids_and_words_on_the_screen`
`verified-by: bravebot_tui::sessions_screen_tests::the_grouping_and_the_pins_are_saved_and_the_next_sessions_screen_opens_with_them`

<a id="SCREEN-12"></a>
### SCREEN-12: the pointer chooses a tab, puts the cursor on a row, opens it with a second click and walks the list with the wheel

A click on a tab chooses it. A click on a row puts the cursor on it, and a second click on the same
row within the time of a double click opens it as Enter does, as a row of a picker does
([MOUSE-15](mouse.md#MOUSE-15)). The same holds for "Show more". A press on a heading, on the blank
line between groups, on the header or below the last row does nothing. A turn of the wheel moves the
cursor `tui.wheelRows` rows over the headings and stops at the first row and the last
([MOUSE-46](mouse.md#MOUSE-46)). Each row and tab is recorded as a target where it is drawn, and
nothing the plain frame drew stands while the Sessions screen is open, so a press never reaches what the Sessions screen
covers ([MOUSE-50](mouse.md#MOUSE-50)).

`verified-by: bravebot_tui::sessions_screen_tests::a_click_on_a_tab_chooses_it_and_a_click_on_a_row_puts_the_cursor_on_it`
`verified-by: bravebot_tui::sessions_screen_tests::a_second_click_on_a_row_opens_it_as_enter_does`
`verified-by: bravebot_tui::sessions_screen_tests::a_press_that_is_on_no_row_does_nothing`
`verified-by: bravebot_tui::sessions_screen_tests::the_wheel_walks_the_cursor_and_stops_at_the_ends`
`verified-by: bravebot_tui::sessions_screen_tests::a_press_on_the_sessions_screen_reaches_nothing_beneath_it`
`verified-by: bravebot_tui::sessions_screen_tests::a_click_or_the_wheel_while_a_question_is_up_leaves_the_list_alone`
`verified-by: bravebot_tui::app::tests::standing::every_kind_of_target_stands_in_the_states_its_click_works_in_and_in_no_other`

<a id="SCREEN-13"></a>
### SCREEN-13: `--resume`, `--from-pr` and a bare `/resume` open the Sessions screen, and leaving it leaves the session as it was

`bravebot --resume` and `-r` with no id begin a new session with the Sessions screen open over it, and
`--from-pr <number or address>` does the same narrowed to a pull request ([SCREEN-4](#SCREEN-4)).
Escape leaves the screen for the new session, which is the ordinary session that starting without a
list would have begun. `/resume` with nothing after it opens the screen over the session being run,
and leaving it stays in that session. Enter on a session of this directory carries it on in this process
and Enter on one of another directory starts `bravebot --resume <id>` there ([SCREEN-8](#SCREEN-8)).
`--resume <id>`, `-r <id>` and `/resume <id>` name a record outright and open no screen.

**Why.** The commands are what people who come from other tools type. The screen searches what was
said, narrows to a pull request and gives the facts of each session, so `--resume` and `/resume`
need no list of their own.

`verified-by: bravebot_tui::sessions_screen_tests::a_bare_resume_opens_the_sessions_screen_and_a_named_one_does_not`
`verified-by: bravebot_tui::sessions_screen_tests::a_manifest_run_is_not_opened_and_says_why`
`verified-by: bravebot_cli::reopen::the_program_started_again_is_this_one_by_its_absolute_path`

<a id="SCREEN-14"></a>
### SCREEN-14: a settings file can move thirteen keys of the Sessions screen, and nothing else

A `sessionsKeys` block in `settings.json` names an action and the key it is to answer. It layers per
action the way `keybindings` and `env` do, so a project file moving one action says nothing about
the others. These actions can be moved, and each answers the key shown until the block says
otherwise:

- `up` (`k`) and `down` (`j`): move the cursor.
- `new` (`n`), `fork` (`f`), `rename` (`r`), `stop` (`x`), `archive` (`a`), `delete` (Backspace),
  `hide` (`h`) and `pin` (`p`): act on the session under the cursor.
- `group` (`g`): go to the next layout ([SCREEN-5](#SCREEN-5)).
- `search` (`/`): give the search box the focus ([SCREEN-7](#SCREEN-7)).
- `help` (`?`): open and close the list of keys.

A key is spelled as one character, alone or after `ctrl-` or `alt-`, or as a function key from `f1`
to `f12`: `z`, `ctrl-e`, `alt-h`, `f5`, `ctrl+/`. Nothing else can be moved. Enter, Right and the other
arrows, Page Up, Page Down, Home, End, Tab, Shift-Tab, Escape, the digits 1 to 5, Ctrl-P, Ctrl-N,
Ctrl-B, Ctrl-F, Ctrl-C, F3 and the keys that answer a question or a line being typed are the screen's
own. Delete moved to a key takes Backspace and the Delete key with it, and the key an action was
moved off does nothing.

The keys apply while the cursor is on the list. In a search, in a name being typed and under the
question of archiving or deleting, a character is text or an answer whatever it is bound to, so a
bare character in the block takes no letter out of anything that is typed. This is the difference
from [INPUT-32](terminal-input.md#INPUT-32), which refuses every key without Ctrl or Alt because the
box beneath is typed into at all times. The two blocks are separate and a key in one does not contest
a key in the other, since the box is not drawn while the screen is open.

A request is refused, and the action stays on the key it had, when the spelling names no key or names
an event a terminal never reports (Shift over a character, Ctrl and Alt together, `f13`); when it
names a key the screen keeps; when the name is no action; and when another action stands on the key.
A key two actions both ask for is given up by both, and two actions trading keys is no contest, by
the rule that settles the input box's chords. Each refusal is said when the Sessions screen opens, the
first with the name, the spelling and the key the action stays on, and a count of the others, until
the next key. What is said is pictured and cut, since the file may hold anything.

The footer, the hint beside the title and the list `?` open name the keys in force.

**Why.** The keys are bare letters, so a settings file that could move only a chord carrying Ctrl or
Alt could move none of them, and a person whose terminal or habits want `x` elsewhere could not move
it. A line being typed answers every character already, so a bound letter costs nothing there. A
refusal that is not said leaves the person pressing a key that does what it always did.

`verified-by: bravebot_config::settings::a_sessions_keys_block_is_read_and_layers_per_action`
`verified-by: bravebot_tui::sessions_keys::the_defaults_are_the_keys_the_screen_has_always_had`
`verified-by: bravebot_tui::sessions_keys::a_character_a_ctrl_or_alt_character_and_a_function_key_can_be_asked_for`
`verified-by: bravebot_tui::sessions_keys::a_key_the_screen_keeps_is_refused_and_the_action_stays`
`verified-by: bravebot_tui::sessions_keys::a_spelling_nothing_can_read_is_refused_and_the_action_stays`
`verified-by: bravebot_tui::sessions_keys::a_name_that_is_no_action_is_refused`
`verified-by: bravebot_tui::sessions_keys::a_contested_key_is_given_up_and_a_trade_is_not`
`verified-by: bravebot_tui::sessions_keys::a_moved_key_is_read_as_the_action_and_the_old_key_does_nothing`
`verified-by: bravebot_tui::sessions_keys::a_refusal_says_the_action_the_spelling_and_the_key_it_stays_on`
`verified-by: bravebot_tui::sessions_keys::what_is_said_back_is_pictured_and_cut`
`verified-by: bravebot_tui::keybindings::conflicting_chords_fall_back_to_defaults`
`verified-by: bravebot_tui::keybindings::two_actions_can_trade_chords`
`verified-by: bravebot_tui::sessions_screen::a_moved_key_acts_and_the_key_it_left_does_nothing`
`verified-by: bravebot_tui::sessions_screen::a_moved_delete_takes_backspace_and_delete_with_it_and_the_rest_stay`
`verified-by: bravebot_tui::sessions_screen::a_moved_key_is_text_in_a_search_and_in_a_rename`
`verified-by: bravebot_tui::sessions_screen::the_moved_help_key_opens_and_closes_the_help`
`verified-by: bravebot_tui::render::sessions_screen::the_footer_the_hint_and_the_help_name_the_keys_in_force`
`verified-by: bravebot_tui::render::sessions_screen::a_key_that_cannot_be_had_is_said_when_the_screen_opens`
`verified-by: bravebot_tui::sessions_screen_tests::the_keys_a_settings_file_moved_reach_the_screen_through_the_adopted_settings`

<a id="SCREEN-15"></a>
### SCREEN-15: opening, copying or beginning a session while a turn runs asks once, and only `y` stops the turn

Opening a session, copying it and beginning one end the session this terminal is in, which a turn in
flight does not allow without being stopped. While a turn runs, Enter, Right, a second click on a row
or `f` on a session, and `n`, put a question over the list in place of doing it. The question names
the session, says what will be done with it (open it, copy it, or begin a session where it ran) and
says that the turn that is running is stopped, as Escape stops it, and the session saved as it then
stands. `y` answers it. Escape, `n` and Ctrl-C withdraw it and leave the turn running, and every other
key does nothing while it is up, as the question of archiving does ([SCREEN-10](#SCREEN-10)).

Nothing is asked where nothing would end. Opening the session this terminal is in only leaves the
screen. A manifest run, a record another terminal holds and a directory that is gone are refused with
the sentence they have at rest, before the turn is touched.

After `y` the turn is stopped by the press that stops a turn, and what was asked is carried out when
the turn has ended and its record is written, as it would have been at rest: the session is opened
here, by a process started in its directory, or joined where a background process runs it, as
[SCREEN-8](#SCREEN-8) says. If the turn ends by itself while the question is up, `y` carries out what
was asked with nothing to stop.

Only a turn is asked about. While a compaction, an aside, a commit message, a handoff, a goal check, a
plan run or a command for the shell runs, the same keys put no question: none of them can be stopped
part-way and left as it was, and each ends on its own. The list says in one line that names what is
running that opening, copying or beginning a session ends this one, so it is done when that has ended.
Opening the session this terminal is in only leaves the screen, as it does during a turn.

**Why.** A person who sees in the list that another session is waiting on them wants to go to it, and
refusing until the turn ends sends them back to the transcript to stop the turn by hand and to the
list again. One question, answered by one key that no other key can stand in for, keeps a stray press
from ending work the person did not mean to end.

`verified-by: bravebot_tui::app::tests::interrupt::opening_copying_or_beginning_asks_before_the_turn_is_touched`
`verified-by: bravebot_tui::app::tests::interrupt::y_stops_the_turn_and_holds_what_was_asked_until_it_has_ended`
`verified-by: bravebot_tui::app::tests::interrupt::n_escape_and_ctrl_c_withdraw_the_question_and_every_other_key_leaves_it_up`
`verified-by: bravebot_tui::app::tests::interrupt::the_session_in_use_is_opened_by_leaving_the_screen_without_a_question`
`verified-by: bravebot_tui::app::tests::interrupt::a_session_that_would_be_refused_is_refused_before_the_turn_is_stopped`
`verified-by: bravebot_tui::app::tests::interrupt::a_second_click_on_a_row_over_a_running_turn_asks_as_enter_does`
`verified-by: bravebot_tui::app::tests::interrupt::a_loop_that_is_not_a_turn_says_why_opening_copying_or_beginning_waits`
`verified-by: bravebot_tui::app::tests::interrupt::the_notice_names_what_is_running`
`verified-by: bravebot_tui::sessions_screen::the_question_about_a_running_turn_is_answered_by_y_alone`
`verified-by: bravebot_tui::render::sessions_screen::the_question_about_a_running_turn_names_the_session_and_what_is_done`

<a id="SCREEN-16"></a>
### SCREEN-16: the Sessions screen and `bravebot sessions` list the same sessions from one listing, in one set of words

The sessions of the roster are read by `Reader::roster`, the roster-only view of the listing the screen
is read by. For a session the screen also lists, it returns the same task: the same status, name,
held prompt kind, time last written and whether a process runs it. It returns every roster entry,
including one whose process is gone and that has written no record, which the screen leaves out because
there is nothing to open. A record the roster has no entry for is not in it.

The list `bravebot sessions` opens ([BG-16](background-sessions.md#BG-16)) and the lines it prints
([BG-5](background-sessions.md#BG-5)) are made from those tasks. Under headings, the statuses come in the
order the screen lists them: waiting on a person (`needs input`, then `interrupted`), `working`,
`ready`, then `stopped`. The list names a status in the screen's word, so a session the screen calls
`Ready` is under a heading `Ready` and not `idle`, and a session that needs input says the kind of
prompt in the same words in both.

The state names of [BG-6](background-sessions.md#BG-6) stay where they are protocol: the roster holds
them, `bravebot sessions --json` prints the roster's fields with them, and a line printed for a pipe
names the state in them (`idle`), since a script reads that. A roster state maps to one status and back
(`idle` is `ready`), and the two cannot name different sessions or different states.

**Why.** Two readings of the same sessions could disagree about which sessions there are, what state
one is in and what it is called, and a person who sees the list in one place and the screen in the
other would not know which to believe. The words of a script are a contract with its author and do not
change with a screen's.

`verified-by: bravebot_session::listing::the_roster_view_and_the_screen_list_the_same_sessions_the_same_way`
`verified-by: bravebot_session::listing::a_status_maps_to_the_state_of_the_roster_and_back_and_has_one_order`
`verified-by: bravebot_tui::sessions_view::the_headings_are_in_the_order_and_the_words_of_the_sessions_screens_statuses`
`verified-by: bravebot_cli::background::the_list_and_the_screen_say_one_word_and_the_lines_say_the_word_of_bg_6`

<a id="SCREEN-17"></a>
### SCREEN-17: the records are read on a thread of their own, and a key never waits for them

The periodic read of the list, the read of every record that a search asks for, and the read of what
the sessions say are done by one worker thread, in the order they were asked. The screen draws and
takes keys while they run, applies what has been found when the loop next looks, and looks every 50
milliseconds while a request is outstanding. A request for the list is not sent while another is
outstanding, and is sent again at the next refresh. A result for the text that was in the box is
applied only while that is still the text, so a late answer to text that has changed is dropped. The
first read when the screen opens, and the read after a key that changed a record (rename, archive,
delete, stop), are made at once, on the page, because the key's result is what is drawn next.

**Why.** A search reads every record and builds the index entry of each session not yet indexed, and a
screen that took no key while it did would look hung. What the keys change is read at once because a
list that did not show the change would look as though the key had not worked.

`verified-by: bravebot_tui::sessions_screen::the_list_is_read_on_the_worker_and_the_keys_do_not_wait_for_it`
`verified-by: bravebot_tui::sessions_screen::what_the_sessions_say_is_read_on_the_worker_after_the_first_character_and_the_keys_do_not_wait`
`verified-by: bravebot_tui::sessions_screen::an_answer_for_text_that_has_changed_is_not_applied`
`verified-by: bravebot_tui::sessions_screen::a_refresh_keeps_the_lines_of_what_was_said_until_the_new_answer_arrives`

## Not decided

- **A reply or the latest activity of a session.** Other tools show the last message a session wrote.
  [background-sessions.md](background-sessions.md) keeps text a model wrote out of the list of
  sessions a person reads at a glance, and the Sessions screen keeps to it. Showing a reply, marked as the
  model's words, is a change to that rule.
- **A key to begin a session in a new worktree.** [worktrees.md](worktrees.md) is proposed and nothing
  builds it, so `n` begins a session in the directory and no key makes a worktree.

## Known costs

- **A session elsewhere is a started process.** The flags this terminal was started with (`--model`, a
  settings file, a permission mode) are not passed on to a session opened in another directory, which
  starts as `bravebot --resume <id>` would from a shell there.
- **The program started again keeps this process's `PATH` and environment.** It is this process's own
  executable by its absolute path ([SANDBOX-30](sandboxing.md#SANDBOX-30)), started with the `PATH`
  and the environment this process has, because it is the person's own session continuing. The
  credentials this agent holds stay in that environment, as in the process they continue from, and no
  program a turn runs and no prompt receives them ([CRED-14](credential-protection.md#CRED-14)). What
  the new process starts by name it starts under that same rule.
- **Stop waits.** Ending a process waits up to five seconds for it to go, and the screen is not drawn
  in that time.
- **A terminal elsewhere has no state.** A session open in another terminal is listed as open
  elsewhere, and what it is doing is not known to this one.
- **The delegates of a session are not rows.** They are not sessions, and are watched with the
  agents view ([WATCH-7](watching.md#WATCH-7)).
- **Usage is the record's tokens.** The details give the tokens a session spent as read and written,
  and no estimate of what they cost.
- **The first read is a page.** The 40 most recent records are read first, and "Show more" reads 40
  more. A search reads every record the first time something is typed, on a thread of its own
  ([SCREEN-17](#SCREEN-17)), so the rows found by name show at once and the rows found by what was said
  arrive when their records have been read, which on a machine with a great many sessions can be
  seconds.
