//! The named forms a field's value may be required to have. A closed set, shared by every
//! definition: a new form is added to [`FORMS`] under a name, and a definition naming any other is
//! refused.
//!
//! The credential authority checks the same forms again before it substitutes a value into a call,
//! with its own copy of these rules, which must stay the same.

/// The test a value of one form passes.
type Rule = fn(&str) -> bool;

/// Each form a surface may name, with its rule.
const FORMS: &[(&str, Rule)] = &[
    ("owner/name", owner_name),
    ("positive-integer", positive_integer),
    ("branch", branch),
    ("relative-path", relative_path),
    ("address", address),
    ("addresses", addresses),
    ("opaque-id", opaque_id),
    ("date", date),
    ("datetime", datetime),
    ("service-id", service_id),
    ("iata", iata),
    ("date-dmy", date_dmy),
    ("personal-name", personal_name),
    ("e164", e164),
    ("country-code", country_code),
    ("hex-id", hex_id),
    ("local-datetime", local_datetime),
    ("time-zone", time_zone),
];

/// Whether a form is one a surface may name.
pub fn known(form: &str) -> bool {
    FORMS.iter().any(|(name, _)| *name == form)
}

/// A value of `form` to show the planner, where the form's spelling is not obvious from its name.
pub fn example(form: &str) -> Option<&'static str> {
    match form {
        "date" => Some("2026-10-09"),
        "datetime" => Some("2026-10-09T14:00:00+01:00"),
        "date-dmy" => Some("09/10/2026"),
        "iata" => Some("LHR"),
        "e164" => Some("+447700900123"),
        "country-code" => Some("GB"),
        "local-datetime" => Some("2026-10-09T14:00:00"),
        "time-zone" => Some("Europe/London"),
        _ => None,
    }
}

/// Whether `value` has `form`.
pub fn check(form: &str, value: &str) -> Result<(), String> {
    if value.is_empty() || value.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err("is empty or holds a space or control character".to_string());
    }
    let Some((_, fine)) = FORMS.iter().find(|(name, _)| *name == form) else {
        return Err(format!("names a form nothing defines, '{form}'"));
    };
    if fine(value) {
        Ok(())
    } else {
        Err(format!("is not a {form}"))
    }
}

fn segments_fine(value: &str) -> bool {
    value.split('/').all(|segment| {
        !segment.is_empty()
            && segment != "."
            && segment != ".."
            && segment
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
    })
}

fn owner_name(value: &str) -> bool {
    value.split('/').count() == 2 && segments_fine(value)
}

fn positive_integer(value: &str) -> bool {
    value.len() <= 10 && !value.starts_with('0') && value.chars().all(|c| c.is_ascii_digit())
}

// branch: at most 100; segments of [A-Za-z0-9._-], none empty, "." or ".."; not starting with "-";
// not ending in ".lock" or "."; not "HEAD"; no "..", "@{" or segment starting with "."; not starting
// with "refs/", "heads/", "tags/" or "remotes/", which git reads as another ref.
fn branch(value: &str) -> bool {
    value.len() <= 100
        && !value.starts_with('-')
        && !value.ends_with(".lock")
        && !value.ends_with('.')
        && value != "HEAD"
        && !value.contains("..")
        && !value.contains("@{")
        && !value.split('/').any(|segment| segment.starts_with('.'))
        && !["refs/", "heads/", "tags/", "remotes/"]
            .iter()
            .any(|prefix| value.starts_with(prefix))
        && segments_fine(value)
}

fn relative_path(value: &str) -> bool {
    value.len() <= 255 && segments_fine(value)
}

fn opaque_id(value: &str) -> bool {
    value.len() <= 64
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
}

/// At most 2048 of letters, digits and `. _ = -`, not dots alone, so it stays one path segment:
/// an id such as `1712345678.000100` or a long base64 one.
fn service_id(value: &str) -> bool {
    value.len() <= 2048
        && value.chars().any(|c| c != '.')
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '=' | '-'))
}

/// 1 to 50 letters, hyphens and apostrophes, one word: a traveller's given or family name.
fn personal_name(value: &str) -> bool {
    (1..=50).contains(&value.chars().count())
        && value.chars().any(char::is_alphabetic)
        && value
            .chars()
            .all(|c| c.is_alphabetic() || matches!(c, '-' | '\''))
}

/// A phone number in E.164: `+` then 8 to 15 digits, the first not 0.
fn e164(value: &str) -> bool {
    value.len() >= 9
        && value.len() <= 16
        && value.starts_with('+')
        && !value[1..].starts_with('0')
        && value[1..].bytes().all(|b| b.is_ascii_digit())
}

/// A wall-clock time with no offset, `YYYY-MM-DDThh:mm:ss`, read in a time zone named beside it.
fn local_datetime(value: &str) -> bool {
    datetime(&format!("{value}Z"))
}

/// An IANA time zone name such as `Europe/London`: segments of letters, digits and `_ + -`, each
/// starting with a letter, at most 64 in all.
fn time_zone(value: &str) -> bool {
    value.len() <= 64
        && value.split('/').all(|s| {
            !s.is_empty()
                && s.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
                && s.chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '+' | '-'))
        })
}

