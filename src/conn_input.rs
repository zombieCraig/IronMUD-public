//! Per-mode input handlers for `handle_connection`.
//!
//! Each function handles one line (or keypress) while the session is in a
//! particular mode — an OLC text editor, the AI confirm prompt, a script
//! wizard — and was lifted verbatim out of the connection loop. `ConnCx`
//! carries the handful of loop locals they share.

use super::*;

/// The connection-loop locals the input handlers read.
#[derive(Clone, Copy)]
pub(crate) struct ConnCx<'a> {
    pub connection_id: ConnectionId,
    pub connections: &'a SharedConnections,
    pub state: &'a SharedState,
    pub tx_client: &'a session::ClientSender,
}

/// Tab pressed: complete the current input line in place.
pub(crate) fn tab_completion(cx: &ConnCx<'_>) {
    let ConnCx {
        connection_id,
        connections,
        state,
        tx_client,
        ..
    } = *cx;

    // Get current input buffer and cursor position
    let (current_input, cursor_pos, window_width, colors_enabled) = {
        let conns = connections.lock();
        conns
            .get(&connection_id)
            .map(|s| {
                (
                    s.input_buffer.clone(),
                    s.cursor_pos,
                    s.telnet_state.window_width,
                    s.colors_enabled,
                )
            })
            .unwrap_or_default()
    };

    // Gather completion data
    let (
        available_commands,
        room_vnums,
        item_vnums,
        mobile_vnums,
        area_prefixes,
        recipe_vnums,
        transport_vnums,
        property_template_vnums,
        shop_preset_vnums,
        plant_vnums,
        quest_vnums,
        spell_names,
        language_keys,
        online_players,
        mobs_in_room,
        items_in_room,
        class_ids,
        race_ids,
        achievement_keys,
        custom_skill_keys,
        faction_keys,
        social_commands,
        has_builder_access,
    ) = {
        // Snapshot everything needed from the session and release the
        // Connections lock BEFORE taking World. Lock order rule: never
        // acquire World while holding Connections (ABBA deadlock).
        let (is_logged_in, is_builder, is_admin, current_room_id, online_players) = {
            let conns = connections.lock();
            let me = conns.get(&connection_id).and_then(|s| s.character.as_ref());
            let online: Vec<String> = conns
                .values()
                .filter_map(|s| s.character.as_ref())
                .map(|c| c.name.clone())
                .collect();
            (
                me.is_some(),
                me.is_some_and(|c| c.is_builder),
                me.is_some_and(|c| c.is_admin),
                me.map(|c| c.current_room_id),
                online,
            )
        };

        let world = state.lock();

        let visible: Vec<(&String, &CommandMeta)> = world
            .command_metadata
            .iter()
            .filter(|(_, meta)| match meta.access.as_str() {
                "guest" => true,
                "user" => is_logged_in,
                "builder" => is_builder || is_admin,
                "admin" => is_admin,
                _ => true,
            })
            .collect();

        let mut available_commands: Vec<String> = visible.iter().map(|(name, _)| (*name).clone()).collect();

        // Socials are virtual `command_metadata` entries tagged
        // `kind = "social"` (see `register_socials_in_command_metadata`).
        // Sorted so the completer can binary-search it when tinting
        // the candidate list.
        let mut social_commands: Vec<String> = visible
            .iter()
            .filter(|(_, meta)| meta.kind.as_deref() == Some("social"))
            .map(|(name, _)| name.to_lowercase())
            .collect();
        social_commands.sort();

        // Append the player's current room contextual verbs (deduped).
        // Runtime dispatch is still handled by DG OnCommand triggers;
        // this just lets the verbs surface in TAB completion.
        if let Some(room_id) = current_room_id {
            if room_id != Uuid::nil() {
                if let Ok(Some(room)) = world.db.get_room_data(&room_id) {
                    for cc in &room.contextual_commands {
                        if !available_commands.iter().any(|c| c == &cc.verb) {
                            available_commands.push(cc.verb.clone());
                        }
                    }
                }
            }
        }

        // Get vnums from database
        let room_vnums: Vec<String> = world
            .db
            .list_all_rooms()
            .unwrap_or_default()
            .iter()
            .filter_map(|r| r.vnum.clone())
            .collect();

        let item_vnums: Vec<String> = world
            .db
            .list_all_items()
            .unwrap_or_default()
            .iter()
            .filter(|i| i.is_prototype)
            .filter_map(|i| i.vnum.clone())
            .collect();

        let mobile_vnums: Vec<String> = world
            .db
            .list_all_mobiles()
            .unwrap_or_default()
            .iter()
            .filter(|m| m.is_prototype)
            .filter_map(|m| Some(m.vnum.clone()))
            .filter(|v| !v.is_empty())
            .collect();

        let area_prefixes: Vec<String> = world
            .db
            .list_all_areas()
            .unwrap_or_default()
            .iter()
            .map(|a| a.prefix.clone())
            .collect();

        let recipe_vnums: Vec<String> = world.recipes.keys().cloned().collect();

        let transport_vnums: Vec<String> = world
            .db
            .list_all_transports()
            .unwrap_or_default()
            .iter()
            .filter_map(|t| t.vnum.clone())
            .collect();

        let property_template_vnums: Vec<String> = world
            .db
            .list_all_property_templates()
            .unwrap_or_default()
            .iter()
            .map(|t| t.vnum.clone())
            .collect();

        let shop_preset_vnums: Vec<String> = world
            .db
            .list_all_shop_presets()
            .unwrap_or_default()
            .iter()
            .map(|p| p.vnum.clone())
            .collect();

        let plant_vnums: Vec<String> = world
            .db
            .list_all_plant_prototypes()
            .unwrap_or_default()
            .iter()
            .filter_map(|p| p.vnum.clone())
            .collect();

        let quest_vnums: Vec<String> = world
            .db
            .list_all_quests()
            .unwrap_or_default()
            .iter()
            .map(|q| q.vnum.clone())
            .collect();

        let spell_names: Vec<String> = world.spell_definitions.values().map(|s| s.name.clone()).collect();

        let language_keys: Vec<String> = world.language_definitions.keys().cloned().collect();

        // What is standing and lying in this player's current room:
        // mobs for `talk <mob>`, both for `build audit mob|item <kw>`,
        // which resolves a keyword against the room before the world.
        let (mobs_in_room, items_in_room): (Vec<String>, Vec<String>) = match current_room_id {
            Some(room_id) if room_id != Uuid::nil() => {
                let mobs = world
                    .db
                    .get_mobiles_in_room(&room_id)
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|m| !m.is_prototype)
                    .flat_map(|m| {
                        let mut keys = m.keywords.clone();
                        keys.push(m.name);
                        keys
                    })
                    .collect();
                let items = world
                    .db
                    .get_items_in_room(&room_id)
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|i| !i.is_prototype && !i.flags.buried)
                    .flat_map(|i| {
                        let mut keys = i.keywords.clone();
                        keys.push(i.name);
                        keys
                    })
                    .collect();
                (mobs, items)
            }
            _ => (Vec::new(), Vec::new()),
        };

        let has_builder_access = is_builder || is_admin;

        let class_ids: Vec<String> = world.class_definitions.keys().cloned().collect();
        let race_ids: Vec<String> = world.race_definitions.keys().cloned().collect();
        let achievement_keys: Vec<String> = world.achievement_definitions.keys().cloned().collect();
        let custom_skill_keys: Vec<String> = world.custom_skill_definitions.keys().cloned().collect();
        let faction_keys: Vec<String> = world.faction_definitions.keys().cloned().collect();

        (
            available_commands,
            room_vnums,
            item_vnums,
            mobile_vnums,
            area_prefixes,
            recipe_vnums,
            transport_vnums,
            property_template_vnums,
            shop_preset_vnums,
            plant_vnums,
            quest_vnums,
            spell_names,
            language_keys,
            online_players,
            mobs_in_room,
            items_in_room,
            class_ids,
            race_ids,
            achievement_keys,
            custom_skill_keys,
            faction_keys,
            social_commands,
            has_builder_access,
        )
    };

    // Perform completion
    let result = completion::complete(
        &current_input,
        cursor_pos,
        &completion::CompletionData {
            available_commands: &available_commands,
            room_vnums: &room_vnums,
            item_vnums: &item_vnums,
            mobile_vnums: &mobile_vnums,
            area_prefixes: &area_prefixes,
            recipe_vnums: &recipe_vnums,
            transport_vnums: &transport_vnums,
            property_template_vnums: &property_template_vnums,
            shop_preset_vnums: &shop_preset_vnums,
            plant_vnums: &plant_vnums,
            quest_vnums: &quest_vnums,
            spell_names: &spell_names,
            language_keys: &language_keys,
            online_players: &online_players,
            mobs_in_room: &mobs_in_room,
            items_in_room: &items_in_room,
            class_ids: &class_ids,
            race_ids: &race_ids,
            achievement_keys: &achievement_keys,
            custom_skill_keys: &custom_skill_keys,
            faction_keys: &faction_keys,
            social_commands: &social_commands,
            is_builder: has_builder_access,
        },
    );

    if result.is_empty() {
        // No completions - just beep or do nothing
        return;
    }

    if result.is_unique() {
        // Single match - auto-complete
        let completed = &result.completions[0];
        let to_add = &completed[result.partial.len()..];

        let new_input = {
            let mut conns = connections.lock();
            if let Some(session) = conns.get_mut(&connection_id) {
                // Find byte offset for cursor position (char-based)
                let byte_pos = session
                    .input_buffer
                    .char_indices()
                    .nth(session.cursor_pos)
                    .map(|(i, _)| i)
                    .unwrap_or(session.input_buffer.len());

                let at_end = byte_pos >= session.input_buffer.len();

                // Insert completion at cursor position
                session.input_buffer.insert_str(byte_pos, to_add);
                session.cursor_pos += to_add.chars().count();

                // Add trailing space only if cursor was at end of buffer
                if at_end {
                    session.input_buffer.push(' ');
                    session.cursor_pos += 1;
                }

                session.input_buffer.clone()
            } else {
                return;
            }
        };

        // Redraw the full line (handles cursor positioning)
        let cursor_pos = {
            let conns = connections.lock();
            conns.get(&connection_id).map(|s| s.cursor_pos).unwrap_or(0)
        };
        let redraw = telnet::redraw_input_line("> ", &new_input, cursor_pos);
        let _ = tx_client.send(String::from_utf8_lossy(&redraw).into_owned());
    } else {
        // Multiple matches - show list and complete common prefix
        let display = completion::format_completions(&result, window_width, colors_enabled);

        // Complete to common prefix if it's longer than what we have
        let to_add = if result.common_prefix.len() > result.partial.len() {
            &result.common_prefix[result.partial.len()..]
        } else {
            ""
        };

        if !to_add.is_empty() {
            let mut conns = connections.lock();
            if let Some(session) = conns.get_mut(&connection_id) {
                let byte_pos = session
                    .input_buffer
                    .char_indices()
                    .nth(session.cursor_pos)
                    .map(|(i, _)| i)
                    .unwrap_or(session.input_buffer.len());

                session.input_buffer.insert_str(byte_pos, to_add);
                session.cursor_pos += to_add.chars().count();
            }
        }

        // Redraw: show completions list, then redraw input line with cursor
        let (new_input, cursor_pos) = {
            let conns = connections.lock();
            conns
                .get(&connection_id)
                .map(|s| (s.input_buffer.clone(), s.cursor_pos))
                .unwrap_or_default()
        };
        let redraw = telnet::redraw_input_line("> ", &new_input, cursor_pos);
        let redraw_str = String::from_utf8_lossy(&redraw).into_owned();
        let output = format!("\r\n{}\r\n{}", display, redraw_str);
        let _ = tx_client.send(output);
    }
}

