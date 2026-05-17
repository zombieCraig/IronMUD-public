use std::collections::HashSet;

use crate::import::{
    IrMob,
    IrZone, MappingOptions, PlannedMobile, Severity, Warning, WarningKind,
};
use crate::types::MobileFlags;

use super::FlagAction;

pub(super) fn map_mob(
    zone: &IrZone,
    area_prefix: &str,
    mob: &IrMob,
    opts: &MappingOptions,
    seen_extra_attrs: &mut HashSet<String>,
) -> (PlannedMobile, Vec<Warning>) {
    let _ = zone;
    let mut warnings = Vec::new();
    let mut flags = MobileFlags::default();
    let mut active_buffs: Vec<crate::types::ActiveBuff> = Vec::new();

    // MOB_* bits
    let (known_mob, unknown_mob) = crate::import::engines::circle::flags::decode_mob_flags(mob.mob_flag_bits);
    for flag in known_mob {
        match opts.circle.mob_flag_actions.get(flag) {
            Some(FlagAction::SetFlag { ironmud_flag }) => {
                if !apply_named_mob_flag(&mut flags, ironmud_flag) {
                    warnings.push(Warning::new(
                        WarningKind::UnsupportedFlag,
                        Severity::Warn,
                        mob.source.clone(),
                        format!("mapping points MOB_{flag} → {ironmud_flag}, but no such IronMUD MobileFlag"),
                    ));
                }
            }
            Some(FlagAction::SetCombatZone { .. }) => {
                warnings.push(Warning::new(
                    WarningKind::UnsupportedFlag,
                    Severity::Warn,
                    mob.source.clone(),
                    format!("mapping uses set_combat_zone for MOB_{flag} — that action only applies to rooms"),
                ));
            }
            Some(FlagAction::Warn { message }) => {
                warnings.push(Warning::new(
                    WarningKind::UnsupportedFlag,
                    Severity::Warn,
                    mob.source.clone(),
                    format!("MOB_{flag}: {message}"),
                ));
            }
            Some(FlagAction::Drop { .. }) => {}
            Some(FlagAction::SetStat { .. })
            | Some(FlagAction::SetArmorClass { .. })
            | Some(FlagAction::SetHitBonus)
            | Some(FlagAction::SetDamageBonus)
            | Some(FlagAction::SetMaxHpBonus)
            | Some(FlagAction::SetMaxManaBonus)
            | Some(FlagAction::AddItemAffect { .. })
            | Some(FlagAction::AddItemAffectMulti { .. }) => {
                warnings.push(Warning::new(
                    WarningKind::UnsupportedFlag,
                    Severity::Warn,
                    mob.source.clone(),
                    format!("mapping uses an item-only action for MOB_{flag}; ignored on mobs"),
                ));
            }
            Some(FlagAction::AddBuff {
                effect_type,
                magnitude,
                remaining_secs,
                source,
            }) => match crate::types::EffectType::from_str(effect_type) {
                Some(et) => {
                    active_buffs.push(crate::types::ActiveBuff {
                        effect_type: et,
                        magnitude: *magnitude,
                        remaining_secs: *remaining_secs,
                        source: source.clone(),
                        damage_type: None,
                        vs_effect: None,
                    });
                }
                None => warnings.push(Warning::new(
                    WarningKind::UnsupportedFlag,
                    Severity::Warn,
                    mob.source.clone(),
                    format!("MOB_{flag}: unknown effect_type `{effect_type}` for add_buff action"),
                )),
            },
            None => warnings.push(Warning::new(
                WarningKind::UnknownFlag,
                Severity::Warn,
                mob.source.clone(),
                format!("no mapping for MOB_{flag}"),
            )),
        }
    }
    for u in unknown_mob {
        warnings.push(Warning::new(
            WarningKind::UnknownFlag,
            Severity::Warn,
            mob.source.clone(),
            format!("unrecognised mob flag bit {u} (likely a patched flag)"),
        ));
    }

    // AFF_* bits — most have no IronMUD equivalent. Anything not listed in
    // the mapping JSON gets a default "permanent affect not modeled" warn,
    // generated here so the JSON stays compact.
    let (known_aff, unknown_aff) = crate::import::engines::circle::flags::decode_aff_flags(mob.aff_flag_bits);
    for flag in known_aff {
        match opts.circle.aff_flag_actions.get(flag) {
            Some(FlagAction::SetFlag { ironmud_flag }) => {
                if !apply_named_mob_flag(&mut flags, ironmud_flag) {
                    warnings.push(Warning::new(
                        WarningKind::UnsupportedFlag,
                        Severity::Warn,
                        mob.source.clone(),
                        format!("mapping points AFF_{flag} → {ironmud_flag}, but no such IronMUD MobileFlag"),
                    ));
                }
            }
            Some(FlagAction::SetCombatZone { .. }) => {
                warnings.push(Warning::new(
                    WarningKind::UnsupportedFlag,
                    Severity::Warn,
                    mob.source.clone(),
                    format!("mapping uses set_combat_zone for AFF_{flag} — that action only applies to rooms"),
                ));
            }
            Some(FlagAction::Warn { message }) => {
                warnings.push(Warning::new(
                    WarningKind::UnsupportedFlag,
                    Severity::Warn,
                    mob.source.clone(),
                    format!("AFF_{flag}: {message}"),
                ));
            }
            Some(FlagAction::Drop { .. }) => {}
            Some(FlagAction::SetStat { .. })
            | Some(FlagAction::SetArmorClass { .. })
            | Some(FlagAction::SetHitBonus)
            | Some(FlagAction::SetDamageBonus)
            | Some(FlagAction::SetMaxHpBonus)
            | Some(FlagAction::SetMaxManaBonus)
            | Some(FlagAction::AddItemAffect { .. })
            | Some(FlagAction::AddItemAffectMulti { .. }) => {
                warnings.push(Warning::new(
                    WarningKind::UnsupportedFlag,
                    Severity::Warn,
                    mob.source.clone(),
                    format!("mapping uses an item-only action for AFF_{flag}; ignored on mobs"),
                ));
            }
            Some(FlagAction::AddBuff {
                effect_type,
                magnitude,
                remaining_secs,
                source,
            }) => match crate::types::EffectType::from_str(effect_type) {
                Some(et) => {
                    active_buffs.push(crate::types::ActiveBuff {
                        effect_type: et,
                        magnitude: *magnitude,
                        remaining_secs: *remaining_secs,
                        source: source.clone(),
                        damage_type: None,
                        vs_effect: None,
                    });
                }
                None => warnings.push(Warning::new(
                    WarningKind::UnsupportedFlag,
                    Severity::Warn,
                    mob.source.clone(),
                    format!("AFF_{flag}: unknown effect_type `{effect_type}` for add_buff action"),
                )),
            },
            None => warnings.push(Warning::new(
                WarningKind::UnsupportedFlag,
                Severity::Warn,
                mob.source.clone(),
                format!("permanent AFF_{flag} not modeled at prototype level"),
            )),
        }
    }
    for u in unknown_aff {
        warnings.push(Warning::new(
            WarningKind::UnknownFlag,
            Severity::Warn,
            mob.source.clone(),
            format!("unrecognised affected-by flag bit {u} (likely a patched flag)"),
        ));
    }

    // Numeric stats with no IronMUD equivalent. Most are silently dropped;
    // a few warn so builders know to revisit balance.
    if mob.alignment != 0 {
        warnings.push(Warning::new(
            WarningKind::Info,
            Severity::Info,
            mob.source.clone(),
            format!(
                "alignment {} dropped (IronMUD has no alignment system)",
                mob.alignment
            ),
        ));
    }
    // CircleMUD SEX field: 1 → male, 2 → female, 0/other → unset.
    // Stamps a default Characteristics on the prototype with the gender
    // field set; DG `%self.heshe%`/`.sex%` reads it via resolved_gender().
    let characteristics_gender = match mob.sex {
        1 => Some("male".to_string()),
        2 => Some("female".to_string()),
        _ => None,
    };

    // E-block named attrs: warn once per distinct attribute name across the
    // whole import. `BareHandAttack` shows up on dozens of stock mobs and
    // would otherwise dominate the report.
    for (name, value) in &mob.extra_attrs {
        if seen_extra_attrs.insert(name.clone()) {
            warnings.push(Warning::new(
                WarningKind::UnsupportedFlag,
                Severity::Warn,
                mob.source.clone(),
                format!("E-block attr {name:?} (e.g. {value:?}) not imported"),
            ));
        }
    }

    let max_hp = dice_max(&mob.hp_dice).unwrap_or(0).max(1);
    let level = mob.level.max(0);
    let gold = mob.gold.max(0);

    let vnum = format!("{}_{}", area_prefix, mob.vnum);

    // CircleMUD default_position: 0/4/5/6→Sleeping, 7→Sitting, 8/9→Standing.
    // Unknown values fall through to Standing. (0 = dead, 1-3 = combat-only
    // states that don't apply at spawn; 4 = stunned ≈ unconscious, mapped to
    // Sleeping; 5 = sleeping; 6 = resting → Sitting in IronMUD's 3-state
    // model; 7 = sitting; 8 = fighting; 9 = standing.)
    let position = match mob.default_position {
        0 | 4 | 5 => Some(crate::types::MobilePosition::Sleeping),
        6 | 7 => Some(crate::types::MobilePosition::Sitting),
        8 | 9 => Some(crate::types::MobilePosition::Standing),
        _ => None,
    };

    (
        PlannedMobile {
            area_prefix: area_prefix.to_string(),
            source_vnum: mob.vnum,
            vnum,
            name: mob.short_descr.clone(),
            short_desc: mob.long_descr.clone(),
            long_desc: mob.description.clone(),
            keywords: mob.keywords.clone(),
            level,
            max_hp,
            damage_dice: mob.damage_dice.clone(),
            armor_class: mob.ac,
            gold,
            flags,
            world_max_count: None,
            active_buffs,
            position,
            characteristics_gender,
            source: mob.source.clone(),
        },
        warnings,
    )
}

