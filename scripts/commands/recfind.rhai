// scripts/commands/recfind.rhai
// recfind <keyword> - Search recipes by name, vnum, or ingredient
// For builders to find specific recipes

// OLC Color helper functions
fn c_header(text, conn_id) {
    color_or(ansi_green(text), text, conn_id)
}

fn c_label(text, conn_id) {
    color_or(ansi_cyan(text), text, conn_id)
}

fn c_value(text, conn_id) {
    color_or(ansi_white(text), text, conn_id)
}

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    if args == "" {
        send_client_message(connection_id, "Usage: recfind <keyword>");
        send_client_message(connection_id, "Searches recipe names, vnums, and ingredient vnums/categories.");
        return;
    }

    let keyword = args.to_lower().trim();

    // Search recipes
    let recipes = search_recipes(keyword);

    if recipes.len() == 0 {
        send_client_message(connection_id, "No recipes found matching: " + keyword);
        return;
    }

    let output = c_header("=== Recipes matching '" + keyword + "' ===", connection_id) + "\n\n";

    for recipe in recipes {
        output += format_recipe_summary(recipe, connection_id);
    }

    output += c_label("Found: ", connection_id) + recipes.len() + " recipe(s)\n";
    output += "Use 'recedit <vnum>' to view/edit a recipe.";

    send_client_message(connection_id, output);
}

fn format_recipe_summary(recipe, connection_id) {
    let output = c_label(recipe.id, connection_id) + " - " + c_value(recipe.name, connection_id) + "\n";
    output += "  Skill: " + recipe.skill + " (level " + recipe.skill_required + ")";
    if recipe.auto_learn {
        output += " [auto-learn]";
    }
    output += "\n";

    // Show ingredients summary
    let ing_count = recipe.ingredients.len();
    if ing_count > 0 {
        output += "  Ingredients: " + ing_count + " (";
        let ing_list = [];
        for ing in recipe.ingredients {
            let name = "";
            if ing.vnum != () && ing.vnum != "" {
                name = ing.vnum;
            } else if ing.category != () && ing.category != "" {
                name = "@" + ing.category;
            }
            ing_list.push(ing.quantity + "x " + name);
        }
        output += join_array(ing_list) + ")\n";
    }

    // Show output
    output += "  Output: " + recipe.output_vnum;
    if recipe.output_quantity > 1 {
        output += " x" + recipe.output_quantity;
    }
    let proto = get_item_by_vnum(recipe.output_vnum);
    if proto != () {
        output += " (" + proto.name + ")";
    }
    output += "\n\n";

    return output;
}

fn join_array(arr) {
    let result = "";
    for item in arr {
        if result != "" { result += ", "; }
        result += item;
    }
    return result;
}
