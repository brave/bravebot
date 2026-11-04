//! The sentences a quarantined tool result carries beside its reference.
//!
//! Each is built from what the driver already holds: the tool's name, whether its output was cut
//! short, how a command was run, and the size of what it printed. None reads the content that was
//! quarantined, which is why they can live apart from the presentation of it in `one_turn`.

use super::{ReadOutputIs, STOP_BEING_ASKED};
use crate::tools::{Beside, Named};
use crate::workspace::Paging;

/// What a result says about a cap, written beside the reference because the reference describes
/// shape and provenance, and a cap is neither: a search that stopped short would otherwise reach
/// the planner looking exactly like one that found everything there was.
pub(super) fn capped_note(tool: &str, incomplete: bool, paging: Option<Paging>) -> String {
    if incomplete {
        // Where the cap can be asked past, the offset is the advice:
        // narrowing is a guess, and a guess that misses loses the part
        // that was cut off.
        let rest = match paging {
            Some(Paging::Continue(offset)) => {
                format!("Ask again with offset {offset} for the rest.")
            }
            _ => "Narrow it, or work through a subdirectory to cover the rest.".to_string(),
        };
        format!(
            "\n\nThe {tool} stopped at a cap, so this result is incomplete: it \
             is a sample and not the whole answer. {rest}"
        )
    } else if let Some(Paging::PastTheEnd { found }) = paging {
        // The other end of the same contract. A page past the last match is
        // empty, and an empty result nobody explains reads as the pattern
        // having gone from the tree: the count is what says otherwise, and
        // it is written into a body the planner may not read.
        format!(
            "\n\nThat offset is past the last match. The {tool} found {} in \
             all, and the earlier ones are still there.",
            crate::tools::tally(found, "match", "matches")
        )
    } else {
        String::new()
    }
}

/// What a result says about the symbolic links its walk did not enter, written beside the reference
/// for the reason `capped_note` is: the body that would carry it reaches nobody. A number and never a
/// name, from the count the walk kept while it read file types.
pub(super) fn links_note(tool: &str, links: usize) -> String {
    if links == 0 {
        return String::new();
    }
    match tool {
        // The entries of a listing follow on the same paragraph.
        "list_files" => format!(
            " {} not entered and {} not among them.",
            crate::tools::tally(links, "symbolic link was", "symbolic links were"),
            if links == 1 { "is" } else { "are" }
        ),
        _ => format!("\n\n{}.", crate::tools::search_links_clause(links)),
    }
}

/// What a result says about the parts of a server's answer its transport left out, beside the
/// reference for the reason `capped_note` is. A number and never a word of what was left out.
pub(super) fn left_out_note(left_out: usize) -> String {
    if left_out == 0 {
        return String::new();
    }
    format!(
        "\n\n{} left out of what the server returned: only its text and a picture of a type and a \
         size this client takes are kept.",
        crate::tools::tally(left_out, "part was", "parts were")
    )
}

