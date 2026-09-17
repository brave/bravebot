// Turning parsed catalogs into the Rust the crate exposes.
//
// The build script includes this file, so it must stay dependency-free, and the crate compiles it
// again under `cfg(test)`: what a catalog compiles to is decided here, and a rule that only a
// build script can reach is a rule nothing can pin. Nothing in here runs while the agent does; it
// writes the code that does.

use crate::catalog::*;

/// One catalog as the build script holds it: what it is named for, what language its words are
/// in, and what it defines.
pub struct Catalog {
    /// The BCP-47 tag the file is named for.
    pub tag: String,
    pub language: String,
    pub variant: String,
    pub messages: std::collections::BTreeMap<String, Message>,
}

impl Catalog {
    /// The catalog whose words a reader of this one is shown for `id`.
    ///
    /// Its own when it defines the message, the reference when it does not: a translation may
    /// leave a message out and fall back (LOCALE-3), and what reaches the screen then is the
    /// reference's prose. One catalog answers for both the words and the plural rules that form
    /// them, so the two cannot be taken from different places. French rules applied to an English
    /// sentence read wrongly only to a French speaker, which is a defect no review in English
    /// finds.
    fn words_for<'a>(&'a self, id: &str, reference: &'a Catalog) -> &'a Catalog {
        if self.messages.contains_key(id) {
            self
        } else {
            reference
        }
    }

    /// The message itself. A catalog `words_for` answered with has it, by construction.
    fn message(&self, id: &str) -> &Message {
        self.messages
            .get(id)
            .unwrap_or_else(|| panic!("locales/{}.ftl defines `{id}`", self.tag))
    }
}

pub fn messages(catalogs: &[Catalog], reference: &Catalog) -> String {
    let mut out = String::new();
    for (id, source) in &reference.messages {
        let args = arguments(&source.value);
        // Which catalog each locale is shown this message out of, which is not always its own.
        let from: Vec<&Catalog> = catalogs
            .iter()
            .map(|catalog| catalog.words_for(id, reference))
            .collect();

        if args.is_empty() {
            out.push_str(&constant(id, catalogs, &from));
        } else {
            out.push_str(&structure(id, catalogs, &from, &args));
        }
    }
    out
}

/// A message with no arguments: one table, indexed by locale.
fn constant(id: &str, catalogs: &[Catalog], from: &[&Catalog]) -> String {
    let table = from
        .iter()
        .map(|catalog| match &catalog.message(id).value {
            Value::Pattern(parts) => literal(parts),
            Value::Select { .. } => unreachable!("a select takes the argument it selects on"),
        })
        .collect::<Vec<_>>()
        .join(", ");

    format!(
        "\n#[allow(dead_code)]\npub fn {}() -> &'static str {{\n    \
         const TEXT: [&str; {}] = [{table}];\n    TEXT[crate::locale() as usize]\n}}\n",
        snake_case(id),
        catalogs.len()
    )
}

