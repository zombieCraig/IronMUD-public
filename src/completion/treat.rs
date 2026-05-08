use super::consts::*;
use super::helpers::*;
use super::types::*;

/// Context-aware completion for treat command
/// Syntax: treat <target> [body_part or condition]
pub(super) fn complete_treat(words: &[&str], completing_word: bool, online_players: &[String]) -> CompletionResult {
    let partial = get_partial(words, completing_word);

    match words.len() {
        // treat - show "self" and online players
        1 if !completing_word => {
            let mut matches = vec!["self".to_string()];
            matches.extend(online_players.iter().cloned());
            CompletionResult::new(matches, "", CompletionType::TreatTarget)
        }
        // treat <partial_target> - complete "self" or player name
        2 if completing_word => {
            let mut matches: Vec<String> = Vec::new();
            if "self".starts_with(&partial) {
                matches.push("self".to_string());
            }
            matches.extend(
                online_players
                    .iter()
                    .filter(|p| p.to_lowercase().starts_with(&partial))
                    .cloned(),
            );
            CompletionResult::new(matches, &partial, CompletionType::TreatTarget)
        }
        // treat <target> - show body parts and conditions
        2 if !completing_word => all_static(TREAT_TARGETS, CompletionType::BodyPart),
        // treat <target> <partial_part_or_condition> - complete body part or condition
        3 if completing_word => {
            let matches: Vec<String> = TREAT_TARGETS
                .iter()
                .filter(|s| s.starts_with(&partial))
                .map(|s| s.to_string())
                .collect();
            // Determine appropriate completion type based on what's matching
            let completion_type = if matches.iter().all(|m| TREATABLE_CONDITIONS.contains(&m.as_str())) {
                CompletionType::TreatableCondition
            } else if matches.iter().all(|m| BODY_PARTS.contains(&m.as_str())) {
                CompletionType::BodyPart
            } else {
                CompletionType::BodyPart // Mixed results default to BodyPart
            };
            CompletionResult::new(matches, &partial, completion_type)
        }
        _ => CompletionResult::empty(),
    }
}
