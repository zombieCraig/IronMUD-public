// scripts/lib/dg_olc.rhai
// Shared DG-trigger builder UX helpers used by medit/oedit/redit.
// Import with: import "dg_olc" as dg;
// Public entry: dg::handle(host_kind, host_id, display_id, action, rest, connection_id);
//   `host_id` is the entity UUID used for setter lookups.
//   `display_id` is the keyword/vnum the user typed; echoed back in help so
//   the printed lines stay re-typable.

fn handle(host_kind, host_id, display_id, action, rest, connection_id) {
    if action == "" {
        show_help(host_kind, display_id, connection_id);
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
    if act == "detach" { detach_instance(host_kind, host_id, rest, connection_id); return; }
    if act == "makeproto" { makeproto(host_kind, host_id, rest, connection_id); return; }
    if act == "proto" { proto_dispatch(host_kind, host_id, rest, connection_id); return; }

    show_help(host_kind, display_id, connection_id);
}

fn proto_dispatch(host_kind, host_id, rest, connection_id) {
    if rest.len() < 1 {
        send_client_message(connection_id,
            "Usage: trigger dg proto <new|view|edit|delete> <vnum> [...]");
        return;
    }
    let sub = rest[0].to_lower();
    let tail = [];
    let i = 1;
    while i < rest.len() { tail.push(rest[i]); i += 1; }
    if sub == "new"    { proto_new(host_kind, tail, connection_id); return; }
    if sub == "view"   { proto_view(tail, connection_id); return; }
    if sub == "edit"   { proto_edit(tail, connection_id); return; }
    if sub == "delete" { proto_delete(tail, connection_id); return; }
    send_client_message(connection_id,
        "Unknown 'trigger dg proto' subcommand. Use: new, view, edit, delete.");
}

fn show_help(host_kind, display_id, connection_id) {
    let cmd = if host_kind == "mobile" { "medit" }
              else if host_kind == "item" { "oedit" }
              else { "redit" };
    let id_part = if host_kind == "room" { "" } else { display_id + " " };
    let output = "DG Trigger commands (" + host_kind + "):\n"
        + "  " + cmd + " " + id_part + "trigger dg list           - List dg-bodied triggers\n"
        + "  " + cmd + " " + id_part + "trigger dg view <idx>     - Show body\n"
        + "  " + cmd + " " + id_part + "trigger dg add <type> [name]  - Append empty body, open editor\n"
        + "  " + cmd + " " + id_part + "trigger dg edit <idx>     - Edit body (saves to proto if attached)\n"
        + "  " + cmd + " " + id_part + "trigger dg attach <vnum>  - Attach an imported prototype by vnum\n"
        + "  " + cmd + " " + id_part + "trigger dg detach <idx>   - Break the proto link on instance (allow divergence)\n"
        + "  " + cmd + " " + id_part + "trigger dg makeproto <idx> <vnum>  - Promote host-local trigger to a proto\n"
        + "  " + cmd + " " + id_part + "trigger dg elevate <idx> [on|off]  - (admin) Bypass area gate on dangerous opcodes\n"
        + "  " + cmd + " " + id_part + "trigger dg protos         - List all registered prototypes\n"
        + "  " + cmd + " " + id_part + "trigger dg proto new <vnum> <flags> [name]  - Create empty proto, open editor\n"
        + "  " + cmd + " " + id_part + "trigger dg proto view <vnum>   - Show proto metadata + body\n"
        + "  " + cmd + " " + id_part + "trigger dg proto edit <vnum>   - Open proto body editor (refreshes siblings on save)\n"
        + "  " + cmd + " " + id_part + "trigger dg proto delete <vnum> - (admin) Orphan attached instances and delete proto\n"
        + "\nValid types for " + host_kind + ": " + valid_types_for(host_kind)
        + "\nSee docs/builder/dg-scripts.md#prototypes for the shared-script model.";
    send_client_message(connection_id, output);
}

fn valid_types_for(host_kind) {
    if host_kind == "mobile" {
        return "greet, attack, death, say, idle, always, flee, fight, hitprcnt, receive, bribe, load, command";
    }
    if host_kind == "item" {
        return "get, drop, use, examine, prompt, load, command";
    }
    return "enter, exit, look, periodic, time_change, weather_change, season_change, month_change, command";
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
    if rest.len() < 1 {
        send_client_message(connection_id,
            "Usage: trigger dg add <type> [name]\n"
            + "Valid types for " + host_kind + ": " + valid_types_for(host_kind));
        return;
    }
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
        send_client_message(connection_id,
            "Could not add trigger (unknown type '" + ttype + "').\n"
            + "Valid types for " + host_kind + ": " + valid_types_for(host_kind));
        return;
    }
    open_editor(host_kind, host_id, new_idx, connection_id);
}

