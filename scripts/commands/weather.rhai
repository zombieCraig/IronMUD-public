// weather - Display current time, temperature, and weather conditions

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to check the weather.");
        return;
    }

    // Get room to check for indoor/special conditions
    let room = get_room_data(char.current_room_id);
    if room == () {
        send_client_message(connection_id, "You are in a void.");
        return;
    }

    let flags = room.flags;
    let time = get_game_time();

    let output = "";

    // Time of day and date
    output += "It is currently " + time.time_of_day_desc + ".\n";
    output += "The date is Day " + time.day + " of Month " + time.month + ", Year " + time.year + ".\n";
    output += "The season is " + capitalize(time.season) + ".\n\n";

    // Weather and temperature based on room conditions
    let effective_climate = get_effective_climate_controlled(char.current_room_id);
    if effective_climate {
        // Sealed environment - no outside weather information
        output += "This environment is sealed from outside conditions.\n";
        output += "Weather and temperature are locally controlled.";
    } else if flags.always_hot {
        output += "The heat here is oppressive and overwhelming.\n";
        output += "You feel as though you could cook an egg on the ground.";
    } else if flags.always_cold {
        output += "The cold here bites to the bone.\n";
        output += "Your breath forms clouds in the frigid air.";
    } else if flags.indoors {
        output += "You are indoors, but you can hear ";
        output += get_weather_sound(time.weather) + ".\n";
        let room_temp = get_room_temperature(char.current_room_id);
        if room_temp != () {
            output += "It feels " + room_temp.temperature_desc + " in here.";
        } else {
            output += "It feels " + time.temperature_desc + " in here.";
        }
    } else {
        output += "The weather is " + time.weather_desc + ".\n";
        output += "It is " + time.temperature_desc + " outside.";

        // Add temperature in degrees
        let temp = time.temperature;
        let temp_f = (temp * 9 / 5) + 32;
        output += " (" + temp + "C / " + temp_f + "F)";
    }

    // Check thirst status
    let thirst = get_character_thirst(connection_id);
    if thirst != () {
        output += "\n\n";
        if thirst.percent > 75 {
            output += "You feel well hydrated.";
        } else if thirst.percent > 50 {
            output += "You could use a drink soon.";
        } else if thirst.percent > 25 {
            output += "You are quite thirsty.";
        } else if thirst.percent > 10 {
            output += "You are very thirsty!";
        } else {
            output += "You are dangerously dehydrated!";
        }
    }

    send_client_message(connection_id, output);
}

fn get_weather_sound(weather) {
    if weather == "clear" { return "nothing but silence"; }
    if weather == "partlycloudy" { return "a light breeze"; }
    if weather == "cloudy" { return "the wind picking up"; }
    if weather == "overcast" { return "the wind howling"; }
    if weather == "lightrain" { return "a gentle patter of rain"; }
    if weather == "rain" { return "steady rainfall"; }
    if weather == "heavyrain" { return "rain pounding down"; }
    if weather == "thunderstorm" { return "thunder rumbling"; }
    if weather == "lightsnow" { return "light snowfall"; }
    if weather == "snow" { return "snow falling heavily"; }
    if weather == "blizzard" { return "howling blizzard winds"; }
    if weather == "fog" { return "an eerie stillness"; }
    return "the weather outside";
}

fn capitalize(s) {
    if s == "" { return s; }
    let first = s.sub_string(0, 1);
    let rest = s.sub_string(1, s.len() - 1);
    // Convert first char to uppercase
    if first == "a" { first = "A"; }
    if first == "s" { first = "S"; }
    if first == "w" { first = "W"; }
    return first + rest;
}
