//! Which earlier excerpts a retrieving turn sends, chosen from the vectors kept beside the record.
//!
//! Pure: it reads the query, the rows and the record's text, and decides. Chunks are ranked by
//! their nearness in meaning to the prompt fused with how well they match its words, and the best
//! are taken until a share of the budget is spent ([RETRIEVE-18]). A reserve goes to later chunks
//! on the subject of the best hits, and an excerpt brings the prompt and the last reply of its
//! exchange ([RETRIEVE-19]). What is chosen comes back in the order it was said ([RETRIEVE-9]).
//!
//! [RETRIEVE-9]: ../../../docs/specs/retrieval.md#RETRIEVE-9
//! [RETRIEVE-18]: ../../../docs/specs/retrieval.md#RETRIEVE-18
//! [RETRIEVE-19]: ../../../docs/specs/retrieval.md#RETRIEVE-19

use crate::vectors::{Passage, Row, Speaker};
use bravebot_agent::recall::rank::{self, Doc};
use std::collections::{HashMap, HashSet};

/// The constant of reciprocal rank fusion, the usual one.
const RRF_K: f64 = 60.0;

/// The share of the best word match's score a chunk needs for its word match to count. Below it,
/// a chunk sharing a common word with a reworded question would outrank what it means.
const WORD_FLOOR: f64 = 0.8;

/// Percent of the budget held back for the prompt and last reply of each exchange an excerpt
/// came from. What they leave unspent goes to the next best hits.
const CONTEXT_SHARE: usize = 30;

/// Percent of what is left held back for later chunks on the subject of the best hits.
const FOLLOW_UP_RESERVE: usize = 15;

/// How many of the best hits later chunks are looked for after.
const FOLLOW_UP_TOP: usize = 5;

/// How many later chunks one hit may bring.
const FOLLOW_UP_PER_HIT: usize = 2;

/// A later chunk is on a hit's subject when it shares this many salient terms with it, or
/// [`FOLLOW_UP_NEARNESS`] of meaning.
const FOLLOW_UP_SHARED: usize = 1;
const FOLLOW_UP_NEARNESS: f32 = 0.6;

/// How much of a message brought for context is sent, in bytes.
pub const CONTEXT_BYTES: usize = 400;

/// What each excerpt costs beyond its text, for the lines that say what it is.
pub const FRAMING_TOKENS: usize = 20;

const BYTES_PER_TOKEN: usize = 4;

/// One chunk of the record as retrieval sees it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Chunk<'a> {
    /// Its message's place in the record.
    pub ordinal: u32,
    /// Its place among its message's chunks.
    pub chunk: u16,
    /// Who said it.
    pub speaker: Speaker,
    /// The place of the prompt that opened its exchange, when one did.
    pub exchange: Option<u32>,
    /// Its words, read from the record.
    pub text: &'a str,
    /// What it was embedded as.
    pub vector: &'a [f32],
}

/// One chunk chosen to be sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Excerpt {
    /// Its message's place in the record.
    pub ordinal: u32,
    /// Its place among its message's chunks.
    pub chunk: u16,
    /// Who said it.
    pub speaker: Speaker,
    /// Whether it is sent only as the prompt or last reply of a chosen excerpt's exchange, cut to
    /// [`CONTEXT_BYTES`].
    pub context: bool,
}

