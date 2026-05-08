//! Internal helpers shared by the per-command completers.

use unicode_width::UnicodeWidthStr;

use super::types::{ArgumentContext, CompletionResult, CompletionType};

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
        "aedit" | "adelete" | "spedit" | "areset" | "acreate" => ArgumentContext::AreaPrefix,

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
    let mut prefix_len = first.len();

    for s in &strings[1..] {
        let common_len = first
            .chars()
            .zip(s.chars())
            .take_while(|(a, b)| a.eq_ignore_ascii_case(b))
            .count();
        prefix_len = prefix_len.min(common_len);
    }

    first[..prefix_len].to_string()
}

/// Format completion result for display
pub fn format_completions(result: &CompletionResult, max_width: u16) -> String {
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
    for chunk in result.completions.chunks(cols) {
        let line: Vec<String> = chunk
            .iter()
            .map(|s| {
                // Pad to col_width based on display width, not byte length
                let display_len = s.width();
                let padding = col_width.saturating_sub(display_len);
                format!("{}{}", s, " ".repeat(padding))
            })
            .collect();
        lines.push(line.join(""));
    }

    lines.join("\n")
}
