//! Speech input (STT) and output (TTS) subsystems for bravebot.
//!
//! Provides speech-to-text transcription landing strictly in the terminal input box
//! for human review (SPEECH-1), active visual status indication (SPEECH-2),
//! safe network model acquisition (SPEECH-3), immediate cancellation (SPEECH-4),
//! and keystroke-triggered synthesis silence (SPEECH-5).

pub mod model;
pub mod recognizer;
pub mod synthesis;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;

pub use model::{resolve_model_path, ModelError};
pub use recognizer::{RecognitionResult, Recognizer};
pub use synthesis::SpeechSynthesizer;

/// Events emitted during speech capture and recognition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpeechEvent {
    Started,
    Partial(String),
    Transcript(String),
    Canceled,
    Error(String),
}

/// The current state of speech capture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SpeechState {
    #[default]
    Idle,
    Listening,
    Processing,
}

/// High-level coordinator for speech input and recording.
#[derive(Clone, Default)]
pub struct SpeechController {
    recording: Arc<AtomicBool>,
    canceled: Arc<AtomicBool>,
}

impl SpeechController {
    pub fn new() -> Self {
        Self {
            recording: Arc::new(AtomicBool::new(false)),
            canceled: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Whether the microphone capture stream is actively open.
    pub fn is_recording(&self) -> bool {
        self.recording.load(Ordering::SeqCst)
    }

    /// Begins recording, streaming speech events across the provided channel.
    pub fn start(&self, tx: mpsc::Sender<SpeechEvent>) -> Result<(), String> {
        if self.is_recording() {
            return Err("already recording".to_string());
        }

        self.recording.store(true, Ordering::SeqCst);
        self.canceled.store(false, Ordering::SeqCst);

        let _ = tx.send(SpeechEvent::Started);
        Ok(())
    }

    /// Completes recording and emits final transcript if available.
    pub fn stop(&self, tx: &mpsc::Sender<SpeechEvent>, final_text: Option<String>) {
        if !self.is_recording() {
            return;
        }
        self.recording.store(false, Ordering::SeqCst);

        if self.canceled.load(Ordering::SeqCst) {
            let _ = tx.send(SpeechEvent::Canceled);
        } else if let Some(text) = final_text {
            if !text.trim().is_empty() {
                let _ = tx.send(SpeechEvent::Transcript(text));
            }
        }
    }

    /// Cancels recording immediately, purging any pending audio and text.
    pub fn cancel(&self, tx: &mpsc::Sender<SpeechEvent>) {
        self.canceled.store(true, Ordering::SeqCst);
        self.recording.store(false, Ordering::SeqCst);
        let _ = tx.send(SpeechEvent::Canceled);
    }
}

#[cfg(test)]
pub mod tests;
