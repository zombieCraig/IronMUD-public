// scripts/triggers/garden_atmosphere.rhai
// Combined weather and time trigger for garden rooms
//
// Usage:
//   redit trigger add on_weather_change garden_atmosphere
//   redit trigger add on_time_change garden_atmosphere
//
// This trigger combines weather and time of day into a single dynamic
// description, preventing them from overwriting each other.

fn run_trigger(room_id, connection_id, context) {
    // Get current conditions from game time
    let game_time = get_game_time();
    if game_time == () { return "continue"; }

    let time_of_day = game_time.get("time_of_day");
    let weather = game_time.get("weather");

    if time_of_day == () { time_of_day = "day"; }
    if weather == () { weather = "clear"; }

    let time_lower = time_of_day.to_lower();
    let weather_lower = weather.to_lower();

    // Build description based on conditions
    let desc = "";

    // Check for severe weather first (overrides time-based descriptions)
    if weather_lower == "thunderstorm" {
        desc = "Thunder rumbles overhead as rain lashes the garden. Lightning illuminates the scene in brief, stark flashes, and the stone lanterns flicker in the storm's fury.";
    } else if weather_lower == "blizzard" {
        desc = "Driving snow obscures the garden. The wind howls through the branches, piling drifts against the stone lanterns.";
    } else if weather_lower == "heavy_rain" {
        desc = "Heavy rain drums on the leaves and churns the surface of the koi pond. The gravel paths have become small streams.";
    } else {
        // Combine time and weather for milder conditions
        desc = get_time_description(time_lower);

        let weather_addition = get_weather_addition(weather_lower, time_lower);
        if weather_addition != "" {
            desc += " " + weather_addition;
        }
    }

    if desc != "" {
        set_room_dynamic_desc(room_id, desc);
    } else {
        clear_room_dynamic_desc(room_id);
    }

    return "continue";
}

fn get_time_description(time_lower) {
    if time_lower == "dawn" {
        return "Pale golden light filters through the branches as dawn breaks. Birdsong fills the air, and dewdrops glisten on every surface.";
    }
    if time_lower == "morning" {
        return "Warm morning sunlight bathes the garden in soft gold. The koi swim lazily near the surface.";
    }
    if time_lower == "noon" {
        return "The midday sun casts short shadows beneath the trees. The garden drowses in the warmth.";
    }
    if time_lower == "afternoon" {
        return "Long afternoon shadows stretch across the gravel paths. The light has turned honey-gold.";
    }
    if time_lower == "dusk" {
        return "The sky blazes with sunset colors reflected in the still pond. The stone lanterns begin to glow.";
    }
    if time_lower == "evening" {
        return "The stone lanterns cast pools of warm light across the darkening garden. Fireflies drift above the pond.";
    }
    if time_lower == "night" {
        return "Moonlight silvers the garden. The lanterns glow like earthbound stars amid the shadows.";
    }
    return "";
}

fn get_weather_addition(weather_lower, time_lower) {
    // Rain
    if weather_lower == "light_rain" || weather_lower == "rain" {
        if time_lower == "night" || time_lower == "evening" {
            return "Rain patters softly on the leaves, the droplets catching the lantern light.";
        }
        return "Gentle rain patters on the leaves and dimples the pond's surface.";
    }

    // Snow
    if weather_lower == "light_snow" || weather_lower == "snow" {
        if time_lower == "night" {
            return "Snowflakes drift down like frozen starlight, dusting the garden in white.";
        }
        return "Delicate snowflakes spiral down through the branches.";
    }

    // Fog
    if weather_lower == "foggy" {
        if time_lower == "dawn" || time_lower == "morning" {
            return "Morning mist clings to the ground, wrapping the garden in mystery.";
        }
        if time_lower == "night" || time_lower == "evening" {
            return "Fog drifts through the lantern light, creating halos of gold in the darkness.";
        }
        return "A soft mist hangs in the air, muting colors and sounds.";
    }

    // Overcast
    if weather_lower == "overcast" {
        return "Gray clouds diffuse the light into a soft, even glow.";
    }

    // Partly cloudy
    if weather_lower == "partly_cloudy" {
        if time_lower == "night" {
            return "Clouds drift across the moon, sending shadows racing through the garden.";
        }
        return "Clouds drift across the sun, sending shadows chasing across the paths.";
    }

    // Clear weather - no addition needed
    return "";
}
