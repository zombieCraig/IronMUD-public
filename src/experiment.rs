//! Recipe discovery by experiment.
//!
//! Every other way to learn a recipe is somebody handing it to you: a skill
//! level crosses a threshold, a book is read, a quest pays out. None of them
//! is *learning* in the sense that makes a game fun — the player is told,
//! not taught. This module is the one path where the world does not
//! volunteer the answer: put materials together and find out.
//!
//! The matching problem is deliberately strict. A set of items discovers a
//! recipe only when it fills that recipe's ingredient list **exactly** —
//! every ingredient satisfied and no item left over. Anything looser and the
//! optimal play is to carry one of everything and mash the button, which is
//! not a puzzle, it is a slot machine.
//!
//! A near miss — items that all fit somewhere but do not finish the list —
//! is reported separately so a failure can say *something*. Losing the
//! materials teaches nothing on its own; losing them and learning you were
//! one ingredient short is the part worth paying for.

use crate::types::{Recipe, RecipeIngredient};
use std::collections::{HashMap, HashSet};

/// Widest problem we will try to solve. Backtracking over ingredient slots
/// is exponential in the worst case, and a recipe with more than this many
/// slots is not a thing a player discovers by guessing anyway.
pub const MAX_SLOTS: usize = 12;

/// What a held item looks like to the matcher — the two facts an ingredient
/// can name, and the id so the caller can consume it afterwards.
#[derive(Clone, Debug)]
pub struct ItemFacts {
    pub id: String,
    pub vnum: Option<String>,
    pub categories: Vec<String>,
}

/// The result of holding a set of materials up against every recipe the
/// player has not yet learned.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Match {
    /// Every ingredient satisfied and every item consumed.
    Exact {
        recipe_id: String,
    },
    /// The items all fit, but the recipe wants more. `missing` is how many
    /// ingredient slots went unfilled — enough to say "close" without
    /// naming the answer.
    Partial {
        recipe_id: String,
        missing: i32,
    },
    None,
}

/// Liquid ingredients are addressed as containers with a level, not as items
/// you can name and hand over, so they are outside what `experiment` can
/// express. A recipe that needs one is simply not discoverable this way.
pub fn is_liquid(ing: &RecipeIngredient) -> bool {
    ing.category
        .as_deref()
        .map(|c| c.starts_with("@liquid:"))
        .unwrap_or(false)
}

/// Does this item satisfy this ingredient? Same rule `find_ingredients`
/// uses for the ordinary craft path — exact vnum, or a category the item
/// carries, compared case-insensitively.
pub fn ingredient_matches(ing: &RecipeIngredient, item: &ItemFacts) -> bool {
    if let Some(vnum) = ing.vnum.as_deref() {
        return item.vnum.as_deref() == Some(vnum);
    }
    if let Some(category) = ing.category.as_deref() {
        return item.categories.iter().any(|c| c.eq_ignore_ascii_case(category));
    }
    false
}

/// Every recipe the player already knows, by either route.
///
/// `learned_recipes` is only half the answer: an `auto_learn` recipe the
/// player has the skill for is theirs without ever being written down, and
/// the matcher has to treat it as known or `experiment` will happily charge
/// them materials to discover something already in their hands. This is the
/// same rule `knows_recipe` applies for the ordinary craft path.
pub fn effective_known(
    recipes: &HashMap<String, Recipe>,
    learned: &HashSet<String>,
    skills: &HashMap<String, crate::types::SkillProgress>,
) -> HashSet<String> {
    let mut known = learned.clone();
    for (id, recipe) in recipes {
        if !recipe.auto_learn {
            continue;
        }
        let level = skills.get(&recipe.skill.to_lowercase()).map(|s| s.level).unwrap_or(0);
        if level >= recipe.skill_required {
            known.insert(id.clone());
        }
    }
    known
}

/// Whether a recipe is a candidate for discovery at all: unknown to the
/// player, has ingredients to match against, and needs nothing the command
/// cannot name.
pub fn is_discoverable(recipe: &Recipe, known: &HashSet<String>) -> bool {
    !known.contains(&recipe.id)
        && !recipe.ingredients.is_empty()
        && !recipe.ingredients.iter().any(is_liquid)
        && slot_count(recipe) <= MAX_SLOTS
}

