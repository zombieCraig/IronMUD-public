// scripts/commands/reclist.rhai
// reclist [cooking|crafting] - List all recipes
// For builders to view available recipes

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

fn c_warn(text, conn_id) {
    color_or(ansi_yellow(text), text, conn_id)
}

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    // Get optional skill filter
    let skill_filter = "";
    if args != "" {
        let filter = args.to_lower().trim();
        if filter == "cooking" || filter == "crafting" {
            skill_filter = filter;
        } else {
            send_client_message(connection_id, "Invalid filter. Use: reclist [cooking|crafting]");
            return;
        }
    }

    // Get recipes
    let recipes = [];
    if skill_filter != "" {
        recipes = get_recipes_by_skill(skill_filter);
    } else {
        recipes = search_recipes("");  // Get all
    }

    if recipes.len() == 0 {
        if skill_filter != "" {
            send_client_message(connection_id, "No " + skill_filter + " recipes found.");
        } else {
            send_client_message(connection_id, "No recipes found.");
        }
        return;
    }

    // Sort by skill, then by level, then by name
    let cooking_recipes = [];
    let crafting_recipes = [];
    for recipe in recipes {
        if recipe.skill == "cooking" {
            cooking_recipes.push(recipe);
        } else {
            crafting_recipes.push(recipe);
        }
    }

    // Build output
    let output = "";

    if skill_filter == "" || skill_filter == "cooking" {
        if cooking_recipes.len() > 0 {
            output += c_header("=== Cooking Recipes (" + cooking_recipes.len() + ") ===", connection_id) + "\n";
            output += format_recipe_table_header(connection_id);
            for recipe in cooking_recipes {
                output += format_recipe_row(recipe, connection_id);
            }
            output += "\n";
        }
    }

    if skill_filter == "" || skill_filter == "crafting" {
        if crafting_recipes.len() > 0 {
            output += c_header("=== Crafting Recipes (" + crafting_recipes.len() + ") ===", connection_id) + "\n";
            output += format_recipe_table_header(connection_id);
            for recipe in crafting_recipes {
                output += format_recipe_row(recipe, connection_id);
            }
        }
    }

    let total = recipes.len();
    output += "\n" + c_label("Total: ", connection_id) + total + " recipes";
    if skill_filter != "" {
        output += " (filtered by " + skill_filter + ")";
    }
    output += "\n";
    output += "Use 'recedit <vnum>' to view/edit a recipe.";

    send_client_message(connection_id, output);
}

fn format_recipe_table_header(connection_id) {
    return c_label("VNUM                      Lvl  AL  Diff  XP    Name", connection_id) + "\n";
}

fn format_recipe_row(recipe, connection_id) {
    let vnum = recipe.id;
    // Pad vnum to 24 chars
    while vnum.len() < 24 {
        vnum += " ";
    }
    if vnum.len() > 24 {
        vnum = vnum.sub_string(0, 24);
    }

    let lvl = "" + recipe.skill_required;
    while lvl.len() < 3 {
        lvl = " " + lvl;
    }

    let autolearn = "N";
    if recipe.auto_learn { autolearn = "Y"; }
    while autolearn.len() < 3 {
        autolearn = " " + autolearn;
    }

    let diff = "" + recipe.difficulty;
    while diff.len() < 4 {
        diff = " " + diff;
    }

    let xp = "" + recipe.base_xp;
    while xp.len() < 5 {
        xp = " " + xp;
    }

    let name = recipe.name;
    if name.len() > 30 {
        name = name.sub_string(0, 27) + "...";
    }

    return vnum + " " + lvl + " " + autolearn + " " + diff + " " + xp + "  " + name + "\n";
}
