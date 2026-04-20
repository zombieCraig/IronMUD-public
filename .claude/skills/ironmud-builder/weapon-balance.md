# Weapon Damage Balance Reference

Use these tables when creating weapons via MCP tools. Per-shot damage for burst/auto ranged weapons must be lower than melee equivalents because fire mode multiplies damage (burst = 3x, auto = all loaded rounds).

## Damage Scale

| Avg | Dice | Tier | Examples |
|-----|------|------|----------|
| 1.5 | 1d2 | Unarmed | Fists |
| 2.0 | 1d3 | Improvised | Rock, bottle, pipe |
| 2.5 | 1d4 | Light | Knife, club, sling, stun gun |
| 3.5 | 1d6 | Standard light | Shortsword, mace, shortbow, holdout pistol, baton |
| 4.5 | 1d8 | Standard martial | Longsword, longbow, 9mm pistol, assault rifle (per-shot) |
| 5.5 | 1d10 | Heavy 1H / polearm | Bastard sword, pike, revolver, hunting rifle, vibroblade |
| 6.5 | 1d12 | High-end | Greataxe, magnum, lance, nano blade |
| 7.0 | 2d6 | Two-handed power | Greatsword, maul, pump shotgun, plasma rifle |
| 9.0 | 2d8 | Elite | Sniper rifle, monofilament katana, gauss rifle, heavy laser |
| 11.0 | 2d10 | Legendary | Railgun, gravity hammer |
| 13.0 | 2d10+2 | Artifact ceiling | Disintegrator (absolute max) |

**Hard ceiling: 2d10+2.** Do not exceed this for any weapon.

## Medieval Weapons

### Melee

| Weapon | Dice | Type | Skill | 2H |
|--------|------|------|-------|----|
| Fists | 1d2 | bludgeoning | melee | No |
| Rock / Bottle | 1d3 | bludgeoning | melee | No |
| Knife / Dagger | 1d4 | piercing | melee | No |
| Club | 1d4 | bludgeoning | melee | No |
| Shortsword | 1d6 | slashing | melee | No |
| Mace | 1d6 | bludgeoning | melee | No |
| Handaxe | 1d6 | slashing | melee | No |
| Longsword | 1d8 | slashing | melee | No |
| Warhammer | 1d8 | bludgeoning | melee | No |
| Rapier | 1d8 | piercing | melee | No |
| Bastard Sword | 1d10 | slashing | melee | No |
| Battle Axe | 1d10 | slashing | melee | No |
| Spear | 1d10 | piercing | melee | No |
| Pike | 1d10 | piercing | melee | Yes |
| Lance | 1d12 | piercing | melee | Yes |
| Greatsword | 2d6 | slashing | melee | Yes |
| Maul | 2d6 | bludgeoning | melee | Yes |
| Halberd | 2d6 | slashing | melee | Yes |

### Ranged

| Weapon | Dice | Type | Fire | Caliber | 2H |
|--------|------|------|------|---------|----|
| Sling | 1d4 | bludgeoning | single | — | No |
| Shortbow | 1d6 | piercing | single | arrow | Yes |
| Longbow | 1d8 | piercing | single | arrow | Yes |
| Light Crossbow | 1d8 | piercing | single | bolt | Yes |
| Heavy Crossbow | 1d10 | piercing | single | bolt | Yes |
| Composite Bow | 1d10 | piercing | single | arrow | Yes |

## Modern Weapons

### Melee

| Weapon | Dice | Type | Notes |
|--------|------|------|-------|
| Improvised | 1d3 | bludgeoning | Breakable |
| Combat Knife | 1d4 | piercing | Concealable |
| Baton | 1d6 | bludgeoning | Law enforcement |
| Machete | 1d8 | slashing | Utility blade |
| Fire Axe | 1d10 | slashing | Two-handed |
| Sledgehammer | 2d6 | bludgeoning | Two-handed |

### Ranged

