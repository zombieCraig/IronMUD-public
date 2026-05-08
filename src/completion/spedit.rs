use super::consts::*;
use super::helpers::*;
use super::types::*;

/// Context-aware completion for spedit command
/// Syntax: spedit <area> create <room> <type> <vnum> <max> <interval>
/// Helper to check if a word is a spedit filter keyword
pub(super) fn is_spedit_filter(word: &str) -> bool {
    let lower = word.to_lowercase();
    SPEDIT_FILTERS.iter().any(|&f| f == lower)
}

/// Helper to check if a word is a spedit modification command that supports filters
pub(super) fn is_spedit_mod_command(word: &str) -> bool {
    let lower = word.to_lowercase();
    matches!(
        lower.as_str(),
        "delete" | "enable" | "disable" | "max" | "interval" | "dep"
    )
}

pub(super) fn complete_spedit(
    words: &[&str],
    completing_word: bool,
    _area_prefixes: &[String],
    room_vnums: &[String],
    mobile_vnums: &[String],
    item_vnums: &[String],
) -> CompletionResult {
    let partial = get_partial(words, completing_word);

    match words.len() {
        // spedit - show all subcommands
        1 if !completing_word => all_static(SPEDIT_SUBCOMMANDS, CompletionType::SpeditSubcommand),
        // spedit <partial_subcmd> - complete subcommand
        2 if completing_word => filter_static(SPEDIT_SUBCOMMANDS, &partial, CompletionType::SpeditSubcommand),

        // === list command ===
        // spedit list - show filter options
        2 if !completing_word && words[1].to_lowercase() == "list" => {
            all_static(SPEDIT_FILTERS, CompletionType::SpeditFilter)
        }
        // spedit list <partial_filter> - complete filter
        3 if completing_word && words[1].to_lowercase() == "list" => {
            filter_static(SPEDIT_FILTERS, &partial, CompletionType::SpeditFilter)
        }
        // spedit list room - show room vnums
        3 if !completing_word && words[1].to_lowercase() == "list" && words[2].to_lowercase() == "room" => {
            all_dynamic(room_vnums, CompletionType::RoomVnum)
        }
        // spedit list room <partial_vnum> - complete room vnum
        4 if completing_word && words[1].to_lowercase() == "list" && words[2].to_lowercase() == "room" => {
            filter_dynamic(room_vnums, &partial, CompletionType::RoomVnum)
        }

        // === create command ===
        // spedit create - show room vnums (including "." for current room)
        2 if !completing_word && words[1].to_lowercase() == "create" => {
            let mut matches: Vec<String> = vec![".".to_string()];
            matches.extend(room_vnums.iter().cloned());
            CompletionResult::new(matches, "", CompletionType::RoomVnum)
        }
        // spedit create <partial_room> - complete room vnum
        3 if completing_word && words[1].to_lowercase() == "create" => {
            let mut matches: Vec<String> = if ".".starts_with(&partial) {
                vec![".".to_string()]
            } else {
                vec![]
            };
            matches.extend(
                room_vnums
                    .iter()
                    .filter(|v| v.to_lowercase().starts_with(&partial))
                    .cloned(),
            );
            CompletionResult::new(matches, &partial, CompletionType::RoomVnum)
        }
        // spedit create <room> - show entity types
        3 if !completing_word && words[1].to_lowercase() == "create" => {
            all_static(SPAWN_ENTITY_TYPES, CompletionType::SpawnEntityType)
        }
        // spedit create <room> <partial_type> - complete entity type
        4 if completing_word && words[1].to_lowercase() == "create" => {
            filter_static(SPAWN_ENTITY_TYPES, &partial, CompletionType::SpawnEntityType)
        }
        // spedit create <room> mobile - complete mobile vnums
        4 if !completing_word && words[1].to_lowercase() == "create" && words[3].to_lowercase() == "mobile" => {
            all_dynamic(mobile_vnums, CompletionType::MobileVnum)
        }
        // spedit create <room> mobile <partial_vnum> - complete mobile vnum
        5 if completing_word && words[1].to_lowercase() == "create" && words[3].to_lowercase() == "mobile" => {
            filter_dynamic(mobile_vnums, &partial, CompletionType::MobileVnum)
        }
        // spedit create <room> item - complete item vnums
        4 if !completing_word && words[1].to_lowercase() == "create" && words[3].to_lowercase() == "item" => {
            all_dynamic(item_vnums, CompletionType::ItemVnum)
        }
        // spedit create <room> item <partial_vnum> - complete item vnum
        5 if completing_word && words[1].to_lowercase() == "create" && words[3].to_lowercase() == "item" => {
            filter_dynamic(item_vnums, &partial, CompletionType::ItemVnum)
        }

        // === modification commands (delete, enable, disable, max, interval) ===
        // spedit <mod_cmd> - show filter options (since index can also be typed directly)
        2 if !completing_word && is_spedit_mod_command(words[1]) && words[1].to_lowercase() != "dep" => {
            all_static(SPEDIT_FILTERS, CompletionType::SpeditFilter)
        }
        // spedit <mod_cmd> <partial_filter> - complete filter (or could be index)
        3 if completing_word && is_spedit_mod_command(words[1]) && words[1].to_lowercase() != "dep" => {
            filter_static(SPEDIT_FILTERS, &partial, CompletionType::SpeditFilter)
        }
        // spedit <mod_cmd> room - show room vnums for "room <vnum>" filter
        3 if !completing_word
            && is_spedit_mod_command(words[1])
            && words[1].to_lowercase() != "dep"
            && words[2].to_lowercase() == "room" =>
        {
            all_dynamic(room_vnums, CompletionType::RoomVnum)
        }
        // spedit <mod_cmd> room <partial_vnum> - complete room vnum
        4 if completing_word
            && is_spedit_mod_command(words[1])
            && words[1].to_lowercase() != "dep"
            && words[2].to_lowercase() == "room" =>
        {
            filter_dynamic(room_vnums, &partial, CompletionType::RoomVnum)
        }

        // === dep command ===
        // spedit dep - show filter options AND dep actions combined
        2 if !completing_word && words[1].to_lowercase() == "dep" => {
            let mut combined: Vec<String> = SPEDIT_FILTERS.iter().map(|s| s.to_string()).collect();
            combined.extend(SPEDIT_DEP_ACTIONS.iter().map(|s| s.to_string()));
            CompletionResult::new(combined, "", CompletionType::SpeditDepAction)
        }
        // spedit dep <partial> - complete filter or dep action
        3 if completing_word && words[1].to_lowercase() == "dep" => {
            let mut combined: Vec<&str> = SPEDIT_FILTERS.to_vec();
            combined.extend(SPEDIT_DEP_ACTIONS);
            let matches: Vec<String> = combined
                .iter()
                .filter(|s| s.to_lowercase().starts_with(&partial))
                .map(|s| s.to_string())
                .collect();
            CompletionResult::new(matches, &partial, CompletionType::SpeditDepAction)
        }
        // spedit dep <filter> - show dep actions
        3 if !completing_word && words[1].to_lowercase() == "dep" && is_spedit_filter(words[2]) => {
            all_static(SPEDIT_DEP_ACTIONS, CompletionType::SpeditDepAction)
        }
        // spedit dep <filter> <partial_action> - complete dep action
        4 if completing_word && words[1].to_lowercase() == "dep" && is_spedit_filter(words[2]) => {
            filter_static(SPEDIT_DEP_ACTIONS, &partial, CompletionType::SpeditDepAction)
        }
        // spedit dep room - show room vnums
        3 if !completing_word && words[1].to_lowercase() == "dep" && words[2].to_lowercase() == "room" => {
            all_dynamic(room_vnums, CompletionType::RoomVnum)
        }
        // spedit dep room <partial_vnum> - complete room vnum
        4 if completing_word && words[1].to_lowercase() == "dep" && words[2].to_lowercase() == "room" => {
            filter_dynamic(room_vnums, &partial, CompletionType::RoomVnum)
        }
        // spedit dep room <vnum> - show dep actions
        4 if !completing_word && words[1].to_lowercase() == "dep" && words[2].to_lowercase() == "room" => {
            all_static(SPEDIT_DEP_ACTIONS, CompletionType::SpeditDepAction)
        }
        // spedit dep room <vnum> <partial_action> - complete dep action
        5 if completing_word && words[1].to_lowercase() == "dep" && words[2].to_lowercase() == "room" => {
            filter_static(SPEDIT_DEP_ACTIONS, &partial, CompletionType::SpeditDepAction)
        }

        // === dep add without filter (spedit dep add <index> <type> <vnum>) ===
        // spedit dep add <index> - show dep types
        4 if !completing_word && words[1].to_lowercase() == "dep" && words[2].to_lowercase() == "add" => {
            all_static(SPEDIT_DEP_TYPES, CompletionType::SpeditDepType)
        }
        // spedit dep add <index> <partial_type> - complete dep type
        5 if completing_word && words[1].to_lowercase() == "dep" && words[2].to_lowercase() == "add" => {
            filter_static(SPEDIT_DEP_TYPES, &partial, CompletionType::SpeditDepType)
        }
        // spedit dep add <index> <type> - show item vnums
        5 if !completing_word && words[1].to_lowercase() == "dep" && words[2].to_lowercase() == "add" => {
            all_dynamic(item_vnums, CompletionType::ItemVnum)
        }
        // spedit dep add <index> <type> <partial_vnum> - complete item vnum
        6 if completing_word && words[1].to_lowercase() == "dep" && words[2].to_lowercase() == "add" => {
            filter_dynamic(item_vnums, &partial, CompletionType::ItemVnum)
        }
        // spedit dep add <index> equip <vnum> - show wear slots
        6 if !completing_word
            && words[1].to_lowercase() == "dep"
            && words[2].to_lowercase() == "add"
            && words[4].to_lowercase() == "equip" =>
        {
            all_static(WEAR_SLOTS, CompletionType::WearSlot)
        }
        // spedit dep add <index> equip <vnum> <partial_slot> - complete wear slot
        7 if completing_word
            && words[1].to_lowercase() == "dep"
            && words[2].to_lowercase() == "add"
            && words[4].to_lowercase() == "equip" =>
        {
            filter_static(WEAR_SLOTS, &partial, CompletionType::WearSlot)
        }

        // === dep add with filter (spedit dep <filter> add <index> <type> <vnum>) ===
        // spedit dep <filter> add <index> - show dep types
        5 if !completing_word
            && words[1].to_lowercase() == "dep"
            && is_spedit_filter(words[2])
            && words[3].to_lowercase() == "add" =>
        {
            all_static(SPEDIT_DEP_TYPES, CompletionType::SpeditDepType)
        }
        // spedit dep <filter> add <index> <partial_type> - complete dep type
        6 if completing_word
            && words[1].to_lowercase() == "dep"
            && is_spedit_filter(words[2])
            && words[3].to_lowercase() == "add" =>
        {
            filter_static(SPEDIT_DEP_TYPES, &partial, CompletionType::SpeditDepType)
        }
        // spedit dep <filter> add <index> <type> - show item vnums
        6 if !completing_word
            && words[1].to_lowercase() == "dep"
            && is_spedit_filter(words[2])
            && words[3].to_lowercase() == "add" =>
        {
            all_dynamic(item_vnums, CompletionType::ItemVnum)
        }
        // spedit dep <filter> add <index> <type> <partial_vnum> - complete item vnum
        7 if completing_word
            && words[1].to_lowercase() == "dep"
            && is_spedit_filter(words[2])
            && words[3].to_lowercase() == "add" =>
        {
            filter_dynamic(item_vnums, &partial, CompletionType::ItemVnum)
        }
        // spedit dep <filter> add <index> equip <vnum> - show wear slots
        7 if !completing_word
            && words[1].to_lowercase() == "dep"
            && is_spedit_filter(words[2])
            && words[3].to_lowercase() == "add"
            && words[5].to_lowercase() == "equip" =>
        {
            all_static(WEAR_SLOTS, CompletionType::WearSlot)
        }
        // spedit dep <filter> add <index> equip <vnum> <partial_slot> - complete wear slot
        8 if completing_word
            && words[1].to_lowercase() == "dep"
            && is_spedit_filter(words[2])
            && words[3].to_lowercase() == "add"
            && words[5].to_lowercase() == "equip" =>
        {
            filter_static(WEAR_SLOTS, &partial, CompletionType::WearSlot)
        }

        // === dep add with "room <vnum>" filter (spedit dep room <vnum> add <index> <type> <item_vnum>) ===
        // spedit dep room <vnum> add <index> - show dep types
        6 if !completing_word
            && words[1].to_lowercase() == "dep"
            && words[2].to_lowercase() == "room"
            && words[4].to_lowercase() == "add" =>
        {
            all_static(SPEDIT_DEP_TYPES, CompletionType::SpeditDepType)
        }
        // spedit dep room <vnum> add <index> <partial_type> - complete dep type
        7 if completing_word
            && words[1].to_lowercase() == "dep"
            && words[2].to_lowercase() == "room"
            && words[4].to_lowercase() == "add" =>
        {
            filter_static(SPEDIT_DEP_TYPES, &partial, CompletionType::SpeditDepType)
        }
        // spedit dep room <vnum> add <index> <type> - show item vnums
        7 if !completing_word
            && words[1].to_lowercase() == "dep"
            && words[2].to_lowercase() == "room"
            && words[4].to_lowercase() == "add" =>
        {
            all_dynamic(item_vnums, CompletionType::ItemVnum)
        }
        // spedit dep room <vnum> add <index> <type> <partial_vnum> - complete item vnum
        8 if completing_word
            && words[1].to_lowercase() == "dep"
            && words[2].to_lowercase() == "room"
            && words[4].to_lowercase() == "add" =>
        {
            filter_dynamic(item_vnums, &partial, CompletionType::ItemVnum)
        }
        // spedit dep room <vnum> add <index> equip <vnum> - show wear slots
        8 if !completing_word
            && words[1].to_lowercase() == "dep"
            && words[2].to_lowercase() == "room"
            && words[4].to_lowercase() == "add"
            && words[6].to_lowercase() == "equip" =>
        {
            all_static(WEAR_SLOTS, CompletionType::WearSlot)
        }
        // spedit dep room <vnum> add <index> equip <vnum> <partial_slot> - complete wear slot
        9 if completing_word
            && words[1].to_lowercase() == "dep"
            && words[2].to_lowercase() == "room"
            && words[4].to_lowercase() == "add"
            && words[6].to_lowercase() == "equip" =>
        {
            filter_static(WEAR_SLOTS, &partial, CompletionType::WearSlot)
        }

        _ => CompletionResult::empty(),
    }
}