fn slot_count(recipe: &Recipe) -> usize {
    recipe.ingredients.iter().map(|i| i.quantity.max(1) as usize).sum()
}

/// Expand `2x flour` into two separate slots, because matching is per item.
fn slots(recipe: &Recipe) -> Vec<&RecipeIngredient> {
    let mut out = Vec::new();
    for ing in &recipe.ingredients {
        for _ in 0..ing.quantity.max(1) {
            out.push(ing);
        }
    }
    out
}

/// Largest number of items that can be assigned to distinct ingredient
/// slots. `items.len()` means every item found a home.
///
/// Backtracking rather than a greedy pass: an item that matches two slots
/// can block a second item that matches only one, and a greedy walk gets
/// that wrong.
///
/// Memoised on `(item index, set of slots already used)`. Without the memo
/// this is exponential and only prunes when *everything* places — the bad
/// case is an assignment that fails partway, where a dozen items each
/// matching the same handful of slots costs hundreds of thousands of nodes,
/// once per candidate recipe. With it the work is bounded by
/// `items.len() << MAX_SLOTS` states, and [`MAX_SLOTS`] being 12 is what lets
/// the used-set be a `u16` bitmask and the memo a plain `HashMap`. This runs
/// on a player's command, so the worst case is the one that matters.
fn max_assignable(slots: &[&RecipeIngredient], items: &[ItemFacts]) -> usize {
    fn walk(
        slots: &[&RecipeIngredient],
        items: &[ItemFacts],
        idx: usize,
        used: u16,
        memo: &mut HashMap<(usize, u16), usize>,
    ) -> usize {
        if idx >= items.len() {
            return 0;
        }
        if let Some(hit) = memo.get(&(idx, used)) {
            return *hit;
        }
        // Best if this item is placed somewhere...
        let mut best = 0;
        for (s, slot) in slots.iter().enumerate() {
            let bit = 1u16 << s;
            if used & bit != 0 || !ingredient_matches(slot, &items[idx]) {
                continue;
            }
            let got = 1 + walk(slots, items, idx + 1, used | bit, memo);
            if got > best {
                best = got;
            }
            if best == items.len() - idx {
                break; // Cannot do better than placing everything left.
            }
        }
        // ...or if it is left out, which matters only for the partial case.
        let without = walk(slots, items, idx + 1, used, memo);
        let answer = best.max(without);
        memo.insert((idx, used), answer);
        answer
    }

    // The bitmask is why this holds. `slots(recipe)` is only called for
    // recipes `is_discoverable` has already capped at MAX_SLOTS, but assert
    // the invariant here too rather than silently mis-masking if that changes.
    debug_assert!(slots.len() <= MAX_SLOTS, "slot count must fit the u16 mask");
    if slots.len() > MAX_SLOTS {
        return 0;
    }
    let mut memo: HashMap<(usize, u16), usize> = HashMap::new();
    walk(slots, items, 0, 0, &mut memo)
}

