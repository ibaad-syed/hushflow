//! Meetings, their transcript segments, the user's notes and every version of
//! the AI notes, in their own SQLite file.
//!
//! Copied from the desktop's `src-tauri/src/managers/meeting/store.rs`;
//! keep the two in step until the desktop depends on this crate.

use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use rusqlite_migration::{Migrations, M};
use serde::Serialize;
use std::path::{Path, PathBuf};

static MIGRATIONS: &[M] = &[M::up(
    "CREATE TABLE meetings (
        id INTEGER PRIMARY KEY,
        title TEXT NOT NULL,
        started_at INTEGER NOT NULL,
        ended_at INTEGER,
        status TEXT NOT NULL,
        mic_audio_path TEXT,
        system_audio_path TEXT,
        has_system_audio BOOLEAN NOT NULL DEFAULT 0,
        notes_error TEXT
    );
    CREATE TABLE meeting_segments (
        id INTEGER PRIMARY KEY,
        meeting_id INTEGER NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
        channel TEXT NOT NULL,
        t0_ms INTEGER NOT NULL,
        t1_ms INTEGER NOT NULL,
        text TEXT NOT NULL,
        suppressed_echo BOOLEAN NOT NULL DEFAULT 0
    );
    CREATE INDEX meeting_segments_by_time ON meeting_segments(meeting_id, t0_ms);
    CREATE TABLE meeting_user_notes (
        meeting_id INTEGER PRIMARY KEY REFERENCES meetings(id) ON DELETE CASCADE,
        body TEXT NOT NULL,
        updated_at INTEGER NOT NULL
    );
    CREATE TABLE meeting_ai_notes (
        id INTEGER PRIMARY KEY,
        meeting_id INTEGER NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
        model TEXT NOT NULL,
        body_md TEXT NOT NULL,
        created_at INTEGER NOT NULL
    );",
)];

/// Where a meeting is in its life: recording → summarizing → ready (or failed
/// to write notes, with the transcript still kept).
pub const STATUS_RECORDING: &str = "recording";
pub const STATUS_SUMMARIZING: &str = "summarizing";
pub const STATUS_READY: &str = "ready";

