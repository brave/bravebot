//! The size of the tool list a `--safe` one-shot turn sends with every request (#4712).
//!
//! Every request repeats the whole list, so its length is a fixed cost of each request. The list is
//! built as `turn/setup.rs` and `turn.rs` build it for `bravebot --safe -p`: nothing asks the turn
//! again, no watch can be armed, no link is kept, and the built-in delegate kinds.

use bravebot_agent::exec::Deadlines;
use bravebot_agent::tools::{self, Running, Scheduling};
use bravebot_agent::watch::Arming;
use bravebot_core::delegate::Definitions;

/// The most bytes of JSON the list may take, so the list cannot grow back. #4712's first
/// shortening left it at 31,020 bytes and shortening `run`'s confinement statement at 30,718.
/// Sentences that spec clauses require (the asked-again sentence of READ-6 and RUN-18, the example
/// of SPAWN-6, ASK-12's plain-words sentences, the cache cost of a longer `job_output` wait) and
/// the `NAME=value` sentence of SANDBOX-26 brought it to 32,896, and the sentence of `run` on
/// quoting a pattern to 32,983. #4712's target is under 22,000
/// bytes, which this list does not yet meet.
const MOST_BYTES: usize = 33_000;

fn one_shot_list() -> Vec<bravebot_aichat::protocol::Tool> {
    let mut offered = tools::for_planner(
        Scheduling::NoLaterLook,
        Arming::Unavailable,
        &Definitions::default(),
        Deadlines::BUILT_IN,
        Running::Offered,
    );
    tools::offer_remember(&mut offered);
    tools::offer_propose_rule(&mut offered);
    tools::offer_recall(&mut offered);
    tools::state_confinement(
        &mut offered,
        true,
        bravebot_sandbox::SandboxMode::WorkspaceWrite,
        true,
        true,
    );
    tools::state_withheld_variables(&mut offered);
    tools::state_what_is_beside(&mut offered);
    offered
}

/// Prints each tool's size and the total, and fails where the total is over [`MOST_BYTES`].
///
/// Rejects: a description grown back past the budget, which costs every request of every turn.
#[test]
#[expect(clippy::expect_used, reason = "a test fails by panicking")]
fn the_one_shot_tool_list_fits_its_budget() {
    let offered = one_shot_list();
    assert_eq!(
        offered.len(),
        25,
        "the one-shot list is not the 25 tools it was measured on"
    );
    let mut sizes: Vec<(String, usize, usize, usize)> = offered
        .iter()
        .map(|tool| {
            let whole = serde_json::to_string(tool)
                .expect("a tool serializes")
                .len();
            let description = serde_json::to_string(&tool.function.description)
                .expect("a description serializes")
                .len();
            let parameters = serde_json::to_string(&tool.function.parameters)
                .expect("parameters serialize")
                .len();
            (tool.function.name.clone(), whole, description, parameters)
        })
        .collect();
    sizes.sort_by_key(|size| std::cmp::Reverse(size.1));
    for (name, whole, description, parameters) in &sizes {
        println!("{name} {whole}/{description}/{parameters}");
    }
    let total = serde_json::to_string(&offered)
        .expect("the list serializes")
        .len();
    println!("total {total}");
    if let Some(dump) = std::env::var_os("BRAVEBOT_TOOL_PREFIX_DUMP") {
        std::fs::write(
            dump,
            serde_json::to_string_pretty(&offered).expect("the list serializes"),
        )
        .expect("write the dump");
    }
    assert!(
        total < MOST_BYTES,
        "the one-shot tool list is {total} bytes, over {MOST_BYTES}"
    );
}
