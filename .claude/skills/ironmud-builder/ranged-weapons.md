# Ranged Weapons Builder Reference

## Ranged Weapon Types

| Type | `ranged_type` | Ammo Source | Noise Default | Notes |
|------|---------------|-------------|---------------|-------|
| Bow | `bow` | Ready slot (arrow per shot) | Silent | No magazine; consumes directly from quiver |
| Crossbow | `crossbow` | Magazine (loaded bolts) | Quiet | Must `reload`; fires from internal magazine |
| Firearm | `firearm` | Magazine (loaded rounds) | Loud | Must `reload`; supports fire modes |

## Creating Ranged Weapons (oedit)

Set these fields on a weapon item:

| Field | Required | Description |
|-------|----------|-------------|
| `item_type` | Yes | `weapon` |
| `weapon_skill` | Yes | `ranged` |
| `ranged_type` | Yes | `bow`, `crossbow`, or `firearm` |
| `caliber` | Yes | Must match ammo caliber (see below) |
| `damage_dice_count` | Yes | Number of dice |
| `damage_dice_sides` | Yes | Sides per die |
| `two_handed` | Often | `true` for longbows, rifles, heavy crossbows |
| `magazine_size` | Crossbow/Firearm | How many rounds the weapon holds |
| `fire_mode` | Firearm | Default fire mode: `single`, `burst`, or `auto` |
| `supported_fire_modes` | Firearm | Array of available modes |
| `noise_level` | Optional | Override default: `silent`, `quiet`, `normal`, `loud` |

### Caliber Values

| Caliber | Weapon Type |
|---------|-------------|
| `arrow` | Bows |
| `bolt` | Crossbows |
| `9mm` | Pistols |
| `5.56mm` | Assault rifles |
| `.45` | Heavy pistols |
| `.308` | Sniper rifles |
| `12gauge` | Shotguns |

### Sample Weapons

**Longbow** — silent, two-handed, arrow-per-shot
```
item_type: weapon
weapon_skill: ranged
ranged_type: bow
caliber: arrow
damage_dice: 1d8
two_handed: true
```

**Heavy Crossbow** — quiet, magazine-based, single bolt
```
item_type: weapon
weapon_skill: ranged
ranged_type: crossbow
caliber: bolt
magazine_size: 1
damage_dice: 1d10
two_handed: true
```

**9mm Pistol** — loud, magazine-based, single/burst
```
item_type: weapon
weapon_skill: ranged
ranged_type: firearm
caliber: 9mm
magazine_size: 15
supported_fire_modes: [single, burst]
damage_dice: 1d8
```

**Assault Rifle** — loud, two-handed, all fire modes
```
item_type: weapon
weapon_skill: ranged
ranged_type: firearm
caliber: 5.56mm
magazine_size: 30
supported_fire_modes: [single, burst, auto]
damage_dice: 1d8
two_handed: true
```

## Creating Ammunition (oedit)

| Field | Required | Description |
|-------|----------|-------------|
| `item_type` | Yes | `ammunition` |
| `caliber` | Yes | Must match weapon caliber |
| `ammo_count` | Yes | Stack size (e.g., 20 arrows, 15 rounds) |
| `ammo_damage_bonus` | Optional | Quality bonus to damage (default 0) |

### Special Ammo Fields

| Field | Description |
|-------|-------------|
| `ammo_effect_type` | `fire`, `cold`, `poison`, or `acid` |
| `ammo_effect_duration` | Rounds the effect lasts |
| `ammo_effect_damage` | Damage per tick from the effect |

Special ammo applies an ongoing effect on hit. It is **not recoverable** from corpses (consumed on impact).

### Sample Ammunition

**Standard Arrow** (20-stack)
```
item_type: ammunition
caliber: arrow
ammo_count: 20
ammo_damage_bonus: 0
```

**Crossbow Bolt** (10-stack)
```
item_type: ammunition
caliber: bolt
ammo_count: 10
ammo_damage_bonus: 0
```

**9mm Rounds** (15-stack)
```
item_type: ammunition
caliber: 9mm
ammo_count: 15
ammo_damage_bonus: 0
```

**Fire Arrow** (special ammo)
```
item_type: ammunition
caliber: arrow
ammo_count: 5
ammo_damage_bonus: 0
ammo_effect_type: fire
ammo_effect_duration: 3
ammo_effect_damage: 2
```

**Poisoned Bolt** (special ammo)
```
item_type: ammunition
caliber: bolt
ammo_count: 5
ammo_damage_bonus: 0
ammo_effect_type: poison
ammo_effect_duration: 4
ammo_effect_damage: 1
```

## Creating Attachments (oedit)

Attachments are items that can be placed inside ranged weapons to modify their stats. One attachment per slot type.

