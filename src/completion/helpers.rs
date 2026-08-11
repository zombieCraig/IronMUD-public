//! Internal helpers shared by the per-command completers.

use unicode_width::UnicodeWidthStr;

use super::types::{ArgumentContext, CompletionCategory, CompletionResult, CompletionType};

const ANSI_RESET: &str = "\x1b[0m";

/// The single place a completion category maps to an escape sequence.
/// Bright cyan for socials so `re<TAB>` visibly separates the emote
/// `reconnect` from real actions like `rest`.
pub fn category_ansi(category: CompletionCategory) -> &'static str {
    match category {
        CompletionCategory::Plain => "",
        CompletionCategory::Social => "\x1b[1;36m",
    }
}

/// Get the argument context for a command
pub fn get_argument_context(command: &str) -> ArgumentContext {
    match command.to_lowercase().as_str() {
        // Room vnum commands
        "rgoto" | "redit" | "rdelete" | "link" | "unlink" | "rcopy" => ArgumentContext::RoomVnum,

        // Item vnum commands
        "oedit" | "ospawn" | "idelete" | "orefresh" => ArgumentContext::ItemVnum,

        // Mobile vnum commands
        "medit" | "mspawn" | "mdelete" | "mrefresh" => ArgumentContext::MobileVnum,

        // Area prefix commands
        "aedit" | "adelete" | "spedit" | "areset" | "acreate" | "agoto" => ArgumentContext::AreaPrefix,

        // Direction commands
        "go" | "dig" | "snipe" => ArgumentContext::Direction,

        // Player name commands
        "tell" | "whisper" => ArgumentContext::PlayerName,

        // Skill name commands
        "recipes" => ArgumentContext::SkillName,

        // Recipe vnum commands
        "recedit" | "recdelete" => ArgumentContext::RecipeVnum,

        // Transport vnum commands
        "tedit" => ArgumentContext::TransportVnum,

        // Property template vnum commands
        "pedit" | "pdelete" | "upgrade" | "tour" | "rent" => ArgumentContext::PropertyTemplateVnum,

        // Visit uses player names
        "visit" => ArgumentContext::PlayerName,

        // Shop preset vnum commands
        "bpredit" => ArgumentContext::ShopPresetVnum,

        // Plant vnum commands
        "plantedit" => ArgumentContext::PlantVnum,

        // Spell name commands
        "cast" => ArgumentContext::SpellName,

        "speak" => ArgumentContext::Language,

        "talk" => ArgumentContext::MobInRoom,

        _ => ArgumentContext::None,
    }
}

/// Helper: Filter static options by prefix
pub fn filter_static(options: &[&str], partial: &str, comp_type: CompletionType) -> CompletionResult {
    let matches: Vec<String> = options
        .iter()
        .filter(|s| s.starts_with(partial))
        .map(|s| s.to_string())
        .collect();
    CompletionResult::new(matches, partial, comp_type)
}

/// Helper: Return all static options (no filtering)
pub fn all_static(options: &[&str], comp_type: CompletionType) -> CompletionResult {
    CompletionResult::new(options.iter().map(|s| s.to_string()).collect(), "", comp_type)
}

/// Helper: Filter dynamic (runtime) options by prefix
pub fn filter_dynamic(options: &[String], partial: &str, comp_type: CompletionType) -> CompletionResult {
    let matches: Vec<String> = options
        .iter()
        .filter(|v| v.to_lowercase().starts_with(partial))
        .cloned()
        .collect();
    CompletionResult::new(matches, partial, comp_type)
}

/// Helper: Return all dynamic options (no filtering)
pub fn all_dynamic(options: &[String], comp_type: CompletionType) -> CompletionResult {
    CompletionResult::new(options.to_vec(), "", comp_type)
}

/// Helper: Extract partial from words array
pub fn get_partial(words: &[&str], completing_word: bool) -> String {
    if completing_word {
        words.last().unwrap_or(&"").to_lowercase()
    } else {
        String::new()
    }
}

/// Find the longest common prefix among a list of strings
pub fn find_common_prefix(strings: &[String]) -> String {
    if strings.is_empty() {
        return String::new();
    }
    if strings.len() == 1 {
        return strings[0].clone();
    }

    let first = &strings[0];
    // Characters throughout: `common_len` below counts characters, so seeding
    // this with `first.len()` (bytes) and slicing by the result mixed the two
    // — a single accented candidate made TAB either cut in the wrong place or
    // panic outright.
    let mut prefix_len = first.chars().count();

    for s in &strings[1..] {
        let common_len = first
            .chars()
            .zip(s.chars())
            .take_while(|(a, b)| a.eq_ignore_ascii_case(b))
            .count();
        prefix_len = prefix_len.min(common_len);
    }

    crate::text::truncate_chars(first, prefix_len).to_string()
}

