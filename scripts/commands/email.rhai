// scripts/commands/email.rhai
// Self-service account email management for a logged-in player. Lets a player
// view, set, verify, and resend the verification code for the email on their
// account — the address used by `forgot` for password recovery. Modeled on
// password.rhai (account-level self-service) and mirrors the verification logic
// in create.rhai, but stays a plain command since the player is already logged
// in (no OLC mode needed).
//
// Sends are gated by the same throttles the create/resend flow uses so this
// can't be abused to run up the SMTP bill or block other registrations:
//   - per-IP cap        (is_email_send_throttled)
//   - per-account cooldown (can_resend_verification_code: 60s / 3-hour / 5-day)
//   - global daily/monthly budget (inside send_verification_code)

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to manage your email.");
        return;
    }

    let account_id = get_authenticated_account_id(connection_id);
    if account_id == "" {
        send_client_message(connection_id, "Your session is missing account context. Try `quit` and log in again.");
        return;
    }

    // Gate on the same master switch as forgot.rhai. When email isn't enabled
    // on the server, the normal flow doesn't let you register an address at all;
    // this command must not become a side door around that.
    if !is_email_verification_required() {
        send_client_message(connection_id, "Email features are not enabled on this server.");
        return;
    }

    let parts = args.split(" ");
    let sub = if parts.len() > 0 && parts[0] != "" { parts[0].to_lower() } else { "" };

    if sub == "" || sub == "show" || sub == "status" {
        show_email_status(connection_id, account_id);
        return;
    }

    if sub == "set" {
        let address = if parts.len() >= 2 && parts[1] != "" { parts[1] } else { "" };
        handle_email_set(connection_id, account_id, address);
        return;
    }

    if sub == "verify" {
        let code = if parts.len() >= 2 && parts[1] != "" { parts[1] } else { "" };
        handle_email_verify(connection_id, account_id, code);
        return;
    }

    if sub == "resend" {
        handle_email_resend(connection_id, account_id);
        return;
    }

    send_client_message(connection_id,
        "Usage:\n" +
        "  email                 - show your current email and its status\n" +
        "  email set <address>   - set/change your email (sends a verification code)\n" +
        "  email verify <code>   - confirm the code emailed to you\n" +
        "  email resend          - send a fresh verification code");
}

fn show_email_status(connection_id, account_id) {
    let account = get_account_by_id(account_id);
    if account == () {
        send_client_message(connection_id, "Could not load your account.");
        return;
    }
    let email = account.email;
    if email == "" {
        send_client_message(connection_id,
            "No email is on file for your account.\n" +
            "Set one with `email set <address>` so you can recover a forgotten password.");
        return;
    }
    let status = if account.email_verified { "verified" } else { "NOT verified" };
    let out = "Account email: " + email + "  (" + status + ")\n";
    if !account.email_verified {
        out += "Confirm it with `email verify <code>` (check your inbox), or `email resend` for a new code.";
    } else {
        out += "You can recover a forgotten password with the `forgot` command at the login screen.";
    }
    send_client_message(connection_id, out);
}

fn handle_email_set(connection_id, account_id, address) {
    if address == "" {
        send_client_message(connection_id, "Usage: email set <address>");
        return;
    }
    if !is_valid_email_format(address) {
        send_client_message(connection_id, "That doesn't look like an email address. Usage: email set <address>");
        return;
    }
    if is_disposable_email(address) {
        send_client_message(connection_id, "Disposable email addresses aren't allowed. Use a real inbox you can receive password resets at.");
        return;
    }

    // Reject an address already tied to a different account (normalized form,
    // so dot/+tag tricks don't slip a duplicate through). Re-setting your own
    // address is allowed.
    let existing = find_account_id_by_normalized_email(address);
    if existing != "" && existing != account_id {
        send_client_message(connection_id, "That email is already registered to another account.");
        return;
    }

    // Cooldown gates — refuse before spending an email.
    if !check_send_allowed(connection_id, account_id) {
        return;
    }

    set_account_email(account_id, address);
    if !send_verification_code(account_id, address) {
        send_client_message(connection_id, "Couldn't send a verification code right now. Email may not be configured on this server — contact an administrator.");
        return;
    }
    record_email_send(connection_id);
    send_client_message(connection_id,
        "A 6-digit verification code has been sent to " + address + ".\n" +
        "Confirm it with `email verify <code>`.");
}

fn handle_email_verify(connection_id, account_id, code) {
    if code == "" {
        send_client_message(connection_id, "Usage: email verify <code>");
        return;
    }
    let result = verify_account_code(account_id, code);
    if result == 0 {
        send_client_message(connection_id, "Email verified. You can now recover a forgotten password with `forgot`.");
        return;
    }
    if result == 1 {
        send_client_message(connection_id, "No verification code is outstanding. Use `email set <address>` or `email resend` first.");
        return;
    }
    if result == 2 {
        send_client_message(connection_id, "That code has expired. Use `email resend` to get a fresh one.");
        return;
    }
    send_client_message(connection_id, "That code doesn't match. Check your email and try again, or `email resend`.");
}

fn handle_email_resend(connection_id, account_id) {
    let account = get_account_by_id(account_id);
    if account == () || account.email == "" {
        send_client_message(connection_id, "No email is on file. Use `email set <address>` first.");
        return;
    }
    if account.email_verified {
        send_client_message(connection_id, "Your email is already verified — no code needed.");
        return;
    }
    if !check_send_allowed(connection_id, account_id) {
        return;
    }
    if !send_verification_code(account_id, account.email) {
        send_client_message(connection_id, "Couldn't send a verification code right now. Contact an administrator.");
        return;
    }
    record_email_send(connection_id);
    send_client_message(connection_id, "A fresh code has been sent to " + account.email + ". Confirm it with `email verify <code>`.");
}

// Shared cooldown check for any send. Returns true when a send is allowed;
// otherwise sends the appropriate refusal and returns false.
fn check_send_allowed(connection_id, account_id) {
    let gate = can_resend_verification_code(account_id);
    if gate == 1 {
        send_client_message(connection_id, "Too soon since the last code. Wait a minute and try again.");
        return false;
    }
    if gate == 2 {
        send_client_message(connection_id, "You've requested too many codes this hour. Try again later.");
        return false;
    }
    if gate == 3 {
        send_client_message(connection_id, "You've requested too many codes today. Try again tomorrow.");
        return false;
    }
    if is_email_send_throttled(connection_id) {
        send_client_message(connection_id, "Too many recent emails from your address. Try again later.");
        return false;
    }
    true
}
