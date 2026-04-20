# Medical System Documentation

## Overview

A comprehensive medical skill system that allows players to specialize as medics, treating wounds, conditions, and reviving unconscious players. Includes weather exposure effects, healer NPCs, and a helpline channel for emergencies.

**Status: Implemented** (January 2026)

---

## Wound Type System

### Existing Physical Wound Types
| Wound Type | Caused By | Current Mapping |
|------------|-----------|-----------------|
| Cut | Slashing damage | Always from slashing |
| Puncture | Piercing damage | Always from piercing |
| Bruise | Bludgeoning (light) | Minor/moderate bludgeoning |
| Fracture | Bludgeoning (severe) | Severe+ bludgeoning |

### New Elemental Wound Types (to add)
| Wound Type | Caused By | Treatment |
|------------|-----------|-----------|
| Burn | Fire damage | Burn cream, cooling salve |
| Frostbite | Cold damage OR cold exposure | Warming salve |
| Poisoned | Poison damage | Antidote |
| Corroded | Acid damage | Neutralizing salve |

### Changes to src/lib.rs WoundType Enum
```rust
pub enum WoundType {
    Cut,       // Slashing
    Puncture,  // Piercing
    Bruise,    // Light bludgeoning
    Fracture,  // Severe bludgeoning
    Burn,      // Fire (NEW)
    Frostbite, // Cold (NEW)
    Poisoned,  // Poison (NEW)
    Corroded,  // Acid (NEW)
}
```

### Changes to get_wound_type_for_damage()
```rust
match damage_type {
    "slashing" => "cut",
    "piercing" => "puncture",
    "bludgeoning" => if severe { "fracture" } else { "bruise" },
    "fire" => "burn",
    "cold" => "frostbite",
    "poison" => "poisoned",
    "acid" => "corroded",
    "lightning" => "bruise",  // Internal damage
}
```

---

## Medical Supplies System

### Supply-to-Wound Type Mapping

| Supply | Wound Types Treated | Crafting Skill | Quality Effect |
|--------|---------------------|----------------|----------------|
| Bandage | Cut, Puncture | Crafting | +5% success per quality tier |
| Splint | Fracture | Crafting | +5% success per quality tier |
| Bruise Salve | Bruise | Cooking | +5% success per quality tier |
| Burn Cream | Burn | Cooking | +5% success per quality tier |
| Warming Salve | Frostbite | Cooking | +5% success per quality tier |
| Antidote | Poisoned | Cooking | +5% success per quality tier |
| Neutralizer | Corroded | Cooking | +5% success per quality tier |
| Surgery Kit | All physical | Found/bought | Reusable, no quality |

### Quality System Integration
- Items have existing `quality` field (0-100)
- Higher quality = better treatment success
- Quality bonus: `quality / 20` (0-5 bonus to success %)
- Crafting skill affects quality when crafting supplies

### Crafting Recipes

**Crafting Skill (physical tools):**
- Bandage: 2x cloth strip → bandage
- Splint: 2x wood stick + 1x cloth → splint
- Tourniquet: 1x leather strip + 1x wood stick → tourniquet

**Cooking Skill (medicinal items):**
- Bruise Salve: 2x comfrey leaf + 1x oil → bruise salve
- Burn Cream: 2x aloe + 1x oil → burn cream
- Warming Salve: 2x ginger root + 1x oil → warming salve
- Antidote: 2x charcoal + 1x healing herb → antidote
- Neutralizer: 2x chalk powder + 1x water → neutralizer

---

## Medical Skill

### Skill: "medical" (Level 0-10)
- Follows existing XP progression (100, 200, 350... total 10,000 to master)
- Self-treatment allowed with -20% success penalty

### Medical Tools (3 Tiers)

| Tier | Items | Treats | Consumable | Min Skill |
|------|-------|--------|------------|-----------|
| Basic | Bandages, Salves | Minor wounds | Yes | 0 |
| Intermediate | Splints, Antidotes | Moderate wounds, fractures | Yes | 3 |
| Advanced | Surgery Kit | Severe/critical, revival | No (reusable) | 6 |

### Configuring Medical Tools via oedit

Enable the medical_tool flag and configure properties:

```
oedit <item> flag medical_tool on     - Enable medical tool functionality
oedit <item> medical                  - Show current configuration
oedit <item> medical tier <1|2|3>     - Set tier (1=Basic, 2=Intermediate, 3=Advanced)
oedit <item> medical uses <num>       - Set uses (0 = reusable, >0 = consumable)
oedit <item> medical treats add <type>    - Add treatable wound/condition
oedit <item> medical treats remove <type> - Remove treatable wound/condition
oedit <item> medical treats clear         - Clear all treatable types
oedit <item> medical max <severity>       - Set max severity (minor/moderate/severe/critical)
```

