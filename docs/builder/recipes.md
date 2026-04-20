# Recipe Editing

This guide covers creating and editing crafting recipes using IronMUD's Online Creation (OLC) system.

## Recipe Commands

| Command | Usage | Description |
|---------|-------|-------------|
| `recedit create` | `recedit create <vnum>` | Create a new recipe |
| `recedit` | `recedit <vnum> [subcommand]` | Edit recipe properties |
| `reclist` | `reclist [cooking\|crafting]` | List recipes (optionally filter) |
| `recfind` | `recfind <keyword>` | Search recipes |
| `recdelete` | `recdelete <vnum> [confirm]` | Delete a recipe |

## Creating Recipes

```
> recedit create food:honey_bread
Created recipe with vnum: food:honey_bread

=== Recipe Editor ===
VNUM: food:honey_bread
Name: New Recipe
Skill: cooking
Level Required: 0
...
```

## recedit Subcommands

### Basic Properties

| Subcommand | Usage | Description |
|------------|-------|-------------|
| `show` | `recedit <vnum>` | Display recipe properties |
| `name` | `recedit <vnum> name <text>` | Set recipe display name |
| `vnum` | `recedit <vnum> vnum <new_vnum>` | Change the recipe vnum |
| `skill` | `recedit <vnum> skill <cooking\|crafting>` | Set skill type |
| `level` | `recedit <vnum> level <0-10>` | Set skill level required |
| `autolearn` | `recedit <vnum> autolearn [on\|off]` | Auto-learn at skill level |
| `difficulty` | `recedit <vnum> difficulty <1-10>` | Set crafting difficulty |
| `xp` | `recedit <vnum> xp <amount>` | Set base XP awarded |
| `output` | `recedit <vnum> output <item_vnum> [qty]` | Set output item |

### Ingredients

| Subcommand | Usage | Description |
|------------|-------|-------------|
| `ingredient list` | `recedit <vnum> ingredient list` | List ingredients |
| `ingredient add` | `recedit <vnum> ingredient add <spec> <qty>` | Add ingredient |
| `ingredient remove` | `recedit <vnum> ingredient remove <index>` | Remove ingredient |

### Tools

| Subcommand | Usage | Description |
|------------|-------|-------------|
| `tool list` | `recedit <vnum> tool list` | List required tools |
| `tool add` | `recedit <vnum> tool add <spec> <location>` | Add tool requirement |
| `tool remove` | `recedit <vnum> tool remove <index>` | Remove tool |

## Ingredient/Tool Syntax

Items can be specified two ways:

| Syntax | Description | Example |
|--------|-------------|---------|
| `material:flour` | Exact vnum match | Requires specific item |
| `@flour` | Category match | Any item with a matching category |

The `@` prefix is **recipe syntax only** — when setting categories on items via `oedit`, use the bare name (no `@`).

```
> recedit bread ingredient add @flour 2
Added ingredient: 2x category 'flour'

> recedit bread ingredient add material:honey 1
Added ingredient: 1x vnum 'material:honey' (Honey)
```

## Item Categories

Items can belong to **multiple categories**, allowing flexible recipe matching. For example, a bamboo pole could have categories `bamboo` and `stick`, matching any recipe that requires either.

### Managing categories (oedit)

```
> oedit material:bamboo category add bamboo
Category 'bamboo' added. Categories: bamboo

> oedit material:bamboo category add stick
Category 'stick' added. Categories: bamboo, stick

> oedit material:bamboo category
Categories: bamboo, stick

> oedit material:bamboo category remove stick
Category 'stick' removed. Categories: bamboo

> oedit material:bamboo category clear
All categories cleared.

> oedit material:bamboo category list-all
All categories in use (5):
  bamboo, flour, meat, stick, wood
```

A recipe ingredient using `@stick` will match any item that has `stick` as one of its categories.

## Tool Locations

| Location | Description |
|----------|-------------|
| `inv` or `inventory` | Tool must be in player's inventory |
| `room` | Tool must be in the current room |
| `either` | Tool can be in inventory or room |

```
> recedit bread tool add @oven room
Added tool: category 'oven' [room]
```

## Complete Example

Creating a Honey Bread recipe:

```
> recedit create food:honey_bread
Created recipe with vnum: food:honey_bread

> recedit food:honey_bread name Honey Bread
Name set to: Honey Bread

> recedit food:honey_bread skill cooking
Skill set to: cooking

> recedit food:honey_bread level 2
Skill level requirement set to: 2

> recedit food:honey_bread autolearn on
Auto-learn: ON (learned automatically at skill level)

> recedit food:honey_bread difficulty 3
Difficulty set to: 3

> recedit food:honey_bread xp 20
Base XP set to: 20

> recedit food:honey_bread output food:bread_basic 1
Output set to: food:bread_basic (Basic Bread)

> recedit food:honey_bread ingredient add @flour 2
Added ingredient: 2x category 'flour'

> recedit food:honey_bread ingredient add material:honey 1
Added ingredient: 1x vnum 'material:honey' (Honey)

> recedit food:honey_bread tool add @oven room
Added tool: category 'oven' [room]
```

Viewing the finished recipe:

```
> recedit food:honey_bread
=== Recipe Editor ===
VNUM: food:honey_bread
Name: Honey Bread
Skill: cooking
Level Required: 2
Auto-Learn: ON (learned at skill level)
Difficulty: 3
Base XP: 20

Output: food:bread_basic (Basic Bread)

Ingredients:
  0: 2x @flour (category)
  1: 1x material:honey (Honey)

Tools:
  0: @oven (category) [room]
```

## Recipe Properties

### Skill Types

| Skill | Description |
|-------|-------------|
| `cooking` | Food preparation |
| `crafting` | Item creation |

### Difficulty

Difficulty affects the quality of crafted items:
- Lower difficulty = higher success rate
- Higher difficulty = better rewards on success

### Auto-Learn

When enabled, players automatically learn the recipe when they reach the required skill level.

## Listing Recipes

```
> reclist
=== All Recipes ===
[food:honey_bread] Honey Bread (cooking 2)
[food:stew] Hearty Stew (cooking 3)
[craft:sword] Iron Sword (crafting 4)

> reclist cooking
=== Cooking Recipes ===
[food:honey_bread] Honey Bread (level 2)
[food:stew] Hearty Stew (level 3)
```

## Searching Recipes

```
> recfind bread
Found 2 recipes:
[food:honey_bread] Honey Bread
[food:bread_basic] Basic Bread

> recfind @flour
Found 3 recipes using '@flour':
[food:honey_bread] Honey Bread
[food:bread_basic] Basic Bread
[food:cake] Simple Cake
```

## Deleting Recipes

```
> recdelete food:honey_bread
Are you sure you want to delete 'Honey Bread'? Use: recdelete food:honey_bread confirm

> recdelete food:honey_bread confirm
Recipe 'Honey Bread' deleted.
```

## Related Documentation

- [Items](items.md) - Creating output items and ingredients
- [Builder Guide](../builder-guide.md) - Overview of building
