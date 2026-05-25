// scripts/commands/mail.rhai
// In-game mail system for sending and receiving messages and packages

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

// Check if player is standing in a post office
fn in_post_office(char) {
    let room = get_room_data(char.current_room_id);
    if room == () { return false; }
    return room.flags.post_office;
}

// Postage for the current draft: stamp + per-item surcharge.
// god_mode pays nothing — matches give.rhai's builder bypass convention.
fn compute_postage(char, attachment_count) {
    if char.god_mode { return 0; }
    let total = get_stamp_price();
    if attachment_count > 0 {
        total += attachment_count * get_package_postage();
    }
    return total;
}

// Check if player can send mail (in post office + level requirement + can pay postage)
fn can_send_mail(connection_id, char, attachment_count) {
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
    let price = compute_postage(char, attachment_count);
    if char.gold < price {
        return #{ ok: false, error: "You need " + price + " gold to send mail. (You have: " + char.gold + ")" };
    }

    return #{ ok: true, price: price };
}

fn show_help(connection_id) {
    let help = "=== Mail System ===\n\n";
    help += "Commands:\n";
    help += "  mail             - Check for new mail\n";
    help += "  mail list        - List all messages\n";
    help += "  mail read <#>    - Read a message by number\n";
    help += "  mail send <player> <message> - Send a quick text-only message\n";
    help += "  mail compose <player>  - Write a multi-line message (supports packages)\n";
    help += "  mail reply <#>   - Reply to a message\n";
    help += "  mail claim <#>   - Collect items attached to a message (post office only)\n";
    help += "  mail delete <#>  - Delete a message (use 'mail delete <#> confirm' if it has unclaimed items)\n\n";
    help += "While composing:\n";
    help += "  @attach <item>   - Attach an item from your inventory (max " + get_max_attachments() + ")\n";
    help += "  @detach <#>      - Remove a pending attachment by its number\n";
    help += "  @                - Send the message\n";
    help += "  @@               - Cancel\n\n";
    help += "Postage:\n";
    help += "  - " + get_stamp_price() + " gold per message\n";
    help += "  - +" + get_package_postage() + " gold per attached item\n";
    help += "  - Effective level " + get_mail_level_requirement() + " required to send\n";
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

        let tag = "";
        if msg.attachment_count > 0 {
            tag = " [" + msg.attachment_count + " item" + plural(msg.attachment_count) + "]";
        }

        output += "[" + status + "] " + num + ". From " + msg.sender + " (" + time_str + ")" + tag + "\n";
        output += "       " + msg.preview + "\n";
        num += 1;
    }

    output += "\nUse 'mail read <#>' to read a message.";
    send_client_message(connection_id, output);
}

fn cmd_read(connection_id, char, num_str) {
    let num = try_parse_int(num_str);
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

    if msg.attachment_count > 0 {
        let attachments = get_mail_attachments_by_index(char.name, num);
        if attachments.len() > 0 {
            output += "\nAttached package:\n";
            for att in attachments {
                output += "  - " + att.short_desc + "\n";
            }
            output += "Use 'mail claim " + num + "' at a post office to collect these.\n---\n";
        }
    }

    output += "Commands: 'mail reply " + num + "' | 'mail delete " + num + "'";
    if msg.attachment_count > 0 {
        output += " | 'mail claim " + num + "'";
    }

    send_client_message(connection_id, output);
}

fn cmd_delete(connection_id, char, args) {
    let parts = args.split(" ");
    if parts.len() == 0 {
        send_client_message(connection_id, "Usage: mail delete <number> [confirm]");
        return;
    }
    let num = try_parse_int(parts[0]);
    if num == () || num < 1 {
        send_client_message(connection_id, "Usage: mail delete <number> [confirm]");
        return;
    }

    let msg = get_mail_by_index(char.name, num);
    if msg == () {
        send_client_message(connection_id, "Message #" + num + " not found.");
        return;
    }

    let has_confirm = parts.len() >= 2 && parts[1].to_lower() == "confirm";
    if msg.attachment_count > 0 && !has_confirm {
        send_client_message(connection_id, "Message #" + num + " has " + msg.attachment_count
            + " unclaimed item" + plural(msg.attachment_count)
            + ". Type 'mail delete " + num + " confirm' to destroy them.");
        return;
    }

    if delete_mail_by_index(char.name, num) {
        if msg.attachment_count > 0 {
            send_client_message(connection_id, "Message #" + num + " deleted (with " + msg.attachment_count + " unclaimed item" + plural(msg.attachment_count) + " destroyed).");
        } else {
            send_client_message(connection_id, "Message #" + num + " deleted.");
        }
    } else {
        send_client_message(connection_id, "Message #" + num + " not found.");
    }
}

