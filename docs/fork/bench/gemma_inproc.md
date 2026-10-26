# Gemma meeting notes in-process (no `llama-server`): spike results

2026-10-26. The question was whether Gemma 4 E2B can write meeting notes **inside the app process**, the way it would have to on iOS, which forbids child processes, and whether it is fast and lean enough for a phone. The stand-in phone is this Mac (M4 Pro, 48 GB), with CPU-only runs at 4 threads as the closer stand-in for a phone's compute. Code: `spikes/gemma-inproc/`, a standalone crate outside the app workspace.

## Answer

**Yes, it works, and in-process costs nothing compared with the server.** llama.cpp linked as a library (`llama-cpp-2` 0.1.160) gives the same prompt tokens (2,696 for the 1.5k case) as `llama-server` b11887 and the same speed: 1,011 tok/s prefill and 81 tok/s decode on a 10k-word meeting, against the server's 1,042 and 82. Greedy decoding gives **byte-identical notes**. Load takes 0.4–0.9 s. Freeing everything takes 0.1 s.

What decides the phone question is memory and the CPU fallback:

- **Memory is almost all weights.** With the weights fully resident (`--no-mmap`), the peak physical footprint is **3.76–3.93 GB** for Q4_K_M at any meeting length. Context size and KV type change it by at most 150 MB, because Gemma 4 E2B keeps KV in only 15 of its 35 layers, 12 of them sliding-window: 60 MB at 8k context, 132 MB at 20k. With mmap (the default), macOS counts only **0.36–0.45 GB** of footprint on Metal, because clean file-backed pages aren't charged. Whether iOS jetsam charges the same pages under Metal is unverified and is the first thing to test on a device.
- **The CPU path is too slow for a long meeting.** With 4 M4 Pro performance cores (more than any phone gives a background app), an hour of transcript takes **3.7 minutes**. The prefill is 101 tok/s, against 1,011 on Metal. A phone CPU will be slower, and slower again when hot.
- **At about 10k words the model degenerates** on every engine, including `llama-server`: it repeats one citation until it reaches the 1,500-token cap. The synthetic transcript makes this worse, because 33 utterances repeat about 14 times, but production should still guard against it (see below).

## Setup

