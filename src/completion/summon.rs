use super::helpers::*;
use super::types::*;

/// Context-aware completion for summon command
/// Syntax: summon mob <vnum> [room_vnum] | summon player <name> [room_vnum]
pub(super) fn complete_summon(
    words: &[&str],
    completing_word: bool,
    mobile_vnums: &[String],
    online_players: &[String],
    room_vnums: &[String],
) -> CompletionResult {
    let partial = get_partial(words, completing_word);

    match words.len() {
        // summon - show type keywords
        1 if !completing_word => all_static(&["mob", "player"], CompletionType::SummonTarget),
        // summon <partial> - complete type keyword
        2 if completing_word => filter_static(&["mob", "player"], &partial, CompletionType::SummonTarget),
        // summon mob - show mobile vnums
        2 if !completing_word && words[1].to_lowercase() == "mob" => {
            all_dynamic(mobile_vnums, CompletionType::MobileVnum)
        }
        // summon player - show online players
        2 if !completing_word && words[1].to_lowercase() == "player" => {
            all_dynamic(online_players, CompletionType::PlayerName)
        }
        // summon mob <partial_vnum> - complete mobile vnum
        3 if completing_word && words[1].to_lowercase() == "mob" => {
            filter_dynamic(mobile_vnums, &partial, CompletionType::MobileVnum)
        }
        // summon player <partial_name> - complete player name
        3 if completing_word && words[1].to_lowercase() == "player" => {
            filter_dynamic(online_players, &partial, CompletionType::PlayerName)
        }
        // summon mob <vnum> - show room vnums
        3 if !completing_word => all_dynamic(room_vnums, CompletionType::RoomVnum),
        // summon mob/player <id> <partial_room> - complete room vnum
        4 if completing_word => filter_dynamic(room_vnums, &partial, CompletionType::RoomVnum),
        _ => CompletionResult::empty(),
    }
}
