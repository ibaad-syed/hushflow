//! Spoken formatting commands: "new line" and "new paragraph" become line
//! breaks, as in Wispr Flow and macOS dictation.
//!
//! A command only counts when it stands alone ("…the deck. New line. Thanks"),
//! i.e. it is followed by punctuation or ends the text, so ordinary phrases
//! like "a new line of products" are left as they are.

use once_cell::sync::Lazy;
use regex::Regex;

static COMMAND: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?i)[ \t,]*\b(new paragraph|next paragraph|new line|newline|next line)\b(?:[.,!?;:]+|$)[ \t]*",
    )
    .expect("spoken formatting pattern is valid")
});

/// Replaces standalone "new line" / "new paragraph" commands with line breaks
/// and capitalizes the text that follows each break.
pub fn apply_spoken_formatting(text: &str) -> String {
    if !COMMAND.is_match(text) {
        return text.to_string();
    }
    let replaced = COMMAND.replace_all(text, |caps: &regex::Captures| {
        if caps[1].to_lowercase().contains("paragraph") {
            "\n\n"
        } else {
            "\n"
        }
    });

    // Capitalize the first letter after each break.
    let mut out = String::with_capacity(replaced.len());
    let mut capitalize = false;
    for c in replaced.trim().chars() {
        if c == '\n' {
            capitalize = true;
            out.push(c);
        } else if capitalize && c.is_alphabetic() {
            out.extend(c.to_uppercase());
            capitalize = false;
        } else {
            if !c.is_whitespace() {
                capitalize = false;
            }
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::apply_spoken_formatting as fmt;

    #[test]
    fn new_line_between_sentences() {
        assert_eq!(
            fmt("Send me the deck. New line. Thanks, Sam."),
            "Send me the deck.\nThanks, Sam."
        );
    }

    #[test]
    fn new_paragraph_and_capitalization() {
        assert_eq!(
            fmt("Hi team, new paragraph, the launch moved to Friday."),
            "Hi team\n\nThe launch moved to Friday."
        );
    }

    #[test]
    fn trailing_command_is_dropped() {
        assert_eq!(fmt("First item. New line."), "First item.");
    }

    #[test]
    fn ordinary_phrases_are_untouched() {
        let text = "We need a new line of products and the next line item.";
        assert_eq!(fmt(text), text);
    }
}
