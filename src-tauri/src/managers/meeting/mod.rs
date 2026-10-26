//! Meeting notes: records the microphone ("Me") and the computer's audio
//! ("Them") as two tracks, transcribes each utterance as the meeting goes,
//! and after it ends writes notes from the transcript and the user's own
//! notes. The verbatim transcript is always kept; see docs/fork/NOTES_SPEC.md.
//!
//! Dictation goes first: utterances wait while the user dictates, so live
//! captions can lag but nothing is lost.

mod capture;
pub mod notes;
pub mod store;

use crate::managers::audio::create_detector;
use crate::managers::cleanup::CleanupManager;
use crate::managers::transcription::TranscriptionManager;
use crate::settings::get_settings;
use anyhow::{anyhow, Result};
use capture::{Audio, MicCapture, Segmenter, SystemCapture};
use log::{error, info, warn};
use serde::Serialize;
use specta::Type;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;
use store::{
    MeetingDetail, MeetingSegment, MeetingSummary, Store, CHANNEL_MIC, CHANNEL_SYSTEM,
    STATUS_READY, STATUS_SUMMARIZING,
};
use tauri::{AppHandle, Emitter, Manager};

const RATE: u32 = 16_000;
const DICTATION_POLL: Duration = Duration::from_millis(200);

/// A new line of transcript, for the live view.
#[derive(Debug, Clone, Serialize, Type)]
pub struct MeetingSegmentEvent {
    pub meeting_id: i64,
    pub segment: MeetingSegment,
    /// Earlier mic lines this one showed were echo, now hidden.
    pub suppressed: Vec<i64>,
}

/// The meeting being recorded, if any.
#[derive(Debug, Clone, Serialize, Type)]
pub struct ActiveMeeting {
    pub id: i64,
    pub started_at: i64,
    pub has_system_audio: bool,
}

struct Active {
    info: ActiveMeeting,
    mic: MicCapture,
    system: Option<SystemCapture>,
    threads: Vec<JoinHandle<()>>,
}

