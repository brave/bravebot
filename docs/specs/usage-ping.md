---
id: PING
title: The daily usage ping
status: normative
governs:
  - crates/usage-ping/src/lib.rs
  - crates/config/src/usage_ping.rs
  - crates/ui-bridge/src/usage_ping.rs
  - ui/src/renderer/components/UsagePingNotice.tsx
  - ui/src/renderer/usage-ping.ts
documented-by:
  - docs/website/docs/customize/configuration.md
  - docs/website/docs/security/usage-ping.md
---

## Scope

One request a day to Brave saying that a copy of bravebot ran: what it holds, when it is sent,
which launches count, where it goes, and how a person stops it. It is sent without being asked for,
so most of the clauses below are about what it cannot say.

The request is the program's own, made on its own behalf outside any session. No model is involved,
and what comes back is a status.

The service is go-vault's `GET /1/usage/bravebot`. It is not in this repository, and what it does is
not this spec's, but a person reading what is sent should know it. It adds the country and region it
reads from the connection, and the date it received the ping. It drops a ping that comes from a data
centre. In a country with few daily users it blanks the region and `woi`, so that the two together
cannot single somebody out.

## Clauses

<a id="PING-1"></a>
### PING-1: a ping holds eleven fields, each a fixed word, a boolean or a date

A ping is a `GET` of `/1/usage/bravebot` with eleven query fields and nothing else:

- `platform`: `darwin`, `linux` or `windows`.
- `version`: this build's version, three numbers separated by dots.
- `install`: how this copy was installed: `npm`, `script`, `powershell`, `homebrew`, `winget`,
  `apt`, `dnf`, or `unknown` for anything else, a build from source included.
