// scripts/commands/drag.rhai
//
// Drag an unconscious mobile in your room to an adjacent room. Useful for
// rescuing a sun-burning vampire ally before the next sun tick finishes
// them, but works on any unconscious mob — wounded NPCs, bashed enemies, etc.
// PC-on-PC drag is a future enhancement.

const DRAG_STAMINA_COST = 10;

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to drag.");
        return;
    }

    if char.position == "sleeping" {
        send_client_message(connection_id, "You can't drag anyone while asleep.");
        return;
    }
    if char.position == "sitting" {
        send_client_message(connection_id, "Stand up before dragging anyone.");
        return;
    }
    if char.position != "standing" {
        send_client_message(connection_id, "You need to be standing to drag.");
        return;
    }

    if is_in_combat(char.name) {
        send_client_message(connection_id, "You are in combat! Drag won't help here.");
        return;
    }

    let parts = split_quoted_args(args);
    if parts.len() < 2 {
        send_client_message(connection_id, "Usage: drag <target> <direction>\nExample: drag vampire north");
        return;
    }

    let target_keyword = parts[0];
    let direction = parts[1].to_lower();

    let bypass_stamina = char.god_mode || is_in_build_mode(char.name, char.current_room_id);
    if !bypass_stamina && char.stamina < DRAG_STAMINA_COST {
        send_client_message(connection_id, "You're too exhausted to drag anything (need " + DRAG_STAMINA_COST + " stamina).");
        return;
    }

    // Find target mob in the same room.
    let room_mobiles = get_mobiles_in_room(char.current_room_id);
    let target = find_mobile_by_keyword(target_keyword, room_mobiles);
    if target == () {
        send_client_message(connection_id, "There's no '" + target_keyword + "' here to drag.");
        return;
    }

    // Draggable iff the target is unconscious or in the sun-burning rescue
    // window. Anything else is conscious enough to walk on its own.
    let draggable = target.is_unconscious
        || target.current_hp <= 0
        || mobile_has_buff(target.id, "sunlight_burning");
    if !draggable {
        send_client_message(connection_id, target.name + " is conscious — they can walk on their own.");
        return;
    }

    // Resolve the destination.
    let room = get_room_data(char.current_room_id);
    if room == () {
        send_client_message(connection_id, "You are nowhere. Something is wrong.");
        return;
    }
    let dest_room_id = get_exit_for_direction(room.exits, direction);
    if dest_room_id == "" {
        send_client_message(connection_id, "You can't drag them that way — no exit there.");
        return;
    }

    // Closed door check — same rules as walking.
    if check_door_blocks_movement(room.id, direction, connection_id, char.name) {
        return;
    }

    // Pay stamina up-front.
    if !bypass_stamina {
        char.stamina = char.stamina - DRAG_STAMINA_COST;
        if char.stamina < 0 { char.stamina = 0; }
    }

    let old_room_id = char.current_room_id;

    // Broadcast before moving so everyone in the source room sees the drag.
    broadcast_to_room_awake(old_room_id, char.name + " drags " + target.name + " " + direction + ".", char.name);

    // Move the mobile first; if that fails, abort without moving the player.
    if !move_mobile_to_room(target.id, dest_room_id) {
        send_client_message(connection_id, "Something blocks you from dragging " + target.name + ".");
        // Refund stamina since the action didn't complete.
        if !bypass_stamina { char.stamina = char.stamina + DRAG_STAMINA_COST; }
        save_character_data(char);
        set_player_character(connection_id, char);
        return;
    }

    // Move the player.
    char.current_room_id = dest_room_id;
    save_character_data(char);
    set_player_character(connection_id, char);

    // Bring along charmed pets, mirror of go.rhai's behavior.
    propagate_charmed_mobs(char.name, old_room_id, dest_room_id, direction);

    // Broadcast arrival in the destination room.
    let arrival_dir = get_opposite_direction(direction);
    broadcast_to_room_awake(dest_room_id, char.name + " arrives from the " + arrival_dir + ", dragging " + target.name + ".", char.name);

    send_client_message(connection_id, "You drag " + target.name + " " + direction + ".");
}

// Mirrors of go.rhai helpers — kept local so drag.rhai is self-contained.
fn get_opposite_direction(dir) {
    if dir == "north" || dir == "n" { return "south"; }
    if dir == "south" || dir == "s" { return "north"; }
    if dir == "east" || dir == "e" { return "west"; }
    if dir == "west" || dir == "w" { return "east"; }
    if dir == "up" || dir == "u" { return "below"; }
    if dir == "down" || dir == "d" { return "above"; }
    return "somewhere";
}

fn get_exit_for_direction(exits, direction) {
    if direction == "north" || direction == "n" { return exits.north; }
    if direction == "east" || direction == "e" { return exits.east; }
    if direction == "south" || direction == "s" { return exits.south; }
    if direction == "west" || direction == "w" { return exits.west; }
    if direction == "up" || direction == "u" { return exits.up; }
    if direction == "down" || direction == "d" { return exits.down; }
    if direction == "out" { return exits.out; }
    let dir_lower = direction.to_lower();
    if exits.custom.contains(dir_lower) {
        return exits.custom[dir_lower];
    }
    return "";
}

fn normalize_direction(dir) {
    if dir == "n" { return "north"; }
    if dir == "s" { return "south"; }
    if dir == "e" { return "east"; }
    if dir == "w" { return "west"; }
    if dir == "u" { return "up"; }
    if dir == "d" { return "down"; }
    return dir;
}

fn check_door_blocks_movement(room_id, direction, connection_id, char_name) {
    let dir_normalized = normalize_direction(direction);
    let door = get_door(room_id, dir_normalized);
    if door == () { return false; }
    if !door.is_closed { return false; }
    if is_in_build_mode(char_name, room_id) {
        send_client_message(connection_id, "\x1b[1;33m[BUILD]\x1b[0m You pass through the " + door.name + ".");
        return false;
    }
    if door.is_locked {
        send_client_message(connection_id, "The " + door.name + " is closed and locked.");
    } else {
        send_client_message(connection_id, "The " + door.name + " is closed.");
    }
    return true;
}
