use crate::model;
use crate::{SpeechController, SpeechEvent, SpeechSynthesizer};
use std::sync::mpsc;

#[test]
fn speech_transcript_lands_in_input_box_and_does_not_send() {
    // SPEECH-1: Transcript emits as text event, never initiating a turn execution automatically.
    let controller = SpeechController::new();
    let (tx, rx) = mpsc::channel();

    controller.start(tx.clone()).unwrap();
    assert!(controller.is_recording());

    controller.stop(&tx, Some("hello bravebot".to_string()));
    assert!(!controller.is_recording());

    let events: Vec<_> = rx.try_iter().collect();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0], SpeechEvent::Started);
    assert_eq!(
        events[1],
        SpeechEvent::Transcript("hello bravebot".to_string())
    );
}

#[test]
fn microphone_capture_sets_live_indicator() {
    // SPEECH-2: While recording, is_recording reports true to drive the TUI visual indicator.
    let controller = SpeechController::new();
    let (tx, _rx) = mpsc::channel();

    assert!(!controller.is_recording());
    controller.start(tx.clone()).unwrap();
    assert!(controller.is_recording());
    controller.stop(&tx, None);
    assert!(!controller.is_recording());
}

#[test]
fn model_download_verifies_checksum_and_uses_state_dir() {
    // SPEECH-3: Verifies SHA-256 integrity checks and secure directory resolution.
    let bytes = b"test speech model content";
    use sha2::{Digest, Sha256};
    let expected = format!("{:x}", Sha256::digest(bytes));

    assert!(model::verify_digest(bytes, &expected).is_ok());
    assert!(model::verify_digest(bytes, "bad_checksum").is_err());

    let state_dir = model::default_model_directory();
    assert!(state_dir.is_ok());
}

#[test]
fn cancellation_purges_audio_buffers() {
    // SPEECH-4: Cancelling discards partial audio and emits Canceled without transcript text.
    let controller = SpeechController::new();
    let (tx, rx) = mpsc::channel();

    controller.start(tx.clone()).unwrap();
    controller.cancel(&tx);
    assert!(!controller.is_recording());

    let events: Vec<_> = rx.try_iter().collect();
    assert!(events.contains(&SpeechEvent::Canceled));
    for ev in &events {
        if let SpeechEvent::Transcript(_) = ev {
            panic!("transcript emitted despite cancellation");
        }
    }
}

#[test]
fn keystroke_silences_speech_output() {
    // SPEECH-5: Speech synthesis silences immediately upon keystroke or interrupt.
    let synth = SpeechSynthesizer::new();
    synth.speak("reading a long response");
    assert!(synth.is_speaking());

    synth.silence();
    assert!(!synth.is_speaking());
}
