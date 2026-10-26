# Hushflow

**Wispr Flow-style dictation that runs entirely on your computer.**

Hold a key, speak, let go: your words appear in whatever app you're typing in, with the fillers, false starts and self-corrections cleaned up. Speech recognition and cleanup both run locally, so nothing you say leaves your machine.

Hushflow is a fork of [Handy](https://github.com/cjpais/Handy) by CJ Pais (MIT), built with Tauri (Rust + React).

## Features

- **Wispr-style hotkeys.** Hold **Fn** (Mac) or **Ctrl + Win** (Windows) to talk; tap it instead to keep recording hands-free until you tap again. Esc cancels. Remappable.
- **Works right away on a Mac.** On macOS 26+ it starts with Apple's built-in speech recognizer (no download), fetches [Parakeet](https://huggingface.co/nvidia/parakeet-tdt-0.6b-v2) in the background and switches to it once it's ready. You can also pick an engine yourself.
- **Local cleanup ("Clean up my text").** A small on-device LLM (Gemma 4 E2B by default, or Qwen 3.5, or Apple Intelligence) removes filler words, applies "no wait, Tuesday"-style corrections, and formats lists and paragraphs in about 0.2 s. A rule-based pass covers you while the model downloads, and a guard pastes your words unchanged if the model ever answers instead of cleaning. The instructions are editable.
- **Light on resources.** The cleanup model sleeps after the idle time you choose and wakes while you're speaking.
- **History**, file transcription, a slim waveform overlay.

English only for now. See [docs/fork/SPEC.md](docs/fork/SPEC.md) for the design decisions and benchmarks.

## Download

Every push to `main` builds a Mac (Apple Silicon) app and a Windows installer: open the latest run under [Actions → Main Branch Build](../../actions/workflows/main-build.yml) and download the artifacts. Tagged versions are published under [Releases](../../releases).

The builds aren't code-signed yet:

- **macOS:** after moving Hushflow to Applications, run `xattr -dr com.apple.quarantine /Applications/Hushflow.app` (or right-click → Open) the first time.
- **Windows:** in the SmartScreen prompt, choose **More info → Run anyway**.

## Build from source

Requires [Rust](https://rustup.rs/) and [Bun](https://bun.sh/). On macOS, Xcode 26 for the Apple speech and Apple Intelligence bridges.

```bash
bun install
bun scripts/fetch-llama.ts      # llama.cpp server for local cleanup
bun run tauri dev
```

`bun run tauri build` also signs update bundles, which needs the maintainer's key. Without it, build with `bun run tauri build -c '{"bundle":{"createUpdaterArtifacts":false}}'`.

Platform details are in [BUILD.md](BUILD.md); the upstream Handy README (troubleshooting, CLI flags, integrations) is kept at [docs/HANDY_README.md](docs/HANDY_README.md).

## License

MIT, like Handy. See [LICENSE](LICENSE).