/// The chunks the rows name, with their words read from the record and the exchange each is in.
///
/// `rows` and `passages` are in record order, as [`crate::vectors::open`] and
/// [`crate::vectors::indexable`] give them. A row whose message is not among the passages, or
/// whose range is not in its message's text, is left out.
pub fn chunks_of<'a>(rows: &'a [Row], passages: &[Passage<'a>]) -> Vec<Chunk<'a>> {
    let mut prompt = None;
    let mut next = 0;
    rows.iter()
        .filter_map(|row| {
            while let Some(passage) = passages.get(next).filter(|p| p.ordinal <= row.ordinal) {
                if passage.speaker == Speaker::Person {
                    prompt = Some(passage.ordinal);
                }
                next += 1;
            }
            let passage = passages[..next]
                .last()
                .filter(|p| p.ordinal == row.ordinal)?;
            let text = passage
                .text
                .get(row.range.start as usize..row.range.end as usize)?;
            Some(Chunk {
                ordinal: row.ordinal,
                chunk: row.chunk,
                speaker: row.speaker,
                exchange: prompt,
                text,
                vector: &row.vector,
            })
        })
        .collect()
}

/// The excerpts to send for a prompt, in the order they were said.
///
/// `vector` is the prompt as embedded for a query and `text` its words. Only chunks of messages
/// before `cut`, the first message still sent whole, are chosen, and what is chosen costs at most
/// `budget` tokens, framing included. `chunks` are in record order.
pub fn retrieve(
    vector: &[f32],
    text: &str,
    chunks: &[Chunk],
    cut: u32,
    budget: usize,
) -> Vec<Excerpt> {
    let eligible: Vec<usize> = (0..chunks.len())
        .filter(|&i| chunks[i].ordinal < cut && chunks[i].vector.len() == vector.len())
        .collect();
    let order = ranking(vector, text, chunks, &eligible);
    let for_excerpts = budget * (100 - CONTEXT_SHARE) / 100;
    let first = for_excerpts * (100 - FOLLOW_UP_RESERVE) / 100;
    let mut picked = Vec::new();
    let mut chosen = vec![false; chunks.len()];
    let mut used = fill(&order, chunks, &mut picked, &mut chosen, 0, first);
    used = follow_ups(
        chunks,
        &eligible,
        &mut picked,
        &mut chosen,
        used,
        for_excerpts,
    );
    used = fill(&order, chunks, &mut picked, &mut chosen, used, for_excerpts);
    let context = exchange_context(chunks, &picked, cut, &mut used, budget);
    for &p in &context {
        chosen[p] = true;
    }
    fill(&order, chunks, &mut picked, &mut chosen, used, budget);
    let mut sent: Vec<(usize, bool)> = picked
        .into_iter()
        .map(|i| (i, false))
        .chain(context.into_iter().map(|i| (i, true)))
        .collect();
    sent.sort_unstable();
    sent.into_iter()
        .map(|(i, context)| Excerpt {
            ordinal: chunks[i].ordinal,
            chunk: chunks[i].chunk,
            speaker: chunks[i].speaker,
            context,
        })
        .collect()
}

/// The eligible chunks best first: reciprocal rank fusion of their nearness to the prompt and,
/// for those within [`WORD_FLOOR`] of the best, their BM25 score for its words.
fn ranking(vector: &[f32], text: &str, chunks: &[Chunk], eligible: &[usize]) -> Vec<usize> {
    let nearness: Vec<f32> = eligible
        .iter()
        .map(|&i| dot(vector, chunks[i].vector))
        .collect();
    let mut by_nearness: Vec<usize> = (0..eligible.len()).collect();
    by_nearness.sort_by(|&a, &b| nearness[b].total_cmp(&nearness[a]).then(a.cmp(&b)));
    let docs: Vec<Doc> = eligible
        .iter()
        .map(|&i| Doc {
            key: "",
            tags: &[],
            body: chunks[i].text,
        })
        .collect();
    let words = rank::rank(text, &docs).hits;
    let floor = words.first().map_or(0.0, |best| best.score * WORD_FLOOR);
    let mut fused = vec![0.0; eligible.len()];
    for (place, &e) in by_nearness.iter().enumerate() {
        fused[e] += reciprocal(place);
    }
    for (place, hit) in words.iter().enumerate() {
        if hit.score < floor {
            break;
        }
        fused[hit.index] += reciprocal(place);
    }
    let mut order: Vec<usize> = (0..eligible.len()).collect();
    order.sort_by(|&a, &b| fused[b].total_cmp(&fused[a]).then(a.cmp(&b)));
    order.into_iter().map(|e| eligible[e]).collect()
}

#[expect(
    clippy::cast_precision_loss,
    reason = "a place in a list of chunks, far below 2^52"
)]
fn reciprocal(place: usize) -> f64 {
    1.0 / (RRF_K + place as f64)
}

