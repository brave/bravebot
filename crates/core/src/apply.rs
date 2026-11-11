//! The shape of an `apply` step, and what a processor's proposal for it resolves to.
//!
//! A plan fixes the tools a step may call, the lists a proposal may point into, and for each
//! argument where its value may come from: the plan's own text, a field of an entry of one of those
//! lists, the same field of several entries as one list, short text the processor wrote, or the
//! value an earlier row of the same step gave such an argument. A processor proposes rows of those
//! tools by number. What the rows resolve to is put to the person as one list, and only the rows
//! they keep are called (`docs/design/manifest-apply.md`).
//!
//! Nothing here reads a proposal. The types are filled by `bravebot-choice`, through the policy
//! layer's gate for that one read, and carried by the driver without being looked at.

use std::collections::{BTreeMap, BTreeSet};

/// Where one argument of a tool may take its value from, fixed by the plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    /// The plan's own text.
    Literal(String),
    /// A field of one entry of the list at this index, copied byte for byte.
    Entry { list: usize, field: String },
    /// The same field of several entries of the list at this index, as one list of values, for an
    /// argument the tool takes as an array.
    Entries { list: usize, field: String },
    /// Short text the processor wrote, for a value the call sets and does not look up.
    New,
    /// The value an earlier row of the tool at this index gave the named argument.
    Row { tool: usize, argument: String },
    /// Any one of these, chosen by each row, as a folder a bookmark moves to may be one an
    /// earlier row made or one a list holds. None of them is a literal or another choice.
    Either(Vec<Kind>),
}

impl Kind {
    /// The word a declaration names this kind by, or each word of a choice joined by commas.
    pub fn word(&self) -> String {
        match self {
            Self::Literal(_) => "literal".to_string(),
            Self::Entry { .. } => "entry".to_string(),
            Self::Entries { .. } => "entries".to_string(),
            Self::New => "new".to_string(),
            Self::Row { .. } => "row".to_string(),
            Self::Either(kinds) => kinds.iter().map(Kind::word).collect::<Vec<_>>().join(","),
        }
    }

    /// The fields of the list at `list` this kind may take a value from.
    fn fields(&self, list: usize) -> Vec<String> {
        match self {
            Self::Entry { list: at, field } | Self::Entries { list: at, field } if *at == list => {
                vec![field.clone()]
            }
            Self::Either(kinds) => kinds.iter().flat_map(|kind| kind.fields(list)).collect(),
            _ => Vec::new(),
        }
    }
}

/// One tool an `apply` step may call, with the kind of each of its arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tool {
    pub name: String,
    pub arguments: Vec<(String, Kind)>,
    /// The JSON type the list a person vouched for gives each argument, where it gives one: a
    /// resolved value is sent as that type, and a row whose value is not that type is dropped.
    pub types: BTreeMap<String, Declared>,
}

/// The type a tool's list gives one argument, and the type of its items where it is an array.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Declared {
    pub kind: Option<String>,
    pub items: Option<String>,
}

/// A resolved value as its call sends it, in the type the argument is declared to take.
#[derive(Debug, Clone, PartialEq)]
pub enum Sent {
    Text(String),
    Integer(i64),
    /// A finite number.
    Number(f64),
    Switch(bool),
    Array(Vec<Sent>),
}

/// Everything about an `apply` step the plan fixed before anything was read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shape {
    /// The lists a row may point into, by the names of the slots that hold them, in the plan's
    /// order. A row names an entry as `name:N`.
    pub lists: Vec<String>,
    pub tools: Vec<Tool>,
    pub max_rows: usize,
    pub max_new: usize,
}

impl Shape {
    /// The fields an argument of any tool may take from an entry of the list at `list`. An entry
    /// counts where it holds at least one of them, and a row naming it for a field it lacks is
    /// dropped.
    pub fn entry_fields(&self, list: usize) -> BTreeSet<String> {
        self.tools
            .iter()
            .flat_map(|tool| &tool.arguments)
            .flat_map(|(_, kind)| kind.fields(list))
            .collect()
    }
}

/// The most entries a list may hold, so an entry's number is at most three digits.
pub const LARGEST_LIST: usize = 999;

/// The most entries one argument may take as a list.
pub const LARGEST_ENTRIES: usize = 50;

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
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// The plan wrote it.
    Plan,
    /// The entry with this number, from 1, of the list at this index.
    Entry { list: usize, number: usize },
    /// The entries with these numbers of the list at this index, in the order the row gave them.
    Entries { list: usize, numbers: Vec<usize> },
    /// The processor wrote it.
    Processor,
    /// The call with this index in the resolved list gave it.
    Call(usize),
}

/// One call a proposal resolved to.
#[derive(Debug, Clone, PartialEq)]
pub struct Call {
    /// The index of the tool in the shape.
    pub tool: usize,
    /// The tool's name.
    pub name: String,
    /// Each argument with its value and where the value came from, in the shape's order. A value
    /// taken from several entries is the JSON array of their values.
    pub arguments: Vec<(String, String, Source)>,
    /// Each argument as the call sends it, typed as its tool declares, in the same order.
    pub sent: Vec<(String, Sent)>,
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
    /// An entry number was outside its list, or the entry lacked the field.
    Entry,
    /// It named new text or a row that is not there, or one that was dropped.
    Reference,
    /// An argument was given in a way the plan did not allow, or left out.
    Argument,
    /// It repeated an earlier call, or new text an earlier line gave.
    Repeated,
    /// It went past a cap on rows, new values or entries.
    Cap,
    /// A value was not the type the tool's list gives its argument.
    Type,
}

impl Dropped {
    /// The reason, as the person reads it.
    pub fn describe(self) -> &'static str {
        match self {
            Self::Unreadable => "could not be read",
            Self::Tool => "named no tool of the step",
            Self::Entry => "named an entry that is not in its list",
            Self::Reference => "named new text or a row that is not there",
            Self::Argument => "gave an argument the plan does not let it give",
            Self::Repeated => "repeated an earlier call or name",
            Self::Cap => "went past a cap",
            Self::Type => "gave a value that is not the type its tool takes",
        }
    }
}

/// What a proposal resolved to: the calls, in row order, and how many rows were dropped and why.
#[derive(Debug, Clone, Default, PartialEq)]
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
            sent: Vec::new(),
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

    /// The fields an entry of each list needs are the ones an argument takes from that list, each
    /// once, including through a choice of kinds and a list of entries.
    #[test]
    fn each_list_needs_the_fields_the_arguments_take_from_it() {
        let shape = Shape {
            lists: vec!["issues".into(), "labels".into()],
            tools: vec![
                Tool {
                    types: Default::default(),
                    name: "label".into(),
                    arguments: vec![
                        (
                            "issue".into(),
                            Kind::Entry {
                                list: 0,
                                field: "number".into(),
                            },
                        ),
                        (
                            "label".into(),
                            Kind::Either(vec![
                                Kind::Entry {
                                    list: 1,
                                    field: "name".into(),
                                },
                                Kind::New,
                            ]),
                        ),
                    ],
                },
                Tool {
                    types: Default::default(),
                    name: "close".into(),
                    arguments: vec![(
                        "issues".into(),
                        Kind::Entries {
                            list: 0,
                            field: "number".into(),
                        },
                    )],
                },
            ],
            max_rows: 50,
            max_new: 12,
        };
        assert_eq!(
            shape.entry_fields(0),
            BTreeSet::from(["number".to_string()])
        );
        assert_eq!(shape.entry_fields(1), BTreeSet::from(["name".to_string()]));
    }
}
