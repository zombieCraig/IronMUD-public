// scripts/commands/bank.rhai
// Banking system - deposit, withdraw, and check balance from any bank/ATM

// Check if player can use banking (in bank room or room with ATM item)
fn can_use_bank(connection_id, char) {
    let room = get_room_data(char.current_room_id);
    if room == () {
        return #{ ok: false, error: "Cannot determine your location." };
    }

    // Check bank room flag
    if room.flags.bank {
        return #{ ok: true };
    }

    // Check for ATM item in room
    if room_has_atm(char.current_room_id) {
        return #{ ok: true };
    }

    return #{ ok: false, error: "You must be at a bank or ATM to use banking services." };
}

// Format gold amount with commas for readability
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

fn show_help(connection_id) {
    let help = "=== Banking System ===\n\n";
    help += "Personal account (per-character):\n";
    help += "  bank                       - Check your balances\n";
    help += "  bank balance               - Check your balances\n";
    help += "  bank deposit <amount|all>  - Deposit gold into your account\n";
    help += "  bank withdraw <amount|all> - Withdraw gold from your account\n\n";
    help += "Shared account (account-wide, every alt can access):\n";
    help += "  bank shared balance        - Check the shared balance\n";
    help += "  bank shared deposit <n|all>  - Move pocket gold into the shared pile\n";
    help += "  bank shared withdraw <n|all> - Move shared gold into your pocket\n\n";
    help += "  bank help                  - Show this help\n\n";
    help += "All bank actions require a bank or ATM.\n";
    send_client_message(connection_id, help);
}

fn cmd_balance(connection_id, char) {
    let bank = get_bank_gold(char.name);
    let pocket = get_character_gold(char.name);
    let acct_id = get_authenticated_account_id(connection_id);
    let shared_amt = 0;
    if acct_id != "" {
        shared_amt = account_shared_bank_gold(acct_id);
    }

    let msg = "=== Bank Account ===\n";
    msg += "Personal Balance: " + format_gold(bank) + " gold\n";
    msg += "Shared (account): " + format_gold(shared_amt) + " gold\n";
    msg += "Carried Gold:     " + format_gold(pocket) + " gold\n";
    msg += "Total Wealth:     " + format_gold(bank + shared_amt + pocket) + " gold";
    send_client_message(connection_id, msg);
}

fn cmd_shared_balance(connection_id, char) {
    let acct_id = get_authenticated_account_id(connection_id);
    if acct_id == "" {
        send_client_message(connection_id, "Your character isn't bound to an account.");
        return;
    }
    let shared_amt = account_shared_bank_gold(acct_id);
    send_client_message(connection_id, "Shared account balance: " + format_gold(shared_amt) + " gold");
}

fn cmd_shared_deposit(connection_id, char, amount_str) {
    let check = can_use_bank(connection_id, char);
    if !check.ok {
        send_client_message(connection_id, check.error);
        return;
    }
    let acct_id = get_authenticated_account_id(connection_id);
    if acct_id == "" {
        send_client_message(connection_id, "Your character isn't bound to an account.");
        return;
    }
    let pocket = get_character_gold(char.name);
    let amount = 0;
    if amount_str == "all" {
        amount = pocket;
        if amount <= 0 {
            send_client_message(connection_id, "You have no gold to deposit.");
            return;
        }
    } else {
        amount = parse_int(amount_str);
        if amount == () || amount <= 0 {
            send_client_message(connection_id, "Usage: bank shared deposit <amount|all>");
            return;
        }
    }
    if amount > pocket {
        send_client_message(connection_id, "You don't have that much gold. (You have: " + format_gold(pocket) + " gold)");
        return;
    }
    if transfer_pocket_to_shared_bank(char.name, amount) {
        let new_shared_amt = account_shared_bank_gold(acct_id);
        send_client_message(connection_id, "You deposit " + format_gold(amount) + " gold into the shared account.\nShared balance: " + format_gold(new_shared_amt) + " gold");
    } else {
        send_client_message(connection_id, "Failed to complete deposit.");
    }
}

fn cmd_shared_withdraw(connection_id, char, amount_str) {
    let check = can_use_bank(connection_id, char);
    if !check.ok {
        send_client_message(connection_id, check.error);
        return;
    }
    let acct_id = get_authenticated_account_id(connection_id);
    if acct_id == "" {
        send_client_message(connection_id, "Your character isn't bound to an account.");
        return;
    }
    let shared_amt = account_shared_bank_gold(acct_id);
    let amount = 0;
    if amount_str == "all" {
        amount = shared_amt;
        if amount <= 0 {
            send_client_message(connection_id, "The shared account is empty.");
            return;
        }
    } else {
        amount = parse_int(amount_str);
        if amount == () || amount <= 0 {
            send_client_message(connection_id, "Usage: bank shared withdraw <amount|all>");
            return;
        }
    }
    if amount > shared_amt {
        send_client_message(connection_id, "Insufficient shared funds. (Shared balance: " + format_gold(shared_amt) + " gold)");
        return;
    }
    if transfer_shared_bank_to_pocket(char.name, amount) {
        let new_shared_amt = account_shared_bank_gold(acct_id);
        let pocket = get_character_gold(char.name);
        send_client_message(connection_id, "You withdraw " + format_gold(amount) + " gold from the shared account.\nShared balance: " + format_gold(new_shared_amt) + " gold\nYou are now carrying " + format_gold(pocket) + " gold.");
    } else {
        send_client_message(connection_id, "Failed to complete withdrawal.");
    }
}