| Weapon | Dice | Type | Modes | Caliber | Mag | 2H |
|--------|------|------|-------|---------|-----|----|
| Holdout Pistol | 1d6 | ballistic | single | 9mm | 6 | No |
| 9mm Pistol | 1d8 | ballistic | single, burst | 9mm | 15 | No |
| .45 Pistol | 1d10 | ballistic | single | .45 | 8 | No |
| Revolver | 1d10 | ballistic | single | .45 | 6 | No |
| Magnum | 1d12 | ballistic | single | .45 | 6 | No |
| Pump Shotgun | 2d6 | ballistic | single | 12gauge | 6 | Yes |
| Assault Rifle | 1d8 | ballistic | single, burst, auto | 5.56mm | 30 | Yes |
| Hunting Rifle | 1d10 | ballistic | single | .308 | 5 | Yes |
| SMG | 1d6 | ballistic | burst, auto | 9mm | 30 | No |
| Sniper Rifle | 2d8 | ballistic | single | .308 | 5 | Yes |

### Effective DPS (Ranged)

Fire mode multiplies per-shot damage:

| Weapon | Per-Shot | Mode | Effective |
|--------|----------|------|-----------|
| 9mm Pistol | 1d8 | burst | 3d8 (avg 13.5) |
| Assault Rifle | 1d8 | burst | 3d8 (avg 13.5) |
| Assault Rifle | 1d8 | auto (30) | 30d8 (theoretical max) |
| SMG | 1d6 | burst | 3d6 (avg 10.5) |

Auto accuracy degrades per shot and empties the magazine, so real output is much lower than theoretical.

## Cyberpunk Weapons

### Enhanced Melee

| Weapon | Dice | Type | Notes |
|--------|------|------|-------|
| Stun Baton | 1d4 | lightning | Non-lethal |
| Monofilament Whip | 1d8 | slashing | Concealable |
| Vibroblade | 1d10 | slashing | Vibrating edge |
| Power Fist | 1d10 | bludgeoning | Cybernetic |
| Monofilament Katana | 2d8 | slashing | Elite, two-handed |

### Energy Weapons

| Weapon | Dice | Type | Modes | Mag | 2H |
|--------|------|------|-------|-----|----|
| Ion Stunner | 1d6 | lightning | single | 15 | No |
| Laser Pistol | 1d8 | fire | single | 20 | No |
| Plasma Rifle | 2d6 | fire | single, burst | 12 | Yes |
| Heavy Laser | 2d8 | fire | single | 8 | Yes |
| Disintegrator | 2d10+2 | acid | single | 3 | Yes |

### Advanced Firearms

| Weapon | Dice | Type | Modes | Caliber | Mag | 2H |
|--------|------|------|-------|---------|-----|----|
| Smart Pistol | 1d8 | ballistic | single, burst | 9mm | 20 | No |
| Gauss Rifle | 2d8 | ballistic | single | 5.56mm | 10 | Yes |
| Railgun | 2d10 | ballistic | single | .308 | 1 | Yes |

### Exotic

| Weapon | Dice | Type | Notes |
|--------|------|------|-------|
| Neural Disruptor | 1d8 | lightning | Causes confusion |
| Gravity Hammer | 2d10 | bludgeoning | Legendary, two-handed |
| Nano Blade | 1d12 | slashing | Self-repairing |
| Arc Caster | 1d6 | lightning | Chain effect |

## Mobile Level Correlation

| Mob Level | Mob Damage | Player Weapon Tier | Dice |
|-----------|------------|--------------------|------|
| 1 | 1d4 | Light | 1d4 |
| 2 | 1d6 | Standard light | 1d6 |
| 3-4 | 1d8+1 | Standard martial | 1d8 |
| 5-6 | 2d6 | Heavy | 1d10 |
| 7-8 | 2d8+2 | Two-handed power | 2d6 |
| 9-10 | 3d8+4 | Elite | 2d8 |

Players deal less per-hit than same-level mobs since players have 100 HP, healing, and better tactics.
