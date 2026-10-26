//! Hushflow Mobile: record in-person meetings, transcribe them with the OS's
//! on-device recognizer and write notes with Gemma, all offline.
//!
//! For now this is the shell: it opens the meeting store from
//! `hushflow-core` in the app's data directory and lists its meetings.

use hushflow_core::{MeetingSummary, Store};
use tauri::Manager;

/// The meeting store, opened once at startup.
struct Meetings(Store);

/// Every meeting, newest first.
#[tauri::command]
fn list_meetings(meetings: tauri::State<'_, Meetings>) -> Result<Vec<MeetingSummary>, String> {
    meetings.0.list(None).map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            // Doesn't exist yet on a fresh install.
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            let store = Store::open(&dir.join("meetings.db"))?;
            app.manage(Meetings(store));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![list_meetings])
        .run(tauri::generate_context!())
        .expect("error while running Hushflow");
}
