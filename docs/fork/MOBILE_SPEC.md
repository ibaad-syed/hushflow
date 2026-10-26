# Hushflow Notes on iPhone and Android: plan and decisions

Status: proposal, 2026-10-25 (revised after the product decision below). Research only; no code yet. Builds on `NOTES_SPEC.md` (the desktop note-taker, already built) and `SPEC.md` (engines, cleanup, guard). Sources were accessed on 2026-10-25 unless another date is given.

## 0. The decision

The mobile product is **a Granola-style meeting note-taker that works completely offline**, on iPhone and Android. It is not a dictation keyboard.

**The flow:** open the app, tap Start, and record an in-person meeting, lecture, interview or voice memo. Recording continues with the screen locked. A live transcript comes from the **OS's built-in on-device recognizer** (Apple SpeechAnalyzer on iOS 26; Android on-device `SpeechRecognizer`). The user types rough notes. On stop, **Gemma 4 E2B runs on the phone, offline,** and writes the notes: title, summary, the user's notes expanded, decisions and action items with `[mm:ss]` citations checked against the transcript, and open questions.

**Same guarantees as desktop:** the verbatim transcript is always kept, notes are versioned (regenerating never overwrites), full-text search, and export or share as Markdown.

**Constraints, stated plainly:**

- **No call recording on either OS.** In-person or speakerphone only, so there are no Me/Them channels. Speaker diarization comes later.
- **Gemma 4 E2B needs a phone with about 8 GB RAM.** Smaller phones fall back to Qwen 3.5 2B, or to Apple Foundation Models on supported iPhones.
- **Notes are written in the foreground.** If the user locks the phone, they are written when the user comes back (see §3).

| #   | Decision                                                                                                                                                                              | Why (one line)                                                                                                                                           |
| --- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------- |
| M1  | **Product = offline meeting notes.** The keyboard, IME and bubble plan moves to §7 "Later".                                                                                           | Recording a meeting needs no keyboard extension, no app bounce and no accessibility service, which removes the hardest App Review and Play policy risks. |
| M2  | **Stack: Tauri 2 mobile first**, reusing the Rust core and the React meeting UI, with small Swift/Kotlin plugins. Native Swift/Kotlin apps are the fallback if the Tauri spike fails. | The app is now a normal full-screen app, which is what Tauri mobile is best at. Most of the code already exists (§2).                                    |
| M3  | **Live transcript from the OS recognizer.** `transcribe-cpp`/Parakeet is not needed for the MVP.                                                                                      | Zero model download, no Android native-build risk. Parakeet on device can be a later option.                                                             |
| M4  | **Gemma 4 E2B runs in-process**, not as a child process.                                                                                                                              | iOS forbids child processes, so the desktop's `llama-server` can't move over.                                                                            |
| M5  | **Record in the background, write notes in the foreground.**                                                                                                                          | A recording started in the foreground may continue locked, but iOS forbids GPU work in the background.                                                   |
| M6  | **Both platforms, spiked in parallel.** iPhone is not a hard prerequisite for Android.                                                                                                | With no keyboard, the two platforms are close in effort, and the shared code is the same.                                                                |

## 1. Platform facts that drive the decisions

**Recording and the lock screen**

