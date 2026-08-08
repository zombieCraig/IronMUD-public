# Achievement Editor (achedit)

The achievement system rewards players for reaching milestones. Achievements can grant titles, gold, items, morality shifts, and trait points.

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

This is the complete list of counters the engine writes. A Counter achievement
naming anything else will never unlock — `tests/achievements.rs::test_seed_counters_are_written_by_the_engine`
fails the build if a shipped definition waits on a counter nothing bumps.

| Key | Counts |
|---|---|
| `kills.any` | Every credited mobile kill. |
| `kills.<mob_vnum>` | Kills of one specific mobile prototype. Generated per vnum at kill time, so any vnum works. |
| `deaths` | The character's own deaths, from every route (bleedout, a hit while unconscious, synth shutdown). |
| `skills_maxed` | Skills taken to level 10. |
| `recipes.learned` | Recipes learned, by any route. |
| `recipes.discovered` | Recipes worked out with `experiment` rather than taught. Fires alongside `recipes.learned`, never instead of it. |
| `items.crafted` | Successful `craft` completions. |
| `meals.cooked` | Successful `cook` completions. |
| `fish.landed` | Fish landed with `reel`. |
| `plants.harvested` | Plants taken with `harvest`. |
| `spells.cast` | Successful `cast` completions. |
| `gold.spent` | Gold paid out — shops, rent, `identify`, mail postage, upgrades. Counts the amount, not the transaction. |
| `gold.earned` | Gold taken in, from any credit. Counts the amount, not the transaction. |
| `quests.completed` | Quests completed. |
| `mail.sent` | Letters sent. |
| `board.posts` | Bulletin board posts written. |
| `leases.bought` | Property leases purchased. |
| `npcs.befriended` | Times the character crossed *up* into Accepted with a faction. Counts the act, not the state — a falling-out and reconciliation counts twice. |
| `items.sold` | Items sold through a consignment broker. Counts sales, not gold. |
| `items.bought` | Items bought from another player through a consignment broker. |

Three counters track a *set* rather than a tally, so repeating the same thing
does not advance them. They are also reconciled against the stored set on every
bump, which means characters who did the thing before the counter existed get
credited retroactively:

| Key | Counts |
|---|---|
| `rooms.visited` | Distinct rooms entered. |
| `socials.distinct` | Distinct socials performed. |
| `npcs.talked_to` | Distinct NPC *prototypes* talked to (not instances). |

### Counters are also leaderboards

Every counter above is a `top` board, and so is any counter you add — the scan
in `src/leaderboard.rs` discovers them from the character data rather than
from a list, so a new counter starts ranking the moment something bumps it.
Two consequences worth knowing when you name one:

- The key is what players type (`top gold.earned`), and an unlabelled key is
  title-cased into its own board name (`spanners.thrown` → *Spanners Thrown*).
  Add a row to `COUNTER_META` if you want a better title or a specific group.
- `kills.<vnum>` keys are grouped separately from the rest of Combat, because
  a busy world generates one per mobile prototype and they would otherwise
  bury the handful of boards worth browsing.

Adding a counter of your own means adding a call to
`notify_achievement_counter(player_name, "<key>", n)` in a Rhai command script,
or `crate::script::achievements::notify_counter_core` in Rust — place it *after*
any wholesale character save in the surrounding code, or that save will
overwrite the bump.

## Rewards

- **Title**: A string granted to the player (e.g., `the Brave`). Players can set their active title via the `achievements` command.
- **Gold**: Instant gold delivered upon unlock.
- **Item**: (Wired in Slice 3) An item delivered to the player's inventory or escrow.
- **Morality delta**: Shifts the player's morality slider on unlock. Positive values push toward Good, negative toward Evil; clamped into `[-200, 200]`. Useful for narrative achievements that reward virtuous or villainous deeds. When the shift crosses a tier boundary (±25, ±50, ±75, ±100), the player sees the corresponding "feel" message; sub-tier nudges are silent.
- **Trait points**: Spendable build currency, added to the pool the `traits` command draws on. Must be zero or positive — `achedit` refuses a negative value, and a hand-edited JSON file carrying one is ignored at unlock rather than deducting.

### Granting trait points

Trait points are the only reward that changes what a character can *become*, rather than what they hold. Two conventions keep them meaningful:

1. **Top of a ladder only.** Grant them on the deepest rung of a long progression (1000 kills, 250 crafted items, 100 quests), never on an early or mid-tier achievement.
2. **Not for individual skill masteries.** The `skills_maxed` counter ladder already pays for mastery breadth; granting points for *both* a single skill hitting 10 and the `skills_maxed` tier it feeds double-pays the same effort.

A character starts with 10 points and traits cost 1–5, so the shipped seed grants total 22 across 13 capstones — roughly one extra build over a full career. `tests/achievements.rs::test_seed_trait_point_grants_stay_scarce_and_earned` enforces both conventions and the total budget.

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

### Creating a Capstone That Grants Trait Points
```
achedit create dragon_reckoning Dragon's Reckoning
achedit dragon_reckoning desc You have broken the wyrms of the northern reach.
achedit dragon_reckoning category combat
achedit dragon_reckoning criterion counter kills.dragon 25
achedit dragon_reckoning reward title the Wyrmbane
achedit dragon_reckoning reward traitpoints 2
```

### Listing and Showing
- `achedit list`: Shows all achievement definitions and their source (JSON or Database).
- `achedit <key>`: Shows full details of a specific definition.