/// OLC editor collecting a room/mobile/item description.
pub(crate) fn collect_desc(cx: &ConnCx<'_>, input: &str) {
    let ConnCx {
        connection_id,
        connections,
        state,
        tx_client,
        ..
    } = *cx;

    // Use shared editor command handler
    let result = {
        let mut conns = connections.lock();
        if let Some(session) = conns.get_mut(&connection_id) {
            handle_editor_command(&input, &mut session.olc_buffer, &mut session.olc_undo_buffer)
        } else {
            EditorCommandResult::Cancel
        }
    };

    match result {
        EditorCommandResult::Handled(msg) => {
            let _ = tx_client.send(msg);
        }
        EditorCommandResult::Save(content) => {
            if content.len() > crate::api::validate::DESCRIPTION_MAX {
                let _ = tx_client.send(format!(
                    "Description too long ({} bytes, max {}). Trim with .d or .r, then .save.\n",
                    content.len(),
                    crate::api::validate::DESCRIPTION_MAX
                ));
                return;
            }
            // Save buffer to room description
            let edit_room_id = {
                let conns = connections.lock();
                conns.get(&connection_id).and_then(|s| s.olc_edit_room)
            };

            if let Some(room_id) = edit_room_id {
                let world = state.lock();
                if let Ok(Some(mut room)) = world.db.get_room_data(&room_id) {
                    room.description = content;
                    let _ = world.db.save_room_data(room);
                }
                drop(world);
                let _ = tx_client.send("Description saved.\n".to_string());
            } else {
                let _ = tx_client.send("Error: No room being edited.\n".to_string());
            }

            // Clear OLC state
            {
                let mut conns = connections.lock();
                if let Some(session) = conns.get_mut(&connection_id) {
                    session.olc_mode = None;
                    session.olc_buffer.clear();
                    session.olc_edit_room = None;
                    session.olc_undo_buffer = None;
                }
            }
            let prompt = build_prompt(&connection_id, &connections, &state);
            let _ = tx_client.send(prompt);
        }
        EditorCommandResult::Cancel => {
            // Clear OLC state
            {
                let mut conns = connections.lock();
                if let Some(session) = conns.get_mut(&connection_id) {
                    session.olc_mode = None;
                    session.olc_buffer.clear();
                    session.olc_edit_room = None;
                    session.olc_undo_buffer = None;
                }
            }
            let _ = tx_client.send("Description editing cancelled.\n".to_string());
            let prompt = build_prompt(&connection_id, &connections, &state);
            let _ = tx_client.send(prompt);
        }
        EditorCommandResult::AppendText(text) => {
            let mut conns = connections.lock();
            if let Some(session) = conns.get_mut(&connection_id) {
                session.olc_undo_buffer = Some(session.olc_buffer.clone());
                session.olc_buffer.push(text);
            }
        }
    }
}

