//! Ranking facts against a query: BM25 over the key (twice), tags and body ([RECALL-10]).
//!
//! Pure: no I/O, and no knowledge of superseded facts, so callers filter first.

use std::collections::{HashMap, HashSet};

use super::{BM25_B, BM25_K1, WEAK_BELOW};

/// Words too common to rank on.
const STOP_WORDS: &[&str] = &[
    "the", "and", "or", "of", "to", "in", "on", "for", "is", "it", "be", "as", "at", "by", "with",
    "that", "this", "from", "are", "was", "not", "but", "if", "so", "we", "you", "my", "our", "me",
    "do", "does", "can", "may", "should", "how", "what", "which", "who", "when", "where", "why",
    "there", "their", "they", "them", "its", "into", "than", "then", "also", "just", "any", "all",
    "about", "after", "before", "over", "under", "out", "up", "off", "an",
];

/// Where a word splits into parts, so `~ricsul-bilwyt` is one word and also `ricsul` and `bilwyt`.
fn is_seam(c: char) -> bool {
    matches!(c, '~' | '-' | '.' | '_' | '/')
}

/// Plurals and the commonest suffixes off, so releases, released and releasing meet release.
fn stem(w: &str) -> String {
    let n = w.chars().count();
    if n < 5 {
        return w.to_string();
    }
    if let Some(head) = w.strip_suffix("ies") {
        return format!("{head}y");
    }
    if let Some(head) = w.strip_suffix("sses") {
        return format!("{head}ss");
    }
    if n > 6
        && let Some(head) = w.strip_suffix("ing")
    {
        return head.to_string();
    }
    if n > 5
        && let Some(head) = w.strip_suffix("ed")
    {
        return head.to_string();
    }
    if w.ends_with('s') && !w.ends_with("ss") && !w.ends_with("us") {
        return w.strip_suffix('s').unwrap_or(w).to_string();
    }
    w.to_string()
}

/// A text's ranking terms, in order with repeats kept: lower-cased words, a compound
/// giving the whole word and each part, words under two characters and stop words
/// dropped, then stemmed.
pub fn terms(text: &str) -> Vec<String> {
    let lower = text.to_lowercase();
    let mut out = Vec::new();
    for run in lower.split(|c: char| !(c.is_alphanumeric() || is_seam(c))) {
        let whole = run.trim_matches(is_seam);
        if whole.is_empty() {
            continue;
        }
        let parts: Vec<&str> = if whole.contains(is_seam) {
            std::iter::once(whole)
                .chain(whole.split(is_seam).filter(|p| !p.is_empty()))
                .collect()
        } else {
            vec![whole]
        };
        for p in parts {
            if p.chars().count() >= 2 && !STOP_WORDS.contains(&p) {
                out.push(stem(p));
            }
        }
    }
    out
}

/// One fact as ranking sees it. The front matter is never part of it.
pub struct Doc<'a> {
    pub key: &'a str,
    pub tags: &'a [String],
    pub body: &'a str,
}

/// One fact that matched: its place in the input, BM25 score, strength (percent of the
/// query's IDF weight it carries) and how many distinct query terms it holds.
#[derive(Debug, Clone, PartialEq)]
pub struct Hit {
    pub index: usize,
    pub score: f64,
    pub strength: u8,
    pub matched_terms: usize,
}

/// Hits best first, and whether the best is under [`WEAK_BELOW`] (or there is none),
/// meaning probably nothing is stored.
#[derive(Debug, Clone, PartialEq)]
pub struct Ranked {
    pub hits: Vec<Hit>,
    pub weak: bool,
}

/// Text inside double quotes in the query, each piece lower-cased with whitespace collapsed.
fn phrases(query: &str) -> Vec<String> {
    query
        .split('"')
        .skip(1)
        .step_by(2)
        .map(|p| {
            p.to_lowercase()
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
        })
        .filter(|p| !p.is_empty())
        .collect()
}

