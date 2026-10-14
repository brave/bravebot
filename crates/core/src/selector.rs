//! Selector terms: `key:value`, where a value is a bare word or a double-quoted phrase. The
//! grammar a connector step's `select` is read in, the same as the credential authority's.

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
