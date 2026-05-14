use super::consts::*;
use super::helpers::*;
use super::types::*;

/// Context-aware completion for achedit command
pub(super) fn complete_achedit(words: &[&str], completing_word: bool, achievement_keys: &[String]) -> CompletionResult {
    let partial = get_partial(words, completing_word);

    match words.len() {
        // achedit <partial>
        2 if completing_word => {
            let mut combined: Vec<String> = ACHEDIT_SUBCOMMANDS
                .iter()
                .filter(|s| s.starts_with(partial.as_str()))
                .map(|s| s.to_string())
                .collect();
            combined.extend(
                achievement_keys
                    .iter()
                    .filter(|k| k.to_lowercase().starts_with(partial.as_str()))
                    .cloned(),
            );
            CompletionResult::new(combined, &partial, CompletionType::AcheditSubcommand)
        }
        // achedit <subcmd/key> - show subcommands
        2 if !completing_word => all_static(ACHEDIT_SUBCOMMANDS, CompletionType::AcheditSubcommand),
        // achedit <key> <partial_subcmd>
        3 if completing_word => filter_static(ACHEDIT_SUBCOMMANDS, &partial, CompletionType::AcheditSubcommand),
        // achedit <key> category <partial_cat>
        4 if completing_word && words[2].to_lowercase() == "category" => {
            filter_static(ACHIEVEMENT_CATEGORIES, &partial, CompletionType::AchievementCategory)
        }
        // achedit <key> category - show categories
        3 if !completing_word && words[2].to_lowercase() == "category" => {
            all_static(ACHIEVEMENT_CATEGORIES, CompletionType::AchievementCategory)
        }
        // achedit <key> reward <partial_action>
        4 if completing_word && words[2].to_lowercase() == "reward" => {
            filter_static(ACHIEVEMENT_REWARD_ACTIONS, &partial, CompletionType::AchievementRewardAction)
        }
        // achedit <key> reward - show reward actions
        3 if !completing_word && words[2].to_lowercase() == "reward" => {
            all_static(ACHIEVEMENT_REWARD_ACTIONS, CompletionType::AchievementRewardAction)
        }
        // achedit <key> criterion <partial_action>
        4 if completing_word && words[2].to_lowercase() == "criterion" => {
            filter_static(ACHIEVEMENT_CRITERION_ACTIONS, &partial, CompletionType::AchievementCriterionAction)
        }
        // achedit <key> criterion - show criterion actions
        3 if !completing_word && words[2].to_lowercase() == "criterion" => {
            all_static(ACHIEVEMENT_CRITERION_ACTIONS, CompletionType::AchievementCriterionAction)
        }
        // achedit <key> criterion skill <partial_skill>
        5 if completing_word && words[2].to_lowercase() == "criterion" && words[3].to_lowercase() == "skill" => {
            // We don't have skill list here easily, but we can filter by SKILL_NAMES
            filter_static(SKILL_NAMES, &partial, CompletionType::SkillName)
        }
        _ => CompletionResult::empty(),
    }
}