fn handle_shared(connection_id, char, subargs) {
    // Subcmd parsing for the `shared` arm.
    let parts = subargs.split(" ");
    let subcmd = "";
    if parts.len() > 0 { subcmd = parts[0].to_lower(); }
    let rest = "";
    if parts.len() > 1 {
        for i in 1..parts.len() {
            if i > 1 { rest += " "; }
            rest += parts[i];
        }
    }
    if subcmd == "" || subcmd == "balance" || subcmd == "bal" {
        cmd_shared_balance(connection_id, char);
        return;
    }
    if subcmd == "deposit" || subcmd == "dep" {
        cmd_shared_deposit(connection_id, char, rest);
        return;
    }
    if subcmd == "withdraw" || subcmd == "with" {
        cmd_shared_withdraw(connection_id, char, rest);
        return;
    }
    send_client_message(connection_id, "Usage: bank shared balance | deposit <n|all> | withdraw <n|all>");
}

fn cmd_deposit(connection_id, char, amount_str) {
    // Check bank access
    let check = can_use_bank(connection_id, char);
    if !check.ok {
        send_client_message(connection_id, check.error);
        return;
    }

    let pocket = get_character_gold(char.name);

    // Parse amount
    let amount = 0;
    if amount_str == "all" {
        amount = pocket;
        if amount <= 0 {
            send_client_message(connection_id, "You have no gold to deposit.");
            return;
        }
    } else {
        amount = parse_int(amount_str);
        if amount == () || amount <= 0 {
            send_client_message(connection_id, "Usage: bank deposit <amount|all>");
            return;
        }
    }

    // Check funds
    if amount > pocket {
        send_client_message(connection_id, "You don't have that much gold. (You have: " + format_gold(pocket) + " gold)");
        return;
    }

    // Transfer
    if transfer_to_bank(char.name, amount) {
        let new_bank = get_bank_gold(char.name);
        send_client_message(connection_id, "You deposit " + format_gold(amount) + " gold.\nAccount balance: " + format_gold(new_bank) + " gold");
    } else {
        send_client_message(connection_id, "Failed to complete deposit.");
    }
}

fn cmd_withdraw(connection_id, char, amount_str) {
    // Check bank access
    let check = can_use_bank(connection_id, char);
    if !check.ok {
        send_client_message(connection_id, check.error);
        return;
    }

    let bank = get_bank_gold(char.name);

    // Parse amount
    let amount = 0;
    if amount_str == "all" {
        amount = bank;
        if amount <= 0 {
            send_client_message(connection_id, "Your account is empty.");
            return;
        }
    } else {
        amount = parse_int(amount_str);
        if amount == () || amount <= 0 {
            send_client_message(connection_id, "Usage: bank withdraw <amount|all>");
            return;
        }
    }

    // Check funds
    if amount > bank {
        send_client_message(connection_id, "Insufficient funds. (Account balance: " + format_gold(bank) + " gold)");
        return;
    }

    // Transfer
    if transfer_from_bank(char.name, amount) {
        let new_bank = get_bank_gold(char.name);
        let pocket = get_character_gold(char.name);
        send_client_message(connection_id, "You withdraw " + format_gold(amount) + " gold.\nAccount balance: " + format_gold(new_bank) + " gold\nYou are now carrying " + format_gold(pocket) + " gold.");
    } else {
        send_client_message(connection_id, "Failed to complete withdrawal.");
    }
}

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    // Parse subcommand
    let parts = args.split(" ");
    let subcmd = "";
    if parts.len() > 0 { subcmd = parts[0].to_lower(); }

    // Get remaining args
    let subargs = "";
    if parts.len() > 1 {
        for i in 1..parts.len() {
            if i > 1 { subargs += " "; }
            subargs += parts[i];
        }
    }

    if subcmd == "" || subcmd == "balance" || subcmd == "bal" {
        // Check bank access for balance too (optional - could allow checking balance anywhere)
        let check = can_use_bank(connection_id, char);
        if !check.ok {
            send_client_message(connection_id, check.error);
            return;
        }
        cmd_balance(connection_id, char);
    } else if subcmd == "deposit" || subcmd == "dep" {
        cmd_deposit(connection_id, char, subargs);
    } else if subcmd == "withdraw" || subcmd == "with" {
        cmd_withdraw(connection_id, char, subargs);
    } else if subcmd == "shared" {
        handle_shared(connection_id, char, subargs);
    } else if subcmd == "help" {
        show_help(connection_id);
    } else {
        send_client_message(connection_id, "Unknown bank command. Use 'bank help' for usage.");
    }
}
