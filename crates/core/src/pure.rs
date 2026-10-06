//! Programs whose output is a function of their input.
//!
//! Most of what [`crate::command`] runs is opaque: it might write, it might reach the network, and
//! nothing here can tell, so its output is untrusted and a person approves it. A few programs are
//! different. `wc -l` reading stdin cannot do anything except count lines, so its output is
//! determined by what went in, and a stage like that needs neither a prompt nor a pessimistic
//! label. Trusted input gives trusted output; untrusted input gives untrusted output.
//!
//! That is not a relabel and grants nothing. It is the same reasoning
//! [`crate::policy::Policy::label_model_output`] rests on, applied to a process instead of a model:
//! when the output is a function of the inputs, the output's first label is the meet of theirs.
//!
//! # Eligibility is about (program, argv), never a program alone
//!
//! The temptation is a list of trustworthy program names. That would be wrong, and wrong in a way
//! that fails open, because the two programs anyone would list first are the two that must never be
//! on it.
//!
//! `sed` and `awk` are not filters, they are interpreters, and the program they run is an argument:
//!
//! ```text
//! printf 'x\n' | sed 'w leaked.txt'          # writes a file
//! printf 'x\n' | awk '{system("...")}'       # runs an arbitrary command
//! ```
//!
//! Awk's `system()` reaches the shell this repository excludes, so admitting `awk` would undo the
//! exclusion sideways. Recognising a *safe* sed or awk script means parsing sed's and awk's
//! languages, which is the same trap as parsing a shell string: a parser racing an interpreter it
//! does not control. Neither is eligible, and neither ever becomes eligible.
//!
//! `sort` and `uniq` are ordinary filters with an output file (`sort -o`, and `uniq`'s second
//! operand). They are in the table because the allowlist refuses those shapes: `sort` lists no
//! option that writes or runs a program, and `uniq` admits one operand, so a second is refused.
//!
//! # What a proof establishes
//!
//! [`read_set`] answers with the paths a call reads, and a call it cannot account for gets no
//! answer at all. An answer is a claim that all four hold for that exact argv:
//!
//! 1. it writes no file;
//! 2. it executes nothing and starts no process;
//! 3. it opens no socket;
//! 4. every byte it reads comes from stdin or from a path in the answer.
//!
//! So `wc -l` answers with nothing, meaning stdin was its only input, and `wc -l Cargo.toml`
//! answers with that file. A caller takes the label from the answer: the meet over those paths and
//! over stdin, which is the ordinary rule for a derived value applied to a process.
//!
//! No answer is the conservative case and the default. A program not in [`FILTERS`] gets none, and
//! so do an option the entry does not list, an option written after an operand, an operand a
//! program has no reading for, and a recursive call naming no path.
//!
//! # The options are an allowlist, and that is the whole of why this is safe
//!
//! A list of *bad* options fails open. An option nobody thought of reads as harmless, and one of
//! them is `-S`, which makes BSD grep follow every symlink it meets while walking, so the tree read
//! is not the tree the answer names. An abbreviation fails open the same way: `--recursi` is not
//! `--recursive` to a matcher and is exactly `--recursive` to the program.
//!
//! So each entry lists the options it recognises, spelled exactly, and anything else refuses the
//! call. An option that takes a value says so, and its value is skipped whether it is attached or
//! is the next word, so it is never counted as an operand: `grep -A 1 -r TODO` walks the working
//! directory and names no path, and a count read as the pattern would have left `TODO` standing in
//! for the tree.
//!
//! # An option after an operand proves nothing
//!
//! GNU's option parser reads an option anywhere in argv. BSD `head`, `tail`, `wc` and `cut` stop
//! reading options at the first operand, and so does GNU with `POSIXLY_CORRECT` set, so a word
//! spelled like an option after it is a file: `head a.md -n secret.txt` prints `secret.txt` as
//! well as `a.md`. The two readings name different files, so a call with such a word proves
//! nothing. `-` alone is not such a word: both read it as an operand wherever it is written.
//!
//! What that operand names differs instead. GNU and BSD `cut` and `grep` read standard input for
//! it, while BSD `head`, `tail` and `wc` open a file called `-`, so for those three it is counted
//! as a path.
//!
//! # An option that supplies the instruction is excluded, not parsed
//!
//! Which operand is a file depends on how many of them the program was told to treat as its
//! instruction, and an option can supply that instruction instead: `grep -e pattern file` puts the
//! pattern behind `-e`, so counting operands no longer says which is which. Recognising that means
//! tracking what each option means rather than what it consumes, which is the option-parsing race
//! this module exists to avoid, so the option is excluded and the call proves nothing.
//!
//! # Resolve the program, do not trust the name
//!
//! A name is not a program. On the machine this was developed against, `grep` resolves to `ugrep`, a
//! different implementation with a far larger option surface. `$PATH` and shell aliases both decide
//! what a name means, so a caller must match on what the name resolved to and record it. This module
//! judges a resolved program; finding out what a name resolves to is the caller's job.

/// A program that reads stdin and the paths it is given, and does nothing else.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Filter {
    /// The program's name, as resolved.
    pub program: &'static str,
    /// Every option a call may use. Anything else refuses the call.
    ///
    /// An allowlist rather than a list of bad options, because a list of bad options fails open:
    /// an option nobody thought of is read as harmless, and one of them is `-S`, which makes BSD
    /// grep follow symlinks out of the tree it was pointed at.
    pub flags: &'static [Flag],
    /// Options that must never appear in [`Filter::flags`], with the reason kept.
    ///
    /// Absence is what refuses them, so this list changes nothing. It is here because absence is
    /// easy to reverse by accident and says nothing about why, and because a test asserts the two
    /// lists stay disjoint.
    pub excluded: &'static [&'static str],
    /// Whether `-20` is a count this program accepts.
    ///
    /// `head`, `tail` and `grep` all take a number that way, and it is a spelling rather than an
    /// option: there is no letter to look up.
    pub numeric: bool,
    /// How many non-flag arguments are *not* files.
    ///
    /// Operand meaning is per-program and cannot be generalised. `wc FILE` reads a file, while
    /// `grep PATTERN` takes a pattern and `tr SET1 SET2` takes two character sets. So each program
    /// says how many operands are part of its instruction, and anything beyond that count is a
    /// path.
    pub operands: usize,
    /// Whether an operand past [`Filter::operands`] is a file the program reads.
    ///
    /// True where the audit establishes that a trailing operand is an input path and nothing else,
    /// as with `wc FILE` and `grep PATTERN FILE`. False where the program has no reading for one at
    /// all: `tr` takes two character sets and reads only stdin, so a third operand is a call
    /// nothing here recognises rather than a file, and it proves nothing.
    pub reads_files: bool,
    /// Whether a file operand of `-` names standard input under every implementation.
    ///
    /// GNU reads standard input for it, and so do BSD `cut` and `grep`. BSD `head`, `tail` and `wc`
    /// open a file called `-`, so for them it stays in the answer as a path: the answer then covers
    /// the file one reads, and the caller's meet over stdin covers the stream the other reads.
    /// False for a program that reads no file, where a `-` past its instruction is an operand it
    /// has no reading for.
    pub dash_is_stdin: bool,
    /// Whether a call must name at least one path operand.
    ///
    /// True where the program given no path reads something the answer cannot name: `ls` and `du`
    /// report on the working directory, and BSD `stat` on its standard input, so a call without a
    /// path proves nothing.
    pub needs_path: bool,
    /// The most operands past [`Filter::operands`] a call may carry, counting a `-`.
    ///
    /// `uniq INPUT OUTPUT` writes its second operand, so the entry admits one. `diff` compares two.
    /// A `-` counts here although [`Filter::dash_is_stdin`] keeps it out of the answer, because
    /// `uniq - out` has an output operand in the position a path would have been left out of.
    pub most_paths: Option<usize>,
}

