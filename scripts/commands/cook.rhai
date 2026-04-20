// scripts/commands/cook.rhai
// cook <recipe_name> - Prepare food using the cooking skill
// Requires: Known recipe, required skill level, ingredients, tools

fn get_level_up_message(level) {
    if level == 1 { return "You learn the basics of food preparation."; }
    if level == 2 { return "Your cooking technique improves."; }
    if level == 3 { return "You begin to understand flavor combinations."; }
    if level == 4 { return "Your dishes become more palatable."; }
    if level == 5 { return "You're becoming a skilled cook!"; }
    if level == 6 { return "You can prepare meals with greater finesse."; }
    if level == 7 { return "Your cooking shows true artistry."; }
    if level == 8 { return "Master chefs would admire your skill."; }
    if level == 9 { return "You're approaching mastery of the culinary arts."; }
    if level == 10 { return "You have achieved MASTERY in Cooking! Your dishes are legendary."; }
    return "";
}

fn get_quality_message(tier, item_name) {
    if tier == 0 { return "You prepare a bland " + item_name + "."; }
    if tier == 1 { return "You cook a " + item_name + "."; }
    if tier == 2 { return "You prepare a delicious " + item_name + "!"; }
    return "You masterfully create an exquisite " + item_name + "!";
}

fn get_ingredient_name(ingredient) {
    if ingredient.vnum != () { return ingredient.vnum; }
    if ingredient.category != () { return ingredient.category; }
    return "unknown";
}

fn is_liquid_ingredient(ingredient) {
    if ingredient.category == () { return false; }
    return ingredient.category.starts_with("@liquid:");
}

fn get_total_liquid(container_ids) {
    let total = 0;
    for id in container_ids {
        total += get_item_liquid_current(id);
    }
    return total;
}

fn check_missing_ingredient(ingredient, char_name) {
    let found = find_ingredients(char_name, ingredient);
    let name = get_ingredient_name(ingredient);

    if is_liquid_ingredient(ingredient) {
        let total_liquid = get_total_liquid(found);
        if total_liquid >= ingredient.quantity { return ""; }
        let needed = ingredient.quantity - total_liquid;
        return needed + " units " + name;
    }

    if found.len() >= ingredient.quantity { return ""; }
    let needed = ingredient.quantity - found.len();
    return needed + "x " + name;
}

fn get_missing_ingredients_message(recipe, char_name) {
    let missing = [];
    for ingredient in recipe.ingredients {
        let msg = check_missing_ingredient(ingredient, char_name);
        if msg != "" {
            missing.push(msg);
        }
    }
    if missing.len() > 0 {
        return "Missing: " + missing.join(", ");
    }
    return "";
}

fn check_ingredient_available(ingredient, found) {
    if is_liquid_ingredient(ingredient) {
        let total_liquid = get_total_liquid(found);
        return total_liquid >= ingredient.quantity;
    }
    return found.len() >= ingredient.quantity;
}

fn collect_ingredient_data(ingredient, found) {
    let result = #{
        is_liquid: false,
        item_ids: [],
        quality_sum: 0,
        count: 0
    };

    if is_liquid_ingredient(ingredient) {
        result.is_liquid = true;
        for container_id in found {
            result.quality_sum += get_item_quality(container_id);
            result.count += 1;
        }
        return result;
    }

    for i in 0..ingredient.quantity {
        let item_id = found[i];
        result.item_ids.push(item_id);
        result.quality_sum += get_item_quality(item_id);
        result.count += 1;
    }
    return result;
}

// Returns [is_liquid, data] where data is either item_ids array or [found, quantity] for liquids
fn get_items_to_consume(result, found, quantity) {
    if result.is_liquid {
        return [true, [found, quantity]];
    }
    return [false, result.item_ids];
}

fn get_tool_name(tool) {
    if tool.vnum != () { return tool.vnum; }
    if tool.category != () { return tool.category; }
    return "unknown";
}

fn get_location_hint(tool) {
    if tool.location == "Room" { return " (in room)"; }
    if tool.location == "Inventory" { return " (in inventory)"; }
    return "";
}

