// scripts/triggers/sector7_street_ambiance.rhai
// Ambient messages for Sector 7 Neon Streets

fn run_trigger(room_id, connection_id, context) {
    let messages = [
        "A neon sign flickers and buzzes overhead.",
        "Steam hisses from a grate in the sidewalk.",
        "Distant gunfire echoes from somewhere deeper in the sector.",
        "A stray cat darts between the legs of passersby.",
        "The acrid smell of ozone mixes with street food smoke.",
        "A holographic ad glitches, briefly showing corrupted data.",
        "Someone bumps past you without a word, eyes on the ground.",
        "Rain begins to drip from the tangle of wires overhead.",
        "A surveillance drone passes overhead, its red eye scanning."
    ];

    let idx = random_int(0, messages.len() - 1);
    send_client_message(connection_id, messages[idx]);

    return "continue";
}
