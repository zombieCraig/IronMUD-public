# IronMUD Combat System Design Document

## Overview

A comprehensive combat system for IronMUD featuring round-based combat, body part wound tracking, critical hits, and death/respawn mechanics.

---

## Core Design Decisions

| Aspect | Decision |
|--------|----------|
| Combat Flow | Round-based (5 second rounds) |
| Body Parts | 11 parts: head, neck, torso, arms (L/R), legs (L/R), hands (L/R), feet (L/R) |
| Wound Levels | None → Minor → Moderate → Severe → Critical → Disabled |
| Wound Penalties | Progressive functional penalties based on severity |
| Defense | Passive only (armor + DEX-based dodge) |
| Hit Chance | `50 + (skill * 5) + attacker_DEX_mod - target_DEX_mod - target_AC` (capped 5-95%) |
| Critical Chance | `5% + (skill_level * 1%)` |
| Critical Effects | Bleeding, Stun (1-2 rounds), Limb disable, Bonus damage (1.5-2x) |
| Targeting | Random weighted (called shots stubbed for future) |
| Multi-Combat | Multiple enemies attack each round; player can switch targets |
| Engagement | Locked in combat; must flee to leave |
| Flee | Chance-based (DEX vs opponent), requires stamina, failure wastes turn |
| Stamina | Per-action cost for attacks |
| Death | 0 HP total OR vital critical → unconscious → 5 round bleedout timer |
| Aggro | Aggressive mobs attack on sight and continue attacking unconscious targets |
| Corpses | Container with inventory/equipment/gold; decay 10min (mob) / 1hr (player) |
| Respawn | Return to spawn point (default: starting room) |
| XP | Per hit dealt, awarded to weapon skill used |
| Combat Zones | CombatZoneType enum: PvE (default), Safe (no combat), PvP (players can attack each other) |

---

## Combat Zone Types

Combat zones are controlled at the **area level** with optional **room-level overrides**.

| Zone Type | Attack Mobiles | Attack Players | Use Case |
|-----------|----------------|----------------|----------|
| **PvE** (default) | Yes | No | Normal gameplay |
| **Safe** | No | No | Towns, safe havens |
| **PvP** | Yes | Yes | Arenas, PvP zones |

**Priority**: Room zone overrides area zone if set.

**Implementation**:
- `CombatZoneType` enum with variants: `Pve`, `Safe`, `Pvp`
- Add `combat_zone: CombatZoneType` to AreaData (default: Pve)
- Add `combat_zone: Option<CombatZoneType>` to RoomFlags (None = inherit from area)
- Check area first, then room override

**Editor Commands**:
- `aedit zone [pve|safe|pvp]` - Set area combat zone
- `redit zone [pve|safe|pvp|inherit]` - Set room zone or inherit from area
- `redit zone` (no args) - Cycle through zone types

---

## Armor System

Armor provides protection to specific body parts and can be damaged in combat.

### Armor Protection
- Each armor piece protects specific wear locations (e.g., helmet → head, breastplate → torso)
- When a body part is hit, check if armor covers that location
- If armored: roll armor save (based on effective AC after holes)
- **Successful save**: All damage blocked, but armor gains a hole
- **Failed save**: Full damage goes through, no hole added

### Armor Degradation (Holes)
- Each hit that armor absorbs creates a "hole"
- Each hole reduces that armor's effectiveness by **33%**
- After 3 holes: armor is destroyed/unusable
- Holes tracked per item: `holes: i32` (0-3)
- Effective AC = `base_AC * (1.0 - (holes * 0.33))`

### Armor Repair (Out of Scope)
- Repairing armor will be a future crafting/smith feature

---

## Weapon Skills

| Skill | Weapon Types |
|-------|--------------|
| `short_blades` | Daggers, knives, shortswords |
| `long_blades` | Swords, longswords, greatswords |
| `short_blunt` | Clubs, maces, hammers |
| `long_blunt` | Warhammers, staves, mauls |
| `polearms` | Spears, halberds, pikes |
| `unarmed` | Fists, natural weapons |
| `ranged` | Bows, crossbows (future) |

