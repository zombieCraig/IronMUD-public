# Distance States and Ranged Combat System

This document describes the implementation of combat distance tracking and ranged weapons for IronMUD.

## Overview

The ranged combat system adds:
- **Distance states** within a room (ranged/pole/melee)
- **Ammunition tracking** for bows, crossbows, and firearms
- **Cross-room sniping** with pursuit mechanics
- **Weapon-specific behaviors** (reload, fire modes, noise levels)

## Implementation Stages

| Stage | Focus | Status |
|-------|-------|--------|
| 1 | Distance States Foundation | **Complete** |
| 2 | Ammunition System & Basic Ranged | **Complete** |
| 3 | Crossbows & Firearms | **Complete** |
| 4 | Cross-Room Sniping & Pursuit | **Complete** |
| 5 | Advanced Features & Polish | **Complete** |

## Changelog

### 2026-01-30 - Stage 5 Complete

**Features Implemented:**
- **Distance Prompt**: Combat prompt shows `[Ranged: target]` (yellow) or `[Pole: target]` (cyan) during combat
- **Special Ammunition**: Fire/cold/poison/acid effects on ammunition, applied on ranged hit via ongoing effects system
- **Weapon Attachments**: Scope (+accuracy), suppressor (-noise), extended magazine (+capacity), laser sight (+accuracy); attach/detach commands
- **Arrow Recovery**: Arrows/bolts embed in mobs on hit; 50% recoverable on death, 25% of recovered spawn broken; bullets and special ammo excluded
- **Body-Part Contextual Messages**: Ranged hit messages vary by weapon type and damage severity (grazes/lodges in/punches through/tears through/etc.)

**Implementation Notes:**
- Ammo effects captured onto weapon at reload time for magazine weapons; bows read directly from ammo item
- Attachments stored as items in weapon container; bonuses calculated at use time by scanning contents
- Arrow recovery runs in both Rhai scripts (shoot/snipe death handling) and Rust combat tick (process_mobile_death)
- Body-part messages applied to shoot.rhai, snipe.rhai, and combat tick ranged hit path

### 2026-01-29 - Stage 4 Complete

**Commits:**
- `5adc0f1` - Add noise_level, cowardly flag, and pursuit fields for cross-room sniping
- `c460096` - Register Rhai functions for noise level, cowardly flag, and pursuit system
- `283d456` - Add pursuit tick for mob response to cross-room sniping
- `7d73690` - Add snipe command for cross-room ranged combat
- `276c559` - Add cowardly mob flee behavior in combat at HP <= 25%
- `9a28dc7` - Add OLC editor support and tab completion for noise level and cowardly flag

**Implementation Notes:**
- `noise_level` stored as String on ItemData; defaults by ranged_type (bow=silent, crossbow=quiet, firearm=loud)
- Pursuit is single-hop only: mob moves one room toward sniper then stops
- Pursuit tick runs every 10s, only processes mobs with active pursuit state
- Cowardly mobs flee on snipe hit and also flee during regular combat when HP <= 25%
- Loud weapons broadcast gunshot message to rooms adjacent to the target room
- Miss halves pursuit chances (silent=25%, quiet=37%, normal=50%, loud=100%)
- Rhai max expression depth increased to 128 for large command scripts
- Standard exits only (n/s/e/w/u/d); closed doors block both sniping and pursuit

### 2026-01-29 - Stage 2 Complete

**Implementation Notes:**
- Stage 2 was implemented alongside Stage 3; all ammunition system features (caliber, ammo_count, ready slot, shoot command) landed in the same commits as crossbow/firearm support
- Commands implemented: `shoot`, `ready`, `reload`, `unload`, `firemode`
- `fill` command deferred (conflicts with liquid container fill)

### 2026-01-29 - Stage 1 Complete

**Commits:**
- `d4f54e9` - Add combat distance system (Stage 1 of ranged weapons)
- `3196f65` - Fix combat continuing after flee (room validation + distance cleanup)
- `f2354b6` - Fix retreat command variable scope error
- `43d78fa` - Melee mobs must close distance before attacking
- `cce18f2` - Melee players must close distance before attacking

