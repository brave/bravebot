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
import subprocess  # nosemgrep: gitlab.bandit.B404
import sys
import tempfile
from contextlib import contextmanager, redirect_stderr, redirect_stdout
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

    def labelled(*lines):
        return {Path("crates/core/src/value.rs"): list(lines)}

    for name, source in {
        "a derived PartialEq": labelled("#[derive(Clone, PartialEq)]", "pub struct Labelled<T> {"),
        "a derive wrapped over lines": labelled(
            "#[derive(", "    Clone,", "    std::hash::Hash,", ")]", "pub struct Labelled<T> {"
        ),
        "a derive inside cfg_attr": labelled(
            "#[cfg_attr(test, derive(Clone, Eq))]", "pub struct Labelled<T> {"
        ),
        "a derive above another attribute": labelled(
            "#[derive(Debug)]", "#[must_use]", "pub struct Labelled<T> {"
        ),
        "an impl with a nested generic bound": labelled(
            "impl<T: AsRef<str>> PartialEq<str> for Labelled<T> {"
        ),
        "an impl whose header wraps": labelled(
            "impl<T>", "    std::ops::Deref", "    for Labelled<T>", "where", "    T: Clone,", "{"
        ),
        "an impl with a generic trait argument": labelled(
            "impl<T: Into<Vec<u8>>> AsRef<[u8]> for Labelled<T> {"
        ),
    }.items():
        found = list(audit.check_labelled_impls(source))
        check(f"{name} is an error", kinds(found) == ["labelled-impl"], str(kinds(found)))

    for name, source in {
        "a derive of Clone alone": labelled("#[derive(Clone)]", "pub struct Labelled<T> {"),
        "a derive on another struct": labelled(
            "#[derive(PartialEq)]", "pub struct Shape {", "}", "pub struct Labelled<T> {"
        ),
        "a wrapped impl of the one recorded trait": labelled(
            "impl<T>", "    fmt::Debug", "    for Labelled<T> {"
        ),
        "an impl for another type that names Labelled in a bound": labelled(
            "impl<T> Deref for Wrapper<T>", "where", "    T: Into<Labelled<String>>,", "{"
        ),
    }.items():
        found = list(audit.check_labelled_impls(source))
        check(f"{name} is clean", found == [], str(kinds(found)))

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


