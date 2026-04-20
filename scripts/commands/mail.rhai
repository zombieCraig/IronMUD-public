// scripts/commands/mail.rhai
// In-game mail system for sending and receiving messages

// Helper to get plural suffix
fn plural(n) {
    if n == 1 { return ""; }
    return "s";
}

// Format timestamp to human-readable date
fn format_timestamp(ts) {
    let now = get_current_time();
    let diff = now - ts;

    // Just now (< 60 seconds)
    if diff < 60 { return "just now"; }

    // Minutes ago
    if diff < 3600 {
        let mins = diff / 60;
        return "" + mins + " minute" + plural(mins) + " ago";
    }

    // Hours ago
    if diff < 86400 {
        let hours = diff / 3600;
        return "" + hours + " hour" + plural(hours) + " ago";
    }

    // Days ago
    let days = diff / 86400;
    return "" + days + " day" + plural(days) + " ago";
}

// Check if player can send mail (in post office + level requirement)
fn can_send_mail(connection_id, char) {
    let room = get_room_data(char.current_room_id);
    if room == () {
        return #{ ok: false, error: "Cannot determine your location." };
    }

    // Check post office flag
    if !room.flags.post_office {
        return #{ ok: false, error: "You must be in a post office to send mail." };
    }

    // Check effective level
    let eff_level = get_effective_level(char.name);
    let required = get_mail_level_requirement();
    if eff_level < required {
        return #{ ok: false, error: "You need an effective level of " + required + " to send mail. (Current: " + eff_level + ")" };
    }

    // Check gold
    let price = get_stamp_price();
    if char.gold < price {
        return #{ ok: false, error: "You need " + price + " gold to send mail. (You have: " + char.gold + ")" };
    }

    return #{ ok: true };
}

fn show_help(connection_id) {
    let help = "=== Mail System ===\n\n";
    help += "Commands:\n";
    help += "  mail             - Check for new mail\n";
    help += "  mail list        - List all messages\n";
    help += "  mail read <#>    - Read a message by number\n";
    help += "  mail send <player> <message> - Send a quick message\n";
    help += "  mail compose <player>  - Write a multi-line message\n";
    help += "  mail delete <#>  - Delete a message\n";
    help += "  mail reply <#>   - Reply to a message\n\n";
    help += "Sending mail requires:\n";
    help += "  - Being in a post office room\n";
    help += "  - Effective level " + get_mail_level_requirement() + " (sum of all skill levels)\n";
    help += "  - " + get_stamp_price() + " gold per message\n";
    send_client_message(connection_id, help);
}

fn cmd_check(connection_id, char) {
    let unread = get_unread_mail_count(char.name);
    let total = get_mailbox_size(char.name);

    if unread == 0 && total == 0 {
        send_client_message(connection_id, "Your mailbox is empty.");
        return;
    }
    if unread == 0 {
        send_client_message(connection_id, "You have no new mail. (" + total + " message" + plural(total) + " total)");
        return;
    }
    send_client_message(connection_id, "You have " + unread + " unread message" + plural(unread) + ". (" + total + " total)");
}

fn cmd_list(connection_id, char) {
    let mail_list = get_mail_list(char.name);

    if mail_list.len() == 0 {
        send_client_message(connection_id, "Your mailbox is empty.");
        return;
    }

    let max_size = get_max_mailbox_size();
    let output = "=== Mailbox (" + mail_list.len() + "/" + max_size + ") ===\n\n";

    let num = 1;
    for msg in mail_list {
        let status = " ";
        if !msg.read { status = "*"; }
        let time_str = format_timestamp(msg.sent_at);

        // Format: [*] 1. From Sender (2 hours ago)
        //            Preview of message...
        output += "[" + status + "] " + num + ". From " + msg.sender + " (" + time_str + ")\n";
        output += "       " + msg.preview + "\n";
        num += 1;
    }

    output += "\nUse 'mail read <#>' to read a message.";
    send_client_message(connection_id, output);
}

fn cmd_read(connection_id, char, num_str) {
    let num = parse_int(num_str);
    if num == () || num < 1 {
        send_client_message(connection_id, "Usage: mail read <number>");
        return;
    }

    let msg = get_mail_by_index(char.name, num);
    if msg == () {
        send_client_message(connection_id, "Message #" + num + " not found.");
        return;
    }

    // Mark as read
    mark_mail_read(msg.id);

    let time_str = format_timestamp(msg.sent_at);
    let output = "=== Message #" + num + " ===\n\n";
    output += "From: " + msg.sender + "\n";
    output += "Sent: " + time_str + "\n";
    output += "---\n";
    output += msg.body;
    output += "\n---\n";
    output += "Commands: 'mail reply " + num + "' | 'mail delete " + num + "'";

    send_client_message(connection_id, output);
}

fn cmd_delete(connection_id, char, num_str) {
    let num = parse_int(num_str);
    if num == () || num < 1 {
        send_client_message(connection_id, "Usage: mail delete <number>");
        return;
    }

    if delete_mail_by_index(char.name, num) {
        send_client_message(connection_id, "Message #" + num + " deleted.");
    } else {
        send_client_message(connection_id, "Message #" + num + " not found.");
    }
}

