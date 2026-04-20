// scripts/triggers/smart_watch.rhai
// on_examine trigger for smart watch items
// Shows precise game time and weather when examined

fn run_trigger(item_id, connection_id, context) {
    let time = get_game_time();

    // Format hour with leading zero
    let hour_str = "";
    if time.hour < 10 {
        hour_str = "0" + time.hour;
    } else {
        hour_str = "" + time.hour;
    }

    // Convert temperature to Fahrenheit
    let temp_c = time.temperature;
    let temp_f = (temp_c * 9 / 5) + 32;

    let output = "\n=== Smart Watch Display ===\n";
    output += "Time: " + hour_str + ":00\n";
    output += "Date: Day " + time.day + ", Month " + time.month + ", Year " + time.year + "\n";
    output += "Season: " + capitalize(time.season) + "\n";
    output += "Weather: " + capitalize_first(time.weather_desc) + "\n";
    output += "Temperature: " + temp_c + "C (" + temp_f + "F)\n";

    // Check thirst status
    let thirst = get_character_thirst(connection_id);
    if thirst != () {
        output += "Hydration: " + thirst.percent + "%\n";
    }

    output += "===========================";

    send_client_message(connection_id, output);

    return "continue";
}

fn capitalize(s) {
    if s == "" { return s; }
    let first = s.sub_string(0, 1);
    let rest = s.sub_string(1, s.len() - 1);
    // Convert first char to uppercase
    if first == "a" { first = "A"; }
    if first == "s" { first = "S"; }
    if first == "w" { first = "W"; }
    if first == "c" { first = "C"; }
    if first == "m" { first = "M"; }
    if first == "f" { first = "F"; }
    if first == "h" { first = "H"; }
    return first + rest;
}

fn capitalize_first(s) {
    if s == "" { return s; }
    let first = s.sub_string(0, 1);
    let rest = s.sub_string(1, s.len() - 1);
    // Convert first char to uppercase
    if first == "a" { first = "A"; }
    if first == "b" { first = "B"; }
    if first == "c" { first = "C"; }
    if first == "d" { first = "D"; }
    if first == "e" { first = "E"; }
    if first == "f" { first = "F"; }
    if first == "g" { first = "G"; }
    if first == "h" { first = "H"; }
    if first == "i" { first = "I"; }
    if first == "l" { first = "L"; }
    if first == "m" { first = "M"; }
    if first == "n" { first = "N"; }
    if first == "o" { first = "O"; }
    if first == "p" { first = "P"; }
    if first == "r" { first = "R"; }
    if first == "s" { first = "S"; }
    if first == "t" { first = "T"; }
    if first == "w" { first = "W"; }
    return first + rest;
}