**Implementation Notes:**
- PvP distance tracking deferred - current system uses character names as IDs, not UUIDs
- All mobs default to melee preference (future: add MobileData.preferred_combat_style)
- Combat tick validates room location before allowing attacks
- Distance cleanup integrated into all combat exit paths (flee, target death, room change)
- Both players AND mobs with melee weapons must close the gap - retreat is tactically meaningful

---

## Stage 1: Distance States Foundation

### Goal
Add combat distance tracking so melee and ranged weapons have meaningful tactical differences within a room.

### Combat Distance Enum

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CombatDistance {
    #[default]
    Melee,
    Pole,
    Ranged,
}
```

### Distance Tracking

Each combatant tracks their distance to each opponent independently:

```rust
pub struct CombatState {
    pub in_combat: bool,
    pub targets: Vec<CombatTarget>,
    pub stun_rounds_remaining: i32,
    pub distances: HashMap<Uuid, CombatDistance>,  // target_id -> distance
}
```

### Distance Modifiers by Weapon Type

| Weapon Type | Ranged | Pole | Melee |
|-------------|--------|------|-------|
| Ranged (bow/gun) | +2 | -2 | -4 |
| Polearms | -2 | +1 | 0 |
| Long Blades | -4 | -1 | 0 |
| Short Blades | -6 | -3 | +1 |
| Short Blunt | -6 | -3 | 0 |
| Long Blunt | -4 | 0 | 0 |
| Unarmed | -6 | -4 | 0 |

### Distance Initialization

- `attack <target>`: Combat starts at MELEE distance
- Entering room with hostile mobs: Combat starts at RANGED distance

### Commands

**advance** - Move one step closer to primary target
- Ranged -> Pole -> Melee
- No cost, automatic success

**retreat** - Move one step back from primary target
- Melee -> Pole -> Ranged
- Costs 15 stamina
- Requires DEX vs opponent DEX roll
- Leg wounds apply penalty

### Combat Tick Behavior

Melee-preferring mobs (weapon_skill != ranged) auto-advance each combat tick until at melee distance.

---

## Stage 2: Ammunition System & Basic Ranged

### Goal
Add ammunition items, ready slot, and basic bow mechanics.

### New Item Type

```rust
pub enum ItemType {
    // ... existing types ...
    Ammunition,
}
```

### Ammunition Fields

```rust
pub struct ItemData {
    // Ammunition fields
    pub caliber: Option<String>,      // "arrow", "bolt", "9mm", "5.56mm"
    pub ammo_count: i32,              // Stack size
    pub ammo_damage_bonus: i32,       // Quality bonus
}
```

### Ready Wear Location

New wear location for quivers and magazines:

```rust
pub enum WearLocation {
    // ... existing locations ...
    Ready,
}
```

### Ammo Search Priority

1. Ready slot (quiver/magazine)
2. Inventory

If ammo found only in inventory: skip turn + fumble message.

### Commands

**ready** - Manage ready slot
- `ready <item>` - Equip in ready slot
- `ready` - Show current
- `ready remove` - Unequip

**shoot** - Fire at target
- Requires ranged weapon + compatible ammo
- Consumes 1 ammo per shot
- Maintains RANGED distance

---

## Stage 3: Crossbows & Firearms

### Goal
Add weapon-specific mechanics for crossbows and firearms.

### Ranged Weapon Types

```rust
pub enum RangedWeaponType {
    Bow,        // Arrow per shot, silent
    Crossbow,   // Loaded bolts, reload, silent
    Firearm,    // Magazine, reload, loud
}
```

### Fire Modes

```rust
pub enum FireMode {
    Single,     // 1 shot, normal accuracy
    Burst,      // 3 shots, -1 accuracy
    Auto,       // Empty mag, -3 accuracy
}
```

### Weapon Fields

```rust
pub struct ItemData {
    pub ranged_type: Option<RangedWeaponType>,
    pub magazine_size: i32,
    pub loaded_ammo: i32,
    pub fire_mode: FireMode,
    pub supported_fire_modes: Vec<FireMode>,
}
```

### Commands

**reload** - Swap magazines or load bolts
**unload** - Eject magazine/bolts
**fill** - Load loose rounds into magazine (out of combat)
**firemode** - Change fire mode (single/burst/auto)

### Hit Messages by Projectile Type

| Type | Verb Examples |
|------|---------------|
| Arrow | "lodges in", "pierces through", "sticks into" |
| Bolt | "punches into", "embeds in", "tears through" |
| Bullet | "rips into", "tears through", "grazes" |

---

## Stage 4: Cross-Room Sniping & Pursuit

### Goal
Add ability to attack mobs in adjacent rooms with pursuit mechanics.

### Noise Levels

```rust
pub enum NoiseLevel {
    Silent,     // Bows - uncertain pursuit direction
    Quiet,      // Suppressed, crossbows
    Normal,     // Most weapons
    Loud,       // Unsuppressed firearms
}
```

### Pursuit Mechanics

When hit by cross-room attack:
- **Silent**: 50% pursue, random direction if wrong
- **Quiet**: 75% pursue, 50% correct direction
- **Normal**: 100% pursue, correct direction
- **Loud**: 100% pursue + alerts adjacent rooms

### Mobile Flags

```rust
pub struct MobileFlags {
    pub cowardly: bool,  // Flees when sniped or HP < threshold
}
```

### Commands

**snipe** - Attack into adjacent room
- `snipe <direction> [target]`
- Requires ranged weapon with ammo
- Target may pursue

### Pursuit Flow

1. Player snipes mob in adjacent room
2. Mob takes damage, starts pursuit
3. Pursuit tick moves mob toward player
4. Mob enters player's room at RANGED distance
5. Combat continues with distance states

---

## Stage 5: Advanced Features & Polish

### Implementation Order

1. Distance Prompt (standalone Rust change)
2. Special Ammunition (new fields + script changes)
3. Weapon Attachments (new fields + new commands + script changes)
4. Arrow Recovery (depends on Feature 1 for special ammo exclusion)
5. Body-Part Messages (touches same scripts, best done last)

---

### Feature 1: Special Ammunition

Add status effects to ammunition that transfer to the target on ranged hit.

#### New ItemData Fields (Ammunition)

```rust
pub struct ItemData {
    // Special ammo effect payload
    pub ammo_effect_type: String,       // "fire", "cold", "poison", "acid", or "" for none
    pub ammo_effect_duration: i32,      // rounds the effect lasts
    pub ammo_effect_damage: i32,        // damage per tick from the effect
}
```

#### New ItemData Fields (Weapons with Magazines)

When a magazine weapon is reloaded, the ammo effect payload is captured onto the weapon so the shoot/snipe scripts can access it without re-looking up the ammo source:

```rust
pub struct ItemData {
    // Captured at reload for magazine weapons (crossbow, firearm)
    pub loaded_ammo_effect_type: String,
    pub loaded_ammo_effect_duration: i32,
    pub loaded_ammo_effect_damage: i32,
}
```

For bows (no magazine), the effect is read directly from the ammo item at fire time.

#### Effect Application

On ranged hit:
1. Check weapon (or ammo for bows) for `ammo_effect_type`
2. If non-empty, call existing `add_ongoing_effect()` (players) or `add_mobile_ongoing_effect()` (mobs) with the effect type, duration, and damage
3. Uses existing effect types already in the game: `"fire"`, `"cold"`, `"poison"`, `"acid"`

#### Special Ammo Recovery

Special ammunition is **NOT** recoverable from corpses (see Feature 3). The magical/chemical payload is consumed on impact.

#### Rhai Functions

```
get_item_ammo_effect_type(item_id) -> String
set_item_ammo_effect_type(item_id, effect_type) -> bool
get_item_ammo_effect_duration(item_id) -> i32
set_item_ammo_effect_duration(item_id, duration) -> bool
get_item_ammo_effect_damage(item_id) -> i32
set_item_ammo_effect_damage(item_id, damage) -> bool
get_item_loaded_ammo_effect_type(item_id) -> String
set_item_loaded_ammo_effect_type(item_id, effect_type) -> bool
get_item_loaded_ammo_effect_duration(item_id) -> i32
set_item_loaded_ammo_effect_duration(item_id, duration) -> bool
get_item_loaded_ammo_effect_damage(item_id) -> i32
set_item_loaded_ammo_effect_damage(item_id, damage) -> bool
has_ammo_effect(item_id) -> bool   // true if ammo_effect_type is non-empty
```

#### Script Changes

- **shoot.rhai** / **snipe.rhai**: After damage is dealt on hit, check for ammo effect and apply via `add_ongoing_effect()` / `add_mobile_ongoing_effect()`
- **reload.rhai**: When loading magazine weapons, copy `ammo_effect_*` fields from ammo to `loaded_ammo_effect_*` on weapon
- **oedit.rhai**: Add `ammoeffect` subcommand to set effect type, duration, and damage on ammunition items

---

### Feature 2: Weapon Attachments

Weapons act as containers; attachments are items placed inside them. Bonuses are calculated at use time by reading the weapon's container contents.

#### Design

- Attachments are regular items with attachment-specific fields
- Weapons use their existing `container_max_items` field as `attachment_slots`
- Attachments are removable and weapon-type restricted
- One attachment per slot type (e.g., can't have two scopes)
- Bonuses are calculated each round by scanning container contents (not cached)

#### New ItemData Fields (Attachment Items)

```rust
pub struct ItemData {
    // Attachment properties (only meaningful on attachment items)
    pub attachment_slot: String,               // "scope", "suppressor", "magazine", "accessory"
    pub attachment_accuracy_bonus: i32,        // accuracy modifier
    pub attachment_noise_reduction: i32,       // noise level steps reduced (0-2)
    pub attachment_magazine_bonus: i32,        // extra magazine capacity
    pub attachment_compatible_types: Vec<String>,  // ["firearm"], ["crossbow", "firearm"], etc.
}
```

#### Attachment Types

| Type | Slot | Effect | Compatible |
|------|------|--------|------------|
| Scope | scope | +2 accuracy at ranged distance only | All ranged |
| Suppressor | suppressor | -1 noise step | Firearm only |
| Extended Magazine | magazine | +50% magazine capacity | Firearm, Crossbow |
| Laser Sight | accessory | +1 accuracy at all distances | All ranged |

#### Commands

**attach** (`scripts/commands/attach.rhai`)
- `attach <attachment> <weapon>` - Attach an item to a weapon
- Validates: attachment has `attachment_slot`, weapon is ranged, weapon has free slot, compatible type, no duplicate slot

**detach** (`scripts/commands/detach.rhai`)
- `detach <attachment> <weapon>` - Remove an attachment from a weapon
- Returns attachment to inventory

#### Rhai Functions

```
get_weapon_attachment_bonuses(weapon_id) -> Map
    // Returns #{ accuracy_bonus: i32, noise_reduction: i32, magazine_bonus: i32 }
    // Scans weapon's container_contents, sums all attachment bonuses
