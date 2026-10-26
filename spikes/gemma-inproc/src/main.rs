//! Spike: write meeting notes with Gemma in-process (llama.cpp as a library,
//! no `llama-server` child), as a phone would have to.
//!
//! ```text
//! gemma-inproc --model <gguf> --transcript <txt> [--notes <txt>]
//!              [--cpu] [--threads N] [--ctx N|auto] [--kv f16|q8_0]
//!              [--max-tokens 1500] [--out notes.md] [--dump-prompt p.txt]
//!              [--no-mmap] [--greedy] [--seed N] [--ubatch 512] [--swa-full]
//! ```
//!
//! The request mirrors `src-tauri/src/managers/meeting/notes.rs`: the same
//! SYSTEM text, the same user message, temperature 0.2, max 1500 tokens, and
//! the same context sizing (`--ctx auto`). Sampling matches llama-server's
//! default chain at that temperature (top-k 40, top-p 0.95, min-p 0.05).
//! Timings go to stderr as one `RESULT` line.

use anyhow::{anyhow, bail, Context, Result};
use llama_cpp_2::context::params::{KvCacheType, LlamaContextParams};
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::LlamaModel;
use llama_cpp_2::sampling::LlamaSampler;
use std::io::Write;
use std::num::NonZeroU32;
use std::time::Instant;

/// Verbatim from `notes.rs`.
const SYSTEM: &str = "You write meeting notes from a meeting transcript. Lines are \"[mm:ss] Me: ...\" (the user) or \"[mm:ss] Them: ...\" (the other people on the call).

Write Markdown in exactly this shape:
# <a short, specific title for the meeting>

## Summary
2-4 sentences: what the meeting was about and where it landed.

## Notes
If the user wrote notes, keep every one of their points, in their order and wording, and add the detail from the transcript under each. Otherwise, the main topics as short bullets in your own words: summarize, never copy transcript lines.

## Decisions
- Each decision, ending with the [mm:ss] where it was made.

## Action items
- Owner: task, ending with the [mm:ss] where it was agreed. Use \"Me\" for the user.

## Open questions
- Anything left unresolved.

Rules:
- Use only what the transcript and the user's notes say. Never invent names, numbers, dates or decisions.
- Cite times exactly as they appear in the transcript.
- Leave a section with just \"- None\" when it has nothing.
- No preamble and nothing after the notes.";

// Context sizing, as in notes.rs.
const TOKENS_PER_WORD: f64 = 1.7;
const MAX_CTX: u32 = 32_768;
const OVERHEAD_TOKENS: u32 = 2_500;

struct Args {
    model: String,
    transcript: String,
    notes: Option<String>,
    cpu: bool,
    threads: i32,
    ctx: Option<u32>,
    kv_q8: bool,
    max_tokens: usize,
    out: Option<String>,
    dump_prompt: Option<String>,
    seed: u32,
    no_mmap: bool,
    greedy: bool,
    ubatch: u32,
    swa_full: bool,
}

fn parse_args() -> Result<Args> {
    let mut a = Args {
        model: String::new(),
        transcript: String::new(),
        notes: None,
        cpu: false,
        threads: 4,
        ctx: None,
        kv_q8: false,
        max_tokens: 1500,
        out: None,
        dump_prompt: None,
        seed: 1,
        no_mmap: false,
        greedy: false,
        ubatch: 512,
        swa_full: false,
    };
    let mut it = std::env::args().skip(1);
    while let Some(k) = it.next() {
        let mut v = || it.next().ok_or_else(|| anyhow!("{k} needs a value"));
        match k.as_str() {
            "--model" => a.model = v()?,
            "--transcript" => a.transcript = v()?,
            "--notes" => a.notes = Some(v()?),
            "--cpu" => a.cpu = true,
            "--threads" => a.threads = v()?.parse()?,
            "--ctx" => {
                let s = v()?;
                a.ctx = if s == "auto" { None } else { Some(s.parse()?) }
            }
            "--kv" => {
                a.kv_q8 = match v()?.as_str() {
                    "q8_0" => true,
                    "f16" => false,
                    o => bail!("--kv {o}: use f16 or q8_0"),
                }
            }
            "--max-tokens" => a.max_tokens = v()?.parse()?,
            "--out" => a.out = Some(v()?),
            "--dump-prompt" => a.dump_prompt = Some(v()?),
            "--seed" => a.seed = v()?.parse()?,
            "--no-mmap" => a.no_mmap = true,
            "--greedy" => a.greedy = true,
            "--ubatch" => a.ubatch = v()?.parse()?,
            "--swa-full" => a.swa_full = true,
            o => bail!("unknown argument {o}"),
        }
    }
    if a.model.is_empty() || a.transcript.is_empty() {
        bail!("--model and --transcript are required");
    }
    Ok(a)
}

