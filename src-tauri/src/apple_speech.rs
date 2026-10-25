//! Apple's on-device SpeechAnalyzer (macOS 26+) as a transcription engine.
//!
//! The OS owns and updates the speech model, so this engine needs no download
//! from us. All calls block on Swift async work; call them off the main thread.

use std::ffi::{c_char, c_void, CStr};
use std::sync::OnceLock;

#[repr(C)]
struct AppleSpeechResult {
    text: *mut c_char,
    success: i32,
    error_message: *mut c_char,
}

type StreamCallback = extern "C" fn(*mut c_void, *const c_char, *const c_char);

extern "C" {
    fn apple_speech_available() -> i32;
    fn apple_speech_prepare() -> *mut AppleSpeechResult;
    fn apple_speech_transcribe(samples: *const f32, count: isize) -> *mut AppleSpeechResult;
    fn apple_speech_stream_begin(context: *mut c_void, callback: StreamCallback) -> *mut c_void;
    fn apple_speech_stream_feed(session: *mut c_void, samples: *const f32, count: isize);
    fn apple_speech_stream_finish(session: *mut c_void) -> *mut AppleSpeechResult;
    fn apple_speech_stream_cancel(session: *mut c_void);
    fn apple_speech_free_result(result: *mut AppleSpeechResult);
}

/// Whether SpeechTranscriber can run on this Mac. Hardware and OS support
/// don't change while the app runs, so the answer is computed once.
pub fn is_available() -> bool {
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    *AVAILABLE.get_or_init(|| unsafe { apple_speech_available() == 1 })
}

/// Installs the OS speech assets if they aren't present yet.
pub fn prepare() -> Result<(), String> {
    take_result(unsafe { apple_speech_prepare() }).map(|_| ())
}

/// Transcribes 16 kHz mono samples in one pass.
pub fn transcribe(samples: &[f32]) -> Result<String, String> {
    take_result(unsafe { apple_speech_transcribe(samples.as_ptr(), samples.len() as isize) })
}

/// A live session that reports `(committed, tentative)` text as audio arrives.
pub struct AppleStream {
    session: *mut c_void,
    callback: *mut Box<dyn Fn(&str, &str) + Send + Sync>,
}

// The Swift session serializes feeds on its own queue; the handle is safe to move.
unsafe impl Send for AppleStream {}

impl AppleStream {
    pub fn begin(on_text: impl Fn(&str, &str) + Send + Sync + 'static) -> Result<Self, String> {
        let boxed: Box<Box<dyn Fn(&str, &str) + Send + Sync>> = Box::new(Box::new(on_text));
        let callback = Box::into_raw(boxed);
        let session = unsafe { apple_speech_stream_begin(callback.cast(), trampoline) };
        if session.is_null() {
            drop(unsafe { Box::from_raw(callback) });
            return Err("Apple speech recognition could not start a live session".into());
        }
        Ok(Self { session, callback })
    }

    pub fn feed(&self, samples: &[f32]) {
        unsafe { apple_speech_stream_feed(self.session, samples.as_ptr(), samples.len() as isize) }
    }

    /// Finalizes the session and returns the full transcript.
    pub fn finish(mut self) -> Result<String, String> {
        let session = std::mem::replace(&mut self.session, std::ptr::null_mut());
        take_result(unsafe { apple_speech_stream_finish(session) })
    }
}

impl Drop for AppleStream {
    fn drop(&mut self) {
        if !self.session.is_null() {
            unsafe { apple_speech_stream_cancel(self.session) };
        }
        // Both finish and cancel wait for Swift's result loop, so no callback
        // can still be running here.
        drop(unsafe { Box::from_raw(self.callback) });
    }
}

extern "C" fn trampoline(context: *mut c_void, committed: *const c_char, tentative: *const c_char) {
    let callback = unsafe { &*(context as *const Box<dyn Fn(&str, &str) + Send + Sync>) };
    let committed = unsafe { lossy(committed) };
    let tentative = unsafe { lossy(tentative) };
    callback(&committed, &tentative);
}

unsafe fn lossy(ptr: *const c_char) -> String {
    if ptr.is_null() {
        String::new()
    } else {
        CStr::from_ptr(ptr).to_string_lossy().into_owned()
    }
}

fn take_result(ptr: *mut AppleSpeechResult) -> Result<String, String> {
    if ptr.is_null() {
        return Err("Null response from Apple speech recognition".into());
    }
    let result = unsafe { &*ptr };
    let outcome = if result.success == 1 {
        Ok(unsafe { lossy(result.text) })
    } else {
        let message = unsafe { lossy(result.error_message) };
        Err(if message.is_empty() {
            "Unknown Apple speech recognition error".into()
        } else {
            message
        })
    };
    unsafe { apple_speech_free_result(ptr) };
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Synthesizes speech with `say` and checks both paths transcribe it.
    /// Skips where the engine or `say` is unavailable (CI, older macOS).
    #[test]
    fn transcribes_synthesized_speech() {
        if !is_available() {
            eprintln!("Apple speech recognition unavailable; skipping");
            return;
        }
        let dir = std::env::temp_dir().join("handy-apple-speech-test");
        std::fs::create_dir_all(&dir).unwrap();
        let wav = dir.join("hello.wav");
        let said = std::process::Command::new("say")
            .args(["-o"])
            .arg(&wav)
            .args([
                "--file-format=WAVE",
                "--data-format=LEF32@16000",
                "Please send me the deck before the meeting tomorrow",
            ])
            .status();
        if !matches!(said, Ok(s) if s.success()) {
            eprintln!("`say` unavailable; skipping");
            return;
        }
        let samples: Vec<f32> = hound::WavReader::open(&wav)
            .unwrap()
            .into_samples::<f32>()
            .map(Result::unwrap)
            .collect();

        prepare().expect("prepare");
        let batch = transcribe(&samples).expect("batch transcribe");
        assert!(batch.to_lowercase().contains("meeting"), "batch: {batch}");

        let partials = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let seen = partials.clone();
        let stream = AppleStream::begin(move |_, _| {
            seen.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        })
        .expect("begin stream");
        for chunk in samples.chunks(1600) {
            stream.feed(chunk);
        }
        let live = stream.finish().expect("finish stream");
        assert!(live.to_lowercase().contains("meeting"), "stream: {live}");
        assert!(partials.load(std::sync::atomic::Ordering::Relaxed) > 0);
    }
}
