use super::consts::*;
use super::helpers::*;
use super::types::*;

/// Context-aware completion for bpredit command
pub(super) fn complete_bpredit(words: &[&str], completing_word: bool, shop_preset_vnums: &[String]) -> CompletionResult {
    let partial = get_partial(words, completing_word);

    match words.len() {
        // `bpredit ` (trailing space) — surface the static commands plus every preset vnum.
        1 if !completing_word => {
            let mut matches: Vec<String> = vec!["list".to_string(), "create".to_string(), "delete".to_string()];
            matches.extend(shop_preset_vnums.iter().cloned());
            CompletionResult::new(matches, "", CompletionType::ShopPresetVnum)
        }
        // bpredit <partial_vnum> - complete vnum (also matches "list", "create", "delete")
        2 if completing_word => {
            // Combine static subcommands and dynamic vnums
            let static_cmds = &["list", "create", "delete"];
            let mut matches: Vec<String> = static_cmds
                .iter()
                .filter(|s| s.starts_with(&partial))
                .map(|s| s.to_string())
                .collect();
            matches.extend(
                shop_preset_vnums
                    .iter()
                    .filter(|v| v.to_lowercase().starts_with(&partial))
                    .cloned(),
            );
            CompletionResult::new(matches, &partial, CompletionType::ShopPresetVnum)
        }
        // bpredit <vnum> - show subcommands
        2 if !completing_word => all_static(BPREDIT_SUBCOMMANDS, CompletionType::BpreditSubcommand),
        // bpredit <vnum> <partial_subcmd>
        3 if completing_word => filter_static(BPREDIT_SUBCOMMANDS, &partial, CompletionType::BpreditSubcommand),
        // bpredit <vnum> type - show add/remove
        3 if !completing_word && words[2].to_lowercase() == "type" => {
            all_static(SHOP_STOCK_ACTIONS, CompletionType::ShopStockAction)
        }
        // bpredit <vnum> type <partial_action>
        4 if completing_word && words[2].to_lowercase() == "type" => {
            filter_static(SHOP_STOCK_ACTIONS, &partial, CompletionType::ShopStockAction)
        }
        // bpredit <vnum> type add - show item types
        4 if !completing_word && words[2].to_lowercase() == "type" && words[3].to_lowercase() == "add" => {
            all_static(ITEM_TYPES, CompletionType::ItemType)
        }
        // bpredit <vnum> type add <partial_type>
        5 if completing_word && words[2].to_lowercase() == "type" && words[3].to_lowercase() == "add" => {
            filter_static(ITEM_TYPES, &partial, CompletionType::ItemType)
        }
        // bpredit <vnum> category - show add/remove
        3 if !completing_word && words[2].to_lowercase() == "category" => {
            all_static(SHOP_STOCK_ACTIONS, CompletionType::ShopStockAction)
        }
        // bpredit <vnum> category <partial_action>
        4 if completing_word && words[2].to_lowercase() == "category" => {
            filter_static(SHOP_STOCK_ACTIONS, &partial, CompletionType::ShopStockAction)
        }
        _ => CompletionResult::empty(),
    }
}
