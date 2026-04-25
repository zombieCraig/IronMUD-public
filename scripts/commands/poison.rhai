// scripts/commands/poison.rhai
// poison <target> [with <vial>] - Lace a piece of food or a drink with poison.
//
// Anyone can attempt this, but with thievery skill < 3 there is a chance the
// poison is poorly hidden — the food/drink is still flagged as poisoned (so a
// careful examiner can tell) but no actual poison effect is applied. At
// thievery 3+ the dose lands cleanly.

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    if args == "" {
        send_client_message(connection_id, "Poison what? (poison <food/drink> [with <vial>])");
        return;
    }

    // Parse "<target> with <vial>" or just "<target>"
    let target_keyword = "";
    let vial_keyword = "";
    let lower = args.to_lower();
    let with_idx = lower.index_of(" with ");
    if with_idx >= 0 {
        target_keyword = args.sub_string(0, with_idx);
        let after = args.sub_string(with_idx + 6);
        vial_keyword = after;
    } else {
        target_keyword = args;
    }
    target_keyword = target_keyword.to_lower();
    vial_keyword = vial_keyword.to_lower();

    // Resolve target in inventory.
    let target = find_item_in_inventory(char.name, target_keyword);
    if target == () {
        send_client_message(connection_id, "You don't have '" + target_keyword + "'.");
        return;
    }

    let target_is_food = target.is_food;
    let target_is_drink = target.is_liquid_container;
    if !target_is_food && !target_is_drink {
        send_client_message(connection_id, "You can only poison food or drink.");
        return;
    }

    // Don't let players poison the poison vial they just made.
    if target_is_drink && target.liquid_type == "poison" {
        send_client_message(connection_id, "It is already poison.");
        return;
    }

    // Resolve the poison vial. If the user specified a keyword, use it; otherwise
    // pick the first poison vial in inventory (matching envenom.rhai's pattern).
    let vial = ();
    if vial_keyword != "" {
        vial = find_item_in_inventory(char.name, vial_keyword);
        if vial == () {
            send_client_message(connection_id, "You don't have '" + vial_keyword + "'.");
            return;
        }
        if !vial.is_liquid_container || vial.liquid_type != "poison" {
            send_client_message(connection_id, "That isn't poison.");
            return;
        }
    } else {
        let inv = get_items_in_inventory(char.name);
        for item in inv {
            if item.is_liquid_container && item.liquid_type == "poison" && item.liquid_current > 0 {
                vial = item;
                break;
            }
        }
        if vial == () {
            send_client_message(connection_id, "You aren't carrying any poison.");
            return;
        }
    }

    if vial.liquid_current <= 0 {
        send_client_message(connection_id, vial.name + " is empty.");
        return;
    }

    // Don't poison the vial with itself.
    if target.id == vial.id {
        send_client_message(connection_id, "You can't poison the vial with itself.");
        return;
    }

    // Skill-graded outcome: thievery 3+ always lands cleanly; below that has a
    // botch chance that scales 50% -> 30% -> 10% at thievery 0/1/2.
    let skill = get_skill_level(char.name, "thievery");
    let botched = false;
    if skill < 3 {
        let botch_chance = 50 - skill * 20;
        if botch_chance < 0 { botch_chance = 0; }
        let roll = random_int(1, 100);
        if roll <= botch_chance {
            botched = true;
        }
    }

    // Always consume one sip — fumbling still wastes the dose.
    drink_from(vial.id, 1);

    // Mark the food/drink as visibly tainted so an examiner can spot it.
    if target_is_food {
        set_food_poisoned(target.id, true);
    } else {
        set_liquid_poisoned(target.id, true);
    }

    if botched {
        send_client_message(connection_id, "\x1b[0;33mYou fumble the dose — a slick of poison clings to " + target.name + " in plain sight. The venom is wasted.\x1b[0m");
        return;
    }

    // Clean success: append a poison effect to the target's effects list. Drink
    // and eat scripts route effect_type=="poison" through apply_ongoing_effect,
    // which produces a real DoT (magnitude = damage-per-round, duration = rounds).
    let dpr = 3;
    let rounds = 3;
    if target_is_food {
        add_food_effect(target.id, "poison", dpr, rounds);
    } else {
        add_liquid_effect(target.id, "poison", dpr, rounds);
    }

    send_client_message(connection_id, "\x1b[0;32mYou carefully lace " + target.name + " with poison.\x1b[0m");
    broadcast_to_room_except(char.current_room_id, connection_id, char.name + " quietly tampers with " + target.name + ".");

    let leveled = add_skill_experience(char.name, "thievery", 4);
    if leveled {
        send_client_message(connection_id, "\x1b[1;33mYour thievery skill has improved!\x1b[0m");
    }
}
