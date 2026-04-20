// scripts/commands/recedit.rhai
// recedit <vnum> [subcommand] or recedit create <vnum>
// Recipe Editor for builders

// Import shared OLC helpers
import "olc_helpers" as olc;

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    let parts = args.split(" ");

    // Handle create first - recedit create <vnum>
    if parts.len() >= 1 && parts[0].to_lower() == "create" {
        if parts.len() < 2 {
            send_client_message(connection_id, "Usage: recedit create <vnum>\nExample: recedit create food:honey_bread");
            return;
        }
        let vnum = parts[1];

        // Check if recipe already exists
        if recipe_exists(vnum) {
            send_client_message(connection_id, "A recipe with vnum '" + vnum + "' already exists.");
            return;
        }

        // Create new recipe
        let recipe = new_recipe(vnum);
        if save_recipe(recipe) {
            send_client_message(connection_id, "Created recipe with vnum: " + vnum + "\n");
            // Re-fetch to ensure we have the saved version
            let recipe = get_recipe(vnum);
            show_recipe_properties(recipe, connection_id);
        } else {
            send_client_message(connection_id, "Failed to create recipe.");
        }
        return;
    }

    if args == "" {
        send_client_message(connection_id, "Usage: recedit <vnum> [subcommand] or recedit create <vnum>");
        return;
    }

    let vnum = parts[0];
    let recipe = get_recipe(vnum);

    if recipe == () {
        send_client_message(connection_id, "Recipe not found: " + vnum);
        send_client_message(connection_id, "Use 'recedit create " + vnum + "' to create it.");
        return;
    }

    let subcommand = "";
    if parts.len() >= 2 {
        subcommand = parts[1].to_lower();
    }

    // Show properties if no subcommand
    if subcommand == "" || subcommand == "show" {
        show_recipe_properties(recipe, connection_id);
        return;
    }

    // Handle subcommands
    handle_subcommand(recipe, vnum, subcommand, parts, connection_id);
}

fn handle_subcommand(recipe, vnum, subcommand, parts, connection_id) {
    if subcommand == "name" { handle_name_subcommand(vnum, parts, connection_id); return; }
    if subcommand == "vnum" { handle_vnum_subcommand(vnum, parts, connection_id); return; }
    if subcommand == "skill" { handle_skill_subcommand(vnum, parts, connection_id); return; }
    if subcommand == "level" { handle_level_subcommand(vnum, parts, connection_id); return; }
    if subcommand == "autolearn" { handle_autolearn_subcommand(vnum, parts, connection_id); return; }
    if subcommand == "difficulty" { handle_difficulty_subcommand(vnum, parts, connection_id); return; }
    if subcommand == "xp" { handle_xp_subcommand(vnum, parts, connection_id); return; }
    if subcommand == "output" { handle_output_subcommand(vnum, parts, connection_id); return; }
    if subcommand == "ingredient" { handle_ingredient_subcommand(recipe, vnum, parts, connection_id); return; }
    if subcommand == "tool" { handle_tool_subcommand(recipe, vnum, parts, connection_id); return; }

    send_client_message(connection_id, "Unknown subcommand: " + subcommand);
    send_client_message(connection_id, "Use 'recedit " + vnum + "' to see available commands.");
}

// === Property Subcommands ===

fn handle_name_subcommand(vnum, parts, connection_id) {
    if parts.len() < 3 {
        send_client_message(connection_id, "Usage: recedit " + vnum + " name <text>");
        return;
    }
    let name = olc::build_string_from_parts(parts, 2);
    if set_recipe_name(vnum, name) {
        send_client_message(connection_id, "Name set to: " + name);
    } else {
        send_client_message(connection_id, "Failed to set name.");
    }
}

fn handle_vnum_subcommand(vnum, parts, connection_id) {
    if parts.len() < 3 {
        send_client_message(connection_id, "Usage: recedit " + vnum + " vnum <new_vnum>");
        send_client_message(connection_id, "Example: recedit " + vnum + " vnum food:honey_bread");
        return;
    }
    let new_vnum = parts[2];

    // Check if new vnum already exists
    if recipe_exists(new_vnum) {
        send_client_message(connection_id, "A recipe with vnum '" + new_vnum + "' already exists.");
        return;
    }

    if rename_recipe_vnum(vnum, new_vnum) {
        send_client_message(connection_id, "Recipe vnum changed from '" + vnum + "' to '" + new_vnum + "'.");
        send_client_message(connection_id, "Use 'recedit " + new_vnum + "' to continue editing.");
    } else {
        send_client_message(connection_id, "Failed to rename recipe vnum.");
    }
}

