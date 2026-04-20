// scripts/commands/prompt.rhai
// Toggle between simple (>) and verbose ([HP:100/100] >) prompts

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to change prompt settings.");
        return;
    }

    let current_mode = char.prompt_mode;
    if current_mode == "" { current_mode = "simple"; }

    if args == "" {
        // Show current setting and usage
        let output = "Current prompt mode: " + current_mode + "\n\n";
        output += "Usage: prompt <simple|verbose>\n\n";
        output += "  simple  - Just shows '>'\n";
        output += "  verbose - Shows [HP:x/y] [ST:x/y] with color coding\n";
        output += "            If mana-enabled, also shows [MP:x/y]\n\n";
        output += "  HP colors: Green (70%+), Yellow (30-69%), Red (<30%)\n";
        output += "  ST colors: Cyan (70%+), Blue (30-69%), Magenta (<30%)\n";
        output += "  MP colors: Bright Blue (70%+), Blue (30-69%), Magenta (<30%)\n\n";
        output += "Equipped items with on_prompt triggers can also add\n";
        output += "information to the verbose prompt (e.g., a watch showing time).";
        send_client_message(connection_id, output);
        return;
    }

    let arg = args.to_lower();

    if arg == "simple" {
        char.prompt_mode = "simple";
        save_character_data(char);
        set_player_character(connection_id, char);
        send_client_message(connection_id, "Prompt set to simple mode.");
        return;
    }

    if arg == "verbose" {
        char.prompt_mode = "verbose";
        save_character_data(char);
        set_player_character(connection_id, char);
        send_client_message(connection_id, "Prompt set to verbose mode.");
        return;
    }

    send_client_message(connection_id, "Unknown prompt mode. Use 'simple' or 'verbose'.");
}
