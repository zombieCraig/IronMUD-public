use super::CompletionData;
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

    // Most settings are on/off; `xpfeed` is three-way, so offering it a
    // toggle pair would complete to values it rejects.
    let values = |setting: &str| -> &'static [&'static str] {
        if setting.eq_ignore_ascii_case("xpfeed") {
            SET_XPFEED_VALUES
        } else {
            SET_TOGGLE_VALUES
        }
    };

    match words.len() {
        // set - show all available settings
        1 if !completing_word => all_static(&available, CompletionType::SetSubcommand),
        // set <partial_setting> - complete setting name
        2 if completing_word => filter_static(&available, &partial, CompletionType::SetSubcommand),
        // set <setting> - show that setting's values
        2 if !completing_word => all_static(values(words[1]), CompletionType::SetSubcommand),
        // set <setting> <partial_value> - complete the value
        3 if completing_word => filter_static(values(words[1]), &partial, CompletionType::SetSubcommand),
        _ => CompletionResult::empty(),
    }
}

/// Context-aware completion for the prompt command
pub(super) fn complete_prompt(words: &[&str], completing_word: bool) -> CompletionResult {
    let partial = get_partial(words, completing_word);

    match words.len() {
        1 if !completing_word => all_static(PROMPT_SUBCOMMANDS, CompletionType::PromptSubcommand),
        2 if completing_word => filter_static(PROMPT_SUBCOMMANDS, &partial, CompletionType::PromptSubcommand),
        _ => CompletionResult::empty(),
    }
}

/// Context-aware completion for the top command.
///
/// The board names come from `leaderboard::completion_hints`, which derives
/// them from the same constants the scan uses. Boards discovered from data
/// are not offered here — `top boards` is the surface for those.
pub(super) fn complete_top(words: &[&str], completing_word: bool) -> CompletionResult {
    let partial = get_partial(words, completing_word);
    let hints = crate::leaderboard::completion_hints();

    match words.len() {
        1 if !completing_word => all_static(&hints, CompletionType::TopBoard),
        2 if completing_word => filter_static(&hints, &partial, CompletionType::TopBoard),
        _ => CompletionResult::empty(),
    }
}

/// Context-aware completion for the build command.
///
/// `build audit <kind> <key>` completes the key too, from the same list the
/// command will resolve it against. For items and mobiles that is the room's
/// contents **first** — a builder auditing something almost always has it in
/// front of them, and `sword` is what they would type at any other command —
/// then the world's prototype vnums.
///
/// Room keywords go in ahead of vnums deliberately: with a partial that matches
/// both, the thing you can see wins the unique-completion shortcut.
pub(super) fn complete_build(words: &[&str], completing_word: bool, data: &CompletionData) -> CompletionResult {
    let partial = get_partial(words, completing_word);

    // `build audit <kind> <partial>` — which list the key comes from.
    let keys_for = |kind: &str| -> (&[String], CompletionType) {
        match kind.to_lowercase().as_str() {
            "room" => (data.room_vnums, CompletionType::RoomVnum),
            "item" | "object" | "obj" => (data.item_vnums, CompletionType::ItemVnum),
            "mob" | "mobile" | "npc" => (data.mobile_vnums, CompletionType::MobileVnum),
            "quest" => (data.quest_vnums, CompletionType::QuestVnum),
            "area" | "zone" => (data.area_prefixes, CompletionType::AreaPrefix),
            _ => (&[], CompletionType::None),
        }
    };

    // What is standing or lying in the room, for the kinds that can be there.
    let in_room = |kind: &str| -> &[String] {
        match kind.to_lowercase().as_str() {
            "item" | "object" | "obj" => data.items_in_room,
            "mob" | "mobile" | "npc" => data.mobs_in_room,
            _ => &[],
        }
    };

    let key_completions = |kind: &str, partial: &str| -> CompletionResult {
        let (vnums, comp_type) = keys_for(kind);
        let mut out: Vec<String> = Vec::new();
        for k in in_room(kind).iter().chain(vnums.iter()) {
            if k.to_lowercase().starts_with(partial) && !out.iter().any(|s| s.eq_ignore_ascii_case(k)) {
                out.push(k.clone());
            }
        }
        CompletionResult::new(out, partial, comp_type)
    };

    let is_audit = words.len() >= 2 && words[1].eq_ignore_ascii_case("audit");
    let is_waive = words.len() >= 2 && words[1].eq_ignore_ascii_case("waive");

    match words.len() {
        1 if !completing_word => all_static(BUILD_SUBCOMMANDS, CompletionType::BuildSubcommand),
        2 if completing_word => filter_static(BUILD_SUBCOMMANDS, &partial, CompletionType::BuildSubcommand),
        2 if !completing_word && is_waive => all_static(BUILD_WAIVE_SUBCOMMANDS, CompletionType::BuildSubcommand),
        3 if completing_word && is_waive => {
            filter_static(BUILD_WAIVE_SUBCOMMANDS, &partial, CompletionType::BuildSubcommand)
        }
        2 if !completing_word && is_audit => all_static(BUILD_AUDIT_TARGETS, CompletionType::BuildAuditTarget),
        3 if completing_word && is_audit => {
            filter_static(BUILD_AUDIT_TARGETS, &partial, CompletionType::BuildAuditTarget)
        }
        3 if !completing_word && is_audit => key_completions(words[2], ""),
        4 if completing_word && is_audit => key_completions(words[2], &partial),
        _ => CompletionResult::empty(),
    }
}