/// OLC editor collecting an item note body.
pub(crate) fn collect_note(cx: &ConnCx<'_>, input: &str) {
    let ConnCx {
        connection_id,
        connections,
        state,
        tx_client,
        ..
    } = *cx;

    let result = {
        let mut conns = connections.lock();
        if let Some(session) = conns.get_mut(&connection_id) {
            handle_editor_command(&input, &mut session.olc_buffer, &mut session.olc_undo_buffer)
        } else {
            EditorCommandResult::Cancel
        }
    };

    match result {
        EditorCommandResult::Handled(msg) => {
            let _ = tx_client.send(msg);
        }
        EditorCommandResult::Save(content) => {
            // Cap at 32 KB to avoid runaway bodies. Keep mode active on over-cap.
            const MAX_NOTE_BYTES: usize = 32 * 1024;
            if content.len() > MAX_NOTE_BYTES {
                let _ = tx_client.send(format!(
                    "Note too long ({} bytes, max {}). Trim with .d or .r, then .save.\n",
                    content.len(),
                    MAX_NOTE_BYTES
                ));
            } else {
                let edit_item_id = {
                    let conns = connections.lock();
                    conns.get(&connection_id).and_then(|s| s.olc_edit_item)
                };

                let (saved, cleared) = if let Some(item_id) = edit_item_id {
                    let world = state.lock();
                    match world.db.get_item_data(&item_id) {
                        Ok(Some(mut item)) => {
                            let cleared = content.is_empty();
                            item.note_content = if cleared { None } else { Some(content) };
                            let ok = world.db.save_item_data(item).is_ok();
                            (ok, cleared)
                        }
                        _ => (false, false),
                    }
                } else {
                    (false, false)
                };

                if saved {
                    let _ = tx_client.send(if cleared { "Note cleared.\n" } else { "Note saved.\n" }.to_string());
                } else {
                    let _ = tx_client.send("Error: could not save note.\n".to_string());
                }

                {
                    let mut conns = connections.lock();
                    if let Some(session) = conns.get_mut(&connection_id) {
                        session.olc_mode = None;
                        session.olc_buffer.clear();
                        session.olc_edit_item = None;
                        session.olc_undo_buffer = None;
                    }
                }
                let prompt = build_prompt(&connection_id, &connections, &state);
                let _ = tx_client.send(prompt);
            }
        }
        EditorCommandResult::Cancel => {
            {
                let mut conns = connections.lock();
                if let Some(session) = conns.get_mut(&connection_id) {
                    session.olc_mode = None;
                    session.olc_buffer.clear();
                    session.olc_edit_item = None;
                    session.olc_undo_buffer = None;
                }
            }
            let _ = tx_client.send("Note editing cancelled.\n".to_string());
            let prompt = build_prompt(&connection_id, &connections, &state);
            let _ = tx_client.send(prompt);
        }
        EditorCommandResult::AppendText(text) => {
            let mut conns = connections.lock();
            if let Some(session) = conns.get_mut(&connection_id) {
                session.olc_undo_buffer = Some(session.olc_buffer.clone());
                session.olc_buffer.push(text);
            }
        }
    }
}

