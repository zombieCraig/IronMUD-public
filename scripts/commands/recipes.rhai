// scripts/commands/recipes.rhai
// recipes <skill> [search] - List known recipes for a skill
// Usage: recipes cooking, recipes crafting, recipes cooking bread

fn get_ingredient_name(ingredient) {
    if ingredient.vnum != () { return ingredient.vnum; }
    if ingredient.category != () { return ingredient.category; }
    return "item";
}

fn get_tool_name(tool) {
    if tool.vnum != () { return tool.vnum; }
    if tool.category != () { return tool.category; }
    return "tool";
}

fn get_recipe_status(knows, auto_learn, has_skill) {
    if knows { return "[*]"; }
    if auto_learn && has_skill { return "[*]"; }
    if !auto_learn { return "[L]"; }
    return "[ ]";
}

fn should_swap(recipe_a, recipe_b) {
    recipe_a.skill_required > recipe_b.skill_required
}

fn sort_recipes_by_skill(recipes) {
    // Simple selection sort by skill_required
    let n = recipes.len();
    if n <= 1 { return recipes; }

    let sorted = [];
    let remaining = recipes;

    while remaining.len() > 0 {
        let min_idx = 0;
        let min_skill = remaining[0].skill_required;

        let i = 1;
        while i < remaining.len() {
            if remaining[i].skill_required < min_skill {
                min_idx = i;
                min_skill = remaining[i].skill_required;
            }
            i += 1;
        }

        sorted.push(remaining[min_idx]);
        remaining.remove(min_idx);
    }

    return sorted;
}

fn format_ingredients(recipe) {
    let parts = [];
    for ingredient in recipe.ingredients {
        let name = get_ingredient_name(ingredient);
        let part = "" + ingredient.quantity + "x " + name;
        parts.push(part);
    }
    return parts.join(", ");
}

fn format_tools(recipe) {
    let parts = [];
    for tool in recipe.tools {
        let name = get_tool_name(tool);
        parts.push(name);
    }
    if parts.len() == 0 {
        return "none";
    }
    return parts.join(", ");
}

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    // Parse arguments
    if args == "" {
        send_client_message(connection_id, "Usage: recipes <skill> [search]");
        send_client_message(connection_id, "Available skills: cooking, crafting");
        send_client_message(connection_id, "Example: recipes cooking");
        send_client_message(connection_id, "Example: recipes crafting sword");
        return;
    }

    // Split into skill and optional search term
    let parts = args.split(" ");
    let skill = parts[0].to_lower();
    let search = "";
    if parts.len() > 1 {
        // Join remaining parts as search term
        let search_parts = [];
        for i in 1..parts.len() {
            search_parts.push(parts[i]);
        }
        search = search_parts.join(" ").to_lower();
    }

    // Validate skill
    if skill != "cooking" && skill != "crafting" {
        send_client_message(connection_id, "Unknown skill: " + skill);
        send_client_message(connection_id, "Available skills: cooking, crafting");
        return;
    }

    // Get all recipes for this skill
    let all_recipes = get_recipes_by_skill(skill);

    // Filter by search term if provided
    let filtered_recipes = [];
    if search == "" {
        filtered_recipes = all_recipes;
    } else {
        for recipe in all_recipes {
            let name_lower = recipe.name.to_lower();
            let id_lower = recipe.id.to_lower();
            if name_lower.contains(search) || id_lower.contains(search) {
                filtered_recipes.push(recipe);
            }
        }
    }

    if filtered_recipes.len() == 0 {
        if search == "" {
            send_client_message(connection_id, "No " + skill + " recipes found.");
        } else {
            send_client_message(connection_id, "No " + skill + " recipes matching '" + search + "' found.");
        }
        return;
    }

    // Get current skill level
    let skill_level = get_skill_level(char.name, skill);

    // Build output
    let skill_display = skill.sub_string(0, 1).to_upper() + skill.sub_string(1);
    let output = "=== " + skill_display + " Recipes ===\n\n";

    // Sort recipes by skill_required
    filtered_recipes = sort_recipes_by_skill(filtered_recipes);

    // Display each recipe
    for recipe in filtered_recipes {
        let knows = knows_recipe(char.name, recipe.id);
        let has_skill = skill_level >= recipe.skill_required;

        // Status indicator
        let status = get_recipe_status(knows, recipe.auto_learn, has_skill);

        // Name and skill requirement
        output += status + " " + recipe.name + " (skill " + recipe.skill_required + ")";

        // Show if character can't make it yet
        if !has_skill && knows {
            output += " [need skill " + recipe.skill_required + "]";
        }

        output += "\n";

        // Show details
        let ingredients_str = format_ingredients(recipe);
        let tools_str = format_tools(recipe);

        output += "    Ingredients: " + ingredients_str + "\n";
        if tools_str != "none" {
            output += "    Tools: " + tools_str + "\n";
        }
        output += "\n";
    }

    // Legend
    output += "---\n";
    output += "[*] = known recipe  [ ] = not yet learned  [L] = requires recipe book\n";

    send_client_message(connection_id, output);
}
