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
  - crates/listener/src/limits.rs
  - crates/listener/src/phone.rs
  - crates/listener/src/registry.rs
  - crates/listener/src/server.rs
  - crates/listener/src/serving_tests.rs
  - crates/listener/src/tls.rs
documented-by: none (internal: the listener is started by a person from the host's command line and no page says how a phone reaches it yet)
---

## Scope

How the host accepts a connection from a phone: the keys each end names the other by, which connection is served and which is closed, and what is decided before anything a device sent is read. The listener is a library the host starts and gives a service to answer with. It depends on no crate that opens a session, so a device is answered only what that service says.

Not here: how a device is paired and how it is revoked, which lines a device may send, and what a session shows a device.

## Clauses

<a id="LISTEN-1"></a>
### LISTEN-1: a device is served over TLS 1.3 with keys that name each end

The listener binds one address that its caller names, and serves each connection on a thread of its own. A connection is TLS 1.3 and no other version. The host shows a self-signed certificate for a P-256 key that it made in memory when it started, and writes that key nowhere. The listener asks the device for a certificate and requires one, and takes any that holds a P-256 key, whoever signed it. A key is named by the SHA-256 hash of its SubjectPublicKeyInfo. That is how a device tells the host's key from another program's, since the person's invitation gives it the hash, and how the host tells one device from another. A certificate that holds another key type, another curve or a point that is not in uncompressed form names no key, and its connection ends. A certificate that is cut short or has a byte changed is read without a panic, and names no key or names the key it holds.

Whether a key is a device is the registry's answer, asked once the handshake is over. The registry is in this process's memory and nowhere else, and a host that restarts has no devices. The listener sends no session ticket and keeps no session, so every connection is a full handshake, and a device that was never paired, or was revoked, cannot return on a ticket it kept. A device has one connection. A connection it makes ends the one it had. Stopping the listener ends every connection and accepts no more.

The certificate is written by this crate, a few dozen bytes of DER, so no certificate library is a dependency of the host. The certificate's own validity dates are not read by either side, since a certificate here stands for a key.

`verified-by: bravebot_listener::serving_tests::a_paired_phone_is_answered_and_the_service_is_given_its_record`
`verified-by: bravebot_listener::serving_tests::a_phone_pinned_to_another_key_refuses_the_host`
`verified-by: bravebot_listener::serving_tests::a_second_connection_is_a_full_handshake_and_not_a_resumed_session`
`verified-by: bravebot_listener::serving_tests::a_new_connection_of_a_device_ends_its_old_one`
`verified-by: bravebot_listener::serving_tests::stopping_ends_every_connection_and_accepts_no_more`
`verified-by: bravebot_listener::identity::the_certificate_is_one_the_tls_library_reads_and_holds_the_key_that_signs`
`verified-by: bravebot_listener::identity::a_key_is_named_by_the_hash_of_its_own_public_key_info`
`verified-by: bravebot_listener::identity::the_name_in_the_certificate_does_not_change_the_hash_of_its_key`
`verified-by: bravebot_listener::identity::a_key_that_is_not_p256_is_not_named`
`verified-by: bravebot_listener::identity::a_certificate_that_is_cut_or_changed_is_read_without_a_panic`
`verified-by: bravebot_listener::identity::the_debug_form_of_an_identity_names_the_key_and_holds_back_the_private_key`
`verified-by: bravebot_listener::der::an_element_is_read_back_at_each_length_form`
`verified-by: bravebot_listener::der::bytes_that_do_not_hold_an_element_are_not_read`

<a id="LISTEN-2"></a>
### LISTEN-2: a connection that is not a paired device is closed with nothing sent, and what one may use before it is a device is bounded

A connection whose key the registry does not hold, one that shows no certificate and one whose handshake does not finish is closed, and nothing is written to it after the handshake. What it sends reaches no service. The limits are global and not per address. At most 16 connections are open at once and at most 4 have not finished the handshake, and a connection past either is closed as it arrives. A handshake that is not finished 10 seconds after the first byte ends the connection, however slowly bytes keep arriving. A line is at most 8 MiB, newline excluded: a line of exactly that length is read, one that reaches it without ending ends the connection and is not answered, and bytes that end without a newline are not a request. A line that is not text ends the connection, and a blank line is skipped. A write that waits more than five seconds ends the connection. An address that is in use is reported by the call that starts the listener, which tries no other.

`verified-by: bravebot_listener::serving_tests::an_unknown_phone_is_closed_with_nothing_sent_and_reaches_no_service`
`verified-by: bravebot_listener::serving_tests::a_phone_that_shows_no_certificate_reaches_no_service`
`verified-by: bravebot_listener::serving_tests::a_flood_of_silent_connections_is_closed_past_the_limit_and_does_not_keep_a_phone_out`
`verified-by: bravebot_listener::serving_tests::a_connection_past_the_limit_of_open_connections_is_closed_as_it_arrives`
`verified-by: bravebot_listener::serving_tests::a_stalled_handshake_is_closed_when_its_time_is_up`
`verified-by: bravebot_listener::serving_tests::a_handshake_that_trickles_bytes_is_closed_when_its_time_is_up`
`verified-by: bravebot_listener::serving_tests::a_line_past_the_limit_ends_the_connection_and_one_at_it_is_read`
`verified-by: bravebot_listener::serving_tests::a_line_that_is_not_text_ends_the_connection_and_a_blank_one_is_skipped`
`verified-by: bravebot_listener::serving_tests::an_address_already_in_use_is_reported`

<a id="LISTEN-3"></a>
### LISTEN-3: a line is answered only after the host has asked whether the device may do what it asks

Before a line is given to the service, the registry says whether the device may do what the line asks, from what the person did on the host and from no byte of the line. A device the registry holds as paired with a grant that covers the operation is allowed. A device with no record, a revoked device whatever grant it held, and a `read` grant asked to control are refused, and the connection ends with nothing sent. Every line a service answers asks to read, because nothing a service answers changes a session. Which operation a given request is, and so which grant it needs, is decided where requests are offered to a device and not here.

The check and the answer are made in one hold of the registry's lock. A change the person makes on the host, such as pairing a device again with another grant, waits for a request being answered and so falls before it or after it, never between its check and its answer. The decision is `Standing::authorizes` in `bravebot-core`, which Verus proves equal to its statement. That statement says a request is allowed only for a paired device whose grant covers it.

`verified-by: bravebot_core::device::each_standing_gets_the_verdict_the_table_gives`
`verified-by: bravebot_core::device::a_device_the_host_holds_no_grant_for_is_never_allowed`
`verified-by: bravebot_core::device::a_read_grant_never_allows_control`
`verified-by: bravebot_core::spec::a_request_is_allowed_only_for_a_paired_device_whose_grant_covers_it`
`verified-by: bravebot_listener::registry::a_request_is_answered_for_a_held_device_and_not_computed_for_any_other`
`verified-by: bravebot_listener::registry::a_change_on_the_host_waits_for_the_request_being_answered`
`verified-by: bravebot_listener::registry::a_device_is_added_under_a_name_nobody_else_has`
`verified-by: bravebot_listener::registry::a_name_is_one_to_thirty_two_of_lower_case_digits_and_hyphens`

<a id="LISTEN-4"></a>
### LISTEN-4: a code the host shows is a QR code a phone's own reader reads

The host draws an invitation as a QR code in the terminal, from a crate that does no I/O and has no dependency. The code is QR Model 2 in byte mode at error correction level M, in the smallest of versions 1 to 10 that holds the text, which is up to 213 bytes; a longer text is refused. Its lines set black on white themselves, with a quiet zone of four modules, and reset afterwards, because a scanner reads dark modules on a light ground whatever colours the terminal uses. Two rows of modules share one line of text.

Each of these is held by a reader written from the standard's description of a square and not from the encoder's code: the error correction of the standard's own example, the format and version information the standard tabulates, every length from 0 to 213 bytes read back to the bytes it was made from with every block's error correction checking, and the fixed patterns where the standard puts them. Codes of each size were also read back by the operating system's own QR reader when their expected squares were recorded.

`verified-by: bravebot_qr::tests::the_error_correction_of_the_standards_example_is_the_one_it_prints`
`verified-by: bravebot_qr::tests::a_product_in_the_field_is_what_the_polynomial_arithmetic_gives`
`verified-by: bravebot_qr::tests::every_length_up_to_the_limit_reads_back_as_it_was_made`
`verified-by: bravebot_qr::tests::the_smallest_version_that_holds_the_text_is_used_and_more_than_the_limit_is_refused`
`verified-by: bravebot_qr::tests::the_format_and_version_information_are_the_standards`
`verified-by: bravebot_qr::tests::the_finders_the_timing_and_the_dark_module_are_where_the_standard_puts_them`
`verified-by: bravebot_qr::tests::a_place_outside_the_code_is_light`
`verified-by: bravebot_qr::tests::the_lines_draw_the_code_with_a_quiet_zone_in_black_on_white`
`verified-by: bravebot_qr::tests::codes_the_system_reader_read_are_drawn_as_they_were`
