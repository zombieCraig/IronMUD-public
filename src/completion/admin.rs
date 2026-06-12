use super::consts::*;
use super::helpers::*;
use super::types::*;

/// Context-aware completion for admin command
pub(super) fn complete_admin(words: &[&str], completing_word: bool, online_players: &[String]) -> CompletionResult {
    let partial = get_partial(words, completing_word);

    match words.len() {
        // admin - show all subcommands
        1 if !completing_word => all_static(ADMIN_SUBCOMMANDS, CompletionType::AdminSubcommand),
        // admin <partial_subcommand> - complete subcommand
        2 if completing_word => filter_static(ADMIN_SUBCOMMANDS, &partial, CompletionType::AdminSubcommand),
        // admin <subcommand> - show next level options
        2 if !completing_word => {
            let subcommand = words[1].to_lowercase();
            match subcommand.as_str() {
                "kick" | "summon" | "heal" => all_dynamic(online_players, CompletionType::PlayerName),
                "user" => all_static(ADMIN_USER_ACTIONS, CompletionType::AdminUserAction),
                "api-key" => all_static(ADMIN_API_KEY_ACTIONS, CompletionType::AdminApiKeyAction),
                _ => CompletionResult::empty(),
            }
        }
        // admin <subcommand> <partial> - complete next level
        3 if completing_word => {
            let subcommand = words[1].to_lowercase();
            match subcommand.as_str() {
                "kick" | "summon" | "heal" => filter_dynamic(online_players, &partial, CompletionType::PlayerName),
                "user" => filter_static(ADMIN_USER_ACTIONS, &partial, CompletionType::AdminUserAction),
                "api-key" => filter_static(ADMIN_API_KEY_ACTIONS, &partial, CompletionType::AdminApiKeyAction),
                _ => CompletionResult::empty(),
            }
        }
        // admin user <action> - show player names for actions that need them
        3 if !completing_word => {
            let subcommand = words[1].to_lowercase();
            if subcommand == "user" {
                let action = words[2].to_lowercase();
                match action.as_str() {
                    "info" | "grant-admin" | "revoke-admin" | "grant-builder" | "revoke-builder" | "password"
                    | "delete" => all_dynamic(online_players, CompletionType::PlayerName),
                    _ => CompletionResult::empty(),
                }
            } else {
                CompletionResult::empty()
            }
        }
        // admin user <action> <partial_player> - complete player name
        4 if completing_word => {
            let subcommand = words[1].to_lowercase();
            if subcommand == "user" {
                let action = words[2].to_lowercase();
                match action.as_str() {
                    "info" | "grant-admin" | "revoke-admin" | "grant-builder" | "revoke-builder" | "password"
                    | "delete" => filter_dynamic(online_players, &partial, CompletionType::PlayerName),
                    _ => CompletionResult::empty(),
                }
            } else {
                CompletionResult::empty()
            }
        }
        _ => CompletionResult::empty(),
    }
}