def test_grant_is_behind_an_environment():
    def run(files):
        return with_cwd(in_tree(files), lambda: list(audit.check_grant_is_behind_an_environment()))

    bare = run({".github/workflows/publish-npm.yml": GRANT_BESIDE_THE_INSTALL})
    check(
        "a job holding id-token: write and no environment is an error naming the job",
        kinds(bare) == ["grant-without-environment"]
        and "`publish`" in bare[0]["summary"]
        and bare[0]["evidence"] == [
            ".github/workflows/publish-npm.yml:14 job `publish` holds id-token: write"
        ],
        str([one["summary"] for one in bare]),
    )

    named = run(
        {
            ".github/workflows/publish-npm.yml": GRANT_BESIDE_THE_INSTALL.replace(
                "    permissions:\n      contents: read\n      id-token: write",
                "    environment: npm-publish\n    permissions:\n      contents: read\n"
                "      id-token: write",
            )
        }
    )
    check("the same job naming an environment is clean", named == [], str(kinds(named)))

    mapping = run(
        {
            ".github/workflows/publish-npm.yml": GRANT_BESIDE_THE_INSTALL.replace(
                "    permissions:\n      contents: read\n      id-token: write",
                "    environment:\n      name: npm-publish\n    permissions:\n"
                "      contents: read\n      id-token: write",
            )
        }
    )
    check("the mapping form of environment is clean", mapping == [], str(kinds(mapping)))

    # The environment of another job is that job's, and a workflow-level grant reaches every job.
    other = run(
        {
            ".github/workflows/publish-npm.yml": GRANT_OVER_THE_INSTALL
            + "  other:\n    runs-on: ubuntu-latest\n    environment: npm-publish\n"
            "    steps:\n      - run: true\n"
        }
    )
    check(
        "a job without an environment is reported although a sibling names one",
        kinds(other) == ["grant-without-environment"] and "`publish`" in other[0]["summary"],
        str(kinds(other)),
    )

    check(
        "a job holding only contents: read needs no environment",
        run({".github/workflows/ci.yml": GRANT_BESIDE_THE_INSTALL.replace("id-token: write", "")})
        == [],
    )

    check(
        "the tree's own workflows put every grant behind an environment",
        with_cwd(ROOT, lambda: list(audit.check_grant_is_behind_an_environment())) == [],
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
        ("npm with an option first", "npm --prefix ui ci"),
        ("pnpm install", "pnpm install --frozen-lockfile"),
        ("pnpm i", "pnpm i"),
        ("pnpm add", "pnpm add left-pad"),
        ("pnpm run", "pnpm run lint"),
        ("pnpm run-script", "pnpm run-script lint"),
        ("pnpm exec", "pnpm exec lockfile-lint"),
        ("pnpm dlx", "pnpm dlx lockfile-lint"),
        ("pnpm x", "pnpm x lockfile-lint"),
        ("pnpm with a script name", "pnpm lint:lockfile"),
        ("pnpm test", "pnpm test"),
        ("pnpm start", "pnpm start"),
        ("pnpm rebuild", "pnpm rebuild"),
        ("pnpm approve-builds", "pnpm approve-builds"),
        ("pnpm --dir before the subcommand", "pnpm --dir ui install"),
        ("pnpm -C before the subcommand", "pnpm -C ui run build"),
        ("pnpm --filter before the subcommand", "pnpm --filter app exec tsc"),
        ("pnpm -r before the subcommand", "pnpm -r run build"),
        ("pnpm -w before the subcommand", "pnpm -w test"),
        ("corepack pnpm", "corepack pnpm install --frozen-lockfile"),
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

    for name, command in (
        ("pnpm publish", "pnpm publish --provenance"),
        ("pnpm config", "pnpm config set registry https://registry.npmjs.org/"),
        ("a sentence that names pnpm", "echo pnpm build is not run here"),
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
        check(f"{name} runs no dependency code", kinds(found) == [], str(kinds(found)))

    # The publish workflow holds the one grant in this tree, and this is what keeps the install out of
    # the job that holds it.
    check(
        "no job in the tree's own workflows runs a dependency beside a credential",
        with_cwd(ROOT, lambda: list(audit.check_privileged_job_runs_only_its_own_code())) == [],
    )


def project_code_workflow(permissions, steps, job_env=""):
    return (
        "permissions:\n  contents: read\n\njobs:\n  measure:\n    runs-on: ubuntu-latest\n"
        f"    permissions:\n{permissions}\n{job_env}    steps:\n{steps}"
    )


CHECKOUT_PERSISTING = "      - uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v7.0.1\n"
CHECKOUT_CLEAN = CHECKOUT_PERSISTING + "        with:\n          persist-credentials: false\n"
ISSUES_WRITE = "      contents: read\n      issues: write"


def test_privileged_job_runs_project_code():
    def run(workflow):
        tree = in_tree({".github/workflows/test-determinism.yml": workflow})
        return with_cwd(tree, lambda: list(audit.check_privileged_job_runs_only_its_own_code()))

    token_step = (
        CHECKOUT_CLEAN
        + "      - name: Test\n        env:\n          GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}\n"
        "        run: cargo test --all --locked\n"
    )
    found = run(project_code_workflow(ISSUES_WRITE, token_step))
    evidence = " | ".join(found[0]["evidence"]) if found else "(nothing)"
    check(
        "a cargo step whose environment holds the workflow token is reported, naming the step",
        kinds(found) == ["privileged-job-runs-project-code"] and "cargo test --all --locked" in evidence,
        f"{kinds(found)}: {evidence}",
    )

    script_step = (
        CHECKOUT_CLEAN
        + "      - name: Measure\n        env:\n          GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}\n"
        "        run: |\n          python3 contrib/measure-flakes.py --runs 100 --file-issues x/y\n"
    )
    found = run(project_code_workflow(ISSUES_WRITE, script_step))
    check(
        "the measuring script is reported where it holds the token, since it runs cargo",
        kinds(found) == ["privileged-job-runs-project-code"],
        str(kinds(found)),
    )

    mutants_step = (
        CHECKOUT_CLEAN
        + "      - name: Measure\n        env:\n          GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}\n"
        "        run: |\n          python3 contrib/measure-mutants.py run --package bravebot-core --save x.json\n"
    )
    found = run(project_code_workflow(ISSUES_WRITE, mutants_step))
    check(
        "the mutation script's run mode is reported where it holds the token, since it runs cargo",
        kinds(found) == ["privileged-job-runs-project-code"],
        str(kinds(found)),
    )
    report_step = (
        CHECKOUT_CLEAN
        + "      - name: Report\n        env:\n          GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}\n"
        "        run: python3 contrib/measure-mutants.py report shards/*.json\n"
    )
    check(
        "the mutation script's report mode reads files and is clean beside the token",
        run(project_code_workflow(ISSUES_WRITE, report_step)) == [],
    )

    check(
        "the same build in a job holding only contents: read is clean",
        run(
            project_code_workflow(
                "      contents: read", CHECKOUT_CLEAN + "      - run: cargo test --all --locked\n"
            )
        )
        == [],
    )

    found = run(
        project_code_workflow(
            ISSUES_WRITE, CHECKOUT_PERSISTING + "      - run: cargo test --all --locked\n"
        )
    )
    check(
        "a write grant is reported when the checkout leaves its credential for the build to read",
        kinds(found) == ["privileged-job-runs-project-code"],
        str(kinds(found)),
    )
    check(
        "the same job with persist-credentials: false is clean",
        run(
            project_code_workflow(
                ISSUES_WRITE, CHECKOUT_CLEAN + "      - run: cargo test --all --locked\n"
            )
        )
        == [],
    )

    found = run(
        project_code_workflow(
            ISSUES_WRITE,
            CHECKOUT_CLEAN + "      - run: cargo build\n",
            job_env="    env:\n      GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}\n",
        )
    )
    check(
        "a token in the job's own environment reaches every step, so a build in it is reported",
        kinds(found) == ["privileged-job-runs-project-code"],
        str(kinds(found)),
    )

    split = (
        CHECKOUT_CLEAN
        + "      - name: Measure\n        run: cargo test --all --locked\n"
        "      - name: File\n        env:\n          GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}\n"
        "        run: contrib/measure-flakes.py --load measured.json --file-issues x/y\n"
    )
    check(
        "a token in the step that only files, beside a build step that has none, is clean",
        run(project_code_workflow(ISSUES_WRITE, split)) == [],
    )

    found = run(project_code_workflow(ISSUES_WRITE, CHECKOUT_CLEAN + "      - run: cargo fmt --all --check\n"))
    check("a cargo command that compiles nothing is not reported", found == [], str(kinds(found)))


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


def test_downloads_verified():
    def found_in(files):
        return with_cwd(in_tree(files), lambda: list(audit.check_downloads_verified()))

    bare = found_in({"contrib/get.sh": "curl -sfL -o x https://example.com/x\nchmod +x x\n"})
    check(
        "a download made executable with no digest compared is one error",
        kinds(bare) == ["unverified-download"] and "contrib/get.sh:1" in bare[0]["evidence"],
        str(bare),
    )
    check(
        "a download compared with a digest before the chmod is clean",
        found_in({
            "contrib/get.sh": "curl -sfL -o x https://example.com/x\n"
            '[ "$(sha256sum x | cut -d" " -f1)" = "$want" ] || exit 1\nchmod +x x\n'
        }) == [],
    )
    check(
        "a comparison after the chmod does not count",
        kinds(found_in({
            "contrib/get.sh": "curl -o x https://example.com/x\nchmod +x x\nsha256sum x\n"
        })) == ["unverified-download"],
    )
    check(
        "a download piped into tar has nowhere to compare, so it is an error",
        kinds(found_in({
            "contrib/get.sh": "curl -sfL https://example.com/x.tgz \\\n  | tar xz -C bin x\n"
        })) == ["unverified-download"],
    )
    check(
        "a download piped into a shell is an error",
        kinds(found_in({"Makefile": "install:\n\tcurl -fsSL https://example.com/i | sh\n"}))
        == ["unverified-download"],
    )
    check(
        "a fetch whose bytes are never run, and a comment naming one, are clean",
        found_in({
            "contrib/get.sh": "# curl -o x URL; chmod +x x\ncurl -sf -o data.json https://example.com/d\n"
        }) == [],
    )
    check(
        "the tree's own scripts verify what they download",
        with_cwd(ROOT, lambda: list(audit.check_downloads_verified())) == [],
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


def key_tree(**bodies):
    """Sources holding the functions that key a name, each with the body given or a clean one.

    `normalise`, `key` (the trust map's), `file_authority_key` and `key_of` are the names a body is
    given under, and the helpers that fold and decompose a name are there with clean bodies. The
    map's `key` sits in an `impl` and a second function follows it, so a body that ran on would be
    read for a construct that belongs to the neighbour.
    """
    clean = {
        "normalise": "    path.split('/').collect::<Vec<_>>().join(\"/\")",
        "key": "    format!(\"{}/{}\", self.root, normalise(path))",
        "file_authority_key": "    self.state().trust.key(path)",
        "key_of": "    spelling::to_key(path, backslash_separates).into_owned()",
    }
    clean.update(bodies)
    return {
        Path("crates/core/src/trust.rs"): [
            "pub(crate) fn normalise(path: &str) -> String {",
            *clean["normalise"].split("\n"),
            "}",
            "pub fn fold_case(key: &str) -> String {",
            "    key.to_string()",
            "}",
            "pub(crate) fn canonical_form(key: &str, folds_case: bool) -> Cow<'_, str> {",
            "    Cow::Borrowed(key)",
            "}",
            "impl TrustStore {",
            "    pub(crate) fn key(&self, path: &str) -> String {",
            *clean["key"].split("\n"),
            "    }",
            "    fn other(&self, text: &str) -> String {",
            "        text.trim().to_string()",
            "    }",
            "}",
        ],
        Path("crates/core/src/file_authority.rs"): [
            "impl FileAuthority {",
            "    pub fn key(&self, path: &str) -> String {",
            *clean["file_authority_key"].split("\n"),
            "    }",
            "}",
        ],
        Path("crates/core/src/permissions.rs"): [
            "fn key_of(path: &str, backslash_separates: bool, folds_case: bool) -> String {",
            *clean["key_of"].split("\n"),
            "}",
            "fn fold(path: &str, folds: bool) -> String {",
            "    path.to_string()",
            "}",
            "fn drive_in_upper_case(key: &str, backslash_separates: bool) -> Cow<'_, str> {",
            "    Cow::Borrowed(key)",
            "}",
            "fn elsewhere(text: &str) -> String {",
            "    text.to_lowercase()",
            "}",
        ],
    }


def allowing(*entries, spec_id="TRUST"):
    spec = FakeSpec([])
    spec.front = {"id": spec_id, audit.KEY_STEP_ALLOWED: list(entries)}
    return spec


def key_findings(tree, *specs):
    return list(audit.check_key_steps_join_no_names(list(specs) or [FakeSpec([])], tree))


def test_key_steps_join_no_names():
    """The check that keeps a trim, a fold or a lossy decode out of the functions that key a name."""
    check("keys that only split and rejoin the segments are clean", key_findings(key_tree()) == [])

    found = key_findings(
        key_tree(
            normalise="    path.trim().split('/').collect::<Vec<_>>().join(\"/\")",
        )
    )
    check(
        "a trim in `normalise` is an error naming the function and the construct",
        kinds(found) == ["key-step-joins-names"]
        and found[0]["severity"] == audit.ERROR
        and "normalise" in found[0]["title"]
        and "trim" in found[0]["title"],
        str(kinds(found)),
    )
    check(
        "the finding says where the call is and what to do about a deliberate one",
        found
        and found[0]["evidence"]
        == ["crates/core/src/trust.rs:2: path.trim().split('/').collect::<Vec<_>>().join(\"/\")"]
        and "key_step_allowed" in found[0]["fix"]
        and "PATH-007" in found[0]["fix"],
        str(found[0]) if found else "no finding",
    )

    for construct, body in (
        ("trim", "    path.split('/').map(str::trim).collect::<Vec<_>>().join(\"/\")"),
        ("trim_left", "    path.trim_left().to_string()"),
        ("trim_matches", "    path.trim_matches(' ').to_string()"),
        ("trim_end_matches", "    path.trim_end_matches('.').to_string()"),
        ("to_lowercase", "    path.to_lowercase()"),
        ("to_lowercase", "    str::to_lowercase(path)"),
        ("to_ascii_lowercase", "    path.to_ascii_lowercase()"),
        ("to_string_lossy", "    path_buf.to_string_lossy().into_owned()"),
        ("to_string_lossy", "    Path::to_string_lossy(&path_buf).into_owned()"),
        ("from_utf8_lossy", "    String::from_utf8_lossy(bytes).into_owned()"),
        ("fold_case", "    fold_case(path)"),
        ("to_lowercase", "    path.chars().flat_map(char::to_lowercase).collect()"),
        ("canonical_form", "    canonical_form(path, true).into_owned()"),
        ("decompose", "    crate::canonical::decompose(path)"),
        ("nfc", "    path.nfc().collect()"),
    ):
        found = key_findings(key_tree(key=body))
        check(
            f"`{construct}` as `{body.strip()}` in the map's `key` is an error",
            kinds(found) == ["key-step-joins-names"] and construct in found[0]["title"],
            f"{kinds(found)}",
        )

    check(
        "an iterator's `.fold(` is not a case fold",
        key_findings(
            key_tree(key_of="    path.split('/').fold(String::new(), |all, one| all + one)")
        )
        == [],
    )
    check(
        "a construct named in a string or a comment is not one",
        key_findings(
            key_tree(
                key='    // never call .trim() here\n    panic!("expected no .to_lowercase() in {}", path)'
            )
        )
        == [],
    )
    check(
        "a construct in the function after a key function is not read as the key's",
        key_findings(key_tree()) == [],
        "`other` and `elsewhere` call trim and to_lowercase in the clean tree",
    )
    check(
        "a brace in a string or a character does not carry the body into the next function",
        key_findings(
            key_tree(key='    let open = \'{\';\n    format!("{open}/{}", self.root)')
        )
        == [],
    )
    check(
        "a quote character does not pair with the next string and move the brace depth",
        key_findings(key_tree(key='    let quote = \'"\';\n    let open = "{";\n    quote.to_string()'))
        == [],
    )

    generic = key_tree(key="    path.trim().to_string()")
    generic[Path("crates/core/src/trust.rs")] = [
        line.replace("fn key(&self,", "fn key<'a>(&'a self,")
        for line in generic[Path("crates/core/src/trust.rs")]
    ]
    check(
        "a key function with generics is still read",
        kinds(key_findings(generic)) == ["key-step-joins-names"],
    )

    in_a_test = key_tree()
    in_a_test[Path("crates/core/src/trust.rs")] += [
        "#[cfg(test)]",
        "mod tests {",
        '    const OPEN: &str = "{";',
        "    fn key(text: &str) -> String {",
        "        text.trim().to_string()",
        "    }",
        "}",
    ]
    check(
        "a function of the same name in a test module is not a key function",
        key_findings(in_a_test) == [],
    )

    folding = key_tree(key_of="    fold(&keyed, folds_case)")
    reason = "crates/core/src/permissions.rs::key_of fold: case is folded where the volume folds it"
    check(
        "a construct the spec allows with a reason is clean",
        key_findings(folding, allowing(reason)) == [],
    )
    check(
        "the same construct with no allowance is an error",
        kinds(key_findings(folding)) == ["key-step-joins-names"],
    )
    found = key_findings(folding, allowing("crates/core/src/permissions.rs::key_of fold:"))
    check(
        "an allowance with no reason is reported even though it matches a call",
        kinds(found) == ["key-step-allowance-unexplained"] and found[0]["severity"] == audit.ERROR,
        str(kinds(found)),
    )
    found = key_findings(key_tree(), allowing(reason))
    check(
        "an allowance the function no longer needs is a warning",
        kinds(found) == ["key-step-allowance-stale"] and found[0]["severity"] == audit.WARNING,
        str(kinds(found)),
    )
    check(
        "an allowance for one construct does not cover another in the same function",
        kinds(
            key_findings(
                key_tree(key_of="    fold(&keyed, folds_case).trim().to_string()"), allowing(reason)
            )
        )
        == ["key-step-joins-names"],
    )

    twice = key_tree(key_of="    fold(&fold(&keyed, true), folds_case)")
    found = key_findings(twice, allowing(reason))
    check(
        "a second call of an allowed construct is an error, since the entry allows one",
        kinds(found) == ["key-step-joins-names"] and "allows 1" in found[0]["summary"],
        str(kinds(found)),
    )
    check(
        "an entry saying how many calls it allows covers that many",
        key_findings(twice, allowing(reason.replace("fold:", "fold x2:"))) == [],
    )
    found = key_findings(folding, allowing(reason.replace("fold:", "fold x2:")))
    check(
        "an entry allowing more calls than the function makes is a warning",
        kinds(found) == ["key-step-allowance-stale"],
        str(kinds(found)),
    )

    found = key_findings(folding, allowing(reason), allowing(reason, spec_id="OTHER"))
    check(
        "an entry in a spec other than the trust spec is reported and does not count",
        kinds(found) == ["key-step-allowance-misplaced"],
        str(kinds(found)),
    )
    check(
        "an entry in only another spec leaves the construct unallowed",
        kinds(key_findings(folding, allowing(reason, spec_id="OTHER")))
        == ["key-step-allowance-misplaced", "key-step-joins-names"],
    )
    scalar = FakeSpec([])
    scalar.front = {"id": "TRUST", audit.KEY_STEP_ALLOWED: reason}
    check(
        "a single entry written as a scalar is read as one entry, not letter by letter",
        key_findings(folding, scalar) == [],
    )

    missing = key_tree()
    del missing[Path("crates/core/src/permissions.rs")]
    found = key_findings(missing)
    check(
        "a key function that cannot be found is reported rather than silently passing",
        kinds(found) == ["key-function-missing"] * 3
        and any("key_of" in one["title"] for one in found),
        str(kinds(found)),
    )

    check(
        "the tree's own key functions keep no construct the spec does not allow",
        with_cwd(
            ROOT,
            lambda: list(
                audit.check_key_steps_join_no_names(
                    audit.load_specs(), audit.mechanics.load_sources()
                )
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


def test_the_prompt_lane_starts_from_every_prompt():
    """A prompt missing from the lane's list is one the lane never asks about.

    The list is read off the files the prompting spec governs, so a prompt added to one of them, or a
    file added to the spec, is on it without anybody remembering to put it there.
    """
    class Governing:
        id = "PROMPT"
        rel = "docs/specs/asking.md"
        governs = ["crates/tui/src/asks.rs", "ui/src/Cards.tsx"]

    sources = {
        Path("crates/tui/src/asks.rs"): [
            "pub fn ask<B: Backend>(terminal: &mut Terminal<B>) -> Answer {",
            "pub fn ask_vet<B: Backend>(",
            "fn draw_vet(frame: &mut Frame) {",
            "pub(crate) fn ask_move(request: &MoveRequest) -> Answer {",
        ],
        Path("crates/tui/src/elsewhere.rs"): ["pub fn ask_other<B: Backend>() {}"],
        Path("ui/src/Cards.tsx"): [
            "    case 'write': {",
            "      return (",
            "        <DecisionCard",
            "          head={<YesOrNo />}",
            "    case 'plan-task':",
            "    case 'plan-reply':",
            "      return <div className=\"plan-reply\" />",
            "    case 'fetch': {",
            "          <YesOrNo",
            "    case 'move':",
            "      return <ThreeAnswers />",
        ],
    }
    found = [
        (site["path"], site["line"], site["function"])
        for site in audit.prompt_sites(sources, [Governing()])
    ]
    check(
        "the prompt lane starts from every function and card the prompting spec's files ask with",
        found
        == [
            ("crates/tui/src/asks.rs", 1, "ask"),
            ("crates/tui/src/asks.rs", 2, "ask_vet"),
            ("crates/tui/src/asks.rs", 4, "ask_move"),
            ("ui/src/Cards.tsx", 1, "case 'write'"),
            ("ui/src/Cards.tsx", 8, "case 'fetch'"),
            ("ui/src/Cards.tsx", 10, "case 'move'"),
        ],
        str(found),
    )

    real = with_cwd(
        ROOT, lambda: audit.prompt_sites(audit.mechanics.load_sources(), audit.load_specs())
    )
    names = {(Path(site["path"]).name, site["function"]) for site in real}
    wanted = {
        ("write.rs", "ask"),
        ("mcp.rs", "ask_mcp_call"),
        ("trust_prompt.rs", "ask"),
        ("Transcript.tsx", "case 'confirm'"),
        ("Transcript.tsx", "case 'vet'"),
        ("Transcript.tsx", "case 'mcp-call'"),
    }
    check(
        "the tree's write, call and trust prompts and the window's cards are on the lane's list",
        wanted <= names and ("Transcript.tsx", "case 'ask'") not in names,
        str(sorted(names)),
    )

    was = audit.mechanics.changed_files
    try:
        for touched in ["crates/tui/src/asks.rs", "docs/specs/asking.md"]:
            audit.mechanics.changed_files = lambda base, touched=touched: iter([touched])
            lanes = audit.changed_lanes("base", [Governing()])
            check(
                f"a branch that touches {touched} runs the lane that asks about prompts",
                "shown-before-answered" in lanes,
                str(lanes),
            )
    finally:
        audit.mechanics.changed_files = was


PINNED_ROW = (
    "| one effect | a write | the body | the value | the one effect "
    "| `verified-by: bravebot_demo::keys::a_key_tells_two_cases_apart` |"
)
ACCEPTED_ROW = (
    "| the trust map | `y` | the directory | the path | the session "
    "| accepted: [costs.md](costs.md#known-costs) |"
)
KEPT = [
    "Confirmer::confirm_write: one effect",
    "Confirmer::interjection: nothing",
    "Answers::trust: the trust map",
    "trust_prompt::ask: the trust map",
    "Asker::ask_to_start: one effect",
]


def register_findings(
    kept=KEPT,
    rows=(PINNED_ROW, ACCEPTED_ROW),
    trait="Confirmer",
    methods=("confirm_write", "interjection"),
    fields=("trust",),
    asks=("ask",),
    questions=("ask_to_start",),
    spec_id="PROMPT",
    elsewhere="",
    visibility="pub ",
):
    """Run the register check in a tree holding one of each declaration it reads."""
    front = "".join(f"  - {one}\n" for one in kept)
    tree = in_tree(
        {
            "docs/specs/asking.md": f"---\nid: {spec_id}\ntitle: Asking\nstatus: normative\n"
            f"governs:\n  - crates/tui/src/trust_prompt.rs\nkept_in:\n{front}---\n\n"
            '<a id="PROMPT-11"></a>\n### PROMPT-11: the register\n\n'
            "| Answer kept | Asked by | Shown | Kept under | Lasts | Evidence |\n"
            "|---|---|---|---|---|---|\n" + "".join(f"{row}\n" for row in rows) + "\n**Why.** So.\n",
            "docs/specs/costs.md": "---\nid: COST\n---\n\n## Known costs\n\n- **One.** It costs.\n",
            "docs/specs/plain.md": f"---\nid: PLAIN\n{elsewhere}---\n\n### Known costs\n\nNone.\n",
            "crates/agent/src/confirm.rs": f"pub trait {trait} {{\n"
            + "".join(f"    /// Asks.\n    fn {one}(&mut self) -> Decision;\n" for one in methods)
            + "}\n",
            "crates/agent/src/answers.rs": "pub struct Answers<Rules = ()> {\n"
            + "".join(f"    /// Kept.\n    {visibility}{one}: Kept,\n" for one in fields)
            + "}\n",
            "crates/agent/src/servers.rs": "pub trait Asker {\n"
            + "".join(f"    /// Asks.\n    fn {one}(&mut self) -> Answer;\n" for one in questions)
            + "}\n",
            "crates/tui/src/trust_prompt.rs": "".join(
                f"{one}<B: Backend>(terminal: &mut Terminal<B>) -> Answer {{\n}}\n"
                for one in (f"pub fn {ask}" if " " not in ask else ask for ask in asks)
            ),
            "crates/demo/Cargo.toml": '[package]\nname = "bravebot-demo"\n',
            "crates/demo/src/keys.rs": "#[cfg(test)]\nmod tests {\n    #[test]\n"
            "    fn a_key_tells_two_cases_apart() {}\n}\n",
        }
    )
    return with_cwd(
        tree,
        lambda: list(
            audit.check_answers_registered(audit.load_specs(), audit.mechanics.load_sources())
        ),
    )


def test_answers_registered():
    """A question or a remembered set with no row is how a yes comes to cover something unshown.

    The check reads the declarations rather than a list, so each fixture adds a member to one of
    them and expects the finding that names it, and each row fault is one a reader would take for a
    decision somebody made.
    """
    found = register_findings()
    check("a register accounting for every question and set is silent", found == [], str(kinds(found)))

    cases = [
        ("a confirmer method nobody listed", {"methods": ("confirm_write", "interjection", "confirm_new")}, ["answer-unregistered"]),
        ("a server question nobody listed", {"questions": ("ask_to_start", "ask_to_move")}, ["answer-unregistered"]),
        ("a remembered set nobody listed", {"fields": ("trust", "granted")}, ["answer-unregistered"]),
        ("a trust question nobody listed", {"asks": ("ask", "ask_again")}, ["answer-unregistered"]),
        ("a field with no visibility nobody listed", {"fields": ("trust", "granted"), "visibility": ""}, ["answer-unregistered"]),
        (
            "a field visible to the crate nobody listed",
            {"fields": ("trust", "granted"), "visibility": "pub(crate) "},
            ["answer-unregistered"],
        ),
        ("an entry for a member that is gone", {"kept": KEPT + ["Answers::gone: the trust map"]}, ["kept-in-stale"]),
        (
            "an entry landing in a row the table does not have",
            {"kept": KEPT[:2] + ["Answers::trust: a row nobody wrote", KEPT[3], KEPT[4]]},
            ["register-row-unknown"],
        ),
        ("an entry that is not member and row", {"kept": KEPT + ["the trust map"]}, ["kept-in-malformed"]),
        ("an entry for a declaration the check reads nothing of", {"kept": KEPT + ["Elsewhere::thing: nothing"]}, ["kept-in-malformed"]),
        ("a row with a cell left empty", {"rows": (PINNED_ROW, ACCEPTED_ROW.replace("the session", ""))}, ["register-cell-empty"]),
        ("a row with a cell past the last column", {"rows": (PINNED_ROW, ACCEPTED_ROW + " more |")}, ["register-cell-empty"]),
        (
            "a row citing a test that does not exist",
            {"rows": (PINNED_ROW.replace("a_key_tells_two_cases_apart", "a_key_tells_nothing_apart"), ACCEPTED_ROW)},
            ["register-test-missing"],
        ),
        ("a row citing neither a test nor a cost", {"rows": (PINNED_ROW, ACCEPTED_ROW.replace("accepted:", "see"))}, ["register-unpinned"]),
        (
            "a row accepting a cost from a spec that admits none",
            {"rows": (PINNED_ROW, ACCEPTED_ROW.replace("costs.md", "plain.md"))},
            ["register-cost-missing"],
        ),
        ("a clause with no rows under its header", {"rows": ()}, ["register-unreadable"]),
        ("a confirmer that is no longer a trait the check can find", {"trait": "Asker"}, ["register-unreadable"]),
        ("a trust prompt that asks nothing the check can read", {"asks": ()}, ["register-unreadable"]),
        ("a prompting spec that is no longer the one with the register's id", {"spec_id": "ASKING"}, ["register-unreadable"]),
        ("a trust question only the crate can see", {"asks": ("ask", "pub(crate) fn ask_again")}, ["answer-unregistered"]),
        ("a public helper whose name only starts like a question", {"asks": ("ask", "asking_line")}, []),
        ("a member listed twice", {"kept": KEPT + ["Answers::trust: one effect"]}, ["kept-in-duplicate"]),
        ("an entry keeping nothing and naming a row", {"kept": KEPT[:1] + ["Confirmer::interjection: nothing; one effect"] + KEPT[2:]}, ["kept-in-malformed"]),
        ("a list in a spec the check does not read it from", {"elsewhere": "kept_in:\n  - Answers::trust: the trust map\n"}, ["kept-in-misplaced"]),
        ("a cell holding a dash", {"rows": (PINNED_ROW, ACCEPTED_ROW.replace("the session", "-"))}, ["register-cell-empty"]),
        ("a cell holding n/a", {"rows": (PINNED_ROW, ACCEPTED_ROW.replace("the session", "n/a"))}, ["register-cell-empty"]),
        ("a row of empty cells", {"rows": (PINNED_ROW, ACCEPTED_ROW, "| | | | | | |")}, ["register-cell-empty", "register-unpinned"]),
        ("a cost the cell says is not accepted", {"rows": (PINNED_ROW, ACCEPTED_ROW.replace("accepted:", "not accepted:"))}, ["register-unpinned"]),
    ]
    for name, change, expected in cases:
        found = register_findings(**change)
        check(f"{name} is reported", kinds(found) == expected, str(kinds(found)))

    stale = register_findings(kept=KEPT + ["Answers::gone: the trust map"])
    check(
        "an entry outliving its member is a warning, since nothing is let through by it yet",
        [one["severity"] for one in stale] == [audit.WARNING],
        str(stale),
    )
    unlisted = register_findings(fields=("trust", "granted"))
    check(
        "the finding names the member and where it is declared",
        unlisted and "Answers::granted" in unlisted[0]["title"] and "crates/agent/src/answers.rs" in unlisted[0]["summary"],
        str(unlisted[0] if unlisted else "no finding"),
    )

    asker = register_findings(kept=[one for one in KEPT if not one.startswith("Asker::")])
    check(
        "a question asked through the servers' asker with no entry is named by the finding",
        kinds(asker) == ["answer-unregistered"] and "Asker::ask_to_start" in asker[0]["title"],
        str(asker),
    )

    def tool_list_cells():
        spec = next(one for one in audit.load_specs() if one.id == audit.PROMPT_SPEC)
        return [
            line
            for line in spec.lines
            if line.startswith("| MCP tool lists approved |")
        ]

    cells = with_cwd(ROOT, tool_list_cells)
    check(
        "no cell of the tool list row says a yes is kept in a file, which SERVERS-8 says it is not",
        len(cells) == 2 and not any("a yes lives in a file every later session reads" in one for one in cells),
        str(len(cells)),
    )

    missing = with_cwd(ROOT, lambda: [one for one in audit.REMEMBERED_SETS if not Path(one).is_file()])
    check("every file the binding lane is selected by exists", missing == [], str(missing))

    class Governing:
        id = "PROMPT"
        rel = "docs/specs/asking.md"
        governs = ["crates/tui/src/asks.rs"]

    was = audit.mechanics.changed_files
    try:
        for touched, wanted in [
            ("crates/agent/src/confirm.rs", True),
            ("crates/core/src/credentials.rs", True),
            ("crates/agent/src/tools.rs", True),
            ("crates/tui/src/asks.rs", True),
            ("crates/tui/src/app.rs", True),
            ("crates/tui/src/app/actions.rs", True),
            ("docs/specs/asking.md", True),
            ("crates/core/src/label.rs", False),
        ]:
            audit.mechanics.changed_files = lambda base, touched=touched: iter([touched])
            lanes = audit.changed_lanes("base", [Governing()])
            check(
                f"a branch that touches {touched} "
                + ("runs" if wanted else "does not run")
                + " the lane that asks what a kept answer covers",
                ("approval-binding" in lanes) == wanted,
                str(lanes),
            )
    finally:
        audit.mechanics.changed_files = was


CHANGED = (
    "Moved",
    "New session",
    "Subject replaced",
    "Other directory",
    "Invocation changed",
    "Declaration changed",
    "More than shown",
)
CHANGES_HEADER = "| Answer kept | " + " | ".join(CHANGED) + " |"
NOT_REACHED = "not reached: the key holds nothing of it"
TABLE_TEST = "bravebot_agent::kept_answers::changing_the_world_under_a_yes_asks_again"


def changes_row(name, **said):
    """One row of the table of changes, every cell a reason unless the caller says otherwise."""
    cells = {change: NOT_REACHED for change in CHANGED}
    cells.update(said)
    return "| " + name + " | " + " | ".join(cells[change] for change in CHANGED) + " |"


def changes_findings(
    rows=None,
    header=CHANGES_HEADER,
    table=True,
    cases=(("the trust map", "Moved"),),
    registered=("one effect", "the trust map"),
):
    """Run the check on what changes do to an answer in a tree holding a register and its cases."""
    if rows is None:
        rows = (
            changes_row(
                "one effect",
                **{"Moved": "asked again: the key holds the root `verified-by: bravebot_demo::keys::a_key_tells_two_cases_apart`"},
            ),
            changes_row(
                "the trust map",
                **{
                    "Moved": f"asked again: a rule is a path made absolute `verified-by: {TABLE_TEST}`",
                    "Subject replaced": "accepted: [costs.md](costs.md#known-costs)",
                    "Other directory": "by design: a directory covers what is beneath it; asked again: a sibling is not reached `verified-by: bravebot_demo::keys::a_key_tells_two_cases_apart`",
                },
            ),
        )
    register = "".join(
        f"| {name} | `y` | the directory | the path | the session | accepted: [costs.md](costs.md#known-costs) |\n"
        for name in registered
    )
    second = (
        f"\nWhat changes do.\n\n{header}\n|---|{'---|' * len(CHANGED)}\n"
        + "".join(f"{row}\n" for row in rows)
        if table
        else ""
    )
    literals = "".join(
        f'        Case {{\n            row: "{row}",\n            change: "{change}",\n            asked_again: f,\n        }},\n'
        for row, change in cases
    )
    tree = in_tree(
        {
            "docs/specs/asking.md": "---\nid: PROMPT\ntitle: Asking\nstatus: normative\n"
            "governs:\n  - crates/tui/src/trust_prompt.rs\nkept_in:\n  - Answers::trust: the trust map\n---\n\n"
            '<a id="PROMPT-11"></a>\n### PROMPT-11: the register\n\n'
            "| Answer kept | Asked by | Shown | Kept under | Lasts | Evidence |\n"
            "|---|---|---|---|---|---|\n" + register + second + "\n**Why.** So.\n",
            "docs/specs/costs.md": "---\nid: COST\n---\n\n## Known costs\n\n- **One.** It costs.\n",
            "docs/specs/plain.md": "---\nid: PLAIN\n---\n\n### Known costs\n\nNone.\n",
            "crates/demo/Cargo.toml": '[package]\nname = "bravebot-demo"\n',
            "crates/demo/src/keys.rs": "#[cfg(test)]\nmod tests {\n    #[test]\n"
            "    fn a_key_tells_two_cases_apart() {}\n}\n",
            "crates/agent/Cargo.toml": '[package]\nname = "bravebot-agent"\n',
            "crates/agent/src/kept_answers.rs": "fn table() {\n    let cases = vec![\n" + literals + "    ];\n}\n"
            "#[test]\nfn changing_the_world_under_a_yes_asks_again() {}\n",
        }
    )
    return with_cwd(
        tree,
        lambda: list(
            audit.check_changes_under_a_yes(audit.load_specs(), audit.mechanics.load_sources())
        ),
    )


def test_changes_under_a_yes():
    """A kept answer with nothing said of a moved checkout or a replaced file is how a yes covers more.

    Each fixture is the clean table with one thing wrong, and the finding it expects is the one a
    reader would otherwise find out by a person being asked nothing. The cells are read for the
    leader and the reason because a cell holding a mark reads as decided when nobody decided it.
    """
    found = changes_findings()
    check("a table with a reasoned cell for every answer and change is silent", found == [], str(kinds(found)))

    good = {
        "Other directory": "asked again: a sibling is not reached `verified-by: bravebot_demo::keys::a_key_tells_two_cases_apart`",
    }
    cases = [
        ("a register with no second table", {"table": False}, ["changes-unreadable"]),
        (
            "a table whose columns are not the changes",
            {"header": CHANGES_HEADER.replace("Moved", "Relocated")},
            ["changes-unreadable"],
        ),
        (
            "an answer in the register with no row in the table",
            {"registered": ("one effect", "the trust map", "hosts answered")},
            ["changes-row-missing"],
        ),
        (
            "a row for an answer the register does not have",
            {"registered": ("one effect",)},
            ["changes-row-unknown"],
        ),
        (
            "a row written twice",
            {
                "rows": (
                    changes_row("one effect", **good),
                    changes_row("one effect", **good),
                    changes_row("the trust map", **good),
                ),
                "cases": (),
            },
            ["changes-row-duplicate"],
        ),
        (
            "a cell left empty",
            {"rows": (changes_row("one effect", **{"Moved": ""}), changes_row("the trust map", **good)), "cases": ()},
            ["changes-cell-unsaid"],
        ),
        (
            "a cell holding n/a",
            {"rows": (changes_row("one effect", **{"Moved": "n/a"}), changes_row("the trust map", **good)), "cases": ()},
            ["changes-cell-unsaid"],
        ),
        (
            "a cell that opens with none of the four words",
            {
                "rows": (changes_row("one effect", **{"Moved": "see the spec for this one"}), changes_row("the trust map", **good)),
                "cases": (),
            },
            ["changes-cell-unsaid"],
        ),
        (
            "a not reached with no reason",
            {"rows": (changes_row("one effect", **{"Moved": "not reached: no"}), changes_row("the trust map", **good)), "cases": ()},
            ["changes-cell-unsaid"],
        ),
        (
            "a cell past the last column",
            {
                "rows": (
                    changes_row("one effect", **good).replace(" |\n", " | more |\n") + " more |",
                    changes_row("the trust map", **good),
                ),
                "cases": (),
            },
            ["changes-cell-unsaid"],
        ),
        (
            "an asked again that cites no test",
            {
                "rows": (changes_row("one effect", **{"Moved": "asked again: the key holds the root"}), changes_row("the trust map", **good)),
                "cases": (),
            },
            ["changes-unpinned"],
        ),
        (
            "an asked again citing a test that does not exist",
            {
                "rows": (
                    changes_row("one effect", **{"Moved": "asked again: the key holds the root `verified-by: bravebot_demo::keys::a_key_tells_nothing`"}),
                    changes_row("the trust map", **good),
                ),
                "cases": (),
            },
            ["changes-test-missing"],
        ),
        (
            "an accepted that links no Known cost",
            {
                "rows": (changes_row("one effect", **{"Moved": "accepted: it is fine"}), changes_row("the trust map", **good)),
                "cases": (),
            },
            ["changes-cost-missing"],
        ),
        (
            "an accepted linking a spec that admits none",
            {
                "rows": (
                    changes_row("one effect", **{"Moved": "accepted: [plain.md](plain.md#known-costs)"}),
                    changes_row("the trust map", **good),
                ),
                "cases": (),
            },
            ["changes-cost-missing"],
        ),
        ("a cell citing the table test with no case for it", {"cases": ()}, ["changes-case-missing"]),
        (
            "a case that no cell cites",
            {"cases": (("the trust map", "Moved"), ("one effect", "Other directory"))},
            ["changes-case-unlisted"],
        ),
    ]
    for name, change, expected in cases:
        found = changes_findings(**change)
        check(f"{name} is reported", kinds(found) == expected, str(kinds(found)))

    found = changes_findings(
        rows=(
            changes_row("one effect", **{"Moved": "by design: the move keeps it; asked again: a copy forgets it `verified-by: bravebot_demo::keys::a_key_tells_two_cases_apart`"}),
            changes_row("the trust map", **{"Moved": f"asked again: the root moves `verified-by: {TABLE_TEST}`"}),
        ),
    )
    check("a cell of two statements is read as two", found == [], str(kinds(found)))


def test_the_stop_hook():
    """A task that ends with the register broken is sent back, and one that never went near it is not.

    The hook decides two things a session depends on: whether the working tree's diff is one the
    check is worth running for, and what to say when it fails. A hook that never blocks and one that
    blocks forever both read as a quiet failure, so each is run against the input that tells it apart
    from the other.
    """
    broken = audit.finding(audit.ERROR, "answer-unregistered", "t", "a set nobody listed", "trust", "low")
    only_warned = audit.finding(audit.WARNING, "kept-in-stale", "t", "an entry outliving its member", "trust", "low")

    check("a tree the pass finds nothing in is let stop", audit.stop_response({}, []) is None)
    check("a warning alone is let stop", audit.stop_response({}, [only_warned]) is None)
    blocked = audit.stop_response({}, [only_warned, broken])
    check(
        "an error blocks the stop and hands the session the finding to fix",
        blocked is not None
        and blocked.get("decision") == "block"
        and "a set nobody listed" in blocked["reason"]
        and "an entry outliving its member" not in blocked["reason"],
        str(blocked),
    )
    again = audit.stop_response({"stop_hook_active": True}, [broken])
    check(
        "a stop after a block is let through with the finding shown, so a session that cannot fix it ends",
        again is not None and "decision" not in again and "a set nobody listed" in again["systemMessage"],
        str(again),
    )

    specs = with_cwd(ROOT, audit.load_specs)
    for touched, wanted in [
        ("crates/agent/src/answers.rs", True),
        ("crates/agent/src/kept_answers.rs", True),
        ("crates/ui-bridge/src/running.rs", True),
        ("crates/cli/src/plain.rs", True),
        ("crates/tui/src/app.rs", True),
        ("crates/tui/src/app/actions.rs", True),
        ("crates/tui/src/confirm.rs", True),
        ("docs/specs/prompting.md", True),
        ("crates/core/src/label.rs", False),
        ("README.md", False),
    ]:
        check(
            f"a working tree that touches {touched} "
            + ("is checked" if wanted else "is not checked"),
            audit.touches_kept_answers({touched}, specs) == wanted,
        )

    with tempfile.TemporaryDirectory() as tmp:
        def git(*arguments):
            subprocess.run(
                ["git", "-c", "user.name=t", "-c", "user.email=t@example.invalid", "-c", "commit.gpgsign=false", *arguments],
                cwd=tmp,
                check=True,
                capture_output=True,
            )

        git("init", "-q")
        for name in ("edited.txt", "moved.txt", "same.txt"):
            (Path(tmp) / name).write_text("before\n", encoding="utf-8")
        git("add", ".")
        git("commit", "-q", "-m", "start")
        none_yet = sorted(with_cwd(tmp, audit.branch_files))
        check(
            "a clean tree with no default branch to measure from has changed nothing",
            none_yet == [],
            str(none_yet),
        )
        git("update-ref", "refs/remotes/upstream/main", "HEAD")
        (Path(tmp) / "committed.txt").write_text("kept\n", encoding="utf-8")
        git("add", "committed.txt")
        git("commit", "-q", "-m", "a change the session already committed")
        (Path(tmp) / "edited.txt").write_text("after\n", encoding="utf-8")
        git("mv", "moved.txt", "renamed.txt")
        (Path(tmp) / "staged.txt").write_text("new\n", encoding="utf-8")
        git("add", "staged.txt")
        (Path(tmp) / "deep").mkdir()
        (Path(tmp) / "deep" / "untracked.txt").write_text("new\n", encoding="utf-8")
        got = sorted(with_cwd(tmp, audit.uncommitted_files))
        check(
            "the working tree's diff is the edited, staged, untracked and both ends of a renamed path",
            got == sorted(["edited.txt", "moved.txt", "renamed.txt", "staged.txt", "deep/untracked.txt"]),
            str(got),
        )
        whole = sorted(with_cwd(tmp, audit.branch_files))
        check(
            "the branch's changes are those and what it has already committed since the default branch",
            whole == sorted(got + ["committed.txt"]),
            str(whole),
        )

    wired = json.loads((ROOT / "agents" / "settings.json").read_text(encoding="utf-8"))
    commands = [
        hook.get("command", "")
        for entry in wired.get("hooks", {}).get("Stop", [])
        for hook in entry.get("hooks", [])
    ]
    check(
        "the Stop hook in agents/settings.json runs this script in the mode that answers it",
        commands == ['python3 "$CLAUDE_PROJECT_DIR/agents/skills/security-audit/security-audit.py" --stop-hook']
        and (ROOT / "agents/skills/security-audit/security-audit.py").is_file(),
        str(commands),
    )


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

    ranged = dict(
        finding,
        evidence=[
            "crates/core/src/policy.rs:1-3,9 the branch",
            "crates/agent/src/tools.rs:7-8 the effect",
        ],
    )
    listed = with_cwd(ROOT, lambda: drafts.body_for(ranged))
    check(
        "a line range or list in the evidence stays inside the code span that names its file",
        "- `crates/core/src/policy.rs:1-3,9` the branch" in listed
        and "- `crates/agent/src/tools.rs:7-8` the effect" in listed,
        listed[listed.find("Where it is") :][:300],
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


def test_a_draft_key_is_one_its_title_keeps():
    """The key the issue-poster definition searches the tracker for has to be in the title.

    The summary names a path and the title shortens it to the file name, so a key taken from the
    summary can match no title on the tracker, and a search for it reads as nothing like this being filed.
    """
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
    ]
    out = Path(tempfile.mkdtemp(prefix="security-audit-selftest-")) / "issues"
    written = with_cwd(ROOT, lambda: drafts.draft(findings, out))
    check(
        "a draft's key is one its own title keeps, where the finding names one at all",
        all(one["key"].lower() in one["title"].lower() for one in written),
        str([(one["key"], one["title"]) for one in written]),
    )
    report = drafts.report(written, out)
    check(
        "the report sends each draft to the issue-poster definition",
        "agents/agents/issue-poster.md" in report and "post-issues" not in report,
        report,
    )


def test_a_run_writes_a_manifest_and_posts_nothing():
    """The mechanical half end to end, in a work directory, with no model anywhere in it."""
    def run():
        work = tempfile.mkdtemp(prefix="security-audit-selftest-")
        listed = Path(work) / "issues.json"
        listed.write_text(json.dumps([{"number": 7, "title": "a filed defect", "state": "OPEN"}]))
        argv = sys.argv
        sys.argv = ["security-audit.py", "--work-dir", work, "--tracker", str(listed)]
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
    tracker = Path(work) / "tracker.txt"
    check(
        "a run writes the tracker's titles where every lane's prompt says to find them",
        tracker.read_text(encoding="utf-8") == "#7 open: a filed defect\n"
        and all(str(tracker) in Path(one["prompt_file"]).read_text(encoding="utf-8") for one in manifest["lanes"])
        and any("1 security issues" in line for line in manifest["progress_lines"]),
        str(manifest["progress_lines"]),
    )

    def mechanical():
        asked = []
        was = audit.tracker_entries
        audit.tracker_entries = lambda *args, **kwargs: asked.append(args) or ([], None)
        argv = sys.argv
        sys.argv = ["security-audit.py", "--mechanical-only", "--work-dir", tempfile.mkdtemp()]
        try:
            with redirect_stdout(io.StringIO()), redirect_stderr(io.StringIO()):
                audit.main()
        finally:
            sys.argv = argv
            audit.tracker_entries = was
        return asked

    check(
        "a mechanical-only run, which CI makes, never asks the tracker for anything",
        with_cwd(ROOT, mechanical) == [],
    )


def test_the_manifest_is_read_from_the_work_directory():
    with tempfile.TemporaryDirectory() as tmp:
        (Path(tmp) / "manifest.json").write_text('{"lanes": []}', encoding="utf-8")
        check("a manifest in the work directory is read", collect.read_manifest(tmp) == {"lanes": []})
        empty = Path(tmp) / "empty"
        empty.mkdir()
        err = io.StringIO()
        with redirect_stderr(err):
            missing = collect.read_manifest(empty)
        check(
            "a work directory without one reads as none and says where it looked",
            missing is None and err.getvalue() == f"no manifest at {empty / 'manifest.json'}\n",
            err.getvalue(),
        )


def test_verifying_saves_the_candidates_the_collector_reads():
    """The collector takes each candidate, and where its verdict goes, from the manifest.

    A verify pass that pairs candidates with verifiers and does not save the pairing leaves a report
    of no candidates, whatever the verifiers decided.
    """
    with tempfile.TemporaryDirectory() as tmp:
        work = Path(tmp)
        lane_results = work / "gates_results.json"
        lane_results.write_text(
            json.dumps({"candidates": [{"summary": "a claim", "place": "crates/core/src/policy.rs:1"}]}),
            encoding="utf-8",
        )
        collect.workdir.write_manifest(
            work,
            {
                "lanes": [
                    {
                        "lane": "gates",
                        "prompt_file": str(work / "gates_prompt.md"),
                        "results_file": str(lane_results),
                    }
                ]
            },
        )
        argv = sys.argv
        sys.argv = ["verify-findings.py", "--work-dir", str(work)]
        try:
            with redirect_stdout(io.StringIO()):
                status = verifying.main()
        finally:
            sys.argv = argv
        check("pairing candidates with verifiers exits clean", status == 0, str(status))

        pairs = collect.read_manifest(work).get("verifications", [])
        if not check(
            "the manifest keeps one verification for the one candidate",
            len(pairs) == 1 and pairs[0]["lane"] == "gates" and Path(pairs[0]["prompt_file"]).is_file(),
            str(pairs),
        ):
            return
        Path(pairs[0]["results_file"]).write_text(
            json.dumps({"verdict": "CONFIRMED", "impact": "low"}), encoding="utf-8"
        )
        findings = collect.load_verdicts(collect.read_manifest(work))[0]
        check(
            "a verdict written where the manifest says is collected as a finding",
            [one["lane"] for one in findings] == ["gates"],
            str(findings),
        )


LISTED = (
    "disallowed-methods = [\n"
    '    { path = "std::process::Command::new", reason = "r" },\n'
    '    { path = "windows_sys::Win32::System::Threading::CreateProcessW", reason = "r" },\n'
    "]\n"
)
MARKED = (
    "fn go(program: &Path) {\n"
    '    #[allow(clippy::disallowed_methods, reason = "the person named it")]\n'
    "    let _ = std::process::Command::new(program);\n"
    "}\n"
)


def audit_starts(files):
    where = in_tree(files)
    return with_cwd(
        where,
        lambda: list(audit.check_process_starts_are_marked(audit.mechanics.load_sources())),
    )


def test_process_starts_are_refused_by_the_configuration():
    found = audit_starts({"clippy.toml": LISTED, "crates/sandbox/src/programs.rs": MARKED})
    check("a start marked with a reason is not a finding", found == [], str(found))

    found = audit_starts(
        {
            "clippy.toml": 'disallowed-methods = ["std::process::Command::new", '
            '"windows_sys::Win32::System::Threading::CreateProcessW"]\n',
        }
    )
    check("the calls listed as bare strings are read as listed", found == [], str(found))

    for name, files in {
        "no clippy.toml": {},
        "a clippy.toml listing another call": {
            "clippy.toml": 'disallowed-methods = [{ path = "std::env::var" }]\n'
        },
        "a clippy.toml naming both calls only in comments": {
            "clippy.toml": "# std::process::Command::new\n"
            "# windows_sys::Win32::System::Threading::CreateProcessW\n"
            "disallowed-methods = []\n"
        },
        "a clippy.toml naming both calls only after a list that is empty": {
            "clippy.toml": "disallowed-methods = [] # std::process::Command::new, "
            "windows_sys::Win32::System::Threading::CreateProcessW\n"
        },
        "a clippy.toml banning both under another key": {
            "clippy.toml": 'disallowed-types = [{ path = "std::process::Command::new" }, '
            '{ path = "windows_sys::Win32::System::Threading::CreateProcessW" }]\n'
        },
        "a clippy.toml that cannot be read": {"clippy.toml": "disallowed-methods = [\n"},
        "a clippy.toml without the Windows call": {
            "clippy.toml": 'disallowed-methods = [{ path = "std::process::Command::new" }]\n'
        },
        "a clippy.toml without the Unix call": {
            "clippy.toml": "disallowed-methods = "
            '[{ path = "windows_sys::Win32::System::Threading::CreateProcessW" }]\n'
        },
    }.items():
        found = audit_starts({**files, "crates/sandbox/src/programs.rs": MARKED})
        check(
            f"{name} is an error, since nothing refuses a start",
            kinds(found) == ["process-start-not-refused"] and found[0]["severity"] == audit.ERROR,
            str(kinds(found)),
        )
    found = audit_starts(
        {
            "clippy.toml": 'disallowed-methods = [{ path = "std::process::Command::new" }]\n',
            "crates/sandbox/src/programs.rs": MARKED,
        }
    )
    check(
        "the call missing from clippy.toml is the one the error names",
        bool(found)
        and "CreateProcessW" in found[0]["summary"]
        and "Command::new" not in found[0]["summary"],
        str([one["summary"] for one in found]),
    )


def test_nothing_makes_clippy_read_another_configuration():
    crate = {"clippy.toml": LISTED, "crates/agent/Cargo.toml": "[package]\n"}
    for name, (files, hidden) in {
        "a clippy.toml in a crate": ({"crates/agent/clippy.toml": 'msrv = "1.90"\n'}, "crates/agent/clippy.toml"),
        "a .clippy.toml in a crate": ({"crates/agent/.clippy.toml": 'msrv = "1.90"\n'}, "crates/agent/.clippy.toml"),
        "a clippy.toml above the crates": ({"crates/clippy.toml": 'msrv = "1.90"\n'}, "crates/clippy.toml"),
        "a .clippy.toml beside the root one": ({".clippy.toml": 'msrv = "1.90"\n'}, ".clippy.toml"),
        "a manifest that allows the lint": (
            {"crates/agent/Cargo.toml": '[lints.clippy]\ndisallowed_methods = "allow"\n'},
            "crates/agent/Cargo.toml",
        ),
        "a manifest that allows the lint with a level": (
            {"Cargo.toml": '[workspace.lints.clippy]\ndisallowed_methods = { level = "allow", priority = 1 }\n'},
            "Cargo.toml",
        ),
        "a cargo configuration that moves clippy's directory": (
            {".cargo/config.toml": '[env]\nCLIPPY_CONF_DIR = "elsewhere"\n'},
            ".cargo/config.toml",
        ),
    }.items():
        found = audit_starts({**crate, **files})
        check(
            f"{name} is an error naming the file",
            kinds(found) == ["process-start-refusal-hidden"] and hidden in found[0]["summary"],
            str([one["summary"] for one in found]),
        )

    for name, files in {
        "a clippy.toml deeper than any crate's manifest": {
            "crates/agent/tests/fixtures/one/clippy.toml": 'msrv = "1.90"\n'
        },
        "a manifest that denies the lint": {
            "crates/agent/Cargo.toml": '[lints.clippy]\ndisallowed_methods = "deny"\n'
        },
    }.items():
        found = audit_starts({**crate, **files})
        check(f"{name} is not a finding", found == [], str(found))


def test_a_mark_is_one_start_with_a_reason():
    def marks(attribute, path="crates/sandbox/src/programs.rs", after="fn start() {}\n"):
        return audit_starts(
            {"clippy.toml": LISTED, path: f"fn go() {{}}\n{attribute}\n{after}"}
        )

    def at_line_two(found, path="crates/sandbox/src/programs.rs"):
        return all(f"{path}:2" in one["summary"] for one in found)

    for name, attribute in {
        "no reason": "#[allow(clippy::disallowed_methods)]",
        "an empty reason": '#[allow(clippy::disallowed_methods, reason = "")]',
        "a blank reason": '#[allow(clippy::disallowed_methods, reason = "   ")]',
        "a reason of only a line continuation": '#[allow(clippy::disallowed_methods, reason = "\\\n   ")]',
        "an expect with no reason": "#[expect(clippy::disallowed_methods)]",
        "another lint beside it": "#[allow(unused, clippy::disallowed_methods)]",
    }.items():
        found = marks(attribute)
        check(
            f"an allow with {name} is an error naming the file and line",
            kinds(found) == ["process-start-mark-without-reason"]
            and found[0]["severity"] == audit.ERROR
            and at_line_two(found),
            str([one["summary"] for one in found]),
        )

    # A test's allow is no production mark, so it is read in a file the list does not name.
    elsewhere = "crates/cli/src/identity.rs"
    found = marks("#![cfg_attr(test, allow(clippy::disallowed_methods))]", path=elsewhere)
    check(
        "an allow with a crate-wide test allow and no reason is an error naming the file and line",
        kinds(found) == ["process-start-mark-without-reason"]
        and found[0]["severity"] == audit.ERROR
        and at_line_two(found, elsewhere),
        str([one["summary"] for one in found]),
    )

    for name, attribute in {
        "a reason": '#[allow(clippy::disallowed_methods, reason = "the person named it")]',
        "a reason on its own lines": "#[allow(\n    clippy::disallowed_methods,\n"
        '    reason = "a path written here, \\\n              and nothing a stage wrote"\n)]',
        "a reason holding brackets": '#[allow(clippy::disallowed_methods, reason = "see [a] ] and )")]',
        "an expect with a reason": '#[expect(clippy::disallowed_methods, reason = "the person named it")]',
    }.items():
        check(f"an attribute with {name} is not a finding", marks(attribute) == [], str(marks(attribute)))

    for name, attribute in {
        "a deny, which loosens nothing": "#![deny(clippy::disallowed_methods)]",
        "a warn, which loosens nothing": "#[warn(clippy::disallowed_methods)]",
    }.items():
        found = marks(attribute, path="crates/cli/src/identity.rs")
        check(f"an attribute with {name} is not a finding", found == [], str(found))

    for name, (attribute, after) in {
        "a crate-wide allow": ('#![allow(clippy::disallowed_methods, reason = "needed")]', "fn start() {}\n"),
        "an allow on a module": (
            '#[allow(clippy::disallowed_methods, reason = "needed")]',
            "mod inner {\n    fn start() {}\n}\n",
        ),
        "an allow on a public module behind another attribute": (
            '#[allow(clippy::disallowed_methods, reason = "needed")]\n#[cfg(unix)]',
            "pub(crate) mod inner {}\n",
        ),
        "an allow on an impl": (
            '#[allow(clippy::disallowed_methods, reason = "needed")]',
            "impl Thing {\n    fn start() {}\n}\n",
        ),
        "an allow of the style group": ('#[allow(clippy::style)]', "fn start() {}\n"),
        "an allow of the all group": ('#![allow(clippy::all)]', "fn start() {}\n"),
        "an allow of warnings": ('#[allow(warnings)]', "fn start() {}\n"),
    }.items():
        # A group allow holds no mark, so a file the list names would also be reported as holding
        # none; one that holds the lint is read where marks are allowed.
        path = (
            "crates/sandbox/src/programs.rs"
            if "disallowed_methods" in attribute
            else "crates/cli/src/identity.rs"
        )
        found = audit_starts({"clippy.toml": LISTED, path: f"fn go() {{}}\n{attribute}\n{after}"})
        check(
            f"{name} is an error, since it lifts the refusal from more than one start",
            kinds(found) == ["process-start-mark-too-wide"] and found[0]["severity"] == audit.ERROR,
            str([one["summary"] for one in found]),
        )

    for name in ("testutil.rs", "test_support.rs", "helpers_tests.rs"):
        found = audit_starts(
            {
                "clippy.toml": LISTED,
                f"crates/cli/src/{name}": "fn go() {}\n#![allow(clippy::disallowed_methods)]\n",
            }
        )
        check(
            f"a crate-wide allow in {name} is still too wide, since only a directory exempts it",
            "process-start-mark-too-wide" in kinds(found),
            str(kinds(found)),
        )

    found = marks("#![allow(clippy::disallowed_methods)]")
    check(
        "a crate-wide allow with no reason is both too wide and unexplained",
        kinds(found) == ["process-start-mark-too-wide", "process-start-mark-without-reason"],
        str(kinds(found)),
    )

    for name, (attribute, path) in {
        "a crate-wide test allow": (
            '#![cfg_attr(test, allow(clippy::disallowed_methods, reason = "a test"))]',
            "crates/cli/src/identity.rs",
        ),
        "a crate-wide test allow of every lint": (
            "#![cfg_attr(test, allow(clippy::all))]",
            "crates/cli/src/identity.rs",
        ),
        "an integration test file's allow": (
            '#![allow(clippy::disallowed_methods, reason = "a test")]',
            "crates/cli/tests/running.rs",
        ),
        "a test support file's allow": (
            '#![allow(clippy::disallowed_methods, reason = "a test")]',
            "crates/session/test-support/profile.rs",
        ),
    }.items():
        found = marks(attribute, path=path)
        check(f"{name} is not a finding", found == [], str(found))

    found = marks("#![allow(clippy::disallowed_methods)]", path="crates/cli/tests/running.rs")
    check(
        "an integration test file's allow still needs a reason",
        kinds(found) == ["process-start-mark-without-reason"],
        str(kinds(found)),
    )

    for name, body in {
        "a comment": "// #[allow(clippy::disallowed_methods)]\nfn go() {}\n",
        "a string": "fn go() -> &'static str { \"clippy::disallowed_methods\" }\n",
        "a comment about warnings": "// #[allow(warnings)] is not used\nfn go() {}\n",
    }.items():
        found = audit_starts({"clippy.toml": LISTED, "crates/cli/src/identity.rs": body})
        check(f"the lint named in {name} is not a finding", found == [], str(found))

    found = audit_starts(
        {
            "clippy.toml": LISTED,
            "crates/cli/tests/a.rs": '#[allow(clippy::disallowed_methods, reason = "named")]\nfn a() {}\n',
            "crates/cli/tests/b.rs": "fn b() {}\n\n#[allow(clippy::disallowed_methods)]\nfn b2() {}\n"
            '#[allow(clippy::disallowed_methods, reason = "named")]\nfn b3() {}\n',
        }
    )
    check(
        "only the mark without a reason is reported, at its own line",
        kinds(found) == ["process-start-mark-without-reason"]
        and "crates/cli/tests/b.rs:3" in found[0]["summary"],
        str([one["summary"] for one in found]),
    )


def test_a_test_attribute_covers_only_what_it_is_attached_to():
    """A `#[cfg(test)]` on a field, a `use` or a statement leaves the code after it in the list.

    Reading the next `{` after the attribute as the item's made a test-only field above
    an `impl` hide the whole `impl` from every start list. Each shape below sits above code that has
    to stay listed.
    """
    lines = [
        "pub struct Holder {",                      # 1
        "    #[cfg(test)]",                         # 2
        "    pub(crate) seen: Vec<String>,",        # 3
        "    live: u32,",                           # 4
        "}",                                        # 5
        "#[cfg(test)]",                             # 6
        "use std::fs::canonicalize;",               # 7
        "impl Holder {",                            # 8
        "    fn real(&self) {",                     # 9
        "        #[cfg(test)]",                     # 10
        "        let probe = 1;",                   # 11
        "        std::fs::canonicalize(a);",        # 12
        "    }",                                    # 13
        "}",                                        # 14
        "#[cfg(test)]",                             # 15
        "mod tests {",                              # 16
        "    fn t() { std::fs::canonicalize(b); }",  # 17
        "}",                                        # 18
        "#[cfg(test)]",                             # 19
        "fn helper<A, B>(a: A,",                    # 20
        "    b: B) {",                              # 21
        "    canonicalize(c);",                     # 22
        "}",                                        # 23
        "fn after() { canonicalize(d); }",          # 24
    ]
    covered = audit.test_lines(lines)
    check(
        "a test-only field, use and statement cover their own line and nothing after them",
        {3, 7, 11} <= covered and not covered & {4, 8, 9, 12, 13, 14},
        str(sorted(covered)),
    )
    literals = [
        "#[cfg(test)]",                             # 1
        "mod tests {",                              # 2
        "    fn t() { let c = '{'; let s = \"}\"; }",  # 3
        "    fn u() { let d = '}'; }",              # 4
        "}",                                        # 5
        "fn after() { canonicalize(d); }",          # 6
    ]
    check(
        "a brace in a string or a character literal does not move the end of a test module",
        audit.test_lines(literals) == {1, 2, 3, 4, 5},
        str(sorted(audit.test_lines(literals))),
    )
    check(
        "a test module and a generic test function are covered to their closing brace",
        set(range(16, 19)) <= covered and set(range(20, 24)) <= covered and 24 not in covered,
        str(sorted(covered)),
    )


def test_one_answer_to_which_lines_are_test_code():
    """Every check that skips tests asks `test_lines`, so a shape it reads right is right everywhere."""
    field_above_impl = declaring(
        {
            Path("crates/agent/src/confine.rs"): [
                "pub struct Confinement {",
                "    #[cfg(test)]",
                "    seen: Vec<String>,",
                "}",
                "impl Confinement {",
                "    fn holds(&self, step: &Step) -> bool {",
                "        length_prefixed_path(out, &step.resolved);",
                "        length_prefixed(out, &step.args);",
                "        true",
                "    }",
                "}",
            ]
        }
    )
    found = list(audit.check_key_sites_exhaustive([FakeSpec([])], field_above_impl))
    check(
        "a test-only field above an impl does not hide the impl's functions from the key check",
        kinds(found) == ["key-site-not-exhaustive"],
        str(kinds(found)),
    )

    def site(path, number):
        return {"path": path, "line": number, "core": False, "test_file": False}

    path = Path("crates/agent/src/turn.rs")
    on_a_statement = {
        path: [
            "fn collect() {",
            "    #[cfg(test)]",
            "    if let Some(x) = probe() { note(x); }",
            "    policy.present(a);",
            "}",
        ]
    }
    kept = audit.outside_the_kernel([site(str(path), 3), site(str(path), 4)], on_a_statement)
    check(
        "a test-only statement hides only itself, so the production call after it stays listed",
        [one["line"] for one in kept] == [4],
        str(kept),
    )

    in_a_module = {
        path: [
            "#[cfg(test)]",
            "mod tests {",
            "    impl Fixture {",
            "        fn new() {}",
            "    }",
            "    fn helper() {",
            "        policy.present(a);",
            "    }",
            "}",
            "fn live() {",
            "    policy.present(b);",
            "}",
        ]
    }
    kept = audit.outside_the_kernel([site(str(path), 7), site(str(path), 11)], in_a_module)
    check(
        "a non-pub helper after an impl inside a test module is still test code",
        [one["line"] for one in kept] == [11],
        str(kept),
    )


def test_the_identity_lane_starts_from_the_calls_that_resolve_a_name():
    """The identity lane starts from every call that turns a name into a thing, outside the tests."""
    sources = {
        Path("crates/agent/src/workspace.rs"): [
            "pub struct Workspace {",
            "    #[cfg(test)]",
            "    pub seen: Vec<String>,",
            "}",
            "impl Workspace {",
            "    fn open(&self, named: &Path) {",
            "        let real = std::fs::canonicalize(named);",
            "        let links = self.hard_links(named);",
            "        // symlink_metadata is named in this comment only",
            "    }",
            "    fn open_again(&self) { let fd = PathFd::new(path); let l = fs::read_link(p); }",
            "}",
            "#[cfg(test)]",
            "mod tests {",
            "    fn t() { std::fs::canonicalize(x); }",
            "}",
        ],
        Path("crates/agent/tests/workspace.rs"): ["fn t() { std::fs::symlink_metadata(x); }"],
        Path("crates/agent/test-support/aliases.rs"): ["fn t() { std::fs::canonicalize(x); }"],
        Path("crates/sandbox/src/testutil.rs"): ["fn t() { std::fs::canonicalize(x); }"],
    }
    found = [(s["path"], s["line"], s["function"]) for s in audit.identity_sites(sources)]
    check(
        "a call in code that ships is listed with the function it is in, and a comment, a test module, "
        "a test file and a test helper are not",
        found
        == [
            ("crates/agent/src/workspace.rs", 7, "open"),
            ("crates/agent/src/workspace.rs", 8, "open"),
            ("crates/agent/src/workspace.rs", 11, "open_again"),
        ],
        str(found),
    )

    real = with_cwd(ROOT, lambda: audit.identity_sites(audit.mechanics.load_sources()))
    names = {(Path(s["path"]).name, s["function"]) for s in real}
    check(
        "the tree's own name-to-thing calls are listed, the workspace's hard-link check among them",
        ("workspace.rs", "hard_links") in names and ("linux.rs", "opened_rows") in names,
        str(sorted(names)[:8]),
    )
    rows = with_cwd(ROOT, lambda: audit.confinement_rows(audit.mechanics.load_sources()))
    check(
        "the rows `composed` grants, the scratch directory and the agent socket among them, are listed",
        any(s["function"] == "composed" and "scratch" in s["text"] for s in rows)
        and any(s["function"] == "composed" and "socket" in s["text"] for s in rows)
        and any("socket_path" in s["text"] for s in rows),
        str([(s["line"], s["text"]) for s in rows]),
    )


def test_the_identity_lane_starts_from_the_questions_put_to_a_deny_rule():
    """The identity lane lists every question put to a deny rule about a spelled path, so a walk that
    asks it of each entry is opened, and the prompt it is given names the list."""
    sources = {
        Path("crates/agent/src/workspace.rs"): [
            "impl Workspace {",
            "    fn list(&self, policy: &Policy) {",
            "        let denied = |path: &str| policy.read_is_denied(path);",
            "        // read_is_denied is named in this comment only",
            "        let any = policy.any_read_is_denied();",
            "    }",
            "}",
            "#[cfg(test)]",
            "mod tests {",
            "    fn t() { policy.read_is_denied(x); }",
            "}",
        ],
        Path("crates/core/src/policy.rs"): [
            "impl Policy {",
            "    pub fn read_is_denied(&self, path: &str) -> bool { self.rules(path) }",
            "}",
        ],
        Path("crates/agent/tests/workspace.rs"): ["fn t() { policy.read_is_denied(x); }"],
    }
    found = [(s["path"], s["line"], s["function"]) for s in audit.deny_judgements(sources)]
    check(
        "a question asked in code that ships is listed with the function it is in, and the definition, "
        "a longer name that ends in it, a comment, a test module and a test file are not",
        found == [("crates/agent/src/workspace.rs", 3, "list")],
        str(found),
    )

    def build():
        specs = audit.load_specs()
        real = audit.mechanics.load_sources()
        return (
            audit.deny_judgements(real),
            audit.build_lane(
                "identity",
                audit.surface(real, specs),
                specs,
                Path("results.json"),
                Path("surface.json"),
                Path("tracker.txt"),
            ),
        )

    real, prompt = with_cwd(ROOT, build)
    check(
        "the tree's own questions are listed, the workspace's among them",
        any(s["path"] == "crates/agent/src/workspace.rs" for s in real),
        str(sorted({s["path"] for s in real})),
    )
    shown = with_cwd(ROOT, lambda: audit.as_files(real))
    check(
        "the identity prompt gives the deny list with its count, apart from the identity list, and asks "
        "what a walk judges each entry by",
        f"{len(real)} questions put to a deny rule" in prompt
        and f"every entry it meets, so open the callers of these as well as the functions named:\n\n{shown}\n"
        in prompt
        and "{deny_files}" not in prompt
        and "what the question is asked about" in prompt,
        prompt[prompt.find("questions put to") - 20 : prompt.find("questions put to") + 500],
    )


def test_the_output_lane_lists_a_write_only_where_its_arguments_are_not_all_literals():
    """The output lane lists a write only where its arguments are not all literals.

    A write of literals cannot carry anything somebody else chose, so listing it only buries the
    others. Each case below is one shape the filter has to tell apart.
    """
    def listed(text, path="crates/cli/src/x.rs"):
        sources = {Path(path): text.split("\n")}
        return [(s["line"], tuple(s["kinds"])) for s in audit.output_sites(sources)]

    check(
        "a macro of literals is left out and one filling a placeholder is listed",
        listed(
            'fn f() {\n'
            '    println!("plain");\n'
            '    println!("a {x}");\n'
            '    println!("a {}", x);\n'
            '    println!("{{kept}} braces");\n'
            '    println!("one, two");\n'
            '}'
        )
        == [(3, ("println",)), (4, ("println",))],
        str(listed('fn f() {\n    println!("a {x}");\n}')),
    )
    check(
        "the destination of a write is not its text",
        listed('fn f() {\n    writeln!(out, "plain");\n    writeln!(out, "{}", x);\n    write!(out, "{y}");\n}')
        == [(3, ("writeln",)), (4, ("write",))],
    )
    check(
        "a call that runs over several lines is read to its closing parenthesis",
        listed('fn f() {\n    eprintln!(\n        "a {}",\n        y\n    );\n}') == [(2, ("eprintln",))],
        str(listed('fn f() {\n    eprintln!(\n        "a {}",\n        y\n    );\n}')),
    )
    check(
        "a message with no values or only literal ones is left out, and one given a variable is listed",
        listed(
            'fn f() {\n'
            '    t!(heading);\n'
            '    t!(heading, name = "fixed");\n'
            '    t!(heading, name = value);\n'
            '}'
        )
        == [(4, ("t",))],
    )
    check(
        "a say of a literal or of a message with no values is left out, and one of a built string is listed",
        listed(
            'fn f() {\n'
            '    asking.say("fixed");\n'
            '    asking.say(t!(heading));\n'
            '    asking.say(&t!(held, status = status));\n'
            '    asking.say(&format!("{n}"));\n'
            '}'
        )
        == [(4, ("say", "t")), (5, ("say",))],
        str(listed('fn f() {\n    asking.say(&t!(held, status = status));\n}')),
    )
    check(
        "a string joined to others is not a literal, however it begins and ends",
        listed('fn f() {\n    asking.say("x".to_string() + &y + "z");\n}') == [(2, ("say",))]
        and listed('fn f() {\n    println!("{}", "a".to_owned() + &t + "b");\n}') == [(2, ("println",))],
        str(listed('fn f() {\n    asking.say("x".to_string() + &y + "z");\n}')),
    )
    check(
        "a write in a test module, outside the three crates, or in a comment is not listed",
        listed('#[cfg(test)]\nmod tests {\n    fn t() { println!("{x}"); }\n}\n') == []
        and listed('fn f() { println!("{x}"); }', "crates/agent/src/x.rs") == []
        and listed('fn f() {\n    // println!("{x}");\n}') == [],
    )
    check(
        "a closing parenthesis inside a string or a character literal does not end a call",
        listed('fn f() { println!("a) {}", \')\'); }') == [(1, ("println",))],
        str(listed('fn f() { println!("a) {}", \')\'); }')),
    )


def test_the_window_list_holds_the_places_a_string_becomes_markup():
    """The window's start list holds the places a string becomes markup or preformatted text."""
    where = in_tree(
        {
            "ui/src/renderer/Card.tsx": (
                "export function Card() {\n"
                "  return <pre>{text}</pre>\n"
                "}\n"
                "const Other = () => <div dangerouslySetInnerHTML={{ __html: html }} />\n"
                "// innerHTML is named in this comment only\n"
            ),
            "ui/src/renderer/Card.test.tsx": "<pre>{fixture}</pre>\n",
            "ui/src/renderer/Plain.tsx": "export function Plain() { return <p>{text}</p> }\n",
        }
    )
    found = [(s["path"], s["line"], s["function"]) for s in with_cwd(where, audit.window_sinks)]
    check(
        "a pre and an innerHTML are listed with their component, and a comment, a test and a plain "
        "element are not",
        found == [("ui/src/renderer/Card.tsx", 2, "Card"), ("ui/src/renderer/Card.tsx", 4, "Other")],
        str(found),
    )


def test_the_checkout_lane_starts_from_the_layers():
    """The lane for what a clone can set starts from where layers merge and what a file can set."""
    sources = {
        Path("crates/config/src/settings.rs"): [
            "pub struct Settings {",
            "    env: BTreeMap<String, String>,",
            "    /// A doc line.",
            "    model: Option<String>,",
            "    model_ignored: Vec<PathBuf>,",
            "    unread: Vec<(PathBuf, String)>,",
            "    layers: Vec<PathBuf>,",
            "    run_network: Option<Network>,",
            "}",
            "    unrelated: u32,",
        ]
    }
    check(
        "the fields a file can set are listed and the ones that record what was ignored are not",
        audit.settings_fields(sources) == ["env", "model", "run_network"],
        str(audit.settings_fields(sources)),
    )

    def real():
        found = audit.surface(audit.mechanics.load_sources(), audit.load_specs())
        return found

    found = with_cwd(ROOT, real)
    named = {(Path(s["path"]).name, s["function"]) for s in found["layered"]}
    check(
        "the merge, the unread keys, the permission texts and the names a file set are on the lane's list",
        {
            ("settings.rs", "layered_since"),
            ("settings.rs", "unread_keys"),
            ("settings.rs", "rule_texts"),
            ("settings.rs", "names"),
        }
        <= named,
        str(sorted(named)),
    )
    check(
        "the two clauses that say which layer answers for which name are on it, and so are the fields",
        [one.split(" ")[1].rstrip(":") for one in found["layer_clauses"]] == ["BACKEND-24", "BACKEND-33"]
        and {"env", "permissions", "model"} <= set(found["settings_fields"])
        and "model_ignored" not in found["settings_fields"],
        str(found["layer_clauses"]),
    )


def test_the_prompt_lane_reaches_the_line_interface_and_the_replay():
    """The prompt lane starts from the questions the full-screen interface's table never draws.

    The line interface, the editor protocol and the background host's replay buffer put the same
    questions with a layout of their own, and the lane used to turn them away in its last sentence. A
    prompt whose drawn text is not the bytes a yes releases is the finding that sentence excluded.
    """
    def real():
        specs = audit.load_specs()
        found = audit.surface(audit.mechanics.load_sources(), specs)
        return found, audit.build_lane("shown-before-answered", found, specs, Path("results.json"))

    found, text = with_cwd(ROOT, real)
    named = {(Path(s["path"]).name, s["function"]) for s in found["other_prompts"]}
    check(
        "the line interface's put and confirmations, the approval crate's lines, the editor protocol's ask "
        "and the host's emit and attach are on the lane's list",
        {
            ("plain.rs", "put"),
            ("plain.rs", "confirm_vetted_read"),
            ("lib.rs", "vet_lines"),
            ("acp.rs", "ask"),
            ("host.rs", "emit"),
            ("host.rs", "attach"),
        }
        <= named,
        str(sorted(named)),
    )
    check(
        "the lane no longer turns the line interface away and does not exclude a drawn text that "
        "differs from the released bytes",
        "The line interface, which puts the same questions with a layout of its own" not in text
        and "Whether the drawn text is the bytes a yes releases" in text
        and "Whether a question survives being replayed" in text
        and "crates/cli/src/host.rs" in text,
    )


def test_the_tooling_lane_starts_from_the_scripts():
    """The tooling lane lists the scripts, the hooks, the places they start a process and the git
    configuration a make target sets."""
    def real():
        return audit.surface(audit.mechanics.load_sources(), audit.load_specs())

    found = with_cwd(ROOT, real)
    files = set(found["tool_files"])
    check(
        "the scripts, the contrib checks and the hooks are listed",
        {"agents/pr_worktree.py", "contrib/affected-checks.py", ".githooks/pre-commit"} <= files
        and "agents/skills/pr-fix/pr-fix.py" in files,
        str(sorted(files)[:6]),
    )
    check(
        "the skill and agent definitions and the settings they run under are listed",
        {"agents/settings.json", "agents/agents/issue-poster.md", "agents/skills/pr-fix/SKILL.md"}
        <= set(found["tool_definitions"]),
        str(found["tool_definitions"]),
    )
    starts = {(s["path"], s["function"]) for s in found["process_starts"]}
    check(
        "a script's process start is listed with the function it is in",
        ("agents/issue_helper.py", "gh") in starts,
        str(sorted(starts)[:5]),
    )
    check(
        "the make target that sets git's hooks path is listed",
        any(s["function"] == "hooks" and "core.hooksPath" in s["text"] for s in found["make_recipes"])
        and not any("check-spec" in (s["function"] or "") for s in found["make_recipes"]),
        str(found["make_recipes"]),
    )

    where = in_tree(
        {
            "Makefile": (
                "VERSION = 1\n\n"
                ".PHONY: hooks build\n"
                "hooks:\n"
                "\tgit config core.hooksPath .githooks\n"
                "build:\n"
                "\tgit -c user.name=x commit\n"
                "\tcargo build\n"
            )
        }
    )
    recipes = [(s["function"], s["line"]) for s in with_cwd(where, audit.make_recipes)]
    check(
        "a git configuration line is listed under its own target and a cargo line is not",
        recipes == [("hooks", 5), ("build", 7)],
        str(recipes),
    )

    where = in_tree(
        {
            "Makefile": (
                "hooks:\n"
                "\tgit \\\n"
                "\t  config core.hooksPath .githooks\n"
                "build:\n"
                "\tcargo build \\\n"
                "\t  --release\n"
            ),
            "agents/aliased.py": (
                "from subprocess import run\n"
                "import subprocess as sp\n"
                "def go():\n"
                "    run(['git', 'push'])\n"
            ),
        }
    )
    recipes = [(s["function"], s["line"]) for s in with_cwd(where, audit.make_recipes)]
    check(
        "a recipe command written over several lines is read whole and listed from its first line",
        recipes == [("hooks", 2)],
        str(recipes),
    )
    starts = [s["line"] for s in with_cwd(where, lambda: audit.process_starts(["agents/aliased.py"]))]
    check(
        "a script that imports a process function by name or under another name is listed",
        starts == [1, 2],
        str(starts),
    )

    where = in_tree(
        {
            "Makefile": (
                "hooks:\n"
                "\t@echo " + "x" * 300 + " && \\\n"
                "\t  git config core.hooksPath /somewhere\n"
            )
        }
    )
    texts = [s["text"] for s in with_cwd(where, audit.make_recipes)]
    check(
        "a git configuration set late in a long recipe is still in the listed text",
        len(texts) == 1 and "core.hooksPath /somewhere" in texts[0],
        str(texts),
    )


def test_the_new_lanes_are_run_for_the_files_they_read():
    """A branch's diff selects the lanes that read what it touched."""
    class Governing:
        id = "PROMPT"
        rel = "docs/specs/asking.md"
        governs = ["crates/tui/src/asks.rs"]

    found = {
        "identity": [{"path": "crates/agent/src/workspace.rs"}],
        "deny_judgements": [{"path": "crates/agent/src/walker.rs"}],
    }
    was = audit.mechanics.changed_files
    try:
        for touched, lane, shown in [
            ("crates/agent/src/workspace.rs", "identity", found),
            ("crates/agent/src/confine.rs", "identity", found),
            ("crates/agent/src/walker.rs", "identity", found),
            ("crates/config/src/settings.rs", "checkout-controls", found),
            ("docs/specs/backends.md", "checkout-controls", found),
            ("crates/cli/src/doctor.rs", "output-sinks", found),
            ("ui/src/renderer/components/Card.tsx", "output-sinks", found),
            ("agents/pr_worktree.py", "tooling", found),
            ("contrib/check-source.py", "tooling", found),
            (".githooks/pre-push", "tooling", found),
            ("Makefile", "tooling", found),
        ]:
            audit.mechanics.changed_files = lambda base, touched=touched: iter([touched])
            lanes = audit.changed_lanes("base", [Governing()], shown)
            check(f"a branch that touches {touched} runs the {lane} lane", lane in lanes, str(lanes))

        audit.mechanics.changed_files = lambda base: iter(["crates/tui/src/asks.rs"])
        lanes = audit.changed_lanes("base", [Governing()], found)
        check(
            "a Rust file that makes no decision on a name does not run the identity lane",
            "identity" not in lanes and "tooling" not in lanes and "checkout-controls" not in lanes,
            str(lanes),
        )

        audit.mechanics.changed_files = lambda base: iter(["crates/tui/src/asks.rs"])
        lanes = audit.changed_lanes("base", [Governing()])
        check(
            "a Rust file runs the identity lane where there is no surface to ask",
            "identity" in lanes,
            str(lanes),
        )
        audit.mechanics.changed_files = lambda base: iter(["docs/specs/asking.md"])
        lanes = audit.changed_lanes("base", [Governing()])
        check(
            "a file that is not Rust does not run the identity lane where there is no surface to ask",
            "identity" not in lanes,
            str(lanes),
        )
    finally:
        audit.mechanics.changed_files = was


def test_the_tracker_is_written_for_the_lanes():
    """The lanes are given the titles of the security issues, as text and never as anything else.

    A title is somebody's text and a lane reads the file it goes into, so a control or an invisible
    character is taken out, and a tracker that could not be read says so instead of passing as empty.
    """
    with tempfile.TemporaryDirectory() as tmp:
        work = Path(tmp)
        listed = work / "issues.json"
        listed.write_text(
            json.dumps(
                [
                    {"number": 2900, "title": "b: a second\ndefect\u202e\u200b", "state": "CLOSED"},
                    {"number": 2821, "title": "SANDBOX-18: a first defect", "state": "OPEN"},
                    {"title": "no number is not an issue", "state": "OPEN"},
                ]
            ),
            encoding="utf-8",
        )
        entries, problem = audit.tracker_entries("brave/bravebot", str(listed))
        written = audit.write_tracker(work, entries, problem).read_text(encoding="utf-8")
        check(
            "the titles are written one to a line, by number, with newlines and invisible characters gone",
            written == "#2821 open: SANDBOX-18: a first defect\n#2900 closed: b: a second defect\n"
            and problem is None,
            repr(written),
        )

        entries, problem = audit.tracker_entries("brave/bravebot", str(work / "missing.json"))
        written = audit.write_tracker(work, entries, problem).read_text(encoding="utf-8")
        check(
            "an unreadable list is written as that, and not as a tracker with nothing in it",
            entries == [] and written.startswith("(the tracker could not be read:"),
            repr(written),
        )

        listed.write_text(
            json.dumps([{"number": at, "title": "t", "state": "OPEN"} for at in (1, 2)]),
            encoding="utf-8",
        )
        was_limit = audit.TRACKER_LIMIT
        try:
            audit.TRACKER_LIMIT = 2
            entries, problem = audit.tracker_entries("brave/bravebot", str(listed))
            written = audit.write_tracker(work, entries, problem).read_text(encoding="utf-8")
        finally:
            audit.TRACKER_LIMIT = was_limit
        check(
            "a list that reached the cap says older issues are missing",
            written.startswith("(only the newest 2 security issues were listed"),
            repr(written),
        )

        listed.write_text("[]", encoding="utf-8")
        entries, problem = audit.tracker_entries("brave/bravebot", str(listed))
        written = audit.write_tracker(work, entries, problem).read_text(encoding="utf-8")
        check(
            "a tracker with no security issue says so",
            written == "(the tracker holds no security issue)\n",
            repr(written),
        )

        was = audit.subprocess.run
        try:
            def refuse(*args, **kwargs):
                raise FileNotFoundError("gh")

            audit.subprocess.run = refuse
            entries, problem = audit.tracker_entries("brave/bravebot")
            check(
                "a machine with no gh gets a lane prompt run rather than a failed audit",
                entries == [] and "gh could not be run" in (problem or ""),
                str(problem),
            )
            asked = []

            def answer(command, **kwargs):
                asked.append(command)
                return subprocess.CompletedProcess(command, 0, '[{"number": 5, "title": "t", "state": "OPEN"}]', "")

            audit.subprocess.run = answer
            entries, problem = audit.tracker_entries("brave/bravebot")
            check(
                "the tracker is asked for the issues labelled security, open and closed, read only",
                entries == [(5, "open", "t")]
                and asked[0][:3] == ["gh", "issue", "list"]
                and "security" in asked[0]
                and asked[0][asked[0].index("--state") + 1] == "all"
                and not {"create", "comment", "edit", "close"} & set(asked[0]),
                str(asked),
            )
        finally:
            audit.subprocess.run = was


def test_no_name_in_the_enumerator_is_bound_twice():
    """A module-level name defined twice means the later one answers for both.

    `PLACEHOLDER` was a pattern for a format hole in one place and for an empty table cell in
    another, and the second silently replaced the first: a write of a filled-in format string was
    read as a literal and left out of a lane's start list while every test of the filter was green.
    """
    import ast

    tree = ast.parse((HERE / "security-audit.py").read_text(encoding="utf-8"))
    seen, twice = {}, []
    for node in tree.body:
        bound = (
            [target.id for target in node.targets if isinstance(target, ast.Name)]
            if isinstance(node, ast.Assign)
            else [node.name]
            if isinstance(node, (ast.FunctionDef, ast.ClassDef))
            else []
        )
        for name in bound:
            if name in seen:
                twice.append(f"{name} (lines {seen[name]} and {node.lineno})")
            seen.setdefault(name, node.lineno)
    check("every module-level name in security-audit.py is bound once", not twice, ", ".join(twice))


def test_every_lane_has_its_file_and_its_row():
    """A lane in `LANES` has a prompt file and a row in the skill's table, and the table's heading
    counts them.

    The count in the heading and the description was ten while the table held ten, and only the
    table moved when a lane was added.
    """
    words = {
        10: "ten", 11: "eleven", 12: "twelve", 13: "thirteen", 14: "fourteen", 15: "fifteen",
        16: "sixteen", 17: "seventeen", 18: "eighteen", 19: "nineteen", 20: "twenty",
    }
    skill = (HERE / "SKILL.md").read_text(encoding="utf-8")
    missing = [
        name
        for name in audit.LANES
        if not (HERE / "lanes" / f"{name}.md").is_file() or f"| `{name}` |" not in skill
    ]
    check("every lane has a prompt file and a row in the skill's table", not missing, str(missing))
    word = words.get(len(audit.LANES), str(len(audit.LANES)))
    check(
        "the skill counts the lanes it has",
        f"## The {word} lanes" in skill and f"all {word} lanes" in skill and f"in {word} lanes" in skill,
        word,
    )


def test_the_exemption_stops_at_the_person_s_own_configuration():
    """The preamble and the verifier read "this program's own configuration" as the person's, so a
    candidate about a clone's files is not dropped under it."""
    def build():
        specs = audit.load_specs()
        found = audit.surface(audit.mechanics.load_sources(), specs)
        return audit.build_lane("checkout-controls", found, specs, Path("results.json"), None, Path("tracker.txt"))

    text = with_cwd(ROOT, build)
    check(
        "the lane prompt limits the exemption to the person's own settings, command line and environment",
        "covers the person's own configuration" in text and "It does not cover a checkout's" in text,
    )
    check(
        "a lane leaves out a defect only an open issue holds, and reports one a closed issue names",
        "A title with state\n`open`" in text and "A `closed` issue was fixed or declined" in text,
    )
    verifier = verifying.build_prompt({"summary": "s", "place": "crates/config/src/x.rs:1", "lane": "checkout-controls"}, Path("r.json"))
    check(
        "the verifier is told the same and is not made to drop a candidate whose bytes never reach the planner",
        "It does not cover a checkout's" in verifier
        and "A candidate from those lanes is not dropped because no byte reaches the planner" in verifier,
    )


def candidate_for(lane, summary, place, clause=None):
    return {"lane": lane, "summary": summary, "place": place, **({"clause": clause} if clause else {})}


def test_candidates_that_make_one_claim_are_checked_together():
    """Two lanes that return one defect get one verifier, and two defects stay apart.

    Grouping saves a verifier only when it joins duplicates. It costs a defect when it joins two
    different ones, so each case below is one the grouping has to get right in a different direction.
    """
    same_a = candidate_for(
        "identity",
        "the scratch directory is resolved once and a stage can relink it, so the next row names a credential directory",
        "crates/agent/src/confine.rs:156 in new",
    )
    same_b = candidate_for(
        "checkout-controls",
        "a stage can relink the scratch directory and the resolved row then names a credential directory",
        "crates/agent/src/confine.rs:1289 in composed",
    )
    other_claim = candidate_for(
        "identity",
        "the proxy socket is connected by name for every request and an unconfined helper follows a replaced link",
        "crates/agent/src/confine.rs:1848 in prepared",
    )
    other_file = candidate_for(
        "tooling",
        "the scratch directory is resolved once and a stage can relink it, so the next row names a credential directory",
        "agents/pr_worktree.py:259 in pull",
    )
    groups = verifying.group_candidates([same_a, other_claim, same_b, other_file])
    check(
        "two lanes' summaries of one defect in one file are one group, kept in the order returned",
        [[one["lane"] for one in group] for group in groups]
        == [["identity", "checkout-controls"], ["identity"], ["tooling"]],
        str([[one["summary"][:20] for one in group] for group in groups]),
    )
    check(
        "the same words about another file, and another claim about the same file, are not joined",
        other_file not in groups[0] and other_claim not in groups[0],
    )

    # What two lanes returned for the write prompt's cap, and for two other defects in the same file,
    # as the lanes worded them.
    cap_a = candidate_for(
        "output-sinks",
        "The line-oriented write prompt (--plain and ACP) shows the first 40 changed rows of the body and"
        " replaces the rest with a count, although a yes writes every row, so for a processor-produced "
        "body the rows after the 40th are approved unseen, which the code's own comment on output and vet"
        " prompts calls 'approved unread'.",
        "crates/approval/src/lib.rs:147 in write_lines (if changed == MOST_CONTENT_LINES { left_out += 1;"
        " continue; }, then the transcript_more_lines row at :160)",
        "PROMPT-1",
    )
    cap_b = candidate_for(
        "shown-before-answered",
        "The line interface and the editor protocol cut a write's change at 40 rows and, where the diff "
        "is not exact, show none of it, so a person is asked to approve a file body they cannot see and "
        "have no way to scroll to, unlike the output, vetting and message prompts that show every row.",
        "crates/approval/src/lib.rs:144 in write_lines (the cap at 147-151, the early return for an "
        "inexact diff at 135-142)",
        "PROMPT-4",
    )
    other_in_the_function = candidate_for(
        "output-sinks",
        "The write approval draws a processor-produced body, and a destination name taken from a "
        "quarantined listing, with only control characters replaced, so tag characters, zero-width "
        "characters and bidirectional controls in what a yes writes draw as nothing in the terminal and "
        "are reordered by the browser engine in the desktop window; the person approves bytes the card "
        "did not show.",
        "crates/approval/src/lib.rs:153 in write_lines (+/-/context rows through printable at :41); the "
        "same rows reach crates/tui/src/confirm/write.rs:389 in draw (marked_rows, then "
        "crates/tui/src/render.rs:152 printable) and ui/src/renderer/components/Diff.tsx:31 in Diff",
        "PROMPT-1",
    )
    far_in_the_file = candidate_for(
        "shown-before-answered",
        "The line interface and the editor protocol draw the vouch prompt's twenty-line head as if it "
        "were the file and drop the check's sentence, although the check read the whole file and the "
        "answer is a standing trust rule, so a person vouches for a path on a head they are not told is "
        "only a head.",
        "crates/approval/src/lib.rs:371 in vouch_lines (called from crates/cli/src/plain.rs:1477 "
        "confirm_vouch and crates/ui-bridge/src/acp.rs:1068)",
        "PROMPT-1",
    )
    check(
        "two lanes' wording of one cap, naming lines three apart in one function, is one claim",
        verifying.same_claim(cap_a, cap_b) and verifying.same_claim(cap_b, cap_a),
    )
    check(
        "a different defect a few lines away, and another a long way off in the same file, are not joined",
        not verifying.same_claim(cap_a, other_in_the_function)
        and not verifying.same_claim(cap_b, other_in_the_function)
        and not verifying.same_claim(cap_b, far_in_the_file),
    )

    near = candidate_for("a", "reads a value", "crates/x.rs:90 in f", "LABEL-4")
    near_again = candidate_for("b", "writes something else entirely", "crates/x.rs:95 in g", "LABEL-4")
    far_labelled = candidate_for("d", "writes something else entirely", "crates/x.rs:900 in h", "LABEL-4")
    check(
        "one clause joins nothing: lines a few apart or a long way apart, the words still have to agree",
        not verifying.same_claim(near, near_again) and not verifying.same_claim(near, far_labelled),
    )
    check(
        "two lanes citing different clauses for one defect still make one claim",
        verifying.same_claim(cap_a, cap_b) and cap_a["clause"] != cap_b["clause"],
    )
    check(
        "a candidate with no place is never grouped",
        not verifying.same_claim({"summary": same_a["summary"]}, {"summary": same_a["summary"]}),
    )
    chain_a = candidate_for("a", "alpha bravo charlie delta echo foxtrot", "crates/x.rs:1 in f")
    chain_b = candidate_for("b", "alpha bravo charlie xray yankee zulu", "crates/x.rs:2 in f")
    chain_c = candidate_for("c", "xray yankee zulu quebec romeo sierra", "crates/x.rs:3 in f")
    check(
        "a chain of near duplicates is one group",
        len(verifying.group_candidates([chain_a, chain_c, chain_b])) == 1,
        str(verifying.group_candidates([chain_a, chain_c, chain_b])),
    )
    short_a = candidate_for("a", "reads value", "crates/x.rs:1 in f")
    short_b = candidate_for("b", "reads other value", "crates/x.rs:400 in g")
    check(
        "two short summaries that share a word or two are not one claim, whatever share of the shorter that is",
        not verifying.same_claim(short_a, short_b),
    )
    same_five = [
        candidate_for(f"lane{at}", "alpha bravo charlie delta echo foxtrot", "crates/x.rs:1 in f")
        for at in range(5)
    ]
    sizes = [len(group) for group in verifying.group_candidates(same_five)]
    check(
        "a verifier is given no more than the limit, and the rest of a long chain are checked together apart",
        sizes == [verifying.GROUP_LIMIT, 5 - verifying.GROUP_LIMIT],
        str(sizes),
    )


def test_a_group_is_one_verifier_and_the_others_are_in_its_prompt():
    """The manifest keeps the grouped candidates and the prompt hands the verifier all of them."""
    with tempfile.TemporaryDirectory() as tmp:
        work = Path(tmp)
        one = candidate_for(
            "identity",
            "the scratch directory is resolved once and a stage can relink it",
            "crates/agent/src/confine.rs:156 in new",
        )
        two = candidate_for(
            "output-sinks",
            "a stage can relink the scratch directory once it is resolved",
            "crates/agent/src/confine.rs:1289 in composed",
        )
        alone = candidate_for("tooling", "a hook runs from the branch", "agents/pr_worktree.py:534 in rebase_onto")
        results = {"identity": [one], "output-sinks": [two], "tooling": [alone]}
        lanes = []
        for name, found in results.items():
            path = work / f"{name}_results.json"
            path.write_text(
                json.dumps(
                    {
                        "candidates": found,
                        "already_tracked": [{"issue": 2821, "claim": "a scratch relink"}] if name == "tooling" else [],
                    }
                ),
                encoding="utf-8",
            )
            lanes.append({"lane": name, "prompt_file": str(work / f"{name}_prompt.md"), "results_file": str(path)})
        collect.workdir.write_manifest(work, {"lanes": lanes})
        argv = sys.argv
        sys.argv = ["verify-findings.py", "--work-dir", str(work)]
        out = io.StringIO()
        try:
            with redirect_stdout(out):
                status = verifying.main()
        finally:
            sys.argv = argv
        manifest = collect.read_manifest(work)
        pairs = manifest.get("verifications", [])
        check(
            "three candidates, two of one defect, are two verifications",
            status == 0 and len(pairs) == 2 and [len(p["also"]) for p in pairs] == [1, 0],
            str(pairs),
        )
        if len(pairs) != 2:
            return
        grouped = Path(pairs[0]["prompt_file"]).read_text(encoding="utf-8")
        alone_prompt = Path(pairs[1]["prompt_file"]).read_text(encoding="utf-8")
        check(
            "the group's prompt carries the other candidate, numbered, and asks which are the same defect",
            "a stage can relink the scratch directory once it is resolved" in grouped
            and '"number": 2' in grouped
            and "`grouped`" in grouped
            and "`separate`" in grouped
            and "decides which form of question 1 applies to it" in grouped
            and not SURVIVING.search(grouped),
        )
        check(
            "a candidate alone is given the prompt it always was, with nothing about a group",
            "Candidates grouped with this one" not in alone_prompt and not SURVIVING.search(alone_prompt),
        )
        check(
            "the output says which verifications are checked together and what the tracker already held",
            "checked together with 1 more, from: output-sinks" in out.getvalue()
            and "#2821 (tooling)" in out.getvalue()
            and manifest["already_tracked"] == [{"lane": "tooling", "issue": 2821, "claim": "a scratch relink"}],
            out.getvalue(),
        )


def verification(candidate, others=()):
    return {
        "id": 1,
        "lane": candidate["lane"],
        "summary": candidate["summary"],
        "candidate": candidate,
        "also": [
            {"number": at, "lane": one["lane"], "summary": one["summary"], "candidate": one}
            for at, one in enumerate(others, start=2)
        ],
    }


def test_a_verdict_covers_the_candidates_it_says_are_the_same_defect():
    """The collector files one finding for a defect two lanes returned, and loses none.

    A verdict is the first candidate's. A candidate it does not account for was never decided, so it
    is unverified and not filed, and one it calls a different defect is filed under its own verdict.
    """
    first = candidate_for("identity", "the first claim", "crates/a.rs:1 in f")
    second = candidate_for("checkout-controls", "the same claim again", "crates/a.rs:2 in f")
    third = candidate_for("output-sinks", "a different claim in the same file", "crates/a.rs:9 in g")

    def settled(verdict, *others):
        with tempfile.TemporaryDirectory() as tmp:
            entry = verification(first, others)
            entry["results_file"] = str(Path(tmp) / "r.json")
            Path(entry["results_file"]).write_text(json.dumps(verdict), encoding="utf-8")
            return collect.load_verdicts({"verifications": [entry]})

    findings, dropped, unclear, missing = settled(
        {"verdict": "CONFIRMED", "impact": "high", "grouped": [{"number": 2, "same_defect": True}]},
        second,
    )
    check(
        "a defect two lanes returned is one finding that names both lanes",
        len(findings) == 1
        and findings[0]["lane"] == "identity"
        and findings[0].get("also_reported_by") == ["checkout-controls"]
        and not (dropped or unclear or missing),
        str(findings),
    )

    findings, dropped, unclear, missing = settled(
        {
            "verdict": "CONFIRMED",
            "impact": "high",
            "grouped": [{"number": 2, "same_defect": True}, {"number": 3, "same_defect": False}],
            "separate": {"3": {"verdict": "CONFIRMED", "impact": "low", "reason": "its own"}},
        },
        second,
        third,
    )
    check(
        "a candidate the verifier calls a different defect is filed under the verdict it gave that one",
        [(f["lane"], f["impact"]) for f in findings] == [("identity", "high"), ("output-sinks", "low")]
        and findings[0].get("also_reported_by") == ["checkout-controls"]
        and "also_reported_by" not in findings[1],
        str(findings),
    )

    findings, dropped, unclear, missing = settled(
        {"verdict": "CONFIRMED", "impact": "high", "grouped": [{"number": 2, "same_defect": True}]},
        second,
        third,
    )
    check(
        "a candidate the verdict does not account for is unverified and not filed",
        [f["lane"] for f in findings] == ["identity"]
        and [m["lane"] for m in missing] == ["output-sinks"],
        str((findings, missing)),
    )

    findings, dropped, unclear, missing = settled(
        {
            "verdict": "CONFIRMED",
            "impact": "high",
            "grouped": [{"number": 3, "same_defect": False}],
            "separate": {},
        },
        second,
        third,
    )
    check(
        "a candidate called a different defect with no verdict of its own is unverified, not dropped",
        [f["lane"] for f in findings] == ["identity"]
        and sorted(m["lane"] for m in missing) == ["checkout-controls", "output-sinks"],
        str((findings, missing)),
    )

    findings, dropped, unclear, missing = settled(
        {"verdict": "CONFIRMED", "impact": "high", "grouped": [{"number": "2", "same_defect": True}]},
        second,
    )
    check(
        "a number written as text is read as the number",
        len(findings) == 1 and findings[0].get("also_reported_by") == ["checkout-controls"] and not missing,
        str((findings, missing)),
    )

    far = {**second, "place": "crates/a.rs:400 in far", "evidence": ["crates/a.rs:401 the other site"]}
    findings, dropped, unclear, missing = settled(
        {"verdict": "CONFIRMED", "impact": "high", "grouped": [{"number": 2, "same_defect": True}]},
        far,
    )
    check(
        "a site only the second lane named is a site of the finding",
        "crates/a.rs:400 in far" in findings[0]["evidence"]
        and "crates/a.rs:401 the other site" in findings[0]["evidence"],
        str(findings[0]["evidence"]),
    )

    findings, dropped, unclear, missing = settled(
        {
            "verdict": "CONFIRMED",
            "impact": "high",
            "grouped": [{"number": 2, "same_defect": False}],
            "separate": {"2": {"reason": "no verdict word"}},
        },
        third,
    )
    check(
        "a separate verdict that says none of confirmed, dropped or unclear leaves the candidate unverified",
        [f["lane"] for f in findings] == ["identity"]
        and [m["lane"] for m in missing] == ["output-sinks"]
        and not dropped,
        str((findings, dropped, missing)),
    )

    findings, dropped, unclear, missing = settled(
        {
            "verdict": "DROPPED",
            "reason": "question 3",
            "grouped": [{"number": 2, "same_defect": True}],
        },
        second,
    )
    check(
        "a dropped defect is reported once for each lane that returned it",
        not findings and sorted(d["lane"] for d in dropped) == ["checkout-controls", "identity"],
        str(dropped),
    )

    findings, dropped, unclear, missing = settled(
        {"verdict": "CONFIRMED", "impact": "low"}, second
    )
    check(
        "a verdict with no grouped list leaves the others unverified",
        [f["lane"] for f in findings] == ["identity"] and [m["lane"] for m in missing] == ["checkout-controls"],
        str((findings, missing)),
    )

    with tempfile.TemporaryDirectory() as tmp:
        work = Path(tmp)
        entry = verification(first, [second])
        entry["results_file"] = str(work / "r.json")
        collect.workdir.write_manifest(work, {"lanes": [], "verifications": [entry]})
        argv = sys.argv
        sys.argv = ["collect-findings.py", "--work-dir", str(work), "--json"]
        out = io.StringIO()
        try:
            with redirect_stdout(out):
                collect.main()
        finally:
            sys.argv = argv
        said = json.loads(out.getvalue())
    check(
        "the machine-readable report lists the candidates nobody verified, the lane that returned each named",
        [one["lane"] for one in said.get("unverified", [])] == ["identity", "checkout-controls"],
        str(said.get("unverified")),
    )

    finding = collect.merge(first, {"verdict": "CONFIRMED", "impact": "high"})
    finding["also_reported_by"] = ["tooling"]
    rendered = collect.render(
        [finding], [], [], [], [], {"lanes": [], "verifications": [verification(first, [second])]}
    )
    check(
        "the report lists every lane that returned a confirmed defect and counts every candidate",
        "[identity, tooling]" in rendered and "2 candidates" in rendered,
        rendered,
    )


def test_a_draft_names_every_lane_that_found_it():
    """A draft names every lane that reported the defect, not only the one whose candidate was kept.

    A reader of the issue judges from that list how many independent readings the finding has had.
    A draft that named one lane would report a defect that two lanes reached from different starting
    points as the claim of a single lane.
    """
    one = {"lane": "identity"}
    two = {"lane": "identity", "also_reported_by": ["checkout-controls", "output-sinks"]}
    check(
        "a body names the one lane that read the code, and every lane where there were several",
        "the `identity` lane read the code" in drafts.readers_of(one)
        and "`identity`, `checkout-controls` and `output-sinks` lanes each read the code"
        in drafts.readers_of(two),
        drafts.readers_of(two),
    )
def test_a_mark_is_only_in_a_file_that_may_start_a_program():
    mark = '#[allow(clippy::disallowed_methods, reason = "named")]\nfn start() {}\n'

    found = audit_starts({"clippy.toml": LISTED, "crates/cli/src/identity.rs": mark})
    check(
        "a mark in a production file the list does not name is an error naming the file and line",
        kinds(found) == ["process-start-mark-outside-start"]
        and found[0]["severity"] == audit.ERROR
        and "crates/cli/src/identity.rs:1" in found[0]["summary"],
        str([one["summary"] for one in found]),
    )

    for path, (count, _) in audit.START_MARKS_ALLOWED.items():
        found = audit_starts({"clippy.toml": LISTED, path: mark * count})
        check(
            f"the {count} mark in {path}, which the list names, is not a finding",
            found == [],
            str(found),
        )

        found = audit_starts({"clippy.toml": LISTED, path: mark * (count + 1)})
        check(
            f"a mark in {path} beyond the {count} the list names is an error naming the file",
            kinds(found) == ["process-start-mark-count"]
            and found[0]["severity"] == audit.ERROR
            and path in found[0]["summary"],
            str([one["summary"] for one in found]),
        )

    found = audit_starts({"clippy.toml": LISTED, "crates/cli/src/programs.rs": mark})
    check(
        "a file with the name of a listed one in another directory is not the listed file",
        kinds(found) == ["process-start-mark-outside-start"],
        str(kinds(found)),
    )

    found = audit_starts(
        {
            "clippy.toml": LISTED,
            "crates/cli/src/identity.rs": '#[allow(clippy::disallowed_methods)]\nfn start() {}\n',
        }
    )
    check(
        "a mark with no reason in a file the list does not name is both unexplained and misplaced",
        kinds(found) == ["process-start-mark-outside-start", "process-start-mark-without-reason"],
        str(kinds(found)),
    )

    for name, files in {
        "an integration test file": {"crates/cli/tests/running.rs": mark},
        "a test support file": {"crates/session/test-support/profile.rs": mark},
        "a crate-wide test allow": {
            "crates/cli/src/identity.rs": '#![cfg_attr(test, allow(clippy::disallowed_methods, reason = "a test"))]\n'
        },
    }.items():
        found = audit_starts({"clippy.toml": LISTED, **files})
        check(f"a mark in {name} is not a finding, since tests start what they name", found == [], str(found))

    found = audit_starts({"clippy.toml": LISTED, "crates/sandbox/src/programs.rs": "fn go() {}\n"})
    check(
        "a file the list names that holds no mark is an error, so the list does not outlive its marks",
        kinds(found) == ["process-start-allowance-unused"]
        and "crates/sandbox/src/programs.rs" in found[0]["summary"],
        str([one["summary"] for one in found]),
    )

    found = audit_starts(
        {
            "clippy.toml": LISTED,
            "crates/sandbox/src/programs.rs": '#![cfg_attr(test, allow(clippy::disallowed_methods, reason = "a test"))]\n',
        }
    )
    check(
        "a listed file that holds only a test's allow holds no mark",
        kinds(found) == ["process-start-allowance-unused"],
        str(kinds(found)),
    )

    found = audit_starts({"clippy.toml": LISTED})
    check("a listed file that is not in the tree is not a finding", found == [], str(found))


@contextmanager
def contained_temporary_files():
    """Put every temporary file the tests make, and every one a program they run makes, in one directory.

    The fixtures are made with `mkdtemp`, which nothing removes, so a run that used the system
    temporary directory directly left one directory for each fixture there, on every run. This
    one is removed when the run ends, however it ends.
    """
    was = tempfile.tempdir, os.environ.get("TMPDIR")
    with tempfile.TemporaryDirectory(prefix="security-audit-selftest-", ignore_cleanup_errors=True) as holding:
        tempfile.tempdir = holding
        os.environ["TMPDIR"] = holding
        try:
            yield holding
        finally:
            tempfile.tempdir = was[0]
            if was[1] is None:
                os.environ.pop("TMPDIR", None)
            else:
                os.environ["TMPDIR"] = was[1]


def test_a_run_leaves_nothing_in_the_temporary_directory():
    """The fixtures a test makes go when the run does, in whatever directory the run was given."""
    with tempfile.TemporaryDirectory() as given:
        done = subprocess.run(
            [sys.executable, str(HERE / "selftest.py"), "test_exception_counts"],
            cwd=ROOT,
            env={**os.environ, "TMPDIR": given},
            capture_output=True,
            text=True,
        )
        left = sorted(os.listdir(given))
    check("the run that was given a temporary directory passes", done.returncode == 0, done.stdout + done.stderr)
    check("a run leaves nothing in the temporary directory it was given", left == [], str(left))


def test_a_name_selects_the_tests_that_run():
    with tempfile.TemporaryDirectory() as given:
        done = subprocess.run(
            [sys.executable, str(HERE / "selftest.py"), "test_nothing_has_this_name"],
            cwd=ROOT,
            env={**os.environ, "TMPDIR": given},
            capture_output=True,
            text=True,
        )
    check("a name no test has is refused", done.returncode == 2 and "test_nothing_has_this_name" in done.stderr, done.stdout + done.stderr)


TESTS = (
    test_exception_counts,
    test_network_clients_are_recorded,
    test_process_starts_are_refused_by_the_configuration,
    test_nothing_makes_clippy_read_another_configuration,
    test_a_mark_is_one_start_with_a_reason,
    test_a_mark_is_only_in_a_file_that_may_start_a_program,
    test_prompt_split,
    test_exhaustive_reader_docs,
    test_labelled_impls,
    test_pinned_actions,
    test_pinned_images,
    test_downloads_verified,
    test_privileged_job_runs_only_its_own_code,
    test_privileged_job_runs_project_code,
    test_checkout_ref_is_qualified,
    test_grant_is_behind_an_environment,
    test_required_checks_are_the_gates,
    test_construction_pinned,
    test_gates_pinned,
    test_key_sites_exhaustive,
    test_one_answer_to_which_lines_are_test_code,
    test_key_steps_join_no_names,
    test_guarantee_specs_are_read,
    test_declassify_counts_match_the_spec,
    test_every_lane_prompt_composes,
    test_the_prompt_lane_starts_from_every_prompt,
    test_answers_registered,
    test_changes_under_a_yes,
    test_the_stop_hook,
    test_verifier_prompt_composes,
    test_impact_sets_severity,
    test_labels,
    test_titles,
    test_a_name_stays_a_name,
    test_bodies_say_where_and_why,
    test_a_draft_key_is_one_its_title_keeps,
    test_a_run_writes_a_manifest_and_posts_nothing,
    test_the_manifest_is_read_from_the_work_directory,
    test_verifying_saves_the_candidates_the_collector_reads,
    test_a_test_attribute_covers_only_what_it_is_attached_to,
    test_the_identity_lane_starts_from_the_calls_that_resolve_a_name,
    test_the_identity_lane_starts_from_the_questions_put_to_a_deny_rule,
    test_the_output_lane_lists_a_write_only_where_its_arguments_are_not_all_literals,
    test_the_window_list_holds_the_places_a_string_becomes_markup,
    test_the_checkout_lane_starts_from_the_layers,
    test_the_prompt_lane_reaches_the_line_interface_and_the_replay,
    test_the_tooling_lane_starts_from_the_scripts,
    test_the_new_lanes_are_run_for_the_files_they_read,
    test_the_tracker_is_written_for_the_lanes,
    test_no_name_in_the_enumerator_is_bound_twice,
    test_every_lane_has_its_file_and_its_row,
    test_the_exemption_stops_at_the_person_s_own_configuration,
    test_candidates_that_make_one_claim_are_checked_together,
    test_a_group_is_one_verifier_and_the_others_are_in_its_prompt,
    test_a_verdict_covers_the_candidates_it_says_are_the_same_defect,
    test_a_draft_names_every_lane_that_found_it,
    test_a_run_leaves_nothing_in_the_temporary_directory,
    test_a_name_selects_the_tests_that_run,
)


def main(names):
    by_name = {test.__name__: test for test in TESTS}
    unknown = [name for name in names if name not in by_name]
    if unknown:
        print(f"no test is named {', '.join(unknown)}", file=sys.stderr)
        return 2
    with contained_temporary_files():
        for test in [by_name[name] for name in names] or TESTS:
            print(test.__name__)
            test()
    print("")
    if FAILURES:
        print(f"{len(FAILURES)} failed: {', '.join(FAILURES)}")
        return 1
    print("security-audit selftest passed")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