fn handle_skill_subcommand(vnum, parts, connection_id) {
    if parts.len() < 3 {
        send_client_message(connection_id, "Usage: recedit " + vnum + " skill <cooking|crafting>");
        return;
    }
    let skill = parts[2].to_lower();
    if skill != "cooking" && skill != "crafting" {
        send_client_message(connection_id, "Invalid skill. Use: cooking, crafting");
        return;
    }
    if set_recipe_skill(vnum, skill) {
        send_client_message(connection_id, "Skill set to: " + skill);
    } else {
        send_client_message(connection_id, "Failed to set skill.");
    }
}

fn handle_level_subcommand(vnum, parts, connection_id) {
    if parts.len() < 3 {
        send_client_message(connection_id, "Usage: recedit " + vnum + " level <0-10>");
        return;
    }
    let level = parse_int(parts[2]);
    if level == () {
        send_client_message(connection_id, "Invalid number.");
        return;
    }
    if level < 0 || level > 10 {
        send_client_message(connection_id, "Level must be between 0 and 10.");
        return;
    }
    if set_recipe_level(vnum, level) {
        send_client_message(connection_id, "Skill level requirement set to: " + level);
    } else {
        send_client_message(connection_id, "Failed to set level.");
    }
}

fn handle_autolearn_subcommand(vnum, parts, connection_id) {
    let new_val = true;
    if parts.len() >= 3 {
        let setting = parts[2].to_lower();
        if setting == "off" || setting == "false" || setting == "0" || setting == "no" {
            new_val = false;
        }
    }
    if set_recipe_autolearn(vnum, new_val) {
        let status = "ON (learned automatically at skill level)";
        if !new_val { status = "OFF (requires recipe book/trainer)"; }
        send_client_message(connection_id, "Auto-learn: " + status);
    } else {
        send_client_message(connection_id, "Failed to set auto-learn.");
    }
}

fn handle_difficulty_subcommand(vnum, parts, connection_id) {
    if parts.len() < 3 {
        send_client_message(connection_id, "Usage: recedit " + vnum + " difficulty <1-10>");
        return;
    }
    let difficulty = parse_int(parts[2]);
    if difficulty == () {
        send_client_message(connection_id, "Invalid number.");
        return;
    }
    if difficulty < 1 || difficulty > 10 {
        send_client_message(connection_id, "Difficulty must be between 1 and 10.");
        return;
    }
    if set_recipe_difficulty(vnum, difficulty) {
        send_client_message(connection_id, "Difficulty set to: " + difficulty);
    } else {
        send_client_message(connection_id, "Failed to set difficulty.");
    }
}

fn handle_xp_subcommand(vnum, parts, connection_id) {
    if parts.len() < 3 {
        send_client_message(connection_id, "Usage: recedit " + vnum + " xp <amount>");
        return;
    }
    let xp = parse_int(parts[2]);
    if xp == () {
        send_client_message(connection_id, "Invalid number.");
        return;
    }
    if xp < 0 {
        send_client_message(connection_id, "XP cannot be negative.");
        return;
    }
    if set_recipe_xp(vnum, xp) {
        send_client_message(connection_id, "Base XP set to: " + xp);
    } else {
        send_client_message(connection_id, "Failed to set XP.");
    }
}

fn handle_output_subcommand(vnum, parts, connection_id) {
    if parts.len() < 3 {
        send_client_message(connection_id, "Usage: recedit " + vnum + " output <item_vnum> [quantity]");
        return;
    }
    let output_vnum = parts[2];
    let quantity = 1;
    if parts.len() >= 4 {
        let q = parse_int(parts[3]);
        if q != () && q > 0 {
            quantity = q;
        }
    }

    // Validate item prototype exists
    let proto = get_item_by_vnum(output_vnum);
    if proto == () {
        send_client_message(connection_id, olc::c_warn("Warning: Item prototype '" + output_vnum + "' not found.", connection_id));
        send_client_message(connection_id, "Setting anyway - make sure to create the item prototype.");
    }

    if set_recipe_output(vnum, output_vnum, quantity) {
        let msg = "Output set to: " + output_vnum;
        if quantity > 1 {
            msg += " x" + quantity;
        }
        if proto != () {
            msg += " (" + proto.name + ")";
        }
        send_client_message(connection_id, msg);
    } else {
        send_client_message(connection_id, "Failed to set output.");
    }
}