fn edit_trigger(host_kind, host_id, rest, connection_id) {
    if rest.len() < 1 { send_client_message(connection_id, "Usage: trigger dg edit <index>"); return; }
    let idx = try_parse_int(rest[0]);
    if idx == () { send_client_message(connection_id, "Index must be a number."); return; }
    // If the trigger is tagged with a source_proto_vnum, route to the
    // proto editor: save-through with refresh sweep across siblings.
    // Untagged (host-local) triggers stay in the per-instance editor.
    let src = source_proto_for(host_kind, host_id, idx);
    if src != "" {
        send_client_message(connection_id,
            "This trigger is attached from proto '" + src + "'. Editing the body will\n"
            + "update the proto and refresh all attached instances. Use\n"
            + "`trigger dg detach " + idx + "` first if you want a one-off variant.");
        proto_open_editor(src, connection_id);
        return;
    }
    open_editor(host_kind, host_id, idx, connection_id);
}

fn source_proto_for(host_kind, host_id, idx) {
    if host_kind == "mobile" { return get_mobile_trigger_source_proto(host_id, idx); }
    if host_kind == "item"   { return get_item_trigger_source_proto(host_id, idx); }
    return get_room_trigger_source_proto(host_id, idx);
}

fn detach_instance(host_kind, host_id, rest, connection_id) {
    if rest.len() < 1 {
        send_client_message(connection_id, "Usage: trigger dg detach <index>");
        return;
    }
    let idx = try_parse_int(rest[0]);
    if idx == () { send_client_message(connection_id, "Index must be a number."); return; }
    let ok = if host_kind == "mobile" {
        dg_detach_mobile_trigger(host_id, idx)
    } else if host_kind == "item" {
        dg_detach_item_trigger(host_id, idx)
    } else {
        dg_detach_room_trigger(host_id, idx)
    };
    if ok {
        send_client_message(connection_id,
            "Detached trigger #" + idx + " from its proto. Future edits stay local.");
    } else {
        send_client_message(connection_id, "Could not detach trigger #" + idx + " (bad index?).");
    }
}

fn makeproto(host_kind, host_id, rest, connection_id) {
    if rest.len() < 2 {
        send_client_message(connection_id, "Usage: trigger dg makeproto <index> <vnum>");
        return;
    }
    let idx = try_parse_int(rest[0]);
    if idx == () { send_client_message(connection_id, "Index must be a number."); return; }
    let vnum = rest[1];
    let ok = if host_kind == "mobile" {
        dg_makeproto_from_mobile_trigger(host_id, idx, vnum)
    } else if host_kind == "item" {
        dg_makeproto_from_item_trigger(host_id, idx, vnum)
    } else {
        dg_makeproto_from_room_trigger(host_id, idx, vnum)
    };
    if ok {
        send_client_message(connection_id,
            "Promoted trigger #" + idx + " to proto vnum '" + vnum + "'.\n"
            + "Future edits via `trigger dg edit " + idx + "` will refresh all attached siblings.");
    } else {
        send_client_message(connection_id,
            "Could not promote trigger #" + idx + " to vnum '" + vnum + "'.\n"
            + "Common causes: vnum already exists, trigger has no dg_body, bad index.");
    }
}

