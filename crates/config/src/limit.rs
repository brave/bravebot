//! The figure a session spend limit is written as.
//!
//! One reader for the `limit` setting and the `/limit` command, so a number means the same thing
//! in a settings file and at the prompt.

/// What a limit counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unit {
    /// Tokens the backends reported.
    Tokens,
    /// Leo Premium credentials spent, one for each premium request.
    Credits,
}

impl Unit {
    /// The word a message selects on and the trail records. Never shown as it stands.
    pub fn word(self) -> &'static str {
        match self {
            Self::Tokens => "tokens",
            Self::Credits => "credits",
        }
    }
}

/// A limit: a positive count of one unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limit {
    pub unit: Unit,
    pub figure: u64,
}

impl Limit {
    pub fn tokens(figure: u64) -> Self {
        Self {
            unit: Unit::Tokens,
            figure,
        }
    }

    pub fn credits(figure: u64) -> Self {
        Self {
            unit: Unit::Credits,
            figure,
        }
    }
}

/// Read a limit: a count, then optionally `tokens` or `credits`. Without a unit the count is in
/// `bare`.
///
/// `None` for anything else, and for zero.
pub fn parse(text: &str, bare: Unit) -> Option<Limit> {
    let text = text.trim();
    let (count, unit) = [
        ("tokens", Unit::Tokens),
        ("token", Unit::Tokens),
        ("credits", Unit::Credits),
        ("credit", Unit::Credits),
    ]
    .into_iter()
    .find_map(|(word, unit)| {
        let rest = text.strip_suffix(word)?;
        // A word only counts after a space, so `5xcredits` is not read as a count and a unit.
        rest.ends_with(char::is_whitespace).then_some((rest, unit))
    })
    .unwrap_or((text, bare));
    Some(Limit {
        unit,
        figure: parse_count(count)?,
    })
}

/// Read a count: whole digits, optionally followed by `k` (thousands) or `m` (millions).
///
/// `None` for anything else, and for zero: a limit of nothing would stop the first request, and
/// "no limit" has its own word at the command.
pub fn parse_count(text: &str) -> Option<u64> {
    let text = text.trim();
    let (digits, scale) = match text.chars().last()? {
        'k' | 'K' => (&text[..text.len() - 1], 1_000),
        'm' | 'M' => (&text[..text.len() - 1], 1_000_000),
        _ => (text, 1),
    };
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    digits
        .parse::<u64>()
        .ok()?
        .checked_mul(scale)
        .filter(|count| *count > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_count_is_whole_digits_with_an_optional_scale() {
        assert_eq!(parse_count("500000"), Some(500_000));
        assert_eq!(parse_count(" 500k "), Some(500_000));
        assert_eq!(parse_count("2M"), Some(2_000_000));
    }

    #[test]
    fn anything_else_is_not_a_count() {
        for text in [
            "", "k", "0", "0k", "-5", "1.5m", "1,000", "12x", "off", "1e6", "+5",
        ] {
            assert_eq!(parse_count(text), None, "{text:?}");
        }
        assert_eq!(parse_count("18446744073709551615m"), None);
    }

    #[test]
    fn a_unit_word_after_a_space_names_what_is_counted() {
        for (text, expected) in [
            ("500k", Limit::tokens(500_000)),
            ("500k tokens", Limit::tokens(500_000)),
            ("1 token", Limit::tokens(1)),
            ("40 credits", Limit::credits(40)),
            ("1 credit", Limit::credits(1)),
            ("  2k   credits ", Limit::credits(2_000)),
        ] {
            assert_eq!(parse(text, Unit::Tokens), Some(expected), "{text:?}");
        }
    }

    #[test]
    fn a_bare_count_is_in_the_unit_given() {
        assert_eq!(parse("40", Unit::Credits), Some(Limit::credits(40)));
        assert_eq!(parse("40", Unit::Tokens), Some(Limit::tokens(40)));
    }

    #[test]
    fn a_unit_without_a_usable_count_is_not_a_limit() {
        for text in [
            "credits",
            "tokens",
            "0 credits",
            "-3 credits",
            "5xcredits",
            "5 dollars",
            "1.5 credits",
            "5 credits please",
            "5 credits tokens",
        ] {
            assert_eq!(parse(text, Unit::Tokens), None, "{text:?}");
        }
    }
}
