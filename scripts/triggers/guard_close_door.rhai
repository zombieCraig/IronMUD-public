// scripts/triggers/guard_close_door.rhai
// Mobile idle trigger: guard checks for open doors and closes one per fire
// Attach to a guard mobile as an OnIdle trigger

fn get_opposite_direction(dir) {
    if dir == "north" { return "south"; }
    if dir == "south" { return "north"; }
    if dir == "east" { return "west"; }
    if dir == "west" { return "east"; }
    if dir == "up" { return "down"; }
    if dir == "down" { return "up"; }
    return "";
}

fn run_trigger(mobile_id, room_id, context) {
    let mob = get_mobile_data(mobile_id);
    let mob_name = mob["name"];

    let directions = ["north", "south", "east", "west", "up", "down"];

    for dir in directions {
        let door = get_door(room_id, dir);
        if door == () { continue; }
        if door.is_closed { continue; }

        // Close the door on this side
        set_door_closed(room_id, dir, true);
        broadcast_to_room(room_id, mob_name + " closes the " + door.name + ".", "");

        // Sync the opposite side
        let target_room_id = get_exit_target(room_id, dir);
        if target_room_id != "" {
            let opposite = get_opposite_direction(dir);
            if opposite != "" {
                if has_door(target_room_id, opposite) {
                    set_door_closed(target_room_id, opposite, true);
                    broadcast_to_room(target_room_id, "The " + door.name + " closes from the other side.", "");
                }
            }
        }

        // Only close one door per trigger fire for natural pacing
        return;
    }
}
