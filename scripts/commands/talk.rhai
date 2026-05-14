// scripts/commands/talk.rhai
// talk <mob>
//
// Enters a sticky dialogue mode with a mob in the room. The mob's text is
// broadcast to the room; the numbered choice menu prints to the actor only.
// Numeric input (1, 2, 3, ...) and choice keywords are routed by lib.rs's
// input dispatcher while `dialogue_partner_id` is set. `bye`/`leave`/movement
// exit the conversation.

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    if char.position == "sleeping" {
        send_client_message(connection_id, "You are asleep. Wake up first!");
        return;
    }

    let arg = args;
    if arg == "" {
        send_client_message(connection_id, "Talk to whom?");
        return;
    }

    let result = start_talk(connection_id, arg);
    if !result.ok {
        let err = result.error;
        if err == "" { err = "You don't see anyone like that to talk to."; }
        send_client_message(connection_id, err);
        return;
    }

    let room_id = result.mob_room_id;
    let mob_id = result.mob_id;
    let short = result.mob_short_desc;
    let response = result.response;
    let menu = result.menu;

    if response != "" {
        let say_line = short + " says: " + response;
        let lang = get_mobile_spoken_language(mob_id);
        if lang == "" || is_lingua_franca(lang) {
            send_client_message(connection_id, say_line);
            broadcast_to_room_except(room_id, connection_id, say_line);
        } else {
            for listener in get_characters_in_room(room_id) {
                let listener_conn = find_player_connection_by_name(listener);
                if listener_conn != "" {
                    let heard = garble_for_mob_listener(response, lang, listener);
                    send_client_message(listener_conn, short + " says: " + heard);
                }
            }
        }
    }
    if menu != "" {
        send_client_message(connection_id, menu);
    }

    if result.finished {
        // Tree only had a root with effects then immediately Exit'd; clean exit.
        send_client_message(connection_id, "(The conversation ends.)");
    }
}