/// BM25 of each fact for the query, best first. A quoted phrase must appear as written
/// in the fact's key, tags or body, or the fact is dropped.
pub fn rank(query: &str, docs: &[Doc]) -> Ranked {
    struct Counted {
        tf: HashMap<String, usize>,
        len: usize,
        text: String,
    }
    let query_terms: Vec<String> = {
        let mut seen = HashSet::new();
        terms(query)
            .into_iter()
            .filter(|t| seen.insert(t.clone()))
            .collect()
    };
    let none = Ranked {
        hits: Vec::new(),
        weak: true,
    };
    if docs.is_empty() || query_terms.is_empty() {
        return none;
    }
    let phrases = phrases(query);
    let counted: Vec<Counted> = docs
        .iter()
        .map(|d| {
            let mut all = terms(d.key);
            all.extend(terms(d.key));
            for tag in d.tags {
                all.extend(terms(tag));
            }
            all.extend(terms(d.body));
            let len = all.len();
            let mut tf = HashMap::new();
            for t in all {
                *tf.entry(t).or_insert(0) += 1;
            }
            let text = format!("{} {} {}", d.key, d.tags.join(" "), d.body)
                .to_lowercase()
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            Counted { tf, len, text }
        })
        .collect();

    let n = docs.len() as f64;
    let avg_len = (counted.iter().map(|c| c.len).sum::<usize>() as f64 / n).max(1.0);
    let idf: Vec<f64> = query_terms
        .iter()
        .map(|t| {
            let df = counted.iter().filter(|c| c.tf.contains_key(t)).count() as f64;
            (1.0 + (n - df + 0.5) / (df + 0.5)).ln()
        })
        .collect();
    let total_idf: f64 = idf.iter().sum();

    let mut hits: Vec<Hit> = counted
        .iter()
        .enumerate()
        .filter(|(_, c)| phrases.iter().all(|p| c.text.contains(p.as_str())))
        .filter_map(|(index, c)| {
            let mut score = 0.0;
            let mut matched_idf = 0.0;
            let mut matched_terms = 0;
            for (t, w) in query_terms.iter().zip(&idf) {
                let Some(&f) = c.tf.get(t) else { continue };
                let f = f as f64;
                let norm = 1.0 - BM25_B + BM25_B * c.len as f64 / avg_len;
                score += w * f * (BM25_K1 + 1.0) / (f + BM25_K1 * norm);
                matched_idf += w;
                matched_terms += 1;
            }
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "a share of the idf, so 0 to 100 once rounded and held to 100"
            )]
            let strength = (100.0 * matched_idf / total_idf).round().min(100.0) as u8;
            (score > 0.0).then_some(Hit {
                index,
                score,
                strength,
                matched_terms,
            })
        })
        .collect();
    hits.sort_by(|a, b| b.score.total_cmp(&a.score).then(a.index.cmp(&b.index)));
    let weak = hits.first().is_none_or(|h| h.strength < WEAK_BELOW);
    Ranked { hits, weak }
}

/// The percent of `a`'s distinct terms found in `b`.
pub fn term_share(a: &str, b: &str) -> u8 {
    let a: HashSet<String> = terms(a).into_iter().collect();
    if a.is_empty() {
        return 0;
    }
    let b: HashSet<String> = terms(b).into_iter().collect();
    u8::try_from(100 * a.intersection(&b).count() / a.len()).unwrap_or(100)
}

