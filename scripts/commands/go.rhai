// scripts/commands/go.rhai

fn get_opposite_direction(dir) {
    if dir == "north" || dir == "n" { return "south"; }
    if dir == "south" || dir == "s" { return "north"; }
    if dir == "east" || dir == "e" { return "west"; }
    if dir == "west" || dir == "w" { return "east"; }
    if dir == "up" || dir == "u" { return "below"; }
    if dir == "down" || dir == "d" { return "above"; }
    return "somewhere";
}

fn normalize_direction(dir) {
    if dir == "n" { return "north"; }
    if dir == "s" { return "south"; }
    if dir == "e" { return "east"; }
    if dir == "w" { return "west"; }
    if dir == "u" { return "up"; }
    if dir == "d" { return "down"; }
    return dir;
}

fn get_exit_for_direction(exits, direction) {
    if direction == "north" || direction == "n" { return exits.north; }
    if direction == "east" || direction == "e" { return exits.east; }
    if direction == "south" || direction == "s" { return exits.south; }
    if direction == "west" || direction == "w" { return exits.west; }
    if direction == "up" || direction == "u" { return exits.up; }
    if direction == "down" || direction == "d" { return exits.down; }
    if direction == "out" { return exits.out; }
    // Check custom exits (e.g., "elevator", "train")
    let dir_lower = direction.to_lower();
    if exits.custom.contains(dir_lower) {
        return exits.custom[dir_lower];
    }
    return "";
}

// Get a random exit room_id that isn't the excluded direction
fn get_random_exit(exits, exclude_direction) {
    let options = [];
    if exits.north != "" && exclude_direction != "north" { options.push(exits.north); }
    if exits.south != "" && exclude_direction != "south" { options.push(exits.south); }
    if exits.east != "" && exclude_direction != "east" { options.push(exits.east); }
    if exits.west != "" && exclude_direction != "west" { options.push(exits.west); }
    if exits.up != "" && exclude_direction != "up" { options.push(exits.up); }
    if exits.down != "" && exclude_direction != "down" { options.push(exits.down); }
    if options.len() == 0 { return ""; }
    let idx = random_int(0, options.len() - 1);
    return options[idx];
}

fn move_follower(follower_name, leader_name, direction) {
    let follower_conn = find_player_connection_by_name(follower_name);
    if follower_conn == "" { return; }

    let follower = get_player_character(follower_conn);
    if follower == () { return; }

    // Must be standing
    if follower.position == "sitting" {
        send_client_message(follower_conn, "You cannot follow " + leader_name + " while sitting.");
        return;
    }
    if follower.position == "sleeping" {
        send_client_message(follower_conn, "You cannot follow " + leader_name + " while sleeping.");
        return;
    }
    if follower.position != "standing" { return; }

    // Must have stamina (god mode and build mode bypass)
    if !follower.god_mode && !is_in_build_mode(follower.name, follower.current_room_id) && follower.stamina <= 0 {
        send_client_message(follower_conn, "You are too exhausted to follow " + leader_name + "!");
        return;
    }

    // Send follow message and move them
    send_client_message(follower_conn, "You follow " + leader_name + ".");
    run_command(direction, follower_conn);
}