/// Context-aware completion for the bounty command.
///
/// Stops at the subcommand. Ticket numbers are not offered: the list is
/// unbounded and a builder reads them off `bounty` anyway.
pub(super) fn complete_bounty(words: &[&str], completing_word: bool) -> CompletionResult {
    let partial = get_partial(words, completing_word);
    match words.len() {
        1 if !completing_word => all_static(BOUNTY_SUBCOMMANDS, CompletionType::BountySubcommand),
        2 if completing_word => filter_static(BOUNTY_SUBCOMMANDS, &partial, CompletionType::BountySubcommand),
        _ => CompletionResult::empty(),
    }
}

/// Context-aware completion for the world command.
pub(super) fn complete_world(words: &[&str], completing_word: bool) -> CompletionResult {
    let partial = get_partial(words, completing_word);
    match words.len() {
        1 if !completing_word => all_static(WORLD_SUBCOMMANDS, CompletionType::WorldSubcommand),
        2 if completing_word => filter_static(WORLD_SUBCOMMANDS, &partial, CompletionType::WorldSubcommand),
        _ => CompletionResult::empty(),
    }
}

/// Context-aware completion for the standing command.
///
/// Unlike `top`, whose board names are mostly named in code, factions are pure
/// data — so this takes the declared keys from the world rather than deriving
/// them from a constant. A faction tag nobody declared still works at the
/// command; it just cannot be completed, which is the same trade `top` makes
/// for boards discovered from character data.
pub(super) fn complete_standing(words: &[&str], completing_word: bool, faction_keys: &[String]) -> CompletionResult {
    let partial = get_partial(words, completing_word);

    match words.len() {
        1 if !completing_word => all_dynamic(faction_keys, CompletionType::FactionKey),
        2 if completing_word => filter_dynamic(faction_keys, &partial, CompletionType::FactionKey),
        _ => CompletionResult::empty(),
    }
}

/// Context-aware completion for `locate`.
pub(super) fn complete_locate(words: &[&str], completing_word: bool) -> CompletionResult {
    let partial = get_partial(words, completing_word);

    match words.len() {
        1 if !completing_word => all_static(LOCATE_TARGETS, CompletionType::LocateTarget),
        2 if completing_word => filter_static(LOCATE_TARGETS, &partial, CompletionType::LocateTarget),
        _ => CompletionResult::empty(),
    }
}

/// Context-aware completion for `consignments`.
///
/// Only the subcommand completes. The listing number that follows is a position
/// in a list only the player can see, and offering "1 2 3" would be guessing at
/// how much they have out.
pub(super) fn complete_consignments(words: &[&str], completing_word: bool) -> CompletionResult {
    let partial = get_partial(words, completing_word);

    match words.len() {
        1 if !completing_word => all_static(CONSIGNMENTS_SUBCOMMANDS, CompletionType::ConsignmentsSubcommand),
        2 if completing_word => filter_static(
            CONSIGNMENTS_SUBCOMMANDS,
            &partial,
            CompletionType::ConsignmentsSubcommand,
        ),
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
