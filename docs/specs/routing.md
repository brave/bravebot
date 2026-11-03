---
id: ROUTE
title: Routing and content
status: normative
governs:
  - crates/core/src/capability.rs
  - crates/core/src/policy.rs
guards:
  - symbol: Policy::before_endorsed_destination
  - symbol: Policy::promote_confined_read
  - symbol: Policy::accept_reference
  - symbol: Policy::path_of_reference
  - symbol: Policy::carry_a_choice
documented-by:
  - docs/website/docs/how-it-works.md
  - docs/website/docs/security/permissions.md
---

## Scope

An **effect** is anything the agent does that changes something or leaves this process: a file
written, a program run, a request sent over the network, a question put on somebody's screen. A
read is not an effect, because it changes nothing and stays inside the working directory. That
asymmetry runs through everything below: the planner may choose what to read, and never what an
effect touches.

This spec is where an effect is allowed to land and what may decide it. What a label means is in
[labels.md](labels.md). Which paths a person vouched for is in [trust-map.md](trust-map.md).

## The split

<a id="ROUTE-1"></a>
### ROUTE-1: every effect splits into routing and content

**Routing** decides where an action goes: a file path, a URL, a recipient, an argument vector.
**Content** is the payload that is merely carried. No field is both, and nothing at run time
reclassifies one. Which argument is which is fixed per tool by the tool surface.

**Why.** This asymmetry is half the mechanism. Untrusted text can be carried into an action as
content but can never become routing, so it cannot redirect anything. The other half is that it
never reaches a component that decides, neither the planner nor the driver.

`verified-by: bravebot_core::policy::fetched_content_can_be_written_but_cannot_choose_the_path`

<a id="ROUTE-2"></a>
### ROUTE-2: routing must be `(T,pub)`

Derived only from trusted input, and never from fetched content. Untrusted routing is an injection
attempt and is refused. Trusted-but-private is refused too, since a routing field ends up
somewhere this policy stops governing.

