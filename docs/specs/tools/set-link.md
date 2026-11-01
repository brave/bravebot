---
id: SETLINK
title: set_link
status: normative
governs:
  - crates/agent/src/tools/link.rs
  - crates/agent/src/links.rs
documented-by: docs/website/docs/reference/tools.md
---

## Scope

Recording the issue or pull request a session is for, on the planner's call, where the person wrote the address in one of their own messages. The links, how they are checked and how the panel draws them are [PANEL-12](../info-panel.md#PANEL-12); this file is the tool's own surface.

## Clauses

<a id="SETLINK-1"></a>
### SETLINK-1: the address is the one routing field, and it must be a word of a line the person typed

`kind` is `issue` or `pull_request` and `url` is the address. `url` decides what the session records, so it is routing: it is read through the gate every planner argument goes through, which refuses it in a context that has met something untrusted, and then held to what a link may be ([PANEL-12](../info-panel.md#PANEL-12)): one `http` or `https` address with a host, in printable ASCII. It is recorded only if it is, as a whole, one of the words of a line the person typed to this session, with the quotes, brackets and trailing punctuation of a sentence taken off the word. A substring of a longer typed address does not count. A reply, a tool result, a file's contents and a message from another session are not lines the person typed, and neither is a line restored from a saved session or sent with files dropped on it, which `typed_by_the_person` leaves out. The excerpts of sessions a line names with `@session:` share its message and are another session's words, the assistant's included, so only the line itself counts.

So a person reading the address could approve it alone ([TOOL-2](tool-surface.md#TOOL-2)): they wrote it. The call asks nothing, since the answer would be the address they already typed.

The refusals say that nothing was recorded and do not repeat the address.

`verified-by: bravebot_agent::tools::an_address_the_person_typed_is_recorded_under_the_kind_asked_for`
`verified-by: bravebot_agent::tools::an_address_the_person_did_not_type_is_refused_and_nothing_is_recorded`
`verified-by: bravebot_agent::tools::the_start_of_a_longer_typed_address_is_refused`
`verified-by: bravebot_agent::tools::a_typed_word_that_is_not_a_web_address_is_refused`
`verified-by: bravebot_agent::tools::a_kind_other_than_issue_or_pull_request_is_refused`
`verified-by: bravebot_agent::tools::a_context_that_has_met_something_untrusted_cannot_set_a_link`
`verified-by: bravebot_agent::tools::a_context_holding_private_content_still_records_a_typed_address_and_nothing_else`
`verified-by: bravebot_agent::tools::an_address_on_another_tracker_is_recorded`
`verified-by: bravebot_agent::tools::only_the_address_the_planner_names_is_recorded_from_a_line_with_several`
`verified-by: bravebot_agent::tools::only_a_call_that_dispatches_as_set_link_is_handed_the_typed_lines`
`verified-by: bravebot_agent::links::the_start_of_a_longer_address_is_not_found`
`verified-by: bravebot_agent::links::a_whole_word_of_a_typed_line_is_found`
`verified-by: bravebot_agent::links::an_address_wrapped_in_brackets_or_quotes_is_found`
`verified-by: bravebot_agent::links::an_address_nobody_typed_is_not_found`
`verified-by: bravebot_agent::conversation::a_line_sent_with_session_excerpts_is_typed_without_the_excerpts`
`verified-by: bravebot_agent::turn::an_address_only_a_named_sessions_excerpt_holds_is_not_one_the_person_typed`
`verified-by: bravebot_agent::turn::an_address_in_a_later_line_of_a_multi_line_prompt_is_one_the_person_typed`
`verified-by: bravebot_agent::turn::an_address_no_typed_line_holds_is_refused_as_not_typed`

<a id="SETLINK-2"></a>
### SETLINK-2: the lines reach the tool in every mode, and the link leaves the turn as text

The driver hands the tool the lines the person typed in every permission mode, not only the one that reads them to decide ([AUTO-3](../permission-modes.md#AUTO-3)). The classifier is given them only in that mode. The tool stores nothing: the accepted address leaves the turn beside the other results, in the order asked, and whoever owns the session record reads it as a link again before keeping it, so a kind asked for twice leaves the last. A link recorded in a turn that then fails still leaves the turn. The terminal and the desktop window both keep it as `/issue` and `/pr` keep theirs, and the panel draws it on the next frame.

The tool is offered only to a turn whose front end keeps the link: the terminal, and the desktop and ACP bridge. The one-shot command line, the plain loop, the goal runner and a manifest run do not keep one, so they are not offered the tool and a call to it is answered as an unknown name. An incognito session saves nothing, so it is not offered the tool either.

`verified-by: bravebot_agent::turn::an_address_the_person_typed_reaches_set_link_and_leaves_the_turn`
`verified-by: bravebot_agent::turn::an_address_the_person_did_not_type_leaves_nothing_in_links`
`verified-by: bravebot_agent::turn::a_link_recorded_in_a_turn_that_then_fails_still_leaves_the_turn`
`verified-by: bravebot_tui::app::a_link_set_by_a_turn_that_failed_reaches_the_record_and_the_panel`
`verified-by: bravebot_tui::app::a_turn_that_set_no_link_leaves_the_links_it_found`
`verified-by: bravebot_ui_bridge::link_tests::a_link_set_by_a_turn_that_failed_reaches_the_record_and_leaves_the_rewind_points`
`verified-by: bravebot_ui_bridge::link_tests::a_turn_that_set_no_link_opens_no_record`
`verified-by: bravebot_agent::turn::a_turn_whose_front_end_keeps_no_link_is_not_offered_set_link`
`verified-by: bravebot_agent::incognito::a_private_session_is_not_offered_set_link`

<a id="SETLINK-3"></a>
### SETLINK-3: only the turn a person is watching can set a link

A delegate is offered no `set_link`, and a call from one is answered as an unknown name is, since a delegate's task is a sentence the driver composed and the lines the person typed are not its to cite. `set_link` is among the names a delegate is never offered.

`verified-by: bravebot_agent::tools::a_delegate_cannot_set_a_link`
`verified-by: bravebot_core::policy::an_addressed_turn_is_offered_only_the_tools_its_definition_named`
`verified-by: bravebot_core::policy::the_tools_no_delegate_is_offered_are_offered_to_an_addressed_turn`

## Known costs

- **Only a line typed in this run of the session counts.** A message restored from a saved session has no recorded origin and is left out, so after a resume the person types the address again, or uses `/issue` and `/pr`. A line sent with files dropped on it is left out as well. An address in a file or a page does not count, and neither does one a program printed.
