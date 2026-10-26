# Meeting notes: scope and implementation plan

Status: proposal, 2026-10-25. Research and design only; no code yet. Builds on `SPEC.md` (engines, cleanup server, guardrails).

## 1. What it does (user's view)

A notepad for calls that needs no bot in the meeting. The verbatim transcript is always kept, and the AI notes sit on top of it, never in its place.

1. **Start/stop.** The user starts it from the tray, a hotkey, or a "New meeting" button, and stops it the same way. Meeting detection, such as "Zoom is using your mic, take notes?", is **not** in the MVP (Phase 3).
2. **During the meeting.** A meeting window has two panes: a notepad where the user types rough bullets, and a live transcript labelled **Me** (microphone) and **Them** (system audio). Dictation hotkeys keep working, and dictation takes priority over meeting transcription.
3. **After stop.** (a) The canonical transcript, with timestamps and Me/Them labels, is finalized: live utterances become canonical, and any gaps are filled from the recorded audio. (b) **Enhanced notes:** the user's bullets are expanded and filled in from the transcript, Granola-style. If the user wrote nothing, a fixed template is used instead (Summary / Decisions / Action items / Open questions). Text the user typed and text the AI added are visually distinct.
4. **Always kept:** the verbatim speaker-labelled transcript, the user's own notes as typed, and every version of the enhanced notes (regenerating one never overwrites the others). Audio is kept by default and expires under the existing recording-retention setting (`RecordingRetentionPeriod`). It enables re-transcription and Phase 2 diarization. Granola keeps no audio, and reviewers name that as a drawback ([happyscribe.com/pl, accessed 2026-10-25](https://www.happyscribe.com/pl/blog/recenzja-granola-ai)).
5. **Library:** meetings get their own list, separate from dictation history, with full-text search over titles, notes and transcripts. Export as Markdown (enhanced notes, the user's notes, and the transcript with timestamps), or copy any section.
6. **Privacy:** everything runs on the device by default (speech-to-text, LLM, storage). The app warns once about consent: "Let people know you're taking notes; recording laws vary." System-audio permission is requested only when the user first starts a meeting.

## 2. What we learned (evidence that drives the decisions)

- **Granola** captures mic plus system audio on the device, with no bot. Its transcript shows **"Me and Them, corresponding to your microphone input and system audio"**. Real names come only from platform hooks (a Google Meet browser extension, and the Zoom and Teams desktop apps), not from acoustic diarization. It cannot tell apart several people sharing one room device ([Granola docs, speaker attribution, accessed 2026-10-25](https://docs.granola.ai/help-center/taking-notes/speaker-attribution.md)). The transcript is kept and searchable in a side panel: green bubbles for the mic, grey for system audio. Audio is never saved ([Granola docs, transcription](https://docs.granola.ai/help-center/taking-notes/transcription.md)). Its core flow is that the user types rough notes, then clicks **Enhance notes**, which applies a template ("Auto" by default) and merges in transcript detail ([Zapier, Jul 2025](https://zapier.com/blog/granola-ai/)). Reviewers praise the design and that no bot joins. They complain about speaker identification, about accuracy on names, and that there is no audio to check the notes against ([tl;dv review](https://tldv.io/blog/granola-review), [happyscribe](https://www.happyscribe.com/pl/blog/recenzja-granola-ai), accessed 2026-10-25).
- **Local alternatives.** Meetily (Tauri + Rust, Whisper/Parakeet, Ollama) is at v0.4.1 and still ships **without** diarization; that is "planned for PRO" ([GitHub README, accessed 2026-10-25](https://github.com/Zackriya-Solutions/meeting-minutes)). On macOS it captures system audio through ScreenCaptureKit, via the `cidre` crate ([pinggy](https://pinggy.io/blog/meetily_local_ai_meeting_assistant_pinggy/)). Hyprnote, now anarlog (MIT), stores sessions in SQLite, keeps recordings as local files, and exports Markdown ([GitHub, accessed 2026-10-25](https://github.com/fastrepl/hyprnote)).
- **This repo has no ONNX Runtime.** `Cargo.toml` has no `ort`, and all inference goes through `transcribe-cpp` (ggml, Metal/Vulkan). Two facts change the cost picture:
  - **`transcribe-cpp` 0.4.0 already has a diarization role** (`DiarizeSession`, `SortformerPreset`). handy-computer publishes `diar_streaming_sortformer_4spk-v2.1` as GGUF: Q8_0 is **139 MB**, with **14.73% DER on AMI IHM** (collar 0, overlap scored). Licence: NVIDIA Open Model License; ship its NOTICE ([HF card, accessed 2026-10-25](https://huggingface.co/handy-computer/diar_streaming_sortformer_4spk-v2.1-gguf)).
  - **`handy-cpal` 0.18.2**, which our `handy-recorder` already links, has macOS loopback through Core Audio **process taps** (`host/coreaudio/macos/loopback.rs`, "macOS > 14.6") and WASAPI loopback (`AUDCLNT_STREAMFLAGS_LOOPBACK`). Upstream cpal 0.18.2 is dated 2026-08-16.

### Diarization options

| Option | Licence | Size | Accuracy (published) | Speed | Platforms | Path from Rust | Verdict |
|---|---|---|---|---|---|---|---|
| **Mic = Me, system = Them** | n/a | 0 | Exact for Me vs Them, as long as there's no echo | free | all | channel tag | **MVP** |
| **Sortformer 4spk v2.1 via transcribe-cpp** | NVIDIA Open Model (commercial OK, ship the notice) | 139 MB Q8 | DER 14.6–14.7% AMI IHM, 10.1% CALLHOME, 38.9% DIHARD with ≥5 speakers ([NVIDIA card](https://huggingface.co/nvidia/diar_streaming_sortformer_4spk-v2.1)) | RTF 0.002 offline on an RTX 6000; Mac not yet measured | Metal/Vulkan/CPU, via the existing dependency | `Model::diarize_session()`, no new runtime | **Phase 2** |
| Nemotron-3-Diarization | OpenMDW-1.1 | ~100M params | 14.72% on Diarization-Bench (NVIDIA's own figure) | 0.32 s streaming | NeMo only today | wait for a transcribe.cpp port | Watch (released 2026-09-23, [unite.ai](https://www.unite.ai/nvidia-releases-nemotron-3-diarization-open-weight-speaker-model/)); supports 8 speakers |
| pyannote community-1 | CC-BY-4.0, gated on Hugging Face | ~30 MB | 17.0% AMI IHM, ~11.2% VoxConverse ([HF](https://huggingface.co/pyannote/speaker-diarization-community-1)) | 24× real time on an M4 Pro (Python) | needs ORT or CoreML | new runtime | No |
| speakrs 0.6.0 (2026-10-14) | not stated | HF bundle | 7.1% VoxConverse, collar 0 ([docs.rs](https://docs.rs/crate/speakrs/latest)) | 529× real time with CoreML on an M4 Pro | CoreML, CUDA, CPU; no Vulkan or DirectML | pure Rust crate | Fallback if we need more than 4 speakers; licence must be checked |
| sherpa-onnx (pyannote-seg-3 + embeddings) | Apache-2.0 | ~40 MB | depends on the model pair | fast on CPU | all | official `sherpa-onnx` crate, downloads a prebuilt ORT | No: adds a second inference runtime |
| FluidAudio | Apache-2.0 (models vary) | – | community-1 / Sortformer ports | ANE | Mac only, Swift | Swift bridge | No: Mac only |

## 3. MVP versus later phases

**Your MVP guess holds, with one refinement.** Granola, the category leader, still bases its labels on "mic = Me, system = Them", and Meetily ships with no diarization at all. Two channels give exact Me/Them labels for free and settle the hardest case, attributing the user's own speech, by construction. The refinement is that Granola's real product is **enhancing the user's own rough notes**. So the MVP's enhancement takes *user bullets + transcript* as input, and the fixed template is only the fallback when the notepad is empty.

**MVP (Phase 1)**
- Manual start/stop. Records mic and system audio as two separate 16 kHz mono tracks on disk. System audio is unavailable on macOS below 14.6, or when permission is denied, and the meeting then falls back to mic only with a banner.
- Live Me/Them transcript (§4.2): each utterance is a full Parakeet batch run and becomes canonical. Finalize fills gaps after stop, and redoes the whole meeting only when live ran on a different engine or the user asks.
- A notepad with plain text and bullets, autosaved.
- After stop: enhanced notes in one fixed template, merged with the user's notes, using the local LLM. Regenerate keeps every version. The title is generated.
- A meetings list, FTS5 search, and Markdown export. Audio retention follows the existing setting.
- Echo protection (§5, risk 1): a "use headphones" hint plus cross-channel suppression.

**Phase 2: who said what**
- Sortformer runs offline on the **system channel only**, with the `VeryHighLatency` preset, producing Them 1–4. "Me" stays exact. The user can rename a speaker once and the rename applies to the whole meeting. This phase is short, since the model, runtime and backends are already in the tree.
- **In-person mode** (mic only, several people): Sortformer on the mic channel.
- Re-diarize and re-transcribe old meetings from the stored audio.
- User-editable templates (1:1, standup, interview, sales call) and an "ask this meeting" chat.

**Phase 3**
- Meeting detection: another process starts using the mic, or Zoom/Teams/Meet is in front; offer a notification, never auto-record.
- Optional bigger notes model; bring-your-own-key summarization (reusing SPEC phase 3); names from the calendar.

## 4. Architecture

### 4.1 Audio capture
- **macOS: Core Audio process taps** through `handy-cpal`'s loopback device (a global tap that excludes no process, plus a private aggregate device). Taps beat ScreenCaptureKit here:
  - They need only the "System Audio Recording" permission, not Screen Recording, which on Sequoia and later re-prompts users monthly ([9to5mac, Aug 2024](https://9to5mac.com/2024/08/14/macos-sequoia-screen-recording-prompt-monthly/)).
  - SCK can't capture audio alone without also setting up a video stream ([Apple forums](https://developer.apple.com/forums/thread/718279)).
  - SCK misses daemon-routed audio such as FaceTime or phone calls ([dev.to, accessed 2026-10-25](https://dev.to/nickdelv/2000-buffers-of-nothing-3i8)).
  - **We must add `NSAudioCaptureUsageDescription` to `Info.plist`.** Without it, the tap reports success but every buffer is zeros. Detect all-zero buffers and show "permission needed" ([same source](https://dev.to/nickdelv/2000-buffers-of-nothing-3i8)).
  - `tauri.conf.json` targets 10.15, so the feature is gated at runtime.
- **Windows: WASAPI loopback** on the default render device, through the same cpal path. Process loopback, which can exclude our own PID, needs `ActivateAudioInterfaceAsync` and Windows build 20348+ ([Microsoft sample](https://learn.microsoft.com/en-us/samples/microsoft/windows-classic-samples/applicationloopbackaudio-sample/)). That path is not in cpal, so it is deferred.
- **Our own sounds:** feedback sounds and the dictation overlay are muted while a meeting records, so the global tap doesn't capture them.
- **Output-device changes mid-call** (switching to AirPods, say) invalidate the tap's device UID. Recreate the tap from the property listener and splice the gap.
- Each channel is written to its own file (a FLAC or WAV per channel; size and encoding to be decided in a spike). One hour of 16 kHz 16-bit mono is about 115 MB uncompressed, and FLAC brings it to roughly 40–60 MB.

### 4.2 Transcription
The worker is **single-model** (`Request::Load` replaces what's loaded), and one model can't run two streams at once because of its compute lock. So:
- **Live (becomes canonical):** VAD-segment each channel (reuse `Chunker` and `create_detector`), batch-transcribe each finished utterance with Parakeet, and tag it with its channel and its start time on the meeting clock. Parakeet does 12 s of audio in 104 ms on Metal (SPEC.md), so two channels run comfortably in real time. Captions are delayed by roughly 1–3 s after a pause, which is fine for a side pane. The Apple engine path is the same, utterance by utterance.
- **Canonical (after stop):** live utterances are already full Parakeet batch runs, so they're near-final. The final pass re-transcribes only the spans the live pass dropped or queued past a deadline, and redoes the whole meeting only when live ran on a different engine (Apple before Parakeet was ready) or the user asks. The re-transcription runs the channel files through chunked transcription, cutting on long pauses and **keeping segment timestamps**. Today `transcribe_in_chunks` joins only `t.text` and discards `segments`, so we add `transcribe_timed_chunks()` that returns `(t0_ms, t1_ms, text)` offset by each chunk's start. Interleave both channels by `t0`. It runs as a background job that yields to dictation through the existing `ChunkControl`. A full re-run (the rare case) should take about 1–2 min for a 60-minute meeting on an M-series Mac (scaled from the SPEC measurements; to be verified); filling gaps takes seconds.
- **Phase 2 diarization:** add a `Diarize { preset }` request to the worker protocol and load Sortformer in a **second worker instance** (or in-process: it is small and has its own compute lock). Map turns onto segments by maximum time overlap.

### 4.3 Notes generation (local LLM)
- One hour is about 8–12k words, or roughly 11–16k tokens. The cleanup server runs with `-c 4096 -np 1`, which is tuned for dictation latency.
  - **Map steps during the call run on that existing 4096 server.** A 10-minute block is about 1.5–2k words ≈ 2–2.5k tokens, plus the prompt and ~300 output tokens, which fits. They run at low priority behind dictation cleanup, in the same `-np 1` queue.
  - **After stop, the reduce step (or the single pass) runs on an on-demand `llama-server`** with the same Gemma 4 E2B file and `-c 16384`, which exits afterwards. Alternatively, the one server is restarted with a larger `-c` for the job. E2B supports 128K context with a 512-token sliding window ([HF card](https://huggingface.co/google/gemma-4-E2B-it)), so a 16K KV cache stays small.
- **Up to about 12k tokens of transcript:** a single pass over the template, the user's notes and the full transcript (one line per segment, `[mm:ss] Me|Them: text`).
- **Longer meetings: incremental map-reduce.** During the meeting, every ~10 minutes of live-pass text gets a "minutes" extraction (facts, decisions, owners, each with timestamps), so the work is done before stop. After stop, the job processes only the last block and then a reduce step: template + user notes + block minutes. This hides most of the latency. Small models do better on blocks than on one long pass ("lost in the middle"; [Frontiers 2025](https://www.frontiersin.org/journals/artificial-intelligence/articles/10.3389/frai.2025.1604034/pdf)).
- **Latency estimate (M4 Pro, unmeasured):** decode runs at about 95 tok/s (SPEC.md), so 600–900 output tokens take ≈ 6–10 s. Prefilling 12–16k tokens is the unknown. Google's LiteRT figures for E2B on M4 are about 900 tok/s on CPU and 7,800 tok/s on GPU ([Google, accessed 2026-10-25](https://developers.google.com/edge/litert-lm/models/gemma-4)), which suggests roughly 3–15 s on llama.cpp Metal. **Target: notes 20–40 s after the transcript finishes.** Show progress and stream the output.
- **Quality expectation:** E2B is fine for summaries and action items, but weak at subtle attribution and long-range reasoning (E2B scores 19.1% on MRCR v2 at 128k). Offer Gemma 4 E4B as the "higher quality notes" option (53 tok/s). The SPEC's 8 GB default (Qwen3.5 2B) applies here too.
- **Apple Foundation Models** has a 4,096-token *session* window, counting input and output ([TN3193](https://developer.apple.com/tutorials/data/documentation/technotes/tn3193-managing-the-on-device-foundation-model-s-context-window.md)). It could only serve the map step with chunks of ~2.5k tokens, and the background rate limits noted in the SPEC apply. **Not used for notes in the MVP.**
- **Guardrail:** the dictation output guard (paste the raw text when too few of the words are kept) doesn't apply to summaries. Instead:
  - Every decision and action item must cite a `[mm:ss]` that exists in the transcript; items with no anchor are dropped or marked unverified.
  - The user's own bullets must all appear, verbatim or expanded.
  - The UI links each cited time to the transcript.

### 4.4 UI surfaces
- **Tray and hotkey:** Start/Stop meeting, plus a recording indicator.
- **Meeting window:** the notepad, the live Me/Them transcript, a timer, and a warning if system audio is missing.
- **Meeting detail:** tabs for Enhanced / My notes / Transcript. Cited timestamps link into the transcript; there are Regenerate (with version picker), Export Markdown and Copy actions.
- **Meetings list:** in the main window sidebar, with search.
- **Settings → Meetings:** notes model (E2B / E4B), audio retention, system-audio permission status, and the headphones hint.

### 4.5 Data model (SQLite, new migrations in `history.rs`'s list, or a sibling `meetings.rs` on the same DB)
```sql
CREATE TABLE meetings (id INTEGER PRIMARY KEY, title TEXT NOT NULL, started_at INTEGER NOT NULL,
  ended_at INTEGER, status TEXT NOT NULL,            -- recording|transcribing|summarizing|ready|failed
  mic_audio_path TEXT, system_audio_path TEXT, audio_expires_at INTEGER,
  stt_model TEXT, has_system_audio BOOLEAN NOT NULL DEFAULT 0);
CREATE TABLE meeting_segments (id INTEGER PRIMARY KEY, meeting_id INTEGER NOT NULL REFERENCES meetings ON DELETE CASCADE,
  channel TEXT NOT NULL,                              -- 'mic'|'system'
  speaker_key TEXT,                                   -- NULL in MVP; 'sys:1'.. in phase 2
  t0_ms INTEGER NOT NULL, t1_ms INTEGER NOT NULL, text TEXT NOT NULL,
  pass TEXT NOT NULL,                                 -- 'live'|'gapfill'|'rerun' (a full rerun supersedes earlier rows)
  suppressed_echo BOOLEAN NOT NULL DEFAULT 0);
CREATE TABLE meeting_speakers (meeting_id INTEGER, speaker_key TEXT, display_name TEXT,
  PRIMARY KEY (meeting_id, speaker_key));             -- 'me' → "Me", 'them' → "Them"
CREATE TABLE meeting_user_notes (meeting_id INTEGER PRIMARY KEY, body TEXT NOT NULL, updated_at INTEGER);
CREATE TABLE meeting_ai_notes (id INTEGER PRIMARY KEY, meeting_id INTEGER NOT NULL, template_id TEXT NOT NULL,
  model TEXT NOT NULL, body_md TEXT NOT NULL, created_at INTEGER NOT NULL);  -- append-only versions
CREATE TABLE meeting_chunk_minutes (meeting_id INTEGER, block INTEGER, t0_ms INTEGER, t1_ms INTEGER,
  body TEXT, PRIMARY KEY (meeting_id, block));        -- map-step cache
CREATE VIRTUAL TABLE meeting_fts USING fts5(title, user_notes, ai_notes, transcript, content='');
```
FTS5 is compiled into our bundled SQLite (`libsqlite3-sys` sets `-DSQLITE_ENABLE_FTS5`).

### 4.6 Files to add or change
| Area | Change |
|---|---|
| `src-tauri/Info.plist` | add `NSAudioCaptureUsageDescription` |
| `audio_toolkit/audio/system.rs` (new) | loopback capture through handy-cpal, permission and zero-buffer detection, device-change recreation; Windows render-device loopback |
| `managers/meeting/` (new: `mod.rs`, `recorder.rs`, `live.rs`, `finalize.rs`, `notes.rs`, `store.rs`) | session state machine; two-track writer; live utterance transcription; final pass; notes job; SQLite and FTS |
| `managers/audio.rs` | let a meeting hold the mic alongside dictation (shared stream fan-out), mute feedback sounds during meetings |
| `managers/transcription/job.rs` | `transcribe_timed_chunks()` that keeps segment times |
| `managers/cleanup/server.rs`, `prompt.rs` | `ServerOptions.ctx` (configurable `-c`), an on-demand notes-server profile; notes prompts and templates (`cleanup/notes_prompt.rs`) |
| `engine_supervisor/protocol.rs`, `worker.rs` | Phase 2: `Diarize` request; second-worker support |
| `managers/model.rs`, catalog | Phase 2: Sortformer GGUF download, plus licence and NOTICE |
| `commands/meeting.rs` (new), `lib.rs`, `tray.rs`, `shortcut/` | start/stop/list/get/export/regenerate commands; tray item and hotkey |
| `src/components/meetings/` (new) | meeting list, meeting window (notepad + live transcript), note view (tabs: Enhanced / My notes / Transcript), export |
| `src/i18n/locales/en/translation.json` | all new strings |

## 5. Risks, unknowns and spikes

1. **Echo and double capture (the biggest MVP risk).** Without headphones, the mic picks up the remote voices, so "Them" speech also appears as "Me". Granola itself recommends headsets. MVP mitigation: when a mic utterance overlaps system-channel speech in time and its text is ≥70% similar (`strsim`), mark it `suppressed_echo`, hide it, and keep it in storage. Real acoustic echo cancellation (Apple voice processing I/O, WebRTC AEC3) is out of scope unless the spike shows the heuristic fails. **Spike: 3 real Zoom/Meet calls on laptop speakers, measuring the false Me lines.**
2. **Process-tap reliability.** There is a report of all-zero buffers on macOS 26.5 beta ([Apple forums](https://developer.apple.com/forums/tags/core-audio)); the minimum version differs by source (14.2, 14.4, or cpal's 14.6); and output-device switches break the tap. **Spike:** run handy-cpal loopback on macOS 26 with AirPods switching, TCC deny and allow, and a 60-minute run.
3. **Sharing the engine with dictation.** Live meeting transcription and dictation contend for one Parakeet worker. Dictation must win, so meeting utterances queue (live captions lag). They are never lost: utterances that miss the live deadline are transcribed in finalize. **Spike:** measure dictation latency while a meeting is live.
4. **Notes latency and quality on E2B.** **Spike:** llama.cpp Metal prefill speed at 8k and 16k tokens on M4 / M1 8 GB; single pass versus map-reduce on 5 real transcripts, scored for missed action items and wrong owners. Decide whether E4B should be the default for notes.
5. **Memory.** Parakeet + Gemma at 16K context + (Phase 2) Sortformer is fine on 16 GB; on 8 GB, unload Parakeet before the notes job and use Qwen3.5 2B. A second llama-server loads the 3.1 GB weights again unless the cleanup server is asleep first; otherwise restart the one server with a larger `-c`. Confirm in the memory spike.
6. **Sortformer limits (Phase 2).** It handles at most 4 speakers, and DER rises to ~39% with 5 or more. **Spike:** measure Metal and Vulkan RTF through transcribe-cpp on a 60-minute system track. If large calls matter, switch to speakrs (licence to be checked) or a Nemotron-3 port.
7. **Windows.** Device loopback also captures our own sounds and other apps' audio, such as music. The fix is process loopback in "exclude tree" mode with our own PID (Windows 20348+, not in cpal).
8. **Legal and UX.** Recording-consent rules vary; show a one-time notice. The NVIDIA licence notice must ship with the Sortformer download.
