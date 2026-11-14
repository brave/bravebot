# Verification

<!-- applicability: paths:crates/,verify/ -->

What a reviewer checks about a Verus contract, which `make check-verus` cannot: it reports that the
code meets the contract, never that the contract says the right thing or that a change needed one.
[development/verification.md](../development/verification.md) is how a contract is written and run.

---

<a id="VER-001"></a>

## Code a reviewer would otherwise trust gets a contract

**A gate, a change to the label lattice or to `Labelled`, and arithmetic, an index or a slice on a
length or an offset from outside come with a Verus contract where the crate is set up for one.** A
test shows the inputs somebody chose; a contract states the property for every input, and a later
edit that breaks it fails `make check-verus`. A fix for a panic or an overflow comes with the
contract that refuses the old code and a mutation in `verify/mutations.txt` that restores it.

Where the crate is not set up for Verus yet, the pull request says so and names the property a
contract would state, so the gap is a sentence somebody can act on rather than nothing.

---

<a id="VER-002"></a>

## A contract says what the code is for, and can fail

**A contract is written from the meaning of the function, not from its body, and ships with a
mutation Verus refuses.** An `ensures` that restates the body proves the code equals itself, and a
contract no broken body could fail proves nothing. Read the contract against what the function is
for, and the mutation against the defect the contract exists to refuse. A contract that changes in
`verify/contracts.txt` is weaker or stronger than it was, and the pull request says which and why.

A function marked `external_body` is assumed rather than proved. A new one is named in the pull
request, with the reason Verus cannot read it, and is small enough to check by eye.
