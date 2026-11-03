//! A cron expression as the times a launchd job or a systemd timer is given, and the text of both
//! (CLI-42).
//!
//! The expression has already been held to five fields of digits and `* / , -` by the caller, so
//! nothing here reads a name or an `@` word. Every function is pure: where the files go and which
//! program installs them is `schedule.rs`'s.

/// The most calendar dictionaries a launchd job is given, so `*/1`-style expressions that expand to
/// the whole minute-by-hour grid cannot write a plist nobody could read.
const MOST_TIMES: usize = 2048;

/// One field's values, and whether the text began with `*`.
#[derive(Debug, PartialEq, Eq)]
struct Field {
    values: Vec<u8>,
    starred: bool,
}

/// The fields of one group of times: each list is every value the field takes.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Group {
    minutes: Vec<u8>,
    hours: Vec<u8>,
    days: Vec<u8>,
    months: Vec<u8>,
    /// 0 is Sunday and 7 is not used.
    weekdays: Vec<u8>,
}

/// One field of the expression: a comma list of `*`, `*/n`, `a`, `a-b` and `a-b/n`.
fn field(text: &str, low: u8, high: u8) -> Option<Field> {
    let mut values = Vec::new();
    for item in text.split(',') {
        let (range, step) = match item.split_once('/') {
            Some((range, step)) => (range, step.parse::<u8>().ok().filter(|n| *n > 0)?),
            None => (item, 1),
        };
        let (from, to) = if range == "*" {
            (low, high)
        } else if let Some((from, to)) = range.split_once('-') {
            (from.parse().ok()?, to.parse().ok()?)
        } else if step == 1 && item == range {
            let only = range.parse().ok()?;
            (only, only)
        } else {
            // `5/10` is not standard cron.
            return None;
        };
        if from < low || to > high || from > to {
            return None;
        }
        values.extend((from..=to).step_by(usize::from(step)));
    }
    values.sort_unstable();
    values.dedup();
    Some(Field {
        values,
        starred: text.starts_with('*'),
    })
}

/// The groups of times `expression` stands for, or `None` where a field is out of range.
///
/// Two groups where both the day of the month and the day of the week are restricted, since cron
/// runs on a day that matches either; one otherwise.
pub(crate) fn groups(expression: &str) -> Option<Vec<Group>> {
    let fields: Vec<&str> = expression.split_whitespace().collect();
    let [minute, hour, day, month, weekday] = fields.as_slice() else {
        return None;
    };
    let minutes = field(minute, 0, 59)?;
    let hours = field(hour, 0, 23)?;
    let days = field(day, 1, 31)?;
    let months = field(month, 1, 12)?;
    let mut weekdays = field(weekday, 0, 7)?;
    // Both 0 and 7 are Sunday.
    for value in &mut weekdays.values {
        *value %= 7;
    }
    weekdays.values.sort_unstable();
    weekdays.values.dedup();
    let every_day: Vec<u8> = (1..=31).collect();
    let every_weekday: Vec<u8> = (0..=6).collect();
    let group = |days: Vec<u8>, weekdays: Vec<u8>| Group {
        minutes: minutes.values.clone(),
        hours: hours.values.clone(),
        days,
        months: months.values.clone(),
        weekdays,
    };
    if days.starred || weekdays.starred {
        Some(vec![group(days.values, weekdays.values)])
    } else {
        Some(vec![
            group(days.values, every_weekday),
            group(every_day, weekdays.values),
        ])
    }
}

/// The key and its values for a field that is restricted, and nothing for one that is every value.
fn restricted<'a>(
    key: &'static str,
    values: &'a [u8],
    every: usize,
) -> Option<(&'static str, &'a [u8])> {
    (values.len() < every).then_some((key, values))
}

/// The `StartCalendarInterval` entries of a launchd job, or `None` past [`MOST_TIMES`].
fn intervals(groups: &[Group]) -> Option<Vec<Vec<(&'static str, u8)>>> {
    let mut all: Vec<Vec<(&'static str, u8)>> = Vec::new();
    for group in groups {
        let mut product: Vec<Vec<(&'static str, u8)>> = vec![Vec::new()];
        let fields = [
            restricted("Minute", &group.minutes, 60),
            restricted("Hour", &group.hours, 24),
            restricted("Day", &group.days, 31),
            restricted("Month", &group.months, 12),
            restricted("Weekday", &group.weekdays, 7),
        ];
        for (key, values) in fields.into_iter().flatten() {
            if product.len().saturating_mul(values.len()) > MOST_TIMES {
                return None;
            }
            product = product
                .iter()
                .flat_map(|so_far| {
                    values.iter().map(move |value| {
                        let mut next = so_far.clone();
                        next.push((key, *value));
                        next
                    })
                })
                .collect();
        }
        all.extend(product);
        if all.len() > MOST_TIMES {
            return None;
        }
    }
    Some(all)
}

/// `text` as the content of an XML element.
fn xml(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '&' => "&amp;".to_string(),
            '<' => "&lt;".to_string(),
            '>' => "&gt;".to_string(),
            '"' => "&quot;".to_string(),
            '\'' => "&apos;".to_string(),
            other => other.to_string(),
        })
        .collect()
}

