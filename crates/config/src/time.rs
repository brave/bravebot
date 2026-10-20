//! Reading a time written as RFC 3339.
//!
//! One reader for every crate that may depend on this one, so a time is read the same way wherever
//! it arrives from: a model's `modified_at` in an Ollama listing, and the `Expiration` the AWS CLI
//! prints with a session's credentials.

/// An RFC 3339 time as seconds and nanoseconds since the epoch, or `None` where it is not one.
///
/// The zone is applied, so two times that carry different offsets compare as the instants they
/// name: a model pulled later in New York can read as earlier than one pulled in Paris when the
/// text is compared. A time with no `Z` and no numeric offset is not an RFC 3339 time and is
/// `None`, as is any text after the zone. Anything unreadable is `None`, which sorts before every
/// time.
///
/// A second of 60 is accepted, as a leap second is written. Seconds before the epoch are negative.
pub fn rfc3339(text: &str) -> Option<(i64, u32)> {
    let bytes = text.as_bytes();
    let digits = |range: std::ops::Range<usize>| {
        let part = text.get(range)?;
        part.bytes()
            .all(|byte| byte.is_ascii_digit())
            .then(|| part.parse::<i64>().ok())
            .flatten()
    };
    // Rejected rather than skipped over: a value whose separators are somewhere else is not the
    // format this claims to read, and guessing at it would produce a confidently wrong date.
    if [4, 7].iter().any(|&at| bytes.get(at) != Some(&b'-'))
        || !matches!(bytes.get(10), Some(b'T' | b't'))
        || [13, 16].iter().any(|&at| bytes.get(at) != Some(&b':'))
    {
        return None;
    }
    let (year, month, day) = (digits(0..4)?, digits(5..7)?, digits(8..10)?);
    let (hour, minute, second) = (digits(11..13)?, digits(14..16)?, digits(17..19)?);
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 60
    {
        return None;
    }

    let mut at = 19;
    let mut nanos = 0;
    if bytes.get(at) == Some(&b'.') {
        let start = at + 1;
        at = start;
        while bytes.get(at).is_some_and(u8::is_ascii_digit) {
            at += 1;
        }
        let fraction = text
            .get(start..at)
            .filter(|fraction| !fraction.is_empty())?;
        let mut padded: String = fraction.chars().take(9).collect();
        while padded.len() < 9 {
            padded.push('0');
        }
        nanos = padded.parse::<u32>().ok()?;
    }
    let offset = match text.get(at..)? {
        "Z" | "z" => 0,
        zone => {
            let sign = match zone.as_bytes().first() {
                Some(b'+') => 1,
                Some(b'-') => -1,
                _ => return None,
            };
            if zone.len() != 6 || zone.as_bytes()[3] != b':' {
                return None;
            }
            let (hours, minutes) = (digits(at + 1..at + 3)?, digits(at + 4..at + 6)?);
            if hours > 23 || minutes > 59 {
                return None;
            }
            sign * (hours * 3600 + minutes * 60)
        }
    };
    let days = days_from_civil(year, month, day);
    Some((
        days * 86_400 + hour * 3600 + minute * 60 + second - offset,
        nanos,
    ))
}

