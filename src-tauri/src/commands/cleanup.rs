//! Commands for the cleanup step (Settings and onboarding).

use crate::managers::cleanup::{catalog, CleanupManager, CleanupStatus};
use crate::settings::{get_settings, write_settings};
use std::sync::Arc;
use tauri::{AppHandle, State};

#[tauri::command]
#[specta::specta]
pub fn get_cleanup_status(cleanup: State<'_, Arc<CleanupManager>>) -> CleanupStatus {
    cleanup.status()
}

/// Turns cleanup on or off. Turning it on loads the chosen engine, starting
/// its download if needed; dictation keeps working meanwhile.
#[tauri::command]
#[specta::specta]
pub async fn set_cleanup_enabled(
    app: AppHandle,
    cleanup: State<'_, Arc<CleanupManager>>,
    enabled: bool,
) -> Result<(), String> {
    let mut settings = get_settings(&app);
    settings.cleanup_enabled = enabled;
    let engine = settings.cleanup_engine.clone();
    write_settings(&app, settings);
    let cleanup = Arc::clone(&cleanup);
    if enabled {
        if catalog::find(&engine).is_some() && !cleanup.is_downloaded(&engine) {
            cleanup.start_download(&engine).map_err(|e| e.to_string())?;
        } else {
            tauri::async_runtime::spawn(async move { cleanup.warm().await });
        }
    } else {
        cleanup.stop().await;
    }
    Ok(())
}

/// Picks the cleanup engine (and turns cleanup on), downloading it in the
/// background if it isn't on disk yet.
#[tauri::command]
#[specta::specta]
pub async fn set_cleanup_engine(
    app: AppHandle,
    cleanup: State<'_, Arc<CleanupManager>>,
    engine: String,
) -> Result<(), String> {
    if engine != catalog::APPLE_INTELLIGENCE && catalog::find(&engine).is_none() {
        return Err(format!("Unknown cleanup engine {engine}"));
    }
    let mut settings = get_settings(&app);
    settings.cleanup_engine = engine.clone();
    settings.cleanup_enabled = true;
    write_settings(&app, settings);
    let cleanup = Arc::clone(&cleanup);
    if catalog::find(&engine).is_some() && !cleanup.is_downloaded(&engine) {
        cleanup.start_download(&engine).map_err(|e| e.to_string())?;
    } else {
        tauri::async_runtime::spawn(async move { cleanup.warm().await });
    }
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn delete_cleanup_model(
    cleanup: State<'_, Arc<CleanupManager>>,
    id: String,
) -> Result<(), String> {
    cleanup.delete_model(&id).map_err(|e| e.to_string())
}
