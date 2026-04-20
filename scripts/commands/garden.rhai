// scripts/commands/garden.rhai
// garden - View status of all plants in the current room

fn get_water_bar(level) {
    if level >= 80.0 { return "[||||||||..] " + level + "%"; }
    if level >= 60.0 { return "[||||||....] " + level + "%"; }
    if level >= 40.0 { return "[||||......] " + level + "%"; }
    if level >= 20.0 { return "[||........] " + level + "%"; }
    return "[..........] " + level + "%";
}

fn get_water_status(level) {
    if level >= 80.0 { return "Well-watered"; }
    if level >= 50.0 { return "Adequate"; }
    if level >= 20.0 { return "Needs water"; }
    return "Critically dry!";
}

fn get_health_color(health) {
    if health >= 80.0 { return "Healthy"; }
    if health >= 50.0 { return "Fair"; }
    if health >= 30.0 { return "Poor"; }
    return "Critical!";
}

fn stage_label(stage) {
    if stage == "seed" { return "Seed"; }
    if stage == "sprout" { return "Sprout"; }
    if stage == "seedling" { return "Seedling"; }
    if stage == "growing" { return "Growing"; }
    if stage == "mature" { return "Mature"; }
    if stage == "flowering" { return "Flowering"; }
    if stage == "wilting" { return "Wilting"; }
    if stage == "dead" { return "Dead"; }
    return stage;
}

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to do that.");
        return;
    }

    let plants = get_plants_in_room(char.current_room_id);
    if plants.len() == 0 {
        send_client_message(connection_id, "There are no plants growing here.");
        return;
    }

    let output = "=== Garden Status ===\n";
    let idx = 1;

    for plant in plants {
        let proto = get_plant_prototype_by_vnum(plant.prototype_vnum);
        let name = "Unknown Plant";
        if proto != () {
            name = proto.name;
        }

        let stage = stage_label(plant.stage);
        let water_pct = plant.water_level;
        if water_pct > 100.0 { water_pct = 100.0; }
        let water_int = water_pct;
        // Round to integer for display
        water_int = (water_pct * 10) / 10;

        let health_pct = plant.health;
        let health_int = (health_pct * 10) / 10;

        // Build status line
        let potted = "";
        if plant.is_potted {
            potted = " (potted)";
        }

        output += "" + idx + ". " + name + potted + " [" + stage + "]\n";
        output += "   Water: " + get_water_status(water_pct) + " (" + water_int + "%) | Health: " + get_health_color(health_pct) + " (" + health_int + "%)\n";

        // Status indicators
        let indicators = [];
        if plant.fertilized {
            indicators.push("Fertilized");
        }
        if plant.infestation != "none" {
            let severity = "";
            if plant.infestation_severity >= 0.7 { severity = "severe "; }
            else if plant.infestation_severity >= 0.4 { severity = "moderate "; }
            else { severity = "mild "; }
            indicators.push(severity + plant.infestation + " infestation");
        }
        if plant.stage == "flowering" {
            indicators.push("Ready to harvest!");
        }
        if plant.stage == "wilting" {
            indicators.push("Wilting - harvest soon!");
        }
        if plant.stage == "dead" {
            indicators.push("Dead - uproot to clear");
        }
        if water_pct < 20.0 {
            indicators.push("Needs water urgently!");
        }

        if indicators.len() > 0 {
            output += "   Status: " + indicators.join(" | ") + "\n";
        }

        output += "   Planted by: " + plant.planter_name;
        if plant.times_harvested > 0 {
            output += " | Harvested " + plant.times_harvested + " time(s)";
        }
        output += "\n";

        idx += 1;
    }

    send_client_message(connection_id, output);
}
