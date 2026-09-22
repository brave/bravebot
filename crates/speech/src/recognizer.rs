//! Audio processing and speech recognition worker.
//!
//! When `vosk-backend` is enabled, interacts with Vosk Kaldi bindings.
//! In default builds without external dynamic libraries, provides a clean fallback
//! recognizer to ensure 100% pure Rust cross-compilability across all 6 architectures.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// A speech recognition engine instance.
pub struct Recognizer {
    canceled: Arc<AtomicBool>,
    #[cfg(feature = "vosk-backend")]
    inner: Option<vosk::Recognizer>,
}

/// Result of feeding an audio chunk into the recognizer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecognitionResult {
    Partial(String),
    Final(String),
    None,
}

impl Recognizer {
    /// Creates a new recognizer from a model directory path.
    pub fn new(model_path: &Path) -> Result<Self, String> {
        let canceled = Arc::new(AtomicBool::new(false));

        #[cfg(feature = "vosk-backend")]
        {
            let model = vosk::Model::new(model_path.to_str().ok_or("invalid path")?)
                .ok_or_else(|| "failed to load vosk model".to_string())?;
            let recognizer = vosk::Recognizer::new(&model, 16000.0)
                .ok_or_else(|| "failed to instantiate vosk recognizer".to_string())?;
            Ok(Self {
                canceled,
                inner: Some(recognizer),
            })
        }

        #[cfg(not(feature = "vosk-backend"))]
        {
            let _ = model_path;
            Ok(Self { canceled })
        }
    }

    /// Sets the canceled flag, immediately preventing further audio processing.
    pub fn cancel(&self) {
        self.canceled.store(true, Ordering::SeqCst);
    }

    /// Checks if recognition was canceled.
    pub fn is_canceled(&self) -> bool {
        self.canceled.load(Ordering::SeqCst)
    }

    /// Feeds 16-bit PCM audio samples (16kHz mono) into the recognizer.
    pub fn accept_waveform(&mut self, data: &[i16]) -> RecognitionResult {
        if self.is_canceled() {
            return RecognitionResult::None;
        }

        #[cfg(feature = "vosk-backend")]
        {
            if let Some(ref mut rec) = self.inner {
                let state = rec.accept_waveform(data);
                if let Ok(vosk::DecodingState::Finalized) = state {
                    match rec.result() {
                        vosk::CompleteResult::Single(s) => {
                            if !s.text.is_empty() {
                                return RecognitionResult::Final(s.text.to_string());
                            }
                        }
                        vosk::CompleteResult::Multiple(m) => {
                            if let Some(first) = m.alternatives.first()
                                && !first.text.is_empty()
                            {
                                return RecognitionResult::Final(first.text.to_string());
                            }
                        }
                    }
                } else {
                    let partial = rec.partial_result();
                    if !partial.partial.is_empty() {
                        return RecognitionResult::Partial(partial.partial.to_string());
                    }
                }
            }
            RecognitionResult::None
        }

        #[cfg(not(feature = "vosk-backend"))]
        {
            let _ = data;
            RecognitionResult::None
        }
    }

    /// Obtains final recognized text and flushes buffers.
    pub fn finish(&mut self) -> Option<String> {
        if self.is_canceled() {
            return None;
        }

        #[cfg(feature = "vosk-backend")]
        {
            if let Some(ref mut rec) = self.inner {
                match rec.final_result() {
                    vosk::CompleteResult::Single(s) => {
                        if !s.text.is_empty() {
                            return Some(s.text.to_string());
                        }
                    }
                    vosk::CompleteResult::Multiple(m) => {
                        if let Some(first) = m.alternatives.first()
                            && !first.text.is_empty()
                        {
                            return Some(first.text.to_string());
                        }
                    }
                }
            }
            None
        }

        #[cfg(not(feature = "vosk-backend"))]
        {
            None
        }
    }
}