/// An ISO 3166-1 alpha-2 country code: two upper-case letters such as `GB`.
fn country_code(value: &str) -> bool {
    value.len() == 2 && value.bytes().all(|b| b.is_ascii_uppercase())
}

/// 7 to 64 hexadecimal digits: a content hash such as a commit id, whole or abbreviated.
fn hex_id(value: &str) -> bool {
    (7..=64).contains(&value.len()) && value.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Three upper-case letters: an airport or city code such as `LHR` or `NYC`.
fn iata(value: &str) -> bool {
    value.len() == 3 && value.bytes().all(|b| b.is_ascii_uppercase())
}

/// A calendar day that exists, `dd/mm/yyyy`, as some services take one.
fn date_dmy(value: &str) -> bool {
    let b = value.as_bytes();
    b.len() == 10
        && b[2] == b'/'
        && b[5] == b'/'
        && date(&format!("{}-{}-{}", &value[6..], &value[3..5], &value[..2]))
}

/// A calendar day, `YYYY-MM-DD`, that exists.
fn date(value: &str) -> bool {
    let b = value.as_bytes();
    let digits = |r: std::ops::Range<usize>| b[r.clone()].iter().all(u8::is_ascii_digit);
    if b.len() != 10
        || b[4] != b'-'
        || b[7] != b'-'
        || !digits(0..4)
        || !digits(5..7)
        || !digits(8..10)
    {
        return false;
    }
    let number = |r: std::ops::Range<usize>| value[r].parse::<u32>().unwrap_or(0);
    let (year, month, day) = (number(0..4), number(5..7), number(8..10));
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    (1..=days).contains(&day)
}

/// An RFC 3339 instant with its offset and no fraction: `YYYY-MM-DDThh:mm:ss` then `Z` or
/// `+hh:mm` or `-hh:mm`, so the service never guesses a time zone.
fn datetime(value: &str) -> bool {
    // Every cut below is at a byte position, which is a character boundary only in ASCII.
    if !value.is_ascii() {
        return false;
    }
    let Some((day, time)) = value.split_once('T') else {
        return false;
    };
    let (clock, offset) = match time.strip_suffix('Z') {
        Some(clock) => (clock, None),
        None if time.len() > 6 => {
            let (clock, offset) = time.split_at(time.len() - 6);
            (clock, Some(offset))
        }
        None => return false,
    };
    let hhmm = |s: &str, hours: u32| {
        let b = s.as_bytes();
        b.len() == 5
            && b[2] == b':'
            && b.iter()
                .enumerate()
                .all(|(i, c)| i == 2 || c.is_ascii_digit())
            && s[..2].parse::<u32>().is_ok_and(|h| h < hours)
            && s[3..].parse::<u32>().is_ok_and(|m| m < 60)
    };
    let offset_fine =
        offset.is_none_or(|o| (o.starts_with('+') || o.starts_with('-')) && hhmm(&o[1..], 24));
    date(day)
        && clock.len() == 8
        && clock.as_bytes()[5] == b':'
        && hhmm(&clock[..5], 24)
        && clock[6..].bytes().all(|c| c.is_ascii_digit())
        && clock[6..].parse::<u32>().is_ok_and(|s| s < 60)
        && offset_fine
}

// The same list as the authority's `addresses` form.
fn addresses(value: &str) -> bool {
    let each: Vec<&str> = value.split(',').collect();
    let mut seen: Vec<String> = each.iter().map(|a| a.to_ascii_lowercase()).collect();
    seen.sort();
    seen.dedup();
    value.len() <= 900
        && each.len() <= 10
        && seen.len() == each.len()
        && each.iter().all(|a| address(a))
}

fn address(value: &str) -> bool {
    match value.split_once('@') {
        Some((local, domain)) => {
            value.len() <= 254 && address_local_fine(local) && address_domain_fine(domain)
        }
        None => false,
    }
}

/// Letters, digits and `. _ % + ' -` only, which keeps one approved recipient one recipient.
/// The same grammar as the authority's `address` form.
fn address_local_fine(local: &str) -> bool {
    (1..=64).contains(&local.len())
        && local
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '%' | '+' | '\'' | '-'))
}

/// Dot-separated letter-digit-hyphen labels, at least two.
fn address_domain_fine(domain: &str) -> bool {
    domain.split('.').count() >= 2
        && domain.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        })
}

#[cfg(test)]
mod tests {
    use super::check;

    #[test]
    fn a_branch_is_one_git_would_read_as_a_new_branch_and_nothing_else() {
        for good in ["bravebot/fix-x", "fix_1.2", "v1.0-docs", "a/b.c"] {
            assert!(check("branch", good).is_ok(), "{good} refused");
        }
        for bad in [
            "refs/heads/main",
            "heads/main",
            "tags/v1",
            "remotes/origin/main",
            "HEAD",
            "a..b",
            "a@{1}",
            ".hidden",
            "a/.hidden",
            "fix.",
            "-x",
            "fix.lock",
            "a//b",
            "a/../b",
            &"x".repeat(101),
        ] {
            assert!(check("branch", bad).is_err(), "{bad} passed");
        }
    }

