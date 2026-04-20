# Gardening System - Builder Reference

## Overview

The gardening system lets players plant seeds, tend plants over game time, and harvest produce. Plants grow via a background tick using elapsed-time calculation, so they grow while offline. Growth is affected by water, fertilizer, seasons, weather, and infestations.

**Timing:** 1 game hour = 2 real minutes. A fully-watered plant survives ~3-4 real hours without water.

## Plant Prototype Editor (plantedit)

Plant prototypes define species templates. Use the `plantedit` command in-game or the MCP `create_plant_prototype` / `update_plant_prototype` tools.

### Key Properties

| Property | Description | Default |
|----------|-------------|---------|
| `name` | Plant display name | Required |
| `vnum` | Unique identifier (e.g., `plants:tomato`) | Required |
| `seed_vnum` | Item vnum of the seed | - |
| `harvest_vnum` | Item vnum of the produce | - |
| `category` | vegetable, herb, flower, fruit, grain | vegetable |
| `harvest_min` / `harvest_max` | Yield range | 1 / 3 |
| `min_skill_to_plant` | Gardening skill level required (0-10) | 0 |
| `base_xp` | XP awarded on harvest | 10 |
| `pest_resistance` | Resistance to infestations (0-100) | 30 |
| `multi_harvest` | Resets to Growing after harvest | false |
| `indoor_only` | Can only grow in pots indoors | false |

### Water Settings

| Property | Description | Default |
|----------|-------------|---------|
| `water_consumption_per_hour` | Water drained per game hour | 1.0 |
| `water_capacity` | Maximum water level | 100.0 |

### Growth Stages

Each prototype defines stages with durations. Plants progress: Seed → Sprout → Seedling → Growing → Mature → Flowering → (harvest or Wilting → Dead).

```
stages: [
  { stage: "seed", duration_game_hours: 12, description: "...", examine_desc: "..." },
  { stage: "sprout", duration_game_hours: 24, description: "...", examine_desc: "..." },
  ...
]
```

Stage names: `seed`, `sprout`, `seedling`, `growing`, `mature`, `flowering`, `wilting`, `dead`

### Seasons

- `preferred_seasons`: Growth boosted x1.25 (e.g., `["spring", "summer"]`)
- `forbidden_seasons`: Growth blocked entirely
- Valid values: `spring`, `summer`, `autumn`, `winter`
- No preferred seasons = grows normally in all seasons

## Required Item Setup

### Seeds (item type: misc)

Seeds need `plant_prototype_vnum` set to the plant prototype vnum:
```
create_item: name="Tomato Seeds", vnum="garden:tomato_seeds", item_type="misc"
```
Then via in-game OLC: `oedit garden:tomato_seeds plantproto plants:tomato`

### Watering Containers (item type: liquid_container)

Any `liquid_container` with liquid can water plants. Water is the most efficient; other liquids vary:
- **Water**: 100% efficiency
- **Beneficial** (healing_potion, tea, juice, milk): 75% efficiency
- **Neutral** (ale, wine, beer, coffee): 50% efficiency
- **Harmful** (alcohol, poison, oil, blood): No water, damages plant

```
create_item: name="Watering Can", vnum="garden:watering_can", item_type="liquid_container"
```
Then: `oedit garden:watering_can liquid water 10 10`

### Fertilizer (item type: misc)

Set `fertilizer_duration` (game hours of effect):
```
create_item: name="Fertilizer", vnum="garden:fertilizer", item_type="misc"
```
Then: `oedit garden:fertilizer fertduration 48`

### Pest Treatment (item type: misc)

Set `treats_infestation` to the type it treats:
```
create_item: name="Bug Spray", vnum="garden:bug_spray", item_type="misc"
```
Then: `oedit garden:bug_spray treats aphids`

Treatment types: `aphids`, `blight`, `root_rot`, `frost`, `all`

### Plant Pots (item type: misc)

Set the `plant_pot` item flag:
```
create_item: name="Clay Pot", vnum="garden:clay_pot", item_type="misc"
```
Then: `oedit garden:clay_pot flag plant_pot`

## Room Setup

- Set `dirt_floor` flag on rooms where ground planting is allowed
- Set `garden` flag for thematic room display (optional)
- Indoor rooms (`indoors` flag) protect potted plants from weather

## Building Workflow

1. Create a plant prototype via `create_plant_prototype` MCP tool or `plantedit create` in-game
2. Create seed item with `plant_prototype_vnum` linking to the prototype
3. Create harvest produce item (food, herb, flower, etc.)
4. Create gardening tools (watering container, fertilizer, pest treatment, pots)
5. Set up `dirt_floor` rooms where outdoor planting is allowed
6. Create spawn points for seeds and tools in appropriate shops/areas
7. Optionally create a shopkeeper that sells gardening supplies

## Skill Progression

| Level | Unlocks |
|-------|---------|
| 0 | Basic vegetables (tomato, potato) |
| 1-2 | Wheat, roses, basic herbs, lavender |
| 3-4 | Moderate pest treatment, firemint |
| 5-6 | Universal treatment, multi-harvest plants |
| 7-8 | Rare species (starfruit), weather resistance |
| 9-10 | Mastery, immune to mild infestations |

## Infestation Types

| Type | Description |
|------|-------------|
| `aphids` | Common insect infestation |
| `blight` | Fungal disease |
| `root_rot` | Waterlogging damage |
| `frost` | Cold weather damage |

Infestations occur ~1% per game day, modified by health and pest_resistance. Severity grows if untreated.

## Weather Effects (Outdoor Plants Only)

| Condition | Effect |
|-----------|--------|
| Light Rain | +3 water/hr |
| Rain | +6 water/hr |
| Heavy Rain | +10 water/hr |
| Freezing | Frost damage |

Indoor plants and potted plants in indoor rooms are protected from weather.
