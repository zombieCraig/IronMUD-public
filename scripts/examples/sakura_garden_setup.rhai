// scripts/examples/sakura_garden_setup.rhai
// Example: Creating a room with seasonal descriptions
//
// This script demonstrates how to set up a room with seasonal descriptions
// that automatically change based on the current game season.
//
// To use: Run this script from a builder command or adapt for your area setup.

fn setup_sakura_garden(connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in.");
        return;
    }

    // Create the room
    let room = create_room(
        "Sakura Garden",
        "A serene Japanese garden surrounds you, centered around ancient sakura trees. Stone lanterns line a winding gravel path, and a small wooden bridge arches over a koi pond. The air is peaceful and still."
    );

    if room == () {
        send_client_message(connection_id, "Failed to create room.");
        return;
    }

    let room_id = room.id;

    // Set seasonal descriptions - these automatically display based on current season
    set_room_spring_desc(room_id,
        "The sakura trees are in full bloom, their branches heavy with delicate pink blossoms. Petals drift lazily through the air like pink snow, carpeting the ground in soft color."
    );

    set_room_summer_desc(room_id,
        "Lush green leaves shade the garden from the warm sun. The sakura trees provide cool refuge, their dense canopy filtering dappled sunlight onto the path below."
    );

    set_room_autumn_desc(room_id,
        "The sakura leaves have turned brilliant shades of gold and crimson. They fall in gentle spirals, gathering in rustling drifts along the garden paths."
    );

    set_room_winter_desc(room_id,
        "Bare sakura branches reach toward the gray sky like dark calligraphy. A light frost clings to the wood, and the garden lies quiet beneath winter's hush."
    );

    // Add extra descriptions for interactive elements
    add_room_extra_desc(room_id, "sakura cherry trees tree blossoms",
        "The ancient sakura trees have stood in this garden for generations. Their gnarled trunks speak of age and wisdom, while their branches reach gracefully toward the sky. In spring, they burst into spectacular bloom; in winter, they stand as silent sentinels."
    );

    add_room_extra_desc(room_id, "pond koi water fish",
        "Crystal clear water reveals colorful koi swimming lazily beneath the surface. Orange, white, and black fish glide between smooth stones and trailing water plants. Occasionally one rises to make ripples on the surface."
    );

    add_room_extra_desc(room_id, "bridge wooden",
        "A graceful wooden bridge arches over the narrowest part of the koi pond. Its red-painted rails have weathered to a soft vermillion, and the planks are worn smooth by countless footsteps."
    );

    add_room_extra_desc(room_id, "lanterns stone lantern",
        "Traditional stone lanterns are placed at intervals along the path. Their weathered gray surfaces are spotted with lichen, and small openings hint at where candles might be placed for evening illumination."
    );

    add_room_extra_desc(room_id, "path gravel",
        "Fine white gravel has been carefully raked into flowing patterns that suggest water or clouds. The crunch underfoot is oddly satisfying, and the patterns seem to invite contemplation."
    );

    // Set a vnum for easy reference
    set_room_vnum(room_id, "example:sakura_garden");

    // Add atmosphere triggers for dynamic descriptions (combined time + weather)
    add_room_trigger(room_id, "on_weather_change", "garden_atmosphere");
    add_room_trigger(room_id, "on_time_change", "garden_atmosphere");

    // Move the player to the new room
    let old_room = char.current_room_id;
    char.current_room_id = room_id;
    save_character_data(char);
    set_player_character(connection_id, char);

    send_client_message(connection_id, "Created Sakura Garden with seasonal descriptions!");
    send_client_message(connection_id, "Room ID: " + room_id);
    send_client_message(connection_id, "Vnum: example:sakura_garden");
    send_client_message(connection_id, "");
    send_client_message(connection_id, "Current season: " + get_current_season());
    send_client_message(connection_id, "Use 'look' to see the seasonal description in action.");
    send_client_message(connection_id, "Try 'look sakura', 'look pond', 'look bridge' for extra descriptions.");
}

// Entry point - can be called from a command or trigger
fn run_command(args, connection_id) {
    setup_sakura_garden(connection_id);
}