fn cmd_claim(connection_id, char, num_str) {
    let num = try_parse_int(num_str);
    if num == () || num < 1 {
        send_client_message(connection_id, "Usage: mail claim <number>");
        return;
    }

    if !in_post_office(char) {
        send_client_message(connection_id, "You must be in a post office to collect packages.");
        return;
    }

    let msg = get_mail_by_index(char.name, num);
    if msg == () {
        send_client_message(connection_id, "Message #" + num + " not found.");
        return;
    }
    if msg.attachment_count <= 0 {
        send_client_message(connection_id, "That message has no items attached.");
        return;
    }

    let attachments = get_mail_attachments_by_index(char.name, num);
    let result = claim_mail_attachments_by_index(char.name, num);

    if !result.ok {
        send_client_message(connection_id, "Failed to collect package.");
        return;
    }

    let output = "You collect your package:\n";
    for att in attachments {
        output += "  - " + att.short_desc + "\n";
    }
    if result.missing > 0 {
        output += "(" + result.missing + " item" + plural(result.missing) + " were missing.)\n";
    }
    output += result.claimed + " item" + plural(result.claimed) + " added to your inventory.";
    send_client_message(connection_id, output);
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

    // Check if can send (no attachments for one-liner)
    let check = can_send_mail(connection_id, char, 0);
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
    let price = check.price;
    if price > 0 { add_character_gold(char.name, -price); }

    // Send the mail
    let result = send_mail(char.name, recipient, message);
    if result == "ok" {
        if price > 0 {
            send_client_message(connection_id, "Mail sent to " + recipient + ". (Cost: " + price + " gold)");
        } else {
            send_client_message(connection_id, "Mail sent to " + recipient + ".");
        }
        // Notify recipient if online
        notify_mail_received(recipient, char.name);
    } else {
        // Refund gold on failure
        if price > 0 { add_character_gold(char.name, price); }
        send_client_message(connection_id, "Failed to send mail: " + result);
    }
}

// Open compose mode against a recipient. Extras layout:
//   [0]      = recipient name
//   [1..]    = attached item UUIDs (strings)
fn enter_compose(connection_id, char, recipient) {
    set_olc_mode(connection_id, "mail_compose");
    set_olc_buffer(connection_id, []);
    set_olc_extra_keywords(connection_id, [recipient]);

    let price = get_stamp_price();
    send_client_message(connection_id, "Composing message to " + recipient + "... (Stamp: " + price + " gold; +" + get_package_postage() + " per attached item)\n");
    send_client_message(connection_id, "Enter your message (one line per entry).");
    send_client_message(connection_id, "  @attach <item>  - attach an item from your inventory");
    send_client_message(connection_id, "  @detach <#>     - remove an attachment");
    send_client_message(connection_id, "  @               - send  |  @@ - cancel");
}

fn cmd_reply(connection_id, char, num_str) {
    let num = try_parse_int(num_str);
    if num == () || num < 1 {
        send_client_message(connection_id, "Usage: mail reply <number>");
        return;
    }

    let msg = get_mail_by_index(char.name, num);
    if msg == () {
        send_client_message(connection_id, "Message #" + num + " not found.");
        return;
    }

    // Check if can send (assume no attachments yet)
    let check = can_send_mail(connection_id, char, 0);
    if !check.ok {
        send_client_message(connection_id, check.error);
        return;
    }

    enter_compose(connection_id, char, msg.sender);
}

fn cmd_compose(connection_id, char, recipient) {
    if recipient == "" {
        send_client_message(connection_id, "Usage: mail compose <player>");
        return;
    }

    // Check if can send
    let check = can_send_mail(connection_id, char, 0);
    if !check.ok {
        send_client_message(connection_id, check.error);
        return;
    }

    // Check recipient exists
    if !character_exists(recipient) {
        send_client_message(connection_id, "Character '" + recipient + "' does not exist.");
        return;
    }

    enter_compose(connection_id, char, recipient);
}

// Pull the current attachment id list out of the OLC extras (skipping slot 0).
fn get_pending_attachments(connection_id) {
    let extras = get_olc_extra_keywords(connection_id);
    let ids = [];
    if extras.len() <= 1 { return ids; }
    for i in 1..extras.len() {
        ids.push(extras[i]);
    }
    return ids;
}

// Re-pack the extras with [recipient] + ids.
fn set_pending_attachments(connection_id, recipient, ids) {
    let extras = [recipient];
    for id in ids { extras.push(id); }
    set_olc_extra_keywords(connection_id, extras);
}