/// Days since 1970-01-01 for a civil date: Howard Hinnant's `days_from_civil`, which shifts the
/// epoch to 0000-03-01 so leap days fall at the end of the year and the arithmetic needs no table.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let shifted_month = if month > 2 { month - 3 } else { month + 9 };
    let day_of_year = (153 * shifted_month + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

#[cfg(test)]
mod tests {
    use super::rfc3339;

    /// Checked against the dates the arithmetic gets wrong: the epoch itself, a leap day in a
    /// century year that is one, a leap day in an ordinary leap year, a year end, the day after
    /// February in a century year that is not a leap year, and a time before the epoch. A
    /// conversion error here hands back a credential after it stopped working, or picks the wrong
    /// model as the newest.
    #[test]
    fn a_time_is_converted_to_the_instant_it_names() {
        for (text, seconds) in [
            ("1970-01-01T00:00:00Z", 0),
            ("1969-12-31T23:59:59Z", -1),
            ("2000-02-29T12:00:00Z", 951_825_600),
            ("2024-02-29T00:00:00Z", 1_709_164_800),
            ("2026-09-05T03:04:02+00:00", 1_788_577_442),
            ("2026-12-31T23:59:59Z", 1_798_761_599),
            ("2100-03-01T23:59:59Z", 4_107_628_799),
        ] {
            assert_eq!(rfc3339(text), Some((seconds, 0)), "{text}");
        }
    }

    /// A zone offset moves the instant. Read as UTC regardless, a `+09:00` time is nine hours late
    /// and a session would be kept as good after it expired.
    #[test]
    fn the_zone_offset_is_applied() {
        for text in [
            "2026-09-05T12:04:02+09:00",
            "2026-09-04T22:04:02-05:00",
            "2026-09-05T03:04:02Z",
            "2026-09-05T03:04:02z",
            "2026-09-05t03:04:02Z",
        ] {
            assert_eq!(rfc3339(text), Some((1_788_577_442, 0)), "{text}");
        }
        // Eight minutes later in New York than in Paris is the later instant, though it reads as
        // the earlier text.
        let paris = rfc3339("2026-09-16T20:00:00.5+02:00").expect("paris");
        let new_york = rfc3339("2026-09-16T14:08:37-04:00").expect("new york");
        assert!(new_york > paris, "{new_york:?} {paris:?}");
        assert_eq!(paris, (1_789_581_600, 500_000_000));
    }

    /// The fraction is nanoseconds, kept to nine digits, and a point with no digits after it is not
    /// a time.
    #[test]
    fn a_fraction_is_read_as_nanoseconds() {
        for (text, nanos) in [
            ("2026-09-05T03:04:02.5Z", 500_000_000),
            ("2026-09-05T03:04:02.000000001Z", 1),
            ("2026-09-05T03:04:02.1234567891234Z", 123_456_789),
            ("2026-09-05T03:04:02.5+00:00", 500_000_000),
        ] {
            assert_eq!(rfc3339(text), Some((1_788_577_442, nanos)), "{text}");
        }
        assert_eq!(rfc3339("2026-09-05T03:04:02.Z"), None);
    }

    /// An unreadable time must not become a confident date. Read as one, a garbled value either
    /// pins a session as good long after it expired or forces the CLI on every turn. A time with no
    /// zone, or text after the zone, names no instant, and a sign inside a digit field is not a
    /// digit.
    #[test]
    fn text_that_is_not_an_rfc_3339_time_is_not_guessed_at() {
        for text in [
            "",
            "not a date",
            "yesterday",
            "2026-09-05",
            "2026/09/05T03:04:02Z",
            "2026-09-05 03:04:02Z",
            "2026-13-05T03:04:02Z",
            "2026-00-05T03:04:02Z",
            "2026-09-32T03:04:02Z",
            "2026-09-00T03:04:02Z",
            "2026-09-05T24:04:02Z",
            "2026-09-05T03:60:02Z",
            "2026-09-05T03:04:61Z",
            "xxxx-09-05T03:04:02Z",
            "+026-09-05T03:04:02Z",
            "2026-+9-05T03:04:02Z",
            "2026-09-05T03:04:02",
            "2026-09-05T03:04:02Zjunk",
            "2026-09-05T03:04:02 UTC",
            "2026-09-05T03:04:02+0900",
            "2026-09-05T03:04:02+24:00",
            "2026-09-05T03:04:02+09:60",
            "2026-09-05T03:04:02+0:00:00",
            "2026-09-05T03:04:02+-9:00",
        ] {
            assert_eq!(rfc3339(text), None, "{text} was read as a time");
        }
    }
}