/// Which side of the call a segment came from.
pub const CHANNEL_MIC: &str = "mic";
pub const CHANNEL_SYSTEM: &str = "system";

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct MeetingSummary {
    pub id: i64,
    pub title: String,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    pub status: String,
    pub has_system_audio: bool,
    /// Why AI notes are missing, when they are.
    pub notes_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct MeetingSegment {
    pub id: i64,
    pub channel: String,
    pub t0_ms: i64,
    pub t1_ms: i64,
    pub text: String,
    /// The mic picked up the other side (no headphones): hidden, kept.
    pub suppressed_echo: bool,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct AiNotes {
    pub id: i64,
    pub model: String,
    pub body_md: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct MeetingDetail {
    pub meeting: MeetingSummary,
    pub segments: Vec<MeetingSegment>,
    pub user_notes: String,
    /// Every version, newest first.
    pub ai_notes: Vec<AiNotes>,
}

pub struct Store {
    path: PathBuf,
}

fn summary_from_row(row: &rusqlite::Row) -> rusqlite::Result<MeetingSummary> {
    Ok(MeetingSummary {
        id: row.get(0)?,
        title: row.get(1)?,
        started_at: row.get(2)?,
        ended_at: row.get(3)?,
        status: row.get(4)?,
        has_system_audio: row.get(5)?,
        notes_error: row.get(6)?,
    })
}

const SUMMARY_COLUMNS: &str =
    "id, title, started_at, ended_at, status, has_system_audio, notes_error";

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        let mut conn = Connection::open(path)?;
        Migrations::from_slice(MIGRATIONS).to_latest(&mut conn)?;
        // A crash mid-meeting leaves it "recording" forever otherwise.
        conn.execute(
            "UPDATE meetings SET status = ?1, ended_at = COALESCE(ended_at, started_at)
             WHERE status IN (?2, ?3)",
            params![STATUS_READY, STATUS_RECORDING, STATUS_SUMMARIZING],
        )?;
        Ok(Self {
            path: path.to_path_buf(),
        })
    }

    fn conn(&self) -> Result<Connection> {
        let conn = Connection::open(&self.path)?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        Ok(conn)
    }

    pub fn create(
        &self,
        title: &str,
        started_at: i64,
        mic_path: &str,
        system_path: Option<&str>,
    ) -> Result<i64> {
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO meetings (title, started_at, status, mic_audio_path, system_audio_path, has_system_audio)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                title,
                started_at,
                STATUS_RECORDING,
                mic_path,
                system_path,
                system_path.is_some()
            ],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn set_status(&self, id: i64, status: &str, notes_error: Option<&str>) -> Result<()> {
        self.conn()?.execute(
            "UPDATE meetings SET status = ?2, notes_error = ?3 WHERE id = ?1",
            params![id, status, notes_error],
        )?;
        Ok(())
    }

    pub fn set_ended(&self, id: i64, ended_at: i64) -> Result<()> {
        self.conn()?.execute(
            "UPDATE meetings SET ended_at = ?2 WHERE id = ?1",
            params![id, ended_at],
        )?;
        Ok(())
    }

    pub fn rename(&self, id: i64, title: &str) -> Result<()> {
        self.conn()?.execute(
            "UPDATE meetings SET title = ?2 WHERE id = ?1",
            params![id, title],
        )?;
        Ok(())
    }

    /// Adds a segment. A mic segment that repeats an overlapping system one
    /// (or the other way round) is the room echo of the remote side: the mic
    /// copy is marked, never deleted. Returns the stored segment and the ids of
    /// earlier mic segments this one marked.
    pub fn add_segment(
        &self,
        meeting_id: i64,
        channel: &str,
        t0_ms: i64,
        t1_ms: i64,
        text: &str,
    ) -> Result<(MeetingSegment, Vec<i64>)> {
        let conn = self.conn()?;
        let other = if channel == CHANNEL_MIC {
            CHANNEL_SYSTEM
        } else {
            CHANNEL_MIC
        };
        // Echo arrives with a little delay, so allow some slack around it.
        let mut stmt = conn.prepare(
            "SELECT id, text FROM meeting_segments
             WHERE meeting_id = ?1 AND channel = ?2 AND t0_ms <= ?4 + 1500 AND t1_ms >= ?3 - 1500",
        )?;
        let overlapping: Vec<(i64, String)> = stmt
            .query_map(params![meeting_id, other, t0_ms, t1_ms], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })?
            .collect::<rusqlite::Result<_>>()?;
        let echoes: Vec<i64> = overlapping
            .iter()
            .filter(|(_, other_text)| is_echo(text, other_text))
            .map(|(id, _)| *id)
            .collect();
        let suppressed = channel == CHANNEL_MIC && !echoes.is_empty();
        conn.execute(
            "INSERT INTO meeting_segments (meeting_id, channel, t0_ms, t1_ms, text, suppressed_echo)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![meeting_id, channel, t0_ms, t1_ms, text, suppressed],
        )?;
        let id = conn.last_insert_rowid();
        let mut marked = Vec::new();
        if channel == CHANNEL_SYSTEM {
            for mic_id in echoes {
                conn.execute(
                    "UPDATE meeting_segments SET suppressed_echo = 1 WHERE id = ?1",
                    params![mic_id],
                )?;
                marked.push(mic_id);
            }
        }
        Ok((
            MeetingSegment {
                id,
                channel: channel.to_string(),
                t0_ms,
                t1_ms,
                text: text.to_string(),
                suppressed_echo: suppressed,
            },
            marked,
        ))
    }

    pub fn save_user_notes(&self, id: i64, body: &str, now: i64) -> Result<()> {
        self.conn()?.execute(
            "INSERT INTO meeting_user_notes (meeting_id, body, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(meeting_id) DO UPDATE SET body = excluded.body, updated_at = excluded.updated_at",
            params![id, body, now],
        )?;
        Ok(())
    }

    pub fn add_ai_notes(&self, id: i64, model: &str, body_md: &str, now: i64) -> Result<()> {
        self.conn()?.execute(
            "INSERT INTO meeting_ai_notes (meeting_id, model, body_md, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![id, model, body_md, now],
        )?;
        Ok(())
    }

    pub fn summary(&self, id: i64) -> Result<Option<MeetingSummary>> {
        Ok(self
            .conn()?
            .query_row(
                &format!("SELECT {SUMMARY_COLUMNS} FROM meetings WHERE id = ?1"),
                params![id],
                summary_from_row,
            )
            .optional()?)
    }

    /// Newest first. `query` matches titles, the user's notes, AI notes and
    /// the transcript.
    pub fn list(&self, query: Option<&str>) -> Result<Vec<MeetingSummary>> {
        let conn = self.conn()?;
        let query = query.map(str::trim).filter(|q| !q.is_empty());
        let rows = match query {
            None => {
                let mut stmt = conn.prepare(&format!(
                    "SELECT {SUMMARY_COLUMNS} FROM meetings ORDER BY started_at DESC"
                ))?;
                let rows = stmt
                    .query_map([], summary_from_row)?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                rows
            }
            Some(q) => {
                let like = format!("%{}%", q.replace('%', "\\%").replace('_', "\\_"));
                let mut stmt = conn.prepare(&format!(
                    "SELECT {SUMMARY_COLUMNS} FROM meetings m WHERE
                       m.title LIKE ?1 ESCAPE '\\'
                       OR EXISTS (SELECT 1 FROM meeting_user_notes n WHERE n.meeting_id = m.id AND n.body LIKE ?1 ESCAPE '\\')
                       OR EXISTS (SELECT 1 FROM meeting_ai_notes a WHERE a.meeting_id = m.id AND a.body_md LIKE ?1 ESCAPE '\\')
                       OR EXISTS (SELECT 1 FROM meeting_segments s WHERE s.meeting_id = m.id AND s.text LIKE ?1 ESCAPE '\\')
                     ORDER BY started_at DESC"
                ))?;
                let rows = stmt
                    .query_map(params![like], summary_from_row)?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                rows
            }
        };
        Ok(rows)
    }

    pub fn segments(&self, id: i64) -> Result<Vec<MeetingSegment>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, channel, t0_ms, t1_ms, text, suppressed_echo FROM meeting_segments
             WHERE meeting_id = ?1 ORDER BY t0_ms, id",
        )?;
        let rows = stmt
            .query_map(params![id], |row| {
                Ok(MeetingSegment {
                    id: row.get(0)?,
                    channel: row.get(1)?,
                    t0_ms: row.get(2)?,
                    t1_ms: row.get(3)?,
                    text: row.get(4)?,
                    suppressed_echo: row.get(5)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn user_notes(&self, id: i64) -> Result<String> {
        Ok(self
            .conn()?
            .query_row(
                "SELECT body FROM meeting_user_notes WHERE meeting_id = ?1",
                params![id],
                |row| row.get(0),
            )
            .optional()?
            .unwrap_or_default())
    }

    pub fn detail(&self, id: i64) -> Result<Option<MeetingDetail>> {
        let Some(meeting) = self.summary(id)? else {
            return Ok(None);
        };
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, model, body_md, created_at FROM meeting_ai_notes
             WHERE meeting_id = ?1 ORDER BY created_at DESC, id DESC",
        )?;
        let ai_notes = stmt
            .query_map(params![id], |row| {
                Ok(AiNotes {
                    id: row.get(0)?,
                    model: row.get(1)?,
                    body_md: row.get(2)?,
                    created_at: row.get(3)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(Some(MeetingDetail {
            meeting,
            segments: self.segments(id)?,
            user_notes: self.user_notes(id)?,
            ai_notes,
        }))
    }

    /// Removes the meeting and returns its audio files, for the caller to delete.
    pub fn delete(&self, id: i64) -> Result<Vec<String>> {
        let conn = self.conn()?;
        let paths: Option<(Option<String>, Option<String>)> = conn
            .query_row(
                "SELECT mic_audio_path, system_audio_path FROM meetings WHERE id = ?1",
                params![id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        conn.execute("DELETE FROM meetings WHERE id = ?1", params![id])?;
        Ok(paths
            .map(|(a, b)| a.into_iter().chain(b).collect())
            .unwrap_or_default())
    }
}

fn words(text: &str) -> String {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric() && c != '\'')
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Two transcripts of the same speech: the mic heard the speakers.
fn is_echo(a: &str, b: &str) -> bool {
    let (a, b) = (words(a), words(b));
    if a.is_empty() || b.is_empty() {
        return false;
    }
    // A short mic line inside a longer system one (or vice versa) is an echo
    // of part of it.
    let (short, long) = if a.len() <= b.len() {
        (&a, &b)
    } else {
        (&b, &a)
    };
    if short.len() >= 12 && long.contains(short.as_str()) {
        return true;
    }
    strsim::normalized_levenshtein(&a, &b) >= 0.7
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(&dir.path().join("meetings.db")).unwrap();
        (dir, store)
    }

    #[test]
    fn echo_of_the_remote_side_is_marked_on_the_mic_copy() {
        let (_dir, store) = store();
        let id = store.create("Test", 0, "mic.wav", Some("sys.wav")).unwrap();
        let (mic, _) = store
            .add_segment(id, CHANNEL_MIC, 1000, 4000, "Let's ship it on Tuesday.")
            .unwrap();
        assert!(!mic.suppressed_echo);
        let (_, marked) = store
            .add_segment(id, CHANNEL_SYSTEM, 900, 4100, "Let's ship it on Tuesday")
            .unwrap();
        assert_eq!(marked, vec![mic.id]);
        // Arriving the other way round, the mic copy is marked at once.
        let (late_mic, _) = store
            .add_segment(id, CHANNEL_MIC, 950, 3900, "lets ship it on tuesday")
            .unwrap();
        assert!(late_mic.suppressed_echo);
        // Different words at the same time are two people talking.
        let (other, _) = store
            .add_segment(
                id,
                CHANNEL_MIC,
                1000,
                4000,
                "Sounds good to me, I'll tell QA.",
            )
            .unwrap();
        assert!(!other.suppressed_echo);
    }

    #[test]
    fn search_finds_transcript_and_notes_and_delete_cascades() {
        let (_dir, store) = store();
        let a = store.create("Standup", 1, "a.wav", None).unwrap();
        let b = store.create("Planning", 2, "b.wav", None).unwrap();
        store
            .add_segment(a, CHANNEL_MIC, 0, 1000, "The Kubernetes upgrade is done")
            .unwrap();
        store.save_user_notes(b, "budget 50%", 3).unwrap();
        let hits = |q| {
            store
                .list(Some(q))
                .unwrap()
                .iter()
                .map(|m| m.id)
                .collect::<Vec<_>>()
        };
        assert_eq!(hits("kubernetes"), vec![a]);
        assert_eq!(hits("50%"), vec![b]);
        assert_eq!(store.list(None).unwrap().len(), 2);
        assert_eq!(store.delete(a).unwrap(), vec!["a.wav".to_string()]);
        assert!(store.segments(a).unwrap().is_empty());
        assert!(store.summary(a).unwrap().is_none());
    }
}
