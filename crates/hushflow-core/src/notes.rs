//! Meeting notes from the transcript and the user's own notes, Granola-style:
//! the user's bullets are expanded with what was said, and the rest fills a
//! fixed template.
//!
//! This is the platform-free half of the desktop's
//! `src-tauri/src/managers/meeting/notes.rs`: the prompt, the context budget,
//! the citation check and the title. Running the model is the caller's job
//! (llama-server on the desktop, Gemma in-process on mobile). Keep the two
//! copies in step until the desktop depends on this crate.
//!
//! The transcript is never replaced: notes are a separate, versioned layer,
//! and every cited [mm:ss] is checked against it.

use crate::store::{MeetingSegment, CHANNEL_MIC};
use anyhow::{anyhow, Result};
use once_cell::sync::Lazy;
use regex::Regex;

pub const SYSTEM: &str = "You write meeting notes from a meeting transcript. Lines are \"[mm:ss] Me: ...\" (the user) or \"[mm:ss] Them: ...\" (the other people on the call).

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

/// Tokens per word for English speech with timestamps, generously.
pub const TOKENS_PER_WORD: f64 = 1.7;
pub const MAX_CTX: u32 = 32_768;
/// Room for the instructions and the notes the model writes.
pub const OVERHEAD_TOKENS: u32 = 2_500;

pub fn clock(ms: i64) -> String {
    let s = ms / 1000;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    } else {
        format!("{:02}:{:02}", s / 60, s % 60)
    }
}

fn speaker(channel: &str) -> &'static str {
    if channel == CHANNEL_MIC {
        "Me"
    } else {
        "Them"
    }
}

/// The transcript as the model (and the Markdown export) reads it.
pub fn transcript_lines(segments: &[MeetingSegment]) -> Vec<String> {
    segments
        .iter()
        .filter(|s| !s.suppressed_echo)
        .map(|s| format!("[{}] {}: {}", clock(s.t0_ms), speaker(&s.channel), s.text))
        .collect()
}

/// Keeps the last lines that fit `max_words`, for a meeting longer than the
/// context. Rare: 32k tokens is about two hours.
pub fn fit(lines: &[String], max_words: usize) -> (usize, &[String]) {
    let mut words = 0;
    let mut start = lines.len();
    while start > 0 {
        let w = lines[start - 1].split_whitespace().count();
        if words + w > max_words {
            break;
        }
        words += w;
        start -= 1;
    }
    (start, &lines[start..])
}

/// What to send the model: the system and user messages, and the context
/// size the whole exchange needs.
#[derive(Debug, Clone, PartialEq)]
pub struct Prompt {
    pub system: &'static str,
    pub user: String,
    pub ctx: u32,
    /// Transcript lines left out from the start of a very long meeting.
    pub dropped_lines: usize,
}

/// Builds the notes prompt from the transcript and the user's notes.
pub fn build_prompt(segments: &[MeetingSegment], user_notes: &str) -> Result<Prompt> {
    let lines = transcript_lines(segments);
    if lines.is_empty() && user_notes.trim().is_empty() {
        return Err(anyhow!("nothing was said"));
    }
    let budget_words = ((MAX_CTX - OVERHEAD_TOKENS) as f64 / TOKENS_PER_WORD) as usize
        - user_notes.split_whitespace().count().min(2_000);
    let (dropped, kept) = fit(&lines, budget_words);
    let words: usize = kept
        .iter()
        .map(|l| l.split_whitespace().count())
        .sum::<usize>()
        + user_notes.split_whitespace().count();
    let ctx = ((words as f64 * TOKENS_PER_WORD) as u32 + OVERHEAD_TOKENS)
        .next_multiple_of(1024)
        .clamp(8_192, MAX_CTX);

    let mut user = String::new();
    if !user_notes.trim().is_empty() {
        user.push_str("My notes:\n");
        user.push_str(user_notes.trim());
        user.push_str("\n\n");
    }
    if dropped > 0 {
        user.push_str("(The start of this long meeting is left out.)\n");
    }
    user.push_str("Transcript:\n");
    user.push_str(&kept.join("\n"));
    Ok(Prompt {
        system: SYSTEM,
        user,
        ctx,
        dropped_lines: dropped,
    })
}

