//! The ways a volume gives one file two names, as one table (TRUST-2).
//!
//! A row is a file made under one name and a second name that opens it. A test per consumer of a
//! file's name drives that consumer over every row, so a new spelling is a new row here and is
//! asked of all of them. A row a consumer does not handle lists the issue that tracks it, and the
//! consumer's test fails when the row is handled, so the list cannot go stale.

use std::path::Path;

/// What every file in the table holds, so a read that gets through is recognisable in a request.
pub const CONTENT: &str = "SECRET_TOKEN=hunter2";

/// A file no row names, in the working directory. A consumer that treats every file as the one a
/// rule is about passes a row it should fail, so each test also asks about this one.
pub const CONTROL: &str = "control.txt";

/// How the second name comes to open the file.
#[derive(Clone, Copy, Debug)]
pub enum Made {
    /// The file is made under `name`, and the volume opens it as `alias` by itself.
    ByTheVolume,
    /// The file is made under `alias`, and the volume opens it as `name`. A record or a rule about
    /// `name` then meets a call that names the file as it is stored.
    StoredAsTheAlias,
    /// `alias` is a hard link to `name`.
    HardLink,
    /// `alias` is a symbolic link to `name`.
    SymbolicLink,
    /// The first segment of `alias` is a symbolic link to the first segment of `name`.
    LinkedDirectory,
}

/// Something that decides what to do with a file by its name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Consumer {
    /// The label a read of the alias gets once the name is recorded untrusted.
    TrustRead,
    /// The label both names read back with after an untrusted write through the alias.
    TrustWrite,
    /// Whether a writer through the alias is refused while a writer through the name is open.
    WriterReservation,
    /// Whether a deny rule written about the name refuses a read and a write of the alias.
    DenyRule,
    /// Whether the lookup of a program bravebot starts itself leaves out a `PATH` entry spelled as
    /// the alias, when the directory spelled as the name is one a stage may write (SANDBOX-30).
    /// Asked of the rows that spell a directory another way.
    ProgramLookup,
}

pub struct Alias {
    /// The row in the TRUST-2 table of spellings.
    pub kind: &'static str,
    /// Relative to the working directory. The record or the rule is about this name.
    pub name: &'static str,
    pub alias: &'static str,
    pub made: Made,
    /// Whether a volume may hold the two names apart. The row then says nothing there; any other
    /// row that does not open is a failure, so a broken setup cannot skip the table.
    pub skippable: bool,
    /// Consumers that do not treat the alias as the name, each with the issue that tracks it.
    pub unhandled: &'static [(Consumer, &'static str)],
}

const LINK: &[(Consumer, &str)] = &[
    (Consumer::TrustRead, "#2328"),
    (Consumer::TrustWrite, "#2328"),
    (Consumer::WriterReservation, "#2328"),
];

const HARD_LINK: &[(Consumer, &str)] = &[
    (Consumer::TrustRead, "#2328"),
    (Consumer::TrustWrite, "#2328"),
    (Consumer::WriterReservation, "#2328"),
];

pub const ALIASES: &[Alias] = &[
    Alias {
        kind: "interior dot segment",
        name: "src/fetched.json",
        alias: "src/./fetched.json",
        made: Made::ByTheVolume,
        skippable: false,
        unhandled: &[],
    },
    Alias {
        kind: "doubled separator",
        name: "src/fetched.json",
        alias: "src//fetched.json",
        made: Made::ByTheVolume,
        skippable: false,
        unhandled: &[],
    },
    Alias {
        kind: "letter case",
        name: "src/fetched.json",
        alias: "SRC/fetched.json",
        made: Made::ByTheVolume,
        skippable: true,
        unhandled: &[],
    },
    Alias {
        kind: "letter case, file stored in the other case",
        name: "SRC/fetched.json",
        alias: "src/fetched.json",
        made: Made::StoredAsTheAlias,
        skippable: true,
        unhandled: &[],
    },
    Alias {
        kind: "long s",
        name: "src/fetched.json",
        alias: "\u{17f}rc/fetched.json",
        made: Made::ByTheVolume,
        skippable: true,
        unhandled: &[],
    },
    Alias {
        kind: "NFC and NFD, NFD name",
        name: "cafe\u{301}.md",
        alias: "caf\u{e9}.md",
        made: Made::ByTheVolume,
        skippable: true,
        unhandled: &[],
    },
    Alias {
        kind: "NFC and NFD, NFC name",
        name: "caf\u{e9}.md",
        alias: "cafe\u{301}.md",
        made: Made::ByTheVolume,
        skippable: true,
        unhandled: &[],
    },
    Alias {
        kind: "NFC and NFD, directory",
        name: "cafe\u{301}/fetched.json",
        alias: "caf\u{e9}/fetched.json",
        made: Made::ByTheVolume,
        skippable: true,
        unhandled: &[],
    },
    Alias {
        kind: "NFC and NFD, NFC name, file stored as NFD",
        name: "caf\u{e9}.md",
        alias: "cafe\u{301}.md",
        made: Made::StoredAsTheAlias,
        skippable: true,
        unhandled: &[],
    },
    Alias {
        kind: "NFC and NFD, NFD name, file stored as NFC",
        name: "cafe\u{301}.md",
        alias: "caf\u{e9}.md",
        made: Made::StoredAsTheAlias,
        skippable: true,
        unhandled: &[],
    },
    Alias {
        kind: "symbolic link to the file",
        name: "src/fetched.json",
        alias: "src/linked.json",
        made: Made::SymbolicLink,
        skippable: false,
        unhandled: LINK,
    },
    Alias {
        kind: "symbolic link on the way",
        name: "src/fetched.json",
        alias: "linked/fetched.json",
        made: Made::LinkedDirectory,
        skippable: false,
        unhandled: LINK,
    },
    Alias {
        kind: "hard link",
        name: "src/fetched.json",
        alias: "src/linked.json",
        made: Made::HardLink,
        skippable: false,
        unhandled: HARD_LINK,
    },
];

