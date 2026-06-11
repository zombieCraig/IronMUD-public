// scripts/commands/push.rhai
// Replicant push: exceed engineered limits at the cost of Resolve.
// Blade Runner rule: pushing never costs a replicant physical damage —
// it ALWAYS costs stress. Spending down to 0 is allowed and triggers an
// immediate breakdown; that risk is the core of the mechanic.

const PUSH_COOLDOWN_SECS = 120;
const PUSH_BUFF_SECS = 30;

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    if !is_pc_replicant(connection_id) {
        send_client_message(connection_id, "Push what? Your body already gives everything it has.");
        return;
    }

    if is_pc_in_breakdown(connection_id) {
        send_client_message(connection_id, "Your mind is already past its limit.");
        return;
    }

    // Cooldown rides the shared racial_cooldowns map.
    let now = get_current_time();
    let cooldowns = char.racial_cooldowns;
    let cd_end = 0;
    if "push" in cooldowns {
        cd_end = cooldowns["push"];
    }
    if cd_end > now {
        let remaining = cd_end - now;
        send_client_message(connection_id, "Your systems are still recovering from the last push. (" + remaining + "s)");
        return;
    }

    let cost_str = get_setting_or_default("resolve_cost_push", "2");
    let cost = try_parse_int(cost_str);
    if cost == () { cost = 2; }
    if get_pc_resolve(connection_id) <= 0 {
        send_client_message(connection_id, "Your mind has nothing left to give.");
        return;
    }

    cooldowns["push"] = now + PUSH_COOLDOWN_SECS;
    char.racial_cooldowns = cooldowns;
    save_character_data(char);

    // The push itself: combat blessing + haste, body unharmed.
    apply_buff(char.name, "bless", 3, PUSH_BUFF_SECS, "Push");
    apply_buff(char.name, "haste", 1, PUSH_BUFF_SECS, "Push");

    let new_resolve = change_pc_resolve(connection_id, -cost);
    let max_resolve = get_pc_max_resolve(connection_id);

    let room_id = char.current_room_id;
    send_client_message(connection_id, "\x1b[1;36mYou push past every governor in your design. The world sharpens; your body sings.\x1b[0m");
    send_client_message(connection_id, "\x1b[35mIt costs you something deeper. (Resolve " + new_resolve + "/" + max_resolve + ")\x1b[0m");
    broadcast_to_room(room_id, char.name + " moves with sudden, impossible precision.", char.name);

    if new_resolve == 0 {
        let bd = trigger_pc_breakdown(connection_id);
        if bd["success"] {
            send_client_message(connection_id, "\x1b[1;31m" + bd["message"] + "\x1b[0m");
            let room_msg = bd["room_message"];
            room_msg.replace("{name}", char.name);
            broadcast_to_room(room_id, room_msg, char.name);
        }
    }
}
