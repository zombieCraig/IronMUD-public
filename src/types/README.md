# Types

Core data type definitions for IronMUD, organized by domain.

## Module Structure

```
src/types/
    mod.rs          # Re-exports all types for backward compatibility
    combat.rs       # Combat system types
    time.rs         # Time, weather, and environment types
    trigger.rs      # Room, item, and mobile trigger types
```

## Types by Module

| Module | Types |
|--------|-------|
| `combat` | `CombatState`, `CombatTarget`, `CombatTargetType`, `CombatZoneType`, `BodyPart`, `Wound`, `WoundLevel`, `WoundType`, `OngoingEffect`, `WeaponSkill` |
| `time` | `GameTime`, `Season`, `TimeOfDay`, `WeatherCondition`, `TemperatureCategory`, time constants |
| `trigger` | `TriggerType`, `RoomTrigger`, `ItemTriggerType`, `ItemTrigger`, `MobileTriggerType`, `MobileTrigger` |

## Types in mod.rs (to be extracted)

| Category | Types |
|----------|-------|
| Character | `CharacterData`, `CharacterPosition`, `ClassDefinition`, `TraitDefinition`, `TraitCategory`, `SkillProgress` |
| Room | `RoomData`, `RoomExits`, `RoomFlags`, `DoorState`, `ExtraDesc`, `WaterType`, `CatchEntry`, `ForageEntry` |
| Item | `ItemData`, `ItemType`, `ItemFlags`, `ItemEffect`, `ItemLocation`, `WearLocation`, `LiquidType`, `DamageType`, `EffectType` |
| Mobile | `MobileData`, `MobileFlags` |
| Area | `AreaData`, `AreaFlags`, `AreaPermission`, `SpawnPointData`, `SpawnEntityType`, `SpawnDestination`, `SpawnDependency` |
| Transport | `TransportData`, `TransportType`, `TransportState`, `TransportSchedule`, `TransportStop`, `TransportRoute`, `NPCTravelSchedule` |
| Recipe | `Recipe`, `RecipeIngredient`, `RecipeTool`, `ToolLocation` |
| Property | `PropertyTemplate`, `LeaseData`, `EscrowData` |
| Session | `CommandMeta`, `FishingState`, `OnlinePlayer`, `PartyAccessLevel`, `ShopPreset` |
| API | `ApiPermissions`, `ApiKey` |

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