    #[test]
    fn an_address_is_one_recipient_and_a_real_domain() {
        for good in [
            "someone@example.com",
            "a.b+tag@mail.example.co.uk",
            "o'brien@x.org",
        ] {
            assert!(check("address", good).is_ok(), "{good} refused");
        }
        for bad in [
            "a,b@evil.com",
            "a;b@evil.com",
            "<a>@evil.com",
            "\"a\"@evil.com",
            "a(b)@evil.com",
            "a\\b@evil.com",
            "a:b@evil.com",
            "a@b@evil.com",
            "a@evil",
            "a@.evil.com",
            "a@-evil.com",
            "a@evil..com",
            "a@evil.com,b@x.com",
            "me",
        ] {
            assert!(check("address", bad).is_err(), "{bad} passed");
        }
    }

    #[test]
    fn a_local_datetime_and_a_time_zone() {
        assert!(check("local-datetime", "2026-10-10T10:00:00").is_ok());
        for bad in [
            "2026-10-10T10:00:00Z",
            "2026-10-10T10:00",
            "2026-10-10T24:00:00",
            "2026-10-10Té0:00:00",
        ] {
            assert!(check("local-datetime", bad).is_err(), "{bad} passed");
        }
        for good in [
            "Europe/London",
            "America/Argentina/Buenos_Aires",
            "UTC",
            "Etc/GMT+5",
        ] {
            assert!(check("time-zone", good).is_ok(), "{good} refused");
        }
        for bad in [
            "Europe/../etc",
            "/Europe/London",
            "Europe London",
            "Europe/London?x=1",
        ] {
            assert!(check("time-zone", bad).is_err(), "{bad} passed");
        }
    }

    #[test]
    fn a_personal_name_and_an_e164_number() {
        for good in ["Ali", "O'Brien", "Jean-Luc", "Zoë"] {
            assert!(check("personal-name", good).is_ok(), "{good} refused");
        }
        for bad in ["Ali Shahin", "x1", "a<b", "", &"a".repeat(51)] {
            assert!(check("personal-name", bad).is_err(), "{bad} passed");
        }
        assert!(check("e164", "+447700900123").is_ok());
        for bad in [
            "447700900123",
            "+0447700900",
            "+44 7700",
            "+12345",
            "+1234567890123456",
        ] {
            assert!(check("e164", bad).is_err(), "{bad} passed");
        }
    }

    #[test]
    fn an_iata_code_and_a_day_month_year_date() {
        assert!(check("iata", "LHR").is_ok());
        for bad in ["lhr", "LH", "LHRX", "L1R"] {
            assert!(check("iata", bad).is_err(), "{bad}");
        }
        assert!(check("country-code", "GB").is_ok());
        assert!(check("country-code", "gb").is_err() && check("country-code", "GBR").is_err());
        assert!(check("date-dmy", "20/11/2026").is_ok());
        for bad in ["2026-11-20", "31/11/2026", "20/11/26", "20-11-2026"] {
            assert!(check("date-dmy", bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn a_service_id_is_one_path_segment() {
        for good in [
            "1712345678.000100",
            "AAMkAGI2TG93AAA=",
            "C024BE91L",
            &"a".repeat(2048),
        ] {
            assert!(check("service-id", good).is_ok(), "{good} refused");
        }
        for bad in ["..", ".", "a/b", "a b", "a?b", "%2e%2e", &"a".repeat(2049)] {
            assert!(check("service-id", bad).is_err(), "{bad} passed");
        }
    }

    #[test]
    fn a_date_and_a_datetime_are_real_instants_with_their_zone() {
        for good in ["2026-10-09", "2028-02-29", "2026-12-31"] {
            assert!(check("date", good).is_ok(), "{good} refused");
        }
        for bad in [
            "2026-02-29",
            "2026-13-01",
            "2026-04-31",
            "26-10-09",
            "2026/10/09",
            "2026-10-9",
        ] {
            assert!(check("date", bad).is_err(), "{bad} passed");
        }
        for good in [
            "2026-10-09T10:00:00+01:00",
            "2026-10-09T23:59:59Z",
            "2026-10-09T00:00:00-05:30",
        ] {
            assert!(check("datetime", good).is_ok(), "{good} refused");
        }
        for bad in [
            "2026-10-09T10:00:00",
            "2026-10-09T10:00+01:00",
            "2026-10-09T24:00:00Z",
            "2026-10-09T10:00:00.5Z",
            "2026-10-09 10:00:00Z",
            "2026-10-09T10:00:00+1:00",
            "2026-02-30T10:00:00Z",
            "2026-10-09Taé12345",
            "2026-10-09T10:00:00+01:0é",
        ] {
            assert!(check("datetime", bad).is_err(), "{bad} passed");
        }
    }

    #[test]
    fn a_form_nothing_defines_is_refused_after_the_value_is_held_to_no_space() {
        assert_eq!(
            check("integer", "1").unwrap_err(),
            "names a form nothing defines, 'integer'"
        );
        assert!(check("integer", "").unwrap_err().contains("space"));
    }
}
