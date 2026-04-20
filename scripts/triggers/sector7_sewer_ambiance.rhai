// scripts/triggers/sector7_sewer_ambiance.rhai
// Ambient messages for Sector 7 Sewers and Undercity

fn run_trigger(room_id, connection_id, context) {
    let messages = [
        "Water drips steadily from the ceiling, echoing in the darkness.",
        "Something scurries in the shadows -- rats, probably.",
        "A distant rumble passes through the walls -- heavy machinery somewhere above.",
        "The lights flicker, plunging everything into darkness for a heartbeat.",
        "A foul-smelling breeze wafts from deeper in the tunnels.",
        "You hear muffled voices somewhere in the distance.",
        "A pipe groans and releases a jet of steam with a sharp hiss.",
        "The bioluminescent fungi on the wall pulse faintly.",
        "Somewhere, metal scrapes against concrete -- footsteps or something worse."
    ];

    let idx = random_int(0, messages.len() - 1);
    send_client_message(connection_id, messages[idx]);

    return "continue";
}
