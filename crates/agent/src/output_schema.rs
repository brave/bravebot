//! A JSON Schema a person supplied, and the check of a run's reply against it.
//!
//! The schema is the person's: a file named on the command line, trusted as `--settings` is. The
//! reply is the planner's text. Checking one against the other is the driver reading a finished
//! string for an answer about its shape, and the answer reaches an exit status and one field of the
//! result object. It chooses no tool, no route and no destination, and the planner never holds the
//! schema's file.
//!
//! No JSON Schema crate is a dependency, so this checks the subset below and refuses a schema that
//! uses anything else when it is loaded. A keyword ignored would be a constraint the person wrote
//! and the run did not hold the reply to, which is the failure the flag exists to remove.
//!
//! What a [`Mismatch`] names is a position the schema's own `properties` and an array index give,
//! never a key or a value the reply spelt, so a message built from it carries nothing the reply
//! wrote.

use serde_json::{Map, Value};

/// Keywords that describe a schema and constrain nothing.
const ANNOTATIONS: [&str; 6] = [
    "$schema",
    "$id",
    "title",
    "description",
    "default",
    "examples",
];

/// The names `type` may take.
const TYPES: [&str; 7] = [
    "object", "array", "string", "number", "integer", "boolean", "null",
];

/// Why a schema file cannot be used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchemaError {
    /// The file is not JSON.
    NotJson,
    /// A schema, at the position given, is not a JSON object.
    NotAnObject { at: String },
    /// A keyword this build does not check, at the position given.
    Unsupported { at: String, keyword: String },
    /// A supported keyword whose value is not one it can take.
    Malformed { at: String, keyword: String },
}

/// Which constraint a reply broke.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rule {
    /// The reply is not one JSON value.
    NotJson,
    /// The value is of another type.
    Type,
    /// The value is not one of those listed.
    Enum,
    /// The value is not the one allowed.
    Const,
    /// A property the schema requires is absent.
    Required,
    /// An object has a property the schema does not list, where it forbids those.
    Extra,
    /// A string is shorter or longer than allowed.
    Length,
    /// An array has fewer or more items than allowed.
    Count,
    /// A number is below or above what is allowed.
    Range,
}

/// Where a reply broke the schema, and which constraint it broke.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mismatch {
    /// `$` for the reply itself, then `.name` and `[n]` down to the value.
    pub at: String,
    pub rule: Rule,
}

/// A schema this build can hold a reply to.
#[derive(Debug, Clone, PartialEq)]
pub struct OutputSchema {
    root: Value,
}

impl OutputSchema {
    /// Read a schema from the text of its file, refusing anything outside the subset.
    pub fn parse(text: &str) -> Result<Self, SchemaError> {
        let root: Value = serde_json::from_str(text).map_err(|_| SchemaError::NotJson)?;
        vet(&root, "$")?;
        Ok(Self { root })
    }

    /// The schema as the request carries it.
    pub fn value(&self) -> &Value {
        &self.root
    }

    /// Hold a reply to the schema.
    ///
    /// The value on success, written on one line with every control character escaped, so a reply
    /// that spelt a terminal sequence into a string cannot repaint the screen of whoever reads it.
    pub fn check(&self, reply: &str) -> Result<String, Mismatch> {
        let value: Value = serde_json::from_str(reply.trim()).map_err(|_| Mismatch {
            at: "$".to_string(),
            rule: Rule::NotJson,
        })?;
        conforms(&self.root, &value, "$")?;
        let line = serde_json::to_string(&value).map_err(|_| Mismatch {
            at: "$".to_string(),
            rule: Rule::NotJson,
        })?;
        Ok(line.replace('\u{7f}', "\\u007f"))
    }
}

fn unsupported(at: &str, keyword: &str) -> SchemaError {
    SchemaError::Unsupported {
        at: at.to_string(),
        keyword: keyword.to_string(),
    }
}

fn malformed(at: &str, keyword: &str) -> SchemaError {
    SchemaError::Malformed {
        at: at.to_string(),
        keyword: keyword.to_string(),
    }
}

fn is_count(value: &Value) -> bool {
    value.is_u64()
}

fn vet(schema: &Value, at: &str) -> Result<(), SchemaError> {
    let Some(keywords) = schema.as_object() else {
        return Err(SchemaError::NotAnObject { at: at.to_string() });
    };
    for (keyword, value) in keywords {
        let keyword = keyword.as_str();
        if ANNOTATIONS.contains(&keyword) {
            continue;
        }
        let well_formed = match keyword {
            "type" => match value {
                Value::String(name) => TYPES.contains(&name.as_str()),
                Value::Array(names) => {
                    !names.is_empty()
                        && names
                            .iter()
                            .all(|n| n.as_str().is_some_and(|n| TYPES.contains(&n)))
                }
                _ => false,
            },
            "enum" => value.as_array().is_some_and(|options| !options.is_empty()),
            "const" => true,
            "required" => value
                .as_array()
                .is_some_and(|names| names.iter().all(Value::is_string)),
            "properties" => match value.as_object() {
                Some(properties) => {
                    for (name, sub) in properties {
                        vet(sub, &format!("{at}.{name}"))?;
                    }
                    true
                }
                None => false,
            },
            "additionalProperties" => value.is_boolean(),
            "items" => {
                if value.is_object() {
                    vet(value, &format!("{at}[]"))?;
                    true
                } else {
                    false
                }
            }
            "minItems" | "maxItems" | "minLength" | "maxLength" => is_count(value),
            "minimum" | "maximum" => value.is_number(),
            _ => return Err(unsupported(at, keyword)),
        };
        if !well_formed {
            return Err(malformed(at, keyword));
        }
    }
    Ok(())
}

