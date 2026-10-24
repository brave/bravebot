//! What the fuzz targets in `fuzz_targets/` share: the generators that turn the fuzzer's bytes into
//! patterns, texts and byte streams, the encoders that make a stream a decoder can get into, and
//! the evaluators the targets compare the code under test with.
//!
//! Nothing here reads a file or starts a program. An evaluator shares no code with the function it
//! checks, so a disagreement is a defect in one of the two.

#![forbid(unsafe_code)]

pub mod frames;
pub mod lines;
pub mod pattern;
pub mod splits;