pub struct MeetingManager {
    app: AppHandle,
    dir: PathBuf,
    store: Arc<Store>,
    active: Mutex<Option<Active>>,
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

type Utterance = (&'static str, usize, Vec<f32>);

impl MeetingManager {
    pub fn new(app: &AppHandle) -> Result<Self> {
        let dir = crate::portable::app_data_dir(app)?.join("meetings");
        std::fs::create_dir_all(&dir)?;
        let store = Store::open(&dir.join("meetings.db"))?;
        Ok(Self {
            app: app.clone(),
            dir,
            store: Arc::new(store),
            active: Mutex::new(None),
        })
    }

    fn changed(&self, id: i64) {
        let _ = self.app.emit("meeting-changed", id);
    }

    pub fn active(&self) -> Option<ActiveMeeting> {
        self.active.lock().unwrap().as_ref().map(|a| a.info.clone())
    }

    pub fn start(&self) -> Result<ActiveMeeting> {
        let mut active = self.active.lock().unwrap();
        if let Some(a) = active.as_ref() {
            return Ok(a.info.clone());
        }
        let settings = get_settings(&self.app);
        let started_at = now_ms();
        let stem = format!("meeting-{started_at}");
        let mic_path = self.dir.join(format!("{stem}-mic.wav"));
        let system_path = self.dir.join(format!("{stem}-system.wav"));

        let (mic, mic_audio) = MicCapture::start(settings.selected_microphone.as_deref())?;
        let system = match SystemCapture::start() {
            Ok(system) => Some(system),
            Err(e) => {
                warn!("Meeting without system audio (mic only): {e}");
                None
            }
        };
        let title = chrono::Local::now()
            .format("Meeting, %b %-d at %-I:%M %p")
            .to_string();
        let id = self.store.create(
            &title,
            started_at,
            &mic_path.to_string_lossy(),
            system
                .as_ref()
                .map(|_| system_path.to_string_lossy())
                .as_deref(),
        )?;

        // Warm the speech model now so the first line isn't late.
        if let Some(tm) = self.app.try_state::<Arc<TranscriptionManager>>() {
            tm.initiate_model_load();
        }

        let (utterances, queue) = mpsc::channel::<Utterance>();
        let mut threads =
            vec![self.pipeline(CHANNEL_MIC, mic_audio, &mic_path, utterances.clone())?];
        let (system, system_audio) = match system {
            Some((system, audio)) => (Some(system), Some(audio)),
            None => (None, None),
        };
        if let Some(audio) = system_audio {
            threads.push(self.pipeline(CHANNEL_SYSTEM, audio, &system_path, utterances)?);
        } else {
            drop(utterances);
        }
        threads.push(self.transcriber(id, queue)?);

        let info = ActiveMeeting {
            id,
            started_at,
            has_system_audio: system.is_some(),
        };
        info!(
            "Meeting {id} started (system audio: {})",
            info.has_system_audio
        );
        *active = Some(Active {
            info: info.clone(),
            mic,
            system,
            threads,
        });
        drop(active);
        self.changed(id);
        Ok(info)
    }

    /// One channel: writes its track and cuts it into utterances.
    fn pipeline(
        &self,
        channel: &'static str,
        audio: Audio,
        path: &Path,
        out: Sender<Utterance>,
    ) -> Result<JoinHandle<()>> {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: RATE,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut wav = hound::WavWriter::create(path, spec)?;
        let scorer = create_detector(get_settings(&self.app).vad_backend, &self.app)?;
        Ok(std::thread::Builder::new()
            .name(format!("meeting-{channel}"))
            .spawn(move || {
                let mut segmenter = Segmenter::new(scorer);
                for chunk in audio {
                    for &s in &chunk {
                        let _ = wav.write_sample((s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16);
                    }
                    for (start, samples) in segmenter.push(&chunk) {
                        let _ = out.send((channel, start, samples));
                    }
                }
                if let Some((start, samples)) = segmenter.finish() {
                    let _ = out.send((channel, start, samples));
                }
                if let Err(e) = wav.finalize() {
                    warn!("Meeting {channel} track not finalized: {e}");
                }
            })?)
    }

    /// Transcribes utterances in order, after any dictation in progress.
    fn transcriber(
        &self,
        meeting_id: i64,
        queue: mpsc::Receiver<Utterance>,
    ) -> Result<JoinHandle<()>> {
        let app = self.app.clone();
        let store = Arc::clone(&self.store);
        Ok(std::thread::Builder::new()
            .name("meeting-transcribe".into())
            .spawn(move || {
                let Some(tm) = app
                    .try_state::<Arc<TranscriptionManager>>()
                    .map(|t| Arc::clone(&t))
                else {
                    return;
                };
                for (channel, start, samples) in queue {
                    while crate::tray::dictation_in_progress(&app) {
                        std::thread::sleep(DICTATION_POLL);
                    }
                    let t0_ms = (start as i64) * 1000 / RATE as i64;
                    let t1_ms = t0_ms + (samples.len() as i64) * 1000 / RATE as i64;
                    let text = match tm.transcribe(samples) {
                        Ok(text) => text.trim().to_string(),
                        Err(e) => {
                            warn!("Meeting utterance at {t0_ms} ms not transcribed: {e}");
                            continue;
                        }
                    };
                    if text.is_empty() {
                        continue;
                    }
                    match store.add_segment(meeting_id, channel, t0_ms, t1_ms, &text) {
                        Ok((segment, suppressed)) => {
                            let _ = app.emit(
                                "meeting-segment",
                                MeetingSegmentEvent {
                                    meeting_id,
                                    segment,
                                    suppressed,
                                },
                            );
                        }
                        Err(e) => error!("Meeting segment not saved: {e}"),
                    }
                }
            })?)
    }

    /// Ends the recording. The transcript's last lines and the notes finish
    /// in the background; `meeting-changed` says when.
    pub fn stop(self: &Arc<Self>) -> Result<i64> {
        let active = self
            .active
            .lock()
            .unwrap()
            .take()
            .ok_or_else(|| anyhow!("no meeting is recording"))?;
        let id = active.info.id;
        active.mic.stop();
        if let Some(system) = active.system {
            system.stop();
        }
        let _ = self.store.set_ended(id, now_ms());
        let _ = self.store.set_status(id, STATUS_SUMMARIZING, None);
        self.changed(id);
        info!("Meeting {id} stopped");
        let manager = Arc::clone(self);
        let threads = active.threads;
        std::thread::spawn(move || {
            // Every utterance is transcribed before the notes are written.
            for thread in threads {
                let _ = thread.join();
            }
            tauri::async_runtime::block_on(manager.write_notes(id));
        });
        Ok(id)
    }

    async fn write_notes(&self, id: i64) {
        let result = async {
            let cleanup = self
                .app
                .try_state::<Arc<CleanupManager>>()
                .ok_or_else(|| anyhow!("local models unavailable"))?;
            let segments = self.store.segments(id)?;
            let user_notes = self.store.user_notes(id)?;
            notes::generate(&cleanup, &segments, &user_notes).await
        }
        .await;
        match result {
            Ok((body, model)) => {
                let _ = self.store.add_ai_notes(id, &model, &body, now_ms());
                if let Some(title) = notes::title_of(&body) {
                    let _ = self.store.rename(id, &title);
                }
                let _ = self.store.set_status(id, STATUS_READY, None);
            }
            Err(e) => {
                warn!("Meeting {id}: notes not written: {e}");
                let _ = self
                    .store
                    .set_status(id, STATUS_READY, Some(&e.to_string()));
            }
        }
        self.changed(id);
    }

    /// Writes a new version of the notes; earlier versions stay.
    pub fn regenerate(self: &Arc<Self>, id: i64) -> Result<()> {
        if self.active().is_some_and(|a| a.id == id) {
            return Err(anyhow!("the meeting is still recording"));
        }
        self.store.set_status(id, STATUS_SUMMARIZING, None)?;
        self.changed(id);
        let manager = Arc::clone(self);
        tauri::async_runtime::spawn(async move { manager.write_notes(id).await });
        Ok(())
    }

    pub fn list(&self, query: Option<&str>) -> Result<Vec<MeetingSummary>> {
        self.store.list(query)
    }

    pub fn detail(&self, id: i64) -> Result<Option<MeetingDetail>> {
        self.store.detail(id)
    }

    pub fn save_user_notes(&self, id: i64, body: &str) -> Result<()> {
        self.store.save_user_notes(id, body, now_ms())
    }

    pub fn rename(&self, id: i64, title: &str) -> Result<()> {
        self.store.rename(id, title.trim())?;
        self.changed(id);
        Ok(())
    }

    pub fn delete(&self, id: i64) -> Result<()> {
        if self.active().is_some_and(|a| a.id == id) {
            return Err(anyhow!("stop the meeting before deleting it"));
        }
        for path in self.store.delete(id)? {
            let _ = std::fs::remove_file(path);
        }
        self.changed(id);
        Ok(())
    }

    /// The latest notes, the user's notes and the full transcript.
    pub fn markdown(&self, id: i64) -> Result<String> {
        let detail = self
            .store
            .detail(id)?
            .ok_or_else(|| anyhow!("no such meeting"))?;
        let mut md = format!("# {}\n\n", detail.meeting.title);
        md.push_str(
            &chrono::DateTime::from_timestamp_millis(detail.meeting.started_at)
                .map(|t| {
                    t.with_timezone(&chrono::Local)
                        .format("%A, %B %-d, %Y at %-I:%M %p\n\n")
                        .to_string()
                })
                .unwrap_or_default(),
        );
        if let Some(latest) = detail.ai_notes.first() {
            // The notes' own title would repeat the heading.
            let body: Vec<&str> = latest
                .body_md
                .lines()
                .skip_while(|l| l.starts_with("# ") || l.trim().is_empty())
                .collect();
            md.push_str(&body.join("\n"));
            md.push_str("\n\n");
        }
        if !detail.user_notes.trim().is_empty() {
            md.push_str("## My notes\n\n");
            md.push_str(detail.user_notes.trim());
            md.push_str("\n\n");
        }
        md.push_str("## Transcript\n\n");
        for line in notes::transcript_lines(&detail.segments) {
            md.push_str(&line);
            md.push_str("  \n");
        }
        Ok(md)
    }

    /// On quit: a meeting still recording keeps what it has.
    pub fn stop_now(self: &Arc<Self>) {
        if self.active().is_some() {
            let _ = self.stop();
        }
    }
}