/// The label a launchd job for entry `id` has.
pub(crate) fn label(id: &str) -> String {
    format!("com.brave.bravebot.schedule.{id}")
}

/// A plist that starts `program schedule run-now id` at the times `groups` stand for. `None` for a
/// program path with a control character, which XML cannot hold, and for more times than
/// [`MOST_TIMES`].
pub(crate) fn plist(groups: &[Group], program: &str, id: &str) -> Option<String> {
    if program.chars().any(char::is_control) {
        return None;
    }
    let mut times = String::new();
    for interval in intervals(groups)? {
        times.push_str("    <dict>\n");
        for (key, value) in interval {
            times.push_str(&format!(
                "      <key>{key}</key>\n      <integer>{value}</integer>\n"
            ));
        }
        times.push_str("    </dict>\n");
    }
    Some(format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
         <plist version=\"1.0\">\n\
         <dict>\n\
         \x20 <key>Label</key>\n  <string>{}</string>\n\
         \x20 <key>ProgramArguments</key>\n  <array>\n    <string>{}</string>\n    <string>schedule</string>\n    <string>run-now</string>\n    <string>{id}</string>\n  </array>\n\
         \x20 <key>StartCalendarInterval</key>\n  <array>\n{times}  </array>\n\
         </dict>\n\
         </plist>\n",
        xml(&label(id)),
        xml(program)
    ))
}

/// The name of the timer and the service for entry `id`, without the suffix.
pub(crate) fn unit(id: &str) -> String {
    format!("bravebot-schedule-{id}")
}

/// `program` as one word of an `ExecStart=` line: in double quotes, with the backslash and the
/// quote escaped and `%` and `$` doubled, which systemd would otherwise expand.
fn exec_word(program: &str) -> String {
    let mut word = String::from("\"");
    for c in program.chars() {
        match c {
            '\\' => word.push_str("\\\\"),
            '"' => word.push_str("\\\""),
            '%' => word.push_str("%%"),
            '$' => word.push_str("$$"),
            other => word.push(other),
        }
    }
    word.push('"');
    word
}

/// A weekday as systemd names it.
fn weekday_name(value: u8) -> &'static str {
    ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"]
        .get(usize::from(value))
        .copied()
        .unwrap_or("Sun")
}

fn list_or_every(values: &[u8], every: usize, width: usize) -> String {
    if values.len() >= every {
        return "*".to_string();
    }
    values
        .iter()
        .map(|value| format!("{value:0width$}"))
        .collect::<Vec<_>>()
        .join(",")
}

/// The `OnCalendar=` value for one group.
fn on_calendar(group: &Group) -> String {
    let weekdays = if group.weekdays.len() >= 7 {
        String::new()
    } else {
        let names: Vec<&str> = group.weekdays.iter().map(|d| weekday_name(*d)).collect();
        format!("{} ", names.join(","))
    };
    format!(
        "{weekdays}*-{}-{} {}:{}:00",
        list_or_every(&group.months, 12, 1),
        list_or_every(&group.days, 31, 1),
        list_or_every(&group.hours, 24, 2),
        list_or_every(&group.minutes, 60, 2)
    )
}