/// Format completion result for display.
///
/// Column maths always uses the *plain* display width, so the grid lines up
/// identically whether or not colour is on. Colour, when enabled, wraps only
/// the candidate text — the reset lands before the padding so no attribute
/// bleeds across the gutter.
pub fn format_completions(result: &CompletionResult, max_width: u16, colors_enabled: bool) -> String {
    if result.is_empty() {
        return String::new();
    }

    if result.is_unique() {
        // Single match - no need to display list
        return String::new();
    }

    // Calculate column width using display width for proper emoji/CJK handling
    let max_item_width = result.completions.iter().map(|s| s.width()).max().unwrap_or(0);
    let col_width = max_item_width + 2; // Add padding
    let cols = ((max_width as usize) / col_width).max(1);

    // Format as columns with proper padding for display width
    let mut lines = Vec::new();
    for (chunk_idx, chunk) in result.completions.chunks(cols).enumerate() {
        let line: Vec<String> = chunk
            .iter()
            .enumerate()
            .map(|(i, s)| {
                // Pad to col_width based on display width, not byte length
                let display_len = s.width();
                let padding = col_width.saturating_sub(display_len);
                let color = if colors_enabled {
                    category_ansi(result.category_at(chunk_idx * cols + i))
                } else {
                    ""
                };
                if color.is_empty() {
                    format!("{}{}", s, " ".repeat(padding))
                } else {
                    format!("{}{}{}{}", color, s, ANSI_RESET, " ".repeat(padding))
                }
            })
            .collect();
        lines.push(line.join(""));
    }

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strip_ansi(s: &str) -> String {
        let mut out = String::new();
        let mut in_escape = false;
        for c in s.chars() {
            if c == '\x1b' {
                in_escape = true;
            } else if in_escape {
                if c == 'm' {
                    in_escape = false;
                }
            } else {
                out.push(c);
            }
        }
        out
    }

    fn mixed() -> CompletionResult {
        CompletionResult::new_categorized(
            vec![
                "read".to_string(),
                "reconnect".to_string(),
                "recline".to_string(),
                "rest".to_string(),
            ],
            vec![
                CompletionCategory::Plain,
                CompletionCategory::Social,
                CompletionCategory::Social,
                CompletionCategory::Plain,
            ],
            "re",
            CompletionType::Command,
        )
    }

    #[test]
    fn colors_off_emits_no_escapes() {
        let out = format_completions(&mixed(), 80, false);
        assert!(!out.contains('\x1b'), "expected plain output, got {:?}", out);
        assert_eq!(
            out, "read       reconnect  recline    rest       ",
            "plain rendering must be unchanged"
        );
    }

    #[test]
    fn socials_are_wrapped_in_bright_cyan() {
        let out = format_completions(&mixed(), 80, true);
        assert!(out.contains("\x1b[1;36mreconnect\x1b[0m"));
        assert!(out.contains("\x1b[1;36mrecline\x1b[0m"));
        // Plain entries stay untouched.
        assert!(!out.contains("\x1b[1;36mread"));
        assert!(!out.contains("\x1b[1;36mrest"));
    }

    #[test]
    fn color_does_not_disturb_column_alignment() {
        let plain = format_completions(&mixed(), 80, false);
        let colored = format_completions(&mixed(), 80, true);
        assert_eq!(strip_ansi(&colored), plain);
    }

    #[test]
    fn alignment_holds_when_the_grid_wraps() {
        // col_width = 9 + 2 = 11, so a 24-col window fits 2 per row.
        let plain = format_completions(&mixed(), 24, false);
        let colored = format_completions(&mixed(), 24, true);
        assert_eq!(plain.lines().count(), 2);
        assert_eq!(strip_ansi(&colored), plain);
    }

    #[test]
    fn empty_and_unique_results_render_nothing() {
        assert_eq!(format_completions(&CompletionResult::empty(), 80, true), "");
        let unique = CompletionResult::new(vec!["wave".to_string()], "wa", CompletionType::Command);
        assert_eq!(format_completions(&unique, 80, true), "");
    }
}