fn dot(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

fn cost(text: &str) -> usize {
    text.len().div_ceil(BYTES_PER_TOKEN) + FRAMING_TOKENS
}

fn context_cost(text: &str) -> usize {
    text.len().min(CONTEXT_BYTES).div_ceil(BYTES_PER_TOKEN) + FRAMING_TOKENS
}

/// Takes chunks in `order` that are not yet chosen while they fit under `limit`, and returns the
/// tokens used.
fn fill(
    order: &[usize],
    chunks: &[Chunk],
    picked: &mut Vec<usize>,
    chosen: &mut [bool],
    mut used: usize,
    limit: usize,
) -> usize {
    for &i in order {
        let cost = cost(chunks[i].text);
        if chosen[i] || used + cost > limit {
            continue;
        }
        picked.push(i);
        chosen[i] = true;
        used += cost;
    }
    used
}

/// Later chunks on the subject of the best hits, so a decision comes with what changed it.
/// What the person said and what reads as an update go first.
fn follow_ups(
    chunks: &[Chunk],
    eligible: &[usize],
    picked: &mut Vec<usize>,
    chosen: &mut [bool],
    mut used: usize,
    limit: usize,
) -> usize {
    let salient: Vec<HashSet<String>> = chunks.iter().map(|c| salient(c.text)).collect();
    let best: Vec<usize> = picked.iter().take(FOLLOW_UP_TOP).copied().collect();
    for hit in best {
        let mut later: Vec<(u8, usize, f32, usize)> = eligible
            .iter()
            .filter(|&&j| !chosen[j] && chunks[j].ordinal > chunks[hit].ordinal)
            .filter_map(|&j| {
                let shared = salient[hit].intersection(&salient[j]).count();
                let nearness = dot(chunks[hit].vector, chunks[j].vector);
                (shared >= FOLLOW_UP_SHARED || nearness >= FOLLOW_UP_NEARNESS)
                    .then(|| (preference(&chunks[j]), shared, nearness, j))
            })
            .collect();
        later.sort_by(|a, b| {
            b.0.cmp(&a.0)
                .then(b.1.cmp(&a.1))
                .then(b.2.total_cmp(&a.2))
                .then(b.3.cmp(&a.3))
        });
        for &(_, _, _, j) in later.iter().take(FOLLOW_UP_PER_HIT) {
            let cost = cost(chunks[j].text);
            if used + cost > limit {
                continue;
            }
            picked.push(j);
            chosen[j] = true;
            used += cost;
        }
    }
    used
}

fn preference(chunk: &Chunk) -> u8 {
    u8::from(chunk.speaker == Speaker::Person)
        + u8::from(chunk.speaker != Speaker::Tool && acknowledges(chunk.text))
}

/// Terms that name a subject: a word with a digit or a capital in it, or one of five letters or
/// more, as ranking spells it.
fn salient(text: &str) -> HashSet<String> {
    text.split(|c: char| !(c.is_alphanumeric() || "_.-/".contains(c)))
        .map(|word| word.trim_matches(|c| ".:-/".contains(c)))
        .filter(|word| {
            word.chars().any(|c| c.is_ascii_digit() || c.is_uppercase())
                || word.chars().count() >= 5
        })
        .flat_map(rank::terms)
        .collect()
}

/// Whether a text reads as an update to something said before.
fn acknowledges(text: &str) -> bool {
    const PHRASES: [&str; 4] = ["change of plan", "no longer", "fell through", "takes over"];
    const WORDS: [&str; 11] = [
        "update", "updated", "noted", "change", "changed", "now", "instead", "moved", "recorded",
        "saved", "added",
    ];
    let lower = text.to_lowercase();
    PHRASES.iter().any(|p| lower.contains(p))
        || lower
            .split(|c: char| !c.is_alphabetic())
            .any(|w| WORDS.contains(&w) || w.starts_with("replac") || w.starts_with("upgrad"))
}

/// The first chunk of the prompt and of the planner's last reply in each exchange a chosen
/// excerpt came from, where no chunk of that message is chosen, earliest first while they fit
/// under `budget`. Their cost is added to `used`.
fn exchange_context(
    chunks: &[Chunk],
    picked: &[usize],
    cut: u32,
    used: &mut usize,
    budget: usize,
) -> Vec<usize> {
    let mut last_reply = HashMap::new();
    for (i, c) in chunks.iter().enumerate() {
        if let (Some(exchange), Speaker::Planner, 0) = (c.exchange, c.speaker, c.chunk)
            && c.ordinal < cut
        {
            last_reply.insert(exchange, i);
        }
    }
    let sent: HashSet<u32> = picked.iter().map(|&i| chunks[i].ordinal).collect();
    let mut prompts: Vec<usize> = picked
        .iter()
        .filter_map(|&i| chunks[i].exchange)
        .flat_map(|exchange| {
            let prompt = chunks
                .binary_search_by_key(&(exchange, 0), |c| (c.ordinal, c.chunk))
                .ok();
            prompt.into_iter().chain(last_reply.get(&exchange).copied())
        })
        .filter(|&p| !sent.contains(&chunks[p].ordinal))
        .collect();
    prompts.sort_unstable();
    prompts.dedup();
    prompts.retain(|&p| {
        let cost = context_cost(chunks[p].text);
        let fits = *used + cost <= budget;
        if fits {
            *used += cost;
        }
        fits
    });
    prompts
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every fixture text is padded to this length, so each excerpt costs the same.
    const LENGTH: usize = 120;
    const COST: usize = LENGTH / BYTES_PER_TOKEN + FRAMING_TOKENS;

    struct Said {
        speaker: Speaker,
        exchange: Option<u32>,
        text: String,
        vector: Vec<f32>,
    }

    fn said(speaker: Speaker, text: &str, vector: &[f32]) -> Said {
        assert!(text.len() <= LENGTH, "{text}");
        let norm = vector.iter().map(|x| x * x).sum::<f32>().sqrt();
        Said {
            speaker,
            exchange: None,
            text: format!("{text:<LENGTH$}"),
            vector: vector.iter().map(|x| x / norm).collect(),
        }
    }

    fn person(text: &str, vector: &[f32]) -> Said {
        said(Speaker::Person, text, vector)
    }

    /// One chunk per message, the message's place being its place in `said`.
    fn chunks(said: &[Said]) -> Vec<Chunk<'_>> {
        said.iter()
            .zip(0u32..)
            .map(|(s, ordinal)| Chunk {
                ordinal,
                chunk: 0,
                speaker: s.speaker,
                exchange: s.exchange,
                text: &s.text,
                vector: &s.vector,
            })
            .collect()
    }

    /// The smallest budget whose share for excerpts holds `n` of them.
    fn budget_for(n: usize) -> usize {
        (n * COST * 100).div_ceil(100 - CONTEXT_SHARE)
    }

    fn ordinals(excerpts: &[Excerpt]) -> Vec<u32> {
        excerpts.iter().map(|e| e.ordinal).collect()
    }

    const QUERY: [f32; 3] = [1.0, 0.0, 0.0];

    #[test]
    fn an_exact_term_outranks_a_chunk_only_nearer_in_meaning() {
        let said = [
            person(
                "We talked about the weather in Lisbon and the light there.",
                &[1.0, 0.0, 0.0],
            ),
            person(
                "The zephyr-relay listens on port 8443 behind the proxy.",
                &[0.9, 0.44, 0.0],
            ),
            person(
                "Lunch orders go in before noon on Fridays.",
                &[0.0, 0.0, 1.0],
            ),
        ];
        let chosen = retrieve(
            &QUERY,
            "Which port does zephyr-relay use?",
            &chunks(&said),
            3,
            budget_for(1) + COST / 2,
        );
        assert_eq!(ordinals(&chosen), [1]);
    }

    #[test]
    fn a_chunk_sharing_no_word_with_the_prompt_is_found_by_meaning() {
        let said = [
            person(
                "The printer on the third floor is jammed.",
                &[0.5, 0.0, 0.87],
            ),
            person("The formula one race starts at noon.", &[0.1, 0.0, 0.99]),
            person(
                "Grandma's recipe needs two cups of flour and a pinch of salt.",
                &[0.95, 0.31, 0.0],
            ),
        ];
        let chosen = retrieve(
            &QUERY,
            "what was the baking formula from my grandmother",
            &chunks(&said),
            3,
            budget_for(2),
        );
        assert!(ordinals(&chosen).contains(&2), "{chosen:?}");
    }

    #[test]
    fn a_weak_word_match_adds_nothing_to_a_chunks_rank() {
        let said = [
            person(
                "Kestrel moves the ledger to the new cluster in May.",
                &[0.4, 0.0, 0.92],
            ),
            person("We should plan the holiday rota soon.", &[0.5, 0.87, 0.0]),
            person(
                "Notes from the standup about the coffee machine.",
                &[1.0, 0.0, 0.0],
            ),
        ];
        let chosen = retrieve(
            &QUERY,
            "the kestrel plan for the ledger",
            &chunks(&said),
            3,
            budget_for(2),
        );
        assert_eq!(ordinals(&chosen), [0, 2]);
    }

    #[test]
    fn a_word_match_under_four_fifths_of_the_best_adds_nothing() {
        let said = [
            person(
                "Kestrel moves the ledger migration to the new cluster in May.",
                &[0.0, 0.0, 1.0],
            ),
            person("Kestrel and the ledger.", &[0.5, 0.87, 0.0]),
            person(
                "Notes from the standup about the coffee machine.",
                &[1.0, 0.0, 0.0],
            ),
        ];
        let chunks = chunks(&said);
        let prompt = "the kestrel ledger migration";
        let docs: Vec<Doc> = chunks
            .iter()
            .map(|c| Doc {
                key: "",
                tags: &[],
                body: c.text,
            })
            .collect();
        let hits = rank::rank(prompt, &docs).hits;
        let share = hits[1].score / hits[0].score;
        assert!((0.7..0.8).contains(&share), "{share}");
        assert_eq!(ranking(&QUERY, prompt, &chunks, &[0, 1, 2]), [0, 2, 1]);
    }

    #[test]
    fn the_excerpts_fit_the_budget_and_come_in_the_order_they_were_said() {
        let said: Vec<Said> = (0..12u8)
            .map(|n| {
                let x = f32::from(n % 5) / 4.0;
                let speaker = [Speaker::Person, Speaker::Tool, Speaker::File][usize::from(n % 3)];
                said(
                    speaker,
                    &format!("Item {n} of the inventory."),
                    &[x, 1.0 - x, 0.1],
                )
            })
            .collect();
        let chunks = chunks(&said);
        let budget = budget_for(5);
        let chosen = retrieve(&QUERY, "inventory item", &chunks, 12, budget);
        assert!(chosen.len() >= 3, "{chosen:?}");
        let spent: usize = chosen
            .iter()
            .map(|e| cost(chunks[e.ordinal as usize].text))
            .sum();
        assert!(spent <= budget, "{spent} > {budget}");
        assert!(
            chosen.is_sorted_by_key(|e| (e.ordinal, e.chunk)),
            "{chosen:?}"
        );
        for e in &chosen {
            assert_eq!(e.speaker, chunks[e.ordinal as usize].speaker);
        }
    }

    #[test]
    fn a_reserve_left_unspent_goes_to_the_next_best_hits() {
        let said: Vec<Said> = (0..10u8)
            .map(|n| {
                let x = f32::from(n) / 10.0;
                person(
                    &format!("Shelf {n} holds the blue folders."),
                    &[x, 1.0 - x, 0.0],
                )
            })
            .collect();
        let chosen = retrieve(&QUERY, "folders", &chunks(&said), 10, 10 * COST);
        assert_eq!(chosen.len(), 10, "{chosen:?}");
    }

    #[test]
    fn nothing_from_the_cut_on_is_chosen() {
        let said = [
            person("The staging host is called amber.", &[0.6, 0.8, 0.0]),
            person("The staging host is now called jade.", &[1.0, 0.0, 0.0]),
        ];
        let chosen = retrieve(
            &QUERY,
            "what is the staging host called",
            &chunks(&said),
            1,
            budget_for(2),
        );
        assert_eq!(ordinals(&chosen), [0]);
    }

    fn mixed(nearness: f32) -> [f32; 3] {
        [nearness, (1.0 - nearness * nearness).sqrt(), 0.0]
    }

    #[test]
    fn a_later_change_comes_with_the_hit_it_changes() {
        let said = [
            person(
                "The team retreat is at Lakeview Lodge on the fourteenth.",
                &[1.0, 0.0, 0.0],
            ),
            person("Bring a laptop charger and a notebook.", &mixed(0.9)),
            person("Parking costs eight dollars a day.", &mixed(0.85)),
            person("Dinner on Thursday is pizza for everyone.", &mixed(0.8)),
            person("Badges are printed at the front desk.", &mixed(0.75)),
            person("Coffee arrives at nine in the morning.", &mixed(0.7)),
            person("Lockers are on the second floor.", &mixed(0.65)),
            person("Wifi details are on the back of the badge.", &mixed(0.6)),
            person("The shuttle leaves from the north gate.", &mixed(0.55)),
            person(
                "Change of plan: Lakeview Lodge fell through, so we booked Harbor Hall.",
                &[0.3, 0.0, 0.95],
            ),
        ];
        let chosen = retrieve(
            &QUERY,
            "where is the team retreat",
            &chunks(&said),
            10,
            budget_for(5),
        );
        assert_eq!(chosen.len(), 7, "{chosen:?}");
        assert!(ordinals(&chosen).contains(&0), "{chosen:?}");
        assert!(ordinals(&chosen).contains(&9), "{chosen:?}");
    }

    #[test]
    fn a_later_chunk_sharing_one_term_with_a_hit_follows_it() {
        let said = [
            person("our offsite sits at lakeview", &[1.0, 0.0, 0.0]),
            person("bring a spare charger", &[0.0, 1.0, 0.0]),
            person("bookings moved off lakeview", &[0.0, 0.0, 1.0]),
        ];
        let chunks = chunks(&said);
        let mut picked = vec![0];
        let mut chosen = vec![true, false, false];
        follow_ups(&chunks, &[0, 1, 2], &mut picked, &mut chosen, 0, usize::MAX);
        assert_eq!(picked, [0, 2]);
    }

    #[test]
    fn an_excerpt_of_a_reply_brings_the_prompt_it_answered() {
        let mut reply = said(
            Speaker::Planner,
            "Postgres 16, with the ledger table partitioned by month.",
            &[1.0, 0.0, 0.0],
        );
        reply.exchange = Some(0);
        let mut prompt = person(
            "Which database should the billing service use?",
            &[0.0, 1.0, 0.0],
        );
        prompt.exchange = Some(0);
        let mut thanks = person("Thanks, and lunch is at noon.", &[0.0, 0.0, 1.0]);
        thanks.exchange = Some(2);
        let said = [prompt, reply, thanks];
        let chosen = retrieve(
            &QUERY,
            "what did we pick for storing invoices",
            &chunks(&said),
            3,
            2 * COST,
        );
        assert_eq!(
            chosen,
            [
                Excerpt {
                    ordinal: 0,
                    chunk: 0,
                    speaker: Speaker::Person,
                    context: true,
                },
                Excerpt {
                    ordinal: 1,
                    chunk: 0,
                    speaker: Speaker::Planner,
                    context: false,
                },
            ]
        );
    }

    #[test]
    fn an_excerpt_of_a_prompt_brings_the_last_reply_of_its_exchange() {
        let mut prompt = person(
            "Which database should the billing service use?",
            &[1.0, 0.0, 0.0],
        );
        prompt.exchange = Some(0);
        let mut first = said(
            Speaker::Planner,
            "Let me look at the options.",
            &[0.0, 1.0, 0.0],
        );
        first.exchange = Some(0);
        let mut last = said(Speaker::Planner, "Postgres 16 it is.", &[0.0, 0.0, 1.0]);
        last.exchange = Some(0);
        let said = [prompt, first, last];
        let chosen = retrieve(
            &QUERY,
            "which database for billing",
            &chunks(&said),
            3,
            2 * COST,
        );
        assert_eq!(
            chosen,
            [
                Excerpt {
                    ordinal: 0,
                    chunk: 0,
                    speaker: Speaker::Person,
                    context: false,
                },
                Excerpt {
                    ordinal: 2,
                    chunk: 0,
                    speaker: Speaker::Planner,
                    context: true,
                },
            ]
        );
    }

    /// RETRIEVE-18, RETRIEVE-19: an excerpt from an exchange the cut falls inside brings the last
    /// reply before the cut, never one from the cut on, which is sent whole anyway.
    #[test]
    fn an_exchange_the_cut_falls_inside_brings_no_reply_from_the_cut_on() {
        let mut prompt = person(
            "Which database should the billing service use?",
            &[1.0, 0.0, 0.0],
        );
        prompt.exchange = Some(0);
        let mut before = said(
            Speaker::Planner,
            "Let me look at the options.",
            &[0.0, 1.0, 0.0],
        );
        before.exchange = Some(0);
        let mut after = said(Speaker::Planner, "Postgres 16 it is.", &[0.0, 0.0, 1.0]);
        after.exchange = Some(0);
        let said = [prompt, before, after];
        let chunks = chunks(&said);

        let chosen = retrieve(&QUERY, "which database for billing", &chunks, 2, 2 * COST);
        assert_eq!(
            chosen
                .iter()
                .map(|e| (e.ordinal, e.context))
                .collect::<Vec<_>>(),
            [(0, false), (1, true)]
        );
        let chosen = retrieve(&QUERY, "which database for billing", &chunks, 1, 2 * COST);
        assert_eq!(ordinals(&chosen), [0]);
    }

    /// RETRIEVE-19: what is chosen, the context lines at the length they are cut to included,
    /// costs at most the budget, whatever the budget.
    #[test]
    fn excerpts_and_their_context_lines_fit_the_budget_together() {
        let long = |speaker, exchange, text: &str, vector: &[f32]| {
            let mut said = said(speaker, text, vector);
            said.text = format!("{text} {}", "and more besides ".repeat(60));
            said.exchange = Some(exchange);
            said
        };
        let mut said = Vec::new();
        for n in 0..6u8 {
            let at = u32::from(n) * 2;
            let x = f32::from(n) / 6.0;
            let mut prompt = person(&format!("Where does service {n} run?"), &[0.2, 1.0, x]);
            prompt.exchange = Some(at);
            said.push(prompt);
            said.push(long(
                Speaker::Planner,
                at,
                &format!("Service {n} runs in region {n}."),
                &[1.0 - x, 0.1, x],
            ));
        }
        let chunks = chunks(&said);
        assert!(chunks[1].text.len() > CONTEXT_BYTES);

        let mut with_context = 0;
        for budget in (0..=12 * COST).step_by(7) {
            let chosen = retrieve(&QUERY, "where does the service run", &chunks, 12, budget);
            let spent: usize = chosen
                .iter()
                .map(|e| {
                    let text = chunks[e.ordinal as usize].text;
                    if e.context {
                        context_cost(text)
                    } else {
                        cost(text)
                    }
                })
                .sum();
            assert!(spent <= budget, "{spent} > {budget}: {chosen:?}");
            with_context += usize::from(chosen.iter().any(|e| e.context));
        }
        assert!(with_context > 0, "no budget brought a context line");
    }

    #[test]
    fn each_row_is_read_from_its_message_in_the_exchange_its_prompt_opened() {
        let row = |ordinal, chunk, speaker, range: std::ops::Range<u32>| Row {
            ordinal,
            chunk,
            speaker,
            range,
            digest: 0,
            vector: vec![1.0],
        };
        let passages = [
            Passage {
                ordinal: 0,
                speaker: Speaker::Planner,
                text: "a greeting",
            },
            Passage {
                ordinal: 2,
                speaker: Speaker::Person,
                text: "deploy the site",
            },
            Passage {
                ordinal: 3,
                speaker: Speaker::Planner,
                text: "done. first part, second part",
            },
        ];
        let rows = [
            row(0, 0, Speaker::Planner, 0..10),
            row(1, 0, Speaker::Tool, 0..4),
            row(2, 0, Speaker::Person, 0..15),
            row(3, 0, Speaker::Planner, 6..16),
            row(3, 1, Speaker::Planner, 18..29),
            row(3, 2, Speaker::Planner, 18..40),
        ];
        let read: Vec<(u32, u16, Option<u32>, &str)> = chunks_of(&rows, &passages)
            .iter()
            .map(|c| (c.ordinal, c.chunk, c.exchange, c.text))
            .collect();
        assert_eq!(
            read,
            [
                (0, 0, None, "a greeting"),
                (2, 0, Some(2), "deploy the site"),
                (3, 0, Some(2), "first part"),
                (3, 1, Some(2), "second part"),
            ]
        );
    }
}