**Valid wound types:** cut, puncture, bruise, fracture, burn, frostbite, poisoned, corroded

**Valid conditions:** illness, hypothermia, heat_exhaustion, heat_stroke

**Example configurations:**

```
# Basic Bandage (treats cuts and punctures)
oedit bandage flag medical_tool on
oedit bandage medical tier 1
oedit bandage medical uses 3
oedit bandage medical treats add cut
oedit bandage medical treats add puncture
oedit bandage medical max minor

# Intermediate Splint (treats fractures)
oedit splint flag medical_tool on
oedit splint medical tier 2
oedit splint medical uses 1
oedit splint medical treats add fracture
oedit splint medical max moderate

# Advanced Surgery Kit (treats all physical wounds, reusable)
oedit surgerykit flag medical_tool on
oedit surgerykit medical tier 3
oedit surgerykit medical uses 0
oedit surgerykit medical treats add cut
oedit surgerykit medical treats add puncture
oedit surgerykit medical treats add bruise
oedit surgerykit medical treats add fracture
oedit surgerykit medical max critical

# Warming Salve (treats frostbite and hypothermia)
oedit warmingsalve flag medical_tool on
oedit warmingsalve medical tier 1
oedit warmingsalve medical uses 2
oedit warmingsalve medical treats add frostbite
oedit warmingsalve medical treats add hypothermia
oedit warmingsalve medical max moderate

# Herbal Remedy (treats illness)
oedit herbalremedy flag medical_tool on
oedit herbalremedy medical tier 2
oedit herbalremedy medical uses 1
oedit herbalremedy medical treats add illness
oedit herbalremedy medical max moderate
```

### Treatment Mechanics

**Success Formula:**
```
base = 30 + (skill_level * 5) + tool_bonus + quality_bonus
penalty = wound_level_penalty + self_penalty(-20 if self) + wrong_tool_penalty(-30)
chance = clamp(base + penalty, 5%, 95%)
```

**Wound Level Penalties:** minor: 0, moderate: -10, severe: -20, critical: -30

**On Failure:** Consumable tools are used up with message: "The bandage was ineffective."

### XP Awards
- Base: 20 XP per successful treatment
- Difficulty bonus: +5 XP per wound level above minor
- Save bonus: +50 XP for reviving unconscious player
- Failure: 5 XP (learning experience)

### Unconscious Revival
- Skilled medics (level 6+) with surgery kit can revive unconscious players
- Revives to conscious state with 10% max HP
- Requires active treatment during bleedout window

---

## Healer NPCs

### Tiered Healer System

Three specializations of healer NPCs, each with different capabilities:

| NPC Type | Specialization | Services | Typical Location |
|----------|----------------|----------|------------------|
| Medic | Physical wounds | Treat cuts, punctures, bruises, fractures, bleeding | Military camps, hospitals |
| Herbalist | Elemental/illness | Treat burns, frostbite, poison, acid, illness | Apothecaries, forests |
| Cleric | HP and revival | Restore HP, revive unconscious, cure conditions | Temples, shrines |

### Mobile Flags (add to MobileFlags)
```rust
pub healer: bool,           // Is this mobile a healer?
```

### Mobile Fields (add to MobileData)
```rust
pub healer_type: String,              // "medic", "herbalist", "cleric"
pub healing_free: bool,               // Free healing or charges gold?
pub healing_cost_multiplier: i32,     // 100 = base price, 200 = 2x, etc.
```

### Service Pricing (when healing_free = false)

| Service | Base Cost | Medic | Herbalist | Cleric |
|---------|-----------|-------|-----------|--------|
| Treat minor wound | 25 gold | Yes | Yes | No |
| Treat moderate wound | 75 gold | Yes | Yes | No |
| Treat severe wound | 200 gold | Yes | Yes | No |
| Stop bleeding | 50 gold | Yes | No | No |
| Cure poison | 100 gold | No | Yes | No |
| Cure illness | 150 gold | No | Yes | No |
| Restore HP (per 10 HP) | 20 gold | No | No | Yes |
| Revive unconscious | 500 gold | No | No | Yes |
| Full heal (all HP) | 300 gold | No | No | Yes |

### Interaction via Dialogue Keywords