| Field | Description |
|-------|-------------|
| `attachment_slot` | `scope`, `suppressor`, `magazine`, or `accessory` |
| `attachment_accuracy_bonus` | Hit roll modifier |
| `attachment_noise_reduction` | Noise level steps reduced (0-2) |
| `attachment_magazine_bonus` | Extra magazine capacity (percentage) |
| `attachment_compatible_types` | Array of compatible `ranged_type` values |

### Attachment Types

| Attachment | Slot | Effect | Compatible With |
|------------|------|--------|-----------------|
| Scope | `scope` | +2 accuracy at ranged distance | All ranged |
| Suppressor | `suppressor` | -1 noise step | Firearm only |
| Extended Magazine | `magazine` | +50% magazine capacity | Firearm, Crossbow |
| Laser Sight | `accessory` | +1 accuracy at all distances | All ranged |

### Sample Attachments

**Rifle Scope**
```
attachment_slot: scope
attachment_accuracy_bonus: 2
attachment_noise_reduction: 0
attachment_magazine_bonus: 0
attachment_compatible_types: [bow, crossbow, firearm]
```

**Suppressor**
```
attachment_slot: suppressor
attachment_accuracy_bonus: 0
attachment_noise_reduction: 1
attachment_magazine_bonus: 0
attachment_compatible_types: [firearm]
```

**Extended Magazine**
```
attachment_slot: magazine
attachment_accuracy_bonus: 0
attachment_noise_reduction: 0
attachment_magazine_bonus: 50
attachment_compatible_types: [firearm, crossbow]
```

**Laser Sight**
```
attachment_slot: accessory
attachment_accuracy_bonus: 1
attachment_noise_reduction: 0
attachment_magazine_bonus: 0
attachment_compatible_types: [bow, crossbow, firearm]
```

## Equipping Mobiles with Ranged Weapons

Mobiles need both a weapon and ammunition via spawn dependencies.

### Bow-Armed Mobile

1. Create weapon prototype (bow)
2. Create ammo prototype (arrows)
3. Create mobile spawn point
4. Add spawn dependency: **weapon** as `equipped`, `wear_location: wielded`
5. Add spawn dependency: **ammo** as `equipped`, `wear_location: ready`

The arrow stack goes in the `ready` slot (quiver). Bows consume directly from the ready slot each shot.

### Crossbow/Firearm-Armed Mobile

1. Create weapon prototype (crossbow/firearm)
2. Create ammo prototype (matching caliber)
3. Create mobile spawn point
4. Add spawn dependency: **weapon** as `equipped`, `wear_location: wielded`
5. Add spawn dependency: **ammo** as `inventory`

Magazine weapons load from inventory via `reload`. The mob's combat AI handles reloading automatically.

### Attachment on Spawned Weapon

Attachments cannot currently be added via spawn dependencies. Create the weapon prototype with desired stats instead.

## Distance and Combat Notes

### Combat Distances

Combat occurs at three distances within a room:

| Distance | When | Effect |
|----------|------|--------|
| `melee` | `attack` command, or mob advances | Melee weapons effective |
| `pole` | Middle ground (advance/retreat) | Polearms effective |
| `ranged` | Room entry with hostile mob, or `shoot` | Ranged weapons effective |

### Aggressive Mob Behavior

- When a player enters a room with an **aggressive** mob, combat starts at **ranged** distance
- Mobs with melee weapons auto-advance toward melee each combat tick
- Mobs with ranged weapons stay at range and shoot

### Mobile Flags for Ranged Combat

| Flag | Effect |
|------|--------|
| `cowardly` | Mob flees combat when HP drops to 25% or below; also flees when sniped from adjacent room |

### Triggers for Ranged Combat

| Trigger | When Fired | Use |
|---------|------------|-----|
| `on_flee` | Mobile flees combat | React to mob fleeing (call for help, drop items) |

The `@shout` template can be used in trigger scripts to broadcast messages to adjacent rooms (e.g., a fleeing mob shouting for reinforcements).

### Cross-Room Sniping

Players can `snipe <direction> [target]` to attack mobs in adjacent rooms. Noise level determines pursuit behavior:

| Noise Level | Pursuit Chance | Direction Accuracy |
|-------------|---------------|-------------------|
| Silent (bows) | 50% | Random direction |
| Quiet (crossbows, suppressed) | 75% | 50% correct |
| Normal | 100% | Correct direction |
| Loud (firearms) | 100% + alerts adjacent rooms | Correct direction |

Closed doors block both sniping and pursuit.

### Arrow/Bolt Recovery

Arrows and bolts embed in targets on hit. On mob death:
- 50% of embedded projectiles are recoverable
- 25% of recovered projectiles spawn as broken (unusable)
- Bullets are never recoverable
- Special ammo (with effects) is never recoverable
