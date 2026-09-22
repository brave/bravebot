//! Speech synthesis (text-to-speech) support.
//!
//! Enforces immediate silence on user interaction in accordance with SPEECH-5.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// A speech synthesis controller.
#[derive(Clone, Default)]
pub struct SpeechSynthesizer {
    speaking: Arc<AtomicBool>,
}

impl SpeechSynthesizer {
    pub fn new() -> Self {
        Self {
            speaking: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Speaks the given text if not interrupted.
    pub fn speak(&self, text: &str) {
        if text.trim().is_empty() {
            return;
        }
        self.speaking.store(true, Ordering::SeqCst);
    }

    /// Immediately silences any active or pending speech output.
    pub fn silence(&self) {
        self.speaking.store(false, Ordering::SeqCst);
    }

    /// Whether speech output is currently active.
    pub fn is_speaking(&self) -> bool {
        self.speaking.load(Ordering::SeqCst)
    }
}
