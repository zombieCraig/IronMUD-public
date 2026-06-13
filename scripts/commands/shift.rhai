// scripts/commands/shift.rhai
// Werewolf form shifting: homid (human) / crinos (war-form) / lupus (wolf).
// @description Werewolf only — shift between homid, crinos (war-form, 3 Rage) and lupus (wolf, 1 Rage)
// @category skills
//
// The transaction (form validation, frenzy lock, rage cost, buff swap, the
// forced frenzy roll when shifting at high rage) lives in the Rust binding
// `werewolf_shift`; this script renders messages.

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    if !is_pc_werewolf(connection_id) {
        send_client_message(connection_id, "Your skin fits you fine the way it is.");
        return;
    }

    let form = args.to_lower();
    if form == "" {
        let current = get_pc_form(connection_id);
        send_client_message(connection_id, "You wear the " + current + " form. Shift to: homid (free), crinos (3 Rage), lupus (1 Rage).");
        return;
    }

    if char.position == "sleeping" {
        send_client_message(connection_id, "You are asleep. Wake up first!");
        return;
    }

    let result = werewolf_shift(connection_id, form);
    if !result["success"] {
        show_shift_error(result, form, connection_id);
        return;
    }

    show_shift_success(result, char, connection_id);
}

fn show_shift_error(result, form, connection_id) {
    let error = result["error"];
    if error == "unknown form" {
        send_client_message(connection_id, "Forms: homid, crinos, lupus.");
        return;
    }
    if error == "already in form" {
        send_client_message(connection_id, "You already wear the " + form + " form.");
        return;
    }
    if error == "not enough rage" {
        send_client_message(connection_id, "Not enough Rage — the " + form + " form takes " + result["cost"] + ". Fight, take wounds, or use 'rage' to build it.");
        return;
    }
    if error == "frenzying" {
        send_client_message(connection_id, "The wolf is driving. You shift when IT wants to.");
        return;
    }
    send_client_message(connection_id, "The change refuses: " + error);
}

fn show_shift_success(result, char, connection_id) {
    let form = result["form"];
    let room_id = char.current_room_id;
    if form == "crinos" {
        send_client_message(connection_id, "\x1b[1;31mBone cracks and re-knits. You unfold into nine feet of fur, muscle and killing intent.\x1b[0m");
        broadcast_to_room_except(room_id, connection_id, char.name + " erupts into a towering wolf-thing of fur and fang!");
    } else if form == "lupus" {
        send_client_message(connection_id, "\x1b[1;33mYou drop to all fours and the world sharpens into scent and speed.\x1b[0m");
        broadcast_to_room_except(room_id, connection_id, char.name + " flows down into the shape of a great wolf.");
    } else {
        send_client_message(connection_id, "\x1b[36mFur recedes; bone settles. You stand on two human feet again.\x1b[0m");
        broadcast_to_room_except(room_id, connection_id, char.name + " shrinks back into human shape.");
    }
    send_client_message(connection_id, "Rage: " + result["rage"] + "/" + get_pc_max_rage(connection_id));
    if result["frenzied"] {
        send_client_message(connection_id, "\x1b[1;31mThe change tears the cage open — the wolf is driving now!\x1b[0m");
        broadcast_to_room_except(room_id, connection_id, char.name + "'s eyes flood amber. Something with too many teeth is wearing their face.");
    }
}
