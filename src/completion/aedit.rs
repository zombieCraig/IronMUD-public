use super::consts::*;
use super::helpers::*;
use super::types::*;

/// Context-aware completion for aedit command
pub(super) fn complete_aedit(words: &[&str], completing_word: bool, area_prefixes: &[String]) -> CompletionResult {
    let partial = get_partial(words, completing_word);

    match words.len() {
        // aedit <partial> - complete to either an area prefix or a subcommand
        // (aedit.rhai accepts a leading subcommand and defaults area to current room)
        2 if completing_word => {
            let mut combined: Vec<String> = AEDIT_SUBCOMMANDS
                .iter()
                .filter(|s| s.starts_with(partial.as_str()))
                .map(|s| s.to_string())
                .collect();
            combined.extend(
                area_prefixes
                    .iter()
                    .filter(|p| p.to_lowercase().starts_with(partial.as_str()))
                    .cloned(),
            );
            CompletionResult::new(combined, &partial, CompletionType::AeditSubcommand)
        }
        // aedit immigration - show immigration subcommands (no area prefix; area inferred from current room)
        2 if !completing_word && words[1].to_lowercase() == "immigration" => {
            all_static(IMMIGRATION_SUBCOMMANDS, CompletionType::ImmigrationSubcommand)
        }
        // aedit <area> - show all subcommands
        2 if !completing_word => all_static(AEDIT_SUBCOMMANDS, CompletionType::AeditSubcommand),
        // aedit immigration <partial_subcmd> - complete immigration subcommand (no area)
        3 if completing_word && words[1].to_lowercase() == "immigration" => {
            filter_static(IMMIGRATION_SUBCOMMANDS, &partial, CompletionType::ImmigrationSubcommand)
        }
        // aedit <area> <partial_subcmd> - complete subcommand
        3 if completing_word => filter_static(AEDIT_SUBCOMMANDS, &partial, CompletionType::AeditSubcommand),
        // aedit <area> permission - show all permission levels
        3 if !completing_word && words[2].to_lowercase() == "permission" => {
            all_static(PERMISSION_LEVELS, CompletionType::PermissionLevel)
        }
        // aedit <area> permission <partial_level> - complete permission level
        4 if completing_word && words[2].to_lowercase() == "permission" => {
            filter_static(PERMISSION_LEVELS, &partial, CompletionType::PermissionLevel)
        }
        // aedit <area> zone - show zone types
        3 if !completing_word && words[2].to_lowercase() == "zone" => {
            all_static(AREA_ZONE_TYPES, CompletionType::AreaZoneType)
        }
        // aedit <area> zone <partial_type> - complete zone type
        4 if completing_word && words[2].to_lowercase() == "zone" => {
            filter_static(AREA_ZONE_TYPES, &partial, CompletionType::AreaZoneType)
        }
        // aedit <area> flags - show area flags
        3 if !completing_word && words[2].to_lowercase() == "flags" => all_static(AREA_FLAGS, CompletionType::AreaFlag),
        // aedit <area> flags <partial_flag> - complete area flag
        4 if completing_word && words[2].to_lowercase() == "flags" => {
            filter_static(AREA_FLAGS, &partial, CompletionType::AreaFlag)
        }
        // aedit <area> forage - show forage types
        3 if !completing_word && words[2].to_lowercase() == "forage" => {
            all_static(FORAGE_TYPES, CompletionType::ForageType)
        }
        // aedit <area> forage <partial_type> - complete forage type
        4 if completing_word && words[2].to_lowercase() == "forage" => {
            filter_static(FORAGE_TYPES, &partial, CompletionType::ForageType)
        }
        // aedit <area> forage <type> - show forage actions
        4 if !completing_word && words[2].to_lowercase() == "forage" => {
            all_static(FORAGE_ACTIONS, CompletionType::ForageAction)
        }
        // aedit <area> forage <type> <partial_action> - complete forage action
        5 if completing_word && words[2].to_lowercase() == "forage" => {
            filter_static(FORAGE_ACTIONS, &partial, CompletionType::ForageAction)
        }
        // aedit <area> immigration - show immigration subcommands
        3 if !completing_word && words[2].to_lowercase() == "immigration" => {
            all_static(IMMIGRATION_SUBCOMMANDS, CompletionType::ImmigrationSubcommand)
        }
        // aedit <area> immigration <partial_subcmd> - complete immigration subcommand
        4 if completing_word && words[2].to_lowercase() == "immigration" => {
            filter_static(IMMIGRATION_SUBCOMMANDS, &partial, CompletionType::ImmigrationSubcommand)
        }
        _ => CompletionResult::empty(),
    }
}
