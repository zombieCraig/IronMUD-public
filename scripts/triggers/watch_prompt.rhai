// scripts/triggers/watch_prompt.rhai
// on_prompt trigger for watch items
// Shows the current game time in the prompt when equipped
//
// To use: Add an on_prompt trigger to a watch item:
//   oedit <item> trigger add on_prompt watch_prompt
//
// When equipped and prompt mode is "verbose", this will add
// the current game time to the prompt, e.g.:
//   [HP:100/100] [14:00] >

fn run_trigger(item_id, connection_id, context) {
    let time = get_game_time();

    // Format hour with leading zero
    let hour_str = "";
    if time.hour < 10 {
        hour_str = "0" + time.hour;
    } else {
        hour_str = "" + time.hour;
    }

    // Return formatted time string to append to prompt
    return "[" + hour_str + ":00] ";
}
