---
id: NET
title: Network egress
status: normative
governs:
  - crates/net/src/address.rs
  - crates/net/src/lib.rs
  - crates/net/src/transport.rs
documented-by: docs/website/docs/security/security.md
---

## Scope

Every request this process makes to the network: what has to be true before one leaves, and what
comes back. What the returned bytes are labelled, and what may then be done with them, is
[labels.md](labels.md).

The listener of the ChatGPT sign-in ([CHATGPT-4](chatgpt-sign-in.md#CHATGPT-4)) makes no request, so
no clause here applies to it. It binds `127.0.0.1` and accepts a connection from any program on this
machine, and each answer is a fixed status and a fixed page that repeats nothing from the request.
[NET-10](#NET-10) lets a stream to a server on this machine be waited on without a bound, and says
nothing about a port this process listens on. What the listener accepts is the ChatGPT spec's.

A crate that drives a turn for a surface elsewhere is under these clauses without being named by
them. It builds no client of its own, so it is not a second egress and needs no clause here; what it
does instead is [layering.md](layering.md)'s, and it is narrower than "reaches the network through
the one call": a front end composes no request at all. Asking a service what it serves is the crate
that speaks to that service's job, and a front end asks that crate. Two front ends wanting different
fields out of one answer is a reason to widen what that crate returns, never a reason for either to
assemble the request itself.

**Why it matters here rather than only in the layering.** A front end that composes its own request
decodes its own reply, and decoding a reply off the network is a declassification. So the cost of
the shorter road is a guarded site in a crate that should hold none, and it is paid in
[labels.md](labels.md) rather than here, which is why a reader of this document would not otherwise
see it.

## Clauses

<a id="NET-1"></a>
### NET-1: this process has one way out, and it is not optional

Every outbound request this process makes goes through a single call. The HTTP client is private to
that module and no other crate depends on it, so there is no second path that could skip the gate.

**Why.** This is the whole property, and it is structural rather than a matter of discipline. In
the design this replaces, using the hardened helper was optional and two of three fetchers
bypassed the redirect check.

`verified-by: bravebot_core::policy::network_egress_requires_the_fetch_capability`
`verified-by: bravebot_net::egress::a_successful_fetch_returns_a_labelled_body`
`verified-by: bravebot_net::egress::a_fetch_without_the_capability_is_refused`

<a id="NET-2"></a>
### NET-2: a redirect is revalidated on every hop

Redirects are followed by hand and each new URL is put to the gate before it is fetched. The chain
is bounded, so a loop ends rather than running forever.

**Why.** Following redirects automatically would mean the gate only ever saw the first URL, and a
permitted host could hand off to a denied one.

Revalidating is what makes a hop checkable; what the check consists of depends on why the request is
being made. For a `fetch_url` call it is [FETCH-4](tools/fetch-url.md#FETCH-4), which holds the
chain to the host a person approved. For this program's own connection to its endpoint it is the
capability and nothing more, for the reason that clause gives. Whichever it is, the hop is also held
to the transport the hop before it used, by [NET-9](#NET-9), and for a `fetch_url` call to the
address its host resolves to, by [FETCH-8](tools/fetch-url.md#FETCH-8).

A request may instead refuse redirects (`Request::refusing_redirects`). A redirect in answer to it is
returned as the failure it is, with the status it came with, and nothing is sent to the address it names,
so there is no hop to revalidate. It is for a request that carries a credential to a destination fixed by the
program or by the person's own configuration, such as the serving host of a ChatGPT account
([CHATGPT-2](chatgpt-sign-in.md#CHATGPT-2)) and of an xAI account ([XAI-16](xai-sign-in.md#XAI-16)), and Brave's
premium host, which the premium key is presented to ([PREM-10](premium-credentials.md#PREM-10)).

`verified-by: bravebot_net::egress::every_redirect_hop_is_revalidated`
`verified-by: bravebot_core::policy::a_fetch_cannot_be_redirected_to_a_host_nobody_approved`
`verified-by: bravebot_net::egress::a_redirect_loop_is_bounded`
`verified-by: bravebot_net::lib::redirect_status_codes_are_recognised`
`verified-by: bravebot_net::lib::absolute_redirects_are_used_as_given`
`verified-by: bravebot_net::lib::path_absolute_redirects_keep_the_authority`
`verified-by: bravebot_aichat::client::a_gateway_client_refusing_redirects_goes_nowhere_a_reply_names`
`verified-by: bravebot_aichat::client::a_redirect_in_answer_to_the_xai_listing_is_a_failure_and_goes_nowhere`
`verified-by: bravebot_aichat::client::a_redirect_from_the_premium_host_is_a_failure_and_goes_nowhere`
`verified-by: bravebot_net::lib::relative_redirects_resolve_against_the_parent_path`
`verified-by: bravebot_net::lib::scheme_relative_redirects_keep_the_scheme`
`verified-by: bravebot_net::lib::a_request_refusing_redirects_is_answered_with_the_redirects_status_and_goes_no_further`

<a id="NET-3"></a>
### NET-3: only http and https ever reach the network

Any other scheme is refused before a connection is attempted, rather than handed to a library to
interpret.

**Why.** A URL is routing, and a scheme this code does not understand is a destination nobody
decided on.

`verified-by: bravebot_net::lib::only_http_schemes_are_permitted`
`verified-by: bravebot_net::egress::non_http_schemes_never_reach_the_network`

<a id="NET-4"></a>
### NET-4: a body is capped, and a truncated one says so

Past the cap the body is cut, and the result reports that it was. A body that stops partway is a
failure rather than a short success, and a read that fails is not reported as a short body.

**Why.** Silence would let a caller treat half an answer as the whole one. This is resource
hygiene, not content inspection: the bytes are never parsed to decide anything.

`verified-by: bravebot_net::lib::bodies_are_capped`
`verified-by: bravebot_net::lib::small_bodies_are_not_reported_as_truncated`
`verified-by: bravebot_net::lib::a_failed_read_is_not_a_short_body`
`verified-by: bravebot_net::egress::a_body_that_stops_partway_is_a_failure_rather_than_a_short_body`
`verified-by: bravebot_net::lib::a_streamed_body_that_ends_at_the_cap_is_not_truncated`
`verified-by: bravebot_net::lib::a_streamed_body_past_the_cap_is_cut_and_says_so`

<a id="NET-5"></a>
### NET-5: each phase of a request is bounded separately

Connecting, starting to reply, and continuing to reply are timed apart. A reply that is still
arriving is not cut off for having taken a while to start, and one that stops arriving is given
up on rather than waited for indefinitely.

A request that knows how long its reply can run says so, and is given that long and no longer in
place of the bound on a reply of unstated length. A reply written in full before any of it is sent
is given it for the wait and again for the whole. A stream, which begins at once, is given it from
the moment it begins, and waits to begin no longer than any other reply. The gap allowed between
two pieces of a reply is the same however long the request says its reply may take. A stream from
this machine that asks is the one exception ([NET-10](#NET-10)).

A request may state a gap of its own where the person it is made for chose one
([BACKEND-55](backends.md#BACKEND-55)). That gap replaces the default, longer or shorter, and moves
no other bound: the wait for a reply to begin and the time the whole of it may take stay as the
request and this clause set them, so a gap longer than those leaves the reply to be ended by them.
A stream waited on through any silence ([NET-10](#NET-10)) is held to no gap, stated or not.

A reply cut at the end of its time is reported as out of time, apart from one that stopped arriving,
and is not worth another attempt. It was still being written when it was cut, and asked for again it
is written as long again and billed again. A reply asked for whole within a time its request stated
is out of time too when that time runs out before any of it arrives, since it is written before any
of it is sent. Any other reply that never began is a connection that gave out.

**Why.** One timeout over the whole call cannot tell a slow answer from a dead connection, and
choosing a single number makes one of those two cases wrong. The same holds between replies: a
model writing 128,000 tokens takes about half an hour, and a bound that long on every reply would
leave a request nothing is answering waiting as long. A stream's first bytes come at once, so
waiting longer for them buys nothing. A connection that has gone quiet is the same connection
however long its reply was going to be.

`verified-by: bravebot_net::egress::a_reply_still_arriving_is_not_cut_off_for_taking_longer_than_it_took_to_start`
`verified-by: bravebot_net::egress::a_reply_that_takes_longer_than_the_send_bound_to_start_is_not_a_failed_send`
`verified-by: bravebot_net::egress::a_reply_that_never_comes_gives_up`
`verified-by: bravebot_net::egress::a_reply_that_stops_arriving_is_given_up_on`
`verified-by: bravebot_net::egress::a_request_stating_how_long_its_reply_may_take_is_given_that_long`
`verified-by: bravebot_net::egress::a_reply_outlasting_the_time_its_request_stated_is_given_up_on`
`verified-by: bravebot_net::egress::a_reply_written_before_any_of_it_is_sent_is_waited_on_as_long_as_its_request_stated`
`verified-by: bravebot_net::egress::a_reply_written_before_any_of_it_is_sent_is_out_of_time_when_it_outlasts_its_stated_bound`
`verified-by: bravebot_net::egress::a_stream_is_waited_on_to_begin_no_longer_whatever_its_request_said`
`verified-by: bravebot_net::egress::a_reply_that_stops_arriving_is_given_up_on_however_long_its_request_said_it_may_take`
`verified-by: bravebot_net::egress::a_request_stating_a_longer_gap_between_pieces_is_waited_on_that_long`
`verified-by: bravebot_net::egress::a_request_stating_a_shorter_gap_gives_up_on_a_quiet_reply_sooner`
`verified-by: bravebot_net::egress::a_stated_gap_does_not_end_a_stream_from_this_machine_that_asks_to_be_waited_on`

<a id="NET-6"></a>
### NET-6: a failure that means "not now" may be retried, and nothing else may

A connection that gave out is worth another attempt: a timeout, a socket that went away, a name that
did not resolve, and a network that is down or unreachable. A refusal is not, and only the statuses
that mean the server is temporarily unable are treated as retryable.

A `Retry-After` header on such an answer is read as whole seconds or an IMF-fixdate HTTP date and
carried on the failure as a duration of at most one hour. A value that is neither, a date already
past aside, is absent. It is a number that only sets a timer ([BACKEND-54](backends.md#BACKEND-54)):
the header's text is never kept or shown, and the retryable set above is decided from the status
alone.

**Why.** Retrying a refusal turns one denied request into several. A failed name lookup is retried
whatever it failed on, because a name that does not exist and a network that is not up yet cannot be
told apart by the failure ([Known costs](#known-costs)).

`verified-by: bravebot_net::lib::a_connection_that_gave_out_is_worth_another_attempt_and_a_refusal_is_not`
`verified-by: bravebot_net::lib::a_network_that_is_down_is_worth_another_attempt`
`verified-by: bravebot_net::lib::a_request_whose_name_did_not_resolve_is_worth_another_attempt`
`verified-by: bravebot_net::lib::a_lookup_that_failed_another_way_is_reported_as_it_failed`
`verified-by: bravebot_net::lib::the_systems_own_failed_lookup_is_reported_as_a_host_not_found`
`verified-by: bravebot_net::lib::only_the_statuses_that_mean_not_now_are_worth_another_attempt`
`verified-by: bravebot_net::lib::a_retry_after_is_read_as_seconds_or_a_date_and_bounded`
`verified-by: bravebot_net::lib::a_status_carries_the_wait_the_server_asked_for_and_nothing_it_wrote`
`verified-by: bravebot_net::egress::a_non_success_status_is_an_error`

<a id="NET-7"></a>
### NET-7: what a handshake is validated against is the machine's answer, not this build's alone

A build ships a set of certificate authorities, and the environment may name others in their place:
`SSL_CERT_FILE` for a bundle, `SSL_CERT_DIR` for a directory of them. What they name replaces the
shipped set rather than adding to it. The two paths are read independently, so one that yields no
certificate does not discard what the other held; where neither held anything, nothing is trusted
rather than the shipped set coming back. Which roots are in force, and any named path that yielded
nothing, are reported by `doctor` ([CLI-7](cli.md#CLI-7)), each such path rather than the first of
them.

Where the environment names neither path, a settings file may: `SSL_CERT_FILE` and `SSL_CERT_DIR` in
the `env` block of the person's own layers ([BACKEND-33](backends.md#BACKEND-33)), which are read
once, before the first connection, by whatever entry point goes on to connect. The pair comes whole
from one place: an exported variable naming either path means the file's are not consulted, so a
bundle in a file is never added to an exported directory. `doctor` names the file the roots came
from.

**Why.** A network that inspects TLS presents a certificate from an authority whoever set the
machine up has already installed, and every other client on it honours these two variables. A
program that read neither would refuse every connection over an authority its user had already
decided to trust, and nothing in any diagnostic would point at the cause.

Replacing rather than adding is the decision the rest of the machine makes with these variables: a
bundle is the whole of what to trust, and somebody pinning a private authority has ruled the public
ones out on purpose. Returning to the shipped set when a named path yields nothing would undo that
in silence, and would leave the connection they were trying to fix failing for a reason nothing
states.

Reading the two paths independently is the other side of the same bargain. A machine that names both
commonly has only one of them, and discarding the certificates it does have because a second path
was missing would cost every connection this process makes to save a line in a report. The line is
still owed, because the set in force is then not the set that was asked for. Two paths that yield
nothing are two lines, since the two variables are two decisions: a report naming one of them has
whoever fixes that path run `doctor` again to be told about the other, and the roots line says only
that nothing is trusted.

`verified-by: bravebot_net::transport::an_environment_that_names_no_certificates_leaves_the_built_in_roots_in_force`
`verified-by: bravebot_net::transport::a_certificate_file_the_environment_names_is_what_is_trusted`
`verified-by: bravebot_net::transport::a_certificate_directory_is_read_alongside_a_file`
`verified-by: bravebot_net::transport::a_variable_that_is_set_but_empty_names_nothing`
`verified-by: bravebot_net::transport::the_named_certificates_replace_the_built_in_roots`
`verified-by: bravebot_net::transport::a_certificate_file_that_cannot_be_read_trusts_nothing_and_says_which_path`
`verified-by: bravebot_net::transport::a_certificate_file_holding_no_certificate_is_refused`
`verified-by: bravebot_net::transport::a_certificate_directory_holding_none_is_refused`
`verified-by: bravebot_net::transport::a_directory_entry_that_is_not_a_certificate_is_skipped`
`verified-by: bravebot_net::transport::a_path_that_yields_nothing_does_not_discard_one_that_does`
`verified-by: bravebot_net::transport::every_named_path_that_yields_nothing_is_reported_rather_than_the_first`
`verified-by: bravebot_cli::doctor::the_network_section_names_every_path_that_yielded_nothing`
`verified-by: bravebot_net::transport::a_certificate_file_a_settings_file_names_is_what_is_trusted`
`verified-by: bravebot_net::transport::exported_roots_replace_the_files_rather_than_joining_them`
`verified-by: bravebot_net::transport::a_client_gets_the_built_in_roots_when_nothing_names_others`
`verified-by: bravebot_net::lib::the_one_way_out_is_built_against_the_stated_transport_rather_than_a_default_client`

<a id="NET-8"></a>
### NET-8: a proxy is configured on purpose, and named in the report

The proxy every client in this process uses is read once, from `ALL_PROXY`, `HTTPS_PROXY` and
`HTTP_PROXY` in that order and in either case, with `NO_PROXY` naming the hosts that bypass it, and
is then set on each client explicitly rather than left to a library's default. A proxy whose
protocol this build cannot connect through is not carried at all. `doctor` names the proxy by
protocol, host and port, the hosts it is not used for, and whether it requires a credential, or says
that a proxy was named and is not the route; the credential itself is never printed.

Where the environment names none of the three, the person's settings file may
([BACKEND-33](backends.md#BACKEND-33)), for a process a launcher or a service started with no shell
profile. The answer comes whole from the environment when it names any of them, whether or not that
value parses, so a file's `ALL_PROXY` never outranks an exported `HTTPS_PROXY` by the order the
variables are read in and an exported variable that names no route is reported rather than replaced.
`NO_PROXY` is the one name read on its own: an exported one applies to a route a file stated and the
file's is not consulted beside it, and the file's applies when nothing was exported. A project or
local file's value is never used. `doctor` names the file that stated the proxy, and a checkout's
file that stated one as read and not used, neither with the value.

A variable that is set and non-empty and whose value cannot be read as a proxy address is named too,
one line each, in the order the variables are read and in either case. That covers a value that is
not a uri at all, one naming no authority, and one whose scheme no proxy speaks: the report says the
value could not be read as a proxy rather than that it is not a uri, since `gopher://proxy.corp` is a
uri and naming it as one would send a reader after a typo that is not there. Only the name: the value is withheld on the
same terms the credential is. One line each counts variables and not spellings: where the
environment matches a name without regard to case, as Windows does, both spellings answer with the
one value somebody set and it is named once, under the spelling read first. Two spellings holding
two different values are two variables and are named separately. The reader passes over such a value and goes on to the next variable,
so a later variable may still be the route, and every unparseable variable is named whether or not
one of them was: a variable naming no route is a statement about this machine the program is not
honouring, and `doctor` ends on a configuration error for it as it does for a path that holds no
certificate.

**Why.** Honouring the variables is right: they are how whoever set the machine up states the only
route off it, and a program that ignored them would not connect at all. Inheriting them is not.
Behaviour inherited from a default is a property of a dependency's version rather than a decision
here, and a release that changed that default would silently start or stop routing every request
this process makes through somebody else's machine, with no test to notice.

What a proxy is trusted with is what the person who configured it decided to trust it with. It
carries request bodies, which for this process means conversation content, and it can read them only
where it also terminates TLS, which takes an authority this process trusts and therefore
[NET-7](#NET-7): a second statement by the same person, on the same machine, that every other client
on it is held to as well. A proxy nobody stated sees nothing, and one somebody stated sees what they
already let it see.

A protocol that cannot be connected through is dropped rather than held, because holding it would
make the report name a route no request takes: the transport library answers a proxy it has no
support for by connecting directly, and, for one set rather than read from the environment, by
ending the process at the first connection. Saying that one was named and is not the route is what a
person can act on.

The credential is withheld for the reason every other secret in that report is: a proxy uri carries
a username and password on the networks that require one, and a diagnostic that printed one is a
diagnostic people paste into issues. That one is in use is still said, because a proxy refusing an
unauthenticated request is among the failures the report exists to explain. `NO_PROXY` is named for
the same reason in the other direction: it decides whether a proxy in force applies to the host that
is failing, and `NO_PROXY=*` leaves one configured and used for nothing.

A value that cannot be read as a proxy address is named for the reason an unusable protocol is. The reader's answer to one
is to try the next variable, so the route it settles on and the route somebody wrote down are two
different things and the report otherwise states only the first: a machine whose one way out is a
mistyped `HTTPS_PROXY` says that no proxy was named, or names a lower-priority variable as the route,
and either reads as a denial of the thing that was set. The name alone is enough to act on, and is
all that is printed: a value that failed to parse is as likely to carry a credential as one that
did, since the usual mistake is a uri missing its scheme and the username stays where it was.

`verified-by: bravebot_net::transport::the_proxy_a_client_gets_is_the_one_stated_rather_than_a_library_default`
`verified-by: bravebot_net::transport::a_proxy_is_named_without_the_credential_it_carries`
`verified-by: bravebot_net::transport::a_proxy_without_a_credential_is_not_reported_as_having_one`
`verified-by: bravebot_net::transport::a_proxy_protocol_this_build_cannot_connect_through_is_not_the_route`
`verified-by: bravebot_net::transport::a_proxy_variable_that_cannot_be_parsed_is_named`
`verified-by: bravebot_net::transport::the_values_a_proxy_variable_cannot_hold_are_each_named`
`verified-by: bravebot_net::transport::a_proxy_variable_naming_a_bare_host_is_not_a_problem_to_report`
`verified-by: bravebot_net::transport::a_proxy_variable_that_cannot_be_parsed_is_named_without_its_value`
`verified-by: bravebot_net::transport::a_proxy_variable_that_parses_or_is_empty_is_not_named`
`verified-by: bravebot_net::transport::every_proxy_variable_that_cannot_be_parsed_is_named_in_the_order_they_are_read`
`verified-by: bravebot_net::transport::a_lower_case_proxy_variable_that_cannot_be_parsed_is_named`
`verified-by: bravebot_net::transport::a_proxy_variable_a_case_folding_lookup_answers_twice_is_named_once`
`verified-by: bravebot_net::transport::two_spellings_of_a_proxy_variable_holding_different_values_are_both_named`
`verified-by: bravebot_cli::doctor::the_network_section_names_a_proxy_variable_that_cannot_be_parsed`
`verified-by: bravebot_cli::doctor::the_network_section_names_every_proxy_variable_that_cannot_be_parsed`
`verified-by: bravebot_cli::running::doctor_ends_on_the_configuration_status_for_a_proxy_variable_that_names_no_route`
`verified-by: bravebot_cli::doctor::a_proxy_this_build_cannot_connect_through_is_reported_as_not_the_route`
`verified-by: bravebot_cli::doctor::the_network_section_names_the_hosts_a_proxy_is_not_used_for`
`verified-by: bravebot_cli::doctor::the_network_section_names_the_roots_in_force_and_the_proxy`
`verified-by: bravebot_cli::doctor::the_network_section_never_prints_a_proxy_credential`
`verified-by: bravebot_net::transport::a_proxy_a_settings_file_states_is_the_one_a_client_carries`
`verified-by: bravebot_net::transport::an_exported_proxy_beats_the_file_whatever_names_each_used`
`verified-by: bravebot_net::transport::an_exported_proxy_that_does_not_parse_is_not_replaced_by_the_file`
`verified-by: bravebot_net::transport::a_proxy_in_the_file_that_does_not_parse_is_named_and_is_not_the_route`
`verified-by: bravebot_net::transport::a_file_proxy_honours_the_no_proxy_it_is_given_and_keeps_its_credential`
`verified-by: bravebot_net::transport::an_exported_no_proxy_applies_to_a_file_proxy_in_place_of_the_files`
`verified-by: bravebot_net::transport::a_settings_file_hands_over_the_six_names_and_nothing_else`
`verified-by: bravebot_cli::doctor::the_network_section_names_the_file_that_stated_the_proxy_and_the_one_that_was_ignored`
`verified-by: bravebot_cli::doctor::the_network_section_names_no_file_when_the_environment_decided`

<a id="NET-9"></a>
### NET-9: a redirect may not take an https chain into cleartext

A hop from an https URL to a cleartext http one is refused, on every request that goes out the one
way [NET-1](#NET-1) describes and whatever asked for it. Each hop is held to the transport the hop
before it used, so a chain with no TLS to lose continues and one that picks TLS up part way through
cannot put it down again: what is refused is leaving https, not a hop that changes scheme.

**Why.** Every hop re-sends the whole request, headers and body alike, so a chain that lost TLS would
put on the wire in the clear what the hop before it carried under TLS: this program's credential and
the conversation on its own connection, and on a `fetch_url` call a page a person approved after
reading `https` in the prompt. Checking a hop's host and not its transport passes a downgrade,
because the host is the one that was approved, which leaves an endpoint able to turn its own traffic
into plaintext and hand a third party what only it had.

`verified-by: bravebot_net::lib::a_redirect_may_not_take_an_https_chain_into_cleartext`

<a id="NET-10"></a>
### NET-10: a stream from this machine may take as long as it takes

A request that asks is waited on without the bounds [NET-5](#NET-5) sets from the moment it is sent:
not on its start, its length, or the gaps in it. The ask is honoured only on a stream whose caller
passed a way to stop it, and only where the URL the caller named is on this machine, meaning
`localhost` or a loopback address, and no proxy carries it. Resolving and connecting keep their
bounds, and so does every hop past a redirect. The chat client asks on every streamed reply and on
nothing else, so a chat request that nothing can stop keeps its bounds.

**Why.** Every bound on a reply is there to tell a slow answer from a connection that died without
saying so, and a connection to this machine does not die that way: when the server goes, the socket
says it has. A local model can be silent for a long time. Ollama 0.33, asked for a tool call by a
model writing an image as SVG, sent nothing at all, not even its headers, for 540 seconds, and then
sent the whole call at once. Cut at the gap bound, the turn failed with the call half written. The
bounds a model on this machine would need are those of the slowest machine it runs on, and no one
number is that.

A redirect is named by a server, and how long its target is waited on is not a server's to
lengthen. A proxy is another machine, so a request through one is another machine's request. With
no bound, the only end to a wait on a server that took the request and hung is somebody stopping
it, so a request nobody can stop is not waited on this way. A whole reply is always one of those,
because its body is read to the end and a stop pressed meanwhile is not acted on.

`verified-by: bravebot_net::egress::a_stream_from_this_machine_that_asks_is_waited_on_through_any_silence`
`verified-by: bravebot_net::egress::a_redirect_from_this_machine_keeps_the_bounds`
`verified-by: bravebot_net::egress::a_wait_that_nobody_can_stop_keeps_the_bounds`
`verified-by: bravebot_net::lib::only_this_machine_is_this_machine`
`verified-by: bravebot_net::lib::a_request_through_a_proxy_keeps_its_bounds_when_it_asks_to_be_patient`
`verified-by: bravebot_aichat::client::a_model_on_this_machine_that_goes_quiet_while_it_writes_is_waited_for`
`verified-by: bravebot_aichat::client::a_quiet_local_model_is_not_waited_on_without_bound_by_a_client_nobody_can_stop`

<a id="NET-11"></a>
### NET-11: a client certificate is presented when one is named, and a request is refused when it cannot be

`BRAVEBOT_CLIENT_CERT` names a file holding a certificate (with its chain, where there is one) and
`BRAVEBOT_CLIENT_KEY` a file holding its unencrypted private key, both in the environment and
read once with the other transport settings. Both together are set on the TLS configuration every
client in this process is built from, so a server that asks for a certificate during the handshake
is given this one. A settings file does not state them: only the environment does.

Naming one without the other, a file that cannot be read, one holding no certificate or no key, an
encrypted key, and a key that is not the one that belongs to the certificate are each a problem
reported by path, every one of them rather than the first. While there is one, every TLS handshake
is refused, as it is when [NET-7](#NET-7) leaves nothing trusted, instead of the request going to a
server that would have been given the certificate without it. The pair is checked against itself
once, when it is read: the TLS library panics at the first request on a pair that disagree.
`doctor` ([CLI-7](cli.md#CLI-7)) names the two paths and says they loaded, or names each path that
did not and ends on a configuration error. It never prints the key, and neither does the
transport's `Debug`.

**Why.** A gateway or proxy that requires mutual TLS fails at the handshake with nothing to say
which end refused, and a program that had no way to name a certificate cannot be used behind one.
Refusing rather than connecting without it is the same bargain NET-7 makes for a named authority: a
setting that fails open is one whose failure looks like the server's.

The key is a secret for the reason a proxy credential is under [NET-8](#NET-8): the report exists to
be pasted into an issue, so it holds the path and the fact that it loaded. The passphrase an
encrypted key needs is not read; see Known costs.

`verified-by: bravebot_net::transport::a_certificate_and_key_that_agree_are_what_a_client_presents`
`verified-by: bravebot_net::transport::naming_no_client_certificate_presents_none`
`verified-by: bravebot_net::transport::a_key_that_does_not_belong_to_the_certificate_is_refused_with_its_path`
`verified-by: bravebot_net::transport::a_certificate_file_that_cannot_be_read_is_refused_with_its_path`
`verified-by: bravebot_net::transport::every_client_file_that_cannot_be_used_is_reported`
`verified-by: bravebot_net::transport::a_certificate_named_without_a_key_is_refused_and_so_is_a_key_without_a_certificate`
`verified-by: bravebot_net::transport::an_encrypted_key_is_refused_as_encrypted`
`verified-by: bravebot_net::transport::a_debug_print_of_the_transport_holds_the_paths_and_none_of_the_key`
`verified-by: bravebot_cli::doctor::the_network_section_names_a_client_certificate_that_loaded_and_never_its_key`
`verified-by: bravebot_cli::doctor::the_network_section_names_each_client_file_that_cannot_be_used`

<a id="NET-12"></a>
### NET-12: a request may ask for the whole body of a refusal, and nothing here reads it

A request that asks (`Request::reading_a_refusal`) is answered a non-success status with the body as well,
where the body is no longer than 16 KiB: all of it, labelled as the call labels its replies, in the failure
beside the status and the wait the server asked for. A longer body is not handed over cut, since half of it
would be read as if it were whole, and neither is one that could not be read because the connection broke or
went quiet past [NET-5](#NET-5)'s gap. The failure is then the status alone. A request that does not ask is
answered a non-success status with no body, and a success is answered the same whatever the request asked.

The body is read with the reply, under the bounds of NET-5. A body that goes quiet is given up on at the gap.
Where the request is made with a way to stop it, the read is on the thread a stop abandons, so the caller is
answered at once. A read that fails is written to the diagnostic log as the body failing, beside the status
that is the failure, unless the request was stopped.

This crate does not look at the body. What decides anything from it is the caller, which compares one field
of it with a list of its own, as [CHATGPT-8](chatgpt-sign-in.md#CHATGPT-8) does, and no text made from the
failure (its words, its debug form, the diagnostic log) holds any of it. Only the body of the URL the caller
asked for is handed over. A refusal after a redirect is reported against the URL that was asked for, as
[NET-2](#NET-2) says of a status, and carries no body, so a server's redirect cannot speak for the host the
caller asked. A request that waits on its reply without bound (`Request::patient_on_this_machine`) is handed
none either, since a stop would leave the read blocked with no bound to end it.

**Why.** The status alone cannot say whether asking again will help. A service may refuse a request with
the same 401 for a token that a replacement fixes and for an account that no token reaches, and writes
which in the body. The body is untrusted bytes, so it is handed back under the label the request's call
gave, like any other body, and the one place that reads it states its comparison. It is a request's
choice because a caller that decides nothing from the body has no use for a read of up to 16 KiB on every
failed request.

`verified-by: bravebot_net::egress::a_request_that_asks_is_handed_a_refusals_body_and_one_that_does_not_is_told_the_status_alone`
`verified-by: bravebot_net::egress::a_streamed_request_that_asks_is_handed_a_refusals_body`
`verified-by: bravebot_net::egress::a_refusals_body_is_handed_over_whole_up_to_the_cap_and_not_at_all_past_it`
`verified-by: bravebot_net::egress::a_success_is_not_changed_by_asking_for_a_refusals_body`
`verified-by: bravebot_net::egress::a_refusal_whose_body_stops_partway_is_the_status_alone`
`verified-by: bravebot_net::egress::a_refusal_whose_body_goes_quiet_is_the_status_alone_after_the_gap_bound`
`verified-by: bravebot_net::egress::a_stop_while_a_refusals_body_is_awaited_is_acted_on`
`verified-by: bravebot_net::egress::a_refusal_whose_body_fails_while_it_is_read_is_written_to_the_diagnostic_log`
`verified-by: bravebot_net::egress::a_refusal_after_a_redirect_carries_no_body_and_names_the_url_that_was_asked_for`
`verified-by: bravebot_net::egress::a_request_that_waits_without_bound_is_not_handed_a_refusals_body`
`verified-by: bravebot_net::lib::a_refusals_body_is_in_no_text_made_from_the_failure`

## Known costs

- **A refusal's body is read before its failure is returned** ([NET-12](#NET-12)). A server that sends the
  headers of a refusal and then goes quiet holds the failure for [NET-5](#NET-5)'s gap, two minutes by
  default, and a token replacement or a backoff waits behind it. A stop ends the wait where the request was
  made with a way to stop it, and a request made without one waits it out.
- **`bravebot-net` is not the only crate that opens a socket.** `bravebot-skus` builds HTTP clients
  of its own: one for each of the ChatGPT and xAI sign-ins. That traffic carries an authorization
  code, a device code and tokens, never workspace content or model output, so no labelled value
  escapes the gate. NET-1 is about everything carrying labelled
  content. A second egress in this process that ever carried content would be a violation. Its
  clients are its own; its transport is not. The ChatGPT and xAI sign-ins and the renewal of an
  access token are handed the transport configuration this module resolved, by a caller that
  depends on both crates, so NET-7, NET-8 and NET-11 hold of them too and a machine whose authority
  or route off it is stated in the environment reaches each service on the terms it reaches
  everything else. What stays separate is the policy gate, which those clients have nothing to put
  to, and the redirect loop, so NET-9 does not reach them either. Neither follows a redirect. A
  redirect in answer to a ChatGPT request that carries an authorization code, a verifier or a token
  is a failure, and nothing is sent to the address it names
  ([CHATGPT-2](chatgpt-sign-in.md#CHATGPT-2)). The xAI client follows none either, for a request
  that carries a device code or a refresh token ([XAI-1](xai-sign-in.md#XAI-1)).

  This bullet is the register, and it is read rather than only written: `make check-security` takes
  the crates it names out of it and fails on any manifest under `crates/` that declares a network
  client and is not one of them. So a third client is a red build rather than a paragraph in the
  gate crate assuring a reviewer that it cannot exist, which is what it used to be.

- **A ChatGPT sign-in reaches OpenAI's hosts, each a constant of the source, and
  [CHATGPT-2](chatgpt-sign-in.md#CHATGPT-2) names them.** This spec holds no copy of the addresses. What
  this layer does differs by request. The browser is opened at the authorization page by the sign-in
  command, and this process makes no request to that page. The code exchange, the fetch of the key set
  and the renewal of an access token are made by the `bravebot-skus` clients above, over the shared
  transport, with no redirect followed. A turn's request and the listing of the account's models go
  through the one way out of NET-1 as any model request does, and each refuses redirects
  ([NET-2](#NET-2)) and is handed the body of a refusal ([NET-12](#NET-12)). Which destination takes
  which credential is [CHATGPT-2](chatgpt-sign-in.md#CHATGPT-2)'s, and so is the seam by which a
  debug build with the `test-hosts` feature sends the flow to a stand-in, which a build that ships
  cannot.

- **An xAI sign-in reaches xAI's hosts, each a constant of the source, and [XAI-1](xai-sign-in.md#XAI-1)
  names them.** The code request, the poll and the renewal of an access token are made by the
  `bravebot-skus` clients above, over the shared transport, with no redirect followed. A turn's request
  and the listing of the account's models go through the one way out of NET-1 as any model request does,
  and each refuses redirects ([NET-2](#NET-2)): the request carries the access token and a conversation,
  and a person did not choose the host. Neither asks for the body of a refusal, so none of it is read
  ([XAI-18](xai-sign-in.md#XAI-18)). Which destination takes which token is
  [XAI-1](xai-sign-in.md#XAI-1)'s and [XAI-16](xai-sign-in.md#XAI-16)'s, and so is the seam by which a debug
  build with the `test-hosts` feature sends the flow to a stand-in, which a build that ships cannot.

- **The machine's own trust store is not read.** `SSL_CERT_FILE` and `SSL_CERT_DIR` are, so on a
  machine where neither is set an authority installed into the platform store is invisible here.
  Consulting that store takes a dependency reaching a different system library on each platform,
  and the cross-builds that produce the released binaries compile for platforms they are not
  running on. What the variables cost instead is a line in a shell profile, which is what every
  other client on such a machine already asks for, and `doctor` names them so that the remedy is in
  the report rather than in this document.

- **A certificate directory is read whole, rather than by the links OpenSSL follows.** OpenSSL
  consults `<subject hash>.<n>` in a `CApath` and ignores everything else, so a file left behind
  after its link was removed is no longer trusted there and is still trusted here. Following the
  links means computing a subject hash, which means parsing X.509, which this crate does not do and
  should not start doing to read a directory. Reading every file is what the Rust clients that read
  these variables do.

- **A program this agent starts makes its own requests, and they do not come through here.** `run`
  ([tools/run.md](tools/run.md)) executes programs, and a line the user typed in shell mode
  ([shell-mode.md](shell-mode.md)) goes to a real shell with nothing asked first, so an approved
  `curl`, `git push` or package install reaches the network without passing this gate. Unlike
  `bravebot-skus`, this path can carry the user's own data: a program reads what its profile reaches
  (everything the account can on Windows), so it can read a file in the session and send it. Nothing routes or inspects those
  requests, and the host rules a `fetch_url` call is held to do not reach them. What stands in front
  of the path is the prompt that asks before a program runs and a `Bash` deny rule
  ([permissions.md](permissions.md)), both of which refuse a command rather than govern its traffic.
  Operating-system confinement can govern traffic. A program somebody asked for is confined by
  [sandboxing.md](sandboxing.md) ([SANDBOX-17](sandboxing.md#SANDBOX-17)) as a bound on the paths it
  may reach and not on its egress, because a profile cannot tell an approved `git push` from an
  exfiltration, so this cost stands either way.

- **Within https, a hop on this program's own connection may go to any host.** Every hop re-sends the
  whole request, which on those connections means the `authorization` header the aichat and gateway
  backends carry (`x-goog-api-key` for a Google Vertex service), the `authorization` and `x-amz-security-token` pair a Bedrock request is signed
  with, the `mcp-session-id` an HTTP MCP server issued, and a body holding the conversation. The
  `x-api-key` an Anthropic service takes is not among them: it is attached as a socket header, so a
  hop that leaves the endpoint's own scheme, host and port is sent the conversation and not the key. The
  per-hop check there is the capability and nothing more, for the reason NET-2 gives, so an endpoint
  somebody configured can name any https host in a `Location` header and be sent all of it. NET-9
  bounds what a downgrade costs, not where a hop may land. What stands in front of this is that the
  endpoint is one a person chose and already sends every request to; what would bound it is holding
  those chains to a host as well, which is a decision about which hosts a configured endpoint may
  redirect to and is not made here. A request to the serving host of a ChatGPT account or of an xAI
  account is the exception: it carries the access token, and a person did not choose that host, so it
  refuses redirects ([NET-2](#NET-2), [CHATGPT-2](chatgpt-sign-in.md#CHATGPT-2),
  [XAI-16](xai-sign-in.md#XAI-16)).

- **Two pairs of phases share a bound rather than having one each.** The transport gives a phase
  the earliest of its own deadline and those of the phases before it, so a bound tight enough to
  time one phase precisely cuts the phase after it short as well. Each number it is given
  therefore covers every phase it governs: resolving a name is allowed as long as connecting, and
  writing a request body as long as the send and reply bounds together. Both are resolved in
  favour of the later phase, because that is what the clause is for. A slow answer must not be
  read as a dead connection, and timing the phase before it precisely at the price of cutting the
  answer short would be exactly the failure this clause forbids.

- **The bound on a whole reply can be overrun by a second at a time.** The transport ends a reply
  when its deadline passes during a read. A read that begins after the deadline is given one more
  second instead of none, so a reply whose pieces come less than a second apart, and whose deadline
  passes between two reads rather than during one, runs on. A reply streamed by a model is waiting
  on its next piece almost all the time, so its deadline nearly always passes during a read. What
  would close it is checking the deadline between reads here, which is not done.

- **A server on this machine that never answers is waited on until somebody stops it.** A stream
  under [NET-10](#NET-10) has no bound after it is sent, so a local server that took the request and
  then hung holds the turn until a person presses stop, and the thread reading it until the server
  lets go. So does a port on this machine forwarded to another one, a tunnel for instance, whose far
  end went away without the forward noticing. A bound long enough for the slowest local model would
  still be a guess, and one that ran out would cut a working reply, which is the failure the clause
  is there to remove.

- **A request that cannot succeed is sent again when its failure looks like a network that is down.**
  Nothing readable from a failed lookup says whether the name does not exist or the network is not
  up yet, since `std` reports the lookup's own error code as a kind that cannot be matched, and a
  machine without a network can fail a lookup the way it fails one for a name that does not exist.
  So a mistyped host is sent three times, after a pause of one second and then two, before it is
  reported. A lookup that failed for a cause of its own, a descriptor limit for example, is reported
  as a host that could not be found, and so is a proxy that does not resolve, since ureq resolves
  its host through the same lookup. `NetworkUnreachable` and `HostUnreachable` name one route as well
  as a whole network, and the transport stops at the first address that fails other than by being
  refused, so a name whose first address is IPv6, on a network with no IPv6 route, fails the same way
  on each attempt. A request that fails for any of these reasons ends after three attempts with the
  same failure it would have ended with on the first.

- **The certificate is offered to any https host that asks for one, and no test here has a server
  that does.** The TLS library presents it to whichever server requests a certificate during a
  handshake, and nothing records the host it was configured for, so an endpoint that redirects to
  another host ([NET-2](#NET-2) revalidates the hop and no more) is offered it there too. The
  clients built by `bravebot-skus`, for the ChatGPT and xAI sign-ins, are
  handed the same configuration and present it alike. What would bound it is a setting naming the
  hosts, which is not decided here. Pinning that a server demanding a certificate receives this one
  needs a TLS server in the test, and `bravebot-net` has no dependency that is one; the tests pin what
  is handed to the client's configuration, and the change was observed against
  `openssl s_server -Verify`.

- **An encrypted key is refused, not decrypted.** Decrypting a key means a dependency this build
  does not have, so no passphrase is read from the environment or the credential store either. A key that needs one is reported by path as encrypted, and the file has to be
  given an unencrypted copy readable only by its owner.

- **The pair is checked by a stand-in connection.** The TLS library offers no way to ask whether a
  pair agree except building a configuration, which panics when they do not. The check does that
  on a connection that carries nothing, catches the panic and withholds its report for that thread.
  A library release that stopped panicking would still be caught, because any error other than the
  stand-in's own counts as a pair that cannot be used.