fn check_door_blocks_movement(room_id, direction, connection_id, char_name) {
    let dir_normalized = normalize_direction(direction);
    let door = get_door(room_id, dir_normalized);
    if door == () { return false; }
    if !door.is_closed { return false; }

    // Build mode bypasses doors in editable areas
    if is_in_build_mode(char_name, room_id) {
        send_client_message(connection_id, "\x1b[1;33m[BUILD]\x1b[0m You pass through the " + door.name + ".");
        return false;
    }

    if door.is_locked {
        send_client_message(connection_id, "The " + door.name + " is closed and locked.");
    } else {
        send_client_message(connection_id, "The " + door.name + " is closed.");
    }
    return true;
}

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to move.");
        return;
    }

    // Check position - can't move while sitting or sleeping
    if char.position == "sleeping" {
        send_client_message(connection_id, "You are asleep! Use 'wake' to wake up first.");
        return;
    }
    if char.position == "sitting" {
        send_client_message(connection_id, "You are sitting down. Use 'stand' to get up first.");
        return;
    }

    // Check stamina before moving (god mode and build mode bypass)
    if !char.god_mode && !is_in_build_mode(char.name, char.current_room_id) && char.stamina <= 0 {
        send_client_message(connection_id, "You are too exhausted to move!");
        return;
    }

    // Combat lock - must use flee command to escape combat
    if is_in_combat(char.name) {
        send_client_message(connection_id, "You are in combat! Use 'flee' to escape.");
        return;
    }

    // Check encumbrance - cannot move if overloaded (build mode bypasses)
    let enc_level = get_encumbrance_level(char.name);
    if enc_level == "overloaded" && !char.god_mode && !is_in_build_mode(char.name, char.current_room_id) {
        send_client_message(connection_id, "You are carrying too much weight to move!");
        return;
    }

    let direction = args.to_lower();
    if direction == "" {
        send_client_message(connection_id, "Go where? (north, east, south, west, up, down)");
        return;
    }

    let room = get_room_data(char.current_room_id);
    if room == () {
        send_client_message(connection_id, "You are nowhere. Something is wrong.");
        return;
    }

    // Get the exit for the requested direction
    let target_room_id = get_exit_for_direction(room.exits, direction);

    // Check if exit exists - this covers standard directions, "out", and custom exits
    if target_room_id == "" {
        send_client_message(connection_id, "You can't go that way.");
        return;
    }

    // Drunk stumble: chance to go in a random direction instead
    if get_drunk_level(char.name) > 50 && !char.god_mode && !is_in_build_mode(char.name, char.current_room_id) {
        let stumble_roll = random_int(1, 100);
        if stumble_roll <= 25 {
            let alt = get_random_exit(room.exits, direction);
            if alt != "" {
                target_room_id = alt;
                send_client_message(connection_id, "You stumble drunkenly and go the wrong way!");
            }
        }
    }

    // Check for closed door
    if check_door_blocks_movement(room.id, direction, connection_id, char.name) {
        return;
    }

    // Check property access if entering a property room (build mode bypasses)
    if is_property_room(target_room_id) && !is_in_build_mode(char.name, target_room_id) {
        let lease = get_lease_for_room(target_room_id);
        if lease != () && !can_enter_property(lease.id, char.name) {
            send_client_message(connection_id, "You do not have permission to enter that property.");
            return;
        }
    }

    // Area level check - enforce level_min against effective combat level
    if !char.god_mode && !is_in_build_mode(char.name, char.current_room_id) {
        let target_room_data = get_room_data(target_room_id);
        if target_room_data != () && target_room_data.area_id != "" {
            let area = get_area_data(target_room_data.area_id);
            if area != () && area.level_min > 0 {
                let combat_level = get_effective_combat_level(char.name);
                if combat_level < area.level_min {
                    send_client_message(connection_id, "You are not experienced enough to enter this area. (Requires combat level " + area.level_min + ", yours is " + combat_level + ")");
                    return;
                }
            }
        }
    }

    // Single-occupant (tunnel) and 2-occupant (private) caps. Players only
    // — mobs in private inn rooms keep working. god_mode + build_mode bypass.
    if !char.god_mode && !is_in_build_mode(char.name, target_room_id) {
        let cap_room = get_room_data(target_room_id);
        if cap_room != () {
            let cap = 0;
            let label = "";
            if cap_room.flags.tunnel { cap = 1; label = "narrow"; }
            else if cap_room.flags.private_room { cap = 2; label = "private"; }
            if cap > 0 {
                let occupants = get_characters_in_room(target_room_id);
                if occupants.len() >= cap {
                    if cap == 1 {
                        send_client_message(connection_id, "The passage is too " + label + " for another to enter.");
                    } else {
                        send_client_message(connection_id, "That room is too " + label + " — there's no space for you.");
                    }
                    return;
                }
            }
        }
    }

    let old_room_id = char.current_room_id;

    // Fire on_exit trigger - can cancel movement
    let exit_result = fire_room_trigger(old_room_id, "on_exit", connection_id, #{
        "direction": direction,
        "target_room": target_room_id
    });
    if exit_result == "cancel" {
        return;
    }

    // Fire on_enter trigger for target room - can cancel movement
    let enter_result = fire_room_trigger(target_room_id, "on_enter", connection_id, #{
        "direction": direction,
        "source_room": old_room_id
    });
    if enter_result == "cancel" {
        return;
    }

    // Get target room data to check for difficult terrain
    let target_room = get_room_data(target_room_id);

    // Water system checks
    let is_shallow = target_room != () && target_room.flags.shallow_water;
    let is_deep = target_room != () && target_room.flags.deep_water;
    let is_underwater = target_room != () && target_room.flags.underwater;

    // Deep water gate: requires boat or swimming skill 3+
    if is_deep && !char.god_mode && !is_in_build_mode(char.name, target_room_id) {
        let has_boat = player_has_item_flag(char.name, "boat");
        let swim_level = get_skill_level(char.name, "swimming");
        if !has_boat && swim_level < 3 {
            send_client_message(connection_id, "The water is too deep! You need a boat or swimming skill 3+.");
            return;
        }
    }

    // Calculate stamina cost based on terrain (configurable via settings)
    let base_cost_str = get_setting_or_default("stamina_cost_move", "1");
    let base_cost = parse_int(base_cost_str);
    if base_cost == () { base_cost = 1; }
    let difficult_cost_str = get_setting_or_default("stamina_cost_move_difficult", "2");
    let difficult_cost = parse_int(difficult_cost_str);
    if difficult_cost == () { difficult_cost = 2; }

    let stamina_cost = base_cost;
    let is_difficult = target_room != () && target_room.flags.difficult_terrain;
    if is_difficult {
        stamina_cost = difficult_cost;
    }

    // Water stamina costs (stacks with difficult_terrain if both set)
    if is_shallow {
        stamina_cost += 1;
        let swim_skill = get_skill_level(char.name, "swimming");
        if swim_skill >= 3 { stamina_cost -= 1; }
    }
    if is_deep {
        stamina_cost += 2;
        let swim_skill = get_skill_level(char.name, "swimming");
        if swim_skill >= 3 { stamina_cost -= 1; }
        if swim_skill >= 7 { stamina_cost -= 1; }
    }
    if is_underwater {
        stamina_cost += 3;
        let swim_skill = get_skill_level(char.name, "swimming");
        if swim_skill >= 2 { stamina_cost -= 1; }
        if swim_skill >= 5 { stamina_cost -= 1; }
        if swim_skill >= 8 { stamina_cost -= 1; }
    }

    // Check for stamina consumption traits
    let has_endurance = false;
    let has_exhaustion_prone = false;
    let has_marathoner = false;
    let has_light_footed = false;
    let has_clumsy = false;
    for trait_id in char.traits {
        if trait_id == "endurance" { has_endurance = true; }
        if trait_id == "exhaustion_prone" { has_exhaustion_prone = true; }
        if trait_id == "marathoner" { has_marathoner = true; }
        if trait_id == "light_footed" { has_light_footed = true; }
        if trait_id == "clumsy" { has_clumsy = true; }
    }

    // Apply difficult terrain trait modifiers
    if is_difficult {
        if has_light_footed {
            // Difficult terrain costs same as normal
            stamina_cost = base_cost;
        }
        if has_clumsy {
            // Difficult terrain costs extra (3 instead of 2)
            stamina_cost = stamina_cost + 1;
        }
    }

    // Apply general stamina use trait modifiers
    if has_endurance {
        // 50% reduction (minimum 1) - every other step is free
        if stamina_cost > 1 {
            stamina_cost = stamina_cost / 2;
        }
    }
    if has_marathoner {
        // 25% reduction (affects higher costs)
        if stamina_cost >= 2 {
            stamina_cost = stamina_cost - stamina_cost / 4;
        }
    }
    if has_exhaustion_prone {
        // 50% increase
        stamina_cost = stamina_cost + stamina_cost / 2;
    }

    // Add heat and thirst penalties
    let heat_penalty = get_heat_stamina_penalty(connection_id);
    let thirst_penalty = get_thirst_stamina_penalty(connection_id);
    stamina_cost = stamina_cost + heat_penalty + thirst_penalty;

    // Apply encumbrance stamina modifier (100%, 125%, or 150%)
    let enc_modifier = get_encumbrance_stamina_modifier(char.name);
    if enc_modifier != 100 {
        stamina_cost = (stamina_cost * enc_modifier) / 100;
    }

    // Leg/foot injury stamina penalty
    let leg_penalty = get_leg_wound_penalty(char.name);
    let left_foot_penalty = get_character_body_part_penalty(char.name, "left foot");
    let right_foot_penalty = get_character_body_part_penalty(char.name, "right foot");
    let foot_penalty = if left_foot_penalty > right_foot_penalty { left_foot_penalty } else { right_foot_penalty };
    let mobility_penalty = if leg_penalty > foot_penalty { leg_penalty } else { foot_penalty };

    if mobility_penalty > 0 {
        // Scale: minor(10%)→+1, moderate(25%)→+2, severe(50%)→+4, critical(75%)→+6, disabled(100%)→+8
        let extra = (mobility_penalty + 12) / 13;
        stamina_cost += extra;
    }

    // Apply haste/slow buff modifiers
    if has_buff(char.name, "haste") {
        stamina_cost = (stamina_cost + 1) / 2;  // 50% reduction
    }
    if has_buff(char.name, "slow") {
        stamina_cost = stamina_cost * 2;  // 100% increase
    }

    // Show warning messages for penalties
    if heat_penalty > 0 {
        send_client_message(connection_id, "The heat is draining.");
    }
    if thirst_penalty > 0 {
        send_client_message(connection_id, "Your thirst is sapping your strength.");
    }
    if enc_level == "medium" {
        send_client_message(connection_id, "Your load slows you down.");
    }
    if enc_level == "heavy" {
        send_client_message(connection_id, "You struggle under the weight you carry.");
    }
    if mobility_penalty >= 50 {
        send_client_message(connection_id, "You hobble painfully on your injured leg.");
    } else if mobility_penalty >= 25 {
        send_client_message(connection_id, "You limp along, favoring your injured leg.");
    } else if mobility_penalty > 0 {
        send_client_message(connection_id, "Your leg injury slows you slightly.");
    }

    // Ensure minimum cost of 1
    if stamina_cost < 1 {
        stamina_cost = 1;
    }

    // Consume stamina (god mode and build mode bypass)
    if !char.god_mode && !is_in_build_mode(char.name, target_room_id) {
        char.stamina = char.stamina - stamina_cost;
        if char.stamina < 0 {
            char.stamina = 0;
        }
    }

    // Cancel fishing if active
    if is_fishing(connection_id) {
        cancel_fishing(connection_id);
        send_client_message(connection_id, "Your fishing line snaps as you walk away!");
    }

    // Check if sneaking - extra stamina cost
    let currently_sneaking = is_sneaking(char.name);
    if currently_sneaking && !char.god_mode && !is_in_build_mode(char.name, target_room_id) {
        stamina_cost += 1;
    }

    // Pathfinding passive: tracking >= 3 in wilderness reduces stamina cost by 1
    if target_room != () && target_room.flags.dirt_floor && !target_room.flags.city && !target_room.flags.indoors {
        let tracking_skill = get_skill_level(char.name, "tracking");
        if tracking_skill >= 3 && stamina_cost > 1 {
            stamina_cost = stamina_cost - 1;
        }
    }

    // Record departure for tracking system
    let norm_dir = normalize_direction(direction);
    record_departure(old_room_id, char.name, norm_dir, currently_sneaking);

    // Leave a blood trail if bleeding
    let total_bleeding = get_character_total_bleeding(char.name);
    if total_bleeding > 0 {
        add_blood_trail(old_room_id, char.name, total_bleeding, norm_dir);
    }

    // Announce departure to old room (sleeping players don't notice)
    // Sneaking characters don't broadcast departure
    if !currently_sneaking {
        broadcast_to_room_awake(old_room_id, char.name + " leaves heading " + direction + ".", char.name);
    }

    // Move the character
    char.current_room_id = target_room_id;
    save_character_data(char);
    set_player_character(connection_id, char);

    // Clear AFK status on movement
    if is_afk(connection_id) {
        clear_afk(connection_id);
        send_client_message(connection_id, "You are no longer AFK.");
    }

    // Announce arrival to new room (sleeping players don't notice)
    // Sneaking characters don't broadcast arrival
    let arrival_dir = get_opposite_direction(direction);
    if !currently_sneaking {
        broadcast_to_room_awake(target_room_id, char.name + " arrives from the " + arrival_dir + ".", char.name);
    }

    // Drag along any mobiles charmed by this player.
    propagate_charmed_mobs(char.name, old_room_id, target_room_id, direction);

    // Alertness passive: players with tracking >= 3 in the room may detect sneaking arrivals
    if currently_sneaking {
        let room_players = get_characters_in_room(target_room_id);
        for obs_name in room_players {
            if obs_name == char.name { continue; }
            let obs_tracking = get_skill_level(obs_name, "tracking");
            if obs_tracking >= 3 {
                // Perception check vs arriving sneaker's stealth score
                if !stealth_check_vs_player(char.name, obs_name, target_room_id) {
                    // Observer detects presence (does NOT break sneaker's stealth)
                    let obs_conn = find_player_connection_by_name(obs_name);
                    if obs_conn != "" {
                        send_client_message(obs_conn, "Your keen senses detect a presence arriving from the " + arrival_dir + ".");
                    }
                }
            }
        }
    }

    // Check if new room exists
    let new_room = target_room;
    if new_room == () {
        send_client_message(connection_id, "You step into the void...");
        return;
    }

    // Display the room
    display_room(target_room_id, connection_id, char.name);

    // ROOM_DEATH: instant kill on entry. god_mode + build_mode bypass.
    if new_room.flags.death && !char.god_mode && !is_in_build_mode(char.name, target_room_id) {
        send_client_message(connection_id, "\x1b[1;31mA terrible fate befalls you here!\x1b[0m");
        apply_room_death(connection_id);
        return;
    }

    // Swimming position and wet status from water
    if (is_shallow || is_deep || is_underwater) && !char.god_mode {
        let has_boat = player_has_item_flag(char.name, "boat");
        if !has_boat {
            // Set swimming position
            if char.position != "swimming" {
                char.position = "swimming";
                save_character_data(char);
                set_player_character(connection_id, char);
            }
            // Direct water immersion - waterproof items don't help
            if !char.is_wet || char.wet_level < 100 {
                char.wet_level = 100;
                char.is_wet = true;
                save_character_data(char);
                set_player_character(connection_id, char);
            }
        }
    }

    // Reset to standing when leaving water
    if !(is_shallow || is_deep || is_underwater) && char.position == "swimming" {
        char.position = "standing";
        save_character_data(char);
        set_player_character(connection_id, char);
    }

    // Swimming XP from water traversal
    if is_shallow || is_deep || is_underwater {
        let swim_xp = if is_underwater { 15 } else if is_deep { 10 } else { 5 };
        let leveled = add_skill_experience(char.name, "swimming", swim_xp);
        if leveled {
            send_client_message(connection_id, "\x1b[1;33mYour swimming skill has improved!\x1b[0m");
        }
    }

    // Underwater entry warning
    if is_underwater && !char.god_mode && !has_buff(char.name, "water_breathing") {
        send_client_message(connection_id, "\x1b[1;36mYou hold your breath as you plunge underwater.\x1b[0m");
    }

    // Check for traps in the room (build mode bypasses)
    let room_traps = get_room_traps(target_room_id);
    if room_traps.len() > 0 && !is_in_build_mode(char.name, target_room_id) {
        for trap_idx in range(0, room_traps.len()) {
            let trap = room_traps[trap_idx];
            // Don't trigger on trap owner
            if trap["owner_name"] == char.name { continue; }
            // Tracking-based detection check
            let perception = calculate_perception_score_player(char.name);
            if perception >= trap["detect_difficulty"] {
                send_client_message(connection_id, "You notice a " + trap["trap_type"] + " trap just in time!");
            } else {
                // Trap triggers!
                let effect = trap["effect"];
                let damage = trap["damage"];
                let trap_type = trap["trap_type"];
                if effect == "damage" && damage > 0 {
                    char.hp = char.hp - damage;
                    save_character_data(char);
                    set_player_character(connection_id, char);
                    send_client_message(connection_id, "A " + trap_type + " trap hits you for " + damage + " damage!");
                    broadcast_to_room_awake(target_room_id, char.name + " triggers a " + trap_type + " trap!", char.name);
                }
                if effect == "alarm" {
                    broadcast_to_room_awake(target_room_id, "ALARM! " + char.name + " triggers an alarm trap!");
                    send_client_message(connection_id, "You trigger an alarm!");
                }
                if effect == "slow" {
                    apply_buff(char.name, "slow", 1, 30, "snare trap");
                    send_client_message(connection_id, "A snare trap catches you! Your movement is slowed.");
                    broadcast_to_room_awake(target_room_id, char.name + " is caught in a snare trap!", char.name);
                }
                if effect == "poison" {
                    if damage > 0 {
                        char.hp = char.hp - damage;
                        save_character_data(char);
                        set_player_character(connection_id, char);
                        send_client_message(connection_id, "A poison dart hits you for " + damage + " damage!");
                    }
                    apply_ongoing_effect(char.name, "poison", 3, 2, "body");
                    send_client_message(connection_id, "You feel poison coursing through your veins!");
                    broadcast_to_room_awake(target_room_id, char.name + " triggers a poison dart trap!", char.name);
                }
                decrement_trap_charge(target_room_id, trap_idx);
            }
        }
    }

    // Fire on_greet triggers for all NPCs in the new room
    // Sneaking characters: check stealth vs each mobile, only fire triggers if detected
    let mobiles = get_mobiles_in_room(target_room_id);
    for mobile in mobiles {
        if currently_sneaking {
            // Stealth check vs mobile - detected means break stealth and fire trigger
            if !stealth_check_vs_mobile(char.name, mobile.id, target_room_id) {
                // Detected!
                break_stealth(char.name);
                currently_sneaking = false;
                broadcast_to_room_awake(target_room_id, char.name + " is spotted sneaking in!", char.name);
                send_client_message(connection_id, mobile.name + " notices you!");
                fire_mobile_trigger(mobile.id, "on_greet", connection_id, #{
                    "char_name": char.name,
                    "direction": direction,
                    "source_room": old_room_id,
                    "mobile_name": mobile.name
                });
            }
            // If not detected, skip the trigger
        } else {
            fire_mobile_trigger(mobile.id, "on_greet", connection_id, #{
                "char_name": char.name,
                "direction": direction,
                "source_room": old_room_id,
                "mobile_name": mobile.name
            });
        }
    }

    // Move followers who are in the old room
    let followers = get_followers_in_room(char.name, old_room_id);
    for follower_name in followers {
        move_follower(follower_name, char.name, direction);
    }

    // Explicit return to prevent trigger return values from being output
    return;
}
