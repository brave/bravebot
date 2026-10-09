---
id: TODO
title: todo_write
status: normative
governs:
  - crates/core/src/todo.rs
documented-by: docs/website/docs/reference/tools.md
---

## Scope

Recording the planner's own plan. `todos` is content, and there is no routing at all. The result is
a confirmation. The latest accepted list is sent again after a compaction
([COMPACT-16](../compaction.md#COMPACT-16)).

## Clauses

<a id="TODO-1"></a>
### TODO-1: there is no routing, because nothing is touched

The plan is shown to the user and reaches nothing else. It is the one tool with no destination.
The call holds no capability, decides no destination, and touches no path.

`verified-by: bravebot_agent::tools::the_task_list_tool_offers_no_argument_that_names_a_destination`
`verified-by: bravebot_agent::tools::a_task_list_decides_no_destination_and_needs_no_capability`

<a id="TODO-2"></a>
### TODO-2: an unrecognised status reads as outstanding work

A task is struck through only when it is no longer outstanding, and anything the driver does not
recognise counts as unfinished.

**Why.** Showing work as done on the strength of a word nobody recognised would misreport what
happened.

`verified-by: bravebot_core::todo::an_unknown_status_is_outstanding_work`
`verified-by: bravebot_core::todo::outstanding_tasks_are_not_struck_whether_started_or_not`
`verified-by: bravebot_tui::render::outstanding_tasks_are_not_struck_through`
`verified-by: bravebot_agent::tools::an_unrecognised_status_shows_as_outstanding`

<a id="TODO-3"></a>
### TODO-3: a cancelled task is struck through and is not counted as done

`cancelled` is one of the statuses the planner is told to use. A task carrying it is struck
through, drawn with a marker of its own rather than the finished one, and counted apart from the
finished tasks in what the planner is told back.

**Why.** Work dropped is not work completed. Counting it as done would overstate what the turn
achieved, and leaving it unstruck would leave it reading as work still to come.

`verified-by: bravebot_core::todo::a_cancelled_task_is_struck_through_with_its_own_marker`
`verified-by: bravebot_core::todo::a_cancelled_task_is_not_counted_as_done`
`verified-by: bravebot_agent::tools::a_cancelled_task_is_counted_apart_from_the_finished_ones`
`verified-by: bravebot_tui::render::a_cancelled_task_is_struck_without_the_finished_colour`
`verified-by: bravebot_ui_bridge::wire::a_cancelled_task_crosses_as_cancelled`