/// The keys inside `[[key]]` or `[[key|label]]`, in order.
pub fn links(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = body;
    while let Some((_, after)) = rest.split_once("[[") {
        let Some((inside, tail)) = after.split_once("]]") else {
            break;
        };
        let key = inside.split('|').next().unwrap_or("").trim();
        if !key.is_empty() && !key.contains(['[', '\n']) {
            out.push(key.to_string());
        }
        rest = tail;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc<'a>(key: &'a str, body: &'a str) -> Doc<'a> {
        Doc {
            key,
            tags: &[],
            body,
        }
    }

    #[test]
    fn a_compound_gives_the_whole_word_and_each_part() {
        let t = terms("~ricsul-bilwyt");
        for want in ["ricsul-bilwyt", "ricsul", "bilwyt"] {
            assert!(t.contains(&want.to_string()), "{t:?} lacks {want}");
        }
    }

    #[test]
    fn stop_words_are_dropped_and_words_lightly_stemmed() {
        assert_eq!(terms("the polling forges"), ["poll", "forge"]);
    }

    #[test]
    fn the_key_counts_twice() {
        let docs = [
            doc("notes", "the deploy runs nightly"),
            doc("deploy", "the notes runs nightly"),
        ];
        let r = rank("deploy", &docs);
        assert_eq!(r.hits[0].index, 1);
        assert_eq!(r.hits.len(), 2);
        assert!(r.hits[0].score > r.hits[1].score);
    }

    #[test]
    fn strength_is_the_share_of_idf_matched() {
        let docs = [
            doc("a", "alpha"),
            doc("b", "beta"),
            doc("c", "gamma"),
            doc("d", "delta"),
        ];
        let r = rank("alpha beta", &docs);
        assert_eq!(r.hits.len(), 2);
        assert!(r.hits.iter().all(|h| h.strength.abs_diff(50) <= 1), "{r:?}");
        let r = rank("alpha", &docs);
        assert_eq!(r.hits[0].strength, 100);
        let both = [doc("a", "alpha beta"), doc("b", "gamma")];
        assert_eq!(rank("alpha beta", &both).hits[0].strength, 100);
    }

    #[test]
    fn a_best_hit_under_35_is_weak() {
        let docs = [
            doc("a", "alpha"),
            doc("b", "gamma"),
            doc("c", "delta"),
            doc("d", "omega"),
        ];
        let r = rank("alpha sigma tau upsilon", &docs);
        assert!(r.hits[0].strength < 35, "{r:?}");
        assert!(r.weak);
        assert!(!rank("alpha", &docs).weak);
    }

    #[test]
    fn a_quoted_phrase_must_appear_as_written() {
        let docs = [
            doc("a", "forge polls every hour"),
            doc("b", "polls ... forge"),
        ];
        let r = rank("\"forge polls\"", &docs);
        assert_eq!(r.hits.iter().map(|h| h.index).collect::<Vec<_>>(), [0]);
        assert_eq!(rank("forge polls", &docs).hits.len(), 2);
    }

    #[test]
    fn matched_terms_counts_distinct_query_terms() {
        let docs = [doc("a", "forge forge forge polls"), doc("b", "forge")];
        let r = rank("forge forge polls missing", &docs);
        assert_eq!(r.hits[0].matched_terms, 2);
        assert_eq!(r.hits[1].matched_terms, 1);
    }

    #[test]
    fn text_with_no_spaces_or_only_punctuation_ranks_without_panic() {
        assert!(!terms("日本語のテスト").is_empty());
        let docs = [doc("a", "日本語のテスト"), doc("b", "forge")];
        assert_eq!(rank("日本語のテスト", &docs).hits.len(), 1);
        let r = rank("!!!", &docs);
        assert!(r.weak);
        assert!(r.hits.is_empty());
    }

    #[test]
    fn term_share_of_near_duplicates_is_over_60() {
        let a = "the forge polls the queue every minute";
        let b = "forge polls the queue every five minutes";
        assert!(term_share(a, b) > 60);
        assert!(term_share(a, "unrelated words here") < 20);
        assert_eq!(term_share("the", "anything"), 0);
    }

    #[test]
    fn links_reads_both_forms() {
        assert_eq!(
            links("see [[forge-polls]] and [[deploy|how we ship]], not [[ ]] or [[open"),
            ["forge-polls", "deploy"]
        );
    }
}