---

## Damage Type → Wound Type Mapping

| Damage Type | Wound Type |
|-------------|------------|
| Slashing | Cut, Laceration |
| Piercing | Puncture |
| Bludgeoning (minor/moderate) | Bruise |
| Bludgeoning (severe+) | Fracture |

---

## Body Part Targeting Weights

| Body Part | Weight | Notes |
|-----------|--------|-------|
| Torso | 35% | Largest target |
| Left/Right Arm | 12% each | Common |
| Left/Right Leg | 12% each | Common |
| Head | 8% | Vital, harder to hit |
| Left/Right Hand | 4% each | Small targets |
| Left/Right Foot | 4% each | Small targets |
| Neck | 3% | Vital, very hard to hit |

---

## Wound Severity Penalties

| Wound Level | Penalty | Effect |
|-------------|---------|--------|
| None | 0% | No effect |
| Minor | 10% | Slight impairment |
| Moderate | 25% | Noticeable impairment |
| Severe | 50% | Major impairment |
| Critical | 75% | Near-useless |
| Disabled | 100% | Completely unusable |

**Arm wounds**: Affect attack accuracy
**Leg wounds**: Affect movement/flee chance
**Hand wounds**: Affect weapon handling
**Head wounds**: Affect all actions
**Vital criticals** (head/neck/torso): Instant drop to 0 HP

---

## Bleeding System

- Bleeding severity: 1-5 per wound based on wound severity
- Stacks across different body parts
- Damage per round = sum of all bleeding severities
- Continues until treated (first aid - out of scope)

---

## Critical Hit Effects

| Effect | Description |
|--------|-------------|
| Bonus Damage | 1.5x (low skill) or 2x (high skill) damage |
| Bleeding | Adds severity-based bleed to wound |
| Stun | Target loses 1-2 rounds based on hit severity |
| Limb Disable | Instant Severe wound to hit location |

---

## Death & Respawn Flow

1. HP reaches 0 OR vital part gets critical wound
2. Character falls unconscious
3. Bleedout timer starts (5 rounds)
4. Aggressive mobs continue attacking → instant death
5. If timer expires without healing → death
6. If additional damage taken while unconscious → death
7. On death:
   - Create corpse container in room
   - Transfer all inventory to corpse
   - Transfer all equipped items to corpse
   - Transfer gold to corpse
   - Move character to spawn room
   - Restore HP to 25% max
   - Clear all wounds and combat state

---

## Implementation Phases

### Phase 1: Combat State Infrastructure ✓
**Status**: Complete (commit 573fb40)
**Goal**: Data structures for tracking combat state and zone flags

**Rust Changes (src/lib.rs)**:
- Add `CombatZoneType` enum (Pve, Safe, Pvp) with Default trait
- Add `CombatState` struct (in_combat, targets, stun state)
- Add `CombatTarget` struct and `CombatTargetType` enum
- Add `combat: CombatState` to CharacterData
- Add `combat: CombatState` to MobileData
- Add `spawn_room_id: Option<Uuid>` to CharacterData
- Add `combat_zone: CombatZoneType` to AreaData (default: Pve)
- Add `combat_zone: Option<CombatZoneType>` to RoomFlags (None = inherit from area)
- Add `holes: i32` to ItemData (armor degradation, default 0)

**New File (src/script/combat.rs)**:
- Register combat state Rhai functions
- `enter_combat()`, `exit_combat()`, `is_in_combat()`, `get_combat_targets()`
- `get_combat_zone(room_id)` - returns effective zone type ("pve", "safe", "pvp")
- `can_attack_mobiles(room_id)` - true for PvE and PvP zones
- `can_attack_players(room_id)` - true only for PvP zones

**Rhai Changes**:
- `scripts/commands/aedit.rhai` - Add `zone` subcommand for combat zone
- `scripts/commands/redit.rhai` - Add `zone` subcommand with cycling/explicit setting

**Testing**: Unit tests for serialization, combat state transitions, zone flag inheritance

---

### Phase 2: Wound System ✓
**Status**: Complete (commit 573fb40)
**Goal**: Body part and wound tracking