/// Hold a set of materials up against every recipe the player has not
/// learned, and report the best thing that could come of it.
///
/// Ties are broken deterministically, and in the direction that makes the
/// command feel right: among exact matches the *hardest* recipe wins, since
/// materials that make two things should reward the more ambitious reading;
/// among near misses the *closest* one does, because that is the most
/// useful hint.
pub fn find_match(recipes: &HashMap<String, Recipe>, known: &HashSet<String>, items: &[ItemFacts]) -> Match {
    if items.is_empty() || items.len() > MAX_SLOTS {
        return Match::None;
    }

    let mut best_exact: Option<&Recipe> = None;
    let mut best_partial: Option<(&Recipe, i32)> = None;

    // Sorted so ties resolve the same way on every run — a HashMap's order
    // is not stable and a discovery that depends on it is a bug report
    // nobody can reproduce.
    let mut ids: Vec<&String> = recipes.keys().collect();
    ids.sort();

    for id in ids {
        let recipe = &recipes[id];
        if !is_discoverable(recipe, known) {
            continue;
        }
        let slots = slots(recipe);
        if items.len() > slots.len() {
            continue; // More materials than the recipe has room for.
        }
        if max_assignable(&slots, items) != items.len() {
            continue; // Something the player named has no place here.
        }
        let missing = (slots.len() - items.len()) as i32;
        if missing == 0 {
            let better = match best_exact {
                None => true,
                Some(cur) => recipe.skill_required > cur.skill_required,
            };
            if better {
                best_exact = Some(recipe);
            }
        } else {
            let better = match best_partial {
                None => true,
                Some((cur, cur_missing)) => {
                    missing < cur_missing || (missing == cur_missing && recipe.skill_required < cur.skill_required)
                }
            };
            if better {
                best_partial = Some((recipe, missing));
            }
        }
    }

    if let Some(recipe) = best_exact {
        return Match::Exact {
            recipe_id: recipe.id.clone(),
        };
    }
    if let Some((recipe, missing)) = best_partial {
        return Match::Partial {
            recipe_id: recipe.id.clone(),
            missing,
        };
    }
    Match::None
}

/// Chance in 100 that an experiment with the right materials actually lands.
///
/// Not a certainty even at the required level: an experiment that always
/// works is just a crafting recipe with extra steps. Skill above the
/// requirement is what buys reliability, so a discovery stays a reward for
/// investment rather than a lottery ticket anyone can hold.
pub fn success_chance(skill_level: i32, recipe: &Recipe) -> i32 {
    let over = skill_level - recipe.skill_required;
    let chance = 50 + 10 * over - 5 * (recipe.difficulty.max(1) - 1);
    chance.clamp(15, 95)
}

