//! `bravebot_agent::regex`: a pattern the planner gives `search`.
//!
//! A pattern rendered from a syntax tree is compared with an evaluator that reads the tree, so a
//! pattern the engine mismatches is found by what it matches rather than by whether it panics.
//! Raw pattern text reaches the parser's error paths, where a tree never goes.

#![no_main]
#![forbid(unsafe_code)]

use arbitrary::Arbitrary;
use bravebot_agent::regex::{PatternError, Regex};
use bravebot_fuzz::pattern::{self, Expected, MAX_PATTERN, Re, Text};
use libfuzzer_sys::fuzz_target;

#[derive(Arbitrary, Debug)]
enum Input {
    /// A pattern rendered from `tree`, wrapped in `nest` groups and padded with literals toward
    /// `pad`, so that the limits on depth and length are crossed from both sides.
    Tree {
        tree: Re,
        nest: u8,
        pad: Option<u8>,
        folded: bool,
        text: Text,
    },
    /// Pattern text as it arrives, which has no oracle: it is compiled and matched, and the
    /// engine is held to agreeing with itself.
    Raw {
        pattern: String,
        folded: bool,
        text: String,
    },
}

fn compile(pattern: &str, folded: bool) -> Result<Regex, PatternError> {
    if folded {
        Regex::compile_folded(pattern)
    } else {
        Regex::compile(pattern)
    }
}

/// What `regex` says of `text`, held to being the same after a longer text has gone through the
/// buffers the engine keeps between calls, and the same from the automaton alone.
fn answer(regex: &Regex, text: &str) -> bool {
    let first = regex.matches(text);
    regex.matches(&text.repeat(3));
    assert_eq!(regex.matches(text), first, "{text:?} changed its answer");
    assert_eq!(
        regex.matches_by_automaton(text),
        first,
        "{text:?} is answered differently by the automaton alone"
    );
    first
}

fuzz_target!(|input: Input| {
    match input {
        Input::Tree {
            tree,
            nest,
            pad,
            folded,
            text,
        } => {
            let nest = usize::from(nest % 64);
            let pad = pad.map_or(0, |at| {
                // Padding toward 992 + `at` characters puts the length either side of the limit.
                let base = pattern::build(&tree, nest, 0, folded).text.chars().count();
                (992 + usize::from(at)).saturating_sub(base + 2)
            });
            let built = pattern::build(&tree, nest, pad, folded);
            match (compile(&built.text, folded), built.expected) {
                (Err(PatternError::TooLong { length }), Expected::TooLong { length: want }) => {
                    assert_eq!(length, want);
                }
                (Err(PatternError::TooDeep), Expected::TooDeep) => {}
                (Ok(regex), Expected::Compiles(node)) => {
                    let want = pattern::matches(&node, &text.0);
                    assert_eq!(
                        answer(&regex, &text.0),
                        want,
                        "{:?} against {:?}: the evaluator reads the tree as {node:?}",
                        built.text,
                        text.0
                    );
                }
                (got, want) => panic!("{:?} compiled to {got:?}, expected {want:?}", built.text),
            }
        }
        Input::Raw {
            pattern,
            folded,
            text,
        } => {
            let length = pattern.chars().count();
            let compiled = compile(&pattern, folded);
            match &compiled {
                Err(PatternError::TooLong { length: reported }) => {
                    assert!(length > MAX_PATTERN && *reported == length);
                }
                _ => assert!(
                    length <= MAX_PATTERN,
                    "{length} characters compiled to {compiled:?}"
                ),
            }
            if let Err(refusal) = &compiled {
                assert!(!refusal.to_string().is_empty());
            }
            // Folding changes what a pattern matches and never whether it is a pattern.
            let other = compile(&pattern, !folded);
            assert_eq!(compiled.as_ref().err(), other.as_ref().err());

            if let Ok(regex) = compiled {
                // Cut so that a run is not dominated by one long line.
                let text: String = text.chars().take(256).collect();
                answer(&regex, &text);
            }
        }
    }
});
