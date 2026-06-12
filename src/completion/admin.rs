use super::consts::*;
use super::helpers::*;
use super::types::*;

/// Context-aware completion for admin command
pub(super) fn complete_admin(
    words: &[&str],
    completing_word: bool,
    online_players: &[String],
    class_ids: &[String],
    race_ids: &[String],
) -> CompletionResult {
    let partial = get_partial(words, completing_word);

    // `loadout` and a few other subcommands have several completion levels;
    // dispatch them to dedicated helpers so this top-level match stays readable.
    if words.len() >= 2 && words[1].to_lowercase() == "loadout" {
        return complete_loadout(words, completing_word, class_ids, race_ids);
    }
    if words.len() >= 2 && words[1].to_lowercase() == "account" {
        return complete_account(words, completing_word, &partial);
    }
    if words.len() >= 2 && words[1].to_lowercase() == "world" {
        return complete_world(words, completing_word, &partial);
    }

    match words.len() {
        // admin - show all subcommands
        1 if !completing_word => all_static(ADMIN_SUBCOMMANDS, CompletionType::AdminSubcommand),
        // admin <partial_subcommand> - complete subcommand
        2 if completing_word => filter_static(ADMIN_SUBCOMMANDS, &partial, CompletionType::AdminSubcommand),
        // admin <subcommand> - show next level options
        2 if !completing_word => {
            let subcommand = words[1].to_lowercase();
            match subcommand.as_str() {
                "kick" | "summon" | "heal" | "morality" | "embrace" | "embrace_revoke" | "masquerade_reset" => {
                    all_dynamic(online_players, CompletionType::PlayerName)
                }
                "user" => all_static(ADMIN_USER_ACTIONS, CompletionType::AdminUserAction),
                "api-key" => all_static(ADMIN_API_KEY_ACTIONS, CompletionType::AdminApiKeyAction),
                _ => CompletionResult::empty(),
            }
        }
        // admin <subcommand> <partial> - complete next level
        3 if completing_word => {
            let subcommand = words[1].to_lowercase();
            match subcommand.as_str() {
                "kick" | "summon" | "heal" | "morality" | "embrace" | "embrace_revoke" | "masquerade_reset" => {
                    filter_dynamic(online_players, &partial, CompletionType::PlayerName)
                }
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

/// Completion for `admin loadout ...`. Levels:
///   admin loadout <list|class|race|help>
///   admin loadout <class|race> <id>
///   admin loadout <class|race> <id> <show|gold|items>
///   admin loadout <class|race> <id> items <add|remove|clear>
fn complete_loadout(
    words: &[&str],
    completing_word: bool,
    class_ids: &[String],
    race_ids: &[String],
) -> CompletionResult {
    let partial = get_partial(words, completing_word);

    // The kind ("class"/"race") determines which id pool to draw from.
    let kind = if words.len() >= 3 {
        words[2].to_lowercase()
    } else {
        String::new()
    };
    let ids: &[String] = match kind.as_str() {
        "class" => class_ids,
        "race" => race_ids,
        _ => &[],
    };

    match words.len() {
        // admin loadout - show targets
        2 if !completing_word => all_static(ADMIN_LOADOUT_TARGETS, CompletionType::AdminSubcommand),
        // admin loadout <partial_target>
        3 if completing_word => filter_static(ADMIN_LOADOUT_TARGETS, &partial, CompletionType::AdminSubcommand),
        // admin loadout <class|race> - show ids
        3 if !completing_word => match kind.as_str() {
            "class" | "race" => all_dynamic(ids, CompletionType::LoadoutId),
            _ => CompletionResult::empty(),
        },
        // admin loadout <class|race> <partial_id>
        4 if completing_word => match kind.as_str() {
            "class" | "race" => filter_dynamic(ids, &partial, CompletionType::LoadoutId),
            _ => CompletionResult::empty(),
        },
        // admin loadout <class|race> <id> - show actions
        4 if !completing_word => all_static(ADMIN_LOADOUT_ACTIONS, CompletionType::AdminSubcommand),
        // admin loadout <class|race> <id> <partial_action>
        5 if completing_word => filter_static(ADMIN_LOADOUT_ACTIONS, &partial, CompletionType::AdminSubcommand),
        // admin loadout <class|race> <id> items - show item actions
        5 if !completing_word => {
            if words[4].to_lowercase() == "items" {
                all_static(ADMIN_LOADOUT_ITEM_ACTIONS, CompletionType::AdminSubcommand)
            } else {
                CompletionResult::empty()
            }
        }
        // admin loadout <class|race> <id> items <partial_item_action>
        6 if completing_word => {
            if words[4].to_lowercase() == "items" {
                filter_static(ADMIN_LOADOUT_ITEM_ACTIONS, &partial, CompletionType::AdminSubcommand)
            } else {
                CompletionResult::empty()
            }
        }
        _ => CompletionResult::empty(),
    }
}

/// Completion for `admin account <action> [name]`.
fn complete_account(words: &[&str], completing_word: bool, partial: &str) -> CompletionResult {
    match words.len() {
        // admin account - show actions
        2 if !completing_word => all_static(ADMIN_ACCOUNT_ACTIONS, CompletionType::AdminUserAction),
        // admin account <partial_action>
        3 if completing_word => filter_static(ADMIN_ACCOUNT_ACTIONS, partial, CompletionType::AdminUserAction),
        _ => CompletionResult::empty(),
    }
}

/// Completion for `admin world <action>`.
fn complete_world(words: &[&str], completing_word: bool, partial: &str) -> CompletionResult {
    match words.len() {
        // admin world - show actions
        2 if !completing_word => all_static(ADMIN_WORLD_ACTIONS, CompletionType::AdminSubcommand),
        // admin world <partial_action>
        3 if completing_word => filter_static(ADMIN_WORLD_ACTIONS, partial, CompletionType::AdminSubcommand),
        _ => CompletionResult::empty(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn classes() -> Vec<String> {
        vec!["warrior".to_string(), "wizard".to_string(), "thief".to_string()]
    }
    fn races() -> Vec<String> {
        vec!["human".to_string(), "elf".to_string()]
    }

    fn run(input: &str) -> CompletionResult {
        let words: Vec<&str> = input.split_whitespace().collect();
        let completing_word = !input.is_empty() && !input.ends_with(' ');
        complete_admin(&words, completing_word, &[], &classes(), &races())
    }

    #[test]
    fn loadout_appears_in_subcommands() {
        let r = run("admin ");
        assert!(r.completions.contains(&"loadout".to_string()));
    }

    #[test]
    fn loadout_subcommand_prefix_completes() {
        let r = run("admin loa");
        assert_eq!(r.completions, vec!["loadout".to_string()]);
    }

    #[test]
    fn loadout_targets_listed() {
        let r = run("admin loadout ");
        assert_eq!(r.completions, vec!["list", "class", "race", "help"]);
    }

    #[test]
    fn loadout_class_ids_listed() {
        let r = run("admin loadout class ");
        assert_eq!(r.completions, classes());
        assert_eq!(r.completion_type, CompletionType::LoadoutId);
    }

    #[test]
    fn loadout_class_id_prefix_filters() {
        let r = run("admin loadout class w");
        assert_eq!(r.completions, vec!["warrior".to_string(), "wizard".to_string()]);
    }

    #[test]
    fn loadout_race_ids_listed() {
        let r = run("admin loadout race ");
        assert_eq!(r.completions, races());
    }

    #[test]
    fn loadout_kit_actions_listed() {
        let r = run("admin loadout class warrior ");
        assert_eq!(r.completions, vec!["show", "gold", "items"]);
    }

    #[test]
    fn loadout_item_actions_listed() {
        let r = run("admin loadout class warrior items ");
        assert_eq!(r.completions, vec!["add", "remove", "clear"]);
    }

    #[test]
    fn account_and_world_actions_listed() {
        assert!(run("admin account ").completions.contains(&"set-email".to_string()));
        assert_eq!(run("admin world ").completions, vec!["info", "clear"]);
    }
}
