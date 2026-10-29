//! Every word this program says to a person, and the machinery for saying it in their language.
//!
//! Catalogs live in `locales/`, one file per locale, in a subset of Project Fluent. The build
//! script compiles them into Rust, so what ships is `&'static str`s and the code that assembles
//! them, and a message is reached through [`t!`]:
//!
//! ```ignore
//! use bravebot_i18n::t;
//!
//! block.title(t!(confirm_write_title));
//! footer.push(t!(status_turns, count = session.turns));
//! ```
//!
//! ## What is not in here
//!
//! Only text a person reads. The words the planner reads are not translated and do not belong in
//! a catalog: a tool's description, the preamble, and the `refused: …` a tool answers with are all
//! part of the interface to the model, and rewording them in another language changes what the
//! model does. They are pinned by the specs for exactly that reason. The rule is the audience, not
//! the crate: a string in `bravebot-agent` that ends up on the screen is a message, and a string
//! in `bravebot-tui` that ends up in a request is not.
//!
//! ## Why a key can never be a value
//!
//! [`t!`] takes the message name as a bare word, and the macro has one arm per message in the
//! reference catalog. There is no arm that accepts a runtime string, so there is no way to write
//! a lookup whose result depends on a value, which means no path by which content the agent was
//! handed can choose what is said. A misspelled name does not fall back and does not render a key
//! on somebody's screen: it fails to compile, naming the line.
//!
//! Argument names are checked the same way, by being the fields of a generated struct. Passing an
//! argument the message does not take, or forgetting one it does, is a compilation error.
//!
//! ```
//! assert_eq!(bravebot_i18n::t!(count_turns, count = 2), "2 turns");
//! ```
//!
//! A name no catalog defines:
//!
//! ```compile_fail
//! bravebot_i18n::t!(no_message_is_called_this);
//! ```
//!
//! An argument the message does not take:
//!
//! ```compile_fail
//! bravebot_i18n::t!(count_turns, quantity = 2);
//! ```
//!
//! A key that is a value rather than a name, which is the shape this crate exists to make
//! unwritable:
//!
//! ```compile_fail
//! let chosen = untrusted_file_contents();
//! bravebot_i18n::t!(chosen);
//! ```
//!
//! ## Why nothing here can parse a catalog
//!
//! The parser is compiled for the build script and for tests and is not part of the library a
//! binary links, so a call to it is not a call anybody can write. The sibling module the
//! generated code does call is there:
//!
//! ```
//! assert_eq!(bravebot_i18n::plural::category("en", 1), "one");
//! ```
//!
//! The parser is not:
//!
//! ```compile_fail
//! bravebot_i18n::catalog::parse("hello = hi");
//! ```

#![forbid(unsafe_code)]
#![cfg_attr(
    test,
    allow(clippy::disallowed_methods, reason = "the test names its program")
)]

/// Where a question stands in a series the planner is asking, which every interface says alike.
pub mod ask;
/// The catalog format and its parser, compiled only for tests.
///
/// The build script does not reach it through here: it `include!`s `src/catalog.rs` into itself,
/// so the parser exists where a catalog is actually read, which is before the binary does. Gated
/// so the shipped library links no parser, which is what makes LOCALE-6 a property of the build
/// rather than of nobody calling an exported function.
#[cfg(test)]
mod catalog;
/// Which plural category a number falls into, per language.
pub mod plural;
/// The notes that say a size was too large to attach, which the terminal and the desktop share.
pub mod sizes;

/// The code the build script writes from those catalogs, compiled here so the rules it applies
/// have tests. Build-script code that nothing can call is build-script code nothing can pin, and
/// which catalog's plural rules form a message is exactly such a rule.
#[cfg(test)]
mod generate;

/// The desktop window's messages as TypeScript, written by a test so the window ships finished
/// text and links no parser either.
#[cfg(test)]
mod typescript;

/// The compiled catalogs: one item per message, plus [`Locale`] and the [`t!`] arms.
pub mod messages {
    include!(concat!(env!("OUT_DIR"), "/messages.rs"));
}

pub use messages::{DEFAULT, LOCALES, Locale};

use std::sync::OnceLock;

/// Set to a locale tag to override what the environment says, mostly so a test can pin one.
pub const LOCALE: &str = "BRAVEBOT_LOCALE";