**Rust Changes (src/lib.rs)**:
- Add `BodyPart` enum (11 variants)
- Add `WoundLevel` enum (None through Disabled)
- Add `WoundType` enum (Cut, Puncture, Bruise, Fracture)
- Add `Wound` struct (body_part, level, wound_type, bleeding_severity)
- Add `wounds: Vec<Wound>` to CharacterData and MobileData

**Rust Changes (src/script/combat.rs)**:
- `get_wound_level()`, `inflict_wound()`, `get_total_bleeding()`
- `get_body_part_penalty()`, `is_vital_part()`
- `roll_random_body_part()` (weighted)

**Testing**: Wound infliction, escalation, penalty calculations

---

### Phase 3: Weapon Skills ✓
**Status**: Complete (commit 59151a3)
**Goal**: Weapon skill types and item mapping

**Rust Changes (src/lib.rs)**:
- Add `WeaponSkill` enum (7 variants)
- Add `weapon_skill: Option<WeaponSkill>` to ItemData

**Rust Changes (src/script/combat.rs + items.rs)**:
- `get_weapon_skill_type()`, `get_equipped_weapon_skill()`
- `set_item_weapon_skill()`

**Rhai Changes (scripts/commands/oedit.rhai)**:
- Add `wskill` subcommand for weapon skill assignment

**Testing**: Skill assignment, retrieval from equipped weapons

---

### Phase 4: Core Combat Mechanics ✓
**Status**: Complete (commit 97cb266)
**Goal**: Hit/damage calculations, armor system, attack command

**Rust Changes (src/script/combat.rs)**:
- `calculate_hit_chance()` - formula implementation
- `roll_attack()`, `calculate_damage()`, `roll_dice()`
- `get_stat_modifier()` - (stat - 10) / 2
- `check_critical_hit()` - skill-based crit chance
- `consume_stamina_for_attack()`
- `get_armor_for_body_part(char_name, body_part)` - find equipped armor covering location
- `calculate_effective_ac(item_id)` - base_AC * (1.0 - holes * 0.33)
- `roll_armor_save(armor_id, damage)` - returns (damage_taken, armor_absorbed)
- `add_armor_hole(item_id)` - increment holes, destroy at 3
- `get_armor_condition(item_id)` - "pristine" / "damaged" / "battered" / "destroyed"

**New Rhai Scripts**:
- `scripts/commands/attack.rhai` (alias: kill.rhai)
  - Check `is_location_safe()` → block all combat
  - Check `is_location_pvp()` → allow/deny player targets
  - Check mobile `no_attack` flag
  - On hit: check armor → roll save → apply holes if absorbed
- `scripts/lib/combat.rhai` - message formatting helpers

**Modified Rhai Scripts**:
- `scripts/commands/examine.rhai` - Show armor condition (holes)
- `scripts/commands/equipment.rhai` - Show armor condition

**Testing**: Hit chance, damage calculations, armor saves, hole accumulation, zone checks

---

### Phase 5: Combat Round System ✓
**Status**: Complete
**Goal**: 5-second automatic combat rounds

**Rust Changes (src/main.rs)**:
- Add combat tick task (5 second interval)
- Pattern: iterate combatants, process attacks, apply bleeding
- `process_character_combat_round()`, `process_mobile_combat_round()`
- Combat round processing with stun handling, bleeding, attacks

**Rust Changes (src/script/combat.rs)**:
- Combat state functions: `enter_combat()`, `exit_combat()`, `is_in_combat()`
- Target management: `get_combat_targets()`, `get_primary_target()`, `remove_combat_target()`
- Stun handling: `apply_stun()`, `get_stun_rounds()`, `reduce_stun()`
- Mobile combat equivalents for all above functions
- `apply_bleeding_damage()`, `apply_mobile_bleeding_damage()`

**Rust Changes (src/db.rs)**:
- `get_all_characters_in_combat()`, `get_all_mobiles_in_combat()`

**Modified Rhai Script**:
- `scripts/commands/attack.rhai` - enters combat state when attacking