/// OLC editor collecting an item's long description.
pub(crate) fn collect_long(cx: &ConnCx<'_>, input: &str) {
    let ConnCx {
        connection_id,
        connections,
        state,
        tx_client,
        ..
    } = *cx;

    let result = {
        let mut conns = connections.lock();
        if let Some(session) = conns.get_mut(&connection_id) {
            handle_editor_command(&input, &mut session.olc_buffer, &mut session.olc_undo_buffer)
        } else {
            EditorCommandResult::Cancel
        }
    };

    match result {
        EditorCommandResult::Handled(msg) => {
            let _ = tx_client.send(msg);
        }
        EditorCommandResult::Save(content) => {
            const MAX_LONG_BYTES: usize = 32 * 1024;
            if content.len() > MAX_LONG_BYTES {
                let _ = tx_client.send(format!(
                    "Long description too long ({} bytes, max {}). Trim with .d or .r, then .save.\n",
                    content.len(),
                    MAX_LONG_BYTES
                ));
            } else {
                let edit_item_id = {
                    let conns = connections.lock();
                    conns.get(&connection_id).and_then(|s| s.olc_edit_item)
                };

                let saved = if let Some(item_id) = edit_item_id {
                    let world = state.lock();
                    match world.db.get_item_data(&item_id) {
                        Ok(Some(mut item)) => {
                            item.long_desc = content;
                            world.db.save_item_data(item).is_ok()
                        }
                        _ => false,
                    }
                } else {
                    false
                };

                if saved {
                    let _ = tx_client.send("Long description saved.\n".to_string());
                } else {
                    let _ = tx_client.send("Error: could not save long description.\n".to_string());
                }

                {
                    let mut conns = connections.lock();
                    if let Some(session) = conns.get_mut(&connection_id) {
                        session.olc_mode = None;
                        session.olc_buffer.clear();
                        session.olc_edit_item = None;
                        session.olc_undo_buffer = None;
                    }
                }
                let prompt = build_prompt(&connection_id, &connections, &state);
                let _ = tx_client.send(prompt);
            }
        }
        EditorCommandResult::Cancel => {
            {
                let mut conns = connections.lock();
                if let Some(session) = conns.get_mut(&connection_id) {
                    session.olc_mode = None;
                    session.olc_buffer.clear();
                    session.olc_edit_item = None;
                    session.olc_undo_buffer = None;
                }
            }
            let _ = tx_client.send("Long description editing cancelled.\n".to_string());
            let prompt = build_prompt(&connection_id, &connections, &state);
            let _ = tx_client.send(prompt);
        }
        EditorCommandResult::AppendText(text) => {
            let mut conns = connections.lock();
            if let Some(session) = conns.get_mut(&connection_id) {
                session.olc_undo_buffer = Some(session.olc_buffer.clone());
                session.olc_buffer.push(text);
            }
        }
    }
}

/// Editor collecting a bulletin board post.
pub(crate) fn collect_board_post(cx: &ConnCx<'_>, input: &str) {
    let ConnCx {
        connection_id,
        connections,
        state,
        tx_client,
        ..
    } = *cx;

    let result = {
        let mut conns = connections.lock();
        if let Some(session) = conns.get_mut(&connection_id) {
            handle_editor_command(&input, &mut session.olc_buffer, &mut session.olc_undo_buffer)
        } else {
            EditorCommandResult::Cancel
        }
    };

    match result {
        EditorCommandResult::Handled(msg) => {
            let _ = tx_client.send(msg);
        }
        EditorCommandResult::Save(content) => {
            const MAX_POST_BYTES: usize = 32 * 1024;
            if content.len() > MAX_POST_BYTES {
                let _ = tx_client.send(format!(
                    "Post too long ({} bytes, max {}). Trim with .d or .r, then .save.\n",
                    content.len(),
                    MAX_POST_BYTES
                ));
            } else if content.is_empty() {
                let _ = tx_client.send("Empty post not saved. Type some text or .cancel to abort.\n".to_string());
            } else {
                let (board_vnum, subject, author) = {
                    let conns = connections.lock();
                    match conns.get(&connection_id) {
                        Some(s) => (
                            s.olc_edit_board_vnum.clone(),
                            s.olc_board_subject.clone(),
                            s.character.as_ref().map(|c| c.name.clone()),
                        ),
                        None => (None, None, None),
                    }
                };

                let saved = match (board_vnum, subject, author) {
                    (Some(vnum), Some(subj), Some(name)) => {
                        let world = state.lock();
                        let max = world
                            .db
                            .get_item_by_vnum(&vnum)
                            .ok()
                            .flatten()
                            .and_then(|item| item.board_max_messages);
                        let post = BoardPost::new(vnum, name, subj, content);
                        world.db.store_board_post(post, max).is_ok()
                    }
                    _ => false,
                };

                if saved {
                    let _ = tx_client.send("Post saved.\n".to_string());
                } else {
                    let _ = tx_client.send("Error: could not save post.\n".to_string());
                }

                {
                    let mut conns = connections.lock();
                    if let Some(session) = conns.get_mut(&connection_id) {
                        session.olc_mode = None;
                        session.olc_buffer.clear();
                        session.olc_edit_board_vnum = None;
                        session.olc_board_subject = None;
                        session.olc_undo_buffer = None;
                    }
                }
                let prompt = build_prompt(&connection_id, &connections, &state);
                let _ = tx_client.send(prompt);
            }
        }
        EditorCommandResult::Cancel => {
            {
                let mut conns = connections.lock();
                if let Some(session) = conns.get_mut(&connection_id) {
                    session.olc_mode = None;
                    session.olc_buffer.clear();
                    session.olc_edit_board_vnum = None;
                    session.olc_board_subject = None;
                    session.olc_undo_buffer = None;
                }
            }
            let _ = tx_client.send("Post cancelled.\n".to_string());
            let prompt = build_prompt(&connection_id, &connections, &state);
            let _ = tx_client.send(prompt);
        }
        EditorCommandResult::AppendText(text) => {
            let mut conns = connections.lock();
            if let Some(session) = conns.get_mut(&connection_id) {
                session.olc_undo_buffer = Some(session.olc_buffer.clone());
                session.olc_buffer.push(text);
            }
        }
    }
}

