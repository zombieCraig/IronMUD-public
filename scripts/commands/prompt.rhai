// scripts/commands/prompt.rhai
//
//   prompt                  - show the current prompt and how to change it
//   prompt simple           - preset: just '>'
//   prompt verbose          - preset: every pool that applies to you
//   prompt tokens           - list every token, marked for what applies to you
//   prompt <format>         - a custom prompt, e.g. %h/%H hp %s/%S st %t >
//   prompt default          - drop the custom format, back to the preset
//
// The tokens themselves live in Rust (src/prompt.rs) and are listed through
// prompt_token_lines(), so this script never has to know what they are — a
// token added there shows up here with no edit.

fn run_command(args, connection_id) {
    let char = get_player_character(connection_id);
    if char == () {
        send_client_message(connection_id, "You must be logged in to change prompt settings.");
        return;
    }

    if args == "" {
        show_current(char, connection_id);
        return;
    }

    let arg = args.to_lower();

    if arg == "simple" || arg == "verbose" {
        char.prompt_mode = arg;
        // A stored custom format outranks the preset, so switching presets
        // has to clear it — otherwise 'prompt simple' would appear to do
        // nothing at all.
        char.prompt_format = "";
        save_character_data(char);
        set_player_character(connection_id, char);
        send_client_message(connection_id, "Prompt set to " + arg + " mode.");
        return;
    }

    if arg == "default" || arg == "clear" || arg == "reset" {
        if char.prompt_format == "" {
            send_client_message(connection_id, "You have no custom prompt to clear.");
            return;
        }
        char.prompt_format = "";
        save_character_data(char);
        set_player_character(connection_id, char);
        send_client_message(connection_id, "Custom prompt cleared. Back to " + current_mode(char) + " mode.");
        return;
    }

    if arg == "tokens" || arg == "help" {
        show_tokens(connection_id);
        return;
    }

    // Anything with a % in it is a format string. Anything else is a typo,
    // and guessing would store nonsense as somebody's prompt.
    if !args.contains("%") {
        send_client_message(
            connection_id,
            "Unknown prompt setting. Use 'simple', 'verbose', 'default', 'tokens', or a format containing % tokens."
        );
        return;
    }

    let problem = validate_prompt_format(args);
    if problem != "" {
        send_client_message(connection_id, problem);
        return;
    }

    char.prompt_format = args;
    save_character_data(char);
    set_player_character(connection_id, char);
    send_client_message(connection_id, "Prompt set. Here it comes:");
}

fn current_mode(char) {
    if char.prompt_mode == "" { return "simple"; }
    return char.prompt_mode;
}

fn show_current(char, connection_id) {
    let output = "\x1b[1;36mPrompt\x1b[0m\n";
    if char.prompt_format != "" {
        output += "Current: custom — " + char.prompt_format + "\n";
    } else {
        output += "Current: " + current_mode(char) + "\n";
    }
    output += "\n";
    output += "  prompt simple      just '>'\n";
    output += "  prompt verbose     every pool that applies to you\n";
    output += "  prompt tokens      list the tokens you can use\n";
    output += "  prompt <format>    a prompt of your own\n";
    output += "  prompt default     drop a custom prompt, back to the preset\n";
    output += "\n";
    output += "Example: \x1b[33mprompt %h/%H hp %s/%S st %t >\x1b[0m\n";
    output += "The verbose preset is: \x1b[33m" + default_prompt_format("verbose") + "\x1b[0m\n";
    output += "\nEquipped items with on_prompt triggers add to the %{extra} token.";
    send_client_message(connection_id, output);
}

fn show_tokens(connection_id) {
    let output = "\x1b[1;36mPrompt tokens\x1b[0m\n";
    output += "Lines marked '-' do not apply to you right now.\n";
    output += "'%{name}' tokens draw a whole bracketed, coloured group; the short\n";
    output += "ones draw a bare number you can lay out yourself. '%%' is a literal %.\n\n";
    for line in prompt_token_lines(connection_id) {
        output += line + "\n";
    }
    send_client_message(connection_id, output);
}
