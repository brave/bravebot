---
id: WRITE
title: write_file
status: normative
governs:
  - crates/agent/src/workspace.rs
documented-by: docs/website/docs/reference/tools.md
---

## Scope

Writing a whole file. `path` and `contents_ref` are routing; `contents` is content. The result is a
confirmation.

## Clauses

<a id="WRITE-1"></a>
### WRITE-1: contents or a reference, never both

`contents_ref` names quarantined content that becomes the whole file. It is **routing**, since it
decides which bytes the write carries, and it is a name the driver handed out rather than anything
derived from content.

Exactly one of the two. Both would leave the driver choosing between text the planner wrote and
bytes nobody has read, and neither names anything to write, so both shapes are refused before
anyone is asked to approve a write.

A blank `contents_ref`, `path` or `path_ref` counts as left out, since an empty string names nothing.
Planners that fill every optional field send the unused ones as `""`, and refusing those as "not
both" gave them a refusal they could not act on. A blank `contents` is still given: it is an empty
file.

**Why.** The worst a wrong reference can do is put the wrong quarantined bytes into a path that
still had to be endorsed on its own.

`verified-by: bravebot_agent::turn::a_quarantined_file_is_rewritten_by_a_processor`
`verified-by: bravebot_agent::turn::a_write_that_names_two_bodies_or_none_is_refused`
`verified-by: bravebot_agent::turn::a_write_with_blank_references_beside_its_path_and_contents_goes_through`

<a id="WRITE-2"></a>
### WRITE-2: a reference that names no file is not a destination

Everything a processor produced is such a reference. That refusal is what stops untrusted text
choosing where an effect lands.

`verified-by: bravebot_agent::turn::a_processors_output_cannot_be_a_destination`
`verified-by: bravebot_core::policy::a_reference_that_names_no_file_is_not_a_destination`

<a id="WRITE-3"></a>
### WRITE-3: the planner never chooses a destination on its own

A write needs a person's approval, which mints a single-use endorsement bound to that exact path,
so it cannot be replayed or redirected. Where nobody can be asked, writes are refused rather than
applied unseen.

**Why.** The wrong file destroys work rather than wasting a step.

`verified-by: bravebot_agent::turn::an_approved_write_is_recorded_as_endorsed`
`verified-by: bravebot_agent::turn::a_refused_write_does_not_happen`
`verified-by: bravebot_agent::turn::a_refused_overwrite_leaves_the_original`
`verified-by: bravebot_agent::turn::an_approved_write_cannot_escape_the_workspace`
`verified-by: bravebot_agent::turn::an_approved_write_does_not_authorise_a_write_somewhere_else`
`verified-by: bravebot_core::policy::an_unendorsed_destination_is_refused`
`verified-by: bravebot_core::policy::an_endorsement_does_not_outlive_the_destination_it_authorised`
`verified-by: bravebot_core::policy::an_endorsement_does_not_authorise_a_different_destination`

<a id="WRITE-4"></a>
### WRITE-4: a write through a reference is always shown

Even where the trust map would not ask. The person approving is shown the path.

**Why.** The approval is the only moment the filename is visible to anybody. They own the
directory and are the only party who can say whether that file should be rewritten.

`verified-by: bravebot_agent::turn::every_write_through_a_reference_is_shown`
`verified-by: bravebot_agent::turn::a_write_through_a_reference_says_what_landed_and_that_it_is_done`
`verified-by: bravebot_agent::turn::a_reference_write_is_reviewed_as_a_diff`

<a id="WRITE-5"></a>
### WRITE-5: the result says what a running language server found wrong

Where a language server for the file's language is already running, the result of a write adds
the line numbers of the errors it reports, as [LSP-12](lsp.md#LSP-12) specifies: counts and lines
in this repository's words, never the server's, never started by the write, and given only for a
file the trust map vouches for after the write.

**Why.** A planner that finds a syntax error only by running a build spends a round and a second
approval on something the server already knew.

`verified-by: bravebot_agent::lsp::a_write_reports_the_error_lines_a_running_server_found`
`verified-by: bravebot_agent::lsp::a_write_starts_no_language_server`
`verified-by: bravebot_agent::lsp::a_write_of_quarantined_bytes_reports_no_diagnostics`