/// Editor collecting a DG trigger body.
pub(crate) fn collect_dg_body(cx: &ConnCx<'_>, input: &str) {
    let ConnCx {
        connection_id,
        connections,
        state,
        tx_client,
        ..
    } = *cx;

    let result = {
        let mut conns = connections.lock();
        if let Some(session) = conns.get_mut(&connection_id) {
            handle_editor_command(&input, &mut session.olc_buffer, &mut session.olc_undo_buffer)
        } else {
            EditorCommandResult::Cancel
        }
    };

    match result {
        EditorCommandResult::Handled(msg) => {
            let _ = tx_client.send(msg);
        }
        EditorCommandResult::Save(content) => {
            const MAX_DG_BODY_BYTES: usize = 32 * 1024;
            if content.len() > MAX_DG_BODY_BYTES {
                let _ = tx_client.send(format!(
                    "DG body too long ({} bytes, max {}). Trim with .d or .r, then .save.\n",
                    content.len(),
                    MAX_DG_BODY_BYTES
                ));
            } else {
                // Run the analyzer up front. ParseError refuses the
                // save (mirrors proto path); other issues become
                // post-save warnings shown to the editor.
                let issues = crate::script::dg::analyze::analyze(&content);
                let parse_errors: Vec<&_> = issues
                    .iter()
                    .filter(|i| i.kind == crate::script::dg::analyze::IssueKind::ParseError)
                    .collect();
                if !parse_errors.is_empty() {
                    let detail = parse_errors
                        .iter()
                        .map(|i| i.detail.as_str())
                        .collect::<Vec<_>>()
                        .join("; ");
                    let _ = tx_client.send(format!(
                        "Save refused: parse error: {}\nUse .r to keep editing or .a to abort.\n",
                        detail
                    ));
                    return;
                }
                let warnings: Vec<String> = issues
                    .into_iter()
                    .filter(|i| i.kind != crate::script::dg::analyze::IssueKind::ParseError)
                    .map(|i| format!("  warning: {:?}: {}", i.kind, i.detail))
                    .collect();

                let (host_kind, host_id, idx, author, author_is_admin) = {
                    let conns = connections.lock();
                    conns
                        .get(&connection_id)
                        .map(|s| {
                            let kind = s.olc_edit_trigger_host.clone();
                            let id = match kind.as_deref() {
                                Some("mobile") => s.olc_edit_mobile,
                                Some("item") => s.olc_edit_item,
                                Some("room") => s.olc_edit_room,
                                _ => None,
                            };
                            let (author, is_admin) = s
                                .character
                                .as_ref()
                                .map(|c| (c.name.clone(), c.is_admin))
                                .unwrap_or((String::new(), false));
                            (kind, id, s.olc_edit_trigger_index, author, is_admin)
                        })
                        .unwrap_or((None, None, None, String::new(), false))
                };

                // Author stamping policy (F3 full):
                // - Always overwrite `authored_by` to the editing
                //   character's name. Admins are stamped as well so
                //   the audit trail is complete; the opcode gate
                //   trusts admin authors via can_edit_area.
                // - Non-admin authors must NOT carry `elevated=true`
                //   forward; if a builder edits a body that an admin
                //   previously elevated, the elevation is dropped to
                //   prevent privilege passthrough.
                let saved = if let (Some(kind), Some(id), Some(idx)) = (host_kind, host_id, idx) {
                    let world = state.lock();
                    let body = if content.is_empty() { None } else { Some(content) };
                    let author_opt = if author.is_empty() { None } else { Some(author.clone()) };
                    match kind.as_str() {
                        "mobile" => world
                            .db
                            .update_mobile(&id, |m| {
                                if idx < m.triggers.len() {
                                    m.triggers[idx].dg_body = body.clone();
                                    m.triggers[idx].authored_by = author_opt.clone();
                                    if !author_is_admin {
                                        m.triggers[idx].elevated = false;
                                    }
                                }
                            })
                            .is_ok(),
                        "item" => world
                            .db
                            .update_item(&id, |i| {
                                if idx < i.triggers.len() {
                                    i.triggers[idx].dg_body = body.clone();
                                    i.triggers[idx].authored_by = author_opt.clone();
                                    if !author_is_admin {
                                        i.triggers[idx].elevated = false;
                                    }
                                }
                            })
                            .is_ok(),
                        "room" => world
                            .db
                            .update_room(&id, |r| {
                                if idx < r.triggers.len() {
                                    r.triggers[idx].dg_body = body.clone();
                                    r.triggers[idx].authored_by = author_opt.clone();
                                    if !author_is_admin {
                                        r.triggers[idx].elevated = false;
                                    }
                                }
                            })
                            .is_ok(),
                        _ => false,
                    }
                } else {
                    false
                };

                let mut save_msg = if saved {
                    "DG trigger body saved.\n".to_string()
                } else {
                    "Error: could not save DG trigger body.\n".to_string()
                };
                if saved && !warnings.is_empty() {
                    save_msg.push_str(&warnings.join("\n"));
                    save_msg.push('\n');
                }
                let _ = tx_client.send(save_msg);

                {
                    let mut conns = connections.lock();
                    if let Some(session) = conns.get_mut(&connection_id) {
                        session.olc_mode = None;
                        session.olc_buffer.clear();
                        session.olc_edit_room = None;
                        session.olc_edit_item = None;
                        session.olc_edit_mobile = None;
                        session.olc_edit_trigger_host = None;
                        session.olc_edit_trigger_index = None;
                        session.olc_undo_buffer = None;
                    }
                }
                let prompt = build_prompt(&connection_id, &connections, &state);
                let _ = tx_client.send(prompt);
            }
        }
        EditorCommandResult::Cancel => {
            {
                let mut conns = connections.lock();
                if let Some(session) = conns.get_mut(&connection_id) {
                    session.olc_mode = None;
                    session.olc_buffer.clear();
                    session.olc_edit_room = None;
                    session.olc_edit_item = None;
                    session.olc_edit_mobile = None;
                    session.olc_edit_trigger_host = None;
                    session.olc_edit_trigger_index = None;
                    session.olc_undo_buffer = None;
                }
            }
            let _ = tx_client.send("DG trigger body editing cancelled.\n".to_string());
            let prompt = build_prompt(&connection_id, &connections, &state);
            let _ = tx_client.send(prompt);
        }
        EditorCommandResult::AppendText(text) => {
            let mut conns = connections.lock();
            if let Some(session) = conns.get_mut(&connection_id) {
                session.olc_undo_buffer = Some(session.olc_buffer.clone());
                session.olc_buffer.push(text);
            }
        }
    }
}

