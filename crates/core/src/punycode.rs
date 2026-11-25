//! Punycode (RFC 3492), the encoding that spells a label outside ASCII in the letters, digits and
//! `-` a host name may hold. Encoding only: a host name is turned into its ASCII form so a rule
//! written in one form covers a URL written in the other.

#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

#[cfg_attr(verus_keep_ghost, verus_verify)]
const BASE: u32 = 36;
#[cfg_attr(verus_keep_ghost, verus_verify)]
const T_MIN: u32 = 1;
#[cfg_attr(verus_keep_ghost, verus_verify)]
const T_MAX: u32 = 26;
#[cfg_attr(verus_keep_ghost, verus_verify)]
const SKEW: u32 = 38;
#[cfg_attr(verus_keep_ghost, verus_verify)]
const DAMP: u32 = 700;
#[cfg_attr(verus_keep_ghost, verus_verify)]
const INITIAL_BIAS: u32 = 72;
#[cfg_attr(verus_keep_ghost, verus_verify)]
const INITIAL_N: u32 = 128;

/// The largest bias [`adapt`] returns, below which [`digits_of`] needs no wider integer.
#[cfg_attr(verus_keep_ghost, verus_verify)]
const MOST_BIAS: u32 = 287;

/// The bias adaptation function of RFC 3492 section 6.1.
///
/// Each round divides `delta` by 35 and adds 36 to `k`, and a `u32` is below 455 within seven
/// rounds, so `k` is at most 252 and the result below 288 for every input. Nothing here wraps.
#[cfg_attr(verus_keep_ghost, verus_spec(r =>
    ensures
        r <= 287,
))]
fn adapt(delta: u32, points: u32, first: bool) -> u32 {
    let mut delta = if first { delta / DAMP } else { delta / 2 };
    // Half of a u32 at most, so adding a part of itself cannot wrap. A zero count of points never
    // reaches here, and is read as one.
    delta += delta / points.max(1);
    let mut k: u32 = 0;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            k % BASE == 0,
            k <= 252,
            k >= 36 ==> delta <= 122713351,
            k >= 72 ==> delta <= 3506095,
            k >= 108 ==> delta <= 100174,
            k >= 144 ==> delta <= 2862,
            k >= 180 ==> delta <= 81,
            k >= 216 ==> delta <= 2,
        decreases delta,
    ))]
    while delta > ((BASE - T_MIN) * T_MAX) / 2 {
        delta /= BASE - T_MIN;
        k += BASE;
    }
    // 36 times delta over delta plus 38 is below 36.
    let step = ((BASE - T_MIN + 1) * delta) / (delta + SKEW);
    #[cfg(verus_keep_ghost)]
    proof! {
        assert(step < 36) by (nonlinear_arith)
            requires
                step == (36 * delta) / (delta + 38),
                delta <= 455;
    }
    k + step
}

/// The basic code point for a digit from 0 to 35: `a` to `z`, then `0` to `9`. `None` for a
/// larger digit, which the encoder never produces.
#[cfg_attr(verus_keep_ghost, verus_verify(external_body))]
#[cfg_attr(verus_keep_ghost, verus_spec(r =>
    ensures
        d < 36 ==> r is Some,
))]
fn digit(d: u32) -> Option<char> {
    char::from_digit((d + 10) % BASE, BASE).filter(|_| d < BASE)
}

/// The digits that spell `delta` at `bias`, the generalized variable-length integer of RFC 3492
/// section 6.3, pushed onto `output`.
///
/// Each digit but the last divides what is left by at least 10, so a `u32` takes at most eleven
/// and `k` stays far below where it could wrap. Every digit is below 36, and the last one below
/// the threshold it stopped at. A bias above the one
/// [`adapt`] can return is read as that one.
#[cfg_attr(verus_keep_ghost, verus_verify)]
fn digits_of(delta: u32, bias: u32, output: &mut String) {
    let bias = bias.min(MOST_BIAS);
    let mut q = delta;
    let mut k: u32 = BASE;
    // The digit the threshold stopped at, set when the loop ends.
    let mut last = false;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant
            k % BASE == 0,
            k >= 36,
            k <= 396,
            bias <= 287,
            last ==> q < 36,
            k >= 72 ==> q <= 429496729,
            k >= 108 ==> q <= 42949672,
            k >= 144 ==> q <= 4294967,
            k >= 180 ==> q <= 429496,
            k >= 216 ==> q <= 42949,
            k >= 252 ==> q <= 4294,
            k >= 288 ==> q <= 429,
            k >= 324 ==> q <= 42,
            k >= 360 ==> q <= 4,
            k >= 396 ==> q == 0,
        decreases q + if last { 0u32 } else { 1u32 },
    ))]
    while !last {
        let t = if k <= bias {
            T_MIN
        } else if k >= bias + T_MAX {
            T_MAX
        } else {
            k - bias
        };
        if q < t {
            // The threshold is at most 26, so what is left is the last digit.
            last = true;
        } else {
            #[cfg(verus_keep_ghost)]
            proof! {
                assert(t + (q - t) % (BASE - t) < 36) by (nonlinear_arith)
                    requires
                        BASE - t >= 10,
                        q >= t;
                assert((q - t) / (BASE - t) <= q / 10) by (nonlinear_arith)
                    requires
                        BASE - t >= 10,
                        q >= t,
                        t >= 1;
            }
            push_digit(output, t + (q - t) % (BASE - t));
            q = (q - t) / (BASE - t);
            k += BASE;
        }
    }
    push_digit(output, q);
}

