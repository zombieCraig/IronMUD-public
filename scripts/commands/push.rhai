// scripts/commands/push.rhai
// Two races can push past their limits, with opposite costs:
//
// Replicant push: exceed engineered limits at the cost of Resolve.
// Blade Runner rule: pushing never costs a replicant physical damage —
// it ALWAYS costs stress. Spending down to 0 is allowed and triggers an
// immediate breakdown; that risk is the core of the mechanic.
//
// Mutant push (Mutant: Year Zero): strain the warped body for a short
// exertion surge — and the trauma feeds the mutation. The ONLY way a
// mutant earns Mutation Points is by hurting themselves like this.

const PUSH_COOLDOWN_SECS = 120;
const PUSH_BUFF_SECS = 30;
const MUTANT_PUSH_COOLDOWN_SECS = 60;
const MUTANT_PUSH_MIN_HP_PCT = 15;

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    if is_pc_mutant(connection_id) {
        run_mutant_push(connection_id, char);
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

// Mutant exertion surge: trauma for Mutation Points + a short combat edge.
fn run_mutant_push(connection_id, char) {
    let now = get_current_time();
    let cooldowns = char.racial_cooldowns;
    let cd_end = 0;
    if "push" in cooldowns {
        cd_end = cooldowns["push"];
    }
    if cd_end > now {
        let remaining = cd_end - now;
        send_client_message(connection_id, "Your body is still knitting from the last push. (" + remaining + "s)");
        return;
    }

    let hp_floor = char.max_hp * MUTANT_PUSH_MIN_HP_PCT / 100;
    if char.hp <= hp_floor {
        send_client_message(connection_id, "You're too torn up to push any harder. Your body has nothing left to feed the Zone.");
        return;
    }

    if get_pc_mp(connection_id) >= get_pc_max_mp(connection_id) {
        send_client_message(connection_id, "The mutation already thrums at full strength under your skin.");
        return;
    }

    // Biohazard severity 1-3: scales both the flesh price and the MP gain.
    let severity = random_int(1, 3);
    let hp_cost = char.max_hp * 5 * severity / 100;
    if hp_cost < 1 { hp_cost = 1; }

    cooldowns["push"] = now + MUTANT_PUSH_COOLDOWN_SECS;
    char.racial_cooldowns = cooldowns;
    char.hp = char.hp - hp_cost;
    if char.hp < 1 { char.hp = 1; }
    save_character_data(char);

    let new_mp = change_pc_mp(connection_id, severity);
    let max_mp = get_pc_max_mp(connection_id);

    // The surge: combat blessing + luck for the skill-roll edge.
    apply_buff(char.name, "bless", 3, PUSH_BUFF_SECS, "Pushing");
    apply_buff(char.name, "luck", 10, PUSH_BUFF_SECS, "Pushing");

    let room_id = char.current_room_id;
    send_client_message(connection_id, "\x1b[1;33mYou grit your teeth and PUSH past your limits.\x1b[0m");
    send_client_message(connection_id, "\x1b[31mPain flares through your warped muscles. [-" + hp_cost + " hp]\x1b[0m");
    send_client_message(connection_id, "\x1b[1;32mThe Zone stirs in your blood. [+" + severity + " MP] (" + new_mp + "/" + max_mp + ")\x1b[0m");
    send_client_message(connection_id, "\x1b[36mYou feel unstoppable. (Pushing: " + PUSH_BUFF_SECS + "s)\x1b[0m");
    broadcast_to_room(room_id, char.name + " strains against something unseen — flesh shifting, eyes wild.", char.name);
}