/// Editor collecting a DG prototype body.
pub(crate) fn collect_dg_proto_body(cx: &ConnCx<'_>, input: &str) {
    let ConnCx {
        connection_id,
        connections,
        state,
        tx_client,
        ..
    } = *cx;

    let result = {
        let mut conns = connections.lock();
        if let Some(session) = conns.get_mut(&connection_id) {
            handle_editor_command(&input, &mut session.olc_buffer, &mut session.olc_undo_buffer)
        } else {
            EditorCommandResult::Cancel
        }
    };
    match result {
        EditorCommandResult::Handled(msg) => {
            let _ = tx_client.send(msg);
        }
        EditorCommandResult::Save(content) => {
            const MAX_DG_BODY_BYTES: usize = 32 * 1024;
            if content.len() > MAX_DG_BODY_BYTES {
                let _ = tx_client.send(format!(
                    "Proto body too long ({} bytes, max {}). Trim with .d or .r, then .save.\n",
                    content.len(),
                    MAX_DG_BODY_BYTES
                ));
            } else {
                let (proto_vnum, author) = {
                    let conns = connections.lock();
                    conns
                        .get(&connection_id)
                        .map(|s| {
                            (
                                s.olc_edit_proto_vnum.clone(),
                                s.character.as_ref().map(|c| c.name.clone()).unwrap_or_default(),
                            )
                        })
                        .unwrap_or_default()
                };
                let msg = if let Some(vnum) = proto_vnum {
                    let world = state.lock();
                    save_dg_proto_body_msg(&world.db, &vnum, content, &author)
                } else {
                    "Proto save failed: no proto vnum bound to this editor session.\n".to_string()
                };
                let _ = tx_client.send(msg);
                {
                    let mut conns = connections.lock();
                    if let Some(session) = conns.get_mut(&connection_id) {
                        session.olc_mode = None;
                        session.olc_buffer.clear();
                        session.olc_edit_proto_vnum = None;
                        session.olc_undo_buffer = None;
                    }
                }
                let prompt = build_prompt(&connection_id, &connections, &state);
                let _ = tx_client.send(prompt);
            }
        }
        EditorCommandResult::Cancel => {
            {
                let mut conns = connections.lock();
                if let Some(session) = conns.get_mut(&connection_id) {
                    session.olc_mode = None;
                    session.olc_buffer.clear();
                    session.olc_edit_proto_vnum = None;
                    session.olc_undo_buffer = None;
                }
            }
            let _ = tx_client.send("DG proto body editing cancelled.\n".to_string());
            let prompt = build_prompt(&connection_id, &connections, &state);
            let _ = tx_client.send(prompt);
        }
        EditorCommandResult::AppendText(text) => {
            let mut conns = connections.lock();
            if let Some(session) = conns.get_mut(&connection_id) {
                session.olc_undo_buffer = Some(session.olc_buffer.clone());
                session.olc_buffer.push(text);
            }
        }
    }
}

/// Editor collecting dialogue node text.
pub(crate) fn collect_dialogue_node_text(cx: &ConnCx<'_>, input: &str) {
    let ConnCx {
        connection_id,
        connections,
        state,
        tx_client,
        ..
    } = *cx;

    let result = {
        let mut conns = connections.lock();
        if let Some(session) = conns.get_mut(&connection_id) {
            handle_editor_command(&input, &mut session.olc_buffer, &mut session.olc_undo_buffer)
        } else {
            EditorCommandResult::Cancel
        }
    };

    match result {
        EditorCommandResult::Handled(msg) => {
            let _ = tx_client.send(msg);
        }
        EditorCommandResult::Save(content) => {
            let (mob_id, node_name) = {
                let conns = connections.lock();
                conns
                    .get(&connection_id)
                    .map(|s| (s.olc_edit_mobile, s.olc_dialogue_node_name.clone()))
                    .unwrap_or((None, None))
            };
            let saved = if let (Some(mid), Some(name)) = (mob_id, node_name) {
                let world = state.lock();
                world
                    .db
                    .update_mobile(&mid, |m| {
                        if let Some(tree) = m.dialogue_tree.as_mut() {
                            if let Some(node) = tree.nodes.get_mut(&name) {
                                node.text = content.clone();
                            }
                        }
                    })
                    .is_ok()
            } else {
                false
            };
            let _ = tx_client.send(if saved {
                "Dialogue node text saved.\n".to_string()
            } else {
                "Error: could not save dialogue node text.\n".to_string()
            });
            {
                let mut conns = connections.lock();
                if let Some(session) = conns.get_mut(&connection_id) {
                    session.olc_mode = None;
                    session.olc_buffer.clear();
                    session.olc_edit_mobile = None;
                    session.olc_dialogue_node_name = None;
                    session.olc_undo_buffer = None;
                }
            }
            let prompt = build_prompt(&connection_id, &connections, &state);
            let _ = tx_client.send(prompt);
        }
        EditorCommandResult::Cancel => {
            {
                let mut conns = connections.lock();
                if let Some(session) = conns.get_mut(&connection_id) {
                    session.olc_mode = None;
                    session.olc_buffer.clear();
                    session.olc_edit_mobile = None;
                    session.olc_dialogue_node_name = None;
                    session.olc_undo_buffer = None;
                }
            }
            let _ = tx_client.send("Dialogue node text editing cancelled.\n".to_string());
            let prompt = build_prompt(&connection_id, &connections, &state);
            let _ = tx_client.send(prompt);
        }
        EditorCommandResult::AppendText(text) => {
            let mut conns = connections.lock();
            if let Some(session) = conns.get_mut(&connection_id) {
                session.olc_undo_buffer = Some(session.olc_buffer.clone());
                session.olc_buffer.push(text);
            }
        }
    }
}

