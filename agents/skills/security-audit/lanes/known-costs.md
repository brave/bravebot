### the named exceptions, and what is missing from their lists

`{labels_spec}` has a `## Known costs` section. It records the places the driver does look at
untrusted bytes in order to decide something, and beside each it enumerates what an attacker who owns
the bytes gains. The reason it is written that way is stated there: the shape "it looks at untrusted
bytes" is exactly the shape of a real hole, so the argument that this one is acceptable has to be on
the page.

Your job is not to decide whether the exceptions should exist. It is to check the enumerations are
still complete against the code as it stands. Each list ends with the thing that is deliberately not
on it, and that is the claim to attack.

For the processor answer split, the list says an attacker can leave the mark out, move or duplicate
it, put words in the remark, or add and drop a trailing newline. It then says: "What is not on the
list is the thing that would matter: choosing *which* file is written."

For the vetting verdict, the list says an attacker can force `safe`, force `unsafe`, or put words in
the reason. It then says what is not on the list: "choosing which slot is checked, choosing any
destination, lowering confidentiality, promoting anything without a person's approval, writing a trust
rule, or reaching the planner at all."

For the search comparison, the list says an attacker can set the result's line and byte counts, make
the search report itself incomplete, tell the planner how many matches there are when it asks for a
page past the last one, choose which matches fall on which page, and set the tally the person sees.
It then says what is not on the list: which files the walk reaches, the label of the result, the
planner's own arguments, the text of any line reaching the planner, and any destination.

So, for each exception:

1. Read the code that implements it, and the clauses around it. The processor path is
   `crates/agent/src/processor.rs` and the composing and splitting gates in
   `crates/core/src/policy.rs`. The vetting path is `crates/core/src/vetting.rs`,
   `crates/agent/src/vet.rs`, and the vetting gates in `policy.rs`. The search path is
   `Workspace::grep_shaped` in `crates/agent/src/workspace.rs` and the `search` function in
   `crates/agent/src/tools.rs`, which decides what of the result is released beside the reference.
2. Take each item the list says an attacker **cannot** reach, and try to reach it. Steering only the
   bytes, is there a path to it? A path exists where some later decision reads a value that the split
   or the verdict influenced, even indirectly: a length, an offset, a count, a name, a flag.
3. Check the residues the lists admit are still only residues. The first two say a plausible remark
   or reason might persuade somebody to skim. Confirm the thing that makes that survivable is still
   true: the write is approved from a diff of the actual bytes, and the remark is drawn beside that
   diff. The search says the planner reads some numbers and can be moved by them. Confirm that only
   the result's line and byte counts, whether it is incomplete, the next offset and the total past
   the end reach the planner, and that no byte of a file reaches an argument of a later call.
4. Check the count. The section names a number of places, and a place that branches on untrusted bytes
   and is not in the section is not an exception, it is a violation. If you find one, that is the
   finding, and `{review_doc}` says why: "an unlisted exception is indistinguishable from a violation".

A candidate here is an item that should be on one of those lists and is not, or an item the list says
is out of reach that you reached. Quote the list you are amending.
