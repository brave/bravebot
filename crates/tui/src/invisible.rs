//! Characters a terminal draws as nothing, taken out of a paste.
//!
//! Text copied from a page or a file can hold characters that change what a model reads without
//! showing anything: the tag block, which writes ASCII in characters with no glyph, bidirectional
//! controls, zero-width characters, and runs of variation selectors. A paste lands in the person's
//! own message, so it carries no label, and what the person cannot see they cannot have meant.
//!
//! The map is [`bravebot_core::invisible::without_invisible`], shared with the desktop app. It
//! reads no meaning from the text, decides nothing about any effect, and its only output besides
//! the cleaned text is a count, which goes to the person's screen.

pub use bravebot_core::invisible::without_invisible;
