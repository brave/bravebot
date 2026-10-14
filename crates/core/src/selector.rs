//! Selector terms: `key:value`, where a value is a bare word or a double-quoted phrase. The one
//! grammar a connector step's `select`, `with` and `take` are read in, the same as the credential
//! authority's.

use crate::slot::SlotId;

/// The terms of a selector, in order.
pub fn terms(select: &str) -> Result<Vec<(String, String)>, String> {
    let mut terms = Vec::new();
    let mut rest = select.trim_start();
    while !rest.is_empty() {
        let (key, after) = rest
            .split_once(':')
            .ok_or_else(|| format!("'{rest}' is not a key:value term"))?;
        if key.is_empty() || !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Err(format!("'{key}' is not a selector key"));
        }
        let (value, remaining) = match after.strip_prefix('"') {
            Some(quoted) => {
                let end = quoted
                    .find('"')
                    .ok_or_else(|| format!("'{key}' opens a quote it does not close"))?;
                (&quoted[..end], &quoted[end + 1..])
            }
            None => {
                let end = after.find(char::is_whitespace).unwrap_or(after.len());
                (&after[..end], &after[end..])
            }
        };
        if value.is_empty() || value.len() > 100 || value.chars().any(char::is_control) {
            return Err(format!("'{key}' has no usable value"));
        }
        terms.push((key.to_string(), value.to_string()));
        rest = remaining.trim_start();
    }
    Ok(terms)
}

/// Whether a term matches a pattern from a definition. A trailing `*` stands for the rest of
/// the value.
pub fn matches(pattern: &str, key: &str, value: &str) -> bool {
    let Some((want_key, want_value)) = pattern.split_once(':') else {
        return false;
    };
    if want_key != key {
        return false;
    }
    match want_value.strip_suffix('*') {
        Some(prefix) => value.starts_with(prefix),
        None => want_value == value,
    }
}

/// The terms of an act's `take`, each `key:slot.field`: a `with` value, the fetch slot it is taken
/// from, and the field that fetch recorded. Read as a selector, so its keys and values are held to
/// the same rules as every other term.
pub fn take_terms(take: &str) -> Result<Vec<(String, SlotId, String)>, String> {
    terms(take)?
        .into_iter()
        .map(|(key, from)| {
            let (slot, field) = from
                .split_once('.')
                .filter(|(slot, field)| !slot.is_empty() && !field.is_empty())
                .ok_or_else(|| format!("'take' {key}:{from} does not name a slot and a field"))?;
            Ok((key, SlotId::new(slot), field.to_string()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A selector is routing a person approves, so every way it can be malformed is refused rather
    /// than read as something nobody wrote.
    #[test]
    fn a_malformed_term_is_refused() {
        for (select, why) in [
            ("state", "is not a key:value term"),
            (":open", "is not a selector key"),
            ("sta-te:open", "is not a selector key"),
            ("subject:\"two words", "opens a quote it does not close"),
            ("state:", "has no usable value"),
            ("subject:\"\"", "has no usable value"),
            ("subject:\"a\tb\"", "has no usable value"),
        ] {
            let refused = terms(select).expect_err(select);
            assert!(refused.contains(why), "{select}: {refused}");
        }
        let long = format!("text:{}", "a".repeat(101));
        assert!(terms(&long).unwrap_err().contains("has no usable value"));
        assert!(terms(&format!("text:{}", "a".repeat(100))).is_ok());
    }

    #[test]
    fn a_pattern_matches_its_key_and_its_value_or_the_rest_of_it() {
        assert!(matches("state:open", "state", "open"));
        assert!(!matches("state:open", "state", "closed"));
        assert!(!matches("state:open", "label", "open"));
        assert!(matches("label:*", "label", "bug"));
        assert!(matches("sort:created-*", "sort", "created-asc"));
        assert!(!matches("sort:created-*", "sort", "updated-asc"));
        assert!(!matches("no-colon", "no-colon", ""));
    }

    /// A take names a slot and a field for every value, so a value cannot be taken from nowhere or
    /// from a whole slot.
    #[test]
    fn a_take_names_a_slot_and_a_field_for_each_value() {
        assert_eq!(
            take_terms("branch:pr.head").unwrap(),
            vec![("branch".to_string(), SlotId::new("pr"), "head".to_string())]
        );
        for bad in ["branch:pr", "branch:.head", "branch:pr.", "branch"] {
            assert!(take_terms(bad).is_err(), "{bad}");
        }
    }
}
