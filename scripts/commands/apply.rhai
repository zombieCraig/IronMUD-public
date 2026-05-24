// scripts/commands/apply.rhai
// apply <tool> [on <target>]  - Use a Tool item (consumable/script-driven kit)
//
// The verb only resolves the item + target and fires the item's OnUse
// trigger; all gameplay effects (cooldown, HP changes, broadcasts, self-
// purge) live in the trigger body. This is the generic counterpart to
// quaff (Potion), zap (Wand), brandish (Staff), eat (Food), drink (Liquid).
//
// Target is optional and defaults to the actor (kit-on-self).

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    if args == "" {
        send_client_message(connection_id, "Apply what?");
        return;
    }

    let parsed = parse_apply_args(args);
    let item_keyword = parsed.item;
    let target_keyword = parsed.target;

    if item_keyword == "" {
        send_client_message(connection_id, "Apply what?");
        return;
    }

    let item = find_item_in_inventory(char.name, item_keyword);
    if item == () {
        send_client_message(connection_id, "You don't have any '" + item_keyword + "'.");
        return;
    }

    if item.item_type != "tool" {
        send_client_message(connection_id, type_specific_reject(item.item_type));
        return;
    }

    // Resolve target. Empty / self / me → actor.
    let target_kind = "self";
    let target_name = char.name;
    let target_id = "";

    if target_keyword != "" && target_keyword.to_lower() != "self"
        && target_keyword.to_lower() != "me"
        && target_keyword.to_lower() != char.name.to_lower()
    {
        // Try same-room players first.
        let chars_in_room = get_characters_in_room(char.current_room_id);
        let found_player = ();
        for other in chars_in_room {
            if other.name.to_lower() == target_keyword.to_lower()
                || other.name.to_lower().starts_with(target_keyword.to_lower())
            {
                found_player = other;
                break;
            }
        }
        if found_player != () {
            target_kind = "player";
            target_name = found_player.name;
            target_id = "";
        } else {
            // Then same-room mobs.
            let mobs = get_mobiles_in_room(char.current_room_id);
            let found_mob = find_mobile_by_keyword(target_keyword, mobs);
            if found_mob == () {
                send_client_message(connection_id, "There is no '" + target_keyword + "' here.");
                return;
            }
            target_kind = "mob";
            target_name = found_mob.name;
            target_id = found_mob.id;
        }
    }

    // Fire OnUse — the script owns all effects.
    fire_item_trigger(item.id, "on_use", connection_id, #{
        "char_name": char.name,
        "room_id": char.current_room_id,
        "use_type": "apply",
        "target_kind": target_kind,
        "target_name": target_name,
        "target_id": target_id,
    });
}

// Parses `<item> [on <target>]` / `<item> <target>` (with-or-without `on`).
// Returns #{ item, target } — either may be "".
fn parse_apply_args(args) {
    let parts = args.split(" ");
    if parts.len() == 1 {
        return #{ item: parts[0], target: "" };
    }
    // Look for an "on" / "to" pivot (case-insensitive).
    let pivot = -1;
    let idx = 0;
    for p in parts {
        let lower = p.to_lower();
        if lower == "on" || lower == "to" {
            pivot = idx;
            break;
        }
        idx += 1;
    }
    if pivot >= 0 {
        let item_parts = parts.extract(0, pivot);
        let target_parts = parts.extract(pivot + 1, parts.len() - pivot - 1);
        return #{ item: item_parts.join(" "), target: target_parts.join(" ") };
    }
    // No pivot: shorthand `apply <item> <target>` — last word is target.
    let target = parts[parts.len() - 1];
    let item_parts = parts.extract(0, parts.len() - 1);
    return #{ item: item_parts.join(" "), target: target };
}

// Type-specific rejection so players get a hint toward the right verb.
fn type_specific_reject(item_type) {
    if item_type == "potion" { return "Quaff that, don't apply it."; }
    if item_type == "wand" { return "Zap with that, don't apply it."; }
    if item_type == "staff" { return "Brandish that, don't apply it."; }
    if item_type == "food" { return "Eat that, don't apply it."; }
    if item_type == "liquid_container" { return "Drink from that, don't apply it."; }
    if item_type == "note" { return "Read that, don't apply it."; }
    if item_type == "board" { return "Use the `board` command for that."; }
    return "You can't apply that.";
}