- **iOS:** a recording started in the foreground continues in the background with `UIBackgroundModes=audio`, but it can't be started from the background and can't resume from the background after an interruption such as a call or Siri (Apple DTS; [forum thread 120038](https://developer.apple.com/forums/thread/120038)). So we start with a visible tap, show a Live Activity, and on interruption post a "Paused, tap to resume" notification and splice the segments.
- **Android:** a `microphone` foreground service must be started while an activity is visible, or from a notification or widget tap ([Android docs, updated 2026-10-23](https://developer.android.com/develop/background-work/services/fgs/restrictions-bg-start)). Tapping Start satisfies this. OEM battery killers (Xiaomi, Samsung) need an "unrestricted battery" prompt.
- **Granola's iPhone app** is the reference: in-person meetings on the phone's microphone, start and stop from the lock screen and widgets, no audio kept ([App Store](https://apps.apple.com/us/app/6739429409), [tl;dv](https://tldv.io/blog/granola-review)). We differ by keeping the audio under the retention setting and by working fully offline.

**No call recording**

- iOS offers no API for call audio. The Play policy explicitly bars the accessibility API for call audio ([Engadget, 2022](https://www.engadget.com/google-is-banning-third-party-call-recording-apps-from-the-play-store-093201443.html)). So this is mic-only, one audio track, no Me/Them split.

**Live transcript engines**

- **iOS: SpeechAnalyzer / `SpeechTranscriber`** (iOS 26, system-managed asset). There is no official device list. Developers report it doesn't run on A13 (iPhone 11, SE 2) and works on iPhone 12, SE 3, 14 Pro and 15 Pro ([forum 807739](https://developer.apple.com/forums/thread/807739), [806765](https://developer.apple.com/forums/thread/806765)). Gate it at runtime. Whether it keeps running from a backgrounded app with the screen locked is spike (c).
- **Android: `SpeechRecognizer.createOnDeviceSpeechRecognizer`** (API 31+, `isOnDeviceRecognitionAvailable`) ([Android ref](https://developer.android.com/reference/android/speech/SpeechRecognizer)). The ML Kit GenAI Speech Recognition API is alpha, and its "Advanced" mode runs on Pixel 10 only ([ML Kit](https://developers.google.com/ml-kit/genai/speech-recognition/android)).
- **Fallback for both:** if a recognizer stops while locked or on a device without it, keep recording the audio and transcribe it on stop. The audio file is the safety net, as on desktop.
- **Later option:** Parakeet TDT 0.6B as Core ML on the Neural Engine (FluidAudio's conversion, [HF](https://huggingface.co/FluidInference/parakeet-tdt-0.6b-v3-coreml)) was measured by LocalFlow at 40 ms warm for 2.4 s of audio and 165 MB peak on an iPhone 15 Pro ([PR #11](https://github.com/ajbarryiii/LocalFlow/pull/11)). Android would use Parakeet GGUF through `transcribe-cpp`, whose sys crate has no Android build branch yet. Neither is needed now.

**On-device Gemma**

- **iOS forbids GPU work in the background**, and ggml's Metal backend crashes the app when iOS cuts that work off (iOS 26.2; [forum 816774](https://developer.apple.com/forums/thread/816774), [whisper.cpp #3531](https://github.com/ggml-org/whisper.cpp/issues/3531)). So the notes job runs only while the app is in the foreground and checks `applicationState`. If the user locks the phone mid-job, we stop, keep the partial map-step results, and finish when they return. Background CPU-only generation is possible but slow, and is not planned.
- **Gemma 4 E2B numbers** (Google): 2.58 GB file; S26 Ultra GPU 52 tok/s, 0.3 s to first token, 676 MB peak; iPhone 17 Pro CPU 25 tok/s with about 0.6 GB; 128K context ([LiteRT-LM](https://developers.google.com/edge/litert-lm/models/gemma-4), [Google blog](https://developers.googleblog.com/blazing-fast-on-device-genai-with-litert-lm/), [HF card](https://huggingface.co/google/gemma-4-E2B-it)). These are Google's figures for LiteRT-LM, not for our runtime. Quality caveat from NOTES_SPEC: E2B is fine for summaries and action items but weak at subtle attribution and long-range reasoning, which is why the citation check matters.
- **Fallbacks:** Qwen 3.5 2B on phones under 8 GB. **Apple Foundation Models** (about 3B, iPhone 15 Pro and later, Apple Intelligence on) has only a 4,096-token session window ([TN3193](https://developer.apple.com/tutorials/data/documentation/technotes/tn3193-managing-the-on-device-foundation-model-s-context-window.md)), so it can serve only the map step on roughly 2.5k-token chunks. **Gemini Nano** (ML Kit Prompt API, alpha; Pixel 10 first) is too narrow to be a default ([blog](https://android-developers.googleblog.com/2025/10/ml-kit-genai-prompt-api-alpha-release.html)).

## 2. Architecture

### 2.1 Stack: Tauri 2 mobile first

Reuse the Rust core and the React meeting UI in `src/components/settings/meetings/`. Add small native plugins:

| Plugin (Swift / Kotlin) | Job                                                                                                             |
| ----------------------- | --------------------------------------------------------------------------------------------------------------- |
| Background recording    | `AVAudioSession` + `UIBackgroundModes=audio` + Live Activity; Android `microphone` foreground service           |
| Platform speech         | SpeechAnalyzer streaming; Android on-device `SpeechRecognizer`; partial and final results as events to the core |
| File sharing            | Share sheet / `ACTION_SEND` for the Markdown export                                                             |

Desktop-only plugins (tray, hotkeys, overlay, updater) are already gated off mobile in `Cargo.toml`. The fallback is **native SwiftUI and Compose apps** sharing `hushflow-core` through UniFFI (used extensively by Mozilla in Firefox mobile, pre-1.0; [docs.rs](https://docs.rs/crate/uniffi/latest)). We switch if spike (a) or (b) fails. Third-party reviews still call Tauri mobile "less mature" ([codercops](https://blog.codercops.com/blog/tauri-2-desktop-mobile-apps-rust-web-2026), [makerstack](https://makerstack.co/reviews/tauri-review/)), and the background-audio plugin is the part most likely to hurt. React Native and Flutter stay rejected: they reuse none of our UI or core.

### 2.2 In-process inference

The desktop runs Gemma through a `llama-server` child process (`managers/cleanup/server.rs`) and the STT worker through `engine_supervisor`. **iOS forbids child processes**, so on mobile Gemma runs **in-process**:

- **iOS and shared path:** llama.cpp as a library called from Rust, for example the `llama-cpp-2` crate, with the same GGUF and prompts as desktop.
- **Android:** the same crate on CPU, or **LiteRT-LM** (GPU, Google's published numbers) behind the same trait. Decide in spike (d).
- The notes job sits behind a `NotesEngine` trait in the core, so the runtime can differ per platform while the prompt and citation check stay identical. Crash isolation is lost on iOS, so the job must be resumable from the saved map-step results.

### 2.3 Shared code: `hushflow-core`

A new Rust crate used by both desktop and mobile, so the prompts and checks can't drift:

- the meeting store (SQLite, the schema in NOTES_SPEC §4.5, FTS5 search, versioned notes);
- the notes prompt, templates and the `[mm:ss]` citation check;
- the cleanup prompts and rules (`audio_toolkit/text.rs`, `managers/cleanup/{prompt,catalog}.rs`).

Measured in this repo: 36K lines of Rust in `src-tauri/src`, and these modules import nothing from Tauri. Extracting them is worth doing even without mobile. Desktop-only: `shortcut/`, `paste_tx/`, `clipboard.rs`, `overlay.rs`, `tray.rs`, `secure_input.rs`, `transcription_coordinator.rs`.

### 2.4 Notes flow on the phone

1. **During:** record one mono track to disk (FLAC; NOTES_SPEC §4.1), stream live captions from the OS recognizer into the store as utterances with times, autosave the typed notes.
2. **On stop:** finalize the transcript (fill gaps from the audio if the recognizer dropped any), then, with the app in the foreground, run Gemma: a single pass up to about 12k transcript tokens, otherwise map-reduce over 10-minute blocks (NOTES_SPEC §4.3). The map blocks may run during the meeting while the app is in the foreground.
3. **Guardrails, same as desktop:** every decision and action item must cite a `[mm:ss]` that exists in the transcript, or it is dropped or marked unverified; every user bullet must appear, verbatim or expanded; cited times link into the transcript.
4. **If the phone is locked** after stop, show "Notes will be written when you open the app", and resume on return.

## 3. Risks

| Risk                                                                                | Likelihood / impact | Mitigation                                                                                                           |
| ----------------------------------------------------------------------------------- | ------------------- | -------------------------------------------------------------------------------------------------------------------- |
| Tauri mobile can't host background audio or the plugins reliably                    | Medium / high       | Spikes (a) and (b) first. Fall back to native apps over `hushflow-core`.                                             |
| The OS recognizer stops while the screen is locked, or is missing on older devices  | Medium / high       | Always record audio; transcribe on stop as the fallback. Gate on runtime availability. Parakeet as a later option.   |
| Gemma in-process: memory (jetsam on iOS, low-memory killer on Android), speed, heat | Medium / high       | Spike (d). Qwen 3.5 2B or Apple Foundation Models on small phones. Unload after the job. Resumable map-step results. |
| Notes can't run while locked, so users wait on return                               | Certain / medium    | Say so in the UI. Run map blocks during the meeting while foregrounded. Keep the quality target realistic (below).   |
| Quality: E2B misattributes and loses detail on long meetings; no speakers           | High / medium       | The citation check, versioned regenerate, the transcript always visible. Diarization later.                          |
| Interruptions (calls, Siri) end iOS recording                                       | Medium / medium     | Notification to resume, splice the segments, mark the gap in the transcript.                                         |
| Storage and download: a 2.58 GB model, plus audio                                   | Certain / low       | Opt-in download on Wi-Fi at first run; audio under the existing retention setting.                                   |
| Consent: recording laws vary                                                        | Certain / medium    | The same one-time notice as desktop.                                                                                 |

## 4. Prerequisites on the build machine

- **iOS:** full **Xcode** (not installed yet, the user has to install it) and an **iPhone connected** for real testing. Background audio and the speech recognizer don't work properly in the simulator.
- **Android:** JDK, the Android SDK and NDK, and an Android phone or an emulator (a real phone for the background and recognizer spikes).
- Rust targets `aarch64-apple-ios` and `aarch64-linux-android`.

## 5. Phases and effort

**Phase 0: spikes, 1-2 weeks.** Each has a pass/fail:

| #   | Question                                                                                                         | Pass                                                                                                                                                                  |
| --- | ---------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| (a) | Does Tauri mobile build and launch with our Rust core and the React meeting UI on Android and iOS?               | App runs on both, calls into the core, and stores a meeting in SQLite.                                                                                                |
| (b) | Does background recording continue with the screen locked on both?                                               | A 60-minute recording locked on an iPhone and an Android phone, no gaps. Interruption and resume behave as designed (§1).                                             |
| (c) | Do the native recognizers transcribe live, on device, offline (airplane mode)?                                   | Usable live text for a 30-minute meeting, including with the screen locked. If not, record and transcribe on stop.                                                    |
| (d) | What are in-process Gemma notes speed and memory on a phone? Run on the Mac as a stand-in first, then on phones. | A 30-minute transcript yields checked notes in an acceptable time (target under 2 minutes) without being killed, on an 8 GB phone. Eval scores compared with desktop. |

If (a) or (b) fails, the fallback is native apps; the other spikes still apply.

**After the spikes: MVP builds.** The previous estimate was 2-3 months solo for both platforms, before testing. **That is provisional until the spikes report.** Rough shape, to be re-estimated after Phase 0:

| Phase                       | Scope                                                                                                                                        |
| --------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------- |
| 1. Core extraction          | `hushflow-core`, desktop moved onto it, iOS and Android cross-compile in CI                                                                  |
| 2. Mobile MVP               | Start/stop, background recording, live transcript, typed notes, in-process Gemma notes with citation check, versions, search, Markdown share |
| 3. Hardening and submission | Interruptions, low-memory fallbacks, model download, testing on real devices, App Store and Play submission                                  |

## 6. Not in the MVP

Speaker diarization (NOTES_SPEC Phase 2, in-person mode); call recording (impossible); Parakeet on device; sync with the desktop app; user-editable templates and "ask this meeting".

## 7. Later: dictation keyboard

The earlier plan is parked, not deleted. If we return to it: on iPhone, the Wispr Flow "session" model (a main app holding the mic in the background, plus a thin keyboard extension, App Review 4.4.1 risk; [Wispr docs](https://docs.wisprflow.ai/articles/7453988911-set-up-the-flow-keyboard-on-iphone)); on Android, our own voice IME that records in its own process, with a floating bubble and accessibility service as a later step. The research is in the git history of this file. Shared code from `hushflow-core` (cleanup prompts and rules) carries over.
