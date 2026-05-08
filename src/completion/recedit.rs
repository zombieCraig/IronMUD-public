use super::consts::*;
use super::helpers::*;
use super::types::*;

/// Context-aware completion for recedit command
pub(super) fn complete_recedit(
    words: &[&str],
    completing_word: bool,
    recipe_vnums: &[String],
    item_vnums: &[String],
) -> CompletionResult {
    let partial = get_partial(words, completing_word);

    match words.len() {
        // recedit <partial_vnum> - complete recipe vnum
        2 if completing_word => filter_dynamic(recipe_vnums, &partial, CompletionType::RecipeVnum),
        // recedit <vnum> - show all subcommands
        2 if !completing_word => all_static(RECEDIT_SUBCOMMANDS, CompletionType::ReceditSubcommand),
        // recedit <vnum> <partial_subcmd> - complete subcommand
        3 if completing_word => filter_static(RECEDIT_SUBCOMMANDS, &partial, CompletionType::ReceditSubcommand),
        // recedit <vnum> skill - show skill types
        3 if !completing_word && words[2].to_lowercase() == "skill" => {
            all_static(RECIPE_SKILLS, CompletionType::RecipeSkill)
        }
        // recedit <vnum> output - show item vnums (hint)
        3 if !completing_word && words[2].to_lowercase() == "output" => {
            all_dynamic(item_vnums, CompletionType::ItemVnum)
        }
        // recedit <vnum> skill <partial> - complete skill
        4 if completing_word && words[2].to_lowercase() == "skill" => {
            filter_static(RECIPE_SKILLS, &partial, CompletionType::RecipeSkill)
        }
        // recedit <vnum> autolearn - show on/off
        3 if !completing_word && words[2].to_lowercase() == "autolearn" => {
            all_static(SET_TOGGLE_VALUES, CompletionType::SetSubcommand)
        }
        // recedit <vnum> autolearn <partial> - complete on/off
        4 if completing_word && words[2].to_lowercase() == "autolearn" => {
            filter_static(SET_TOGGLE_VALUES, &partial, CompletionType::SetSubcommand)
        }
        // recedit <vnum> output <partial_item_vnum> - complete item vnum
        4 if completing_word && words[2].to_lowercase() == "output" => {
            filter_dynamic(item_vnums, &partial, CompletionType::ItemVnum)
        }
        // recedit <vnum> ingredient - show actions
        3 if !completing_word && words[2].to_lowercase() == "ingredient" => {
            all_static(INGREDIENT_ACTIONS, CompletionType::IngredientAction)
        }
        // recedit <vnum> ingredient <partial> - complete action
        4 if completing_word && words[2].to_lowercase() == "ingredient" => {
            filter_static(INGREDIENT_ACTIONS, &partial, CompletionType::IngredientAction)
        }
        // recedit <vnum> ingredient add - show item vnums
        4 if !completing_word && words[2].to_lowercase() == "ingredient" && words[3].to_lowercase() == "add" => {
            all_dynamic(item_vnums, CompletionType::ItemVnum)
        }
        // recedit <vnum> ingredient add <partial_vnum> - complete item vnum
        5 if completing_word && words[2].to_lowercase() == "ingredient" && words[3].to_lowercase() == "add" => {
            filter_dynamic(item_vnums, &partial, CompletionType::ItemVnum)
        }
        // recedit <vnum> tool - show actions
        3 if !completing_word && words[2].to_lowercase() == "tool" => {
            all_static(TOOL_ACTIONS, CompletionType::ToolAction)
        }
        // recedit <vnum> tool <partial> - complete action
        4 if completing_word && words[2].to_lowercase() == "tool" => {
            filter_static(TOOL_ACTIONS, &partial, CompletionType::ToolAction)
        }
        // recedit <vnum> tool add - show item vnums
        4 if !completing_word && words[2].to_lowercase() == "tool" && words[3].to_lowercase() == "add" => {
            all_dynamic(item_vnums, CompletionType::ItemVnum)
        }
        // recedit <vnum> tool add <partial_vnum> - complete item vnum
        5 if completing_word && words[2].to_lowercase() == "tool" && words[3].to_lowercase() == "add" => {
            filter_dynamic(item_vnums, &partial, CompletionType::ItemVnum)
        }
        // recedit <vnum> tool add <spec> - show locations
        5 if !completing_word && words[2].to_lowercase() == "tool" && words[3].to_lowercase() == "add" => {
            all_static(TOOL_LOCATIONS, CompletionType::ToolLocation)
        }
        // recedit <vnum> tool add <spec> <partial_loc> - complete location
        6 if completing_word && words[2].to_lowercase() == "tool" && words[3].to_lowercase() == "add" => {
            filter_static(TOOL_LOCATIONS, &partial, CompletionType::ToolLocation)
        }
        _ => CompletionResult::empty(),
    }
}

/// Context-aware completion for reclist command
pub(super) fn complete_reclist(words: &[&str], completing_word: bool) -> CompletionResult {
    let partial = get_partial(words, completing_word);

    match words.len() {
        // reclist - show skill filters
        1 if !completing_word => all_static(RECIPE_SKILLS, CompletionType::RecipeSkill),
        // reclist <partial_skill> - complete skill filter
        2 if completing_word => filter_static(RECIPE_SKILLS, &partial, CompletionType::RecipeSkill),
        _ => CompletionResult::empty(),
    }
}