/// One option a program in the table may be called with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Flag {
    /// The exact spelling, `-c` or `--bytes`. Matched whole: an abbreviation of a long option is
    /// not this option, because deciding which option `--recursi` abbreviates is the option
    /// parsing this module refuses to do.
    pub spelling: &'static str,
    /// What it does to the proof.
    pub effect: Effect,
}

/// What one option does to what a call reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    /// Changes nothing about what is read.
    Plain,
    /// Takes a value, which is part of the instruction and never a path.
    ///
    /// The value is skipped wherever it is written, attached or as the next argument, so it cannot
    /// be counted as an operand. A value counted as an operand is what makes `grep -A 1 -r x` look
    /// like a call naming a path when it names none.
    Takes,
    /// Makes the program read each path operand as a tree rather than as one file.
    ///
    /// The paths are the same paths, so the answer is unchanged and the caller's meet is taken over
    /// whole subtrees. A recursive call naming no path is not recognised: what it walks is then the
    /// working directory by one implementation's convention and stdin by another's, and the
    /// difference is the whole of what the label would be taken from.
    Recursive,
}

/// Shorthand for an option that changes nothing about what is read.
const fn plain(spelling: &'static str) -> Flag {
    Flag {
        spelling,
        effect: Effect::Plain,
    }
}

/// Shorthand for an option whose value is part of the instruction.
const fn takes(spelling: &'static str) -> Flag {
    Flag {
        spelling,
        effect: Effect::Takes,
    }
}

