// scripts/triggers/demo_ambiance.rhai
// Generic periodic ambient message trigger driven by args.
//
// Usage: redit trigger add periodic demo_ambiance
//        redit trigger args 0 "A bird sings|Water drips|Wind howls"
//        redit trigger interval 0 45
//        redit trigger chance 0 40
//
// Pipe-separated messages in the first arg. Picks one at random.

fn run_trigger(room_id, connection_id, context) {
    let args = context.args;
    if args.len() == 0 { return "continue"; }

    let messages_str = args[0];
    let messages = messages_str.split("|");
    if messages.len() == 0 { return "continue"; }

    let idx = random_int(0, messages.len() - 1);
    send_client_message(connection_id, messages[idx]);

    return "continue";
}
