// scripts/lib/crafting.rhai
// Machinery shared by every command that turns materials into an item.
//
// `craft` and `cook` had their own copies of the tool check, the quality
// average and the output spawn; `experiment` would have been a third. These
// three pieces are the ones all of them genuinely share — an experiment
// works from an explicit list of named items, so it needs none of the
// ingredient-finding or liquid handling that craft and cook still own.
//
// (craft.rhai and cook.rhai remain near-identical to each other beyond this.
// That duplication is older than this file and is left alone deliberately:
// collapsing it is a separate change with its own risk, and doing it here
// would have hidden it inside an unrelated feature.)

fn tool_name(tool) {
    if tool.vnum != () { return tool.vnum; }
    if tool.category != () { return tool.category; }
    return "unknown";
}

fn location_hint(tool) {
    if tool.location == "Room" { return " (in room)"; }
    if tool.location == "Inventory" { return " (in inventory)"; }
    return "";
}

fn missing_tools_message(recipe, char_name, room_id) {
    let missing = [];
    for tool in recipe.tools {
        let found = find_tool(char_name, room_id, tool);
        if found == () {
            missing.push(tool_name(tool) + location_hint(tool));
        }
    }
    if missing.len() > 0 {
        return "Missing tools: " + missing.join(", ");
    }
    return "";
}

// Locate every tool a recipe needs and average their quality in one pass.
// Returns #{ ok, message, ids, avg_quality }; `message` names what is
// missing when ok is false. Tools are never consumed — only their condition
// feeds the quality roll.
fn gather_tools(recipe, char_name, room_id) {
    let result = #{ ok: true, message: "", ids: [], avg_quality: 50 };
    let quality_sum = 0;
    for tool in recipe.tools {
        let found = find_tool(char_name, room_id, tool);
        if found == () {
            result.ok = false;
            result.message = missing_tools_message(recipe, char_name, room_id);
            return result;
        }
        result.ids.push(found);
        quality_sum += get_item_quality(found);
    }
    if result.ids.len() > 0 {
        result.avg_quality = quality_sum / result.ids.len();
    }
    return result;
}

// Mean quality of a set of items, or 50 for an empty set — the same neutral
// midpoint the quality roll assumes when a recipe has no ingredients.
fn average_item_quality(item_ids) {
    if item_ids.len() == 0 { return 50; }
    let total = 0;
    for id in item_ids {
        total += get_item_quality(id);
    }
    return total / item_ids.len();
}

// Spawn a recipe's output. Returns the items created, which is empty when
// the prototype is missing — callers report that rather than failing
// silently, because a builder typo here costs the player their materials.
fn spawn_outputs(recipe, char_name, quality_tier) {
    let created = [];
    for i in 0..recipe.output_quantity {
        let item = spawn_crafted_item(recipe.output_vnum, char_name, quality_tier);
        if item != () {
            created.push(item);
        }
    }
    return created;
}
