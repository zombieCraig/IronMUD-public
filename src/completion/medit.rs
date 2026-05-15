use super::consts::*;
use super::helpers::*;
use super::types::*;

/// Context-aware completion for medit command
pub(super) fn complete_medit(
    words: &[&str],
    completing_word: bool,
    mobile_vnums: &[String],
    item_vnums: &[String],
    transport_vnums: &[String],
    property_template_vnums: &[String],
    shop_preset_vnums: &[String],
    spell_names: &[String],
) -> CompletionResult {
    let partial = get_partial(words, completing_word);

    match words.len() {
        // `medit ` (trailing space, nothing to filter yet) — surface every
        // mobile vnum so the builder can see what's available.
        1 if !completing_word => all_dynamic(mobile_vnums, CompletionType::MobileVnum),
        // medit <partial_vnum> - complete vnum
        2 if completing_word => filter_dynamic(mobile_vnums, &partial, CompletionType::MobileVnum),
        // medit <vnum> - show all subcommands
        2 if !completing_word => all_static(MEDIT_SUBCOMMANDS, CompletionType::MeditSubcommand),
        // medit <vnum> <partial_subcmd> - complete subcommand
        3 if completing_word => filter_static(MEDIT_SUBCOMMANDS, &partial, CompletionType::MeditSubcommand),
        // medit <vnum> trigger - show all trigger actions
        3 if !completing_word && words[2].to_lowercase() == "trigger" => {
            all_static(TRIGGER_ACTIONS, CompletionType::TriggerAction)
        }
        // medit <vnum> trigger <partial_action> - complete trigger action
        4 if completing_word && words[2].to_lowercase() == "trigger" => {
            filter_static(TRIGGER_ACTIONS, &partial, CompletionType::TriggerAction)
        }
        // medit <vnum> trigger add - show all trigger types
        4 if !completing_word && words[2].to_lowercase() == "trigger" && words[3].to_lowercase() == "add" => {
            all_static(TRIGGER_TYPES, CompletionType::TriggerType)
        }
        // medit <vnum> trigger add <partial_type> - complete trigger type
        5 if completing_word && words[2].to_lowercase() == "trigger" && words[3].to_lowercase() == "add" => {
            filter_static(TRIGGER_TYPES, &partial, CompletionType::TriggerType)
        }
        // medit <vnum> trigger add <type> - show all templates
        5 if !completing_word && words[2].to_lowercase() == "trigger" && words[3].to_lowercase() == "add" => {
            all_static(MOBILE_TRIGGER_TEMPLATES, CompletionType::TriggerScript)
        }
        // medit <vnum> trigger add <type> <partial_script> - complete template/script
        6 if completing_word && words[2].to_lowercase() == "trigger" && words[3].to_lowercase() == "add" => {
            filter_static(MOBILE_TRIGGER_TEMPLATES, &partial, CompletionType::TriggerScript)
        }
        // medit <vnum> transport - show all transport actions
        3 if !completing_word && words[2].to_lowercase() == "transport" => {
            all_static(MOBILE_TRANSPORT_ACTIONS, CompletionType::MobileTransportAction)
        }
        // medit <vnum> transport <partial_action> - complete transport action
        4 if completing_word && words[2].to_lowercase() == "transport" => filter_static(
            MOBILE_TRANSPORT_ACTIONS,
            &partial,
            CompletionType::MobileTransportAction,
        ),
        // medit <vnum> transport set - show all transport vnums
        4 if !completing_word && words[2].to_lowercase() == "transport" && words[3].to_lowercase() == "set" => {
            all_dynamic(transport_vnums, CompletionType::TransportVnum)
        }
        // medit <vnum> transport set <partial_vnum> - complete transport vnum
        5 if completing_word && words[2].to_lowercase() == "transport" && words[3].to_lowercase() == "set" => {
            filter_dynamic(transport_vnums, &partial, CompletionType::TransportVnum)
        }
        // medit <vnum> flag - show all mobile flags
        3 if !completing_word && words[2].to_lowercase() == "flag" => {
            all_static(MOBILE_FLAGS, CompletionType::MobileFlag)
        }
        // medit <vnum> flag <partial_flag> - complete flag name
        4 if completing_word && words[2].to_lowercase() == "flag" => {
            filter_static(MOBILE_FLAGS, &partial, CompletionType::MobileFlag)
        }
        // medit <vnum> shop - show all shop subcommands
        3 if !completing_word && words[2].to_lowercase() == "shop" => {
            all_static(SHOP_SUBCOMMANDS, CompletionType::ShopSubcommand)
        }
        // medit <vnum> shop <partial_subcmd> - complete shop subcommand
        4 if completing_word && words[2].to_lowercase() == "shop" => {
            filter_static(SHOP_SUBCOMMANDS, &partial, CompletionType::ShopSubcommand)
        }
        // medit <vnum> shop stock - show stock actions
        4 if !completing_word && words[2].to_lowercase() == "shop" && words[3].to_lowercase() == "stock" => {
            all_static(SHOP_STOCK_ACTIONS, CompletionType::ShopStockAction)
        }
        // medit <vnum> shop stock <partial_action> - complete stock action
        5 if completing_word && words[2].to_lowercase() == "shop" && words[3].to_lowercase() == "stock" => {
            filter_static(SHOP_STOCK_ACTIONS, &partial, CompletionType::ShopStockAction)
        }
        // medit <vnum> shop stock add - show item vnums
        5 if !completing_word
            && words[2].to_lowercase() == "shop"
            && words[3].to_lowercase() == "stock"
            && words[4].to_lowercase() == "add" =>
        {
            all_dynamic(item_vnums, CompletionType::ItemVnum)
        }
        // medit <vnum> shop stock add <partial_vnum> - complete item vnum
        6 if completing_word
            && words[2].to_lowercase() == "shop"
            && words[3].to_lowercase() == "stock"
            && words[4].to_lowercase() == "add" =>
        {
            filter_dynamic(item_vnums, &partial, CompletionType::ItemVnum)
        }
        // medit <vnum> shop categories - show categories actions
        4 if !completing_word && words[2].to_lowercase() == "shop" && words[3].to_lowercase() == "categories" => {
            all_static(SHOP_CATEGORIES_ACTIONS, CompletionType::ShopCategoriesAction)
        }
        // medit <vnum> shop categories <partial_action>
        5 if completing_word && words[2].to_lowercase() == "shop" && words[3].to_lowercase() == "categories" => {
            filter_static(SHOP_CATEGORIES_ACTIONS, &partial, CompletionType::ShopCategoriesAction)
        }
        // medit <vnum> shop preset - show preset actions
        4 if !completing_word && words[2].to_lowercase() == "shop" && words[3].to_lowercase() == "preset" => {
            all_static(SHOP_PRESET_ACTIONS, CompletionType::ShopPresetAction)
        }
        // medit <vnum> shop preset <partial_action>
        5 if completing_word && words[2].to_lowercase() == "shop" && words[3].to_lowercase() == "preset" => {
            filter_static(SHOP_PRESET_ACTIONS, &partial, CompletionType::ShopPresetAction)
        }
        // medit <vnum> shop preset set - show preset vnums
        5 if !completing_word
            && words[2].to_lowercase() == "shop"
            && words[3].to_lowercase() == "preset"
            && words[4].to_lowercase() == "set" =>
        {
            all_dynamic(shop_preset_vnums, CompletionType::ShopPresetVnum)
        }
        // medit <vnum> shop preset set <partial_vnum>
        6 if completing_word
            && words[2].to_lowercase() == "shop"
            && words[3].to_lowercase() == "preset"
            && words[4].to_lowercase() == "set" =>
        {
            filter_dynamic(shop_preset_vnums, &partial, CompletionType::ShopPresetVnum)
        }
        // medit <vnum> leasing - show all leasing subcommands
        3 if !completing_word && words[2].to_lowercase() == "leasing" => {
            all_static(LEASING_SUBCOMMANDS, CompletionType::LeasingSubcommand)
        }
        // medit <vnum> leasing <partial_subcmd> - complete leasing subcommand
        4 if completing_word && words[2].to_lowercase() == "leasing" => {
            filter_static(LEASING_SUBCOMMANDS, &partial, CompletionType::LeasingSubcommand)
        }
        // medit <vnum> leasing add - show property template vnums
        4 if !completing_word && words[2].to_lowercase() == "leasing" && words[3].to_lowercase() == "add" => {
            all_dynamic(property_template_vnums, CompletionType::PropertyTemplateVnum)
        }
        // medit <vnum> leasing add <partial_vnum> - complete property template vnum
        5 if completing_word && words[2].to_lowercase() == "leasing" && words[3].to_lowercase() == "add" => {
            filter_dynamic(property_template_vnums, &partial, CompletionType::PropertyTemplateVnum)
        }
        // medit <vnum> leasing remove - show property template vnums
        4 if !completing_word && words[2].to_lowercase() == "leasing" && words[3].to_lowercase() == "remove" => {
            all_dynamic(property_template_vnums, CompletionType::PropertyTemplateVnum)
        }
        // medit <vnum> leasing remove <partial_vnum> - complete property template vnum
        5 if completing_word && words[2].to_lowercase() == "leasing" && words[3].to_lowercase() == "remove" => {
            filter_dynamic(property_template_vnums, &partial, CompletionType::PropertyTemplateVnum)
        }
        // medit <vnum> damtype - show all damage types
        3 if !completing_word && words[2].to_lowercase() == "damtype" => {
            all_static(DAMAGE_TYPES, CompletionType::DamageType)
        }
        // medit <vnum> damtype <partial_type> - complete damage type
        4 if completing_word && words[2].to_lowercase() == "damtype" => {
            filter_static(DAMAGE_TYPES, &partial, CompletionType::DamageType)
        }
        // medit <vnum> simulation - show all simulation subcommands
        3 if !completing_word && (words[2].to_lowercase() == "simulation" || words[2].to_lowercase() == "sim") => {
            all_static(SIMULATION_SUBCOMMANDS, CompletionType::SimulationSubcommand)
        }
        // medit <vnum> simulation <partial_subcmd> - complete simulation subcommand
        4 if completing_word && (words[2].to_lowercase() == "simulation" || words[2].to_lowercase() == "sim") => {
            filter_static(SIMULATION_SUBCOMMANDS, &partial, CompletionType::SimulationSubcommand)
        }
        // medit <vnum> routine - show all routine subcommands
        3 if !completing_word && words[2].to_lowercase() == "routine" => {
            all_static(ROUTINE_SUBCOMMANDS, CompletionType::RoutineSubcommand)
        }
        // medit <vnum> routine <partial_subcmd> - complete routine subcommand
        4 if completing_word && words[2].to_lowercase() == "routine" => {
            filter_static(ROUTINE_SUBCOMMANDS, &partial, CompletionType::RoutineSubcommand)
        }
        // medit <vnum> routine add <hour> - show activity states
        5 if !completing_word && words[2].to_lowercase() == "routine" && words[3].to_lowercase() == "add" => {
            all_static(ACTIVITY_STATES, CompletionType::ActivityState)
        }
        // medit <vnum> routine add <hour> <partial_activity> - complete activity state
        6 if completing_word && words[2].to_lowercase() == "routine" && words[3].to_lowercase() == "add" => {
            filter_static(ACTIVITY_STATES, &partial, CompletionType::ActivityState)
        }
        // medit <vnum> combat_spells - show all action keywords
        3 if !completing_word
            && (words[2].to_lowercase() == "combat_spells" || words[2].to_lowercase() == "spells") =>
        {
            all_static(COMBAT_SPELLS_ACTIONS, CompletionType::CombatSpellsAction)
        }
        // medit <vnum> combat_spells <partial_action>
        4 if completing_word
            && (words[2].to_lowercase() == "combat_spells" || words[2].to_lowercase() == "spells") =>
        {
            filter_static(COMBAT_SPELLS_ACTIONS, &partial, CompletionType::CombatSpellsAction)
        }
        // medit <vnum> combat_spells add | remove - show all spell names
        4 if !completing_word
            && (words[2].to_lowercase() == "combat_spells" || words[2].to_lowercase() == "spells")
            && (words[3].to_lowercase() == "add"
                || words[3].to_lowercase() == "remove"
                || words[3].to_lowercase() == "rm") =>
        {
            all_dynamic(spell_names, CompletionType::SpellName)
        }
        // medit <vnum> combat_spells add | remove <partial_spell>
        5 if completing_word
            && (words[2].to_lowercase() == "combat_spells" || words[2].to_lowercase() == "spells")
            && (words[3].to_lowercase() == "add"
                || words[3].to_lowercase() == "remove"
                || words[3].to_lowercase() == "rm") =>
        {
            filter_dynamic(spell_names, &partial, CompletionType::SpellName)
        }
        _ => CompletionResult::empty(),
    }
}
