# Gardening System

A plant growth system where players find or buy seeds, plant them in dirt floor rooms (ground) or pots (anywhere), water and tend them over game-days, and harvest food, herbs, or flowers. Growth happens via a background tick using elapsed-time calculation, so plants grow while offline.

**Timing reference:** 2 real minutes = 1 game hour. Plants survive approximately 3-4 real hours without water if fully watered before logging off.

## Player Commands

| Command | Usage | Description |
|---------|-------|-------------|
| `plant` | `plant <seed> [in <pot>]` | Plant a seed in the ground or a pot |
| `water` | `water <plant> [with <item>]` | Water a plant |
| `fertilize` | `fertilize <plant>` | Apply fertilizer to a plant |
| `planttreat` | `planttreat <plant>` | Treat a plant infestation |
| `harvest` | `harvest <plant>` | Harvest a mature (flowering) plant |
| `garden` | `garden` | View status of all plants in the room |
| `uproot` | `uproot <plant>` | Remove a planted plant |

## Builder Commands

| Command | Usage | Description |
|---------|-------|-------------|
| `plantedit` | `plantedit <vnum>` | Edit an existing plant prototype |
| `plantedit create` | `plantedit create <vnum>` | Create a new plant prototype |
| `plantedit list` | `plantedit list` | List all plant prototypes |

## Plant Prototype Editor (plantedit)

### Basic Properties

| Subcommand | Usage | Description |
|------------|-------|-------------|
| `name` | `plantedit <vnum> name <name>` | Set plant name |
| `seed_vnum` | `plantedit <vnum> seed_vnum <item_vnum>` | Set seed item vnum |
| `harvest_vnum` | `plantedit <vnum> harvest_vnum <item_vnum>` | Set harvest produce item vnum |
| `category` | `plantedit <vnum> category <type>` | Set category (vegetable, herb, flower, fruit, grain) |
| `harvest_min` | `plantedit <vnum> harvest_min <n>` | Minimum harvest yield |
| `harvest_max` | `plantedit <vnum> harvest_max <n>` | Maximum harvest yield |
| `skill` | `plantedit <vnum> skill <n>` | Minimum gardening skill to plant (0-10) |
| `xp` | `plantedit <vnum> xp <n>` | Base XP awarded on harvest |
| `pest_resistance` | `plantedit <vnum> pest_resistance <n>` | Pest resistance (0-100) |
| `multi_harvest` | `plantedit <vnum> multi_harvest <on/off>` | Whether plant resets to Growing after harvest |
| `indoor_only` | `plantedit <vnum> indoor_only <on/off>` | Whether plant can only grow indoors |
| `water` | `plantedit <vnum> water <consumption> <capacity>` | Water consumption/hr and max capacity |
| `show` | `plantedit <vnum> show` | Display all prototype properties |
| `delete` | `plantedit <vnum> delete` | Delete prototype |

### Seasons

| Subcommand | Usage | Description |
|------------|-------|-------------|
| `season add` | `plantedit <vnum> season add <season>` | Add preferred season |
| `season remove` | `plantedit <vnum> season remove <season>` | Remove preferred season |

Valid seasons: `spring`, `summer`, `autumn`, `winter`

### Growth Stages

| Subcommand | Usage | Description |
|------------|-------|-------------|
| `stage add` | `plantedit <vnum> stage add <stage> <hours> <desc>` | Add a growth stage |
| `stage remove` | `plantedit <vnum> stage remove <index>` | Remove a growth stage by index |

### Keywords

| Subcommand | Usage | Description |
|------------|-------|-------------|
| `keyword add` | `plantedit <vnum> keyword add <word>` | Add a keyword |
| `keyword remove` | `plantedit <vnum> keyword remove <word>` | Remove a keyword |

## Growth Stages

Plants progress through these stages:

| Stage | Description |
|-------|-------------|
| Seed | Underground, not visible in room |
| Sprout | First visible growth |
| Seedling | Small plant establishing roots |
| Growing | Active growth phase |
| Mature | Full-sized plant |
| Flowering | Ready to harvest |
| Wilting | Unharvested or neglected, declining |
| Dead | No longer viable, will not grow |