can_attach_to_weapon(attachment_id, weapon_id) -> Map
    // Returns #{ allowed: bool, reason: String }
    // Checks compatibility, slot availability, etc.
get_item_attachment_slot(item_id) -> String
set_item_attachment_slot(item_id, slot) -> bool
get_item_attachment_accuracy_bonus(item_id) -> i32
set_item_attachment_accuracy_bonus(item_id, bonus) -> bool
get_item_attachment_noise_reduction(item_id) -> i32
set_item_attachment_noise_reduction(item_id, reduction) -> bool
get_item_attachment_magazine_bonus(item_id) -> i32
set_item_attachment_magazine_bonus(item_id, bonus) -> bool
get_item_attachment_compatible_types(item_id) -> Array
set_item_attachment_compatible_types(item_id, types) -> bool
```

#### Script Changes

- **shoot.rhai** / **snipe.rhai**: Call `get_weapon_attachment_bonuses()` and apply accuracy bonus to hit roll; apply noise reduction to effective noise level
- **reload.rhai**: Call `get_weapon_attachment_bonuses()` and add magazine bonus to effective magazine size
- **oedit.rhai**: Add `attachment` subcommand group (slot, accuracy, noise, magazine, compatible)
- **examine.rhai**: If weapon has container contents with attachment slots, display "Attachments:" section

---

### Feature 3: Arrow Recovery

Track projectile vnums embedded in mobs during combat, then recover a percentage on mob death.

#### Design

- On ranged hit with arrow/bolt projectiles, record the ammo vnum on the target mob
- On mob death, process embedded projectiles: 50% recoverable, 25% of recovered are broken
- Bullets are excluded (not recoverable)
- Special ammo (items with `ammo_effect_type` set) is excluded (consumed on impact)

#### New MobileData Field

```rust
pub struct MobileData {
    pub embedded_projectiles: Vec<String>,   // vnums of projectiles embedded in this mob
}
```

#### New ItemFlags Field

```rust
pub struct ItemFlags {
    pub broken: bool,   // Broken arrows/bolts cannot be used as ammo
}
```

#### Recovery Logic

On mob death:
1. Read `embedded_projectiles` from the dead mob
2. For each projectile vnum:
   - Skip if caliber is a bullet type ("9mm", "5.56mm", ".45", ".308", "12gauge")
   - 50% chance the projectile is recoverable
   - Of recovered: 25% chance it spawns with `broken: true` flag
   - Remaining spawn intact into the corpse container
3. Clear `embedded_projectiles` on the mob

#### Rhai Functions

```
embed_projectile(mobile_id, vnum) -> bool
get_embedded_projectiles(mobile_id) -> Array    // returns array of vnum strings
clear_embedded_projectiles(mobile_id) -> bool
process_arrow_recovery(mobile_id, corpse_id) -> Map
    // Returns #{ recovered: i32, broken: i32, lost: i32 }
    // Handles full recovery logic, spawns items into corpse
