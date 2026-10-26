//! Platform-free Hushflow code shared by the desktop and mobile apps: the
//! meeting store and the meeting-notes prompt, citation check and title.
//!
//! No Tauri, no audio, no model runtime: anything here must build for
//! macOS, Windows, Linux, Android and iOS.

pub mod notes;
pub mod store;

pub use store::{MeetingDetail, MeetingSegment, MeetingSummary, Store};