/// The service and the timer units for entry `id`, or `None` for a program path with a line break.
pub(crate) fn units(groups: &[Group], program: &str, id: &str) -> Option<(String, String)> {
    if program.chars().any(char::is_control) {
        return None;
    }
    let service = format!(
        "[Unit]\nDescription=bravebot schedule {id}\n\n[Service]\nType=oneshot\nExecStart={} schedule run-now {id}\n",
        exec_word(program)
    );
    let calendars: String = groups
        .iter()
        .map(|group| format!("OnCalendar={}\n", on_calendar(group)))
        .collect();
    let timer = format!(
        "[Unit]\nDescription=bravebot schedule {id}\n\n[Timer]\n{calendars}\n[Install]\nWantedBy=timers.target\n"
    );
    Some((service, timer))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn only(expression: &str) -> Group {
        let mut all = groups(expression).expect("a valid expression");
        assert_eq!(all.len(), 1, "{expression}");
        all.remove(0)
    }

    #[test]
    fn ranges_lists_and_steps_expand_to_the_values_cron_gives() {
        let group = only("*/15 9-17/4 * 1,6 1-5");
        assert_eq!(group.minutes, [0, 15, 30, 45]);
        assert_eq!(group.hours, [9, 13, 17]);
        assert_eq!(group.months, [1, 6]);
        assert_eq!(group.weekdays, [1, 2, 3, 4, 5]);
        assert_eq!(group.days.len(), 31);
    }

    #[test]
    fn sunday_is_zero_whether_it_was_written_as_zero_or_seven() {
        assert_eq!(only("0 0 * * 7").weekdays, [0]);
        assert_eq!(only("0 0 * * 5-7").weekdays, [0, 5, 6]);
    }

    #[test]
    fn a_value_out_of_range_or_a_step_of_nothing_is_refused() {
        for refused in [
            "60 * * * *",
            "* 24 * * *",
            "* * 0 * *",
            "* * 32 * *",
            "* * * 13 *",
            "* * * * 8",
            "5-1 * * * *",
            "*/0 * * * *",
            "5/10 * * * *",
            "1,,2 * * * *",
            "1-2-3 * * * *",
        ] {
            assert!(groups(refused).is_none(), "{refused}");
        }
    }

    #[test]
    fn a_day_and_a_weekday_that_are_both_restricted_run_on_either() {
        let both = groups("0 9 1 * 1").expect("valid");
        assert_eq!(both.len(), 2);
        assert_eq!(both[0].days, [1]);
        assert_eq!(both[0].weekdays.len(), 7);
        assert_eq!(both[1].weekdays, [1]);
        assert_eq!(both[1].days.len(), 31);
        // A star in either field makes the other the only one that counts.
        assert_eq!(groups("0 9 * * 1").expect("valid").len(), 1);
        assert_eq!(groups("0 9 */2 * 1").expect("valid").len(), 1);
    }

    #[test]
    fn a_plist_names_only_the_fields_that_are_restricted() {
        let text = plist(&groups("30 9 * * 1,5").unwrap(), "/opt/bravebot", "4").expect("a plist");
        assert!(text.contains("<string>com.brave.bravebot.schedule.4</string>"));
        assert!(text.contains(
            "<string>/opt/bravebot</string>\n    <string>schedule</string>\n    <string>run-now</string>\n    <string>4</string>"
        ));
        assert_eq!(text.matches("<key>Minute</key>").count(), 2);
        assert_eq!(text.matches("<key>Weekday</key>").count(), 2);
        assert!(!text.contains("<key>Day</key>"));
        assert!(!text.contains("<key>Month</key>"));
    }

    #[test]
    fn a_plist_escapes_the_program_path_and_refuses_one_xml_cannot_hold() {
        let all = groups("0 9 * * *").unwrap();
        let text = plist(&all, "/a&b<c>/bravebot", "1").expect("a plist");
        assert!(text.contains("<string>/a&amp;b&lt;c&gt;/bravebot</string>"));
        assert!(plist(&all, "/a\nb", "1").is_none());
        assert!(plist(&all, "/a\u{1}b", "1").is_none());
    }

    #[test]
    fn a_plist_for_more_times_than_the_limit_is_refused() {
        // 59 minutes by 23 hours by 30 days is far past the limit.
        let all = groups("0-58 0-22 1-30 * *").unwrap();
        assert!(plist(&all, "/opt/bravebot", "1").is_none());
        assert!(plist(&groups("*/5 * * * *").unwrap(), "/opt/bravebot", "1").is_some());
    }

    #[test]
    fn a_timer_names_the_times_in_systemd_calendar_form() {
        let (_, timer) = units(&groups("*/30 9,17 * 1 1-5").unwrap(), "/opt/b", "2").unwrap();
        assert!(
            timer.contains("OnCalendar=Mon,Tue,Wed,Thu,Fri *-1-* 09,17:00,30:00\n"),
            "{timer}"
        );
        assert!(timer.contains("WantedBy=timers.target"));
        let (_, either) = units(&groups("0 9 1 * 1").unwrap(), "/opt/b", "2").unwrap();
        assert!(either.contains("OnCalendar=*-*-1 09:00:00\n"), "{either}");
        assert!(
            either.contains("OnCalendar=Mon *-*-* 09:00:00\n"),
            "{either}"
        );
    }

    #[test]
    fn a_service_quotes_the_program_and_doubles_what_systemd_would_expand() {
        let all = groups("0 9 * * *").unwrap();
        let (service, _) = units(&all, "/opt/it's \"100%\" $HOME\\x/bravebot", "3").unwrap();
        assert!(
            service.contains(
                "ExecStart=\"/opt/it's \\\"100%%\\\" $$HOME\\\\x/bravebot\" schedule run-now 3\n"
            ),
            "{service}"
        );
        assert!(service.contains("Type=oneshot"));
        assert!(units(&all, "/a\nb", "3").is_none());
    }
}
