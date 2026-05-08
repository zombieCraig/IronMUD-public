use super::consts::*;
use super::helpers::*;
use super::types::*;

/// Context-aware completion for plantedit command
/// Syntax: plantedit <vnum> <subcommand> [args...]
/// Or: plantedit create <vnum>
/// Or: plantedit list
pub(super) fn complete_plantedit(
    words: &[&str],
    completing_word: bool,
    plant_vnums: &[String],
    item_vnums: &[String],
) -> CompletionResult {
    let partial = get_partial(words, completing_word);

    match words.len() {
        // plantedit <partial_vnum_or_cmd> - complete vnum or "create"/"list"
        2 if completing_word => {
            let static_cmds = &["create", "list"];
            let mut matches: Vec<String> = static_cmds
                .iter()
                .filter(|s| s.starts_with(&partial))
                .map(|s| s.to_string())
                .collect();
            matches.extend(
                plant_vnums
                    .iter()
                    .filter(|v| v.to_lowercase().starts_with(&partial))
                    .cloned(),
            );
            CompletionResult::new(matches, &partial, CompletionType::PlantVnum)
        }
        // plantedit - show "create", "list", and all vnums
        1 if !completing_word => {
            let mut matches = vec!["create".to_string(), "list".to_string()];
            matches.extend(plant_vnums.iter().cloned());
            CompletionResult::new(matches, "", CompletionType::PlantVnum)
        }
        // plantedit <vnum> - show all subcommands
        2 if !completing_word => {
            if words[1].to_lowercase() == "create" || words[1].to_lowercase() == "list" {
                CompletionResult::empty()
            } else {
                all_static(PLANTEDIT_SUBCOMMANDS, CompletionType::PlanteditSubcommand)
            }
        }
        // plantedit <vnum> <partial_subcmd> - complete subcommand
        3 if completing_word => filter_static(PLANTEDIT_SUBCOMMANDS, &partial, CompletionType::PlanteditSubcommand),
        // plantedit <vnum> category - show plant categories
        3 if !completing_word && words[2].to_lowercase() == "category" => {
            all_static(PLANT_CATEGORIES, CompletionType::PlantCategory)
        }
        // plantedit <vnum> category <partial> - complete category
        4 if completing_word && words[2].to_lowercase() == "category" => {
            filter_static(PLANT_CATEGORIES, &partial, CompletionType::PlantCategory)
        }
        // plantedit <vnum> season - show season actions
        3 if !completing_word && words[2].to_lowercase() == "season" => {
            all_static(PLANT_SEASON_ACTIONS, CompletionType::PlanteditSubcommand)
        }
        // plantedit <vnum> season <partial_action> - complete action
        4 if completing_word && words[2].to_lowercase() == "season" => {
            filter_static(PLANT_SEASON_ACTIONS, &partial, CompletionType::PlanteditSubcommand)
        }
        // plantedit <vnum> season add/remove - show seasons
        4 if !completing_word && words[2].to_lowercase() == "season" => {
            all_static(PLANT_SEASONS, CompletionType::PlantSeason)
        }
        // plantedit <vnum> season add/remove <partial_season> - complete season
        5 if completing_word && words[2].to_lowercase() == "season" => {
            filter_static(PLANT_SEASONS, &partial, CompletionType::PlantSeason)
        }
        // plantedit <vnum> stage - show add/remove
        3 if !completing_word && words[2].to_lowercase() == "stage" => {
            all_static(PLANT_SEASON_ACTIONS, CompletionType::PlanteditSubcommand)
        }
        // plantedit <vnum> stage <partial_action>
        4 if completing_word && words[2].to_lowercase() == "stage" => {
            filter_static(PLANT_SEASON_ACTIONS, &partial, CompletionType::PlanteditSubcommand)
        }
        // plantedit <vnum> stage add - show stage names
        4 if !completing_word && words[2].to_lowercase() == "stage" && words[3].to_lowercase() == "add" => {
            all_static(PLANT_STAGES, CompletionType::PlantStage)
        }
        // plantedit <vnum> stage add <partial_stage> - complete stage name
        5 if completing_word && words[2].to_lowercase() == "stage" && words[3].to_lowercase() == "add" => {
            filter_static(PLANT_STAGES, &partial, CompletionType::PlantStage)
        }
        // plantedit <vnum> keyword - show add/remove
        3 if !completing_word && words[2].to_lowercase() == "keyword" => {
            all_static(PLANT_SEASON_ACTIONS, CompletionType::PlanteditSubcommand)
        }
        // plantedit <vnum> keyword <partial_action>
        4 if completing_word && words[2].to_lowercase() == "keyword" => {
            filter_static(PLANT_SEASON_ACTIONS, &partial, CompletionType::PlanteditSubcommand)
        }
        // plantedit <vnum> seed_vnum - show item vnums
        3 if !completing_word && words[2].to_lowercase() == "seed_vnum" => {
            all_dynamic(item_vnums, CompletionType::ItemVnum)
        }
        // plantedit <vnum> seed_vnum <partial> - complete item vnum
        4 if completing_word && words[2].to_lowercase() == "seed_vnum" => {
            filter_dynamic(item_vnums, &partial, CompletionType::ItemVnum)
        }
        // plantedit <vnum> harvest_vnum - show item vnums
        3 if !completing_word && words[2].to_lowercase() == "harvest_vnum" => {
            all_dynamic(item_vnums, CompletionType::ItemVnum)
        }
        // plantedit <vnum> harvest_vnum <partial> - complete item vnum
        4 if completing_word && words[2].to_lowercase() == "harvest_vnum" => {
            filter_dynamic(item_vnums, &partial, CompletionType::ItemVnum)
        }
        _ => CompletionResult::empty(),
    }
}
