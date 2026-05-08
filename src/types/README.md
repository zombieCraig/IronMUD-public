# Types

Core data type definitions for IronMUD, organized by domain.

## Module Structure

```
src/types/
    mod.rs              # Re-exports all types for backward compatibility
    achievements.rs     # Achievement system types
    api_keys.rs         # REST API key + permissions
    area.rs             # AreaData + permissions, immigration config
    bugs.rs             # Bug-report types
    characters.rs       # CharacterData + position, fishing, command meta, party access, shop preset
    combat.rs           # Combat system types
    definitions.rs      # Class/Trait/Race/Language/Spell definitions (loaded from JSON)
    dialogue.rs         # Dialogue trees, choices, conditions, effects, pair state
    effects.rs          # DamageType, EffectType, ItemEffect, ActiveBuff (shared)
    items.rs            # ItemData, ItemType, flags, location, liquid, gold helpers
    mail_board.rs       # Player mail and bulletin-board posts
    mobiles.rs          # MobileData, MobileFlags, MobilePosition, RememberedEnemy
    property.rs         # Property templates, leases, escrow
    quests.rs           # Quest prototypes, objectives, rewards, active progress
    recipes.rs          # Recipe / crafting system types
    room.rs             # RoomData, exits, doors, flags, fishing/forage, traps
    serde_defaults.rs   # Shared serde default helpers (default_true, default_stat, ...)
    simulation.rs       # NPC needs simulation + daily routines
    social.rs           # Visual characteristics, relationships, mood, life stage
    spawn.rs            # SpawnPointData, dependencies, WearLocation
    time.rs             # Time, weather, and environment types
    transport.rs        # Transports, NPC travel routes, opposite-direction helper
    trigger.rs          # Room, item, and mobile trigger types
```

## Types by Module

| Module | Types |
|--------|-------|
| `achievements` | `AchievementCategory`, `AchievementCriterion`, `AchievementReward`, `AchievementSource`, `AchievementDef`, `AchievementUnlock` |
| `api_keys` | `ApiPermissions`, `ApiKey` |
| `area` | `AreaData`, `AreaFlags`, `AreaPermission`, `GoldRange`, `ImmigrationVariationChances`, `ImmigrationFamilyChance` |
| `bugs` | `BugStatus`, `BugPriority`, `BugContext`, `AdminNote`, `BugReport` |
| `characters` | `CharacterData`, `CharacterPosition`, `CommandMeta`, `CommandRequirements`, `FishingState`, `PartyAccessLevel`, `ShopPreset` |
| `combat` | `CombatState`, `CombatTarget`, `CombatTargetType`, `CombatZoneType`, `BodyPart`, `Wound`, `WoundLevel`, `WoundType`, `OngoingEffect`, `WeaponSkill` |
| `definitions` | `ClassDefinition`, `TraitCategory`, `TraitDefinition`, `SkillProgress`, `RaceSuggestion`, `RacialPassive`, `RacialActive`, `RaceDefinition`, `LanguageDefinition`, `SpellDefinition` |
| `dialogue` | `DialogueTree`, `DialogueNode`, `DialogueChoice`, `DialogueTarget`, `DialogueCondition`, `DialogueEffect`, `FlagScope`, `DgScope`, `DialoguePairState` |
| `effects` | `DamageType`, `EffectType`, `ItemEffect`, `ActiveBuff` |
| `items` | `ItemType`, `ItemFlags`, `ItemLocation`, `ItemData`, `LiquidType`, `CastOnUse`, gold helpers |
| `mail_board` | `MailMessage`, `BoardPost` |
| `mobiles` | `MobileData`, `MobileFlags`, `MobilePosition`, `RememberedEnemy` |
| `property` | `PropertyTemplate`, `LeaseData`, `EscrowData` |
| `quests` | `QuestData`, `QuestObjective`, `QuestReward`, `ActiveQuest` |
| `recipes` | `Recipe`, `RecipeIngredient`, `RecipeTool`, `ToolLocation` |
| `room` | `RoomData`, `RoomExits`, `DoorState`, `RoomFlags`, `WaterType`, `CatchEntry`, `ForageEntry`, `ExtraDesc`, `DepartureRecord`, `BloodTrail`, `RoomTrap` |
| `simulation` | `ActivityState`, `RoutineEntry`, `find_active_entry`, `SimGoal`, `NeedsState`, `SimulationConfig` |
| `social` | `Characteristics`, `RelationshipKind`, `Relationship`, `MoodState`, `LifeStage`, `BereavementNote`, `SocialState`, `TOPIC_FATIGUE_WINDOW`, `GAME_DAYS_PER_YEAR`, `life_stage_for_age`, `age_label_for_stage` |
| `spawn` | `SpawnEntityType`, `SpawnDestination`, `SpawnDependency`, `SpawnPointData`, `WearLocation` |
| `time` | `GameTime`, `Season`, `TimeOfDay`, `WeatherCondition`, `TemperatureCategory`, time constants |
| `transport` | `TransportType`, `TransportState`, `TransportSchedule`, `TransportStop`, `TransportData`, `NPCTravelSchedule`, `TransportRoute`, `get_opposite_direction` |
| `trigger` | `TriggerType`, `RoomTrigger`, `ItemTriggerType`, `ItemTrigger`, `MobileTriggerType`, `MobileTrigger` |

`mod.rs` itself is now just `mod`/`pub use` declarations — no types live there.

## Usage

All types are re-exported from `mod.rs`, so consumers can import them directly:

```rust
use crate::types::*;
// or
use crate::{CharacterData, RoomData, ItemData};
```

The submodules can also be used directly if preferred:

```rust
use crate::types::combat::{CombatState, Wound};
use crate::types::time::{GameTime, Season};
```
