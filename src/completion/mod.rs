//! Tab completion engine for IronMUD
//!
//! Provides context-aware completion for:
//! - Command names
//! - Room vnums (for rgoto, redit, link, etc.)
//! - Item vnums (for oedit, ospawn, etc.)
//! - Mobile vnums (for medit, mspawn, etc.)
//! - Area prefixes (for aedit, spedit, etc.)

mod consts;
mod helpers;
mod types;

pub use consts::MOBILE_FLAGS;
pub use helpers::{category_ansi, format_completions};
pub use types::{ArgumentContext, CompletionCategory, CompletionResult, CompletionType};

use consts::DIRECTIONS;
use consts::SKILL_NAMES;
#[cfg(test)]
use helpers::find_common_prefix;
use helpers::get_argument_context;

mod achedit;
mod admin;
mod aedit;
mod bpredit;
mod medit;
mod misc;
mod oedit;
mod plantedit;
mod recedit;
mod redit;
mod spedit;
mod summon;
mod tedit;
mod treat;

use achedit::complete_achedit;
use admin::complete_admin;
use aedit::complete_aedit;
use bpredit::complete_bpredit;
use medit::complete_medit;
use misc::{
    complete_bank, complete_bounty, complete_bugs, complete_build, complete_consignments, complete_escrow,
    complete_locate, complete_mail, complete_motd, complete_pedit, complete_press, complete_prompt, complete_property,
    complete_set, complete_standing, complete_top, complete_world,
};
use oedit::complete_oedit;
use plantedit::complete_plantedit;
use recedit::{complete_recedit, complete_reclist};
use redit::{complete_rcopy, complete_redit};
use spedit::complete_spedit;
use summon::complete_summon;
use tedit::complete_tedit;
use treat::complete_treat;

/// Everything the completer is allowed to know about the world and the player.
///
/// This was twenty-one positional parameters, and every new completion source
/// meant editing fifty call sites to add another `&[]`. A struct with `Default`
/// makes the next source one field and one line at the one real caller — the
/// tests name only what they exercise.
///
/// All fields are `Copy`, so `complete` destructures it into the locals its
/// body already used.
#[derive(Default, Clone, Copy)]
pub struct CompletionData<'a> {
    pub available_commands: &'a [String],
    pub room_vnums: &'a [String],
    pub item_vnums: &'a [String],
    pub mobile_vnums: &'a [String],
    pub area_prefixes: &'a [String],
    pub recipe_vnums: &'a [String],
    pub transport_vnums: &'a [String],
    pub property_template_vnums: &'a [String],
    pub shop_preset_vnums: &'a [String],
    pub plant_vnums: &'a [String],
    pub quest_vnums: &'a [String],
    pub spell_names: &'a [String],
    pub language_keys: &'a [String],
    pub online_players: &'a [String],
    /// Keywords of the mobiles standing in the player's room.
    pub mobs_in_room: &'a [String],
    /// Keywords of the items lying in the player's room.
    pub items_in_room: &'a [String],
    /// class_ids / race_ids drive `admin loadout class|race <id>` completion.
    pub class_ids: &'a [String],
    pub race_ids: &'a [String],
    pub achievement_keys: &'a [String],
    pub custom_skill_keys: &'a [String],
    pub faction_keys: &'a [String],
    /// Lowercased, **sorted** names of commands whose `CommandMeta.kind` is
    /// `"social"`. Display only — it tints the completion list so a player
    /// can tell the emote `reconnect` from a real action. Never affects
    /// matching or ordering.
    pub social_commands: &'a [String],
    pub is_builder: bool,
}

/// Tag each command candidate by whether it is a virtual social entry.
fn categorize_commands(commands: &[String], social_commands: &[String]) -> Vec<CompletionCategory> {
    commands
        .iter()
        .map(|cmd| {
            if social_commands.binary_search(&cmd.to_lowercase()).is_ok() {
                CompletionCategory::Social
            } else {
                CompletionCategory::Plain
            }
        })
        .collect()
}

