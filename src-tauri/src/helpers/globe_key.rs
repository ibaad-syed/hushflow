//! macOS's Globe (Fn) key has its own system action, set under Keyboard ›
//! "Press 🌐 key to". Unless it is "Do Nothing", holding Fn to dictate also
//! opens the emoji picker, switches input source, or starts Apple dictation.

/// `AppleFnUsageType` values: 0 Do Nothing, 1 Change Input Source,
/// 2 Show Emoji & Symbols, 3 Start Dictation. Unset reads as the system
/// default, which is never "Do Nothing".
fn conflicts(fn_usage_type: Option<i64>) -> bool {
    fn_usage_type != Some(0)
}

/// Whether the Globe key's own action fires alongside an Fn shortcut.
#[tauri::command]
#[specta::specta]
pub fn globe_key_has_system_action() -> bool {
    #[cfg(target_os = "macos")]
    {
        let value = std::process::Command::new("defaults")
            .args(["read", "com.apple.HIToolbox", "AppleFnUsageType"])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse().ok());
        conflicts(value)
    }
    #[cfg(not(target_os = "macos"))]
    false
}

#[cfg(test)]
mod tests {
    use super::conflicts;

    #[test]
    fn only_do_nothing_is_free_of_conflicts() {
        assert!(!conflicts(Some(0)));
        assert!(conflicts(Some(2)));
        assert!(conflicts(None));
    }
}
