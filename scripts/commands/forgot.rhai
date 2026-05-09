// scripts/commands/forgot.rhai
// Pre-auth password recovery. Looks up the account by verified email,
// generates a random temp password, hashes it onto the account + all linked
// characters, marks them must_change_password, and emails the temp password.
// All replies use a single generic message so a flooder can't enumerate
// which emails are registered.

fn run_command(args, connection_id) {
    if !is_email_verification_required() {
        send_client_message(connection_id, "Password recovery by email is not enabled on this server.");
        return;
    }

    let email = args.trim();
    if email == "" {
        send_client_message(connection_id, "Usage: forgot <email>");
        return;
    }
    if !is_valid_email_format(email) {
        send_client_message(connection_id, "That doesn't look like an email address. Usage: forgot <email>");
        return;
    }

    let generic = "If an account with that email exists, a temporary password has been sent. Check your inbox.";

    // Per-IP email-send cap. Checked before the account lookup so a flooder
    // can't probe many addresses to bypass the per-account throttle. Always
    // returns the generic line so the cap state isn't an enumeration oracle.
    if is_email_send_throttled(connection_id) {
        // Burn equivalent Argon2 work so per-IP throttled requests don't
        // return faster than legitimate ones — closes a timing oracle.
        do_dummy_password_work();
        send_client_message(connection_id, generic);
        return;
    }

    let account_id = find_verified_account_id_by_email(email);
    if account_id == "" {
        // No verified account on file. Burn Argon2-equivalent work so this
        // branch takes about as long as the success branch — without it,
        // a fast "no" vs slow "yes" is an enumeration oracle.
        do_dummy_password_work();
        send_client_message(connection_id, generic);
        return;
    }

    let throttle = can_request_password_reset(account_id);
    if throttle != 0 {
        // Throttle hit (60s spacing, hourly cap, or daily cap). Fold all
        // codes into the same generic reply, with dummy work to keep the
        // timing similar to a real send.
        do_dummy_password_work();
        send_client_message(connection_id, generic);
        return;
    }

    if issue_password_reset(account_id) {
        record_email_send(connection_id);
    }
    send_client_message(connection_id, generic);
}