/// A message with arguments: a struct whose fields are those arguments, by name.
///
/// A struct rather than a function so the call site names what it passes, in any order, and so a
/// forgotten argument is a missing-field error naming the field rather than a count mismatch.
fn structure(id: &str, catalogs: &[Catalog], from: &[&Catalog], args: &[(String, bool)]) -> String {
    let name = camel_case(id);
    let parameters = args
        .iter()
        .enumerate()
        .map(|(i, _)| format!("T{i}"))
        .collect::<Vec<_>>()
        .join(", ");
    let bounds = args
        .iter()
        .enumerate()
        .map(|(i, (_, selects))| {
            if *selects {
                format!("T{i}: Into<crate::Count>")
            } else {
                format!("T{i}: ::std::fmt::Display")
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
    let fields = args
        .iter()
        .enumerate()
        .map(|(i, (arg, _))| format!("    pub {arg}: T{i},\n"))
        .collect::<String>();

    let mut out = format!(
        "\n#[allow(dead_code)]\npub struct {name}<{parameters}> {{\n{fields}}}\n\n\
         #[allow(dead_code)]\nimpl<{bounds}> {name}<{parameters}> {{\n    \
         // A locale that says the same thing without one of the arguments leaves it unread.\n    \
         #[allow(unused_variables)]\n    \
         pub fn render(self) -> String {{\n        let mut out = String::new();\n"
    );
    for (arg, selects) in args {
        if *selects {
            out.push_str(&format!(
                "        let {arg}: crate::Count = self.{arg}.into();\n"
            ));
        } else {
            out.push_str(&format!("        let {arg} = self.{arg};\n"));
        }
    }

    out.push_str("        match crate::locale() {\n");
    // The locale being asked for and the catalog the words come from are two different things,
    // and a plural is formed by the rules of the second.
    for (catalog, from) in catalogs.iter().zip(from) {
        out.push_str(&format!("            Locale::{} => {{\n", catalog.variant));
        out.push_str(&render(&from.message(id).value, &from.language, 16));
        out.push_str("            }\n");
    }
    out.push_str("        }\n        out\n    }\n}\n");
    out
}

/// The body that appends one locale's text to `out`.
fn render(value: &Value, language: &str, indent: usize) -> String {
    let pad = " ".repeat(indent);
    let Value::Select {
        arg,
        variants,
        default,
    } = value
    else {
        let Value::Pattern(parts) = value else {
            unreachable!("a value is a pattern or a select")
        };
        return append(parts, &pad);
    };

    let others: Vec<&Variant> = variants
        .iter()
        .enumerate()
        .filter(|(index, _)| index != default)
        .map(|(_, variant)| variant)
        .collect();

    // A select whose only variant is the default decides nothing, so it reads the count for
    // display and never asks which category it is in.
    if others.is_empty() {
        return append(&variants[*default].parts, &pad);
    }

    let mut out = String::new();
    if others.iter().any(|v| matches!(v.key, Key::Exact(_))) {
        out.push_str(&format!("{pad}let exact = {arg}.get();\n"));
    }
    if others.iter().any(|v| matches!(v.key, Key::Category(_))) {
        out.push_str(&format!(
            "{pad}let category = crate::plural::category({language:?}, {arg}.get());\n"
        ));
    }

    for (position, variant) in others.iter().enumerate() {
        let condition = match &variant.key {
            Key::Exact(n) => format!("exact == {n}"),
            Key::Category(c) => format!("category == {c:?}"),
        };
        let keyword = if position == 0 { "if" } else { "} else if" };
        out.push_str(&format!("{pad}{keyword} {condition} {{\n"));
        out.push_str(&append(&variant.parts, &format!("{pad}    ")));
    }
    out.push_str(&format!("{pad}}} else {{\n"));
    out.push_str(&append(&variants[*default].parts, &format!("{pad}    ")));
    out.push_str(&format!("{pad}}}\n"));
    out
}

fn append(parts: &[Part], pad: &str) -> String {
    let mut out = String::new();
    for part in parts {
        match part {
            // A one-character run becomes a `push`, because clippy is right that it should and
            // because clippy reads what is generated here as readily as what is written by hand.
            Part::Text(text) => {
                let mut chars = text.chars();
                match (chars.next(), chars.next()) {
                    (Some(only), None) => {
                        out.push_str(&format!("{pad}out.push({only:?});\n"));
                    }
                    _ => out.push_str(&format!("{pad}out.push_str({text:?});\n")),
                }
            }
            Part::Arg(name) => {
                out.push_str(&format!("{pad}out.push_str(&{name}.to_string());\n"));
            }
        }
    }
    out
}

/// A message with no arguments is one run of text, written back out as a Rust literal.
fn literal(parts: &[Part]) -> String {
    let text: String = parts
        .iter()
        .map(|part| match part {
            Part::Text(text) => text.as_str(),
            Part::Arg(_) => unreachable!("a message with an argument is not a constant"),
        })
        .collect();
    format!("{text:?}")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A message that counts, as the reference states it.
    const ROUNDS: &str = "rounds = { $rounds ->\n    [one] sent back { $rounds } time\n   *[other] sent back { $rounds } times\n    }\n";

    /// The same message translated, which French says with its own two forms.
    const ROUNDS_IN_FRENCH: &str = "rounds = { $rounds ->\n    [one] renvoyé { $rounds } fois\n   *[other] renvoyés { $rounds } fois\n    }\n";

    fn catalog(tag: &str, source: &str) -> Catalog {
        let language = tag.split('-').next().expect("a tag has a language");
        Catalog {
            tag: tag.to_string(),
            language: language.to_string(),
            variant: camel_case(&tag.to_lowercase()),
            messages: parse(source)
                .expect("the fixture parses")
                .into_iter()
                .map(|message| (message.id.clone(), message))
                .collect(),
        }
    }

    /// The arm of the generated `match` that a request for `variant` runs.
    fn arm(generated: &str, variant: &str) -> String {
        let wanted = format!("{variant} => ");
        generated
            .split("Locale::")
            .find(|chunk| chunk.starts_with(&wanted))
            .unwrap_or_else(|| panic!("no arm for Locale::{variant} in:\n{generated}"))
            .to_string()
    }

    /// A translation may leave a message out, and then what a French reader is shown is the
    /// reference's English words. Deciding the variant by French rules would hand them "sent
    /// back 0 time" out of an English sentence, since French counts zero as singular and English
    /// does not. It is a defect only a speaker of the language can see, and one no review in
    /// English catches.
    #[test]
    fn a_message_a_translation_omits_is_pluralised_by_the_language_it_is_written_in() {
        let catalogs = vec![catalog("en-US", ROUNDS), catalog("fr", "")];
        let generated = messages(&catalogs, &catalogs[0]);

        let french = arm(&generated, "Fr");
        assert!(
            french.contains(r#"category("en", rounds.get())"#),
            "the fallback took the reader's rules rather than the catalog's:\n{french}"
        );
        assert!(
            french.contains(r#"out.push_str("sent back ")"#),
            "the fallback did not take the reference's words:\n{french}"
        );
    }

    /// The other half of the same rule: a catalog that has the message says it in its own words,
    /// which are its own language's to pluralise.
    #[test]
    fn a_message_a_translation_has_is_pluralised_by_that_translation_s_rules() {
        let catalogs = vec![catalog("en-US", ROUNDS), catalog("fr", ROUNDS_IN_FRENCH)];
        let generated = messages(&catalogs, &catalogs[0]);

        let french = arm(&generated, "Fr");
        assert!(
            french.contains(r#"category("fr", rounds.get())"#),
            "a translation's own plurals were formed by the reference's rules:\n{french}"
        );
        assert!(
            french.contains(r#"out.push_str("renvoyé ")"#),
            "the translation's own words were not used:\n{french}"
        );
        assert!(
            arm(&generated, "EnUs").contains(r#"category("en", rounds.get())"#),
            "the reference stopped being formed by its own rules:\n{generated}"
        );
    }

    /// A message with no arguments falls back the same way, and the table it compiles to holds
    /// the reference's text in the omitting locale's slot.
    #[test]
    fn a_message_with_no_arguments_falls_back_to_the_reference_text() {
        let catalogs = vec![catalog("en-US", "greeting = hello\n"), catalog("fr", "")];
        let generated = messages(&catalogs, &catalogs[0]);

        assert!(
            generated.contains(r#"["hello", "hello"]"#),
            "the omitting locale's slot is not the reference's text:\n{generated}"
        );
    }
}
