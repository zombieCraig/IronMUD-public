//! DG-script context awareness for the modern editor.
//!
//! Phase 4 adds three DG-specific layers on top of the base editor:
//! - **Syntax highlighting**: tokenize each rendered line and wrap
//!   spans in ANSI colour codes.
//! - **Tab completion**: cycle through keyword / command / variable
//!   candidates based on the cursor's context (after `%` → variables;
//!   at start-of-line → keywords + commands).
//! - **Live syntax check**: re-parse the buffer on each edit and
//!   surface the first error line in the footer + an underline marker
//!   on that body row.
//!
//! All three only activate when the editor is hosting a
//! `EditorKind::DgTriggerBody` session and the player has colour
//! output enabled.

use crate::script::dg::cmds::COMMANDS;
use crate::script::dg::parser::{self, KEYWORDS, VARIABLES};

// ANSI colour codes. 16-colour palette only — works on every client
// that negotiated `MTTS_ANSI`. 256-colour / truecolour upgrades belong
// in Phase 5 along with the configurable palette.
const RESET: &str = "\x1b[0m";
const KEYWORD: &str = "\x1b[36m"; // cyan
const COMMAND: &str = "\x1b[32m"; // green
const VARIABLE: &str = "\x1b[33m"; // yellow
const COMMENT: &str = "\x1b[90m"; // bright black / grey

/// One coloured span of a syntax-highlighted line. The renderer joins
/// spans with the chosen ANSI prefix and a trailing reset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub text: String,
    pub kind: TokenKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    Plain,
    Keyword,
    Command,
    Variable,
    Comment,
}

impl TokenKind {
    fn ansi_prefix(self) -> &'static str {
        match self {
            TokenKind::Plain => "",
            TokenKind::Keyword => KEYWORD,
            TokenKind::Command => COMMAND,
            TokenKind::Variable => VARIABLE,
            TokenKind::Comment => COMMENT,
        }
    }
}

