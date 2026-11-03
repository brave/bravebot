//! The `bravebot schedule` subcommands as functions that return their lines and how they ended,
//! so a front end other than the terminal can run them. See [`command`].

#![forbid(unsafe_code)]

mod calendar;
pub mod firing;
mod schedule;
pub mod words;

pub use schedule::*;
