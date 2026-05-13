// scripts/lib/dg_olc.rhai
// Shared DG-trigger builder UX helpers used by medit/oedit/redit.
// Import with: import "dg_olc" as dg;
// Public entry: dg::handle(host_kind, host_id, action, rest, connection_id);

fn handle(host_kind, host_id, action, rest, connection_id) {
    if action == "" {
        show_help(host_kind, host_id, connection_id);
        return;
    }
    let act = action.to_lower();

    if act == "list" { list_triggers(host_kind, host_id, connection_id); return; }
    if act == "view" { view_trigger(host_kind, host_id, rest, connection_id); return; }
    if act == "add"  { add_trigger(host_kind, host_id, rest, connection_id); return; }
    if act == "edit" { edit_trigger(host_kind, host_id, rest, connection_id); return; }
    if act == "attach" { attach_proto(host_kind, host_id, rest, connection_id); return; }
    if act == "protos" { protos_list(connection_id); return; }
    if act == "elevate" { elevate_trigger(host_kind, host_id, rest, connection_id); return; }

    show_help(host_kind, host_id, connection_id);
}

fn show_help(host_kind, host_id, connection_id) {
    let cmd = if host_kind == "mobile" { "medit" }
              else if host_kind == "item" { "oedit" }
              else { "redit" };
    let id_part = if host_kind == "room" { "" } else { host_id + " " };
    let output = "DG Trigger commands (" + host_kind + "):\n"
        + "  " + cmd + " " + id_part + "trigger dg list           - List dg-bodied triggers\n"
        + "  " + cmd + " " + id_part + "trigger dg view <idx>     - Show body\n"
        + "  " + cmd + " " + id_part + "trigger dg add <type> [name]  - Append empty body, open editor\n"
        + "  " + cmd + " " + id_part + "trigger dg edit <idx>     - Re-open the editor\n"
        + "  " + cmd + " " + id_part + "trigger dg attach <vnum>  - Attach an imported prototype by vnum\n"
        + "  " + cmd + " " + id_part + "trigger dg elevate <idx> [on|off]  - (admin) Bypass area gate on dangerous opcodes\n"
        + "  " + cmd + " " + id_part + "trigger dg protos         - List imported prototypes";
    send_client_message(connection_id, output);
}

fn triggers_for(host_kind, host_id) {
    if host_kind == "mobile" { return get_mobile_triggers(host_id); }
    if host_kind == "item"   { return get_item_triggers(host_id); }
    return get_room_triggers(host_id);
}

fn type_str(host_kind, t) {
    if host_kind == "mobile" { return get_mobile_trigger_type(t); }
    if host_kind == "item"   { return get_item_trigger_type(t); }
    return get_trigger_type(t);
}

fn body_for(host_kind, host_id, idx) {
    if host_kind == "mobile" { return get_mobile_trigger_dg_body(host_id, idx); }
    if host_kind == "item"   { return get_item_trigger_dg_body(host_id, idx); }
    return get_room_trigger_dg_body(host_id, idx);
}

fn name_for(host_kind, host_id, idx) {
    if host_kind == "mobile" { return get_mobile_trigger_dg_name(host_id, idx); }
    if host_kind == "item"   { return get_item_trigger_dg_name(host_id, idx); }
    return get_room_trigger_dg_name(host_id, idx);
}

fn author_for(host_kind, host_id, idx) {
    if host_kind == "mobile" { return get_mobile_trigger_authored_by(host_id, idx); }
    if host_kind == "item"   { return get_item_trigger_authored_by(host_id, idx); }
    return get_room_trigger_authored_by(host_id, idx);
}

fn elevated_for(host_kind, host_id, idx) {
    if host_kind == "mobile" { return get_mobile_trigger_elevated(host_id, idx); }
    if host_kind == "item"   { return get_item_trigger_elevated(host_id, idx); }
    return get_room_trigger_elevated(host_id, idx);
}

fn set_elevated_for(host_kind, host_id, idx, on) {
    if host_kind == "mobile" { return set_mobile_trigger_elevated(host_id, idx, on); }
    if host_kind == "item"   { return set_item_trigger_elevated(host_id, idx, on); }
    return set_room_trigger_elevated(host_id, idx, on);
}

fn list_triggers(host_kind, host_id, connection_id) {
    let triggers = triggers_for(host_kind, host_id);
    if triggers.len() == 0 {
        send_client_message(connection_id, "No triggers on this " + host_kind + ".");
        return;
    }
    let output = "DG triggers:\n";
    let i = 0;
    for t in triggers {
        let body = body_for(host_kind, host_id, i);
        let name = name_for(host_kind, host_id, i);
        let kind_tag = if body == "" { "[rhai]" } else { "[ dg ]" };
        let label = if name == "" { "(unnamed)" } else { name };
        let author = author_for(host_kind, host_id, i);
        let elev = elevated_for(host_kind, host_id, i);
        let suffix = "";
        if author != "" { suffix += "  by " + author; }
        if elev { suffix += " [elevated]"; }
        output += "  " + i.to_string() + ". " + kind_tag + " "
                + type_str(host_kind, t) + " - " + label + suffix + "\n";
        i += 1;
    }
    send_client_message(connection_id, output);
}