/// How to see this one and how to stop being asked, for a run and
/// only for a run.
/// Without it the quarantine reads as a fact about running programs,
/// and a planner told once that a command it ran cannot be shown to it
/// stops running commands: it spent the rest of a session reading files
/// one at a time through read_file, having concluded that the shell was
/// a dead end. It is not one. The label is about who answered for the
/// command, and a person can answer for it.
///
/// From `printed_by` rather than the tool's name, which is the same
/// condition the provenance above is recorded under: a result carries a
/// command when a command produced it.
///
/// The half about vouching is left out where a record already stops the
/// asking for this exact line: no prompt will return there for anybody
/// to answer, so advice about what to press at one is advice about
/// something that will not happen, and `read_output` is then the whole
/// of what can be said.
///
/// Neither half holds where what a run printed is read with nobody
/// asked: nobody is shown what read_output releases, and a run
/// approved by the mode vouches for nothing. What holds there is that
/// read_output is a yes, and it is said without naming the mode: only
/// plan mode tells the planner which one it is in, since a model told
/// nobody is watching has been handed a reason to be bolder.
///
/// Screening asked for is neither of those, in any mode. A check's
/// word answers whenever it is safe, so the user is not shown that
/// output and a vouch stops no asking, and a word that objects keeps
/// the output back or puts it to the person, so read_output is not a
/// yes either. It is said to come back or be kept back, and nothing
/// more about what decides which (CHECK-9), in the same words in
/// every mode. A `read` asked for there is answered in the result
/// that ran it when the check finds nothing. When it does not, the
/// result says the output was not returned and where to go instead,
/// so the call does not read as a quarantined result with no
/// explanation, and without saying why.
///
/// `read` is an argument of run alone, so it is offered on a run's
/// result and not on a job's. It is not offered again where it was
/// asked for: the output was then too long for one result, which is
/// said, or its release was refused, which read_output reports.
///
/// Too long is said with where the rest is. Told only that, a planner
/// took read_output's result to be held to the same size and ran a
/// four-minute make check again through grep, when the whole log was
/// in the slot and a filter fed it through stdin_ref reads it there.
///
/// A turn whose list lacks `read_output` has nothing to be pointed at, and none of the sentences
/// that name it, or that lean on a release it makes, is said to it (TOOL-6). What is left is the
/// statement that the quarantine is about the command, the vouching sentence where it applies,
/// which names no tool, `spawn_processor` as the way to work with the output where the list holds
/// it, and `read_file` where the list holds it. The same holds for `read_file`:
/// its sentence is said only to a list that holds it.
///
/// `printed_by` is whether a command produced the result, `bytes` is the size of what it printed
/// where that is known, `cap` is the largest result a run may hold, and `beside` is which of the
/// tools named here the turn's list holds.
#[expect(
    clippy::too_many_arguments,
    reason = "the facts of the result, the mode's answer and the list's tools are separate inputs"
)]
pub(super) fn run_advice(
    printed_by: bool,
    covered_by_record: bool,
    read_output_is: ReadOutputIs,
    beside: Beside,
    tool: &str,
    read_asked: bool,
    bytes: Option<usize>,
    cap: usize,
) -> String {
    let read_file = if beside.holds(Named::ReadFile) {
        " To read a file, use read_file."
    } else {
        ""
    };
    match (printed_by, covered_by_record, read_output_is) {
        (false, _, _) => String::new(),
        // Whatever the mode says, a list without `read_output` has no release for a `read` to make
        // early and nothing to call for a look at the output.
        (true, _, _) if !beside.holds(Named::ReadOutput) => {
            let vouching = if covered_by_record {
                String::new()
            } else {
                format!(" {STOP_BEING_ASKED}")
            };
            let processor = if beside.holds(Named::SpawnProcessor) {
                " To work with what it printed, give the reference to spawn_processor."
            } else {
                ""
            };
            format!(
                "\n\nThis is about the command rather than about what it printed.{vouching}{processor}{read_file}"
            )
        }
        (true, _, ReadOutputIs::Unasked) => {
            let in_the_result = match (tool == "run", read_asked) {
                (false, _) => "",
                (true, false) => {
                    " To have what a command prints come back in \
                         the result that ran it, call run with read: \
                         true."
                }
                (true, true) if bytes.is_none_or(|bytes| bytes > cap) => {
                    " It is longer than a run's result may hold, so \
                         it was left out of this one. read_output is not \
                         held to that size and hands back all of it. To \
                         see part of it without running the command \
                         again, call run with a filter such as tail -n \
                         100 or grep, stdin_ref set to the reference, \
                         and read: true."
                }
                (true, true) => "",
            };
            format!(
                "\n\nThis is about the command rather than about what \
                 it printed, and it is not the end of the road. To see \
                 this one, call read_output with the \
                 reference.{in_the_result}{read_file}"
            )
        }
        (true, _, ReadOutputIs::ByACheck) => {
            let not_returned = match (tool == "run" && read_asked, bytes) {
                (false, _) => "",
                (true, Some(bytes)) if bytes <= cap => {
                    " Your read: true did not return it in this \
                     result, so read_output is the way to see it."
                }
                (true, _) => {
                    " It is longer than a run's result may hold, so \
                     it was left out of this one. read_output is not \
                     held to that size and is the way to see all of \
                     it. To see part of it without running the \
                     command again, call run with a filter such as \
                     tail -n 100 or grep, stdin_ref set to the \
                     reference, and read: true."
                }
            };
            format!(
                "\n\nThis is about the command rather than about \
                 what it printed, and it is not the end of the road. \
                 To see this one, call read_output with the \
                 reference.{not_returned}{read_file}"
            )
        }
        (true, true, ReadOutputIs::Asked) => format!(
            "\n\nThis is about the command rather \
             than about what it printed, and it is not the end of the \
             road. To see this one, call read_output with the \
             reference.{read_file}"
        ),
        (true, false, ReadOutputIs::Asked) => format!(
            "\n\nThis is about the command rather \
             than about what it printed, and it is not the end of the \
             road. To see this one, call read_output with the \
             reference. {STOP_BEING_ASKED}{read_file}"
        ),
    }
}

