---
id: DOWNLOAD
title: download_url
status: normative
governs:
  - crates/agent/src/tools.rs
  - crates/agent/src/workspace.rs
  - crates/net/src/lib.rs
  - crates/config/src/settings.rs
documented-by: docs/website/docs/reference/tools.md
---

## Scope

Saving what an http or https URL serves to a file, byte for byte. [fetch-url.md](fetch-url.md)
returns a reference to a body decoded lossily and capped at 2 MiB, which corrupts a picture, an
archive, a PDF or a font. `url` and `path` are routing; the only content argument is the `why`
every tool takes ([TOOL-5](tool-surface.md#TOOL-5)). The result is a sentence.

## Why both arguments are admissible as routing

A URL is one destination and a path is one destination, and a person can read each and answer for
it ([TOOL-2](tool-surface.md#TOOL-2)). The bytes between them are carried to the file and never
consulted.

## Clauses

<a id="DOWNLOAD-1"></a>
### DOWNLOAD-1: the bytes go from the socket to the file and nothing holds them

The body is read in pieces. Each piece carries the label of a fetched body, which is untrusted and
public and which no approval or header changes ([FETCH-1](fetch-url.md#FETCH-1)), and is written to
a file beside the destination. The driver never holds the whole body and never reads a piece. The
planner is told the path and the number of bytes and gets no reference, because nothing was kept
in memory to refer to.

The staged file replaces the destination only when the whole body has arrived. A body that stops
half-way, is cut by the cap or is stopped by the person leaves what was at the destination and no
partial file.

**Why.** A file's bytes are what an attacker controls. Writing them as they arrive keeps the rule
that untrusted content is carried and never decides anything, and keeping the destination until
the end means a failed download cannot destroy a file a person approved replacing.

`verified-by: bravebot_agent::turn::a_download_saves_every_byte_exactly`
`verified-by: bravebot_agent::turn::a_downloaded_file_is_untrusted_and_nothing_of_it_reaches_the_planner`
`verified-by: bravebot_agent::turn::a_download_over_the_cap_saves_nothing_and_one_at_it_is_saved`

<a id="DOWNLOAD-2"></a>
### DOWNLOAD-2: the host is asked about as a fetch's is, and the destination every time

The URL goes through [FETCH-2](fetch-url.md#FETCH-2): a `deny` rule refuses without asking, an
`allow` rule answers the host question, and anything else is put to a person. Every hop is held to
the host that was approved ([FETCH-4](fetch-url.md#FETCH-4)) and a name that resolves to a
non-public address is refused ([FETCH-8](fetch-url.md#FETCH-8)). The request carries no `Accept`
preference of its own, since [FETCH-7](fetch-url.md#FETCH-7) is for a page a processor reads.

The destination goes through the write gate of [WRITE-3](write-file.md#WRITE-3): the path must be
endorsed by a person and the endorsement is single-use and bound to that path. The question is
put whatever the trust map says about the path, and shows the pair `url -> path`. An existing file
is shown as an overwrite. Nothing is sent before it is answered, and a decline sends nothing.

`verified-by: bravebot_agent::turn::a_download_asks_about_the_host_and_then_shows_url_and_path`
`verified-by: bravebot_agent::turn::a_download_over_an_existing_file_is_put_as_an_overwrite`
`verified-by: bravebot_agent::turn::a_declined_destination_sends_no_request_and_leaves_the_file`
`verified-by: bravebot_agent::turn::a_download_from_a_denied_host_is_refused_without_asking`
`verified-by: bravebot_agent::turn::a_download_redirected_to_another_host_saves_nothing`

<a id="DOWNLOAD-3"></a>
### DOWNLOAD-3: the file is recorded as not vouched for, and the server's words go to the person only

The file is recorded in the trust map as untrusted, as a write of a fetched body is, so a later
read of it is quarantined. The result gives the planner the path, the byte count and the HTTP
status. The content type the server sent is shown to the person in the transcript with everything
but printable ASCII removed and cut to 100 characters, and is not part of what the planner is
told.

**Why.** A header is the server's own text. Printed raw it can carry terminal escape sequences,
and given to the planner it is the one channel [FETCH-6](fetch-url.md#FETCH-6) closes for a body.

`verified-by: bravebot_agent::turn::a_downloaded_file_is_untrusted_and_nothing_of_it_reaches_the_planner`
`verified-by: bravebot_agent::tools::a_content_type_is_shown_without_anything_a_terminal_would_obey`

<a id="DOWNLOAD-4"></a>
### DOWNLOAD-4: the size cap is a settings value, and a body over it saves nothing

`download.maxBytes` in `settings.json` is the most one call may write. Zero and anything that is
not a whole count are absence, and the nearest layer that names one wins. Where nobody names one
the cap is 100 MiB. A body over the cap is not saved and the planner is told which setting names
the limit; a body exactly at it is saved whole.

**Why.** The cap is a disk budget and so a person's to set. Half a file is worse than none, so the
body is read to the cap plus one byte to tell the two cases apart.

`verified-by: bravebot_config::settings::a_settings_file_names_the_most_a_download_may_write`
`verified-by: bravebot_net::lib::a_stream_raised_or_lowered_is_cut_at_the_figure_it_was_given`
`verified-by: bravebot_agent::turn::a_download_over_the_cap_saves_nothing_and_one_at_it_is_saved`

<a id="DOWNLOAD-5"></a>
### DOWNLOAD-5: a delegate does not hold it, and plan mode refuses it

Every delegate kind holds the capability for reaching the network so the driver can make its model
call, and that is all it buys, so `download_url` is in the list of tools no delegate is offered, as
`fetch_url` is. A call from one is answered as an unknown name. It is a write, so plan mode
refuses it before anything is read or sent ([MODE-3](../permission-modes.md#MODE-3)).

`verified-by: bravebot_agent::tools::no_kind_is_offered_a_tool_that_reaches_the_network`
`verified-by: bravebot_agent::turn::plan_mode_refuses_a_download_before_anything_is_sent`
