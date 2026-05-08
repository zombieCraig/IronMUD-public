use super::consts::*;
use super::helpers::*;
use super::types::*;

/// Context-aware completion for tedit command
/// Syntax: tedit <vnum> <subcommand> [args...]
/// Or: tedit create <vnum>
pub(super) fn complete_tedit(
    words: &[&str],
    completing_word: bool,
    transport_vnums: &[String],
    room_vnums: &[String],
) -> CompletionResult {
    let partial = get_partial(words, completing_word);

    match words.len() {
        // tedit <partial_vnum_or_create> - complete vnum or "create"
        2 if completing_word => {
            let mut matches: Vec<String> = Vec::new();
            if "create".starts_with(&partial) {
                matches.push("create".to_string());
            }
            matches.extend(
                transport_vnums
                    .iter()
                    .filter(|v| v.to_lowercase().starts_with(&partial))
                    .cloned(),
            );
            CompletionResult::new(matches, &partial, CompletionType::TransportVnum)
        }
        // tedit - show "create" and all vnums
        1 if !completing_word => {
            let mut matches = vec!["create".to_string()];
            matches.extend(transport_vnums.iter().cloned());
            CompletionResult::new(matches, "", CompletionType::TransportVnum)
        }
        // tedit <vnum> - show all subcommands (if not "create")
        2 if !completing_word => {
            if words[1].to_lowercase() == "create" {
                CompletionResult::empty()
            } else {
                all_static(TEDIT_SUBCOMMANDS, CompletionType::TeditSubcommand)
            }
        }
        // tedit <vnum> <partial_subcmd> - complete subcommand
        3 if completing_word => filter_static(TEDIT_SUBCOMMANDS, &partial, CompletionType::TeditSubcommand),
        // tedit <vnum> type - show transport types
        3 if !completing_word && words[2].to_lowercase() == "type" => {
            all_static(TRANSPORT_TYPES, CompletionType::TransportType)
        }
        // tedit <vnum> type <partial_type> - complete transport type
        4 if completing_word && words[2].to_lowercase() == "type" => {
            filter_static(TRANSPORT_TYPES, &partial, CompletionType::TransportType)
        }
        // tedit <vnum> schedule - show schedule types
        3 if !completing_word && words[2].to_lowercase() == "schedule" => {
            all_static(SCHEDULE_TYPES, CompletionType::TeditSubcommand)
        }
        // tedit <vnum> schedule <partial_type> - complete schedule type
        4 if completing_word && words[2].to_lowercase() == "schedule" => {
            filter_static(SCHEDULE_TYPES, &partial, CompletionType::TeditSubcommand)
        }
        // tedit <vnum> stop - show stop actions
        3 if !completing_word && words[2].to_lowercase() == "stop" => {
            all_static(STOP_ACTIONS, CompletionType::StopAction)
        }
        // tedit <vnum> stop <partial_action> - complete stop action
        4 if completing_word && words[2].to_lowercase() == "stop" => {
            filter_static(STOP_ACTIONS, &partial, CompletionType::StopAction)
        }
        // tedit <vnum> stop add - show room vnums
        4 if !completing_word && words[2].to_lowercase() == "stop" && words[3].to_lowercase() == "add" => {
            all_dynamic(room_vnums, CompletionType::RoomVnum)
        }
        // tedit <vnum> stop add <partial_room> - complete room vnum
        5 if completing_word && words[2].to_lowercase() == "stop" && words[3].to_lowercase() == "add" => {
            filter_dynamic(room_vnums, &partial, CompletionType::RoomVnum)
        }
        // tedit <vnum> stop add <room> <name> - show directions (after name entered)
        6 if !completing_word && words[2].to_lowercase() == "stop" && words[3].to_lowercase() == "add" => {
            all_static(DIRECTIONS, CompletionType::Direction)
        }
        // tedit <vnum> stop add <room> <name> <partial_dir> - complete direction
        7 if completing_word && words[2].to_lowercase() == "stop" && words[3].to_lowercase() == "add" => {
            filter_static(DIRECTIONS, &partial, CompletionType::Direction)
        }
        // tedit <vnum> interior - show room vnums
        3 if !completing_word && words[2].to_lowercase() == "interior" => {
            all_dynamic(room_vnums, CompletionType::RoomVnum)
        }
        // tedit <vnum> interior <partial_room> - complete room vnum
        4 if completing_word && words[2].to_lowercase() == "interior" => {
            filter_dynamic(room_vnums, &partial, CompletionType::RoomVnum)
        }
        _ => CompletionResult::empty(),
    }
}