**Testing**: Automatic combat, multi-target, bleeding ticks

---

### Phase 6: Flee & Combat Lock ✓
**Status**: Complete
**Goal**: Escape mechanics, movement restrictions

**Rust Changes (src/script/combat.rs)**:
- `calculate_flee_chance()` - DEX comparison with leg wound penalties (10-90% range)
- `get_leg_wound_penalty()` - max of left/right leg wound penalties
- `get_valid_flee_directions()` - returns valid exits (not locked)
- `roll_d100()` - random d100 roll
- `remove_mobile_combat_target()` - removes player from mobile's combat list

**New Rhai Script**:
- `scripts/commands/flee.rhai` - flee command with stamina cost (10), random direction escape

**Modified Rhai Script**:
- `scripts/commands/go.rhai` - combat lock check blocks normal movement in combat

**Testing**: Flee success/failure, movement blocking, leg wound penalties

---

### Phase 7: Death, Corpses, Respawn ✓
**Status**: Complete
**Goal**: Complete death cycle

**Rust Changes (src/lib.rs)**:
- Add `is_unconscious: bool` and `bleedout_rounds_remaining: i32` to CharacterData (serde skip)
- Add same fields to MobileData
- Add corpse flags to ItemFlags: `is_corpse`, `corpse_owner`, `corpse_created_at`, `corpse_is_player`, `corpse_gold`

**Rust Changes (src/script/combat.rs)**:
- Unconscious state: `set_unconscious()`, `is_character_unconscious()`, `get_bleedout_rounds()`, `set_bleedout_rounds()`
- Mobile equivalents for unconscious state
- Corpse creation: `create_corpse(name, room_id, is_player)` - returns corpse_id
- Item transfer: `transfer_inventory_to_corpse()`, `transfer_equipment_to_corpse()`, `transfer_gold_to_corpse()`
- Corpse gold: `get_corpse_gold()`, `take_corpse_gold()`
- Respawn: `respawn_character()` - moves to spawn room, 25% HP, clears wounds/combat
- Corpse decay: `get_all_corpses()`, `should_corpse_decay()`, `remove_corpse()`, `get_corpse_room()`

**Rust Changes (src/main.rs)**:
- Add corpse decay tick (60 second interval)
- Modify combat tick to handle unconscious state and bleedout timer
- Add `process_player_death()` - creates corpse, transfers items, respawns character
- Add `process_mobile_death()` - creates corpse for mobile

**Modified Rhai Scripts**:
- `scripts/commands/look.rhai` - `look_at_corpse()` displays corpse gold and contents
- `scripts/commands/get.rhai` - special handling for corpse gold, skip closed check for corpses
- `scripts/commands/attack.rhai` - coup de grace on unconscious targets (instant kill)

**Testing**: Death conditions, corpse creation, decay (10min mob/1hr player), respawn, looting

---

### Phase 8: Criticals & XP ✓
**Status**: Complete
**Goal**: Critical effects, combat experience

**Rust Changes (src/script/combat.rs)**:
- `roll_critical_effect()` - Returns effect type: "bleeding", "stun", "disable", "clean"
- `calculate_crit_damage(base, skill)` - Returns scaled damage (1.5x or 2x)
- `get_crit_stun_rounds(skill)` - Returns 1 or 2 based on skill
- `get_crit_bleeding_severity(skill)` - Returns 2-5 based on skill
- `add_wound_bleeding()` / `add_mobile_wound_bleeding()` - Adds bleeding to wounds
- `escalate_wound_to_severe()` / `escalate_mobile_wound_to_severe()` - Limb disable

**Rust Changes (src/main.rs)**:
- `get_character_weapon_info()` - Get weapon skill and damage from equipped weapon
- `get_skill_level_for_character()` - Get skill level from character
- `add_skill_experience_to_character()` - Award XP, returns true on level-up
- `roll_random_body_part()` - Weighted body part selection
- `add_mobile_wound_bleeding()` / `add_character_wound_bleeding()` - Add bleeding
- `escalate_mobile_wound_to_severe()` / `escalate_character_wound_to_severe()` - Disable limb
- Updated `process_character_attacks_mobile()` with crits and XP
- Updated `process_mobile_attacks_player()` with crits