fn cmd_send(connection_id, char, args) {
    // Parse: <recipient> <message>
    let parts = args.split(" ");
    if parts.len() < 2 {
        send_client_message(connection_id, "Usage: mail send <player> <message>");
        return;
    }

    let recipient = parts[0];
    // Rejoin the rest as the message
    let message = "";
    for i in 1..parts.len() {
        if i > 1 { message += " "; }
        message += parts[i];
    }

    // Check if can send
    let check = can_send_mail(connection_id, char);
    if !check.ok {
        send_client_message(connection_id, check.error);
        return;
    }

    // Check recipient exists
    if !character_exists(recipient) {
        send_client_message(connection_id, "Character '" + recipient + "' does not exist.");
        return;
    }

    // Deduct gold
    let price = get_stamp_price();
    add_character_gold(char.name, -price);

    // Send the mail
    let result = send_mail(char.name, recipient, message);
    if result == "ok" {
        send_client_message(connection_id, "Mail sent to " + recipient + ". (Cost: " + price + " gold)");
        // Notify recipient if online
        notify_mail_received(recipient, char.name);
    } else {
        // Refund gold on failure
        add_character_gold(char.name, price);
        send_client_message(connection_id, "Failed to send mail: " + result);
    }
}

fn cmd_reply(connection_id, char, num_str) {
    let num = parse_int(num_str);
    if num == () || num < 1 {
        send_client_message(connection_id, "Usage: mail reply <number>");
        return;
    }

    let msg = get_mail_by_index(char.name, num);
    if msg == () {
        send_client_message(connection_id, "Message #" + num + " not found.");
        return;
    }

    // Check if can send
    let check = can_send_mail(connection_id, char);
    if !check.ok {
        send_client_message(connection_id, check.error);
        return;
    }

    // Enter compose mode with recipient pre-filled
    set_olc_mode(connection_id, "mail_compose");
    set_olc_buffer(connection_id, []);

    // Store recipient in a temporary buffer entry we'll use later
    // We'll use olc_extra_keywords to store metadata
    set_olc_extra_keywords(connection_id, [msg.sender, "Re: " + msg.body.split("\n")[0]]);

    let price = get_stamp_price();
    send_client_message(connection_id, "Composing reply to " + msg.sender + "... (Cost: " + price + " gold)\n");
    send_client_message(connection_id, "Enter your message (one line per entry). Type '@' on a line to send, '@@' to cancel.\n");
}

fn cmd_compose(connection_id, char, recipient) {
    if recipient == "" {
        send_client_message(connection_id, "Usage: mail compose <player>");
        return;
    }

    // Check if can send
    let check = can_send_mail(connection_id, char);
    if !check.ok {
        send_client_message(connection_id, check.error);
        return;
    }

    // Check recipient exists
    if !character_exists(recipient) {
        send_client_message(connection_id, "Character '" + recipient + "' does not exist.");
        return;
    }

    // Enter compose mode
    set_olc_mode(connection_id, "mail_compose");
    set_olc_buffer(connection_id, []);
    set_olc_extra_keywords(connection_id, [recipient]);

    let price = get_stamp_price();
    send_client_message(connection_id, "Composing message to " + recipient + "... (Cost: " + price + " gold)\n");
    send_client_message(connection_id, "Enter your message (one line per entry). Type '@' on a line to send, '@@' to cancel.\n");
}

fn handle_compose_mode(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        set_olc_mode(connection_id, "");
        return;
    }

    // Get recipient from stored keywords
    let keywords = get_olc_extra_keywords(connection_id);
    if keywords.len() == 0 {
        send_client_message(connection_id, "Error: No recipient set. Cancelling.");
        set_olc_mode(connection_id, "");
        return;
    }
    let recipient = keywords[0];

    if args == "@@" {
        // Cancel
        clear_olc_buffer(connection_id);
        set_olc_mode(connection_id, "");
        send_client_message(connection_id, "Mail cancelled.");
        return;
    }

    if args == "@" {
        // Send the message
        let buffer = get_olc_buffer(connection_id);
        if buffer.len() == 0 {
            send_client_message(connection_id, "Cannot send empty message. Type '@@' to cancel.");
            return;
        }

        // Re-check sending requirements (time may have passed)
        let check = can_send_mail(connection_id, char);
        if !check.ok {
            send_client_message(connection_id, check.error);
            return;
        }

        // Build message
        let message = "";
        for i in 0..buffer.len() {
            if i > 0 { message += "\n"; }
            message += buffer[i];
        }

        // Deduct gold
        let price = get_stamp_price();
        add_character_gold(char.name, -price);

        // Send mail
        let result = send_mail(char.name, recipient, message);

        clear_olc_buffer(connection_id);
        set_olc_mode(connection_id, "");

        if result == "ok" {
            send_client_message(connection_id, "Mail sent to " + recipient + ". (Cost: " + price + " gold)");
            notify_mail_received(recipient, char.name);
        } else {
            // Refund on failure
            add_character_gold(char.name, price);
            send_client_message(connection_id, "Failed to send mail: " + result);
        }
        return;
    }

    // Add line to buffer
    append_olc_buffer(connection_id, args);
}

fn run_command(args, connection_id) {
    // Check if in compose mode
    let mode = get_olc_mode(connection_id);
    if mode == "mail_compose" {
        handle_compose_mode(args, connection_id);
        return;
    }

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

    if subcmd == "" || subcmd == "check" {
        cmd_check(connection_id, char);
    } else if subcmd == "list" {
        cmd_list(connection_id, char);
    } else if subcmd == "read" {
        cmd_read(connection_id, char, subargs);
    } else if subcmd == "delete" || subcmd == "del" {
        cmd_delete(connection_id, char, subargs);
    } else if subcmd == "send" {
        cmd_send(connection_id, char, subargs);
    } else if subcmd == "compose" || subcmd == "write" {
        cmd_compose(connection_id, char, subargs);
    } else if subcmd == "reply" {
        cmd_reply(connection_id, char, subargs);
    } else if subcmd == "help" {
        show_help(connection_id);
    } else {
        send_client_message(connection_id, "Unknown mail command. Use 'mail help' for usage.");
    }
}
