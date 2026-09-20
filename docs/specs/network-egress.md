---
id: NET
title: Network egress
status: normative
governs:
  - crates/net/src/lib.rs
  - crates/net/src/transport.rs
documented-by: docs/website/docs/security/security.md
---

## Scope

Every request this process makes to the network: what has to be true before one leaves, and what
comes back. What the returned bytes are labelled, and what may then be done with them, is
[labels.md](labels.md).

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
to the transport the hop before it used, by [NET-9](#NET-9).

`verified-by: bravebot_net::egress::every_redirect_hop_is_revalidated`
`verified-by: bravebot_core::policy::a_fetch_cannot_be_redirected_to_a_host_nobody_approved`
`verified-by: bravebot_net::egress::a_redirect_loop_is_bounded`
`verified-by: bravebot_net::lib::redirect_status_codes_are_recognised`
`verified-by: bravebot_net::lib::absolute_redirects_are_used_as_given`
`verified-by: bravebot_net::lib::path_absolute_redirects_keep_the_authority`
`verified-by: bravebot_net::lib::relative_redirects_resolve_against_the_parent_path`
`verified-by: bravebot_net::lib::scheme_relative_redirects_keep_the_scheme`

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

**Why.** One timeout over the whole call cannot tell a slow answer from a dead connection, and
choosing a single number makes one of those two cases wrong.

`verified-by: bravebot_net::egress::a_reply_still_arriving_is_not_cut_off_for_taking_longer_than_it_took_to_start`
`verified-by: bravebot_net::egress::a_reply_that_takes_longer_than_the_send_bound_to_start_is_not_a_failed_send`
`verified-by: bravebot_net::egress::a_reply_that_never_comes_gives_up`
`verified-by: bravebot_net::egress::a_reply_that_stops_arriving_is_given_up_on`

<a id="NET-6"></a>
### NET-6: a failure that means "not now" may be retried, and nothing else may

A connection that gave out is worth another attempt. A refusal is not, and only the statuses that
mean the server is temporarily unable are treated as retryable.

**Why.** Retrying a refusal turns one denied request into several.

`verified-by: bravebot_net::lib::a_connection_that_gave_out_is_worth_another_attempt_and_a_refusal_is_not`
`verified-by: bravebot_net::lib::only_the_statuses_that_mean_not_now_are_worth_another_attempt`
`verified-by: bravebot_net::egress::a_non_success_status_is_an_error`

<a id="NET-7"></a>
### NET-7: what a handshake is validated against is the machine's answer, not this build's alone

A build ships a set of certificate authorities, and the environment may name others in their place:
`SSL_CERT_FILE` for a bundle, `SSL_CERT_DIR` for a directory of them. What they name replaces the
shipped set rather than adding to it. The two paths are read independently, so one that yields no
certificate does not discard what the other held; where neither held anything, nothing is trusted
rather than the shipped set coming back. Which roots are in force, and any named path that yielded
nothing, are reported by `doctor` ([CLI-7](cli.md#CLI-7)).

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
still owed, because the set in force is then not the set that was asked for.

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

`verified-by: bravebot_net::transport::the_proxy_a_client_gets_is_the_one_stated_rather_than_a_library_default`
`verified-by: bravebot_net::transport::a_proxy_is_named_without_the_credential_it_carries`
`verified-by: bravebot_net::transport::a_proxy_without_a_credential_is_not_reported_as_having_one`
`verified-by: bravebot_net::transport::a_proxy_protocol_this_build_cannot_connect_through_is_not_the_route`
`verified-by: bravebot_cli::main::a_proxy_this_build_cannot_connect_through_is_reported_as_not_the_route`
`verified-by: bravebot_cli::main::the_network_section_names_the_hosts_a_proxy_is_not_used_for`
`verified-by: bravebot_cli::main::the_network_section_names_the_roots_in_force_and_the_proxy`
`verified-by: bravebot_cli::main::the_network_section_never_prints_a_proxy_credential`

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

## Known costs

- **`bravebot-net` is not the only crate that opens a socket.** `bravebot-skus` builds its own
  HTTP client for the subscription service. That traffic carries credentials and an order id,
  never workspace content or model output, so no labelled value escapes the gate. NET-1 is about
  everything carrying labelled content. A second egress in this process that ever carried content
  would be a violation. Its client is its own; its transport is not. `register` is handed the
  transport configuration this module resolved, by a caller that depends on both crates, so NET-7
  and NET-8 hold of it too and a machine whose authority or route off it is stated in the
  environment reaches the subscription service on the terms it reaches everything else. What stays
  separate is the policy gate, which that client has nothing to put to, and the redirect loop, so
  NET-9 does not reach it either: that client follows its hops through its library's default, which
  refuses nothing, so the subscription service can redirect its own traffic into cleartext and put an
  order id and a credential on the wire. Holding it to the same rule means either following its
  redirects here or configuring that client to follow none, and neither is decided in this document.

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
  `bravebot-skus`, this path can carry the user's own data: a program runs with the access their
  shell would give it, so it can read a private file and send it. Nothing routes or inspects those
  requests, and the host rules a `fetch_url` call is held to do not reach them. What stands in front
  of the path is the prompt that asks before a program runs and a `Bash` deny rule
  ([permissions.md](permissions.md)), both of which refuse a command rather than govern its traffic.
  Operating-system confinement can govern traffic, and is applied to the stdio servers it exists for
  rather than to a program somebody asked for. Confining one is decided in
  [sandboxing.md](sandboxing.md) as a bound on the paths it may reach and not on its egress, because
  a profile cannot tell an approved `git push` from an exfiltration, so this cost stands either way.

- **Within https, a hop on this program's own connection may go to any host.** Every hop re-sends the
  whole request, which on those connections means the `authorization` header the aichat and gateway
  backends carry, the `authorization` and `x-amz-security-token` pair a Bedrock request is signed
  with, the `mcp-session-id` an HTTP MCP server issued, and a body holding the conversation. The
  per-hop check there is the capability and nothing more, for the reason NET-2 gives, so an endpoint
  somebody configured can name any https host in a `Location` header and be sent all of it. NET-9
  bounds what a downgrade costs, not where a hop may land. What stands in front of this is that the
  endpoint is one a person chose and already sends every request to; what would bound it is holding
  those chains to a host as well, which is a decision about which hosts a configured endpoint may
  redirect to and is not made here.

- **Two pairs of phases share a bound rather than having one each.** The transport gives a phase
  the earliest of its own deadline and those of the phases before it, so a bound tight enough to
  time one phase precisely cuts the phase after it short as well. Each number it is given
  therefore covers every phase it governs: resolving a name is allowed as long as connecting, and
  writing a request body as long as the send and reply bounds together. Both are resolved in
  favour of the later phase, because that is what the clause is for. A slow answer must not be
  read as a dead connection, and timing the phase before it precisely at the price of cutting the
  answer short would be exactly the failure this clause forbids.
