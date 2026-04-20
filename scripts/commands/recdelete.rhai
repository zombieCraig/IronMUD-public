// scripts/commands/recdelete.rhai
// recdelete <vnum> [confirm] - Delete a recipe from the database
// For builders to remove recipes

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

fn c_error(text, conn_id) {
    color_or(ansi_red(text), text, conn_id)
}

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    let parts = args.split(" ");

    if parts.len() < 1 || parts[0] == "" {
        send_client_message(connection_id, "Usage: recdelete <vnum> [confirm]");
        return;
    }

    let vnum = parts[0];
    let confirmed = false;
    if parts.len() >= 2 && parts[1].to_lower() == "confirm" {
        confirmed = true;
    }

    // Check if recipe exists
    let recipe = get_recipe(vnum);
    if recipe == () {
        send_client_message(connection_id, "Recipe not found: " + vnum);
        return;
    }

    // If not confirmed, show warning and require confirmation
    if !confirmed {
        let output = c_warn("=== Delete Recipe ===", connection_id) + "\n\n";
        output += c_label("VNUM: ", connection_id) + c_value(vnum, connection_id) + "\n";
        output += c_label("Name: ", connection_id) + c_value(recipe.name, connection_id) + "\n";
        output += c_label("Skill: ", connection_id) + c_value(recipe.skill + " (level " + recipe.skill_required + ")", connection_id) + "\n";
        output += c_label("Ingredients: ", connection_id) + recipe.ingredients.len() + "\n";
        output += c_label("Tools: ", connection_id) + recipe.tools.len() + "\n";
        output += c_label("Output: ", connection_id) + recipe.output_vnum + "\n";
        output += "\n" + c_error("WARNING: This action cannot be undone!", connection_id) + "\n";
        output += c_warn("Players who know this recipe will still have it listed but won't be able to use it.", connection_id) + "\n";
        output += "\nTo confirm deletion, type: recdelete " + vnum + " confirm";
        send_client_message(connection_id, output);
        return;
    }

    // Delete the recipe
    if delete_recipe(vnum) {
        send_client_message(connection_id, c_header("Recipe '" + vnum + "' (" + recipe.name + ") deleted.", connection_id));
    } else {
        send_client_message(connection_id, c_error("Failed to delete recipe.", connection_id));
    }
}