/// A number a message counts on.
///
/// A distinct type rather than an integer so that every width a call site might hold converts on
/// the way in, and so a plural select can be sure it has something to compare.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Count(i64);

impl Count {
    pub(crate) fn get(self) -> i64 {
        self.0
    }
}

impl std::fmt::Display for Count {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

macro_rules! counts {
    (lossless $($from:ty)*) => { $(
        impl From<$from> for Count {
            fn from(value: $from) -> Self {
                Count(i64::from(value))
            }
        }
    )* };
    (saturating $($from:ty)*) => { $(
        impl From<$from> for Count {
            // A count past i64 is a count no message is going to read out anyway, and an
            // interface that panicked while drawing a number would be worse than one off by
            // the last few quintillion.
            fn from(value: $from) -> Self {
                Count(i64::try_from(value).unwrap_or(i64::MAX))
            }
        }
    )* };
}

counts!(lossless u8 u16 u32 i8 i16 i32 i64);
counts!(saturating u64 usize i128 u128 isize);

static CHOSEN: OnceLock<Locale> = OnceLock::new();

/// The catalog this run is using.
///
/// The reference until something says otherwise, which for the shipped binaries is one call to
/// [`init_from_environment`] at startup. A library that read the environment the first time
/// anything drew a word would make every test's output depend on the machine it ran on, and the
/// point of a rendered-output test is that two people reading it see the same thing.
pub fn locale() -> Locale {
    *CHOSEN.get().unwrap_or(&DEFAULT)
}

/// Fix the catalog for the rest of the process. The first call wins.
///
/// A person does not change their language partway through a session, and an interface that
/// redrew half of itself in another one would be worse than one that asked them to start again.
pub fn init(locale: Locale) {
    let _ = CHOSEN.set(locale);
}

/// What a person has said about their language somewhere other than the environment.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Said<'a> {
    /// The tag `/language` recorded.
    pub picked: Option<&'a str>,
    /// The tag the `language` key of the person's own settings file holds.
    pub written: Option<&'a str>,
}

/// The word that gives a choice back to the next source.
pub const AUTO: &str = "auto";

/// Fix the catalog from what the person said and from what their shell says their language is.
///
/// `BRAVEBOT_LOCALE` first, since a launch can name it for itself and a test pins a catalog with
/// it. Then the tag `/language` recorded, then the settings key, which both outrank the POSIX
/// variables because they were written for this program. Then the POSIX variables in the precedence
/// POSIX gives them.
pub fn init_from_environment(said: Said<'_>) {
    init(choose(said));
}

/// The catalog [`init_from_environment`] would fix, without fixing it.
///
/// For a process that chooses for a window other than itself: the desktop bridge keeps its own notes
/// in the reference and tells the window which catalog its chrome is read in.
pub fn choose(said: Said<'_>) -> Locale {
    chosen(|name| std::env::var(name).ok(), said)
}

fn chosen(variable: impl Fn(&str) -> Option<String>, said: Said<'_>) -> Locale {
    window_chosen(variable, said, &[])
}

/// [`choose`] for a desktop window, which where no source names a language takes the first of the
/// system's preferred languages a catalog answers.
///
/// A program started from a launcher has no shell, so the POSIX variables are usually absent there
/// and the operating system's own list is the only word on the person's language.
pub fn choose_for_window(said: Said<'_>, system: &[String]) -> Locale {
    window_chosen(|name| std::env::var(name).ok(), said, system)
}

fn window_chosen(
    variable: impl Fn(&str) -> Option<String>,
    said: Said<'_>,
    system: &[String],
) -> Locale {
    match requested(variable, said) {
        Some(requested) => resolve(&requested),
        None => system.iter().find_map(|tag| find(tag)).unwrap_or(DEFAULT),
    }
}

/// Whether `BRAVEBOT_LOCALE` names something, so that it outranks whatever the person picked.
pub fn overridden() -> bool {
    std::env::var(LOCALE).is_ok_and(|value| !value.trim().is_empty())
}

