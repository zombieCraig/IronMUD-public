// scripts/commands/recipes.rhai
// recipes <skill> [search] - List recipes for a skill
// Usage: recipes cooking, recipes crafting, recipes crafting sword
//
// Visibility:
//   * Regular players see known recipes plus auto-learn recipes they
//     haven't reached yet (skill progression preview).
//   * Book-required (auto_learn=false) recipes the player hasn't learned
//     are hidden — they're discovered in-world.
//   * Builders/admins see everything, with a banner.

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

fn pad_right(s, width) {
    let out = s;
    while out.len() < width {
        out += " ";
    }
    return out;
}

fn sort_recipes_by_skill(recipes) {
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
    return parts.join(", ");
}

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    if args == "" {
        send_client_message(connection_id, "Usage: recipes <skill> [search]");
        send_client_message(connection_id, "Available skills: cooking, crafting");
        send_client_message(connection_id, "Example: recipes cooking");
        send_client_message(connection_id, "Example: recipes crafting sword");
        return;
    }

    let parts = args.split(" ");
    let skill = parts[0].to_lower();
    let search = "";
    if parts.len() > 1 {
        let search_parts = [];
        for i in 1..parts.len() {
            search_parts.push(parts[i]);
        }
        search = search_parts.join(" ").to_lower();
    }

    if skill != "cooking" && skill != "crafting" {
        send_client_message(connection_id, "Unknown skill: " + skill);
        send_client_message(connection_id, "Available skills: cooking, crafting");
        return;
    }

    let all_recipes = get_recipes_by_skill(skill);

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

    let can_see_all = char.is_builder || char.is_admin;

    // Hide book-required, unlearned recipes from regular players.
    let visible_recipes = [];
    for recipe in filtered_recipes {
        let knows = knows_recipe(char.name, recipe.id);
        if can_see_all || knows || recipe.auto_learn {
            visible_recipes.push(recipe);
        }
    }

    if visible_recipes.len() == 0 {
        if search != "" {
            send_client_message(connection_id, "No " + skill + " recipes matching '" + search + "' found.");
        } else if all_recipes.len() == 0 {
            send_client_message(connection_id, "No " + skill + " recipes found.");
        } else {
            send_client_message(connection_id, "You haven't learned any " + skill + " recipes yet.");
        }
        return;
    }

    let skill_level = get_skill_level(char.name, skill);
    visible_recipes = sort_recipes_by_skill(visible_recipes);

    // Compute name padding width for column alignment.
    let max_name_len = 0;
    for recipe in visible_recipes {
        if recipe.name.len() > max_name_len {
            max_name_len = recipe.name.len();
        }
    }

    let skill_display = skill.sub_string(0, 1).to_upper() + skill.sub_string(1);
    let output = "=== " + skill_display + " Recipes (your skill: " + skill_level + ") ===\n";
    if can_see_all {
        output += "(builder view: showing all recipes, including unlearned book recipes)\n";
    }
    output += "\n";

    let known_count = 0;
    let to_learn_count = 0;
    let book_count = 0;

    for recipe in visible_recipes {
        let knows = knows_recipe(char.name, recipe.id);
        let has_skill = skill_level >= recipe.skill_required;
        let status = get_recipe_status(knows, recipe.auto_learn, has_skill);

        if status == "[*]" {
            known_count += 1;
        } else if status == "[L]" {
            book_count += 1;
        } else {
            to_learn_count += 1;
        }

        let name_col = pad_right(recipe.name, max_name_len);
        let line = status + " " + name_col + "  (" + recipe.skill_required + ")  " + format_ingredients(recipe);

        let tools_str = format_tools(recipe);
        if tools_str != "" {
            line += "  [tools: " + tools_str + "]";
        }

        if knows && !has_skill {
            line += "  [need skill " + recipe.skill_required + "]";
        }

        output += line + "\n";
    }

    output += "\n";
    let summary = "" + known_count + " known, " + to_learn_count + " to learn";
    if can_see_all && book_count > 0 {
        summary += ", " + book_count + " book-only";
    }
    summary += ".  [*] known  [ ] not yet learned";
    if can_see_all {
        summary += "  [L] book-required";
    }
    output += summary;

    send_client_message(connection_id, output);
}