// === Ingredient Subcommands ===

fn handle_ingredient_subcommand(recipe, vnum, parts, connection_id) {
    if parts.len() < 3 {
        show_ingredient_help(vnum, connection_id);
        return;
    }

    let action = parts[2].to_lower();

    if action == "list" { list_ingredients(recipe, vnum, connection_id); return; }
    if action == "add" { add_ingredient(vnum, parts, connection_id); return; }
    if action == "remove" { remove_ingredient(vnum, parts, connection_id); return; }

    show_ingredient_help(vnum, connection_id);
}

fn show_ingredient_help(vnum, connection_id) {
    let output = "Ingredient Commands:\n";
    output += "  recedit " + vnum + " ingredient list\n";
    output += "  recedit " + vnum + " ingredient add <vnum|@category> <quantity>\n";
    output += "  recedit " + vnum + " ingredient remove <index>\n";
    output += "\nExamples:\n";
    output += "  ingredient add material:flour 2     - Requires 2x specific item\n";
    output += "  ingredient add @flour 2             - Requires 2x any item with category 'flour'\n";
    output += "  ingredient add @liquid:water 10     - Requires 10 units of water liquid\n";
    output += "\nLiquid ingredients use @liquid:<type> syntax where quantity is in sips/units.\n";
    output += "Liquid types: water, ale, wine, beer, milk, juice, tea, coffee, etc.\n";
    output += "\nNote: Items can have multiple categories (set via oedit category add).\n";
    output += "A recipe matches if ANY of the item's categories matches.\n";
    output += "The @ prefix is recipe syntax only - in oedit, just use the category name.\n";
    send_client_message(connection_id, output);
}

fn list_ingredients(recipe, vnum, connection_id) {
    let ingredients = recipe.ingredients;
    if ingredients.len() == 0 {
        send_client_message(connection_id, "No ingredients defined for this recipe.");
        return;
    }

    let output = olc::c_header("=== Ingredients for " + recipe.name + " ===", connection_id) + "\n";
    let idx = 0;
    for ing in ingredients {
        output += format_ingredient_line(idx, ing);
        idx += 1;
    }
    send_client_message(connection_id, output);
}

fn format_ingredient_line(idx, ingredient) {
    let name = "";
    let quantity_suffix = "x";

    if ingredient.vnum != () && ingredient.vnum != "" {
        name = ingredient.vnum;
        // Try to get item name
        let item = get_item_by_vnum(ingredient.vnum);
        if item != () {
            name += " (" + item.name + ")";
        }
    } else if ingredient.category != () && ingredient.category != "" {
        let cat = ingredient.category;
        if cat.starts_with("@liquid:") {
            // Liquid ingredient - show liquid type and units
            let liquid_type = cat.sub_string(8);
            name = cat + " (liquid)";
            quantity_suffix = " units";
        } else {
            // Regular category - add @ prefix for display
            name = "@" + cat + " (category)";
        }
    } else {
        name = "(invalid)";
    }
    return "  " + idx + ": " + ingredient.quantity + quantity_suffix + " " + name + "\n";
}

fn add_ingredient(vnum, parts, connection_id) {
    // recedit <vnum> ingredient add <vnum|@category> <quantity>
    if parts.len() < 5 {
        send_client_message(connection_id, "Usage: recedit " + vnum + " ingredient add <item_vnum|@category> <quantity>");
        return;
    }

    let spec = parts[3];
    let quantity = parse_int(parts[4]);

    if quantity == () || quantity < 1 {
        send_client_message(connection_id, "Quantity must be a positive number.");
        return;
    }

    if add_recipe_ingredient(vnum, spec, quantity) {
        let desc = "";
        if spec.starts_with("@") {
            desc = "category '" + spec.sub_string(1, spec.len() - 1) + "'";
        } else {
            desc = "vnum '" + spec + "'";
            let item = get_item_by_vnum(spec);
            if item != () {
                desc += " (" + item.name + ")";
            }
        }
        send_client_message(connection_id, "Added ingredient: " + quantity + "x " + desc);
    } else {
        send_client_message(connection_id, "Failed to add ingredient.");
    }
}

