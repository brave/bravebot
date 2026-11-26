//! What the quoting in [`crate::quote`] is proved to give: the value it was handed, read back by
//! the platform's own rules (SANDBOX-4).
//!
//! Each reader is written from the platform's documented rules, not from the code it checks:
//! Seatbelt's string literal, and the quoted argument `CommandLineToArgvW` splits a command line
//! into. A proof here is about bravebot's half, that what is written reads back as the value. That
//! the platform reads it by those rules is the platform's, and is not proved.

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

} // verus!