```

#### Script Changes

- **shoot.rhai** / **snipe.rhai**: On hit, call `embed_projectile()` with the ammo vnum (arrows and bolts only)
- **Death handling**: Call `process_arrow_recovery()` when a mob dies, before corpse is finalized
- **oedit.rhai**: Add `broken` flag support

#### Rust Changes

- **`src/ticks/combat/tick.rs`**: In `process_mobile_death()`, call `process_arrow_recovery()` for tick-based combat deaths to ensure recovery happens for mobs killed by the combat tick

---

### Feature 4: Body-Part Contextual Messages (Ranged Only)

Ranged hits mention a specific body part struck, tied to the wound system. This adds flavor without new mechanics.

#### Message Templates by Ranged Type

| Ranged Type | Low Damage | Medium Damage | High Damage |
|-------------|-----------|---------------|-------------|
| Bow | "arrow grazes {target}'s {part}" | "arrow lodges in {target}'s {part}" | "arrow punches through {target}'s {part}" |
| Crossbow | "bolt nicks {target}'s {part}" | "bolt punches into {target}'s {part}" | "bolt tears through {target}'s {part}" |
| Firearm | "shot grazes {target}'s {part}" | "shot rips into {target}'s {part}" | "shot tears through {target}'s {part}" |

#### Damage Severity Thresholds

- **Low**: damage <= 25% of weapon's max damage dice
- **Medium**: damage 26-75% of weapon's max damage dice
- **High**: damage > 75% of weapon's max damage dice

#### Body Parts

Uses the existing wound/body-part system. Part selection is random weighted:
- Torso (35%), Arms (20%), Legs (20%), Head (10%), Hands (10%), Feet (5%)

#### Script Changes

- **shoot.rhai** / **snipe.rhai**: Replace generic hit messages with body-part contextual messages based on ranged_type and damage severity
- **Combat tick ranged path**: Same replacement for tick-based ranged attacks

No new types or Rhai functions needed — uses existing body part selection and string formatting in scripts.

---

### Feature 5: Distance Display in Combat Prompt

Show the current combat distance to the primary target in the player's prompt during combat.

#### Display Format

During combat, append a distance tag to the prompt:
- **Ranged distance**: `[Ranged: goblin]` (yellow)
- **Pole distance**: `[Pole: goblin]` (cyan)
- **Melee distance**: no tag displayed (melee is the default/expected state)

Shows primary target only (first target in combat target list).

#### Rust Changes

- **`src/lib.rs`** `build_prompt()`: Check if player is in combat. If so, get distance to primary target. If distance is ranged or pole, append colored tag to prompt string.

#### Color Codes

Uses existing ANSI color codes:
- Yellow: `\x1b[33m` ... `\x1b[0m`
- Cyan: `\x1b[36m` ... `\x1b[0m`

---

### Future Enhancements

These features are noted for potential future implementation but are **not** part of Stage 5:

**Suppressive Fire**
- Auto-fire mode pins targets at range, preventing advance for 1-2 rounds
- Consumes large amount of ammo (half magazine)
- Requires firearm with auto fire mode
- Suppressed targets cannot advance and take accuracy penalty

**Aimed Shots**
- Target specific body parts at an accuracy cost
- `aim <body_part>` then `shoot` applies the aimed shot
- Head: -4 accuracy, +50% damage
- Limbs: -2 accuracy, chance to disarm/slow
- Torso: no penalty, no bonus (default)

---

## Rhai Function Reference

### Stage 1 Functions (Implemented)

**Character Distance Functions:**
```
get_combat_distance(char_name, target_id) -> String
set_combat_distance(char_name, target_id, distance) -> bool
get_distance_modifier(weapon_skill, distance) -> i64
can_advance(char_name, target_id) -> bool
can_retreat(char_name, target_id) -> bool
attempt_advance(char_name, target_id) -> bool
attempt_retreat(char_name, target_id) -> String  // "success", "failed", "no_room"
get_distance_step_name(from_distance, to_distance) -> String
weapon_prefers_melee(weapon_skill) -> bool
```

**Mobile Distance Functions:**
```
get_mobile_combat_distance(mobile_id, target_id) -> String
set_mobile_combat_distance(mobile_id, target_id, distance) -> bool
mobile_can_advance(mobile_id, target_id) -> bool
mobile_attempt_advance(mobile_id, target_id) -> bool
```

### Stage 2 Functions (Implemented)

```
get_item_caliber(item_id) -> Option<String>
set_item_caliber(item_id, caliber) -> bool
get_item_ammo_count(item_id) -> i32
set_item_ammo_count(item_id, count) -> bool
consume_ammo(item_id, amount) -> bool
get_readied_item(char_name) -> Option<ItemData>
find_compatible_ammo(char_name, caliber) -> Option<Uuid>
```

### Stage 3 Functions

```
get_ranged_type(item_id) -> Option<String>
get_loaded_ammo(item_id) -> i32
set_loaded_ammo(item_id, count) -> bool
get_magazine_size(item_id) -> i32
get_fire_mode(item_id) -> String
set_fire_mode(item_id, mode) -> bool
can_reload(item_id, char_name) -> bool
perform_reload(item_id, char_name) -> (i32, i32)
```

### Stage 4 Functions (Implemented)

```
get_item_noise_level(item_id) -> String         // effective noise (explicit or default from ranged_type)
set_item_noise_level(item_id, level) -> bool    // "silent", "quiet", "normal", "loud", ""
start_mob_pursuit(mobile_id, target_name, target_room_id, direction, certain)
clear_mob_pursuit(mobile_id)
is_mob_pursuing(mobile_id) -> bool
```

**MobileData pursuit getters:** `pursuit_target_name`, `pursuit_target_room`, `pursuit_direction`, `pursuit_certain`

**Snipe command** (`scripts/commands/snipe.rhai`): Handles sniping logic, pursuit calculation, cowardly flee, and noise broadcast entirely in Rhai script.

### Stage 5 Functions

**Special Ammunition:**
```
get_item_ammo_effect_type(item_id) -> String
set_item_ammo_effect_type(item_id, effect_type) -> bool
get_item_ammo_effect_duration(item_id) -> i32
set_item_ammo_effect_duration(item_id, duration) -> bool
get_item_ammo_effect_damage(item_id) -> i32
set_item_ammo_effect_damage(item_id, damage) -> bool
get_item_loaded_ammo_effect_type(item_id) -> String
set_item_loaded_ammo_effect_type(item_id, effect_type) -> bool
get_item_loaded_ammo_effect_duration(item_id) -> i32
set_item_loaded_ammo_effect_duration(item_id, duration) -> bool
get_item_loaded_ammo_effect_damage(item_id) -> i32
set_item_loaded_ammo_effect_damage(item_id, damage) -> bool
has_ammo_effect(item_id) -> bool
```

**Weapon Attachments:**
```
get_weapon_attachment_bonuses(weapon_id) -> Map
can_attach_to_weapon(attachment_id, weapon_id) -> Map
get_item_attachment_slot(item_id) -> String
set_item_attachment_slot(item_id, slot) -> bool
get_item_attachment_accuracy_bonus(item_id) -> i32
set_item_attachment_accuracy_bonus(item_id, bonus) -> bool
get_item_attachment_noise_reduction(item_id) -> i32
set_item_attachment_noise_reduction(item_id, reduction) -> bool
get_item_attachment_magazine_bonus(item_id) -> i32
set_item_attachment_magazine_bonus(item_id, bonus) -> bool
get_item_attachment_compatible_types(item_id) -> Array
set_item_attachment_compatible_types(item_id, types) -> bool
```

**Arrow Recovery:**
```
embed_projectile(mobile_id, vnum) -> bool
get_embedded_projectiles(mobile_id) -> Array
clear_embedded_projectiles(mobile_id) -> bool
process_arrow_recovery(mobile_id, corpse_id) -> Map
```

**Attach/Detach commands** (`scripts/commands/attach.rhai`, `scripts/commands/detach.rhai`): Handle attaching and removing weapon attachments.

---

## Sample Items

### Bows

```
Longbow:
  type: weapon
  weapon_skill: ranged
  ranged_type: bow
  caliber: arrow
  damage_dice: 1d8
  two_handed: true
```

### Crossbows

```
Heavy Crossbow:
  type: weapon
  weapon_skill: ranged
  ranged_type: crossbow
  caliber: bolt
  magazine_size: 1
  damage_dice: 2d6
  two_handed: true
```

### Firearms

```
9mm Pistol:
  type: weapon
  weapon_skill: ranged
  ranged_type: firearm
  caliber: 9mm
  magazine_size: 15
  fire_modes: [single, burst]
  damage_dice: 2d4

Assault Rifle:
  type: weapon
  weapon_skill: ranged
  ranged_type: firearm
  caliber: 5.56mm
  magazine_size: 30
  fire_modes: [single, burst, auto]
  damage_dice: 2d6
  two_handed: true
```

### Ammunition

```
Standard Arrow:
  type: ammunition
  caliber: arrow
  ammo_damage_bonus: 0

9mm Magazine:
  type: container
  caliber: 9mm
  capacity: 15
```

### Special Ammunition (Stage 5)

```
Fire Arrow:
  type: ammunition
  caliber: arrow
  ammo_damage_bonus: 0
  ammo_effect_type: fire
  ammo_effect_duration: 3
  ammo_effect_damage: 2

Poisoned Bolt:
  type: ammunition
  caliber: bolt
  ammo_damage_bonus: 0
  ammo_effect_type: poison
  ammo_effect_duration: 4
  ammo_effect_damage: 1

Acid-tipped 9mm:
  type: ammunition
  caliber: 9mm
  ammo_damage_bonus: 0
  ammo_effect_type: acid
  ammo_effect_duration: 2
  ammo_effect_damage: 3
```

### Attachments (Stage 5)

```
Rifle Scope:
  type: item
  attachment_slot: scope
  attachment_accuracy_bonus: 2
  attachment_noise_reduction: 0
  attachment_magazine_bonus: 0
  attachment_compatible_types: [bow, crossbow, firearm]

Suppressor:
  type: item
  attachment_slot: suppressor
  attachment_accuracy_bonus: 0
  attachment_noise_reduction: 1
  attachment_magazine_bonus: 0
  attachment_compatible_types: [firearm]

Extended Magazine:
  type: item
  attachment_slot: magazine
  attachment_accuracy_bonus: 0
  attachment_noise_reduction: 0
  attachment_magazine_bonus: 50
  attachment_compatible_types: [firearm, crossbow]

Laser Sight:
  type: item
  attachment_slot: accessory
  attachment_accuracy_bonus: 1
  attachment_noise_reduction: 0
  attachment_magazine_bonus: 0
  attachment_compatible_types: [bow, crossbow, firearm]
```

---

## Testing Checklist

### Stage 1 (Complete)
- [x] `attack goblin` starts combat at melee distance
- [x] Enter room with aggressive mob -> combat at ranged distance
- [x] `advance` moves ranged->pole->melee
- [x] `retreat` costs stamina and requires DEX roll
- [x] Melee mobs auto-advance each combat tick
- [x] Distance modifier applies to hit chance
- [x] Combat ends properly when fleeing to different room

### Stage 2 - COMPLETE
- [x] Can create ammunition items with `oedit`
- [x] Can `ready` a quiver in ready slot
- [x] `shoot goblin` finds ammo from ready slot
- [x] `shoot goblin` without ready ammo = skip turn
- [x] Ammo consumed on shot
- [x] `shoot` initiates/maintains combat at ranged distance

### Stage 3 - COMPLETE
- [x] Crossbow fires loaded bolt (loaded_ammo consumed from weapon)
- [x] Crossbow shows "reload required" when empty
- [x] Firearm fires from magazine (loaded_ammo)
- [x] `reload` loads ammo from ready slot/inventory into weapon magazine
- [x] `unload` ejects loaded ammo back to inventory
- [x] `firemode` cycles/sets fire mode (single/burst/auto)
- [x] Fire modes affect accuracy and shots per round
- [x] Reload in combat costs one combat turn
- [x] Bows unchanged (consume from ready slot, no magazine)
- Note: `fill` command deferred (conflicts with liquid container fill)

### Stage 4 - COMPLETE
- [x] `snipe north goblin` hits mob in adjacent room
- [x] Loud weapon alerts mob to correct direction
- [x] Silent weapon: mob may pursue wrong direction
- [x] Mob arrives at player's room after pursuit
- [x] Cowardly mob flees when sniped
- [x] Cowardly mob flees in regular combat at HP <= 25%
- [x] Closed doors block sniping and pursuit
- [x] `oedit <weapon> noise <level>` sets noise level
- [x] `medit <mob> flag cowardly` toggles cowardly flag

### Stage 5

**Feature 1 - Special Ammunition:**
- [x] `oedit` can set ammo effect type, duration, and damage on ammunition items
- [x] Fire arrow applies burning effect on ranged hit
- [x] Poisoned bolt applies poison effect on ranged hit
- [x] Acid-tipped bullet applies acid effect on ranged hit
- [x] Reload captures ammo effect payload onto magazine weapons
- [x] Bows read effect directly from ammo at fire time
- [x] Special ammo excluded from arrow recovery (consumed on impact)

**Feature 2 - Weapon Attachments:**
- [x] `attach scope rifle` attaches scope to weapon
- [x] `detach scope rifle` removes scope from weapon
- [x] Scope gives +2 accuracy at ranged distance only
- [x] Suppressor reduces noise by 1 step (firearm only)
- [x] Extended magazine increases capacity by 50%
- [x] Laser sight gives +1 accuracy at all distances
- [x] Cannot attach incompatible type (e.g., suppressor on bow)
- [x] Cannot attach duplicate slot (e.g., two scopes)
- [x] `examine` shows weapon attachments

**Feature 3 - Arrow Recovery:**
- [x] Arrows embed in mob on ranged hit
- [x] On mob death, 50% of arrows are recoverable
- [x] 25% of recovered arrows spawn as broken
- [x] Broken arrows cannot be used as ammo
- [x] Bullets are not recoverable
- [x] Combat tick deaths also trigger arrow recovery

**Feature 4 - Body-Part Messages:**
- [x] Ranged hit messages mention body part struck
- [x] Message varies by ranged type (bow/crossbow/firearm)
- [x] Damage severity affects verb choice (grazes/lodges/tears)

**Feature 5 - Distance Prompt:**
- [x] Combat prompt shows `[Ranged: target]` in yellow at ranged distance
- [x] Combat prompt shows `[Pole: target]` in cyan at pole distance
- [x] No distance tag at melee distance
