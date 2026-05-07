// scripts/commands/pet.rhai
//
// Manage pets — permanent charmed companions stamped via the charm spell on
// `flags.tameable` mobiles. Pets persist across logout and follow propagation.
//
//   pet              -- list your pets
//   pet list         -- list your pets
//   pet dismiss <kw> -- release a same-room pet (clears pet_owner + buff)

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if type_of(char) == "()" { return; }

    let parts = args.split(" ");
    let cleaned = [];
    for p in parts {
        if p != "" { cleaned.push(p); }
    }

    if cleaned.len() == 0 {
        list_pets(connection_id, char);
        return;
    }

    let sub = cleaned[0].to_lower();

    if sub == "list" {
        list_pets(connection_id, char);
        return;
    }

    if sub == "dismiss" || sub == "release" {
        if cleaned.len() < 2 {
            send_client_message(connection_id, "Usage: pet dismiss <keyword>");
            return;
        }
        dismiss_pet(connection_id, char, cleaned[1]);
        return;
    }

    send_client_message(connection_id, "Usage: pet [list|dismiss <keyword>]");
}

fn list_pets(connection_id, char) {
    let pets = list_pets_for_player(char.name);
    if pets.len() == 0 {
        send_client_message(connection_id, "You have no pets.");
        return;
    }

    let output = "Your pets:\n";
    let here_room = char.current_room_id;
    for pet in pets {
        let location = "(elsewhere)";
        if pet.current_room_id == here_room {
            location = "(here)";
        } else if pet.current_room_id != "" {
            let room = get_room_data(pet.current_room_id);
            if room != () {
                location = "(" + room.title + ")";
            }
        }

        let status = "follows you";
        if get_charm_stay(pet.id) {
            status = "waits where left";
        } else {
            let follow_other = get_charm_follow_player(pet.id);
            if follow_other != "" && follow_other.to_lower() != char.name.to_lower() {
                status = "follows " + follow_other;
            }
        }

        output += "  " + pet.name + " " + location + " — " + status + "\n";
    }
    send_client_message(connection_id, output);
}

fn dismiss_pet(connection_id, char, keyword) {
    let mobiles = filter_visible_mobiles(get_mobiles_in_room(char.current_room_id), char.name);
    let mob = find_mobile_by_keyword(keyword, mobiles);
    if mob == () {
        send_client_message(connection_id, "There is no '" + keyword + "' here.");
        return;
    }

    let owner = get_mobile_pet_owner(mob.id);
    if owner == "" || owner.to_lower() != char.name.to_lower() {
        send_client_message(connection_id, mob.short_desc + " is not your pet.");
        return;
    }

    set_mobile_pet_owner(mob.id, "");
    remove_buff_from_mobile(mob.id, "charmed");
    set_charm_stay(mob.id, false);
    set_charm_follow_player(mob.id, "");

    send_client_message(connection_id, "You release " + mob.name + ". " + mob.short_desc + " wanders off.");
    broadcast_to_room_except(char.current_room_id, connection_id, mob.short_desc + " wanders off, no longer bonded to " + char.name + ".");
}
