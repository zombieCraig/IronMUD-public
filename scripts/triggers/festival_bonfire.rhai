// scripts/triggers/festival_bonfire.rhai
// Monthly trigger for festival bonfire events
// Usage: redit trigger add on_month_change festival_bonfire
//
// Activates on months 4, 8, and 12 - sets city flag (bonfire light)
// and updates room description for the festival.
//
// The city flag prevents the room from being dark at night, simulating
// the illumination from the bonfire.

fn run_trigger(room_id, connection_id, context) {
    let new_month = context.get("new_month");
    if new_month == () { return "continue"; }

    let month = parse_int(new_month);
    let is_festival = (month == 4 || month == 8 || month == 12);

    if is_festival {
        // Festival starts - light the bonfire!
        set_room_flag(room_id, "city", true);
        set_room_dynamic_desc(room_id,
            "A massive bonfire blazes in the center of the square, its flames reaching toward the stars. Colorful banners flutter between buildings, and the air is thick with music, laughter, and the aroma of festival foods. Villagers dance around the fire while merchants hawk their wares from gaily decorated stalls."
        );
    } else {
        // Festival ends - back to normal
        set_room_flag(room_id, "city", false);
        clear_room_dynamic_desc(room_id);
    }

    return "continue";
}
