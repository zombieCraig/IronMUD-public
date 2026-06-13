// scripts/commands/repair.rhai
// Synth field repair: consume repair kits to patch the chassis.
// @description Synth only — consume a repair kit to restore your chassis (a CRITICAL chassis needs two at once)
// @category skills
//
// The transaction (cooldown, critical kit requirement, HP math, malfunction
// buff clears) lives in the Rust binding `synth_kit_repair`; this script
// validates kit possession, consumes kits on success, and renders messages.

const KIT_VNUM = "synth-repair-kit";

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    if !is_pc_synth(connection_id) {
        send_client_message(connection_id, "You have no chassis to repair. Try a medic.");
        return;
    }

    if char.position == "sleeping" {
        send_client_message(connection_id, "You are asleep. Wake up first!");
        return;
    }

    if is_in_combat(char.name) {
        send_client_message(connection_id, "You can't open your own panels while something is hitting you!");
        return;
    }

    let critical = is_pc_chassis_critical(connection_id);
    let kits_needed = if critical { 2 } else { 1 };

    let kits_carried = count_reagent(char.name, KIT_VNUM);
    if kits_carried < kits_needed {
        if critical {
            send_client_message(connection_id, "\x1b[1;31mYour chassis is CRITICAL — clawing back from emergency reserve takes TWO repair kits at once.\x1b[0m You carry " + kits_carried + ".");
        } else {
            send_client_message(connection_id, "You have no synth repair kit. Buy one, scavenge one, or find a technician.");
        }
        return;
    }

    let result = synth_kit_repair(connection_id, kits_needed);
    if !result["success"] {
        show_repair_error(result, connection_id);
        return;
    }

    // Consume the kits only after the binding accepted the repair.
    let used = 0;
    while used < kits_needed {
        consume_reagent(char.name, KIT_VNUM);
        used += 1;
    }

    let label = result["stage_label"];
    send_client_message(connection_id, "\x1b[36mYou crack a repair kit and work sealant into the damage. +" + result["healed"] + " HP. Chassis: " + label + ".\x1b[0m");
    if critical {
        send_client_message(connection_id, "\x1b[32mEmergency reserve countdown cleared.\x1b[0m");
    }
    broadcast_to_room(char.current_room_id, char.name + " patches " + char.name + "'s own chassis with a repair kit.", char.name);
}

fn show_repair_error(result, connection_id) {
    let error = result["error"];
    if error == "cooldown" {
        send_client_message(connection_id, "Your self-repair routines are still cycling. (" + result["cooldown_remaining"] + "s)");
        return;
    }
    if error == "needs more kits" {
        send_client_message(connection_id, "A CRITICAL chassis takes " + result["kits_needed"] + " kits at once.");
        return;
    }
    if error == "already nominal" {
        send_client_message(connection_id, "Diagnostics clean — nothing to repair.");
        return;
    }
    send_client_message(connection_id, "The repair fails: " + error);
}
