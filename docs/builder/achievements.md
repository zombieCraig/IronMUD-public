# Achievement Editor (achedit)

The achievement system rewards players for reaching milestones. Achievements can grant titles, gold, and items.

## Achievement Definitions

Achievements are identified by a unique snake_case **key** (e.g., `slayer_of_goblins`).

### Core Fields
- **Name**: The display name shown in lists and unlock banners.
- **Description**: Evocative text describing the feat.
- **Category**: Broad classification (`skill`, `combat`, `crafting`, `exploration`, `social`, `wealth`, `builder`).
- **Hidden**: If set to `on`, the achievement is invisible in the player's list until unlocked.

## Criteria

Every achievement has exactly one criterion that triggers the unlock.

| Criterion | Command | Description |
|-----------|---------|-------------|
| **Manual** | `manual` | Awarded only by scripts (`award_achievement`) or admins. |
| **Counter** | `counter <key> <n>` | Unlocks when a character counter reaches threshold `<n>`. |
| **Skill** | `skill <key> <n>` | Unlocks when a skill reaches level `<n>`. |
| **Recipe** | `recipe <vnum>` | Unlocks when a recipe is learned. |
| **Lease** | `lease [vnum]` | Unlocks when a property lease is purchased (optionally in a specific area). |
| **Gold** | `gold <amount>` | Unlocks when a player's gold high-water mark reaches `<amount>`. |

### Counter Keys
Common counter keys include:
- `kills.any`: Total mobile kills.
- `kills.<mob_vnum>`: Kills of a specific mobile type.

## Rewards

- **Title**: A string granted to the player (e.g., `the Brave`). Players can set their active title via the `achievements` command.
- **Gold**: Instant gold delivered upon unlock.
- **Item**: (Wired in Slice 3) An item delivered to the player's inventory or escrow.
- **Morality delta**: Shifts the player's morality slider on unlock. Positive values push toward Good, negative toward Evil; clamped into `[-200, 200]`. Useful for narrative achievements that reward virtuous or villainous deeds. When the shift crosses a tier boundary (±25, ±50, ±75, ±100), the player sees the corresponding "feel" message; sub-tier nudges are silent.

## Usage Examples

### Creating a Combat Achievement
```
achedit create orc_slayer Orc Slayer
achedit orc_slayer desc You have proven your mettle against the orcish hordes.
achedit orc_slayer category combat
achedit orc_slayer criterion counter kills.orc 50
achedit orc_slayer reward title the Orc-Bane
achedit orc_slayer reward gold 100
```

### Creating a Skill Milestone
```
achedit create master_chef Master Chef
achedit master_chef desc Your culinary skills are the talk of the town.
achedit master_chef category skill
achedit master_chef criterion skill cooking 10
achedit master_chef reward title the Gourmet
```

### Creating a Morality-Shifting Achievement
```
achedit create paragon_of_virtue Paragon of Virtue
achedit paragon_of_virtue desc Your selfless deeds inspire those around you.
achedit paragon_of_virtue category social
achedit paragon_of_virtue criterion manual
achedit paragon_of_virtue reward title the Virtuous
achedit paragon_of_virtue reward morality 25

achedit create bloodstained Bloodstained
achedit bloodstained desc You have crossed a line that cannot be uncrossed.
achedit bloodstained category social
achedit bloodstained criterion manual
achedit bloodstained reward title the Bloodstained
achedit bloodstained reward morality -30
```

### Listing and Showing
- `achedit list`: Shows all achievement definitions and their source (JSON or Database).
- `achedit <key>`: Shows full details of a specific definition.
