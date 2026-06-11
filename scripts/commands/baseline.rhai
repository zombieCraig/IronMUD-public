// scripts/commands/baseline.rhai
// Replicant baseline test — the "doctor visit" of the resolve economy.
// Only works in a room flagged `baseline_office`. Passing fully restores
// Resolve and works off a strike; failing restores nothing, adds a strike,
// and locks retries behind a long cooldown. Three strikes: a retirement
// order is issued (24h recalibration debuff + the retirement_order trait).

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    if !is_pc_replicant(connection_id) {
        send_client_message(connection_id, "Baseline tests are for replicants. You'd pass. That's the problem.");
        return;
    }

    let room = get_room_data(char.current_room_id);
    if room == () || !room.flags.baseline_office {
        send_client_message(connection_id, "There's no baseline apparatus here. Find a baseline office.");
        return;
    }

    if is_pc_in_breakdown(connection_id) {
        send_client_message(connection_id, "The examiner takes one look at you and shakes their head. Come back when you can sit still.");
        return;
    }

    let result = replicant_baseline_test(connection_id);
    if !result["success"] {
        if result["error"] == "cooldown" {
            let remaining = result["cooldown_remaining"];
            let mins = remaining / 60;
            send_client_message(connection_id, "Your last test is still on file. Retest available in " + mins + " minutes.");
        } else {
            send_client_message(connection_id, "The apparatus refuses to engage.");
        }
        return;
    }

    // The ritual itself.
    send_client_message(connection_id, "\x1b[2mYou sit. The aperture dilates. A voice, flat as glass:\x1b[0m");
    send_client_message(connection_id, "\x1b[2m\"And blood-black nothingness began to spin... within cells interlinked within cells interlinked...\"\x1b[0m");
    send_client_message(connection_id, "\x1b[2m\"Cells.\" Cells. \"Interlinked.\" Interlinked.\x1b[0m");
    broadcast_to_room(char.current_room_id, char.name + " sits motionless before the baseline apparatus, answering in a flat monotone.", char.name);

    if result["passed"] {
        send_client_message(connection_id, "\x1b[1;32m\"Constant K. We're done. You can pick up your bonus on the way out.\"\x1b[0m");
        send_client_message(connection_id, "\x1b[36mYour Resolve is fully restored.\x1b[0m");
        if result["strikes"] > 0 {
            send_client_message(connection_id, "Strikes on record: " + result["strikes"] + "/3.");
        } else {
            send_client_message(connection_id, "Your record is clean.");
        }
        return;
    }

    // Failed.
    send_client_message(connection_id, "\x1b[1;31m\"You're not even close to baseline.\"\x1b[0m");
    if result["retired"] {
        send_client_message(connection_id, "\x1b[1;31mRETIREMENT ORDER ISSUED. You are escorted to recalibration. Everything dims for a long time.\x1b[0m");
        send_client_message(connection_id, "\x1b[31mAll attributes suppressed for 24 hours. Your record now carries a retirement order.\x1b[0m");
        broadcast_to_room(char.current_room_id, "\x1b[1;31mA flat voice announces: \"RETIREMENT ORDER — " + char.name + ".\"\x1b[0m", char.name);
    } else {
        send_client_message(connection_id, "\x1b[33mStrike " + result["strikes"] + "/3 recorded. No restoration. Retest in 4 hours.\x1b[0m");
    }
}
