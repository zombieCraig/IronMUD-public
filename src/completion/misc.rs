use super::consts::*;
use super::helpers::*;
use super::types::*;

/// Context-aware completion for set command
pub(super) fn complete_set(words: &[&str], completing_word: bool, is_builder: bool) -> CompletionResult {
    let partial = get_partial(words, completing_word);

    // Build available settings based on permissions
    let mut available: Vec<&str> = SET_SUBCOMMANDS_BASE.to_vec();
    if is_builder {
        available.extend(SET_SUBCOMMANDS_BUILDER);
    }

    match words.len() {
        // set - show all available settings
        1 if !completing_word => all_static(&available, CompletionType::SetSubcommand),
        // set <partial_setting> - complete setting name
        2 if completing_word => filter_static(&available, &partial, CompletionType::SetSubcommand),
        // set <setting> - show on/off options
        2 if !completing_word => all_static(SET_TOGGLE_VALUES, CompletionType::SetSubcommand),
        // set <setting> <partial_value> - complete on/off
        3 if completing_word => filter_static(SET_TOGGLE_VALUES, &partial, CompletionType::SetSubcommand),
        _ => CompletionResult::empty(),
    }
}

/// Context-aware completion for motd command
pub(super) fn complete_motd(words: &[&str], completing_word: bool) -> CompletionResult {
    let partial = get_partial(words, completing_word);

    match words.len() {
        // motd - show all subcommands
        1 if !completing_word => all_static(MOTD_SUBCOMMANDS, CompletionType::MotdSubcommand),
        // motd <partial_subcommand> - complete subcommand
        2 if completing_word => filter_static(MOTD_SUBCOMMANDS, &partial, CompletionType::MotdSubcommand),
        _ => CompletionResult::empty(),
    }
}

/// Context-aware completion for bugs command
pub(super) fn complete_bugs(words: &[&str], completing_word: bool) -> CompletionResult {
    let partial = get_partial(words, completing_word);

    match words.len() {
        // bugs - show subcommands
        1 if !completing_word => all_static(BUGS_SUBCOMMANDS, CompletionType::BugsSubcommand),
        // bugs <partial> - complete subcommand
        2 if completing_word => filter_static(BUGS_SUBCOMMANDS, &partial, CompletionType::BugsSubcommand),
        // bugs list - show status filters
        2 if !completing_word && words[1].to_lowercase() == "list" => {
            all_static(BUG_STATUS_FILTERS, CompletionType::BugStatusFilter)
        }
        // bugs list <partial> - complete status filter
        3 if completing_word && words[1].to_lowercase() == "list" => {
            filter_static(BUG_STATUS_FILTERS, &partial, CompletionType::BugStatusFilter)
        }
        // bugs status <#> - show status values
        3 if !completing_word && words[1].to_lowercase() == "status" => {
            all_static(BUG_STATUS_VALUES, CompletionType::BugStatusFilter)
        }
        // bugs status <#> <partial> - complete status value
        4 if completing_word && words[1].to_lowercase() == "status" => {
            filter_static(BUG_STATUS_VALUES, &partial, CompletionType::BugStatusFilter)
        }
        // bugs priority <#> - show priority values
        3 if !completing_word && words[1].to_lowercase() == "priority" => {
            all_static(BUG_PRIORITY_VALUES, CompletionType::BugPriorityValue)
        }
        // bugs priority <#> <partial> - complete priority value
        4 if completing_word && words[1].to_lowercase() == "priority" => {
            filter_static(BUG_PRIORITY_VALUES, &partial, CompletionType::BugPriorityValue)
        }
        _ => CompletionResult::empty(),
    }
}

/// Context-aware completion for press command
/// At a transport stop: press button
/// Inside a transport: press <number> or press <stop_name>
pub(super) fn complete_press(words: &[&str], completing_word: bool) -> CompletionResult {
    let partial = get_partial(words, completing_word);

    match words.len() {
        // press - show "button" (stop names would need runtime data)
        1 if !completing_word => all_static(PRESS_TARGETS, CompletionType::PressTarget),
        // press <partial> - complete "button"
        2 if completing_word => filter_static(PRESS_TARGETS, &partial, CompletionType::PressTarget),
        _ => CompletionResult::empty(),
    }
}

