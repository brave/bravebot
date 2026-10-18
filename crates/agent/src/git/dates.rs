//! Dates and times in the form a log prints and a `since` or `until` is read in.

use crate::calendar::{civil_from_days, days_from_civil};

/// A signature's time and zone, or its time in UTC when the zone does not parse.
pub(super) fn when(time: Option<gix_object::date::Time>, seconds: i64) -> (i64, i32) {
    time.map_or((seconds, 0), |time| (time.seconds, time.offset))
}

/// `YYYY-MM-DD` in the signature's own offset, as `git log --date=short` prints it.
pub(super) fn day(seconds: i64, offset: i32) -> String {
    let (y, m, d) = civil_from_days((seconds + i64::from(offset)).div_euclid(86_400));
    format!("{y:04}-{m:02}-{d:02}")
}

/// `YYYY-MM-DD HH:MM:SS +zzzz` in the signature's own offset.
pub(super) fn stamp(seconds: i64, offset: i32) -> String {
    let local = seconds + i64::from(offset);
    let (y, m, d) = civil_from_days(local.div_euclid(86_400));
    let within = local.rem_euclid(86_400);
    let sign = if offset < 0 { '-' } else { '+' };
    let minutes = offset.unsigned_abs() / 60;
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02} {sign}{:02}{:02}",
        within / 3600,
        within % 3600 / 60,
        within % 60,
        minutes / 60,
        minutes % 60
    )
}

/// The start of a `YYYY-MM-DD` day in UTC, in seconds since the epoch.
pub fn parse_day(text: &str) -> Option<i64> {
    let bytes = text.as_bytes();
    let shaped = bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(i, b)| i == 4 || i == 7 || b.is_ascii_digit());
    if !shaped {
        return None;
    }
    let y: i64 = text[..4].parse().ok()?;
    let m: u32 = text[5..7].parse().ok()?;
    let d: u32 = text[8..].parse().ok()?;
    if !(1..=12).contains(&m) || d == 0 {
        return None;
    }
    let (next_y, next_m) = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
    let length = days_from_civil(next_y, next_m, 1) - days_from_civil(y, m, 1);
    if i64::from(d) > length {
        return None;
    }
    Some(days_from_civil(y, m, d) * 86_400)
}
