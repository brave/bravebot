---
id: LISTEN
title: The device listener
status: normative
governs:
  - crates/core/src/device.rs
  - crates/qr/src/lib.rs
  - crates/qr/src/galois.rs
  - crates/qr/src/layout.rs
  - crates/qr/src/render.rs
  - crates/qr/src/tests.rs
  - crates/listener/src/lib.rs
  - crates/listener/src/der.rs
  - crates/listener/src/identity.rs
  - crates/listener/src/invitation.rs
  - crates/listener/src/limits.rs
  - crates/listener/src/pairing.rs
  - crates/listener/src/pairing_tests.rs
  - crates/listener/src/phone.rs
  - crates/listener/examples/phone.rs
  - crates/listener/src/registry.rs
  - crates/listener/src/revocation_tests.rs
  - crates/listener/src/server.rs
  - crates/listener/src/serving_tests.rs
  - crates/listener/src/tls.rs
  - crates/ui-bridge/src/device_link.rs
  - crates/ui-bridge/tests/device_listener.rs
documented-by: none (internal: the listener is started by a person from the host's command line and no page says how a phone reaches it yet)
---

## Scope

How the host accepts a connection from a phone: the keys each end names the other by, which connection is served and which is closed, what is decided before anything a device sent is read, and how a phone is paired and what it is answered. The listener is a library the host starts and gives a service to answer with. It depends on no crate that opens a session, so a device is answered only what that service says.

Not here: which lines a device may send once a real session is shown to it, and what a session shows a device.

## Clauses

<a id="LISTEN-1"></a>
### LISTEN-1: a device is served over TLS 1.3 with keys that name each end

The listener binds one address that its caller names, and serves each connection on a thread of its own. A connection is TLS 1.3 and no other version. The host shows a self-signed certificate for a P-256 key that it made in memory when it started, and writes that key nowhere. The listener asks the device for a certificate and requires one, and takes any that holds a P-256 key, whoever signed it. A key is named by the SHA-256 hash of its SubjectPublicKeyInfo. That is how a device tells the host's key from another program's, since the person's invitation gives it the hash, and how the host tells one device from another. A certificate that holds another key type, another curve or a point that is not in uncompressed form names no key, and its connection ends. A certificate that is cut short or has a byte changed is read without a panic, and names no key or names the key it holds.

Whether a key is a device is the registry's answer, asked once the handshake is over. The registry is in this process's memory and nowhere else, and a host that restarts has no devices. The listener sends no session ticket and keeps no session, so every connection is a full handshake, and a device that was never paired, or was revoked, cannot return on a ticket it kept. A device has one connection. A connection it makes ends the one it had. Stopping the listener ends every connection, including one that has not finished its handshake or sent its first line, and accepts no more.

The certificate is written by this crate, a few dozen bytes of DER, so no certificate library is a dependency of the host. The certificate's own validity dates are not read by either side, since a certificate here stands for a key.

`verified-by: bravebot_listener::serving_tests::a_paired_phone_is_answered_and_the_service_is_given_its_record`
`verified-by: bravebot_listener::serving_tests::a_phone_pinned_to_another_key_refuses_the_host`
`verified-by: bravebot_listener::serving_tests::a_second_connection_is_a_full_handshake_and_not_a_resumed_session`
`verified-by: bravebot_listener::serving_tests::a_new_connection_of_a_device_ends_its_old_one`
`verified-by: bravebot_listener::serving_tests::stopping_ends_every_connection_and_accepts_no_more`
`verified-by: bravebot_listener::serving_tests::stopping_ends_a_connection_still_in_its_handshake`
`verified-by: bravebot_listener::identity::the_certificate_is_one_the_tls_library_reads_and_holds_the_key_that_signs`
`verified-by: bravebot_listener::identity::a_key_is_named_by_the_hash_of_its_own_public_key_info`
`verified-by: bravebot_listener::identity::the_name_in_the_certificate_does_not_change_the_hash_of_its_key`
`verified-by: bravebot_listener::identity::a_key_that_is_not_p256_is_not_named`
`verified-by: bravebot_listener::identity::a_certificate_that_is_cut_or_changed_is_read_without_a_panic`
`verified-by: bravebot_listener::identity::the_key_named_is_the_key_the_signature_is_checked_against`
`verified-by: bravebot_listener::identity::the_debug_form_of_an_identity_names_the_key_and_holds_back_the_private_key`
`verified-by: bravebot_listener::der::an_element_is_read_back_at_each_length_form`
`verified-by: bravebot_listener::der::bytes_that_do_not_hold_an_element_are_not_read`

<a id="LISTEN-2"></a>
### LISTEN-2: a connection that is not a paired device is closed with nothing sent, and what one may use before it is a device is bounded

A connection whose key the registry does not hold, one that shows no certificate and one whose handshake does not finish is closed, and nothing is written to it after the handshake. What it sends reaches no service. The limits are global and not per address. At most 16 connections are open at once and at most 4 have not finished the handshake, and a connection past either is closed as it arrives. A handshake that is not finished 10 seconds after the connection is accepted ends it, however slowly bytes keep arriving. A line is at most 8 MiB, newline excluded: a line of exactly that length is read, one that reaches it without ending ends the connection and is not answered, and bytes that end without a newline are not a request. A line that is not text ends the connection, and a blank line is skipped. A write to a paired device that makes no progress for five seconds ends the connection, and the writes of a handshake are held to the time of the handshake. An address that is in use is reported by the call that starts the listener, which tries no other.

`verified-by: bravebot_listener::serving_tests::an_unknown_phone_is_closed_with_nothing_sent_and_reaches_no_service`
`verified-by: bravebot_listener::serving_tests::a_phone_that_shows_no_certificate_reaches_no_service`
`verified-by: bravebot_listener::serving_tests::a_flood_of_silent_connections_is_closed_past_the_limit_and_does_not_keep_a_phone_out`
`verified-by: bravebot_listener::serving_tests::a_connection_past_the_limit_of_open_connections_is_closed_as_it_arrives`
`verified-by: bravebot_listener::serving_tests::a_device_that_stops_reading_has_its_connection_ended_when_a_write_makes_no_progress`
`verified-by: bravebot_listener::serving_tests::a_stalled_handshake_is_closed_when_its_time_is_up`
`verified-by: bravebot_listener::serving_tests::a_handshake_that_trickles_bytes_is_closed_when_its_time_is_up`
`verified-by: bravebot_listener::serving_tests::a_line_past_the_limit_ends_the_connection_and_one_at_it_is_read`
`verified-by: bravebot_listener::serving_tests::a_line_that_is_not_text_ends_the_connection_and_a_blank_one_is_skipped`
`verified-by: bravebot_listener::serving_tests::an_address_already_in_use_is_reported`

<a id="LISTEN-3"></a>
### LISTEN-3: a line is answered only after the host has asked whether the device may do what it asks

The service reads a line first, taking as long as that takes with no lock held, and what it returns is run only if the registry then says the device may do what the line asks. The registry decides that from what the person did on the host and from no byte of the line. A device the registry holds as paired with a grant that covers the operation is allowed. A device with no record, a revoked device whatever grant it held, and a `read` grant asked to control are refused, and the connection ends with nothing sent. The service says, for each line it reads, whether the line asks to read or to control, and the registry is asked about that operation. Which requests are which is decided where requests are offered to a device ([LISTEN-6](#LISTEN-6)), and not by the listener.

The check and the answer, which only builds the lines to send, are made in one hold of the registry's lock. A change the person makes on the host, such as pairing a device again with another grant, waits for a request being answered and so falls before it or after it, never between its check and its answer. The decision is `Standing::authorizes` in `bravebot-core`, which Verus proves equal to its statement. That statement says a request is allowed only for a paired device whose grant covers it.

`verified-by: bravebot_core::device::each_standing_gets_the_verdict_the_table_gives`
`verified-by: bravebot_core::device::a_device_the_host_holds_no_grant_for_is_never_allowed`
`verified-by: bravebot_core::device::a_read_grant_never_allows_control`
`verified-by: bravebot_core::spec::a_request_is_allowed_only_for_a_paired_device_whose_grant_covers_it`
`verified-by: bravebot_listener::registry::a_request_is_answered_for_a_held_device_and_not_computed_for_any_other`
`verified-by: bravebot_listener::serving_tests::a_line_that_asks_to_control_is_answered_only_for_a_device_whose_grant_is_control`
`verified-by: bravebot_listener::registry::a_change_on_the_host_waits_for_the_request_being_answered`
`verified-by: bravebot_listener::registry::a_device_is_added_under_a_name_nobody_else_has`
`verified-by: bravebot_listener::registry::a_name_is_one_to_thirty_two_of_lower_case_digits_and_hyphens`

<a id="LISTEN-4"></a>
### LISTEN-4: a code the host shows is a QR code a phone's own reader reads

The host draws an invitation as a QR code in the terminal, from a crate that does no I/O and has no dependency. The code is QR Model 2 in byte mode at error correction level M, in the smallest of versions 1 to 10 that holds the text, which is up to 213 bytes; a longer text is refused. Its lines set black on white themselves, with a quiet zone of four modules, and reset afterwards, because a scanner reads dark modules on a light ground whatever colours the terminal uses. Two rows of modules share one line of text.

Each of these is held by a reader written from the standard's description of a square and not from the encoder's code: the error correction of the standard's own example, the format and version information the standard tabulates, every length from 0 to 213 bytes read back to the bytes it was made from with every block's error correction checking, and the fixed patterns where the standard puts them. The squares the encoder draws for a short text, for one of each of several sizes, are ones the operating system's own QR reader decodes.

`verified-by: bravebot_qr::tests::the_error_correction_of_the_standards_example_is_the_one_it_prints`
`verified-by: bravebot_qr::tests::a_product_in_the_field_is_what_the_polynomial_arithmetic_gives`
`verified-by: bravebot_qr::tests::every_length_up_to_the_limit_reads_back_as_it_was_made`
`verified-by: bravebot_qr::tests::the_smallest_version_that_holds_the_text_is_used_and_more_than_the_limit_is_refused`
`verified-by: bravebot_qr::tests::the_format_and_version_information_are_the_standards`
`verified-by: bravebot_qr::tests::the_finders_the_timing_and_the_dark_module_are_where_the_standard_puts_them`
`verified-by: bravebot_qr::tests::a_place_outside_the_code_is_light`
`verified-by: bravebot_qr::tests::the_lines_draw_the_code_with_a_quiet_zone_in_black_on_white`
`verified-by: bravebot_qr::tests::codes_the_system_reader_read_are_drawn_as_they_were`

<a id="LISTEN-5"></a>
### LISTEN-5: a device pairs with an invitation the person opens and a code the person types on the host

The person types `pair NAME read|control [WORKSPACE]` on the host. NAME is 1 to 32 of `a-z`, `0-9` and `-` and no device has it. The grant has no default. WORKSPACE is one of the directories the host was given with `--workspace`, named by a path that resolves to it. It can be left out where the host was given one workspace, which is then the device's, and the pairing line the host shows names it. Where the host has several, a pair without one is refused, and so is a directory that is not one of them, each with the host's workspaces listed. The host opens one window with a new 32-byte secret, and shows it as the link `bravebot://pair?v=1&h=<hash of the host's key>&a=<address>&s=<secret>` and, where its standard error is a terminal, as a QR code of that link. The secret is in memory, and in that link and nowhere else the host says. Opening another window ends the one open. The window ends two minutes after it opens by the machine's monotonic clock or its wall clock, whichever says so first, so a code left on the screen of a machine that slept is refused when it wakes, and the host says that the pairing ended once the listener next looks, which is within a moment. A listener with no device paired and no window open accepts nobody.

A phone that has read the link connects, pins the host's key to the hash in it, and sends one line, `pair.begin`, with a `proof` and optionally a `label` of up to 64 bytes that it calls itself. The proof is an HMAC-SHA256 under the secret of the TLS exporter value for the connection and the hash of the phone's own key, and the host checks it without learning how much of it was right. The value is different for every connection, so a proof made on one is no proof on another, and it names the phone's key, so it is no proof for another key. The phone then shows a six-digit code made from the same inputs under another label, and the host does not show it. The host says that a phone is asking to pair as NAME, with the phone's label drawn with only the characters of ASCII that print and a space kept, and everything else, which includes a quote and a backslash, every control character, every mark that reorders, hides or combines text and every letter that is not ASCII, written as `\u{..}`, and the person types `confirm CODE` with the code the phone shows. A code that matches adds the device with the grant and workspace the person chose, and the phone's connection is answered `{"id":N,"ok":{"paired":true}}` and served as that device from then on.

Before the invitation, the host shows the limits the device is approved for, one line each: the scope a session `session.new` opened in the workspace now would hold before anything is answered in it, read under the same settings and built as `session.new` builds the session, with nothing minted or written (decision 18 of `docs/design/mobile/decisions-and-tradeoffs.md`). Each field of the scope is shown under the name a suspension gives it, with each key of a list on a line of its own, and a list with nothing in it is named in one line. Every character that is not printing ASCII or a space, and a backslash, is written as `\u{..}`, since a rule or a server's name can come from a settings file in the checkout. The host says that typing the code also approves these limits, and agrees the device may show what a session there shows the person, with what bravebot marks as from somewhere nobody vouched for shown as a placeholder, though file names may still show ([HOST-16](local-host.md#HOST-16)). The device holds them as its ceiling, and `devices` lists them under it. Where they cannot be read, no window opens and the host says to type the pair again.

The window ends when its device is paired, after two minutes, when another is opened, when the host stops listening, and at the first failed attempt: a proof that does not check, a second phone while one waits, or a code that does not match. A connection that fails ends with nothing sent, so a phone that fails does not learn which input was wrong. A first line that is not a request to pair, is longer than 4,096 bytes or does not arrive within the time of the handshake, however slowly its bytes come, ends its connection and leaves the window open. A window pairs one device. Anyone who can reach the address can close a window by failing an attempt, so pairing can be denied and cannot be completed by a program that does not hold the secret and the phone's code.

`verified-by: bravebot_listener::invitation::a_phone_that_proves_the_secret_and_whose_code_is_typed_is_paired`
`verified-by: bravebot_listener::invitation::a_window_pairs_one_device`
`verified-by: bravebot_ui_bridge::device_link::the_workspace_can_be_left_out_only_where_the_host_has_one`
`verified-by: bravebot_ui_bridge::device_link::a_workspace_the_host_was_not_given_is_refused_with_the_ones_it_has`
`verified-by: bravebot_ui_bridge::scope_tests::the_scope_of_a_new_session_is_the_scope_session_new_opens_with`
`verified-by: bravebot_ui_bridge::device_link::every_field_of_a_ceiling_is_shown_under_its_name`
`verified-by: bravebot_ui_bridge::device_link::a_key_in_a_ceiling_is_drawn_without_anything_that_could_rewrite_the_line`
`verified-by: bravebot_ui_bridge::device_link::pairing_shows_the_limits_the_code_approves_and_devices_lists_them`
`verified-by: bravebot_ui_bridge::device_link::no_window_opens_where_the_limits_cannot_be_read`
`verified-by: bravebot_listener::invitation::a_proof_that_does_not_check_closes_the_window_whichever_input_was_wrong`
`verified-by: bravebot_listener::invitation::a_wrong_code_closes_the_window_and_pairs_nothing`
`verified-by: bravebot_listener::invitation::a_second_device_closes_the_window_of_the_first`
`verified-by: bravebot_listener::invitation::a_window_ends_when_its_time_is_up`
`verified-by: bravebot_listener::pairing_tests::a_window_that_runs_out_is_announced_while_the_listener_runs`
`verified-by: bravebot_listener::invitation::a_window_ends_when_the_wall_clock_says_so_though_the_other_clock_stopped`
`verified-by: bravebot_listener::invitation::a_code_typed_after_the_time_is_up_pairs_nothing`
`verified-by: bravebot_listener::invitation::a_new_window_ends_the_old_one`
`verified-by: bravebot_listener::invitation::closing_ends_the_window`
`verified-by: bravebot_listener::invitation::a_window_is_not_opened_for_a_bad_or_taken_name_or_without_a_secret`
`verified-by: bravebot_listener::invitation::a_waiting_connection_is_told_when_the_code_is_typed`
`verified-by: bravebot_listener::invitation::the_secret_is_in_no_debug_form_and_no_announcement`
`verified-by: bravebot_listener::invitation::a_link_is_read_back_as_it_was_written`
`verified-by: bravebot_listener::invitation::text_that_is_not_a_link_is_not_read`
`verified-by: bravebot_listener::invitation::a_devices_text_is_drawn_without_anything_that_could_rewrite_the_line`
`verified-by: bravebot_listener::invitation::what_is_said_when_a_window_ends_is_said_outside_the_lock`
`verified-by: bravebot_listener::pairing::the_proof_and_the_code_follow_each_of_their_inputs`
`verified-by: bravebot_listener::pairing::a_proof_is_right_only_when_every_input_is_and_every_byte_is`
`verified-by: bravebot_listener::pairing::the_code_is_not_made_from_the_start_of_the_proof`
`verified-by: bravebot_listener::pairing::a_code_is_six_digits`
`verified-by: bravebot_listener::pairing_tests::a_listener_with_no_device_and_no_window_accepts_nobody`
`verified-by: bravebot_listener::pairing_tests::a_phone_pairs_with_the_secret_and_the_code_typed_on_the_host`
`verified-by: bravebot_listener::pairing_tests::the_host_does_not_say_the_code_or_the_secret`
`verified-by: bravebot_listener::pairing_tests::a_label_is_carried_as_written_and_left_out_where_it_is_too_long`
`verified-by: bravebot_listener::pairing_tests::a_phone_without_the_secret_is_closed_and_the_window_closes`
`verified-by: bravebot_listener::pairing_tests::a_proof_made_on_another_connection_is_refused`
`verified-by: bravebot_listener::pairing_tests::a_second_phone_closes_the_pairing_of_the_first`
`verified-by: bravebot_listener::pairing_tests::a_wrong_code_pairs_nothing_and_ends_the_connection`
`verified-by: bravebot_listener::pairing_tests::a_first_line_that_is_not_a_pairing_request_ends_the_connection_and_leaves_the_window_open`
`verified-by: bravebot_listener::pairing_tests::a_first_line_over_the_limit_is_refused_though_it_would_pair_the_phone`
`verified-by: bravebot_listener::pairing_tests::a_first_line_that_never_ends_is_closed_when_the_time_is_up`
`verified-by: bravebot_listener::pairing_tests::a_first_line_that_trickles_is_closed_when_the_time_is_up`
`verified-by: bravebot_listener::pairing_tests::stopping_ends_a_phone_that_has_not_sent_its_first_line`
`verified-by: bravebot_listener::pairing_tests::stopping_closes_the_window_and_releases_a_phone_waiting_for_its_code`
`verified-by: bravebot_ui_bridge::device_link::what_the_host_says_of_a_phone_is_drawn_escaped`
`verified-by: bravebot_ui_bridge::device_listener::a_phone_pairs_through_the_hosts_input_and_is_shown_synthetic_data`

<a id="LISTEN-6"></a>
### LISTEN-6: a paired device is answered with a session that is not real

A device the person paired is answered by a service that holds no way to a session, a bridge or a tool. Each request asks to read or to control as the host's allowlist says for its method: `session.new`, `session.close`, `turn.send`, `turn.cancel`, `trust.reply` and every other reply the host offers control, the other methods the host offers read, and a method the host does not offer controls, so a method nobody classified is never taken for a read. A line that is not a request the host can read, such as one with no number to answer under, controls too, whatever method it names. A `read` device that sends anything that controls has its connection ended with nothing sent ([LISTEN-3](#LISTEN-3)), and is still paired. `instance.info` reports one session, `synthetic-1`, and says that the connection serves synthetic data, with the device's name and grant. `session.attach` of that session answers an empty view, and any other session is refused with `no_such_session`. For a `control` device, `session.new` answers `synthetic-1` whatever directory it names, and every other control the host offers, with the parameters the host offers it with, is answered `{"synthetic":true}` for `synthetic-1` and `no_such_session` for any other session. None of them changes anything. For a `control` device, every other method, which includes the other reads the host offers, a control with a parameter the host does not offer it with, and `pair.begin`, is refused with `not_allowed`, and a line that is not a request is answered `bad_request` where it has a number to answer under and not answered where it has none. A `read` device asking one of the other reads is refused with `not_allowed` in the same way. A device is therefore shown nothing of what the host's sessions hold, and can start nothing. A line a device sends is never read as something the person typed on the host.

`verified-by: bravebot_ui_bridge::device_link::a_device_is_shown_one_synthetic_session_and_told_it_is_not_real`
`verified-by: bravebot_ui_bridge::device_link::every_other_method_is_refused`
`verified-by: bravebot_ui_bridge::device_link::each_request_asks_the_gate_for_the_access_its_method_has`
`verified-by: bravebot_ui_bridge::device_link::a_control_is_answered_for_the_synthetic_session_and_changes_nothing`
`verified-by: bravebot_ui_bridge::allowlist::each_offered_method_reads_or_controls_as_written_here`
`verified-by: bravebot_ui_bridge::allowlist::a_method_the_host_does_not_offer_needs_control`
`verified-by: bravebot_ui_bridge::device_listener::a_read_phones_raw_controls_end_its_connection_and_a_control_phones_are_answered`
`verified-by: bravebot_ui_bridge::device_link::a_line_that_cannot_be_answered_gets_no_answer_and_one_that_can_gets_a_failure`
`verified-by: bravebot_ui_bridge::device_listener::a_phone_pairs_through_the_hosts_input_and_is_shown_synthetic_data`

<a id="LISTEN-7"></a>
### LISTEN-7: a device the person revokes is gone at once and is not let back

The person types `revoke NAME` on the host. The host removes the device, ends its connection and says so. The removal and the end of the connection are made in one hold of the registry's lock, which a request being answered also holds, so a request of the device that is being answered finishes, and a request the host has read and not yet run, or has not yet read, is not run: the device is no longer held when it is asked. A device that connects again completes the handshake, since that is how the host learns who it is, and is closed with nothing sent. It is held as revoked and not as a device nobody paired, until the person pairs it again with a new window ([LISTEN-5](#LISTEN-5)), when it is a device again with the grant and workspace that window gave. A name no device has is refused and changes nothing. Pairing, and revoking, are the person's alone: nothing a device sends is read as either.

`verified-by: bravebot_listener::revocation_tests::revoking_a_device_ends_its_connection_and_removes_it`
`verified-by: bravebot_listener::revocation_tests::a_name_no_device_has_is_not_revoked`
`verified-by: bravebot_listener::revocation_tests::a_revoked_device_is_refused_when_it_comes_back`
`verified-by: bravebot_listener::revocation_tests::revoking_waits_for_the_request_being_answered_and_then_removes_the_device`
`verified-by: bravebot_listener::revocation_tests::a_line_is_read_before_the_lock_so_revoking_does_not_wait_for_it`
`verified-by: bravebot_listener::revocation_tests::a_revoked_device_can_be_paired_again_by_the_person`
`verified-by: bravebot_listener::registry::a_revoked_device_is_not_answered_until_the_person_pairs_it_again`
`verified-by: bravebot_ui_bridge::device_listener::revoke_on_the_hosts_input_ends_the_phones_connection_and_removes_the_device`

<a id="LISTEN-8"></a>
### LISTEN-8: a command-line phone pairs with a host from its link, for trying the host by hand

`cargo run -p bravebot-listener --example phone --features testing -- LINK` is a phone at a terminal, for a person who wants to try a host started with `--listen` and has no phone app. It takes the link the host shows after `pair` as its one argument and refuses anything else, a link that is not one a host writes, and a host whose key is not the one the link names, each with a reason and before it shows a code. It connects, proves the secret, and writes the code the person types on the host as `confirm CODE`. It waits for them to do so for as long as the host's window lasts. Once the host has paired it, each line it is given on its standard input is sent as a request, one at a time with what the host sends read between them so that a large input does not leave both ends waiting to write, and each line the host sends is written after `host:`, except a heartbeat, which it answers with a blank line and does not write. It reads its input on a thread of its own, so a person who types nothing for a while keeps the connection ([LISTEN-9](#LISTEN-9)), and an input it cannot read, such as a line that is not text, stops it with that error once the lines before it are sent and the host's answers to them are written. It is a client like any other: the host applies every rule above to it, and it holds no credential and no setting.

`verified-by: bravebot_listener::pairing_tests::a_command_line_phone_pairs_from_a_link_and_talks_to_the_host`
`verified-by: bravebot_listener::pairing_tests::a_command_line_phone_refuses_a_link_that_is_not_the_hosts`
`verified-by: bravebot_listener::pairing_tests::a_command_line_phone_says_when_the_host_ends_the_pairing`
`verified-by: bravebot_listener::pairing_tests::a_command_line_phone_takes_one_argument_that_is_text`
`verified-by: bravebot_listener::pairing_tests::a_command_line_phone_answers_heartbeats_while_its_person_types_nothing`
`verified-by: bravebot_listener::pairing_tests::a_command_line_phone_given_a_large_input_writes_every_answer`
`verified-by: bravebot_listener::pairing_tests::a_command_line_phone_stops_with_an_error_its_input_gives`
`verified-by: bravebot_ui_bridge::device_listener::a_command_line_phone_pairs_with_the_real_host_and_is_answered`

<a id="LISTEN-9"></a>
### LISTEN-9: a paired device that goes quiet has its connection ended in a bounded time

When the host has written nothing to a paired device for 15 seconds, it sends it the line `{"event":"heartbeat","data":{}}`. The device answers with any line, and a blank line is enough: it is skipped and reaches no service. A paired device that has sent nothing for 45 seconds has its connection ended. So a phone that lost its network, stopped or was put in the background, and a connection whose other end is gone without closing it, are ended within that time, and no longer count against the 16 connections the listener holds ([LISTEN-2](#LISTEN-2)). What came of a line before a wait is kept, so a line that arrives in parts is read whole. Only bytes the host can decrypt count as something sent, and the socket is read one read at a time with the time looked at after each, so a device that sends the bytes of a record slowly is sent its heartbeats on time and is ended when the silence is up. Ending the connection changes nothing about the device: it is still paired, and it connects again with a full handshake ([LISTEN-1](#LISTEN-1)). A heartbeat and its answer grant nothing.

`verified-by: bravebot_listener::serving_tests::a_paired_device_that_sends_nothing_is_ended_once_the_silence_allowed_has_passed`
`verified-by: bravebot_listener::serving_tests::a_device_that_answers_each_heartbeat_keeps_its_connection`
`verified-by: bravebot_listener::serving_tests::a_line_that_arrives_across_heartbeats_is_read_whole`
`verified-by: bravebot_listener::serving_tests::a_device_that_trickles_a_record_is_ended_when_the_silence_is_up`
