# Character Creation System

IronMUD's character creation system provides a flexible wizard for new character setup. The system is designed for modern/futuristic themes and is not restricted to traditional fantasy tropes.

## Overview

When a player creates a new character, they go through a multi-step wizard that allows customization of:

- **Race**: Free-form text entry (player types whatever they want)
- **Short Description**: Brief character appearance/description
- **Class/Occupation**: Selectable from a data-driven list
- **Traits**: Positive and negative traits with point costs

The wizard uses a "quick defaults + edit" approach: characters start with random/default values, and players can edit any section before finalizing.

## Design Philosophy

1. **Theme Agnostic**: No hardcoded D&D-style races or classes. All options are data-driven.
2. **Free-Form Where Appropriate**: Race is pure text - players can be anything.
3. **Expandable**: Classes and traits are loaded from JSON files, easily customizable.
4. **Consequential Choices**: Traits are permanent to make decisions meaningful.

---

## Character Creation Flow

### Step 1: Initial Create Command

```
create <character_name> <password>
```

Validates name and password, checks for existing character, creates character with defaults.

### Step 2: Wizard Menu

```
=== Character Creation: CharacterName ===

  [1] Race:        Human
  [2] Description: A nondescript adventurer.
  [3] Class:       Unemployed
  [4] Traits:      (none) (Points: 10)

  [R] Randomize all
  [D] Done - Finish character creation
  [Q] Quit - Cancel and delete character

Enter number to edit, or R/D/Q:
```

### Step 3: Section Editing

Each section (except Traits) supports:
- Type a value to set it
- Type `random` for a random selection
- Type `back` to return to menu

### Step 4: Trait Selection

```
=== Trait Selection (Points: 10) ===

Your traits: (none)

Available Positive Traits (cost points):
  [1] Strong (+3 STR) - Costs 3 points

Available Negative Traits (grant points):
  [2] Clumsy (-2 DEX) - Grants 2 points

WARNING: Traits are PERMANENT once accepted!

Commands:
  <number> - Preview trait details
  accept <number> - PERMANENTLY add trait
  back - Return to menu
```

### Step 5: Finalization

When player enters `D` (Done):
1. `creation_complete` flag is set to `true`
2. Character is saved to database
3. Player is shown login instructions

---

## Permanence Rules

### During Character Creation (before Done)

| Field | Can Change? | Notes |
|-------|-------------|-------|
| Race | Yes | Free text, can edit until Done |
| Description | Yes | Can edit until Done |
| Class | Yes | Can change selection until Done |
| Traits | **PERMANENT** | Once accepted, cannot be removed |

### After Character Creation (after Done)

| Field | Can Change? | How |
|-------|-------------|-----|
| Race | **No** | Locked forever |
| Class | **No** | Locked forever |
| Description | Yes | `describe <new description>` command |
| Traits | Add only | `traits` command (if points remain) |

**Important**: Trait points spent are gone forever. You cannot remove accepted traits or get points back.

---

## Data Structures

### CharacterData Fields

```rust
pub struct CharacterData {
    // ... existing fields ...

    // Character creation fields
    pub race: String,              // Free-form text
    pub short_description: String, // Brief character description
    pub class_name: String,        // Class identifier (default: "unemployed")
    pub traits: Vec<String>,       // List of trait identifiers
    pub trait_points: i32,         // Remaining trait point pool (default: 10)
    pub creation_complete: bool,   // Whether wizard was completed
}
```

### ClassDefinition

```rust
pub struct ClassDefinition {
    pub id: String,                         // Unique identifier
    pub name: String,                       // Display name
    pub description: String,                // Description shown to player
    pub starting_skills: HashMap<String, i32>, // Skill name -> starting level
    pub stat_bonuses: HashMap<String, i32>, // Stat abbreviation -> bonus
    pub available: bool,                    // Selectable in character creation
}
```

**Available Classes:**