Players interact with healers by saying keywords:

**Medic keywords:**
- "heal", "treat", "wound" → Offers to treat wounds
- "bleeding", "bandage" → Offers to stop bleeding
- "cost", "price" → Lists prices

**Herbalist keywords:**
- "cure", "poison", "antidote" → Offers poison cure
- "illness", "sick", "cold" → Offers illness treatment
- "burn", "frostbite" → Offers elemental wound treatment

**Cleric keywords:**
- "heal", "restore" → Offers HP restoration
- "revive", "resurrection" → Offers revival service
- "bless", "prayer" → Offers full heal

### Healer Dialogue Response Example
```
> say heal
You say, "heal"
Brother Marcus says, "I can mend your wounds, child. Minor wounds cost 25 gold,
moderate 75, and severe 200. Say 'treat' when ready."

> say treat
Brother Marcus examines your wounds...
Brother Marcus says, "You have a moderate cut on your left arm. That will be 75 gold."
You pay 75 gold.
Brother Marcus carefully treats your wound.
Your left arm cut has been healed!
```

### NPC Configuration via medit

```
medit <id> healer on           - Enable healer flag
medit <id> healer type medic   - Set healer type (medic/herbalist/cleric)
medit <id> healer free on      - Make healing free
medit <id> healer free off     - Charge for healing
medit <id> healer cost 150     - Set cost multiplier (150 = 1.5x prices)
```

---

## Weather Exposure System

### Status Flags (on CharacterData)
- `is_wet: bool` - Currently wet
- `wet_level: i32` (0-100) - How soaked
- `cold_exposure: i32` (0-100) - Accumulated cold
- `heat_exposure: i32` (0-100) - Accumulated heat
- `illness_progress: i32` (0-100) - Illness severity

### Conditions (escalated from exposure)

| Condition | Trigger | Effects |
|-----------|---------|---------|
| Hypothermia | cold_exposure >= 50 | Shivering, -2 DEX, -1 STR |
| Frostbite | cold_exposure >= 80 | Wounds on hands/feet (escalating) |
| Heat Exhaustion | heat_exposure >= 50 | Stamina drain, -2 CON |
| Heat Stroke | heat_exposure >= 80 | Collapse risk, HP drain |
| Illness (cold/flu) | wet + cold extended | HP drain, sneezing/coughing |

### Visible Symptoms

Players experiencing exposure or illness display involuntary symptoms visible to others in the room. These serve as visual cues for roleplaying and alerting nearby players to someone in distress.

**Cold Exposure (shivering):**
| Exposure Level | Player Message | Room Message | Chance/tick |
|----------------|----------------|--------------|-------------|
| 25-49 | "You shiver from the cold." | "<Name> shivers." | ~5-9% |
| 50-74 | "You shiver uncontrollably!" | "<Name> shivers uncontrollably." | ~10-14% |
| 75+ | "You shiver violently from the bitter cold!" | "<Name> shivers violently." | ~15-20% |

**Heat Exposure (sweating):**
| Exposure Level | Player Message | Room Message | Chance/tick |
|----------------|----------------|--------------|-------------|
| 25-49 | "You feel yourself starting to sweat." | "<Name> is starting to sweat." | ~5-9% |
| 50-74 | "You wipe the sweat from your brow." | "<Name> wipes sweat from their brow." | ~10-14% |
| 75+ | "Sweat pours down your face..." | "<Name> is drenched in sweat." | ~15-20% |

**Illness (sneezing and coughing):**
| Illness Level | Symptom | Player Message | Room Message | Chance/tick |
|---------------|---------|----------------|--------------|-------------|
| 50+ (mild) | Sneeze | "You sneeze uncontrollably!" | "<Name> sneezes." | ~10% |
| 75+ (severe) | Cough | "You have a coughing fit!" | "<Name> coughs violently." | ~15% |

Note: Exposure tick runs every 30 seconds. A 10% chance per tick means the symptom appears roughly every 5 minutes on average.

### Indoor Temperature Moderation

Indoor rooms moderate temperature extremes by shifting the effective temperature 60% toward 15°C (comfortable):

```
effective_temp = outdoor_temp + ((15 - outdoor_temp) * 60 / 100)
```

**Examples:**
| Outdoor Temp | Indoor Effective | Category Change |
|--------------|------------------|-----------------|
| 5°C (41°F) | 11°C (52°F) | Cold → Cool (safe) |
| -10°C (14°F) | 5°C (41°F) | Freezing → Cold |
| 35°C (95°F) | 23°C (73°F) | Sweltering → Warm |
| 40°C (104°F) | 25°C (77°F) | Sweltering → Warm |