/// Every program eligible for label pass-through.
///
/// Short on purpose. Each entry is a claim that no argument can make the program write, execute, or
/// read anything but stdin and the paths it was given, and that claim was checked against the
/// program's own option list rather than assumed from what it is usually used for. The entry is
/// matched by the name a program resolved to and either the GNU or the BSD implementation may be
/// the one behind it, so an option is listed only where each one that accepts it reads the same
/// files for it and takes its value the same way. One that rejects it, as BSD `wc` rejects every
/// long option, reads nothing.
pub const FILTERS: &[Filter] = &[
    Filter {
        program: "wc",
        flags: &[
            plain("-c"),
            plain("-l"),
            plain("-m"),
            plain("-w"),
            plain("-L"),
            plain("--bytes"),
            plain("--chars"),
            plain("--lines"),
            plain("--words"),
            plain("--max-line-length"),
        ],
        // Names a file holding the names of more, so the files actually read would appear nowhere
        // in the answer.
        excluded: &["--files0-from"],
        numeric: false,
        operands: 0,
        reads_files: true,
        dash_is_stdin: false,
        needs_path: false,
        most_paths: None,
    },
    Filter {
        program: "head",
        flags: &[
            takes("-c"),
            takes("-n"),
            plain("-q"),
            plain("-v"),
            takes("--bytes"),
            takes("--lines"),
            plain("--quiet"),
            plain("--silent"),
            plain("--verbose"),
        ],
        excluded: &[],
        numeric: true,
        operands: 0,
        reads_files: true,
        dash_is_stdin: false,
        needs_path: false,
        most_paths: None,
    },
    Filter {
        program: "tail",
        flags: &[
            takes("-c"),
            takes("-n"),
            plain("-q"),
            plain("-v"),
            takes("--bytes"),
            takes("--lines"),
            plain("--quiet"),
            plain("--silent"),
            plain("--verbose"),
        ],
        // Following never terminates, so a stage using it would never finish, and --pid waits on a
        // process this knows nothing about.
        excluded: &["-f", "-F", "--follow", "--retry", "--pid"],
        numeric: true,
        operands: 0,
        reads_files: true,
        dash_is_stdin: false,
        needs_path: false,
        most_paths: None,
    },
    Filter {
        program: "cut",
        flags: &[
            takes("-b"),
            takes("-c"),
            takes("-f"),
            takes("-d"),
            plain("-s"),
            plain("-n"),
            takes("--bytes"),
            takes("--characters"),
            takes("--fields"),
            takes("--delimiter"),
            takes("--output-delimiter"),
            plain("--complement"),
            plain("--only-delimited"),
        ],
        excluded: &[],
        numeric: false,
        operands: 0,
        reads_files: true,
        dash_is_stdin: true,
        needs_path: false,
        most_paths: None,
    },
    // SET1 and SET2 are character sets, not files. tr reads only stdin.
    Filter {
        program: "tr",
        flags: &[
            plain("-c"),
            plain("-C"),
            plain("-d"),
            plain("-s"),
            plain("-t"),
            plain("-u"),
            plain("--complement"),
            plain("--delete"),
            plain("--squeeze-repeats"),
            plain("--truncate-set1"),
        ],
        excluded: &[],
        numeric: false,
        operands: 2,
        reads_files: false,
        dash_is_stdin: false,
        needs_path: false,
        most_paths: None,
    },
    // The first operand is the pattern; anything after it is a path.
    Filter {
        program: "grep",
        flags: &[
            plain("-i"),
            plain("-v"),
            plain("-n"),
            plain("-c"),
            plain("-l"),
            plain("-L"),
            plain("-o"),
            plain("-q"),
            plain("-s"),
            plain("-h"),
            plain("-H"),
            plain("-w"),
            plain("-x"),
            plain("-a"),
            plain("-b"),
            plain("-E"),
            plain("-F"),
            plain("-G"),
            plain("-P"),
            plain("--ignore-case"),
            plain("--invert-match"),
            plain("--line-number"),
            plain("--count"),
            plain("--files-with-matches"),
            plain("--files-without-match"),
            plain("--only-matching"),
            plain("--quiet"),
            plain("--silent"),
            plain("--no-messages"),
            plain("--no-filename"),
            plain("--with-filename"),
            plain("--word-regexp"),
            plain("--line-regexp"),
            plain("--text"),
            plain("--byte-offset"),
            plain("--extended-regexp"),
            plain("--fixed-strings"),
            plain("--basic-regexp"),
            plain("--perl-regexp"),
            takes("-A"),
            takes("-B"),
            takes("-C"),
            takes("-m"),
            takes("--after-context"),
            takes("--before-context"),
            takes("--max-count"),
            // Both only narrow which of the named paths are read, so the answer still covers what
            // was read.
            takes("--include"),
            takes("--exclude"),
            Flag {
                spelling: "-r",
                effect: Effect::Recursive,
            },
            Flag {
                spelling: "--recursive",
                effect: Effect::Recursive,
            },
        ],
        excluded: &[
            // -e and -f supply the pattern, so the operand count stops saying which operand is the
            // pattern and which is a path. -f and --exclude-from also name files of patterns that
            // do not look like file operands: `grep -f ~/.ssh/id_rsa` would read a key in as
            // patterns while the answer named nothing.
            "-e",
            "--regexp",
            "-f",
            "--file",
            "--exclude-from",
            // -R and -S follow symlinks while walking, so the tree read is not the tree the answer
            // names. -r walks the same paths without leaving them, under both implementations.
            "-R",
            "--dereference-recursive",
            "-S",
            // -d read makes grep read a directory as a file, and which of read, skip and recurse
            // it was given is in the value rather than in the option.
            "-d",
            "--directories",
            "-D",
            "--devices",
            // The value is optional, so neither reading of a bare --color is safe to assume: as an
            // option taking one it would swallow the pattern, and as one taking none it would
            // refuse the spelling everybody writes.
            "--color",
            "--colour",
            // BSD gives --context an optional value, so a separate word after it is the pattern
            // there and the value under GNU. -C takes a required value under both.
            "--context",
        ],
        numeric: true,
        operands: 1,
        reads_files: true,
        dash_is_stdin: true,
        needs_path: false,
        most_paths: None,
    },
    // Operates on the string it is given rather than on a file of that name.
    Filter {
        program: "basename",
        flags: &[plain("-a"), takes("-s")],
        excluded: &[],
        numeric: false,
        operands: 2,
        reads_files: false,
        dash_is_stdin: false,
        needs_path: false,
        most_paths: None,
    },
    Filter {
        program: "dirname",
        flags: &[],
        excluded: &[],
        numeric: false,
        operands: 1,
        reads_files: false,
        dash_is_stdin: false,
        needs_path: false,
        most_paths: None,
    },
    // Takes no input at all: it reports the working directory, which the user established, so it
    // reads neither a path nor stdin.
    Filter {
        program: "pwd",
        flags: &[plain("-L"), plain("-P")],
        excluded: &[],
        numeric: false,
        operands: 0,
        reads_files: false,
        dash_is_stdin: false,
        needs_path: false,
        most_paths: None,
    },
    // Reads each named path's directory entries and metadata, never a file's contents. With no path
    // it lists the working directory, which the answer cannot name. -L and -H follow links to a
    // target the answer does not name, and the options that take a value (--hide, -I, -w, -T) or
    // differ between GNU and BSD (-G, --color, -D) are left out.
    Filter {
        program: "ls",
        flags: &[
            plain("-1"),
            plain("-a"),
            plain("-A"),
            plain("-d"),
            plain("-F"),
            plain("-h"),
            plain("-i"),
            plain("-l"),
            plain("-n"),
            plain("-p"),
            plain("-r"),
            plain("-S"),
            plain("-t"),
            plain("--all"),
            plain("--almost-all"),
            plain("--directory"),
            plain("--reverse"),
            Flag {
                spelling: "-R",
                effect: Effect::Recursive,
            },
            Flag {
                spelling: "--recursive",
                effect: Effect::Recursive,
            },
        ],
        excluded: &[
            "-L",
            "-H",
            "--dereference",
            "--dereference-command-line",
            "--color",
        ],
        numeric: false,
        operands: 0,
        reads_files: true,
        dash_is_stdin: false,
        needs_path: true,
        most_paths: None,
    },
    Filter {
        program: "cat",
        flags: &[
            plain("-b"),
            plain("-e"),
            plain("-n"),
            plain("-s"),
            plain("-t"),
            plain("-u"),
            plain("-v"),
            plain("--number"),
            plain("--number-nonblank"),
            plain("--squeeze-blank"),
        ],
        excluded: &[],
        numeric: false,
        operands: 0,
        reads_files: true,
        dash_is_stdin: true,
        needs_path: false,
        most_paths: None,
    },
    // Without -o, --output, --compress-program, -T, --temporary-directory, --files0-from or
    // --random-source, which write a file, run a program, or name a file of names. -S is left out
    // because the two implementations read its unit suffixes differently. The scratch files sort
    // spills to under TMPDIR hold bytes it was given and are named by the program, never by the call.
    Filter {
        program: "sort",
        flags: &[
            plain("-b"),
            plain("-c"),
            plain("-C"),
            plain("-d"),
            plain("-f"),
            plain("-g"),
            plain("-h"),
            plain("-i"),
            plain("-M"),
            plain("-m"),
            plain("-n"),
            plain("-r"),
            plain("-s"),
            plain("-u"),
            plain("-V"),
            plain("-z"),
            takes("-k"),
            takes("-t"),
            plain("--ignore-leading-blanks"),
            plain("--ignore-case"),
            plain("--numeric-sort"),
            plain("--reverse"),
            plain("--stable"),
            plain("--unique"),
            takes("--key"),
            takes("--field-separator"),
        ],
        excluded: &[
            "-o",
            "--output",
            "--compress-program",
            "-T",
            "--temporary-directory",
            "--files0-from",
            "--random-source",
            "-S",
            "--buffer-size",
        ],
        numeric: false,
        operands: 0,
        reads_files: true,
        dash_is_stdin: true,
        needs_path: false,
        most_paths: None,
    },
    // The second operand of `uniq INPUT OUTPUT` is a file it writes, so at most one operand is
    // admitted. -D, -w and -z exist under one implementation only and are left out.
    Filter {
        program: "uniq",
        flags: &[
            plain("-c"),
            plain("-d"),
            plain("-i"),
            plain("-u"),
            takes("-f"),
            takes("-s"),
        ],
        excluded: &[],
        numeric: false,
        operands: 0,
        reads_files: true,
        dash_is_stdin: true,
        needs_path: false,
        most_paths: Some(1),
    },
    // Two paths compared, never a tree: GNU `-r` follows a link to a directory, -l pipes through
    // `pr`, and -X and --from-file name files of names. Both operands are read, so both are in the
    // answer. A directory operand compares the file of the same name inside it, which is a file
    // beneath a path the answer already names.
    Filter {
        program: "diff",
        flags: &[
            plain("-a"),
            plain("-b"),
            plain("-B"),
            plain("-c"),
            plain("-i"),
            plain("-p"),
            plain("-q"),
            plain("-s"),
            plain("-u"),
            plain("-w"),
            takes("-C"),
            takes("-L"),
            takes("-U"),
            plain("--brief"),
            plain("--ignore-case"),
            plain("--ignore-all-space"),
            plain("--ignore-space-change"),
            plain("--ignore-blank-lines"),
            plain("--text"),
        ],
        excluded: &[
            "-r",
            "--recursive",
            "-l",
            "--paginate",
            "-X",
            "--exclude-from",
            "--from-file",
            "--to-file",
        ],
        numeric: false,
        operands: 0,
        reads_files: true,
        dash_is_stdin: true,
        needs_path: false,
        most_paths: Some(2),
    },
    // Reports a path's metadata. GNU -c names a format and BSD has no -c, which then reads nothing.
    // -f is a format under BSD and a flag under GNU, and -t a time format under BSD and a flag under
    // GNU, so neither is listed. With no path BSD reports on standard input.
    Filter {
        program: "stat",
        flags: &[takes("-c"), takes("--format")],
        excluded: &["-f", "-t", "-L", "--dereference"],
        numeric: false,
        operands: 0,
        reads_files: true,
        dash_is_stdin: false,
        needs_path: true,
        most_paths: None,
    },
    // Walks each named path and reports sizes from metadata. With no path it walks the working
    // directory. -L and -H follow links out of the tree, and -X, --exclude-from and --files0-from
    // name files of names.
    Filter {
        program: "du",
        flags: &[
            plain("-a"),
            plain("-c"),
            plain("-h"),
            plain("-k"),
            plain("-m"),
            plain("-s"),
            plain("-x"),
            takes("-d"),
            plain("--summarize"),
            plain("--total"),
            plain("--human-readable"),
            takes("--max-depth"),
        ],
        excluded: &[
            "-L",
            "-H",
            "--dereference",
            "-X",
            "--exclude-from",
            "--files0-from",
        ],
        numeric: false,
        operands: 0,
        reads_files: true,
        dash_is_stdin: false,
        needs_path: true,
        most_paths: None,
    },
];