fn conforms(schema: &Value, value: &Value, at: &str) -> Result<(), Mismatch> {
    let Some(keywords) = schema.as_object() else {
        return Ok(());
    };
    let broke = |rule: Rule| Mismatch {
        at: at.to_string(),
        rule,
    };

    if let Some(allowed) = keywords.get("type") {
        let names: Vec<&str> = match allowed {
            Value::String(name) => vec![name.as_str()],
            Value::Array(names) => names.iter().filter_map(Value::as_str).collect(),
            _ => Vec::new(),
        };
        if !names.iter().any(|name| is_of_type(value, name)) {
            return Err(broke(Rule::Type));
        }
    }
    if let Some(Value::Array(options)) = keywords.get("enum")
        && !options.contains(value)
    {
        return Err(broke(Rule::Enum));
    }
    if let Some(only) = keywords.get("const")
        && only != value
    {
        return Err(broke(Rule::Const));
    }

    match value {
        Value::Object(members) => object_conforms(keywords, members, at)?,
        Value::Array(items) => {
            if keywords
                .get("minItems")
                .and_then(Value::as_u64)
                .is_some_and(|least| (items.len() as u64) < least)
                || keywords
                    .get("maxItems")
                    .and_then(Value::as_u64)
                    .is_some_and(|most| (items.len() as u64) > most)
            {
                return Err(broke(Rule::Count));
            }
            if let Some(each) = keywords.get("items") {
                for (index, item) in items.iter().enumerate() {
                    conforms(each, item, &format!("{at}[{index}]"))?;
                }
            }
        }
        Value::String(text) => {
            let length = text.chars().count() as u64;
            if keywords
                .get("minLength")
                .and_then(Value::as_u64)
                .is_some_and(|least| length < least)
                || keywords
                    .get("maxLength")
                    .and_then(Value::as_u64)
                    .is_some_and(|most| length > most)
            {
                return Err(broke(Rule::Length));
            }
        }
        Value::Number(number) => {
            if let Some(n) = number.as_f64()
                && (keywords
                    .get("minimum")
                    .and_then(Value::as_f64)
                    .is_some_and(|least| n < least)
                    || keywords
                        .get("maximum")
                        .and_then(Value::as_f64)
                        .is_some_and(|most| n > most))
            {
                return Err(broke(Rule::Range));
            }
        }
        _ => {}
    }
    Ok(())
}

fn object_conforms(
    keywords: &Map<String, Value>,
    members: &Map<String, Value>,
    at: &str,
) -> Result<(), Mismatch> {
    if let Some(Value::Array(required)) = keywords.get("required") {
        for name in required.iter().filter_map(Value::as_str) {
            if !members.contains_key(name) {
                return Err(Mismatch {
                    at: format!("{at}.{name}"),
                    rule: Rule::Required,
                });
            }
        }
    }
    let listed = keywords.get("properties").and_then(Value::as_object);
    if let Some(listed) = listed {
        for (name, sub) in listed {
            if let Some(member) = members.get(name) {
                conforms(sub, member, &format!("{at}.{name}"))?;
            }
        }
    }
    if keywords.get("additionalProperties") == Some(&Value::Bool(false))
        && members
            .keys()
            .any(|name| listed.is_none_or(|listed| !listed.contains_key(name)))
    {
        return Err(Mismatch {
            at: at.to_string(),
            rule: Rule::Extra,
        });
    }
    Ok(())
}