/// Make the file under `root` and its second name, and say whether the volume opens that name.
///
/// False where the volume holds the pair apart and the row may be skipped, and for a link on a
/// host without Unix links. A row that may not be skipped and does not open is a failure.
pub fn prepare(root: &Path, row: &Alias) -> bool {
    let (stored, other) = match row.made {
        Made::StoredAsTheAlias => (row.alias, row.name),
        _ => (row.name, row.alias),
    };
    let stored_path = root.join(stored);
    std::fs::create_dir_all(stored_path.parent().expect("a name is in a directory"))
        .expect("directory");
    std::fs::write(&stored_path, CONTENT).expect("file");
    std::fs::write(root.join(CONTROL), CONTENT).expect("control file");
    match row.made {
        Made::ByTheVolume | Made::StoredAsTheAlias => {}
        #[cfg(unix)]
        Made::HardLink => {
            std::fs::hard_link(&stored_path, root.join(other)).expect("hard link");
        }
        #[cfg(unix)]
        Made::SymbolicLink => {
            std::os::unix::fs::symlink(&stored_path, root.join(other)).expect("symbolic link");
        }
        #[cfg(unix)]
        Made::LinkedDirectory => {
            let first = |text: &'static str| text.split('/').next().expect("a segment");
            std::os::unix::fs::symlink(root.join(first(stored)), root.join(first(other)))
                .expect("symbolic link to a directory");
        }
        #[cfg(not(unix))]
        _ => return false,
    }
    let opens = root.join(other).exists();
    assert!(
        opens || row.skippable,
        "{}: the volume does not open {other:?} as {stored:?}, and no volume holds them apart",
        row.kind
    );
    opens
}

/// Hold what a consumer did with `row` to what the table says.
pub fn expect(row: &Alias, consumer: Consumer, handled: bool) {
    let listed = row.unhandled.iter().find(|(listed, _)| *listed == consumer);
    match listed {
        None => assert!(
            handled,
            "{consumer:?} treats {:?} as another file than {:?} ({})",
            row.alias, row.name, row.kind
        ),
        Some((_, issue)) => assert!(
            !handled,
            "{consumer:?} now treats {:?} as {:?} ({}): remove it from the row's unhandled list \
             and update the TRUST-2 table; {issue} tracked it",
            row.alias, row.name, row.kind
        ),
    }
}

/// Legal but awkward file names, shared by every test that asks what a name is held under. Each is
/// a relative name for a file, and none is a directory another entry sits in, so any two can be
/// created side by side. A host that cannot hold a name skips the pair it is in.
///
/// The second field is the name's group: names of one group are one name on purpose, because a
/// refusal recorded about one is about every canonically equivalent spelling (TRUST-2). Every
/// other pair is two files wherever the volume holds them apart, and a key that joins them is a
/// rule about one file deciding another.
///
/// Every name is text, because a trust key is a `&str`: a name that is not UTF-8 never reaches the
/// map, and the step before it refuses to render one. That step is covered by
/// `a_second_tree_name_with_invalid_utf8_is_not_matched_as_its_lossy_lookalike` and
/// `a_hard_link_named_with_invalid_utf8_is_not_matched_as_its_lossy_lookalike` in
/// `crates/agent/src/workspace.rs` (PATH-003).
pub const AWKWARD_NAMES: &[(&str, &str)] = &[
    ("a.md", "plain"),
    ("A.md", "upper"),
    ("a.md ", "trailing space"),
    (" a.md", "leading space"),
    ("a.md\t", "trailing tab"),
    ("a.md\n", "trailing newline"),
    ("a.md\u{a0}", "trailing no-break space"),
    ("a.md\u{3000}", "trailing ideographic space"),
    ("a.md.", "trailing dot"),
    ("a.md\u{200b}", "trailing zero-width space"),
    ("\u{202e}a.md", "right-to-left override"),
    ("-a.md", "leading dash"),
    ("*.md", "glob"),
    ("a\\b.md", "backslash"),
    ("a:b.md", "colon"),
    (" ", "one space"),
    ("\t", "one tab"),
    ("\u{a0}", "one no-break space"),
    ("\u{3000}", "one ideographic space"),
    ("x/a.md", "in a directory"),
    ("x /a.md", "in a directory with a trailing space"),
    ("x/ /a.md", "in a directory under a space"),
    ("caf\u{e9}.md", "accent"),
    ("cafe\u{301}.md", "accent"),
];

/// Whether the volume holds `first` and `second` as two files, made side by side under `root`.
///
/// False where the second name opens the first (the TRUST-2 table covers those pairs), where the
/// host cannot make either name, and on a host with no file identity to compare.
#[cfg(unix)]
pub fn two_files(root: &Path, first: &str, second: &str) -> bool {
    use std::os::unix::fs::MetadataExt;
    let make = |name: &str| -> bool {
        let path = root.join(name);
        std::fs::create_dir_all(path.parent().expect("a name is in a directory")).is_ok()
            && std::fs::write(&path, CONTENT).is_ok()
    };
    if !make(first) || root.join(second).exists() || !make(second) {
        return false;
    }
    match (
        std::fs::metadata(root.join(first)),
        std::fs::metadata(root.join(second)),
    ) {
        (Ok(one), Ok(other)) => (one.dev(), one.ino()) != (other.dev(), other.ino()),
        _ => false,
    }
}

#[cfg(not(unix))]
pub fn two_files(_: &Path, _: &str, _: &str) -> bool {
    false
}
