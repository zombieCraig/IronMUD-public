// scripts/triggers/sector7_plaza_ambiance.rhai
// Ambient messages for Sector 7 Main Plaza

fn run_trigger(room_id, connection_id, context) {
    let messages = [
        "A hoverbike weaves through the crowd, its engine whining.",
        "The LED screens flicker briefly, displaying static before resuming.",
        "A courier drone buzzes overhead, its cargo light blinking green.",
        "Somewhere in the crowd, a vendor shouts about discount cybernetics.",
        "The decommissioned mech groans as wind passes through its joints.",
        "A group of Chrome Fang gangers swagger through the plaza.",
        "The smell of grilled synth-meat drifts across the plaza.",
        "A NovaCorp advertisement jingle plays from a nearby speaker.",
        "Two corporate workers hurry past, clutching briefcases nervously."
    ];

    let idx = random_int(0, messages.len() - 1);
    send_client_message(connection_id, messages[idx]);

    return "continue";
}