fn get_missing_tools_message(recipe, char_name, room_id) {
    let missing = [];
    for tool in recipe.tools {
        let found = find_tool(char_name, room_id, tool);
        if found == () {
            let name = get_tool_name(tool);
            let hint = get_location_hint(tool);
            missing.push(name + hint);
        }
    }
    if missing.len() > 0 {
        return "Missing tools: " + missing.join(", ");
    }
    return "";
}

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    // Check position
    if char.position == "sleeping" {
        send_client_message(connection_id, "You are asleep! Use 'wake' to wake up first.");
        return;
    }
    if char.position == "sitting" {
        send_client_message(connection_id, "You are sitting down. Use 'stand' to get up first.");
        return;
    }

    // Parse recipe name
    let recipe_name = args;
    if recipe_name == "" {
        send_client_message(connection_id, "Usage: cook <recipe_name>");
        send_client_message(connection_id, "Use 'recipes cooking' to see available recipes.");
        return;
    }

    // Find recipe by ID or partial name match
    let recipe = get_recipe(recipe_name);
    if recipe == () {
        // Try partial name match
        let all_recipes = get_recipes_by_skill("cooking");
        for r in all_recipes {
            let r_name_lower = r.name.to_lower();
            let r_id_lower = r.id.to_lower();
            let search_lower = recipe_name.to_lower();
            if r_name_lower.contains(search_lower) || r_id_lower.contains(search_lower) {
                recipe = r;
            }
        }
    }

    if recipe == () {
        send_client_message(connection_id, "Unknown recipe: " + recipe_name);
        send_client_message(connection_id, "Use 'recipes cooking' to see available recipes.");
        return;
    }

    // Verify this is a cooking recipe
    if recipe.skill != "cooking" {
        send_client_message(connection_id, "That is not a cooking recipe. Try 'craft " + recipe_name + "' instead.");
        return;
    }

    // Check if character knows the recipe
    if !knows_recipe(char.name, recipe.id) {
        let skill_level = get_skill_level(char.name, "cooking");
        if recipe.auto_learn {
            send_client_message(connection_id, "You need cooking level " + recipe.skill_required + " to learn this recipe. (You have " + skill_level + ")");
        } else {
            send_client_message(connection_id, "You haven't learned this recipe yet. Find a recipe book or trainer.");
        }
        return;
    }

    // Get skill level
    let skill_level = get_skill_level(char.name, "cooking");

    // Check minimum skill requirement
    if skill_level < recipe.skill_required {
        send_client_message(connection_id, "You need cooking level " + recipe.skill_required + " to cook this. (You have " + skill_level + ")");
        return;
    }

    // First pass: validate all ingredients are available
    for ingredient in recipe.ingredients {
        let found = find_ingredients(char.name, ingredient);
        let has_enough = check_ingredient_available(ingredient, found);
        if !has_enough {
            let missing_msg = get_missing_ingredients_message(recipe, char.name);
            send_client_message(connection_id, "You don't have the required ingredients.");
            send_client_message(connection_id, missing_msg);
            return;
        }
    }

    // Second pass: collect items and quality for consumption
    let all_ingredient_ids = [];
    let liquid_ingredients = [];
    let total_ingredient_quality = 0;
    let ingredient_count = 0;

    for ingredient in recipe.ingredients {
        let found = find_ingredients(char.name, ingredient);
        let result = collect_ingredient_data(ingredient, found);
        total_ingredient_quality += result.quality_sum;
        ingredient_count += result.count;
        // Get items to consume (returns [is_liquid, data])
        let consume_info = get_items_to_consume(result, found, ingredient.quantity);
        if consume_info[0] {
            liquid_ingredients.push(consume_info[1]);
        } else {
            all_ingredient_ids += consume_info[1];
        }
    }

    // Check tools
    let tool_ids = [];
    let total_tool_quality = 0;
    let tool_count = 0;

    for tool in recipe.tools {
        let found = find_tool(char.name, char.current_room_id, tool);
        if found == () {
            let missing_msg = get_missing_tools_message(recipe, char.name, char.current_room_id);
            send_client_message(connection_id, "You don't have the required tools.");
            send_client_message(connection_id, missing_msg);
            return;
        }
        tool_ids.push(found);
        let quality = get_item_quality(found);
        total_tool_quality += quality;
        tool_count += 1;
    }

    // Calculate average qualities
    let avg_ingredient_quality = 50; // Default if no ingredients
    if ingredient_count > 0 {
        avg_ingredient_quality = total_ingredient_quality / ingredient_count;
    }

    let avg_tool_quality = 50; // Default if no tools
    if tool_count > 0 {
        avg_tool_quality = total_tool_quality / tool_count;
    }

    // Calculate quality tier
    let quality_tier = calculate_craft_quality(skill_level, avg_tool_quality, avg_ingredient_quality, recipe.difficulty);

    // Consume regular ingredients (items are deleted)
    if all_ingredient_ids.len() > 0 {
        if !consume_items(all_ingredient_ids) {
            send_client_message(connection_id, "Something went wrong consuming the ingredients. Please try again.");
            return;
        }
    }

    // Consume liquid ingredients (containers remain, liquid is reduced)
    for liquid_req in liquid_ingredients {
        let container_ids = liquid_req[0];
        let amount_needed = liquid_req[1];
        if !consume_liquid_ingredient(container_ids, amount_needed) {
            send_client_message(connection_id, "Something went wrong consuming liquid ingredients. Please try again.");
            return;
        }
    }

    // Spawn the cooked item(s)
    let items_created = [];
    for i in 0..recipe.output_quantity {
        let item = spawn_crafted_item(recipe.output_vnum, char.name, quality_tier);
        if item != () {
            items_created.push(item);
        }
    }

    if items_created.len() == 0 {
        send_client_message(connection_id, "Failed to create the food. The prototype '" + recipe.output_vnum + "' may not exist.");
        return;
    }

    // Get quality tier name for messages
    let quality_name = get_quality_tier_name(quality_tier);

    // Build success message
    let item_name = items_created[0].name;
    if items_created.len() > 1 {
        item_name = items_created.len() + "x " + item_name;
    }

    let success_msg = get_quality_message(quality_tier, item_name);
    send_client_message(connection_id, success_msg);

    // Broadcast to room
    broadcast_to_room(char.current_room_id, char.name + " finishes cooking something.", char.name);

    // Award XP based on output quality
    // Quality 50 = base XP, quality 100 = 2x base XP, quality 0 = 0 XP
    let item_quality = items_created[0].quality;
    let xp_gained = (recipe.base_xp * item_quality) / 50;
    if xp_gained < 1 && recipe.base_xp > 0 {
        xp_gained = 1; // Minimum 1 XP if recipe has any base XP
    }

    let leveled_up = add_skill_experience(char.name, "cooking", xp_gained);

    // Refresh session with updated character data (including new skills)
    let updated_char = get_character_data(char.name);
    if updated_char != () {
        set_player_character(connection_id, updated_char);
    }

    send_client_message(connection_id, "You gain " + xp_gained + " cooking experience.");

    if leveled_up {
        let new_level = get_skill_level(char.name, "cooking");
        let level_msg = get_level_up_message(new_level);
        send_client_message(connection_id, "\n*** LEVEL UP! ***");
        send_client_message(connection_id, "Your cooking skill is now level " + new_level + "!");
        if level_msg != "" {
            send_client_message(connection_id, level_msg);
        }
    }
}