/// What a result says when `read: true` was asked of `fetch_url` or a server's tool and the
/// content still came back as a reference.
///
/// Where a person would be asked at `vet_content` anyway, `read` has nothing to answer and nothing
/// is said. Elsewhere the planner asked for the content in this result, so the result says it was
/// not returned and where to go instead, rather than reading as a quarantined result with no
/// explanation. A slot of 0 bytes is not named here: the trail says no check was made, and
/// there is nothing to see. `bytes` is the size the slot states, and `cap` is the largest result
/// a read may hold.
pub(super) fn read_not_returned(
    read_asked: bool,
    vet_content_is: ReadOutputIs,
    bytes: Option<usize>,
    cap: usize,
) -> &'static str {
    match (read_asked, vet_content_is, bytes) {
        (false, _, _) | (_, ReadOutputIs::Asked, _) | (_, _, Some(0)) => "",
        (true, _, Some(bytes)) if bytes <= cap => {
            "\n\nYour read: true did not return it in this result, so vet_content is the way to \
             see it."
        }
        (true, _, _) => {
            "\n\nIt is longer than a result may hold, so it was left out of this one. \
             vet_content is the way to see it."
        }
    }
}

/// Where git ran in a directory whose repository `read_git` opens: the output of git log or show
/// is what `read_git` answers with nobody asked, and for any other program, or a repository it
/// declines, the sentence is noise. `ran_git` is decided by the caller and not when the line ran,
/// so a line whose output is shown pays nothing.
///
/// Said only to a turn whose list holds `read_git`: a list without it has nothing to be pointed at
/// and the git output it was given is what there is (TOOL-6).
pub(super) fn history_note(ran_git: bool, beside: Beside) -> &'static str {
    if ran_git && beside.holds(Named::ReadGit) {
        " To read a repository's history, use read_git: it reads \
         .git without starting git, and in a trusted repository \
         nobody is asked."
    } else {
        ""
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `run_advice` for a list that holds every tool it names, which is what the text was written
    /// for and what the pins below read.
    fn advice(
        printed_by: bool,
        covered_by_record: bool,
        read_output_is: ReadOutputIs,
        tool: &str,
        read_asked: bool,
        bytes: Option<usize>,
        cap: usize,
    ) -> String {
        run_advice(
            printed_by,
            covered_by_record,
            read_output_is,
            Beside::every(),
            tool,
            read_asked,
            bytes,
            cap,
        )
    }
    const ABOUT: &str = "\n\nThis is about the command rather than about what it printed, and it is not the end of the road. To see this one, call read_output with the reference.";

    #[test]
    fn a_cut_short_result_with_an_offset_names_the_offset() {
        assert_eq!(
            capped_note("search", true, Some(Paging::Continue(40))),
            "\n\nThe search stopped at a cap, so this result is incomplete: it is a sample and not the whole answer. Ask again with offset 40 for the rest."
        );
    }

    #[test]
    fn a_cut_short_result_without_an_offset_says_to_narrow() {
        assert_eq!(
            capped_note("list_dir", true, None),
            "\n\nThe list_dir stopped at a cap, so this result is incomplete: it is a sample and not the whole answer. Narrow it, or work through a subdirectory to cover the rest."
        );
    }

    #[test]
    fn a_page_past_the_end_says_how_many_there_were() {
        assert_eq!(
            capped_note("search", false, Some(Paging::PastTheEnd { found: 1 })),
            "\n\nThat offset is past the last match. The search found 1 match in all, and the earlier ones are still there."
        );
        assert!(
            capped_note("search", false, Some(Paging::PastTheEnd { found: 3 }))
                .contains("found 3 matches in all")
        );
    }

    #[test]
    fn a_complete_result_has_no_cap_note() {
        assert_eq!(capped_note("search", false, None), "");
        // A cut-off wins over a stale past-the-end paging.
        assert!(
            capped_note("search", true, Some(Paging::PastTheEnd { found: 3 }))
                .contains("stopped at a cap")
        );
    }

    /// The regression it rejects: the link count dropped for a result the planner may not read,
    /// or the note naming something other than a count.
    #[test]
    fn links_not_entered_are_counted_beside_the_reference_and_no_note_means_none() {
        assert_eq!(links_note("search", 0), "");
        assert_eq!(links_note("list_files", 0), "");
        assert_eq!(
            links_note("list_files", 1),
            " 1 symbolic link was not entered and is not among them."
        );
        assert_eq!(
            links_note("search", 2),
            "\n\n2 symbolic links were not entered, so files behind them were not searched."
        );
    }

    #[test]
    fn a_result_no_command_printed_has_no_advice() {
        for is in [
            ReadOutputIs::Unasked,
            ReadOutputIs::ByACheck,
            ReadOutputIs::Asked,
        ] {
            assert_eq!(advice(false, false, is, "run", true, Some(1), 10), "");
            assert_eq!(advice(false, true, is, "read_file", false, None, 10), "");
        }
    }

    #[test]
    fn an_unasked_run_is_offered_the_read_flag_until_it_was_used() {
        let told = advice(
            true,
            false,
            ReadOutputIs::Unasked,
            "run",
            false,
            Some(1),
            10,
        );
        assert_eq!(
            told,
            format!(
                "{ABOUT} To have what a command prints come back in the result that ran it, call run with read: true. To read a file, use read_file."
            )
        );
        // Not a run: nothing about `read`.
        assert_eq!(
            advice(
                true,
                false,
                ReadOutputIs::Unasked,
                "job_output",
                false,
                Some(1),
                10
            ),
            format!("{ABOUT} To read a file, use read_file.")
        );
    }

    #[test]
    fn an_unasked_run_read_but_too_long_says_where_the_rest_is() {
        let long = format!(
            "{ABOUT} It is longer than a run's result may hold, so it was left out of this one. read_output is not held to that size and hands back all of it. To see part of it without running the command again, call run with a filter such as tail -n 100 or grep, stdin_ref set to the reference, and read: true. To read a file, use read_file."
        );
        assert_eq!(
            advice(
                true,
                false,
                ReadOutputIs::Unasked,
                "run",
                true,
                Some(11),
                10
            ),
            long
        );
        // An unknown size counts as too long.
        assert_eq!(
            advice(true, false, ReadOutputIs::Unasked, "run", true, None, 10),
            long
        );
        // At the cap it fit, so there is nothing to add.
        assert_eq!(
            advice(
                true,
                false,
                ReadOutputIs::Unasked,
                "run",
                true,
                Some(10),
                10
            ),
            format!("{ABOUT} To read a file, use read_file.")
        );
    }

    #[test]
    fn a_check_answering_says_the_output_was_not_returned_only_for_a_run_that_asked() {
        assert_eq!(
            advice(
                true,
                false,
                ReadOutputIs::ByACheck,
                "run",
                true,
                Some(1),
                10
            ),
            format!(
                "{ABOUT} Your read: true did not return it in this result, so read_output is the way to see it. To read a file, use read_file."
            )
        );
        for (tool, asked) in [("run", false), ("job_output", true)] {
            assert_eq!(
                advice(
                    true,
                    false,
                    ReadOutputIs::ByACheck,
                    tool,
                    asked,
                    Some(1),
                    10
                ),
                format!("{ABOUT} To read a file, use read_file.")
            );
        }
    }

    #[test]
    fn a_check_answering_says_where_the_rest_is_for_output_past_the_cap() {
        let long = format!(
            "{ABOUT} It is longer than a run's result may hold, so it was left out of this one. read_output is not held to that size and is the way to see all of it. To see part of it without running the command again, call run with a filter such as tail -n 100 or grep, stdin_ref set to the reference, and read: true. To read a file, use read_file."
        );
        for bytes in [Some(11), None] {
            assert_eq!(
                advice(true, false, ReadOutputIs::ByACheck, "run", true, bytes, 10),
                long
            );
        }
    }

    #[test]
    fn a_person_asked_hears_about_vouching_only_where_no_record_stops_the_asking() {
        assert_eq!(
            advice(true, true, ReadOutputIs::Asked, "run", false, None, 10),
            format!("{ABOUT} To read a file, use read_file.")
        );
        assert_eq!(
            advice(true, false, ReadOutputIs::Asked, "run", false, None, 10),
            format!(
                "{ABOUT} To stop being asked, a person vouching for every stage of the exact command makes what it prints visible from then on. To read a file, use read_file."
            )
        );
    }

    const NOT_RETURNED: &str = "\n\nYour read: true did not return it in this result, so vet_content is the way to see it.";
    const TOO_LONG: &str = "\n\nIt is longer than a result may hold, so it was left out of this one. vet_content is the way to see it.";

    /// The regression it rejects: a sentence about a flag the planner did not pass, or about a
    /// content that was handed over, either of which tells the planner something false; and
    /// naming the flag where a person is asked at `vet_content` anyway, which the flag does not
    /// change.
    #[test]
    fn nothing_is_said_of_a_read_that_was_not_asked_or_has_nothing_to_answer() {
        for gate in [ReadOutputIs::Unasked, ReadOutputIs::ByACheck] {
            assert_eq!(read_not_returned(false, gate, Some(5), 10), "");
            assert_eq!(read_not_returned(false, gate, None, 10), "");
        }
        for bytes in [Some(0), Some(5), Some(11), None] {
            assert_eq!(
                read_not_returned(true, ReadOutputIs::Asked, bytes, 10),
                "",
                "a person is asked anyway, so the flag has nothing to answer ({bytes:?})"
            );
        }
        assert_eq!(
            read_not_returned(true, ReadOutputIs::ByACheck, Some(0), 10),
            ""
        );
        assert_eq!(
            read_not_returned(true, ReadOutputIs::Unasked, Some(0), 10),
            ""
        );
    }

    /// The regression it rejects: the size sentence used where the content fitted (the planner
    /// is then told to shorten a result that was not too long), or the cap compared with `<`
    /// so that a result of exactly the cap is called too long.
    #[test]
    fn a_read_that_fit_and_was_not_returned_says_so_and_names_vet_content() {
        for gate in [ReadOutputIs::Unasked, ReadOutputIs::ByACheck] {
            assert_eq!(read_not_returned(true, gate, Some(1), 10), NOT_RETURNED);
            assert_eq!(read_not_returned(true, gate, Some(10), 10), NOT_RETURNED);
        }
    }

    /// The regression it rejects: a result past the cap, or one whose size is not known, being
    /// told it was merely not returned, which sends the planner to ask again for the same thing.
    #[test]
    fn a_read_past_the_cap_or_of_unknown_size_says_it_is_too_long_for_a_result() {
        for gate in [ReadOutputIs::Unasked, ReadOutputIs::ByACheck] {
            assert_eq!(read_not_returned(true, gate, Some(11), 10), TOO_LONG);
            assert_eq!(read_not_returned(true, gate, None, 10), TOO_LONG);
        }
    }

    #[test]
    fn the_history_sentence_appears_only_where_git_ran() {
        assert_eq!(history_note(false, Beside::every()), "");
        assert_eq!(
            history_note(true, Beside::every()),
            " To read a repository's history, use read_git: it reads .git without starting git, and in a trusted repository nobody is asked."
        );
    }

    /// A list that holds every tool except the named ones.
    fn without(names: &[&str]) -> Beside {
        [
            ("run", Named::Run),
            ("read_output", Named::ReadOutput),
            ("read_file", Named::ReadFile),
            ("read_git", Named::ReadGit),
            ("spawn_processor", Named::SpawnProcessor),
        ]
        .into_iter()
        .filter(|(name, _)| names.contains(name))
        .fold(Beside::every(), |beside, (_, named)| beside.without(named))
    }

    /// R31: a list that holds every tool reads what it always read, for every mode, record state,
    /// tool and read-ask. The pins above are for the full list too, so this ties the two entry
    /// points together: the full list is the helper's own.
    #[test]
    fn a_list_holding_every_tool_reads_the_text_it_always_did() {
        for is in [
            ReadOutputIs::Unasked,
            ReadOutputIs::ByACheck,
            ReadOutputIs::Asked,
        ] {
            for (record, tool, asked, bytes) in [
                (false, "run", false, Some(1)),
                (true, "run", true, Some(11)),
                (false, "job_output", false, None),
            ] {
                let text = run_advice(true, record, is, Beside::every(), tool, asked, bytes, 10);
                assert!(text.starts_with(ABOUT), "{is:?}: {text}");
                assert!(text.ends_with(" To read a file, use read_file."), "{text}");
            }
        }
    }

    /// The fault: the advice names `read_output` whatever the list holds, so a turn left without it
    /// is sent to a tool it was not offered. Every mode, record state, tool and read-ask is
    /// checked, and the text still says whose answer the quarantine is about.
    #[test]
    fn a_list_without_read_output_is_not_pointed_at_it_in_any_arm() {
        for is in [
            ReadOutputIs::Unasked,
            ReadOutputIs::ByACheck,
            ReadOutputIs::Asked,
        ] {
            for (record, tool, asked, bytes) in [
                (false, "run", false, Some(1)),
                (false, "run", true, Some(1)),
                (false, "run", true, Some(11)),
                (true, "run", true, None),
                (false, "job_output", false, None),
            ] {
                let text = run_advice(
                    true,
                    record,
                    is,
                    without(&["read_output"]),
                    tool,
                    asked,
                    bytes,
                    10,
                );
                assert!(!text.contains("read_output"), "{is:?}: {text}");
                assert!(!text.contains("filter"), "{is:?}: {text}");
                assert!(!text.contains("read: true"), "{is:?}: {text}");
                assert!(
                    text.starts_with(
                        "\n\nThis is about the command rather than about what it printed."
                    ),
                    "{is:?}: {text}"
                );
            }
        }
    }

    /// Without `read_output` the advice keeps `read_file` and the vouching sentence, which names no
    /// tool, and drops the vouching sentence where a record already stops the asking.
    #[test]
    fn a_list_without_read_output_keeps_what_names_no_missing_tool() {
        let beside = without(&["read_output", "spawn_processor"]);
        assert_eq!(
            run_advice(
                true,
                false,
                ReadOutputIs::Asked,
                beside,
                "run",
                false,
                None,
                10
            ),
            "\n\nThis is about the command rather than about what it printed. To stop being asked, a person vouching for every stage of the exact command makes what it prints visible from then on. To read a file, use read_file."
        );
        assert_eq!(
            run_advice(
                true,
                true,
                ReadOutputIs::Asked,
                beside,
                "run",
                false,
                None,
                10
            ),
            "\n\nThis is about the command rather than about what it printed. To read a file, use read_file."
        );
    }

    /// The fault: a list without `read_output` is left with no way forward even where it holds
    /// `spawn_processor`, or is pointed at `spawn_processor` where it lacks it.
    #[test]
    fn a_list_without_read_output_is_pointed_at_spawn_processor_only_where_it_holds_it() {
        let with = run_advice(
            true,
            true,
            ReadOutputIs::Asked,
            without(&["read_output", "read_file"]),
            "run",
            false,
            None,
            10,
        );
        assert_eq!(
            with,
            "\n\nThis is about the command rather than about what it printed. To work with what it printed, give the reference to spawn_processor."
        );
        let without_it = run_advice(
            true,
            true,
            ReadOutputIs::Asked,
            without(&["read_output", "read_file", "spawn_processor"]),
            "run",
            false,
            None,
            10,
        );
        assert!(!without_it.contains("spawn_processor"), "{without_it}");
    }

    /// The fault: `read_file` is named whatever the list holds.
    #[test]
    fn a_list_without_read_file_is_not_pointed_at_it() {
        let beside = without(&["read_file"]);
        for is in [
            ReadOutputIs::Unasked,
            ReadOutputIs::ByACheck,
            ReadOutputIs::Asked,
        ] {
            let text = run_advice(true, true, is, beside, "run", false, Some(1), 10);
            assert!(!text.contains("read_file"), "{is:?}: {text}");
            assert!(
                text.contains("call read_output with the reference"),
                "{text}"
            );
        }
        assert_eq!(
            run_advice(
                true,
                true,
                ReadOutputIs::Asked,
                beside,
                "run",
                false,
                None,
                10
            ),
            ABOUT
        );
        let neither = without(&["read_output", "read_file", "spawn_processor"]);
        assert_eq!(
            run_advice(
                true,
                true,
                ReadOutputIs::Asked,
                neither,
                "run",
                false,
                None,
                10
            ),
            "\n\nThis is about the command rather than about what it printed."
        );
    }

    /// The fault: the history sentence is said to a list that lacks `read_git`.
    #[test]
    fn the_history_sentence_is_left_out_where_the_list_lacks_read_git() {
        assert_eq!(history_note(true, without(&["read_git"])), "");
        assert_eq!(history_note(false, without(&["read_git"])), "");
    }

    /// A result no command printed has no advice whatever the list holds.
    #[test]
    fn a_result_no_command_printed_has_no_advice_on_a_short_list() {
        assert_eq!(
            run_advice(
                false,
                false,
                ReadOutputIs::Asked,
                without(&["read_output", "read_file"]),
                "read_file",
                false,
                None,
                10
            ),
            ""
        );
    }
}
