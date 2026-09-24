#!/usr/bin/env python3
"""Prove the mechanical half still reports what it was written to report.

    python3 agents/skills/security-audit/selftest.py

A check that stops firing is worse than one that was never written, because the tree it was supposed
to hold goes on looking clean. So each check is run against a fixture that violates it and a fixture
that does not, and the pass is that it fires on the first and stays silent on the second.

Two of these run against the real tree rather than a fixture. Both are about the two halves of this
skill agreeing with something outside it: that the counts pinned in `docs/specs/labels.md` are the
counts this enumerator measures, and that every lane prompt still composes. A drift in either is
invisible in a run, because a lane whose prompt failed to fill in just produces a thinner audit.
"""

import importlib.util
import io
import json
import os
import re
import sys
import tempfile
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path

HERE = Path(__file__).resolve().parent

# A `.format()` placeholder that came through unfilled. An unknown key raises, so this catches the
# other direction: a lane naming a list the enumerator no longer computes.
SURVIVING = re.compile(r"\{[a-z][a-z_]*\}")


def load(name):
    spec = importlib.util.spec_from_file_location(name.replace("-", "_"), HERE / f"{name}.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


ROOT = Path.cwd()
audit = load("security-audit")
verifying = load("verify-findings")
collect = load("collect-findings")
drafts = load("draft-issues")
posting = load("post-issues")

FAILURES = []


def check(name, condition, detail=""):
    if condition:
        print(f"  ok    {name}")
        return True
    print(f"  FAIL  {name}" + (f": {detail}" if detail else ""))
    FAILURES.append(name)
    return False


def kinds(found):
    return sorted(item["kind"] for item in found)


class FakeSpec:
    """A spec that pins the symbols named and nothing else.

    `named` is for the checks that read both halves of a `guards` entry: a symbol a spec names
    without counting its call sites is pinned by neither this mapping nor `check-spec`, and it is
    a different thing from one no spec mentions at all. Pinning a symbol names it too, which is
    what a real spec does, so the default follows `pinned`.

    `sites` is for a check that reads which files an entry counts rather than only that one
    exists, and a symbol left out of it pins no file, the way a `guards` entry with no `sites:`
    does.
    """

    def __init__(self, pinned, named=None, sites=None):
        self.allowlists = {symbol: list((sites or {}).get(symbol, [])) for symbol in pinned}
        self.guards = list(pinned if named is None else named)
        self.front = {}


def in_tree(files):
    """Run a check with the working directory set to a tree built out of `files`."""
    holding = tempfile.mkdtemp(prefix="security-audit-selftest-")
    for name, body in files.items():
        path = Path(holding) / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(body, encoding="utf-8")
    return holding


def with_cwd(where, work):
    was = Path.cwd()
    os.chdir(where)
    try:
        return work()
    finally:
        os.chdir(was)


# The sentence each document uses to count the admitted exceptions. The check reads prose, so the
# fixtures are prose, and a rewording that stops matching is the thing being caught.
REVIEW_TWO = "Two places in the kernel do branch on untrusted bytes, deliberately.\n"
REVIEW_THREE = "Three places in the kernel do branch on untrusted bytes, deliberately.\n"
SPEC_THREE = "- **Three places in the policy layer do look at untrusted bytes to decide.**\n"


def test_exception_counts():
    disagreeing = in_tree(
        {
            "docs/development/reviewing-for-the-rule.md": REVIEW_TWO,
            "docs/specs/labels.md": SPEC_THREE,
        }
    )
    found = with_cwd(disagreeing, lambda: list(audit.check_exception_counts()))
    check(
        "a document naming fewer exceptions than the spec is an error",
        kinds(found) == ["exception-count-disagreement"]
        and found[0]["severity"] == audit.ERROR,
        str(kinds(found)),
    )

    agreeing = in_tree(
        {
            "docs/development/reviewing-for-the-rule.md": REVIEW_THREE,
            "docs/specs/labels.md": SPEC_THREE,
        }
    )
    found = with_cwd(agreeing, lambda: list(audit.check_exception_counts()))
    check("two documents naming the same number are clean", found == [], str(kinds(found)))

    silent = in_tree(
        {
            "docs/development/reviewing-for-the-rule.md": "Read the diff carefully.\n",
            "docs/specs/labels.md": SPEC_THREE,
        }
    )
    found = with_cwd(silent, lambda: list(audit.check_exception_counts()))
    check(
        "a document that counts nothing is a warning, not an error",
        kinds(found) == ["exception-count-unstated"] and found[0]["severity"] == audit.WARNING,
        str(kinds(found)),
    )


# A register with one admitted second client, as the tree has. The bullet has to be about opening a
# socket for the crates it names to be read out of it, so a crate mentioned in that section for some
# other reason does not become permitted to keep a client.
EGRESS_SPEC = """---
id: NET
title: Network egress
---

## Clauses

<a id="NET-1"></a>
### NET-1: this process has one way out, and it is not optional

Every outbound request this process makes goes through a single call, and no other crate depends
on it.

## Known costs

- **`bravebot-net` is not the only crate that opens a socket.** `bravebot-skus` builds its own
  HTTP client for the subscription service. That traffic carries a credential and an order id,
  never workspace content.

- **The machine's own trust store is not read.** `SSL_CERT_FILE` is, and `bravebot-config` is what
  reads it, which is a mention of a crate and not an admission that it opens anything.
"""

# The same register, with a second admission for a crate the fixtures below give a client to.
EGRESS_SPEC_ADMITTING_LSP = EGRESS_SPEC + """
- **`bravebot-lsp` keeps a client too.** It opens its own socket for the registry it downloads
  language servers from.
"""

# The same register with the second-egress bullet taken out, for a fixture where the gate crate is
# the only crate there is.
EGRESS_SPEC_ADMITTING_NOBODY = EGRESS_SPEC.replace(
    """- **`bravebot-net` is not the only crate that opens a socket.** `bravebot-skus` builds its own
  HTTP client for the subscription service. That traffic carries a credential and an order id,
  never workspace content.

""",
    "",
)


def crate_manifest(name, *deps):
    declared = "".join(f"{one} = {{ workspace = true }}\n" for one in deps)
    return f'[package]\nname = "{name}"\n\n[dependencies]\n{declared}'


# The workspace as the tree has it: the gate crate and the one recorded exception.
RECORDED_WORKSPACE = {
    "docs/specs/network-egress.md": EGRESS_SPEC,
    "crates/net/Cargo.toml": crate_manifest("bravebot-net", "ureq"),
    "crates/skus/Cargo.toml": crate_manifest("bravebot-skus", "ureq"),
    "crates/config/Cargo.toml": crate_manifest("bravebot-config", "serde_json"),
}

# The claim as the file carried it, on one line.
CLAIM = """//! The single network egress path.
//!
//! Every outbound request in the process goes through [`Egress::fetch`]. The HTTP
//! client is private to this module and no other crate depends on `ureq`, so there is
//! no second path that could skip the policy gate.

pub struct Egress;
"""

# The same claim one word further along the margin, so the sentence ends one line and names the
# crate on the next. A doc comment wraps wherever the margin falls, so where the break lands is an
# accident of the sentence before it, and a scan reading one line at a time sees neither half.
CLAIM_ACROSS_TWO_LINES = """//! The single network egress path.
//!
//! Every outbound request in the process goes through [`Egress::fetch`]. The HTTP client
//! is private to this module and no other crate depends on
//! `ureq`, so there is no second path that could skip the policy gate.

pub struct Egress;
"""

CLAIM_CORRECTED = """//! The single network egress path.
//!
//! Every outbound request carrying labelled content goes through [`Egress::fetch`]. One other
//! crate opens a socket, recorded under `## Known costs` in `docs/specs/network-egress.md`.

pub struct Egress;
"""


def audit_egress(files):
    """Run the check over a fixture tree, reading its sources the way a real run reads them."""
    where = in_tree(files)
    return with_cwd(
        where,
        lambda: list(audit.check_network_clients_are_recorded(audit.mechanics.load_sources())),
    )


def test_network_clients_are_recorded():
    found = audit_egress(
        {
            **RECORDED_WORKSPACE,
            "crates/lsp/Cargo.toml": crate_manifest("bravebot-lsp", "reqwest"),
        }
    )
    check(
        "a crate the register does not admit keeping a client is an error",
        kinds(found) == ["unrecorded-network-client"]
        and found[0]["severity"] == audit.ERROR
        and "bravebot-lsp" in found[0]["summary"]
        and "reqwest" in found[0]["summary"],
        str([one["summary"] for one in found]),
    )

    found = audit_egress(
        {
            **RECORDED_WORKSPACE,
            "docs/specs/network-egress.md": EGRESS_SPEC_ADMITTING_LSP,
            "crates/lsp/Cargo.toml": crate_manifest("bravebot-lsp", "reqwest"),
        }
    )
    check("the same crate, once the register admits it, is clean", found == [], str(kinds(found)))

    found = audit_egress(
        {
            **RECORDED_WORKSPACE,
            "crates/lsp/Cargo.toml": crate_manifest(
                "bravebot-lsp", "reqwest"
            ).replace("[dependencies]", "[dev-dependencies]"),
        }
    )
    check(
        "a client a crate takes only for its tests is not a second egress",
        found == [],
        str(kinds(found)),
    )

    found = audit_egress(
        {
            **RECORDED_WORKSPACE,
            "crates/lsp/Cargo.toml": '[package]\nname = "bravebot-lsp"\n\n'
            "[target.'cfg(windows)'.dependencies]\nureq = { workspace = true }\n",
        }
    )
    check(
        "a client declared for one platform only is still a second egress",
        kinds(found) == ["unrecorded-network-client"],
        str(kinds(found)),
    )

    found = audit_egress(
        {
            **RECORDED_WORKSPACE,
            "crates/lsp/Cargo.toml": '[package]\nname = "bravebot-lsp"\n\n[dependencies]\n'
            'fetcher = { package = "reqwest", workspace = true }\n',
        }
    )
    check(
        "a client renamed in the manifest is still found",
        kinds(found) == ["unrecorded-network-client"],
        str(kinds(found)),
    )

    found = audit_egress(
        {
            **RECORDED_WORKSPACE,
            "crates/lsp/Cargo.toml": '[package]\nname = "bravebot-lsp"\n\n[dependencies.reqwest]\n'
            'version = "0.12"\n',
        }
    )
    check(
        "a client given a table of its own is still found",
        kinds(found) == ["unrecorded-network-client"],
        str(kinds(found)),
    )

    found = audit_egress(
        {
            **RECORDED_WORKSPACE,
            "crates/lsp/Cargo.toml": '[package]\nname = "bravebot-lsp"\n\n[dependencies.fetcher]\n'
            'package = "reqwest"\nversion = "0.12"\n',
        }
    )
    check(
        "a client renamed in a table of its own is still found",
        kinds(found) == ["unrecorded-network-client"],
        str(kinds(found)),
    )

    found = audit_egress(
        {
            **RECORDED_WORKSPACE,
            "crates/lsp/Cargo.toml": '[package]\nname = "bravebot-lsp"\n\n[dependencies]\n'
            "reqwest.workspace = true\n",
        }
    )
    check(
        "a client declared with a dotted key is still found",
        kinds(found) == ["unrecorded-network-client"],
        str(kinds(found)),
    )

    found = audit_egress(
        {
            **RECORDED_WORKSPACE,
            "crates/lsp/Cargo.toml": '[package]  # the language server client\n'
            'name = "bravebot-lsp"\n\n[dependencies]  # nothing here opens a socket\n'
            "reqwest = { workspace = true }\n\n[dev-dependencies]  # test only\n"
            "serde_json = { workspace = true }\n",
        }
    )
    check(
        "a comment after a table header hides neither the package name nor the table",
        kinds(found) == ["unrecorded-network-client"] and "bravebot-lsp" in found[0]["summary"],
        str([one["summary"] for one in found]),
    )

    found = audit_egress(
        {
            **RECORDED_WORKSPACE,
            "crates/lsp/Cargo.toml": '[package]\nname = "bravebot-lsp"\n\n[dependencies]\n'
            '# fetcher = { package = "reqwest" } was dropped when this went through the gate\n'
            "serde_json = { workspace = true }\n",
        }
    )
    check(
        "a dependency left commented out in a manifest is not one",
        found == [],
        str(kinds(found)),
    )

    found = audit_egress(
        {
            **RECORDED_WORKSPACE,
            "crates/skus/Cargo.toml": crate_manifest("bravebot-skus", "serde_json"),
        }
    )
    check(
        "an admission no manifest uses is a warning, not an error",
        kinds(found) == ["network-client-record-is-stale"]
        and found[0]["severity"] == audit.WARNING,
        str(kinds(found)),
    )

    found = audit_egress({**RECORDED_WORKSPACE, "crates/net/src/lib.rs": CLAIM})
    check(
        "a claim that the crate's client is its alone is an error",
        kinds(found) == ["exclusive-dependency-claim-is-false"]
        and found[0]["severity"] == audit.ERROR
        and "bravebot-skus" in found[0]["summary"],
        str([one["summary"] for one in found]),
    )

    check(
        "the claim is reported at the line it is on, not at the head of the comment",
        "crates/net/src/lib.rs:4" in found[0]["summary"]
        and "no other crate depends on `ureq`" in found[0]["evidence"][0],
        str(found[0]["evidence"]),
    )

    found = audit_egress({**RECORDED_WORKSPACE, "crates/net/src/lib.rs": CLAIM_ACROSS_TWO_LINES})
    check(
        "the same claim, broken across two lines by the margin, is the same error",
        kinds(found) == ["exclusive-dependency-claim-is-false"],
        str([one["summary"] for one in found]),
    )

    found = audit_egress({**RECORDED_WORKSPACE, "crates/net/src/lib.rs": CLAIM_CORRECTED})
    check(
        "the corrected paragraph is clean",
        found == [],
        str([one["summary"] for one in found]),
    )

    alone = {
        "docs/specs/network-egress.md": EGRESS_SPEC_ADMITTING_NOBODY,
        "crates/net/Cargo.toml": crate_manifest("bravebot-net", "ureq"),
        "crates/net/src/lib.rs": CLAIM_ACROSS_TWO_LINES,
    }
    found = audit_egress(alone)
    check(
        "the same claim, where no other manifest declares the crate, is not a finding",
        found == [],
        str([one["summary"] for one in found]),
    )

    check(
        "this repository's manifests and the register agree",
        with_cwd(
            ROOT,
            lambda: list(
                audit.check_network_clients_are_recorded(audit.mechanics.load_sources())
            ),
        )
        == [],
        str(
            with_cwd(
                ROOT,
                lambda: [
                    one["summary"]
                    for one in audit.check_network_clients_are_recorded(
                        audit.mechanics.load_sources()
                    )
                ],
            )
        ),
    )


# CHECK-12's table and the sentence in the Known cost that counts what it leaves out of reach. Two
# of the three prompts are answered here, as in the tree, so a register naming more than one out of
# reach is the drift being caught.
CLAUSE_TWO_OF_THREE = """<a id="CHECK-12"></a>
### CHECK-12: with it on, a safe verdict promotes one slot

| The prompt | What a yes does | With the mode on |
|---|---|---|
| vet-content | promotes one slot's bytes once | a safe verdict answers |
| read-output | promotes one slot's bytes once | a safe verdict answers |
| the vouch offer | writes a rule about the path | still asks |

<a id="CHECK-13"></a>
"""
COST_ONE_OUT_OF_REACH = "or answering the one other prompt a check runs for, the vouch offer.\n"
COST_TWO_OUT_OF_REACH = "or answering either of the other two prompts a check runs for.\n"


def test_prompt_split():
    undercounting = in_tree(
        {
            "docs/specs/vetting.md": CLAUSE_TWO_OF_THREE,
            "docs/specs/labels.md": COST_TWO_OUT_OF_REACH,
        }
    )
    found = with_cwd(undercounting, lambda: list(audit.check_prompt_split()))
    check(
        "a register putting more prompts out of reach than the clause does is an error",
        kinds(found) == ["prompt-split-disagreement"] and found[0]["severity"] == audit.ERROR,
        str(kinds(found)),
    )

    agreeing = in_tree(
        {
            "docs/specs/vetting.md": CLAUSE_TWO_OF_THREE,
            "docs/specs/labels.md": COST_ONE_OUT_OF_REACH,
        }
    )
    found = with_cwd(agreeing, lambda: list(audit.check_prompt_split()))
    check("a register stating the clause's split is clean", found == [], str(kinds(found)))

    # The same sentence the clean tree above passes on, against a clause that answers one prompt
    # rather than two, so two are out of reach and the register names one. The count comes from the
    # table or it comes from nowhere, and a check reading a constant instead would call this clean.
    narrowed = in_tree(
        {
            "docs/specs/vetting.md": CLAUSE_TWO_OF_THREE.replace(
                "| read-output | promotes one slot's bytes once | a safe verdict answers |",
                "| read-output | promotes one slot's bytes once | still asks |",
            ),
            "docs/specs/labels.md": COST_ONE_OUT_OF_REACH,
        }
    )
    found = with_cwd(narrowed, lambda: list(audit.check_prompt_split()))
    check(
        "the split is read from the clause's table rather than assumed",
        kinds(found) == ["prompt-split-disagreement"],
        str(kinds(found)),
    )

    # A table saying which prompts the mode reaches in some other words is a table this cannot
    # read, and reporting that as a disagreement sends the reader to edit the register, which is
    # the document still telling the truth.
    reworded = in_tree(
        {
            "docs/specs/vetting.md": CLAUSE_TWO_OF_THREE.replace(
                "a safe verdict answers", "the check answers in the person's place"
            ),
            "docs/specs/labels.md": COST_ONE_OUT_OF_REACH,
        }
    )
    found = with_cwd(reworded, lambda: list(audit.check_prompt_split()))
    check(
        "a table this cannot read is a warning against the clause, not a disagreement",
        kinds(found) == ["prompt-split-unstated"] and found[0]["severity"] == audit.WARNING,
        str(kinds(found)),
    )

    uncounted = in_tree(
        {
            "docs/specs/vetting.md": CLAUSE_TWO_OF_THREE,
            "docs/specs/labels.md": "The cost is that the bytes reach the planner.\n",
        }
    )
    found = with_cwd(uncounted, lambda: list(audit.check_prompt_split()))
    check(
        "a register counting nothing is a warning, not an error",
        kinds(found) == ["prompt-split-unstated"] and found[0]["severity"] == audit.WARNING,
        str(kinds(found)),
    )


# A field claiming to name every function that reads it, and the two functions in the file that do.
# The declaration is what the claim is attached to, so the fixture carries the doc as written.
def reader_doc(claim, between="", visibility="pub"):
    return {
        Path("crates/agent/src/tools.rs"): [
            "pub struct Tools<'a> {",
            f"    /// {claim}",
            *([f"    {between}"] if between else []),
            f"    {visibility} auto_vetting: bool,",
            "}",
            "",
            "fn vet_content(tools: &mut Tools<'_>) -> bool {",
            "    tools.auto_vetting",
            "}",
            "",
            "fn read_output(tools: &mut Tools<'_>) -> bool {",
            "    tools.auto_vetting",
            "}",
        ]
    }


def test_exhaustive_reader_docs():
    stale = reader_doc("Read by `vet_content` and by nothing else.")
    found = list(audit.check_exhaustive_reader_docs(stale))
    check(
        "a doc naming one reader of a field two functions read is an error",
        kinds(found) == ["reader-doc-incomplete"] and found[0]["severity"] == audit.ERROR,
        str(kinds(found)),
    )
    check(
        "the finding names the reader the doc left out",
        found and "`read_output`" in found[0]["summary"] and "`vet_content`" in found[0]["summary"],
        found[0]["summary"] if found else "",
    )

    current = reader_doc("Read by `vet_content` and by `read_output` and by nothing else.")
    found = list(audit.check_exhaustive_reader_docs(current))
    check("a doc naming both readers is clean", found == [], str(kinds(found)))

    # No claim, no check: a field doc that says nothing about who reads it is not asserting this.
    quiet = reader_doc("Whether a check that finds nothing may promote a slot.")
    found = list(audit.check_exhaustive_reader_docs(quiet))
    check(
        "a doc claiming nothing about its readers is not held to this",
        found == [],
        str(kinds(found)),
    )

    # A name backticked outside the claiming sentence is prose, not a reader the doc accounted for.
    elsewhere = reader_doc(
        "Read by `vet_content` and by nothing else. Not to be confused with `read_output`."
    )
    found = list(audit.check_exhaustive_reader_docs(elsewhere))
    check(
        "a reader named outside the claim does not count as named",
        kinds(found) == ["reader-doc-incomplete"],
        str(kinds(found)),
    )

    # An attribute sits between a field's doc and the field, and is not the end of the doc. A walk
    # back that stops at one reads no claim, and a claim nothing reads is a claim nothing holds.
    attributed = reader_doc(
        "Read by `vet_content` and by nothing else.", between="#[serde(default)]"
    )
    found = list(audit.check_exhaustive_reader_docs(attributed))
    check(
        "an attribute between the doc and the field does not hide the claim",
        kinds(found) == ["reader-doc-incomplete"],
        str(kinds(found)),
    )

    # Narrowing a field's visibility narrows who can read it, not what its doc claims.
    narrowed = reader_doc(
        "Read by `vet_content` and by nothing else.", visibility="pub(crate)"
    )
    found = list(audit.check_exhaustive_reader_docs(narrowed))
    check(
        "a pub(crate) field is held to its claim as a pub one is",
        kinds(found) == ["reader-doc-incomplete"],
        str(kinds(found)),
    )


def test_labelled_impls():
    reaching = {
        Path("crates/core/src/value.rs"): [
            "impl std::ops::Deref for Labelled<String> {",
            "    type Target = String;",
            "}",
        ]
    }
    found = list(audit.check_labelled_impls(reaching))
    check(
        "an implementation that reaches a label's content is an error",
        kinds(found) == ["labelled-impl"] and found[0]["impact"] == "high",
        str(kinds(found)),
    )

    allowed = {
        Path("crates/core/src/value.rs"): ["impl fmt::Debug for Labelled<String> {", "}"]
    }
    check("the one recorded implementation is clean", list(audit.check_labelled_impls(allowed)) == [])

    described = {
        Path("crates/core/src/value.rs"): [
            "// impl Deref for Labelled would let a caller read the content, so there is none.",
            "struct Labelled<T> { inner: T }",
        ]
    }
    check(
        "an implementation named in a comment is not a use",
        list(audit.check_labelled_impls(described)) == [],
    )

    # `impl Labelled` is the type's own block, not a trait reaching into it.
    own = {Path("crates/core/src/value.rs"): ["impl<T> Labelled<T> {", "}"]}
    check("the type's own impl block is not a trait", list(audit.check_labelled_impls(own)) == [])


PINNED_STEP = "      - uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v7.0.1\n"


def test_pinned_actions():
    moving = in_tree({".github/workflows/ci.yml": "      - uses: actions/checkout@v7\n"})
    found = with_cwd(moving, lambda: list(audit.check_pinned_actions()))
    check(
        "a step on a tag is an error",
        kinds(found) == ["unpinned-action"] and found[0]["impact"] == "high",
        str(kinds(found)),
    )

    fixed = in_tree({".github/workflows/ci.yml": PINNED_STEP})
    check(
        "a step on a commit is clean",
        with_cwd(fixed, lambda: list(audit.check_pinned_actions())) == [],
    )

    local = in_tree({".github/workflows/ci.yml": "      - uses: ./.github/actions/setup\n"})
    check(
        "an action from this repository is not a third party",
        with_cwd(local, lambda: list(audit.check_pinned_actions())) == [],
    )

    # Every step in the tree is pinned today, and this is what keeps it that way.
    check(
        "the tree's own workflows are pinned",
        with_cwd(ROOT, lambda: list(audit.check_pinned_actions())) == [],
    )


# A dispatch that names the ref it publishes, in the two shapes a version tag can be written in: the
# bare name, which a branch of that name answers to as readily as the tag does, and the tag itself.
DISPATCH_BY_NAME = """\
on:
  workflow_dispatch:
    inputs:
      tag:
        required: true

jobs:
  publish:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v7.0.1
        with:
          ref: ${{ inputs.tag }}
"""

DISPATCH_BY_TAG = DISPATCH_BY_NAME.replace("ref: ${{", "ref: refs/tags/${{")


def test_checkout_ref_is_qualified():
    by_name = in_tree({".github/workflows/publish-npm.yml": DISPATCH_BY_NAME})
    found = with_cwd(by_name, lambda: list(audit.check_checkout_ref_is_qualified()))
    check(
        "a checkout of a bare name is an error",
        kinds(found) == ["unqualified-checkout-ref"] and found[0]["impact"] == "high",
        str(kinds(found)),
    )
    check(
        "it says which line and which ref",
        bool(found) and found[0]["evidence"] == [".github/workflows/publish-npm.yml:13 "
                                                 "ref: ${{ inputs.tag }}"],
        str(found[0]["evidence"]) if found else "(nothing)",
    )

    by_tag = in_tree({".github/workflows/publish-npm.yml": DISPATCH_BY_TAG})
    check(
        "a checkout of refs/tags/ is clean",
        with_cwd(by_tag, lambda: list(audit.check_checkout_ref_is_qualified())) == [],
    )

    # The triggering commit, which is a commit rather than a name, so there is nothing to resolve.
    plain = in_tree(
        {".github/workflows/ci.yml": "    steps:\n      - uses: actions/checkout@" + "a" * 40 + "\n"}
    )
    check(
        "a checkout with no ref is clean",
        with_cwd(plain, lambda: list(audit.check_checkout_ref_is_qualified())) == [],
    )

    sha = in_tree(
        {".github/workflows/ci.yml": DISPATCH_BY_NAME.replace("inputs.tag", "github.sha")}
    )
    check(
        "an expression naming a commit is clean",
        with_cwd(sha, lambda: list(audit.check_checkout_ref_is_qualified())) == [],
    )

    # A step's keys are a mapping, so `with:` above `uses:` is the same step. Read downwards from
    # the `uses:` line this would be a bare name nothing reported.
    reordered = in_tree(
        {
            ".github/workflows/publish-npm.yml": """\
jobs:
  publish:
    steps:
      - with:
          ref: ${{ inputs.tag }}
        uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v7.0.1
"""
        }
    )
    check(
        "the ref is found above its own uses:",
        kinds(with_cwd(reordered, lambda: list(audit.check_checkout_ref_is_qualified())))
        == ["unqualified-checkout-ref"],
    )

    # `ref:` is an input of other actions too, and what they do with one is their own business.
    # This check is about the tree a step puts on the runner.
    elsewhere = in_tree(
        {
            ".github/workflows/gh-pages.yml": """\
jobs:
  deploy:
    steps:
      - uses: actions/deploy-pages@3d3c42e5aac5ba805825da76410c181273ba90b1 # v4
        with:
          ref: ${{ inputs.tag }}
"""
        }
    )
    check(
        "a ref handed to something other than a checkout is not read",
        with_cwd(elsewhere, lambda: list(audit.check_checkout_ref_is_qualified())) == [],
    )

    # Both checkouts in the publish workflow name the tag, and this is what keeps them there.
    check(
        "the tree's own checkouts name a kind of ref",
        with_cwd(ROOT, lambda: list(audit.check_checkout_ref_is_qualified())) == [],
    )


# The publish workflow in the two shapes that matter. In the first the grant is declared once for a
# whole workflow whose single job also installs the lockfile and runs what it installed; in the second
# those two commands are a job of their own and the grant is on the job that publishes. The second is
# what this repository ships, so a check that reported it would be one nobody could keep.
GRANT_OVER_THE_INSTALL = """\
permissions:
  contents: read
  id-token: write

jobs:
  publish:
    runs-on: ubuntu-latest
    steps:
      - name: Install from the lockfile
        run: npm ci --ignore-scripts

      - name: Lint the lockfile
        run: npm run lint:lockfile

      - name: Publish
        run: npm publish --access public --provenance --ignore-scripts
"""

GRANT_BESIDE_THE_INSTALL = """\
permissions:
  contents: read

jobs:
  lint:
    runs-on: ubuntu-latest
    steps:
      - name: Install from the lockfile
        run: npm ci --ignore-scripts

      - name: Lint the lockfile
        run: npm run lint:lockfile

  publish:
    needs: lint
    runs-on: ubuntu-latest
    permissions:
      contents: read
      id-token: write
    steps:
      - name: Publish
        run: |
          # No npm ci here: the lint job installed and linted the lockfile already.
          npm publish --access public --provenance --ignore-scripts
"""


def test_privileged_job_runs_only_its_own_code():
    over = in_tree({".github/workflows/publish-npm.yml": GRANT_OVER_THE_INSTALL})
    found = with_cwd(over, lambda: list(audit.check_privileged_job_runs_only_its_own_code()))
    evidence = " | ".join(found[0]["evidence"]) if found else "(nothing)"
    check(
        "a grant declared for a whole workflow reaches the job that installs and runs the lockfile",
        kinds(found) == ["privileged-job-runs-dependencies"]
        and "installs a dependency: npm ci" in evidence
        and "runs a dependency: npm run lint:lockfile" in evidence,
        f"{kinds(found)}: {evidence}",
    )
    check(
        "the publish step is not what is reported: it runs nothing that was installed",
        bool(found) and "npm publish" not in evidence,
        evidence,
    )

    beside = in_tree({".github/workflows/publish-npm.yml": GRANT_BESIDE_THE_INSTALL})
    found = with_cwd(beside, lambda: list(audit.check_privileged_job_runs_only_its_own_code()))
    check(
        "the same two commands in a job holding only contents: read are clean",
        found == [],
        str(kinds(found)),
    )

    # A job's own `permissions` replaces the workflow's rather than adding to it, which is how GitHub
    # reads it, so the narrowed job below holds no grant however the workflow above it is written.
    narrowed = in_tree(
        {
            ".github/workflows/publish-npm.yml": "permissions:\n"
            "  contents: read\n"
            "  id-token: write\n"
            "\n"
            "jobs:\n"
            "  lint:\n"
            "    runs-on: ubuntu-latest\n"
            "    permissions:\n"
            "      contents: read\n"
            "    steps:\n"
            "      - run: npm ci --ignore-scripts\n"
        }
    )
    found = with_cwd(narrowed, lambda: list(audit.check_privileged_job_runs_only_its_own_code()))
    check(
        "a job that narrows the workflow's permissions holds what it declares and nothing more",
        found == [],
        str(kinds(found)),
    )

    on_the_job = in_tree(
        {
            ".github/workflows/publish-npm.yml": "jobs:\n"
            "  publish:\n"
            "    runs-on: ubuntu-latest\n"
            "    permissions:\n"
            "      id-token: write\n"
            "    steps:\n"
            "      - run: npx tsc --outDir dist\n"
        }
    )
    found = with_cwd(on_the_job, lambda: list(audit.check_privileged_job_runs_only_its_own_code()))
    check(
        "a grant declared on the job is read as well as one declared above it",
        kinds(found) == ["privileged-job-runs-dependencies"]
        and "runs a dependency: npx tsc" in " | ".join(found[0]["evidence"]),
        str(kinds(found)),
    )

    # A secret is the other thing a step can read out of the job it is in, and what a job holding one
    # can lose is the secret rather than a publishing credential.
    secret = in_tree(
        {
            ".github/workflows/publish-npm.yml": "jobs:\n"
            "  measure:\n"
            "    runs-on: ubuntu-latest\n"
            "    steps:\n"
            "      - name: Measure\n"
            "        env:\n"
            "          GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}\n"
            "        run: npm ci --ignore-scripts\n"
        }
    )
    found = with_cwd(secret, lambda: list(audit.check_privileged_job_runs_only_its_own_code()))
    check(
        "a job holding a secret rather than a grant is reported, and says which it holds",
        kinds(found) == ["privileged-job-runs-dependencies"]
        and "holds a secret" in found[0]["summary"],
        str(kinds(found)),
    )

    unreadable = in_tree(
        {
            ".github/workflows/publish-npm.yml": "jobs:\n"
            "    publish:\n"
            "        runs-on: ubuntu-latest\n"
            "        permissions:\n"
            "            id-token: write\n"
            "        steps:\n"
            "          - run: npm ci --ignore-scripts\n"
        }
    )
    found = with_cwd(unreadable, lambda: list(audit.check_privileged_job_runs_only_its_own_code()))
    check(
        "a workflow whose jobs this cannot read is an error rather than a silent pass",
        kinds(found) == ["workflow-unreadable"],
        str(kinds(found)),
    )

    # Every spelling the grant has in a file GitHub accepts. A check that reads one of them is a check
    # somebody undoes by quoting a value or collapsing a map, neither of which changes what is granted.
    for name, declaration in (
        ("quoted", '    permissions:\n      id-token: "write"\n'),
        ("an inline map", "    permissions: { contents: read, id-token: write }\n"),
        ("write-all", "    permissions: write-all\n"),
    ):
        spelt = in_tree(
            {
                ".github/workflows/publish-npm.yml": "jobs:\n"
                "  publish:\n"
                "    runs-on: ubuntu-latest\n" + declaration + "    steps:\n"
                "      - run: npm ci --ignore-scripts\n"
            }
        )
        found = with_cwd(spelt, lambda: list(audit.check_privileged_job_runs_only_its_own_code()))
        check(
            f"a grant written as {name} grants the same thing and is read the same way",
            kinds(found) == ["privileged-job-runs-dependencies"],
            str(kinds(found)),
        )

    # A comment is prose. Naming a secret in one does not put it in any job's environment, and this
    # tree's workflows have comments that discuss tokens.
    discussed = in_tree(
        {
            ".github/workflows/ci.yml": "# The token here is ${{ secrets.GITHUB_TOKEN }}, which this\n"
            "# workflow deliberately does not use.\n"
            "jobs:\n"
            "  npm-lockfile:\n"
            "    runs-on: ubuntu-latest\n"
            "    steps:\n"
            "      - run: npm ci --ignore-scripts\n"
        }
    )
    found = with_cwd(discussed, lambda: list(audit.check_privileged_job_runs_only_its_own_code()))
    check(
        "a secret named in a comment is not a secret the jobs below it hold",
        found == [],
        str(kinds(found)),
    )

    # The step's own keys are not its script. `- run: x` puts the dash left of the key, so reading the
    # body from the dash column would take the `env:` beside it and report whatever the value says.
    beside_the_run = in_tree(
        {
            ".github/workflows/publish-npm.yml": "jobs:\n"
            "  publish:\n"
            "    runs-on: ubuntu-latest\n"
            "    permissions:\n"
            "      id-token: write\n"
            "    steps:\n"
            "      - run: ./contrib/report.sh\n"
            "        env:\n"
            '          NOTE: "do not npm ci here"\n'
        }
    )
    found = with_cwd(beside_the_run, lambda: list(audit.check_privileged_job_runs_only_its_own_code()))
    check(
        "a step's env is not a command, however the value reads",
        found == [],
        str(kinds(found)),
    )

    # The other spellings of running what was installed. `npm run` is one of several, and a lockfile
    # is installed by more than one program.
    for name, command in (
        ("npm exec", "npm exec -- lockfile-lint"),
        ("yarn install", "yarn install --frozen-lockfile"),
    ):
        other = in_tree(
            {
                ".github/workflows/publish-npm.yml": "jobs:\n"
                "  publish:\n"
                "    runs-on: ubuntu-latest\n"
                "    permissions:\n"
                "      id-token: write\n"
                "    steps:\n"
                f"      - run: {command}\n"
            }
        )
        found = with_cwd(other, lambda: list(audit.check_privileged_job_runs_only_its_own_code()))
        check(
            f"{name} runs code nobody here wrote as surely as npm run does",
            kinds(found) == ["privileged-job-runs-dependencies"],
            str(kinds(found)),
        )

    # The publish workflow holds the one grant in this tree, and this is what keeps the install out of
    # the job that holds it.
    check(
        "no job in the tree's own workflows runs a dependency beside a credential",
        with_cwd(ROOT, lambda: list(audit.check_privileged_job_runs_only_its_own_code())) == [],
    )


# A real reference and the digest it resolved to, because a fixture written with a made-up hash
# would pass a check that only counted hex digits and say nothing about the one that runs.
ZIGBUILD = "ghcr.io/rust-cross/cargo-zigbuild:0.23.0"
DIGEST = "@sha256:b8364c2c60cdcc9b95c402d17654bff517410926a35678bd89dd924b8158d6ae"
RUN_LINE = (
    'check:\n\tdocker run --rm --platform linux/amd64 -e A=1 \\\n'
    '\t\t-v "$(PWD):/src:ro" -w /work {image} sh -c "cargo build"\n'
)


def test_pinned_images():
    moving = in_tree({"Dockerfile.cross": f"FROM {ZIGBUILD} AS builder\nCOPY . .\n"})
    found = with_cwd(moving, lambda: list(audit.check_pinned_images()))
    check(
        "a Dockerfile base image on a tag is an error",
        kinds(found) == ["unpinned-image"] and found[0]["impact"] == "high",
        str(kinds(found)),
    )

    fixed = in_tree({"Dockerfile.cross": f"FROM {ZIGBUILD}{DIGEST} AS builder\nCOPY . .\n"})
    check(
        "a base image that names a digest is clean",
        with_cwd(fixed, lambda: list(audit.check_pinned_images())) == [],
    )

    # The last stage of Dockerfile.cross is `FROM scratch`, and the one before it is copied out of
    # a stage this file named: neither is pulled from anywhere.
    staged = in_tree(
        {
            "Dockerfile.cross": f"FROM {ZIGBUILD}{DIGEST} AS builder\n"
            "FROM scratch\nCOPY --from=builder /out/bravebot /bravebot\n"
        }
    )
    check(
        "the empty image and a stage of this build are not pulled from anywhere",
        with_cwd(staged, lambda: list(audit.check_pinned_images())) == [],
    )

    # The image of a `docker run` is not the token after it: `--platform linux/amd64` and
    # `-v "$(PWD):/src:ro"` both come first, and both would read as an image reference.
    recipe = in_tree({"Makefile": RUN_LINE.format(image="rust:slim")})
    found = with_cwd(recipe, lambda: list(audit.check_pinned_images()))
    check(
        "the image a recipe runs is read past the options and their values",
        kinds(found) == ["unpinned-image"] and "`rust:slim`" in found[0]["summary"],
        str(kinds(found)) + " " + (found[0]["summary"] if found else ""),
    )

    pinned = in_tree({"Makefile": RUN_LINE.format(image=f"rust:slim{DIGEST}")})
    check(
        "a recipe that names a digest is clean",
        with_cwd(pinned, lambda: list(audit.check_pinned_images())) == [],
    )

    # `make strip` runs a second image inside a shell loop, where the option before it holds a
    # space: a split on whitespace ends up reading the second half of that value as the image.
    quoted = in_tree(
        {
            "Makefile": 'strip:\n\t@for f in dist/*; do \\\n'
            '\t\tdocker run --rm -e ASSET="/dist/$$(basename $$f)" \\\n'
            f'\t\t\t{ZIGBUILD} sh -c "true"; \\\n\tdone\n'
        }
    )
    found = with_cwd(quoted, lambda: list(audit.check_pinned_images()))
    check(
        "an option whose value holds a space does not hide the image after it",
        kinds(found) == ["unpinned-image"] and f"`{ZIGBUILD}`" in found[0]["summary"],
        str(kinds(found)) + " " + (found[0]["summary"] if found else ""),
    )

    # `docker create` in the extract step runs the image the build produced a line earlier. There
    # is no registry above it and nothing to pin.
    built = in_tree(
        {
            "Makefile": "extract:\n\tdocker build -f Dockerfile.cross -t $(BINARY)-$(1) .\n"
            "\tdocker create --name tmp-$(BINARY)-$(2) $(1) /dev/null\n"
        }
    )
    check(
        "an image this build produced is not pulled from anywhere",
        with_cwd(built, lambda: list(audit.check_pinned_images())) == [],
    )

    # A name the Makefile assigns is still a reference: it is read through the assignment rather
    # than passed over for having a `$` in it.
    through = in_tree(
        {"Makefile": "IMAGE = rust:slim\n" + RUN_LINE.format(image="$(IMAGE)")}
    )
    check(
        "an image written through a variable is read through it",
        kinds(with_cwd(through, lambda: list(audit.check_pinned_images()))) == ["unpinned-image"],
    )

    # The failure this check must not have is going quiet: a command it cannot read is a finding,
    # because a pass that reports nothing reads as a tree with nothing in it.
    unreadable = in_tree({"Makefile": "check:\n\tdocker run --rm --platform linux/amd64\n"})
    found = with_cwd(unreadable, lambda: list(audit.check_pinned_images()))
    check(
        "a docker command whose image cannot be read is reported rather than passed over",
        kinds(found) == ["unpinned-image"],
    )

    # A quote the lexer cannot close swallows the rest of the line, and what it swallowed here is
    # the whole of the command.
    swallowed = in_tree(
        {
            "Makefile": "strip:\n\t@echo don't ship this; \\\n"
            '\t\tdocker run --rm rust:slim sh -c "true"\n'
        }
    )
    check(
        "a command behind a quote the lexer cannot close is reported too",
        kinds(with_cwd(swallowed, lambda: list(audit.check_pinned_images()))) == ["unpinned-image"],
    )

    # A base written through an `ARG` is still a base. Parameterising it is how the one image that
    # compiles every shipped binary stops being written down anywhere.
    through_arg = in_tree(
        {"Dockerfile.cross": f"ARG BASE={ZIGBUILD}\nFROM $BASE AS builder\n"}
    )
    check(
        "a base image written through an ARG is read through it",
        kinds(with_cwd(through_arg, lambda: list(audit.check_pinned_images())))
        == ["unpinned-image"],
    )

    # `COPY --from` pulls whatever it names, the same as a `FROM` does, unless what it names is a
    # stage of this build.
    reached = in_tree(
        {
            "Dockerfile.cross": f"FROM {ZIGBUILD}{DIGEST} AS builder\n"
            "FROM scratch\nCOPY --from=builder /out/bravebot /bravebot\n"
            "COPY --from=alpine:3 /etc/ssl /etc/ssl\n"
        }
    )
    found = with_cwd(reached, lambda: list(audit.check_pinned_images()))
    check(
        "a COPY that reaches into an image is that image, and one into a stage is not",
        kinds(found) == ["unpinned-image"] and "`alpine:3`" in found[0]["summary"],
        str(kinds(found)),
    )

    # A command inside a quoted script is a command: the line's own tokens hold that script as one
    # token, so a pass that reads them alone never looks inside it.
    nested = in_tree(
        {"Makefile": "check:\n\tsh -c 'docker run --rm rust:slim true'\n"}
    )
    check(
        "a command inside a quoted script is read too",
        kinds(with_cwd(nested, lambda: list(audit.check_pinned_images()))) == ["unpinned-image"],
    )

    # The Makefile explains its own `docker create` in a comment above it.
    described = in_tree(
        {
            "Makefile": "# `docker create` on a scratch image needs a command argument.\n"
            "check:\n\ttrue\n"
        }
    )
    check(
        "a comment naming a command is prose about it, not a command",
        with_cwd(described, lambda: list(audit.check_pinned_images())) == [],
    )

    # `check_pinned_actions` passes over `docker://` because an image is not a `uses:` step, which
    # left the two forms a workflow can name one in read by nothing at all.
    job = in_tree(
        {
            ".github/workflows/ci.yml": "    container:\n      image: rust:slim\n"
            "    steps:\n      - uses: docker://alpine:3\n"
        }
    )
    found = with_cwd(job, lambda: list(audit.check_pinned_images()))
    check(
        "a job container and a `docker://` step are both images",
        kinds(found) == ["unpinned-image"] * 2,
        str(kinds(found)),
    )

    # A nested worktree is a second checkout of this repository, often parked several commits back.
    # Its Makefile is not this tree's to pin: a finding against it names a file no commit here can
    # fix, and it turns this check into one that passes or fails on whether the person running it
    # happens to keep a worktree.
    nested = in_tree({".claude/worktrees/old/Makefile": RUN_LINE.format(image="rust:slim")})
    check(
        "an image named by a nested worktree is not this tree's to answer for",
        with_cwd(nested, lambda: list(audit.check_pinned_images())) == [],
        str(kinds(with_cwd(nested, lambda: list(audit.check_pinned_images())))),
    )

    nested_checkout = in_tree({
        "other-checkout/.git": "gitdir: /elsewhere/worktrees/other\n",
        "other-checkout/Dockerfile": "FROM rust:slim\n",
        "tools/Dockerfile": "FROM alpine:3\n",
    })
    found = with_cwd(nested_checkout, lambda: list(audit.check_pinned_images()))
    check(
        "nested worktrees are excluded but source subdirectories are checked",
        len(found) == 1 and "tools/Dockerfile:1" in found[0]["evidence"],
        str(found),
    )

    # Every image in the tree names a digest today, and this is what keeps it that way.
    check(
        "the tree's own build images are pinned",
        with_cwd(ROOT, lambda: list(audit.check_pinned_images())) == [],
    )


def test_construction_pinned():
    sources = {
        Path("crates/agent/src/tools.rs"): [
            "fn answer(bytes: Vec<u8>) -> Labelled<String> {",
            "    Labelled::new(text, Label::trusted_public())",
            "}",
        ]
    }
    found = list(audit.check_construction_pinned([FakeSpec([])], sources))
    check(
        "a constructor no spec pins is an error",
        kinds(found) == ["construction-unpinned"] and found[0]["impact"] == "medium",
        str(kinds(found)),
    )

    check(
        "a constructor a spec pins is clean",
        list(audit.check_construction_pinned([FakeSpec(audit.CONSTRUCTORS)], sources)) == [],
    )

    core_only = {
        Path("crates/core/src/policy.rs"): ["    Labelled::new(text, label)"],
    }
    check(
        "a constructor used only inside the kernel is not the reported surface",
        list(audit.check_construction_pinned([FakeSpec([])], core_only)) == [],
    )

    # The count is the argument for a `sites:` pin, so a count that includes an unrelated `fn new` is
    # a check that fails on the next file to define one.
    unrelated = {
        Path("crates/agent/src/tools.rs"): [
            "impl Jobs {",
            "    pub fn new() -> Self { Self::default() }",
            "}",
            "fn label(text: String) -> Labelled<String> { Labelled::trusted(text) }",
        ]
    }
    found = list(audit.check_construction_pinned([FakeSpec(["Labelled::trusted"])], unrelated))
    check(
        "an unrelated `fn new` in a file that names Labelled is not a construction",
        found == [],
        str([one["summary"] for one in found]),
    )
    check(
        "the constructor that is there is still counted",
        sum(
            site["count"]
            for site in audit.sites_for("Labelled::trusted", unrelated)
        ) == 1,
    )


def test_gates_pinned():
    kernel = [
        "impl Policy {",
        "    pub fn render_in_place<T: Clone, R>(",
        "        &mut self,",
        "        content: &Labelled<T>,",
        "        shape: impl FnOnce(T) -> R,",
        "    ) -> Labelled<R> {",
        '        let proof = Declassification::authorise("reshaped");',
        "        Labelled::new(shape(content.clone().declassify(&proof)), content.label())",
        "    }",
        "    pub fn read_trusted_content<T: Clone>(&mut self, value: &Labelled<T>) -> Gated<T> {",
        '        self.refuse_untrusted("trusted-read", "tool", value.label())?;',
        '        let proof = Declassification::authorise("examined");',
        "        Ok(value.clone().declassify(&proof))",
        "    }",
        "}",
    ]
    sources = {
        Path("crates/core/src/policy.rs"): kernel,
        Path("crates/agent/src/tools.rs"): [
            "fn names(policy: &mut Policy, found: &Labelled<Listing>) -> Labelled<Vec<String>> {",
            '    policy.render_in_place("list_files", found, |found| found.names())',
            "}",
            "fn body(policy: &mut Policy, skill: &Labelled<String>) -> Gated<String> {",
            '    policy.read_trusted_content("load_skill", skill)',
            "}",
        ],
    }

    found = list(audit.check_gates_pinned([FakeSpec([])], sources))
    check(
        "a gate no spec counts is an error, whichever way it treats a label",
        kinds(found) == ["gate-unpinned"] * 2 and all(one["impact"] == "low" for one in found),
        str(kinds(found)),
    )

    # The difference the check exists to draw. A gate that refuses untrusted content can only hand
    # the driver bytes it was already allowed to read, so naming it catches the one thing left to
    # catch. A gate that releases whatever it is given needs the count, and a name buys nothing.
    named = FakeSpec([], named=["Policy::render_in_place", "Policy::read_trusted_content"])
    found = list(audit.check_gates_pinned([named], sources))
    check(
        "naming clears the gate that refuses untrusted content and not the gate that releases it",
        [one["title"].split(",")[0] for one in found]
        == ["no spec counts Policy::render_in_place"],
        str([one["title"] for one in found]),
    )

    check(
        "a counted gate is clean",
        list(audit.check_gates_pinned([FakeSpec(audit.GATES)], sources)) == [],
    )

    kernel_only = {Path("crates/core/src/policy.rs"): kernel}
    check(
        "a gate the driver never calls is not the reported surface",
        list(audit.check_gates_pinned([FakeSpec([])], kernel_only)) == [],
    )

    # The half a count on the gate alone leaves open: every caller of this wrapper writes a closure
    # that runs on released content, and none of them moves the gate's count.
    wrapped = dict(sources)
    wrapped[Path("crates/agent/src/tools.rs")] = [
        "fn note_for<T: Clone>(",
        "    policy: &mut Policy,",
        "    content: &Labelled<T>,",
        "    shape: impl FnOnce(T) -> String,",
        ") -> String {",
        '    let shaped = policy.render_in_place("read_file", content, shape);',
        "    shaped.declassify(&proof)",
        "}",
    ]
    found = list(audit.check_gates_pinned([FakeSpec(audit.GATES)], wrapped))
    check(
        "a function that forwards its caller's closure to a gate is an error of its own",
        kinds(found) == ["gate-forwarder-unpinned"] and "note_for" in found[0]["title"],
        str([one["title"] for one in found]),
    )
    in_tools = FakeSpec(
        list(audit.GATES) + ["note_for"],
        sites={"note_for": ["crates/agent/src/tools.rs: 2"]},
    )
    check(
        "counting the wrapper clears it",
        list(audit.check_gates_pinned([in_tools], wrapped)) == [],
    )

    # A bare `guards` name is satisfied by any symbol of that name anywhere in the tree, so an
    # entry that counts a different file is an entry about a different function.
    elsewhere = FakeSpec(
        list(audit.GATES) + ["note_for"],
        sites={"note_for": ["crates/agent/src/workspace.rs: 2"]},
    )
    check(
        "an entry counting another file does not clear this one",
        kinds(list(audit.check_gates_pinned([elsewhere], wrapped))) == ["gate-forwarder-unpinned"],
        str(kinds(list(audit.check_gates_pinned([elsewhere], wrapped)))),
    )

    # The rewrite that would answer a check reading only `impl Fn`: the same closure, taken as a
    # generic bound in the brackets and spelled by name where the parameter is declared.
    generic = dict(sources)
    generic[Path("crates/agent/src/tools.rs")] = [
        "fn note_for<T: Clone, F: FnOnce(T) -> String>(",
        "    policy: &mut Policy,",
        "    content: &Labelled<T>,",
        "    shape: F,",
        ") -> String {",
        '    let shaped = policy.render_in_place("read_file", content, shape);',
        "    shaped.declassify(&proof)",
        "}",
    ]
    found = list(audit.check_gates_pinned([FakeSpec(audit.GATES)], generic))
    check(
        "a closure taken as a generic bound is the same way in",
        kinds(found) == ["gate-forwarder-unpinned"] and "note_for" in found[0]["title"],
        str([one["title"] for one in found]),
    )

    # A call written as an associated function rather than on a receiver reaches the same gate.
    qualified = dict(sources)
    qualified[Path("crates/agent/src/tools.rs")] = [
        "fn note_for<T: Clone>(",
        "    policy: &mut Policy,",
        "    content: &Labelled<T>,",
        "    shape: impl FnOnce(T) -> String,",
        ") -> String {",
        '    Policy::render_in_place(policy, "read_file", content, shape)',
        "}",
    ]
    check(
        "a gate called by its qualified name is still forwarded to",
        kinds(list(audit.check_gates_pinned([FakeSpec(audit.GATES)], qualified)))
        == ["gate-forwarder-unpinned"],
        str(kinds(list(audit.check_gates_pinned([FakeSpec(audit.GATES)], qualified)))),
    )

    # A parenthesis inside a string literal closes nothing, so a scan that counted one would run
    # past the end of this call and come back with no arguments, which reads as a function that
    # forwards nothing. Every one of these calls names its tool with a string literal.
    literal = dict(sources)
    literal[Path("crates/agent/src/tools.rs")] = [
        "fn note_for<T: Clone>(",
        "    policy: &mut Policy,",
        "    content: &Labelled<T>,",
        "    shape: impl FnOnce(T) -> String,",
        ") -> String {",
        '    let shaped = policy.render_in_place("read (file", content, shape);',
        "    shaped.declassify(&proof)",
        "}",
    ]
    check(
        "a parenthesis inside a string literal does not hide the argument list",
        kinds(list(audit.check_gates_pinned([FakeSpec(audit.GATES)], literal)))
        == ["gate-forwarder-unpinned"],
        str([one["title"] for one in audit.check_gates_pinned([FakeSpec(audit.GATES)], literal)]),
    )

    # A refusal below the release is not one: the bytes are in the caller's hands by the time it
    # runs, so a gate cannot be exempted from being counted by a line that does nothing.
    late = {
        Path("crates/core/src/policy.rs"): [
            "impl Policy {",
            "    pub fn render_in_place<T: Clone, R>(",
            "        &mut self,",
            "        content: &Labelled<T>,",
            "        shape: impl FnOnce(T) -> R,",
            "    ) -> Labelled<R> {",
            "        let out = shape(content.clone().declassify(&proof));",
            '        self.refuse_untrusted("render", "tool", content.label())?;',
            "        Labelled::new(out, content.label())",
            "    }",
            "}",
        ],
        Path("crates/agent/src/tools.rs"): [
            "fn names(policy: &mut Policy, found: &Labelled<Listing>) -> Labelled<Vec<String>> {",
            '    policy.render_in_place("list_files", found, |found| found.names())',
            "}",
        ],
    }
    check(
        "a refusal after the release does not excuse the count",
        kinds(
            list(
                audit.check_gates_pinned(
                    [FakeSpec([], named=["Policy::render_in_place"])], late
                )
            )
        )
        == ["gate-unpinned"],
    )

    # A callback the gate never receives is not a way into it, and reporting one would make this
    # check fire on every function that happens to take a closure for a reason of its own.
    callback = dict(sources)
    callback[Path("crates/agent/src/tools.rs")] = [
        "fn watched<T: Clone>(",
        "    policy: &mut Policy,",
        "    content: &Labelled<T>,",
        "    watching: impl Fn(usize),",
        ") -> Labelled<String> {",
        "    watching(1);",
        '    policy.render_in_place("read_file", content, |text| text.to_string())',
        "}",
    ]
    check(
        "a closure the call never passes on is not a way into the gate",
        list(audit.check_gates_pinned([FakeSpec(audit.GATES)], callback)) == [],
        str([one["title"] for one in audit.check_gates_pinned([FakeSpec(audit.GATES)], callback)]),
    )

    # The counts in `labels.md` are what makes this pass, so a gate added to the tree without one
    # fails here rather than at whoever next reads `crates/agent/src/tools.rs`.
    check(
        "the tree's own gates and the wrapper that forwards to one are counted",
        with_cwd(
            ROOT,
            lambda: list(
                audit.check_gates_pinned(audit.load_specs(), audit.mechanics.load_sources())
            ),
        )
        == [],
    )


STEP_DECLARED = [
    "pub struct Step {",
    "    pub program: String,",
    "    pub resolved: PathBuf,",
    "    pub args: Vec<String>,",
    "    pub environment: Vec<(String, String)>,",
    "    pub routes: Vec<Route>,",
    "}",
]


def declaring(more):
    """Sources holding the `Step` declaration, plus whatever the fixture is about."""
    sources = {audit.STEP_STRUCT: list(STEP_DECLARED)}
    sources.update(more)
    return sources


def test_key_sites_exhaustive():
    """The check that makes a field-by-field key a build failure rather than a habit."""
    check(
        "the fields are read from the declaration, not listed in the check",
        audit.step_fields(declaring({})) == [
            "program",
            "resolved",
            "args",
            "environment",
            "routes",
        ],
        str(audit.step_fields(declaring({}))),
    )

    reading = declaring(
        {
            Path("crates/core/src/command.rs"): STEP_DECLARED
            + [
                "fn encode_step(out: &mut String, step: &Step) {",
                "    length_prefixed_path(out, &step.resolved);",
                "    for arg in &step.args {",
                "        length_prefixed(out, arg);",
                "    }",
                "}",
            ]
        }
    )
    found = list(audit.check_key_sites_exhaustive([FakeSpec([])], reading))
    check(
        "a function reading two Step fields without destructuring is an error",
        kinds(found) == ["key-site-not-exhaustive"] and found[0]["severity"] == audit.ERROR,
        str(kinds(found)),
    )
    check(
        "the finding names the function and the fields it leaves out",
        found
        and "encode_step" in found[0]["summary"]
        and any("environment" in one for one in found[0]["evidence"]),
        str(found[0]["evidence"]) if found else "no finding",
    )

    destructured = declaring(
        {
            Path("crates/core/src/command.rs"): STEP_DECLARED
            + [
                "fn encode_step(out: &mut String, step: &Step) {",
                "    let Step { program: _, resolved, args, environment, routes } = step;",
                "    length_prefixed_path(out, resolved);",
                "    for arg in args {",
                "        length_prefixed(out, arg);",
                "    }",
                "}",
            ]
        }
    )
    check(
        "the same function destructuring first is clean",
        list(audit.check_key_sites_exhaustive([FakeSpec([])], destructured)) == [],
    )

    # The conversion out of `RememberedStep` destructures that type rather than `Step`, and a check
    # that only recognised `let Step {` would leave a fixed site permanently red.
    wrapper = declaring(
        {
            Path("crates/agent/src/remembered.rs"): [
                "fn written(step: &RememberedStep) -> WrittenStep {",
                "    let RememberedStep { resolved, args, environment } = step;",
                "    WrittenStep { path: step.resolved.clone(), args: step.args.clone() }",
                "}",
            ]
        }
    )
    check(
        "destructuring a RememberedStep counts as destructuring",
        list(audit.check_key_sites_exhaustive([FakeSpec([])], wrapper)) == [],
    )

    # A function reading one field is reading a field, not building a key out of the step.
    single = declaring(
        {
            Path("crates/core/src/policy.rs"): [
                "fn describe(step: &Step) -> String {",
                "    step.program.clone()",
                "}",
            ]
        }
    )
    check(
        "a function reading one field is not a key",
        list(audit.check_key_sites_exhaustive([FakeSpec([])], single)) == [],
    )

    class Admitting:
        """A spec naming one site as reading a step for something that is not a key."""

        allowlists = {}
        front = {audit.NOT_A_KEY: ["crates/core/src/policy.rs::plan_lines"]}

    admitted = declaring(
        {
            Path("crates/core/src/policy.rs"): [
                "fn plan_lines(step: &Step) -> String {",
                "    format!(\"{} {}\", step.program, step.args.join(\" \"))",
                "}",
            ]
        }
    )
    check(
        "a site the spec admits reads a step for something other than a key is clean",
        list(audit.check_key_sites_exhaustive([Admitting()], admitted)) == [],
    )
    check(
        "the same site is an error when the spec does not admit it",
        kinds(list(audit.check_key_sites_exhaustive([FakeSpec([])], admitted)))
        == ["key-site-not-exhaustive"],
    )

    class Stale:
        """A spec admitting a function that is no longer there."""

        allowlists = {}
        front = {audit.NOT_A_KEY: ["crates/core/src/policy.rs::deleted_long_ago"]}

    found = list(audit.check_key_sites_exhaustive([Stale()], declaring({})))
    check(
        "an admission for a function that reads no Step field is reported",
        kinds(found) == ["key-site-admission-stale"]
        and found[0]["severity"] == audit.WARNING,
        str(kinds(found)),
    )

    # Tests read a step field by field constantly, and each one reported would bury the real site.
    in_a_test = declaring(
        {
            Path("crates/core/src/command.rs"): STEP_DECLARED
            + [
                "#[cfg(test)]",
                "mod tests {",
                "    use super::*;",
                "",
                "    impl Step {",
                "        fn sample() -> Self { Self::default() }",
                "    }",
                "",
                "    #[test]",
                "    fn every_field_survives() {",
                "        let step = Step::sample();",
                "        assert_eq!(step.resolved, other.resolved);",
                "        assert_eq!(step.args, other.args);",
                "        assert_eq!(step.environment, other.environment);",
                "    }",
                "}",
            ]
        }
    )
    found = list(audit.check_key_sites_exhaustive([FakeSpec([])], in_a_test))
    check(
        "a test module holding an impl block is still recognised as test code",
        found == [],
        str([one["summary"] for one in found]),
    )

    check(
        "a tree with no readable Step declaration is reported rather than silently passing",
        kinds(list(audit.check_key_sites_exhaustive([FakeSpec([])], {})))
        == ["key-sites-unreadable"],
    )

    check(
        "the tree's own key sites destructure",
        with_cwd(
            ROOT,
            lambda: list(
                audit.check_key_sites_exhaustive(audit.load_specs(), audit.mechanics.load_sources())
            ),
        )
        == [],
    )


# One job, named the way a check run is named, running a target the documentation makes a promise
# about. The promise is prose, so the fixture is prose: a rewording that stops matching is the thing
# being caught, the same as the exception counts above.
GATE_JOB = """\
name: CI

jobs:
  specs:
    name: Specs
    runs-on: ubuntu-latest
    steps:
      - name: Check the specs against the implementation
        run: make check-spec
"""

GATE_PROMISE = """\
# Checks

`make check-spec` checks the mechanical half of the specs. CI runs it too, so a new use of a
guarded symbol fails a pull request rather than waiting for somebody to notice it.
"""

GATE_TREE = {".github/workflows/ci.yml": GATE_JOB, "docs/development/checks.md": GATE_PROMISE}


def gates(files):
    return with_cwd(in_tree(files), lambda: list(audit.check_required_checks_are_the_gates()))


def test_required_checks_are_the_gates():
    found = gates({**GATE_TREE, "contrib/required-checks.txt": "# nothing yet\n"})
    check(
        "a job that runs a check and that no context names is an error",
        kinds(found) == ["ungated-check"] and found[0]["severity"] == audit.ERROR,
        str(kinds(found)),
    )
    check(
        "it names the job, the target and the promise made about it",
        bool(found)
        and found[0]["evidence"] == ["Specs runs make check-spec", "docs/development/checks.md:3"],
        str(found[0]["evidence"] if found else None),
    )

    # The rule is that a job whose purpose is a check gates, not that the page said so: a paragraph
    # making no promise leaves the job exactly as unrequired.
    silent = dict(GATE_TREE, **{"contrib/required-checks.txt": ""})
    silent["docs/development/checks.md"] = GATE_PROMISE.replace(
        "fails a pull request rather than waiting for somebody to notice it",
        "reports into a pull request",
    )
    check(
        "a job that runs a check is required whether or not the page promises anything",
        kinds(gates(silent)) == ["ungated-check"],
        str(kinds(gates(silent))),
    )

    # A promise about a target no job runs: the page says a pull request fails over something that
    # never runs on one.
    unrun = dict(GATE_TREE, **{"contrib/required-checks.txt": "Specs\n"})
    unrun["docs/development/checks.md"] = GATE_PROMISE + (
        "\n`make check-deps` decides deny.toml. CI runs it too, so a git dependency fails a pull\n"
        "request rather than warning into one.\n"
    )
    found = gates(unrun)
    check(
        "a target the page promises will fail a pull request that no job runs is an error",
        kinds(found) == ["unrun-promise"]
        and found[0]["evidence"] == ["docs/development/checks.md:6"],
        str(found[0]["evidence"] if found else kinds(found)),
    )

    found = gates({**GATE_TREE, "contrib/required-checks.txt": "Specs\nSecurity\n"})
    check(
        "a context that is the display name of no job is an error",
        kinds(found) == ["orphaned-required-check"] and found[0]["impact"] == "high",
        str(kinds(found)),
    )

    # The name in the file and the name of the job differ only in case, which is exactly the
    # difference between the organisation's `security` and this repository's `Security`.
    check(
        "a context matching a job's name but not its case is an error",
        kinds(gates({**GATE_TREE, "contrib/required-checks.txt": "specs\n"}))
        == ["orphaned-required-check", "ungated-check"],
    )

    # A trailing comment is not part of an unquoted YAML scalar, so it is not part of the name a
    # required context has to match.
    commented = dict(GATE_TREE, **{"contrib/required-checks.txt": "Specs\n"})
    commented[".github/workflows/ci.yml"] = GATE_JOB.replace(
        "name: Specs", "name: Specs # the mechanical half"
    )
    check(
        "a comment beside a job's name is not part of the name",
        gates(commented) == [],
        str(kinds(gates(commented))),
    )

    # Three ways a target arrives that reading the word after `make` would miss.
    behind = ("make -k check-spec", "make RUST_TEST_THREADS=8 check-spec", "make fmt check-spec")
    for command in behind:
        hidden = dict(GATE_TREE, **{"contrib/required-checks.txt": ""})
        hidden[".github/workflows/ci.yml"] = GATE_JOB.replace("make check-spec", command)
        check(
            f"a target reached by `{command}` is still a check the job runs",
            kinds(gates(hidden)) == ["ungated-check"],
            str(kinds(gates(hidden))),
        )

    check(
        "a job a context names is clean",
        gates({**GATE_TREE, "contrib/required-checks.txt": "# the gates\n\nSpecs\n"}) == [],
    )

    check(
        "a tree with no record of its gates at all is an error",
        kinds(gates(GATE_TREE)) == ["unrecorded-required-check"],
    )

    # The real file against the real workflows and the real page, which is what it is for.
    check(
        "the tree's own record names jobs that exist and covers every job running a check",
        with_cwd(ROOT, lambda: list(audit.check_required_checks_are_the_gates())) == [],
    )


def test_guarantee_specs_are_read():
    """Naming a spec in the list is only worth something if every use resolves it.

    The list went years matched by base name while an entry in a subdirectory would have been
    skipped by two of its three uses, which is exactly how `tools/run.md` stayed unread.
    """

    class At:
        def __init__(self, rel):
            self.rel = rel
            self.name = Path(rel).name

    check(
        "a spec in a subdirectory of docs/specs is recognised",
        audit.carries_the_guarantee(At("docs/specs/tools/run.md")),
    )
    check(
        "a spec at the top level is recognised",
        audit.carries_the_guarantee(At("docs/specs/labels.md")),
    )
    check(
        "a spec sharing a base name with a listed one is not recognised",
        not audit.carries_the_guarantee(At("docs/specs/elsewhere/labels.md")),
    )
    check(
        "a path outside docs/specs is not recognised",
        not audit.carries_the_guarantee(At("docs/labels.md")),
    )

    check(
        "every listed spec resolves to a file in this tree",
        with_cwd(ROOT, lambda: list(audit.check_guarantee_specs_exist())) == [],
        str([one["summary"] for one in with_cwd(ROOT, audit.check_guarantee_specs_exist)]),
    )

    moved = in_tree({f"docs/specs/{one}": "# spec\n" for one in audit.GUARANTEE_SPECS[:-1]})
    found = with_cwd(moved, lambda: list(audit.check_guarantee_specs_exist()))
    check(
        "a listed spec that is not a file is an error, not a silent skip",
        len(found) == 1 and found[0]["severity"] == audit.ERROR,
        str([one["summary"] for one in found]),
    )

    check(
        "every spec the list names is one load_specs returns",
        with_cwd(
            ROOT,
            lambda: len([one for one in audit.load_specs() if audit.carries_the_guarantee(one)]),
        )
        == len(audit.GUARANTEE_SPECS),
    )


def test_declassify_counts_match_the_spec():
    """The enumerator and `check-spec` have to measure the same thing.

    `docs/specs/labels.md` pins `Labelled::declassify` to a count per file and `make check-spec`
    fails when the code disagrees. This enumerator counts the same symbol its own way, and the two
    counts being equal is what says a lane starting from its list starts from the real list.
    """
    def measure():
        specs = audit.load_specs()
        sources = audit.mechanics.load_sources()
        pinned = {}
        for spec in specs:
            for item in spec.allowlists.get(audit.RELEASE, []):
                # A `sites:` entry is written `path: count`, and the path can hold a colon of its own.
                path, _, count = str(item).rpartition(":")
                if path.strip() and count.strip().isdigit():
                    pinned[path.strip()] = int(count)
        measured = {}
        for site in audit.sites_for(audit.RELEASE, sources):
            measured[site["path"]] = measured.get(site["path"], 0) + site["count"]
        return pinned, measured

    pinned, measured = with_cwd(ROOT, measure)
    check(f"`{audit.RELEASE}` is pinned somewhere", bool(pinned))
    for path, count in sorted(pinned.items()):
        check(
            f"{path} releases {count} times, as the spec pins",
            measured.get(path) == count,
            f"the enumerator counts {measured.get(path)}",
        )


def test_every_lane_prompt_composes():
    """A lane whose prompt fails to fill in produces a thinner audit and no error.

    The prompts are markdown with `.format()` placeholders, so a placeholder added to a lane file
    without a value in the enumerator raises, and one removed from the enumerator leaves the lane
    reading about a list that is not there. Both are silent in a run.
    """
    def build():
        specs = audit.load_specs()
        sources = audit.mechanics.load_sources()
        found = audit.surface(sources, specs)
        for name in audit.LANES:
            text = audit.build_lane(name, found, specs, Path("results.json"))
            left = SURVIVING.search(text)
            if left:
                return name, f"{left.group(0)} was never filled in"
            if "crates/" not in text and "docs/specs/" not in text:
                return name, "it names no place to start from"
            if "Untrusted content never enters" not in text:
                return name, "the rule is not stated in it"
            if "Write JSON to" not in text:
                return name, "it does not say what to return"
        return None, ""

    name, why = with_cwd(ROOT, build)
    check("every lane prompt composes with the rule and an output contract", name is None, f"{name}: {why}")


def test_verifier_prompt_composes():
    candidate = {
        "summary": "a branch on a released value",
        "place": "crates/core/src/policy.rs:100 in decide",
        "lane": "decisions-after-release",
    }
    text = verifying.build_prompt(candidate, Path("results.json"))
    check(
        "the verifier prompt composes and starts from disbelief",
        "start from the position that it is not" in text
        and "a branch on a released value" in text
        and not SURVIVING.search(text),
    )
    check(
        "the verifier is told the case that calibrates a false positive",
        "GET /v1/models" in text,
    )


def test_impact_sets_severity():
    candidate = {"summary": "s", "kind": "violation", "lane": "gates", "impact": "low"}
    merged = collect.merge(candidate, {"verdict": "CONFIRMED", "impact": "high", "reason": "r"})
    check(
        "the verifier's impact wins over the lane's, and sets the severity",
        merged["impact"] == "high" and merged["severity"] == collect.ERROR,
    )
    merged = collect.merge(candidate, {"verdict": "CONFIRMED", "reason": "r"})
    check(
        "a verifier that set no impact leaves the lane's",
        merged["impact"] == "low" and merged["severity"] == collect.WARNING,
    )
    merged = collect.merge(
        candidate, {"verdict": "CONFIRMED", "reason": "r", "corrected": {"summary": "narrower"}}
    )
    check("a correction wins over the claim it corrects", merged["summary"] == "narrower")


def test_labels():
    labels = drafts.label_set(
        {"kind": "violation", "impact": "high", "area": "trust"}
    )
    check(
        "every issue carries security, the kind, the severity and the area",
        labels == ["area/trust", "bug", "needs-security-review", "security", "severity/high"],
        str(labels),
    )
    check(
        "a workflow finding is infrastructure rather than an invented area",
        drafts.label_set({"kind": "unpinned-action", "impact": "high", "area": "infrastructure"})
        == ["bug", "infrastructure", "needs-security-review", "security", "severity/high"],
    )
    for impact in ("high", "medium", "low"):
        labels = drafts.label_set({"kind": "violation", "impact": impact, "area": "trust"})
        check(
            f"severity/{impact} is the only scale a run judges",
            not any(one.startswith(("importance/", "urgency/", "size/")) for one in labels),
        )
    check(
        "an area the tracker does not have is left off rather than invented",
        drafts.label_set({"kind": "violation", "impact": "low", "area": "kernel"})
        == ["bug", "needs-security-review", "security", "severity/low"],
    )


def test_titles():
    long = {
        "title": "the driver decides which file to write from bytes a page it fetched supplied, so a "
        "person approving one path gets another",
        "area": "trust",
        "kind": "violation",
    }
    title = drafts.title_for(long)
    check(
        "a title is under the limit, leads with the subsystem and has no backticks",
        len(title) <= drafts.TITLE_LIMIT and title.startswith("trust: ") and "`" not in title,
        title,
    )
    check(
        "a title already leading with its subsystem is not given it twice",
        drafts.title_for({"title": "LABEL-4: a witness is minted outside the gates", "clause": "LABEL-4"})
        == "LABEL-4: a witness is minted outside the gates",
    )
    check(
        "a title cut at the limit does not end on the word before the part that went over",
        not drafts.DANGLING.search(title) and not title.endswith(","),
        title,
    )
    check(
        "a lane's sentence is lowercased after the colon",
        drafts.title_for(
            {"title": "A failed fetch reports the URL the server chose", "clause": "LABEL-3"}
        ).startswith("LABEL-3: a failed fetch"),
    )
    check(
        "a symbol keeps the case it is spelled with",
        drafts.title_for(
            {"title": "`Labelled` now implements `PartialEq`, so content can be compared", "area": "trust"}
        ).startswith("trust: Labelled now implements PartialEq"),
    )
    colon = drafts.title_for(
        {
            "clause": "LAYER-2",
            "title": "LAYER-2's by-construction bracket justifies the clause from the properties of "
            "Labelled: reading one takes a witness, and two accessors are public",
        }
    )
    check(
        "a summary that says the mechanism before a colon is cut at the colon",
        colon.endswith("from the properties of Labelled"),
        colon,
    )
    coordination = drafts.title_for(
        {
            "area": "delegation",
            "title": "the drafter dereferences the place string a lane or a verifier wrote and "
            "inlines seven lines of it into a body it posts to a public tracker",
        }
    )
    check(
        "half a coordination whose other half went over the limit is dropped",
        coordination.endswith("a lane or a verifier wrote"),
        coordination,
    )
    comparison = drafts.title_for(
        {
            "area": "infrastructure",
            "title": "three containers that run with this whole tree are named by a movable tag "
            "rather than by the digest the build was tested against",
        }
    )
    check(
        "a comparison cut before what it compares to does not end on 'rather than'",
        comparison.endswith("by a movable tag"),
        comparison,
    )


def test_a_name_stays_a_name():
    """The fields a name is built from are prose, and a name has to open a file.

    `verify.md` asks for a clause id "else omit". A verifier that answered the question instead wrote
    265 characters into the field, and every draft in that run was lost: the name went past what the
    file system will open, so the drafter raised before writing any of them.
    """
    answered = {
        "kind": "reachable",
        "impact": "medium",
        "area": "infrastructure",
        "clause": "none. No clause governs agents/skills/, which is part of why nothing caught this; "
        "`infrastructure` is arguably the truer area label here, and draft-issues.py defines it as "
        "the build and the tracker rather than the product, so no spec lists this directory among "
        "the trees the guarantee is read to rest on.",
        "summary": "the drafter inlines the file `first_site` returns into an issue it posts",
        "place": "agents/skills/check-spec/draft-issues.py:126 in first_site",
        "gain": "a file outside the checkout is published verbatim",
        "evidence": ["agents/skills/check-spec/draft-issues.py:126 the path is read"],
        "fix": "hold the path inside the checkout",
        "lane": "supply-chain",
        "source": "audit",
        "verified_reason": "the body is posted to a public tracker",
    }
    out = Path(tempfile.mkdtemp(prefix="security-audit-selftest-")) / "issues"
    written = with_cwd(ROOT, lambda: drafts.draft([answered], out))
    check(
        "the body is written rather than the run dying on the name",
        len(written) == 1 and (out / f"{written[0]['slug']}.md").is_file(),
    )
    slug = drafts.slug_for(answered, set())
    check(
        "a sentence in the clause field does not become the name",
        len(slug) <= drafts.SLUG_LIMIT and slug.startswith("infrastructure"),
        slug,
    )
    check(
        "the title leads with the area the sentence was written instead of",
        drafts.title_for(answered).startswith("infrastructure: "),
        drafts.title_for(answered),
    )
    check(
        "a clause reference is still what a title leads with",
        drafts.subsystem({"clause": "FETCH-6 with LABEL-3", "area": "trust"})
        == "FETCH-6 with LABEL-3",
    )
    check(
        "the key dedup runs on is one the title keeps rather than the sentence",
        drafts.key_for(answered, drafts.title_for(answered)) == "first_site",
        drafts.key_for(answered, drafts.title_for(answered)),
    )
    taken = set()
    reference = "LABEL-4" + " and LABEL-5" * 40
    first = drafts.slug_for(dict(answered, clause=reference), taken)
    second = drafts.slug_for(dict(answered, clause=reference), taken)
    check(
        "a clause reference is as long as it was written, and two cut to one stem still differ",
        first != second and len(second) <= drafts.SLUG_LIMIT + 3,
        f"{first} / {second}",
    )


def test_bodies_say_where_and_why():
    finding = {
        "kind": "violation",
        "impact": "high",
        "area": "trust",
        "summary": "the driver branches on a released value",
        "place": "crates/core/src/policy.rs:1 in decide",
        "gain": "the attacker picks which file is written",
        "evidence": ["crates/core/src/policy.rs:1 the branch"],
        "fix": "hand the value to the effect instead of comparing it",
        "lane": "decisions-after-release",
        "source": "audit",
        "verified_reason": "the bytes reach the planner through the transcript",
        "reproduce": ["cargo test -p bravebot-core policy::"],
    }
    body = with_cwd(ROOT, lambda: drafts.body_for(finding))
    for wanted in (
        "User impact:",
        "## What happens",
        "## Reproduce",
        "## What this buys an attacker",
        "## Why bug and not spec-bug",
        "## The fix",
        "## How this was found",
        "cargo test -p bravebot-core policy::",
    ):
        check(f"a body says {wanted!r}", wanted in body)
    check("a body says a tool filed it", "rather than by a person" in body)
    check("no em dash reaches an issue body", "—" not in body)

    named = dict(finding, evidence=["crates/agent/src/tools.rs:1 the branch"])
    shown = with_cwd(ROOT, lambda: drafts.body_for(named))
    excerpt = shown.split("## Reproduce")[0]
    check(
        "the code a body shows is the place, not the first evidence item",
        "`crates/core/src/policy.rs:1`" in excerpt and "tools.rs" not in excerpt,
        excerpt,
    )

    outside = Path(tempfile.mkdtemp(prefix="security-audit-selftest-")) / "hosts.yml"
    outside.write_text("github.com:\n  oauth_token: gho_SELFTEST\n", encoding="utf-8")
    poisoned = dict(finding, place=f"{outside}:2 in decide")
    drafted = with_cwd(ROOT, lambda: drafts.body_for(poisoned))
    check(
        "a place outside the checkout is not excerpted into a body",
        "gho_SELFTEST" not in drafted and str(outside) not in drafted,
        str(outside),
    )

    local = dict(finding, reproduce=[f"grep -n problem {ROOT}/crates/agent/src/tools.rs"])
    said = with_cwd(ROOT, lambda: drafts.body_for(local))
    check(
        "no path from the machine the run happened on reaches an issue body",
        str(ROOT) not in said and "grep -n problem crates/agent/src/tools.rs" in said,
    )

    wants_screen = dict(finding, screen="the transcript after the fetch returns")
    out = Path(tempfile.mkdtemp(prefix="security-audit-selftest-")) / "issues"
    written = with_cwd(ROOT, lambda: drafts.draft([wants_screen], out))
    check(
        "a finding a person could see names the files to capture it into",
        written[0]["screen_wanted"] and not written[0]["has_screen"],
    )
    check(
        "the report says a screen is missing rather than posting without one",
        "wants a screen" in drafts.report(written, out),
    )


def test_posting_skips_what_the_tracker_already_holds():
    """Dedup, on the pair of findings that break the obvious way of doing it.

    Two specs with unpinned clauses produce one sentence with one word changed. Comparing wording
    alone calls the second a duplicate of the first, and the second never gets filed.
    """
    tracker = []

    def fake_gh(args, repo):
        """`gh issue list --search`, near enough: every term has to be in the title.

        Returning the whole tracker whatever was asked for would pass a query that finds nothing,
        which is the way a duplicate actually got filed: the search looked for a key that no title
        holds, found none, and read that as nothing like this being on the tracker.
        """
        if args[:2] != ["issue", "list"]:
            raise AssertionError(f"a dedup check called gh {' '.join(args[:2])}")
        query = args[args.index("--search") + 1]
        terms = [one.lower() for one in query.split() if ":" not in one]
        return json.dumps(
            [one for one in tracker if all(term in one["title"].lower() for term in terms)]
        )

    posting.gh = fake_gh

    check(
        "a run is paced to one issue every ten seconds and change",
        posting.PACE == 10.0 and posting.JITTER == (1.0, 5.0),
        f"{posting.PACE}, {posting.JITTER}",
    )

    # The shape the enumerator writes, including the part that makes this hard: the summary names a
    # path and the title shortens it to the file name, so the summary's own string is not in the
    # title and a key taken from it matches nothing on the tracker.
    findings = [
        {
            "kind": "unpinned-guarantee",
            "impact": "low",
            "area": "trust",
            "title": "nothing pins 1 clause of `layering.md`, so a change that breaks one passes every check",
            "summary": "`docs/specs/layering.md` has 1 clause at `verified-by: none`",
        },
        {
            "kind": "unpinned-guarantee",
            "impact": "low",
            "area": "trust",
            "title": "nothing pins 2 clauses of `processors.md`, so a change that breaks one passes every check",
            "summary": "`docs/specs/processors.md` has 2 clauses at `verified-by: none`",
        },
        # And the case with nothing to anchor to: everything this one names is a path its title does
        # not keep, so the wording is the whole of what dedup has.
        {
            "kind": "exception-count-disagreement",
            "impact": "low",
            "area": "trust",
            "title": "the review pass names fewer admitted exceptions than the spec, so an unnamed one reads as a violation",
            "summary": "`docs/development/reviewing-for-the-rule.md` says 2 and `docs/specs/labels.md` names 3",
        },
    ]
    out = Path(tempfile.mkdtemp(prefix="security-audit-selftest-")) / "issues"
    written = with_cwd(ROOT, lambda: drafts.draft(findings, out))
    check(
        "a draft's key is one its own title keeps, where the finding names one at all",
        all(one["key"].lower() in one["title"].lower() for one in written[:2]),
        str([(one["key"], one["title"]) for one in written[:2]]),
    )

    tracker = [{"number": 11, "state": "OPEN", "url": "u", "title": written[0]["title"]}]
    check(
        "a finding the tracker already holds is not filed again",
        posting.already_filed("brave/bravebot", written[0]) is not None,
    )
    check(
        "a finding whose wording matches another spec's is still filed",
        posting.already_filed("brave/bravebot", written[1]) is None,
    )

    tracker = [{"number": 12, "state": "CLOSED", "url": "u", "title": written[0]["title"]}]
    check(
        "an issue somebody read and closed is an answer, not a gap",
        posting.already_filed("brave/bravebot", written[0]) is not None,
    )

    tracker = [{"number": 13, "state": "OPEN", "url": "u", "title": written[2]["title"]}]
    check(
        "a finding with no key its title keeps is still recognised by its wording",
        posting.already_filed("brave/bravebot", written[2]) is not None,
        f"key {written[2]['key']!r} is not in {written[2]['title']!r}",
    )

    tracker = []
    check(
        "a finding nothing on the tracker names is filed",
        posting.already_filed("brave/bravebot", written[0]) is None,
    )


def test_an_assignee_reaches_the_create():
    """The login has to arrive at `gh issue create`, not just at the parser.

    Nothing else in this file calls `post`, so an `--assignee` that is parsed and never threaded
    through files every issue unassigned and still passes every other check here.
    """
    original = posting.gh
    calls = []

    def fake_gh(args, repo=None):
        calls.append(list(args))
        return "https://github.com/brave/bravebot/issues/1"

    posting.gh = fake_gh
    draft = {"title": "t", "body_file": "b", "labels": ["security"]}

    posting.post("brave/bravebot", draft, "netzenbot")
    passed = calls[-1]
    check(
        "the login a run was given is the one the create is passed",
        "--assignee" in passed and passed[passed.index("--assignee") + 1] == "netzenbot",
        str(passed),
    )

    posting.post("brave/bravebot", draft)
    check(
        "a run given no login names no assignee, so an unassigned issue stays the default",
        "--assignee" not in calls[-1],
        str(calls[-1]),
    )

    def refusing_gh(args, repo=None):
        raise RuntimeError("Not Found")

    posting.gh = refusing_gh
    check(
        "a login the repository would refuse is caught before anything is posted",
        not posting.assignable("brave/bravebot", "nobody"),
    )

    posting.gh = original


def test_a_run_writes_a_manifest_and_posts_nothing():
    """The mechanical half end to end, in a work directory, with no model anywhere in it."""
    def run():
        work = tempfile.mkdtemp(prefix="security-audit-selftest-")
        argv = sys.argv
        sys.argv = ["security-audit.py", "--work-dir", work]
        out, err = io.StringIO(), io.StringIO()
        try:
            with redirect_stdout(out), redirect_stderr(err):
                audit.main()
        finally:
            sys.argv = argv
        return work, json.loads(out.getvalue())

    work, said = with_cwd(ROOT, run)
    check("a run says where its work is", said.get("work_dir") == work)
    manifest = json.loads(Path(said["manifest"]).read_text(encoding="utf-8"))
    check(
        "a run writes one prompt per lane and no results",
        len(manifest["lanes"]) == len(audit.LANES)
        and all(Path(one["prompt_file"]).is_file() for one in manifest["lanes"])
        and not any(Path(one["results_file"]).exists() for one in manifest["lanes"]),
    )
    check(
        "a lane that returned nothing is reported rather than passed over",
        kinds(verifying.load_candidates(manifest)[1]) == ["lane-incomplete"] * len(audit.LANES),
    )


def main():
    for test in (
        test_exception_counts,
        test_network_clients_are_recorded,
        test_prompt_split,
        test_exhaustive_reader_docs,
        test_labelled_impls,
        test_pinned_actions,
        test_pinned_images,
        test_privileged_job_runs_only_its_own_code,
        test_checkout_ref_is_qualified,
        test_required_checks_are_the_gates,
        test_construction_pinned,
        test_gates_pinned,
        test_key_sites_exhaustive,
        test_guarantee_specs_are_read,
        test_declassify_counts_match_the_spec,
        test_every_lane_prompt_composes,
        test_verifier_prompt_composes,
        test_impact_sets_severity,
        test_labels,
        test_titles,
        test_a_name_stays_a_name,
        test_bodies_say_where_and_why,
        test_posting_skips_what_the_tracker_already_holds,
        test_an_assignee_reaches_the_create,
        test_a_run_writes_a_manifest_and_posts_nothing,
    ):
        print(test.__name__)
        test()
    print("")
    if FAILURES:
        print(f"{len(FAILURES)} failed: {', '.join(FAILURES)}")
        return 1
    print("security-audit selftest passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
