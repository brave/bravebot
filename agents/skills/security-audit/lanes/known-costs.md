### the named exceptions, and what is missing from their lists

`{labels_spec}` has a `## Known costs` section. It records the places the policy layer does look at
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

So, for each exception:

1. Read the code that implements it, and the clauses around it. The processor path is
   `crates/agent/src/processor.rs` and the composing and splitting gates in
   `crates/core/src/policy.rs`. The vetting path is `crates/core/src/vetting.rs`,
   `crates/agent/src/vet.rs`, and the vetting gates in `policy.rs`.
2. Take each item the list says an attacker **cannot** reach, and try to reach it. Steering only the
   bytes, is there a path to it? A path exists where some later decision reads a value that the split
   or the verdict influenced, even indirectly: a length, an offset, a count, a name, a flag.
3. Check the residues the lists admit are still only residues. Both say a plausible remark or reason
   might persuade somebody to skim. Confirm the thing that makes that survivable is still true: the
   write is approved from a diff of the actual bytes, and the remark is drawn beside that diff.
4. Check the count. The section names a number of places, and a place that branches on untrusted bytes
   and is not in the section is not an exception, it is a violation. If you find one, that is the
   finding, and `{review_doc}` says why: "an unlisted exception is indistinguishable from a violation".

A candidate here is an item that should be on one of those lists and is not, or an item the list says
is out of reach that you reached. Quote the list you are amending.
