//! Punycode (RFC 3492), the encoding that spells a label outside ASCII in the letters, digits and
//! `-` a host name may hold. Encoding only: a host name is turned into its ASCII form so a rule
//! written in one form covers a URL written in the other.

const BASE: u32 = 36;
const T_MIN: u32 = 1;
const T_MAX: u32 = 26;
const SKEW: u32 = 38;
const DAMP: u32 = 700;
const INITIAL_BIAS: u32 = 72;
const INITIAL_N: u32 = 128;

/// The bias adaptation function of RFC 3492 section 6.1.
fn adapt(delta: u32, points: u32, first: bool) -> u32 {
    let mut delta = if first { delta / DAMP } else { delta / 2 };
    delta += delta / points;
    let mut k = 0;
    while delta > ((BASE - T_MIN) * T_MAX) / 2 {
        delta /= BASE - T_MIN;
        k += BASE;
    }
    k + ((BASE - T_MIN + 1) * delta) / (delta + SKEW)
}

/// The basic code point for a digit from 0 to 35: `a` to `z`, then `0` to `9`. `None` for a
/// larger digit, which the encoder never produces.
fn digit(d: u32) -> Option<char> {
    char::from_digit((d + 10) % BASE, BASE).filter(|_| d < BASE)
}

/// `label` in Punycode (RFC 3492 section 6.3), without the `xn--` prefix, or `None` where the
/// arithmetic overflows. Basic code points are copied as they are, case included.
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
                let mut q = delta;
                let mut k = BASE;
                loop {
                    let t = if k <= bias {
                        T_MIN
                    } else if k >= bias + T_MAX {
                        T_MAX
                    } else {
                        k - bias
                    };
                    if q < t {
                        break;
                    }
                    output.push(digit(t + (q - t) % (BASE - t))?);
                    q = (q - t) / (BASE - t);
                    k += BASE;
                }
                output.push(digit(q)?);
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
