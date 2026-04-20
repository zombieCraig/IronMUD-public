// scripts/triggers/garden_time.rhai
// Time of day trigger for garden rooms - sets dynamic descriptions based on time
//
// Usage: redit trigger add on_time_change garden_time
//
// This trigger updates the room's dynamic_desc based on time of day,
// adding atmospheric lighting and mood details.
//
// Note: If using with garden_weather, the weather trigger will overwrite
// the time description when weather changes. For best results, combine
// time and weather logic in a single trigger, or use garden_atmosphere.rhai

fn run_trigger(room_id, connection_id, context) {
    let new_time = context.get("new_time");
    if new_time == () { return "continue"; }

    let time_lower = new_time.to_lower();

    if time_lower == "dawn" {
        set_room_dynamic_desc(room_id,
            "Pale golden light filters through the branches as dawn breaks. Birdsong fills the air, and dewdrops glisten on every surface like scattered diamonds."
        );
    } else if time_lower == "morning" {
        set_room_dynamic_desc(room_id,
            "Warm morning sunlight bathes the garden in soft gold. The koi are active near the surface, and the air is fresh with the scent of green growing things."
        );
    } else if time_lower == "noon" {
        set_room_dynamic_desc(room_id,
            "The midday sun hangs directly overhead, casting short shadows beneath the trees. The garden drowses in the warmth, and even the koi have retreated to cooler depths."
        );
    } else if time_lower == "afternoon" {
        set_room_dynamic_desc(room_id,
            "Long afternoon shadows stretch across the gravel paths. The light has turned honey-gold, painting everything in warm tones."
        );
    } else if time_lower == "dusk" {
        set_room_dynamic_desc(room_id,
            "The sky blazes with sunset colors reflected in the still pond. Shadows deepen beneath the trees, and the stone lanterns begin to glow with soft light."
        );
    } else if time_lower == "evening" {
        set_room_dynamic_desc(room_id,
            "The stone lanterns cast pools of warm light across the darkening garden. Fireflies drift lazily above the pond, their soft glow mirrored in the water."
        );
    } else if time_lower == "night" {
        set_room_dynamic_desc(room_id,
            "Moonlight silvers the garden, transforming it into a realm of shadow and pale light. The lanterns glow like earthbound stars, and the only sound is the gentle splash of koi."
        );
    } else {
        // Unknown time - clear to be safe
        clear_room_dynamic_desc(room_id);
    }

    return "continue";
}
