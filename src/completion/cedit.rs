use super::consts::*;
use super::helpers::*;
use super::types::*;

/// Context-aware completion for the cedit class kit editor.
///
/// Grammar:
///   cedit <class>                          — complete class id
///   cedit <class> <subcommand>             — show/gold/items
///   cedit <class> items <action>           — add/remove/clear
///   cedit <class> items add <vnum>         — complete from item vnums
///   cedit <class> items remove <vnum>      — could narrow to current kit
///                                            members; we don't have that
///                                            data plumbed yet, so fall back
///                                            to all item vnums.
pub(super) fn complete_cedit(
    words: &[&str],
    completing_word: bool,
    class_ids: &[String],
    item_vnums: &[String],
) -> CompletionResult {
    let partial = get_partial(words, completing_word);

    match words.len() {
        // cedit <partial> — class id (or "list")
        2 if completing_word => {
            // Synthesize "list" alongside class ids so it tab-completes too.
            let mut combined: Vec<String> = class_ids.to_vec();
            combined.push("list".to_string());
            combined.sort();
            filter_dynamic(&combined, &partial, CompletionType::ClassId)
        }
        // cedit <class> — show subcommands
        2 if !completing_word => all_static(CEDIT_SUBCOMMANDS, CompletionType::CeditSubcommand),
        // cedit <class> <partial> — complete subcommand
        3 if completing_word => filter_static(CEDIT_SUBCOMMANDS, &partial, CompletionType::CeditSubcommand),
        // cedit <class> items — show items actions
        3 if !completing_word && words[2].to_lowercase() == "items" => {
            all_static(CEDIT_ITEMS_ACTIONS, CompletionType::CeditItemsAction)
        }
        // cedit <class> items <partial> — complete items action
        4 if completing_word && words[2].to_lowercase() == "items" => {
            filter_static(CEDIT_ITEMS_ACTIONS, &partial, CompletionType::CeditItemsAction)
        }
        // cedit <class> items add — show item vnums
        4 if !completing_word
            && words[2].to_lowercase() == "items"
            && (words[3].to_lowercase() == "add" || words[3].to_lowercase() == "remove") =>
        {
            all_dynamic(item_vnums, CompletionType::ItemVnum)
        }
        // cedit <class> items add <partial_vnum>
        5 if completing_word
            && words[2].to_lowercase() == "items"
            && (words[3].to_lowercase() == "add" || words[3].to_lowercase() == "remove") =>
        {
            filter_dynamic(item_vnums, &partial, CompletionType::ItemVnum)
        }
        _ => CompletionResult::empty(),
    }
}