/// Set a `RoomFlags` bool by snake_case name. Returns false if the name
/// isn't a known flag — surfaces typos in the mapping JSON.
/// Set a `MobileFlags` bool by snake_case name. Returns false if the name
/// isn't a known flag — surfaces typos in the mapping JSON.
pub(super) fn apply_named_mob_flag(flags: &mut MobileFlags, name: &str) -> bool {
    match name {
        "aggressive" => flags.aggressive = true,
        "sentinel" => flags.sentinel = true,
        "scavenger" => flags.scavenger = true,
        "shopkeeper" => flags.shopkeeper = true,
        "no_attack" => flags.no_attack = true,
        "healer" => flags.healer = true,
        "leasing_agent" => flags.leasing_agent = true,
        "cowardly" => flags.cowardly = true,
        "can_open_doors" => flags.can_open_doors = true,
        "guard" => flags.guard = true,
        "helper" => flags.helper = true,
        "thief" => flags.thief = true,
        "cant_swim" => flags.cant_swim = true,
        "poisonous" => flags.poisonous = true,
        "fiery" => flags.fiery = true,
        "chilling" => flags.chilling = true,
        "corrosive" => flags.corrosive = true,
        "shocking" => flags.shocking = true,
        "stay_zone" => flags.stay_zone = true,
        "aware" => flags.aware = true,
        "memory" => flags.memory = true,
        "no_sleep" => flags.no_sleep = true,
        "no_blind" => flags.no_blind = true,
        "no_bash" => flags.no_bash = true,
        "no_summon" => flags.no_summon = true,
        "no_charm" => flags.no_charm = true,
        _ => return false,
    }
    true
}

