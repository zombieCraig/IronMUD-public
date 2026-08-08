// scripts/commands/experiment.rhai
//
//   experiment <item>, <item>, ...   - put materials together and find out
//
// Every other way to learn a recipe is somebody handing it to you: a skill
// level crosses a threshold, a book is read, a quest pays out. This is the
// one path where the world does not volunteer the answer.
//
// The named items are consumed whatever happens — that is what makes a guess
// a decision rather than a free roll. What a failure buys you is knowing
// you were close, which is the only thing worth paying materials for when
// you were wrong.
//
// Matching lives in Rust (src/experiment.rs) because assigning items to
// ingredient slots needs backtracking. This script owns the flow, the roll
// and the words.

import "crafting" as craftlib;

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    if char.position == "sleeping" {
        send_client_message(connection_id, "You are asleep! Use 'wake' to wake up first.");
        return;
    }
    if char.position == "sitting" {
        send_client_message(connection_id, "You are sitting down. Use 'stand' to get up first.");
        return;
    }

    if args == "" {
        show_usage(connection_id);
        return;
    }

    let keywords = parse_materials(args);
    if keywords.len() == 0 {
        show_usage(connection_id);
        return;
    }

    let resolved = resolve_experiment_materials(char.name, keywords);
    if !resolved.ok {
        send_client_message(connection_id, "You aren't carrying " + resolved.missing + ".");
        return;
    }

    let item_ids = resolved.item_ids;
    let found = experiment_match(char.name, item_ids);

    if found.kind == "none" {
        finish_failure(char, connection_id, item_ids, "", 0);
        return;
    }

    let recipe = get_recipe(found.recipe_id);
    if recipe == () {
        send_client_message(connection_id, "Something goes wrong and the materials are wasted.");
        consume_items(item_ids);
        return;
    }

    if found.kind == "partial" {
        finish_failure(char, connection_id, item_ids, recipe.skill, found.missing);
        return;
    }

    // An exact fill. From here every exit consumes the materials, including
    // the ones that refuse: telling a player "these go together, you just
    // need a forge" for free would turn the command into an oracle for the
    // puzzle it is supposed to be.
    let tools = craftlib::gather_tools(recipe, char.name, char.current_room_id);
    if !tools.ok {
        send_client_message(connection_id, "You have nothing here to work these with.");
        send_client_message(connection_id, tools.message);
        consume_items(item_ids);
        return;
    }

    let skill_level = get_skill_level(char.name, recipe.skill);
    if skill_level < recipe.skill_required {
        send_client_message(
            connection_id,
            "\x1b[33mYou can see these belong together, but your hands aren't practised enough to prove it.\x1b[0m"
        );
        send_client_message(connection_id, "The materials are spoiled in the attempt.");
        consume_items(item_ids);
        return;
    }

    let chance = experiment_success_chance(skill_level, recipe.id);
    let material_quality = craftlib::average_item_quality(item_ids);
    let quality_tier = calculate_craft_quality(skill_level, tools.avg_quality, material_quality, recipe.difficulty);

    // Materials go whichever way the roll falls — the attempt is what costs.
    consume_items(item_ids);

    if random_int(1, 100) > chance {
        report_near_miss(connection_id, recipe.skill, 0);
        award_consolation(char, connection_id, recipe);
        return;
    }

    succeed(char, connection_id, recipe, quality_tier);
}

// "flour, water" / "flour and water" / "flour with water" all mean the same
// thing to a player, so all three separate. Commas are the documented form.
fn parse_materials(args) {
    let normalised = args;
    normalised.replace(" and ", ",");
    normalised.replace(" with ", ",");
    let out = [];
    for part in normalised.split(",") {
        let word = part;
        // Rhai has no trim; strip the spaces a comma list collects by hand.
        while word.starts_with(" ") { word = word.sub_string(1); }
        while word.ends_with(" ") { word = word.sub_string(0, word.len() - 1); }
        if word != "" { out.push(word); }
    }
    return out;
}

fn show_usage(connection_id) {
    let output = "\x1b[1;36mExperiment\x1b[0m\n";
    output += "Usage: experiment <item>, <item>, ...\n\n";
    output += "Put materials together and see what comes of them. Name every\n";
    output += "ingredient a recipe needs — no more, no less — and you may work\n";
    output += "out how it is made.\n\n";
    output += "  experiment flour, water\n";
    output += "  experiment log, log, iron nail\n\n";
    output += "The materials are used up either way. A failure will tell you\n";
    output += "whether you were close, which is what you are paying for.\n";
    output += "Recipes you already know cannot be rediscovered, and anything\n";
    output += "measured out of a container has to be made the ordinary way.";
    send_client_message(connection_id, output);
}

// A failure with nothing to say about it, or a near miss. `skill` is empty
// when nothing matched at all — the materials simply do not go together.
fn finish_failure(char, connection_id, item_ids, skill, missing) {
    consume_items(item_ids);
    report_near_miss(connection_id, skill, missing);
}

fn report_near_miss(connection_id, skill, missing) {
    if skill == "" {
        send_client_message(connection_id, "You work at the materials and get nothing out of them but waste.");
        return;
    }
    if missing > 0 {
        let short = "something";
        if missing > 1 { short = missing + " more things"; }
        send_client_message(
            connection_id,
            "\x1b[33mThey begin to come together — and then stop. You are " + short + " short.\x1b[0m"
        );
        return;
    }
    send_client_message(connection_id, "\x1b[33mYou almost have it. The materials are spent working out why not.\x1b[0m");
}

// Failing with the right materials still teaches something, so it pays a
// fraction of the recipe's XP. Deliberately small: it must never beat
// crafting as a way to grind the skill.
fn award_consolation(char, connection_id, recipe) {
    let xp = experiment_consolation_xp(recipe.id);
    if xp <= 0 { return; }
    add_skill_experience(char.name, recipe.skill, xp, "experiment");
    let updated = get_character_data(char.name);
    if updated != () {
        set_player_character(connection_id, updated);
    }
}

fn succeed(char, connection_id, recipe, quality_tier) {
    let items_created = craftlib::spawn_outputs(recipe, char.name, quality_tier);
    if items_created.len() == 0 {
        send_client_message(
            connection_id,
            "You work out how it is done — but the result falls apart in your hands. (Prototype '" + recipe.output_vnum + "' is missing.)"
        );
        return;
    }

    // Learning goes through learn_recipe so the `recipes.learned` counter
    // and the `recipe_learned` achievement hook fire here exactly as they do
    // for a book or a quest reward.
    learn_recipe(char.name, recipe.id);

    send_client_message(connection_id, format_discovery(recipe.name));
    send_client_message(connection_id, "You produce " + items_created[0].name + ".");
    broadcast_to_room(char.current_room_id, char.name + " works something out.", char.name);

    add_skill_experience(char.name, recipe.skill, recipe.base_xp, "experiment");
    notify_achievement_counter(char.name, "recipes.discovered", 1);

    let updated = get_character_data(char.name);
    if updated != () {
        set_player_character(connection_id, updated);
    }
}

// The same banner shape a skill level-up uses, because a discovery is the
// same kind of moment and should read like one.
fn format_discovery(recipe_name) {
    return "\x1b[1;33m*** You have discovered how to make " + recipe_name + "! ***\x1b[0m";
}