| Class | Starting Skills | Stat Bonuses |
|-------|----------------|--------------|
| Unemployed | (none) | (none) |
| Chef | cooking:2, short_blades:1, foraging:1 | CON+1, WIS+1, CHA+1 |
| Paramedic | medical:2, unarmed:1, foraging:1 | WIS+2, DEX+1 |
| Park Ranger | foraging:2, ranged:1, fishing:1 | CON+1, WIS+1, DEX+1 |
| Security Guard | unarmed:2, short_blunt:1, ranged:1 | STR+1, CON+2 |
| Dock Worker | fishing:2, long_blunt:1, crafting:1 | STR+2, CON+1 |
| Mechanic | crafting:2, short_blades:1, short_blunt:1 | INT+2, DEX+1 |
| Soldier | ranged:2, long_blades:1, polearms:1 | STR+1, DEX+1, CON+1 |

Each class gets 4 starting skills (one at level 2, three at level 1) and +3 total stat bonuses. Bonuses are applied when the player finalizes character creation.

### TraitDefinition

```rust
pub struct TraitDefinition {
    pub id: String,                    // Unique identifier
    pub name: String,                  // Display name
    pub description: String,           // Description shown to player
    pub cost: i32,                     // Positive = costs, negative = grants
    pub category: TraitCategory,       // Positive or Negative
    pub effects: HashMap<String, i32>, // Stat/skill mods (future)
    pub conflicts_with: Vec<String>,   // Cannot combine with these traits
    pub available: bool,               // Selectable in character creation
}

pub enum TraitCategory {
    Positive,  // Beneficial traits that cost points
    Negative,  // Drawback traits that grant points
}
```

---

## Data Files

Game data is stored in `scripts/data/` and loaded at server startup.

### Directory Structure

```
scripts/data/
  classes.json           # Class/occupation definitions
  traits.json            # Trait definitions
  race_suggestions.json  # Race names for randomization
```

### classes.json

```json
{
  "unemployed": {
    "id": "unemployed",
    "name": "Unemployed",
    "description": "No particular profession. You start with no special skills but can learn anything.",
    "starting_skills": {},
    "stat_bonuses": {},
    "available": true
  },
  "chef": {
    "id": "chef",
    "name": "Chef",
    "description": "Kitchen expertise grants knife skills and knowledge of ingredients.",
    "starting_skills": {"cooking": 2, "short_blades": 1, "foraging": 1},
    "stat_bonuses": {"con": 1, "wis": 1, "cha": 1},
    "available": true
  }
}
```

**Adding a new class:**
1. Add entry to `classes.json`
2. Set `available: true` to make it selectable
3. Server will hot-reload on file change (no restart needed)

### traits.json

```json
{
  "strong": {
    "id": "strong",
    "name": "Strong",
    "description": "Increased physical strength. +2 to STR-based checks.",
    "cost": 3,
    "category": "positive",
    "effects": {"str": 2},
    "conflicts_with": ["weak"],
    "available": true
  },
  "weak": {
    "id": "weak",
    "name": "Weak",
    "description": "Below average strength. -2 to STR-based checks.",
    "cost": -2,
    "category": "negative",
    "effects": {"str": -2},
    "conflicts_with": ["strong"],
    "available": true
  }
}
```

**Cost values:**
- Positive number = trait costs that many points (e.g., `3` means spend 3 points)
- Negative number = trait grants that many points (e.g., `-2` means gain 2 points)

**Adding a new trait:**
1. Add entry to `traits.json`
2. Set `available: true` to make it selectable
3. Define `conflicts_with` to prevent incompatible trait combinations
4. Server will hot-reload on file change

### race_suggestions.json

Used for the "random" race option:

```json
{
  "races": [
    {"name": "Human", "description": "Versatile and adaptable."},
    {"name": "Android", "description": "Synthetic humanoid with enhanced processing."},
    {"name": "Mutant", "description": "Genetically altered with unique abilities."},
    {"name": "Clone", "description": "Artificially created duplicate."},
    {"name": "Cyborg", "description": "Human-machine hybrid."}
  ]
}
```

---

## Commands Reference

### create

Creates a new character and enters the wizard.

```
create <name> <password>
```

**Access**: Guest only (not logged in)

**Example:**
```
> create Neo mypassword
Character 'Neo' created with default values.

=== Character Creation: Neo ===
...
```

### describe

Changes your character's short description after creation.

```
describe <new description>
```

**Access**: Logged in users

**Example:**
```
> describe A tall figure in a long black coat, with mirrored sunglasses.
Description updated.
```