Each stage has a configurable duration in game hours. Flowering plants that go unharvested will eventually wilt and die.

## Seasonal Effects

| Condition | Effect |
|-----------|--------|
| Preferred season | Growth speed x1.25 |
| Forbidden season | Growth completely stopped |
| Neutral season | Normal growth rate |

If a plant prototype has no preferred seasons configured, it grows normally in all seasons.

## Weather Effects

| Condition | Effect |
|-----------|--------|
| Light Rain | +3 water/hr to outdoor plants |
| Rain | +6 water/hr to outdoor plants |
| Heavy Rain | +10 water/hr to outdoor plants |
| Freezing temperature | Frost damage to outdoor plants |

Indoor plants and potted plants in indoor rooms are protected from weather effects.

## Infestation System

| Type | Description |
|------|-------------|
| Aphids | Common insect infestation |
| Blight | Fungal disease |
| Root Rot | Waterlogging damage |
| Frost | Cold weather damage |

Infestations occur randomly (~1% per game day), modified by plant health and pest resistance. Severity grows over time if untreated. Treat with `planttreat` using appropriate pest treatment items.

## Potted Plants

Plants can be placed in pots (items with the `plant_pot` flag) to allow planting anywhere, not just in `dirt_floor` rooms. Potted plants behave like ground plants but are protected from some weather effects when indoors.

## Item Setup Guide

### Seeds
Create a `misc` type item with the `plant_prototype_vnum` field set to the plant prototype vnum:

```
oedit garden:tomato_seeds
oedit garden:tomato_seeds plantproto plants:tomato
```

### Watering Containers
Any `liquid_container` item with liquid can water plants. Water is the most efficient liquid; others have varying effects:

- **Water**: 100% watering efficiency
- **Beneficial** (healing_potion, tea, juice, milk): 75% efficiency
- **Neutral** (ale, wine, beer, coffee): 50% efficiency
- **Harmful** (alcohol, poison, oil, blood): No water, damages plant

```
oedit garden:watering_can type liquid_container
oedit garden:watering_can liquid water 10 10
```

### Fertilizer
Create a `misc` type item with the `fertilizer_duration` field set (game hours of effect):

```
oedit garden:fertilizer
oedit garden:fertilizer fertduration 48
oedit garden:fertilizer category fertilizer
```

### Pest Treatment
Create a `misc` type item with the `treats_infestation` field set to the infestation type (or "all"):

```
oedit garden:bug_spray
oedit garden:bug_spray treats aphids
oedit garden:bug_spray category pest_treatment
```

Valid treatment types: `aphids`, `blight`, `root_rot`, `frost`, `all`

### Plant Pots
Create a `misc` type item with the `plant_pot` flag:

```
oedit garden:clay_pot
oedit garden:clay_pot flag plant_pot
```

### Room Setup
Set the `dirt_floor` flag on rooms where ground planting is allowed:

```
redit flag dirt_floor
```

Optionally set the `garden` flag for thematic room display.

## Skill Progression

Skill name: `gardening`

| Level | Unlocks | Bonus |
|-------|---------|-------|
| 0 | Basic vegetables (tomato, potato) | Base yield |
| 1-2 | Wheat, roses, basic herbs, lavender | +5-10% yield |
| 3-4 | Moderate infestation treatment, firemint | +10-15% yield |
| 5-6 | Universal treatment, multi-harvest, advanced flowers | +15-20% yield |
| 7-8 | Rare species (starfruit), weather resistance | +20-25% yield |
| 9-10 | Mastery, immune to mild infestations | +25-30% yield |

XP sources: planting (5), watering (2), fertilizing (3), treating (10 x severity), harvesting (base_xp x yield_ratio).

## Related Documentation

- [Items](items.md) - Item editor for seeds, tools, pots
- [Rooms](rooms.md) - Room flags (dirt_floor, garden)
- [Areas](areas.md) - Area editor for spawn points