fn remove_ingredient(vnum, parts, connection_id) {
    // recedit <vnum> ingredient remove <index>
    if parts.len() < 4 {
        send_client_message(connection_id, "Usage: recedit " + vnum + " ingredient remove <index>");
        return;
    }

    let idx = parse_int(parts[3]);
    if idx == () {
        send_client_message(connection_id, "Invalid index.");
        return;
    }

    if remove_recipe_ingredient(vnum, idx) {
        send_client_message(connection_id, "Removed ingredient at index " + idx + ".");
    } else {
        send_client_message(connection_id, "Failed to remove ingredient. Check the index.");
    }
}

// === Tool Subcommands ===

fn handle_tool_subcommand(recipe, vnum, parts, connection_id) {
    if parts.len() < 3 {
        show_tool_help(vnum, connection_id);
        return;
    }

    let action = parts[2].to_lower();

    if action == "list" { list_tools(recipe, vnum, connection_id); return; }
    if action == "add" { add_tool(vnum, parts, connection_id); return; }
    if action == "remove" { remove_tool(vnum, parts, connection_id); return; }

    show_tool_help(vnum, connection_id);
}

fn show_tool_help(vnum, connection_id) {
    let output = "Tool Commands:\n";
    output += "  recedit " + vnum + " tool list\n";
    output += "  recedit " + vnum + " tool add <vnum|@category> <location>\n";
    output += "  recedit " + vnum + " tool remove <index>\n";
    output += "\nLocations: inv (inventory), room, either\n";
    output += "\nExamples:\n";
    output += "  tool add furniture:oven room       - Requires oven in room\n";
    output += "  tool add @knife inv                - Requires any knife in inventory\n";
    output += "  tool add tool:hammer either        - Hammer can be in inv or room\n";
    send_client_message(connection_id, output);
}

fn list_tools(recipe, vnum, connection_id) {
    let tools = recipe.tools;
    if tools.len() == 0 {
        send_client_message(connection_id, "No tools required for this recipe.");
        return;
    }

    let output = olc::c_header("=== Tools for " + recipe.name + " ===", connection_id) + "\n";
    let idx = 0;
    for tool in tools {
        output += format_tool_line(idx, tool);
        idx += 1;
    }
    send_client_message(connection_id, output);
}

fn format_tool_line(idx, tool) {
    let name = "";
    if tool.vnum != () && tool.vnum != "" {
        name = tool.vnum;
        let item = get_item_by_vnum(tool.vnum);
        if item != () {
            name += " (" + item.name + ")";
        }
    } else if tool.category != () && tool.category != "" {
        name = "@" + tool.category + " (category)";
    } else {
        name = "(invalid)";
    }

    let loc = format_tool_location(tool.location);
    return "  " + idx + ": " + name + " [" + loc + "]\n";
}

fn format_tool_location(location) {
    if location == "Inventory" { return "inventory"; }
    if location == "Room" { return "room"; }
    if location == "Either" { return "either"; }
    return location;
}

fn add_tool(vnum, parts, connection_id) {
    // recedit <vnum> tool add <vnum|@category> <location>
    if parts.len() < 5 {
        send_client_message(connection_id, "Usage: recedit " + vnum + " tool add <item_vnum|@category> <inv|room|either>");
        return;
    }

    let spec = parts[3];
    let loc = parts[4].to_lower();

    // Normalize location
    if loc == "inv" { loc = "inventory"; }

    // Validate location
    if loc != "inventory" && loc != "room" && loc != "either" {
        send_client_message(connection_id, "Invalid location. Use: inv, room, either");
        return;
    }

    if add_recipe_tool(vnum, spec, loc) {
        let desc = "";
        if spec.starts_with("@") {
            desc = "category '" + spec.sub_string(1, spec.len() - 1) + "'";
        } else {
            desc = "vnum '" + spec + "'";
            let item = get_item_by_vnum(spec);
            if item != () {
                desc += " (" + item.name + ")";
            }
        }
        send_client_message(connection_id, "Added tool: " + desc + " [" + loc + "]");
    } else {
        send_client_message(connection_id, "Failed to add tool.");
    }
}

