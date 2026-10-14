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

const UNMATCHED_DENY_RULE: &[(Consumer, &str)] = &[(Consumer::DenyRule, "#2337")];

const LINK: &[(Consumer, &str)] = &[
    (Consumer::TrustRead, "#2328"),
    (Consumer::TrustWrite, "#2328"),
    (Consumer::WriterReservation, "#2328"),
];

const HARD_LINK: &[(Consumer, &str)] = &[
    (Consumer::TrustRead, "#2328"),
    (Consumer::TrustWrite, "#2328"),
    (Consumer::WriterReservation, "#2328"),
    (Consumer::DenyRule, "#2327"),
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
        unhandled: UNMATCHED_DENY_RULE,
    },
    Alias {
        kind: "NFC and NFD, NFD name, file stored as NFC",
        name: "cafe\u{301}.md",
        alias: "caf\u{e9}.md",
        made: Made::StoredAsTheAlias,
        skippable: true,
        unhandled: UNMATCHED_DENY_RULE,
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