Some routing fields are authorised by something other than their own integrity. A destination a
person endorsed is one, and ROUTE-8 governs it. A reference the planner names is another. A number
a processor chose is not routing, and [ROUTE-10](#ROUTE-10) says what authorises the one argument
it fills.

A reference name is minted by the driver and handed to the planner, and it arrives back wrapped as
pessimistically as anything the planner composes, so its own label says nothing about the name
inside it. What is asked instead is the integrity of the context the planner named it in: a context
that has met untrusted content may name no reference. That the driver minted the name is not what
settles this. Minting bounds what a wrong name reaches, since such a name resolves to a file the
driver already holds or to nothing at all, but choosing among the names already handed out still
chooses where an effect lands, and the person who endorses the resolved destination is being asked
about that choice rather than making it.

Confidentiality stays the value's own. A private name is refused, since a name derived from the
user's data is that data.

`verified-by: bravebot_core::policy::routing_refuses_untrusted_values`
`verified-by: bravebot_core::policy::routing_refuses_private_values`
`verified-by: bravebot_core::policy::a_reference_cannot_be_named_once_the_context_has_met_something_untrusted`
`verified-by: bravebot_core::value::trusted_private_values_are_not_routing_safe`

<a id="ROUTE-3"></a>
### ROUTE-3: content may be untrusted, but must not be private at release

Carrying bytes decides nothing, so untrusted content is ordinary. Private content is different:
releasing it hands the user's data somewhere this policy no longer governs. Writing back into the
workspace is the one move that lowers confidentiality without releasing anything, because the
destination is inside the boundary the bytes came from. The same holds for what the planner writes
with `write_file` or `edit_file` once it has been shown private content
([LABEL-8](labels.md#LABEL-8)): the text is released public at its own integrity, so the write is
decided as it was before.

`verified-by: bravebot_core::policy::a_write_back_into_the_workspace_lowers_only_confidentiality`
`verified-by: bravebot_core::policy::what_the_planner_writes_into_the_workspace_is_released_public_at_its_own_integrity`
`verified-by: bravebot_agent::mcp::writing_a_file_and_asking_a_question_still_work_after_a_vet`
`verified-by: bravebot_core::policy::private_input_asks_even_for_a_vouched_line`

## The one relaxation

<a id="ROUTE-4"></a>
### ROUTE-4: a read path may be promoted, and nothing else may

Promotion lets the model choose which file to read next, because a read
changes nothing and is confined to the workspace. Every such choice is recorded as a promotion, so
an audit separates the model's decisions from the user's. The proposal is an argument the planner
wrote, so it is held to the context as every argument is ([LABEL-5](labels.md#LABEL-5)): once the
context has met something untrusted, it is refused rather than promoted.

It **must never be used for an effect**. Private content cannot be promoted, and content cannot be
promoted by being read aloud: what is promoted is the model's own proposal, not bytes that came
back from somewhere.

`verified-by: bravebot_core::policy::a_model_proposal_can_be_promoted_for_a_confined_read`
`verified-by: bravebot_core::policy::private_content_cannot_be_promoted`
`verified-by: bravebot_core::policy::a_proposal_cannot_be_promoted_once_the_context_has_met_something_untrusted`
`verified-by: bravebot_core::policy::a_file_cannot_be_promoted_by_reading_it_aloud`

## Effects

<a id="ROUTE-5"></a>
### ROUTE-5: an effect needs a capability and a human endorsement

An action whose capability was not granted is refused outright. A person's
approval mints a **single-use** endorsement bound to that exact value, so it cannot be replayed,
redirected to another value, or reused for a different pipeline. An answer a mode gives in a person's
place mints the same endorsement for the same value and no other, and in the auto mode that answer is
a classifier's word on one question ([AUTO-6](auto-mode.md#AUTO-6)).

A write that would make a trusted path untrusted is the case that must be shown. Where nobody can
be asked, effects are refused rather than applied unseen. A run that reads input lines and was
started with `--approve-input-lines` is asked through the program sending them
([cli.md](cli.md#CLI-36)).

A connector act's content is released only to the place the plan fixed for it.

A connector act's endorsement is not minted by this driver: the driver records no grant for the
act, and the person approves it in the authority's popup.

`verified-by: bravebot_core::policy::an_endorsement_authorises_a_destination_it_does_not_relabel`
`verified-by: bravebot_core::policy::an_endorsement_does_not_outlive_the_destination_it_authorised`
`verified-by: bravebot_core::policy::an_endorsement_does_not_authorise_a_different_destination`
`verified-by: bravebot_core::policy::an_endorsement_does_not_authorise_a_different_line`
`verified-by: bravebot_core::policy::ungranted_capabilities_are_refused`
`verified-by: bravebot_core::policy::a_run_needs_the_capability_as_well_as_the_endorsement`
`verified-by: bravebot_core::policy::a_connector_release_is_refused_unless_precommitted_to_that_place`
`verified-by: bravebot_agent::manifest::a_connector_fetch_fills_one_unread_slot_and_the_act_sends_the_released_text`
`verified-by: bravebot_agent::permission_mode::auto_answers_a_run_only_on_the_classifiers_allow_word_for_a_confined_line`

<a id="ROUTE-6"></a>
### ROUTE-6: a reference is not a destination unless it names a file

Resolving a reference to the name it stands for is the only way that name leaves the policy layer,
and it
authorises nothing by itself. For a read the name is promoted exactly as ROUTE-4 promotes the model's own
choice. For a write it is never promoted: the name goes to a person, and the grant is issued for
the path they saw, which is why such a write always asks. A reference naming no file,
which is everything a processor produced, is refused as a destination. A number a processor chose
([PROC-16](processors.md#PROC-16)) names no file either and is not a destination;
[ROUTE-10](#ROUTE-10) is the one place it may go.

**Why.** That refusal is what stops untrusted text choosing where an effect lands.

`verified-by: bravebot_core::reference::content_is_not_offered_as_a_destination`
`verified-by: bravebot_core::policy::a_reference_that_names_no_file_is_not_a_destination`
`verified-by: bravebot_agent::turn::a_processors_output_cannot_be_a_destination`
`verified-by: bravebot_core::policy::a_choice_is_no_reference_to_any_gate_that_takes_text`
`verified-by: bravebot_agent::turn::a_choice_cannot_be_a_path_a_body_or_a_document_for_a_processor`

<a id="ROUTE-7"></a>
### ROUTE-7: before adding a tool, ask what its routing field is

If a person could not approve that field alone, the tool does not get built. A shell string is
destination and payload at once, which is why the planner has no shell and why `apply_patch` is
excluded. An argv vector passes the test, which is why running a pipeline of argv stages does not.
Code the planner wrote, shown whole to a person for one page, is the one other case, and
[ROUTE-11](#ROUTE-11) settles it.

`verified-by: bravebot_agent::tools::the_tool_set_is_reads_plus_gated_writes`
`verified-by: bravebot_agent::tools::only_run_takes_a_command_line`

<a id="ROUTE-8"></a>
### ROUTE-8: an endorsement authorises a destination without relabelling it

A destination reaches the effect at the label it arrived with, which for a path the planner
proposed is untrusted. What authorises it is the single-use endorsement for that exact path, and
the gate consumes the endorsement rather than changing the label. A destination carrying private
data is refused all the same, since a path derived from the user's data is that data in a
directory entry.

**Why.** Promotion exists for a read, so a write routed on a promoted path would have the model's
own proposal as the reason an effect landed where it did, which ROUTE-4 forbids. Consuming the
endorsement leaves no trusted path behind for another field to route on, and it puts a person
rather than a label behind every place an effect can reach.

`verified-by: bravebot_core::policy::an_endorsement_authorises_a_destination_it_does_not_relabel`
`verified-by: bravebot_core::policy::an_unendorsed_destination_is_refused`
`verified-by: bravebot_core::policy::a_private_destination_is_refused_even_when_endorsed`
`verified-by: bravebot_core::policy::an_endorsement_does_not_outlive_the_destination_it_authorised`
`verified-by: bravebot_agent::workspace::a_write_does_not_promote_its_destination`

<a id="ROUTE-9"></a>
### ROUTE-9: an action in a web page lands on the one element a person is shown in their browser

The tools of the browser's `input` and `input-trusted` groups act in a page the person has open.
Those of `input` click, type, choose, press a key, scroll and wait
([BROWSER-33](browser.md#BROWSER-33)). Those of `input-trusted` click, type, press a key, hover and
drag with real input that a second extension sends ([BROWSER-37](browser.md#BROWSER-37),
[BROWSER-38](browser.md#BROWSER-38)). The page decides which element a name leads to, as a site
decides what a URL leads to, so [ROUTE-7](#ROUTE-7) is met for them as follows.

The one tool of `capture-trusted`, `screenshot_full_page`, is held by the same rules and acts on no
element. It takes a picture of the whole page at one URL after the person allows it in a window of
the second extension ([BROWSER-43](browser.md#BROWSER-43)). Its routing field is
`brave-capture-trusted:screenshot_full_page` and its one argument is the URL. The result is a
picture, which is a reference the planner cannot read ([MCP-1](mcp.md#MCP-1)).

The one tool of `console-trusted`, `read_console`, is held by the same rules and acts on no element.
It reads the console of the page at one URL after the person allows it in a window of the second
extension ([BROWSER-44](browser.md#BROWSER-44)). Its routing field is
`brave-console-trusted:read_console`, and its arguments are the URL, one of three words and a count.
The result is text, which is a reference the planner cannot read ([MCP-1](mcp.md#MCP-1)).

The one tool of `upload-trusted`, `choose_file`, is held by the same rules and acts on one element, a
file input, which the second answer below shows. It gives a file of the person's to the page at one URL
after the person allows it in a window of the second extension ([BROWSER-45](browser.md#BROWSER-45)).
Its routing field is `brave-upload-trusted:choose_file`, and its arguments are the URL, a name, a role
where one is given and a path. The path is a word the planner wrote, which this process carries to the
server and decides nothing from. The server looks at the file it leads to inside a directory the person
declared and refuses by a closed list of sentences; the window shows the person the file's name, size and
full path and the input the page names. The path routes no effect on its own: where the file goes is the
page the URL names and the input the name leads to. This changes none of [ROUTE-2](#ROUTE-2),
[ROUTE-6](#ROUTE-6) and [LABEL-5](labels.md#LABEL-5). The result says only that the file was chosen.

The two tools of `screen`, `capture_screen` and `record_gif`, act on no element either, so the
second answer below does not apply to them. [ROUTE-7](#ROUTE-7),
[SERVERS-7](mcp-servers.md#SERVERS-7) and [SERVERS-21](mcp-servers.md#SERVERS-21) hold them as they
hold the tools of every server that asks every call ([BROWSER-42](browser.md#BROWSER-42)). Their
routing fields are `brave-screen:capture_screen`, which takes no argument, and
`brave-screen:record_gif`, whose arguments are the exact URL of one tab and a number of frames. The
question before each call draws them, as it draws the URL of `screenshot_page`. The person chooses
what `capture_screen` takes a picture of in the browser's own picker, so neither the planner nor a
page names it. A call from a context that has met untrusted content is refused with nobody asked,
and each result is a picture, which is a reference the planner cannot read
([MCP-1](mcp.md#MCP-1)).

**The routing field is the alias and the tool.** It is the alias the person declared the server
under and the tool, `brave-input:<tool>` or `brave-input-trusted:<tool>` as `install` prints it
([SERVERS-7](mcp-servers.md#SERVERS-7)). No rule, standing answer or mode reads an argument. The
arguments are words the planner wrote, which this process carries to the server and decides nothing
from: the exact URL of one open tab, a role from a fixed list and an accessible name or, for
`click_element`, `type_text` and `select_option`, in their place the number a line has in the list
`list_page_elements` made for that tab, and for the action its text, option, key, direction or
number, and for `drag_element` a second role and name for the element it drops on. No argument is
read as a selector, a node id, a coordinate, a tab id or a reference, except the `<argument>_ref`
that [ROUTE-10](#ROUTE-10) says. Elsewhere a string that looks like a reference is sent as written,
and the bytes a reference stands for are never an argument ([ROUTE-6](#ROUTE-6)).

**A person approves it alone, in two answers.** The alias, the tool and the words do not say which
element they lead to, because the page decides that. So the person answers twice before anything is
done, the second time about the element, and the action is aimed at the element they were shown or
at nothing. The tools of `input` act on the node itself, and those of `input-trusted` act at the
point of the element, which the Known costs below qualify:

1. The call is put to the person every time, and no allow rule, earlier answer or mode answers it
   ([SERVERS-21](mcp-servers.md#SERVERS-21)). The question draws the alias, the tool and every
   argument as the planner wrote them, except that an argument filled from a choice is drawn as
   the number and which reference it came from ([ROUTE-10](#ROUTE-10)). Bypassing puts the call to
   the person, and a one-shot run, a delegate and a client that cannot draw the question refuse it
   with nobody asked.
2. The extension, which is not the driver, looks for the element in the top document of that tab and
   fails where none, or more than one, has the role and name. For a number it takes the element the
   list recorded for that line and fails unless the page still shows it as it was listed
   ([BROWSER-41](browser.md#BROWSER-41)). It shows the person, in a window of
   the browser, the one element it found, with the page it is in, the words the page gave it and where
   it leads, and acts on that element only if the person allows it there
   ([BROWSER-34](browser.md#BROWSER-34)). For `input-trusted` the window is the second
   extension's. A drag shows both elements, the one it takes and the one it drops on. If the page
   opens a dialog while the input is sent, the person answers it in a panel of that extension, which
   shows the page's words as text, and the planner and this process give no answer to it
   ([BROWSER-38](browser.md#BROWSER-38)). `scroll_page` and `wait_for_element` act on no element and
   open no window, and the first answer is the only one they have.

**What the planner may write.** The arguments are the planner's own words, written in a context that
holds only trusted content: what the person wrote, files and configuration they vouched for, and a
result a person let through with `vet_content`, or one a check let through where auto-vetting is on
([CHECK-12](vetting.md#CHECK-12)). A call whose arguments were written in a context that has met
untrusted content is refused before anybody is asked and sends nothing
([LABEL-5](labels.md#LABEL-5), [SERVERS-7](mcp-servers.md#SERVERS-7)). After `vet_content` the
arguments are private, and every call is asked about whatever a rule says
([PERM-9](permissions.md#PERM-9)).

A number is a word of the same kind as a name. `list_page_elements` makes the page's controls into
a numbered text. That text is a tool's result, so it goes to the person, who reads it beside the role
and name of each line, and the planner is given a reference it cannot read
([MCP-1](mcp.md#MCP-1)). The schema accepts any integer in range and the driver cannot know where
the planner got the number. The person checks it against the line it stands for in the list they
were shown, and the extension shows the element the number stands for before anything is done to it.
A number a processor read off the list is no argument of its own, except as
[ROUTE-10](#ROUTE-10) says; a person who reads a processor's answer and gives the number in their
own words has written it themselves. The extension keeps the list for one URL for five minutes and
refuses a number it has no list for.

**What comes back.** A result of these tools is untrusted and private ([MCP-1](mcp.md#MCP-1)), and
the planner is given a reference it cannot read. The reference says whether the server reported a
failure ([MCP-5](mcp.md#MCP-5)). A page can make a call fail, by hiding an element or by giving two
the same name, and it sets the line and byte counts the reference states
([LABEL-3](labels.md#LABEL-3)). It sets no word the planner reads.

**What this clause does not allow.** This process choosing the target from what the page shows, and
a processor choosing it except as [ROUTE-10](#ROUTE-10) says. Elsewhere a processor's answer is a
reference that names no file and is not a destination ([ROUTE-6](#ROUTE-6)), a context that has
met untrusted content names no reference ([ROUTE-2](#ROUTE-2)), and no argument of a server's tool
is filled from a reference. A page's text reaches the planner only through a person's yes to
`vet_content`, or a check's word where auto-vetting is on, and the planner then writes the words
itself. This clause relaxes none of
ROUTE-2, ROUTE-6 and [LABEL-5](labels.md#LABEL-5). The events of `input` are synthetic, so a site
that checks for a person's, a file chooser and a JavaScript dialog are out of reach of `input`
([BROWSER-33](browser.md#BROWSER-33)). `input-trusted` sends real input through a second extension.
Except as [ROUTE-10](#ROUTE-10) says, neither group chooses its target from what the page shows:
the target is the element the planner's role and name led to, which the extension finds and the
person is shown. `choose_file` of `upload-trusted` gives a file to a file input in the same way
([BROWSER-45](browser.md#BROWSER-45)), and [BROWSER-29](browser.md#BROWSER-29) lists what is not
built.

**Known costs.** The person is shown the element and where it leads. A number says less than a name
in the question, since the line it stands for is in the transcript and not in the question, and the
page worded the line. What the page's own script
does when the click arrives is not shown ([BROWSER-34](browser.md#BROWSER-34)). Where auto-vetting is on, a
check's word can let a page's text reach the planner with nobody reading it (the third of the [Known
costs](labels.md#known-costs) of labels.md), and the planner may then name an element from it.
Neither answer is a check's to give, so each action still has both. A real click, press and release
of `input-trusted` land on a point in the window and not on a node. A page can put another element at
the point between the last check and the event, and the event reaches it. The second extension asks
the browser again after the move and before each press and the release of a drag, and the document
with it, and the focus before a key and before typed text, which narrows the time and does not
remove it ([BROWSER-38](browser.md#BROWSER-38)), and the person's second answer is
about the element they were shown and not about what is at the point when the event is sent. The
second extension also holds the debugger on every site while it is enabled
([BROWSER-37](browser.md#BROWSER-37)), so the window and the first answer are what stand between a
call and the page.

**Why.** A person can judge a page, a role and a name as they judge a URL. They cannot judge a
coordinate, a node id or a selector read from a page, which says nothing about which element it is
and comes from the page, so no tool takes one. A line number of a list they read is not one of
those: it stands for the role and name beside it, which they read, and the window shows the element.
The page decides which element the words lead to, so the second answer is given about the element
itself, in a window the page cannot rewrite.

`verified-by: bravebot_agent::browser_group_servers::a_call_to_an_input_tool_from_a_context_that_has_met_untrusted_content_is_refused_before_anybody_is_asked`
`verified-by: bravebot_agent::browser_group_servers::neither_a_rule_nor_bypassing_answers_a_call_to_an_input_tool`
`verified-by: bravebot_agent::browser_group_servers::a_run_with_nobody_to_ask_refuses_each_call_to_an_input_tool`
`verified-by: bravebot_agent::browser_group_servers::a_delegate_is_refused_each_call_to_an_input_tool_with_nobody_asked`
`verified-by: bravebot_agent::browser_group_servers::what_a_tool_of_input_answers_is_a_reference_and_the_planner_never_holds_it`
`verified-by: bravebot_agent::browser_group_servers::a_reference_named_in_an_argument_of_an_input_tool_reaches_the_server_as_the_words_written`
`verified-by: bravebot_agent::browser_group_servers::after_a_vet_the_planner_acts_on_a_page_and_every_call_it_makes_is_put_to_the_person`
`verified-by: bravebot_agent::browser_group_servers::a_call_to_a_trusted_input_tool_from_a_context_that_has_met_untrusted_content_is_refused_before_anybody_is_asked`
`verified-by: bravebot_agent::browser_group_servers::neither_a_rule_nor_bypassing_answers_a_call_to_a_trusted_input_tool`
`verified-by: bravebot_agent::browser_group_servers::a_run_with_nobody_to_ask_refuses_each_call_to_a_trusted_input_tool`
`verified-by: bravebot_agent::browser_group_servers::a_delegate_is_refused_each_call_to_a_trusted_input_tool_with_nobody_asked`
`verified-by: bravebot_agent::browser_group_servers::what_a_tool_of_input_trusted_answers_is_a_reference_and_the_planner_never_holds_it`
`verified-by: bravebot_agent::browser_group_servers::a_reference_named_in_an_argument_of_a_trusted_input_tool_reaches_the_server_as_the_words_written`
`verified-by: bravebot_agent::browser_group_servers::after_a_vet_the_planner_acts_on_a_page_and_every_call_to_a_trusted_input_tool_is_put_to_the_person`
`verified-by: bravebot_tui::confirm::the_arguments_of_an_input_call_take_no_yes_before_all_of_them_are_drawn`
`verified-by: bravebot_ui_bridge::refusal::the_window_refuses_a_call_asked_every_time_without_asking`
`verified-by: bravebot_browser::tools::only_cookies_remove_data_scripts_the_input_groups_capture_trusted_console_trusted_upload_trusted_and_screen_are_asked_about_every_call`
`verified-by: bravebot_browser::relay::installing_declares_only_remove_data_cookies_scripts_the_input_groups_capture_trusted_console_trusted_upload_trusted_and_screen_to_ask_about_every_call`
`verified-by: bravebot_browser::tools::every_argument_of_an_input_tool_is_a_word_from_a_list_or_has_a_largest_size`
`verified-by: bravebot_browser::tool_surface::every_argument_of_every_tool_is_one_a_person_can_judge_in_the_question`
`verified-by: bravebot_agent::browser_group_servers::an_element_number_is_asked_about_and_sent_as_written_and_is_refused_from_a_context_that_has_met_untrusted_content`
`verified-by: bravebot_browser::tools::an_element_is_named_by_a_role_and_a_name_or_by_the_number_a_list_gave`
`verified-by: by-construction (extension/tests/input.test.mjs asserts that a name matching none or several elements fails and acts on nothing, that nothing the planner wrote is evaluated or used as a selector, and that the function that acts takes the token of the browser window and refuses without it; extension/tests/confirmation.test.mjs asserts that the window shows the element and where it leads as text, that only a click the person made allows, and that a denial, a closed window and a timeout allow nothing; extension-trusted/tests/tools.test.mjs asserts that the second extension sends no input before the person has allowed it in its window, that a drag shows both elements and sends nothing if either changed, and that no argument is a coordinate, a selector or a node number; extension/tests/trusted.test.mjs asserts that the BraveBot extension sends a call of input-trusted to it only for an allowed site that is not blocked; browser.md lists them under the clauses for the tools of input, of input-trusted and for the windows, and they run with the rest of the extension tests)`
`verified-by: bravebot_agent::browser_group_servers::a_call_to_the_capture_tool_from_a_context_that_has_met_untrusted_content_is_refused_before_anybody_is_asked`
`verified-by: bravebot_agent::browser_group_servers::neither_a_rule_nor_bypassing_answers_a_call_to_the_capture_tool`
`verified-by: bravebot_agent::browser_group_servers::a_run_with_nobody_to_ask_refuses_a_call_to_the_capture_tool`
`verified-by: bravebot_agent::browser_group_servers::a_delegate_is_refused_a_call_to_the_capture_tool_with_nobody_asked`
`verified-by: bravebot_agent::browser_group_servers::a_reference_named_in_an_argument_of_the_capture_tool_reaches_the_server_as_the_words_written`
`verified-by: bravebot_agent::browser_group_servers::after_a_vet_a_call_to_the_capture_tool_is_put_to_the_person`
`verified-by: bravebot_agent::browser_group_servers::a_call_to_a_screen_tool_from_a_context_that_has_met_untrusted_content_is_refused_before_anybody_is_asked`
`verified-by: bravebot_agent::browser_group_servers::neither_a_rule_nor_bypassing_answers_a_call_to_a_screen_tool`
`verified-by: bravebot_agent::browser_group_servers::a_run_with_nobody_to_ask_refuses_each_call_to_a_screen_tool`
`verified-by: bravebot_agent::browser_group_servers::a_delegate_is_refused_each_call_to_a_screen_tool_with_nobody_asked`
`verified-by: bravebot_agent::browser_group_servers::what_a_tool_of_screen_answers_is_a_reference_and_the_planner_never_holds_it`
`verified-by: bravebot_agent::browser_group_servers::a_reference_named_in_an_argument_of_a_screen_tool_reaches_the_server_as_the_words_written`
`verified-by: bravebot_agent::browser_group_servers::after_a_vet_a_call_to_a_screen_tool_is_put_to_the_person`

<a id="ROUTE-10"></a>
### ROUTE-10: a processor may nominate the element of a page action, and the action lands on it only if the person allows it where they see it

A processor given the list `list_page_elements` made ([BROWSER-41](browser.md#BROWSER-41)) may be
asked to choose one entry ([PROC-16](processors.md#PROC-16)). What it answers is read for a number
by a crate that is not the driver ([LABEL-4](labels.md#LABEL-4)), and the number is held under a
reference of its own kind. A choice names no file, so it is not a destination
([ROUTE-6](#ROUTE-6)), and it is not the routing field of a call, which stays the alias and the tool
([ROUTE-9](#ROUTE-9)).

**One argument of one tool.** A choice may fill an argument only where a person's declaration of the
server names that tool and that argument as taking one ([SERVERS-22](mcp-servers.md#SERVERS-22)).
`bravebot-browser install --groups` prints the declaration of `input` with `element_number` of
`click_element`, `type_text` and `select_option`. The planner names the choice as `<argument>_ref`,
from a context that has met nothing untrusted ([LABEL-5](labels.md#LABEL-5),
[MCP-14](mcp.md#MCP-14)).

**What authorises it.** The choice reaches the call at the label it has, untrusted and private. What
authorises it is the person's answer to the call, which draws the number and which reference it
came from, and then the person's answer about the element the number leads to, in the window of the
program that acts ([ROUTE-9](#ROUTE-9)). No rule, standing answer or mode gives either answer. A
one-shot run, a delegate and a client that cannot put the question refuse the call, and a server
that does not ask every call cannot declare a choice ([SERVERS-21](mcp-servers.md#SERVERS-21)).

**When a call is refused.** A call that names a choice is refused, and sends nothing, where the
argument is also written out, where more than one choice is named, where the reference is not a
choice a processor made or is one a call has used, where the context has met untrusted content, and
where any other argument of the call is private. A choice is spent by the call it fills, and a call
refused or declined before that leaves it unspent. A plan has no field for one
([MANIFEST-15](manifest.md#MANIFEST-15)).

**What this clause does not allow.** A choice in any other argument, any other reference in an
argument, a choice for a tool whose program does not show the person the element it leads to, a
rule, mode or check giving either answer, and this process or a processor deciding that the action
happens or what a 0 means.

**Known costs.** The page decides which elements a processor is shown and can talk the processor
round, so it decides which element the person is asked about. The person can allow or deny and is
given no list to choose from in the question, which shows the number and that a processor chose it
and not the line the number stands for. A person who allows without comparing the element with what
they asked has let the page choose. The window shows the element, and the extension does not know
who supplied the number. What the page's own script does when the click arrives is not shown
([BROWSER-34](browser.md#BROWSER-34)). A call whose other arguments are private is asked about and
refused after the answer. The read of the answer is the ninth of the
[Known costs](labels.md#known-costs). A page can also make the processor's request fail, and the
planner is then told one word of a closed set the driver wrote ([PROC-16](processors.md#PROC-16)).

**Why.** The number is one of at most a hundred values, it is not trusted, and nothing relies on it
being trusted. The action needs the person's answer about the one element it leads to, as the action
on a name does ([ROUTE-9](#ROUTE-9)). That is the arrangement of [ROUTE-8](#ROUTE-8): a value reaches
the effect at the label it arrived with, and a single-use answer for the exact call authorises it.

`verified-by: bravebot_core::policy::carry_a_choice_consumes_the_endorsement_spends_the_choice_and_refuses_before_spending`
`verified-by: bravebot_core::policy::the_trail_of_carrying_a_choice_is_the_same_for_every_number_and_holds_no_digit`
`verified-by: bravebot_core::policy::a_choice_is_drawn_as_digits_to_the_person_and_only_while_it_can_fill_a_call`
`verified-by: bravebot_core::slot::a_choice_is_read_without_being_used_and_used_once`
`verified-by: bravebot_agent::browser_group_servers::a_choice_fills_only_the_argument_the_declaration_named`
`verified-by: bravebot_agent::browser_group_servers::an_undeclared_tool_sends_a_ref_argument_as_written`
`verified-by: bravebot_agent::browser_group_servers::a_free_text_answer_cannot_be_named_as_a_choice`
`verified-by: bravebot_agent::browser_group_servers::a_choice_call_is_refused_and_sends_nothing_where_the_argument_is_also_written_out`
`verified-by: bravebot_agent::browser_group_servers::a_choice_call_is_refused_and_sends_nothing_where_another_argument_is_private`
`verified-by: bravebot_agent::browser_group_servers::a_choice_call_is_refused_and_sends_nothing_where_the_context_has_met_untrusted_content`
`verified-by: bravebot_agent::browser_group_servers::a_spent_choice_cannot_fill_a_second_call`
`verified-by: bravebot_agent::browser_group_servers::a_refused_call_does_not_spend_the_choice`
`verified-by: bravebot_agent::browser_group_servers::a_no_to_the_question_does_not_spend_the_choice`
`verified-by: bravebot_agent::browser_group_servers::the_question_draws_the_number_and_where_it_came_from`
`verified-by: bravebot_agent::browser_group_servers::no_denial_or_sentence_to_the_planner_holds_a_digit`
`verified-by: bravebot_agent::browser_group_servers::whatever_the_processor_answered_the_driver_makes_the_same_calls`
`verified-by: bravebot_agent::browser_group_servers::neither_a_rule_nor_bypassing_answers_a_call_with_a_choice`
`verified-by: bravebot_agent::browser_group_servers::a_run_with_nobody_to_ask_refuses_a_call_with_a_choice`
`verified-by: bravebot_agent::browser_group_servers::a_delegate_is_refused_a_call_with_a_choice_with_nobody_asked`
`verified-by: bravebot_agent::manifest_server_calls::a_plan_cannot_name_a_choice`
`verified-by: bravebot_tui::confirm::mcp::a_choice_is_drawn_as_the_number_and_where_it_came_from_at_every_width`
`verified-by: bravebot_tui::confirm::a_call_filled_from_a_choice_takes_no_yes_before_the_number_and_its_source_are_drawn`
`verified-by: bravebot_cli::plain::a_call_filled_from_a_choice_is_asked_with_the_number_and_where_it_came_from`
`verified-by: bravebot_ui_bridge::acp::an_editor_reads_the_number_of_a_choice_and_where_it_came_from_as_the_terminal_draws_them`
`verified-by: bravebot_ui_bridge::refusal::the_window_refuses_a_call_a_choice_fills_without_asking`

<a id="ROUTE-11"></a>
### ROUTE-11: code the planner wrote is a routing field that a person can approve alone where they read all of it for one page, are asked every time, and the clause that allows it states what limits it

The tool `run_script` of the browser's `scripts` group runs code in a web page
([BROWSER-46](browser.md#BROWSER-46)). Code in a page is destination and payload at once, as a shell
string is ([ROUTE-7](#ROUTE-7)): it names the elements it acts on, the requests it makes and the data it
carries in one string, and nothing in this process turns it into a smaller thing a rule can match. It
is built because a person can approve that field alone when all of these hold, and no other tool that
runs code is built without them.

1. **The code is the planner's own argument, and nothing here reads it.** The planner writes it in a
   context that has met nothing untrusted. A call whose arguments were written in a context that has met
   untrusted content is refused before anybody is asked and sends nothing
   ([LABEL-5](labels.md#LABEL-5), [SERVERS-7](mcp-servers.md#SERVERS-7)). No byte of a reference or of a
   processor's answer is in `code` or `url`. A string that looks like a reference is sent as written
   ([ROUTE-9](#ROUTE-9)). The one number a processor's answer can supply is the integer of
   [ROUTE-10](#ROUTE-10), which a declaration names for an argument of one tool, and the table the
   relay prints its declarations from (`CHOICES` in `crates/browser/src/install.rs`) names
   `element_number` of the three `input` tools and no argument of `scripts`. This process carries `code`
   to the server and compares it with nothing and branches on none of it. The one reading of it is the
   drawing of the question (condition 2), which splits it into lines and writes out the characters a
   person cannot tell from another or from nothing, as a command or a file name is drawn, and nothing
   takes a decision from that drawing. The extension, which is not the driver, counts it, refuses a
   `code` that is only the name of a reference before anybody is asked, and shows it.
2. **The person reads all of the code and the page it runs in, in the browser.** A window of the
   extension shows the exact address of one open tab and every line of the code, with each character a
   person cannot see written out, and nothing runs until the person presses Allow once there
   ([BROWSER-34](browser.md#BROWSER-34)). Code too long to show whole is refused and is never cut. The
   planner and this process cannot answer that window. The question put before the call draws the same
   code in the terminal and the plain interface. Code with more than one line or with a character a
   person cannot tell from another or from nothing is drawn as one numbered row to a line, under a row
   that counts the lines, with each such character written out and counted
   ([SERVERS-7](mcp-servers.md#SERVERS-7), [PROMPT-13](prompting.md#PROMPT-13)). Code of one line with
   no such character is drawn as its JSON, on one row. The desktop window and
   an editor draw no such question, since they refuse a call to a server that asks every call with
   nobody asked ([SERVERS-21](mcp-servers.md#SERVERS-21)).
3. **The call is asked every time.** The server of the group is declared to ask about every call, so
   no rule, standing answer, mode or remembered answer answers the question before the call, bypassing
   puts it to the person, and a one-shot run, a delegate and a client that cannot draw the question
   refuse it with nobody asked ([SERVERS-21](mcp-servers.md#SERVERS-21)). The person's yes in the window
   is for that code and that page and is spent when the code starts. The code starts once. Timers,
   listeners and observers it sets keep running in the page until the page is closed or reloaded, and
   each can read and send without asking again.
4. **The result is untrusted and private** ([MCP-1](mcp.md#MCP-1)). The planner is given a reference it
   cannot read, a processor reads it only through that reference ([PROC-1](processors.md#PROC-1)), and
   the person reads it. The reference says whether the server reported a failure
   ([MCP-5](mcp.md#MCP-5)).
5. **The clause that allows the tool states what limits the code, and says so where only the person's
   reading does.** Nothing here relies on a sandbox for it, and [BROWSER-46](browser.md#BROWSER-46)
   states what the browser's own policy for the code stops and what it does not, as observed in Brave
   1.98.62 and 1.98.63 on macOS. In those versions it did not stop a navigation, a form submission, a
   frame that loads an address, or `window.open` once the page has had a click, so the code could send
   what it read to another address and act through the page's own buttons and forms as the person who
   is signed in. The one limit on what the code reaches is that the person read all of it.

**The routing field is the alias and the tool**, `brave-scripts:run_script` as `install` prints it
([SERVERS-7](mcp-servers.md#SERVERS-7)). No rule reads an argument. The arguments are the exact address
of one open tab and the code. A person can approve the field alone: the window puts the address and
every line of the code in front of them, the code is the whole of what is run, and nothing is added to
it after they answer.

**Why.** [ROUTE-7](#ROUTE-7) refuses a shell string because nothing but the string limits it and no
component can read it for a person. `run` meets the test because the driver compiles a line to resolved
programs and literal arguments, which a person approves, and a sandbox limits what they do
([RUN-1](tools/run.md#RUN-1), [SANDBOX-20](sandboxing.md#SANDBOX-20)). Code in a page has no compiled form,
and nothing in this tree confines what it does in the page. This clause accepts that the approval is
the person's reading and nothing more, and keeps it from being less: the person is shown all of the
code for one page every time, and a code they cannot be shown whole is not run.

**Known costs.** The code can contain a condition on the page's content, so the page decides which of
the branches the person already read is taken. The person read every branch, and a page can choose
among them and add none. A person can misjudge code they read, and the window has no word to help them
beyond the one that says nothing limits it. The planner writes the code without reading the page, since
it never holds page content, so useful code does not depend on content the planner has not seen, or
depends on what the person told it, or on what the person let through with `vet_content`
([LABEL-5](labels.md#LABEL-5)). This clause relaxes none of [ROUTE-2](#ROUTE-2), [ROUTE-6](#ROUTE-6)
and [LABEL-5](labels.md#LABEL-5), adds no label and no gate, and puts no `declassify` in the driver.

`verified-by: bravebot_agent::browser_group_servers::a_call_to_the_script_tool_from_a_context_that_has_met_untrusted_content_is_refused_before_anybody_is_asked`
`verified-by: bravebot_agent::browser_group_servers::neither_a_rule_nor_bypassing_answers_a_call_to_the_script_tool`
`verified-by: bravebot_agent::browser_group_servers::a_run_with_nobody_to_ask_refuses_a_call_to_the_script_tool`
`verified-by: bravebot_agent::browser_group_servers::a_delegate_is_refused_a_call_to_the_script_tool_with_nobody_asked`
`verified-by: bravebot_agent::browser_group_servers::what_a_tool_of_scripts_answers_is_a_reference_and_the_planner_never_holds_it`
`verified-by: bravebot_agent::browser_group_servers::a_reference_named_in_an_argument_of_the_script_tool_reaches_the_server_as_the_words_written`
`verified-by: bravebot_agent::browser_group_servers::a_reference_named_in_the_code_of_the_script_tool_reaches_the_server_as_the_words_written`
`verified-by: bravebot_agent::browser_group_servers::after_a_vet_a_call_to_the_script_tool_is_put_to_the_person`
`verified-by: bravebot_agent::confirm::a_string_argument_with_line_feeds_is_drawn_one_numbered_row_to_a_line`
`verified-by: bravebot_agent::confirm::a_string_argument_writes_out_what_cannot_be_seen_and_counts_it`
`verified-by: bravebot_agent::confirm::a_string_argument_marks_what_the_confirmation_window_marks`
`verified-by: bravebot_agent::confirm::a_written_out_form_typed_in_a_string_argument_is_not_taken_for_one_the_question_wrote`
`verified-by: bravebot_tui::confirm::mcp::a_call_carrying_code_draws_every_line_and_takes_no_yes_before_the_last_line_is_drawn`
`verified-by: bravebot_tui::confirm::mcp::a_short_script_is_drawn_with_its_address_its_lines_and_the_three_answers`
`verified-by: bravebot_cli::plain::a_script_is_asked_about_in_lines_with_every_line_of_the_code_and_the_unseen_written_out`
`verified-by: bravebot_browser::tools::run_script_is_in_scripts_alone_and_takes_a_url_and_code_with_their_largest_sizes`
`verified-by: bravebot_browser::tools::the_script_description_says_the_person_reads_all_the_code_nothing_else_limits_it_and_the_result_is_private`
`verified-by: bravebot_browser::relay::a_script_is_called_through_its_group_alone_and_the_extension_is_sent_the_code_as_written`
`verified-by: bravebot_browser::install::the_declared_choices_name_no_tool_of_scripts_and_no_argument_that_holds_code_or_an_address`
`verified-by: by-construction (extension/tests/scripts.test.mjs asserts that the code reaches the browser's userScripts API as written and only after the person was asked and allowed it, that a refused, denied, closed or unanswered question and a token made for another call run nothing, that a code that is blank, over the limit or only the name of a reference, a site that is blocked or not allowed, a tab that is not exactly one and an API the person's switch leaves unusable are each refused before anybody is asked, that the person is given the complete code and the exact address and nothing the page wrote, that a second call is refused while one waits for the person, and that the activity log keeps no code; extension/tests/confirmation.test.mjs asserts that the window shows every line of the code with each line feed and each unseen character written out, shows no element, and cannot be allowed for code it cannot show whole; extension/tests/groups.test.mjs asserts that the group is in no preset, lists userScripts and the hosts of the allowed sites and never all sites; all run with the rest of the extension tests, as browser.md says under the clause for the tool)`