This means mild outdoor temperatures (like 5°C) are completely safe indoors.

### Temperature Categories and Insulation Requirements

| Category | Temp Range | Insulation Needed | Notes |
|----------|------------|-------------------|-------|
| Freezing | < 0°C | 80 | Dangerous, heavy clothing required |
| Cold | 0-9°C | 40 | Uncomfortable, moderate clothing |
| Cool | 10-14°C | 0 | Safe, no insulation needed |
| Mild | 15-19°C | 0 | Comfortable |
| Warm | 20-24°C | 0 | Comfortable |
| Hot | 25-34°C | 0 (heat threshold) | Heat exposure if high insulation |
| Sweltering | ≥35°C | N/A | Heat exposure accumulates |

### Wet Mechanic

**Applied by:**
- Rain/snow outdoors without waterproof gear: +20/tick
- Swimming/wading: instant 100

**Removed by:**
- Near warmth source (provides_warmth item): -30/tick
- Indoors: -10/tick
- Outdoors (clear weather): -5/tick

### Protection

**Insulation (existing 0-100):**
- Protects against cold
- Works against you in hot weather (causes overheating)
- Wet clothing loses `wet_level / 2` percent effectiveness

**Item Flag: `waterproof`**
- Prevents getting wet from rain
- Added to appropriate clothing items
- Must be equipped to provide protection

**Item Flag: `provides_warmth`**
- Items like campfires, fireplaces, braziers radiate warmth to the room
- Boosts cold recovery and drying rates for all players in the room
- Does not need to be equipped - just present in the room
- Set via: `oedit <item> flag provides_warmth`

### Recovery Rates

Recovery from exposure is faster indoors, and even faster near warmth sources:

| Condition | Outdoors | Indoors | Near Warmth |
|-----------|----------|---------|-------------|
| Cold (threshold met) | -5/tick | -10/tick | -20/tick |
| Cold (warm enough) | -10/tick | -15/tick | -25/tick |
| Drying (wet) | -5/tick | -10/tick | -30/tick |

Note: Players can dry near a warmth source even outdoors during bad weather.

---

## Helpline Channel

### Subscription
- `set helpline on|off` - Toggle listening to distress calls
- Similar to existing builder debug channel pattern

### Auto-Alerts (triggered by system)
- When player goes unconscious
- When bleeding severity >= 3

### Manual Alerts
- `distress` command sends help request
- `distress <message>` with custom text

### Message Format
```
[HELPLINE] PlayerName needs help! Location: Room Name (Area)
[HELPLINE] PlayerName is unconscious and bleeding out! Location: ...
```

---

## New Commands

| Command | Usage | Description |
|---------|-------|-------------|
| `treat` | `treat <target> [wound]` | Treat wounds on target |
| `diagnose` | `diagnose <target>` | View medical status |
| `stabilize` | `stabilize <target>` | Emergency stabilization |
| `revive` | `revive <target>` | Wake unconscious player |
| `distress` | `distress [message]` | Call for help |

---

## Implementation Details

The following changes were made to implement this system.

### src/lib.rs - New Fields

**CharacterData:**
```rust
pub is_wet: bool,
pub wet_level: i32,
pub cold_exposure: i32,
pub heat_exposure: i32,
pub illness_progress: i32,
pub has_hypothermia: bool,
pub has_frostbite: Vec<BodyPart>,
pub has_heat_exhaustion: bool,
pub has_heat_stroke: bool,
pub has_illness: bool,
pub helpline_enabled: bool,
```

**ItemFlags:**
```rust
pub waterproof: bool,
pub provides_warmth: bool,
pub medical_tool: bool,
```

**ItemData:**
```rust
pub medical_tier: i32,              // 1, 2, or 3
pub medical_uses: i32,              // 0 = reusable
pub treats_wound_types: Vec<String>, // ["cut", "puncture", "burn", etc.]
pub max_treatable_wound: String,    // "minor", "moderate", "severe", "critical"
```

**MobileFlags:**
```rust
pub healer: bool,              // Is this mobile a healer?
```

**MobileData:**
```rust
pub healer_type: String,              // "medic", "herbalist", "cleric"
pub healing_free: bool,               // Free healing or charges gold?
pub healing_cost_multiplier: i32,     // 100 = base price, 200 = 2x, etc.
```