/// The request that decides the catalog, from the sources in the order each outranks the next.
///
/// A blank value asks for nothing. So does [`AUTO`] in what the person said, which is how they
/// hand the choice to the next source; the variables are read as they always were, where a blank
/// is the only value that asks for nothing.
fn requested(variable: impl Fn(&str) -> Option<String>, said: Said<'_>) -> Option<String> {
    let variable = |name: &str| variable(name).filter(|value| !value.trim().is_empty());
    let stated = |value: Option<&str>| {
        value
            .map(str::trim)
            .filter(|value| !value.is_empty() && !value.eq_ignore_ascii_case(AUTO))
            .map(str::to_string)
    };
    variable(LOCALE)
        .or_else(|| stated(said.picked))
        .or_else(|| stated(said.written))
        .or_else(|| {
            ["LC_ALL", "LC_MESSAGES", "LANG"]
                .into_iter()
                .find_map(variable)
        })
}

/// Which catalog answers a request.
pub fn resolve(requested: &str) -> Locale {
    find(requested).unwrap_or(DEFAULT)
}

/// The shipped catalog that answers a request, or `None` where none does.
///
/// [`resolve`] answers the reference there, which is right for a request from the environment and
/// wrong for a person naming a language: they are told there is no catalog rather than shown
/// English under the name they typed.
pub fn find(requested: &str) -> Option<Locale> {
    best(requested, &tags()).map(|index| LOCALES[index])
}

/// The tag of every shipped catalog, reference first.
pub fn tags() -> Vec<&'static str> {
    LOCALES.iter().map(|locale| locale.tag()).collect()
}

/// A word somebody typed or picked that no shipped catalog answers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoCatalog;

/// What a person asked `/language` or the window's picker to keep: `None` for [`AUTO`], which
/// gives the choice back, or the catalog that answers the word.
///
/// The terminal and the window both read a choice through this, so the two cannot come to differ
/// on what a word means. Keeping it is left to the caller, which is the one that knows whether it
/// has somewhere to keep it. A word no catalog answers is refused rather than read as the reference
/// (see [`find`]); what is kept is the catalog's own tag, a word the build knows whatever spelling
/// was typed.
pub fn pick(word: &str) -> Result<Option<Locale>, NoCatalog> {
    let word = word.trim();
    if word.eq_ignore_ascii_case(AUTO) {
        return Ok(None);
    }
    find(word).map(Some).ok_or(NoCatalog)
}