/// Tokenize a single DG line into coloured spans. The caller chooses
/// whether to actually emit ANSI codes (off when the player has colour
/// disabled). Plain spans carry no escape codes, so an all-`Plain` line
/// renders identically to the raw input.
pub fn tokenize_line(line: &str) -> Vec<Span> {
    let trimmed = line.trim_start();
    // Comments are line-level: a `*` at the first non-whitespace column
    // turns the entire line grey.
    if trimmed.starts_with('*') {
        return vec![Span {
            text: line.to_string(),
            kind: TokenKind::Comment,
        }];
    }

    let mut spans: Vec<Span> = Vec::new();
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0usize;
    let mut keyword_phase = true; // first non-whitespace token gets the keyword/cmd check

    while i < chars.len() {
        let c = chars[i];

        // Leading whitespace before the first token.
        if c.is_whitespace() && keyword_phase {
            let start = i;
            while i < chars.len() && chars[i].is_whitespace() && chars[i] != '\n' {
                i += 1;
            }
            push_plain(&mut spans, &chars[start..i]);
            continue;
        }

        // % introduces a variable substitution.
        if c == '%' {
            // Close-percent must follow on the same line for highlight;
            // otherwise emit the lone '%' as one plain char so we still
            // advance the cursor. (Without this branch, falling through
            // to the "plain until next %" loop below would spin forever
            // because the loop refuses to step over the '%' it's on.)
            if let Some(close) = chars[i + 1..].iter().position(|c| *c == '%') {
                let end = i + 1 + close + 1; // inclusive of closing %
                let span_text: String = chars[i..end].iter().collect();
                spans.push(Span {
                    text: span_text,
                    kind: TokenKind::Variable,
                });
                i = end;
                keyword_phase = false;
                continue;
            } else {
                push_plain(&mut spans, &[c]);
                i += 1;
                keyword_phase = false;
                continue;
            }
        }

        // First-token check: is this a keyword or command verb?
        if keyword_phase && is_word_char(c) {
            let start = i;
            while i < chars.len() && is_word_char(chars[i]) {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            let lc = word.to_ascii_lowercase();
            let kind = if KEYWORDS.contains(&lc.as_str()) {
                TokenKind::Keyword
            } else if COMMANDS.contains(&lc.as_str()) {
                TokenKind::Command
            } else {
                TokenKind::Plain
            };
            spans.push(Span { text: word, kind });
            keyword_phase = false;
            continue;
        }

        // Anything else flows through as plain text until the next `%`.
        let start = i;
        while i < chars.len() && chars[i] != '%' {
            i += 1;
        }
        push_plain(&mut spans, &chars[start..i]);
        // Once we've passed the leading token, future words are plain.
        keyword_phase = false;
    }

    spans
}

fn push_plain(spans: &mut Vec<Span>, chars: &[char]) {
    if chars.is_empty() {
        return;
    }
    let text: String = chars.iter().collect();
    if let Some(last) = spans.last_mut() {
        if last.kind == TokenKind::Plain {
            last.text.push_str(&text);
            return;
        }
    }
    spans.push(Span {
        text,
        kind: TokenKind::Plain,
    });
}

fn is_word_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// Render a tokenized line as a byte string with ANSI codes. When
/// `colour` is false, only the plain text is emitted.
pub fn render_line(spans: &[Span], colour: bool) -> String {
    let mut out = String::new();
    for span in spans {
        if colour && span.kind != TokenKind::Plain {
            out.push_str(span.kind.ansi_prefix());
            out.push_str(&span.text);
            out.push_str(RESET);
        } else {
            out.push_str(&span.text);
        }
    }
    out
}

/// Tab-completion source for DG bodies. Given the line up to the cursor,
/// returns the candidate replacement strings (longest match first).
///
/// Rules:
/// - If the prefix ends with a `%`-started token, complete against
///   `VARIABLES` (emit `%name%` form).
/// - Else if the cursor is in the first token of the line, complete
///   against `KEYWORDS ∪ COMMANDS`.
/// - Otherwise return an empty list.
pub fn completions_for(prefix_before_cursor: &str) -> Vec<String> {
    // First, look for an open `%name` segment in the trailing token.
    if let Some(pct_pos) = prefix_before_cursor.rfind('%') {
        let trailing = &prefix_before_cursor[pct_pos + 1..];
        // If a closing `%` already appeared after the open, the token
        // is complete and we fall through to plain-token completion.
        if !trailing.contains('%') {
            let lc = trailing.to_ascii_lowercase();
            return VARIABLES
                .iter()
                .filter(|v| v.starts_with(&lc))
                .map(|v| format!("%{}%", v))
                .collect();
        }
    }

    // First-token completion: every char up to here must be word-or-space,
    // and the trailing word is what we extend.
    let trimmed = prefix_before_cursor.trim_start();
    let first_token_only = !trimmed.contains(char::is_whitespace);
    if first_token_only {
        let lc = trimmed.to_ascii_lowercase();
        let mut out: Vec<String> = KEYWORDS
            .iter()
            .chain(COMMANDS.iter())
            .filter(|s| s.starts_with(&lc))
            .map(|s| s.to_string())
            .collect();
        out.sort();
        out.dedup();
        return out;
    }

    Vec::new()
}

/// Re-parse a DG body and return the first error's `(line_number, message)`
/// if the parse fails. Phase 4 surfaces only one error at a time; the
/// existing parser short-circuits on first failure, which matches.
pub fn syntax_check(body: &str) -> Option<(usize, String)> {
    match parser::parse(body) {
        Ok(_) => None,
        Err(e) => Some((e.line, e.msg)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(line: &str) -> Vec<TokenKind> {
        tokenize_line(line).into_iter().map(|s| s.kind).collect()
    }

    #[test]
    fn comment_line_is_one_grey_span() {
        let s = tokenize_line("* this is a comment");
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].kind, TokenKind::Comment);
    }

    #[test]
    fn indented_comment_is_one_grey_span() {
        let s = tokenize_line("    * indented comment");
        assert_eq!(s[0].kind, TokenKind::Comment);
    }

    #[test]
    fn keyword_at_line_start_is_highlighted() {
        let _ = kinds; // suppress dead-code warning when only used here
        let spans = tokenize_line("if some condition");
        assert_eq!(spans[0].text, "if");
        assert_eq!(spans[0].kind, TokenKind::Keyword);
    }

    #[test]
    fn command_at_line_start_is_highlighted() {
        let spans = tokenize_line("mecho hello there");
        assert_eq!(spans[0].text, "mecho");
        assert_eq!(spans[0].kind, TokenKind::Command);
    }

    #[test]
    fn variable_in_middle_of_line_is_yellow() {
        let spans = tokenize_line("send %actor.name% greet");
        let kinds: Vec<_> = spans.iter().map(|s| s.kind).collect();
        assert!(kinds.contains(&TokenKind::Variable));
        assert!(spans.iter().any(|s| s.text == "%actor.name%"));
    }

    #[test]
    fn unmatched_percent_passes_as_plain() {
        let spans = tokenize_line("send 50%% of the time");
        assert!(!spans.is_empty());
    }

    #[test]
    fn lone_percent_does_not_infinite_loop() {
        // Regression: tokenizer would spin forever on a single unmatched
        // `%` because the fall-through plain-text loop refused to step
        // past the character it was sitting on.
        let spans = tokenize_line("%");
        assert!(!spans.is_empty());
        assert_eq!(spans.iter().map(|s| s.text.as_str()).collect::<String>(), "%");
    }

    #[test]
    fn percent_then_partial_word_no_loop() {
        let spans = tokenize_line("send %act");
        let joined: String = spans.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(joined, "send %act");
    }

    #[test]
    fn percent_at_end_of_line_no_loop() {
        let spans = tokenize_line("if a %");
        let joined: String = spans.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(joined, "if a %");
    }

    #[test]
    fn render_strips_ansi_when_colour_off() {
        let spans = tokenize_line("if %actor.name% bigger");
        let monochrome = render_line(&spans, false);
        assert_eq!(monochrome, "if %actor.name% bigger");
        assert!(!monochrome.contains('\x1b'));
    }

    #[test]
    fn render_emits_ansi_when_colour_on() {
        let spans = tokenize_line("if %actor.name% bigger");
        let coloured = render_line(&spans, true);
        assert!(coloured.contains("\x1b[36m"), "keyword cyan: {coloured:?}");
        assert!(coloured.contains("\x1b[33m"), "variable yellow: {coloured:?}");
    }

    #[test]
    fn completion_first_token_returns_keywords_and_commands() {
        let c = completions_for("se");
        assert!(c.contains(&"send".to_string()));
        assert!(c.contains(&"set".to_string()));
    }

    #[test]
    fn completion_variable_after_percent() {
        let c = completions_for("send %act");
        assert!(c.iter().any(|s| s == "%actor%"));
        assert!(c.iter().any(|s| s == "%actor.name%"));
    }

    #[test]
    fn completion_returns_empty_mid_line() {
        let c = completions_for("send actor more text");
        assert!(c.is_empty());
    }

    #[test]
    fn syntax_check_passes_on_well_formed_body() {
        let body = "if 1\n  mecho hi\nend";
        assert!(syntax_check(body).is_none());
    }

    #[test]
    fn syntax_check_reports_first_error() {
        let body = "if 1\n  mecho hi";
        // Missing `end` should produce a parse error with a line number.
        let err = syntax_check(body);
        assert!(err.is_some(), "expected error for missing 'end'");
    }
}
