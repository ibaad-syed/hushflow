import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

/** Mirrors hushflow-core's `MeetingSummary`. */
interface MeetingSummary {
  id: number;
  title: string;
  started_at: number;
  ended_at: number | null;
  status: string;
  has_system_audio: boolean;
  notes_error: string | null;
}

export default function App() {
  const [meetings, setMeetings] = useState<MeetingSummary[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    invoke<MeetingSummary[]>("list_meetings")
      .then(setMeetings)
      .catch((e) => setError(String(e)));
  }, []);

  return (
    <main>
      <h1>Hushflow</h1>
      <p className="tagline">Meetings, offline.</p>

      {/* Recording comes next; this only proves the shell works. */}
      <button type="button" className="start" onClick={() => {}}>
        Start
      </button>

      <h2>Meetings</h2>
      {error && <p className="error">Couldn't load meetings: {error}</p>}
      {!error && meetings === null && <p className="muted">Loading…</p>}
      {meetings?.length === 0 && <p className="muted">No meetings yet.</p>}
      {meetings && meetings.length > 0 && (
        <ul>
          {meetings.map((m) => (
            <li key={m.id}>
              <span>{m.title}</span>
              <span className="muted">
                {new Date(m.started_at).toLocaleString()}
              </span>
            </li>
          ))}
        </ul>
      )}
    </main>
  );
}