/// Which of `available` best answers `requested`, by the usual widening: the exact locale, then
/// any locale in the same language, then nothing.
///
/// Written against tags rather than against the catalogs that shipped so that the rule can be
/// tested on locale sets this build does not have.
fn best(requested: &str, available: &[&str]) -> Option<usize> {
    // POSIX spells "no locale at all" as C, and the encoding and modifier a shell appends say
    // nothing about which language the words should be in.
    let tag = requested
        .trim()
        .split(['.', '@'])
        .next()
        .unwrap_or_default()
        .replace('_', "-");
    if tag.is_empty() || tag.eq_ignore_ascii_case("c") || tag.eq_ignore_ascii_case("posix") {
        return None;
    }

    if let Some(exact) = available
        .iter()
        .position(|candidate| candidate.eq_ignore_ascii_case(&tag))
    {
        return Some(exact);
    }

    let language = tag.split('-').next().unwrap_or_default();
    available.iter().position(|candidate| {
        candidate
            .split('-')
            .next()
            .is_some_and(|other| other.eq_ignore_ascii_case(language))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHIPPED: [&str; 4] = ["en-US", "fr-CA", "fr-FR", "ja"];

    /// A rendered-output test has to say the same thing on every machine that runs it, so nothing
    /// about the environment reaches a message unless a binary asked for it.
    #[test]
    fn a_process_that_never_chose_a_locale_reads_the_reference() {
        assert_eq!(locale(), DEFAULT);
        assert_eq!(DEFAULT.tag(), "en-US");
    }

    /// The same test run in a process whose environment asks for another language. A `locale()`
    /// that read the environment lazily passes on an English or unset machine and fails here.
    #[test]
    fn a_process_whose_environment_names_a_language_still_reads_the_reference_until_asked() {
        // nosemgrep: rust.lang.security.current-exe.current-exe
        let exe = std::env::current_exe().expect("the test binary has a path");
        let other = LOCALES
            .iter()
            .find(|l| **l != DEFAULT)
            .expect("a build ships a second catalog")
            .tag();
        let output = std::process::Command::new(exe)
            .args([
                "--exact",
                "tests::a_process_that_never_chose_a_locale_reads_the_reference",
            ])
            .env(LOCALE, other)
            .env("LC_ALL", other)
            .env("LC_MESSAGES", other)
            .env("LANG", other)
            .output()
            .expect("the test binary starts");
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(output.status.success(), "child failed:\n{stdout}");
        assert!(stdout.contains("1 passed"), "child ran nothing:\n{stdout}");
    }

    /// The environment a launch has, as the lookup reads it.
    fn environment<'a>(set: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |name| {
            set.iter()
                .find(|(held, _)| *held == name)
                .map(|(_, value)| (*value).to_string())
        }
    }

    /// A person whose shell says English and who reads French has to be able to say so once, in
    /// the file: a settings key that lost to `LANG` would be a key that did nothing.
    #[test]
    fn a_language_the_person_wrote_outranks_the_posix_variables() {
        let said = Said {
            written: Some("fr"),
            ..Said::default()
        };
        let shell = [("LC_ALL", "en_US.UTF-8"), ("LANG", "en_US.UTF-8")];
        assert_eq!(requested(environment(&shell), said).as_deref(), Some("fr"));
    }

    /// With nothing written, the variables answer as they did before the key existed, so a machine
    /// already set up for French has nothing to do.
    #[test]
    fn without_a_choice_the_posix_variables_answer_in_their_order() {
        let shell = [
            ("LANG", "en_US"),
            ("LC_MESSAGES", "fr_FR"),
            ("LC_ALL", "ja_JP"),
        ];
        assert_eq!(
            requested(environment(&shell), Said::default()).as_deref(),
            Some("ja_JP")
        );
        let shell = [("LANG", "en_US"), ("LC_MESSAGES", "fr_FR")];
        assert_eq!(
            requested(environment(&shell), Said::default()).as_deref(),
            Some("fr_FR")
        );
    }

    /// `/language` is the later and narrower act, so it answers over a file written by hand: a
    /// file read afterwards would undo what somebody had just asked for.
    #[test]
    fn what_the_person_picked_outranks_what_the_settings_file_says() {
        let said = Said {
            picked: Some("ja"),
            written: Some("fr"),
        };
        assert_eq!(requested(environment(&[]), said).as_deref(), Some("ja"));
    }

    /// `BRAVEBOT_LOCALE` is the one name a launch can give for itself, and the tests that run a
    /// binary pin their catalog with it whatever the home they were handed holds.
    #[test]
    fn the_variable_made_for_this_program_outranks_what_the_person_said() {
        let said = Said {
            picked: Some("fr"),
            written: Some("fr"),
        };
        let shell = [(LOCALE, "en-US")];
        assert_eq!(
            requested(environment(&shell), said).as_deref(),
            Some("en-US")
        );
    }

    /// A blank asks for nothing, and `auto` is the word for handing the choice on, so neither may
    /// stop the next source being read: a `language` of `auto` that answered as a language called
    /// auto would show English to somebody whose shell says French.
    #[test]
    fn a_blank_or_auto_choice_leaves_the_question_to_the_next_source() {
        let shell = [("LANG", "fr_FR")];
        for picked in [Some(""), Some("  "), Some("auto"), Some(" AUTO ")] {
            let said = Said {
                picked,
                written: Some("auto"),
            };
            assert_eq!(
                requested(environment(&shell), said).as_deref(),
                Some("fr_FR"),
                "{picked:?}"
            );
        }
        let said = Said {
            picked: Some("auto"),
            written: Some("ja"),
        };
        assert_eq!(requested(environment(&shell), said).as_deref(), Some("ja"));
    }

    /// What a window is told it is read in follows the same order the terminal starts by, and ends
    /// at a catalog rather than a request: a word nobody translated is the reference.
    #[test]
    fn the_catalog_chosen_for_a_window_follows_the_order_the_terminal_starts_by() {
        let shell = [("LANG", "fr_FR.UTF-8")];
        assert_eq!(chosen(environment(&shell), Said::default()).tag(), "fr");
        let said = Said {
            picked: Some("en"),
            written: Some("fr"),
        };
        assert_eq!(chosen(environment(&shell), said).tag(), "en-US");
        let shell = [(LOCALE, "fr"), ("LANG", "en_US")];
        assert_eq!(chosen(environment(&shell), said).tag(), "fr");
        let said = Said {
            picked: Some("auto"),
            written: None,
        };
        assert_eq!(chosen(environment(&[("LANG", "zz_ZZ")]), said), DEFAULT);
        assert_eq!(chosen(environment(&[("LANG", "C")]), said), DEFAULT);
    }

    /// A window started with no shell reads the system's languages in their order, after everything
    /// the terminal reads, and still only chooses among the catalogs that shipped.
    #[test]
    fn a_window_with_nothing_named_takes_the_first_system_language_a_catalog_answers() {
        let system = |tags: &[&str]| tags.iter().map(|tag| tag.to_string()).collect::<Vec<_>>();
        let none = Said::default();
        assert_eq!(
            window_chosen(environment(&[]), none, &system(&["fr-CA"])).tag(),
            "fr"
        );
        let preferred = system(&["de-DE", "fr-FR", "en-US"]);
        assert_eq!(
            window_chosen(environment(&[]), none, &preferred).tag(),
            "fr"
        );
        assert_eq!(
            window_chosen(environment(&[]), none, &system(&["de", "ja"])),
            DEFAULT
        );
        assert_eq!(window_chosen(environment(&[]), none, &[]), DEFAULT);
        let shell = [("LANG", "en_US.UTF-8")];
        assert_eq!(
            window_chosen(environment(&shell), none, &system(&["fr"])),
            DEFAULT
        );
        let said = Said {
            picked: Some("auto"),
            written: Some("en"),
        };
        assert_eq!(
            window_chosen(environment(&[]), said, &system(&["fr"])),
            DEFAULT
        );
        assert_eq!(chosen(environment(&[]), none), DEFAULT);
    }

    /// The picker labels each catalog by what it calls itself, which only that catalog says.
    #[test]
    fn every_catalog_names_itself() {
        assert_eq!(DEFAULT.name(), "English (US)");
        assert_eq!(find("fr").map(Locale::name), Some("Français"));
    }

    /// `/language` has to tell a language nobody translated from one somebody did, which
    /// [`resolve`] cannot: it answers the reference for both.
    #[test]
    fn a_language_that_did_not_ship_is_found_by_nothing_though_it_resolves_to_the_reference() {
        assert_eq!(find("zz-ZZ"), None);
        assert_eq!(resolve("zz-ZZ"), DEFAULT);
        assert_eq!(find("fr").map(Locale::tag), Some("fr"));
        assert_eq!(find("en").map(Locale::tag), Some("en-US"));
    }

    /// The whole point of naming a catalog after a locale.
    #[test]
    fn a_request_takes_the_catalog_of_that_exact_name() {
        assert_eq!(best("fr-FR", &SHIPPED), Some(2));
    }

    /// A French speaker in Belgium is better served by any French than by English.
    #[test]
    fn a_request_with_no_catalog_of_its_own_takes_one_in_its_language() {
        assert_eq!(best("fr-BE", &SHIPPED), Some(1));
    }

    /// Nothing here says which region, so the first catalog in the language answers.
    #[test]
    fn a_bare_language_takes_a_catalog_in_that_language() {
        assert_eq!(best("fr", &SHIPPED), Some(1));
    }

    /// A locale with no catalog at all is what the fallback exists for.
    #[test]
    fn a_language_that_did_not_ship_falls_back() {
        assert_eq!(best("de-DE", &SHIPPED), None);
        assert_eq!(resolve("de-DE"), DEFAULT);
    }

    /// What a shell actually exports, which is neither a bare tag nor spelled with a hyphen.
    #[test]
    fn the_encoding_and_modifier_a_shell_appends_are_not_part_of_the_tag() {
        assert_eq!(best("fr_FR.UTF-8", &SHIPPED), Some(2));
        assert_eq!(best("fr_FR.UTF-8@euro", &SHIPPED), Some(2));
    }

    /// A modifier can come straight after the territory with no encoding before it. Left on, it
    /// spoils the exact match and the request widens to whichever catalog of the language comes
    /// first, which for `fr_FR@euro` is the Canadian one.
    #[test]
    fn a_modifier_with_no_encoding_before_it_is_not_part_of_the_tag() {
        assert_eq!(best("fr_FR@euro", &SHIPPED), Some(2));
    }

    /// `C` is POSIX for "do not localise anything", not a request for a language called C.
    #[test]
    fn the_posix_locale_asks_for_no_catalog() {
        assert_eq!(best("C", &SHIPPED), None);
        assert_eq!(best("POSIX", &SHIPPED), None);
        assert_eq!(best("", &SHIPPED), None);
    }

    /// No shipped catalog's language is `C` or `POSIX`, so against the real set a request for
    /// either finds nothing whether or not it is read as a language. A set holding such tags is
    /// the one where the two readings differ.
    #[test]
    fn the_posix_locale_is_not_read_as_a_language_even_where_a_tag_spells_it() {
        let named = ["c", "posix", "en-US"];
        assert_eq!(best("C", &named), None);
        assert_eq!(best("C.UTF-8", &named), None);
        assert_eq!(best("POSIX", &named), None);
    }

    /// Counts come from lengths, from indices, and from parsed numbers, and no call site should
    /// have to cast one on the way to a message.
    #[test]
    fn a_count_is_made_from_whatever_width_the_caller_holds() {
        let lines: Vec<u8> = vec![1, 2, 3];
        assert_eq!(t!(count_turns, count = lines.len()), "3 turns");
        assert_eq!(t!(count_turns, count = 1_u32), "1 turn");
        assert_eq!(t!(count_turns, count = -4_i64), "-4 turns");
    }

    /// A count too large to be a count is still not worth aborting the interface over.
    #[test]
    fn a_count_past_what_a_message_could_read_saturates_rather_than_panicking() {
        assert_eq!(Count::from(u64::MAX), Count::from(i64::MAX));
    }

    /// The reason the catalog has selects at all.
    #[test]
    fn a_plural_select_picks_the_variant_the_language_calls_for() {
        assert_eq!(t!(count_turns, count = 0), "0 turns");
        assert_eq!(t!(count_turns, count = 1), "1 turn");
        assert_eq!(t!(count_turns, count = 2), "2 turns");
    }

    /// Every language whose plural rule is written down has to be one a catalog may be named for.
    #[test]
    fn a_language_with_a_plural_rule_is_the_only_kind_a_select_may_ship_in() {
        assert!(plural::is_known("en"));
        assert!(!plural::is_known("qq"));
        assert_eq!(plural::category("en", 1), "one");
        assert_eq!(plural::category("en", 0), "other");
        assert_eq!(plural::category("fr", 0), "one");
        assert_eq!(plural::category("ja", 1), "other");
    }

    /// French and English both give `other` above one. `pt` has no rule because `pt-BR` and
    /// `pt-PT` disagree about zero. A language with no rule gives `other` for every count,
    /// including 1.
    #[test]
    fn french_english_and_unknown_languages_get_the_categories_their_rules_give() {
        assert_eq!(plural::category("fr", 1), "one");
        assert_eq!(plural::category("fr", 2), "other");
        assert_eq!(plural::category("en", 2), "other");
        assert!(plural::is_known("ja"));
        assert!(!plural::is_known("pt"));
        assert!(!plural::is_known("pt-BR"));
        for count in [0, 1, 2] {
            assert_eq!(plural::category("qq", count), "other", "{count}");
        }
    }

    /// `auto` in any case gives the choice back, a word a catalog answers is that catalog under its
    /// own tag, and anything else is refused rather than read as the reference.
    #[test]
    fn a_language_picked_by_name_is_auto_a_catalog_or_refused() {
        for word in ["auto", " AUTO ", "Auto"] {
            assert_eq!(pick(word), Ok(None), "{word:?}");
        }
        assert_eq!(pick(" fr_FR ").map(|l| l.map(|l| l.tag())), Ok(Some("fr")));
        assert_eq!(pick("en").map(|l| l.map(|l| l.tag())), Ok(Some("en-US")));
        for word in ["", "  ", "zz", "C", "automatic"] {
            assert_eq!(pick(word), Err(NoCatalog), "{word:?}");
        }
    }
}
