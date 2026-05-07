// scripts/commands/pet.rhai
//
// Manage pets — permanent charmed companions stamped via the charm spell on
// `flags.tameable` mobiles. Pets persist across logout and follow propagation.
//
//   pet                       -- show pet status
//   pet list / pet status     -- show pet status (HP, location, mood, follow state)
//   pet rename <kw> <name>    -- give a pet a custom nickname (clears with empty)
//   pet dismiss <kw>          -- release a same-room pet (clears pet_owner + buff)

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if type_of(char) == "()" { return; }

    let parts = args.split(" ");
    let cleaned = [];
    for p in parts {
        if p != "" { cleaned.push(p); }
    }

    if cleaned.len() == 0 {
        show_pet_status(connection_id, char);
        return;
    }

    let sub = cleaned[0].to_lower();

    if sub == "list" || sub == "status" {
        show_pet_status(connection_id, char);
        return;
    }

    if sub == "rename" {
        if cleaned.len() < 2 {
            send_client_message(connection_id, "Usage: pet rename <keyword> <new name>  (omit name to clear)");
            return;
        }
        let keyword = cleaned[1];
        // Rejoin remaining tokens as the new name. If only one extra token was
        // given (the keyword), treat as clear.
        let new_name = "";
        if cleaned.len() >= 3 {
            for i in 2..cleaned.len() {
                if i > 2 { new_name += " "; }
                new_name += cleaned[i];
            }
        }
        rename_pet(connection_id, char, keyword, new_name);
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

    send_client_message(connection_id, "Usage: pet [list|rename <kw> <name>|dismiss <kw>]");
}

fn show_pet_status(connection_id, char) {
    let pets = list_pets_for_player(char.name);
    if pets.len() == 0 {
        send_client_message(connection_id, "You have no pets.");
        return;
    }

    let here_room = char.current_room_id;
    let output = "Your pets:\n";
    for pet in pets {
        let label = pet.name;
        if pet.has_nickname { label = pet.nickname + " (" + pet.name + ")"; }

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

        let hp_line = "" + pet.current_hp + "/" + pet.max_hp + " hp";

        output += "  " + label + " " + location + " — " + hp_line + ", " + status + "\n";
    }
    send_client_message(connection_id, output);
}

fn pet_lookup(char, keyword) {
    let pets = list_pets_for_player(char.name);
    let lower = keyword.to_lower();
    let here_room = char.current_room_id;
    // Prefer same-room match; fall back to anywhere-in-world match.
    let same_room = ();
    let elsewhere = ();
    for pet in pets {
        let matched = false;
        if pet.has_nickname && pet.nickname.to_lower().contains(lower) { matched = true; }
        if !matched && pet.name.to_lower().contains(lower) { matched = true; }
        if !matched {
            for kw in pet.keywords {
                if kw.to_lower().starts_with(lower) { matched = true; break; }
            }
        }
        if matched {
            if pet.current_room_id == here_room {
                same_room = pet;
                break;
            } else if elsewhere == () {
                elsewhere = pet;
            }
        }
    }
    if same_room != () { return same_room; }
    return elsewhere;
}

fn rename_pet(connection_id, char, keyword, new_name) {
    let pet = pet_lookup(char, keyword);
    if pet == () {
        send_client_message(connection_id, "You have no pet matching '" + keyword + "'.");
        return;
    }
    let stored = set_mobile_nickname(pet.id, new_name);
    if stored == "" && new_name != "" {
        send_client_message(connection_id, "That nickname could not be saved.");
        return;
    }
    if stored == "" {
        send_client_message(connection_id, pet.name + "'s nickname is cleared.");
    } else {
        send_client_message(connection_id, "You name " + pet.name + " '" + stored + "'.");
        if pet.current_room_id == char.current_room_id {
            broadcast_to_room_except(char.current_room_id, connection_id, char.name + " names " + pet.name + " '" + stored + "'.");
        }
    }
}

fn dismiss_pet(connection_id, char, keyword) {
    let pet = pet_lookup(char, keyword);
    if pet == () {
        send_client_message(connection_id, "You have no pet matching '" + keyword + "'.");
        return;
    }
    if pet.current_room_id != char.current_room_id {
        send_client_message(connection_id, pet.display_name + " is not here.");
        return;
    }

    set_mobile_pet_owner(pet.id, "");
    set_mobile_nickname(pet.id, "");
    remove_buff_from_mobile(pet.id, "charmed");
    set_charm_stay(pet.id, false);
    set_charm_follow_player(pet.id, "");

    let display = pet.display_name;
    send_client_message(connection_id, "You release " + display + ". " + display + " wanders off.");
    broadcast_to_room_except(char.current_room_id, connection_id, display + " wanders off, no longer bonded to " + char.name + ".");
}