/// Editor collecting an extra description.
pub(crate) fn collect_extra_desc(cx: &ConnCx<'_>, input: &str) {
    let ConnCx {
        connection_id,
        connections,
        state,
        tx_client,
        ..
    } = *cx;

    // Use shared editor command handler
    let result = {
        let mut conns = connections.lock();
        if let Some(session) = conns.get_mut(&connection_id) {
            handle_editor_command(&input, &mut session.olc_buffer, &mut session.olc_undo_buffer)
        } else {
            EditorCommandResult::Cancel
        }
    };

    match result {
        EditorCommandResult::Handled(msg) => {
            let _ = tx_client.send(msg);
        }
        EditorCommandResult::Save(content) => {
            // Mode-specific save: add extra description to either the room
            // or the item being edited (item path takes precedence when set).
            let (edit_room_id, edit_item_id, keywords) = {
                let conns = connections.lock();
                if let Some(session) = conns.get(&connection_id) {
                    (
                        session.olc_edit_room,
                        session.olc_edit_item,
                        session.olc_extra_keywords.clone(),
                    )
                } else {
                    (None, None, Vec::new())
                }
            };

            if let Some(item_id) = edit_item_id {
                let world = state.lock();
                if let Ok(Some(mut item)) = world.db.get_item_data(&item_id) {
                    item.extra_descs.push(ExtraDesc {
                        keywords,
                        description: content,
                    });
                    let _ = world.db.save_item_data(item);
                }
                drop(world);
                let _ = tx_client.send("Extra description saved.\n".to_string());
            } else if let Some(room_id) = edit_room_id {
                let world = state.lock();
                if let Ok(Some(mut room)) = world.db.get_room_data(&room_id) {
                    room.extra_descs.push(ExtraDesc {
                        keywords,
                        description: content,
                    });
                    let _ = world.db.save_room_data(room);
                }
                drop(world);
                let _ = tx_client.send("Extra description saved.\n".to_string());
            } else {
                let _ = tx_client.send("Error: No room or item being edited.\n".to_string());
            }

            // Clear OLC mode and buffer
            {
                let mut conns = connections.lock();
                if let Some(session) = conns.get_mut(&connection_id) {
                    session.olc_mode = None;
                    session.olc_buffer.clear();
                    session.olc_edit_room = None;
                    session.olc_edit_item = None;
                    session.olc_extra_keywords.clear();
                    session.olc_undo_buffer = None;
                }
            }
        }
        EditorCommandResult::Cancel => {
            // Clear OLC mode and buffer
            {
                let mut conns = connections.lock();
                if let Some(session) = conns.get_mut(&connection_id) {
                    session.olc_mode = None;
                    session.olc_buffer.clear();
                    session.olc_edit_room = None;
                    session.olc_edit_item = None;
                    session.olc_extra_keywords.clear();
                    session.olc_undo_buffer = None;
                }
            }
            let _ = tx_client.send("Extra description editing cancelled.\n".to_string());
        }
        EditorCommandResult::AppendText(text) => {
            // Append line to buffer (save undo first)
            {
                let mut conns = connections.lock();
                if let Some(session) = conns.get_mut(&connection_id) {
                    session.olc_undo_buffer = Some(session.olc_buffer.clone());
                    session.olc_buffer.push(text);
                }
            }
            let _ = tx_client.send("Line added.\n".to_string());
        }
    }
}

/// Editor collecting the MOTD.
pub(crate) fn collect_motd(cx: &ConnCx<'_>, input: &str) {
    let ConnCx {
        connection_id,
        connections,
        state,
        tx_client,
        ..
    } = *cx;

    let result = {
        let mut conns = connections.lock();
        if let Some(session) = conns.get_mut(&connection_id) {
            handle_editor_command(&input, &mut session.olc_buffer, &mut session.olc_undo_buffer)
        } else {
            EditorCommandResult::Cancel
        }
    };

    match result {
        EditorCommandResult::Handled(msg) => {
            let _ = tx_client.send(msg);
        }
        EditorCommandResult::Save(content) => {
            let world = state.lock();
            let _ = world.db.set_setting("motd", &content);
            drop(world);
            let _ = tx_client.send("MOTD saved.\n".to_string());

            // Clear OLC state
            {
                let mut conns = connections.lock();
                if let Some(session) = conns.get_mut(&connection_id) {
                    session.olc_mode = None;
                    session.olc_buffer.clear();
                    session.olc_undo_buffer = None;
                }
            }
            let prompt = build_prompt(&connection_id, &connections, &state);
            let _ = tx_client.send(prompt);
        }
        EditorCommandResult::Cancel => {
            // Clear OLC state
            {
                let mut conns = connections.lock();
                if let Some(session) = conns.get_mut(&connection_id) {
                    session.olc_mode = None;
                    session.olc_buffer.clear();
                    session.olc_undo_buffer = None;
                }
            }
            let _ = tx_client.send("MOTD editing cancelled.\n".to_string());
            let prompt = build_prompt(&connection_id, &connections, &state);
            let _ = tx_client.send(prompt);
        }
        EditorCommandResult::AppendText(text) => {
            let mut conns = connections.lock();
            if let Some(session) = conns.get_mut(&connection_id) {
                session.olc_undo_buffer = Some(session.olc_buffer.clone());
                session.olc_buffer.push(text);
            }
        }
    }
}

