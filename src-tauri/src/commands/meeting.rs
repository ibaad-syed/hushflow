//! Meeting notes: start/stop, the library, notes and export.

use crate::managers::meeting::store::{MeetingDetail, MeetingSummary};
use crate::managers::meeting::{ActiveMeeting, MeetingManager};
use crate::settings::{get_settings, write_settings};
use std::sync::Arc;
use tauri::{AppHandle, State};

type Meetings<'a> = State<'a, Arc<MeetingManager>>;

#[tauri::command]
#[specta::specta]
pub fn start_meeting(meetings: Meetings<'_>) -> Result<ActiveMeeting, String> {
    meetings.start().map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn stop_meeting(meetings: Meetings<'_>) -> Result<i64, String> {
    meetings.stop().map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn get_active_meeting(meetings: Meetings<'_>) -> Option<ActiveMeeting> {
    meetings.active()
}

#[tauri::command]
#[specta::specta]
pub fn list_meetings(
    meetings: Meetings<'_>,
    query: Option<String>,
) -> Result<Vec<MeetingSummary>, String> {
    meetings.list(query.as_deref()).map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn get_meeting(meetings: Meetings<'_>, id: i64) -> Result<Option<MeetingDetail>, String> {
    meetings.detail(id).map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn save_meeting_notes(meetings: Meetings<'_>, id: i64, body: String) -> Result<(), String> {
    meetings
        .save_user_notes(id, &body)
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn rename_meeting(meetings: Meetings<'_>, id: i64, title: String) -> Result<(), String> {
    meetings.rename(id, &title).map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn regenerate_meeting_notes(meetings: Meetings<'_>, id: i64) -> Result<(), String> {
    meetings.regenerate(id).map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn delete_meeting(meetings: Meetings<'_>, id: i64) -> Result<(), String> {
    meetings.delete(id).map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn meeting_markdown(meetings: Meetings<'_>, id: i64) -> Result<String, String> {
    meetings.markdown(id).map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn accept_meeting_prompt(meetings: Meetings<'_>) -> Result<ActiveMeeting, String> {
    meetings.accept_prompt().map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn dismiss_meeting_prompt(meetings: Meetings<'_>) {
    meetings.dismiss_prompt();
}

#[tauri::command]
#[specta::specta]
pub fn set_meeting_detection(app: AppHandle, enabled: bool) {
    let mut settings = get_settings(&app);
    settings.meeting_detection = enabled;
    write_settings(&app, settings);
}