fn proto_new(host_kind, rest, connection_id) {
    if rest.len() < 2 {
        send_client_message(connection_id,
            "Usage: trigger dg proto new <vnum> <flags> [name]\n"
            + "Flags: letter string (e.g. 'cw' = OnCommand+OnWear for items).");
        return;
    }
    let vnum = rest[0];
    let flags = rest[1];
    let name = "";
    let i = 2;
    while i < rest.len() {
        if name != "" { name += " "; }
        name += rest[i];
        i += 1;
    }
    let kind_key = if host_kind == "mobile" { "mob" }
                   else if host_kind == "item" { "obj" }
                   else { "room" };
    if !dg_proto_new(vnum, kind_key, flags, name) {
        send_client_message(connection_id,
            "Could not create proto '" + vnum + "'. The vnum may already exist.");
        return;
    }
    send_client_message(connection_id,
        "Created proto '" + vnum + "' (kind=" + kind_key + ", flags='" + flags + "'). Opening body editor.");
    proto_open_editor(vnum, connection_id);
}

fn proto_view(rest, connection_id) {
    if rest.len() < 1 { send_client_message(connection_id, "Usage: trigger dg proto view <vnum>"); return; }
    let vnum = rest[0];
    let proto = dg_proto_get(vnum);
    if proto == () {
        send_client_message(connection_id, "No proto with vnum '" + vnum + "'.");
        return;
    }
    let output = "DG Proto " + proto["vnum"] + " [" + proto["kind"] + " flags='" + proto["flags"] + "']\n"
        + "  Name:     " + proto["name"] + "\n"
        + "  Attached: " + proto["attached"] + " live instance(s)\n"
        + "--- body ---\n" + proto["body"];
    send_client_message(connection_id, output);
}

fn proto_edit(rest, connection_id) {
    if rest.len() < 1 { send_client_message(connection_id, "Usage: trigger dg proto edit <vnum>"); return; }
    let vnum = rest[0];
    if dg_proto_get(vnum) == () {
        send_client_message(connection_id, "No proto with vnum '" + vnum + "'.");
        return;
    }
    proto_open_editor(vnum, connection_id);
}

fn proto_delete(rest, connection_id) {
    if rest.len() < 1 { send_client_message(connection_id, "Usage: trigger dg proto delete <vnum>"); return; }
    let actor = get_character_data(connection_id);
    if actor == () || !actor.is_admin {
        send_client_message(connection_id, "Only admins can delete DG prototypes.");
        return;
    }
    let vnum = rest[0];
    let orphaned = dg_proto_delete(vnum);
    if orphaned < 0 {
        send_client_message(connection_id, "No proto with vnum '" + vnum + "'.");
        return;
    }
    send_client_message(connection_id,
        "Deleted proto '" + vnum + "' (" + orphaned + " attached instance(s) orphaned — bodies preserved).");
}

fn proto_open_editor(vnum, connection_id) {
    let proto = dg_proto_get(vnum);
    if proto == () {
        send_client_message(connection_id, "No proto with vnum '" + vnum + "'.");
        return;
    }
    set_olc_edit_proto_vnum(connection_id, vnum);
    set_olc_buffer_text(connection_id, proto["body"]);
    set_olc_mode(connection_id, "collecting_dg_proto_body");
    if try_enter_modern_editor(connection_id) {
        return;
    }
    send_client_message(connection_id,
        "Editing proto '" + vnum + "' body. Type lines, .h for help, .save to save, .abort to cancel.\n"
        + "Save will parse-check the body, then refresh all attached instances.");
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
    if try_enter_modern_editor(connection_id) {
        return;
    }
    send_client_message(connection_id,
        "Editing DG trigger #" + index + " body. Type lines, .h for help, .save to save, .abort to cancel.");
}