fn remove_tool(vnum, parts, connection_id) {
    // recedit <vnum> tool remove <index>
    if parts.len() < 4 {
        send_client_message(connection_id, "Usage: recedit " + vnum + " tool remove <index>");
        return;
    }

    let idx = parse_int(parts[3]);
    if idx == () {
        send_client_message(connection_id, "Invalid index.");
        return;
    }

    if remove_recipe_tool(vnum, idx) {
        send_client_message(connection_id, "Removed tool at index " + idx + ".");
    } else {
        send_client_message(connection_id, "Failed to remove tool. Check the index.");
    }
}

// === Display Functions ===

fn show_recipe_properties(recipe, connection_id) {
    let vnum = recipe.id;

    let output = olc::c_header("=== Recipe Editor ===", connection_id) + "\n";
    output += olc::c_label("VNUM: ", connection_id) + olc::c_value(vnum, connection_id) + "\n";
    output += olc::c_label("Name: ", connection_id) + olc::c_value(recipe.name, connection_id) + "\n";
    output += olc::c_label("Skill: ", connection_id) + olc::c_value(recipe.skill, connection_id) + "\n";
    output += olc::c_label("Level Required: ", connection_id) + olc::c_value("" + recipe.skill_required, connection_id) + "\n";

    let autolearn_str = "OFF (requires recipe book/trainer)";
    if recipe.auto_learn { autolearn_str = "ON (learned at skill level)"; }
    output += olc::c_label("Auto-Learn: ", connection_id) + olc::c_value(autolearn_str, connection_id) + "\n";

    output += olc::c_label("Difficulty: ", connection_id) + olc::c_value("" + recipe.difficulty, connection_id) + "\n";
    output += olc::c_label("Base XP: ", connection_id) + olc::c_value("" + recipe.base_xp, connection_id) + "\n";

    // Output item
    output += "\n" + olc::c_label("Output: ", connection_id);
    let proto = get_item_by_vnum(recipe.output_vnum);
    if proto != () {
        output += olc::c_value(recipe.output_vnum + " (" + proto.name + ")", connection_id);
    } else if recipe.output_vnum != "" {
        output += olc::c_warn(recipe.output_vnum + " (NOT FOUND)", connection_id);
    } else {
        output += olc::c_warn("(not set)", connection_id);
    }
    if recipe.output_quantity > 1 {
        output += " x" + recipe.output_quantity;
    }
    output += "\n";

    // Ingredients
    output += "\n" + olc::c_header("Ingredients:", connection_id) + "\n";
    if recipe.ingredients.len() == 0 {
        output += "  (none)\n";
    } else {
        let idx = 0;
        for ing in recipe.ingredients {
            output += format_ingredient_line(idx, ing);
            idx += 1;
        }
    }

    // Tools
    output += "\n" + olc::c_header("Tools:", connection_id) + "\n";
    if recipe.tools.len() == 0 {
        output += "  (none)\n";
    } else {
        let idx = 0;
        for tool in recipe.tools {
            output += format_tool_line(idx, tool);
            idx += 1;
        }
    }

    // Commands help
    output += build_commands_help(vnum);

    send_client_message(connection_id, output);
}

fn build_commands_help(vnum) {
    let output = "\n" + "Commands:\n";
    output += "  recedit " + vnum + " name <text>\n";
    output += "  recedit " + vnum + " vnum <new_vnum>\n";
    output += "  recedit " + vnum + " skill <cooking|crafting>\n";
    output += "  recedit " + vnum + " level <0-10>\n";
    output += "  recedit " + vnum + " autolearn [on|off]\n";
    output += "  recedit " + vnum + " difficulty <1-10>\n";
    output += "  recedit " + vnum + " xp <amount>\n";
    output += "  recedit " + vnum + " output <item_vnum> [quantity]\n";
    output += "\nIngredient Commands:\n";
    output += "  recedit " + vnum + " ingredient list\n";
    output += "  recedit " + vnum + " ingredient add <vnum|@category> <qty>\n";
    output += "  recedit " + vnum + " ingredient remove <index>\n";
    output += "\nTool Commands:\n";
    output += "  recedit " + vnum + " tool list\n";
    output += "  recedit " + vnum + " tool add <vnum|@category> <inv|room|either>\n";
    output += "  recedit " + vnum + " tool remove <index>\n";
    return output;
}