/// XP for an experiment that failed with the right materials in hand. A
/// failure that teaches nothing is just a tax; this is the "you learned
/// what does not work" half, and it is deliberately a fraction of the real
/// thing so it never beats crafting as a way to grind.
pub fn consolation_xp(recipe: &Recipe) -> i32 {
    if recipe.base_xp <= 0 {
        return 0;
    }
    (recipe.base_xp / 4).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recipe(id: &str, ings: Vec<(&str, i32)>, skill_required: i32) -> Recipe {
        Recipe {
            id: id.to_string(),
            name: id.to_string(),
            skill: "crafting".to_string(),
            skill_required,
            auto_learn: false,
            ingredients: ings
                .into_iter()
                .map(|(cat, qty)| RecipeIngredient {
                    vnum: None,
                    category: Some(cat.to_string()),
                    quantity: qty,
                })
                .collect(),
            tools: Vec::new(),
            output_vnum: "out".to_string(),
            output_quantity: 1,
            base_xp: 20,
            difficulty: 1,
        }
    }

    fn item(id: &str, categories: &[&str]) -> ItemFacts {
        ItemFacts {
            id: id.to_string(),
            vnum: None,
            categories: categories.iter().map(|c| c.to_string()).collect(),
        }
    }

    fn registry(rs: Vec<Recipe>) -> HashMap<String, Recipe> {
        rs.into_iter().map(|r| (r.id.clone(), r)).collect()
    }

    #[test]
    fn an_exact_fill_discovers_the_recipe() {
        let recipes = registry(vec![recipe("bread", vec![("flour", 1), ("water_pouch", 1)], 0)]);
        let items = vec![item("a", &["flour"]), item("b", &["water_pouch"])];
        assert_eq!(
            find_match(&recipes, &HashSet::new(), &items),
            Match::Exact {
                recipe_id: "bread".into()
            }
        );
    }

    #[test]
    fn a_leftover_item_is_not_a_match() {
        // The strictness is the whole design: without it the winning move is
        // to carry one of everything and mash the button.
        let recipes = registry(vec![recipe("bread", vec![("flour", 1)], 0)]);
        let items = vec![item("a", &["flour"]), item("b", &["iron"])];
        assert_eq!(find_match(&recipes, &HashSet::new(), &items), Match::None);
    }

    #[test]
    fn a_short_set_is_a_near_miss_with_a_count() {
        let recipes = registry(vec![recipe("stew", vec![("meat", 1), ("carrot", 2)], 0)]);
        let items = vec![item("a", &["meat"]), item("b", &["carrot"])];
        assert_eq!(
            find_match(&recipes, &HashSet::new(), &items),
            Match::Partial {
                recipe_id: "stew".into(),
                missing: 1
            }
        );
    }

    #[test]
    fn quantity_expands_into_separate_slots() {
        let recipes = registry(vec![recipe("plank", vec![("log", 2)], 0)]);
        let one = vec![item("a", &["log"])];
        let two = vec![item("a", &["log"]), item("b", &["log"])];
        assert!(matches!(
            find_match(&recipes, &HashSet::new(), &one),
            Match::Partial { missing: 1, .. }
        ));
        assert!(matches!(
            find_match(&recipes, &HashSet::new(), &two),
            Match::Exact { .. }
        ));
    }

    /// The worst case has to be bounded, because this runs on a command.
    ///
    /// Twelve items that each match the same six slots is the shape that used
    /// to blow up: nothing can place more than six, so the "everything fits"
    /// short-circuit never fires, and the walk enumerated which six —
    /// hundreds of thousands of nodes, once per candidate recipe, and (until
    /// the caller was changed too) with the World lock held the whole time.
    /// Memoising on `(item index, used-slot mask)` is what makes this return
    /// rather than hang; the wall-clock assertion is loose on purpose, since
    /// the point is the difference between milliseconds and minutes.
    #[test]
    fn a_pathological_material_set_still_returns_promptly() {
        // Six slots of one category, and a seventh nobody can fill, so no
        // assignment ever completes.
        let mut recipes = Vec::new();
        for n in 0..40 {
            recipes.push(recipe(&format!("r{}", n), vec![("scrap", 6), ("unobtainium", 1)], 0));
        }
        let items: Vec<ItemFacts> = (0..12).map(|i| item(&format!("i{}", i), &["scrap"])).collect();

        let started = std::time::Instant::now();
        let got = find_match(&registry(recipes), &HashSet::new(), &items);
        let elapsed = started.elapsed();

        assert_eq!(got, Match::None, "nothing can fill the unobtainium slot");
        assert!(
            elapsed < std::time::Duration::from_secs(2),
            "the search must stay bounded; took {:?}",
            elapsed
        );
    }

    #[test]
    fn assignment_backtracks_rather_than_grabbing_greedily() {
        // The dual-category item matches both slots; the plain one matches
        // only the second. A greedy walk puts the dual item in the first
        // slot it fits and then strands the other.
        let recipes = registry(vec![recipe("alloy", vec![("tin", 1), ("copper", 1)], 0)]);
        let items = vec![item("a", &["tin", "copper"]), item("b", &["tin"])];
        assert!(matches!(
            find_match(&recipes, &HashSet::new(), &items),
            Match::Exact { .. }
        ));
    }

    #[test]
    fn a_known_recipe_is_never_rediscovered() {
        let recipes = registry(vec![recipe("bread", vec![("flour", 1)], 0)]);
        let known: HashSet<String> = ["bread".to_string()].into_iter().collect();
        assert_eq!(find_match(&recipes, &known, &[item("a", &["flour"])]), Match::None);
    }

    #[test]
    fn an_auto_learn_recipe_at_skill_counts_as_known() {
        // Otherwise `experiment` charges the player materials to discover
        // something the skill ladder already gave them.
        let mut r = recipe("bread", vec![("flour", 1)], 2);
        r.auto_learn = true;
        let recipes = registry(vec![r]);
        let skills = |level: i32| -> HashMap<String, crate::types::SkillProgress> {
            [(
                "crafting".to_string(),
                crate::types::SkillProgress { level, experience: 0 },
            )]
            .into_iter()
            .collect()
        };

        let known = effective_known(&recipes, &HashSet::new(), &skills(2));
        assert!(known.contains("bread"));
        assert_eq!(find_match(&recipes, &known, &[item("a", &["flour"])]), Match::None);

        // Below the threshold it is still out there to be found.
        let known = effective_known(&recipes, &HashSet::new(), &skills(1));
        assert!(!known.contains("bread"));
        assert!(matches!(
            find_match(&recipes, &known, &[item("a", &["flour"])]),
            Match::Exact { .. }
        ));
    }

    #[test]
    fn a_taught_recipe_stays_known_regardless_of_skill() {
        let recipes = registry(vec![recipe("bread", vec![("flour", 1)], 9)]);
        let learned: HashSet<String> = ["bread".to_string()].into_iter().collect();
        let known = effective_known(&recipes, &learned, &HashMap::new());
        assert!(known.contains("bread"));
    }

    #[test]
    fn liquid_recipes_are_out_of_reach_of_experiment() {
        let mut r = recipe("soup", vec![("meat", 1)], 0);
        r.ingredients.push(RecipeIngredient {
            vnum: None,
            category: Some("@liquid:water".to_string()),
            quantity: 1,
        });
        assert!(!is_discoverable(&r, &HashSet::new()));
    }

    #[test]
    fn exact_matches_resolve_to_the_harder_recipe() {
        let recipes = registry(vec![
            recipe("crude_blade", vec![("iron", 1)], 1),
            recipe("fine_blade", vec![("iron", 1)], 5),
        ]);
        assert_eq!(
            find_match(&recipes, &HashSet::new(), &[item("a", &["iron"])]),
            Match::Exact {
                recipe_id: "fine_blade".into()
            }
        );
    }

    #[test]
    fn near_misses_resolve_to_the_closest_recipe() {
        let recipes = registry(vec![
            recipe("far", vec![("iron", 4)], 1),
            recipe("near", vec![("iron", 2)], 1),
        ]);
        assert_eq!(
            find_match(&recipes, &HashSet::new(), &[item("a", &["iron"])]),
            Match::Partial {
                recipe_id: "near".into(),
                missing: 1
            }
        );
    }

    #[test]
    fn vnum_ingredients_match_exactly() {
        let mut r = recipe("keyed", vec![], 0);
        r.ingredients.push(RecipeIngredient {
            vnum: Some("iron_ingot".into()),
            category: None,
            quantity: 1,
        });
        let recipes = registry(vec![r]);
        let right = ItemFacts {
            id: "a".into(),
            vnum: Some("iron_ingot".into()),
            categories: vec![],
        };
        let wrong = ItemFacts {
            id: "b".into(),
            vnum: Some("tin_ingot".into()),
            categories: vec!["iron_ingot".into()],
        };
        assert!(matches!(
            find_match(&recipes, &HashSet::new(), &[right]),
            Match::Exact { .. }
        ));
        assert_eq!(find_match(&recipes, &HashSet::new(), &[wrong]), Match::None);
    }

    #[test]
    fn categories_compare_case_insensitively() {
        let recipes = registry(vec![recipe("bread", vec![("Flour", 1)], 0)]);
        assert!(matches!(
            find_match(&recipes, &HashSet::new(), &[item("a", &["flour"])]),
            Match::Exact { .. }
        ));
    }

    #[test]
    fn no_materials_matches_nothing() {
        let recipes = registry(vec![recipe("bread", vec![("flour", 1)], 0)]);
        assert_eq!(find_match(&recipes, &HashSet::new(), &[]), Match::None);
    }

    #[test]
    fn success_chance_rewards_skill_over_the_requirement() {
        let mut r = recipe("thing", vec![("iron", 1)], 3);
        r.difficulty = 1;
        assert_eq!(success_chance(3, &r), 50);
        assert_eq!(success_chance(5, &r), 70);
        // Never a certainty, and never hopeless.
        assert_eq!(success_chance(10, &r), 95);
        assert_eq!(success_chance(0, &r), 20);
        r.difficulty = 10;
        assert_eq!(success_chance(0, &r), 15);
    }

    #[test]
    fn consolation_xp_is_a_quarter_and_never_rounds_to_nothing() {
        let mut r = recipe("thing", vec![("iron", 1)], 0);
        r.base_xp = 40;
        assert_eq!(consolation_xp(&r), 10);
        r.base_xp = 2;
        assert_eq!(consolation_xp(&r), 1);
        r.base_xp = 0;
        assert_eq!(consolation_xp(&r), 0);
    }
}