- Model: `gemma-4-E2B-it-Q4_K_M.gguf` (3.11 GB, the desktop's file). Smaller quant: unsloth `gemma-4-E2B-it-Q3_K_M.gguf` (2.54 GB), downloaded to scratch and deleted after the runs.
- Prompt: the `SYSTEM` text from `src-tauri/src/managers/meeting/notes.rs`, copied verbatim, then `Transcript:\n` and the lines. Settings as in notes.rs: temperature 0.2, at most 1,500 tokens, and the same context sizing, which gives 8,192, 11,264 and 20,480 for the three lengths. Sampling uses llama-server's default chain (top-k 40, top-p 0.95, min-p 0.05, then temperature). `swa_full` is off, which is llama-server's default; the C API's default keeps a full-size SWA cache, 96 MB instead of 12 MB at 8k.
- Chat template: llama.cpp's built-in `llama_chat_apply_template` only knows the Gemma 1–3 `<start_of_turn>` format, so the spike writes Gemma 4's format directly: `<|turn>system\n…<turn|>\n<|turn>user\n…<turn|>\n<|turn>model\n`. The result is **byte-identical** to llama-server's `/apply-template` with `enable_thinking: false`, checked with `cmp`. BOS comes from the tokenizer.
- Transcripts: `make_transcripts.py` builds them the same way as the earlier prefill bench: random `cleanup_eval.json` utterances, rising `[mm:ss]` stamps and random Me/Them speakers. Seed 1 gives 1,505, 5,061 and 10,046 words, or 2,696, 8,479 and 17,453 prompt tokens (about 1.7 tokens per word, matching notes.rs's estimate).
- Memory: `/usr/bin/time -l`. The table reports maximum resident set size and **peak memory footprint** (the lifetime peak `phys_footprint`, the number iOS jetsam limits).
- CPU-only means no GPU device at all (`with_devices(&[])` plus `n_gpu_layers 0`, like `--device none`). The log confirms `offloaded 0/36 layers` and only `CPU_*` buffers. Threads are set to 4 for both decode and batch.
- Build: the CPU numbers use `GGML_CPU_ARM_ARCH=armv8.6-a+dotprod+i8mm+fp16`, explained under build notes. Metal is unaffected by the flag.

## Results: Q4_K_M (3.11 GB)

Warm page cache. Load is the model load; context creation adds a further 0.01–0.2 s. All timings are in-process wall clock. "cap" means the run hit the 1,500-token limit.

| Mode                     | Words  | Prompt tok | Load s | Prefill tok/s (s) | Decode tok/s (tokens) | Total s   | Max RSS MB | Peak footprint MB |
| ------------------------ | ------ | ---------- | ------ | ----------------- | --------------------- | --------- | ---------- | ----------------- |
| Metal, mmap              | 1,505  | 2,696      | 0.47   | 1,445 (1.9)       | 92 (823)              | **11.3**  | 3,267      | **359**           |
| Metal, mmap              | 5,061  | 8,479      | 0.43   | 1,250 (6.8)       | 86 (1,355)            | **23.1**  | 3,287      | **379**           |
| Metal, mmap              | 10,046 | 17,453     | 0.44   | 1,011 (17.3)      | 81 (cap)              | **36.3**  | 3,368      | **450**           |
| Metal, no mmap           | 1,505  | 2,696      | 0.89   | 1,452 (1.9)       | 93 (823)              | 11.8      | 3,712      | **3,782**         |
| Metal, no mmap           | 10,046 | 17,453     | 0.92   | 1,015 (17.2)      | 79 (cap)              | 37.2      | 3,821      | **3,879**         |
| CPU ×4, mmap             | 1,505  | 2,696      | 0.96   | 132 (20.5)        | 53 (889)              | **38.2**  | 4,583      | 1,649             |
| CPU ×4, mmap             | 5,061  | 8,479      | 0.75   | 117 (72.5)        | 43 (cap)              | **107.9** | 4,617      | 1,669             |
| CPU ×4, mmap             | 10,046 | 17,453     | 0.78   | 101 (172.6)       | 29 (cap)              | **224.8** | 4,680      | 1,721             |
| CPU ×4, no mmap          | 1,505  | 2,696      | 1.70   | 120 (22.5)        | 52 (889)              | 41.3      | 3,371      | 3,729             |
| CPU ×4, no mmap          | 10,046 | 17,453     | 1.36   | 98 (177.4)        | 36 (cap)              | 220.2     | 3,854      | 3,815             |
| llama-server, Metal      | 1,505  | 2,696      | –      | 1,215–1,312       | 85–95                 | 10.1–11.5 | –          | –                 |
| llama-server, Metal      | 5,061  | 8,479      | –      | 1,148             | 81                    | 24.9      | –          | –                 |
| llama-server, Metal      | 10,046 | 17,453     | –      | 1,042             | 82                    | 35.0      | –          | –                 |
| llama-server, CPU ×4 (†) | 1,505  | 2,696      | –      | 194               | 49                    | 29.2      | –          | –                 |

(†) The bundled llama-server ships `libggml-blas` (Accelerate). Its prefill matrix multiplications therefore run on Apple's AMX/SME units, not only on the 4 threads, so its CPU prefill is not a 4-thread number. The crate's build script turns BLAS off on Apple targets.

The machine was shared with other agents. A CPU 10k rerun made at a load average of about 13 took 416 s (56 tok/s prefill, 14 tok/s decode). That is a reminder of how much contention, or heat on a phone, costs the CPU path.

### Context size and KV-cache type (Metal, no mmap, 1.5k words)

| Context | KV f16: footprint MB, decode tok/s | KV q8_0: footprint MB, decode tok/s |
| ------- | ---------------------------------- | ----------------------------------- |
| 8,192   | 3,787, 92                          | 3,759, 77                           |
| 16,384  | 3,830, 90                          | 3,782, 61                           |
| 32,768  | 3,930, 77                          | 3,842, 78                           |

On CPU at 16k, q8_0 drops the footprint from 3,822 to 3,771 MB, but prefill falls from 131 to 104 tok/s. At 10k words on Metal with mmap, q8_0 gives 381 MB against 450 MB, but decode falls to 66 tok/s from 81. **KV quantization isn't worth it for this model**: it saves 30–90 MB and costs 15–35% of decode speed. Single runs vary by about ±15% on this shared machine, which explains the slow 32k f16 run; the 8k, 16k and 10k-mmap pairs carry the claim. `--ubatch 256` saves about 26 MB on Metal and nothing on CPU, with no change in speed.

Memory breakdown (`LLAMA_LOG=1`, Q4_K_M):

- Metal: the whole file is mapped as `MTL0_Mapped` (2,948 MiB). The per-layer token-embedding table, 1,756 MiB of the file, is a lookup table that stays on the CPU (`CPU_Mapped`). KV is 48 MiB (global) plus 12 MiB (SWA) at 8k, and 120 plus 12 MiB at 20k. Compute is 518 MiB on Metal plus 42–54 MiB on the CPU.
- CPU: `CPU_Mapped` is 2,948 MiB, plus a 1,275 MiB `CPU_REPACK` copy of the matrix-multiplication weights in anonymous memory, which is why CPU-with-mmap still has a 1.65 GB footprint. Compute is 536 MiB.

The 1.76 GB embedding table matters for phones. It is touched one row per token, so with mmap little of it ever needs to be resident. It is also why lower quants barely shrink the file. LiteRT-LM's published 0.6–0.7 GB peaks for this model (MOBILE_SPEC) likely depend on the same property.

### Smaller quant: Q3_K_M (2.54 GB)

| Mode            | Words  | Prefill tok/s | Decode tok/s | Total s | Peak footprint MB |
| --------------- | ------ | ------------- | ------------ | ------- | ----------------- |
| Metal, mmap     | 1,505  | 1,335         | 75           | 10.8    | 362               |
| Metal, no mmap  | 1,505  | 1,407         | 75           | 10.5    | **3,137**         |
| Metal, mmap     | 10,046 | 1,009         | 67           | 40.3    | 439               |
| Metal, no mmap  | 10,046 | 1,004         | 67           | 40.7    | **3,241**         |
| CPU ×4, mmap    | 1,505  | 68            | 40           | 54.0    | 733               |
| CPU ×4, mmap    | 10,046 | 59            | 29           | 348.7   | 803               |
| CPU ×4, no mmap | 1,505  | 69            | 40           | 53.9    | 2,957             |

Q3_K_M saves about 640 MB of resident memory, and it is slower: decode drops 20% on Metal, and CPU prefill halves, likely because Q3_K has no repacked CPU kernels. That cause wasn't confirmed from the logs, and the Q3 CPU runs weren't repeated on an idle machine, so some contention may be included; the Metal rows are unaffected. Its notes are shorter (345–356 words against about 480) but well-formed and correctly cited at 1.5k words. It isn't a good trade, since the 1.76 GB embedding table dominates the size whatever the quant.

## Quality

`check_notes.py` checks each output for shape (title plus the five sections) and checks that every cited time is within 2 s of a transcript line, the same check as notes.rs's `check_citations`. Results:

- **Greedy decoding, 1.5k words: the in-process notes and llama-server's (`temperature: 0`) are byte-identical.** The engine and the prompt are equivalent.
- Sampled, 1.5k words: llama-server gave 469 words, 36 citations, 0 unverified. In-process gave 479 words, 38 citations, 0 unverified on Metal, and 492 words, 47 citations, 0 unverified on CPU. The notes are comparable: the same title style and sections, the same facts (onboarding redesign, API 500 on token expiry, $4.2M revenue, deadline moved to the 20th) and the same action items. The transcript is random utterances, so this comparison is about format and grounding, not about a coherent summary.
- 5k words: the shape is right everywhere, with 1–2 unverified citations out of about 100 on both engines.
- **10k words: broken on both engines.** llama-server, sampled and greedy, and in-process on Metal and CPU all run into the 1,500-token cap while listing the same timestamp over and over, for example `[1:12:22, 1:12:22, …]`. They also invent `mm:ss` stamps past 60 minutes, such as `[132:31]`. This is model behaviour on a long, highly repetitive input, not an engine problem. Real meetings repeat less, but the app should still guard against it: limit citations per bullet in the prompt, add a DRY or repetition penalty, stop when the output starts repeating, or summarize long meetings in two passes (chunk, then merge). The desktop has the same exposure today.

## Crate and build notes

- **`llama-cpp-2` 0.1.160** (utilityai/llama-cpp-rs), released 2026-10-15, about weekly releases, 1.7M downloads. It vendors a llama.cpp that has `gemma4.cpp` and `gemma4-assistant.cpp`, so Gemma 4 support is newer than the desktop's b11887. Alternatives: `llama-cpp-4` (eugenehp fork; 32k downloads, active), `llama_cpp` (edgenai; last release 2024-04, stale), `llama-cpp-rs` (2023, stale). utilityai is the clear choice.
- Build: about 30 s on the M4 Pro (cmake via the `cmake` crate, plus bindgen, which needs libclang from the Command Line Tools). `target/` is 0.86 GB per profile; three profiles were built for the comparisons.
- Metal is always on for Apple targets: the `metal` cargo feature changes nothing in `build.rs`, and the Metal library is embedded (`GGML_METAL_EMBED_LIBRARY=ON`), so no `.metallib` needs packaging. Backend init usually takes 0.05 s, and occasionally about 3 s: this happened on the first run after a build and once in the middle of the matrix, presumably while the embedded Metal source was compiling. On a phone, expect that cost at some launches.
- **The CPU architecture flag matters a lot.** The crate builds ggml with `GGML_NATIVE=OFF` and no `GGML_CPU_ARM_ARCH` on Apple, and with `-march=armv8-a` on Android. That default CPU build managed 73 tok/s prefill at 1.5k words. `GGML_CPU_ARM_ARCH=armv8.6-a+dotprod+i8mm+fp16` gives 130 tok/s, the same as `-C target-cpu=native` (132). A phone build must set it: i8mm is only on recent A-series and Armv8.6+ Android cores (check the minimum device), so a build for older phones should use `armv8.2-a+dotprod`.
- APIs used: `LlamaModelParams::{with_n_gpu_layers, with_devices, with_use_mmap}`; `LlamaContextParams::{with_n_ctx, with_n_batch, with_n_ubatch, with_n_threads(_batch), with_type_k/v, with_flash_attention_policy, with_swa_full, with_offload_kqv, with_op_offload}`; `LlamaSampler::chain_simple` / `greedy`; `vocab().tokenize`. KV quantization is exposed (`KvCacheType::Q8_0`). The perf counters in `timings()` read zero unless `no_perf` is off, so the spike times the phases itself.

### Cross-compiling (not tried; from build.rs, docs and issues)

- **`aarch64-apple-ios`:** `build.rs` treats every `*-apple-*` target alike, apart from watchOS (which loses Metal). It keeps Metal on, turns BLAS off and links Foundation, Metal, MetalKit, Accelerate and libc++. cmake-rs supplies the iOS sysroot through `xcrun`, which needs **full Xcode with the iPhoneOS SDK**. Only the Command Line Tools are installed here, and `xcrun --sdk iphoneos --show-sdk-path` fails. bindgen also runs at build time against that SDK; open issue #1111 proposes generating bindings ahead of time so libclang isn't needed. There are no open iOS bugs. An "add iOS support" PR (#949) was closed unmerged with no description, and #1132 (2026-09) fixed a duplicate libc++ link on Apple targets. Expected recipe: install Xcode, `rustup target add aarch64-apple-ios`, set `GGML_CPU_ARM_ARCH`, build a staticlib inside `hushflow-core`. On a device, the notes run must stay in the foreground: MOBILE_SPEC M4 records that iOS forbids background GPU work and that ggml's Metal backend crashes when the work is cut off.
- **`aarch64-linux-android`:** supported in `build.rs`. It finds the NDK through `ANDROID_NDK_HOME`, `ANDROID_NDK_ROOT`, `NDK_HOME` or `$ANDROID_HOME/ndk/<latest>`, checks for `build/cmake/android.toolchain.cmake`, takes the API level from `ANDROID_API_LEVEL`, `ANDROID_PLATFORM` or `CARGO_NDK_ANDROID_PLATFORM`, and works with cargo-ndk's ABI names. It sets `GGML_LLAMAFILE=OFF`, links `log` and `android`, and offers `android-shared-stdcxx` (the default) or `android-static-stdcxx`. The CPU baseline is `armv8-a` unless `GGML_CPU_ARM_ARCH` is set (#1067). Recent fixes: an NDK triple with an API level (#1140, 2026-09), Vulkan on Android (#1084), and an `opencl` feature for Adreno (#1046). Open: Snapdragon/Hexagon wiring (#956) and Direct IO on Android (#910). An NDK, 27.3.13750724, is now present under `~/Library/Android/sdk/ndk` (installed by another agent). No Android Rust target is installed, and nothing was built for Android here.

## Recommendation

**8 GB phones: ship it, foreground only, Q4_K_M, mmap on.**

- On iPhone: link llama.cpp through `llama-cpp-2` into `hushflow-core` and run the notes on Metal after the meeting, while the app is in the foreground (MOBILE_SPEC M7). Keep the desktop's context sizing, f16 KV, the default ubatch and mmap. Load the model when notes start and free it when they finish (0.1 s). Ask for the `com.apple.developer.kernel.increased-memory-limit` entitlement, and check `os_proc_available_memory()` against about 4 GB before loading.
- Expected time on a phone, as an estimate rather than a measurement: phone GPUs have about a quarter of the M4 Pro's compute and memory bandwidth, so an hour-long meeting would take roughly 1.5–2.5 minutes, against 36 s here.
- The worst case is that every mapped page counts. That is about 3.9 GB, which fits under the limit of an 8 GB iPhone with the entitlement.
- On Android, the app can keep the desktop's separate-process design (the `:engine` process, MOBILE_SPEC M6), so in-process isn't required there. If llama.cpp is used instead of LiteRT-LM, set `GGML_CPU_ARM_ARCH` and treat the CPU path as good for meetings up to about 20 minutes (1.5–5k words: 40–110 s here, and slower on a phone).

**6 GB phones: don't plan on the resident model.**

- 3.8 GB (Q4_K_M) or 3.1–3.2 GB (Q3_K_M) of resident weights is above, or right at, the 3 GB or so an iOS app can expect on a 6 GB device. Context and KV knobs only move about 150 MB, and the smaller quant costs speed for about 0.6 GB.
- The only lean path is mmap. Its footprint here was 0.36–0.45 GB on Metal, and 1.2 GB of real weights plus about 0.6 GB of buffers in theory, because the 1.76 GB embedding table is used sparsely. That has to be proven on a real 6 GB iPhone under memory pressure before relying on it. **That device test is the next spike:** load Q4_K_M with mmap, run the 5k-word case on Metal, and log `phys_footprint` and jetsam events.
- If it fails: 6 GB iPhones (iPhone 13–15 non-Pro) don't get Apple Intelligence either, so use rules-only notes plus the transcript (MOBILE_SPEC's older-phone path), or let the desktop generate the notes after sync.
- On 6 GB Android phones, gate the feature on the measured available memory, and prefer LiteRT-LM, whose published peak for this model is 0.68 GB.

## Commands

```bash
cd spikes/gemma-inproc
# Build. CPU arch flag as recommended; Metal is always on.
GGML_CPU_ARM_ARCH=armv8.6-a+dotprod+i8mm+fp16 CARGO_TARGET_DIR=target/i8mm cargo build --release
python3 -I make_transcripts.py ../../docs/fork/bench/cleanup_eval.json "$DATA"

M="$HOME/Library/Application Support/com.hushflow.app/models/cleanup/gemma-4-E2B-it-Q4_K_M.gguf"
BIN=target/i8mm/release/gemma-inproc ./matrix.sh "$DATA" "$M" q4km          # full matrix
BIN=target/i8mm/release/gemma-inproc ./matrix.sh "$DATA" "$Q3" q3km quick    # smaller quant
# One run:
target/i8mm/release/gemma-inproc --model "$M" --transcript "$DATA/transcript_1500.txt" \
    [--cpu --threads 4] [--ctx 16384] [--kv q8_0] [--no-mmap] [--greedy] [--ubatch 256] \
    [--dump-prompt p.txt] [--out notes.md]
LLAMA_LOG=1 ...                       # llama.cpp logs, buffer sizes
python3 -I check_notes.py "$DATA/transcript_1500.txt" "$DATA"/*_1500.md

# Reference: the bundled server with the same request (temperature 0.2, max_tokens 1500,
# enable_thinking false, cache_prompt false) via /v1/chat/completions; /apply-template for the prompt.
src-tauri/resources/llama/llama-server -m "$M" --port 8197 -c 20480 -np 1 --jinja --no-webui
src-tauri/resources/llama/llama-server -m "$M" --port 8197 -c 8192 -np 1 --jinja --no-webui --device none -ngl 0 -t 4 -tb 4
```