/// Context-aware completion for pedit command
pub(super) fn complete_pedit(
    words: &[&str],
    completing_word: bool,
    property_template_vnums: &[String],
) -> CompletionResult {
    let partial = get_partial(words, completing_word);

    match words.len() {
        // pedit - show all template vnums
        1 if !completing_word => all_dynamic(property_template_vnums, CompletionType::PropertyTemplateVnum),
        // pedit <partial_vnum> - complete vnum
        2 if completing_word => filter_dynamic(property_template_vnums, &partial, CompletionType::PropertyTemplateVnum),
        // pedit <vnum> - show all subcommands
        2 if !completing_word => all_static(PEDIT_SUBCOMMANDS, CompletionType::PeditSubcommand),
        // pedit <vnum> <partial_subcmd> - complete subcommand
        3 if completing_word => filter_static(PEDIT_SUBCOMMANDS, &partial, CompletionType::PeditSubcommand),
        _ => CompletionResult::empty(),
    }
}

/// Context-aware completion for property command
pub(super) fn complete_property(words: &[&str], completing_word: bool, online_players: &[String]) -> CompletionResult {
    let partial = get_partial(words, completing_word);

    match words.len() {
        // property - show subcommands
        1 if !completing_word => all_static(PROPERTY_SUBCOMMANDS, CompletionType::PropertySubcommand),
        // property <partial_subcmd> - complete subcommand
        2 if completing_word => filter_static(PROPERTY_SUBCOMMANDS, &partial, CompletionType::PropertySubcommand),
        // property access - show access levels
        2 if !completing_word && words[1].to_lowercase() == "access" => {
            all_static(PROPERTY_ACCESS_LEVELS, CompletionType::PropertyAccessLevel)
        }
        // property access <partial_level> - complete access level
        3 if completing_word && words[1].to_lowercase() == "access" => {
            filter_static(PROPERTY_ACCESS_LEVELS, &partial, CompletionType::PropertyAccessLevel)
        }
        // property trust/untrust - show online players
        2 if !completing_word && (words[1].to_lowercase() == "trust" || words[1].to_lowercase() == "untrust") => {
            all_dynamic(online_players, CompletionType::PlayerName)
        }
        // property trust/untrust <partial_name> - complete player name
        3 if completing_word && (words[1].to_lowercase() == "trust" || words[1].to_lowercase() == "untrust") => {
            filter_dynamic(online_players, &partial, CompletionType::PlayerName)
        }
        _ => CompletionResult::empty(),
    }
}

/// Context-aware completion for mail command
pub(super) fn complete_mail(words: &[&str], completing_word: bool, online_players: &[String]) -> CompletionResult {
    let partial = get_partial(words, completing_word);

    match words.len() {
        // mail - show subcommands
        1 if !completing_word => all_static(MAIL_SUBCOMMANDS, CompletionType::MailSubcommand),
        // mail <partial_subcmd> - complete subcommand
        2 if completing_word => filter_static(MAIL_SUBCOMMANDS, &partial, CompletionType::MailSubcommand),
        // mail send/compose/reply - show online players (for recipient)
        2 if !completing_word
            && (words[1].to_lowercase() == "send"
                || words[1].to_lowercase() == "compose"
                || words[1].to_lowercase() == "reply") =>
        {
            all_dynamic(online_players, CompletionType::PlayerName)
        }
        // mail send/compose <partial_name> - complete player name
        3 if completing_word && (words[1].to_lowercase() == "send" || words[1].to_lowercase() == "compose") => {
            filter_dynamic(online_players, &partial, CompletionType::PlayerName)
        }
        _ => CompletionResult::empty(),
    }
}

/// Context-aware completion for bank command
pub(super) fn complete_bank(words: &[&str], completing_word: bool) -> CompletionResult {
    let partial = get_partial(words, completing_word);

    match words.len() {
        // bank - show subcommands
        1 if !completing_word => all_static(BANK_SUBCOMMANDS, CompletionType::BankSubcommand),
        // bank <partial_subcmd> - complete subcommand
        2 if completing_word => filter_static(BANK_SUBCOMMANDS, &partial, CompletionType::BankSubcommand),
        _ => CompletionResult::empty(),
    }
}

/// Context-aware completion for escrow command
pub(super) fn complete_escrow(words: &[&str], completing_word: bool) -> CompletionResult {
    let partial = get_partial(words, completing_word);

    match words.len() {
        // escrow - show subcommands
        1 if !completing_word => all_static(ESCROW_SUBCOMMANDS, CompletionType::EscrowSubcommand),
        // escrow <partial_subcmd> - complete subcommand
        2 if completing_word => filter_static(ESCROW_SUBCOMMANDS, &partial, CompletionType::EscrowSubcommand),
        _ => CompletionResult::empty(),
    }
}
