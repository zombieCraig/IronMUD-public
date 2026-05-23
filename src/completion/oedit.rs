use super::consts::*;
use super::helpers::*;
use super::types::*;

/// Context-aware completion for oedit command
pub(super) fn complete_oedit(
    words: &[&str],
    completing_word: bool,
    item_vnums: &[String],
    transport_vnums: &[String],
    spell_names: &[String],
    custom_skill_keys: &[String],
) -> CompletionResult {
    let partial = get_partial(words, completing_word);

    match words.len() {
        // `oedit ` (trailing space) — surface every item vnum.
        1 if !completing_word => all_dynamic(item_vnums, CompletionType::ItemVnum),
        // oedit <partial_vnum> - complete vnum
        2 if completing_word => filter_dynamic(item_vnums, &partial, CompletionType::ItemVnum),
        // oedit <vnum> - show all subcommands
        2 if !completing_word => all_static(OEDIT_SUBCOMMANDS, CompletionType::OeditSubcommand),
        // oedit <vnum> <partial_subcmd> - complete subcommand
        3 if completing_word => filter_static(OEDIT_SUBCOMMANDS, &partial, CompletionType::OeditSubcommand),
        // oedit <vnum> type - show all item types
        3 if !completing_word && words[2].to_lowercase() == "type" => all_static(ITEM_TYPES, CompletionType::ItemType),
        // oedit <vnum> type <partial_type> - complete item type
        4 if completing_word && words[2].to_lowercase() == "type" => {
            filter_static(ITEM_TYPES, &partial, CompletionType::ItemType)
        }
        // oedit <vnum> extra - show extra actions
        3 if !completing_word && words[2].to_lowercase() == "extra" => {
            all_static(EXTRA_DESC_ACTIONS, CompletionType::ExtraDescAction)
        }
        // oedit <vnum> extra <partial_action> - complete extra action
        4 if completing_word && words[2].to_lowercase() == "extra" => {
            filter_static(EXTRA_DESC_ACTIONS, &partial, CompletionType::ExtraDescAction)
        }
        // oedit <vnum> trigger - show all trigger actions
        3 if !completing_word && words[2].to_lowercase() == "trigger" => {
            all_static(ITEM_TRIGGER_ACTIONS, CompletionType::ItemTriggerAction)
        }
        // oedit <vnum> trigger <partial_action> - complete trigger action
        4 if completing_word && words[2].to_lowercase() == "trigger" => {
            filter_static(ITEM_TRIGGER_ACTIONS, &partial, CompletionType::ItemTriggerAction)
        }
        // oedit <vnum> trigger add - show all trigger types
        4 if !completing_word && words[2].to_lowercase() == "trigger" && words[3].to_lowercase() == "add" => {
            all_static(ITEM_TRIGGER_TYPES, CompletionType::ItemTriggerType)
        }
        // oedit <vnum> trigger add <partial_type> - complete trigger type
        5 if completing_word && words[2].to_lowercase() == "trigger" && words[3].to_lowercase() == "add" => {
            filter_static(ITEM_TRIGGER_TYPES, &partial, CompletionType::ItemTriggerType)
        }
        // oedit <vnum> trigger add <type> - show all templates
        5 if !completing_word && words[2].to_lowercase() == "trigger" && words[3].to_lowercase() == "add" => {
            all_static(ITEM_TRIGGER_TEMPLATES, CompletionType::TriggerScript)
        }
        // oedit <vnum> trigger add <type> <partial_script> - complete template/script
        6 if completing_word && words[2].to_lowercase() == "trigger" && words[3].to_lowercase() == "add" => {
            filter_static(ITEM_TRIGGER_TEMPLATES, &partial, CompletionType::TriggerScript)
        }
        // oedit <vnum> trigger dg - show dg subcommands
        4 if !completing_word && words[2].to_lowercase() == "trigger" && words[3].to_lowercase() == "dg" => {
            all_static(TRIGGER_DG_SUBCOMMANDS, CompletionType::TriggerDgSubcommand)
        }
        // oedit <vnum> trigger dg <partial_subcmd>
        5 if completing_word && words[2].to_lowercase() == "trigger" && words[3].to_lowercase() == "dg" => {
            filter_static(TRIGGER_DG_SUBCOMMANDS, &partial, CompletionType::TriggerDgSubcommand)
        }
        // oedit <vnum> trigger dg add - dg item trigger types
        5 if !completing_word
            && words[2].to_lowercase() == "trigger"
            && words[3].to_lowercase() == "dg"
            && words[4].to_lowercase() == "add" =>
        {
            all_static(DG_ITEM_TRIGGER_TYPES, CompletionType::DgItemTriggerType)
        }
        6 if completing_word
            && words[2].to_lowercase() == "trigger"
            && words[3].to_lowercase() == "dg"
            && words[4].to_lowercase() == "add" =>
        {
            filter_static(DG_ITEM_TRIGGER_TYPES, &partial, CompletionType::DgItemTriggerType)
        }
        // oedit <vnum> trigger dg retype <idx> <partial_type>
        6 if !completing_word
            && words[2].to_lowercase() == "trigger"
            && words[3].to_lowercase() == "dg"
            && words[4].to_lowercase() == "retype" =>
        {
            all_static(DG_ITEM_TRIGGER_TYPES, CompletionType::DgItemTriggerType)
        }
        7 if completing_word
            && words[2].to_lowercase() == "trigger"
            && words[3].to_lowercase() == "dg"
            && words[4].to_lowercase() == "retype" =>
        {
            filter_static(DG_ITEM_TRIGGER_TYPES, &partial, CompletionType::DgItemTriggerType)
        }
        // oedit <vnum> trigger dg proto - proto subcommands
        5 if !completing_word
            && words[2].to_lowercase() == "trigger"
            && words[3].to_lowercase() == "dg"
            && words[4].to_lowercase() == "proto" =>
        {
            all_static(TRIGGER_DG_PROTO_SUBCOMMANDS, CompletionType::TriggerDgProtoSubcommand)
        }
        6 if completing_word
            && words[2].to_lowercase() == "trigger"
            && words[3].to_lowercase() == "dg"
            && words[4].to_lowercase() == "proto" =>
        {
            filter_static(TRIGGER_DG_PROTO_SUBCOMMANDS, &partial, CompletionType::TriggerDgProtoSubcommand)
        }
        // oedit <vnum> trigger dg proto retype <vnum> <partial_type>
        8 if completing_word
            && words[2].to_lowercase() == "trigger"
            && words[3].to_lowercase() == "dg"
            && words[4].to_lowercase() == "proto"
            && words[5].to_lowercase() == "retype" =>
        {
            filter_static(DG_ITEM_TRIGGER_TYPES, &partial, CompletionType::DgItemTriggerType)
        }
        // oedit <vnum> transport - show all transport vnums + clear
        3 if !completing_word && words[2].to_lowercase() == "transport" => {
            let mut matches: Vec<String> = transport_vnums.to_vec();
            matches.push("clear".to_string());
            CompletionResult::new(matches, "", CompletionType::TransportVnum)
        }
        // oedit <vnum> transport <partial> - complete transport vnum + clear
        4 if completing_word && words[2].to_lowercase() == "transport" => {
            let mut matches: Vec<String> = transport_vnums
                .iter()
                .filter(|v| v.to_lowercase().starts_with(&partial))
                .cloned()
                .collect();
            if "clear".starts_with(&partial) {
                matches.push("clear".to_string());
            }
            CompletionResult::new(matches, &partial, CompletionType::TransportVnum)
        }
        // oedit <vnum> flag - show all item flags
        3 if !completing_word && words[2].to_lowercase() == "flag" => all_static(ITEM_FLAGS, CompletionType::ItemFlag),
        // oedit <vnum> flag <partial_flag> - complete flag name
        4 if completing_word && words[2].to_lowercase() == "flag" => {
            filter_static(ITEM_FLAGS, &partial, CompletionType::ItemFlag)
        }
        // oedit <vnum> vending - show vending subcommands
        3 if !completing_word && words[2].to_lowercase() == "vending" => {
            all_static(VENDING_SUBCOMMANDS, CompletionType::VendingSubcommand)
        }
        // oedit <vnum> vending <partial_subcmd> - complete vending subcommand
        4 if completing_word && words[2].to_lowercase() == "vending" => {
            filter_static(VENDING_SUBCOMMANDS, &partial, CompletionType::VendingSubcommand)
        }
        // oedit <vnum> vending stock - show stock actions
        4 if !completing_word && words[2].to_lowercase() == "vending" && words[3].to_lowercase() == "stock" => {
            all_static(SHOP_STOCK_ACTIONS, CompletionType::ShopStockAction)
        }
        // oedit <vnum> vending stock <partial_action> - complete stock action
        5 if completing_word && words[2].to_lowercase() == "vending" && words[3].to_lowercase() == "stock" => {
            filter_static(SHOP_STOCK_ACTIONS, &partial, CompletionType::ShopStockAction)
        }
        // oedit <vnum> vending stock add - show item vnums
        5 if !completing_word
            && words[2].to_lowercase() == "vending"
            && words[3].to_lowercase() == "stock"
            && words[4].to_lowercase() == "add" =>
        {
            all_dynamic(item_vnums, CompletionType::ItemVnum)
        }
        // oedit <vnum> vending stock add <partial_vnum> - complete item vnum
        6 if completing_word
            && words[2].to_lowercase() == "vending"
            && words[3].to_lowercase() == "stock"
            && words[4].to_lowercase() == "add" =>
        {
            filter_dynamic(item_vnums, &partial, CompletionType::ItemVnum)
        }
        // oedit <vnum> damtype - show all damage types
        3 if !completing_word && words[2].to_lowercase() == "damtype" => {
            all_static(DAMAGE_TYPES, CompletionType::DamageType)
        }
        // oedit <vnum> damtype <partial_type> - complete damage type
        4 if completing_word && words[2].to_lowercase() == "damtype" => {
            filter_static(DAMAGE_TYPES, &partial, CompletionType::DamageType)
        }
        // oedit <vnum> rangedtype - show all ranged types
        3 if !completing_word && (words[2].to_lowercase() == "rangedtype" || words[2].to_lowercase() == "rtype") => {
            all_static(RANGED_TYPES, CompletionType::OeditSubcommand)
        }
        // oedit <vnum> rangedtype <partial> - complete ranged type
        4 if completing_word && (words[2].to_lowercase() == "rangedtype" || words[2].to_lowercase() == "rtype") => {
            filter_static(RANGED_TYPES, &partial, CompletionType::OeditSubcommand)
        }
        // oedit <vnum> firemode - show all fire modes
        3 if !completing_word && words[2].to_lowercase() == "firemode" => {
            all_static(FIRE_MODES, CompletionType::OeditSubcommand)
        }
        // oedit <vnum> firemode <partial> - complete fire mode
        4 if completing_word && words[2].to_lowercase() == "firemode" => {
            filter_static(FIRE_MODES, &partial, CompletionType::OeditSubcommand)
        }
        // oedit <vnum> noise - show all noise levels
        3 if !completing_word && (words[2].to_lowercase() == "noise" || words[2].to_lowercase() == "noiselevel") => {
            all_static(NOISE_LEVELS, CompletionType::OeditSubcommand)
        }
        // oedit <vnum> noise <partial> - complete noise level
        4 if completing_word && (words[2].to_lowercase() == "noise" || words[2].to_lowercase() == "noiselevel") => {
            filter_static(NOISE_LEVELS, &partial, CompletionType::OeditSubcommand)
        }
        // oedit <vnum> firemodes - show all fire modes (multi-select)
        3 if !completing_word && words[2].to_lowercase() == "firemodes" => {
            all_static(FIRE_MODES, CompletionType::OeditSubcommand)
        }
        // oedit <vnum> firemodes <partial> - complete fire modes
        4.. if completing_word && words[2].to_lowercase() == "firemodes" => {
            filter_static(FIRE_MODES, &partial, CompletionType::OeditSubcommand)
        }
        // oedit <vnum> cast_on_use - show spell ids + show/clear actions
        3 if !completing_word
            && (words[2].to_lowercase() == "cast_on_use" || words[2].to_lowercase() == "spell") =>
        {
            let mut matches: Vec<String> = spell_names.to_vec();
            matches.push("show".to_string());
            matches.push("clear".to_string());
            CompletionResult::new(matches, "", CompletionType::SpellName)
        }
        // oedit <vnum> cast_on_use <partial> - complete spell id (or show/clear)
        4 if completing_word
            && (words[2].to_lowercase() == "cast_on_use" || words[2].to_lowercase() == "spell") =>
        {
            let mut matches: Vec<String> = spell_names
                .iter()
                .filter(|s| s.to_lowercase().starts_with(&partial))
                .cloned()
                .collect();
            for verb in ["show", "clear"] {
                if verb.starts_with(&partial) {
                    matches.push(verb.to_string());
                }
            }
            CompletionResult::new(matches, &partial, CompletionType::SpellName)
        }
        // oedit <vnum> teaches_spell - show spell ids + clear
        3 if !completing_word && words[2].to_lowercase() == "teaches_spell" => {
            let mut matches: Vec<String> = spell_names.to_vec();
            matches.push("clear".to_string());
            CompletionResult::new(matches, "", CompletionType::SpellName)
        }
        // oedit <vnum> teaches_spell <partial> - complete spell id
        4 if completing_word && words[2].to_lowercase() == "teaches_spell" => {
            let mut matches: Vec<String> = spell_names
                .iter()
                .filter(|s| s.to_lowercase().starts_with(&partial))
                .cloned()
                .collect();
            if "clear".starts_with(&partial) {
                matches.push("clear".to_string());
            }
            CompletionResult::new(matches, &partial, CompletionType::SpellName)
        }
        // oedit <vnum> affect - show affect sub-actions
        3 if !completing_word && (words[2].to_lowercase() == "affect" || words[2].to_lowercase() == "affects") => {
            all_static(AFFECT_ACTIONS, CompletionType::AffectAction)
        }
        // oedit <vnum> affect <partial_action> - complete affect action
        4 if completing_word && (words[2].to_lowercase() == "affect" || words[2].to_lowercase() == "affects") => {
            filter_static(AFFECT_ACTIONS, &partial, CompletionType::AffectAction)
        }
        // oedit <vnum> affect add - show common effect types
        4 if !completing_word
            && (words[2].to_lowercase() == "affect" || words[2].to_lowercase() == "affects")
            && words[3].to_lowercase() == "add" =>
        {
            all_static(AFFECT_EFFECT_TYPES, CompletionType::EffectType)
        }
        // oedit <vnum> affect add <partial_effect> - complete effect type
        5 if completing_word
            && (words[2].to_lowercase() == "affect" || words[2].to_lowercase() == "affects")
            && words[3].to_lowercase() == "add" =>
        {
            filter_static(AFFECT_EFFECT_TYPES, &partial, CompletionType::EffectType)
        }
        // oedit <vnum> affect add damage_resistance <mag> - show damage types as tag
        6 if !completing_word
            && (words[2].to_lowercase() == "affect" || words[2].to_lowercase() == "affects")
            && words[3].to_lowercase() == "add"
            && words[4].to_lowercase() == "damage_resistance" =>
        {
            all_static(DAMAGE_TYPES, CompletionType::DamageType)
        }
        7 if completing_word
            && (words[2].to_lowercase() == "affect" || words[2].to_lowercase() == "affects")
            && words[3].to_lowercase() == "add"
            && words[4].to_lowercase() == "damage_resistance" =>
        {
            filter_static(DAMAGE_TYPES, &partial, CompletionType::DamageType)
        }
        // oedit <vnum> affect add status_resistance <mag> - show vs_effect tags
        6 if !completing_word
            && (words[2].to_lowercase() == "affect" || words[2].to_lowercase() == "affects")
            && words[3].to_lowercase() == "add"
            && words[4].to_lowercase() == "status_resistance" =>
        {
            all_static(STATUS_RESISTANCE_VS_EFFECTS, CompletionType::EffectType)
        }
        7 if completing_word
            && (words[2].to_lowercase() == "affect" || words[2].to_lowercase() == "affects")
            && words[3].to_lowercase() == "add"
            && words[4].to_lowercase() == "status_resistance" =>
        {
            filter_static(STATUS_RESISTANCE_VS_EFFECTS, &partial, CompletionType::EffectType)
        }
        // oedit <vnum> affect add custom_skill_boost <mag> - show registered skill keys
        6 if !completing_word
            && (words[2].to_lowercase() == "affect" || words[2].to_lowercase() == "affects")
            && words[3].to_lowercase() == "add"
            && words[4].to_lowercase() == "custom_skill_boost" =>
        {
            all_dynamic(custom_skill_keys, CompletionType::EffectType)
        }
        7 if completing_word
            && (words[2].to_lowercase() == "affect" || words[2].to_lowercase() == "affects")
            && words[3].to_lowercase() == "add"
            && words[4].to_lowercase() == "custom_skill_boost" =>
        {
            filter_dynamic(custom_skill_keys, &partial, CompletionType::EffectType)
        }
        _ => CompletionResult::empty(),
    }
}
