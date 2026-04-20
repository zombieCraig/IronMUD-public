// scripts/triggers/garden_weather.rhai
// Weather trigger for garden rooms - sets dynamic descriptions based on weather
//
// Usage: redit trigger add on_weather_change garden_weather
//
// This trigger updates the room's dynamic_desc based on weather conditions,
// adding atmospheric details that complement the base and seasonal descriptions.

fn run_trigger(room_id, connection_id, context) {
    let new_weather = context.get("new_weather");
    if new_weather == () { return "continue"; }

    let weather_lower = new_weather.to_lower();
    let desc = get_weather_description(weather_lower);

    if desc == "" {
        clear_room_dynamic_desc(room_id);
    } else {
        set_room_dynamic_desc(room_id, desc);
    }

    return "continue";
}

fn get_weather_description(weather) {
    if weather == "light_rain" || weather == "rain" {
        return "Gentle rain patters on the leaves and dimples the surface of the koi pond. The air smells fresh and green.";
    }
    if weather == "heavy_rain" {
        return "Heavy rain cascades from the sky, drumming loudly on leaves and turning the gravel paths into small streams. The koi have retreated to the depths of the pond.";
    }
    if weather == "thunderstorm" {
        return "Thunder rumbles overhead as rain lashes the garden. Lightning illuminates the scene in brief, stark flashes, and the stone lanterns seem to glow in the darkness between strikes.";
    }
    if weather == "light_snow" || weather == "snow" {
        return "Delicate snowflakes drift down through the branches, dusting the garden in white. The world grows hushed and still.";
    }
    if weather == "blizzard" {
        return "Driving snow obscures the far end of the garden. The wind howls through the bare branches, and drifts are beginning to form against the stone lanterns.";
    }
    if weather == "foggy" {
        return "A soft mist hangs over the garden, wrapping the trees in gauze and muting all sound. The stone lanterns emerge from the fog like silent guardians.";
    }
    if weather == "overcast" {
        return "Gray clouds blanket the sky, casting the garden in soft, diffused light. The colors seem muted but peaceful.";
    }
    if weather == "partly_cloudy" {
        return "Clouds drift across the sun, sending shadows chasing across the garden paths.";
    }
    // Clear or unknown weather - return empty to clear dynamic desc
    return "";
}