/// Programs that must never be eligible, whatever their arguments.
///
/// Held explicitly rather than left absent so the reason survives, and so a test can assert they
/// stay out. Absence is easy to reverse by accident; a named exclusion is not.
pub const NEVER: &[&str] = &[
    "sed", "awk", "gawk", "perl", "python", "python3", "ruby", "sh", "bash", "zsh", "tee", "dd",
    "xargs", "find",
];

/// The paths a resolved program with these arguments reads, or `None` where nothing here can say.
///
/// `Some(paths)` is a claim that the call writes no file, starts no process, opens no socket, and
/// reads nothing except stdin and the paths returned. `Some(&[])` says stdin was its only input.
///
/// Conservative by construction: an unknown program, an option the entry does not list, an option
/// written after an operand, an operand the program has no reading for, a call with more operands
/// than the entry admits, a call without the path the entry needs, and a recursive call naming no
/// path all answer `None`, and `None` means the caller keeps the opaque default.
pub fn read_set(program: &str, args: &[String]) -> Option<Vec<String>> {
    // A path resolves to a file name; the table names programs.
    let name = program_name(program);

    if NEVER.contains(&name) {
        return None;
    }

    let filter = FILTERS.iter().find(|f| f.program == name)?;

    let mut operands: Vec<&str> = Vec::new();
    let mut recursing = false;
    // Past `--` a word is an operand however it is spelled, which is what the program itself does.
    // Reading `-i` after one as an option would leave a file the program opens out of the answer.
    let mut only_operands_left = false;
    let mut at = 0usize;

    while let Some(arg) = args.get(at) {
        at += 1;
        if only_operands_left {
            operands.push(arg);
            continue;
        }
        let argument = recognise(filter, arg)?;
        // GNU reads this word as an option, and a program that stops reading options at its first
        // operand opens it as a file. `--` is one of them: past an operand it names a file too.
        if !operands.is_empty() && argument != Argument::Operand {
            return None;
        }
        match argument {
            Argument::EndOfFlags => only_operands_left = true,
            Argument::Flag => {}
            Argument::Recursive => recursing = true,
            // The value, wherever it goes, is part of the instruction. Skipped so it is never
            // counted as an operand, which is what would make a call naming no path look like one
            // naming a path.
            Argument::FlagTakingTheNextWord => at += 1,
            Argument::Operand => operands.push(arg),
        }
    }

    // The first `operands` of them are part of the instruction, such as grep's pattern or tr's
    // character sets. Beyond that they are paths, except a `-` the entry says names standard input:
    // the caller's meet covers that already, and it is not a file anybody vouched for.
    let paths: Vec<String> = operands
        .iter()
        .skip(filter.operands)
        .filter(|operand| !(filter.dash_is_stdin && **operand == "-"))
        .map(|path| (*path).to_string())
        .collect();

    if !paths.is_empty() && !filter.reads_files {
        return None;
    }
    if recursing && paths.is_empty() {
        return None;
    }
    if filter.needs_path && paths.is_empty() {
        return None;
    }
    // Counted before a `-` is dropped: `uniq - out` names its output where an input would be.
    if filter
        .most_paths
        .is_some_and(|most| operands.len().saturating_sub(filter.operands) > most)
    {
        return None;
    }

    Some(paths)
}

/// Whether a resolved program with these arguments only transforms stdin.
///
/// The narrow case of [`read_set`]: an answer that names no path at all, so the output is a
/// function of what was piped in and of nothing on disk.
pub fn is_pure_filter(program: &str, args: &[String]) -> bool {
    read_set(program, args).is_some_and(|paths| paths.is_empty())
}

/// The file name part of a program path, so `/usr/bin/wc` matches `wc`.
fn program_name(program: &str) -> &str {
    program
        .rsplit_once('/')
        .map(|(_, name)| name)
        .unwrap_or(program)
}

/// What one argument is to the program it was given to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Argument {
    /// `--`, after which nothing is an option.
    EndOfFlags,
    /// An option that changes nothing about what is read.
    Flag,
    /// An option that makes the program read its path operands as trees.
    Recursive,
    /// An option whose value is the argument after it.
    FlagTakingTheNextWord,
    /// A word that is not an option.
    Operand,
}

