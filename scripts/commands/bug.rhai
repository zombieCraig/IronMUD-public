// bug - Report a bug (player command)
// Usage: bug <description>

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to report a bug.");
        return;
    }

    if args == "" {
        send_client_message(connection_id, "Usage: bug <description>\n\nDescribe the bug you encountered. Game state is automatically captured.");
        return;
    }

    // Cap description at 1000 characters
    let description = args;
    if description.len() > 1000 {
        description = description.sub_string(0, 1000);
    }

    // Auto-capture context
    let context = #{};

    // Room info
    let room = get_room_data(char.current_room_id);
    if room != () {
        context.room_id = char.current_room_id;
        context.room_vnum = room.vnum;
        context.room_title = room.title;
    }

    // Character stats
    context.character_level = char.level;
    context.character_class = char.class_name;
    context.character_race = char.race;
    context.character_position = char.position;
    context.hp = char.hp;
    context.max_hp = char.max_hp;
    context.mana = char.mana;
    context.max_mana = char.max_mana;
    context.in_combat = is_in_combat(char.name);

    // Game time, season, weather
    let gt = get_game_time();
    context.game_time = "Hour " + gt.hour + ", Day " + gt.day + ", Month " + gt.month + ", Year " + gt.year;
    context.season = gt.season;
    context.weather = gt.weather;

    // Room occupants
    let players_in_room = [];
    let chars_in_room = get_characters_in_room(char.current_room_id);
    for p in chars_in_room {
        if p != char.name {
            players_in_room.push(p);
        }
    }
    context.players_in_room = players_in_room;

    let mobiles_in_room = [];
    let mobs = get_mobiles_in_room(char.current_room_id);
    for m in mobs {
        mobiles_in_room.push(m.name + " (" + m.vnum + ")");
    }
    context.mobiles_in_room = mobiles_in_room;

    // Submit the report
    let result = submit_bug_report(char.name, description, context, char.is_admin);

    if result.starts_with("ERROR") {
        send_client_message(connection_id, "\x1b[1;31m" + result + "\x1b[0m");
        return;
    }

    let ticket = result;
    let msg = "\x1b[1;32mBug report #" + ticket + " submitted. Thank you!\x1b[0m";
    if char.is_admin {
        msg += " (auto-approved)";
    }
    send_client_message(connection_id, msg);

    // Notify online admins (unless reporter is admin)
    if !char.is_admin {
        let preview = description;
        if preview.len() > 60 {
            preview = preview.sub_string(0, 57) + "...";
        }
        notify_admins_bug(char.name, parse_int(ticket), preview);
    }
}
