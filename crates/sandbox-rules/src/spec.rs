//! What the code in this crate is proved to give.
//!
//! The quoting in [`crate::quote`] gives the value it was handed, read back by the platform's own
//! rules (SANDBOX-4). Each reader is written from the platform's documented rules, not from the
//! code it checks: Seatbelt's string literal, and the quoted argument `CommandLineToArgvW` splits a
//! command line into. A proof here is about bravebot's half, that what is written reads back as the
//! value. That the platform reads it by those rules is the platform's, and is not proved.
//!
//! A Seatbelt profile [`crate::seatbelt::rules`] builds opens by refusing everything, and each rule
//! after that is one [`may_hold`] allows for the parts it was built from (SANDBOX-3, SANDBOX-31).

use crate::mode::SandboxMode;
use crate::seatbelt::{Action, Hold, Operation, Parts, Row, Rule, Target, Write};
use crate::seatbelt::{LOOPBACK, MACH_SERVICES, RESOLVER, SYSCTL_NAMES, TRUST_SERVICES};
use vstd::prelude::*;

verus! {

// Seatbelt.

/// What [`crate::quote::seatbelt_literal`] writes for one character: a backslash or a quotation
/// mark behind a backslash, and any other character as it is.
pub open spec fn escaped(c: char) -> Seq<char> {
    if c == '\\' || c == '"' {
        seq!['\\', c]
    } else {
        seq![c]
    }
}

/// What it writes for `s` between its quotation marks.
pub open spec fn escape(s: Seq<char>) -> Seq<char>
    decreases s.len(),
{
    if s.len() == 0 {
        Seq::empty()
    } else {
        escaped(s[0]) + escape(s.drop_first())
    }
}

/// How Seatbelt reads what follows a literal's opening quotation mark: a backslash takes the
/// character after it as it is, and a quotation mark ends the literal. `None` where the literal
/// does not end, or where anything follows the quotation mark that ends it, which is a literal
/// that let the text after it be read as profile syntax.
pub open spec fn read_body(t: Seq<char>) -> Option<Seq<char>>
    decreases t.len(),
{
    if t.len() == 0 {
        None
    } else if t[0] == '"' {
        if t.len() == 1 {
            Some(Seq::empty())
        } else {
            None
        }
    } else if t[0] == '\\' {
        if t.len() < 2 || (t[1] != '\\' && t[1] != '"') {
            None
        } else {
            match read_body(t.drop_first().drop_first()) {
                Some(r) => Some(seq![t[1]] + r),
                None => None,
            }
        }
    } else {
        match read_body(t.drop_first()) {
            Some(r) => Some(seq![t[0]] + r),
            None => None,
        }
    }
}

/// `t` read as one Seatbelt string literal, and the value it holds.
pub open spec fn read_literal(t: Seq<char>) -> Option<Seq<char>> {
    if t.len() > 0 && t[0] == '"' {
        read_body(t.drop_first())
    } else {
        None
    }
}

pub proof fn lemma_escape_push(a: Seq<char>, c: char)
    ensures
        escape(a.push(c)) == escape(a) + escaped(c),
    decreases a.len(),
{
    if a.len() == 0 {
        let one = a.push(c);
        assert(one.len() == 1 && one[0] == c);
        assert(one.drop_first() =~= Seq::<char>::empty());
        assert(escape(one.drop_first()) =~= Seq::<char>::empty());
        assert(escape(one) == escaped(c) + escape(one.drop_first()));
        assert(escape(one) =~= escaped(c));
        assert(escape(a) =~= Seq::<char>::empty());
    } else {
        let longer = a.push(c);
        assert(longer[0] == a[0]);
        assert(longer.drop_first() =~= a.drop_first().push(c));
        lemma_escape_push(a.drop_first(), c);
        assert(escape(longer) == escaped(a[0]) + escape(a.drop_first().push(c)));
        assert(escape(a) == escaped(a[0]) + escape(a.drop_first()));
        assert(escape(longer) =~= escape(a) + escaped(c));
    }
}

/// What [`crate::quote::seatbelt_literal`] writes between its quotation marks, and the one that
/// closes it, reads back as the value and nothing after it.
pub proof fn a_seatbelt_literal_reads_back_as_its_path(s: Seq<char>)
    ensures
        read_body(escape(s) + seq!['"']) == Some(s),
    decreases s.len(),
{
    if s.len() == 0 {
        assert(escape(s) + seq!['"'] =~= seq!['"']);
    } else {
        let t = escape(s) + seq!['"'];
        let rest = escape(s.drop_first()) + seq!['"'];
        a_seatbelt_literal_reads_back_as_its_path(s.drop_first());
        if s[0] == '\\' || s[0] == '"' {
            assert(t =~= seq!['\\', s[0]] + rest);
            assert(t.drop_first().drop_first() =~= rest);
        } else {
            assert(t =~= seq![s[0]] + rest);
            assert(t.drop_first() =~= rest);
        }
        assert(seq![s[0]] + s.drop_first() =~= s);
    }
}

// Windows.

/// `n` backslashes.
pub open spec fn backslashes(n: nat) -> Seq<char> {
    Seq::new(n, |_i| '\\')
}

/// How `CommandLineToArgvW` reads what follows a quoted argument's opening quotation mark, with
/// `n` backslashes read and not yet written out: a run of backslashes before a quotation mark
/// gives half as many, and a quotation mark that an even run leaves unescaped ends the argument,
/// where an odd run makes it content. A run before any other character is literal. Gives the
/// argument and what follows its closing quotation mark, or `None` where it does not close.
pub open spec fn read_quoted(t: Seq<char>, n: nat) -> Option<(Seq<char>, Seq<char>)>
    decreases t.len(),
{
    if t.len() == 0 {
        None
    } else if t[0] == '\\' {
        read_quoted(t.drop_first(), n + 1)
    } else if t[0] == '"' {
        if n % 2 == 0 {
            Some((backslashes(n / 2), t.drop_first()))
        } else {
            match read_quoted(t.drop_first(), 0) {
                Some((a, r)) => Some((backslashes(n / 2).push('"') + a, r)),
                None => None,
            }
        }
    } else {
        match read_quoted(t.drop_first(), 0) {
            Some((a, r)) => Some((backslashes(n).push(t[0]) + a, r)),
            None => None,
        }
    }
}

pub proof fn lemma_skip(m: nat, t: Seq<char>, n: nat)
    ensures
        read_quoted(backslashes(m) + t, n) == read_quoted(t, n + m),
    decreases m,
{
    if m > 0 {
        let u = backslashes(m) + t;
        assert(u[0] == '\\');
        assert(u.drop_first() =~= backslashes((m - 1) as nat) + t);
        lemma_skip((m - 1) as nat, t, n + 1);
    } else {
        assert(backslashes(0) + t =~= t);
    }
}

/// What [`crate::quote::quoted`] writes for `s` between its quotation marks, `n` being the run of
/// backslashes at the end of what it wrote before: each backslash as it comes, the run before a
/// quotation mark doubled and the mark escaped, and the run before the closing mark doubled.
pub open spec fn emit(s: Seq<char>, n: nat) -> Seq<char>
    decreases s.len(),
{
    if s.len() == 0 {
        backslashes(n)
    } else if s[0] == '\\' {
        seq!['\\'] + emit(s.drop_first(), n + 1)
    } else if s[0] == '"' {
        backslashes(n + 1).push('"') + emit(s.drop_first(), 0)
    } else {
        seq![s[0]] + emit(s.drop_first(), 0)
    }
}

pub proof fn lemma_read_emit(s: Seq<char>, n: nat, rest: Seq<char>)
    ensures
        read_quoted(emit(s, n) + seq!['"'] + rest, n) == Some((backslashes(n) + s, rest)),
    decreases s.len(),
{
    if s.len() == 0 {
        let t = backslashes(n) + (seq!['"'] + rest);
        assert(emit(s, n) + seq!['"'] + rest =~= t);
        lemma_skip(n, seq!['"'] + rest, n);
        let q = seq!['"'] + rest;
        assert(q[0] == '"');
        assert(q.drop_first() =~= rest);
        assert((n + n) % 2 == 0) by (nonlinear_arith);
        assert((n + n) / 2 == n) by (nonlinear_arith);
        assert(s =~= Seq::<char>::empty());
        assert(backslashes(n) + s =~= backslashes(n));
    } else if s[0] == '\\' {
        let t = seq!['\\'] + (emit(s.drop_first(), n + 1) + seq!['"'] + rest);
        assert(emit(s, n) + seq!['"'] + rest =~= t);
        assert(t[0] == '\\');
        assert(t.drop_first() =~= emit(s.drop_first(), n + 1) + seq!['"'] + rest);
        lemma_read_emit(s.drop_first(), n + 1, rest);
        assert(backslashes(n + 1) + s.drop_first() =~= backslashes(n) + s);
    } else if s[0] == '"' {
        let tail = emit(s.drop_first(), 0) + seq!['"'] + rest;
        let t = backslashes(n + 1) + (seq!['"'] + tail);
        assert(emit(s, n) + seq!['"'] + rest =~= t);
        lemma_skip(n + 1, seq!['"'] + tail, n);
        let q = seq!['"'] + tail;
        assert(q[0] == '"');
        assert(q.drop_first() =~= tail);
        assert((n + (n + 1)) % 2 == 1) by (nonlinear_arith);
        assert((n + (n + 1)) / 2 == n) by (nonlinear_arith);
        lemma_read_emit(s.drop_first(), 0, rest);
        assert(backslashes(0) + s.drop_first() =~= s.drop_first());
        assert(backslashes(n).push('"') + s.drop_first() =~= backslashes(n) + s);
    } else {
        let tail = emit(s.drop_first(), 0) + seq!['"'] + rest;
        let t = seq![s[0]] + tail;
        assert(emit(s, n) + seq!['"'] + rest =~= t);
        assert(t[0] == s[0]);
        assert(t.drop_first() =~= tail);
        lemma_read_emit(s.drop_first(), 0, rest);
        assert(backslashes(0) + s.drop_first() =~= s.drop_first());
        assert(backslashes(n).push(s[0]) + s.drop_first() =~= backslashes(n) + s);
    }
}

/// An argument [`crate::quote::quoted`] writes reads back as itself, ending at its closing
/// quotation mark, whatever follows it.
pub proof fn a_quoted_argument_reads_back_as_itself(s: Seq<char>, rest: Seq<char>)
    ensures
        read_quoted(emit(s, 0) + seq!['"'] + rest, 0) == Some((s, rest)),
{
    lemma_read_emit(s, 0, rest);
    assert(backslashes(0) + s =~= s);
}

/// The run of backslashes `s` ends on, after a run of `n` before it.
pub open spec fn run_after(s: Seq<char>, n: nat) -> nat
    decreases s.len(),
{
    if s.len() == 0 {
        n
    } else if s[0] == '\\' {
        run_after(s.drop_first(), n + 1)
    } else {
        run_after(s.drop_first(), 0)
    }
}

/// What [`crate::quote::quoted`] has written for `s` before it knows what follows: [`emit`] less
/// the run before the closing mark.
pub open spec fn written(s: Seq<char>, n: nat) -> Seq<char>
    decreases s.len(),
{
    if s.len() == 0 {
        Seq::empty()
    } else if s[0] == '\\' {
        seq!['\\'] + written(s.drop_first(), n + 1)
    } else if s[0] == '"' {
        backslashes(n + 1).push('"') + written(s.drop_first(), 0)
    } else {
        seq![s[0]] + written(s.drop_first(), 0)
    }
}

pub proof fn lemma_emit_is_written_and_the_run(a: Seq<char>, n: nat)
    ensures
        emit(a, n) == written(a, n) + backslashes(run_after(a, n)),
    decreases a.len(),
{
    if a.len() == 0 {
        assert(written(a, n) + backslashes(n) =~= backslashes(n));
    } else if a[0] == '\\' {
        lemma_emit_is_written_and_the_run(a.drop_first(), n + 1);
        assert(emit(a, n) =~= seq!['\\'] + (written(a.drop_first(), n + 1) + backslashes(
            run_after(a.drop_first(), n + 1),
        )));
    } else {
        lemma_emit_is_written_and_the_run(a.drop_first(), 0);
        if a[0] == '"' {
            assert(emit(a, n) =~= backslashes(n + 1).push('"') + (written(a.drop_first(), 0)
                + backslashes(run_after(a.drop_first(), 0))));
        } else {
            assert(emit(a, n) =~= seq![a[0]] + (written(a.drop_first(), 0) + backslashes(
                run_after(a.drop_first(), 0),
            )));
        }
    }
}

pub proof fn lemma_written_of_one(c: char, n: nat)
    ensures
        written(seq![c], n) == (if c == '\\' {
            seq!['\\']
        } else if c == '"' {
            backslashes(n + 1).push('"')
        } else {
            seq![c]
        }),
        run_after(seq![c], n) == (if c == '\\' {
            n + 1
        } else {
            0
        }),
{
    let s = seq![c];
    assert(s.len() == 1 && s[0] == c);
    assert(s.drop_first() =~= Seq::<char>::empty());
    assert(written(s.drop_first(), 0) =~= Seq::<char>::empty());
    assert(written(s.drop_first(), n + 1) =~= Seq::<char>::empty());
    assert(run_after(s.drop_first(), 0) == 0);
    assert(run_after(s.drop_first(), n + 1) == n + 1);
    if c == '\\' {
        assert(written(s, n) =~= seq!['\\']);
    } else if c == '"' {
        assert(written(s, n) =~= backslashes(n + 1).push('"'));
    } else {
        assert(written(s, n) =~= seq![c]);
    }
}

pub proof fn lemma_written_push(a: Seq<char>, c: char, n: nat)
    ensures
        written(a.push(c), n) == written(a, n) + written(seq![c], run_after(a, n)),
        run_after(a.push(c), n) == run_after(seq![c], run_after(a, n)),
    decreases a.len(),
{
    if a.len() == 0 {
        assert(a.push(c) =~= seq![c]);
        assert(written(a, n) + written(seq![c], n) =~= written(seq![c], n));
    } else {
        assert(a.push(c)[0] == a[0]);
        assert(a.push(c).drop_first() =~= a.drop_first().push(c));
        if a[0] == '\\' {
            lemma_written_push(a.drop_first(), c, n + 1);
            assert(written(a.push(c), n) =~= seq!['\\'] + (written(a.drop_first(), n + 1) + written(
                seq![c],
                run_after(a.drop_first(), n + 1),
            )));
        } else {
            lemma_written_push(a.drop_first(), c, 0);
            if a[0] == '"' {
                assert(written(a.push(c), n) =~= backslashes(n + 1).push('"') + (written(
                    a.drop_first(),
                    0,
                ) + written(seq![c], run_after(a.drop_first(), 0))));
            } else {
                assert(written(a.push(c), n) =~= seq![a[0]] + (written(a.drop_first(), 0)
                    + written(seq![c], run_after(a.drop_first(), 0))));
            }
        }
    }
}

// Seatbelt profiles.

/// The rule every profile opens with, which refuses everything the rules after it do not grant.
pub open spec fn deny_default() -> Rule {
    Rule { action: Action::Deny, operation: Operation::Default, target: Target::Any }
}

/// Whether `name` is one of the names in `list`.
pub open spec fn in_list<const N: usize>(list: [&'static str; N], name: Seq<char>) -> bool {
    exists|j: int| 0 <= j < N && #[trigger] list@[j]@ == name
}

/// Whether one of `rows` grants `path`.
pub open spec fn granted(rows: Seq<Row>, path: Seq<char>) -> bool {
    exists|j: int| 0 <= j < rows.len() && #[trigger] rows[j].path@ == path && rows[j].allows
}

/// A Mach service a profile may allow a lookup of: one of the fixed list, or a trust service
/// where the network is allowed (SANDBOX-31).
pub open spec fn service_allowed(name: Seq<char>, network: bool) -> bool {
    in_list(MACH_SERVICES, name) || (network && in_list(TRUST_SERVICES, name))
}

/// Whether `target` is any port on the loopback address.
pub open spec fn is_loopback(target: Target) -> bool {
    match target {
        Target::LocalIp(address) => address@ == LOOPBACK@,
        Target::RemoteIp(Some(address)) => address@ == LOOPBACK@,
        _ => false,
    }
}

/// The rule a row is written as, for `operation`.
pub open spec fn row_rule(row: Row, operation: Operation) -> Rule {
    Rule {
        action: if row.allows { Action::Allow } else { Action::Deny },
        operation,
        target: Target::Subpath(row.path),
    }
}

/// The operation a rule about `write` is about.
pub open spec fn write_operation(write: Write) -> Operation {
    match write {
        Write::Any => Operation::FileWrite,
        Write::Unlink => Operation::FileWriteUnlink,
        Write::Create => Operation::FileWriteCreate,
    }
}

/// Whether `operation` is a write of any kind.
pub open spec fn is_write(operation: Operation) -> bool {
    operation == Operation::FileWrite || operation == Operation::FileWriteUnlink || operation
        == Operation::FileWriteCreate
}

/// Whether one of `holds` is the rule `rule`.
pub open spec fn held(holds: Seq<Hold>, rule: Rule) -> bool {
    exists|j: int|
        0 <= j < holds.len() && #[trigger] holds[j].action == rule.action && write_operation(
            holds[j].write,
        ) == rule.operation && holds[j].target == rule.target
}

/// Whether a profile for `parts` may allow a connect to `target`: a socket a socket row grants,
/// the resolver and any address where egress is open to every address, the one address egress is
/// held to, and the loopback address where loopback is granted and egress is not held.
pub open spec fn connect_allowed(target: Target, parts: Parts) -> bool {
    match target {
        Target::Subpath(path) => parts.network && granted(parts.sockets@, path@),
        Target::Literal(path) => parts.network && parts.egress_to is None && path@ == RESOLVER@,
        Target::RemoteIp(None) => parts.network && parts.egress_to is None,
        Target::RemoteIp(Some(address)) => (parts.network && parts.egress_to is Some
            && address@ == parts.egress_to->Some_0@) || (parts.loopback && parts.egress_to is None
            && address@ == LOOPBACK@),
        _ => false,
    }
}

/// What a rule after a profile's first may be, for the parts the profile was built from.
///
/// None refuses or grants everything again. The process information of other processes is never
/// granted, and a signal reaches only what the profile started. A machine-information query and a
/// Mach lookup are of the fixed lists, the trust services only where the network is allowed. A
/// rule about the network is there only where the network is allowed, or where loopback is and it
/// names the loopback address. A listen is allowed only where loopback is granted, and only on the
/// loopback address. A connect is allowed only where [`connect_allowed`] says. A read is allowed
/// only of the root directory itself and of the paths the read rows grant, and a write only where
/// a write row or one of the holds grants it. A process starts another only where `subprocesses`
/// is set.
pub open spec fn may_hold(rule: Rule, parts: Parts) -> bool {
    &&& rule.operation != Operation::Default
    &&& rule.operation == Operation::ProcessInfo ==> rule.action == Action::Deny
    &&& rule.operation == Operation::ProcessInfoPidinfo ==> rule.target == Target::SelfProcess
    &&& rule.operation == Operation::ProcessFork ==> parts.subprocesses
    &&& rule.operation == Operation::Signal ==> rule.target == Target::SameSandbox
    &&& rule.operation == Operation::SysctlRead ==> match rule.target {
        Target::SysctlNames(names) => forall|k: int|
            0 <= k < names@.len() ==> in_list(SYSCTL_NAMES, #[trigger] names@[k]@),
        _ => false,
    }
    &&& rule.operation == Operation::MachLookup ==> match rule.target {
        Target::GlobalNames(names) => forall|k: int|
            0 <= k < names@.len() ==> service_allowed(#[trigger] names@[k]@, parts.network),
        _ => false,
    }
    &&& (rule.operation == Operation::NetworkOutbound || rule.operation
        == Operation::NetworkInbound) ==> (parts.network || (parts.loopback && is_loopback(
        rule.target,
    )))
    &&& rule.operation == Operation::NetworkInbound ==> parts.loopback && is_loopback(rule.target)
    &&& (rule.action == Action::Allow && rule.operation == Operation::NetworkOutbound)
        ==> connect_allowed(rule.target, parts)
    &&& (rule.action == Action::Allow && is_write(rule.operation)) ==> (held(parts.holds@, rule) || (
    rule.operation == Operation::FileWrite && match rule.target {
        Target::Subpath(path) => granted(parts.writes@, path@),
        _ => false,
    }))
    &&& (rule.action == Action::Allow && rule.operation == Operation::FileRead) ==> match rule.target {
        Target::Literal(path) => path@ == "/"@,
        Target::Subpath(path) => granted(parts.reads@, path@),
        _ => false,
    }
}

/// A profile [`crate::seatbelt::rules`] builds refuses everything before it grants anything, and
/// every rule after that is one [`may_hold`] allows. So with the network closed and loopback not
/// granted no rule is about the network, without loopback nothing listens, without subprocesses
/// nothing starts another process, the trust services are not looked up, and a read is allowed
/// only of `/` and of the read rows.
pub proof fn a_seatbelt_profile_refuses_first_and_grants_only_what_its_parts_allow(
    profile: Seq<Rule>,
    parts: Parts,
)
    requires
        profile.len() > 0,
        profile[0] == deny_default(),
        forall|i: int| 1 <= i < profile.len() ==> may_hold(#[trigger] profile[i], parts),
    ensures
        forall|i: int| 1 <= i < profile.len() ==> #[trigger] profile[i].operation != Operation::Default,
        !parts.network && !parts.loopback ==> forall|i: int|
            0 <= i < profile.len() ==> #[trigger] profile[i].operation != Operation::NetworkOutbound
                && profile[i].operation != Operation::NetworkInbound,
        !parts.subprocesses ==> forall|i: int|
            0 <= i < profile.len() ==> #[trigger] profile[i].operation != Operation::ProcessFork,
        !parts.loopback ==> forall|i: int|
            0 <= i < profile.len() ==> #[trigger] profile[i].operation != Operation::NetworkInbound,
{
}

// Sandbox modes.

/// How much of the machine a stage under `mode` reads, as SANDBOX-22 ranks it: on Windows only
/// `off` reads it, and elsewhere `strict` reads none of it, `read-only` and `workspace-write` read
/// it but for the credential table, and `off` reads all of it.
pub open spec fn reach_of_reads(mode: SandboxMode, windows: bool) -> u8 {
    match (mode, windows) {
        (SandboxMode::Off, true) => 1,
        (_, true) => 0,
        (SandboxMode::Strict, false) => 0,
        (SandboxMode::Off, false) => 2,
        _ => 1,
    }
}

/// How much a stage under `mode` writes, as SANDBOX-22 ranks it: `read-only` none of the session,
/// `strict` the session, `workspace-write` the session and what a request would have been granted,
/// and `off` everything.
pub open spec fn reach_of_writes(mode: SandboxMode) -> u8 {
    match mode {
        SandboxMode::ReadOnly => 0,
        SandboxMode::Strict => 1,
        SandboxMode::WorkspaceWrite => 2,
        SandboxMode::Off => 3,
    }
}

/// `off` reads at least as much of the machine as every other mode and writes more than every
/// other mode, and `read-only` writes less than every other mode. So no mode is looser than `off`,
/// and a pin of `read-only` refuses every other mode for what it writes (SANDBOX-22).
pub proof fn off_is_the_loosest_mode_and_read_only_writes_the_least(mode: SandboxMode, windows: bool)
    ensures
        reach_of_reads(mode, windows) <= reach_of_reads(SandboxMode::Off, windows),
        mode != SandboxMode::Off ==> reach_of_writes(mode) < reach_of_writes(SandboxMode::Off),
        mode != SandboxMode::ReadOnly ==> reach_of_writes(SandboxMode::ReadOnly) < reach_of_writes(
            mode,
        ),
{
}


} // verus!
