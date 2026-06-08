use super::consts::*;
use super::helpers::*;
use super::types::*;

/// Context-aware completion for redit command (edits current room)
pub(super) fn complete_redit(words: &[&str], completing_word: bool) -> CompletionResult {
    let partial = get_partial(words, completing_word);

    match words.len() {
        // redit - show all subcommands
        1 if !completing_word => all_static(REDIT_SUBCOMMANDS, CompletionType::ReditSubcommand),
        // redit <partial_subcmd> - complete subcommand
        2 if completing_word => filter_static(REDIT_SUBCOMMANDS, &partial, CompletionType::ReditSubcommand),
        // redit flag - show all flags
        2 if !completing_word && words[1].to_lowercase() == "flag" => all_static(ROOM_FLAGS, CompletionType::RoomFlag),
        // redit flag <partial_flag> - complete flag name
        3 if completing_word && words[1].to_lowercase() == "flag" => {
            filter_static(ROOM_FLAGS, &partial, CompletionType::RoomFlag)
        }
        // redit zone - show combat zone types
        2 if !completing_word && words[1].to_lowercase() == "zone" => {
            all_static(COMBAT_ZONE_TYPES, CompletionType::CombatZone)
        }
        // redit zone <partial_type> - complete zone type
        3 if completing_word && words[1].to_lowercase() == "zone" => {
            filter_static(COMBAT_ZONE_TYPES, &partial, CompletionType::CombatZone)
        }
        // redit water - show water types
        2 if !completing_word && words[1].to_lowercase() == "water" => {
            all_static(WATER_TYPES, CompletionType::WaterType)
        }
        // redit water <partial_type> - complete water type
        3 if completing_word && words[1].to_lowercase() == "water" => {
            filter_static(WATER_TYPES, &partial, CompletionType::WaterType)
        }
        // redit door - show door subcommands
        2 if !completing_word && words[1].to_lowercase() == "door" => {
            all_static(DOOR_SUBCOMMANDS, CompletionType::DoorSubcommand)
        }
        // redit door <partial_subcmd> - complete door subcommand
        3 if completing_word && words[1].to_lowercase() == "door" => {
            filter_static(DOOR_SUBCOMMANDS, &partial, CompletionType::DoorSubcommand)
        }
        // redit door <subcmd> - show directions (for subcommands that take direction)
        3 if !completing_word && words[1].to_lowercase() == "door" => all_static(DIRECTIONS, CompletionType::Direction),
        // redit door <subcmd> <partial_dir> - complete direction
        4 if completing_word && words[1].to_lowercase() == "door" => {
            filter_static(DIRECTIONS, &partial, CompletionType::Direction)
        }
        // redit extra - show extra actions
        2 if !completing_word && words[1].to_lowercase() == "extra" => {
            all_static(EXTRA_DESC_ACTIONS, CompletionType::ExtraDescAction)
        }
        // redit extra <partial_action> - complete extra action
        3 if completing_word && words[1].to_lowercase() == "extra" => {
            filter_static(EXTRA_DESC_ACTIONS, &partial, CompletionType::ExtraDescAction)
        }
        // redit trigger - show trigger actions
        2 if !completing_word && words[1].to_lowercase() == "trigger" => {
            all_static(ROOM_TRIGGER_ACTIONS, CompletionType::RoomTriggerAction)
        }
        // redit trigger <partial_action> - complete trigger action
        3 if completing_word && words[1].to_lowercase() == "trigger" => {
            filter_static(ROOM_TRIGGER_ACTIONS, &partial, CompletionType::RoomTriggerAction)
        }
        // redit trigger add - show trigger types
        3 if !completing_word && words[1].to_lowercase() == "trigger" && words[2].to_lowercase() == "add" => {
            all_static(ROOM_TRIGGER_TYPES, CompletionType::RoomTriggerType)
        }
        // redit trigger add <partial_type> - complete trigger type
        4 if completing_word && words[1].to_lowercase() == "trigger" && words[2].to_lowercase() == "add" => {
            filter_static(ROOM_TRIGGER_TYPES, &partial, CompletionType::RoomTriggerType)
        }
        // redit trigger add <type> - show all templates
        4 if !completing_word && words[1].to_lowercase() == "trigger" && words[2].to_lowercase() == "add" => {
            all_static(ROOM_TRIGGER_TEMPLATES, CompletionType::TriggerScript)
        }
        // redit trigger add <type> <partial_script> - complete template/script
        5 if completing_word && words[1].to_lowercase() == "trigger" && words[2].to_lowercase() == "add" => {
            filter_static(ROOM_TRIGGER_TEMPLATES, &partial, CompletionType::TriggerScript)
        }
        // redit trigger dg - show dg subcommands
        3 if !completing_word && words[1].to_lowercase() == "trigger" && words[2].to_lowercase() == "dg" => {
            all_static(TRIGGER_DG_SUBCOMMANDS, CompletionType::TriggerDgSubcommand)
        }
        4 if completing_word && words[1].to_lowercase() == "trigger" && words[2].to_lowercase() == "dg" => {
            filter_static(TRIGGER_DG_SUBCOMMANDS, &partial, CompletionType::TriggerDgSubcommand)
        }
        // redit trigger dg add - dg room trigger types
        4 if !completing_word
            && words[1].to_lowercase() == "trigger"
            && words[2].to_lowercase() == "dg"
            && words[3].to_lowercase() == "add" =>
        {
            all_static(DG_ROOM_TRIGGER_TYPES, CompletionType::DgRoomTriggerType)
        }
        5 if completing_word
            && words[1].to_lowercase() == "trigger"
            && words[2].to_lowercase() == "dg"
            && words[3].to_lowercase() == "add" =>
        {
            filter_static(DG_ROOM_TRIGGER_TYPES, &partial, CompletionType::DgRoomTriggerType)
        }
        // redit trigger dg retype <idx> <partial_type>
        5 if !completing_word
            && words[1].to_lowercase() == "trigger"
            && words[2].to_lowercase() == "dg"
            && words[3].to_lowercase() == "retype" =>
        {
            all_static(DG_ROOM_TRIGGER_TYPES, CompletionType::DgRoomTriggerType)
        }
        6 if completing_word
            && words[1].to_lowercase() == "trigger"
            && words[2].to_lowercase() == "dg"
            && words[3].to_lowercase() == "retype" =>
        {
            filter_static(DG_ROOM_TRIGGER_TYPES, &partial, CompletionType::DgRoomTriggerType)
        }
        // redit trigger dg proto - proto subcommands
        4 if !completing_word
            && words[1].to_lowercase() == "trigger"
            && words[2].to_lowercase() == "dg"
            && words[3].to_lowercase() == "proto" =>
        {
            all_static(TRIGGER_DG_PROTO_SUBCOMMANDS, CompletionType::TriggerDgProtoSubcommand)
        }
        5 if completing_word
            && words[1].to_lowercase() == "trigger"
            && words[2].to_lowercase() == "dg"
            && words[3].to_lowercase() == "proto" =>
        {
            filter_static(
                TRIGGER_DG_PROTO_SUBCOMMANDS,
                &partial,
                CompletionType::TriggerDgProtoSubcommand,
            )
        }
        // redit trigger dg proto retype <vnum> <partial_type>
        7 if completing_word
            && words[1].to_lowercase() == "trigger"
            && words[2].to_lowercase() == "dg"
            && words[3].to_lowercase() == "proto"
            && words[4].to_lowercase() == "retype" =>
        {
            filter_static(DG_ROOM_TRIGGER_TYPES, &partial, CompletionType::DgRoomTriggerType)
        }
        _ => CompletionResult::empty(),
    }
}

/// Context-aware completion for rcopy command
pub(super) fn complete_rcopy(words: &[&str], completing_word: bool, room_vnums: &[String]) -> CompletionResult {
    let partial = get_partial(words, completing_word);

    match words.len() {
        // rcopy <partial_vnum> - complete room vnum
        2 if completing_word => filter_dynamic(room_vnums, &partial, CompletionType::RoomVnum),
        // rcopy <vnum> - show categories
        2 if !completing_word => all_static(RCOPY_CATEGORIES, CompletionType::RcopyCategory),
        // rcopy <vnum> <partial_category> - complete category
        3 if completing_word => filter_static(RCOPY_CATEGORIES, &partial, CompletionType::RcopyCategory),
        _ => CompletionResult::empty(),
    }
}