/// Compute the maximum value of a dice expression like `5d10+550` or
/// `2d6+3`. Returns `None` if the input doesn't parse.
pub(super) fn dice_max(expr: &str) -> Option<i32> {
    let s = expr.trim();
    let (dice_part, bonus): (&str, i32) = match s.find(['+', '-']) {
        Some(i) => {
            let (left, right) = s.split_at(i);
            let bonus: i32 = right.parse().ok()?;
            (left, bonus)
        }
        None => (s, 0),
    };
    let (n, sides) = dice_part.split_once('d')?;
    let n: i32 = n.parse().ok()?;
    let sides: i32 = sides.parse().ok()?;
    Some(n * sides + bonus)
}

/// Slug a zone name into an IronMUD area prefix (alphanumeric + underscore,
/// lowercase). Falls back to `zone_<vnum>` for empty / collision cases.
pub(super) fn unique_prefix(name: &str, vnum: i32, taken: &[String]) -> String {
    let base = slug(name);
    let base = if base.is_empty() { format!("zone_{vnum}") } else { base };
    if !taken.iter().any(|t| t == &base) {
        return base;
    }
    let with_vnum = format!("{base}_{vnum}");
    if !taken.iter().any(|t| t == &with_vnum) {
        return with_vnum;
    }
    // Should be vanishingly rare; final fallback walks an integer suffix.
    let mut i = 2;
    loop {
        let candidate = format!("{with_vnum}_{i}");
        if !taken.iter().any(|t| t == &candidate) {
            return candidate;
        }
        i += 1;
    }
}

pub(super) fn slug(s: &str) -> String {
    let mut out = String::new();
    let mut last_was_underscore = false;
    for ch in s.chars() {
        let c = ch.to_ascii_lowercase();
        if c.is_ascii_alphanumeric() {
            out.push(c);
            last_was_underscore = false;
        } else if !last_was_underscore && !out.is_empty() {
            out.push('_');
            last_was_underscore = true;
        }
    }
    while out.ends_with('_') {
        out.pop();
    }
    out
}