fn handle_attach(args, connection_id, char) {
    let keyword = args;
    if keyword == "" {
        send_client_message(connection_id, "Usage: @attach <item keyword>");
        return;
    }

    let max = get_max_attachments();
    let extras = get_olc_extra_keywords(connection_id);
    let recipient = extras[0];
    let current = get_pending_attachments(connection_id);

    if current.len() >= max {
        send_client_message(connection_id, "You can attach at most " + max + " item" + plural(max) + " per package.");
        return;
    }

    let item = find_item_in_inventory(char.name, keyword);
    if item == () {
        send_client_message(connection_id, "You don't have '" + keyword + "' in your inventory.");
        return;
    }

    if item.flags.cursed && !char.god_mode {
        send_client_message(connection_id, "The postmaster recoils. " + item.short_desc + " is cursed and cannot be sent.");
        return;
    }
    if item.flags.no_drop && !char.god_mode {
        send_client_message(connection_id, "You can't part with " + item.short_desc + ".");
        return;
    }
    if item.flags.quest_item && !char.god_mode {
        send_client_message(connection_id, "You cannot mail quest items.");
        return;
    }

    let id_str = "" + item.id;
    // No duplicates (find_item_in_inventory always returns the same first match anyway).
    for existing in current {
        if existing == id_str {
            send_client_message(connection_id, "That item is already attached.");
            return;
        }
    }

    current.push(id_str);
    set_pending_attachments(connection_id, recipient, current);

    let total = compute_postage(char, current.len());
    send_client_message(connection_id, "Attached " + item.short_desc + " (" + current.len() + "/" + max + "). Postage so far: " + total + " gold.");
}

fn handle_detach(args, connection_id, char) {
    let extras = get_olc_extra_keywords(connection_id);
    let recipient = extras[0];
    let current = get_pending_attachments(connection_id);
    if current.len() == 0 {
        send_client_message(connection_id, "No attachments to detach.");
        return;
    }

    let num = try_parse_int(args);
    if num == () || num < 1 || num > current.len() {
        send_client_message(connection_id, "Usage: @detach <number 1-" + current.len() + ">");
        return;
    }

    let removed_id = current[num - 1];
    let kept = [];
    let i = 0;
    for id in current {
        if i != num - 1 { kept.push(id); }
        i += 1;
    }
    set_pending_attachments(connection_id, recipient, kept);

    let item = get_item_data(removed_id);
    let label = "attachment #" + num;
    if item != () { label = item.short_desc; }
    let total = compute_postage(char, kept.len());
    send_client_message(connection_id, "Removed " + label + ". (" + kept.len() + " remaining; postage: " + total + " gold)");
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
        // Cancel — no items were parked yet, so nothing to restore.
        clear_olc_buffer(connection_id);
        set_olc_extra_keywords(connection_id, []);
        set_olc_mode(connection_id, "");
        send_client_message(connection_id, "Mail cancelled.");
        return;
    }

    // @attach <keyword>
    if args.starts_with("@attach ") {
        let keyword = args.sub_string(8);
        handle_attach(keyword, connection_id, char);
        return;
    }
    if args == "@attach" {
        send_client_message(connection_id, "Usage: @attach <item keyword>");
        return;
    }

    // @detach <#>
    if args.starts_with("@detach ") {
        let rest = args.sub_string(8);
        handle_detach(rest, connection_id, char);
        return;
    }
    if args == "@detach" {
        let pending = get_pending_attachments(connection_id);
        send_client_message(connection_id, "Usage: @detach <number 1-" + pending.len() + ">");
        return;
    }

    if args == "@" {
        // Send the message
        let buffer = get_olc_buffer(connection_id);
        if buffer.len() == 0 {
            send_client_message(connection_id, "Cannot send empty message. Type '@@' to cancel.");
            return;
        }

        let attachments = get_pending_attachments(connection_id);

        // Re-check sending requirements (time may have passed)
        let check = can_send_mail(connection_id, char, attachments.len());
        if !check.ok {
            send_client_message(connection_id, check.error);
            return;
        }

        // Build message body
        let message = "";
        for i in 0..buffer.len() {
            if i > 0 { message += "\n"; }
            message += buffer[i];
        }

        // Deduct gold
        let price = check.price;
        if price > 0 { add_character_gold(char.name, -price); }

        // Send mail (with or without attachments)
        let result = "";
        if attachments.len() == 0 {
            result = send_mail(char.name, recipient, message);
        } else {
            result = send_mail_with_attachments(char.name, recipient, message, attachments);
        }

        clear_olc_buffer(connection_id);
        set_olc_extra_keywords(connection_id, []);
        set_olc_mode(connection_id, "");

        if result == "ok" {
            let cost_msg = "";
            if price > 0 { cost_msg = " (Cost: " + price + " gold)"; }
            if attachments.len() > 0 {
                send_client_message(connection_id, "Package sent to " + recipient + " with " + attachments.len() + " item" + plural(attachments.len()) + "." + cost_msg);
            } else {
                send_client_message(connection_id, "Mail sent to " + recipient + "." + cost_msg);
            }
            notify_mail_received(recipient, char.name);
        } else {
            // Refund on failure. Items are restored by the Rust side.
            if price > 0 { add_character_gold(char.name, price); }
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
    } else if subcmd == "claim" {
        cmd_claim(connection_id, char, subargs);
    } else if subcmd == "help" {
        show_help(connection_id);
    } else {
        send_client_message(connection_id, "Unknown mail command. Use 'mail help' for usage.");
    }
}