fn is_of_type(value: &Value, name: &str) -> bool {
    match name {
        "object" => value.is_object(),
        "array" => value.is_array(),
        "string" => value.is_string(),
        "boolean" => value.is_boolean(),
        "null" => value.is_null(),
        "number" => value.is_number(),
        "integer" => match value {
            Value::Number(n) => {
                n.is_i64() || n.is_u64() || n.as_f64().is_some_and(|f| f.fract() == 0.0)
            }
            _ => false,
        },
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn schema(text: &str) -> OutputSchema {
        OutputSchema::parse(text).expect("a schema in the subset")
    }

    const VERDICT: &str = r#"{
        "type": "object",
        "properties": {
            "verdict": {"enum": ["pass", "fail"]},
            "files": {"type": "array", "items": {"type": "string"}, "maxItems": 2}
        },
        "required": ["verdict", "files"],
        "additionalProperties": false
    }"#;

    #[test]
    fn a_reply_that_matches_comes_back_as_one_line() {
        let checked = schema(VERDICT)
            .check("  {\n \"files\": [\"a.rs\"], \"verdict\": \"pass\"}\n")
            .expect("conforms");
        assert_eq!(checked, r#"{"files":["a.rs"],"verdict":"pass"}"#);
    }

    #[test]
    fn prose_is_not_a_value() {
        let broke = schema(VERDICT).check("It passes.").unwrap_err();
        assert_eq!(broke.rule, Rule::NotJson);
        assert_eq!(broke.at, "$");
    }

    #[test]
    fn each_constraint_names_where_it_was_broken() {
        let s = schema(VERDICT);
        let cases = [
            (r#"[]"#, "$", Rule::Type),
            (r#"{"files":[]}"#, "$.verdict", Rule::Required),
            (r#"{"verdict":"maybe","files":[]}"#, "$.verdict", Rule::Enum),
            (
                r#"{"verdict":"pass","files":[1]}"#,
                "$.files[0]",
                Rule::Type,
            ),
            (
                r#"{"verdict":"pass","files":["a","b","c"]}"#,
                "$.files",
                Rule::Count,
            ),
            (
                r#"{"verdict":"pass","files":[],"note":"x"}"#,
                "$",
                Rule::Extra,
            ),
        ];
        for (reply, at, rule) in cases {
            let broke = s.check(reply).unwrap_err();
            assert_eq!((broke.at.as_str(), broke.rule), (at, rule), "{reply}");
        }
    }

    #[test]
    fn a_mismatch_never_carries_what_the_reply_spelt() {
        let broke = schema(VERDICT)
            .check(r#"{"verdict":"pass","files":[],"\u001b[2Jevil":1}"#)
            .unwrap_err();
        assert_eq!(broke.at, "$");
    }

    #[test]
    fn bounds_and_lengths_hold() {
        let s = schema(
            r#"{"type":"object","properties":{
                "n":{"type":"integer","minimum":1,"maximum":3},
                "s":{"type":"string","minLength":2,"maxLength":3}}}"#,
        );
        assert!(s.check(r#"{"n":2,"s":"ab"}"#).is_ok());
        assert!(s.check(r#"{"n":2.0}"#).is_ok());
        assert_eq!(s.check(r#"{"n":0}"#).unwrap_err().rule, Rule::Range);
        assert_eq!(s.check(r#"{"n":4}"#).unwrap_err().rule, Rule::Range);
        assert_eq!(s.check(r#"{"n":1.5}"#).unwrap_err().rule, Rule::Type);
        assert_eq!(s.check(r#"{"s":"a"}"#).unwrap_err().rule, Rule::Length);
        assert_eq!(s.check(r#"{"s":"abcd"}"#).unwrap_err().rule, Rule::Length);
    }

    #[test]
    fn a_type_list_and_const_hold() {
        let s = schema(r#"{"type":["string","null"]}"#);
        assert!(s.check("null").is_ok());
        assert!(s.check(r#""x""#).is_ok());
        assert_eq!(s.check("1").unwrap_err().rule, Rule::Type);
        let only = schema(r#"{"const":"yes"}"#);
        assert!(only.check(r#""yes""#).is_ok());
        assert_eq!(only.check(r#""no""#).unwrap_err().rule, Rule::Const);
    }

    #[test]
    fn a_keyword_outside_the_subset_is_refused_wherever_it_stands() {
        for text in [
            r##"{"$ref":"#/definitions/x"}"##,
            r#"{"oneOf":[{"type":"string"}]}"#,
            r#"{"properties":{"a":{"pattern":"^x"}}}"#,
            r#"{"items":{"format":"date"}}"#,
        ] {
            assert!(
                matches!(
                    OutputSchema::parse(text),
                    Err(SchemaError::Unsupported { .. })
                ),
                "{text}"
            );
        }
    }

    #[test]
    fn a_supported_keyword_with_a_value_it_cannot_take_is_refused() {
        for text in [
            r#"{"type":"text"}"#,
            r#"{"type":[]}"#,
            r#"{"required":"a"}"#,
            r#"{"additionalProperties":{"type":"string"}}"#,
            r#"{"minItems":-1}"#,
            r#"{"items":[{"type":"string"}]}"#,
            r#"{"enum":[]}"#,
        ] {
            assert!(
                matches!(
                    OutputSchema::parse(text),
                    Err(SchemaError::Malformed { .. })
                ),
                "{text}"
            );
        }
        assert_eq!(
            OutputSchema::parse("[]"),
            Err(SchemaError::NotAnObject { at: "$".into() })
        );
        assert_eq!(OutputSchema::parse("{"), Err(SchemaError::NotJson));
    }

    #[test]
    fn annotations_are_accepted_and_constrain_nothing() {
        let s = schema(
            r#"{"$schema":"http://json-schema.org/draft-07/schema#","title":"t",
                "description":"d","default":1,"type":"string"}"#,
        );
        assert!(s.check(r#""x""#).is_ok());
    }

    #[test]
    fn a_delete_character_in_a_string_is_escaped_on_the_way_out() {
        let checked = schema(r#"{"type":"string"}"#)
            .check("\"a\u{7f}b\"")
            .expect("conforms");
        assert_eq!(checked, "\"a\\u007fb\"");
    }
}