/// Gemma 4's turn format, exactly as its Jinja template renders a system +
/// user conversation with `enable_thinking: false` (checked byte for byte
/// against llama-server's `/apply-template`). llama.cpp's built-in
/// `llama_chat_apply_template` only knows the Gemma 1-3 `<start_of_turn>`
/// format, so it can't be used for Gemma 4.
fn gemma4_prompt(system: &str, user: &str) -> String {
    format!("<|turn>system\n{system}<turn|>\n<|turn>user\n{user}<turn|>\n<|turn>model\n")
}

fn main() -> Result<()> {
    let args = parse_args()?;
    let t_start = Instant::now();

    let transcript = std::fs::read_to_string(&args.transcript)?;
    let user_notes = match &args.notes {
        Some(p) => std::fs::read_to_string(p)?,
        None => String::new(),
    };
    let lines: Vec<&str> = transcript
        .lines()
        .filter(|l| !l.trim().is_empty())
        .collect();
    let words: usize = lines
        .iter()
        .map(|l| l.split_whitespace().count())
        .sum::<usize>()
        + user_notes.split_whitespace().count();
    let ctx = args.ctx.unwrap_or_else(|| {
        ((words as f64 * TOKENS_PER_WORD) as u32 + OVERHEAD_TOKENS)
            .next_multiple_of(1024)
            .clamp(8_192, MAX_CTX)
    });

    let mut user = String::new();
    if !user_notes.trim().is_empty() {
        user.push_str("My notes:\n");
        user.push_str(user_notes.trim());
        user.push_str("\n\n");
    }
    user.push_str("Transcript:\n");
    user.push_str(&lines.join("\n"));
    let prompt = gemma4_prompt(SYSTEM, &user);
    if let Some(p) = &args.dump_prompt {
        std::fs::write(p, &prompt)?;
    }

    let t_init = Instant::now();
    let mut backend = LlamaBackend::init()?;
    if std::env::var_os("LLAMA_LOG").is_none() {
        backend.void_logs();
    }
    let init_s = t_init.elapsed().as_secs_f64();

    // --- load ---
    let t_load = Instant::now();
    // --no-mmap reads the weights into anonymous memory, so they count in the
    // physical footprint (the bound a phone has to plan for).
    let mut mparams = LlamaModelParams::default().with_use_mmap(!args.no_mmap);
    if args.cpu {
        // No GPU device at all (like `llama-server --device none`), so no op
        // is offloaded to Metal behind our back.
        mparams = mparams.with_n_gpu_layers(0).with_devices(&[])?;
    } else {
        mparams = mparams.with_n_gpu_layers(999);
    }
    let model = LlamaModel::load_from_file(&backend, &args.model, &mparams)
        .with_context(|| format!("loading {}", args.model))?;
    let load_s = t_load.elapsed().as_secs_f64();

    let t_ctx = Instant::now();
    let mut cparams = LlamaContextParams::default()
        .with_n_ctx(NonZeroU32::new(ctx))
        .with_n_batch(2048)
        .with_n_ubatch(args.ubatch)
        // llama-server's default; the C default keeps a full-size SWA cache.
        .with_swa_full(args.swa_full)
        .with_n_threads(args.threads)
        .with_n_threads_batch(args.threads);
    if args.kv_q8 {
        cparams = cparams
            .with_type_k(KvCacheType::Q8_0)
            .with_type_v(KvCacheType::Q8_0)
            // quantized V needs flash attention
            .with_flash_attention_policy(llama_cpp_sys_2::LLAMA_FLASH_ATTN_TYPE_ENABLED);
    }
    if args.cpu {
        cparams = cparams.with_offload_kqv(false).with_op_offload(false);
    }
    let mut lctx = model.new_context(&backend, cparams)?;
    let ctx_s = t_ctx.elapsed().as_secs_f64();
    if std::env::var_os("LLAMA_LOG").is_some() {
        lctx.print_memory_breakdown();
    }

    // --- prefill ---
    let vocab = model.vocab();
    let tokens = vocab.tokenize(prompt.as_bytes(), true, true);
    let n_prompt = tokens.len();
    if n_prompt + args.max_tokens > ctx as usize {
        bail!(
            "prompt {n_prompt} + {} new tokens > ctx {ctx}",
            args.max_tokens
        );
    }
    let t_prefill = Instant::now();
    let n_batch = 2048;
    let mut batch = LlamaBatch::new(n_batch, 1);
    for (ci, chunk) in tokens.chunks(n_batch).enumerate() {
        batch.clear();
        let last_chunk = (ci + 1) * n_batch >= n_prompt;
        for (j, &tok) in chunk.iter().enumerate() {
            let pos = (ci * n_batch + j) as i32;
            batch.add(tok, pos, &[0], last_chunk && j == chunk.len() - 1)?;
        }
        lctx.decode(&mut batch)?;
    }
    let prefill_s = t_prefill.elapsed().as_secs_f64();

    // --- decode ---
    let mut sampler = if args.greedy {
        LlamaSampler::greedy()
    } else {
        LlamaSampler::chain_simple([
            LlamaSampler::top_k(40),
            LlamaSampler::top_p(0.95, 1),
            LlamaSampler::min_p(0.05, 1),
            LlamaSampler::temp(0.2),
            LlamaSampler::dist(args.seed),
        ])
    };
    let t_decode = Instant::now();
    let mut out_bytes: Vec<u8> = Vec::new();
    let mut pos = n_prompt as i32;
    let mut n_gen = 0;
    let mut idx = batch.n_tokens() - 1;
    let stdout = std::io::stdout();
    while n_gen < args.max_tokens {
        let tok = sampler.sample(&lctx, idx);
        sampler.accept(tok);
        if vocab.is_eog(tok) {
            break;
        }
        let piece = vocab.token_to_piece(tok, false, None);
        out_bytes.extend_from_slice(&piece);
        if args.out.is_none() {
            let mut h = stdout.lock();
            h.write_all(&piece)?;
            h.flush()?;
        }
        n_gen += 1;
        batch.clear();
        batch.add(tok, pos, &[0], true)?;
        pos += 1;
        idx = 0;
        lctx.decode(&mut batch)?;
    }
    let decode_s = t_decode.elapsed().as_secs_f64();
    let text = String::from_utf8_lossy(&out_bytes).trim().to_string();
    if let Some(p) = &args.out {
        std::fs::write(p, &text)?;
    }
    let total_s = t_start.elapsed().as_secs_f64();

    // What an app pays to give the memory back.
    let t_free = Instant::now();
    drop(batch);
    drop(sampler);
    drop(lctx);
    drop(model);
    drop(backend);
    let free_s = t_free.elapsed().as_secs_f64();

    eprintln!(
        "RESULT mode={} mmap={} ubatch={} threads={} ctx={} kv={} words={} prompt_tok={} gen_tok={} \
         init_s={:.2} load_s={:.2} ctx_s={:.2} prefill_s={:.2} prefill_tps={:.0} \
         decode_s={:.2} decode_tps={:.1} total_s={:.2} free_s={:.2}",
        if args.cpu { "cpu" } else { "metal" },
        !args.no_mmap,
        args.ubatch,
        args.threads,
        ctx,
        if args.kv_q8 { "q8_0" } else { "f16" },
        words,
        n_prompt,
        n_gen,
        init_s,
        load_s,
        ctx_s,
        prefill_s,
        n_prompt as f64 / prefill_s,
        decode_s,
        n_gen as f64 / decode_s,
        total_s,
        free_s,
    );
    Ok(())
}
