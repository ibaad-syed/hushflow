# Local dictation app: v1 spec and decisions

Open-source (GitHub, MIT), fully local by default, English only. A Wispr Flow-style dictation app.
Mac first and best; Windows second (assume a GPU is often present).

## Decisions so far

| #   | Decision                                                                                                                                                                                                                                                                                                                                                                                                                                   | Why                                                                                                                                                                                                                                                                                                            |
| --- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1   | Fork **Handy** (cjpais/Handy, MIT, Tauri + Rust) as a thin fork that tracks upstream                                                                                                                                                                                                                                                                                                                                                       | It already has Parakeet on Metal/Vulkan, a live overlay, bring-your-own-key and Apple Intelligence cleanup, and a separate inference worker process. OpenWhispr is too large and too tied to its paid cloud; the Rust Hex depends on Anomaly's own UI framework and on OpenCode; VoiceInk is GPL and Mac-only. |
| 2   | English only for v1                                                                                                                                                                                                                                                                                                                                                                                                                        | It lets us use Parakeet English and trim the multilingual scope.                                                                                                                                                                                                                                               |
| 3   | **Adaptive speech-to-text.** On a Mac running macOS 26+, start with Apple's built-in SpeechAnalyzer (no download), download Parakeet in the background, and switch to it automatically once it's ready                                                                                                                                                                                                                                     | Apple's engine works instantly and is fast enough; Parakeet is faster on the GPU and more accurate.                                                                                                                                                                                                            |
| 4   | Every engine and provider sits behind an availability check, with automatic fallback                                                                                                                                                                                                                                                                                                                                                       | Built-in engines can be missing, turned off, or fail.                                                                                                                                                                                                                                                          |
| 5   | **Users can choose the engine themselves.** Settings has "Transcription engine: Automatic (recommended) / System built-in / Parakeet (our model)" and the same for cleanup: "Automatic / Apple Intelligence / Local model / Your API key". In Automatic, dictation starts on the OS engine immediately, while Parakeet downloads in the background and takes over once verified (decision #3). "System built-in" never downloads Parakeet. | Instant start with no download, the best accuracy once Parakeet is ready, and users who want native-only can pin it.                                                                                                                                                                                           |

## Engine selection

Speech-to-text, in order of preference:

1. Parakeet English (Unified), once downloaded and verified: Metal on Mac, Vulkan on Windows (CPU fallback).
2. Apple SpeechAnalyzer `SpeechTranscriber` (macOS 26+). The system manages its asset via `AssetInventory`.
3. Windows AI Speech Recognition API, **only once Microsoft marks it as officially supported**. It is experimental as of Oct 2026, needs MSIX packaging, and must not ship in production yet.
4. If nothing is ready, download Parakeet in the foreground with a progress bar.

Cleanup (when the "Clean up my text" toggle is on), in order of preference:

1. Bring-your-own-key provider, if the user configured one and is online (Phase 3).
2. Apple Foundation Models, if Apple Intelligence is enabled.
3. Bundled llama.cpp sidecar with a small model (default to be chosen from the benchmarks).
4. Rule-based cleanup only (fillers, capitalization), or the raw text.

Every cleanup call has a deadline (~1.5 s) and a meaning-drift guard; on failure it pastes the raw transcript.

## Phases

1. **Fast speech-to-text, Mac + Windows.**
   - Hotkeys: Wispr-style defaults. Mac: Fn (hold to talk), Fn+Space (hands-free). Windows: Ctrl+Win, Ctrl+Win+Space.
   - Shortcuts can be remapped; paste-last hotkey; Esc to cancel.
   - Live overlay; reliable paste with clipboard restore.
   - Personal dictionary; history; signed builds.
2. **Local cleanup.** A single toggle, the bundled model, the guardrails, and short phrases skipping the LLM.
3. **Bring your own key.** OpenAI-compatible endpoints plus native Anthropic/Gemini; keys in the OS keychain; fallback to local.

## Measurements (M4 Pro, 48 GB, macOS 26.5; macOS text-to-speech test clips; warm runs)

| Speech-to-text                | 3 s    | 12 s   | 29 s   |
| ----------------------------- | ------ | ------ | ------ |
| Parakeet English, Metal       | 48 ms  | 104 ms | 251 ms |
| Parakeet English, CPU         | 64 ms  | 297 ms | 661 ms |
| Apple SpeechTranscriber       | 83 ms  | 200 ms | 430 ms |
| Whisper large-v3-turbo, Metal | 569 ms | 656 ms | 790 ms |

| Cleanup (llama.cpp, Q4_K_M)   | Speed                                           | Quality notes                                                                                                                                                                    |
| ----------------------------- | ----------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Qwen3.5 0.8B                  | 90–570 ms                                       | Fails: no punctuation, keeps self-corrections, answered "write me a poem"                                                                                                        |
| Qwen3.5 2B                    | 110–470 ms (1.1 s for 30 s of speech)           | Good lists, resisted the instruction trap; kept "no wait" and "you know"                                                                                                         |
| **Gemma 4 E2B** (3.1 GB file) | 150–440 ms (1.2 s for 30 s of speech), 95 tok/s | **Best balance.** Applied the "no wait → Friday" correction, removed fillers, resisted the instruction trap. Dropped "Hey"; wrote the 4-item update as prose rather than a list. |
| Qwen3.5 4B (2.7 GB)           | 210–700 ms (1.7 s), 57 tok/s                    | Nicest list formatting, but rewrote meaning ("I'd like to schedule" became "Scheduled") and dropped the intro.                                                                   |
| Gemma 4 E4B (5.0 GB)          | 280–760 ms (2.2 s), 53 tok/s                    | The output that stayed truest to the speaker; too slow for the default.                                                                                                          |
| Apple Foundation Models       | not testable here                               | Apple Intelligence is off on this Mac                                                                                                                                            |

Cleanup default (provisional, needs a ~100-utterance eval on real voices): **Gemma 4 E2B** on Macs with 16 GB+; **Qwen3.5 2B** on 8 GB machines; E4B as a "higher quality" option.
| Apple Foundation Models | not testable here | Apple Intelligence is off on this Mac |

Benchmark scripts: `bench/cleanup_bench.py`, `bench/apple_stt.swift`, `bench/apple_cleanup.swift`.
