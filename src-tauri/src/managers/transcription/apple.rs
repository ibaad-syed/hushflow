//! Apple's on-device SpeechAnalyzer as a transcription engine (macOS 26+).
//!
//! It stands in for the transcribe-cpp worker when the "apple-speech" system
//! model is selected: batch runs and live streams go to the OS instead. The
//! OS owns the model, so "loading" only makes sure its assets are installed.

use super::{emit_stream_text, run_setup, RunOutcome, StreamCmd, TranscriptionManager};
use crate::engine_supervisor::{EngineError, LoadedInfo};
use crate::settings::get_settings;
use log::{info, warn};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use transcribe_cpp::{Capabilities, TimestampKind, Transcript};

/// Registry id of the Apple engine's system model.
pub const MODEL_ID: &str = "apple-speech";

/// What the rest of the pipeline sees as the "loaded model" while the Apple
/// engine is active. Long-form so recordings are never chunked: the analyzer
/// handles long audio itself.
pub(super) fn loaded_info() -> LoadedInfo {
    LoadedInfo {
        arch: "apple_speech".into(),
        variant: "SpeechTranscriber".into(),
        backend: "Apple".into(),
        device: "System".into(),
        on_gpu: false,
        capabilities: Capabilities {
            native_sample_rate: 16_000,
            languages: vec!["en".into()],
            translate_target_languages: Vec::new(),
            max_timestamp_kind: TimestampKind::None,
            supports_language_detect: false,
            supports_translate: false,
            supports_streaming: true,
            supports_spec_decode: false,
            max_audio_ms: 0,
        },
        supports_initial_prompt: false,
        supports_long_form: true,
        supports_vocabulary: false,
    }
}

/// Makes sure the OS speech assets are installed.
pub(super) fn prepare() -> Result<(), String> {
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        if !crate::apple_speech::is_available() {
            return Err("Apple speech recognition is not available on this Mac".into());
        }
        crate::apple_speech::prepare()
    }
    #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
    Err("Apple speech recognition is only available on Apple Silicon Macs".into())
}

/// One batch run, shaped like a transcribe-cpp result.
pub(super) fn transcribe(pcm: &[f32]) -> Result<Transcript, EngineError> {
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    let text = crate::apple_speech::transcribe(pcm).map_err(EngineError::Failed)?;
    #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
    let text: String = {
        let _ = pcm;
        return Err(EngineError::Failed(
            "Apple speech recognition is unavailable".into(),
        ));
    };
    Ok(Transcript {
        raw_text: text.clone(),
        text,
        ..Default::default()
    })
}

/// Runs one recording's live decode on the Apple engine: frames in, overlay
/// text out, the full transcript on finalize. If the session can't start,
/// the recording falls back to a batch run when it stops.
pub(super) fn run_stream(
    tm: &TranscriptionManager,
    rx: mpsc::Receiver<StreamCmd>,
    cancelled: Arc<AtomicBool>,
    model_id: &str,
) {
    let settings = get_settings(&tm.app_handle);
    let setup = run_setup(&loaded_info(), &settings, "en");

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    let stream = {
        let app_handle = tm.app_handle.clone();
        crate::apple_speech::AppleStream::begin(move |committed, tentative| {
            emit_stream_text(&app_handle, committed, tentative);
        })
    };
    #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
    let stream: Result<NoStream, String> = Err("unavailable".into());

    let stream = match stream {
        Ok(stream) => stream,
        Err(e) => {
            warn!("Apple live preview unavailable ({e}); using batch transcription");
            tm.router.clear();
            super::drain_until_finalize(rx);
            return;
        }
    };
    info!("Live preview: Apple SpeechAnalyzer stream started (model '{model_id}')");
    tm.stream_active.store(true, Ordering::Release);
    tm.touch_activity();

    while let Ok(cmd) = rx.recv() {
        match cmd {
            StreamCmd::Feed(pcm) => {
                if cancelled.load(Ordering::Acquire) {
                    continue;
                }
                tm.touch_activity();
                stream.feed(&pcm);
            }
            StreamCmd::Finalize { reply, .. } => {
                let outcome = match stream.finish() {
                    Ok(text) if !text.is_empty() => Some(RunOutcome {
                        setup,
                        text,
                        model_detected_language: None,
                    }),
                    Ok(_) => None,
                    Err(e) => {
                        warn!("Apple live transcription failed ({e}); using batch transcription");
                        None
                    }
                };
                let _ = reply.send(Ok(outcome));
                return;
            }
            // Dropping the stream cancels the OS session.
            StreamCmd::Cancel => return,
        }
    }
}

/// Placeholder so non-macOS builds type-check `run_stream`.
#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
struct NoStream;
#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
impl NoStream {
    fn feed(&self, _: &[f32]) {}
    fn finish(self) -> Result<String, String> {
        Err("unavailable".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::managers::transcription::policy::{batch_decode, BatchDecode, ModelTraits};

    #[test]
    fn apple_engine_decodes_recordings_whole() {
        // The analyzer handles long audio itself; chunking would only add seams.
        let traits = ModelTraits::of(&loaded_info());
        assert_eq!(batch_decode(traits, true), BatchDecode::Whole);
        assert!(traits.streaming);
    }
}