/// Push the basic code point for `d`, a digit below 36, onto `output`.
#[cfg_attr(verus_keep_ghost, verus_verify(external_body))]
#[cfg_attr(verus_keep_ghost, verus_spec(
    requires
        d < 36,
))]
fn push_digit(output: &mut String, d: u32) {
    if let Some(c) = digit(d) {
        output.push(c);
    }
}

/// `label` in Punycode (RFC 3492 section 6.3), without the `xn--` prefix, or `None` where the
/// arithmetic overflows. Basic code points are copied as they are, case included.
#[cfg_attr(verus_keep_ghost, verifier::external)]
pub(crate) fn encode(label: &str) -> Option<String> {
    let points: Vec<u32> = label.chars().map(u32::from).collect();
    let mut output: String = label.chars().filter(char::is_ascii).collect();
    let basic = u32::try_from(output.len()).ok()?;
    let total = u32::try_from(points.len()).ok()?;
    if basic > 0 {
        output.push('-');
    }
    let mut n = INITIAL_N;
    let mut delta: u32 = 0;
    let mut bias = INITIAL_BIAS;
    let mut handled = basic;
    while handled < total {
        let m = points.iter().copied().filter(|&p| p >= n).min()?;
        delta = delta.checked_add((m - n).checked_mul(handled + 1)?)?;
        n = m;
        for &point in &points {
            if point < n {
                delta = delta.checked_add(1)?;
            }
            if point == n {
                digits_of(delta, bias, &mut output);
                bias = adapt(delta, handled + 1, handled == basic);
                delta = 0;
                handled += 1;
            }
        }
        delta = delta.checked_add(1)?;
        n = n.checked_add(1)?;
    }
    Some(output)
}

/// `name` with each label outside ASCII written as `xn--` and its Punycode, the ASCII form a
/// URL holds for an international name. `None` where a label cannot be encoded.
#[cfg_attr(verus_keep_ghost, verifier::external)]
pub(crate) fn to_ascii(name: &str) -> Option<String> {
    let labels: Option<Vec<String>> = name
        .split('.')
        .map(|label| match label.is_ascii() {
            true => Some(label.to_string()),
            false => encode(label).map(|encoded| format!("xn--{encoded}")),
        })
        .collect();
    labels.map(|labels| labels.join("."))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Known pairs: two German names and two samples of RFC 3492 section 7.1, (A) Arabic and (B)
    /// simplified Chinese, whose longer outputs exercise bias adaptation past the first point.
    #[test]
    fn a_label_encodes_to_its_known_punycode() {
        for (label, encoded) in [
            ("bücher", "bcher-kva"),
            ("münchen", "mnchen-3ya"),
            (
                "\u{644}\u{64a}\u{647}\u{645}\u{627}\u{628}\u{62a}\u{643}\u{644}\u{645}\u{648}\
                 \u{634}\u{639}\u{631}\u{628}\u{64a}\u{61f}",
                "egbpdaj6bu4bxfgehfvwxn",
            ),
            (
                "\u{4ed6}\u{4eec}\u{4e3a}\u{4ec0}\u{4e48}\u{4e0d}\u{8bf4}\u{4e2d}\u{6587}",
                "ihqwcrb4cv8a8dqg056pqjye",
            ),
        ] {
            assert_eq!(encode(label).as_deref(), Some(encoded), "{label}");
        }
    }

    /// RFC 3492 divides delta down while it is above 455, not while it is 455, so a delta that
    /// comes to exactly 455 takes no round. Verus proves `adapt` never wraps but not which bias it
    /// returns, so this pins the boundary a wrong comparison would move.
    #[test]
    fn a_delta_of_exactly_455_is_not_divided_down() {
        // 910 halved is 455, and a part of it over u32::MAX points adds nothing.
        assert_eq!(adapt(910, u32::MAX, false), 33);
        assert_eq!(adapt(912, u32::MAX, false), 45);
    }

    /// Only a label outside ASCII is encoded; the others, and the dots between them, are kept.
    #[test]
    fn a_name_is_written_with_each_label_outside_ascii_encoded() {
        assert_eq!(
            to_ascii("docs.bücher.example").as_deref(),
            Some("docs.xn--bcher-kva.example")
        );
        assert_eq!(to_ascii("example.com").as_deref(), Some("example.com"));
    }
}
