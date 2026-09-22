---
id: SPEECH
title: Speech input and output
status: normative
documented-by: none [speech input and output is an experimental feature under Issue #73]
governs:
  - crates/speech/src/lib.rs
  - crates/speech/src/recognizer.rs
  - crates/speech/src/model.rs
  - crates/speech/src/synthesis.rs
---

## Scope

Microphone audio capture, speech-to-text transcription, model asset management, and speech synthesis. What a transcript is labelled, how it reaches the input box, and what keeps spoken input from becoming an unvouched prompt.

## Clauses

<a id="SPEECH-1"></a>
### SPEECH-1: speech transcripts land in the input box, and convey no standing provenance

Transcribed text lands in the terminal input box as though typed or pasted. It does not initiate a turn, cannot auto-send on silence, and receives a first label only when the person reads it and presses Enter.

**Why.** A speech recognition model can emit tokens nobody spoke, either through acoustic noise or model hallucination. Landing the transcript in the input box preserves human review and prevents spoken audio from acting as an unvouched prompt injection vector.

`verified-by: bravebot_speech::tests::speech_transcript_lands_in_input_box_and_does_not_send`

<a id="SPEECH-2"></a>
### SPEECH-2: microphone capture displays a persistent visual indicator

While the audio capture stream is open, a visual indicator is drawn continuously in the terminal status or indicator line.

**Why.** Microphone capture is a local disclosure. A person must know at every moment whether audio is being captured or recorded.

`verified-by: bravebot_speech::tests::microphone_capture_sets_live_indicator`

<a id="SPEECH-3"></a>
### SPEECH-3: speech models are acquired solely through the standard network egress

Any model asset fetched over the network goes through bravebot-net under the fetch capability, verifies its SHA-256 digest before use, and is stored under the state directory complying with mode 0700 for directories and 0600 for files.

**Why.** [network-egress.md](network-egress.md) permits only one way out of this process. An audio subsystem opening independent HTTP sockets or unverified downloads would be a structural egress violation.

`verified-by: bravebot_speech::tests::model_download_verifies_checksum_and_uses_state_dir`

<a id="SPEECH-4"></a>
### SPEECH-4: cancellation terminates capture and purges audio buffers immediately

Pressing Escape or the cancellation key halts audio recording, drops pending audio frames, and discards partial recognition buffers without inserting text into the active prompt.

**Why.** An aborted voice input must not leave lingering audio in memory or bleed into subsequent commands.

`verified-by: bravebot_speech::tests::cancellation_purges_audio_buffers`

<a id="SPEECH-5"></a>
### SPEECH-5: speech output silences immediately upon any keystroke

When assistant speech synthesis is active, any keyboard event silences speech output immediately.

**Why.** Spoken output must never talk over a person who is typing or navigating the interface.

`verified-by: bravebot_speech::tests::keystroke_silences_speech_output`