/// Builder answering the y/n prompt for an AI-written description.
pub(crate) fn ai_confirm(cx: &ConnCx<'_>, input: &str) -> bool {
    let ConnCx {
        connection_id,
        connections,
        state,
        tx_client,
        ..
    } = *cx;

    let trimmed = input.trim().to_lowercase();

    if trimmed == "y" || trimmed == "yes" {
        // Accept the AI description
        let (response, target) = {
            let mut conns = connections.lock();
            if let Some(session) = conns.get_mut(&connection_id) {
                let response = session.pending_ai_response.take();
                let target = session.pending_ai_target.take();
                session.olc_mode = None;
                session.pending_ai_request = None;
                (response, target)
            } else {
                (None, None)
            }
        };

        if let (Some(response), Some(target)) = (response, target) {
            if response.success {
                // Apply the description based on target type
                let engine = state.lock().engine.clone();
                // A panicking setter must not take the connection
                // task (and its session) down with it.
                let run = |scope: &mut rhai::Scope, src: &str| {
                    let _ = script::trigger_sandbox::guard_panic(|| engine.eval_with_scope::<()>(scope, src));
                };
                let mut scope = rhai::Scope::new();
                scope.push("connection_id", connection_id.to_string());

                // Apply main description
                if let Some(ref desc) = response.description {
                    match target.target_type {
                        claude::AiTargetType::Room => {
                            scope.push("room_id", target.entity_id.to_string());
                            scope.push("new_desc", desc.clone());
                            // Seasonal drafts go to their season's slot,
                            // never over the main description.
                            let call = match target.field.as_str() {
                                "spring_desc" => "set_room_spring_desc(room_id, new_desc);",
                                "summer_desc" => "set_room_summer_desc(room_id, new_desc);",
                                "autumn_desc" => "set_room_autumn_desc(room_id, new_desc);",
                                "winter_desc" => "set_room_winter_desc(room_id, new_desc);",
                                _ => "set_room_description(room_id, new_desc);",
                            };
                            run(&mut scope, call);
                        }
                        claude::AiTargetType::Mobile => {
                            scope.push("mobile_id", target.entity_id.to_string());
                            scope.push("new_desc", desc.clone());
                            if target.field == "short_desc" {
                                run(&mut scope, "set_mobile_short_desc(mobile_id, new_desc);");
                            } else {
                                run(&mut scope, "set_mobile_long_desc(mobile_id, new_desc);");
                            }
                        }
                        claude::AiTargetType::Item => {
                            scope.push("item_id", target.entity_id.to_string());
                            scope.push("new_desc", desc.clone());
                            if target.field == "short_desc" {
                                run(&mut scope, "set_item_short_desc(item_id, new_desc);");
                            } else {
                                run(&mut scope, "set_item_long_desc(item_id, new_desc);");
                            }
                        }
                    }
                }

                // Apply extra descriptions for rooms
                let extra_count = response.extra_descs.len();
                let main_room_desc =
                    matches!(target.target_type, claude::AiTargetType::Room) && target.field == "description";
                if main_room_desc && extra_count > 0 {
                    for extra in &response.extra_descs {
                        let keywords_str = extra.keywords.join(" ");
                        scope.push("room_id", target.entity_id.to_string());
                        scope.push("keywords", keywords_str);
                        scope.push("extra_desc", extra.description.clone());
                        run(&mut scope, "add_room_extra_desc(room_id, keywords, extra_desc);");
                    }
                    let _ = tx_client.send(format!(
                        "Description updated. {} extra description(s) added.\n",
                        extra_count
                    ));
                } else {
                    let _ = tx_client.send("Description updated.\n".to_string());
                }
            }
        }
        return true;
    } else if trimmed == "n" || trimmed == "no" {
        // Reject the AI description
        {
            let mut conns = connections.lock();
            if let Some(session) = conns.get_mut(&connection_id) {
                session.pending_ai_response = None;
                session.pending_ai_target = None;
                session.pending_ai_request = None;
                session.olc_mode = None;
            }
        }
        let _ = tx_client.send("Description rejected.\n".to_string());
        return true;
    } else {
        let _ = tx_client.send("Please enter 'y' to accept or 'n' to reject.\n".to_string());
        return true;
    }
}

/// Any other session mode (character wizards, migration prompt, ...): route the line to the script that owns the mode. Returns true when the line was consumed.
pub(crate) fn script_mode(cx: &ConnCx<'_>, input: &str, mode: &str) -> bool {
    let ConnCx {
        connection_id,
        connections,
        state,
        tx_client,
        ..
    } = *cx;

    let script_path = if mode.starts_with("chargen_") || mode == "migration_prompt" {
        // Check wizard_data to determine which script started the wizard
        let is_migration = {
            let conns = connections.lock();
            conns
                .get(&connection_id)
                .and_then(|s| s.wizard_data.as_ref())
                .map(|d| d.contains("migration=true"))
                .unwrap_or(false)
        };
        if is_migration || mode == "migration_prompt" {
            Some("scripts/commands/login.rhai".to_string())
        } else {
            Some("scripts/commands/create.rhai".to_string())
        }
    } else if mode == "traits_select" {
        Some("scripts/commands/traits.rhai".to_string())
    } else if mode == "select_character"
        || mode == "retire_pick"
        || mode == "retire_confirm"
        || mode == "delete_account_confirm"
    {
        // Roster mode (post-auth, pre-character-pick) and its
        // self-service retire / account-delete sub-flows. All inputs
        // go to login.rhai, which branches on the OLC mode string.
        Some("scripts/commands/login.rhai".to_string())
    } else if mode == "email_verify_prompt" {
        // Email verification: 6-digit code, 'resend', or 'cancel'.
        // create.rhai owns the unauthenticated path (which is where
        // verification originates); login.rhai's gate also routes
        // through create.rhai for consistent handling.
        Some("scripts/commands/create.rhai".to_string())
    } else if mode == "mail_compose" {
        // mail compose stays in mail.rhai so @attach/@detach/@@/@
        // and free-form lines all reach handle_compose_mode.
        Some("scripts/commands/mail.rhai".to_string())
    } else {
        None
    };

    if let Some(script) = script_path {
        // Clone the AST and engine reference
        let (ast_opt, engine) = {
            let world = state.lock();
            let ast = world.scripts.get(&script).cloned();
            (ast, world.engine.clone())
        };

        if let Some(ast) = ast_opt {
            let mut scope = rhai::Scope::new();
            scope.set_value("args", input.to_string());
            scope.set_value("connection_id", connection_id.to_string());

            match call_run_command_guarded(
                &engine,
                &mut scope,
                &ast,
                input.to_string(),
                connection_id.to_string(),
                &script,
            ) {
                Ok(output) => {
                    if output.is_string() {
                        let msg = output.into_string().unwrap();
                        if !msg.is_empty() {
                            let _ = tx_client.send(msg);
                        }
                    }
                }
                Err(e) => {
                    let _ = tx_client.send(format!("Error: {}\n", e));
                }
            }
            // Send prompt after wizard command execution
            let prompt = build_prompt(&connection_id, &connections, &state);
            let _ = tx_client.send(prompt);
        }
        return true;
    }
    false
}