pub fn complete(input: &str, cursor_pos: usize, data: &CompletionData) -> CompletionResult {
    let &CompletionData {
        available_commands,
        room_vnums,
        item_vnums,
        mobile_vnums,
        area_prefixes,
        recipe_vnums,
        transport_vnums,
        property_template_vnums,
        shop_preset_vnums,
        plant_vnums,
        spell_names,
        language_keys,
        online_players,
        mobs_in_room,
        class_ids,
        race_ids,
        achievement_keys,
        custom_skill_keys,
        faction_keys,
        social_commands,
        is_builder,
        // `quest_vnums` / `items_in_room` are read straight off `data` by the
        // sub-completer that wants them.
        ..
    } = data;

    // Get the portion of input up to cursor
    let input_to_cursor = if cursor_pos <= input.len() {
        &input[..cursor_pos]
    } else {
        input
    };

    // Split into words
    let words: Vec<&str> = input_to_cursor.split_whitespace().collect();

    // Check if we're completing a word or starting a new one
    let completing_word = !input_to_cursor.is_empty() && !input_to_cursor.ends_with(' ');

    match words.len() {
        0 => {
            // Empty input - return all commands
            let matches = available_commands.to_vec();
            let categories = categorize_commands(&matches, social_commands);
            CompletionResult::new_categorized(matches, categories, "", CompletionType::Command)
        }
        1 if completing_word => {
            // Completing first word (command name)
            let partial = words[0].to_lowercase();
            let matches: Vec<String> = available_commands
                .iter()
                .filter(|cmd| cmd.to_lowercase().starts_with(&partial))
                .cloned()
                .collect();
            let categories = categorize_commands(&matches, social_commands);
            CompletionResult::new_categorized(matches, categories, &partial, CompletionType::Command)
        }
        _ => {
            // Completing an argument
            let command = words[0];
            let context = get_argument_context(command);
            let partial = if completing_word {
                words.last().unwrap_or(&"").to_lowercase()
            } else {
                String::new()
            };

            match context {
                ArgumentContext::RoomVnum => {
                    // For redit, provide context-aware completion (edits current room)
                    if command.to_lowercase() == "redit" {
                        return complete_redit(&words, completing_word);
                    }
                    // For rcopy, provide vnum + category completion
                    if command.to_lowercase() == "rcopy" {
                        return complete_rcopy(&words, completing_word, room_vnums);
                    }
                    // Default room vnum completion for rgoto, rdelete, link, unlink
                    let matches: Vec<String> = room_vnums
                        .iter()
                        .filter(|v| v.to_lowercase().starts_with(&partial))
                        .cloned()
                        .collect();
                    CompletionResult::new(matches, &partial, CompletionType::RoomVnum)
                }
                ArgumentContext::ItemVnum => {
                    // For oedit, provide context-aware completion based on position
                    if command.to_lowercase() == "oedit" {
                        return complete_oedit(
                            &words,
                            completing_word,
                            item_vnums,
                            transport_vnums,
                            spell_names,
                            custom_skill_keys,
                        );
                    }
                    // Default item vnum completion for ospawn, idelete
                    let matches: Vec<String> = item_vnums
                        .iter()
                        .filter(|v| v.to_lowercase().starts_with(&partial))
                        .cloned()
                        .collect();
                    CompletionResult::new(matches, &partial, CompletionType::ItemVnum)
                }
                ArgumentContext::MobileVnum => {
                    // For medit, provide context-aware completion based on position
                    if command.to_lowercase() == "medit" {
                        return complete_medit(
                            &words,
                            completing_word,
                            mobile_vnums,
                            item_vnums,
                            transport_vnums,
                            property_template_vnums,
                            shop_preset_vnums,
                            spell_names,
                        );
                    }
                    // Default mobile vnum completion for mspawn, mdelete
                    let matches: Vec<String> = mobile_vnums
                        .iter()
                        .filter(|v| v.to_lowercase().starts_with(&partial))
                        .cloned()
                        .collect();
                    CompletionResult::new(matches, &partial, CompletionType::MobileVnum)
                }
                ArgumentContext::AreaPrefix => {
                    // For aedit, provide context-aware completion based on position
                    if command.to_lowercase() == "aedit" {
                        return complete_aedit(&words, completing_word, area_prefixes);
                    }
                    // For spedit, provide context-aware completion based on position
                    if command.to_lowercase() == "spedit" {
                        return complete_spedit(
                            &words,
                            completing_word,
                            area_prefixes,
                            room_vnums,
                            mobile_vnums,
                            item_vnums,
                        );
                    }
                    // Default area prefix completion for adelete, areset, acreate
                    let matches: Vec<String> = area_prefixes
                        .iter()
                        .filter(|v| v.to_lowercase().starts_with(&partial))
                        .cloned()
                        .collect();
                    CompletionResult::new(matches, &partial, CompletionType::AreaPrefix)
                }
                ArgumentContext::Direction => {
                    let matches: Vec<String> = DIRECTIONS
                        .iter()
                        .filter(|d| d.starts_with(&partial))
                        .map(|s| s.to_string())
                        .collect();
                    CompletionResult::new(matches, &partial, CompletionType::Direction)
                }
                ArgumentContext::PlayerName => {
                    let matches: Vec<String> = online_players
                        .iter()
                        .filter(|p| p.to_lowercase().starts_with(&partial))
                        .cloned()
                        .collect();
                    CompletionResult::new(matches, &partial, CompletionType::PlayerName)
                }
                ArgumentContext::SkillName => {
                    let matches: Vec<String> = SKILL_NAMES
                        .iter()
                        .filter(|s| s.starts_with(&partial))
                        .map(|s| s.to_string())
                        .collect();
                    CompletionResult::new(matches, &partial, CompletionType::SkillName)
                }
                ArgumentContext::RecipeVnum => {
                    // For recedit, provide context-aware completion based on position
                    if command.to_lowercase() == "recedit" {
                        return complete_recedit(&words, completing_word, recipe_vnums, item_vnums);
                    }
                    // Default recipe vnum completion for recdelete
                    let matches: Vec<String> = recipe_vnums
                        .iter()
                        .filter(|v| v.to_lowercase().starts_with(&partial))
                        .cloned()
                        .collect();
                    CompletionResult::new(matches, &partial, CompletionType::RecipeVnum)
                }
                ArgumentContext::TransportVnum => {
                    // For tedit, provide context-aware completion based on position
                    return complete_tedit(&words, completing_word, transport_vnums, room_vnums);
                }
                ArgumentContext::PropertyTemplateVnum => {
                    // For pedit, provide context-aware completion based on position
                    if command.to_lowercase() == "pedit" {
                        return complete_pedit(&words, completing_word, property_template_vnums);
                    }
                    // Default property template vnum completion for upgrade, tour, rent
                    let matches: Vec<String> = property_template_vnums
                        .iter()
                        .filter(|v| v.to_lowercase().starts_with(&partial))
                        .cloned()
                        .collect();
                    CompletionResult::new(matches, &partial, CompletionType::PropertyTemplateVnum)
                }
                ArgumentContext::ShopPresetVnum => {
                    return complete_bpredit(&words, completing_word, shop_preset_vnums);
                }
                ArgumentContext::PlantVnum => {
                    return complete_plantedit(&words, completing_word, plant_vnums, item_vnums);
                }
                ArgumentContext::SpellName => {
                    let matches: Vec<String> = spell_names
                        .iter()
                        .filter(|s| s.to_lowercase().starts_with(&partial))
                        .cloned()
                        .collect();
                    CompletionResult::new(matches, &partial, CompletionType::SpellName)
                }
                ArgumentContext::Language => {
                    let matches: Vec<String> = language_keys
                        .iter()
                        .filter(|k| k.to_lowercase().starts_with(&partial))
                        .cloned()
                        .collect();
                    CompletionResult::new(matches, &partial, CompletionType::SpellName)
                }
                ArgumentContext::MobInRoom => {
                    let matches: Vec<String> = mobs_in_room
                        .iter()
                        .filter(|m| m.to_lowercase().starts_with(&partial))
                        .cloned()
                        .collect();
                    CompletionResult::new(matches, &partial, CompletionType::PlayerName)
                }
                ArgumentContext::None => {
                    // Handle "set" command specially
                    if command.to_lowercase() == "set" {
                        return complete_set(&words, completing_word, is_builder);
                    }
                    // Handle reclist command
                    if command.to_lowercase() == "reclist" {
                        return complete_reclist(&words, completing_word);
                    }
                    // Handle admin command
                    if command.to_lowercase() == "admin" {
                        return complete_admin(&words, completing_word, online_players, class_ids, race_ids);
                    }
                    // Handle treat command
                    if command.to_lowercase() == "treat" {
                        return complete_treat(&words, completing_word, online_players);
                    }
                    // Handle press command
                    if command.to_lowercase() == "press" {
                        return complete_press(&words, completing_word);
                    }
                    // Handle property command
                    if command.to_lowercase() == "property" {
                        return complete_property(&words, completing_word, online_players);
                    }
                    // Handle mail command
                    if command.to_lowercase() == "mail" {
                        return complete_mail(&words, completing_word, online_players);
                    }
                    // Handle bank command
                    if command.to_lowercase() == "bank" {
                        return complete_bank(&words, completing_word);
                    }
                    // Handle escrow command
                    if command.to_lowercase() == "escrow" {
                        return complete_escrow(&words, completing_word);
                    }
                    // Handle prompt command
                    if command.to_lowercase() == "prompt" {
                        return complete_prompt(&words, completing_word);
                    }
                    // Handle top command
                    if command.to_lowercase() == "top" {
                        return complete_top(&words, completing_word);
                    }
                    // Handle build command
                    if command.to_lowercase() == "build" {
                        return complete_build(&words, completing_word, data);
                    }
                    // Handle world command
                    if command.to_lowercase() == "world" {
                        return complete_world(&words, completing_word);
                    }
                    // Handle bounty command
                    if command.to_lowercase() == "bounty" {
                        return complete_bounty(&words, completing_word);
                    }
                    // Handle standing command
                    if command.to_lowercase() == "standing" {
                        return complete_standing(&words, completing_word, faction_keys);
                    }
                    // Handle consignments command
                    if command.to_lowercase() == "consignments" {
                        return complete_consignments(&words, completing_word);
                    }
                    // Handle locate command
                    if command.to_lowercase() == "locate" {
                        return complete_locate(&words, completing_word);
                    }
                    // Handle motd command
                    if command.to_lowercase() == "motd" {
                        return complete_motd(&words, completing_word);
                    }
                    // Handle bugs command
                    if command.to_lowercase() == "bugs" {
                        return complete_bugs(&words, completing_word);
                    }
                    // Handle summon command
                    if command.to_lowercase() == "summon" {
                        return complete_summon(&words, completing_word, mobile_vnums, online_players, room_vnums);
                    }
                    // Handle achedit (achievement editor)
                    if command.to_lowercase() == "achedit" {
                        return complete_achedit(&words, completing_word, achievement_keys);
                    }
                    CompletionResult::empty()
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_complete_command() {
        let commands = vec![
            "look".to_string(),
            "login".to_string(),
            "logout".to_string(),
            "help".to_string(),
        ];

        let result = complete(
            "lo",
            2,
            &CompletionData {
                available_commands: &commands,
                ..Default::default()
            },
        );
        assert_eq!(result.completions.len(), 3);
        assert!(result.completions.contains(&"look".to_string()));
        assert!(result.completions.contains(&"login".to_string()));
        assert!(result.completions.contains(&"logout".to_string()));
        assert_eq!(result.completion_type, CompletionType::Command);
    }

    #[test]
    fn test_command_completion_tags_socials() {
        let commands = vec![
            "read".to_string(),
            "reconnect".to_string(),
            "recline".to_string(),
            "rest".to_string(),
        ];
        // Sorted, lowercase — the contract `categorize_commands` relies on.
        let socials = vec!["recline".to_string(), "reconnect".to_string()];

        let result = complete(
            "re",
            2,
            &CompletionData {
                available_commands: &commands,
                social_commands: &socials,
                ..Default::default()
            },
        );

        // Matching is untouched: same candidates, same order.
        assert_eq!(result.completions, commands);
        assert_eq!(
            result.categories,
            vec![
                CompletionCategory::Plain,
                CompletionCategory::Social,
                CompletionCategory::Social,
                CompletionCategory::Plain,
            ]
        );
    }

    #[test]
    fn test_non_command_completions_are_plain() {
        let commands = vec!["rgoto".to_string()];
        let room_vnums = vec!["town:square".to_string(), "town:tavern".to_string()];

        let result = complete(
            "rgoto town:",
            11,
            &CompletionData {
                available_commands: &commands,
                room_vnums: &room_vnums,
                ..Default::default()
            },
        );
        assert_eq!(result.completions.len(), 2);
        assert!(result.categories.iter().all(|c| *c == CompletionCategory::Plain));
    }

    #[test]
    fn test_complete_room_vnum() {
        let commands = vec!["rgoto".to_string()];
        let room_vnums = vec![
            "town:square".to_string(),
            "town:tavern".to_string(),
            "forest:entrance".to_string(),
        ];

        let result = complete(
            "rgoto town:",
            11,
            &CompletionData {
                available_commands: &commands,
                room_vnums: &room_vnums,
                ..Default::default()
            },
        );
        assert_eq!(result.completions.len(), 2);
        assert!(result.completions.contains(&"town:square".to_string()));
        assert!(result.completions.contains(&"town:tavern".to_string()));
        assert_eq!(result.completion_type, CompletionType::RoomVnum);
    }

    #[test]
    fn test_common_prefix() {
        let strings = vec![
            "town:square".to_string(),
            "town:tavern".to_string(),
            "town:market".to_string(),
        ];
        assert_eq!(find_common_prefix(&strings), "town:");
    }

    #[test]
    fn test_empty_input() {
        let commands = vec!["look".to_string(), "help".to_string()];
        let result = complete(
            "",
            0,
            &CompletionData {
                available_commands: &commands,
                ..Default::default()
            },
        );
        assert_eq!(result.completions.len(), 2);
    }

    #[test]
    fn test_direction_completion() {
        let commands = vec!["go".to_string()];
        let result = complete(
            "go nor",
            6,
            &CompletionData {
                available_commands: &commands,
                ..Default::default()
            },
        );
        assert_eq!(result.completions.len(), 3); // north, northeast, northwest
        assert!(result.completions.contains(&"north".to_string()));
        assert!(result.completions.contains(&"northeast".to_string()));
        assert!(result.completions.contains(&"northwest".to_string()));
    }

    #[test]
    fn test_medit_subcommand_completion() {
        let commands = vec!["medit".to_string()];
        let mobile_vnums = vec!["town:guard".to_string()];

        // Complete subcommand after vnum
        let result = complete(
            "medit town:guard tr",
            19,
            &CompletionData {
                available_commands: &commands,
                mobile_vnums: &mobile_vnums,
                ..Default::default()
            },
        );
        assert!(result.completions.contains(&"trigger".to_string()));
        assert_eq!(result.completion_type, CompletionType::MeditSubcommand);
    }

    #[test]
    fn test_medit_trigger_action_completion() {
        let commands = vec!["medit".to_string()];
        let mobile_vnums = vec!["town:guard".to_string()];

        // Complete trigger action after "trigger"
        let result = complete(
            "medit town:guard trigger a",
            26,
            &CompletionData {
                available_commands: &commands,
                mobile_vnums: &mobile_vnums,
                ..Default::default()
            },
        );
        assert!(result.completions.contains(&"add".to_string()));
        assert_eq!(result.completion_type, CompletionType::TriggerAction);
    }

    #[test]
    fn test_medit_trigger_type_completion() {
        let commands = vec!["medit".to_string()];
        let mobile_vnums = vec!["town:guard".to_string()];

        // Complete trigger type after "add"
        let result = complete(
            "medit town:guard trigger add gr",
            31,
            &CompletionData {
                available_commands: &commands,
                mobile_vnums: &mobile_vnums,
                ..Default::default()
            },
        );
        assert!(result.completions.contains(&"greet".to_string()));
        assert_eq!(result.completion_type, CompletionType::TriggerType);
    }

    #[test]
    fn test_medit_trigger_template_completion() {
        let commands = vec!["medit".to_string()];
        let mobile_vnums = vec!["town:guard".to_string()];

        // Complete template after trigger type
        let result = complete(
            "medit town:guard trigger add greet @say",
            39,
            &CompletionData {
                available_commands: &commands,
                mobile_vnums: &mobile_vnums,
                ..Default::default()
            },
        );
        assert!(result.completions.contains(&"@say_greeting".to_string()));
        assert!(result.completions.contains(&"@say_random".to_string()));
        assert_eq!(result.completion_type, CompletionType::TriggerScript);
    }

    #[test]
    fn test_oedit_subcommand_completion() {
        let commands = vec!["oedit".to_string()];
        let item_vnums = vec!["town:sword".to_string()];

        // Complete subcommand after vnum
        let result = complete(
            "oedit town:sword ty",
            19,
            &CompletionData {
                available_commands: &commands,
                item_vnums: &item_vnums,
                ..Default::default()
            },
        );
        assert!(result.completions.contains(&"type".to_string()));
        assert_eq!(result.completion_type, CompletionType::OeditSubcommand);
    }

    #[test]
    fn test_oedit_type_completion() {
        let commands = vec!["oedit".to_string()];
        let item_vnums = vec!["town:sword".to_string()];

        // Complete item type after "type"
        let result = complete(
            "oedit town:sword type ar",
            24,
            &CompletionData {
                available_commands: &commands,
                item_vnums: &item_vnums,
                ..Default::default()
            },
        );
        assert!(result.completions.contains(&"armor".to_string()));
        assert_eq!(result.completion_type, CompletionType::ItemType);
    }

    #[test]
    fn test_oedit_trigger_action_completion() {
        let commands = vec!["oedit".to_string()];
        let item_vnums = vec!["town:sword".to_string()];

        // Complete trigger action after "trigger"
        let result = complete(
            "oedit town:sword trigger a",
            26,
            &CompletionData {
                available_commands: &commands,
                item_vnums: &item_vnums,
                ..Default::default()
            },
        );
        assert!(result.completions.contains(&"add".to_string()));
        assert_eq!(result.completion_type, CompletionType::ItemTriggerAction);
    }

    #[test]
    fn test_oedit_trigger_type_completion() {
        let commands = vec!["oedit".to_string()];
        let item_vnums = vec!["town:sword".to_string()];

        // Complete trigger type after "add"
        let result = complete(
            "oedit town:sword trigger add ge",
            31,
            &CompletionData {
                available_commands: &commands,
                item_vnums: &item_vnums,
                ..Default::default()
            },
        );
        assert!(result.completions.contains(&"get".to_string()));
        assert_eq!(result.completion_type, CompletionType::ItemTriggerType);
    }

    #[test]
    fn test_redit_subcommand_completion() {
        let commands = vec!["redit".to_string()];

        // Complete subcommand
        let result = complete(
            "redit tr",
            8,
            &CompletionData {
                available_commands: &commands,
                ..Default::default()
            },
        );
        assert!(result.completions.contains(&"trigger".to_string()));
        assert_eq!(result.completion_type, CompletionType::ReditSubcommand);
    }

    #[test]
    fn test_redit_flag_completion() {
        let commands = vec!["redit".to_string()];

        // Complete flag name after "flag"
        let result = complete(
            "redit flag da",
            13,
            &CompletionData {
                available_commands: &commands,
                ..Default::default()
            },
        );
        assert!(result.completions.contains(&"dark".to_string()));
        assert_eq!(result.completion_type, CompletionType::RoomFlag);
    }

    #[test]
    fn test_redit_extra_action_completion() {
        let commands = vec!["redit".to_string()];

        // Complete extra action
        let result = complete(
            "redit extra li",
            14,
            &CompletionData {
                available_commands: &commands,
                ..Default::default()
            },
        );
        assert!(result.completions.contains(&"list".to_string()));
        assert_eq!(result.completion_type, CompletionType::ExtraDescAction);
    }

    #[test]
    fn test_redit_trigger_action_completion() {
        let commands = vec!["redit".to_string()];

        // Complete trigger action
        let result = complete(
            "redit trigger a",
            15,
            &CompletionData {
                available_commands: &commands,
                ..Default::default()
            },
        );
        assert!(result.completions.contains(&"add".to_string()));
        assert_eq!(result.completion_type, CompletionType::RoomTriggerAction);
    }

    #[test]
    fn test_redit_trigger_type_completion() {
        let commands = vec!["redit".to_string()];

        // Complete trigger type after "add"
        let result = complete(
            "redit trigger add en",
            20,
            &CompletionData {
                available_commands: &commands,
                ..Default::default()
            },
        );
        assert!(result.completions.contains(&"enter".to_string()));
        assert_eq!(result.completion_type, CompletionType::RoomTriggerType);
    }

    #[test]
    fn test_aedit_subcommand_completion() {
        let commands = vec!["aedit".to_string()];
        let area_prefixes = vec!["town".to_string()];

        // Complete subcommand after area
        let result = complete(
            "aedit town pe",
            13,
            &CompletionData {
                available_commands: &commands,
                area_prefixes: &area_prefixes,
                ..Default::default()
            },
        );
        assert!(result.completions.contains(&"permission".to_string()));
        assert_eq!(result.completion_type, CompletionType::AeditSubcommand);
    }

    #[test]
    fn test_aedit_permission_completion() {
        let commands = vec!["aedit".to_string()];
        let area_prefixes = vec!["town".to_string()];

        // Complete permission level
        let result = complete(
            "aedit town permission ow",
            24,
            &CompletionData {
                available_commands: &commands,
                area_prefixes: &area_prefixes,
                ..Default::default()
            },
        );
        assert!(result.completions.contains(&"owner_only".to_string()));
        assert_eq!(result.completion_type, CompletionType::PermissionLevel);
    }

    #[test]
    fn test_spedit_subcommand_completion() {
        let commands = vec!["spedit".to_string()];

        // Complete subcommand (no area prefix in new syntax)
        let result = complete(
            "spedit cr",
            9,
            &CompletionData {
                available_commands: &commands,
                ..Default::default()
            },
        );
        assert!(result.completions.contains(&"create".to_string()));
        assert_eq!(result.completion_type, CompletionType::SpeditSubcommand);
    }

    #[test]
    fn test_spedit_list_filter_completion() {
        let commands = vec!["spedit".to_string()];

        // Complete filter after "list"
        let result = complete(
            "spedit list mo",
            14,
            &CompletionData {
                available_commands: &commands,
                ..Default::default()
            },
        );
        assert!(result.completions.contains(&"mobs".to_string()));
        assert_eq!(result.completion_type, CompletionType::SpeditFilter);
    }

    #[test]
    fn test_spedit_room_vnum_completion() {
        let commands = vec!["spedit".to_string()];
        let room_vnums = vec!["town:plaza".to_string(), "town:market".to_string()];

        // Complete room vnum after "create"
        let result = complete(
            "spedit create town:p",
            20,
            &CompletionData {
                available_commands: &commands,
                room_vnums: &room_vnums,
                ..Default::default()
            },
        );
        assert!(result.completions.contains(&"town:plaza".to_string()));
        assert_eq!(result.completion_type, CompletionType::RoomVnum);
    }

    #[test]
    fn test_spedit_entity_type_completion() {
        let commands = vec!["spedit".to_string()];
        let room_vnums = vec!["town:plaza".to_string()];

        // Complete entity type after "create <room>"
        let result = complete(
            "spedit create town:plaza mo",
            27,
            &CompletionData {
                available_commands: &commands,
                room_vnums: &room_vnums,
                ..Default::default()
            },
        );
        assert!(result.completions.contains(&"mobile".to_string()));
        assert_eq!(result.completion_type, CompletionType::SpawnEntityType);
    }

    #[test]
    fn test_spedit_mobile_vnum_completion() {
        let commands = vec!["spedit".to_string()];
        let room_vnums = vec!["town:plaza".to_string()];
        let mobile_vnums = vec!["town:guard".to_string(), "town:merchant".to_string()];

        // Complete mobile vnum after "create <room> mobile"
        let result = complete(
            "spedit create town:plaza mobile town:g",
            38,
            &CompletionData {
                available_commands: &commands,
                room_vnums: &room_vnums,
                mobile_vnums: &mobile_vnums,
                ..Default::default()
            },
        );
        assert!(result.completions.contains(&"town:guard".to_string()));
        assert_eq!(result.completion_type, CompletionType::MobileVnum);
    }

    #[test]
    fn test_spedit_item_vnum_completion() {
        let commands = vec!["spedit".to_string()];
        let room_vnums = vec!["town:plaza".to_string()];
        let item_vnums = vec!["town:sword".to_string(), "town:shield".to_string()];

        // Complete item vnum after "create <room> item"
        let result = complete(
            "spedit create town:plaza item town:sw",
            37,
            &CompletionData {
                available_commands: &commands,
                room_vnums: &room_vnums,
                item_vnums: &item_vnums,
                ..Default::default()
            },
        );
        assert!(result.completions.contains(&"town:sword".to_string()));
        assert_eq!(result.completion_type, CompletionType::ItemVnum);
    }

    #[test]
    fn test_spedit_delete_filter_completion() {
        let commands = vec!["spedit".to_string()];

        // Complete filter after "delete"
        let result = complete(
            "spedit delete mo",
            16,
            &CompletionData {
                available_commands: &commands,
                ..Default::default()
            },
        );
        assert!(result.completions.contains(&"mobs".to_string()));
        assert_eq!(result.completion_type, CompletionType::SpeditFilter);
    }

    #[test]
    fn test_spedit_dep_filter_and_action_completion() {
        let commands = vec!["spedit".to_string()];

        // Complete filter OR dep action after "dep" (both should be offered)
        let result = complete(
            "spedit dep a",
            12,
            &CompletionData {
                available_commands: &commands,
                ..Default::default()
            },
        );
        assert!(result.completions.contains(&"all".to_string())); // filter
        assert!(result.completions.contains(&"add".to_string())); // dep action
    }

    #[test]
    fn test_spedit_dep_with_filter_action_completion() {
        let commands = vec!["spedit".to_string()];

        // Complete dep action after "dep mobs"
        let result = complete(
            "spedit dep mobs a",
            17,
            &CompletionData {
                available_commands: &commands,
                ..Default::default()
            },
        );
        assert!(result.completions.contains(&"add".to_string()));
        assert_eq!(result.completion_type, CompletionType::SpeditDepAction);
    }

    #[test]
    fn test_set_subcommand_completion_non_builder() {
        let commands = vec!["set".to_string()];

        // Non-builder should see mxp, color, and afk but NOT roomflags
        let result = complete(
            "set ",
            4,
            &CompletionData {
                available_commands: &commands,
                ..Default::default()
            },
        );
        assert!(result.completions.contains(&"mxp".to_string()));
        assert!(result.completions.contains(&"color".to_string()));
        assert!(result.completions.contains(&"afk".to_string()));
        assert!(!result.completions.contains(&"roomflags".to_string()));
        assert_eq!(result.completion_type, CompletionType::SetSubcommand);
    }

    #[test]
    fn test_set_subcommand_completion_builder() {
        let commands = vec!["set".to_string()];

        // Builder should see all settings including roomflags
        let result = complete(
            "set ",
            4,
            &CompletionData {
                available_commands: &commands,
                is_builder: true,
                ..Default::default()
            },
        );
        assert!(result.completions.contains(&"mxp".to_string()));
        assert!(result.completions.contains(&"color".to_string()));
        assert!(result.completions.contains(&"afk".to_string()));
        assert!(result.completions.contains(&"roomflags".to_string()));
        assert_eq!(result.completion_type, CompletionType::SetSubcommand);
    }

    #[test]
    fn test_set_partial_completion() {
        let commands = vec!["set".to_string()];

        // Complete partial setting name
        let result = complete(
            "set m",
            5,
            &CompletionData {
                available_commands: &commands,
                ..Default::default()
            },
        );
        assert!(result.completions.contains(&"mxp".to_string()));
        assert!(!result.completions.contains(&"color".to_string()));
        assert_eq!(result.completion_type, CompletionType::SetSubcommand);
    }

    #[test]
    fn test_set_toggle_value_completion() {
        let commands = vec!["set".to_string()];

        // After setting name, show on/off
        let result = complete(
            "set mxp ",
            8,
            &CompletionData {
                available_commands: &commands,
                ..Default::default()
            },
        );
        assert!(result.completions.contains(&"on".to_string()));
        assert!(result.completions.contains(&"off".to_string()));
        assert_eq!(result.completion_type, CompletionType::SetSubcommand);
    }

    #[test]
    fn test_set_toggle_partial_completion() {
        let commands = vec!["set".to_string()];

        // Complete partial toggle value
        let result = complete(
            "set mxp o",
            9,
            &CompletionData {
                available_commands: &commands,
                ..Default::default()
            },
        );
        assert!(result.completions.contains(&"on".to_string()));
        assert!(result.completions.contains(&"off".to_string()));
        assert_eq!(result.completion_type, CompletionType::SetSubcommand);
    }

    #[test]
    fn test_treat_target_completion() {
        let commands = vec!["treat".to_string()];
        let online_players = vec!["Alice".to_string(), "Bob".to_string()];

        // After "treat" show self and online players
        let result = complete(
            "treat ",
            6,
            &CompletionData {
                available_commands: &commands,
                online_players: &online_players,
                ..Default::default()
            },
        );
        assert!(result.completions.contains(&"self".to_string()));
        assert!(result.completions.contains(&"Alice".to_string()));
        assert!(result.completions.contains(&"Bob".to_string()));
        assert_eq!(result.completion_type, CompletionType::TreatTarget);
    }

    #[test]
    fn test_treat_target_partial_completion() {
        let commands = vec!["treat".to_string()];
        let online_players = vec!["Alice".to_string(), "Bob".to_string()];

        // Complete partial target
        let result = complete(
            "treat se",
            8,
            &CompletionData {
                available_commands: &commands,
                online_players: &online_players,
                ..Default::default()
            },
        );
        assert!(result.completions.contains(&"self".to_string()));
        assert!(!result.completions.contains(&"Alice".to_string()));
        assert_eq!(result.completion_type, CompletionType::TreatTarget);
    }

    #[test]
    fn test_treat_body_part_completion() {
        let commands = vec!["treat".to_string()];
        let online_players = vec!["Alice".to_string()];

        // After target, show body parts and conditions
        let result = complete(
            "treat self ",
            11,
            &CompletionData {
                available_commands: &commands,
                online_players: &online_players,
                ..Default::default()
            },
        );
        assert!(result.completions.contains(&"head".to_string()));
        assert!(result.completions.contains(&"torso".to_string()));
        assert!(result.completions.contains(&"hypothermia".to_string()));
        assert!(result.completions.contains(&"illness".to_string()));
        assert_eq!(result.completion_type, CompletionType::BodyPart);
    }

    #[test]
    fn test_treat_body_part_partial_completion() {
        let commands = vec!["treat".to_string()];
        let online_players = vec!["Alice".to_string()];

        // Complete partial body part
        let result = complete(
            "treat self le",
            13,
            &CompletionData {
                available_commands: &commands,
                online_players: &online_players,
                ..Default::default()
            },
        );
        assert!(result.completions.contains(&"leftarm".to_string()));
        assert!(result.completions.contains(&"leftleg".to_string()));
        assert!(result.completions.contains(&"lefthand".to_string()));
        assert!(result.completions.contains(&"leftfoot".to_string()));
        assert!(!result.completions.contains(&"head".to_string()));
        assert_eq!(result.completion_type, CompletionType::BodyPart);
    }

    #[test]
    fn test_treat_condition_partial_completion() {
        let commands = vec!["treat".to_string()];
        let online_players = vec!["Alice".to_string()];

        // Complete partial condition - should get only conditions
        let result = complete(
            "treat self heat",
            15,
            &CompletionData {
                available_commands: &commands,
                online_players: &online_players,
                ..Default::default()
            },
        );
        assert!(result.completions.contains(&"heat_exhaustion".to_string()));
        assert!(result.completions.contains(&"heat_stroke".to_string()));
        assert!(!result.completions.contains(&"head".to_string()));
        assert_eq!(result.completion_type, CompletionType::TreatableCondition);
    }

    #[test]
    fn test_medit_bare_completion() {
        // `medit ` (trailing space) should list every mobile vnum; previously
        // returned empty because the completer had no len==1 arm.
        let commands = vec!["medit".to_string()];
        let mobile_vnums = vec!["town:guard".to_string(), "town:merchant".to_string()];
        let result = complete(
            "medit ",
            6,
            &CompletionData {
                available_commands: &commands,
                mobile_vnums: &mobile_vnums,
                ..Default::default()
            },
        );
        assert!(result.completions.contains(&"town:guard".to_string()));
        assert!(result.completions.contains(&"town:merchant".to_string()));
        assert_eq!(result.completion_type, CompletionType::MobileVnum);
    }

    #[test]
    fn test_oedit_bare_completion() {
        let commands = vec!["oedit".to_string()];
        let item_vnums = vec!["town:sword".to_string(), "town:shield".to_string()];
        let result = complete(
            "oedit ",
            6,
            &CompletionData {
                available_commands: &commands,
                item_vnums: &item_vnums,
                ..Default::default()
            },
        );
        assert!(result.completions.contains(&"town:sword".to_string()));
        assert!(result.completions.contains(&"town:shield".to_string()));
        assert_eq!(result.completion_type, CompletionType::ItemVnum);
    }

    #[test]
    fn test_recedit_bare_completion() {
        let commands = vec!["recedit".to_string()];
        let recipe_vnums = vec!["smith:longsword".to_string(), "cook:stew".to_string()];
        let result = complete(
            "recedit ",
            8,
            &CompletionData {
                available_commands: &commands,
                recipe_vnums: &recipe_vnums,
                ..Default::default()
            },
        );
        assert!(result.completions.contains(&"smith:longsword".to_string()));
        assert!(result.completions.contains(&"cook:stew".to_string()));
        assert_eq!(result.completion_type, CompletionType::RecipeVnum);
    }

    #[test]
    fn test_bpredit_bare_completion() {
        let commands = vec!["bpredit".to_string()];
        let shop_preset_vnums = vec!["weapons_basic".to_string(), "potions_low".to_string()];
        let result = complete(
            "bpredit ",
            8,
            &CompletionData {
                available_commands: &commands,
                shop_preset_vnums: &shop_preset_vnums,
                ..Default::default()
            },
        );
        assert!(result.completions.contains(&"list".to_string()));
        assert!(result.completions.contains(&"create".to_string()));
        assert!(result.completions.contains(&"weapons_basic".to_string()));
        assert!(result.completions.contains(&"potions_low".to_string()));
    }

    #[test]
    fn test_aedit_bare_completion() {
        let commands = vec!["aedit".to_string()];
        let area_prefixes = vec!["town".to_string(), "forest".to_string()];
        let result = complete(
            "aedit ",
            6,
            &CompletionData {
                available_commands: &commands,
                area_prefixes: &area_prefixes,
                ..Default::default()
            },
        );
        // Both subcommands and area prefixes should be offered.
        assert!(result.completions.contains(&"town".to_string()));
        assert!(result.completions.contains(&"forest".to_string()));
        assert!(result.completions.contains(&"permission".to_string()));
    }

    #[test]
    fn test_achedit_bare_completion() {
        let commands = vec!["achedit".to_string()];
        let achievement_keys = vec![
            "first_blood".to_string(),
            "first_kill".to_string(),
            "millionaire".to_string(),
        ];

        // `achedit ` (trailing space) should surface every subcommand AND every
        // existing key — this previously returned empty because the completer
        // had no words.len() == 1 arm.
        let result = complete(
            "achedit ",
            8,
            &CompletionData {
                available_commands: &commands,
                achievement_keys: &achievement_keys,
                is_builder: true,
                ..Default::default()
            },
        );
        assert!(result.completions.contains(&"create".to_string()));
        assert!(result.completions.contains(&"list".to_string()));
        assert!(result.completions.contains(&"first_blood".to_string()));
        assert!(result.completions.contains(&"millionaire".to_string()));
        assert_eq!(result.completion_type, CompletionType::AcheditSubcommand);
    }

    #[test]
    fn test_achedit_partial_completion() {
        let commands = vec!["achedit".to_string()];
        let achievement_keys = vec![
            "first_blood".to_string(),
            "first_kill".to_string(),
            "millionaire".to_string(),
        ];

        // `achedit fi` should match both achievement keys starting with "fi"
        // (subcommands like create/list don't start with "fi").
        let result = complete(
            "achedit fi",
            10,
            &CompletionData {
                available_commands: &commands,
                achievement_keys: &achievement_keys,
                is_builder: true,
                ..Default::default()
            },
        );
        assert!(result.completions.contains(&"first_blood".to_string()));
        assert!(result.completions.contains(&"first_kill".to_string()));
        assert!(!result.completions.contains(&"millionaire".to_string()));
    }

    #[test]
    fn test_mail_subcommand_completion() {
        let commands = vec!["mail".to_string()];
        let online_players = vec!["Alice".to_string(), "Bob".to_string()];

        // Complete subcommand
        let result = complete(
            "mail ",
            5,
            &CompletionData {
                available_commands: &commands,
                online_players: &online_players,
                ..Default::default()
            },
        );
        assert!(result.completions.contains(&"check".to_string()));
        assert!(result.completions.contains(&"list".to_string()));
        assert!(result.completions.contains(&"read".to_string()));
        assert!(result.completions.contains(&"send".to_string()));
        assert!(result.completions.contains(&"compose".to_string()));
        assert!(result.completions.contains(&"delete".to_string()));
        assert!(result.completions.contains(&"reply".to_string()));
        assert_eq!(result.completion_type, CompletionType::MailSubcommand);

        // Complete partial subcommand
        let result = complete(
            "mail se",
            7,
            &CompletionData {
                available_commands: &commands,
                online_players: &online_players,
                ..Default::default()
            },
        );
        assert!(result.completions.contains(&"send".to_string()));
        assert!(!result.completions.contains(&"list".to_string()));
        assert_eq!(result.completion_type, CompletionType::MailSubcommand);

        // Complete player name for send
        let result = complete(
            "mail send ",
            10,
            &CompletionData {
                available_commands: &commands,
                online_players: &online_players,
                ..Default::default()
            },
        );
        assert!(result.completions.contains(&"Alice".to_string()));
        assert!(result.completions.contains(&"Bob".to_string()));
        assert_eq!(result.completion_type, CompletionType::PlayerName);
    }

    #[test]
    fn test_complete_build_audit_kind_and_key() {
        let commands = vec!["build".to_string()];
        let room_vnums = vec!["town:square".to_string(), "town:tavern".to_string()];
        let mobile_vnums = vec!["town:baker".to_string(), "forest:wolf".to_string()];
        let item_vnums = vec!["town:sword".to_string()];
        let quest_vnums = vec!["town:lost_cat".to_string()];
        let mobs_in_room = vec!["baker".to_string(), "dog".to_string()];
        let items_in_room = vec!["lantern".to_string()];

        let data = CompletionData {
            available_commands: &commands,
            room_vnums: &room_vnums,
            mobile_vnums: &mobile_vnums,
            item_vnums: &item_vnums,
            quest_vnums: &quest_vnums,
            mobs_in_room: &mobs_in_room,
            items_in_room: &items_in_room,
            ..Default::default()
        };

        // The kind itself.
        let result = complete("build audit ", 12, &data);
        assert!(result.completions.contains(&"room".to_string()));
        assert!(result.completions.contains(&"world".to_string()));
        assert_eq!(result.completion_type, CompletionType::BuildAuditTarget);

        // Room keys come from the world's room vnums.
        let result = complete("build audit room town:", 22, &data);
        assert!(result.completions.contains(&"town:square".to_string()));
        assert!(!result.completions.contains(&"town:baker".to_string()));
        assert_eq!(result.completion_type, CompletionType::RoomVnum);

        // Mob keys offer what is in the room ahead of the world's vnums.
        let result = complete("build audit mob ", 16, &data);
        assert_eq!(result.completions.first(), Some(&"baker".to_string()));
        assert!(result.completions.contains(&"forest:wolf".to_string()));
        assert_eq!(result.completion_type, CompletionType::MobileVnum);

        // A partial matching both a room keyword and a vnum keeps both.
        let result = complete("build audit mob b", 17, &data);
        assert_eq!(result.completions, vec!["baker".to_string()]);

        let result = complete("build audit item l", 18, &data);
        assert_eq!(result.completions, vec!["lantern".to_string()]);
        assert_eq!(result.completion_type, CompletionType::ItemVnum);

        let result = complete("build audit quest ", 18, &data);
        assert_eq!(result.completions, vec!["town:lost_cat".to_string()]);
        assert_eq!(result.completion_type, CompletionType::QuestVnum);

        // `here` and `world` take no key.
        let result = complete("build audit world ", 18, &data);
        assert!(result.is_empty());
    }
}