### traits

Opens the trait selection menu to spend remaining trait points.

```
traits
```

**Access**: Logged in users with remaining trait points

**Example:**
```
> traits
=== Trait Selection (Points: 7) ===

Your traits: Strong

Available Positive Traits (cost points):
  [1] Quick Reflexes (+1 DEX) - Costs 2 points
...
```

**Note:** You can only add new traits. Existing traits cannot be removed.

---

## Migration System

### Detection

Characters need migration if:
- `creation_complete == false` AND
- `race == ""` (empty string)

This identifies pre-wizard characters created before this system was implemented.

### Migration Flow

When a legacy character logs in:

```
=== Character Update Required ===

Welcome back, OldCharacter!

The character creation system has been updated with new options:
  - Race selection
  - Character description
  - Class/profession selection
  - Personality traits

Would you like to:
  [1] Apply defaults and continue playing
  [2] Customize your character now

Enter 1 or 2:
```

**Option 1 - Defaults:**
- Race: "Human"
- Description: "A seasoned adventurer."
- Class: "Unemployed"
- Traits: (none)
- `creation_complete` set to `true`
- Proceeds to normal login

**Option 2 - Customize:**
- Enters the character wizard menu
- Same rules apply (traits are permanent)
- After completion, proceeds to normal login

---

## Rhai Functions

### Wizard State Functions

```rhai
set_wizard_data(connection_id, json_string)  // Store wizard state
get_wizard_data(connection_id) -> String     // Retrieve wizard state
clear_wizard_data(connection_id)             // Clear wizard state
```

### Data Access Functions

```rhai
get_class_list() -> Array           // Available class IDs
get_class_info(class_id) -> Map     // Class details
get_trait_list() -> Array           // Available trait IDs
get_trait_info(trait_id) -> Map     // Trait details
get_random_race() -> String         // Random race from suggestions
get_random_short_desc() -> String   // Default description
```

### CharacterData Accessors

```rhai
// Getters
char.race                  // String
char.short_description     // String
char.class_name            // String
char.traits                // Array of Strings
char.trait_points          // Integer
char.creation_complete     // Boolean

// Setters
char.race = "Android"
char.short_description = "A chrome-plated figure."
char.class_name = "soldier"
char.traits = ["strong", "brave"]
char.trait_points = 5
char.creation_complete = true
```

---

## OLC Modes

The character creation wizard uses OLC mode to track state across multiple inputs.

| Mode | Purpose |
|------|---------|
| `chargen_menu` | Main wizard menu |
| `chargen_race` | Waiting for race input |
| `chargen_desc` | Waiting for description input |
| `chargen_class` | Class selection submenu |
| `chargen_traits` | Trait selection submenu |
| `migration_prompt` | Migration choice (1 or 2) |

---

## Future Expansion Ideas

### Race Mechanical Effects

If races need mechanical effects, add `RaceDefinition`:

```rust
pub struct RaceDefinition {
    pub id: String,
    pub name: String,
    pub description: String,
    pub stat_bonuses: HashMap<String, i32>,
    pub innate_abilities: Vec<String>,
}
```

### Level-Based Trait Unlocks

Allow earning trait points at level milestones:

```
Level 5:  +2 trait points
Level 10: +2 trait points
Level 20: +3 trait points
```

### Trait Prerequisites

Traits could require other traits or minimum stats:

```json
{
  "master_swordsman": {
    "cost": 5,
    "requires_traits": ["combat_training"],
    "requires_stats": {"str": 14, "dex": 12}
  }
}
```

### Background System

Add a background layer between race and class:

```
Race (free-form) -> Background (data-driven) -> Class (data-driven)
```

Backgrounds could grant roleplay hooks, starting equipment, or minor bonuses.

---

## File Structure

```
scripts/
  commands/
    create.rhai       # Character creation wizard
    describe.rhai     # Change description post-creation
    traits.rhai       # Spend remaining trait points
    login.rhai        # Modified for migration handling
  data/
    classes.json      # Class definitions
    traits.json       # Trait definitions
    race_suggestions.json  # Random race options

src/
  lib.rs              # CharacterData, ClassDefinition, TraitDefinition
  script.rs           # Rhai function registrations
```