/// How one argument bears on the proof, or `None` where the entry does not recognise it.
fn recognise(filter: &Filter, arg: &str) -> Option<Argument> {
    if arg == "--" {
        return Some(Argument::EndOfFlags);
    }

    // `-` alone names standard input or a file called `-`, as [`Filter::dash_is_stdin`] says. An
    // operand rather than an option, because it occupies an operand's place: for a program whose
    // first operand is an instruction, reading it as an option
    // would make the next word the instruction and leave the file it actually reads out of the
    // answer.
    if arg == "-" || !arg.starts_with('-') {
        return Some(Argument::Operand);
    }

    if let Some(long) = arg.strip_prefix("--") {
        let (name, value_attached) = match long.split_once('=') {
            Some((name, _)) => (name, true),
            None => (long, false),
        };
        return match (effect_of(filter, &format!("--{name}"))?, value_attached) {
            (Effect::Takes, true) => Some(Argument::Flag),
            (Effect::Takes, false) => Some(Argument::FlagTakingTheNextWord),
            // A value on an option that takes none is a spelling the entry does not describe.
            (_, true) => None,
            (Effect::Plain, false) => Some(Argument::Flag),
            (Effect::Recursive, false) => Some(Argument::Recursive),
        };
    }

    let letters = &arg[1..];

    // `-20` is a count rather than an option: there is no letter in it to look up.
    if filter.numeric && letters.chars().all(|letter| letter.is_ascii_digit()) {
        return Some(Argument::Flag);
    }

    // The whole spelling first, so an option given its value as the next word is recognised before
    // the reading below gets to it.
    if let Some(effect) = effect_of(filter, arg) {
        return Some(match effect {
            Effect::Plain => Argument::Flag,
            Effect::Recursive => Argument::Recursive,
            Effect::Takes => Argument::FlagTakingTheNextWord,
        });
    }

    // `-d:` is `-d` with its value attached, which is a reading available only to an option that
    // takes one.
    let first = letters.chars().next()?;
    if letters.chars().count() > 1 && effect_of(filter, &format!("-{first}")) == Some(Effect::Takes)
    {
        return Some(Argument::Flag);
    }

    // Otherwise every letter is an option of its own. One that takes a value cannot be read here:
    // where in the bundle its value sits would be a guess.
    let mut found = Argument::Flag;
    for letter in letters.chars() {
        match effect_of(filter, &format!("-{letter}"))? {
            Effect::Plain => {}
            Effect::Recursive => found = Argument::Recursive,
            Effect::Takes => return None,
        }
    }
    Some(found)
}

