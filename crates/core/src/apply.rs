//! The shape of an `apply` step, and what a processor's proposal for it resolves to.
//!
//! A plan fixes the tools a step may call and, for each argument, where its value may come from:
//! the plan's own text, a field of an entry of a list a fetch returned, short text the processor
//! wrote for a thing the call creates, or the value an earlier row of the same step gave such an
//! argument. A processor proposes rows of those tools by number. What the rows resolve to is put to
//! the person as one list, and only the rows they keep are called (`docs/design/manifest-apply.md`).
//!
//! Nothing here reads a proposal. The types are filled by `bravebot-choice`, through the policy
//! layer's gate for that one read, and carried by the driver without being looked at.

use std::collections::{BTreeMap, BTreeSet};

/// Where one argument of a tool may take its value from, fixed by the plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    /// The plan's own text.
    Literal(String),
    /// A field of one entry of the list, copied byte for byte.
    Entry(String),
    /// Short text the processor wrote, for a thing the call creates.
    New,
    /// The value an earlier row of the tool at this index gave the named argument.
    Row { tool: usize, argument: String },
}

/// One tool an `apply` step may call, with the kind of each of its arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tool {
    pub name: String,
    pub arguments: Vec<(String, Kind)>,
}

/// Everything about an `apply` step the plan fixed before anything was read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shape {
    pub tools: Vec<Tool>,
    pub max_rows: usize,
    pub max_new: usize,
}

impl Shape {
    /// The fields an entry must have, as text, to count as an entry of the list: those an
    /// argument of any tool takes.
    pub fn entry_fields(&self) -> BTreeSet<String> {
        self.tools
            .iter()
            .flat_map(|tool| &tool.arguments)
            .filter_map(|(_, kind)| match kind {
                Kind::Entry(field) => Some(field.clone()),
                _ => None,
            })
            .collect()
    }
}

/// The most entries a list may hold, so an entry's number is at most three digits.
pub const LARGEST_LIST: usize = 999;

/// The longest new text a row may give, in characters.
pub const LONGEST_NEW: usize = 64;

/// The entries of a list: for each, the value of each field an argument takes, in list order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Entries(pub Vec<BTreeMap<String, String>>);

/// A list as a processor is shown it, and its entries as the rows will be resolved against them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Numbered {
    pub shown: String,
    pub entries: Entries,
}

/// Where a resolved value came from, as the person is told.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// The plan wrote it.
    Plan,
    /// The entry of the list with this number, from 1.
    Entry(usize),
    /// The processor wrote it.
    Processor,
    /// The call with this index in the resolved list gave it.
    Call(usize),
}

/// One call a proposal resolved to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Call {
    /// The index of the tool in the shape.
    pub tool: usize,
    /// The tool's name.
    pub name: String,
    /// Each argument with its value and where the value came from, in the shape's order.
    pub arguments: Vec<(String, String, Source)>,
    /// The earlier call this one depends on, by its index in the resolved list.
    pub after: Option<usize>,
}

/// Why a row of a proposal was dropped. A closed set, so a count of them is all that is said.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Dropped {
    /// The row could not be read.
    Unreadable,
    /// It named no tool of the step.
    Tool,
    /// An entry number was outside the list.
    Entry,
    /// It named new text or a row that is not there, or one that was dropped.
    Reference,
    /// An argument was given in a way the plan did not allow, or left out.
    Argument,
    /// It repeated an earlier call, or new text an earlier line gave.
    Repeated,
    /// It went past a cap on rows or new values.
    Cap,
}

impl Dropped {
    /// The reason, as the person reads it.
    pub fn describe(self) -> &'static str {
        match self {
            Self::Unreadable => "could not be read",
            Self::Tool => "named no tool of the step",
            Self::Entry => "named an entry that is not in the list",
            Self::Reference => "named new text or a row that is not there",
            Self::Argument => "gave an argument the plan does not let it give",
            Self::Repeated => "repeated an earlier call or name",
            Self::Cap => "went past a cap",
        }
    }
}

/// What a proposal resolved to: the calls, in row order, and how many rows were dropped and why.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Resolved {
    pub calls: Vec<Call>,
    pub dropped: BTreeMap<Dropped, usize>,
}

impl Resolved {
    /// The calls that depend, directly or through others, on the call at `index`.
    pub fn dependants(&self, index: usize) -> BTreeSet<usize> {
        let mut found = BTreeSet::new();
        for (later, call) in self.calls.iter().enumerate().skip(index + 1) {
            if let Some(before) = call.after
                && (before == index || found.contains(&before))
            {
                found.insert(later);
            }
        }
        found
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(tool: usize, after: Option<usize>) -> Call {
        Call {
            tool,
            name: format!("tool{tool}"),
            arguments: Vec::new(),
            after,
        }
    }

    /// A call that depends on a dependant depends on the first call too, and an unrelated one does
    /// not, so unticking or failing one call takes exactly its chain with it.
    #[test]
    fn the_dependants_of_a_call_are_its_whole_chain_and_nothing_else() {
        let resolved = Resolved {
            calls: vec![
                call(0, None),
                call(0, None),
                call(1, Some(0)),
                call(1, Some(1)),
                call(1, Some(2)),
            ],
            dropped: BTreeMap::new(),
        };
        assert_eq!(resolved.dependants(0), BTreeSet::from([2, 4]));
        assert_eq!(resolved.dependants(1), BTreeSet::from([3]));
        assert!(resolved.dependants(4).is_empty());
    }

    /// The fields an entry needs are the ones an argument takes, each once.
    #[test]
    fn an_entry_needs_the_fields_the_arguments_take() {
        let shape = Shape {
            tools: vec![
                Tool {
                    name: "make".into(),
                    arguments: vec![
                        ("title".into(), Kind::New),
                        ("parent".into(), Kind::Literal("Organised".into())),
                    ],
                },
                Tool {
                    name: "move".into(),
                    arguments: vec![
                        ("url".into(), Kind::Entry("url".into())),
                        (
                            "folder".into(),
                            Kind::Row {
                                tool: 0,
                                argument: "title".into(),
                            },
                        ),
                    ],
                },
            ],
            max_rows: 50,
            max_new: 12,
        };
        assert_eq!(shape.entry_fields(), BTreeSet::from(["url".to_string()]));
    }
}
