//! macOS ties an Accessibility grant to the app's code signature. After an
//! update or rebuild the old entry can stay in System Settings, switched on,
//! while the new build is still untrusted, which leaves users stuck: "it's
//! already there". Clearing our own entry before asking again makes macOS add
//! a fresh one that the toggle actually applies to.

/// Removes this app's Accessibility entry (only ours) so the next request
/// starts clean. A no-op elsewhere.
#[tauri::command]
#[specta::specta]
pub fn reset_accessibility_permission(app: tauri::AppHandle) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let identifier = app.config().identifier.clone();
        let status = std::process::Command::new("tccutil")
            .args(["reset", "Accessibility", &identifier])
            .status()
            .map_err(|e| format!("Failed to run tccutil: {e}"))?;
        if !status.success() {
            return Err(format!("tccutil reset failed with {status}"));
        }
        log::info!("Cleared Accessibility entry for {identifier} before asking again");
    }
    #[cfg(not(target_os = "macos"))]
    let _ = app;
    Ok(())
}
