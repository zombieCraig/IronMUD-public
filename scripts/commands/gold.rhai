// scripts/commands/gold.rhai
// Quick gold check - shows pocket gold and bank savings

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in.");
        return;
    }

    let pocket = get_character_gold(char.name);
    let bank = get_bank_gold(char.name);

    let pocket_str = format_gold(pocket);

    if bank > 0 {
        let bank_str = format_gold(bank);
        let total_str = format_gold(pocket + bank);
        let msg = "Gold on hand: " + pocket_str + "\n";
        msg += "Bank savings: " + bank_str + "\n";
        msg += "Total wealth: " + total_str;
        send_client_message(connection_id, color_or(ansi_yellow(msg), msg, connection_id));
    } else {
        let msg = "Gold on hand: " + pocket_str;
        send_client_message(connection_id, color_or(ansi_yellow(msg), msg, connection_id));
    }
}

fn format_gold(amount) {
    let str = "" + amount;
    if amount < 1000 { return str; }

    let result = "";
    let count = 0;
    for i in 0..str.len() {
        let idx = str.len() - 1 - i;
        if count > 0 && count % 3 == 0 {
            result = "," + result;
        }
        result = str.sub_string(idx, 1) + result;
        count += 1;
    }
    return result;
}