**Modified Rhai Script (scripts/commands/attack.rhai)**:
- `calculate_attack_damage()` - Returns crit_effect and crit_effect_value
- `get_crit_text()` - Shows effect type: "[CRITICAL - Stun!]", etc.
- `apply_crit_effects_to_mobile()` / `apply_crit_effects_to_player()` - Apply effects
- XP awards on successful hits (10 XP per hit to weapon skill)

**Testing**: Each crit type, bleeding stacking, XP progression

---

## Critical Files Reference

| File | Purpose |
|------|---------|
| `src/lib.rs` | Core structs (CombatState, BodyPart, Wound, etc.) |
| `src/script/combat.rs` | New combat Rhai functions module |
| `src/script/mod.rs` | Register combat module |
| `src/main.rs` | Combat tick and corpse decay tick |
| `scripts/commands/attack.rhai` | Attack command |
| `scripts/commands/flee.rhai` | Flee command |
| `scripts/commands/go.rhai` | Add combat lock |
| `scripts/combat/round.rhai` | Round processing |

---

## Verification Plan

After each phase:
1. Run `cargo test` to verify Rust changes compile and pass unit tests
2. Run `cargo build` to verify full compilation
3. Manual testing in-game:
   - Phase 1: Verify combat state persists across reconnect
   - Phase 2: Use debug commands to inflict wounds, verify penalties
   - Phase 3: Assign weapon skills via oedit, verify on wield
   - Phase 4: Attack passive mobs, verify hit/miss/damage
   - Phase 5: Engage combat, verify automatic rounds fire
   - Phase 6: Test flee command, verify can't walk during combat
   - Phase 7: Kill character, verify corpse and respawn
   - Phase 8: Watch for crits, verify XP awarded to skills

---

## Troubleshooting & Debug Logging

### Known Issues & Fixes

**Mobile Combat State Not Persisting (Fixed Jan 2026)**

**Symptom**: Mobiles would not attack back when players attacked them. The `enter_mobile_combat()` function would report successful saves, but mobiles never appeared in `get_all_mobiles_in_combat()`.

**Root Cause**: In `scripts/commands/attack.rhai`, the mobile was loaded before calling `enter_mobile_combat()`, then saved afterwards with HP changes. This overwrote the combat state that `enter_mobile_combat()` had just set.

**Fix**: Added a reload of the mobile after `enter_mobile_combat()` so that subsequent saves preserve the combat state:
```rhai
enter_mobile_combat(target_info.target_id, "player", attacker.name);

// IMPORTANT: Reload mobile after entering combat so we have the updated combat state
let mobile = get_mobile_data(target_info.target_id);
```

### Debug Logging (Currently Enabled)

Debug logging is enabled for combat troubleshooting. Run with `RUST_LOG=debug cargo run` to see:

| Location | What It Logs |
|----------|--------------|
| `enter_mobile_combat` (combat.rs) | Mobile ID, current combat state, target added, save result, verification re-read |
| `get_all_mobiles_in_combat` (db.rs) | Each non-prototype mobile's name, ID, and combat state during iteration |
| `process_mobile_combat_round` (main.rs) | Mobile name, combat state, room ID, stun/bleeding checks |
| `wander_tick` (main.rs) | Skipped mobiles in combat, movement decisions |

### Potential Future Issues

If combat state issues recur, check for:
1. **Stale data overwrites**: Any Rhai script that loads a mobile, calls a combat function, then saves the mobile without reloading
2. **Race conditions**: Tick systems (wander, combat, idle) loading mobiles at start of iteration vs mid-iteration database changes
3. **Mutex poisoning**: Panics in tick systems while holding locks can poison the connections mutex, causing all new connections to fail

---

## Out of Scope (Future Work)

- First aid / wound treatment system
- Called shots (targeting specific body parts)
- Custom spawn point saving
- Character leveling system
- Ranged combat details
- Combat groups/parties
- Armor repair (smithing/crafting)