**WoundType Enum (add variants):**
```rust
Burn,       // Fire damage
Frostbite,  // Cold damage
Poisoned,   // Poison damage
Corroded,   // Acid damage
```

### New File: src/script/medical.rs

Register functions for:
- Treatment: `attempt_treatment()`, `attempt_revive()`, `calculate_treatment_success()`
- Tools: `is_medical_tool()`, `consume_medical_tool()`, `find_best_medical_tool()`
- Tool matching: `can_tool_treat_wound_type()`, `get_tool_quality_bonus()`
- Exposure: `apply_wet_status()`, `dry_character()`, `get_effective_insulation()`
- Conditions: `has_condition()`, `apply_condition_effects()`
- Helpline: `broadcast_to_helpline()`, `set_helpline_enabled()`

### New File: src/script/healers.rs

Register functions for:
- NPC identification: `is_healer()`, `get_healer_type()`, `find_healer_in_room()`
- Services: `can_healer_treat()`, `get_healing_cost()`, `perform_npc_healing()`
- Dialogue: Integrate with existing say.rhai trigger system

### src/main.rs - Exposure Tick

New background task running every 30 seconds:
1. Check weather and room conditions
2. Update wet status based on weather/location
3. Calculate temperature exposure with insulation
4. Apply/progress conditions
5. Deal condition damage
6. Auto-alert helpline for critical states

### Scripts Created
- `scripts/commands/treat.rhai` - Wound treatment command
- `scripts/commands/diagnose.rhai` - Medical status display
- `scripts/commands/stabilize.rhai` - Emergency bleeding stabilization
- `scripts/commands/revive.rhai` - Revive unconscious players
- `scripts/commands/distress.rhai` - Helpline distress calls

### Scripts Modified
- `scripts/commands/set.rhai` - Added helpline setting
- `scripts/commands/attack.rhai` - Auto-alert on unconscious/heavy bleeding, elemental wounds
- `scripts/commands/oedit.rhai` - Medical tool properties, waterproof flag
- `scripts/commands/medit.rhai` - Healer configuration subcommands
- `scripts/commands/say.rhai` - Healer dialogue trigger handling
- `scripts/commands/skills.rhai` - Added medical to skill categories
- `src/script/combat.rs` - get_wound_type_for_damage() for elemental types

---

## Testing Checklist

### 1. Wound Type System
- [ ] Attack with fire weapon, verify burn wound created
- [ ] Attack with poison weapon, verify poisoned wound created
- [ ] Check `diagnose` shows correct wound types

### 2. Medical Supplies
- [ ] Create bandage via crafting, verify quality from crafting skill
- [ ] Use bandage on cut wound - should work
- [ ] Use bandage on burn wound - should fail (wrong type)
- [ ] Use burn cream on burn wound - should work

### 3. Medical Skill
- [ ] Create character, verify medical skill at level 0
- [ ] Treat wound, verify XP gain with `skills`
- [ ] Self-treatment should have lower success rate

### 4. Quality System
- [ ] Craft low-quality bandage (low crafting skill)
- [ ] Craft high-quality bandage (high crafting skill)
- [ ] Verify high-quality has better success rate

### 5. Healer NPCs
- [ ] Create medic NPC: `medit test healer on`, `medit test healer type medic`
- [ ] Set as paid: `medit test healer free off`, `medit test healer cost 100`
- [ ] Say "heal" near medic, verify dialogue response
- [ ] Say "treat" to trigger wound treatment
- [ ] Verify gold deducted on healing

### 6. Free Healer
- [ ] Create cleric NPC, set as free: `medit temple_priest healer free on`
- [ ] Interact, verify no gold charged

### 7. Weather Exposure
- [ ] Stand in rain outdoors, verify wet status applied
- [ ] Enter cold room while wet, verify cold_exposure increases
- [ ] Check hypothermia triggers at threshold
- [ ] Verify frostbite creates Frostbite wound type
- [x] Verify indoor temperature moderation (5°C outdoors = safe indoors)
- [x] Verify Cool category (10-14°C) requires no insulation

### 8. Helpline Channel
- [ ] Enable helpline on medic character: `set helpline on`
- [ ] Have another player use `distress`
- [ ] Verify medic receives message with location
- [ ] Verify auto-alert when player goes unconscious

### 9. Revival Flow
- [ ] Player A goes unconscious in combat
- [ ] System auto-alerts helpline
- [ ] Player B (medic) arrives, uses `revive` command
- [ ] Verify revival succeeds with high skill + surgery kit
- [ ] Verify gold charged if using paid cleric NPC instead
