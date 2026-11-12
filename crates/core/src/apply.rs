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

/// What became of one call of an approved list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Came {
    /// The server answered with no failure reported.
    Made,
    /// The server reported a failure, or its reply could not be used.
    Failed,
    /// It depended on a call that failed, so it was not made.
    Skipped,
    /// The person left it out, or a call it depended on.
    LeftOut,
    /// The run stopped before it, or at it.
    Unmade,
}

/// The calls of an approved list to make, in order, and what became of each.
///
/// Built only by the policy layer, from the labelled list and the person's ticks, so the driver
/// carries call numbers and never reads which call depends on which (LABEL-10). A call is kept
/// only where every call it depends on is kept too, and a call that fails takes every call that
/// depends on it, directly or through others, with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Applying {
    after: Vec<Option<usize>>,
    kept: Vec<usize>,
    next: usize,
    came: Vec<Came>,
}

impl Applying {
    pub(crate) fn new(after: Vec<Option<usize>>, ticked: &[usize]) -> Self {
        let mut kept: Vec<usize> = Vec::new();
        for (at, before) in after.iter().enumerate() {
            if ticked.contains(&at) && before.is_none_or(|before| kept.contains(&before)) {
                kept.push(at);
            }
        }
        let came = (0..after.len())
            .map(|at| match kept.contains(&at) {
                true => Came::Unmade,
                false => Came::LeftOut,
            })
            .collect();
        Self {
            after,
            kept,
            next: 0,
            came,
        }
    }

    /// Whether no call is kept.
    pub fn is_empty(&self) -> bool {
        self.kept.is_empty()
    }

    /// The next call to make, by its place in the list, passing over any a failure skipped.
    pub fn next_call(&mut self) -> Option<usize> {
        while let Some(&at) = self.kept.get(self.next) {
            self.next += 1;
            if self.came[at] != Came::Skipped {
                return Some(at);
            }
        }
        None
    }

    /// Record that the call at `at` was answered with no failure reported.
    pub fn made(&mut self, at: usize) {
        self.came[at] = Came::Made;
    }

    /// Record that the call at `at` failed, and skip every kept call that depends on it.
    pub fn failed(&mut self, at: usize) {
        self.came[at] = Came::Failed;
        let mut gone = BTreeSet::from([at]);
        for later in at + 1..self.after.len() {
            if self.after[later].is_some_and(|before| gone.contains(&before)) {
                gone.insert(later);
                if self.came[later] == Came::Unmade {
                    self.came[later] = Came::Skipped;
                }
            }
        }
    }

    /// What became of each call of the list, in list order.
    pub fn came(&self) -> &[Came] {
        &self.came
    }

    /// How many calls were kept, made, failed and skipped.
    pub fn tally(&self) -> (usize, usize, usize, usize) {
        let count = |came: Came| self.came.iter().filter(|c| **c == came).count();
        (
            self.kept.len(),
            count(Came::Made),
            count(Came::Failed),
            count(Came::Skipped),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// LABEL-10. A call is kept only where every call it depends on is kept too, calls are handed
    /// out in list order, and a failed call skips every kept call that depends on it, directly or
    /// through another. The regressions it rejects are a call kept after the call it needs was
    /// left out, and a failure that skips only the calls depending on it directly.
    #[test]
    fn applying_keeps_what_its_dependencies_allow_and_a_failure_skips_its_dependants() {
        // 0 makes a folder, 1 and 2 depend on it, 3 depends on 2, and 4 stands alone.
        let after = vec![None, Some(0), Some(0), Some(2), None];
        let left_out = Applying::new(after.clone(), &[1, 4]);
        assert_eq!(
            left_out.tally(),
            (1, 0, 0, 0),
            "1 needs 0, which was left out"
        );

        let mut applying = Applying::new(after, &[0, 1, 2, 3, 4]);
        assert_eq!(applying.next_call(), Some(0));
        applying.made(0);
        assert_eq!(applying.next_call(), Some(1));
        applying.made(1);
        assert_eq!(applying.next_call(), Some(2));
        applying.failed(2);
        assert_eq!(applying.next_call(), Some(4), "3 depended on 2");
        applying.made(4);
        assert_eq!(applying.next_call(), None);
        assert_eq!(
            applying.came(),
            &[
                Came::Made,
                Came::Made,
                Came::Failed,
                Came::Skipped,
                Came::Made
            ]
        );
        assert_eq!(applying.tally(), (5, 3, 1, 1));
        assert!(Applying::new(vec![None], &[]).is_empty());
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