static CITATION: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\[(\d{1,2}:)?(\d{1,2}):(\d{2})\]").unwrap());

fn citation_ms(c: &regex::Captures) -> i64 {
    let h: i64 = c
        .get(1)
        .map(|m| m.as_str().trim_end_matches(':').parse().unwrap_or(0))
        .unwrap_or(0);
    let m: i64 = c[2].parse().unwrap_or(0);
    let s: i64 = c[3].parse().unwrap_or(0);
    ((h * 60 + m) * 60 + s) * 1000
}

/// Marks lines citing a time the transcript doesn't have, so a made-up
/// decision can't pass as sourced.
pub fn check_citations(notes: &str, segments: &[MeetingSegment]) -> String {
    let starts: Vec<i64> = segments
        .iter()
        .filter(|s| !s.suppressed_echo)
        .map(|s| s.t0_ms / 1000 * 1000)
        .collect();
    notes
        .lines()
        .map(|line| {
            let unknown = CITATION
                .captures_iter(line)
                .any(|c| !starts.iter().any(|&t| (t - citation_ms(&c)).abs() <= 2000));
            if unknown {
                format!("{line} _(unverified)_")
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The title the notes give, if any.
pub fn title_of(notes: &str) -> Option<String> {
    notes
        .lines()
        .find_map(|l| l.strip_prefix("# "))
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty() && t.len() <= 120)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(channel: &str, t0: i64, text: &str) -> MeetingSegment {
        MeetingSegment {
            id: 0,
            channel: channel.into(),
            t0_ms: t0,
            t1_ms: t0 + 2000,
            text: text.into(),
            suppressed_echo: false,
        }
    }

    #[test]
    fn transcript_lines_label_speakers_and_skip_echo() {
        let mut echo = seg(CHANNEL_MIC, 4_000, "Ship Tuesday");
        echo.suppressed_echo = true;
        let lines = transcript_lines(&[
            seg(CHANNEL_MIC, 1_500, "Hi all"),
            seg("system", 65_000, "Ship Tuesday"),
            echo,
        ]);
        assert_eq!(
            lines,
            vec!["[00:01] Me: Hi all", "[01:05] Them: Ship Tuesday"]
        );
        assert_eq!(clock(3_725_000), "1:02:05");
    }

    #[test]
    fn citations_not_in_the_transcript_are_marked() {
        let segs = [seg(CHANNEL_MIC, 65_400, "Ship Tuesday")];
        let notes = "- Ship on Tuesday [01:05]\n- Budget doubled [07:30]\n- No time here";
        let checked = check_citations(notes, &segs);
        assert_eq!(
            checked,
            "- Ship on Tuesday [01:05]\n- Budget doubled [07:30] _(unverified)_\n- No time here"
        );
    }

    #[test]
    fn title_comes_from_the_first_heading() {
        assert_eq!(
            title_of("# Q3 launch sync\n\n## Summary"),
            Some("Q3 launch sync".into())
        );
        assert_eq!(title_of("## Summary"), None);
    }

    #[test]
    fn long_meetings_keep_their_most_recent_lines() {
        let lines: Vec<String> = (0..10).map(|i| format!("w{i} a b")).collect();
        let (dropped, kept) = fit(&lines, 9);
        assert_eq!(dropped, 7);
        assert_eq!(kept.len(), 3);
    }

    #[test]
    fn prompt_carries_user_notes_then_transcript() {
        let segs = [seg(CHANNEL_MIC, 1_000, "Ship Tuesday")];
        let p = build_prompt(&segs, "  - launch date  ").unwrap();
        assert_eq!(
            p.user,
            "My notes:\n- launch date\n\nTranscript:\n[00:01] Me: Ship Tuesday"
        );
        assert_eq!(p.ctx, 8_192);
        assert_eq!(p.dropped_lines, 0);
        assert!(build_prompt(&[], " ").is_err());
    }
}
