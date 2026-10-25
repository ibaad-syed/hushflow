//! Automatic transcription-engine selection.
//!
//! In [`TranscriptionEngineMode::Automatic`], a new user can dictate at once on
//! the OS's built-in engine (Apple's SpeechAnalyzer on macOS 26+) while the
//! default Parakeet model downloads in the background; once it is on disk and
//! the user isn't mid-dictation, the app switches to it. Without a built-in
//! engine the download is the only way in, and the switch happens the same way.

use crate::catalog;
use crate::commands::models::switch_active_model;
use crate::managers::audio::AudioRecordingManager;
use crate::managers::model::{system_speech_available, ModelManager, APPLE_SPEECH_MODEL_ID};
use crate::managers::transcription::TranscriptionManager;
use crate::settings::{get_settings, TranscriptionEngineMode};
use log::{info, warn};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use tauri::{AppHandle, Manager};

/// What Automatic mode should do, given where things stand.
#[derive(Debug, PartialEq, Eq)]
pub enum Plan {
    /// Use this model now.
    Select(&'static str),
    /// Use the built-in engine now and fetch the default model to switch to.
    SelectAndUpgrade {
        now: &'static str,
        later: &'static str,
    },
    /// Nothing usable yet: fetch the default model, then select it.
    DownloadThenSelect(&'static str),
    /// Already on the best available engine.
    Stay,
}

pub fn plan(
    default_model: &'static str,
    default_ready: bool,
    system_available: bool,
    selected: &str,
) -> Plan {
    if default_ready {
        return if selected == default_model {
            Plan::Stay
        } else {
            Plan::Select(default_model)
        };
    }
    if system_available && selected != APPLE_SPEECH_MODEL_ID {
        return Plan::SelectAndUpgrade {
            now: APPLE_SPEECH_MODEL_ID,
            later: default_model,
        };
    }
    Plan::DownloadThenSelect(default_model)
}

/// Only one policy run (and so one background download) at a time.
static RUNNING: AtomicBool = AtomicBool::new(false);

/// Applies Automatic mode in the background. A no-op in Manual mode or while
/// a previous run is still going.
pub fn apply(app: &AppHandle) {
    if get_settings(app).transcription_engine_mode != TranscriptionEngineMode::Automatic {
        return;
    }
    if RUNNING.swap(true, Ordering::AcqRel) {
        return;
    }
    let app = app.clone();
    thread::spawn(move || {
        run(&app);
        RUNNING.store(false, Ordering::Release);
    });
}

fn run(app: &AppHandle) {
    let Some(default_model) = catalog::default_model_id() else {
        warn!("Automatic engine: no default model in the catalog");
        return;
    };
    let model_manager = app.state::<Arc<ModelManager>>();
    let default_ready = model_manager
        .get_model_info(default_model)
        .is_some_and(|m| m.is_downloaded);
    let selected = get_settings(app).selected_model;
    let step = plan(
        default_model,
        default_ready,
        system_speech_available(),
        &selected,
    );
    info!("Automatic engine: {step:?} (selected '{selected}')");

    match step {
        Plan::Stay => {}
        Plan::Select(model) => select_when_idle(app, model),
        Plan::SelectAndUpgrade { now, later } => {
            select_when_idle(app, now);
            download_then_select(app, later);
        }
        Plan::DownloadThenSelect(model) => download_then_select(app, model),
    }
}

fn download_then_select(app: &AppHandle, model: &str) {
    let model_manager = app.state::<Arc<ModelManager>>().inner().clone();
    info!("Automatic engine: downloading '{model}' in the background");
    if let Err(e) = tauri::async_runtime::block_on(model_manager.download_model(model)) {
        warn!("Automatic engine: download of '{model}' failed: {e}");
        return;
    }
    // The user may have picked a model themselves while this downloaded.
    if get_settings(app).transcription_engine_mode != TranscriptionEngineMode::Automatic {
        info!("Automatic engine: switched to Manual during download; not switching");
        return;
    }
    select_when_idle(app, model);
}

/// Switches models only between dictations, so a recording in flight always
/// finishes on the engine it started with.
fn select_when_idle(app: &AppHandle, model: &str) {
    const POLL: Duration = Duration::from_millis(500);
    const ATTEMPTS: u32 = 20;

    let audio = app.state::<Arc<AudioRecordingManager>>().inner().clone();
    let transcription = app.state::<Arc<TranscriptionManager>>().inner().clone();
    for _ in 0..ATTEMPTS {
        let busy =
            audio.is_recording() || transcription.job_active() || transcription.is_streaming();
        if !busy {
            match switch_active_model(app, model) {
                Ok(()) => {
                    info!("Automatic engine: now using '{model}'");
                    return;
                }
                // A load already in progress (e.g. the one a dictation kicked
                // off) clears on its own; anything else won't.
                Err(e) if e.contains("already in progress") => {}
                Err(e) => {
                    warn!("Automatic engine: could not switch to '{model}': {e}");
                    return;
                }
            }
        }
        thread::sleep(POLL);
    }
    warn!("Automatic engine: gave up switching to '{model}' after waiting for idle");
}

#[cfg(test)]
mod tests {
    use super::*;

    const PARAKEET: &str = "handy-computer/parakeet-unified-en-0.6b-gguf/x.gguf";

    #[test]
    fn new_mac_starts_on_apple_and_upgrades() {
        assert_eq!(
            plan(PARAKEET, false, true, ""),
            Plan::SelectAndUpgrade {
                now: APPLE_SPEECH_MODEL_ID,
                later: PARAKEET
            }
        );
    }

    #[test]
    fn already_on_apple_only_downloads() {
        assert_eq!(
            plan(PARAKEET, false, true, APPLE_SPEECH_MODEL_ID),
            Plan::DownloadThenSelect(PARAKEET)
        );
    }

    #[test]
    fn downloaded_parakeet_takes_over() {
        assert_eq!(
            plan(PARAKEET, true, true, APPLE_SPEECH_MODEL_ID),
            Plan::Select(PARAKEET)
        );
        assert_eq!(plan(PARAKEET, true, true, PARAKEET), Plan::Stay);
    }

    #[test]
    fn without_a_builtin_engine_parakeet_is_fetched() {
        assert_eq!(
            plan(PARAKEET, false, false, ""),
            Plan::DownloadThenSelect(PARAKEET)
        );
    }
}