fn elevate_trigger(host_kind, host_id, rest, connection_id) {
    if rest.len() < 1 {
        send_client_message(connection_id, "Usage: trigger dg elevate <index> [on|off]  (admin only)");
        return;
    }
    let actor = get_character_data(connection_id);
    if actor == () || !actor.is_admin {
        send_client_message(connection_id, "Only admins can elevate DG triggers.");
        return;
    }
    let idx = try_parse_int(rest[0]);
    if idx == () { send_client_message(connection_id, "Index must be a number."); return; }
    let want_on = true;
    if rest.len() >= 2 {
        let arg = rest[1].to_lower();
        if arg == "off" || arg == "false" || arg == "no" || arg == "0" { want_on = false; }
    }
    if set_elevated_for(host_kind, host_id, idx, want_on) {
        let state = if want_on { "ON" } else { "OFF" };
        send_client_message(connection_id, "DG trigger #" + idx + " elevation: " + state);
    } else {
        send_client_message(connection_id, "Could not update trigger #" + idx + " (bad index?).");
    }
}

fn view_trigger(host_kind, host_id, rest, connection_id) {
    if rest.len() < 1 { send_client_message(connection_id, "Usage: trigger dg view <index>"); return; }
    let idx = try_parse_int(rest[0]);
    if idx == () { send_client_message(connection_id, "Index must be a number."); return; }
    let body = body_for(host_kind, host_id, idx);
    if body == "" { send_client_message(connection_id, "Trigger #" + idx + " has no DG body."); return; }
    send_client_message(connection_id, "--- DG body for trigger #" + idx + " ---\n" + body);
}

fn add_trigger(host_kind, host_id, rest, connection_id) {
    if rest.len() < 1 { send_client_message(connection_id, "Usage: trigger dg add <type> [name]"); return; }
    let ttype = rest[0];
    let name = "";
    if rest.len() > 1 {
        let i = 1;
        while i < rest.len() {
            if name != "" { name += " "; }
            name += rest[i];
            i += 1;
        }
    }
    let new_idx = -1;
    if host_kind == "mobile" { new_idx = add_mobile_dg_trigger(host_id, ttype, name); }
    else if host_kind == "item" { new_idx = add_item_dg_trigger(host_id, ttype, name); }
    else { new_idx = add_room_dg_trigger(host_id, ttype, name); }

    if new_idx < 0 {
        send_client_message(connection_id, "Could not add trigger (unknown type '" + ttype + "'?).");
        return;
    }
    open_editor(host_kind, host_id, new_idx, connection_id);
}

fn edit_trigger(host_kind, host_id, rest, connection_id) {
    if rest.len() < 1 { send_client_message(connection_id, "Usage: trigger dg edit <index>"); return; }
    let idx = try_parse_int(rest[0]);
    if idx == () { send_client_message(connection_id, "Index must be a number."); return; }
    open_editor(host_kind, host_id, idx, connection_id);
}

fn attach_proto(host_kind, host_id, rest, connection_id) {
    if rest.len() < 1 { send_client_message(connection_id, "Usage: trigger dg attach <vnum>"); return; }
    let vnum = rest[0];
    if attach_dg_trigger_proto(host_id, vnum) {
        send_client_message(connection_id, "Attached DG prototype " + vnum + ".");
    } else {
        send_client_message(connection_id, "Could not attach DG prototype " + vnum + " (kind mismatch or unknown vnum?).");
    }
}

fn protos_list(connection_id) {
    let protos = list_dg_trigger_protos();
    if protos.len() == 0 {
        send_client_message(connection_id, "No DG trigger prototypes registered. (Import a tbamud area first.)");
        return;
    }
    let output = "DG trigger prototypes (" + protos.len() + "):\n";
    for p in protos {
        output += "  " + p["vnum"] + "  [" + p["kind"] + " flags=" + p["flags"] + "] " + p["name"] + "\n";
    }
    send_client_message(connection_id, output);
}

fn open_editor(host_kind, host_id, index, connection_id) {
    if host_kind == "mobile" { set_olc_edit_mobile(connection_id, host_id); }
    else if host_kind == "item" { set_olc_edit_item(connection_id, host_id); }
    else { set_olc_edit_room(connection_id, host_id); }
    set_olc_edit_trigger(connection_id, host_kind, index);

    let body = body_for(host_kind, host_id, index);
    set_olc_buffer_text(connection_id, body);
    set_olc_mode(connection_id, "collecting_dg_body");
    send_client_message(connection_id,
        "Editing DG trigger #" + index + " body. Type lines, .h for help, .save to save, .abort to cancel.");
}