/// What the entry says about one option, spelled exactly as it is matched.
fn effect_of(filter: &Filter, spelling: &str) -> Option<Effect> {
    filter
        .flags
        .iter()
        .find(|flag| flag.spelling == spelling)
        .map(|flag| flag.effect)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|a| a.to_string()).collect()
    }

    #[test]
    fn a_plain_filter_reading_stdin_qualifies() {
        assert!(is_pure_filter("wc", &args(&["-l"])));
        assert!(is_pure_filter("head", &args(&["-20"])));
        assert!(is_pure_filter("cut", &args(&["-d:", "-f1"])));
        assert!(is_pure_filter("grep", &args(&["-i", "error"])));
    }

    /// A resolved absolute path is what a caller will have, so the table has to match on the file
    /// name rather than the whole path.
    #[test]
    fn a_resolved_path_matches_its_program_name() {
        assert!(is_pure_filter("/usr/bin/wc", &args(&["-l"])));
        assert!(!is_pure_filter("/usr/bin/sed", &args(&["s/a/b/"])));
    }

    /// The most important test here. These are the two programs a reader would list first, and both
    /// can write files and run commands from an argument, so neither may ever qualify.
    #[test]
    fn interpreters_never_qualify_however_harmless_they_look() {
        for program in ["sed", "awk", "gawk"] {
            assert!(
                !is_pure_filter(program, &args(&[])),
                "{program} qualified with no arguments"
            );
            assert!(
                !is_pure_filter(program, &args(&["-n", "1p"])),
                "{program} qualified with innocuous-looking arguments"
            );
        }
    }

    /// Awk's `system()` reaches the shell this repository excludes, so admitting awk would undo that
    /// exclusion by another route.
    #[test]
    fn awk_cannot_qualify_even_though_it_looks_like_a_filter() {
        assert!(!is_pure_filter("awk", &args(&["{print $1}"])));
        assert!(!is_pure_filter("awk", &args(&["{system(\"rm -rf /\")}"])));
    }

    /// `sort` and `uniq` take an output file, `sort` through `-o` and `uniq` through a second
    /// operand. Each shape is refused by the entry, so the call is unproven however the file is
    /// spelled, and `tee` stays out of the table altogether.
    #[test]
    fn a_sort_or_uniq_call_that_names_an_output_file_proves_nothing() {
        let calls: &[(&str, &[&str])] = &[
            ("sort", &["-o", "out.txt", "in.txt"]),
            ("sort", &["-oout.txt", "in.txt"]),
            ("sort", &["--output=out.txt", "in.txt"]),
            ("sort", &["--output", "out.txt", "in.txt"]),
            ("sort", &["-ro", "out.txt", "in.txt"]),
            ("sort", &["--compress-program=sh", "in.txt"]),
            ("sort", &["-T", "/tmp", "in.txt"]),
            ("sort", &["--temporary-directory=/tmp", "in.txt"]),
            ("sort", &["--files0-from=list"]),
            ("sort", &["--out=out.txt", "in.txt"]),
            ("uniq", &["in.txt", "out.txt"]),
            ("uniq", &["-c", "in.txt", "out.txt"]),
            // A dash as the input leaves the second operand as the output file.
            ("uniq", &["-", "out.txt"]),
            ("uniq", &["in.txt", "-", "extra"]),
        ];
        for (program, call) in calls {
            assert_eq!(
                read_set(program, &args(call)),
                None,
                "{program} {call:?} was proven"
            );
        }
        assert!(!is_pure_filter("tee", &args(&["out.txt"])));
    }

    /// The point of adding them: the ordinary reading shapes answer with the paths they read.
    #[test]
    fn sort_and_uniq_reading_one_input_answer_with_it() {
        assert_eq!(read_set("sort", &args(&["-rn"])), Some(vec![]));
        assert_eq!(
            read_set(
                "sort",
                &args(&["-t", ",", "-k", "2,2", "-u", "rows.csv", "more.csv"])
            ),
            Some(vec!["rows.csv".to_string(), "more.csv".to_string()])
        );
        assert_eq!(read_set("uniq", &args(&["-c"])), Some(vec![]));
        assert_eq!(
            read_set("uniq", &args(&["-f", "1", "names.txt"])),
            Some(vec!["names.txt".to_string()])
        );
        // A second operand is an output position whatever it is spelled, `-` included.
        assert_eq!(read_set("uniq", &args(&["names.txt", "-"])), None);
    }

    /// `diff` compares two paths. A tree walk follows a link to a directory under GNU, and `-l`
    /// pipes the output through `pr`, so neither is listed.
    #[test]
    fn diff_compares_two_paths_and_never_walks_a_tree() {
        assert_eq!(
            read_set("diff", &args(&["-u", "a.txt", "b.txt"])),
            Some(vec!["a.txt".to_string(), "b.txt".to_string()])
        );
        assert_eq!(
            read_set(
                "diff",
                &args(&["-U", "5", "-L", "old", "-L", "new", "a", "b"])
            ),
            Some(vec!["a".to_string(), "b".to_string()])
        );
        let calls: &[&[&str]] = &[
            &["-r", "a", "b"],
            &["-ru", "a", "b"],
            &["--recursive", "a", "b"],
            &["-l", "a", "b"],
            &["--paginate", "a", "b"],
            &["-X", "patterns", "a", "b"],
            &["--from-file=a", "b"],
            &["--to-file=a", "b"],
            &["a", "b", "c"],
        ];
        for call in calls {
            assert_eq!(
                read_set("diff", &args(call)),
                None,
                "diff {call:?} was proven"
            );
        }
    }

    /// `ls`, `stat` and `du` report on what they are pointed at. Called with no path they report on
    /// the working directory, or under BSD `stat` on standard input, which the answer cannot name.
    #[test]
    fn a_reporting_program_with_no_path_proves_nothing() {
        for program in ["ls", "stat", "du"] {
            assert_eq!(read_set(program, &args(&[])), None, "{program} was proven");
        }
        assert_eq!(read_set("ls", &args(&["-la"])), None);
        assert_eq!(read_set("du", &args(&["-sh"])), None);
        assert_eq!(read_set("stat", &args(&["-c", "%s"])), None);
        assert_eq!(
            read_set("ls", &args(&["-la", "src"])),
            Some(vec!["src".to_string()])
        );
        assert_eq!(
            read_set("du", &args(&["-sh", "src", "docs"])),
            Some(vec!["src".to_string(), "docs".to_string()])
        );
        assert_eq!(
            read_set("stat", &args(&["-c", "%s", "Cargo.toml"])),
            Some(vec!["Cargo.toml".to_string()])
        );
        // A recursive listing still has to name the tree it lists.
        assert_eq!(read_set("ls", &args(&["-R"])), None);
        assert_eq!(
            read_set("ls", &args(&["-R", "src"])),
            Some(vec!["src".to_string()])
        );
    }

    /// An option that follows a link to a target the answer does not name, or that reads a file of
    /// names, leaves the call unproven.
    #[test]
    fn a_reporting_program_following_links_or_reading_a_list_proves_nothing() {
        let calls: &[(&str, &[&str])] = &[
            ("ls", &["-L", "src"]),
            ("ls", &["-H", "src"]),
            ("ls", &["-lL", "src"]),
            ("ls", &["--dereference", "src"]),
            ("ls", &["--color", "src"]),
            ("stat", &["-L", "link"]),
            ("stat", &["-f", "%N", "link"]),
            ("stat", &["-t", "link"]),
            ("du", &["-L", "src"]),
            ("du", &["-H", "src"]),
            ("du", &["--files0-from=list"]),
            ("du", &["-X", "patterns", "src"]),
        ];
        for (program, call) in calls {
            assert_eq!(
                read_set(program, &args(call)),
                None,
                "{program} {call:?} was proven"
            );
        }
    }

    /// `cat` of a named file is the commonest read there is, and an option it does not list leaves
    /// the call unproven.
    #[test]
    fn cat_answers_with_the_files_it_reads() {
        assert_eq!(
            read_set("cat", &args(&["-n", "a.md", "b.md"])),
            Some(vec!["a.md".to_string(), "b.md".to_string()])
        );
        assert_eq!(
            read_set("cat", &args(&["-", "a.md"])),
            Some(vec!["a.md".to_string()])
        );
        assert_eq!(read_set("cat", &args(&["--no-such-option", "a.md"])), None);
        assert_eq!(read_set("cat", &args(&["a.md", "-n"])), None);
    }

    /// A shell is never a filter, whatever it is asked to do.
    #[test]
    fn shells_never_qualify() {
        for shell in ["sh", "bash", "zsh"] {
            assert!(!is_pure_filter(shell, &args(&["-c", "echo hi"])));
        }
    }

    /// An unrecognised program falls through to the opaque default rather than being guessed at.
    #[test]
    fn an_unknown_program_does_not_qualify() {
        assert!(!is_pure_filter("some-tool-nobody-listed", &args(&[])));
        assert!(!is_pure_filter("git", &args(&["log"])));
        assert!(!is_pure_filter("curl", &args(&["https://example.com"])));
    }

    /// A file operand is an input the label would not account for, so it disqualifies even a program
    /// that is otherwise eligible.
    #[test]
    fn a_file_operand_disqualifies_an_otherwise_pure_filter() {
        assert!(!is_pure_filter("head", &args(&["-1", "/etc/hosts"])));
        assert!(!is_pure_filter("wc", &args(&["-l", "secrets.txt"])));
        assert!(!is_pure_filter("grep", &args(&["error", "log.txt"])));
    }

    /// A bare `-` means stdin to GNU and to BSD `cut` and `grep`, so it stays out of their answer.
    /// BSD `head`, `tail` and `wc` open a file called `-`, and an answer without it would leave
    /// that file out.
    #[test]
    fn a_lone_dash_is_a_path_where_bsd_opens_a_file_by_that_name() {
        assert!(is_pure_filter("cut", &args(&["-c1-3", "-"])));
        assert!(is_pure_filter("grep", &args(&["TODO", "-"])));
        let calls: &[(&str, &[&str])] = &[
            ("head", &["-n", "1", "-"]),
            ("tail", &["-"]),
            ("wc", &["-l", "-"]),
        ];
        for (program, call) in calls {
            assert_eq!(
                read_set(program, &args(call)),
                Some(vec!["-".to_string()]),
                "{program} {call:?} left out the file BSD opens"
            );
        }
        // tr reads no file, so a `-` past its two sets is an operand it has no reading for.
        assert_eq!(read_set("tr", &args(&["a", "b", "-"])), None);
    }

    /// `grep -f` names a pattern file, so it reads from disk without looking like it does.
    #[test]
    fn grep_reading_a_pattern_file_does_not_qualify() {
        assert!(!is_pure_filter("grep", &args(&["-f", "patterns.txt"])));
        assert!(!is_pure_filter("grep", &args(&["--file=patterns.txt"])));
    }

    /// The hole adversarial testing found: `grep -r pattern` with no file operand does not read
    /// stdin at all. It recurses the working directory, so the output would be labelled from stdin
    /// while the data came from the filesystem.
    #[test]
    fn grep_recursing_the_filesystem_does_not_qualify() {
        assert!(!is_pure_filter("grep", &args(&["-r", "secret"])));
        assert!(!is_pure_filter("grep", &args(&["-R", "secret"])));
        assert!(!is_pure_filter("grep", &args(&["--recursive", "secret"])));
        // Bundled with another flag, it must still be caught.
        assert!(!is_pure_filter("grep", &args(&["-ir", "secret"])));
    }

    /// Directory traversal flags are the same hazard: they name inputs the label cannot account for.
    #[test]
    fn grep_directory_traversal_flags_do_not_qualify() {
        assert!(!is_pure_filter("grep", &args(&["-d", "recurse", "x"])));
        assert!(!is_pure_filter(
            "grep",
            &args(&["--directories=recurse", "x"])
        ));
    }

    /// An option bundled with others must still be looked up, or `-if` would slip past a check that
    /// only compared whole arguments.
    #[test]
    fn an_option_bundled_with_others_is_still_looked_up() {
        // -f names a pattern file, so it is excluded, and a bundle carrying it proves nothing.
        assert_eq!(read_set("grep", &args(&["-if", "patterns.txt"])), None);
        assert_eq!(read_set("tail", &args(&["-qf"])), None);
        // An option that takes a value cannot be read inside a bundle at all: where its value sits
        // would be a guess.
        assert_eq!(read_set("grep", &args(&["-iA", "3", "x"])), None);
    }

    /// Following a file never terminates, so a stage doing it would hang the turn.
    #[test]
    fn tail_following_a_file_does_not_qualify() {
        assert!(!is_pure_filter("tail", &args(&["-f"])));
        assert!(!is_pure_filter("tail", &args(&["-F"])));
        assert!(!is_pure_filter("tail", &args(&["--follow"])));
        // But an ordinary tail is fine.
        assert!(is_pure_filter("tail", &args(&["-5"])));
    }

    /// The string transforms take their input as arguments rather than reading files, so an operand
    /// is not a file for them.
    #[test]
    fn string_transforms_take_operands_rather_than_files() {
        assert!(is_pure_filter("basename", &args(&["/a/b/c.txt"])));
        assert!(is_pure_filter("dirname", &args(&["/a/b/c.txt"])));
        assert!(is_pure_filter("tr", &args(&["a-z", "A-Z"])));
    }

    /// An operand's meaning is per-program, so the count is what decides. grep's first operand is a
    /// pattern and its second is a file, and only the second disqualifies.
    #[test]
    fn a_pattern_is_an_instruction_but_a_second_operand_is_a_file() {
        assert!(is_pure_filter("grep", &args(&["error"])));
        assert!(!is_pure_filter("grep", &args(&["error", "log.txt"])));
    }

    /// The counts have to be exact in both directions, or a file would slip past as an instruction.
    #[test]
    fn an_operand_beyond_the_count_is_a_file() {
        // tr takes two sets and reads only stdin, so a third operand is a file.
        assert!(is_pure_filter("tr", &args(&["a-z", "A-Z"])));
        assert!(!is_pure_filter("tr", &args(&["a-z", "A-Z", "input.txt"])));

        // dirname takes one path.
        assert!(is_pure_filter("dirname", &args(&["/a/b"])));
        assert!(!is_pure_filter("dirname", &args(&["/a/b", "/c/d"])));
    }

    /// wc takes no operands at all: every one of them is a file it would read.
    #[test]
    fn a_program_with_no_operands_rejects_the_first_one() {
        assert!(is_pure_filter("wc", &args(&["-l"])));
        assert!(!is_pure_filter("wc", &args(&["anything"])));
    }

    /// Flags do not consume the operand budget, or `grep -i error` would look like two operands.
    #[test]
    fn flags_do_not_count_against_the_operand_budget() {
        assert!(is_pure_filter("grep", &args(&["-i", "-v", "error"])));
        assert!(!is_pure_filter(
            "grep",
            &args(&["-i", "-v", "error", "f.txt"])
        ));
    }

    /// `pwd` takes nothing and reports what the user established.
    #[test]
    fn pwd_qualifies() {
        assert!(is_pure_filter("pwd", &args(&[])));
        assert!(is_pure_filter("pwd", &args(&["-P"])));
    }

    /// The extension the proof road rests on: a named file is an input the answer accounts for
    /// rather than a reason to give up, so a caller can take the label from that file.
    #[test]
    fn a_named_file_is_the_answer_rather_than_a_refusal() {
        assert_eq!(
            read_set("wc", &args(&["-l", "Cargo.toml"])),
            Some(vec!["Cargo.toml".to_string()])
        );
        assert_eq!(
            read_set("grep", &args(&["error", "log.txt", "other.txt"])),
            Some(vec!["log.txt".to_string(), "other.txt".to_string()])
        );
        assert_eq!(read_set("grep", &args(&["-i", "error"])), Some(vec![]));
    }

    /// Recursion is provable once the paths it walks are named, which is what makes searching a
    /// large tree readable rather than opaque.
    #[test]
    fn a_recursive_search_answers_with_the_trees_it_walks() {
        assert_eq!(
            read_set("grep", &args(&["-r", "TODO", "src"])),
            Some(vec!["src".to_string()])
        );
        assert_eq!(
            read_set("grep", &args(&["--recursive", "TODO", "src", "docs"])),
            Some(vec!["src".to_string(), "docs".to_string()])
        );
    }

    /// A recursive call naming no path walks the working directory under one implementation and
    /// reads stdin under another, so what the label would come from is not established.
    #[test]
    fn recursion_naming_no_path_proves_nothing() {
        assert_eq!(read_set("grep", &args(&["-r", "secret"])), None);
        assert_eq!(read_set("grep", &args(&["-ir", "secret"])), None);
    }

    /// `-R` follows every symlink it meets, so the tree it reads is not the tree the answer names
    /// and a link inside a vouched-for directory would carry the label out of it.
    #[test]
    fn recursion_that_follows_symlinks_proves_nothing() {
        assert_eq!(read_set("grep", &args(&["-R", "TODO", "src"])), None);
        assert_eq!(
            read_set("grep", &args(&["--dereference-recursive", "TODO", "src"])),
            None
        );
    }

    /// `-e` supplies the pattern, so the operand count no longer says which operand is the pattern
    /// and which is a file. Parsing each option's arity is the race this module refuses to enter.
    #[test]
    fn a_flag_that_supplies_the_pattern_proves_nothing() {
        assert_eq!(read_set("grep", &args(&["-e", "error", "log.txt"])), None);
        assert_eq!(read_set("grep", &args(&["--regexp=error"])), None);
        assert_eq!(read_set("grep", &args(&["-ie", "error"])), None);
    }

    /// `--files0-from` names a file holding the names of more, so the files actually read would
    /// never appear in the answer.
    #[test]
    fn a_flag_naming_a_file_of_names_proves_nothing() {
        assert_eq!(read_set("wc", &args(&["--files0-from=list"])), None);
        assert_eq!(read_set("wc", &args(&["--files0-from", "list"])), None);
    }

    /// Past `--` the program treats every word as an operand, so reading one as a flag would leave
    /// a file it opens out of the answer.
    #[test]
    fn a_word_past_the_end_of_flags_marker_is_an_operand() {
        assert_eq!(
            read_set("grep", &args(&["--", "-i", "notes.txt"])),
            Some(vec!["notes.txt".to_string()])
        );
        assert_eq!(
            read_set("grep", &args(&["--", "pattern", "-i"])),
            Some(vec!["-i".to_string()])
        );
        // The pattern itself can be spelled like a flag, and then nothing is read from disk.
        assert_eq!(read_set("grep", &args(&["--", "-i"])), Some(vec![]));
    }

    /// A program with no reading for a path operand is not proven by being handed one: the call is
    /// one nothing here recognises rather than one that reads a file.
    #[test]
    fn an_operand_a_program_has_no_reading_for_proves_nothing() {
        assert_eq!(read_set("tr", &args(&["a-z", "A-Z", "input.txt"])), None);
        assert_eq!(read_set("dirname", &args(&["/a/b", "/c/d"])), None);
        assert_eq!(read_set("pwd", &args(&["anything"])), None);
    }

    /// The hole an option denylist leaves open. An option nobody listed as bad reads as harmless,
    /// so a call whose value shifts the operand count looks like one naming a path when it names
    /// none, and a recursive walk of the working directory answers as a walk of one directory.
    #[test]
    fn an_option_value_is_never_counted_as_a_path() {
        // `grep -A 1 -r x` walks the working directory and names nothing. Counting the 1 as the
        // pattern would leave x as the answer and the walk unaccounted for.
        assert_eq!(read_set("grep", &args(&["-A", "1", "-r", "TODO"])), None);
        assert_eq!(read_set("grep", &args(&["-m", "5", "-r", "TODO"])), None);
        // The same value, where a path is named, must not appear in the answer.
        assert_eq!(
            read_set("head", &args(&["-n", "20", "notes.txt"])),
            Some(vec!["notes.txt".to_string()])
        );
        assert_eq!(
            read_set("cut", &args(&["-d", ",", "-f", "1", "rows.csv"])),
            Some(vec!["rows.csv".to_string()])
        );
    }

    /// BSD grep gives `--context` an optional value, so `grep --context TODO secret.txt` searches
    /// `secret.txt` for `TODO` there, while GNU takes `TODO` as the value and reads stdin. An
    /// answer taken from the GNU reading leaves out the file the BSD one prints.
    #[test]
    fn a_context_option_given_its_value_apart_proves_nothing() {
        assert_eq!(
            read_set("grep", &args(&["--context", "TODO", "secret.txt"])),
            None
        );
        // -C takes a required value under both, so a call spelled with it is still proven.
        assert_eq!(
            read_set("grep", &args(&["-C", "2", "TODO", "notes.txt"])),
            Some(vec!["notes.txt".to_string()])
        );
    }

    /// GNU reads an option anywhere in argv, while BSD `head`, `tail`, `wc` and `cut`, and GNU with
    /// `POSIXLY_CORRECT` set, stop reading options at the first operand and open every word after
    /// it as a file. Under the second reading each call below also prints `secret.txt` or a file
    /// named like the option, and an answer taken from the first reading leaves that file out.
    #[test]
    fn an_option_after_an_operand_proves_nothing() {
        let calls: &[(&str, &[&str])] = &[
            ("head", &["a.md", "-n", "secret.txt"]),
            ("tail", &["a.md", "-n", "secret.txt"]),
            ("cut", &["-c1-3", "a.md", "-c", "secret.txt"]),
            ("grep", &["TODO", "a.md", "-A", "secret.txt"]),
            ("wc", &["a.md", "-l"]),
            ("grep", &["TODO", "a.md", "-l"]),
            // Past an operand, `--` is a file name too.
            ("head", &["a.md", "--", "secret.txt"]),
            ("grep", &["TODO", "--", "-i"]),
        ];
        for (program, call) in calls {
            assert_eq!(
                read_set(program, &args(call)),
                None,
                "{program} {call:?} was proven"
            );
        }
    }

    /// An abbreviation of a long option is not that option. Deciding which option `--recursi`
    /// abbreviates is the option parsing this module refuses to do, and an entry that guessed would
    /// let every exclusion be spelled around.
    #[test]
    fn an_abbreviated_long_option_proves_nothing() {
        assert_eq!(read_set("grep", &args(&["--recursi", "TODO"])), None);
        assert_eq!(read_set("grep", &args(&["--rege=TODO"])), None);
        assert_eq!(read_set("wc", &args(&["--files0-f=list"])), None);
    }

    /// An option the entry does not list proves nothing, whatever it looks like. This is what makes
    /// the list an allowlist: `-S` makes one grep follow symlinks out of the tree it was pointed
    /// at, and nobody has to have thought of it.
    #[test]
    fn an_unlisted_option_proves_nothing() {
        assert_eq!(read_set("grep", &args(&["-S", "-r", "TODO", "src"])), None);
        assert_eq!(read_set("grep", &args(&["-rS", "TODO", "src"])), None);
        assert_eq!(read_set("wc", &args(&["--no-such-option"])), None);
    }

    /// `grep - file` gives the pattern as a dash, so the dash occupies the operand a pattern would
    /// have occupied. Reading it as an option would make the file the pattern and leave the file
    /// grep reads out of the answer entirely.
    #[test]
    fn a_dash_occupies_an_operands_place() {
        assert_eq!(
            read_set("grep", &args(&["-", "secret.txt"])),
            Some(vec!["secret.txt".to_string()])
        );
        // And where the dash is the input rather than the instruction, it names stdin and not a
        // file anybody vouched for.
        assert_eq!(read_set("grep", &args(&["pattern", "-"])), Some(vec![]));
    }

    /// Nothing may appear in both tables, or the answer would depend on which was consulted first.
    #[test]
    fn the_tables_do_not_overlap() {
        for filter in FILTERS {
            assert!(
                !NEVER.contains(&filter.program),
                "{} is both eligible and forbidden",
                filter.program
            );
        }
    }

    /// Every spelling in an entry must look like an option, or it is describing a rule nothing can
    /// apply.
    #[test]
    fn every_spelling_looks_like_an_option() {
        for filter in FILTERS {
            for spelling in filter
                .flags
                .iter()
                .map(|flag| flag.spelling)
                .chain(filter.excluded.iter().copied())
            {
                assert!(
                    spelling.starts_with('-'),
                    "{}: '{spelling}' is not an option",
                    filter.program
                );
            }
        }
    }

    /// The load-bearing invariant of the two lists. An option in both would be recognised, so the
    /// reason written beside it in the exclusions would be describing behaviour the code does not
    /// have.
    #[test]
    fn no_option_is_both_listed_and_excluded() {
        for filter in FILTERS {
            for spelling in filter.excluded {
                assert!(
                    !filter.flags.iter().any(|flag| flag.spelling == *spelling),
                    "{}: '{spelling}' is both listed and excluded",
                    filter.program
                );
            }
        }
    }

    /// A recursive option on a program that has no path operand to walk describes a rule nothing
    /// can apply, since recursion naming no path is never proven.
    #[test]
    fn only_a_program_that_reads_files_recurses() {
        for filter in FILTERS {
            let recurses = filter
                .flags
                .iter()
                .any(|flag| flag.effect == Effect::Recursive);
            assert!(
                !recurses || filter.reads_files,
                "{} recurses but reads no path operand",
                filter.program
            );
        }
    }

    /// Every excluded option must be refused, which is what makes the list beside it a record of a
    /// decision rather than a comment.
    #[test]
    fn an_excluded_option_is_refused() {
        for filter in FILTERS {
            for spelling in filter.excluded {
                let call = args(&[spelling, "x", "y", "z"]);
                assert_eq!(
                    read_set(filter.program, &call),
                    None,
                    "{}: '{spelling}' was accepted",
                    filter.program
                );
            }
        }
    }
}