- `surface`: `tui`, `desktop` or `cli` ([PING-7](#PING-7)).
- `backend`: the kind of service that answers the model the launch starts with ([PING-8](#PING-8)).
- `premium`: whether the configuration holds a premium key for Brave's endpoint.
- `daily`, `weekly`, `monthly`, `first`: [PING-3](#PING-3).
- `woi`: [PING-4](#PING-4).

A build on another operating system, or whose version is not three numbers, sends nothing. No
identifier, host name, account name, path, prompt, file content, credential or model name is sent,
and no field is the same from one install to the next except by coincidence of these values.

**Why.** Every value comes from a fixed set in the source, a number or a date, so nothing a person
typed or a file holds can reach the request, and none needs escaping. Counting launches needs no
more than this, and a field that told installs apart would let the service follow one person.

`verified-by: bravebot_usage_ping::lib::the_url_holds_every_field`
`verified-by: bravebot_usage_ping::lib::platforms_are_named_as_the_service_takes_them`
`verified-by: bravebot_usage_ping::lib::only_a_three_number_version_is_sent`
`verified-by: bravebot_usage_ping::lib::every_open_field_word_is_short_and_alphanumeric`
`verified-by: bravebot_usage_ping::sending::one_ping_a_day_with_exactly_the_required_fields`

<a id="PING-2"></a>
### PING-2: the ping is on until a person or an administrator turns it off

Five things turn it off, strongest first:

1. An incognito session ([PING-11](#PING-11)).
2. `"usagePing": false` in the managed file ([managed settings](backends.md#BACKEND-38)).
3. `BRAVEBOT_USAGE_PING=0`, or `DO_NOT_TRACK` set to anything but an empty value, `0` or `false`.
   The first is named as the reason when both are set.
4. `"usagePing": false` in the person's own `~/.bravebot/settings.json`.
5. The switch in `/config` or the desktop settings, set to off ([PING-10](#PING-10)).

Any one is enough, and nothing turns the ping back on over a stronger one: the switch or a settings
file saying `true` is no stronger than saying nothing. Only the boolean `false` counts in a settings
file, and only `0` for `BRAVEBOT_USAGE_PING`. Of the settings files only the person's own and the
managed one are read, and the key is not reported as unread. Every caller resolves the switches
through one function, read fresh on each call.

**Why.** On by default, so that Brave can count the people using a release. The variables are the
ones a person who turns off every program's reporting has already set. The managed file can only
refuse, as it does for crash reports ([CRASH-4](crash-reports.md#CRASH-4)). A checkout's settings
file is left out because a layer that arrives with a clone is not the person's statement about what
their machine sends. One function decides so that the terminal, the desktop and the crate that sends
cannot each resolve the switches in a different order.

`verified-by: bravebot_config::usage_ping::the_ping_is_on_when_nothing_turned_it_off`
`verified-by: bravebot_config::usage_ping::each_switch_alone_turns_the_ping_off_and_is_named`
`verified-by: bravebot_config::usage_ping::the_switch_cannot_turn_the_ping_back_on_over_a_stronger_layer`
`verified-by: bravebot_config::usage_ping::a_setting_of_true_does_not_overrule_the_switch_turned_off`
`verified-by: bravebot_config::usage_ping::the_strongest_layer_is_named_when_several_turned_it_off`
`verified-by: bravebot_config::usage_ping::which_variable_values_turn_the_ping_off`
`verified-by: bravebot_config::managed::a_managed_usage_ping_false_is_a_pin_and_nothing_else_is`
`verified-by: bravebot_usage_ping::sending::a_variable_that_is_not_text_sends_nothing`
`verified-by: bravebot_config::settings::only_the_persons_own_file_says_whether_the_usage_ping_goes`
`verified-by: bravebot_config::settings::only_a_boolean_answers_for_the_usage_ping`
`verified-by: bravebot_config::settings::the_usage_ping_key_is_not_reported_as_unread`
`verified-by: bravebot_tui::persist::the_usage_ping_answer_gathers_the_settings_file_the_switch_and_the_variables`
`verified-by: bravebot_usage_ping::sending::nothing_goes_when_the_ping_is_off`

<a id="PING-3"></a>
### PING-3: one ping a UTC day per install, across every front end

A ping is sent at most once per UTC day for each state directory, whichever front end starts first
that day. The record of the last accepted day is a file in the state directory, so a person using
the terminal and the desktop on one day sends one ping.

`daily` is true when no ping was accepted today. `weekly` is true when none was accepted this week,
the weeks starting on Monday. `monthly` is true when none was accepted this calendar month. `first`
is true until the service has accepted one ping from this install, and the first ping sets all four.

**Why.** One a day is enough to count daily, weekly and monthly users, and the flags let the service
count each without knowing which install sent which ping. UTC because the workspace has no timezone
database and the crate may not call the C library, so a person far from UTC has their day change at
a different hour from their clock's.

`verified-by: bravebot_usage_ping::lib::the_first_ping_sets_every_flag`
`verified-by: bravebot_usage_ping::lib::the_next_day_in_the_same_week_is_daily_alone`
`verified-by: bravebot_usage_ping::lib::the_week_turns_on_monday_and_the_month_on_the_first`
`verified-by: bravebot_usage_ping::sending::one_ping_a_day_with_exactly_the_required_fields`
`verified-by: bravebot_cli::running::a_one_shot_run_sends_one_usage_ping_a_day_and_a_scheduled_firing_sends_none`
`verified-by: bravebot_ui_bridge::usage_ping_notice::the_window_sends_one_ping_a_day_once_its_notice_is_shown`

<a id="PING-4"></a>
### PING-4: `woi` is the Monday of the week of the install's first accepted ping

`woi` is a date, `YYYY-MM-DD`, the Monday on or before the UTC day of the first ping the service
accepted, and every later ping from the install sends the same date. A ping that fails records no
`woi`, so an install whose first attempt fails on a Sunday and succeeds on the Monday is dated to
the Monday's week.

**Why.** The week an install began lets the service count how many installs from one week are still
in use, the way the browser's ping does, and a week is coarse enough to be shared by many installs.
Dating it from the first accepted ping keeps a failure from moving it a week earlier than the service
has any record of.

`verified-by: bravebot_usage_ping::lib::woi_is_the_monday_on_or_before_the_day`
`verified-by: bravebot_usage_ping::lib::woi_is_set_once_and_first_holds_until_accepted`
`verified-by: bravebot_usage_ping::lib::woi_is_the_week_of_the_first_accepted_ping`

<a id="PING-5"></a>
### PING-5: a ping that fails waits an hour, and two starts at once send one

A ping is recorded as tried before its request goes. A start less than an hour after a try that was
not accepted sends nothing, and a try stamped in the future does not count, so a clock set back does
not stop the ping. The decision and the request are made under a lock file in the state directory,
which is readable only by this user. A start that finds the lock held sends nothing, and the record
is read again once the lock is taken.

**Why.** A service that is down should not be asked on every launch, and a start that crashed during
the request should wait like one that failed. Several front ends can start in the same second, and
without the lock each would see no ping today and send one.

`verified-by: bravebot_usage_ping::lib::a_failure_waits_an_hour`
`verified-by: bravebot_usage_ping::sending::a_refused_ping_is_retried_after_an_hour`
`verified-by: bravebot_usage_ping::sending::starts_at_once_send_once`

<a id="PING-6"></a>
### PING-6: nothing is sent before a front end has said what is sent

No ping is sent until a front end has shown the notice. The notice says what a ping holds, that it
never holds prompts, files or keys, and how to turn it off. One marker in the state directory records
that it was shown, for every front end, so a person told in the terminal is not told again by the
desktop. It is recorded before the text is handed over, so a front end that fails to show it does not
say it on every launch. While the ping is off the notice is not due, and a person who turns it on
later is told before the first ping.

Where it is said:

- The terminal says it once on the startup screen, as a note.
- `-p`, `--plain` and `bravebot acp` write it once to standard error and never to standard output,
  which is the reply a script reads and, for ACP, the protocol. A scheduled or triggered firing says
  nothing.
- The desktop window shows it in the conversation panel: under the welcome's actions when no
  conversation is open, and docked on the composer when one is. It has a button that opens Settings,
  under General. The window records it as shown when it draws it.

**Why.** The ping is on by default, so the person is told before the first one is sent rather than
after. Nobody reads the standard error of a scheduled run, and the first run a person starts tells
them instead.

`verified-by: bravebot_usage_ping::sending::nothing_goes_before_the_notice`
`verified-by: bravebot_tui::usage_ping_notice::the_notice_is_said_once_and_names_what_is_sent_and_how_to_stop_it`
`verified-by: bravebot_tui::usage_ping_notice::the_notice_waits_while_the_ping_is_off_and_is_due_once_it_is_turned_on`
`verified-by: bravebot_tui::usage_ping_notice::a_notice_marked_shown_by_another_front_end_is_not_said_again`
`verified-by: bravebot_cli::running::a_one_shot_run_says_what_the_usage_ping_sends_once_on_stderr_and_never_on_stdout`
`verified-by: bravebot_ui_bridge::usage_ping_notice::the_notice_is_due_until_the_window_marks_it_shown`
`verified-by: bravebot_ui_bridge::usage_ping_notice::the_notice_is_not_due_while_the_ping_is_off`

<a id="PING-7"></a>
### PING-7: a launch a person started counts, and nothing else does

These send the ping, each on a thread of its own so that no start waits for it:

- The terminal interface, as `tui`, when its session begins.
- The desktop app, as `desktop`, when `bravebot-rpc` starts and again when the window records the
  notice as shown, since the first start of a new install comes before the notice.
- `-p`, `--plain` and `bravebot acp`, as `cli`. A `-p` run that has finished waits at most two
  seconds for a ping still in flight before it exits, since exiting would end the request.

These send nothing: a scheduled or triggered firing, a session the local host keeps (`__bg-host`),
and the standalone `bravebot-acp` executable, which shows no notice.

**Why.** The count is of people using bravebot, and a firing nobody started, or a process kept for a
session already counted, is not one. Two seconds is long enough for one request to an answering
service and short enough that a script does not notice it, and only a run whose ping was due waits
at all.

`verified-by: bravebot_cli::usage_ping::the_interface_that_draws_sends_one_ping_a_day_as_tui`
`verified-by: bravebot_cli::usage_ping::a_session_in_lines_sends_one_ping_as_cli`
`verified-by: bravebot_cli::usage_ping::the_agent_client_protocol_sends_one_ping_as_cli`
`verified-by: bravebot_cli::running::a_one_shot_run_sends_one_usage_ping_a_day_and_a_scheduled_firing_sends_none`
`verified-by: bravebot_usage_ping::sending::a_ping_started_at_startup_can_be_waited_for`
`verified-by: bravebot_ui_bridge::usage_ping_notice::the_window_sends_one_ping_a_day_once_its_notice_is_shown`
`verified-by: bravebot_ui_bridge::usage_ping_notice::a_window_whose_notice_was_shown_before_pings_at_its_start`
`verified-by: bravebot_ui_bridge::usage_ping_notice::the_standalone_acp_executable_says_no_notice_and_sends_no_ping`

<a id="PING-8"></a>
### PING-8: `backend` names a kind of service, never an id a person chose

`backend` is one of `brave`, `bedrock`, `chatgpt`, `xai`, `openai`, `anthropic`, `openrouter`,
`ollama`, `lmstudio`, `llamacpp`, `other` or `none`. It is the kind that serves the model the
terminal, `-p` or `--plain` session starts with, or the configured default model where the front end
picks a session's model later, as the desktop and `bravebot acp` do. A gateway entry whose id is not
one of the listed services is `other`, and no configured model is `none`. A desktop or ACP start whose
configuration does not load sends nothing.

**Why.** A gateway's id is a name a person wrote, and could say who they are or where they work.

`verified-by: bravebot_usage_ping::lib::a_gateway_id_not_known_is_other`
`verified-by: bravebot_usage_ping::lib::every_open_field_word_is_short_and_alphanumeric`
`verified-by: bravebot_cli::running::a_one_shot_run_sends_one_usage_ping_a_day_and_a_scheduled_firing_sends_none`

<a id="PING-9"></a>
### PING-9: the ping goes to one address through the one egress call

A released build sends to Brave's usage service. `BRAVEBOT_USAGE_PING_URL`, where it is set, replaces
the base address in any build; it changes where the ping goes and nothing about whether it goes,
except that a value that is not text sends nothing rather than sending to the address it was meant
to replace. A debug build with the variable unset sends nothing. The request goes through the call every request
here goes through ([NET-1](network-egress.md#NET-1)), with only its own address routed and a user
agent of `bravebot`, and carries no credential. The reply's body is never read: a status from 200 to
299 is accepted and anything else is a failure ([PING-5](#PING-5)).

The base address of a released build is `https://laptop-updates.brave.com`, which is not yet
confirmed by the service's owners ([Open questions](#open-questions)).

**Why.** Neither `cargo run` nor a test that starts the program should be counted as a person's
launch, so a debug build sends only where somebody named a destination. The variable lets a person
point it at a server of their own to see exactly what is sent. Nothing in the reply could change
what happens next, so nothing in it is read.

`verified-by: bravebot_usage_ping::sending::a_development_build_with_no_destination_named_sends_nothing`
`verified-by: bravebot_usage_ping::sending::one_ping_a_day_with_exactly_the_required_fields`
`verified-by: by-construction (the manifest depends on bravebot-net and on no crate that speaks to a network, and the layering test holds the manifest to this crate's row in layering.md)`
`verified-by: bravebot_usage_ping::lib::a_released_build_refuses_an_address_that_is_not_text`

<a id="PING-10"></a>
### PING-10: the switch is one file, shown in the terminal and the desktop

`/config` in the terminal has a row for the ping, and the desktop has a switch in Settings, under
General, Privacy. Both write `~/.bravebot/usage-ping`, holding `on` or `off`, by rename, and both read
the answer through [PING-2](#PING-2), so a change in one is seen by the next read in the other. Off
is written rather than the file removed, so a person who turned it off is not turned back on by the
default. Where something stronger than the switch turned the ping off, both show it as off with the
reason, the terminal offers no switch, and the desktop refuses a change.

The desktop window asks the bridge with four methods: `usage_ping.get` answers `enabled`,
`changeable` and the reason; `usage_ping.set` takes `enabled`, a boolean; `usage_ping.notice_due`
and `usage_ping.mark_notice_shown` are the notice's ([PING-6](#PING-6)). A client of the local host
may call none of them.

**Why.** A switch shown as changeable where a stronger layer decides would do nothing when used. One
file means a person does not have to turn the ping off once per front end.

`verified-by: bravebot_session::store::the_usage_ping_switch_is_read_back_both_ways_and_nothing_else_is_a_choice`
`verified-by: bravebot_tui::config_prompt::whether_the_ping_is_on_is_drawn_as_current`
`verified-by: bravebot_tui::config_prompt::a_ping_turned_off_elsewhere_is_shown_with_its_reason_and_offers_no_switch`
`verified-by: bravebot_tui::persist::the_usage_ping_switch_outlives_the_session_that_set_it`
`verified-by: bravebot_tui::persist::turning_the_usage_ping_off_in_config_is_read_back_by_the_next_answer`
`verified-by: bravebot_tui::persist::a_session_that_does_not_persist_leaves_the_usage_ping_switch_alone`
`verified-by: bravebot_ui_bridge::usage_ping::the_ping_is_on_and_changeable_when_nothing_turned_it_off`
`verified-by: bravebot_ui_bridge::usage_ping::turning_the_ping_off_in_the_window_writes_the_file_the_terminal_reads`
`verified-by: bravebot_ui_bridge::usage_ping::a_variable_locks_the_switch_names_itself_and_refuses_a_change`
`verified-by: bravebot_ui_bridge::usage_ping::the_settings_file_locks_the_switch_as_a_setting`
`verified-by: bravebot_ui_bridge::usage_ping::a_change_that_is_not_true_or_false_is_refused`
`verified-by: by-construction (the four methods are in the allowlist's DENIED list, and the host's tests hold no method to be both offered and denied)`

<a id="PING-11"></a>
### PING-11: an incognito session sends nothing and records nothing

A session with no state directory, incognito or on a machine with no home directory, sends no ping,
says no notice, and reads and writes none of the ping's files. The desktop shows the switch as off
for that reason.

**Why.** Once a day needs a record of the last day, and an incognito session adds nothing to
`~/.bravebot` ([incognito.md](incognito.md)). Sending without the record would send on every launch.

`verified-by: bravebot_usage_ping::incognito::an_incognito_session_sends_nothing`
`verified-by: bravebot_tui::incognito::the_usage_ping_is_off_and_its_switch_is_neither_read_nor_written`
`verified-by: bravebot_tui::incognito::the_usage_ping_notice_is_neither_said_nor_recorded`

## Known costs

- **The day is UTC.** A person far from UTC can send two pings in one of their own days, or none, and
  `woi` may be the Monday of a week that began on their Sunday.

- **The service sees the sender's network address.** Nothing in a ping holds it, and the service
  reads a country and region from it. What the service keeps of the connection is the service's.

- **`woi` and `first` together say something about one install.** They are shared by every install
  that began in the same week, and the service blanks `woi` in a country with few users.

- **One ping a day names one surface.** A person who uses the terminal and then the desktop on one day
  is counted once, as whichever started first.

- **The desktop reports the install of the copy it runs.** That copy was not installed by any of the
  listed methods, so a desktop ping says `unknown`.

- **A `-p` run that ends within two seconds of a slow request loses that ping.** The try is recorded,
  so the next start an hour later sends it.

- **A checkout's settings cannot turn it off.** Only the person's own file and the managed file are
  read, so a checkout's `usagePing` is ignored in both directions.

## Open questions

<!-- TODO(en9.1): confirm production host -->
- **The production address.** [PING-9](#PING-9) names `https://laptop-updates.brave.com`, which
  the service's owners have not yet confirmed, so it may change.
